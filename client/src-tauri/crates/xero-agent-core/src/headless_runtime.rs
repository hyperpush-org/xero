use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::{
    AgentCoreStore, AgentRuntimeFacade, ApprovalDecisionRequest, CancelRunRequest,
    CompactSessionRequest, ContinueRunRequest, CoreError, CoreResult, EnvironmentLifecycleConfig,
    EnvironmentLifecycleService, ExportTraceRequest, FakeProviderRuntime, FileAgentCoreStore,
    ForkSessionRequest, MessageRole, NewContextManifest, NewMessageRecord, NewRunRecord,
    NewRuntimeEvent, OpenAiCompatibleProviderPreflightProbeRequest, PermissionProfileSandbox,
    ProductionRuntimeContract, ProjectTrustState, ProviderCapabilityCatalogInput,
    ProviderPreflightInput, ProviderPreflightRequiredFeatures, ProviderPreflightSnapshot,
    ProviderPreflightSource, ProviderSelection, ResumeRunRequest, RunSnapshot, RunStatus,
    RuntimeEventKind, RuntimeMessageProviderMetadata, RuntimeProviderToolCallMetadata,
    RuntimeTrace, RuntimeTraceContext, SandboxApprovalSource, SandboxExecutionContext,
    SandboxPlatform, StartRunRequest, StaticToolHandler, ToolApplicationKind,
    ToolApplicationMetadata, ToolApprovalRequirement, ToolBatchDispatchReport,
    ToolBatchDispatchSafety, ToolBudget, ToolCallInput, ToolDescriptorV2, ToolDispatchConfig,
    ToolDispatchFailure, ToolDispatchOutcome, ToolDispatchSuccess, ToolEffectClass,
    ToolExecutionContext, ToolExecutionError, ToolGroupExecutionMode, ToolHandlerOutput,
    ToolMutability, ToolPolicy, ToolPolicyDecision, ToolRegistryV2, ToolResultTruncationContract,
    ToolRollback, ToolSandboxRequirement, UserInputRequest, MUTATION_EXECUTION_SCOPE_ATTRIBUTE,
};

