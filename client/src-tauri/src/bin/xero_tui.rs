use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::Arc,
};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value as JsonValue};
use xero_cli::{CliError, TuiCommandAdapter};
use xero_desktop_lib::{
    auth::now_timestamp,
    commands::{project_runner, solana, CommandError, CommandErrorClass, RuntimeAgentIdDto},
    db::{self, project_store},
    environment::service as environment_service,
    global_db::GLOBAL_DATABASE_FILE_NAME,
    registry,
    runtime::{
        discover_local_skill_directory, discover_plugin_roots, discover_project_skill_directory,
        load_skill_source_settings_from_path, persist_skill_source_settings,
        AutonomousAgentDefinitionAction, AutonomousAgentDefinitionRequest, AutonomousToolOutput,
        AutonomousToolRuntime, XeroPluginRoot,
    },
};

struct DesktopProjectStoreTuiAdapter;

impl TuiCommandAdapter for DesktopProjectStoreTuiAdapter {
    fn invoke_json(
        &self,
        state_dir: &Path,
        args: &[String],
    ) -> Option<Result<JsonValue, CliError>> {
        match args.first().map(String::as_str) {
            Some("project-record") | Some("project-records") => {
                Some(handle_project_record_command(state_dir, &args[1..]).map_err(cli_error))
            }
            Some("memory") | Some("memories") => {
                Some(handle_memory_command(state_dir, &args[1..]).map_err(cli_error))
            }
            Some("agent-definition") | Some("agent-definitions") => {
                Some(handle_agent_definition_command(state_dir, &args[1..]).map_err(cli_error))
            }
            Some("code-history") | Some("code") => {
                Some(handle_code_history_command(state_dir, &args[1..]).map_err(cli_error))
            }
            Some("conversation") if args.get(1).map(String::as_str) == Some("rewind") => {
                Some(handle_conversation_rewind(state_dir, &args[2..]).map_err(cli_error))
            }
            Some("terminal") | Some("pty") => {
                Some(handle_terminal_command(state_dir, &args[1..]).map_err(cli_error))
            }
            Some("skill-sources") | Some("skill-source") => {
                Some(handle_skill_sources_command(state_dir, &args[1..]).map_err(cli_error))
            }
            Some("plugin-sources") | Some("plugin-source") => {
                Some(handle_plugin_sources_command(state_dir, &args[1..]).map_err(cli_error))
            }
            Some("solana") => Some(handle_solana_command(state_dir, &args[1..]).map_err(cli_error)),
            Some("environment")
                if matches!(
                    args.get(1).map(String::as_str),
                    Some(
                        "start"
                            | "refresh"
                            | "resolve-permissions"
                            | "verify-tool"
                            | "save-tool-verified"
                            | "remove-tool-verified"
                    )
                ) =>
            {
                Some(handle_environment_service_command(state_dir, &args[1..]).map_err(cli_error))
            }
            _ => None,
        }
    }
}

fn main() {
    process::exit(xero_cli::run_tui_from_env_with_adapter(Arc::new(
        DesktopProjectStoreTuiAdapter,
    )));
}

fn handle_project_record_command(
    state_dir: &Path,
    args: &[String],
) -> Result<JsonValue, CommandError> {
    let command = args.first().map(String::as_str).unwrap_or("list");
    match command {
        "list" | "records" => {
            let project_id = required_option(&args[1..], "--project-id", "projectId")?;
            configure_project_store_paths(state_dir);
            let repo_root = project_root(state_dir, &project_id)?;
            let mut records = project_store::list_project_records(&repo_root, &project_id)?
                .into_iter()
                .map(project_record_summary_json)
                .collect::<Vec<_>>();
            records.sort_by(|left, right| {
                string_field(right, "updatedAt").cmp(&string_field(left, "updatedAt"))
            });
            Ok(json!({
                "kind": "projectRecordList",
                "schema": "xero.project_context_record_list_command.v1",
                "projectId": project_id,
                "records": records,
                "backend": "desktop_project_store",
                "uiDeferred": false
            }))
        }
        "delete" | "remove" => {
            let project_id = required_option(&args[1..], "--project-id", "projectId")?;
            let record_id = positional_or_option(&args[1..], "--record-id", "recordId")?;
            configure_project_store_paths(state_dir);
            let repo_root = project_root(state_dir, &project_id)?;
            project_store::delete_project_record(&repo_root, &project_id, &record_id)?;
            Ok(json!({
                "kind": "projectRecordDelete",
                "schema": "xero.project_context_record_delete_command.v1",
                "projectId": project_id,
                "recordId": record_id,
                "retrievalRemoved": true,
                "backend": "desktop_project_store",
                "uiDeferred": false
            }))
        }
        "supersede" => {
            let project_id = required_option(&args[1..], "--project-id", "projectId")?;
            let superseded = required_option(
                &args[1..],
                "--superseded-record-id",
                "supersededRecordId",
            )?;
            let superseding = required_option(
                &args[1..],
                "--superseding-record-id",
                "supersedingRecordId",
            )?;
            configure_project_store_paths(state_dir);
            let repo_root = project_root(state_dir, &project_id)?;
            project_store::mark_project_record_superseded_by(
                &repo_root,
                &project_id,
                &superseded,
                &superseding,
                &now_timestamp(),
            )?;
            Ok(json!({
                "kind": "projectRecordSupersede",
                "schema": "xero.project_context_record_supersede_command.v1",
                "projectId": project_id,
                "supersededRecordId": superseded,
                "supersedingRecordId": superseding,
                "retrievalChanged": true,
                "backend": "desktop_project_store",
                "uiDeferred": false
            }))
        }
        _ => Err(CommandError::user_fixable(
            "xero_tui_project_record_command_unknown",
            format!(
                "Unknown desktop-backed project-record command `{command}`. Use list, delete, or supersede."
            ),
        )),
    }
}

