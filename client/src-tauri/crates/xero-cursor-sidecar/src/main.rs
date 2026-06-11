use std::{
    collections::BTreeSet,
    env, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
};

use serde_json::{json, Value as JsonValue};
use xero_agent_core::{
    HeadlessProductionToolRuntime, ToolCallInput, ToolDescriptorV2, ToolDispatchFailure,
    ToolDispatchOutcome, ToolDispatchSuccess, ToolEffectClass, ToolErrorCategory,
    ToolExecutionError, ToolGroupDispatchReport, ToolMutability,
};

const SIDECAR_VERSION: &str = "xero-cursor-sidecar.v1";
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MCP_TOOLS_SERVER_NAME: &str = "xero-tool-registry-v2";
const TOOL_REGISTRY_MCP_RUNTIME: &str = "cursor_sdk_xero_mcp";
const DEFAULT_MCP_MODE: ToolRegistryMcpMode = ToolRegistryMcpMode::ObserveOnly;
const DEFAULT_TIMEOUT_MS: u64 = 30 * 60 * 1000;
const TOOL_NAME_WRITE: &str = "write";
const TOOL_NAME_PATCH: &str = "patch";
const TOOL_NAME_DELETE: &str = "delete";
const TOOL_NAME_MOVE: &str = "move";
const TOOL_NAME_REPLACE: &str = "replace";
const TOOL_NAME_COMMAND: &str = "command";

#[derive(Debug)]
struct SidecarError {
    code: String,
    message: String,
}

impl SidecarError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: "cursor_sidecar_usage".into(),
            message: message.into(),
        }
    }

    fn system(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SidecarError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolRegistryMcpMode {
    ObserveOnly,
    WorkspaceWrite,
    CommandEnabled,
}

impl ToolRegistryMcpMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::ObserveOnly => "observe-only",
            Self::WorkspaceWrite => "workspace-write",
            Self::CommandEnabled => "command-enabled",
        }
    }

    fn allow_writes(self) -> bool {
        matches!(self, Self::WorkspaceWrite | Self::CommandEnabled)
    }

    fn allow_commands(self) -> bool {
        matches!(self, Self::CommandEnabled)
    }
}

#[derive(Debug)]
struct RunConfig {
    repo_root: PathBuf,
    project_id: String,
    run_id: String,
    session_id: String,
    runtime_agent_id: String,
    model_id: String,
    prompt_file: PathBuf,
    attachments_json_file: Option<PathBuf>,
    api_key_file: Option<PathBuf>,
    state_dir: PathBuf,
    bridge_path: PathBuf,
    node_command: String,
    mcp_mode: ToolRegistryMcpMode,
    timeout_ms: u64,
    cursor_agent_id: Option<String>,
    local_auto_mode: Option<String>,
    fixture: Option<PathBuf>,
}

#[derive(Debug)]
struct McpConfig {
    repo_root: PathBuf,
    project_id: String,
    run_id: String,
    session_id: String,
    runtime_agent_id: String,
    state_dir: PathBuf,
    event_log: PathBuf,
    mode: ToolRegistryMcpMode,
    self_test: bool,
}

