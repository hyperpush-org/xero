use std::{
    collections::VecDeque,
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    global_db::environment_profile::{
        validate_environment_payload, validate_environment_summary, EnvironmentCapability,
        EnvironmentCapabilityState, EnvironmentDiagnostic, EnvironmentDiagnosticSeverity,
        EnvironmentPathProfile, EnvironmentPlatform, EnvironmentProfilePayload,
        EnvironmentProfileStatus, EnvironmentProfileSummary, EnvironmentProfileValidationError,
        EnvironmentToolCategory, EnvironmentToolProbeStatus, EnvironmentToolRecord,
        EnvironmentToolSource, EnvironmentToolSummary, ENVIRONMENT_PROFILE_SCHEMA_VERSION,
    },
    global_db::user_added_tools::UserAddedToolRow,
    runtime::redaction::find_prohibited_persistence_content,
};

const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_PROBE_CONCURRENCY: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentProbeReport {
    pub status: EnvironmentProfileStatus,
    pub payload: EnvironmentProfilePayload,
    pub summary: EnvironmentProfileSummary,
    pub started_at: String,
    pub completed_at: String,
}

pub type EnvironmentProbeResult<T> = Result<T, EnvironmentProfileValidationError>;

#[derive(Debug, Clone)]
pub struct EnvironmentProbeOptions {
    pub timeout: Duration,
    pub concurrency: usize,
}

