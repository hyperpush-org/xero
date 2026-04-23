//! Owns the single active cluster process. Two backends today:
//!
//! - `solana-test-validator` for `Localnet`
//! - `surfpool` (or `solana-test-validator --clone`) for `MainnetFork`
//!
//! The supervisor enforces the one-validator-at-a-time invariant. Any
//! `start(...)` call stops the previously-active cluster before spawning
//! the new one. Drop order ensures the spawned child is killed when the
//! supervisor is torn down.

pub mod process_launcher;
pub mod test_validator;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::commands::emulator::process::ChildGuard;
use crate::commands::{CommandError, CommandResult};

use super::cluster::ClusterKind;
use super::events::{ValidatorPhase, ValidatorStatusPayload};

/// Default RPC/WS ports the supervisor asks its children to listen on.
/// Matches the canonical `solana-test-validator` defaults.
pub const DEFAULT_RPC_PORT: u16 = 8899;
pub const DEFAULT_WS_PORT: u16 = 8900;

const MIN_BOOT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BOOT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartOpts {
    /// Programs to clone from the source cluster into the local fork.
    #[serde(default)]
    pub clone_programs: Vec<String>,
    /// Account addresses to clone (uses `--clone <pubkey>` / surfpool equiv.).
    #[serde(default)]
    pub clone_accounts: Vec<String>,
    /// Reset ledger on boot. Default: true for localnet, false for fork.
    #[serde(default)]
    pub reset: Option<bool>,
    /// Path to a ledger directory. If omitted, the supervisor picks one in
    /// the OS temp dir, so the same `start` call is reproducible.
    #[serde(default)]
    pub ledger_dir: Option<PathBuf>,
    /// Bind a non-default RPC port (so two windows can't collide).
    #[serde(default)]
    pub rpc_port: Option<u16>,
    /// Bind a non-default WS port.
    #[serde(default)]
    pub ws_port: Option<u16>,
    /// Boot timeout override (seconds). Clamped to [5, 120].
    #[serde(default)]
    pub boot_timeout_secs: Option<u64>,
    /// Seed personas after boot. Handled by the Persona layer in Phase 2 —
    /// the supervisor just records the intent today.
    #[serde(default)]
    pub seed_personas: bool,
    /// Optional snapshot id to restore immediately after boot.
    #[serde(default)]
    pub snapshot_id: Option<String>,
    /// Cap ledger disk usage for long sessions.
    #[serde(default)]
    pub limit_ledger: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClusterHandle {
    pub kind: ClusterKind,
    pub rpc_url: String,
    pub ws_url: String,
    pub pid: Option<u32>,
    pub ledger_dir: String,
    pub started_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClusterStatus {
    pub running: bool,
    pub kind: Option<ClusterKind>,
    pub rpc_url: Option<String>,
    pub ws_url: Option<String>,
    pub ledger_dir: Option<String>,
    pub started_at_ms: Option<u64>,
    pub uptime_s: Option<u64>,
}

impl ClusterStatus {
    pub fn idle() -> Self {
        Self {
            running: false,
            kind: None,
            rpc_url: None,
            ws_url: None,
            ledger_dir: None,
            started_at_ms: None,
            uptime_s: None,
        }
    }
}

/// A backend-agnostic session wrapping the child process plus its metadata.
pub struct ValidatorSession {
    pub kind: ClusterKind,
    pub handle: ClusterHandle,
    pub child: ChildGuard,
    pub started_at: Instant,
}

impl std::fmt::Debug for ValidatorSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidatorSession")
            .field("kind", &self.kind)
            .field("handle", &self.handle)
            .field("pid", &self.child.pid())
            .finish()
    }
}

impl ValidatorSession {
    pub fn status(&self) -> ClusterStatus {
        ClusterStatus {
            running: true,
            kind: Some(self.kind),
            rpc_url: Some(self.handle.rpc_url.clone()),
            ws_url: Some(self.handle.ws_url.clone()),
            ledger_dir: Some(self.handle.ledger_dir.clone()),
            started_at_ms: Some(self.handle.started_at_ms),
            uptime_s: Some(self.started_at.elapsed().as_secs()),
        }
    }
}

/// Inject-able launcher so unit tests can swap the real binary for a stub.
pub trait ValidatorLauncher: Send + Sync + std::fmt::Debug {
    fn launch(&self, kind: ClusterKind, opts: &StartOpts) -> CommandResult<ValidatorSession>;
}

#[derive(Debug)]
pub struct ValidatorSupervisor {
    active: Mutex<Option<ValidatorSession>>,
    launcher: Box<dyn ValidatorLauncher>,
}

impl ValidatorSupervisor {
    pub fn new(launcher: Box<dyn ValidatorLauncher>) -> Self {
        Self {
            active: Mutex::new(None),
            launcher,
        }
    }

