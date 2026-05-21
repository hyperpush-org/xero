//! Token-2022 extension matrix.
//!
//! The Solana ecosystem's Token-2022 program exposes >16 extensions
//! (transfer hooks, transfer fees, metadata pointers, non-transferable
//! accounts, etc.). Whether any given wallet / SDK actually *handles* a
//! given extension today is moving ground — the numbers here are the
//! current, human-maintained snapshot of what the workbench will warn
//! the developer about when they toggle an extension on.
//!
//! The matrix is bundled as a static JSON manifest (embedded into the
//! binary via `include_str!`) so we can ship without a network call and
//! so deterministic integration tests can assert on the shape. A future
//! update path (Phase 9 cost governance) will point `matrix()` at a
//! cached file that `cargo xtask` can refresh.
//!
//! Every extension → sdk-compat row carries a `supportLevel` and a
//! `remediationHint` the UI renders verbatim. Tests lock the shape by
//! pinning specific rows (e.g. "@solana/wallet-adapter-react must flag
//! transfer_hook as unsupported on versions < 0.15.x").

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

/// Canonical Token-2022 extension names. The `snake_case` form matches
/// what the Solana CLI uses (`spl-token create-token --enable-transfer-hook`)
/// and the `@solana/spl-token` TS package's `ExtensionType` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenExtension {
    TransferFee,
    TransferHook,
    MetadataPointer,
    TokenMetadata,
    InterestBearing,
    NonTransferable,
    PermanentDelegate,
    DefaultAccountState,
    MintCloseAuthority,
    ConfidentialTransfer,
    MemoTransfer,
    CpiGuard,
    ImmutableOwner,
    GroupPointer,
    GroupMemberPointer,
    ScaledUiAmount,
}

impl TokenExtension {
    pub const ALL: [TokenExtension; 16] = [
        TokenExtension::TransferFee,
        TokenExtension::TransferHook,
        TokenExtension::MetadataPointer,
        TokenExtension::TokenMetadata,
        TokenExtension::InterestBearing,
        TokenExtension::NonTransferable,
        TokenExtension::PermanentDelegate,
        TokenExtension::DefaultAccountState,
        TokenExtension::MintCloseAuthority,
        TokenExtension::ConfidentialTransfer,
        TokenExtension::MemoTransfer,
        TokenExtension::CpiGuard,
        TokenExtension::ImmutableOwner,
        TokenExtension::GroupPointer,
        TokenExtension::GroupMemberPointer,
        TokenExtension::ScaledUiAmount,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TokenExtension::TransferFee => "transfer_fee",
            TokenExtension::TransferHook => "transfer_hook",
            TokenExtension::MetadataPointer => "metadata_pointer",
            TokenExtension::TokenMetadata => "token_metadata",
            TokenExtension::InterestBearing => "interest_bearing",
            TokenExtension::NonTransferable => "non_transferable",
            TokenExtension::PermanentDelegate => "permanent_delegate",
            TokenExtension::DefaultAccountState => "default_account_state",
            TokenExtension::MintCloseAuthority => "mint_close_authority",
            TokenExtension::ConfidentialTransfer => "confidential_transfer",
            TokenExtension::MemoTransfer => "memo_transfer",
            TokenExtension::CpiGuard => "cpi_guard",
            TokenExtension::ImmutableOwner => "immutable_owner",
            TokenExtension::GroupPointer => "group_pointer",
            TokenExtension::GroupMemberPointer => "group_member_pointer",
            TokenExtension::ScaledUiAmount => "scaled_ui_amount",
        }
    }
}

/// How well a given SDK/wallet handles an extension right now. `Full`
/// means the integration has explicit code paths; `Partial` means the
/// integration can *see* the extension but doesn't exercise every
/// associated instruction; `Unsupported` means the integration will
/// either refuse or silently mis-handle the extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Full,
    Partial,
    Unsupported,
    Unknown,
}

