//! Account-layout diff, `.so` size check, upgrade-authority check.
//!
//! Three independent checks composed into a single report:
//!
//! 1. **Layout diff** — feeds the local-vs-on-chain IDL through the
//!    same drift classifier the IDL panel uses. Any `Breaking` change is
//!    a hard block by default; risky and non-breaking changes surface
//!    as warnings.
//! 2. **`.so` size check** — buffer / ProgramData accounts are sized at
//!    deploy time; an upgrade `.so` larger than the on-chain
//!    ProgramData allocation will fail at land time. We pre-flight by
//!    reading ProgramData via `getAccountInfo` and comparing the size.
//!    A separate hard cap (default 10 MiB, configurable) catches the
//!    case where the deploy gate has no on-chain reference yet.
//! 3. **Upgrade-authority check** — fetches the BPF Upgradeable Loader
//!    program account, derives the ProgramData PDA, decodes the
//!    enum tag + authority pubkey, and confirms it matches the
//!    expected authority (a direct-deploy persona pubkey OR a Squads
//!    vault PDA).
//!
//! All three checks degrade gracefully — a missing on-chain program is
//! reported as `programNotDeployed` (deploy will be a fresh init), not
//! as a fault.

use std::sync::Arc;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::idl::{drift, Idl};
use crate::commands::solana::tx::RpcTransport;
use crate::commands::{CommandError, CommandResult};

/// Solana SBF program max deployable size (BPF loader v3). Each cluster
/// can be configured but in practice this is the widely-quoted default.
pub const PROGRAM_DATA_MAX_BYTES: u64 = 10 * 1024 * 1024;

/// The well-known BPF Upgradeable Loader program id.
pub const BPF_UPGRADEABLE_LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";

/// Discriminator byte for `UpgradeableLoaderState::ProgramData`.
const PROGRAM_DATA_DISCRIMINATOR: u32 = 3;

/// Discriminator byte for `UpgradeableLoaderState::Program` (which
/// stores the ProgramData address).
const PROGRAM_DISCRIMINATOR: u32 = 2;

