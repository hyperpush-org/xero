//! Solana workbench backend — Phase 1.
//!
//! Mirrors the `emulator` module layout: a single `SolanaState` held as
//! Tauri state, a narrow set of JSON-in/JSON-out commands, and events
//! emitted onto well-known channel names. Everything is designed so a
//! future autonomous-runtime tool wrapper can drive the same surface that
//! the UI drives.

pub mod audit;
pub mod cluster;
pub mod events;
pub mod idl;
pub mod pda;
pub mod persona;
pub mod program;
pub mod rpc_router;
pub mod scenario;
pub mod snapshot;
pub mod toolchain;
pub mod tx;
pub mod validator;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::commands::{CommandError, CommandResult};

pub use audit::{
    coverage::{CoverageReport, CoverageRequest, FunctionCoverage, InstructionCoverage, LcovRecord},
    replay::{ExploitDescriptor, ExploitKey, ReplayOutcome, ReplayReport, ReplayRequest, ReplayStep},
    sec3::{AnalyzerKind, ExternalAnalyzerReport, ExternalAnalyzerRequest},
    static_lints::{AnchorFinding, StaticLintReport, StaticLintRequest, StaticLintRule},
    trident::{FuzzCrash, FuzzReport, FuzzRequest, TridentHarnessRequest, TridentHarnessResult},
    AuditEngine, AuditEventPayload, AuditEventPhase, AuditEventSink, AuditRunKind, Finding,
    FindingSeverity, FindingSource, NullAuditEventSink, SeverityCounts,
};
pub use cluster::{descriptors as cluster_descriptors, ClusterDescriptor, ClusterKind};
pub use events::{
    PersonaEventKind, PersonaEventPayload, ScenarioEventKind, ScenarioEventPayload, TxEventKind,
    TxEventPayload, ValidatorLogLevel, ValidatorLogPayload, ValidatorPhase, ValidatorStatusPayload,
    SOLANA_AUDIT_EVENT, SOLANA_DEPLOY_PROGRESS_EVENT, SOLANA_IDL_CHANGED_EVENT,
    SOLANA_PERSONA_EVENT, SOLANA_RPC_HEALTH_EVENT, SOLANA_SCENARIO_EVENT,
    SOLANA_TOOLCHAIN_STATUS_CHANGED_EVENT, SOLANA_TX_EVENT, SOLANA_VALIDATOR_LOG_EVENT,
    SOLANA_VALIDATOR_STATUS_EVENT,
};
pub use idl::{
    codama::{CodamaGenerationReport, CodamaGenerationRequest, CodamaTarget, CodamaTargetResult},
    publish::{
        DeployProgressPayload, DeployProgressPhase, DeployProgressSink, IdlPublishMode,
        IdlPublishReport, IdlPublishRequest, NullProgressSink,
    },
    DriftChange, DriftReport, DriftSeverity, FetchedIdl, Idl, IdlChangePhase, IdlChangedEvent,
    IdlEventSink, IdlFetcher, IdlRegistry, IdlSource, IdlSubscriptionToken, RpcIdlFetcher,
};
pub use pda::{
    analyse_bump, predict_cross_cluster, scan_project as pda_scan_project, BumpAnalysis,
    ClusterPda, DerivedAddress, PdaSite, PdaSiteSeedKind, SeedPart,
};
pub use program::{
    build as program_build, deploy as program_deploy, rollback as program_rollback,
    squads_synthesize, upgrade_safety_check, verified_build_submit, ArchiveRecord,
    AuthorityCheck, AuthorityCheckOutcome, BufferWriteOutcome, BuildKind, BuildProfile,
    BuildReport, BuildRequest, BuildRunner, BuiltArtifact, DeployAuthority, DeployResult,
    DeployRunner, DeployServices, DeploySpec, DirectDeployOutcome, LayoutCheck,
    PostDeployOptions, RollbackRequest, RollbackResult, SizeCheck, SizeCheckOutcome,
    SquadsProposalDescriptor, SquadsProposalRequest, SystemBuildRunner, SystemDeployRunner,
    SystemVerifiedBuildRunner, UpgradeInstruction, UpgradeInstructionAccount,
    UpgradeSafetyReport, UpgradeSafetyRequest, UpgradeSafetyVerdict, VerifiedBuildRequest,
    VerifiedBuildResult, VerifiedBuildRunner, BPF_UPGRADEABLE_LOADER, DEFAULT_VAULT_INDEX,
    PROGRAM_DATA_MAX_BYTES, SQUADS_V4_PROGRAM_ID,
};
pub use persona::fund::{
    DefaultFundingBackend, FundingBackend, FundingDelta, FundingReceipt, FundingStep,
};
pub use persona::keygen::{KeypairBytes, KeypairStore, OsRngKeypairProvider};
pub use persona::roles::{
    descriptors as persona_role_descriptors, mint_for_symbol, NftAllocation, PersonaRole,
    RoleDescriptor, RolePreset, TokenAllocation,
};
pub use persona::{Persona, PersonaSpec, PersonaStore};
pub use rpc_router::{EndpointHealth, EndpointSpec, RpcRouter};
pub use scenario::{
    scenarios as scenario_descriptors, ScenarioDescriptor, ScenarioEngine, ScenarioKind,
    ScenarioRun, ScenarioSpec, ScenarioStatus,
};
pub use snapshot::{
    AccountFetcher, AccountRecord, RpcAccountFetcher, SnapshotManifest, SnapshotMeta, SnapshotStore,
};
pub use toolchain::{ToolProbe, ToolchainStatus};
pub use tx::{
    AccountMetaSpec, AltCandidate, AltCreateResult, AltExtendResult, AltResolveReport, AltRunner,
    BundleStatus, BundleSubmission, Commitment, CompiledComputeInstruction, ComputeBudgetPlan,
    CpiResolution, DecodedLogs, ExplainRequest, Explanation, FeeEstimate, FeeSample,
    HttpRpcTransport, IdlErrorMap, KnownProgramLookup, LandingStrategy, PercentileFee, ResolveArgs,
    RpcTransport, SamplePercentile, SendRequest, SimulateRequest, SimulationResult, TxPipeline,
    TxPlan, TxResult, TxSpec,
};
pub use validator::{
    ClusterHandle, ClusterStatus, StartOpts, ValidatorLauncher, ValidatorSession,
    ValidatorSupervisor,
};

