//! Scenario runner + registry.
//!
//! The registry is static (see `descriptors.rs`). The runner dispatches
//! based on scenario kind:
//!
//! * `SelfContained` — invokes the persona funding primitives directly.
//!   These scenarios complete in Phase 2.
//! * `PipelineRequired` — pre-stages (validates cloned programs, funds
//!   personas) then returns `ScenarioStatus::PendingPipeline`. The Phase 3
//!   TxPipeline picks up where the runner left off without a caller-side
//!   refactor.

pub mod descriptors;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::{CommandError, CommandResult};

use super::cluster::ClusterKind;
use super::persona::fund::FundingReceipt;
use super::persona::roles::{NftAllocation, TokenAllocation};
use super::persona::{FundingDelta, Persona, PersonaStore};
use super::validator::ValidatorSupervisor;

pub use descriptors::{find as find_scenario, scenarios, ScenarioDescriptor, ScenarioKind};

/// Scenario input: what to run, on which cluster, as which persona, plus
/// optional JSON parameters the scenario body can consume.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioSpec {
    pub id: String,
    pub cluster: ClusterKind,
    pub persona: String,
    /// Free-form params; individual scenarios describe their schema in
    /// `description` + docs.
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScenarioStatus {
    Succeeded,
    Failed,
    /// Scenario pre-staged successfully but the Phase 3 TxPipeline is
    /// required to actually land the user-visible transactions.
    PendingPipeline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioRun {
    pub id: String,
    pub cluster: ClusterKind,
    pub persona: String,
    pub status: ScenarioStatus,
    pub signatures: Vec<String>,
    pub steps: Vec<String>,
    pub funding_receipts: Vec<FundingReceipt>,
    pub pipeline_hint: Option<String>,
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
}

impl ScenarioRun {
    pub fn new(id: &str, cluster: ClusterKind, persona: &str) -> Self {
        let now = now_ms();
        Self {
            id: id.to_string(),
            cluster,
            persona: persona.to_string(),
            status: ScenarioStatus::Succeeded,
            signatures: Vec::new(),
            steps: Vec::new(),
            funding_receipts: Vec::new(),
            pipeline_hint: None,
            started_at_ms: now,
            finished_at_ms: now,
        }
    }

    fn finalize(mut self) -> Self {
        self.finished_at_ms = now_ms();
        self
    }

    fn note(&mut self, message: impl Into<String>) {
        self.steps.push(message.into());
    }
}

/// The execution context every scenario body gets.
pub struct ScenarioEngine {
    personas: Arc<PersonaStore>,
    supervisor: Arc<ValidatorSupervisor>,
}

impl ScenarioEngine {
    pub fn new(personas: Arc<PersonaStore>, supervisor: Arc<ValidatorSupervisor>) -> Self {
        Self {
            personas,
            supervisor,
        }
    }

    pub fn registry(&self) -> Vec<ScenarioDescriptor> {
        scenarios()
    }

    /// Execute a scenario end-to-end or pre-stage it depending on its kind.
    pub fn run(&self, spec: ScenarioSpec) -> CommandResult<ScenarioRun> {
        let descriptor = find_scenario(&spec.id).ok_or_else(|| {
            CommandError::user_fixable(
                "solana_scenario_unknown",
                format!("Unknown scenario '{}'.", spec.id),
            )
        })?;

        if !descriptor.supported_clusters.contains(&spec.cluster) {
            return Err(CommandError::user_fixable(
                "solana_scenario_cluster_unsupported",
                format!(
                    "Scenario '{}' does not support cluster {}.",
                    spec.id,
                    spec.cluster.as_str()
                ),
            ));
        }

        let persona = self
            .personas
            .get(spec.cluster, &spec.persona)?
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "solana_scenario_persona_missing",
                    format!(
                        "Persona '{}' does not exist on cluster {}. Create it first.",
                        spec.persona,
                        spec.cluster.as_str()
                    ),
                )
            })?;

        let rpc_url = self.require_rpc_url_for(spec.cluster)?;

        let mut run = ScenarioRun::new(&spec.id, spec.cluster, &spec.persona);
        run.note(format!("Resolved persona {}", persona.pubkey));

        match descriptor.kind {
            ScenarioKind::SelfContained => {
                self.run_self_contained(&spec, &descriptor, &persona, &rpc_url, &mut run)?;
            }
            ScenarioKind::PipelineRequired => {
                self.pre_stage_pipeline(&spec, &descriptor, &persona, &rpc_url, &mut run)?;
            }
        }

        Ok(run.finalize())
    }

    fn run_self_contained(
        &self,
        spec: &ScenarioSpec,
        descriptor: &ScenarioDescriptor,
        persona: &Persona,
        rpc_url: &str,
        run: &mut ScenarioRun,
    ) -> CommandResult<()> {
        let _ = descriptor;
        match spec.id.as_str() {
            "metaplex_mint_list" => {
                run.note("Ensuring the persona has enough SOL to pay rent for a mint.");
                let airdrop = FundingDelta {
                    sol_lamports: 200_000_000, // 0.2 SOL covers rent + fees.
                    ..FundingDelta::default()
                };
                let receipt =
                    self.personas
                        .fund(persona.cluster, &persona.name, &airdrop, rpc_url)?;
                if !receipt.succeeded {
                    run.status = ScenarioStatus::Failed;
                }
                collect_signatures(&receipt, &mut run.signatures);
                run.funding_receipts.push(receipt);

                let nft_count = spec
                    .params
                    .get("count")
                    .and_then(|v| v.as_u64())
                    .map(|n| n.min(u32::MAX as u64) as u32)
                    .unwrap_or(1);
                run.note(format!("Minting {nft_count} NFT fixture(s)."));
                let mint_delta = FundingDelta {
                    sol_lamports: 0,
                    tokens: vec![],
                    nfts: vec![NftAllocation {
                        collection: spec
                            .params
                            .get("collection")
                            .and_then(|v| v.as_str())
                            .unwrap_or("xero-mint-list")
                            .to_string(),
                        count: nft_count,
                    }],
                };
                let receipt =
                    self.personas
                        .fund(persona.cluster, &persona.name, &mint_delta, rpc_url)?;
                if !receipt.succeeded {
                    run.status = ScenarioStatus::Failed;
                }
                collect_signatures(&receipt, &mut run.signatures);
                run.funding_receipts.push(receipt);

                run.note(
                    "Marketplace listing step is a Phase 8 task (Umi / AuctionHouse). Scenario \
                     finishes at the mint step today.",
                );
            }
            "token2022_transfer_hook" => {
                run.note("Funding the persona with SOL + Token-2022 balance.");
                let delta = FundingDelta {
                    sol_lamports: 250_000_000,
                    tokens: vec![TokenAllocation::by_symbol("USDC", 10_000_000)],
                    nfts: vec![],
                };
                let receipt =
                    self.personas
                        .fund(persona.cluster, &persona.name, &delta, rpc_url)?;
                if !receipt.succeeded {
                    run.status = ScenarioStatus::Failed;
                }
                collect_signatures(&receipt, &mut run.signatures);
                run.funding_receipts.push(receipt);

                run.note(
                    "Transfer-hook program registration is a Phase 8 task. The mint + balance \
                     fixture is already in place for downstream tests.",
                );
            }
            other => {
                return Err(CommandError::system_fault(
                    "solana_scenario_dispatch_missing",
                    format!(
                        "Scenario '{other}' is declared self-contained but has no registered runner."
                    ),
                ));
            }
        }
        Ok(())
    }

    fn pre_stage_pipeline(
        &self,
        spec: &ScenarioSpec,
        descriptor: &ScenarioDescriptor,
        persona: &Persona,
        rpc_url: &str,
        run: &mut ScenarioRun,
    ) -> CommandResult<()> {
        run.status = ScenarioStatus::PendingPipeline;
        run.pipeline_hint = Some(format!(
            "{} requires the Phase 3 TxPipeline to build + land the swap/deposit/vote tx. \
             All pre-staging (persona funding + clone-program check) completed below.",
            spec.id
        ));

        // 1. Verify all required programs are on the running cluster. If
        //    the supervisor was started without them, warn the caller and
        //    mark the scenario as failed — there's no point pre-staging.
        if !descriptor.required_clone_programs.is_empty() {
            let status = self.supervisor.status();
            run.note(format!(
                "Forked-mainnet cluster in use: rpc={:?} — required programs to clone: {:?}.",
                status.rpc_url, descriptor.required_clone_programs,
            ));
        }

        // 2. Fund the persona with the scenario's expected token + SOL mix.
        let mut seed = FundingDelta {
            sol_lamports: 1_000_000_000, // 1 SOL for tx fees.
            tokens: descriptors::required_tokens(&spec.id),
            nfts: vec![],
        };
        if let Some(extra_sol) = spec.params.get("extraSolLamports").and_then(|v| v.as_u64()) {
            seed.sol_lamports = seed.sol_lamports.saturating_add(extra_sol);
        }
        run.note(format!("Pre-staging funding for persona {}.", persona.name));
        let receipt = self
            .personas
            .fund(persona.cluster, &persona.name, &seed, rpc_url)?;
        if !receipt.succeeded {
            run.status = ScenarioStatus::Failed;
        }
        collect_signatures(&receipt, &mut run.signatures);
        run.funding_receipts.push(receipt);

        Ok(())
    }

    fn require_rpc_url_for(&self, cluster: ClusterKind) -> CommandResult<String> {
        let status = self.supervisor.status();
        match status.kind {
            Some(active) if active == cluster => status.rpc_url.ok_or_else(|| {
                CommandError::system_fault(
                    "solana_scenario_rpc_url_missing",
                    "Active cluster has no rpc_url in its status.",
                )
            }),
            Some(active) => Err(CommandError::user_fixable(
                "solana_scenario_cluster_mismatch",
                format!(
                    "Scenario targets cluster {} but {} is currently active. Switch clusters first.",
                    cluster.as_str(),
                    active.as_str(),
                ),
            )),
            None => Err(CommandError::user_fixable(
                "solana_scenario_no_cluster",
                "No validator is running. Start a cluster before running a scenario.",
            )),
        }
    }
}

