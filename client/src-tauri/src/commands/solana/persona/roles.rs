//! Built-in persona roles. Each role captures a reusable funding preset the
//! agent (or user) can apply in a single call — a "whale" arrives with SOL
//! and stable balances set, a "liquidator" with enough SOL to pay fees plus
//! the margin collateral, etc.
//!
//! Role presets describe *intent* as serializable numbers and well-known
//! token symbols. The actual mint addresses are looked up in `MINT_CATALOG`
//! below, so the frontend and the funding code stay decoupled.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Built-in roles. The `Custom` variant is what the agent uses when it
/// synthesizes an ad-hoc persona with no preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum PersonaRole {
    Whale,
    Lp,
    Voter,
    Liquidator,
    NewUser,
    Custom,
}

impl PersonaRole {
    pub const BUILT_IN: [PersonaRole; 5] = [
        PersonaRole::Whale,
        PersonaRole::Lp,
        PersonaRole::Voter,
        PersonaRole::Liquidator,
        PersonaRole::NewUser,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PersonaRole::Whale => "whale",
            PersonaRole::Lp => "lp",
            PersonaRole::Voter => "voter",
            PersonaRole::Liquidator => "liquidator",
            PersonaRole::NewUser => "new_user",
            PersonaRole::Custom => "custom",
        }
    }

    /// Canonical preset — lamports + well-known token balances + NFT count.
    /// Callers can override individual fields when they want a one-off.
    pub fn preset(self) -> RolePreset {
        match self {
            PersonaRole::Whale => RolePreset {
                display_label: "Whale".into(),
                description: "Large SOL + stablecoin balance for stressing liquidity-driven paths."
                    .into(),
                lamports: sol_to_lamports(10_000.0),
                tokens: vec![
                    TokenAllocation::by_symbol("USDC", 1_000_000_000_000), // 1M USDC (6 decimals)
                    TokenAllocation::by_symbol("USDT", 1_000_000_000_000), // 1M USDT (6 decimals)
                    TokenAllocation::by_symbol("mSOL", 5_000_000_000_000), // 5k mSOL (9 decimals)
                ],
                nfts: vec![NftAllocation {
                    collection: "cadence-whale-fixture".to_string(),
                    count: 3,
                }],
            },
            PersonaRole::Lp => RolePreset {
                display_label: "Liquidity Provider".into(),
                description: "Balanced SOL + USDC + USDT position for seeding AMM pools.".into(),
                lamports: sol_to_lamports(2_500.0),
                tokens: vec![
                    TokenAllocation::by_symbol("USDC", 250_000_000_000), // 250k USDC
                    TokenAllocation::by_symbol("USDT", 250_000_000_000),
                    TokenAllocation::by_symbol("BONK", 1_000_000_000_000_000_000), // loose meme slug
                ],
                nfts: vec![],
            },
            PersonaRole::Voter => RolePreset {
                display_label: "Governance Voter".into(),
                description: "Holds governance token plus a small SOL float for tx fees.".into(),
                lamports: sol_to_lamports(25.0),
                tokens: vec![
                    TokenAllocation::by_symbol("JTO", 100_000_000_000), // 100k JTO-ish (6 decimals)
                    TokenAllocation::by_symbol("MNGO", 1_000_000_000),
                ],
                nfts: vec![],
            },
            PersonaRole::Liquidator => RolePreset {
                display_label: "Liquidator".into(),
                description: "Enough collateral to trigger liquidation-bot style paths.".into(),
                lamports: sol_to_lamports(500.0),
                tokens: vec![
                    TokenAllocation::by_symbol("USDC", 100_000_000_000),
                    TokenAllocation::by_symbol("mSOL", 1_000_000_000_000),
                ],
                nfts: vec![],
            },
            PersonaRole::NewUser => RolePreset {
                display_label: "New User".into(),
                description: "Just-arrived-on-chain wallet — small airdrop, no tokens.".into(),
                lamports: sol_to_lamports(0.5),
                tokens: vec![],
                nfts: vec![],
            },
            PersonaRole::Custom => RolePreset {
                display_label: "Custom".into(),
                description: "Empty preset. Provide funding explicitly.".into(),
                lamports: 0,
                tokens: vec![],
                nfts: vec![],
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RolePreset {
    pub display_label: String,
    pub description: String,
    pub lamports: u64,
    pub tokens: Vec<TokenAllocation>,
    pub nfts: Vec<NftAllocation>,
}

/// A token balance target, either by well-known symbol (resolved via the
/// catalog below) or by explicit mint address for one-off tokens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenAllocation {
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub mint: Option<String>,
    /// Raw base-unit amount (decimals already applied — e.g. 1_000_000_000 for
    /// 1000 USDC which has 6 decimals).
    pub amount: u64,
}

impl TokenAllocation {
    pub fn by_symbol(symbol: &str, amount: u64) -> Self {
        Self {
            symbol: Some(symbol.to_string()),
            mint: None,
            amount,
        }
    }

    pub fn by_mint(mint: impl Into<String>, amount: u64) -> Self {
        Self {
            symbol: None,
            mint: Some(mint.into()),
            amount,
        }
    }

    /// Canonicalize: use the symbol catalog to resolve a symbol to a mint
    /// address. Leaves explicit-mint allocations untouched.
    pub fn resolve_mint(&self) -> Option<String> {
        if let Some(mint) = &self.mint {
            return Some(mint.clone());
        }
        if let Some(symbol) = &self.symbol {
            return mint_for_symbol(symbol).map(|m| m.to_string());
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NftAllocation {
    /// Logical collection name — the funding code uses this as the mint-label
    /// prefix when it fabricates fixtures on localnet.
    pub collection: String,
    pub count: u32,
}

/// Curated mainnet mint addresses for the well-known tokens that role
/// presets reference. A missing entry simply means the symbol isn't part of
/// the preset-funded set; the caller can always pass an explicit mint.
pub fn mint_for_symbol(symbol: &str) -> Option<&'static str> {
    static CATALOG: OnceLock<BTreeMap<&'static str, &'static str>> = OnceLock::new();
    let catalog = CATALOG.get_or_init(|| {
        let mut map = BTreeMap::new();
        map.insert("USDC", "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
        map.insert("USDT", "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB");
        map.insert("mSOL", "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So");
        map.insert("BONK", "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263");
        map.insert("JTO", "jtojtomepa8beP8AuQc6eXt5FriJwfFMwQx2v2f9mCL");
        map.insert("MNGO", "MangoCzJ36AjZyKwVj3VnYU4GTonjfVEnJmvvWaxLac");
        map
    });
    catalog.get(symbol).copied()
}

fn sol_to_lamports(sol: f64) -> u64 {
    // 1 SOL = 1_000_000_000 lamports.
    (sol * 1_000_000_000.0).round().max(0.0) as u64
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoleDescriptor {
    pub id: PersonaRole,
    pub preset: RolePreset,
}

pub fn descriptors() -> Vec<RoleDescriptor> {
    PersonaRole::BUILT_IN
        .iter()
        .map(|role| RoleDescriptor {
            id: *role,
            preset: role.preset(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_role_has_a_nonempty_preset() {
        for role in PersonaRole::BUILT_IN {
            let preset = role.preset();
            assert!(!preset.display_label.is_empty(), "role {role:?} label");
            assert!(!preset.description.is_empty(), "role {role:?} desc");
        }
    }

    #[test]
    fn custom_role_has_empty_preset() {
        let preset = PersonaRole::Custom.preset();
        assert_eq!(preset.lamports, 0);
        assert!(preset.tokens.is_empty());
        assert!(preset.nfts.is_empty());
    }

    #[test]
    fn symbol_catalog_resolves_well_known_tokens() {
        assert!(mint_for_symbol("USDC").is_some());
        assert!(mint_for_symbol("BONK").is_some());
        assert!(mint_for_symbol("NOT_A_REAL_TOKEN").is_none());
    }

    #[test]
    fn token_allocation_resolves_from_symbol() {
        let alloc = TokenAllocation::by_symbol("USDC", 100);
        assert_eq!(
            alloc.resolve_mint().as_deref(),
            Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
        );
    }

    #[test]
    fn token_allocation_prefers_explicit_mint() {
        let alloc = TokenAllocation::by_mint("MyMint11111111111111111111111111111", 5);
        assert_eq!(
            alloc.resolve_mint().as_deref(),
            Some("MyMint11111111111111111111111111111")
        );
    }

    #[test]
    fn descriptors_covers_every_builtin_role() {
        let descriptors = descriptors();
        assert_eq!(descriptors.len(), PersonaRole::BUILT_IN.len());
        for role in PersonaRole::BUILT_IN {
            assert!(descriptors.iter().any(|d| d.id == role));
        }
    }

    #[test]
    fn role_serde_uses_snake_case() {
        let json = serde_json::to_string(&PersonaRole::NewUser).unwrap();
        assert_eq!(json, "\"new_user\"");
    }

    #[test]
    fn whale_preset_matches_acceptance_criteria() {
        // Acceptance: "whale" persona with 10k SOL, 1M USDC, 3 Metaplex NFTs.
        let preset = PersonaRole::Whale.preset();
        assert_eq!(preset.lamports, 10_000 * 1_000_000_000);
        assert!(preset
            .tokens
            .iter()
            .any(|t| t.symbol.as_deref() == Some("USDC") && t.amount == 1_000_000_000_000));
        assert_eq!(preset.nfts.iter().map(|n| n.count).sum::<u32>(), 3);
    }
}
