//! `solana-verify` wrapper for verified-build submission.
//!
//! Drives the `solana-verify` CLI in `verify-from-repo` mode (which
//! both rebuilds the program inside the same Docker image used by the
//! verified-builds infrastructure AND posts the result to the
//! Verifier registry). Captures stdout/stderr, the program-hash if
//! present, and a structured result the deploy panel can render.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::{CommandError, CommandResult};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1_800); // 30 min — verified builds are slow.
const CAPTURE_BYTES: usize = 16_384;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedBuildRequest {
    pub program_id: String,
    pub cluster: ClusterKind,
    /// Path to the project root (the directory containing `Anchor.toml`
    /// or the program's `Cargo.toml`).
    pub manifest_path: String,
    /// Public GitHub URL hosting the source. `solana-verify` won't
    /// accept private or local-only repos because the registry needs
    /// a publicly reproducible reference.
    pub github_url: String,
    /// Git commit hash to pin. Strongly recommended — without a
    /// commit the registry pins to whatever HEAD points at when the
    /// build runs, which is non-reproducible.
    #[serde(default)]
    pub commit_hash: Option<String>,
    /// Library / program name inside the repo (`-p <name>` for
    /// workspace projects).
    #[serde(default)]
    pub library_name: Option<String>,
    /// Skip the post-build registry submit step. The verification
    /// still runs locally; useful for dry-runs.
    #[serde(default)]
    pub skip_remote_submit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedBuildResult {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub argv: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub program_hash: Option<String>,
    pub registry_url: Option<String>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBuildInvocation {
    pub argv: Vec<String>,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBuildOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait VerifiedBuildRunner: Send + Sync + std::fmt::Debug {
    fn run(&self, invocation: &VerifiedBuildInvocation) -> CommandResult<VerifiedBuildOutcome>;
}

#[derive(Debug, Default)]
pub struct SystemVerifiedBuildRunner;

impl SystemVerifiedBuildRunner {
    pub fn new() -> Self {
        Self
    }
}

impl VerifiedBuildRunner for SystemVerifiedBuildRunner {
    fn run(&self, invocation: &VerifiedBuildInvocation) -> CommandResult<VerifiedBuildOutcome> {
        let (program, args) = invocation.argv.split_first().ok_or_else(|| {
            CommandError::system_fault(
                "solana_verified_build_empty_argv",
                "Empty argv passed to solana-verify runner.",
            )
        })?;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        let child = cmd.spawn().map_err(|err| {
            CommandError::user_fixable(
                "solana_verified_build_spawn_failed",
                format!(
                    "Could not run `{program}`: {err}. Install solana-verify with `cargo install solana-verify`.",
                ),
            )
        })?;
        let output = wait_with_timeout(child, invocation.timeout).ok_or_else(|| {
            CommandError::retryable(
                "solana_verified_build_timeout",
                format!(
                    "solana-verify did not finish in {}s — verified builds can be slow under load; try again or check Docker availability.",
                    invocation.timeout.as_secs()
                ),
            )
        })?;
        Ok(VerifiedBuildOutcome {
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

pub fn submit(
    runner: &dyn VerifiedBuildRunner,
    request: &VerifiedBuildRequest,
) -> CommandResult<VerifiedBuildResult> {
    validate_request(request)?;

    let argv = build_argv(request);
    let start = Instant::now();
    let outcome = runner.run(&VerifiedBuildInvocation {
        argv: argv.clone(),
        timeout: DEFAULT_TIMEOUT,
    })?;
    let elapsed_ms = start.elapsed().as_millis();
    let program_hash =
        extract_program_hash(&outcome.stdout).or_else(|| extract_program_hash(&outcome.stderr));
    let registry_url = if request.skip_remote_submit {
        None
    } else {
        Some(default_registry_url(&request.program_id))
    };
    Ok(VerifiedBuildResult {
        program_id: request.program_id.clone(),
        cluster: request.cluster,
        argv,
        success: outcome.success,
        exit_code: outcome.exit_code,
        program_hash,
        registry_url,
        stdout_excerpt: truncate(&outcome.stdout, CAPTURE_BYTES),
        stderr_excerpt: truncate(&outcome.stderr, CAPTURE_BYTES),
        elapsed_ms,
    })
}

fn validate_request(request: &VerifiedBuildRequest) -> CommandResult<()> {
    if request.program_id.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_verified_build_missing_program_id",
            "program_id is required.",
        ));
    }
    if request.github_url.trim().is_empty()
        || !(request.github_url.starts_with("https://github.com/")
            || request.github_url.starts_with("https://gitlab.com/"))
    {
        return Err(CommandError::user_fixable(
            "solana_verified_build_bad_github_url",
            "github_url must be a public GitHub or GitLab https URL — verified builds need a public source.",
        ));
    }
    if !Path::new(&request.manifest_path).exists() {
        return Err(CommandError::user_fixable(
            "solana_verified_build_missing_manifest",
            format!("manifest_path {} does not exist.", request.manifest_path),
        ));
    }
    Ok(())
}

fn build_argv(request: &VerifiedBuildRequest) -> Vec<String> {
    let mut argv: Vec<String> = vec![
        "solana-verify".into(),
        "verify-from-repo".into(),
        request.github_url.clone(),
        "--program-id".into(),
        request.program_id.clone(),
        "--url".into(),
        rpc_url_for(request.cluster).to_string(),
    ];
    if let Some(commit) = request.commit_hash.as_deref() {
        argv.push("--commit-hash".into());
        argv.push(commit.to_string());
    }
    if let Some(name) = request.library_name.as_deref() {
        argv.push("--library-name".into());
        argv.push(name.to_string());
    }
    if request.skip_remote_submit {
        argv.push("--skip-prompt".into());
    } else {
        argv.push("--remote".into());
        argv.push("--skip-prompt".into());
    }
    argv
}

fn rpc_url_for(cluster: ClusterKind) -> &'static str {
    match cluster {
        ClusterKind::Devnet => "https://api.devnet.solana.com",
        ClusterKind::Mainnet => "https://api.mainnet-beta.solana.com",
        ClusterKind::Localnet | ClusterKind::MainnetFork => "http://127.0.0.1:8899",
    }
}

fn default_registry_url(program_id: &str) -> String {
    // Verified-builds public registry. Each entry is keyed by program id.
    format!("https://verify.osec.io/program/{program_id}")
}

fn extract_program_hash(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        for prefix in ["Program hash:", "On-chain hash:", "Build hash:"] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let h = rest.trim();
                if !h.is_empty() {
                    return Some(h.split_whitespace().next().unwrap_or(h).to_string());
                }
            }
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
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
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

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    pub struct MockVerifiedBuildRunner {
        pub calls: Mutex<Vec<VerifiedBuildInvocation>>,
        pub outcome: Mutex<Option<VerifiedBuildOutcome>>,
    }

    impl MockVerifiedBuildRunner {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn set_outcome(&self, outcome: VerifiedBuildOutcome) {
            *self.outcome.lock().unwrap() = Some(outcome);
        }
    }

    impl VerifiedBuildRunner for MockVerifiedBuildRunner {
        fn run(&self, invocation: &VerifiedBuildInvocation) -> CommandResult<VerifiedBuildOutcome> {
            self.calls.lock().unwrap().push(invocation.clone());
            Ok(self.outcome.lock().unwrap().clone().unwrap_or(VerifiedBuildOutcome {
                exit_code: Some(0),
                success: true,
                stdout: "Program hash: deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n".into(),
                stderr: String::new(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockVerifiedBuildRunner;
    use super::*;
    use tempfile::TempDir;

    fn make_request(tmp: &TempDir) -> VerifiedBuildRequest {
        let manifest = tmp.path().join("Anchor.toml");
        std::fs::write(&manifest, b"[programs.localnet]\n").unwrap();
        VerifiedBuildRequest {
            program_id: "Prog1111111111111111111111111111111111111".into(),
            cluster: ClusterKind::Devnet,
            manifest_path: manifest.display().to_string(),
            github_url: "https://github.com/example/foo".into(),
            commit_hash: Some("abc1234".into()),
            library_name: Some("foo".into()),
            skip_remote_submit: false,
        }
    }

    #[test]
    fn submit_returns_program_hash_and_registry_url_on_success() {
        let tmp = TempDir::new().unwrap();
        let runner = MockVerifiedBuildRunner::new();
        let report = submit(&runner, &make_request(&tmp)).unwrap();
        assert!(report.success);
        assert!(report.program_hash.is_some());
        assert!(report.registry_url.unwrap().contains("verify.osec.io"));
        let calls = runner.calls.lock().unwrap();
        assert!(calls[0].argv.contains(&"--remote".to_string()));
    }

    #[test]
    fn submit_skip_remote_omits_registry_url() {
        let tmp = TempDir::new().unwrap();
        let runner = MockVerifiedBuildRunner::new();
        let mut req = make_request(&tmp);
        req.skip_remote_submit = true;
        let report = submit(&runner, &req).unwrap();
        assert!(report.registry_url.is_none());
        let calls = runner.calls.lock().unwrap();
        assert!(!calls[0].argv.contains(&"--remote".to_string()));
        assert!(calls[0].argv.contains(&"--skip-prompt".to_string()));
    }

    #[test]
    fn submit_rejects_non_https_github_url() {
        let tmp = TempDir::new().unwrap();
        let runner = MockVerifiedBuildRunner::new();
        let mut req = make_request(&tmp);
        req.github_url = "git@github.com:example/foo.git".into();
        let err = submit(&runner, &req).unwrap_err();
        assert_eq!(err.code, "solana_verified_build_bad_github_url");
    }

    #[test]
    fn submit_passes_commit_hash_through_argv() {
        let tmp = TempDir::new().unwrap();
        let runner = MockVerifiedBuildRunner::new();
        let report = submit(&runner, &make_request(&tmp)).unwrap();
        assert!(report.argv.contains(&"--commit-hash".to_string()));
        assert!(report.argv.contains(&"abc1234".to_string()));
    }

    #[test]
    fn submit_surfaces_failure_and_keeps_excerpt() {
        let tmp = TempDir::new().unwrap();
        let runner = MockVerifiedBuildRunner::new();
        runner.set_outcome(VerifiedBuildOutcome {
            exit_code: Some(2),
            success: false,
            stdout: String::new(),
            stderr: "Error: docker unavailable".into(),
        });
        let report = submit(&runner, &make_request(&tmp)).unwrap();
        assert!(!report.success);
        assert_eq!(report.exit_code, Some(2));
        assert!(report.stderr_excerpt.contains("docker unavailable"));
    }
}