const DEFAULT_HEADLESS_PROVIDER_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_HEADLESS_MAX_PROVIDER_TURNS: usize = 8;
const DEFAULT_HEADLESS_COMMAND_TIMEOUT_MS: u64 = 120_000;
const MAX_HEADLESS_COMMAND_TIMEOUT_MS: u64 = 10 * 60 * 1_000;
const MAX_TOOL_OUTPUT_BYTES: usize = 128 * 1024;
const HEADLESS_LIST_LIMIT: usize = 200;
const SKIPPED_WORKSPACE_DIRS: &[&str] = &[
    ".git",
    ".xero",
    ".next",
    ".turbo",
    ".cache",
    ".yarn",
    ".pnpm-store",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "target",
];
const HEADLESS_TOOL_READ: &str = "read";
const HEADLESS_TOOL_LIST: &str = "list";
const HEADLESS_TOOL_WRITE: &str = "write";
const HEADLESS_TOOL_PATCH: &str = "patch";
const HEADLESS_TOOL_DELETE: &str = "delete";
const HEADLESS_TOOL_MOVE: &str = "move";
const HEADLESS_TOOL_REPLACE: &str = "replace";
const HEADLESS_TOOL_COMMAND: &str = "command";
const HEADLESS_TOOL_TODO: &str = "todo";
const HEADLESS_SUPPORTED_AGENT_TOOLS: &[&str] = &[
    HEADLESS_TOOL_READ,
    HEADLESS_TOOL_LIST,
    HEADLESS_TOOL_WRITE,
    HEADLESS_TOOL_PATCH,
    HEADLESS_TOOL_DELETE,
    HEADLESS_TOOL_MOVE,
    HEADLESS_TOOL_REPLACE,
    HEADLESS_TOOL_COMMAND,
    HEADLESS_TOOL_TODO,
];
const LEGACY_HEADLESS_MINI_TOOLS: &[&str] = &["read_file", "write_file", "list_files"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadlessProviderExecutionConfig {
    Fake,
    OpenAiCompatible(OpenAiCompatibleHeadlessConfig),
    OpenAiCodexResponses(OpenAiCodexHeadlessConfig),
}

impl HeadlessProviderExecutionConfig {
    pub fn provider_id(&self) -> &str {
        match self {
            Self::Fake => "fake_provider",
            Self::OpenAiCompatible(config) => config.provider_id.as_str(),
            Self::OpenAiCodexResponses(config) => config.provider_id.as_str(),
        }
    }

    pub fn model_id(&self) -> &str {
        match self {
            Self::Fake => "fake-model",
            Self::OpenAiCompatible(config) => config.model_id.as_str(),
            Self::OpenAiCodexResponses(config) => config.model_id.as_str(),
        }
    }

    fn has_provider_credentials(&self) -> bool {
        match self {
            Self::Fake => true,
            Self::OpenAiCompatible(config) => {
                config
                    .api_key
                    .as_deref()
                    .is_some_and(|key| !key.trim().is_empty())
                    || is_local_http_endpoint(&config.base_url)
            }
            Self::OpenAiCodexResponses(config) => {
                !config.access_token.trim().is_empty() && !config.account_id.trim().is_empty()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleHeadlessConfig {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout_ms: u64,
    pub workspace_root: Option<PathBuf>,
    pub allow_workspace_writes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCodexHeadlessConfig {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub access_token: String,
    pub account_id: String,
    pub session_id: Option<String>,
    pub timeout_ms: u64,
    pub workspace_root: Option<PathBuf>,
    pub allow_workspace_writes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessRuntimeOptions {
    pub ci_mode: bool,
    pub max_provider_turns: usize,
    pub max_wall_time_ms: Option<u64>,
    pub max_tool_calls: Option<u64>,
    pub max_command_calls: Option<u64>,
    pub provider_preflight: Option<ProviderPreflightSnapshot>,
}

impl Default for HeadlessRuntimeOptions {
    fn default() -> Self {
        Self {
            ci_mode: false,
            max_provider_turns: DEFAULT_HEADLESS_MAX_PROVIDER_TURNS,
            max_wall_time_ms: None,
            max_tool_calls: None,
            max_command_calls: None,
            provider_preflight: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HeadlessAgentStageWorkflow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start_phase_id: Option<String>,
    phases: Vec<HeadlessAgentStage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HeadlessAgentStage {
    id: String,
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    required_checks: Vec<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retry_limit: Option<u64>,
    #[serde(default)]
    branches: Vec<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HeadlessAgentDefinitionProfile {
    definition_id: String,
    definition_version: i64,
    base_capability_profile: String,
    display_name: String,
    task_purpose: String,
    workflow_contract: String,
    final_response_contract: String,
    system_prompt_fragments: Vec<String>,
    allowed_approval_modes: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowed_tools: Option<BTreeSet<String>>,
    #[serde(default)]
    denied_tools: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workflow_structure: Option<HeadlessAgentStageWorkflow>,
}

impl HeadlessAgentDefinitionProfile {
    fn from_snapshot(
        snapshot: &JsonValue,
        expected_definition_id: &str,
        expected_definition_version: i64,
    ) -> CoreResult<Self> {
        let object = snapshot.as_object().ok_or_else(|| {
            CoreError::invalid_request(
                "agent_core_headless_agent_definition_invalid",
                "The selected Agent definition snapshot must be a JSON object.",
            )
        })?;
        let definition_id = object
            .get("id")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CoreError::invalid_request(
                    "agent_core_headless_agent_definition_id_missing",
                    "The selected Agent definition snapshot has no id.",
                )
            })?;
        if definition_id != expected_definition_id {
            return Err(CoreError::invalid_request(
                "agent_core_headless_agent_definition_id_mismatch",
                format!(
                    "Selected Agent definition `{expected_definition_id}` does not match snapshot `{definition_id}`."
                ),
            ));
        }
        let definition_version = object
            .get("version")
            .and_then(JsonValue::as_i64)
            .unwrap_or(expected_definition_version);
        if definition_version != expected_definition_version {
            return Err(CoreError::invalid_request(
                "agent_core_headless_agent_definition_version_mismatch",
                format!(
                    "Selected Agent definition `{definition_id}` version `{expected_definition_version}` does not match snapshot version `{definition_version}`."
                ),
            ));
        }
        if object
            .get("lifecycleState")
            .and_then(JsonValue::as_str)
            .is_some_and(|state| state != "active")
        {
            return Err(CoreError::invalid_request(
                "agent_core_headless_agent_definition_inactive",
                format!("Agent definition `{definition_id}` is not active."),
            ));
        }

        let base_capability_profile = object
            .get("baseCapabilityProfile")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| base_capability_profile_for_runtime_agent(definition_id))
            .to_owned();
        let allowed_approval_modes = object
            .get("allowedApprovalModes")
            .and_then(JsonValue::as_array)
            .map(|values| normalized_string_set(values.iter()))
            .filter(|values| !values.is_empty())
            .unwrap_or_else(|| default_approval_modes_for_profile(&base_capability_profile));

        let mut allowed_tools = None;
        let mut denied_tools = BTreeSet::new();
        if let Some(policy) = object.get("toolPolicy") {
            match policy {
                JsonValue::String(policy) => {
                    allowed_tools = named_headless_tool_policy(policy);
                }
                JsonValue::Object(policy) => {
                    allowed_tools = policy
                        .get("allowedTools")
                        .and_then(JsonValue::as_array)
                        .map(|values| normalized_headless_tool_set(values.iter()));
                    denied_tools = policy
                        .get("deniedTools")
                        .and_then(JsonValue::as_array)
                        .map(|values| normalized_headless_tool_set(values.iter()))
                        .unwrap_or_default();
                    if policy.get("commandAllowed").and_then(JsonValue::as_bool) == Some(false) {
                        denied_tools.insert(HEADLESS_TOOL_COMMAND.into());
                    }
                    if policy
                        .get("destructiveWriteAllowed")
                        .and_then(JsonValue::as_bool)
                        == Some(false)
                    {
                        denied_tools.insert(HEADLESS_TOOL_DELETE.into());
                        denied_tools.insert(HEADLESS_TOOL_MOVE.into());
                    }
                }
                _ => {
                    return Err(CoreError::invalid_request(
                        "agent_core_headless_agent_tool_policy_invalid",
                        format!("Agent definition `{definition_id}` has an invalid toolPolicy."),
                    ));
                }
            }
        }
        if !profile_allows_workspace_writes(&base_capability_profile) {
            allowed_tools = Some(BTreeSet::from([
                HEADLESS_TOOL_READ.into(),
                HEADLESS_TOOL_LIST.into(),
                HEADLESS_TOOL_TODO.into(),
            ]));
        }
        if let Some(allowed) = allowed_tools.as_mut() {
            allowed.retain(|tool| supported_headless_agent_tools().contains(tool.as_str()));
            for denied in &denied_tools {
                allowed.remove(denied);
            }
        }

        let workflow_structure = object
            .get("workflowStructure")
            .cloned()
            .map(serde_json::from_value::<HeadlessAgentStageWorkflow>)
            .transpose()
            .map_err(|error| {
                CoreError::invalid_request(
                    "agent_core_headless_agent_stages_invalid",
                    format!("Agent definition `{definition_id}` has invalid Stages: {error}"),
                )
            })?;
        validate_headless_agent_stages(definition_id, workflow_structure.as_ref())?;

        let system_prompt_fragments = object
            .get("prompts")
            .and_then(JsonValue::as_array)
            .into_iter()
            .flatten()
            .filter(|prompt| prompt.get("role").and_then(JsonValue::as_str) == Some("system"))
            .filter_map(|prompt| prompt.get("body").and_then(JsonValue::as_str))
            .map(str::trim)
            .filter(|body| !body.is_empty())
            .map(str::to_owned)
            .collect();

        Ok(Self {
            definition_id: definition_id.into(),
            definition_version,
            base_capability_profile,
            display_name: definition_text(object, "displayName", definition_id),
            task_purpose: definition_text(object, "taskPurpose", ""),
            workflow_contract: definition_text(object, "workflowContract", ""),
            final_response_contract: definition_text(object, "finalResponseContract", ""),
            system_prompt_fragments,
            allowed_approval_modes,
            allowed_tools,
            denied_tools,
            workflow_structure,
        })
    }

    fn initial_stage(&self) -> Option<&HeadlessAgentStage> {
        let workflow = self.workflow_structure.as_ref()?;
        let start = workflow
            .start_phase_id
            .as_deref()
            .or_else(|| workflow.phases.first().map(|stage| stage.id.as_str()))?;
        workflow.phases.iter().find(|stage| stage.id == start)
    }

    fn effective_allowed_tools_for_stage(
        &self,
        stage: Option<&HeadlessAgentStage>,
    ) -> Option<BTreeSet<String>> {
        let mut allowed = self.allowed_tools.clone();
        if let Some(stage_tools) = stage.and_then(|stage| stage.allowed_tools.as_ref()) {
            let declares_restriction = !stage_tools.is_empty();
            let stage_tools = stage_tools
                .iter()
                .flat_map(|tool| normalized_headless_tool_names(tool))
                .map(str::to_owned)
                .collect::<BTreeSet<_>>();
            if declares_restriction {
                allowed = Some(match allowed {
                    Some(policy_tools) => policy_tools.intersection(&stage_tools).cloned().collect(),
                    None => stage_tools,
                });
            }
        }
        if let Some(allowed) = allowed.as_mut() {
            allowed.insert(HEADLESS_TOOL_TODO.into());
            for denied in &self.denied_tools {
                allowed.remove(denied);
            }
        }
        allowed
    }

    fn effective_allowed_tools(&self) -> Option<BTreeSet<String>> {
        self.effective_allowed_tools_for_stage(self.initial_stage())
    }
}

#[derive(Debug, Clone)]
struct HeadlessAgentStageRuntime {
    profile: HeadlessAgentDefinitionProfile,
    current_stage_id: String,
    successful_tools: BTreeMap<String, u64>,
    completed_todos: BTreeSet<String>,
    completed: bool,
    final_reprompted: bool,
}

#[derive(Debug, Clone)]
struct HeadlessAgentStageTransition {
    from_stage_id: String,
    to_stage_id: Option<String>,
}

impl HeadlessAgentStageRuntime {
    fn new(identity: &HeadlessRunIdentity, snapshot: &RunSnapshot) -> Option<Self> {
        let profile = identity.definition_profile.clone()?;
        let initial_stage = profile.initial_stage()?.id.clone();
        let mut runtime = Self {
            profile,
            current_stage_id: initial_stage,
            successful_tools: BTreeMap::new(),
            completed_todos: BTreeSet::new(),
            completed: false,
            final_reprompted: false,
        };
        runtime.advance_if_satisfied();
        for event in &snapshot.events {
            match event.event_kind {
                RuntimeEventKind::ToolCompleted if event.payload["ok"] == true => {
                    if let Some(tool_name) = event.payload["toolName"].as_str() {
                        *runtime
                            .successful_tools
                            .entry(tool_name.to_owned())
                            .or_default() += 1;
                    }
                    if let Some(todo_id) = event.payload["completedTodoId"].as_str() {
                        runtime.completed_todos.insert(todo_id.to_owned());
                    }
                }
                RuntimeEventKind::VerificationGate
                    if event.payload["state"] == "passed"
                        && event.payload["stageId"].as_str()
                            == Some(runtime.current_stage_id.as_str()) =>
                {
                    if let Some(next_stage_id) =
                        event.payload["nextStageId"].as_str().map(str::to_owned)
                    {
                        runtime.enter_stage(next_stage_id);
                    } else {
                        runtime.completed = true;
                    }
                }
                RuntimeEventKind::VerificationGate
                    if event.payload["state"] == "blocked"
                        && event.payload["stageId"].as_str()
                            == Some(runtime.current_stage_id.as_str()) =>
                {
                    runtime.final_reprompted = true;
                }
                _ => {}
            }
        }
        runtime.advance_if_satisfied();
        Some(runtime)
    }

    fn current_stage(&self) -> Option<&HeadlessAgentStage> {
        if self.completed {
            return None;
        }
        self.profile
            .workflow_structure
            .as_ref()?
            .phases
            .iter()
            .find(|stage| stage.id == self.current_stage_id)
    }

    fn allowed_tools(&self) -> Option<BTreeSet<String>> {
        self.profile
            .effective_allowed_tools_for_stage(self.current_stage())
    }

    fn stage_json(&self) -> JsonValue {
        self.current_stage()
            .map(|stage| {
                json!({
                    "id": stage.id,
                    "title": stage.title,
                    "allowedTools": self.allowed_tools().map(|tools| tools.into_iter().collect::<Vec<_>>()),
                    "requiredChecks": stage.required_checks,
                })
            })
            .unwrap_or(JsonValue::Null)
    }

    fn record_tool_results(
        &mut self,
        results: &[HeadlessToolResultMessage],
    ) -> Vec<HeadlessAgentStageTransition> {
        for result in results {
            if result.payload["ok"] == true {
                *self
                    .successful_tools
                    .entry(result.tool_name.clone())
                    .or_default() += 1;
                if let Some(todo_id) = completed_todo_id_from_headless_result(result) {
                    self.completed_todos.insert(todo_id);
                }
            }
        }
        self.advance_if_satisfied()
    }

    fn advance_if_satisfied(&mut self) -> Vec<HeadlessAgentStageTransition> {
        let mut transitions = Vec::new();
        let mut visited = BTreeSet::new();
        while !self.completed && visited.insert(self.current_stage_id.clone()) {
            let Some(stage) = self.current_stage().cloned() else {
                break;
            };
            if !self.stage_checks_satisfied(&stage) {
                break;
            }
            let next_stage_id = stage
                .branches
                .iter()
                .find(|branch| self.branch_matches(branch))
                .and_then(|branch| branch["targetPhaseId"].as_str())
                .map(str::to_owned)
                .or_else(|| {
                    let phases = &self.profile.workflow_structure.as_ref()?.phases;
                    let index = phases.iter().position(|candidate| candidate.id == stage.id)?;
                    phases.get(index + 1).map(|next| next.id.clone())
                });
            if next_stage_id.as_deref() == Some(stage.id.as_str()) {
                break;
            }
            transitions.push(HeadlessAgentStageTransition {
                from_stage_id: stage.id,
                to_stage_id: next_stage_id.clone(),
            });
            if let Some(next_stage_id) = next_stage_id {
                self.enter_stage(next_stage_id);
            } else {
                self.completed = true;
            }
        }
        transitions
    }

    fn enter_stage(&mut self, stage_id: String) {
        self.current_stage_id = stage_id;
        self.successful_tools.clear();
        self.completed_todos.clear();
        self.final_reprompted = false;
        self.completed = false;
    }

    fn stage_checks_satisfied(&self, stage: &HeadlessAgentStage) -> bool {
        stage
            .required_checks
            .iter()
            .all(|check| self.condition_satisfied(check))
    }

    fn branch_matches(&self, branch: &JsonValue) -> bool {
        self.condition_satisfied(&branch["condition"])
    }

    fn condition_satisfied(&self, condition: &JsonValue) -> bool {
        match condition["kind"].as_str() {
            Some("always") => true,
            Some("todo_completed") => condition["todoId"]
                .as_str()
                .is_some_and(|todo_id| self.completed_todos.contains(todo_id)),
            Some("tool_succeeded") => {
                headless_condition_tool_names(condition)
                    .iter()
                    .map(|tool| self.successful_tools.get(tool).copied().unwrap_or(0))
                    .sum::<u64>()
                    >= condition["minCount"].as_u64().unwrap_or(1)
            }
            _ => false,
        }
    }

    fn completion_gate_message(&self) -> Option<String> {
        if self.completed {
            return None;
        }
        let Some(stage) = self.current_stage() else {
            return Some("Complete the active Stage checks before answering.".into());
        };
        if self.stage_checks_satisfied(stage)
            && stage
                .branches
                .iter()
                .all(|branch| !self.branch_matches(branch))
            && self.next_sequential_stage(stage).is_none()
        {
            return None;
        };
        Some(format!(
            "Xero Stage gate: `{}` is incomplete. Complete these required checks before the final response: {}.",
            stage.title,
            stage
                .required_checks
                .iter()
                .map(|check| check.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    fn next_sequential_stage(&self, stage: &HeadlessAgentStage) -> Option<&HeadlessAgentStage> {
        let phases = &self.profile.workflow_structure.as_ref()?.phases;
        let index = phases.iter().position(|candidate| candidate.id == stage.id)?;
        phases.get(index + 1)
    }
}

fn completed_todo_id_from_headless_result(result: &HeadlessToolResultMessage) -> Option<String> {
    (result.tool_name == HEADLESS_TOOL_TODO)
        .then(|| completed_todo_id_from_headless_output(&result.payload["output"]))
        .flatten()
        .map(str::to_owned)
}

fn completed_todo_id_from_headless_output(output: &JsonValue) -> Option<&str> {
    (output["changedItem"]["status"] == "completed")
        .then(|| output["changedItem"]["id"].as_str())
        .flatten()
}

#[derive(Debug, Clone)]
pub struct HeadlessProviderRuntime<S = FileAgentCoreStore> {
    store: S,
    provider: HeadlessProviderExecutionConfig,
    options: HeadlessRuntimeOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeadlessRunIdentity {
    pub(crate) runtime_agent_id: String,
    pub(crate) agent_definition_id: String,
    pub(crate) agent_definition_version: i64,
    pub(crate) system_prompt: String,
    pub(crate) thinking_effort: Option<String>,
    pub(crate) approval_mode: String,
    definition_profile: Option<HeadlessAgentDefinitionProfile>,
}

impl HeadlessRunIdentity {
    pub(crate) fn from_request(
        request: &StartRunRequest,
        workspace_root: Option<&Path>,
    ) -> CoreResult<Self> {
        let runtime_agent_id = request
            .controls
            .as_ref()
            .map(|controls| controls.runtime_agent_id.trim())
            .filter(|id| !id.is_empty())
            .unwrap_or("engineer")
            .to_owned();
        let agent_definition_id = request
            .controls
            .as_ref()
            .and_then(|controls| controls.agent_definition_id.as_deref())
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or(runtime_agent_id.as_str())
            .to_owned();
        let agent_definition_version = request
            .controls
            .as_ref()
            .and_then(|controls| controls.agent_definition_version)
            .filter(|version| *version > 0)
            .unwrap_or(1);
        let thinking_effort = request.controls.as_ref().and_then(|controls| {
            normalize_headless_thinking_effort(controls.thinking_effort.as_deref())
        });
        let approval_mode = request
            .controls
            .as_ref()
            .map(|controls| controls.approval_mode.trim())
            .filter(|mode| !mode.is_empty())
            .unwrap_or("suggest")
            .to_owned();
        let definition_profile = request
            .controls
            .as_ref()
            .and_then(|controls| controls.agent_definition_snapshot.as_ref())
            .map(|snapshot| {
                HeadlessAgentDefinitionProfile::from_snapshot(
                    snapshot,
                    &agent_definition_id,
                    agent_definition_version,
                )
            })
            .transpose()?;
        if let Some(profile) = definition_profile.as_ref() {
            if approval_mode != "strict" && !profile.allowed_approval_modes.contains(&approval_mode) {
                return Err(CoreError::invalid_request(
                    "agent_core_headless_agent_approval_mode_denied",
                    format!(
                        "Agent definition `{}` does not allow approval mode `{approval_mode}`.",
                        profile.definition_id
                    ),
                ));
            }
        }
        let system_prompt = definition_profile.as_ref().map_or_else(
            || headless_system_prompt_for_agent(&runtime_agent_id, workspace_root),
            |profile| custom_headless_agent_system_prompt(profile, workspace_root),
        );
        Ok(Self {
            runtime_agent_id,
            agent_definition_id,
            agent_definition_version,
            system_prompt,
            thinking_effort,
            approval_mode,
            definition_profile,
        })
    }

    pub(crate) fn definition_runtime_json(&self) -> Option<JsonValue> {
        self.definition_profile
            .as_ref()
            .and_then(|profile| serde_json::to_value(profile).ok())
    }

    pub(crate) fn incomplete_stage_gate(
        &self,
        snapshot: &RunSnapshot,
    ) -> Option<(String, String)> {
        let runtime = HeadlessAgentStageRuntime::new(self, snapshot)?;
        runtime
            .completion_gate_message()
            .map(|message| (runtime.current_stage_id, message))
    }

    pub(crate) fn from_snapshot(snapshot: &RunSnapshot) -> CoreResult<Self> {
        let run_started = snapshot
            .events
            .iter()
            .find(|event| event.event_kind == RuntimeEventKind::RunStarted);
        let thinking_effort = run_started
            .and_then(|event| event.payload.get("thinkingEffort"))
            .and_then(JsonValue::as_str)
            .and_then(|effort| normalize_headless_thinking_effort(Some(effort)));
        let approval_mode = run_started
            .and_then(|event| event.payload.get("approvalMode"))
            .and_then(JsonValue::as_str)
            .filter(|mode| matches!(*mode, "suggest" | "auto_edit" | "yolo" | "strict"))
            .unwrap_or("suggest")
            .to_owned();
        let definition_profile = match run_started
            .and_then(|event| event.payload.get("agentDefinitionRuntime"))
        {
            None | Some(JsonValue::Null) => None,
            Some(profile) => {
                let profile = serde_json::from_value::<HeadlessAgentDefinitionProfile>(
                    profile.clone(),
                )
                .map_err(|error| {
                    CoreError::system_fault(
                        "agent_core_headless_agent_definition_runtime_invalid",
                        format!(
                            "Persisted headless Agent runtime policy is invalid: {error}"
                        ),
                    )
                })?;
                if profile.definition_id != snapshot.agent_definition_id
                    || profile.definition_version != snapshot.agent_definition_version
                {
                    return Err(CoreError::system_fault(
                        "agent_core_headless_agent_definition_runtime_mismatch",
                        "Persisted headless Agent runtime policy does not match the run identity.",
                    ));
                }
                if approval_mode != "strict"
                    && !profile.allowed_approval_modes.contains(&approval_mode)
                {
                    return Err(CoreError::system_fault(
                        "agent_core_headless_agent_approval_runtime_mismatch",
                        "Persisted headless Agent approval mode is outside its saved policy.",
                    ));
                }
                Some(profile)
            }
        };
        Ok(Self {
            runtime_agent_id: snapshot.runtime_agent_id.clone(),
            agent_definition_id: snapshot.agent_definition_id.clone(),
            agent_definition_version: snapshot.agent_definition_version,
            system_prompt: snapshot.system_prompt.clone(),
            thinking_effort,
            approval_mode,
            definition_profile,
        })
    }

    fn allows_workspace_writes(&self) -> bool {
        self.definition_profile.as_ref().map_or_else(
            || headless_agent_allows_workspace_writes(&self.runtime_agent_id),
            |profile| {
                profile_allows_workspace_writes(&profile.base_capability_profile)
                    && profile.effective_allowed_tools().is_none_or(|tools| {
                        tools.iter().any(|tool| headless_tool_is_write(tool))
                    })
            },
        )
            && matches!(self.approval_mode.as_str(), "auto_edit" | "yolo")
    }

    fn allows_commands(&self) -> bool {
        self.definition_profile.as_ref().map_or_else(
            || headless_agent_allows_workspace_writes(&self.runtime_agent_id),
            |profile| {
                profile_allows_workspace_writes(&profile.base_capability_profile)
                    && profile
                        .effective_allowed_tools()
                        .is_none_or(|tools| tools.contains(HEADLESS_TOOL_COMMAND))
            },
        )
            && self.approval_mode == "yolo"
    }

    fn allowed_tools(&self) -> Option<BTreeSet<String>> {
        self.definition_profile
            .as_ref()
            .and_then(HeadlessAgentDefinitionProfile::effective_allowed_tools)
    }

}

impl<S> HeadlessProviderRuntime<S>
where
    S: AgentCoreStore,
{
    pub fn new(
        store: S,
        provider: HeadlessProviderExecutionConfig,
        options: HeadlessRuntimeOptions,
    ) -> Self {
        Self {
            store,
            provider,
            options,
        }
    }

    pub fn store(&self) -> S {
        self.store.clone()
    }

    fn start_real_run(&self, request: StartRunRequest) -> CoreResult<RunSnapshot> {
        self.validate_selected_provider(&request.provider)?;
        let workspace_root = self.workspace_root();
        let identity = HeadlessRunIdentity::from_request(&request, workspace_root.as_deref())?;
        let runtime_contract = ProductionRuntimeContract::real_provider(
            "headless_provider_runtime",
            request.project_id.clone(),
            request.provider.provider_id.clone(),
            request.provider.model_id.clone(),
            self.store.runtime_store_descriptor(&request.project_id),
        );
        crate::validate_production_runtime_contract(&runtime_contract)?;
        let preflight = self.provider_preflight_snapshot()?;
        let blockers = crate::provider_preflight_blockers(&preflight);
        if let Some(blocker) = blockers.first() {
            return Err(CoreError::invalid_request(
                "agent_core_provider_preflight_blocked",
                format!(
                    "Headless real-provider execution is blocked because `{}` failed: {}",
                    blocker.code, blocker.message
                ),
            ));
        }
        if self.options.ci_mode {
            return Err(CoreError::invalid_request(
                "agent_core_ci_real_provider_blocked",
                "Headless CI mode requires explicit harness execution with `--provider fake_provider` until non-interactive write approvals are configured.",
            ));
        }

        let snapshot = self.store.insert_run(NewRunRecord {
            trace_id: None,
            runtime_agent_id: identity.runtime_agent_id.clone(),
            agent_definition_id: identity.agent_definition_id.clone(),
            agent_definition_version: identity.agent_definition_version,
            system_prompt: identity.system_prompt.clone(),
            project_id: request.project_id,
            agent_session_id: request.agent_session_id,
            run_id: request.run_id,
            provider_id: request.provider.provider_id,
            model_id: request.provider.model_id,
            prompt: request.prompt.clone(),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::RunStarted,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "run_started",
            )),
            payload: json!({
                "status": "starting",
                "providerId": snapshot.provider_id,
                "modelId": snapshot.model_id,
                "execution": "production_real_provider",
                "runtimeAgentId": identity.runtime_agent_id.clone(),
                "agentDefinitionId": identity.agent_definition_id.clone(),
                "agentDefinitionVersion": identity.agent_definition_version,
                "agentDefinitionRuntime": identity.definition_profile.clone(),
                "thinkingEffort": identity.thinking_effort.clone(),
                "approvalMode": identity.approval_mode.clone(),
                "providerPreflight": preflight.clone(),
            }),
        })?;

        let semantic_index_requirement_reasons =
            crate::semantic_workspace_prompt_requirement_reasons(&request.prompt);
        let semantic_index_required = !semantic_index_requirement_reasons.is_empty();
        let semantic_index_state = self
            .store
            .semantic_workspace_index_state(&snapshot.project_id);
        let lifecycle = EnvironmentLifecycleService::new(self.store.clone());
        let environment = lifecycle.start_environment(EnvironmentLifecycleConfig {
            environment_id: format!("env-{}-{}", snapshot.project_id, snapshot.run_id),
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            workspace_root: self
                .workspace_root()
                .map(|root| root.display().to_string())
                .unwrap_or_else(|| ".".into()),
            provider_credentials_required: true,
            provider_credentials_valid: self.provider.has_provider_credentials(),
            tool_packs: vec!["owned_agent_core".into(), "tool_registry_v2".into()],
            semantic_index_required,
            semantic_index_available: semantic_index_state.is_ready(),
            semantic_index_state,
            semantic_index_requirement_reasons,
            ..EnvironmentLifecycleConfig::local(&snapshot.project_id, &snapshot.run_id)
        })?;
        if !environment.state.is_ready() {
            return self.store.load_run(&snapshot.project_id, &snapshot.run_id);
        }

        self.store.append_message(NewMessageRecord {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            role: MessageRole::System,
            content: identity.system_prompt.clone(),
            provider_metadata: None,
        })?;
        self.store.append_message(NewMessageRecord {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            role: MessageRole::User,
            content: request.prompt.clone(),
            provider_metadata: None,
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::MessageDelta,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "user_message",
            )),
            payload: json!({ "role": "user", "text": request.prompt }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ValidationStarted,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "provider_diagnostics_started",
            )),
            payload: json!({ "label": "provider_diagnostics" }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ValidationCompleted,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "provider_diagnostics_completed",
            )),
            payload: json!({
                "label": "provider_diagnostics",
                "outcome": preflight.status.as_str(),
                "providerId": self.provider.provider_id(),
                "modelId": self.provider.model_id(),
                "providerPreflight": preflight.clone(),
            }),
        })?;

        let started = self
            .store
            .load_run(&snapshot.project_id, &snapshot.run_id)?;
        self.drive_real_turn(started, &preflight, &identity)?;
        self.store.load_run(&snapshot.project_id, &snapshot.run_id)
    }

    fn continue_real_run(&self, request: ContinueRunRequest) -> CoreResult<RunSnapshot> {
        crate::validate_required(&request.prompt, "prompt")?;
        let before = self.store.load_run(&request.project_id, &request.run_id)?;
        self.validate_selected_provider(&ProviderSelection {
            provider_id: before.provider_id.clone(),
            model_id: before.model_id.clone(),
        })?;
        let preflight = self.provider_preflight_snapshot()?;
        let blockers = crate::provider_preflight_blockers(&preflight);
        if let Some(blocker) = blockers.first() {
            return Err(CoreError::invalid_request(
                "agent_core_provider_preflight_blocked",
                format!(
                    "Headless real-provider continuation is blocked because `{}` failed: {}",
                    blocker.code, blocker.message
                ),
            ));
        }
        self.store
            .update_run_status(&request.project_id, &request.run_id, RunStatus::Running)?;
        self.store.append_message(NewMessageRecord {
            project_id: request.project_id.clone(),
            run_id: request.run_id.clone(),
            role: MessageRole::User,
            content: request.prompt.clone(),
            provider_metadata: None,
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: request.project_id.clone(),
            run_id: request.run_id.clone(),
            event_kind: RuntimeEventKind::MessageDelta,
            trace: Some(RuntimeTraceContext::for_run(
                &before.trace_id,
                &request.run_id,
                "user_message",
            )),
            payload: json!({ "role": "user", "text": request.prompt }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: request.project_id.clone(),
            run_id: request.run_id.clone(),
            event_kind: RuntimeEventKind::ValidationCompleted,
            trace: Some(RuntimeTraceContext::for_run(
                &before.trace_id,
                &request.run_id,
                "provider_diagnostics_completed",
            )),
            payload: json!({
                "label": "provider_diagnostics",
                "outcome": preflight.status.as_str(),
                "providerId": self.provider.provider_id(),
                "modelId": self.provider.model_id(),
                "providerPreflight": preflight.clone(),
            }),
        })?;
        let snapshot = self.store.load_run(&request.project_id, &request.run_id)?;
        let identity = HeadlessRunIdentity::from_snapshot(&snapshot)?;
        self.drive_real_turn(snapshot, &preflight, &identity)?;
        self.store.load_run(&request.project_id, &request.run_id)
    }

    fn drive_real_turn(
        &self,
        snapshot: RunSnapshot,
        provider_preflight: &ProviderPreflightSnapshot,
        identity: &HeadlessRunIdentity,
    ) -> CoreResult<()> {
        let provider_timeout_ms = match &self.provider {
            HeadlessProviderExecutionConfig::OpenAiCompatible(config) => config.timeout_ms,
            HeadlessProviderExecutionConfig::OpenAiCodexResponses(config) => config.timeout_ms,
            HeadlessProviderExecutionConfig::Fake => {
                return Err(CoreError::invalid_request(
                    "agent_core_provider_mismatch",
                    "The fake provider cannot drive a real provider turn.",
                ));
            }
        };
        let client = Client::builder()
            .timeout(Duration::from_millis(normalize_timeout(
                provider_timeout_ms,
            )))
            .build()
            .map_err(|error| {
                CoreError::system_fault(
                    "agent_core_provider_http_client_failed",
                    format!("Xero could not build the headless provider HTTP client: {error}"),
                )
            })?;
        let mut chat_messages = chat_messages_from_snapshot(&snapshot);
        let mut current = snapshot;
        let started_at = Instant::now();
        let mut tool_call_count = 0_u64;
        let mut command_call_count = 0_u64;
        let provider_turn_base = next_headless_provider_turn_index(&current);
        let mut stage_runtime = HeadlessAgentStageRuntime::new(identity, &current);
        let todo_state = Arc::new(Mutex::new(BTreeMap::new()));
        for turn_offset in 0..self.options.max_provider_turns {
            let turn_index = provider_turn_base.saturating_add(turn_offset);
            if let Some(max_wall_time_ms) = self.options.max_wall_time_ms {
                if started_at.elapsed().as_millis() as u64 > max_wall_time_ms {
                    return self.fail_real_provider_run(
                        &current,
                        "agent_core_headless_wall_time_exceeded",
                        "The headless real-provider loop exceeded its configured wall-time limit.",
                        "wall_time_limit_exceeded",
                        json!({
                            "limitMs": max_wall_time_ms,
                            "elapsedMs": started_at.elapsed().as_millis() as u64,
                        }),
                    );
                }
            }
            let workspace_root = self.workspace_root();
            let allowed_tools = stage_runtime
                .as_ref()
                .and_then(HeadlessAgentStageRuntime::allowed_tools)
                .or_else(|| identity.allowed_tools());
            let stage = stage_runtime
                .as_ref()
                .map(HeadlessAgentStageRuntime::stage_json)
                .unwrap_or(JsonValue::Null);
            let tool_runtime = HeadlessProductionToolRuntime::new_with_modes(
                workspace_root.as_ref(),
                self.allow_workspace_writes() && identity.allows_workspace_writes(),
                self.allow_workspace_writes() && identity.allows_commands(),
                self.app_data_roots_for_project(&current.project_id),
            )?
            .with_allowed_tools(allowed_tools)
            .with_todo_state(Arc::clone(&todo_state));
            self.record_tool_registry_snapshot(&current, turn_index, &tool_runtime)?;
            self.record_provider_context_manifest(
                &current,
                turn_index,
                &tool_runtime,
                provider_preflight,
                &stage,
            )?;
            let project_id = current.project_id.clone();
            let run_id = current.run_id.clone();
            let trace_id = current.trace_id.clone();
            let store = &self.store;
            let response = match match &self.provider {
                HeadlessProviderExecutionConfig::OpenAiCompatible(config) => {
                    send_openai_compatible_chat(
                        &client,
                        config,
                        &chat_messages,
                        tool_runtime.openai_tool_definitions(),
                        identity.thinking_effort.as_deref(),
                    )
                }
                HeadlessProviderExecutionConfig::OpenAiCodexResponses(config) => {
                    send_openai_codex_responses(
                        &client,
                        config,
                        &chat_messages,
                        tool_runtime.openai_response_tool_definitions(),
                        identity.thinking_effort.as_deref(),
                        |text, reasoning| {
                            // Best-effort progress event for the TUI's
                            // inline preview. Errors here would only
                            // bubble up as a missing preview update; the
                            // final message is still persisted below.
                            let _ = store.append_event(NewRuntimeEvent {
                                project_id: project_id.clone(),
                                run_id: run_id.clone(),
                                event_kind: RuntimeEventKind::MessageDelta,
                                trace: Some(RuntimeTraceContext::for_provider_turn(
                                    &trace_id, &run_id, turn_index,
                                )),
                                payload: json!({
                                    "role": "assistant",
                                    "text": text,
                                    "reasoningText": reasoning,
                                    "inProgress": true,
                                }),
                            });
                        },
                    )
                }
                HeadlessProviderExecutionConfig::Fake => {
                    unreachable!("fake provider rejected above")
                }
            } {
                Ok(response) => response,
                Err(error) => return self.fail_real_provider_turn(&current, error),
            };
            let content = response.content_text();
            let tool_calls = response.tool_calls;
            let reasoning_text = response.reasoning;
            let reasoning_for_metadata = if reasoning_text.trim().is_empty() {
                None
            } else {
                Some(reasoning_text.clone())
            };
            let next_tool_call_count = tool_call_count + tool_calls.len() as u64;
            if self
                .options
                .max_tool_calls
                .is_some_and(|limit| next_tool_call_count > limit)
            {
                return self.fail_real_provider_run(
                    &current,
                    "agent_core_headless_tool_call_limit_exceeded",
                    "The headless real-provider loop exceeded its configured tool-call limit.",
                    "tool_call_limit_exceeded",
                    json!({
                        "limit": self.options.max_tool_calls,
                        "attemptedToolCalls": next_tool_call_count,
                    }),
                );
            }
            let next_command_call_count = command_call_count
                + tool_calls
                    .iter()
                    .filter(|call| headless_tool_is_command(&call.name))
                    .count() as u64;
            if self
                .options
                .max_command_calls
                .is_some_and(|limit| next_command_call_count > limit)
            {
                return self.fail_real_provider_run(
                    &current,
                    "agent_core_headless_command_call_limit_exceeded",
                    "The headless real-provider loop exceeded its configured command-call limit.",
                    "command_call_limit_exceeded",
                    json!({
                        "limit": self.options.max_command_calls,
                        "attemptedCommandCalls": next_command_call_count,
                    }),
                );
            }

            if !content.trim().is_empty() {
                self.store.append_event(NewRuntimeEvent {
                    project_id: current.project_id.clone(),
                    run_id: current.run_id.clone(),
                    event_kind: RuntimeEventKind::MessageDelta,
                    trace: Some(RuntimeTraceContext::for_provider_turn(
                        &current.trace_id,
                        &current.run_id,
                        turn_index,
                    )),
                    payload: json!({ "role": "assistant", "text": content }),
                })?;
            }

            if tool_calls.is_empty() {
                if let Some((stage_runtime, gate_message)) = stage_runtime
                    .as_mut()
                    .and_then(|runtime| runtime.completion_gate_message().map(|message| (runtime, message)))
                {
                    let stage_id = stage_runtime.current_stage_id.clone();
                    self.store.append_event(NewRuntimeEvent {
                        project_id: current.project_id.clone(),
                        run_id: current.run_id.clone(),
                        event_kind: RuntimeEventKind::AssistantCandidate,
                        trace: Some(RuntimeTraceContext::for_provider_turn(
                            &current.trace_id,
                            &current.run_id,
                            turn_index,
                        )),
                        payload: json!({
                            "state": "superseded",
                            "disposition": "stage_gate",
                            "stageId": stage_id,
                            "text": content,
                        }),
                    })?;
                    self.store.append_event(NewRuntimeEvent {
                        project_id: current.project_id.clone(),
                        run_id: current.run_id.clone(),
                        event_kind: RuntimeEventKind::VerificationGate,
                        trace: Some(RuntimeTraceContext::for_run(
                            &current.trace_id,
                            &current.run_id,
                            "stage_gate_blocked",
                        )),
                        payload: json!({
                            "state": "blocked",
                            "stageId": stage_id,
                            "message": gate_message,
                            "turnIndex": turn_index,
                        }),
                    })?;
                    if stage_runtime.final_reprompted {
                        return self.fail_real_provider_run(
                            &current,
                            "agent_core_headless_stage_incomplete",
                            "The headless Agent returned repeated final answers before completing its Stage checks.",
                            "stage_gate_incomplete",
                            json!({
                                "stageId": stage_id,
                                "turnIndex": turn_index,
                            }),
                        );
                    }
                    stage_runtime.final_reprompted = true;
                    self.store.append_message(NewMessageRecord {
                        project_id: current.project_id.clone(),
                        run_id: current.run_id.clone(),
                        role: MessageRole::Developer,
                        content: gate_message.clone(),
                        provider_metadata: None,
                    })?;
                    chat_messages.push(json!({
                        "role": "user",
                        "content": gate_message,
                    }));
                    current = self.store.load_run(&current.project_id, &current.run_id)?;
                    continue;
                }
                if !content.trim().is_empty() {
                    let assistant_provider_message_id =
                        provider_assistant_message_id(&current.run_id, turn_index);
                    let provider_metadata = reasoning_for_metadata.as_ref().map(|reasoning| {
                        RuntimeMessageProviderMetadata::assistant_turn(
                            assistant_provider_message_id.clone(),
                            Some(reasoning.clone()),
                            None,
                            Vec::new(),
                        )
                    });
                    self.store.append_message(NewMessageRecord {
                        project_id: current.project_id.clone(),
                        run_id: current.run_id.clone(),
                        role: MessageRole::Assistant,
                        content,
                        provider_metadata,
                    })?;
                }
                self.store.update_run_status(
                    &current.project_id,
                    &current.run_id,
                    RunStatus::Completed,
                )?;
                self.store.append_event(NewRuntimeEvent {
                    project_id: current.project_id.clone(),
                    run_id: current.run_id.clone(),
                    event_kind: RuntimeEventKind::RunCompleted,
                    trace: Some(RuntimeTraceContext::for_run(
                        &current.trace_id,
                        &current.run_id,
                        "run_completed",
                    )),
                    payload: json!({
                        "summary": "Headless real-provider run completed.",
                        "state": "complete",
                    }),
                })?;
                return Ok(());
            }

            let assistant_provider_message_id =
                provider_assistant_message_id(&current.run_id, turn_index);
            chat_messages.push(openai_assistant_message(&content, &tool_calls));
            self.store.append_message(NewMessageRecord {
                project_id: current.project_id.clone(),
                run_id: current.run_id.clone(),
                role: MessageRole::Assistant,
                content: content.clone(),
                provider_metadata: Some(RuntimeMessageProviderMetadata::assistant_turn(
                    assistant_provider_message_id.clone(),
                    reasoning_for_metadata.clone(),
                    None,
                    tool_calls
                        .iter()
                        .map(RuntimeProviderToolCallMetadata::from)
                        .collect(),
                )),
            })?;

            let tool_results = self.dispatch_headless_tool_batch(
                &tool_runtime,
                &current,
                turn_index,
                &tool_calls,
                &assistant_provider_message_id,
            )?;
            if let Some(stage_runtime) = stage_runtime.as_mut() {
                for transition in stage_runtime.record_tool_results(&tool_results) {
                    self.store.append_event(NewRuntimeEvent {
                        project_id: current.project_id.clone(),
                        run_id: current.run_id.clone(),
                        event_kind: RuntimeEventKind::VerificationGate,
                        trace: Some(RuntimeTraceContext::for_run(
                            &current.trace_id,
                            &current.run_id,
                            "stage_gate_passed",
                        )),
                        payload: json!({
                            "state": "passed",
                            "stageId": transition.from_stage_id,
                            "nextStageId": transition.to_stage_id,
                            "turnIndex": turn_index,
                        }),
                    })?;
                }
            }
            for result in tool_results {
                let result_payload = serde_json::to_string(&result.payload).map_err(|error| {
                    CoreError::system_fault(
                        "agent_core_tool_result_encode_failed",
                        format!("Xero could not encode a headless tool result: {error}"),
                    )
                })?;
                self.store.append_message(NewMessageRecord {
                    project_id: current.project_id.clone(),
                    run_id: current.run_id.clone(),
                    role: MessageRole::Tool,
                    content: result_payload.clone(),
                    provider_metadata: Some(RuntimeMessageProviderMetadata::tool_result(
                        provider_tool_result_message_id(
                            &current.run_id,
                            turn_index,
                            &result.tool_call_id,
                        ),
                        result.tool_call_id.clone(),
                        result.tool_name.clone(),
                        result.parent_assistant_message_id.clone(),
                    )),
                })?;
                chat_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": result.tool_call_id,
                    "content": result_payload,
                }));
            }
            tool_call_count = next_tool_call_count;
            command_call_count = next_command_call_count;
            current = self.store.load_run(&current.project_id, &current.run_id)?;
        }

        self.store
            .update_run_status(&current.project_id, &current.run_id, RunStatus::Failed)?;
        self.store.append_event(NewRuntimeEvent {
            project_id: current.project_id.clone(),
            run_id: current.run_id.clone(),
            event_kind: RuntimeEventKind::RunFailed,
            trace: Some(RuntimeTraceContext::for_run(
                &current.trace_id,
                &current.run_id,
                "turn_limit_exceeded",
            )),
            payload: json!({
                "code": "agent_core_headless_turn_limit_exceeded",
                "message": "The headless real-provider loop reached its turn limit.",
                "retryable": true,
            }),
        })?;
        Err(CoreError::invalid_request(
            "agent_core_headless_turn_limit_exceeded",
            "The headless real-provider loop reached its turn limit.",
        ))
    }

    fn fail_real_provider_run(
        &self,
        snapshot: &RunSnapshot,
        code: &'static str,
        message: &'static str,
        trace_label: &'static str,
        details: JsonValue,
    ) -> CoreResult<()> {
        self.store
            .update_run_status(&snapshot.project_id, &snapshot.run_id, RunStatus::Failed)?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::RunFailed,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                trace_label,
            )),
            payload: json!({
                "code": code,
                "message": message,
                "retryable": false,
                "details": details,
            }),
        })?;
        Err(CoreError::invalid_request(code, message))
    }

    fn fail_real_provider_turn<T>(
        &self,
        snapshot: &RunSnapshot,
        error: CoreError,
    ) -> CoreResult<T> {
        self.store
            .update_run_status(&snapshot.project_id, &snapshot.run_id, RunStatus::Failed)?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::RunFailed,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "provider_turn_failed",
            )),
            payload: json!({
                "code": error.code,
                "message": "The headless provider turn failed.",
                "retryable": matches!(
                    error.code.as_str(),
                    "agent_core_provider_request_failed"
                        | "agent_core_provider_response_read_failed"
                        | "agent_core_provider_status_failed"
                ),
            }),
        })?;
        Err(error)
    }

    fn dispatch_headless_tool_batch(
        &self,
        tool_runtime: &HeadlessProductionToolRuntime,
        snapshot: &RunSnapshot,
        turn_index: usize,
        tool_calls: &[OpenAiToolCall],
        parent_assistant_message_id: &str,
    ) -> CoreResult<Vec<HeadlessToolResultMessage>> {
        if tool_calls.is_empty() {
            return Ok(Vec::new());
        }

        let inputs = tool_calls
            .iter()
            .map(|call| ToolCallInput {
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                input: call.arguments.clone(),
            })
            .collect::<Vec<_>>();
        for input in &inputs {
            let (persisted_input, input_redacted) =
                redacted_headless_tool_input(&input.tool_name, &input.input);
            self.store.append_event(NewRuntimeEvent {
                project_id: snapshot.project_id.clone(),
                run_id: snapshot.run_id.clone(),
                event_kind: RuntimeEventKind::ToolStarted,
                trace: Some(RuntimeTraceContext::for_tool_call(
                    &snapshot.trace_id,
                    &snapshot.run_id,
                    &input.tool_call_id,
                )),
                payload: json!({
                    "toolCallId": input.tool_call_id,
                    "toolName": input.tool_name,
                    "turnIndex": turn_index,
                    "runtime": "production_real_provider",
                    "input": persisted_input,
                    "inputRedacted": input_redacted,
                    "dispatch": {
                        "registryVersion": "tool_registry_v2",
                        "providerLoop": "headless_production_provider_loop",
                    },
                }),
            })?;
        }

        let report = tool_runtime.dispatch_batch(
            &snapshot.project_id,
            &snapshot.run_id,
            turn_index,
            &inputs,
        )?;
        self.persist_headless_tool_report(snapshot, report, parent_assistant_message_id)
    }

    fn record_provider_context_manifest(
        &self,
        snapshot: &RunSnapshot,
        turn_index: usize,
        tool_runtime: &HeadlessProductionToolRuntime,
        provider_preflight: &ProviderPreflightSnapshot,
        stage: &JsonValue,
    ) -> CoreResult<()> {
        let manifest_id = format!("context-manifest-{}-{turn_index}", snapshot.run_id);
        let context_hash = headless_context_hash(snapshot, turn_index);
        let provider_preflight_hash = stable_provider_preflight_hash(provider_preflight);
        self.store.record_context_manifest(NewContextManifest {
            manifest_id: manifest_id.clone(),
            project_id: snapshot.project_id.clone(),
            agent_session_id: snapshot.agent_session_id.clone(),
            run_id: snapshot.run_id.clone(),
            provider_id: snapshot.provider_id.clone(),
            model_id: snapshot.model_id.clone(),
            turn_index,
            context_hash: context_hash.clone(),
            trace: Some(RuntimeTraceContext::for_context_manifest(
                &snapshot.trace_id,
                &snapshot.run_id,
                &manifest_id,
                turn_index,
            )),
            manifest: json!({
                "kind": "provider_context_package",
                "schema": "xero.provider_context_package.v1",
                "schemaVersion": 1,
                "projectId": snapshot.project_id,
                "agentSessionId": snapshot.agent_session_id,
                "runId": snapshot.run_id,
                "providerId": snapshot.provider_id,
                "modelId": snapshot.model_id,
                "turnIndex": turn_index,
                "runtime": "production_real_provider",
                "workspaceRoot": self.workspace_root().map(|root| root.display().to_string()),
                "tools": tool_runtime.tool_names(),
                "stage": stage,
                "executionRegistry": "tool_registry_v2",
                "providerPreflight": provider_preflight,
                "admittedProviderPreflightHash": provider_preflight_hash,
            }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ContextManifestRecorded,
            trace: Some(RuntimeTraceContext::for_storage_write(
                &snapshot.trace_id,
                &snapshot.run_id,
                "context_manifest",
                turn_index,
            )),
            payload: json!({
                "manifestId": manifest_id,
                "contextHash": context_hash,
                "turnIndex": turn_index,
                "runtime": "production_real_provider",
            }),
        })?;
        Ok(())
    }

    fn record_tool_registry_snapshot(
        &self,
        snapshot: &RunSnapshot,
        turn_index: usize,
        tool_runtime: &HeadlessProductionToolRuntime,
    ) -> CoreResult<()> {
        let descriptors = tool_runtime.descriptors();
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ToolRegistrySnapshot,
            trace: Some(RuntimeTraceContext::for_provider_turn(
                &snapshot.trace_id,
                &snapshot.run_id,
                turn_index,
            )),
            payload: json!({
                "kind": "active_tool_registry",
                "runtime": "production_real_provider",
                "providerLoop": "headless_production_provider_loop",
                "turnIndex": turn_index,
                "executionRegistry": "tool_registry_v2",
                "descriptorNames": descriptors.iter().map(|descriptor| descriptor.name.clone()).collect::<Vec<_>>(),
                "descriptorsV2": descriptors,
                "legacyMiniToolsAvailable": false,
                "unavailableMiniTools": LEGACY_HEADLESS_MINI_TOOLS,
            }),
        })?;
        Ok(())
    }

    fn persist_headless_tool_report(
        &self,
        snapshot: &RunSnapshot,
        report: ToolBatchDispatchReport,
        parent_assistant_message_id: &str,
    ) -> CoreResult<Vec<HeadlessToolResultMessage>> {
        let mut results = Vec::new();
        for group in report.groups {
            let persistence_group = HeadlessToolPersistenceGroup {
                mode: group.mode.clone(),
                elapsed_ms: group.elapsed_ms,
                timeout_error: group.timeout_error.as_ref(),
            };
            for outcome in group.outcomes {
                match outcome {
                    ToolDispatchOutcome::Succeeded(success) => {
                        self.persist_headless_tool_success(
                            snapshot,
                            success,
                            &persistence_group,
                            parent_assistant_message_id,
                            &mut results,
                        )?;
                    }
                    ToolDispatchOutcome::Failed(failure) => {
                        self.persist_headless_tool_failure(
                            snapshot,
                            failure,
                            &persistence_group,
                            parent_assistant_message_id,
                            &mut results,
                        )?;
                    }
                }
            }
        }
        Ok(results)
    }

    fn persist_headless_tool_success(
        &self,
        snapshot: &RunSnapshot,
        success: ToolDispatchSuccess,
        group: &HeadlessToolPersistenceGroup<'_>,
        parent_assistant_message_id: &str,
        results: &mut Vec<HeadlessToolResultMessage>,
    ) -> CoreResult<()> {
        let dispatch = headless_dispatch_success_metadata(
            &success,
            group.mode.clone(),
            group.elapsed_ms,
            group.timeout_error,
        );
        let tool_call_id = success.tool_call_id.clone();
        let tool_name = success.tool_name.clone();
        let provider_payload = json!({
            "toolCallId": tool_call_id,
            "toolName": tool_name,
            "ok": true,
            "summary": success.summary,
            "output": success.output,
            "parentAssistantMessageId": parent_assistant_message_id,
            "providerToolName": tool_name,
        });
        let completed_todo_id = (tool_name == HEADLESS_TOOL_TODO)
            .then(|| completed_todo_id_from_headless_output(&provider_payload["output"]))
            .flatten();
        if tool_name == HEADLESS_TOOL_WRITE {
            self.record_headless_file_changed(
                snapshot,
                provider_payload["output"]["path"].clone(),
                "write",
                json!({
                    "bytes": provider_payload["output"]["bytes"].clone(),
                    "fileReservation": provider_payload["output"]["fileReservation"].clone(),
                    "rollback": provider_payload["output"]["rollback"].clone(),
                }),
            )?;
        } else if tool_name == HEADLESS_TOOL_PATCH {
            for path in provider_payload["output"]["changedFiles"]
                .as_array()
                .into_iter()
                .flatten()
            {
                self.record_headless_file_changed(snapshot, path.clone(), "patch", json!({}))?;
            }
        } else if tool_name == HEADLESS_TOOL_DELETE {
            self.record_headless_file_changed(
                snapshot,
                provider_payload["output"]["path"].clone(),
                "delete",
                json!({
                    "kind": provider_payload["output"]["kind"].clone(),
                    "recursive": provider_payload["output"]["recursive"].clone(),
                    "fileReservation": provider_payload["output"]["fileReservation"].clone(),
                    "rollback": provider_payload["output"]["rollback"].clone(),
                }),
            )?;
        } else if tool_name == HEADLESS_TOOL_MOVE {
            self.record_headless_file_changed(
                snapshot,
                provider_payload["output"]["from"].clone(),
                "move_from",
                json!({
                    "to": provider_payload["output"]["to"].clone(),
                    "kind": provider_payload["output"]["kind"].clone(),
                    "fileReservation": provider_payload["output"]["fileReservation"].clone(),
                    "rollback": provider_payload["output"]["rollback"].clone(),
                }),
            )?;
            self.record_headless_file_changed(
                snapshot,
                provider_payload["output"]["to"].clone(),
                "move_to",
                json!({
                    "from": provider_payload["output"]["from"].clone(),
                    "kind": provider_payload["output"]["kind"].clone(),
                    "fileReservation": provider_payload["output"]["fileReservation"].clone(),
                    "rollback": provider_payload["output"]["rollback"].clone(),
                }),
            )?;
        } else if tool_name == HEADLESS_TOOL_REPLACE {
            for changed_file in provider_payload["output"]["changedFiles"]
                .as_array()
                .into_iter()
                .flatten()
            {
                self.record_headless_file_changed(
                    snapshot,
                    changed_file["path"].clone(),
                    "replace",
                    json!({
                        "replacements": changed_file["replacements"].clone(),
                        "occurrences": changed_file["occurrences"].clone(),
                        "truncated": changed_file["truncated"].clone(),
                        "fileReservation": changed_file["fileReservation"].clone(),
                        "rollback": changed_file["rollback"].clone(),
                        "dryRun": provider_payload["output"]["dryRun"].clone(),
                    }),
                )?;
            }
        }
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ToolCompleted,
            trace: Some(RuntimeTraceContext::for_tool_call(
                &snapshot.trace_id,
                &snapshot.run_id,
                provider_payload["toolCallId"].as_str().unwrap_or_default(),
            )),
            payload: json!({
                "toolCallId": provider_payload["toolCallId"].clone(),
                "toolName": provider_payload["toolName"].clone(),
                "ok": true,
                "summary": provider_payload["summary"].clone(),
                "completedTodoId": completed_todo_id,
                "resultPreview": truncate_text(&provider_payload.to_string(), 2048),
                "dispatch": dispatch,
            }),
        })?;
        results.push(HeadlessToolResultMessage {
            tool_call_id: provider_payload["toolCallId"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            tool_name,
            parent_assistant_message_id: parent_assistant_message_id.to_owned(),
            payload: provider_payload,
        });
        Ok(())
    }

    fn record_headless_file_changed(
        &self,
        snapshot: &RunSnapshot,
        path: JsonValue,
        operation: &'static str,
        dispatch_extra: JsonValue,
    ) -> CoreResult<()> {
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::FileChanged,
            trace: Some(RuntimeTraceContext::for_storage_write(
                &snapshot.trace_id,
                &snapshot.run_id,
                "workspace_file",
                snapshot.context_manifests.len(),
            )),
            payload: json!({
                "path": path,
                "operation": operation,
                "runtime": "production_real_provider",
                "dispatch": {
                    "registryVersion": "tool_registry_v2",
                    "details": dispatch_extra,
                },
            }),
        })?;
        Ok(())
    }

    fn persist_headless_tool_failure(
        &self,
        snapshot: &RunSnapshot,
        failure: ToolDispatchFailure,
        group: &HeadlessToolPersistenceGroup<'_>,
        parent_assistant_message_id: &str,
        results: &mut Vec<HeadlessToolResultMessage>,
    ) -> CoreResult<()> {
        let dispatch = headless_dispatch_failure_metadata(
            &failure,
            group.mode.clone(),
            group.elapsed_ms,
            group.timeout_error,
        );
        let tool_call_id = failure.tool_call_id.clone();
        let tool_name = failure.tool_name.clone();
        let provider_payload = json!({
            "toolCallId": tool_call_id,
            "toolName": tool_name,
            "ok": false,
            "error": {
                "category": failure.error.category,
                "code": failure.error.code,
                "message": failure.error.model_message,
                "retryable": failure.error.retryable,
            },
            "parentAssistantMessageId": parent_assistant_message_id,
            "providerToolName": tool_name,
        });
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ToolCompleted,
            trace: Some(RuntimeTraceContext::for_tool_call(
                &snapshot.trace_id,
                &snapshot.run_id,
                provider_payload["toolCallId"].as_str().unwrap_or_default(),
            )),
            payload: json!({
                "toolCallId": provider_payload["toolCallId"].clone(),
                "toolName": provider_payload["toolName"].clone(),
                "ok": false,
                "code": provider_payload["error"]["code"].clone(),
                "message": provider_payload["error"]["message"].clone(),
                "dispatch": dispatch,
            }),
        })?;
        results.push(HeadlessToolResultMessage {
            tool_call_id: provider_payload["toolCallId"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            tool_name,
            parent_assistant_message_id: parent_assistant_message_id.to_owned(),
            payload: provider_payload,
        });
        Ok(())
    }

    fn validate_selected_provider(&self, provider: &ProviderSelection) -> CoreResult<()> {
        if provider.provider_id != self.provider.provider_id() {
            return Err(CoreError::invalid_request(
                "agent_core_provider_mismatch",
                format!(
                    "Headless runtime was configured for provider `{}` but request selected `{}`.",
                    self.provider.provider_id(),
                    provider.provider_id
                ),
            ));
        }
        if provider.model_id != self.provider.model_id() {
            return Err(CoreError::invalid_request(
                "agent_core_model_mismatch",
                format!(
                    "Headless runtime was configured for model `{}` but request selected `{}`.",
                    self.provider.model_id(),
                    provider.model_id
                ),
            ));
        }
        Ok(())
    }

    fn workspace_root(&self) -> Option<PathBuf> {
        match &self.provider {
            HeadlessProviderExecutionConfig::Fake => None,
            HeadlessProviderExecutionConfig::OpenAiCompatible(config) => {
                config.workspace_root.clone()
            }
            HeadlessProviderExecutionConfig::OpenAiCodexResponses(config) => {
                config.workspace_root.clone()
            }
        }
    }

    fn allow_workspace_writes(&self) -> bool {
        match &self.provider {
            HeadlessProviderExecutionConfig::Fake => false,
            HeadlessProviderExecutionConfig::OpenAiCompatible(config) => {
                config.allow_workspace_writes
            }
            HeadlessProviderExecutionConfig::OpenAiCodexResponses(config) => {
                config.allow_workspace_writes
            }
        }
    }

    fn app_data_roots_for_project(&self, project_id: &str) -> Vec<String> {
        let descriptor = self.store.runtime_store_descriptor(project_id);
        let mut roots = BTreeSet::new();
        if let Some(root) = descriptor.root_path {
            roots.insert(root);
        }
        if let Some(database_path) = descriptor.database_path {
            if let Some(parent) = Path::new(&database_path).parent() {
                roots.insert(parent.display().to_string());
            }
        }
        roots.into_iter().collect()
    }

    fn provider_preflight_snapshot(&self) -> CoreResult<ProviderPreflightSnapshot> {
        if let Some(snapshot) = self.options.provider_preflight.clone() {
            return Ok(snapshot);
        }

        match &self.provider {
            HeadlessProviderExecutionConfig::Fake => {
                Ok(crate::provider_preflight_snapshot(ProviderPreflightInput {
                    profile_id: "fake_provider".into(),
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                    source: ProviderPreflightSource::LiveProbe,
                    checked_at: crate::now_timestamp(),
                    age_seconds: Some(0),
                    ttl_seconds: None,
                    required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
                    capabilities: crate::provider_capability_catalog(
                        ProviderCapabilityCatalogInput {
                            provider_id: "fake_provider".into(),
                            model_id: "fake-model".into(),
                            catalog_source: "live".into(),
                            fetched_at: Some(crate::now_timestamp()),
                            last_success_at: Some(crate::now_timestamp()),
                            cache_age_seconds: Some(0),
                            cache_ttl_seconds: Some(crate::DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
                            credential_proof: Some("none_required".into()),
                            context_window_tokens: Some(128_000),
                            max_output_tokens: Some(16_384),
                            context_limit_source: Some("built_in_registry".into()),
                            context_limit_confidence: Some("high".into()),
                            thinking_supported: false,
                            thinking_efforts: Vec::new(),
                            thinking_default_effort: None,
                            input_modalities: Vec::new(),
                            input_modalities_source: Some("unknown".into()),
                        },
                    ),
                    credential_ready: Some(true),
                    endpoint_reachable: Some(true),
                    model_available: Some(true),
                    streaming_route_available: Some(true),
                    tool_schema_accepted: Some(true),
                    reasoning_controls_accepted: None,
                    attachments_accepted: None,
                    context_limit_known: Some(true),
                    provider_error: None,
                }))
            }
            HeadlessProviderExecutionConfig::OpenAiCompatible(config) => {
                Ok(crate::run_openai_compatible_provider_preflight_probe(
                    OpenAiCompatibleProviderPreflightProbeRequest {
                        profile_id: config.provider_id.clone(),
                        provider_id: config.provider_id.clone(),
                        model_id: config.model_id.clone(),
                        required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(
                        ),
                        base_url: config.base_url.clone(),
                        api_version: None,
                        api_key: config.api_key.clone(),
                        timeout_ms: config.timeout_ms,
                        credential_proof: config
                            .api_key
                            .as_deref()
                            .filter(|key| !key.trim().is_empty())
                            .map(|_| "api_key_env_recorded".to_string())
                            .or_else(|| {
                                is_local_http_endpoint(&config.base_url)
                                    .then(|| "local_endpoint".to_string())
                            }),
                        context_window_tokens: Some(128_000),
                        max_output_tokens: Some(16_384),
                        context_limit_source: Some("configured_default".into()),
                        context_limit_confidence: Some("medium".into()),
                        thinking_supported: false,
                        thinking_efforts: Vec::new(),
                        thinking_default_effort: None,
                        input_modalities: Vec::new(),
                        input_modalities_source: Some("unknown".into()),
                    },
                ))
            }
            HeadlessProviderExecutionConfig::OpenAiCodexResponses(config) => {
                Ok(crate::provider_preflight_snapshot(ProviderPreflightInput {
                    profile_id: "openai_codex-app-oauth".into(),
                    provider_id: config.provider_id.clone(),
                    model_id: config.model_id.clone(),
                    source: ProviderPreflightSource::LiveProbe,
                    checked_at: crate::now_timestamp(),
                    age_seconds: Some(0),
                    ttl_seconds: None,
                    required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
                    capabilities: crate::provider_capability_catalog(
                        ProviderCapabilityCatalogInput {
                            provider_id: config.provider_id.clone(),
                            model_id: config.model_id.clone(),
                            catalog_source: "app_oauth_session".into(),
                            fetched_at: Some(crate::now_timestamp()),
                            last_success_at: Some(crate::now_timestamp()),
                            cache_age_seconds: Some(0),
                            cache_ttl_seconds: Some(crate::DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
                            credential_proof: Some("app_data_openai_codex_session".into()),
                            context_window_tokens: Some(272_000),
                            max_output_tokens: Some(16_384),
                            context_limit_source: Some("configured_default".into()),
                            context_limit_confidence: Some("medium".into()),
                            thinking_supported: true,
                            thinking_efforts: vec!["low".into(), "medium".into(), "high".into()],
                            thinking_default_effort: Some("medium".into()),
                            input_modalities: Vec::new(),
                            input_modalities_source: Some("unknown".into()),
                        },
                    ),
                    credential_ready: Some(self.provider.has_provider_credentials()),
                    endpoint_reachable: Some(true),
                    model_available: Some(true),
                    streaming_route_available: Some(true),
                    tool_schema_accepted: Some(true),
                    reasoning_controls_accepted: None,
                    attachments_accepted: None,
                    context_limit_known: Some(true),
                    provider_error: None,
                }))
            }
        }
    }

    fn fork_headless_session(&self, request: ForkSessionRequest) -> CoreResult<RunSnapshot> {
        let source = self
            .store
            .latest_run_for_session(&request.project_id, &request.source_agent_session_id)?;
        let run_id = generate_headless_id("run-fork");
        let forked = self.store.insert_run(NewRunRecord {
            trace_id: None,
            runtime_agent_id: source.runtime_agent_id.clone(),
            agent_definition_id: source.agent_definition_id.clone(),
            agent_definition_version: source.agent_definition_version,
            system_prompt: source.system_prompt.clone(),
            project_id: request.project_id.clone(),
            agent_session_id: request.target_agent_session_id.clone(),
            run_id: run_id.clone(),
            provider_id: source.provider_id.clone(),
            model_id: source.model_id.clone(),
            prompt: source.prompt.clone(),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: forked.project_id.clone(),
            run_id: forked.run_id.clone(),
            event_kind: RuntimeEventKind::RunStarted,
            trace: Some(RuntimeTraceContext::for_run(
                &forked.trace_id,
                &forked.run_id,
                "session_forked",
            )),
            payload: json!({
                "kind": "session_forked",
                "sourceAgentSessionId": request.source_agent_session_id,
                "targetAgentSessionId": request.target_agent_session_id,
                "sourceRunId": source.run_id,
                "sourceTraceId": source.trace_id,
            }),
        })?;
        for message in source.messages {
            self.store.append_message(NewMessageRecord {
                project_id: forked.project_id.clone(),
                run_id: forked.run_id.clone(),
                role: message.role,
                content: message.content,
                provider_metadata: message.provider_metadata,
            })?;
        }
        let manifest_id = format!("context-manifest-{}-fork", forked.run_id);
        self.store.record_context_manifest(NewContextManifest {
            manifest_id: manifest_id.clone(),
            project_id: forked.project_id.clone(),
            agent_session_id: forked.agent_session_id.clone(),
            run_id: forked.run_id.clone(),
            provider_id: forked.provider_id.clone(),
            model_id: forked.model_id.clone(),
            turn_index: 0,
            context_hash: headless_context_hash(&forked, 0),
            trace: Some(RuntimeTraceContext::for_context_manifest(
                &forked.trace_id,
                &forked.run_id,
                &manifest_id,
                0,
            )),
            manifest: json!({
                "kind": "session_fork",
                "schema": "xero.session_fork.v1",
                "sourceRunId": source.run_id,
                "sourceTraceId": source.trace_id,
            }),
        })?;
        self.store
            .update_run_status(&forked.project_id, &forked.run_id, RunStatus::Completed)?;
        self.store.load_run(&forked.project_id, &forked.run_id)
    }

    fn compact_headless_session(&self, request: CompactSessionRequest) -> CoreResult<RunSnapshot> {
        let snapshot = self
            .store
            .latest_run_for_session(&request.project_id, &request.agent_session_id)?;
        let turn_index = snapshot.context_manifests.len();
        let manifest_id = format!("context-manifest-{}-compact-{turn_index}", snapshot.run_id);
        let summary = compact_summary_from_snapshot(&snapshot);
        let context_hash = headless_context_hash(&snapshot, turn_index);
        self.store.record_context_manifest(NewContextManifest {
            manifest_id: manifest_id.clone(),
            project_id: snapshot.project_id.clone(),
            agent_session_id: snapshot.agent_session_id.clone(),
            run_id: snapshot.run_id.clone(),
            provider_id: snapshot.provider_id.clone(),
            model_id: snapshot.model_id.clone(),
            turn_index,
            context_hash: context_hash.clone(),
            trace: Some(RuntimeTraceContext::for_context_manifest(
                &snapshot.trace_id,
                &snapshot.run_id,
                &manifest_id,
                turn_index,
            )),
            manifest: json!({
                "kind": "session_compaction_artifact",
                "schema": "xero.session_compaction_artifact.v1",
                "reason": request.reason,
                "summary": summary,
                "rawTailMessageCount": 6,
                "runtime": "headless_facade",
            }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::PolicyDecision,
            trace: Some(RuntimeTraceContext::for_storage_write(
                &snapshot.trace_id,
                &snapshot.run_id,
                "session_compaction",
                turn_index,
            )),
            payload: json!({
                "kind": "session_compaction",
                "action": "compacted",
                "manifestId": manifest_id,
                "reason": request.reason,
                "contextHash": context_hash,
            }),
        })?;
        self.store.load_run(&snapshot.project_id, &snapshot.run_id)
    }
}

impl<S> AgentRuntimeFacade for HeadlessProviderRuntime<S>
where
    S: AgentCoreStore,
{
    type StartRunRequest = StartRunRequest;
    type ContinueRunRequest = ContinueRunRequest;
    type UserInputRequest = UserInputRequest;
    type ApprovalRequest = ApprovalDecisionRequest;
    type RejectRequest = ApprovalDecisionRequest;
    type CancelRunRequest = CancelRunRequest;
    type ResumeRunRequest = ResumeRunRequest;
    type ForkSessionRequest = ForkSessionRequest;
    type CompactSessionRequest = CompactSessionRequest;
    type ExportTraceRequest = ExportTraceRequest;
    type Snapshot = RunSnapshot;
    type Trace = RuntimeTrace;
    type Error = CoreError;

    fn start_run(&self, request: StartRunRequest) -> CoreResult<RunSnapshot> {
        match &self.provider {
            HeadlessProviderExecutionConfig::Fake => {
                FakeProviderRuntime::new(self.store.clone()).start_run(request)
            }
            HeadlessProviderExecutionConfig::OpenAiCompatible(_)
            | HeadlessProviderExecutionConfig::OpenAiCodexResponses(_) => {
                self.start_real_run(request)
            }
        }
    }

    fn continue_run(&self, request: ContinueRunRequest) -> CoreResult<RunSnapshot> {
        match &self.provider {
            HeadlessProviderExecutionConfig::Fake => {
                FakeProviderRuntime::new(self.store.clone()).continue_run(request)
            }
            HeadlessProviderExecutionConfig::OpenAiCompatible(_)
            | HeadlessProviderExecutionConfig::OpenAiCodexResponses(_) => {
                self.continue_real_run(request)
            }
        }
    }

    fn submit_user_input(&self, request: UserInputRequest) -> CoreResult<RunSnapshot> {
        self.continue_run(ContinueRunRequest {
            project_id: request.project_id,
            run_id: request.run_id,
            prompt: request.text,
        })
    }

    fn approve_action(&self, request: ApprovalDecisionRequest) -> CoreResult<RunSnapshot> {
        crate::validate_required(&request.project_id, "projectId")?;
        crate::validate_required(&request.run_id, "runId")?;
        crate::validate_required(&request.action_id, "actionId")?;
        self.continue_run(ContinueRunRequest {
            project_id: request.project_id,
            run_id: request.run_id,
            prompt: request.response.unwrap_or_else(|| {
                format!(
                    "Approved action `{}` through headless facade.",
                    request.action_id
                )
            }),
        })
    }

    fn reject_action(&self, request: ApprovalDecisionRequest) -> CoreResult<RunSnapshot> {
        crate::validate_required(&request.project_id, "projectId")?;
        crate::validate_required(&request.run_id, "runId")?;
        crate::validate_required(&request.action_id, "actionId")?;
        self.store.append_event(NewRuntimeEvent {
            project_id: request.project_id.clone(),
            run_id: request.run_id.clone(),
            event_kind: RuntimeEventKind::PolicyDecision,
            trace: None,
            payload: json!({
                "kind": "approval",
                "actionId": request.action_id,
                "decision": "rejected",
                "response": request.response,
                "runtime": "headless_facade",
            }),
        })?;
        self.store.load_run(&request.project_id, &request.run_id)
    }

    fn cancel_run(&self, request: CancelRunRequest) -> CoreResult<RunSnapshot> {
        self.store
            .update_run_status(&request.project_id, &request.run_id, RunStatus::Cancelled)
    }

    fn resume_run(&self, request: ResumeRunRequest) -> CoreResult<RunSnapshot> {
        self.continue_run(ContinueRunRequest {
            project_id: request.project_id,
            run_id: request.run_id,
            prompt: request.response,
        })
    }

    fn fork_session(&self, request: ForkSessionRequest) -> CoreResult<RunSnapshot> {
        self.fork_headless_session(request)
    }

    fn compact_session(&self, request: CompactSessionRequest) -> CoreResult<RunSnapshot> {
        self.compact_headless_session(request)
    }

    fn export_trace(&self, request: ExportTraceRequest) -> CoreResult<RuntimeTrace> {
        self.store
            .export_trace(&request.project_id, &request.run_id)
    }
}

#[derive(Debug, Clone)]
struct OpenAiToolCall {
    id: String,
    name: String,
    arguments: JsonValue,
}

impl From<&OpenAiToolCall> for RuntimeProviderToolCallMetadata {
    fn from(call: &OpenAiToolCall) -> Self {
        Self {
            tool_call_id: call.id.clone(),
            provider_tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct HeadlessToolPersistenceGroup<'a> {
    mode: ToolGroupExecutionMode,
    elapsed_ms: u128,
    timeout_error: Option<&'a ToolExecutionError>,
}

#[derive(Debug, Clone)]
struct HeadlessToolResultMessage {
    tool_call_id: String,
    tool_name: String,
    parent_assistant_message_id: String,
    payload: JsonValue,
}

#[derive(Debug, Clone)]
struct HeadlessCommandOutput {
    argv: Vec<String>,
    cwd: String,
    stdout: Option<String>,
    stderr: Option<String>,
    stdout_truncated: bool,
    stderr_truncated: bool,
    exit_code: Option<i32>,
    timed_out: bool,
    elapsed_ms: u64,
    context_epoch: String,
    tool_call_id: String,
}

#[derive(Debug, Clone)]
struct LimitedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

#[derive(Debug, Clone)]
struct OpenAiProviderMessage {
    content: JsonValue,
    tool_calls: Vec<OpenAiToolCall>,
    /// Reasoning ("thinking") text accumulated from the provider stream.
    /// Empty for providers that don't expose reasoning summaries.
    reasoning: String,
}

impl OpenAiProviderMessage {
    fn content_text(&self) -> String {
        match &self.content {
            JsonValue::String(text) => text.clone(),
            JsonValue::Array(items) => items
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(JsonValue::as_str)
                        .or_else(|| item.get("content").and_then(JsonValue::as_str))
                })
                .collect::<Vec<_>>()
                .join(""),
            JsonValue::Null => String::new(),
            other => other.to_string(),
        }
    }
}

fn provider_assistant_message_id(run_id: &str, turn_index: usize) -> String {
    format!("provider-assistant-{run_id}-{turn_index}")
}

fn provider_tool_result_message_id(run_id: &str, turn_index: usize, tool_call_id: &str) -> String {
    format!("provider-tool-result-{run_id}-{turn_index}-{tool_call_id}")
}

#[derive(Debug, Clone)]
pub struct HeadlessProductionToolRuntime {
    workspace_root: PathBuf,
    allow_workspace_writes: bool,
    allow_commands: bool,
    allowed_tools: Option<BTreeSet<String>>,
    app_data_roots: Vec<String>,
    todos: Arc<Mutex<BTreeMap<String, JsonValue>>>,
}

impl HeadlessProductionToolRuntime {
    pub fn new(
        workspace_root: Option<&PathBuf>,
        allow_workspace_writes: bool,
        app_data_roots: Vec<String>,
    ) -> CoreResult<Self> {
        Self::new_with_modes(
            workspace_root,
            allow_workspace_writes,
            allow_workspace_writes,
            app_data_roots,
        )
    }

    pub fn new_with_modes(
        workspace_root: Option<&PathBuf>,
        allow_workspace_writes: bool,
        allow_commands: bool,
        app_data_roots: Vec<String>,
    ) -> CoreResult<Self> {
        let workspace_root = workspace_root.ok_or_else(|| {
            CoreError::invalid_request(
                "agent_core_headless_workspace_missing",
                "Production Tool Registry V2 dispatch requires a registered workspace root.",
            )
        })?;
        let workspace_root = fs::canonicalize(workspace_root).map_err(|error| {
            CoreError::invalid_request(
                "agent_core_headless_workspace_unavailable",
                format!(
                    "Workspace root `{}` is unavailable: {error}",
                    workspace_root.display()
                ),
            )
        })?;
        Ok(Self {
            workspace_root,
            allow_workspace_writes,
            allow_commands,
            allowed_tools: None,
            app_data_roots,
            todos: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    fn with_allowed_tools(mut self, allowed_tools: Option<BTreeSet<String>>) -> Self {
        self.allowed_tools = allowed_tools;
        self
    }

    fn with_todo_state(mut self, todos: Arc<Mutex<BTreeMap<String, JsonValue>>>) -> Self {
        self.todos = todos;
        self
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptorV2> {
        let mut descriptors = vec![
            headless_read_descriptor(),
            headless_list_descriptor(),
            headless_todo_descriptor(),
        ];
        if self.allow_workspace_writes {
            descriptors.push(headless_write_descriptor());
            descriptors.push(headless_patch_descriptor());
            descriptors.push(headless_delete_descriptor());
            descriptors.push(headless_move_descriptor());
            descriptors.push(headless_replace_descriptor());
        }
        if self.allow_commands {
            descriptors.push(headless_command_descriptor());
        }
        if let Some(allowed_tools) = self.allowed_tools.as_ref() {
            descriptors.retain(|descriptor| allowed_tools.contains(&descriptor.name));
        }
        descriptors
    }

    fn tool_names(&self) -> Vec<String> {
        self.descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name)
            .collect()
    }

    fn openai_tool_definitions(&self) -> Vec<JsonValue> {
        self.descriptors()
            .into_iter()
            .map(openai_tool_definition_from_descriptor)
            .collect()
    }

    fn openai_response_tool_definitions(&self) -> Vec<JsonValue> {
        self.descriptors()
            .into_iter()
            .map(openai_response_tool_definition_from_descriptor)
            .collect()
    }

    pub fn dispatch_batch(
        &self,
        project_id: &str,
        run_id: &str,
        turn_index: usize,
        inputs: &[ToolCallInput],
    ) -> CoreResult<ToolBatchDispatchReport> {
        let registry = self.build_registry()?;
        let budget = ToolBudget {
            max_command_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            ..ToolBudget::default()
        };
        let config = ToolDispatchConfig {
            budget,
            policy: Arc::new(HeadlessProductionToolPolicy {
                allow_workspace_writes: self.allow_workspace_writes,
                allow_commands: self.allow_commands,
                allowed_tools: self.allowed_tools.clone(),
            }),
            sandbox: Arc::new(PermissionProfileSandbox::new(SandboxExecutionContext {
                workspace_root: self.workspace_root.display().to_string(),
                app_data_roots: self.app_data_roots.clone(),
                project_trust: ProjectTrustState::Trusted,
                approval_source: if self.allow_workspace_writes || self.allow_commands {
                    SandboxApprovalSource::Policy
                } else {
                    SandboxApprovalSource::None
                },
                platform: SandboxPlatform::current(),
                preserved_environment_keys: vec!["PATH".into()],
                ..SandboxExecutionContext::default()
            })),
            rollback: Some(Arc::new(HeadlessFileRollback {
                workspace_root: self.workspace_root.clone(),
            })),
            context: ToolExecutionContext {
                project_id: project_id.into(),
                run_id: run_id.into(),
                turn_index,
                context_epoch: format!("turn-{turn_index}"),
                telemetry_attributes: BTreeMap::from([
                    (
                        "xero.dispatch.path".into(),
                        "headless_production_provider_loop".into(),
                    ),
                    ("xero.dispatch.registry".into(), "tool_registry_v2".into()),
                    (
                        MUTATION_EXECUTION_SCOPE_ATTRIBUTE.into(),
                        self.workspace_root.to_string_lossy().into_owned(),
                    ),
                ]),
            },
            cancellation_check: None,
        };
        Ok(registry.dispatch_batch(inputs, &config))
    }

    fn build_registry(&self) -> CoreResult<ToolRegistryV2> {
        let mut registry = ToolRegistryV2::new();

        let read_runtime = self.clone();
        registry
            .register(StaticToolHandler::new_cancellable(
                headless_read_descriptor(),
                move |_context, call, control| {
                    control.ensure_not_cancelled(&call.tool_name)?;
                    let output = read_runtime.read(call)?;
                    control.ensure_not_cancelled(&call.tool_name)?;
                    Ok(output)
                },
            ))
            .map_err(tool_execution_error_to_core_error)?;

        let list_runtime = self.clone();
        registry
            .register(StaticToolHandler::new_cancellable(
                headless_list_descriptor(),
                move |_context, call, control| {
                    control.ensure_not_cancelled(&call.tool_name)?;
                    let output = list_runtime.list(call)?;
                    control.ensure_not_cancelled(&call.tool_name)?;
                    Ok(output)
                },
            ))
            .map_err(tool_execution_error_to_core_error)?;

        let todo_runtime = self.clone();
        registry
            .register(StaticToolHandler::new_cancellable(
                headless_todo_descriptor(),
                move |_context, call, control| {
                    control.ensure_not_cancelled(&call.tool_name)?;
                    let output = todo_runtime.todo(call)?;
                    control.ensure_not_cancelled(&call.tool_name)?;
                    Ok(output)
                },
            ))
            .map_err(tool_execution_error_to_core_error)?;

        if self.allow_workspace_writes {
            let write_runtime = self.clone();
            registry
                .register(StaticToolHandler::new_cancellable(
                    headless_write_descriptor(),
                    move |context, call, control| {
                        control.ensure_not_cancelled(&call.tool_name)?;
                        let output = write_runtime.write(context, call)?;
                        control.ensure_not_cancelled(&call.tool_name)?;
                        Ok(output)
                    },
                ))
                .map_err(tool_execution_error_to_core_error)?;

            let patch_runtime = self.clone();
            registry
                .register(StaticToolHandler::new_cancellable(
                    headless_patch_descriptor(),
                    move |context, call, control| {
                        control.ensure_not_cancelled(&call.tool_name)?;
                        let output = patch_runtime.apply_patch(context, call)?;
                        control.ensure_not_cancelled(&call.tool_name)?;
                        Ok(output)
                    },
                ))
                .map_err(tool_execution_error_to_core_error)?;

            let delete_runtime = self.clone();
            registry
                .register(StaticToolHandler::new_cancellable(
                    headless_delete_descriptor(),
                    move |context, call, control| {
                        control.ensure_not_cancelled(&call.tool_name)?;
                        let output = delete_runtime.delete(context, call)?;
                        control.ensure_not_cancelled(&call.tool_name)?;
                        Ok(output)
                    },
                ))
                .map_err(tool_execution_error_to_core_error)?;

            let move_runtime = self.clone();
            registry
                .register(StaticToolHandler::new_cancellable(
                    headless_move_descriptor(),
                    move |context, call, control| {
                        control.ensure_not_cancelled(&call.tool_name)?;
                        let output = move_runtime.move_path(context, call)?;
                        control.ensure_not_cancelled(&call.tool_name)?;
                        Ok(output)
                    },
                ))
                .map_err(tool_execution_error_to_core_error)?;

            let replace_runtime = self.clone();
            registry
                .register(StaticToolHandler::new_cancellable(
                    headless_replace_descriptor(),
                    move |context, call, control| {
                        control.ensure_not_cancelled(&call.tool_name)?;
                        let output = replace_runtime.replace_text(context, call)?;
                        control.ensure_not_cancelled(&call.tool_name)?;
                        Ok(output)
                    },
                ))
                .map_err(tool_execution_error_to_core_error)?;
        }

        if self.allow_commands {
            let command_runtime = self.clone();
            registry
                .register(StaticToolHandler::new_cancellable(
                    headless_command_descriptor(),
                    move |context, call, control| {
                        control.ensure_not_cancelled(&call.tool_name)?;
                        let output = command_runtime.command(context, call)?;
                        control.ensure_not_cancelled(&call.tool_name)?;
                        Ok(output)
                    },
                ))
                .map_err(tool_execution_error_to_core_error)?;
        }

        Ok(registry)
    }

    fn read(&self, call: &ToolCallInput) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let path = required_tool_string(&call.input, "path")?;
        let resolved = resolve_workspace_path_for_root(&self.workspace_root, path, false)
            .map_err(core_error_to_tool_execution_error)?;
        let content = fs::read_to_string(&resolved).map_err(|error| {
            ToolExecutionError::retryable(
                "agent_core_headless_read_failed",
                format!("Xero could not read `{path}`: {error}"),
            )
        })?;
        Ok(ToolHandlerOutput::new(
            format!("Read `{path}` through Tool Registry V2."),
            json!({
                "ok": true,
                "path": path,
                "content": truncate_text(&content, MAX_TOOL_OUTPUT_BYTES),
                "truncated": content.len() > MAX_TOOL_OUTPUT_BYTES,
            }),
        ))
    }

    fn todo(&self, call: &ToolCallInput) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let action = required_tool_string(&call.input, "action")?;
        let mut todos = self.todos.lock().map_err(|_| {
            ToolExecutionError::unavailable(
                "agent_core_headless_todo_state_failed",
                "Xero could not lock headless todo state.",
            )
        })?;
        let mut changed_item = JsonValue::Null;
        match action {
            "list" => {}
            "upsert" => {
                let id = call
                    .input
                    .get("id")
                    .and_then(JsonValue::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("todo-{}", todos.len().saturating_add(1)));
                let existing = todos.get(&id);
                let title = call
                    .input
                    .get("title")
                    .and_then(JsonValue::as_str)
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                    .map(str::to_owned)
                    .or_else(|| {
                        existing
                            .and_then(|item| item["title"].as_str())
                            .map(str::to_owned)
                    })
                    .ok_or_else(|| {
                        ToolExecutionError::invalid_input(
                            "agent_core_headless_todo_title_missing",
                            "Todo upsert requires a non-empty title.",
                        )
                    })?;
                let status = call
                    .input
                    .get("status")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("pending");
                let item = json!({
                    "id": id,
                    "title": title,
                    "notes": call.input.get("notes").cloned().unwrap_or(JsonValue::Null),
                    "status": status,
                });
                todos.insert(id, item.clone());
                changed_item = item;
            }
            "complete" => {
                let id = required_tool_string(&call.input, "id")?;
                let item = todos.get_mut(id).ok_or_else(|| {
                    ToolExecutionError::invalid_input(
                        "agent_core_headless_todo_not_found",
                        format!("Xero could not find todo `{id}`."),
                    )
                })?;
                item["status"] = json!("completed");
                changed_item = item.clone();
            }
            "delete" => {
                let id = required_tool_string(&call.input, "id")?;
                changed_item = todos.remove(id).unwrap_or(JsonValue::Null);
            }
            "clear" => todos.clear(),
            _ => {
                return Err(ToolExecutionError::invalid_input(
                    "agent_core_headless_todo_action_invalid",
                    "Todo action must be list, upsert, complete, delete, or clear.",
                ));
            }
        }
        let items = todos.values().cloned().collect::<Vec<_>>();
        Ok(ToolHandlerOutput::new(
            format!("Todo action `{action}` returned {} item(s).", items.len()),
            json!({
                "action": action,
                "items": items,
                "changedItem": changed_item,
            }),
        ))
    }

    fn list(&self, call: &ToolCallInput) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let prefix = call
            .input
            .get("path")
            .and_then(JsonValue::as_str)
            .unwrap_or(".");
        let start = resolve_workspace_path_for_root(&self.workspace_root, prefix, false)
            .map_err(core_error_to_tool_execution_error)?;
        let listing = collect_workspace_listing(&self.workspace_root, &start, HEADLESS_LIST_LIMIT)
            .map_err(core_error_to_tool_execution_error)?;
        Ok(ToolHandlerOutput::new(
            format!("Listed files below `{prefix}` through Tool Registry V2."),
            json!({
                "ok": true,
                "root": self.workspace_root.display().to_string(),
                "path": prefix,
                "directories": listing.directories,
                "files": listing.files,
                "entries": listing.entries,
                "skippedDirectories": listing.skipped_directories,
                "omittedEntryCount": listing.omitted_entry_count,
                "truncated": listing.truncated,
            }),
        ))
    }

    fn write(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let path = required_tool_string(&call.input, "path")?;
        let content = required_tool_string_allow_empty(&call.input, "content")?;
        let resolved = resolve_workspace_path_for_root(&self.workspace_root, path, true)
            .map_err(core_error_to_tool_execution_error)?;
        let rollback = rollback_checkpoint_metadata(path, &resolved);
        let file_reservation = file_reservation_metadata(context, call, path);
        if let Some(parent) = resolved.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_write_prepare_failed",
                    format!(
                        "Xero could not prepare `{}` for writing: {error}",
                        parent.display()
                    ),
                )
            })?;
        }
        fs::write(&resolved, content.as_bytes()).map_err(|error| {
            ToolExecutionError::retryable(
                "agent_core_headless_write_failed",
                format!("Xero could not write `{path}`: {error}"),
            )
        })?;
        Ok(ToolHandlerOutput::new(
            format!("Wrote `{path}` through Tool Registry V2."),
            json!({
                "ok": true,
                "path": path,
                "bytes": content.len(),
                "rollback": rollback,
                "fileReservation": file_reservation,
            }),
        ))
    }

    fn apply_patch(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let patch = required_tool_string(&call.input, "patch")?;
        if patch.trim().is_empty() {
            return Err(ToolExecutionError::invalid_input(
                "agent_core_headless_patch_empty",
                "Patch input cannot be empty.",
            ));
        }
        let changed_files = patch_changed_paths(patch);
        for path in &changed_files {
            let _ = resolve_workspace_path_for_root(&self.workspace_root, path, true)
                .map_err(core_error_to_tool_execution_error)?;
        }

        let check = self.run_git_apply(context, call, patch, true)?;
        if check.exit_code != Some(0) {
            return Err(ToolExecutionError::invalid_input(
                "agent_core_headless_patch_check_failed",
                format!(
                    "Patch did not apply cleanly: {}",
                    check.stderr.as_deref().unwrap_or_default()
                ),
            ));
        }
        let applied = self.run_git_apply(context, call, patch, false)?;
        if applied.exit_code != Some(0) {
            return Err(ToolExecutionError::retryable(
                "agent_core_headless_patch_apply_failed",
                format!(
                    "Patch application failed: {}",
                    applied.stderr.as_deref().unwrap_or_default()
                ),
            ));
        }

        Ok(ToolHandlerOutput::new(
            format!("Applied patch touching {} file(s).", changed_files.len()),
            json!({
                "ok": true,
                "changedFiles": changed_files,
                "stdout": truncate_text(applied.stdout.as_deref().unwrap_or_default(), MAX_TOOL_OUTPUT_BYTES),
                "stderr": truncate_text(applied.stderr.as_deref().unwrap_or_default(), MAX_TOOL_OUTPUT_BYTES),
                "exitCode": applied.exit_code,
                "patchBytes": patch.len(),
                "patchRedacted": true,
            }),
        ))
    }

    fn delete(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let path = required_tool_string(&call.input, "path")?;
        let recursive = call
            .input
            .get("recursive")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);
        let resolved = resolve_workspace_path_for_root(&self.workspace_root, path, false)
            .map_err(core_error_to_tool_execution_error)?;
        let metadata = fs::metadata(&resolved).map_err(|error| {
            ToolExecutionError::retryable(
                "agent_core_headless_delete_metadata_failed",
                format!("Xero could not inspect `{path}` before deleting it: {error}"),
            )
        })?;
        let rollback = rollback_checkpoint_metadata(path, &resolved);
        let file_reservation = file_reservation_metadata(context, call, path);
        let kind = if metadata.is_dir() {
            "directory"
        } else {
            "file"
        };
        if metadata.is_dir() {
            if !recursive {
                return Err(ToolExecutionError::invalid_input(
                    "agent_core_headless_delete_directory_requires_recursive",
                    format!("Refusing to delete directory `{path}` without recursive=true."),
                ));
            }
            fs::remove_dir_all(&resolved).map_err(|error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_delete_failed",
                    format!("Xero could not recursively delete `{path}`: {error}"),
                )
            })?;
        } else {
            fs::remove_file(&resolved).map_err(|error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_delete_failed",
                    format!("Xero could not delete `{path}`: {error}"),
                )
            })?;
        }
        Ok(ToolHandlerOutput::new(
            format!("Deleted `{path}` through Tool Registry V2."),
            json!({
                "ok": true,
                "path": path,
                "kind": kind,
                "recursive": recursive,
                "rollback": rollback,
                "fileReservation": file_reservation,
            }),
        ))
    }

    fn move_path(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let from = required_tool_string(&call.input, "from")?;
        let to = required_tool_string(&call.input, "to")?;
        if from == to {
            return Err(ToolExecutionError::invalid_input(
                "agent_core_headless_move_noop",
                "`from` and `to` must be different paths.",
            ));
        }
        let from_resolved = resolve_workspace_path_for_root(&self.workspace_root, from, false)
            .map_err(core_error_to_tool_execution_error)?;
        let to_resolved = resolve_workspace_path_for_root(&self.workspace_root, to, true)
            .map_err(core_error_to_tool_execution_error)?;
        if to_resolved.exists() {
            return Err(ToolExecutionError::invalid_input(
                "agent_core_headless_move_target_exists",
                format!("Refusing to overwrite existing path `{to}`."),
            ));
        }
        let metadata = fs::metadata(&from_resolved).map_err(|error| {
            ToolExecutionError::retryable(
                "agent_core_headless_move_metadata_failed",
                format!("Xero could not inspect `{from}` before moving it: {error}"),
            )
        })?;
        if let Some(parent) = to_resolved.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_move_prepare_failed",
                    format!(
                        "Xero could not prepare `{}` for moving `{from}`: {error}",
                        parent.display()
                    ),
                )
            })?;
        }
        fs::rename(&from_resolved, &to_resolved).map_err(|error| {
            ToolExecutionError::retryable(
                "agent_core_headless_move_failed",
                format!("Xero could not move `{from}` to `{to}`: {error}"),
            )
        })?;
        let kind = if metadata.is_dir() {
            "directory"
        } else {
            "file"
        };
        Ok(ToolHandlerOutput::new(
            format!("Moved `{from}` to `{to}` through Tool Registry V2."),
            json!({
                "ok": true,
                "from": from,
                "to": to,
                "kind": kind,
                "rollback": {
                    "kind": "file_move_rollback",
                    "from": to,
                    "to": from
                },
                "fileReservation": file_reservation_metadata(context, call, from),
            }),
        ))
    }

    fn replace_text(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let root_path = call
            .input
            .get("path")
            .and_then(JsonValue::as_str)
            .unwrap_or(".");
        let search = required_tool_string(&call.input, "search")?;
        let replacement = call
            .input
            .get("replacement")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                ToolExecutionError::invalid_input(
                    "agent_core_headless_replace_missing_replacement",
                    "Missing required string field `replacement`.",
                )
            })?;
        let dry_run = call
            .input
            .get("dryRun")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);
        let max_replacements = call
            .input
            .get("maxReplacements")
            .and_then(JsonValue::as_u64)
            .unwrap_or(500)
            .clamp(1, 5_000) as usize;
        if search.is_empty() {
            return Err(ToolExecutionError::invalid_input(
                "agent_core_headless_replace_empty_search",
                "`search` cannot be empty.",
            ));
        }
        let start = resolve_workspace_path_for_root(&self.workspace_root, root_path, false)
            .map_err(core_error_to_tool_execution_error)?;
        let mut candidates = Vec::new();
        if start.is_dir() {
            candidates = collect_workspace_listing(&self.workspace_root, &start, 2_000)
                .map_err(core_error_to_tool_execution_error)?
                .files;
        } else {
            let relative = start
                .strip_prefix(&self.workspace_root)
                .map_err(|error| {
                    ToolExecutionError::invalid_input(
                        "agent_core_headless_replace_path_invalid",
                        format!("Replacement path `{root_path}` is not in workspace: {error}"),
                    )
                })?
                .to_string_lossy()
                .to_string();
            candidates.push(relative);
        }

        let mut replacements_remaining = max_replacements;
        let mut changed_files = Vec::new();
        let mut total_replacements = 0usize;
        let mut skipped_files = Vec::new();
        for path in candidates {
            if replacements_remaining == 0 {
                break;
            }
            let resolved = resolve_workspace_path_for_root(&self.workspace_root, &path, true)
                .map_err(core_error_to_tool_execution_error)?;
            let Ok(content) = fs::read_to_string(&resolved) else {
                skipped_files.push(json!({
                    "path": path,
                    "reason": "non_utf8_or_unreadable"
                }));
                continue;
            };
            let occurrences = content.matches(search).count();
            if occurrences == 0 {
                continue;
            }
            let replacements_for_file = occurrences.min(replacements_remaining);
            let mut replaced = String::with_capacity(content.len());
            let mut cursor = content.as_str();
            for _ in 0..replacements_for_file {
                let Some(index) = cursor.find(search) else {
                    break;
                };
                replaced.push_str(&cursor[..index]);
                replaced.push_str(replacement);
                cursor = &cursor[index + search.len()..];
            }
            replaced.push_str(cursor);
            let rollback = rollback_checkpoint_metadata(&path, &resolved);
            let file_reservation = file_reservation_metadata(context, call, &path);
            if !dry_run {
                fs::write(&resolved, replaced.as_bytes()).map_err(|error| {
                    ToolExecutionError::retryable(
                        "agent_core_headless_replace_write_failed",
                        format!("Xero could not write replacements to `{path}`: {error}"),
                    )
                })?;
            }
            changed_files.push(json!({
                "path": path,
                "replacements": replacements_for_file,
                "occurrences": occurrences,
                "truncated": occurrences > replacements_for_file,
                "rollback": rollback,
                "fileReservation": file_reservation,
            }));
            total_replacements = total_replacements.saturating_add(replacements_for_file);
            replacements_remaining = replacements_remaining.saturating_sub(replacements_for_file);
        }
        Ok(ToolHandlerOutput::new(
            format!(
                "{} {} replacement(s) across {} file(s) through Tool Registry V2.",
                if dry_run { "Previewed" } else { "Applied" },
                total_replacements,
                changed_files.len()
            ),
            json!({
                "ok": true,
                "path": root_path,
                "dryRun": dry_run,
                "replacements": total_replacements,
                "changedFiles": changed_files,
                "skippedFiles": skipped_files,
                "truncated": replacements_remaining == 0,
                "searchRedacted": true,
                "replacementRedacted": true,
            }),
        ))
    }

    fn run_git_apply(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
        patch: &str,
        check_only: bool,
    ) -> Result<HeadlessCommandOutput, ToolExecutionError> {
        let mut argv = vec![
            "git".to_string(),
            "apply".to_string(),
            "--whitespace=nowarn".to_string(),
        ];
        if check_only {
            argv.push("--check".into());
        }
        argv.push("-".into());
        self.run_process(
            context,
            call,
            argv,
            self.workspace_root.clone(),
            Some(patch.as_bytes()),
            Some(DEFAULT_HEADLESS_COMMAND_TIMEOUT_MS),
        )
    }

    fn command(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let argv = required_tool_string_array(&call.input, "argv")?;
        let cwd = call
            .input
            .get("cwd")
            .and_then(JsonValue::as_str)
            .unwrap_or(".");
        let cwd_path = resolve_workspace_path_for_root(&self.workspace_root, cwd, false)
            .map_err(core_error_to_tool_execution_error)?;
        if !cwd_path.is_dir() {
            return Err(ToolExecutionError::invalid_input(
                "agent_core_headless_command_cwd_invalid",
                format!("Command cwd `{cwd}` is not a directory."),
            ));
        }
        let timeout_ms = call
            .input
            .get("timeoutMs")
            .and_then(JsonValue::as_u64)
            .map(|value| value.clamp(1_000, MAX_HEADLESS_COMMAND_TIMEOUT_MS))
            .or(Some(DEFAULT_HEADLESS_COMMAND_TIMEOUT_MS));
        let output = self.run_process(context, call, argv, cwd_path, None, timeout_ms)?;
        let ok = output.exit_code == Some(0) && !output.timed_out;
        Ok(ToolHandlerOutput::new(
            if ok {
                "Command completed successfully through Tool Registry V2."
            } else {
                "Command completed with a non-zero status through Tool Registry V2."
            },
            json!({
                "ok": ok,
                "argv": output.argv,
                "cwd": output.cwd,
                "stdout": truncate_text(output.stdout.as_deref().unwrap_or_default(), MAX_TOOL_OUTPUT_BYTES),
                "stderr": truncate_text(output.stderr.as_deref().unwrap_or_default(), MAX_TOOL_OUTPUT_BYTES),
                "stdoutTruncated": output.stdout_truncated,
                "stderrTruncated": output.stderr_truncated,
                "exitCode": output.exit_code,
                "timedOut": output.timed_out,
                "elapsedMs": output.elapsed_ms,
                "contextEpoch": output.context_epoch,
                "toolCallId": output.tool_call_id,
            }),
        ))
    }

    fn run_process(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
        argv: Vec<String>,
        cwd: PathBuf,
        stdin: Option<&[u8]>,
        timeout_ms: Option<u64>,
    ) -> Result<HeadlessCommandOutput, ToolExecutionError> {
        validate_headless_argv(&argv)?;
        let mut command = Command::new(&argv[0]);
        command
            .args(argv.iter().skip(1))
            .current_dir(&cwd)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Run in its own process group so a timeout/kill terminates the whole tree, not just the
        // direct child. Otherwise a backgrounded grandchild keeps the stdout pipe open and the
        // capture-thread join below blocks forever, hanging the tool call and the agent run.
        crate::sandbox::configure_sandboxed_process_group(&mut command);
        let mut child = command.spawn().map_err(|error| {
            ToolExecutionError::retryable(
                "agent_core_headless_command_spawn_failed",
                format!("Xero could not launch `{}`: {error}", argv[0]),
            )
        })?;
        let child_id = child.id();

        // Take the pipes and start draining stdout/stderr BEFORE writing stdin. Writing the full
        // stdin payload first (as before) deadlocks when the child emits more than one pipe
        // buffer (~64 KB) of output while we are still blocked on the stdin write — the child
        // blocks writing stdout and we block writing stdin.
        let stdout = child.stdout.take().ok_or_else(|| {
            ToolExecutionError::retryable(
                "agent_core_headless_command_stdout_missing",
                "Xero could not capture command stdout.",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ToolExecutionError::retryable(
                "agent_core_headless_command_stderr_missing",
                "Xero could not capture command stderr.",
            )
        })?;
        let stdout_handle =
            thread::spawn(move || read_limited_output(stdout, MAX_TOOL_OUTPUT_BYTES));
        let stderr_handle =
            thread::spawn(move || read_limited_output(stderr, MAX_TOOL_OUTPUT_BYTES));

        if let Some(bytes) = stdin {
            let mut child_stdin = child.stdin.take().ok_or_else(|| {
                crate::sandbox::cleanup_sandboxed_process_group(child_id);
                ToolExecutionError::retryable(
                    "agent_core_headless_command_stdin_missing",
                    "Xero could not open stdin for the command.",
                )
            })?;
            child_stdin.write_all(bytes).map_err(|error| {
                crate::sandbox::cleanup_sandboxed_process_group(child_id);
                ToolExecutionError::retryable(
                    "agent_core_headless_command_stdin_failed",
                    format!("Xero could not write command stdin: {error}"),
                )
            })?;
            // Close stdin so the child sees EOF (dropping `child_stdin`).
        }

        let started_at = Instant::now();
        let timeout =
            Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_HEADLESS_COMMAND_TIMEOUT_MS));
        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started_at.elapsed() >= timeout => {
                    timed_out = true;
                    crate::sandbox::cleanup_sandboxed_process_group(child_id);
                    break child.wait().map_err(|error| {
                        ToolExecutionError::retryable(
                            "agent_core_headless_command_wait_failed",
                            format!("Xero could not wait for a timed-out command: {error}"),
                        )
                    })?;
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    crate::sandbox::cleanup_sandboxed_process_group(child_id);
                    return Err(ToolExecutionError::retryable(
                        "agent_core_headless_command_wait_failed",
                        format!("Xero could not observe command execution: {error}"),
                    ));
                }
            }
        };
        // Reap any surviving grandchildren so their inherited stdout write-ends close and the
        // capture threads below can reach EOF and join instead of hanging.
        crate::sandbox::cleanup_sandboxed_process_group(child_id);
        let stdout = stdout_handle.join().map_err(|_| {
            ToolExecutionError::retryable(
                "agent_core_headless_command_stdout_failed",
                "Xero could not join stdout capture.",
            )
        })?;
        let stderr = stderr_handle.join().map_err(|_| {
            ToolExecutionError::retryable(
                "agent_core_headless_command_stderr_failed",
                "Xero could not join stderr capture.",
            )
        })?;

        Ok(HeadlessCommandOutput {
            argv,
            cwd: cwd.display().to_string(),
            stdout: Some(String::from_utf8_lossy(&stdout.bytes).into_owned()),
            stderr: Some(String::from_utf8_lossy(&stderr.bytes).into_owned()),
            stdout_truncated: stdout.truncated,
            stderr_truncated: stderr.truncated,
            exit_code: status.code(),
            timed_out,
            elapsed_ms: started_at.elapsed().as_millis() as u64,
            context_epoch: context.context_epoch.clone(),
            tool_call_id: call.tool_call_id.clone(),
        })
    }
}

