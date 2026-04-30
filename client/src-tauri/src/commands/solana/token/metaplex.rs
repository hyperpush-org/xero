//! Metaplex NFT mint via Umi.
//!
//! Umi is a TypeScript-first framework; rather than reimplement the
//! mpl-token-metadata instruction layout inside the Rust backend we
//! materialise a deterministic Node worker script on first use and
//! invoke it through `node` with every argument supplied via env-var
//! (so keypaths / seeds never leak into the argv).
//!
//! The runner trait keeps integration tests free of a real Node
//! installation — the test runner returns a scripted
//! `MetaplexMintOutcome` and we assert on the captured
//! `MetaplexMintInvocation` (argv, envs, cwd).
//!
//! Output shape is intentionally narrow: mint pubkey + signature. DAS
//! indexability is a property of the on-chain data, not of this
//! workbench helper.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::toolchain;
use crate::commands::{CommandError, CommandResult};

/// What token standard to mint under. Umi handles all three; the worker
/// dispatches via `createV1` variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum MetaplexStandard {
    /// Non-fungible. Single supply, decimals 0.
    #[default]
    NonFungible,
    /// Fungible with metadata (semi-fungible tokens).
    Fungible,
    /// Programmable NFT — pNFT with token standard 4.
    ProgrammableNonFungible,
}

