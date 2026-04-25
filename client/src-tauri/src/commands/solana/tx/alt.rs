//! Address Lookup Table (ALT) helpers.
//!
//! ALTs live on-chain at the Solana `AddressLookupTab1e1111111111111111111111111111`
//! program. Full create/extend/deactivate/close flow requires a signer
//! (the ALT authority), so we shell out to the user's `solana` CLI when
//! they have it installed — matching the pattern `fund.rs` uses for
//! `spl-token`. The read-path resolver (`suggest_entries`) is pure: it
//! takes the transaction's flat address set and a list of candidate ALT
//! addresses, and returns which candidate covers the most addresses.
//!
//! When the CLI is absent we return a `user_fixable` error telling the
//! caller to install the Solana CLI — the same pattern as the SPL token
//! fixtures.

use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

pub const ADDRESS_LOOKUP_TABLE_PROGRAM: &str = "AddressLookupTab1e1111111111111111111111111";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AltCreateResult {
    pub pubkey: String,
    pub signature: Option<String>,
    pub stdout: String,
    pub stderr_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AltExtendResult {
    pub alt: String,
    pub added: Vec<String>,
    pub signature: Option<String>,
    pub stdout: String,
    pub stderr_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AltSuggestion {
    pub alt: String,
    pub covered: Vec<String>,
    pub missing: Vec<String>,
    pub score: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AltResolveReport {
    pub addresses: Vec<String>,
    pub suggestions: Vec<AltSuggestion>,
    pub recommended: Option<String>,
    pub uncovered: Vec<String>,
}

/// Narrow trait so tests can stub `solana address-lookup-table create` /
/// `extend` without shelling out. Production uses `SolanaCliRunner`.
pub trait AltRunner: Send + Sync + std::fmt::Debug {
    fn create(&self, rpc_url: &str, authority_keypair: &str) -> CommandResult<AltCreateResult>;

    fn extend(
        &self,
        rpc_url: &str,
        alt: &str,
        addresses: &[String],
        authority_keypair: &str,
    ) -> CommandResult<AltExtendResult>;
}

#[derive(Debug, Default)]
pub struct SolanaCliRunner {
    /// Cached path to `solana`, resolved on first use via `toolchain.rs`.
    resolved: Mutex<Option<Option<String>>>,
}

impl SolanaCliRunner {
    pub fn new() -> Self {
        Self::default()
    }

    fn solana_path(&self) -> Option<String> {
        let mut guard = self.resolved.lock().expect("alt runner poisoned");
        if guard.is_none() {
            let path = crate::commands::solana::toolchain::resolve_binary("solana")
                .map(|p| p.display().to_string());
            *guard = Some(path);
        }
        guard.clone().flatten()
    }
}

impl AltRunner for SolanaCliRunner {
    fn create(&self, rpc_url: &str, authority_keypair: &str) -> CommandResult<AltCreateResult> {
        let solana = self.solana_path().ok_or_else(|| {
            CommandError::user_fixable(
                "solana_alt_cli_missing",
                "solana CLI is not on PATH — install the Solana CLI to create ALTs.",
            )
        })?;
        let mut cmd = Command::new(&solana);
        cmd.arg("address-lookup-table")
            .arg("create")
            .arg("--url")
            .arg(rpc_url)
            .arg("--keypair")
            .arg(authority_keypair)
            .arg("--output")
            .arg("json")
            .stdin(Stdio::null());
        crate::commands::solana::toolchain::augment_command(&mut cmd);
        let output = cmd.output().map_err(|err| {
            CommandError::retryable(
                "solana_alt_create_spawn",
                format!("solana address-lookup-table create failed: {err}"),
            )
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(CommandError::user_fixable(
                "solana_alt_create_failed",
                format!("ALT create failed: {}", trim(&stderr)),
            ));
        }
        let (pubkey, signature) = parse_alt_create_output(&stdout);
        let stderr_excerpt = if stderr.is_empty() {
            None
        } else {
            Some(trim(&stderr))
        };
        Ok(AltCreateResult {
            pubkey: pubkey.unwrap_or_default(),
            signature,
            stdout,
            stderr_excerpt,
        })
    }

    fn extend(
        &self,
        rpc_url: &str,
        alt: &str,
        addresses: &[String],
        authority_keypair: &str,
    ) -> CommandResult<AltExtendResult> {
        if addresses.is_empty() {
            return Err(CommandError::user_fixable(
                "solana_alt_extend_empty",
                "Address list is empty — provide at least one address to extend.",
            ));
        }
        let solana = self.solana_path().ok_or_else(|| {
            CommandError::user_fixable(
                "solana_alt_cli_missing",
                "solana CLI is not on PATH — install the Solana CLI to extend ALTs.",
            )
        })?;
        let mut cmd = Command::new(&solana);
        cmd.arg("address-lookup-table")
            .arg("extend")
            .arg(alt)
            .arg("--url")
            .arg(rpc_url)
            .arg("--keypair")
            .arg(authority_keypair)
            .arg("--addresses")
            .arg(addresses.join(","))
            .stdin(Stdio::null());
        crate::commands::solana::toolchain::augment_command(&mut cmd);
        let output = cmd.output().map_err(|err| {
            CommandError::retryable(
                "solana_alt_extend_spawn",
                format!("solana address-lookup-table extend failed: {err}"),
            )
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(CommandError::user_fixable(
                "solana_alt_extend_failed",
                format!("ALT extend failed: {}", trim(&stderr)),
            ));
        }
        Ok(AltExtendResult {
            alt: alt.to_string(),
            added: addresses.to_vec(),
            signature: parse_signature_line(&stdout),
            stdout,
            stderr_excerpt: if stderr.is_empty() {
                None
            } else {
                Some(trim(&stderr))
            },
        })
    }
}

/// Rank ALT candidates by how many tx addresses they'd cover. Returns a
/// sorted suggestion list plus the leader (if any).
pub fn suggest_entries(tx_addresses: &[String], candidates: &[AltCandidate]) -> AltResolveReport {
    let mut suggestions: Vec<AltSuggestion> = candidates
        .iter()
        .map(|candidate| {
            let mut covered = Vec::new();
            let mut missing = Vec::new();
            for address in tx_addresses {
                if candidate.contents.iter().any(|c| c == address) {
                    covered.push(address.clone());
                } else {
                    missing.push(address.clone());
                }
            }
            let score = covered.len();
            AltSuggestion {
                alt: candidate.pubkey.clone(),
                covered,
                missing,
                score,
            }
        })
        .collect();
    suggestions.sort_by(|a, b| b.score.cmp(&a.score).then(a.alt.cmp(&b.alt)));
    let recommended = suggestions
        .first()
        .filter(|s| s.score > 0)
        .map(|s| s.alt.clone());
    let uncovered = if let Some(best) = suggestions.first() {
        best.missing.clone()
    } else {
        tx_addresses.to_vec()
    };
    AltResolveReport {
        addresses: tx_addresses.to_vec(),
        suggestions,
        recommended,
        uncovered,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AltCandidate {
    pub pubkey: String,
    pub contents: Vec<String>,
}

fn parse_alt_create_output(stdout: &str) -> (Option<String>, Option<String>) {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout) {
        let pk = value
            .get("address")
            .or_else(|| value.pointer("/commandOutput/address"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let sig = value
            .get("signature")
            .or_else(|| value.pointer("/commandOutput/signature"))
            .and_then(|v| v.as_str())
            .map(String::from);
        return (pk, sig);
    }
    let mut pubkey = None;
    let mut signature = None;
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Lookup Table:") {
            pubkey = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Signature:") {
            signature = Some(rest.trim().to_string());
        }
    }
    (pubkey, signature)
}

fn parse_signature_line(stdout: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(sig) = value.get("signature").and_then(|v| v.as_str()) {
            return Some(sig.to_string());
        }
    }
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Signature:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn trim(message: &str) -> String {
    const MAX: usize = 600;
    if message.len() <= MAX {
        return message.to_string();
    }
    let mut truncated = message.to_string();
    truncated.truncate(MAX);
    truncated.push_str("… (truncated)");
    truncated
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::Mutex;

    type AltExtendCall = (String, String, Vec<String>, String);

    #[derive(Debug, Default)]
    pub struct MockAltRunner {
        pub create_calls: Mutex<Vec<(String, String)>>,
        pub extend_calls: Mutex<Vec<AltExtendCall>>,
        pub fail: Mutex<bool>,
        pub canned_alt: Mutex<Option<String>>,
    }

    impl MockAltRunner {
        pub fn new() -> Self {
            Self {
                canned_alt: Mutex::new(Some(
                    "AltMockAddress1111111111111111111111111111".to_string(),
                )),
                ..Self::default()
            }
        }
    }

    impl AltRunner for MockAltRunner {
        fn create(&self, rpc_url: &str, authority_keypair: &str) -> CommandResult<AltCreateResult> {
            self.create_calls
                .lock()
                .unwrap()
                .push((rpc_url.to_string(), authority_keypair.to_string()));
            if *self.fail.lock().unwrap() {
                return Err(CommandError::retryable(
                    "solana_alt_create_failed",
                    "mock failure",
                ));
            }
            let pubkey = self.canned_alt.lock().unwrap().clone().unwrap_or_default();
            Ok(AltCreateResult {
                pubkey,
                signature: Some("mock-sig".to_string()),
                stdout: String::new(),
                stderr_excerpt: None,
            })
        }

        fn extend(
            &self,
            rpc_url: &str,
            alt: &str,
            addresses: &[String],
            authority_keypair: &str,
        ) -> CommandResult<AltExtendResult> {
            self.extend_calls.lock().unwrap().push((
                rpc_url.to_string(),
                alt.to_string(),
                addresses.to_vec(),
                authority_keypair.to_string(),
            ));
            if *self.fail.lock().unwrap() {
                return Err(CommandError::retryable(
                    "solana_alt_extend_failed",
                    "mock failure",
                ));
            }
            Ok(AltExtendResult {
                alt: alt.to_string(),
                added: addresses.to_vec(),
                signature: Some("mock-extend-sig".to_string()),
                stdout: String::new(),
                stderr_excerpt: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggest_entries_prefers_higher_coverage() {
        let tx = vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
        ];
        let alt_a = AltCandidate {
            pubkey: "alt-a".into(),
            contents: vec!["A".into(), "B".into()],
        };
        let alt_b = AltCandidate {
            pubkey: "alt-b".into(),
            contents: vec!["A".into(), "B".into(), "C".into()],
        };
        let report = suggest_entries(&tx, &[alt_a, alt_b]);
        assert_eq!(report.recommended.as_deref(), Some("alt-b"));
        assert_eq!(report.suggestions[0].score, 3);
        assert_eq!(report.uncovered, vec!["D".to_string()]);
    }

    #[test]
    fn suggest_entries_with_no_candidates_marks_all_uncovered() {
        let tx = vec!["X".to_string(), "Y".to_string()];
        let report = suggest_entries(&tx, &[]);
        assert!(report.recommended.is_none());
        assert_eq!(report.uncovered, tx);
    }

    #[test]
    fn parse_alt_create_output_handles_json() {
        let (pk, sig) = parse_alt_create_output(r#"{"address":"AltPub123","signature":"s1"}"#);
        assert_eq!(pk.as_deref(), Some("AltPub123"));
        assert_eq!(sig.as_deref(), Some("s1"));
    }

    #[test]
    fn parse_alt_create_output_handles_human_output() {
        let (pk, sig) =
            parse_alt_create_output("Lookup Table: MyLookupTable111111111\nSignature: mysig\n");
        assert_eq!(pk.as_deref(), Some("MyLookupTable111111111"));
        assert_eq!(sig.as_deref(), Some("mysig"));
    }

    #[test]
    fn mock_runner_records_create_and_extend_calls() {
        use test_support::MockAltRunner;
        let runner = MockAltRunner::new();
        let create = runner
            .create("http://rpc.test", "/tmp/keypair.json")
            .unwrap();
        assert_eq!(create.pubkey, "AltMockAddress1111111111111111111111111111");
        let _ = runner
            .extend(
                "http://rpc.test",
                &create.pubkey,
                &["AddrX".into()],
                "/tmp/keypair.json",
            )
            .unwrap();
        assert_eq!(runner.create_calls.lock().unwrap().len(), 1);
        assert_eq!(runner.extend_calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn mock_runner_extend_errors_on_empty_list_via_production_runner() {
        let runner = SolanaCliRunner::new();
        let err = runner
            .extend("http://rpc.test", "alt", &[], "/tmp/keypair.json")
            .unwrap_err();
        assert_eq!(err.code, "solana_alt_extend_empty");
    }
}
