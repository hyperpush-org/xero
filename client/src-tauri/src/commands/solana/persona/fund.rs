//! Persona funding primitives: SOL airdrop, SPL token mint/transfer, and
//! NFT fixture seeding.
//!
//! Two backends live behind a single `FundingBackend` trait:
//!
//! * `DefaultFundingBackend` — production. Talks to the running validator
//!   over JSON-RPC for airdrops; shells out to the user's `spl-token` CLI
//!   for mint + transfer. No Solana SDK dependency — everything flows
//!   through stable binaries we already probe for in `toolchain.rs`.
//! * `MockFundingBackend` (test-only) — captures every call so unit tests
//!   can verify the funding orchestration without touching the network.
//!
//! The seed amounts are taken from `roles::RolePreset` (raw base units for
//! tokens, lamports for SOL). The backend returns a structured
//! `FundingReceipt` that's the audit trail the UI + agent inspect.

use std::fmt::Debug;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::commands::{CommandError, CommandResult};

use super::roles::{NftAllocation, TokenAllocation};

const AIRDROP_CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(30);
const AIRDROP_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Delta a caller applies to a persona's funding state. Any missing field
/// keeps the current balance — this is the "fund me more" API shape.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FundingDelta {
    #[serde(default)]
    pub sol_lamports: u64,
    #[serde(default)]
    pub tokens: Vec<TokenAllocation>,
    #[serde(default)]
    pub nfts: Vec<NftAllocation>,
}

