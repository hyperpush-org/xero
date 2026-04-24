//! `anchor idl init` / `anchor idl upgrade` orchestration.
//!
//! The Anchor CLI is the canonical publisher for on-chain IDLs; we
//! don't re-implement the IDL account layout writer. Our job is to:
//!
//! 1. Choose the right sub-command based on whether the IDL already
//!    exists on-chain (init the first time, upgrade subsequently).
//! 2. Pipe the args through with the right provider URL + keypair path
//!    so the Anchor CLI doesn't pick up an unexpected Anchor.toml
//!    provider.
//! 3. Capture stdout/stderr with progress events so the UI's deploy
//!    panel can stream phase-by-phase.
//!
//! The runner is behind a trait so unit tests exercise the argv
//! construction + output capture without touching the real CLI.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::toolchain;
use crate::commands::{CommandError, CommandResult};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdlPublishMode {
    Init,
    Upgrade,
}

impl IdlPublishMode {
    pub fn anchor_subcommand(self) -> &'static str {
        match self {
            IdlPublishMode::Init => "init",
            IdlPublishMode::Upgrade => "upgrade",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IdlPublishRequest {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub idl_path: String,
    pub authority_keypair_path: String,
    pub rpc_url: String,
    pub mode: IdlPublishMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IdlPublishReport {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub mode: IdlPublishMode,
    pub success: bool,
    pub signature: Option<String>,
    pub idl_address: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub elapsed_ms: u128,
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployProgressPhase {
    Planning,
    Uploading,
    Finalising,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeployProgressPayload {
    pub program_id: String,
    pub cluster: String,
    pub phase: DeployProgressPhase,
    pub detail: String,
    pub ts_ms: u64,
}

pub trait DeployProgressSink: Send + Sync + std::fmt::Debug {
    fn emit(&self, payload: DeployProgressPayload);
}

#[derive(Debug, Default, Clone)]
pub struct NullProgressSink;

impl DeployProgressSink for NullProgressSink {
    fn emit(&self, _payload: DeployProgressPayload) {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorIdlInvocation {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorIdlOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait AnchorIdlRunner: Send + Sync + std::fmt::Debug {
    fn run(&self, invocation: &AnchorIdlInvocation) -> CommandResult<AnchorIdlOutcome>;
}

#[derive(Debug, Default)]
pub struct SystemAnchorIdlRunner;

impl SystemAnchorIdlRunner {
    pub fn new() -> Self {
        Self
    }
}

impl AnchorIdlRunner for SystemAnchorIdlRunner {
    fn run(&self, invocation: &AnchorIdlInvocation) -> CommandResult<AnchorIdlOutcome> {
        let (program, args) = invocation.argv.split_first().ok_or_else(|| {
            CommandError::system_fault(
                "solana_idl_publish_empty_argv",
                "Empty argv passed to Anchor IDL runner.",
            )
        })?;
        let resolved_program = toolchain::resolve_command(program);
        let mut cmd = Command::new(&resolved_program);
        cmd.args(args);
        if let Some(cwd) = &invocation.cwd {
            cmd.current_dir(cwd);
        }
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        toolchain::augment_command(&mut cmd);
        let child = cmd.spawn().map_err(|err| {
            CommandError::user_fixable(
                "solana_idl_publish_spawn_failed",
                format!(
                    "Could not run `{program}`: {err}. Install the managed Solana toolchain or ensure Anchor is on PATH."
                ),
            )
        })?;
        let output = wait_with_timeout(child, invocation.timeout).ok_or_else(|| {
            CommandError::retryable(
                "solana_idl_publish_timeout",
                format!(
                    "Anchor IDL command timed out after {}s.",
                    invocation.timeout.as_secs()
                ),
            )
        })?;
        Ok(AnchorIdlOutcome {
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

pub fn publish(
    runner: &dyn AnchorIdlRunner,
    sink: &dyn DeployProgressSink,
    request: &IdlPublishRequest,
) -> CommandResult<IdlPublishReport> {
    validate_request(request)?;

    let invocation = build_invocation(request);
    let start = Instant::now();
    sink.emit(DeployProgressPayload {
        program_id: request.program_id.clone(),
        cluster: request.cluster.as_str().to_string(),
        phase: DeployProgressPhase::Planning,
        detail: format!("Preparing anchor idl {}", request.mode.anchor_subcommand()),
        ts_ms: now_ms(),
    });
    sink.emit(DeployProgressPayload {
        program_id: request.program_id.clone(),
        cluster: request.cluster.as_str().to_string(),
        phase: DeployProgressPhase::Uploading,
        detail: format!("Running: {}", invocation.argv.join(" ")),
        ts_ms: now_ms(),
    });

    let outcome = runner.run(&invocation)?;
    let elapsed_ms = start.elapsed().as_millis();
    let signature =
        extract_signature(&outcome.stdout).or_else(|| extract_signature(&outcome.stderr));
    let idl_address =
        extract_idl_address(&outcome.stdout).or_else(|| extract_idl_address(&outcome.stderr));

    let phase = if outcome.success {
        DeployProgressPhase::Completed
    } else {
        DeployProgressPhase::Failed
    };
    sink.emit(DeployProgressPayload {
        program_id: request.program_id.clone(),
        cluster: request.cluster.as_str().to_string(),
        phase,
        detail: if outcome.success {
            format!(
                "Published IDL in {}ms{}",
                elapsed_ms,
                signature
                    .as_deref()
                    .map(|sig| format!(" (signature {sig})"))
                    .unwrap_or_default()
            )
        } else {
            format!(
                "Anchor exited with code {:?}: {}",
                outcome.exit_code,
                truncate(&outcome.stderr, 500)
            )
        },
        ts_ms: now_ms(),
    });

    Ok(IdlPublishReport {
        program_id: request.program_id.clone(),
        cluster: request.cluster,
        mode: request.mode,
        success: outcome.success,
        signature,
        idl_address,
        exit_code: outcome.exit_code,
        stdout_excerpt: truncate(&outcome.stdout, 4_096),
        stderr_excerpt: truncate(&outcome.stderr, 4_096),
        elapsed_ms,
        argv: invocation.argv,
    })
}

fn validate_request(request: &IdlPublishRequest) -> CommandResult<()> {
    if request.program_id.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_idl_publish_missing_program_id",
            "program_id is required to publish an IDL.",
        ));
    }
    if request.rpc_url.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_idl_publish_missing_rpc_url",
            "rpc_url is required.",
        ));
    }
    let idl = Path::new(&request.idl_path);
    if !idl.is_file() {
        return Err(CommandError::user_fixable(
            "solana_idl_publish_missing_idl",
            format!("IDL file {} does not exist.", idl.display()),
        ));
    }
    let keypair = Path::new(&request.authority_keypair_path);
    if !keypair.is_file() {
        return Err(CommandError::user_fixable(
            "solana_idl_publish_missing_keypair",
            format!("Authority keypair {} does not exist.", keypair.display()),
        ));
    }
    if matches!(request.cluster, ClusterKind::Mainnet) {
        return Err(CommandError::policy_denied(
            "Direct anchor idl publishes against mainnet are disabled — generate a Squads proposal instead.",
        ));
    }
    Ok(())
}

fn build_invocation(request: &IdlPublishRequest) -> AnchorIdlInvocation {
    // Anchor CLI shape (0.30+):
    //   anchor idl {init,upgrade} <program_id>
    //     --filepath <path>
    //     --provider.cluster <url>
    //     --provider.wallet <keypair.json>
    let mut argv: Vec<String> = vec![
        "anchor".into(),
        "idl".into(),
        request.mode.anchor_subcommand().into(),
        request.program_id.clone(),
        "--filepath".into(),
        absolute_string(&request.idl_path),
        "--provider.cluster".into(),
        request.rpc_url.clone(),
        "--provider.wallet".into(),
        absolute_string(&request.authority_keypair_path),
    ];
    // Defensive: anchor accepts `--yes` for non-interactive confirmation.
    argv.push("--yes".into());
    AnchorIdlInvocation {
        argv,
        cwd: None,
        timeout: DEFAULT_TIMEOUT,
    }
}

fn absolute_string(path: &str) -> String {
    let pb = PathBuf::from(path);
    fs::canonicalize(&pb).unwrap_or(pb).display().to_string()
}

fn extract_signature(text: &str) -> Option<String> {
    // Anchor prints lines like:
    //   "Idl account created: <address>"
    //   "Signature: <base58 sig>"
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Signature:") {
            let sig = rest.trim();
            if !sig.is_empty() {
                return Some(sig.to_string());
            }
        }
    }
    // Fallback: spot a 64+ char base58-like token near a known keyword.
    for line in text.lines() {
        if line.contains("signature") || line.contains("Signature") {
            for token in line.split_whitespace() {
                if token.len() >= 64 && is_base58(token) {
                    return Some(token.trim_end_matches(&[',', '.', ';'][..]).to_string());
                }
            }
        }
    }
    None
}

fn extract_idl_address(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        for prefix in [
            "Idl account created:",
            "Idl account:",
            "Idl address:",
            "Idl account updated:",
        ] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let addr = rest.trim();
                if !addr.is_empty() {
                    return Some(addr.split_whitespace().next().unwrap_or(addr).to_string());
                }
            }
        }
    }
    None
}

fn is_base58(s: &str) -> bool {
    s.chars().all(|c| match c {
        '0' | 'O' | 'I' | 'l' => false,
        '1'..='9' | 'A'..='N' | 'P'..='Z' | 'a'..='k' | 'm'..='z' => true,
        _ => false,
    })
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
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => return None,
        }
    }
    child.wait_with_output().ok()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut out = s.chars().take(max).collect::<String>();
        out.push_str("… (truncated)");
        out
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    pub struct MockAnchorIdlRunner {
        pub calls: Mutex<Vec<AnchorIdlInvocation>>,
        pub outcome: Mutex<Option<AnchorIdlOutcome>>,
    }

    impl MockAnchorIdlRunner {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn set_outcome(&self, outcome: AnchorIdlOutcome) {
            *self.outcome.lock().unwrap() = Some(outcome);
        }
    }

    impl AnchorIdlRunner for MockAnchorIdlRunner {
        fn run(&self, invocation: &AnchorIdlInvocation) -> CommandResult<AnchorIdlOutcome> {
            self.calls.lock().unwrap().push(invocation.clone());
            Ok(self
                .outcome
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(AnchorIdlOutcome {
                    exit_code: Some(0),
                    success: true,
                    stdout: "Signature: 5abCEsQUFbmnoRsmB8NGbkmSpJWCGt9cZi1dE6HmxY8rB1p7H1MhCV4pHFg6bCSFhXnBQrhbqyvDnG9sGUMuJDRj\n".into(),
                    stderr: String::new(),
                }))
        }
    }

    #[derive(Debug, Default, Clone)]
    pub struct CollectingProgressSink(pub std::sync::Arc<Mutex<Vec<DeployProgressPayload>>>);

    impl DeployProgressSink for CollectingProgressSink {
        fn emit(&self, payload: DeployProgressPayload) {
            self.0.lock().unwrap().push(payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{CollectingProgressSink, MockAnchorIdlRunner};
    use super::*;
    use tempfile::TempDir;

    fn make_request(tmp: &TempDir, mode: IdlPublishMode) -> IdlPublishRequest {
        let idl = tmp.path().join("idl.json");
        fs::write(&idl, b"{}").unwrap();
        let kp = tmp.path().join("auth.json");
        fs::write(&kp, b"[]").unwrap();
        IdlPublishRequest {
            program_id: "Prog11111111111111111111111111111111111111".into(),
            cluster: ClusterKind::Devnet,
            idl_path: idl.display().to_string(),
            authority_keypair_path: kp.display().to_string(),
            rpc_url: "https://api.devnet.solana.com".into(),
            mode,
        }
    }

    #[test]
    fn publish_emits_planning_uploading_completed_events_on_success() {
        let tmp = TempDir::new().unwrap();
        let runner = MockAnchorIdlRunner::new();
        let sink = CollectingProgressSink::default();
        let report = publish(&runner, &sink, &make_request(&tmp, IdlPublishMode::Init)).unwrap();
        assert!(report.success);
        assert!(report.signature.is_some());
        let events = sink.0.lock().unwrap().clone();
        let phases: Vec<_> = events.iter().map(|e| e.phase).collect();
        assert!(phases.contains(&DeployProgressPhase::Planning));
        assert!(phases.contains(&DeployProgressPhase::Uploading));
        assert!(phases.contains(&DeployProgressPhase::Completed));
    }

    #[test]
    fn publish_upgrade_uses_upgrade_subcommand() {
        let tmp = TempDir::new().unwrap();
        let runner = MockAnchorIdlRunner::new();
        let sink = CollectingProgressSink::default();
        let _ = publish(&runner, &sink, &make_request(&tmp, IdlPublishMode::Upgrade)).unwrap();
        let calls = runner.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].argv.contains(&"upgrade".to_string()));
        assert!(calls[0].argv.contains(&"--yes".to_string()));
    }

    #[test]
    fn publish_reports_failure_when_anchor_exits_nonzero() {
        let tmp = TempDir::new().unwrap();
        let runner = MockAnchorIdlRunner::new();
        runner.set_outcome(AnchorIdlOutcome {
            exit_code: Some(1),
            success: false,
            stdout: String::new(),
            stderr: "Error: account does not exist".into(),
        });
        let sink = CollectingProgressSink::default();
        let report = publish(&runner, &sink, &make_request(&tmp, IdlPublishMode::Upgrade)).unwrap();
        assert!(!report.success);
        assert_eq!(report.exit_code, Some(1));
        let events = sink.0.lock().unwrap().clone();
        assert!(events
            .iter()
            .any(|e| e.phase == DeployProgressPhase::Failed));
    }

    #[test]
    fn publish_rejects_mainnet_cluster() {
        let tmp = TempDir::new().unwrap();
        let runner = MockAnchorIdlRunner::new();
        let sink = CollectingProgressSink::default();
        let mut req = make_request(&tmp, IdlPublishMode::Init);
        req.cluster = ClusterKind::Mainnet;
        let err = publish(&runner, &sink, &req).unwrap_err();
        assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
    }

    #[test]
    fn publish_rejects_missing_idl_file() {
        let tmp = TempDir::new().unwrap();
        let runner = MockAnchorIdlRunner::new();
        let sink = CollectingProgressSink::default();
        let mut req = make_request(&tmp, IdlPublishMode::Init);
        req.idl_path = tmp.path().join("nope.json").display().to_string();
        let err = publish(&runner, &sink, &req).unwrap_err();
        assert_eq!(err.code, "solana_idl_publish_missing_idl");
    }

    #[test]
    fn extract_signature_handles_signature_line() {
        let text = "Deploying IDL...\nSignature: 5abCEsQUFbmnoRsmB8NGbkmSpJWCGt9cZi1dE6HmxY8rB1p7H1MhCV4pHFg6bCSFhXnBQrhbqyvDnG9sGUMuJDRj\nDone.";
        let sig = extract_signature(text).unwrap();
        assert!(sig.starts_with("5abCEs"));
    }

    #[test]
    fn extract_idl_address_handles_created_line() {
        let text = "Idl account created: IdLAddressBase58ValueHere123456789\n";
        let addr = extract_idl_address(text).unwrap();
        assert_eq!(addr, "IdLAddressBase58ValueHere123456789");
    }
}
