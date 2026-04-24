//! Codama CLI wrapper.
//!
//! Codama reads an Anchor-format IDL and emits language-specific clients
//! (TS, Rust, Umi). We drive it as a sub-process and route all writes
//! through a caller-specified output directory so an agent can generate
//! into `clients/ts/` and `clients/rust/` by convention.
//!
//! The wrapper is split behind a `CodamaRunner` trait so the unit tests
//! exercise the orchestration (target selection, output path
//! composition, diagnostic capture) without needing the Codama binary on
//! PATH. Production implementation shells out to `codama` or, when the
//! project has `codama` as a devDependency, to `pnpm codama`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::commands::solana::toolchain;
use crate::commands::{CommandError, CommandResult};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CodamaTarget {
    /// Modern TS client (`@codama/renderers-js`).
    Ts,
    /// Rust client (`@codama/renderers-rust`).
    Rust,
    /// Umi-flavored TS client (`@codama/renderers-js-umi`).
    Umi,
}

impl CodamaTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            CodamaTarget::Ts => "ts",
            CodamaTarget::Rust => "rust",
            CodamaTarget::Umi => "umi",
        }
    }

    pub fn renderer_flag(self) -> &'static str {
        match self {
            CodamaTarget::Ts => "js",
            CodamaTarget::Rust => "rust",
            CodamaTarget::Umi => "js-umi",
        }
    }

    pub fn default_subdir(self) -> &'static str {
        match self {
            CodamaTarget::Ts => "ts",
            CodamaTarget::Rust => "rust",
            CodamaTarget::Umi => "umi",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodamaGenerationRequest {
    pub idl_path: String,
    pub targets: Vec<CodamaTarget>,
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodamaGenerationReport {
    pub idl_path: String,
    pub output_dir: String,
    pub targets: Vec<CodamaTargetResult>,
    pub elapsed_ms: u128,
    pub all_succeeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodamaTargetResult {
    pub target: CodamaTarget,
    pub output_subdir: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub elapsed_ms: u128,
}

/// Minimal sub-process descriptor so tests can mock without shelling out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodamaInvocation {
    pub target: CodamaTarget,
    pub idl_path: PathBuf,
    pub output_dir: PathBuf,
}

pub trait CodamaRunner: Send + Sync + std::fmt::Debug {
    fn run(&self, invocation: &CodamaInvocation) -> CommandResult<CodamaTargetResult>;
}

#[derive(Debug, Default)]
pub struct SystemCodamaRunner {
    /// Override the binary path for unit tests; in production we resolve
    /// `codama` / `pnpm exec codama` at run time.
    pub binary_override: Option<PathBuf>,
}

impl SystemCodamaRunner {
    pub fn new() -> Self {
        Self::default()
    }

    fn command(&self, invocation: &CodamaInvocation) -> Command {
        let (program, base_args) = match &self.binary_override {
            Some(path) => (path.clone(), Vec::<String>::new()),
            None => (
                PathBuf::from(toolchain::resolve_command("codama")),
                Vec::new(),
            ),
        };
        let mut cmd = Command::new(program);
        for arg in base_args {
            cmd.arg(arg);
        }
        cmd.arg("run")
            .arg(invocation.target.renderer_flag())
            .arg(invocation.idl_path.as_os_str())
            .arg("--output")
            .arg(invocation.output_dir.as_os_str());
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        toolchain::augment_command(&mut cmd);
        cmd
    }
}

impl CodamaRunner for SystemCodamaRunner {
    fn run(&self, invocation: &CodamaInvocation) -> CommandResult<CodamaTargetResult> {
        let start = Instant::now();
        let mut cmd = self.command(invocation);
        let child = cmd.spawn().map_err(|err| {
            CommandError::user_fixable(
                "solana_codama_spawn_failed",
                format!(
                    "Could not run the Codama CLI: {err}. Install it in the managed toolchain, via npm / pnpm, or pin it as a devDependency."
                ),
            )
        })?;
        let output = wait_with_timeout(child, DEFAULT_TIMEOUT).ok_or_else(|| {
            CommandError::retryable(
                "solana_codama_timeout",
                format!(
                    "Codama did not finish in {}s while generating the `{}` client.",
                    DEFAULT_TIMEOUT.as_secs(),
                    invocation.target.as_str()
                ),
            )
        })?;
        let elapsed_ms = start.elapsed().as_millis();
        let exit_code = output.status.code();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Ok(CodamaTargetResult {
            target: invocation.target,
            output_subdir: invocation.output_dir.display().to_string(),
            success: output.status.success(),
            exit_code,
            stdout_excerpt: truncate(&stdout, 4_096),
            stderr_excerpt: truncate(&stderr, 4_096),
            elapsed_ms,
        })
    }
}

pub fn generate(
    runner: &dyn CodamaRunner,
    request: &CodamaGenerationRequest,
) -> CommandResult<CodamaGenerationReport> {
    let start = Instant::now();
    if request.targets.is_empty() {
        return Err(CommandError::user_fixable(
            "solana_codama_targets_empty",
            "At least one Codama target must be specified.",
        ));
    }
    let idl_path = PathBuf::from(&request.idl_path);
    if !idl_path.is_file() {
        return Err(CommandError::user_fixable(
            "solana_codama_idl_missing",
            format!("IDL file {} does not exist.", idl_path.display()),
        ));
    }
    let output_root = PathBuf::from(&request.output_dir);
    fs::create_dir_all(&output_root).map_err(|err| {
        CommandError::system_fault(
            "solana_codama_output_dir_failed",
            format!("Could not create {}: {err}", output_root.display()),
        )
    })?;

    let mut results = Vec::with_capacity(request.targets.len());
    // De-duplicate so a caller passing the same target twice doesn't
    // double-run codegen.
    let mut seen = std::collections::HashSet::new();
    for target in &request.targets {
        if !seen.insert(*target) {
            continue;
        }
        let subdir = output_root.join(target.default_subdir());
        fs::create_dir_all(&subdir).map_err(|err| {
            CommandError::system_fault(
                "solana_codama_output_subdir_failed",
                format!("Could not create {}: {err}", subdir.display()),
            )
        })?;
        let invocation = CodamaInvocation {
            target: *target,
            idl_path: idl_path.clone(),
            output_dir: subdir,
        };
        let result = runner.run(&invocation)?;
        results.push(result);
    }
    let all_ok = results.iter().all(|r| r.success);
    Ok(CodamaGenerationReport {
        idl_path: idl_path.display().to_string(),
        output_dir: output_root.display().to_string(),
        targets: results,
        elapsed_ms: start.elapsed().as_millis(),
        all_succeeded: all_ok,
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

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    pub struct MockCodamaRunner {
        pub calls: Mutex<Vec<CodamaInvocation>>,
        pub script: Mutex<std::collections::HashMap<CodamaTarget, (bool, String, String)>>,
    }

    impl MockCodamaRunner {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn set_success(&self, target: CodamaTarget, stdout: impl Into<String>) {
            self.script
                .lock()
                .unwrap()
                .insert(target, (true, stdout.into(), String::new()));
        }

        pub fn set_failure(
            &self,
            target: CodamaTarget,
            stdout: impl Into<String>,
            stderr: impl Into<String>,
        ) {
            self.script
                .lock()
                .unwrap()
                .insert(target, (false, stdout.into(), stderr.into()));
        }
    }

    impl CodamaRunner for MockCodamaRunner {
        fn run(&self, invocation: &CodamaInvocation) -> CommandResult<CodamaTargetResult> {
            self.calls.lock().unwrap().push(invocation.clone());
            let scripted = self
                .script
                .lock()
                .unwrap()
                .get(&invocation.target)
                .cloned()
                .unwrap_or((true, "ok".into(), "".into()));
            Ok(CodamaTargetResult {
                target: invocation.target,
                output_subdir: invocation.output_dir.display().to_string(),
                success: scripted.0,
                exit_code: Some(if scripted.0 { 0 } else { 1 }),
                stdout_excerpt: scripted.1,
                stderr_excerpt: scripted.2,
                elapsed_ms: 1,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockCodamaRunner;
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generate_runs_every_unique_target() {
        let tmp = TempDir::new().unwrap();
        let idl = tmp.path().join("idl.json");
        fs::write(&idl, b"{}").unwrap();
        let out = tmp.path().join("clients");

        let runner = MockCodamaRunner::new();
        runner.set_success(CodamaTarget::Ts, "ok-ts");
        runner.set_success(CodamaTarget::Rust, "ok-rust");

        let report = generate(
            &runner,
            &CodamaGenerationRequest {
                idl_path: idl.display().to_string(),
                targets: vec![CodamaTarget::Ts, CodamaTarget::Rust, CodamaTarget::Ts],
                output_dir: out.display().to_string(),
            },
        )
        .unwrap();

        assert_eq!(report.targets.len(), 2); // duplicate Ts skipped
        assert!(report.all_succeeded);
        assert!(out.join("ts").is_dir());
        assert!(out.join("rust").is_dir());
    }

    #[test]
    fn generate_surfaces_target_failures_without_bailing_early() {
        let tmp = TempDir::new().unwrap();
        let idl = tmp.path().join("idl.json");
        fs::write(&idl, b"{}").unwrap();
        let out = tmp.path().join("clients");

        let runner = MockCodamaRunner::new();
        runner.set_failure(CodamaTarget::Ts, "partial", "boom");
        runner.set_success(CodamaTarget::Rust, "ok");

        let report = generate(
            &runner,
            &CodamaGenerationRequest {
                idl_path: idl.display().to_string(),
                targets: vec![CodamaTarget::Ts, CodamaTarget::Rust],
                output_dir: out.display().to_string(),
            },
        )
        .unwrap();
        assert!(!report.all_succeeded);
        let ts = report
            .targets
            .iter()
            .find(|t| t.target == CodamaTarget::Ts)
            .unwrap();
        assert!(!ts.success);
        assert_eq!(ts.stderr_excerpt, "boom");
    }

    #[test]
    fn generate_rejects_missing_idl() {
        let tmp = TempDir::new().unwrap();
        let runner = MockCodamaRunner::new();
        let err = generate(
            &runner,
            &CodamaGenerationRequest {
                idl_path: tmp.path().join("nope.json").display().to_string(),
                targets: vec![CodamaTarget::Ts],
                output_dir: tmp.path().display().to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_codama_idl_missing");
    }

    #[test]
    fn generate_requires_at_least_one_target() {
        let tmp = TempDir::new().unwrap();
        let idl = tmp.path().join("idl.json");
        fs::write(&idl, b"{}").unwrap();
        let runner = MockCodamaRunner::new();
        let err = generate(
            &runner,
            &CodamaGenerationRequest {
                idl_path: idl.display().to_string(),
                targets: vec![],
                output_dir: tmp.path().display().to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_codama_targets_empty");
    }
}