fn main() {
    if let Err(error) = run() {
        emit_stdout(json!({
            "type": "sidecar_failed",
            "code": error.code,
            "message": error.message,
            "sidecarVersion": SIDECAR_VERSION,
        }));
        let _ = writeln!(io::stderr(), "xero-cursor-sidecar: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), SidecarError> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || take_bool_flag(&mut args, "--help") {
        print_usage();
        return Ok(());
    }

    match args.remove(0).as_str() {
        "run" => run_cursor(parse_run_config(args)?),
        "mcp" => {
            if args.first().map(String::as_str) != Some("serve-tools") {
                return Err(SidecarError::usage(
                    "Usage: xero-cursor-sidecar mcp serve-tools ...",
                ));
            }
            args.remove(0);
            run_mcp(parse_mcp_config(args)?)
        }
        "self-test" => run_self_test(args),
        other => Err(SidecarError::usage(format!(
            "Unknown xero-cursor-sidecar command `{other}`."
        ))),
    }
}

fn run_cursor(config: RunConfig) -> Result<(), SidecarError> {
    if !config.repo_root.is_dir() {
        return Err(SidecarError::usage(format!(
            "Repo root `{}` does not exist.",
            config.repo_root.display()
        )));
    }
    if config.fixture.is_none() && !command_available(&config.node_command) {
        return Err(SidecarError::system(
            "cursor_node_runtime_missing",
            format!(
                "Node.js is required to run Cursor SDK sidecar bridge command `{}`.",
                config.node_command
            ),
        ));
    }
    if !config.bridge_path.is_file() {
        return Err(SidecarError::system(
            "cursor_bridge_missing",
            format!(
                "Cursor SDK bridge script `{}` was not found.",
                config.bridge_path.display()
            ),
        ));
    }

    let prompt = fs::read_to_string(&config.prompt_file).map_err(|error| {
        SidecarError::system(
            "cursor_prompt_read_failed",
            format!("Could not read `{}`: {error}", config.prompt_file.display()),
        )
    })?;
    let attachments_json = match &config.attachments_json_file {
        Some(path) => Some(fs::read_to_string(path).map_err(|error| {
            SidecarError::system(
                "cursor_attachment_payload_read_failed",
                format!("Could not read `{}`: {error}", path.display()),
            )
        })?),
        None => None,
    };
    let event_log = config
        .state_dir
        .join("cursor-sdk")
        .join("runs")
        .join(safe_path_segment(&config.run_id))
        .join("mcp-events.jsonl");
    if let Some(parent) = event_log.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            SidecarError::system(
                "cursor_sidecar_state_prepare_failed",
                format!("Could not create `{}`: {error}", parent.display()),
            )
        })?;
    }
    let _ = fs::remove_file(&event_log);

    let self_exe = env::current_exe().map_err(|error| {
        SidecarError::system(
            "cursor_sidecar_exe_unavailable",
            format!("Could not resolve sidecar executable path: {error}"),
        )
    })?;
    let mut argv = vec![
        config.node_command.clone(),
        config.bridge_path.display().to_string(),
        "--prompt".into(),
        prompt,
        "--repo-root".into(),
        config.repo_root.display().to_string(),
        "--project-id".into(),
        config.project_id.clone(),
        "--run-id".into(),
        config.run_id.clone(),
        "--session-id".into(),
        config.session_id.clone(),
        "--model".into(),
        config.model_id.clone(),
        "--xero-cli-path".into(),
        self_exe.display().to_string(),
        "--xero-state-dir".into(),
        config.state_dir.display().to_string(),
        "--mcp-mode".into(),
        config.mcp_mode.as_str().into(),
        "--mcp-sidecar-kind".into(),
        "cursor-sidecar".into(),
        "--mcp-event-log".into(),
        event_log.display().to_string(),
        "--runtime-agent-id".into(),
        config.runtime_agent_id.clone(),
    ];
    if let Some(path) = config.api_key_file.as_ref() {
        argv.push("--api-key-file".into());
        argv.push(path.display().to_string());
    }
    if let Some(cursor_agent_id) = config.cursor_agent_id.as_deref() {
        argv.push("--cursor-agent-id".into());
        argv.push(cursor_agent_id.into());
    }
    if let Some(local_auto_mode) = config.local_auto_mode.as_deref() {
        argv.push("--local-auto-mode".into());
        argv.push(local_auto_mode.into());
    }
    if let Some(fixture) = config.fixture.as_ref() {
        argv.push("--fixture".into());
        argv.push(fixture.display().to_string());
    }
    if let Some(attachments_json) = attachments_json {
        argv.push("--attachments-json".into());
        argv.push(attachments_json);
    }

    emit_stdout(json!({
        "type": "sidecar_started",
        "sidecarVersion": SIDECAR_VERSION,
        "projectId": config.project_id,
        "runId": config.run_id,
        "sessionId": config.session_id,
        "runtimeAgentId": config.runtime_agent_id,
        "model": config.model_id,
        "mcpMode": config.mcp_mode.as_str(),
        "timeoutMs": config.timeout_ms,
    }));

    let mut command = Command::new(&argv[0]);
    command
        .args(argv.iter().skip(1))
        .current_dir(&config.repo_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().map_err(|error| {
        SidecarError::system(
            "cursor_bridge_spawn_failed",
            format!("Could not start Cursor SDK bridge: {error}"),
        )
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        println!("{line}");
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        emit_stdout(json!({
            "type": "sidecar_command_output",
            "stream": "cursor_bridge_stderr",
            "text": truncate_bytes(&stderr, 64 * 1024),
        }));
    }

    if event_log.is_file() {
        let file = fs::File::open(&event_log).map_err(|error| {
            SidecarError::system(
                "cursor_mcp_event_log_read_failed",
                format!("Could not open `{}`: {error}", event_log.display()),
            )
        })?;
        for line in io::BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter(|line| !line.trim().is_empty())
        {
            println!("{line}");
        }
    }

    let exit_code = output.status.code();
    if !output.status.success() {
        emit_stdout(json!({
            "type": "sidecar_failed",
            "code": "cursor_bridge_exit_failed",
            "message": "Cursor SDK bridge exited unsuccessfully.",
            "exitCode": exit_code,
        }));
        return Ok(());
    }
    emit_stdout(json!({
        "type": "sidecar_completed",
        "exitCode": exit_code,
    }));
    Ok(())
}

fn run_mcp(config: McpConfig) -> Result<(), SidecarError> {
    if matches!(
        config.runtime_agent_id.as_str(),
        "ask" | "plan" | "crawl" | "agent_create"
    ) && config.mode != ToolRegistryMcpMode::ObserveOnly
    {
        return run_mcp(McpConfig {
            mode: ToolRegistryMcpMode::ObserveOnly,
            ..config
        });
    }

    let runtime = runtime_for_config(&config)?;
    if config.self_test {
        emit_stdout(json!({
            "type": "self_test_completed",
            "sidecarVersion": SIDECAR_VERSION,
            "mode": config.mode.as_str(),
            "toolCount": runtime.descriptors().len(),
        }));
        return Ok(());
    }

    append_event_log(
        &config.event_log,
        json!({
            "type": "agent_event",
            "eventKind": "tool_registry_snapshot",
            "payload": {
                "kind": "active_tool_registry",
                "runtime": TOOL_REGISTRY_MCP_RUNTIME,
                "providerLoop": "cursor_sdk_bridge",
                "turnIndex": 0,
                "executionRegistry": "tool_registry_v2",
                "mode": config.mode.as_str(),
                "descriptorNames": runtime.descriptors().iter().map(|descriptor| descriptor.name.clone()).collect::<Vec<_>>(),
                "descriptorsV2": runtime.descriptors(),
                "legacyMiniToolsAvailable": false,
            },
        }),
    )?;

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    run_mcp_jsonrpc_stream(config, stdin.lock(), &mut stdout)
}

fn runtime_for_config(config: &McpConfig) -> Result<HeadlessProductionToolRuntime, SidecarError> {
    HeadlessProductionToolRuntime::new_with_modes(
        Some(&config.repo_root),
        config.mode.allow_writes(),
        config.mode.allow_commands(),
        vec![config.state_dir.display().to_string()],
    )
    .map_err(|error| SidecarError::system(error.code, error.message))
}

fn run_mcp_jsonrpc_stream<R: BufRead, W: Write>(
    config: McpConfig,
    reader: R,
    writer: &mut W,
) -> Result<(), SidecarError> {
    let mut session = McpServerSession { config };
    for line in reader.lines() {
        let line = line.map_err(|error| {
            SidecarError::system("cursor_mcp_stdio_read_failed", error.to_string())
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
                json!({ "message": error.to_string() }),
            )),
        };
        if let Some(response) = response {
            let payload = serde_json::to_string(&response).map_err(|error| {
                SidecarError::system("cursor_mcp_encode_failed", error.to_string())
            })?;
            writer.write_all(payload.as_bytes()).map_err(|error| {
                SidecarError::system("cursor_mcp_stdio_write_failed", error.to_string())
            })?;
            writer.write_all(b"\n").map_err(|error| {
                SidecarError::system("cursor_mcp_stdio_write_failed", error.to_string())
            })?;
            writer.flush().map_err(|error| {
                SidecarError::system("cursor_mcp_stdio_flush_failed", error.to_string())
            })?;
        }
    }
    Ok(())
}

