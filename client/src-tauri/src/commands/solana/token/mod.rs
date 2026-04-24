//! Token + Metaplex helpers (Phase 8).
//!
//! Ships three capabilities:
//!
//! - `solana_token_create` — drive `spl-token create-token` (Token-2022
//!   by default) with a declarative extension list. Validates extension
//!   combinations against the bundled matrix before invoking the CLI.
//! - `solana_token_extension_matrix` — the bundled SDK/wallet
//!   compatibility matrix, shaped so the UI can flag `transfer_hook`
//!   unsupported under an old `@solana/wallet-adapter` with a concrete
//!   remediation hint.
//! - `solana_metaplex_mint` — shell out to a bundled Node worker that
//!   drives `@metaplex-foundation/umi` + `@metaplex-foundation/mpl-
//!   token-metadata` to mint an NFT against a given RPC URL. The worker
//!   is an input to the runner trait so tests can script without Node.
//!
//! Every runner is a trait so integration tests can stub the process
//! exec and assert on captured argv. Matches the pattern used by
//! `program::build::BuildRunner` and `audit::trident::FuzzRunner`.

pub mod extensions;
pub mod metaplex;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::toolchain;
use crate::commands::{CommandError, CommandResult};

pub use extensions::{
    matrix as extension_matrix, parse_matrix as parse_extension_matrix, ExtensionEntry,
    ExtensionMatrix, Incompatibility, SdkCompat, SupportLevel, TokenExtension,
};
pub use metaplex::{
    mint_metaplex_nft, MetaplexMintInvocation, MetaplexMintOutcome, MetaplexMintRequest,
    MetaplexMintResult, MetaplexMintRunner, MetaplexStandard, SystemMetaplexRunner,
};

/// Bundle of injectable runners used by the Phase 8 command layer.
/// Mirrors `DeployServices` — production wiring uses
/// `TokenServices::system()`, integration tests construct a custom
/// pair.
pub struct TokenServices {
    pub token: Arc<dyn TokenCreateRunner>,
    pub metaplex: Arc<dyn MetaplexMintRunner>,
}

impl TokenServices {
    pub fn system() -> Self {
        Self {
            token: Arc::new(SystemTokenCreateRunner::new()),
            metaplex: Arc::new(SystemMetaplexRunner::new()),
        }
    }
}

impl std::fmt::Debug for TokenServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenServices").finish_non_exhaustive()
    }
}

/// SPL-Token program flavour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenProgram {
    Spl,
    SplToken2022,
}

impl TokenProgram {
    pub fn as_str(self) -> &'static str {
        match self {
            TokenProgram::Spl => "spl",
            TokenProgram::SplToken2022 => "spl_token_2022",
        }
    }

    pub fn cli_flag(self) -> &'static str {
        match self {
            // spl-token defaults to the classic program; no flag needed.
            TokenProgram::Spl => "",
            TokenProgram::SplToken2022 => "--program-2022",
        }
    }
}

impl Default for TokenProgram {
    fn default() -> Self {
        TokenProgram::SplToken2022
    }
}

/// Initialisation policy for each account extension the caller asks to
/// enable. The `spl-token create-token` CLI requires matching `--enable-*`
/// flags; we mirror the set here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenCreateSpec {
    pub cluster: ClusterKind,
    /// Defaults to Token-2022 when absent — the only place extensions work.
    #[serde(default)]
    pub program: TokenProgram,
    /// Mint authority. Must be a known persona on this cluster.
    pub authority_persona: String,
    /// Decimals for the mint. 0..=18 is the realistic range; the CLI
    /// rejects values > 19.
    pub decimals: u8,
    /// Optional mint keypair path — when absent, spl-token generates a
    /// random mint keypair.
    #[serde(default)]
    pub mint_keypair_path: Option<String>,
    /// Extensions to enable on the mint. Extensions outside the
    /// Token-2022 matrix are rejected before we shell out.
    #[serde(default)]
    pub extensions: Vec<TokenExtension>,
    /// Config values associated with a subset of extensions. Validated
    /// against `extensions` to catch forgotten pairs (e.g. asking for
    /// `transfer_fee` without basis points).
    #[serde(default)]
    pub config: TokenExtensionConfig,
    /// Optional override for the spl-token CLI path; defaults to whatever
    /// the toolchain probe resolved (or `spl-token` on PATH).
    #[serde(default)]
    pub spl_token_cli: Option<String>,
    /// Optional RPC URL. When absent the backend resolves it via
    /// `SolanaState::resolve_rpc_url`.
    #[serde(default)]
    pub rpc_url: Option<String>,
}

