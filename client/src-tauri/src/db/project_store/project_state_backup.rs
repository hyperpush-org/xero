use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::json;

use crate::{
    commands::CommandError,
    db::{database_path_for_repo, project_app_data_dir_for_repo},
};

use super::{
    agent_memory_lance, open_runtime_database, project_record_lance,
    reconcile_agent_handoff_lineage, reconcile_cross_store_outbox,
    storage_observability::record_project_storage_maintenance_success, validate_non_empty_text,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStateBackupRecord {
    pub backup_id: String,
    pub backup_dir: PathBuf,
    pub database_path: PathBuf,
    pub lance_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub created_at: String,
    pub file_count: usize,
    pub byte_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStateRestoreRecord {
    pub backup_dir: PathBuf,
    pub restored_at: String,
    pub pre_restore_backup_dir: PathBuf,
    pub database_path: PathBuf,
    pub lance_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStateRepairReport {
    pub project_id: String,
    pub checked_at: String,
    pub database_path: PathBuf,
    pub lance_dir: PathBuf,
    pub sqlite_checkpointed: bool,
    pub outbox_inspected_count: usize,
    pub outbox_reconciled_count: usize,
    pub outbox_failed_count: usize,
    pub handoff_inspected_count: usize,
    pub handoff_repaired_count: usize,
    pub handoff_failed_count: usize,
    pub project_record_health_status: String,
    pub agent_memory_health_status: String,
    pub diagnostics: Vec<ProjectStateRepairDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStateRepairDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStateBackupListing {
    pub backups: Vec<ProjectStateBackupListingEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStateBackupListingEntry {
    pub backup_id: String,
    pub created_at: Option<String>,
    pub file_count: Option<usize>,
    pub byte_count: Option<u64>,
    pub manifest_present: bool,
    pub pre_restore: bool,
}

pub fn create_project_state_backup(
    repo_root: &Path,
    backup_id: &str,
    created_at: &str,
) -> Result<ProjectStateBackupRecord, CommandError> {
    validate_backup_id(backup_id)?;
    validate_non_empty_text(
        created_at,
        "createdAt",
        "project_state_backup_created_at_required",
    )?;

    let database_path = database_path_for_repo(repo_root);
    let app_data_dir = project_app_data_dir_for_repo(repo_root);
    let lance_dir = project_record_lance::dataset_dir_for_database_path(&database_path);
    let backup_dir = app_data_dir.join("backups").join(backup_id);
    if !database_path.exists() {
        return Err(CommandError::retryable(
            "project_state_backup_database_missing",
            format!(
                "Xero could not back up project state because {} does not exist.",
                database_path.display()
            ),
        ));
    }
    if backup_dir.exists() {
        return Err(CommandError::user_fixable(
            "project_state_backup_exists",
            format!(
                "Project-state backup `{}` already exists at {}.",
                backup_id,
                backup_dir.display()
            ),
        ));
    }

    fs::create_dir_all(&backup_dir).map_err(|error| {
        CommandError::retryable(
            "project_state_backup_dir_failed",
            format!(
                "Xero could not create project-state backup directory {}: {error}",
                backup_dir.display()
            ),
        )
    })?;

    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| {
            CommandError::retryable(
                "project_state_backup_checkpoint_failed",
                format!(
                    "Xero could not checkpoint project state before backup at {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    drop(connection);
    copy_file_if_exists(&database_path, &backup_dir.join("state.db"))?;
    copy_file_if_exists(
        &database_path.with_extension("db-wal"),
        &backup_dir.join("state.db-wal"),
    )?;
    copy_file_if_exists(
        &database_path.with_extension("db-shm"),
        &backup_dir.join("state.db-shm"),
    )?;
    if lance_dir.exists() {
        copy_dir_recursive(&lance_dir, &backup_dir.join("lance"))?;
    }

    let (file_count, byte_count) = directory_metrics(&backup_dir)?;
    let manifest_path = backup_dir.join("manifest.json");
    let manifest = json!({
        "schema": "xero.project_state_backup.v1",
        "backupId": backup_id,
        "createdAt": created_at,
        "databasePath": &database_path,
        "lanceDir": &lance_dir,
        "fileCount": file_count,
        "byteCount": byte_count,
    });
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|error| {
            CommandError::system_fault(
                "project_state_backup_manifest_encode_failed",
                format!("Xero could not encode project-state backup manifest: {error}"),
            )
        })?,
    )
    .map_err(|error| {
        CommandError::retryable(
            "project_state_backup_manifest_write_failed",
            format!(
                "Xero could not write project-state backup manifest at {}: {error}",
                manifest_path.display()
            ),
        )
    })?;

    Ok(ProjectStateBackupRecord {
        backup_id: backup_id.to_string(),
        backup_dir,
        database_path,
        lance_dir,
        manifest_path,
        created_at: created_at.to_string(),
        file_count,
        byte_count,
    })
}

pub fn restore_project_state_backup(
    repo_root: &Path,
    backup_dir: &Path,
    restored_at: &str,
) -> Result<ProjectStateRestoreRecord, CommandError> {
    validate_non_empty_text(
        restored_at,
        "restoredAt",
        "project_state_restore_restored_at_required",
    )?;
    let backup_database = backup_dir.join("state.db");
    if !backup_database.exists() {
        return Err(CommandError::user_fixable(
            "project_state_backup_missing_database",
            format!(
                "Project-state backup at {} does not contain state.db.",
                backup_dir.display()
            ),
        ));
    }

    let database_path = database_path_for_repo(repo_root);
    let app_data_dir = project_app_data_dir_for_repo(repo_root);
    let lance_dir = project_record_lance::dataset_dir_for_database_path(&database_path);
    let pre_restore_id = format!("pre-restore-{}", sanitize_backup_id(restored_at));
    let pre_restore_backup_dir =
        next_available_backup_dir(&app_data_dir.join("backups").join(pre_restore_id));
    snapshot_project_state_files(&database_path, &lance_dir, &pre_restore_backup_dir)?;

    remove_file_if_exists(&database_path)?;
    remove_file_if_exists(&database_path.with_extension("db-wal"))?;
    remove_file_if_exists(&database_path.with_extension("db-shm"))?;
    copy_file_if_exists(&backup_database, &database_path)?;
    copy_file_if_exists(
        &backup_dir.join("state.db-wal"),
        &database_path.with_extension("db-wal"),
    )?;
    copy_file_if_exists(
        &backup_dir.join("state.db-shm"),
        &database_path.with_extension("db-shm"),
    )?;

    if lance_dir.exists() {
        fs::remove_dir_all(&lance_dir).map_err(|error| {
            CommandError::retryable(
                "project_state_restore_lance_remove_failed",
                format!(
                    "Xero could not remove current Lance state at {} during restore: {error}",
                    lance_dir.display()
                ),
            )
        })?;
    }
    let backup_lance = backup_dir.join("lance");
    if backup_lance.exists() {
        copy_dir_recursive(&backup_lance, &lance_dir)?;
    }
    project_record_lance::clear_connection_cache_for_database_path(&database_path);
    agent_memory_lance::clear_connection_cache_for_database_path(&database_path);

    Ok(ProjectStateRestoreRecord {
        backup_dir: backup_dir.to_path_buf(),
        restored_at: restored_at.to_string(),
        pre_restore_backup_dir,
        database_path,
        lance_dir,
    })
}

pub fn repair_project_state(
    repo_root: &Path,
    project_id: &str,
    checked_at: &str,
) -> Result<ProjectStateRepairReport, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "project_state_repair_project_required",
    )?;
    validate_non_empty_text(
        checked_at,
        "checkedAt",
        "project_state_repair_checked_at_required",
    )?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| {
            CommandError::retryable(
                "project_state_repair_checkpoint_failed",
                format!(
                    "Xero could not checkpoint project state at {} during repair: {error}",
                    database_path.display()
                ),
            )
        })?;
    drop(connection);

    let outbox = reconcile_cross_store_outbox(repo_root, project_id, checked_at)?;
    let handoffs = reconcile_agent_handoff_lineage(repo_root, project_id, checked_at)?;
    let record_store = project_record_lance::open_for_database_path(&database_path, project_id);
    let memory_store = agent_memory_lance::open_for_database_path(&database_path, project_id);
    let mut project_record_health = record_store.health_report()?;
    if project_record_health.maintenance_recommended {
        project_record_health = record_store.optimize_for_maintenance()?.after;
    }
    let mut agent_memory_health = memory_store.health_report()?;
    if agent_memory_health.maintenance_recommended {
        agent_memory_health = memory_store.optimize_for_maintenance()?.after;
    }
    let lance_dir = project_record_lance::dataset_dir_for_database_path(&database_path);

    let mut diagnostics = Vec::new();
    if outbox.failed_count > 0 {
        diagnostics.push(ProjectStateRepairDiagnostic {
            code: "project_state_repair_outbox_failed".into(),
            message: format!(
                "{} cross-store outbox operation(s) still need manual repair.",
                outbox.failed_count
            ),
            severity: "error".into(),
        });
    }
    if handoffs.failed_count > 0 {
        diagnostics.push(ProjectStateRepairDiagnostic {
            code: "project_state_repair_handoff_failed".into(),
            message: format!(
                "{} handoff lineage record(s) still need manual repair.",
                handoffs.failed_count
            ),
            severity: "error".into(),
        });
    }
    for health in [
        ("project_records", &project_record_health.status),
        ("agent_memories", &agent_memory_health.status),
    ] {
        if health.1 != "healthy" {
            diagnostics.push(ProjectStateRepairDiagnostic {
                code: "project_state_repair_lance_degraded".into(),
                message: format!("Lance table `{}` reported `{}`.", health.0, health.1),
                severity: "warning".into(),
            });
        }
    }
    record_project_storage_maintenance_success(
        repo_root,
        project_id,
        "repair",
        checked_at,
        Some(json!({
            "outboxInspectedCount": outbox.inspected_count,
            "outboxReconciledCount": outbox.reconciled_count,
            "outboxFailedCount": outbox.failed_count,
            "handoffInspectedCount": handoffs.inspected_count,
            "handoffRepairedCount": handoffs.repaired_count,
            "handoffFailedCount": handoffs.failed_count,
            "projectRecordHealthStatus": &project_record_health.status,
            "agentMemoryHealthStatus": &agent_memory_health.status,
        })),
    )?;

    Ok(ProjectStateRepairReport {
        project_id: project_id.to_string(),
        checked_at: checked_at.to_string(),
        database_path,
        lance_dir,
        sqlite_checkpointed: true,
        outbox_inspected_count: outbox.inspected_count,
        outbox_reconciled_count: outbox.reconciled_count,
        outbox_failed_count: outbox.failed_count,
        handoff_inspected_count: handoffs.inspected_count,
        handoff_repaired_count: handoffs.repaired_count,
        handoff_failed_count: handoffs.failed_count,
        project_record_health_status: project_record_health.status,
        agent_memory_health_status: agent_memory_health.status,
        diagnostics,
    })
}

pub fn list_project_state_backups(
    repo_root: &Path,
) -> Result<ProjectStateBackupListing, CommandError> {
    let backups_root = project_app_data_dir_for_repo(repo_root).join("backups");
    if !backups_root.exists() {
        return Ok(ProjectStateBackupListing {
            backups: Vec::new(),
        });
    }

    let mut entries = Vec::new();
    let read = fs::read_dir(&backups_root).map_err(|error| {
        CommandError::retryable(
            "project_state_backup_list_failed",
            format!(
                "Xero could not list project-state backups in {}: {error}",
                backups_root.display()
            ),
        )
    })?;
    for entry in read {
        let entry = entry.map_err(|error| {
            CommandError::retryable(
                "project_state_backup_list_entry_failed",
                format!(
                    "Xero could not read a backup entry in {}: {error}",
                    backups_root.display()
                ),
            )
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let backup_id = match entry.file_name().to_str() {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => continue,
        };

        let manifest_path = path.join("manifest.json");
        let mut created_at: Option<String> = None;
        let mut file_count: Option<usize> = None;
        let mut byte_count: Option<u64> = None;
        let manifest_present = manifest_path.exists();
        if manifest_present {
            if let Ok(text) = fs::read_to_string(&manifest_path) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    created_at = value
                        .get("createdAt")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    file_count = value
                        .get("fileCount")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);
                    byte_count = value.get("byteCount").and_then(|v| v.as_u64());
                }
            }
        }

        entries.push(ProjectStateBackupListingEntry {
            pre_restore: backup_id.starts_with("pre-restore-"),
            backup_id,
            created_at,
            file_count,
            byte_count,
            manifest_present,
        });
    }

    entries.sort_by(|a, b| match (&b.created_at, &a.created_at) {
        (Some(left), Some(right)) => left.cmp(right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.backup_id.cmp(&a.backup_id),
    });

    Ok(ProjectStateBackupListing { backups: entries })
}

fn snapshot_project_state_files(
    database_path: &Path,
    lance_dir: &Path,
    backup_dir: &Path,
) -> Result<(), CommandError> {
    fs::create_dir_all(backup_dir).map_err(|error| {
        CommandError::retryable(
            "project_state_pre_restore_backup_dir_failed",
            format!(
                "Xero could not create pre-restore backup directory {}: {error}",
                backup_dir.display()
            ),
        )
    })?;
    copy_file_if_exists(database_path, &backup_dir.join("state.db"))?;
    copy_file_if_exists(
        &database_path.with_extension("db-wal"),
        &backup_dir.join("state.db-wal"),
    )?;
    copy_file_if_exists(
        &database_path.with_extension("db-shm"),
        &backup_dir.join("state.db-shm"),
    )?;
    if lance_dir.exists() {
        copy_dir_recursive(lance_dir, &backup_dir.join("lance"))?;
    }
    Ok(())
}

fn copy_file_if_exists(source: &Path, target: &Path) -> Result<(), CommandError> {
    if !source.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "project_state_backup_parent_failed",
                format!(
                    "Xero could not create backup parent directory {}: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    fs::copy(source, target).map_err(|error| {
        CommandError::retryable(
            "project_state_file_copy_failed",
            format!(
                "Xero could not copy project-state file from {} to {}: {error}",
                source.display(),
                target.display()
            ),
        )
    })?;
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), CommandError> {
    fs::create_dir_all(target).map_err(|error| {
        CommandError::retryable(
            "project_state_dir_copy_failed",
            format!(
                "Xero could not create project-state directory {}: {error}",
                target.display()
            ),
        )
    })?;
    for entry in fs::read_dir(source).map_err(|error| {
        CommandError::retryable(
            "project_state_dir_read_failed",
            format!(
                "Xero could not read project-state directory {}: {error}",
                source.display()
            ),
        )
    })? {
        let entry = entry.map_err(|error| {
            CommandError::retryable(
                "project_state_dir_entry_failed",
                format!(
                    "Xero could not read an entry in project-state directory {}: {error}",
                    source.display()
                ),
            )
        })?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            copy_file_if_exists(&source_path, &target_path)?;
        }
    }
    Ok(())
}

fn directory_metrics(path: &Path) -> Result<(usize, u64), CommandError> {
    if !path.exists() {
        return Ok((0, 0));
    }
    let mut file_count = 0;
    let mut byte_count = 0;
    for entry in fs::read_dir(path).map_err(|error| {
        CommandError::retryable(
            "project_state_backup_metric_read_failed",
            format!(
                "Xero could not inspect project-state backup directory {}: {error}",
                path.display()
            ),
        )
    })? {
        let entry = entry.map_err(|error| {
            CommandError::retryable(
                "project_state_backup_metric_entry_failed",
                format!(
                    "Xero could not inspect a project-state backup entry in {}: {error}",
                    path.display()
                ),
            )
        })?;
        let metadata = entry.metadata().map_err(|error| {
            CommandError::retryable(
                "project_state_backup_metric_metadata_failed",
                format!(
                    "Xero could not inspect project-state backup entry {}: {error}",
                    entry.path().display()
                ),
            )
        })?;
        if metadata.is_dir() {
            let (nested_files, nested_bytes) = directory_metrics(&entry.path())?;
            file_count += nested_files;
            byte_count += nested_bytes;
        } else {
            file_count += 1;
            byte_count += metadata.len();
        }
    }
    Ok((file_count, byte_count))
}

fn remove_file_if_exists(path: &Path) -> Result<(), CommandError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CommandError::retryable(
            "project_state_restore_remove_failed",
            format!(
                "Xero could not remove project-state file {} during restore: {error}",
                path.display()
            ),
        )),
    }
}

