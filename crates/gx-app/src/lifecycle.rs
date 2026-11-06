//! Lifecycle of the owner process (feature `f5-background`, S4/S5/S6).
//!
//! The owner is not a window with an MCP server inside: it is a process that
//! lives while it has sessions, and the window is only one view of it. This
//! module is the state machine that decides when it dies (S4/S5) and the one
//! that keeps the human counted as a session (S6).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use gx_mcp::session::{resolve, ClientFingerprint, Declared, ResolvedIdentity, SessionRegistry};
use tauri::Manager;

use crate::relay;

/// The id of the human's session (S6).
///
/// It is the key of the record **and** the transport that holds it: the human
/// has one window, so one session.
pub const UI_SESSION_ID: &str = "ui";

/// How often the watcher looks at the window and at the sessions.
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// How long the owner lives with no session at all (S5: 30-60 s). The grace is
/// what keeps a `/mcp reconnect` from killing the program (S7).
pub const IDLE_GRACE: Duration = Duration::from_secs(60);

/// How long the exit waits for the tool calls in flight (S7).
pub const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// A SIGTERM arrived: the same graceful exit as the tray and `graphium quit`.
static TERMINATED: AtomicBool = AtomicBool::new(false);

/// A SIGTERM was received.
pub fn term_requested() -> bool {
    TERMINATED.load(Ordering::SeqCst)
}

/// Orders the exit through the SIGTERM path.
pub fn request_term() {
    TERMINATED.store(true, Ordering::SeqCst);
}

/// Whether the owner has to begin its graceful exit, wherever the order came
/// from (S5: tray «Salir», `graphium quit`, SIGTERM).
pub fn quit_requested() -> bool {
    relay::quit_requested() || term_requested()
}

/// The identity of the human in front of the window (S6): the `ui` family and
/// the project of the daemon.
///
/// The levels are the ones every session goes through (D1): this one declares
/// its client — there is no `clientInfo` and no parent process to read — and
/// the project falls back to the `cwd`, which is the plan the daemon owns. No
/// id is declared, so the key of the session is the transport id: `ui`.
pub fn ui_identity() -> ResolvedIdentity {
    let declared = Declared {
        client: Some(UI_SESSION_ID.to_string()),
        session: None,
        project: None,
    };
    resolve(
        &declared,
        &ClientFingerprint::default(),
        &gx_mcp::session::process_fingerprint(),
    )
}

/// Keeps the session of the human in sync with the window (S6).
///
/// «Mirando = en uso»: a visible window is a live session, so the process does
/// not close the plan under the human's feet. Hiding it ends the session, and
/// a window that never showed opens none — hiding is not what registers it.
pub fn sync_ui_session(registry: &SessionRegistry, visible: bool, now: SystemTime) {
    if visible {
        registry.connect(UI_SESSION_ID, &ui_identity());
        return;
    }
    if registry.get(UI_SESSION_ID).is_some() {
        registry.begin_grace(UI_SESSION_ID, now);
    }
}

/// The state machine of S4/S5: no session → grace → draining → exit.
///
/// The clock is a parameter, not a call to `Instant::now()`, so the rule is
/// checkable without waiting a minute.
#[derive(Debug, Default)]
pub struct Shutdown {
    idle_since: Option<Instant>,
    drain_until: Option<Instant>,
}

impl Shutdown {
    /// Notes the sessions and the tool calls in flight at `now`.
    ///
    /// Answers `true` when the owner has to begin its exit: no session for
    /// [`IDLE_GRACE`] in a row and nothing in flight — or the cap of
    /// [`DRAIN_TIMEOUT`] ran out while a tool was still running. A session that
    /// comes back cancels the exit and the next empty stretch counts from
    /// scratch (S5); a tool in flight is never abandoned (S7).
    pub fn observe(&mut self, active: usize, in_flight: usize, now: Instant) -> bool {
        if active > 0 {
            *self = Shutdown::default();
            return false;
        }
        let idle_since = *self.idle_since.get_or_insert(now);
        if now.duration_since(idle_since) < IDLE_GRACE {
            return false;
        }
        let drain_until = *self.drain_until.get_or_insert(now + DRAIN_TIMEOUT);
        in_flight == 0 || now >= drain_until
    }
}

