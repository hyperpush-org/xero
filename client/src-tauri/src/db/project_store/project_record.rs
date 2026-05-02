use std::path::Path;

use rand::RngCore;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, RuntimeAgentIdDto},
    db::database_path_for_repo,
};

use super::{
    agent_embeddings::embedding_for_storage,
    open_runtime_database,
    project_record_lance::{self, ProjectRecordRow},
    read_project_row, validate_non_empty_text,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRecordKind {
    AgentHandoff,
    ProjectFact,
    Decision,
    Constraint,
    Plan,
    Finding,
    Verification,
    Question,
    Artifact,
    ContextNote,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRecordImportance {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRecordRedactionState {
    Clean,
    Redacted,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRecordVisibility {
    Workflow,
    Retrieval,
    MemoryCandidate,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectRecordRecord {
    pub record_id: String,
    pub project_id: String,
    pub record_kind: ProjectRecordKind,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub agent_session_id: Option<String>,
    pub run_id: String,
    pub workflow_run_id: Option<String>,
    pub workflow_step_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub text: String,
    pub text_hash: String,
    pub content_json: Option<JsonValue>,
    pub content_hash: Option<String>,
    pub schema_name: Option<String>,
    pub schema_version: i32,
    pub importance: ProjectRecordImportance,
    pub confidence: Option<f64>,
    pub tags: Vec<String>,
    pub source_item_ids: Vec<String>,
    pub related_paths: Vec<String>,
    pub produced_artifact_refs: Vec<String>,
    pub redaction_state: ProjectRecordRedactionState,
    pub visibility: ProjectRecordVisibility,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewProjectRecordRecord {
    pub record_id: String,
    pub project_id: String,
    pub record_kind: ProjectRecordKind,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub agent_session_id: Option<String>,
    pub run_id: String,
    pub workflow_run_id: Option<String>,
    pub workflow_step_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub text: String,
    pub content_json: Option<JsonValue>,
    pub schema_name: Option<String>,
    pub schema_version: i32,
    pub importance: ProjectRecordImportance,
    pub confidence: Option<f64>,
    pub tags: Vec<String>,
    pub source_item_ids: Vec<String>,
    pub related_paths: Vec<String>,
    pub produced_artifact_refs: Vec<String>,
    pub redaction_state: ProjectRecordRedactionState,
    pub visibility: ProjectRecordVisibility,
    pub created_at: String,
}

pub fn generate_project_record_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "project-record-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub fn project_record_text_hash(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn project_record_content_hash(content_json: &JsonValue) -> Result<String, CommandError> {
    let bytes = serde_json::to_vec(content_json).map_err(|error| {
        CommandError::system_fault(
            "project_record_content_hash_failed",
            format!("Xero could not hash project record content: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn insert_project_record(
    repo_root: &Path,
    record: &NewProjectRecordRecord,
) -> Result<ProjectRecordRecord, CommandError> {
    validate_new_project_record(record)?;
    let store = open_store_with_project_check(repo_root, &record.project_id)?;
    let content_hash = record
        .content_json
        .as_ref()
        .map(project_record_content_hash)
        .transpose()?;
    let embedding = embedding_for_storage(&project_record_embedding_text(record))?;
    let row = ProjectRecordRow {
        record_id: record.record_id.clone(),
        project_id: record.project_id.clone(),
        record_kind: project_record_kind_sql_value(&record.record_kind).into(),
        runtime_agent_id: record.runtime_agent_id,
        agent_definition_id: record.agent_definition_id.clone(),
        agent_definition_version: record.agent_definition_version,
        agent_session_id: record.agent_session_id.clone(),
        run_id: record.run_id.clone(),
        workflow_run_id: record.workflow_run_id.clone(),
        workflow_step_id: record.workflow_step_id.clone(),
        title: record.title.clone(),
        summary: record.summary.clone(),
        text: record.text.clone(),
        text_hash: project_record_text_hash(&record.text),
        content_json: record
            .content_json
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| {
                CommandError::system_fault(
                    "project_record_content_serialize_failed",
                    format!("Xero could not serialize project record content: {error}"),
                )
            })?,
        content_hash,
        schema_name: record.schema_name.clone(),
        schema_version: record.schema_version,
        importance: project_record_importance_sql_value(&record.importance).into(),
        confidence: record.confidence,
        tags_json: json_array(&record.tags, "tags")?,
        source_item_ids_json: json_array(&record.source_item_ids, "sourceItemIds")?,
        related_paths_json: json_array(&record.related_paths, "relatedPaths")?,
        produced_artifact_refs_json: json_array(
            &record.produced_artifact_refs,
            "producedArtifactRefs",
        )?,
        redaction_state: project_record_redaction_state_sql_value(&record.redaction_state).into(),
        visibility: project_record_visibility_sql_value(&record.visibility).into(),
        created_at: record.created_at.clone(),
        updated_at: record.created_at.clone(),
        embedding: Some(embedding.vector),
        embedding_model: Some(embedding.model),
        embedding_dimension: Some(embedding.dimension),
        embedding_version: Some(embedding.version),
    };
    store.insert_dedup(row).and_then(row_into_record)
}

pub fn list_project_records(
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<ProjectRecordRecord>, CommandError> {
    let store = open_store_with_project_check(repo_root, project_id)?;
    store
        .list()?
        .into_iter()
        .map(row_into_record)
        .collect::<Result<Vec<_>, _>>()
}

fn open_store_with_project_check(
    repo_root: &Path,
    project_id: &str,
) -> Result<project_record_lance::ProjectRecordStore, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "project_record_project_id_required",
    )?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    Ok(project_record_lance::open_for_database_path(
        &database_path,
        project_id,
    ))
}

fn validate_new_project_record(record: &NewProjectRecordRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.record_id,
        "recordId",
        "project_record_record_id_required",
    )?;
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "project_record_project_id_required",
    )?;
    validate_non_empty_text(
        &record.agent_definition_id,
        "agentDefinitionId",
        "project_record_agent_definition_id_required",
    )?;
    if record.agent_definition_version == 0 {
        return Err(CommandError::invalid_request("agentDefinitionVersion"));
    }
    validate_non_empty_text(&record.run_id, "runId", "project_record_run_id_required")?;
    validate_non_empty_text(&record.title, "title", "project_record_title_required")?;
    validate_non_empty_text(
        &record.summary,
        "summary",
        "project_record_summary_required",
    )?;
    validate_non_empty_text(&record.text, "text", "project_record_text_required")?;
    if record.schema_version <= 0 {
        return Err(CommandError::invalid_request("schemaVersion"));
    }
    if record
        .confidence
        .is_some_and(|confidence| !(0.0..=1.0).contains(&confidence))
    {
        return Err(CommandError::invalid_request("confidence"));
    }
    Ok(())
}

fn project_record_embedding_text(record: &NewProjectRecordRecord) -> String {
    let mut text = format!("{}.\n{}\n{}", record.title, record.summary, record.text);
    if !record.tags.is_empty() {
        text.push_str("\nTags: ");
        text.push_str(&record.tags.join(", "));
    }
    if !record.related_paths.is_empty() {
        text.push_str("\nRelated paths: ");
        text.push_str(&record.related_paths.join(", "));
    }
    text
}

fn row_into_record(row: ProjectRecordRow) -> Result<ProjectRecordRecord, CommandError> {
    Ok(ProjectRecordRecord {
        record_id: row.record_id,
        project_id: row.project_id,
        record_kind: parse_project_record_kind(&row.record_kind),
        runtime_agent_id: row.runtime_agent_id,
        agent_definition_id: row.agent_definition_id,
        agent_definition_version: row.agent_definition_version,
        agent_session_id: row.agent_session_id,
        run_id: row.run_id,
        workflow_run_id: row.workflow_run_id,
        workflow_step_id: row.workflow_step_id,
        title: row.title,
        summary: row.summary,
        text: row.text,
        text_hash: row.text_hash,
        content_json: row
            .content_json
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(|error| {
                CommandError::system_fault(
                    "project_record_content_decode_failed",
                    format!("Xero could not decode project record content: {error}"),
                )
            })?,
        content_hash: row.content_hash,
        schema_name: row.schema_name,
        schema_version: row.schema_version,
        importance: parse_project_record_importance(&row.importance),
        confidence: row.confidence,
        tags: parse_json_array(&row.tags_json, "tags")?,
        source_item_ids: parse_json_array(&row.source_item_ids_json, "sourceItemIds")?,
        related_paths: parse_json_array(&row.related_paths_json, "relatedPaths")?,
        produced_artifact_refs: parse_json_array(
            &row.produced_artifact_refs_json,
            "producedArtifactRefs",
        )?,
        redaction_state: parse_project_record_redaction_state(&row.redaction_state),
        visibility: parse_project_record_visibility(&row.visibility),
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn json_array(values: &[String], field: &'static str) -> Result<String, CommandError> {
    serde_json::to_string(values).map_err(|error| {
        CommandError::system_fault(
            "project_record_array_serialize_failed",
            format!("Xero could not serialize project record {field}: {error}"),
        )
    })
}

fn parse_json_array(value: &str, field: &'static str) -> Result<Vec<String>, CommandError> {
    serde_json::from_str(value).map_err(|error| {
        CommandError::system_fault(
            "project_record_array_decode_failed",
            format!("Xero could not decode project record {field}: {error}"),
        )
    })
}

pub fn project_record_kind_sql_value(kind: &ProjectRecordKind) -> &'static str {
    match kind {
        ProjectRecordKind::AgentHandoff => "agent_handoff",
        ProjectRecordKind::ProjectFact => "project_fact",
        ProjectRecordKind::Decision => "decision",
        ProjectRecordKind::Constraint => "constraint",
        ProjectRecordKind::Plan => "plan",
        ProjectRecordKind::Finding => "finding",
        ProjectRecordKind::Verification => "verification",
        ProjectRecordKind::Question => "question",
        ProjectRecordKind::Artifact => "artifact",
        ProjectRecordKind::ContextNote => "context_note",
        ProjectRecordKind::Diagnostic => "diagnostic",
    }
}

fn parse_project_record_kind(value: &str) -> ProjectRecordKind {
    match value {
        "project_fact" => ProjectRecordKind::ProjectFact,
        "decision" => ProjectRecordKind::Decision,
        "constraint" => ProjectRecordKind::Constraint,
        "plan" => ProjectRecordKind::Plan,
        "finding" => ProjectRecordKind::Finding,
        "verification" => ProjectRecordKind::Verification,
        "question" => ProjectRecordKind::Question,
        "artifact" => ProjectRecordKind::Artifact,
        "context_note" => ProjectRecordKind::ContextNote,
        "diagnostic" => ProjectRecordKind::Diagnostic,
        _ => ProjectRecordKind::AgentHandoff,
    }
}

fn project_record_importance_sql_value(importance: &ProjectRecordImportance) -> &'static str {
    match importance {
        ProjectRecordImportance::Low => "low",
        ProjectRecordImportance::Normal => "normal",
        ProjectRecordImportance::High => "high",
        ProjectRecordImportance::Critical => "critical",
    }
}

fn parse_project_record_importance(value: &str) -> ProjectRecordImportance {
    match value {
        "low" => ProjectRecordImportance::Low,
        "high" => ProjectRecordImportance::High,
        "critical" => ProjectRecordImportance::Critical,
        _ => ProjectRecordImportance::Normal,
    }
}

fn project_record_redaction_state_sql_value(
    redaction_state: &ProjectRecordRedactionState,
) -> &'static str {
    match redaction_state {
        ProjectRecordRedactionState::Clean => "clean",
        ProjectRecordRedactionState::Redacted => "redacted",
        ProjectRecordRedactionState::Blocked => "blocked",
    }
}

fn parse_project_record_redaction_state(value: &str) -> ProjectRecordRedactionState {
    match value {
        "redacted" => ProjectRecordRedactionState::Redacted,
        "blocked" => ProjectRecordRedactionState::Blocked,
        _ => ProjectRecordRedactionState::Clean,
    }
}

fn project_record_visibility_sql_value(visibility: &ProjectRecordVisibility) -> &'static str {
    match visibility {
        ProjectRecordVisibility::Workflow => "workflow",
        ProjectRecordVisibility::Retrieval => "retrieval",
        ProjectRecordVisibility::MemoryCandidate => "memory_candidate",
        ProjectRecordVisibility::Diagnostic => "diagnostic",
    }
}

fn parse_project_record_visibility(value: &str) -> ProjectRecordVisibility {
    match value {
        "workflow" => ProjectRecordVisibility::Workflow,
        "memory_candidate" => ProjectRecordVisibility::MemoryCandidate,
        "diagnostic" => ProjectRecordVisibility::Diagnostic,
        _ => ProjectRecordVisibility::Retrieval,
    }
}

pub fn now_project_record_timestamp() -> String {
    now_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use rusqlite::{params, Connection};
    use serde_json::json;

    use crate::db::{configure_connection, database_path_for_repo, migrations::migrations};

    fn create_project_database(repo_root: &Path, project_id: &str) -> std::path::PathBuf {
        let database_path = repo_root
            .parent()
            .expect("repo parent")
            .join("app-data")
            .join("projects")
            .join(project_id)
            .join("state.db");
        fs::create_dir_all(database_path.parent().expect("database parent")).expect("database dir");
        let mut connection = Connection::open(&database_path).expect("open database");
        configure_connection(&connection).expect("configure database");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate database");
        connection
            .execute(
                "INSERT INTO projects (id, name, description, milestone) VALUES (?1, 'Project', '', '')",
                params![project_id],
            )
            .expect("insert project");
        connection
            .execute(
                r#"
                INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo)
                VALUES ('repo-1', ?1, ?2, 'Project', 'main', 'abc123', 1)
                "#,
                params![project_id, repo_root.to_string_lossy().as_ref()],
            )
            .expect("insert repository");
        crate::db::register_project_database_path(repo_root, &database_path);
        database_path
    }

    fn new_project_record(project_id: &str, record_id: &str, text: &str) -> NewProjectRecordRecord {
        NewProjectRecordRecord {
            record_id: record_id.into(),
            project_id: project_id.into(),
            record_kind: ProjectRecordKind::AgentHandoff,
            runtime_agent_id: RuntimeAgentIdDto::Ask,
            agent_definition_id: "ask".into(),
            agent_definition_version: crate::db::project_store::BUILTIN_AGENT_DEFINITION_VERSION,
            agent_session_id: Some("agent-session-1".into()),
            run_id: "run-1".into(),
            workflow_run_id: None,
            workflow_step_id: None,
            title: "Ask run handoff".into(),
            summary: "Ask answered a repository question.".into(),
            text: text.into(),
            content_json: Some(json!({
                "schema": "xero.project_record.run_handoff.v1",
                "facts": ["Ask inspected the project without mutating it."]
            })),
            schema_name: Some("xero.project_record.run_handoff.v1".into()),
            schema_version: 1,
            importance: ProjectRecordImportance::Normal,
            confidence: Some(1.0),
            tags: vec!["ask".into(), "handoff".into()],
            source_item_ids: vec!["message-1".into()],
            related_paths: vec!["src/main.rs".into()],
            produced_artifact_refs: Vec::new(),
            redaction_state: ProjectRecordRedactionState::Clean,
            visibility: ProjectRecordVisibility::Retrieval,
            created_at: "2026-05-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn project_records_store_under_app_data_lance_and_deduplicate() {
        project_record_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-ask-records";
        let database_path = create_project_database(&repo_root, project_id);
        let lance_dir = project_record_lance::dataset_dir_for_database_path(&database_path);

        assert_eq!(database_path_for_repo(&repo_root), database_path);
        assert!(lance_dir.starts_with(database_path.parent().expect("database parent")));
        assert!(!repo_root.join(".xero").exists());

        let inserted = insert_project_record(
            &repo_root,
            &new_project_record(
                project_id,
                "project-record-1",
                "Ask found that the app stores project records in LanceDB.",
            ),
        )
        .expect("insert record");
        let duplicate = insert_project_record(
            &repo_root,
            &new_project_record(
                project_id,
                "project-record-2",
                "Ask found that the app stores project records in LanceDB.",
            ),
        )
        .expect("dedupe record");
        let records = list_project_records(&repo_root, project_id).expect("list records");

        assert_eq!(duplicate.record_id, inserted.record_id);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].runtime_agent_id, RuntimeAgentIdDto::Ask);
        assert_eq!(records[0].record_kind, ProjectRecordKind::AgentHandoff);
        assert!(lance_dir.join("project_records.lance").exists());
        assert!(!repo_root.join(".xero").exists());
    }
}
