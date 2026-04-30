//! Health-checked JSON-RPC router across a free-tier endpoint pool.
//!
//! The router keeps a per-cluster ordered list of endpoints. A caller asks
//! for a healthy endpoint (`pick_healthy`) and the router returns the first
//! entry that passed its most-recent health check. When a call fails, the
//! caller reports it back via `report_failure(endpoint)` which demotes the
//! endpoint below its peers for the rest of the process lifetime, so the
//! next call lands on the next healthy endpoint — this is the failover the
//! Phase 1 acceptance criteria call for.
//!
//! Health checks are a lightweight `getHealth` RPC call. The check has a
//! short timeout so a dead endpoint doesn't stall the UI.

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::commands::{CommandError, CommandResult};

use super::cluster::ClusterKind;

const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_millis(3_500);
const DEFAULT_USER_AGENT: &str = "xero-solana-workbench/0.1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointSpec {
    /// Free-form identifier, e.g. "solana-mainnet-public" or
    /// "helius-free-mainnet". Unique within a cluster.
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub ws_url: Option<String>,
    /// UI label shown next to latency/status.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub requires_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointHealth {
    pub cluster: ClusterKind,
    pub id: String,
    pub url: String,
    pub label: Option<String>,
    pub healthy: bool,
    pub latency_ms: Option<u64>,
    pub last_error: Option<String>,
    pub last_checked_ms: Option<u64>,
    pub consecutive_failures: u32,
}

#[derive(Debug)]
struct EndpointState {
    spec: EndpointSpec,
    healthy: bool,
    last_latency: Option<Duration>,
    last_error: Option<String>,
    last_checked: Option<Instant>,
    last_checked_wall: Option<u64>,
    consecutive_failures: u32,
}

impl EndpointState {
    fn new(spec: EndpointSpec) -> Self {
        Self {
            spec,
            healthy: false,
            last_latency: None,
            last_error: None,
            last_checked: None,
            last_checked_wall: None,
            consecutive_failures: 0,
        }
    }

    fn snapshot(&self, cluster: ClusterKind) -> EndpointHealth {
        EndpointHealth {
            cluster,
            id: self.spec.id.clone(),
            url: self.spec.url.clone(),
            label: self.spec.label.clone(),
            healthy: self.healthy,
            latency_ms: self.last_latency.map(|d| d.as_millis() as u64),
            last_error: self.last_error.clone(),
            last_checked_ms: self.last_checked_wall,
            consecutive_failures: self.consecutive_failures,
        }
    }
}

#[derive(Debug)]
struct ClusterPool {
    endpoints: Vec<EndpointState>,
}

impl ClusterPool {
    fn from_specs(specs: Vec<EndpointSpec>) -> Self {
        Self {
            endpoints: specs.into_iter().map(EndpointState::new).collect(),
        }
    }
}

pub trait RpcHealthCheck: Send + Sync + std::fmt::Debug {
    /// Returns `Ok(())` if the endpoint responds healthily, else a descriptive
    /// error string (never panic on transport error — map it to Err).
    fn check(&self, url: &str) -> Result<(), String>;
}

#[derive(Debug)]
pub struct HttpHealthCheck {
    client: Client,
}

impl HttpHealthCheck {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(HEALTH_CHECK_TIMEOUT)
            .user_agent(DEFAULT_USER_AGENT)
            .build()
            .expect("http client should build");
        Self { client }
    }
}

impl Default for HttpHealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

impl RpcHealthCheck for HttpHealthCheck {
    fn check(&self, url: &str) -> Result<(), String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getHealth"
        });

        let response = self
            .client
            .post(url)
            .json(&body)
            .send()
            .map_err(|e| format!("transport: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("http {}", response.status().as_u16()));
        }

        let body: serde_json::Value = response.json().map_err(|e| format!("body: {e}"))?;

        // `getHealth` returns "ok" when healthy and an RPC error otherwise.
        if let Some(result) = body.get("result") {
            if result.as_str() == Some("ok") {
                return Ok(());
            }
        }
        if let Some(error) = body.get("error") {
            return Err(error.to_string());
        }
        Err("unexpected response".into())
    }
}

