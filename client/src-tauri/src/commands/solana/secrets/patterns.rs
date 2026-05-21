//! Built-in pattern library for the Solana-focused secrets scanner.
//!
//! Every entry here has a stable `rule_id` so the frontend and the
//! deploy gate can route on it without string-matching messages. New
//! patterns are safe to append; existing ids must not change.
//!
//! The scanner in `scan.rs` consumes these patterns; the list is
//! kept here so contributors have a single, readable catalogue.

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::SecretSeverity;

/// What a pattern *is*. The scanner uses this to pick the right
/// evidence/extraction strategy:
///
/// * `SolanaKeypairJson` — JSON array of 64 bytes; we also open the
///   file and check the byte count to cut false-positives like Cargo
///   lockfiles that happen to contain big JSON arrays.
/// * `Regex` — a standard regex search over each line; the first
///   capture group (when present) is used as the "evidence" for the
///   finding.
/// * `LiteralMarker` — a fixed substring match. Cheap and precise for
///   tokens like "Bearer" prefixes that require a follow-up regex to
///   extract the actual secret.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretPatternKind {
    SolanaKeypairJson,
    Regex,
    LiteralMarker,
}

/// Serializable pattern descriptor — shipped to the UI so the "Safety"
/// tab can explain exactly which rule fired and link to remediation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecretPattern {
    pub rule_id: String,
    pub title: String,
    pub severity: SecretSeverity,
    pub kind: SecretPatternKind,
    /// Short English description surfaced in the UI.
    pub description: String,
    /// Regex source for `Regex` / `LiteralMarker` patterns. Empty for
    /// `SolanaKeypairJson` (structural check).
    pub pattern: String,
    /// Filename globs this pattern cares about. Empty = match every
    /// text file under the scan root.
    pub file_globs: Vec<String>,
    /// Remediation hint surfaced in the UI + appended to the
    /// agent-facing error message.
    pub remediation: String,
    /// Doc URL the Phase 9 "doc-grounded prompt" feature injects into
    /// the agent catalog.
    pub reference_url: Option<String>,
}

