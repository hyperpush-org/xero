//! `anchor build` / `cargo build-sbf` wrapper.
//!
//! Captures diagnostics, hashes the resulting `.so`, and returns a
//! structured report that the deploy pipeline (and the agent) can reason
//! about without re-parsing build logs.
//!
//! Anchor projects (those with an `Anchor.toml` next to or above the
//! manifest) build via `anchor build`, which internally invokes
//! `cargo build-sbf` plus IDL extraction. Plain `cargo build-sbf`
//! projects skip the IDL step. We auto-detect Anchor unless the caller
//! pins `BuildKind` explicitly.
//!
//! The runner trait keeps the integration tests free of any real
//! cargo / anchor invocation — they script stdout / stderr / exit code
//! and assert on the captured argv.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::commands::{CommandError, CommandResult};

/// Default release-build timeout. SBF builds of medium-sized Anchor
/// programs commonly take 30–90s; CI cold builds can exceed 5 minutes.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// Maximum captured stdout/stderr in the report. Anything past this
/// cutoff is truncated with a trailing marker so the agent doesn't
/// drown in megabytes of cargo output.
const CAPTURE_BYTES: usize = 16_384;

/// `solana_program_build` profile selector.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildProfile {
    /// `--debug` — fast iteration, larger `.so`, never deployed to
    /// non-local clusters by the deploy gate.
    Dev,
    /// `--release` (Anchor's default) — what every cluster except a
    /// local debug session expects.
    Release,
}

impl BuildProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            BuildProfile::Dev => "dev",
            BuildProfile::Release => "release",
        }
    }
}

impl Default for BuildProfile {
    fn default() -> Self {
        BuildProfile::Release
    }
}

