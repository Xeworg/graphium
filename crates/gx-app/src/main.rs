//! Graphium binary entry point (PRD §8.6.1).

use gx_app::{help_text, mcp_config_for, run_solo_ui, run_stdio};

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.first().map(String::as_str) {
        None => run_solo_ui(),
        Some("--stdio") => run_stdio(),
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