struct McpServerSession {
    config: McpConfig,
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
            "initialize" => Ok(mcp_tools_initialize_result(&params, self.config.mode)),
            "ping" => Ok(json!({})),
            "tools/list" => self.handle_tools_list(),
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

    fn handle_tools_list(&self) -> Result<JsonValue, JsonValue> {
        match runtime_for_config(&self.config) {
            Ok(runtime) => Ok(json!({
                "tools": runtime
                    .descriptors()
                    .into_iter()
                    .map(mcp_tool_definition_from_descriptor)
                    .collect::<Vec<_>>()
            })),
            Err(error) => Err(mcp_error(
                JsonValue::Null,
                -32000,
                "Xero Tool Registry runtime unavailable",
                json!({ "code": error.code, "message": error.message }),
            )),
        }
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

        let runtime = match runtime_for_config(&self.config) {
            Ok(runtime) => runtime,
            Err(error) => return Ok(mcp_tool_error(error.code, error.message)),
        };
        let descriptor_names = runtime
            .descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name)
            .collect::<BTreeSet<_>>();
        if !descriptor_names.contains(name) {
            let _ = append_event_log(
                &self.config.event_log,
                json!({
                    "type": "agent_event",
                    "eventKind": "policy_decision",
                    "payload": {
                        "decision": "deny",
                        "policy": "cursor_mcp_tool_not_allowed",
                        "reasonCode": "cursor_mcp_tool_not_allowed",
                        "message": format!("Tool `{name}` is not exposed in {} mode.", self.config.mode.as_str()),
                        "details": {
                            "toolName": name,
                            "mode": self.config.mode.as_str(),
                            "allowedTools": descriptor_names,
                            "runtime": TOOL_REGISTRY_MCP_RUNTIME,
                        },
                    },
                }),
            );
            return Ok(mcp_tool_error(
                "cursor_mcp_tool_not_allowed",
                format!(
                    "Tool `{name}` is not available in {} mode.",
                    self.config.mode.as_str()
                ),
            ));
        }

