//! Indexer scaffold generator + local dev runner.
//!
//! Phase 7 ships three scaffolds, each matching a real production
//! indexer path developers commonly pick:
//!
//! - **Carbon** — a Rust cargo binary using [Carbon](https://github.com/sevenlabs-hq/carbon)
//!   to listen to cluster tx streams and decode them through an
//!   Anchor-compatible IDL. Targets Rust developers already running a
//!   backend. We emit a complete cargo crate with a `Cargo.toml` and
//!   `src/main.rs` that compiles unmodified against the generated IDL.
//! - **log_parser** — a zero-deps TypeScript Node script that polls
//!   `getSignaturesForAddress` + `getTransaction` through
//!   `@solana/web3.js` and runs a caller-supplied handler over each
//!   decoded log. Intended for small projects and throwaway CLIs.
//! - **helius_webhook** — an Express endpoint that Helius' Enhanced
//!   Webhooks can POST transaction payloads to, with handler stubs
//!   keyed on the IDL's instruction + event definitions. Targets
//!   teams using Helius' free tier.
//!
//! The scaffold generator is deterministic: given the same IDL + output
//! directory it produces bit-identical files. Tests assert exact shape.
//!
//! The local dev runner is a thin wrapper around the RPC log source
//! that replays the last `n` slots against an in-process log decoder
//! so the agent can answer "what events did my program emit recently"
//! without the indexer actually being running yet.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::idl::{Idl, IdlRegistry, IdlSource};
use crate::commands::solana::logs::{LogBus, LogEntry, RpcLogSource};
use crate::commands::{CommandError, CommandResult};

pub mod carbon;
pub mod log_parser;
pub mod webhook;

pub use carbon::render_carbon_scaffold;
pub use log_parser::render_log_parser_scaffold;
pub use webhook::render_webhook_scaffold;

/// Scaffold flavour. Serializes as `snake_case` so the frontend's
/// ScaffoldRequest JSON stays compact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IndexerKind {
    Carbon,
    LogParser,
    HeliusWebhook,
}

impl IndexerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            IndexerKind::Carbon => "carbon",
            IndexerKind::LogParser => "log_parser",
            IndexerKind::HeliusWebhook => "helius_webhook",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScaffoldRequest {
    pub kind: IndexerKind,
    /// Path to the Anchor IDL used to derive program id, event names,
    /// and instruction stubs. Must be readable via the registry's
    /// `load_file` path.
    pub idl_path: String,
    /// Destination directory. Created if missing. Files are written
    /// relative to this root.
    pub output_dir: String,
    /// Optional project slug — by default derived from the IDL
    /// metadata.name / metadata.address.
    #[serde(default)]
    pub project_slug: Option<String>,
    /// If true, overwrite existing files. Defaults to false — the
    /// generator errors instead of stomping handwritten code.
    #[serde(default)]
    pub overwrite: bool,
    /// Optional RPC URL the scaffold should bake in as its default.
    #[serde(default)]
    pub rpc_url: Option<String>,
}