impl Default for EnvironmentProbeOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_PROBE_TIMEOUT,
            concurrency: DEFAULT_PROBE_CONCURRENCY,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentProbeCatalogEntry {
    pub id: String,
    pub category: EnvironmentToolCategory,
    pub command: String,
    pub args: Vec<String>,
    pub custom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnvironmentBinary {
    pub path: PathBuf,
    pub source: EnvironmentToolSource,
}

pub trait EnvironmentBinaryResolver: Send + Sync {
    fn resolve(&self, command: &str) -> Option<ResolvedEnvironmentBinary>;
    fn path_profile(&self) -> EnvironmentPathProfile;
    fn child_envs(&self) -> Vec<(OsString, OsString)>;
}

#[derive(Debug, Clone)]
pub enum EnvironmentCommandExecution {
    Completed {
        success: bool,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    Timeout,
    SpawnFailed(String),
}

pub trait EnvironmentCommandExecutor: Send + Sync {
    fn run(
        &self,
        binary: &Path,
        args: &[String],
        timeout: Duration,
        child_envs: &[(OsString, OsString)],
    ) -> EnvironmentCommandExecution;
}

#[derive(Debug, Clone)]
pub struct SystemEnvironmentBinaryResolver {
    bundled_dirs: Vec<PathBuf>,
    managed_dirs: Vec<PathBuf>,
    path_dirs: Vec<PathBuf>,
    common_dirs: Vec<PathBuf>,
}

impl SystemEnvironmentBinaryResolver {
    pub fn from_process() -> Self {
        let bundled_dirs = bundled_tool_dirs();
        let managed_dirs = managed_tool_dirs();
        let path_dirs = env::var_os("PATH")
            .map(|value| env::split_paths(&value).collect())
            .unwrap_or_default();
        let common_dirs = common_dev_dirs();

        Self {
            bundled_dirs,
            managed_dirs,
            path_dirs,
            common_dirs,
        }
    }

    fn ordered_dirs(&self) -> Vec<(&Path, EnvironmentToolSource)> {
        self.bundled_dirs
            .iter()
            .map(|path| (path.as_path(), EnvironmentToolSource::BundledToolchain))
            .chain(
                self.managed_dirs
                    .iter()
                    .map(|path| (path.as_path(), EnvironmentToolSource::ManagedToolchain)),
            )
            .chain(
                self.path_dirs
                    .iter()
                    .map(|path| (path.as_path(), EnvironmentToolSource::Path)),
            )
            .chain(
                self.common_dirs
                    .iter()
                    .map(|path| (path.as_path(), EnvironmentToolSource::CommonDevDir)),
            )
            .collect()
    }
}

impl EnvironmentBinaryResolver for SystemEnvironmentBinaryResolver {
    fn resolve(&self, command: &str) -> Option<ResolvedEnvironmentBinary> {
        if command.trim().is_empty() {
            return None;
        }

        if looks_like_path(command) {
            let path = PathBuf::from(command);
            return path.is_file().then_some(ResolvedEnvironmentBinary {
                path,
                source: EnvironmentToolSource::Path,
            });
        }

        for (dir, source) in self.ordered_dirs() {
            if let Some(path) = candidate_in_dir(dir, command) {
                return Some(ResolvedEnvironmentBinary { path, source });
            }
        }

        None
    }

    fn path_profile(&self) -> EnvironmentPathProfile {
        let dirs = self.ordered_dirs();
        let normalized = normalized_path_entries(dirs.iter().map(|(path, _)| *path));
        let mut sources = Vec::new();
        if !self.bundled_dirs.is_empty() {
            sources.push("bundled-toolchain".to_string());
        }
        if !self.managed_dirs.is_empty() {
            sources.push("managed-toolchain".to_string());
        }
        if !self.path_dirs.is_empty() {
            sources.push("tauri-process-path".to_string());
        }
        if !self.common_dirs.is_empty() {
            sources.push("common-dev-dirs".to_string());
        }

        EnvironmentPathProfile {
            entry_count: normalized.len(),
            fingerprint: Some(path_fingerprint(&normalized)),
            sources,
        }
    }

    fn child_envs(&self) -> Vec<(OsString, OsString)> {
        let mut paths = self
            .bundled_dirs
            .iter()
            .chain(self.managed_dirs.iter())
            .filter(|path| path.is_dir())
            .cloned()
            .collect::<Vec<_>>();

        paths.extend(self.path_dirs.iter().cloned());
        env::join_paths(paths)
            .ok()
            .map(|path| vec![(OsString::from("PATH"), path)])
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SystemEnvironmentCommandExecutor;

impl EnvironmentCommandExecutor for SystemEnvironmentCommandExecutor {
    fn run(
        &self,
        binary: &Path,
        args: &[String],
        timeout: Duration,
        child_envs: &[(OsString, OsString)],
    ) -> EnvironmentCommandExecution {
        let mut command = Command::new(binary);
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        for (key, value) in child_envs {
            command.env(key, value);
        }

        let child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return EnvironmentCommandExecution::SpawnFailed(error.to_string()),
        };

        match wait_with_timeout(child, timeout) {
            Some(output) => EnvironmentCommandExecution::Completed {
                success: output.status.success(),
                stdout: output.stdout,
                stderr: output.stderr,
            },
            None => EnvironmentCommandExecution::Timeout,
        }
    }
}

pub fn probe_environment_profile() -> EnvironmentProbeResult<EnvironmentProbeReport> {
    probe_environment_profile_with_user_tools(vec![])
}

pub fn probe_environment_profile_with_user_tools(
    user_entries: Vec<UserAddedToolRow>,
) -> EnvironmentProbeResult<EnvironmentProbeReport> {
    let resolver = Arc::new(SystemEnvironmentBinaryResolver::from_process());
    let executor = Arc::new(SystemEnvironmentCommandExecutor);
    probe_environment_profile_with(
        merged_environment_probe_catalog(user_entries),
        resolver,
        executor,
        EnvironmentProbeOptions::default(),
    )
}

pub fn probe_environment_profile_with(
    catalog: Vec<EnvironmentProbeCatalogEntry>,
    resolver: Arc<dyn EnvironmentBinaryResolver>,
    executor: Arc<dyn EnvironmentCommandExecutor>,
    options: EnvironmentProbeOptions,
) -> EnvironmentProbeResult<EnvironmentProbeReport> {
    let started_at = now_timestamp();
    let platform = current_platform();
    let path_profile = resolver.path_profile();
    let tool_records = run_catalog(catalog, resolver, executor, options);
    let capabilities = derive_capabilities(&tool_records);
    let diagnostics = collect_diagnostics(&tool_records);
    let status = profile_status(&tool_records);
    let completed_at = now_timestamp();

    let payload = EnvironmentProfilePayload {
        schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
        platform: platform.clone(),
        path: path_profile,
        tools: tool_records.clone(),
        capabilities: capabilities.clone(),
        permissions: vec![],
        diagnostics: diagnostics.clone(),
    };
    validate_environment_payload(&payload)?;

    let summary = EnvironmentProfileSummary {
        schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
        status,
        platform,
        refreshed_at: Some(completed_at.clone()),
        tools: tool_records.iter().map(tool_summary).collect(),
        capabilities,
        permission_requests: vec![],
        diagnostics,
    };
    validate_environment_summary(&summary)?;

    Ok(EnvironmentProbeReport {
        status,
        payload,
        summary,
        started_at,
        completed_at,
    })
}

pub fn environment_probe_catalog() -> Vec<EnvironmentProbeCatalogEntry> {
    built_in_environment_probe_catalog()
}

pub fn built_in_environment_probe_catalog() -> Vec<EnvironmentProbeCatalogEntry> {
    use EnvironmentToolCategory::*;

    let mut entries = vec![
        // Base developer tools
        entry("git", BaseDeveloperTool, "git", &["--version"]),
        entry("git_lfs", BaseDeveloperTool, "git-lfs", &["--version"]),
        entry("hub", BaseDeveloperTool, "hub", &["--version"]),
        entry("ssh", BaseDeveloperTool, "ssh", &["-V"]),
        entry("gh", BaseDeveloperTool, "gh", &["--version"]),
        entry("glab", BaseDeveloperTool, "glab", &["--version"]),
        entry("protoc", BaseDeveloperTool, "protoc", &["--version"]),
        entry("make", BaseDeveloperTool, "make", &["--version"]),
        entry("cmake", BaseDeveloperTool, "cmake", &["--version"]),
        entry(
            "pkg_config",
            BaseDeveloperTool,
            "pkg-config",
            &["--version"],
        ),
        entry("openssl", BaseDeveloperTool, "openssl", &["version"]),
        entry("curl", BaseDeveloperTool, "curl", &["--version"]),
        entry("wget", BaseDeveloperTool, "wget", &["--version"]),
        // Package managers (cross-language)
        entry("pnpm", PackageManager, "pnpm", &["--version"]),
        entry("npm", PackageManager, "npm", &["--version"]),
        entry("yarn", PackageManager, "yarn", &["--version"]),
        entry("bun", PackageManager, "bun", &["--version"]),
        entry("deno", PackageManager, "deno", &["--version"]),
        entry("uv", PackageManager, "uv", &["--version"]),
        entry("pipx", PackageManager, "pipx", &["--version"]),
        entry("pip", PackageManager, "pip", &["--version"]),
        entry("pip3", PackageManager, "pip3", &["--version"]),
        entry("poetry", PackageManager, "poetry", &["--version"]),
        entry("conda", PackageManager, "conda", &["--version"]),
        entry("mamba", PackageManager, "mamba", &["--version"]),
        entry("brew", PackageManager, "brew", &["--version"]),
        entry("gem", PackageManager, "gem", &["--version"]),
        entry("bundler", PackageManager, "bundle", &["--version"]),
        entry("composer", PackageManager, "composer", &["--version"]),
        entry("cocoapods", PackageManager, "pod", &["--version"]),
        entry("nuget", PackageManager, "nuget", &["help"]),
        // Language runtimes
        entry("node", LanguageRuntime, "node", &["--version"]),
        entry("python", LanguageRuntime, "python", &["--version"]),
        entry("python3", LanguageRuntime, "python3", &["--version"]),
        entry("rustc", LanguageRuntime, "rustc", &["--version"]),
        entry("cargo", LanguageRuntime, "cargo", &["--version"]),
        entry("rustup", LanguageRuntime, "rustup", &["--version"]),
        entry("go", LanguageRuntime, "go", &["version"]),
        entry("java", LanguageRuntime, "java", &["-version"]),
        entry("javac", LanguageRuntime, "javac", &["-version"]),
        entry("kotlin", LanguageRuntime, "kotlin", &["-version"]),
        entry("kotlinc", LanguageRuntime, "kotlinc", &["-version"]),
        entry("scala", LanguageRuntime, "scala", &["-version"]),
        entry("swift", LanguageRuntime, "swift", &["--version"]),
        entry("ruby", LanguageRuntime, "ruby", &["--version"]),
        entry("php", LanguageRuntime, "php", &["--version"]),
        entry("dotnet", LanguageRuntime, "dotnet", &["--version"]),
        entry("zig", LanguageRuntime, "zig", &["version"]),
        entry("dart", LanguageRuntime, "dart", &["--version"]),
        entry("elixir", LanguageRuntime, "elixir", &["--version"]),
        entry("erl", LanguageRuntime, "erl", &["-version"]),
        entry("ghc", LanguageRuntime, "ghc", &["--version"]),
        entry("ocaml", LanguageRuntime, "ocaml", &["-version"]),
        entry("lua", LanguageRuntime, "lua", &["-v"]),
        entry("perl", LanguageRuntime, "perl", &["-v"]),
        entry("julia", LanguageRuntime, "julia", &["--version"]),
        entry("crystal", LanguageRuntime, "crystal", &["--version"]),
        entry("nim", LanguageRuntime, "nim", &["--version"]),
        entry("clojure", LanguageRuntime, "clojure", &["--version"]),
        entry("r", LanguageRuntime, "R", &["--version"]),
        // Editors
        entry("vim", Editor, "vim", &["--version"]),
        entry("nvim", Editor, "nvim", &["--version"]),
        entry("emacs", Editor, "emacs", &["--version"]),
        entry("nano", Editor, "nano", &["--version"]),
        entry("code", Editor, "code", &["--version"]),
        entry("code_insiders", Editor, "code-insiders", &["--version"]),
        entry("cursor", Editor, "cursor", &["--version"]),
        entry("subl", Editor, "subl", &["--version"]),
        entry("hx", Editor, "hx", &["--version"]),
        entry("micro", Editor, "micro", &["--version"]),
        entry("zed", Editor, "zed", &["--version"]),
        // Build tools
        entry("gradle", BuildTool, "gradle", &["--version"]),
        entry("maven", BuildTool, "mvn", &["--version"]),
        entry("bazel", BuildTool, "bazel", &["--version"]),
        entry("sbt", BuildTool, "sbt", &["--version"]),
        entry("ninja", BuildTool, "ninja", &["--version"]),
        entry("ant", BuildTool, "ant", &["-version"]),
        entry("meson", BuildTool, "meson", &["--version"]),
        entry("scons", BuildTool, "scons", &["--version"]),
        entry("buck2", BuildTool, "buck2", &["--version"]),
        entry("just", BuildTool, "just", &["--version"]),
        entry("task", BuildTool, "task", &["--version"]),
        // Linters / formatters
        entry("rustfmt", Linter, "rustfmt", &["--version"]),
        entry("clippy", Linter, "cargo-clippy", &["--version"]),
        entry("eslint", Linter, "eslint", &["--version"]),
        entry("prettier", Linter, "prettier", &["--version"]),
        entry("biome", Linter, "biome", &["--version"]),
        entry("ruff", Linter, "ruff", &["--version"]),
        entry("black", Linter, "black", &["--version"]),
        entry("flake8", Linter, "flake8", &["--version"]),
        entry("mypy", Linter, "mypy", &["--version"]),
        entry("pylint", Linter, "pylint", &["--version"]),
        entry("pytest", Linter, "pytest", &["--version"]),
        entry("golangci_lint", Linter, "golangci-lint", &["--version"]),
        entry("shellcheck", Linter, "shellcheck", &["--version"]),
        entry("shfmt", Linter, "shfmt", &["--version"]),
        entry("hadolint", Linter, "hadolint", &["--version"]),
        entry("tsc", Linter, "tsc", &["--version"]),
        // Version managers
        entry("asdf", VersionManager, "asdf", &["--version"]),
        entry("mise", VersionManager, "mise", &["--version"]),
        entry("nvm", VersionManager, "nvm", &["--version"]),
        entry("fnm", VersionManager, "fnm", &["--version"]),
        entry("volta", VersionManager, "volta", &["--version"]),
        entry("pyenv", VersionManager, "pyenv", &["--version"]),
        entry("rbenv", VersionManager, "rbenv", &["--version"]),
        entry("jenv", VersionManager, "jenv", &["--version"]),
        entry("sdkman", VersionManager, "sdk", &["version"]),
        entry("nodenv", VersionManager, "nodenv", &["--version"]),
        // Infrastructure / IaC
        entry("terraform", IacTool, "terraform", &["--version"]),
        entry("tofu", IacTool, "tofu", &["--version"]),
        entry("ansible", IacTool, "ansible", &["--version"]),
        entry("pulumi", IacTool, "pulumi", &["version"]),
        entry("packer", IacTool, "packer", &["--version"]),
        entry("vault", IacTool, "vault", &["--version"]),
        entry("consul", IacTool, "consul", &["--version"]),
        entry("nomad", IacTool, "nomad", &["--version"]),
        entry("helmfile", IacTool, "helmfile", &["--version"]),
        entry("terragrunt", IacTool, "terragrunt", &["--version"]),
        // Shell utilities
        entry("jq", ShellUtility, "jq", &["--version"]),
        entry("yq", ShellUtility, "yq", &["--version"]),
        entry("rg", ShellUtility, "rg", &["--version"]),
        entry("fd", ShellUtility, "fd", &["--version"]),
        entry("bat", ShellUtility, "bat", &["--version"]),
        entry("fzf", ShellUtility, "fzf", &["--version"]),
        entry("lazygit", ShellUtility, "lazygit", &["--version"]),
        entry("tig", ShellUtility, "tig", &["--version"]),
        entry("tmux", ShellUtility, "tmux", &["-V"]),
        entry("zellij", ShellUtility, "zellij", &["--version"]),
        entry("direnv", ShellUtility, "direnv", &["--version"]),
        entry("htop", ShellUtility, "htop", &["--version"]),
        entry("btop", ShellUtility, "btop", &["--version"]),
        entry("watchman", ShellUtility, "watchman", &["--version"]),
        entry("ngrok", ShellUtility, "ngrok", &["--version"]),
        entry("tldr", ShellUtility, "tldr", &["--version"]),
        entry("entr", ShellUtility, "entr", &["-h"]),
        entry("delta", ShellUtility, "delta", &["--version"]),
        // Container / orchestration
        entry("docker", ContainerOrchestration, "docker", &["--version"]),
        entry(
            "docker_compose",
            ContainerOrchestration,
            "docker",
            &["compose", "version"],
        ),
        entry("podman", ContainerOrchestration, "podman", &["--version"]),
        entry("colima", ContainerOrchestration, "colima", &["version"]),
        entry("lima", ContainerOrchestration, "limactl", &["--version"]),
        entry(
            "kubectl",
            ContainerOrchestration,
            "kubectl",
            &["version", "--client=true"],
        ),
        entry(
            "helm",
            ContainerOrchestration,
            "helm",
            &["version", "--short"],
        ),
        entry("minikube", ContainerOrchestration, "minikube", &["version"]),
        entry("kind", ContainerOrchestration, "kind", &["version"]),
        entry("k3d", ContainerOrchestration, "k3d", &["version"]),
        entry("k9s", ContainerOrchestration, "k9s", &["version"]),
        entry("skaffold", ContainerOrchestration, "skaffold", &["version"]),
        entry(
            "lazydocker",
            ContainerOrchestration,
            "lazydocker",
            &["--version"],
        ),
        // Mobile tooling
        entry("xcodebuild", MobileTooling, "xcodebuild", &["-version"]),
        entry("xcrun", MobileTooling, "xcrun", &["--version"]),
        entry("adb", MobileTooling, "adb", &["version"]),
        entry("emulator", MobileTooling, "emulator", &["-version"]),
        entry("flutter", MobileTooling, "flutter", &["--version"]),
        entry("fastlane", MobileTooling, "fastlane", &["--version"]),
        entry("expo", MobileTooling, "expo", &["--version"]),
        entry("eas", MobileTooling, "eas", &["--version"]),
        // Cloud deployment
        entry("aws", CloudDeployment, "aws", &["--version"]),
        entry("gcloud", CloudDeployment, "gcloud", &["--version"]),
        entry("az", CloudDeployment, "az", &["version"]),
        entry("flyctl", CloudDeployment, "flyctl", &["version"]),
        entry("vercel", CloudDeployment, "vercel", &["--version"]),
        entry("netlify", CloudDeployment, "netlify", &["--version"]),
        entry("heroku", CloudDeployment, "heroku", &["--version"]),
        entry("doctl", CloudDeployment, "doctl", &["version"]),
        entry("railway", CloudDeployment, "railway", &["--version"]),
        entry("render", CloudDeployment, "render", &["--version"]),
        entry("supabase", CloudDeployment, "supabase", &["--version"]),
        entry("firebase", CloudDeployment, "firebase", &["--version"]),
        entry(
            "cloudflare_wrangler",
            CloudDeployment,
            "wrangler",
            &["--version"],
        ),
        // Database CLIs
        entry("sqlite3", DatabaseCli, "sqlite3", &["--version"]),
        entry("psql", DatabaseCli, "psql", &["--version"]),
        entry("pg_dump", DatabaseCli, "pg_dump", &["--version"]),
        entry("mysql", DatabaseCli, "mysql", &["--version"]),
        entry("mongosh", DatabaseCli, "mongosh", &["--version"]),
        entry("mongo", DatabaseCli, "mongo", &["--version"]),
        entry("redis_cli", DatabaseCli, "redis-cli", &["--version"]),
        entry("influx", DatabaseCli, "influx", &["version"]),
        entry(
            "clickhouse_client",
            DatabaseCli,
            "clickhouse-client",
            &["--version"],
        ),
        entry("duckdb", DatabaseCli, "duckdb", &["--version"]),
        // Solana tooling
        entry("solana", SolanaTooling, "solana", &["--version"]),
        entry("anchor", SolanaTooling, "anchor", &["--version"]),
        entry(
            "cargo_build_sbf",
            SolanaTooling,
            "cargo-build-sbf",
            &["--version"],
        ),
        entry("spl_token", SolanaTooling, "spl-token", &["--version"]),
        entry("surfpool", SolanaTooling, "surfpool", &["--version"]),
        entry("trident", SolanaTooling, "trident", &["--version"]),
        entry("codama", SolanaTooling, "codama", &["--version"]),
        entry(
            "solana_verify",
            SolanaTooling,
            "solana-verify",
            &["--version"],
        ),
        // AI / agent CLIs
        entry("codex", AgentAiCli, "codex", &["--version"]),
        entry("claude", AgentAiCli, "claude", &["--version"]),
        entry("opencode", AgentAiCli, "opencode", &["--version"]),
        entry("aider", AgentAiCli, "aider", &["--version"]),
        entry("gemini", AgentAiCli, "gemini", &["--version"]),
        entry("ollama", AgentAiCli, "ollama", &["--version"]),
        entry("llm", AgentAiCli, "llm", &["--version"]),
        entry("continue", AgentAiCli, "continue", &["--version"]),
    ];

    entries.extend(platform_package_manager_entries());
    entries
}

pub fn merged_environment_probe_catalog(
    user_entries: Vec<UserAddedToolRow>,
) -> Vec<EnvironmentProbeCatalogEntry> {
    let mut entries = built_in_environment_probe_catalog();
    entries.extend(user_entries.into_iter().map(user_tool_entry));
    entries
}

fn user_tool_entry(row: UserAddedToolRow) -> EnvironmentProbeCatalogEntry {
    EnvironmentProbeCatalogEntry {
        id: row.id,
        category: row.category,
        command: row.command,
        args: row.args,
        custom: true,
    }
}

fn entry(
    id: &'static str,
    category: EnvironmentToolCategory,
    command: &'static str,
    args: &'static [&'static str],
) -> EnvironmentProbeCatalogEntry {
    EnvironmentProbeCatalogEntry {
        id: id.into(),
        category,
        command: command.into(),
        args: args.iter().map(|arg| (*arg).into()).collect(),
        custom: false,
    }
}

fn platform_package_manager_entries() -> Vec<EnvironmentProbeCatalogEntry> {
    use EnvironmentToolCategory::PlatformPackageManager;

    if cfg!(target_os = "macos") {
        vec![]
    } else if cfg!(target_os = "windows") {
        vec![
            entry("winget", PlatformPackageManager, "winget", &["--version"]),
            entry("choco", PlatformPackageManager, "choco", &["--version"]),
            entry("scoop", PlatformPackageManager, "scoop", &["--version"]),
        ]
    } else {
        vec![
            entry("apt", PlatformPackageManager, "apt", &["--version"]),
            entry("dnf", PlatformPackageManager, "dnf", &["--version"]),
            entry("pacman", PlatformPackageManager, "pacman", &["--version"]),
        ]
    }
}

fn run_catalog(
    catalog: Vec<EnvironmentProbeCatalogEntry>,
    resolver: Arc<dyn EnvironmentBinaryResolver>,
    executor: Arc<dyn EnvironmentCommandExecutor>,
    options: EnvironmentProbeOptions,
) -> Vec<EnvironmentToolRecord> {
    if catalog.is_empty() {
        return vec![];
    }

    let concurrency = options.concurrency.clamp(1, catalog.len());
    let queue = Arc::new(Mutex::new(
        catalog.into_iter().enumerate().collect::<VecDeque<_>>(),
    ));
    let (tx, rx) = mpsc::channel();

    for _ in 0..concurrency {
        let queue = Arc::clone(&queue);
        let tx = tx.clone();
        let resolver = Arc::clone(&resolver);
        let executor = Arc::clone(&executor);
        let options = options.clone();
        thread::spawn(move || loop {
            let next = queue.lock().expect("probe queue lock").pop_front();
            let Some((index, entry)) = next else {
                break;
            };
            let record = run_catalog_entry(&entry, &*resolver, &*executor, &options);
            if tx.send((index, record)).is_err() {
                break;
            }
        });
    }
    drop(tx);

    let mut records = rx.into_iter().collect::<Vec<_>>();
    records.sort_by_key(|(index, _)| *index);
    records.into_iter().map(|(_, record)| record).collect()
}

fn run_catalog_entry(
    entry: &EnvironmentProbeCatalogEntry,
    resolver: &dyn EnvironmentBinaryResolver,
    executor: &dyn EnvironmentCommandExecutor,
    options: &EnvironmentProbeOptions,
) -> EnvironmentToolRecord {
    let Some(resolved) = resolver.resolve(&entry.command) else {
        return EnvironmentToolRecord {
            id: entry.id.clone(),
            category: entry.category,
            command: entry.command.clone(),
            custom: entry.custom,
            present: false,
            path: None,
            version: None,
            source: EnvironmentToolSource::Unresolved,
            probe_status: EnvironmentToolProbeStatus::Missing,
            duration_ms: None,
        };
    };

    let started = Instant::now();
    let execution = executor.run(
        &resolved.path,
        &entry.args,
        options.timeout,
        &resolver.child_envs(),
    );
    let duration_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);

    let (probe_status, version) = match execution {
        EnvironmentCommandExecution::Completed {
            success,
            stdout,
            stderr,
        } => {
            let line = first_useful_line(&stdout, &stderr);
            let redacted = line
                .as_deref()
                .is_some_and(|line| find_prohibited_persistence_content(line).is_some());
            let safe_line = line.and_then(sanitize_version_line);
            let status = if redacted {
                EnvironmentToolProbeStatus::Failed
            } else if safe_line.is_some() || success {
                EnvironmentToolProbeStatus::Ok
            } else {
                EnvironmentToolProbeStatus::Failed
            };
            (status, safe_line)
        }
        EnvironmentCommandExecution::Timeout => (EnvironmentToolProbeStatus::Timeout, None),
        EnvironmentCommandExecution::SpawnFailed(_) => (EnvironmentToolProbeStatus::Failed, None),
    };

    EnvironmentToolRecord {
        id: entry.id.clone(),
        category: entry.category,
        command: entry.command.clone(),
        custom: entry.custom,
        present: true,
        path: safe_persisted_path(&resolved.path),
        version,
        source: resolved.source,
        probe_status,
        duration_ms: Some(duration_ms),
    }
}

fn collect_diagnostics(tools: &[EnvironmentToolRecord]) -> Vec<EnvironmentDiagnostic> {
    tools
        .iter()
        .filter_map(|tool| match tool.probe_status {
            EnvironmentToolProbeStatus::Timeout => Some(EnvironmentDiagnostic {
                code: "environment_probe_timeout".into(),
                severity: EnvironmentDiagnosticSeverity::Warning,
                message: format!("{} version probe timed out.", tool.id),
                retryable: true,
                tool_id: Some(tool.id.clone()),
            }),
            EnvironmentToolProbeStatus::Failed => Some(EnvironmentDiagnostic {
                code: "environment_probe_failed".into(),
                severity: EnvironmentDiagnosticSeverity::Warning,
                message: format!("{} version probe did not return usable output.", tool.id),
                retryable: true,
                tool_id: Some(tool.id.clone()),
            }),
            _ => None,
        })
        .collect()
}

fn profile_status(tools: &[EnvironmentToolRecord]) -> EnvironmentProfileStatus {
    if tools.iter().any(|tool| {
        matches!(
            tool.probe_status,
            EnvironmentToolProbeStatus::Timeout | EnvironmentToolProbeStatus::Failed
        )
    }) {
        EnvironmentProfileStatus::Partial
    } else {
        EnvironmentProfileStatus::Ready
    }
}

fn derive_capabilities(tools: &[EnvironmentToolRecord]) -> Vec<EnvironmentCapability> {
    let present = tools
        .iter()
        .filter(|tool| tool.present && tool.probe_status == EnvironmentToolProbeStatus::Ok)
        .map(|tool| tool.id.as_str())
        .collect::<std::collections::HashSet<_>>();

    let package_managers = ["pnpm", "npm", "yarn", "bun"];
    vec![
        capability_any(
            "node_project_ready",
            &present,
            &["node"],
            &package_managers,
            "Node is present, but no JavaScript package manager was found.",
        ),
        capability_all(
            "rust_project_ready",
            &present,
            &["rustc", "cargo"],
            "Rust requires both rustc and cargo.",
        ),
        capability_tauri(&present),
        capability_all(
            "docker_available",
            &present,
            &["docker"],
            "Docker CLI was not found.",
        ),
        capability_all(
            "ios_simulator_available",
            &present,
            &["xcodebuild", "xcrun"],
            "iOS simulator tooling requires xcodebuild and xcrun.",
        ),
        capability_all(
            "android_emulator_available",
            &present,
            &["adb", "emulator"],
            "Android emulator tooling requires adb and emulator.",
        ),
        capability_all(
            "solana_localnet_ready",
            &present,
            &["solana", "cargo_build_sbf", "spl_token"],
            "Solana localnet workflows require solana, cargo-build-sbf, and spl-token.",
        ),
        capability_all(
            "protobuf_build_ready",
            &present,
            &["protoc"],
            "Protocol Buffer builds require protoc.",
        ),
    ]
}

fn capability_all(
    id: &str,
    present: &std::collections::HashSet<&str>,
    required: &[&str],
    partial_message: &str,
) -> EnvironmentCapability {
    let evidence = required
        .iter()
        .filter(|tool| present.contains(**tool))
        .map(|tool| (*tool).to_string())
        .collect::<Vec<_>>();
    let state = if evidence.len() == required.len() {
        EnvironmentCapabilityState::Ready
    } else if evidence.is_empty() {
        EnvironmentCapabilityState::Missing
    } else {
        EnvironmentCapabilityState::Partial
    };

    EnvironmentCapability {
        id: id.into(),
        state,
        evidence,
        message: (state != EnvironmentCapabilityState::Ready).then(|| partial_message.into()),
    }
}

fn capability_any(
    id: &str,
    present: &std::collections::HashSet<&str>,
    required_all: &[&str],
    required_any: &[&str],
    partial_message: &str,
) -> EnvironmentCapability {
    let mut evidence = required_all
        .iter()
        .chain(required_any.iter())
        .filter(|tool| present.contains(**tool))
        .map(|tool| (*tool).to_string())
        .collect::<Vec<_>>();
    evidence.sort();
    evidence.dedup();

    let has_all = required_all.iter().all(|tool| present.contains(*tool));
    let has_any = required_any.iter().any(|tool| present.contains(*tool));
    let state = if has_all && has_any {
        EnvironmentCapabilityState::Ready
    } else if evidence.is_empty() {
        EnvironmentCapabilityState::Missing
    } else {
        EnvironmentCapabilityState::Partial
    };

    EnvironmentCapability {
        id: id.into(),
        state,
        evidence,
        message: (state != EnvironmentCapabilityState::Ready).then(|| partial_message.into()),
    }
}

fn capability_tauri(present: &std::collections::HashSet<&str>) -> EnvironmentCapability {
    let package_managers = ["pnpm", "npm", "yarn", "bun"];
    let mut evidence = ["node", "rustc", "cargo", "protoc"]
        .into_iter()
        .filter(|tool| present.contains(tool))
        .map(str::to_string)
        .collect::<Vec<_>>();
    evidence.extend(
        package_managers
            .into_iter()
            .filter(|tool| present.contains(tool))
            .map(str::to_string),
    );
    evidence.sort();
    evidence.dedup();

    let has_core = ["node", "rustc", "cargo", "protoc"]
        .into_iter()
        .all(|tool| present.contains(tool));
    let has_package_manager = ["pnpm", "npm", "yarn", "bun"]
        .into_iter()
        .any(|tool| present.contains(tool));
    let state = if has_core && has_package_manager {
        EnvironmentCapabilityState::Ready
    } else if evidence.is_empty() {
        EnvironmentCapabilityState::Missing
    } else {
        EnvironmentCapabilityState::Partial
    };

    EnvironmentCapability {
        id: "tauri_desktop_build".into(),
        state,
        evidence,
        message: (state != EnvironmentCapabilityState::Ready).then(|| {
            "Tauri desktop builds require Node, a package manager, Rust, and protoc.".into()
        }),
    }
}

fn tool_summary(tool: &EnvironmentToolRecord) -> EnvironmentToolSummary {
    EnvironmentToolSummary {
        id: tool.id.clone(),
        category: tool.category,
        custom: tool.custom,
        present: tool.present,
        version: tool.version.clone(),
        display_path: tool.path.as_deref().and_then(display_path),
        probe_status: tool.probe_status,
    }
}

fn current_platform() -> EnvironmentPlatform {
    EnvironmentPlatform {
        os_kind: env::consts::OS.to_string(),
        os_version: None,
        arch: env::consts::ARCH.to_string(),
        default_shell: None,
    }
}

fn first_useful_line(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(300).collect::<String>())
}