fn handle_memory_command(state_dir: &Path, args: &[String]) -> Result<JsonValue, CommandError> {
    let command = args.first().map(String::as_str).unwrap_or("review-queue");
    match command {
        "review-queue" | "queue" | "list" => {
            let project_id = required_option(&args[1..], "--project-id", "projectId")?;
            let agent_session_id = option_value(&args[1..], "--session-id")
                .or_else(|| option_value(&args[1..], "--agent-session-id"));
            let limit = option_value(&args[1..], "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(50);
            configure_project_store_paths(state_dir);
            let repo_root = project_root(state_dir, &project_id)?;
            let queue = project_store::load_agent_memory_review_queue(
                &repo_root,
                &project_id,
                agent_session_id.as_deref(),
                limit,
            )?;
            Ok(json!({
                "kind": "memoryReviewQueue",
                "projectId": project_id,
                "queue": queue,
                "backend": "desktop_project_store",
                "uiDeferred": false
            }))
        }
        "approve" | "reject" | "disable" => {
            let project_id = required_option(&args[1..], "--project-id", "projectId")?;
            let memory_id = positional_or_option(&args[1..], "--memory-id", "memoryId")?;
            configure_project_store_paths(state_dir);
            let repo_root = project_root(state_dir, &project_id)?;
            let review_state = match command {
                "approve" => Some(project_store::AgentMemoryReviewState::Approved),
                "reject" => Some(project_store::AgentMemoryReviewState::Rejected),
                _ => None,
            };
            let enabled = match command {
                "approve" => Some(true),
                "disable" | "reject" => Some(false),
                _ => None,
            };
            let updated = project_store::update_agent_memory(
                &repo_root,
                &project_store::AgentMemoryUpdateRecord {
                    project_id: project_id.clone(),
                    memory_id: memory_id.clone(),
                    review_state,
                    enabled,
                    diagnostic: None,
                },
            )?;
            Ok(json!({
                "kind": "memoryReviewMutation",
                "projectId": project_id,
                "memoryId": memory_id,
                "memory": memory_summary_json(updated),
                "backend": "desktop_project_store",
                "uiDeferred": false
            }))
        }
        "delete" | "remove" => {
            let project_id = required_option(&args[1..], "--project-id", "projectId")?;
            let memory_id = positional_or_option(&args[1..], "--memory-id", "memoryId")?;
            configure_project_store_paths(state_dir);
            let repo_root = project_root(state_dir, &project_id)?;
            project_store::delete_agent_memory(&repo_root, &project_id, &memory_id)?;
            Ok(json!({
                "kind": "memoryReviewDelete",
                "projectId": project_id,
                "memoryId": memory_id,
                "backend": "desktop_project_store",
                "uiDeferred": false
            }))
        }
        _ => Err(CommandError::user_fixable(
            "xero_tui_memory_command_unknown",
            format!(
                "Unknown desktop-backed memory command `{command}`. Use review-queue, approve, reject, disable, or delete."
            ),
        )),
    }
}

fn handle_agent_definition_command(
    state_dir: &Path,
    args: &[String],
) -> Result<JsonValue, CommandError> {
    let command = args.first().map(String::as_str).unwrap_or("list");
    match command {
        "list" | "browse" => run_agent_definition_runtime(
            state_dir,
            &args[1..],
            AutonomousAgentDefinitionAction::List,
            AgentDefinitionCommandOptions {
                include_archived: option_present(&args[1..], "--include-archived"),
                ..AgentDefinitionCommandOptions::from_args(&args[1..])?
            },
        ),
        "draft" => run_agent_definition_runtime(
            state_dir,
            &args[1..],
            AutonomousAgentDefinitionAction::Draft,
            AgentDefinitionCommandOptions::from_args(&args[1..])?,
        ),
        "validate" => run_agent_definition_runtime(
            state_dir,
            &args[1..],
            AutonomousAgentDefinitionAction::Validate,
            AgentDefinitionCommandOptions::from_args(&args[1..])?,
        ),
        "preview" => run_agent_definition_runtime(
            state_dir,
            &args[1..],
            AutonomousAgentDefinitionAction::Preview,
            AgentDefinitionCommandOptions::from_args(&args[1..])?,
        ),
        "save" => run_agent_definition_runtime(
            state_dir,
            &args[1..],
            AutonomousAgentDefinitionAction::Save,
            AgentDefinitionCommandOptions::from_args(&args[1..])?,
        ),
        "update" => {
            let mut options = AgentDefinitionCommandOptions::from_args(&args[1..])?;
            if options.definition_id.is_none() {
                options.definition_id = first_positional(&args[1..]);
            }
            run_agent_definition_runtime(
                state_dir,
                &args[1..],
                AutonomousAgentDefinitionAction::Update,
                options,
            )
        }
        "clone" | "duplicate" => run_agent_definition_runtime(
            state_dir,
            &args[1..],
            AutonomousAgentDefinitionAction::Clone,
            AgentDefinitionCommandOptions::from_args(&args[1..])?,
        ),
        "archive" => {
            let mut options = AgentDefinitionCommandOptions::from_args(&args[1..])?;
            if options.definition_id.is_none() {
                options.definition_id = first_positional(&args[1..]);
            }
            run_agent_definition_runtime(
                state_dir,
                &args[1..],
                AutonomousAgentDefinitionAction::Archive,
                options,
            )
        }
        "attachable-skills" | "skills" => run_agent_definition_runtime(
            state_dir,
            &args[1..],
            AutonomousAgentDefinitionAction::ListAttachableSkills,
            AgentDefinitionCommandOptions::from_args(&args[1..])?,
        ),
        _ => Err(CommandError::user_fixable(
            "xero_tui_agent_definition_command_unknown",
            format!(
                "Unknown desktop-backed agent-definition command `{command}`. Use list, draft, validate, preview, save, update, clone, archive, or attachable-skills."
            ),
        )),
    }
}

fn handle_code_history_command(
    state_dir: &Path,
    args: &[String],
) -> Result<JsonValue, CommandError> {
    let command = args.first().map(String::as_str).unwrap_or("list");
    match command {
        "list" | "operations" => handle_code_history_list(state_dir, &args[1..]),
        "selective-undo" | "undo" => handle_selective_undo(state_dir, &args[1..]),
        "session-rollback" | "rollback" | "return-to-here" => {
            handle_session_rollback(state_dir, &args[1..])
        }
        _ => Err(CommandError::user_fixable(
            "xero_tui_code_history_command_unknown",
            format!(
                "Unknown desktop-backed code-history command `{command}`. Use list, selective-undo, or session-rollback."
            ),
        )),
    }
}

fn handle_code_history_list(state_dir: &Path, args: &[String]) -> Result<JsonValue, CommandError> {
    let project_id = required_option(args, "--project-id", "projectId")?;
    let agent_session_id = option_value(args, "--session-id")
        .or_else(|| option_value(args, "--agent-session-id"))
        .ok_or_else(|| CommandError::invalid_request("agentSessionId"))?;
    let run_id = option_value(args, "--run-id");
    configure_project_store_paths(state_dir);
    let repo_root = project_root(state_dir, &project_id)?;
    let operations = project_store::list_code_history_operations_for_session(
        &repo_root,
        &project_id,
        &agent_session_id,
        run_id.as_deref(),
    )?
    .into_iter()
    .map(code_history_operation_record_json)
    .collect::<Vec<_>>();
    Ok(json!({
        "kind": "codeHistoryOperations",
        "schema": "xero.code_history_operations_command.v1",
        "projectId": project_id,
        "agentSessionId": agent_session_id,
        "runId": run_id,
        "operations": operations,
        "backend": "desktop_project_store_code_rollback",
        "uiDeferred": false
    }))
}

fn handle_selective_undo(state_dir: &Path, args: &[String]) -> Result<JsonValue, CommandError> {
    require_apply(args, "selective undo")?;
    let project_id = required_option(args, "--project-id", "projectId")?;
    let target_kind = required_option(args, "--target-kind", "target.targetKind")?;
    let target_id = required_option(args, "--target-id", "target.targetId")?;
    let change_group_id = option_value(args, "--change-group-id")
        .or_else(|| matches_normalized(&target_kind, "change_group").then(|| target_id.clone()));
    let expected_workspace_epoch = option_u64(args, "--expected-workspace-epoch")?;
    configure_project_store_paths(state_dir);
    let repo_root = project_root(state_dir, &project_id)?;

    if matches_normalized(&target_kind, "change_group") {
        let applied = project_store::apply_code_change_group_undo(
            &repo_root,
            project_store::ApplyCodeChangeGroupUndoRequest {
                project_id,
                operation_id: option_value(args, "--operation-id"),
                target_change_group_id: change_group_id
                    .ok_or_else(|| CommandError::invalid_request("target.changeGroupId"))?,
                expected_workspace_epoch,
            },
        )?;
        return Ok(json!({
            "kind": "codeHistorySelectiveUndo",
            "schema": "xero.code_history_selective_undo_command.v1",
            "projectId": applied.project_id,
            "operation": selective_change_group_undo_json("change_group", target_id, applied),
            "backend": "desktop_project_store_code_rollback",
            "uiDeferred": false
        }));
    }

    if matches_normalized(&target_kind, "file_change") || matches_normalized(&target_kind, "hunks")
    {
        let file_path = required_option(args, "--file-path", "target.filePath")?;
        let hunk_ids = if matches_normalized(&target_kind, "hunks") {
            let values = option_values(args, "--hunk-id");
            if values.is_empty() {
                return Err(CommandError::invalid_request("target.hunkIds"));
            }
            values
        } else {
            Vec::new()
        };
        let applied = project_store::apply_code_file_undo(
            &repo_root,
            project_store::ApplyCodeFileUndoRequest {
                project_id,
                operation_id: option_value(args, "--operation-id"),
                target_change_group_id: change_group_id
                    .ok_or_else(|| CommandError::invalid_request("target.changeGroupId"))?,
                target_patch_file_id: matches_normalized(&target_kind, "file_change")
                    .then(|| target_id.clone()),
                target_file_path: Some(file_path),
                target_hunk_ids: hunk_ids,
                expected_workspace_epoch,
            },
        )?;
        let target_kind_label = if matches_normalized(&target_kind, "hunks") {
            "hunks"
        } else {
            "file_change"
        };
        return Ok(json!({
            "kind": "codeHistorySelectiveUndo",
            "schema": "xero.code_history_selective_undo_command.v1",
            "projectId": applied.project_id,
            "operation": selective_file_undo_json(target_kind_label, target_id, applied),
            "backend": "desktop_project_store_code_rollback",
            "uiDeferred": false
        }));
    }

    Err(CommandError::user_fixable(
        "xero_tui_selective_undo_target_kind_invalid",
        "Selective undo target kind must be change_group, file_change, or hunks.",
    ))
}

fn handle_session_rollback(state_dir: &Path, args: &[String]) -> Result<JsonValue, CommandError> {
    require_apply(args, "session rollback")?;
    let project_id = required_option(args, "--project-id", "projectId")?;
    let target_kind = required_option(args, "--target-kind", "target.targetKind")?;
    let target_id = required_option(args, "--target-id", "target.targetId")?;
    let agent_session_id = option_value(args, "--session-id")
        .or_else(|| option_value(args, "--agent-session-id"))
        .ok_or_else(|| CommandError::invalid_request("target.agentSessionId"))?;
    let boundary_id = required_option(args, "--boundary-id", "target.boundaryId")?;
    let expected_workspace_epoch = option_u64(args, "--expected-workspace-epoch")?;
    let boundary_kind = if matches_normalized(&target_kind, "session_boundary") {
        project_store::CodeSessionBoundaryTargetKind::SessionBoundary
    } else if matches_normalized(&target_kind, "run_boundary") {
        project_store::CodeSessionBoundaryTargetKind::RunBoundary
    } else {
        return Err(CommandError::user_fixable(
            "xero_tui_session_rollback_target_kind_invalid",
            "Session rollback target kind must be session_boundary or run_boundary.",
        ));
    };
    configure_project_store_paths(state_dir);
    let repo_root = project_root(state_dir, &project_id)?;
    let applied = project_store::apply_code_session_rollback(
        &repo_root,
        project_store::ApplyCodeSessionRollbackRequest {
            boundary: project_store::ResolveCodeSessionBoundaryRequest {
                project_id,
                agent_session_id,
                target_kind: boundary_kind,
                target_id: target_id.clone(),
                boundary_id,
                run_id: option_value(args, "--run-id"),
                change_group_id: option_value(args, "--change-group-id"),
            },
            operation_id: option_value(args, "--operation-id"),
            explicitly_selected_change_group_ids: option_values(args, "--selected-change-group-id"),
            expected_workspace_epoch,
        },
    )?;
    Ok(json!({
        "kind": "codeHistorySessionRollback",
        "schema": "xero.code_history_session_rollback_command.v1",
        "projectId": applied.project_id,
        "operation": session_rollback_json(target_kind_label(boundary_kind), target_id, applied),
        "backend": "desktop_project_store_code_rollback",
        "uiDeferred": false
    }))
}

fn require_apply(args: &[String], operation: &str) -> Result<(), CommandError> {
    if option_present(args, "--apply") || option_present(args, "--yes") {
        return Ok(());
    }
    Err(CommandError::user_fixable(
        "xero_tui_code_history_apply_required",
        format!("Applying {operation} changes project files. Re-run with --apply after reviewing the target."),
    ))
}

fn selective_file_undo_json(
    target_kind: &str,
    target_id: String,
    applied: project_store::AppliedCodeFileUndo,
) -> JsonValue {
    json!({
        "mode": "selective_undo",
        "operationId": applied.operation_id,
        "status": applied.status.as_str(),
        "agentSessionId": applied.agent_session_id,
        "runId": applied.run_id,
        "target": {
            "targetKind": target_kind,
            "targetId": target_id,
            "changeGroupId": applied.target_change_group_id,
            "filePath": applied.target_file_path,
            "hunkIds": applied.selected_hunk_ids
        },
        "affectedPaths": applied.affected_paths,
        "conflicts": conflicts_json(&applied.conflicts),
        "workspaceHead": applied.workspace_head,
        "patchAvailability": applied.patch_availability.map(|metadata| metadata.patch_availability),
        "resultCommitId": applied.result_commit_id,
        "resultChangeGroupId": applied.result_change_group_id
    })
}

fn selective_change_group_undo_json(
    target_kind: &str,
    target_id: String,
    applied: project_store::AppliedCodeChangeGroupUndo,
) -> JsonValue {
    json!({
        "mode": "selective_undo",
        "operationId": applied.operation_id,
        "status": applied.status.as_str(),
        "agentSessionId": applied.agent_session_id,
        "runId": applied.run_id,
        "target": {
            "targetKind": target_kind,
            "targetId": target_id,
            "changeGroupId": applied.target_change_group_id,
            "hunkIds": []
        },
        "affectedPaths": applied.affected_paths,
        "conflicts": conflicts_json(&applied.conflicts),
        "workspaceHead": applied.workspace_head,
        "patchAvailability": applied.patch_availability.map(|metadata| metadata.patch_availability),
        "resultCommitId": applied.result_commit_id,
        "resultChangeGroupId": applied.result_change_group_id
    })
}

fn session_rollback_json(
    target_kind: &str,
    target_id: String,
    applied: project_store::AppliedCodeSessionRollback,
) -> JsonValue {
    json!({
        "mode": "session_rollback",
        "operationId": applied.operation_id,
        "status": applied.status.as_str(),
        "agentSessionId": applied.agent_session_id,
        "runId": applied.run_id,
        "target": {
            "targetKind": target_kind,
            "targetId": target_id,
            "boundaryId": applied.boundary_id,
            "boundaryChangeGroupId": applied.boundary_change_group_id,
            "targetChangeGroupIds": applied.target_change_group_ids
        },
        "affectedPaths": applied.affected_paths,
        "conflicts": conflicts_json(&applied.conflicts),
        "workspaceHead": applied.workspace_head,
        "patchAvailability": applied.patch_availability.map(|metadata| metadata.patch_availability),
        "resultCommitId": applied.result_commit_id,
        "resultChangeGroupId": applied.result_change_group_id
    })
}

fn conflicts_json(conflicts: &[project_store::CodeFileUndoConflict]) -> Vec<JsonValue> {
    conflicts
        .iter()
        .map(|conflict| {
            json!({
                "path": conflict.path,
                "kind": conflict.kind.as_str(),
                "message": conflict.message,
                "baseHash": conflict.base_hash,
                "selectedHash": conflict.selected_hash,
                "currentHash": conflict.current_hash,
                "hunkIds": conflict.hunk_ids
            })
        })
        .collect()
}

fn code_history_operation_record_json(
    operation: project_store::CodeHistoryOperationRecord,
) -> JsonValue {
    json!({
        "projectId": operation.project_id,
        "operationId": operation.operation_id,
        "mode": operation.mode,
        "status": operation.status,
        "targetKind": operation.target_kind,
        "targetId": operation.target_id,
        "targetChangeGroupId": operation.target_change_group_id,
        "targetFilePath": operation.target_file_path,
        "targetHunkIds": operation.target_hunk_ids,
        "agentSessionId": operation.agent_session_id,
        "runId": operation.run_id,
        "expectedWorkspaceEpoch": operation.expected_workspace_epoch,
        "affectedPaths": operation.affected_paths,
        "conflicts": operation.conflicts.into_iter().map(|conflict| {
            json!({
                "path": conflict.path,
                "kind": conflict.kind,
                "message": conflict.message,
                "baseHash": conflict.base_hash,
                "selectedHash": conflict.selected_hash,
                "currentHash": conflict.current_hash,
                "hunkIds": conflict.hunk_ids
            })
        }).collect::<Vec<_>>(),
        "resultChangeGroupId": operation.result_change_group_id,
        "resultCommitId": operation.result_commit_id,
        "failureCode": operation.failure_code,
        "failureMessage": operation.failure_message,
        "repairCode": operation.repair_code,
        "repairMessage": operation.repair_message,
        "targetSummaryLabel": operation.target_summary_label,
        "resultSummaryLabel": operation.result_summary_label,
        "createdAt": operation.created_at,
        "updatedAt": operation.updated_at,
        "completedAt": operation.completed_at
    })
}

fn target_kind_label(kind: project_store::CodeSessionBoundaryTargetKind) -> &'static str {
    match kind {
        project_store::CodeSessionBoundaryTargetKind::SessionBoundary => "session_boundary",
        project_store::CodeSessionBoundaryTargetKind::RunBoundary => "run_boundary",
    }
}

