use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Complete,
    Active,
    Pending,
    Blocked,
}

pub type PhaseStep = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveOperatorActionRequestDto {
    pub project_id: String,
    pub action_id: String,
    pub decision: String,
    pub user_answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeOperatorRunRequestDto {
    pub project_id: String,
    pub action_id: String,
    pub user_answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhaseSummaryDto {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub status: PhaseStatus,
    pub current_step: Option<PhaseStep>,
    pub task_count: u32,
    pub completed_tasks: u32,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperatorApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationRecordStatus {
    Pending,
    Passed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResumeHistoryStatus {
    Started,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperatorApprovalDto {
    pub action_id: String,
    pub session_id: Option<String>,
    pub flow_id: Option<String>,
    pub action_type: String,
    pub title: String,
    pub detail: String,
    pub user_answer: Option<String>,
    pub status: OperatorApprovalStatus,
    pub decision_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationRecordDto {
    pub id: u32,
    pub source_action_id: Option<String>,
    pub status: VerificationRecordStatus,
    pub summary: String,
    pub detail: Option<String>,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeHistoryEntryDto {
    pub id: u32,
    pub source_action_id: Option<String>,
    pub session_id: Option<String>,
    pub status: ResumeHistoryStatus,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveOperatorActionResponseDto {
    pub approval_request: OperatorApprovalDto,
    pub verification_record: VerificationRecordDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeOperatorRunResponseDto {
    pub approval_request: OperatorApprovalDto,
    pub resume_entry: ResumeHistoryEntryDto,
}
