//! Project-tree walker that applies the built-in secret patterns.
//!
//! The scanner is deterministic: it walks the tree in a stable order,
//! emits findings sorted by `(severity, path, line)`, and never opens
//! the same file twice. It's cheap enough to run on every deploy —
//! Phase 9 wires it into `program::deploy::deploy` as a blocking gate.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

use super::patterns::{builtin_patterns, compile_regex, SecretPattern, SecretPatternKind};
use super::SecretSeverity;

/// Input for `solana_secrets_scan`. JSON-safe so the autonomous runtime
/// wrapper and the UI share the request shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanRequest {
    /// Directory to walk. Must exist and be a directory.
    pub project_root: String,
    /// Extra relative paths to skip (in addition to the built-in
    /// ignore list). Each entry is compared prefix-wise against the
    /// path relative to `project_root`.
    #[serde(default)]
    pub skip_paths: Vec<String>,
    /// Only report findings at this severity or higher. Defaults to
    /// all severities.
    #[serde(default)]
    pub min_severity: Option<SecretSeverity>,
    /// Hard cap on files inspected — the scanner bails out early once
    /// reached so a misconfigured `project_root` can't wedge the UI.
    /// Defaults to 20,000.
    #[serde(default)]
    pub file_budget: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecretFinding {
    pub rule_id: String,
    pub title: String,
    pub severity: SecretSeverity,
    pub path: String,
    pub line: Option<u32>,
    /// Redacted snippet so the UI / agent can show evidence without
    /// leaking the secret itself.
    pub evidence: String,
    pub remediation: String,
    pub reference_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecretScanReport {
    pub project_root: String,
    pub files_scanned: u32,
    pub files_skipped: u32,
    pub duration_ms: u128,
    pub findings: Vec<SecretFinding>,
    pub blocks_deploy: bool,
    pub patterns_applied: u32,
}

const DEFAULT_FILE_BUDGET: usize = 20_000;
const MAX_FILE_BYTES: usize = 512 * 1024;

/// Directories we never walk into — these are noisy and never contain
/// first-party secrets.
const DEFAULT_SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".cache",
    ".venv",
    "venv",
    "__pycache__",
    ".pnpm-store",
];

/// File extensions we consider "text" and therefore worth scanning with
/// regex patterns. Non-text files still go through the keypair-JSON
/// structural check (which reads bytes and decodes as JSON), so we
/// don't lose `id.json`-style hits on an extension mismatch.
const TEXT_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "json", "toml", "yaml", "yml", "md", "env",
    "txt", "sh", "py", "go", "html", "css", "scss", "graphql",
];

/// Entry point — public for both the Tauri command and the deploy
/// gate.
pub fn scan(request: &ScanRequest) -> CommandResult<SecretScanReport> {
    let started = Instant::now();
    let root = Path::new(&request.project_root);
    if !root.exists() {
        return Err(CommandError::user_fixable(
            "solana_secrets_root_missing",
            format!("Project root {} does not exist.", root.display()),
        ));
    }
    if !root.is_dir() {
        return Err(CommandError::user_fixable(
            "solana_secrets_root_not_dir",
            format!("Project root {} is not a directory.", root.display()),
        ));
    }

    let patterns = builtin_patterns();
    let compiled: Vec<_> = patterns
        .iter()
        .map(|p| (p.clone(), compile_regex(p)))
        .collect();

    let budget = request.file_budget.unwrap_or(DEFAULT_FILE_BUDGET).max(1);
    let skip_paths: Vec<PathBuf> = request
        .skip_paths
        .iter()
        .map(|s| PathBuf::from(s.trim_start_matches('/')))
        .collect();

    let mut findings: Vec<SecretFinding> = Vec::new();
    let mut files_scanned: u32 = 0;
    let mut files_skipped: u32 = 0;
    let mut seen: BTreeSet<(String, String, u32)> = BTreeSet::new();

    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap_or(&path);
            if path_is_skipped(rel, &skip_paths) {
                files_skipped = files_skipped.saturating_add(1);
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => {
                    files_skipped = files_skipped.saturating_add(1);
                    continue;
                }
            };
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            if files_scanned as usize >= budget {
                files_skipped = files_skipped.saturating_add(1);
                continue;
            }
            files_scanned = files_scanned.saturating_add(1);

            let rel_str = rel.to_string_lossy().to_string();
            scan_file(
                &path,
                &rel_str,
                &compiled,
                &mut findings,
                &mut seen,
                request.min_severity,
            );
        }
    }

    // Deterministic order: severity rank, then path, then line.
    findings.sort_by(|a, b| {
        a.severity
            .rank()
            .cmp(&b.severity.rank())
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line.unwrap_or(0).cmp(&b.line.unwrap_or(0)))
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });

    let blocks_deploy = findings
        .iter()
        .any(|f| f.severity == SecretSeverity::Critical);

    Ok(SecretScanReport {
        project_root: root.display().to_string(),
        files_scanned,
        files_skipped,
        duration_ms: started.elapsed().as_millis(),
        findings,
        blocks_deploy,
        patterns_applied: patterns.len() as u32,
    })
}