fn handle_conversation_rewind(
    state_dir: &Path,
    args: &[String],
) -> Result<JsonValue, CommandError> {
    let project_id = required_option(args, "--project-id", "projectId")?;
    let source_agent_session_id = option_value(args, "--session-id")
        .or_else(|| option_value(args, "--agent-session-id"))
        .ok_or_else(|| CommandError::invalid_request("sourceAgentSessionId"))?;
    let source_run_id = positional_or_option(args, "--run-id", "sourceRunId")?;
    let boundary_kind = required_option(args, "--boundary-kind", "boundaryKind")?;
    let boundary = if matches_normalized(&boundary_kind, "message") {
        project_store::AgentSessionBranchBoundary::Message {
            message_id: option_i64(args, "--source-message-id")?
                .ok_or_else(|| CommandError::invalid_request("sourceMessageId"))?,
        }
    } else if matches_normalized(&boundary_kind, "checkpoint") {
        project_store::AgentSessionBranchBoundary::Checkpoint {
            checkpoint_id: option_i64(args, "--source-checkpoint-id")?
                .ok_or_else(|| CommandError::invalid_request("sourceCheckpointId"))?,
        }
    } else {
        return Err(CommandError::user_fixable(
            "xero_tui_conversation_rewind_boundary_invalid",
            "Conversation rewind boundaryKind must be message or checkpoint.",
        ));
    };
    configure_project_store_paths(state_dir);
    let repo_root = project_root(state_dir, &project_id)?;
    let branch = project_store::create_agent_session_branch(
        &repo_root,
        &project_store::AgentSessionBranchCreateRecord {
            project_id,
            source_agent_session_id,
            source_run_id,
            target_agent_session_id: None,
            title: option_value(args, "--title"),
            selected: !option_present(args, "--no-select"),
            boundary,
        },
    )?;
    Ok(json!({
        "kind": "conversationRewind",
        "schema": "xero.conversation_rewind_command.v1",
        "projectId": branch.session.project_id,
        "session": agent_session_json(branch.session),
        "lineage": agent_session_lineage_json(branch.lineage),
        "replayRun": agent_run_json(branch.replay_run.run),
        "backend": "desktop_project_store_session_lineage",
        "uiDeferred": false
    }))
}