/// One generated file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScaffoldFile {
    /// Path relative to the scaffold root.
    pub path: String,
    pub bytes_written: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScaffoldResult {
    pub kind: IndexerKind,
    pub root: String,
    pub project_slug: String,
    pub program_id: String,
    pub program_name: String,
    pub files: Vec<ScaffoldFile>,
    pub entrypoint: Option<String>,
    /// Human-readable next-step instructions the UI renders in a
    /// callout. Non-null for every kind we support.
    pub run_hint: String,
    /// When present, the CLI the user should run to start the
    /// scaffold. Indexer kinds that can't be "run" as a CLI (webhook)
    /// leave this null.
    pub start_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexerRunRequest {
    pub cluster: ClusterKind,
    /// Program ids the dev runner should stream events for.
    #[serde(default)]
    pub program_ids: Vec<String>,
    /// How many signatures (per program) to fetch and decode. Clamped
    /// to `[1, 1024]`.
    #[serde(default = "default_last_n")]
    pub last_n: u32,
    /// Optional RPC URL override. When None, the caller must have a
    /// running validator or a default router endpoint.
    #[serde(default)]
    pub rpc_url: Option<String>,
}

fn default_last_n() -> u32 {
    25
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexerRunReport {
    pub cluster: ClusterKind,
    pub program_ids: Vec<String>,
    pub fetched_signatures: u32,
    pub events_by_program: Vec<ProgramEventCount>,
    pub entries: Vec<LogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgramEventCount {
    pub program_id: String,
    pub transactions: u32,
    pub anchor_events: u32,
}

/// Context passed to the scaffold templates.
#[derive(Debug, Clone)]
pub struct ScaffoldContext<'a> {
    pub idl: &'a Idl,
    pub project_slug: String,
    pub program_id: String,
    pub program_name: String,
    pub rpc_url: String,
    pub instructions: Vec<InstructionDescriptor>,
    pub events: Vec<EventDescriptor>,
}

#[derive(Debug, Clone)]
pub struct InstructionDescriptor {
    pub name: String,
    pub snake_case: String,
    pub pascal_case: String,
    pub account_count: usize,
}

#[derive(Debug, Clone)]
pub struct EventDescriptor {
    pub name: String,
    pub snake_case: String,
    pub pascal_case: String,
    pub discriminator: Option<[u8; 8]>,
}

/// Dispatch to the right scaffold generator, then materialise the
/// files.
pub fn scaffold(
    registry: &IdlRegistry,
    request: &ScaffoldRequest,
) -> CommandResult<ScaffoldResult> {
    let idl = registry.load_file(Path::new(&request.idl_path))?;
    let program_id = idl.program_id().ok_or_else(|| {
        CommandError::user_fixable(
            "solana_indexer_idl_no_address",
            "IDL has no program address — add `metadata.address` or `address` before scaffolding.",
        )
    })?;
    let program_name = idl.program_name().unwrap_or_else(|| "program".to_string());
    let project_slug = request
        .project_slug
        .clone()
        .unwrap_or_else(|| slugify(&program_name));
    let rpc_url = request
        .rpc_url
        .clone()
        .unwrap_or_else(|| default_rpc_for(&idl));
    let instructions = collect_instructions(&idl);
    let events = collect_events(&idl);

    let ctx = ScaffoldContext {
        idl: &idl,
        project_slug: project_slug.clone(),
        program_id: program_id.clone(),
        program_name: program_name.clone(),
        rpc_url,
        instructions,
        events,
    };

    let files = match request.kind {
        IndexerKind::Carbon => render_carbon_scaffold(&ctx),
        IndexerKind::LogParser => render_log_parser_scaffold(&ctx),
        IndexerKind::HeliusWebhook => render_webhook_scaffold(&ctx),
    };

    let root = PathBuf::from(&request.output_dir).join(&project_slug);
    let written = write_files(&root, &files, request.overwrite)?;

    let entrypoint = entrypoint_for(request.kind);
    let run_hint = run_hint_for(request.kind, &root);
    let start_command = start_command_for(request.kind, &root);

    Ok(ScaffoldResult {
        kind: request.kind,
        root: root.display().to_string(),
        project_slug,
        program_id,
        program_name,
        files: written,
        entrypoint,
        run_hint,
        start_command,
    })
}

fn entrypoint_for(kind: IndexerKind) -> Option<String> {
    match kind {
        IndexerKind::Carbon => Some("src/main.rs".into()),
        IndexerKind::LogParser => Some("src/main.ts".into()),
        IndexerKind::HeliusWebhook => Some("src/server.ts".into()),
    }
}

fn run_hint_for(kind: IndexerKind, root: &Path) -> String {
    match kind {
        IndexerKind::Carbon => format!("cd {} && cargo run --release", root.display()),
        IndexerKind::LogParser => format!("cd {} && pnpm install && pnpm start", root.display()),
        IndexerKind::HeliusWebhook => format!(
            "cd {} && pnpm install && pnpm dev — then point Helius at the printed URL",
            root.display()
        ),
    }
}

fn start_command_for(kind: IndexerKind, root: &Path) -> Option<String> {
    match kind {
        IndexerKind::Carbon => Some(format!(
            "cargo run --release --manifest-path {}/Cargo.toml",
            root.display()
        )),
        IndexerKind::LogParser => Some(format!("pnpm --dir {} start", root.display())),
        IndexerKind::HeliusWebhook => None,
    }
}

fn default_rpc_for(idl: &Idl) -> String {
    // Prefer the cluster the IDL was fetched from if that metadata is
    // present; otherwise default to the localnet port so the scaffold
    // works against `solana-test-validator` out of the box.
    if let IdlSource::Chain { cluster, .. } = &idl.source {
        if let Some(descriptor) = super::cluster::descriptors()
            .into_iter()
            .find(|d| d.kind == *cluster)
        {
            return descriptor.default_rpc_url.to_string();
        }
    }
    "http://127.0.0.1:8899".to_string()
}

fn collect_instructions(idl: &Idl) -> Vec<InstructionDescriptor> {
    let arr = idl
        .value
        .get("instructions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    arr.into_iter()
        .filter_map(|entry| {
            let name = entry.get("name").and_then(|v| v.as_str())?.to_string();
            let account_count = entry
                .get("accounts")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Some(InstructionDescriptor {
                snake_case: to_snake_case(&name),
                pascal_case: to_pascal_case(&name),
                name,
                account_count,
            })
        })
        .collect()
}

fn collect_events(idl: &Idl) -> Vec<EventDescriptor> {
    let arr = idl
        .value
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    arr.into_iter()
        .filter_map(|entry| {
            let name = entry.get("name").and_then(|v| v.as_str())?.to_string();
            let discriminator = entry
                .get("discriminator")
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    let bytes: Vec<u8> = arr
                        .iter()
                        .filter_map(|n| n.as_u64().map(|u| u as u8))
                        .collect();
                    if bytes.len() == 8 {
                        let mut out = [0u8; 8];
                        out.copy_from_slice(&bytes);
                        Some(out)
                    } else {
                        None
                    }
                });
            Some(EventDescriptor {
                snake_case: to_snake_case(&name),
                pascal_case: to_pascal_case(&name),
                name,
                discriminator,
            })
        })
        .collect()
}

fn write_files(
    root: &Path,
    files: &[(String, String)],
    overwrite: bool,
) -> CommandResult<Vec<ScaffoldFile>> {
    fs::create_dir_all(root).map_err(|err| {
        CommandError::system_fault(
            "solana_indexer_mkdir_failed",
            format!("Could not create {}: {err}", root.display()),
        )
    })?;

    // Pre-flight: refuse to overwrite any existing file unless opted in.
    if !overwrite {
        for (rel, _) in files {
            let target = root.join(rel);
            if target.exists() {
                return Err(CommandError::user_fixable(
                    "solana_indexer_output_exists",
                    format!(
                        "{} already exists — pass overwrite=true to replace it.",
                        target.display()
                    ),
                ));
            }
        }
    }

    let mut written: Vec<ScaffoldFile> = Vec::with_capacity(files.len());
    for (rel, contents) in files {
        let target = root.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                CommandError::system_fault(
                    "solana_indexer_mkdir_failed",
                    format!("Could not create {}: {err}", parent.display()),
                )
            })?;
        }
        fs::write(&target, contents.as_bytes()).map_err(|err| {
            CommandError::system_fault(
                "solana_indexer_write_failed",
                format!("Could not write {}: {err}", target.display()),
            )
        })?;
        written.push(ScaffoldFile {
            path: rel.clone(),
            bytes_written: contents.len() as u64,
            sha256: sha256_hex(contents.as_bytes()),
        });
    }
    written.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(written)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
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

