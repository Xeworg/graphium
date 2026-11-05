//! The `render_diagram` tool (PRD §9.1, §11, S21).
//!
//! Every render goes through the single engine door
//! ([`gx_core::engine::render_json`]): resolve the kind, validate against its
//! schema, build the IR, lay out and render. There is no second pipeline here.
//!
//! The finished SVG is handed to the window through the render hook that
//! `gx-app` installs (PRD §8.6.1 step 3): the hook emits
//! `graphium://render { req_id, diagram_id, svg, place }` and answers whether
//! the frontend confirmed the paint after two animation frames. Without a hook
//! the diagram is rendered and stored but not painted.

use std::collections::VecDeque;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::render::{
    Degradation, Extract, RenderMetrics, RenderOpts, DEFAULT_LAYOUT_TIMEOUT_MS,
    MAX_LAYOUT_TIMEOUT_MS,
};
use serde::Serialize;

use crate::session::{self, SessionView};
use crate::store::diagram_store;

/// A request to render one diagram document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderRequest {
    /// The diagram document, already serialized as JSON.
    pub document: String,
    /// The MCP session that owns the diagram (PRD §11.1).
    pub session_id: String,
}

/// The options one render accepts (PRD §7.3, S21).
///
/// `render_diagram` uses the defaults; `render_diagram_with` takes what the
/// agent asked for in `opts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    /// Draws only the neighbourhood of one node instead of the whole graph.
    pub extract: Option<Extract>,
    /// Puts the camera on one node and, optionally, its neighbourhood
    /// (PRD §7, RF27).
    pub focus: Option<Focus>,
    /// Replaces the document of this diagram instead of creating another one
    /// (PRD §7, S5 of `f2-cierre.md`).
    pub replace_id: Option<String>,
    /// Deadline of the whole pipeline, in milliseconds.
    pub layout_timeout_ms: u64,
}

/// Where the camera goes after a render (PRD §7, RF27).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Focus {
    /// `id` of the node to center.
    pub around: String,
    /// Hops of the neighbourhood to frame with it: `0` is the node alone.
    pub depth: usize,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            extract: None,
            focus: None,
            replace_id: None,
            layout_timeout_ms: DEFAULT_LAYOUT_TIMEOUT_MS,
        }
    }
}

/// The response of `render_diagram` (PRD §11, S21).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_kind: Option<String>,
    pub rendered: bool,
    /// Why the diagram was not painted: `headless`, `no_window`,
    /// `ui_timeout` or `ui_error` (PRD §8.6.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Metrics>,
    /// The parent diagram and the depth of the subgraph that was painted:
    /// `<diagram_id>@<depth>` (PRD §7.3, S21).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extracted_from: Option<String>,
    /// The diagram whose document this render replaced, when the agent sent
    /// `opts.replace_id` (PRD §7 RF1 caption; feature doc `f2-cierre.md`, S5).
    /// Without it the render sums to the plan and the field is absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced: Option<String>,
    /// How many renders were waiting when the queue refused this one
    /// (PRD §7.3, S22).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_depth: Option<usize>,
    /// How long the agent should wait before rendering again, in
    /// milliseconds (PRD §7.3, S22).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorPayload>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
    /// The session of the caller: the name the human sees in the window
    /// (S3, D2). [`render_diagram_with`] stamps it on every answer, so no
    /// render path can travel without it; the field is additive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionView>,
}

/// Cost of a render, reported to the agent (PRD §7.3, RF20-bis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Metrics {
    pub nodes: u64,
    pub edges: u64,
    pub layout_ms: u64,
    pub render_ms: u64,
    pub svg_bytes: u64,
    /// Wall time of a render that did not finish: only `layout_timeout`
    /// pays for it (PRD §7.3, S21).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
}

impl From<RenderMetrics> for Metrics {
    fn from(metrics: RenderMetrics) -> Self {
        Self {
            nodes: metrics.nodes as u64,
            edges: metrics.edges as u64,
            layout_ms: metrics.layout_ms,
            render_ms: metrics.render_ms,
            svg_bytes: metrics.svg_bytes as u64,
            elapsed_ms: None,
        }
    }
}

/// A validation problem with its stable code and JSON path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorPayload {
    pub path: String,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl From<&ValidationError> for ErrorPayload {
    fn from(error: &ValidationError) -> Self {
        Self {
            path: error.path.clone(),
            code: error.code.as_str().to_string(),
            message: error.message.clone(),
            hint: error.hint.clone(),
        }
    }
}

/// A non-fatal note about a render.
///
/// One type for the whole crate: the render and the dry run report warnings
/// with the same shape and the same vocabulary (PRD §7 RF20).
pub use crate::contract::Warning;

/// Everything `gx-app` needs to paint a diagram (PRD §8.6.1 step 3).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderHookPayload {
    pub diagram_id: String,
    pub kind: String,
    pub svg: String,
    /// Width of the diagram in plan units, from the layout bounding box.
    pub width: f64,
    /// Height of the diagram in plan units, from the layout bounding box.
    pub height: f64,
    /// The box the camera has to show, in SVG coordinates, when the agent
    /// asked for `opts.focus` (PRD §7, RF27).
    pub focus: Option<gx_core::geom::Rect>,
    pub metrics: Metrics,
}

/// The outcome of the paint in the window (PRD §8.6.1).
///
/// The reasons of a `rendered: false` are stable: the agent branches on
/// them, so they never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStatus {
    /// The window confirmed the paint.
    Painted,
    /// There is no display: the process works headless.
    Headless,
    /// There is no window to paint in.
    NoWindow,
    /// The window did not answer within the timeout.
    Timeout,
    /// The window refused the render or failed.
    Error,
}

impl RenderStatus {
    /// The stable reason of the status, `None` when the diagram was painted.
    pub fn reason(&self) -> Option<&'static str> {
        match self {
            RenderStatus::Painted => None,
            RenderStatus::Headless => Some("headless"),
            RenderStatus::NoWindow => Some("no_window"),
            RenderStatus::Timeout => Some("ui_timeout"),
            RenderStatus::Error => Some("ui_error"),
        }
    }

    /// Whether the diagram reached the screen.
    pub fn painted(&self) -> bool {
        matches!(self, RenderStatus::Painted)
    }
}

/// The callback that paints a diagram in the window.
///
/// The status is what the window knows: only it can tell a timeout from a
/// refusal or from the absence of a display.
pub type RenderHook = Arc<dyn Fn(&RenderHookPayload) -> RenderStatus + Send + Sync>;

fn render_hook_slot() -> &'static RwLock<Option<RenderHook>> {
    static SLOT: OnceLock<RwLock<Option<RenderHook>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Installs (or removes) the hook that paints the diagram in the window.
///
/// `gx-app` owns the webview and the round-trip; `None` means headless, and
/// every render reports `rendered: false`.
pub fn set_render_hook(hook: Option<RenderHook>) {
    *render_hook_slot()
        .write()
        .expect("the render hook slot is not poisoned") = hook;
}

/// The installed hook, if any.
fn installed_hook() -> Option<RenderHook> {
    render_hook_slot()
        .read()
        .expect("the render hook slot is not poisoned")
        .clone()
}

/// How many renders wait for a slot before the queue refuses more
/// (PRD §7.3, S22 / RNF8).
pub const QUEUE_DEPTH: usize = 8;
/// How many renders lay out at the same time (PRD §7.3, S22 / RNF8).
pub const MAX_PARALLEL_RENDERS: usize = 2;
/// What the `busy` answer tells the agent to wait before trying again.
///
/// The PRD asks for `retry_after_ms` but fixes no number: a render of the
/// warning line takes tens of milliseconds, so a tenth of a second is long
/// enough to be useful and short enough not to be a stall.
pub const BUSY_RETRY_AFTER_MS: u64 = 100;

/// The FIFO queue of renders of this process (PRD §7.3, S22).
///
/// Two renders lay out at a time; the rest wait in arrival order, up to
/// [`QUEUE_DEPTH`]. The [`MAX_PARALLEL_RENDERS`] runs and the waiting room
/// are one state, so the agent gets an honest `busy` instead of a timeout.
#[derive(Debug, Default)]
pub struct RenderQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

#[derive(Debug, Default)]
struct QueueState {
    running: usize,
    waiting: VecDeque<u64>,
    next_ticket: u64,
}

/// What the queue says to a render that just arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// There was a free slot: run now.
    Admitted,
    /// The render is in the queue, at that position.
    Queued(u64),
    /// The waiting room is full: come back later.
    Full { depth: usize },
}

