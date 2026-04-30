//! Test helpers for the Solana integration tests. Swaps in a fake launcher
//! so we never spawn a real validator in CI, and a mock account fetcher
//! for the snapshot round-trip.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use xero_desktop_lib::commands::solana::persona::fund::{
    FundingBackend, FundingContext, FundingStep,
};
use xero_desktop_lib::commands::solana::persona::keygen::{KeypairBytes, KeypairProvider};
use xero_desktop_lib::commands::solana::{
    AccountFetcher, AccountRecord, ClusterHandle, ClusterKind, KeypairStore, PersonaStore,
    RpcRouter, SnapshotStore, SolanaState, StartOpts, ValidatorLauncher, ValidatorSession,
    ValidatorSupervisor,
};
use xero_desktop_lib::commands::CommandResult;

pub use xero_desktop_lib::commands::solana::rpc_router::{EndpointSpec, RpcHealthCheck};
pub use xero_desktop_lib::commands::CommandError;

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
        let guard =
            xero_desktop_lib::commands::emulator::process::ChildGuard::new("test-validator", child);

        let ledger = opts
            .ledger_dir
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("xero-solana-int-test"));
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
    #[allow(dead_code)]
    pub _personas_dir: Option<TempDir>,
    pub funding: Option<Arc<TestFundingBackend>>,
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
        _personas_dir: None,
        funding: None,
    }
}

/// Fixture variant that installs a sandboxed `PersonaStore` with a mock
/// funding backend so Phase 2 integration tests can drive create/fund/run
/// flows without touching the host filesystem or the network.
pub fn fixture_with_persona_store() -> FixtureState {
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

    let personas_dir = TempDir::new().expect("persona tempdir");
    let funding = Arc::new(TestFundingBackend::new());
    let keypairs = KeypairStore::new(
        personas_dir.path().join("keypairs"),
        Box::new(DeterministicKeypairProvider::new()),
    );
    let personas = Arc::new(PersonaStore::new(
        personas_dir.path().to_path_buf(),
        keypairs,
        Box::new(FundingBackendHandle(Arc::clone(&funding))),
    ));

    let state = SolanaState::with_personas(
        Arc::clone(&supervisor),
        Arc::clone(&router),
        Arc::clone(&snapshots),
        personas,
    );

    FixtureState {
        state,
        fetcher,
        launcher,
        router,
        _snapshots_dir: snapshots_dir,
        _personas_dir: Some(personas_dir),
        funding: Some(funding),
    }
}

#[derive(Debug)]
pub struct LauncherHandle(pub Arc<ScriptedLauncher>);
impl ValidatorLauncher for LauncherHandle {
    fn launch(&self, kind: ClusterKind, opts: &StartOpts) -> CommandResult<ValidatorSession> {
        self.0.launch(kind, opts)
    }
}

/// Deterministic keypair provider so integration tests produce stable
/// pubkeys across runs. The byte pattern is the same trick `persona::
/// keygen::test_support::DeterministicProvider` uses — an ed25519 signing
/// key seeded with a counter value.
#[derive(Debug)]
pub struct DeterministicKeypairProvider {
    counter: Mutex<u8>,
}

impl DeterministicKeypairProvider {
    pub fn new() -> Self {
        Self {
            counter: Mutex::new(0),
        }
    }
}

impl KeypairProvider for DeterministicKeypairProvider {
    fn generate(&self) -> CommandResult<KeypairBytes> {
        use ed25519_dalek::SigningKey;
        let mut guard = self.counter.lock().unwrap();
        *guard = guard.wrapping_add(1);
        let n = *guard;
        drop(guard);

        let seed = [n; 32];
        let signing = SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(&seed);
        out[32..].copy_from_slice(verifying.as_bytes());
        Ok(KeypairBytes(out))
    }
}

/// Mock funding backend used by the Phase 2 integration fixture. Records
/// every call (with persona name, mint, amount, NFT index) so the test
/// can assert on the shape of the funding pipeline without running a
/// real validator.
#[derive(Debug, Default)]
pub struct TestFundingBackend {
    pub airdrops: Mutex<Vec<(String, u64)>>,
    pub tokens: Mutex<Vec<(String, String, u64)>>,
    pub nfts: Mutex<Vec<(String, String, u32)>>,
}

impl TestFundingBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn airdrop_count(&self) -> usize {
        self.airdrops.lock().unwrap().len()
    }

    pub fn token_count(&self) -> usize {
        self.tokens.lock().unwrap().len()
    }

    pub fn nft_count(&self) -> usize {
        self.nfts.lock().unwrap().len()
    }
}

impl FundingBackend for TestFundingBackend {
    fn airdrop(&self, ctx: &FundingContext, lamports: u64) -> CommandResult<FundingStep> {
        self.airdrops
            .lock()
            .unwrap()
            .push((ctx.persona_name.clone(), lamports));
        Ok(FundingStep::Airdrop {
            signature: Some(format!("sig-airdrop-{}", ctx.persona_name)),
            lamports,
            ok: true,
            error: None,
        })
    }

    fn ensure_token_balance(
        &self,
        ctx: &FundingContext,
        mint: &str,
        amount: u64,
        _authority_keypair_path: Option<&Path>,
    ) -> CommandResult<FundingStep> {
        self.tokens
            .lock()
            .unwrap()
            .push((ctx.persona_name.clone(), mint.to_string(), amount));
        Ok(FundingStep::TokenMint {
            mint: mint.to_string(),
            amount,
            signature: Some(format!("sig-token-{}-{}", mint, ctx.persona_name)),
            ok: true,
            error: None,
        })
    }

    fn mint_nft_fixture(
        &self,
        ctx: &FundingContext,
        collection: &str,
        index: u32,
    ) -> CommandResult<FundingStep> {
        self.nfts
            .lock()
            .unwrap()
            .push((ctx.persona_name.clone(), collection.to_string(), index));
        Ok(FundingStep::NftFixture {
            collection: collection.to_string(),
            mint: Some(format!(
                "mock-nft-{}-{}-{}",
                ctx.persona_name, collection, index
            )),
            signature: Some(format!(
                "sig-nft-{}-{}-{}",
                ctx.persona_name, collection, index
            )),
            ok: true,
            error: None,
        })
    }
}

/// Newtype so the `TestFundingBackend` can be shared with the fixture
/// caller *and* handed to the `PersonaStore` as a `Box<dyn FundingBackend>`.
#[derive(Debug)]
pub struct FundingBackendHandle(pub Arc<TestFundingBackend>);

impl FundingBackend for FundingBackendHandle {
    fn airdrop(&self, ctx: &FundingContext, lamports: u64) -> CommandResult<FundingStep> {
        self.0.airdrop(ctx, lamports)
    }

    fn ensure_token_balance(
        &self,
        ctx: &FundingContext,
        mint: &str,
        amount: u64,
        authority: Option<&Path>,
    ) -> CommandResult<FundingStep> {
        self.0.ensure_token_balance(ctx, mint, amount, authority)
    }

    fn mint_nft_fixture(
        &self,
        ctx: &FundingContext,
        collection: &str,
        index: u32,
    ) -> CommandResult<FundingStep> {
        self.0.mint_nft_fixture(ctx, collection, index)
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
