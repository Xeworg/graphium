//! Relay of MCP calls to the process that owns the window (PRD §8.6).
//!
//! The owner of the socket serves a minimal JSON-RPC dispatcher: the
//! forwarded calls are the same tool calls the stdio server serves, so a
//! second MCP client gets the same answers from the same window.
//!
//! Every accepted connection is a session of the owner (feature `f3-sesiones`,
//! T1-bis/T3): it registers with the identity its client declares on the wire,
//! it spends one place of `MAX_SESSIONS` together with the stdio session of
//! the owner, and when its transport closes it goes into the grace of D4
//! instead of erasing the diagrams of the agent that may reconnect.
//!
//! The **key** of that session is the id the client declares, else the id of
//! the connection ([`gx_mcp::session`], D4): a client that reconnects and
//! declares the same `GRAPHIUM_SESSION` lands on its own record and its own
//! diagrams, on whichever socket it arrives.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::SystemTime;

use gx_mcp::session::{
    resolve, ClientFingerprint, Declared, ProcessFingerprint, ResolvedIdentity, SessionAdmission,
    SessionRegistry,
};
use serde_json::{json, Value};

use crate::daemon;

/// Counter used to build the session id of a forwarded connection.
static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A graceful shutdown was ordered: `graphium quit`, the tray, or SIGTERM.
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// How many tool calls are running right now, across every connection.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// A shutdown order arrived (quit control line, tray, SIGTERM).
pub fn quit_requested() -> bool {
    QUIT_REQUESTED.load(Ordering::SeqCst)
}

/// Orders the graceful shutdown.
pub fn request_quit() {
    QUIT_REQUESTED.store(true, Ordering::SeqCst);
}

/// Tool calls running at this moment: the exit never abandons one (S7).
pub fn in_flight() -> usize {
    IN_FLIGHT.load(Ordering::SeqCst)
}

/// The transport id of a forwarded connection: `sock-<pid>-<n>`.
///
/// It names the connection, not the session: the key of the session is the id
/// its client declares, and this one is only the key of a client that declares
/// nothing (D4).
pub fn forwarded_session_id() -> String {
    let counter = CONNECTION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("sock-{}-{}", std::process::id(), counter)
}

/// The tools of the server, as a forwarded client sees them.
///
/// The list comes from the single dispatcher of `gx-mcp`, so a client that
/// reaches the owner through the socket sees the same tools as one that
/// speaks stdio to the owner directly.
pub fn tools() -> Vec<Value> {
    gx_mcp::dispatch::TOOL_NAMES
        .iter()
        .map(|name| {
            json!({
                "name": name,
                "description": gx_mcp::harness::description(name),
                "inputSchema": gx_mcp::dispatch::tool_schema(name)
            })
        })
        .collect()
}

/// Handles one JSON-RPC line and returns the response line.
///
/// Notifications (requests without `id`) return an empty string: nothing is
/// written back to the client.
pub fn handle_request(line: &str, session_id: &str) -> String {
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => return error_response(Value::Null, -32700, &format!("parse error: {error}")),
    };

    let id = request.get("id").cloned();
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if id.is_none() {
        // Notification: no response.
        return String::new();
    }
    let id = id.unwrap_or(Value::Null);

    match method {
        "initialize" => success_response(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "graphium", "version": env!("CARGO_PKG_VERSION") },
                // The harness text travels with the answer: an agent that
                // reaches the owner through the socket gets the same
                // instructions one that speaks stdio to a fresh binary gets
                // (the harness's own contract, harness.rs).
                "instructions": gx_mcp::harness::INSTRUCTIONS
            }),
        ),
        "tools/list" => success_response(id, json!({ "tools": tools() })),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(json!({}));
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            match gx_mcp::dispatch::call_tool(name, &arguments, session_id) {
                Ok(payload) => {
                    let failed = payload.get("ok") == Some(&Value::Bool(false));
                    success_response(
                        id,
                        json!({
                            "content": [ { "type": "text", "text": payload.to_string() } ],
                            "structuredContent": payload,
                            "isError": failed
                        }),
                    )
                }
                Err(message) => error_response(id, -32602, &message),
            }
        }
        other => error_response(id, -32601, &format!("method not found: {other}")),
    }
}