fn agent_session_json(session: project_store::AgentSessionRecord) -> JsonValue {
    json!({
        "projectId": session.project_id,
        "agentSessionId": session.agent_session_id,
        "title": session.title,
        "summary": session.summary,
        "status": agent_session_status_label(&session.status),
        "selected": session.selected,
        "createdAt": session.created_at,
        "updatedAt": session.updated_at,
        "archivedAt": session.archived_at,
        "lastRunId": session.last_run_id,
        "lastRuntimeKind": session.last_runtime_kind,
        "lastProviderId": session.last_provider_id
    })
}

fn agent_session_lineage_json(lineage: project_store::AgentSessionLineageRecord) -> JsonValue {
    json!({
        "lineageId": lineage.lineage_id,
        "projectId": lineage.project_id,
        "childAgentSessionId": lineage.child_agent_session_id,
        "sourceAgentSessionId": lineage.source_agent_session_id,
        "sourceRunId": lineage.source_run_id,
        "sourceBoundaryKind": agent_session_lineage_boundary_label(&lineage.source_boundary_kind),
        "sourceMessageId": lineage.source_message_id,
        "sourceCheckpointId": lineage.source_checkpoint_id,
        "sourceCompactionId": lineage.source_compaction_id,
        "sourceTitle": lineage.source_title,
        "branchTitle": lineage.branch_title,
        "replayRunId": lineage.replay_run_id,
        "fileChangeSummary": lineage.file_change_summary,
        "diagnostic": lineage.diagnostic.map(|diagnostic| json!({
            "code": diagnostic.code,
            "message": diagnostic.message
        })),
        "createdAt": lineage.created_at,
        "sourceDeletedAt": lineage.source_deleted_at
    })
}

fn agent_run_json(run: project_store::AgentRunRecord) -> JsonValue {
    json!({
        "projectId": run.project_id,
        "agentSessionId": run.agent_session_id,
        "runId": run.run_id,
        "traceId": run.trace_id,
        "lineageKind": run.lineage_kind,
        "parentRunId": run.parent_run_id,
        "providerId": run.provider_id,
        "modelId": run.model_id,
        "status": agent_run_status_label(&run.status),
        "prompt": text_preview(&run.prompt),
        "startedAt": run.started_at,
        "completedAt": run.completed_at,
        "cancelledAt": run.cancelled_at,
        "updatedAt": run.updated_at
    })
}

fn agent_session_status_label(value: &project_store::AgentSessionStatus) -> &'static str {
    match value {
        project_store::AgentSessionStatus::Active => "active",
        project_store::AgentSessionStatus::Archived => "archived",
    }
}

fn agent_session_lineage_boundary_label(
    value: &project_store::AgentSessionLineageBoundaryKind,
) -> &'static str {
    match value {
        project_store::AgentSessionLineageBoundaryKind::Run => "run",
        project_store::AgentSessionLineageBoundaryKind::Message => "message",
        project_store::AgentSessionLineageBoundaryKind::Checkpoint => "checkpoint",
    }
}

fn agent_run_status_label(value: &project_store::AgentRunStatus) -> &'static str {
    match value {
        project_store::AgentRunStatus::Starting => "starting",
        project_store::AgentRunStatus::Running => "running",
        project_store::AgentRunStatus::Paused => "paused",
        project_store::AgentRunStatus::Cancelling => "cancelling",
        project_store::AgentRunStatus::Cancelled => "cancelled",
        project_store::AgentRunStatus::HandedOff => "handed_off",
        project_store::AgentRunStatus::Completed => "completed",
        project_store::AgentRunStatus::Failed => "failed",
    }
}

fn handle_solana_command(state_dir: &Path, args: &[String]) -> Result<JsonValue, CommandError> {
    let command = args.first().map(String::as_str).unwrap_or("catalog");
    match command {
        "catalog" | "surface" => Ok(json!({
            "kind": "solanaCatalog",
            "schema": "xero.solana_tui_catalog_command.v1",
            "commands": [
                "cluster-list",
                "scenario-list",
                "persona-roles",
                "token-extension-matrix",
                "wallet-scaffold-list",
                "doc-catalog",
                "doc-snippets --tool TOOL",
                "secrets-patterns",
                "secrets-scan --project-id ID|--project-root PATH",
                "pda-scan --project-id ID|--project-root PATH",
                "pda-derive --request-json JSON"
            ],
            "backend": "desktop_solana_module",
            "uiDeferred": false
        })),
        "cluster-list" | "clusters" => solana_command_json(
            "solanaClusterList",
            "xero.solana_cluster_list_command.v1",
            solana::solana_cluster_list()?,
        ),
        "scenario-list" | "scenarios" => solana_command_json(
            "solanaScenarioList",
            "xero.solana_scenario_list_command.v1",
            solana::solana_scenario_list()?,
        ),
        "persona-roles" | "roles" => solana_command_json(
            "solanaPersonaRoles",
            "xero.solana_persona_roles_command.v1",
            solana::solana_persona_roles()?,
        ),
        "token-extension-matrix" | "token-matrix" => solana_command_json(
            "solanaTokenExtensionMatrix",
            "xero.solana_token_extension_matrix_command.v1",
            solana::solana_token_extension_matrix()?,
        ),
        "wallet-scaffold-list" | "wallets" => solana_command_json(
            "solanaWalletScaffoldList",
            "xero.solana_wallet_scaffold_list_command.v1",
            solana::solana_wallet_scaffold_list()?,
        ),
        "doc-catalog" | "docs" => solana_command_json(
            "solanaDocCatalog",
            "xero.solana_doc_catalog_command.v1",
            solana::solana_doc_catalog()?,
        ),
        "doc-snippets" | "doc" => {
            let tool = required_option(&args[1..], "--tool", "tool")?;
            solana_command_json(
                "solanaDocSnippets",
                "xero.solana_doc_snippets_command.v1",
                solana::solana_doc_snippets(solana::DocSnippetsArgs { tool })?,
            )
        }
        "secrets-patterns" => solana_command_json(
            "solanaSecretsPatterns",
            "xero.solana_secrets_patterns_command.v1",
            solana::solana_secrets_patterns()?,
        ),
        "secrets-scan" => {
            let request = if let Some(value) = request_json_arg(&args[1..])? {
                value
            } else {
                solana::SecretsScanArgs {
                    request: solana::SecretsScanRequest {
                        project_root: solana_project_root(state_dir, &args[1..])?,
                        skip_paths: option_values(&args[1..], "--skip-path"),
                        min_severity: None,
                        file_budget: option_u64(&args[1..], "--file-budget")?
                            .map(|value| value as usize),
                    },
                }
            };
            solana_command_json(
                "solanaSecretsScan",
                "xero.solana_secrets_scan_command.v1",
                solana::solana_secrets_scan(request)?,
            )
        }
        "pda-scan" => {
            let request = solana::PdaScanRequest {
                project_root: solana_project_root(state_dir, &args[1..])?,
            };
            solana_command_json(
                "solanaPdaScan",
                "xero.solana_pda_scan_command.v1",
                solana::solana_pda_scan(request)?,
            )
        }
        "pda-derive" => {
            let request = request_json_arg::<solana::PdaDeriveRequest>(&args[1..])?
                .ok_or_else(|| CommandError::invalid_request("requestJson"))?;
            solana_command_json(
                "solanaPdaDerive",
                "xero.solana_pda_derive_command.v1",
                solana::solana_pda_derive(request)?,
            )
        }
        _ => Err(CommandError::user_fixable(
            "xero_tui_solana_command_unknown",
            format!(
                "Unknown desktop-backed solana command `{command}`. Use catalog, cluster-list, scenario-list, persona-roles, token-extension-matrix, wallet-scaffold-list, doc-catalog, doc-snippets, secrets-patterns, secrets-scan, pda-scan, or pda-derive."
            ),
        )),
    }
}

