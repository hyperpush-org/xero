use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use xero_agent_core::{
    domain_tool_pack_health_report, domain_tool_pack_manifest, domain_tool_pack_manifests,
    provider_capability_catalog, provider_preflight_blockers, provider_preflight_snapshot,
    provider_preflight_snapshot_as_cached_probe, run_openai_compatible_provider_preflight_probe,
    AgentCoreStore, AgentRuntimeFacade, ContextManifest, DomainToolPackHealthInput,
    DomainToolPackHealthReport, DomainToolPackHealthStatus, DomainToolPackManifest,
    EnvironmentSemanticIndexState, FileAgentCoreStore, HeadlessProviderExecutionConfig,
    HeadlessProviderRuntime, HeadlessRuntimeOptions, MessageRole, NewContextManifest,
    NewMessageRecord, NewRunRecord, NewRuntimeEvent, OpenAiCompatibleHeadlessConfig,
    OpenAiCompatibleProviderPreflightProbeRequest, PermissionProfileSandbox,
    ProductionRuntimeContract, ProjectTrustState, ProviderCapabilityCatalog,
    ProviderCapabilityCatalogInput, ProviderPreflightInput, ProviderPreflightRequiredFeatures,
    ProviderPreflightSnapshot, ProviderPreflightSource, ProviderSelection, RunControls,
    RunSnapshot, RunStatus, RunSummary, RuntimeEvent, RuntimeEventKind, RuntimeMessage,
    RuntimeMessageProviderMetadata, RuntimeStoreDescriptor, RuntimeTrace, RuntimeTraceContext,
    SandboxApprovalSource, SandboxExecutionContext, SandboxExecutionMetadata, SandboxPlatform,
    SandboxedProcessRequest, SandboxedProcessRunner, StartRunRequest, ToolApprovalRequirement,
    ToolCallInput, ToolDescriptorV2, ToolEffectClass, ToolExecutionContext, ToolMutability,
    ToolSandbox, ToolSandboxRequirement, DEFAULT_PROVIDER_CATALOG_TTL_SECONDS,
    DOMAIN_TOOL_PACK_CONTRACT_VERSION,
};

const APP_DATA_DIRECTORY_NAME: &str = "dev.sn0w.xero";
const HEADLESS_DIRECTORY_NAME: &str = "headless";
const AGENT_CORE_STATE_FILE: &str = "agent-core-runs.json";
const CLI_CONFIG_FILE: &str = "cli-config.json";
const PROVIDER_PREFLIGHT_STATE_FILE: &str = "provider-preflight-results.json";
const GLOBAL_DATABASE_FILE: &str = "xero.db";
const PROJECTS_DIRECTORY: &str = "projects";
const STATE_DATABASE_FILE: &str = "state.db";
const FAKE_PROVIDER_ID: &str = "fake_provider";
const DEFAULT_MODEL_ID: &str = "fake-model";
const DEFAULT_PROJECT_ID: &str = "headless-local";
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MCP_SERVER_NAME: &str = "xero-local-harness";
const WORKSPACE_INDEX_VERSION: u32 = 1;
const DEFAULT_INDEX_FILE_LIMIT: usize = 5_000;
const HARD_INDEX_FILE_LIMIT: usize = 20_000;
const MAX_INDEX_FILE_BYTES: u64 = 512 * 1024;
const DEFAULT_WORKSPACE_QUERY_LIMIT: usize = 8;
const MAX_QUERY_RESULTS: usize = 20;
const MAX_INDEX_SNIPPET_CHARS: usize = 1_200;
const MAX_WORKSPACE_FEATURES: usize = 40;
const MAX_WORKSPACE_IMPORTS: usize = 32;
const MAX_WORKSPACE_SYMBOLS: usize = 64;
const MAX_WORKSPACE_TESTS: usize = 32;
const WORKSPACE_EMBEDDING_DIM: usize = 768;
const WORKSPACE_EMBEDDING_MODEL: &str = "xero-local-hash-embedding";
const WORKSPACE_EMBEDDING_VERSION: &str = "xero-local-hash-embedding.v1";
const EXTERNAL_AGENT_DEFAULT_TIMEOUT_MS: u64 = 10 * 60 * 1_000;
const XERO_BENCHMARK_ADAPTER_VERSION: &str = "xero-terminal-bench-harbor-adapter.v1";
const XERO_BENCHMARK_PROMPT_VERSION: &str = "xero-terminal-bench-prompt.v1";
const XERO_BENCHMARK_TOOL_POLICY_VERSION: &str = "owned-agent-core-tool-registry-v2";

const BENCHMARK_PROJECT_SCHEMA: &str = r#"
    PRAGMA foreign_keys = ON;
    PRAGMA journal_mode = WAL;
    CREATE TABLE IF NOT EXISTS projects (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        milestone TEXT NOT NULL DEFAULT '',
        total_phases INTEGER NOT NULL DEFAULT 0 CHECK (total_phases >= 0),
        completed_phases INTEGER NOT NULL DEFAULT 0 CHECK (completed_phases >= 0),
        active_phase INTEGER NOT NULL DEFAULT 0 CHECK (active_phase >= 0),
        branch TEXT,
        runtime TEXT,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );
    CREATE TABLE IF NOT EXISTS repositories (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        root_path TEXT NOT NULL UNIQUE,
        display_name TEXT NOT NULL,
        branch TEXT,
        head_sha TEXT,
        is_git_repo INTEGER NOT NULL DEFAULT 1 CHECK (is_git_repo IN (0, 1)),
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );
    CREATE TABLE IF NOT EXISTS agent_definitions (
        definition_id TEXT PRIMARY KEY,
        current_version INTEGER NOT NULL,
        display_name TEXT NOT NULL,
        short_label TEXT NOT NULL,
        scope TEXT NOT NULL,
        lifecycle_state TEXT NOT NULL,
        base_capability_profile TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS agent_definition_versions (
        definition_id TEXT NOT NULL,
        version INTEGER NOT NULL,
        snapshot_json TEXT NOT NULL,
        validation_report_json TEXT,
        created_at TEXT NOT NULL,
        PRIMARY KEY (definition_id, version),
        FOREIGN KEY (definition_id) REFERENCES agent_definitions(definition_id)
    );
    INSERT OR IGNORE INTO agent_definitions (
        definition_id,
        current_version,
        display_name,
        short_label,
        scope,
        lifecycle_state,
        base_capability_profile,
        updated_at
    )
    VALUES ('engineer', 1, 'Engineer', 'Build', 'built_in', 'active', 'engineering', '2026-05-01T00:00:00Z');
    INSERT OR IGNORE INTO agent_definition_versions (
        definition_id,
        version,
        snapshot_json,
        validation_report_json,
        created_at
    )
    VALUES ('engineer', 1, '{"id":"engineer","version":1}', '{"status":"valid","source":"benchmark_seed"}', '2026-05-01T00:00:00Z');
    CREATE TABLE IF NOT EXISTS agent_sessions (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        agent_session_id TEXT NOT NULL,
        title TEXT NOT NULL,
        summary TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL,
        selected INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        archived_at TEXT,
        last_run_id TEXT,
        last_runtime_kind TEXT,
        last_provider_id TEXT,
        PRIMARY KEY (project_id, agent_session_id)
    );
    CREATE TABLE IF NOT EXISTS agent_runs (
        runtime_agent_id TEXT NOT NULL,
        agent_definition_id TEXT NOT NULL,
        agent_definition_version INTEGER NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        agent_session_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        trace_id TEXT NOT NULL,
        lineage_kind TEXT NOT NULL DEFAULT 'top_level',
        parent_run_id TEXT,
        parent_trace_id TEXT,
        parent_subagent_id TEXT,
        subagent_role TEXT,
        provider_id TEXT NOT NULL,
        model_id TEXT NOT NULL,
        status TEXT NOT NULL,
        prompt TEXT NOT NULL,
        system_prompt TEXT NOT NULL,
        started_at TEXT NOT NULL,
        last_heartbeat_at TEXT,
        completed_at TEXT,
        cancelled_at TEXT,
        last_error_code TEXT,
        last_error_message TEXT,
        updated_at TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        PRIMARY KEY (project_id, run_id),
        FOREIGN KEY (project_id, agent_session_id) REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (agent_definition_id, agent_definition_version) REFERENCES agent_definition_versions(definition_id, version)
    );
    CREATE TABLE IF NOT EXISTS agent_messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        role TEXT NOT NULL,
        content TEXT NOT NULL,
        provider_metadata_json TEXT,
        created_at TEXT NOT NULL,
        FOREIGN KEY (project_id, run_id) REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );
    CREATE INDEX IF NOT EXISTS idx_agent_messages_project_run_id
        ON agent_messages(project_id, run_id, id ASC);
    CREATE TABLE IF NOT EXISTS agent_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        event_kind TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        FOREIGN KEY (project_id, run_id) REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );
    CREATE TABLE IF NOT EXISTS agent_context_manifests (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        manifest_id TEXT NOT NULL UNIQUE,
        project_id TEXT NOT NULL,
        agent_session_id TEXT NOT NULL,
        run_id TEXT,
        runtime_agent_id TEXT NOT NULL,
        agent_definition_id TEXT NOT NULL,
        agent_definition_version INTEGER NOT NULL,
        provider_id TEXT,
        model_id TEXT,
        request_kind TEXT NOT NULL,
        policy_action TEXT NOT NULL,
        policy_reason_code TEXT NOT NULL,
        budget_tokens INTEGER,
        estimated_tokens INTEGER NOT NULL DEFAULT 0,
        pressure TEXT NOT NULL,
        context_hash TEXT NOT NULL,
        included_contributors_json TEXT NOT NULL,
        excluded_contributors_json TEXT NOT NULL,
        retrieval_query_ids_json TEXT NOT NULL,
        retrieval_result_ids_json TEXT NOT NULL,
        compaction_id TEXT,
        handoff_id TEXT,
        redaction_state TEXT NOT NULL,
        manifest_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        FOREIGN KEY (project_id, agent_session_id) REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, run_id) REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (agent_definition_id, agent_definition_version) REFERENCES agent_definition_versions(definition_id, version)
    );
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Text,
    Json,
}

#[derive(Debug, Clone)]
pub struct CliResponse {
    pub output_mode: OutputMode,
    pub text: String,
    pub json: JsonValue,
    emit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub code: String,
    pub message: String,
    pub exit_code: i32,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: "xero_cli_usage".into(),
            message: message.into(),
            exit_code: 2,
        }
    }

    fn user_fixable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            exit_code: 1,
        }
    }

    fn system_fault(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            exit_code: 1,
        }
    }

    fn benchmark(code: impl Into<String>, message: impl Into<String>, exit_code: i32) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            exit_code,
        }
    }
}

#[derive(Debug, Clone)]
struct GlobalOptions {
    output_mode: OutputMode,
    ci: bool,
    state_dir: PathBuf,
}

pub fn run_from_env() -> i32 {
    let args = env::args().collect::<Vec<_>>();
    let output_mode = requested_output_mode(&args);
    match run_with_args(args) {
        Ok(response) => {
            if response.emit {
                emit_response(&response);
            }
            0
        }
        Err(error) => {
            emit_error(&error, output_mode);
            error.exit_code
        }
    }
}

pub fn run_with_args<I, S>(args: I) -> Result<CliResponse, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let raw_args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let (globals, command_args) = parse_global_options(raw_args)?;
    dispatch(globals, command_args)
}

fn dispatch(globals: GlobalOptions, args: Vec<String>) -> Result<CliResponse, CliError> {
    if args.is_empty()
        || args
            .first()
            .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(response(&globals, root_help(), root_help_json()));
    }

    match args.first().map(String::as_str) {
        Some("--version") | Some("version") => Ok(response(
            &globals,
            format!("xero-cli {}", env!("CARGO_PKG_VERSION")),
            json!({
                "kind": "version",
                "name": "xero-cli",
                "version": env!("CARGO_PKG_VERSION"),
            }),
        )),
        Some("agent") => dispatch_agent(globals, args[1..].to_vec()),
        Some("benchmark") => dispatch_benchmark(globals, args[1..].to_vec()),
        Some("conversation") => dispatch_conversation(globals, args[1..].to_vec()),
        Some("provider") => dispatch_provider(globals, args[1..].to_vec()),
        Some("mcp") => dispatch_mcp(globals, args[1..].to_vec()),
        Some("workspace") => dispatch_workspace(globals, args[1..].to_vec()),
        Some("tool-pack") => dispatch_tool_pack(globals, args[1..].to_vec()),
        Some("commit-message") => command_commit_message(globals, args[1..].to_vec()),
        Some("suggest-command") => command_suggest_command(globals, args[1..].to_vec()),
        Some("daemon") => command_daemon(globals, args[1..].to_vec()),
        Some(other) => Err(CliError::usage(format!(
            "Unknown xero command `{other}`. Run `xero --help`."
        ))),
        None => Ok(response(&globals, root_help(), root_help_json())),
    }
}

fn dispatch_benchmark(globals: GlobalOptions, args: Vec<String>) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("terminal-bench") | Some("run") => {
            command_benchmark_terminal_bench(globals, args[1..].to_vec())
        }
        Some(other) => Err(CliError::usage(format!(
            "Unknown benchmark command `{other}`. Use `xero benchmark terminal-bench`."
        ))),
        None => Err(CliError::usage(
            "Missing benchmark command. Use `xero benchmark terminal-bench`.",
        )),
    }
}

fn dispatch_agent(globals: GlobalOptions, args: Vec<String>) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("exec") => command_agent_exec(globals, args[1..].to_vec()),
        Some("host") => command_agent_host(globals, args[1..].to_vec()),
        Some(other) => Err(CliError::usage(format!(
            "Unknown agent command `{other}`. Run `xero agent exec --help` or `xero agent host --help`."
        ))),
        None => Err(CliError::usage(
            "Missing agent command. Use `xero agent exec` or `xero agent host`.",
        )),
    }
}

fn dispatch_conversation(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") => command_conversation_list(globals, args[1..].to_vec()),
        Some("show") => command_conversation_show(globals, args[1..].to_vec()),
        Some("dump") => command_conversation_dump(globals, args[1..].to_vec()),
        Some("support-bundle") => command_conversation_support_bundle(globals, args[1..].to_vec()),
        Some("compact") => command_conversation_compact(globals, args[1..].to_vec()),
        Some("retry") => command_conversation_retry(globals, args[1..].to_vec()),
        Some("clone") => command_conversation_clone(globals, args[1..].to_vec()),
        Some("stats") => command_conversation_stats(globals, args[1..].to_vec()),
        Some(other) => Err(CliError::usage(format!(
            "Unknown conversation command `{other}`."
        ))),
        None => Err(CliError::usage(
            "Missing conversation command. Use list, show, dump, support-bundle, compact, retry, clone, or stats.",
        )),
    }
}

fn dispatch_provider(globals: GlobalOptions, args: Vec<String>) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") => command_provider_list(globals, args[1..].to_vec()),
        Some("login") => command_provider_login(globals, args[1..].to_vec()),
        Some("doctor") => command_provider_doctor(globals, args[1..].to_vec()),
        Some("preflight") => command_provider_preflight(globals, args[1..].to_vec()),
        Some(other) => Err(CliError::usage(format!(
            "Unknown provider command `{other}`."
        ))),
        None => Err(CliError::usage(
            "Missing provider command. Use list, login, doctor, or preflight.",
        )),
    }
}

fn dispatch_mcp(globals: GlobalOptions, args: Vec<String>) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") => command_mcp_list(globals, args[1..].to_vec()),
        Some("add") => command_mcp_add(globals, args[1..].to_vec()),
        Some("login") => command_mcp_login(globals, args[1..].to_vec()),
        Some("serve") => command_mcp_serve(globals, args[1..].to_vec()),
        Some(other) => Err(CliError::usage(format!("Unknown MCP command `{other}`."))),
        None => Err(CliError::usage(
            "Missing MCP command. Use list, add, login, or serve.",
        )),
    }
}

fn dispatch_workspace(globals: GlobalOptions, args: Vec<String>) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("index") => command_workspace_index(globals, args[1..].to_vec()),
        Some("status") => command_workspace_status(globals, args[1..].to_vec()),
        Some("query") => command_workspace_query(globals, args[1..].to_vec()),
        Some("explain") => command_workspace_explain(globals, args[1..].to_vec()),
        Some("reset") => command_workspace_reset(globals, args[1..].to_vec()),
        Some(other) => Err(CliError::usage(format!(
            "Unknown workspace command `{other}`."
        ))),
        None => Err(CliError::usage(
            "Missing workspace command. Use index, status, query, explain, or reset.",
        )),
    }
}

fn dispatch_tool_pack(globals: GlobalOptions, args: Vec<String>) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") => command_tool_pack_list(globals, args[1..].to_vec()),
        Some("doctor") => command_tool_pack_doctor(globals, args[1..].to_vec()),
        Some(other) => Err(CliError::usage(format!(
            "Unknown tool-pack command `{other}`."
        ))),
        None => Err(CliError::usage(
            "Missing tool-pack command. Use list or doctor.",
        )),
    }
}

fn command_agent_exec(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero agent exec [PROMPT] [--project-id ID] [--session-id ID] [--run-id ID] --provider ID [--model ID]\nRuns a headless owned-agent turn through the shared Xero runtime. Use --provider fake_provider only for harness tests.",
            json!({ "command": "agent exec" }),
        ));
    }

    let prompt_flag = take_option(&mut args, "--prompt")?;
    let project_id = take_option(&mut args, "--project-id")?;
    let agent_session_id =
        take_option(&mut args, "--session-id")?.unwrap_or_else(|| generate_id("session"));
    let run_id = take_option(&mut args, "--run-id")?.unwrap_or_else(|| generate_id("run"));
    let provider_id = take_option(&mut args, "--provider")?;
    let model_id = take_option(&mut args, "--model")?;
    reject_unknown_options(&args)?;

    let prompt = prompt_flag
        .or_else(|| (!args.is_empty()).then(|| args.join(" ")))
        .ok_or_else(|| CliError::usage("Missing prompt. Use `xero agent exec \"...\"`."))?;
    if prompt.trim().is_empty() {
        return Err(CliError::usage("Prompt cannot be empty."));
    }

    let provider = resolve_cli_provider_execution(&globals, provider_id, model_id)?;
    let (project_id, store) = open_run_store_for_provider(&globals, project_id, &provider)?;
    let provider = provider.with_project_workspace(&store);
    if provider.execution_mode == "real_provider" {
        ensure_cli_real_provider_runtime_contract(&project_id, &provider, &store)?;
    }
    let provider_preflight = ensure_cli_provider_preflight_for_run(&globals, &provider)?;
    let runtime = HeadlessProviderRuntime::new(
        store.clone(),
        provider.execution.clone(),
        HeadlessRuntimeOptions {
            ci_mode: globals.ci,
            provider_preflight: Some(provider_preflight.clone()),
            ..HeadlessRuntimeOptions::default()
        },
    );
    let snapshot = runtime
        .start_run(StartRunRequest {
            project_id: project_id.clone(),
            agent_session_id,
            run_id,
            prompt: prompt.clone(),
            provider: ProviderSelection {
                provider_id: provider.provider_id.clone(),
                model_id: provider.model_id.clone(),
            },
            controls: Some(RunControls {
                runtime_agent_id: "engineer".into(),
                approval_mode: if globals.ci { "strict" } else { "on_request" }.into(),
                plan_mode_required: globals.ci,
            }),
        })
        .map_err(core_error)?;

    let assistant = last_assistant_message(&snapshot).unwrap_or_default();
    let text = format!(
        "Headless run {} finished with status {:?}.\nProvider: {}/{}\nTrace: {}\nAssistant: {}\nState: {}",
        snapshot.run_id,
        snapshot.status,
        snapshot.provider_id,
        snapshot.model_id,
        snapshot.trace_id,
        assistant,
        store.path().display()
    );
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "agentExec",
            "executionMode": provider.execution_mode,
            "ciMode": globals.ci,
            "sandboxDefaults": sandbox_defaults_json(globals.ci),
            "storePath": store.path(),
            "providerPreflight": provider_preflight,
            "snapshot": snapshot,
        }),
    ))
}

#[derive(Debug, Clone)]
struct BenchmarkRunConfig {
    instruction: String,
    workspace_root: PathBuf,
    trial_app_data_root: PathBuf,
    output_dir: PathBuf,
    project_id: String,
    agent_session_id: String,
    run_id: String,
    benchmark_name: String,
    dataset_id: String,
    dataset_digest: Option<String>,
    task_id: String,
    attempt_index: u64,
    provider_id: String,
    model_id: String,
    profile_id: Option<String>,
    api_key_env: Option<String>,
    base_url: Option<String>,
    temperature: Option<String>,
    reasoning_effort: Option<String>,
    max_output_tokens: Option<u64>,
    context_budget: Option<u64>,
    wall_time_seconds: Option<u64>,
    max_turns: Option<usize>,
    max_tool_calls: Option<u64>,
    max_command_calls: Option<u64>,
    max_cost_usd: Option<f64>,
    approval_mode: String,
    sandbox_policy: String,
    network_policy: String,
    sandbox_provider: String,
    environment_id: Option<String>,
    image_digest: Option<String>,
    prompt_version: String,
    tool_policy_version: String,
    adapter_version: String,
    harness_version: String,
    xero_source_revision: Option<String>,
    comparison_mode: String,
    provider_account_class: String,
    endpoint_class: String,
    allow_fake_provider_fixture: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkArtifactSummary {
    output_dir: String,
    manifest: String,
    trajectory: String,
    xero_trace: String,
    final_diff: String,
    support_bundle: String,
    stdout: String,
    stderr: String,
}

fn command_benchmark_terminal_bench(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            [
                "Usage: xero benchmark terminal-bench --instruction-file PATH --workspace-root PATH --trial-app-data-root PATH --output-dir PATH --task-id ID --dataset-id ID --provider ID --model ID [options]",
                "",
                "Runs one Harbor/Terminal-Bench trial through Xero's headless owned-agent runtime and writes manifest.json, trajectory.json, xero-trace.json, final.diff, support-bundle.zip, stdout.txt, and stderr.txt.",
                "Use --allow-fake-provider-fixture with --provider fake_provider only for adapter fixture tests.",
            ]
            .join("\n"),
            json!({ "command": "benchmark terminal-bench" }),
        ));
    }

    let config = parse_benchmark_terminal_bench_config(&globals, &mut args)?;
    reject_unknown_options(&args)?;
    ensure_not_legacy_xero_state(&config.trial_app_data_root, "trial app-data root")?;
    ensure_not_legacy_xero_state(&config.output_dir, "benchmark output directory")?;
    fs::create_dir_all(&config.output_dir).map_err(|error| {
        CliError::benchmark(
            "xero_benchmark_output_prepare_failed",
            format!(
                "Could not create benchmark output directory `{}`: {error}",
                config.output_dir.display()
            ),
            1,
        )
    })?;

    let benchmark_globals = GlobalOptions {
        output_mode: globals.output_mode,
        ci: false,
        state_dir: config.trial_app_data_root.clone(),
    };
    let registered_project = ensure_benchmark_project_registered(
        &benchmark_globals,
        &config.project_id,
        &config.workspace_root,
    )?;
    let app_data_store = AppDataProjectAgentStore::open(registered_project)?;
    let store = if config.provider_id == FAKE_PROVIDER_ID {
        CliAgentStore::Harness(open_harness_agent_store(&benchmark_globals)?)
    } else {
        CliAgentStore::AppData(app_data_store)
    };
    let provider = resolve_benchmark_provider_execution(&benchmark_globals, &config)?;
    let provider = provider.with_project_workspace(&store);
    if provider.execution_mode == "real_provider" {
        ensure_cli_real_provider_runtime_contract(&config.project_id, &provider, &store)?;
    }
    let provider_preflight = ensure_cli_provider_preflight_for_run(&benchmark_globals, &provider)?;
    let runtime = HeadlessProviderRuntime::new(
        store.clone(),
        provider.execution.clone(),
        HeadlessRuntimeOptions {
            ci_mode: false,
            max_provider_turns: config
                .max_turns
                .unwrap_or_else(|| HeadlessRuntimeOptions::default().max_provider_turns),
            max_wall_time_ms: config
                .wall_time_seconds
                .map(|seconds| seconds.saturating_mul(1_000)),
            max_tool_calls: config.max_tool_calls,
            max_command_calls: config.max_command_calls,
            provider_preflight: Some(provider_preflight.clone()),
        },
    );

    let started = Instant::now();
    let run_result = runtime.start_run(StartRunRequest {
        project_id: config.project_id.clone(),
        agent_session_id: config.agent_session_id.clone(),
        run_id: config.run_id.clone(),
        prompt: config.instruction.clone(),
        provider: ProviderSelection {
            provider_id: provider.provider_id.clone(),
            model_id: provider.model_id.clone(),
        },
        controls: Some(RunControls {
            runtime_agent_id: "engineer".into(),
            approval_mode: config.approval_mode.clone(),
            plan_mode_required: config.approval_mode == "strict",
        }),
    });
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let (snapshot, run_error) = match run_result {
        Ok(snapshot) => (Some(snapshot), None),
        Err(error) => {
            let snapshot = store.load_run(&config.project_id, &config.run_id).ok();
            (snapshot, Some(error))
        }
    };
    let run_error_cli = run_error.as_ref().map(benchmark_runtime_error_to_cli);
    let artifacts = write_benchmark_trial_artifacts(
        &config,
        &store,
        snapshot.as_ref(),
        run_error_cli.as_ref(),
        elapsed_ms,
    )?;

    if let Some(error) = run_error_cli {
        return Err(error);
    }
    let snapshot = snapshot.ok_or_else(|| {
        CliError::benchmark(
            "xero_benchmark_run_missing",
            "The benchmark run finished without a runtime snapshot.",
            4,
        )
    })?;
    if snapshot.status != RunStatus::Completed {
        return Err(CliError::benchmark(
            "xero_benchmark_agent_incomplete",
            format!(
                "Benchmark run `{}` ended with status {:?}. Artifacts were written to `{}`.",
                snapshot.run_id,
                snapshot.status,
                config.output_dir.display()
            ),
            4,
        ));
    }

    let text = format!(
        "Benchmark trial {} completed for task {}. Artifacts: {}",
        config.run_id,
        config.task_id,
        config.output_dir.display()
    );
    Ok(response(
        &globals,
        text.clone(),
        json!({
            "kind": "benchmarkTerminalBench",
            "status": "completed",
            "runId": config.run_id,
            "taskId": config.task_id,
            "stdout": text,
            "artifacts": artifacts,
            "providerPreflight": provider_preflight,
        }),
    ))
}

fn parse_benchmark_terminal_bench_config(
    globals: &GlobalOptions,
    args: &mut Vec<String>,
) -> Result<BenchmarkRunConfig, CliError> {
    let instruction = read_benchmark_instruction(args)?;
    let workspace_root = required_existing_path_option(args, "--workspace-root")?;
    let output_dir = required_path_option(args, "--output-dir")?;
    let trial_app_data_root = match take_option(args, "--trial-app-data-root")? {
        Some(value) => Some(value),
        None => take_option(args, "--app-data-root")?,
    }
    .map(PathBuf::from)
    .unwrap_or_else(|| globals.state_dir.clone());
    let project_id = take_option(args, "--project-id")?
        .unwrap_or_else(|| stable_project_id_for_repo_root(&workspace_root));
    let agent_session_id =
        take_option(args, "--session-id")?.unwrap_or_else(|| generate_id("benchmark-session"));
    let run_id = take_option(args, "--run-id")?.unwrap_or_else(|| generate_id("benchmark-run"));
    let benchmark_name =
        take_option(args, "--benchmark")?.unwrap_or_else(|| "terminal-bench".into());
    let dataset_id = take_option(args, "--dataset-id")?
        .or_else(|| env::var("TERMINAL_BENCH_DATASET").ok())
        .ok_or_else(|| CliError::usage("Missing `--dataset-id`."))?;
    let dataset_digest = take_option(args, "--dataset-digest")?;
    let task_id =
        take_option(args, "--task-id")?.ok_or_else(|| CliError::usage("Missing `--task-id`."))?;
    let attempt_index = take_option(args, "--attempt-index")?
        .map(|value| parse_nonnegative_u64(&value, "--attempt-index"))
        .transpose()?
        .unwrap_or(0);
    let provider_id = normalize_benchmark_provider_id(
        &take_option(args, "--provider")?
            .ok_or_else(|| CliError::usage("Missing `--provider`."))?,
    );
    let model_id = take_option(args, "--model")?
        .or_else(|| provider_catalog_entry(&provider_id).map(|entry| entry.default_model.into()))
        .ok_or_else(|| CliError::usage("Missing `--model`."))?;
    let profile_id = take_option(args, "--profile-id")?;
    let base_url = take_option(args, "--base-url")?;
    let api_key_env = take_option(args, "--api-key-env")?.or_else(|| {
        let local_endpoint = base_url.as_deref().is_some_and(is_local_provider_base_url);
        (!local_endpoint)
            .then(|| default_benchmark_api_key_env(&provider_id).map(str::to_owned))
            .flatten()
    });
    let temperature = take_option(args, "--temperature")?;
    let reasoning_effort = take_option(args, "--reasoning-effort")?;
    let max_output_tokens = take_option(args, "--max-output-tokens")?
        .map(|value| parse_positive_u64(&value, "--max-output-tokens"))
        .transpose()?;
    let context_budget = take_option(args, "--context-budget")?
        .map(|value| parse_positive_u64(&value, "--context-budget"))
        .transpose()?;
    let wall_time_seconds = match take_option(args, "--wall-time-seconds")? {
        Some(value) => Some(value),
        None => take_option(args, "--timeout-seconds")?,
    }
    .map(|value| parse_positive_u64(&value, "--wall-time-seconds"))
    .transpose()?;
    let max_turns = take_option(args, "--max-turns")?
        .map(|value| parse_positive_usize(&value, "--max-turns"))
        .transpose()?;
    let max_tool_calls = take_option(args, "--max-tool-calls")?
        .map(|value| parse_positive_u64(&value, "--max-tool-calls"))
        .transpose()?;
    let max_command_calls = take_option(args, "--max-command-calls")?
        .map(|value| parse_positive_u64(&value, "--max-command-calls"))
        .transpose()?;
    let max_cost_usd = take_option(args, "--max-cost-usd")?
        .map(|value| parse_nonnegative_f64(&value, "--max-cost-usd"))
        .transpose()?;
    let approval_mode = take_option(args, "--approval-mode")?.unwrap_or_else(|| "strict".into());
    let sandbox_policy =
        take_option(args, "--sandbox-policy")?.unwrap_or_else(|| "harbor_task_sandbox".into());
    let network_policy =
        take_option(args, "--network-policy")?.unwrap_or_else(|| "harbor_controlled".into());
    let sandbox_provider =
        take_option(args, "--sandbox-provider")?.unwrap_or_else(|| "harbor".into());
    let environment_id = take_option(args, "--environment-id")?;
    let image_digest = take_option(args, "--image-digest")?;
    let prompt_version = take_option(args, "--prompt-version")?
        .unwrap_or_else(|| XERO_BENCHMARK_PROMPT_VERSION.into());
    let tool_policy_version = take_option(args, "--tool-policy-version")?
        .unwrap_or_else(|| XERO_BENCHMARK_TOOL_POLICY_VERSION.into());
    let adapter_version = take_option(args, "--adapter-version")?
        .unwrap_or_else(|| XERO_BENCHMARK_ADAPTER_VERSION.into());
    let harness_version =
        take_option(args, "--harness-version")?.unwrap_or_else(|| "harbor".into());
    let xero_source_revision = take_option(args, "--xero-source-revision")?
        .or_else(|| env::var("XERO_SOURCE_REVISION").ok());
    let comparison_mode =
        take_option(args, "--comparison-mode")?.unwrap_or_else(|| "fixed-model".into());
    let provider_account_class =
        take_option(args, "--provider-account-class")?.unwrap_or_else(|| "unspecified".into());
    let endpoint_class =
        take_option(args, "--endpoint-class")?.unwrap_or_else(|| "openai-compatible".into());
    let allow_fake_provider_fixture = take_bool_flag(args, "--allow-fake-provider-fixture");
    if provider_id == FAKE_PROVIDER_ID && !allow_fake_provider_fixture {
        return Err(CliError::usage(
            "`--provider fake_provider` is fixture-only for benchmarks. Add `--allow-fake-provider-fixture` for adapter smoke tests.",
        ));
    }
    if instruction.trim().is_empty() {
        return Err(CliError::usage("Benchmark instruction cannot be empty."));
    }

    Ok(BenchmarkRunConfig {
        instruction,
        workspace_root,
        trial_app_data_root,
        output_dir,
        project_id,
        agent_session_id,
        run_id,
        benchmark_name,
        dataset_id,
        dataset_digest,
        task_id,
        attempt_index,
        provider_id,
        model_id,
        profile_id,
        api_key_env,
        base_url,
        temperature,
        reasoning_effort,
        max_output_tokens,
        context_budget,
        wall_time_seconds,
        max_turns,
        max_tool_calls,
        max_command_calls,
        max_cost_usd,
        approval_mode,
        sandbox_policy,
        network_policy,
        sandbox_provider,
        environment_id,
        image_digest,
        prompt_version,
        tool_policy_version,
        adapter_version,
        harness_version,
        xero_source_revision,
        comparison_mode,
        provider_account_class,
        endpoint_class,
        allow_fake_provider_fixture,
    })
}

fn read_benchmark_instruction(args: &mut Vec<String>) -> Result<String, CliError> {
    let instruction = take_option(args, "--instruction")?;
    let instruction_file = take_option(args, "--instruction-file")?;
    match (instruction, instruction_file) {
        (Some(_), Some(_)) => Err(CliError::usage(
            "Use either `--instruction` or `--instruction-file`, not both.",
        )),
        (Some(value), None) => Ok(value),
        (None, Some(path)) if path == "-" => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input).map_err(|error| {
                CliError::benchmark(
                    "xero_benchmark_instruction_read_failed",
                    format!("Could not read benchmark instruction from stdin: {error}"),
                    2,
                )
            })?;
            Ok(input)
        }
        (None, Some(path)) => fs::read_to_string(&path).map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_instruction_read_failed",
                format!("Could not read benchmark instruction file `{path}`: {error}"),
                2,
            )
        }),
        (None, None) if !args.is_empty() && !args[0].starts_with('-') => Ok(args.remove(0)),
        (None, None) => Err(CliError::usage(
            "Missing benchmark instruction. Use `--instruction`, `--instruction-file`, or stdin with `--instruction-file -`.",
        )),
    }
}

fn required_path_option(args: &mut Vec<String>, name: &str) -> Result<PathBuf, CliError> {
    let value =
        take_option(args, name)?.ok_or_else(|| CliError::usage(format!("Missing `{name}`.")))?;
    Ok(PathBuf::from(value))
}

fn required_existing_path_option(args: &mut Vec<String>, name: &str) -> Result<PathBuf, CliError> {
    let value =
        take_option(args, name)?.ok_or_else(|| CliError::usage(format!("Missing `{name}`.")))?;
    canonicalize_existing_path(&value)
}

fn parse_positive_usize(value: &str, name: &str) -> Result<usize, CliError> {
    let parsed = parse_positive_u64(value, name)?;
    usize::try_from(parsed).map_err(|_| CliError::usage(format!("`{name}` is too large.")))
}

fn parse_nonnegative_u64(value: &str, name: &str) -> Result<u64, CliError> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| CliError::usage(format!("`{name}` must be a non-negative integer.")))
}

fn parse_nonnegative_f64(value: &str, name: &str) -> Result<f64, CliError> {
    let parsed = value
        .trim()
        .parse::<f64>()
        .map_err(|_| CliError::usage(format!("`{name}` must be a number.")))?;
    if parsed < 0.0 || !parsed.is_finite() {
        return Err(CliError::usage(format!(
            "`{name}` must be a finite non-negative number."
        )));
    }
    Ok(parsed)
}

fn normalize_benchmark_provider_id(provider_id: &str) -> String {
    match provider_id.trim() {
        "openai" => "openai_api".into(),
        "google" | "gemini" => "gemini_ai_studio".into(),
        "github" => "github_models".into(),
        other => other.into(),
    }
}

fn default_benchmark_api_key_env(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "openai_api" => Some("OPENAI_API_KEY"),
        "openrouter" => Some("OPENROUTER_API_KEY"),
        "github_models" => Some("GITHUB_TOKEN"),
        "gemini_ai_studio" => Some("GEMINI_API_KEY"),
        _ => None,
    }
}

fn resolve_benchmark_provider_execution(
    globals: &GlobalOptions,
    config: &BenchmarkRunConfig,
) -> Result<CliProviderExecution, CliError> {
    if config.provider_id == FAKE_PROVIDER_ID {
        return Ok(CliProviderExecution {
            profile_id: FAKE_PROVIDER_ID.into(),
            provider_id: FAKE_PROVIDER_ID.into(),
            model_id: config.model_id.clone(),
            execution_mode: "fake_provider_harness",
            execution: HeadlessProviderExecutionConfig::Fake,
            credential_proof: Some("fixture_only_none_required".into()),
        });
    }

    ensure_owned_agent_provider(&config.provider_id)?;
    let entry = provider_catalog_entry(&config.provider_id).ok_or_else(|| {
        CliError::benchmark(
            "xero_benchmark_provider_unknown",
            format!(
                "Provider `{}` is not in Xero's headless provider catalog.",
                config.provider_id
            ),
            2,
        )
    })?;
    let profile = ProviderProfile {
        profile_id: config
            .profile_id
            .clone()
            .unwrap_or_else(|| format!("benchmark-{}", config.provider_id)),
        provider_id: config.provider_id.clone(),
        api_key_env: config.api_key_env.clone(),
        base_url: config.base_url.clone(),
        recorded_at: now_timestamp(),
    };
    let execution = openai_compatible_execution_config(globals, &profile, &config.model_id)?;
    Ok(CliProviderExecution {
        profile_id: profile.profile_id.clone(),
        provider_id: profile.provider_id.clone(),
        model_id: config.model_id.clone(),
        execution_mode: "real_provider",
        execution,
        credential_proof: provider_credential_proof_for_entry(entry, Some(&profile)),
    })
}

fn ensure_benchmark_project_registered(
    globals: &GlobalOptions,
    project_id: &str,
    repo_root: &Path,
) -> Result<RegisteredProject, CliError> {
    validate_required_cli(project_id, "projectId")?;
    ensure_not_legacy_xero_state(&cli_app_data_root(globals), "trial app-data root")?;
    let app_data_root = cli_app_data_root(globals);
    fs::create_dir_all(&app_data_root).map_err(|error| {
        CliError::benchmark(
            "xero_benchmark_app_data_prepare_failed",
            format!(
                "Could not create benchmark app-data root `{}`: {error}",
                app_data_root.display()
            ),
            1,
        )
    })?;

    let global_database = global_database_path(globals);
    let global = Connection::open(&global_database).map_err(|error| {
        CliError::benchmark(
            "xero_benchmark_registry_open_failed",
            format!(
                "Could not open benchmark app-data registry `{}`: {error}",
                global_database.display()
            ),
            1,
        )
    })?;
    global
        .execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS repositories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                root_path TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_registry_schema_failed",
                format!("Could not prepare benchmark app-data registry schema: {error}"),
                1,
            )
        })?;
    let now = now_timestamp();
    let display_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(project_id);
    global
        .execute(
            "INSERT INTO projects (id, name, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at",
            params![project_id, display_name, now],
        )
        .map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_registry_project_write_failed",
                format!("Could not register benchmark project `{project_id}`: {error}"),
                1,
            )
        })?;
    global
        .execute(
            "INSERT INTO repositories (id, project_id, root_path, display_name, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(root_path) DO UPDATE SET
                project_id = excluded.project_id,
                display_name = excluded.display_name,
                updated_at = excluded.updated_at",
            params![
                format!("repo-{project_id}"),
                project_id,
                repo_root.display().to_string(),
                display_name,
                now_timestamp(),
            ],
        )
        .map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_registry_repository_write_failed",
                format!(
                    "Could not register benchmark workspace `{}`: {error}",
                    repo_root.display()
                ),
                1,
            )
        })?;

    let database_path = workspace_project_database_path(globals, project_id);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_project_state_prepare_failed",
                format!(
                    "Could not create project state directory `{}`: {error}",
                    parent.display()
                ),
                1,
            )
        })?;
    }
    let project = Connection::open(&database_path).map_err(|error| {
        CliError::benchmark(
            "xero_benchmark_project_state_open_failed",
            format!(
                "Could not open benchmark project database `{}`: {error}",
                database_path.display()
            ),
            1,
        )
    })?;
    project
        .execute_batch(BENCHMARK_PROJECT_SCHEMA)
        .map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_project_schema_failed",
                format!("Could not prepare benchmark project schema: {error}"),
                1,
            )
        })?;
    project
        .execute(
            "INSERT INTO projects (id, name, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at",
            params![project_id, display_name, now_timestamp()],
        )
        .map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_project_write_failed",
                format!("Could not seed benchmark project `{project_id}`: {error}"),
                1,
            )
        })?;
    drop(project);
    let _ = open_workspace_index_database(globals, repo_root, project_id)?;

    Ok(RegisteredProject {
        project_id: project_id.into(),
        repo_root: repo_root.to_path_buf(),
        database_path,
    })
}

fn ensure_not_legacy_xero_state(path: &Path, label: &str) -> Result<(), CliError> {
    if path
        .components()
        .any(|component| component.as_os_str().to_string_lossy() == ".xero")
    {
        return Err(CliError::benchmark(
            "xero_benchmark_legacy_state_rejected",
            format!(
                "The {label} `{}` is inside legacy repo-local `.xero` state.",
                path.display()
            ),
            2,
        ));
    }
    Ok(())
}

fn benchmark_runtime_error_to_cli(error: &xero_agent_core::CoreError) -> CliError {
    let exit_code = if is_provider_or_auth_error(&error.code) {
        3
    } else {
        4
    };
    CliError::benchmark(error.code.clone(), error.message.clone(), exit_code)
}

fn is_provider_or_auth_error(code: &str) -> bool {
    let lower = code.to_ascii_lowercase();
    lower.contains("provider") || lower.contains("credential") || lower.contains("api_key")
}

fn write_benchmark_trial_artifacts(
    config: &BenchmarkRunConfig,
    store: &CliAgentStore,
    snapshot: Option<&RunSnapshot>,
    run_error: Option<&CliError>,
    elapsed_ms: u64,
) -> Result<BenchmarkArtifactSummary, CliError> {
    fs::create_dir_all(&config.output_dir).map_err(|error| {
        CliError::benchmark(
            "xero_benchmark_artifact_dir_failed",
            format!(
                "Could not create benchmark artifact directory `{}`: {error}",
                config.output_dir.display()
            ),
            1,
        )
    })?;
    let manifest_path = config.output_dir.join("manifest.json");
    let trajectory_path = config.output_dir.join("trajectory.json");
    let trace_path = config.output_dir.join("xero-trace.json");
    let diff_path = config.output_dir.join("final.diff");
    let support_bundle_path = config.output_dir.join("support-bundle.zip");
    let stdout_path = config.output_dir.join("stdout.txt");
    let stderr_path = config.output_dir.join("stderr.txt");

    let diff = final_workspace_diff(&config.workspace_root).unwrap_or_else(|error| {
        format!(
            "Xero could not collect final diff for `{}`: {}\n",
            config.workspace_root.display(),
            error.message
        )
    });
    fs::write(&diff_path, &diff).map_err(|error| {
        CliError::benchmark(
            "xero_benchmark_diff_write_failed",
            format!("Could not write `{}`: {error}", diff_path.display()),
            1,
        )
    })?;

    if let Some(snapshot) = snapshot {
        let trace = store
            .export_trace(&snapshot.project_id, &snapshot.run_id)
            .map_err(core_error)?;
        write_json_file(&trace_path, &trace)?;
        let trajectory = benchmark_trajectory_json(snapshot, &config.adapter_version);
        write_json_file(&trajectory_path, &trajectory)?;
    } else {
        write_json_file(
            &trace_path,
            &json!({
                "schema": "xero.benchmark.trace.v1",
                "status": "unavailable",
                "reason": run_error.map(|error| error.code.as_str()).unwrap_or("run_not_started"),
            }),
        )?;
        write_json_file(
            &trajectory_path,
            &json!({
                "schema_version": "ATIF-v1.4",
                "session_id": config.run_id,
                "agent": {
                    "name": "xero",
                    "version": env!("CARGO_PKG_VERSION"),
                    "model_name": config.model_id,
                },
                "steps": [],
                "extra": {
                    "conversion_status": "unavailable",
                    "xero_trace_path": "xero-trace.json",
                },
            }),
        )?;
    }

    let stdout_text = benchmark_stdout_text(config, snapshot, elapsed_ms);
    let stderr_text = run_error
        .map(|error| format!("{}: {}\n", error.code, error.message))
        .unwrap_or_default();
    fs::write(&stdout_path, stdout_text).map_err(|error| {
        CliError::benchmark(
            "xero_benchmark_stdout_write_failed",
            format!("Could not write `{}`: {error}", stdout_path.display()),
            1,
        )
    })?;
    fs::write(&stderr_path, stderr_text).map_err(|error| {
        CliError::benchmark(
            "xero_benchmark_stderr_write_failed",
            format!("Could not write `{}`: {error}", stderr_path.display()),
            1,
        )
    })?;

    let manifest = benchmark_manifest_json(
        config,
        snapshot,
        run_error,
        elapsed_ms,
        &diff,
        &BenchmarkArtifactSummary {
            output_dir: config.output_dir.display().to_string(),
            manifest: "manifest.json".into(),
            trajectory: "trajectory.json".into(),
            xero_trace: "xero-trace.json".into(),
            final_diff: "final.diff".into(),
            support_bundle: "support-bundle.zip".into(),
            stdout: "stdout.txt".into(),
            stderr: "stderr.txt".into(),
        },
    );
    write_json_file(&manifest_path, &manifest)?;
    write_support_bundle(
        &support_bundle_path,
        &[
            ("manifest.json", manifest_path.as_path()),
            ("trajectory.json", trajectory_path.as_path()),
            ("xero-trace.json", trace_path.as_path()),
            ("final.diff", diff_path.as_path()),
            ("stdout.txt", stdout_path.as_path()),
            ("stderr.txt", stderr_path.as_path()),
        ],
    )?;

    Ok(BenchmarkArtifactSummary {
        output_dir: config.output_dir.display().to_string(),
        manifest: manifest_path.display().to_string(),
        trajectory: trajectory_path.display().to_string(),
        xero_trace: trace_path.display().to_string(),
        final_diff: diff_path.display().to_string(),
        support_bundle: support_bundle_path.display().to_string(),
        stdout: stdout_path.display().to_string(),
        stderr: stderr_path.display().to_string(),
    })
}

fn benchmark_manifest_json(
    config: &BenchmarkRunConfig,
    snapshot: Option<&RunSnapshot>,
    run_error: Option<&CliError>,
    elapsed_ms: u64,
    diff: &str,
    artifacts: &BenchmarkArtifactSummary,
) -> JsonValue {
    let status = if let Some(error) = run_error {
        if is_provider_or_auth_error(&error.code) {
            "provider_failure"
        } else {
            "agent_failure"
        }
    } else if snapshot
        .map(|snapshot| snapshot.status == RunStatus::Completed)
        .unwrap_or(false)
    {
        "completed"
    } else {
        "incomplete"
    };
    let metrics = snapshot
        .map(|snapshot| benchmark_metrics_json(snapshot, diff, elapsed_ms, &config.workspace_root))
        .unwrap_or_else(|| {
            json!({
                "wallTimeMs": elapsed_ms,
                "costUsd": null,
                "inputTokens": null,
                "outputTokens": null,
                "toolCalls": 0,
                "commandCalls": 0,
                "editedFiles": 0,
                "diffBytes": diff.len(),
                "diffLines": diff.lines().count(),
            })
        });
    json!({
        "schema": "xero.benchmark.terminal_bench_manifest.v1",
        "benchmark": {
            "name": config.benchmark_name,
            "datasetId": config.dataset_id,
            "datasetDigest": config.dataset_digest,
            "taskId": config.task_id,
            "attemptIndex": config.attempt_index,
            "runDate": now_timestamp(),
        },
        "harness": {
            "name": "xero",
            "harnessVersion": config.harness_version,
            "adapterVersion": config.adapter_version,
            "xeroCliVersion": env!("CARGO_PKG_VERSION"),
            "xeroSourceRevision": config.xero_source_revision.clone().or_else(|| repository_head_sha(Path::new("."))),
            "promptVersion": config.prompt_version,
            "toolPolicyVersion": config.tool_policy_version,
            "comparisonMode": config.comparison_mode,
            "fakeProviderFixture": config.allow_fake_provider_fixture,
        },
        "runtime": {
            "scoringPath": if config.allow_fake_provider_fixture {
                "fixture_fake_provider_harness"
            } else {
                "owned_agent_core_tool_registry_v2"
            },
            "productionRuntimeRequired": !config.allow_fake_provider_fixture,
            "legacyMiniHeadlessToolsAvailable": false,
            "requiredCoreCapabilities": [
                "provider_profile_model_resolution",
                "app_data_project_store_access",
                "context_package_assembly",
                "preflight_admission",
                "provider_loop_execution",
                "tool_registry_v2_dispatch",
                "command_execution",
                "patch_editing",
                "trace_support_bundle_export"
            ],
            "uiBoundDisabledCapabilities": [
                "desktop_window_events",
                "browser_control_requiring_ui_process",
                "emulator_control_requiring_ui_process"
            ],
        },
        "run": {
            "projectId": config.project_id,
            "sessionId": config.agent_session_id,
            "runId": config.run_id,
            "traceId": snapshot.map(|snapshot| snapshot.trace_id.as_str()),
            "status": status,
            "xeroRunStatus": snapshot.map(|snapshot| format!("{:?}", snapshot.status)),
            "failureCategory": run_error.map(|error| benchmark_failure_category(&error.code)),
            "failureCode": run_error.map(|error| error.code.as_str()),
            "failureMessage": run_error.map(|error| error.message.as_str()),
        },
        "model": {
            "provider": config.provider_id,
            "modelId": config.model_id,
            "endpointClass": config.endpoint_class,
            "temperature": config.temperature,
            "reasoningEffort": config.reasoning_effort,
            "maxOutputTokens": config.max_output_tokens,
            "contextBudget": config.context_budget,
            "providerAccountClass": config.provider_account_class,
            "credentialEnv": config.api_key_env,
            "baseUrlConfigured": config.base_url.is_some(),
        },
        "limits": {
            "wallTimeSeconds": config.wall_time_seconds,
            "maxTurns": config.max_turns,
            "maxToolCalls": config.max_tool_calls,
            "maxCommandCalls": config.max_command_calls,
            "maxCostUsd": config.max_cost_usd,
            "approvalMode": config.approval_mode,
            "sandboxPolicy": config.sandbox_policy,
            "networkPolicy": config.network_policy,
            "enforcement": {
                "maxTurns": "xero_runtime",
                "wallTime": "xero_runtime_and_harbor_timeout",
                "maxToolCalls": "xero_runtime",
                "maxCommandCalls": "xero_runtime",
                "maxCostUsd": "recorded_not_enforced_in_this_adapter",
            },
        },
        "environment": {
            "sandboxProvider": config.sandbox_provider,
            "environmentId": config.environment_id,
            "imageDigest": config.image_digest,
            "os": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "workspaceRoot": config.workspace_root,
            "trialAppDataRoot": config.trial_app_data_root,
            "installedCliVersions": installed_cli_versions_json(),
            "secretValuesRedacted": true,
        },
        "metrics": metrics,
        "artifacts": artifacts,
    })
}

fn benchmark_failure_category(code: &str) -> &'static str {
    let lower = code.to_ascii_lowercase();
    if lower.contains("provider") || lower.contains("credential") || lower.contains("api_key") {
        "provider_auth_failure"
    } else if lower.contains("timeout") || lower.contains("limit") {
        "timeout_or_limit"
    } else if lower.contains("sandbox") || lower.contains("policy") {
        "policy_blocked"
    } else {
        "agent_or_infrastructure_failure"
    }
}

fn benchmark_metrics_json(
    snapshot: &RunSnapshot,
    diff: &str,
    elapsed_ms: u64,
    repo_root: &Path,
) -> JsonValue {
    let tool_calls = snapshot
        .messages
        .iter()
        .filter_map(|message| message.provider_metadata.as_ref())
        .map(|metadata| metadata.assistant_tool_calls.len())
        .sum::<usize>();
    let command_calls = snapshot
        .events
        .iter()
        .filter(|event| {
            event.event_kind == RuntimeEventKind::ToolStarted
                && event
                    .payload
                    .get("toolName")
                    .and_then(JsonValue::as_str)
                    .is_some_and(|tool| tool.contains("command"))
        })
        .count();
    let mut edited_files = BTreeSet::<String>::new();
    for event in &snapshot.events {
        if event.event_kind == RuntimeEventKind::FileChanged {
            if let Some(path) = event.payload.get("path").and_then(JsonValue::as_str) {
                edited_files.insert(path.into());
            }
        }
    }
    for change in git_name_status(repo_root, false).unwrap_or_default() {
        edited_files.insert(change.path);
    }
    json!({
        "wallTimeMs": elapsed_ms,
        "costUsd": null,
        "inputTokens": null,
        "outputTokens": null,
        "toolCalls": tool_calls,
        "commandCalls": command_calls,
        "editedFiles": edited_files.len(),
        "editedFilePaths": edited_files,
        "diffBytes": diff.len(),
        "diffLines": diff.lines().count(),
    })
}

fn benchmark_trajectory_json(snapshot: &RunSnapshot, adapter_version: &str) -> JsonValue {
    let mut steps = Vec::new();
    for (index, message) in snapshot.messages.iter().enumerate() {
        let source = match message.role {
            MessageRole::Assistant => "agent",
            MessageRole::User => "user",
            MessageRole::System => "system",
            MessageRole::Tool => "environment",
            MessageRole::Developer => "system",
        };
        let mut step = json!({
            "step_id": index + 1,
            "timestamp": message.created_at,
            "source": source,
            "message": message.content,
        });
        if message.role == MessageRole::Assistant {
            step["model_name"] = json!(snapshot.model_id);
        }
        if let Some(metadata) = &message.provider_metadata {
            if !metadata.assistant_tool_calls.is_empty() {
                step["tool_calls"] = json!(metadata
                    .assistant_tool_calls
                    .iter()
                    .map(|call| {
                        json!({
                            "tool_call_id": call.tool_call_id,
                            "function_name": call.provider_tool_name,
                            "arguments": call.arguments,
                        })
                    })
                    .collect::<Vec<_>>());
            }
        }
        steps.push(step);
    }
    let step_count = steps.len();
    json!({
        "schema_version": "ATIF-v1.4",
        "session_id": snapshot.run_id,
        "agent": {
            "name": "xero",
            "version": env!("CARGO_PKG_VERSION"),
            "model_name": snapshot.model_id,
            "extra": {
                "adapter_version": adapter_version,
            },
        },
        "steps": steps,
        "final_metrics": {
            "total_steps": step_count,
        },
        "extra": {
            "conversion_status": "partial_xero_trace_is_lossless",
            "xero_trace_path": "xero-trace.json",
            "trace_id": snapshot.trace_id,
        },
    })
}

fn benchmark_stdout_text(
    config: &BenchmarkRunConfig,
    snapshot: Option<&RunSnapshot>,
    elapsed_ms: u64,
) -> String {
    match snapshot {
        Some(snapshot) => format!(
            "Xero benchmark run {} finished with status {:?} for task {} in {} ms.\n",
            snapshot.run_id, snapshot.status, config.task_id, elapsed_ms
        ),
        None => format!(
            "Xero benchmark run {} did not start for task {}.\n",
            config.run_id, config.task_id
        ),
    }
}

fn final_workspace_diff(repo_root: &Path) -> Result<String, CliError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("diff")
        .arg("--binary")
        .arg("--no-ext-diff")
        .output()
        .map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_git_diff_failed",
                format!(
                    "Could not run git diff in `{}`: {error}",
                    repo_root.display()
                ),
                1,
            )
        })?;
    if !output.status.success() {
        return Err(CliError::benchmark(
            "xero_benchmark_git_diff_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
            1,
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn write_support_bundle(path: &Path, files: &[(&str, &Path)]) -> Result<(), CliError> {
    let file = fs::File::create(path).map_err(|error| {
        CliError::benchmark(
            "xero_benchmark_support_bundle_write_failed",
            format!(
                "Could not create support bundle `{}`: {error}",
                path.display()
            ),
            1,
        )
    })?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, file_path) in files {
        let bytes = fs::read(file_path).map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_support_bundle_read_failed",
                format!(
                    "Could not read support bundle source `{}`: {error}",
                    file_path.display()
                ),
                1,
            )
        })?;
        zip.start_file(*name, options).map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_support_bundle_write_failed",
                format!("Could not add `{name}` to support bundle: {error}"),
                1,
            )
        })?;
        zip.write_all(&bytes).map_err(|error| {
            CliError::benchmark(
                "xero_benchmark_support_bundle_write_failed",
                format!("Could not write `{name}` to support bundle: {error}"),
                1,
            )
        })?;
    }
    zip.finish().map_err(|error| {
        CliError::benchmark(
            "xero_benchmark_support_bundle_write_failed",
            format!(
                "Could not finish support bundle `{}`: {error}",
                path.display()
            ),
            1,
        )
    })?;
    Ok(())
}

fn installed_cli_versions_json() -> JsonValue {
    json!({
        "xero": env!("CARGO_PKG_VERSION"),
        "git": command_version("git", &["--version"]),
        "python3": command_version("python3", &["--version"]),
        "docker": command_version("docker", &["--version"]),
        "uvx": command_version("uvx", &["--version"]),
    })
}

fn command_version(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                (!stdout.is_empty())
                    .then_some(stdout)
                    .or_else(|| (!stderr.is_empty()).then_some(stderr))
            } else {
                None
            }
        })
}

fn command_agent_host(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero agent host [PROMPT] --adapter codex|claude|gemini|custom [--command CMD] [--arg ARG]... [--timeout-ms MS] --allow-subprocess\nLaunches an explicitly-approved external CLI agent as an auditable Xero conversation.",
            json!({ "command": "agent host" }),
        ));
    }

    let prompt_flag = take_option(&mut args, "--prompt")?;
    let adapter_id = take_option(&mut args, "--adapter")?.unwrap_or_else(|| "custom".into());
    let command_override = take_option(&mut args, "--command")?;
    let mut command_args = Vec::new();
    while let Some(value) = take_option(&mut args, "--arg")? {
        command_args.push(value);
    }
    let project_id =
        take_option(&mut args, "--project-id")?.unwrap_or_else(|| DEFAULT_PROJECT_ID.into());
    let agent_session_id =
        take_option(&mut args, "--session-id")?.unwrap_or_else(|| generate_id("external-session"));
    let run_id = take_option(&mut args, "--run-id")?.unwrap_or_else(|| generate_id("external-run"));
    let model_override = take_option(&mut args, "--model")?;
    let timeout_ms = take_option(&mut args, "--timeout-ms")?
        .map(|value| parse_positive_u64(&value, "--timeout-ms"))
        .transpose()?
        .unwrap_or(EXTERNAL_AGENT_DEFAULT_TIMEOUT_MS);
    let allow_subprocess = take_bool_flag(&mut args, "--allow-subprocess");
    reject_unknown_options(&args)?;

    let prompt = prompt_flag
        .or_else(|| (!args.is_empty()).then(|| args.join(" ")))
        .ok_or_else(|| CliError::usage("Missing prompt. Use `xero agent host \"...\"`."))?;
    if prompt.trim().is_empty() {
        return Err(CliError::usage("Prompt cannot be empty."));
    }

    let adapter = external_agent_adapter(&adapter_id)?;
    let Some(approval_source) = external_agent_approval_source(allow_subprocess) else {
        return Err(CliError::user_fixable(
            "xero_cli_external_agent_approval_required",
            "External agent hosting launches a local subprocess. Re-run with `--allow-subprocess` or set XERO_EXTERNAL_AGENT_APPROVED=1 after reviewing the command.",
        ));
    };

    let command = command_override
        .or_else(|| adapter.default_command.map(str::to_owned))
        .ok_or_else(|| {
            CliError::usage(
                "Custom external agents require `--command`; built-in adapters provide a default command.",
            )
        })?;
    let argv = external_agent_argv(&adapter, command_args, &prompt);
    let model_id = model_override.unwrap_or_else(|| adapter.default_model_id.into());
    let store = open_harness_agent_store(&globals)?;
    let snapshot = host_external_agent_run(
        &globals,
        &store,
        ExternalAgentRunRequest {
            project_id,
            agent_session_id,
            run_id,
            prompt,
            adapter,
            command,
            argv,
            model_id,
            timeout_ms,
            approval_source,
        },
    )?;

    let text = format!(
        "External agent run {} finished with status {:?}.\nProvider: {}/{}\nTrace: {}\nState: {}",
        snapshot.run_id,
        snapshot.status,
        snapshot.provider_id,
        snapshot.model_id,
        snapshot.trace_id,
        store.path().display()
    );
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "externalAgentHost",
            "storePath": store.path(),
            "snapshot": snapshot,
        }),
    ))
}

fn command_conversation_list(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let project_filter = take_option(&mut args, "--project-id")?;
    reject_unknown_options(&args)?;
    let runs = list_conversation_runs(&globals, project_filter.as_deref())?;

    let text = if runs.is_empty() {
        "No headless conversations found.".into()
    } else {
        let mut lines = vec![
            "RUN ID                 PROJECT              STATUS      PROVIDER          EVENTS"
                .into(),
        ];
        for run in &runs {
            lines.push(format!(
                "{:<22} {:<20} {:<11} {:<17} {}",
                truncate(&run.run_id, 22),
                truncate(&run.project_id, 20),
                format!("{:?}", run.status),
                truncate(&run.provider_id, 17),
                run.event_count
            ));
        }
        lines.join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "conversationList", "appDataRoot": cli_app_data_root(&globals), "harnessStorePath": harness_store_path(&globals), "runs": runs }),
    ))
}

fn command_conversation_show(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let (store, snapshot) = load_conversation_from_args(&globals, args)?;
    let text = conversation_text(&snapshot, false);
    Ok(response(
        &globals,
        text,
        json!({ "kind": "conversationShow", "storePath": store.path(), "snapshot": snapshot }),
    ))
}

fn command_conversation_dump(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let (store, snapshot) = load_conversation_from_args(&globals, args)?;
    let trace = store
        .export_trace(&snapshot.project_id, &snapshot.run_id)
        .map_err(core_error)?;
    let canonical_trace = trace.canonical_snapshot().map_err(core_error)?;
    let text = trace.to_markdown_summary().map_err(core_error)?;
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "conversationDump",
            "storePath": store.path(),
            "trace": canonical_trace.trace,
            "timeline": canonical_trace.timeline,
            "diagnostics": canonical_trace.diagnostics,
            "qualityGates": canonical_trace.quality_gates,
            "productionReadiness": canonical_trace.production_readiness,
            "canonicalTrace": canonical_trace,
        }),
    ))
}

fn command_conversation_support_bundle(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let (store, snapshot) = load_conversation_from_args(&globals, args)?;
    let trace = store
        .export_trace(&snapshot.project_id, &snapshot.run_id)
        .map_err(core_error)?;
    let support_bundle = trace.redacted_support_bundle().map_err(core_error)?;
    let text = trace.to_markdown_summary().map_err(core_error)?;
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "conversationSupportBundle",
            "storePath": store.path(),
            "supportBundle": support_bundle,
        }),
    ))
}

fn command_conversation_compact(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let (store, snapshot) = load_conversation_from_args(&globals, args)?;
    let provider = resolve_runtime_for_existing_snapshot(&globals, &snapshot)?;
    let runtime = HeadlessProviderRuntime::new(
        store.clone(),
        provider.execution,
        HeadlessRuntimeOptions {
            ci_mode: globals.ci,
            ..HeadlessRuntimeOptions::default()
        },
    );
    let compacted = runtime
        .compact_session(xero_agent_core::CompactSessionRequest {
            project_id: snapshot.project_id.clone(),
            agent_session_id: snapshot.agent_session_id.clone(),
            reason: "cli_compact_requested".into(),
        })
        .map_err(core_error)?;
    let text = format!(
        "Compacted conversation {}. It now has {} context manifests.",
        compacted.run_id,
        compacted.context_manifests.len()
    );
    Ok(response(
        &globals,
        text,
        json!({ "kind": "conversationCompact", "snapshot": compacted }),
    ))
}

fn command_conversation_retry(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let (store, snapshot) = load_conversation_from_args(&globals, args)?;
    let provider = resolve_runtime_for_existing_snapshot(&globals, &snapshot)?;
    let provider_preflight = ensure_cli_provider_preflight_for_run(&globals, &provider)?;
    let runtime = HeadlessProviderRuntime::new(
        store.clone(),
        provider.execution,
        HeadlessRuntimeOptions {
            ci_mode: globals.ci,
            provider_preflight: Some(provider_preflight.clone()),
            ..HeadlessRuntimeOptions::default()
        },
    );
    let retried = runtime
        .start_run(StartRunRequest {
            project_id: snapshot.project_id.clone(),
            agent_session_id: snapshot.agent_session_id.clone(),
            run_id: generate_id("run-retry"),
            prompt: snapshot.prompt.clone(),
            provider: ProviderSelection {
                provider_id: provider.provider_id,
                model_id: provider.model_id,
            },
            controls: Some(RunControls {
                runtime_agent_id: "engineer".into(),
                approval_mode: if globals.ci { "strict" } else { "on_request" }.into(),
                plan_mode_required: globals.ci,
            }),
        })
        .map_err(core_error)?;
    let text = format!(
        "Retried conversation {} as {}.",
        snapshot.run_id, retried.run_id
    );
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "conversationRetry",
            "sourceRunId": snapshot.run_id,
            "providerPreflight": provider_preflight,
            "snapshot": retried
        }),
    ))
}

fn command_conversation_clone(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let (store, snapshot) = load_conversation_from_args(&globals, args)?;
    let provider = resolve_runtime_for_existing_snapshot(&globals, &snapshot)?;
    let runtime = HeadlessProviderRuntime::new(
        store.clone(),
        provider.execution,
        HeadlessRuntimeOptions {
            ci_mode: globals.ci,
            ..HeadlessRuntimeOptions::default()
        },
    );
    let cloned = runtime
        .fork_session(xero_agent_core::ForkSessionRequest {
            project_id: snapshot.project_id.clone(),
            source_agent_session_id: snapshot.agent_session_id.clone(),
            target_agent_session_id: generate_id("session-clone"),
        })
        .map_err(core_error)?;
    let text = format!(
        "Cloned conversation {} as session {} / run {}.",
        snapshot.run_id, cloned.agent_session_id, cloned.run_id
    );
    Ok(response(
        &globals,
        text,
        json!({ "kind": "conversationClone", "sourceRunId": snapshot.run_id, "snapshot": cloned }),
    ))
}

fn command_conversation_stats(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let (_store, snapshot) = load_conversation_from_args(&globals, args)?;
    let stats = conversation_stats_json(&snapshot);
    let text = format!(
        "Run: {}\nStatus: {:?}\nMessages: {}\nEvents: {}\nContext manifests: {}",
        snapshot.run_id,
        snapshot.status,
        snapshot.messages.len(),
        snapshot.events.len(),
        snapshot.context_manifests.len()
    );
    Ok(response(&globals, text, stats))
}

fn command_provider_list(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    reject_unknown_options(&args)?;
    let config = load_config(&globals)?;
    let providers = provider_catalog()
        .into_iter()
        .map(|entry| {
            let profiles = config
                .providers
                .values()
                .filter(|profile| profile.provider_id == entry.provider_id)
                .map(|profile| profile.profile_id.clone())
                .collect::<Vec<_>>();
            let profile = config
                .providers
                .values()
                .find(|profile| profile.provider_id == entry.provider_id);
            let capabilities = provider_capability_for_entry(entry, profile);
            json!({
                "providerId": entry.provider_id,
                "label": entry.label,
                "defaultModel": entry.default_model,
                "credentialKind": entry.credential_kind,
                "headlessStatus": entry.headless_status,
                "catalogKind": entry.catalog_kind,
                "adapterKind": entry.adapter_kind,
                "capabilities": capabilities,
                "profiles": profiles,
            })
        })
        .collect::<Vec<_>>();
    let text = providers
        .iter()
        .map(|provider| {
            format!(
                "{:<26} {:<22} {:<18} {}",
                provider["providerId"].as_str().unwrap_or_default(),
                provider["catalogKind"].as_str().unwrap_or_default(),
                provider["headlessStatus"].as_str().unwrap_or_default(),
                provider["defaultModel"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(response(
        &globals,
        text,
        json!({ "kind": "providerList", "providers": providers }),
    ))
}

fn command_provider_login(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let profile_id = take_option(&mut args, "--profile")?;
    let api_key_env = take_option(&mut args, "--api-key-env")?;
    let base_url = take_option(&mut args, "--base-url")?;
    reject_unknown_options(&args)?;
    let provider_id = args
        .first()
        .cloned()
        .ok_or_else(|| CliError::usage("Missing provider id. Use `xero provider login openrouter --api-key-env OPENROUTER_API_KEY`."))?;
    if provider_catalog_entry(&provider_id)
        .is_some_and(|entry| entry.catalog_kind == "external_agent_adapter")
    {
        return Err(CliError::usage(
            "External agent adapters do not use provider credentials. Launch them with `xero agent host --adapter ... --allow-subprocess`.",
        ));
    }
    let profile_id = profile_id.unwrap_or_else(|| format!("{provider_id}-headless"));
    let mut config = load_config(&globals)?;
    let profile = ProviderProfile {
        profile_id: profile_id.clone(),
        provider_id: provider_id.clone(),
        api_key_env,
        base_url,
        recorded_at: now_timestamp(),
    };
    config.providers.insert(profile_id.clone(), profile.clone());
    save_config(&globals, &config)?;
    let text = format!(
        "Recorded headless provider profile `{}` for `{}`. Secrets stay in environment variables.",
        profile_id, provider_id
    );
    Ok(response(
        &globals,
        text,
        json!({ "kind": "providerLogin", "profile": profile }),
    ))
}

fn command_provider_doctor(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let profile_id = take_option(&mut args, "--profile")?;
    reject_unknown_options(&args)?;
    let config = load_config(&globals)?;
    let provider_id = args
        .first()
        .cloned()
        .or_else(|| {
            profile_id.as_ref().and_then(|id| {
                config
                    .providers
                    .get(id)
                    .map(|profile| profile.provider_id.clone())
            })
        })
        .unwrap_or_else(|| FAKE_PROVIDER_ID.into());
    let profile = profile_id
        .as_ref()
        .and_then(|id| config.providers.get(id))
        .or_else(|| {
            config
                .providers
                .values()
                .find(|profile| profile.provider_id == provider_id)
        });
    let checks = provider_doctor_checks(&provider_id, profile);
    let capabilities = provider_catalog_entry(&provider_id)
        .map(|entry| provider_capability_for_entry(entry, profile));
    let failed = checks.iter().any(|check| check.status == "failed");
    let text = checks
        .iter()
        .map(|check| format!("{}: {}", check.status, check.message))
        .collect::<Vec<_>>()
        .join("\n");
    let mut output = response(
        &globals,
        text,
        json!({
            "kind": "providerDoctor",
            "checkedAt": now_timestamp(),
            "providerId": provider_id,
            "profileId": profile.map(|profile| profile.profile_id.clone()),
            "capabilities": capabilities,
            "checks": checks,
        }),
    );
    if failed && globals.output_mode == OutputMode::Text {
        output
            .text
            .push_str("\nProvider diagnostics found a headless configuration issue.");
    }
    Ok(output)
}

fn command_provider_preflight(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero provider preflight [PROVIDER_ID] [--profile PROFILE_ID] [--model MODEL_ID] [--force]\nChecks the selected headless provider/profile/model contract without starting an agent run.",
            json!({ "command": "provider preflight" }),
        ));
    }
    let profile_id = take_option(&mut args, "--profile")?;
    let model_id = take_option(&mut args, "--model")?;
    let force = take_bool_flag(&mut args, "--force");
    reject_unknown_options(&args)?;
    let snapshot = cli_provider_preflight_for_selection(
        &globals,
        args.first().map(String::as_str),
        profile_id.as_deref(),
        model_id.as_deref(),
        force,
    )?;
    let blockers = provider_preflight_blockers(&snapshot);
    let lines = snapshot
        .checks
        .iter()
        .map(|check| format!("{}: {}", check.status.as_str(), check.message))
        .collect::<Vec<_>>();
    let text = if lines.is_empty() {
        format!(
            "Provider preflight for {}/{} produced no checks.",
            snapshot.provider_id, snapshot.model_id
        )
    } else {
        format!(
            "Provider preflight for {}/{}: {}\n{}",
            snapshot.provider_id,
            snapshot.model_id,
            snapshot.status.as_str(),
            lines.join("\n")
        )
    };
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "providerPreflight",
            "snapshot": snapshot,
            "blockers": blockers,
        }),
    ))
}

fn command_tool_pack_list(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero tool-pack list\nLists the explicit domain tool packs and their policy profiles.",
            json!({ "command": "tool-pack list" }),
        ));
    }
    reject_unknown_options(&args)?;
    if !args.is_empty() {
        return Err(CliError::usage(
            "Unexpected arguments. Use `xero tool-pack list`.",
        ));
    }

    let packs = domain_tool_pack_manifests();
    let text = packs
        .iter()
        .map(|pack| {
            format!(
                "{:<18} {:<34} {:>2} tools  {:>2} scenarios",
                pack.pack_id,
                pack.policy_profile,
                pack.tools.len(),
                pack.scenario_checks.len()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "toolPackList",
            "contractVersion": DOMAIN_TOOL_PACK_CONTRACT_VERSION,
            "packs": packs,
        }),
    ))
}

fn command_tool_pack_doctor(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero tool-pack doctor [PACK_ID]\nChecks prerequisite health for explicit domain tool packs.",
            json!({ "command": "tool-pack doctor" }),
        ));
    }
    reject_unknown_options(&args)?;
    if args.len() > 1 {
        return Err(CliError::usage(
            "Too many arguments. Use `xero tool-pack doctor [PACK_ID]`.",
        ));
    }

    let manifests = if let Some(pack_id) = args.first() {
        vec![domain_tool_pack_manifest(pack_id).ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_tool_pack_unknown",
                format!("Tool pack `{pack_id}` is not in Xero's tool-pack catalog."),
            )
        })?]
    } else {
        domain_tool_pack_manifests()
    };
    let reports = manifests
        .iter()
        .map(|manifest| cli_tool_pack_health_report(&globals, manifest))
        .collect::<Vec<_>>();
    let text = reports
        .iter()
        .map(tool_pack_report_text)
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "toolPackDoctor",
            "contractVersion": DOMAIN_TOOL_PACK_CONTRACT_VERSION,
            "checkedAt": now_timestamp(),
            "reports": reports,
        }),
    ))
}

fn cli_tool_pack_health_report(
    globals: &GlobalOptions,
    manifest: &DomainToolPackManifest,
) -> DomainToolPackHealthReport {
    domain_tool_pack_health_report(
        manifest,
        &DomainToolPackHealthInput {
            pack_id: manifest.pack_id.clone(),
            enabled_by_policy: true,
            available_prerequisites: cli_tool_pack_available_prerequisites(globals, manifest),
            checked_at: now_timestamp(),
        },
    )
}

fn cli_tool_pack_available_prerequisites(
    globals: &GlobalOptions,
    manifest: &DomainToolPackManifest,
) -> Vec<String> {
    manifest
        .prerequisites
        .iter()
        .filter(|prerequisite| {
            cli_tool_pack_prerequisite_available(
                globals,
                &manifest.pack_id,
                &prerequisite.prerequisite_id,
            )
        })
        .map(|prerequisite| prerequisite.prerequisite_id.clone())
        .collect()
}

fn cli_tool_pack_prerequisite_available(
    globals: &GlobalOptions,
    _pack_id: &str,
    prerequisite_id: &str,
) -> bool {
    match prerequisite_id {
        "adb" | "anchor" | "solana" | "xcrun" => command_available(prerequisite_id),
        "app_data_store" => directory_available_or_creatable(&globals.state_dir),
        "repo_root" => env::current_dir()
            .map(|path| path.is_dir())
            .unwrap_or(false),
        "workspace_index_store" => cli_app_data_root(globals).join(PROJECTS_DIRECTORY).is_dir(),
        "macos_platform" => cfg!(target_os = "macos"),
        "webview_runtime" => cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )),
        "accessibility_permission"
        | "desktop_browser_executor"
        | "desktop_emulator_executor"
        | "desktop_runtime"
        | "screen_recording_permission"
        | "solana_state_executor" => false,
        _ => false,
    }
}

fn directory_available_or_creatable(path: &Path) -> bool {
    if path.is_dir() {
        return true;
    }
    path.ancestors()
        .skip(1)
        .find(|ancestor| ancestor.exists())
        .and_then(|ancestor| fs::metadata(ancestor).ok())
        .is_some_and(|metadata| metadata.is_dir() && !metadata.permissions().readonly())
}

fn tool_pack_report_text(report: &DomainToolPackHealthReport) -> String {
    let checks = report
        .checks
        .iter()
        .map(|check| {
            let remediation = check
                .diagnostic
                .as_ref()
                .map(|diagnostic| format!(" ({})", diagnostic.remediation))
                .unwrap_or_default();
            format!(
                "  {} {:<28} {}{}",
                tool_pack_status_label(check.status),
                check.check_id,
                check.summary,
                remediation
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let scenarios = report
        .scenario_checks
        .iter()
        .map(|scenario| {
            format!(
                "  {} {:<28} {}",
                tool_pack_status_label(scenario.status),
                scenario.scenario_id,
                scenario.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{} [{}]\nChecks:\n{}\nScenarios:\n{}",
        report.pack_id,
        tool_pack_status_label(report.status),
        checks,
        scenarios
    )
}

fn tool_pack_status_label(status: DomainToolPackHealthStatus) -> &'static str {
    match status {
        DomainToolPackHealthStatus::Passed => "passed",
        DomainToolPackHealthStatus::Warning => "warning",
        DomainToolPackHealthStatus::Failed => "failed",
        DomainToolPackHealthStatus::Skipped => "skipped",
    }
}

fn command_mcp_list(globals: GlobalOptions, args: Vec<String>) -> Result<CliResponse, CliError> {
    reject_unknown_options(&args)?;
    let config = load_config(&globals)?;
    let servers = config.mcp_servers.values().cloned().collect::<Vec<_>>();
    let text = if servers.is_empty() {
        "No headless MCP servers configured.".into()
    } else {
        servers
            .iter()
            .map(|server| format!("{}: {}", server.name, server.command))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "mcpList", "servers": servers }),
    ))
}

fn command_mcp_add(globals: GlobalOptions, mut args: Vec<String>) -> Result<CliResponse, CliError> {
    let command = take_option(&mut args, "--command")?;
    let mut command_args = Vec::new();
    while let Some(value) = take_option(&mut args, "--arg")? {
        command_args.push(value);
    }
    let mut env_vars = Vec::new();
    while let Some(value) = take_option(&mut args, "--env")? {
        env_vars.push(value);
    }
    reject_unknown_options(&args)?;
    let name = args.first().cloned().ok_or_else(|| {
        CliError::usage("Missing MCP server name. Use `xero mcp add name --command command`.")
    })?;
    let command = command.ok_or_else(|| CliError::usage("Missing `--command` for MCP server."))?;
    let mut config = load_config(&globals)?;
    let server = McpServerConfig {
        name: name.clone(),
        command,
        args: command_args,
        env: env_vars,
        token_env: None,
        added_at: now_timestamp(),
    };
    config.mcp_servers.insert(name.clone(), server.clone());
    save_config(&globals, &config)?;
    Ok(response(
        &globals,
        format!("Added headless MCP server `{name}`."),
        json!({ "kind": "mcpAdd", "server": server }),
    ))
}

fn command_mcp_login(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let token_env = take_option(&mut args, "--token-env")?;
    reject_unknown_options(&args)?;
    let name = args.first().cloned().ok_or_else(|| {
        CliError::usage("Missing MCP server name. Use `xero mcp login name --token-env TOKEN_ENV`.")
    })?;
    let token_env = token_env.ok_or_else(|| CliError::usage("Missing `--token-env`."))?;
    let mut config = load_config(&globals)?;
    let server = config.mcp_servers.get_mut(&name).ok_or_else(|| {
        CliError::user_fixable(
            "xero_cli_mcp_server_not_found",
            format!("MCP server `{name}` is not configured. Add it first with `xero mcp add`."),
        )
    })?;
    server.token_env = Some(token_env.clone());
    save_config(&globals, &config)?;
    Ok(response(
        &globals,
        format!("Recorded token environment variable `{token_env}` for MCP server `{name}`."),
        json!({ "kind": "mcpLogin", "serverName": name, "tokenEnv": token_env }),
    ))
}

fn command_mcp_serve(globals: GlobalOptions, args: Vec<String>) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero mcp serve\nRuns Xero's local harness MCP server over stdio. The server exposes only read-oriented and explicitly controlled tools by default.",
            json!({
                "command": "mcp serve",
                "transport": "stdio",
                "protocolVersion": MCP_PROTOCOL_VERSION,
            }),
        ));
    }
    reject_unknown_options(&args)?;
    run_mcp_stdio_server(globals.clone())?;
    Ok(silent_response(
        &globals,
        json!({ "kind": "mcpServe", "transport": "stdio" }),
    ))
}

fn command_workspace_index(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero workspace index [--repo PATH] [--project-id ID] [--max-files N] [--force]\nBuilds Xero's local semantic workspace index in app-data project state.",
            json!({ "command": "workspace index" }),
        ));
    }
    let repo = take_option(&mut args, "--repo")?.unwrap_or_else(|| ".".into());
    let project_id = take_option(&mut args, "--project-id")?;
    let max_files = take_option(&mut args, "--max-files")?;
    let legacy_limit = if max_files.is_none() {
        take_option(&mut args, "--limit")?
    } else {
        None
    };
    let limit = max_files
        .or(legacy_limit)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_INDEX_FILE_LIMIT)
        .clamp(1, HARD_INDEX_FILE_LIMIT);
    let force = take_bool_flag(&mut args, "--force");
    reject_unknown_options(&args)?;
    let repo_root = canonicalize_existing_path(&repo)?;
    let project_id = project_id.unwrap_or_else(|| stable_project_id_for_repo_root(&repo_root));
    let response_body = build_workspace_index(&globals, &repo_root, &project_id, limit, force)?;
    let text = format!(
        "Indexed {} changed file(s), reused {}, removed {} for `{}` at {}.",
        response_body.changed_files,
        response_body.unchanged_files,
        response_body.removed_files,
        project_id,
        response_body.status.storage_path
    );
    Ok(response(
        &globals,
        text,
        json!({ "kind": "workspaceIndex", "response": response_body }),
    ))
}

fn command_workspace_status(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero workspace status [--repo PATH] [--project-id ID]\nReports freshness and coverage for Xero's app-data semantic workspace index.",
            json!({ "command": "workspace status" }),
        ));
    }
    let repo = take_option(&mut args, "--repo")?.unwrap_or_else(|| ".".into());
    let project_id = take_option(&mut args, "--project-id")?;
    reject_unknown_options(&args)?;
    let repo_root = canonicalize_existing_path(&repo)?;
    let project_id = project_id.unwrap_or_else(|| stable_project_id_for_repo_root(&repo_root));
    let status = workspace_index_status(&globals, &repo_root, &project_id)?;
    Ok(response(
        &globals,
        format!(
            "Workspace index is {} with {} of {} files indexed.",
            status.state, status.indexed_files, status.total_files
        ),
        json!({ "kind": "workspaceStatus", "status": status }),
    ))
}

fn command_workspace_query(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero workspace query [--repo PATH] [--project-id ID] [--mode semantic|symbol|related-tests|impact|auto] [--limit N] [--path PATH] QUERY\nQueries Xero's local semantic workspace index.",
            json!({ "command": "workspace query" }),
        ));
    }
    let repo = take_option(&mut args, "--repo")?.unwrap_or_else(|| ".".into());
    let project_id = take_option(&mut args, "--project-id")?;
    let mode = take_option(&mut args, "--mode")?
        .map(|value| parse_workspace_query_mode(&value))
        .transpose()?
        .unwrap_or(WorkspaceQueryMode::Auto);
    let path_filter = take_option(&mut args, "--path")?;
    let limit = take_option(&mut args, "--limit")?
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_WORKSPACE_QUERY_LIMIT)
        .min(MAX_QUERY_RESULTS);
    reject_unknown_options(&args)?;
    let query = if args.is_empty() {
        return Err(CliError::usage(
            "Missing query. Use `xero workspace query \"...\"`.",
        ));
    } else {
        args.join(" ")
    };
    let repo_root = canonicalize_existing_path(&repo)?;
    let project_id = project_id.unwrap_or_else(|| stable_project_id_for_repo_root(&repo_root));
    let response_body = query_workspace_index_for_repo(
        &globals,
        &repo_root,
        &project_id,
        &query,
        mode,
        limit,
        path_filter.into_iter().collect(),
    )?;
    let text = if response_body.results.is_empty() {
        "No workspace index results matched.".into()
    } else {
        response_body
            .results
            .iter()
            .map(|result| format!("{:.3}  {}", result.score, result.path))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "workspaceQuery", "response": response_body }),
    ))
}

fn command_workspace_explain(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero workspace explain [--repo PATH] [--project-id ID] [--path PATH] [QUERY]\nExplains index freshness and the top retrieval signals.",
            json!({ "command": "workspace explain" }),
        ));
    }
    let repo = take_option(&mut args, "--repo")?.unwrap_or_else(|| ".".into());
    let project_id = take_option(&mut args, "--project-id")?;
    let path = take_option(&mut args, "--path")?;
    reject_unknown_options(&args)?;
    let query = (!args.is_empty()).then(|| args.join(" "));
    let repo_root = canonicalize_existing_path(&repo)?;
    let project_id = project_id.unwrap_or_else(|| stable_project_id_for_repo_root(&repo_root));
    let explanation = explain_workspace_index(&globals, &repo_root, &project_id, query, path)?;
    Ok(response(
        &globals,
        explanation.summary.clone(),
        json!({ "kind": "workspaceExplain", "explanation": explanation }),
    ))
}

fn command_workspace_reset(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero workspace reset [--repo PATH] [--project-id ID]\nDeletes Xero's local semantic workspace index rows for the selected project.",
            json!({ "command": "workspace reset" }),
        ));
    }
    let repo = take_option(&mut args, "--repo")?.unwrap_or_else(|| ".".into());
    let project_id = take_option(&mut args, "--project-id")?;
    reject_unknown_options(&args)?;
    let repo_root = canonicalize_existing_path(&repo)?;
    let project_id = project_id.unwrap_or_else(|| stable_project_id_for_repo_root(&repo_root));
    let status = reset_workspace_index(&globals, &repo_root, &project_id)?;
    Ok(response(
        &globals,
        format!("Reset workspace index for `{project_id}`."),
        json!({ "kind": "workspaceReset", "status": status }),
    ))
}

fn command_commit_message(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let repo = take_option(&mut args, "--repo")?.unwrap_or_else(|| ".".into());
    reject_unknown_options(&args)?;
    let repo_root = canonicalize_existing_path(&repo)?;
    let mut changes = git_name_status(&repo_root, true)?;
    if changes.is_empty() {
        changes = git_name_status(&repo_root, false)?;
    }
    let message = suggest_commit_message(&changes);
    Ok(response(
        &globals,
        message.clone(),
        json!({ "kind": "commitMessage", "message": message, "changes": changes }),
    ))
}

fn command_suggest_command(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    reject_unknown_options(&args)?;
    if args.is_empty() {
        return Err(CliError::usage(
            "Missing request. Use `xero suggest-command \"find TODO comments\"`.",
        ));
    }
    let request = args.join(" ");
    let suggestions = suggest_shell_commands(&request);
    let text = suggestions
        .iter()
        .map(|suggestion| format!("{}  # {}", suggestion.command, suggestion.reason))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(response(
        &globals,
        text,
        json!({ "kind": "suggestCommand", "request": request, "suggestions": suggestions }),
    ))
}

fn command_daemon(globals: GlobalOptions, args: Vec<String>) -> Result<CliResponse, CliError> {
    reject_unknown_options(&args)?;
    let text = "No local Xero daemon is required for the current headless CLI; commands use the durable app-data store directly.";
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "daemonStatus",
            "enabled": false,
            "reason": "headless_cli_uses_file_store_directly"
        }),
    ))
}

fn load_conversation_from_args(
    globals: &GlobalOptions,
    mut args: Vec<String>,
) -> Result<(CliAgentStore, RunSnapshot), CliError> {
    let project_id = take_option(&mut args, "--project-id")?;
    reject_unknown_options(&args)?;
    let run_id = args
        .first()
        .cloned()
        .ok_or_else(|| CliError::usage("Missing run id."))?;
    load_conversation_snapshot(globals, project_id.as_deref(), &run_id)
}

fn load_conversation_snapshot(
    globals: &GlobalOptions,
    project_id: Option<&str>,
    run_id: &str,
) -> Result<(CliAgentStore, RunSnapshot), CliError> {
    let mut app_data_error = None;
    if let Some(project_id) = project_id {
        match open_registered_project_store(globals, project_id) {
            Ok(store) => match store.load_run(project_id, run_id) {
                Ok(snapshot) => return Ok((CliAgentStore::AppData(store), snapshot)),
                Err(error) if error.code != "agent_core_run_not_found" => {
                    return Err(core_error(error));
                }
                Err(_) => {}
            },
            Err(error) => app_data_error = Some(error),
        }
    } else if let Some(found) = load_registered_run_by_id(globals, run_id)? {
        return Ok(found);
    }

    let harness = open_harness_agent_store(globals)?;
    let harness_result = match project_id {
        Some(project_id) => harness.load_run(project_id, run_id),
        None => harness.load_run_by_id(run_id),
    };
    let snapshot = match harness_result {
        Ok(snapshot) => snapshot,
        Err(error) => {
            if let Some(app_data_error) = app_data_error {
                if app_data_error.code != "xero_cli_project_registry_missing" {
                    return Err(app_data_error);
                }
            }
            return Err(core_error(error));
        }
    };
    if snapshot.provider_id != FAKE_PROVIDER_ID {
        return Err(CliError::user_fixable(
            "xero_cli_harness_store_real_run_rejected",
            format!(
                "Run `{}` was found only in `{}`, but real-provider runs must be inspected from app-data project state.",
                snapshot.run_id,
                harness.path().display()
            ),
        ));
    }
    Ok((CliAgentStore::Harness(harness), snapshot))
}

fn list_conversation_runs(
    globals: &GlobalOptions,
    project_id: Option<&str>,
) -> Result<Vec<RunSummary>, CliError> {
    let mut runs = Vec::new();
    if let Some(project_id) = project_id {
        if let Ok(store) = open_registered_project_store(globals, project_id) {
            runs.extend(store.list_project_runs(project_id).map_err(core_error)?);
        }
    } else {
        for project in read_registered_projects_optional(globals)? {
            if let Ok(store) = AppDataProjectAgentStore::open(project) {
                runs.extend(store.list_runs().map_err(core_error)?);
            }
        }
    }

    let harness = open_harness_agent_store(globals)?;
    let harness_runs = match project_id {
        Some(project_id) => harness.list_project_runs(project_id),
        None => harness.list_runs(),
    }
    .map_err(core_error)?
    .into_iter()
    .filter(|run| run.provider_id == FAKE_PROVIDER_ID);
    runs.extend(harness_runs);
    runs.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.started_at.cmp(&left.started_at))
            .then_with(|| left.project_id.cmp(&right.project_id))
            .then_with(|| left.run_id.cmp(&right.run_id))
    });
    Ok(runs)
}

fn load_registered_run_by_id(
    globals: &GlobalOptions,
    run_id: &str,
) -> Result<Option<(CliAgentStore, RunSnapshot)>, CliError> {
    let mut matches = Vec::new();
    for project in read_registered_projects_optional(globals)? {
        let store = match AppDataProjectAgentStore::open(project) {
            Ok(store) => store,
            Err(_) => continue,
        };
        match store.load_run_by_id(run_id) {
            Ok(snapshot) => matches.push((CliAgentStore::AppData(store), snapshot)),
            Err(error) if error.code == "agent_core_run_not_found" => {}
            Err(error) => return Err(core_error(error)),
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        _ => Err(CliError::user_fixable(
            "agent_core_run_ambiguous",
            format!("Run `{run_id}` exists in multiple projects. Pass `--project-id`."),
        )),
    }
}

fn open_run_store_for_provider(
    globals: &GlobalOptions,
    project_id: Option<String>,
    provider: &CliProviderExecution,
) -> Result<(String, CliAgentStore), CliError> {
    if provider.execution_mode == "real_provider" {
        let project = resolve_registered_project_for_run(globals, project_id.as_deref())?;
        let project_id = project.project_id.clone();
        let store = AppDataProjectAgentStore::open(project)?;
        return Ok((project_id, CliAgentStore::AppData(store)));
    }

    Ok((
        project_id.unwrap_or_else(default_harness_project_id),
        CliAgentStore::Harness(open_harness_agent_store(globals)?),
    ))
}

fn default_harness_project_id() -> String {
    env::current_dir()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PROJECT_ID.into())
}

fn open_harness_agent_store(globals: &GlobalOptions) -> Result<FileAgentCoreStore, CliError> {
    FileAgentCoreStore::open(harness_store_path(globals)).map_err(core_error)
}

fn harness_store_path(globals: &GlobalOptions) -> PathBuf {
    globals.state_dir.join(AGENT_CORE_STATE_FILE)
}

fn ensure_cli_real_provider_runtime_contract(
    project_id: &str,
    provider: &CliProviderExecution,
    store: &CliAgentStore,
) -> Result<(), CliError> {
    let contract = ProductionRuntimeContract::real_provider(
        "cli_agent_exec",
        project_id.to_owned(),
        provider.provider_id.clone(),
        provider.model_id.clone(),
        store.runtime_store_descriptor(project_id),
    );
    xero_agent_core::validate_production_runtime_contract(&contract).map_err(core_error)
}

#[derive(Debug, Clone)]
enum CliAgentStore {
    Harness(FileAgentCoreStore),
    AppData(AppDataProjectAgentStore),
}

impl CliAgentStore {
    fn path(&self) -> &Path {
        match self {
            Self::Harness(store) => store.path(),
            Self::AppData(store) => store.path(),
        }
    }

    fn repo_root(&self) -> Option<&Path> {
        match self {
            Self::Harness(_) => None,
            Self::AppData(store) => Some(store.repo_root.as_path()),
        }
    }
}

impl AgentCoreStore for CliAgentStore {
    fn runtime_store_descriptor(&self, project_id: &str) -> RuntimeStoreDescriptor {
        match self {
            Self::Harness(store) => store.runtime_store_descriptor(project_id),
            Self::AppData(store) => store.runtime_store_descriptor(project_id),
        }
    }

    fn semantic_workspace_index_state(&self, project_id: &str) -> EnvironmentSemanticIndexState {
        match self {
            Self::Harness(store) => store.semantic_workspace_index_state(project_id),
            Self::AppData(store) => store.semantic_workspace_index_state(project_id),
        }
    }

    fn insert_run(&self, run: NewRunRecord) -> xero_agent_core::CoreResult<RunSnapshot> {
        match self {
            Self::Harness(store) => store.insert_run(run),
            Self::AppData(store) => store.insert_run(run),
        }
    }

    fn load_run(&self, project_id: &str, run_id: &str) -> xero_agent_core::CoreResult<RunSnapshot> {
        match self {
            Self::Harness(store) => store.load_run(project_id, run_id),
            Self::AppData(store) => store.load_run(project_id, run_id),
        }
    }

    fn append_message(
        &self,
        message: NewMessageRecord,
    ) -> xero_agent_core::CoreResult<RunSnapshot> {
        match self {
            Self::Harness(store) => store.append_message(message),
            Self::AppData(store) => store.append_message(message),
        }
    }

    fn append_event(&self, event: NewRuntimeEvent) -> xero_agent_core::CoreResult<RuntimeEvent> {
        match self {
            Self::Harness(store) => store.append_event(event),
            Self::AppData(store) => store.append_event(event),
        }
    }

    fn record_context_manifest(
        &self,
        manifest: NewContextManifest,
    ) -> xero_agent_core::CoreResult<ContextManifest> {
        match self {
            Self::Harness(store) => store.record_context_manifest(manifest),
            Self::AppData(store) => store.record_context_manifest(manifest),
        }
    }

    fn update_run_status(
        &self,
        project_id: &str,
        run_id: &str,
        status: RunStatus,
    ) -> xero_agent_core::CoreResult<RunSnapshot> {
        match self {
            Self::Harness(store) => store.update_run_status(project_id, run_id, status),
            Self::AppData(store) => store.update_run_status(project_id, run_id, status),
        }
    }

    fn export_trace(
        &self,
        project_id: &str,
        run_id: &str,
    ) -> xero_agent_core::CoreResult<RuntimeTrace> {
        match self {
            Self::Harness(store) => store.export_trace(project_id, run_id),
            Self::AppData(store) => store.export_trace(project_id, run_id),
        }
    }

    fn latest_run_for_session(
        &self,
        project_id: &str,
        agent_session_id: &str,
    ) -> xero_agent_core::CoreResult<RunSnapshot> {
        match self {
            Self::Harness(store) => store.latest_run_for_session(project_id, agent_session_id),
            Self::AppData(store) => store.latest_run_for_session(project_id, agent_session_id),
        }
    }
}

#[derive(Debug, Clone)]
struct AppDataProjectAgentStore {
    project_id: String,
    repo_root: PathBuf,
    database_path: PathBuf,
}

impl AppDataProjectAgentStore {
    fn open(project: RegisteredProject) -> Result<Self, CliError> {
        if !project.repo_root.is_dir() {
            return Err(CliError::user_fixable(
                "xero_cli_project_root_missing",
                format!(
                    "Imported project `{}` points at `{}`, but that root is unavailable.",
                    project.project_id,
                    project.repo_root.display()
                ),
            ));
        }
        if !project.database_path.exists() {
            return Err(CliError::user_fixable(
                "xero_cli_project_state_missing",
                format!(
                    "Imported project `{}` is missing app-data project database `{}`.",
                    project.project_id,
                    project.database_path.display()
                ),
            ));
        }
        Ok(Self {
            project_id: project.project_id,
            repo_root: project.repo_root,
            database_path: project.database_path,
        })
    }

    fn path(&self) -> &Path {
        self.database_path.as_path()
    }

    fn connection(&self) -> xero_agent_core::CoreResult<Connection> {
        let connection = Connection::open(&self.database_path).map_err(|error| {
            xero_agent_core::CoreError::system_fault(
                "agent_core_app_data_store_open_failed",
                format!(
                    "Xero could not open app-data project state `{}`: {error}",
                    self.database_path.display()
                ),
            )
        })?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|error| {
                xero_agent_core::CoreError::system_fault(
                    "agent_core_app_data_store_config_failed",
                    format!("Xero could not configure app-data project state: {error}"),
                )
            })?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(|error| {
                xero_agent_core::CoreError::system_fault(
                    "agent_core_app_data_store_config_failed",
                    format!("Xero could not configure app-data project state pragmas: {error}"),
                )
            })?;
        Ok(connection)
    }

    fn workspace_index_status_for_lifecycle(
        &self,
        project_id: &str,
    ) -> Option<WorkspaceIndexStatus> {
        let connection = self.connection().ok()?;
        let mut status = read_workspace_status(
            &connection,
            &self.repo_root,
            project_id,
            &self.database_path,
        )
        .ok()?
        .unwrap_or_else(|| {
            empty_workspace_status(&self.repo_root, project_id, &self.database_path)
        });
        let scan = scan_workspace(&self.repo_root, HARD_INDEX_FILE_LIMIT).ok()?;
        let indexed = read_workspace_fingerprints(&connection, project_id).ok()?;
        let current_paths = scan
            .files
            .iter()
            .map(|candidate| candidate.virtual_path.clone())
            .collect::<BTreeSet<_>>();
        let stale_current = scan
            .files
            .iter()
            .filter(|candidate| {
                indexed
                    .get(&candidate.virtual_path)
                    .map(|fingerprint| {
                        fingerprint.modified_at != candidate.modified_at
                            || fingerprint.byte_length != candidate.byte_length
                    })
                    .unwrap_or(true)
            })
            .count();
        let removed = indexed
            .keys()
            .filter(|path| !current_paths.contains(*path))
            .count();
        status.total_files = scan.files.len() as u32;
        status.skipped_files = scan.skipped_files as u32;
        status.stale_files = stale_current.saturating_add(removed) as u32;
        status.coverage_percent = coverage_percent(
            status.indexed_files.saturating_sub(removed as u32),
            status.total_files,
        );
        if status.indexed_files == 0 {
            status.state = WorkspaceIndexState::Empty;
        } else if status.stale_files > 0 || status.head_sha != repository_head_sha(&self.repo_root)
        {
            status.state = WorkspaceIndexState::Stale;
        } else if status.state != WorkspaceIndexState::Failed {
            status.state = WorkspaceIndexState::Ready;
        }
        Some(status)
    }

    fn list_runs(&self) -> xero_agent_core::CoreResult<Vec<RunSummary>> {
        self.list_project_runs(&self.project_id)
    }

    fn list_project_runs(&self, project_id: &str) -> xero_agent_core::CoreResult<Vec<RunSummary>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT
                    trace_id,
                    project_id,
                    agent_session_id,
                    run_id,
                    provider_id,
                    model_id,
                    status,
                    prompt,
                    (SELECT COUNT(*) FROM agent_messages WHERE agent_messages.project_id = agent_runs.project_id AND agent_messages.run_id = agent_runs.run_id),
                    (SELECT COUNT(*) FROM agent_events WHERE agent_events.project_id = agent_runs.project_id AND agent_events.run_id = agent_runs.run_id),
                    (SELECT COUNT(*) FROM agent_context_manifests WHERE agent_context_manifests.project_id = agent_runs.project_id AND agent_context_manifests.run_id = agent_runs.run_id),
                    started_at,
                    updated_at
                FROM agent_runs
                WHERE project_id = ?1
                ORDER BY updated_at DESC, started_at DESC, run_id ASC
                "#,
            )
            .map_err(|error| self.query_error("agent_core_app_data_run_list_prepare_failed", error))?;
        let rows = statement
            .query_map(params![project_id], |row| {
                Ok(RunSummary {
                    trace_id: row.get(0)?,
                    project_id: row.get(1)?,
                    agent_session_id: row.get(2)?,
                    run_id: row.get(3)?,
                    provider_id: row.get(4)?,
                    model_id: row.get(5)?,
                    status: parse_core_run_status(row.get::<_, String>(6)?.as_str()),
                    prompt: row.get(7)?,
                    message_count: nonnegative_count(row.get(8)?),
                    event_count: nonnegative_count(row.get(9)?),
                    context_manifest_count: nonnegative_count(row.get(10)?),
                    started_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            })
            .map_err(|error| self.query_error("agent_core_app_data_run_list_failed", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| self.query_error("agent_core_app_data_run_list_decode_failed", error))
    }

    fn load_run_by_id(&self, run_id: &str) -> xero_agent_core::CoreResult<RunSnapshot> {
        let connection = self.connection()?;
        let project_id = connection
            .query_row(
                "SELECT project_id FROM agent_runs WHERE run_id = ?1",
                params![run_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| self.query_error("agent_core_app_data_run_lookup_failed", error))?
            .ok_or_else(|| {
                xero_agent_core::CoreError::invalid_request(
                    "agent_core_run_not_found",
                    format!("Run `{run_id}` was not found."),
                )
            })?;
        self.load_run(&project_id, run_id)
    }

    fn latest_run_for_session(
        &self,
        project_id: &str,
        agent_session_id: &str,
    ) -> xero_agent_core::CoreResult<RunSnapshot> {
        let connection = self.connection()?;
        let run_id = connection
            .query_row(
                r#"
                SELECT run_id
                FROM agent_runs
                WHERE project_id = ?1 AND agent_session_id = ?2
                ORDER BY updated_at DESC, started_at DESC, run_id DESC
                LIMIT 1
                "#,
                params![project_id, agent_session_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| {
                self.query_error("agent_core_app_data_session_latest_failed", error)
            })?
            .ok_or_else(|| {
                xero_agent_core::CoreError::invalid_request(
                    "agent_core_session_run_not_found",
                    format!(
                        "No run was found for session `{agent_session_id}` in project `{project_id}`."
                    ),
                )
            })?;
        self.load_run(project_id, &run_id)
    }

    fn query_error(
        &self,
        code: &'static str,
        error: rusqlite::Error,
    ) -> xero_agent_core::CoreError {
        xero_agent_core::CoreError::system_fault(
            code,
            format!(
                "Xero could not read app-data project state `{}` for `{}`: {error}",
                self.database_path.display(),
                self.repo_root.display()
            ),
        )
    }

    fn write_error(
        &self,
        code: &'static str,
        error: rusqlite::Error,
    ) -> xero_agent_core::CoreError {
        xero_agent_core::CoreError::system_fault(
            code,
            format!(
                "Xero could not write app-data project state `{}` for `{}`: {error}",
                self.database_path.display(),
                self.repo_root.display()
            ),
        )
    }
}

impl AgentCoreStore for AppDataProjectAgentStore {
    fn runtime_store_descriptor(&self, project_id: &str) -> RuntimeStoreDescriptor {
        RuntimeStoreDescriptor::app_data_project_state(project_id, self.database_path.clone())
    }

    fn semantic_workspace_index_state(&self, project_id: &str) -> EnvironmentSemanticIndexState {
        self.workspace_index_status_for_lifecycle(project_id)
            .map(|status| semantic_index_state_from_workspace_status(status.state))
            .unwrap_or(EnvironmentSemanticIndexState::Unavailable)
    }

    fn insert_run(&self, run: NewRunRecord) -> xero_agent_core::CoreResult<RunSnapshot> {
        validate_core_required(&run.project_id, "projectId")?;
        validate_core_required(&run.agent_session_id, "agentSessionId")?;
        validate_core_required(&run.run_id, "runId")?;
        validate_core_required(&run.prompt, "prompt")?;
        validate_core_required(&run.provider_id, "providerId")?;
        validate_core_required(&run.model_id, "modelId")?;
        if run.project_id != self.project_id {
            return Err(xero_agent_core::CoreError::invalid_request(
                "agent_core_app_data_project_mismatch",
                format!(
                    "Store for project `{}` cannot persist run for `{}`.",
                    self.project_id, run.project_id
                ),
            ));
        }

        let connection = self.connection()?;
        let project_exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
                params![run.project_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| self.query_error("agent_core_app_data_project_read_failed", error))?;
        if !project_exists {
            return Err(xero_agent_core::CoreError::invalid_request(
                "agent_core_app_data_project_missing",
                format!(
                    "App-data project database `{}` does not contain project `{}`.",
                    self.database_path.display(),
                    run.project_id
                ),
            ));
        }

        let now = now_timestamp();
        connection
            .execute(
                r#"
                INSERT INTO agent_sessions (
                    project_id,
                    agent_session_id,
                    title,
                    summary,
                    status,
                    selected,
                    updated_at,
                    last_run_id,
                    last_runtime_kind,
                    last_provider_id
                )
                VALUES (?1, ?2, ?3, '', 'active', 0, ?4, ?5, 'owned_agent', ?6)
                ON CONFLICT(project_id, agent_session_id) DO UPDATE SET
                    status = 'active',
                    archived_at = NULL,
                    updated_at = excluded.updated_at,
                    last_run_id = excluded.last_run_id,
                    last_runtime_kind = excluded.last_runtime_kind,
                    last_provider_id = excluded.last_provider_id
                "#,
                params![
                    run.project_id,
                    run.agent_session_id,
                    session_title(&run.agent_session_id),
                    now,
                    run.run_id,
                    run.provider_id,
                ],
            )
            .map_err(|error| {
                self.write_error("agent_core_app_data_session_upsert_failed", error)
            })?;

        let trace_id = run.trace_id.clone().unwrap_or_else(|| {
            xero_agent_core::runtime_trace_id_for_run(&run.project_id, &run.run_id)
        });
        connection
            .execute(
                r#"
                INSERT INTO agent_runs (
                    runtime_agent_id,
                    agent_definition_id,
                    agent_definition_version,
                    project_id,
                    agent_session_id,
                    run_id,
                    trace_id,
                    provider_id,
                    model_id,
                    status,
                    prompt,
                    system_prompt,
                    started_at,
                    last_heartbeat_at,
                    updated_at
                )
                VALUES ('engineer', 'engineer', 1, ?1, ?2, ?3, ?4, ?5, ?6, 'starting', ?7, ?8, ?9, ?9, ?9)
                "#,
                params![
                    run.project_id,
                    run.agent_session_id,
                    run.run_id,
                    trace_id,
                    run.provider_id,
                    run.model_id,
                    run.prompt,
                    "Xero CLI production runtime.",
                    now,
                ],
            )
            .map_err(|error| self.write_error("agent_core_app_data_run_insert_failed", error))?;
        self.load_run(&run.project_id, &run.run_id)
    }

    fn load_run(&self, project_id: &str, run_id: &str) -> xero_agent_core::CoreResult<RunSnapshot> {
        let connection = self.connection()?;
        let run = connection
            .query_row(
                r#"
                SELECT trace_id, project_id, agent_session_id, run_id, provider_id, model_id, status, prompt
                FROM agent_runs
                WHERE project_id = ?1 AND run_id = ?2
                "#,
                params![project_id, run_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| self.query_error("agent_core_app_data_run_read_failed", error))?
            .ok_or_else(|| {
                xero_agent_core::CoreError::invalid_request(
                    "agent_core_run_not_found",
                    format!("Run `{run_id}` was not found in project `{project_id}`."),
                )
            })?;
        let messages = read_app_data_messages(&connection, self, project_id, run_id)?;
        let events = read_app_data_events(&connection, self, &run.0, project_id, run_id)?;
        let context_manifests =
            read_app_data_context_manifests(&connection, self, &run.0, project_id, run_id)?;
        Ok(RunSnapshot {
            trace_id: run.0,
            project_id: run.1,
            agent_session_id: run.2,
            run_id: run.3,
            provider_id: run.4,
            model_id: run.5,
            status: parse_core_run_status(&run.6),
            prompt: run.7,
            messages,
            events,
            context_manifests,
        })
    }

    fn append_message(
        &self,
        message: NewMessageRecord,
    ) -> xero_agent_core::CoreResult<RunSnapshot> {
        validate_core_required(&message.project_id, "projectId")?;
        validate_core_required(&message.run_id, "runId")?;
        validate_core_message_content(&message)?;
        let provider_metadata_json = encode_message_provider_metadata(&message.provider_metadata)?;
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO agent_messages (project_id, run_id, role, content, provider_metadata_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    message.project_id,
                    message.run_id,
                    message_role_wire(&message.role),
                    message.content,
                    provider_metadata_json,
                    now_timestamp(),
                ],
            )
            .map_err(|error| self.write_error("agent_core_app_data_message_insert_failed", error))?;
        self.load_run(&message.project_id, &message.run_id)
    }

    fn append_event(&self, event: NewRuntimeEvent) -> xero_agent_core::CoreResult<RuntimeEvent> {
        validate_core_required(&event.project_id, "projectId")?;
        validate_core_required(&event.run_id, "runId")?;
        let connection = self.connection()?;
        let trace_id = run_trace_id(&connection, self, &event.project_id, &event.run_id)?;
        let payload_json = serde_json::to_string(&event.payload).map_err(|error| {
            xero_agent_core::CoreError::system_fault(
                "agent_core_app_data_event_encode_failed",
                format!("Xero could not encode app-data event payload: {error}"),
            )
        })?;
        let created_at = now_timestamp();
        connection
            .execute(
                "INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    event.project_id,
                    event.run_id,
                    runtime_event_kind_wire(&event.event_kind),
                    payload_json,
                    created_at,
                ],
            )
            .map_err(|error| self.write_error("agent_core_app_data_event_insert_failed", error))?;
        let id = connection.last_insert_rowid();
        let run_id_for_trace = event.run_id.clone();
        let event_kind_for_trace = event.event_kind.clone();
        Ok(RuntimeEvent {
            id,
            project_id: event.project_id,
            run_id: event.run_id,
            event_kind: event.event_kind,
            trace: event.trace.unwrap_or_else(|| {
                RuntimeTraceContext::for_event(
                    &trace_id,
                    &run_id_for_trace,
                    id,
                    &event_kind_for_trace,
                )
            }),
            payload: event.payload,
            created_at,
        })
    }

    fn record_context_manifest(
        &self,
        manifest: NewContextManifest,
    ) -> xero_agent_core::CoreResult<ContextManifest> {
        validate_core_required(&manifest.project_id, "projectId")?;
        validate_core_required(&manifest.agent_session_id, "agentSessionId")?;
        validate_core_required(&manifest.run_id, "runId")?;
        validate_core_required(&manifest.manifest_id, "manifestId")?;
        let connection = self.connection()?;
        let trace_id = run_trace_id(&connection, self, &manifest.project_id, &manifest.run_id)?;
        let recorded_after_event_id =
            latest_app_data_event_id(&connection, self, &manifest.project_id, &manifest.run_id)?;
        let manifest_json = serde_json::to_string(&manifest.manifest).map_err(|error| {
            xero_agent_core::CoreError::system_fault(
                "agent_core_app_data_context_manifest_encode_failed",
                format!("Xero could not encode app-data context manifest: {error}"),
            )
        })?;
        let created_at = now_timestamp();
        let context_hash = normalize_context_hash(&manifest.context_hash);
        connection
            .execute(
                r#"
                INSERT INTO agent_context_manifests (
                    manifest_id,
                    project_id,
                    agent_session_id,
                    run_id,
                    runtime_agent_id,
                    agent_definition_id,
                    agent_definition_version,
                    provider_id,
                    model_id,
                    request_kind,
                    policy_action,
                    policy_reason_code,
                    budget_tokens,
                    estimated_tokens,
                    pressure,
                    context_hash,
                    included_contributors_json,
                    excluded_contributors_json,
                    retrieval_query_ids_json,
                    retrieval_result_ids_json,
                    redaction_state,
                    manifest_json,
                    created_at
                )
                VALUES (?1, ?2, ?3, ?4, 'engineer', 'engineer', 1, ?5, ?6, 'provider_turn', 'continue_now', 'cli_app_data_runtime', NULL, 0, 'unknown', ?7, '[]', '[]', '[]', '[]', 'clean', ?8, ?9)
                "#,
                params![
                    manifest.manifest_id,
                    manifest.project_id,
                    manifest.agent_session_id,
                    manifest.run_id,
                    manifest.provider_id,
                    manifest.model_id,
                    context_hash,
                    manifest_json,
                    created_at,
                ],
            )
            .map_err(|error| {
                self.write_error("agent_core_app_data_context_manifest_insert_failed", error)
            })?;
        let run_id_for_trace = manifest.run_id.clone();
        let manifest_id_for_trace = manifest.manifest_id.clone();
        Ok(ContextManifest {
            manifest_id: manifest.manifest_id,
            project_id: manifest.project_id,
            agent_session_id: manifest.agent_session_id,
            run_id: manifest.run_id,
            provider_id: manifest.provider_id,
            model_id: manifest.model_id,
            turn_index: manifest.turn_index,
            context_hash,
            recorded_after_event_id,
            trace: manifest.trace.unwrap_or_else(|| {
                RuntimeTraceContext::for_context_manifest(
                    &trace_id,
                    &run_id_for_trace,
                    &manifest_id_for_trace,
                    manifest.turn_index,
                )
            }),
            manifest: manifest.manifest,
            created_at,
        })
    }

    fn update_run_status(
        &self,
        project_id: &str,
        run_id: &str,
        status: RunStatus,
    ) -> xero_agent_core::CoreResult<RunSnapshot> {
        let connection = self.connection()?;
        let now = now_timestamp();
        connection
            .execute(
                r#"
                UPDATE agent_runs
                SET status = ?3,
                    last_heartbeat_at = ?4,
                    completed_at = CASE WHEN ?3 IN ('completed', 'handed_off') THEN ?4 ELSE completed_at END,
                    cancelled_at = CASE WHEN ?3 = 'cancelled' THEN ?4 ELSE cancelled_at END,
                    updated_at = ?4
                WHERE project_id = ?1 AND run_id = ?2
                "#,
                params![project_id, run_id, run_status_wire(&status), now],
            )
            .map_err(|error| self.write_error("agent_core_app_data_status_update_failed", error))?;
        self.load_run(project_id, run_id)
    }

    fn export_trace(
        &self,
        project_id: &str,
        run_id: &str,
    ) -> xero_agent_core::CoreResult<RuntimeTrace> {
        RuntimeTrace::from_snapshot(self.load_run(project_id, run_id)?)
    }

    fn latest_run_for_session(
        &self,
        project_id: &str,
        agent_session_id: &str,
    ) -> xero_agent_core::CoreResult<RunSnapshot> {
        self.latest_run_for_session(project_id, agent_session_id)
    }
}

fn read_app_data_messages(
    connection: &Connection,
    store: &AppDataProjectAgentStore,
    project_id: &str,
    run_id: &str,
) -> xero_agent_core::CoreResult<Vec<RuntimeMessage>> {
    let mut statement = connection
        .prepare(
            "SELECT id, project_id, run_id, role, content, provider_metadata_json, created_at FROM agent_messages WHERE project_id = ?1 AND run_id = ?2 ORDER BY id ASC",
        )
        .map_err(|error| store.query_error("agent_core_app_data_messages_prepare_failed", error))?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            let provider_metadata_json: Option<String> = row.get(5)?;
            let provider_metadata = match provider_metadata_json {
                Some(value) => Some(
                    serde_json::from_str::<RuntimeMessageProviderMetadata>(&value).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                5,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                ),
                None => None,
            };
            Ok(RuntimeMessage {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                role: parse_message_role(row.get::<_, String>(3)?.as_str()),
                content: row.get(4)?,
                provider_metadata,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| store.query_error("agent_core_app_data_messages_query_failed", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| store.query_error("agent_core_app_data_messages_decode_failed", error))
}

fn read_app_data_events(
    connection: &Connection,
    store: &AppDataProjectAgentStore,
    trace_id: &str,
    project_id: &str,
    run_id: &str,
) -> xero_agent_core::CoreResult<Vec<RuntimeEvent>> {
    let mut statement = connection
        .prepare(
            "SELECT id, project_id, run_id, event_kind, payload_json, created_at FROM agent_events WHERE project_id = ?1 AND run_id = ?2 ORDER BY id ASC",
        )
        .map_err(|error| store.query_error("agent_core_app_data_events_prepare_failed", error))?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            let id = row.get::<_, i64>(0)?;
            let event_kind = parse_runtime_event_kind(row.get::<_, String>(3)?.as_str());
            let payload_text = row.get::<_, String>(4)?;
            let payload = serde_json::from_str::<JsonValue>(&payload_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(RuntimeEvent {
                id,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                trace: RuntimeTraceContext::for_event(trace_id, run_id, id, &event_kind),
                event_kind,
                payload,
                created_at: row.get(5)?,
            })
        })
        .map_err(|error| store.query_error("agent_core_app_data_events_query_failed", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| store.query_error("agent_core_app_data_events_decode_failed", error))
}

fn read_app_data_context_manifests(
    connection: &Connection,
    store: &AppDataProjectAgentStore,
    trace_id: &str,
    project_id: &str,
    run_id: &str,
) -> xero_agent_core::CoreResult<Vec<ContextManifest>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT manifest_id, project_id, agent_session_id, run_id, provider_id, model_id, context_hash, manifest_json, created_at
            FROM agent_context_manifests
            WHERE project_id = ?1 AND run_id = ?2
            ORDER BY id ASC
            "#,
        )
        .map_err(|error| {
            store.query_error("agent_core_app_data_context_manifests_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            let manifest_text = row.get::<_, String>(7)?;
            let manifest = serde_json::from_str::<JsonValue>(&manifest_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            let manifest_id = row.get::<_, String>(0)?;
            let turn_index = manifest
                .get("turnIndex")
                .and_then(JsonValue::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0);
            Ok(ContextManifest {
                trace: RuntimeTraceContext::for_context_manifest(
                    trace_id,
                    run_id,
                    &manifest_id,
                    turn_index,
                ),
                manifest_id,
                project_id: row.get(1)?,
                agent_session_id: row.get(2)?,
                run_id: row.get(3)?,
                provider_id: row.get(4)?,
                model_id: row.get(5)?,
                turn_index,
                context_hash: row.get(6)?,
                recorded_after_event_id: None,
                manifest,
                created_at: row.get(8)?,
            })
        })
        .map_err(|error| {
            store.query_error("agent_core_app_data_context_manifests_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        store.query_error("agent_core_app_data_context_manifests_decode_failed", error)
    })
}

fn run_trace_id(
    connection: &Connection,
    store: &AppDataProjectAgentStore,
    project_id: &str,
    run_id: &str,
) -> xero_agent_core::CoreResult<String> {
    connection
        .query_row(
            "SELECT trace_id FROM agent_runs WHERE project_id = ?1 AND run_id = ?2",
            params![project_id, run_id],
            |row| row.get(0),
        )
        .map_err(|error| store.query_error("agent_core_app_data_run_trace_read_failed", error))
}

fn latest_app_data_event_id(
    connection: &Connection,
    store: &AppDataProjectAgentStore,
    project_id: &str,
    run_id: &str,
) -> xero_agent_core::CoreResult<Option<i64>> {
    connection
        .query_row(
            "SELECT MAX(id) FROM agent_events WHERE project_id = ?1 AND run_id = ?2",
            params![project_id, run_id],
            |row| row.get(0),
        )
        .map_err(|error| store.query_error("agent_core_app_data_latest_event_failed", error))
}

fn validate_core_required(value: &str, field: &str) -> xero_agent_core::CoreResult<()> {
    if value.trim().is_empty() {
        Err(xero_agent_core::CoreError::invalid_request(
            "agent_core_app_data_required_field_missing",
            format!("App-data store field `{field}` is required."),
        ))
    } else {
        Ok(())
    }
}

fn validate_core_message_content(message: &NewMessageRecord) -> xero_agent_core::CoreResult<()> {
    if !message.content.trim().is_empty() {
        return Ok(());
    }
    let has_provider_tool_calls = message
        .provider_metadata
        .as_ref()
        .is_some_and(|metadata| !metadata.assistant_tool_calls.is_empty());
    if matches!(message.role, MessageRole::Assistant) && has_provider_tool_calls {
        return Ok(());
    }
    Err(xero_agent_core::CoreError::invalid_request(
        "agent_core_app_data_required_field_missing",
        "App-data store field `content` is required.",
    ))
}

fn encode_message_provider_metadata(
    metadata: &Option<RuntimeMessageProviderMetadata>,
) -> xero_agent_core::CoreResult<Option<String>> {
    metadata
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            xero_agent_core::CoreError::system_fault(
                "agent_core_app_data_message_provider_metadata_encode_failed",
                format!("Xero could not encode app-data message provider metadata: {error}"),
            )
        })
}

fn nonnegative_count(value: i64) -> usize {
    usize::try_from(value.max(0)).unwrap_or(0)
}

fn normalize_context_hash(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() == 64 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        trimmed.to_ascii_lowercase()
    } else {
        sha256_text(trimmed)
    }
}

fn session_title(agent_session_id: &str) -> String {
    let trimmed = agent_session_id.trim();
    if trimmed.is_empty() {
        "CLI Session".into()
    } else {
        format!("CLI {trimmed}")
    }
}

#[derive(Debug, Clone)]
struct RegisteredProject {
    project_id: String,
    repo_root: PathBuf,
    database_path: PathBuf,
}

fn resolve_registered_project_for_run(
    globals: &GlobalOptions,
    project_id: Option<&str>,
) -> Result<RegisteredProject, CliError> {
    if let Some(project_id) = project_id {
        return registered_project_by_id(globals, project_id);
    }

    let current_dir = env::current_dir().map_err(|error| {
        CliError::system_fault(
            "xero_cli_current_dir_failed",
            format!("Could not inspect the current workspace directory: {error}"),
        )
    })?;
    let current_dir = fs::canonicalize(&current_dir).unwrap_or(current_dir);
    let mut matches = read_registered_projects_strict(globals)?
        .into_iter()
        .filter(|project| current_dir.starts_with(&project.repo_root))
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .repo_root
            .components()
            .count()
            .cmp(&left.repo_root.components().count())
    });
    matches.into_iter().next().ok_or_else(|| {
        CliError::user_fixable(
            "xero_cli_project_unimported",
            format!(
                "Current directory `{}` is not registered in Xero app-data. Import the project in desktop first or pass a registered `--project-id`.",
                current_dir.display()
            ),
        )
    })
}

fn open_registered_project_store(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<AppDataProjectAgentStore, CliError> {
    AppDataProjectAgentStore::open(registered_project_by_id(globals, project_id)?)
}

fn registered_project_by_id(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<RegisteredProject, CliError> {
    validate_required_cli(project_id, "projectId")?;
    read_registered_projects_strict(globals)?
        .into_iter()
        .find(|project| project.project_id == project_id)
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_project_unknown",
                format!(
                    "Project `{project_id}` is not registered in Xero app-data at `{}`.",
                    cli_app_data_root(globals).display()
                ),
            )
        })
}

fn read_registered_projects_optional(
    globals: &GlobalOptions,
) -> Result<Vec<RegisteredProject>, CliError> {
    let database_path = global_database_path(globals);
    if !database_path.exists() {
        return Ok(Vec::new());
    }
    read_registered_projects_from_database(globals, &database_path).or_else(|_| Ok(Vec::new()))
}

fn read_registered_projects_strict(
    globals: &GlobalOptions,
) -> Result<Vec<RegisteredProject>, CliError> {
    let database_path = global_database_path(globals);
    if !database_path.exists() {
        return Err(CliError::user_fixable(
            "xero_cli_project_registry_missing",
            format!(
                "Xero app-data registry `{}` does not exist. Import the project in desktop before starting real-provider CLI or MCP runs.",
                database_path.display()
            ),
        ));
    }
    read_registered_projects_from_database(globals, &database_path)
}

fn read_registered_projects_from_database(
    globals: &GlobalOptions,
    database_path: &Path,
) -> Result<Vec<RegisteredProject>, CliError> {
    let connection = Connection::open(database_path).map_err(|error| {
        CliError::system_fault(
            "xero_cli_project_registry_open_failed",
            format!(
                "Xero could not open app-data project registry `{}`: {error}",
                database_path.display()
            ),
        )
    })?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT projects.id, repositories.root_path
            FROM projects
            JOIN repositories ON repositories.project_id = projects.id
            ORDER BY projects.updated_at DESC, repositories.updated_at DESC, repositories.root_path ASC
            "#,
        )
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_project_registry_read_failed",
                format!("Xero could not prepare app-data project registry read: {error}"),
            )
        })?;
    let app_data_root = cli_app_data_root(globals);
    let rows = statement
        .query_map([], |row| {
            let project_id = row.get::<_, String>(0)?;
            let repo_root = PathBuf::from(row.get::<_, String>(1)?);
            Ok(RegisteredProject {
                database_path: workspace_project_database_path_for_app_root(
                    &app_data_root,
                    &project_id,
                ),
                project_id,
                repo_root,
            })
        })
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_project_registry_read_failed",
                format!("Xero could not read app-data project registry: {error}"),
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        CliError::system_fault(
            "xero_cli_project_registry_decode_failed",
            format!("Xero could not decode app-data project registry: {error}"),
        )
    })
}

fn global_database_path(globals: &GlobalOptions) -> PathBuf {
    cli_app_data_root(globals).join(GLOBAL_DATABASE_FILE)
}

fn run_status_wire(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Starting => "starting",
        RunStatus::Running => "running",
        RunStatus::Paused => "paused",
        RunStatus::Cancelling => "cancelling",
        RunStatus::Cancelled => "cancelled",
        RunStatus::HandedOff => "handed_off",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
    }
}

fn parse_core_run_status(value: &str) -> RunStatus {
    match value {
        "starting" => RunStatus::Starting,
        "running" => RunStatus::Running,
        "paused" => RunStatus::Paused,
        "cancelling" => RunStatus::Cancelling,
        "cancelled" => RunStatus::Cancelled,
        "handed_off" => RunStatus::HandedOff,
        "completed" => RunStatus::Completed,
        "failed" => RunStatus::Failed,
        _ => RunStatus::Failed,
    }
}

fn message_role_wire(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::Developer => "developer",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn parse_message_role(value: &str) -> MessageRole {
    match value {
        "system" => MessageRole::System,
        "developer" => MessageRole::Developer,
        "user" => MessageRole::User,
        "tool" => MessageRole::Tool,
        _ => MessageRole::Assistant,
    }
}

fn runtime_event_kind_wire(kind: &RuntimeEventKind) -> &'static str {
    match kind {
        RuntimeEventKind::RunStarted => "run_started",
        RuntimeEventKind::MessageDelta => "message_delta",
        RuntimeEventKind::ReasoningSummary => "reasoning_summary",
        RuntimeEventKind::ToolStarted => "tool_started",
        RuntimeEventKind::ToolDelta => "tool_delta",
        RuntimeEventKind::ToolCompleted => "tool_completed",
        RuntimeEventKind::FileChanged => "file_changed",
        RuntimeEventKind::CommandOutput => "command_output",
        RuntimeEventKind::ValidationStarted => "validation_started",
        RuntimeEventKind::ValidationCompleted => "validation_completed",
        RuntimeEventKind::ToolRegistrySnapshot => "tool_registry_snapshot",
        RuntimeEventKind::PolicyDecision => "policy_decision",
        RuntimeEventKind::StateTransition => "state_transition",
        RuntimeEventKind::PlanUpdated => "plan_updated",
        RuntimeEventKind::VerificationGate => "verification_gate",
        RuntimeEventKind::ContextManifestRecorded => "context_manifest_recorded",
        RuntimeEventKind::RetrievalPerformed => "retrieval_performed",
        RuntimeEventKind::MemoryCandidateCaptured => "memory_candidate_captured",
        RuntimeEventKind::EnvironmentLifecycleUpdate => "environment_lifecycle_update",
        RuntimeEventKind::SandboxLifecycleUpdate => "sandbox_lifecycle_update",
        RuntimeEventKind::ActionRequired => "action_required",
        RuntimeEventKind::ApprovalRequired => "approval_required",
        RuntimeEventKind::ToolPermissionGrant => "tool_permission_grant",
        RuntimeEventKind::ProviderModelChanged => "provider_model_changed",
        RuntimeEventKind::RuntimeSettingsChanged => "runtime_settings_changed",
        RuntimeEventKind::RunPaused => "run_paused",
        RuntimeEventKind::RunCompleted => "run_completed",
        RuntimeEventKind::RunFailed => "run_failed",
        RuntimeEventKind::SubagentLifecycle => "subagent_lifecycle",
    }
}

fn parse_runtime_event_kind(value: &str) -> RuntimeEventKind {
    match value {
        "run_started" => RuntimeEventKind::RunStarted,
        "message_delta" => RuntimeEventKind::MessageDelta,
        "reasoning_summary" => RuntimeEventKind::ReasoningSummary,
        "tool_started" => RuntimeEventKind::ToolStarted,
        "tool_delta" => RuntimeEventKind::ToolDelta,
        "tool_completed" => RuntimeEventKind::ToolCompleted,
        "file_changed" => RuntimeEventKind::FileChanged,
        "command_output" => RuntimeEventKind::CommandOutput,
        "validation_started" => RuntimeEventKind::ValidationStarted,
        "validation_completed" => RuntimeEventKind::ValidationCompleted,
        "tool_registry_snapshot" => RuntimeEventKind::ToolRegistrySnapshot,
        "policy_decision" => RuntimeEventKind::PolicyDecision,
        "state_transition" => RuntimeEventKind::StateTransition,
        "plan_updated" => RuntimeEventKind::PlanUpdated,
        "verification_gate" => RuntimeEventKind::VerificationGate,
        "context_manifest_recorded" => RuntimeEventKind::ContextManifestRecorded,
        "retrieval_performed" => RuntimeEventKind::RetrievalPerformed,
        "memory_candidate_captured" => RuntimeEventKind::MemoryCandidateCaptured,
        "environment_lifecycle_update" => RuntimeEventKind::EnvironmentLifecycleUpdate,
        "sandbox_lifecycle_update" => RuntimeEventKind::SandboxLifecycleUpdate,
        "action_required" => RuntimeEventKind::ActionRequired,
        "approval_required" => RuntimeEventKind::ApprovalRequired,
        "tool_permission_grant" => RuntimeEventKind::ToolPermissionGrant,
        "provider_model_changed" => RuntimeEventKind::ProviderModelChanged,
        "runtime_settings_changed" => RuntimeEventKind::RuntimeSettingsChanged,
        "run_paused" => RuntimeEventKind::RunPaused,
        "run_completed" => RuntimeEventKind::RunCompleted,
        "run_failed" => RuntimeEventKind::RunFailed,
        "subagent_lifecycle" => RuntimeEventKind::SubagentLifecycle,
        _ => RuntimeEventKind::RunFailed,
    }
}

fn parse_global_options(raw_args: Vec<String>) -> Result<(GlobalOptions, Vec<String>), CliError> {
    let mut output_mode = OutputMode::Text;
    let mut ci = false;
    let mut state_dir = None;
    let mut command_args = Vec::new();
    let mut iter = raw_args.into_iter();
    let _program = iter.next();

    while let Some(arg) = iter.next() {
        if arg == "--json" {
            output_mode = OutputMode::Json;
        } else if arg == "--ci" {
            ci = true;
        } else if arg == "--state-dir" {
            let value = iter
                .next()
                .ok_or_else(|| CliError::usage("Missing value for `--state-dir`."))?;
            state_dir = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--state-dir=") {
            state_dir = Some(PathBuf::from(value));
        } else {
            command_args.push(arg);
        }
    }

    let state_dir = state_dir.unwrap_or(default_headless_state_dir()?);
    Ok((
        GlobalOptions {
            output_mode,
            ci,
            state_dir,
        },
        command_args,
    ))
}

fn requested_output_mode(args: &[String]) -> OutputMode {
    if args.iter().any(|arg| arg == "--json") {
        OutputMode::Json
    } else {
        OutputMode::Text
    }
}

fn response(
    globals: &GlobalOptions,
    text: impl Into<String>,
    json_value: JsonValue,
) -> CliResponse {
    CliResponse {
        output_mode: globals.output_mode,
        text: text.into(),
        json: json_value,
        emit: true,
    }
}

fn silent_response(globals: &GlobalOptions, json_value: JsonValue) -> CliResponse {
    CliResponse {
        output_mode: globals.output_mode,
        text: String::new(),
        json: json_value,
        emit: false,
    }
}

fn emit_response(response: &CliResponse) {
    match response.output_mode {
        OutputMode::Text => println!("{}", response.text),
        OutputMode::Json => match serde_json::to_string_pretty(&response.json) {
            Ok(payload) => println!("{payload}"),
            Err(error) => {
                eprintln!("xero_cli_json_encode_failed: {error}");
            }
        },
    }
}

fn emit_error(error: &CliError, output_mode: OutputMode) {
    match output_mode {
        OutputMode::Text => eprintln!("{}: {}", error.code, error.message),
        OutputMode::Json => {
            let payload = json!({
                "error": {
                    "code": error.code,
                    "message": error.message,
                }
            });
            match serde_json::to_string_pretty(&payload) {
                Ok(payload) => eprintln!("{payload}"),
                Err(_) => eprintln!("{}: {}", error.code, error.message),
            }
        }
    }
}

fn root_help() -> String {
    [
        "Usage: xero [--json] [--ci] [--state-dir PATH] <command>",
        "",
        "Commands:",
        "  agent exec",
        "  agent host",
        "  benchmark terminal-bench",
        "  conversation list|show|dump|support-bundle|compact|retry|clone|stats",
        "  provider list|login|doctor|preflight",
        "  mcp list|add|login|serve",
        "  workspace index|status|query|explain|reset",
        "  tool-pack list|doctor",
        "  commit-message",
        "  suggest-command",
        "  daemon",
    ]
    .join("\n")
}

fn root_help_json() -> JsonValue {
    json!({
        "kind": "help",
        "commands": [
            "agent exec",
            "agent host",
            "benchmark terminal-bench",
            "conversation list",
            "conversation show",
            "conversation dump",
            "conversation support-bundle",
            "conversation compact",
            "conversation retry",
            "conversation clone",
            "conversation stats",
            "provider list",
            "provider login",
            "provider doctor",
            "provider preflight",
            "mcp list",
            "mcp add",
            "mcp login",
            "mcp serve",
            "workspace index",
            "workspace status",
            "workspace query",
            "workspace explain",
            "workspace reset",
            "tool-pack list",
            "tool-pack doctor",
            "commit-message",
            "suggest-command"
        ]
    })
}

fn take_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

fn take_option(args: &mut Vec<String>, name: &str) -> Result<Option<String>, CliError> {
    if let Some(index) = args.iter().position(|arg| arg == name) {
        args.remove(index);
        if index >= args.len() {
            return Err(CliError::usage(format!("Missing value for `{name}`.")));
        }
        return Ok(Some(args.remove(index)));
    }

    let prefix = format!("{name}=");
    if let Some(index) = args.iter().position(|arg| arg.starts_with(&prefix)) {
        let value = args.remove(index);
        return Ok(value.strip_prefix(&prefix).map(ToOwned::to_owned));
    }

    Ok(None)
}

fn take_bool_flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == name) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn reject_unknown_options(args: &[String]) -> Result<(), CliError> {
    if let Some(option) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(CliError::usage(format!("Unknown option `{option}`.")));
    }
    Ok(())
}

fn parse_positive_u64(value: &str, name: &str) -> Result<u64, CliError> {
    let parsed = value
        .trim()
        .parse::<u64>()
        .map_err(|_| CliError::usage(format!("`{name}` must be a positive integer.")))?;
    if parsed == 0 {
        return Err(CliError::usage(format!(
            "`{name}` must be greater than zero."
        )));
    }
    Ok(parsed)
}

fn core_error(error: xero_agent_core::CoreError) -> CliError {
    CliError::user_fixable(error.code, error.message)
}

fn default_headless_state_dir() -> Result<PathBuf, CliError> {
    if let Some(path) = env::var_os("XERO_APP_DATA_DIR") {
        if !path.is_empty() {
            return Ok(PathBuf::from(path).join(HEADLESS_DIRECTORY_NAME));
        }
    }

    #[cfg(target_os = "macos")]
    {
        home_dir()
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join(APP_DATA_DIRECTORY_NAME)
                    .join(HEADLESS_DIRECTORY_NAME)
            })
            .ok_or_else(home_dir_error)
    }

    #[cfg(target_os = "windows")]
    {
        return env::var_os("APPDATA")
            .or_else(|| env::var_os("LOCALAPPDATA"))
            .map(|root| {
                PathBuf::from(root)
                    .join(APP_DATA_DIRECTORY_NAME)
                    .join(HEADLESS_DIRECTORY_NAME)
            })
            .ok_or_else(|| {
                CliError::system_fault(
                    "xero_cli_app_data_unavailable",
                    "APPDATA or LOCALAPPDATA is required to locate Xero headless state.",
                )
            });
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(path) = env::var_os("XDG_DATA_HOME") {
            if !path.is_empty() {
                return Ok(PathBuf::from(path)
                    .join(APP_DATA_DIRECTORY_NAME)
                    .join(HEADLESS_DIRECTORY_NAME));
            }
        }
        home_dir()
            .map(|home| {
                home.join(".local")
                    .join("share")
                    .join(APP_DATA_DIRECTORY_NAME)
                    .join(HEADLESS_DIRECTORY_NAME)
            })
            .ok_or_else(home_dir_error)
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

fn home_dir_error() -> CliError {
    CliError::system_fault(
        "xero_cli_home_unavailable",
        "HOME is required to locate Xero headless app-data state.",
    )
}

fn generate_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{prefix}-{millis}-{}", std::process::id())
}

fn now_timestamp() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let seconds = millis / 1_000;
    let remainder = millis % 1_000;
    format!("{seconds}.{remainder:03}Z")
}

fn last_assistant_message(snapshot: &RunSnapshot) -> Option<String> {
    snapshot
        .messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, xero_agent_core::MessageRole::Assistant))
        .map(|message| message.content.clone())
}

fn sandbox_defaults_json(ci: bool) -> JsonValue {
    if ci {
        json!({
            "profile": "ci_strict",
            "network": "disabled_by_default",
            "approvalMode": "strict",
            "planModeRequired": true,
            "interactivePrompts": false
        })
    } else {
        json!({
            "profile": "local_headless",
            "network": "approval_gated",
            "approvalMode": "on_request",
            "planModeRequired": false,
            "interactivePrompts": false
        })
    }
}

fn conversation_text(snapshot: &RunSnapshot, include_events: bool) -> String {
    let mut lines = vec![
        format!("Run: {}", snapshot.run_id),
        format!("Project: {}", snapshot.project_id),
        format!("Session: {}", snapshot.agent_session_id),
        format!("Status: {:?}", snapshot.status),
        format!("Provider: {}/{}", snapshot.provider_id, snapshot.model_id),
        format!("Trace: {}", snapshot.trace_id),
        String::new(),
        "Messages:".into(),
    ];
    for message in &snapshot.messages {
        lines.push(format!("  {:?}: {}", message.role, message.content));
    }
    if include_events {
        lines.push(String::new());
        lines.push("Events:".into());
        for event in &snapshot.events {
            lines.push(format!(
                "  #{} {:?}: {}",
                event.id, event.event_kind, event.payload
            ));
        }
    }
    lines.join("\n")
}

fn conversation_stats_json(snapshot: &RunSnapshot) -> JsonValue {
    let mut event_counts = BTreeMap::<String, usize>::new();
    for event in &snapshot.events {
        *event_counts
            .entry(format!("{:?}", event.event_kind))
            .or_default() += 1;
    }
    json!({
        "kind": "conversationStats",
        "projectId": snapshot.project_id,
        "runId": snapshot.run_id,
        "status": snapshot.status,
        "messageCount": snapshot.messages.len(),
        "eventCount": snapshot.events.len(),
        "contextManifestCount": snapshot.context_manifests.len(),
        "eventCounts": event_counts,
    })
}

fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.into()
    } else {
        let mut out = value
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>();
        out.push('~');
        out
    }
}

fn truncate_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.into();
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &value[..end])
}

#[derive(Debug, Clone, Copy)]
struct ProviderCatalogEntry {
    provider_id: &'static str,
    label: &'static str,
    default_model: &'static str,
    credential_kind: &'static str,
    headless_status: &'static str,
    catalog_kind: &'static str,
    adapter_kind: Option<&'static str>,
}

fn provider_catalog() -> Vec<ProviderCatalogEntry> {
    vec![
        ProviderCatalogEntry {
            provider_id: FAKE_PROVIDER_ID,
            label: "Fake Provider",
            default_model: DEFAULT_MODEL_ID,
            credential_kind: "none",
            headless_status: "harness_only_explicit",
            catalog_kind: "owned_agent_provider",
            adapter_kind: None,
        },
        ProviderCatalogEntry {
            provider_id: "openai_codex",
            label: "OpenAI Codex",
            default_model: "gpt-5.4",
            credential_kind: "app_session",
            headless_status: "diagnostics_only",
            catalog_kind: "model_provider",
            adapter_kind: None,
        },
        ProviderCatalogEntry {
            provider_id: "openai_api",
            label: "OpenAI API",
            default_model: "gpt-5.4",
            credential_kind: "api_key_env",
            headless_status: "configured_profile_required",
            catalog_kind: "model_provider",
            adapter_kind: None,
        },
        ProviderCatalogEntry {
            provider_id: "openrouter",
            label: "OpenRouter",
            default_model: "openai/gpt-5.4",
            credential_kind: "api_key_env",
            headless_status: "configured_profile_required",
            catalog_kind: "model_provider",
            adapter_kind: None,
        },
        ProviderCatalogEntry {
            provider_id: "anthropic",
            label: "Anthropic",
            default_model: "claude-sonnet-4-5",
            credential_kind: "api_key_env",
            headless_status: "configured_profile_required",
            catalog_kind: "model_provider",
            adapter_kind: None,
        },
        ProviderCatalogEntry {
            provider_id: "gemini_ai_studio",
            label: "Gemini",
            default_model: "gemini-2.5-pro",
            credential_kind: "api_key_env",
            headless_status: "configured_profile_required",
            catalog_kind: "model_provider",
            adapter_kind: None,
        },
        ProviderCatalogEntry {
            provider_id: "ollama",
            label: "Ollama",
            default_model: "llama3.2",
            credential_kind: "local_endpoint",
            headless_status: "configured_profile_required",
            catalog_kind: "model_provider",
            adapter_kind: None,
        },
        ProviderCatalogEntry {
            provider_id: "azure_openai",
            label: "Azure OpenAI",
            default_model: "deployment",
            credential_kind: "api_key_env",
            headless_status: "configured_profile_required",
            catalog_kind: "model_provider",
            adapter_kind: None,
        },
        ProviderCatalogEntry {
            provider_id: "bedrock",
            label: "Amazon Bedrock",
            default_model: "anthropic.claude-3-5-sonnet",
            credential_kind: "aws_environment",
            headless_status: "diagnostics_only",
            catalog_kind: "model_provider",
            adapter_kind: None,
        },
        ProviderCatalogEntry {
            provider_id: "vertex",
            label: "Vertex AI",
            default_model: "claude-sonnet-4@20250514",
            credential_kind: "google_environment",
            headless_status: "diagnostics_only",
            catalog_kind: "model_provider",
            adapter_kind: None,
        },
        ProviderCatalogEntry {
            provider_id: "external_codex_cli",
            label: "Codex CLI",
            default_model: "codex-cli",
            credential_kind: "external_process",
            headless_status: "approval_required",
            catalog_kind: "external_agent_adapter",
            adapter_kind: Some("codex"),
        },
        ProviderCatalogEntry {
            provider_id: "external_claude_code",
            label: "Claude Code",
            default_model: "claude-code",
            credential_kind: "external_process",
            headless_status: "approval_required",
            catalog_kind: "external_agent_adapter",
            adapter_kind: Some("claude"),
        },
        ProviderCatalogEntry {
            provider_id: "external_gemini_cli",
            label: "Gemini CLI",
            default_model: "gemini-cli",
            credential_kind: "external_process",
            headless_status: "approval_required",
            catalog_kind: "external_agent_adapter",
            adapter_kind: Some("gemini"),
        },
        ProviderCatalogEntry {
            provider_id: "external_custom_agent",
            label: "Custom External Agent",
            default_model: "external-agent",
            credential_kind: "external_process",
            headless_status: "approval_required",
            catalog_kind: "external_agent_adapter",
            adapter_kind: Some("custom"),
        },
    ]
}

fn provider_catalog_entry(provider_id: &str) -> Option<ProviderCatalogEntry> {
    provider_catalog()
        .into_iter()
        .find(|entry| entry.provider_id == provider_id)
}

fn provider_capability_for_entry(
    entry: ProviderCatalogEntry,
    profile: Option<&ProviderProfile>,
) -> ProviderCapabilityCatalog {
    let catalog_source = if entry.provider_id == FAKE_PROVIDER_ID {
        "live"
    } else if profile.is_some() || entry.catalog_kind == "external_agent_adapter" {
        "manual"
    } else {
        "unavailable"
    };
    let credential_proof = provider_credential_proof_for_entry(entry, profile);
    provider_capability_for_selection(entry, entry.default_model, catalog_source, credential_proof)
}

fn provider_credential_proof_for_entry(
    entry: ProviderCatalogEntry,
    profile: Option<&ProviderProfile>,
) -> Option<String> {
    match (entry.credential_kind, profile) {
        ("none", _) => Some("none_required".into()),
        ("external_process", _) => Some("external_process".into()),
        (_, Some(profile)) if profile.api_key_env.is_some() => Some("api_key_env_recorded".into()),
        (_, Some(profile))
            if profile
                .base_url
                .as_deref()
                .is_some_and(is_local_provider_base_url) =>
        {
            Some("local_endpoint".into())
        }
        (_, Some(_)) => Some("profile_recorded".into()),
        _ => None,
    }
}

fn provider_capability_for_selection(
    entry: ProviderCatalogEntry,
    model_id: &str,
    catalog_source: &str,
    credential_proof: Option<String>,
) -> ProviderCapabilityCatalog {
    let thinking_efforts = provider_reasoning_efforts(entry.provider_id);
    let thinking_default_effort = provider_reasoning_default_effort(entry.provider_id);

    provider_capability_catalog(ProviderCapabilityCatalogInput {
        provider_id: entry.provider_id.into(),
        model_id: model_id.into(),
        catalog_source: catalog_source.into(),
        fetched_at: None,
        last_success_at: None,
        cache_age_seconds: None,
        cache_ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
        credential_proof,
        context_window_tokens: None,
        max_output_tokens: None,
        context_limit_source: Some("unknown".into()),
        context_limit_confidence: Some("unknown".into()),
        thinking_supported: provider_supports_reasoning(entry.provider_id),
        thinking_efforts,
        thinking_default_effort,
    })
}

fn provider_reasoning_efforts(provider_id: &str) -> Vec<String> {
    match provider_id {
        "openai_api" | "openai_codex" => vec!["low".into(), "medium".into(), "high".into()],
        "openrouter" => vec![
            "minimal".into(),
            "low".into(),
            "medium".into(),
            "high".into(),
            "x_high".into(),
        ],
        "anthropic" | "bedrock" | "vertex" => {
            vec!["low".into(), "medium".into(), "high".into()]
        }
        _ => Vec::new(),
    }
}

fn provider_supports_reasoning(provider_id: &str) -> bool {
    !provider_reasoning_efforts(provider_id).is_empty()
}

fn provider_reasoning_default_effort(provider_id: &str) -> Option<String> {
    let efforts = provider_reasoning_efforts(provider_id);
    efforts
        .iter()
        .any(|effort| effort == "medium")
        .then(|| "medium".into())
}

#[derive(Debug, Clone)]
struct CliProviderExecution {
    profile_id: String,
    provider_id: String,
    model_id: String,
    execution_mode: &'static str,
    execution: HeadlessProviderExecutionConfig,
    credential_proof: Option<String>,
}

impl CliProviderExecution {
    fn with_project_workspace(mut self, store: &CliAgentStore) -> Self {
        if let (
            "real_provider",
            Some(repo_root),
            HeadlessProviderExecutionConfig::OpenAiCompatible(config),
        ) = (self.execution_mode, store.repo_root(), &mut self.execution)
        {
            config.workspace_root = Some(repo_root.to_path_buf());
        }
        self
    }
}

fn resolve_cli_provider_execution(
    globals: &GlobalOptions,
    provider_id: Option<String>,
    model_id: Option<String>,
) -> Result<CliProviderExecution, CliError> {
    let explicit_provider = provider_id
        .as_deref()
        .map(str::trim)
        .filter(|provider_id| !provider_id.is_empty());
    if explicit_provider == Some(FAKE_PROVIDER_ID) {
        return Ok(CliProviderExecution {
            profile_id: FAKE_PROVIDER_ID.into(),
            provider_id: FAKE_PROVIDER_ID.into(),
            model_id: model_id.unwrap_or_else(|| DEFAULT_MODEL_ID.into()),
            execution_mode: "fake_provider_harness",
            execution: HeadlessProviderExecutionConfig::Fake,
            credential_proof: Some("none_required".into()),
        });
    }
    if let Some(provider_id) = explicit_provider {
        ensure_owned_agent_provider(provider_id)?;
    }

    let config = load_config(globals)?;
    let profile = match explicit_provider {
        Some(provider_id) => find_provider_profile(&config, provider_id).ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_provider_profile_missing",
                format!(
                    "No headless profile is configured for `{provider_id}`. Run `xero provider login {provider_id} --api-key-env NAME`, or use `--provider fake_provider` for harness-only tests."
                ),
            )
        })?,
        None => {
            if config.providers.is_empty() {
                return Err(CliError::user_fixable(
                    "xero_cli_provider_required",
                    "Headless real execution requires a configured provider. Pass `--provider PROVIDER_ID`, or pass `--provider fake_provider` for harness-only tests.",
                ));
            }
            config.providers.values().next().expect("checked non-empty")
        }
    };
    ensure_owned_agent_provider(&profile.provider_id)?;
    if globals.ci {
        return Err(CliError::user_fixable(
            "xero_cli_provider_unavailable_in_ci",
            "Headless CI mode currently allows only `--provider fake_provider` because real-provider writes require explicit non-interactive approval policy.",
        ));
    }

    let entry = provider_catalog_entry(&profile.provider_id).ok_or_else(|| {
        CliError::user_fixable(
            "xero_cli_provider_unknown",
            format!(
                "Provider `{}` is not in Xero's headless provider catalog.",
                profile.provider_id
            ),
        )
    })?;
    let model_id = model_id.unwrap_or_else(|| entry.default_model.into());
    let execution = openai_compatible_execution_config(globals, profile, &model_id)?;
    Ok(CliProviderExecution {
        profile_id: profile.profile_id.clone(),
        provider_id: profile.provider_id.clone(),
        model_id,
        execution_mode: "real_provider",
        execution,
        credential_proof: provider_credential_proof_for_entry(entry, Some(profile)),
    })
}

fn resolve_runtime_for_existing_snapshot(
    globals: &GlobalOptions,
    snapshot: &RunSnapshot,
) -> Result<CliProviderExecution, CliError> {
    if snapshot.provider_id == FAKE_PROVIDER_ID {
        return Ok(CliProviderExecution {
            profile_id: snapshot.provider_id.clone(),
            provider_id: snapshot.provider_id.clone(),
            model_id: snapshot.model_id.clone(),
            execution_mode: "fake_provider_harness",
            execution: HeadlessProviderExecutionConfig::Fake,
            credential_proof: Some("none_required".into()),
        });
    }
    resolve_cli_provider_execution(
        globals,
        Some(snapshot.provider_id.clone()),
        Some(snapshot.model_id.clone()),
    )
}

fn find_provider_profile<'a>(config: &'a CliConfig, selector: &str) -> Option<&'a ProviderProfile> {
    config.providers.get(selector).or_else(|| {
        config
            .providers
            .values()
            .find(|profile| profile.provider_id == selector)
    })
}

fn openai_compatible_execution_config(
    globals: &GlobalOptions,
    profile: &ProviderProfile,
    model_id: &str,
) -> Result<HeadlessProviderExecutionConfig, CliError> {
    let base_url = profile
        .base_url
        .clone()
        .or_else(|| default_openai_compatible_base_url(&profile.provider_id).map(str::to_owned))
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_provider_not_executable",
                format!(
                    "Provider `{}` does not have a headless OpenAI-compatible execution route yet.",
                    profile.provider_id
                ),
            )
        })?;
    let api_key = match profile.api_key_env.as_deref() {
        Some(env_name) => Some(env::var(env_name).map_err(|_| {
            CliError::user_fixable(
                "xero_cli_provider_api_key_missing",
                format!(
                    "Environment variable `{env_name}` is not set for provider `{}`.",
                    profile.provider_id
                ),
            )
        })?),
        None => None,
    };
    let hosted_without_key = !is_local_provider_base_url(&base_url) && api_key.is_none();
    if hosted_without_key {
        return Err(CliError::user_fixable(
            "xero_cli_provider_api_key_missing",
            format!(
                "Provider `{}` targets hosted endpoint `{base_url}` without an API key environment variable.",
                profile.provider_id
            ),
        ));
    }
    Ok(HeadlessProviderExecutionConfig::OpenAiCompatible(
        OpenAiCompatibleHeadlessConfig {
            provider_id: profile.provider_id.clone(),
            model_id: model_id.into(),
            base_url,
            api_key,
            timeout_ms: 0,
            workspace_root: env::current_dir().ok(),
            allow_workspace_writes: !globals.ci,
        },
    ))
}

fn default_openai_compatible_base_url(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "openai_api" => Some("https://api.openai.com/v1"),
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "github_models" => Some("https://models.github.ai/inference"),
        "ollama" => Some("http://localhost:11434/v1"),
        "gemini_ai_studio" => Some("https://generativelanguage.googleapis.com/v1beta/openai"),
        _ => None,
    }
}

fn is_local_provider_base_url(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    lower.starts_with("http://localhost")
        || lower.starts_with("http://127.")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("http://0.0.0.0")
}

fn ensure_owned_agent_provider(provider_id: &str) -> Result<(), CliError> {
    let Some(entry) = provider_catalog_entry(provider_id) else {
        return Err(CliError::user_fixable(
            "xero_cli_provider_unknown",
            format!("Provider `{provider_id}` is not in Xero's headless provider catalog."),
        ));
    };

    if entry.catalog_kind == "external_agent_adapter" {
        return Err(CliError::user_fixable(
            "xero_cli_provider_external_agent_mismatch",
            format!(
                "Provider `{provider_id}` is an external-agent adapter. Use `xero agent host --adapter {}` so the run is labeled and audited as external.",
                entry.adapter_kind.unwrap_or("custom")
            ),
        ));
    }

    Ok(())
}

fn ensure_cli_provider_preflight_for_run(
    globals: &GlobalOptions,
    provider: &CliProviderExecution,
) -> Result<ProviderPreflightSnapshot, CliError> {
    let required_features = ProviderPreflightRequiredFeatures::owned_agent_text_turn();
    if let Some(snapshot) = load_cli_provider_preflight_snapshot(
        globals,
        &provider.profile_id,
        &provider.provider_id,
        &provider.model_id,
    )? {
        let snapshot = provider_preflight_snapshot_as_cached_probe(snapshot);
        if !snapshot.stale && snapshot.required_features == required_features {
            return reject_provider_preflight_blockers(snapshot);
        }
    }

    let snapshot = cli_provider_preflight_for_execution(provider, required_features)?;
    persist_cli_provider_preflight_snapshot(globals, &snapshot)?;
    reject_provider_preflight_blockers(snapshot)
}

fn reject_provider_preflight_blockers(
    snapshot: ProviderPreflightSnapshot,
) -> Result<ProviderPreflightSnapshot, CliError> {
    if let Some(blocker) = provider_preflight_blockers(&snapshot).first() {
        return Err(CliError::user_fixable(
            "xero_cli_provider_preflight_blocked",
            format!(
                "Provider preflight blocked {}/{} because `{}` failed: {}",
                snapshot.provider_id, snapshot.model_id, blocker.code, blocker.message
            ),
        ));
    }
    Ok(snapshot)
}

fn cli_provider_preflight_for_selection(
    globals: &GlobalOptions,
    provider_selector: Option<&str>,
    profile_selector: Option<&str>,
    model_selector: Option<&str>,
    force: bool,
) -> Result<ProviderPreflightSnapshot, CliError> {
    let config = load_config(globals)?;
    let provider_id = provider_selector
        .map(str::trim)
        .filter(|provider_id| !provider_id.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            profile_selector.and_then(|profile_id| {
                config
                    .providers
                    .get(profile_id)
                    .map(|profile| profile.provider_id.clone())
            })
        })
        .unwrap_or_else(|| FAKE_PROVIDER_ID.into());
    ensure_owned_agent_provider(&provider_id)?;
    let entry = provider_catalog_entry(&provider_id).ok_or_else(|| {
        CliError::user_fixable(
            "xero_cli_provider_unknown",
            format!("Provider `{provider_id}` is not in Xero's headless provider catalog."),
        )
    })?;

    let profile = if provider_id == FAKE_PROVIDER_ID {
        None
    } else if let Some(profile_id) = profile_selector {
        Some(config.providers.get(profile_id).ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_provider_profile_missing",
                format!("No headless provider profile `{profile_id}` is configured."),
            )
        })?)
    } else {
        config
            .providers
            .values()
            .find(|profile| profile.provider_id == provider_id)
            .ok_or_else(|| {
                CliError::user_fixable(
                    "xero_cli_provider_profile_missing",
                    format!(
                        "No headless profile is configured for `{provider_id}`. Run `xero provider login {provider_id} --api-key-env NAME`."
                    ),
                )
            })
            .map(Some)?
    };

    let profile_id = profile
        .map(|profile| profile.profile_id.clone())
        .unwrap_or_else(|| FAKE_PROVIDER_ID.into());
    let model_id = model_selector
        .map(str::trim)
        .filter(|model_id| !model_id.is_empty())
        .unwrap_or(entry.default_model);
    if !force {
        if let Some(snapshot) =
            load_cli_provider_preflight_snapshot(globals, &profile_id, &provider_id, model_id)?
        {
            return Ok(provider_preflight_snapshot_as_cached_probe(snapshot));
        }
    }

    let snapshot = if provider_id == FAKE_PROVIDER_ID {
        let credential_proof = provider_credential_proof_for_entry(entry, profile);
        cli_provider_preflight_snapshot(CliProviderPreflightSnapshotInput {
            profile_id: &profile_id,
            provider_id: &provider_id,
            model_id,
            entry,
            source: ProviderPreflightSource::LiveProbe,
            credential_proof,
            credential_ready: true,
            required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
        })
    } else {
        let profile = profile.ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_provider_profile_missing",
                format!("No headless provider profile `{profile_id}` is configured."),
            )
        })?;
        let execution = openai_compatible_execution_config(globals, profile, model_id)?;
        let provider = CliProviderExecution {
            profile_id: profile.profile_id.clone(),
            provider_id: profile.provider_id.clone(),
            model_id: model_id.into(),
            execution_mode: "real_provider",
            execution,
            credential_proof: provider_credential_proof_for_entry(entry, Some(profile)),
        };
        cli_provider_preflight_for_execution(
            &provider,
            ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
        )?
    };
    persist_cli_provider_preflight_snapshot(globals, &snapshot)?;
    Ok(snapshot)
}

fn cli_provider_preflight_for_execution(
    provider: &CliProviderExecution,
    required_features: ProviderPreflightRequiredFeatures,
) -> Result<ProviderPreflightSnapshot, CliError> {
    let entry = provider_catalog_entry(&provider.provider_id).ok_or_else(|| {
        CliError::user_fixable(
            "xero_cli_provider_unknown",
            format!(
                "Provider `{}` is not in Xero's headless provider catalog.",
                provider.provider_id
            ),
        )
    })?;
    match &provider.execution {
        HeadlessProviderExecutionConfig::Fake => Ok(cli_provider_preflight_snapshot(
            CliProviderPreflightSnapshotInput {
                profile_id: &provider.profile_id,
                provider_id: &provider.provider_id,
                model_id: &provider.model_id,
                entry,
                source: ProviderPreflightSource::LiveProbe,
                credential_proof: provider.credential_proof.clone(),
                credential_ready: cli_provider_execution_credentials_available(&provider.execution),
                required_features,
            },
        )),
        HeadlessProviderExecutionConfig::OpenAiCompatible(config) => {
            Ok(run_openai_compatible_provider_preflight_probe(
                OpenAiCompatibleProviderPreflightProbeRequest {
                    profile_id: provider.profile_id.clone(),
                    provider_id: config.provider_id.clone(),
                    model_id: config.model_id.clone(),
                    base_url: config.base_url.clone(),
                    api_key: config.api_key.clone(),
                    timeout_ms: config.timeout_ms,
                    required_features,
                    credential_proof: provider.credential_proof.clone(),
                    context_window_tokens: Some(128_000),
                    max_output_tokens: Some(16_384),
                    context_limit_source: Some("configured_default".into()),
                    context_limit_confidence: Some("medium".into()),
                    thinking_supported: provider_supports_reasoning(entry.provider_id),
                    thinking_efforts: provider_reasoning_efforts(entry.provider_id),
                    thinking_default_effort: provider_reasoning_default_effort(entry.provider_id),
                },
            ))
        }
    }
}

struct CliProviderPreflightSnapshotInput<'a> {
    profile_id: &'a str,
    provider_id: &'a str,
    model_id: &'a str,
    entry: ProviderCatalogEntry,
    source: ProviderPreflightSource,
    credential_proof: Option<String>,
    credential_ready: bool,
    required_features: ProviderPreflightRequiredFeatures,
}

fn cli_provider_preflight_snapshot(
    input: CliProviderPreflightSnapshotInput<'_>,
) -> ProviderPreflightSnapshot {
    let CliProviderPreflightSnapshotInput {
        profile_id,
        provider_id,
        model_id,
        entry,
        source,
        credential_proof,
        credential_ready,
        required_features,
    } = input;
    let catalog_source = match source {
        ProviderPreflightSource::LiveProbe | ProviderPreflightSource::LiveCatalog => "live",
        ProviderPreflightSource::CachedProbe => "cache",
        ProviderPreflightSource::StaticManual => "manual",
        ProviderPreflightSource::Unavailable => "unavailable",
    };
    let capabilities = if provider_id == FAKE_PROVIDER_ID {
        provider_capability_catalog(ProviderCapabilityCatalogInput {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            catalog_source: catalog_source.into(),
            fetched_at: Some(now_timestamp()),
            last_success_at: Some(now_timestamp()),
            cache_age_seconds: Some(0),
            cache_ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
            credential_proof,
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(16_384),
            context_limit_source: Some("built_in_registry".into()),
            context_limit_confidence: Some("high".into()),
            thinking_supported: false,
            thinking_efforts: Vec::new(),
            thinking_default_effort: None,
        })
    } else {
        provider_capability_for_selection(entry, model_id, catalog_source, credential_proof)
    };
    let live_probe = matches!(source, ProviderPreflightSource::LiveProbe);
    let context_limit_known = capabilities
        .capabilities
        .context_limits
        .context_window_tokens
        .is_some()
        && capabilities.capabilities.context_limits.confidence != "unknown";
    let context_limit_known = if live_probe || context_limit_known {
        Some(context_limit_known)
    } else {
        None
    };
    provider_preflight_snapshot(ProviderPreflightInput {
        profile_id: profile_id.into(),
        provider_id: provider_id.into(),
        model_id: model_id.into(),
        source,
        checked_at: now_timestamp(),
        age_seconds: Some(0),
        ttl_seconds: None,
        required_features,
        capabilities,
        credential_ready: Some(credential_ready),
        endpoint_reachable: live_probe.then_some(true),
        model_available: live_probe.then_some(true),
        streaming_route_available: live_probe.then_some(true),
        tool_schema_accepted: live_probe.then_some(true),
        reasoning_controls_accepted: None,
        attachments_accepted: None,
        context_limit_known,
        provider_error: None,
    })
}

fn cli_provider_execution_credentials_available(
    execution: &HeadlessProviderExecutionConfig,
) -> bool {
    match execution {
        HeadlessProviderExecutionConfig::Fake => true,
        HeadlessProviderExecutionConfig::OpenAiCompatible(config) => {
            config
                .api_key
                .as_deref()
                .is_some_and(|key| !key.trim().is_empty())
                || is_local_provider_base_url(&config.base_url)
        }
    }
}

fn load_cli_provider_preflight_snapshot(
    globals: &GlobalOptions,
    profile_id: &str,
    provider_id: &str,
    model_id: &str,
) -> Result<Option<ProviderPreflightSnapshot>, CliError> {
    let store = load_provider_preflight_store(globals)?;
    Ok(store
        .snapshots
        .get(&provider_preflight_key(profile_id, provider_id, model_id))
        .cloned())
}

fn persist_cli_provider_preflight_snapshot(
    globals: &GlobalOptions,
    snapshot: &ProviderPreflightSnapshot,
) -> Result<(), CliError> {
    let mut store = load_provider_preflight_store(globals)?;
    store.snapshots.insert(
        provider_preflight_key(
            &snapshot.profile_id,
            &snapshot.provider_id,
            &snapshot.model_id,
        ),
        snapshot.clone(),
    );
    write_json_file(
        &globals.state_dir.join(PROVIDER_PREFLIGHT_STATE_FILE),
        &store,
    )
}

fn load_provider_preflight_store(
    globals: &GlobalOptions,
) -> Result<ProviderPreflightStore, CliError> {
    let path = globals.state_dir.join(PROVIDER_PREFLIGHT_STATE_FILE);
    if !path.exists() {
        return Ok(ProviderPreflightStore::default());
    }
    read_json_file(&path)
}

fn provider_preflight_key(profile_id: &str, provider_id: &str, model_id: &str) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        profile_id.trim(),
        provider_id.trim(),
        model_id.trim()
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CliConfig {
    schema_version: u32,
    providers: BTreeMap<String, ProviderProfile>,
    mcp_servers: BTreeMap<String, McpServerConfig>,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            providers: BTreeMap::new(),
            mcp_servers: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderProfile {
    profile_id: String,
    provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderPreflightStore {
    schema_version: u32,
    snapshots: BTreeMap<String, ProviderPreflightSnapshot>,
}

impl Default for ProviderPreflightStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            snapshots: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpServerConfig {
    name: String,
    command: String,
    args: Vec<String>,
    env: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token_env: Option<String>,
    added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderDoctorCheck {
    code: String,
    status: String,
    message: String,
}

fn provider_doctor_checks(
    provider_id: &str,
    profile: Option<&ProviderProfile>,
) -> Vec<ProviderDoctorCheck> {
    if provider_id == FAKE_PROVIDER_ID {
        return vec![ProviderDoctorCheck {
            code: "provider_fake_ready".into(),
            status: "passed".into(),
            message:
                "The built-in fake provider is ready for explicit harness-only protocol and storage checks."
                    .into(),
        }];
    }

    if let Some(entry) = provider_catalog_entry(provider_id)
        .filter(|entry| entry.catalog_kind == "external_agent_adapter")
    {
        let adapter = external_agent_adapter(entry.adapter_kind.unwrap_or("custom"));
        let mut checks = vec![ProviderDoctorCheck {
            code: "provider_external_agent_cataloged".into(),
            status: "passed".into(),
            message: format!(
                "`{provider_id}` is cataloged as an external-agent adapter, separate from model providers."
            ),
        }];
        match adapter.ok().and_then(|adapter| adapter.default_command) {
            Some(command) if command_available(command) => checks.push(ProviderDoctorCheck {
                code: "provider_external_agent_command_found".into(),
                status: "passed".into(),
                message: format!("Default command `{command}` is available on PATH."),
            }),
            Some(command) => checks.push(ProviderDoctorCheck {
                code: "provider_external_agent_command_missing".into(),
                status: "failed".into(),
                message: format!(
                    "Default command `{command}` was not found on PATH. Use `xero agent host --adapter {} --command PATH --allow-subprocess` or install the CLI.",
                    entry.adapter_kind.unwrap_or("custom")
                ),
            }),
            None => checks.push(ProviderDoctorCheck {
                code: "provider_external_agent_custom_command_required".into(),
                status: "passed".into(),
                message: "Custom external agents require an explicit `--command` at launch time."
                    .into(),
            }),
        }
        return checks;
    }

    let Some(profile) = profile else {
        return vec![ProviderDoctorCheck {
            code: "provider_profile_missing".into(),
            status: "failed".into(),
            message: format!(
                "No headless profile is configured for `{provider_id}`. Run `xero provider login {provider_id} --api-key-env NAME`."
            ),
        }];
    };

    let mut checks = vec![ProviderDoctorCheck {
        code: "provider_profile_recorded".into(),
        status: "passed".into(),
        message: format!(
            "Profile `{}` is recorded in headless app-data state.",
            profile.profile_id
        ),
    }];

    match profile.api_key_env.as_deref() {
        Some(env_name) if env::var_os(env_name).is_some() => checks.push(ProviderDoctorCheck {
            code: "provider_api_key_env_present".into(),
            status: "passed".into(),
            message: format!("Environment variable `{env_name}` is set."),
        }),
        Some(env_name) => checks.push(ProviderDoctorCheck {
            code: "provider_api_key_env_missing".into(),
            status: "failed".into(),
            message: format!("Environment variable `{env_name}` is not set."),
        }),
        None => checks.push(ProviderDoctorCheck {
            code: "provider_api_key_env_not_required".into(),
            status: "passed".into(),
            message: "This provider profile does not require an API key environment variable."
                .into(),
        }),
    }

    checks
}

#[derive(Debug, Clone, Copy)]
struct ExternalAgentAdapter {
    adapter_id: &'static str,
    provider_id: &'static str,
    label: &'static str,
    default_model_id: &'static str,
    default_command: Option<&'static str>,
    default_args_before_prompt: &'static [&'static str],
}

#[derive(Debug)]
struct ExternalAgentRunRequest {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    prompt: String,
    adapter: ExternalAgentAdapter,
    command: String,
    argv: Vec<String>,
    model_id: String,
    timeout_ms: u64,
    approval_source: ExternalAgentApprovalSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExternalAgentApprovalSource {
    OperatorFlag,
    Environment,
}

fn external_agent_adapter(adapter_id: &str) -> Result<ExternalAgentAdapter, CliError> {
    match adapter_id {
        "codex" | "codex_cli" | "external_codex_cli" => Ok(ExternalAgentAdapter {
            adapter_id: "codex",
            provider_id: "external_codex_cli",
            label: "Codex CLI",
            default_model_id: "codex-cli",
            default_command: Some("codex"),
            default_args_before_prompt: &["exec"],
        }),
        "claude" | "claude_code" | "external_claude_code" => Ok(ExternalAgentAdapter {
            adapter_id: "claude",
            provider_id: "external_claude_code",
            label: "Claude Code",
            default_model_id: "claude-code",
            default_command: Some("claude"),
            default_args_before_prompt: &["-p"],
        }),
        "gemini" | "gemini_cli" | "external_gemini_cli" => Ok(ExternalAgentAdapter {
            adapter_id: "gemini",
            provider_id: "external_gemini_cli",
            label: "Gemini CLI",
            default_model_id: "gemini-cli",
            default_command: Some("gemini"),
            default_args_before_prompt: &["-p"],
        }),
        "custom" | "external_custom_agent" => Ok(ExternalAgentAdapter {
            adapter_id: "custom",
            provider_id: "external_custom_agent",
            label: "Custom External Agent",
            default_model_id: "external-agent",
            default_command: None,
            default_args_before_prompt: &[],
        }),
        other => Err(CliError::usage(format!(
            "Unknown external agent adapter `{other}`. Use codex, claude, gemini, or custom."
        ))),
    }
}

fn external_agent_argv(
    adapter: &ExternalAgentAdapter,
    explicit_args: Vec<String>,
    prompt: &str,
) -> Vec<String> {
    if explicit_args.is_empty() {
        let mut argv = adapter
            .default_args_before_prompt
            .iter()
            .map(|arg| (*arg).to_owned())
            .collect::<Vec<_>>();
        argv.push(prompt.to_owned());
        return argv;
    }

    let mut replaced = false;
    let argv = explicit_args
        .into_iter()
        .map(|arg| {
            if arg.contains("{prompt}") {
                replaced = true;
                arg.replace("{prompt}", prompt)
            } else {
                arg
            }
        })
        .collect::<Vec<_>>();
    if replaced {
        argv
    } else {
        let mut argv = argv;
        argv.push(prompt.to_owned());
        argv
    }
}

fn external_agent_approval_source(allow_subprocess: bool) -> Option<ExternalAgentApprovalSource> {
    if allow_subprocess {
        return Some(ExternalAgentApprovalSource::OperatorFlag);
    }
    env::var("XERO_EXTERNAL_AGENT_APPROVED")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .map(|_| ExternalAgentApprovalSource::Environment)
}

fn host_external_agent_run(
    globals: &GlobalOptions,
    store: &FileAgentCoreStore,
    request: ExternalAgentRunRequest,
) -> Result<RunSnapshot, CliError> {
    validate_required_cli(&request.project_id, "projectId")?;
    validate_required_cli(&request.agent_session_id, "sessionId")?;
    validate_required_cli(&request.run_id, "runId")?;
    validate_required_cli(&request.command, "command")?;
    if request.command.contains('\n') {
        return Err(CliError::usage(
            "External agent command cannot contain a newline.",
        ));
    }

    let sandbox_metadata = external_agent_sandbox_metadata(globals, &request)?;
    let snapshot = store
        .insert_run(NewRunRecord {
            trace_id: None,
            project_id: request.project_id.clone(),
            agent_session_id: request.agent_session_id.clone(),
            run_id: request.run_id.clone(),
            provider_id: request.adapter.provider_id.into(),
            model_id: request.model_id.clone(),
            prompt: request.prompt.clone(),
        })
        .map_err(core_error)?;
    store
        .append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::RunStarted,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "external_run_started",
            )),
            payload: json!({
                "status": "starting",
                "providerId": snapshot.provider_id,
                "modelId": snapshot.model_id,
                "provenance": external_agent_provenance(&request),
                "sandbox": external_agent_sandbox_payload(&sandbox_metadata, request.timeout_ms),
            }),
        })
        .map_err(core_error)?;
    store
        .append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::PolicyDecision,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "external_subprocess_approved",
            )),
            payload: json!({
                "decision": "allow",
                "policy": "external_agent_subprocess_requires_explicit_approval",
                "approvalSource": match request.approval_source {
                    ExternalAgentApprovalSource::OperatorFlag => "operator_flag",
                    ExternalAgentApprovalSource::Environment => "environment",
                },
                "command": request.command,
                "argv": request.argv,
                "timeoutMs": request.timeout_ms,
            }),
        })
        .map_err(core_error)?;
    store
        .append_message(NewMessageRecord {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            role: MessageRole::System,
            content: format!(
                "External agent session hosted by Xero. Provenance: {} ({})",
                request.adapter.label, request.adapter.provider_id
            ),
            provider_metadata: None,
        })
        .map_err(core_error)?;
    store
        .append_message(NewMessageRecord {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            role: MessageRole::User,
            content: request.prompt.clone(),
            provider_metadata: None,
        })
        .map_err(core_error)?;
    store
        .update_run_status(&snapshot.project_id, &snapshot.run_id, RunStatus::Running)
        .map_err(core_error)?;

    let mut process_argv = vec![request.command.clone()];
    process_argv.extend(request.argv.iter().cloned());
    let output = SandboxedProcessRunner::new().run(
        SandboxedProcessRequest {
            argv: process_argv,
            cwd: env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().into_owned()),
            timeout_ms: Some(request.timeout_ms),
            stdout_limit_bytes: 64 * 1024,
            stderr_limit_bytes: 64 * 1024,
            metadata: sandbox_metadata,
        },
        || false,
    );
    let run = store
        .load_run(&snapshot.project_id, &snapshot.run_id)
        .map_err(core_error)?;
    let completed_snapshot = match output {
        Ok(output) => {
            let stdout = output.stdout.unwrap_or_default();
            let stderr = output.stderr.unwrap_or_default();
            if !stdout.is_empty() {
                append_external_agent_output_event(store, &run, "stdout", &stdout)?;
            }
            if !stderr.is_empty() {
                append_external_agent_output_event(store, &run, "stderr", &stderr)?;
            }
            let assistant_content = if stdout.trim().is_empty() {
                "External agent completed without stdout output.".into()
            } else {
                stdout.trim().to_owned()
            };
            store
                .append_message(NewMessageRecord {
                    project_id: run.project_id.clone(),
                    run_id: run.run_id.clone(),
                    role: MessageRole::Assistant,
                    content: assistant_content.clone(),
                    provider_metadata: None,
                })
                .map_err(core_error)?;
            store
                .append_event(NewRuntimeEvent {
                    project_id: run.project_id.clone(),
                    run_id: run.run_id.clone(),
                    event_kind: RuntimeEventKind::MessageDelta,
                    trace: Some(RuntimeTraceContext::for_provider_turn(
                        &run.trace_id,
                        &run.run_id,
                        0,
                    )),
                    payload: json!({
                        "role": "assistant",
                        "text": assistant_content,
                        "provenance": external_agent_provenance(&request),
                    }),
                })
                .map_err(core_error)?;

            if output.exit_code == Some(0) {
                store
                    .update_run_status(&run.project_id, &run.run_id, RunStatus::Completed)
                    .map_err(core_error)?;
                store
                    .append_event(NewRuntimeEvent {
                        project_id: run.project_id.clone(),
                        run_id: run.run_id.clone(),
                        event_kind: RuntimeEventKind::RunCompleted,
                        trace: Some(RuntimeTraceContext::for_run(
                            &run.trace_id,
                            &run.run_id,
                            "external_run_completed",
                        )),
                        payload: json!({
                            "summary": "External agent subprocess completed.",
                            "exitCode": output.exit_code,
                            "provenance": external_agent_provenance(&request),
                            "sandbox": external_agent_sandbox_payload(&output.metadata, request.timeout_ms),
                        }),
                    })
                    .map_err(core_error)?;
            } else {
                store
                    .update_run_status(&run.project_id, &run.run_id, RunStatus::Failed)
                    .map_err(core_error)?;
                store
                    .append_event(NewRuntimeEvent {
                        project_id: run.project_id.clone(),
                        run_id: run.run_id.clone(),
                        event_kind: RuntimeEventKind::RunFailed,
                        trace: Some(RuntimeTraceContext::for_run(
                            &run.trace_id,
                            &run.run_id,
                            "external_run_failed",
                        )),
                        payload: json!({
                            "code": "external_agent_exit_failed",
                            "message": "External agent subprocess exited unsuccessfully.",
                            "exitCode": output.exit_code,
                            "provenance": external_agent_provenance(&request),
                            "sandbox": external_agent_sandbox_payload(&output.metadata, request.timeout_ms),
                        }),
                    })
                    .map_err(core_error)?;
            }
            store
                .load_run(&run.project_id, &run.run_id)
                .map_err(core_error)?
        }
        Err(error) => {
            store
                .update_run_status(&run.project_id, &run.run_id, RunStatus::Failed)
                .map_err(core_error)?;
            store
                .append_event(NewRuntimeEvent {
                    project_id: run.project_id.clone(),
                    run_id: run.run_id.clone(),
                    event_kind: RuntimeEventKind::RunFailed,
                    trace: Some(RuntimeTraceContext::for_run(
                        &run.trace_id,
                        &run.run_id,
                        "external_spawn_failed",
                    )),
                    payload: json!({
                        "code": error.code,
                        "message": error.message,
                        "provenance": external_agent_provenance(&request),
                        "sandbox": external_agent_sandbox_payload(&error.metadata, request.timeout_ms),
                    }),
                })
                .map_err(core_error)?;
            store
                .load_run(&run.project_id, &run.run_id)
                .map_err(core_error)?
        }
    };

    Ok(completed_snapshot)
}

fn external_agent_sandbox_metadata(
    globals: &GlobalOptions,
    request: &ExternalAgentRunRequest,
) -> Result<SandboxExecutionMetadata, CliError> {
    let workspace_root = env::current_dir()
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_current_dir_failed",
                format!("Could not inspect the current workspace directory: {error}"),
            )
        })?
        .display()
        .to_string();
    let descriptor = ToolDescriptorV2 {
        name: "external_agent_host".into(),
        description: "Launch an explicitly approved external agent subprocess and capture its output as an auditable Xero run.".into(),
        input_schema: json!({ "type": "object" }),
        capability_tags: vec!["external_agent".into(), "subprocess".into()],
        application_metadata: Default::default(),
        effect_class: ToolEffectClass::CommandExecution,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::FullLocal,
        approval_requirement: ToolApprovalRequirement::Always,
        telemetry_attributes: BTreeMap::new(),
        result_truncation: Default::default(),
    };
    let sandbox = PermissionProfileSandbox::new(SandboxExecutionContext {
        workspace_root,
        app_data_roots: vec![globals.state_dir.display().to_string()],
        project_trust: ProjectTrustState::UserApproved,
        approval_source: match request.approval_source {
            ExternalAgentApprovalSource::OperatorFlag => SandboxApprovalSource::Operator,
            ExternalAgentApprovalSource::Environment => SandboxApprovalSource::Policy,
        },
        platform: SandboxPlatform::current(),
        explicit_git_mutation_allowed: false,
        legacy_xero_migration_allowed: false,
        preserved_environment_keys: vec![
            "PATH".into(),
            "HOME".into(),
            "USER".into(),
            "LOGNAME".into(),
            "SHELL".into(),
            "TMPDIR".into(),
            "TMP".into(),
            "TEMP".into(),
        ],
    });
    let call = ToolCallInput {
        tool_call_id: format!("external-agent-{}", request.run_id),
        tool_name: descriptor.name.clone(),
        input: json!({
            "command": request.command,
            "argv": request.argv,
            "projectId": request.project_id,
            "runId": request.run_id,
        }),
    };
    sandbox
        .evaluate(&descriptor, &call, &ToolExecutionContext::default())
        .map_err(|denied| CliError::user_fixable(denied.error.code, denied.error.message))
}

fn external_agent_sandbox_payload(
    metadata: &SandboxExecutionMetadata,
    timeout_ms: u64,
) -> JsonValue {
    json!({
        "metadata": metadata,
        "enforcement": "os_sandbox_runner",
        "subprocessStdio": "piped",
        "shellExpansion": false,
        "timeoutMs": timeout_ms,
    })
}

fn append_external_agent_output_event(
    store: &FileAgentCoreStore,
    run: &RunSnapshot,
    stream: &str,
    output: &str,
) -> Result<(), CliError> {
    store
        .append_event(NewRuntimeEvent {
            project_id: run.project_id.clone(),
            run_id: run.run_id.clone(),
            event_kind: RuntimeEventKind::CommandOutput,
            trace: Some(RuntimeTraceContext::for_provider_turn(
                &run.trace_id,
                &run.run_id,
                0,
            )),
            payload: json!({
                "stream": stream,
                "text": truncate_bytes(output, 64 * 1024),
            }),
        })
        .map(|_| ())
        .map_err(core_error)
}

fn external_agent_provenance(request: &ExternalAgentRunRequest) -> JsonValue {
    json!({
        "kind": "external_agent",
        "adapterId": request.adapter.adapter_id,
        "adapterLabel": request.adapter.label,
        "providerId": request.adapter.provider_id,
        "modelId": request.model_id,
        "command": request.command,
        "argv": request.argv,
    })
}

fn command_available(command: &str) -> bool {
    if command.contains('/') || command.contains('\\') {
        return Path::new(command).exists();
    }
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|path| path.join(command).is_file())
}

fn validate_required_cli(value: &str, field: &str) -> Result<(), CliError> {
    if value.trim().is_empty() {
        Err(CliError::usage(format!("`{field}` is required.")))
    } else {
        Ok(())
    }
}

fn run_mcp_stdio_server(globals: GlobalOptions) -> Result<(), CliError> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    run_mcp_jsonrpc_stream(globals, stdin.lock(), &mut stdout)
}

fn run_mcp_jsonrpc_stream<R: BufRead, W: Write>(
    globals: GlobalOptions,
    reader: R,
    writer: &mut W,
) -> Result<(), CliError> {
    let mut session = McpServerSession { globals };
    for line in reader.lines() {
        let line = line.map_err(|error| {
            CliError::system_fault("xero_mcp_stdio_read_failed", error.to_string())
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<JsonValue>(&line) {
            Ok(message) => session.handle_message(message),
            Err(error) => Some(mcp_error(
                JsonValue::Null,
                -32700,
                "Parse error",
                json!({
                    "message": error.to_string()
                }),
            )),
        };
        if let Some(response) = response {
            let payload = serde_json::to_string(&response).map_err(|error| {
                CliError::system_fault("xero_mcp_encode_failed", error.to_string())
            })?;
            writer.write_all(payload.as_bytes()).map_err(|error| {
                CliError::system_fault("xero_mcp_stdio_write_failed", error.to_string())
            })?;
            writer.write_all(b"\n").map_err(|error| {
                CliError::system_fault("xero_mcp_stdio_write_failed", error.to_string())
            })?;
            writer.flush().map_err(|error| {
                CliError::system_fault("xero_mcp_stdio_flush_failed", error.to_string())
            })?;
        }
    }
    Ok(())
}

struct McpServerSession {
    globals: GlobalOptions,
}

impl McpServerSession {
    fn handle_message(&mut self, message: JsonValue) -> Option<JsonValue> {
        let Some(object) = message.as_object() else {
            return Some(mcp_error(
                JsonValue::Null,
                -32600,
                "Invalid Request",
                json!({"message": "JSON-RPC messages must be objects."}),
            ));
        };
        let id = object.get("id").cloned();
        let Some(method) = object.get("method").and_then(JsonValue::as_str) else {
            return id.map(|id| {
                mcp_error(
                    id,
                    -32600,
                    "Invalid Request",
                    json!({"message": "Requests must include a string method."}),
                )
            });
        };
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));

        if id.is_none() {
            return match method {
                "notifications/initialized" | "notifications/cancelled" => None,
                _ => None,
            };
        }
        let id = id.unwrap_or(JsonValue::Null);

        let result = match method {
            "initialize" => Ok(mcp_initialize_result(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": mcp_tool_definitions() })),
            "tools/call" => self.handle_tool_call(params, &id),
            _ => Err(mcp_error(
                id.clone(),
                -32601,
                "Method not found",
                json!({ "method": method }),
            )),
        };

        Some(match result {
            Ok(result) => mcp_success(id, result),
            Err(error) => error,
        })
    }

    fn handle_tool_call(
        &mut self,
        params: JsonValue,
        request_id: &JsonValue,
    ) -> Result<JsonValue, JsonValue> {
        let name = params
            .get("name")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                mcp_error(
                    request_id.clone(),
                    -32602,
                    "Invalid params",
                    json!({"message": "`name` is required for tools/call."}),
                )
            })?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return Err(mcp_error(
                request_id.clone(),
                -32602,
                "Invalid params",
                json!({"message": "`arguments` must be an object."}),
            ));
        }

        match name {
            "xero_start_run" => Ok(self.tool_start_run(&arguments)),
            "xero_provider_preflight" => Ok(self.tool_provider_preflight(&arguments)),
            "xero_query_conversation" => Ok(self.tool_query_conversation(&arguments)),
            "xero_query_workspace_index" => Ok(self.tool_query_workspace_index(&arguments)),
            "xero_fetch_project_memory" => Ok(self.tool_fetch_project_memory(&arguments)),
            "xero_invoke_tool_pack" => Ok(self.tool_invoke_tool_pack(&arguments)),
            "xero_export_trace" => Ok(self.tool_export_trace(&arguments)),
            _ => Err(mcp_error(
                request_id.clone(),
                -32602,
                "Unknown tool",
                json!({ "toolName": name }),
            )),
        }
    }

    fn tool_start_run(&self, arguments: &JsonValue) -> JsonValue {
        let Some(prompt) = mcp_string_arg(arguments, "prompt") else {
            return mcp_tool_error(
                "xero_mcp_prompt_required",
                "`prompt` is required to start a run.",
            );
        };
        let provider_id = mcp_string_arg(arguments, "providerId");
        let project_id = mcp_string_arg(arguments, "projectId");
        let agent_session_id =
            mcp_string_arg(arguments, "sessionId").unwrap_or_else(|| generate_id("mcp-session"));
        let run_id = mcp_string_arg(arguments, "runId").unwrap_or_else(|| generate_id("mcp-run"));
        let model_id = mcp_string_arg(arguments, "modelId");
        let provider = match resolve_cli_provider_execution(&self.globals, provider_id, model_id) {
            Ok(provider) => provider,
            Err(error) => return mcp_tool_error(error.code, error.message),
        };
        let (project_id, store) =
            match open_run_store_for_provider(&self.globals, project_id, &provider) {
                Ok(resolved) => resolved,
                Err(error) => return mcp_tool_error(error.code, error.message),
            };
        let provider = provider.with_project_workspace(&store);
        if provider.execution_mode == "real_provider" {
            if let Err(error) =
                ensure_cli_real_provider_runtime_contract(&project_id, &provider, &store)
            {
                return mcp_tool_error(error.code, error.message);
            }
        }
        let provider_preflight =
            match ensure_cli_provider_preflight_for_run(&self.globals, &provider) {
                Ok(snapshot) => snapshot,
                Err(error) => return mcp_tool_error(error.code, error.message),
            };

        let runtime = HeadlessProviderRuntime::new(
            store.clone(),
            provider.execution.clone(),
            HeadlessRuntimeOptions {
                ci_mode: self.globals.ci,
                provider_preflight: Some(provider_preflight.clone()),
                ..HeadlessRuntimeOptions::default()
            },
        );
        match runtime.start_run(StartRunRequest {
            project_id,
            agent_session_id,
            run_id,
            prompt,
            provider: ProviderSelection {
                provider_id: provider.provider_id,
                model_id: provider.model_id,
            },
            controls: Some(RunControls {
                runtime_agent_id: "mcp_harness".into(),
                approval_mode: if self.globals.ci {
                    "strict"
                } else {
                    "on_request"
                }
                .into(),
                plan_mode_required: self.globals.ci,
            }),
        }) {
            Ok(snapshot) => mcp_tool_success(
                format!(
                    "Started and completed Xero owned-agent run `{}` with provider `{}`.",
                    snapshot.run_id, snapshot.provider_id
                ),
                json!({
                    "snapshot": snapshot,
                    "storePath": store.path(),
                    "providerPreflight": provider_preflight,
                }),
            ),
            Err(error) => mcp_tool_error(error.code, error.message),
        }
    }

    fn tool_provider_preflight(&self, arguments: &JsonValue) -> JsonValue {
        let provider_id = mcp_string_arg(arguments, "providerId");
        let profile_id = mcp_string_arg(arguments, "profileId");
        let model_id = mcp_string_arg(arguments, "modelId");
        let force = mcp_bool_arg(arguments, "force").unwrap_or(false);
        match cli_provider_preflight_for_selection(
            &self.globals,
            provider_id.as_deref(),
            profile_id.as_deref(),
            model_id.as_deref(),
            force,
        ) {
            Ok(snapshot) => {
                let blockers = provider_preflight_blockers(&snapshot);
                mcp_tool_success(
                    format!(
                        "Provider preflight for `{}/{}` completed with `{}`.",
                        snapshot.provider_id,
                        snapshot.model_id,
                        snapshot.status.as_str()
                    ),
                    json!({ "snapshot": snapshot, "blockers": blockers }),
                )
            }
            Err(error) => mcp_tool_error(error.code, error.message),
        }
    }

    fn tool_query_conversation(&self, arguments: &JsonValue) -> JsonValue {
        let Some(run_id) = mcp_string_arg(arguments, "runId") else {
            return mcp_tool_error(
                "xero_mcp_run_id_required",
                "`runId` is required to query a conversation.",
            );
        };
        let include_events = mcp_bool_arg(arguments, "includeEvents").unwrap_or(false);
        let project_id = mcp_string_arg(arguments, "projectId");
        match load_conversation_snapshot(&self.globals, project_id.as_deref(), &run_id) {
            Ok((_store, snapshot)) => {
                let payload = if include_events {
                    json!({ "snapshot": snapshot })
                } else {
                    json!({
                        "snapshot": {
                            "traceId": snapshot.trace_id,
                            "projectId": snapshot.project_id,
                            "agentSessionId": snapshot.agent_session_id,
                            "runId": snapshot.run_id,
                            "providerId": snapshot.provider_id,
                            "modelId": snapshot.model_id,
                            "status": snapshot.status,
                            "prompt": snapshot.prompt,
                            "messages": snapshot.messages,
                            "contextManifests": snapshot.context_manifests,
                            "eventCount": snapshot.events.len(),
                        }
                    })
                };
                mcp_tool_success("Loaded Xero conversation from durable state.", payload)
            }
            Err(error) => mcp_tool_error(error.code, error.message),
        }
    }

    fn tool_query_workspace_index(&self, arguments: &JsonValue) -> JsonValue {
        let Some(query) = mcp_string_arg(arguments, "query") else {
            return mcp_tool_error(
                "xero_mcp_query_required",
                "`query` is required to search the workspace index.",
            );
        };
        let repo = mcp_string_arg(arguments, "repo").unwrap_or_else(|| ".".into());
        let repo_root = match canonicalize_existing_path(&repo) {
            Ok(repo_root) => repo_root,
            Err(error) => return mcp_tool_error(error.code, error.message),
        };
        let project_id = mcp_string_arg(arguments, "projectId")
            .unwrap_or_else(|| stable_project_id_for_repo_root(&repo_root));
        let mode = match mcp_string_arg(arguments, "mode")
            .map(|value| parse_workspace_query_mode(&value))
            .transpose()
        {
            Ok(mode) => mode.unwrap_or(WorkspaceQueryMode::Auto),
            Err(error) => return mcp_tool_error(error.code, error.message),
        };
        let limit = mcp_usize_arg(arguments, "limit")
            .unwrap_or(DEFAULT_WORKSPACE_QUERY_LIMIT)
            .min(MAX_QUERY_RESULTS);
        let paths = mcp_string_arg(arguments, "path").into_iter().collect();
        match query_workspace_index_for_repo(
            &self.globals,
            &repo_root,
            &project_id,
            &query,
            mode,
            limit,
            paths,
        ) {
            Ok(response) => mcp_tool_success(
                format!("Found {} workspace index result(s).", response.result_count),
                json!({ "response": response }),
            ),
            Err(error) => mcp_tool_error(error.code, error.message),
        }
    }

    fn tool_fetch_project_memory(&self, arguments: &JsonValue) -> JsonValue {
        let project_id = mcp_string_arg(arguments, "projectId");
        let run_id = mcp_string_arg(arguments, "runId");
        if project_id.is_none() && run_id.is_none() {
            return mcp_tool_error(
                "xero_mcp_memory_scope_required",
                "Pass `projectId`, `runId`, or both to fetch Xero project memory.",
            );
        }
        let limit = mcp_usize_arg(arguments, "limit").unwrap_or(20).min(100);

        let snapshots = match (&project_id, &run_id) {
            (Some(project_id), Some(run_id)) => {
                match load_conversation_snapshot(&self.globals, Some(project_id), run_id) {
                    Ok((_store, snapshot)) => vec![snapshot],
                    Err(error) => return mcp_tool_error(error.code, error.message),
                }
            }
            (None, Some(run_id)) => match load_conversation_snapshot(&self.globals, None, run_id) {
                Ok((_store, snapshot)) => vec![snapshot],
                Err(error) => return mcp_tool_error(error.code, error.message),
            },
            (Some(project_id), None) => {
                let runs = match list_conversation_runs(&self.globals, Some(project_id)) {
                    Ok(runs) => runs,
                    Err(error) => return mcp_tool_error(error.code, error.message),
                };
                let mut snapshots = Vec::new();
                for run in runs.into_iter().take(limit) {
                    if let Ok((_store, snapshot)) = load_conversation_snapshot(
                        &self.globals,
                        Some(&run.project_id),
                        &run.run_id,
                    ) {
                        snapshots.push(snapshot);
                    }
                }
                snapshots
            }
            (None, None) => unreachable!(),
        };

        let mut memories = snapshots
            .into_iter()
            .flat_map(|snapshot| {
                snapshot.context_manifests.into_iter().map(|manifest| {
                    json!({
                        "manifestId": manifest.manifest_id,
                        "projectId": manifest.project_id,
                        "runId": manifest.run_id,
                        "providerId": manifest.provider_id,
                        "modelId": manifest.model_id,
                        "turnIndex": manifest.turn_index,
                        "contextHash": manifest.context_hash,
                        "createdAt": manifest.created_at,
                        "manifest": manifest.manifest,
                    })
                })
            })
            .collect::<Vec<_>>();
        memories.truncate(limit);
        mcp_tool_success(
            format!("Fetched {} Xero project memory record(s).", memories.len()),
            json!({ "memories": memories }),
        )
    }

    fn tool_invoke_tool_pack(&self, arguments: &JsonValue) -> JsonValue {
        let Some(tool_pack) = mcp_string_arg(arguments, "toolPack") else {
            return mcp_tool_error(
                "xero_mcp_tool_pack_required",
                "`toolPack` is required. Allowed values: conversation.stats, workspace.query.",
            );
        };
        let pack_arguments = arguments
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !pack_arguments.is_object() {
            return mcp_tool_error(
                "xero_mcp_tool_pack_arguments_invalid",
                "`arguments` must be an object.",
            );
        }

        match tool_pack.as_str() {
            "conversation.stats" => {
                let Some(run_id) = mcp_string_arg(&pack_arguments, "runId") else {
                    return mcp_tool_error(
                        "xero_mcp_run_id_required",
                        "`runId` is required for conversation.stats.",
                    );
                };
                let project_id = mcp_string_arg(&pack_arguments, "projectId");
                match load_conversation_snapshot(&self.globals, project_id.as_deref(), &run_id) {
                    Ok((_store, snapshot)) => mcp_tool_success(
                        "Computed conversation statistics.",
                        conversation_stats_json(&snapshot),
                    ),
                    Err(error) => mcp_tool_error(error.code, error.message),
                }
            }
            "workspace.query" => self.tool_query_workspace_index(&pack_arguments),
            other => mcp_tool_error(
                "xero_mcp_tool_pack_not_approved",
                format!(
                    "Tool pack `{other}` is not approved for the MCP harness. Allowed values: conversation.stats, workspace.query."
                ),
            ),
        }
    }

    fn tool_export_trace(&self, arguments: &JsonValue) -> JsonValue {
        let Some(run_id) = mcp_string_arg(arguments, "runId") else {
            return mcp_tool_error(
                "xero_mcp_run_id_required",
                "`runId` is required to export a trace.",
            );
        };
        let include_timeline = mcp_bool_arg(arguments, "includeTimeline").unwrap_or(true);
        let include_support_bundle =
            mcp_bool_arg(arguments, "includeSupportBundle").unwrap_or(false);
        let project_id = mcp_string_arg(arguments, "projectId");
        let (store, snapshot) =
            match load_conversation_snapshot(&self.globals, project_id.as_deref(), &run_id) {
                Ok(resolved) => resolved,
                Err(error) => return mcp_tool_error(error.code, error.message),
            };
        let trace = match store.export_trace(&snapshot.project_id, &snapshot.run_id) {
            Ok(trace) => trace,
            Err(error) => return mcp_tool_error(error.code, error.message),
        };
        let canonical_trace = match trace.canonical_snapshot() {
            Ok(canonical_trace) => canonical_trace,
            Err(error) => return mcp_tool_error(error.code, error.message),
        };
        let timeline = if include_timeline {
            Some(canonical_trace.timeline.clone())
        } else {
            None
        };
        let support_bundle = if include_support_bundle {
            match trace.redacted_support_bundle() {
                Ok(support_bundle) => Some(support_bundle),
                Err(error) => return mcp_tool_error(error.code, error.message),
            }
        } else {
            None
        };
        mcp_tool_success(
            "Exported Xero runtime trace.",
            json!({
                "trace": canonical_trace.trace,
                "timeline": timeline,
                "diagnostics": canonical_trace.diagnostics,
                "qualityGates": canonical_trace.quality_gates,
                "productionReadiness": canonical_trace.production_readiness,
                "canonicalTrace": canonical_trace,
                "supportBundle": support_bundle,
            }),
        )
    }
}

fn mcp_initialize_result(params: &JsonValue) -> JsonValue {
    let requested = params
        .get("protocolVersion")
        .and_then(JsonValue::as_str)
        .unwrap_or(MCP_PROTOCOL_VERSION);
    let protocol_version = if matches!(requested, "2025-06-18" | "2025-03-26") {
        requested
    } else {
        MCP_PROTOCOL_VERSION
    };
    json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": {
                "listChanged": false
            }
        },
        "serverInfo": {
            "name": MCP_SERVER_NAME,
            "title": "Xero Local Harness",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "Use Xero's MCP tools for auditable local harness actions. Dangerous subprocess and mutation capabilities are not exposed by default; external agents must be launched through the separate approval-gated CLI host path."
    })
}

fn mcp_tool_definitions() -> Vec<JsonValue> {
    vec![
        json!({
            "name": "xero_start_run",
            "title": "Start Xero Owned Run",
            "description": "Start a Xero-owned headless run through the shared runtime. Real providers require a configured headless profile and registered app-data project state; fake_provider is explicit harness-only mode. Do not use this for external CLI agents; those require the approval-gated `xero agent host` path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "User prompt for the run." },
                    "projectId": { "type": "string", "description": "Durable project id. Real-provider runs may omit this only when the current directory belongs to a registered app-data project." },
                    "sessionId": { "type": "string", "description": "Optional agent session id." },
                    "runId": { "type": "string", "description": "Optional run id." },
                    "providerId": { "type": "string", "description": "Owned-agent provider id. Use fake_provider only for explicit harness tests." },
                    "modelId": { "type": "string", "description": "Model id for the owned-agent provider." }
                },
                "required": ["prompt"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        }),
        json!({
            "name": "xero_provider_preflight",
            "title": "Preflight Xero Provider",
            "description": "Check the selected Xero provider/profile/model contract without starting an agent run. Results are persisted in headless app-data and contain only redacted request metadata.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "providerId": { "type": "string", "description": "Owned-agent provider id. Defaults to fake_provider when no profile is configured." },
                    "profileId": { "type": "string", "description": "Optional configured headless profile id." },
                    "modelId": { "type": "string", "description": "Model id for the selected provider." },
                    "force": { "type": "boolean", "description": "Recompute the preflight snapshot instead of returning the cached snapshot." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "xero_query_conversation",
            "title": "Query Xero Conversation",
            "description": "Load a durable Xero conversation by run id. Use this to inspect messages, status, and context manifests from desktop or headless runs. Set includeEvents only when event-level audit data is needed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "runId": { "type": "string", "description": "Run id to load." },
                    "projectId": { "type": "string", "description": "Optional project id when run ids are ambiguous." },
                    "includeEvents": { "type": "boolean", "description": "Include raw runtime events in the response." }
                },
                "required": ["runId"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "xero_query_workspace_index",
            "title": "Query Workspace Index",
            "description": "Search a previously-built Xero workspace index. Use `xero workspace index` first if the index is missing or stale. This tool is read-only and returns ranked file previews.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search terms to match against the workspace index." },
                    "repo": { "type": "string", "description": "Repository path. Defaults to the current directory." },
                    "projectId": { "type": "string", "description": "Optional project id. Defaults to Xero's stable id for the repo root." },
                    "mode": { "type": "string", "enum": ["auto", "semantic", "symbol", "related_tests", "impact"], "description": "Semantic query mode." },
                    "path": { "type": "string", "description": "Optional indexed path or directory filter." },
                    "limit": { "type": "integer", "description": "Maximum number of results, capped by Xero." }
                },
                "required": ["query"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "xero_fetch_project_memory",
            "title": "Fetch Xero Project Memory",
            "description": "Fetch persisted context manifests as Xero project memory. Use this when an agent needs the stored context package history for a project or run. Pass a project id, run id, or both to keep the response scoped.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectId": { "type": "string", "description": "Project id to fetch memory from." },
                    "runId": { "type": "string", "description": "Optional run id to narrow memory." },
                    "limit": { "type": "integer", "description": "Maximum memory records to return." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "xero_invoke_tool_pack",
            "title": "Invoke Approved Xero Tool Pack",
            "description": "Invoke one of Xero's approved MCP tool packs. Only read-only packs are available by default: conversation.stats and workspace.query. Do not use this for shell commands, filesystem writes, or external services.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "toolPack": { "type": "string", "enum": ["conversation.stats", "workspace.query"], "description": "Approved tool pack to invoke." },
                    "arguments": { "type": "object", "description": "Arguments for the selected tool pack." }
                },
                "required": ["toolPack"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "xero_export_trace",
            "title": "Export Xero Trace",
            "description": "Export the runtime trace for a Xero run. Use this for audit, replay, or debugging after locating the target run. The optional timeline is derived from the trace and is read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "runId": { "type": "string", "description": "Run id to export." },
                    "projectId": { "type": "string", "description": "Optional project id when run ids are ambiguous." },
                    "includeTimeline": { "type": "boolean", "description": "Include replay timeline derived from the canonical trace snapshot. Defaults to true." },
                    "includeSupportBundle": { "type": "boolean", "description": "Include the default-redacted support bundle generated from the same canonical trace snapshot. Defaults to false." }
                },
                "required": ["runId"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        }),
    ]
}

fn mcp_success(id: JsonValue, result: JsonValue) -> JsonValue {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn mcp_error(id: JsonValue, code: i64, message: &str, data: JsonValue) -> JsonValue {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
            "data": data,
        }
    })
}

fn mcp_tool_success(summary: impl Into<String>, structured_content: JsonValue) -> JsonValue {
    let summary = summary.into();
    json!({
        "content": [
            {
                "type": "text",
                "text": summary,
            }
        ],
        "structuredContent": structured_content,
        "isError": false,
    })
}

fn mcp_tool_error(code: impl Into<String>, message: impl Into<String>) -> JsonValue {
    let code = code.into();
    let message = message.into();
    json!({
        "content": [
            {
                "type": "text",
                "text": message.clone(),
            }
        ],
        "structuredContent": {
            "error": {
                "code": code,
                "message": message,
            }
        },
        "isError": true,
    })
}

fn mcp_string_arg(arguments: &JsonValue, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn mcp_bool_arg(arguments: &JsonValue, key: &str) -> Option<bool> {
    arguments.get(key).and_then(JsonValue::as_bool)
}

fn mcp_usize_arg(arguments: &JsonValue, key: &str) -> Option<usize> {
    arguments
        .get(key)
        .and_then(JsonValue::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn load_config(globals: &GlobalOptions) -> Result<CliConfig, CliError> {
    let path = globals.state_dir.join(CLI_CONFIG_FILE);
    if !path.exists() {
        return Ok(CliConfig::default());
    }
    read_json_file(&path)
}

fn save_config(globals: &GlobalOptions, config: &CliConfig) -> Result<(), CliError> {
    write_json_file(&globals.state_dir.join(CLI_CONFIG_FILE), config)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceIndexState {
    Empty,
    Indexing,
    Ready,
    Stale,
    Failed,
}

impl WorkspaceIndexState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Indexing => "indexing",
            Self::Ready => "ready",
            Self::Stale => "stale",
            Self::Failed => "failed",
        }
    }
}

impl Serialize for WorkspaceIndexState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl std::fmt::Display for WorkspaceIndexState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceQueryMode {
    Auto,
    Semantic,
    Symbol,
    RelatedTests,
    Impact,
}

impl WorkspaceQueryMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Semantic => "semantic",
            Self::Symbol => "symbol",
            Self::RelatedTests => "related_tests",
            Self::Impact => "impact",
        }
    }
}

impl Serialize for WorkspaceQueryMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceIndexDiagnostic {
    severity: String,
    code: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceIndexStatus {
    project_id: String,
    state: WorkspaceIndexState,
    index_version: u32,
    root_path: String,
    storage_path: String,
    total_files: u32,
    indexed_files: u32,
    skipped_files: u32,
    stale_files: u32,
    symbol_count: u32,
    indexed_bytes: u64,
    coverage_percent: f64,
    head_sha: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    updated_at: Option<String>,
    diagnostics: Vec<WorkspaceIndexDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceIndexResponse {
    status: WorkspaceIndexStatus,
    changed_files: u32,
    unchanged_files: u32,
    removed_files: u32,
    duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceQueryResponse {
    project_id: String,
    query: String,
    mode: WorkspaceQueryMode,
    result_count: u32,
    stale: bool,
    diagnostics: Vec<WorkspaceIndexDiagnostic>,
    results: Vec<WorkspaceQueryResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceQueryResult {
    rank: u32,
    path: String,
    score: f64,
    language: String,
    summary: String,
    snippet: String,
    symbols: Vec<String>,
    imports: Vec<String>,
    tests: Vec<String>,
    diffs: Vec<String>,
    failures: Vec<String>,
    reasons: Vec<String>,
    content_hash: String,
    indexed_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceExplainResponse {
    project_id: String,
    summary: String,
    status: WorkspaceIndexStatus,
    top_signals: Vec<String>,
    diagnostics: Vec<WorkspaceIndexDiagnostic>,
}

#[derive(Debug, Clone)]
struct WorkspaceCandidate {
    absolute_path: PathBuf,
    virtual_path: String,
    language: String,
    modified_at: String,
    byte_length: i64,
}

#[derive(Debug, Clone)]
struct WorkspaceScan {
    files: Vec<WorkspaceCandidate>,
    skipped_files: usize,
    truncated: bool,
    diagnostics: Vec<WorkspaceIndexDiagnostic>,
}

#[derive(Debug, Clone)]
struct WorkspaceFingerprint {
    modified_at: String,
    byte_length: i64,
}

#[derive(Debug, Clone)]
struct IndexedWorkspaceRow {
    project_id: String,
    path: String,
    language: String,
    content_hash: String,
    modified_at: String,
    byte_length: i64,
    summary: String,
    snippet: String,
    symbols: Vec<String>,
    imports: Vec<String>,
    tests: Vec<String>,
    routes: Vec<String>,
    commands: Vec<String>,
    diffs: Vec<String>,
    failures: Vec<String>,
    embedding_json: String,
    embedding_model: String,
    embedding_version: String,
    indexed_at: String,
}

#[derive(Debug, Clone)]
struct StoredWorkspaceRow {
    path: String,
    language: String,
    content_hash: String,
    summary: String,
    snippet: String,
    symbols_json: String,
    imports_json: String,
    tests_json: String,
    routes_json: String,
    commands_json: String,
    diffs_json: String,
    failures_json: String,
    embedding_json: String,
    indexed_at: String,
}

#[derive(Debug, Clone)]
struct FileFeatures {
    symbols: Vec<String>,
    imports: Vec<String>,
    tests: Vec<String>,
    routes: Vec<String>,
    commands: Vec<String>,
    diffs: Vec<String>,
    failures: Vec<String>,
}

#[derive(Debug, Clone)]
struct LexicalScore {
    total: f64,
    reasons: Vec<String>,
}

#[derive(Debug, Clone)]
struct ScoredWorkspaceRow {
    row: StoredWorkspaceRow,
    score: f64,
    reasons: Vec<String>,
}

fn build_workspace_index(
    globals: &GlobalOptions,
    repo_root: &Path,
    project_id: &str,
    limit: usize,
    force: bool,
) -> Result<WorkspaceIndexResponse, CliError> {
    let started = SystemTime::now();
    let started_at = now_timestamp();
    let database_path = workspace_project_database_path(globals, project_id);
    let mut connection = open_workspace_index_database(globals, repo_root, project_id)?;
    write_workspace_status(
        &connection,
        &database_path,
        repo_root,
        project_id,
        WorkspaceIndexState::Indexing,
        0,
        count_workspace_rows(&connection, project_id).unwrap_or(0),
        0,
        0,
        0,
        0,
        &[],
        Some(&started_at),
        None,
        None,
    )?;

    let scan = scan_workspace(repo_root, limit)?;
    let existing = read_workspace_fingerprints(&connection, project_id)?;
    let current_paths = scan
        .files
        .iter()
        .map(|candidate| candidate.virtual_path.clone())
        .collect::<BTreeSet<_>>();
    let diff_signals = recent_diff_signals(repo_root);
    let indexed_at = now_timestamp();
    let mut rows = Vec::new();
    let mut changed_files = 0_u32;
    let mut unchanged_files = 0_u32;

    for candidate in scan.files {
        let unchanged = !force
            && existing
                .get(&candidate.virtual_path)
                .map(|fingerprint| {
                    fingerprint.modified_at == candidate.modified_at
                        && fingerprint.byte_length == candidate.byte_length
                })
                .unwrap_or(false);
        if unchanged {
            unchanged_files = unchanged_files.saturating_add(1);
            continue;
        }
        let Some(row) =
            index_workspace_candidate(candidate, project_id, &indexed_at, &diff_signals)?
        else {
            continue;
        };
        changed_files = changed_files.saturating_add(1);
        rows.push(row);
    }

    let removed_paths = existing
        .keys()
        .filter(|path| !current_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    let tx = connection
        .transaction()
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_transaction_failed", error))?;
    for row in &rows {
        upsert_workspace_row(&tx, row)?;
    }
    for path in &removed_paths {
        tx.execute(
            "DELETE FROM workspace_index_files WHERE project_id = ?1 AND path = ?2",
            params![project_id, path],
        )
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_delete_failed", error))?;
    }
    tx.commit()
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_commit_failed", error))?;

    let indexed_files = count_workspace_rows(&connection, project_id)?;
    let (indexed_bytes, symbol_count) = workspace_index_stats(&connection, project_id)?;
    let mut diagnostics = scan.diagnostics;
    if scan.truncated {
        diagnostics.push(workspace_diagnostic(
            "warning",
            "workspace_index_file_cap_reached",
            "Workspace indexing stopped at the configured file cap. Increase max files for broader coverage.",
        ));
    }
    let state = if indexed_files == 0 {
        WorkspaceIndexState::Empty
    } else if scan.truncated {
        WorkspaceIndexState::Stale
    } else {
        WorkspaceIndexState::Ready
    };
    let completed_at = now_timestamp();
    write_workspace_status(
        &connection,
        &database_path,
        repo_root,
        project_id,
        state,
        current_paths.len() as u32,
        indexed_files,
        scan.skipped_files as u32,
        if scan.truncated { 1 } else { 0 },
        symbol_count,
        indexed_bytes,
        &diagnostics,
        Some(&started_at),
        Some(&completed_at),
        None,
    )?;
    let status = workspace_index_status(globals, repo_root, project_id)?;
    let duration_ms = started
        .elapsed()
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    Ok(WorkspaceIndexResponse {
        status,
        changed_files,
        unchanged_files,
        removed_files: removed_paths.len() as u32,
        duration_ms,
    })
}

fn workspace_index_status(
    globals: &GlobalOptions,
    repo_root: &Path,
    project_id: &str,
) -> Result<WorkspaceIndexStatus, CliError> {
    let connection = open_workspace_index_database(globals, repo_root, project_id)?;
    let database_path = workspace_project_database_path(globals, project_id);
    let mut status = read_workspace_status(&connection, repo_root, project_id, &database_path)?
        .unwrap_or_else(|| empty_workspace_status(repo_root, project_id, &database_path));
    let scan = scan_workspace(repo_root, HARD_INDEX_FILE_LIMIT)?;
    let indexed = read_workspace_fingerprints(&connection, project_id)?;
    let current_paths = scan
        .files
        .iter()
        .map(|candidate| candidate.virtual_path.clone())
        .collect::<BTreeSet<_>>();
    let stale_current = scan
        .files
        .iter()
        .filter(|candidate| {
            indexed
                .get(&candidate.virtual_path)
                .map(|fingerprint| {
                    fingerprint.modified_at != candidate.modified_at
                        || fingerprint.byte_length != candidate.byte_length
                })
                .unwrap_or(true)
        })
        .count();
    let removed = indexed
        .keys()
        .filter(|path| !current_paths.contains(*path))
        .count();
    status.total_files = scan.files.len() as u32;
    status.skipped_files = scan.skipped_files as u32;
    status.stale_files = stale_current.saturating_add(removed) as u32;
    status.coverage_percent = coverage_percent(
        status.indexed_files.saturating_sub(removed as u32),
        status.total_files,
    );
    if status.indexed_files == 0 {
        status.state = WorkspaceIndexState::Empty;
    } else if status.stale_files > 0 || status.head_sha != repository_head_sha(repo_root) {
        status.state = WorkspaceIndexState::Stale;
    } else if status.state != WorkspaceIndexState::Failed {
        status.state = WorkspaceIndexState::Ready;
    }
    if scan.truncated {
        status.diagnostics.push(workspace_diagnostic(
            "warning",
            "workspace_index_status_scan_truncated",
            "Workspace status was estimated from the first indexed-file scan window.",
        ));
    }
    Ok(status)
}

fn query_workspace_index_for_repo(
    globals: &GlobalOptions,
    repo_root: &Path,
    project_id: &str,
    query: &str,
    mode: WorkspaceQueryMode,
    limit: usize,
    paths: Vec<String>,
) -> Result<WorkspaceQueryResponse, CliError> {
    let status = workspace_index_status(globals, repo_root, project_id)?;
    let connection = open_workspace_index_database(globals, repo_root, project_id)?;
    let query_embedding = workspace_embedding(&query_embedding_text(query, mode))?;
    let query_tokens = tokenize_workspace_query(query);
    let path_filters = paths
        .iter()
        .filter_map(|path| normalize_virtual_path(path))
        .collect::<Vec<_>>();
    let mut rows = read_workspace_rows(&connection, project_id)?;
    if !path_filters.is_empty() {
        rows.retain(|row| {
            path_filters
                .iter()
                .any(|filter| path_matches_filter(&row.path, filter))
        });
    }

    let mut ranked = rows
        .into_iter()
        .filter_map(|row| {
            let embedding = serde_json::from_str::<Vec<f32>>(&row.embedding_json).ok()?;
            let semantic = cosine_similarity(&query_embedding, embedding.as_slice());
            let lexical = lexical_score(&query_tokens, &row, mode);
            let score = score_for_mode(semantic, lexical.total, mode);
            (score > 0.001).then_some(ScoredWorkspaceRow {
                row,
                score,
                reasons: lexical.reasons,
            })
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.row.path.cmp(&right.row.path))
    });
    let results = ranked
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(index, scored)| workspace_row_to_query_result(index as u32 + 1, scored))
        .collect::<Result<Vec<_>, _>>()?;
    let mut diagnostics = status.diagnostics.clone();
    if status.state == WorkspaceIndexState::Empty {
        diagnostics.push(workspace_diagnostic(
            "warning",
            "workspace_index_empty",
            "No workspace index exists yet. Run workspace index before relying on semantic results.",
        ));
    } else if status.state == WorkspaceIndexState::Stale {
        diagnostics.push(workspace_diagnostic(
            "warning",
            "workspace_index_stale",
            "Workspace index has stale or missing files. Results may omit recent changes.",
        ));
    }
    Ok(WorkspaceQueryResponse {
        project_id: project_id.to_owned(),
        query: query.to_owned(),
        mode,
        result_count: results.len() as u32,
        stale: status.state == WorkspaceIndexState::Stale,
        diagnostics,
        results,
    })
}

fn explain_workspace_index(
    globals: &GlobalOptions,
    repo_root: &Path,
    project_id: &str,
    query: Option<String>,
    path: Option<String>,
) -> Result<WorkspaceExplainResponse, CliError> {
    let status = workspace_index_status(globals, repo_root, project_id)?;
    let connection = open_workspace_index_database(globals, repo_root, project_id)?;
    let mut top_signals = Vec::new();
    let mut diagnostics = status.diagnostics.clone();
    if let Some(path) = path.as_deref().and_then(normalize_virtual_path) {
        match read_workspace_row(&connection, project_id, &path)? {
            Some(row) => {
                top_signals.push(format!("{} is indexed as {}.", row.path, row.language));
                top_signals.push(row.summary.clone());
                let symbols = decode_string_array(&row.symbols_json, "symbols")?;
                if !symbols.is_empty() {
                    top_signals.push(format!("Symbols: {}.", symbols.join(", ")));
                }
                let imports = decode_string_array(&row.imports_json, "imports")?;
                if !imports.is_empty() {
                    top_signals.push(format!(
                        "Imports: {}.",
                        imports.into_iter().take(8).collect::<Vec<_>>().join(", ")
                    ));
                }
            }
            None => diagnostics.push(workspace_diagnostic(
                "warning",
                "workspace_index_path_missing",
                "The requested path is not present in the workspace index.",
            )),
        }
    }
    if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
        let response = query_workspace_index_for_repo(
            globals,
            repo_root,
            project_id,
            &query,
            WorkspaceQueryMode::Auto,
            5,
            Vec::new(),
        )?;
        for result in response.results {
            top_signals.push(format!(
                "{} scored {:.3}: {}",
                result.path,
                result.score,
                result.reasons.join("; ")
            ));
        }
        diagnostics.extend(response.diagnostics);
    }
    if top_signals.is_empty() {
        top_signals.push(format!(
            "Index state is {}; {} of {} files are indexed.",
            status.state, status.indexed_files, status.total_files
        ));
    }
    let summary = match status.state {
        WorkspaceIndexState::Ready => "Workspace index is fresh and queryable.",
        WorkspaceIndexState::Stale => "Workspace index is queryable but has stale coverage.",
        WorkspaceIndexState::Empty => "Workspace index has not been built yet.",
        WorkspaceIndexState::Indexing => "Workspace index is currently being rebuilt.",
        WorkspaceIndexState::Failed => "Workspace index failed during the previous rebuild.",
    }
    .to_string();
    Ok(WorkspaceExplainResponse {
        project_id: project_id.to_owned(),
        summary,
        status,
        top_signals,
        diagnostics,
    })
}

fn reset_workspace_index(
    globals: &GlobalOptions,
    repo_root: &Path,
    project_id: &str,
) -> Result<WorkspaceIndexStatus, CliError> {
    let connection = open_workspace_index_database(globals, repo_root, project_id)?;
    connection
        .execute(
            "DELETE FROM workspace_index_files WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_reset_failed", error))?;
    connection
        .execute(
            "DELETE FROM workspace_index_metadata WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_reset_failed", error))?;
    Ok(empty_workspace_status(
        repo_root,
        project_id,
        &workspace_project_database_path(globals, project_id),
    ))
}

fn scan_workspace(repo_root: &Path, limit: usize) -> Result<WorkspaceScan, CliError> {
    let mut files = Vec::new();
    let mut skipped_files = 0usize;
    let mut truncated = false;
    scan_workspace_dir(
        repo_root,
        repo_root,
        limit,
        &mut files,
        &mut skipped_files,
        &mut truncated,
    )?;
    files.sort_by(|left, right| left.virtual_path.cmp(&right.virtual_path));
    let mut diagnostics = Vec::new();
    if skipped_files > 0 {
        diagnostics.push(workspace_diagnostic(
            "info",
            "workspace_index_skipped_files",
            format!("Skipped {skipped_files} non-source, oversized, ignored, or unreadable files."),
        ));
    }
    Ok(WorkspaceScan {
        files,
        skipped_files,
        truncated,
        diagnostics,
    })
}

fn scan_workspace_dir(
    repo_root: &Path,
    current: &Path,
    limit: usize,
    files: &mut Vec<WorkspaceCandidate>,
    skipped_files: &mut usize,
    truncated: &mut bool,
) -> Result<(), CliError> {
    if files.len() >= limit {
        *truncated = true;
        return Ok(());
    }
    let entries = fs::read_dir(current).map_err(|error| {
        CliError::system_fault(
            "xero_cli_workspace_scan_failed",
            format!(
                "Could not read workspace directory `{}`: {error}",
                current.display()
            ),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            CliError::system_fault(
                "xero_cli_workspace_scan_failed",
                format!(
                    "Could not read workspace entry in `{}`: {error}",
                    current.display()
                ),
            )
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            CliError::system_fault(
                "xero_cli_workspace_scan_failed",
                format!(
                    "Could not inspect workspace entry `{}`: {error}",
                    path.display()
                ),
            )
        })?;
        if file_type.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            scan_workspace_dir(repo_root, &path, limit, files, skipped_files, truncated)?;
            if *truncated {
                return Ok(());
            }
        } else if file_type.is_file() {
            if files.len() >= limit {
                *truncated = true;
                return Ok(());
            }
            match workspace_candidate(repo_root, &path) {
                Ok(Some(candidate)) => files.push(candidate),
                Ok(None) | Err(_) => *skipped_files = skipped_files.saturating_add(1),
            }
        }
    }
    Ok(())
}

fn workspace_candidate(
    repo_root: &Path,
    path: &Path,
) -> Result<Option<WorkspaceCandidate>, CliError> {
    let metadata = fs::metadata(path).map_err(|error| {
        CliError::system_fault(
            "xero_cli_workspace_file_metadata_failed",
            format!("Could not inspect `{}`: {error}", path.display()),
        )
    })?;
    let Some(language) = workspace_language(path) else {
        return Ok(None);
    };
    if metadata.len() > MAX_INDEX_FILE_BYTES {
        return Ok(None);
    }
    let Some(virtual_path) = to_virtual_path(repo_root, path) else {
        return Ok(None);
    };
    Ok(Some(WorkspaceCandidate {
        absolute_path: path.to_path_buf(),
        virtual_path,
        language,
        byte_length: metadata.len() as i64,
        modified_at: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| format!("{}Z", duration.as_secs()))
            .unwrap_or_else(now_timestamp),
    }))
}

fn should_skip_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            matches!(
                name,
                ".git"
                    | ".xero"
                    | "target"
                    | "node_modules"
                    | ".next"
                    | ".turbo"
                    | ".cache"
                    | "dist"
                    | "build"
                    | "coverage"
            )
        })
        .unwrap_or(false)
}

fn index_workspace_candidate(
    candidate: WorkspaceCandidate,
    project_id: &str,
    indexed_at: &str,
    diff_signals: &BTreeMap<String, Vec<String>>,
) -> Result<Option<IndexedWorkspaceRow>, CliError> {
    let Ok(content) = fs::read_to_string(&candidate.absolute_path) else {
        return Ok(None);
    };
    let mut features = extract_workspace_features(&candidate, &content);
    features.diffs = diff_signals
        .get(&candidate.virtual_path)
        .cloned()
        .unwrap_or_default();
    let summary = summarize_workspace_file(&candidate, &features);
    let snippet = content
        .chars()
        .take(MAX_INDEX_SNIPPET_CHARS)
        .collect::<String>();
    let content_hash = sha256_text(&content);
    let embedding_text = [
        candidate.virtual_path.as_str(),
        candidate.language.as_str(),
        summary.as_str(),
        &features.symbols.join(" "),
        &features.imports.join(" "),
        &features.tests.join(" "),
        &features.routes.join(" "),
        &features.commands.join(" "),
        &features.diffs.join(" "),
        &snippet,
    ]
    .join("\n");
    let embedding_json =
        serde_json::to_string(&workspace_embedding(&embedding_text)?).map_err(|error| {
            CliError::system_fault(
                "xero_cli_workspace_embedding_encode_failed",
                format!("Could not serialize workspace embedding: {error}"),
            )
        })?;
    Ok(Some(IndexedWorkspaceRow {
        project_id: project_id.to_owned(),
        path: candidate.virtual_path,
        language: candidate.language,
        content_hash,
        modified_at: candidate.modified_at,
        byte_length: candidate.byte_length,
        summary,
        snippet,
        symbols: features.symbols,
        imports: features.imports,
        tests: features.tests,
        routes: features.routes,
        commands: features.commands,
        diffs: features.diffs,
        failures: features.failures,
        embedding_json,
        embedding_model: WORKSPACE_EMBEDDING_MODEL.into(),
        embedding_version: WORKSPACE_EMBEDDING_VERSION.into(),
        indexed_at: indexed_at.to_owned(),
    }))
}

fn extract_workspace_features(candidate: &WorkspaceCandidate, content: &str) -> FileFeatures {
    let mut features = FileFeatures {
        symbols: Vec::new(),
        imports: Vec::new(),
        tests: Vec::new(),
        routes: Vec::new(),
        commands: Vec::new(),
        diffs: Vec::new(),
        failures: Vec::new(),
    };
    let mut previous_tauri_command = false;
    if candidate.virtual_path.contains("/routes/")
        || candidate.virtual_path.contains("/app/")
        || candidate.virtual_path.contains("/pages/")
        || candidate.virtual_path.contains("/components/")
    {
        features.routes.push(candidate.virtual_path.clone());
    }
    if is_test_path(&candidate.virtual_path) {
        features
            .tests
            .push(format!("test file {}", candidate.virtual_path));
    }
    for (line_index, raw_line) in content.lines().enumerate() {
        if line_index > 2_000
            && features.symbols.len() >= MAX_WORKSPACE_SYMBOLS
            && features.imports.len() >= MAX_WORKSPACE_IMPORTS
            && features.tests.len() >= MAX_WORKSPACE_TESTS
        {
            break;
        }
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if features.imports.len() < MAX_WORKSPACE_IMPORTS {
            if let Some(import) = import_from_workspace_line(line) {
                push_unique(&mut features.imports, import, MAX_WORKSPACE_IMPORTS);
            }
        }
        if features.tests.len() < MAX_WORKSPACE_TESTS {
            if let Some(test) = test_from_workspace_line(line, line_index + 1) {
                push_unique(&mut features.tests, test, MAX_WORKSPACE_TESTS);
            }
        }
        if line.contains("#[tauri::command]") {
            previous_tauri_command = true;
            push_unique(
                &mut features.commands,
                format!("tauri command marker at line {}", line_index + 1),
                MAX_WORKSPACE_FEATURES,
            );
            continue;
        }
        if features.symbols.len() < MAX_WORKSPACE_SYMBOLS {
            if let Some((kind, name)) = symbol_from_workspace_line(line) {
                if previous_tauri_command {
                    push_unique(
                        &mut features.commands,
                        format!("{name} at line {}", line_index + 1),
                        MAX_WORKSPACE_FEATURES,
                    );
                }
                previous_tauri_command = false;
                push_unique(
                    &mut features.symbols,
                    format!("{kind} {name}:{}", line_index + 1),
                    MAX_WORKSPACE_SYMBOLS,
                );
            }
        }
    }
    features
}

fn import_from_workspace_line(line: &str) -> Option<String> {
    let keep = line
        .strip_prefix("import ")
        .or_else(|| line.strip_prefix("export "))
        .or_else(|| line.strip_prefix("use "))
        .or_else(|| line.strip_prefix("mod "))
        .or_else(|| line.strip_prefix("from "))?;
    let trimmed = keep.trim().trim_end_matches(';');
    (!trimmed.is_empty()).then(|| trimmed.chars().take(160).collect())
}

fn test_from_workspace_line(line: &str, line_number: usize) -> Option<String> {
    if line.starts_with("#[test]")
        || line.starts_with("#[tokio::test]")
        || line.starts_with("describe(")
        || line.starts_with("it(")
        || line.starts_with("test(")
        || line.contains("vitest")
    {
        Some(format!("test signal at line {line_number}"))
    } else {
        None
    }
}

fn symbol_from_workspace_line(line: &str) -> Option<(&'static str, String)> {
    let normalized = line
        .strip_prefix("pub ")
        .or_else(|| line.strip_prefix("export default "))
        .or_else(|| line.strip_prefix("export "))
        .or_else(|| line.strip_prefix("async "))
        .unwrap_or(line);
    for (prefix, kind) in [
        ("async fn ", "function"),
        ("fn ", "function"),
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("impl ", "impl"),
        ("function ", "function"),
        ("class ", "class"),
        ("interface ", "interface"),
        ("type ", "type"),
        ("const ", "constant"),
        ("let ", "binding"),
        ("def ", "function"),
    ] {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            let name = rest
                .split(|character: char| {
                    character.is_whitespace()
                        || matches!(character, '(' | '<' | ':' | '=' | '{' | ';' | ',')
                })
                .next()
                .unwrap_or_default()
                .trim()
                .trim_matches(|character: char| !character.is_alphanumeric() && character != '_')
                .to_string();
            if !name.is_empty() {
                return Some((kind, name));
            }
        }
    }
    None
}

fn summarize_workspace_file(candidate: &WorkspaceCandidate, features: &FileFeatures) -> String {
    let mut parts = vec![format!(
        "{} source at {}",
        candidate.language, candidate.virtual_path
    )];
    if !features.symbols.is_empty() {
        parts.push(format!(
            "defines {}",
            features
                .symbols
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !features.imports.is_empty() {
        parts.push(format!(
            "imports {}",
            features
                .imports
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !features.tests.is_empty() {
        parts.push("contains test signals".into());
    }
    if !features.routes.is_empty() {
        parts.push("looks route/component-facing".into());
    }
    if !features.commands.is_empty() {
        parts.push("exposes Tauri command signals".into());
    }
    if !features.diffs.is_empty() {
        parts.push("has recent working-tree diff signals".into());
    }
    parts.join("; ")
}

fn open_workspace_index_database(
    globals: &GlobalOptions,
    repo_root: &Path,
    project_id: &str,
) -> Result<Connection, CliError> {
    let database_path = workspace_project_database_path(globals, project_id);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::system_fault(
                "xero_cli_workspace_index_dir_failed",
                format!(
                    "Could not prepare workspace index directory `{}`: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    let connection = Connection::open(&database_path).map_err(|error| {
        CliError::system_fault(
            "xero_cli_workspace_index_open_failed",
            format!(
                "Could not open workspace index database `{}`: {error}",
                database_path.display()
            ),
        )
    })?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_config_failed", error))?;
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                milestone TEXT NOT NULL DEFAULT '',
                total_phases INTEGER NOT NULL DEFAULT 0 CHECK (total_phases >= 0),
                completed_phases INTEGER NOT NULL DEFAULT 0 CHECK (completed_phases >= 0),
                active_phase INTEGER NOT NULL DEFAULT 0 CHECK (active_phase >= 0),
                branch TEXT,
                runtime TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
            CREATE TABLE IF NOT EXISTS repositories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                root_path TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                branch TEXT,
                head_sha TEXT,
                is_git_repo INTEGER NOT NULL DEFAULT 1 CHECK (is_git_repo IN (0, 1)),
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
            CREATE TABLE IF NOT EXISTS workspace_index_metadata (
                project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
                status TEXT NOT NULL CHECK (status IN ('empty', 'indexing', 'ready', 'stale', 'failed')),
                index_version INTEGER NOT NULL CHECK (index_version > 0),
                root_path TEXT NOT NULL CHECK (root_path <> ''),
                storage_path TEXT NOT NULL CHECK (storage_path <> ''),
                head_sha TEXT,
                worktree_fingerprint TEXT,
                total_files INTEGER NOT NULL DEFAULT 0 CHECK (total_files >= 0),
                indexed_files INTEGER NOT NULL DEFAULT 0 CHECK (indexed_files >= 0),
                skipped_files INTEGER NOT NULL DEFAULT 0 CHECK (skipped_files >= 0),
                stale_files INTEGER NOT NULL DEFAULT 0 CHECK (stale_files >= 0),
                symbol_count INTEGER NOT NULL DEFAULT 0 CHECK (symbol_count >= 0),
                indexed_bytes INTEGER NOT NULL DEFAULT 0 CHECK (indexed_bytes >= 0),
                coverage_percent REAL NOT NULL DEFAULT 0 CHECK (coverage_percent >= 0 AND coverage_percent <= 100),
                diagnostics_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(diagnostics_json)),
                last_error_code TEXT,
                last_error_message TEXT,
                started_at TEXT,
                completed_at TEXT,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS workspace_index_files (
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                path TEXT NOT NULL CHECK (path <> ''),
                language TEXT NOT NULL CHECK (language <> ''),
                content_hash TEXT NOT NULL CHECK (content_hash <> ''),
                modified_at TEXT NOT NULL CHECK (modified_at <> ''),
                byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
                summary TEXT NOT NULL,
                snippet TEXT NOT NULL,
                symbols_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(symbols_json)),
                imports_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(imports_json)),
                tests_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(tests_json)),
                routes_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(routes_json)),
                commands_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(commands_json)),
                diffs_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(diffs_json)),
                failures_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(failures_json)),
                embedding_json TEXT NOT NULL CHECK (embedding_json <> '' AND json_valid(embedding_json)),
                embedding_model TEXT NOT NULL CHECK (embedding_model <> ''),
                embedding_version TEXT NOT NULL CHECK (embedding_version <> ''),
                indexed_at TEXT NOT NULL,
                PRIMARY KEY (project_id, path)
            );
            CREATE INDEX IF NOT EXISTS idx_workspace_index_files_project_language
                ON workspace_index_files(project_id, language, path);
            CREATE INDEX IF NOT EXISTS idx_workspace_index_files_project_hash
                ON workspace_index_files(project_id, content_hash);
            ",
        )
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_schema_failed", error))?;
    let display_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(project_id);
    connection
        .execute(
            "INSERT INTO projects (id, name, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at",
            params![project_id, display_name, now_timestamp()],
        )
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_project_write_failed", error))?;
    connection
        .execute(
            "INSERT INTO repositories (id, project_id, root_path, display_name, head_sha, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(root_path) DO UPDATE SET
                project_id = excluded.project_id,
                display_name = excluded.display_name,
                head_sha = excluded.head_sha,
                updated_at = excluded.updated_at",
            params![
                format!("repo-{}", project_id),
                project_id,
                repo_root.display().to_string(),
                display_name,
                repository_head_sha(repo_root),
                now_timestamp(),
            ],
        )
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_repository_write_failed", error))?;
    Ok(connection)
}

fn upsert_workspace_row(
    connection: &Connection,
    row: &IndexedWorkspaceRow,
) -> Result<(), CliError> {
    connection
        .execute(
            "INSERT INTO workspace_index_files (
                project_id, path, language, content_hash, modified_at, byte_length,
                summary, snippet, symbols_json, imports_json, tests_json, routes_json,
                commands_json, diffs_json, failures_json, embedding_json, embedding_model,
                embedding_version, indexed_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            ON CONFLICT(project_id, path) DO UPDATE SET
                language = excluded.language,
                content_hash = excluded.content_hash,
                modified_at = excluded.modified_at,
                byte_length = excluded.byte_length,
                summary = excluded.summary,
                snippet = excluded.snippet,
                symbols_json = excluded.symbols_json,
                imports_json = excluded.imports_json,
                tests_json = excluded.tests_json,
                routes_json = excluded.routes_json,
                commands_json = excluded.commands_json,
                diffs_json = excluded.diffs_json,
                failures_json = excluded.failures_json,
                embedding_json = excluded.embedding_json,
                embedding_model = excluded.embedding_model,
                embedding_version = excluded.embedding_version,
                indexed_at = excluded.indexed_at",
            params![
                &row.project_id,
                &row.path,
                &row.language,
                &row.content_hash,
                &row.modified_at,
                row.byte_length,
                &row.summary,
                &row.snippet,
                json_array(&row.symbols)?,
                json_array(&row.imports)?,
                json_array(&row.tests)?,
                json_array(&row.routes)?,
                json_array(&row.commands)?,
                json_array(&row.diffs)?,
                json_array(&row.failures)?,
                &row.embedding_json,
                &row.embedding_model,
                &row.embedding_version,
                &row.indexed_at,
            ],
        )
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_file_write_failed", error))?;
    Ok(())
}

fn read_workspace_fingerprints(
    connection: &Connection,
    project_id: &str,
) -> Result<BTreeMap<String, WorkspaceFingerprint>, CliError> {
    let mut stmt = connection
        .prepare("SELECT path, modified_at, byte_length FROM workspace_index_files WHERE project_id = ?1")
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_read_failed", error))?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                WorkspaceFingerprint {
                    modified_at: row.get(1)?,
                    byte_length: row.get(2)?,
                },
            ))
        })
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_read_failed", error))?;
    let mut fingerprints = BTreeMap::new();
    for row in rows {
        let (path, fingerprint) =
            row.map_err(|error| sqlite_cli_error("xero_cli_workspace_index_read_failed", error))?;
        fingerprints.insert(path, fingerprint);
    }
    Ok(fingerprints)
}

fn read_workspace_rows(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<StoredWorkspaceRow>, CliError> {
    let mut stmt = connection
        .prepare(
            "SELECT path, language, content_hash, summary, snippet, symbols_json, imports_json,
                    tests_json, routes_json, commands_json, diffs_json, failures_json,
                    embedding_json, indexed_at
             FROM workspace_index_files WHERE project_id = ?1",
        )
        .map_err(|error| {
            sqlite_cli_error("xero_cli_workspace_index_query_prepare_failed", error)
        })?;
    let rows = stmt
        .query_map(params![project_id], stored_workspace_row_from_sql)
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_query_failed", error))?;
    let mut output = Vec::new();
    for row in rows {
        output.push(
            row.map_err(|error| sqlite_cli_error("xero_cli_workspace_index_query_failed", error))?,
        );
    }
    Ok(output)
}

fn read_workspace_row(
    connection: &Connection,
    project_id: &str,
    path: &str,
) -> Result<Option<StoredWorkspaceRow>, CliError> {
    connection
        .query_row(
            "SELECT path, language, content_hash, summary, snippet, symbols_json, imports_json,
                    tests_json, routes_json, commands_json, diffs_json, failures_json,
                    embedding_json, indexed_at
             FROM workspace_index_files WHERE project_id = ?1 AND path = ?2",
            params![project_id, path],
            stored_workspace_row_from_sql,
        )
        .optional()
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_query_failed", error))
}

fn stored_workspace_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredWorkspaceRow> {
    Ok(StoredWorkspaceRow {
        path: row.get(0)?,
        language: row.get(1)?,
        content_hash: row.get(2)?,
        summary: row.get(3)?,
        snippet: row.get(4)?,
        symbols_json: row.get(5)?,
        imports_json: row.get(6)?,
        tests_json: row.get(7)?,
        routes_json: row.get(8)?,
        commands_json: row.get(9)?,
        diffs_json: row.get(10)?,
        failures_json: row.get(11)?,
        embedding_json: row.get(12)?,
        indexed_at: row.get(13)?,
    })
}

fn count_workspace_rows(connection: &Connection, project_id: &str) -> Result<u32, CliError> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM workspace_index_files WHERE project_id = ?1",
            params![project_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count.max(0) as u32)
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_count_failed", error))
}

fn workspace_index_stats(
    connection: &Connection,
    project_id: &str,
) -> Result<(u64, u32), CliError> {
    let mut stmt = connection
        .prepare(
            "SELECT byte_length, symbols_json FROM workspace_index_files WHERE project_id = ?1",
        )
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_stats_failed", error))?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_stats_failed", error))?;
    let mut bytes = 0_u64;
    let mut symbols = 0_u32;
    for row in rows {
        let (byte_length, symbols_json) =
            row.map_err(|error| sqlite_cli_error("xero_cli_workspace_index_stats_failed", error))?;
        bytes = bytes.saturating_add(byte_length.max(0) as u64);
        symbols =
            symbols.saturating_add(decode_string_array(&symbols_json, "symbols")?.len() as u32);
    }
    Ok((bytes, symbols))
}

#[allow(clippy::too_many_arguments)]
fn write_workspace_status(
    connection: &Connection,
    database_path: &Path,
    repo_root: &Path,
    project_id: &str,
    state: WorkspaceIndexState,
    total_files: u32,
    indexed_files: u32,
    skipped_files: u32,
    stale_files: u32,
    symbol_count: u32,
    indexed_bytes: u64,
    diagnostics: &[WorkspaceIndexDiagnostic],
    started_at: Option<&str>,
    completed_at: Option<&str>,
    error: Option<(&str, &str)>,
) -> Result<(), CliError> {
    let diagnostics_json = serde_json::to_string(diagnostics).map_err(|error| {
        CliError::system_fault(
            "xero_cli_workspace_index_diagnostics_encode_failed",
            format!("Could not serialize workspace-index diagnostics: {error}"),
        )
    })?;
    let updated_at = now_timestamp();
    let root_path = repo_root.display().to_string();
    let storage_path = database_path
        .parent()
        .unwrap_or(database_path)
        .display()
        .to_string();
    let head_sha = repository_head_sha(repo_root);
    let fingerprint = workspace_fingerprint(repo_root);
    let (last_error_code, last_error_message) = error
        .map(|(code, message)| (Some(code.to_owned()), Some(message.to_owned())))
        .unwrap_or((None, None));
    connection
        .execute(
            "INSERT INTO workspace_index_metadata (
                project_id, status, index_version, root_path, storage_path, head_sha,
                worktree_fingerprint, total_files, indexed_files, skipped_files, stale_files,
                symbol_count, indexed_bytes, coverage_percent, diagnostics_json,
                last_error_code, last_error_message, started_at, completed_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
            ON CONFLICT(project_id) DO UPDATE SET
                status = excluded.status,
                index_version = excluded.index_version,
                root_path = excluded.root_path,
                storage_path = excluded.storage_path,
                head_sha = excluded.head_sha,
                worktree_fingerprint = excluded.worktree_fingerprint,
                total_files = excluded.total_files,
                indexed_files = excluded.indexed_files,
                skipped_files = excluded.skipped_files,
                stale_files = excluded.stale_files,
                symbol_count = excluded.symbol_count,
                indexed_bytes = excluded.indexed_bytes,
                coverage_percent = excluded.coverage_percent,
                diagnostics_json = excluded.diagnostics_json,
                last_error_code = excluded.last_error_code,
                last_error_message = excluded.last_error_message,
                started_at = excluded.started_at,
                completed_at = excluded.completed_at,
                updated_at = excluded.updated_at",
            params![
                project_id,
                state.as_str(),
                WORKSPACE_INDEX_VERSION,
                root_path,
                storage_path,
                head_sha,
                fingerprint,
                total_files,
                indexed_files,
                skipped_files,
                stale_files,
                symbol_count,
                indexed_bytes as i64,
                coverage_percent(indexed_files, total_files),
                diagnostics_json,
                last_error_code,
                last_error_message,
                started_at,
                completed_at,
                updated_at,
            ],
        )
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_status_write_failed", error))?;
    Ok(())
}

fn read_workspace_status(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    database_path: &Path,
) -> Result<Option<WorkspaceIndexStatus>, CliError> {
    connection
        .query_row(
            "SELECT status, index_version, root_path, storage_path, head_sha, total_files,
                    indexed_files, skipped_files, stale_files, symbol_count, indexed_bytes,
                    coverage_percent, diagnostics_json, last_error_code, last_error_message,
                    started_at, completed_at, updated_at
             FROM workspace_index_metadata WHERE project_id = ?1",
            params![project_id],
            |row| {
                let diagnostics_json: String = row.get(12)?;
                let mut diagnostics =
                    serde_json::from_str::<Vec<WorkspaceIndexDiagnostic>>(&diagnostics_json)
                        .unwrap_or_default();
                let last_error_code: Option<String> = row.get(13)?;
                let last_error_message: Option<String> = row.get(14)?;
                if let (Some(code), Some(message)) = (last_error_code, last_error_message) {
                    diagnostics.push(WorkspaceIndexDiagnostic {
                        severity: "error".into(),
                        code,
                        message,
                    });
                }
                Ok(WorkspaceIndexStatus {
                    project_id: project_id.to_owned(),
                    state: parse_workspace_state(row.get::<_, String>(0)?.as_str()),
                    index_version: row.get::<_, i64>(1)?.max(1) as u32,
                    root_path: row.get(2)?,
                    storage_path: row.get(3)?,
                    head_sha: row.get(4)?,
                    total_files: row.get::<_, i64>(5)?.max(0) as u32,
                    indexed_files: row.get::<_, i64>(6)?.max(0) as u32,
                    skipped_files: row.get::<_, i64>(7)?.max(0) as u32,
                    stale_files: row.get::<_, i64>(8)?.max(0) as u32,
                    symbol_count: row.get::<_, i64>(9)?.max(0) as u32,
                    indexed_bytes: row.get::<_, i64>(10)?.max(0) as u64,
                    coverage_percent: row.get(11)?,
                    diagnostics,
                    started_at: row.get(15)?,
                    completed_at: row.get(16)?,
                    updated_at: row.get(17)?,
                })
            },
        )
        .optional()
        .map(|status| {
            status.or_else(|| Some(empty_workspace_status(repo_root, project_id, database_path)))
        })
        .map_err(|error| sqlite_cli_error("xero_cli_workspace_index_status_read_failed", error))
}

fn empty_workspace_status(
    repo_root: &Path,
    project_id: &str,
    database_path: &Path,
) -> WorkspaceIndexStatus {
    WorkspaceIndexStatus {
        project_id: project_id.to_owned(),
        state: WorkspaceIndexState::Empty,
        index_version: WORKSPACE_INDEX_VERSION,
        root_path: repo_root.display().to_string(),
        storage_path: database_path
            .parent()
            .unwrap_or(database_path)
            .display()
            .to_string(),
        total_files: 0,
        indexed_files: 0,
        skipped_files: 0,
        stale_files: 0,
        symbol_count: 0,
        indexed_bytes: 0,
        coverage_percent: 0.0,
        head_sha: repository_head_sha(repo_root),
        started_at: None,
        completed_at: None,
        updated_at: None,
        diagnostics: Vec::new(),
    }
}

fn workspace_row_to_query_result(
    rank: u32,
    scored: ScoredWorkspaceRow,
) -> Result<WorkspaceQueryResult, CliError> {
    Ok(WorkspaceQueryResult {
        rank,
        path: scored.row.path,
        score: round_score(scored.score),
        language: scored.row.language,
        summary: scored.row.summary,
        snippet: scored.row.snippet,
        symbols: decode_string_array(&scored.row.symbols_json, "symbols")?,
        imports: decode_string_array(&scored.row.imports_json, "imports")?,
        tests: decode_string_array(&scored.row.tests_json, "tests")?,
        diffs: decode_string_array(&scored.row.diffs_json, "diffs")?,
        failures: decode_string_array(&scored.row.failures_json, "failures")?,
        reasons: scored.reasons,
        content_hash: scored.row.content_hash,
        indexed_at: scored.row.indexed_at,
    })
}

fn lexical_score(
    tokens: &[String],
    row: &StoredWorkspaceRow,
    mode: WorkspaceQueryMode,
) -> LexicalScore {
    if tokens.is_empty() {
        return LexicalScore {
            total: 0.0,
            reasons: vec!["empty query token set".into()],
        };
    }
    let symbols = decode_string_array(&row.symbols_json, "symbols").unwrap_or_default();
    let imports = decode_string_array(&row.imports_json, "imports").unwrap_or_default();
    let tests = decode_string_array(&row.tests_json, "tests").unwrap_or_default();
    let routes = decode_string_array(&row.routes_json, "routes").unwrap_or_default();
    let commands = decode_string_array(&row.commands_json, "commands").unwrap_or_default();
    let diffs = decode_string_array(&row.diffs_json, "diffs").unwrap_or_default();
    let failures = decode_string_array(&row.failures_json, "failures").unwrap_or_default();
    let path_l = row.path.to_lowercase();
    let summary_l = row.summary.to_lowercase();
    let symbols_l = symbols.join(" ").to_lowercase();
    let imports_l = imports.join(" ").to_lowercase();
    let tests_l = tests.join(" ").to_lowercase();
    let feature_l = [
        routes.join(" "),
        commands.join(" "),
        diffs.join(" "),
        failures.join(" "),
    ]
    .join(" ")
    .to_lowercase();
    let mut score = 0.0_f64;
    let mut reasons = Vec::new();
    for token in tokens {
        if path_l.contains(token) {
            score += 0.24;
            push_reason(&mut reasons, format!("path matches `{token}`"));
        }
        if symbols_l.contains(token) {
            score += 0.22;
            push_reason(&mut reasons, format!("symbol matches `{token}`"));
        }
        if summary_l.contains(token) {
            score += 0.12;
            push_reason(&mut reasons, format!("summary matches `{token}`"));
        }
        if imports_l.contains(token) {
            score += 0.1;
            push_reason(&mut reasons, format!("import/dependency matches `{token}`"));
        }
        if tests_l.contains(token) {
            score += 0.12;
            push_reason(&mut reasons, format!("test signal matches `{token}`"));
        }
        if feature_l.contains(token) {
            score += 0.1;
            push_reason(
                &mut reasons,
                format!("route/command/diff/failure signal matches `{token}`"),
            );
        }
    }
    match mode {
        WorkspaceQueryMode::Symbol if !symbols.is_empty() => {
            score += 0.15;
            push_reason(&mut reasons, "symbol-aware lookup boost".into());
        }
        WorkspaceQueryMode::RelatedTests if !tests.is_empty() || is_test_path(&row.path) => {
            score += 0.25;
            push_reason(&mut reasons, "related test discovery boost".into());
        }
        WorkspaceQueryMode::Impact => {
            if !imports.is_empty() {
                score += 0.12;
                push_reason(&mut reasons, "change-impact import graph signal".into());
            }
            if !diffs.is_empty() {
                score += 0.18;
                push_reason(&mut reasons, "recent diff impact signal".into());
            }
        }
        _ => {}
    }
    if !failures.is_empty() {
        score += 0.1;
        push_reason(&mut reasons, "recent build/test failure signal".into());
    }
    if reasons.is_empty() {
        reasons.push("semantic embedding similarity".into());
    }
    LexicalScore {
        total: score.min(1.0),
        reasons,
    }
}

fn score_for_mode(semantic: f64, lexical: f64, mode: WorkspaceQueryMode) -> f64 {
    let (semantic_weight, lexical_weight) = match mode {
        WorkspaceQueryMode::Semantic => (0.82, 0.18),
        WorkspaceQueryMode::Symbol => (0.4, 0.6),
        WorkspaceQueryMode::RelatedTests | WorkspaceQueryMode::Impact => (0.48, 0.52),
        WorkspaceQueryMode::Auto => (0.62, 0.38),
    };
    (semantic * semantic_weight + lexical * lexical_weight).min(1.0)
}

fn query_embedding_text(query: &str, mode: WorkspaceQueryMode) -> String {
    match mode {
        WorkspaceQueryMode::RelatedTests => format!("tests specs verification related to {query}"),
        WorkspaceQueryMode::Impact => format!("change impact imports dependents {query}"),
        WorkspaceQueryMode::Symbol => format!("symbol definition lookup {query}"),
        WorkspaceQueryMode::Semantic | WorkspaceQueryMode::Auto => query.to_owned(),
    }
}

fn parse_workspace_query_mode(value: &str) -> Result<WorkspaceQueryMode, CliError> {
    match value.trim().replace('-', "_").as_str() {
        "auto" => Ok(WorkspaceQueryMode::Auto),
        "semantic" => Ok(WorkspaceQueryMode::Semantic),
        "symbol" | "symbols" => Ok(WorkspaceQueryMode::Symbol),
        "related_tests" | "tests" => Ok(WorkspaceQueryMode::RelatedTests),
        "impact" | "change_impact" => Ok(WorkspaceQueryMode::Impact),
        other => Err(CliError::usage(format!(
            "Unknown workspace query mode `{other}`. Use auto, semantic, symbol, related-tests, or impact."
        ))),
    }
}

fn parse_workspace_state(value: &str) -> WorkspaceIndexState {
    match value {
        "indexing" => WorkspaceIndexState::Indexing,
        "ready" => WorkspaceIndexState::Ready,
        "stale" => WorkspaceIndexState::Stale,
        "failed" => WorkspaceIndexState::Failed,
        _ => WorkspaceIndexState::Empty,
    }
}

fn semantic_index_state_from_workspace_status(
    state: WorkspaceIndexState,
) -> EnvironmentSemanticIndexState {
    match state {
        WorkspaceIndexState::Ready => EnvironmentSemanticIndexState::Ready,
        WorkspaceIndexState::Indexing => EnvironmentSemanticIndexState::Indexing,
        WorkspaceIndexState::Stale => EnvironmentSemanticIndexState::Stale,
        WorkspaceIndexState::Empty => EnvironmentSemanticIndexState::Empty,
        WorkspaceIndexState::Failed => EnvironmentSemanticIndexState::Failed,
    }
}

fn workspace_language(path: &Path) -> Option<String> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if matches!(
        file_name,
        "Cargo.toml"
            | "package.json"
            | "tsconfig.json"
            | "vite.config.ts"
            | "README.md"
            | "AGENTS.md"
    ) {
        return Some(file_name.to_owned());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())?
        .to_lowercase();
    let language = match extension.as_str() {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "typescript-react",
        "js" => "javascript",
        "jsx" => "javascript-react",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "ex" | "exs" => "elixir",
        "svelte" => "svelte",
        "vue" => "vue",
        "md" | "mdx" => "markdown",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "graphql" | "gql" => "graphql",
        "sql" => "sql",
        "sh" | "bash" | "zsh" => "shell",
        _ => return None,
    };
    Some(language.into())
}

fn is_test_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.contains("/tests/")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_test.py")
}

fn to_virtual_path(repo_root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(repo_root).ok()?;
    let parts = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| format!("/{}", parts.join("/")))
}

fn normalize_virtual_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let prefixed = if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    };
    Some(prefixed.trim_end_matches('/').to_owned())
}

fn path_matches_filter(path: &str, filter: &str) -> bool {
    path == filter
        || path
            .strip_prefix(filter)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn tokenize_workspace_query(query: &str) -> Vec<String> {
    query
        .split(|character: char| {
            !character.is_alphanumeric() && character != '_' && character != '-'
        })
        .map(|token| token.trim().to_lowercase())
        .filter(|token| token.len() >= 2)
        .take(24)
        .collect()
}

fn workspace_embedding(text: &str) -> Result<Vec<f32>, CliError> {
    let mut vector = vec![0.0_f32; WORKSPACE_EMBEDDING_DIM];
    let tokens = embedding_tokens(text);
    if tokens.is_empty() {
        return Ok(vector);
    }
    for token in tokens {
        let digest = Sha256::digest(token.as_bytes());
        let index = u64::from_be_bytes([
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
        ]) as usize
            % WORKSPACE_EMBEDDING_DIM;
        let sign = if digest[8] & 1 == 0 { 1.0 } else { -1.0 };
        let weight = 1.0 + ((digest[9] % 7) as f32 / 16.0);
        vector[index] += sign * weight;
    }
    let norm = vector
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value = (f64::from(*value) / norm) as f32;
        }
    }
    Ok(vector)
}

fn embedding_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() || character == '_' || character == '-' {
            current.push(character);
        } else if !current.is_empty() {
            push_embedding_token_variants(&mut tokens, &current);
            current.clear();
        }
    }
    if !current.is_empty() {
        push_embedding_token_variants(&mut tokens, &current);
    }
    tokens
}

fn push_embedding_token_variants(tokens: &mut Vec<String>, token: &str) {
    tokens.push(format!("tok:{token}"));
    let chars = token.chars().collect::<Vec<_>>();
    if chars.len() >= 4 {
        for window in chars.windows(4) {
            tokens.push(format!("gram:{}", window.iter().collect::<String>()));
        }
    }
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        let left_value = f64::from(*left_value);
        let right_value = f64::from(*right_value);
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        (dot / (left_norm.sqrt() * right_norm.sqrt())).max(0.0)
    }
}

fn recent_diff_signals(repo_root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut signals = BTreeMap::<String, Vec<String>>::new();
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("status")
        .arg("--porcelain")
        .output();
    let Ok(output) = output else {
        return signals;
    };
    if !output.status.success() {
        return signals;
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.len() < 4 {
            continue;
        }
        let status = line[..2].trim();
        let path = line[3..].split(" -> ").last().unwrap_or_default().trim();
        if path.is_empty() {
            continue;
        }
        let virtual_path = normalize_virtual_path(path).unwrap_or_else(|| format!("/{path}"));
        let label = match status {
            "M" | "MM" | "AM" | "A" | "??" => "recent worktree change",
            "D" => "recent deletion",
            "R" => "recent rename",
            _ => "recent diff signal",
        };
        push_unique_map_signal(
            &mut signals,
            virtual_path,
            label.into(),
            MAX_WORKSPACE_FEATURES,
        );
    }
    signals
}

fn repository_head_sha(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn workspace_fingerprint(repo_root: &Path) -> Option<String> {
    let mut hasher = Sha256::new();
    hasher.update(repo_root.display().to_string().as_bytes());
    if let Some(head) = repository_head_sha(repo_root) {
        hasher.update(head.as_bytes());
    }
    Some(format!("{:x}", hasher.finalize()))
}

fn stable_project_id_for_repo_root(repo_root: &Path) -> String {
    let root_path_string = fs::canonicalize(repo_root)
        .unwrap_or_else(|_| repo_root.to_path_buf())
        .to_string_lossy()
        .into_owned();
    let digest = Sha256::digest(root_path_string.as_bytes());
    let short = digest
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("project_{short}")
}

fn cli_app_data_root(globals: &GlobalOptions) -> PathBuf {
    if globals.state_dir.file_name().and_then(|name| name.to_str()) == Some(HEADLESS_DIRECTORY_NAME)
    {
        globals
            .state_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| globals.state_dir.clone())
    } else {
        globals.state_dir.clone()
    }
}

fn workspace_project_database_path(globals: &GlobalOptions, project_id: &str) -> PathBuf {
    workspace_project_database_path_for_app_root(&cli_app_data_root(globals), project_id)
}

fn workspace_project_database_path_for_app_root(app_data_root: &Path, project_id: &str) -> PathBuf {
    app_data_root
        .join(PROJECTS_DIRECTORY)
        .join(project_id)
        .join(STATE_DATABASE_FILE)
}

fn sha256_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn json_array(values: &[String]) -> Result<String, CliError> {
    serde_json::to_string(values).map_err(|error| {
        CliError::system_fault(
            "xero_cli_workspace_index_json_encode_failed",
            format!("Could not serialize workspace-index features: {error}"),
        )
    })
}

fn decode_string_array(value: &str, label: &'static str) -> Result<Vec<String>, CliError> {
    serde_json::from_str::<Vec<String>>(value).map_err(|error| {
        CliError::system_fault(
            "xero_cli_workspace_index_json_decode_failed",
            format!("Could not decode workspace-index {label}: {error}"),
        )
    })
}

fn push_unique(values: &mut Vec<String>, value: String, limit: usize) {
    if values.len() >= limit || values.iter().any(|existing| existing == &value) {
        return;
    }
    values.push(value);
}

fn push_unique_map_signal(
    values: &mut BTreeMap<String, Vec<String>>,
    path: String,
    value: String,
    limit: usize,
) {
    let entry = values.entry(path).or_default();
    push_unique(entry, value, limit);
}

fn push_reason(values: &mut Vec<String>, value: String) {
    push_unique(values, value, 8);
}

fn coverage_percent(indexed_files: u32, total_files: u32) -> f64 {
    if total_files == 0 {
        0.0
    } else {
        ((indexed_files as f64 / total_files as f64) * 100.0).clamp(0.0, 100.0)
    }
}

fn round_score(score: f64) -> f64 {
    (score * 1000.0).round() / 1000.0
}

fn workspace_diagnostic(
    severity: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
) -> WorkspaceIndexDiagnostic {
    WorkspaceIndexDiagnostic {
        severity: severity.into(),
        code: code.into(),
        message: message.into(),
    }
}

fn sqlite_cli_error(code: &'static str, error: rusqlite::Error) -> CliError {
    CliError::system_fault(
        code,
        format!("Xero workspace index storage failed: {error}"),
    )
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GitNameStatus {
    status: String,
    path: String,
}

fn git_name_status(repo_root: &Path, cached: bool) -> Result<Vec<GitNameStatus>, CliError> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo_root)
        .arg("diff")
        .arg("--name-status");
    if cached {
        command.arg("--cached");
    }
    let output = command.output().map_err(|error| {
        CliError::system_fault(
            "xero_cli_git_failed",
            format!(
                "Could not run git diff in `{}`: {error}",
                repo_root.display()
            ),
        )
    })?;
    if !output.status.success() {
        return Err(CliError::user_fixable(
            "xero_cli_git_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, char::is_whitespace);
            let status = parts.next()?.trim().to_string();
            let path = parts.next()?.trim().to_string();
            (!path.is_empty()).then_some(GitNameStatus { status, path })
        })
        .collect())
}

fn suggest_commit_message(changes: &[GitNameStatus]) -> String {
    if changes.is_empty() {
        return "chore: no local changes".into();
    }

    let paths = changes
        .iter()
        .map(|change| change.path.as_str())
        .collect::<Vec<_>>();
    let prefix = if paths
        .iter()
        .all(|path| path.ends_with(".md") || path.starts_with("docs/"))
    {
        "docs"
    } else if paths
        .iter()
        .all(|path| path.contains("test") || path.contains("spec"))
    {
        "test"
    } else if paths
        .iter()
        .any(|path| path.contains("fix") || path.contains("bug"))
    {
        "fix"
    } else {
        "chore"
    };

    if paths.len() == 1 {
        format!("{prefix}: update {}", paths[0])
    } else {
        format!("{prefix}: update {} files", paths.len())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandSuggestion {
    command: String,
    reason: String,
}

fn suggest_shell_commands(request: &str) -> Vec<CommandSuggestion> {
    let lower = request.to_ascii_lowercase();
    if lower.contains("git status") || lower.contains("what changed") {
        return vec![CommandSuggestion {
            command: "git status --short".into(),
            reason: "Shows a compact worktree summary.".into(),
        }];
    }
    if lower.contains("find") || lower.contains("search") || lower.contains("grep") {
        return vec![CommandSuggestion {
            command: "rg \"pattern\"".into(),
            reason: "Searches the workspace quickly with ripgrep.".into(),
        }];
    }
    if lower.contains("list") && lower.contains("file") {
        return vec![CommandSuggestion {
            command: "rg --files".into(),
            reason: "Lists tracked and visible workspace files quickly.".into(),
        }];
    }
    if lower.contains("test") {
        return vec![CommandSuggestion {
            command: "cargo test -p xero-agent-core".into(),
            reason: "Runs a scoped Rust package test without taking the whole workspace lock for longer than needed.".into(),
        }];
    }
    if lower.contains("format") || lower.contains("fmt") {
        return vec![CommandSuggestion {
            command: "cargo fmt -p xero-agent-core -p xero-cli".into(),
            reason: "Formats the headless harness crates only.".into(),
        }];
    }
    vec![CommandSuggestion {
        command: "rg --files".into(),
        reason: "A safe first inspection command for most local workspace questions.".into(),
    }]
}

fn canonicalize_existing_path(path: &str) -> Result<PathBuf, CliError> {
    fs::canonicalize(path).map_err(|error| {
        CliError::user_fixable(
            "xero_cli_path_not_found",
            format!("Could not resolve path `{path}`: {error}"),
        )
    })
}

fn read_json_file<T>(path: &Path) -> Result<T, CliError>
where
    T: for<'de> Deserialize<'de>,
{
    let bytes = fs::read(path).map_err(|error| {
        CliError::user_fixable(
            "xero_cli_state_read_failed",
            format!("Could not read `{}`: {error}", path.display()),
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        CliError::user_fixable(
            "xero_cli_state_decode_failed",
            format!("Could not decode `{}`: {error}", path.display()),
        )
    })
}

fn write_json_file<T>(path: &Path, value: &T) -> Result<(), CliError>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::system_fault(
                "xero_cli_state_prepare_failed",
                format!("Could not create `{}`: {error}", parent.display()),
            )
        })?;
    }
    let payload = serde_json::to_vec_pretty(value).map_err(|error| {
        CliError::system_fault(
            "xero_cli_state_encode_failed",
            format!(
                "Could not encode JSON state for `{}`: {error}",
                path.display()
            ),
        )
    })?;
    fs::write(path, payload).map_err(|error| {
        CliError::system_fault(
            "xero_cli_state_write_failed",
            format!("Could not write `{}`: {error}", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_provider_harness_agent_exec_persists_headless_run_for_conversation_show() {
        let state_dir = unique_temp_dir("agent-exec");
        let exec = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--project-id",
            "project-1",
            "--session-id",
            "session-1",
            "--run-id",
            "run-1",
            "--provider",
            "fake_provider",
            "Summarize this repo.",
        ])
        .expect("agent exec should succeed");
        assert_eq!(exec.output_mode, OutputMode::Json);

        let show = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "conversation",
            "show",
            "run-1",
        ])
        .expect("conversation show should load persisted run");

        assert_eq!(show.json["snapshot"]["status"], json!("completed"));
        assert_eq!(
            show.json["snapshot"]["contextManifests"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn agent_exec_requires_explicit_provider_when_unconfigured() {
        let state_dir = unique_temp_dir("agent-exec-provider-required");
        let error = run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "Summarize this repo.",
        ])
        .expect_err("unconfigured real provider should be rejected");

        assert_eq!(error.code, "xero_cli_provider_required");
    }

    #[test]
    fn real_provider_agent_exec_rejects_missing_app_data_registry_without_json_store() {
        let state_dir = unique_temp_dir("agent-exec-real-provider-contract");
        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            "http://127.0.0.1:9/v1",
        ])
        .expect("provider profile should be recorded");

        let error = run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--provider",
            "openai_api",
            "--model",
            "test-model",
            "--project-id",
            "project-real",
            "Complete a real provider run.",
        ])
        .expect_err("real provider should require app-data project registration");

        assert_eq!(error.code, "xero_cli_project_registry_missing");
        assert!(
            !state_dir.join(AGENT_CORE_STATE_FILE).exists(),
            "real provider rejection must happen before opening the harness JSON store"
        );
    }

    #[test]
    fn real_provider_agent_exec_rejects_missing_app_data_project_database() {
        let state_dir = unique_temp_dir("agent-exec-real-provider-missing-db");
        let repo_dir = unique_temp_dir("agent-exec-real-provider-missing-db-repo");
        seed_global_project_registration_only(&state_dir, "project-real", &repo_dir);
        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            "http://127.0.0.1:9/v1",
        ])
        .expect("provider profile should be recorded");

        let error = run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--provider",
            "openai_api",
            "--model",
            "test-model",
            "--project-id",
            "project-real",
            "Complete a real provider run.",
        ])
        .expect_err("missing app-data project state should be rejected");

        assert_eq!(error.code, "xero_cli_project_state_missing");
        assert!(
            !state_dir.join(AGENT_CORE_STATE_FILE).exists(),
            "real provider rejection must not create the harness JSON store"
        );
    }

    #[test]
    fn real_provider_agent_exec_rejects_unimported_current_project_root() {
        let state_dir = unique_temp_dir("agent-exec-real-provider-unimported");
        let registered_repo = unique_temp_dir("agent-exec-real-provider-registered-repo");
        let unregistered_repo = unique_temp_dir("agent-exec-real-provider-unregistered-repo");
        seed_registered_project(&state_dir, "project-registered", &registered_repo);
        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            "http://127.0.0.1:9/v1",
        ])
        .expect("provider profile should be recorded");

        let previous_dir = env::current_dir().expect("current dir");
        env::set_current_dir(&unregistered_repo).expect("switch to unregistered repo");
        let error = run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--provider",
            "openai_api",
            "--model",
            "test-model",
            "Complete a real provider run.",
        ])
        .expect_err("unimported current project root should be rejected");
        env::set_current_dir(previous_dir).expect("restore current dir");

        assert_eq!(error.code, "xero_cli_project_unimported");
        assert!(
            !state_dir.join(AGENT_CORE_STATE_FILE).exists(),
            "real provider rejection must not create the harness JSON store"
        );
    }

    #[test]
    fn cli_show_and_dump_read_desktop_style_app_data_run() {
        let state_dir = unique_temp_dir("desktop-style-run");
        let repo_dir = unique_temp_dir("desktop-style-run-repo");
        let trace_id = seed_desktop_style_app_data_run(
            &state_dir,
            "project-desktop",
            &repo_dir,
            "session-desktop",
            "run-desktop",
        );

        let show = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "conversation",
            "show",
            "--project-id",
            "project-desktop",
            "run-desktop",
        ])
        .expect("conversation show should read desktop-style app-data state");
        assert_eq!(show.json["snapshot"]["traceId"], json!(trace_id));
        assert_eq!(
            show.json["snapshot"]["contextManifests"][0]["manifestId"],
            json!("manifest-desktop-run-0")
        );

        let dump = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "conversation",
            "dump",
            "--project-id",
            "project-desktop",
            "run-desktop",
        ])
        .expect("conversation dump should export the canonical trace");
        assert_eq!(
            dump.json["canonicalTrace"]["traceId"],
            show.json["snapshot"]["traceId"]
        );
        assert_eq!(
            dump.json["timeline"]["items"][0]["eventKind"],
            json!("run_started")
        );
        assert!(
            !state_dir.join(AGENT_CORE_STATE_FILE).exists(),
            "desktop-style app-data inspection must not create the harness JSON store"
        );
    }

    #[test]
    fn real_provider_uses_project_store() {
        let state_dir = unique_temp_dir("agent-exec-real-provider-store");
        let repo_dir = unique_temp_dir("agent-exec-real-provider-store-repo");
        seed_registered_project(&state_dir, "project-real", &repo_dir);
        let server = MockOpenAiCompatibleServer::start(vec![json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Real provider run completed."
                }
            }]
        })]);

        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            server.base_url.as_str(),
        ])
        .expect("provider profile should be recorded");

        let previous_dir = env::current_dir().expect("current dir");
        env::set_current_dir(&repo_dir).expect("switch to registered repo");
        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--provider",
            "openai_api",
            "--model",
            "test-model",
            "--session-id",
            "session-real",
            "--run-id",
            "run-real",
            "Complete a real provider run.",
        ]);
        env::set_current_dir(previous_dir).expect("restore current dir");
        let output = output.expect("real provider run should succeed");
        server.join();

        assert_eq!(output.json["executionMode"], json!("real_provider"));
        assert_eq!(output.json["snapshot"]["status"], json!("completed"));
        assert!(
            !state_dir.join(AGENT_CORE_STATE_FILE).exists(),
            "real provider runs must not write the harness JSON store"
        );
        let project_database =
            workspace_project_database_path_for_app_root(&state_dir, "project-real");
        assert_eq!(
            output.json["storePath"],
            json!(project_database.display().to_string()),
            "real provider runs must use app-data project state"
        );
        let show = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "conversation",
            "show",
            "--project-id",
            "project-real",
            "run-real",
        ])
        .expect("CLI conversation show should read the app-data project run");
        assert_eq!(
            show.json["snapshot"]["traceId"],
            output.json["snapshot"]["traceId"]
        );
    }

    #[test]
    fn real_provider_uses_tool_registry_v2() {
        let state_dir = unique_temp_dir("agent-exec-real-provider-v2");
        let workspace = unique_temp_dir("agent-exec-real-provider-v2-workspace");
        seed_registered_project(&state_dir, "project-real", &workspace);
        fs::write(workspace.join("tracked.txt"), "tracked\n").expect("write tracked file");
        let server = MockOpenAiCompatibleServer::start(vec![
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call-read",
                            "type": "function",
                            "function": {
                                "name": "read",
                                "arguments": "{\"path\":\"tracked.txt\"}"
                            }
                        }, {
                            "id": "call-list",
                            "type": "function",
                            "function": {
                                "name": "list",
                                "arguments": "{\"path\":\".\"}"
                            }
                        }]
                    }
                }]
            }),
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Listed files through Tool Registry V2."
                    }
                }]
            }),
        ]);

        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            server.base_url.as_str(),
        ])
        .expect("provider profile should be recorded");

        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--provider",
            "openai_api",
            "--model",
            "test-model",
            "--project-id",
            "project-real",
            "--session-id",
            "session-real",
            "--run-id",
            "run-real",
            "List files.",
        ]);
        let output = output.expect("real provider run should succeed");
        server.join();

        let events = output.json["snapshot"]["events"]
            .as_array()
            .expect("events");
        assert!(events.iter().any(
            |event| event["eventKind"] == json!("tool_registry_snapshot")
                && event["payload"]["executionRegistry"] == json!("tool_registry_v2")
        ));
        let registry = events
            .iter()
            .find(|event| event["eventKind"] == json!("tool_registry_snapshot"))
            .expect("tool registry snapshot");
        assert!(registry["payload"]["descriptorNames"]
            .as_array()
            .expect("descriptor names")
            .iter()
            .any(|name| name == "read"));
        assert!(registry["payload"]["descriptorNames"]
            .as_array()
            .expect("descriptor names")
            .iter()
            .all(|name| {
                !matches!(
                    name.as_str(),
                    Some("read_file" | "write_file" | "list_files")
                )
            }));
        assert!(events
            .iter()
            .filter(|event| {
                event["eventKind"] == json!("tool_started")
                    || event["eventKind"] == json!("tool_completed")
            })
            .all(|event| event["payload"]["dispatch"]["registryVersion"]
                == json!("tool_registry_v2")));
        let completed_ids = events
            .iter()
            .filter(|event| event["eventKind"] == json!("tool_completed"))
            .map(|event| event["payload"]["toolCallId"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(
            completed_ids,
            vec!["call-read", "call-list"],
            "read-only V2 batches must preserve provider tool-call result order"
        );
    }

    #[test]
    fn real_provider_write_records_v2_rollback_and_reservation_metadata() {
        let state_dir = unique_temp_dir("agent-exec-real-provider-v2-write");
        let workspace = unique_temp_dir("agent-exec-real-provider-v2-write-workspace");
        seed_registered_project(&state_dir, "project-real", &workspace);
        let server = MockOpenAiCompatibleServer::start(vec![
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call-write",
                            "type": "function",
                            "function": {
                                "name": "write",
                                "arguments": "{\"path\":\"scratch/generated.txt\",\"content\":\"hello from v2\\n\"}"
                            }
                        }]
                    }
                }]
            }),
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Wrote through Tool Registry V2."
                    }
                }]
            }),
        ]);

        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            server.base_url.as_str(),
        ])
        .expect("provider profile should be recorded");

        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--provider",
            "openai_api",
            "--model",
            "test-model",
            "--project-id",
            "project-real",
            "--session-id",
            "session-real",
            "--run-id",
            "run-real",
            "Write a scratch file.",
        ]);
        let output = output.expect("real provider run should succeed");
        server.join();

        assert_eq!(
            fs::read_to_string(workspace.join("scratch/generated.txt")).expect("generated file"),
            "hello from v2\n"
        );
        let events = output.json["snapshot"]["events"]
            .as_array()
            .expect("events");
        let started = events
            .iter()
            .find(|event| {
                event["eventKind"] == json!("tool_started")
                    && event["payload"]["toolCallId"] == json!("call-write")
            })
            .expect("started write event");
        assert_eq!(started["payload"]["inputRedacted"], json!(true));
        assert_eq!(
            started["payload"]["input"]["content"]["redacted"],
            json!(true)
        );

        let completed = events
            .iter()
            .find(|event| {
                event["eventKind"] == json!("tool_completed")
                    && event["payload"]["toolCallId"] == json!("call-write")
            })
            .expect("completed write event");
        assert_eq!(
            completed["payload"]["dispatch"]["registryVersion"],
            json!("tool_registry_v2")
        );
        assert_eq!(
            completed["payload"]["dispatch"]["fileReservation"]["kind"],
            json!("file_reservation")
        );
        assert_eq!(
            completed["payload"]["dispatch"]["rollback"]["kind"],
            json!("file_rollback")
        );
        assert!(events.iter().any(|event| {
            event["eventKind"] == json!("file_changed")
                && event["payload"]["dispatch"]["registryVersion"] == json!("tool_registry_v2")
        }));
    }

    #[test]
    fn real_provider_denied_write_emits_typed_v2_failure_without_writing() {
        let state_dir = unique_temp_dir("agent-exec-real-provider-v2-denied-write");
        let workspace = unique_temp_dir("agent-exec-real-provider-v2-denied-write-workspace");
        seed_registered_project(&state_dir, "project-real", &workspace);
        let server = MockOpenAiCompatibleServer::start(vec![
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call-denied",
                            "type": "function",
                            "function": {
                                "name": "write",
                                "arguments": "{\"path\":\".xero/blocked.txt\",\"content\":\"must not write\"}"
                            }
                        }]
                    }
                }]
            }),
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Write was denied."
                    }
                }]
            }),
        ]);

        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            server.base_url.as_str(),
        ])
        .expect("provider profile should be recorded");

        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--provider",
            "openai_api",
            "--model",
            "test-model",
            "--project-id",
            "project-real",
            "--session-id",
            "session-real",
            "--run-id",
            "run-real",
            "Attempt a denied write.",
        ]);
        let output = output.expect("real provider run should finish after denied tool result");
        server.join();

        assert!(
            !workspace.join(".xero/blocked.txt").exists(),
            "sandbox-denied V2 write must not execute the handler"
        );
        let events = output.json["snapshot"]["events"]
            .as_array()
            .expect("events");
        let completed = events
            .iter()
            .find(|event| {
                event["eventKind"] == json!("tool_completed")
                    && event["payload"]["toolCallId"] == json!("call-denied")
            })
            .expect("completed denied event");
        assert_eq!(completed["payload"]["ok"], json!(false));
        assert_eq!(
            completed["payload"]["dispatch"]["typedErrorCategory"],
            json!("sandbox_denied")
        );
        assert_eq!(
            completed["payload"]["dispatch"]["sandbox"]["exitClassification"],
            json!("denied_by_sandbox")
        );
        assert!(completed["payload"]["dispatch"]["sandbox"]["blockedReason"]
            .as_str()
            .is_some_and(|reason| reason.contains(".xero")));
    }

    #[test]
    fn real_provider_legacy_mini_tool_names_are_unavailable() {
        let state_dir = unique_temp_dir("agent-exec-real-provider-v2-legacy-tool");
        let workspace = unique_temp_dir("agent-exec-real-provider-v2-legacy-tool-workspace");
        seed_registered_project(&state_dir, "project-real", &workspace);
        let server = MockOpenAiCompatibleServer::start(vec![
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call-legacy",
                            "type": "function",
                            "function": {
                                "name": "write_file",
                                "arguments": "{\"path\":\"legacy.txt\",\"content\":\"old loop\"}"
                            }
                        }]
                    }
                }]
            }),
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Legacy tool was unavailable."
                    }
                }]
            }),
        ]);

        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            server.base_url.as_str(),
        ])
        .expect("provider profile should be recorded");

        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--provider",
            "openai_api",
            "--model",
            "test-model",
            "--project-id",
            "project-real",
            "--session-id",
            "session-real",
            "--run-id",
            "run-real",
            "Try a legacy mini tool.",
        ]);
        let output = output.expect("real provider run should finish after unavailable tool result");
        server.join();

        assert!(
            !workspace.join("legacy.txt").exists(),
            "legacy mini tool names must not reach direct filesystem handlers"
        );
        let events = output.json["snapshot"]["events"]
            .as_array()
            .expect("events");
        let registry = events
            .iter()
            .find(|event| event["eventKind"] == json!("tool_registry_snapshot"))
            .expect("tool registry snapshot");
        assert_eq!(
            registry["payload"]["legacyMiniToolsAvailable"],
            json!(false)
        );
        let completed = events
            .iter()
            .find(|event| {
                event["eventKind"] == json!("tool_completed")
                    && event["payload"]["toolCallId"] == json!("call-legacy")
            })
            .expect("completed legacy event");
        assert_eq!(completed["payload"]["ok"], json!(false));
        assert_eq!(
            completed["payload"]["dispatch"]["typedErrorCategory"],
            json!("tool_unavailable")
        );
        assert_eq!(
            completed["payload"]["dispatch"]["registryVersion"],
            json!("tool_registry_v2")
        );
    }

    #[test]
    fn real_provider_exposes_command_and_patch_tools_for_benchmark_workloads() {
        let state_dir = unique_temp_dir("agent-exec-real-provider-v2-command-patch");
        let workspace = unique_temp_dir("agent-exec-real-provider-v2-command-patch-workspace");
        seed_registered_project(&state_dir, "project-real", &workspace);
        let patch = [
            "diff --git a/generated.txt b/generated.txt",
            "new file mode 100644",
            "index 0000000..8baef1b",
            "--- /dev/null",
            "+++ b/generated.txt",
            "@@ -0,0 +1 @@",
            "+patched",
            "",
        ]
        .join("\n");
        let command_args = serde_json::to_string(&json!({
            "argv": ["sh", "-c", "printf command-ok"],
            "timeoutMs": 5000
        }))
        .expect("command args");
        let patch_args = serde_json::to_string(&json!({ "patch": patch })).expect("patch args");
        let server = MockOpenAiCompatibleServer::start(vec![
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call-command",
                            "type": "function",
                            "function": {
                                "name": "command",
                                "arguments": command_args
                            }
                        }, {
                            "id": "call-patch",
                            "type": "function",
                            "function": {
                                "name": "patch",
                                "arguments": patch_args
                            }
                        }]
                    }
                }]
            }),
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Command and patch completed."
                    }
                }]
            }),
        ]);

        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            server.base_url.as_str(),
        ])
        .expect("provider profile should be recorded");

        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--provider",
            "openai_api",
            "--model",
            "test-model",
            "--project-id",
            "project-real",
            "--session-id",
            "session-real",
            "--run-id",
            "run-real",
            "Run a command and apply a patch.",
        ]);
        let output = output.expect("real provider run should succeed");
        server.join();

        assert_eq!(
            fs::read_to_string(workspace.join("generated.txt")).expect("patched file"),
            "patched\n"
        );
        let events = output.json["snapshot"]["events"]
            .as_array()
            .expect("events");
        let registry = events
            .iter()
            .find(|event| event["eventKind"] == json!("tool_registry_snapshot"))
            .expect("tool registry snapshot");
        for tool_name in ["command", "patch"] {
            assert!(registry["payload"]["descriptorNames"]
                .as_array()
                .expect("descriptor names")
                .iter()
                .any(|name| name == tool_name));
        }
        let completed = events
            .iter()
            .filter(|event| event["eventKind"] == json!("tool_completed"))
            .map(|event| event["payload"]["toolName"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(completed, vec!["command", "patch"]);
        assert!(events.iter().any(|event| {
            event["eventKind"] == json!("file_changed")
                && event["payload"]["operation"] == json!("patch")
                && event["payload"]["path"] == json!("generated.txt")
        }));
    }

    #[test]
    fn fake_provider_harness_conversation_dump_and_support_bundle_use_canonical_trace_snapshot() {
        let state_dir = unique_temp_dir("conversation-trace");
        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--project-id",
            "project-trace",
            "--session-id",
            "session-trace",
            "--run-id",
            "run-trace",
            "--provider",
            "fake_provider",
            "Summarize bearer token handling without exposing Bearer secret-token.",
        ])
        .expect("agent exec should succeed");

        let dump = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "conversation",
            "dump",
            "run-trace",
        ])
        .expect("conversation dump should export canonical trace");
        assert_eq!(
            dump.json["canonicalTrace"]["generatedFrom"],
            JsonValue::Null,
            "canonical traces are snapshots, not support bundles"
        );
        assert_eq!(
            dump.json["canonicalTrace"]["traceId"],
            dump.json["trace"]["traceId"]
        );
        assert!(
            dump.json["qualityGates"]["gates"].is_array(),
            "harness trace exports should expose quality gates without counting as production proof"
        );
        assert_eq!(
            dump.json["productionReadiness"]["traceId"],
            dump.json["canonicalTrace"]["traceId"]
        );
        assert_eq!(
            dump.json["productionReadiness"]["status"],
            json!("blocked"),
            "trace exports must not imply release readiness without focused test evidence"
        );

        let bundle = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "conversation",
            "support-bundle",
            "run-trace",
        ])
        .expect("support bundle should export");
        let serialized = serde_json::to_string(&bundle.json).expect("serialize support bundle");
        assert!(!serialized.contains("secret-token"));
        assert!(serialized.contains("canonical_runtime_trace_snapshot"));
        assert_eq!(
            bundle.json["supportBundle"]["productionReadiness"]["traceId"],
            dump.json["canonicalTrace"]["traceId"]
        );
    }

    #[test]
    fn fake_provider_harness_conversation_compact_and_clone_use_facade_operations() {
        let state_dir = unique_temp_dir("conversation-facade-ops");
        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--project-id",
            "project-facade",
            "--session-id",
            "session-facade",
            "--run-id",
            "run-facade",
            "--provider",
            "fake_provider",
            "Create a durable conversation that can be compacted and cloned.",
        ])
        .expect("agent exec should succeed");

        let compact = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "conversation",
            "compact",
            "run-facade",
        ])
        .expect("conversation compact should use the facade");
        assert!(compact.json["snapshot"]["contextManifests"]
            .as_array()
            .expect("context manifests")
            .iter()
            .any(|manifest| manifest["manifest"]["kind"] == json!("session_compaction_artifact")));
        assert!(compact.json["snapshot"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| event["eventKind"] == json!("policy_decision")
                && event["payload"]["kind"] == json!("session_compaction")));

        let cloned = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "conversation",
            "clone",
            "run-facade",
        ])
        .expect("conversation clone should fork through the facade");
        assert_ne!(
            cloned.json["snapshot"]["agentSessionId"],
            json!("session-facade")
        );
        assert!(cloned.json["snapshot"]["contextManifests"]
            .as_array()
            .expect("clone manifests")
            .iter()
            .any(|manifest| manifest["manifest"]["kind"] == json!("session_fork")));
        assert!(cloned.json["snapshot"]["messages"]
            .as_array()
            .expect("clone messages")
            .iter()
            .any(|message| message["role"] == json!("user")));

        let retried = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "conversation",
            "retry",
            "run-facade",
        ])
        .expect("conversation retry should use the resolved runtime");
        assert_ne!(retried.json["snapshot"]["runId"], json!("run-facade"));
        assert_eq!(retried.json["snapshot"]["status"], json!("completed"));
        assert_eq!(retried.json["sourceRunId"], json!("run-facade"));
    }

    #[test]
    fn workspace_index_query_round_trip_uses_app_data_state() {
        let state_dir = unique_temp_dir("workspace-state");
        let repo_dir = unique_temp_dir("workspace-repo");
        fs::create_dir_all(repo_dir.join("src")).expect("create src");
        fs::write(
            repo_dir.join("src/lib.rs"),
            "pub fn durable_context_manifest() -> &'static str { \"manifest\" }",
        )
        .expect("write source");

        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "workspace",
            "index",
            "--repo",
            repo_dir.to_str().expect("repo dir"),
        ])
        .expect("workspace index should succeed");

        let query = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "workspace",
            "query",
            "--repo",
            repo_dir.to_str().expect("repo dir"),
            "durable manifest",
        ])
        .expect("workspace query should succeed");

        assert_eq!(
            query.json["response"]["results"][0]["path"],
            json!("/src/lib.rs")
        );
    }

    #[test]
    fn workspace_status_reset_and_staleness_use_semantic_project_state() {
        let state_dir = unique_temp_dir("workspace-status-state");
        let repo_dir = unique_temp_dir("workspace-status-repo");
        fs::create_dir_all(repo_dir.join("src")).expect("create src");
        fs::write(
            repo_dir.join("src/lib.rs"),
            "pub fn semantic_workspace_status() -> bool { true }\n",
        )
        .expect("write source");

        let indexed = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "workspace",
            "index",
            "--repo",
            repo_dir.to_str().expect("repo dir"),
            "--project-id",
            "project-status",
        ])
        .expect("workspace index should succeed");
        assert_eq!(
            indexed.json["response"]["status"]["storagePath"],
            json!(state_dir
                .join("projects/project-status")
                .display()
                .to_string())
        );

        fs::write(
            repo_dir.join("src/lib.rs"),
            "pub fn semantic_workspace_status_changed() -> bool { true }\n",
        )
        .expect("modify source");

        let status = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "workspace",
            "status",
            "--repo",
            repo_dir.to_str().expect("repo dir"),
            "--project-id",
            "project-status",
        ])
        .expect("workspace status should succeed");
        assert_eq!(status.json["status"]["state"], json!("stale"));
        assert_eq!(status.json["status"]["staleFiles"], json!(1));

        let reset = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "workspace",
            "reset",
            "--repo",
            repo_dir.to_str().expect("repo dir"),
            "--project-id",
            "project-status",
        ])
        .expect("workspace reset should succeed");
        assert_eq!(reset.json["status"]["state"], json!("empty"));
    }

    #[test]
    fn workspace_query_modes_rank_symbols_tests_and_impacts() {
        let state_dir = unique_temp_dir("workspace-modes-state");
        let repo_dir = unique_temp_dir("workspace-modes-repo");
        fs::create_dir_all(repo_dir.join("src")).expect("create src");
        fs::create_dir_all(repo_dir.join("tests")).expect("create tests");
        fs::write(
            repo_dir.join("src/payment.ts"),
            "import { ledger } from './ledger'\nexport function reconcileInvoice() { return ledger() }\n",
        )
        .expect("write source");
        fs::write(
            repo_dir.join("tests/payment.test.ts"),
            "import { reconcileInvoice } from '../src/payment'\ntest('reconcile invoice', () => reconcileInvoice())\n",
        )
        .expect("write test");

        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "workspace",
            "index",
            "--repo",
            repo_dir.to_str().expect("repo dir"),
            "--project-id",
            "project-modes",
        ])
        .expect("workspace index should succeed");

        let symbol = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "workspace",
            "query",
            "--repo",
            repo_dir.to_str().expect("repo dir"),
            "--project-id",
            "project-modes",
            "--mode",
            "symbol",
            "reconcileInvoice",
        ])
        .expect("symbol query should succeed");
        assert_eq!(
            symbol.json["response"]["results"][0]["path"],
            json!("/src/payment.ts")
        );

        let tests = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "workspace",
            "query",
            "--repo",
            repo_dir.to_str().expect("repo dir"),
            "--project-id",
            "project-modes",
            "--mode",
            "related-tests",
            "payment test",
        ])
        .expect("related tests query should succeed");
        assert_eq!(
            tests.json["response"]["results"][0]["path"],
            json!("/tests/payment.test.ts")
        );

        let impact = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "workspace",
            "query",
            "--repo",
            repo_dir.to_str().expect("repo dir"),
            "--project-id",
            "project-modes",
            "--mode",
            "impact",
            "ledger",
        ])
        .expect("impact query should succeed");
        assert_eq!(
            impact.json["response"]["results"][0]["path"],
            json!("/src/payment.ts")
        );
    }

    #[test]
    fn fake_provider_harness_ci_mode_records_strict_sandbox_defaults() {
        let state_dir = unique_temp_dir("ci-mode");
        let output = run_with_args([
            "xero",
            "--json",
            "--ci",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--run-id",
            "run-ci",
            "--provider",
            "fake_provider",
            "Run in CI mode.",
        ])
        .expect("ci agent exec should succeed");

        assert_eq!(
            output.json["sandboxDefaults"]["profile"],
            json!("ci_strict")
        );
        assert_eq!(output.json["snapshot"]["runId"], json!("run-ci"));
    }

    #[test]
    fn fake_provider_harness_mcp_server_lists_tools_and_queries_started_run() {
        let state_dir = unique_temp_dir("mcp-server");
        let globals = GlobalOptions {
            output_mode: OutputMode::Json,
            ci: false,
            state_dir,
        };
        let messages = [
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "xero-test", "version": "1.0.0" }
                }
            }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "xero_start_run",
                    "arguments": {
                        "projectId": "project-mcp",
                        "sessionId": "session-mcp",
                        "runId": "run-mcp",
                        "providerId": "fake_provider",
                        "prompt": "Start from MCP."
                    }
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "xero_query_conversation",
                    "arguments": {
                        "projectId": "project-mcp",
                        "runId": "run-mcp"
                    }
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "xero_export_trace",
                    "arguments": {
                        "projectId": "project-mcp",
                        "runId": "run-mcp"
                    }
                }
            }),
        ];
        let input = messages
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .expect("serialize MCP messages")
            .join("\n")
            + "\n";
        let mut output = Vec::new();
        run_mcp_jsonrpc_stream(globals, std::io::Cursor::new(input), &mut output)
            .expect("MCP stream should complete");
        let responses = String::from_utf8(output)
            .expect("utf8 output")
            .lines()
            .map(|line| serde_json::from_str::<JsonValue>(line).expect("json response"))
            .collect::<Vec<_>>();

        assert_eq!(
            responses.len(),
            5,
            "initialized notification has no response"
        );
        assert_eq!(
            responses[0]["result"]["protocolVersion"],
            json!(MCP_PROTOCOL_VERSION)
        );
        assert!(responses[1]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == json!("xero_export_trace")));
        assert_eq!(responses[2]["result"]["isError"], json!(false));
        assert_eq!(
            responses[2]["result"]["structuredContent"]["providerPreflight"]["source"],
            json!("live_probe")
        );
        assert_eq!(
            responses[3]["result"]["structuredContent"]["snapshot"]["runId"],
            json!("run-mcp")
        );
        assert_eq!(responses[4]["result"]["isError"], json!(false));
        assert_eq!(
            responses[4]["result"]["structuredContent"]["trace"]["snapshot"]["runId"],
            json!("run-mcp")
        );
    }

    #[test]
    fn real_provider_mcp_start_run_rejects_missing_app_data_registry_without_json_store() {
        let state_dir = unique_temp_dir("mcp-real-provider-contract");
        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            "http://127.0.0.1:9/v1",
        ])
        .expect("provider profile should be recorded");

        let globals = GlobalOptions {
            output_mode: OutputMode::Json,
            ci: false,
            state_dir: state_dir.clone(),
        };
        let messages = [
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "xero-test", "version": "1.0.0" }
                }
            }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "xero_start_run",
                    "arguments": {
                        "projectId": "project-mcp-real",
                        "providerId": "openai_api",
                        "modelId": "test-model",
                        "prompt": "Start a real provider run from MCP."
                    }
                }
            }),
        ];
        let input = messages
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .expect("serialize MCP messages")
            .join("\n")
            + "\n";
        let mut output = Vec::new();
        run_mcp_jsonrpc_stream(globals, std::io::Cursor::new(input), &mut output)
            .expect("MCP stream should complete");
        let responses = String::from_utf8(output)
            .expect("utf8 output")
            .lines()
            .map(|line| serde_json::from_str::<JsonValue>(line).expect("json response"))
            .collect::<Vec<_>>();

        assert_eq!(responses[1]["result"]["isError"], json!(true));
        assert_eq!(
            responses[1]["result"]["structuredContent"]["error"]["code"],
            json!("xero_cli_project_registry_missing")
        );
        assert!(
            !state_dir.join(AGENT_CORE_STATE_FILE).exists(),
            "real MCP rejection must happen before opening the harness JSON store"
        );
    }

    #[test]
    fn mcp_uses_canonical_runtime() {
        let state_dir = unique_temp_dir("mcp-real-provider");
        let repo_dir = unique_temp_dir("mcp-real-provider-repo");
        seed_registered_project(&state_dir, "project-mcp-real", &repo_dir);
        let server = MockOpenAiCompatibleServer::start(vec![json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "MCP real provider run completed."
                }
            }]
        })]);

        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            server.base_url.as_str(),
        ])
        .expect("provider profile should be recorded");

        let globals = GlobalOptions {
            output_mode: OutputMode::Json,
            ci: false,
            state_dir: state_dir.clone(),
        };
        let messages = [
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "xero-test", "version": "1.0.0" }
                }
            }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "xero_start_run",
                    "arguments": {
                        "projectId": "project-mcp-real",
                        "sessionId": "session-mcp-real",
                        "runId": "run-mcp-real",
                        "providerId": "openai_api",
                        "modelId": "test-model",
                        "prompt": "Start a real provider run from MCP."
                    }
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "xero_export_trace",
                    "arguments": {
                        "projectId": "project-mcp-real",
                        "runId": "run-mcp-real",
                        "includeSupportBundle": true
                    }
                }
            }),
        ];
        let input = messages
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .expect("serialize MCP messages")
            .join("\n")
            + "\n";
        let mut output = Vec::new();
        run_mcp_jsonrpc_stream(globals, std::io::Cursor::new(input), &mut output)
            .expect("MCP stream should complete");
        server.join();
        let responses = String::from_utf8(output)
            .expect("utf8 output")
            .lines()
            .map(|line| serde_json::from_str::<JsonValue>(line).expect("json response"))
            .collect::<Vec<_>>();

        assert_eq!(responses[1]["result"]["isError"], json!(false));
        assert_eq!(
            responses[2]["result"]["structuredContent"]["trace"]["snapshot"]["runId"],
            json!("run-mcp-real")
        );
        let show = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "conversation",
            "show",
            "--project-id",
            "project-mcp-real",
            "run-mcp-real",
        ])
        .expect("CLI should query the MCP-created app-data run");
        assert_eq!(
            show.json["snapshot"]["traceId"],
            responses[1]["result"]["structuredContent"]["snapshot"]["traceId"]
        );
        assert_eq!(
            responses[2]["result"]["structuredContent"]["canonicalTrace"]["traceId"],
            show.json["snapshot"]["traceId"]
        );
        assert_eq!(
            responses[2]["result"]["structuredContent"]["productionReadiness"]["traceId"],
            show.json["snapshot"]["traceId"]
        );
        let exported_event_ids = responses[2]["result"]["structuredContent"]["trace"]["snapshot"]
            ["events"]
            .as_array()
            .expect("exported events")
            .iter()
            .map(|event| event["id"].clone())
            .collect::<Vec<_>>();
        let shown_event_ids = show.json["snapshot"]["events"]
            .as_array()
            .expect("shown events")
            .iter()
            .map(|event| event["id"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            exported_event_ids, shown_event_ids,
            "MCP export and CLI show must reference the same canonical event ids"
        );
        assert!(
            !state_dir.join(AGENT_CORE_STATE_FILE).exists(),
            "real MCP runs must not use the harness JSON store"
        );
        assert_eq!(
            responses[2]["result"]["structuredContent"]["supportBundle"]["run"]["runId"],
            json!("run-mcp-real"),
            "MCP support bundle must be generated from the same canonical run snapshot"
        );
        assert_eq!(
            responses[2]["result"]["structuredContent"]["supportBundle"]["traceId"],
            show.json["snapshot"]["traceId"]
        );
        assert!(
            responses[2]["result"]["structuredContent"]["supportBundle"]["qualityGates"]
                ["gates"]
                .is_array(),
            "MCP support bundle should include trace quality gates without treating later-phase gates as Phase 2 blockers"
        );
    }

    #[test]
    fn agent_exec_rejects_external_provider_catalog_mismatch() {
        let state_dir = unique_temp_dir("provider-mismatch");
        let error = run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "exec",
            "--provider",
            "external_codex_cli",
            "This should not run through the owned fake provider.",
        ])
        .expect_err("external provider should be rejected by agent exec");

        assert_eq!(error.code, "xero_cli_provider_external_agent_mismatch");
    }

    #[test]
    fn provider_list_uses_shared_capability_catalog() {
        let state_dir = unique_temp_dir("provider-list-capabilities");
        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "list",
        ])
        .expect("provider list should succeed");
        let providers = output.json["providers"]
            .as_array()
            .expect("provider list array");
        let openrouter = providers
            .iter()
            .find(|provider| provider["providerId"] == json!("openrouter"))
            .expect("openrouter provider");
        assert_eq!(
            openrouter["capabilities"]["capabilities"]["toolCalls"]["status"],
            json!("supported")
        );

        let external = providers
            .iter()
            .find(|provider| provider["providerId"] == json!("external_codex_cli"))
            .expect("external codex provider");
        assert_eq!(
            external["capabilities"]["catalogKind"],
            json!("external_agent_adapter")
        );
        assert_eq!(
            external["capabilities"]["capabilities"]["toolCalls"]["status"],
            json!("not_applicable")
        );
    }

    #[test]
    fn provider_doctor_reports_shared_capability_snapshot() {
        let state_dir = unique_temp_dir("provider-doctor-capabilities");
        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "doctor",
            "external_codex_cli",
        ])
        .expect("provider doctor should succeed");

        assert_eq!(
            output.json["capabilities"]["catalogKind"],
            json!("external_agent_adapter")
        );
        assert_eq!(
            output.json["capabilities"]["requestPreview"]["route"],
            json!("external-agent-cli")
        );
        assert!(output.json["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .any(|check| check["code"] == json!("provider_external_agent_cataloged")));
    }

    #[test]
    fn provider_preflight_force_runs_live_probe_and_cache_becomes_cached_probe() {
        let state_dir = unique_temp_dir("provider-preflight-static");
        let server = MockOpenAiCompatibleServer::start(Vec::new());
        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            server.base_url.as_str(),
        ])
        .expect("provider profile should be recorded");

        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "preflight",
            "openai_api",
            "--model",
            "test-model",
            "--force",
        ])
        .expect("provider preflight should succeed");
        server.join();

        assert_eq!(output.json["snapshot"]["source"], json!("live_probe"));
        assert_eq!(output.json["snapshot"]["modelId"], json!("test-model"));
        assert!(output.json["snapshot"]["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .any(
                |check| check["code"] == json!("provider_preflight_tool_schema")
                    && check["status"] == json!("passed")
            ));

        let cached = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "preflight",
            "openai_api",
            "--model",
            "test-model",
        ])
        .expect("cached provider preflight should load");
        assert_eq!(cached.json["snapshot"]["source"], json!("cached_probe"));
        assert_eq!(
            cached.json["snapshot"]["modelId"],
            output.json["snapshot"]["modelId"]
        );
    }

    #[test]
    fn live_provider_preflight_probe() {
        let state_dir = unique_temp_dir("provider-preflight-live");
        let server = MockLiveProbeServer::start(json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "probe ok"
                }
            }]
        }));
        run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "login",
            "openai_api",
            "--base-url",
            server.base_url.as_str(),
        ])
        .expect("provider profile should be recorded");

        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "provider",
            "preflight",
            "openai_api",
            "--model",
            "test-model",
            "--force",
        ])
        .expect("provider preflight should succeed");
        let request_count = server.finish();

        assert_eq!(output.json["snapshot"]["source"], json!("live_probe"));
        assert_eq!(
            output.json["snapshot"]["checks"]
                .as_array()
                .expect("checks")
                .iter()
                .find(|check| check["code"] == json!("provider_preflight_tool_schema"))
                .expect("tool schema check")["status"],
            json!("passed")
        );
        assert!(
            request_count >= 1,
            "preflight must call the mock provider at least once"
        );
    }

    #[test]
    fn tool_pack_list_uses_shared_manifests() {
        let state_dir = unique_temp_dir("tool-pack-list");
        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "tool-pack",
            "list",
        ])
        .expect("tool-pack list should succeed");

        assert_eq!(output.json["kind"], json!("toolPackList"));
        let packs = output.json["packs"].as_array().expect("tool packs");
        assert!(packs.iter().any(|pack| pack["packId"] == json!("browser")));
        assert!(packs.iter().any(|pack| pack["packId"] == json!("solana")));
        let browser = packs
            .iter()
            .find(|pack| pack["packId"] == json!("browser"))
            .expect("browser pack");
        assert_eq!(
            browser["policyProfile"],
            json!("browser_control_with_observe_split")
        );
        assert!(browser["cliCommands"]
            .as_array()
            .expect("cli commands")
            .contains(&json!("xero tool-pack doctor browser")));
    }

    #[test]
    fn tool_pack_doctor_reports_missing_browser_prerequisites() {
        let state_dir = unique_temp_dir("tool-pack-doctor");
        let output = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "tool-pack",
            "doctor",
            "browser",
        ])
        .expect("tool-pack doctor should succeed");

        assert_eq!(output.json["kind"], json!("toolPackDoctor"));
        let reports = output.json["reports"].as_array().expect("reports");
        let report = reports.first().expect("browser report");
        assert_eq!(report["packId"], json!("browser"));
        assert_eq!(report["status"], json!("failed"));
        assert!(report["missingPrerequisites"]
            .as_array()
            .expect("missing prerequisites")
            .contains(&json!("desktop_browser_executor")));
        assert!(report["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .any(|check| check["diagnostic"]["code"]
                == json!("tool_pack_desktop_browser_executor_missing")));
    }

    #[cfg(unix)]
    #[test]
    fn external_agent_host_requires_approval_and_records_provenance() {
        let state_dir = unique_temp_dir("external-agent");
        let denied = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "host",
            "Hosted prompt.",
            "--adapter",
            "custom",
            "--command",
            "/bin/sh",
            "--arg",
            "-c",
            "--arg",
            "printf 'external-output\\n'",
        ])
        .expect_err("external subprocess should require approval");
        assert_eq!(denied.code, "xero_cli_external_agent_approval_required");

        let hosted = run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent",
            "host",
            "Hosted prompt.",
            "--adapter",
            "custom",
            "--command",
            "/bin/sh",
            "--arg",
            "-c",
            "--arg",
            "printf 'external-output\\n'",
            "--allow-subprocess",
            "--project-id",
            "project-external",
            "--session-id",
            "session-external",
            "--run-id",
            "run-external",
        ])
        .expect("approved external host should run");

        assert_eq!(hosted.json["snapshot"]["status"], json!("completed"));
        assert_eq!(
            hosted.json["snapshot"]["providerId"],
            json!("external_custom_agent")
        );
        assert!(hosted.json["snapshot"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| event["eventKind"] == json!("policy_decision")
                && event["payload"]["approvalSource"] == json!("operator_flag")));
        assert!(hosted.json["snapshot"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| event["payload"]["provenance"]["kind"] == json!("external_agent")));
        assert!(hosted.json["snapshot"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| event["eventKind"] == json!("run_started")
                && event["payload"]["sandbox"]["enforcement"] == json!("os_sandbox_runner")));
        assert!(hosted.json["snapshot"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| event["eventKind"] == json!("run_completed")
                && event["payload"]["sandbox"]["metadata"]["exitClassification"]
                    == json!("success")));
        assert!(hosted.json["snapshot"]["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .any(|message| message["role"] == json!("assistant")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("external-output"))));
    }

    fn seed_registered_project(state_dir: &Path, project_id: &str, repo_root: &Path) {
        let repo_root = fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
        fs::create_dir_all(state_dir).expect("create app-data root");
        let now = now_timestamp();
        let global_database = state_dir.join(GLOBAL_DATABASE_FILE);
        let global = Connection::open(&global_database).expect("open global registry");
        global
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS repositories (
                    id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    root_path TEXT NOT NULL UNIQUE,
                    display_name TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                "#,
            )
            .expect("create global registry schema");
        global
            .execute(
                "INSERT INTO projects (id, name, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at",
                params![project_id, project_id, now],
            )
            .expect("seed global project");
        global
            .execute(
                "INSERT INTO repositories (id, project_id, root_path, display_name, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(root_path) DO UPDATE SET
                    project_id = excluded.project_id,
                    display_name = excluded.display_name,
                    updated_at = excluded.updated_at",
                params![
                    format!("repo-{project_id}"),
                    project_id,
                    repo_root.display().to_string(),
                    repo_root
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(project_id),
                    now_timestamp(),
                ],
            )
            .expect("seed global repository");

        let project_database = workspace_project_database_path_for_app_root(state_dir, project_id);
        fs::create_dir_all(project_database.parent().expect("project database parent"))
            .expect("create project state directory");
        let project = Connection::open(&project_database).expect("open project database");
        project
            .execute_batch(APP_DATA_PROJECT_TEST_SCHEMA)
            .expect("create app-data project schema");
        project
            .execute(
                "INSERT INTO projects (id, name, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at",
                params![project_id, project_id, now_timestamp()],
            )
            .expect("seed project row");
    }

    fn seed_global_project_registration_only(state_dir: &Path, project_id: &str, repo_root: &Path) {
        let repo_root = fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
        fs::create_dir_all(state_dir).expect("create app-data root");
        let global =
            Connection::open(state_dir.join(GLOBAL_DATABASE_FILE)).expect("open global registry");
        global
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS repositories (
                    id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    root_path TEXT NOT NULL UNIQUE,
                    display_name TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                "#,
            )
            .expect("create global registry schema");
        global
            .execute(
                "INSERT INTO projects (id, name, updated_at) VALUES (?1, ?2, ?3)",
                params![project_id, project_id, now_timestamp()],
            )
            .expect("seed global-only project");
        global
            .execute(
                "INSERT INTO repositories (id, project_id, root_path, display_name, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    format!("repo-{project_id}"),
                    project_id,
                    repo_root.display().to_string(),
                    project_id,
                    now_timestamp(),
                ],
            )
            .expect("seed global-only repository");
    }

    fn seed_desktop_style_app_data_run(
        state_dir: &Path,
        project_id: &str,
        repo_root: &Path,
        agent_session_id: &str,
        run_id: &str,
    ) -> String {
        seed_registered_project(state_dir, project_id, repo_root);
        let project_database = workspace_project_database_path_for_app_root(state_dir, project_id);
        let connection = Connection::open(&project_database).expect("open project database");
        let trace_id = xero_agent_core::runtime_trace_id_for_run(project_id, run_id);
        let context_hash = "a".repeat(64);
        let now = now_timestamp();
        connection
            .execute(
                r#"
                INSERT INTO agent_sessions (
                    project_id,
                    agent_session_id,
                    title,
                    summary,
                    status,
                    selected,
                    updated_at,
                    last_run_id,
                    last_runtime_kind,
                    last_provider_id
                )
                VALUES (?1, ?2, 'Desktop Session', '', 'active', 1, ?3, ?4, 'owned_agent', 'openai_api')
                "#,
                params![project_id, agent_session_id, now, run_id],
            )
            .expect("seed desktop-style session");
        connection
            .execute(
                r#"
                INSERT INTO agent_runs (
                    runtime_agent_id,
                    agent_definition_id,
                    agent_definition_version,
                    project_id,
                    agent_session_id,
                    run_id,
                    trace_id,
                    provider_id,
                    model_id,
                    status,
                    prompt,
                    system_prompt,
                    started_at,
                    last_heartbeat_at,
                    completed_at,
                    updated_at
                )
                VALUES ('engineer', 'engineer', 1, ?1, ?2, ?3, ?4, 'openai_api', 'test-model', 'completed', 'Desktop prompt', 'Desktop system prompt', ?5, ?5, ?5, ?5)
                "#,
                params![project_id, agent_session_id, run_id, trace_id, now],
            )
            .expect("seed desktop-style run");
        connection
            .execute(
                "INSERT INTO agent_messages (project_id, run_id, role, content, created_at)
                 VALUES (?1, ?2, 'user', 'Desktop prompt', ?3)",
                params![project_id, run_id, now_timestamp()],
            )
            .expect("seed user message");
        connection
            .execute(
                "INSERT INTO agent_messages (project_id, run_id, role, content, created_at)
                 VALUES (?1, ?2, 'assistant', 'Desktop assistant response.', ?3)",
                params![project_id, run_id, now_timestamp()],
            )
            .expect("seed assistant message");
        connection
            .execute(
                "INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at)
                 VALUES (?1, ?2, 'run_started', ?3, ?4)",
                params![
                    project_id,
                    run_id,
                    r#"{"status":"starting","source":"desktop"}"#,
                    now_timestamp(),
                ],
            )
            .expect("seed run started event");
        connection
            .execute(
                r#"
                INSERT INTO agent_context_manifests (
                    manifest_id,
                    project_id,
                    agent_session_id,
                    run_id,
                    runtime_agent_id,
                    agent_definition_id,
                    agent_definition_version,
                    provider_id,
                    model_id,
                    request_kind,
                    policy_action,
                    policy_reason_code,
                    budget_tokens,
                    estimated_tokens,
                    pressure,
                    context_hash,
                    included_contributors_json,
                    excluded_contributors_json,
                    retrieval_query_ids_json,
                    retrieval_result_ids_json,
                    redaction_state,
                    manifest_json,
                    created_at
                )
                VALUES (?1, ?2, ?3, ?4, 'engineer', 'engineer', 1, 'openai_api', 'test-model', 'provider_turn', 'continue_now', 'desktop_test_seed', NULL, 12, 'low', ?5, '[]', '[]', '[]', '[]', 'clean', ?6, ?7)
                "#,
                params![
                    "manifest-desktop-run-0",
                    project_id,
                    agent_session_id,
                    run_id,
                    context_hash,
                    r#"{"kind":"provider_context_package","turnIndex":0,"source":"desktop"}"#,
                    now_timestamp(),
                ],
            )
            .expect("seed context manifest");
        connection
            .execute(
                "INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at)
                 VALUES (?1, ?2, 'context_manifest_recorded', ?3, ?4)",
                params![
                    project_id,
                    run_id,
                    r#"{"manifestId":"manifest-desktop-run-0","turnIndex":0}"#,
                    now_timestamp(),
                ],
            )
            .expect("seed context manifest event");
        connection
            .execute(
                "INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at)
                 VALUES (?1, ?2, 'run_completed', ?3, ?4)",
                params![
                    project_id,
                    run_id,
                    r#"{"summary":"Desktop run completed."}"#,
                    now_timestamp(),
                ],
            )
            .expect("seed run completed event");
        trace_id
    }

    const APP_DATA_PROJECT_TEST_SCHEMA: &str = r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS agent_definitions (
            definition_id TEXT PRIMARY KEY,
            current_version INTEGER NOT NULL,
            display_name TEXT NOT NULL,
            short_label TEXT NOT NULL,
            scope TEXT NOT NULL,
            lifecycle_state TEXT NOT NULL,
            base_capability_profile TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS agent_definition_versions (
            definition_id TEXT NOT NULL,
            version INTEGER NOT NULL,
            snapshot_json TEXT NOT NULL,
            validation_report_json TEXT,
            created_at TEXT NOT NULL,
            PRIMARY KEY (definition_id, version),
            FOREIGN KEY (definition_id) REFERENCES agent_definitions(definition_id)
        );
        INSERT OR IGNORE INTO agent_definitions (
            definition_id,
            current_version,
            display_name,
            short_label,
            scope,
            lifecycle_state,
            base_capability_profile,
            updated_at
        )
        VALUES ('engineer', 1, 'Engineer', 'Build', 'built_in', 'active', 'engineering', '2026-05-01T00:00:00Z');
        INSERT OR IGNORE INTO agent_definition_versions (
            definition_id,
            version,
            snapshot_json,
            validation_report_json,
            created_at
        )
        VALUES ('engineer', 1, '{"id":"engineer","version":1}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z');
        CREATE TABLE IF NOT EXISTS agent_sessions (
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            agent_session_id TEXT NOT NULL,
            title TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL,
            selected INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            archived_at TEXT,
            last_run_id TEXT,
            last_runtime_kind TEXT,
            last_provider_id TEXT,
            PRIMARY KEY (project_id, agent_session_id)
        );
        CREATE TABLE IF NOT EXISTS agent_runs (
            runtime_agent_id TEXT NOT NULL,
            agent_definition_id TEXT NOT NULL,
            agent_definition_version INTEGER NOT NULL,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            agent_session_id TEXT NOT NULL,
            run_id TEXT NOT NULL,
            trace_id TEXT NOT NULL,
            lineage_kind TEXT NOT NULL DEFAULT 'top_level',
            parent_run_id TEXT,
            parent_trace_id TEXT,
            parent_subagent_id TEXT,
            subagent_role TEXT,
            provider_id TEXT NOT NULL,
            model_id TEXT NOT NULL,
            status TEXT NOT NULL,
            prompt TEXT NOT NULL,
            system_prompt TEXT NOT NULL,
            started_at TEXT NOT NULL,
            last_heartbeat_at TEXT,
            completed_at TEXT,
            cancelled_at TEXT,
            last_error_code TEXT,
            last_error_message TEXT,
            updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            PRIMARY KEY (project_id, run_id),
            FOREIGN KEY (project_id, agent_session_id) REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
            FOREIGN KEY (agent_definition_id, agent_definition_version) REFERENCES agent_definition_versions(definition_id, version)
        );
        CREATE TABLE IF NOT EXISTS agent_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id TEXT NOT NULL,
            run_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            provider_metadata_json TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (project_id, run_id) REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS agent_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id TEXT NOT NULL,
            run_id TEXT NOT NULL,
            event_kind TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (project_id, run_id) REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS agent_context_manifests (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            manifest_id TEXT NOT NULL UNIQUE,
            project_id TEXT NOT NULL,
            agent_session_id TEXT NOT NULL,
            run_id TEXT,
            runtime_agent_id TEXT NOT NULL,
            agent_definition_id TEXT NOT NULL,
            agent_definition_version INTEGER NOT NULL,
            provider_id TEXT,
            model_id TEXT,
            request_kind TEXT NOT NULL,
            policy_action TEXT NOT NULL,
            policy_reason_code TEXT NOT NULL,
            budget_tokens INTEGER,
            estimated_tokens INTEGER NOT NULL DEFAULT 0,
            pressure TEXT NOT NULL,
            context_hash TEXT NOT NULL,
            included_contributors_json TEXT NOT NULL,
            excluded_contributors_json TEXT NOT NULL,
            retrieval_query_ids_json TEXT NOT NULL,
            retrieval_result_ids_json TEXT NOT NULL,
            compaction_id TEXT,
            handoff_id TEXT,
            redaction_state TEXT NOT NULL,
            manifest_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (project_id, agent_session_id) REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
            FOREIGN KEY (project_id, run_id) REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
            FOREIGN KEY (agent_definition_id, agent_definition_version) REFERENCES agent_definition_versions(definition_id, version)
        );
    "#;

    struct MockOpenAiCompatibleServer {
        base_url: String,
        handle: std::thread::JoinHandle<()>,
    }

    impl MockOpenAiCompatibleServer {
        fn start(mut responses: Vec<JsonValue>) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
            let address = listener.local_addr().expect("mock address");
            responses.insert(0, mock_preflight_response());
            let handle = std::thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) = listener.accept().expect("accept provider request");
                    read_http_request(&mut stream);
                    let body = serde_json::to_string(&response).expect("serialize response");
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    use std::io::Write as _;
                    stream
                        .write_all(reply.as_bytes())
                        .expect("write provider response");
                }
            });
            Self {
                base_url: format!("http://{address}/v1"),
                handle,
            }
        }

        fn join(self) {
            self.handle.join().expect("mock provider thread");
        }
    }

    fn mock_preflight_response() -> JsonValue {
        json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "preflight ok"
                }
            }]
        })
    }

    struct MockLiveProbeServer {
        base_url: String,
        request_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        handle: std::thread::JoinHandle<()>,
    }

    impl MockLiveProbeServer {
        fn start(response: JsonValue) -> Self {
            let listener =
                std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock probe provider");
            listener
                .set_nonblocking(true)
                .expect("configure mock probe listener");
            let address = listener.local_addr().expect("mock probe address");
            let request_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let request_count_for_thread = std::sync::Arc::clone(&request_count);
            let handle = std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
                while std::time::Instant::now() < deadline {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            request_count_for_thread
                                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            read_http_request(&mut stream);
                            let body =
                                serde_json::to_string(&response).expect("serialize probe response");
                            let reply = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            use std::io::Write as _;
                            stream
                                .write_all(reply.as_bytes())
                                .expect("write probe response");
                            return;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(error) => panic!("mock probe accept failed: {error}"),
                    }
                }
            });
            Self {
                base_url: format!("http://{address}/v1"),
                request_count,
                handle,
            }
        }

        fn finish(self) -> usize {
            self.handle.join().expect("mock probe provider thread");
            self.request_count.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    fn read_http_request(stream: &mut std::net::TcpStream) {
        use std::io::Read as _;
        let mut buffer = [0_u8; 8192];
        let mut request = Vec::new();
        loop {
            let read = stream.read(&mut buffer).expect("read request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
    }

    #[test]
    fn benchmark_terminal_bench_fake_fixture_writes_trial_artifacts_in_app_data() {
        let workspace = unique_temp_dir("benchmark-workspace");
        fs::write(workspace.join("README.md"), "benchmark workspace\n").expect("write workspace");
        let state_dir = unique_temp_dir("benchmark-app-data");
        let output_dir = unique_temp_dir("benchmark-output").join("trial-1");

        let response = run_with_args([
            "xero",
            "--json",
            "benchmark",
            "terminal-bench",
            "--instruction",
            "Summarize the benchmark workspace.",
            "--workspace-root",
            workspace.to_str().expect("workspace"),
            "--trial-app-data-root",
            state_dir.to_str().expect("state dir"),
            "--output-dir",
            output_dir.to_str().expect("output dir"),
            "--project-id",
            "benchmark-project",
            "--session-id",
            "benchmark-session",
            "--run-id",
            "benchmark-run",
            "--task-id",
            "adapter-smoke-task",
            "--dataset-id",
            "terminal-bench@2.0",
            "--provider",
            "fake_provider",
            "--model",
            "fake-model",
            "--allow-fake-provider-fixture",
        ])
        .expect("benchmark fake fixture should complete");

        assert_eq!(response.json["kind"], json!("benchmarkTerminalBench"));
        assert_eq!(response.json["status"], json!("completed"));
        assert!(output_dir.join("manifest.json").is_file());
        assert!(output_dir.join("trajectory.json").is_file());
        assert!(output_dir.join("xero-trace.json").is_file());
        assert!(output_dir.join("final.diff").is_file());
        assert!(output_dir.join("support-bundle.zip").is_file());
        assert!(state_dir.join(GLOBAL_DATABASE_FILE).is_file());
        assert!(
            workspace_project_database_path_for_app_root(&state_dir, "benchmark-project").is_file()
        );

        let manifest: JsonValue =
            read_json_file(&output_dir.join("manifest.json")).expect("read manifest");
        assert_eq!(manifest["run"]["status"], json!("completed"));
        assert_eq!(manifest["harness"]["fakeProviderFixture"], json!(true));
        assert_eq!(manifest["benchmark"]["taskId"], json!("adapter-smoke-task"));
        assert!(!manifest.to_string().contains(".xero"));
    }

    #[test]
    fn benchmark_terminal_bench_rejects_unlabeled_fake_provider() {
        let workspace = unique_temp_dir("benchmark-workspace-unlabeled");
        let state_dir = unique_temp_dir("benchmark-app-data-unlabeled");
        let output_dir = unique_temp_dir("benchmark-output-unlabeled");
        let error = run_with_args([
            "xero",
            "benchmark",
            "terminal-bench",
            "--instruction",
            "Fixture run.",
            "--workspace-root",
            workspace.to_str().expect("workspace"),
            "--trial-app-data-root",
            state_dir.to_str().expect("state dir"),
            "--output-dir",
            output_dir.to_str().expect("output dir"),
            "--task-id",
            "adapter-smoke-task",
            "--dataset-id",
            "terminal-bench@2.0",
            "--provider",
            "fake_provider",
        ])
        .expect_err("fake provider must be explicitly labeled");

        assert_eq!(error.code, "xero_cli_usage");
        assert!(error.message.contains("fixture-only"));
    }

    #[test]
    fn benchmark_terminal_bench_rejects_legacy_xero_state() {
        let workspace = unique_temp_dir("benchmark-workspace-legacy");
        let legacy_state = workspace.join(".xero").join("benchmark-state");
        let output_dir = unique_temp_dir("benchmark-output-legacy");
        let error = run_with_args([
            "xero",
            "benchmark",
            "terminal-bench",
            "--instruction",
            "Fixture run.",
            "--workspace-root",
            workspace.to_str().expect("workspace"),
            "--trial-app-data-root",
            legacy_state.to_str().expect("legacy state"),
            "--output-dir",
            output_dir.to_str().expect("output dir"),
            "--task-id",
            "adapter-smoke-task",
            "--dataset-id",
            "terminal-bench@2.0",
            "--provider",
            "fake_provider",
            "--allow-fake-provider-fixture",
        ])
        .expect_err("legacy .xero state must be rejected");

        assert_eq!(error.code, "xero_benchmark_legacy_state_rejected");
        assert_eq!(error.exit_code, 2);
    }

    #[test]
    fn benchmark_terminal_bench_enforces_command_call_limit_before_dispatch() {
        let workspace = unique_temp_dir("benchmark-workspace-command-limit");
        let state_dir = unique_temp_dir("benchmark-app-data-command-limit");
        let output_dir = unique_temp_dir("benchmark-output-command-limit").join("trial-1");
        let first_command = serde_json::to_string(&json!({
            "argv": ["sh", "-c", "touch should-not-run-a"]
        }))
        .expect("first command");
        let second_command = serde_json::to_string(&json!({
            "argv": ["sh", "-c", "touch should-not-run-b"]
        }))
        .expect("second command");
        let server = MockOpenAiCompatibleServer::start(vec![json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call-command-a",
                        "type": "function",
                        "function": {
                            "name": "command",
                            "arguments": first_command
                        }
                    }, {
                        "id": "call-command-b",
                        "type": "function",
                        "function": {
                            "name": "command",
                            "arguments": second_command
                        }
                    }]
                }
            }]
        })]);

        let error = run_with_args([
            "xero",
            "benchmark",
            "terminal-bench",
            "--instruction",
            "Try too many commands.",
            "--workspace-root",
            workspace.to_str().expect("workspace"),
            "--trial-app-data-root",
            state_dir.to_str().expect("state dir"),
            "--output-dir",
            output_dir.to_str().expect("output dir"),
            "--project-id",
            "benchmark-project",
            "--session-id",
            "benchmark-session",
            "--run-id",
            "benchmark-run",
            "--task-id",
            "command-limit-task",
            "--dataset-id",
            "terminal-bench@2.0",
            "--provider",
            "openai_api",
            "--model",
            "test-model",
            "--base-url",
            server.base_url.as_str(),
            "--max-command-calls",
            "1",
        ])
        .expect_err("command call limit should fail the benchmark run");
        drop(server);

        assert_eq!(
            error.code,
            "agent_core_headless_command_call_limit_exceeded"
        );
        assert!(!workspace.join("should-not-run-a").exists());
        assert!(!workspace.join("should-not-run-b").exists());
        let manifest: JsonValue =
            read_json_file(&output_dir.join("manifest.json")).expect("read manifest");
        assert_eq!(manifest["run"]["status"], json!("agent_failure"));
        assert_eq!(
            manifest["run"]["failureCategory"],
            json!("timeout_or_limit")
        );
        assert_eq!(
            manifest["limits"]["enforcement"]["maxCommandCalls"],
            json!("xero_runtime")
        );
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = env::temp_dir().join(format!("xero-cli-{label}-{suffix}"));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }
}