/// Header bytes preceding the ProgramData payload (4-byte enum tag +
/// 8-byte slot + 1-byte option discriminator + optional 32-byte authority).
const PROGRAMDATA_DATA_OFFSET: usize = 45;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeSafetyRequest {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub rpc_url: String,
    /// Path to the locally-built `.so` whose size will be compared
    /// against the on-chain ProgramData allocation.
    pub local_so_path: String,
    /// Path to the locally-generated IDL whose layout will be diffed
    /// against the on-chain IDL. Optional — when omitted, layout diff
    /// is skipped (e.g. plain `cargo build-sbf` projects).
    #[serde(default)]
    pub local_idl_path: Option<String>,
    /// The on-chain IDL (already fetched). When the caller has it
    /// cached we skip a redundant RPC round-trip. Pass `None` to skip
    /// the layout diff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_idl: Option<Idl>,
    /// Local IDL (already loaded). Optional — same shortcut as
    /// `chain_idl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_idl: Option<Idl>,
    /// The authority that the deploy expects to find on-chain. For a
    /// direct-keypair deploy this is the persona pubkey; for a Squads
    /// deploy this is the Squads vault PDA.
    pub expected_authority: String,
    /// Hard cap on `.so` bytes — overrides `PROGRAM_DATA_MAX_BYTES`
    /// when supplied. Set to 0 to disable the absolute check (the
    /// on-chain comparison still runs).
    #[serde(default)]
    pub max_program_size_bytes: Option<u64>,
    /// `.so` size known to the caller (avoids a stat() round-trip when
    /// the build pipeline already has the value). When omitted, the
    /// checker stats `local_so_path` itself.
    #[serde(default)]
    pub local_so_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityCheckOutcome {
    /// Match.
    Match,
    /// On-chain authority differs — the deploy will fail.
    Mismatch,
    /// Program is immutable (authority cleared).
    Immutable,
    /// Program is not deployed on this cluster yet — first-time deploy.
    ProgramNotDeployed,
    /// Could not decode ProgramData; the gate degrades to a manual review.
    Indeterminate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SizeCheckOutcome {
    Fits,
    /// Exceeds the on-chain ProgramData allocation — the upgrade will
    /// fail at land time.
    OverProgramDataAllocation,
    /// Exceeds the absolute cap (`max_program_size_bytes` /
    /// `PROGRAM_DATA_MAX_BYTES`).
    OverAbsoluteCap,
    /// No on-chain ProgramData yet (first-time deploy); only the
    /// absolute cap is enforced.
    FirstDeploy,
    /// Could not measure — deploy is gated for a manual recheck.
    Indeterminate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityCheck {
    pub outcome: AuthorityCheckOutcome,
    pub expected_authority: String,
    pub on_chain_authority: Option<String>,
    pub program_data_address: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SizeCheck {
    pub outcome: SizeCheckOutcome,
    pub local_so_size_bytes: u64,
    pub on_chain_program_data_bytes: Option<u64>,
    pub absolute_cap_bytes: u64,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LayoutCheck {
    /// `None` when no IDL was provided to diff against.
    pub drift: Option<drift::DriftReport>,
    /// `true` when the caller intentionally skipped the diff (no IDL).
    pub skipped: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeSafetyVerdict {
    /// All checks pass — deploy may proceed.
    Ok,
    /// At least one check produced a warning. Deploy may proceed but
    /// the report should be surfaced to the user.
    Warn,
    /// At least one check is a hard block. Deploy must not proceed.
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeSafetyReport {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub verdict: UpgradeSafetyVerdict,
    pub layout: LayoutCheck,
    pub size: SizeCheck,
    pub authority: AuthorityCheck,
    /// Top-level breaking-change list extracted from `layout.drift` so
    /// the UI doesn't have to re-walk the report. Empty when the
    /// layout check passed or was skipped.
    pub breaking_changes: Vec<drift::DriftChange>,
}

pub fn check(
    transport: &Arc<dyn RpcTransport>,
    request: &UpgradeSafetyRequest,
) -> CommandResult<UpgradeSafetyReport> {
    validate_request(request)?;

    let local_so_size = match request.local_so_size_bytes {
        Some(n) => n,
        None => stat_size(&request.local_so_path)?,
    };

    // Fetch ProgramData + decode authority + allocation.
    let program_data = fetch_program_data(transport.as_ref(), request)?;

    let absolute_cap = request
        .max_program_size_bytes
        .filter(|n| *n > 0)
        .unwrap_or(PROGRAM_DATA_MAX_BYTES);

    let size = compute_size_check(local_so_size, &program_data, absolute_cap);
    let authority = compute_authority_check(&request.expected_authority, &program_data);

    let layout = compute_layout_check(request)?;

    let verdict = aggregate_verdict(&size, &authority, &layout);

    let breaking_changes = layout
        .drift
        .as_ref()
        .map(|d| {
            d.changes
                .iter()
                .filter(|c| matches!(c.severity, drift::DriftSeverity::Breaking))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    Ok(UpgradeSafetyReport {
        program_id: request.program_id.clone(),
        cluster: request.cluster,
        verdict,
        layout,
        size,
        authority,
        breaking_changes,
    })
}

fn validate_request(request: &UpgradeSafetyRequest) -> CommandResult<()> {
    if request.program_id.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_upgrade_check_missing_program_id",
            "program_id is required.",
        ));
    }
    if request.rpc_url.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_upgrade_check_missing_rpc_url",
            "rpc_url is required.",
        ));
    }
    if request.expected_authority.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_upgrade_check_missing_authority",
            "expected_authority is required (persona pubkey or Squads vault PDA).",
        ));
    }
    Ok(())
}

fn stat_size(path: &str) -> CommandResult<u64> {
    let meta = std::fs::metadata(path).map_err(|err| {
        CommandError::user_fixable(
            "solana_upgrade_check_stat_failed",
            format!("Could not stat {path}: {err}"),
        )
    })?;
    Ok(meta.len())
}