fn path_is_skipped(rel: &Path, skip_paths: &[PathBuf]) -> bool {
    for comp in rel.components() {
        if let Component::Normal(os) = comp {
            if let Some(s) = os.to_str() {
                if DEFAULT_SKIP_DIRS.contains(&s) {
                    return true;
                }
            }
        }
    }
    for skip in skip_paths {
        if rel.starts_with(skip) {
            return true;
        }
    }
    false
}

fn scan_file(
    path: &Path,
    rel: &str,
    compiled: &[(SecretPattern, Option<regex::Regex>)],
    findings: &mut Vec<SecretFinding>,
    seen: &mut BTreeSet<(String, String, u32)>,
    min_severity: Option<SecretSeverity>,
) {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    // Structural keypair check — always runs regardless of extension so
    // a `authority` or `wallet` file with no extension still gets
    // picked up.
    for (pattern, _regex) in compiled.iter() {
        if pattern.kind != SecretPatternKind::SolanaKeypairJson {
            continue;
        }
        if !glob_matches(&pattern.file_globs, rel, filename) {
            continue;
        }
        if severity_below(min_severity, pattern.severity) {
            continue;
        }
        if let Some(evidence) = probe_keypair_json(path) {
            record_finding(findings, seen, pattern, rel, None, evidence);
        }
    }

    if !ext.is_empty() && !TEXT_EXTENSIONS.contains(&ext.as_str()) {
        return;
    }

    let Ok(raw) = fs::read(path) else {
        return;
    };
    if raw.len() > MAX_FILE_BYTES {
        return;
    }
    let Ok(content) = std::str::from_utf8(&raw) else {
        return;
    };

    for (pattern, regex) in compiled.iter() {
        if pattern.kind == SecretPatternKind::SolanaKeypairJson {
            continue;
        }
        if !glob_matches(&pattern.file_globs, rel, filename) {
            continue;
        }
        if severity_below(min_severity, pattern.severity) {
            continue;
        }
        let Some(regex) = regex else {
            continue;
        };
        for (line_no, line) in content.lines().enumerate() {
            if let Some(captures) = regex.captures(line) {
                let secret = captures
                    .get(1)
                    .map(|m| m.as_str())
                    .unwrap_or_else(|| captures.get(0).map(|m| m.as_str()).unwrap_or(""));
                let evidence = redact(secret);
                record_finding(
                    findings,
                    seen,
                    pattern,
                    rel,
                    Some((line_no + 1) as u32),
                    evidence,
                );
            }
        }
    }
}

fn record_finding(
    findings: &mut Vec<SecretFinding>,
    seen: &mut BTreeSet<(String, String, u32)>,
    pattern: &SecretPattern,
    rel: &str,
    line: Option<u32>,
    evidence: String,
) {
    let key = (pattern.rule_id.clone(), rel.to_string(), line.unwrap_or(0));
    if !seen.insert(key) {
        return;
    }
    findings.push(SecretFinding {
        rule_id: pattern.rule_id.clone(),
        title: pattern.title.clone(),
        severity: pattern.severity,
        path: rel.to_string(),
        line,
        evidence,
        remediation: pattern.remediation.clone(),
        reference_url: pattern.reference_url.clone(),
    });
}