        let call = ToolCallInput {
            tool_call_id: mcp_tool_call_id(name, request_id, &arguments),
            tool_name: name.to_owned(),
            input: arguments,
        };
        if let Err(error) = record_tool_started(&self.config, &call) {
            return Ok(mcp_tool_error(error.code, error.message));
        }

        let report = match runtime.dispatch_batch(
            &self.config.project_id,
            &self.config.run_id,
            0,
            std::slice::from_ref(&call),
        ) {
            Ok(report) => report,
            Err(error) => return Ok(mcp_tool_error(error.code, error.message)),
        };
        if let Err(error) = persist_tool_registry_mcp_report(&self.config, &report, 0) {
            return Ok(mcp_tool_error(error.code, error.message));
        }
        let report_json = serde_json::to_value(&report).unwrap_or_else(|_| json!({}));
        if let Some((code, message)) = first_tool_dispatch_failure(&report) {
            Ok(mcp_tool_result(
                message.clone(),
                json!({
                    "ok": false,
                    "projectId": self.config.project_id,
                    "runId": self.config.run_id,
                    "sessionId": self.config.session_id,
                    "toolName": name,
                    "mode": self.config.mode.as_str(),
                    "error": { "code": code, "message": message },
                    "report": report_json,
                }),
                true,
            ))
        } else {
            Ok(mcp_tool_result(
                format!("Dispatched `{name}` through Xero Tool Registry V2."),
                json!({
                    "ok": true,
                    "projectId": self.config.project_id,
                    "runId": self.config.run_id,
                    "sessionId": self.config.session_id,
                    "toolName": name,
                    "mode": self.config.mode.as_str(),
                    "report": report_json,
                }),
                false,
            ))
        }
    }
}

fn record_tool_started(config: &McpConfig, call: &ToolCallInput) -> Result<(), SidecarError> {
    let (persisted_input, input_redacted) =
        redacted_tool_registry_mcp_input(&call.tool_name, &call.input);
    append_event_log(
        &config.event_log,
        json!({
            "type": "agent_event",
            "eventKind": "tool_started",
            "payload": {
                "toolCallId": call.tool_call_id,
                "toolName": call.tool_name,
                "turnIndex": 0,
                "runtime": TOOL_REGISTRY_MCP_RUNTIME,
                "input": persisted_input,
                "inputRedacted": input_redacted,
                "dispatch": {
                    "registryVersion": "tool_registry_v2",
                    "providerLoop": "cursor_sdk_bridge",
                    "path": "xero_cursor_sidecar_mcp",
                },
            },
        }),
    )
}

fn persist_tool_registry_mcp_report(
    config: &McpConfig,
    report: &xero_agent_core::ToolBatchDispatchReport,
    turn_index: usize,
) -> Result<(), SidecarError> {
    for group in &report.groups {
        for outcome in &group.outcomes {
            match outcome {
                ToolDispatchOutcome::Succeeded(success) => {
                    persist_tool_registry_mcp_success(config, group, success, turn_index)?
                }
                ToolDispatchOutcome::Failed(failure) => {
                    persist_tool_registry_mcp_failure(config, group, failure)?
                }
            }
        }
    }
    Ok(())
}

fn persist_tool_registry_mcp_success(
    config: &McpConfig,
    group: &ToolGroupDispatchReport,
    success: &ToolDispatchSuccess,
    turn_index: usize,
) -> Result<(), SidecarError> {
    persist_tool_registry_mcp_file_changes(config, success, turn_index)?;
    if success.tool_name == TOOL_NAME_COMMAND {
        persist_tool_registry_mcp_command_output(config, success)?;
    }
    append_event_log(
        &config.event_log,
        json!({
            "type": "agent_event",
            "eventKind": "tool_completed",
            "payload": {
                "toolCallId": success.tool_call_id,
                "toolName": success.tool_name,
                "ok": true,
                "summary": success.summary,
                "resultPreview": truncate_bytes(&success.output.to_string(), 2048),
                "result": success.output,
                "dispatch": tool_registry_mcp_success_dispatch(group, success),
            },
        }),
    )
}

fn persist_tool_registry_mcp_failure(
    config: &McpConfig,
    group: &ToolGroupDispatchReport,
    failure: &ToolDispatchFailure,
) -> Result<(), SidecarError> {
    if matches!(
        failure.error.category,
        ToolErrorCategory::PolicyDenied | ToolErrorCategory::SandboxDenied
    ) {
        append_event_log(
            &config.event_log,
            json!({
                "type": "agent_event",
                "eventKind": "policy_decision",
                "payload": {
                    "decision": "deny",
                    "policy": "tool_registry_v2",
                    "reasonCode": failure.error.code,
                    "message": failure.error.message,
                    "toolName": failure.tool_name,
                    "runtime": TOOL_REGISTRY_MCP_RUNTIME,
                },
            }),
        )?;
    }
    append_event_log(
        &config.event_log,
        json!({
            "type": "agent_event",
            "eventKind": "tool_completed",
            "payload": {
                "toolCallId": failure.tool_call_id,
                "toolName": failure.tool_name,
                "ok": false,
                "code": failure.error.code,
                "message": failure.error.message,
                "dispatch": tool_registry_mcp_failure_dispatch(group, failure),
            },
        }),
    )
}