fn success_response(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
    .to_string()
}

/// The maximum number of MCP sessions a single owner serves (PRD RNF9).
///
/// The count includes the stdio session of the owner (D6): with the owner
/// speaking stdio, `MAX_SESSIONS - 1` socket clients get in and the next one
/// is refused with `too_many_sessions`.
pub const MAX_SESSIONS: usize = 8;

/// The JSON-RPC method the forwarder uses to declare who it stands for (D1).
///
/// It is a notification, so an owner that does not know it ignores it: the
/// wire keeps working with a relay of another version.
pub const IDENTITY_METHOD: &str = "graphium/identity";

/// Serves the socket until the process ends: one thread per connection.
pub fn serve(listener: UnixListener) {
    listener
        .set_nonblocking(true)
        .expect("the accept loop must poll");
    loop {
        if quit_requested() {
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                std::thread::spawn(move || {
                    let session_id = forwarded_session_id();
                    serve_session(stream, &session_id, gx_mcp::session::session_registry());
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => continue,
        }
    }
}

/// Serves one accepted connection as a session (T1-bis, T3, D4, D6).
///
/// The session registers before the first answer, so it spends one place of
/// `MAX_SESSIONS` together with the owner's stdio session; past the quota the
/// connection is refused with `too_many_sessions` and the number of open
/// sessions. The connection begins under its transport id and moves to the key
/// its client declares as soon as the identity arrives, so the tools of this
/// connection stamp their diagrams with that key. When the client closes the
/// stream the session does not vanish: it goes into the grace of D4 — unless
/// another transport of the same declared session is still holding it —, which
/// is what keeps its diagrams alive for a reconnect.
pub fn serve_session(stream: UnixStream, transport_id: &str, registry: &SessionRegistry) {
    let mut identity = WireIdentity::default();
    let mut key = match registry.admit(transport_id, &identity.resolved(), MAX_SESSIONS) {
        SessionAdmission::Accepted(record) => record.id,
        SessionAdmission::Refused { open } => {
            refuse(stream, open);
            return;
        }
    };

    serve_connection(stream, transport_id, &mut key, registry, &mut identity);
    registry.begin_grace(transport_id, SystemTime::now());
}

/// What the owner knows about the identity of a connection, level by level
/// (D1).
///
/// The socket carries it piece by piece: the forwarder declares the `env` of
/// the client it stands for (level 1) and the handshake reports `clientInfo`
/// (level 2). The process level is the one of the forwarder, because the
/// process of the client is not reachable across the socket: the owner never
/// invents one of its own.
#[derive(Debug, Default)]
struct WireIdentity {
    declared: Declared,
    client: ClientFingerprint,
    process: ProcessFingerprint,
}

impl WireIdentity {
    fn resolved(&self) -> ResolvedIdentity {
        resolve(&self.declared, &self.client, &self.process)
    }
}

/// Answers a session that exceeds the quota and closes it (PRD RNF9, D6).
///
/// The shape of the error is the one it always had; it now says how many
/// sessions are open, which is the number the client hit.
fn refuse(mut stream: UnixStream, open: usize) {
    let error = json!({
        "jsonrpc": "2.0",
        "id": Value::Null,
        "error": {
            "code": -32000,
            "message": format!(
                "too_many_sessions: {open} sesiones abiertas (máximo {MAX_SESSIONS})"
            ),
            "data": {
                "code": "too_many_sessions",
                "limit": MAX_SESSIONS,
                "open": open
            }
        }
    });
    let _ = writeln!(stream, "{error}");
    let _ = stream.flush();
}

fn serve_connection(
    stream: UnixStream,
    transport_id: &str,
    session_key: &mut String,
    registry: &SessionRegistry,
    identity: &mut WireIdentity,
) {
    let reader = BufReader::new(match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    });
    let mut writer = stream;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        // A control line is not JSON-RPC: `graphium quit` opens its own
        // connection, orders the graceful shutdown and closes (S5).
        if line.trim() == daemon::QUIT_LINE {
            request_quit();
            break;
        }
        // The identity of the client arrives on the wire, in the two levels
        // that reach this side of the socket. Refreshing the same session does
        // not open a second one, so no level consumes a place of the quota;
        // the declared id moves the connection onto the key of the session.
        if let Some((declared, process)) = declared_of_line(&line) {
            identity.declared = declared;
            identity.process = process;
            *session_key = registry.connect(transport_id, &identity.resolved()).id;
            continue;
        }
        if let Some(client) = client_info_of_line(&line) {
            identity.client = client;
            *session_key = registry.connect(transport_id, &identity.resolved()).id;
        }
        let response = {
            // The exit drains these: a tool in flight is never abandoned (S7).
            IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
            let response = handle_request(&line, session_key.as_str());
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
            response
        };
        if response.is_empty() {
            continue;
        }
        if writeln!(writer, "{response}").is_err() || writer.flush().is_err() {
            break;
        }
    }
}

