//! Squads v4 proposal synthesis for program upgrades.
//!
//! When the on-chain upgrade authority of a Solana program is a Squads
//! v4 multisig vault PDA, no single keypair can call
//! `BPFLoaderUpgradeable::Upgrade` directly — the upgrade has to land
//! as a Squads vault transaction proposal. This module produces a
//! `SquadsProposalDescriptor` containing:
//!
//! - The vault PDA (derived from the multisig account address + a
//!   numeric vault index, defaulting to 0).
//! - The fully-formed `Upgrade` instruction the proposal must execute.
//! - The Squads `vault_transaction_create` + `proposal_create` argv
//!   the user (or the agent) can paste into the Squads CLI to submit
//!   the proposal.
//! - A web URL into the Squads UI for the multisig account so the
//!   user can navigate, review, and approve.
//!
//! We do NOT submit the proposal ourselves. Direct submission would
//! require the desktop app to hold the multisig member keypair, which
//! crosses the policy line drawn in the Phase 5 plan ("never sign for
//! the user against mainnet"). The descriptor is the deliverable.
//!
//! Squads v4 program id: `SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf`.
//! Vault PDA derivation: `["multisig", multisig_pda, "vault", index_le]`
//! seeded against the Squads program id.

use serde::{Deserialize, Serialize};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::pda::{find_program_address, SeedPart};
use crate::commands::{CommandError, CommandResult};

use super::upgrade_safety::BPF_UPGRADEABLE_LOADER;

/// Squads v4 mainnet program id.
pub const SQUADS_V4_PROGRAM_ID: &str = "SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf";

/// Default vault index used by the Squads UI when a multisig has only a
/// single vault.
pub const DEFAULT_VAULT_INDEX: u8 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SquadsProposalRequest {
    pub program_id: String,
    pub cluster: ClusterKind,
    /// Address of the Squads multisig account that owns the program's
    /// upgrade authority (NOT the vault PDA — the multisig PDA).
    pub multisig_pda: String,
    /// Address of the buffer account holding the new `.so`. Produced by
    /// `solana program write-buffer` (or the deploy module's buffer
    /// upload step) before the proposal is created.
    pub buffer: String,
    /// Address of the spill / refund account that receives the buffer
    /// account's lamports after the upgrade lands. Conventionally the
    /// member that paid for the buffer upload.
    pub spill: String,
    /// The Squads multisig member that will create + sign the proposal.
    /// Used as the `creator` and the `payer` in the synthesized argv.
    pub creator: String,
    /// Vault index inside the multisig. Defaults to 0 if omitted.
    #[serde(default)]
    pub vault_index: Option<u8>,
    /// Free-form note appended to the proposal body — surfaces to other
    /// multisig members when they review.
    #[serde(default)]
    pub memo: Option<String>,
}