fn persist_tool_registry_mcp_file_changes(
    config: &McpConfig,
    success: &ToolDispatchSuccess,
    turn_index: usize,
) -> Result<(), SidecarError> {
    match success.tool_name.as_str() {
        TOOL_NAME_WRITE => record_tool_registry_mcp_file_changed(
            config,
            success.output["path"].clone(),
            "write",
            turn_index,
            json!({
                "bytes": success.output["bytes"].clone(),
                "fileReservation": success.output["fileReservation"].clone(),
                "rollback": success.output["rollback"].clone(),
            }),
        ),
        TOOL_NAME_PATCH => {
            for path in success.output["changedFiles"]
                .as_array()
                .into_iter()
                .flatten()
            {
                record_tool_registry_mcp_file_changed(
                    config,
                    path.clone(),
                    "patch",
                    turn_index,
                    json!({}),
                )?;
            }
            Ok(())
        }
        TOOL_NAME_DELETE => record_tool_registry_mcp_file_changed(
            config,
            success.output["path"].clone(),
            "delete",
            turn_index,
            json!({
                "kind": success.output["kind"].clone(),
                "recursive": success.output["recursive"].clone(),
                "fileReservation": success.output["fileReservation"].clone(),
                "rollback": success.output["rollback"].clone(),
            }),
        ),
        TOOL_NAME_MOVE => {
            record_tool_registry_mcp_file_changed(
                config,
                success.output["from"].clone(),
                "move_from",
                turn_index,
                json!({
                    "to": success.output["to"].clone(),
                    "kind": success.output["kind"].clone(),
                    "fileReservation": success.output["fileReservation"].clone(),
                    "rollback": success.output["rollback"].clone(),
                }),
            )?;
            record_tool_registry_mcp_file_changed(
                config,
                success.output["to"].clone(),
                "move_to",
                turn_index,
                json!({
                    "from": success.output["from"].clone(),
                    "kind": success.output["kind"].clone(),
                    "fileReservation": success.output["fileReservation"].clone(),
                    "rollback": success.output["rollback"].clone(),
                }),
            )
        }
        TOOL_NAME_REPLACE => {
            for changed_file in success.output["changedFiles"]
                .as_array()
                .into_iter()
                .flatten()
            {
                record_tool_registry_mcp_file_changed(
                    config,
                    changed_file["path"].clone(),
                    "replace",
                    turn_index,
                    json!({
                        "replacements": changed_file["replacements"].clone(),
                        "occurrences": changed_file["occurrences"].clone(),
                        "truncated": changed_file["truncated"].clone(),
                        "fileReservation": changed_file["fileReservation"].clone(),
                        "rollback": changed_file["rollback"].clone(),
                        "dryRun": success.output["dryRun"].clone(),
                    }),
                )?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn record_tool_registry_mcp_file_changed(
    config: &McpConfig,
    path: JsonValue,
    operation: &'static str,
    turn_index: usize,
    dispatch_extra: JsonValue,
) -> Result<(), SidecarError> {
    append_event_log(
        &config.event_log,
        json!({
            "type": "agent_event",
            "eventKind": "file_changed",
            "payload": {
                "path": path,
                "operation": operation,
                "runtime": TOOL_REGISTRY_MCP_RUNTIME,
                "turnIndex": turn_index,
                "dispatch": {
                    "registryVersion": "tool_registry_v2",
                    "details": dispatch_extra,
                },
            },
        }),
    )
}

fn persist_tool_registry_mcp_command_output(
    config: &McpConfig,
    success: &ToolDispatchSuccess,
) -> Result<(), SidecarError> {
    for stream in ["stdout", "stderr"] {
        let text = success.output[stream].as_str().unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        append_event_log(
            &config.event_log,
            json!({
                "type": "agent_event",
                "eventKind": "command_output",
                "payload": {
                    "stream": stream,
                    "text": truncate_bytes(text, 64 * 1024),
                    "toolCallId": success.tool_call_id,
                    "runtime": TOOL_REGISTRY_MCP_RUNTIME,
                },
            }),
        )?;
    }
    Ok(())
}

fn first_tool_dispatch_failure(
    report: &xero_agent_core::ToolBatchDispatchReport,
) -> Option<(String, String)> {
    for group in &report.groups {
        for outcome in &group.outcomes {
            if let ToolDispatchOutcome::Failed(failure) = outcome {
                return Some((failure.error.code.clone(), failure.error.message.clone()));
            }
        }
    }
    None
}

fn tool_registry_mcp_success_dispatch(
    group: &ToolGroupDispatchReport,
    success: &ToolDispatchSuccess,
) -> JsonValue {
    json!({
        "registryVersion": "tool_registry_v2",
        "providerLoop": "cursor_sdk_bridge",
        "groupMode": group.mode,
        "groupElapsedMs": group.elapsed_ms,
        "elapsedMs": success.elapsed_ms,
        "truncation": success.truncation,
        "sandbox": success.sandbox_metadata,
        "telemetry": success.telemetry_attributes,
        "preHook": success.pre_hook_payload,
        "postHook": success.post_hook_payload,
        "fileReservation": success.output.get("fileReservation").cloned(),
        "rollback": success.output.get("rollback").cloned(),
        "timeout": group.timeout_error.as_ref().map(tool_execution_error_json),
    })
}

fn tool_registry_mcp_failure_dispatch(
    group: &ToolGroupDispatchReport,
    failure: &ToolDispatchFailure,
) -> JsonValue {
    json!({
        "registryVersion": "tool_registry_v2",
        "providerLoop": "cursor_sdk_bridge",
        "groupMode": group.mode,
        "groupElapsedMs": group.elapsed_ms,
        "elapsedMs": failure.elapsed_ms,
        "typedErrorCategory": failure.error.category,
        "modelMessage": failure.error.model_message,
        "retryable": failure.error.retryable,
        "doomLoopSignal": failure.doom_loop_signal,
        "rollbackPayload": failure.rollback_payload,
        "rollbackError": failure.rollback_error.as_ref().map(tool_execution_error_json),
        "sandbox": failure.sandbox_metadata,
        "preHook": failure.pre_hook_payload,
        "postHook": failure.post_hook_payload,
        "timeout": group.timeout_error.as_ref().map(tool_execution_error_json),
    })
}

fn tool_execution_error_json(error: &ToolExecutionError) -> JsonValue {
    json!({
        "category": error.category,
        "code": error.code,
        "message": error.message,
        "modelMessage": error.model_message,
        "retryable": error.retryable,
    })
}

fn redacted_tool_registry_mcp_input(tool_name: &str, input: &JsonValue) -> (JsonValue, bool) {
    let Some(object) = input.as_object() else {
        return (input.clone(), false);
    };
    let mut redacted = object.clone();
    let mut changed = false;
    for key in ["content", "patch", "search", "replacement"] {
        if redacted.contains_key(key)
            && matches!(
                tool_name,
                TOOL_NAME_WRITE | TOOL_NAME_PATCH | TOOL_NAME_REPLACE
            )
        {
            redacted.insert(
                key.into(),
                json!({
                    "redacted": true,
                    "reason": "tool_input_may_contain_source_text",
                }),
            );
            changed = true;
        }
    }
    (JsonValue::Object(redacted), changed)
}

fn mcp_tools_initialize_result(params: &JsonValue, mode: ToolRegistryMcpMode) -> JsonValue {
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
            "tools": { "listChanged": false }
        },
        "serverInfo": {
            "name": MCP_TOOLS_SERVER_NAME,
            "title": "Xero Tool Registry V2",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": format!(
            "Use Xero Tool Registry V2 tools for auditable workspace actions. Current mode: {}.",
            mode.as_str()
        )
    })
}

fn mcp_tool_definition_from_descriptor(descriptor: ToolDescriptorV2) -> JsonValue {
    json!({
        "name": descriptor.name,
        "title": descriptor.name,
        "description": descriptor.description,
        "inputSchema": descriptor.input_schema,
        "annotations": {
            "readOnlyHint": descriptor.mutability == ToolMutability::ReadOnly,
            "destructiveHint": matches!(
                descriptor.effect_class,
                ToolEffectClass::WorkspaceMutation | ToolEffectClass::CommandExecution
            ),
            "idempotentHint": descriptor.mutability == ToolMutability::ReadOnly,
            "openWorldHint": matches!(
                descriptor.effect_class,
                ToolEffectClass::ExternalService | ToolEffectClass::BrowserControl | ToolEffectClass::DeviceControl
            ),
        }
    })
}

fn mcp_tool_call_id(name: &str, request_id: &JsonValue, arguments: &JsonValue) -> String {
    if let Some(id) = arguments
        .get("toolCallId")
        .or_else(|| arguments.get("tool_call_id"))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return id.to_owned();
    }
    let id = match request_id {
        JsonValue::String(value) => value.clone(),
        other => other.to_string(),
    };
    format!(
        "cursor-mcp-{}-{}",
        sanitize_identifier_segment(name),
        sanitize_identifier_segment(&id)
    )
}

