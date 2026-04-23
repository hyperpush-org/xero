//! CLI toolchain probe for the Solana workbench. Detects whether each of the
//! user-provided prerequisite CLIs is on PATH, parses their `--version`
//! output, and returns a single serializable status blob to the frontend.
//!
//! None of these detection failures are fatal — the workbench surfaces a
//! "missing-toolchain" panel and lets the user keep opening the sidebar.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const PROBE_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolProbe {
    pub present: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainStatus {
    pub solana_cli: ToolProbe,
    pub anchor: ToolProbe,
    pub cargo_build_sbf: ToolProbe,
    pub rust: ToolProbe,
    pub node: ToolProbe,
    pub pnpm: ToolProbe,
    pub surfpool: ToolProbe,
    pub trident: ToolProbe,
    pub codama: ToolProbe,
    pub solana_verify: ToolProbe,
    /// Windows-only probe for WSL2 presence. `None` on non-Windows.
    pub wsl2: Option<ToolProbe>,
}

impl ToolchainStatus {
    /// True when the bare-minimum binaries for starting any cluster are
    /// present. Used by the UI's "ready to go" summary.
    pub fn has_minimum_for_localnet(&self) -> bool {
        self.solana_cli.present
    }
}

/// Probe every CLI the workbench cares about. Safe to call on any platform
/// — absent binaries return `present: false` rather than erroring.
pub fn probe() -> ToolchainStatus {
    ToolchainStatus {
        solana_cli: probe_tool("solana", &["--version"]),
        anchor: probe_tool("anchor", &["--version"]),
        cargo_build_sbf: probe_tool("cargo-build-sbf", &["--version"]),
        rust: probe_tool("rustc", &["--version"]),
        node: probe_tool("node", &["--version"]),
        pnpm: probe_tool("pnpm", &["--version"]),
        surfpool: probe_tool("surfpool", &["--version"]),
        trident: probe_tool("trident", &["--version"]),
        codama: probe_tool("codama", &["--version"]),
        solana_verify: probe_tool("solana-verify", &["--version"]),
        wsl2: probe_wsl2(),
    }
}

fn probe_wsl2() -> Option<ToolProbe> {
    if cfg!(target_os = "windows") {
        Some(probe_tool("wsl", &["--status"]))
    } else {
        None
    }
}

/// Look up `name` on PATH (plus common shell-profile dirs missed by `which`).
/// If found, run it once to capture a one-line version string.
pub fn probe_tool(name: &str, version_args: &[&str]) -> ToolProbe {
    let Some(path) = locate_binary(name) else {
        return ToolProbe::default();
    };

    let version = run_version(&path, version_args);
    ToolProbe {
        present: true,
        path: Some(path_to_string(&path)),
        version,
    }
}

fn locate_binary(name: &str) -> Option<PathBuf> {
    // Standard PATH scan first.
    if let Ok(path_var) = env::var("PATH") {
        for entry in env::split_paths(&path_var) {
            if let Some(candidate) = candidate_in_dir(&entry, name) {
                return Some(candidate);
            }
        }
    }

    // Fallback to well-known install prefixes that the user's shell might
    // add only inside interactive sessions (e.g. a launched-from-Finder
    // Tauri app inherits the LaunchServices env, not ~/.zshrc).
    for extra in fallback_dirs() {
        if let Some(candidate) = candidate_in_dir(&extra, name) {
            return Some(candidate);
        }
    }

    None
}

fn fallback_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/share/solana/install/active_release/bin"));
        dirs.push(home.join(".cargo/bin"));
        dirs.push(home.join(".avm/bin"));
        dirs.push(home.join(".nvm/versions/node"));
    }
    // Homebrew common locations (Apple Silicon + Intel).
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.push(PathBuf::from("/usr/bin"));
    dirs
}

fn candidate_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    if cfg!(target_os = "windows") {
        for suffix in ["exe", "cmd", "bat"] {
            let named = dir.join(format!("{name}.{suffix}"));
            if named.is_file() {
                return Some(named);
            }
        }
    }
    None
}

fn run_version(path: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(path);
    cmd.args(args);
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let child = cmd.spawn().ok()?;
    let output = wait_with_timeout(child, Duration::from_secs(PROBE_TIMEOUT_SECS))?;

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    first_non_empty_line(&combined).map(|s| s.trim().to_string())
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Option<std::process::Output> {
    use std::thread;
    use std::time::Instant;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => return None,
        }
    }
    child.wait_with_output().ok()
}

fn first_non_empty_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

fn path_to_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_missing_tool_returns_absent() {
        let tool = probe_tool("this-binary-should-never-exist-xyz123", &["--version"]);
        assert!(!tool.present);
        assert!(tool.path.is_none());
        assert!(tool.version.is_none());
    }

    #[test]
    fn first_non_empty_line_skips_leading_whitespace() {
        assert_eq!(first_non_empty_line("\n\n  v1.2.3\nextra"), Some("v1.2.3"));
        assert_eq!(first_non_empty_line(""), None);
    }

    #[test]
    fn probe_returns_all_fields() {
        // We don't care whether any particular binary is present on the
        // CI host — just that the struct is populated and serializable.
        let status = probe();
        let json = serde_json::to_string(&status).expect("serializable");
        assert!(json.contains("\"solanaCli\""));
        assert!(json.contains("\"anchor\""));
        assert!(json.contains("\"rust\""));
        assert!(json.contains("\"node\""));
        if cfg!(target_os = "windows") {
            assert!(status.wsl2.is_some());
        } else {
            assert!(status.wsl2.is_none());
        }
    }

    #[test]
    fn has_minimum_for_localnet_requires_solana_cli() {
        let mut status = ToolchainStatus::default();
        assert!(!status.has_minimum_for_localnet());
        status.solana_cli.present = true;
        assert!(status.has_minimum_for_localnet());
    }
}