#[derive(Debug, Clone)]
struct HeadlessProductionToolPolicy {
    allow_workspace_writes: bool,
    allow_commands: bool,
    allowed_tools: Option<BTreeSet<String>>,
}

impl ToolPolicy for HeadlessProductionToolPolicy {
    fn evaluate(&self, descriptor: &ToolDescriptorV2, _call: &ToolCallInput) -> ToolPolicyDecision {
        if self
            .allowed_tools
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(&descriptor.name))
        {
            return ToolPolicyDecision::Deny {
                code: "agent_core_headless_agent_tool_denied".into(),
                message: format!(
                    "The selected Agent definition does not allow headless tool `{}`.",
                    descriptor.name
                ),
            };
        }
        if descriptor.name == HEADLESS_TOOL_COMMAND && !self.allow_commands {
            return ToolPolicyDecision::Deny {
                code: "agent_core_headless_command_not_approved".into(),
                message: "Headless command execution is disabled for this run.".into(),
            };
        }
        if descriptor.name != HEADLESS_TOOL_COMMAND
            && descriptor.mutability == ToolMutability::Mutating
            && !self.allow_workspace_writes
        {
            return ToolPolicyDecision::Deny {
                code: "agent_core_headless_write_not_approved".into(),
                message: "Headless real-provider writes are disabled for this run.".into(),
            };
        }
        ToolPolicyDecision::Allow
    }
}