fn sanitize_identifier_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('-');
    if sanitized.is_empty() {
        "id".into()
    } else {
        sanitized.chars().take(80).collect()
    }
}

fn mcp_tool_result(
    summary: impl Into<String>,
    structured_content: JsonValue,
    is_error: bool,
) -> JsonValue {
    let summary = summary.into();
    json!({
        "content": [{ "type": "text", "text": summary }],
        "structuredContent": structured_content,
        "isError": is_error,
    })
}

fn mcp_tool_error(code: impl Into<String>, message: impl Into<String>) -> JsonValue {
    let code = code.into();
    let message = message.into();
    mcp_tool_result(
        message.clone(),
        json!({ "ok": false, "error": { "code": code, "message": message } }),
        true,
    )
}

fn mcp_success(id: JsonValue, result: JsonValue) -> JsonValue {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn mcp_error(id: JsonValue, code: i64, message: &str, data: JsonValue) -> JsonValue {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message, "data": data }
    })
}

fn parse_run_config(mut args: Vec<String>) -> Result<RunConfig, SidecarError> {
    let repo_root = required_path(take_option(&mut args, "--repo-root")?, "--repo-root")?;
    let project_id = required(take_option(&mut args, "--project-id")?, "--project-id")?;
    let run_id = required(take_option(&mut args, "--run-id")?, "--run-id")?;
    let session_id = required(take_option(&mut args, "--session-id")?, "--session-id")?;
    let runtime_agent_id =
        take_option(&mut args, "--runtime-agent-id")?.unwrap_or_else(|| "engineer".into());
    let model_id = required(take_option(&mut args, "--model")?, "--model")?;
    let prompt_file = required_path(take_option(&mut args, "--prompt-file")?, "--prompt-file")?;
    let attachments_json_file =
        take_option(&mut args, "--attachments-json-file")?.map(PathBuf::from);
    let api_key_file = take_option(&mut args, "--api-key-file")?.map(PathBuf::from);
    let state_dir = required_path(take_option(&mut args, "--state-dir")?, "--state-dir")?;
    let bridge_path = take_option(&mut args, "--bridge-path")?
        .map(PathBuf::from)
        .or_else(default_bridge_path)
        .ok_or_else(|| SidecarError::usage("Missing --bridge-path."))?;
    let node_command = take_option(&mut args, "--node-command")?.unwrap_or_else(|| "node".into());
    let mcp_mode = take_option(&mut args, "--mcp-mode")?
        .map(|value| parse_mcp_mode(&value))
        .transpose()?
        .unwrap_or(DEFAULT_MCP_MODE);
    let timeout_ms = take_option(&mut args, "--timeout-ms")?
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .max(1);
    let cursor_agent_id = take_option(&mut args, "--cursor-agent-id")?;
    let local_auto_mode = take_option(&mut args, "--local-auto-mode")?;
    let fixture = take_option(&mut args, "--fixture")?.map(PathBuf::from);
    reject_unknown_options(&args)?;
    Ok(RunConfig {
        repo_root,
        project_id,
        run_id,
        session_id,
        runtime_agent_id,
        model_id,
        prompt_file,
        attachments_json_file,
        api_key_file,
        state_dir,
        bridge_path,
        node_command,
        mcp_mode,
        timeout_ms,
        cursor_agent_id,
        local_auto_mode,
        fixture,
    })
}