fn solana_project_root(state_dir: &Path, args: &[String]) -> Result<String, CommandError> {
    if let Some(root) = option_value(args, "--project-root") {
        return Ok(root);
    }
    let project_id = required_option(args, "--project-id", "projectId")?;
    Ok(project_root(state_dir, &project_id)?
        .to_string_lossy()
        .into_owned())
}

fn request_json_arg<T>(args: &[String]) -> Result<Option<T>, CommandError>
where
    T: DeserializeOwned,
{
    option_value(args, "--request-json")
        .map(|raw| {
            serde_json::from_str(&raw).map_err(|error| {
                CommandError::user_fixable(
                    "xero_tui_request_json_invalid",
                    format!("Could not parse --request-json: {error}"),
                )
            })
        })
        .transpose()
}

fn solana_command_json<T>(
    kind: &'static str,
    schema: &'static str,
    value: T,
) -> Result<JsonValue, CommandError>
where
    T: Serialize,
{
    Ok(json!({
        "kind": kind,
        "schema": schema,
        "result": serde_json::to_value(value).map_err(|error| {
            CommandError::system_fault(
                "xero_tui_solana_encode_failed",
                format!("Could not encode Solana command result: {error}"),
            )
        })?,
        "backend": "desktop_solana_module",
        "uiDeferred": false
    }))
}

fn handle_environment_service_command(
    state_dir: &Path,
    args: &[String],
) -> Result<JsonValue, CommandError> {
    let command = args.first().map(String::as_str).unwrap_or("refresh");
    let database_path = state_dir.join(GLOBAL_DATABASE_FILE_NAME);
    match command {
        "start" => environment_command_json(
            "environmentDiscoveryStart",
            environment_service::start_environment_discovery(database_path)?,
        ),
        "refresh" => environment_command_json(
            "environmentDiscoveryRefresh",
            environment_service::refresh_environment_discovery(database_path)?,
        ),
        "resolve-permissions" => {
            let decisions = option_value(&args[1..], "--decisions-json")
                .ok_or_else(|| CommandError::invalid_request("decisions"))?;
            let decisions = serde_json::from_str::<Vec<environment_service::EnvironmentPermissionDecision>>(&decisions)
                .map_err(|error| {
                    CommandError::user_fixable(
                        "xero_tui_environment_decisions_invalid",
                        format!("Could not parse --decisions-json: {error}"),
                    )
                })?;
            environment_command_json(
                "environmentPermissionResolution",
                environment_service::resolve_environment_permission_requests(&database_path, decisions)?,
            )
        }
        "verify-tool" => {
            let request = request_json_arg::<environment_service::VerifyUserToolRequest>(&args[1..])?
                .ok_or_else(|| CommandError::invalid_request("requestJson"))?;
            environment_command_json(
                "environmentToolVerify",
                environment_service::verify_user_environment_tool(request)?,
            )
        }
        "save-tool-verified" => {
            let request = request_json_arg::<environment_service::VerifyUserToolRequest>(&args[1..])?
                .ok_or_else(|| CommandError::invalid_request("requestJson"))?;
            environment_command_json(
                "environmentToolSaveVerified",
                environment_service::save_user_environment_tool(&database_path, request)?,
            )
        }
        "remove-tool-verified" => {
            let tool_id = positional_or_option(&args[1..], "--tool-id", "toolId")?;
            environment_command_json(
                "environmentToolRemoveVerified",
                environment_service::remove_user_environment_tool(&database_path, tool_id)?,
            )
        }
        _ => Err(CommandError::user_fixable(
            "xero_tui_environment_command_unknown",
            format!(
                "Unknown desktop-backed environment command `{command}`. Use start, refresh, resolve-permissions, verify-tool, save-tool-verified, or remove-tool-verified."
            ),
        )),
    }
}

