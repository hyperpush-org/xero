//! Wallet scaffolds (Phase 8).
//!
//! Generates drop-in client-side wallet integrations. Matches the
//! `indexer::scaffold` contract: declarative request, deterministic
//! file set, sha256-hashed output, fail-closed on existing files
//! unless `overwrite=true`.
//!
//! Four flavours ship in this phase:
//!
//! - `WalletStandard` — the newer `@wallet-standard/react` + Mobile
//!   Wallet Standard path. Recommended for new projects.
//! - `Privy` — Privy's free-tier embedded-wallet flow (social login +
//!   external wallet connect).
//! - `Dynamic` — Dynamic's free-tier SDK with Solana connectors.
//! - `MwaStub` — desktop stub for Mobile Wallet Adapter with a "test
//!   on phone" checklist. MWA cannot run in a desktop app without a
//!   phone in the loop; the stub is a scaffold that points the
//!   developer at the right entry points.
//!
//! Each kind is a plain module that returns `Vec<(RelativePath, Bytes)>`.
//! The dispatcher writes them with `indexer::write_files`-style safety
//! (mkdir parents, fail on existing unless overwrite, sha256 every
//! byte). The toolchain gate is enforced by the command layer — we
//! require `node` + `pnpm` to be present before we'll scaffold, since
//! none of these scaffolds compile without them.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::toolchain::ToolchainStatus;
use crate::commands::{CommandError, CommandResult};

pub mod dynamic;
pub mod mwa;
pub mod privy;
pub mod wallet_standard;

/// Scaffold flavour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalletKind {
    WalletStandard,
    Privy,
    Dynamic,
    MwaStub,
}