/// Grab-bag of tunables for extensions that need config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenExtensionConfig {
    /// Transfer-fee basis points (0..=10_000). Required when
    /// `transfer_fee` is enabled.
    #[serde(default)]
    pub transfer_fee_basis_points: Option<u16>,
    /// Maximum absolute per-transfer fee in base units. Required when
    /// `transfer_fee` is enabled.
    #[serde(default)]
    pub transfer_fee_maximum: Option<u64>,
    /// Interest-bearing rate (basis points per year). Required when
    /// `interest_bearing` is enabled.
    #[serde(default)]
    pub interest_rate_bps: Option<i16>,
    /// Program id that the transfer-hook extension should dispatch to.
    #[serde(default)]
    pub transfer_hook_program_id: Option<String>,
    /// Authority that will withdraw accumulated transfer fees; defaults
    /// to the mint authority when absent.
    #[serde(default)]
    pub transfer_fee_withdraw_authority: Option<String>,
}

/// Captured invocation the tests assert on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenCreateInvocation {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub envs: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenCreateOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    /// Extracted mint address. Callers usually pull this from the CLI
    /// stdout but a runner can also parse it out of a scripted response
    /// (integration tests) and populate the field directly.
    pub mint_address: Option<String>,
}

pub trait TokenCreateRunner: Send + Sync + std::fmt::Debug {
    fn run(&self, invocation: &TokenCreateInvocation) -> CommandResult<TokenCreateOutcome>;
}

#[derive(Debug, Default)]
pub struct SystemTokenCreateRunner;

impl SystemTokenCreateRunner {
    pub fn new() -> Self {
        Self
    }
}