#[derive(Debug, Clone, Default)]
struct ProgramDataDecode {
    /// `None` when the program account doesn't exist at all.
    program_data_address: Option<String>,
    /// `None` when the program is deployed but ProgramData couldn't be
    /// fetched / decoded.
    upgrade_authority: Option<String>,
    /// `Some(true)` when the option discriminator was 0 (no authority);
    /// `Some(false)` when an authority was present; `None` when the
    /// payload couldn't be parsed.
    immutable: Option<bool>,
    /// Total bytes of the ProgramData account, including the
    /// `PROGRAMDATA_DATA_OFFSET` header. The deployable `.so` payload
    /// is `total - PROGRAMDATA_DATA_OFFSET` bytes.
    program_data_total_bytes: Option<u64>,
    /// Whether the program account was missing entirely.
    program_missing: bool,
}

fn fetch_program_data(
    transport: &dyn RpcTransport,
    request: &UpgradeSafetyRequest,
) -> CommandResult<ProgramDataDecode> {
    let program = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getAccountInfo",
        "params": [
            request.program_id,
            {"encoding": "base64", "commitment": "confirmed"}
        ],
    });
    let program_response = transport.post(&request.rpc_url, program)?;
    let program_value = program_response
        .pointer("/result/value")
        .cloned()
        .unwrap_or(Value::Null);
    if program_value.is_null() {
        return Ok(ProgramDataDecode {
            program_missing: true,
            ..Default::default()
        });
    }
    let owner = program_value
        .get("owner")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if owner != BPF_UPGRADEABLE_LOADER {
        // Either a non-upgradeable program or someone else owns it; we
        // can't reason about authority and the caller almost certainly
        // wanted a different program id.
        return Ok(ProgramDataDecode {
            program_data_address: None,
            ..Default::default()
        });
    }
    let program_data_address = decode_program_data_address(&program_value)?;
    let pd_value = match &program_data_address {
        Some(addr) => fetch_account(transport, &request.rpc_url, addr)?,
        None => Value::Null,
    };
    if pd_value.is_null() {
        return Ok(ProgramDataDecode {
            program_data_address,
            ..Default::default()
        });
    }
    let pd_bytes = decode_b64_data(&pd_value)?;
    let total_bytes = pd_bytes.len() as u64;
    let (immutable, authority) = decode_program_data_header(&pd_bytes);
    Ok(ProgramDataDecode {
        program_data_address,
        upgrade_authority: authority,
        immutable: Some(immutable),
        program_data_total_bytes: Some(total_bytes),
        program_missing: false,
    })
}

fn fetch_account(
    transport: &dyn RpcTransport,
    rpc_url: &str,
    pubkey: &str,
) -> CommandResult<Value> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getAccountInfo",
        "params": [pubkey, {"encoding": "base64", "commitment": "confirmed"}],
    });
    let response = transport.post(rpc_url, body)?;
    Ok(response
        .pointer("/result/value")
        .cloned()
        .unwrap_or(Value::Null))
}

fn decode_b64_data(account_value: &Value) -> CommandResult<Vec<u8>> {
    let data_tuple = account_value
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            CommandError::system_fault(
                "solana_upgrade_check_bad_account_shape",
                "getAccountInfo data array missing.",
            )
        })?;
    let encoded = data_tuple.first().and_then(|v| v.as_str()).ok_or_else(|| {
        CommandError::system_fault(
            "solana_upgrade_check_bad_account_data",
            "getAccountInfo data array empty.",
        )
    })?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .map_err(|err| {
            CommandError::system_fault(
                "solana_upgrade_check_b64_decode_failed",
                format!("Could not base64-decode account data: {err}"),
            )
        })
}

