//! Graphium binary entry point (PRD §8.6.1).

use gx_app::{help_text, mcp_config, run_solo_ui, run_stdio};

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.first().map(String::as_str) {
        None => run_solo_ui(),
        Some("--stdio") => run_stdio(),
        Some("mcp-config") => {
            println!("{}", mcp_config());
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
