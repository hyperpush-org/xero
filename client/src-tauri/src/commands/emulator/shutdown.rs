//! App-close + startup hooks.
//!
//! On **app close**, we tear down any active emulator session so the
//! emulator child / idb_companion don't outlive Xero and leak AVD locks.
//!
//! On **app startup**, we sweep leftover `emulator-*` / `idb_companion`
//! processes from previous crashes. This is intentionally best-effort: we
//! don't kill processes we can't positively identify as ours, but we log
//! anything suspicious so users can clean up manually.

use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};

use super::EmulatorState;

/// Teardown for `WindowEvent::CloseRequested`. Runs synchronously on the
/// window-event thread — the emulator session Drop impls finish in well
/// under a second.
pub fn shutdown_on_close<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<EmulatorState>() else {
        return;
    };

    // Take the active session out before dropping so we release the lock
    // while the Drop impls run (they may emit status events that try to
    // re-enter the mutex).
    let taken = {
        let mut active = state
            .active
            .lock()
            .expect("emulator active mutex poisoned on close");
        active.take()
    };
    drop(taken);

    // Same thing for the log stream.
    let log = {
        let mut slot = state
            .log_stream
            .lock()
            .expect("emulator log stream mutex poisoned on close");
        slot.take()
    };
    drop(log);

    state.frame_bus().clear();

    let _: Arc<_> = state.frame_bus(); // reuse to satisfy the type.
}

/// Best-effort scan for leftover emulator-related processes on startup.
/// We only *report* them via structured logs — the user explicitly decides
/// whether to kill them. Returns a list of suspicious PIDs and names.
pub fn zombie_processes() -> Vec<ZombieProcess> {
    #[cfg(unix)]
    {
        unix_scan_ps()
    }
    #[cfg(not(unix))]
    {
        Vec::new()
    }
}

#[derive(Debug, Clone)]
pub struct ZombieProcess {
    pub pid: u32,
    pub name: String,
}

#[cfg(unix)]
fn unix_scan_ps() -> Vec<ZombieProcess> {
    use std::process::Command;

    let output = match Command::new("ps").args(["-Ao", "pid=,comm="]).output() {
        Ok(out) if out.status.success() => out,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let my_pid = std::process::id();
    let mut out = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        let (pid_str, name) = match trimmed.split_once(' ') {
            Some(pair) => pair,
            None => continue,
        };
        let pid: u32 = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if pid == my_pid {
            continue;
        }
        let name = name.trim().to_string();
        if is_emulator_relevant(&name) {
            out.push(ZombieProcess { pid, name });
        }
    }
    out
}

#[cfg(unix)]
fn is_emulator_relevant(name: &str) -> bool {
    // Match by the binary's leaf name. `emulator` alone is too common
    // (Safari has an HTML5 emulator fork, etc.) — we require the known
    // suffixes Android emulator uses.
    let lower = name.to_ascii_lowercase();
    let leaf = lower.rsplit('/').next().unwrap_or(&lower);
    matches!(
        leaf,
        "qemu-system-aarch64"
            | "qemu-system-x86_64"
            | "idb_companion"
            | "scrcpy-server"
            | "xero-ios-helper"
    ) || leaf.starts_with("qemu-system-")
        || leaf == "emulator64-arm"
        || leaf == "emulator64-x86"
        || leaf == "emulator64-crash-service"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn is_emulator_relevant_recognizes_known_names() {
        assert!(is_emulator_relevant("qemu-system-aarch64"));
        assert!(is_emulator_relevant(
            "/opt/Android/emulator/qemu-system-x86_64"
        ));
        assert!(is_emulator_relevant("idb_companion"));
        assert!(!is_emulator_relevant("Safari"));
        assert!(!is_emulator_relevant("Xero"));
    }
}
