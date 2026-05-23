use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        validate_non_empty, AgentDefaultModelDto, AgentRefDto, CommandError, CommandResult,
        SetAgentDefaultModelRequestDto, SetAgentDefaultModelResponseDto,
    },
    db::project_store,
    global_db::open_global_database,
    runtime::autonomous_tool_runtime::{
        AutonomousAgentDefinitionAction, AutonomousAgentDefinitionRequest, AutonomousToolOutput,
        AutonomousToolRuntime,
    },
    state::DesktopState,
};

use super::{agent_definition::write_response_from_output, runtime_support::resolve_project_root};

#[tauri::command]
pub fn set_agent_default_model<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SetAgentDefaultModelRequestDto,
) -> CommandResult<SetAgentDefaultModelResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    if let Some(default_model) = request.default_model.as_ref() {
        validate_agent_default_model(default_model)?;
    }

    match request.r#ref {
        AgentRefDto::BuiltIn {
            runtime_agent_id, ..
        } => {
            let db_path = state.global_db_path(&app)?;
            upsert_builtin_agent_default_model(&db_path, runtime_agent_id, request.default_model)?;
            Ok(SetAgentDefaultModelResponseDto {
                default_model: load_builtin_agent_default_model(&db_path, runtime_agent_id)?,
            })
        }
        AgentRefDto::Custom { definition_id, .. } => {
            let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
            let current = project_store::load_agent_definition(&repo_root, &definition_id)?
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "agent_definition_not_found",
                        format!("Xero could not find agent definition `{definition_id}`."),
                    )
                })?;
            let version = project_store::load_agent_definition_version(
                &repo_root,
                &definition_id,
                current.current_version,
            )?
            .ok_or_else(|| {
                CommandError::system_fault(
                    "agent_definition_version_missing",
                    format!(
                        "Xero resolved `{definition_id}` but could not load version {}.",
                        current.current_version
                    ),
                )
            })?;
            let mut snapshot = version.snapshot;
            let object = snapshot.as_object_mut().ok_or_else(|| {
                CommandError::system_fault(
                    "agent_definition_snapshot_invalid",
                    format!("Agent definition `{definition_id}` snapshot is not an object."),
                )
            })?;
            match request.default_model {
                Some(default_model) => {
                    object.insert(
                        "defaultModel".into(),
                        serde_json::to_value(default_model.clone()).map_err(|error| {
                            CommandError::system_fault(
                                "agent_default_model_serialize_failed",
                                format!(
                                    "Xero could not serialize the agent default model: {error}"
                                ),
                            )
                        })?,
                    );
                }
                None => {
                    object.remove("defaultModel");
                }
            }

            let runtime = AutonomousToolRuntime::new(&repo_root)?;
            let output = runtime.agent_definition_with_operator_approval(
                AutonomousAgentDefinitionRequest {
                    action: AutonomousAgentDefinitionAction::Update,
                    definition_id: Some(definition_id),
                    source_definition_id: None,
                    include_archived: false,
                    definition: Some(snapshot),
                },
            )?;
            let output = match output.output {
                AutonomousToolOutput::AgentDefinition(value) => value,
                _ => {
                    return Err(CommandError::system_fault(
                        "agent_default_model_unexpected_output",
                        "Xero received an unexpected runtime response while saving the agent default model.",
                    ));
                }
            };
            let response = write_response_from_output(&repo_root, output)?;
            if !response.applied {
                return Err(CommandError::user_fixable(
                    "agent_default_model_save_rejected",
                    response.message,
                ));
            }
            Ok(SetAgentDefaultModelResponseDto {
                default_model: response.summary.and_then(|summary| summary.default_model),
            })
        }
    }
}

pub(crate) fn load_builtin_agent_default_model(
    database_path: &std::path::Path,
    runtime_agent_id: crate::commands::RuntimeAgentIdDto,
) -> CommandResult<Option<AgentDefaultModelDto>> {
    let connection = open_global_database(database_path)?;
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM builtin_agent_default_models WHERE runtime_agent_id = ?1",
            params![runtime_agent_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            CommandError::retryable(
                "builtin_agent_default_model_read_failed",
                format!("Xero could not read the built-in agent default model: {error}"),
            )
        })?;
    payload
        .map(|payload| {
            serde_json::from_str(&payload).map_err(|error| {
                CommandError::system_fault(
                    "builtin_agent_default_model_decode_failed",
                    format!(
                        "Xero could not decode the saved built-in agent default model: {error}"
                    ),
                )
            })
        })
        .transpose()
}

fn upsert_builtin_agent_default_model(
    database_path: &std::path::Path,
    runtime_agent_id: crate::commands::RuntimeAgentIdDto,
    default_model: Option<AgentDefaultModelDto>,
) -> CommandResult<()> {
    let connection = open_global_database(database_path)?;
    match default_model {
        Some(default_model) => {
            let now = now_timestamp();
            let payload = serde_json::to_string(&default_model).map_err(|error| {
                CommandError::system_fault(
                    "builtin_agent_default_model_serialize_failed",
                    format!("Xero could not serialize the built-in agent default model: {error}"),
                )
            })?;
            connection
                .execute(
                    r#"
                    INSERT INTO builtin_agent_default_models (
                        runtime_agent_id, payload, created_at, updated_at
                    )
                    VALUES (?1, ?2, ?3, ?3)
                    ON CONFLICT(runtime_agent_id) DO UPDATE SET
                        payload = excluded.payload,
                        updated_at = excluded.updated_at
                    "#,
                    params![runtime_agent_id.as_str(), payload, now],
                )
                .map_err(|error| {
                    CommandError::retryable(
                        "builtin_agent_default_model_write_failed",
                        format!("Xero could not save the built-in agent default model: {error}"),
                    )
                })?;
        }
        None => {
            connection
                .execute(
                    "DELETE FROM builtin_agent_default_models WHERE runtime_agent_id = ?1",
                    params![runtime_agent_id.as_str()],
                )
                .map_err(|error| {
                    CommandError::retryable(
                        "builtin_agent_default_model_reset_failed",
                        format!("Xero could not reset the built-in agent default model: {error}"),
                    )
                })?;
        }
    }
    Ok(())
}

fn validate_agent_default_model(default_model: &AgentDefaultModelDto) -> CommandResult<()> {
    validate_non_empty(&default_model.provider_id, "defaultModel.providerId")?;
    validate_non_empty(&default_model.model_id, "defaultModel.modelId")?;
    if let Some(profile_id) = default_model.provider_profile_id.as_deref() {
        validate_non_empty(profile_id, "defaultModel.providerProfileId")?;
    }
    if let Some(selection_key) = default_model.selection_key.as_deref() {
        validate_non_empty(selection_key, "defaultModel.selectionKey")?;
    }
    Ok(())
}
