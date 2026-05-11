use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{runtime_support::resolve_project_root, validate_non_empty, CommandResult},
    db::project_store::{
        self, project_record_kind_sql_value, ProjectRecordImportance, ProjectRecordRecord,
        ProjectRecordRedactionState, ProjectRecordVisibility,
    },
    state::DesktopState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListProjectContextRecordsRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListProjectContextRecordsResponseDto {
    pub schema: String,
    pub project_id: String,
    pub records: Vec<ProjectContextRecordSummaryDto>,
    pub ui_deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectContextRecordSummaryDto {
    pub record_id: String,
    pub record_kind: String,
    pub title: String,
    pub summary: Option<String>,
    pub text_preview: Option<String>,
    pub importance: String,
    pub redaction_state: String,
    pub visibility: String,
    pub freshness_state: String,
    pub tags: Vec<String>,
    pub related_paths: Vec<String>,
    pub supersedes_id: Option<String>,
    pub superseded_by_id: Option<String>,
    pub invalidated_at: Option<String>,
    pub runtime_agent_id: String,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub run_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteProjectContextRecordRequestDto {
    pub project_id: String,
    pub record_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProjectContextRecordResponseDto {
    pub schema: String,
    pub project_id: String,
    pub record_id: String,
    pub retrieval_removed: bool,
    pub ui_deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupersedeProjectContextRecordRequestDto {
    pub project_id: String,
    pub superseded_record_id: String,
    pub superseding_record_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SupersedeProjectContextRecordResponseDto {
    pub schema: String,
    pub project_id: String,
    pub superseded_record_id: String,
    pub superseding_record_id: String,
    pub retrieval_changed: bool,
    pub ui_deferred: bool,
}

#[tauri::command]
pub fn list_project_context_records<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListProjectContextRecordsRequestDto,
) -> CommandResult<ListProjectContextRecordsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let mut records = project_store::list_project_records(&repo_root, &request.project_id)?
        .into_iter()
        .map(project_context_record_summary)
        .collect::<Vec<_>>();
    records.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(ListProjectContextRecordsResponseDto {
        schema: "xero.project_context_record_list_command.v1".into(),
        project_id: request.project_id,
        records,
        ui_deferred: true,
    })
}

#[tauri::command]
pub fn delete_project_context_record<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: DeleteProjectContextRecordRequestDto,
) -> CommandResult<DeleteProjectContextRecordResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.record_id, "recordId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    project_store::delete_project_record(&repo_root, &request.project_id, &request.record_id)?;
    Ok(DeleteProjectContextRecordResponseDto {
        schema: "xero.project_context_record_delete_command.v1".into(),
        project_id: request.project_id,
        record_id: request.record_id,
        retrieval_removed: true,
        ui_deferred: true,
    })
}

#[tauri::command]
pub fn supersede_project_context_record<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SupersedeProjectContextRecordRequestDto,
) -> CommandResult<SupersedeProjectContextRecordResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.superseded_record_id, "supersededRecordId")?;
    validate_non_empty(&request.superseding_record_id, "supersedingRecordId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    project_store::mark_project_record_superseded_by(
        &repo_root,
        &request.project_id,
        &request.superseded_record_id,
        &request.superseding_record_id,
        &now_timestamp(),
    )?;
    Ok(SupersedeProjectContextRecordResponseDto {
        schema: "xero.project_context_record_supersede_command.v1".into(),
        project_id: request.project_id,
        superseded_record_id: request.superseded_record_id,
        superseding_record_id: request.superseding_record_id,
        retrieval_changed: true,
        ui_deferred: true,
    })
}

fn project_context_record_summary(record: ProjectRecordRecord) -> ProjectContextRecordSummaryDto {
    let is_visible = record.redaction_state == ProjectRecordRedactionState::Clean;
    ProjectContextRecordSummaryDto {
        record_id: record.record_id,
        record_kind: project_record_kind_sql_value(&record.record_kind).into(),
        title: record.title,
        summary: is_visible.then(|| text_preview(&record.summary)),
        text_preview: is_visible.then(|| text_preview(&record.text)),
        importance: importance_label(&record.importance).into(),
        redaction_state: redaction_label(&record.redaction_state).into(),
        visibility: visibility_label(&record.visibility).into(),
        freshness_state: record.freshness_state,
        tags: record.tags,
        related_paths: record.related_paths,
        supersedes_id: record.supersedes_id,
        superseded_by_id: record.superseded_by_id,
        invalidated_at: record.invalidated_at,
        runtime_agent_id: record.runtime_agent_id.as_str().to_string(),
        agent_definition_id: record.agent_definition_id,
        agent_definition_version: record.agent_definition_version,
        run_id: record.run_id,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn importance_label(importance: &ProjectRecordImportance) -> &'static str {
    match importance {
        ProjectRecordImportance::Low => "low",
        ProjectRecordImportance::Normal => "normal",
        ProjectRecordImportance::High => "high",
        ProjectRecordImportance::Critical => "critical",
    }
}

fn redaction_label(redaction: &ProjectRecordRedactionState) -> &'static str {
    match redaction {
        ProjectRecordRedactionState::Clean => "clean",
        ProjectRecordRedactionState::Redacted => "redacted",
        ProjectRecordRedactionState::Blocked => "blocked",
    }
}

fn visibility_label(visibility: &ProjectRecordVisibility) -> &'static str {
    match visibility {
        ProjectRecordVisibility::Workflow => "workflow",
        ProjectRecordVisibility::Retrieval => "retrieval",
        ProjectRecordVisibility::MemoryCandidate => "memory_candidate",
        ProjectRecordVisibility::Diagnostic => "diagnostic",
    }
}

fn text_preview(value: &str) -> String {
    value.chars().take(240).collect()
}
