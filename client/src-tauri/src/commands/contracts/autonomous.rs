use serde::{Deserialize, Serialize};

use super::runtime::RuntimeRunDiagnosticDto;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousRunStatusDto {
    Starting,
    Running,
    Paused,
    Cancelling,
    Cancelled,
    Stale,
    Failed,
    Stopped,
    Crashed,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousRunRecoveryStateDto {
    Healthy,
    RecoveryRequired,
    Terminal,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousUnitKindDto {
    Researcher,
    Planner,
    Executor,
    Verifier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousUnitStatusDto {
    Pending,
    Active,
    Blocked,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousUnitArtifactStatusDto {
    Pending,
    Recorded,
    Rejected,
    Redacted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousToolCallStateDto {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousVerificationOutcomeDto {
    Passed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillLifecycleStageDto {
    Discovery,
    Install,
    Invoke,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillLifecycleResultDto {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillCacheStatusDto {
    Miss,
    Hit,
    Refreshed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandResultDto {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub summary: String,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitToolResultScopeDto {
    Staged,
    Unstaged,
    Worktree,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebToolResultContentKindDto {
    Html,
    PlainText,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandToolResultSummaryDto {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_redacted: bool,
    pub stderr_redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileToolResultSummaryDto {
    pub path: Option<String>,
    pub scope: Option<String>,
    pub line_count: Option<usize>,
    pub match_count: Option<usize>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitToolResultSummaryDto {
    pub scope: Option<GitToolResultScopeDto>,
    pub changed_files: usize,
    pub truncated: bool,
    pub base_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebToolResultSummaryDto {
    pub target: String,
    pub result_count: Option<usize>,
    pub final_url: Option<String>,
    pub content_kind: Option<WebToolResultContentKindDto>,
    pub content_type: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ToolResultSummaryDto {
    Command(CommandToolResultSummaryDto),
    File(FileToolResultSummaryDto),
    Git(GitToolResultSummaryDto),
    Web(WebToolResultSummaryDto),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolResultPayloadDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub tool_state: AutonomousToolCallStateDto,
    pub command_result: Option<AutonomousCommandResultDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_summary: Option<ToolResultSummaryDto>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousVerificationEvidencePayloadDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub evidence_kind: String,
    pub label: String,
    pub outcome: AutonomousVerificationOutcomeDto,
    pub command_result: Option<AutonomousCommandResultDto>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPolicyDeniedPayloadDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub diagnostic_code: String,
    pub message: String,
    pub tool_name: Option<String>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillLifecycleSourceDto {
    pub repo: String,
    pub path: String,
    pub reference: String,
    pub tree_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillLifecycleCacheDto {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AutonomousSkillCacheStatusDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillLifecycleDiagnosticDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillLifecyclePayloadDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub stage: AutonomousSkillLifecycleStageDto,
    pub result: AutonomousSkillLifecycleResultDto,
    pub skill_id: String,
    pub source: AutonomousSkillLifecycleSourceDto,
    pub cache: AutonomousSkillLifecycleCacheDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<AutonomousSkillLifecycleDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutonomousArtifactPayloadDto {
    ToolResult(AutonomousToolResultPayloadDto),
    VerificationEvidence(AutonomousVerificationEvidencePayloadDto),
    PolicyDenied(AutonomousPolicyDeniedPayloadDto),
    SkillLifecycle(AutonomousSkillLifecyclePayloadDto),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousLifecycleReasonDto {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRunDto {
    pub project_id: String,
    pub run_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub supervisor_kind: String,
    pub status: AutonomousRunStatusDto,
    pub recovery_state: AutonomousRunRecoveryStateDto,
    pub active_unit_id: Option<String>,
    pub active_attempt_id: Option<String>,
    pub duplicate_start_detected: bool,
    pub duplicate_start_run_id: Option<String>,
    pub duplicate_start_reason: Option<String>,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
    pub last_checkpoint_at: Option<String>,
    pub paused_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub completed_at: Option<String>,
    pub crashed_at: Option<String>,
    pub stopped_at: Option<String>,
    pub pause_reason: Option<AutonomousLifecycleReasonDto>,
    pub cancel_reason: Option<AutonomousLifecycleReasonDto>,
    pub crash_reason: Option<AutonomousLifecycleReasonDto>,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticDto>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWorkflowLinkageDto {
    pub workflow_node_id: String,
    pub transition_id: String,
    pub causal_transition_id: Option<String>,
    pub handoff_transition_id: String,
    pub handoff_package_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousUnitDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub sequence: u32,
    pub kind: AutonomousUnitKindDto,
    pub status: AutonomousUnitStatusDto,
    pub summary: String,
    pub boundary_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_linkage: Option<AutonomousWorkflowLinkageDto>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub updated_at: String,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousUnitAttemptDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub attempt_number: u32,
    pub child_session_id: String,
    pub status: AutonomousUnitStatusDto,
    pub boundary_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_linkage: Option<AutonomousWorkflowLinkageDto>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub updated_at: String,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousUnitArtifactDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub artifact_kind: String,
    pub status: AutonomousUnitArtifactStatusDto,
    pub summary: String,
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<AutonomousArtifactPayloadDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousUnitHistoryEntryDto {
    pub unit: AutonomousUnitDto,
    pub latest_attempt: Option<AutonomousUnitAttemptDto>,
    #[serde(default)]
    pub artifacts: Vec<AutonomousUnitArtifactDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRunStateDto {
    pub run: Option<AutonomousRunDto>,
    pub unit: Option<AutonomousUnitDto>,
    pub attempt: Option<AutonomousUnitAttemptDto>,
    #[serde(default)]
    pub history: Vec<AutonomousUnitHistoryEntryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAutonomousRunRequestDto {
    pub project_id: String,
}

use super::runtime::RuntimeRunControlInputDto;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartAutonomousRunRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_controls: Option<RuntimeRunControlInputDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelAutonomousRunRequestDto {
    pub project_id: String,
    pub run_id: String,
}