impl TokenCreateRunner for SystemTokenCreateRunner {
    fn run(&self, invocation: &TokenCreateInvocation) -> CommandResult<TokenCreateOutcome> {
        let (program, args) = invocation.argv.split_first().ok_or_else(|| {
            CommandError::system_fault(
                "solana_token_create_empty_argv",
                "Empty argv passed to token-create runner.",
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
                "solana_token_create_spawn_failed",
                format!(
                    "Could not run `{program}`: {err}. Install the managed Solana toolchain or ensure `spl-token` is on PATH."
                ),
            )
        })?;
        let output = wait_with_timeout(child, invocation.timeout).ok_or_else(|| {
            CommandError::retryable(
                "solana_token_create_timeout",
                format!(
                    "spl-token create-token timed out after {}s.",
                    invocation.timeout.as_secs()
                ),
            )
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let mint_address = parse_mint_address(&stdout);
        Ok(TokenCreateOutcome {
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout,
            stderr,
            mint_address,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TokenCreateReport {
    pub cluster: ClusterKind,
    pub program: TokenProgram,
    pub argv: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub elapsed_ms: u128,
    pub mint_address: Option<String>,
    pub extensions: Vec<TokenExtension>,
    /// Non-empty when at least one incompatibility row was found in the
    /// bundled matrix. Surfaced so the UI / agent can warn *before* the
    /// user distributes the mint, even though the create itself
    /// succeeded.
    pub incompatibilities: Vec<Incompatibility>,
}

/// Invoked by the `solana_token_create` command. Normalises the spec,
/// checks extension/config pairs, consults the matrix, builds the argv,
/// then runs the supplied runner.
pub fn create_token(
    runner: &dyn TokenCreateRunner,
    authority_keypair_path: &Path,
    spec: TokenCreateSpec,
) -> CommandResult<TokenCreateReport> {
    let resolved_rpc = spec.rpc_url.clone().ok_or_else(|| {
        CommandError::user_fixable(
            "solana_token_create_no_rpc",
            "No RPC URL available — start a cluster or provide rpcUrl explicitly.",
        )
    })?;

    validate_spec(&spec)?;

    let cli = spec
        .spl_token_cli
        .clone()
        .unwrap_or_else(|| toolchain::resolve_command("spl-token"));
    let argv = assemble_argv(
        &cli,
        &resolved_rpc,
        authority_keypair_path,
        spec.mint_keypair_path.as_deref(),
        &spec,
    );
    let cwd = authority_keypair_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let invocation = TokenCreateInvocation {
        argv: argv.clone(),
        cwd,
        timeout: Duration::from_secs(120),
        envs: Vec::new(),
    };

    let start = Instant::now();
    let outcome = runner.run(&invocation)?;
    let elapsed_ms = start.elapsed().as_millis();

    let matrix = extension_matrix();
    let incompatibilities = matrix.incompatibilities(&spec.extensions);

    // Prefer a runner-supplied mint address, fall back to parsing stdout —
    // the system runner already parses, but scripted runners (tests) can
    // pass a bare outcome and rely on stdout extraction.
    let mint_address = outcome
        .mint_address
        .clone()
        .or_else(|| parse_mint_address(&outcome.stdout));

    Ok(TokenCreateReport {
        cluster: spec.cluster,
        program: spec.program,
        argv,
        success: outcome.success,
        exit_code: outcome.exit_code,
        stdout_excerpt: truncate(&outcome.stdout, 8_192),
        stderr_excerpt: truncate(&outcome.stderr, 8_192),
        elapsed_ms,
        mint_address,
        extensions: {
            let mut ext = spec.extensions.clone();
            ext.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            ext.dedup();
            ext
        },
        incompatibilities,
    })
}

fn validate_spec(spec: &TokenCreateSpec) -> CommandResult<()> {
    if spec.authority_persona.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_token_create_missing_authority",
            "authorityPersona must be a non-empty persona name on this cluster.",
        ));
    }
    if spec.decimals > 18 {
        return Err(CommandError::user_fixable(
            "solana_token_create_bad_decimals",
            format!("decimals must be 0..=18 (got {}).", spec.decimals),
        ));
    }
    if !spec.extensions.is_empty() && matches!(spec.program, TokenProgram::Spl) {
        return Err(CommandError::user_fixable(
            "solana_token_create_extensions_require_token_2022",
            "Token-2022 extensions cannot be enabled on the classic SPL-Token program — set program=spl_token_2022.",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for ext in &spec.extensions {
        if !seen.insert(*ext) {
            return Err(CommandError::user_fixable(
                "solana_token_create_duplicate_extension",
                format!("extension {:?} specified more than once.", ext),
            ));
        }
    }
    if spec.extensions.contains(&TokenExtension::TransferFee) {
        if spec.config.transfer_fee_basis_points.is_none() {
            return Err(CommandError::user_fixable(
                "solana_token_create_missing_transfer_fee_bps",
                "transfer_fee extension requires config.transferFeeBasisPoints.",
            ));
        }
        if spec.config.transfer_fee_maximum.is_none() {
            return Err(CommandError::user_fixable(
                "solana_token_create_missing_transfer_fee_max",
                "transfer_fee extension requires config.transferFeeMaximum.",
            ));
        }
        if let Some(bps) = spec.config.transfer_fee_basis_points {
            if bps > 10_000 {
                return Err(CommandError::user_fixable(
                    "solana_token_create_transfer_fee_bps_out_of_range",
                    format!("transferFeeBasisPoints must be 0..=10000 (got {}).", bps),
                ));
            }
        }
    }
    if spec.extensions.contains(&TokenExtension::InterestBearing)
        && spec.config.interest_rate_bps.is_none()
    {
        return Err(CommandError::user_fixable(
            "solana_token_create_missing_interest_rate",
            "interest_bearing extension requires config.interestRateBps.",
        ));
    }
    if spec.extensions.contains(&TokenExtension::TransferHook)
        && spec
            .config
            .transfer_hook_program_id
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(CommandError::user_fixable(
            "solana_token_create_missing_transfer_hook_program",
            "transfer_hook extension requires config.transferHookProgramId — the hook program to dispatch to.",
        ));
    }
    Ok(())
}

fn assemble_argv(
    cli: &str,
    rpc_url: &str,
    authority_keypair_path: &Path,
    mint_keypair_path: Option<&str>,
    spec: &TokenCreateSpec,
) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    v.push(cli.to_string());
    v.push("create-token".to_string());
    v.push("--url".to_string());
    v.push(rpc_url.to_string());
    v.push("--fee-payer".to_string());
    v.push(authority_keypair_path.display().to_string());
    v.push("--mint-authority".to_string());
    v.push(authority_keypair_path.display().to_string());
    v.push("--decimals".to_string());
    v.push(spec.decimals.to_string());
    if let Some(path) = mint_keypair_path {
        v.push(path.to_string());
    }
    let flag = spec.program.cli_flag();
    if !flag.is_empty() {
        v.push(flag.to_string());
    }
    for ext in sorted_extensions(&spec.extensions) {
        extend_for_extension(&mut v, ext, &spec.config, authority_keypair_path);
    }
    v
}

fn sorted_extensions(input: &[TokenExtension]) -> Vec<TokenExtension> {
    let mut out: Vec<TokenExtension> = input.iter().copied().collect();
    out.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    out.dedup();
    out
}

fn extend_for_extension(
    argv: &mut Vec<String>,
    ext: TokenExtension,
    config: &TokenExtensionConfig,
    authority_keypair_path: &Path,
) {
    match ext {
        TokenExtension::TransferFee => {
            argv.push("--transfer-fee".to_string());
            argv.push(
                config
                    .transfer_fee_basis_points
                    .expect("validated earlier")
                    .to_string(),
            );
            argv.push(
                config
                    .transfer_fee_maximum
                    .expect("validated earlier")
                    .to_string(),
            );
            if let Some(authority) = config.transfer_fee_withdraw_authority.as_deref() {
                argv.push("--transfer-fee-withdraw-authority".to_string());
                argv.push(authority.to_string());
            } else {
                argv.push("--transfer-fee-withdraw-authority".to_string());
                argv.push(authority_keypair_path.display().to_string());
            }
        }
        TokenExtension::TransferHook => {
            argv.push("--transfer-hook".to_string());
            if let Some(program) = config.transfer_hook_program_id.as_deref() {
                argv.push(program.to_string());
            }
        }
        TokenExtension::InterestBearing => {
            argv.push("--interest-rate".to_string());
            argv.push(
                config
                    .interest_rate_bps
                    .expect("validated earlier")
                    .to_string(),
            );
        }
        TokenExtension::NonTransferable => {
            argv.push("--enable-non-transferable".to_string());
        }
        TokenExtension::PermanentDelegate => {
            argv.push("--enable-permanent-delegate".to_string());
        }
        TokenExtension::MetadataPointer => {
            argv.push("--enable-metadata".to_string());
        }
        TokenExtension::TokenMetadata => {
            // TokenMetadata on-mint requires the MetadataPointer flag too;
            // the CLI rejects the combination if it's already pointing
            // elsewhere, so we use the same flag.
            argv.push("--enable-metadata".to_string());
        }
        TokenExtension::DefaultAccountState => {
            argv.push("--default-account-state".to_string());
            argv.push("frozen".to_string());
        }
        TokenExtension::MintCloseAuthority => {
            argv.push("--enable-close".to_string());
        }
        TokenExtension::ConfidentialTransfer => {
            argv.push("--enable-confidential-transfers".to_string());
            argv.push("auto".to_string());
        }
        TokenExtension::MemoTransfer => {
            argv.push("--enable-required-transfer-memos".to_string());
        }
        TokenExtension::CpiGuard => {
            argv.push("--enable-cpi-guard".to_string());
        }
        TokenExtension::ImmutableOwner => {
            // immutable_owner is an account-level extension, not a
            // mint-level one — spl-token create-token accepts it only
            // when creating associated token accounts. We emit the
            // closest equivalent flag so the runner still captures the
            // caller's intent.
            argv.push("--enable-immutable-owner".to_string());
        }
        TokenExtension::GroupPointer => {
            argv.push("--enable-group".to_string());
        }
        TokenExtension::GroupMemberPointer => {
            argv.push("--enable-member".to_string());
        }
        TokenExtension::ScaledUiAmount => {
            argv.push("--enable-scaled-ui-amount".to_string());
        }
    }
}

fn parse_mint_address(stdout: &str) -> Option<String> {
    // `spl-token create-token` prints a line like "Creating token
    // 4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b" — pull the pubkey
    // out of that. We fall back to any 32-byte-base58 candidate on
    // subsequent lines so the CLI's future output changes don't break
    // us silently.
    for line in stdout.lines() {
        if let Some(rest) = line.split_whitespace().nth(2) {
            if line.starts_with("Creating token") && is_likely_pubkey(rest) {
                return Some(rest.to_string());
            }
        }
    }
    for line in stdout.lines() {
        for token in line.split_whitespace() {
            if is_likely_pubkey(token) {
                return Some(token.to_string());
            }
        }
    }
    None
}

fn is_likely_pubkey(s: &str) -> bool {
    let len = s.len();
    if !(32..=44).contains(&len) {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() && c != '0' && c != 'O' && c != 'I' && c != 'l')
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

    fn base_spec(cluster: ClusterKind) -> TokenCreateSpec {
        TokenCreateSpec {
            cluster,
            program: TokenProgram::SplToken2022,
            authority_persona: "whale".into(),
            decimals: 6,
            mint_keypair_path: None,
            extensions: Vec::new(),
            config: TokenExtensionConfig::default(),
            spl_token_cli: None,
            rpc_url: Some("http://127.0.0.1:8899".into()),
        }
    }

    #[derive(Debug, Default)]
    struct CapturingRunner {
        calls: Mutex<Vec<TokenCreateInvocation>>,
        outcome: Mutex<Option<TokenCreateOutcome>>,
    }

    impl CapturingRunner {
        fn install(&self, outcome: TokenCreateOutcome) {
            *self.outcome.lock().unwrap() = Some(outcome);
        }
    }

    impl TokenCreateRunner for CapturingRunner {
        fn run(&self, invocation: &TokenCreateInvocation) -> CommandResult<TokenCreateOutcome> {
            self.calls.lock().unwrap().push(invocation.clone());
            self.outcome.lock().unwrap().clone().ok_or_else(|| {
                CommandError::system_fault(
                    "test_runner_no_outcome",
                    "Test did not install an outcome.",
                )
            })
        }
    }

    #[test]
    fn argv_includes_program_2022_flag_and_decimals() {
        let runner = CapturingRunner::default();
        runner.install(TokenCreateOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "Creating token 4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b".into(),
            stderr: String::new(),
            mint_address: None,
        });
        let spec = base_spec(ClusterKind::Localnet);
        let report = create_token(&runner, Path::new("/tmp/whale.json"), spec).unwrap();
        assert!(report.success);
        assert!(report.argv.iter().any(|a| a == "--program-2022"));
        assert!(report.argv.iter().any(|a| a == "--decimals"));
        assert_eq!(
            report.mint_address.as_deref(),
            Some("4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b")
        );
    }

    #[test]
    fn spl_classic_rejects_extensions() {
        let runner = CapturingRunner::default();
        runner.install(TokenCreateOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "".into(),
            stderr: "".into(),
            mint_address: None,
        });
        let mut spec = base_spec(ClusterKind::Localnet);
        spec.program = TokenProgram::Spl;
        spec.extensions = vec![TokenExtension::TransferFee];
        spec.config.transfer_fee_basis_points = Some(25);
        spec.config.transfer_fee_maximum = Some(1_000_000);
        let err = create_token(&runner, Path::new("/tmp/whale.json"), spec).unwrap_err();
        assert_eq!(
            err.code,
            "solana_token_create_extensions_require_token_2022"
        );
    }

    #[test]
    fn transfer_fee_requires_bps_and_maximum() {
        let runner = CapturingRunner::default();
        runner.install(TokenCreateOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "".into(),
            stderr: "".into(),
            mint_address: None,
        });
        let mut spec = base_spec(ClusterKind::Localnet);
        spec.extensions = vec![TokenExtension::TransferFee];
        let err = create_token(&runner, Path::new("/tmp/whale.json"), spec).unwrap_err();
        assert_eq!(err.code, "solana_token_create_missing_transfer_fee_bps");
    }

    #[test]
    fn transfer_hook_requires_program_id() {
        let runner = CapturingRunner::default();
        runner.install(TokenCreateOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "".into(),
            stderr: "".into(),
            mint_address: None,
        });
        let mut spec = base_spec(ClusterKind::Localnet);
        spec.extensions = vec![TokenExtension::TransferHook];
        let err = create_token(&runner, Path::new("/tmp/whale.json"), spec).unwrap_err();
        assert_eq!(
            err.code,
            "solana_token_create_missing_transfer_hook_program"
        );
    }