impl RenderQueue {
    fn lock(&self) -> std::sync::MutexGuard<'_, QueueState> {
        self.state.lock().expect("the render queue is not poisoned")
    }

    /// Whether a slot is free and nobody is waiting ahead.
    ///
    /// The waiting room is FIFO: an arrival never overtakes a ticket, not
    /// even when a slot is free, or the queue would starve its own front.
    pub fn try_acquire(&self) -> Admission {
        let mut state = self.lock();
        if state.running < MAX_PARALLEL_RENDERS && state.waiting.is_empty() {
            state.running += 1;
            return Admission::Admitted;
        }
        if state.waiting.len() >= QUEUE_DEPTH {
            return Admission::Full {
                depth: state.waiting.len(),
            };
        }
        let ticket = state.next_ticket;
        state.next_ticket += 1;
        state.waiting.push_back(ticket);
        Admission::Queued(ticket)
    }

    /// Takes the slot for `ticket` if it is the head of the queue and a slot
    /// is free. Returns whether the render may run now.
    pub fn poll(&self, ticket: u64) -> bool {
        let mut state = self.lock();
        if state.running >= MAX_PARALLEL_RENDERS || state.waiting.front() != Some(&ticket) {
            return false;
        }
        state.waiting.pop_front();
        state.running += 1;
        true
    }

    /// Waits, in queue order, until the render may run.
    pub fn wait_for(&self, ticket: u64) {
        let mut state = self.lock();
        loop {
            if state.running < MAX_PARALLEL_RENDERS && state.waiting.front() == Some(&ticket) {
                state.waiting.pop_front();
                state.running += 1;
                return;
            }
            state = self
                .ready
                .wait(state)
                .expect("the render queue is not poisoned");
        }
    }

    /// How many renders are running and how many are waiting.
    pub fn counters(&self) -> (usize, usize) {
        let state = self.lock();
        (state.running, state.waiting.len())
    }

    /// Frees a slot and wakes the queue.
    pub fn release(&self) {
        {
            let mut state = self.lock();
            state.running = state.running.saturating_sub(1);
        }
        self.ready.notify_all();
    }
}

/// The queue of this process.
pub fn render_queue() -> &'static RenderQueue {
    static QUEUE: OnceLock<RenderQueue> = OnceLock::new();
    QUEUE.get_or_init(RenderQueue::default)
}

/// Holds a slot of the queue and frees it when it leaves the scope, also on
/// the early returns of a rejected render.
struct RenderSlot;

impl Drop for RenderSlot {
    fn drop(&mut self) {
        render_queue().release();
    }
}

/// Takes a slot, or returns how deep the waiting room was when it refused.
fn enter_queue() -> Result<RenderSlot, usize> {
    let queue = render_queue();
    match queue.try_acquire() {
        Admission::Admitted => Ok(RenderSlot),
        Admission::Queued(ticket) => {
            queue.wait_for(ticket);
            Ok(RenderSlot)
        }
        Admission::Full { depth } => Err(depth),
    }
}

/// The answer of a full queue (PRD §7.3, S22).
fn busy(depth: usize) -> RenderResponse {
    RenderResponse {
        ok: false,
        diagram_id: None,
        rendered_kind: None,
        rendered: false,
        reason: None,
        metrics: None,
        extracted_from: None,
        queue_depth: Some(depth),
        retry_after_ms: Some(BUSY_RETRY_AFTER_MS),
        replaced: None,
        errors: vec![ErrorPayload {
            path: "diagram".to_string(),
            code: "busy".to_string(),
            message: format!(
                "la cola de renders está llena: {depth} renders esperando y {MAX_PARALLEL_RENDERS} en curso"
            ),
            hint: Some(format!(
                "esperá {BUSY_RETRY_AFTER_MS} ms y volvé a intentar, o mandá menos diagramas a la vez"
            )),
        }],
        warnings: Vec::new(),
        session: None,
    }
}

/// Renders a diagram document through the single engine door
/// ([`gx_core::engine::render_json`]) and stores it in the diagram store.
///
/// The door is the only path from JSON to SVG: it resolves the kind through
/// the registry, validates against the kind schema, builds the IR, measures
/// the text, lays out and renders. On success the diagram is stored and then
/// handed to the render hook; `rendered` is whatever the hook answered.
pub fn render_diagram(request: &RenderRequest) -> RenderResponse {
    render_diagram_with(request, &RenderOptions::default())
}

/// Renders with the options the agent asked for (PRD §7.3, S21).
///
/// The pipeline runs on a worker thread and the caller waits for it with the
/// deadline of `opts.layout_timeout_ms`, clamped to
/// `1..=MAX_LAYOUT_TIMEOUT_MS`. When the deadline beats the layout the answer
/// is `layout_timeout` with the elapsed time and nothing is stored, so the
/// plane stays untouched. The detached worker may finish later: its result is
/// dropped.
pub fn render_diagram_with(request: &RenderRequest, opts: &RenderOptions) -> RenderResponse {
    with_session(render_inner(request, opts), &request.session_id)
}

/// Stamps the session of the caller on the answer (S3, D2).
///
/// Every render leaves through here, so the agent learns with which name the
/// human sees it even when the render failed (`busy`, `layout_timeout`,
/// `too_large`); the field is additive and no other field of the answer moves.
fn with_session(mut response: RenderResponse, session_id: &str) -> RenderResponse {
    response.session = Some(session::current_view(session_id));
    response
}

/// Checks the diagram `opts.replace_id` names before the layout runs (S5, R2).
///
/// Replacing is an update of that diagram, so the errors are the ones of
/// `update_diagram` — `unknown_diagram`, `not_owner` — with the path of the
/// option that named the id: the agent fixes `opts.replace_id`, not a
/// `diagram_id` it never sent. Nothing is created on either error. The summary
/// of the target comes back so the kind can be checked after the render
/// without asking the store twice.
fn resolve_replacement(
    replace_id: Option<&str>,
    session_id: &str,
) -> Result<Option<crate::store::DiagramSummary>, ErrorPayload> {
    let Some(replace_id) = replace_id else {
        return Ok(None);
    };
    let store = diagram_store();
    let Some(record) = store.record(replace_id) else {
        return Err(contract_payload(
            crate::contract::unknown_diagram(replace_id, &store.known_ids()),
            "opts.replace_id",
        ));
    };
    if record.summary.session_id != session_id {
        return Err(contract_payload(
            crate::contract::not_owner(replace_id),
            "opts.replace_id",
        ));
    }
    Ok(Some(record.summary))
}

/// The answer of a replacement whose document is another kind.
///
/// An existing diagram keeps its kind (PRD §11.1): replacing is an update plus
/// the render, and `list_diagrams` would otherwise name a kind the document no
/// longer has.
fn kind_change_not_allowed(replace_id: &str, kind: &str, current: &str) -> RenderResponse {
    RenderResponse {
        ok: false,
        diagram_id: None,
        rendered_kind: None,
        rendered: false,
        reason: None,
        metrics: None,
        extracted_from: None,
        queue_depth: None,
        retry_after_ms: None,
        replaced: None,
        errors: vec![ErrorPayload {
            path: "kind".to_string(),
            code: ErrorCode::KindChangeNotAllowed.as_str().to_string(),
            message: format!(
                "el diagrama `{replace_id}` es `{current}` y el documento nuevo es `{kind}`: el `kind` de un diagrama no se puede cambiar"
            ),
            hint: Some(
                "creá otro diagrama con `render_diagram` sin `opts.replace_id`".to_string(),
            ),
        }],
        warnings: Vec::new(),
        session: None,
    }
}

/// Joins a lifecycle payload of `contract` with the response type of the
/// tools, under the path of the field that failed.
///
/// The wording of `unknown_diagram` and `not_owner` (with `known_ids[]`) is
/// owned by `contract`; this is the one place where the two copies of the
/// type meet, so the messages cannot drift.
pub(crate) fn contract_payload(payload: crate::contract::ErrorPayload, path: &str) -> ErrorPayload {
    ErrorPayload {
        path: path.to_string(),
        code: payload.code,
        message: payload.message,
        hint: payload.hint,
    }
}

