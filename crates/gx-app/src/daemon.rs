//! The detached owner process (PRD §8.6, corte f5-background).
//!
//! The daemon owns the store, the socket and the window. The `--stdio`
//! process spawns it detached — `process_group(0)` and stdio to `/dev/null`
//! — so the SIGTERM/SIGKILL the MCP client sends when it stops its servers
//! kills only the forwarder, never the owner. The owner answers on the
//! single-instance socket, the same one a second MCP server would use.

use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The control line the owner reads as a graceful shutdown order.
pub const QUIT_LINE: &str = "graphium-quit";

/// How long the forwarder waits for the daemon to open its socket.
pub const SPAWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Spawns the owner detached: its own process group, stdio to `/dev/null`.
///
/// Detached is not cosmetic: the process group is what separates the daemon
/// from the group of the MCP client, and `/dev/null` is what keeps a dead
/// stdout from blocking a `writeln!` of the Tauri loop.
pub fn spawn_owner() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let devnull = || std::fs::File::open("/dev/null");
    Command::new(exe)
        .arg("--owner")
        .stdin(Stdio::from(devnull()?))
        .stdout(Stdio::from(devnull()?))
        .stderr(Stdio::from(devnull()?))
        // Its own group: the SIGTERM of the MCP client does not reach it.
        .process_group(0)
        .spawn()?;
    Ok(())
}

/// Waits until the socket accepts a connection, so the first forward does
/// not race the daemon's bind.
pub fn wait_for_socket(path: &Path, timeout: Duration) -> io::Result<()> {
    let start = Instant::now();
    loop {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "el daemon no abrió el socket a tiempo",
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Asks the owner to shut down gracefully (the `graphium quit` command).
pub fn send_quit(path: &Path) -> io::Result<()> {
    use std::io::Write;
    let mut stream = std::os::unix::net::UnixStream::connect(path)?;
    writeln!(stream, "{QUIT_LINE}")?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quit_line_is_a_plain_marker() {
        // It must not be parseable as JSON-RPC: the relay checks it before
        // the request path, and a valid JSON would be ambiguous.
        assert!(serde_json::from_str::<serde_json::Value>(QUIT_LINE).is_err());
    }

    #[test]
    fn wait_for_socket_times_out_without_a_daemon() {
        let dir = std::env::temp_dir().join(format!("graphium-daemon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("nothing.sock");
        let _ = std::fs::remove_file(&path);
        let started = Instant::now();
        assert!(wait_for_socket(&path, Duration::from_millis(120)).is_err());
        assert!(started.elapsed() >= Duration::from_millis(120));
        std::fs::remove_dir_all(&dir).ok();
    }
}