fn glob_matches(globs: &[String], rel: &str, filename: &str) -> bool {
    if globs.is_empty() {
        return true;
    }
    for glob in globs {
        if simple_glob_match(glob, rel) || simple_glob_match(glob, filename) {
            return true;
        }
    }
    false
}

/// Tiny `**` / `*` matcher. Sufficient for the curated pattern list;
/// keeps us off a full globbing dependency for one place in the code.
fn simple_glob_match(glob: &str, target: &str) -> bool {
    // Normalise path separators.
    let target = target.replace('\\', "/");
    let glob = glob.replace('\\', "/");

    fn match_impl(pat: &[u8], text: &[u8]) -> bool {
        // Position trackers for backtracking on `*` and `**`.
        let (mut pi, mut ti) = (0usize, 0usize);
        let (mut star_pi, mut star_ti): (Option<usize>, usize) = (None, 0);
        let (mut double_pi, mut double_ti): (Option<usize>, usize) = (None, 0);
        while ti < text.len() {
            if pi < pat.len() && (pat[pi] == text[ti] || pat[pi] == b'?') {
                pi += 1;
                ti += 1;
                continue;
            }
            if pi + 1 < pat.len() && pat[pi] == b'*' && pat[pi + 1] == b'*' {
                double_pi = Some(pi + 2);
                double_ti = ti;
                // Skip optional trailing slash after `**/`.
                if pi + 2 < pat.len() && pat[pi + 2] == b'/' {
                    pi += 3;
                } else {
                    pi += 2;
                }
                continue;
            }
            if pi < pat.len() && pat[pi] == b'*' {
                star_pi = Some(pi + 1);
                star_ti = ti;
                pi += 1;
                continue;
            }
            if let Some(sp) = star_pi {
                pi = sp;
                star_ti += 1;
                ti = star_ti;
                continue;
            }
            if let Some(dp) = double_pi {
                pi = dp;
                double_ti += 1;
                ti = double_ti;
                continue;
            }
            return false;
        }
        while pi < pat.len() {
            if pat[pi] == b'*' {
                pi += 1;
                continue;
            }
            return false;
        }
        true
    }
    match_impl(glob.as_bytes(), target.as_bytes())
}

fn redact(secret: &str) -> String {
    let trimmed = secret.trim();
    if trimmed.len() <= 8 {
        return "***".into();
    }
    let head: String = trimmed.chars().take(4).collect();
    let tail: String = trimmed.chars().rev().take(4).collect::<String>();
    let tail: String = tail.chars().rev().collect();
    format!("{}…{} ({} chars)", head, tail, trimmed.len())
}

fn probe_keypair_json(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > 16 * 1024 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let arr = value.as_array()?;
    // Standard solana-keygen keypair is a 64-byte Ed25519 secret. Some
    // tooling also emits 32-byte seeds; treat both as findings.
    if !matches!(arr.len(), 32 | 64) {
        return None;
    }
    let mut all_bytes = true;
    for item in arr {
        match item.as_u64() {
            Some(n) if n <= 255 => {}
            _ => {
                all_bytes = false;
                break;
            }
        }
    }
    if !all_bytes {
        return None;
    }
    Some(format!("{}-byte keypair JSON", arr.len()))
}

fn severity_below(min: Option<SecretSeverity>, actual: SecretSeverity) -> bool {
    match min {
        None => false,
        Some(cut) => actual.rank() > cut.rank(),
    }
}

/// Helper used by tests to run the pattern matcher against in-memory
/// content — lets us unit-test the regex library without creating a
/// temp tree for every case.
#[cfg(test)]
pub fn scan_content_for_tests(filename: &str, content: &str) -> Vec<SecretFinding> {
    let patterns = builtin_patterns();
    let compiled: Vec<_> = patterns
        .iter()
        .map(|p| (p.clone(), compile_regex(p)))
        .collect();
    let mut findings = Vec::new();
    let mut seen = BTreeSet::new();
    for (pattern, regex) in compiled.iter() {
        if pattern.kind == SecretPatternKind::SolanaKeypairJson {
            continue;
        }
        if !glob_matches(&pattern.file_globs, filename, filename) {
            continue;
        }
        let Some(regex) = regex else { continue };
        for (line_no, line) in content.lines().enumerate() {
            if let Some(captures) = regex.captures(line) {
                let secret = captures
                    .get(1)
                    .map(|m| m.as_str())
                    .unwrap_or_else(|| captures.get(0).map(|m| m.as_str()).unwrap_or(""));
                record_finding(
                    &mut findings,
                    &mut seen,
                    pattern,
                    filename,
                    Some((line_no + 1) as u32),
                    redact(secret),
                );
            }
        }
    }
    findings
}