/// Process-wide Solana state. Registered alongside `EmulatorState` in the
/// Tauri builder.
pub struct SolanaState {
    supervisor: Arc<ValidatorSupervisor>,
    rpc_router: Arc<RpcRouter>,
    snapshots: Arc<SnapshotStore>,
    personas: Arc<PersonaStore>,
    scenarios: Arc<ScenarioEngine>,
    tx_pipeline: Arc<TxPipeline>,
    idl_registry: Arc<IdlRegistry>,
    /// Shared RPC transport — also fed into the IDL registry and the
    /// upgrade-safety checker so all Phase 5 RPC reads go through the
    /// same client (and therefore the same scripted transport in
    /// integration tests).
    transport: Arc<dyn RpcTransport>,
    /// Deploy services (system runners by default). Tests can swap
    /// these out via `with_deploy_services` to script `solana program
    /// ...`, `anchor idl ...`, and `codama` invocations.
    deploy_services: Arc<DeployServices>,
    /// Phase 6 — audit engine. Tests inject scripted runners via
    /// `with_audit_engine`.
    audit_engine: Arc<AuditEngine>,
}

fn build_tx_pipeline(
    supervisor: &Arc<ValidatorSupervisor>,
    router: &Arc<RpcRouter>,
    personas: &Arc<PersonaStore>,
) -> (Arc<TxPipeline>, Arc<dyn RpcTransport>) {
    let transport: Arc<dyn RpcTransport> = Arc::new(HttpRpcTransport::new());
    let alt_runner: Arc<dyn AltRunner> = Arc::new(tx::alt::SolanaCliRunner::new());
    (
        Arc::new(TxPipeline::new(
            Arc::clone(&transport),
            Arc::clone(router),
            Arc::clone(personas),
            Arc::clone(supervisor),
            alt_runner,
        )),
        transport,
    )
}

fn build_idl_registry(transport: Arc<dyn RpcTransport>) -> Arc<IdlRegistry> {
    let fetcher: Arc<dyn IdlFetcher> = Arc::new(RpcIdlFetcher::new(transport));
    Arc::new(IdlRegistry::new(fetcher))
}

#[derive(Debug)]
struct NoopIdlFetcher;

impl IdlFetcher for NoopIdlFetcher {
    fn fetch(
        &self,
        _cluster: ClusterKind,
        _rpc_url: &str,
        _program_id: &str,
    ) -> CommandResult<Option<FetchedIdl>> {
        Ok(None)
    }
}

impl Default for SolanaState {
    fn default() -> Self {
        let snapshots = SnapshotStore::with_default_root(Box::new(RpcAccountFetcher))
            .unwrap_or_else(|_| {
                // Fall back to an in-temp scratch dir if the OS data dir
                // can't be resolved so the app still boots.
                let scratch = std::env::temp_dir().join("cadence-solana-snapshots");
                SnapshotStore::new(scratch, Box::new(RpcAccountFetcher))
            });
        let supervisor = Arc::new(ValidatorSupervisor::with_default_launcher());
        let personas = PersonaStore::with_default_root().unwrap_or_else(|_| {
            // Same fallback reasoning as snapshots: never block the app
            // from booting because the OS data dir is missing.
            let scratch = std::env::temp_dir().join("cadence-solana-personas");
            let keypairs =
                KeypairStore::new(scratch.join("keypairs"), Box::new(OsRngKeypairProvider));
            PersonaStore::new(scratch, keypairs, Box::new(DefaultFundingBackend::new()))
        });
        let personas = Arc::new(personas);
        let scenarios = Arc::new(ScenarioEngine::new(
            Arc::clone(&personas),
            Arc::clone(&supervisor),
        ));
        let rpc_router = Arc::new(RpcRouter::new_with_default_pool());
        let (tx_pipeline, transport) = build_tx_pipeline(&supervisor, &rpc_router, &personas);
        let idl_registry = build_idl_registry(Arc::clone(&transport));
        Self {
            supervisor,
            rpc_router,
            snapshots: Arc::new(snapshots),
            personas,
            scenarios,
            tx_pipeline,
            idl_registry,
            transport,
            deploy_services: Arc::new(DeployServices::system()),
            audit_engine: Arc::new(AuditEngine::system()),
        }
    }
}

impl SolanaState {
    pub fn new(
        supervisor: Arc<ValidatorSupervisor>,
        rpc_router: Arc<RpcRouter>,
        snapshots: Arc<SnapshotStore>,
    ) -> Self {
        let personas = PersonaStore::with_default_root().unwrap_or_else(|_| {
            let scratch = std::env::temp_dir().join("cadence-solana-personas-test");
            let keypairs =
                KeypairStore::new(scratch.join("keypairs"), Box::new(OsRngKeypairProvider));
            PersonaStore::new(scratch, keypairs, Box::new(DefaultFundingBackend::new()))
        });
        let personas = Arc::new(personas);
        let scenarios = Arc::new(ScenarioEngine::new(
            Arc::clone(&personas),
            Arc::clone(&supervisor),
        ));
        let (tx_pipeline, transport) = build_tx_pipeline(&supervisor, &rpc_router, &personas);
        let idl_registry = build_idl_registry(Arc::clone(&transport));
        Self {
            supervisor,
            rpc_router,
            snapshots,
            personas,
            scenarios,
            tx_pipeline,
            idl_registry,
            transport,
            deploy_services: Arc::new(DeployServices::system()),
            audit_engine: Arc::new(AuditEngine::system()),
        }
    }

    /// Explicit constructor for tests that want to inject a persona store
    /// (and a scenario engine wired to it) instead of the default one.
    pub fn with_personas(
        supervisor: Arc<ValidatorSupervisor>,
        rpc_router: Arc<RpcRouter>,
        snapshots: Arc<SnapshotStore>,
        personas: Arc<PersonaStore>,
    ) -> Self {
        let scenarios = Arc::new(ScenarioEngine::new(
            Arc::clone(&personas),
            Arc::clone(&supervisor),
        ));
        let (tx_pipeline, transport) = build_tx_pipeline(&supervisor, &rpc_router, &personas);
        let idl_registry = build_idl_registry(Arc::clone(&transport));
        Self {
            supervisor,
            rpc_router,
            snapshots,
            personas,
            scenarios,
            tx_pipeline,
            idl_registry,
            transport,
            deploy_services: Arc::new(DeployServices::system()),
            audit_engine: Arc::new(AuditEngine::system()),
        }
    }