#[derive(Debug)]
pub struct RpcRouter {
    pools: RwLock<HashMap<ClusterKind, ClusterPool>>,
    health_check: Mutex<Box<dyn RpcHealthCheck>>,
}

impl RpcRouter {
    pub fn new_with_default_pool() -> Self {
        let mut pools: HashMap<ClusterKind, ClusterPool> = HashMap::new();
        for cluster in ClusterKind::ALL {
            pools.insert(cluster, ClusterPool::from_specs(default_endpoints(cluster)));
        }
        Self {
            pools: RwLock::new(pools),
            health_check: Mutex::new(Box::new(HttpHealthCheck::new())),
        }
    }

    pub fn with_health_check(mut self, check: Box<dyn RpcHealthCheck>) -> Self {
        self.health_check = Mutex::new(check);
        self
    }

    /// Replace the endpoint list for one cluster. Unknown clusters return an
    /// invalid-request error.
    pub fn set_endpoints(
        &self,
        cluster: ClusterKind,
        endpoints: Vec<EndpointSpec>,
    ) -> CommandResult<()> {
        if endpoints.is_empty() {
            return Err(CommandError::user_fixable(
                "solana_rpc_endpoints_empty",
                "At least one RPC endpoint is required per cluster.",
            ));
        }
        let mut pools = self.pools.write().expect("rpc router pool poisoned");
        pools.insert(cluster, ClusterPool::from_specs(endpoints));
        Ok(())
    }

    /// Snapshot of the most-recent health state across every cluster.
    pub fn snapshot_all(&self) -> Vec<EndpointHealth> {
        let pools = self.pools.read().expect("rpc router pool poisoned");
        let mut out = Vec::new();
        for (cluster, pool) in pools.iter() {
            for endpoint in &pool.endpoints {
                out.push(endpoint.snapshot(*cluster));
            }
        }
        // Deterministic order: cluster, then insertion order within the pool.
        out.sort_by(|a, b| (a.cluster, &a.id).cmp(&(b.cluster, &b.id)));
        out
    }

    /// Run the health check synchronously across every configured endpoint.
    /// Returns the updated snapshot.
    pub fn refresh_health(&self) -> Vec<EndpointHealth> {
        let urls: Vec<(ClusterKind, String, String)> = {
            let pools = self.pools.read().expect("rpc router pool poisoned");
            pools
                .iter()
                .flat_map(|(cluster, pool)| {
                    pool.endpoints
                        .iter()
                        .map(|e| (*cluster, e.spec.id.clone(), e.spec.url.clone()))
                        .collect::<Vec<_>>()
                })
                .collect()
        };

        for (cluster, id, url) in urls {
            let started = Instant::now();
            let outcome = {
                let check = self.health_check.lock().expect("health check poisoned");
                check.check(&url)
            };
            let elapsed = started.elapsed();
            self.apply_result(cluster, &id, outcome, elapsed);
        }

        self.snapshot_all()
    }

    /// Ordered list of every endpoint configured for `cluster`. Used
    /// by callers (e.g. the cost-governance probe) that iterate across
    /// every provider, not just the healthy one.
    pub fn endpoints_for(&self, cluster: ClusterKind) -> Vec<EndpointSpec> {
        let pools = self.pools.read().expect("rpc router pool poisoned");
        pools
            .get(&cluster)
            .map(|pool| pool.endpoints.iter().map(|e| e.spec.clone()).collect())
            .unwrap_or_default()
    }

    /// Returns the first healthy endpoint for `cluster`. Prefers endpoints
    /// that have passed a health check; falls back to the first configured
    /// endpoint when every one has failed so the UI can still surface a URL.
    pub fn pick_healthy(&self, cluster: ClusterKind) -> Option<EndpointSpec> {
        let pools = self.pools.read().expect("rpc router pool poisoned");
        let pool = pools.get(&cluster)?;
        pool.endpoints
            .iter()
            .find(|e| e.healthy)
            .map(|e| e.spec.clone())
            .or_else(|| pool.endpoints.first().map(|e| e.spec.clone()))
    }