fn decode_program_data_address(program_value: &Value) -> CommandResult<Option<String>> {
    let bytes = decode_b64_data(program_value)?;
    if bytes.len() < 4 + 32 {
        return Ok(None);
    }
    let tag = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if tag != PROGRAM_DISCRIMINATOR {
        // `Buffer` (0), `Uninitialized` (1), `ProgramData` (3) — none of
        // these belong on a program account.
        return Ok(None);
    }
    let address_bytes = &bytes[4..4 + 32];
    Ok(Some(bs58::encode(address_bytes).into_string()))
}

fn decode_program_data_header(bytes: &[u8]) -> (bool, Option<String>) {
    if bytes.len() < 4 + 8 + 1 {
        return (false, None);
    }
    let tag = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if tag != PROGRAM_DATA_DISCRIMINATOR {
        return (false, None);
    }
    // bytes[4..12] = slot (u64 LE), unused here.
    let opt_tag = bytes[12];
    if opt_tag == 0 {
        // None — immutable.
        return (true, None);
    }
    if bytes.len() < 13 + 32 {
        return (false, None);
    }
    let auth_bytes = &bytes[13..13 + 32];
    (false, Some(bs58::encode(auth_bytes).into_string()))
}

fn compute_size_check(local_so_size: u64, pd: &ProgramDataDecode, absolute_cap: u64) -> SizeCheck {
    let on_chain_payload_bytes = pd
        .program_data_total_bytes
        .map(|t| t.saturating_sub(PROGRAMDATA_DATA_OFFSET as u64));
    let cap_label = if absolute_cap == 0 {
        "no absolute cap".to_string()
    } else {
        format!("{} bytes", absolute_cap)
    };
    if absolute_cap > 0 && local_so_size > absolute_cap {
        return SizeCheck {
            outcome: SizeCheckOutcome::OverAbsoluteCap,
            local_so_size_bytes: local_so_size,
            on_chain_program_data_bytes: on_chain_payload_bytes,
            absolute_cap_bytes: absolute_cap,
            detail: format!("Local .so is {local_so_size} bytes, exceeds {cap_label}."),
        };
    }
    if pd.program_missing || pd.program_data_total_bytes.is_none() {
        return SizeCheck {
            outcome: SizeCheckOutcome::FirstDeploy,
            local_so_size_bytes: local_so_size,
            on_chain_program_data_bytes: None,
            absolute_cap_bytes: absolute_cap,
            detail: format!(
                "No on-chain ProgramData found — first deploy. Local .so is {local_so_size} bytes (cap {cap_label})."
            ),
        };
    }
    let on_chain = on_chain_payload_bytes.unwrap_or(0);
    if local_so_size > on_chain {
        return SizeCheck {
            outcome: SizeCheckOutcome::OverProgramDataAllocation,
            local_so_size_bytes: local_so_size,
            on_chain_program_data_bytes: Some(on_chain),
            absolute_cap_bytes: absolute_cap,
            detail: format!(
                "Local .so is {local_so_size} bytes; on-chain ProgramData payload is {on_chain} bytes — upgrade will fail at land time without `solana program extend-program`."
            ),
        };
    }
    SizeCheck {
        outcome: SizeCheckOutcome::Fits,
        local_so_size_bytes: local_so_size,
        on_chain_program_data_bytes: Some(on_chain),
        absolute_cap_bytes: absolute_cap,
        detail: format!("Local .so {local_so_size}B fits within on-chain ProgramData {on_chain}B."),
    }
}

