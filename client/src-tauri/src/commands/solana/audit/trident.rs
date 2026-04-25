//! Trident fuzzer integration.
//!
//! Two entry points:
//!   1. `generate_harness` — scaffold a Trident fuzz harness from an
//!      IDL. The scaffold mirrors Trident's `trident-cli init` template
//!      so the user can drop it into `trident-tests/` and iterate.
//!   2. `run_fuzz` — drive `trident fuzz run --target <name>` for a
//!      bounded duration and surface crashes + coverage delta.
//!
//! The actual Trident binary is user-provided (cargo install trident-cli
//! takes ~3 min on a cold machine; we install-on-first-run per the
//! plan, not at app start). All invocations go through a
//! trait so tests can assert argv and response parsing without a
//! real fuzzer installed.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::commands::solana::toolchain;
use crate::commands::{CommandError, CommandResult};

use super::{Finding, FindingSeverity, FindingSource};

const DEFAULT_TIMEOUT_OVERHEAD: Duration = Duration::from_secs(60);
const CAPTURE_BYTES: usize = 16_384;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FuzzRequest {
    /// Project root (Anchor workspace containing a `trident-tests/`
    /// directory — or the directory we'll scaffold into first).
    pub project_root: String,
    /// Trident fuzz target name (usually the Anchor program name).
    pub target: String,
    /// Fuzz run duration in seconds. Defaults to 60.
    #[serde(default)]
    pub duration_s: Option<u64>,
    /// Optional path to a seed corpus directory.
    #[serde(default)]
    pub corpus: Option<String>,
    /// Baseline coverage line count (from a previous run) so the report
    /// can compute a coverage delta.
    #[serde(default)]
    pub baseline_coverage_lines: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FuzzCrash {
    pub id: String,
    pub instruction: Option<String>,
    pub panic_message: Option<String>,
    pub reproducer_argv: Vec<String>,
    pub backtrace_excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FuzzReport {
    #[serde(default)]
    pub run_id: String,
    pub target: String,
    pub project_root: String,
    pub argv: Vec<String>,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub duration_s: u64,
    pub elapsed_ms: u128,
    pub crashes: Vec<FuzzCrash>,
    pub coverage_lines: u64,
    pub coverage_delta: i64,
    pub findings: Vec<Finding>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TridentHarnessRequest {
    pub project_root: String,
    pub target: String,
    /// Optional path to an IDL file. When present, the scaffold emits
    /// entrypoint stubs for every instruction.
    #[serde(default)]
    pub idl_path: Option<String>,
    /// When true, overwrite existing scaffold files. Defaults to false
    /// (scaffold only when the file does not exist).
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TridentHarnessResult {
    pub root: String,
    pub generated_files: Vec<String>,
    pub skipped_files: Vec<String>,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TridentInvocation {
    pub argv: Vec<String>,
    pub cwd: String,
    pub timeout: Duration,
    pub envs: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TridentOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TridentProbe {
    pub installed: bool,
    pub binary_path: Option<String>,
}

pub trait TridentRunner: Send + Sync + std::fmt::Debug {
    fn probe(&self) -> TridentProbe;
    fn run(&self, invocation: &TridentInvocation) -> CommandResult<TridentOutcome>;
}

#[derive(Debug, Default)]
pub struct SystemTridentRunner;

impl SystemTridentRunner {
    pub fn new() -> Self {
        Self
    }
}

impl TridentRunner for SystemTridentRunner {
    fn probe(&self) -> TridentProbe {
        let binary = toolchain::resolve_binary("trident").map(|path| path.display().to_string());
        TridentProbe {
            installed: binary.is_some(),
            binary_path: binary,
        }
    }

    fn run(&self, invocation: &TridentInvocation) -> CommandResult<TridentOutcome> {
        let (program, args) = invocation.argv.split_first().ok_or_else(|| {
            CommandError::system_fault(
                "solana_audit_fuzz_empty_argv",
                "Empty argv passed to Trident runner.",
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
                "solana_audit_fuzz_spawn_failed",
                format!(
                    "Could not run `{}`: {err}. Install Trident with `cargo install trident-cli`.",
                    program
                ),
            )
        })?;
        let output = wait_with_timeout(child, invocation.timeout).ok_or_else(|| {
            CommandError::retryable(
                "solana_audit_fuzz_timeout",
                format!(
                    "Trident fuzz run timed out after {}s.",
                    invocation.timeout.as_secs()
                ),
            )
        })?;
        Ok(TridentOutcome {
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

pub fn run_fuzz(runner: &dyn TridentRunner, request: &FuzzRequest) -> CommandResult<FuzzReport> {
    let root = Path::new(&request.project_root);
    if !root.is_dir() {
        return Err(CommandError::user_fixable(
            "solana_audit_fuzz_bad_root",
            format!(
                "Trident project root {} is not a directory.",
                root.display()
            ),
        ));
    }
    if request.target.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_audit_fuzz_missing_target",
            "A fuzz target name is required — the Trident test identifier.",
        ));
    }

    let duration_s = request.duration_s.unwrap_or(60).max(1);
    let timeout = Duration::from_secs(duration_s) + DEFAULT_TIMEOUT_OVERHEAD;

    let probe = runner.probe();
    if !probe.installed {
        let finding = Finding::new(
            FindingSource::Fuzz,
            "trident_not_installed",
            FindingSeverity::Informational,
            "Trident not installed",
            "Install with `cargo install trident-cli` (≈3 minutes on a cold machine).",
        )
        .with_fix_hint(
            "Trident is a fuzzer for Anchor programs. Install it once, then run fuzz targets from the audit panel.",
        );
        return Ok(FuzzReport {
            run_id: String::new(),
            target: request.target.clone(),
            project_root: request.project_root.clone(),
            argv: Vec::new(),
            exit_code: None,
            success: false,
            duration_s,
            elapsed_ms: 0,
            crashes: Vec::new(),
            coverage_lines: 0,
            coverage_delta: 0,
            findings: vec![finding],
            stdout_excerpt: String::new(),
            stderr_excerpt: "trident binary not found on PATH".to_string(),
        });
    }

    let mut argv: Vec<String> = vec![
        "trident".to_string(),
        "fuzz".to_string(),
        "run".to_string(),
        "--target".to_string(),
        request.target.clone(),
        "--duration".to_string(),
        duration_s.to_string(),
        "--json".to_string(),
    ];
    if let Some(corpus) = request.corpus.as_deref() {
        argv.push("--corpus".to_string());
        argv.push(corpus.to_string());
    }

    let invocation = TridentInvocation {
        argv: argv.clone(),
        cwd: request.project_root.clone(),
        timeout,
        envs: Vec::new(),
    };
    let start = Instant::now();
    let outcome = runner.run(&invocation)?;
    let elapsed_ms = start.elapsed().as_millis();

    let (crashes, coverage_lines) = parse_fuzz_output(&outcome.stdout, &outcome.stderr);
    let baseline = request.baseline_coverage_lines.unwrap_or(0) as i64;
    let coverage_delta = coverage_lines as i64 - baseline;
    let findings = lift_crashes(&request.target, &crashes);

    Ok(FuzzReport {
        run_id: String::new(),
        target: request.target.clone(),
        project_root: request.project_root.clone(),
        argv,
        exit_code: outcome.exit_code,
        success: outcome.success && crashes.is_empty(),
        duration_s,
        elapsed_ms,
        crashes,
        coverage_lines,
        coverage_delta,
        findings,
        stdout_excerpt: truncate(&outcome.stdout, CAPTURE_BYTES),
        stderr_excerpt: truncate(&outcome.stderr, CAPTURE_BYTES),
    })
}

pub fn generate_harness(
    runner: &dyn TridentRunner,
    request: &TridentHarnessRequest,
) -> CommandResult<TridentHarnessResult> {
    let _ = runner; // scaffold is local — probe is informational only.
    let root = Path::new(&request.project_root);
    if !root.is_dir() {
        return Err(CommandError::user_fixable(
            "solana_audit_fuzz_bad_root",
            format!(
                "Trident scaffold root {} is not a directory.",
                root.display()
            ),
        ));
    }
    if request.target.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_audit_fuzz_missing_target",
            "A fuzz target name is required for the scaffold.",
        ));
    }

    let target = sanitize_target(&request.target);
    let fuzz_dir = root.join("trident-tests").join(&target);
    fs::create_dir_all(fuzz_dir.join("src")).map_err(|err| {
        CommandError::system_fault(
            "solana_audit_fuzz_scaffold_mkdir_failed",
            format!(
                "Could not create trident scaffold at {}: {err}",
                fuzz_dir.display()
            ),
        )
    })?;

    let ix_names = if let Some(path) = request.idl_path.as_deref() {
        read_instruction_names(path).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut generated = Vec::new();
    let mut skipped = Vec::new();

    let cargo_toml = fuzz_dir.join("Cargo.toml");
    let cargo_body = cargo_toml_body(&target);
    write_if_allowed(
        &cargo_toml,
        &cargo_body,
        request.overwrite,
        &mut generated,
        &mut skipped,
    )?;

    let trident_toml = root.join("trident-tests").join("Trident.toml");
    let trident_body = trident_toml_body();
    write_if_allowed(
        &trident_toml,
        trident_body,
        request.overwrite,
        &mut generated,
        &mut skipped,
    )?;

    let fuzz_entry = fuzz_dir.join("src").join("fuzz_target.rs");
    let fuzz_body = fuzz_entry_body(&target, &ix_names);
    write_if_allowed(
        &fuzz_entry,
        &fuzz_body,
        request.overwrite,
        &mut generated,
        &mut skipped,
    )?;

    Ok(TridentHarnessResult {
        root: fuzz_dir.display().to_string(),
        generated_files: generated,
        skipped_files: skipped,
        target: request.target.clone(),
    })
}

fn write_if_allowed(
    path: &Path,
    body: &str,
    overwrite: bool,
    generated: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> CommandResult<()> {
    let exists = path.is_file();
    if exists && !overwrite {
        skipped.push(path.display().to_string());
        return Ok(());
    }
    fs::write(path, body).map_err(|err| {
        CommandError::system_fault(
            "solana_audit_fuzz_scaffold_write_failed",
            format!("Could not write {}: {err}", path.display()),
        )
    })?;
    generated.push(path.display().to_string());
    Ok(())
}

fn sanitize_target(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn cargo_toml_body(target: &str) -> String {
    format!(
        r#"# Generated by Cadence for Trident fuzzing.
[package]
name = "trident-fuzz-{target}"
version = "0.1.0"
edition = "2021"
publish = false

[[bin]]
name = "fuzz_target"
path = "src/fuzz_target.rs"

[dependencies]
trident-fuzz = "0.8"
anchor-lang = "0.30"
"#
    )
}

fn trident_toml_body() -> &'static str {
    r#"# Cadence-generated Trident workspace descriptor.
# Tune these knobs per target as you iterate on the fuzzer.

[fuzz]
iterations = 0           # 0 = run until duration elapses
seed = 0                 # 0 = random
allow_duplicate_txs = false

[fuzz.stats]
report_interval_ms = 1000
coverage = true
"#
}

fn fuzz_entry_body(target: &str, instructions: &[String]) -> String {
    let mut ix_lines = String::new();
    if instructions.is_empty() {
        ix_lines.push_str(
            "    // TODO(cadence): add one InstructionAccount arm per IDL instruction.\n    // See https://ackee.xyz/trident for the fuzz attribute DSL.\n",
        );
    } else {
        for name in instructions {
            ix_lines.push_str(&format!(
                "    #[instruction(name = \"{name}\")]\n    {}Ix,\n",
                pascal_case(name)
            ));
        }
    }

    format!(
        r#"//! Cadence-generated Trident fuzz harness for `{target}`.
//!
//! Replace the TODO stubs with concrete account wiring once you have a
//! clearer picture of the invariants you want to maintain across tx
//! sequences. The key knobs:
//!
//!   * `FuzzInstruction` — one variant per instruction you want the
//!     fuzzer to mutate.
//!   * `FuzzAccounts` — the accounts the fuzzer will re-use across
//!     transactions.
//!   * `pre_ixs` / `post_ixs` — invariants checked before and after
//!     each generated tx.

use trident_fuzz::prelude::*;

#[derive(Default)]
pub struct FuzzAccounts;

#[derive(Arbitrary)]
#[instruction_parser]
pub enum FuzzInstruction {{
{ix_lines}
}}

fn main() {{
    fuzz_trident!({{
        target: "{target}",
        fuzz_instruction: FuzzInstruction,
        fuzz_accounts: FuzzAccounts,
    }});
}}
"#,
        target = target,
        ix_lines = ix_lines.trim_end_matches('\n'),
    )
}

fn pascal_case(s: &str) -> String {
    let mut out = String::new();
    let mut upper_next = true;
    for c in s.chars() {
        if c == '_' || c == '-' || c.is_whitespace() {
            upper_next = true;
            continue;
        }
        if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn read_instruction_names(path: &str) -> Option<Vec<String>> {
    let text = fs::read_to_string(path).ok()?;
    let value: JsonValue = serde_json::from_str(&text).ok()?;
    let ixs = value.get("instructions")?.as_array()?;
    Some(
        ixs.iter()
            .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect(),
    )
}

pub(crate) fn parse_fuzz_output(stdout: &str, stderr: &str) -> (Vec<FuzzCrash>, u64) {
    // Try JSON first — modern Trident has `--json`. Fall back to a
    // best-effort stdout/stderr scrape so the report still shows
    // *something* when the user is on an older Trident.
    if let Some((crashes, coverage)) = parse_json_output(stdout) {
        return (crashes, coverage);
    }
    if let Some((crashes, coverage)) = parse_json_output(stderr) {
        return (crashes, coverage);
    }
    (
        scrape_crashes(stdout, stderr),
        scrape_coverage(stdout, stderr),
    )
}

fn parse_json_output(raw: &str) -> Option<(Vec<FuzzCrash>, u64)> {
    let trimmed = raw.trim_start();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return None;
    }
    let value: JsonValue = serde_json::from_str(trimmed).ok()?;
    let crashes_arr = value
        .get("crashes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let coverage = value
        .get("coverage")
        .and_then(|v| v.get("lines"))
        .and_then(|v| v.as_u64())
        .or_else(|| value.get("coverageLines").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    let crashes = crashes_arr
        .iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            let id = obj
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let instruction = obj
                .get("instruction")
                .and_then(|v| v.as_str())
                .map(String::from);
            let panic_message = obj
                .get("panic")
                .or_else(|| obj.get("message"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let reproducer_argv = obj
                .get("reproducer")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let backtrace_excerpt = obj
                .get("backtrace")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(CAPTURE_BYTES)
                .collect::<String>();
            Some(FuzzCrash {
                id,
                instruction,
                panic_message,
                reproducer_argv,
                backtrace_excerpt,
            })
        })
        .collect();
    Some((crashes, coverage))
}

fn scrape_crashes(stdout: &str, stderr: &str) -> Vec<FuzzCrash> {
    let mut crashes = Vec::new();
    for raw in stdout.lines().chain(stderr.lines()) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("CRASH") || trimmed.contains("panicked at") {
            crashes.push(FuzzCrash {
                id: format!("scrape-{:x}", crashes.len()),
                instruction: None,
                panic_message: Some(trimmed.to_string()),
                reproducer_argv: Vec::new(),
                backtrace_excerpt: raw.to_string(),
            });
        }
    }
    crashes
}

fn scrape_coverage(stdout: &str, stderr: &str) -> u64 {
    let re = regex::Regex::new(r"coverage[:=]\s*(\d+)").unwrap();
    for raw in stdout.lines().chain(stderr.lines()) {
        if let Some(cap) = re.captures(raw) {
            if let Ok(n) = cap[1].parse::<u64>() {
                return n;
            }
        }
    }
    0
}

fn lift_crashes(target: &str, crashes: &[FuzzCrash]) -> Vec<Finding> {
    crashes
        .iter()
        .map(|c| {
            let title = match c.instruction.as_deref() {
                Some(ix) => format!("Fuzz crash in `{ix}` ({target})"),
                None => format!("Fuzz crash in `{target}`"),
            };
            let message = c
                .panic_message
                .clone()
                .unwrap_or_else(|| "Crash reproducer captured.".to_string());
            let mut finding = Finding::new(
                FindingSource::Fuzz,
                format!("crash:{}", c.id),
                FindingSeverity::High,
                title,
                message,
            );
            if !c.reproducer_argv.is_empty() {
                finding = finding
                    .with_fix_hint(format!("Reproduce with: {}", c.reproducer_argv.join(" ")));
            }
            finding
        })
        .collect()
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
    pub struct ScriptedTridentRunner {
        pub probe_response: Mutex<Option<TridentProbe>>,
        pub outcome: Mutex<Option<TridentOutcome>>,
        pub invocations: Mutex<Vec<TridentInvocation>>,
    }

    impl ScriptedTridentRunner {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn set_probe(&self, probe: TridentProbe) {
            *self.probe_response.lock().unwrap() = Some(probe);
        }
        pub fn set_outcome(&self, outcome: TridentOutcome) {
            *self.outcome.lock().unwrap() = Some(outcome);
        }
    }

    impl TridentRunner for ScriptedTridentRunner {
        fn probe(&self) -> TridentProbe {
            self.probe_response
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(TridentProbe {
                    installed: true,
                    binary_path: Some("/tmp/trident".into()),
                })
        }

        fn run(&self, invocation: &TridentInvocation) -> CommandResult<TridentOutcome> {
            self.invocations.lock().unwrap().push(invocation.clone());
            Ok(self
                .outcome
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(TridentOutcome {
                    exit_code: Some(0),
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::ScriptedTridentRunner;
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn reports_trident_not_installed_as_informational() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedTridentRunner::new();
        runner.set_probe(TridentProbe {
            installed: false,
            binary_path: None,
        });
        let report = run_fuzz(
            &runner,
            &FuzzRequest {
                project_root: tmp.path().display().to_string(),
                target: "my_prog".to_string(),
                duration_s: Some(10),
                corpus: None,
                baseline_coverage_lines: None,
            },
        )
        .unwrap();
        assert!(!report.success);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule_id, "trident_not_installed");
    }

    #[test]
    fn rejects_empty_target() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedTridentRunner::new();
        let err = run_fuzz(
            &runner,
            &FuzzRequest {
                project_root: tmp.path().display().to_string(),
                target: "   ".to_string(),
                duration_s: Some(10),
                corpus: None,
                baseline_coverage_lines: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_audit_fuzz_missing_target");
    }

    #[test]
    fn parses_json_crash_and_coverage_output() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedTridentRunner::new();
        runner.set_outcome(TridentOutcome {
            exit_code: Some(0),
            success: true,
            stdout: r#"{
                "crashes": [
                    {
                        "id": "c0",
                        "instruction": "withdraw",
                        "panic": "arithmetic overflow in withdraw",
                        "reproducer": ["trident", "fuzz", "repro", "c0"],
                        "backtrace": "frame 0: ..."
                    }
                ],
                "coverage": {"lines": 512}
            }"#
            .to_string(),
            stderr: String::new(),
        });

        let report = run_fuzz(
            &runner,
            &FuzzRequest {
                project_root: tmp.path().display().to_string(),
                target: "my_prog".to_string(),
                duration_s: Some(60),
                corpus: None,
                baseline_coverage_lines: Some(400),
            },
        )
        .unwrap();
        assert_eq!(report.crashes.len(), 1);
        assert_eq!(report.coverage_lines, 512);
        assert_eq!(report.coverage_delta, 112);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].severity, FindingSeverity::High);
        assert!(
            !report.success,
            "success should be false when crashes were reported"
        );
        // argv contains --json so non-JSON older trident still scrapes.
        assert!(report.argv.iter().any(|a| a == "--json"));
        assert!(report.argv.iter().any(|a| a == "--duration"));
    }

    #[test]
    fn falls_back_to_scrape_when_output_is_not_json() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedTridentRunner::new();
        runner.set_outcome(TridentOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "running target my_prog\npanicked at src/lib.rs:12\ncoverage: 220\n".into(),
            stderr: String::new(),
        });
        let report = run_fuzz(
            &runner,
            &FuzzRequest {
                project_root: tmp.path().display().to_string(),
                target: "my_prog".to_string(),
                duration_s: Some(30),
                corpus: None,
                baseline_coverage_lines: None,
            },
        )
        .unwrap();
        assert_eq!(report.coverage_lines, 220);
        assert!(!report.crashes.is_empty());
    }

    #[test]
    fn generate_harness_writes_cargo_and_fuzz_entry() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedTridentRunner::new();
        let result = generate_harness(
            &runner,
            &TridentHarnessRequest {
                project_root: tmp.path().display().to_string(),
                target: "swap".into(),
                idl_path: None,
                overwrite: false,
            },
        )
        .unwrap();
        assert!(result
            .generated_files
            .iter()
            .any(|f| f.ends_with("Cargo.toml")));
        assert!(result
            .generated_files
            .iter()
            .any(|f| f.ends_with("fuzz_target.rs")));
        assert!(tmp
            .path()
            .join("trident-tests/swap/src/fuzz_target.rs")
            .is_file());
    }

    #[test]
    fn generate_harness_honours_overwrite_flag() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedTridentRunner::new();
        let first = generate_harness(
            &runner,
            &TridentHarnessRequest {
                project_root: tmp.path().display().to_string(),
                target: "swap".into(),
                idl_path: None,
                overwrite: false,
            },
        )
        .unwrap();
        assert!(!first.generated_files.is_empty());

        let second = generate_harness(
            &runner,
            &TridentHarnessRequest {
                project_root: tmp.path().display().to_string(),
                target: "swap".into(),
                idl_path: None,
                overwrite: false,
            },
        )
        .unwrap();
        assert!(!second.skipped_files.is_empty());
        assert!(second.generated_files.is_empty());
    }

    #[test]
    fn generate_harness_emits_instruction_stubs_from_idl() {
        let tmp = TempDir::new().unwrap();
        let idl_path = tmp.path().join("prog.json");
        fs::write(
            &idl_path,
            r#"{
                "name": "prog",
                "instructions": [
                    {"name": "deposit", "accounts": []},
                    {"name": "withdraw", "accounts": []}
                ]
            }"#,
        )
        .unwrap();
        let runner = ScriptedTridentRunner::new();
        generate_harness(
            &runner,
            &TridentHarnessRequest {
                project_root: tmp.path().display().to_string(),
                target: "prog".into(),
                idl_path: Some(idl_path.display().to_string()),
                overwrite: false,
            },
        )
        .unwrap();
        let fuzz =
            fs::read_to_string(tmp.path().join("trident-tests/prog/src/fuzz_target.rs")).unwrap();
        assert!(fuzz.contains("DepositIx"));
        assert!(fuzz.contains("WithdrawIx"));
    }
}