/// Waits (bounded) until no tool call is running (S7).
///
/// Returns whether the process is void of calls: `false` means the cap ran out
/// and the caller leaves with a call still running instead of waiting forever.
pub fn drain(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while relay::in_flight() > 0 {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

/// Watches the window and the sessions, and ends the owner when nothing is
/// left (S4, S5, S6).
///
/// One thread for the whole life of the process: it is the only place that
/// decides the exit, so the tray, `graphium quit` and SIGTERM all land in the
/// same graceful path.
pub fn watch(handle: tauri::AppHandle) {
    install_signal_handler();
    std::thread::spawn(move || {
        let registry = gx_mcp::session::session_registry();
        // The flag and the books root are read once, by the owner: the
        // watcher and the tool share the same configuration (F9, S5).
        let books = gx_mcp::book::config_from_env();
        let mut shutdown = Shutdown::default();
        loop {
            sync_ui_session(registry, window_visible(&handle), SystemTime::now());
            // The monitor of the project starts with the owner, one book per
            // project of the open sessions (F9, S9; ideas §3 «adopta 6»).
            gx_mcp::book::maintain(&books, &session_projects(registry));
            if quit_requested() {
                drain_and_exit(&handle);
                return;
            }
            if shutdown.observe(registry.active_count(), relay::in_flight(), Instant::now()) {
                drain_and_exit(&handle);
                return;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

/// Ends the sessions whose grace finished and leaves with code 0 (S5).
///
/// The diagrams of the sessions that ended go with them, and a tool in flight
/// gets its bounded wait before the process leaves (S7). The two clocks agree:
/// the grace of a session (45 s) is shorter than the empty stretch that brings
/// the owner here (60 s), so by the time the exit begins the sessions that
/// ended are already expired and their diagrams are the ones being deleted.
pub fn drain_and_exit(handle: &tauri::AppHandle) {
    gx_mcp::session::end_expired_sessions(
        gx_mcp::session::session_registry(),
        gx_mcp::store::diagram_store(),
        SystemTime::now(),
    );
    let _ = drain(DRAIN_TIMEOUT);
    // The books of the sessions stop and close cleanly: the `TRUNCATE`
    // checkpoint and the `clean_shutdown` marker of F9 (ideas §10.1).
    gx_mcp::book::shutdown_all();
    handle.exit(0);
}

/// The projects of the open sessions, without repeats (F9, D4: the project of
/// a session is the record of the session, never `current_dir()`).
fn session_projects(registry: &SessionRegistry) -> Vec<String> {
    let mut projects: Vec<String> = registry
        .list()
        .into_iter()
        .filter_map(|record| record.project)
        .filter(|project| !project.trim().is_empty())
        .collect();
    projects.sort();
    projects.dedup();
    projects
}

/// Whether the plan window is on screen (S6).
fn window_visible(handle: &tauri::AppHandle) -> bool {
    handle
        .get_webview_window(crate::app::MAIN_WINDOW)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

/// Installs the SIGTERM handler (S5).
///
/// Only the flag is raised here: the watcher is the one that drains and exits,
/// so a signal cannot leave the store or the socket half-done. Without it, an
/// owner killed by the system leaves its socket behind.
pub fn install_signal_handler() {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Ok(mut signals) = signal_hook::iterator::Signals::new([signal_hook::consts::SIGTERM])
    else {
        return;
    };
    std::thread::spawn(move || {
        for _signal in signals.forever() {
            request_term();
        }
    });
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant, SystemTime};

    use gx_mcp::session::{SessionFamily, SessionRegistry, SessionState};

    use super::{
        drain, session_projects, sync_ui_session, ui_identity, Shutdown, DRAIN_TIMEOUT, IDLE_GRACE,
        UI_SESSION_ID,
    };

    fn project_of_this_process() -> Option<String> {
        std::env::current_dir()
            .ok()
            .map(|path| path.to_string_lossy().into_owned())
    }

    /// S6: the human counts as a session named `ui`, of the `ui` family, and
    /// its project is the one of the daemon. There is one record, not two.
    #[test]
    fn the_human_is_a_session_of_the_ui_family() {
        let identity = ui_identity();
        assert_eq!(identity.family, SessionFamily::Ui);

        let registry = SessionRegistry::new();
        registry.connect(UI_SESSION_ID, &identity);

        let record = registry
            .get(UI_SESSION_ID)
            .expect("the ui session registers under its own id");
        assert_eq!(record.id, UI_SESSION_ID);
        assert_eq!(record.family, SessionFamily::Ui);
        assert_eq!(record.project, project_of_this_process());
        assert_eq!(record.state, SessionState::Active);
        assert_eq!(registry.list().len(), 1, "the human is one session");
    }

    /// S6: a visible window is a live session («mirando = en uso»); hiding it
    /// ends the session, and a window that never showed is not registered at
    /// all — hiding is not what opens a session.
    #[test]
    fn the_visible_window_is_a_live_session_and_hiding_it_ends_it() {
        let registry = SessionRegistry::new();

        sync_ui_session(&registry, false, SystemTime::now());
        assert!(
            registry.get(UI_SESSION_ID).is_none(),
            "a hidden window that never showed opens no session"
        );
        assert_eq!(registry.active_count(), 0);

        sync_ui_session(&registry, true, SystemTime::now());
        assert_eq!(registry.active_count(), 1, "mirando = en uso");
        assert_eq!(
            registry.get(UI_SESSION_ID).map(|record| record.state),
            Some(SessionState::Active)
        );

        sync_ui_session(&registry, false, SystemTime::now());
        let record = registry
            .get(UI_SESSION_ID)
            .expect("the session is listed while it keeps its grace");
        assert_eq!(record.state, SessionState::InGrace);
        assert!(record.grace_deadline.is_some());
        assert_eq!(registry.active_count(), 0);
    }

    /// S5: the process lives while it has sessions and leaves only after the
    /// grace with none — never the moment the last one ends.
    #[test]
    fn the_owner_exits_only_after_the_grace_with_no_sessions() {
        let mut shutdown = Shutdown::default();
        let t0 = Instant::now();

        assert!(!shutdown.observe(0, 0, t0), "no sale de inmediato");
        assert!(
            !shutdown.observe(0, 0, t0 + IDLE_GRACE - Duration::from_millis(1)),
            "la gracia todavía no terminó"
        );
        assert!(
            shutdown.observe(0, 0, t0 + IDLE_GRACE),
            "sin sesiones durante la gracia, sale"
        );
    }

    /// S5/S7: a session that comes back restarts the count, so a reconnect is
    /// never the death of the process.
    #[test]
    fn a_session_that_comes_back_restarts_the_grace() {
        let mut shutdown = Shutdown::default();
        let t0 = Instant::now();

        assert!(!shutdown.observe(0, 0, t0));
        // A session arrives one second before the end of the grace.
        assert!(!shutdown.observe(1, 0, t0 + IDLE_GRACE - Duration::from_secs(1)));
        // The grace counts from the new empty moment, not from the old one.
        assert!(
            !shutdown.observe(0, 0, t0 + IDLE_GRACE),
            "el contador se reinició"
        );
        assert!(shutdown.observe(0, 0, t0 + IDLE_GRACE * 2));
    }

    /// S7: a tool in flight is never abandoned: the exit waits for it, but it
    /// does not wait forever.
    #[test]
    fn a_tool_in_flight_delays_the_exit_with_a_cap() {
        let mut shutdown = Shutdown::default();
        let t0 = Instant::now();

        assert!(!shutdown.observe(0, 1, t0));
        let expired = t0 + IDLE_GRACE;
        assert!(
            !shutdown.observe(0, 1, expired),
            "la tool en vuelo no se abandona"
        );
        assert!(
            !shutdown.observe(0, 1, expired + DRAIN_TIMEOUT - Duration::from_millis(1)),
            "el drenado todavía espera"
        );
        assert!(
            shutdown.observe(0, 1, expired + DRAIN_TIMEOUT),
            "el tope del drenado llega"
        );
    }

    /// S5/S7: a session that arrives while the owner is draining cancels the
    /// exit and its grace starts from scratch.
    #[test]
    fn a_session_that_arrives_while_draining_cancels_the_exit() {
        let mut shutdown = Shutdown::default();
        let t0 = Instant::now();

        assert!(!shutdown.observe(0, 1, t0));
        assert!(!shutdown.observe(0, 1, t0 + IDLE_GRACE), "drenando");
        let back = t0 + IDLE_GRACE + Duration::from_secs(1);
        assert!(
            !shutdown.observe(1, 1, back),
            "la sesión que vuelve cancela la salida"
        );
        let empty_again = back + Duration::from_secs(1);
        assert!(!shutdown.observe(0, 0, empty_again));
        assert!(shutdown.observe(0, 0, empty_again + IDLE_GRACE));
    }

    /// F9/S9: the owner monitors the project of every open session, once per
    /// project even when two sessions share it (D4).
    #[test]
    fn the_projects_of_the_sessions_are_the_ones_the_owner_monitors() {
        let registry = SessionRegistry::new();
        for (id, project) in [
            ("a", Some("/work/alpha")),
            ("b", Some("/work/beta")),
            ("c", Some("/work/alpha")),
            ("d", None),
        ] {
            let identity = gx_mcp::session::ResolvedIdentity {
                id: Some(id.to_string()),
                family: SessionFamily::Other,
                client_name: None,
                client_version: None,
                project: project.map(str::to_string),
                source: gx_mcp::session::IdentitySource::Declared,
            };
            registry.connect(id, &identity);
        }
        assert_eq!(
            session_projects(&registry),
            vec!["/work/alpha".to_string(), "/work/beta".to_string()],
            "one book per project, and a session without one adds nothing"
        );
    }

    /// S7: with nothing in flight the drain returns at once instead of
    /// spending its cap.
    #[test]
    fn the_drain_returns_at_once_when_nothing_is_in_flight() {
        let started = Instant::now();
        assert!(drain(DRAIN_TIMEOUT));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "el drenado no espera cuando no hay nada que drenar"
        );
    }
}