fn environment_command_json<T>(kind: &'static str, value: T) -> Result<JsonValue, CommandError>
where
    T: Serialize,
{
    Ok(json!({
        "kind": kind,
        "schema": "xero.environment_service_command.v1",
        "result": serde_json::to_value(value).map_err(|error| {
            CommandError::system_fault(
                "xero_tui_environment_encode_failed",
                format!("Could not encode environment service result: {error}"),
            )
        })?,
        "backend": "desktop_environment_service",
        "uiDeferred": false
    }))
}

fn handle_terminal_command(state_dir: &Path, args: &[String]) -> Result<JsonValue, CommandError> {
    let command = args.first().map(String::as_str).unwrap_or("list");
    match command {
        "open" | "new" | "shell" => {
            let cwd = terminal_cwd(state_dir, &args[1..])?;
            let terminal =
                project_runner::terminal_open_for_cwd(&cwd, option_u16(&args[1..], "--cols")?, option_u16(&args[1..], "--rows")?)?;
            terminal_command_json(
                "terminalOpen",
                "Opened project PTY through the shared desktop project runner.",
                terminal,
            )
        }
        "list" | "sessions" => terminal_command_json(
            "terminalList",
            "Listed active shared project-runner PTYs.",
            project_runner::terminal_list_active()?,
        ),
        "read" | "tail" => {
            let terminal_id = positional_or_option(&args[1..], "--terminal-id", "terminalId")?;
            let output = project_runner::terminal_read_buffer(project_runner::TerminalReadRequestDto {
                terminal_id,
                after_sequence: option_u64(&args[1..], "--after-sequence")?,
                max_bytes: option_usize(&args[1..], "--max-bytes")?,
            })?;
            terminal_command_json(
                "terminalRead",
                "Read retained PTY output from the shared desktop project runner.",
                output,
            )
        }
        "write" | "send" => {
            let terminal_id = required_option(&args[1..], "--terminal-id", "terminalId")?;
            let mut data = option_value(&args[1..], "--data")
                .or_else(|| option_value(&args[1..], "--text"))
                .or_else(|| first_positional(&args[1..]))
                .ok_or_else(|| CommandError::invalid_request("data"))?;
            if option_present(&args[1..], "--newline") {
                data.push('\n');
            }
            project_runner::terminal_write_direct(project_runner::TerminalWriteRequestDto {
                terminal_id: terminal_id.clone(),
                data,
            })?;
            terminal_command_json(
                "terminalWrite",
                "Wrote input to the shared desktop project-runner PTY.",
                json!({ "terminalId": terminal_id }),
            )
        }
        "resize" => {
            let terminal_id = required_option(&args[1..], "--terminal-id", "terminalId")?;
            let cols = option_u16(&args[1..], "--cols")?
                .ok_or_else(|| CommandError::invalid_request("cols"))?;
            let rows = option_u16(&args[1..], "--rows")?
                .ok_or_else(|| CommandError::invalid_request("rows"))?;
            project_runner::terminal_resize_direct(project_runner::TerminalResizeRequestDto {
                terminal_id: terminal_id.clone(),
                cols,
                rows,
            })?;
            terminal_command_json(
                "terminalResize",
                "Resized the shared desktop project-runner PTY.",
                json!({ "terminalId": terminal_id, "cols": cols, "rows": rows }),
            )
        }
        "close" | "stop" => {
            let terminal_id = positional_or_option(&args[1..], "--terminal-id", "terminalId")?;
            project_runner::terminal_close_direct(project_runner::TerminalIdRequestDto {
                terminal_id: terminal_id.clone(),
            })?;
            terminal_command_json(
                "terminalClose",
                "Closed the shared desktop project-runner PTY.",
                json!({ "terminalId": terminal_id }),
            )
        }
        _ => Err(CommandError::user_fixable(
            "xero_tui_terminal_command_unknown",
            format!(
                "Unknown desktop-backed terminal command `{command}`. Use open, list, read, write, resize, or close."
            ),
        )),
    }
}

fn terminal_cwd(state_dir: &Path, args: &[String]) -> Result<PathBuf, CommandError> {
    let cwd = if let Some(cwd) = option_value(args, "--cwd") {
        PathBuf::from(cwd)
    } else if let Some(project_id) = option_value(args, "--project-id") {
        project_root(state_dir, &project_id)?
    } else {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    };
    if !cwd.is_dir() {
        return Err(CommandError::user_fixable(
            "xero_tui_terminal_cwd_invalid",
            format!("Terminal cwd `{}` is not a directory.", cwd.display()),
        ));
    }
    Ok(cwd)
}

fn terminal_command_json<T>(
    kind: &'static str,
    message: &'static str,
    value: T,
) -> Result<JsonValue, CommandError>
where
    T: Serialize,
{
    Ok(json!({
        "kind": kind,
        "schema": "xero.project_terminal_command.v1",
        "message": message,
        "result": serde_json::to_value(value).map_err(|error| {
            CommandError::system_fault(
                "xero_tui_terminal_encode_failed",
                format!("Could not encode terminal command result: {error}"),
            )
        })?,
        "backend": "desktop_project_runner_pty",
        "uiDeferred": false
    }))
}

fn handle_skill_sources_command(
    state_dir: &Path,
    args: &[String],
) -> Result<JsonValue, CommandError> {
    let command = args.first().map(String::as_str).unwrap_or("list");
    let database_path = state_dir.join(GLOBAL_DATABASE_FILE_NAME);
    match command {
        "list" | "settings" => skill_source_command_json(
            "skillSourceSettings",
            "Loaded shared skill source settings.",
            load_skill_source_settings_from_path(&database_path)?,
        ),
        "upsert-local-root" | "local-root-upsert" => {
            let settings = load_skill_source_settings_from_path(&database_path)?;
            let updated = settings.upsert_local_root(
                option_value(&args[1..], "--root-id"),
                required_option(&args[1..], "--path", "path")?,
                enabled_from_flags(&args[1..], true),
            )?;
            skill_source_command_json(
                "skillSourceLocalRootUpsert",
                "Saved shared local skill root settings.",
                persist_skill_source_settings(&database_path, updated)?,
            )
        }
        "remove-local-root" | "local-root-remove" => {
            let settings = load_skill_source_settings_from_path(&database_path)?;
            let updated =
                settings.remove_local_root(&positional_or_option(&args[1..], "--root-id", "rootId")?)?;
            skill_source_command_json(
                "skillSourceLocalRootRemove",
                "Removed shared local skill root settings.",
                persist_skill_source_settings(&database_path, updated)?,
            )
        }
        "github" | "github-source" => {
            let settings = load_skill_source_settings_from_path(&database_path)?;
            let repo = required_option(&args[1..], "--repo", "repo")?;
            let reference = option_value(&args[1..], "--reference")
                .or_else(|| option_value(&args[1..], "--ref"))
                .unwrap_or_else(|| settings.github.reference.clone());
            let root = option_value(&args[1..], "--root").unwrap_or_else(|| settings.github.root.clone());
            let updated = settings.update_github(
                repo,
                reference,
                root,
                enabled_from_flags(&args[1..], true),
            )?;
            skill_source_command_json(
                "skillSourceGithubUpdate",
                "Updated shared GitHub skill source settings.",
                persist_skill_source_settings(&database_path, updated)?,
            )
        }
        "project" | "project-source" => {
            let settings = load_skill_source_settings_from_path(&database_path)?;
            let project_id = required_option(&args[1..], "--project-id", "projectId")?;
            let updated = settings.update_project(project_id, enabled_from_flags(&args[1..], true))?;
            skill_source_command_json(
                "skillSourceProjectUpdate",
                "Updated shared project skill source settings.",
                persist_skill_source_settings(&database_path, updated)?,
            )
        }
        "reload" | "discover" => handle_skill_sources_reload(state_dir, &args[1..]),
        _ => Err(CommandError::user_fixable(
            "xero_tui_skill_sources_command_unknown",
            format!(
                "Unknown desktop-backed skill-sources command `{command}`. Use list, upsert-local-root, remove-local-root, github, project, or reload."
            ),
        )),
    }
}

fn handle_plugin_sources_command(
    state_dir: &Path,
    args: &[String],
) -> Result<JsonValue, CommandError> {
    let command = args.first().map(String::as_str).unwrap_or("list");
    let database_path = state_dir.join(GLOBAL_DATABASE_FILE_NAME);
    match command {
        "list" | "settings" => skill_source_command_json(
            "pluginSourceSettings",
            "Loaded shared plugin source settings.",
            load_skill_source_settings_from_path(&database_path)?,
        ),
        "upsert-root" | "root-upsert" => {
            let settings = load_skill_source_settings_from_path(&database_path)?;
            let updated = settings.upsert_plugin_root(
                option_value(&args[1..], "--root-id"),
                required_option(&args[1..], "--path", "path")?,
                enabled_from_flags(&args[1..], true),
            )?;
            skill_source_command_json(
                "pluginSourceRootUpsert",
                "Saved shared plugin root settings.",
                persist_skill_source_settings(&database_path, updated)?,
            )
        }
        "remove-root" | "root-remove" => {
            let settings = load_skill_source_settings_from_path(&database_path)?;
            let updated =
                settings.remove_plugin_root(&positional_or_option(&args[1..], "--root-id", "rootId")?)?;
            skill_source_command_json(
                "pluginSourceRootRemove",
                "Removed shared plugin root settings.",
                persist_skill_source_settings(&database_path, updated)?,
            )
        }
        "reload" | "discover" => handle_plugin_sources_reload(state_dir, &args[1..]),
        _ => Err(CommandError::user_fixable(
            "xero_tui_plugin_sources_command_unknown",
            format!(
                "Unknown desktop-backed plugin-sources command `{command}`. Use list, upsert-root, remove-root, or reload."
            ),
        )),
    }
}

fn handle_skill_sources_reload(
    state_dir: &Path,
    args: &[String],
) -> Result<JsonValue, CommandError> {
    let database_path = state_dir.join(GLOBAL_DATABASE_FILE_NAME);
    let settings = load_skill_source_settings_from_path(&database_path)?;
    let mut candidates = Vec::new();
    let mut diagnostics = Vec::new();
    if let Some(project_id) = option_value(args, "--project-id")
        .filter(|project_id| settings.project_discovery_enabled(project_id))
    {
        let repo_root = project_root(state_dir, &project_id)?;
        let project_app_data_dir = db::project_app_data_dir_for_repo(&repo_root);
        let discovered = discover_project_skill_directory(project_id, project_app_data_dir)?;
        candidates.extend(discovered.candidates.into_iter().map(discovered_skill_json));
        diagnostics.extend(
            discovered
                .diagnostics
                .into_iter()
                .map(skill_diagnostic_json),
        );
    }
    for root in settings.enabled_local_roots() {
        let discovered = discover_local_skill_directory(root.root_id, root.path)?;
        candidates.extend(discovered.candidates.into_iter().map(discovered_skill_json));
        diagnostics.extend(
            discovered
                .diagnostics
                .into_iter()
                .map(skill_diagnostic_json),
        );
    }
    skill_source_command_json(
        "skillSourceReload",
        "Reloaded shared project/local skill discovery.",
        json!({
            "candidateCount": candidates.len(),
            "diagnosticCount": diagnostics.len(),
            "candidates": candidates,
            "diagnostics": diagnostics
        }),
    )
}

fn handle_plugin_sources_reload(
    state_dir: &Path,
    args: &[String],
) -> Result<JsonValue, CommandError> {
    let project_id = required_option(args, "--project-id", "projectId")?;
    let database_path = state_dir.join(GLOBAL_DATABASE_FILE_NAME);
    let settings = load_skill_source_settings_from_path(&database_path)?;
    let roots = settings
        .enabled_plugin_roots()
        .into_iter()
        .map(|root| XeroPluginRoot {
            root_id: root.root_id,
            root_path: PathBuf::from(root.path),
        });
    let discovery = discover_plugin_roots(roots)?;
    configure_project_store_paths(state_dir);
    let repo_root = project_root(state_dir, &project_id)?;
    let records = project_store::sync_discovered_plugins(&repo_root, &discovery.plugins, true)?;
    let plugins = records
        .into_iter()
        .map(|record| {
            json!({
                "pluginId": record.plugin_id,
                "name": record.name,
                "version": record.version,
                "state": record.state,
                "trust": record.trust,
                "rootId": record.root_id,
                "commandCount": record.manifest.commands.len(),
                "skillCount": record.manifest.skills.len(),
                "lastReloadedAt": record.last_reloaded_at
            })
        })
        .collect::<Vec<_>>();
    let diagnostics = discovery
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            json!({
                "code": diagnostic.code,
                "message": diagnostic.message,
                "rootId": diagnostic.root_id,
                "relativePath": diagnostic.relative_path
            })
        })
        .collect::<Vec<_>>();
    skill_source_command_json(
        "pluginSourceReload",
        "Reloaded shared plugin roots and synced discovered plugins.",
        json!({
            "projectId": project_id,
            "pluginCount": plugins.len(),
            "diagnosticCount": diagnostics.len(),
            "plugins": plugins,
            "diagnostics": diagnostics
        }),
    )
}