#[derive(Debug, Clone)]
struct HeadlessFileRollback {
    workspace_root: PathBuf,
}

impl ToolRollback for HeadlessFileRollback {
    fn checkpoint_before(
        &self,
        call: &ToolCallInput,
        descriptor: &ToolDescriptorV2,
    ) -> Result<Option<JsonValue>, ToolExecutionError> {
        if descriptor.name != HEADLESS_TOOL_WRITE {
            return Ok(None);
        }
        let path = required_tool_string(&call.input, "path")?;
        let resolved = resolve_workspace_path_for_root(&self.workspace_root, path, true)
            .map_err(core_error_to_tool_execution_error)?;
        let bytes = fs::read(&resolved).ok();
        Ok(Some(json!({
            "kind": "file_rollback_checkpoint",
            "path": path,
            "existed": bytes.is_some(),
            "contentBytes": bytes,
        })))
    }

    fn rollback_after_failure(
        &self,
        call: &ToolCallInput,
        descriptor: &ToolDescriptorV2,
        checkpoint: &JsonValue,
        error: &ToolExecutionError,
    ) -> Result<JsonValue, ToolExecutionError> {
        if descriptor.name != HEADLESS_TOOL_WRITE {
            return Ok(json!({ "kind": "rollback_not_required" }));
        }
        let path = required_tool_string(&call.input, "path")?;
        let resolved = resolve_workspace_path_for_root(&self.workspace_root, path, true)
            .map_err(core_error_to_tool_execution_error)?;
        let existed = checkpoint
            .get("existed")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);
        if existed {
            let bytes = checkpoint
                .get("contentBytes")
                .and_then(JsonValue::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(JsonValue::as_u64)
                        .filter_map(|value| u8::try_from(value).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            fs::write(&resolved, bytes).map_err(|write_error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_rollback_failed",
                    format!("Xero could not restore `{path}` after tool failure: {write_error}"),
                )
            })?;
        } else if resolved.exists() {
            fs::remove_file(&resolved).map_err(|remove_error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_rollback_failed",
                    format!(
                        "Xero could not remove newly-created `{path}` after tool failure: {remove_error}"
                    ),
                )
            })?;
        }
        Ok(json!({
            "kind": "file_rollback",
            "path": path,
            "restored": true,
            "triggerErrorCode": error.code,
        }))
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: JsonValue,
}