impl SupportLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            SupportLevel::Full => "full",
            SupportLevel::Partial => "partial",
            SupportLevel::Unsupported => "unsupported",
            SupportLevel::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdkCompat {
    /// e.g. "@solana/spl-token", "@solana/wallet-adapter-react",
    /// "phantom", "backpack".
    pub sdk: String,
    /// Concrete versions the matrix speaks for — the UI surfaces these
    /// verbatim so developers can cross-check against their lockfile.
    pub version_range: String,
    pub support_level: SupportLevel,
    /// Short actionable hint ("upgrade to 0.4.x", "use custom RPC
    /// override", "file won't compile"). Empty string when the support
    /// level is Full and no action is needed.
    #[serde(default)]
    pub remediation_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionEntry {
    pub extension: TokenExtension,
    /// Human-facing label ("Transfer Hook", "Metadata Pointer").
    pub label: String,
    /// One-sentence summary of what the extension does.
    pub summary: String,
    /// SPL-Token feature flag or runtime requirement, if any.
    pub requires_program: String,
    /// Per-SDK/wallet support rows.
    pub sdk_support: Vec<SdkCompat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionMatrix {
    pub manifest_version: String,
    pub generated_at: String,
    pub entries: Vec<ExtensionEntry>,
}

impl ExtensionMatrix {
    pub fn entry(&self, ext: TokenExtension) -> Option<&ExtensionEntry> {
        self.entries.iter().find(|e| e.extension == ext)
    }

    /// Given a set of enabled extensions, return every row that is
    /// either `Unsupported` or `Partial`. The UI renders the result as
    /// a warning banner; empty result = ship it.
    pub fn incompatibilities(&self, enabled: &[TokenExtension]) -> Vec<Incompatibility> {
        let mut out: Vec<Incompatibility> = Vec::new();
        let wanted: std::collections::BTreeSet<TokenExtension> = enabled.iter().copied().collect();
        for entry in &self.entries {
            if !wanted.contains(&entry.extension) {
                continue;
            }
            for sdk in &entry.sdk_support {
                if matches!(
                    sdk.support_level,
                    SupportLevel::Unsupported | SupportLevel::Partial
                ) {
                    out.push(Incompatibility {
                        extension: entry.extension,
                        sdk: sdk.sdk.clone(),
                        version_range: sdk.version_range.clone(),
                        support_level: sdk.support_level,
                        remediation_hint: sdk.remediation_hint.clone(),
                    });
                }
            }
        }
        // Deterministic ordering — extension first, then sdk name.
        out.sort_by(|a, b| {
            a.extension
                .as_str()
                .cmp(b.extension.as_str())
                .then_with(|| a.sdk.cmp(&b.sdk))
        });
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Incompatibility {
    pub extension: TokenExtension,
    pub sdk: String,
    pub version_range: String,
    pub support_level: SupportLevel,
    pub remediation_hint: String,
}

/// Embedded copy of the matrix JSON. Kept in a sibling file so `cargo
/// xtask refresh` (future) can regenerate it from upstream changelogs
/// without touching any Rust.
const EMBEDDED_MATRIX_JSON: &str = include_str!("extension_matrix.json");

static MATRIX: OnceLock<ExtensionMatrix> = OnceLock::new();

/// Return the bundled matrix. Parses once, caches for process lifetime.
pub fn matrix() -> &'static ExtensionMatrix {
    MATRIX.get_or_init(|| {
        parse_matrix(EMBEDDED_MATRIX_JSON).expect("bundled extension matrix must parse")
    })
}

pub fn parse_matrix(json: &str) -> CommandResult<ExtensionMatrix> {
    let mut parsed: ExtensionMatrix = serde_json::from_str(json).map_err(|err| {
        CommandError::system_fault(
            "solana_token_matrix_parse_failed",
            format!("Token-2022 extension matrix JSON is malformed: {err}"),
        )
    })?;
    // Guard against duplicate rows + stable ordering.
    let mut seen: BTreeMap<TokenExtension, ()> = BTreeMap::new();
    for entry in &parsed.entries {
        if seen.insert(entry.extension, ()).is_some() {
            return Err(CommandError::system_fault(
                "solana_token_matrix_duplicate_extension",
                format!(
                    "Extension {:?} appears more than once in the matrix manifest.",
                    entry.extension
                ),
            ));
        }
    }
    parsed
        .entries
        .sort_by(|a, b| a.extension.as_str().cmp(b.extension.as_str()));
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_extensions_produce_no_incompatibilities() {
        let m = matrix();
        assert!(m.incompatibilities(&[]).is_empty());
    }

    #[test]
    fn duplicate_extension_in_manifest_rejected() {
        let bad = r#"{
            "manifestVersion": "x",
            "generatedAt": "2025-01-01",
            "entries": [
                {"extension":"transfer_hook","label":"A","summary":"","requiresProgram":"","sdkSupport":[]},
                {"extension":"transfer_hook","label":"B","summary":"","requiresProgram":"","sdkSupport":[]}
            ]
        }"#;
        let err = parse_matrix(bad).unwrap_err();
        assert_eq!(err.code, "solana_token_matrix_duplicate_extension");
    }
}