fn compute_authority_check(expected: &str, pd: &ProgramDataDecode) -> AuthorityCheck {
    if pd.program_missing {
        return AuthorityCheck {
            outcome: AuthorityCheckOutcome::ProgramNotDeployed,
            expected_authority: expected.to_string(),
            on_chain_authority: None,
            program_data_address: pd.program_data_address.clone(),
            detail: "Program not deployed yet — authority will be set at first deploy.".into(),
        };
    }
    if pd.immutable.is_none() {
        return AuthorityCheck {
            outcome: AuthorityCheckOutcome::Indeterminate,
            expected_authority: expected.to_string(),
            on_chain_authority: None,
            program_data_address: pd.program_data_address.clone(),
            detail: "Could not decode ProgramData header.".into(),
        };
    }
    if pd.immutable == Some(true) {
        return AuthorityCheck {
            outcome: AuthorityCheckOutcome::Immutable,
            expected_authority: expected.to_string(),
            on_chain_authority: None,
            program_data_address: pd.program_data_address.clone(),
            detail: "Program authority is None — program is immutable, upgrades are impossible."
                .into(),
        };
    }
    let on_chain = pd.upgrade_authority.clone().unwrap_or_default();
    if on_chain.eq(expected) {
        AuthorityCheck {
            outcome: AuthorityCheckOutcome::Match,
            expected_authority: expected.to_string(),
            on_chain_authority: Some(on_chain),
            program_data_address: pd.program_data_address.clone(),
            detail: "On-chain upgrade authority matches expected.".into(),
        }
    } else {
        AuthorityCheck {
            outcome: AuthorityCheckOutcome::Mismatch,
            expected_authority: expected.to_string(),
            on_chain_authority: Some(on_chain.clone()),
            program_data_address: pd.program_data_address.clone(),
            detail: format!(
                "On-chain authority is {on_chain}; expected {expected}. Deploy will be rejected by the loader."
            ),
        }
    }
}

fn compute_layout_check(request: &UpgradeSafetyRequest) -> CommandResult<LayoutCheck> {
    let local = match request.local_idl.clone() {
        Some(idl) => Some(idl),
        None => match request.local_idl_path.as_ref() {
            Some(path) => Some(load_local_idl(path)?),
            None => None,
        },
    };
    let chain = request.chain_idl.clone();
    let local = match local {
        Some(idl) => idl,
        None => {
            return Ok(LayoutCheck {
                drift: None,
                skipped: true,
                detail:
                    "No local IDL supplied — layout diff skipped (caller is responsible for verifying account compatibility)."
                        .into(),
            });
        }
    };
    let report = drift::classify(&local, chain.as_ref());
    let detail = if report.identical {
        "Local IDL matches on-chain IDL.".to_string()
    } else if report.breaking_count == 0 && report.risky_count == 0 {
        format!(
            "{} non-breaking change(s); upgrade is safe.",
            report.non_breaking_count
        )
    } else if report.breaking_count == 0 {
        format!(
            "{} risky / {} non-breaking change(s); review before deploy.",
            report.risky_count, report.non_breaking_count
        )
    } else {
        format!(
            "{} BREAKING / {} risky / {} non-breaking change(s).",
            report.breaking_count, report.risky_count, report.non_breaking_count
        )
    };
    Ok(LayoutCheck {
        drift: Some(report),
        skipped: false,
        detail,
    })
}

fn load_local_idl(path: &str) -> CommandResult<Idl> {
    let bytes = std::fs::read(path).map_err(|err| {
        CommandError::user_fixable(
            "solana_upgrade_check_idl_read_failed",
            format!("Could not read IDL {path}: {err}"),
        )
    })?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|err| {
        CommandError::user_fixable(
            "solana_upgrade_check_idl_parse_failed",
            format!("IDL {path} is not valid JSON: {err}"),
        )
    })?;
    Ok(Idl::from_value(
        value,
        crate::commands::solana::idl::IdlSource::File {
            path: path.to_string(),
        },
    ))
}