    /// Caller reports that `id` (in `cluster`) failed a real request. We
    /// demote it below the healthy peers so the next call lands elsewhere.
    pub fn report_failure(&self, cluster: ClusterKind, id: &str, reason: impl Into<String>) {
        let mut pools = self.pools.write().expect("rpc router pool poisoned");
        let Some(pool) = pools.get_mut(&cluster) else {
            return;
        };
        let reason_text = reason.into();
        for endpoint in &mut pool.endpoints {
            if endpoint.spec.id == id {
                endpoint.healthy = false;
                endpoint.consecutive_failures = endpoint.consecutive_failures.saturating_add(1);
                endpoint.last_error = Some(reason_text.clone());
                endpoint.last_checked = Some(Instant::now());
                endpoint.last_checked_wall = Some(now_millis());
            }
        }
        // Demote failing endpoint to the back of the pool while keeping the
        // rest in declared order.
        pool.endpoints.sort_by_key(|e| u32::from(!e.healthy));
    }

    fn apply_result(
        &self,
        cluster: ClusterKind,
        id: &str,
        outcome: Result<(), String>,
        elapsed: Duration,
    ) {
        let mut pools = self.pools.write().expect("rpc router pool poisoned");
        let Some(pool) = pools.get_mut(&cluster) else {
            return;
        };
        for endpoint in &mut pool.endpoints {
            if endpoint.spec.id != id {
                continue;
            }
            endpoint.last_checked = Some(Instant::now());
            endpoint.last_checked_wall = Some(now_millis());
            match &outcome {
                Ok(()) => {
                    endpoint.healthy = true;
                    endpoint.last_latency = Some(elapsed);
                    endpoint.last_error = None;
                    endpoint.consecutive_failures = 0;
                }
                Err(err) => {
                    endpoint.healthy = false;
                    endpoint.last_error = Some(err.clone());
                    endpoint.consecutive_failures = endpoint.consecutive_failures.saturating_add(1);
                }
            }
        }
    }
}

