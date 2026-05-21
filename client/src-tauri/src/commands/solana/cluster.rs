//! Cluster identity and descriptors shared between the supervisor and the
//! RPC router.

use serde::{Deserialize, Serialize};

/// Every cluster the workbench understands. Extending this list means
/// extending the RPC router's default pool as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum ClusterKind {
    /// `solana-test-validator` running on localhost.
    Localnet,
    /// Forked-mainnet validator (surfpool or `solana-test-validator --clone`).
    MainnetFork,
    /// Solana devnet — remote-only.
    Devnet,
    /// Solana mainnet-beta — remote-only, read-path default.
    Mainnet,
}

impl ClusterKind {
    pub const ALL: [ClusterKind; 4] = [
        ClusterKind::Localnet,
        ClusterKind::MainnetFork,
        ClusterKind::Devnet,
        ClusterKind::Mainnet,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ClusterKind::Localnet => "localnet",
            ClusterKind::MainnetFork => "mainnet_fork",
            ClusterKind::Devnet => "devnet",
            ClusterKind::Mainnet => "mainnet",
        }
    }

    pub fn is_local(self) -> bool {
        matches!(self, ClusterKind::Localnet | ClusterKind::MainnetFork)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClusterDescriptor {
    pub kind: ClusterKind,
    pub label: &'static str,
    pub startable: bool,
    pub default_rpc_url: &'static str,
}

pub fn descriptors() -> Vec<ClusterDescriptor> {
    vec![
        ClusterDescriptor {
            kind: ClusterKind::Localnet,
            label: "Localnet",
            startable: true,
            default_rpc_url: "http://127.0.0.1:8899",
        },
        ClusterDescriptor {
            kind: ClusterKind::MainnetFork,
            label: "Forked mainnet",
            startable: true,
            default_rpc_url: "http://127.0.0.1:8899",
        },
        ClusterDescriptor {
            kind: ClusterKind::Devnet,
            label: "Devnet",
            startable: false,
            default_rpc_url: "https://api.devnet.solana.com",
        },
        ClusterDescriptor {
            kind: ClusterKind::Mainnet,
            label: "Mainnet-beta",
            startable: false,
            default_rpc_url: "https://api.mainnet-beta.solana.com",
        },
    ]
}
