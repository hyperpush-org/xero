//! Test helpers for the Solana integration tests. Swaps in a fake launcher
//! so we never spawn a real validator in CI, and a mock account fetcher
//! for the snapshot round-trip.

use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cadence_desktop_lib::commands::solana::{
    AccountFetcher, AccountRecord, ClusterHandle, ClusterKind, RpcRouter, SnapshotStore,
    SolanaState, StartOpts, ValidatorLauncher, ValidatorSession, ValidatorSupervisor,
};
use cadence_desktop_lib::commands::CommandResult;

pub use cadence_desktop_lib::commands::solana::rpc_router::{EndpointSpec, RpcHealthCheck};
pub use cadence_desktop_lib::commands::CommandError;

pub use tempfile::TempDir;

/// Simple account fetcher seeded with a canned key → record map. Shared
/// between snapshot create/restore calls via an `Arc`.
#[derive(Debug, Default)]
pub struct MockAccountFetcher {
    pub records: Mutex<HashMap<String, AccountRecord>>,
}

impl MockAccountFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, record: AccountRecord) {
        self.records
            .lock()
            .unwrap()
            .insert(record.pubkey.clone(), record);
    }

    pub fn mutate(&self, pubkey: &str, mutate: impl FnOnce(&mut AccountRecord)) {
        let mut map = self.records.lock().unwrap();
        if let Some(record) = map.get_mut(pubkey) {
            mutate(record);
        }
    }
}

impl AccountFetcher for MockAccountFetcher {
    fn fetch(&self, _rpc_url: &str, pubkeys: &[String]) -> Result<Vec<AccountRecord>, String> {
        let map = self.records.lock().unwrap();
        Ok(pubkeys.iter().filter_map(|p| map.get(p).cloned()).collect())
    }
}

/// Fetcher handle so the Arc can outlive the SnapshotStore's boxed trait.
#[derive(Debug)]
pub struct FetcherHandle(pub Arc<MockAccountFetcher>);

impl AccountFetcher for FetcherHandle {
    fn fetch(&self, rpc_url: &str, pubkeys: &[String]) -> Result<Vec<AccountRecord>, String> {
        self.0.fetch(rpc_url, pubkeys)
    }
}

/// Validator launcher that spawns a short-lived `sleep` process instead of
/// a real validator. Lets the supervisor test its single-active-cluster
/// invariant without dragging in the Solana CLI.
#[derive(Debug)]
pub struct ScriptedLauncher {
    calls: Mutex<Vec<(ClusterKind, StartOpts)>>,
    fail: Mutex<bool>,
}

impl ScriptedLauncher {
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            fail: Mutex::new(false),
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    #[allow(dead_code)]
    pub fn set_fail(&self, fail: bool) {
        *self.fail.lock().unwrap() = fail;
    }
}

impl ValidatorLauncher for ScriptedLauncher {
    fn launch(&self, kind: ClusterKind, opts: &StartOpts) -> CommandResult<ValidatorSession> {
        self.calls.lock().unwrap().push((kind, opts.clone()));
        if *self.fail.lock().unwrap() {
            return Err(CommandError::system_fault(
                "scripted_launch_failed",
                "Scripted launcher was configured to fail.",
            ));
        }

        let child = Command::new("sleep")
            .arg("3600")
            .spawn()
            .expect("sleep should spawn in test environment");
        let guard = cadence_desktop_lib::commands::emulator::process::ChildGuard::new(
            "test-validator",
            child,
        );

        let ledger = opts
            .ledger_dir
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("cadence-solana-int-test"));
        let handle = ClusterHandle {
            kind,
            rpc_url: "http://127.0.0.1:8899".into(),
            ws_url: "ws://127.0.0.1:8900".into(),
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

/// Scripted health check used for the RPC failover tests. Thread-safe so
/// tests can flip individual endpoint outcomes mid-run.
#[derive(Debug, Default)]
pub struct ScriptedHealthCheck {
    responses: Mutex<HashMap<String, Result<(), String>>>,
}

impl ScriptedHealthCheck {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, url: &str, outcome: Result<(), String>) {
        self.responses
            .lock()
            .unwrap()
            .insert(url.to_string(), outcome);
    }
}

impl RpcHealthCheck for ScriptedHealthCheck {
    fn check(&self, url: &str) -> Result<(), String> {
        self.responses
            .lock()
            .unwrap()
            .get(url)
            .cloned()
            .unwrap_or_else(|| Err("not scripted".into()))
    }
}

#[derive(Debug)]
pub struct ScriptedHealthCheckHandle(pub Arc<ScriptedHealthCheck>);

impl RpcHealthCheck for ScriptedHealthCheckHandle {
    fn check(&self, url: &str) -> Result<(), String> {
        self.0.check(url)
    }
}

pub struct FixtureState {
    pub state: SolanaState,
    pub fetcher: Arc<MockAccountFetcher>,
    pub launcher: Arc<ScriptedLauncher>,
    #[allow(dead_code)]
    pub router: Arc<RpcRouter>,
    pub _snapshots_dir: TempDir,
}

pub fn fixture_with_scripted_launcher_and_fetcher() -> FixtureState {
    let launcher = Arc::new(ScriptedLauncher::new());
    let supervisor = Arc::new(ValidatorSupervisor::new(Box::new(LauncherHandle(
        Arc::clone(&launcher),
    ))));

    let router = Arc::new(RpcRouter::new_with_default_pool());

    let fetcher = Arc::new(MockAccountFetcher::new());
    let snapshots_dir = TempDir::new().expect("tempdir");
    let snapshots = Arc::new(SnapshotStore::new(
        snapshots_dir.path().to_path_buf(),
        Box::new(FetcherHandle(Arc::clone(&fetcher))),
    ));

    let state = SolanaState::new(
        Arc::clone(&supervisor),
        Arc::clone(&router),
        Arc::clone(&snapshots),
    );

    FixtureState {
        state,
        fetcher,
        launcher,
        router,
        _snapshots_dir: snapshots_dir,
    }
}

#[derive(Debug)]
pub struct LauncherHandle(pub Arc<ScriptedLauncher>);
impl ValidatorLauncher for LauncherHandle {
    fn launch(&self, kind: ClusterKind, opts: &StartOpts) -> CommandResult<ValidatorSession> {
        self.0.launch(kind, opts)
    }
}

pub fn sample_record(pubkey: &str, lamports: u64) -> AccountRecord {
    use base64::Engine as _;
    AccountRecord {
        pubkey: pubkey.into(),
        lamports,
        owner: "11111111111111111111111111111111".into(),
        executable: false,
        rent_epoch: 0,
        data_base64: base64::engine::general_purpose::STANDARD.encode([0xde, 0xad]),
    }
}