    /// Wrap the default (`solana-test-validator` / `surfpool`) launcher. Used
    /// by the production build. Tests pass a stub instead.
    pub fn with_default_launcher() -> Self {
        Self::new(Box::new(test_validator::CliLauncher::default()))
    }

    /// Start a new cluster, replacing any currently-active one.
    pub fn start(
        &self,
        kind: ClusterKind,
        opts: StartOpts,
    ) -> CommandResult<(ClusterHandle, Vec<ValidatorStatusPayload>)> {
        if !kind.is_local() {
            return Err(CommandError::user_fixable(
                "solana_cluster_not_startable",
                format!(
                    "Cluster {} is remote-only and cannot be started locally.",
                    kind.as_str()
                ),
            ));
        }

        let mut events = Vec::new();
        events.push(ValidatorStatusPayload::new(ValidatorPhase::Stopping).with_kind(kind.as_str()));

        // Single-active invariant — drop any existing session first.
        self.stop_locked(&mut self.lock_active()?)?;

        events.push(ValidatorStatusPayload::new(ValidatorPhase::Booting).with_kind(kind.as_str()));

        let session = self.launcher.launch(kind, &opts)?;
        let handle = session.handle.clone();

        events.push(
            ValidatorStatusPayload::new(ValidatorPhase::Ready)
                .with_kind(kind.as_str())
                .with_rpc_url(&handle.rpc_url)
                .with_ws_url(&handle.ws_url),
        );

        let mut active = self.lock_active()?;
        *active = Some(session);
        Ok((handle, events))
    }

    /// Stop the active cluster (no-op when nothing is running).
    pub fn stop(&self) -> CommandResult<Vec<ValidatorStatusPayload>> {
        let mut active = self.lock_active()?;
        let mut events = Vec::new();
        if let Some(session) = active.as_ref() {
            events.push(
                ValidatorStatusPayload::new(ValidatorPhase::Stopping)
                    .with_kind(session.kind.as_str()),
            );
        }
        self.stop_locked(&mut active)?;
        events.push(ValidatorStatusPayload::new(ValidatorPhase::Stopped));
        Ok(events)
    }

    fn stop_locked(
        &self,
        active: &mut MutexGuard<'_, Option<ValidatorSession>>,
    ) -> CommandResult<()> {
        if let Some(mut session) = active.take() {
            // Give the child 1s to shut down cleanly before escalating.
            session.child.shutdown(Duration::from_secs(1));
        }
        Ok(())
    }

    pub fn status(&self) -> ClusterStatus {
        match self.active.lock() {
            Ok(active) => active
                .as_ref()
                .map(ValidatorSession::status)
                .unwrap_or_else(ClusterStatus::idle),
            Err(_) => ClusterStatus::idle(),
        }
    }

    /// Access the active session for read-only inspection (ledger dir etc.).
    pub fn with_active<R>(
        &self,
        f: impl FnOnce(Option<&ValidatorSession>) -> R,
    ) -> CommandResult<R> {
        let active = self.lock_active()?;
        Ok(f(active.as_ref()))
    }

    fn lock_active(&self) -> CommandResult<MutexGuard<'_, Option<ValidatorSession>>> {
        self.active.lock().map_err(|_| {
            CommandError::system_fault(
                "solana_supervisor_poisoned",
                "Internal validator supervisor lock was poisoned.",
            )
        })
    }
}