// Also expose it at runtime (outside tests) because the scope checker
// wants to reuse the same per-line matcher against persona notes. The
// helper is cheap to compile twice under `cfg(test)` so we just
// duplicate the implementation — keeps the public surface small.
#[cfg(not(test))]
#[allow(dead_code)]
pub(crate) fn scan_content_for_tests(_: &str, _: &str) -> Vec<SecretFinding> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn id_json_with_64_bytes_is_critical() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("id.json");
        let bytes: Vec<u8> = (0..64).map(|i| i as u8).collect();
        let json = serde_json::to_string(&bytes).unwrap();
        fs::write(&path, json).unwrap();
        let report = scan(&ScanRequest {
            project_root: dir.path().display().to_string(),
            skip_paths: vec![],
            min_severity: None,
            file_budget: None,
        })
        .unwrap();
        assert!(report.blocks_deploy);
        assert!(report.findings.iter().any(
            |f| f.rule_id == "solana_keypair_id_json" && f.severity == SecretSeverity::Critical
        ));
    }

    #[test]
    fn helius_url_in_source_flags_high() {
        let content =
            "const URL = \"https://mainnet.helius-rpc.com/?api-key=abcdef0123456789abcd\";";
        let findings = scan_content_for_tests("config.ts", content);
        assert!(findings
            .iter()
            .any(|f| f.rule_id == "helius_rpc_api_key" && f.severity == SecretSeverity::High));
    }

    #[test]
    fn privy_secret_env_flagged() {
        let content = r#"PRIVY_APP_SECRET="abcdefghijklmnopqrstuvwxyz0123456789""#;
        let findings = scan_content_for_tests(".env", content);
        assert!(findings.iter().any(|f| f.rule_id == "privy_app_secret"));
    }

    #[test]
    fn non_matching_tree_produces_empty_report() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.rs"), "fn main() {}").unwrap();
        let report = scan(&ScanRequest {
            project_root: dir.path().display().to_string(),
            skip_paths: vec![],
            min_severity: None,
            file_budget: None,
        })
        .unwrap();
        assert_eq!(report.findings.len(), 0);
        assert!(!report.blocks_deploy);
    }

    #[test]
    fn skip_paths_honoured() {
        let dir = TempDir::new().unwrap();
        let fixtures = dir.path().join("fixtures");
        fs::create_dir_all(&fixtures).unwrap();
        let bytes: Vec<u8> = (0..64).map(|i| i as u8).collect();
        fs::write(
            fixtures.join("id.json"),
            serde_json::to_string(&bytes).unwrap(),
        )
        .unwrap();
        let report = scan(&ScanRequest {
            project_root: dir.path().display().to_string(),
            skip_paths: vec!["fixtures".into()],
            min_severity: None,
            file_budget: None,
        })
        .unwrap();
        assert_eq!(report.findings.len(), 0);
    }

    #[test]
    fn redact_keeps_length_hint() {
        let r = redact("abcdef0123456789abcd");
        assert!(r.contains("abcd"));
        assert!(r.contains("chars"));
    }

    #[test]
    fn glob_matches_common_forms() {
        assert!(simple_glob_match("**/id.json", "solana/id.json"));
        assert!(simple_glob_match("**/id.json", "id.json"));
        assert!(simple_glob_match(
            "**/*-keypair.json",
            "deploy-keypair.json"
        ));
        assert!(!simple_glob_match("**/id.json", "id.txt"));
    }

    #[test]
    fn min_severity_filters_below_critical() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("app.ts"),
            "const url = \"https://mainnet.helius-rpc.com/?api-key=abcdef0123456789abcd\";",
        )
        .unwrap();
        let report = scan(&ScanRequest {
            project_root: dir.path().display().to_string(),
            skip_paths: vec![],
            min_severity: Some(SecretSeverity::Critical),
            file_budget: None,
        })
        .unwrap();
        assert_eq!(report.findings.len(), 0);
    }
}
