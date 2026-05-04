use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use xero_agent_core::{
    domain_tool_pack_health_report, domain_tool_pack_manifest, domain_tool_pack_manifests,
    provider_capability_catalog, AgentCoreStore, AgentRuntimeFacade, DomainToolPackHealthInput,
    DomainToolPackHealthReport, DomainToolPackHealthStatus, DomainToolPackManifest,
    FakeProviderRuntime, FileAgentCoreStore, MessageRole, NewMessageRecord, NewRunRecord,
    NewRuntimeEvent, PermissionProfileSandbox, ProjectTrustState, ProviderCapabilityCatalog,
    ProviderCapabilityCatalogInput, ProviderSelection, RunControls, RunSnapshot, RunStatus,
    RuntimeEventKind, RuntimeTraceContext, SandboxApprovalSource, SandboxExecutionContext,
    SandboxPlatform, StartRunRequest, ToolApprovalRequirement, ToolCallInput, ToolDescriptorV2,
    ToolEffectClass, ToolExecutionContext, ToolMutability, ToolSandbox, ToolSandboxRequirement,
    DEFAULT_PROVIDER_CATALOG_TTL_SECONDS, DOMAIN_TOOL_PACK_CONTRACT_VERSION,
};

const APP_DATA_DIRECTORY_NAME: &str = "dev.sn0w.xero";
const HEADLESS_DIRECTORY_NAME: &str = "headless";
const AGENT_CORE_STATE_FILE: &str = "agent-core-runs.json";
const CLI_CONFIG_FILE: &str = "cli-config.json";
const WORKSPACE_INDEX_DIRECTORY: &str = "workspace-indexes";
const DEFAULT_PROVIDER_ID: &str = "fake_provider";
const DEFAULT_MODEL_ID: &str = "fake-model";
const DEFAULT_PROJECT_ID: &str = "headless-local";
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MCP_SERVER_NAME: &str = "xero-local-harness";
const DEFAULT_INDEX_FILE_LIMIT: usize = 1_000;
const MAX_INDEX_FILE_BYTES: u64 = 512 * 1024;
const MAX_QUERY_RESULTS: usize = 20;

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
        Some("agent") => dispatch_agent(globals, args[1..].to_vec()),
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
        Some(other) => Err(CliError::usage(format!(
            "Unknown provider command `{other}`."
        ))),
        None => Err(CliError::usage(
            "Missing provider command. Use list, login, or doctor.",
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
        Some("query") => command_workspace_query(globals, args[1..].to_vec()),
        Some(other) => Err(CliError::usage(format!(
            "Unknown workspace command `{other}`."
        ))),
        None => Err(CliError::usage(
            "Missing workspace command. Use index or query.",
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
            "Usage: xero agent exec [PROMPT] [--project-id ID] [--session-id ID] [--run-id ID] [--provider ID] [--model ID]\nRuns a headless owned-agent turn through xero-agent-core.",
            json!({ "command": "agent exec" }),
        ));
    }

    let prompt_flag = take_option(&mut args, "--prompt")?;
    let project_id = take_option(&mut args, "--project-id")?.unwrap_or_else(|| {
        env::current_dir()
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_PROJECT_ID.into())
    });
    let agent_session_id =
        take_option(&mut args, "--session-id")?.unwrap_or_else(|| generate_id("session"));
    let run_id = take_option(&mut args, "--run-id")?.unwrap_or_else(|| generate_id("run"));
    let provider_id =
        take_option(&mut args, "--provider")?.unwrap_or_else(|| DEFAULT_PROVIDER_ID.into());
    let model_id = take_option(&mut args, "--model")?.unwrap_or_else(|| DEFAULT_MODEL_ID.into());
    reject_unknown_options(&args)?;

    ensure_owned_agent_provider(&provider_id)?;
    if globals.ci && provider_id != DEFAULT_PROVIDER_ID {
        return Err(CliError::user_fixable(
            "xero_cli_provider_unavailable_in_ci",
            "Headless CI mode currently supports the built-in fake provider only, so provider execution stays non-interactive and sandbox-strict.",
        ));
    }

    let prompt = prompt_flag
        .or_else(|| (!args.is_empty()).then(|| args.join(" ")))
        .ok_or_else(|| CliError::usage("Missing prompt. Use `xero agent exec \"...\"`."))?;
    if prompt.trim().is_empty() {
        return Err(CliError::usage("Prompt cannot be empty."));
    }

    let store = open_agent_store(&globals)?;
    let runtime = FakeProviderRuntime::new(store.clone());
    let snapshot = runtime
        .start_run(StartRunRequest {
            project_id: project_id.clone(),
            agent_session_id,
            run_id,
            prompt: prompt.clone(),
            provider: ProviderSelection {
                provider_id,
                model_id,
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
            "ciMode": globals.ci,
            "sandboxDefaults": sandbox_defaults_json(globals.ci),
            "storePath": store.path(),
            "snapshot": snapshot,
        }),
    ))
}