fn discovered_skill_json(skill: xero_desktop_lib::runtime::XeroDiscoveredSkill) -> JsonValue {
    json!({
        "sourceId": skill.source.source_id,
        "skillId": skill.skill_id,
        "name": skill.name,
        "description": skill.description,
        "userInvocable": skill.user_invocable,
        "localLocation": skill.local_location,
        "versionHash": skill.version_hash,
        "assetPaths": skill.asset_paths,
        "totalBytes": skill.total_bytes
    })
}

fn skill_diagnostic_json(
    diagnostic: xero_desktop_lib::runtime::XeroSkillDiscoveryDiagnostic,
) -> JsonValue {
    json!({
        "code": diagnostic.code,
        "message": diagnostic.message,
        "relativePath": diagnostic.relative_path
    })
}

fn skill_source_command_json<T>(
    kind: &'static str,
    message: &'static str,
    value: T,
) -> Result<JsonValue, CommandError>
where
    T: Serialize,
{
    Ok(json!({
        "kind": kind,
        "schema": "xero.skill_source_service_command.v1",
        "message": message,
        "result": serde_json::to_value(value).map_err(|error| {
            CommandError::system_fault(
                "xero_tui_skill_source_encode_failed",
                format!("Could not encode skill source command result: {error}"),
            )
        })?,
        "backend": "desktop_skill_source_settings_and_discovery",
        "uiDeferred": false
    }))
}

fn enabled_from_flags(args: &[String], default: bool) -> bool {
    if option_present(args, "--disabled") {
        return false;
    }
    option_value(args, "--enabled")
        .map(|value| {
            !matches!(
                value.to_ascii_lowercase().as_str(),
                "false" | "0" | "no" | "off"
            )
        })
        .unwrap_or(default)
}

#[derive(Debug, Default)]
struct AgentDefinitionCommandOptions {
    project_id: Option<String>,
    definition_id: Option<String>,
    source_definition_id: Option<String>,
    include_archived: bool,
    apply: bool,
    definition: Option<JsonValue>,
}

impl AgentDefinitionCommandOptions {
    fn from_args(args: &[String]) -> Result<Self, CommandError> {
        Ok(Self {
            project_id: option_value(args, "--project-id"),
            definition_id: option_value(args, "--definition-id"),
            source_definition_id: option_value(args, "--source-definition-id")
                .or_else(|| option_value(args, "--source-id")),
            include_archived: option_present(args, "--include-archived"),
            apply: option_present(args, "--apply")
                || option_present(args, "--operator-approved")
                || option_present(args, "--yes"),
            definition: definition_payload(args)?,
        })
    }
}

fn run_agent_definition_runtime(
    state_dir: &Path,
    args: &[String],
    action: AutonomousAgentDefinitionAction,
    options: AgentDefinitionCommandOptions,
) -> Result<JsonValue, CommandError> {
    let project_id = options
        .project_id
        .ok_or_else(|| CommandError::invalid_request("projectId"))?;
    configure_project_store_paths(state_dir);
    let repo_root = project_root(state_dir, &project_id)?;
    let runtime = AutonomousToolRuntime::new(&repo_root)?;
    let request = AutonomousAgentDefinitionRequest {
        action,
        definition_id: options.definition_id,
        source_definition_id: options.source_definition_id,
        include_archived: options.include_archived,
        definition: options.definition,
    };
    let result = if options.apply {
        runtime.agent_definition_with_operator_approval(request)?
    } else {
        runtime.agent_definition(request)?
    };
    let AutonomousToolOutput::AgentDefinition(output) = result.output else {
        return Err(CommandError::system_fault(
            "xero_tui_agent_definition_unexpected_output",
            "The shared autonomous definition service returned a non-definition output.",
        ));
    };
    let mut value = serde_json::to_value(output).map_err(|error| {
        CommandError::system_fault(
            "xero_tui_agent_definition_encode_failed",
            format!("Could not encode autonomous definition output: {error}"),
        )
    })?;
    normalize_agent_definition_output_json(&mut value);
    let Some(object) = value.as_object_mut() else {
        return Err(CommandError::system_fault(
            "xero_tui_agent_definition_output_invalid",
            "The shared autonomous definition service returned an invalid JSON output.",
        ));
    };
    object.insert("kind".into(), json!("agentDefinitionRuntime"));
    object.insert(
        "schema".into(),
        json!("xero.agent_definition_runtime_command.v1"),
    );
    object.insert("projectId".into(), json!(project_id));
    object.insert(
        "backend".into(),
        json!("desktop_autonomous_definition_service"),
    );
    object.insert("uiDeferred".into(), json!(false));
    object.insert(
        "operatorApprovalApplied".into(),
        json!(
            option_present(args, "--apply")
                || option_present(args, "--operator-approved")
                || option_present(args, "--yes")
        ),
    );
    Ok(value)
}

fn definition_payload(args: &[String]) -> Result<Option<JsonValue>, CommandError> {
    if let Some(inline) = option_value(args, "--definition-json") {
        return serde_json::from_str(&inline).map(Some).map_err(|error| {
            CommandError::user_fixable(
                "xero_tui_agent_definition_json_invalid",
                format!("Could not parse --definition-json as JSON: {error}"),
            )
        });
    }
    if let Some(path) = option_value(args, "--definition-file") {
        let text = fs::read_to_string(&path).map_err(|error| {
            CommandError::user_fixable(
                "xero_tui_agent_definition_file_read_failed",
                format!("Could not read agent definition file `{path}`: {error}"),
            )
        })?;
        return serde_json::from_str(&text).map(Some).map_err(|error| {
            CommandError::user_fixable(
                "xero_tui_agent_definition_file_json_invalid",
                format!("Could not parse agent definition file `{path}` as JSON: {error}"),
            )
        });
    }
    Ok(None)
}

fn normalize_agent_definition_output_json(value: &mut JsonValue) {
    if let Some(object) = value.as_object_mut() {
        if let Some(definition) = object.get_mut("definition") {
            normalize_agent_definition_summary_json(definition);
        }
        if let Some(definitions) = object
            .get_mut("definitions")
            .and_then(JsonValue::as_array_mut)
        {
            for definition in definitions {
                normalize_agent_definition_summary_json(definition);
            }
        }
    }
}

fn normalize_agent_definition_summary_json(value: &mut JsonValue) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if !object.contains_key("currentVersion") {
        if let Some(version) = object.get("version").cloned() {
            object.insert("currentVersion".into(), version);
        }
    }
    if !object.contains_key("definitionId") {
        if let Some(id) = object.get("id").cloned() {
            object.insert("definitionId".into(), id);
        }
    }
}

fn configure_project_store_paths(state_dir: &Path) {
    db::configure_project_database_paths(&state_dir.join(GLOBAL_DATABASE_FILE_NAME));
}

fn project_root(state_dir: &Path, project_id: &str) -> Result<PathBuf, CommandError> {
    let registry_path = state_dir.join(GLOBAL_DATABASE_FILE_NAME);
    registry::read_project_records(&registry_path, project_id)?
        .into_iter()
        .next()
        .map(|record| PathBuf::from(record.root_path))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "xero_tui_project_unknown",
                format!(
                    "Project `{project_id}` is not registered in Xero app-data at `{}`.",
                    registry_path.display()
                ),
            )
        })
}