impl WalletKind {
    pub const ALL: [WalletKind; 4] = [
        WalletKind::WalletStandard,
        WalletKind::Privy,
        WalletKind::Dynamic,
        WalletKind::MwaStub,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            WalletKind::WalletStandard => "wallet_standard",
            WalletKind::Privy => "privy",
            WalletKind::Dynamic => "dynamic",
            WalletKind::MwaStub => "mwa_stub",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            WalletKind::WalletStandard => "Wallet Standard",
            WalletKind::Privy => "Privy (free tier)",
            WalletKind::Dynamic => "Dynamic (free tier)",
            WalletKind::MwaStub => "Mobile Wallet Adapter (stub)",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            WalletKind::WalletStandard => {
                "Modern @wallet-standard/react + MWA provider. Recommended for new projects — smaller bundle, better mobile story."
            }
            WalletKind::Privy => {
                "Embedded-wallet flow via Privy's free tier. Social login + external wallet connect behind one SDK."
            }
            WalletKind::Dynamic => {
                "Dynamic's free-tier SDK with Solana connectors. Ships onboarding UI with minimal code."
            }
            WalletKind::MwaStub => {
                "Desktop stub + phone-testing checklist. MWA cannot auth on desktop without a companion phone — this scaffold sets up the pieces you need to test against a real device."
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WalletScaffoldRequest {
    pub kind: WalletKind,
    pub output_dir: String,
    #[serde(default)]
    pub project_slug: Option<String>,
    /// Default cluster the generated client should connect to.
    #[serde(default = "default_cluster")]
    pub cluster: ClusterKind,
    /// Optional RPC URL baked into the scaffold. When absent we use the
    /// cluster's default RPC URL.
    #[serde(default)]
    pub rpc_url: Option<String>,
    /// App name surfaced to the generated wallet client.
    #[serde(default)]
    pub app_name: Option<String>,
    /// Some providers require an app id / publishable key to run. The
    /// scaffold still writes placeholders when this is None so the
    /// developer can paste the key in later.
    #[serde(default)]
    pub app_id: Option<String>,
    /// Overwrite guard — mirrors the indexer scaffold contract.
    #[serde(default)]
    pub overwrite: bool,
}

fn default_cluster() -> ClusterKind {
    ClusterKind::Localnet
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WalletScaffoldFile {
    pub path: String,
    pub bytes_written: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WalletScaffoldResult {
    pub kind: WalletKind,
    pub root: String,
    pub project_slug: String,
    pub cluster: ClusterKind,
    pub rpc_url: String,
    pub app_name: String,
    pub files: Vec<WalletScaffoldFile>,
    pub entrypoint: Option<String>,
    pub run_hint: String,
    /// When the scaffold requires a paid provider key (Privy, Dynamic),
    /// this is the env variable name the developer must populate. None
    /// for purely free flavours.
    pub api_key_env: Option<String>,
    /// Human-readable next-step checklist (render as a bulleted list in
    /// the UI).
    pub next_steps: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WalletScaffoldContext {
    pub project_slug: String,
    pub app_name: String,
    pub cluster: ClusterKind,
    pub rpc_url: String,
    pub app_id: Option<String>,
}

/// Top-level entry used by the `solana_wallet_scaffold_generate` command.
pub fn generate(
    toolchain: &ToolchainStatus,
    request: &WalletScaffoldRequest,
) -> CommandResult<WalletScaffoldResult> {
    if !toolchain.node.present {
        return Err(CommandError::user_fixable(
            "solana_wallet_scaffold_requires_node",
            "Node 20+ is required to scaffold a wallet client — install Node and retry.",
        ));
    }
    if !toolchain.pnpm.present {
        return Err(CommandError::user_fixable(
            "solana_wallet_scaffold_requires_pnpm",
            "pnpm is required to scaffold a wallet client — install pnpm (npm i -g pnpm) and retry.",
        ));
    }
    if request.output_dir.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_wallet_scaffold_missing_output",
            "outputDir must be a non-empty filesystem path.",
        ));
    }
    let slug = request
        .project_slug
        .clone()
        .unwrap_or_else(|| default_slug(request.kind));
    let app_name = request
        .app_name
        .clone()
        .unwrap_or_else(|| format!("{} dapp", request.kind.label()));
    let rpc_url = request
        .rpc_url
        .clone()
        .unwrap_or_else(|| default_rpc_for(request.cluster));

    let ctx = WalletScaffoldContext {
        project_slug: slug.clone(),
        app_name: app_name.clone(),
        cluster: request.cluster,
        rpc_url: rpc_url.clone(),
        app_id: request.app_id.clone(),
    };

    let (files, meta) = match request.kind {
        WalletKind::WalletStandard => wallet_standard::render(&ctx),
        WalletKind::Privy => privy::render(&ctx),
        WalletKind::Dynamic => dynamic::render(&ctx),
        WalletKind::MwaStub => mwa::render(&ctx),
    };

    let root = PathBuf::from(&request.output_dir).join(&slug);
    let written = write_files(&root, &files, request.overwrite)?;

    Ok(WalletScaffoldResult {
        kind: request.kind,
        root: root.display().to_string(),
        project_slug: slug,
        cluster: request.cluster,
        rpc_url,
        app_name,
        files: written,
        entrypoint: meta.entrypoint,
        run_hint: format!(
            "cd {} && pnpm install && {}",
            root.display(),
            meta.start_command
        ),
        api_key_env: meta.api_key_env,
        next_steps: meta.next_steps,
    })
}

/// Descriptor shape exposed via `solana_wallet_scaffold_list`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WalletDescriptor {
    pub kind: WalletKind,
    pub label: String,
    pub summary: String,
    pub requires_api_key: bool,
    pub supported_clusters: Vec<ClusterKind>,
}

pub fn descriptors() -> Vec<WalletDescriptor> {
    WalletKind::ALL
        .iter()
        .map(|kind| WalletDescriptor {
            kind: *kind,
            label: kind.label().to_string(),
            summary: kind.summary().to_string(),
            requires_api_key: matches!(kind, WalletKind::Privy | WalletKind::Dynamic),
            supported_clusters: vec![
                ClusterKind::Localnet,
                ClusterKind::MainnetFork,
                ClusterKind::Devnet,
                ClusterKind::Mainnet,
            ],
        })
        .collect()
}

fn default_slug(kind: WalletKind) -> String {
    match kind {
        WalletKind::WalletStandard => "wallet-standard-app".to_string(),
        WalletKind::Privy => "privy-solana-app".to_string(),
        WalletKind::Dynamic => "dynamic-solana-app".to_string(),
        WalletKind::MwaStub => "mwa-solana-app".to_string(),
    }
}

fn default_rpc_for(cluster: ClusterKind) -> String {
    match cluster {
        ClusterKind::Localnet => "http://127.0.0.1:8899".into(),
        ClusterKind::MainnetFork => "http://127.0.0.1:8899".into(),
        ClusterKind::Devnet => "https://api.devnet.solana.com".into(),
        ClusterKind::Mainnet => "https://api.mainnet-beta.solana.com".into(),
    }
}

pub struct ScaffoldMeta {
    pub entrypoint: Option<String>,
    pub start_command: String,
    pub api_key_env: Option<String>,
    pub next_steps: Vec<String>,
}

fn write_files(
    root: &Path,
    files: &[(String, String)],
    overwrite: bool,
) -> CommandResult<Vec<WalletScaffoldFile>> {
    fs::create_dir_all(root).map_err(|err| {
        CommandError::system_fault(
            "solana_wallet_scaffold_mkdir_failed",
            format!("Could not create {}: {err}", root.display()),
        )
    })?;

    if !overwrite {
        for (rel, _) in files {
            let target = root.join(rel);
            if target.exists() {
                return Err(CommandError::user_fixable(
                    "solana_wallet_scaffold_output_exists",
                    format!(
                        "{} already exists — pass overwrite=true to replace it.",
                        target.display()
                    ),
                ));
            }
        }
    }

    // Guard against duplicate paths in the scaffold template itself.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (rel, _) in files {
        if !seen.insert(rel.as_str()) {
            return Err(CommandError::system_fault(
                "solana_wallet_scaffold_duplicate_path",
                format!("Scaffold attempted to write {rel} twice."),
            ));
        }
    }

    let mut written: Vec<WalletScaffoldFile> = Vec::with_capacity(files.len());
    for (rel, contents) in files {
        let target = root.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                CommandError::system_fault(
                    "solana_wallet_scaffold_mkdir_failed",
                    format!("Could not create {}: {err}", parent.display()),
                )
            })?;
        }
        fs::write(&target, contents.as_bytes()).map_err(|err| {
            CommandError::system_fault(
                "solana_wallet_scaffold_write_failed",
                format!("Could not write {}: {err}", target.display()),
            )
        })?;
        written.push(WalletScaffoldFile {
            path: rel.clone(),
            bytes_written: contents.len() as u64,
            sha256: sha256_hex(contents.as_bytes()),
        });
    }
    written.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(written)
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub(crate) fn escape_ts_string(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('"', r#"\""#)
        .replace('\n', r"\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::solana::toolchain::ToolProbe;
    use tempfile::TempDir;

    fn full_toolchain() -> ToolchainStatus {
        ToolchainStatus {
            node: ToolProbe {
                present: true,
                path: Some("/usr/local/bin/node".into()),
                version: Some("v20.11.1".into()),
            },
            pnpm: ToolProbe {
                present: true,
                path: Some("/usr/local/bin/pnpm".into()),
                version: Some("9.0.0".into()),
            },
            ..ToolchainStatus::default()
        }
    }

    fn base_request(kind: WalletKind, dir: &Path) -> WalletScaffoldRequest {
        WalletScaffoldRequest {
            kind,
            output_dir: dir.display().to_string(),
            project_slug: Some("scaffold-test".into()),
            cluster: ClusterKind::Devnet,
            rpc_url: None,
            app_name: Some("Demo Dapp".into()),
            app_id: Some("demo-app-id".into()),
            overwrite: false,
        }
    }

    #[test]
    fn missing_node_is_user_fixable() {
        let tmp = TempDir::new().unwrap();
        let toolchain = ToolchainStatus::default();
        let err = generate(
            &toolchain,
            &base_request(WalletKind::WalletStandard, tmp.path()),
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_wallet_scaffold_requires_node");
    }

    #[test]
    fn missing_pnpm_is_user_fixable() {
        let tmp = TempDir::new().unwrap();
        let mut toolchain = ToolchainStatus::default();
        toolchain.node.present = true;
        let err = generate(
            &toolchain,
            &base_request(WalletKind::WalletStandard, tmp.path()),
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_wallet_scaffold_requires_pnpm");
    }

    #[test]
    fn every_scaffold_writes_at_least_package_json_and_tsx_file() {
        let tmp = TempDir::new().unwrap();
        let toolchain = full_toolchain();
        for kind in WalletKind::ALL {
            // Give each kind its own sub-dir so existing-file guards don't fire.
            let sub = tmp.path().join(kind.as_str());
            std::fs::create_dir_all(&sub).unwrap();
            let result = generate(&toolchain, &base_request(kind, &sub)).unwrap();
            let paths: BTreeSet<_> = result.files.iter().map(|f| f.path.as_str()).collect();
            assert!(
                paths.contains("package.json"),
                "{:?} scaffold missing package.json",
                kind
            );
            assert!(
                paths
                    .iter()
                    .any(|p| p.ends_with(".tsx") || p.ends_with(".ts")),
                "{:?} scaffold must write at least one ts/tsx source file",
                kind
            );
            assert!(
                paths.contains("README.md"),
                "{:?} scaffold must write a README with next-step instructions",
                kind
            );
        }
    }

    #[test]
    fn scaffold_rejects_overwriting_existing_without_flag() {
        let tmp = TempDir::new().unwrap();
        let toolchain = full_toolchain();
        let request = base_request(WalletKind::WalletStandard, tmp.path());
        generate(&toolchain, &request).unwrap();
        let err = generate(&toolchain, &request).unwrap_err();
        assert_eq!(err.code, "solana_wallet_scaffold_output_exists");
    }

    #[test]
    fn overwrite_flag_replaces_existing() {
        let tmp = TempDir::new().unwrap();
        let toolchain = full_toolchain();
        let request = base_request(WalletKind::WalletStandard, tmp.path());
        generate(&toolchain, &request).unwrap();
        let mut second = request.clone();
        second.overwrite = true;
        generate(&toolchain, &second).unwrap();
    }

    #[test]
    fn privy_scaffold_includes_api_key_env() {
        let tmp = TempDir::new().unwrap();
        let toolchain = full_toolchain();
        let result = generate(&toolchain, &base_request(WalletKind::Privy, tmp.path())).unwrap();
        assert_eq!(result.api_key_env.as_deref(), Some("PRIVY_APP_ID"));
    }

    #[test]
    fn dynamic_scaffold_includes_api_key_env() {
        let tmp = TempDir::new().unwrap();
        let toolchain = full_toolchain();
        let result = generate(&toolchain, &base_request(WalletKind::Dynamic, tmp.path())).unwrap();
        assert_eq!(
            result.api_key_env.as_deref(),
            Some("DYNAMIC_ENVIRONMENT_ID")
        );
    }

    #[test]
    fn wallet_standard_scaffold_does_not_require_api_key() {
        let tmp = TempDir::new().unwrap();
        let toolchain = full_toolchain();
        let result = generate(
            &toolchain,
            &base_request(WalletKind::WalletStandard, tmp.path()),
        )
        .unwrap();
        assert!(result.api_key_env.is_none());
    }

    #[test]
    fn mwa_stub_next_steps_contain_phone_testing_instructions() {
        let tmp = TempDir::new().unwrap();
        let toolchain = full_toolchain();
        let result = generate(&toolchain, &base_request(WalletKind::MwaStub, tmp.path())).unwrap();
        let joined = result.next_steps.join("\n").to_lowercase();
        assert!(
            joined.contains("phone") || joined.contains("mobile"),
            "MWA stub next-step checklist must mention phone / mobile testing"
        );
    }

    #[test]
    fn scaffold_output_is_deterministic_for_identical_input() {
        let tmp_a = TempDir::new().unwrap();
        let tmp_b = TempDir::new().unwrap();
        let toolchain = full_toolchain();
        let req_a = base_request(WalletKind::WalletStandard, tmp_a.path());
        let req_b = base_request(WalletKind::WalletStandard, tmp_b.path());
        let a = generate(&toolchain, &req_a).unwrap();
        let b = generate(&toolchain, &req_b).unwrap();
        let hashes_a: Vec<_> = a.files.iter().map(|f| f.sha256.clone()).collect();
        let hashes_b: Vec<_> = b.files.iter().map(|f| f.sha256.clone()).collect();
        assert_eq!(hashes_a, hashes_b);
    }

    #[test]
    fn descriptors_expose_every_kind_once() {
        let descs = descriptors();
        let seen: BTreeSet<_> = descs.iter().map(|d| d.kind).collect();
        assert_eq!(seen.len(), WalletKind::ALL.len());
        for kind in WalletKind::ALL {
            let d = descs.iter().find(|d| d.kind == kind).unwrap();
            assert!(!d.label.is_empty());
            assert!(!d.summary.is_empty());
        }
    }
}