/// The built-in catalogue. Ordering is stable: scanners walk the list
/// in order and dedupe on `(rule_id, file, line)`.
pub fn builtin_patterns() -> Vec<SecretPattern> {
    vec![
        SecretPattern {
            rule_id: "solana_keypair_id_json".into(),
            title: "Solana keypair JSON file".into(),
            severity: SecretSeverity::Critical,
            kind: SecretPatternKind::SolanaKeypairJson,
            description:
                "Raw Solana keypair (`solana-keygen` JSON array format). Check this does not hold a \
                 mainnet upgrade authority before committing."
                    .into(),
            pattern: String::new(),
            file_globs: vec![
                "**/id.json".into(),
                "**/*-keypair.json".into(),
                "**/keypair.json".into(),
                "**/authority.json".into(),
            ],
            remediation:
                "Move the keypair out of the project tree, rotate the authority, and add the path to \
                 `.gitignore`. Use a Squads vault for mainnet authorities."
                    .into(),
            reference_url: Some(
                "https://docs.solanalabs.com/cli/wallets/file-system".into(),
            ),
        },
        SecretPattern {
            rule_id: "helius_rpc_api_key".into(),
            title: "Helius RPC API key in URL".into(),
            severity: SecretSeverity::High,
            kind: SecretPatternKind::Regex,
            description:
                "Helius mainnet/devnet RPC URL with an embedded API key. Rotate and load from env."
                    .into(),
            pattern:
                r"https://(?:mainnet|devnet|rpc)\.helius-rpc\.com/(?:\?api-key=|v0/)([a-zA-Z0-9\-]{16,})"
                    .into(),
            file_globs: vec![],
            remediation:
                "Move the key into an environment variable (e.g. `HELIUS_API_KEY`) and read it at \
                 runtime. Never commit raw keys."
                    .into(),
            reference_url: Some("https://docs.helius.dev/rpc".into()),
        },
        SecretPattern {
            rule_id: "triton_rpc_api_key".into(),
            title: "Triton / RPC-pool API key".into(),
            severity: SecretSeverity::High,
            kind: SecretPatternKind::Regex,
            description: "Triton / RPC-pool mainnet URL with an embedded API key.".into(),
            pattern:
                r"https://[a-z0-9-]+\.(?:rpcpool|rpc-pool|triton\.one)\.com/([A-Za-z0-9]{32,})"
                    .into(),
            file_globs: vec![],
            remediation:
                "Route Triton through the RPC router settings or an env var. Do not commit the \
                 keyed URL."
                    .into(),
            reference_url: Some("https://docs.triton.one/".into()),
        },
        SecretPattern {
            rule_id: "quicknode_rpc_api_key".into(),
            title: "QuickNode RPC URL with key".into(),
            severity: SecretSeverity::High,
            kind: SecretPatternKind::Regex,
            description: "QuickNode Solana RPC URL with the tenant secret baked into the path.".into(),
            pattern:
                r"https://[a-z0-9-]+\.solana-[a-z]+\.quiknode\.pro/([A-Za-z0-9]{16,})".into(),
            file_globs: vec![],
            remediation: "Store the QuickNode URL in an env var. Rotate via the QuickNode dashboard.".into(),
            reference_url: Some("https://www.quicknode.com/docs".into()),
        },
        SecretPattern {
            rule_id: "alchemy_rpc_api_key".into(),
            title: "Alchemy Solana RPC key".into(),
            severity: SecretSeverity::High,
            kind: SecretPatternKind::Regex,
            description: "Alchemy Solana RPC URL with embedded app key.".into(),
            pattern:
                r"https://solana-(?:mainnet|devnet)\.g\.alchemy\.com/v2/([A-Za-z0-9_\-]{16,})"
                    .into(),
            file_globs: vec![],
            remediation: "Load Alchemy keys from env; rotate on leak.".into(),
            reference_url: Some("https://docs.alchemy.com".into()),
        },
        SecretPattern {
            rule_id: "privy_app_secret".into(),
            title: "Privy app secret".into(),
            severity: SecretSeverity::High,
            kind: SecretPatternKind::Regex,
            description:
                "Privy app secret (server-side only). Must never appear in frontend source or \
                 `.env` files that get committed."
                    .into(),
            pattern: r#"(?i)privy[_\-]?app[_\-]?secret["'\s:=]+([A-Za-z0-9_\-]{30,})"#.into(),
            file_globs: vec![],
            remediation:
                "Move the secret to your backend-only env (e.g. Vercel/Server env). Rotate via the \
                 Privy dashboard."
                    .into(),
            reference_url: Some("https://docs.privy.io/".into()),
        },
        SecretPattern {
            rule_id: "jito_tip_account_hardcoded".into(),
            title: "Jito tip account hardcoded".into(),
            severity: SecretSeverity::Medium,
            kind: SecretPatternKind::Regex,
            description:
                "Jito tip-account pubkey pinned in source. Prefer rotating through the full tip-account \
                 list at send time so tips are evenly distributed."
                    .into(),
            pattern:
                r#"(?i)(?:jito|tip)[_\-]?account["'\s:=]+"?(96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5|HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe|Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY|DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL|96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5)"#
                    .into(),
            file_globs: vec![],
            remediation:
                "Fetch the tip-account rotation at send time via `solana_priority_fee_estimate`, or \
                 pick one at random per submission."
                    .into(),
            reference_url: Some("https://docs.jito.wtf/lowlatencytxnsend/#tip-amount".into()),
        },
    ]
}

/// Compile a pattern to a `Regex`. The `SolanaKeypairJson` kind has no
/// regex; the scanner handles it structurally.
pub fn compile_regex(pattern: &SecretPattern) -> Option<Regex> {
    match pattern.kind {
        SecretPatternKind::Regex | SecretPatternKind::LiteralMarker => {
            Regex::new(&pattern.pattern).ok()
        }
        SecretPatternKind::SolanaKeypairJson => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_regex_pattern_compiles() {
        for pat in builtin_patterns() {
            match pat.kind {
                SecretPatternKind::Regex | SecretPatternKind::LiteralMarker => {
                    assert!(
                        compile_regex(&pat).is_some(),
                        "bad regex for {}: {}",
                        pat.rule_id,
                        pat.pattern
                    );
                }
                SecretPatternKind::SolanaKeypairJson => {}
            }
        }
    }
}
