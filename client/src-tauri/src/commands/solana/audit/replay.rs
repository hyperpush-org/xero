//! Exploit replay library — replay historical Solana exploits against
//! a forked-mainnet snapshot so the developer can prove their program
//! isn't vulnerable to the same class.
//!
//! Four exploits ship in the built-in library:
//!   * `wormhole_sig_skip`   — Wormhole signature bypass (Feb 2022).
//!   * `cashio_fake_collateral` — Cashio fake-collateral mint (Mar 2022).
//!   * `mango_oracle_manip`  — Mango oracle-manipulation drain (Oct 2022).
//!   * `nirvana_flash_loan`  — Nirvana flash-loan redemption skew (Jul 2022).
//!
//! Each scenario is represented as a structured set of steps (fork block,
//! clone accounts, send tx sequence, assert post-state). The `ReplayRunner`
//! trait is the boundary the workbench crosses to drive those steps
//! against whatever validator is active — the default production runner
//! relies on the existing forked-mainnet surfpool integration plus the
//! tx pipeline, but the scenarios themselves are pure data and therefore
//! trivially unit-testable.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::{CommandError, CommandResult};

use super::{Finding, FindingSeverity, FindingSource};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExploitKey {
    WormholeSigSkip,
    CashioFakeCollateral,
    MangoOracleManip,
    NirvanaFlashLoan,
}

