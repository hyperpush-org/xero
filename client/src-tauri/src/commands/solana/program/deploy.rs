//! Deploy / upgrade orchestrator with two authority modes.
//!
//! Wraps `solana program deploy` (or upgrade) for the direct-keypair
//! path, and the buffer-upload + Squads-proposal synthesis for the
//! multisig path. After a successful direct deploy this module
//! optionally runs the post-deploy hooks: publish the IDL on-chain via
//! `anchor idl init/upgrade` and regenerate clients via Codama.
//!
//! The module is split into:
//!   1. A `DeployRunner` trait that wraps the actual `solana` CLI
//!      (`spawn` + `wait` + capture). Tests inject a `MockDeployRunner`
//!      so we can assert on argv without shelling out.
//!   2. The `deploy` orchestration function that takes a `DeploySpec`,
//!      decides which path to take, emits progress events, and
//!      composes the result with the post-deploy hooks.
//!   3. A `program_archive_dir` helper that writes the deployed `.so`
//!      to a per-program directory so future rollbacks have a snapshot
//!      to restore from (Phase 5 plan §8.7).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::idl::{
    self,
    codama::{
        CodamaGenerationReport, CodamaGenerationRequest, CodamaRunner, CodamaTarget,
        SystemCodamaRunner,
    },
    publish::{
        AnchorIdlRunner, DeployProgressPayload, DeployProgressPhase, DeployProgressSink,
        IdlPublishMode, IdlPublishReport, IdlPublishRequest, SystemAnchorIdlRunner,
    },
};
use crate::commands::solana::toolchain;
use crate::commands::{CommandError, CommandResult};

use super::squads::{synthesize as synthesize_squads, SquadsProposalDescriptor};

const DEFAULT_DEPLOY_TIMEOUT: Duration = Duration::from_secs(900);
const CAPTURE_BYTES: usize = 16_384;