    /// Test/integration constructor that takes a caller-provided
    /// `TxPipeline`. Phase 3 integration tests use this to inject a
    /// scripted transport + mock ALT runner without touching the network.
    pub fn with_tx_pipeline(
        supervisor: Arc<ValidatorSupervisor>,
        rpc_router: Arc<RpcRouter>,
        snapshots: Arc<SnapshotStore>,
        personas: Arc<PersonaStore>,
        tx_pipeline: Arc<TxPipeline>,
    ) -> Self {
        let scenarios = Arc::new(ScenarioEngine::new(
            Arc::clone(&personas),
            Arc::clone(&supervisor),
        ));
        let idl_registry = Arc::new(IdlRegistry::new(
            Arc::new(NoopIdlFetcher) as Arc<dyn IdlFetcher>
        ));
        let transport: Arc<dyn RpcTransport> = Arc::new(HttpRpcTransport::new());
        Self {
            supervisor,
            rpc_router,
            snapshots,
            personas,
            scenarios,
            tx_pipeline,
            idl_registry,
            transport,
            deploy_services: Arc::new(DeployServices::system()),
            audit_engine: Arc::new(AuditEngine::system()),
        }
    }

    /// Test/integration constructor with everything injectable. Phase 5
    /// integration tests use this to wire a scripted RPC transport into
    /// the upgrade-safety checker plus a mock deploy runner.
    pub fn with_program_pipeline(
        supervisor: Arc<ValidatorSupervisor>,
        rpc_router: Arc<RpcRouter>,
        snapshots: Arc<SnapshotStore>,
        personas: Arc<PersonaStore>,
        tx_pipeline: Arc<TxPipeline>,
        transport: Arc<dyn RpcTransport>,
        deploy_services: Arc<DeployServices>,
    ) -> Self {
        let scenarios = Arc::new(ScenarioEngine::new(
            Arc::clone(&personas),
            Arc::clone(&supervisor),
        ));
        let idl_registry = build_idl_registry(Arc::clone(&transport));
        Self {
            supervisor,
            rpc_router,
            snapshots,
            personas,
            scenarios,
            tx_pipeline,
            idl_registry,
            transport,
            deploy_services,
            audit_engine: Arc::new(AuditEngine::system()),
        }
    }

    /// Test/integration constructor that lets the caller inject a
    /// scripted `AuditEngine` (so unit tests can drive the Phase 6
    /// surface without hitting external binaries).
    pub fn with_audit_engine(mut self, engine: Arc<AuditEngine>) -> Self {
        self.audit_engine = engine;
        self
    }

    pub fn supervisor(&self) -> Arc<ValidatorSupervisor> {
        Arc::clone(&self.supervisor)
    }

    pub fn rpc_router(&self) -> Arc<RpcRouter> {
        Arc::clone(&self.rpc_router)
    }

    pub fn snapshots(&self) -> Arc<SnapshotStore> {
        Arc::clone(&self.snapshots)
    }

    pub fn personas(&self) -> Arc<PersonaStore> {
        Arc::clone(&self.personas)
    }

    pub fn scenarios(&self) -> Arc<ScenarioEngine> {
        Arc::clone(&self.scenarios)
    }

    pub fn tx_pipeline(&self) -> Arc<TxPipeline> {
        Arc::clone(&self.tx_pipeline)
    }

    pub fn idl_registry(&self) -> Arc<IdlRegistry> {
        Arc::clone(&self.idl_registry)
    }

    pub fn transport(&self) -> Arc<dyn RpcTransport> {
        Arc::clone(&self.transport)
    }

    pub fn deploy_services(&self) -> Arc<DeployServices> {
        Arc::clone(&self.deploy_services)
    }

    pub fn audit_engine(&self) -> Arc<AuditEngine> {
        Arc::clone(&self.audit_engine)
    }

    /// Resolve the RPC URL the persona / scenario commands should use when
    /// the caller hasn't supplied one. Prefers the active supervisor's URL;
    /// falls back to whichever endpoint the router considers healthy.
    pub fn resolve_rpc_url(&self, cluster: ClusterKind) -> Option<String> {
        let status = self.supervisor.status();
        if status.kind == Some(cluster) {
            if let Some(url) = status.rpc_url.clone() {
                return Some(url);
            }
        }
        self.rpc_router.pick_healthy(cluster).map(|e| e.url)
    }
}

// ---------- Tauri commands --------------------------------------------------

#[tauri::command]
pub fn solana_toolchain_status() -> CommandResult<ToolchainStatus> {
    Ok(toolchain::probe())
}

#[tauri::command]
pub fn solana_cluster_list() -> CommandResult<Vec<ClusterDescriptor>> {
    Ok(cluster_descriptors())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterStartRequest {
    pub kind: ClusterKind,
    #[serde(default)]
    pub opts: StartOpts,
}

#[tauri::command]
pub fn solana_cluster_start<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: ClusterStartRequest,
) -> CommandResult<ClusterHandle> {
    let (handle, events) = state.supervisor.start(request.kind, request.opts)?;
    for payload in events {
        let _ = app.emit(SOLANA_VALIDATOR_STATUS_EVENT, payload);
    }
    Ok(handle)
}

#[tauri::command]
pub fn solana_cluster_stop<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
) -> CommandResult<()> {
    let events = state.supervisor.stop()?;
    for payload in events {
        let _ = app.emit(SOLANA_VALIDATOR_STATUS_EVENT, payload);
    }
    Ok(())
}