pub(crate) fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("indexer");
    }
    out
}

pub(crate) fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut prev_lower = false;
    for ch in name.chars() {
        if ch.is_uppercase() {
            if prev_lower && !out.is_empty() {
                out.push('_');
            }
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            prev_lower = false;
        } else if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
            prev_lower = false;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

pub(crate) fn to_pascal_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut next_upper = true;
    for ch in name.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            next_upper = true;
            continue;
        }
        if !ch.is_ascii_alphanumeric() {
            continue;
        }
        if next_upper {
            for upper in ch.to_uppercase() {
                out.push(upper);
            }
            next_upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Local dev runner. Replays the last N signatures per program id
/// through the `LogBus` decoder.
pub fn run_local(
    source: &dyn RpcLogSource,
    bus: Arc<LogBus>,
    request: &IndexerRunRequest,
    rpc_url_resolver: impl Fn(ClusterKind) -> Option<String>,
) -> CommandResult<IndexerRunReport> {
    if request.program_ids.is_empty() {
        return Err(CommandError::user_fixable(
            "solana_indexer_run_no_programs",
            "At least one program_id is required for the local indexer run.",
        ));
    }
    if !(1..=1024).contains(&request.last_n) {
        return Err(CommandError::user_fixable(
            "solana_indexer_run_bad_last_n",
            format!(
                "last_n must be between 1 and 1024 (got {}).",
                request.last_n
            ),
        ));
    }

    let rpc_url = request
        .rpc_url
        .clone()
        .or_else(|| rpc_url_resolver(request.cluster))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "solana_indexer_run_no_rpc",
                "No RPC URL available — start a cluster or supply rpcUrl explicitly.",
            )
        })?;

    let batches = source.fetch_recent_many(
        request.cluster,
        &rpc_url,
        &request.program_ids,
        request.last_n,
    )?;

    let fetched_signatures = batches.len() as u32;
    let mut entries: Vec<LogEntry> = Vec::with_capacity(batches.len());
    for batch in batches {
        let entry = bus.publish_raw(batch);
        entries.push(entry);
    }

    let wanted: BTreeSet<&str> = request.program_ids.iter().map(String::as_str).collect();
    let mut events_by_program: Vec<ProgramEventCount> = request
        .program_ids
        .iter()
        .map(|pid| ProgramEventCount {
            program_id: pid.clone(),
            transactions: 0,
            anchor_events: 0,
        })
        .collect();

    for entry in &entries {
        for count in events_by_program.iter_mut() {
            if entry
                .programs_invoked
                .iter()
                .any(|p| p.as_str() == count.program_id.as_str())
            {
                count.transactions += 1;
            }
        }
        for event in &entry.anchor_events {
            if wanted.contains(event.program_id.as_str()) {
                if let Some(slot) = events_by_program
                    .iter_mut()
                    .find(|p| p.program_id == event.program_id)
                {
                    slot.anchor_events += 1;
                }
            }
        }
    }

    Ok(IndexerRunReport {
        cluster: request.cluster,
        program_ids: request.program_ids.clone(),
        fetched_signatures,
        events_by_program,
        entries,
    })
}