impl ExploitKey {
    pub fn as_str(self) -> &'static str {
        match self {
            ExploitKey::WormholeSigSkip => "wormhole_sig_skip",
            ExploitKey::CashioFakeCollateral => "cashio_fake_collateral",
            ExploitKey::MangoOracleManip => "mango_oracle_manip",
            ExploitKey::NirvanaFlashLoan => "nirvana_flash_loan",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "wormhole_sig_skip" => Some(ExploitKey::WormholeSigSkip),
            "cashio_fake_collateral" => Some(ExploitKey::CashioFakeCollateral),
            "mango_oracle_manip" => Some(ExploitKey::MangoOracleManip),
            "nirvana_flash_loan" => Some(ExploitKey::NirvanaFlashLoan),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExploitDescriptor {
    pub key: ExploitKey,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub exploit_slot: u64,
    pub reference_url: String,
    pub impacted_program: String,
    pub clone_accounts: Vec<String>,
    pub clone_programs: Vec<String>,
    pub steps: Vec<ReplayStep>,
    pub expected_bad_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ReplayStep {
    Fork {
        slot: u64,
        note: String,
    },
    CloneAccount {
        address: String,
        note: String,
    },
    SendTx {
        label: String,
        description: String,
        affected_program: String,
        rationale: String,
    },
    AssertBadState {
        description: String,
        rationale: String,
    },
}

#[derive(Debug, Clone)]
pub struct ExploitLibrary {
    exploits: HashMap<ExploitKey, ExploitDescriptor>,
}

impl ExploitLibrary {
    pub fn builtin() -> Self {
        let mut exploits = HashMap::new();
        for exploit in builtin_exploits() {
            exploits.insert(exploit.key, exploit);
        }
        Self { exploits }
    }

    pub fn all(&self) -> Vec<&ExploitDescriptor> {
        let mut v: Vec<&ExploitDescriptor> = self.exploits.values().collect();
        v.sort_by_key(|d| d.key.as_str());
        v
    }

    pub fn get(&self, key: ExploitKey) -> Option<&ExploitDescriptor> {
        self.exploits.get(&key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayRequest {
    pub exploit: ExploitKey,
    pub target_program: String,
    pub cluster: ClusterKind,
    #[serde(default)]
    pub rpc_url: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
    /// Caller can pin to a specific slot. Defaults to the exploit's
    /// historical slot.
    #[serde(default)]
    pub snapshot_slot: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayOutcome {
    ExpectedBadState,
    Mitigated,
    UnexpectedFailure,
    Inconclusive,
}

impl ReplayOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            ReplayOutcome::ExpectedBadState => "expected_bad_state",
            ReplayOutcome::Mitigated => "mitigated",
            ReplayOutcome::UnexpectedFailure => "unexpected_failure",
            ReplayOutcome::Inconclusive => "inconclusive",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayStepTrace {
    pub step_index: u32,
    pub label: String,
    pub success: bool,
    pub message: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayReport {
    #[serde(default)]
    pub run_id: String,
    pub exploit: ExploitKey,
    pub target_program: String,
    pub cluster: ClusterKind,
    pub snapshot_slot: u64,
    pub outcome: ReplayOutcome,
    pub dry_run: bool,
    pub steps: Vec<ReplayStepTrace>,
    pub summary: String,
    pub findings: Vec<Finding>,
    pub reference_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayContext {
    pub exploit: ExploitKey,
    pub target_program: String,
    pub cluster: ClusterKind,
    pub rpc_url: Option<String>,
    pub snapshot_slot: u64,
}

pub trait ReplayRunner: Send + Sync + std::fmt::Debug {
    /// Execute a single step against the active validator. The default
    /// implementation returns `Inconclusive` for every step so the
    /// library can be linked without the tx pipeline actually being
    /// wired in.
    fn execute_step(
        &self,
        ctx: &ReplayContext,
        step_index: u32,
        step: &ReplayStep,
    ) -> CommandResult<ReplayStepTrace>;

    /// Called after every step executes to let the runner tell us what
    /// the final outcome was. The default `SystemReplayRunner`
    /// conservatively reports `Inconclusive` so the UI can prompt the
    /// user to validate manually.
    fn verify_outcome(&self, ctx: &ReplayContext) -> CommandResult<(ReplayOutcome, String)>;
}

/// Production runner — currently a stub. Phase 6 ships the scenario
/// catalogue and the structural hooks; wiring into the live validator
/// is a tracked follow-up per the plan, so the runner marks every
/// replay as "inconclusive, please validate against your fork". We
/// still stream the per-step descriptors so the user sees what the
/// replay *would* do.
#[derive(Debug, Default)]
pub struct SystemReplayRunner;

impl SystemReplayRunner {
    pub fn new() -> Self {
        Self
    }
}

impl ReplayRunner for SystemReplayRunner {
    fn execute_step(
        &self,
        _ctx: &ReplayContext,
        step_index: u32,
        step: &ReplayStep,
    ) -> CommandResult<ReplayStepTrace> {
        let (label, message) = match step {
            ReplayStep::Fork { slot, note } => (
                format!("fork@{slot}"),
                format!("Pinned fork at slot {slot} — {note}"),
            ),
            ReplayStep::CloneAccount { address, note } => (
                format!("clone {address}"),
                format!("Clone {address} from mainnet — {note}"),
            ),
            ReplayStep::SendTx {
                label,
                description,
                affected_program,
                ..
            } => (
                label.clone(),
                format!("[dry-run] would submit `{description}` touching {affected_program}"),
            ),
            ReplayStep::AssertBadState { description, .. } => (
                "assert".to_string(),
                format!("[dry-run] would verify: {description}"),
            ),
        };
        Ok(ReplayStepTrace {
            step_index,
            label,
            success: true,
            message,
            signature: None,
        })
    }

    fn verify_outcome(&self, _ctx: &ReplayContext) -> CommandResult<(ReplayOutcome, String)> {
        Ok((
            ReplayOutcome::Inconclusive,
            "Dry-run: the replay library executed the descriptor structurally. Wire a live runner to assert bad/mitigated state.".to_string(),
        ))
    }
}

pub fn run(
    runner: &dyn ReplayRunner,
    library: &ExploitLibrary,
    request: &ReplayRequest,
) -> CommandResult<ReplayReport> {
    // Target program is a pubkey or the literal program id — validate
    // trivially that it's non-empty.
    if request.target_program.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_audit_replay_missing_target",
            "A target program id is required to scope the replay.",
        ));
    }
    // Mainnet writes would be catastrophic — replay is forked-only.
    if request.cluster == ClusterKind::Mainnet {
        return Err(CommandError::policy_denied(
            "Exploit replay is not permitted against mainnet. Use the forked-mainnet cluster instead.",
        ));
    }

    let descriptor = library.get(request.exploit).ok_or_else(|| {
        CommandError::user_fixable(
            "solana_audit_replay_unknown_exploit",
            format!(
                "No built-in replay for key {:?}. Call `solana_replay_list` for the catalogue.",
                request.exploit
            ),
        )
    })?;

    let snapshot_slot = request.snapshot_slot.unwrap_or(descriptor.exploit_slot);
    let ctx = ReplayContext {
        exploit: descriptor.key,
        target_program: request.target_program.clone(),
        cluster: request.cluster,
        rpc_url: request.rpc_url.clone(),
        snapshot_slot,
    };

    let mut traces: Vec<ReplayStepTrace> = Vec::with_capacity(descriptor.steps.len());
    for (idx, step) in descriptor.steps.iter().enumerate() {
        let trace = runner.execute_step(&ctx, idx as u32, step)?;
        traces.push(trace);
    }

    let (outcome, summary) = if request.dry_run {
        (
            ReplayOutcome::Inconclusive,
            format!(
                "Dry-run: {} step(s) narrated. Wire a live runner to verify state.",
                descriptor.steps.len()
            ),
        )
    } else {
        runner.verify_outcome(&ctx)?
    };

    let findings = findings_for(descriptor, outcome);

    Ok(ReplayReport {
        run_id: String::new(),
        exploit: descriptor.key,
        target_program: request.target_program.clone(),
        cluster: request.cluster,
        snapshot_slot,
        outcome,
        dry_run: request.dry_run,
        steps: traces,
        summary,
        findings,
        reference_url: descriptor.reference_url.clone(),
    })
}

fn findings_for(descriptor: &ExploitDescriptor, outcome: ReplayOutcome) -> Vec<Finding> {
    match outcome {
        ReplayOutcome::ExpectedBadState => vec![Finding::new(
            FindingSource::Replay,
            descriptor.key.as_str(),
            FindingSeverity::Critical,
            format!("Replay produced bad state: {}", descriptor.title),
            format!(
                "The target program is vulnerable to the {} class. Replay produced the expected bad state: {}.",
                descriptor.slug, descriptor.expected_bad_state
            ),
        )
        .with_fix_hint(format!(
            "Review {} and apply the mitigation documented in the postmortem.",
            descriptor.reference_url
        ))
        .with_reference(descriptor.reference_url.clone())],
        ReplayOutcome::Mitigated => vec![Finding::new(
            FindingSource::Replay,
            format!("{}_mitigated", descriptor.key.as_str()),
            FindingSeverity::Informational,
            format!("Replay mitigated: {}", descriptor.title),
            format!(
                "Target program resisted the {} scenario — the expected bad state was NOT produced.",
                descriptor.slug
            ),
        )
        .with_reference(descriptor.reference_url.clone())],
        ReplayOutcome::UnexpectedFailure => vec![Finding::new(
            FindingSource::Replay,
            format!("{}_failed", descriptor.key.as_str()),
            FindingSeverity::Medium,
            format!("Replay failed to execute: {}", descriptor.title),
            "The replay harness could not complete. Check logs; the fork slot or cloned accounts may need to be refreshed.".to_string(),
        )
        .with_reference(descriptor.reference_url.clone())],
        ReplayOutcome::Inconclusive => vec![Finding::new(
            FindingSource::Replay,
            format!("{}_inconclusive", descriptor.key.as_str()),
            FindingSeverity::Informational,
            format!("Replay inconclusive: {}", descriptor.title),
            "Dry-run completed the structural walk. Wire a live replay runner to produce a pass/fail verdict.".to_string(),
        )
        .with_reference(descriptor.reference_url.clone())],
    }
}

fn builtin_exploits() -> Vec<ExploitDescriptor> {
    vec![
        ExploitDescriptor {
            key: ExploitKey::WormholeSigSkip,
            slug: "wormhole_sig_skip".into(),
            title: "Wormhole signature-verify bypass".into(),
            summary: "Attacker skipped signature verification on the Wormhole token bridge by pointing the ix sysvar at a spoofed account, letting them mint 120k wETH on Solana.".into(),
            exploit_slot: 117_097_056,
            reference_url: "https://rekt.news/wormhole-rekt/".into(),
            impacted_program: "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth".into(),
            clone_accounts: vec![
                "3u8hJUVTA4jH1wYAyUur7FFZVQ8H635K3tSHHF4ssjQ5".into(),
                "DZnkkTmCiFWfYTfT41X3Rd1kDgozqzxWaHqsw6W4x2oe".into(),
            ],
            clone_programs: vec![
                "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth".into(),
                "wormDTUJ6AWPNvk59vGibHW34MDZnCTzebhqGVvjdLQW".into(),
            ],
            steps: vec![
                ReplayStep::Fork { slot: 117_097_056, note: "One slot before the exploit tx".into() },
                ReplayStep::CloneAccount {
                    address: "3u8hJUVTA4jH1wYAyUur7FFZVQ8H635K3tSHHF4ssjQ5".into(),
                    note: "Bridge sequence account".into(),
                },
                ReplayStep::SendTx {
                    label: "verify_signatures".into(),
                    description: "Submit verify_signatures with a spoofed instructions sysvar".into(),
                    affected_program: "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth".into(),
                    rationale: "Original CVE: a program consumed `instructions` without checking `Sysvar::key() == sysvar::instructions::ID`.".into(),
                },
                ReplayStep::SendTx {
                    label: "post_vaa".into(),
                    description: "Post a forged VAA derived from the un-verified signatures".into(),
                    affected_program: "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth".into(),
                    rationale: "With signatures not actually verified, the VAA mints bridged wETH.".into(),
                },
                ReplayStep::AssertBadState {
                    description: "Expect 120k wETH minted to attacker ATA".into(),
                    rationale: "Target program should reject the sysvar spoof and leave the vault balance untouched.".into(),
                },
            ],
            expected_bad_state: "Bridge mints tokens without a signed VAA".into(),
        },
        ExploitDescriptor {
            key: ExploitKey::CashioFakeCollateral,
            slug: "cashio_fake_collateral".into(),
            title: "Cashio fake-collateral mint".into(),
            summary: "Cashio failed to validate that collateral LP tokens were actually minted by the expected Saber pool, letting the attacker print arbitrary CASH.".into(),
            exploit_slot: 126_080_000,
            reference_url: "https://rekt.news/cashio-rekt/".into(),
            impacted_program: "CASHVDm2wsJXfhj6VWxb7GiMdoLc17Du7paH4bNr5woT".into(),
            clone_accounts: vec![
                "FqeqCsJJjMHrC6CS9fVj7YdEBhnAP8wBzwZC5ZJ3h6o9".into(),
                "5wRjzrwWZG3af3FE26ZrRj3s8A3BVNyeJ9Pt9Uf2ogdf".into(),
            ],
            clone_programs: vec!["CASHVDm2wsJXfhj6VWxb7GiMdoLc17Du7paH4bNr5woT".into()],
            steps: vec![
                ReplayStep::Fork { slot: 126_080_000, note: "Block before the exploit".into() },
                ReplayStep::CloneAccount {
                    address: "FqeqCsJJjMHrC6CS9fVj7YdEBhnAP8wBzwZC5ZJ3h6o9".into(),
                    note: "Cashio bank state (owner + mint authority)".into(),
                },
                ReplayStep::SendTx {
                    label: "mint_fake_lp".into(),
                    description: "Create an SPL-Token mint impersonating a Saber LP mint".into(),
                    affected_program: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".into(),
                    rationale: "Cashio trusted LP mint metadata from a PDA derived off the fake mint, bypassing the real Saber check.".into(),
                },
                ReplayStep::SendTx {
                    label: "print_cash".into(),
                    description: "Deposit the fake LP and print CASH against it".into(),
                    affected_program: "CASHVDm2wsJXfhj6VWxb7GiMdoLc17Du7paH4bNr5woT".into(),
                    rationale: "Program accepts the fake collateral and mints CASH to the attacker wallet.".into(),
                },
                ReplayStep::AssertBadState {
                    description: "Expect attacker CASH balance > 0 with no Saber-issued LP deposit".into(),
                    rationale: "Target program must reject unknown LP mints or verify the mint authority on-chain.".into(),
                },
            ],
            expected_bad_state: "Arbitrary CASH minted against an attacker-controlled LP token".into(),
        },
        ExploitDescriptor {
            key: ExploitKey::MangoOracleManip,
            slug: "mango_oracle_manip".into(),
            title: "Mango Markets oracle manipulation drain".into(),
            summary: "Avraham Eisenberg pumped the MNGO perp price on Mango v3, then borrowed against the inflated unrealised PnL to drain the treasury.".into(),
            exploit_slot: 153_162_790,
            reference_url: "https://rekt.news/mango-markets-rekt/".into(),
            impacted_program: "mv3ekLzLbnVPNxjSKvqBpU3ZeZXPQdEC3bp5MDEBG68".into(),
            clone_accounts: vec![
                "4MangoMjqJ2firMokCjjGgoK8d4MXcrgL7XJaL3w6fVg".into(),
                "G8KnvNg5puzLmxQVeWT2cRHCm1XmurbWBDZCu3mRqjW3".into(),
            ],
            clone_programs: vec!["mv3ekLzLbnVPNxjSKvqBpU3ZeZXPQdEC3bp5MDEBG68".into()],
            steps: vec![
                ReplayStep::Fork { slot: 153_162_790, note: "Right before Eisenberg's MNGO ramp".into() },
                ReplayStep::CloneAccount {
                    address: "G8KnvNg5puzLmxQVeWT2cRHCm1XmurbWBDZCu3mRqjW3".into(),
                    note: "MNGO/USDC perp market".into(),
                },
                ReplayStep::SendTx {
                    label: "spot_and_perp_long".into(),
                    description: "Open a large MNGO perp long from one account".into(),
                    affected_program: "mv3ekLzLbnVPNxjSKvqBpU3ZeZXPQdEC3bp5MDEBG68".into(),
                    rationale: "Sets up the unrealised PnL vector before the price ramp.".into(),
                },
                ReplayStep::SendTx {
                    label: "pump_oracle".into(),
                    description: "Push spot MNGO up 10x via a coordinated buy on the source oracle market".into(),
                    affected_program: "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin".into(),
                    rationale: "Mango oracle took the source mid-price with no TWAP or sanity ceiling.".into(),
                },
                ReplayStep::SendTx {
                    label: "borrow_against_pnl".into(),
                    description: "Borrow USDC against the inflated unrealised PnL".into(),
                    affected_program: "mv3ekLzLbnVPNxjSKvqBpU3ZeZXPQdEC3bp5MDEBG68".into(),
                    rationale: "With the oracle pumped, the collateral appears ~100x overvalued.".into(),
                },
                ReplayStep::AssertBadState {
                    description: "Expect > $100M USDC drained with open long positions that cannot be unwound".into(),
                    rationale: "Target should price oracle reads through a TWAP and cap unrealised PnL as borrow collateral.".into(),
                },
            ],
            expected_bad_state: "Treasury drained via unrealised-PnL collateral on a manipulated oracle".into(),
        },
        ExploitDescriptor {
            key: ExploitKey::NirvanaFlashLoan,
            slug: "nirvana_flash_loan".into(),
            title: "Nirvana flash-loan redemption skew".into(),
            summary: "Attacker borrowed USDC via Solend flash-loan, bought ANA until the bonding-curve redemption price spiked, then redeemed ANA for USDC at the skewed price.".into(),
            exploit_slot: 144_260_000,
            reference_url: "https://rekt.news/nirvana-rekt/".into(),
            impacted_program: "NirvaNa11111111111111111111111111111111112".into(),
            clone_accounts: vec![
                "nirvEy5XhW4iepH6pDKvB6FpDKSmAkM2a6rFuCRphvg".into(),
            ],
            clone_programs: vec![
                "NirvaNa11111111111111111111111111111111112".into(),
                "So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo".into(),
            ],
            steps: vec![
                ReplayStep::Fork { slot: 144_260_000, note: "Slot before the flash-loan drain".into() },
                ReplayStep::CloneAccount {
                    address: "nirvEy5XhW4iepH6pDKvB6FpDKSmAkM2a6rFuCRphvg".into(),
                    note: "Nirvana treasury + ANA mint".into(),
                },
                ReplayStep::SendTx {
                    label: "flash_loan_usdc".into(),
                    description: "Borrow $10M USDC via Solend flash-loan".into(),
                    affected_program: "So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo".into(),
                    rationale: "Supplies the capital needed to walk the bonding curve in one tx.".into(),
                },
                ReplayStep::SendTx {
                    label: "buy_ana_steep".into(),
                    description: "Buy ANA along the bonding curve until redemption price spikes".into(),
                    affected_program: "NirvaNa11111111111111111111111111111111112".into(),
                    rationale: "Nirvana's redemption math did not bound intra-tx slippage.".into(),
                },
                ReplayStep::SendTx {
                    label: "redeem_at_spike".into(),
                    description: "Redeem ANA for USDC at the skewed price".into(),
                    affected_program: "NirvaNa11111111111111111111111111111111112".into(),
                    rationale: "Redemption was calculated off the instantaneous curve, not a TWAP.".into(),
                },
                ReplayStep::SendTx {
                    label: "repay_flash".into(),
                    description: "Repay the Solend flash-loan".into(),
                    affected_program: "So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo".into(),
                    rationale: "Closes the flash-loan loop; attacker keeps the skew delta.".into(),
                },
                ReplayStep::AssertBadState {
                    description: "Expect attacker USDC balance > pre-tx by 3.5M with ANA mint supply unchanged".into(),
                    rationale: "Target program must bound intra-tx curve movement and TWAP the redemption math.".into(),
                },
            ],
            expected_bad_state: "Treasury drained via single-tx bonding-curve skew".into(),
        },
    ]
}

/// Convenience: used by the backend to reject absolute paths that slip
/// through the argument plumbing. Not strictly part of the replay
/// library but handy enough to keep co-located.
#[allow(dead_code)]
pub(crate) fn reject_if_not_file(path: &str) -> CommandResult<()> {
    if !Path::new(path).is_file() {
        return Err(CommandError::user_fixable(
            "solana_audit_replay_bad_path",
            format!("Expected a file at {path}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
pub mod test_support {
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ScriptedReplayRunner {
        pub step_messages: Mutex<Vec<(u32, String)>>,
        pub outcome: Mutex<Option<(ReplayOutcome, String)>>,
        pub fail_step: Mutex<Option<u32>>,
    }

    impl ScriptedReplayRunner {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn set_outcome(&self, outcome: ReplayOutcome, summary: &str) {
            *self.outcome.lock().unwrap() = Some((outcome, summary.to_string()));
        }
        pub fn fail_at(&self, idx: u32) {
            *self.fail_step.lock().unwrap() = Some(idx);
        }
    }

    impl ReplayRunner for ScriptedReplayRunner {
        fn execute_step(
            &self,
            _ctx: &ReplayContext,
            step_index: u32,
            step: &ReplayStep,
        ) -> CommandResult<ReplayStepTrace> {
            let msg = match step {
                ReplayStep::Fork { slot, .. } => format!("forked@{slot}"),
                ReplayStep::CloneAccount { address, .. } => format!("cloned {address}"),
                ReplayStep::SendTx { label, .. } => format!("sent {label}"),
                ReplayStep::AssertBadState { description, .. } => {
                    format!("asserted {description}")
                }
            };
            self.step_messages.lock().unwrap().push((step_index, msg.clone()));
            let fail_at = *self.fail_step.lock().unwrap();
            let success = Some(step_index) != fail_at;
            Ok(ReplayStepTrace {
                step_index,
                label: format!("step-{step_index}"),
                success,
                message: msg,
                signature: None,
            })
        }

        fn verify_outcome(
            &self,
            _ctx: &ReplayContext,
        ) -> CommandResult<(ReplayOutcome, String)> {
            Ok(self
                .outcome
                .lock()
                .unwrap()
                .clone()
                .unwrap_or((ReplayOutcome::Inconclusive, "test-scripted".into())))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::ScriptedReplayRunner;
    use super::*;

    #[test]
    fn library_contains_four_builtin_exploits() {
        let lib = ExploitLibrary::builtin();
        let descriptors = lib.all();
        assert_eq!(descriptors.len(), 4);
        let keys: Vec<_> = descriptors.iter().map(|d| d.key).collect();
        assert!(keys.contains(&ExploitKey::WormholeSigSkip));
        assert!(keys.contains(&ExploitKey::CashioFakeCollateral));
        assert!(keys.contains(&ExploitKey::MangoOracleManip));
        assert!(keys.contains(&ExploitKey::NirvanaFlashLoan));
    }

    #[test]
    fn wormhole_descriptor_has_fork_and_assert_steps() {
        let lib = ExploitLibrary::builtin();
        let d = lib.get(ExploitKey::WormholeSigSkip).unwrap();
        assert!(matches!(d.steps.first(), Some(ReplayStep::Fork { .. })));
        assert!(matches!(
            d.steps.last(),
            Some(ReplayStep::AssertBadState { .. })
        ));
    }

    #[test]
    fn rejects_mainnet_replay_for_safety() {
        let lib = ExploitLibrary::builtin();
        let runner = ScriptedReplayRunner::new();
        let err = run(
            &runner,
            &lib,
            &ReplayRequest {
                exploit: ExploitKey::WormholeSigSkip,
                target_program: "Prog111".into(),
                cluster: ClusterKind::Mainnet,
                rpc_url: None,
                dry_run: true,
                snapshot_slot: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
    }

    #[test]
    fn rejects_empty_target_program() {
        let lib = ExploitLibrary::builtin();
        let runner = ScriptedReplayRunner::new();
        let err = run(
            &runner,
            &lib,
            &ReplayRequest {
                exploit: ExploitKey::CashioFakeCollateral,
                target_program: "".into(),
                cluster: ClusterKind::MainnetFork,
                rpc_url: None,
                dry_run: false,
                snapshot_slot: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_audit_replay_missing_target");
    }

    #[test]
    fn dry_run_returns_inconclusive_with_every_step_traced() {
        let lib = ExploitLibrary::builtin();
        let runner = ScriptedReplayRunner::new();
        let report = run(
            &runner,
            &lib,
            &ReplayRequest {
                exploit: ExploitKey::MangoOracleManip,
                target_program: "mv3ekLzLbnVPNxjSKvqBpU3ZeZXPQdEC3bp5MDEBG68".into(),
                cluster: ClusterKind::MainnetFork,
                rpc_url: None,
                dry_run: true,
                snapshot_slot: None,
            },
        )
        .unwrap();
        assert_eq!(report.outcome, ReplayOutcome::Inconclusive);
        let descriptor = lib.get(ExploitKey::MangoOracleManip).unwrap();
        assert_eq!(report.steps.len(), descriptor.steps.len());
    }

    #[test]
    fn expected_bad_state_produces_critical_finding() {
        let lib = ExploitLibrary::builtin();
        let runner = ScriptedReplayRunner::new();
        runner.set_outcome(ReplayOutcome::ExpectedBadState, "vulnerable");
        let report = run(
            &runner,
            &lib,
            &ReplayRequest {
                exploit: ExploitKey::WormholeSigSkip,
                target_program: "Prog111".into(),
                cluster: ClusterKind::MainnetFork,
                rpc_url: None,
                dry_run: false,
                snapshot_slot: None,
            },
        )
        .unwrap();
        assert_eq!(report.outcome, ReplayOutcome::ExpectedBadState);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].severity, FindingSeverity::Critical);
    }

    #[test]
    fn mitigated_outcome_produces_informational_finding() {
        let lib = ExploitLibrary::builtin();
        let runner = ScriptedReplayRunner::new();
        runner.set_outcome(ReplayOutcome::Mitigated, "target resisted");
        let report = run(
            &runner,
            &lib,
            &ReplayRequest {
                exploit: ExploitKey::NirvanaFlashLoan,
                target_program: "Prog111".into(),
                cluster: ClusterKind::MainnetFork,
                rpc_url: None,
                dry_run: false,
                snapshot_slot: None,
            },
        )
        .unwrap();
        assert_eq!(report.outcome, ReplayOutcome::Mitigated);
        assert_eq!(report.findings[0].severity, FindingSeverity::Informational);
    }

    #[test]
    fn exploit_key_serializes_as_snake_case() {
        let serialized = serde_json::to_string(&ExploitKey::WormholeSigSkip).unwrap();
        assert_eq!(serialized, "\"wormhole_sig_skip\"");
        let parsed: ExploitKey = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, ExploitKey::WormholeSigSkip);
    }

    #[test]
    fn unknown_exploit_key_is_user_fixable() {
        // Simulate an unknown key by building an empty library.
        let lib = ExploitLibrary {
            exploits: HashMap::new(),
        };
        let runner = ScriptedReplayRunner::new();
        let err = run(
            &runner,
            &lib,
            &ReplayRequest {
                exploit: ExploitKey::WormholeSigSkip,
                target_program: "Prog111".into(),
                cluster: ClusterKind::Localnet,
                rpc_url: None,
                dry_run: true,
                snapshot_slot: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_audit_replay_unknown_exploit");
    }
}