fn aggregate_verdict(
    size: &SizeCheck,
    authority: &AuthorityCheck,
    layout: &LayoutCheck,
) -> UpgradeSafetyVerdict {
    let mut warn = false;
    let mut block = false;

    match size.outcome {
        SizeCheckOutcome::OverAbsoluteCap | SizeCheckOutcome::OverProgramDataAllocation => {
            block = true;
        }
        SizeCheckOutcome::Indeterminate => {
            warn = true;
        }
        SizeCheckOutcome::FirstDeploy | SizeCheckOutcome::Fits => {}
    }

    match authority.outcome {
        AuthorityCheckOutcome::Mismatch | AuthorityCheckOutcome::Immutable => block = true,
        AuthorityCheckOutcome::Indeterminate => warn = true,
        AuthorityCheckOutcome::Match | AuthorityCheckOutcome::ProgramNotDeployed => {}
    }

    if let Some(report) = layout.drift.as_ref() {
        if report.breaking_count > 0 {
            block = true;
        } else if report.risky_count > 0 {
            warn = true;
        }
    }

    if block {
        UpgradeSafetyVerdict::Block
    } else if warn {
        UpgradeSafetyVerdict::Warn
    } else {
        UpgradeSafetyVerdict::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::solana::idl::IdlSource;
    use crate::commands::solana::tx::transport::test_support::ScriptedTransport;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    const TEST_PROGRAM_ID: &str = "11111111111111111111111111111111";
    const TEST_PROGRAM_DATA: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
    const PERSONA: &str = "PersonaPubkeyBase58111111111111111111111111";

    fn make_program_account_data(program_data_address_bytes: [u8; 32]) -> String {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&PROGRAM_DISCRIMINATOR.to_le_bytes());
        bytes.extend_from_slice(&program_data_address_bytes);
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    }

    fn make_program_data_account(authority: Option<[u8; 32]>, payload_size: usize) -> String {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&PROGRAM_DATA_DISCRIMINATOR.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes()); // slot
        match authority {
            Some(a) => {
                bytes.push(1u8);
                bytes.extend_from_slice(&a);
            }
            None => {
                bytes.push(0u8);
                bytes.extend_from_slice(&[0u8; 32]);
            }
        }
        bytes.extend(std::iter::repeat(0u8).take(payload_size));
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    }

    fn script_program(
        transport: &Arc<ScriptedTransport>,
        program_value: serde_json::Value,
        pd_value: serde_json::Value,
    ) {
        // The transport script is keyed by the RPC method, so two
        // sequential getAccountInfo calls (program then ProgramData)
        // both hit the same key. Use the queue facility instead.
        transport.queue("http://rpc.test", "getAccountInfo", program_value);
        transport.queue("http://rpc.test", "getAccountInfo", pd_value);
    }

    fn write_so(tmp: &TempDir, bytes: usize) -> String {
        let path = tmp.path().join("p.so");
        std::fs::write(&path, vec![0u8; bytes]).unwrap();
        path.display().to_string()
    }

    fn pkbytes_persona() -> [u8; 32] {
        let raw = bs58::decode(PERSONA).into_vec().unwrap();
        let mut out = [0u8; 32];
        out.copy_from_slice(&raw[..32]);
        out
    }

    fn make_request(tmp: &TempDir, local_so_size: usize, max: Option<u64>) -> UpgradeSafetyRequest {
        UpgradeSafetyRequest {
            program_id: TEST_PROGRAM_ID.into(),
            cluster: ClusterKind::Devnet,
            rpc_url: "http://rpc.test".into(),
            local_so_path: write_so(tmp, local_so_size),
            local_idl_path: None,
            chain_idl: None,
            local_idl: None,
            expected_authority: PERSONA.into(),
            max_program_size_bytes: max,
            local_so_size_bytes: None,
        }
    }

    fn pd_address_bytes() -> [u8; 32] {
        let mut b = [0u8; 32];
        for (i, x) in b.iter_mut().enumerate() {
            *x = i as u8;
        }
        b
    }

    fn pd_address_b58() -> String {
        bs58::encode(pd_address_bytes()).into_string()
    }

    #[test]
    fn first_time_deploy_when_program_account_missing() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.queue(
            "http://rpc.test",
            "getAccountInfo",
            json!({"result": {"value": null}}),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let tmp = TempDir::new().unwrap();
        let request = make_request(&tmp, 1_024, None);
        let report = check(&transport_dyn, &request).unwrap();
        assert_eq!(report.verdict, UpgradeSafetyVerdict::Ok);
        assert_eq!(report.size.outcome, SizeCheckOutcome::FirstDeploy);
        assert_eq!(
            report.authority.outcome,
            AuthorityCheckOutcome::ProgramNotDeployed
        );
    }

    #[test]
    fn matches_authority_and_size_when_chain_state_aligns() {
        let transport = Arc::new(ScriptedTransport::new());
        let pd_addr = pd_address_bytes();
        script_program(
            &transport,
            json!({
                "result": {
                    "value": {
                        "data": [make_program_account_data(pd_addr), "base64"],
                        "executable": true,
                        "lamports": 1,
                        "owner": TEST_PROGRAM_DATA,
                        "rentEpoch": 0,
                    }
                }
            }),
            json!({
                "result": {
                    "value": {
                        "data": [make_program_data_account(Some(pkbytes_persona()), 8_192), "base64"],
                        "executable": false,
                        "lamports": 1,
                        "owner": TEST_PROGRAM_DATA,
                        "rentEpoch": 0,
                    }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let tmp = TempDir::new().unwrap();
        let request = make_request(&tmp, 4_096, None);
        let report = check(&transport_dyn, &request).unwrap();
        assert_eq!(report.verdict, UpgradeSafetyVerdict::Ok);
        assert_eq!(report.authority.outcome, AuthorityCheckOutcome::Match);
        assert_eq!(report.size.outcome, SizeCheckOutcome::Fits);
        assert_eq!(
            report.authority.program_data_address,
            Some(pd_address_b58())
        );
    }

    #[test]
    fn over_program_data_allocation_blocks_deploy() {
        let transport = Arc::new(ScriptedTransport::new());
        let pd_addr = pd_address_bytes();
        script_program(
            &transport,
            json!({
                "result": {
                    "value": {
                        "data": [make_program_account_data(pd_addr), "base64"],
                        "executable": true, "lamports": 1,
                        "owner": TEST_PROGRAM_DATA, "rentEpoch": 0,
                    }
                }
            }),
            json!({
                "result": {
                    "value": {
                        "data": [make_program_data_account(Some(pkbytes_persona()), 1_000), "base64"],
                        "executable": false, "lamports": 1,
                        "owner": TEST_PROGRAM_DATA, "rentEpoch": 0,
                    }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let tmp = TempDir::new().unwrap();
        let request = make_request(&tmp, 8_000, None);
        let report = check(&transport_dyn, &request).unwrap();
        assert_eq!(report.verdict, UpgradeSafetyVerdict::Block);
        assert_eq!(
            report.size.outcome,
            SizeCheckOutcome::OverProgramDataAllocation
        );
    }

    #[test]
    fn over_absolute_cap_blocks_deploy() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.queue(
            "http://rpc.test",
            "getAccountInfo",
            json!({"result": {"value": null}}),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let tmp = TempDir::new().unwrap();
        let request = make_request(&tmp, 50, Some(10));
        let report = check(&transport_dyn, &request).unwrap();
        assert_eq!(report.size.outcome, SizeCheckOutcome::OverAbsoluteCap);
        assert_eq!(report.verdict, UpgradeSafetyVerdict::Block);
    }

    #[test]
    fn mismatched_authority_blocks_deploy() {
        let transport = Arc::new(ScriptedTransport::new());
        let other_authority = [9u8; 32];
        let pd_addr = pd_address_bytes();
        script_program(
            &transport,
            json!({
                "result": {
                    "value": {
                        "data": [make_program_account_data(pd_addr), "base64"],
                        "executable": true, "lamports": 1,
                        "owner": TEST_PROGRAM_DATA, "rentEpoch": 0,
                    }
                }
            }),
            json!({
                "result": {
                    "value": {
                        "data": [make_program_data_account(Some(other_authority), 8_192), "base64"],
                        "executable": false, "lamports": 1,
                        "owner": TEST_PROGRAM_DATA, "rentEpoch": 0,
                    }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let tmp = TempDir::new().unwrap();
        let request = make_request(&tmp, 4_096, None);
        let report = check(&transport_dyn, &request).unwrap();
        assert_eq!(report.authority.outcome, AuthorityCheckOutcome::Mismatch);
        assert_eq!(report.verdict, UpgradeSafetyVerdict::Block);
    }

    #[test]
    fn immutable_program_blocks_deploy() {
        let transport = Arc::new(ScriptedTransport::new());
        let pd_addr = pd_address_bytes();
        script_program(
            &transport,
            json!({
                "result": {
                    "value": {
                        "data": [make_program_account_data(pd_addr), "base64"],
                        "executable": true, "lamports": 1,
                        "owner": TEST_PROGRAM_DATA, "rentEpoch": 0,
                    }
                }
            }),
            json!({
                "result": {
                    "value": {
                        "data": [make_program_data_account(None, 4_096), "base64"],
                        "executable": false, "lamports": 1,
                        "owner": TEST_PROGRAM_DATA, "rentEpoch": 0,
                    }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let tmp = TempDir::new().unwrap();
        let request = make_request(&tmp, 1_024, None);
        let report = check(&transport_dyn, &request).unwrap();
        assert_eq!(report.authority.outcome, AuthorityCheckOutcome::Immutable);
        assert_eq!(report.verdict, UpgradeSafetyVerdict::Block);
    }

    #[test]
    fn breaking_layout_change_blocks_deploy() {
        let transport = Arc::new(ScriptedTransport::new());
        let pd_addr = pd_address_bytes();
        script_program(
            &transport,
            json!({
                "result": {
                    "value": {
                        "data": [make_program_account_data(pd_addr), "base64"],
                        "executable": true, "lamports": 1,
                        "owner": TEST_PROGRAM_DATA, "rentEpoch": 0,
                    }
                }
            }),
            json!({
                "result": {
                    "value": {
                        "data": [make_program_data_account(Some(pkbytes_persona()), 8_192), "base64"],
                        "executable": false, "lamports": 1,
                        "owner": TEST_PROGRAM_DATA, "rentEpoch": 0,
                    }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let tmp = TempDir::new().unwrap();
        let mut request = make_request(&tmp, 4_096, None);
        request.local_idl = Some(Idl::from_value(
            json!({"instructions": []}),
            IdlSource::Synthetic,
        ));
        request.chain_idl = Some(Idl::from_value(
            json!({"instructions": [{"name": "init", "accounts": [], "args": []}]}),
            IdlSource::Synthetic,
        ));
        let report = check(&transport_dyn, &request).unwrap();
        assert!(!report.breaking_changes.is_empty());
        assert_eq!(report.verdict, UpgradeSafetyVerdict::Block);
    }

    #[test]
    fn risky_layout_change_warns_but_does_not_block() {
        let transport = Arc::new(ScriptedTransport::new());
        let pd_addr = pd_address_bytes();
        script_program(
            &transport,
            json!({
                "result": {
                    "value": {
                        "data": [make_program_account_data(pd_addr), "base64"],
                        "executable": true, "lamports": 1,
                        "owner": TEST_PROGRAM_DATA, "rentEpoch": 0,
                    }
                }
            }),
            json!({
                "result": {
                    "value": {
                        "data": [make_program_data_account(Some(pkbytes_persona()), 8_192), "base64"],
                        "executable": false, "lamports": 1,
                        "owner": TEST_PROGRAM_DATA, "rentEpoch": 0,
                    }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let tmp = TempDir::new().unwrap();
        let mut request = make_request(&tmp, 4_096, None);
        request.chain_idl = Some(Idl::from_value(
            json!({"instructions": [{"name": "init", "accounts": [], "args": []}]}),
            IdlSource::Synthetic,
        ));
        request.local_idl = Some(Idl::from_value(
            json!({"instructions": [{"name": "init", "accounts": [], "args": [{"name": "amount", "type": "u64"}]}]}),
            IdlSource::Synthetic,
        ));
        let report = check(&transport_dyn, &request).unwrap();
        assert_eq!(report.verdict, UpgradeSafetyVerdict::Warn);
    }
}
