//! `idb_companion` sidecar lifecycle.
//!
//! The companion is a long-running macOS binary that owns the private
//! `CoreSimulator`/`IndigoHID` surface. We spawn it with a specific UDID
//! targeting the user-selected simulator, on a local TCP port we allocate.
//! Once it's accepting connections the gRPC client in `idb_client.rs`
//! speaks to it over the port.

use std::io::Result;
use std::net::{SocketAddrV4, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::commands::emulator::process::ChildGuard;

/// Launch configuration for `idb_companion`.
pub struct Launch {
    pub binary: PathBuf,
    pub udid: String,
    pub grpc_port: u16,
}

impl Launch {
    pub fn new(binary: impl Into<PathBuf>, udid: impl Into<String>) -> Result<Self> {
        Ok(Self {
            binary: binary.into(),
            udid: udid.into(),
            grpc_port: pick_free_port()?,
        })
    }
}

/// Running companion instance. Drop to terminate.
pub struct Companion {
    pub grpc_port: u16,
    pub guard: ChildGuard,
}

/// Spawn idb_companion and wait for it to start accepting connections.
pub fn spawn(launch: Launch, startup_timeout: Duration) -> Result<Companion> {
    let mut cmd = Command::new(&launch.binary);
    cmd.args([
        "--udid",
        &launch.udid,
        "--grpc-port",
        &launch.grpc_port.to_string(),
        "--log-level",
        "warn",
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::piped());

    let child = cmd.spawn()?;
    let mut guard = ChildGuard::new("idb-companion", child);

    let deadline = Instant::now() + startup_timeout;
    loop {
        match guard.try_wait() {
            Ok(Some(status)) => {
                let tail = guard.stderr_tail();
                return Err(std::io::Error::other(
                    format!(
                        "idb_companion exited before accepting connections (status={status}). stderr: {tail}"
                    ),
                ));
            }
            Ok(None) if Instant::now() >= deadline => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "idb_companion did not start listening on 127.0.0.1:{} within {:?}",
                        launch.grpc_port, startup_timeout
                    ),
                ));
            }
            Ok(None) => {}
            Err(err) => return Err(err),
        }

        if TcpStream::connect_timeout(
            &std::net::SocketAddr::V4(SocketAddrV4::new([127, 0, 0, 1].into(), launch.grpc_port)),
            Duration::from_millis(200),
        )
        .is_ok()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
    }

    Ok(Companion {
        grpc_port: launch.grpc_port,
        guard,
    })
}

/// Verify that `path` exists and is marked executable. Best-effort — returns
/// the path unchanged when it already looks correct, and attempts a `chmod`
/// if it's missing the execute bit (common when users grabbed the binary
/// out of a zip without unzipping preserve-attrs).
pub fn ensure_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(path)?;
        let mut perms = metadata.permissions();
        let mode = perms.mode();
        if mode & 0o111 == 0 {
            perms.set_mode(mode | 0o755);
            std::fs::set_permissions(path, perms)?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn pick_free_port() -> Result<u16> {
    use std::net::TcpListener;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}
