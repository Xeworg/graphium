//! `gx-app` — the Graphium application.
//!
//! This crate is the composition root: it installs the engines, opens the
//! plan window, owns the single instance socket and serves the MCP client
//! (PRD §8.6, §8.6.1).

pub mod app;
pub mod camera;
pub mod relay;
pub mod socket;

use std::error::Error;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

/// Installs the real engines in the single engine door (PRD §9.1, decision L7).
pub fn install_engines() -> Result<(), Box<dyn Error>> {
    let metrics: Arc<dyn gx_core::text::TextMetrics> =
        Arc::new(gx_render_svg::InterMetrics::new()?);
    let layout: Arc<dyn gx_core::layout::LayoutEngine> =
        Arc::new(gx_layout::DagreEngine::new(metrics.clone()));
    let renderer: Arc<dyn gx_core::render::SvgRenderer> =
        Arc::new(gx_render_svg::SvgRenderer::new(metrics.clone()));

    gx_core::engine::install(gx_core::engine::Engines {
        layout,
        metrics,
        renderer,
    });
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

/// Runs the application in MCP mode: the window plus the stdio server.
///
/// When another process already owns the window, this one forwards its stdio
/// to the owner (PRD §8.6).
pub fn run_stdio() -> Result<(), Box<dyn Error>> {
    let path = socket::socket_path();

    if let socket::Instance::Forwarder = socket::probe(&path) {
        return forward_to_owner(&path);
    }

    install_engines()?;
    gx_kinds::register_all();

    let listener = bind_socket(&path)?;
    std::thread::spawn(move || relay::serve(listener));

    let (application, state) = app::build(true);
    app::install_render_hook(application.handle().clone(), state);

    // The stdio transport lives in its own thread with its own runtime, so
    // the blocking read of stdin stays out of the window loop (PRD §8.6.1).
    std::thread::spawn(|| {
        if let Err(error) = gx_mcp::serve_stdio() {
            eprintln!("graphium: the MCP server stopped: {error}");
        }
    });

    application.run(|_, _| {});
    Ok(())
}

/// Runs the application in solo-ui mode: the window without an MCP server.
pub fn run_solo_ui() -> Result<(), Box<dyn Error>> {
    install_engines()?;
    gx_kinds::register_all();

    let (application, _state) = app::build(false);
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
         graphium --stdio      sirve MCP por stdio\n  \
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
    }
}