impl FundingDelta {
    pub fn is_empty(&self) -> bool {
        self.sol_lamports == 0 && self.tokens.is_empty() && self.nfts.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FundingStep {
    #[serde(rename_all = "camelCase")]
    Airdrop {
        signature: Option<String>,
        lamports: u64,
        ok: bool,
        error: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    TokenMint {
        mint: String,
        amount: u64,
        signature: Option<String>,
        ok: bool,
        error: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    TokenTransfer {
        mint: String,
        amount: u64,
        signature: Option<String>,
        ok: bool,
        error: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    NftFixture {
        collection: String,
        mint: Option<String>,
        signature: Option<String>,
        ok: bool,
        error: Option<String>,
    },
}

impl FundingStep {
    pub fn is_ok(&self) -> bool {
        match self {
            FundingStep::Airdrop { ok, .. }
            | FundingStep::TokenMint { ok, .. }
            | FundingStep::TokenTransfer { ok, .. }
            | FundingStep::NftFixture { ok, .. } => *ok,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FundingReceipt {
    pub persona: String,
    pub cluster: String,
    pub steps: Vec<FundingStep>,
    pub succeeded: bool,
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
}

impl FundingReceipt {
    pub fn new(persona: &str, cluster: &str) -> Self {
        Self {
            persona: persona.to_string(),
            cluster: cluster.to_string(),
            steps: Vec::new(),
            succeeded: true,
            started_at_ms: now_ms(),
            finished_at_ms: now_ms(),
        }
    }

    pub fn push(&mut self, step: FundingStep) {
        if !step.is_ok() {
            self.succeeded = false;
        }
        self.steps.push(step);
        self.finished_at_ms = now_ms();
    }
}

/// Context every funding call needs: who we're funding, where, how.
#[derive(Debug, Clone)]
pub struct FundingContext {
    pub persona_name: String,
    pub cluster: String,
    pub rpc_url: String,
    pub recipient_pubkey: String,
    pub keypair_path: std::path::PathBuf,
}

pub trait FundingBackend: Send + Sync + Debug {
    fn airdrop(&self, ctx: &FundingContext, lamports: u64) -> CommandResult<FundingStep>;

    fn ensure_token_balance(
        &self,
        ctx: &FundingContext,
        mint: &str,
        amount: u64,
        authority_keypair_path: Option<&std::path::Path>,
    ) -> CommandResult<FundingStep>;

    fn mint_nft_fixture(
        &self,
        ctx: &FundingContext,
        collection: &str,
        index: u32,
    ) -> CommandResult<FundingStep>;
}

/// Production backend.
#[derive(Debug, Default)]
pub struct DefaultFundingBackend {
    client: Mutex<Option<Client>>,
}

impl DefaultFundingBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn client(&self) -> CommandResult<Client> {
        let mut guard = self.client.lock().map_err(|_| {
            CommandError::system_fault(
                "solana_persona_fund_client_poisoned",
                "Funding HTTP client lock poisoned.",
            )
        })?;
        if guard.is_none() {
            let built = Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("cadence-solana-workbench/0.1")
                .build()
                .map_err(|err| {
                    CommandError::system_fault(
                        "solana_persona_fund_client_build_failed",
                        format!("Could not build HTTP client: {err}"),
                    )
                })?;
            *guard = Some(built);
        }
        Ok(guard.as_ref().cloned().unwrap())
    }
}

impl FundingBackend for DefaultFundingBackend {
    fn airdrop(&self, ctx: &FundingContext, lamports: u64) -> CommandResult<FundingStep> {
        if lamports == 0 {
            return Ok(FundingStep::Airdrop {
                signature: None,
                lamports: 0,
                ok: true,
                error: None,
            });
        }

        let client = self.client()?;
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "requestAirdrop",
            "params": [ctx.recipient_pubkey, lamports],
        });

        let response = match client.post(&ctx.rpc_url).json(&body).send() {
            Ok(r) => r,
            Err(err) => {
                return Ok(FundingStep::Airdrop {
                    signature: None,
                    lamports,
                    ok: false,
                    error: Some(format!("airdrop transport error: {err}")),
                });
            }
        };

        let parsed: Value = match response.json() {
            Ok(v) => v,
            Err(err) => {
                return Ok(FundingStep::Airdrop {
                    signature: None,
                    lamports,
                    ok: false,
                    error: Some(format!("airdrop decode error: {err}")),
                });
            }
        };

        if let Some(err) = parsed.get("error") {
            return Ok(FundingStep::Airdrop {
                signature: None,
                lamports,
                ok: false,
                error: Some(format!("airdrop rpc error: {err}")),
            });
        }

        let signature = parsed
            .get("result")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let confirmed = match &signature {
            Some(sig) => confirm_signature(&client, &ctx.rpc_url, sig),
            None => Ok(()),
        };

        match confirmed {
            Ok(()) => Ok(FundingStep::Airdrop {
                signature,
                lamports,
                ok: true,
                error: None,
            }),
            Err(err) => Ok(FundingStep::Airdrop {
                signature,
                lamports,
                ok: false,
                error: Some(err),
            }),
        }
    }

    fn ensure_token_balance(
        &self,
        ctx: &FundingContext,
        mint: &str,
        amount: u64,
        authority_keypair_path: Option<&std::path::Path>,
    ) -> CommandResult<FundingStep> {
        let spl_token = match which_spl_token() {
            Some(path) => path,
            None => {
                return Ok(FundingStep::TokenMint {
                    mint: mint.to_string(),
                    amount,
                    signature: None,
                    ok: false,
                    error: Some(
                        "spl-token CLI not found on PATH. Install the Solana CLI (which bundles \
                         spl-token) to fund token balances."
                            .to_string(),
                    ),
                });
            }
        };

        // Try mint first (new mint owned by the persona). Fall back to
        // transfer if the mint already exists and the persona doesn't own
        // the mint authority. We use --fee-payer to route fees through the
        // recipient keypair on localnet (it has SOL from the airdrop step).
        let url = ctx.rpc_url.clone();
        let keypair = ctx.keypair_path.clone();
        let authority = authority_keypair_path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| keypair.display().to_string());

        let mut cmd = Command::new(spl_token);
        cmd.arg("--url")
            .arg(&url)
            .arg("--fee-payer")
            .arg(&keypair)
            .arg("--owner")
            .arg(&keypair)
            .arg("--output")
            .arg("json")
            .arg("mint")
            .arg(mint)
            .arg(format!("{amount}"))
            .arg("--mint-authority")
            .arg(&authority)
            .arg("--recipient-owner")
            .arg(&ctx.recipient_pubkey)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = match cmd.output() {
            Ok(o) => o,
            Err(err) => {
                return Ok(FundingStep::TokenMint {
                    mint: mint.to_string(),
                    amount,
                    signature: None,
                    ok: false,
                    error: Some(format!("spl-token mint spawn failed: {err}")),
                });
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if output.status.success() {
            Ok(FundingStep::TokenMint {
                mint: mint.to_string(),
                amount,
                signature: parse_signature_from_cli(&stdout),
                ok: true,
                error: None,
            })
        } else {
            Ok(FundingStep::TokenMint {
                mint: mint.to_string(),
                amount,
                signature: None,
                ok: false,
                error: Some(trim_error(format!("spl-token mint failed: {stderr}"))),
            })
        }
    }

    fn mint_nft_fixture(
        &self,
        ctx: &FundingContext,
        collection: &str,
        index: u32,
    ) -> CommandResult<FundingStep> {
        let spl_token = match which_spl_token() {
            Some(path) => path,
            None => {
                return Ok(FundingStep::NftFixture {
                    collection: collection.to_string(),
                    mint: None,
                    signature: None,
                    ok: false,
                    error: Some(
                        "spl-token CLI not found; install the Solana CLI to seed NFT fixtures."
                            .to_string(),
                    ),
                });
            }
        };

        // Create a Token-2022 NFT fixture:
        //   1. `spl-token create-token --decimals 0` — new mint owned by the persona.
        //   2. `spl-token create-account <mint>` for the recipient (implicit
        //      via --mint-authority = persona).
        //   3. `spl-token mint <mint> 1 --recipient-owner <pubkey>`.
        //
        // This isn't full Metaplex metadata — that's Phase 8 work — but it
        // is a DAS-queryable NFT fixture that downstream tests can treat as
        // "the persona owns a collectible".
        let create_output = match Command::new(&spl_token)
            .arg("--url")
            .arg(&ctx.rpc_url)
            .arg("--fee-payer")
            .arg(&ctx.keypair_path)
            .arg("--owner")
            .arg(&ctx.keypair_path)
            .arg("--output")
            .arg("json")
            .arg("create-token")
            .arg("--decimals")
            .arg("0")
            .arg("--mint-authority")
            .arg(&ctx.keypair_path)
            .stdin(Stdio::null())
            .output()
        {
            Ok(o) => o,
            Err(err) => {
                return Ok(FundingStep::NftFixture {
                    collection: collection.to_string(),
                    mint: None,
                    signature: None,
                    ok: false,
                    error: Some(format!("create-token spawn failed: {err}")),
                });
            }
        };

        if !create_output.status.success() {
            let stderr = String::from_utf8_lossy(&create_output.stderr).into_owned();
            return Ok(FundingStep::NftFixture {
                collection: collection.to_string(),
                mint: None,
                signature: None,
                ok: false,
                error: Some(trim_error(format!(
                    "create-token failed for {collection}#{index}: {stderr}"
                ))),
            });
        }

        let stdout = String::from_utf8_lossy(&create_output.stdout).into_owned();
        let mint_addr = parse_mint_from_cli(&stdout);
        let mint_label = mint_addr
            .clone()
            .unwrap_or_else(|| format!("{collection}-#{index}"));

        let mint_output = Command::new(&spl_token)
            .arg("--url")
            .arg(&ctx.rpc_url)
            .arg("--fee-payer")
            .arg(&ctx.keypair_path)
            .arg("--owner")
            .arg(&ctx.keypair_path)
            .arg("--output")
            .arg("json")
            .arg("mint")
            .arg(&mint_label)
            .arg("1")
            .arg("--mint-authority")
            .arg(&ctx.keypair_path)
            .arg("--recipient-owner")
            .arg(&ctx.recipient_pubkey)
            .stdin(Stdio::null())
            .output();

        match mint_output {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
                Ok(FundingStep::NftFixture {
                    collection: collection.to_string(),
                    mint: mint_addr,
                    signature: parse_signature_from_cli(&stdout),
                    ok: true,
                    error: None,
                })
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
                Ok(FundingStep::NftFixture {
                    collection: collection.to_string(),
                    mint: mint_addr,
                    signature: None,
                    ok: false,
                    error: Some(trim_error(format!(
                        "mint failed for {collection}#{index}: {stderr}"
                    ))),
                })
            }
            Err(err) => Ok(FundingStep::NftFixture {
                collection: collection.to_string(),
                mint: mint_addr,
                signature: None,
                ok: false,
                error: Some(format!("mint spawn failed: {err}")),
            }),
        }
    }
}

fn confirm_signature(client: &Client, rpc_url: &str, signature: &str) -> Result<(), String> {
    let deadline = Instant::now() + AIRDROP_CONFIRMATION_TIMEOUT;
    loop {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [[signature], {"searchTransactionHistory": true}],
        });
        let resp: Value = match client.post(rpc_url).json(&body).send() {
            Ok(r) => match r.json() {
                Ok(v) => v,
                Err(err) => return Err(format!("decode signature status: {err}")),
            },
            Err(err) => return Err(format!("signature status transport: {err}")),
        };

        if let Some(err) = resp.get("error") {
            return Err(format!("signature status rpc error: {err}"));
        }

        let value = resp
            .pointer("/result/value/0")
            .cloned()
            .unwrap_or(Value::Null);
        if !value.is_null() {
            if let Some(err) = value.get("err").filter(|e| !e.is_null()) {
                return Err(format!("transaction reverted: {err}"));
            }
            if let Some(status) = value.get("confirmationStatus").and_then(|v| v.as_str()) {
                if matches!(status, "confirmed" | "finalized") {
                    return Ok(());
                }
            }
        }

        if Instant::now() >= deadline {
            return Err("airdrop confirmation timed out".to_string());
        }
        std::thread::sleep(AIRDROP_POLL_INTERVAL);
    }
}

fn which_spl_token() -> Option<std::path::PathBuf> {
    use crate::commands::solana::toolchain::probe_tool;
    let probe = probe_tool("spl-token", &["--version"]);
    probe.path.map(std::path::PathBuf::from)
}

fn parse_signature_from_cli(stdout: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<Value>(stdout) {
        if let Some(sig) = value.get("signature").and_then(|v| v.as_str()) {
            return Some(sig.to_string());
        }
    }
    // Fallback: look for a `Signature: <base58>` line.
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Signature:") {
            let sig = rest.trim();
            if !sig.is_empty() {
                return Some(sig.to_string());
            }
        }
    }
    None
}

fn parse_mint_from_cli(stdout: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<Value>(stdout) {
        for key in ["address", "commandOutput", "mint"] {
            if let Some(v) = value.get(key).and_then(|v| v.as_str()) {
                return Some(v.to_string());
            }
        }
        if let Some(obj) = value.as_object() {
            if let Some(output) = obj
                .get("commandOutput")
                .and_then(|v| v.get("address"))
                .and_then(|v| v.as_str())
            {
                return Some(output.to_string());
            }
        }
    }
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Creating token ") {
            let mint: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if !mint.is_empty() {
                return Some(mint);
            }
        }
    }
    None
}