/// The render itself, with the answer the caller asked for.
///
/// [`render_diagram_with`] is the one door out: it stamps the session on
/// whatever this answers, so no return of this pipeline can skip it.
fn render_inner(request: &RenderRequest, opts: &RenderOptions) -> RenderResponse {
    crate::bootstrap();

    // The queue is taken before anything else: a refused render must not
    // touch the plan, the store nor the clock of the deadline.
    let _slot = match enter_queue() {
        Ok(slot) => slot,
        Err(depth) => return busy(depth),
    };

    // `opts.replace_id` names the diagram whose document this render replaces
    // (PRD §7, S5). The target is resolved before the layout: an id that is
    // not on the plan is answered without paying a layout, and a replacement
    // never creates a diagram, so it does not need a free slot either.
    let replacing = match resolve_replacement(opts.replace_id.as_deref(), &request.session_id) {
        Ok(target) => target,
        Err(payload) => {
            return RenderResponse {
                ok: false,
                diagram_id: None,
                rendered_kind: None,
                rendered: false,
                reason: None,
                metrics: None,
                extracted_from: None,
                queue_depth: None,
                retry_after_ms: None,
                replaced: None,
                errors: vec![payload],
                warnings: Vec::new(),
                session: None,
            }
        }
    };

    let deadline_ms = opts
        .layout_timeout_ms
        .clamp(MIN_LAYOUT_TIMEOUT_MS, MAX_LAYOUT_TIMEOUT_MS);
    let started = Instant::now();
    let (sender, receiver) = mpsc::channel();
    let document = request.document.clone();
    let worker_opts = opts.clone();
    let worker = std::thread::Builder::new()
        .name("gx-mcp-render".to_string())
        .spawn(move || {
            // A closed channel means the caller already answered: what a slow
            // render produces past its deadline is discarded, never stored.
            let _ = sender.send(pipeline(&document, &worker_opts));
        });

    let (outcome, applied) = match worker {
        // Without a thread the deadline cannot be enforced, so the pipeline
        // runs here instead of failing a valid render.
        Err(_) => match pipeline(&request.document, opts) {
            Ok(painted) => painted,
            Err(errors) => return rejected(errors),
        },
        Ok(_) => match receiver.recv_timeout(Duration::from_millis(deadline_ms)) {
            Ok(Ok(painted)) => painted,
            Ok(Err(errors)) => return rejected(errors),
            Err(RecvTimeoutError::Timeout) => {
                return layout_timeout(deadline_ms, started.elapsed());
            }
            Err(RecvTimeoutError::Disconnected) => {
                return rejected(vec![ValidationError::without_hint(
                    "diagram",
                    ErrorCode::InvalidValue,
                    "el render terminó sin resultado",
                )]);
            }
        },
    };

    // Step 4: even the last step could not enter the budget. The render
    // cuts and says how to continue instead of painting two megabytes.
    if outcome.metrics.svg_bytes > SVG_BUDGET_BYTES {
        let metrics = Metrics::from(outcome.metrics);
        return RenderResponse {
            ok: false,
            diagram_id: None,
            rendered_kind: None,
            rendered: false,
            reason: None,
            metrics: Some(metrics),
            extracted_from: None,
            queue_depth: None,
            retry_after_ms: None,
            replaced: None,
            errors: vec![ErrorPayload {
                path: "diagram".to_string(),
                code: "too_large".to_string(),
                message: format!(
                    "el SVG resultante tiene {} bytes y el presupuesto es {}",
                    metrics.svg_bytes, SVG_BUDGET_BYTES
                ),
                hint: Some("usá opts.extract: {around, depth}".to_string()),
            }],
            warnings: Vec::new(),
            session: None,
        };
    }

    // The document is valid JSON from here on: the door just accepted it.
    let document: serde_json::Value =
        serde_json::from_str(&request.document).unwrap_or(serde_json::Value::Null);
    let kind = document
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let title = document
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("Untitled")
        .to_string();

    let metrics = Metrics::from(outcome.metrics);
    // An existing diagram keeps its kind (PRD §11.1): replacing is an update
    // plus the render, so `list_diagrams` cannot end up naming a kind the
    // document no longer has. Nothing is stored when they disagree.
    if let Some(target) = &replacing {
        if target.kind != kind {
            return kind_change_not_allowed(&target.diagram_id, &kind, &target.kind);
        }
    }
    // The size of what was just painted is reported, never hidden: the same
    // warning the dry run gives, in the same words (PRD §7 RF20, S6).
    let mut warnings = match gx_core::kind::for_kind(&kind) {
        Some(registered) => crate::contract::size_warnings(
            outcome.metrics.nodes,
            outcome.metrics.edges,
            registered.limits(),
        ),
        None => Vec::new(),
    };
    if !applied.is_empty() {
        warnings.push(Warning::new(
            "degraded",
            format!(
                "el diagrama se degradó para entrar en el presupuesto: {}",
                applied
                    .iter()
                    .map(|step| match step {
                        1 => "paso 1 (labels de aristas omitidos)",
                        2 => "paso 2 (espaciado al 70%)",
                        _ => "paso 3 (texto sin kerning fino)",
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    // Bounding box of the layout: the size of the diagram in plan units.
    let width = outcome.bbox.w;
    let height = outcome.bbox.h;

    // The camera intent of `opts.focus` is computed before anything is stored:
    // an unknown id has to leave the plane exactly as it was.
    let focus = match opts.focus.as_ref() {
        Some(focus) => match focus_box(&outcome.layout, focus) {
            Ok(rect) => Some(rect),
            Err(error) => return rejected(vec![error]),
        },
        None => None,
    };

    let store = diagram_store();
    // The document is kept normalized: `get_diagram` returns exactly this.
    let document = serde_json::to_string(
        &serde_json::from_str::<serde_json::Value>(&request.document)
            .unwrap_or(serde_json::Value::Null),
    )
    .unwrap_or_else(|_| request.document.clone());
    let diagram_id = match opts.replace_id.as_deref() {
        // The same cycle as `update_diagram` followed by a re-render: the
        // document and the title move to the stored diagram, the revision
        // grows and the id stays. Nothing is inserted, so the plane does not
        // grow.
        Some(replace_id) => match store.replace(replace_id, title, document) {
            Some(summary) => summary.diagram_id,
            None => {
                // The diagram was closed between the check and here.
                return RenderResponse {
                    ok: false,
                    diagram_id: None,
                    rendered_kind: None,
                    rendered: false,
                    reason: None,
                    metrics: Some(metrics),
                    extracted_from: None,
                    queue_depth: None,
                    retry_after_ms: None,
                    replaced: None,
                    errors: vec![contract_payload(
                        crate::contract::unknown_diagram(replace_id, &store.known_ids()),
                        "opts.replace_id",
                    )],
                    warnings: Vec::new(),
                    session: None,
                };
            }
        },
        None => {
            // The plan holds a dozen diagrams (PRD §11.1, RF13). A render that
            // would not fit is refused **before** anything is stored or
            // painted, so the plane stays exactly as the human left it.
            if store.is_full() {
                return RenderResponse {
                    ok: false,
                    diagram_id: None,
                    rendered_kind: None,
                    rendered: false,
                    reason: None,
                    metrics: Some(metrics),
                    extracted_from: None,
                    queue_depth: None,
                    retry_after_ms: None,
                    replaced: None,
                    errors: vec![ErrorPayload {
                        path: "diagram".to_string(),
                        code: "too_many_diagrams".to_string(),
                        message: format!(
                            "el plano ya tiene {} diagramas abiertos y el tope es {}",
                            store.open_count(),
                            crate::store::MAX_OPEN_DIAGRAMS
                        ),
                        hint: Some(
                            "cerrá un diagrama desde el panel lateral y volvé a intentar"
                                .to_string(),
                        ),
                    }],
                    warnings: Vec::new(),
                    session: None,
                };
            }
            let diagram_id = store.next_id();
            store.insert(
                diagram_id.clone(),
                title,
                kind.clone(),
                request.session_id.clone(),
                document,
            );
            diagram_id
        }
    };

    let status = match installed_hook() {
        Some(hook) => hook(&RenderHookPayload {
            diagram_id: diagram_id.clone(),
            kind: kind.clone(),
            svg: outcome.svg,
            width,
            height,
            focus,
            metrics,
        }),
        None => {
            warnings.push(Warning::new(
                "no_window",
                "the diagram was rendered and stored but not painted: no window hook is installed",
            ));
            RenderStatus::NoWindow
        }
    };
    if !status.painted() {
        if let Some(reason) = status.reason() {
            warnings.push(Warning::new(
                reason,
                format!("the diagram was rendered and stored but not painted: {reason}"),
            ));
        }
    }

    let extracted_from = opts
        .extract
        .as_ref()
        .map(|extract| format!("{diagram_id}@{}", extract.depth));

    RenderResponse {
        ok: true,
        diagram_id: Some(diagram_id),
        rendered_kind: Some(kind),
        rendered: status.painted(),
        reason: status.reason().map(str::to_string),
        metrics: Some(metrics),
        extracted_from,
        queue_depth: None,
        retry_after_ms: None,
        replaced: opts.replace_id.clone(),
        errors: Vec::new(),
        warnings,
        session: None,
    }
}

/// The number of nodes above which a render is warned and degraded
/// (PRD §7.3, S6 and S20).
pub const WARNING_NODES: usize = 150;
/// The budget of bytes of a painted SVG: past it the render cuts
/// (PRD §7.3, S20 step 4).
pub const SVG_BUDGET_BYTES: usize = 2 * 1024 * 1024;
/// The shortest deadline accepted: `0` would never let a layout finish.
pub const MIN_LAYOUT_TIMEOUT_MS: u64 = 1;

/// Runs the whole pipeline: the door, then the degradation ladder.
///
/// The ladder (PRD §7.3, S20) is walked here and not inside the door: past
/// the warning line the diagram is not rejected, it is degraded. Step 1
/// always applies above the line; the later steps are only paid when the SVG
/// is still past the budget, so a big but healthy diagram degrades once and
/// no more. The steps applied come back with the outcome.
#[allow(clippy::type_complexity)]
fn pipeline(
    document: &str,
    opts: &RenderOptions,
) -> Result<(gx_core::render::RenderOutcome, Vec<u8>), Vec<ValidationError>> {
    let mut outcome = render_with(document, opts, Degradation::NONE)?;
    let mut applied = Vec::new();
    if outcome.metrics.nodes > WARNING_NODES {
        outcome = render_with(document, opts, Degradation::step_one())?;
        applied.push(1u8);
        for (step, degradation) in [
            (2u8, Degradation::step_two()),
            (3, Degradation::step_three()),
        ] {
            if outcome.metrics.svg_bytes <= SVG_BUDGET_BYTES {
                break;
            }
            outcome = render_with(document, opts, degradation)?;
            applied.push(step);
        }
    }
    Ok((outcome, applied))
}

/// Renders a document with a given level of the degradation ladder.
fn render_with(
    document: &str,
    opts: &RenderOptions,
    degradation: Degradation,
) -> Result<gx_core::render::RenderOutcome, Vec<ValidationError>> {
    gx_core::engine::render_json(
        document,
        &RenderOpts {
            degradation,
            extract: opts.extract.clone(),
            layout_timeout_ms: opts.layout_timeout_ms,
            ..RenderOpts::default()
        },
    )
}

/// The response of a document the door rejected.
pub(crate) fn rejected(errors: Vec<ValidationError>) -> RenderResponse {
    RenderResponse {
        ok: false,
        diagram_id: None,
        rendered_kind: None,
        rendered: false,
        reason: None,
        metrics: None,
        extracted_from: None,
        queue_depth: None,
        retry_after_ms: None,
        replaced: None,
        errors: errors.iter().map(ErrorPayload::from).collect(),
        warnings: Vec::new(),
        session: None,
    }
}

/// The response of a layout that did not finish in time (PRD §7.3, S21).
///
/// Nothing was stored and nothing was painted: the plane stays untouched.
fn layout_timeout(deadline_ms: u64, elapsed: Duration) -> RenderResponse {
    let elapsed_ms = elapsed.as_millis() as u64;
    RenderResponse {
        ok: false,
        diagram_id: None,
        rendered_kind: None,
        rendered: false,
        reason: None,
        metrics: Some(Metrics {
            nodes: 0,
            edges: 0,
            layout_ms: elapsed_ms,
            render_ms: 0,
            svg_bytes: 0,
            elapsed_ms: Some(elapsed_ms),
        }),
        extracted_from: None,
        queue_depth: None,
        retry_after_ms: None,
        replaced: None,
        errors: vec![ErrorPayload {
            path: "diagram".to_string(),
            code: ErrorCode::LayoutTimeout.as_str().to_string(),
            message: format!("el layout no terminó en {deadline_ms} ms"),
            hint: Some(
                "subí opts.layout_timeout_ms o partí el diagrama con opts.extract".to_string(),
            ),
        }],
        warnings: Vec::new(),
        session: None,
    }
}

/// Depth of the subgraph when `opts.extract.depth` is absent (PRD §7.3, S21).
pub const DEFAULT_EXTRACT_DEPTH: usize = 1;

/// Reads `opts` into [`RenderOptions`] (PRD §7.3, S21).
///
/// The keys that belong to a later phase are accepted and reported in an
/// `opts_ignored` warning that names them, so the agent knows when they
/// arrive instead of guessing. A malformed value is an error with its own
/// path, never a silent default: rendering the whole diagram because `around`
/// was not a string would paint the opposite of what was asked.
pub fn parse_opts(
    raw: Option<&serde_json::Value>,
) -> Result<(RenderOptions, Vec<Warning>), Vec<gx_core::error::ValidationError>> {
    use gx_core::error::{ErrorCode, ValidationError};

    let mut render_opts = RenderOptions::default();
    let mut warnings = Vec::new();
    let Some(value) = raw else {
        return Ok((render_opts, warnings));
    };
    let Some(object) = value.as_object() else {
        return Err(vec![ValidationError::new(
            "opts",
            ErrorCode::InvalidValue,
            "`opts` tiene que ser un objeto JSON",
            Some(
                "mandá `opts: { extract: { around, depth? }, layout_timeout_ms? }` o nada"
                    .to_string(),
            ),
        )]);
    };

    let mut ignored: Vec<&str> = Vec::new();
    for (key, value) in object {
        match key.as_str() {
            "extract" => render_opts.extract = Some(parse_extract(value)?),
            "focus" => render_opts.focus = Some(parse_focus(value)?),
            "replace_id" => render_opts.replace_id = Some(parse_replace_id(value)?),
            "layout_timeout_ms" => {
                let Some(ms) = value.as_u64() else {
                    return Err(vec![ValidationError::new(
                        "opts.layout_timeout_ms",
                        ErrorCode::InvalidValue,
                        format!(
                            "`opts.layout_timeout_ms` tiene que ser un entero de milisegundos, no `{value}`"
                        ),
                        Some(format!(
                            "mandá un número entre 1 y {}",
                            gx_core::render::MAX_LAYOUT_TIMEOUT_MS
                        )),
                    )]);
                };
                render_opts.layout_timeout_ms = ms;
            }
            other => ignored.push(other),
        }
    }

    if !ignored.is_empty() {
        warnings.push(Warning {
            code: "opts_ignored".to_string(),
            message: format!(
                "`opts.{}` todavía se ignora: `opts.interactive` llega en F3; esta versión honra `opts.extract`, `opts.focus`, `opts.replace_id` y `opts.layout_timeout_ms`",
                ignored.join("`, `opts.")
            ),
        });
    }
    Ok((render_opts, warnings))
}

/// Reads `opts.replace_id` into the id of the diagram to replace (S5).
///
/// The value is the id a previous render answered. An empty or non-string
/// value is an error with its own path: falling back to "create another"
/// would leave two diagrams where the agent asked for one.
fn parse_replace_id(
    value: &serde_json::Value,
) -> Result<String, Vec<gx_core::error::ValidationError>> {
    use gx_core::error::{ErrorCode, ValidationError};

    let id = value.as_str().map(str::trim).unwrap_or_default();
    if id.is_empty() {
        return Err(vec![ValidationError::new(
            "opts.replace_id",
            ErrorCode::InvalidValue,
            format!("`opts.replace_id` tiene que ser el id de un diagrama del plano, no `{value}`"),
            Some(
                "mandá el `diagram_id` que devolvió `render_diagram` o `list_diagrams`".to_string(),
            ),
        )]);
    }
    Ok(id.to_string())
}

/// Reads `opts.focus` into a [`Focus`] (PRD §7, RF27).
fn parse_focus(value: &serde_json::Value) -> Result<Focus, Vec<gx_core::error::ValidationError>> {
    use gx_core::error::{ErrorCode, ValidationError};

    let Some(object) = value.as_object() else {
        return Err(vec![ValidationError::new(
            "opts.focus",
            ErrorCode::InvalidValue,
            "`opts.focus` tiene que ser un objeto con `id` y, opcionalmente, `depth`",
            Some("mandá `opts.focus: { id: \"<id>\", depth: 1 }`".to_string()),
        )]);
    };
    let around = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .or_else(|| object.get("around").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .unwrap_or_default();
    if around.is_empty() {
        return Err(vec![ValidationError::new(
            "opts.focus.id",
            ErrorCode::MissingField,
            "`opts.focus.id` falta: es el `id` del nodo que la cámara tiene que mostrar",
            Some("usá el `id` de un nodo de este diagrama".to_string()),
        )]);
    }
    let depth = match object.get("depth") {
        None | Some(serde_json::Value::Null) => DEFAULT_EXTRACT_DEPTH,
        Some(value) => match value.as_u64() {
            Some(depth) => usize::try_from(depth).unwrap_or(usize::MAX),
            None => {
                return Err(vec![ValidationError::new(
                    "opts.focus.depth",
                    ErrorCode::InvalidValue,
                    format!("`opts.focus.depth` tiene que ser un entero >= 0, no `{value}`"),
                    Some(
                        "omití `depth` para el default 1, o mandá 0 para el nodo solo".to_string(),
                    ),
                )])
            }
        },
    };
    Ok(Focus {
        around: around.to_string(),
        depth,
    })
}

/// Reads `opts.extract` into a [`gx_core::render::Extract`] (PRD §7.3, S21).
pub fn parse_extract(
    value: &serde_json::Value,
) -> Result<gx_core::render::Extract, Vec<gx_core::error::ValidationError>> {
    use gx_core::error::{ErrorCode, ValidationError};

    let Some(object) = value.as_object() else {
        return Err(vec![ValidationError::new(
            "opts.extract",
            ErrorCode::InvalidValue,
            "`opts.extract` tiene que ser un objeto con `around` y, opcionalmente, `depth`",
            Some("mandá `opts.extract: { around: \"<id>\", depth: 1 }`".to_string()),
        )]);
    };
    let around = object
        .get("around")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if around.is_empty() {
        return Err(vec![ValidationError::new(
            "opts.extract.around",
            ErrorCode::MissingField,
            "`opts.extract.around` falta: es el `id` del nodo alrededor del cual se dibuja",
            Some("usá el `id` de un nodo de este diagrama".to_string()),
        )]);
    }
    let depth = match object.get("depth") {
        None | Some(serde_json::Value::Null) => DEFAULT_EXTRACT_DEPTH,
        Some(value) => match value.as_u64() {
            Some(depth) => usize::try_from(depth).unwrap_or(usize::MAX),
            None => {
                return Err(vec![ValidationError::new(
                    "opts.extract.depth",
                    ErrorCode::InvalidValue,
                    format!("`opts.extract.depth` tiene que ser un entero >= 0, no `{value}`"),
                    Some("omití `depth` para el default 1 o mandá un entero".to_string()),
                )])
            }
        },
    };
    Ok(gx_core::render::Extract {
        around: around.to_string(),
        depth,
    })
}

/// The box the camera has to show for `opts.focus` (PRD §7, RF27).
///
/// It walks the routed layout in both directions up to `depth` hops and
/// returns the union of the boxes it reaches. An unknown id is an error with
/// its own path: pointing the camera at nothing would leave the human
/// looking at an empty spot.
fn focus_box(
    layout: &gx_core::layout::LayoutGraph,
    focus: &Focus,
) -> Result<gx_core::geom::Rect, ValidationError> {
    use std::collections::{BTreeSet, VecDeque};

    if !layout.nodes.contains_key(&focus.around) {
        return Err(ValidationError::new(
            "opts.focus.around",
            ErrorCode::UnknownRef,
            format!("no hay ningún nodo `{}` en el diagrama", focus.around),
            Some("usá el `id` de un nodo de este diagrama".to_string()),
        ));
    }

    let mut reached: BTreeSet<&str> = BTreeSet::new();
    reached.insert(focus.around.as_str());
    let mut queue: VecDeque<(&str, usize)> = VecDeque::new();
    queue.push_back((focus.around.as_str(), 0));
    while let Some((id, hops)) = queue.pop_front() {
        if hops >= focus.depth {
            continue;
        }
        for edge in &layout.edges {
            let next = if edge.from == id {
                edge.to.as_str()
            } else if edge.to == id {
                edge.from.as_str()
            } else {
                continue;
            };
            if reached.insert(next) {
                queue.push_back((next, hops + 1));
            }
        }
    }

    let mut union: Option<gx_core::geom::Rect> = None;
    for id in reached {
        let Some(placed) = layout.nodes.get(id) else {
            continue;
        };
        let rect = gx_core::geom::Rect::new(
            placed.center.x - placed.size.w / 2.0,
            placed.center.y - placed.size.h / 2.0,
            placed.size.w,
            placed.size.h,
        );
        union = Some(match union {
            Some(current) => current.union(&rect),
            None => rect,
        });
    }
    union.ok_or_else(|| {
        ValidationError::without_hint(
            "opts.focus.around",
            ErrorCode::UnknownRef,
            "no hay ninguna caja que mostrar para `opts.focus`",
        )
    })
}

/// Renders a document with the raw `opts` the agent sent (PRD §7.3, S21).
///
/// One implementation for every entry point: the stdio server and the socket
/// relay both come through here, so `opts.extract` and
/// `opts.layout_timeout_ms` cannot work in one and be silently ignored in the
/// other.
pub fn render_diagram_from_json(
    request: &RenderRequest,
    raw_opts: Option<&serde_json::Value>,
) -> RenderResponse {
    let (opts, warnings) = match parse_opts(raw_opts) {
        Ok(parsed) => parsed,
        // A rejected `opts` leaves no diagram, but it still answers with the
        // session: the field travels on every path out (S3, D2).
        Err(errors) => return with_session(rejected(errors), &request.session_id),
    };
    let mut response = render_diagram_with(request, &opts);
    response.warnings.extend(warnings);
    response
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{
        busy, render_diagram, render_diagram_from_json, render_diagram_with, render_queue,
        set_render_hook, Admission, RenderHook, RenderOptions, RenderQueue, RenderRequest,
        RenderResponse, RenderStatus, BUSY_RETRY_AFTER_MS, MAX_PARALLEL_RENDERS, QUEUE_DEPTH,
        SVG_BUDGET_BYTES,
    };
    use crate::store::diagram_store;
    use crate::test_support::{lock_renders_blocking, MINIMAL};
    use gx_core::render::Extract;

    fn render(document: &str) -> RenderResponse {
        render_diagram(&RenderRequest {
            document: document.to_string(),
            session_id: "session-test".to_string(),
        })
    }

    fn render_with_opts(document: &str, opts: RenderOptions) -> RenderResponse {
        render_diagram_with(
            &RenderRequest {
                document: document.to_string(),
                session_id: "session-test".to_string(),
            },
            &opts,
        )
    }

    /// Renders with the raw `opts` of the MCP contract, the way the stdio
    /// server and the relay do: the only door that sees `opts.replace_id`.
    fn render_from_json(document: &str, opts: serde_json::Value) -> RenderResponse {
        render_from_json_in("session-test", document, opts)
    }

    fn render_from_json_in(
        session_id: &str,
        document: &str,
        opts: serde_json::Value,
    ) -> RenderResponse {
        render_diagram_from_json(
            &RenderRequest {
                document: document.to_string(),
                session_id: session_id.to_string(),
            },
            Some(&opts),
        )
    }

    fn focus(around: &str, depth: usize) -> RenderOptions {
        RenderOptions {
            focus: Some(super::Focus {
                around: around.to_string(),
                depth,
            }),
            ..RenderOptions::default()
        }
    }

    fn extract(around: &str, depth: usize) -> RenderOptions {
        RenderOptions {
            extract: Some(Extract {
                around: around.to_string(),
                depth,
            }),
            ..RenderOptions::default()
        }
    }

    fn error_codes(response: &RenderResponse) -> Vec<String> {
        response
            .errors
            .iter()
            .map(|error| error.code.clone())
            .collect()
    }

    fn warning_codes(response: &RenderResponse) -> Vec<String> {
        response
            .warnings
            .iter()
            .map(|warning| warning.code.clone())
            .collect()
    }

    /// A chain of `nodes` nodes: wide enough to test the size ladder.
    fn chain(nodes: usize) -> String {
        let nodes: Vec<String> = (0..nodes)
            .map(|i| format!(r#"{{ "id": "n{i}", "label": "nodo {i}" }}"#))
            .collect();
        let edges: Vec<String> = (0..nodes.len().saturating_sub(1))
            .map(|i| {
                format!(
                    r#"{{ "from": "n{i}", "to": "n{}", "label": "sigue" }}"#,
                    i + 1
                )
            })
            .collect();
        format!(
            r#"{{ "version": 1, "kind": "graph", "title": "cadena",
                 "nodes": [{}], "edges": [{}] }}"#,
            nodes.join(", "),
            edges.join(", ")
        )
    }

    /// PRD §7.3, S22 / RNF8: two renders lay out at a time and eight wait in
    /// arrival order. The ninth is refused instead of quietly piling up.
    #[test]
    fn the_queue_runs_two_and_holds_eight_in_order() {
        let queue = RenderQueue::default();

        assert_eq!(MAX_PARALLEL_RENDERS, 2);
        assert_eq!(QUEUE_DEPTH, 8);
        assert_eq!(queue.try_acquire(), Admission::Admitted);
        assert_eq!(queue.try_acquire(), Admission::Admitted);

        let mut tickets = Vec::new();
        for _ in 0..QUEUE_DEPTH {
            match queue.try_acquire() {
                Admission::Queued(ticket) => tickets.push(ticket),
                other => panic!("the room is not full yet: {other:?}"),
            }
        }
        assert_eq!(queue.try_acquire(), Admission::Full { depth: QUEUE_DEPTH });

        // FIFO: the second ticket never overtakes the first.
        queue.release();
        assert!(
            !queue.poll(tickets[1]),
            "the queue has to serve the head first"
        );
        assert!(queue.poll(tickets[0]));
        queue.release();
        assert!(queue.poll(tickets[1]));
        // Both tickets hold a slot now; the other six wait their turn.
        assert_eq!(queue.counters(), (2, 6));
    }

    #[test]
    fn the_busy_answer_names_the_queue_and_the_retry() {
        let response = busy(QUEUE_DEPTH);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "busy");
        assert_eq!(response.queue_depth, Some(QUEUE_DEPTH));
        assert_eq!(response.retry_after_ms, Some(BUSY_RETRY_AFTER_MS));
        let json = serde_json::to_value(&response).expect("the response serializes");
        assert_eq!(json["queue_depth"], 8);
        assert_eq!(json["retry_after_ms"], BUSY_RETRY_AFTER_MS);
    }

    /// A render that finishes leaves its slot free: the queue is a gate, not
    /// a leak.
    #[test]
    fn a_finished_render_leaves_the_queue_empty() {
        let _guard = lock_renders_blocking();

        let response = render(MINIMAL);

        assert!(response.ok, "{:?}", response.errors);
        assert_eq!(render_queue().counters(), (0, 0));
    }

    /// PRD §11.1 (S9): en F2 un render **suma** al plano. Hasta F1 el
    /// frontend reemplazaba el objeto anterior; el modelo ya no pierde el
    /// primero cuando llega el segundo.
    #[test]
    fn a_render_sums_to_the_plane_instead_of_replacing() {
        let _guard = lock_renders_blocking();

        let first = render(MINIMAL);
        let second = render(MINIMAL);

        assert!(first.ok && second.ok, "{first:?} {second:?}");
        assert_ne!(first.diagram_id, second.diagram_id);
        let listed = diagram_store().list();
        assert_eq!(listed.len(), 2, "los dos siguen en el plano: {listed:?}");
        assert_eq!(diagram_store().open_count(), 2);
        // Sin `opts.replace_id` no hay diagrama reemplazado: el render suma.
        assert!(first.replaced.is_none());
        assert!(second.replaced.is_none());
        let json = serde_json::to_value(&first).expect("the response serializes");
        assert!(json.get("replaced").is_none(), "{json}");
    }

    /// S5/R2 (f2-cierre): `opts.replace_id` reemplaza el documento del
    /// diagrama nombrado en vez de acumular otro. Es el ciclo de un `update`
    /// seguido de un re-render: la revisión avanza, el id no cambia y la
    /// respuesta reporta el diagrama afectado.
    #[test]
    fn replace_id_replaces_the_named_diagram_instead_of_creating_another() {
        let _guard = lock_renders_blocking();

        let first = render(MINIMAL);
        let diagram_id = first.diagram_id.expect("the render stores the diagram");

        let replacement = r#"{
          "version": 1, "kind": "graph", "title": "reemplazado",
          "nodes": [ { "id": "a", "label": "A" }, { "id": "b", "label": "B" } ],
          "edges": [ { "from": "a", "to": "b" } ]
        }"#;
        let response =
            render_from_json(replacement, serde_json::json!({ "replace_id": diagram_id }));

        assert!(response.ok, "{:?}", response.errors);
        assert_eq!(response.diagram_id.as_deref(), Some(diagram_id.as_str()));
        assert_eq!(response.replaced.as_deref(), Some(diagram_id.as_str()));
        assert!(
            response
                .warnings
                .iter()
                .all(|warning| warning.code != "opts_ignored"),
            "`opts.replace_id` se honra: {:?}",
            response.warnings
        );

        let listed = diagram_store().list();
        assert_eq!(
            listed.len(),
            1,
            "no se acumula un segundo diagrama: {listed:?}"
        );
        assert_eq!(listed[0].diagram_id, diagram_id);
        assert_eq!(listed[0].revision, 2, "el ciclo de un update + re-render");
        assert_eq!(listed[0].title, "reemplazado");
        let stored = diagram_store()
            .record(&diagram_id)
            .expect("the diagram stays on the plan")
            .document;
        let stored: serde_json::Value =
            serde_json::from_str(&stored).expect("the document is JSON");
        assert_eq!(stored["title"], "reemplazado");
        assert_eq!(stored["nodes"].as_array().expect("nodes").len(), 2);
        // Reemplazar también escribe una versión del documento (RF14): el
        // render inicial y este reemplazo.
        let versions = diagram_store()
            .versions(&diagram_id)
            .expect("the diagram stays on the plan");
        assert_eq!(versions.len(), 2, "el render y el reemplazo");
        assert_eq!(versions.last().expect("hay versiones").version, 2);

        let json = serde_json::to_value(&response).expect("the response serializes");
        assert_eq!(json["replaced"], diagram_id);
    }

    /// S5/R2: un `replace_id` que no está en el plano es `unknown_diagram` y
    /// **nada** se crea; el plano queda exactamente como estaba.
    #[test]
    fn replace_id_of_an_unknown_diagram_is_unknown_diagram_and_creates_nothing() {
        let _guard = lock_renders_blocking();

        let existing = render(MINIMAL)
            .diagram_id
            .expect("the render stores the diagram");
        let response = render_from_json(MINIMAL, serde_json::json!({ "replace_id": "d-9999" }));

        assert!(!response.ok, "{response:?}");
        assert_eq!(error_codes(&response), vec!["unknown_diagram"]);
        assert_eq!(response.errors[0].path, "opts.replace_id");
        assert!(
            response.errors[0].message.contains("d-9999"),
            "the error names the id: {:?}",
            response.errors[0]
        );
        assert!(response.errors[0].hint.is_some());
        assert_eq!(diagram_store().list().len(), 1, "nada se crea");
        assert_eq!(diagram_store().list()[0].diagram_id, existing);
        assert!(diagram_store().get("d-9999").is_none());
    }

    /// S5/R2: un `replace_id` que no es un id (número, vacío) es un valor que
    /// no encaja y no se convierte en un render nuevo por default.
    #[test]
    fn a_malformed_replace_id_is_invalid_value_and_stores_nothing() {
        let _guard = lock_renders_blocking();

        for bad in [serde_json::json!(42), serde_json::json!("   ")] {
            let response = render_from_json(MINIMAL, serde_json::json!({ "replace_id": bad }));

            assert!(!response.ok, "{response:?}");
            assert_eq!(error_codes(&response), vec!["invalid_value"]);
            assert_eq!(response.errors[0].path, "opts.replace_id");
            assert!(response.errors[0].hint.is_some());
            assert!(diagram_store().list().is_empty(), "nada se crea");
        }
    }

    /// PRD §11.1 (RF30): reemplazar el diagrama de otra sesión es `not_owner`
    /// y no cambia el documento ajeno.
    #[test]
    fn replace_id_of_another_session_is_not_owner_and_changes_nothing() {
        let _guard = lock_renders_blocking();

        let foreign = render_diagram(&RenderRequest {
            document: MINIMAL.to_string(),
            session_id: "otra-sesion".to_string(),
        })
        .diagram_id
        .expect("the render stores the diagram");

        let response = render_from_json(MINIMAL, serde_json::json!({ "replace_id": foreign }));

        assert!(!response.ok, "{response:?}");
        assert_eq!(error_codes(&response), vec!["not_owner"]);
        let record = diagram_store()
            .record(&foreign)
            .expect("el diagrama ajeno sigue en el plano");
        assert_eq!(record.summary.revision, 1, "no se re-renderizó");
        let stored: serde_json::Value =
            serde_json::from_str(&record.document).expect("the document is JSON");
        let original: serde_json::Value =
            serde_json::from_str(MINIMAL).expect("the example is JSON");
        assert_eq!(stored, original, "el documento no cambia");
    }

    /// PRD §11.1: un diagrama existente no cambia de `kind`, ni siquiera por
    /// un reemplazo completo del documento. El store y el plano quedan como
    /// estaban.
    #[test]
    fn replace_id_cannot_change_the_kind() {
        let _guard = lock_renders_blocking();

        let diagram_id = render(MINIMAL)
            .diagram_id
            .expect("the render stores the diagram");
        let er = r#"{
          "version": 1, "kind": "er", "title": "otro",
          "tables": [ { "name": "users", "columns": [ { "name": "id", "type": "uuid", "key": "pk" } ] } ]
        }"#;

        let response = render_from_json(er, serde_json::json!({ "replace_id": diagram_id }));

        assert!(!response.ok, "{response:?}");
        assert_eq!(error_codes(&response), vec!["kind_change_not_allowed"]);
        assert_eq!(response.errors[0].path, "kind");
        assert!(response.errors[0].hint.is_some());
        let record = diagram_store()
            .record(&diagram_id)
            .expect("el diagrama sigue en el plano");
        assert_eq!(record.summary.kind, "graph", "el kind no cambia");
        assert_eq!(record.summary.revision, 1, "no se re-renderizó");
        assert_eq!(
            diagram_store().versions(&diagram_id).map(|v| v.len()),
            Some(1)
        );
    }

    /// R2/RF13: reemplazar no pide un slot libre, porque no suma un diagrama;
    /// el plano lleno sigue rechazando un render que sí crearía otro.
    #[test]
    fn a_full_plan_accepts_a_replacement_and_still_refuses_a_new_diagram() {
        let _guard = lock_renders_blocking();

        for _ in 0..crate::store::MAX_OPEN_DIAGRAMS {
            assert!(render(MINIMAL).ok);
        }
        assert!(diagram_store().is_full());
        let diagram_id = diagram_store().list()[0].diagram_id.clone();

        let replaced = render_from_json(MINIMAL, serde_json::json!({ "replace_id": diagram_id }));
        assert!(replaced.ok, "reemplazar no necesita un slot: {replaced:?}");
        assert_eq!(replaced.replaced.as_deref(), Some(diagram_id.as_str()));
        assert_eq!(
            diagram_store().open_count(),
            crate::store::MAX_OPEN_DIAGRAMS,
            "el plano no crece"
        );

        let refused = render(MINIMAL);
        assert!(!refused.ok, "{refused:?}");
        assert_eq!(error_codes(&refused), vec!["too_many_diagrams"]);
    }

    /// PRD §11.1 (RF13, S7): el plano tiene tope. El decimotercero se rechaza
    /// con su código y **no** se guarda ni se pinta.
    #[test]
    fn the_plane_refuses_the_thirteenth_diagram() {
        let _guard = lock_renders_blocking();

        for _ in 0..crate::store::MAX_OPEN_DIAGRAMS {
            let response = render(MINIMAL);
            assert!(response.ok, "{:?}", response.errors);
        }
        assert!(diagram_store().is_full());

        let response = render(MINIMAL);

        assert!(!response.ok, "{response:?}");
        assert_eq!(error_codes(&response), vec!["too_many_diagrams"]);
        assert!(
            response.errors[0].hint.is_some(),
            "el error dice cómo liberar un slot"
        );
        assert_eq!(
            diagram_store().open_count(),
            crate::store::MAX_OPEN_DIAGRAMS,
            "el plano queda como estaba"
        );

        // Cerrar uno libera el slot y el siguiente render entra.
        let first = diagram_store().list()[0].diagram_id.clone();
        assert!(diagram_store().close(&first));
        let response = render(MINIMAL);
        assert!(response.ok, "{:?}", response.errors);
        assert_eq!(
            diagram_store().open_count(),
            crate::store::MAX_OPEN_DIAGRAMS
        );
    }

    /// PRD §7 (RF27): `opts.focus` pide que la cámara muestre un nodo y, si
    /// se pide, su vecindad. El payload del hook lleva la caja a mostrar.
    #[test]
    fn focus_asks_the_window_to_show_one_node() {
        let _guard = lock_renders_blocking();

        let painted = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&painted);
        set_render_hook(Some(Arc::new(move |payload| {
            recorder
                .lock()
                .expect("the recorder is not poisoned")
                .push(payload.focus);
            RenderStatus::Painted
        })));

        let graph = chain(4);
        let alone = render_with_opts(&graph, focus("n2", 0));
        let neighbourhood = render_with_opts(&graph, focus("n2", 1));

        assert!(alone.ok && neighbourhood.ok, "{alone:?} {neighbourhood:?}");
        let boxes = painted
            .lock()
            .expect("the recorder is not poisoned")
            .clone();
        let alone = boxes[0].expect("el foco del nodo solo");
        let neighbourhood = boxes[1].expect("el foco de la vecindad");
        assert!(
            neighbourhood.w >= alone.w && neighbourhood.h >= alone.h,
            "la vecindad no puede ser más chica que el nodo solo: {alone:?} vs {neighbourhood:?}"
        );

        set_render_hook(None);
    }

    /// Un `opts.focus` que apunta a un nodo que no existe deja el plano como
    /// estaba: no hay nada que mostrar y el agente tiene que enterarse.
    #[test]
    fn focus_on_an_unknown_node_stores_nothing() {
        let _guard = lock_renders_blocking();

        let response = render_with_opts(&chain(3), focus("no-existe", 1));

        assert!(!response.ok, "{response:?}");
        assert_eq!(error_codes(&response), vec!["unknown_ref"]);
        assert_eq!(response.errors[0].path, "opts.focus.around");
        assert!(diagram_store().list().is_empty());
    }

    /// PRD §7 RF20, S6: between 150 nodes and the hard limit the diagram is
    /// not rejected; the render reports the size in `warnings[]`.
    #[test]
    fn a_big_diagram_renders_with_the_size_warning() {
        let _guard = lock_renders_blocking();

        let response = render(&chain(160));

        assert!(response.ok, "{:?}", response.errors);
        assert_eq!(response.metrics.expect("metrics").nodes, 160);
        let codes = warning_codes(&response);
        assert!(
            codes.contains(&"too_many_nodes".to_string()),
            "the render has to report the size it just painted: {codes:?}"
        );
    }

    /// The hard limit is the one thing that rejects (PRD §7.3, S6): the
    /// error names the count it received and the maximum, and the plane is
    /// left untouched.
    #[test]
    fn a_diagram_over_the_hard_limit_is_rejected_and_not_stored() {
        let _guard = lock_renders_blocking();

        let response = render(&chain(301));

        assert!(!response.ok);
        assert_eq!(error_codes(&response), vec!["too_large"]);
        assert!(
            response.errors[0].message.contains("301"),
            "the error names the count it received: {:?}",
            response.errors[0]
        );
        assert!(response.errors[0].message.contains("300"));
        assert!(response.errors[0].hint.is_some());
        assert!(response.metrics.is_none());
        assert!(
            diagram_store().list().is_empty(),
            "a rejected diagram never reaches the plane"
        );
    }

    #[test]
    fn a_diagram_under_the_warning_line_has_no_size_warning() {
        let _guard = lock_renders_blocking();

        let response = render(&chain(150));

        assert!(response.ok, "{:?}", response.errors);
        let codes = warning_codes(&response);
        assert!(
            !codes.contains(&"too_many_nodes".to_string()),
            "150 nodes are exactly the line: {codes:?}"
        );
    }

    /// PRD §7.3, S20 step 1: past the warning line the edge labels go first,
    /// and the render says which step it applied.
    #[test]
    fn a_big_diagram_is_degraded_and_the_warning_names_the_step() {
        let _guard = lock_renders_blocking();

        let painted = Arc::new(Mutex::new(String::new()));
        let recorder = Arc::clone(&painted);
        set_render_hook(Some(Arc::new(move |payload| {
            *recorder.lock().expect("the recorder is not poisoned") = payload.svg.clone();
            RenderStatus::Painted
        })));

        let response = render(&chain(160));

        assert!(response.ok, "{:?}", response.errors);
        let degraded = response
            .warnings
            .iter()
            .find(|warning| warning.code == "degraded")
            .expect("the degradation is reported");
        assert!(
            degraded.message.contains("paso 1"),
            "the warning names the step: {}",
            degraded.message
        );
        let svg = painted
            .lock()
            .expect("the recorder is not poisoned")
            .clone();
        assert!(
            !svg.contains("sigue"),
            "step 1 omits the edge labels: {svg}"
        );

        set_render_hook(None);
    }

    /// A small diagram pays nothing: the ladder only starts above the
    /// warning line (PRD §7.3).
    #[test]
    fn a_small_diagram_is_not_degraded() {
        let _guard = lock_renders_blocking();

        let response = render(MINIMAL);

        let codes = warning_codes(&response);
        assert!(
            !codes.contains(&"degraded".to_string()),
            "a two-node diagram is drawn whole: {codes:?}"
        );
    }

    /// PRD §7.3, S20 step 4: when even the last step leaves the SVG past the
    /// budget, the render cuts and says how to continue instead of painting
    /// two megabytes.
    #[test]
    fn an_oversized_diagram_is_cut_with_the_budget_and_the_hint() {
        let _guard = lock_renders_blocking();
        crate::test_support::set_oversize(true);

        let response = render(&chain(160));

        assert!(!response.ok, "{response:?}");
        assert_eq!(error_codes(&response), vec!["too_large"]);
        let error = &response.errors[0];
        assert!(
            error
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("opts.extract"),
            "the hint points at extract: {error:?}"
        );
        let metrics = response.metrics.expect("the cost is reported");
        assert!(
            metrics.svg_bytes as usize > SVG_BUDGET_BYTES,
            "the response names the size: {metrics:?}"
        );
        assert!(
            diagram_store().list().is_empty(),
            "a cut diagram never reaches the plane"
        );
    }

    #[test]
    fn renders_the_minimal_example_of_the_prd() {
        let _guard = lock_renders_blocking();

        let response = render(MINIMAL);

        assert!(response.ok, "{:?}", response.errors);
        assert_eq!(response.diagram_id.as_deref(), Some("d-0001"));
        assert_eq!(response.rendered_kind.as_deref(), Some("graph"));
        assert!(response.errors.is_empty());
        let metrics = response.metrics.expect("metrics are reported");
        assert_eq!(metrics.nodes, 2);
        assert_eq!(metrics.edges, 1);
        assert!(metrics.svg_bytes > 0, "{metrics:?}");
        // No hook: rendered, stored, but not painted.
        assert!(!response.rendered);
        assert_eq!(response.warnings[0].code, "no_window");

        // The wire shape of S21: empty error and warning lists are omitted.
        let json = serde_json::to_value(&response).expect("the response serializes");
        assert_eq!(json["ok"], true);
        assert_eq!(json["diagram_id"], "d-0001");
        assert_eq!(json["rendered_kind"], "graph");
        assert_eq!(json["rendered"], false);
        assert!(json["metrics"].is_object(), "{json}");
        assert!(json["warnings"].is_array(), "{json}");
        assert!(json.get("errors").is_none(), "{json}");
    }

    #[test]
    fn ids_are_monotonic_across_renders() {
        let _guard = lock_renders_blocking();

        let first = render(MINIMAL);
        let second = render(MINIMAL);

        assert_eq!(first.diagram_id.as_deref(), Some("d-0001"));
        assert_eq!(second.diagram_id.as_deref(), Some("d-0002"));
        let listed = diagram_store().list();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].diagram_id, "d-0001");
        assert_eq!(listed[1].diagram_id, "d-0002");
        assert_eq!(listed[0].title, "parse_config() — flujo");
        assert_eq!(listed[0].session_id, "session-test");
    }

    #[test]
    fn invalid_json_is_reported() {
        let _guard = lock_renders_blocking();

        let response = render("{ \"version\": 1, ");

        assert!(!response.ok);
        assert_eq!(error_codes(&response), vec!["invalid_json"]);
        assert_eq!(response.diagram_id, None);
        assert_eq!(response.metrics, None);
        assert!(!response.rendered);
        assert!(diagram_store().list().is_empty());
    }

    /// The `er` kind draws tables with their columns and a crow's foot
    /// relation (PRD §6.1, H2).
    #[test]
    fn the_er_kind_renders_a_schema() {
        let _guard = lock_renders_blocking();

        let document = r#"{
          "version": 1, "kind": "er", "title": "Schema — posts",
          "tables": [
            { "name": "users", "columns": [ { "name": "id", "type": "uuid", "key": "pk" } ] },
            { "name": "posts", "columns": [
                { "name": "id", "type": "uuid", "key": "pk" },
                { "name": "user_id", "type": "uuid", "key": "fk", "nullable": true }
            ] }
          ],
          "relations": [
            { "from": { "table": "users", "column": "id" },
              "to": { "table": "posts", "column": "user_id" },
              "cardinality": "1:N" }
          ]
        }"#;
        let response = render(document);

        assert!(response.ok, "{response:?}");
        assert_eq!(response.rendered_kind.as_deref(), Some("er"));
        assert_eq!(response.metrics.expect("metrics").nodes, 2);
    }

    #[test]
    fn an_unknown_kind_is_reported() {
        let _guard = lock_renders_blocking();

        let response = render(r#"{ "version": 1, "kind": "sankey" }"#);

        assert!(!response.ok);
        assert_eq!(error_codes(&response), vec!["unknown_kind"]);
        assert_eq!(response.errors[0].path, "kind");
        assert!(response.errors[0].hint.is_some());
        assert!(diagram_store().list().is_empty());
    }

    #[test]
    fn an_unknown_field_is_reported() {
        let _guard = lock_renders_blocking();

        let response = render(
            r#"{
              "version": 1,
              "kind": "graph",
              "nodes": [ { "id": "a", "colour": "red" } ]
            }"#,
        );

        assert!(!response.ok);
        assert_eq!(error_codes(&response), vec!["unknown_field"]);
        assert!(diagram_store().list().is_empty());
    }

    #[test]
    fn the_render_hook_confirms_the_paint() {
        let _guard = lock_renders_blocking();

        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let hook: RenderHook = Arc::new(move |payload| {
            recorder
                .lock()
                .expect("the recorder is not poisoned")
                .push((
                    payload.diagram_id.clone(),
                    payload.kind.clone(),
                    payload.svg.clone(),
                    payload.width,
                    payload.height,
                ));
            RenderStatus::Painted
        });
        set_render_hook(Some(hook));

        let response = render(MINIMAL);

        assert!(response.ok);
        assert!(response.rendered);
        assert_eq!(response.reason, None);
        assert!(response.warnings.is_empty(), "{:?}", response.warnings);
        let seen = seen.lock().expect("the recorder is not poisoned");
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "d-0001");
        assert_eq!(seen[0].1, "graph");
        assert!(seen[0].2.starts_with("<svg"), "{}", seen[0].2);
        assert!(seen[0].3 > 0.0);
        assert!(seen[0].4 > 0.0);

        set_render_hook(None);
    }

    #[test]
    fn a_rejected_paint_keeps_the_diagram_unrendered() {
        let _guard = lock_renders_blocking();

        set_render_hook(Some(Arc::new(|_payload| RenderStatus::Error)));
        let response = render(MINIMAL);
        set_render_hook(None);

        assert!(response.ok);
        assert!(!response.rendered);
        assert_eq!(response.diagram_id.as_deref(), Some("d-0001"));
        assert_eq!(response.reason.as_deref(), Some("ui_error"));
    }

    /// The reasons of a render that did not reach the screen are stable:
    /// the agent branches on them (PRD §8.6.1).
    #[test]
    fn every_reason_of_an_unpainted_render_is_stable() {
        let _guard = lock_renders_blocking();

        for (status, reason) in [
            (RenderStatus::Headless, "headless"),
            (RenderStatus::NoWindow, "no_window"),
            (RenderStatus::Timeout, "ui_timeout"),
            (RenderStatus::Error, "ui_error"),
        ] {
            set_render_hook(Some(Arc::new(move |_payload| status)));
            let response = render(MINIMAL);
            set_render_hook(None);

            assert!(!response.rendered, "{reason}");
            assert_eq!(response.reason.as_deref(), Some(reason));
            assert!(
                response.warnings.iter().any(|w| w.code == reason),
                "the warning has to carry the reason: {:?}",
                response.warnings
            );
        }
    }

    /// S21: the subgraph of `opts.extract` is what gets painted, it keeps the
    /// id of the parent and the response names it in `extracted_from`.
    #[test]
    fn extract_renders_the_neighbourhood_and_names_the_parent() {
        let _guard = lock_renders_blocking();

        let painted = Arc::new(Mutex::new(String::new()));
        let recorder = Arc::clone(&painted);
        set_render_hook(Some(Arc::new(move |payload| {
            *recorder.lock().expect("the recorder is not poisoned") = payload.svg.clone();
            RenderStatus::Painted
        })));

        let response = render_with_opts(&chain(4), extract("n1", 1));

        set_render_hook(None);

        assert!(response.ok, "{:?}", response.errors);
        assert_eq!(response.diagram_id.as_deref(), Some("d-0001"));
        assert_eq!(response.extracted_from.as_deref(), Some("d-0001@1"));
        let metrics = response.metrics.expect("the cost is reported");
        assert_eq!(metrics.nodes, 3, "the neighbourhood of n1 in a chain of 4");
        assert_eq!(metrics.edges, 2);

        let svg = painted
            .lock()
            .expect("the recorder is not poisoned")
            .clone();
        for inside in ["nodo 0", "nodo 1", "nodo 2"] {
            assert!(svg.contains(inside), "the subgraph paints {inside}: {svg}");
        }
        assert!(!svg.contains("nodo 3"), "the far node stays out: {svg}");

        // Extract is a view, not an edit: the plane keeps the whole document.
        let stored = diagram_store()
            .record("d-0001")
            .expect("the diagram is stored")
            .document;
        let stored: serde_json::Value =
            serde_json::from_str(&stored).expect("the document is JSON");
        assert_eq!(
            stored["nodes"].as_array().expect("nodes is an array").len(),
            4,
            "get_diagram returns the document the agent sent"
        );

        let json = serde_json::to_value(&response).expect("the response serializes");
        assert_eq!(json["extracted_from"], "d-0001@1");
        assert_eq!(json["diagram_id"], "d-0001");
    }

    /// S21: an `around` that is not a node of the diagram is an actionable
    /// error and the plane is left untouched.
    #[test]
    fn extract_of_an_unknown_node_is_reported_and_stores_nothing() {
        let _guard = lock_renders_blocking();

        let response = render_with_opts(MINIMAL, extract("nope", 1));

        assert!(!response.ok, "{response:?}");
        assert_eq!(error_codes(&response), vec!["unknown_ref"]);
        assert_eq!(response.errors[0].path, "opts.extract.around");
        assert!(
            response.errors[0].message.contains("nope"),
            "the error names the id: {:?}",
            response.errors[0]
        );
        assert!(response.errors[0].hint.is_some());
        assert_eq!(response.diagram_id, None);
        assert_eq!(response.extracted_from, None);
        assert!(response.metrics.is_none());
        assert!(
            diagram_store().list().is_empty(),
            "a rejected diagram never reaches the plane"
        );
    }

    /// S21: a layout that runs past `opts.layout_timeout_ms` answers
    /// `layout_timeout` with the elapsed time and paints nothing.
    #[test]
    fn a_layout_that_misses_the_deadline_reports_the_elapsed_time_and_paints_nothing() {
        let _guard = lock_renders_blocking();
        crate::test_support::set_layout_delay(200);

        let painted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let saw_paint = Arc::clone(&painted);
        set_render_hook(Some(Arc::new(move |_payload| {
            saw_paint.store(true, std::sync::atomic::Ordering::SeqCst);
            RenderStatus::Painted
        })));

        let response = render_with_opts(
            MINIMAL,
            RenderOptions {
                layout_timeout_ms: 20,
                ..RenderOptions::default()
            },
        );

        set_render_hook(None);
        crate::test_support::set_layout_delay(0);

        assert!(!response.ok, "{response:?}");
        assert_eq!(error_codes(&response), vec!["layout_timeout"]);
        let error = &response.errors[0];
        assert_eq!(error.path, "diagram");
        assert!(
            error.message.contains("20 ms"),
            "the error names the deadline: {error:?}"
        );
        let hint = error.hint.as_deref().unwrap_or_default();
        assert!(hint.contains("opts.layout_timeout_ms"), "{error:?}");
        assert!(hint.contains("opts.extract"), "{error:?}");

        let metrics = response.metrics.expect("the elapsed time is reported");
        let elapsed = metrics
            .elapsed_ms
            .expect("layout_timeout carries elapsed_ms");
        assert!(
            elapsed >= 20,
            "the elapsed time is the real one: {metrics:?}"
        );
        assert_eq!(metrics.nodes, 0);
        assert_eq!(metrics.edges, 0);
        assert_eq!(metrics.render_ms, 0);
        assert_eq!(metrics.svg_bytes, 0);

        assert_eq!(response.diagram_id, None);
        assert_eq!(response.extracted_from, None);
        assert!(
            !painted.load(std::sync::atomic::Ordering::SeqCst),
            "a layout past the deadline is never painted"
        );
        assert!(
            diagram_store().list().is_empty(),
            "the plane is left untouched"
        );

        let json = serde_json::to_value(&response).expect("the response serializes");
        assert_eq!(json["metrics"]["elapsed_ms"].as_u64(), Some(elapsed));
        assert!(json.get("diagram_id").is_none(), "{json}");
    }

    /// S21: a layout inside the deadline is painted as always, and a healthy
    /// render does not pay for the deadline in its wire shape.
    #[test]
    fn a_layout_inside_the_deadline_is_painted() {
        let _guard = lock_renders_blocking();
        crate::test_support::set_layout_delay(20);
        set_render_hook(Some(Arc::new(|_payload| RenderStatus::Painted)));

        let response = render_with_opts(
            MINIMAL,
            RenderOptions {
                layout_timeout_ms: 2000,
                ..RenderOptions::default()
            },
        );

        set_render_hook(None);

        assert!(response.ok, "{:?}", response.errors);
        assert!(response.rendered);
        assert_eq!(response.diagram_id.as_deref(), Some("d-0001"));
        assert_eq!(response.extracted_from, None);
        let metrics = response.metrics.expect("metrics are reported");
        assert_eq!(metrics.nodes, 2);
        assert_eq!(
            metrics.elapsed_ms, None,
            "a healthy render keeps the old shape"
        );
        assert_eq!(diagram_store().list().len(), 1);
    }

    /// S3/D2: the answer of a render names the session of the caller, with
    /// the family and the label the human sees in the window.
    #[test]
    fn the_render_carries_the_session_of_the_caller() {
        let _guard = lock_renders_blocking();
        let id = format!("pi-{}", std::process::id());
        let identity = crate::session::resolve(
            &crate::session::Declared {
                client: Some("pi".to_string()),
                session: Some(id.clone()),
                project: Some("/home/x/graphium".to_string()),
            },
            &crate::session::ClientFingerprint::default(),
            &crate::session::ProcessFingerprint::default(),
        );
        crate::session::session_registry().connect(&id, &identity);

        let response = render_diagram(&RenderRequest {
            document: MINIMAL.to_string(),
            session_id: id.clone(),
        });
        let session = response
            .session
            .expect("the session travels with the answer");
        assert_eq!(session.id, id);
        assert_eq!(session.family, "pi");
        assert_eq!(
            session.label,
            format!("Pi · graphium · {}", &id[id.len() - 4..])
        );
        assert_eq!(session.project.as_deref(), Some("/home/x/graphium"));
        assert_eq!(session.state, "active");

        // A render that fails answers with the session too: the agent learns
        // how the human sees it even when nothing reached the plan.
        let rejected = render_diagram_from_json(
            &RenderRequest {
                document: MINIMAL.to_string(),
                session_id: id.clone(),
            },
            Some(&serde_json::json!({ "extract": { "depth": 2 } })),
        );
        assert!(!rejected.ok, "{rejected:?}");
        assert_eq!(
            rejected.session.expect("the session travels").id,
            id,
            "a rejected render still names its session"
        );
    }

    /// PRESERVE: a render answers the fields it always answered; `session`
    /// is added next to them and no existing one moved (S3: additive).
    #[test]
    fn a_render_keeps_the_fields_it_had() {
        let _guard = lock_renders_blocking();
        let response = render(MINIMAL);
        let value = serde_json::to_value(&response).expect("the answer is JSON");

        for key in ["ok", "diagram_id", "rendered_kind", "rendered"] {
            assert!(value.get(key).is_some(), "falta `{key}`: {value}");
        }
        assert!(value["metrics"].is_object(), "{value}");
        assert!(value["session"].is_object(), "{value}");
    }
}
