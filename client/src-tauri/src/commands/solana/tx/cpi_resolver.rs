//! CPI account resolver.
//!
//! Given a program id + instruction name + user-supplied arguments,
//! returns the canonical account list that instruction expects. For now
//! this is a hand-maintained map of the most common programs a dapp dev
//! needs to drive from the workbench: SPL Token, Token-2022, Memo,
//! Associated Token, Metaplex Token Metadata, Jupiter V6, Orca Whirlpool,
//! SPL Governance, Squads v4.
//!
//! When a program is outside the known set, callers fall back to
//! `solana_idl_resolve` (Phase 4) to pull the account layout from the
//! program's IDL. The resolver returns `KnownProgramLookup::UnknownProgram`
//! in that case so the caller knows to escalate.

use serde::{Deserialize, Serialize};

use crate::commands::CommandError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountMetaSpec {
    pub pubkey: String,
    pub is_signer: bool,
    pub is_writable: bool,
    /// Human label, e.g. "source", "destination", "mint". Surfaced in the
    /// tx inspector panel.
    pub label: Option<String>,
}

impl AccountMetaSpec {
    pub fn new(pubkey: impl Into<String>, is_signer: bool, is_writable: bool) -> Self {
        Self {
            pubkey: pubkey.into(),
            is_signer,
            is_writable,
            label: None,
        }
    }

    pub fn labeled(
        pubkey: impl Into<String>,
        label: impl Into<String>,
        is_signer: bool,
        is_writable: bool,
    ) -> Self {
        Self {
            pubkey: pubkey.into(),
            is_signer,
            is_writable,
            label: Some(label.into()),
        }
    }

