//! Autonomous-runtime wrapper for the Solana workbench.
//!
//! Ships cluster lifecycle plus the transaction/program/audit surfaces,
//! and now includes the Phase 7 log + indexer tools (`solana_logs`,
//! `solana_indexer`) so agents can fetch decoded recent events and
//! scaffold/run local indexers from the same command surface.
//!
//! Mirrors `browser.rs`: the request enum is JSON-in/JSON-out, the
//! executor trait is trivially mockable, and the production wiring
//! bridges to the Tauri `SolanaState` the UI uses.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::commands::solana::{
    cost, drift,
    idl::{self, codama::CodamaGenerationRequest},
    indexer, pda, program, secrets, AltCandidate, AltCreateResult, AltExtendResult,
    AltResolveReport, AnalyzerKind, AuditEngine, BuildKind, BuildProfile, BuildReport,
    BuildRequest, BumpAnalysis, ClusterDriftReport, ClusterHandle, ClusterKind, ClusterPda,
    ClusterStatus, CodamaGenerationReport, CodamaTarget, CostSnapshot, CostSnapshotRequest,
    CoverageReport, CoverageRequest, DeployAuthority, DeployResult, DeployServices, DeploySpec,
    DerivedAddress, DocSnippet, DriftCheckRequest, DriftReport, EndpointHealth, ExplainRequest,
    ExploitDescriptor, ExploitKey, ExternalAnalyzerReport, ExternalAnalyzerRequest, FeeEstimate,
    FuzzReport, FuzzRequest, Idl, IdlPublishMode, IdlPublishReport, IdlPublishRequest, IdlRegistry,
    IdlSubscriptionToken, IndexerKind, IndexerRunReport, IndexerRunRequest, KnownProgramLookup,
    LocalCostLedger, LogFilter, LogSubscriptionToken, NullAuditEventSink, PdaSite,
    PostDeployOptions, ProviderUsageRunner, ReplayReport, ReplayRequest, ResolveArgs,
    RollbackRequest, RollbackResult, RpcRouter, RpcTransport, SamplePercentile, ScaffoldRequest,
    ScaffoldResult, ScopeCheckReport, SecretScanReport, SecretsScanRequest, SeedPart, SendRequest,
    SimulateRequest, SimulationResult, SnapshotMeta, SnapshotStore, SolanaState,
    SquadsProposalDescriptor, SquadsProposalRequest, StartOpts, StaticLintReport,
    StaticLintRequest, TrackedProgram, TridentHarnessRequest, TridentHarnessResult, TxCostRecord,
    TxPipeline, TxPlan, TxResult, TxSpec, UpgradeSafetyReport, UpgradeSafetyRequest,
    ValidatorSupervisor, VerifiedBuildRequest, VerifiedBuildResult,
};
use crate::commands::{CommandError, CommandResult};

