//! External-analyzer wrapper — Sec3 / Soteria / Aderyn.
//!
//! None of these ship with Cadence; each is `cargo install`d (Aderyn)
//! or distributed as a single binary by its vendor. We probe PATH for
//! the binary, run it if present, and parse its JSON output into our
//! unified `Finding` shape. When the binary is absent we return a
//! report with `analyzerInstalled = false` and a single informational
//! finding telling the user how to install it — never a hard error.
//!
//! The runner trait abstracts the actual invocation so integration
//! tests can script a JSON response without needing Sec3 / Soteria on
//! the CI host.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::commands::{CommandError, CommandResult};

use super::{Finding, FindingSeverity, FindingSource};

/// Which external analyzer to run. `Auto` tries them in order
/// (Sec3 → Soteria → Aderyn) and returns the first one available.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnalyzerKind {
    #[default]
    Auto,
    Sec3,
    Soteria,
    Aderyn,
}

impl AnalyzerKind {
    pub fn candidates(self) -> &'static [AnalyzerKind] {
        match self {
            AnalyzerKind::Auto => &[
                AnalyzerKind::Sec3,
                AnalyzerKind::Soteria,
                AnalyzerKind::Aderyn,
            ],
            AnalyzerKind::Sec3 => &[AnalyzerKind::Sec3],
            AnalyzerKind::Soteria => &[AnalyzerKind::Soteria],
            AnalyzerKind::Aderyn => &[AnalyzerKind::Aderyn],
        }
    }

    pub fn binary(self) -> &'static str {
        match self {
            AnalyzerKind::Auto => "",
            AnalyzerKind::Sec3 => "sec3",
            AnalyzerKind::Soteria => "soteria",
            AnalyzerKind::Aderyn => "aderyn",
        }
    }

    pub fn install_hint(self) -> &'static str {
        match self {
            AnalyzerKind::Auto => {
                "Install one of: Sec3 (https://www.sec3.dev), Soteria (https://www.soteria.dev), or Aderyn (`cargo install aderyn`)."
            }
            AnalyzerKind::Sec3 => {
                "Install the Sec3 CLI from https://www.sec3.dev/audit-ai and ensure `sec3` is on your PATH."
            }
            AnalyzerKind::Soteria => {
                "Install Soteria from https://www.soteria.dev and ensure `soteria` is on your PATH."
            }
            AnalyzerKind::Aderyn => {
                "Install Aderyn with `cargo install aderyn` (or via the Cyfrin release channel)."
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAnalyzerRequest {
    /// Path to a project root containing the program(s) to analyze.
    pub project_root: String,
    #[serde(default)]
    pub analyzer: AnalyzerKind,
    /// Timeout in seconds. Defaults to 15 min — external analyzers can
    /// be slow.
    #[serde(default)]
    pub timeout_s: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAnalyzerReport {
    #[serde(default)]
    pub run_id: String,
    pub analyzer: AnalyzerKind,
    pub analyzer_installed: bool,
    pub binary_path: Option<String>,
    pub argv: Vec<String>,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u128,
    pub findings: Vec<Finding>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerInvocation {
    pub analyzer: AnalyzerKind,
    pub binary: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub timeout: Duration,
    pub envs: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerProbe {
    pub installed: bool,
    pub binary_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait ExternalAnalyzerRunner: Send + Sync + std::fmt::Debug {
    fn probe(&self, kind: AnalyzerKind) -> AnalyzerProbe;
    fn run(&self, invocation: &AnalyzerInvocation) -> CommandResult<AnalyzerOutcome>;
}

#[derive(Debug, Default)]
pub struct SystemExternalAnalyzerRunner;

impl SystemExternalAnalyzerRunner {
    pub fn new() -> Self {
        Self
    }
}

impl ExternalAnalyzerRunner for SystemExternalAnalyzerRunner {
    fn probe(&self, kind: AnalyzerKind) -> AnalyzerProbe {
        if kind == AnalyzerKind::Auto {
            return AnalyzerProbe {
                installed: false,
                binary_path: None,
            };
        }
        match which_binary(kind.binary()) {
            Some(path) => AnalyzerProbe {
                installed: true,
                binary_path: Some(path),
            },
            None => AnalyzerProbe {
                installed: false,
                binary_path: None,
            },
        }
    }

    fn run(&self, invocation: &AnalyzerInvocation) -> CommandResult<AnalyzerOutcome> {
        let mut cmd = Command::new(&invocation.binary);
        cmd.args(&invocation.argv)
            .current_dir(&invocation.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        for (k, v) in &invocation.envs {
            cmd.env(k, v);
        }
        let child = cmd.spawn().map_err(|err| {
            CommandError::user_fixable(
                "solana_audit_external_spawn_failed",
                format!(
                    "Could not run `{}`: {err}. Install the analyzer or pin a different kind.",
                    invocation.binary
                ),
            )
        })?;
        let output = wait_with_timeout(child, invocation.timeout).ok_or_else(|| {
            CommandError::retryable(
                "solana_audit_external_timeout",
                format!(
                    "External analyzer {} timed out after {}s.",
                    invocation.binary,
                    invocation.timeout.as_secs()
                ),
            )
        })?;
        Ok(AnalyzerOutcome {
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

pub fn run(
    runner: &dyn ExternalAnalyzerRunner,
    request: &ExternalAnalyzerRequest,
) -> CommandResult<ExternalAnalyzerReport> {
    let root = Path::new(&request.project_root);
    if !root.is_dir() {
        return Err(CommandError::user_fixable(
            "solana_audit_external_bad_root",
            format!("External audit root {} is not a directory.", root.display()),
        ));
    }

    let start = Instant::now();
    let timeout = Duration::from_secs(request.timeout_s.unwrap_or(900).max(5));

    // Probe every candidate analyzer; pick the first that's installed.
    let mut resolved: Option<(AnalyzerKind, String)> = None;
    for kind in request.analyzer.candidates() {
        let probe = runner.probe(*kind);
        if probe.installed {
            resolved = Some((
                *kind,
                probe
                    .binary_path
                    .unwrap_or_else(|| kind.binary().to_string()),
            ));
            break;
        }
    }

    let (kind, binary_path) = match resolved {
        Some(v) => v,
        None => {
            let advice_analyzer = match request.analyzer {
                AnalyzerKind::Auto => AnalyzerKind::Aderyn,
                other => other,
            };
            let mut finding = Finding::new(
                FindingSource::External,
                "analyzer_not_installed",
                FindingSeverity::Informational,
                "No external analyzer installed",
                advice_analyzer.install_hint(),
            );
            finding.fix_hint = Some(advice_analyzer.install_hint().to_string());
            return Ok(ExternalAnalyzerReport {
                run_id: String::new(),
                analyzer: request.analyzer,
                analyzer_installed: false,
                binary_path: None,
                argv: Vec::new(),
                exit_code: None,
                elapsed_ms: start.elapsed().as_millis(),
                findings: vec![finding],
                stdout_excerpt: String::new(),
                stderr_excerpt: String::new(),
                summary: format!(
                    "No external analyzer installed (requested: {:?}).",
                    request.analyzer
                ),
            });
        }
    };

    let argv = argv_for(kind, &request.project_root);
    let invocation = AnalyzerInvocation {
        analyzer: kind,
        binary: binary_path.clone(),
        argv: argv.clone(),
        cwd: request.project_root.clone(),
        timeout,
        envs: Vec::new(),
    };
    let outcome = runner.run(&invocation)?;
    let findings = parse_findings(kind, &outcome.stdout, &outcome.stderr);

    let summary = if outcome.success {
        format!("{} completed: {} findings.", kind.binary(), findings.len())
    } else {
        format!(
            "{} exited with code {:?}, {} findings parsed.",
            kind.binary(),
            outcome.exit_code,
            findings.len()
        )
    };

    Ok(ExternalAnalyzerReport {
        run_id: String::new(),
        analyzer: kind,
        analyzer_installed: true,
        binary_path: Some(binary_path),
        argv,
        exit_code: outcome.exit_code,
        elapsed_ms: start.elapsed().as_millis(),
        findings,
        stdout_excerpt: truncate(&outcome.stdout, 16_384),
        stderr_excerpt: truncate(&outcome.stderr, 16_384),
        summary,
    })
}

fn argv_for(kind: AnalyzerKind, project_root: &str) -> Vec<String> {
    match kind {
        AnalyzerKind::Auto => Vec::new(),
        AnalyzerKind::Sec3 => vec![
            "audit".to_string(),
            "--json".to_string(),
            project_root.to_string(),
        ],
        AnalyzerKind::Soteria => vec![
            "--format".to_string(),
            "json".to_string(),
            project_root.to_string(),
        ],
        AnalyzerKind::Aderyn => vec![
            "--output".to_string(),
            "json".to_string(),
            project_root.to_string(),
        ],
    }
}

/// Best-effort JSON parser for analyzer output. Each analyzer formats
/// findings slightly differently; we normalise to the shared `Finding`
/// shape.
pub(crate) fn parse_findings(kind: AnalyzerKind, stdout: &str, stderr: &str) -> Vec<Finding> {
    // First strip any ANSI escapes the CLI may have leaked into stdout.
    let cleaned = strip_ansi(stdout);
    // Try stdout first; fall back to stderr which some analyzers use
    // as their output stream when `--json` is passed.
    let primary = if looks_like_json(&cleaned) {
        cleaned.clone()
    } else if looks_like_json(stderr) {
        strip_ansi(stderr)
    } else {
        cleaned
    };

    let parsed: Option<JsonValue> = serde_json::from_str(&primary).ok();
    let value = match parsed {
        Some(v) => v,
        None => return Vec::new(),
    };

    let mut findings = Vec::new();
    // Try a handful of common top-level shapes:
    // * `{"findings": [...]}` (Sec3, Aderyn)
    // * `{"issues": [...]}`   (Soteria)
    // * `[...]`               (bare array — treat as findings).
    let array_candidate = value
        .get("findings")
        .or_else(|| value.get("issues"))
        .or_else(|| value.get("vulnerabilities"))
        .cloned()
        .or_else(|| {
            if value.is_array() {
                Some(value.clone())
            } else {
                None
            }
        });

    if let Some(JsonValue::Array(items)) = array_candidate {
        for item in items {
            if let Some(f) = lift_item(kind, &item) {
                findings.push(f);
            }
        }
    }

    findings
}

fn lift_item(kind: AnalyzerKind, item: &JsonValue) -> Option<Finding> {
    let obj = item.as_object()?;
    let title = pick_str(obj, &["title", "rule", "id", "name", "check"])?.to_string();
    let rule_id = pick_str(obj, &["id", "rule", "ruleId", "code", "check"])
        .unwrap_or(&title)
        .to_string();
    let severity = parse_severity(pick_str(obj, &["severity", "level", "impact"]).unwrap_or(""));
    let message = pick_str(obj, &["message", "description", "summary"])
        .unwrap_or("")
        .to_string();
    let file = pick_str(obj, &["file", "path", "filename", "source"]);
    let line = pick_u32(obj, &["line", "lineNumber", "line_no"]);
    let column = pick_u32(obj, &["column", "col"]);

    let mut finding = Finding::new(
        FindingSource::External,
        format!("{}:{}", kind.binary(), rule_id),
        severity,
        title,
        message,
    );
    if let Some(f) = file {
        finding = finding.with_file(f.to_string());
    }
    if let (Some(l), Some(c)) = (line, column) {
        finding = finding.with_location(l, c);
    } else if let Some(l) = line {
        finding = finding.with_location(l, 1);
    }
    if let Some(hint) = pick_str(obj, &["remediation", "recommendation", "hint", "fix"]) {
        finding = finding.with_fix_hint(hint.to_string());
    }
    if let Some(reference) = pick_str(obj, &["url", "link", "reference"]) {
        finding = finding.with_reference(reference.to_string());
    }
    Some(finding)
}

fn pick_str<'a>(obj: &'a serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(v) = obj.get(*key).and_then(|v| v.as_str()) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn pick_u32(obj: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<u32> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(n) = v.as_u64() {
                return Some(n as u32);
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.parse::<u32>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn parse_severity(raw: &str) -> FindingSeverity {
    match raw.trim().to_ascii_lowercase().as_str() {
        "critical" | "crit" | "blocker" => FindingSeverity::Critical,
        "high" | "severe" | "major" => FindingSeverity::High,
        "medium" | "med" | "moderate" | "warning" => FindingSeverity::Medium,
        "low" | "minor" => FindingSeverity::Low,
        _ => FindingSeverity::Informational,
    }
}

fn looks_like_json(s: &str) -> bool {
    let trimmed = s.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn strip_ansi(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").unwrap();
    re.replace_all(s, "").into_owned()
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

/// Cross-platform `which` wrapper. On Unix we walk PATH; on Windows we
/// also consider the PATHEXT suffixes (`.exe`, `.cmd`, …). Shared shape
/// with the toolchain probe so behaviour stays consistent.
fn which_binary(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
        #[cfg(target_os = "windows")]
        {
            for ext in ["exe", "cmd", "bat", "ps1"] {
                let with_ext = dir.join(format!("{name}.{ext}"));
                if with_ext.is_file() {
                    return Some(with_ext.display().to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
pub mod test_support {
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ScriptedAnalyzerRunner {
        pub probe_response: Mutex<Option<AnalyzerProbe>>,
        pub outcome: Mutex<Option<AnalyzerOutcome>>,
        pub invocations: Mutex<Vec<AnalyzerInvocation>>,
    }

    impl ScriptedAnalyzerRunner {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn set_probe(&self, probe: AnalyzerProbe) {
            *self.probe_response.lock().unwrap() = Some(probe);
        }
        pub fn set_outcome(&self, outcome: AnalyzerOutcome) {
            *self.outcome.lock().unwrap() = Some(outcome);
        }
    }

    impl ExternalAnalyzerRunner for ScriptedAnalyzerRunner {
        fn probe(&self, _kind: AnalyzerKind) -> AnalyzerProbe {
            self.probe_response
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(AnalyzerProbe {
                    installed: false,
                    binary_path: None,
                })
        }

        fn run(&self, invocation: &AnalyzerInvocation) -> CommandResult<AnalyzerOutcome> {
            self.invocations.lock().unwrap().push(invocation.clone());
            Ok(self
                .outcome
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(AnalyzerOutcome {
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
    use super::test_support::ScriptedAnalyzerRunner;
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn returns_informational_finding_when_no_analyzer_installed() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedAnalyzerRunner::new();
        runner.set_probe(AnalyzerProbe {
            installed: false,
            binary_path: None,
        });
        let report = run(
            &runner,
            &ExternalAnalyzerRequest {
                project_root: tmp.path().display().to_string(),
                analyzer: AnalyzerKind::Auto,
                timeout_s: Some(10),
            },
        )
        .unwrap();
        assert!(!report.analyzer_installed);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule_id, "analyzer_not_installed");
        assert_eq!(report.findings[0].severity, FindingSeverity::Informational);
    }

    #[test]
    fn parses_findings_field_from_analyzer_output() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedAnalyzerRunner::new();
        runner.set_probe(AnalyzerProbe {
            installed: true,
            binary_path: Some("/opt/sec3/bin/sec3".into()),
        });
        runner.set_outcome(AnalyzerOutcome {
            exit_code: Some(0),
            success: true,
            stdout: r#"{
                "findings": [
                    {
                        "id": "SEC3_0001",
                        "title": "Missing signer check",
                        "severity": "high",
                        "message": "Withdraw lacks Signer",
                        "file": "programs/p/src/lib.rs",
                        "line": 42,
                        "column": 5
                    },
                    {
                        "id": "SEC3_0002",
                        "title": "Unchecked arithmetic",
                        "severity": "medium",
                        "message": "Use checked_add",
                        "file": "programs/p/src/calc.rs",
                        "line": 10
                    }
                ]
            }"#
            .to_string(),
            stderr: String::new(),
        });

        let report = run(
            &runner,
            &ExternalAnalyzerRequest {
                project_root: tmp.path().display().to_string(),
                analyzer: AnalyzerKind::Sec3,
                timeout_s: Some(60),
            },
        )
        .unwrap();

        assert!(report.analyzer_installed);
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.findings[0].severity, FindingSeverity::High);
        assert_eq!(report.findings[0].line, Some(42));
        assert_eq!(report.findings[1].severity, FindingSeverity::Medium);
    }

    #[test]
    fn parses_issues_field_as_fallback() {
        let tmp = TempDir::new().unwrap();
        let runner = ScriptedAnalyzerRunner::new();
        runner.set_probe(AnalyzerProbe {
            installed: true,
            binary_path: Some("/usr/local/bin/soteria".into()),
        });
        runner.set_outcome(AnalyzerOutcome {
            exit_code: Some(0),
            success: true,
            stdout: r#"{"issues":[{"title":"Panic in loop","severity":"critical","message":"unwrap in handler"}]}"#
                .to_string(),
            stderr: String::new(),
        });

        let report = run(
            &runner,
            &ExternalAnalyzerRequest {
                project_root: tmp.path().display().to_string(),
                analyzer: AnalyzerKind::Soteria,
                timeout_s: Some(60),
            },
        )
        .unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].severity, FindingSeverity::Critical);
    }

    #[test]
    fn bad_root_rejected() {
        let runner = ScriptedAnalyzerRunner::new();
        let err = run(
            &runner,
            &ExternalAnalyzerRequest {
                project_root: "/does/not/exist/i/hope".into(),
                analyzer: AnalyzerKind::Auto,
                timeout_s: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_audit_external_bad_root");
    }

    #[test]
    fn argv_for_sec3_uses_audit_subcommand() {
        let argv = argv_for(AnalyzerKind::Sec3, "/tmp/proj");
        assert_eq!(
            argv,
            vec!["audit".to_string(), "--json".into(), "/tmp/proj".into()]
        );
    }

    #[test]
    fn severity_strings_round_trip() {
        assert_eq!(parse_severity("Critical"), FindingSeverity::Critical);
        assert_eq!(parse_severity("major"), FindingSeverity::High);
        assert_eq!(parse_severity("warning"), FindingSeverity::Medium);
        assert_eq!(parse_severity("minor"), FindingSeverity::Low);
        assert_eq!(parse_severity(""), FindingSeverity::Informational);
    }
}