/// Silence unused import warnings for `Value` — kept for future
/// IDL validation in this module.
#[allow(dead_code)]
fn _unused_value_type() -> Option<Value> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::solana::idl::{FetchedIdl, IdlFetcher};
    use crate::commands::solana::logs::RawLogBatch;
    use crate::commands::CommandResult;
    use std::sync::Mutex;
    use tempfile::TempDir;

    #[derive(Debug, Default)]
    struct NoopFetcher;

    impl IdlFetcher for NoopFetcher {
        fn fetch(
            &self,
            _cluster: ClusterKind,
            _rpc_url: &str,
            _program_id: &str,
        ) -> CommandResult<Option<FetchedIdl>> {
            Ok(None)
        }
    }

    fn sample_idl() -> serde_json::Value {
        serde_json::json!({
            "address": "Prog1111111111111111111111111111111111111111",
            "metadata": { "name": "my_program", "version": "0.1.0" },
            "instructions": [
                { "name": "initialize", "accounts": [{"name":"payer"},{"name":"state"}], "args": [] },
                { "name": "updateOwner", "accounts": [{"name":"state"}], "args": [] }
            ],
            "events": [
                { "name": "SwapEvent", "discriminator": [1,2,3,4,5,6,7,8] }
            ],
            "errors": [
                { "code": 6000, "name": "InvalidOwner", "msg": "owner mismatch" }
            ]
        })
    }

    fn registry_with_idl(tmp: &TempDir) -> (IdlRegistry, PathBuf) {
        let registry = IdlRegistry::new(Arc::new(NoopFetcher));
        let path = tmp.path().join("idl.json");
        fs::write(&path, serde_json::to_vec(&sample_idl()).unwrap()).unwrap();
        registry.load_file(&path).unwrap();
        (registry, path)
    }

    #[test]
    fn slugify_strips_non_alphanum_and_lowercases() {
        assert_eq!(slugify("My Program!"), "my-program");
        assert_eq!(slugify("____a--b"), "a-b");
        assert_eq!(slugify("!"), "indexer");
    }

    #[test]
    fn snake_case_transforms_camel_and_pascal() {
        assert_eq!(to_snake_case("updateOwner"), "update_owner");
        assert_eq!(to_snake_case("SwapEvent"), "swap_event");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn pascal_case_transforms_snake_and_kebab() {
        assert_eq!(to_pascal_case("update_owner"), "UpdateOwner");
        assert_eq!(to_pascal_case("swap-event"), "SwapEvent");
        assert_eq!(to_pascal_case("alreadyPascal"), "AlreadyPascal");
    }

    #[test]
    fn scaffold_carbon_writes_cargo_crate() {
        let tmp = TempDir::new().unwrap();
        let (registry, idl_path) = registry_with_idl(&tmp);
        let out_dir = tmp.path().join("indexers");
        let result = scaffold(
            &registry,
            &ScaffoldRequest {
                kind: IndexerKind::Carbon,
                idl_path: idl_path.display().to_string(),
                output_dir: out_dir.display().to_string(),
                project_slug: None,
                overwrite: false,
                rpc_url: None,
            },
        )
        .unwrap();
        let paths: BTreeSet<_> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains("Cargo.toml"));
        assert!(paths.contains("src/main.rs"));
        assert!(paths.contains("README.md"));
        assert!(paths.contains("idl/program.json"));
        assert_eq!(result.program_name, "my_program");
        assert_eq!(result.project_slug, "my-program");
        // Re-running without overwrite errors.
        let err = scaffold(
            &registry,
            &ScaffoldRequest {
                kind: IndexerKind::Carbon,
                idl_path: idl_path.display().to_string(),
                output_dir: out_dir.display().to_string(),
                project_slug: None,
                overwrite: false,
                rpc_url: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_indexer_output_exists");
    }

    #[test]
    fn scaffold_carbon_bakes_program_id_into_main_rs() {
        let tmp = TempDir::new().unwrap();
        let (registry, idl_path) = registry_with_idl(&tmp);
        let out_dir = tmp.path().join("indexers");
        let result = scaffold(
            &registry,
            &ScaffoldRequest {
                kind: IndexerKind::Carbon,
                idl_path: idl_path.display().to_string(),
                output_dir: out_dir.display().to_string(),
                project_slug: None,
                overwrite: false,
                rpc_url: Some("http://127.0.0.1:8899".into()),
            },
        )
        .unwrap();
        let main_rs = fs::read_to_string(PathBuf::from(&result.root).join("src/main.rs")).unwrap();
        assert!(main_rs.contains("Prog1111111111111111111111111111111111111111"));
        assert!(main_rs.contains("swap_event") || main_rs.contains("SwapEvent"));
        assert!(main_rs.contains("http://127.0.0.1:8899"));
    }

    #[test]
    fn scaffold_log_parser_writes_ts_files() {
        let tmp = TempDir::new().unwrap();
        let (registry, idl_path) = registry_with_idl(&tmp);
        let out_dir = tmp.path().join("idx");
        let result = scaffold(
            &registry,
            &ScaffoldRequest {
                kind: IndexerKind::LogParser,
                idl_path: idl_path.display().to_string(),
                output_dir: out_dir.display().to_string(),
                project_slug: None,
                overwrite: false,
                rpc_url: None,
            },
        )
        .unwrap();
        let paths: BTreeSet<_> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains("package.json"));
        assert!(paths.contains("src/main.ts"));
        assert!(paths.contains("src/idl.ts"));
        assert!(paths.contains("tsconfig.json"));
        let idl_ts = fs::read_to_string(PathBuf::from(&result.root).join("src/idl.ts")).unwrap();
        assert!(idl_ts.contains("SwapEvent"));
    }

    #[test]
    fn scaffold_webhook_writes_express_handler() {
        let tmp = TempDir::new().unwrap();
        let (registry, idl_path) = registry_with_idl(&tmp);
        let out_dir = tmp.path().join("hooks");
        let result = scaffold(
            &registry,
            &ScaffoldRequest {
                kind: IndexerKind::HeliusWebhook,
                idl_path: idl_path.display().to_string(),
                output_dir: out_dir.display().to_string(),
                project_slug: Some("custom-webhook".into()),
                overwrite: false,
                rpc_url: None,
            },
        )
        .unwrap();
        let paths: BTreeSet<_> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains("package.json"));
        assert!(paths.contains("src/server.ts"));
        assert!(paths.contains("src/handlers.ts"));
        assert_eq!(result.project_slug, "custom-webhook");
        assert!(
            result.start_command.is_none(),
            "webhook scaffolds expose no start_command (needs external trigger)"
        );
    }

    #[test]
    fn scaffold_is_deterministic_for_identical_input() {
        let tmp = TempDir::new().unwrap();
        let (registry, idl_path) = registry_with_idl(&tmp);
        let out_a = tmp.path().join("a");
        let out_b = tmp.path().join("b");
        let req = |dir: &Path| ScaffoldRequest {
            kind: IndexerKind::Carbon,
            idl_path: idl_path.display().to_string(),
            output_dir: dir.display().to_string(),
            project_slug: Some("same".into()),
            overwrite: false,
            rpc_url: Some("http://rpc.test".into()),
        };
        let a = scaffold(&registry, &req(&out_a)).unwrap();
        let b = scaffold(&registry, &req(&out_b)).unwrap();
        let hashes_a: Vec<_> = a.files.iter().map(|f| f.sha256.clone()).collect();
        let hashes_b: Vec<_> = b.files.iter().map(|f| f.sha256.clone()).collect();
        assert_eq!(hashes_a, hashes_b);
    }

    #[derive(Debug, Default)]
    struct RecordingSource {
        batches: Mutex<Vec<RawLogBatch>>,
    }

    impl RpcLogSource for RecordingSource {
        fn fetch_recent(
            &self,
            cluster: ClusterKind,
            _rpc_url: &str,
            program_id: &str,
            _limit: u32,
        ) -> CommandResult<Vec<RawLogBatch>> {
            use base64::Engine as _;
            let payload = base64::engine::general_purpose::STANDARD
                .encode([1u8, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0]);
            let batches = vec![
                RawLogBatch::new(
                    cluster,
                    "sig-1",
                    vec![
                        format!("Program {program_id} invoke [1]"),
                        format!("Program data: {payload}"),
                        format!("Program {program_id} success"),
                    ],
                )
                .with_slot(1)
                .with_program_hint(vec![program_id.to_string()]),
                RawLogBatch::new(
                    cluster,
                    "sig-2",
                    vec![
                        format!("Program {program_id} invoke [1]"),
                        format!("Program {program_id} success"),
                    ],
                )
                .with_slot(2)
                .with_program_hint(vec![program_id.to_string()]),
            ];
            self.batches.lock().unwrap().extend(batches.clone());
            Ok(batches)
        }
    }

    #[test]
    fn run_local_returns_structured_events_for_program() {
        let tmp = TempDir::new().unwrap();
        let (registry, _idl_path) = registry_with_idl(&tmp);
        let bus = Arc::new(LogBus::new(Arc::new(registry)));
        let source = RecordingSource::default();
        let report = run_local(
            &source,
            Arc::clone(&bus),
            &IndexerRunRequest {
                cluster: ClusterKind::Localnet,
                program_ids: vec!["Prog1111111111111111111111111111111111111111".into()],
                last_n: 10,
                rpc_url: Some("http://rpc.test".into()),
            },
            |_| Some("http://rpc.test".into()),
        )
        .unwrap();
        assert_eq!(report.fetched_signatures, 2);
        assert_eq!(report.events_by_program.len(), 1);
        assert_eq!(report.events_by_program[0].transactions, 2);
        assert_eq!(report.events_by_program[0].anchor_events, 1);
        assert_eq!(report.entries.len(), 2);
    }

    #[test]
    fn run_local_refuses_empty_program_list() {
        let registry = IdlRegistry::new(Arc::new(NoopFetcher));
        let bus = Arc::new(LogBus::new(Arc::new(registry)));
        let source = RecordingSource::default();
        let err = run_local(
            &source,
            bus,
            &IndexerRunRequest {
                cluster: ClusterKind::Localnet,
                program_ids: vec![],
                last_n: 10,
                rpc_url: Some("http://rpc.test".into()),
            },
            |_| None,
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_indexer_run_no_programs");
    }

    #[test]
    fn run_local_rejects_bad_last_n() {
        let registry = IdlRegistry::new(Arc::new(NoopFetcher));
        let bus = Arc::new(LogBus::new(Arc::new(registry)));
        let source = RecordingSource::default();
        let err = run_local(
            &source,
            bus,
            &IndexerRunRequest {
                cluster: ClusterKind::Localnet,
                program_ids: vec!["Prog".into()],
                last_n: 0,
                rpc_url: Some("http://rpc.test".into()),
            },
            |_| None,
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_indexer_run_bad_last_n");
    }

    #[test]
    fn run_local_requires_rpc_url() {
        let registry = IdlRegistry::new(Arc::new(NoopFetcher));
        let bus = Arc::new(LogBus::new(Arc::new(registry)));
        let source = RecordingSource::default();
        let err = run_local(
            &source,
            bus,
            &IndexerRunRequest {
                cluster: ClusterKind::Localnet,
                program_ids: vec!["Prog".into()],
                last_n: 5,
                rpc_url: None,
            },
            |_| None,
        )
        .unwrap_err();
        assert_eq!(err.code, "solana_indexer_run_no_rpc");
    }
}