fn send_openai_compatible_chat(
    client: &Client,
    config: &OpenAiCompatibleHeadlessConfig,
    messages: &[JsonValue],
    tools: Vec<JsonValue>,
    thinking_effort: Option<&str>,
) -> CoreResult<OpenAiProviderMessage> {
    let url = openai_compatible_chat_url(&config.base_url)?;
    let mut body = json!({
        "model": config.model_id,
        "messages": messages,
        "tools": tools,
        "tool_choice": "auto",
        "stream": false,
    });
    if let Some(effort) = thinking_effort {
        if config.provider_id == "openai_api" {
            body.as_object_mut()
                .expect("OpenAI-compatible request body is an object")
                .insert("reasoning_effort".into(), json!(effort));
        } else if config.provider_id == "deepseek" {
            body.as_object_mut()
                .expect("OpenAI-compatible request body is an object")
                .insert("thinking".into(), json!({ "type": "enabled" }));
            body.as_object_mut()
                .expect("OpenAI-compatible request body is an object")
                .insert(
                    "reasoning_effort".into(),
                    json!(deepseek_headless_effort(effort)),
                );
        } else if config.provider_id == "openrouter" {
            body.as_object_mut()
                .expect("OpenAI-compatible request body is an object")
                .insert("reasoning".into(), json!({ "effort": effort }));
        }
    }
    let mut request = client.post(url).json(&body);
    if let Some(api_key) = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().map_err(|error| {
        CoreError::system_fault(
            "agent_core_provider_request_failed",
            format!(
                "Headless provider `{}` request failed: {error}",
                config.provider_id
            ),
        )
    })?;
    let status = response.status();
    let text = response.text().map_err(|error| {
        CoreError::system_fault(
            "agent_core_provider_response_read_failed",
            format!("Xero could not read provider response body: {error}"),
        )
    })?;
    if !status.is_success() {
        return Err(CoreError::invalid_request(
            "agent_core_provider_status_failed",
            format!(
                "Headless provider `{}` returned HTTP {}: {}",
                config.provider_id,
                status.as_u16(),
                truncate_text(&text, 2048)
            ),
        ));
    }
    let decoded = serde_json::from_str::<ChatCompletionResponse>(&text).map_err(|error| {
        CoreError::system_fault(
            "agent_core_provider_response_decode_failed",
            format!("Xero could not decode provider response JSON: {error}"),
        )
    })?;
    let message = decoded
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message)
        .ok_or_else(|| {
            CoreError::invalid_request(
                "agent_core_provider_choice_missing",
                "Provider response did not include a completion choice.",
            )
        })?;
    Ok(OpenAiProviderMessage {
        content: message.get("content").cloned().unwrap_or(JsonValue::Null),
        tool_calls: parse_openai_tool_calls(&message)?,
        reasoning: String::new(),
    })
}

#[derive(Debug, Default)]
struct PartialOpenAiResponseToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

fn send_openai_codex_responses<F>(
    client: &Client,
    config: &OpenAiCodexHeadlessConfig,
    messages: &[JsonValue],
    tools: Vec<JsonValue>,
    thinking_effort: Option<&str>,
    on_progress: F,
) -> CoreResult<OpenAiProviderMessage>
where
    F: FnMut(&str, &str),
{
    let url = openai_codex_responses_url(&config.base_url)?;
    let mut body = json!({
        "model": config.model_id,
        "store": false,
        "stream": true,
        "instructions": openai_codex_instructions(messages),
        "input": openai_codex_response_input(messages)?,
        "text": { "verbosity": "medium" },
        "include": ["reasoning.encrypted_content"],
        "tool_choice": "auto",
        "parallel_tool_calls": true,
    });
    if !tools.is_empty() {
        body.as_object_mut()
            .expect("OpenAI Codex request body is an object")
            .insert("tools".into(), JsonValue::Array(tools));
    }
    if let Some(effort) = thinking_effort {
        body.as_object_mut()
            .expect("OpenAI Codex request body is an object")
            .insert(
                "reasoning".into(),
                json!({
                    "effort": clamp_openai_codex_headless_effort(&config.model_id, effort),
                    "summary": "auto",
                }),
            );
    }
    let mut request = client
        .post(url)
        .bearer_auth(config.access_token.trim())
        .header("chatgpt-account-id", config.account_id.trim())
        .header("OpenAI-Beta", "responses=experimental")
        .header("originator", "pi")
        .header(
            "user-agent",
            format!("pi ({}; {})", std::env::consts::OS, std::env::consts::ARCH),
        )
        .header("accept", "text/event-stream")
        .json(&body);
    if let Some(session_id) = config
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty())
    {
        request = request.header("session_id", session_id);
    }
    let response = request.send().map_err(|error| {
        CoreError::system_fault(
            "agent_core_provider_request_failed",
            format!(
                "Headless provider `{}` request failed: {error}",
                config.provider_id
            ),
        )
    })?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().unwrap_or_else(|_| String::new());
        return Err(CoreError::invalid_request(
            "agent_core_provider_status_failed",
            format!(
                "Headless provider `{}` returned HTTP {}: {}",
                config.provider_id,
                status.as_u16(),
                truncate_text(&text, 2048)
            ),
        ));
    }
    parse_openai_codex_responses_sse(&config.provider_id, response, on_progress)
}

fn openai_codex_responses_url(base_url: &str) -> CoreResult<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(CoreError::invalid_request(
            "agent_core_provider_base_url_missing",
            "A provider base URL is required for headless OpenAI OAuth execution.",
        ));
    }
    if trimmed.starts_with("http://") && !is_local_http_endpoint(trimmed) {
        return Err(CoreError::invalid_request(
            "agent_core_provider_base_url_insecure",
            "Headless OpenAI OAuth HTTP endpoints are only allowed for localhost.",
        ));
    }
    let url = if trimmed.ends_with("/codex/responses") {
        trimmed.to_owned()
    } else if trimmed.ends_with("/codex") {
        format!("{trimmed}/responses")
    } else {
        format!("{trimmed}/codex/responses")
    };
    Ok(url)
}

fn openai_codex_instructions(messages: &[JsonValue]) -> String {
    messages
        .iter()
        .find(|message| message.get("role").and_then(JsonValue::as_str) == Some("system"))
        .and_then(|message| message.get("content").and_then(JsonValue::as_str))
        .unwrap_or_default()
        .to_string()
}

fn openai_codex_response_input(messages: &[JsonValue]) -> CoreResult<Vec<JsonValue>> {
    let mut input = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        match message
            .get("role")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
        {
            "system" => {}
            "user" => {
                let content = message
                    .get("content")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default();
                input.push(json!({
                    "role": "user",
                    "content": [{ "type": "input_text", "text": content }],
                }));
            }
            "assistant" => {
                if let Some(content) = message
                    .get("content")
                    .and_then(JsonValue::as_str)
                    .filter(|content| !content.trim().is_empty())
                {
                    input.push(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": content,
                            "annotations": [],
                        }],
                        "status": "completed",
                        "id": format!("msg_{index}"),
                    }));
                }
                for tool_call in message
                    .get("tool_calls")
                    .and_then(JsonValue::as_array)
                    .into_iter()
                    .flatten()
                {
                    let function = tool_call.get("function").unwrap_or(&JsonValue::Null);
                    input.push(json!({
                        "type": "function_call",
                        "call_id": tool_call.get("id").and_then(JsonValue::as_str).unwrap_or("call"),
                        "name": function.get("name").and_then(JsonValue::as_str).unwrap_or("unknown"),
                        "arguments": function.get("arguments").and_then(JsonValue::as_str).unwrap_or("{}"),
                    }));
                }
            }
            "tool" => {
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": message
                        .get("tool_call_id")
                        .and_then(JsonValue::as_str)
                        .unwrap_or("call"),
                    "output": message
                        .get("content")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default(),
                }));
            }
            other => {
                return Err(CoreError::invalid_request(
                    "agent_core_provider_message_role_invalid",
                    format!("Cannot encode provider message role `{other}` for OpenAI OAuth."),
                ));
            }
        }
    }
    Ok(input)
}

/// Lower bound between progress callbacks. The TUI polls every 200ms, so
/// emitting twice that fast keeps the inline preview fresh without
/// hammering the event log.
const STREAM_PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

fn parse_openai_codex_responses_sse<F>(
    provider_id: &str,
    response: reqwest::blocking::Response,
    mut on_progress: F,
) -> CoreResult<OpenAiProviderMessage>
where
    F: FnMut(&str, &str),
{
    let mut message = String::new();
    let mut reasoning = String::new();
    let mut partial_calls = BTreeMap::<usize, PartialOpenAiResponseToolCall>::new();
    let mut completed_call_count = 0_usize;
    let mut last_progress = Instant::now() - STREAM_PROGRESS_INTERVAL;
    let mut dirty = false;
    for line in BufReader::new(response).lines() {
        let line = line.map_err(|error| {
            CoreError::system_fault(
                "agent_core_provider_stream_read_failed",
                format!("Xero lost the {provider_id} Responses stream: {error}"),
            )
        })?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let value: JsonValue = serde_json::from_str(data).map_err(|error| {
            CoreError::system_fault(
                "agent_core_provider_stream_decode_failed",
                format!("Xero could not decode a {provider_id} Responses chunk: {error}"),
            )
        })?;
        match value
            .get("type")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
        {
            "error" | "response.failed" => {
                return Err(CoreError::invalid_request(
                    "agent_core_provider_response_failed",
                    truncate_text(&value.to_string(), 2048),
                ));
            }
            "response.output_text.delta" => {
                if let Some(delta) = value.get("delta").and_then(JsonValue::as_str) {
                    message.push_str(delta);
                    dirty = true;
                }
            }
            "response.reasoning_summary_text.delta" | "response.reasoning.delta" => {
                if let Some(delta) = value.get("delta").and_then(JsonValue::as_str) {
                    reasoning.push_str(delta);
                    dirty = true;
                }
            }
            "response.reasoning_summary_part.added" => {
                // The model is starting a new summary block. Insert a
                // soft separator so consecutive summaries don't read as
                // one run-on paragraph.
                if !reasoning.is_empty() && !reasoning.ends_with("\n\n") {
                    if reasoning.ends_with('\n') {
                        reasoning.push('\n');
                    } else {
                        reasoning.push_str("\n\n");
                    }
                    dirty = true;
                }
            }
            "response.function_call_arguments.delta" => {
                let index = value
                    .get("output_index")
                    .and_then(JsonValue::as_u64)
                    .unwrap_or(completed_call_count as u64) as usize;
                if let Some(delta) = value.get("delta").and_then(JsonValue::as_str) {
                    partial_calls
                        .entry(index)
                        .or_default()
                        .arguments
                        .push_str(delta);
                }
            }
            "response.output_item.added" => {
                apply_openai_codex_function_call_item(
                    &mut partial_calls,
                    &value,
                    completed_call_count,
                );
            }
            "response.output_item.done" => {
                if apply_openai_codex_function_call_item(
                    &mut partial_calls,
                    &value,
                    completed_call_count,
                ) {
                    completed_call_count = completed_call_count.saturating_add(1);
                }
            }
            _ => {}
        }
        if dirty && last_progress.elapsed() >= STREAM_PROGRESS_INTERVAL {
            on_progress(&message, &reasoning);
            last_progress = Instant::now();
            dirty = false;
        }
    }
    // Flush any tail that arrived inside the last interval.
    if dirty {
        on_progress(&message, &reasoning);
    }
    let tool_calls = partial_calls
        .into_iter()
        .map(|(index, partial)| {
            let name = partial.name.ok_or_else(|| {
                CoreError::invalid_request(
                    "agent_core_provider_tool_name_missing",
                    format!(
                        "Xero received an OpenAI OAuth tool call at index {index} without a name."
                    ),
                )
            })?;
            let id = partial
                .id
                .unwrap_or_else(|| format!("{provider_id}-tool-call-{}", index + 1));
            let arguments = if partial.arguments.trim().is_empty() {
                JsonValue::Object(serde_json::Map::new())
            } else {
                serde_json::from_str(&partial.arguments).map_err(|error| {
                    CoreError::invalid_request(
                        "agent_core_provider_tool_arguments_invalid",
                        format!(
                            "Xero could not decode OpenAI OAuth tool call `{name}` arguments as JSON: {error}"
                        ),
                    )
                })?
            };
            Ok(OpenAiToolCall {
                id,
                name,
                arguments,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(OpenAiProviderMessage {
        content: json!(message),
        tool_calls,
        reasoning,
    })
}

fn apply_openai_codex_function_call_item(
    partial_calls: &mut BTreeMap<usize, PartialOpenAiResponseToolCall>,
    value: &JsonValue,
    fallback_index: usize,
) -> bool {
    let Some(item) = value.get("item") else {
        return false;
    };
    if item.get("type").and_then(JsonValue::as_str) != Some("function_call") {
        return false;
    }
    let index = value
        .get("output_index")
        .and_then(JsonValue::as_u64)
        .unwrap_or(fallback_index as u64) as usize;
    let partial = partial_calls.entry(index).or_default();
    if let Some(call_id) = item.get("call_id").and_then(JsonValue::as_str) {
        partial.id = Some(call_id.to_string());
    }
    if let Some(name) = item.get("name").and_then(JsonValue::as_str) {
        partial.name = Some(name.to_string());
    }
    if partial.arguments.is_empty() {
        if let Some(arguments) = item.get("arguments").and_then(JsonValue::as_str) {
            partial.arguments.push_str(arguments);
        }
    }
    true
}

fn parse_openai_tool_calls(message: &JsonValue) -> CoreResult<Vec<OpenAiToolCall>> {
    let Some(calls) = message.get("tool_calls").and_then(JsonValue::as_array) else {
        return Ok(Vec::new());
    };
    calls
        .iter()
        .map(|call| {
            let id = call
                .get("id")
                .and_then(JsonValue::as_str)
                .unwrap_or("call")
                .to_string();
            let function = call.get("function").ok_or_else(|| {
                CoreError::invalid_request(
                    "agent_core_provider_tool_call_invalid",
                    "Provider tool call was missing its function payload.",
                )
            })?;
            let name = function
                .get("name")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    CoreError::invalid_request(
                        "agent_core_provider_tool_name_missing",
                        "Provider tool call was missing a function name.",
                    )
                })?
                .to_string();
            let raw_arguments = function
                .get("arguments")
                .and_then(JsonValue::as_str)
                .unwrap_or("{}");
            let arguments = serde_json::from_str(raw_arguments).map_err(|error| {
                CoreError::invalid_request(
                    "agent_core_provider_tool_arguments_invalid",
                    format!("Provider tool call `{name}` arguments were not JSON: {error}"),
                )
            })?;
            Ok(OpenAiToolCall {
                id,
                name,
                arguments,
            })
        })
        .collect()
}

pub fn replayable_openai_chat_messages_from_snapshot(snapshot: &RunSnapshot) -> Vec<JsonValue> {
    chat_messages_from_snapshot(snapshot)
}

fn chat_messages_from_snapshot(snapshot: &RunSnapshot) -> Vec<JsonValue> {
    let mut messages = vec![json!({
        "role": "system",
        "content": headless_system_prompt(None),
    })];
    for message in &snapshot.messages {
        match message.role {
            MessageRole::System => {
                messages[0] = json!({ "role": "system", "content": message.content.clone() });
            }
            MessageRole::Developer | MessageRole::User => {
                messages.push(json!({ "role": "user", "content": message.content.clone() }));
            }
            MessageRole::Assistant => {
                messages.push(openai_assistant_message_from_runtime_message(message));
            }
            MessageRole::Tool => {
                messages.push(openai_tool_message_from_runtime_message(message));
            }
        }
    }
    messages
}

fn openai_assistant_message_from_runtime_message(message: &crate::RuntimeMessage) -> JsonValue {
    let Some(metadata) = message.provider_metadata.as_ref() else {
        return json!({ "role": "assistant", "content": message.content.clone() });
    };
    if metadata.assistant_tool_calls.is_empty() {
        return json!({ "role": "assistant", "content": message.content.clone() });
    }
    json!({
        "role": "assistant",
        "content": if message.content.is_empty() { JsonValue::Null } else { json!(message.content.clone()) },
        "tool_calls": metadata.assistant_tool_calls.iter().map(|call| {
            json!({
                "id": call.tool_call_id,
                "type": "function",
                "function": {
                    "name": call.provider_tool_name,
                    "arguments": call.arguments.to_string(),
                }
            })
        }).collect::<Vec<_>>(),
    })
}

fn openai_tool_message_from_runtime_message(message: &crate::RuntimeMessage) -> JsonValue {
    if let Some(tool_result) = message
        .provider_metadata
        .as_ref()
        .and_then(|metadata| metadata.tool_result.as_ref())
    {
        return json!({
            "role": "tool",
            "tool_call_id": tool_result.tool_call_id,
            "content": message.content.clone(),
        });
    }

    let parsed = serde_json::from_str::<JsonValue>(&message.content).unwrap_or(JsonValue::Null);
    if let Some(tool_call_id) = parsed
        .get("toolCallId")
        .or_else(|| parsed.get("tool_call_id"))
        .and_then(JsonValue::as_str)
    {
        return json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": message.content.clone(),
        });
    }

    json!({ "role": "tool", "content": message.content.clone() })
}

fn openai_assistant_message(content: &str, tool_calls: &[OpenAiToolCall]) -> JsonValue {
    json!({
        "role": "assistant",
        "content": if content.is_empty() { JsonValue::Null } else { json!(content) },
        "tool_calls": tool_calls.iter().map(|call| {
            json!({
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": call.arguments.to_string(),
                }
            })
        }).collect::<Vec<_>>(),
    })
}

fn headless_read_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_READ.into(),
        description: "Read a UTF-8 text file from the registered workspace.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        capability_tags: vec!["workspace".into(), "filesystem".into()],
        application_metadata: ToolApplicationMetadata::granular("file"),
        effect_class: ToolEffectClass::FileRead,
        mutability: ToolMutability::ReadOnly,
        sandbox_requirement: ToolSandboxRequirement::ReadOnly,
        approval_requirement: ToolApprovalRequirement::Never,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_file_read".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn headless_list_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_LIST.into(),
        description: "List files below a directory in the registered workspace.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "additionalProperties": false
        }),
        capability_tags: vec!["workspace".into(), "filesystem".into()],
        application_metadata: ToolApplicationMetadata {
            family: "search".into(),
            kind: ToolApplicationKind::ReadOnlyBatch,
            dispatch_safety: ToolBatchDispatchSafety::ParallelReadOnly,
            safety_requirements: vec!["bounded_results".into(), "read_only".into()],
        },
        effect_class: ToolEffectClass::Search,
        mutability: ToolMutability::ReadOnly,
        sandbox_requirement: ToolSandboxRequirement::ReadOnly,
        approval_requirement: ToolApprovalRequirement::Never,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_file_list".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn headless_todo_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_TODO.into(),
        description: "List, upsert, complete, delete, or clear Agent todo items used by Stage gates."
            .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "upsert", "complete", "delete", "clear"]
                },
                "id": { "type": "string" },
                "title": { "type": "string" },
                "notes": { "type": "string" },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed"]
                }
            },
            "required": ["action"],
            "additionalProperties": false
        }),
        capability_tags: vec!["agent_state".into(), "stages".into()],
        application_metadata: ToolApplicationMetadata::granular("todo"),
        effect_class: ToolEffectClass::Metadata,
        mutability: ToolMutability::ReadOnly,
        sandbox_requirement: ToolSandboxRequirement::ReadOnly,
        approval_requirement: ToolApprovalRequirement::Never,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "agent_todo".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: true,
        },
    }
}

fn headless_write_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_WRITE.into(),
        description: "Write a UTF-8 text file inside the registered workspace.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"],
            "additionalProperties": false
        }),
        capability_tags: vec!["workspace".into(), "filesystem".into(), "mutation".into()],
        application_metadata: ToolApplicationMetadata::granular("edit"),
        effect_class: ToolEffectClass::WorkspaceMutation,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::WorkspaceWrite,
        approval_requirement: ToolApprovalRequirement::Policy,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_file_write".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn headless_patch_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_PATCH.into(),
        description: "Apply a unified diff patch inside the registered workspace using git apply."
            .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "patch": { "type": "string" }
            },
            "required": ["patch"],
            "additionalProperties": false
        }),
        capability_tags: vec![
            "workspace".into(),
            "filesystem".into(),
            "patch".into(),
            "mutation".into(),
        ],
        application_metadata: ToolApplicationMetadata {
            family: "edit".into(),
            kind: ToolApplicationKind::Declarative,
            dispatch_safety: ToolBatchDispatchSafety::ToolOwnedAtomic,
            safety_requirements: vec![
                "supports_preview".into(),
                "validates_targets_before_writing".into(),
                "reports_diff".into(),
            ],
        },
        effect_class: ToolEffectClass::WorkspaceMutation,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::WorkspaceWrite,
        approval_requirement: ToolApprovalRequirement::Policy,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_patch_apply".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn headless_delete_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_DELETE.into(),
        description:
            "Delete a file, or recursively delete a directory, inside the registered workspace."
                .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "recursive": { "type": "boolean" }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        capability_tags: vec!["workspace".into(), "filesystem".into(), "mutation".into()],
        application_metadata: ToolApplicationMetadata::granular("edit"),
        effect_class: ToolEffectClass::WorkspaceMutation,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::WorkspaceWrite,
        approval_requirement: ToolApprovalRequirement::Policy,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_file_delete".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn headless_move_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_MOVE.into(),
        description: "Move or rename a file or directory inside the registered workspace.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" }
            },
            "required": ["from", "to"],
            "additionalProperties": false
        }),
        capability_tags: vec!["workspace".into(), "filesystem".into(), "mutation".into()],
        application_metadata: ToolApplicationMetadata::granular("edit"),
        effect_class: ToolEffectClass::WorkspaceMutation,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::WorkspaceWrite,
        approval_requirement: ToolApprovalRequirement::Policy,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_file_move".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn headless_replace_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_REPLACE.into(),
        description: "Replace text in one UTF-8 file or across a bounded workspace subtree.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "search": { "type": "string" },
                "replacement": { "type": "string" },
                "dryRun": { "type": "boolean" },
                "maxReplacements": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 5000
                }
            },
            "required": ["search", "replacement"],
            "additionalProperties": false
        }),
        capability_tags: vec![
            "workspace".into(),
            "filesystem".into(),
            "search".into(),
            "mutation".into(),
        ],
        application_metadata: ToolApplicationMetadata {
            family: "edit".into(),
            kind: ToolApplicationKind::Declarative,
            dispatch_safety: ToolBatchDispatchSafety::ToolOwnedAtomic,
            safety_requirements: vec![
                "bounded_results".into(),
                "supports_dry_run".into(),
                "validates_targets_before_writing".into(),
                "reports_diff".into(),
            ],
        },
        effect_class: ToolEffectClass::WorkspaceMutation,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::WorkspaceWrite,
        approval_requirement: ToolApprovalRequirement::Policy,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_text_replace".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: true,
        },
    }
}

fn headless_command_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_COMMAND.into(),
        description:
            "Run a bounded command in the registered workspace under Xero's command policy.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "argv": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1
                },
                "cwd": { "type": "string" },
                "timeoutMs": {
                    "type": "integer",
                    "minimum": 1000,
                    "maximum": MAX_HEADLESS_COMMAND_TIMEOUT_MS
                }
            },
            "required": ["argv"],
            "additionalProperties": false
        }),
        capability_tags: vec!["workspace".into(), "command".into(), "headless".into()],
        application_metadata: ToolApplicationMetadata::granular("command"),
        effect_class: ToolEffectClass::CommandExecution,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::WorkspaceWrite,
        approval_requirement: ToolApprovalRequirement::Policy,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_command".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn openai_tool_definition_from_descriptor(descriptor: ToolDescriptorV2) -> JsonValue {
    json!({
        "type": "function",
        "function": {
            "name": descriptor.name,
            "description": descriptor.description,
            "parameters": descriptor.input_schema,
        }
    })
}

fn openai_response_tool_definition_from_descriptor(descriptor: ToolDescriptorV2) -> JsonValue {
    json!({
        "type": "function",
        "name": descriptor.name,
        "description": descriptor.description,
        "parameters": descriptor.input_schema,
        "strict": JsonValue::Null,
    })
}

fn redacted_headless_tool_input(tool_name: &str, input: &JsonValue) -> (JsonValue, bool) {
    let Some(object) = input.as_object() else {
        return (input.clone(), false);
    };
    let mut redacted = object.clone();
    match tool_name {
        HEADLESS_TOOL_WRITE => {
            if let Some(content) = object.get("content").and_then(JsonValue::as_str) {
                redacted.insert(
                    "content".into(),
                    json!({
                        "redacted": true,
                        "bytes": content.len(),
                    }),
                );
                return (JsonValue::Object(redacted), true);
            }
        }
        HEADLESS_TOOL_PATCH => {
            if let Some(patch) = object.get("patch").and_then(JsonValue::as_str) {
                redacted.insert(
                    "patch".into(),
                    json!({
                        "redacted": true,
                        "bytes": patch.len(),
                        "changedFiles": patch_changed_paths(patch),
                    }),
                );
                return (JsonValue::Object(redacted), true);
            }
        }
        HEADLESS_TOOL_REPLACE => {
            let mut touched = false;
            if let Some(search) = object.get("search").and_then(JsonValue::as_str) {
                redacted.insert(
                    "search".into(),
                    json!({
                        "redacted": true,
                        "bytes": search.len(),
                    }),
                );
                touched = true;
            }
            if let Some(replacement) = object.get("replacement").and_then(JsonValue::as_str) {
                redacted.insert(
                    "replacement".into(),
                    json!({
                        "redacted": true,
                        "bytes": replacement.len(),
                    }),
                );
                touched = true;
            }
            if touched {
                return (JsonValue::Object(redacted), true);
            }
        }
        _ => {}
    }
    (JsonValue::Object(redacted), false)
}

fn headless_dispatch_success_metadata(
    success: &ToolDispatchSuccess,
    group_mode: ToolGroupExecutionMode,
    group_elapsed_ms: u128,
    timeout_error: Option<&ToolExecutionError>,
) -> JsonValue {
    json!({
        "registryVersion": "tool_registry_v2",
        "providerLoop": "headless_production_provider_loop",
        "groupMode": group_mode,
        "groupElapsedMs": group_elapsed_ms,
        "elapsedMs": success.elapsed_ms,
        "truncation": success.truncation.clone(),
        "sandbox": success.sandbox_metadata.clone(),
        "telemetry": success.telemetry_attributes.clone(),
        "preHook": success.pre_hook_payload.clone(),
        "postHook": success.post_hook_payload.clone(),
        "fileReservation": success.output.get("fileReservation").cloned(),
        "rollback": success.output.get("rollback").cloned(),
        "timeout": timeout_error.map(tool_execution_error_json),
    })
}

fn headless_dispatch_failure_metadata(
    failure: &ToolDispatchFailure,
    group_mode: ToolGroupExecutionMode,
    group_elapsed_ms: u128,
    timeout_error: Option<&ToolExecutionError>,
) -> JsonValue {
    json!({
        "registryVersion": "tool_registry_v2",
        "providerLoop": "headless_production_provider_loop",
        "groupMode": group_mode,
        "groupElapsedMs": group_elapsed_ms,
        "elapsedMs": failure.elapsed_ms,
        "typedErrorCategory": failure.error.category.clone(),
        "modelMessage": failure.error.model_message.clone(),
        "retryable": failure.error.retryable,
        "doomLoopSignal": failure.doom_loop_signal.clone(),
        "rollbackPayload": failure.rollback_payload.clone(),
        "rollbackError": failure.rollback_error.as_ref().map(tool_execution_error_json),
        "sandbox": failure.sandbox_metadata.clone(),
        "preHook": failure.pre_hook_payload.clone(),
        "postHook": failure.post_hook_payload.clone(),
        "timeout": timeout_error.map(tool_execution_error_json),
    })
}