/// The Solana instruction the multisig will execute when the proposal
/// is approved. All fields are deterministic — given the same inputs
/// we always produce the same instruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeInstruction {
    pub program_id: String,
    pub instruction_tag: u32,
    pub accounts: Vec<UpgradeInstructionAccount>,
    /// Hex-encoded little-endian instruction tag (the `Upgrade` variant
    /// is `3` in the BPF Upgradeable Loader instruction enum).
    pub data_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeInstructionAccount {
    pub pubkey: String,
    pub is_signer: bool,
    pub is_writable: bool,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SquadsProposalDescriptor {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub multisig_pda: String,
    pub vault_pda: String,
    pub vault_index: u8,
    pub program_data_address: String,
    pub upgrade_instruction: UpgradeInstruction,
    /// argv to drive the Squads CLI for `vault_transaction_create`. The
    /// caller appends the multisig keypair to actually sign.
    pub vault_transaction_create_argv: Vec<String>,
    /// argv for the follow-up `proposal_create` step.
    pub proposal_create_argv: Vec<String>,
    /// Web URL for the multisig in the Squads dashboard.
    pub squads_app_url: String,
    /// Human-readable summary surfaced in the deploy panel.
    pub summary: String,
}

pub fn synthesize(request: &SquadsProposalRequest) -> CommandResult<SquadsProposalDescriptor> {
    validate_request(request)?;

    let vault_index = request.vault_index.unwrap_or(DEFAULT_VAULT_INDEX);
    let vault_pda = derive_vault_pda(&request.multisig_pda, vault_index)?;
    let program_data_address = derive_program_data_address(&request.program_id)?;

    let upgrade_instruction = build_upgrade_instruction(
        &request.program_id,
        &program_data_address,
        &request.buffer,
        &vault_pda,
        &request.spill,
    );

    let vault_transaction_create_argv = build_vault_tx_create_argv(request, vault_index);
    let proposal_create_argv = build_proposal_create_argv(request);
    let squads_app_url = squads_app_url_for(request.cluster, &request.multisig_pda);
    let summary = format!(
        "Upgrade {} via Squads multisig {} (vault {}). Buffer {} → ProgramData {}.",
        request.program_id, request.multisig_pda, vault_index, request.buffer, program_data_address,
    );

    Ok(SquadsProposalDescriptor {
        program_id: request.program_id.clone(),
        cluster: request.cluster,
        multisig_pda: request.multisig_pda.clone(),
        vault_pda,
        vault_index,
        program_data_address,
        upgrade_instruction,
        vault_transaction_create_argv,
        proposal_create_argv,
        squads_app_url,
        summary,
    })
}

fn validate_request(request: &SquadsProposalRequest) -> CommandResult<()> {
    for (field, value) in [
        ("program_id", &request.program_id),
        ("multisig_pda", &request.multisig_pda),
        ("buffer", &request.buffer),
        ("spill", &request.spill),
        ("creator", &request.creator),
    ] {
        if value.trim().is_empty() {
            return Err(CommandError::user_fixable(
                "solana_squads_missing_field",
                format!("Squads proposal field `{field}` is required."),
            ));
        }
        if !looks_like_pubkey(value) {
            return Err(CommandError::user_fixable(
                "solana_squads_bad_pubkey",
                format!("Squads proposal field `{field}` is not a valid base58 pubkey."),
            ));
        }
    }
    if matches!(
        request.cluster,
        ClusterKind::Localnet | ClusterKind::MainnetFork
    ) {
        // Local and forked-mainnet clusters don't host a Squads UI, so
        // emitting a proposal there would just be confusing — the user
        // either wants a real devnet/mainnet proposal or a direct
        // localnet deploy.
        return Err(CommandError::policy_denied(
            "Squads proposals only make sense against devnet/mainnet — switch cluster or use a direct keypair authority for local deploys.",
        ));
    }
    Ok(())
}

fn looks_like_pubkey(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() < 32 || trimmed.len() > 44 {
        return false;
    }
    bs58::decode(trimmed)
        .into_vec()
        .map(|b| b.len() == 32)
        .unwrap_or(false)
}

fn derive_vault_pda(multisig_pda: &str, vault_index: u8) -> CommandResult<String> {
    // Squads v4 vault seeds: ["multisig", multisig_pda, "vault", [index]]
    // signed by the Squads program id.
    let seeds = vec![
        SeedPart::Utf8("multisig".to_string()),
        SeedPart::Pubkey(multisig_pda.to_string()),
        SeedPart::Utf8("vault".to_string()),
        SeedPart::U8(vault_index),
    ];
    let derived = find_program_address(SQUADS_V4_PROGRAM_ID, &seeds)?;
    Ok(derived.pubkey)
}

fn derive_program_data_address(program_id: &str) -> CommandResult<String> {
    // BPF Upgradeable Loader ProgramData PDA: ["program_id_bytes"]
    // signed by `BPFLoaderUpgradeab1e11111111111111111111111`.
    let seeds = vec![SeedPart::Pubkey(program_id.to_string())];
    let derived = find_program_address(BPF_UPGRADEABLE_LOADER, &seeds)?;
    Ok(derived.pubkey)
}

fn build_upgrade_instruction(
    program_id: &str,
    program_data: &str,
    buffer: &str,
    upgrade_authority: &str,
    spill: &str,
) -> UpgradeInstruction {
    // BPF Upgradeable Loader `Upgrade` instruction tag = 3 (u32 LE).
    let tag = 3u32;
    let data_hex = bytes_to_hex(&tag.to_le_bytes());
    UpgradeInstruction {
        program_id: BPF_UPGRADEABLE_LOADER.to_string(),
        instruction_tag: tag,
        accounts: vec![
            UpgradeInstructionAccount {
                pubkey: program_data.to_string(),
                is_signer: false,
                is_writable: true,
                label: "ProgramData".to_string(),
            },
            UpgradeInstructionAccount {
                pubkey: program_id.to_string(),
                is_signer: false,
                is_writable: true,
                label: "Program".to_string(),
            },
            UpgradeInstructionAccount {
                pubkey: buffer.to_string(),
                is_signer: false,
                is_writable: true,
                label: "Buffer".to_string(),
            },
            UpgradeInstructionAccount {
                pubkey: spill.to_string(),
                is_signer: false,
                is_writable: true,
                label: "Spill".to_string(),
            },
            UpgradeInstructionAccount {
                pubkey: "SysvarRent111111111111111111111111111111111".to_string(),
                is_signer: false,
                is_writable: false,
                label: "RentSysvar".to_string(),
            },
            UpgradeInstructionAccount {
                pubkey: "SysvarC1ock11111111111111111111111111111111".to_string(),
                is_signer: false,
                is_writable: false,
                label: "ClockSysvar".to_string(),
            },
            UpgradeInstructionAccount {
                pubkey: upgrade_authority.to_string(),
                is_signer: true,
                is_writable: false,
                label: "UpgradeAuthority (vault PDA)".to_string(),
            },
        ],
        data_hex,
    }
}

fn build_vault_tx_create_argv(request: &SquadsProposalRequest, vault_index: u8) -> Vec<String> {
    // Mirrors `squads-multisig-cli vault-transaction create`. We document
    // the argv but don't execute — the user signs locally.
    vec![
        "squads-multisig-cli".into(),
        "vault-transaction".into(),
        "create".into(),
        "--multisig".into(),
        request.multisig_pda.clone(),
        "--vault-index".into(),
        vault_index.to_string(),
        "--rpc-url".into(),
        rpc_url_for(request.cluster).into(),
        "--memo".into(),
        request
            .memo
            .clone()
            .unwrap_or_else(|| format!("Xero: upgrade {}", request.program_id)),
        "--instruction-program-id".into(),
        BPF_UPGRADEABLE_LOADER.to_string(),
        "--instruction-data-hex".into(),
        bytes_to_hex(&3u32.to_le_bytes()),
        "--keypair".into(),
        format!("<{}-keypair.json>", request.creator),
    ]
}

fn build_proposal_create_argv(request: &SquadsProposalRequest) -> Vec<String> {
    vec![
        "squads-multisig-cli".into(),
        "proposal".into(),
        "create".into(),
        "--multisig".into(),
        request.multisig_pda.clone(),
        "--rpc-url".into(),
        rpc_url_for(request.cluster).into(),
        "--keypair".into(),
        format!("<{}-keypair.json>", request.creator),
    ]
}

fn squads_app_url_for(cluster: ClusterKind, multisig_pda: &str) -> String {
    let cluster_query = match cluster {
        ClusterKind::Devnet => "?cluster=devnet",
        ClusterKind::Mainnet => "",
        ClusterKind::Localnet | ClusterKind::MainnetFork => "?cluster=devnet",
    };
    format!("https://app.squads.so/squads/{multisig_pda}/transactions{cluster_query}")
}

fn rpc_url_for(cluster: ClusterKind) -> &'static str {
    match cluster {
        ClusterKind::Devnet => "https://api.devnet.solana.com",
        ClusterKind::Mainnet => "https://api.mainnet-beta.solana.com",
        ClusterKind::Localnet | ClusterKind::MainnetFork => "http://127.0.0.1:8899",
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_pk(byte: u8) -> String {
        bs58::encode([byte; 32]).into_string()
    }

    fn make_request(cluster: ClusterKind) -> SquadsProposalRequest {
        SquadsProposalRequest {
            program_id: valid_pk(1),
            cluster,
            multisig_pda: valid_pk(2),
            buffer: valid_pk(3),
            spill: valid_pk(4),
            creator: valid_pk(5),
            vault_index: None,
            memo: None,
        }
    }

    #[test]
    fn synthesize_returns_descriptor_for_devnet() {
        let req = make_request(ClusterKind::Devnet);
        let desc = synthesize(&req).unwrap();
        assert_eq!(desc.program_id, req.program_id);
        assert_eq!(desc.cluster, ClusterKind::Devnet);
        assert!(desc.vault_pda.len() >= 32);
        assert_eq!(desc.upgrade_instruction.program_id, BPF_UPGRADEABLE_LOADER);
        assert_eq!(desc.upgrade_instruction.instruction_tag, 3);
        assert!(desc.squads_app_url.contains("app.squads.so"));
        assert!(desc.squads_app_url.contains("cluster=devnet"));
    }

    #[test]
    fn synthesize_emits_seven_account_metas_for_upgrade() {
        let req = make_request(ClusterKind::Mainnet);
        let desc = synthesize(&req).unwrap();
        assert_eq!(desc.upgrade_instruction.accounts.len(), 7);
        let auth = desc
            .upgrade_instruction
            .accounts
            .iter()
            .find(|a| a.label.as_str() == "UpgradeAuthority (vault PDA)")
            .unwrap();
        assert!(auth.is_signer);
        assert_eq!(auth.pubkey, desc.vault_pda);
    }

    #[test]
    fn synthesize_argv_includes_multisig_and_vault_index() {
        let mut req = make_request(ClusterKind::Devnet);
        req.vault_index = Some(2);
        let desc = synthesize(&req).unwrap();
        assert!(desc
            .vault_transaction_create_argv
            .contains(&req.multisig_pda));
        assert!(desc
            .vault_transaction_create_argv
            .contains(&"2".to_string()));
    }

    #[test]
    fn synthesize_uses_default_vault_index_when_unspecified() {
        let req = make_request(ClusterKind::Devnet);
        let desc = synthesize(&req).unwrap();
        assert_eq!(desc.vault_index, DEFAULT_VAULT_INDEX);
    }

    #[test]
    fn synthesize_rejects_localnet() {
        let req = make_request(ClusterKind::Localnet);
        let err = synthesize(&req).unwrap_err();
        assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
    }

    #[test]
    fn synthesize_rejects_mainnet_fork() {
        let req = make_request(ClusterKind::MainnetFork);
        let err = synthesize(&req).unwrap_err();
        assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
    }

    #[test]
    fn synthesize_rejects_invalid_pubkey_field() {
        let mut req = make_request(ClusterKind::Devnet);
        req.buffer = "not-a-pubkey".into();
        let err = synthesize(&req).unwrap_err();
        assert_eq!(err.code, "solana_squads_bad_pubkey");
    }

    #[test]
    fn vault_pda_derivation_is_deterministic() {
        let req = make_request(ClusterKind::Devnet);
        let a = synthesize(&req).unwrap();
        let b = synthesize(&req).unwrap();
        assert_eq!(a.vault_pda, b.vault_pda);
    }

    #[test]
    fn vault_pda_changes_with_index() {
        let mut req = make_request(ClusterKind::Devnet);
        req.vault_index = Some(0);
        let a = synthesize(&req).unwrap();
        req.vault_index = Some(1);
        let b = synthesize(&req).unwrap();
        assert_ne!(a.vault_pda, b.vault_pda);
    }
}