impl MetaplexStandard {
    pub fn as_str(self) -> &'static str {
        match self {
            MetaplexStandard::NonFungible => "non_fungible",
            MetaplexStandard::Fungible => "fungible",
            MetaplexStandard::ProgrammableNonFungible => "programmable_non_fungible",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetaplexMintRequest {
    pub cluster: ClusterKind,
    /// Persona whose keypair will sign the mint (and pay rent).
    pub authority_persona: String,
    /// Off-chain metadata URI (ipfs://, https://, ar://…). Umi fetches
    /// the JSON to sanity-check the schema.
    pub metadata_uri: String,
    /// On-chain name — Metaplex caps at 32 bytes after UTF-8 encoding.
    pub name: String,
    /// On-chain symbol — capped at 10 bytes after UTF-8 encoding.
    pub symbol: String,
    /// Destination wallet for the minted NFT. Defaults to the authority
    /// when absent.
    #[serde(default)]
    pub recipient: Option<String>,
    /// Optional collection mint that this NFT should join; the worker
    /// verifies the collection via `verifyCollectionV1` after the mint.
    #[serde(default)]
    pub collection_mint: Option<String>,
    /// Seller fee in basis points for royalties. Defaults to 0 when
    /// absent. Must be 0..=10_000.
    #[serde(default)]
    pub seller_fee_bps: Option<u16>,
    #[serde(default)]
    pub standard: MetaplexStandard,
    /// Optional override for the Node binary. Defaults to `node` on
    /// PATH.
    #[serde(default)]
    pub node_bin: Option<String>,
    /// When true, the worker is overwritten even if one already exists
    /// — used when the bundled script changes across Xero versions.
    #[serde(default)]
    pub refresh_worker: bool,
    /// Optional RPC URL override; resolved by the caller.
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaplexMintInvocation {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub envs: Vec<(OsString, OsString)>,
    pub worker_path: PathBuf,
    pub worker_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaplexMintOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub mint_address: Option<String>,
    pub signature: Option<String>,
}

pub trait MetaplexMintRunner: Send + Sync + std::fmt::Debug {
    fn run(&self, invocation: &MetaplexMintInvocation) -> CommandResult<MetaplexMintOutcome>;
}

#[derive(Debug, Default)]
pub struct SystemMetaplexRunner;

impl SystemMetaplexRunner {
    pub fn new() -> Self {
        Self
    }
}

impl MetaplexMintRunner for SystemMetaplexRunner {
    fn run(&self, invocation: &MetaplexMintInvocation) -> CommandResult<MetaplexMintOutcome> {
        let (program, args) = invocation.argv.split_first().ok_or_else(|| {
            CommandError::system_fault(
                "solana_metaplex_mint_empty_argv",
                "Empty argv passed to metaplex mint runner.",
            )
        })?;
        let resolved_program = toolchain::resolve_command(program);
        let mut cmd = Command::new(&resolved_program);
        cmd.args(args)
            .current_dir(&invocation.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        for (k, v) in &invocation.envs {
            cmd.env(k, v);
        }
        toolchain::augment_command(&mut cmd);
        let child = cmd.spawn().map_err(|err| {
            CommandError::user_fixable(
                "solana_metaplex_mint_spawn_failed",
                format!(
                    "Could not run `{program}`: {err}. Install Node 20+ in the managed toolchain or ensure it is on PATH.",
                ),
            )
        })?;
        let output = wait_with_timeout(child, invocation.timeout).ok_or_else(|| {
            CommandError::retryable(
                "solana_metaplex_mint_timeout",
                format!(
                    "Metaplex mint timed out after {}s.",
                    invocation.timeout.as_secs()
                ),
            )
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let parsed = parse_worker_output(&stdout);
        Ok(MetaplexMintOutcome {
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout,
            stderr,
            mint_address: parsed.as_ref().and_then(|r| r.mint.clone()),
            signature: parsed.and_then(|r| r.signature),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetaplexMintResult {
    pub cluster: ClusterKind,
    pub standard: MetaplexStandard,
    pub argv: Vec<String>,
    pub worker_path: String,
    pub worker_sha256: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub elapsed_ms: u128,
    pub mint_address: Option<String>,
    pub signature: Option<String>,
}

pub fn mint_metaplex_nft(
    runner: &dyn MetaplexMintRunner,
    worker_root: &Path,
    authority_keypair_path: &Path,
    request: MetaplexMintRequest,
) -> CommandResult<MetaplexMintResult> {
    validate(&request)?;
    let rpc_url = request.rpc_url.clone().ok_or_else(|| {
        CommandError::user_fixable(
            "solana_metaplex_mint_no_rpc",
            "No RPC URL available — start a cluster or provide rpcUrl explicitly.",
        )
    })?;

    let worker_path = worker_root.join("metaplex-mint.mjs");
    let worker_sha256 = materialise_worker(&worker_path, request.refresh_worker)?;

    let node_bin = request
        .node_bin
        .clone()
        .unwrap_or_else(|| toolchain::resolve_command("node"));
    let argv = vec![node_bin, worker_path.display().to_string()];
    let cwd = worker_root.to_path_buf();

    // Route every sensitive / large value through env vars so the argv
    // stays short and audit-friendly.
    let envs: Vec<(OsString, OsString)> = vec![
        ("XERO_RPC_URL".into(), rpc_url.into()),
        (
            "XERO_AUTHORITY".into(),
            authority_keypair_path.display().to_string().into(),
        ),
        (
            "XERO_RECIPIENT".into(),
            request.recipient.clone().unwrap_or_default().into(),
        ),
        ("XERO_NAME".into(), request.name.clone().into()),
        ("XERO_SYMBOL".into(), request.symbol.clone().into()),
        (
            "XERO_METADATA_URI".into(),
            request.metadata_uri.clone().into(),
        ),
        (
            "XERO_COLLECTION".into(),
            request.collection_mint.clone().unwrap_or_default().into(),
        ),
        (
            "XERO_SELLER_FEE_BPS".into(),
            request.seller_fee_bps.unwrap_or(0).to_string().into(),
        ),
        ("XERO_STANDARD".into(), request.standard.as_str().into()),
        ("XERO_CLUSTER".into(), request.cluster.as_str().into()),
    ];
    let invocation = MetaplexMintInvocation {
        argv: argv.clone(),
        cwd,
        timeout: Duration::from_secs(180),
        envs,
        worker_path: worker_path.clone(),
        worker_sha256: worker_sha256.clone(),
    };

    let start = Instant::now();
    let outcome = runner.run(&invocation)?;
    let elapsed_ms = start.elapsed().as_millis();
    // Production runner parses XERO_MINT_RESULT eagerly; scripted
    // runners can return a bare outcome and rely on this fallback.
    let parsed_fallback = parse_worker_output(&outcome.stdout);
    let mint_address = outcome
        .mint_address
        .clone()
        .or_else(|| parsed_fallback.as_ref().and_then(|r| r.mint.clone()));
    let signature = outcome
        .signature
        .clone()
        .or_else(|| parsed_fallback.as_ref().and_then(|r| r.signature.clone()));
    Ok(MetaplexMintResult {
        cluster: request.cluster,
        standard: request.standard,
        argv,
        worker_path: worker_path.display().to_string(),
        worker_sha256,
        success: outcome.success,
        exit_code: outcome.exit_code,
        stdout_excerpt: truncate(&outcome.stdout, 8_192),
        stderr_excerpt: truncate(&outcome.stderr, 8_192),
        elapsed_ms,
        mint_address,
        signature,
    })
}

fn validate(req: &MetaplexMintRequest) -> CommandResult<()> {
    if req.authority_persona.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_metaplex_mint_missing_authority",
            "authorityPersona must be a non-empty persona on this cluster.",
        ));
    }
    if req.metadata_uri.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_metaplex_mint_missing_uri",
            "metadataUri must point at an off-chain JSON document (ipfs://, https://, ar://).",
        ));
    }
    if req.name.is_empty() || req.name.len() > 32 {
        return Err(CommandError::user_fixable(
            "solana_metaplex_mint_bad_name",
            format!(
                "name must be 1..=32 bytes (got {}); Metaplex truncates longer values.",
                req.name.len()
            ),
        ));
    }
    if req.symbol.is_empty() || req.symbol.len() > 10 {
        return Err(CommandError::user_fixable(
            "solana_metaplex_mint_bad_symbol",
            format!(
                "symbol must be 1..=10 bytes (got {}); Metaplex rejects longer values.",
                req.symbol.len()
            ),
        ));
    }
    if let Some(bps) = req.seller_fee_bps {
        if bps > 10_000 {
            return Err(CommandError::user_fixable(
                "solana_metaplex_mint_bad_seller_fee",
                format!("sellerFeeBps must be 0..=10000 (got {}).", bps),
            ));
        }
    }
    Ok(())
}

pub(crate) const METAPLEX_WORKER_SCRIPT: &str = include_str!("metaplex_worker.mjs");

fn materialise_worker(path: &Path, refresh: bool) -> CommandResult<String> {
    let desired_sha = sha256_hex(METAPLEX_WORKER_SCRIPT.as_bytes());
    if path.is_file() && !refresh {
        if let Ok(existing) = fs::read(path) {
            if sha256_hex(&existing) == desired_sha {
                return Ok(desired_sha);
            }
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            CommandError::system_fault(
                "solana_metaplex_mint_mkdir_failed",
                format!("Could not create {}: {err}", parent.display()),
            )
        })?;
    }
    fs::write(path, METAPLEX_WORKER_SCRIPT.as_bytes()).map_err(|err| {
        CommandError::system_fault(
            "solana_metaplex_mint_write_failed",
            format!("Could not write worker {}: {err}", path.display()),
        )
    })?;
    Ok(desired_sha)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[derive(Debug, Default)]
struct ParsedWorkerOutput {
    mint: Option<String>,
    signature: Option<String>,
}

/// The worker emits a single line of JSON on success: `XERO_MINT_RESULT <json>`.
/// Parse that sentinel out of stdout; fall back to best-effort pattern
/// matching so a partial success still reveals the mint address.
fn parse_worker_output(stdout: &str) -> Option<ParsedWorkerOutput> {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("XERO_MINT_RESULT ") {
            let parsed: serde_json::Value = serde_json::from_str(rest.trim()).ok()?;
            let mint = parsed
                .get("mint")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let signature = parsed
                .get("signature")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            return Some(ParsedWorkerOutput { mint, signature });
        }
    }
    None
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Option<std::process::Output> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return None,
        }
    }
    child.wait_with_output().ok()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut out = String::with_capacity(max + 20);
        out.push_str(&s[..max]);
        out.push_str("\n…[truncated]…");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    #[derive(Debug, Default)]
    struct RecordingRunner {
        calls: Mutex<Vec<MetaplexMintInvocation>>,
        outcome: Mutex<Option<MetaplexMintOutcome>>,
    }

    impl RecordingRunner {
        fn with(outcome: MetaplexMintOutcome) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                outcome: Mutex::new(Some(outcome)),
            }
        }
    }

    impl MetaplexMintRunner for RecordingRunner {
        fn run(&self, invocation: &MetaplexMintInvocation) -> CommandResult<MetaplexMintOutcome> {
            self.calls.lock().unwrap().push(invocation.clone());
            self.outcome
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| CommandError::system_fault("test", "no outcome"))
        }
    }

    fn basic_request() -> MetaplexMintRequest {
        MetaplexMintRequest {
            cluster: ClusterKind::Localnet,
            authority_persona: "whale".into(),
            metadata_uri: "https://example.com/meta.json".into(),
            name: "Example NFT".into(),
            symbol: "EX".into(),
            recipient: Some("Recip1111111111111111111111111111111111111".into()),
            collection_mint: None,
            seller_fee_bps: Some(500),
            standard: MetaplexStandard::NonFungible,
            node_bin: None,
            refresh_worker: false,
            rpc_url: Some("http://127.0.0.1:8899".into()),
        }
    }

    #[test]
    fn worker_materialises_and_hashes_to_expected_bytes() {
        let tmp = TempDir::new().unwrap();
        let runner = RecordingRunner::with(MetaplexMintOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "XERO_MINT_RESULT {\"mint\":\"4Rf9\",\"signature\":\"Sig\"}".into(),
            stderr: "".into(),
            mint_address: None,
            signature: None,
        });
        let report = mint_metaplex_nft(
            &runner,
            tmp.path(),
            Path::new("/tmp/authority.json"),
            basic_request(),
        )
        .unwrap();
        assert!(report.success);
        assert!(Path::new(&report.worker_path).exists());
        let disk = std::fs::read(&report.worker_path).unwrap();
        let expected = sha256_hex(&disk);
        assert_eq!(expected, report.worker_sha256);
    }

    #[test]
    fn runner_receives_every_required_env_var() {
        let tmp = TempDir::new().unwrap();
        let runner = RecordingRunner::with(MetaplexMintOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "XERO_MINT_RESULT {\"mint\":\"4Rf9\",\"signature\":\"Sig\"}".into(),
            stderr: "".into(),
            mint_address: None,
            signature: None,
        });
        let _ = mint_metaplex_nft(
            &runner,
            tmp.path(),
            Path::new("/tmp/authority.json"),
            basic_request(),
        )
        .unwrap();
        let calls = runner.calls.lock().unwrap();
        let env: std::collections::BTreeMap<OsString, OsString> =
            calls[0].envs.iter().cloned().collect();
        for key in [
            "XERO_RPC_URL",
            "XERO_AUTHORITY",
            "XERO_NAME",
            "XERO_SYMBOL",
            "XERO_METADATA_URI",
            "XERO_STANDARD",
            "XERO_SELLER_FEE_BPS",
            "XERO_CLUSTER",
        ] {
            assert!(
                env.contains_key::<OsString>(&key.into()),
                "env var {key} missing"
            );
        }
        assert_eq!(
            env[&OsString::from("XERO_SELLER_FEE_BPS")],
            OsString::from("500")
        );
        assert_eq!(
            env[&OsString::from("XERO_STANDARD")],
            OsString::from("non_fungible")
        );
    }

    #[test]
    fn bad_symbol_rejected() {
        let tmp = TempDir::new().unwrap();
        let runner = RecordingRunner::with(MetaplexMintOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "".into(),
            stderr: "".into(),
            mint_address: None,
            signature: None,
        });
        let mut req = basic_request();
        req.symbol = "TOOLONGSYMBOL".into();
        let err = mint_metaplex_nft(&runner, tmp.path(), Path::new("/tmp/authority.json"), req)
            .unwrap_err();
        assert_eq!(err.code, "solana_metaplex_mint_bad_symbol");
    }

    #[test]
    fn no_rpc_rejected() {
        let tmp = TempDir::new().unwrap();
        let runner = RecordingRunner::with(MetaplexMintOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "".into(),
            stderr: "".into(),
            mint_address: None,
            signature: None,
        });
        let mut req = basic_request();
        req.rpc_url = None;
        let err = mint_metaplex_nft(&runner, tmp.path(), Path::new("/tmp/authority.json"), req)
            .unwrap_err();
        assert_eq!(err.code, "solana_metaplex_mint_no_rpc");
    }

    #[test]
    fn output_parser_extracts_mint_and_signature() {
        let stdout =
            "info: bootstrapping umi\nXERO_MINT_RESULT {\"mint\":\"4Rf9\",\"signature\":\"Sig\"}\n";
        let parsed = parse_worker_output(stdout).unwrap();
        assert_eq!(parsed.mint.as_deref(), Some("4Rf9"));
        assert_eq!(parsed.signature.as_deref(), Some("Sig"));
    }
}
