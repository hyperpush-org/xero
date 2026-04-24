//! Built-in scenario library. Each descriptor captures the workbench's
//! understanding of what a scenario does, what cluster it needs, and which
//! programs/accounts the validator must have cloned before the scenario
//! can run end-to-end.
//!
//! Scenarios fall into two buckets today:
//!
//! * **Self-contained** — the scenario can be carried out entirely with
//!   primitives the workbench already has: persona funding + spl-token
//!   CLI. These execute fully in Phase 2 (`metaplex_mint_list`,
//!   `token2022_transfer_hook`).
//! * **Pipeline-required** — the scenario needs a transaction-building
//!   pipeline (build → simulate → auto-tune CU → land) that only arrives
//!   in Phase 3. We still register the descriptor and pre-stage what we
//!   can (clone programs, fund personas); execution returns a
//!   `PendingPipeline` status so the agent knows to come back after
//!   Phase 3 ships.

use serde::{Deserialize, Serialize};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::persona::roles::{PersonaRole, TokenAllocation};

/// Top-level descriptor — stable, static, JSON-serializable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub supported_clusters: Vec<ClusterKind>,
    pub required_clone_programs: Vec<&'static str>,
    pub required_roles: Vec<PersonaRole>,
    /// Which execution kind the runner will use.
    pub kind: ScenarioKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioKind {
    /// Orchestrated from persona funding + spl-token CLI — runs today.
    SelfContained,
    /// Needs the Phase 3 TxPipeline to build a v0 transaction and land it
    /// on a forked-mainnet cluster. The Phase 2 runner pre-stages and
    /// reports the pipeline requirement.
    PipelineRequired,
}

pub fn scenarios() -> Vec<ScenarioDescriptor> {
    vec![
        ScenarioDescriptor {
            id: "swap_jupiter",
            label: "Jupiter swap",
            description: "Execute a mid-size swap through the Jupiter aggregator program on a \
                          forked-mainnet cluster.",
            supported_clusters: vec![ClusterKind::MainnetFork],
            required_clone_programs: vec![
                // Jupiter v6 aggregator program.
                "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4",
            ],
            required_roles: vec![PersonaRole::Whale],
            kind: ScenarioKind::PipelineRequired,
        },
        ScenarioDescriptor {
            id: "add_liquidity_orca",
            label: "Orca add-liquidity",
            description: "Deposit USDC/SOL into an Orca Whirlpool on a forked-mainnet cluster.",
            supported_clusters: vec![ClusterKind::MainnetFork],
            required_clone_programs: vec![
                // Orca Whirlpool program.
                "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
            ],
            required_roles: vec![PersonaRole::Lp],
            kind: ScenarioKind::PipelineRequired,
        },
        ScenarioDescriptor {
            id: "governance_vote",
            label: "SPL governance vote",
            description: "Cast a governance vote via SPL Governance on a forked-mainnet cluster.",
            supported_clusters: vec![ClusterKind::MainnetFork],
            required_clone_programs: vec![
                // SPL Governance.
                "GovER5Lthms3bLBqWub97yVrMmEogzX7xNjdXpPPCVZw",
            ],
            required_roles: vec![PersonaRole::Voter],
            kind: ScenarioKind::PipelineRequired,
        },
        ScenarioDescriptor {
            id: "liquidation_kamino",
            label: "Kamino liquidation",
            description: "Trigger a Kamino liquidation path on a forked-mainnet cluster.",
            supported_clusters: vec![ClusterKind::MainnetFork],
            required_clone_programs: vec![
                // Kamino Lending.
                "KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD",
            ],
            required_roles: vec![PersonaRole::Liquidator],
            kind: ScenarioKind::PipelineRequired,
        },
        ScenarioDescriptor {
            id: "metaplex_mint_list",
            label: "Metaplex mint + list",
            description: "Mint a 1-of-1 NFT to a persona and list it on a simulated marketplace.",
            supported_clusters: vec![ClusterKind::Localnet, ClusterKind::MainnetFork],
            required_clone_programs: vec![],
            required_roles: vec![PersonaRole::NewUser],
            kind: ScenarioKind::SelfContained,
        },
        ScenarioDescriptor {
            id: "token2022_transfer_hook",
            label: "Token-2022 transfer hook",
            description: "Create a Token-2022 mint and issue a balance to the active persona. \
                          Transfer hooks remain out-of-scope until the Token-2022 codepath lands \
                          in Phase 8, but the mint + balance fixture works today.",
            supported_clusters: vec![ClusterKind::Localnet, ClusterKind::MainnetFork],
            required_clone_programs: vec![],
            required_roles: vec![PersonaRole::NewUser],
            kind: ScenarioKind::SelfContained,
        },
    ]
}

/// Look up a scenario by its stable id. Returns `None` for unknown ids so
/// the Tauri layer can map to a user-fixable error.
pub fn find(id: &str) -> Option<ScenarioDescriptor> {
    scenarios().into_iter().find(|s| s.id == id)
}

/// Well-known token allocations a scenario expects before it runs. Keeps
/// the scenario engine free of hardcoded symbols and lets the frontend
/// render the prerequisites.
pub fn required_tokens(scenario_id: &str) -> Vec<TokenAllocation> {
    match scenario_id {
        "swap_jupiter" => vec![
            TokenAllocation::by_symbol("USDC", 100_000_000_000),
            TokenAllocation::by_symbol("mSOL", 10_000_000_000),
        ],
        "add_liquidity_orca" => vec![TokenAllocation::by_symbol("USDC", 50_000_000_000)],
        "governance_vote" => vec![TokenAllocation::by_symbol("JTO", 10_000_000_000)],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenarios_have_stable_unique_ids() {
        let list = scenarios();
        let mut ids: Vec<&str> = list.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            list.len(),
            "scenario ids must be unique within the registry",
        );
    }

    #[test]
    fn every_scenario_has_a_supported_cluster() {
        for s in scenarios() {
            assert!(
                !s.supported_clusters.is_empty(),
                "scenario {} has no cluster",
                s.id
            );
        }
    }

    #[test]
    fn find_returns_none_for_unknown_scenario() {
        assert!(find("definitely-not-a-scenario").is_none());
        assert!(find("swap_jupiter").is_some());
    }

    #[test]
    fn pipeline_scenarios_all_target_mainnet_fork() {
        // Forked-mainnet is the only cluster where cloning Jupiter / Orca
        // makes sense — pipeline scenarios must advertise that.
        for s in scenarios() {
            if matches!(s.kind, ScenarioKind::PipelineRequired) {
                assert!(
                    s.supported_clusters.contains(&ClusterKind::MainnetFork),
                    "pipeline scenario {} must support mainnet_fork",
                    s.id
                );
            }
        }
    }

    #[test]
    fn required_tokens_known_scenarios_return_nonempty_list() {
        assert!(!required_tokens("swap_jupiter").is_empty());
        assert!(required_tokens("metaplex_mint_list").is_empty());
    }
}