fn tool_execution_error_json(error: &ToolExecutionError) -> JsonValue {
    json!({
        "category": &error.category,
        "code": &error.code,
        "message": &error.message,
        "modelMessage": &error.model_message,
        "retryable": error.retryable,
    })
}

fn required_tool_string<'a>(
    input: &'a JsonValue,
    key: &str,
) -> Result<&'a str, ToolExecutionError> {
    input
        .get(key)
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ToolExecutionError::invalid_input(
                "agent_core_headless_tool_argument_missing",
                format!("Tool argument `{key}` is required."),
            )
        })
}

fn required_tool_string_allow_empty<'a>(
    input: &'a JsonValue,
    key: &str,
) -> Result<&'a str, ToolExecutionError> {
    input.get(key).and_then(JsonValue::as_str).ok_or_else(|| {
        ToolExecutionError::invalid_input(
            "agent_core_headless_tool_argument_missing",
            format!("Tool argument `{key}` must be a string."),
        )
    })
}

fn required_tool_string_array(
    input: &JsonValue,
    key: &str,
) -> Result<Vec<String>, ToolExecutionError> {
    let values = input
        .get(key)
        .and_then(JsonValue::as_array)
        .ok_or_else(|| {
            ToolExecutionError::invalid_input(
                "agent_core_headless_tool_argument_missing",
                format!("Tool argument `{key}` must be a non-empty string array."),
            )
        })?;
    let values = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|text| !text.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    ToolExecutionError::invalid_input(
                        "agent_core_headless_tool_argument_invalid",
                        format!("Tool argument `{key}` must contain only non-empty strings."),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.is_empty() {
        return Err(ToolExecutionError::invalid_input(
            "agent_core_headless_tool_argument_invalid",
            format!("Tool argument `{key}` must contain at least one value."),
        ));
    }
    Ok(values)
}

fn validate_headless_argv(argv: &[String]) -> Result<(), ToolExecutionError> {
    if argv.is_empty() || argv[0].trim().is_empty() {
        return Err(ToolExecutionError::invalid_input(
            "agent_core_headless_command_argv_invalid",
            "Command argv must include a program name.",
        ));
    }
    if argv.iter().any(|part| part.contains('\0')) {
        return Err(ToolExecutionError::invalid_input(
            "agent_core_headless_command_argv_invalid",
            "Command argv cannot contain NUL bytes.",
        ));
    }
    Ok(())
}

fn read_limited_output<R>(mut reader: R, limit: usize) -> LimitedOutput
where
    R: Read,
{
    let mut bytes = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = reader.read(&mut buffer) else {
            break;
        };
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        if remaining > 0 {
            let take = read.min(remaining);
            bytes.extend_from_slice(&buffer[..take]);
        }
        if read > remaining {
            truncated = true;
        }
    }
    LimitedOutput { bytes, truncated }
}

fn patch_changed_paths(patch: &str) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for line in patch.lines() {
        let candidate = line
            .strip_prefix("+++ b/")
            .or_else(|| line.strip_prefix("--- a/"))
            .or_else(|| {
                line.strip_prefix("diff --git a/").and_then(|rest| {
                    rest.split_once(" b/")
                        .map(|(_left, right)| right.split_whitespace().next().unwrap_or(right))
                })
            });
        if let Some(path) = candidate {
            let path = path.trim();
            if !path.is_empty() && path != "/dev/null" {
                paths.insert(path.to_owned());
            }
        }
    }
    paths.into_iter().collect()
}

fn headless_tool_is_command(tool_name: &str) -> bool {
    tool_name == HEADLESS_TOOL_COMMAND
}

fn rollback_checkpoint_metadata(path: &str, resolved: &Path) -> JsonValue {
    match fs::read(resolved) {
        Ok(bytes) => json!({
            "kind": "file_rollback",
            "path": path,
            "existed": true,
            "bytes": bytes.len(),
            "stableHash": stable_bytes_hash(&bytes),
            "contentRedacted": true,
        }),
        Err(_) => json!({
            "kind": "file_rollback",
            "path": path,
            "existed": false,
            "bytes": 0,
            "stableHash": JsonValue::Null,
            "contentRedacted": true,
        }),
    }
}

fn file_reservation_metadata(
    context: &ToolExecutionContext,
    call: &ToolCallInput,
    path: &str,
) -> JsonValue {
    json!({
        "kind": "file_reservation",
        "reservationId": format!("reservation-{}-{}", context.run_id, call.tool_call_id),
        "ownerRunId": context.run_id,
        "toolCallId": call.tool_call_id,
        "path": path,
        "operation": "writing",
        "status": "claimed_for_turn",
        "conflictPolicy": "deny_without_override",
    })
}

fn core_error_to_tool_execution_error(error: CoreError) -> ToolExecutionError {
    if error.code.contains("denied") || error.code.contains("protected") {
        return ToolExecutionError::policy_denied(error.code, error.message);
    }
    if error.code.contains("missing")
        || error.code.contains("invalid")
        || error.code.contains("unavailable")
    {
        return ToolExecutionError::invalid_input(error.code, error.message);
    }
    ToolExecutionError::retryable(error.code, error.message)
}

fn tool_execution_error_to_core_error(error: ToolExecutionError) -> CoreError {
    CoreError::system_fault(error.code, error.message)
}

fn headless_system_prompt(workspace_root: Option<&Path>) -> String {
    headless_system_prompt_for_agent("engineer", workspace_root)
}

fn normalized_string_set<'a>(values: impl Iterator<Item = &'a JsonValue>) -> BTreeSet<String> {
    values
        .filter_map(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn normalized_headless_tool_set<'a>(
    values: impl Iterator<Item = &'a JsonValue>,
) -> BTreeSet<String> {
    values
        .filter_map(JsonValue::as_str)
        .flat_map(normalized_headless_tool_names)
        .map(str::to_owned)
        .collect()
}

fn normalized_headless_tool_names(tool_name: &str) -> Vec<&'static str> {
    match tool_name.trim() {
        HEADLESS_TOOL_READ => vec![HEADLESS_TOOL_READ],
        HEADLESS_TOOL_LIST => vec![HEADLESS_TOOL_LIST],
        HEADLESS_TOOL_WRITE => vec![HEADLESS_TOOL_WRITE],
        HEADLESS_TOOL_PATCH | "edit" => vec![HEADLESS_TOOL_PATCH],
        HEADLESS_TOOL_DELETE => vec![HEADLESS_TOOL_DELETE],
        HEADLESS_TOOL_MOVE => vec![HEADLESS_TOOL_MOVE],
        HEADLESS_TOOL_REPLACE => vec![HEADLESS_TOOL_REPLACE],
        HEADLESS_TOOL_COMMAND | "command_run" | "command_verify" | "command_probe" => {
            vec![HEADLESS_TOOL_COMMAND]
        }
        HEADLESS_TOOL_TODO => vec![HEADLESS_TOOL_TODO],
        _ => Vec::new(),
    }
}

fn headless_condition_tool_names(condition: &JsonValue) -> BTreeSet<String> {
    condition["toolName"]
        .as_str()
        .into_iter()
        .chain(
            condition["toolNames"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(JsonValue::as_str),
        )
        .flat_map(normalized_headless_tool_names)
        .map(str::to_owned)
        .collect()
}

fn supported_headless_agent_tools() -> BTreeSet<&'static str> {
    HEADLESS_SUPPORTED_AGENT_TOOLS.iter().copied().collect()
}

fn headless_tool_is_write(tool_name: &str) -> bool {
    matches!(
        tool_name,
        HEADLESS_TOOL_WRITE
            | HEADLESS_TOOL_PATCH
            | HEADLESS_TOOL_DELETE
            | HEADLESS_TOOL_MOVE
            | HEADLESS_TOOL_REPLACE
    )
}

fn base_capability_profile_for_runtime_agent(runtime_agent_id: &str) -> &'static str {
    match runtime_agent_id {
        "ask" => "observe_only",
        "plan" => "planning",
        "crawl" => "repository_recon",
        "debug" => "debugging",
        "agent_create" => "agent_builder",
        "computer_use" => "computer_use",
        _ => "engineering",
    }
}

fn profile_allows_workspace_writes(base_capability_profile: &str) -> bool {
    matches!(base_capability_profile, "engineering" | "debugging")
}

fn default_approval_modes_for_profile(base_capability_profile: &str) -> BTreeSet<String> {
    if profile_allows_workspace_writes(base_capability_profile) {
        ["suggest", "auto_edit", "yolo"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        BTreeSet::from(["suggest".into()])
    }
}

fn named_headless_tool_policy(policy: &str) -> Option<BTreeSet<String>> {
    match policy.trim() {
        "observe_only" | "planning" | "repository_recon" | "agent_builder" => Some(
            [HEADLESS_TOOL_READ, HEADLESS_TOOL_LIST, HEADLESS_TOOL_TODO]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        "engineering" => None,
        _ => Some(BTreeSet::new()),
    }
}

fn definition_text(
    object: &serde_json::Map<String, JsonValue>,
    key: &str,
    fallback: &str,
) -> String {
    object
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn validate_headless_agent_stages(
    definition_id: &str,
    workflow: Option<&HeadlessAgentStageWorkflow>,
) -> CoreResult<()> {
    let Some(workflow) = workflow else {
        return Ok(());
    };
    if workflow.phases.is_empty() {
        return Err(CoreError::invalid_request(
            "agent_core_headless_agent_stages_empty",
            format!("Agent definition `{definition_id}` must declare at least one Stage."),
        ));
    }
    let mut stage_ids = BTreeSet::new();
    for stage in &workflow.phases {
        if stage.id.trim().is_empty() || stage.title.trim().is_empty() {
            return Err(CoreError::invalid_request(
                "agent_core_headless_agent_stage_identity_invalid",
                format!("Agent definition `{definition_id}` has a Stage without an id or title."),
            ));
        }
        if !stage_ids.insert(stage.id.as_str()) {
            return Err(CoreError::invalid_request(
                "agent_core_headless_agent_stage_duplicate",
                format!(
                    "Agent definition `{definition_id}` declares duplicate Stage `{}`.",
                    stage.id
                ),
            ));
        }
        if stage.retry_limit == Some(0) {
            return Err(CoreError::invalid_request(
                "agent_core_headless_agent_stage_retry_limit_invalid",
                format!(
                    "Agent definition `{definition_id}` Stage `{}` must use a positive retryLimit.",
                    stage.id
                ),
            ));
        }
        for check in &stage.required_checks {
            validate_headless_stage_condition(definition_id, stage, check, false)?;
        }
    }
    if workflow
        .start_phase_id
        .as_deref()
        .is_some_and(|start| !stage_ids.contains(start))
    {
        return Err(CoreError::invalid_request(
            "agent_core_headless_agent_start_stage_unknown",
            format!(
                "Agent definition `{definition_id}` start Stage does not reference a declared Stage."
            ),
        ));
    }
    for stage in &workflow.phases {
        for branch in &stage.branches {
            let target = branch
                .get("targetPhaseId")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|target| !target.is_empty())
                .ok_or_else(|| {
                    CoreError::invalid_request(
                        "agent_core_headless_agent_stage_branch_target_missing",
                        format!(
                            "Agent definition `{definition_id}` Stage `{}` has a branch without targetPhaseId.",
                            stage.id
                        ),
                    )
                })?;
            if !stage_ids.contains(target) {
                return Err(CoreError::invalid_request(
                    "agent_core_headless_agent_stage_branch_target_unknown",
                    format!(
                        "Agent definition `{definition_id}` Stage `{}` branches to unknown Stage `{target}`.",
                        stage.id
                    ),
                ));
            }
            validate_headless_stage_condition(
                definition_id,
                stage,
                branch.get("condition").unwrap_or(&JsonValue::Null),
                true,
            )?;
        }
    }
    Ok(())
}

fn validate_headless_stage_condition(
    definition_id: &str,
    stage: &HeadlessAgentStage,
    condition: &JsonValue,
    allow_always: bool,
) -> CoreResult<()> {
    let kind = condition
        .get("kind")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default();
    match kind {
        "always" if allow_always => Ok(()),
        "todo_completed" => {
            if condition
                .get("todoId")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .is_some_and(|todo_id| !todo_id.is_empty())
            {
                Ok(())
            } else {
                Err(CoreError::invalid_request(
                    "agent_core_headless_agent_stage_todo_id_missing",
                    format!(
                        "Agent definition `{definition_id}` Stage `{}` has a todo_completed check without todoId.",
                        stage.id
                    ),
                ))
            }
        }
        "tool_succeeded" => {
            if condition
                .get("minCount")
                .is_some_and(|count| count.as_u64().is_none_or(|count| count == 0))
            {
                return Err(CoreError::invalid_request(
                    "agent_core_headless_agent_stage_min_count_invalid",
                    format!(
                        "Agent definition `{definition_id}` Stage `{}` must use a positive minCount.",
                        stage.id
                    ),
                ));
            }
            let tools = headless_condition_tool_names(condition);
            if tools.is_empty() {
                return Err(CoreError::invalid_request(
                    "agent_core_headless_agent_stage_tool_unsupported",
                    format!(
                        "Agent definition `{definition_id}` Stage `{}` has no headless-executable tool in its tool_succeeded check.",
                        stage.id
                    ),
                ));
            }
            let effective_tools = stage
                .allowed_tools
                .as_ref()
                .filter(|tools| !tools.is_empty())
                .map(|tools| {
                    tools
                        .iter()
                        .flat_map(|tool| normalized_headless_tool_names(tool))
                        .map(str::to_owned)
                        .collect::<BTreeSet<_>>()
                });
            if effective_tools.is_some_and(|allowed| tools.is_disjoint(&allowed)) {
                return Err(CoreError::invalid_request(
                    "agent_core_headless_agent_stage_tool_unavailable",
                    format!(
                        "Agent definition `{definition_id}` Stage `{}` cannot satisfy its tool_succeeded check with its allowedTools.",
                        stage.id
                    ),
                ));
            }
            Ok(())
        }
        _ => Err(CoreError::invalid_request(
            "agent_core_headless_agent_stage_check_unsupported",
            format!(
                "Agent definition `{definition_id}` Stage `{}` uses unsupported headless Stage check `{kind}`.",
                stage.id
            ),
        )),
    }
}

fn custom_headless_agent_system_prompt(
    profile: &HeadlessAgentDefinitionProfile,
    workspace_root: Option<&Path>,
) -> String {
    let baseline_agent = match profile.base_capability_profile.as_str() {
        "observe_only" => "ask",
        "planning" => "plan",
        "debugging" => "debug",
        _ => "engineer",
    };
    let mut sections = vec![headless_system_prompt_for_agent(
        baseline_agent,
        workspace_root,
    )];
    sections.push(format!(
        "Active custom Agent: {} (`{}` v{}).",
        profile.display_name, profile.definition_id, profile.definition_version
    ));
    if !profile.task_purpose.is_empty() {
        sections.push(format!("Task purpose: {}", profile.task_purpose));
    }
    sections.extend(profile.system_prompt_fragments.iter().cloned());
    if !profile.workflow_contract.is_empty() {
        sections.push(format!("Stage contract: {}", profile.workflow_contract));
    }
    if let Some(stage) = profile.initial_stage() {
        sections.push(format!(
            "Current Stage: {} (`{}`). Complete its required checks before the final response.",
            stage.title, stage.id
        ));
    }
    if !profile.final_response_contract.is_empty() {
        sections.push(format!(
            "Custom final response contract: {}",
            profile.final_response_contract
        ));
    }
    sections.join("\n\n")
}

fn headless_system_prompt_for_agent(
    runtime_agent_id: &str,
    workspace_root: Option<&Path>,
) -> String {
    let workspace = workspace_root
        .map(|root| root.display().to_string())
        .unwrap_or_else(|| "the configured workspace".into());
    match runtime_agent_id {
        "ask" => format!(
            "xero-owned-agent-v1\n\nYou are Xero's Ask agent. Answer the user's question in chat using audited observe-only tools only when grounding is needed.\n\nAsk is answer-only in observable effect. Do not edit, write, patch, delete, rename, create directories, run shell commands, start or stop processes, control browsers or devices, invoke external services, install or invoke skills, spawn subagents, or mutate app state. Do not request approval to escape this boundary.\n\nUse the production Tool Registry V2 observe-only tools to inspect files in {workspace}: `read` and `list`. For broad project questions, survey the repository root first, prefer root-level README/package/Cargo/workspace files before descending into a subdirectory, and treat folders such as `landing/` as one component until root context proves otherwise.\n\nFinal response contract: answer directly, cite project facts or uncertainty when relevant, name important files, symbols, decisions, or constraints when helpful, and do not include secrets."
        ),
        "plan" => format!(
            "xero-owned-agent-v1\n\nYou are Xero's Plan agent. Turn ambiguous user intent into a clear implementation plan without mutating repository files.\n\nPlan is planning-only in observable effect. Do not edit, write, patch, delete, rename, create directories, run shell commands, start or stop processes, or mutate external services.\n\nUse the production Tool Registry V2 observe-only tools to inspect files in {workspace}: `read` and `list`. For broad project questions, survey root context before choosing a subsystem.\n\nFinal response contract: summarize the plan, context used, risks, and a deterministic handoff for implementation."
        ),
        "debug" => format!(
            "xero-owned-agent-v1\n\nYou are Xero's Debug agent. Reproduce, gather evidence, test hypotheses, isolate root cause, fix when needed, and verify.\n\nUse the production Tool Registry V2 tools in {workspace}: `read`, `list`, and, when available, `write`, `patch`, `delete`, `move`, `replace`, and `command`. Keep task deliverables inside the workspace, never touch .git or .xero, and use scratch locations such as /tmp for build outputs, compiled binaries, downloaded helpers, and verification debris when the command runner allows it. Before probing fragile recovery inputs, copy them first and work from the copies when possible. Before finishing, remove temporary files you created inside the workspace and leave only task-requested deliverables.\n\nFinal response contract: provide symptom, root cause, fix, files changed, verification, and residual risk."
        ),
        _ => format!(
            "xero-owned-agent-v1\n\nYou are Xero's headless owned-agent runtime. Use the production Tool Registry V2 tools when you need to inspect, edit, patch, or verify files in {workspace}: `read`, `list`, and, when available, `write`, `patch`, `delete`, `move`, `replace`, and `command`. Keep task deliverables inside the workspace, never touch .git or .xero, and use scratch locations such as /tmp for build outputs, compiled binaries, downloaded helpers, and verification debris when the command runner allows it. Before probing fragile recovery inputs, copy them first and work from the copies when possible. Before finishing, remove temporary files you created inside the workspace and leave only task-requested deliverables. Avoid network access unless the active runtime policy explicitly allows it, and finish with a concise summary."
        ),
    }
}

fn headless_agent_allows_workspace_writes(runtime_agent_id: &str) -> bool {
    !matches!(
        runtime_agent_id,
        "ask" | "computer_use" | "plan" | "crawl" | "agent_create"
    )
}

fn normalize_headless_thinking_effort(effort: Option<&str>) -> Option<String> {
    Some(match effort?.trim().to_ascii_lowercase().as_str() {
        "minimal" => "minimal".into(),
        "low" => "low".into(),
        "medium" => "medium".into(),
        "high" => "high".into(),
        "x_high" | "xhigh" => "xhigh".into(),
        _ => return None,
    })
}

fn deepseek_headless_effort(effort: &str) -> &'static str {
    if effort == "xhigh" {
        "max"
    } else {
        "high"
    }
}

fn clamp_openai_codex_headless_effort(model_id: &str, effort: &str) -> &'static str {
    let effort = match effort {
        "minimal" => "minimal",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" => "xhigh",
        _ => "medium",
    };
    let model_id = model_id.rsplit('/').next().unwrap_or(model_id);
    let model_id = model_id.trim().to_ascii_lowercase();
    if ["gpt-5.2", "gpt-5.3", "gpt-5.4", "gpt-5.5", "gpt-5.6"]
        .iter()
        .any(|prefix| model_id.starts_with(prefix))
        && effort == "minimal"
    {
        return "low";
    }
    if model_id == "gpt-5.1" && effort == "xhigh" {
        return "high";
    }
    if model_id == "gpt-5.1-codex-mini" {
        return if effort == "high" || effort == "xhigh" {
            "high"
        } else {
            "medium"
        };
    }
    effort
}

fn openai_compatible_chat_url(base_url: &str) -> CoreResult<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(CoreError::invalid_request(
            "agent_core_provider_base_url_missing",
            "A provider base URL is required for headless real-provider execution.",
        ));
    }
    if trimmed.starts_with("http://") && !is_local_http_endpoint(trimmed) {
        return Err(CoreError::invalid_request(
            "agent_core_provider_base_url_insecure",
            "Headless real-provider HTTP endpoints are only allowed for localhost.",
        ));
    }
    let url = if trimmed.ends_with("/chat/completions") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/chat/completions")
    };
    Ok(url)
}

fn is_local_http_endpoint(base_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return false;
    };
    if url.scheme() != "http" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.parse::<std::net::IpAddr>().is_ok_and(|address| {
        address.is_loopback()
            || matches!(address, std::net::IpAddr::V4(ipv4) if ipv4.is_unspecified())
    })
}

fn normalize_timeout(timeout_ms: u64) -> u64 {
    if timeout_ms == 0 {
        DEFAULT_HEADLESS_PROVIDER_TIMEOUT_MS
    } else {
        timeout_ms
    }
}

fn resolve_workspace_path_for_root(
    root: &Path,
    requested: &str,
    allow_missing_leaf: bool,
) -> CoreResult<PathBuf> {
    let root = fs::canonicalize(root).map_err(|error| {
        CoreError::invalid_request(
            "agent_core_headless_workspace_unavailable",
            format!(
                "Workspace root `{}` is unavailable: {error}",
                root.display()
            ),
        )
    })?;
    let requested_path = Path::new(requested);
    if requested_path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(CoreError::invalid_request(
            "agent_core_headless_path_denied",
            format!("Path `{requested}` must stay inside the workspace."),
        ));
    }
    let joined = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        root.join(requested_path)
    };
    if requested_path
        .components()
        .any(|component| match component {
            // Compare case-insensitively: on macOS's default case-insensitive filesystem
            // `.GIT`/`.Xero` resolve to the same protected directories, so an exact match would
            // let a write slip through the guard while still landing in `.git`/`.xero`.
            Component::Normal(value) => value.to_str().is_some_and(|part| {
                part.eq_ignore_ascii_case(".git") || part.eq_ignore_ascii_case(".xero")
            }),
            _ => false,
        })
    {
        return Err(CoreError::invalid_request(
            "agent_core_headless_path_protected",
            format!("Path `{requested}` targets protected workspace state."),
        ));
    }
    // Reject an existing symlink leaf. In `allow_missing_leaf` mode only the nearest existing
    // ancestor is canonicalized below, so a symlink at the leaf (e.g. a checked-in
    // `deploy.cfg -> ~/.ssh/authorized_keys`) would otherwise be followed by `fs::write`,
    // escaping the workspace. Reads/deletes canonicalize the leaf itself and are unaffected.
    if let Ok(metadata) = fs::symlink_metadata(&joined) {
        if metadata.file_type().is_symlink() {
            return Err(CoreError::invalid_request(
                "agent_core_headless_path_denied",
                format!("Path `{requested}` resolves through a symlink and is not allowed."),
            ));
        }
    }
    let check_path = if allow_missing_leaf {
        let mut candidate = joined.parent().unwrap_or(root.as_path()).to_path_buf();
        while !candidate.exists() && candidate != root {
            candidate = candidate.parent().unwrap_or(root.as_path()).to_path_buf();
        }
        candidate
    } else {
        joined.clone()
    };
    let canonical_check = fs::canonicalize(&check_path).map_err(|error| {
        CoreError::invalid_request(
            "agent_core_headless_path_unavailable",
            format!("Path `{requested}` could not be resolved: {error}"),
        )
    })?;
    if !canonical_check.starts_with(&root) {
        return Err(CoreError::invalid_request(
            "agent_core_headless_path_denied",
            format!("Path `{requested}` escapes the approved workspace."),
        ));
    }
    Ok(joined)
}

#[derive(Debug, Default)]
struct WorkspaceListing {
    directories: Vec<String>,
    files: Vec<String>,
    entries: Vec<JsonValue>,
    skipped_directories: Vec<String>,
    omitted_entry_count: usize,
    truncated: bool,
}

fn collect_workspace_listing(
    root: &Path,
    dir: &Path,
    limit: usize,
) -> CoreResult<WorkspaceListing> {
    let mut listing = WorkspaceListing::default();
    collect_workspace_entries(root, dir, &mut listing, limit)?;
    Ok(listing)
}

fn collect_workspace_entries(
    root: &Path,
    dir: &Path,
    listing: &mut WorkspaceListing,
    limit: usize,
) -> CoreResult<()> {
    let entries = sorted_workspace_entries(dir)?;
    let mut directories = Vec::new();

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() && skipped_workspace_dir(&name) {
            if let Some(relative) = workspace_relative_path(root, &path) {
                listing.skipped_directories.push(relative);
            }
            continue;
        }

        if path.is_dir() {
            if let Some(relative) = workspace_relative_path(root, &path) {
                push_workspace_listing_entry(listing, limit, "directory", &relative);
                directories.push(path);
            }
        } else if let Some(relative) = workspace_relative_path(root, &path) {
            push_workspace_listing_entry(listing, limit, "file", &relative);
        }
    }

    for directory in directories {
        if listing.truncated {
            break;
        }
        collect_workspace_entries(root, &directory, listing, limit)?;
    }

    Ok(())
}

fn sorted_workspace_entries(dir: &Path) -> CoreResult<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(dir)
        .map_err(|error| {
            CoreError::invalid_request(
                "agent_core_headless_list_failed",
                format!("Xero could not list `{}`: {error}", dir.display()),
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CoreError::invalid_request(
                "agent_core_headless_list_failed",
                format!("Xero could not read a directory entry: {error}"),
            )
        })?;
    entries.sort_by(|left, right| {
        left.file_name()
            .to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&right.file_name().to_string_lossy().to_ascii_lowercase())
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });
    Ok(entries)
}

fn push_workspace_listing_entry(
    listing: &mut WorkspaceListing,
    limit: usize,
    kind: &str,
    path: &str,
) {
    if listing.entries.len() >= limit {
        listing.truncated = true;
        listing.omitted_entry_count = listing.omitted_entry_count.saturating_add(1);
        return;
    }
    if kind == "directory" {
        listing.directories.push(path.to_owned());
    } else {
        listing.files.push(path.to_owned());
    }
    listing.entries.push(json!({
        "path": path,
        "kind": kind,
    }));
}

fn workspace_relative_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().to_string())
        .filter(|relative| !relative.is_empty())
}

fn skipped_workspace_dir(name: &str) -> bool {
    SKIPPED_WORKSPACE_DIRS.contains(&name)
}