    fn placeholder(label: &str, is_signer: bool, is_writable: bool) -> Self {
        Self {
            pubkey: format!("<{label}>"),
            is_signer,
            is_writable,
            label: Some(label.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CpiResolution {
    pub program_id: String,
    pub program_label: String,
    pub instruction: String,
    pub accounts: Vec<AccountMetaSpec>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "outcome")]
pub enum KnownProgramLookup {
    Hit {
        resolution: CpiResolution,
    },
    UnknownProgram {
        program_id: String,
    },
    UnknownInstruction {
        program_id: String,
        program_label: String,
        known_instructions: Vec<String>,
    },
}

pub const SPL_TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const SPL_TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const ASSOCIATED_TOKEN_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
pub const MEMO_PROGRAM: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";
pub const METAPLEX_TOKEN_METADATA_PROGRAM: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";
pub const JUPITER_V6_PROGRAM: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
pub const ORCA_WHIRLPOOL_PROGRAM: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";
pub const RAYDIUM_AMM_V4_PROGRAM: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
pub const SPL_GOVERNANCE_PROGRAM: &str = "GovER5Lthms3bLBqWub97yVrMmEogzX7xNjdXpPPCVZw";
pub const SQUADS_V4_PROGRAM: &str = "SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf";
pub const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";

pub fn known_program_label(program_id: &str) -> Option<&'static str> {
    Some(match program_id {
        SPL_TOKEN_PROGRAM => "SPL Token",
        SPL_TOKEN_2022_PROGRAM => "Token-2022",
        ASSOCIATED_TOKEN_PROGRAM => "Associated Token",
        MEMO_PROGRAM => "Memo",
        METAPLEX_TOKEN_METADATA_PROGRAM => "Metaplex Token Metadata",
        JUPITER_V6_PROGRAM => "Jupiter V6",
        ORCA_WHIRLPOOL_PROGRAM => "Orca Whirlpools",
        RAYDIUM_AMM_V4_PROGRAM => "Raydium AMM V4",
        SPL_GOVERNANCE_PROGRAM => "SPL Governance",
        SQUADS_V4_PROGRAM => "Squads V4",
        _ => return None,
    })
}

pub fn known_program_ids() -> &'static [&'static str] {
    &[
        SPL_TOKEN_PROGRAM,
        SPL_TOKEN_2022_PROGRAM,
        ASSOCIATED_TOKEN_PROGRAM,
        MEMO_PROGRAM,
        METAPLEX_TOKEN_METADATA_PROGRAM,
        JUPITER_V6_PROGRAM,
        ORCA_WHIRLPOOL_PROGRAM,
        RAYDIUM_AMM_V4_PROGRAM,
        SPL_GOVERNANCE_PROGRAM,
        SQUADS_V4_PROGRAM,
    ]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveArgs {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub authority: Option<String>,
    #[serde(default)]
    pub mint: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub payer: Option<String>,
    #[serde(default)]
    pub extras: std::collections::BTreeMap<String, String>,
}

pub fn resolve(program_id: &str, instruction: &str, args: &ResolveArgs) -> KnownProgramLookup {
    let label = match known_program_label(program_id) {
        Some(l) => l,
        None => {
            return KnownProgramLookup::UnknownProgram {
                program_id: program_id.to_string(),
            };
        }
    };

    let partial = match program_id {
        SPL_TOKEN_PROGRAM | SPL_TOKEN_2022_PROGRAM => resolve_spl_token(instruction, args),
        ASSOCIATED_TOKEN_PROGRAM => resolve_associated_token(instruction, args),
        MEMO_PROGRAM => resolve_memo(instruction, args),
        METAPLEX_TOKEN_METADATA_PROGRAM => resolve_metaplex(instruction, args),
        JUPITER_V6_PROGRAM => resolve_jupiter(instruction, args),
        ORCA_WHIRLPOOL_PROGRAM => resolve_orca(instruction, args),
        RAYDIUM_AMM_V4_PROGRAM => resolve_raydium(instruction, args),
        SPL_GOVERNANCE_PROGRAM => resolve_spl_governance(instruction, args),
        SQUADS_V4_PROGRAM => resolve_squads(instruction, args),
        _ => None,
    };

    match partial {
        Some(partial) => KnownProgramLookup::Hit {
            resolution: CpiResolution {
                program_id: program_id.to_string(),
                program_label: label.to_string(),
                instruction: instruction.to_string(),
                accounts: partial.accounts,
                notes: partial.notes.into_iter().map(String::from).collect(),
            },
        },
        None => KnownProgramLookup::UnknownInstruction {
            program_id: program_id.to_string(),
            program_label: label.to_string(),
            known_instructions: known_instructions_for(program_id)
                .iter()
                .map(|s| s.to_string())
                .collect(),
        },
    }
}

pub fn unknown_instruction_error(label: &str, instruction: &str, program_id: &str) -> CommandError {
    CommandError::user_fixable(
        "solana_cpi_unknown_instruction",
        format!(
            "Instruction `{instruction}` is not mapped for {label} ({program_id}). Fall back to \
             the IDL resolver or supply the account list manually."
        ),
    )
}

fn known_instructions_for(program_id: &str) -> Vec<&'static str> {
    match program_id {
        SPL_TOKEN_PROGRAM | SPL_TOKEN_2022_PROGRAM => vec![
            "transfer",
            "transferChecked",
            "mintTo",
            "mintToChecked",
            "burn",
            "closeAccount",
            "initializeMint",
            "initializeAccount",
        ],
        ASSOCIATED_TOKEN_PROGRAM => vec!["create", "createIdempotent"],
        MEMO_PROGRAM => vec!["memo"],
        METAPLEX_TOKEN_METADATA_PROGRAM => {
            vec!["createMetadataAccountV3", "updateMetadataAccountV2"]
        }
        JUPITER_V6_PROGRAM => vec!["sharedAccountsRoute", "route"],
        ORCA_WHIRLPOOL_PROGRAM => vec!["swap", "twoHopSwap"],
        RAYDIUM_AMM_V4_PROGRAM => vec!["swapBaseIn", "swapBaseOut"],
        SPL_GOVERNANCE_PROGRAM => {
            vec!["castVote", "createProposal", "executeTransaction"]
        }
        SQUADS_V4_PROGRAM => vec!["vaultTransactionCreate", "vaultTransactionExecute"],
        _ => Vec::new(),
    }
}

struct Partial {
    accounts: Vec<AccountMetaSpec>,
    notes: Vec<&'static str>,
}

fn meta(label: &str, pubkey: Option<&str>, is_signer: bool, is_writable: bool) -> AccountMetaSpec {
    match pubkey {
        Some(key) if !key.is_empty() => {
            AccountMetaSpec::labeled(key, label, is_signer, is_writable)
        }
        _ => AccountMetaSpec::placeholder(label, is_signer, is_writable),
    }
}

fn sysvar_rent() -> AccountMetaSpec {
    AccountMetaSpec::labeled(
        "SysvarRent111111111111111111111111111111111",
        "sysvarRent",
        false,
        false,
    )
}

fn resolve_spl_token(instruction: &str, args: &ResolveArgs) -> Option<Partial> {
    match instruction {
        "transfer" => Some(Partial {
            accounts: vec![
                meta("source", args.source.as_deref(), false, true),
                meta("destination", args.destination.as_deref(), false, true),
                meta("authority", args.authority.as_deref(), true, false),
            ],
            notes: vec!["Multisig authorities append extra signer accounts at the end."],
        }),
        "transferChecked" => Some(Partial {
            accounts: vec![
                meta("source", args.source.as_deref(), false, true),
                meta("mint", args.mint.as_deref(), false, false),
                meta("destination", args.destination.as_deref(), false, true),
                meta("authority", args.authority.as_deref(), true, false),
            ],
            notes: vec!["transferChecked validates the mint's decimals argument on-chain."],
        }),
        "mintTo" => Some(Partial {
            accounts: vec![
                meta("mint", args.mint.as_deref(), false, true),
                meta("destination", args.destination.as_deref(), false, true),
                meta("authority", args.authority.as_deref(), true, false),
            ],
            notes: vec![],
        }),
        "mintToChecked" => Some(Partial {
            accounts: vec![
                meta("mint", args.mint.as_deref(), false, true),
                meta("destination", args.destination.as_deref(), false, true),
                meta("authority", args.authority.as_deref(), true, false),
            ],
            notes: vec!["mintToChecked additionally validates the decimals argument."],
        }),
        "burn" => Some(Partial {
            accounts: vec![
                meta("source", args.source.as_deref(), false, true),
                meta("mint", args.mint.as_deref(), false, true),
                meta("authority", args.authority.as_deref(), true, false),
            ],
            notes: vec![],
        }),
        "closeAccount" => Some(Partial {
            accounts: vec![
                meta("account", args.source.as_deref(), false, true),
                meta("destination", args.destination.as_deref(), false, true),
                meta("owner", args.owner.as_deref(), true, false),
            ],
            notes: vec!["destination receives rent-exempt lamports from the closed account."],
        }),
        "initializeMint" | "initializeAccount" => Some(Partial {
            accounts: vec![
                meta("account", args.source.as_deref(), false, true),
                meta("mint", args.mint.as_deref(), false, false),
                meta("owner", args.owner.as_deref(), false, false),
                sysvar_rent(),
            ],
            notes: vec!["System program + rent sysvar must be present as well."],
        }),
        _ => None,
    }
}

fn resolve_associated_token(instruction: &str, args: &ResolveArgs) -> Option<Partial> {
    match instruction {
        "create" | "createIdempotent" => Some(Partial {
            accounts: vec![
                meta("payer", args.payer.as_deref(), true, true),
                meta(
                    "associatedAccount",
                    args.destination.as_deref(),
                    false,
                    true,
                ),
                meta("owner", args.owner.as_deref(), false, false),
                meta("mint", args.mint.as_deref(), false, false),
                AccountMetaSpec::labeled(SYSTEM_PROGRAM, "systemProgram", false, false),
                AccountMetaSpec::labeled(SPL_TOKEN_PROGRAM, "tokenProgram", false, false),
            ],
            notes: vec!["createIdempotent is a safe default when the ATA may already exist."],
        }),
        _ => None,
    }
}

fn resolve_memo(instruction: &str, args: &ResolveArgs) -> Option<Partial> {
    if instruction != "memo" {
        return None;
    }
    let mut accounts = Vec::new();
    if let Some(authority) = args.authority.as_deref() {
        accounts.push(AccountMetaSpec::labeled(authority, "signer", true, false));
    }
    Some(Partial {
        accounts,
        notes: vec!["Memo program validates UTF-8 up to 566 bytes."],
    })
}

fn resolve_metaplex(instruction: &str, args: &ResolveArgs) -> Option<Partial> {
    match instruction {
        "createMetadataAccountV3" => Some(Partial {
            accounts: vec![
                AccountMetaSpec::placeholder("metadata", false, true),
                meta("mint", args.mint.as_deref(), false, false),
                meta("mintAuthority", args.authority.as_deref(), true, false),
                meta("payer", args.payer.as_deref(), true, true),
                meta("updateAuthority", args.authority.as_deref(), false, false),
                AccountMetaSpec::labeled(SYSTEM_PROGRAM, "systemProgram", false, false),
                sysvar_rent(),
            ],
            notes: vec![
                "metadata PDA = ['metadata', programId, mint] — derive via solana_pda_derive.",
            ],
        }),
        "updateMetadataAccountV2" => Some(Partial {
            accounts: vec![
                AccountMetaSpec::placeholder("metadata", false, true),
                meta("updateAuthority", args.authority.as_deref(), true, false),
            ],
            notes: vec![],
        }),
        _ => None,
    }
}

fn resolve_jupiter(instruction: &str, args: &ResolveArgs) -> Option<Partial> {
    match instruction {
        "route" | "sharedAccountsRoute" => Some(Partial {
            accounts: vec![
                meta(
                    "userTransferAuthority",
                    args.authority.as_deref(),
                    true,
                    false,
                ),
                meta(
                    "userSourceTokenAccount",
                    args.source.as_deref(),
                    false,
                    true,
                ),
                meta(
                    "userDestinationTokenAccount",
                    args.destination.as_deref(),
                    false,
                    true,
                ),
                AccountMetaSpec::labeled(SPL_TOKEN_PROGRAM, "tokenProgram", false, false),
                AccountMetaSpec::placeholder("remaining accounts", false, false),
            ],
            notes: vec![
                "Jupiter routes are dynamic — the remaining-accounts tail comes from the route plan.",
                "Call the Jupiter HTTP API to get the account list for a specific quote.",
            ],
        }),
        _ => None,
    }
}

fn resolve_orca(instruction: &str, args: &ResolveArgs) -> Option<Partial> {
    match instruction {
        "swap" => Some(Partial {
            accounts: vec![
                AccountMetaSpec::labeled(SPL_TOKEN_PROGRAM, "tokenProgram", false, false),
                meta("tokenAuthority", args.authority.as_deref(), true, false),
                AccountMetaSpec::placeholder("whirlpool", false, true),
                meta("tokenOwnerAccountA", args.source.as_deref(), false, true),
                AccountMetaSpec::placeholder("tokenVaultA", false, true),
                meta(
                    "tokenOwnerAccountB",
                    args.destination.as_deref(),
                    false,
                    true,
                ),
                AccountMetaSpec::placeholder("tokenVaultB", false, true),
                AccountMetaSpec::placeholder("tickArray0", false, true),
                AccountMetaSpec::placeholder("tickArray1", false, true),
                AccountMetaSpec::placeholder("tickArray2", false, true),
                AccountMetaSpec::placeholder("oracle", false, false),
            ],
            notes: vec![
                "Oracle account is fixed-PDA per whirlpool — derive via solana_pda_derive.",
            ],
        }),
        "twoHopSwap" => Some(Partial {
            accounts: vec![AccountMetaSpec::placeholder(
                "twoHopSwap accounts (24+)",
                false,
                false,
            )],
            notes: vec!["Two-hop swaps need the double set of swap accounts; see Orca docs."],
        }),
        _ => None,
    }
}

fn resolve_raydium(instruction: &str, _args: &ResolveArgs) -> Option<Partial> {
    match instruction {
        "swapBaseIn" | "swapBaseOut" => Some(Partial {
            accounts: vec![
                AccountMetaSpec::labeled(SPL_TOKEN_PROGRAM, "tokenProgram", false, false),
                AccountMetaSpec::placeholder("ammId", false, true),
                AccountMetaSpec::placeholder("ammAuthority", false, false),
                AccountMetaSpec::placeholder("ammOpenOrders", false, true),
                AccountMetaSpec::placeholder("ammTargetOrders", false, true),
                AccountMetaSpec::placeholder("poolCoinTokenAccount", false, true),
                AccountMetaSpec::placeholder("poolPcTokenAccount", false, true),
                AccountMetaSpec::placeholder("serumProgram", false, false),
                AccountMetaSpec::placeholder("serumMarket", false, true),
                AccountMetaSpec::placeholder("userSourceTokenAccount", false, true),
                AccountMetaSpec::placeholder("userDestinationTokenAccount", false, true),
                AccountMetaSpec::placeholder("userOwner", true, false),
            ],
            notes: vec![
                "Raydium classic AMM accounts are brittle — prefer swapping via Jupiter V6 if possible.",
            ],
        }),
        _ => None,
    }
}

fn resolve_spl_governance(instruction: &str, args: &ResolveArgs) -> Option<Partial> {
    match instruction {
        "castVote" => Some(Partial {
            accounts: vec![
                AccountMetaSpec::placeholder("realm", false, false),
                AccountMetaSpec::placeholder("governance", false, false),
                AccountMetaSpec::placeholder("proposal", false, true),
                AccountMetaSpec::placeholder("proposalOwnerRecord", false, true),
                AccountMetaSpec::placeholder("tokenOwnerRecord", false, true),
                AccountMetaSpec::placeholder("voteRecord", false, true),
                meta("governingTokenMint", args.mint.as_deref(), false, false),
                meta("payer", args.payer.as_deref(), true, true),
                AccountMetaSpec::labeled(SYSTEM_PROGRAM, "systemProgram", false, false),
            ],
            notes: vec!["vote record PDA = ['governance', proposal, token_owner_record]."],
        }),
        "createProposal" | "executeTransaction" => Some(Partial {
            accounts: vec![AccountMetaSpec::placeholder(
                "realm-scoped accounts",
                false,
                false,
            )],
            notes: vec!["Varies with governance version; see spl-governance client docs."],
        }),
        _ => None,
    }
}

fn resolve_squads(instruction: &str, args: &ResolveArgs) -> Option<Partial> {
    match instruction {
        "vaultTransactionCreate" => Some(Partial {
            accounts: vec![
                AccountMetaSpec::placeholder("multisig", false, true),
                AccountMetaSpec::placeholder("transaction", false, true),
                meta("creator", args.payer.as_deref(), true, true),
                meta("rentPayer", args.payer.as_deref(), true, true),
                AccountMetaSpec::labeled(SYSTEM_PROGRAM, "systemProgram", false, false),
            ],
            notes: vec!["vault = PDA(['squad', multisig, 'vault', vault_index])."],
        }),
        "vaultTransactionExecute" => Some(Partial {
            accounts: vec![
                AccountMetaSpec::placeholder("multisig", false, false),
                AccountMetaSpec::placeholder("proposal", false, true),
                AccountMetaSpec::placeholder("transaction", false, true),
                meta("member", args.authority.as_deref(), true, false),
            ],
            notes: vec!["Member must be in the multisig with vote threshold met."],
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_program_bubbles_up() {
        let out = resolve(
            "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ",
            "transfer",
            &ResolveArgs::default(),
        );
        match out {
            KnownProgramLookup::UnknownProgram { program_id } => {
                assert!(program_id.starts_with("ZZZZ"));
            }
            other => panic!("expected UnknownProgram, got {other:?}"),
        }
    }

    #[test]
    fn spl_token_transfer_has_three_accounts() {
        let out = resolve(
            SPL_TOKEN_PROGRAM,
            "transfer",
            &ResolveArgs {
                source: Some("src".into()),
                destination: Some("dst".into()),
                authority: Some("auth".into()),
                ..ResolveArgs::default()
            },
        );
        match out {
            KnownProgramLookup::Hit { resolution } => {
                assert_eq!(resolution.accounts.len(), 3);
                assert_eq!(resolution.accounts[0].pubkey, "src");
                assert!(!resolution.accounts[0].is_signer);
                assert!(resolution.accounts[0].is_writable);
                assert!(resolution.accounts[2].is_signer);
            }
            other => panic!("expected Hit, got {other:?}"),
        }
    }

    #[test]
    fn spl_token_transfer_checked_inserts_mint_slot() {
        let out = resolve(
            SPL_TOKEN_PROGRAM,
            "transferChecked",
            &ResolveArgs {
                source: Some("src".into()),
                destination: Some("dst".into()),
                authority: Some("auth".into()),
                mint: Some("mint".into()),
                ..ResolveArgs::default()
            },
        );
        match out {
            KnownProgramLookup::Hit { resolution } => {
                assert_eq!(resolution.accounts.len(), 4);
                assert_eq!(resolution.accounts[1].pubkey, "mint");
            }
            other => panic!("expected Hit, got {other:?}"),
        }
    }

    #[test]
    fn associated_token_create_includes_system_and_token_programs() {
        let out = resolve(
            ASSOCIATED_TOKEN_PROGRAM,
            "createIdempotent",
            &ResolveArgs {
                payer: Some("payer".into()),
                destination: Some("ata".into()),
                owner: Some("owner".into()),
                mint: Some("mint".into()),
                ..ResolveArgs::default()
            },
        );
        match out {
            KnownProgramLookup::Hit { resolution } => {
                assert!(resolution
                    .accounts
                    .iter()
                    .any(|a| a.label.as_deref() == Some("systemProgram")));
                assert!(resolution
                    .accounts
                    .iter()
                    .any(|a| a.label.as_deref() == Some("tokenProgram")));
            }
            other => panic!("expected Hit, got {other:?}"),
        }
    }

    #[test]
    fn unknown_instruction_returns_catalog_of_knowns() {
        let out = resolve(
            SPL_TOKEN_PROGRAM,
            "neverGonnaGiveYouUp",
            &ResolveArgs::default(),
        );
        match out {
            KnownProgramLookup::UnknownInstruction {
                program_label,
                known_instructions,
                ..
            } => {
                assert_eq!(program_label, "SPL Token");
                assert!(known_instructions.iter().any(|s| s == "transfer"));
            }
            other => panic!("expected UnknownInstruction, got {other:?}"),
        }
    }

    #[test]
    fn jupiter_route_returns_dynamic_tail_note() {
        let out = resolve(
            JUPITER_V6_PROGRAM,
            "route",
            &ResolveArgs {
                authority: Some("usr".into()),
                source: Some("src".into()),
                destination: Some("dst".into()),
                ..ResolveArgs::default()
            },
        );
        match out {
            KnownProgramLookup::Hit { resolution } => {
                assert!(resolution
                    .notes
                    .iter()
                    .any(|n| n.contains("Jupiter HTTP API")));
            }
            other => panic!("expected Hit, got {other:?}"),
        }
    }

    #[test]
    fn unknown_instruction_error_references_program_label() {
        let err = unknown_instruction_error("SPL Token", "foo", SPL_TOKEN_PROGRAM);
        assert_eq!(err.code, "solana_cpi_unknown_instruction");
        assert!(err.message.contains("SPL Token"));
    }

    #[test]
    fn known_program_ids_matches_labels() {
        for id in known_program_ids() {
            assert!(known_program_label(id).is_some());
        }
    }
}
