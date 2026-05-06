use serde::{Deserialize, Serialize};

use crate::commands::{ProviderModelThinkingEffortDto, RuntimeRunControlInputDto};

pub const SUPERVISOR_PROTOCOL_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorProcessStatus {
    Starting,
    Running,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorProtocolDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorToolCallState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorSkillLifecycleStage {
    Discovery,
    Install,
    Invoke,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorSkillLifecycleResult {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorSkillCacheStatus {
    Miss,
    Hit,
    Refreshed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorSkillSourceMetadata {
    pub repo: String,
    pub path: String,
    pub reference: String,
    pub tree_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorSkillDiagnostic {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitToolResultScope {
    Staged,
    Unstaged,
    Worktree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebToolResultContentKind {
    Html,
    PlainText,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandToolResultSummary {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_redacted: bool,
    pub stderr_redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileToolResultSummary {
    pub path: Option<String>,
    pub scope: Option<String>,
    pub line_count: Option<usize>,
    pub match_count: Option<usize>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitToolResultSummary {
    pub scope: Option<GitToolResultScope>,
    pub changed_files: usize,
    pub truncated: bool,
    pub base_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebToolResultSummary {
    pub target: String,
    pub result_count: Option<usize>,
    pub final_url: Option<String>,
    pub content_kind: Option<WebToolResultContentKind>,
    pub content_type: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserComputerUseSurface {
    Browser,
    ComputerUse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserComputerUseActionStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserComputerUseToolResultSummary {
    pub surface: BrowserComputerUseSurface,
    pub action: String,
    pub status: BrowserComputerUseActionStatus,
    pub target: Option<String>,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpCapabilityKind {
    Tool,
    Resource,
    Prompt,
    Command,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpCapabilityToolResultSummary {
    pub server_id: String,
    pub capability_kind: McpCapabilityKind,
    pub capability_id: String,
    pub capability_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ToolResultSummary {
    Command(CommandToolResultSummary),
    File(FileToolResultSummary),
    Git(GitToolResultSummary),
    Web(WebToolResultSummary),
    BrowserComputerUse(BrowserComputerUseToolResultSummary),
    McpCapability(McpCapabilityToolResultSummary),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorActionAnswerShape {
    PlainText,
    TerminalInput,
    SingleChoice,
    MultiChoice,
    ShortText,
    LongText,
    Number,
    Date,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorActionRequiredOption {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorPlanItemStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorPlanItem {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub status: SupervisorPlanItemStatus,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SupervisorLiveEventPayload {
    Transcript {
        text: String,
    },
    Tool {
        tool_call_id: String,
        tool_name: String,
        tool_state: SupervisorToolCallState,
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_summary: Option<ToolResultSummary>,
    },
    Activity {
        code: String,
        title: String,
        detail: Option<String>,
    },
    Skill {
        skill_id: String,
        stage: SupervisorSkillLifecycleStage,
        result: SupervisorSkillLifecycleResult,
        detail: String,
        source: SupervisorSkillSourceMetadata,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_status: Option<SupervisorSkillCacheStatus>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diagnostic: Option<SupervisorSkillDiagnostic>,
    },
    ActionRequired {
        action_id: String,
        boundary_id: String,
        action_type: String,
        title: String,
        detail: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answer_shape: Option<SupervisorActionAnswerShape>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        options: Option<Vec<SupervisorActionRequiredOption>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        allow_multiple: Option<bool>,
    },
    Plan {
        plan_id: String,
        items: Vec<SupervisorPlanItem>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_changed_id: Option<String>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSupervisorLaunchContext {
    pub provider_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_id: Option<String>,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_effort: Option<ProviderModelThinkingEffortDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[allow(clippy::large_enum_variant)]
pub enum SupervisorStartupMessage {
    Ready {
        protocol_version: u8,
        project_id: String,
        run_id: String,
        supervisor_kind: String,
        transport_kind: String,
        endpoint: String,
        started_at: String,
        supervisor_pid: u32,
        child_pid: Option<u32>,
        status: SupervisorProcessStatus,
        launch_context: RuntimeSupervisorLaunchContext,
    },
    Error {
        protocol_version: u8,
        code: String,
        message: String,
        retryable: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SupervisorControlRequest {
    Probe {
        protocol_version: u8,
        project_id: String,
        run_id: String,
    },
    Stop {
        protocol_version: u8,
        project_id: String,
        run_id: String,
    },
    Attach {
        protocol_version: u8,
        project_id: String,
        run_id: String,
        after_sequence: Option<u64>,
    },
    SubmitInput {
        protocol_version: u8,
        project_id: String,
        run_id: String,
        session_id: String,
        flow_id: Option<String>,
        action_id: String,
        boundary_id: String,
        input: String,
    },
    QueueControls {
        protocol_version: u8,
        project_id: String,
        run_id: String,
        session_id: String,
        flow_id: Option<String>,
        controls: Option<RuntimeRunControlInputDto>,
        prompt: Option<String>,
    },
}

impl SupervisorControlRequest {
    pub fn probe(project_id: impl Into<String>, run_id: impl Into<String>) -> Self {
        Self::Probe {
            protocol_version: SUPERVISOR_PROTOCOL_VERSION,
            project_id: project_id.into(),
            run_id: run_id.into(),
        }
    }

    pub fn stop(project_id: impl Into<String>, run_id: impl Into<String>) -> Self {
        Self::Stop {
            protocol_version: SUPERVISOR_PROTOCOL_VERSION,
            project_id: project_id.into(),
            run_id: run_id.into(),
        }
    }

    pub fn attach(
        project_id: impl Into<String>,
        run_id: impl Into<String>,
        after_sequence: Option<u64>,
    ) -> Self {
        Self::Attach {
            protocol_version: SUPERVISOR_PROTOCOL_VERSION,
            project_id: project_id.into(),
            run_id: run_id.into(),
            after_sequence,
        }
    }

    pub fn submit_input(
        project_id: impl Into<String>,
        run_id: impl Into<String>,
        session_id: impl Into<String>,
        flow_id: Option<String>,
        action_id: impl Into<String>,
        boundary_id: impl Into<String>,
        input: impl Into<String>,
    ) -> Self {
        Self::SubmitInput {
            protocol_version: SUPERVISOR_PROTOCOL_VERSION,
            project_id: project_id.into(),
            run_id: run_id.into(),
            session_id: session_id.into(),
            flow_id,
            action_id: action_id.into(),
            boundary_id: boundary_id.into(),
            input: input.into(),
        }
    }

    pub fn queue_controls(
        project_id: impl Into<String>,
        run_id: impl Into<String>,
        session_id: impl Into<String>,
        flow_id: Option<String>,
        controls: Option<RuntimeRunControlInputDto>,
        prompt: Option<String>,
    ) -> Self {
        Self::QueueControls {
            protocol_version: SUPERVISOR_PROTOCOL_VERSION,
            project_id: project_id.into(),
            run_id: run_id.into(),
            session_id: session_id.into(),
            flow_id,
            controls,
            prompt,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SupervisorControlResponse {
    ProbeResult {
        protocol_version: u8,
        project_id: String,
        run_id: String,
        status: SupervisorProcessStatus,
        last_heartbeat_at: Option<String>,
        last_checkpoint_sequence: u32,
        last_checkpoint_at: Option<String>,
        last_error: Option<SupervisorProtocolDiagnostic>,
        child_pid: Option<u32>,
    },
    StopAccepted {
        protocol_version: u8,
        project_id: String,
        run_id: String,
        child_pid: Option<u32>,
    },
    Attached {
        protocol_version: u8,
        project_id: String,
        run_id: String,
        after_sequence: Option<u64>,
        replayed_count: u32,
        replay_truncated: bool,
        oldest_available_sequence: Option<u64>,
        latest_sequence: Option<u64>,
    },
    SubmitInputAccepted {
        protocol_version: u8,
        project_id: String,
        run_id: String,
        action_id: String,
        boundary_id: String,
        delivered_at: String,
    },
    QueueControlsAccepted {
        protocol_version: u8,
        project_id: String,
        run_id: String,
        session_id: String,
        flow_id: Option<String>,
        pending_revision: u32,
        queued_at: String,
        prompt_queued: bool,
    },
    Event {
        protocol_version: u8,
        project_id: String,
        run_id: String,
        sequence: u64,
        created_at: String,
        replay: bool,
        item: SupervisorLiveEventPayload,
    },
    Error {
        protocol_version: u8,
        code: String,
        message: String,
        retryable: bool,
    },
}