/// Authority mode. `DirectKeypair` invokes `solana program deploy`
/// against the provided keypair file; `SquadsVault` short-circuits to
/// a buffer-write + proposal synthesis path so the desktop app never
/// holds the multisig signing key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum DeployAuthority {
    DirectKeypair {
        /// Filesystem path to a `solana-keygen` JSON keypair.
        keypair_path: String,
    },
    SquadsVault {
        /// Multisig PDA — the program's upgrade authority on chain.
        multisig_pda: String,
        /// Vault index inside the multisig (defaults to 0).
        #[serde(default)]
        vault_index: Option<u8>,
        /// Squads member that uploads the buffer + creates the
        /// proposal. The desktop app only signs the buffer write; the
        /// proposal itself is created via the synthesized argv.
        creator: String,
        /// Member keypair path used for the buffer write only. Same
        /// safety properties as the direct-keypair path: never used
        /// to sign an upgrade against mainnet.
        creator_keypair_path: String,
        /// Spill account that receives the buffer's lamports after
        /// the upgrade lands. Conventionally the creator.
        #[serde(default)]
        spill: Option<String>,
        /// Optional memo appended to the proposal.
        #[serde(default)]
        memo: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PostDeployOptions {
    /// Run `anchor idl init/upgrade` after the deploy lands. Skipped
    /// when the project doesn't have an IDL.
    #[serde(default = "default_publish_idl")]
    pub publish_idl: bool,
    /// `init` for first-time deploys; `upgrade` otherwise. When
    /// `None` the orchestrator picks based on
    /// `is_first_time_deploy` from the safety report.
    #[serde(default)]
    pub idl_publish_mode: Option<IdlPublishMode>,
    /// Run Codama codegen against the local IDL so frontend clients
    /// stay in sync after a deploy.
    #[serde(default = "default_run_codama")]
    pub run_codama: bool,
    /// Codama targets when `run_codama` is true.
    #[serde(default = "default_codama_targets")]
    pub codama_targets: Vec<CodamaTarget>,
    /// Output directory for Codama codegen.
    #[serde(default)]
    pub codama_output_dir: Option<String>,
    /// Archive the deployed `.so` under
    /// `<program_archive_root>/<program_id>/<sha>.so` so a future
    /// rollback can restore it.
    #[serde(default = "default_archive_artifact")]
    pub archive_artifact: bool,
    /// Override directory for the `.so` archive. Defaults to the OS
    /// data dir under `xero-solana-program-archive/`.
    #[serde(default)]
    pub program_archive_root: Option<String>,
}

fn default_publish_idl() -> bool {
    true
}
fn default_run_codama() -> bool {
    false
}
fn default_codama_targets() -> Vec<CodamaTarget> {
    vec![CodamaTarget::Ts]
}
fn default_archive_artifact() -> bool {
    true
}

impl Default for PostDeployOptions {
    fn default() -> Self {
        Self {
            publish_idl: default_publish_idl(),
            idl_publish_mode: None,
            run_codama: default_run_codama(),
            codama_targets: default_codama_targets(),
            codama_output_dir: None,
            archive_artifact: default_archive_artifact(),
            program_archive_root: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeploySpec {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub rpc_url: String,
    /// Path to the `.so` to deploy. Produced by `solana_program_build`.
    pub so_path: String,
    /// Path to the local IDL (for the post-deploy publish hook).
    /// Optional — non-Anchor programs skip the publish step.
    #[serde(default)]
    pub idl_path: Option<String>,
    pub authority: DeployAuthority,
    /// When this is the first-ever deploy, set to true so the
    /// orchestrator runs `solana program deploy` (init) instead of
    /// `solana program deploy --program-id <pid>` (upgrade). Defaults
    /// to upgrade because `solana program deploy` happily switches
    /// modes internally — this field only changes the IDL publish
    /// path's `init` vs `upgrade` decision when the caller doesn't
    /// pin it explicitly.
    #[serde(default)]
    pub is_first_deploy: bool,
    #[serde(default)]
    pub post: PostDeployOptions,
    /// Phase 9 deploy-gate — when set, the project tree rooted here is
    /// scanned for committed secrets before the deploy runs. A
    /// `Critical` finding (committed keypair JSON) is a policy-denied
    /// block. `None` skips the scan (unchanged Phase 5 behaviour).
    #[serde(default)]
    pub project_root: Option<String>,
    /// Phase 9 — when true, even `High`/`Medium` findings block the
    /// deploy. Defaults to false so the block behaviour is scoped to
    /// the same "committed keypair" signal called out in the plan.
    #[serde(default)]
    pub block_on_any_secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirectDeployOutcome {
    pub argv: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub signature: Option<String>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BufferWriteOutcome {
    pub argv: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub buffer_address: Option<String>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveRecord {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DeployResult {
    Direct {
        program_id: String,
        cluster: ClusterKind,
        outcome: DirectDeployOutcome,
        idl_publish: Option<IdlPublishReport>,
        codama: Option<CodamaGenerationReport>,
        archive: Option<ArchiveRecord>,
    },
    Squads {
        program_id: String,
        cluster: ClusterKind,
        buffer_write: BufferWriteOutcome,
        proposal: SquadsProposalDescriptor,
        archive: Option<ArchiveRecord>,
    },
}

// ---------- Deploy runner trait ---------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployInvocation {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
    pub envs: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait DeployRunner: Send + Sync + std::fmt::Debug {
    fn run(&self, invocation: &DeployInvocation) -> CommandResult<DeployOutcome>;
}

#[derive(Debug, Default)]
pub struct SystemDeployRunner;

impl SystemDeployRunner {
    pub fn new() -> Self {
        Self
    }
}

impl DeployRunner for SystemDeployRunner {
    fn run(&self, invocation: &DeployInvocation) -> CommandResult<DeployOutcome> {
        let (program, args) = invocation.argv.split_first().ok_or_else(|| {
            CommandError::system_fault(
                "solana_program_deploy_empty_argv",
                "Empty argv passed to deploy runner.",
            )
        })?;
        let resolved_program = toolchain::resolve_command(program);
        let mut cmd = Command::new(&resolved_program);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        if let Some(cwd) = &invocation.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &invocation.envs {
            cmd.env(k, v);
        }
        toolchain::augment_command(&mut cmd);
        let child = cmd.spawn().map_err(|err| {
            CommandError::user_fixable(
                "solana_program_deploy_spawn_failed",
                format!(
                    "Could not run `{program}`: {err}. Install the managed Solana toolchain or ensure the Solana CLI is on PATH.",
                ),
            )
        })?;
        let output = wait_with_timeout(child, invocation.timeout).ok_or_else(|| {
            CommandError::retryable(
                "solana_program_deploy_timeout",
                format!(
                    "`solana program ...` timed out after {}s.",
                    invocation.timeout.as_secs()
                ),
            )
        })?;
        Ok(DeployOutcome {
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

// ---------- Orchestrator ----------------------------------------------

pub struct DeployServices {
    pub runner: Arc<dyn DeployRunner>,
    pub idl_runner: Arc<dyn AnchorIdlRunner>,
    pub codama_runner: Arc<dyn CodamaRunner>,
}

impl DeployServices {
    pub fn system() -> Self {
        Self {
            runner: Arc::new(SystemDeployRunner::new()),
            idl_runner: Arc::new(SystemAnchorIdlRunner::new()),
            codama_runner: Arc::new(SystemCodamaRunner::new()),
        }
    }
}

impl std::fmt::Debug for DeployServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeployServices").finish_non_exhaustive()
    }
}

pub fn deploy(
    services: &DeployServices,
    sink: &dyn DeployProgressSink,
    spec: &DeploySpec,
) -> CommandResult<DeployResult> {
    validate_spec(spec)?;
    secrets_preflight(sink, spec)?;
    emit(
        sink,
        spec,
        DeployProgressPhase::Planning,
        format!("Planning deploy of {}", spec.program_id),
    );

    match &spec.authority {
        DeployAuthority::DirectKeypair { keypair_path } => {
            deploy_direct(services, sink, spec, keypair_path)
        }
        DeployAuthority::SquadsVault {
            multisig_pda,
            vault_index,
            creator,
            creator_keypair_path,
            spill,
            memo,
        } => deploy_squads(
            services,
            sink,
            spec,
            multisig_pda,
            *vault_index,
            creator,
            creator_keypair_path,
            spill.as_deref(),
            memo.clone(),
        ),
    }
}

fn validate_spec(spec: &DeploySpec) -> CommandResult<()> {
    if spec.program_id.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_program_deploy_missing_program_id",
            "program_id is required.",
        ));
    }
    if spec.rpc_url.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_program_deploy_missing_rpc_url",
            "rpc_url is required.",
        ));
    }
    if !Path::new(&spec.so_path).is_file() {
        return Err(CommandError::user_fixable(
            "solana_program_deploy_missing_so",
            format!("Built .so at {} does not exist.", spec.so_path),
        ));
    }
    match &spec.authority {
        DeployAuthority::DirectKeypair { keypair_path } => {
            if !Path::new(keypair_path).is_file() {
                return Err(CommandError::user_fixable(
                    "solana_program_deploy_missing_keypair",
                    format!("Authority keypair {keypair_path} does not exist."),
                ));
            }
            if matches!(spec.cluster, ClusterKind::Mainnet) {
                return Err(CommandError::policy_denied(
                    "Direct-keypair deploys to mainnet are blocked. Use a Squads vault authority instead.",
                ));
            }
        }
        DeployAuthority::SquadsVault {
            creator_keypair_path,
            ..
        } => {
            if !Path::new(creator_keypair_path).is_file() {
                return Err(CommandError::user_fixable(
                    "solana_program_deploy_missing_squads_creator_keypair",
                    format!("Squads creator keypair {creator_keypair_path} does not exist."),
                ));
            }
            if matches!(
                spec.cluster,
                ClusterKind::Localnet | ClusterKind::MainnetFork
            ) {
                return Err(CommandError::policy_denied(
                    "Squads proposals only target devnet / mainnet — use a direct keypair authority for local clusters.",
                ));
            }
        }
    }
    Ok(())
}

fn deploy_direct(
    services: &DeployServices,
    sink: &dyn DeployProgressSink,
    spec: &DeploySpec,
    keypair_path: &str,
) -> CommandResult<DeployResult> {
    let argv = build_direct_argv(spec, keypair_path);
    emit(
        sink,
        spec,
        DeployProgressPhase::Uploading,
        format!("Running: {}", argv.join(" ")),
    );
    let invocation = DeployInvocation {
        argv: argv.clone(),
        cwd: None,
        timeout: DEFAULT_DEPLOY_TIMEOUT,
        envs: Vec::new(),
    };
    let start = Instant::now();
    let outcome = services.runner.run(&invocation)?;
    let elapsed_ms = start.elapsed().as_millis();
    let signature =
        extract_signature(&outcome.stdout).or_else(|| extract_signature(&outcome.stderr));

    let direct = DirectDeployOutcome {
        argv,
        success: outcome.success,
        exit_code: outcome.exit_code,
        signature,
        stdout_excerpt: truncate(&outcome.stdout, CAPTURE_BYTES),
        stderr_excerpt: truncate(&outcome.stderr, CAPTURE_BYTES),
        elapsed_ms,
    };

    if !direct.success {
        emit(
            sink,
            spec,
            DeployProgressPhase::Failed,
            format!(
                "solana program deploy failed (code {:?}): {}",
                direct.exit_code,
                truncate(&direct.stderr_excerpt, 500)
            ),
        );
        return Ok(DeployResult::Direct {
            program_id: spec.program_id.clone(),
            cluster: spec.cluster,
            outcome: direct,
            idl_publish: None,
            codama: None,
            archive: None,
        });
    }

    emit(
        sink,
        spec,
        DeployProgressPhase::Finalising,
        "Deploy landed — running post-deploy hooks.".to_string(),
    );

    let archive = if spec.post.archive_artifact {
        match archive_so(spec) {
            Ok(record) => Some(record),
            Err(err) => {
                // Archive failure shouldn't undo the deploy — emit a
                // warning and keep going.
                emit(
                    sink,
                    spec,
                    DeployProgressPhase::Finalising,
                    format!(".so archive failed: {}", err.message),
                );
                None
            }
        }
    } else {
        None
    };

    let idl_publish = if spec.post.publish_idl {
        run_idl_publish(services, sink, spec, keypair_path)?
    } else {
        None
    };

    let codama = if spec.post.run_codama {
        run_codama(services, sink, spec)?
    } else {
        None
    };

    emit(
        sink,
        spec,
        DeployProgressPhase::Completed,
        format!(
            "Deploy succeeded in {}ms{}.",
            direct.elapsed_ms,
            direct
                .signature
                .as_deref()
                .map(|s| format!(" (signature {s})"))
                .unwrap_or_default()
        ),
    );

    Ok(DeployResult::Direct {
        program_id: spec.program_id.clone(),
        cluster: spec.cluster,
        outcome: direct,
        idl_publish,
        codama,
        archive,
    })
}

#[allow(clippy::too_many_arguments)]
fn deploy_squads(
    services: &DeployServices,
    sink: &dyn DeployProgressSink,
    spec: &DeploySpec,
    multisig_pda: &str,
    vault_index: Option<u8>,
    creator: &str,
    creator_keypair_path: &str,
    spill: Option<&str>,
    memo: Option<String>,
) -> CommandResult<DeployResult> {
    emit(
        sink,
        spec,
        DeployProgressPhase::Uploading,
        "Writing buffer for Squads proposal.".to_string(),
    );
    let argv = build_buffer_write_argv(spec, creator_keypair_path);
    let invocation = DeployInvocation {
        argv: argv.clone(),
        cwd: None,
        timeout: DEFAULT_DEPLOY_TIMEOUT,
        envs: Vec::new(),
    };
    let start = Instant::now();
    let outcome = services.runner.run(&invocation)?;
    let elapsed_ms = start.elapsed().as_millis();
    let buffer_address =
        extract_buffer_address(&outcome.stdout).or_else(|| extract_buffer_address(&outcome.stderr));

    let buffer = BufferWriteOutcome {
        argv,
        success: outcome.success,
        exit_code: outcome.exit_code,
        buffer_address: buffer_address.clone(),
        stdout_excerpt: truncate(&outcome.stdout, CAPTURE_BYTES),
        stderr_excerpt: truncate(&outcome.stderr, CAPTURE_BYTES),
        elapsed_ms,
    };

    if !buffer.success {
        emit(
            sink,
            spec,
            DeployProgressPhase::Failed,
            format!(
                "solana program write-buffer failed (code {:?}): {}",
                buffer.exit_code,
                truncate(&buffer.stderr_excerpt, 500)
            ),
        );
        return Err(CommandError::user_fixable(
            "solana_program_deploy_buffer_write_failed",
            format!(
                "`solana program write-buffer` exited {:?}: {}",
                buffer.exit_code,
                truncate(&buffer.stderr_excerpt, 200)
            ),
        ));
    }

    let buffer_address = buffer_address.ok_or_else(|| {
        CommandError::system_fault(
            "solana_program_deploy_buffer_address_missing",
            "Buffer write succeeded but buffer address could not be parsed from CLI output.",
        )
    })?;

    let proposal_request = super::squads::SquadsProposalRequest {
        program_id: spec.program_id.clone(),
        cluster: spec.cluster,
        multisig_pda: multisig_pda.to_string(),
        buffer: buffer_address.clone(),
        spill: spill.unwrap_or(creator).to_string(),
        creator: creator.to_string(),
        vault_index,
        memo,
    };
    let proposal = synthesize_squads(&proposal_request)?;

    let archive = if spec.post.archive_artifact {
        archive_so(spec).ok()
    } else {
        None
    };

    emit(
        sink,
        spec,
        DeployProgressPhase::Completed,
        format!(
            "Buffer {} uploaded; Squads proposal ready at {}",
            buffer_address, proposal.squads_app_url
        ),
    );

    Ok(DeployResult::Squads {
        program_id: spec.program_id.clone(),
        cluster: spec.cluster,
        buffer_write: buffer,
        proposal,
        archive,
    })
}

fn build_direct_argv(spec: &DeploySpec, keypair_path: &str) -> Vec<String> {
    // `solana program deploy` upgrades or initialises depending on whether
    // the program account exists. We always pass --program-id so the
    // upgrade path is explicit — this lets the loader keep the existing
    // program key without surprising the user.
    let mut argv: Vec<String> = vec![
        "solana".into(),
        "program".into(),
        "deploy".into(),
        spec.so_path.clone(),
        "--program-id".into(),
        spec.program_id.clone(),
        "--keypair".into(),
        keypair_path.to_string(),
        "--upgrade-authority".into(),
        keypair_path.to_string(),
        "--url".into(),
        spec.rpc_url.clone(),
        "--commitment".into(),
        "confirmed".into(),
    ];
    // Disable interactive prompts.
    argv.push("--output".into());
    argv.push("json-compact".into());
    argv
}

fn build_buffer_write_argv(spec: &DeploySpec, creator_keypair_path: &str) -> Vec<String> {
    vec![
        "solana".into(),
        "program".into(),
        "write-buffer".into(),
        spec.so_path.clone(),
        "--keypair".into(),
        creator_keypair_path.to_string(),
        "--url".into(),
        spec.rpc_url.clone(),
        "--commitment".into(),
        "confirmed".into(),
        "--output".into(),
        "json-compact".into(),
    ]
}

fn run_idl_publish(
    services: &DeployServices,
    sink: &dyn DeployProgressSink,
    spec: &DeploySpec,
    keypair_path: &str,
) -> CommandResult<Option<IdlPublishReport>> {
    let idl_path = match &spec.idl_path {
        Some(p) => p.clone(),
        None => return Ok(None),
    };
    if !Path::new(&idl_path).is_file() {
        return Ok(None);
    }
    let mode = spec
        .post
        .idl_publish_mode
        .unwrap_or(if spec.is_first_deploy {
            IdlPublishMode::Init
        } else {
            IdlPublishMode::Upgrade
        });
    if matches!(spec.cluster, ClusterKind::Mainnet) {
        // The publish module rejects mainnet anyway; surface this here
        // so the deploy doesn't appear "succeeded with publish skipped"
        // without explaining why.
        emit(
            sink,
            spec,
            DeployProgressPhase::Finalising,
            "Skipping IDL publish on mainnet — generate a Squads proposal for the IDL upgrade separately.".to_string(),
        );
        return Ok(None);
    }
    let request = IdlPublishRequest {
        program_id: spec.program_id.clone(),
        cluster: spec.cluster,
        idl_path,
        authority_keypair_path: keypair_path.to_string(),
        rpc_url: spec.rpc_url.clone(),
        mode,
    };
    let report = idl::publish::publish(services.idl_runner.as_ref(), sink, &request)?;
    Ok(Some(report))
}

fn run_codama(
    services: &DeployServices,
    sink: &dyn DeployProgressSink,
    spec: &DeploySpec,
) -> CommandResult<Option<CodamaGenerationReport>> {
    let idl_path = match &spec.idl_path {
        Some(p) => p.clone(),
        None => return Ok(None),
    };
    let output_dir = match &spec.post.codama_output_dir {
        Some(d) => d.clone(),
        None => {
            emit(
                sink,
                spec,
                DeployProgressPhase::Finalising,
                "Skipping Codama codegen: no output directory provided.".to_string(),
            );
            return Ok(None);
        }
    };
    let report = idl::codama::generate(
        services.codama_runner.as_ref(),
        &CodamaGenerationRequest {
            idl_path,
            targets: spec.post.codama_targets.clone(),
            output_dir,
        },
    )?;
    Ok(Some(report))
}

fn archive_so(spec: &DeploySpec) -> CommandResult<ArchiveRecord> {
    let bytes = fs::read(&spec.so_path).map_err(|err| {
        CommandError::user_fixable(
            "solana_program_deploy_archive_read_failed",
            format!("Could not read .so {}: {err}", spec.so_path),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha256 = hex_lower(hasher.finalize().as_slice());
    let root = match &spec.post.program_archive_root {
        Some(p) => PathBuf::from(p),
        None => default_archive_root(),
    };
    let dir = root.join(&spec.program_id);
    fs::create_dir_all(&dir).map_err(|err| {
        CommandError::system_fault(
            "solana_program_deploy_archive_mkdir_failed",
            format!("Could not create archive dir {}: {err}", dir.display()),
        )
    })?;
    let archive_path = dir.join(format!("{}.so", sha256));
    fs::write(&archive_path, &bytes).map_err(|err| {
        CommandError::system_fault(
            "solana_program_deploy_archive_write_failed",
            format!("Could not write {}: {err}", archive_path.display()),
        )
    })?;
    Ok(ArchiveRecord {
        path: archive_path.display().to_string(),
        sha256,
        size_bytes: bytes.len() as u64,
    })
}

fn default_archive_root() -> PathBuf {
    if let Some(d) = dirs::data_dir() {
        return d.join("xero/solana/program-archive");
    }
    std::env::temp_dir().join("xero-solana-program-archive")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RollbackRequest {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub rpc_url: String,
    /// SHA-256 (lowercase hex) of the previously deployed `.so` to
    /// restore. Must exist under `program_archive_root`.
    pub previous_sha256: String,
    pub authority: DeployAuthority,
    /// Override directory for the `.so` archive lookup. Defaults to
    /// the same OS data dir used by `deploy()`.
    #[serde(default)]
    pub program_archive_root: Option<String>,
    #[serde(default)]
    pub post: PostDeployOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RollbackResult {
    pub program_id: String,
    pub cluster: ClusterKind,
    pub restored_sha256: String,
    pub deploy: DeployResult,
}

pub fn rollback(
    services: &DeployServices,
    sink: &dyn DeployProgressSink,
    request: &RollbackRequest,
) -> CommandResult<RollbackResult> {
    if request.previous_sha256.len() != 64
        || !request
            .previous_sha256
            .chars()
            .all(|c| c.is_ascii_hexdigit())
    {
        return Err(CommandError::user_fixable(
            "solana_program_rollback_bad_sha",
            "previous_sha256 must be a lowercase 64-hex-digit SHA-256.",
        ));
    }
    let root = match &request.program_archive_root {
        Some(p) => PathBuf::from(p),
        None => default_archive_root(),
    };
    let archived = root
        .join(&request.program_id)
        .join(format!("{}.so", request.previous_sha256));
    if !archived.is_file() {
        return Err(CommandError::user_fixable(
            "solana_program_rollback_no_archive",
            format!(
                "No archived .so for program {} with sha {}.",
                request.program_id, request.previous_sha256
            ),
        ));
    }
    let spec = DeploySpec {
        program_id: request.program_id.clone(),
        cluster: request.cluster,
        rpc_url: request.rpc_url.clone(),
        so_path: archived.display().to_string(),
        idl_path: None,
        authority: request.authority.clone(),
        is_first_deploy: false,
        post: PostDeployOptions {
            // Rollbacks default to NOT re-publishing the IDL or
            // re-running Codama — the caller almost certainly wants
            // the already-deployed IDL.
            publish_idl: false,
            run_codama: false,
            ..request.post.clone()
        },
        // Rollbacks skip the secrets-scan gate: the archive is
        // produced from an already-vetted deploy.
        project_root: None,
        block_on_any_secret: false,
    };
    let outcome = deploy(services, sink, &spec)?;
    Ok(RollbackResult {
        program_id: request.program_id.clone(),
        cluster: request.cluster,
        restored_sha256: request.previous_sha256.clone(),
        deploy: outcome,
    })
}

// ---------- Helpers ---------------------------------------------------

/// Pre-deploy secrets scan. Silently no-ops when `project_root` is
/// `None` so Phase 5 callers keep their existing behaviour.
///
/// Blocks the deploy with a `policy_denied` when the scanner flags at
/// least one `Critical` finding (committed keypair JSON), and — when
/// the caller opts in via `block_on_any_secret` — also on `High` and
/// `Medium` findings.
fn secrets_preflight(sink: &dyn DeployProgressSink, spec: &DeploySpec) -> CommandResult<()> {
    let Some(root) = spec.project_root.as_deref() else {
        return Ok(());
    };
    emit(
        sink,
        spec,
        DeployProgressPhase::Planning,
        format!("Scanning {root} for committed secrets before deploy."),
    );
    let report = crate::commands::solana::secrets::scan_project(
        &crate::commands::solana::secrets::ScanRequest {
            project_root: root.to_string(),
            skip_paths: Vec::new(),
            min_severity: None,
            file_budget: None,
        },
    )?;

    let critical_count = report
        .findings
        .iter()
        .filter(|f| f.severity == crate::commands::solana::secrets::SecretSeverity::Critical)
        .count();
    let high_count = report
        .findings
        .iter()
        .filter(|f| {
            matches!(
                f.severity,
                crate::commands::solana::secrets::SecretSeverity::High
                    | crate::commands::solana::secrets::SecretSeverity::Medium
            )
        })
        .count();

    if critical_count == 0 && (!spec.block_on_any_secret || high_count == 0) {
        emit(
            sink,
            spec,
            DeployProgressPhase::Planning,
            format!(
                "Secrets scan clean ({} files, {} non-blocking findings).",
                report.files_scanned,
                report.findings.len(),
            ),
        );
        return Ok(());
    }

    let worst = report
        .findings
        .iter()
        .find(|f| {
            f.severity == crate::commands::solana::secrets::SecretSeverity::Critical
                || (spec.block_on_any_secret
                    && matches!(
                        f.severity,
                        crate::commands::solana::secrets::SecretSeverity::High
                            | crate::commands::solana::secrets::SecretSeverity::Medium
                    ))
        })
        .cloned();
    let detail = match worst {
        Some(f) => format!(
            "{} at {}{}",
            f.title,
            f.path,
            f.line.map(|l| format!(":{}", l)).unwrap_or_default(),
        ),
        None => "secrets-scan blocked deploy".to_string(),
    };
    emit(
        sink,
        spec,
        DeployProgressPhase::Failed,
        format!("Deploy blocked by secrets-scan: {detail}"),
    );
    Err(CommandError::policy_denied(format!(
        "Secrets-scan blocked deploy. {detail}. Run `solana_secrets_scan` for the full report, rotate the secret, \
         and retry."
    )))
}

fn emit(
    sink: &dyn DeployProgressSink,
    spec: &DeploySpec,
    phase: DeployProgressPhase,
    detail: String,
) {
    sink.emit(DeployProgressPayload {
        program_id: spec.program_id.clone(),
        cluster: spec.cluster.as_str().to_string(),
        phase,
        detail,
        ts_ms: now_ms(),
    });
}

fn extract_signature(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        for prefix in ["Signature:", "Transaction signature:"] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let sig = rest.trim();
                if !sig.is_empty() {
                    return Some(sig.to_string());
                }
            }
        }
    }
    None
}

fn extract_buffer_address(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        for prefix in [
            "Buffer:",
            "Wrote program data buffer to ",
            "Buffer address:",
        ] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let addr = rest.trim();
                if !addr.is_empty() {
                    return Some(addr.split_whitespace().next().unwrap_or(addr).to_string());
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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    pub struct MockDeployRunner {
        pub calls: Mutex<Vec<DeployInvocation>>,
        pub outcomes: Mutex<std::collections::VecDeque<DeployOutcome>>,
    }

    impl MockDeployRunner {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn queue(&self, outcome: DeployOutcome) {
            self.outcomes.lock().unwrap().push_back(outcome);
        }
    }

    impl DeployRunner for MockDeployRunner {
        fn run(&self, invocation: &DeployInvocation) -> CommandResult<DeployOutcome> {
            self.calls.lock().unwrap().push(invocation.clone());
            Ok(self
                .outcomes
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(DeployOutcome {
                    exit_code: Some(0),
                    success: true,
                    stdout: "Signature: SIG\n".into(),
                    stderr: String::new(),
                }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockDeployRunner;
    use super::*;
    use crate::commands::solana::idl::codama::test_support::MockCodamaRunner;
    use crate::commands::solana::idl::publish::test_support::{
        CollectingProgressSink, MockAnchorIdlRunner,
    };
    use tempfile::TempDir;

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    fn make_services(
        deploy_runner: Arc<dyn DeployRunner>,
        idl_runner: Arc<dyn AnchorIdlRunner>,
        codama_runner: Arc<dyn CodamaRunner>,
    ) -> DeployServices {
        DeployServices {
            runner: deploy_runner,
            idl_runner,
            codama_runner,
        }
    }

    fn valid_pk(byte: u8) -> String {
        bs58::encode([byte; 32]).into_string()
    }

    fn make_spec(tmp: &TempDir, cluster: ClusterKind, authority: DeployAuthority) -> DeploySpec {
        let so = tmp.path().join("p.so");
        write(&so, &[0u8; 1024]);
        DeploySpec {
            program_id: valid_pk(1),
            cluster,
            rpc_url: "https://api.devnet.solana.com".into(),
            so_path: so.display().to_string(),
            idl_path: None,
            authority,
            is_first_deploy: false,
            post: PostDeployOptions {
                publish_idl: false,
                idl_publish_mode: None,
                run_codama: false,
                codama_targets: Vec::new(),
                codama_output_dir: None,
                archive_artifact: false,
                program_archive_root: None,
            },
            project_root: None,
            block_on_any_secret: false,
        }
    }

    #[test]
    fn direct_deploy_returns_signature_and_archives_when_requested() {
        let tmp = TempDir::new().unwrap();
        let kp = tmp.path().join("kp.json");
        write(&kp, b"[]");
        let archive_root = tmp.path().join("archive");

        let runner = Arc::new(MockDeployRunner::new());
        runner.queue(DeployOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "Signature: 5abCEsQUFbmnoRsmB8NGbkmSpJWCGt9cZi1dE6HmxY8rB1p7H1MhCV4pHFg6bCSFhXnBQrhbqyvDnG9sGUMuJDRj\n".into(),
            stderr: String::new(),
        });
        let services = make_services(
            runner.clone(),
            Arc::new(MockAnchorIdlRunner::new()),
            Arc::new(MockCodamaRunner::new()),
        );

        let mut spec = make_spec(
            &tmp,
            ClusterKind::Devnet,
            DeployAuthority::DirectKeypair {
                keypair_path: kp.display().to_string(),
            },
        );
        spec.post.archive_artifact = true;
        spec.post.program_archive_root = Some(archive_root.display().to_string());

        let sink = CollectingProgressSink::default();
        let result = deploy(&services, &sink, &spec).unwrap();
        match result {
            DeployResult::Direct {
                outcome, archive, ..
            } => {
                assert!(outcome.success);
                assert!(outcome.signature.is_some());
                let archive = archive.unwrap();
                assert_eq!(archive.size_bytes, 1024);
                assert!(Path::new(&archive.path).is_file());
            }
            DeployResult::Squads { .. } => panic!("expected Direct deploy result"),
        }

        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let argv = &calls[0].argv;
        assert!(argv.contains(&"deploy".to_string()));
        assert!(argv.contains(&"--upgrade-authority".to_string()));

        let events = sink.0.lock().unwrap();
        let phases: Vec<_> = events.iter().map(|e| e.phase).collect();
        assert!(phases.contains(&DeployProgressPhase::Planning));
        assert!(phases.contains(&DeployProgressPhase::Uploading));
        assert!(phases.contains(&DeployProgressPhase::Completed));
    }

    #[test]
    fn direct_deploy_runs_idl_publish_post_hook_when_idl_present() {
        let tmp = TempDir::new().unwrap();
        let kp = tmp.path().join("kp.json");
        write(&kp, b"[]");
        let idl = tmp.path().join("p.json");
        write(&idl, b"{\"name\":\"p\"}");

        let runner = Arc::new(MockDeployRunner::new());
        let idl_runner = Arc::new(MockAnchorIdlRunner::new());
        let services = make_services(
            runner.clone(),
            idl_runner.clone() as Arc<dyn AnchorIdlRunner>,
            Arc::new(MockCodamaRunner::new()),
        );

        let mut spec = make_spec(
            &tmp,
            ClusterKind::Devnet,
            DeployAuthority::DirectKeypair {
                keypair_path: kp.display().to_string(),
            },
        );
        spec.idl_path = Some(idl.display().to_string());
        spec.post.publish_idl = true;
        spec.post.idl_publish_mode = Some(IdlPublishMode::Upgrade);

        let sink = CollectingProgressSink::default();
        let result = deploy(&services, &sink, &spec).unwrap();
        match result {
            DeployResult::Direct { idl_publish, .. } => {
                let report = idl_publish.expect("idl publish report");
                assert_eq!(report.mode, IdlPublishMode::Upgrade);
                assert!(report.success);
            }
            DeployResult::Squads { .. } => panic!("expected Direct"),
        }
        let idl_calls = idl_runner.calls.lock().unwrap();
        assert_eq!(idl_calls.len(), 1);
    }

    #[test]
    fn direct_deploy_fails_when_keypair_missing() {
        let tmp = TempDir::new().unwrap();
        let services = make_services(
            Arc::new(MockDeployRunner::new()),
            Arc::new(MockAnchorIdlRunner::new()),
            Arc::new(MockCodamaRunner::new()),
        );
        let spec = make_spec(
            &tmp,
            ClusterKind::Devnet,
            DeployAuthority::DirectKeypair {
                keypair_path: tmp.path().join("missing.json").display().to_string(),
            },
        );
        let sink = CollectingProgressSink::default();
        let err = deploy(&services, &sink, &spec).unwrap_err();
        assert_eq!(err.code, "solana_program_deploy_missing_keypair");
    }

    #[test]
    fn direct_deploy_blocks_mainnet() {
        let tmp = TempDir::new().unwrap();
        let kp = tmp.path().join("kp.json");
        write(&kp, b"[]");
        let services = make_services(
            Arc::new(MockDeployRunner::new()),
            Arc::new(MockAnchorIdlRunner::new()),
            Arc::new(MockCodamaRunner::new()),
        );
        let spec = make_spec(
            &tmp,
            ClusterKind::Mainnet,
            DeployAuthority::DirectKeypair {
                keypair_path: kp.display().to_string(),
            },
        );
        let sink = CollectingProgressSink::default();
        let err = deploy(&services, &sink, &spec).unwrap_err();
        assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
    }

    #[test]
    fn squads_deploy_writes_buffer_and_emits_proposal() {
        let tmp = TempDir::new().unwrap();
        let kp = tmp.path().join("creator.json");
        write(&kp, b"[]");

        let runner = Arc::new(MockDeployRunner::new());
        runner.queue(DeployOutcome {
            exit_code: Some(0),
            success: true,
            stdout: format!("Buffer: {}\n", valid_pk(7)),
            stderr: String::new(),
        });
        let services = make_services(
            runner.clone(),
            Arc::new(MockAnchorIdlRunner::new()),
            Arc::new(MockCodamaRunner::new()),
        );
        let spec = make_spec(
            &tmp,
            ClusterKind::Devnet,
            DeployAuthority::SquadsVault {
                multisig_pda: valid_pk(2),
                vault_index: Some(0),
                creator: valid_pk(5),
                creator_keypair_path: kp.display().to_string(),
                spill: None,
                memo: Some("upgrade test".into()),
            },
        );
        let sink = CollectingProgressSink::default();
        let result = deploy(&services, &sink, &spec).unwrap();
        match result {
            DeployResult::Squads {
                proposal,
                buffer_write,
                ..
            } => {
                assert_eq!(buffer_write.buffer_address, Some(valid_pk(7)));
                assert!(proposal.squads_app_url.contains("app.squads.so"));
            }
            DeployResult::Direct { .. } => panic!("expected Squads"),
        }
    }

    #[test]
    fn squads_deploy_blocks_localnet() {
        let tmp = TempDir::new().unwrap();
        let kp = tmp.path().join("creator.json");
        write(&kp, b"[]");
        let services = make_services(
            Arc::new(MockDeployRunner::new()),
            Arc::new(MockAnchorIdlRunner::new()),
            Arc::new(MockCodamaRunner::new()),
        );
        let spec = make_spec(
            &tmp,
            ClusterKind::Localnet,
            DeployAuthority::SquadsVault {
                multisig_pda: valid_pk(2),
                vault_index: None,
                creator: valid_pk(5),
                creator_keypair_path: kp.display().to_string(),
                spill: None,
                memo: None,
            },
        );
        let sink = CollectingProgressSink::default();
        let err = deploy(&services, &sink, &spec).unwrap_err();
        assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
    }

    #[test]
    fn squads_deploy_buffer_failure_returns_user_fixable_error() {
        let tmp = TempDir::new().unwrap();
        let kp = tmp.path().join("creator.json");
        write(&kp, b"[]");
        let runner = Arc::new(MockDeployRunner::new());
        runner.queue(DeployOutcome {
            exit_code: Some(1),
            success: false,
            stdout: String::new(),
            stderr: "Error: insufficient funds".into(),
        });
        let services = make_services(
            runner.clone(),
            Arc::new(MockAnchorIdlRunner::new()),
            Arc::new(MockCodamaRunner::new()),
        );
        let spec = make_spec(
            &tmp,
            ClusterKind::Devnet,
            DeployAuthority::SquadsVault {
                multisig_pda: valid_pk(2),
                vault_index: None,
                creator: valid_pk(5),
                creator_keypair_path: kp.display().to_string(),
                spill: None,
                memo: None,
            },
        );
        let sink = CollectingProgressSink::default();
        let err = deploy(&services, &sink, &spec).unwrap_err();
        assert_eq!(err.code, "solana_program_deploy_buffer_write_failed");
    }

    #[test]
    fn rollback_deploys_archived_so_when_present() {
        let tmp = TempDir::new().unwrap();
        let archive_root = tmp.path().join("archive");
        let program_id = valid_pk(1);
        let archived_dir = archive_root.join(&program_id);
        fs::create_dir_all(&archived_dir).unwrap();
        let bytes = vec![1u8, 2, 3, 4];
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha = hex_lower(hasher.finalize().as_slice());
        let archived_path = archived_dir.join(format!("{}.so", sha));
        fs::write(&archived_path, &bytes).unwrap();

        let kp = tmp.path().join("kp.json");
        write(&kp, b"[]");
        let runner = Arc::new(MockDeployRunner::new());
        runner.queue(DeployOutcome {
            exit_code: Some(0),
            success: true,
            stdout: "Signature: SIG\n".into(),
            stderr: String::new(),
        });
        let services = make_services(
            runner.clone(),
            Arc::new(MockAnchorIdlRunner::new()),
            Arc::new(MockCodamaRunner::new()),
        );
        let request = RollbackRequest {
            program_id: program_id.clone(),
            cluster: ClusterKind::Devnet,
            rpc_url: "https://api.devnet.solana.com".into(),
            previous_sha256: sha.clone(),
            authority: DeployAuthority::DirectKeypair {
                keypair_path: kp.display().to_string(),
            },
            program_archive_root: Some(archive_root.display().to_string()),
            post: PostDeployOptions {
                publish_idl: false,
                idl_publish_mode: None,
                run_codama: false,
                codama_targets: Vec::new(),
                codama_output_dir: None,
                archive_artifact: false,
                program_archive_root: None,
            },
        };
        let sink = CollectingProgressSink::default();
        let result = rollback(&services, &sink, &request).unwrap();
        assert_eq!(result.restored_sha256, sha);
        match result.deploy {
            DeployResult::Direct { outcome, .. } => assert!(outcome.success),
            DeployResult::Squads { .. } => panic!("expected Direct"),
        }
    }

    #[test]
    fn rollback_rejects_missing_archive() {
        let tmp = TempDir::new().unwrap();
        let kp = tmp.path().join("kp.json");
        write(&kp, b"[]");
        let services = make_services(
            Arc::new(MockDeployRunner::new()),
            Arc::new(MockAnchorIdlRunner::new()),
            Arc::new(MockCodamaRunner::new()),
        );
        let request = RollbackRequest {
            program_id: valid_pk(1),
            cluster: ClusterKind::Devnet,
            rpc_url: "https://api.devnet.solana.com".into(),
            previous_sha256: "0".repeat(64),
            authority: DeployAuthority::DirectKeypair {
                keypair_path: kp.display().to_string(),
            },
            program_archive_root: Some(tmp.path().join("nope").display().to_string()),
            post: PostDeployOptions::default(),
        };
        let sink = CollectingProgressSink::default();
        let err = rollback(&services, &sink, &request).unwrap_err();
        assert_eq!(err.code, "solana_program_rollback_no_archive");
    }

    #[test]
    fn extract_buffer_address_handles_buffer_prefix() {
        let text = format!("Some prelude\nBuffer: {}\nDone.", valid_pk(9));
        assert_eq!(extract_buffer_address(&text), Some(valid_pk(9)));
    }
}
