use serde::{Deserialize, Serialize};

pub const SUPERVISOR_PROTOCOL_VERSION: u8 = 1;
pub const SUPERVISOR_KIND_DETACHED_PTY: &str = "detached_pty";
pub const SUPERVISOR_TRANSPORT_KIND_TCP: &str = "tcp";

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
    },
    Activity {
        code: String,
        title: String,
        detail: Option<String>,
    },
    ActionRequired {
        action_id: String,
        boundary_id: String,
        action_type: String,
        title: String,
        detail: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
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