#[tauri::command]
pub fn solana_cluster_status(state: State<'_, SolanaState>) -> CommandResult<ClusterStatus> {
    Ok(state.supervisor.status())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotCreateRequest {
    pub label: String,
    pub accounts: Vec<String>,
    #[serde(default)]
    pub cluster: Option<ClusterKind>,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_snapshot_create(
    state: State<'_, SolanaState>,
    request: SnapshotCreateRequest,
) -> CommandResult<SnapshotMeta> {
    if request.accounts.is_empty() {
        return Err(CommandError::user_fixable(
            "solana_snapshot_accounts_empty",
            "At least one account pubkey is required to create a snapshot.",
        ));
    }

    let status = state.supervisor.status();
    let cluster_label = request
        .cluster
        .map(|c| c.as_str().to_string())
        .or_else(|| status.kind.map(|c| c.as_str().to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let rpc_url = request
        .rpc_url
        .or(status.rpc_url.clone())
        .or_else(|| {
            request
                .cluster
                .and_then(|c| state.rpc_router.pick_healthy(c).map(|spec| spec.url))
        })
        .ok_or_else(|| {
            CommandError::user_fixable(
                "solana_snapshot_no_rpc_url",
                "Provide rpcUrl or start a cluster before creating a snapshot.",
            )
        })?;

    state
        .snapshots
        .create(&request.label, &cluster_label, &rpc_url, &request.accounts)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotIdRequest {
    pub id: String,
}

#[tauri::command]
pub fn solana_snapshot_list(state: State<'_, SolanaState>) -> CommandResult<Vec<SnapshotMeta>> {
    state.snapshots.list()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRestoreResponse {
    pub id: String,
    pub account_count: usize,
    pub round_trip_ok: bool,
}

#[tauri::command]
pub fn solana_snapshot_restore(
    state: State<'_, SolanaState>,
    request: SnapshotIdRequest,
) -> CommandResult<SnapshotRestoreResponse> {
    let manifest = state.snapshots.read(&request.id)?;
    // Phase 1 restore semantics: read the manifest and re-pull the same
    // accounts from the live cluster; the round-trip check proves they
    // still match the captured state. Phase 2 will actually push the
    // accounts back into a fresh validator.
    let pubkeys: Vec<String> = manifest.accounts.iter().map(|a| a.pubkey.clone()).collect();
    let fetcher = RpcAccountFetcher;
    let replay = fetcher
        .fetch(&manifest.rpc_url, &pubkeys)
        .unwrap_or_default();
    let round_trip_ok = snapshot::verify_round_trip(&manifest, &replay);
    Ok(SnapshotRestoreResponse {
        id: manifest.id,
        account_count: manifest.accounts.len(),
        round_trip_ok,
    })
}

#[tauri::command]
pub fn solana_snapshot_delete(
    state: State<'_, SolanaState>,
    request: SnapshotIdRequest,
) -> CommandResult<()> {
    state.snapshots.delete(&request.id)
}

#[tauri::command]
pub fn solana_rpc_health(state: State<'_, SolanaState>) -> CommandResult<Vec<EndpointHealth>> {
    Ok(state.rpc_router.refresh_health())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RpcEndpointsSetRequest {
    pub cluster: ClusterKind,
    pub endpoints: Vec<EndpointSpec>,
}

#[tauri::command]
pub fn solana_rpc_endpoints_set(
    state: State<'_, SolanaState>,
    request: RpcEndpointsSetRequest,
) -> CommandResult<Vec<EndpointHealth>> {
    state
        .rpc_router
        .set_endpoints(request.cluster, request.endpoints)?;
    Ok(state.rpc_router.snapshot_all())
}

// ---------- Persona commands -----------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaListRequest {
    pub cluster: ClusterKind,
}

#[tauri::command]
pub fn solana_persona_list(
    state: State<'_, SolanaState>,
    request: PersonaListRequest,
) -> CommandResult<Vec<Persona>> {
    state.personas.list(request.cluster)
}

#[tauri::command]
pub fn solana_persona_roles() -> CommandResult<Vec<RoleDescriptor>> {
    Ok(persona_role_descriptors())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaCreateRequest {
    pub spec: PersonaSpec,
    /// Optional RPC override. When None, the workbench resolves the active
    /// supervisor's URL, or the first healthy router endpoint, or no URL
    /// (in which case funding is skipped and the caller must call
    /// `solana_persona_fund` once a validator is running).
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaCreateResponse {
    pub persona: Persona,
    pub receipt: FundingReceipt,
}

#[tauri::command]
pub fn solana_persona_create<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: PersonaCreateRequest,
) -> CommandResult<PersonaCreateResponse> {
    let rpc_url = request
        .rpc_url
        .or_else(|| state.resolve_rpc_url(request.spec.cluster));
    let cluster = request.spec.cluster;
    let (persona, receipt) = state.personas.create(request.spec, rpc_url)?;
    let payload =
        PersonaEventPayload::new(PersonaEventKind::Created, cluster.as_str(), &persona.name)
            .with_pubkey(&persona.pubkey)
            .with_message(format!(
                "funded {} steps, success={}",
                receipt.steps.len(),
                receipt.succeeded
            ));
    let _ = app.emit(SOLANA_PERSONA_EVENT, payload);
    Ok(PersonaCreateResponse { persona, receipt })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaFundRequest {
    pub cluster: ClusterKind,
    pub name: String,
    pub delta: FundingDelta,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_persona_fund<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: PersonaFundRequest,
) -> CommandResult<FundingReceipt> {
    if request.delta.is_empty() {
        return Err(CommandError::user_fixable(
            "solana_persona_fund_empty_delta",
            "Funding request is empty — specify at least one of solLamports, tokens, or nfts.",
        ));
    }
    let rpc_url = request
        .rpc_url
        .or_else(|| state.resolve_rpc_url(request.cluster))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "solana_persona_fund_no_rpc",
                "No RPC URL available — start a cluster or provide rpcUrl explicitly.",
            )
        })?;

    let receipt = state
        .personas
        .fund(request.cluster, &request.name, &request.delta, &rpc_url)?;
    let payload = PersonaEventPayload::new(
        PersonaEventKind::Funded,
        request.cluster.as_str(),
        &request.name,
    )
    .with_message(format!(
        "delta: sol={} tokens={} nfts={}, ok={}",
        request.delta.sol_lamports,
        request.delta.tokens.len(),
        request.delta.nfts.iter().map(|n| n.count).sum::<u32>(),
        receipt.succeeded,
    ));
    let _ = app.emit(SOLANA_PERSONA_EVENT, payload);
    Ok(receipt)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaDeleteRequest {
    pub cluster: ClusterKind,
    pub name: String,
}

#[tauri::command]
pub fn solana_persona_delete<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: PersonaDeleteRequest,
) -> CommandResult<()> {
    state.personas.delete(request.cluster, &request.name)?;
    let payload = PersonaEventPayload::new(
        PersonaEventKind::Deleted,
        request.cluster.as_str(),
        &request.name,
    );
    let _ = app.emit(SOLANA_PERSONA_EVENT, payload);
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaImportKeypairRequest {
    pub cluster: ClusterKind,
    pub name: String,
    #[serde(default = "default_import_role")]
    pub role: PersonaRole,
    /// Absolute filesystem path to a `solana-keygen` JSON keypair file.
    pub keypair_path: PathBuf,
    #[serde(default)]
    pub note: Option<String>,
}

fn default_import_role() -> PersonaRole {
    PersonaRole::Custom
}

#[tauri::command]
pub fn solana_persona_import_keypair<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: PersonaImportKeypairRequest,
) -> CommandResult<Persona> {
    let bytes = std::fs::read(&request.keypair_path).map_err(|err| {
        CommandError::user_fixable(
            "solana_persona_import_read_failed",
            format!(
                "Could not read keypair file {}: {err}",
                request.keypair_path.display()
            ),
        )
    })?;
    let keypair = KeypairBytes::from_solana_keygen_json(&bytes)?;
    let persona = state.personas.import_keypair(
        request.cluster,
        &request.name,
        request.role,
        keypair,
        request.note,
    )?;
    let payload = PersonaEventPayload::new(
        PersonaEventKind::Imported,
        request.cluster.as_str(),
        &persona.name,
    )
    .with_pubkey(&persona.pubkey);
    let _ = app.emit(SOLANA_PERSONA_EVENT, payload);
    Ok(persona)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaExportKeypairResponse {
    pub path: String,
}

#[tauri::command]
pub fn solana_persona_export_keypair(
    state: State<'_, SolanaState>,
    request: PersonaDeleteRequest, // same shape: cluster + name
) -> CommandResult<PersonaExportKeypairResponse> {
    let path = state
        .personas
        .export_keypair_path(request.cluster, &request.name)?;
    Ok(PersonaExportKeypairResponse {
        path: path.display().to_string(),
    })
}

// ---------- Scenario commands ----------------------------------------------

#[tauri::command]
pub fn solana_scenario_list() -> CommandResult<Vec<ScenarioDescriptor>> {
    Ok(scenario_descriptors())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioRunRequest {
    pub spec: ScenarioSpec,
}

#[tauri::command]
pub fn solana_scenario_run<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: ScenarioRunRequest,
) -> CommandResult<ScenarioRun> {
    let spec = request.spec;
    let _ = app.emit(
        SOLANA_SCENARIO_EVENT,
        ScenarioEventPayload::new(
            ScenarioEventKind::Started,
            &spec.id,
            spec.cluster.as_str(),
            &spec.persona,
        ),
    );
    let run = state.scenarios.run(spec)?;

    let finished_kind = match run.status {
        ScenarioStatus::Succeeded => ScenarioEventKind::Completed,
        ScenarioStatus::Failed => ScenarioEventKind::Failed,
        ScenarioStatus::PendingPipeline => ScenarioEventKind::PendingPipeline,
    };
    let message = run
        .pipeline_hint
        .clone()
        .unwrap_or_else(|| format!("{} steps completed", run.steps.len()));
    let payload =
        ScenarioEventPayload::new(finished_kind, &run.id, run.cluster.as_str(), &run.persona)
            .with_message(message)
            .with_signature_count(run.signatures.len().min(u32::MAX as usize) as u32);
    let _ = app.emit(SOLANA_SCENARIO_EVENT, payload);
    Ok(run)
}

// ---------- Transaction pipeline commands (Phase 3) -----------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TxBuildRequest {
    pub spec: TxSpec,
}

#[tauri::command]
pub fn solana_tx_build<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: TxBuildRequest,
) -> CommandResult<TxPlan> {
    let cluster = request.spec.cluster;
    let plan = state.tx_pipeline.build(request.spec)?;
    let _ = app.emit(
        SOLANA_TX_EVENT,
        TxEventPayload::new(TxEventKind::Building, cluster.as_str())
            .with_summary(plan.compute_budget.rationale.clone()),
    );
    Ok(plan)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TxSimulateRequest {
    pub request: SimulateRequest,
}

#[tauri::command]
pub fn solana_tx_simulate<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: TxSimulateRequest,
) -> CommandResult<SimulationResult> {
    let cluster = request.request.cluster;
    let result = state.tx_pipeline.simulate(request.request)?;
    let _ = app.emit(
        SOLANA_TX_EVENT,
        TxEventPayload::new(TxEventKind::Simulated, cluster.as_str())
            .with_summary(result.explanation.summary.clone()),
    );
    Ok(result)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TxSendRequest {
    pub request: SendRequest,
}

#[tauri::command]
pub fn solana_tx_send<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: TxSendRequest,
) -> CommandResult<TxResult> {
    let cluster = request.request.cluster;
    let result = state.tx_pipeline.send(request.request)?;
    let kind = if result.err.is_some() || !result.explanation.ok {
        TxEventKind::Failed
    } else {
        TxEventKind::Confirmed
    };
    let _ = app.emit(
        SOLANA_TX_EVENT,
        TxEventPayload::new(kind, cluster.as_str())
            .with_signature(&result.signature)
            .with_summary(result.explanation.summary.clone()),
    );
    Ok(result)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TxExplainRequest {
    pub request: ExplainRequest,
}

#[tauri::command]
pub fn solana_tx_explain<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: TxExplainRequest,
) -> CommandResult<TxResult> {
    let cluster = request.request.cluster;
    let signature = request.request.signature.clone();
    let result = state.tx_pipeline.explain(request.request)?;
    let _ = app.emit(
        SOLANA_TX_EVENT,
        TxEventPayload::new(TxEventKind::Decoded, cluster.as_str())
            .with_signature(signature)
            .with_summary(result.explanation.summary.clone()),
    );
    Ok(result)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PriorityFeeRequest {
    pub cluster: ClusterKind,
    #[serde(default)]
    pub program_ids: Vec<String>,
    #[serde(default = "default_priority_percentile")]
    pub target: SamplePercentile,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

fn default_priority_percentile() -> SamplePercentile {
    SamplePercentile::Median
}

#[tauri::command]
pub fn solana_priority_fee_estimate(
    state: State<'_, SolanaState>,
    request: PriorityFeeRequest,
) -> CommandResult<FeeEstimate> {
    state.tx_pipeline.priority_fee_estimate(
        request.cluster,
        &request.program_ids,
        request.target,
        request.rpc_url,
    )
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CpiResolveRequest {
    pub program_id: String,
    pub instruction: String,
    #[serde(default)]
    pub args: ResolveArgs,
}

#[tauri::command]
pub fn solana_cpi_resolve(request: CpiResolveRequest) -> CommandResult<KnownProgramLookup> {
    Ok(tx::cpi_resolver::resolve(
        &request.program_id,
        &request.instruction,
        &request.args,
    ))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AltCreateRequest {
    pub cluster: ClusterKind,
    pub authority_persona: String,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_alt_create(
    state: State<'_, SolanaState>,
    request: AltCreateRequest,
) -> CommandResult<AltCreateResult> {
    state
        .tx_pipeline
        .alt_create(request.cluster, &request.authority_persona, request.rpc_url)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AltExtendRequest {
    pub cluster: ClusterKind,
    pub alt: String,
    pub addresses: Vec<String>,
    pub authority_persona: String,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_alt_extend(
    state: State<'_, SolanaState>,
    request: AltExtendRequest,
) -> CommandResult<AltExtendResult> {
    state.tx_pipeline.alt_extend(
        request.cluster,
        &request.alt,
        &request.addresses,
        &request.authority_persona,
        request.rpc_url,
    )
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AltResolveRequest {
    pub addresses: Vec<String>,
    #[serde(default)]
    pub candidates: Vec<AltCandidate>,
}

#[tauri::command]
pub fn solana_alt_resolve(
    state: State<'_, SolanaState>,
    request: AltResolveRequest,
) -> CommandResult<AltResolveReport> {
    Ok(state
        .tx_pipeline
        .alt_suggest(&request.addresses, &request.candidates))
}

// ---------- IDL commands (Phase 4) -----------------------------------------

/// Sink that bridges the IdlRegistry's watcher events onto a Tauri
/// `AppHandle` so the frontend's `solana:idl:changed` listener hears
/// them.
#[derive(Clone)]
struct TauriIdlEventSink<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> std::fmt::Debug for TauriIdlEventSink<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriIdlEventSink").finish_non_exhaustive()
    }
}

impl<R: Runtime> IdlEventSink for TauriIdlEventSink<R> {
    fn emit(&self, event: IdlChangedEvent) {
        let _ = self.app.emit(SOLANA_IDL_CHANGED_EVENT, event);
    }
}

#[derive(Clone)]
struct TauriDeployProgressSink<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> std::fmt::Debug for TauriDeployProgressSink<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriDeployProgressSink")
            .finish_non_exhaustive()
    }
}

impl<R: Runtime> DeployProgressSink for TauriDeployProgressSink<R> {
    fn emit(&self, payload: DeployProgressPayload) {
        let _ = self.app.emit(SOLANA_DEPLOY_PROGRESS_EVENT, payload);
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdlLoadRequest {
    pub path: String,
}

#[tauri::command]
pub fn solana_idl_load(
    state: State<'_, SolanaState>,
    request: IdlLoadRequest,
) -> CommandResult<Idl> {
    state.idl_registry.load_file(Path::new(&request.path))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdlFetchRequest {
    pub program_id: String,
    pub cluster: ClusterKind,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_idl_fetch(
    state: State<'_, SolanaState>,
    request: IdlFetchRequest,
) -> CommandResult<Option<Idl>> {
    let rpc_url = request
        .rpc_url
        .or_else(|| state.resolve_rpc_url(request.cluster))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "solana_idl_no_rpc",
                "No RPC URL available — start a cluster or provide rpcUrl explicitly.",
            )
        })?;
    state
        .idl_registry
        .fetch_on_chain(request.cluster, &rpc_url, &request.program_id)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdlGetRequest {
    pub program_id: String,
    #[serde(default)]
    pub cluster: Option<ClusterKind>,
}

#[tauri::command]
pub fn solana_idl_get(
    state: State<'_, SolanaState>,
    request: IdlGetRequest,
) -> CommandResult<Option<Idl>> {
    Ok(state
        .idl_registry
        .get_cached(&request.program_id, request.cluster))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdlWatchRequest {
    pub path: String,
}

#[tauri::command]
pub fn solana_idl_watch<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: IdlWatchRequest,
) -> CommandResult<IdlSubscriptionToken> {
    state
        .idl_registry
        .set_sink(Arc::new(TauriIdlEventSink { app }) as Arc<dyn IdlEventSink>);
    state.idl_registry.watch_path(Path::new(&request.path))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdlUnwatchRequest {
    pub token: IdlSubscriptionToken,
}

#[tauri::command]
pub fn solana_idl_unwatch(
    state: State<'_, SolanaState>,
    request: IdlUnwatchRequest,
) -> CommandResult<bool> {
    state.idl_registry.unwatch(&request.token)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdlDriftRequest {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub local_path: String,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_idl_drift(
    state: State<'_, SolanaState>,
    request: IdlDriftRequest,
) -> CommandResult<DriftReport> {
    let local = state
        .idl_registry
        .load_file(Path::new(&request.local_path))?;
    let rpc_url = request
        .rpc_url
        .or_else(|| state.resolve_rpc_url(request.cluster))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "solana_idl_no_rpc",
                "No RPC URL available — start a cluster or provide rpcUrl explicitly.",
            )
        })?;
    let chain =
        state
            .idl_registry
            .fetch_on_chain(request.cluster, &rpc_url, &request.program_id)?;
    Ok(idl::drift::classify(&local, chain.as_ref()))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdlPublishArgs {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub idl_path: String,
    pub authority_persona: String,
    pub mode: IdlPublishMode,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_idl_publish<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: IdlPublishArgs,
) -> CommandResult<IdlPublishReport> {
    let rpc_url = request
        .rpc_url
        .or_else(|| state.resolve_rpc_url(request.cluster))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "solana_idl_no_rpc",
                "No RPC URL available — start a cluster or provide rpcUrl explicitly.",
            )
        })?;
    let keypair = state
        .personas
        .keypair_path(request.cluster, &request.authority_persona)?;
    let runner = idl::publish::SystemAnchorIdlRunner::new();
    let sink = TauriDeployProgressSink { app };
    let publish_request = IdlPublishRequest {
        program_id: request.program_id,
        cluster: request.cluster,
        idl_path: request.idl_path,
        authority_keypair_path: keypair.display().to_string(),
        rpc_url,
        mode: request.mode,
    };
    idl::publish::publish(&runner, &sink, &publish_request)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodamaGenerateRequest {
    pub idl_path: String,
    pub targets: Vec<CodamaTarget>,
    pub output_dir: String,
}

#[tauri::command]
pub fn solana_codama_generate(
    request: CodamaGenerateRequest,
) -> CommandResult<CodamaGenerationReport> {
    let runner = idl::codama::SystemCodamaRunner::new();
    idl::codama::generate(
        &runner,
        &CodamaGenerationRequest {
            idl_path: request.idl_path,
            targets: request.targets,
            output_dir: request.output_dir,
        },
    )
}

// ---------- PDA commands (Phase 4) -----------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PdaDeriveRequest {
    pub program_id: String,
    pub seeds: Vec<SeedPart>,
    #[serde(default)]
    pub bump: Option<u8>,
}

#[tauri::command]
pub fn solana_pda_derive(request: PdaDeriveRequest) -> CommandResult<DerivedAddress> {
    match request.bump {
        Some(bump) => pda::create_program_address(&request.program_id, &request.seeds, bump),
        None => pda::find_program_address(&request.program_id, &request.seeds),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PdaScanRequest {
    pub project_root: String,
}

#[tauri::command]
pub fn solana_pda_scan(request: PdaScanRequest) -> CommandResult<Vec<PdaSite>> {
    pda::scan(Path::new(&request.project_root))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PdaPredictRequest {
    pub program_id: String,
    pub seeds: Vec<SeedPart>,
    pub clusters: Vec<ClusterKind>,
}

#[tauri::command]
pub fn solana_pda_predict(request: PdaPredictRequest) -> CommandResult<Vec<ClusterPda>> {
    pda::predict(&request.program_id, &request.seeds, &request.clusters)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PdaAnalyseBumpRequest {
    pub program_id: String,
    pub seeds: Vec<SeedPart>,
    #[serde(default)]
    pub bump: Option<u8>,
}

#[tauri::command]
pub fn solana_pda_analyse_bump(request: PdaAnalyseBumpRequest) -> CommandResult<BumpAnalysis> {
    pda::analyse_bump(&request.program_id, &request.seeds, request.bump)
}

// ---------- Program build / deploy / upgrade safety (Phase 5) -------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgramBuildArgs {
    pub manifest_path: String,
    #[serde(default)]
    pub profile: BuildProfile,
    #[serde(default)]
    pub kind: Option<BuildKind>,
    #[serde(default)]
    pub program: Option<String>,
}

#[tauri::command]
pub fn solana_program_build(
    state: State<'_, SolanaState>,
    request: ProgramBuildArgs,
) -> CommandResult<BuildReport> {
    let runner = state.deploy_services.runner.as_ref();
    let _ = runner; // The build module uses its own runner trait; avoid type confusion.
    let runner = program::build::SystemBuildRunner::new();
    program::build::build(
        &runner,
        &BuildRequest {
            manifest_path: request.manifest_path,
            profile: request.profile,
            kind: request.kind,
            program: request.program,
        },
    )
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpgradeCheckArgs {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub local_so_path: String,
    pub expected_authority: String,
    #[serde(default)]
    pub local_idl_path: Option<String>,
    #[serde(default)]
    pub max_program_size_bytes: Option<u64>,
    #[serde(default)]
    pub local_so_size_bytes: Option<u64>,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_program_upgrade_check(
    state: State<'_, SolanaState>,
    request: UpgradeCheckArgs,
) -> CommandResult<UpgradeSafetyReport> {
    let rpc_url = request
        .rpc_url
        .or_else(|| state.resolve_rpc_url(request.cluster))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "solana_upgrade_check_no_rpc",
                "No RPC URL available — start a cluster or provide rpcUrl explicitly.",
            )
        })?;
    // Try to fetch on-chain IDL for layout diff. Best-effort — if the
    // program has no published IDL the layout check is simply skipped.
    let chain_idl = state
        .idl_registry
        .fetch_on_chain(request.cluster, &rpc_url, &request.program_id)
        .ok()
        .flatten();
    let safety_request = UpgradeSafetyRequest {
        program_id: request.program_id,
        cluster: request.cluster,
        rpc_url,
        local_so_path: request.local_so_path,
        local_idl_path: request.local_idl_path,
        chain_idl,
        local_idl: None,
        expected_authority: request.expected_authority,
        max_program_size_bytes: request.max_program_size_bytes,
        local_so_size_bytes: request.local_so_size_bytes,
    };
    let transport = state.transport();
    program::upgrade_safety::check(&transport, &safety_request)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeployArgs {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub so_path: String,
    pub authority: DeployAuthority,
    #[serde(default)]
    pub idl_path: Option<String>,
    #[serde(default)]
    pub is_first_deploy: bool,
    #[serde(default)]
    pub post: Option<PostDeployOptions>,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_program_deploy<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: DeployArgs,
) -> CommandResult<DeployResult> {
    let rpc_url = request
        .rpc_url
        .or_else(|| state.resolve_rpc_url(request.cluster))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "solana_program_deploy_no_rpc",
                "No RPC URL available — start a cluster or provide rpcUrl explicitly.",
            )
        })?;
    let spec = DeploySpec {
        program_id: request.program_id,
        cluster: request.cluster,
        rpc_url,
        so_path: request.so_path,
        idl_path: request.idl_path,
        authority: request.authority,
        is_first_deploy: request.is_first_deploy,
        post: request.post.unwrap_or_default(),
    };
    let services = state.deploy_services();
    let sink = TauriDeployProgressSink { app };
    program::deploy::deploy(&services, &sink, &spec)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackArgs {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub previous_sha256: String,
    pub authority: DeployAuthority,
    #[serde(default)]
    pub program_archive_root: Option<String>,
    #[serde(default)]
    pub post: Option<PostDeployOptions>,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_program_rollback<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: RollbackArgs,
) -> CommandResult<RollbackResult> {
    let rpc_url = request
        .rpc_url
        .or_else(|| state.resolve_rpc_url(request.cluster))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "solana_program_rollback_no_rpc",
                "No RPC URL available — start a cluster or provide rpcUrl explicitly.",
            )
        })?;
    let req = RollbackRequest {
        program_id: request.program_id,
        cluster: request.cluster,
        rpc_url,
        previous_sha256: request.previous_sha256,
        authority: request.authority,
        program_archive_root: request.program_archive_root,
        post: request.post.unwrap_or_default(),
    };
    let services = state.deploy_services();
    let sink = TauriDeployProgressSink { app };
    program::deploy::rollback(&services, &sink, &req)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SquadsProposalCreateArgs {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub multisig_pda: String,
    pub buffer: String,
    pub spill: String,
    pub creator: String,
    #[serde(default)]
    pub vault_index: Option<u8>,
    #[serde(default)]
    pub memo: Option<String>,
}

#[tauri::command]
pub fn solana_squads_proposal_create(
    request: SquadsProposalCreateArgs,
) -> CommandResult<SquadsProposalDescriptor> {
    program::squads::synthesize(&SquadsProposalRequest {
        program_id: request.program_id,
        cluster: request.cluster,
        multisig_pda: request.multisig_pda,
        buffer: request.buffer,
        spill: request.spill,
        creator: request.creator,
        vault_index: request.vault_index,
        memo: request.memo,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifiedBuildArgs {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub manifest_path: String,
    pub github_url: String,
    #[serde(default)]
    pub commit_hash: Option<String>,
    #[serde(default)]
    pub library_name: Option<String>,
    #[serde(default)]
    pub skip_remote_submit: bool,
}

#[tauri::command]
pub fn solana_verified_build_submit(
    request: VerifiedBuildArgs,
) -> CommandResult<VerifiedBuildResult> {
    let runner = program::verified_build::SystemVerifiedBuildRunner::new();
    program::verified_build::submit(
        &runner,
        &VerifiedBuildRequest {
            program_id: request.program_id,
            cluster: request.cluster,
            manifest_path: request.manifest_path,
            github_url: request.github_url,
            commit_hash: request.commit_hash,
            library_name: request.library_name,
            skip_remote_submit: request.skip_remote_submit,
        },
    )
}

// ---------- Audit commands (Phase 6) ---------------------------------------

/// Sink that bridges `AuditEngine` events onto the Tauri event bus so
/// the frontend renders streaming findings live.
#[derive(Clone)]
struct TauriAuditEventSink<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> std::fmt::Debug for TauriAuditEventSink<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriAuditEventSink").finish_non_exhaustive()
    }
}

impl<R: Runtime> AuditEventSink for TauriAuditEventSink<R> {
    fn emit(&self, payload: AuditEventPayload) {
        let _ = self.app.emit(SOLANA_AUDIT_EVENT, payload);
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditStaticArgs {
    pub request: StaticLintRequest,
}

#[tauri::command]
pub fn solana_audit_static<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: AuditStaticArgs,
) -> CommandResult<StaticLintReport> {
    let sink = TauriAuditEventSink { app };
    state.audit_engine.run_static_lints(&request.request, &sink)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditExternalArgs {
    pub request: ExternalAnalyzerRequest,
}

#[tauri::command]
pub fn solana_audit_external<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: AuditExternalArgs,
) -> CommandResult<ExternalAnalyzerReport> {
    let sink = TauriAuditEventSink { app };
    state
        .audit_engine
        .run_external_analyzer(&request.request, &sink)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditFuzzArgs {
    pub request: FuzzRequest,
}

#[tauri::command]
pub fn solana_audit_fuzz<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: AuditFuzzArgs,
) -> CommandResult<FuzzReport> {
    let sink = TauriAuditEventSink { app };
    state.audit_engine.run_fuzz(&request.request, &sink)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditFuzzHarnessArgs {
    pub request: TridentHarnessRequest,
}

#[tauri::command]
pub fn solana_audit_fuzz_scaffold(
    state: State<'_, SolanaState>,
    request: AuditFuzzHarnessArgs,
) -> CommandResult<TridentHarnessResult> {
    state
        .audit_engine
        .generate_fuzz_harness(&request.request)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditCoverageArgs {
    pub request: CoverageRequest,
}

#[tauri::command]
pub fn solana_audit_coverage<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: AuditCoverageArgs,
) -> CommandResult<CoverageReport> {
    let sink = TauriAuditEventSink { app };
    state.audit_engine.run_coverage(&request.request, &sink)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditReplayArgs {
    pub request: ReplayRequest,
}

#[tauri::command]
pub fn solana_replay_exploit<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: AuditReplayArgs,
) -> CommandResult<ReplayReport> {
    let sink = TauriAuditEventSink { app };
    state.audit_engine.run_replay(&request.request, &sink)
}

#[tauri::command]
pub fn solana_replay_list(state: State<'_, SolanaState>) -> CommandResult<Vec<ExploitDescriptor>> {
    Ok(state
        .audit_engine
        .library()
        .all()
        .into_iter()
        .cloned()
        .collect())
}

/// Lightweight acknowledgement that the frontend can call when it opens
/// the sidebar so the backend emits the current validator status on a
/// well-known channel (matches the emulator `subscribe_ready` pattern).
#[tauri::command]
pub fn solana_subscribe_ready<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
) -> CommandResult<ClusterStatus> {
    let status = state.supervisor.status();
    let phase = if status.running {
        ValidatorPhase::Ready
    } else {
        ValidatorPhase::Stopped
    };
    let mut payload = ValidatorStatusPayload::new(phase);
    if let Some(kind) = status.kind {
        payload = payload.with_kind(kind.as_str());
    }
    if let Some(url) = status.rpc_url.as_ref() {
        payload = payload.with_rpc_url(url);
    }
    if let Some(url) = status.ws_url.as_ref() {
        payload = payload.with_ws_url(url);
    }
    let _ = app.emit(SOLANA_VALIDATOR_STATUS_EVENT, payload);
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_has_every_cluster_in_router() {
        let state = SolanaState::default();
        let snap = state.rpc_router.snapshot_all();
        for kind in ClusterKind::ALL {
            assert!(snap.iter().any(|e| e.cluster == kind));
        }
    }

    #[test]
    fn default_state_has_idle_supervisor() {
        let state = SolanaState::default();
        assert!(!state.supervisor.status().running);
    }
}