pub fn clamp_boot_timeout(requested_secs: Option<u64>) -> Duration {
    let default = Duration::from_secs(30);
    match requested_secs {
        None => default,
        Some(secs) => {
            let raw = Duration::from_secs(secs);
            raw.max(MIN_BOOT_TIMEOUT).min(MAX_BOOT_TIMEOUT)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::Mutex as StdMutex;

    #[derive(Debug)]
    struct ScriptedLauncher {
        calls: StdMutex<Vec<(ClusterKind, StartOpts)>>,
        fail: StdMutex<bool>,
    }

    impl ScriptedLauncher {
        fn new() -> Self {
            Self {
                calls: StdMutex::new(Vec::new()),
                fail: StdMutex::new(false),
            }
        }

        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    impl ValidatorLauncher for ScriptedLauncher {
        fn launch(&self, kind: ClusterKind, opts: &StartOpts) -> CommandResult<ValidatorSession> {
            self.calls.lock().unwrap().push((kind, opts.clone()));
            if *self.fail.lock().unwrap() {
                return Err(CommandError::system_fault(
                    "test_launch_failed",
                    "scripted failure",
                ));
            }

            let child = Command::new("sleep")
                .arg("3600")
                .spawn()
                .expect("sleep should spawn in test environment");
            let guard = ChildGuard::new("test-validator", child);

            let rpc_port = opts.rpc_port.unwrap_or(DEFAULT_RPC_PORT);
            let ws_port = opts.ws_port.unwrap_or(DEFAULT_WS_PORT);
            let ledger = opts
                .ledger_dir
                .clone()
                .unwrap_or_else(|| std::env::temp_dir().join("cadence-solana-test"));
            let handle = ClusterHandle {
                kind,
                rpc_url: format!("http://127.0.0.1:{rpc_port}"),
                ws_url: format!("ws://127.0.0.1:{ws_port}"),
                pid: guard.pid(),
                ledger_dir: ledger.display().to_string(),
                started_at_ms: 0,
            };

            Ok(ValidatorSession {
                kind,
                handle,
                child: guard,
                started_at: Instant::now(),
            })
        }
    }

    fn make_supervisor() -> (ValidatorSupervisor, std::sync::Arc<ScriptedLauncher>) {
        let launcher = std::sync::Arc::new(ScriptedLauncher::new());
        let supervisor =
            ValidatorSupervisor::new(Box::new(LauncherHandle(std::sync::Arc::clone(&launcher))));
        (supervisor, launcher)
    }

    #[derive(Debug)]
    struct LauncherHandle(std::sync::Arc<ScriptedLauncher>);
    impl ValidatorLauncher for LauncherHandle {
        fn launch(&self, kind: ClusterKind, opts: &StartOpts) -> CommandResult<ValidatorSession> {
            self.0.launch(kind, opts)
        }
    }

    #[test]
    fn remote_clusters_cannot_be_started_locally() {
        let (supervisor, launcher) = make_supervisor();
        let err = supervisor
            .start(ClusterKind::Mainnet, StartOpts::default())
            .unwrap_err();
        assert_eq!(err.code, "solana_cluster_not_startable");
        assert_eq!(launcher.call_count(), 0);
    }

    #[test]
    fn starting_cluster_replaces_the_previous_one() {
        let (supervisor, launcher) = make_supervisor();
        let (_handle1, _) = supervisor
            .start(ClusterKind::Localnet, StartOpts::default())
            .unwrap();
        assert_eq!(launcher.call_count(), 1);

        // A second start replaces the first; supervisor should still only
        // own one active session.
        let (_handle2, _) = supervisor
            .start(ClusterKind::MainnetFork, StartOpts::default())
            .unwrap();
        assert_eq!(launcher.call_count(), 2);

        let status = supervisor.status();
        assert!(status.running);
        assert_eq!(status.kind, Some(ClusterKind::MainnetFork));
    }

    #[test]
    fn stop_is_idempotent() {
        let (supervisor, _launcher) = make_supervisor();
        let _ = supervisor.stop().unwrap();
        let _ = supervisor.stop().unwrap();
        assert!(!supervisor.status().running);
    }

    #[test]
    fn status_reflects_active_session_shape() {
        let (supervisor, _launcher) = make_supervisor();
        assert!(!supervisor.status().running);

        let opts = StartOpts {
            rpc_port: Some(9999),
            ws_port: Some(9998),
            ..StartOpts::default()
        };
        let (handle, events) = supervisor.start(ClusterKind::Localnet, opts).unwrap();
        assert_eq!(handle.rpc_url, "http://127.0.0.1:9999");
        assert_eq!(handle.ws_url, "ws://127.0.0.1:9998");

        let phases: Vec<_> = events.iter().map(|e| e.phase).collect();
        assert!(phases.contains(&ValidatorPhase::Booting));
        assert!(phases.contains(&ValidatorPhase::Ready));

        let status = supervisor.status();
        assert!(status.running);
        assert_eq!(status.rpc_url.as_deref(), Some("http://127.0.0.1:9999"));
    }

    #[test]
    fn launch_failure_leaves_no_residual_session() {
        let (supervisor, launcher) = make_supervisor();
        launcher.set_fail(true);
        let err = supervisor
            .start(ClusterKind::Localnet, StartOpts::default())
            .unwrap_err();
        assert_eq!(err.code, "test_launch_failed");
        assert!(!supervisor.status().running);
    }

    #[test]
    fn clamp_boot_timeout_bounds() {
        assert_eq!(clamp_boot_timeout(None), Duration::from_secs(30));
        assert_eq!(clamp_boot_timeout(Some(0)), MIN_BOOT_TIMEOUT);
        assert_eq!(clamp_boot_timeout(Some(1)), MIN_BOOT_TIMEOUT);
        assert_eq!(clamp_boot_timeout(Some(45)), Duration::from_secs(45));
        assert_eq!(clamp_boot_timeout(Some(9_999)), MAX_BOOT_TIMEOUT);
    }
}
