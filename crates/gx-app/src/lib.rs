//! `gx-app` — the Graphium application.
//!
//! This crate is the composition root: it installs the engines, opens the
//! plan window, owns the single instance socket and serves the MCP client
//! (PRD §8.6, §8.6.1).

pub mod app;
pub mod camera;
pub mod daemon;
pub mod lifecycle;
pub mod relay;
pub mod settings;
pub mod socket;

use std::error::Error;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

/// Installs the real engines in the single engine door (PRD §9.1, decision L7).
pub fn install_engines() -> Result<(), Box<dyn Error>> {
    let metrics: Arc<dyn gx_core::text::TextMetrics> =
        Arc::new(gx_render_svg::InterMetrics::new()?);
    // The dispatcher picks the engine from the `layout_hint` of the kind, so a
    // kind never names a motor (PRD §6, O7).
    let layout: Arc<dyn gx_core::layout::LayoutEngine> =
        Arc::new(gx_layout::Layouts::new(metrics.clone()));
    let renderer: Arc<dyn gx_core::render::SvgRenderer> =
        Arc::new(gx_render_svg::SvgRenderer::new(metrics.clone()));

    gx_core::engine::install(gx_core::engine::Engines {
        layout,
        metrics,
        renderer,
    });

    // The PNG of an export is rasterized by the process that owns the window:
    // `resvg` travels behind the `png` feature so the MCP-only build does not
    // pay for it (RNF1-quater).
    gx_core::export::install_rasterizer(Arc::new(gx_render_svg::ResvgRasterizer::new()));

    // The export paints with the theme the human is looking at, and the theme
    // lives here: `gx-mcp` reads it through `gx-core` and never depends on this
    // crate (decision L7 of `f2-export.md`).
    gx_core::export::install_theme(settings::Settings::load().theme());

    Ok(())
}

/// The JSON printed by `graphium mcp-config` (PRD §8.6.1).
pub fn mcp_config_json(binary: &Path) -> String {
    gx_mcp::mcp_config_json(binary)
}

/// The configuration of this binary for a client (PRD §7, RF19).
pub fn mcp_config_for(client: &str) -> String {
    let binary = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("graphium"));
    gx_mcp::mcp_config_for(client, &binary)
}

/// Runs the application in MCP mode: the stdio server, and nothing else.
///
/// This process is a **forwarder** (S2): it owns no store, no window and no
/// socket. It looks for the owner, starts the detached daemon when nobody
/// answers, waits for the socket and relays its transport through it — so the
/// SIGTERM an MCP client sends when it stops its servers kills this process
/// and never the plan.
pub fn run_stdio() -> Result<(), Box<dyn Error>> {
    let path = socket::socket_path();

    if let socket::Instance::Forwarder = socket::probe(&path) {
        return forward_to_owner(&path);
    }

    // Nobody answers on the socket — it was missing (`Owner`) or stale
    // (`TookOver`): the daemon starts detached and this process waits for its
    // socket, so the first forwarded call does not race the bind (S2, S7).
    daemon::spawn_owner()?;
    daemon::wait_for_socket(&path, daemon::SPAWN_TIMEOUT)?;
    forward_to_owner(&path)
}

/// Runs the owner: the process that owns the store and the socket and lives in
/// the tray while it has sessions (S1-S5).
///
/// It starts with no window — the plan is a view, shown by the tray or by the
/// first diagram — and it never serves stdio: its clients arrive through the
/// socket, one connection per session.
pub fn run_owner() -> Result<(), Box<dyn Error>> {
    install_engines()?;
    gx_kinds::register_all();

    let path = socket::socket_path();
    if let socket::Instance::Forwarder = socket::probe(&path) {
        return Err(
            "ya hay un dueño del socket; `graphium quit` lo cierra antes de arrancar otro".into(),
        );
    }

    let listener = bind_socket(&path)?;
    std::thread::spawn(move || relay::serve(listener));

    let (application, state) = app::build(true, false);
    app::install_render_hook(application.handle().clone(), state);
    app::install_tray(application.handle())?;
    lifecycle::watch(application.handle().clone());

    application.run(|_handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            // The dock of macOS is the other door to a window that is only a
            // view (S8).
            app::show_main_window(_handle, true);
        }
        // Closing the last window must not end the process: the owner lives
        // while it has sessions, and the tray is where it waits (S8,
        // tauri#13511). An explicit exit carries a code and is never prevented.
        if let tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            api.prevent_exit();
        }
    });

    // An exit through the lifecycle leaves no socket behind; one that dies
    // without draining is taken over by the next owner, because `probe`
    // decides with `connect()` and not with the file (S7).
    let _ = std::fs::remove_file(&path);
    Ok(())
}

/// Runs the application in solo-ui mode: the window without an MCP server.
pub fn run_solo_ui() -> Result<(), Box<dyn Error>> {
    install_engines()?;
    gx_kinds::register_all();

    let (application, _state) = app::build(false, true);
    application.run(|_, _| {});
    Ok(())
}

/// Binds the single instance socket with owner-only permissions.
fn bind_socket(path: &Path) -> Result<UnixListener, Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Forwards the stdio of this process to the owner of the window.
fn forward_to_owner(path: &Path) -> Result<(), Box<dyn Error>> {
    let stream = UnixStream::connect(path)?;
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    relay::forward(stream, &mut input, &mut output);
    Ok(())
}

/// The help text of the binary.
pub fn help_text() -> String {
    format!(
        "graphium {}\n\n\
         Uso:\n  \
         graphium              abre la ventana (modo solo-ui)\n  \
         graphium --owner      arranca el dueño en la bandeja\n  \
         graphium --stdio      sirve MCP por stdio (lanza el dueño si no hay)\n  \
         graphium quit         cierra el dueño de forma ordenada\n  \
         graphium mcp-config   imprime la configuración MCP\n  \
         graphium --version    imprime la versión\n  \
         graphium --help       imprime esta ayuda\n",
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::{help_text, mcp_config_for, mcp_config_json};
    use std::path::Path;

    #[test]
    fn every_client_gets_its_own_configuration() {
        for client in ["pi", "claude", "cursor", "generic"] {
            let config = mcp_config_for(client);
            assert!(config.contains("--stdio"), "{client}: {config}");
            // Every shape names the command and the transport.
            assert!(config.contains("command"), "{client}: {config}");
        }
        assert!(mcp_config_for("pi").contains("[mcp_servers.graphium]"));
    }

    #[test]
    fn the_mcp_config_points_at_the_binary_in_stdio_mode() {
        let config = mcp_config_json(Path::new("/usr/local/bin/graphium"));
        let value: serde_json::Value = serde_json::from_str(&config).expect("json");
        assert_eq!(value["command"], "/usr/local/bin/graphium");
        assert_eq!(value["args"][0], "--stdio");
    }

    #[test]
    fn the_help_text_lists_the_modes() {
        let help = help_text();
        assert!(help.contains("--stdio"));
        assert!(help.contains("mcp-config"));
        assert!(help.contains("solo-ui"));
        // The owner is the process that lives in the tray (S1, S2), and
        // `quit` is the way to close it without leaving a dirty socket (S5).
        assert!(help.contains("--owner"), "{help}");
        assert!(help.contains("quit"), "{help}");
    }
}