fn parse_mcp_config(mut args: Vec<String>) -> Result<McpConfig, SidecarError> {
    let self_test = take_bool_flag(&mut args, "--self-test");
    let repo_root = required_path(take_option(&mut args, "--repo")?, "--repo")?;
    let project_id = required(take_option(&mut args, "--project-id")?, "--project-id")?;
    let run_id = required(take_option(&mut args, "--run-id")?, "--run-id")?;
    let session_id = required(take_option(&mut args, "--session-id")?, "--session-id")?;
    let runtime_agent_id =
        take_option(&mut args, "--runtime-agent-id")?.unwrap_or_else(|| "engineer".into());
    let state_dir = required_path(take_option(&mut args, "--state-dir")?, "--state-dir")?;
    let event_log = required_path(take_option(&mut args, "--event-log")?, "--event-log")?;
    let mut mode = take_option(&mut args, "--mode")?
        .map(|value| parse_mcp_mode(&value))
        .transpose()?
        .unwrap_or(DEFAULT_MCP_MODE);
    if take_bool_flag(&mut args, "--allow-writes") && mode == ToolRegistryMcpMode::ObserveOnly {
        mode = ToolRegistryMcpMode::WorkspaceWrite;
    }
    if take_bool_flag(&mut args, "--allow-commands") {
        mode = ToolRegistryMcpMode::CommandEnabled;
    }
    reject_unknown_options(&args)?;
    Ok(McpConfig {
        repo_root,
        project_id,
        run_id,
        session_id,
        runtime_agent_id,
        state_dir,
        event_log,
        mode,
        self_test,
    })
}