/// Which builder to invoke. Detected from the project layout when not
/// pinned by the caller.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildKind {
    Anchor,
    CargoBuildSbf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuildRequest {
    /// Path to either `Anchor.toml` (Anchor projects) or `Cargo.toml`
    /// (plain `cargo build-sbf` projects). The runner switches
    /// behaviour based on the file name + sibling files.
    pub manifest_path: String,
    #[serde(default)]
    pub profile: BuildProfile,
    /// When set, overrides the auto-detected `BuildKind`. Useful for
    /// hybrid repos that contain both an `Anchor.toml` and a parallel
    /// non-Anchor program.
    #[serde(default)]
    pub kind: Option<BuildKind>,
    /// Limit the build to a single program inside an Anchor workspace.
    /// Forwards to `anchor build -p <name>` / `cargo build-sbf -p <name>`.
    #[serde(default)]
    pub program: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuiltArtifact {
    pub program: String,
    pub so_path: String,
    pub so_size_bytes: u64,
    pub so_sha256: String,
    pub idl_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuildReport {
    pub kind: BuildKind,
    pub profile: BuildProfile,
    pub manifest_path: String,
    pub argv: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub elapsed_ms: u128,
    pub artifacts: Vec<BuiltArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInvocation {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub envs: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait BuildRunner: Send + Sync + std::fmt::Debug {
    fn run(&self, invocation: &BuildInvocation) -> CommandResult<BuildOutcome>;
}

#[derive(Debug, Default)]
pub struct SystemBuildRunner;

impl SystemBuildRunner {
    pub fn new() -> Self {
        Self
    }
}

impl BuildRunner for SystemBuildRunner {
    fn run(&self, invocation: &BuildInvocation) -> CommandResult<BuildOutcome> {
        let (program, args) = invocation.argv.split_first().ok_or_else(|| {
            CommandError::system_fault(
                "solana_program_build_empty_argv",
                "Empty argv passed to program build runner.",
            )
        })?;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&invocation.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        for (k, v) in &invocation.envs {
            cmd.env(k, v);
        }
        let child = cmd.spawn().map_err(|err| {
            CommandError::user_fixable(
                "solana_program_build_spawn_failed",
                format!(
                    "Could not run `{}`: {err}. Install Anchor / cargo-build-sbf and ensure they are on PATH.",
                    program
                ),
            )
        })?;
        let output = wait_with_timeout(child, invocation.timeout).ok_or_else(|| {
            CommandError::retryable(
                "solana_program_build_timeout",
                format!(
                    "Program build timed out after {}s.",
                    invocation.timeout.as_secs()
                ),
            )
        })?;
        Ok(BuildOutcome {
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

pub fn build(runner: &dyn BuildRunner, request: &BuildRequest) -> CommandResult<BuildReport> {
    let manifest = Path::new(&request.manifest_path);
    if !manifest.is_file() {
        return Err(CommandError::user_fixable(
            "solana_program_build_missing_manifest",
            format!("Manifest path {} does not exist.", manifest.display()),
        ));
    }
    let manifest_dir = manifest
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let kind = request.kind.unwrap_or_else(|| detect_kind(manifest));
    let invocation = build_invocation(kind, &manifest_dir, request)?;
    let argv = invocation.argv.clone();

    let start = Instant::now();
    let outcome = runner.run(&invocation)?;
    let elapsed_ms = start.elapsed().as_millis();

    let project_root = project_root_for(kind, &manifest_dir);
    let artifacts = if outcome.success {
        collect_artifacts(kind, &project_root, request)?
    } else {
        Vec::new()
    };

    Ok(BuildReport {
        kind,
        profile: request.profile,
        manifest_path: manifest.display().to_string(),
        argv,
        success: outcome.success,
        exit_code: outcome.exit_code,
        stdout_excerpt: truncate(&outcome.stdout, CAPTURE_BYTES),
        stderr_excerpt: truncate(&outcome.stderr, CAPTURE_BYTES),
        elapsed_ms,
        artifacts,
    })
}

fn detect_kind(manifest: &Path) -> BuildKind {
    let name = manifest.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.eq_ignore_ascii_case("Anchor.toml") {
        return BuildKind::Anchor;
    }
    // If a sibling Anchor.toml exists next to a Cargo.toml, treat it as
    // an Anchor project — `anchor build` knows how to drive cargo for us.
    if let Some(parent) = manifest.parent() {
        if parent.join("Anchor.toml").is_file() {
            return BuildKind::Anchor;
        }
        // Walk one level up — Anchor workspaces typically nest programs
        // inside `programs/<name>/Cargo.toml` with the Anchor.toml at the
        // workspace root.
        if let Some(grand) = parent.parent().and_then(|g| g.parent()) {
            if grand.join("Anchor.toml").is_file() {
                return BuildKind::Anchor;
            }
        }
    }
    BuildKind::CargoBuildSbf
}

fn project_root_for(kind: BuildKind, manifest_dir: &Path) -> PathBuf {
    match kind {
        BuildKind::Anchor => {
            // Climb until we find Anchor.toml.
            let mut cursor = manifest_dir.to_path_buf();
            loop {
                if cursor.join("Anchor.toml").is_file() {
                    return cursor;
                }
                match cursor.parent() {
                    Some(p) if p != cursor => cursor = p.to_path_buf(),
                    _ => return manifest_dir.to_path_buf(),
                }
            }
        }
        BuildKind::CargoBuildSbf => manifest_dir.to_path_buf(),
    }
}

fn build_invocation(
    kind: BuildKind,
    manifest_dir: &Path,
    request: &BuildRequest,
) -> CommandResult<BuildInvocation> {
    let mut argv: Vec<String> = match kind {
        BuildKind::Anchor => {
            let mut v = vec!["anchor".to_string(), "build".to_string()];
            if matches!(request.profile, BuildProfile::Release) {
                // anchor build defaults to release; pass --no-docs to skip
                // doc generation noise and keep build stable across versions.
                v.push("--no-docs".to_string());
            }
            if let Some(program) = request.program.as_deref() {
                v.push("-p".to_string());
                v.push(program.to_string());
            }
            v
        }
        BuildKind::CargoBuildSbf => {
            let mut v = vec!["cargo".to_string(), "build-sbf".to_string()];
            // cargo build-sbf has no --debug flag in most versions; the
            // Anchor profile is the source of truth. We still pass
            // --features when the caller asks for dev to leave a hook for
            // future expansion.
            if matches!(request.profile, BuildProfile::Dev) {
                v.push("--features".to_string());
                v.push("debug".to_string());
            }
            if let Some(program) = request.program.as_deref() {
                v.push("-p".to_string());
                v.push(program.to_string());
            }
            v
        }
    };
    if argv.is_empty() {
        return Err(CommandError::system_fault(
            "solana_program_build_empty_argv",
            "Refusing to invoke an empty argv.",
        ));
    }
    // `anchor build` writes IDL and `.so` into <root>/target by default;
    // forcing a deterministic CWD avoids surprises when the caller's
    // current dir is somewhere else (the desktop process root, say).
    let cwd = project_root_for(kind, manifest_dir);
    Ok(BuildInvocation {
        argv: std::mem::take(&mut argv),
        cwd,
        timeout: DEFAULT_TIMEOUT,
        envs: Vec::new(),
    })
}

fn collect_artifacts(
    kind: BuildKind,
    project_root: &Path,
    request: &BuildRequest,
) -> CommandResult<Vec<BuiltArtifact>> {
    let target_root = project_root.join("target");
    let so_dir = match kind {
        BuildKind::Anchor => target_root.join("deploy"),
        BuildKind::CargoBuildSbf => target_root.join("deploy"),
    };
    let idl_dir = target_root.join("idl");

    if !so_dir.is_dir() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&so_dir).map_err(|err| {
        CommandError::system_fault(
            "solana_program_build_read_deploy_failed",
            format!("Could not list {}: {err}", so_dir.display()),
        )
    })?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("so") {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        if let Some(filter) = request.program.as_deref() {
            // Anchor / cargo can sometimes leave the program name as
            // either snake_case (file system) or kebab-case (Cargo
            // package name). Compare flexibly.
            if !names_match(filter, &stem) {
                continue;
            }
        }
        let bytes = fs::read(&path).map_err(|err| {
            CommandError::system_fault(
                "solana_program_build_read_so_failed",
                format!("Could not read {}: {err}", path.display()),
            )
        })?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let so_sha256 = hex_lower(hasher.finalize().as_slice());

        let idl_candidate = idl_dir.join(format!("{}.json", stem));
        let idl_path = if idl_candidate.is_file() {
            Some(idl_candidate.display().to_string())
        } else {
            None
        };

        out.push(BuiltArtifact {
            program: stem,
            so_path: path.display().to_string(),
            so_size_bytes: bytes.len() as u64,
            so_sha256,
            idl_path,
        });
    }
    out.sort_by(|a, b| a.program.cmp(&b.program));
    Ok(out)
}

fn names_match(a: &str, b: &str) -> bool {
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    let normalize = |s: &str| s.replace('-', "_").to_ascii_lowercase();
    normalize(a) == normalize(b)
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
        let mut out = s.chars().take(max).collect::<String>();
        out.push_str("… (truncated)");
        out
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    pub struct MockBuildRunner {
        pub calls: Mutex<Vec<BuildInvocation>>,
        pub outcome: Mutex<Option<BuildOutcome>>,
    }

    impl MockBuildRunner {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn set_outcome(&self, outcome: BuildOutcome) {
            *self.outcome.lock().unwrap() = Some(outcome);
        }
    }

    impl BuildRunner for MockBuildRunner {
        fn run(&self, invocation: &BuildInvocation) -> CommandResult<BuildOutcome> {
            self.calls.lock().unwrap().push(invocation.clone());
            Ok(self
                .outcome
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(BuildOutcome {
                    exit_code: Some(0),
                    success: true,
                    stdout: "Compiling ...\nFinished release [optimized] target(s)\n".into(),
                    stderr: String::new(),
                }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockBuildRunner;
    use super::*;
    use tempfile::TempDir;

    fn touch(path: &Path, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn detect_anchor_when_anchor_toml_present() {
        let tmp = TempDir::new().unwrap();
        let anchor = tmp.path().join("Anchor.toml");
        touch(&anchor, b"[programs.localnet]\n");
        assert_eq!(detect_kind(&anchor), BuildKind::Anchor);
    }

    #[test]
    fn detect_cargo_build_sbf_for_plain_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        let cargo = tmp.path().join("Cargo.toml");
        touch(&cargo, b"[package]\nname = \"p\"\n");
        assert_eq!(detect_kind(&cargo), BuildKind::CargoBuildSbf);
    }

    #[test]
    fn detect_anchor_when_sibling_anchor_toml_present() {
        let tmp = TempDir::new().unwrap();
        let cargo = tmp.path().join("Cargo.toml");
        touch(&cargo, b"[package]\nname = \"p\"\n");
        touch(&tmp.path().join("Anchor.toml"), b"[programs.localnet]\n");
        assert_eq!(detect_kind(&cargo), BuildKind::Anchor);
    }

    #[test]
    fn build_emits_anchor_argv_for_anchor_manifest() {
        let tmp = TempDir::new().unwrap();
        let manifest = tmp.path().join("Anchor.toml");
        touch(&manifest, b"[programs.localnet]\n");
        let runner = MockBuildRunner::new();
        let report = build(
            &runner,
            &BuildRequest {
                manifest_path: manifest.display().to_string(),
                profile: BuildProfile::Release,
                kind: None,
                program: Some("my_program".into()),
            },
        )
        .unwrap();
        assert_eq!(report.kind, BuildKind::Anchor);
        assert!(report.argv.contains(&"anchor".to_string()));
        assert!(report.argv.contains(&"build".to_string()));
        assert!(report.argv.contains(&"-p".to_string()));
        assert!(report.argv.contains(&"my_program".to_string()));
    }

    #[test]
    fn build_emits_cargo_argv_for_plain_cargo() {
        let tmp = TempDir::new().unwrap();
        let manifest = tmp.path().join("Cargo.toml");
        touch(&manifest, b"[package]\n");
        let runner = MockBuildRunner::new();
        let report = build(
            &runner,
            &BuildRequest {
                manifest_path: manifest.display().to_string(),
                profile: BuildProfile::Release,
                kind: None,
                program: None,
            },
        )
        .unwrap();
        assert_eq!(report.kind, BuildKind::CargoBuildSbf);
        assert_eq!(&report.argv[0], "cargo");
        assert_eq!(&report.argv[1], "build-sbf");
    }

    #[test]
    fn build_collects_so_artifacts_and_hashes_them() {
        let tmp = TempDir::new().unwrap();
        let manifest = tmp.path().join("Anchor.toml");
        touch(&manifest, b"[programs.localnet]\n");
        let so = tmp.path().join("target/deploy/my_program.so");
        let so_bytes: Vec<u8> = (0..2048u32).map(|n| (n & 0xff) as u8).collect();
        touch(&so, &so_bytes);
        // also drop a matching IDL so we can confirm pickup
        touch(
            &tmp.path().join("target/idl/my_program.json"),
            b"{\"name\":\"my_program\"}",
        );
        let runner = MockBuildRunner::new();
        let report = build(
            &runner,
            &BuildRequest {
                manifest_path: manifest.display().to_string(),
                profile: BuildProfile::Release,
                kind: None,
                program: None,
            },
        )
        .unwrap();
        assert_eq!(report.artifacts.len(), 1);
        let art = &report.artifacts[0];
        assert_eq!(art.program, "my_program");
        assert_eq!(art.so_size_bytes, so_bytes.len() as u64);
        assert_eq!(art.so_sha256.len(), 64);
        assert!(art.idl_path.is_some());
    }

    #[test]
    fn build_filters_artifacts_by_program_name_with_kebab_normalisation() {
        let tmp = TempDir::new().unwrap();
        let manifest = tmp.path().join("Anchor.toml");
        touch(&manifest, b"[programs.localnet]\n");
        touch(&tmp.path().join("target/deploy/my_program.so"), &[1, 2, 3]);
        touch(&tmp.path().join("target/deploy/other.so"), &[4, 5, 6]);
        let runner = MockBuildRunner::new();
        let report = build(
            &runner,
            &BuildRequest {
                manifest_path: manifest.display().to_string(),
                profile: BuildProfile::Release,
                kind: None,
                program: Some("my-program".into()),
            },
        )
        .unwrap();
        assert_eq!(report.artifacts.len(), 1);
        assert_eq!(report.artifacts[0].program, "my_program");
    }

    #[test]
    fn build_returns_no_artifacts_on_failure() {
        let tmp = TempDir::new().unwrap();
        let manifest = tmp.path().join("Anchor.toml");
        touch(&manifest, b"[programs.localnet]\n");
        touch(&tmp.path().join("target/deploy/my_program.so"), &[1, 2, 3]);
        let runner = MockBuildRunner::new();
        runner.set_outcome(BuildOutcome {
            exit_code: Some(1),
            success: false,
            stdout: String::new(),
            stderr: "error[E0432]: ... ".into(),
        });
        let report = build(
            &runner,
            &BuildRequest {
                manifest_path: manifest.display().to_string(),
                profile: BuildProfile::Release,
                kind: None,
                program: None,
            },
        )
        .unwrap();
        assert!(!report.success);
        assert!(report.artifacts.is_empty());
        assert!(report.stderr_excerpt.contains("E0432"));
    }

    #[test]
    fn build_rejects_missing_manifest() {
        let tmp = TempDir::new().unwrap();
        let manifest = tmp.path().join("nope.toml");
        let runner = MockBuildRunner::new();
        let err = build(
            &runner,
            &BuildRequest {
                manifest_path: manifest.display().to_string(),
                profile: BuildProfile::Release,
                kind: None,
                program: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_program_build_missing_manifest");
    }
}