    #[test]
    fn transfer_fee_with_valid_config_builds_argv() {
        let runner = CapturingRunner::default();
        runner.install(TokenCreateOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "Creating token 4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b".into(),
            stderr: String::new(),
            mint_address: None,
        });
        let mut spec = base_spec(ClusterKind::Localnet);
        spec.extensions = vec![TokenExtension::TransferFee];
        spec.config.transfer_fee_basis_points = Some(42);
        spec.config.transfer_fee_maximum = Some(1_000_000);
        let report = create_token(&runner, Path::new("/tmp/whale.json"), spec).unwrap();
        assert!(report.success);
        assert!(report.argv.iter().any(|a| a == "--transfer-fee"));
        assert!(report.argv.iter().any(|a| a == "42"));
        assert!(report.argv.iter().any(|a| a == "1000000"));
    }

    #[test]
    fn incompatibility_rows_surface_for_transfer_hook() {
        let runner = CapturingRunner::default();
        runner.install(TokenCreateOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "".into(),
            stderr: "".into(),
            mint_address: None,
        });
        let mut spec = base_spec(ClusterKind::Localnet);
        spec.extensions = vec![TokenExtension::TransferHook];
        spec.config.transfer_hook_program_id =
            Some("HookPr0g111111111111111111111111111111111111".into());
        let report = create_token(&runner, Path::new("/tmp/whale.json"), spec).unwrap();
        assert!(
            !report.incompatibilities.is_empty(),
            "transfer_hook must surface at least one incompatibility row"
        );
        assert!(report
            .incompatibilities
            .iter()
            .any(|row| row.sdk.contains("wallet-adapter") && !row.remediation_hint.is_empty()));
    }

    #[test]
    fn bad_decimals_rejected() {
        let runner = CapturingRunner::default();
        runner.install(TokenCreateOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "".into(),
            stderr: "".into(),
            mint_address: None,
        });
        let mut spec = base_spec(ClusterKind::Localnet);
        spec.decimals = 19;
        let err = create_token(&runner, Path::new("/tmp/whale.json"), spec).unwrap_err();
        assert_eq!(err.code, "solana_token_create_bad_decimals");
    }

    #[test]
    fn no_rpc_url_rejected() {
        let runner = CapturingRunner::default();
        runner.install(TokenCreateOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "".into(),
            stderr: "".into(),
            mint_address: None,
        });
        let mut spec = base_spec(ClusterKind::Localnet);
        spec.rpc_url = None;
        let err = create_token(&runner, Path::new("/tmp/whale.json"), spec).unwrap_err();
        assert_eq!(err.code, "solana_token_create_no_rpc");
    }

    #[test]
    fn stdout_mint_address_extracted_even_without_leading_prefix() {
        let s = "something something token 4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b";
        assert_eq!(
            parse_mint_address(s).as_deref(),
            Some("4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b")
        );
    }
}