fn run_self_test(mut args: Vec<String>) -> Result<(), SidecarError> {
    let bridge_path = take_option(&mut args, "--bridge-path")?
        .map(PathBuf::from)
        .or_else(default_bridge_path)
        .ok_or_else(|| SidecarError::usage("Missing --bridge-path."))?;
    let node_command = take_option(&mut args, "--node-command")?.unwrap_or_else(|| "node".into());
    reject_unknown_options(&args)?;
    if !command_available(&node_command) {
        return Err(SidecarError::system(
            "cursor_node_runtime_missing",
            format!("Node command `{node_command}` was not found."),
        ));
    }
    if !bridge_path.is_file() {
        return Err(SidecarError::system(
            "cursor_bridge_missing",
            format!(
                "Cursor SDK bridge script `{}` was not found.",
                bridge_path.display()
            ),
        ));
    }
    let output = Command::new(&node_command)
        .arg(bridge_path)
        .arg("--self-test")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| {
            SidecarError::system(
                "cursor_bridge_self_test_failed",
                format!("Could not run Cursor SDK bridge self-test: {error}"),
            )
        })?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.status.success() {
        return Err(SidecarError::system(
            "cursor_bridge_self_test_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    emit_stdout(json!({ "type": "self_test_completed", "sidecarVersion": SIDECAR_VERSION }));
    Ok(())
}

fn default_bridge_path() -> Option<PathBuf> {
    env::var_os("XERO_CURSOR_BRIDGE_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../scripts/cursor-sdk-bridge.mjs"),
            )
        })
}

fn parse_mcp_mode(value: &str) -> Result<ToolRegistryMcpMode, SidecarError> {
    match value.trim() {
        "observe-only" | "observe_only" | "observe" | "read-only" | "read_only" => {
            Ok(ToolRegistryMcpMode::ObserveOnly)
        }
        "workspace-write" | "workspace_write" | "write" => Ok(ToolRegistryMcpMode::WorkspaceWrite),
        "command-enabled" | "command_enabled" | "command" => {
            Ok(ToolRegistryMcpMode::CommandEnabled)
        }
        other => Err(SidecarError::usage(format!(
            "Unknown MCP mode `{other}`. Use observe-only, workspace-write, or command-enabled."
        ))),
    }
}

fn take_bool_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn take_option(args: &mut Vec<String>, flag: &str) -> Result<Option<String>, SidecarError> {
    let Some(index) = args.iter().position(|arg| arg == flag) else {
        return Ok(None);
    };
    args.remove(index);
    if index >= args.len() {
        return Err(SidecarError::usage(format!("{flag} requires a value.")));
    }
    let value = args.remove(index);
    if value.starts_with("--") {
        return Err(SidecarError::usage(format!("{flag} requires a value.")));
    }
    Ok(Some(value))
}

fn reject_unknown_options(args: &[String]) -> Result<(), SidecarError> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(SidecarError::usage(format!(
            "Unknown argument(s): {}",
            args.join(" ")
        )))
    }
}

fn required(value: Option<String>, flag: &str) -> Result<String, SidecarError> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SidecarError::usage(format!("{flag} is required.")))
}

fn required_path(value: Option<String>, flag: &str) -> Result<PathBuf, SidecarError> {
    required(value, flag).map(PathBuf::from)
}

fn append_event_log(path: &Path, event: JsonValue) -> Result<(), SidecarError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            SidecarError::system(
                "cursor_mcp_event_log_prepare_failed",
                format!("Could not create `{}`: {error}", parent.display()),
            )
        })?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            SidecarError::system(
                "cursor_mcp_event_log_open_failed",
                format!("Could not open `{}`: {error}", path.display()),
            )
        })?;
    let line = serde_json::to_string(&event).map_err(|error| {
        SidecarError::system(
            "cursor_mcp_event_log_encode_failed",
            format!("Could not encode sidecar event: {error}"),
        )
    })?;
    writeln!(file, "{line}").map_err(|error| {
        SidecarError::system(
            "cursor_mcp_event_log_write_failed",
            format!("Could not write `{}`: {error}", path.display()),
        )
    })
}

fn emit_stdout(event: JsonValue) {
    match serde_json::to_string(&event) {
        Ok(line) => println!("{line}"),
        Err(_) => println!(r#"{{"type":"sidecar_failed","code":"cursor_sidecar_encode_failed"}}"#),
    }
}

fn truncate_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes.min(value.len());
    while !value.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}...[truncated]", &value[..end])
}

fn safe_path_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('_');
    if sanitized.is_empty() {
        "run".into()
    } else {
        sanitized.chars().take(120).collect()
    }
}

fn command_available(command: &str) -> bool {
    if command.contains('/') || command.contains('\\') {
        return Path::new(command).is_file();
    }
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|path| path.join(command).is_file())
}

fn print_usage() {
    println!(
        "Usage:\n  xero-cursor-sidecar run --repo-root PATH --project-id ID --run-id ID --session-id ID --model MODEL --prompt-file PATH --state-dir PATH [--api-key-file PATH]\n  xero-cursor-sidecar mcp serve-tools --repo PATH --project-id ID --run-id ID --session-id ID --state-dir PATH --event-log PATH\n  xero-cursor-sidecar self-test"
    );
}
