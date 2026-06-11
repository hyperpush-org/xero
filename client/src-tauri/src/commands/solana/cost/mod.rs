//! Phase 9 — cost governance.
//!
//! Aggregates three data sources into one shipped `CostSnapshot`:
//!
//! 1. **Free-tier provider usage.** Every RPC endpoint in the
//!    `RpcRouter` pool that publishes a public usage endpoint (Helius
//!    free, Triton free) gets polled; we normalise the response into
//!    a `ProviderUsage` record. Providers with no usage endpoint
//!    report `usage_available = false`.
//! 2. **Local tx spend.** The in-process `LocalCostLedger` counts
//!    transactions sent through the workbench, the CUs they consumed,
//!    and the lamports paid (priority fee + base fee). This is
//!    the piece that survives offline work.
//! 3. **Rent estimates.** When the agent or UI asks for a rent-exempt
//!    balance we record the lamports snapshot so the aggregate shows
//!    total rent locked.
//!
//! Every sum is a `u64`; no floating point accounting.

pub mod ledger;
pub mod providers;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::provider_profiles::redact_url;
use crate::commands::solana::rpc_router::RpcRouter;
use crate::commands::CommandResult;

pub use ledger::{LocalCostLedger, LocalCostSummary, TxCostRecord};
pub use providers::{
    ProviderHealth, ProviderKind, ProviderUsage, ProviderUsageProbeRequest, ProviderUsageRunner,
    SystemProviderUsageRunner,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CostSnapshotRequest {
    /// Clusters to include. Defaults to every configured cluster in
    /// the router.
    #[serde(default)]
    pub clusters: Vec<ClusterKind>,
    /// Window in seconds applied to the local ledger rollup. `None`
    /// returns everything since process start.
    #[serde(default)]
    pub window_s: Option<u64>,
    /// Skip the network probe if the caller already polled providers.
    #[serde(default)]
    pub skip_provider_probes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CostSnapshot {
    pub generated_at_ms: u64,
    pub window_s: Option<u64>,
    pub clusters_included: Vec<ClusterKind>,
    pub local: LocalCostSummary,
    pub providers: Vec<ProviderUsage>,
    /// Rolled-up lamport totals across all clusters so the UI doesn't
    /// have to reimplement the arithmetic.
    pub totals: CostTotals,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CostTotals {
    pub lamports_spent: u64,
    pub compute_units_used: u64,
    pub tx_count: u64,
    pub rent_locked_lamports: u64,
    pub providers_healthy: u32,
    pub providers_degraded: u32,
}

/// Assemble the snapshot. Accepts a provider-usage runner so tests
/// can inject scripted responses; production wires `SystemProviderUsageRunner`.
pub fn snapshot(
    request: &CostSnapshotRequest,
    ledger: &Arc<LocalCostLedger>,
    router: &Arc<RpcRouter>,
    runner: &dyn ProviderUsageRunner,
) -> CommandResult<CostSnapshot> {
    let clusters: Vec<ClusterKind> = if request.clusters.is_empty() {
        ClusterKind::ALL.to_vec()
    } else {
        request.clusters.clone()
    };

    let local = ledger.summary(&clusters, request.window_s);

    let mut providers: Vec<ProviderUsage> = Vec::new();
    if !request.skip_provider_probes {
        for cluster in &clusters {
            let endpoints = router.endpoints_for(*cluster);
            for endpoint in endpoints {
                let kind = providers::classify(&endpoint);
                if matches!(kind, ProviderKind::Unknown) {
                    continue;
                }
                let mut usage = runner.probe(&providers::ProviderUsageProbeRequest {
                    cluster: *cluster,
                    endpoint_id: endpoint.id.clone(),
                    endpoint_url: endpoint.url.clone(),
                    kind,
                });
                usage.endpoint_url = redact_url(&usage.endpoint_url);
                providers.push(usage);
            }
        }
    }

    // Deduplicate by (cluster, endpoint_id) and keep deterministic order.
    providers.sort_by(|a, b| {
        a.cluster
            .cmp(&b.cluster)
            .then_with(|| a.endpoint_id.cmp(&b.endpoint_id))
    });
    providers.dedup_by(|a, b| a.cluster == b.cluster && a.endpoint_id == b.endpoint_id);

    let mut totals = CostTotals {
        lamports_spent: local.lamports_spent,
        compute_units_used: local.compute_units_used,
        tx_count: local.tx_count,
        rent_locked_lamports: local.rent_locked_lamports,
        providers_healthy: 0,
        providers_degraded: 0,
    };
    for probe in &providers {
        match probe.health {
            ProviderHealth::Healthy => totals.providers_healthy += 1,
            ProviderHealth::Degraded => totals.providers_degraded += 1,
            ProviderHealth::Unknown => {}
        }
    }

    Ok(CostSnapshot {
        generated_at_ms: ledger::now_ms(),
        window_s: request.window_s,
        clusters_included: clusters,
        local,
        providers,
        totals,
    })
}

#[cfg(test)]
mod tests {
    use super::providers::{ProviderUsageProbeRequest, SystemProviderUsageRunner};
    use super::*;

    #[derive(Debug)]
    struct StaticRunner(ProviderUsage);
    impl ProviderUsageRunner for StaticRunner {
        fn probe(&self, _request: &ProviderUsageProbeRequest) -> ProviderUsage {
            self.0.clone()
        }
    }

    #[test]
    fn snapshot_uses_empty_ledger_when_no_activity() {
        let ledger = Arc::new(LocalCostLedger::new());
        let router = Arc::new(RpcRouter::new_with_default_pool());
        let runner = SystemProviderUsageRunner::new();
        let snap = snapshot(
            &CostSnapshotRequest {
                clusters: vec![ClusterKind::Localnet],
                window_s: None,
                skip_provider_probes: true,
            },
            &ledger,
            &router,
            &runner,
        )
        .unwrap();
        assert_eq!(snap.totals.tx_count, 0);
        assert_eq!(snap.totals.lamports_spent, 0);
        assert_eq!(snap.clusters_included, vec![ClusterKind::Localnet]);
    }

    #[test]
    fn snapshot_rolls_up_ledger_entries_per_cluster() {
        let ledger = Arc::new(LocalCostLedger::new());
        ledger.record(TxCostRecord {
            cluster: ClusterKind::Mainnet,
            signature: "sig-1".into(),
            lamports_fee: 5_000,
            priority_fee_lamports: 1_000,
            compute_units_consumed: 200_000,
            rent_lamports: 0,
            timestamp_ms: ledger::now_ms(),
        });
        ledger.record(TxCostRecord {
            cluster: ClusterKind::Devnet,
            signature: "sig-2".into(),
            lamports_fee: 5_000,
            priority_fee_lamports: 0,
            compute_units_consumed: 50_000,
            rent_lamports: 100_000,
            timestamp_ms: ledger::now_ms(),
        });

        let router = Arc::new(RpcRouter::new_with_default_pool());
        let runner = SystemProviderUsageRunner::new();
        let snap = snapshot(
            &CostSnapshotRequest {
                clusters: vec![ClusterKind::Mainnet, ClusterKind::Devnet],
                window_s: None,
                skip_provider_probes: true,
            },
            &ledger,
            &router,
            &runner,
        )
        .unwrap();
        assert_eq!(snap.totals.tx_count, 2);
        assert_eq!(snap.totals.lamports_spent, 5_000 + 1_000 + 5_000);
        assert_eq!(snap.totals.compute_units_used, 200_000 + 50_000);
        assert_eq!(snap.totals.rent_locked_lamports, 100_000);
    }

    #[test]
    fn snapshot_counts_provider_health_categories() {
        let ledger = Arc::new(LocalCostLedger::new());
        let router = Arc::new(RpcRouter::new_with_default_pool());
        let runner = StaticRunner(ProviderUsage {
            cluster: ClusterKind::Mainnet,
            endpoint_id: "probe".into(),
            endpoint_url: "u".into(),
            kind: ProviderKind::HeliusFree,
            health: ProviderHealth::Healthy,
            usage_available: true,
            requests_last_window: Some(100),
            quota_limit: Some(1_000),
            window_seconds: Some(60),
            warning: None,
        });
        let snap = snapshot(
            &CostSnapshotRequest {
                clusters: vec![ClusterKind::Mainnet],
                window_s: None,
                skip_provider_probes: false,
            },
            &ledger,
            &router,
            &runner,
        )
        .unwrap();
        assert!(snap.totals.providers_healthy >= 1);
    }
}