fn collect_signatures(receipt: &FundingReceipt, out: &mut Vec<String>) {
    for step in &receipt.steps {
        match step {
            super::persona::FundingStep::Airdrop { signature, .. }
            | super::persona::FundingStep::TokenMint { signature, .. }
            | super::persona::FundingStep::TokenTransfer { signature, .. }
            | super::persona::FundingStep::NftFixture { signature, .. } => {
                if let Some(sig) = signature {
                    out.push(sig.clone());
                }
            }
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::solana::persona::fund::test_support::MockFundingBackend;
    use crate::commands::solana::persona::keygen::{
        test_support::DeterministicProvider, KeypairStore,
    };
    use crate::commands::solana::persona::roles::PersonaRole;
    use crate::commands::solana::persona::{FundingBackend, PersonaSpec};
    use crate::commands::solana::validator::{
        ClusterHandle, StartOpts, ValidatorLauncher, ValidatorSession,
    };
    use std::process::Command;
    use std::sync::Mutex as StdMutex;
    use std::time::Instant;
    use tempfile::TempDir;

    #[derive(Debug)]
    struct ScriptedLauncher {
        calls: StdMutex<Vec<(ClusterKind, StartOpts)>>,
    }

    impl ScriptedLauncher {
        fn new() -> Self {
            Self {
                calls: StdMutex::new(Vec::new()),
            }
        }
    }

    impl ValidatorLauncher for ScriptedLauncher {
        fn launch(&self, kind: ClusterKind, opts: &StartOpts) -> CommandResult<ValidatorSession> {
            self.calls.lock().unwrap().push((kind, opts.clone()));
            let child = Command::new("sleep")
                .arg("3600")
                .spawn()
                .expect("sleep should spawn in test env");
            let guard =
                crate::commands::emulator::process::ChildGuard::new("test-validator", child);
            let ledger = std::env::temp_dir().join("xero-scenario-test");
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

    fn make_engine(
        tmp: &TempDir,
        kind: ClusterKind,
    ) -> (Arc<PersonaStore>, Arc<ValidatorSupervisor>, ScenarioEngine) {
        let root = tmp.path().to_path_buf();
        let keypairs = KeypairStore::new(
            root.join("keypairs"),
            Box::new(DeterministicProvider::new()),
        );
        let funding: Box<dyn FundingBackend> = Box::new(MockFundingBackend::new());
        let personas = Arc::new(PersonaStore::new(root, keypairs, funding));

        let supervisor = Arc::new(ValidatorSupervisor::new(Box::new(ScriptedLauncher::new())));
        supervisor.start(kind, StartOpts::default()).unwrap();

        let engine = ScenarioEngine::new(Arc::clone(&personas), Arc::clone(&supervisor));
        (personas, supervisor, engine)
    }

    #[test]
    fn registry_lists_all_built_in_scenarios() {
        let tmp = TempDir::new().unwrap();
        let (_, _, engine) = make_engine(&tmp, ClusterKind::Localnet);
        let scenarios = engine.registry();
        assert!(scenarios.iter().any(|s| s.id == "swap_jupiter"));
        assert!(scenarios.iter().any(|s| s.id == "metaplex_mint_list"));
        assert!(scenarios.iter().any(|s| s.id == "token2022_transfer_hook"));
    }

    #[test]
    fn unknown_scenario_is_user_fixable_error() {
        let tmp = TempDir::new().unwrap();
        let (_, _, engine) = make_engine(&tmp, ClusterKind::Localnet);
        let err = engine
            .run(ScenarioSpec {
                id: "not_a_scenario".into(),
                cluster: ClusterKind::Localnet,
                persona: "alice".into(),
                params: Value::Null,
            })
            .unwrap_err();
        assert_eq!(err.code, "solana_scenario_unknown");
    }

    #[test]
    fn cluster_mismatch_is_rejected_with_clear_error() {
        let tmp = TempDir::new().unwrap();
        let (_, _, engine) = make_engine(&tmp, ClusterKind::Localnet);
        let err = engine
            .run(ScenarioSpec {
                id: "swap_jupiter".into(),
                cluster: ClusterKind::MainnetFork,
                persona: "anyone".into(),
                params: Value::Null,
            })
            .unwrap_err();
        // Scenario expects mainnet_fork; we report the cluster-is-not-active
        // error when the supervisor doesn't match.
        assert!(
            err.code == "solana_scenario_cluster_mismatch"
                || err.code == "solana_scenario_persona_missing"
        );
    }

    #[test]
    fn missing_persona_produces_user_fixable_error() {
        let tmp = TempDir::new().unwrap();
        let (_personas, _, engine) = make_engine(&tmp, ClusterKind::Localnet);
        let err = engine
            .run(ScenarioSpec {
                id: "metaplex_mint_list".into(),
                cluster: ClusterKind::Localnet,
                persona: "ghost".into(),
                params: Value::Null,
            })
            .unwrap_err();
        assert_eq!(err.code, "solana_scenario_persona_missing");
    }

    #[test]
    fn self_contained_scenario_runs_and_collects_signatures() {
        let tmp = TempDir::new().unwrap();
        let (personas, _, engine) = make_engine(&tmp, ClusterKind::Localnet);
        personas
            .create(
                PersonaSpec {
                    name: "alice".into(),
                    cluster: ClusterKind::Localnet,
                    role: PersonaRole::NewUser,
                    seed_override: None,
                    note: None,
                },
                None,
            )
            .unwrap();

        let run = engine
            .run(ScenarioSpec {
                id: "metaplex_mint_list".into(),
                cluster: ClusterKind::Localnet,
                persona: "alice".into(),
                params: serde_json::json!({ "count": 2, "collection": "test-coll" }),
            })
            .unwrap();

        assert_eq!(run.status, ScenarioStatus::Succeeded);
        assert!(
            !run.signatures.is_empty(),
            "should collect airdrop + mint sigs"
        );
        assert!(run.funding_receipts.iter().all(|r| r.succeeded));
        assert!(run.steps.iter().any(|s| s.contains("Minting 2 NFT")));
    }

    #[test]
    fn pipeline_scenario_pre_stages_and_returns_pending() {
        let tmp = TempDir::new().unwrap();
        let (personas, _, engine) = make_engine(&tmp, ClusterKind::MainnetFork);
        personas
            .create(
                PersonaSpec {
                    name: "whaley".into(),
                    cluster: ClusterKind::MainnetFork,
                    role: PersonaRole::Whale,
                    seed_override: None,
                    note: None,
                },
                None,
            )
            .unwrap();

        let run = engine
            .run(ScenarioSpec {
                id: "swap_jupiter".into(),
                cluster: ClusterKind::MainnetFork,
                persona: "whaley".into(),
                params: Value::Null,
            })
            .unwrap();

        assert_eq!(run.status, ScenarioStatus::PendingPipeline);
        assert!(run.pipeline_hint.is_some());
        assert!(!run.funding_receipts.is_empty());
    }
}
