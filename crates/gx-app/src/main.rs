//! Graphium binary entry point (PRD §8.6.1).

use std::error::Error;

use gx_app::{books, daemon, help_text, mcp_config_for, run_owner, run_solo_ui, run_stdio, socket};

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.first().map(String::as_str) {
        None => run_solo_ui(),
        Some("--owner") => run_owner(),
        Some("--stdio") => run_stdio(),
        Some("quit") => quit(),
        Some("books") => books_command(&arguments[1..]),
        Some("mcp-config") => {
            // `graphium mcp-config [--client <pi|claude|cursor|generic>]`
            let client = match arguments.get(1).map(String::as_str) {
                Some("--client") => arguments.get(2).map(String::as_str).unwrap_or("generic"),
                Some(other) => {
                    eprintln!("graphium: unknown argument for mcp-config: {other}");
                    std::process::exit(2);
                }
                None => "generic",
            };
            println!("{}", mcp_config_for(client));
            Ok(())
        }
        Some("--version") | Some("-V") => {
            println!("graphium {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help") | Some("-h") => {
            print!("{}", help_text());
            Ok(())
        }
        Some(other) => {
            eprintln!("graphium: unknown argument: {other}");
            print!("{}", help_text());
            std::process::exit(2);
        }
    };

    if let Err(error) = result {
        eprintln!("graphium: {error}");
        std::process::exit(1);
    }
}

/// `graphium books list|prune|hook` (F9, T4/C3): the books of the user, the
/// prune of the worktrees that no longer exist, and the optional git hook.
fn books_command(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let root = gx_mcp::book::BooksRoot::from_env()
        .ok_or("no hay books root: HOME no está definido y GRAPHIUM_BOOKS_ROOT está vacío")?;
    for line in books::lines(&root, arguments)? {
        println!("{line}");
    }
    Ok(())
}

/// Asks the owner to shut down gracefully (S5).
///
/// Without an owner there is nothing to quit, and saying so is the whole
/// answer: exiting 0 would claim it closed something.
fn quit() -> Result<(), Box<dyn Error>> {
    let path = socket::socket_path();
    daemon::send_quit(&path)
        .map_err(|error| format!("no hay ningún dueño en {path:?}: {error}").into())
        .map(|()| println!("graphium: el dueño se está cerrando"))
}
