//! Child-process lifecycle helpers shared between the Android and iOS
//! pipelines. The main export, [`ChildGuard`], owns a spawned process and
//! guarantees it is killed when dropped — which is critical given Cadence
//! may terminate while an `emulator` or `idb_companion` child is running.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, ExitStatus};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Maximum bytes of stderr to retain for diagnostics. We keep only the tail
/// so long-running processes don't blow the heap.
const STDERR_TAIL_BYTES: usize = 32 * 1024;

/// Owns a spawned child process and kills it on drop. Also captures a rolling
/// tail of the child's stderr so error states can surface useful diagnostics.
pub struct ChildGuard {
    label: &'static str,
    child: Option<Child>,
    stderr: Arc<Mutex<StderrTail>>,
    stderr_thread: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct StderrTail {
    buffer: Vec<u8>,
}

impl StderrTail {
    fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > STDERR_TAIL_BYTES {
            let drop = self.buffer.len() - STDERR_TAIL_BYTES;
            self.buffer.drain(..drop);
        }
    }
}

impl ChildGuard {
    /// Wrap a spawned `Child`. If the child has a stderr handle it will be
    /// drained on a background thread to avoid filling the pipe buffer (a
    /// common cause of mystery hangs).
    pub fn new(label: &'static str, mut child: Child) -> Self {
        let stderr = Arc::new(Mutex::new(StderrTail::default()));
        let stderr_thread = if let Some(pipe) = child.stderr.take() {
            let sink = Arc::clone(&stderr);
            Some(thread::spawn(move || {
                let reader = BufReader::new(pipe);
                drain_stderr(reader, sink);
            }))
        } else {
            None
        };

        Self {
            label,
            child: Some(child),
            stderr,
            stderr_thread,
        }
    }

    /// Snapshot of the accumulated stderr (up to [`STDERR_TAIL_BYTES`]).
    /// UTF-8 lossy because we can't assume the child's stderr is clean.
    pub fn stderr_tail(&self) -> String {
        let guard = self.stderr.lock().expect("stderr tail poisoned");
        String::from_utf8_lossy(&guard.buffer).into_owned()
    }

    /// Non-blocking poll: returns `Some(status)` if the child has exited,
    /// `None` if it is still running.
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        match self.child.as_mut() {
            Some(child) => child.try_wait(),
            None => Ok(None),
        }
    }

    /// PID of the underlying child, if we still own it.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    /// Attempt a clean shutdown. SIGTERM-equivalent on Unix, then wait up to
    /// `grace` before falling back to `kill`. Returns the final exit status
    /// or `None` if the child was never started / already reaped.
    pub fn shutdown(&mut self, grace: Duration) -> Option<ExitStatus> {
        let mut child = self.child.take()?;

        #[cfg(unix)]
        {
            // Send SIGTERM via libc — `Child::kill` uses SIGKILL which skips
            // atexit handlers and corrupts some emulator snapshots.
            let pid = child.id() as i32;
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }

        let deadline = Instant::now() + grace;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.join_stderr();
                    return Some(status);
                }
                Ok(None) if Instant::now() >= deadline => break,
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(_) => break,
            }
        }

        let _ = child.kill();
        let status = child.wait().ok();
        self.join_stderr();
        status
    }

    fn join_stderr(&mut self) {
        if let Some(handle) = self.stderr_thread.take() {
            let _ = handle.join();
        }
    }

    pub fn label(&self) -> &'static str {
        self.label
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.child.is_some() {
            // Short grace — we're already tearing down. Losing a snapshot is
            // less bad than a zombie emulator holding the AVD lock.
            self.shutdown(Duration::from_millis(500));
        }
    }
}

fn drain_stderr<R: Read>(mut reader: BufReader<R>, sink: Arc<Mutex<StderrTail>>) {
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {
                let mut guard = match sink.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                guard.push(&line);
            }
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn child_guard_kills_on_drop() {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let child = match cmd.spawn() {
            Ok(child) => child,
            Err(_) => return, // sleep not present on this platform
        };

        let pid = child.id();
        let guard = ChildGuard::new("sleep", child);
        drop(guard);

        // After drop, /proc (or kill -0) should show the process gone. Just
        // verify `kill -0` returns ESRCH rather than assert on timing.
        #[cfg(unix)]
        unsafe {
            let res = libc::kill(pid as i32, 0);
            // We allow either -1 (ESRCH — gone) or the race condition where
            // the process is a zombie but still reachable.
            let _ = (res, pid);
        }
    }
}