fn sanitize_version_line(line: String) -> Option<String> {
    if find_prohibited_persistence_content(&line).is_some() {
        None
    } else {
        Some(line)
    }
}

fn safe_persisted_path(path: &Path) -> Option<String> {
    let path = path_to_string(path);
    if find_prohibited_persistence_content(&path).is_some() {
        None
    } else {
        Some(path)
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
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => return None,
        }
    }
    child.wait_with_output().ok()
}

fn bundled_tool_dirs() -> Vec<PathBuf> {
    env::var_os("XERO_SOLANA_RESOURCE_ROOT")
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .map(|root| tool_dirs_from_root(&root))
        .unwrap_or_default()
}

fn managed_tool_dirs() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = env::var_os("XERO_SOLANA_TOOLCHAIN_ROOT") {
        roots.push(PathBuf::from(root));
    }
    if let Some(data_dir) = dirs::data_dir() {
        roots.push(data_dir.join("xero").join("solana").join("toolchain"));
        roots.push(data_dir.join("xero").join("toolchains"));
    }

    roots
        .into_iter()
        .flat_map(|root| tool_dirs_from_root(&root))
        .collect()
}

fn tool_dirs_from_root(root: &Path) -> Vec<PathBuf> {
    vec![
        root.join("bin"),
        root.join("agave")
            .join("install")
            .join("active_release")
            .join("bin"),
        root.join("anchor").join("bin"),
        root.join("node").join("bin"),
        root.join("pnpm").join("bin"),
    ]
}