fn command_agent_host(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero agent host [PROMPT] --adapter codex|claude|gemini|custom [--command CMD] [--arg ARG]... --allow-subprocess\nLaunches an explicitly-approved external CLI agent as an auditable Xero conversation.",
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
    let store = open_agent_store(&globals)?;
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
    let store = open_agent_store(&globals)?;
    let runs = match project_filter.as_deref() {
        Some(project_id) => store.list_project_runs(project_id),
        None => store.list_runs(),
    }
    .map_err(core_error)?;

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
        json!({ "kind": "conversationList", "storePath": store.path(), "runs": runs }),
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
    let (_store, snapshot) = load_conversation_from_args(&globals, args)?;
    let store = open_agent_store(&globals)?;
    let runtime = FakeProviderRuntime::new(store.clone());
    let compacted = runtime
        .continue_run(xero_agent_core::ContinueRunRequest {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            prompt: "Compact this headless conversation and retain the durable run summary.".into(),
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
    let (_store, snapshot) = load_conversation_from_args(&globals, args)?;
    let store = open_agent_store(&globals)?;
    let runtime = FakeProviderRuntime::new(store.clone());
    let retried = runtime
        .start_run(StartRunRequest {
            project_id: snapshot.project_id.clone(),
            agent_session_id: snapshot.agent_session_id.clone(),
            run_id: generate_id("run-retry"),
            prompt: snapshot.prompt.clone(),
            provider: ProviderSelection {
                provider_id: snapshot.provider_id.clone(),
                model_id: snapshot.model_id.clone(),
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
        json!({ "kind": "conversationRetry", "sourceRunId": snapshot.run_id, "snapshot": retried }),
    ))
}

fn command_conversation_clone(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let (_store, snapshot) = load_conversation_from_args(&globals, args)?;
    let store = open_agent_store(&globals)?;
    let runtime = FakeProviderRuntime::new(store.clone());
    let cloned = runtime
        .start_run(StartRunRequest {
            project_id: snapshot.project_id.clone(),
            agent_session_id: generate_id("session-clone"),
            run_id: generate_id("run-clone"),
            prompt: snapshot.prompt.clone(),
            provider: ProviderSelection {
                provider_id: snapshot.provider_id.clone(),
                model_id: snapshot.model_id.clone(),
            },
            controls: Some(RunControls {
                runtime_agent_id: "engineer".into(),
                approval_mode: if globals.ci { "strict" } else { "on_request" }.into(),
                plan_mode_required: globals.ci,
            }),
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
        .unwrap_or_else(|| DEFAULT_PROVIDER_ID.into());
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
        "workspace_index_store" => globals.state_dir.join(WORKSPACE_INDEX_DIRECTORY).is_dir(),
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
    let repo = take_option(&mut args, "--repo")?.unwrap_or_else(|| ".".into());
    let project_id =
        take_option(&mut args, "--project-id")?.unwrap_or_else(|| DEFAULT_PROJECT_ID.into());
    let limit = take_option(&mut args, "--limit")?
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_INDEX_FILE_LIMIT);
    reject_unknown_options(&args)?;
    let repo_root = canonicalize_existing_path(&repo)?;
    let index = build_workspace_index(&repo_root, &project_id, limit)?;
    let path = workspace_index_path(&globals, &repo_root);
    write_json_file(&path, &index)?;
    let text = format!(
        "Indexed {} files for `{}` at {}.",
        index.files.len(),
        project_id,
        path.display()
    );
    Ok(response(
        &globals,
        text,
        json!({ "kind": "workspaceIndex", "path": path, "index": index }),
    ))
}

fn command_workspace_query(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let repo = take_option(&mut args, "--repo")?.unwrap_or_else(|| ".".into());
    let limit = take_option(&mut args, "--limit")?
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(8)
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
    let path = workspace_index_path(&globals, &repo_root);
    let index = read_json_file::<WorkspaceIndex>(&path)?;
    let results = query_workspace_index(&index, &query, limit);
    let text = if results.is_empty() {
        "No workspace index results matched.".into()
    } else {
        results
            .iter()
            .map(|result| format!("{:.3}  {}", result.score, result.path))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "workspaceQuery", "query": query, "indexPath": path, "results": results }),
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
) -> Result<(FileAgentCoreStore, RunSnapshot), CliError> {
    let project_id = take_option(&mut args, "--project-id")?;
    reject_unknown_options(&args)?;
    let run_id = args
        .first()
        .cloned()
        .ok_or_else(|| CliError::usage("Missing run id."))?;
    let store = open_agent_store(globals)?;
    let snapshot = match project_id {
        Some(project_id) => store.load_run(&project_id, &run_id),
        None => store.load_run_by_id(&run_id),
    }
    .map_err(core_error)?;
    Ok((store, snapshot))
}

fn open_agent_store(globals: &GlobalOptions) -> Result<FileAgentCoreStore, CliError> {
    FileAgentCoreStore::open(globals.state_dir.join(AGENT_CORE_STATE_FILE)).map_err(core_error)
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
        "  conversation list|show|dump|support-bundle|compact|retry|clone|stats",
        "  provider list|login|doctor",
        "  mcp list|add|login|serve",
        "  workspace index|query",
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
            "mcp list",
            "mcp add",
            "mcp login",
            "mcp serve",
            "workspace index",
            "workspace query",
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
        return home_dir()
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join(APP_DATA_DIRECTORY_NAME)
                    .join(HEADLESS_DIRECTORY_NAME)
            })
            .ok_or_else(home_dir_error);
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
            provider_id: DEFAULT_PROVIDER_ID,
            label: "Fake Provider",
            default_model: DEFAULT_MODEL_ID,
            credential_kind: "none",
            headless_status: "ready",
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
    let catalog_source = if entry.provider_id == DEFAULT_PROVIDER_ID {
        "live"
    } else if profile.is_some() || entry.catalog_kind == "external_agent_adapter" {
        "manual"
    } else {
        "unavailable"
    };
    let credential_proof = match (entry.credential_kind, profile) {
        ("none", _) => Some("none_required".into()),
        ("external_process", _) => Some("external_process".into()),
        (_, Some(profile)) if profile.api_key_env.is_some() => Some("api_key_env_recorded".into()),
        (_, Some(_)) => Some("profile_recorded".into()),
        _ => None,
    };
    let thinking_efforts = match entry.provider_id {
        "openai_codex" => vec!["low".into(), "medium".into(), "high".into()],
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
    };
    let thinking_default_effort = if thinking_efforts.iter().any(|effort| effort == "medium") {
        Some("medium".into())
    } else {
        None
    };

    provider_capability_catalog(ProviderCapabilityCatalogInput {
        provider_id: entry.provider_id.into(),
        model_id: entry.default_model.into(),
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
        thinking_supported: !thinking_efforts.is_empty(),
        thinking_efforts,
        thinking_default_effort,
    })
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

    if entry.provider_id != DEFAULT_PROVIDER_ID {
        return Err(CliError::user_fixable(
            "xero_cli_provider_not_executable",
            format!(
                "Provider `{provider_id}` is a model-provider catalog entry, but this headless `agent exec` path only executes `{DEFAULT_PROVIDER_ID}` until the real provider adapter is wired."
            ),
        ));
    }

    Ok(())
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
struct McpServerConfig {
    name: String,
    command: String,
    args: Vec<String>,
    env: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token_env: Option<String>,
    added_at: String,
}

#[derive(Debug, Clone, Serialize)]
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
    if provider_id == DEFAULT_PROVIDER_ID {
        return vec![ProviderDoctorCheck {
            code: "provider_fake_ready".into(),
            status: "passed".into(),
            message:
                "The built-in fake provider is ready for headless protocol and storage checks."
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
                "sandbox": sandbox_metadata,
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
        })
        .map_err(core_error)?;
    store
        .append_message(NewMessageRecord {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            role: MessageRole::User,
            content: request.prompt.clone(),
        })
        .map_err(core_error)?;
    store
        .update_run_status(&snapshot.project_id, &snapshot.run_id, RunStatus::Running)
        .map_err(core_error)?;

    let output = Command::new(&request.command)
        .args(&request.argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let run = store
        .load_run(&snapshot.project_id, &snapshot.run_id)
        .map_err(core_error)?;
    let completed_snapshot = match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
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

            if output.status.success() {
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
                            "exitCode": output.status.code(),
                            "provenance": external_agent_provenance(&request),
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
                            "exitCode": output.status.code(),
                            "provenance": external_agent_provenance(&request),
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
                        "code": "external_agent_spawn_failed",
                        "message": format!("Could not launch external agent command: {error}"),
                        "provenance": external_agent_provenance(&request),
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
) -> Result<JsonValue, CliError> {
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
        approval_source: SandboxApprovalSource::Operator,
        platform: SandboxPlatform::current(),
        explicit_git_mutation_allowed: false,
        legacy_xero_migration_allowed: false,
        preserved_environment_keys: Vec::new(),
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
    let metadata = sandbox
        .evaluate(&descriptor, &call, &ToolExecutionContext::default())
        .map_err(|denied| CliError::user_fixable(denied.error.code, denied.error.message))?;
    Ok(json!({
        "metadata": metadata,
        "enforcement": "approval_gated_preflight",
        "subprocessStdio": "piped",
        "shellExpansion": false,
    }))
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
        let provider_id =
            mcp_string_arg(arguments, "providerId").unwrap_or_else(|| DEFAULT_PROVIDER_ID.into());
        if let Err(error) = ensure_owned_agent_provider(&provider_id) {
            return mcp_tool_error(error.code, error.message);
        }
        let project_id =
            mcp_string_arg(arguments, "projectId").unwrap_or_else(|| DEFAULT_PROJECT_ID.into());
        let agent_session_id =
            mcp_string_arg(arguments, "sessionId").unwrap_or_else(|| generate_id("mcp-session"));
        let run_id = mcp_string_arg(arguments, "runId").unwrap_or_else(|| generate_id("mcp-run"));
        let model_id =
            mcp_string_arg(arguments, "modelId").unwrap_or_else(|| DEFAULT_MODEL_ID.into());

        let store = match open_agent_store(&self.globals) {
            Ok(store) => store,
            Err(error) => return mcp_tool_error(error.code, error.message),
        };
        let runtime = FakeProviderRuntime::new(store.clone());
        match runtime.start_run(StartRunRequest {
            project_id,
            agent_session_id,
            run_id,
            prompt,
            provider: ProviderSelection {
                provider_id,
                model_id,
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
                json!({ "snapshot": snapshot, "storePath": store.path() }),
            ),
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
        let store = match open_agent_store(&self.globals) {
            Ok(store) => store,
            Err(error) => return mcp_tool_error(error.code, error.message),
        };
        let snapshot = match project_id {
            Some(project_id) => store.load_run(&project_id, &run_id),
            None => store.load_run_by_id(&run_id),
        };
        match snapshot {
            Ok(snapshot) => {
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
        let limit = mcp_usize_arg(arguments, "limit")
            .unwrap_or(8)
            .min(MAX_QUERY_RESULTS);
        match query_workspace_index_for_repo(&self.globals, &repo, &query, limit) {
            Ok((index_path, results)) => mcp_tool_success(
                format!("Found {} workspace index result(s).", results.len()),
                json!({ "query": query, "indexPath": index_path, "results": results }),
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
        let store = match open_agent_store(&self.globals) {
            Ok(store) => store,
            Err(error) => return mcp_tool_error(error.code, error.message),
        };

        let snapshots = match (&project_id, &run_id) {
            (Some(project_id), Some(run_id)) => {
                match store
                    .load_run(project_id, run_id)
                    .map(|snapshot| vec![snapshot])
                {
                    Ok(snapshots) => snapshots,
                    Err(error) => return mcp_tool_error(error.code, error.message),
                }
            }
            (None, Some(run_id)) => {
                match store.load_run_by_id(run_id).map(|snapshot| vec![snapshot]) {
                    Ok(snapshots) => snapshots,
                    Err(error) => return mcp_tool_error(error.code, error.message),
                }
            }
            (Some(project_id), None) => {
                let runs = match store.list_project_runs(project_id) {
                    Ok(runs) => runs,
                    Err(error) => return mcp_tool_error(error.code, error.message),
                };
                let mut snapshots = Vec::new();
                for run in runs.into_iter().take(limit) {
                    if let Ok(snapshot) = store.load_run(&run.project_id, &run.run_id) {
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
                let store = match open_agent_store(&self.globals) {
                    Ok(store) => store,
                    Err(error) => return mcp_tool_error(error.code, error.message),
                };
                let snapshot = match mcp_string_arg(&pack_arguments, "projectId") {
                    Some(project_id) => store.load_run(&project_id, &run_id),
                    None => store.load_run_by_id(&run_id),
                };
                match snapshot {
                    Ok(snapshot) => mcp_tool_success(
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
        let store = match open_agent_store(&self.globals) {
            Ok(store) => store,
            Err(error) => return mcp_tool_error(error.code, error.message),
        };
        let snapshot = match mcp_string_arg(arguments, "projectId") {
            Some(project_id) => store.load_run(&project_id, &run_id),
            None => store.load_run_by_id(&run_id),
        };
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
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
            "description": "Start a Xero-owned headless run through the reusable harness. Use this for safe local harness checks that should persist as conversations. Do not use this for external CLI agents; those require the approval-gated `xero agent host` path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "User prompt for the run." },
                    "projectId": { "type": "string", "description": "Durable project id. Defaults to headless-local." },
                    "sessionId": { "type": "string", "description": "Optional agent session id." },
                    "runId": { "type": "string", "description": "Optional run id." },
                    "providerId": { "type": "string", "description": "Owned-agent provider id. Only fake_provider is executable in this headless harness." },
                    "modelId": { "type": "string", "description": "Model id for the owned-agent provider." }
                },
                "required": ["prompt"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
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

fn query_workspace_index_for_repo(
    globals: &GlobalOptions,
    repo: &str,
    query: &str,
    limit: usize,
) -> Result<(PathBuf, Vec<WorkspaceQueryResult>), CliError> {
    let repo_root = canonicalize_existing_path(repo)?;
    let path = workspace_index_path(globals, &repo_root);
    let index = read_json_file::<WorkspaceIndex>(&path)?;
    Ok((path, query_workspace_index(&index, query, limit)))
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceIndex {
    schema_version: u32,
    project_id: String,
    repo_root: PathBuf,
    indexed_at: String,
    files: Vec<WorkspaceIndexedFile>,
    skipped_files: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceIndexedFile {
    path: String,
    byte_length: u64,
    modified_at: Option<String>,
    terms: BTreeMap<String, u32>,
    preview: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceQueryResult {
    path: String,
    score: f32,
    preview: String,
    matched_terms: Vec<String>,
}

fn build_workspace_index(
    repo_root: &Path,
    project_id: &str,
    limit: usize,
) -> Result<WorkspaceIndex, CliError> {
    let mut files = Vec::new();
    let mut skipped_files = 0usize;
    let mut truncated = false;
    scan_workspace(
        repo_root,
        repo_root,
        limit,
        &mut files,
        &mut skipped_files,
        &mut truncated,
    )?;
    Ok(WorkspaceIndex {
        schema_version: 1,
        project_id: project_id.into(),
        repo_root: repo_root.into(),
        indexed_at: now_timestamp(),
        files,
        skipped_files,
        truncated,
    })
}

fn scan_workspace(
    repo_root: &Path,
    current: &Path,
    limit: usize,
    files: &mut Vec<WorkspaceIndexedFile>,
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
            scan_workspace(repo_root, &path, limit, files, skipped_files, truncated)?;
            if *truncated {
                return Ok(());
            }
        } else if file_type.is_file() {
            if files.len() >= limit {
                *truncated = true;
                return Ok(());
            }
            match index_file(repo_root, &path) {
                Ok(Some(indexed)) => files.push(indexed),
                Ok(None) => *skipped_files = skipped_files.saturating_add(1),
                Err(_) => *skipped_files = skipped_files.saturating_add(1),
            }
        }
    }
    Ok(())
}

fn index_file(repo_root: &Path, path: &Path) -> Result<Option<WorkspaceIndexedFile>, CliError> {
    let metadata = fs::metadata(path).map_err(|error| {
        CliError::system_fault(
            "xero_cli_workspace_file_metadata_failed",
            format!("Could not inspect `{}`: {error}", path.display()),
        )
    })?;
    if metadata.len() > MAX_INDEX_FILE_BYTES || !looks_like_source_file(path) {
        return Ok(None);
    }
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Ok(None),
    };
    let tokens = tokenize(&content);
    if tokens.is_empty() {
        return Ok(None);
    }
    let mut terms = BTreeMap::<String, u32>::new();
    for token in tokens {
        *terms.entry(token).or_default() += 1;
    }
    let relative = path
        .strip_prefix(repo_root)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().to_string());
    Ok(Some(WorkspaceIndexedFile {
        path: relative,
        byte_length: metadata.len(),
        modified_at: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| format!("{}Z", duration.as_secs())),
        terms,
        preview: content.chars().take(240).collect(),
    }))
}

fn should_skip_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            matches!(
                name,
                ".git"
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

fn looks_like_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension,
                "rs" | "ts"
                    | "tsx"
                    | "js"
                    | "jsx"
                    | "json"
                    | "toml"
                    | "md"
                    | "css"
                    | "html"
                    | "ex"
                    | "exs"
                    | "py"
                    | "go"
                    | "java"
                    | "kt"
                    | "swift"
                    | "c"
                    | "cpp"
                    | "h"
                    | "hpp"
                    | "sh"
                    | "sql"
                    | "yaml"
                    | "yml"
            )
        })
        .unwrap_or(false)
}

fn query_workspace_index(
    index: &WorkspaceIndex,
    query: &str,
    limit: usize,
) -> Vec<WorkspaceQueryResult> {
    let query_terms = tokenize(query).into_iter().collect::<BTreeSet<_>>();
    if query_terms.is_empty() {
        return Vec::new();
    }
    let mut results = index
        .files
        .iter()
        .filter_map(|file| {
            let mut score = 0f32;
            let mut matched_terms = Vec::new();
            for term in &query_terms {
                if let Some(count) = file.terms.get(term) {
                    score += (*count as f32).sqrt();
                    matched_terms.push(term.clone());
                }
            }
            if matched_terms.is_empty() {
                None
            } else {
                Some(WorkspaceQueryResult {
                    path: file.path.clone(),
                    score,
                    preview: file.preview.clone(),
                    matched_terms,
                })
            }
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.path.cmp(&right.path))
    });
    results.truncate(limit);
    results
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter_map(|part| {
            let token = part.trim().to_ascii_lowercase();
            (token.len() > 1).then_some(token)
        })
        .collect()
}

fn workspace_index_path(globals: &GlobalOptions, repo_root: &Path) -> PathBuf {
    globals
        .state_dir
        .join(WORKSPACE_INDEX_DIRECTORY)
        .join(format!(
            "{:016x}.json",
            stable_hash(&repo_root.display().to_string())
        ))
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
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
    fn agent_exec_persists_headless_run_for_conversation_show() {
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
    fn conversation_dump_and_support_bundle_use_canonical_trace_snapshot() {
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
        assert_eq!(dump.json["qualityGates"]["passed"], json!(true));

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

        assert_eq!(query.json["results"][0]["path"], json!("src/lib.rs"));
    }

    #[test]
    fn ci_mode_records_strict_sandbox_defaults() {
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
    fn mcp_server_lists_tools_and_queries_started_run() {
        let state_dir = unique_temp_dir("mcp-server");
        let globals = GlobalOptions {
            output_mode: OutputMode::Json,
            ci: false,
            state_dir,
        };
        let messages = vec![
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
            4,
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
            responses[3]["result"]["structuredContent"]["snapshot"]["runId"],
            json!("run-mcp")
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
        assert!(hosted.json["snapshot"]["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .any(|message| message["role"] == json!("assistant")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("external-output"))));
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
