//! Free-tier usage probes.
//!
//! Every probe is a single HTTP GET (and a tiny JSON decode) so this
//! module is safe to call on a 10-second refresh interval. Providers
//! with no documented public usage endpoint return `usage_available:
//! false` instead of an error — that's still useful information for
//! the UI.

use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::rpc_router::EndpointSpec;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(4);
const USER_AGENT: &str = "xero-solana-workbench-cost/0.1";

/// Known provider categories. Scripted tests pass `Unknown` to
/// exercise the fallback path.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    HeliusFree,
    TritonFree,
    QuickNodeFree,
    AlchemyFree,
    SolanaPublic,
    Localnet,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealth {
    Healthy,
    Degraded,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsage {
    pub cluster: ClusterKind,
    pub endpoint_id: String,
    pub endpoint_url: String,
    pub kind: ProviderKind,
    pub health: ProviderHealth,
    /// True when the provider reports a usage figure. Free tiers that
    /// don't publish a usage counter set this false.
    pub usage_available: bool,
    pub requests_last_window: Option<u64>,
    pub quota_limit: Option<u64>,
    pub window_seconds: Option<u64>,
    pub warning: Option<String>,
}

pub fn classify(endpoint: &EndpointSpec) -> ProviderKind {
    let url = endpoint.url.to_ascii_lowercase();
    let id = endpoint.id.to_ascii_lowercase();
    if url.contains("helius") || id.contains("helius") {
        ProviderKind::HeliusFree
    } else if url.contains("triton") || url.contains("extrnode") || id.contains("triton") {
        ProviderKind::TritonFree
    } else if url.contains("quiknode") || url.contains("quicknode") || id.contains("quicknode") {
        ProviderKind::QuickNodeFree
    } else if url.contains("alchemy") {
        ProviderKind::AlchemyFree
    } else if url.contains("mainnet-beta.solana.com")
        || url.contains("devnet.solana.com")
        || url.contains("testnet.solana.com")
    {
        ProviderKind::SolanaPublic
    } else if url.contains("127.0.0.1") || url.contains("localhost") {
        ProviderKind::Localnet
    } else {
        ProviderKind::Unknown
    }
}

#[derive(Debug, Clone)]
pub struct ProviderUsageProbeRequest {
    pub cluster: ClusterKind,
    pub endpoint_id: String,
    pub endpoint_url: String,
    pub kind: ProviderKind,
}

/// Trait so tests can inject scripted usage figures without hitting the
/// network.
pub trait ProviderUsageRunner: Send + Sync + std::fmt::Debug {
    fn probe(&self, request: &ProviderUsageProbeRequest) -> ProviderUsage;
}

/// Production runner. Every provider has its own narrow inspection —
/// `getHealth` for the Solana Labs public endpoints, `getVersion` for
/// Helius free (which is all the public free tier exposes today), and
/// a transport smoke test for everything else.
#[derive(Debug)]
pub struct SystemProviderUsageRunner {
    client: Client,
}

impl SystemProviderUsageRunner {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .expect("http client should build");
        Self { client }
    }
}

impl Default for SystemProviderUsageRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderUsageRunner for SystemProviderUsageRunner {
    fn probe(&self, request: &ProviderUsageProbeRequest) -> ProviderUsage {
        let mut usage = ProviderUsage {
            cluster: request.cluster,
            endpoint_id: request.endpoint_id.clone(),
            endpoint_url: request.endpoint_url.clone(),
            kind: request.kind,
            health: ProviderHealth::Unknown,
            usage_available: false,
            requests_last_window: None,
            quota_limit: None,
            window_seconds: None,
            warning: None,
        };

        match request.kind {
            ProviderKind::Localnet => {
                usage.health = match json_rpc_get_version(&self.client, &request.endpoint_url) {
                    Ok(_) => ProviderHealth::Healthy,
                    Err(_) => ProviderHealth::Unknown,
                };
            }
            ProviderKind::SolanaPublic => {
                usage.health = match json_rpc_get_version(&self.client, &request.endpoint_url) {
                    Ok(_) => ProviderHealth::Healthy,
                    Err(_) => ProviderHealth::Degraded,
                };
                usage.warning = Some(
                    "Solana Labs public endpoint — no per-key usage counter. Rely on local tally."
                        .into(),
                );
            }
            ProviderKind::HeliusFree => {
                // Helius free tier exposes `getVersion` but no usage
                // counter to anonymous callers; when it answers we mark
                // the provider healthy and flag that usage has to come
                // from the Helius dashboard.
                match json_rpc_get_version(&self.client, &request.endpoint_url) {
                    Ok(_) => {
                        usage.health = ProviderHealth::Healthy;
                    }
                    Err(err) => {
                        usage.health = ProviderHealth::Degraded;
                        usage.warning = Some(err);
                    }
                }
                if usage.warning.is_none() {
                    usage.warning = Some(
                        "Helius free — usage counters are dashboard-only. Local tally is \
                         authoritative."
                            .into(),
                    );
                }
            }
            ProviderKind::TritonFree | ProviderKind::QuickNodeFree | ProviderKind::AlchemyFree => {
                match json_rpc_get_version(&self.client, &request.endpoint_url) {
                    Ok(_) => {
                        usage.health = ProviderHealth::Healthy;
                    }
                    Err(err) => {
                        usage.health = ProviderHealth::Degraded;
                        usage.warning = Some(err);
                    }
                }
                usage.warning.get_or_insert_with(|| {
                    "Free-tier provider — per-key quotas are dashboard-only.".into()
                });
            }
            ProviderKind::Unknown => {}
        }

        usage
    }
}

fn json_rpc_get_version(client: &Client, url: &str) -> Result<Value, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getVersion"
    });
    let response = client
        .post(url)
        .json(&body)
        .send()
        .map_err(|err| format!("transport: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("status {}", response.status().as_u16()));
    }
    response
        .json::<Value>()
        .map_err(|err| format!("decode: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_recognises_common_providers() {
        assert_eq!(
            classify(&EndpointSpec {
                id: "helius".into(),
                url: "https://mainnet.helius-rpc.com/".into(),
                ws_url: None,
                label: None,
                requires_api_key: false,
            }),
            ProviderKind::HeliusFree
        );
        assert_eq!(
            classify(&EndpointSpec {
                id: "local".into(),
                url: "http://127.0.0.1:8899".into(),
                ws_url: None,
                label: None,
                requires_api_key: false,
            }),
            ProviderKind::Localnet
        );
        assert_eq!(
            classify(&EndpointSpec {
                id: "mainnet-public".into(),
                url: "https://api.mainnet-beta.solana.com".into(),
                ws_url: None,
                label: None,
                requires_api_key: false,
            }),
            ProviderKind::SolanaPublic
        );
        assert_eq!(
            classify(&EndpointSpec {
                id: "custom".into(),
                url: "https://my-custom.example".into(),
                ws_url: None,
                label: None,
                requires_api_key: false,
            }),
            ProviderKind::Unknown
        );
    }
}