fn common_dev_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(windows)]
    {
        if let Some(profile) = env::var_os("USERPROFILE") {
            let profile = PathBuf::from(profile);
            dirs.push(
                profile
                    .join(".local")
                    .join("share")
                    .join("solana")
                    .join("install")
                    .join("active_release")
                    .join("bin"),
            );
            dirs.push(profile.join(".cargo").join("bin"));
            dirs.push(profile.join(".avm").join("bin"));
        }
        if let Some(app_data) = env::var_os("APPDATA") {
            dirs.push(PathBuf::from(app_data).join("npm"));
        }
    }

    #[cfg(not(windows))]
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/share/solana/install/active_release/bin"));
        dirs.push(home.join(".cargo/bin"));
        dirs.push(home.join(".avm/bin"));
    }

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/usr/bin"));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/usr/bin"));
        dirs.push(PathBuf::from("/bin"));
    }

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

fn looks_like_path(value: &str) -> bool {
    value.contains('/') || value.contains('\\') || Path::new(value).is_absolute()
}

fn normalized_path_entries<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Vec<String> {
    let mut entries = paths
        .into_iter()
        .map(|path| path.to_string_lossy().trim().to_string())
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    entries.sort();
    entries.dedup();
    entries
}

fn path_fingerprint(entries: &[String]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.as_bytes());
        hasher.update([0]);
    }
    format!("sha256-{}", to_hex(&hasher.finalize()))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn display_path(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn now_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Debug, Clone)]
    struct FakeResolver {
        binaries: HashMap<String, ResolvedEnvironmentBinary>,
    }

    impl FakeResolver {
        fn with_binary(command: &str) -> Self {
            let mut binaries = HashMap::new();
            binaries.insert(
                command.into(),
                ResolvedEnvironmentBinary {
                    path: std::env::temp_dir().join(command),
                    source: EnvironmentToolSource::Path,
                },
            );
            Self { binaries }
        }

        fn empty() -> Self {
            Self {
                binaries: HashMap::new(),
            }
        }
    }

    impl EnvironmentBinaryResolver for FakeResolver {
        fn resolve(&self, command: &str) -> Option<ResolvedEnvironmentBinary> {
            self.binaries.get(command).cloned()
        }

        fn path_profile(&self) -> EnvironmentPathProfile {
            EnvironmentPathProfile {
                entry_count: 1,
                fingerprint: Some("sha256-test".into()),
                sources: vec!["tauri-process-path".into()],
            }
        }

        fn child_envs(&self) -> Vec<(OsString, OsString)> {
            vec![]
        }
    }

    #[derive(Debug, Clone)]
    struct FakeExecutor {
        execution: EnvironmentCommandExecution,
    }

    impl EnvironmentCommandExecutor for FakeExecutor {
        fn run(
            &self,
            _binary: &Path,
            _args: &[String],
            _timeout: Duration,
            _child_envs: &[(OsString, OsString)],
        ) -> EnvironmentCommandExecution {
            self.execution.clone()
        }
    }

    fn single_entry() -> Vec<EnvironmentProbeCatalogEntry> {
        vec![entry(
            "node",
            EnvironmentToolCategory::LanguageRuntime,
            "node",
            &["--version"],
        )]
    }

    fn run_fake(
        resolver: FakeResolver,
        execution: EnvironmentCommandExecution,
    ) -> EnvironmentProbeReport {
        probe_environment_profile_with(
            single_entry(),
            Arc::new(resolver),
            Arc::new(FakeExecutor { execution }),
            EnvironmentProbeOptions {
                timeout: Duration::from_millis(25),
                concurrency: 2,
            },
        )
        .expect("fake probe report")
    }

    #[test]
    fn present_probe_extracts_first_non_empty_version_line() {
        let report = run_fake(
            FakeResolver::with_binary("node"),
            EnvironmentCommandExecution::Completed {
                success: true,
                stdout: b"\n v20.11.1\nextra".to_vec(),
                stderr: vec![],
            },
        );

        let tool = &report.payload.tools[0];
        assert!(tool.present);
        assert_eq!(tool.version.as_deref(), Some("v20.11.1"));
        assert_eq!(tool.probe_status, EnvironmentToolProbeStatus::Ok);
        assert_eq!(report.status, EnvironmentProfileStatus::Ready);
    }

    #[test]
    fn absent_probe_records_missing_without_diagnostic() {
        let report = run_fake(
            FakeResolver::empty(),
            EnvironmentCommandExecution::Completed {
                success: true,
                stdout: b"unused".to_vec(),
                stderr: vec![],
            },
        );

        let tool = &report.payload.tools[0];
        assert!(!tool.present);
        assert_eq!(tool.probe_status, EnvironmentToolProbeStatus::Missing);
        assert!(report.payload.diagnostics.is_empty());
        assert_eq!(report.status, EnvironmentProfileStatus::Ready);
    }

    #[test]
    fn timeout_probe_records_retryable_diagnostic() {
        let report = run_fake(
            FakeResolver::with_binary("node"),
            EnvironmentCommandExecution::Timeout,
        );

        let tool = &report.payload.tools[0];
        assert!(tool.present);
        assert_eq!(tool.probe_status, EnvironmentToolProbeStatus::Timeout);
        assert_eq!(report.status, EnvironmentProfileStatus::Partial);
        assert_eq!(
            report.payload.diagnostics[0].code,
            "environment_probe_timeout"
        );
        assert!(report.payload.diagnostics[0].retryable);
    }

    #[test]
    fn bad_utf8_probe_is_lossy_and_valid() {
        let report = run_fake(
            FakeResolver::with_binary("node"),
            EnvironmentCommandExecution::Completed {
                success: true,
                stdout: vec![0xff, b'2', b'0', b'\n'],
                stderr: vec![],
            },
        );

        let version = report.payload.tools[0]
            .version
            .as_deref()
            .expect("lossy version line");
        assert!(version.contains('\u{fffd}'));
        validate_environment_payload(&report.payload).expect("bad UTF-8 probe remains valid");
    }

    #[test]
    fn sensitive_version_output_is_not_persisted() {
        let report = run_fake(
            FakeResolver::with_binary("node"),
            EnvironmentCommandExecution::Completed {
                success: true,
                stdout: b"node sk-demo-token\n".to_vec(),
                stderr: vec![],
            },
        );

        let tool = &report.payload.tools[0];
        assert!(tool.present);
        assert!(tool.version.is_none());
        assert_eq!(tool.probe_status, EnvironmentToolProbeStatus::Failed);
        assert_eq!(report.status, EnvironmentProfileStatus::Partial);
        validate_environment_payload(&report.payload).expect("sensitive output is redacted");
    }

    #[test]
    fn failed_probe_records_partial_status() {
        let report = run_fake(
            FakeResolver::with_binary("node"),
            EnvironmentCommandExecution::Completed {
                success: false,
                stdout: vec![],
                stderr: vec![],
            },
        );

        assert_eq!(
            report.payload.tools[0].probe_status,
            EnvironmentToolProbeStatus::Failed
        );
        assert_eq!(report.status, EnvironmentProfileStatus::Partial);
    }

    #[test]
    fn derives_tauri_capability_from_core_tools_and_protoc() {
        let mut binaries = HashMap::new();
        for command in ["node", "pnpm", "rustc", "cargo", "protoc"] {
            binaries.insert(
                command.into(),
                ResolvedEnvironmentBinary {
                    path: std::env::temp_dir().join(command),
                    source: EnvironmentToolSource::Path,
                },
            );
        }
        let catalog = vec![
            entry(
                "node",
                EnvironmentToolCategory::LanguageRuntime,
                "node",
                &["--version"],
            ),
            entry(
                "pnpm",
                EnvironmentToolCategory::PackageManager,
                "pnpm",
                &["--version"],
            ),
            entry(
                "rustc",
                EnvironmentToolCategory::LanguageRuntime,
                "rustc",
                &["--version"],
            ),
            entry(
                "cargo",
                EnvironmentToolCategory::LanguageRuntime,
                "cargo",
                &["--version"],
            ),
            entry(
                "protoc",
                EnvironmentToolCategory::BaseDeveloperTool,
                "protoc",
                &["--version"],
            ),
        ];
        let report = probe_environment_profile_with(
            catalog,
            Arc::new(FakeResolver { binaries }),
            Arc::new(FakeExecutor {
                execution: EnvironmentCommandExecution::Completed {
                    success: true,
                    stdout: b"ok\n".to_vec(),
                    stderr: vec![],
                },
            }),
            EnvironmentProbeOptions::default(),
        )
        .expect("probe report");

        let capability = report
            .payload
            .capabilities
            .iter()
            .find(|capability| capability.id == "tauri_desktop_build")
            .expect("tauri capability");
        assert_eq!(capability.state, EnvironmentCapabilityState::Ready);
        assert!(capability.evidence.contains(&"protoc".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn custom_probe_uses_executor_and_captures_version_line() {
        use std::os::unix::fs::PermissionsExt;

        let tempdir = tempfile::tempdir().expect("temp dir");
        let script = tempdir.path().join("fixture-tool");
        std::fs::write(&script, "#!/bin/sh\necho fixture-tool 1.2.3\n").expect("write fixture");
        let mut permissions = std::fs::metadata(&script)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod fixture");

        let mut binaries = HashMap::new();
        binaries.insert(
            "fixture-tool".into(),
            ResolvedEnvironmentBinary {
                path: script,
                source: EnvironmentToolSource::Path,
            },
        );

        let report = probe_environment_profile_with(
            vec![EnvironmentProbeCatalogEntry {
                id: "fixture_tool".into(),
                category: EnvironmentToolCategory::ShellUtility,
                command: "fixture-tool".into(),
                args: vec!["--version".into()],
                custom: true,
            }],
            Arc::new(FakeResolver { binaries }),
            Arc::new(SystemEnvironmentCommandExecutor),
            EnvironmentProbeOptions {
                timeout: Duration::from_secs(1),
                concurrency: 1,
            },
        )
        .expect("probe report");

        let tool = &report.payload.tools[0];
        assert!(tool.custom);
        assert!(tool.present);
        assert_eq!(tool.version.as_deref(), Some("fixture-tool 1.2.3"));
        assert_eq!(tool.probe_status, EnvironmentToolProbeStatus::Ok);
        assert!(report.summary.tools[0].custom);
    }
}