/// The `graphium/identity` notification: the declared level and the process
/// level of the client the forwarder stands for (D1).
fn declared_of_line(line: &str) -> Option<(Declared, ProcessFingerprint)> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("method").and_then(Value::as_str) != Some(IDENTITY_METHOD) {
        return None;
    }
    let params = value.get("params");
    let declared = Declared {
        client: text_of(params, "client"),
        session: text_of(params, "session"),
        project: text_of(params, "project"),
    };
    let process = ProcessFingerprint {
        parent: text_of(params, "parent"),
        cwd: text_of(params, "cwd"),
    };
    Some((declared, process))
}

/// The `clientInfo` of an `initialize` request (D1, level 2).
fn client_info_of_line(line: &str) -> Option<ClientFingerprint> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("method").and_then(Value::as_str) != Some("initialize") {
        return None;
    }
    let info = value.get("params")?.get("clientInfo")?;
    Some(ClientFingerprint {
        name: text_of(Some(info), "name"),
        version: text_of(Some(info), "version"),
    })
}

/// A string field of a JSON object; absent, `null` and non-string are `None`.
fn text_of(object: Option<&Value>, key: &str) -> Option<String> {
    object?.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Forwards the stdio of this process to the owner of the window.
///
/// Only requests with an `id` wait for an answer; notifications are relayed
/// without blocking.
pub fn forward(stream: UnixStream, input: &mut dyn BufRead, output: &mut dyn Write) {
    let mut writer = match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);

    // Who this forwarder stands for, before the first request arrives (D1):
    // the owner cannot read the `env` of the process on the other side of the
    // socket, and the parent process that is worth reporting is this one's,
    // not the owner's.
    if send_identity(&mut writer).is_err() {
        return;
    }

    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if writeln!(writer, "{line}").is_err() || writer.flush().is_err() {
            break;
        }
        let expects_answer = serde_json::from_str::<Value>(&line)
            .ok()
            .and_then(|value| value.get("id").cloned())
            .is_some();
        if !expects_answer {
            continue;
        }
        let mut response = String::new();
        match reader.read_line(&mut response) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        if writeln!(output, "{}", response.trim_end()).is_err() || output.flush().is_err() {
            break;
        }
    }
}