fn project_record_summary_json(record: project_store::ProjectRecordRecord) -> JsonValue {
    let visible = record.redaction_state == project_store::ProjectRecordRedactionState::Clean;
    json!({
        "recordId": record.record_id,
        "recordKind": project_store::project_record_kind_sql_value(&record.record_kind),
        "title": record.title,
        "summary": visible.then(|| text_preview(&record.summary)),
        "textPreview": visible.then(|| text_preview(&record.text)),
        "importance": project_record_importance_label(&record.importance),
        "redactionState": project_record_redaction_label(&record.redaction_state),
        "visibility": project_record_visibility_label(&record.visibility),
        "freshnessState": record.freshness_state,
        "tags": record.tags,
        "relatedPaths": record.related_paths,
        "supersedesId": record.supersedes_id,
        "supersededById": record.superseded_by_id,
        "invalidatedAt": record.invalidated_at,
        "runtimeAgentId": runtime_agent_id_label(record.runtime_agent_id),
        "agentDefinitionId": record.agent_definition_id,
        "agentDefinitionVersion": record.agent_definition_version,
        "runId": record.run_id,
        "createdAt": record.created_at,
        "updatedAt": record.updated_at,
    })
}

fn memory_summary_json(memory: project_store::AgentMemoryRecord) -> JsonValue {
    json!({
        "memoryId": memory.memory_id,
        "scope": agent_memory_scope_label(&memory.scope),
        "kind": agent_memory_kind_label(&memory.kind),
        "reviewState": agent_memory_review_state_label(&memory.review_state),
        "enabled": memory.enabled,
        "confidence": memory.confidence,
        "textPreview": text_preview(&memory.text),
        "textHash": memory.text_hash,
        "sourceRunId": memory.source_run_id,
        "sourceItemIds": memory.source_item_ids,
        "freshnessState": memory.freshness_state,
        "retrievalReason": project_store::agent_memory_retrieval_reason(&memory),
        "supersedesId": memory.supersedes_id,
        "supersededById": memory.superseded_by_id,
        "invalidatedAt": memory.invalidated_at,
        "createdAt": memory.created_at,
        "updatedAt": memory.updated_at,
    })
}

fn project_record_importance_label(value: &project_store::ProjectRecordImportance) -> &'static str {
    match value {
        project_store::ProjectRecordImportance::Low => "low",
        project_store::ProjectRecordImportance::Normal => "normal",
        project_store::ProjectRecordImportance::High => "high",
        project_store::ProjectRecordImportance::Critical => "critical",
    }
}

fn project_record_redaction_label(
    value: &project_store::ProjectRecordRedactionState,
) -> &'static str {
    match value {
        project_store::ProjectRecordRedactionState::Clean => "clean",
        project_store::ProjectRecordRedactionState::Redacted => "redacted",
        project_store::ProjectRecordRedactionState::Blocked => "blocked",
    }
}

fn project_record_visibility_label(value: &project_store::ProjectRecordVisibility) -> &'static str {
    match value {
        project_store::ProjectRecordVisibility::Workflow => "workflow",
        project_store::ProjectRecordVisibility::Retrieval => "retrieval",
        project_store::ProjectRecordVisibility::MemoryCandidate => "memory_candidate",
        project_store::ProjectRecordVisibility::Diagnostic => "diagnostic",
    }
}

fn agent_memory_scope_label(value: &project_store::AgentMemoryScope) -> &'static str {
    match value {
        project_store::AgentMemoryScope::Project => "project",
        project_store::AgentMemoryScope::Session => "session",
    }
}

fn agent_memory_kind_label(value: &project_store::AgentMemoryKind) -> &'static str {
    match value {
        project_store::AgentMemoryKind::ProjectFact => "project_fact",
        project_store::AgentMemoryKind::UserPreference => "user_preference",
        project_store::AgentMemoryKind::Decision => "decision",
        project_store::AgentMemoryKind::SessionSummary => "session_summary",
        project_store::AgentMemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn agent_memory_review_state_label(value: &project_store::AgentMemoryReviewState) -> &'static str {
    match value {
        project_store::AgentMemoryReviewState::Candidate => "candidate",
        project_store::AgentMemoryReviewState::Approved => "approved",
        project_store::AgentMemoryReviewState::Rejected => "rejected",
    }
}

fn runtime_agent_id_label(value: RuntimeAgentIdDto) -> &'static str {
    match value {
        RuntimeAgentIdDto::Ask => "ask",
        RuntimeAgentIdDto::Plan => "plan",
        RuntimeAgentIdDto::Engineer => "engineer",
        RuntimeAgentIdDto::Debug => "debug",
        RuntimeAgentIdDto::Crawl => "crawl",
        RuntimeAgentIdDto::AgentCreate => "agent_create",
        RuntimeAgentIdDto::Generalist => "generalist",
    }
}

fn text_preview(value: &str) -> String {
    value.chars().take(240).collect()
}

fn required_option(
    args: &[String],
    name: &'static str,
    field: &'static str,
) -> Result<String, CommandError> {
    option_value(args, name).ok_or_else(|| CommandError::invalid_request(field))
}

fn positional_or_option(
    args: &[String],
    name: &'static str,
    field: &'static str,
) -> Result<String, CommandError> {
    option_value(args, name)
        .or_else(|| first_positional(args))
        .ok_or_else(|| CommandError::invalid_request(field))
}

fn option_value(args: &[String], name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    args.iter().enumerate().find_map(|(index, arg)| {
        if let Some(value) = arg.strip_prefix(&prefix) {
            return Some(value.to_string());
        }
        if arg == name {
            return args.get(index + 1).cloned();
        }
        None
    })
}

fn option_present(args: &[String], name: &str) -> bool {
    args.iter().any(|arg| arg == name)
}

fn option_values(args: &[String], name: &str) -> Vec<String> {
    let prefix = format!("{name}=");
    let mut values = Vec::new();
    let mut index = 0usize;
    while let Some(arg) = args.get(index) {
        if let Some(value) = arg.strip_prefix(&prefix) {
            values.push(value.to_string());
            index += 1;
            continue;
        }
        if arg == name {
            if let Some(value) = args.get(index + 1) {
                values.push(value.clone());
            }
            index += 2;
            continue;
        }
        index += 1;
    }
    values
}

fn option_u64(args: &[String], name: &str) -> Result<Option<u64>, CommandError> {
    option_value(args, name)
        .map(|value| {
            value.parse::<u64>().map_err(|error| {
                CommandError::user_fixable(
                    "xero_tui_number_invalid",
                    format!("{name} must be an unsigned integer: {error}"),
                )
            })
        })
        .transpose()
}

fn option_usize(args: &[String], name: &str) -> Result<Option<usize>, CommandError> {
    option_value(args, name)
        .map(|value| {
            value.parse::<usize>().map_err(|error| {
                CommandError::user_fixable(
                    "xero_tui_number_invalid",
                    format!("{name} must be an unsigned integer: {error}"),
                )
            })
        })
        .transpose()
}

fn option_u16(args: &[String], name: &str) -> Result<Option<u16>, CommandError> {
    option_value(args, name)
        .map(|value| {
            value.parse::<u16>().map_err(|error| {
                CommandError::user_fixable(
                    "xero_tui_number_invalid",
                    format!("{name} must be a terminal-size integer: {error}"),
                )
            })
        })
        .transpose()
}

fn option_i64(args: &[String], name: &str) -> Result<Option<i64>, CommandError> {
    option_value(args, name)
        .map(|value| {
            value.parse::<i64>().map_err(|error| {
                CommandError::user_fixable(
                    "xero_tui_number_invalid",
                    format!("{name} must be an integer: {error}"),
                )
            })
        })
        .transpose()
}

fn first_positional(args: &[String]) -> Option<String> {
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with("--") {
            skip_next = !arg.contains('=');
            continue;
        }
        return Some(arg.clone());
    }
    None
}

fn matches_normalized(value: &str, expected: &str) -> bool {
    value
        .trim()
        .replace('-', "_")
        .eq_ignore_ascii_case(expected)
}

fn string_field(value: &JsonValue, key: &str) -> String {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_string()
}

fn cli_error(error: CommandError) -> CliError {
    let exit_code = match error.class {
        CommandErrorClass::UserFixable | CommandErrorClass::PolicyDenied => 1,
        CommandErrorClass::Retryable | CommandErrorClass::SystemFault => 1,
    };
    CliError {
        code: error.code,
        message: error.message,
        exit_code,
    }
}