fn next_available_backup_dir(base: &Path) -> PathBuf {
    if !base.exists() {
        return base.to_path_buf();
    }
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let name = base
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("backup");
    let mut attempt = 1;
    loop {
        let candidate = parent.join(format!("{name}.{attempt}"));
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
    }
}

fn validate_backup_id(backup_id: &str) -> Result<(), CommandError> {
    validate_non_empty_text(backup_id, "backupId", "project_state_backup_id_required")?;
    if backup_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        Ok(())
    } else {
        Err(CommandError::invalid_request("backupId"))
    }
}

fn sanitize_backup_id(value: &str) -> String {
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
    sanitized.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    use rusqlite::{params, Connection};

    use crate::{
        commands::RuntimeAgentIdDto,
        db::{
            configure_connection,
            migrations::migrations,
            project_store::{
                insert_agent_definition, insert_agent_memory, insert_project_record,
                list_agent_memories, list_project_records, load_agent_definition, AgentMemoryKind,
                AgentMemoryListFilter, AgentMemoryReviewState, AgentMemoryScope,
                NewAgentDefinitionRecord, NewAgentMemoryRecord, NewProjectRecordRecord,
                ProjectRecordImportance, ProjectRecordKind, ProjectRecordRedactionState,
                ProjectRecordVisibility, BUILTIN_AGENT_DEFINITION_VERSION,
            },
            register_project_database_path,
        },
    };

    fn create_project_database(repo_root: &Path, project_id: &str) -> PathBuf {
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
        register_project_database_path(repo_root, &database_path);
        database_path
    }

    fn backup_project_record(
        project_id: &str,
        record_id: &str,
        text: &str,
        created_at: &str,
    ) -> NewProjectRecordRecord {
        NewProjectRecordRecord {
            record_id: record_id.into(),
            project_id: project_id.into(),
            record_kind: ProjectRecordKind::ProjectFact,
            runtime_agent_id: RuntimeAgentIdDto::Ask,
            agent_definition_id: "ask".into(),
            agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
            agent_session_id: None,
            run_id: "run-backup-state".into(),
            workflow_run_id: None,
            workflow_step_id: None,
            title: format!("Backup fact {record_id}"),
            summary: "Project-state backup fixture fact.".into(),
            text: text.into(),
            content_json: Some(json!({
                "schema": "xero.project_state_backup_fixture.v1",
                "recordId": record_id,
            })),
            schema_name: Some("xero.project_state_backup_fixture.v1".into()),
            schema_version: 1,
            importance: ProjectRecordImportance::Normal,
            confidence: Some(1.0),
            tags: vec!["backup".into()],
            source_item_ids: Vec::new(),
            related_paths: Vec::new(),
            produced_artifact_refs: Vec::new(),
            redaction_state: ProjectRecordRedactionState::Clean,
            visibility: ProjectRecordVisibility::Retrieval,
            created_at: created_at.into(),
        }
    }

    fn backup_agent_memory(
        project_id: &str,
        memory_id: &str,
        text: &str,
        created_at: &str,
    ) -> NewAgentMemoryRecord {
        NewAgentMemoryRecord {
            memory_id: memory_id.into(),
            project_id: project_id.into(),
            agent_session_id: None,
            scope: AgentMemoryScope::Project,
            kind: AgentMemoryKind::ProjectFact,
            text: text.into(),
            review_state: AgentMemoryReviewState::Approved,
            enabled: true,
            confidence: Some(90),
            source_run_id: None,
            source_item_ids: Vec::new(),
            diagnostic: None,
            created_at: created_at.into(),
        }
    }

    fn backup_agent_definition(
        definition_id: &str,
        display_name: &str,
        created_at: &str,
    ) -> NewAgentDefinitionRecord {
        NewAgentDefinitionRecord {
            definition_id: definition_id.into(),
            version: 1,
            display_name: display_name.into(),
            short_label: "Backup".into(),
            description: "Project-state backup fixture custom agent.".into(),
            scope: "project_custom".into(),
            lifecycle_state: "active".into(),
            base_capability_profile: "observe_only".into(),
            snapshot: json!({
                "id": definition_id,
                "version": 1,
                "schema": "xero.agent_definition.v1",
                "schemaVersion": 3,
                "scope": "project_custom",
                "lifecycleState": "active",
                "baseCapabilityProfile": "observe_only",
                "label": display_name,
                "shortLabel": "Backup",
                "attachedSkills": []
            }),
            validation_report: Some(json!({
                "status": "valid",
                "source": "s43_project_state_backup_test"
            })),
            created_at: created_at.into(),
            updated_at: created_at.into(),
        }
    }

    #[test]
    fn s43_project_state_backup_restore_and_repair_use_app_data() {
        project_record_lance::reset_connection_cache_for_tests();
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-state-backup";
        let database_path = create_project_database(&repo_root, project_id);
        insert_project_record(
            &repo_root,
            &backup_project_record(
                project_id,
                "project-record-before-backup",
                "Record that should survive restore.",
                "2026-05-09T00:00:00Z",
            ),
        )
        .expect("insert pre-backup project record");
        insert_agent_memory(
            &repo_root,
            &backup_agent_memory(
                project_id,
                "memory-before-backup",
                "Memory that should survive restore.",
                "2026-05-09T00:00:00Z",
            ),
        )
        .expect("insert pre-backup memory");
        insert_agent_definition(
            &repo_root,
            &backup_agent_definition(
                "backup_agent_before",
                "Backup Agent Before",
                "2026-05-09T00:00:00Z",
            ),
        )
        .expect("insert pre-backup agent definition");

        let backup = create_project_state_backup(
            &repo_root,
            "backup-2026-05-09T00.00.00Z",
            "2026-05-09T00:00:00Z",
        )
        .expect("create backup");
        assert!(backup.backup_dir.starts_with(
            database_path
                .parent()
                .expect("database parent")
                .join("backups")
        ));
        assert!(backup.manifest_path.exists());
        assert!(backup.backup_dir.join("state.db").exists());
        assert!(backup.backup_dir.join("lance").exists());
        let backup_manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&backup.manifest_path).expect("read manifest"))
                .expect("decode manifest");
        assert_eq!(
            backup_manifest["schema"],
            json!("xero.project_state_backup.v1")
        );
        assert!(!repo_root.join(".xero").exists());

        {
            let connection = Connection::open(&database_path).expect("open database");
            connection
                .execute(
                    "UPDATE projects SET name = 'Changed' WHERE id = ?1",
                    params![project_id],
                )
                .expect("mutate project");
        }
        insert_project_record(
            &repo_root,
            &backup_project_record(
                project_id,
                "project-record-after-backup",
                "Record that should be removed by restore.",
                "2026-05-09T00:00:30Z",
            ),
        )
        .expect("insert post-backup project record");
        insert_agent_memory(
            &repo_root,
            &backup_agent_memory(
                project_id,
                "memory-after-backup",
                "Memory that should be removed by restore.",
                "2026-05-09T00:00:30Z",
            ),
        )
        .expect("insert post-backup memory");
        insert_agent_definition(
            &repo_root,
            &backup_agent_definition(
                "backup_agent_after",
                "Backup Agent After",
                "2026-05-09T00:00:30Z",
            ),
        )
        .expect("insert post-backup agent definition");

        let restore =
            restore_project_state_backup(&repo_root, &backup.backup_dir, "2026-05-09T00:01:00Z")
                .expect("restore backup");
        assert!(restore.pre_restore_backup_dir.exists());
        let restored_name: String = Connection::open(&database_path)
            .expect("open restored database")
            .query_row(
                "SELECT name FROM projects WHERE id = ?1",
                params![project_id],
                |row| row.get(0),
            )
            .expect("read restored project");
        assert_eq!(restored_name, "Project");
        let restored_records =
            list_project_records(&repo_root, project_id).expect("list restored project records");
        assert!(restored_records
            .iter()
            .any(|record| record.record_id == "project-record-before-backup"));
        assert!(!restored_records
            .iter()
            .any(|record| record.record_id == "project-record-after-backup"));
        let restored_memories = list_agent_memories(
            &repo_root,
            project_id,
            AgentMemoryListFilter {
                include_disabled: true,
                include_rejected: true,
                ..AgentMemoryListFilter::default()
            },
        )
        .expect("list restored memories");
        assert!(restored_memories
            .iter()
            .any(|memory| memory.memory_id == "memory-before-backup"));
        assert!(!restored_memories
            .iter()
            .any(|memory| memory.memory_id == "memory-after-backup"));
        assert!(load_agent_definition(&repo_root, "backup_agent_before")
            .expect("load restored definition")
            .is_some());
        assert!(load_agent_definition(&repo_root, "backup_agent_after")
            .expect("load post-backup definition")
            .is_none());

        let repair = repair_project_state(&repo_root, project_id, "2026-05-09T00:02:00Z")
            .expect("repair project state");
        assert!(repair.sqlite_checkpointed);
        assert_eq!(repair.outbox_inspected_count, 0);
        assert_eq!(repair.project_record_health_status, "healthy");
        assert_eq!(repair.agent_memory_health_status, "healthy");
        assert!(repair.diagnostics.is_empty());
        assert!(repair
            .database_path
            .starts_with(repo_root.parent().expect("repo parent").join("app-data")));
        assert!(!repo_root.join(".xero").exists());
    }

    #[test]
    fn s43_project_state_repair_compacts_lance_maintenance_before_reporting_health() {
        project_record_lance::reset_connection_cache_for_tests();
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-state-fragmented-repair";
        let database_path = create_project_database(&repo_root, project_id);

        for index in 0..32 {
            insert_project_record(
                &repo_root,
                &backup_project_record(
                    project_id,
                    &format!("project-record-fragment-{index:02}"),
                    &format!("Fragmented record {index} that repair should compact."),
                    &format!("2026-05-09T00:00:{index:02}Z"),
                ),
            )
            .expect("insert fragmented project record");
        }

        let record_store = project_record_lance::open_for_database_path(&database_path, project_id);
        let before = record_store
            .health_report()
            .expect("project-record health before repair");
        assert!(
            before.maintenance_recommended,
            "expected fragmented Lance table to request maintenance, got {before:?}"
        );
        assert_eq!(before.status, "degraded");

        let repair = repair_project_state(&repo_root, project_id, "2026-05-09T00:02:00Z")
            .expect("repair fragmented project state");
        assert_eq!(repair.project_record_health_status, "healthy");
        assert_eq!(repair.agent_memory_health_status, "healthy");
        assert!(repair
            .diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.code != "project_state_repair_lance_degraded" }));

        let after = record_store
            .health_report()
            .expect("project-record health after repair");
        assert!(!after.maintenance_recommended);
        assert_eq!(after.status, "healthy");
    }
}