impl Default for RpcRouter {
    fn default() -> Self {
        Self::new_with_default_pool()
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Free-tier defaults. No paid keys; user can add them via
/// `solana_rpc_endpoints_set` later.
pub fn default_endpoints(cluster: ClusterKind) -> Vec<EndpointSpec> {
    match cluster {
        ClusterKind::Localnet => vec![EndpointSpec {
            id: "localnet".to_string(),
            url: "http://127.0.0.1:8899".to_string(),
            ws_url: Some("ws://127.0.0.1:8900".to_string()),
            label: Some("Local validator".to_string()),
            requires_api_key: false,
        }],
        ClusterKind::MainnetFork => vec![EndpointSpec {
            id: "mainnet-fork".to_string(),
            url: "http://127.0.0.1:8899".to_string(),
            ws_url: Some("ws://127.0.0.1:8900".to_string()),
            label: Some("Forked mainnet validator".to_string()),
            requires_api_key: false,
        }],
        ClusterKind::Devnet => vec![
            EndpointSpec {
                id: "devnet-public".to_string(),
                url: "https://api.devnet.solana.com".to_string(),
                ws_url: Some("wss://api.devnet.solana.com".to_string()),
                label: Some("Solana public devnet".to_string()),
                requires_api_key: false,
            },
            EndpointSpec {
                id: "devnet-helius-free".to_string(),
                url: "https://devnet.helius-rpc.com".to_string(),
                ws_url: None,
                label: Some("Helius free (devnet)".to_string()),
                requires_api_key: false,
            },
        ],
        ClusterKind::Mainnet => vec![
            EndpointSpec {
                id: "mainnet-public".to_string(),
                url: "https://api.mainnet-beta.solana.com".to_string(),
                ws_url: Some("wss://api.mainnet-beta.solana.com".to_string()),
                label: Some("Solana public mainnet".to_string()),
                requires_api_key: false,
            },
            EndpointSpec {
                id: "mainnet-helius-free".to_string(),
                url: "https://mainnet.helius-rpc.com".to_string(),
                ws_url: None,
                label: Some("Helius free (mainnet)".to_string()),
                requires_api_key: false,
            },
            EndpointSpec {
                id: "mainnet-triton-free".to_string(),
                url: "https://solana-mainnet.rpc.extrnode.com".to_string(),
                ws_url: None,
                label: Some("Triton / ExtrNode free".to_string()),
                requires_api_key: false,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Arc;

    #[derive(Debug, Default)]
    struct ScriptedCheck {
        responses: Mutex<HashMap<String, Result<(), String>>>,
    }

    impl ScriptedCheck {
        fn set(&self, url: &str, outcome: Result<(), String>) {
            self.responses
                .lock()
                .unwrap()
                .insert(url.to_string(), outcome);
        }
    }

    impl RpcHealthCheck for ScriptedCheck {
        fn check(&self, url: &str) -> Result<(), String> {
            self.responses
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .unwrap_or(Err("unprepared".into()))
        }
    }

    fn test_router() -> (RpcRouter, Arc<ScriptedCheck>) {
        let check = Arc::new(ScriptedCheck::default());
        let router = RpcRouter {
            pools: RwLock::new(HashMap::new()),
            health_check: Mutex::new(Box::new(ScriptedCheckHandle(Arc::clone(&check)))),
        };
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

    // Newtype so ScriptedCheck can satisfy the `Box<dyn RpcHealthCheck>`
    // requirement while still sharing mutable state with the test body.
    #[derive(Debug)]
    struct ScriptedCheckHandle(Arc<ScriptedCheck>);
    impl RpcHealthCheck for ScriptedCheckHandle {
        fn check(&self, url: &str) -> Result<(), String> {
            self.0.check(url)
        }
    }

    #[test]
    fn default_pool_has_every_cluster() {
        let router = RpcRouter::new_with_default_pool();
        let clusters: HashSet<_> = router.snapshot_all().iter().map(|e| e.cluster).collect();
        for kind in ClusterKind::ALL {
            assert!(clusters.contains(&kind), "missing cluster {kind:?}");
        }
    }

    #[test]
    fn refresh_marks_responsive_endpoints_healthy() {
        let (router, check) = test_router();
        check.set("https://primary.example", Ok(()));
        check.set("https://secondary.example", Err("boom".into()));
        check.set("https://tertiary.example", Err("boom".into()));

        let snap = router.refresh_health();
        let primary = snap.iter().find(|e| e.id == "primary").unwrap();
        assert!(primary.healthy);
        assert_eq!(primary.consecutive_failures, 0);
        let secondary = snap.iter().find(|e| e.id == "secondary").unwrap();
        assert!(!secondary.healthy);
        assert_eq!(secondary.last_error.as_deref(), Some("boom"));
    }

    #[test]
    fn pick_healthy_prefers_first_healthy_endpoint() {
        let (router, check) = test_router();
        check.set("https://primary.example", Err("boom".into()));
        check.set("https://secondary.example", Ok(()));
        check.set("https://tertiary.example", Ok(()));
        router.refresh_health();
        let pick = router.pick_healthy(ClusterKind::Mainnet).unwrap();
        assert_eq!(pick.id, "secondary");
    }

    #[test]
    fn report_failure_demotes_endpoint_so_next_call_fails_over() {
        let (router, check) = test_router();
        check.set("https://primary.example", Ok(()));
        check.set("https://secondary.example", Ok(()));
        check.set("https://tertiary.example", Ok(()));
        router.refresh_health();
        assert_eq!(
            router.pick_healthy(ClusterKind::Mainnet).unwrap().id,
            "primary"
        );
        router.report_failure(ClusterKind::Mainnet, "primary", "500 from upstream");
        let pick = router.pick_healthy(ClusterKind::Mainnet).unwrap();
        assert_ne!(
            pick.id, "primary",
            "primary should not be picked after failure"
        );
    }

    #[test]
    fn set_endpoints_rejects_empty_list() {
        let router = RpcRouter::new_with_default_pool();
        let err = router
            .set_endpoints(ClusterKind::Mainnet, vec![])
            .unwrap_err();
        assert_eq!(err.code, "solana_rpc_endpoints_empty");
    }

    #[test]
    fn default_endpoints_are_free_for_every_cluster() {
        for cluster in ClusterKind::ALL {
            for spec in default_endpoints(cluster) {
                assert!(
                    !spec.requires_api_key,
                    "default endpoint {} must not require a paid key",
                    spec.id
                );
            }
        }
    }
}