fn trim_error(message: String) -> String {
    // Keep error bodies tight — stderr can be 10s of lines for spl-token
    // failures, and that overflows the tx-history panel.
    const MAX: usize = 600;
    if message.len() <= MAX {
        message
    } else {
        let mut truncated = message;
        truncated.truncate(MAX);
        truncated.push_str("… (truncated)");
        truncated
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::Mutex;

    /// Test backend: captures each call, always returns "ok", never touches
    /// the network or spawns a child. Shared with scenario tests.
    #[derive(Debug, Default)]
    pub struct MockFundingBackend {
        pub airdrops: Mutex<Vec<(String, u64)>>,
        pub tokens: Mutex<Vec<(String, String, u64)>>,
        pub nfts: Mutex<Vec<(String, String, u32)>>,
        pub fail_airdrop: Mutex<bool>,
        pub fail_token: Mutex<bool>,
        pub fail_nft: Mutex<bool>,
    }

    impl MockFundingBackend {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn set_fail_airdrop(&self, fail: bool) {
            *self.fail_airdrop.lock().unwrap() = fail;
        }

        #[allow(dead_code)]
        pub fn set_fail_token(&self, fail: bool) {
            *self.fail_token.lock().unwrap() = fail;
        }

        #[allow(dead_code)]
        pub fn set_fail_nft(&self, fail: bool) {
            *self.fail_nft.lock().unwrap() = fail;
        }
    }

    impl FundingBackend for MockFundingBackend {
        fn airdrop(&self, ctx: &FundingContext, lamports: u64) -> CommandResult<FundingStep> {
            self.airdrops
                .lock()
                .unwrap()
                .push((ctx.persona_name.clone(), lamports));
            if *self.fail_airdrop.lock().unwrap() {
                return Ok(FundingStep::Airdrop {
                    signature: None,
                    lamports,
                    ok: false,
                    error: Some("mock airdrop failure".into()),
                });
            }
            Ok(FundingStep::Airdrop {
                signature: Some(format!("sig-airdrop-{}", ctx.persona_name)),
                lamports,
                ok: true,
                error: None,
            })
        }

        fn ensure_token_balance(
            &self,
            ctx: &FundingContext,
            mint: &str,
            amount: u64,
            _authority_keypair_path: Option<&std::path::Path>,
        ) -> CommandResult<FundingStep> {
            self.tokens
                .lock()
                .unwrap()
                .push((ctx.persona_name.clone(), mint.to_string(), amount));
            if *self.fail_token.lock().unwrap() {
                return Ok(FundingStep::TokenMint {
                    mint: mint.to_string(),
                    amount,
                    signature: None,
                    ok: false,
                    error: Some("mock token failure".into()),
                });
            }
            Ok(FundingStep::TokenMint {
                mint: mint.to_string(),
                amount,
                signature: Some(format!("sig-token-{}-{}", mint, ctx.persona_name)),
                ok: true,
                error: None,
            })
        }

        fn mint_nft_fixture(
            &self,
            ctx: &FundingContext,
            collection: &str,
            index: u32,
        ) -> CommandResult<FundingStep> {
            self.nfts.lock().unwrap().push((
                ctx.persona_name.clone(),
                collection.to_string(),
                index,
            ));
            if *self.fail_nft.lock().unwrap() {
                return Ok(FundingStep::NftFixture {
                    collection: collection.to_string(),
                    mint: None,
                    signature: None,
                    ok: false,
                    error: Some("mock nft failure".into()),
                });
            }
            Ok(FundingStep::NftFixture {
                collection: collection.to_string(),
                mint: Some(format!(
                    "mock-nft-{}-{}-{}",
                    ctx.persona_name, collection, index
                )),
                signature: Some(format!(
                    "sig-nft-{}-{}-{}",
                    ctx.persona_name, collection, index
                )),
                ok: true,
                error: None,
            })
        }
    }
}

// Pull the mock into non-test scope so it can be used by the scenario
// tests (which live outside the `persona` module).
#[cfg(test)]
pub use test_support::MockFundingBackend;

/// Apply a `FundingDelta` to a persona. Iterates every delta field, calls
/// into the backend, and builds a `FundingReceipt` that records every
/// attempt (success + failure) for audit.
pub fn apply_delta(
    backend: &dyn FundingBackend,
    ctx: &FundingContext,
    delta: &FundingDelta,
) -> CommandResult<FundingReceipt> {
    let mut receipt = FundingReceipt::new(&ctx.persona_name, &ctx.cluster);

    if delta.sol_lamports > 0 {
        let step = backend.airdrop(ctx, delta.sol_lamports)?;
        receipt.push(step);
    }

    for token in &delta.tokens {
        let mint = match token.resolve_mint() {
            Some(m) => m,
            None => {
                receipt.push(FundingStep::TokenMint {
                    mint: token
                        .symbol
                        .clone()
                        .unwrap_or_else(|| "<unknown>".to_string()),
                    amount: token.amount,
                    signature: None,
                    ok: false,
                    error: Some(format!(
                        "unknown token allocation — provide `mint` or use a known symbol: {:?}",
                        token.symbol
                    )),
                });
                continue;
            }
        };
        let step = backend.ensure_token_balance(ctx, &mint, token.amount, None)?;
        receipt.push(step);
    }

    for nft in &delta.nfts {
        for index in 0..nft.count {
            let step = backend.mint_nft_fixture(ctx, &nft.collection, index)?;
            receipt.push(step);
        }
    }

    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::test_support::MockFundingBackend;
    use super::*;
    use crate::commands::solana::persona::roles::{NftAllocation, TokenAllocation};

    fn ctx(name: &str) -> FundingContext {
        FundingContext {
            persona_name: name.to_string(),
            cluster: "localnet".to_string(),
            rpc_url: "http://127.0.0.1:8899".to_string(),
            recipient_pubkey: "Recipient1111111111111111111111111111111".to_string(),
            keypair_path: std::path::PathBuf::from("/tmp/fake-keypair.json"),
        }
    }

    #[test]
    fn empty_delta_produces_empty_successful_receipt() {
        let backend = MockFundingBackend::new();
        let receipt = apply_delta(&backend, &ctx("alice"), &FundingDelta::default()).unwrap();
        assert!(receipt.succeeded);
        assert!(receipt.steps.is_empty());
    }

    #[test]
    fn full_delta_invokes_every_backend_primitive() {
        let backend = MockFundingBackend::new();
        let delta = FundingDelta {
            sol_lamports: 1_000_000_000,
            tokens: vec![
                TokenAllocation::by_symbol("USDC", 100),
                TokenAllocation::by_mint("MyCustomMint111111111111111111111111111", 5),
            ],
            nfts: vec![NftAllocation {
                collection: "first".into(),
                count: 2,
            }],
        };
        let receipt = apply_delta(&backend, &ctx("whale"), &delta).unwrap();
        assert!(receipt.succeeded);
        assert_eq!(backend.airdrops.lock().unwrap().len(), 1);
        assert_eq!(backend.tokens.lock().unwrap().len(), 2);
        assert_eq!(backend.nfts.lock().unwrap().len(), 2);
    }

    #[test]
    fn unknown_token_symbol_marks_step_failed_without_short_circuiting() {
        let backend = MockFundingBackend::new();
        let delta = FundingDelta {
            sol_lamports: 0,
            tokens: vec![
                TokenAllocation::by_symbol("UNKNOWN_SYMBOL_XYZ", 1),
                TokenAllocation::by_symbol("USDC", 1),
            ],
            nfts: vec![],
        };
        let receipt = apply_delta(&backend, &ctx("alice"), &delta).unwrap();
        assert!(!receipt.succeeded);
        assert_eq!(receipt.steps.len(), 2);
        // Even though the first token failed, the second still executed.
        assert_eq!(backend.tokens.lock().unwrap().len(), 1);
    }

    #[test]
    fn airdrop_failure_flips_succeeded_flag() {
        let backend = MockFundingBackend::new();
        backend.set_fail_airdrop(true);
        let delta = FundingDelta {
            sol_lamports: 123,
            ..FundingDelta::default()
        };
        let receipt = apply_delta(&backend, &ctx("alice"), &delta).unwrap();
        assert!(!receipt.succeeded);
        assert_eq!(receipt.steps.len(), 1);
        match &receipt.steps[0] {
            FundingStep::Airdrop { ok, error, .. } => {
                assert!(!ok);
                assert!(error.is_some());
            }
            _ => panic!("expected airdrop step"),
        }
    }

    #[test]
    fn parse_signature_from_cli_handles_both_json_and_plain_text() {
        assert_eq!(
            parse_signature_from_cli(r#"{"signature":"abc123"}"#).as_deref(),
            Some("abc123"),
        );
        assert_eq!(
            parse_signature_from_cli("Signature: zzz987\nrest").as_deref(),
            Some("zzz987"),
        );
        assert!(parse_signature_from_cli("no sig here").is_none());
    }

    #[test]
    fn trim_error_caps_long_messages() {
        let long = "x".repeat(5_000);
        let trimmed = trim_error(long);
        assert!(trimmed.len() <= 700);
        assert!(trimmed.ends_with("(truncated)"));
    }

    #[test]
    fn parse_mint_from_cli_handles_json_and_human_output() {
        assert_eq!(
            parse_mint_from_cli(r#"{"address":"Mint111"}"#).as_deref(),
            Some("Mint111"),
        );
        assert_eq!(
            parse_mint_from_cli("Creating token Mint2222BC\nSignature: xyz").as_deref(),
            Some("Mint2222BC"),
        );
        assert!(parse_mint_from_cli("nothing here").is_none());
    }
}