pub const AUTONOMOUS_TOOL_SOLANA_CLUSTER: &str = "solana_cluster";
pub const AUTONOMOUS_TOOL_SOLANA_LOGS: &str = "solana_logs";
pub const AUTONOMOUS_TOOL_SOLANA_TX: &str = "solana_tx";
pub const AUTONOMOUS_TOOL_SOLANA_SIMULATE: &str = "solana_simulate";
pub const AUTONOMOUS_TOOL_SOLANA_EXPLAIN: &str = "solana_explain";
pub const AUTONOMOUS_TOOL_SOLANA_ALT: &str = "solana_alt";
pub const AUTONOMOUS_TOOL_SOLANA_IDL: &str = "solana_idl";
pub const AUTONOMOUS_TOOL_SOLANA_CODAMA: &str = "solana_codama";
pub const AUTONOMOUS_TOOL_SOLANA_PDA: &str = "solana_pda";
pub const AUTONOMOUS_TOOL_SOLANA_PROGRAM: &str = "solana_program";
pub const AUTONOMOUS_TOOL_SOLANA_DEPLOY: &str = "solana_deploy";
pub const AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK: &str = "solana_upgrade_check";
pub const AUTONOMOUS_TOOL_SOLANA_SQUADS: &str = "solana_squads";
pub const AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD: &str = "solana_verified_build";
pub const AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC: &str = "solana_audit_static";
pub const AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL: &str = "solana_audit_external";
pub const AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ: &str = "solana_audit_fuzz";
pub const AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE: &str = "solana_audit_coverage";
pub const AUTONOMOUS_TOOL_SOLANA_REPLAY: &str = "solana_replay";
pub const AUTONOMOUS_TOOL_SOLANA_INDEXER: &str = "solana_indexer";
pub const AUTONOMOUS_TOOL_SOLANA_SECRETS: &str = "solana_secrets";
pub const AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT: &str = "solana_cluster_drift";
pub const AUTONOMOUS_TOOL_SOLANA_COST: &str = "solana_cost";
pub const AUTONOMOUS_TOOL_SOLANA_DOCS: &str = "solana_docs";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaClusterAction {
    List,
    Start {
        kind: ClusterKind,
        #[serde(default)]
        opts: StartOpts,
    },
    Stop,
    Status,
    SnapshotList,
    SnapshotCreate {
        label: String,
        accounts: Vec<String>,
        #[serde(default)]
        cluster: Option<ClusterKind>,
        #[serde(default)]
        rpc_url: Option<String>,
    },
    SnapshotDelete {
        id: String,
    },
    RpcHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaClusterRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaClusterAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaLogsAction {
    /// Fetch and decode recent logs for one or more program ids.
    Recent {
        cluster: ClusterKind,
        #[serde(default)]
        program_ids: Vec<String>,
        #[serde(default)]
        last_n: Option<u32>,
        #[serde(default)]
        rpc_url: Option<String>,
        #[serde(default)]
        cached_only: bool,
    },
    /// Return currently-active subscriptions from the `LogBus`.
    Active,
    /// Start a live subscription.
    Subscribe { filter: LogFilter },
    /// Stop a live subscription.
    Unsubscribe { token: LogSubscriptionToken },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaLogsRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaLogsAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaIndexerAction {
    Scaffold {
        kind: IndexerKind,
        idl_path: String,
        output_dir: String,
        #[serde(default)]
        project_slug: Option<String>,
        #[serde(default)]
        overwrite: bool,
        #[serde(default)]
        rpc_url: Option<String>,
    },
    Run {
        cluster: ClusterKind,
        program_ids: Vec<String>,
        #[serde(default)]
        last_n: Option<u32>,
        #[serde(default)]
        rpc_url: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaIndexerRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaIndexerAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSolanaOutput {
    pub action: String,
    /// JSON-serialized response held as a string so the overall tool
    /// output can stay `Eq`-derivable.
    pub value_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaTxAction {
    Build {
        spec: TxSpec,
    },
    Send {
        request: SendRequest,
    },
    PriorityFee {
        cluster: ClusterKind,
        #[serde(default)]
        program_ids: Vec<String>,
        #[serde(default = "default_percentile")]
        target: SamplePercentile,
        #[serde(default)]
        rpc_url: Option<String>,
    },
    Cpi {
        program_id: String,
        instruction: String,
        #[serde(default)]
        args: ResolveArgs,
    },
}

fn default_percentile() -> SamplePercentile {
    SamplePercentile::Median
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaTxRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaTxAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSolanaSimulateRequest {
    pub request: SimulateRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSolanaExplainRequest {
    pub request: ExplainRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaAltAction {
    Create {
        cluster: ClusterKind,
        authority_persona: String,
        #[serde(default)]
        rpc_url: Option<String>,
    },
    Extend {
        cluster: ClusterKind,
        alt: String,
        addresses: Vec<String>,
        authority_persona: String,
        #[serde(default)]
        rpc_url: Option<String>,
    },
    Resolve {
        addresses: Vec<String>,
        #[serde(default)]
        candidates: Vec<AltCandidate>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaAltRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaAltAction,
}

// ---------- IDL / Codama / PDA requests (Phase 4) --------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaIdlAction {
    /// Read a local IDL file into the registry.
    Load { path: String },
    /// Fetch from chain via the injected RPC transport.
    Fetch {
        program_id: String,
        cluster: ClusterKind,
        #[serde(default)]
        rpc_url: Option<String>,
    },
    /// Return the most-recently-cached IDL for a program.
    Get {
        program_id: String,
        #[serde(default)]
        cluster: Option<ClusterKind>,
    },
    /// Start watching a local IDL file. Returns a subscription token.
    Watch { path: String },
    /// Stop a previously-started watch.
    Unwatch { token: IdlSubscriptionToken },
    /// Classify local-vs-on-chain drift.
    Drift {
        program_id: String,
        cluster: ClusterKind,
        local_path: String,
        #[serde(default)]
        rpc_url: Option<String>,
    },
    /// Run `anchor idl init/upgrade`. Caller provides the authority
    /// keypair path explicitly (the runtime doesn't expand personas to
    /// keypairs to keep the agent surface local-first).
    Publish {
        program_id: String,
        cluster: ClusterKind,
        idl_path: String,
        authority_keypair_path: String,
        rpc_url: String,
        mode: IdlPublishMode,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaIdlRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaIdlAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSolanaCodamaRequest {
    pub idl_path: String,
    pub targets: Vec<CodamaTarget>,
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaPdaAction {
    Derive {
        program_id: String,
        seeds: Vec<SeedPart>,
        #[serde(default)]
        bump: Option<u8>,
    },
    Scan {
        project_root: String,
    },
    Predict {
        program_id: String,
        seeds: Vec<SeedPart>,
        clusters: Vec<ClusterKind>,
    },
    AnalyseBump {
        program_id: String,
        seeds: Vec<SeedPart>,
        #[serde(default)]
        bump: Option<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaPdaRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaPdaAction,
}

// ---------- Program / deploy / upgrade / squads / verified-build (Phase 5) -

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
#[allow(clippy::large_enum_variant)]
pub enum AutonomousSolanaProgramAction {
    Build {
        manifest_path: String,
        #[serde(default)]
        profile: Option<BuildProfile>,
        #[serde(default)]
        kind: Option<BuildKind>,
        #[serde(default)]
        program: Option<String>,
    },
    Rollback {
        program_id: String,
        cluster: ClusterKind,
        previous_sha256: String,
        authority: DeployAuthority,
        #[serde(default)]
        program_archive_root: Option<String>,
        #[serde(default)]
        post: Option<PostDeployOptions>,
        #[serde(default)]
        rpc_url: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaProgramRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaProgramAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSolanaDeployRequest {
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
    /// Phase 9: project root for the pre-deploy secrets scan.
    #[serde(default)]
    pub project_root: Option<String>,
    /// Phase 9: block on `High`/`Medium` secret findings too.
    #[serde(default)]
    pub block_on_any_secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSolanaUpgradeCheckRequest {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSolanaSquadsRequest {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSolanaVerifiedBuildRequest {
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

// ---------- Audit / fuzz / coverage / replay (Phase 6) --------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaAuditAction {
    Static {
        project_root: String,
        #[serde(default)]
        rule_ids: Vec<String>,
        #[serde(default)]
        skip_paths: Vec<String>,
    },
    External {
        project_root: String,
        #[serde(default)]
        analyzer: AnalyzerKind,
        #[serde(default)]
        timeout_s: Option<u64>,
    },
    Fuzz {
        project_root: String,
        target: String,
        #[serde(default)]
        duration_s: Option<u64>,
        #[serde(default)]
        corpus: Option<String>,
        #[serde(default)]
        baseline_coverage_lines: Option<u64>,
    },
    FuzzScaffold {
        project_root: String,
        target: String,
        #[serde(default)]
        idl_path: Option<String>,
        #[serde(default)]
        overwrite: bool,
    },
    Coverage {
        project_root: String,
        #[serde(default)]
        package: Option<String>,
        #[serde(default)]
        test_filter: Option<String>,
        #[serde(default)]
        lcov_path: Option<String>,
        #[serde(default)]
        instruction_names: Vec<String>,
        #[serde(default)]
        timeout_s: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaAuditRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaAuditAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaReplayAction {
    List,
    Run {
        exploit: ExploitKey,
        target_program: String,
        cluster: ClusterKind,
        #[serde(default)]
        rpc_url: Option<String>,
        #[serde(default)]
        dry_run: bool,
        #[serde(default)]
        snapshot_slot: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaReplayRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaReplayAction,
}

// ---------- Phase 9 — secrets / drift / cost / docs -----------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaSecretsAction {
    Scan { request: SecretsScanRequest },
    Patterns,
    Scope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaSecretsRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaSecretsAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaDriftAction {
    Tracked,
    Check {
        #[serde(flatten)]
        request: DriftCheckRequest,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaDriftRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaDriftAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaCostAction {
    Snapshot {
        #[serde(default)]
        request: Option<CostSnapshotRequest>,
    },
    Record {
        record: TxCostRecord,
    },
    Reset,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaCostRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaCostAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum AutonomousSolanaDocsAction {
    Catalog,
    Tool { tool: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousSolanaDocsRequest {
    #[serde(flatten)]
    pub action: AutonomousSolanaDocsAction,
}

pub trait SolanaExecutor: Send + Sync + std::fmt::Debug {
    fn cluster(
        &self,
        request: AutonomousSolanaClusterRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn logs(&self, request: AutonomousSolanaLogsRequest) -> CommandResult<AutonomousSolanaOutput>;

    fn tx(&self, request: AutonomousSolanaTxRequest) -> CommandResult<AutonomousSolanaOutput>;

    fn simulate(
        &self,
        request: AutonomousSolanaSimulateRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn explain(
        &self,
        request: AutonomousSolanaExplainRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn alt(&self, request: AutonomousSolanaAltRequest) -> CommandResult<AutonomousSolanaOutput>;

    fn idl(&self, request: AutonomousSolanaIdlRequest) -> CommandResult<AutonomousSolanaOutput>;

    fn codama(
        &self,
        request: AutonomousSolanaCodamaRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn pda(&self, request: AutonomousSolanaPdaRequest) -> CommandResult<AutonomousSolanaOutput>;

    fn program(
        &self,
        request: AutonomousSolanaProgramRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn deploy(
        &self,
        request: AutonomousSolanaDeployRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn upgrade_check(
        &self,
        request: AutonomousSolanaUpgradeCheckRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn squads(
        &self,
        request: AutonomousSolanaSquadsRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn verified_build(
        &self,
        request: AutonomousSolanaVerifiedBuildRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn audit(&self, request: AutonomousSolanaAuditRequest)
        -> CommandResult<AutonomousSolanaOutput>;

    fn indexer(
        &self,
        request: AutonomousSolanaIndexerRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn replay(
        &self,
        request: AutonomousSolanaReplayRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn secrets(
        &self,
        request: AutonomousSolanaSecretsRequest,
    ) -> CommandResult<AutonomousSolanaOutput>;

    fn drift(&self, request: AutonomousSolanaDriftRequest)
        -> CommandResult<AutonomousSolanaOutput>;

    fn cost(&self, request: AutonomousSolanaCostRequest) -> CommandResult<AutonomousSolanaOutput>;

    fn docs(&self, request: AutonomousSolanaDocsRequest) -> CommandResult<AutonomousSolanaOutput>;
}

/// Executor that dispatches against a live `SolanaState`. Safe to clone
/// because it just holds an `Arc<SolanaState>`-ish bundle of the
/// supervisor and snapshot store.
#[derive(Debug, Clone)]
pub struct StateSolanaExecutor {
    inner: Arc<StateInner>,
}

#[derive(Debug)]
struct StateInner {
    supervisor: Arc<ValidatorSupervisor>,
    router: Arc<RpcRouter>,
    snapshots: Arc<SnapshotStore>,
    tx_pipeline: Arc<TxPipeline>,
    idl_registry: Arc<IdlRegistry>,
    transport: Arc<dyn RpcTransport>,
    deploy_services: Arc<DeployServices>,
    audit_engine: Arc<AuditEngine>,
    log_bus: Arc<crate::commands::solana::LogBus>,
    log_source: Arc<dyn crate::commands::solana::RpcLogSource>,
    personas: Arc<crate::commands::solana::PersonaStore>,
    cost_ledger: Arc<LocalCostLedger>,
    cost_provider_runner: Arc<dyn ProviderUsageRunner>,
}

impl StateSolanaExecutor {
    pub fn from_state(state: &SolanaState) -> Self {
        Self {
            inner: Arc::new(StateInner {
                supervisor: state.supervisor(),
                router: state.rpc_router(),
                snapshots: state.snapshots(),
                tx_pipeline: state.tx_pipeline(),
                idl_registry: state.idl_registry(),
                transport: state.transport(),
                deploy_services: state.deploy_services(),
                audit_engine: state.audit_engine(),
                log_bus: state.log_bus(),
                log_source: state.log_source(),
                personas: state.personas(),
                cost_ledger: state.cost_ledger(),
                cost_provider_runner: state.cost_provider_runner(),
            }),
        }
    }

    fn resolve_rpc_url(&self, cluster: ClusterKind) -> Option<String> {
        let status = self.inner.supervisor.status();
        if status.kind == Some(cluster) {
            if let Some(url) = status.rpc_url.clone() {
                return Some(url);
            }
        }
        self.inner.router.pick_healthy(cluster).map(|e| e.url)
    }
}

impl SolanaExecutor for StateSolanaExecutor {
    fn cluster(
        &self,
        request: AutonomousSolanaClusterRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaClusterAction::List => {
                let descriptors = crate::commands::solana::cluster_descriptors();
                (
                    "list".to_string(),
                    serde_json::to_value(descriptors).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaClusterAction::Start { kind, opts } => {
                let (handle, _events) = self.inner.supervisor.start(kind, opts)?;
                let value =
                    serde_json::to_value::<ClusterHandle>(handle).unwrap_or(JsonValue::Null);
                ("start".to_string(), value)
            }
            AutonomousSolanaClusterAction::Stop => {
                let _events = self.inner.supervisor.stop()?;
                ("stop".to_string(), JsonValue::Null)
            }
            AutonomousSolanaClusterAction::Status => {
                let status = self.inner.supervisor.status();
                let value =
                    serde_json::to_value::<ClusterStatus>(status).unwrap_or(JsonValue::Null);
                ("status".to_string(), value)
            }
            AutonomousSolanaClusterAction::SnapshotList => {
                let metas = self.inner.snapshots.list()?;
                let value =
                    serde_json::to_value::<Vec<SnapshotMeta>>(metas).unwrap_or(JsonValue::Null);
                ("snapshot_list".to_string(), value)
            }
            AutonomousSolanaClusterAction::SnapshotCreate {
                label,
                accounts,
                cluster,
                rpc_url,
            } => {
                if accounts.is_empty() {
                    return Err(CommandError::user_fixable(
                        "solana_snapshot_accounts_empty",
                        "Snapshot requires at least one account pubkey.",
                    ));
                }
                let status = self.inner.supervisor.status();
                let cluster_label = cluster
                    .map(|c| c.as_str().to_string())
                    .or_else(|| status.kind.map(|c| c.as_str().to_string()))
                    .unwrap_or_else(|| "unknown".to_string());
                let rpc_url = rpc_url
                    .or(status.rpc_url.clone())
                    .or_else(|| {
                        cluster.and_then(|c| self.inner.router.pick_healthy(c).map(|s| s.url))
                    })
                    .ok_or_else(|| {
                        CommandError::user_fixable(
                            "solana_snapshot_no_rpc_url",
                            "Provide rpcUrl or start a cluster before creating a snapshot.",
                        )
                    })?;
                let meta =
                    self.inner
                        .snapshots
                        .create(&label, &cluster_label, &rpc_url, &accounts)?;
                let value = serde_json::to_value::<SnapshotMeta>(meta).unwrap_or(JsonValue::Null);
                ("snapshot_create".to_string(), value)
            }
            AutonomousSolanaClusterAction::SnapshotDelete { id } => {
                self.inner.snapshots.delete(&id)?;
                ("snapshot_delete".to_string(), JsonValue::Null)
            }
            AutonomousSolanaClusterAction::RpcHealth => {
                let snap = self.inner.router.refresh_health();
                let value =
                    serde_json::to_value::<Vec<EndpointHealth>>(snap).unwrap_or(JsonValue::Null);
                ("rpc_health".to_string(), value)
            }
        };

        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn logs(&self, request: AutonomousSolanaLogsRequest) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaLogsAction::Active => {
                let active = self
                    .inner
                    .log_bus
                    .active_subscriptions()
                    .into_iter()
                    .map(|(token, filter)| {
                        serde_json::json!({
                            "token": token,
                            "filter": filter,
                        })
                    })
                    .collect::<Vec<_>>();
                (
                    "active".to_string(),
                    serde_json::to_value(active).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaLogsAction::Subscribe { filter } => {
                let token = self.inner.log_bus.subscribe(filter);
                (
                    "subscribe".to_string(),
                    serde_json::to_value::<LogSubscriptionToken>(token).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaLogsAction::Unsubscribe { token } => {
                let removed = self.inner.log_bus.unsubscribe(&token);
                ("unsubscribe".to_string(), JsonValue::Bool(removed))
            }
            AutonomousSolanaLogsAction::Recent {
                cluster,
                program_ids,
                last_n,
                rpc_url,
                cached_only,
            } => {
                let limit = last_n.unwrap_or(25);
                if !(1..=1024).contains(&limit) {
                    return Err(CommandError::user_fixable(
                        "solana_logs_invalid_last_n",
                        format!("last_n must be between 1 and 1024 (got {limit})."),
                    ));
                }

                let filter = LogFilter {
                    cluster,
                    program_ids: program_ids.clone(),
                    include_decoded: true,
                };

                let entries = if cached_only || program_ids.is_empty() {
                    self.inner.log_bus.recent(&filter, limit as usize)
                } else {
                    let rpc_url = rpc_url
                        .or_else(|| self.resolve_rpc_url(cluster))
                        .ok_or_else(|| {
                            CommandError::user_fixable(
                                "solana_logs_no_rpc",
                                "No RPC URL available — start a cluster or provide rpcUrl.",
                            )
                        })?;

                    crate::commands::solana::logs::rpc_source::fetch_recent_and_publish(
                        self.inner.log_source.as_ref(),
                        self.inner.log_bus.as_ref(),
                        cluster,
                        &rpc_url,
                        &program_ids,
                        limit,
                    )?
                };

                (
                    "recent".to_string(),
                    serde_json::json!({
                        "cluster": cluster,
                        "programIds": program_ids,
                        "fetched": entries.len() as u32,
                        "entries": entries,
                    }),
                )
            }
        };

        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn tx(&self, request: AutonomousSolanaTxRequest) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaTxAction::Build { spec } => {
                let plan = self.inner.tx_pipeline.build(spec)?;
                (
                    "build".to_string(),
                    serde_json::to_value::<TxPlan>(plan).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaTxAction::Send { request } => {
                let result = self.inner.tx_pipeline.send(request)?;
                (
                    "send".to_string(),
                    serde_json::to_value::<TxResult>(result).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaTxAction::PriorityFee {
                cluster,
                program_ids,
                target,
                rpc_url,
            } => {
                let estimate = self.inner.tx_pipeline.priority_fee_estimate(
                    cluster,
                    &program_ids,
                    target,
                    rpc_url,
                )?;
                (
                    "priority_fee".to_string(),
                    serde_json::to_value::<FeeEstimate>(estimate).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaTxAction::Cpi {
                program_id,
                instruction,
                args,
            } => {
                let lookup = self
                    .inner
                    .tx_pipeline
                    .resolve_cpi(&program_id, &instruction, &args);
                (
                    "cpi".to_string(),
                    serde_json::to_value::<KnownProgramLookup>(lookup).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn simulate(
        &self,
        request: AutonomousSolanaSimulateRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let result = self.inner.tx_pipeline.simulate(request.request)?;
        let value = serde_json::to_value::<SimulationResult>(result).unwrap_or(JsonValue::Null);
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: "simulate".to_string(),
            value_json,
        })
    }

    fn explain(
        &self,
        request: AutonomousSolanaExplainRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let result = self.inner.tx_pipeline.explain(request.request)?;
        let value = serde_json::to_value::<TxResult>(result).unwrap_or(JsonValue::Null);
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: "explain".to_string(),
            value_json,
        })
    }

    fn alt(&self, request: AutonomousSolanaAltRequest) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaAltAction::Create {
                cluster,
                authority_persona,
                rpc_url,
            } => {
                let result =
                    self.inner
                        .tx_pipeline
                        .alt_create(cluster, &authority_persona, rpc_url)?;
                (
                    "alt_create".to_string(),
                    serde_json::to_value::<AltCreateResult>(result).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaAltAction::Extend {
                cluster,
                alt,
                addresses,
                authority_persona,
                rpc_url,
            } => {
                let result = self.inner.tx_pipeline.alt_extend(
                    cluster,
                    &alt,
                    &addresses,
                    &authority_persona,
                    rpc_url,
                )?;
                (
                    "alt_extend".to_string(),
                    serde_json::to_value::<AltExtendResult>(result).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaAltAction::Resolve {
                addresses,
                candidates,
            } => {
                let report = self.inner.tx_pipeline.alt_suggest(&addresses, &candidates);
                (
                    "alt_resolve".to_string(),
                    serde_json::to_value::<AltResolveReport>(report).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn idl(&self, request: AutonomousSolanaIdlRequest) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaIdlAction::Load { path } => {
                let idl = self.inner.idl_registry.load_file(Path::new(&path))?;
                (
                    "load".to_string(),
                    serde_json::to_value::<Idl>(idl).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaIdlAction::Fetch {
                program_id,
                cluster,
                rpc_url,
            } => {
                let rpc_url = rpc_url
                    .or_else(|| self.resolve_rpc_url(cluster))
                    .ok_or_else(|| {
                        CommandError::user_fixable(
                            "solana_idl_no_rpc",
                            "No RPC URL available — start a cluster or provide rpcUrl.",
                        )
                    })?;
                let idl = self
                    .inner
                    .idl_registry
                    .fetch_on_chain(cluster, &rpc_url, &program_id)?;
                (
                    "fetch".to_string(),
                    serde_json::to_value::<Option<Idl>>(idl).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaIdlAction::Get {
                program_id,
                cluster,
            } => {
                let idl = self.inner.idl_registry.get_cached(&program_id, cluster);
                (
                    "get".to_string(),
                    serde_json::to_value::<Option<Idl>>(idl).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaIdlAction::Watch { path } => {
                let token = self.inner.idl_registry.watch_path(Path::new(&path))?;
                (
                    "watch".to_string(),
                    serde_json::to_value::<IdlSubscriptionToken>(token).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaIdlAction::Unwatch { token } => {
                let ok = self.inner.idl_registry.unwatch(&token)?;
                ("unwatch".to_string(), JsonValue::Bool(ok))
            }
            AutonomousSolanaIdlAction::Drift {
                program_id,
                cluster,
                local_path,
                rpc_url,
            } => {
                let local = self.inner.idl_registry.load_file(Path::new(&local_path))?;
                let rpc_url = rpc_url
                    .or_else(|| self.resolve_rpc_url(cluster))
                    .ok_or_else(|| {
                        CommandError::user_fixable(
                            "solana_idl_no_rpc",
                            "No RPC URL available — start a cluster or provide rpcUrl.",
                        )
                    })?;
                let chain =
                    self.inner
                        .idl_registry
                        .fetch_on_chain(cluster, &rpc_url, &program_id)?;
                let report = idl::drift::classify(&local, chain.as_ref());
                (
                    "drift".to_string(),
                    serde_json::to_value::<DriftReport>(report).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaIdlAction::Publish {
                program_id,
                cluster,
                idl_path,
                authority_keypair_path,
                rpc_url,
                mode,
            } => {
                let runner = idl::publish::SystemAnchorIdlRunner::new();
                let sink = idl::publish::NullProgressSink;
                let report = idl::publish::publish(
                    &runner,
                    &sink,
                    &IdlPublishRequest {
                        program_id,
                        cluster,
                        idl_path,
                        authority_keypair_path,
                        rpc_url,
                        mode,
                    },
                )?;
                (
                    "publish".to_string(),
                    serde_json::to_value::<IdlPublishReport>(report).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn codama(
        &self,
        request: AutonomousSolanaCodamaRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let runner = idl::codama::SystemCodamaRunner::new();
        let report = idl::codama::generate(
            &runner,
            &CodamaGenerationRequest {
                idl_path: request.idl_path,
                targets: request.targets,
                output_dir: request.output_dir,
            },
        )?;
        let value =
            serde_json::to_value::<CodamaGenerationReport>(report).unwrap_or(JsonValue::Null);
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: "generate".to_string(),
            value_json,
        })
    }

    fn pda(&self, request: AutonomousSolanaPdaRequest) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaPdaAction::Derive {
                program_id,
                seeds,
                bump,
            } => {
                let derived = match bump {
                    Some(b) => pda::create_program_address(&program_id, &seeds, b)?,
                    None => pda::find_program_address(&program_id, &seeds)?,
                };
                (
                    "derive".to_string(),
                    serde_json::to_value::<DerivedAddress>(derived).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaPdaAction::Scan { project_root } => {
                let sites = pda::scan(Path::new(&project_root))?;
                (
                    "scan".to_string(),
                    serde_json::to_value::<Vec<PdaSite>>(sites).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaPdaAction::Predict {
                program_id,
                seeds,
                clusters,
            } => {
                let predictions = pda::predict(&program_id, &seeds, &clusters)?;
                (
                    "predict".to_string(),
                    serde_json::to_value::<Vec<ClusterPda>>(predictions).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaPdaAction::AnalyseBump {
                program_id,
                seeds,
                bump,
            } => {
                let analysis = pda::analyse_bump(&program_id, &seeds, bump)?;
                (
                    "analyse_bump".to_string(),
                    serde_json::to_value::<BumpAnalysis>(analysis).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn program(
        &self,
        request: AutonomousSolanaProgramRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaProgramAction::Build {
                manifest_path,
                profile,
                kind,
                program: program_filter,
            } => {
                let runner = program::build::SystemBuildRunner::new();
                let report = program::build::build(
                    &runner,
                    &BuildRequest {
                        manifest_path,
                        profile: profile.unwrap_or_default(),
                        kind,
                        program: program_filter,
                    },
                )?;
                (
                    "build".to_string(),
                    serde_json::to_value::<BuildReport>(report).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaProgramAction::Rollback {
                program_id,
                cluster,
                previous_sha256,
                authority,
                program_archive_root,
                post,
                rpc_url,
            } => {
                let rpc_url = rpc_url
                    .or_else(|| self.resolve_rpc_url(cluster))
                    .ok_or_else(|| {
                        CommandError::user_fixable(
                            "solana_program_rollback_no_rpc",
                            "No RPC URL available — start a cluster or provide rpcUrl.",
                        )
                    })?;
                let req = RollbackRequest {
                    program_id,
                    cluster,
                    rpc_url,
                    previous_sha256,
                    authority,
                    program_archive_root,
                    post: post.unwrap_or_default(),
                };
                let sink = idl::publish::NullProgressSink;
                let result =
                    program::deploy::rollback(self.inner.deploy_services.as_ref(), &sink, &req)?;
                (
                    "rollback".to_string(),
                    serde_json::to_value::<RollbackResult>(result).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn deploy(
        &self,
        request: AutonomousSolanaDeployRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let rpc_url = request
            .rpc_url
            .or_else(|| self.resolve_rpc_url(request.cluster))
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "solana_program_deploy_no_rpc",
                    "No RPC URL available — start a cluster or provide rpcUrl.",
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
            project_root: request.project_root,
            block_on_any_secret: request.block_on_any_secret,
        };
        let sink = idl::publish::NullProgressSink;
        let result = program::deploy::deploy(self.inner.deploy_services.as_ref(), &sink, &spec)?;
        let value = serde_json::to_value::<DeployResult>(result).unwrap_or(JsonValue::Null);
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: "deploy".to_string(),
            value_json,
        })
    }

    fn upgrade_check(
        &self,
        request: AutonomousSolanaUpgradeCheckRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let rpc_url = request
            .rpc_url
            .or_else(|| self.resolve_rpc_url(request.cluster))
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "solana_upgrade_check_no_rpc",
                    "No RPC URL available — start a cluster or provide rpcUrl.",
                )
            })?;
        let chain_idl = self
            .inner
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
        let report = program::upgrade_safety::check(&self.inner.transport, &safety_request)?;
        let value = serde_json::to_value::<UpgradeSafetyReport>(report).unwrap_or(JsonValue::Null);
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: "upgrade_check".to_string(),
            value_json,
        })
    }

    fn squads(
        &self,
        request: AutonomousSolanaSquadsRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let descriptor = program::squads::synthesize(&SquadsProposalRequest {
            program_id: request.program_id,
            cluster: request.cluster,
            multisig_pda: request.multisig_pda,
            buffer: request.buffer,
            spill: request.spill,
            creator: request.creator,
            vault_index: request.vault_index,
            memo: request.memo,
        })?;
        let value =
            serde_json::to_value::<SquadsProposalDescriptor>(descriptor).unwrap_or(JsonValue::Null);
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: "proposal_create".to_string(),
            value_json,
        })
    }

    fn verified_build(
        &self,
        request: AutonomousSolanaVerifiedBuildRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let runner = program::verified_build::SystemVerifiedBuildRunner::new();
        let report = program::verified_build::submit(
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
        )?;
        let value = serde_json::to_value::<VerifiedBuildResult>(report).unwrap_or(JsonValue::Null);
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: "verified_build_submit".to_string(),
            value_json,
        })
    }

    fn audit(
        &self,
        request: AutonomousSolanaAuditRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let sink = NullAuditEventSink;
        let (action_name, value) = match request.action {
            AutonomousSolanaAuditAction::Static {
                project_root,
                rule_ids,
                skip_paths,
            } => {
                let report = self.inner.audit_engine.run_static_lints(
                    &StaticLintRequest {
                        project_root,
                        rule_ids,
                        skip_paths,
                    },
                    &sink,
                )?;
                (
                    "audit_static".to_string(),
                    serde_json::to_value::<StaticLintReport>(report).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaAuditAction::External {
                project_root,
                analyzer,
                timeout_s,
            } => {
                let report = self.inner.audit_engine.run_external_analyzer(
                    &ExternalAnalyzerRequest {
                        project_root,
                        analyzer,
                        timeout_s,
                    },
                    &sink,
                )?;
                (
                    "audit_external".to_string(),
                    serde_json::to_value::<ExternalAnalyzerReport>(report)
                        .unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaAuditAction::Fuzz {
                project_root,
                target,
                duration_s,
                corpus,
                baseline_coverage_lines,
            } => {
                let report = self.inner.audit_engine.run_fuzz(
                    &FuzzRequest {
                        project_root,
                        target,
                        duration_s,
                        corpus,
                        baseline_coverage_lines,
                    },
                    &sink,
                )?;
                (
                    "audit_fuzz".to_string(),
                    serde_json::to_value::<FuzzReport>(report).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaAuditAction::FuzzScaffold {
                project_root,
                target,
                idl_path,
                overwrite,
            } => {
                let result =
                    self.inner
                        .audit_engine
                        .generate_fuzz_harness(&TridentHarnessRequest {
                            project_root,
                            target,
                            idl_path,
                            overwrite,
                        })?;
                (
                    "audit_fuzz_scaffold".to_string(),
                    serde_json::to_value::<TridentHarnessResult>(result).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaAuditAction::Coverage {
                project_root,
                package,
                test_filter,
                lcov_path,
                instruction_names,
                timeout_s,
            } => {
                let report = self.inner.audit_engine.run_coverage(
                    &CoverageRequest {
                        project_root,
                        package,
                        test_filter,
                        lcov_path,
                        instruction_names,
                        timeout_s,
                    },
                    &sink,
                )?;
                (
                    "audit_coverage".to_string(),
                    serde_json::to_value::<CoverageReport>(report).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn indexer(
        &self,
        request: AutonomousSolanaIndexerRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaIndexerAction::Scaffold {
                kind,
                idl_path,
                output_dir,
                project_slug,
                overwrite,
                rpc_url,
            } => {
                let result = indexer::scaffold(
                    self.inner.idl_registry.as_ref(),
                    &ScaffoldRequest {
                        kind,
                        idl_path,
                        output_dir,
                        project_slug,
                        overwrite,
                        rpc_url,
                    },
                )?;
                (
                    "scaffold".to_string(),
                    serde_json::to_value::<ScaffoldResult>(result).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaIndexerAction::Run {
                cluster,
                program_ids,
                last_n,
                rpc_url,
            } => {
                let report = indexer::run_local(
                    self.inner.log_source.as_ref(),
                    Arc::clone(&self.inner.log_bus),
                    &IndexerRunRequest {
                        cluster,
                        program_ids,
                        last_n: last_n.unwrap_or(25),
                        rpc_url,
                    },
                    |kind| self.resolve_rpc_url(kind),
                )?;
                (
                    "run".to_string(),
                    serde_json::to_value::<IndexerRunReport>(report).unwrap_or(JsonValue::Null),
                )
            }
        };

        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn replay(
        &self,
        request: AutonomousSolanaReplayRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let sink = NullAuditEventSink;
        let (action_name, value) = match request.action {
            AutonomousSolanaReplayAction::List => {
                let descriptors: Vec<ExploitDescriptor> = self
                    .inner
                    .audit_engine
                    .library()
                    .all()
                    .into_iter()
                    .cloned()
                    .collect();
                (
                    "replay_list".to_string(),
                    serde_json::to_value::<Vec<ExploitDescriptor>>(descriptors)
                        .unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaReplayAction::Run {
                exploit,
                target_program,
                cluster,
                rpc_url,
                dry_run,
                snapshot_slot,
            } => {
                let rpc_url = rpc_url.or_else(|| self.resolve_rpc_url(cluster));
                let report = self.inner.audit_engine.run_replay(
                    &ReplayRequest {
                        exploit,
                        target_program,
                        cluster,
                        rpc_url,
                        dry_run,
                        snapshot_slot,
                    },
                    &sink,
                )?;
                (
                    "replay_run".to_string(),
                    serde_json::to_value::<ReplayReport>(report).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn secrets(
        &self,
        request: AutonomousSolanaSecretsRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaSecretsAction::Scan { request } => {
                let report = secrets::scan_project(&request)?;
                (
                    "scan".to_string(),
                    serde_json::to_value::<SecretScanReport>(report).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaSecretsAction::Patterns => {
                let patterns = secrets::builtin_patterns();
                (
                    "patterns".to_string(),
                    serde_json::to_value(patterns).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaSecretsAction::Scope => {
                let report = secrets::check_scope(&self.inner.personas)?;
                (
                    "scope".to_string(),
                    serde_json::to_value::<ScopeCheckReport>(report).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn drift(
        &self,
        request: AutonomousSolanaDriftRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaDriftAction::Tracked => {
                let tracked = drift::builtin_tracked_programs();
                (
                    "tracked".to_string(),
                    serde_json::to_value::<Vec<TrackedProgram>>(tracked).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaDriftAction::Check { request } => {
                let report = drift::check(&self.inner.transport, &self.inner.router, &request)?;
                (
                    "check".to_string(),
                    serde_json::to_value::<ClusterDriftReport>(report).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn cost(&self, request: AutonomousSolanaCostRequest) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaCostAction::Snapshot { request } => {
                let args = request.unwrap_or_default();
                let snap = cost::snapshot(
                    &args,
                    &self.inner.cost_ledger,
                    &self.inner.router,
                    self.inner.cost_provider_runner.as_ref(),
                )?;
                (
                    "snapshot".to_string(),
                    serde_json::to_value::<CostSnapshot>(snap).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaCostAction::Record { record } => {
                self.inner.cost_ledger.record(record);
                ("record".to_string(), JsonValue::Bool(true))
            }
            AutonomousSolanaCostAction::Reset => {
                self.inner.cost_ledger.clear();
                ("reset".to_string(), JsonValue::Bool(true))
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn docs(&self, request: AutonomousSolanaDocsRequest) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaDocsAction::Catalog => {
                let catalog = crate::commands::solana::builtin_doc_catalog();
                (
                    "catalog".to_string(),
                    serde_json::to_value::<Vec<DocSnippet>>(catalog).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaDocsAction::Tool { tool } => {
                let snippets = crate::commands::solana::doc_snippets_for(&tool);
                (
                    "tool".to_string(),
                    serde_json::to_value::<Vec<DocSnippet>>(snippets).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }
}

/// No-op executor. Returns `policy_denied` for every action so environments
/// without a registered `SolanaState` (unit tests, autonomous runtime built
/// off a bare repo) still surface a useful error.
#[derive(Debug, Default)]
pub struct UnavailableSolanaExecutor;

impl SolanaExecutor for UnavailableSolanaExecutor {
    fn cluster(
        &self,
        _request: AutonomousSolanaClusterRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana actions require the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn logs(&self, _request: AutonomousSolanaLogsRequest) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana log streaming requires the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn tx(&self, _request: AutonomousSolanaTxRequest) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana tx pipeline requires the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn simulate(
        &self,
        _request: AutonomousSolanaSimulateRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana simulate requires the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn explain(
        &self,
        _request: AutonomousSolanaExplainRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana explain requires the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn alt(&self, _request: AutonomousSolanaAltRequest) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana ALT actions require the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn idl(&self, _request: AutonomousSolanaIdlRequest) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana IDL actions require the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn codama(
        &self,
        _request: AutonomousSolanaCodamaRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana Codama codegen requires the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn pda(&self, _request: AutonomousSolanaPdaRequest) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana PDA actions require the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn program(
        &self,
        _request: AutonomousSolanaProgramRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana program build/rollback requires the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn deploy(
        &self,
        _request: AutonomousSolanaDeployRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana deploy requires the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn upgrade_check(
        &self,
        _request: AutonomousSolanaUpgradeCheckRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana upgrade check requires the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn squads(
        &self,
        _request: AutonomousSolanaSquadsRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        // Squads proposal synthesis is pure (no validator dependency) so
        // we can just run it directly without a SolanaState. This matches
        // the user-visible expectation that an offline workbench can still
        // produce a proposal payload to hand off.
        let descriptor = program::squads::synthesize(&SquadsProposalRequest {
            program_id: _request.program_id,
            cluster: _request.cluster,
            multisig_pda: _request.multisig_pda,
            buffer: _request.buffer,
            spill: _request.spill,
            creator: _request.creator,
            vault_index: _request.vault_index,
            memo: _request.memo,
        })?;
        let value =
            serde_json::to_value::<SquadsProposalDescriptor>(descriptor).unwrap_or(JsonValue::Null);
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: "proposal_create".to_string(),
            value_json,
        })
    }

    fn verified_build(
        &self,
        _request: AutonomousSolanaVerifiedBuildRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana verified-build submission requires the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn audit(
        &self,
        request: AutonomousSolanaAuditRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        // Every audit surface is filesystem-only; drive a local engine
        // so the autonomous runtime can still run lints + harness scaffolds
        // even without a registered SolanaState.
        let sink = NullAuditEventSink;
        let engine = AuditEngine::system();
        let (action_name, value) = match request.action {
            AutonomousSolanaAuditAction::Static {
                project_root,
                rule_ids,
                skip_paths,
            } => {
                let report = engine.run_static_lints(
                    &StaticLintRequest {
                        project_root,
                        rule_ids,
                        skip_paths,
                    },
                    &sink,
                )?;
                (
                    "audit_static".to_string(),
                    serde_json::to_value::<StaticLintReport>(report).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaAuditAction::External {
                project_root,
                analyzer,
                timeout_s,
            } => {
                let report = engine.run_external_analyzer(
                    &ExternalAnalyzerRequest {
                        project_root,
                        analyzer,
                        timeout_s,
                    },
                    &sink,
                )?;
                (
                    "audit_external".to_string(),
                    serde_json::to_value::<ExternalAnalyzerReport>(report)
                        .unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaAuditAction::Fuzz {
                project_root,
                target,
                duration_s,
                corpus,
                baseline_coverage_lines,
            } => {
                let report = engine.run_fuzz(
                    &FuzzRequest {
                        project_root,
                        target,
                        duration_s,
                        corpus,
                        baseline_coverage_lines,
                    },
                    &sink,
                )?;
                (
                    "audit_fuzz".to_string(),
                    serde_json::to_value::<FuzzReport>(report).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaAuditAction::FuzzScaffold {
                project_root,
                target,
                idl_path,
                overwrite,
            } => {
                let result = engine.generate_fuzz_harness(&TridentHarnessRequest {
                    project_root,
                    target,
                    idl_path,
                    overwrite,
                })?;
                (
                    "audit_fuzz_scaffold".to_string(),
                    serde_json::to_value::<TridentHarnessResult>(result).unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaAuditAction::Coverage {
                project_root,
                package,
                test_filter,
                lcov_path,
                instruction_names,
                timeout_s,
            } => {
                let report = engine.run_coverage(
                    &CoverageRequest {
                        project_root,
                        package,
                        test_filter,
                        lcov_path,
                        instruction_names,
                        timeout_s,
                    },
                    &sink,
                )?;
                (
                    "audit_coverage".to_string(),
                    serde_json::to_value::<CoverageReport>(report).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn indexer(
        &self,
        _request: AutonomousSolanaIndexerRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Solana indexer scaffolds require the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn replay(
        &self,
        request: AutonomousSolanaReplayRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        // The replay library is pure data; the runner is a stub until a
        // SolanaState is wired. Catalogue lookups still work offline.
        let sink = NullAuditEventSink;
        let engine = AuditEngine::system();
        let (action_name, value) = match request.action {
            AutonomousSolanaReplayAction::List => {
                let descriptors: Vec<ExploitDescriptor> =
                    engine.library().all().into_iter().cloned().collect();
                (
                    "replay_list".to_string(),
                    serde_json::to_value::<Vec<ExploitDescriptor>>(descriptors)
                        .unwrap_or(JsonValue::Null),
                )
            }
            AutonomousSolanaReplayAction::Run {
                exploit,
                target_program,
                cluster,
                rpc_url,
                dry_run,
                snapshot_slot,
            } => {
                let report = engine.run_replay(
                    &ReplayRequest {
                        exploit,
                        target_program,
                        cluster,
                        rpc_url,
                        dry_run,
                        snapshot_slot,
                    },
                    &sink,
                )?;
                (
                    "replay_run".to_string(),
                    serde_json::to_value::<ReplayReport>(report).unwrap_or(JsonValue::Null),
                )
            }
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }

    fn secrets(
        &self,
        request: AutonomousSolanaSecretsRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        // The filesystem scanner is pure — run it even without a
        // SolanaState. Scope checks need a persona store and get
        // denied instead.
        match request.action {
            AutonomousSolanaSecretsAction::Scan { request } => {
                let report = secrets::scan_project(&request)?;
                let value =
                    serde_json::to_value::<SecretScanReport>(report).unwrap_or(JsonValue::Null);
                let value_json =
                    serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
                Ok(AutonomousSolanaOutput {
                    action: "scan".to_string(),
                    value_json,
                })
            }
            AutonomousSolanaSecretsAction::Patterns => {
                let value =
                    serde_json::to_value(secrets::builtin_patterns()).unwrap_or(JsonValue::Null);
                let value_json =
                    serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
                Ok(AutonomousSolanaOutput {
                    action: "patterns".to_string(),
                    value_json,
                })
            }
            AutonomousSolanaSecretsAction::Scope => Err(CommandError::policy_denied(
                "Persona scope check requires the desktop runtime; no SolanaState is wired.",
            )),
        }
    }

    fn drift(
        &self,
        request: AutonomousSolanaDriftRequest,
    ) -> CommandResult<AutonomousSolanaOutput> {
        match request.action {
            AutonomousSolanaDriftAction::Tracked => {
                let value =
                    serde_json::to_value::<Vec<TrackedProgram>>(drift::builtin_tracked_programs())
                        .unwrap_or(JsonValue::Null);
                let value_json =
                    serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
                Ok(AutonomousSolanaOutput {
                    action: "tracked".to_string(),
                    value_json,
                })
            }
            AutonomousSolanaDriftAction::Check { .. } => Err(CommandError::policy_denied(
                "Cluster drift check requires the desktop runtime; no SolanaState is wired.",
            )),
        }
    }

    fn cost(&self, _request: AutonomousSolanaCostRequest) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(
            "Cost snapshot requires the desktop runtime; no SolanaState is wired.",
        ))
    }

    fn docs(&self, request: AutonomousSolanaDocsRequest) -> CommandResult<AutonomousSolanaOutput> {
        let (action_name, value) = match request.action {
            AutonomousSolanaDocsAction::Catalog => (
                "catalog".to_string(),
                serde_json::to_value::<Vec<DocSnippet>>(
                    crate::commands::solana::builtin_doc_catalog(),
                )
                .unwrap_or(JsonValue::Null),
            ),
            AutonomousSolanaDocsAction::Tool { tool } => (
                "tool".to_string(),
                serde_json::to_value::<Vec<DocSnippet>>(crate::commands::solana::doc_snippets_for(
                    &tool,
                ))
                .unwrap_or(JsonValue::Null),
            ),
        };
        let value_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        Ok(AutonomousSolanaOutput {
            action: action_name,
            value_json,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_executor_denies_every_tool() {
        let exec = UnavailableSolanaExecutor;
        let err = exec
            .cluster(AutonomousSolanaClusterRequest {
                action: AutonomousSolanaClusterAction::Status,
            })
            .unwrap_err();
        assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);

        let err = exec
            .logs(AutonomousSolanaLogsRequest {
                action: AutonomousSolanaLogsAction::Active,
            })
            .unwrap_err();
        assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
    }

    #[test]
    fn state_executor_list_returns_cluster_descriptors() {
        let state = SolanaState::default();
        let exec = StateSolanaExecutor::from_state(&state);
        let out = exec
            .cluster(AutonomousSolanaClusterRequest {
                action: AutonomousSolanaClusterAction::List,
            })
            .unwrap();
        assert_eq!(out.action, "list");
        let parsed: serde_json::Value = serde_json::from_str(&out.value_json).unwrap();
        let descriptors = parsed.as_array().unwrap();
        assert_eq!(descriptors.len(), 4);
    }

    #[test]
    fn state_executor_status_when_idle_reports_not_running() {
        let state = SolanaState::default();
        let exec = StateSolanaExecutor::from_state(&state);
        let out = exec
            .cluster(AutonomousSolanaClusterRequest {
                action: AutonomousSolanaClusterAction::Status,
            })
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out.value_json).unwrap();
        assert_eq!(parsed.get("running").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn snapshot_create_requires_accounts() {
        let state = SolanaState::default();
        let exec = StateSolanaExecutor::from_state(&state);
        let err = exec
            .cluster(AutonomousSolanaClusterRequest {
                action: AutonomousSolanaClusterAction::SnapshotCreate {
                    label: "test".to_string(),
                    accounts: vec![],
                    cluster: Some(ClusterKind::Localnet),
                    rpc_url: Some("http://127.0.0.1:8899".to_string()),
                },
            })
            .unwrap_err();
        assert_eq!(err.code, "solana_snapshot_accounts_empty");
    }

    #[test]
    fn squads_synthesis_works_on_unavailable_executor() {
        let exec = UnavailableSolanaExecutor;
        let valid = bs58::encode([1u8; 32]).into_string();
        let out = exec
            .squads(AutonomousSolanaSquadsRequest {
                program_id: valid.clone(),
                cluster: ClusterKind::Devnet,
                multisig_pda: valid.clone(),
                buffer: valid.clone(),
                spill: valid.clone(),
                creator: valid,
                vault_index: None,
                memo: None,
            })
            .unwrap();
        assert_eq!(out.action, "proposal_create");
        let parsed: serde_json::Value = serde_json::from_str(&out.value_json).unwrap();
        assert!(parsed.get("vaultPda").is_some());
    }

    #[test]
    fn unavailable_executor_blocks_program_deploy_upgrade_check_verified_build() {
        let exec = UnavailableSolanaExecutor;
        for err in [
            exec.program(AutonomousSolanaProgramRequest {
                action: AutonomousSolanaProgramAction::Build {
                    manifest_path: "/nope".into(),
                    profile: None,
                    kind: None,
                    program: None,
                },
            })
            .unwrap_err(),
            exec.deploy(AutonomousSolanaDeployRequest {
                program_id: "X".into(),
                cluster: ClusterKind::Devnet,
                so_path: "/nope".into(),
                authority: DeployAuthority::DirectKeypair {
                    keypair_path: "/nope".into(),
                },
                idl_path: None,
                is_first_deploy: false,
                post: None,
                rpc_url: None,
                project_root: None,
                block_on_any_secret: false,
            })
            .unwrap_err(),
            exec.upgrade_check(AutonomousSolanaUpgradeCheckRequest {
                program_id: "X".into(),
                cluster: ClusterKind::Devnet,
                local_so_path: "/nope".into(),
                expected_authority: "Y".into(),
                local_idl_path: None,
                max_program_size_bytes: None,
                local_so_size_bytes: None,
                rpc_url: None,
            })
            .unwrap_err(),
            exec.verified_build(AutonomousSolanaVerifiedBuildRequest {
                program_id: "X".into(),
                cluster: ClusterKind::Devnet,
                manifest_path: "/nope".into(),
                github_url: "https://github.com/x/y".into(),
                commit_hash: None,
                library_name: None,
                skip_remote_submit: false,
            })
            .unwrap_err(),
        ] {
            assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
        }
    }

    #[test]
    fn action_enum_round_trips_through_serde() {
        let req = AutonomousSolanaClusterRequest {
            action: AutonomousSolanaClusterAction::Start {
                kind: ClusterKind::Localnet,
                opts: StartOpts::default(),
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"action\":\"start\""));
        let decoded: AutonomousSolanaClusterRequest = serde_json::from_str(&json).unwrap();
        match decoded.action {
            AutonomousSolanaClusterAction::Start { kind, .. } => {
                assert_eq!(kind, ClusterKind::Localnet);
            }
            _ => panic!("round trip lost variant"),
        }
    }
}
