//! `cargo-llvm-cov` orchestration + lcov parsing.
//!
//! Two concerns:
//!   1. Driving `cargo llvm-cov --lcov` so the user doesn't have to
//!      remember the exact invocation. The runner is mockable so tests
//!      can inject a canned lcov report.
//!   2. Parsing the lcov output into per-file + per-instruction
//!      coverage. We map each function onto a nearest-instruction
//!      heuristic: if the function name matches an Anchor handler
//!      (`<program>::<instruction>` or `pub fn <instruction>`) we tag
//!      it so the UI can render a per-instruction column.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::commands::solana::toolchain;
use crate::commands::{CommandError, CommandResult};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(900);
const CAPTURE_BYTES: usize = 16_384;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CoverageRequest {
    pub project_root: String,
    /// Limit coverage to a single cargo package (workspace).
    #[serde(default)]
    pub package: Option<String>,
    /// Passed through to `cargo llvm-cov --test`. When absent we use
    /// `--all-targets`.
    #[serde(default)]
    pub test_filter: Option<String>,
    /// Optional path to a pre-existing lcov report. Short-circuits the
    /// `cargo llvm-cov` invocation — useful when the caller ran
    /// coverage themselves and just wants the parse.
    #[serde(default)]
    pub lcov_path: Option<String>,
    /// Optional list of instruction names. When present we light up a
    /// per-instruction panel in the report.
    #[serde(default)]
    pub instruction_names: Vec<String>,
    /// Timeout for the cargo invocation in seconds.
    #[serde(default)]
    pub timeout_s: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LcovRecord {
    pub file: String,
    pub lines_found: u64,
    pub lines_hit: u64,
    pub functions_found: u64,
    pub functions_hit: u64,
    pub branches_found: u64,
    pub branches_hit: u64,
    pub functions: Vec<FunctionCoverage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCoverage {
    pub name: String,
    pub line: u32,
    pub hits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstructionCoverage {
    pub instruction: String,
    pub functions_found: u64,
    pub functions_hit: u64,
    pub lines_found: u64,
    pub lines_hit: u64,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoverageReport {
    #[serde(default)]
    pub run_id: String,
    pub project_root: String,
    pub argv: Vec<String>,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub elapsed_ms: u128,
    pub line_coverage_percent: f32,
    pub function_coverage_percent: f32,
    pub total_lines_found: u64,
    pub total_lines_hit: u64,
    pub total_functions_found: u64,
    pub total_functions_hit: u64,
    pub files: Vec<LcovRecord>,
    pub instructions: Vec<InstructionCoverage>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub lcov_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageInvocation {
    pub argv: Vec<String>,
    pub cwd: String,
    pub timeout: Duration,
    pub envs: Vec<(OsString, OsString)>,
    pub lcov_out: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    /// Path where the runner wrote the lcov report. When the caller
    /// pre-supplied an `lcov_path`, the runner should echo it back here
    /// so the caller can still parse the file.
    pub lcov_path: String,
}

pub trait CoverageRunner: Send + Sync + std::fmt::Debug {
    fn run(&self, invocation: &CoverageInvocation) -> CommandResult<CoverageOutcome>;
}

#[derive(Debug, Default)]
pub struct SystemCoverageRunner;

impl SystemCoverageRunner {
    pub fn new() -> Self {
        Self
    }
}

impl CoverageRunner for SystemCoverageRunner {
    fn run(&self, invocation: &CoverageInvocation) -> CommandResult<CoverageOutcome> {
        let (program, args) = invocation.argv.split_first().ok_or_else(|| {
            CommandError::system_fault(
                "solana_audit_coverage_empty_argv",
                "Empty argv passed to coverage runner.",
            )
        })?;
        let (resolved_program, resolved_args) = resolve_coverage_program(program, args);
        let mut cmd = Command::new(&resolved_program);
        cmd.args(&resolved_args)
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
                "solana_audit_coverage_spawn_failed",
                format!(
                    "Could not run `{}`: {err}. Install with `cargo install cargo-llvm-cov`.",
                    program
                ),
            )
        })?;
        let output = wait_with_timeout(child, invocation.timeout).ok_or_else(|| {
            CommandError::retryable(
                "solana_audit_coverage_timeout",
                format!(
                    "Coverage run timed out after {}s.",
                    invocation.timeout.as_secs()
                ),
            )
        })?;
        Ok(CoverageOutcome {
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            lcov_path: invocation.lcov_out.clone(),
        })
    }
}

fn resolve_coverage_program(program: &str, args: &[String]) -> (String, Vec<String>) {
    if program == "cargo" && args.first().map(String::as_str) == Some("llvm-cov") {
        if let Some(path) = toolchain::resolve_binary("cargo-llvm-cov") {
            return (
                path.to_string_lossy().into_owned(),
                args.iter().skip(1).cloned().collect(),
            );
        }
    }
    (toolchain::resolve_command(program), args.to_vec())
}

pub fn run(
    runner: &dyn CoverageRunner,
    request: &CoverageRequest,
) -> CommandResult<CoverageReport> {
    let root = Path::new(&request.project_root);
    if !root.is_dir() {
        return Err(CommandError::user_fixable(
            "solana_audit_coverage_bad_root",
            format!("Coverage root {} is not a directory.", root.display()),
        ));
    }

    let start = Instant::now();
    let (argv, invocation, outcome, lcov_path) = if let Some(lcov) = request.lcov_path.clone() {
        let argv = vec!["<pre-existing lcov>".to_string(), lcov.clone()];
        let invocation = CoverageInvocation {
            argv: argv.clone(),
            cwd: request.project_root.clone(),
            timeout: Duration::from_secs(1),
            envs: Vec::new(),
            lcov_out: lcov.clone(),
        };
        let outcome = CoverageOutcome {
            exit_code: Some(0),
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            lcov_path: lcov.clone(),
        };
        (argv, invocation, outcome, lcov)
    } else {
        let lcov_out = root.join("target").join("cadence-llvm-cov.lcov");
        let mut argv: Vec<String> = vec![
            "cargo".to_string(),
            "llvm-cov".to_string(),
            "--lcov".to_string(),
            "--output-path".to_string(),
            lcov_out.display().to_string(),
        ];
        if let Some(package) = request.package.as_deref() {
            argv.push("-p".to_string());
            argv.push(package.to_string());
        }
        if let Some(filter) = request.test_filter.as_deref() {
            argv.push("--tests".to_string());
            argv.push("--".to_string());
            argv.push(filter.to_string());
        } else {
            argv.push("--all-targets".to_string());
        }

        let timeout = Duration::from_secs(request.timeout_s.unwrap_or(DEFAULT_TIMEOUT.as_secs()));
        let invocation = CoverageInvocation {
            argv: argv.clone(),
            cwd: request.project_root.clone(),
            timeout,
            envs: Vec::new(),
            lcov_out: lcov_out.display().to_string(),
        };
        let outcome = runner.run(&invocation)?;
        (argv, invocation, outcome.clone(), outcome.lcov_path)
    };

    let elapsed_ms = start.elapsed().as_millis();

    if !outcome.success && request.lcov_path.is_none() {
        return Ok(CoverageReport {
            run_id: String::new(),
            project_root: request.project_root.clone(),
            argv,
            exit_code: outcome.exit_code,
            success: false,
            elapsed_ms,
            line_coverage_percent: 0.0,
            function_coverage_percent: 0.0,
            total_lines_found: 0,
            total_lines_hit: 0,
            total_functions_found: 0,
            total_functions_hit: 0,
            files: Vec::new(),
            instructions: Vec::new(),
            stdout_excerpt: truncate(&outcome.stdout, CAPTURE_BYTES),
            stderr_excerpt: truncate(&outcome.stderr, CAPTURE_BYTES),
            lcov_path: None,
        });
    }

    let text = std::fs::read_to_string(&lcov_path).map_err(|err| {
        CommandError::system_fault(
            "solana_audit_coverage_read_lcov_failed",
            format!("Could not read lcov report at {lcov_path}: {err}"),
        )
    })?;

    let files = parse_lcov(&text);
    let total_lines_found: u64 = files.iter().map(|r| r.lines_found).sum();
    let total_lines_hit: u64 = files.iter().map(|r| r.lines_hit).sum();
    let total_functions_found: u64 = files.iter().map(|r| r.functions_found).sum();
    let total_functions_hit: u64 = files.iter().map(|r| r.functions_hit).sum();

    let line_coverage_percent = percent(total_lines_hit, total_lines_found);
    let function_coverage_percent = percent(total_functions_hit, total_functions_found);

    let instructions = instruction_rollups(&files, &request.instruction_names);

    Ok(CoverageReport {
        run_id: String::new(),
        project_root: request.project_root.clone(),
        argv,
        exit_code: outcome.exit_code,
        success: true,
        elapsed_ms,
        line_coverage_percent,
        function_coverage_percent,
        total_lines_found,
        total_lines_hit,
        total_functions_found,
        total_functions_hit,
        files,
        instructions,
        stdout_excerpt: truncate(&outcome.stdout, CAPTURE_BYTES),
        stderr_excerpt: truncate(&outcome.stderr, CAPTURE_BYTES),
        lcov_path: Some(invocation.lcov_out),
    })
}

pub(crate) fn parse_lcov(text: &str) -> Vec<LcovRecord> {
    let mut out = Vec::new();
    let mut current: Option<LcovRecord> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(path) = line.strip_prefix("SF:") {
            current = Some(LcovRecord {
                file: path.to_string(),
                lines_found: 0,
                lines_hit: 0,
                functions_found: 0,
                functions_hit: 0,
                branches_found: 0,
                branches_hit: 0,
                functions: Vec::new(),
            });
            continue;
        }
        if line == "end_of_record" {
            if let Some(rec) = current.take() {
                out.push(rec);
            }
            continue;
        }
        let rec = match current.as_mut() {
            Some(r) => r,
            None => continue,
        };
        if let Some(v) = line.strip_prefix("LH:") {
            rec.lines_hit = v.parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("LF:") {
            rec.lines_found = v.parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("FNH:") {
            rec.functions_hit = v.parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("FNF:") {
            rec.functions_found = v.parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("BRH:") {
            rec.branches_hit = v.parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("BRF:") {
            rec.branches_found = v.parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("FN:") {
            // FN:<line>,<name>
            let mut parts = v.splitn(2, ',');
            let line_no = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
            let name = parts.next().unwrap_or("").to_string();
            rec.functions.push(FunctionCoverage {
                name,
                line: line_no,
                hits: 0,
            });
        } else if let Some(v) = line.strip_prefix("FNDA:") {
            // FNDA:<hits>,<name>
            let mut parts = v.splitn(2, ',');
            let hits = parts.next().unwrap_or("0").parse::<u64>().unwrap_or(0);
            let name = parts.next().unwrap_or("");
            if let Some(f) = rec.functions.iter_mut().find(|f| f.name == name) {
                f.hits = f.hits.max(hits);
            }
        }
    }

    if let Some(rec) = current {
        out.push(rec);
    }
    out
}

fn instruction_rollups(files: &[LcovRecord], names: &[String]) -> Vec<InstructionCoverage> {
    if names.is_empty() {
        return Vec::new();
    }
    names
        .iter()
        .map(|ix| {
            let normalized = ix.to_ascii_lowercase();
            let mut rollup = InstructionCoverage {
                instruction: ix.clone(),
                functions_found: 0,
                functions_hit: 0,
                lines_found: 0,
                lines_hit: 0,
                files: Vec::new(),
            };
            for file in files {
                let mut touched_file = false;
                for func in &file.functions {
                    let matches = func.name.to_ascii_lowercase().contains(&normalized);
                    if !matches {
                        continue;
                    }
                    rollup.functions_found += 1;
                    if func.hits > 0 {
                        rollup.functions_hit += 1;
                    }
                    touched_file = true;
                }
                if touched_file {
                    // Apportion a share of the lines to this instruction.
                    // We don't have per-function line attribution in lcov
                    // without DA records, so we approximate with the file
                    // totals — it's a rough but stable number that lines
                    // up with the per-file drilldown.
                    rollup.lines_found += file.lines_found;
                    rollup.lines_hit += file.lines_hit;
                    rollup.files.push(file.file.clone());
                }
            }
            rollup
        })
        .collect()
}

fn percent(hit: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        ((hit as f64 / total as f64) * 100.0) as f32
    }
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

#[cfg(test)]
pub mod test_support {
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ScriptedCoverageRunner {
        pub outcome: Mutex<Option<CoverageOutcome>>,
        pub invocations: Mutex<Vec<CoverageInvocation>>,
        pub write_lcov: Mutex<Option<String>>,
    }

    impl ScriptedCoverageRunner {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn set_outcome(&self, outcome: CoverageOutcome) {
            *self.outcome.lock().unwrap() = Some(outcome);
        }
        /// Pre-populate the lcov file written by the coverage run.
        pub fn set_lcov_body(&self, body: impl Into<String>) {
            *self.write_lcov.lock().unwrap() = Some(body.into());
        }
    }

    impl CoverageRunner for ScriptedCoverageRunner {
        fn run(&self, invocation: &CoverageInvocation) -> CommandResult<CoverageOutcome> {
            self.invocations.lock().unwrap().push(invocation.clone());
            if let Some(body) = self.write_lcov.lock().unwrap().clone() {
                if let Some(parent) = Path::new(&invocation.lcov_out).parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&invocation.lcov_out, body).unwrap();
            }
            Ok(self
                .outcome
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(CoverageOutcome {
                    exit_code: Some(0),
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                    lcov_path: invocation.lcov_out.clone(),
                }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::ScriptedCoverageRunner;
    use super::*;
    use tempfile::TempDir;

    const SAMPLE_LCOV: &str = "SF:programs/p/src/lib.rs\nFN:10,prog::deposit\nFN:30,prog::withdraw\nFNDA:3,prog::deposit\nFNDA:0,prog::withdraw\nFNF:2\nFNH:1\nDA:10,3\nDA:15,3\nDA:30,0\nDA:35,0\nLF:4\nLH:2\nBRF:2\nBRH:1\nend_of_record\nSF:programs/p/src/state.rs\nFN:5,prog::state::pack\nFNDA:1,prog::state::pack\nFNF:1\nFNH:1\nLF:10\nLH:9\nend_of_record\n";

    #[test]
    fn parse_lcov_extracts_totals_and_functions() {
        let files = parse_lcov(SAMPLE_LCOV);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].lines_found, 4);
        assert_eq!(files[0].lines_hit, 2);
        assert_eq!(files[0].functions_found, 2);
        assert_eq!(files[0].functions_hit, 1);
        assert_eq!(files[0].functions.len(), 2);
        let deposit = files[0]
            .functions
            .iter()
            .find(|f| f.name == "prog::deposit")
            .unwrap();
        assert_eq!(deposit.hits, 3);
    }

    #[test]
    fn run_uses_pre_existing_lcov_when_provided() {
        let tmp = TempDir::new().unwrap();
        let lcov = tmp.path().join("existing.lcov");
        std::fs::write(&lcov, SAMPLE_LCOV).unwrap();

        let runner = ScriptedCoverageRunner::new();
        let report = run(
            &runner,
            &CoverageRequest {
                project_root: tmp.path().display().to_string(),
                package: None,
                test_filter: None,
                lcov_path: Some(lcov.display().to_string()),
                instruction_names: vec!["deposit".into(), "withdraw".into()],
                timeout_s: Some(60),
            },
        )
        .unwrap();

        assert!(report.success);
        assert_eq!(report.total_lines_found, 14);
        assert_eq!(report.total_lines_hit, 11);
        assert!((report.line_coverage_percent - (11.0 / 14.0 * 100.0) as f32).abs() < 0.1);
        // Instruction rollups should light up both names.
        assert_eq!(report.instructions.len(), 2);
        let deposit = report
            .instructions
            .iter()
            .find(|i| i.instruction == "deposit")
            .unwrap();
        assert_eq!(deposit.functions_found, 1);
        assert_eq!(deposit.functions_hit, 1);
    }

    #[test]
    fn run_invokes_cargo_llvm_cov_and_parses_output() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedCoverageRunner::new();
        runner.set_lcov_body(SAMPLE_LCOV);
        let report = run(
            &runner,
            &CoverageRequest {
                project_root: tmp.path().display().to_string(),
                package: Some("p".into()),
                test_filter: None,
                lcov_path: None,
                instruction_names: vec!["deposit".into()],
                timeout_s: Some(60),
            },
        )
        .unwrap();

        assert!(report.success);
        let invocations = runner.invocations.lock().unwrap();
        assert_eq!(invocations.len(), 1);
        assert!(invocations[0].argv.iter().any(|a| a == "--lcov"));
        assert!(invocations[0].argv.iter().any(|a| a == "-p"));
        assert!(invocations[0].argv.iter().any(|a| a == "p"));
        assert!(invocations[0].argv.iter().any(|a| a == "--all-targets"));
        assert_eq!(report.files.len(), 2);
        assert_eq!(report.instructions.len(), 1);
    }

    #[test]
    fn run_fails_cleanly_when_root_missing() {
        let runner = ScriptedCoverageRunner::new();
        let err = run(
            &runner,
            &CoverageRequest {
                project_root: "/does/not/exist/i/hope".into(),
                package: None,
                test_filter: None,
                lcov_path: None,
                instruction_names: Vec::new(),
                timeout_s: Some(30),
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_audit_coverage_bad_root");
    }

    #[test]
    fn run_returns_unsuccess_when_cargo_exits_non_zero() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedCoverageRunner::new();
        runner.set_outcome(CoverageOutcome {
            exit_code: Some(101),
            success: false,
            stdout: String::new(),
            stderr: "error: no tests found".into(),
            lcov_path: String::new(),
        });
        let report = run(
            &runner,
            &CoverageRequest {
                project_root: tmp.path().display().to_string(),
                package: None,
                test_filter: None,
                lcov_path: None,
                instruction_names: Vec::new(),
                timeout_s: Some(60),
            },
        )
        .unwrap();
        assert!(!report.success);
        assert_eq!(report.exit_code, Some(101));
        assert!(report.stderr_excerpt.contains("no tests found"));
    }
}
