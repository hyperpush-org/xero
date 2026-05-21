//! Built-in tracked-program registry.
//!
//! We keep this list small and curated — these are the programs whose
//! version skew between clusters has bitten real Solana dapps. The
//! list is *not* an allowlist; drift-check handles arbitrary
//! `additional` programs too.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrackedProgram {
    pub label: String,
    pub program_id: String,
    /// Short description — surfaced in the UI so users can decide
    /// whether the drift matters to them.
    pub description: String,
    /// Doc URL the Phase 9 "doc-grounded prompt" pass injects into
    /// the agent catalog.
    pub reference_url: Option<String>,
}

pub fn builtin_tracked_programs() -> Vec<TrackedProgram> {
    vec![
        TrackedProgram {
            label: "Metaplex Token Metadata".into(),
            program_id: "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s".into(),
            description:
                "NFT + fungible metadata standard. Discriminator drift between devnet and mainnet \
                 is the classic failure mode."
                    .into(),
            reference_url: Some("https://developers.metaplex.com/token-metadata".into()),
        },
        TrackedProgram {
            label: "Jupiter Aggregator v6".into(),
            program_id: "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".into(),
            description:
                "Swap router used by most mainnet dapps. Pinned localnet clones drift from mainnet \
                 quickly."
                    .into(),
            reference_url: Some("https://docs.jup.ag".into()),
        },
        TrackedProgram {
            label: "Squads v4".into(),
            program_id: "SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf".into(),
            description:
                "Multisig vault the workbench relies on for mainnet authorities. Pinned v4 only — \
                 v3 vaults are rejected."
                    .into(),
            reference_url: Some("https://docs.squads.so".into()),
        },
        TrackedProgram {
            label: "SPL Governance".into(),
            program_id: "GovER5Lthms3bLBqWub97yVrMmEogzX7xNjdXpPPCVZw".into(),
            description:
                "DAO governance program. Scenarios that replay votes across clusters break when \
                 the discriminator set changes."
                    .into(),
            reference_url: Some("https://github.com/solana-labs/solana-program-library".into()),
        },
        TrackedProgram {
            label: "Token-2022".into(),
            program_id: "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb".into(),
            description:
                "Extensions-supporting token program. Extension support varies per wallet + \
                 cluster version."
                    .into(),
            reference_url: Some("https://spl.solana.com/token-2022".into()),
        },
    ]
}