fn compact_summary_from_snapshot(snapshot: &RunSnapshot) -> String {
    snapshot
        .messages
        .iter()
        .rev()
        .take(12)
        .rev()
        .map(|message| {
            format!(
                "{:?}: {}",
                message.role,
                truncate_text(message.content.as_str(), 500)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn headless_context_hash(snapshot: &RunSnapshot, turn_index: usize) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in format!(
        "{}:{}:{}:{}:{}",
        snapshot.project_id,
        snapshot.agent_session_id,
        snapshot.run_id,
        snapshot.prompt,
        turn_index
    )
    .bytes()
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn next_headless_provider_turn_index(snapshot: &RunSnapshot) -> usize {
    snapshot
        .events
        .iter()
        .filter(|event| event.event_kind == RuntimeEventKind::ToolRegistrySnapshot)
        .filter_map(|event| event.payload.get("turnIndex").and_then(JsonValue::as_u64))
        .filter_map(|turn_index| usize::try_from(turn_index).ok())
        .max()
        .map_or(0, |turn_index| turn_index.saturating_add(1))
}

fn stable_provider_preflight_hash(snapshot: &ProviderPreflightSnapshot) -> String {
    let serialized = serde_json::to_string(snapshot).unwrap_or_else(|_| "unserializable".into());
    crate::runtime_trace_id("provider-preflight", &[&serialized])
}

fn stable_bytes_hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn generate_headless_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{prefix}-{millis}-{}", std::process::id())
}

fn truncate_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.into();
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuntimeMessage;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    fn serve_http_once(
        status: &str,
        content_type: &str,
        body: impl Into<String>,
    ) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind HTTP fixture");
        let address = listener.local_addr().expect("HTTP fixture address");
        let status = status.to_string();
        let content_type = content_type.to_string();
        let body = body.into();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept HTTP fixture request");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set fixture read timeout");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            let (header_end, content_length) = loop {
                let read = stream.read(&mut buffer).expect("read HTTP fixture request");
                assert!(read > 0, "HTTP fixture request ended before its headers");
                request.extend_from_slice(&buffer[..read]);
                let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().expect("content length"))
                    })
                    .unwrap_or_default();
                break (header_end + 4, content_length);
            };
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).expect("read HTTP fixture body");
                assert!(read > 0, "HTTP fixture request body ended early");
                request.extend_from_slice(&buffer[..read]);
            }
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write HTTP fixture response");
            String::from_utf8(request).expect("HTTP fixture request is UTF-8")
        });
        (format!("http://{address}"), handle)
    }

    fn http_request_json(request: &str) -> JsonValue {
        let (_, body) = request
            .split_once("\r\n\r\n")
            .expect("HTTP fixture request body");
        serde_json::from_str(body).expect("HTTP fixture JSON body")
    }

    #[test]
    fn openai_codex_gpt_5_6_clamps_minimal_headless_reasoning_to_low() {
        assert_eq!(
            clamp_openai_codex_headless_effort("gpt-5.6-terra", "minimal"),
            "low"
        );
    }

    #[test]
    fn insecure_http_provider_rejects_hosts_that_only_prefix_match_localhost() {
        for endpoint in [
            "http://localhost.evil.example/v1",
            "http://127.example.com/v1",
            "http://0.0.0.0.evil.example/v1",
        ] {
            assert_eq!(
                openai_compatible_chat_url(endpoint)
                    .expect_err("non-local HTTP endpoint must be rejected")
                    .code,
                "agent_core_provider_base_url_insecure"
            );
        }
    }

    #[test]
    fn headless_workspace_path_accepts_absolute_paths_inside_root() {
        let root = unique_test_dir("headless-absolute-root");
        fs::create_dir_all(root.join("logs")).expect("create logs dir");

        assert_eq!(
            resolve_workspace_path_for_root(&root, &root.display().to_string(), false)
                .expect("absolute root should resolve"),
            root.clone()
        );
        assert_eq!(
            resolve_workspace_path_for_root(&root, &root.join("logs").display().to_string(), false)
                .expect("absolute child should resolve"),
            root.join("logs")
        );
    }

    #[test]
    fn headless_workspace_path_rejects_absolute_paths_outside_root() {
        let root = unique_test_dir("headless-absolute-root-deny");
        let outside = root
            .parent()
            .expect("test root has parent")
            .join("outside-file");

        let error = resolve_workspace_path_for_root(&root, &outside.display().to_string(), true)
            .expect_err("absolute outside path should be denied");

        assert_eq!(error.code, "agent_core_headless_path_denied");
    }

    #[test]
    fn headless_workspace_path_protects_absolute_legacy_state() {
        let root = unique_test_dir("headless-absolute-protected");
        fs::create_dir_all(root.join(".xero")).expect("create legacy state dir");

        let error = resolve_workspace_path_for_root(
            &root,
            &root.join(".xero").display().to_string(),
            false,
        )
        .expect_err("absolute protected state should be denied");

        assert_eq!(error.code, "agent_core_headless_path_protected");
    }

    #[test]
    fn headless_system_prompt_preserves_workspace_hygiene() {
        let prompt = headless_system_prompt(Some(Path::new("/app")));

        assert!(prompt.contains("/tmp"));
        assert!(prompt.contains("Before probing fragile recovery inputs"));
        assert!(prompt.contains("remove temporary files"));
        assert!(prompt.contains("leave only task-requested deliverables"));
    }

    #[test]
    fn ask_headless_identity_uses_observe_only_prompt_and_tools() {
        let request = StartRunRequest {
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-ask".into(),
            prompt: "What is this project about?".into(),
            provider: ProviderSelection {
                provider_id: "openai_api".into(),
                model_id: "test-model".into(),
            },
            controls: Some(crate::RunControls {
                runtime_agent_id: "ask".into(),
                agent_definition_id: Some("ask".into()),
                agent_definition_version: Some(1),
                agent_definition_snapshot: None,
                thinking_effort: Some("x_high".into()),
                approval_mode: "suggest".into(),
                plan_mode_required: false,
            }),
        };

        let identity = HeadlessRunIdentity::from_request(&request, Some(Path::new("/repo")))
            .expect("headless identity");

        assert_eq!(identity.runtime_agent_id, "ask");
        assert_eq!(identity.agent_definition_id, "ask");
        assert_eq!(identity.thinking_effort.as_deref(), Some("xhigh"));
        assert!(!identity.allows_workspace_writes());
        assert!(identity.system_prompt.contains("You are Xero's Ask agent"));
        assert!(identity
            .system_prompt
            .contains("survey the repository root first"));
    }

    #[test]
    fn headless_list_is_deterministic_root_first_and_skips_build_artifacts() {
        let root = unique_test_dir("headless-list-root-first");
        fs::create_dir_all(root.join("client/src")).expect("create client dir");
        fs::create_dir_all(root.join("landing/.next/cache")).expect("create landing build dir");
        fs::create_dir_all(root.join("server/lib")).expect("create server dir");
        fs::write(root.join("package.json"), "{}\n").expect("write root package");
        fs::write(root.join("client/package.json"), "{}\n").expect("write client package");
        fs::write(root.join("landing/.next/cache/build.txt"), "ignored\n")
            .expect("write ignored build file");
        fs::write(root.join("server/mix.exs"), "[]\n").expect("write server file");

        let runtime =
            HeadlessProductionToolRuntime::new(Some(&root), false, Vec::new()).expect("runtime");
        let output = runtime
            .list(&ToolCallInput {
                tool_call_id: "call-list".into(),
                tool_name: "list".into(),
                input: json!({ "path": "." }),
            })
            .expect("list output")
            .output;
        let directories = output["directories"].as_array().expect("directories");
        let files = output["files"].as_array().expect("files");
        let skipped = output["skippedDirectories"]
            .as_array()
            .expect("skipped directories");

        assert!(directories.iter().any(|path| path == "client"));
        assert!(directories.iter().any(|path| path == "landing"));
        assert!(directories.iter().any(|path| path == "server"));
        assert!(files.iter().any(|path| path == "package.json"));
        assert!(files.iter().any(|path| path == "client/package.json"));
        assert!(files.iter().all(|path| {
            !path
                .as_str()
                .expect("file path")
                .contains("landing/.next/cache/build.txt")
        }));
        assert!(skipped.iter().any(|path| path == "landing/.next"));
    }

    #[test]
    fn openai_codex_responses_url_normalizes_backend_base() {
        assert_eq!(
            openai_codex_responses_url("https://chatgpt.com/backend-api").expect("url"),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            openai_codex_responses_url("https://chatgpt.com/backend-api/codex").expect("url"),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            openai_codex_responses_url("https://chatgpt.com/backend-api/codex/responses")
                .expect("url"),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert!(openai_codex_responses_url("http://example.com").is_err());
    }

    #[test]
    fn openai_codex_response_input_encodes_tool_round_trip() {
        let input = openai_codex_response_input(&[
            json!({"role": "system", "content": "system instructions"}),
            json!({"role": "user", "content": "fix the file"}),
            json!({
                "role": "assistant",
                "content": "I will inspect it.",
                "tool_calls": [{
                    "id": "call-1",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"README.md\"}"
                    }
                }]
            }),
            json!({"role": "tool", "tool_call_id": "call-1", "content": "contents"}),
        ])
        .expect("input");

        assert_eq!(input.len(), 4);
        assert_eq!(input[0]["content"][0]["type"], json!("input_text"));
        assert_eq!(input[1]["type"], json!("message"));
        assert_eq!(input[2]["type"], json!("function_call"));
        assert_eq!(input[2]["call_id"], json!("call-1"));
        assert_eq!(input[3]["type"], json!("function_call_output"));
        assert_eq!(input[3]["output"], json!("contents"));
    }

    #[test]
    fn openai_compatible_local_fixture_covers_requests_responses_and_failures() {
        let client = Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("build fixture client");
        let (base_url, request) = serve_http_once(
            "200 OK",
            "application/json",
            json!({
                "choices": [{
                    "message": {
                        "content": [{"text": "hello"}, {"content": " world"}],
                        "tool_calls": [{
                            "id": "call-1",
                            "function": {"name": "read", "arguments": "{\"path\":\"README.md\"}"}
                        }]
                    }
                }]
            })
            .to_string(),
        );
        let config = OpenAiCompatibleHeadlessConfig {
            provider_id: "openai_api".into(),
            model_id: "model-1".into(),
            base_url: format!("{base_url}/v1"),
            api_key: Some("secret-key".into()),
            timeout_ms: 2_000,
            workspace_root: None,
            allow_workspace_writes: false,
        };
        let message = send_openai_compatible_chat(
            &client,
            &config,
            &[json!({"role": "user", "content": "hello"})],
            vec![json!({"type": "function"})],
            Some("high"),
        )
        .expect("decode compatible response");
        assert_eq!(message.content_text(), "hello world");
        assert_eq!(message.tool_calls.len(), 1);
        assert_eq!(message.tool_calls[0].name, "read");
        assert_eq!(message.tool_calls[0].arguments["path"], "README.md");
        let request = request.join().expect("join HTTP fixture");
        assert!(request.contains("authorization: Bearer secret-key\r\n"));
        let body = http_request_json(&request);
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["tool_choice"], "auto");

        for (provider_id, expected) in [
            ("deepseek", json!({"thinking": {"type": "enabled"}})),
            ("openrouter", json!({"reasoning": {"effort": "medium"}})),
        ] {
            let (base_url, request) = serve_http_once(
                "200 OK",
                "application/json",
                r#"{"choices":[{"message":{"content":"ok"}}]}"#,
            );
            let config = OpenAiCompatibleHeadlessConfig {
                provider_id: provider_id.into(),
                base_url,
                api_key: None,
                ..config.clone()
            };
            send_openai_compatible_chat(&client, &config, &[], Vec::new(), Some("medium"))
                .expect("provider-specific request");
            let body = http_request_json(&request.join().expect("join HTTP fixture"));
            for (key, value) in expected.as_object().expect("expected object") {
                assert_eq!(&body[key], value);
            }
        }

        for (status, response, expected_code) in [
            (
                "429 Too Many Requests",
                "rate limited",
                "agent_core_provider_status_failed",
            ),
            (
                "200 OK",
                "not-json",
                "agent_core_provider_response_decode_failed",
            ),
            (
                "200 OK",
                r#"{"choices":[]}"#,
                "agent_core_provider_choice_missing",
            ),
        ] {
            let (base_url, request) = serve_http_once(status, "application/json", response);
            let config = OpenAiCompatibleHeadlessConfig {
                base_url,
                ..config.clone()
            };
            assert_eq!(
                send_openai_compatible_chat(&client, &config, &[], Vec::new(), None)
                    .expect_err("fixture response must fail")
                    .code,
                expected_code
            );
            request.join().expect("join HTTP fixture");
        }
    }

    #[test]
    fn openai_codex_sse_fixture_reassembles_progress_reasoning_and_tool_calls() {
        let stream = [
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.reasoning.delta\",\"delta\":\"first\"}\n\n",
            "data: {\"type\":\"response.reasoning_summary_part.added\"}\n\n",
            "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"second\"}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call-1\",\"name\":\"read\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"delta\":\"{\\\"path\\\":\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"delta\":\"\\\"README.md\\\"}\"}\n\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"name\":\"read\"}}\n\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"name\":\"list\",\"arguments\":\"{}\"}}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\" world\"}\n\n",
            "data: [DONE]\n\n",
        ]
        .concat();
        let (base_url, request) = serve_http_once("200 OK", "text/event-stream", stream);
        let client = Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("build fixture client");
        let response = client.get(base_url).send().expect("request SSE fixture");
        let mut progress = Vec::new();
        let message = parse_openai_codex_responses_sse("openai_codex", response, |text, reason| {
            progress.push((text.to_string(), reason.to_string()));
        })
        .expect("parse SSE fixture");
        request.join().expect("join HTTP fixture");
        assert_eq!(message.content_text(), "hello world");
        assert_eq!(message.reasoning, "first\n\nsecond");
        assert_eq!(message.tool_calls.len(), 2);
        assert_eq!(message.tool_calls[0].id, "call-1");
        assert_eq!(message.tool_calls[0].arguments["path"], "README.md");
        assert_eq!(message.tool_calls[1].id, "openai_codex-tool-call-2");
        assert!(!progress.is_empty());
        assert_eq!(progress.last().unwrap().0, "hello world");
    }

    #[test]
    fn openai_codex_local_fixture_covers_authenticated_stream_request_and_status_failure() {
        let (base_url, request) = serve_http_once(
            "200 OK",
            "text/event-stream",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"done\"}\n\ndata: [DONE]\n\n",
        );
        let client = Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("build fixture client");
        let config = OpenAiCodexHeadlessConfig {
            provider_id: "openai_codex".into(),
            model_id: "gpt-5.6-terra".into(),
            base_url,
            access_token: "access-token".into(),
            account_id: "account-1".into(),
            session_id: Some("session-1".into()),
            timeout_ms: 2_000,
            workspace_root: None,
            allow_workspace_writes: false,
        };
        let mut progress = Vec::new();
        let message = send_openai_codex_responses(
            &client,
            &config,
            &[
                json!({"role": "system", "content": "be precise"}),
                json!({"role": "user", "content": "hello"}),
            ],
            vec![json!({"type": "function", "name": "read"})],
            Some("minimal"),
            |text, reasoning| progress.push((text.to_string(), reasoning.to_string())),
        )
        .expect("send Codex fixture request");
        assert_eq!(message.content_text(), "done");
        assert_eq!(progress.last().unwrap().0, "done");
        let request = request.join().expect("join HTTP fixture");
        assert!(request.starts_with("POST /codex/responses HTTP/1.1\r\n"));
        assert!(request.contains("authorization: Bearer access-token\r\n"));
        assert!(request.contains("chatgpt-account-id: account-1\r\n"));
        assert!(request.contains("session_id: session-1\r\n"));
        let body = http_request_json(&request);
        assert_eq!(body["instructions"], "be precise");
        assert_eq!(body["reasoning"]["effort"], "low");
        assert_eq!(body["tools"][0]["name"], "read");
        assert_eq!(body["input"][0]["role"], "user");

        let (base_url, request) = serve_http_once(
            "401 Unauthorized",
            "application/json",
            "invalid access token",
        );
        let config = OpenAiCodexHeadlessConfig {
            base_url,
            session_id: Some(" ".into()),
            ..config
        };
        assert_eq!(
            send_openai_codex_responses(&client, &config, &[], Vec::new(), None, |_, _| {})
                .expect_err("unauthorized fixture must fail")
                .code,
            "agent_core_provider_status_failed"
        );
        let request = request.join().expect("join HTTP fixture");
        assert!(!request.contains("session_id:"));
    }

    #[test]
    fn provider_message_parsers_reject_malformed_tool_calls_and_streams() {
        assert!(parse_openai_tool_calls(&json!({})).unwrap().is_empty());
        for (message, expected_code) in [
            (
                json!({"tool_calls": [{"id": "call-1"}]}),
                "agent_core_provider_tool_call_invalid",
            ),
            (
                json!({"tool_calls": [{"function": {"arguments": "{}"}}]}),
                "agent_core_provider_tool_name_missing",
            ),
            (
                json!({"tool_calls": [{"function": {"name": "read", "arguments": "{"}}]}),
                "agent_core_provider_tool_arguments_invalid",
            ),
        ] {
            assert_eq!(
                parse_openai_tool_calls(&message)
                    .expect_err("malformed tool call must fail")
                    .code,
                expected_code
            );
        }
        assert_eq!(
            openai_codex_response_input(&[json!({"role": "developer", "content": "no"})])
                .expect_err("unsupported role must fail")
                .code,
            "agent_core_provider_message_role_invalid"
        );
        assert_eq!(
            OpenAiProviderMessage {
                content: json!({"value": 7}),
                tool_calls: Vec::new(),
                reasoning: String::new(),
            }
            .content_text(),
            r#"{"value":7}"#
        );
        assert_eq!(
            OpenAiProviderMessage {
                content: JsonValue::Null,
                tool_calls: Vec::new(),
                reasoning: String::new(),
            }
            .content_text(),
            ""
        );

        for (body, expected_code) in [
            (
                "data: not-json\n\n",
                "agent_core_provider_stream_decode_failed",
            ),
            (
                "data: {\"type\":\"response.failed\",\"error\":\"bad\"}\n\n",
                "agent_core_provider_response_failed",
            ),
            (
                "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"arguments\":\"{}\"}}\n\n",
                "agent_core_provider_tool_name_missing",
            ),
            (
                "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"name\":\"read\",\"arguments\":\"{\"}}\n\n",
                "agent_core_provider_tool_arguments_invalid",
            ),
        ] {
            let (base_url, request) = serve_http_once("200 OK", "text/event-stream", body);
            let response = Client::new().get(base_url).send().expect("request SSE fixture");
            assert_eq!(
                parse_openai_codex_responses_sse("openai_codex", response, |_, _| {})
                    .expect_err("malformed SSE must fail")
                    .code,
                expected_code
            );
            request.join().expect("join HTTP fixture");
        }
    }

    #[test]
    fn provider_execution_config_and_runtime_defaults_cover_every_backend() {
        let fake = HeadlessProviderExecutionConfig::Fake;
        assert_eq!(fake.provider_id(), "fake_provider");
        assert_eq!(fake.model_id(), "fake-model");
        assert!(fake.has_provider_credentials());

        let compatible = |base_url: &str, api_key: Option<&str>| {
            HeadlessProviderExecutionConfig::OpenAiCompatible(OpenAiCompatibleHeadlessConfig {
                provider_id: "openrouter".into(),
                model_id: "model-1".into(),
                base_url: base_url.into(),
                api_key: api_key.map(str::to_owned),
                timeout_ms: 500,
                workspace_root: None,
                allow_workspace_writes: false,
            })
        };
        assert!(!compatible("https://example.com/v1", None).has_provider_credentials());
        assert!(!compatible("https://example.com/v1", Some(" ")).has_provider_credentials());
        assert!(compatible("https://example.com/v1", Some("key")).has_provider_credentials());
        assert!(compatible("http://127.0.0.1:11434/v1", None).has_provider_credentials());
        assert_eq!(
            compatible("https://example.com/v1", Some("key")).provider_id(),
            "openrouter"
        );
        assert_eq!(
            compatible("https://example.com/v1", Some("key")).model_id(),
            "model-1"
        );

        let codex = |token: &str, account: &str| {
            HeadlessProviderExecutionConfig::OpenAiCodexResponses(OpenAiCodexHeadlessConfig {
                provider_id: "openai_codex".into(),
                model_id: "gpt-5.6".into(),
                base_url: "https://chatgpt.com/backend-api".into(),
                access_token: token.into(),
                account_id: account.into(),
                session_id: None,
                timeout_ms: 500,
                workspace_root: None,
                allow_workspace_writes: false,
            })
        };
        assert!(!codex("", "account").has_provider_credentials());
        assert!(!codex("token", "").has_provider_credentials());
        assert!(codex("token", "account").has_provider_credentials());
        assert_eq!(codex("token", "account").provider_id(), "openai_codex");
        assert_eq!(codex("token", "account").model_id(), "gpt-5.6");

        let defaults = HeadlessRuntimeOptions::default();
        assert!(!defaults.ci_mode);
        assert_eq!(
            defaults.max_provider_turns,
            DEFAULT_HEADLESS_MAX_PROVIDER_TURNS
        );
        assert!(defaults.max_wall_time_ms.is_none());
        assert!(defaults.max_tool_calls.is_none());
        assert!(defaults.max_command_calls.is_none());
        assert!(defaults.provider_preflight.is_none());
    }

    #[test]
    fn runtime_preflight_and_provider_admission_cover_fake_codex_and_overrides() {
        let fake_runtime = HeadlessProviderRuntime::new(
            crate::InMemoryAgentCoreStore::default(),
            HeadlessProviderExecutionConfig::Fake,
            HeadlessRuntimeOptions::default(),
        );
        let fake_preflight = fake_runtime
            .provider_preflight_snapshot()
            .expect("fake provider preflight");
        assert_eq!(fake_preflight.status, crate::ProviderPreflightStatus::Passed);
        assert_eq!(fake_preflight.provider_id, "fake_provider");
        assert!(fake_runtime.workspace_root().is_none());
        assert!(!fake_runtime.allow_workspace_writes());

        let workspace_root = std::env::temp_dir().join("xero-codex-runtime-preflight");
        let codex_provider = HeadlessProviderExecutionConfig::OpenAiCodexResponses(
            OpenAiCodexHeadlessConfig {
                provider_id: "openai_codex".into(),
                model_id: "gpt-5.6".into(),
                base_url: "https://chatgpt.com/backend-api".into(),
                access_token: "test-token".into(),
                account_id: "test-account".into(),
                session_id: Some("test-session".into()),
                timeout_ms: 1_000,
                workspace_root: Some(workspace_root.clone()),
                allow_workspace_writes: true,
            },
        );
        let codex_runtime = HeadlessProviderRuntime::new(
            crate::InMemoryAgentCoreStore::default(),
            codex_provider,
            HeadlessRuntimeOptions::default(),
        );
        let codex_preflight = codex_runtime
            .provider_preflight_snapshot()
            .expect("Codex provider preflight");
        assert_eq!(codex_preflight.status, crate::ProviderPreflightStatus::Passed);
        assert_eq!(codex_preflight.source, ProviderPreflightSource::LiveProbe);
        assert_eq!(codex_runtime.workspace_root(), Some(workspace_root));
        assert!(codex_runtime.allow_workspace_writes());

        let provider_mismatch = codex_runtime
            .validate_selected_provider(&ProviderSelection {
                provider_id: "openai_api".into(),
                model_id: "gpt-5.6".into(),
            })
            .expect_err("provider mismatch must fail before runtime mutation");
        assert_eq!(provider_mismatch.code, "agent_core_provider_mismatch");
        let model_mismatch = codex_runtime
            .validate_selected_provider(&ProviderSelection {
                provider_id: "openai_codex".into(),
                model_id: "gpt-5.5".into(),
            })
            .expect_err("model mismatch must fail before runtime mutation");
        assert_eq!(model_mismatch.code, "agent_core_model_mismatch");
        codex_runtime
            .validate_selected_provider(&ProviderSelection {
                provider_id: "openai_codex".into(),
                model_id: "gpt-5.6".into(),
            })
            .expect("matching provider selection");

        let override_runtime = HeadlessProviderRuntime::new(
            crate::InMemoryAgentCoreStore::default(),
            HeadlessProviderExecutionConfig::Fake,
            HeadlessRuntimeOptions {
                provider_preflight: Some(codex_preflight.clone()),
                ..HeadlessRuntimeOptions::default()
            },
        );
        assert_eq!(
            override_runtime
                .provider_preflight_snapshot()
                .expect("preflight override"),
            codex_preflight
        );
    }

    #[test]
    fn headless_run_identity_defaults_and_snapshot_controls_are_fail_closed() {
        let default_request = StartRunRequest {
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-1".into(),
            prompt: "Fix it.".into(),
            provider: ProviderSelection {
                provider_id: "provider-1".into(),
                model_id: "model-1".into(),
            },
            controls: None,
        };
        let identity =
            HeadlessRunIdentity::from_request(&default_request, None).expect("default identity");
        assert_eq!(identity.runtime_agent_id, "engineer");
        assert_eq!(identity.agent_definition_id, "engineer");
        assert_eq!(identity.agent_definition_version, 1);
        assert_eq!(identity.approval_mode, "suggest");
        assert!(identity.thinking_effort.is_none());
        assert!(!identity.allows_workspace_writes());
        assert!(!identity.allows_commands());

        let mut invalid_controls = default_request;
        invalid_controls.controls = Some(crate::RunControls {
            runtime_agent_id: " ".into(),
            agent_definition_id: Some(" ".into()),
            agent_definition_version: Some(0),
            agent_definition_snapshot: None,
            thinking_effort: Some("unknown".into()),
            approval_mode: " ".into(),
            plan_mode_required: false,
        });
        let identity = HeadlessRunIdentity::from_request(&invalid_controls, None)
            .expect("normalized identity");
        assert_eq!(identity.runtime_agent_id, "engineer");
        assert_eq!(identity.agent_definition_id, "engineer");
        assert_eq!(identity.agent_definition_version, 1);
        assert_eq!(identity.approval_mode, "suggest");

        let mut snapshot = run_snapshot_fixture();
        snapshot.runtime_agent_id = "debug".into();
        snapshot.events.push(runtime_event(
            1,
            RuntimeEventKind::RunStarted,
            json!({"thinkingEffort": "HIGH", "approvalMode": "yolo"}),
        ));
        let identity = HeadlessRunIdentity::from_snapshot(&snapshot).expect("snapshot identity");
        assert_eq!(identity.thinking_effort.as_deref(), Some("high"));
        assert_eq!(identity.approval_mode, "yolo");
        assert!(identity.allows_workspace_writes());
        assert!(identity.allows_commands());

        snapshot.events[0].payload = json!({"approvalMode": "anything"});
        let identity = HeadlessRunIdentity::from_snapshot(&snapshot).expect("snapshot identity");
        assert_eq!(identity.approval_mode, "suggest");
        assert!(!identity.allows_workspace_writes());

        snapshot.events[0].payload["agentDefinitionRuntime"] = json!({"bad": true});
        assert_eq!(
            HeadlessRunIdentity::from_snapshot(&snapshot)
                .expect_err("malformed persisted Agent policy must fail closed")
                .code,
            "agent_core_headless_agent_definition_runtime_invalid"
        );
    }

    #[test]
    fn headless_tool_inputs_and_mutation_metadata_redact_sensitive_content() {
        assert_eq!(
            redacted_headless_tool_input("read", &json!("plain")),
            (json!("plain"), false)
        );

        let (write, redacted) = redacted_headless_tool_input(
            HEADLESS_TOOL_WRITE,
            &json!({"path": "secret.txt", "content": "hunter2"}),
        );
        assert!(redacted);
        assert_eq!(write["content"]["redacted"], true);
        assert_eq!(write["content"]["bytes"], 7);

        let patch = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-old\n+new\n--- /dev/null\n+++ b/src/new.rs\n";
        let (patch_input, redacted) =
            redacted_headless_tool_input(HEADLESS_TOOL_PATCH, &json!({"patch": patch}));
        assert!(redacted);
        assert_eq!(
            patch_input["patch"]["changedFiles"],
            json!(["src/a.rs", "src/new.rs"])
        );

        let (replace, redacted) = redacted_headless_tool_input(
            HEADLESS_TOOL_REPLACE,
            &json!({"search": "old", "replacement": "new", "path": "src/a.rs"}),
        );
        assert!(redacted);
        assert_eq!(replace["search"]["bytes"], 3);
        assert_eq!(replace["replacement"]["bytes"], 3);
        assert_eq!(
            redacted_headless_tool_input(HEADLESS_TOOL_WRITE, &json!({"content": 7})),
            (json!({"content": 7}), false)
        );

        let root = unique_test_dir("headless-rollback");
        let existing = root.join("existing.txt");
        fs::write(&existing, "contents").expect("write rollback fixture");
        let rollback = rollback_checkpoint_metadata("existing.txt", &existing);
        assert_eq!(rollback["existed"], true);
        assert_eq!(rollback["bytes"], 8);
        assert_eq!(rollback["contentRedacted"], true);
        assert_eq!(rollback["stableHash"], stable_bytes_hash(b"contents"));
        assert_eq!(
            rollback_checkpoint_metadata("missing.txt", &root.join("missing.txt"))["existed"],
            false
        );

        let reservation = file_reservation_metadata(
            &ToolExecutionContext {
                run_id: "run-1".into(),
                ..ToolExecutionContext::default()
            },
            &ToolCallInput {
                tool_call_id: "call-1".into(),
                tool_name: HEADLESS_TOOL_WRITE.into(),
                input: json!({}),
            },
            "src/a.rs",
        );
        assert_eq!(reservation["reservationId"], "reservation-run-1-call-1");
        assert_eq!(reservation["conflictPolicy"], "deny_without_override");
        assert!(headless_tool_is_command(HEADLESS_TOOL_COMMAND));
        assert!(!headless_tool_is_command(HEADLESS_TOOL_READ));
    }

    #[test]
    fn headless_tool_argument_and_output_helpers_cover_invalid_boundaries() {
        assert_eq!(
            required_tool_string(&json!({"path": " README.md "}), "path").expect("required string"),
            " README.md "
        );
        for input in [json!({}), json!({"path": " "}), json!({"path": 1})] {
            assert_eq!(
                required_tool_string(&input, "path")
                    .expect_err("reject missing string")
                    .code,
                "agent_core_headless_tool_argument_missing"
            );
        }
        assert_eq!(
            required_tool_string_array(&json!({"argv": ["echo", "hello"]}), "argv")
                .expect("string array"),
            vec!["echo", "hello"]
        );
        assert!(required_tool_string_array(&json!({}), "argv").is_err());
        assert!(required_tool_string_array(&json!({"argv": []}), "argv").is_err());
        assert!(required_tool_string_array(&json!({"argv": [""]}), "argv").is_err());
        assert!(required_tool_string_array(&json!({"argv": [1]}), "argv").is_err());

        assert!(validate_headless_argv(&["echo".into(), "hello".into()]).is_ok());
        assert!(validate_headless_argv(&[]).is_err());
        assert!(validate_headless_argv(&[" ".into()]).is_err());
        assert!(validate_headless_argv(&["echo".into(), "bad\0value".into()]).is_err());

        let complete = read_limited_output(std::io::Cursor::new(b"hello"), 10);
        assert_eq!(complete.bytes, b"hello");
        assert!(!complete.truncated);
        let truncated = read_limited_output(std::io::Cursor::new(b"hello world"), 5);
        assert_eq!(truncated.bytes, b"hello");
        assert!(truncated.truncated);

        let denied = core_error_to_tool_execution_error(CoreError::invalid_request(
            "workspace_write_denied",
            "denied",
        ));
        assert_eq!(denied.category, crate::ToolErrorCategory::PolicyDenied);
        let invalid = core_error_to_tool_execution_error(CoreError::invalid_request(
            "argument_missing",
            "missing",
        ));
        assert_eq!(invalid.category, crate::ToolErrorCategory::InvalidInput);
        let retryable = core_error_to_tool_execution_error(CoreError::system_fault(
            "provider_failed",
            "failed",
        ));
        assert_eq!(
            retryable.category,
            crate::ToolErrorCategory::RetryableProviderToolFailure
        );
        assert_eq!(
            tool_execution_error_to_core_error(retryable).code,
            "provider_failed"
        );
    }

    #[test]
    fn headless_write_supports_creating_an_intentionally_empty_file() {
        let root = unique_test_dir("headless-empty-write");
        let runtime = HeadlessProductionToolRuntime::new(Some(&root), true, Vec::new())
            .expect("create writable runtime");

        runtime
            .write(
                &ToolExecutionContext::default(),
                &ToolCallInput {
                    tool_call_id: "call-empty-write".into(),
                    tool_name: HEADLESS_TOOL_WRITE.into(),
                    input: json!({"path": "empty.txt", "content": ""}),
                },
            )
            .expect("write empty file");

        assert_eq!(
            fs::read(root.join("empty.txt")).expect("read empty file"),
            b""
        );
    }

    #[test]
    fn headless_production_tools_cover_the_complete_file_lifecycle() {
        assert_eq!(
            HeadlessProductionToolRuntime::new(None, false, Vec::new())
                .expect_err("workspace is required")
                .code,
            "agent_core_headless_workspace_missing"
        );
        let root = unique_test_dir("headless-file-lifecycle");
        assert_eq!(
            HeadlessProductionToolRuntime::new(Some(&root.join("missing")), false, Vec::new(),)
                .expect_err("workspace must exist")
                .code,
            "agent_core_headless_workspace_unavailable"
        );
        let runtime = HeadlessProductionToolRuntime::new(Some(&root), true, Vec::new())
            .expect("create writable runtime");
        assert_eq!(
            runtime.tool_names(),
            vec![
                "read", "list", "todo", "write", "patch", "delete", "move", "replace", "command"
            ]
        );
        assert_eq!(runtime.openai_tool_definitions().len(), 9);
        assert_eq!(runtime.openai_response_tool_definitions().len(), 9);
        assert_eq!(runtime.openai_tool_definitions()[0]["type"], "function");
        assert_eq!(
            runtime.openai_response_tool_definitions()[0]["name"],
            "read"
        );

        let context = ToolExecutionContext {
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            turn_index: 1,
            context_epoch: "turn-1".into(),
            telemetry_attributes: BTreeMap::new(),
        };
        let call = |id: &str, name: &str, input: JsonValue| ToolCallInput {
            tool_call_id: id.into(),
            tool_name: name.into(),
            input,
        };

        let written = runtime
            .write(
                &context,
                &call(
                    "write-1",
                    HEADLESS_TOOL_WRITE,
                    json!({"path": "nested/file.txt", "content": "old old"}),
                ),
            )
            .expect("write nested file");
        assert_eq!(written.output["bytes"], 7);
        assert_eq!(written.output["rollback"]["existed"], false);
        assert_eq!(
            runtime
                .read(&call(
                    "read-1",
                    HEADLESS_TOOL_READ,
                    json!({"path": "nested/file.txt"}),
                ))
                .expect("read nested file")
                .output["content"],
            "old old"
        );
        assert_eq!(
            runtime
                .read(&call("read-missing", HEADLESS_TOOL_READ, json!({})))
                .expect_err("read path is required")
                .code,
            "agent_core_headless_tool_argument_missing"
        );

        let preview = runtime
            .replace_text(
                &context,
                &call(
                    "replace-preview",
                    HEADLESS_TOOL_REPLACE,
                    json!({
                        "path": "nested/file.txt",
                        "search": "old",
                        "replacement": "new",
                        "dryRun": true,
                        "maxReplacements": 1,
                    }),
                ),
            )
            .expect("preview replacement");
        assert_eq!(preview.output["replacements"], 1);
        assert_eq!(preview.output["changedFiles"][0]["truncated"], true);
        assert_eq!(
            fs::read_to_string(root.join("nested/file.txt")).unwrap(),
            "old old"
        );

        let replaced = runtime
            .replace_text(
                &context,
                &call(
                    "replace-apply",
                    HEADLESS_TOOL_REPLACE,
                    json!({
                        "path": "nested",
                        "search": "old",
                        "replacement": "new",
                    }),
                ),
            )
            .expect("apply replacement");
        assert_eq!(replaced.output["replacements"], 2);
        assert_eq!(
            fs::read_to_string(root.join("nested/file.txt")).unwrap(),
            "new new"
        );
        assert_eq!(
            runtime
                .replace_text(
                    &context,
                    &call(
                        "replace-missing",
                        HEADLESS_TOOL_REPLACE,
                        json!({"path": ".", "search": "new"}),
                    ),
                )
                .expect_err("replacement is required")
                .code,
            "agent_core_headless_replace_missing_replacement"
        );

        fs::write(root.join("binary.bin"), [0xff, 0xfe]).expect("write binary fixture");
        let skipped = runtime
            .replace_text(
                &context,
                &call(
                    "replace-binary",
                    HEADLESS_TOOL_REPLACE,
                    json!({"path": ".", "search": "absent", "replacement": "value"}),
                ),
            )
            .expect("skip non-UTF8 file");
        assert!(skipped.output["skippedFiles"]
            .as_array()
            .expect("skipped files")
            .iter()
            .any(|item| item["path"] == "binary.bin"));

        assert_eq!(
            runtime
                .move_path(
                    &context,
                    &call(
                        "move-noop",
                        HEADLESS_TOOL_MOVE,
                        json!({"from": "nested/file.txt", "to": "nested/file.txt"}),
                    ),
                )
                .expect_err("reject no-op move")
                .code,
            "agent_core_headless_move_noop"
        );
        fs::write(root.join("occupied.txt"), "occupied").expect("write occupied fixture");
        assert_eq!(
            runtime
                .move_path(
                    &context,
                    &call(
                        "move-occupied",
                        HEADLESS_TOOL_MOVE,
                        json!({"from": "nested/file.txt", "to": "occupied.txt"}),
                    ),
                )
                .expect_err("reject overwrite move")
                .code,
            "agent_core_headless_move_target_exists"
        );
        let moved = runtime
            .move_path(
                &context,
                &call(
                    "move-1",
                    HEADLESS_TOOL_MOVE,
                    json!({"from": "nested/file.txt", "to": "moved/file.txt"}),
                ),
            )
            .expect("move file");
        assert_eq!(moved.output["kind"], "file");
        assert!(!root.join("nested/file.txt").exists());
        assert!(root.join("moved/file.txt").exists());

        fs::create_dir_all(root.join("delete-dir/child")).expect("create delete fixture");
        assert_eq!(
            runtime
                .delete(
                    &context,
                    &call(
                        "delete-dir-denied",
                        HEADLESS_TOOL_DELETE,
                        json!({"path": "delete-dir"}),
                    ),
                )
                .expect_err("directory deletion needs recursive")
                .code,
            "agent_core_headless_delete_directory_requires_recursive"
        );
        let deleted_dir = runtime
            .delete(
                &context,
                &call(
                    "delete-dir",
                    HEADLESS_TOOL_DELETE,
                    json!({"path": "delete-dir", "recursive": true}),
                ),
            )
            .expect("delete directory recursively");
        assert_eq!(deleted_dir.output["kind"], "directory");
        let deleted_file = runtime
            .delete(
                &context,
                &call(
                    "delete-file",
                    HEADLESS_TOOL_DELETE,
                    json!({"path": "moved/file.txt"}),
                ),
            )
            .expect("delete file");
        assert_eq!(deleted_file.output["kind"], "file");

        let patch = "diff --git a/new.txt b/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+new\n";
        let patched = runtime
            .apply_patch(
                &context,
                &call("patch-create", HEADLESS_TOOL_PATCH, json!({"patch": patch})),
            )
            .expect("git apply supports standalone worktrees");
        assert_eq!(patched.output["changedFiles"], json!(["new.txt"]));
        assert_eq!(fs::read_to_string(root.join("new.txt")).unwrap(), "new\n");
        assert_eq!(
            runtime
                .apply_patch(
                    &context,
                    &call(
                        "patch-conflict",
                        HEADLESS_TOOL_PATCH,
                        json!({"patch": patch})
                    ),
                )
                .expect_err("reapplying a create patch must fail its check")
                .code,
            "agent_core_headless_patch_check_failed"
        );

        assert_eq!(
            runtime
                .list(&call(
                    "list-file",
                    HEADLESS_TOOL_LIST,
                    json!({"path": "occupied.txt"}),
                ))
                .expect_err("cannot recursively list a file")
                .code,
            "agent_core_headless_list_failed"
        );
        let report = runtime
            .dispatch_batch(
                "project-1",
                "run-1",
                2,
                &[call(
                    "registry-read",
                    HEADLESS_TOOL_READ,
                    json!({"path": "occupied.txt"}),
                )],
            )
            .expect("dispatch registered read");
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].outcomes.len(), 1);
        assert!(matches!(
            &report.groups[0].outcomes[0],
            ToolDispatchOutcome::Succeeded(_)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn headless_command_policy_timeout_and_file_rollback_are_enforced() {
        let root = unique_test_dir("headless-command-rollback");
        let runtime =
            HeadlessProductionToolRuntime::new_with_modes(Some(&root), false, true, Vec::new())
                .expect("create command runtime");
        assert_eq!(runtime.tool_names(), vec!["read", "list", "todo", "command"]);
        let context = ToolExecutionContext {
            run_id: "run-command".into(),
            context_epoch: "turn-3".into(),
            ..ToolExecutionContext::default()
        };
        let command_call = |id: &str, input: JsonValue| ToolCallInput {
            tool_call_id: id.into(),
            tool_name: HEADLESS_TOOL_COMMAND.into(),
            input,
        };

        let success = runtime
            .command(
                &context,
                &command_call(
                    "command-ok",
                    json!({"argv": ["sh", "-c", "printf 'hello'"]}),
                ),
            )
            .expect("run successful command");
        assert_eq!(success.output["ok"], true);
        assert_eq!(success.output["stdout"], "hello");
        assert_eq!(success.output["contextEpoch"], "turn-3");

        let failure = runtime
            .command(
                &context,
                &command_call(
                    "command-failed",
                    json!({"argv": ["sh", "-c", "printf 'bad' >&2; exit 7"]}),
                ),
            )
            .expect("capture non-zero command");
        assert_eq!(failure.output["ok"], false);
        assert_eq!(failure.output["exitCode"], 7);
        assert_eq!(failure.output["stderr"], "bad");
        assert_eq!(
            runtime
                .command(
                    &context,
                    &command_call(
                        "command-cwd-file",
                        json!({"argv": ["pwd"], "cwd": "file.txt"}),
                    ),
                )
                .expect_err("file cwd is invalid")
                .code,
            "agent_core_headless_path_unavailable"
        );

        let timed_out = runtime
            .run_process(
                &context,
                &command_call("command-timeout", json!({})),
                vec!["sh".into(), "-c".into(), "sleep 1".into()],
                root.clone(),
                None,
                Some(20),
            )
            .expect("time out command");
        assert!(timed_out.timed_out);
        assert_eq!(
            runtime
                .run_process(
                    &context,
                    &command_call("command-missing", json!({})),
                    vec!["xero-command-that-does-not-exist".into()],
                    root.clone(),
                    None,
                    Some(100),
                )
                .expect_err("missing command must be typed")
                .code,
            "agent_core_headless_command_spawn_failed"
        );

        let read_only_policy = HeadlessProductionToolPolicy {
            allow_workspace_writes: false,
            allow_commands: false,
            allowed_tools: None,
        };
        assert!(matches!(
            read_only_policy.evaluate(
                &headless_command_descriptor(),
                &command_call("policy-command", json!({}))
            ),
            ToolPolicyDecision::Deny { .. }
        ));
        assert!(matches!(
            read_only_policy.evaluate(
                &headless_write_descriptor(),
                &ToolCallInput {
                    tool_call_id: "policy-write".into(),
                    tool_name: HEADLESS_TOOL_WRITE.into(),
                    input: json!({}),
                }
            ),
            ToolPolicyDecision::Deny { .. }
        ));
        assert_eq!(
            read_only_policy.evaluate(
                &headless_read_descriptor(),
                &ToolCallInput {
                    tool_call_id: "policy-read".into(),
                    tool_name: HEADLESS_TOOL_READ.into(),
                    input: json!({}),
                }
            ),
            ToolPolicyDecision::Allow
        );

        let rollback = HeadlessFileRollback {
            workspace_root: root.clone(),
        };
        let write_descriptor = headless_write_descriptor();
        let write_call = ToolCallInput {
            tool_call_id: "rollback-existing".into(),
            tool_name: HEADLESS_TOOL_WRITE.into(),
            input: json!({"path": "rollback.txt", "content": "new"}),
        };
        fs::write(root.join("rollback.txt"), "old").expect("write rollback original");
        let checkpoint = rollback
            .checkpoint_before(&write_call, &write_descriptor)
            .expect("checkpoint existing file")
            .expect("write checkpoint");
        fs::write(root.join("rollback.txt"), "new").expect("mutate rollback fixture");
        rollback
            .rollback_after_failure(
                &write_call,
                &write_descriptor,
                &checkpoint,
                &ToolExecutionError::retryable("fixture_failure", "fixture"),
            )
            .expect("restore existing file");
        assert_eq!(
            fs::read_to_string(root.join("rollback.txt")).unwrap(),
            "old"
        );

        let new_call = ToolCallInput {
            tool_call_id: "rollback-new".into(),
            tool_name: HEADLESS_TOOL_WRITE.into(),
            input: json!({"path": "created.txt", "content": "new"}),
        };
        let checkpoint = rollback
            .checkpoint_before(&new_call, &write_descriptor)
            .expect("checkpoint new file")
            .expect("new file checkpoint");
        fs::write(root.join("created.txt"), "new").expect("create rollback fixture");
        rollback
            .rollback_after_failure(
                &new_call,
                &write_descriptor,
                &checkpoint,
                &ToolExecutionError::retryable("fixture_failure", "fixture"),
            )
            .expect("remove newly created file");
        assert!(!root.join("created.txt").exists());

        assert!(rollback
            .checkpoint_before(&new_call, &headless_read_descriptor())
            .expect("non-write checkpoint")
            .is_none());
        assert_eq!(
            rollback
                .rollback_after_failure(
                    &new_call,
                    &headless_read_descriptor(),
                    &json!({}),
                    &ToolExecutionError::retryable("fixture_failure", "fixture"),
                )
                .expect("non-write rollback")["kind"],
            "rollback_not_required"
        );
    }

    #[test]
    fn headless_prompts_effort_urls_and_timeouts_cover_all_agent_modes() {
        assert!(headless_system_prompt_for_agent("plan", None).contains("planning-only"));
        assert!(
            headless_system_prompt_for_agent("debug", Some(Path::new("/repo")))
                .contains("root cause")
        );
        assert!(headless_system_prompt_for_agent("engineer", None).contains("configured workspace"));

        for agent in ["ask", "computer_use", "plan", "crawl", "agent_create"] {
            assert!(!headless_agent_allows_workspace_writes(agent));
        }
        for agent in ["engineer", "debug", "custom"] {
            assert!(headless_agent_allows_workspace_writes(agent));
        }

        for (input, expected) in [
            (Some(" minimal "), Some("minimal")),
            (Some("LOW"), Some("low")),
            (Some("medium"), Some("medium")),
            (Some("high"), Some("high")),
            (Some("x_high"), Some("xhigh")),
            (Some("xhigh"), Some("xhigh")),
            (Some("unknown"), None),
            (None, None),
        ] {
            assert_eq!(
                normalize_headless_thinking_effort(input).as_deref(),
                expected
            );
        }
        assert_eq!(deepseek_headless_effort("xhigh"), "max");
        assert_eq!(deepseek_headless_effort("medium"), "high");
        assert_eq!(
            clamp_openai_codex_headless_effort("gpt-5.1", "xhigh"),
            "high"
        );
        assert_eq!(
            clamp_openai_codex_headless_effort("gpt-5.1-codex-mini", "low"),
            "medium"
        );
        assert_eq!(
            clamp_openai_codex_headless_effort("gpt-5.1-codex-mini", "xhigh"),
            "high"
        );
        assert_eq!(
            clamp_openai_codex_headless_effort("openai/gpt-5.6", "minimal"),
            "low"
        );
        assert_eq!(
            clamp_openai_codex_headless_effort("other", "unexpected"),
            "medium"
        );

        assert_eq!(
            openai_compatible_chat_url("https://example.com/v1/chat/completions")
                .expect("complete chat URL"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            openai_compatible_chat_url("http://localhost:11434/v1").expect("localhost URL"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            openai_compatible_chat_url("http://[::1]:11434/v1").expect("IPv6 loopback URL"),
            "http://[::1]:11434/v1/chat/completions"
        );
        assert!(openai_compatible_chat_url(" ").is_err());
        assert!(openai_compatible_chat_url("http://example.com/v1").is_err());
        assert!(is_local_http_endpoint("http://0.0.0.0:11434"));
        assert!(!is_local_http_endpoint("https://localhost:11434"));
        assert!(!is_local_http_endpoint("not a URL"));
        assert_eq!(normalize_timeout(0), DEFAULT_HEADLESS_PROVIDER_TIMEOUT_MS);
        assert_eq!(normalize_timeout(42), 42);
    }

    #[cfg(unix)]
    #[test]
    fn workspace_path_resolution_rejects_traversal_symlinks_and_case_variants() {
        let root = unique_test_dir("headless-path-boundaries");
        fs::create_dir_all(root.join("nested/existing")).expect("create nested fixture");
        fs::create_dir_all(root.join(".GIT")).expect("create protected case fixture");
        assert_eq!(
            resolve_workspace_path_for_root(&root, "nested/missing/file.txt", true)
                .expect("allow missing nested leaf"),
            root.join("nested/missing/file.txt")
        );
        assert_eq!(
            resolve_workspace_path_for_root(&root, "../outside", true)
                .expect_err("reject parent traversal")
                .code,
            "agent_core_headless_path_denied"
        );
        assert_eq!(
            resolve_workspace_path_for_root(&root, ".GIT/config", true)
                .expect_err("reject case-insensitive protected path")
                .code,
            "agent_core_headless_path_protected"
        );
        assert_eq!(
            resolve_workspace_path_for_root(&root, "missing.txt", false)
                .expect_err("reject unavailable read leaf")
                .code,
            "agent_core_headless_path_unavailable"
        );

        let outside = root
            .parent()
            .expect("fixture parent")
            .join("headless-outside.txt");
        fs::write(&outside, "outside").expect("write outside fixture");
        std::os::unix::fs::symlink(&outside, root.join("linked.txt")).expect("create symlink");
        assert_eq!(
            resolve_workspace_path_for_root(&root, "linked.txt", true)
                .expect_err("reject symlink leaf")
                .code,
            "agent_core_headless_path_denied"
        );
        assert_eq!(
            resolve_workspace_path_for_root(&root.join("missing-root"), ".", false)
                .expect_err("reject missing root")
                .code,
            "agent_core_headless_workspace_unavailable"
        );
    }

    #[test]
    fn workspace_listing_and_snapshot_helpers_are_deterministic_and_bounded() {
        let root = unique_test_dir("headless-helper-boundaries");
        fs::create_dir_all(root.join("target/cache")).expect("create skipped fixture");
        fs::create_dir_all(root.join("src")).expect("create src fixture");
        fs::write(root.join("b.txt"), "b").expect("write b fixture");
        fs::write(root.join("A.txt"), "a").expect("write a fixture");

        let listing = collect_workspace_listing(&root, &root, 1).expect("bounded listing");
        assert_eq!(listing.entries.len(), 1);
        assert!(listing.truncated);
        assert!(listing.omitted_entry_count > 0);
        assert!(listing
            .skipped_directories
            .iter()
            .any(|path| path == "target"));
        assert!(sorted_workspace_entries(&root)
            .expect("sorted entries")
            .windows(2)
            .all(
                |pair| pair[0].file_name().to_string_lossy().to_ascii_lowercase()
                    <= pair[1].file_name().to_string_lossy().to_ascii_lowercase()
            ));
        assert_eq!(
            sorted_workspace_entries(&root.join("A.txt"))
                .expect_err("cannot list a file")
                .code,
            "agent_core_headless_list_failed"
        );
        assert_eq!(
            workspace_relative_path(&root, &root.join("src")),
            Some("src".into())
        );
        assert!(workspace_relative_path(&root, root.parent().expect("fixture parent")).is_none());
        assert!(skipped_workspace_dir("node_modules"));
        assert!(!skipped_workspace_dir("src"));

        let mut direct = WorkspaceListing::default();
        push_workspace_listing_entry(&mut direct, 2, "directory", "src");
        push_workspace_listing_entry(&mut direct, 2, "file", "README.md");
        push_workspace_listing_entry(&mut direct, 2, "file", "omitted.txt");
        assert_eq!(direct.directories, vec!["src"]);
        assert_eq!(direct.files, vec!["README.md"]);
        assert_eq!(direct.omitted_entry_count, 1);

        let mut snapshot = run_snapshot_fixture();
        snapshot.messages = (0..13)
            .map(|index| RuntimeMessage {
                id: index,
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                role: MessageRole::User,
                content: format!("message-{index}-{}", "🦀".repeat(300)),
                provider_metadata: None,
                created_at: "2026-07-18T12:00:00Z".into(),
            })
            .collect();
        let summary = compact_summary_from_snapshot(&snapshot);
        assert!(!summary.contains("message-0-"));
        assert!(summary.contains("message-12-"));
        assert!(summary.contains("..."));

        let first_hash = headless_context_hash(&snapshot, 1);
        assert_eq!(first_hash, headless_context_hash(&snapshot, 1));
        assert_ne!(first_hash, headless_context_hash(&snapshot, 2));
        snapshot.events.extend([
            runtime_event(
                1,
                RuntimeEventKind::ToolRegistrySnapshot,
                json!({"turnIndex": 2}),
            ),
            runtime_event(
                2,
                RuntimeEventKind::ToolRegistrySnapshot,
                json!({"turnIndex": "bad"}),
            ),
            runtime_event(3, RuntimeEventKind::RunStarted, json!({"turnIndex": 99})),
        ]);
        assert_eq!(next_headless_provider_turn_index(&snapshot), 3);

        assert_eq!(stable_bytes_hash(b"same"), stable_bytes_hash(b"same"));
        assert_ne!(stable_bytes_hash(b"same"), stable_bytes_hash(b"different"));
        assert_eq!(stable_bytes_hash(b"same").len(), 16);
        assert!(generate_headless_id("fixture").starts_with("fixture-"));
        assert_eq!(truncate_text("short", 5), "short");
        assert_eq!(truncate_text("🦀ab", 4), "🦀...");
        assert_eq!(truncate_text("value", 0), "...");
    }

    #[test]
    fn replayable_chat_history_covers_every_role_and_provider_metadata_shape() {
        let mut snapshot = run_snapshot_fixture();
        let message = |id, role, content: &str, provider_metadata| RuntimeMessage {
            id,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            role,
            content: content.into(),
            provider_metadata,
            created_at: "2026-07-18T12:00:00Z".into(),
        };
        snapshot.messages = vec![
            message(1, MessageRole::System, "custom system", None),
            message(2, MessageRole::Developer, "developer guidance", None),
            message(3, MessageRole::User, "user prompt", None),
            message(4, MessageRole::Assistant, "plain assistant", None),
            message(
                5,
                MessageRole::Assistant,
                "reasoned assistant",
                Some(RuntimeMessageProviderMetadata::assistant_turn(
                    "provider-5",
                    Some("reasoning".into()),
                    None,
                    Vec::new(),
                )),
            ),
            message(
                6,
                MessageRole::Assistant,
                "",
                Some(RuntimeMessageProviderMetadata::assistant_tool_calls(
                    "provider-6",
                    vec![RuntimeProviderToolCallMetadata {
                        tool_call_id: "call-6".into(),
                        provider_tool_name: "read_file".into(),
                        arguments: json!({ "path": "src/lib.rs" }),
                    }],
                )),
            ),
            message(
                7,
                MessageRole::Tool,
                "metadata result",
                Some(RuntimeMessageProviderMetadata::tool_result(
                    "provider-7",
                    "call-6",
                    "read_file",
                    "provider-6",
                )),
            ),
            message(
                8,
                MessageRole::Tool,
                r#"{"toolCallId":"legacy-camel"}"#,
                None,
            ),
            message(
                9,
                MessageRole::Tool,
                r#"{"tool_call_id":"legacy-snake"}"#,
                None,
            ),
            message(10, MessageRole::Tool, "not-json", None),
        ];

        let messages = replayable_openai_chat_messages_from_snapshot(&snapshot);
        assert_eq!(messages[0]["content"], "custom system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[3]["content"], "plain assistant");
        assert_eq!(messages[4]["content"], "reasoned assistant");
        assert_eq!(messages[5]["content"], JsonValue::Null);
        assert_eq!(messages[5]["tool_calls"][0]["id"], "call-6");
        assert_eq!(messages[6]["tool_call_id"], "call-6");
        assert_eq!(messages[7]["tool_call_id"], "legacy-camel");
        assert_eq!(messages[8]["tool_call_id"], "legacy-snake");
        assert!(messages[9].get("tool_call_id").is_none());

        for error in [
            ToolExecutionError::invalid_input("invalid", "invalid input"),
            ToolExecutionError::sandbox_denied("denied", "sandbox denied"),
            ToolExecutionError::timeout("timeout", "timed out"),
            ToolExecutionError::retryable("retry", "retry later"),
        ] {
            let json = tool_execution_error_json(&error);
            assert_eq!(json["code"], error.code);
            assert_eq!(json["retryable"], error.retryable);
            assert!(!json["category"].as_str().unwrap_or_default().is_empty());
        }
    }

    fn run_snapshot_fixture() -> RunSnapshot {
        RunSnapshot {
            trace_id: "0123456789abcdef0123456789abcdef".into(),
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "fixture system prompt".into(),
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-1".into(),
            provider_id: "provider-1".into(),
            model_id: "model-1".into(),
            status: RunStatus::Running,
            prompt: "Fixture prompt".into(),
            messages: Vec::new(),
            events: Vec::new(),
            context_manifests: Vec::new(),
        }
    }

    fn runtime_event(
        id: i64,
        event_kind: RuntimeEventKind,
        payload: JsonValue,
    ) -> crate::RuntimeEvent {
        crate::RuntimeEvent {
            id,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            event_kind,
            trace: RuntimeTraceContext::for_run(
                "0123456789abcdef0123456789abcdef",
                "run-1",
                "fixture",
            ),
            payload,
            created_at: "2026-07-18T12:00:00Z".into(),
        }
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!("xero-{label}-{nanos}"));
        fs::create_dir_all(&path).expect("create temp workspace");
        fs::canonicalize(path).expect("canonical temp workspace")
    }
}