/// Sends the `graphium/identity` notification of this process (D1).
fn send_identity(writer: &mut impl Write) -> std::io::Result<()> {
    let declared = gx_mcp::session::declared_from_env();
    let process = gx_mcp::session::process_fingerprint();
    let notification = json!({
        "jsonrpc": "2.0",
        "method": IDENTITY_METHOD,
        "params": {
            "client": declared.client,
            "session": declared.session,
            "project": declared.project,
            "parent": process.parent,
            "cwd": process.cwd,
        }
    });
    writeln!(writer, "{notification}")?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant, SystemTime};

    use gx_mcp::session::{
        resolve, ClientFingerprint, Declared, ProcessFingerprint, ResolvedIdentity, SessionFamily,
        SessionRegistry, SessionState, STDIO_SESSION_ID,
    };
    use serde_json::Value;

    use super::{handle_request, serve_session, tools, IDENTITY_METHOD, MAX_SESSIONS};

    #[test]
    fn initialize_answers_with_the_server_info() {
        let response = handle_request(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
            "session-1",
        );
        let value: Value = serde_json::from_str(&response).expect("json");
        assert_eq!(value["id"], 1);
        assert_eq!(value["result"]["serverInfo"]["name"], "graphium");
        // The harness travels with the initialize: an agent over the socket
        // reads the same instructions a direct stdio client reads.
        let instructions = value["result"]["instructions"]
            .as_str()
            .expect("the instructions travel");
        assert!(
            instructions.contains("THE CHEAP PATH"),
            "the instructions are the harness's own"
        );
    }

    #[test]
    fn notifications_get_no_answer() {
        let response = handle_request(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "session-1",
        );
        assert!(response.is_empty());
    }

    /// The identity notification is a notification: an owner of another
    /// version answers nothing to it, which is what keeps the wire working.
    #[test]
    fn the_identity_notification_gets_no_answer() {
        let response = handle_request(
            &format!(
                r#"{{"jsonrpc":"2.0","method":"{IDENTITY_METHOD}","params":{{"client":"pi"}}}}"#
            ),
            "session-1",
        );
        assert!(response.is_empty());
    }

    #[test]
    fn tools_list_returns_every_tool() {
        let response = handle_request(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "session-1",
        );
        let value: Value = serde_json::from_str(&response).expect("json");
        let names: Vec<&str> = value["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert!(names.contains(&"render_diagram"));
        assert!(names.contains(&"validate_diagram"));
        assert!(names.contains(&"list_diagrams"));
    }

    #[test]
    fn unknown_methods_return_an_error() {
        let response = handle_request(r#"{"jsonrpc":"2.0","id":3,"method":"nope"}"#, "session-1");
        let value: Value = serde_json::from_str(&response).expect("json");
        assert_eq!(value["error"]["code"], -32601);
    }

    #[test]
    fn broken_json_returns_a_parse_error() {
        let response = handle_request("{not json", "session-1");
        let value: Value = serde_json::from_str(&response).expect("json");
        assert_eq!(value["error"]["code"], -32700);
    }

    /// The identity a session gets when nothing is known yet (D1): the
    /// transport registers it, and the wire fills the levels in.
    fn unknown_identity() -> ResolvedIdentity {
        resolve(
            &Declared::default(),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        )
    }

    /// The client end of a connection the owner serves as a session.
    struct Client {
        writer: UnixStream,
        reader: BufReader<UnixStream>,
    }

    impl Client {
        fn new(stream: UnixStream) -> Self {
            let writer = stream.try_clone().expect("a cloned end");
            Self {
                writer,
                reader: BufReader::new(stream),
            }
        }

        fn send(&mut self, line: &str) {
            writeln!(self.writer, "{line}").expect("the request fits");
            self.writer.flush().expect("the request goes out");
        }

        /// The next line the owner writes.
        fn receive(&mut self) -> String {
            let mut line = String::new();
            self.reader.read_line(&mut line).expect("an answer");
            line.trim_end().to_string()
        }

        /// Asks `initialize`: the connection is served once the owner answers.
        fn initialize(&mut self) {
            self.send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
            let answer = self.receive();
            assert!(answer.contains("graphium"), "the owner answers: {answer}");
        }
    }

    /// Opens a pair and serves one end as a session, on its own thread.
    fn open_session<'scope, 'env>(
        scope: &'scope std::thread::Scope<'scope, 'env>,
        registry: &'env SessionRegistry,
        id: String,
    ) -> Client {
        let (client, server) = UnixStream::pair().expect("a socket pair");
        scope.spawn(move || serve_session(server, &id, registry));
        Client::new(client)
    }

    /// Waits (bounded) until the transport close of `id` reaches the registry.
    fn wait_in_grace(registry: &SessionRegistry, id: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if registry.get(id).map(|record| record.state) == Some(SessionState::InGrace) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the session {id} never went into grace");
    }

    /// T3/D6: `MAX_SESSIONS` counts every session — the owner's stdio one and
    /// every socket — and the ninth attempt is refused with its count.
    #[test]
    fn the_quota_counts_the_stdio_session_and_every_socket() {
        let registry = SessionRegistry::new();
        // The owner's stdio transport registered itself when it started (T1).
        registry.connect(STDIO_SESSION_ID, &unknown_identity());

        std::thread::scope(|scope| {
            let mut open = Vec::new();
            // 1 stdio + 7 sockets fill the quota: the 8th session.
            for number in 0..MAX_SESSIONS - 1 {
                let id = format!("sock-{number}");
                let mut client = open_session(scope, &registry, id.clone());
                client.initialize();
                assert!(registry.get(&id).is_some(), "{id} is registered");
                open.push(client);
            }
            assert_eq!(registry.active_count(), MAX_SESSIONS);

            // The ninth attempt is refused instead of served.
            let mut refused = open_session(scope, &registry, "sock-ninth".to_string());
            let answer: Value = serde_json::from_str(&refused.receive()).expect("json");
            assert_eq!(answer["error"]["code"], -32000);
            assert_eq!(answer["error"]["data"]["code"], "too_many_sessions");
            assert_eq!(answer["error"]["data"]["limit"], MAX_SESSIONS);
            assert_eq!(
                answer["error"]["data"]["open"], MAX_SESSIONS,
                "the error says how many sessions are open"
            );
            let message = answer["error"]["message"].as_str().expect("a message");
            assert!(message.starts_with("too_many_sessions"), "{message}");
            assert!(
                message.contains(&format!("{MAX_SESSIONS} sesiones abiertas")),
                "the message says how many are open: {message}"
            );
            assert!(
                registry.get("sock-ninth").is_none(),
                "a refused session does not register"
            );
        });
    }

    /// T3/D6: a session that ends frees its slot. A session in grace already
    /// ended for the quota, so a client restart is never locked out for 45 s.
    #[test]
    fn a_session_that_ends_frees_its_slot() {
        let registry = SessionRegistry::new();
        registry.connect(STDIO_SESSION_ID, &unknown_identity());

        std::thread::scope(|scope| {
            let mut open = Vec::new();
            for number in 0..MAX_SESSIONS - 1 {
                let id = format!("sock-{number}");
                let mut client = open_session(scope, &registry, id);
                client.initialize();
                open.push(client);
            }
            assert_eq!(registry.active_count(), MAX_SESSIONS);

            // Closing the transport ends the session: it goes into grace and
            // leaves the count, and the grace is what keeps its diagrams (D4).
            drop(open.remove(0));
            wait_in_grace(&registry, "sock-0");
            assert_eq!(registry.active_count(), MAX_SESSIONS - 1);

            // With the slot free, a new connection is served.
            let mut client = open_session(scope, &registry, "sock-new".to_string());
            client.initialize();
            assert!(registry.get("sock-new").is_some());
            assert_eq!(registry.active_count(), MAX_SESSIONS);
        });
    }

    /// T3/S4: a declared id is the key of its connection, so the tools of
    /// that connection scope their diagrams with it and a reconnect on
    /// another socket finds them instead of starting over.
    #[test]
    fn a_declared_id_is_the_key_of_the_connection() {
        let registry = SessionRegistry::new();
        let declared_id = format!("pi-declared-{}", std::process::id());
        let store = gx_mcp::store::diagram_store();
        let diagram = store
            .insert(
                store.next_id(),
                "t".to_string(),
                "graph".to_string(),
                declared_id.clone(),
                "{}".to_string(),
            )
            .diagram_id;

        std::thread::scope(|scope| {
            let mut client = open_session(scope, &registry, "sock-1".to_string());
            client.send(&format!(
                r#"{{"jsonrpc":"2.0","method":"{IDENTITY_METHOD}","params":{{"client":"pi","session":"{declared_id}","project":"/work/graphium"}}}}"#
            ));
            client.send(
                r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_diagrams","arguments":{}}}"#,
            );
            let answer: Value = serde_json::from_str(&client.receive()).expect("json");
            let listed = answer["result"]["structuredContent"]["diagrams"]
                .as_array()
                .expect("the tool answers with its diagrams");
            assert_eq!(
                listed.len(),
                1,
                "the connection scopes its diagrams with the declared id: {answer}"
            );
            assert_eq!(listed[0]["diagram_id"], diagram);
            assert_eq!(listed[0]["session_id"], declared_id.as_str());
        });

        // One session, keyed by the declared id: the transport id it began
        // with is not a session of its own.
        assert_eq!(
            registry.list().len(),
            1,
            "one record for one declared session"
        );
        let record = registry
            .get(&declared_id)
            .expect("the declared id is the key");
        assert_eq!(record.family, SessionFamily::Pi);
        assert!(
            registry.get("sock-1").is_none(),
            "the transport id is not a session any more"
        );
        store.clear_session(&declared_id);
    }

    /// T1-bis/D1: an accepted socket is a session, and its identity arrives on
    /// the wire: the declared level wins over `clientInfo`.
    ///
    /// The declared id is the key of the session (D4), so the record moves from
    /// its transport id to `pi-4824`: there is one session, not two.
    #[test]
    fn an_accepted_socket_registers_with_the_identity_its_client_declares() {
        let registry = SessionRegistry::new();
        std::thread::scope(|scope| {
            let mut client = open_session(scope, &registry, "sock-1".to_string());

            // Level 2: the `clientInfo` of the handshake, with nothing
            // declared yet, so the key is the transport id.
            client.send(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"claude-ai","version":"2.0.0"}}}"#,
            );
            let answer = client.receive();
            assert!(answer.contains("graphium"), "{answer}");
            let record = registry.get("sock-1").expect("the session is registered");
            assert_eq!(record.family, SessionFamily::Claude);
            assert_eq!(record.client_name.as_deref(), Some("claude-ai"));
            assert_eq!(record.client_version.as_deref(), Some("2.0.0"));
            assert_eq!(record.state, SessionState::Active);

            // Level 1: what the forwarder declared. The declared id is the key
            // of the session now, so the record lands on it.
            client.send(&format!(
                r#"{{"jsonrpc":"2.0","method":"{IDENTITY_METHOD}","params":{{"client":"pi","session":"pi-4824","project":"/work/graphium","parent":"node","cwd":"/work/graphium"}}}}"#
            ));
            client.send(r#"{"jsonrpc":"2.0","id":2,"method":"initialize"}"#);
            let answer = client.receive();
            assert!(answer.contains("graphium"), "{answer}");

            let record = registry.get("pi-4824").expect("the same session");
            assert_eq!(record.family, SessionFamily::Pi);
            assert_eq!(record.project.as_deref(), Some("/work/graphium"));
            assert_eq!(
                record.client_version, None,
                "the version comes from the same level as the family"
            );
            assert!(
                record.label.starts_with("Pi · graphium · "),
                "{}",
                record.label
            );
            assert_eq!(registry.list().len(), 1, "one session, not two");
            assert!(
                registry.get("sock-1").is_none(),
                "the transport id was a key, not a session of its own"
            );
        });
    }

    /// D1: with nothing declared and no `clientInfo`, the third level is the
    /// process of the client, which only the forwarder can see.
    #[test]
    fn the_forwarder_reports_the_process_level_of_the_client() {
        let registry = SessionRegistry::new();
        std::thread::scope(|scope| {
            let mut client = open_session(scope, &registry, "sock-1".to_string());
            client.send(&format!(
                r#"{{"jsonrpc":"2.0","method":"{IDENTITY_METHOD}","params":{{"parent":"opencode","cwd":"/work/web"}}}}"#
            ));
            client.send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
            let answer = client.receive();
            assert!(answer.contains("graphium"), "{answer}");

            let record = registry.get("sock-1").expect("the session is registered");
            assert_eq!(record.family, SessionFamily::OpenCode);
            assert_eq!(record.project.as_deref(), Some("/work/web"));
        });
    }

    /// D4: closing the transport does not erase the session; it puts it in
    /// grace, which is what keeps its diagrams alive for a reconnect.
    #[test]
    fn closing_the_transport_puts_the_session_in_grace() {
        let registry = SessionRegistry::new();
        std::thread::scope(|scope| {
            let mut client = open_session(scope, &registry, "sock-1".to_string());
            client.initialize();
            drop(client);
            wait_in_grace(&registry, "sock-1");
        });

        let record = registry.get("sock-1").expect("the session is still there");
        assert_eq!(record.state, SessionState::InGrace);
        assert!(record.grace_deadline.is_some());
        assert!(
            registry.expired(SystemTime::now()).is_empty(),
            "closing does not shorten the grace"
        );
    }

    /// D1: the forwarder announces who it stands for and keeps relaying the
    /// requests of its client with their answers, as it always did.
    #[test]
    fn the_forwarder_declares_its_identity_and_relays_the_answer() {
        let (forwarder, owner) = UnixStream::pair().expect("a socket pair");
        let request = "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"initialize\"}\n";
        let mut output = Vec::new();

        std::thread::scope(|scope| {
            scope.spawn(move || {
                let mut reader = BufReader::new(owner.try_clone().expect("a cloned end"));
                let mut line = String::new();
                reader.read_line(&mut line).expect("the identity arrives");
                let identity: Value = serde_json::from_str(&line).expect("json");
                assert_eq!(identity["method"], IDENTITY_METHOD);
                assert!(identity["params"].is_object(), "{identity}");
                assert!(identity["params"].get("cwd").is_some(), "{identity}");

                line.clear();
                reader.read_line(&mut line).expect("the request arrives");
                let forwarded: Value = serde_json::from_str(&line).expect("json");
                assert_eq!(forwarded["id"], 7);

                let mut writer = owner;
                writeln!(writer, "{{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{{}}}}")
                    .expect("the answer fits");
                writer.flush().expect("the answer goes out");
            });

            let mut input = std::io::Cursor::new(request.as_bytes());
            super::forward(forwarder, &mut input, &mut output);
        });

        let answer = String::from_utf8(output).expect("the answer is text");
        assert_eq!(answer.trim_end(), r#"{"jsonrpc":"2.0","id":7,"result":{}}"#);
    }

    #[test]
    fn every_tool_is_forwarded_with_its_description() {
        let tools = tools();
        assert_eq!(tools.len(), gx_mcp::dispatch::TOOL_NAMES.len());
        for tool in &tools {
            assert!(tool["description"].as_str().unwrap().len() > 20, "{tool}");
            assert!(tool["inputSchema"].is_object(), "{tool}");
        }
        let render = tools
            .iter()
            .find(|tool| tool["name"] == "render_diagram")
            .expect("render");
        assert_eq!(render["inputSchema"]["required"][0], "diagram");
    }
}
