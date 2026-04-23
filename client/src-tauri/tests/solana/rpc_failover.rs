//! RPC router failover acceptance tests. These run without a network
//! connection — the router accepts a scripted health-check implementation
//! so we can flip endpoint outcomes mid-run.

use std::sync::Arc;

use super::support::{EndpointSpec, ScriptedHealthCheck, ScriptedHealthCheckHandle};
use cadence_desktop_lib::commands::solana::{ClusterKind, RpcRouter};

fn router_with_three_mainnet_endpoints() -> (RpcRouter, Arc<ScriptedHealthCheck>) {
    let check = Arc::new(ScriptedHealthCheck::new());
    let router = RpcRouter::new_with_default_pool()
        .with_health_check(Box::new(ScriptedHealthCheckHandle(Arc::clone(&check))));
    router
        .set_endpoints(
            ClusterKind::Mainnet,
            vec![
                EndpointSpec {
                    id: "primary".into(),
                    url: "https://primary.example".into(),
                    ws_url: None,
                    label: None,
                    requires_api_key: false,
                },
                EndpointSpec {
                    id: "secondary".into(),
                    url: "https://secondary.example".into(),
                    ws_url: None,
                    label: None,
                    requires_api_key: false,
                },
                EndpointSpec {
                    id: "tertiary".into(),
                    url: "https://tertiary.example".into(),
                    ws_url: None,
                    label: None,
                    requires_api_key: false,
                },
            ],
        )
        .unwrap();
    (router, check)
}

pub fn rpc_router_fails_over_when_primary_endpoint_goes_down() {
    let (router, check) = router_with_three_mainnet_endpoints();
    check.set("https://primary.example", Ok(()));
    check.set("https://secondary.example", Ok(()));
    check.set("https://tertiary.example", Ok(()));

    router.refresh_health();
    assert_eq!(
        router.pick_healthy(ClusterKind::Mainnet).unwrap().id,
        "primary"
    );

    // Mid-session failure — caller reports it to the router. Subsequent
    // picks must route to a different endpoint.
    router.report_failure(ClusterKind::Mainnet, "primary", "500 from upstream");
    let next = router.pick_healthy(ClusterKind::Mainnet).unwrap();
    assert_ne!(
        next.id, "primary",
        "primary must not be picked after failure"
    );
    assert!(matches!(next.id.as_str(), "secondary" | "tertiary"));

    // Health snapshot reflects the failure shape (not healthy, error recorded).
    let snap = router.snapshot_all();
    let primary_health = snap.iter().find(|e| e.id == "primary").unwrap();
    assert!(!primary_health.healthy);
    assert_eq!(primary_health.consecutive_failures, 1);
    assert!(primary_health.last_error.is_some());
}

pub fn rpc_router_recovers_when_primary_endpoint_comes_back() {
    let (router, check) = router_with_three_mainnet_endpoints();

    // Start with the primary broken and the secondary healthy.
    check.set("https://primary.example", Err("boom".into()));
    check.set("https://secondary.example", Ok(()));
    check.set("https://tertiary.example", Err("boom".into()));
    router.refresh_health();
    assert_eq!(
        router.pick_healthy(ClusterKind::Mainnet).unwrap().id,
        "secondary"
    );

    // Primary comes back online.
    check.set("https://primary.example", Ok(()));
    let snap = router.refresh_health();
    let primary = snap.iter().find(|e| e.id == "primary").unwrap();
    assert!(primary.healthy, "primary should recover after next check");
    assert_eq!(primary.consecutive_failures, 0);
}

pub fn rpc_router_set_endpoints_replaces_default_pool() {
    let router = RpcRouter::new_with_default_pool();
    let initial = router.snapshot_all();
    let mainnet_count_before = initial
        .iter()
        .filter(|e| e.cluster == ClusterKind::Mainnet)
        .count();
    assert!(
        mainnet_count_before >= 2,
        "default pool has several mainnet endpoints"
    );

    router
        .set_endpoints(
            ClusterKind::Mainnet,
            vec![EndpointSpec {
                id: "single".into(),
                url: "https://single.example".into(),
                ws_url: None,
                label: Some("the only endpoint".into()),
                requires_api_key: false,
            }],
        )
        .unwrap();

    let after = router.snapshot_all();
    let mainnet_after: Vec<_> = after
        .iter()
        .filter(|e| e.cluster == ClusterKind::Mainnet)
        .collect();
    assert_eq!(mainnet_after.len(), 1);
    assert_eq!(mainnet_after[0].id, "single");
}
