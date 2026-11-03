//! Single instance over a local socket (PRD §8.6, RNF9).
//!
//! The first process binds `$XDG_RUNTIME_DIR/graphium.sock` and owns the
//! window. A second MCP server connects to the owner and forwards its calls.
//! If the socket exists but nobody answers, the process takes it over
//! (`owner_takeover`).

use std::io;
use std::path::{Path, PathBuf};

/// Path of the single instance socket.
pub fn socket_path() -> PathBuf {
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        if !runtime.is_empty() {
            return Path::new(&runtime).join("graphium.sock");
        }
    }
    let uid = libc_getuid();
    std::env::temp_dir().join(format!("graphium-{uid}.sock"))
}

/// The user id of the process, without pulling in a dependency.
fn libc_getuid() -> u32 {
    // `/proc/self/status` is available on Linux, the only supported platform
    // for F0 (PRD RNF2).
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("Uid:") {
                if let Some(first) = rest.split_whitespace().next() {
                    if let Ok(uid) = first.parse::<u32>() {
                        return uid;
                    }
                }
            }
        }
    }
    0
}

/// What happened when the process looked for an existing instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instance {
    /// Nobody was running: this process binds the socket.
    Owner,
    /// Another process owns the socket: this process forwards to it.
    Forwarder,
    /// The socket was stale: this process took it over.
    TookOver,
}

/// Removes a stale socket file.
pub fn take_over(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Decides the role of this process against an existing socket.
pub fn probe(path: &Path) -> Instance {
    if !path.exists() {
        return Instance::Owner;
    }
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(_) => Instance::Forwarder,
        Err(_) => {
            let _ = take_over(path);
            Instance::TookOver
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{probe, socket_path, Instance};
    use std::os::unix::net::UnixListener;

    #[test]
    fn the_socket_path_ends_with_graphium_sock() {
        assert!(socket_path().ends_with("graphium.sock"));
    }

    #[test]
    fn a_missing_socket_means_this_process_owns_it() {
        let dir = std::env::temp_dir().join(format!("graphium-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("graphium.sock");
        let _ = std::fs::remove_file(&path);
        assert_eq!(probe(&path), Instance::Owner);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_live_socket_means_this_process_forwards() {
        let dir = std::env::temp_dir().join(format!("graphium-test-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("graphium.sock");
        let _ = std::fs::remove_file(&path);
        let _listener = UnixListener::bind(&path).expect("bind");
        assert_eq!(probe(&path), Instance::Forwarder);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_stale_socket_is_taken_over() {
        let dir = std::env::temp_dir().join(format!("graphium-test-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("graphium.sock");
        std::fs::write(&path, b"").expect("write");
        assert_eq!(probe(&path), Instance::TookOver);
        assert!(!path.exists(), "the stale socket must be removed");
        std::fs::remove_dir_all(&dir).ok();
    }
}
