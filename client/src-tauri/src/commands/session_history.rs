use std::{collections::HashMap, fs, path::Path, path::PathBuf};

use rusqlite::{params, Connection};
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        context_budget_with_source, estimate_tokens, evaluate_compaction_policy,
        provider_context_budget_tokens, redact_session_context_text,
        run_transcript_from_agent_snapshot, session_transcript_from_runs,
        usage_totals_from_agent_usage, validate_context_snapshot_contract,
        validate_export_payload_contract, validate_session_transcript_contract, CommandError,
        CommandResult, ExportSessionTranscriptRequestDto, GetSessionContextSnapshotRequestDto,
        GetSessionTranscriptRequestDto, SaveSessionTranscriptExportRequestDto,
        SearchSessionTranscriptsRequestDto, SearchSessionTranscriptsResponseDto,
        SessionCompactionPolicyInput, SessionContextContributorDto,
        SessionContextContributorKindDto, SessionContextRedactionClassDto,
        SessionContextRedactionDto, SessionContextSnapshotDto, SessionTranscriptDto,
        SessionTranscriptExportFormatDto, SessionTranscriptExportPayloadDto,
        SessionTranscriptExportResponseDto, SessionTranscriptItemDto, SessionTranscriptScopeDto,
        SessionTranscriptSearchResultSnippetDto, SessionUsageSourceDto, SessionUsageTotalsDto,
        CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
    },
    db::project_store::{self, AgentMessageRole, AgentRunSnapshotRecord, AgentSessionRecord},
    runtime::{
        agent_core::{runtime_controls_from_request, tool_registry_for_snapshot},
        AgentToolDescriptor,
    },
    state::DesktopState,
};

use super::{runtime_support::resolve_project_root, validate_non_empty};

const DEFAULT_SEARCH_LIMIT: usize = 25;
const MAX_SEARCH_LIMIT: usize = 100;
const MAX_FALLBACK_SNIPPET_CHARS: usize = 220;
const CONTEXT_PREVIEW_CHARS: usize = 600;
const UNAVAILABLE_CONTEXT_ID: &str = "unavailable";

#[tauri::command]
pub fn get_session_transcript<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetSessionTranscriptRequestDto,
) -> CommandResult<SessionTranscriptDto> {
    validate_transcript_request(
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
    )?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    build_session_transcript(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
    )
}

#[tauri::command]
pub fn export_session_transcript<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ExportSessionTranscriptRequestDto,
) -> CommandResult<SessionTranscriptExportResponseDto> {
    validate_transcript_request(
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
    )?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let transcript = build_session_transcript(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
    )?;
    let generated_at = now_timestamp();
    let scope = if request.run_id.is_some() {
        SessionTranscriptScopeDto::Run
    } else {
        SessionTranscriptScopeDto::Session
    };
    let payload = SessionTranscriptExportPayloadDto {
        contract_version: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
        export_id: format!(
            "session-export:{}:{}:{}",
            transcript.project_id, transcript.agent_session_id, generated_at
        ),
        generated_at,
        scope,
        format: request.format.clone(),
        transcript,
        context_snapshot: None,
        redaction: SessionContextRedactionDto::public(),
    };
    validate_export_payload_contract(&payload).map_err(|details| {
        CommandError::system_fault(
            "session_transcript_export_invalid",
            format!("Cadence could not create a valid transcript export: {details}"),
        )
    })?;

    let (content, mime_type, extension) = match request.format {
        SessionTranscriptExportFormatDto::Json => (
            serde_json::to_string_pretty(&payload).map_err(|error| {
                CommandError::system_fault(
                    "session_transcript_export_serialize_failed",
                    format!("Cadence could not serialize the session transcript export: {error}"),
                )
            })?,
            "application/json".to_string(),
            "json",
        ),
        SessionTranscriptExportFormatDto::Markdown => (
            render_markdown_export(&payload),
            "text/markdown".to_string(),
            "md",
        ),
    };

    Ok(SessionTranscriptExportResponseDto {
        suggested_file_name: suggested_export_file_name(&payload, extension),
        payload,
        content,
        mime_type,
    })
}

#[tauri::command]
pub fn save_session_transcript_export(
    request: SaveSessionTranscriptExportRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.path, "path")?;
    validate_non_empty(&request.content, "content")?;

    let path = PathBuf::from(request.path);
    if path.file_name().is_none() {
        return Err(CommandError::invalid_request("path"));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(CommandError::user_fixable(
                "session_transcript_export_parent_missing",
                format!(
                    "Cadence cannot save the transcript because `{}` does not exist.",
                    parent.display()
                ),
            ));
        }
    }

    fs::write(&path, request.content).map_err(|error| {
        CommandError::retryable(
            "session_transcript_export_write_failed",
            format!(
                "Cadence could not write the transcript export to `{}`: {error}",
                path.display()
            ),
        )
    })?;
    Ok(())
}

#[tauri::command]
pub fn search_session_transcripts<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SearchSessionTranscriptsRequestDto,
) -> CommandResult<SearchSessionTranscriptsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.query, "query")?;
    if let Some(agent_session_id) = request.agent_session_id.as_deref() {
        validate_non_empty(agent_session_id, "agentSessionId")?;
    }
    if let Some(run_id) = request.run_id.as_deref() {
        validate_non_empty(run_id, "runId")?;
    }

    let limit = request
        .limit
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let rows = build_search_rows(&repo_root, &request, limit.saturating_mul(4))?;
    let mut results = search_rows_with_sqlite(&request.query, &rows, limit)
        .unwrap_or_else(|_| search_rows_fallback(&request.query, &rows, limit));
    for (index, result) in results.iter_mut().enumerate() {
        result.rank = index as u32;
    }
    let total = results.len();
    let truncated = total >= limit && rows.len() > limit;

    Ok(SearchSessionTranscriptsResponseDto {
        project_id: request.project_id,
        query: request.query,
        results,
        total,
        truncated,
    })
}

#[tauri::command]
pub fn get_session_context_snapshot<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetSessionContextSnapshotRequestDto,
) -> CommandResult<SessionContextSnapshotDto> {
    validate_transcript_request(
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
    )?;
    if let Some(provider_id) = request.provider_id.as_deref() {
        validate_non_empty(provider_id, "providerId")?;
    }
    if let Some(model_id) = request.model_id.as_deref() {
        validate_non_empty(model_id, "modelId")?;
    }

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    build_session_context_snapshot(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
        request.provider_id.as_deref(),
        request.model_id.as_deref(),
        request.pending_prompt.as_deref(),
    )
}

fn validate_transcript_request(
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
) -> CommandResult<()> {
    validate_non_empty(project_id, "projectId")?;
    validate_non_empty(agent_session_id, "agentSessionId")?;
    if let Some(run_id) = run_id {
        validate_non_empty(run_id, "runId")?;
    }
    Ok(())
}

fn build_session_transcript(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
) -> CommandResult<SessionTranscriptDto> {
    let session = project_store::get_agent_session(repo_root, project_id, agent_session_id)?
        .ok_or_else(|| missing_session_error(project_id, agent_session_id))?;
    let runs = if let Some(run_id) = run_id {
        let snapshot = project_store::load_agent_run(repo_root, project_id, run_id)?;
        ensure_run_belongs_to_session(&snapshot, project_id, agent_session_id)?;
        let usage = project_store::load_agent_usage(repo_root, project_id, run_id)?;
        vec![run_transcript_from_agent_snapshot(
            &snapshot,
            usage.as_ref(),
        )]
    } else {
        project_store::load_agent_session_run_snapshots(repo_root, project_id, agent_session_id)?
            .into_iter()
            .map(|(snapshot, usage)| run_transcript_from_agent_snapshot(&snapshot, usage.as_ref()))
            .collect()
    };

    let transcript = session_transcript_from_runs(&session, runs);
    validate_session_transcript_contract(&transcript).map_err(|details| {
        CommandError::system_fault(
            "session_transcript_invalid",
            format!("Cadence projected an invalid session transcript: {details}"),
        )
    })?;
    Ok(transcript)
}

fn build_session_context_snapshot(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
    request_provider_id: Option<&str>,
    request_model_id: Option<&str>,
    pending_prompt: Option<&str>,
) -> CommandResult<SessionContextSnapshotDto> {
    let session = project_store::get_agent_session(repo_root, project_id, agent_session_id)?
        .ok_or_else(|| missing_session_error(project_id, agent_session_id))?;
    let snapshots = load_context_snapshots(repo_root, project_id, agent_session_id, run_id)?;
    let latest_snapshot = snapshots
        .iter()
        .map(|(snapshot, _)| snapshot)
        .max_by(|left, right| {
            left.run
                .started_at
                .cmp(&right.run.started_at)
                .then_with(|| left.run.run_id.cmp(&right.run.run_id))
        });
    let provider_id = latest_snapshot
        .map(|snapshot| snapshot.run.provider_id.as_str())
        .or_else(|| request_provider_id)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(session.last_provider_id.as_deref())
        .unwrap_or(UNAVAILABLE_CONTEXT_ID)
        .to_string();
    let model_id = latest_snapshot
        .map(|snapshot| snapshot.run.model_id.as_str())
        .or_else(|| request_model_id)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(UNAVAILABLE_CONTEXT_ID)
        .to_string();
    let generated_at = now_timestamp();
    let instruction_text = read_instruction_file(repo_root);
    let mut contributors = Vec::new();

    if let Some(snapshot) = latest_snapshot {
        append_system_prompt_contributor(
            &mut contributors,
            project_id,
            agent_session_id,
            snapshot,
            instruction_text.as_deref(),
        );
    }
    append_instruction_file_contributor(
        &mut contributors,
        project_id,
        agent_session_id,
        instruction_text.as_deref(),
    );
    if let Some(snapshot) = latest_snapshot {
        append_tool_descriptor_contributors(
            &mut contributors,
            repo_root,
            project_id,
            agent_session_id,
            snapshot,
        )?;
    }
    append_history_contributors(&mut contributors, project_id, agent_session_id, &snapshots);
    append_usage_contributors(&mut contributors, project_id, agent_session_id, &snapshots);
    append_pending_prompt_contributor(
        &mut contributors,
        project_id,
        agent_session_id,
        run_id.or_else(|| latest_snapshot.map(|snapshot| snapshot.run.run_id.as_str())),
        pending_prompt,
    );

    let usage_totals = context_usage_totals(&session, &snapshots, run_id);
    let estimated_tokens = contributors
        .iter()
        .filter(|contributor| contributor.included && contributor.model_visible)
        .fold(0_u64, |total, contributor| {
            total.saturating_add(contributor.estimated_tokens)
        });
    let budget_tokens = provider_context_budget_tokens(&provider_id, &model_id);
    let estimation_source = if usage_totals.is_some() {
        SessionUsageSourceDto::Mixed
    } else if contributors.is_empty() {
        SessionUsageSourceDto::Unavailable
    } else {
        SessionUsageSourceDto::Estimated
    };
    let budget = context_budget_with_source(estimated_tokens, budget_tokens, estimation_source);
    let policy_decisions = vec![evaluate_compaction_policy(SessionCompactionPolicyInput {
        manual_requested: false,
        auto_enabled: false,
        provider_supports_compaction: false,
        active_compaction_present: false,
        estimated_tokens,
        budget_tokens,
        threshold_percent: Some(80),
    })];
    let redaction = strongest_context_redaction(
        contributors
            .iter()
            .map(|contributor| &contributor.redaction)
            .chain(policy_decisions.iter().map(|decision| &decision.redaction)),
    );
    let snapshot = SessionContextSnapshotDto {
        contract_version: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
        snapshot_id: format!(
            "context:{}:{}:{}:{}",
            project_id,
            agent_session_id,
            run_id.unwrap_or("session"),
            generated_at
        ),
        project_id: project_id.into(),
        agent_session_id: agent_session_id.into(),
        run_id: run_id.map(ToOwned::to_owned),
        provider_id,
        model_id,
        generated_at,
        budget,
        contributors,
        policy_decisions,
        usage_totals,
        redaction,
    };

    validate_context_snapshot_contract(&snapshot).map_err(|details| {
        CommandError::system_fault(
            "session_context_snapshot_invalid",
            format!("Cadence projected an invalid context snapshot: {details}"),
        )
    })?;
    Ok(snapshot)
}

fn load_context_snapshots(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
) -> CommandResult<
    Vec<(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )>,
> {
    if let Some(run_id) = run_id {
        let snapshot = project_store::load_agent_run(repo_root, project_id, run_id)?;
        ensure_run_belongs_to_session(&snapshot, project_id, agent_session_id)?;
        let usage = project_store::load_agent_usage(repo_root, project_id, run_id)?;
        return Ok(vec![(snapshot, usage)]);
    }

    project_store::load_agent_session_run_snapshots(repo_root, project_id, agent_session_id)
}

fn read_instruction_file(repo_root: &Path) -> Option<String> {
    fs::read_to_string(repo_root.join("AGENTS.md"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn append_system_prompt_contributor(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    snapshot: &AgentRunSnapshotRecord,
    instruction_text: Option<&str>,
) {
    let system_prompt = system_prompt_without_instruction_text(
        snapshot.run.system_prompt.as_str(),
        instruction_text.unwrap_or_default(),
    );
    if system_prompt.trim().is_empty() {
        return;
    }

    append_context_contributor(
        contributors,
        ContextContributorParts {
            contributor_id: format!("system_prompt:{}", snapshot.run.run_id),
            kind: SessionContextContributorKindDto::SystemPrompt,
            label: "System prompt".into(),
            project_id: Some(project_id),
            agent_session_id: Some(agent_session_id),
            run_id: Some(snapshot.run.run_id.as_str()),
            source_id: Some("owned_agent_system_prompt"),
            raw_text: Some(system_prompt.as_str()),
            estimate_text: Some(system_prompt.as_str()),
            included: true,
            model_visible: true,
        },
    );
}

fn append_instruction_file_contributor(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    instruction_text: Option<&str>,
) {
    let Some(instruction_text) = instruction_text else {
        return;
    };
    append_context_contributor(
        contributors,
        ContextContributorParts {
            contributor_id: "instruction:AGENTS.md".into(),
            kind: SessionContextContributorKindDto::InstructionFile,
            label: "Project instructions".into(),
            project_id: Some(project_id),
            agent_session_id: Some(agent_session_id),
            run_id: None,
            source_id: Some("AGENTS.md"),
            raw_text: Some(instruction_text),
            estimate_text: Some(instruction_text),
            included: true,
            model_visible: true,
        },
    );
}

fn append_tool_descriptor_contributors(
    contributors: &mut Vec<SessionContextContributorDto>,
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    let controls = runtime_controls_from_request(None);
    let registry = tool_registry_for_snapshot(repo_root, snapshot, &controls, true)?;
    let mut descriptors = registry.into_descriptors();
    descriptors.sort_by(|left, right| left.name.cmp(&right.name));

    for descriptor in descriptors {
        let estimate_text = tool_descriptor_estimate_text(&descriptor)?;
        append_context_contributor(
            contributors,
            ContextContributorParts {
                contributor_id: format!("tool_descriptor:{}", descriptor.name),
                kind: SessionContextContributorKindDto::ToolDescriptor,
                label: format!("Tool descriptor: {}", descriptor.name),
                project_id: Some(project_id),
                agent_session_id: Some(agent_session_id),
                run_id: Some(snapshot.run.run_id.as_str()),
                source_id: Some(descriptor.name.as_str()),
                raw_text: Some(descriptor.description.as_str()),
                estimate_text: Some(estimate_text.as_str()),
                included: true,
                model_visible: true,
            },
        );
    }
    Ok(())
}

fn tool_descriptor_estimate_text(descriptor: &AgentToolDescriptor) -> CommandResult<String> {
    serde_json::to_string(descriptor).map_err(|error| {
        CommandError::system_fault(
            "session_context_tool_descriptor_serialize_failed",
            format!("Cadence could not estimate a tool descriptor context contribution: {error}"),
        )
    })
}

fn append_history_contributors(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    snapshots: &[(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )],
) {
    let mut sorted = snapshots
        .iter()
        .map(|(snapshot, _)| snapshot)
        .collect::<Vec<_>>();
    sorted.sort_by(|left, right| {
        left.run
            .started_at
            .cmp(&right.run.started_at)
            .then_with(|| left.run.run_id.cmp(&right.run.run_id))
    });

    for snapshot in sorted {
        append_run_prompt_contributor_if_needed(
            contributors,
            project_id,
            agent_session_id,
            snapshot,
        );
        let mut messages = snapshot.messages.iter().collect::<Vec<_>>();
        messages.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        for message in messages {
            match message.role {
                AgentMessageRole::System => {}
                AgentMessageRole::Tool => {
                    let message_id = message.id.to_string();
                    append_context_contributor(
                        contributors,
                        ContextContributorParts {
                            contributor_id: format!(
                                "tool_result:{}:{}",
                                message.run_id, message.id
                            ),
                            kind: SessionContextContributorKindDto::ToolResult,
                            label: tool_result_label(&message.content),
                            project_id: Some(project_id),
                            agent_session_id: Some(agent_session_id),
                            run_id: Some(message.run_id.as_str()),
                            source_id: Some(message_id.as_str()),
                            raw_text: Some(message.content.as_str()),
                            estimate_text: Some(message.content.as_str()),
                            included: true,
                            model_visible: true,
                        },
                    );
                }
                AgentMessageRole::Developer
                | AgentMessageRole::User
                | AgentMessageRole::Assistant => {
                    let message_id = message.id.to_string();
                    append_context_contributor(
                        contributors,
                        ContextContributorParts {
                            contributor_id: format!("message:{}:{}", message.run_id, message.id),
                            kind: SessionContextContributorKindDto::ConversationTail,
                            label: format!("{} message", message_role_label(&message.role)),
                            project_id: Some(project_id),
                            agent_session_id: Some(agent_session_id),
                            run_id: Some(message.run_id.as_str()),
                            source_id: Some(message_id.as_str()),
                            raw_text: Some(message.content.as_str()),
                            estimate_text: Some(message.content.as_str()),
                            included: true,
                            model_visible: true,
                        },
                    );
                }
            }
        }
    }
}

fn append_run_prompt_contributor_if_needed(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    snapshot: &AgentRunSnapshotRecord,
) {
    if snapshot.run.prompt.trim().is_empty() {
        return;
    }
    let prompt_in_messages = snapshot.messages.iter().any(|message| {
        matches!(
            message.role,
            AgentMessageRole::Developer | AgentMessageRole::User
        ) && message.content == snapshot.run.prompt
    });
    if prompt_in_messages {
        return;
    }

    append_context_contributor(
        contributors,
        ContextContributorParts {
            contributor_id: format!("run_prompt:{}", snapshot.run.run_id),
            kind: SessionContextContributorKindDto::ConversationTail,
            label: "Run prompt".into(),
            project_id: Some(project_id),
            agent_session_id: Some(agent_session_id),
            run_id: Some(snapshot.run.run_id.as_str()),
            source_id: Some(snapshot.run.run_id.as_str()),
            raw_text: Some(snapshot.run.prompt.as_str()),
            estimate_text: Some(snapshot.run.prompt.as_str()),
            included: true,
            model_visible: true,
        },
    );
}

fn append_usage_contributors(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    snapshots: &[(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )],
) {
    for (snapshot, usage) in snapshots {
        let Some(usage) = usage else {
            continue;
        };
        let text = format!(
            "{} input + {} output = {} total tokens. Estimated cost: {} micros.",
            usage.input_tokens,
            usage.output_tokens,
            usage.total_tokens,
            usage.estimated_cost_micros
        );
        append_context_contributor(
            contributors,
            ContextContributorParts {
                contributor_id: format!("provider_usage:{}", snapshot.run.run_id),
                kind: SessionContextContributorKindDto::ProviderUsage,
                label: "Provider usage".into(),
                project_id: Some(project_id),
                agent_session_id: Some(agent_session_id),
                run_id: Some(snapshot.run.run_id.as_str()),
                source_id: Some(snapshot.run.run_id.as_str()),
                raw_text: Some(text.as_str()),
                estimate_text: None,
                included: false,
                model_visible: false,
            },
        );
    }
}

fn append_pending_prompt_contributor(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
    pending_prompt: Option<&str>,
) {
    let Some(pending_prompt) = pending_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    append_context_contributor(
        contributors,
        ContextContributorParts {
            contributor_id: "pending_prompt".into(),
            kind: SessionContextContributorKindDto::ConversationTail,
            label: "Pending prompt".into(),
            project_id: Some(project_id),
            agent_session_id: Some(agent_session_id),
            run_id,
            source_id: Some("pending_prompt"),
            raw_text: Some(pending_prompt),
            estimate_text: Some(pending_prompt),
            included: true,
            model_visible: true,
        },
    );
}

fn context_usage_totals(
    session: &AgentSessionRecord,
    snapshots: &[(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )],
    run_id: Option<&str>,
) -> Option<SessionUsageTotalsDto> {
    if run_id.is_some() {
        return snapshots
            .first()
            .and_then(|(_, usage)| usage.as_ref())
            .map(usage_totals_from_agent_usage);
    }

    let run_transcripts = snapshots
        .iter()
        .map(|(snapshot, usage)| run_transcript_from_agent_snapshot(snapshot, usage.as_ref()))
        .collect::<Vec<_>>();
    session_transcript_from_runs(session, run_transcripts).usage_totals
}

fn system_prompt_without_instruction_text(system_prompt: &str, instruction_text: &str) -> String {
    let trimmed_prompt = system_prompt.trim();
    let trimmed_instruction = instruction_text.trim();
    if trimmed_prompt.is_empty() || trimmed_instruction.is_empty() {
        return trimmed_prompt.to_string();
    }

    trimmed_prompt
        .replacen(
            trimmed_instruction,
            "(project instructions counted separately)",
            1,
        )
        .trim()
        .to_string()
}

fn tool_result_label(content: &str) -> String {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|value| {
            value
                .get("toolName")
                .or_else(|| value.get("tool_name"))
                .and_then(|tool_name| tool_name.as_str())
                .map(|tool_name| format!("Tool result: {tool_name}"))
        })
        .unwrap_or_else(|| "Tool result".into())
}

fn message_role_label(role: &AgentMessageRole) -> &'static str {
    match role {
        AgentMessageRole::System => "System",
        AgentMessageRole::Developer => "Developer",
        AgentMessageRole::User => "User",
        AgentMessageRole::Assistant => "Assistant",
        AgentMessageRole::Tool => "Tool",
    }
}

struct ContextContributorParts<'a> {
    contributor_id: String,
    kind: SessionContextContributorKindDto,
    label: String,
    project_id: Option<&'a str>,
    agent_session_id: Option<&'a str>,
    run_id: Option<&'a str>,
    source_id: Option<&'a str>,
    raw_text: Option<&'a str>,
    estimate_text: Option<&'a str>,
    included: bool,
    model_visible: bool,
}

fn append_context_contributor(
    contributors: &mut Vec<SessionContextContributorDto>,
    parts: ContextContributorParts<'_>,
) {
    let (text, redaction) = parts
        .raw_text
        .map(redact_session_context_text)
        .map(|(text, redaction)| (Some(preview_context_text(&text)), redaction))
        .unwrap_or_else(|| (None, SessionContextRedactionDto::public()));
    let char_text = parts.estimate_text.or(parts.raw_text).unwrap_or_default();
    contributors.push(SessionContextContributorDto {
        contributor_id: parts.contributor_id,
        kind: parts.kind,
        label: parts.label,
        project_id: parts.project_id.map(ToOwned::to_owned),
        agent_session_id: parts.agent_session_id.map(ToOwned::to_owned),
        run_id: parts.run_id.map(ToOwned::to_owned),
        source_id: parts.source_id.map(ToOwned::to_owned),
        sequence: contributors.len() as u64 + 1,
        estimated_tokens: parts.estimate_text.map(estimate_tokens).unwrap_or(0),
        estimated_chars: char_text.chars().count() as u64,
        included: parts.included,
        model_visible: parts.model_visible,
        text,
        redaction,
    });
}

fn preview_context_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= CONTEXT_PREVIEW_CHARS {
        return trimmed.to_string();
    }

    let mut preview = trimmed
        .chars()
        .take(CONTEXT_PREVIEW_CHARS)
        .collect::<String>();
    preview.push_str("...");
    preview
}

fn strongest_context_redaction<'a>(
    redactions: impl IntoIterator<Item = &'a SessionContextRedactionDto>,
) -> SessionContextRedactionDto {
    redactions
        .into_iter()
        .cloned()
        .reduce(|left, right| {
            if context_redaction_rank(&left.redaction_class)
                >= context_redaction_rank(&right.redaction_class)
            {
                left
            } else {
                right
            }
        })
        .unwrap_or_else(SessionContextRedactionDto::public)
}

fn context_redaction_rank(class: &SessionContextRedactionClassDto) -> u8 {
    match class {
        SessionContextRedactionClassDto::Public => 0,
        SessionContextRedactionClassDto::LocalPath => 1,
        SessionContextRedactionClassDto::Transcript => 2,
        SessionContextRedactionClassDto::RawPayload => 3,
        SessionContextRedactionClassDto::Secret => 4,
    }
}

fn ensure_run_belongs_to_session(
    snapshot: &AgentRunSnapshotRecord,
    project_id: &str,
    agent_session_id: &str,
) -> CommandResult<()> {
    if snapshot.run.project_id == project_id && snapshot.run.agent_session_id == agent_session_id {
        return Ok(());
    }
    Err(CommandError::user_fixable(
        "agent_run_session_mismatch",
        format!(
            "Cadence found owned-agent run `{}` but it does not belong to session `{agent_session_id}`.",
            snapshot.run.run_id
        ),
    ))
}

fn missing_session_error(project_id: &str, agent_session_id: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_session_not_found",
        format!(
            "Cadence could not find agent session `{agent_session_id}` for project `{project_id}`."
        ),
    )
}

fn render_markdown_export(payload: &SessionTranscriptExportPayloadDto) -> String {
    let transcript = &payload.transcript;
    let mut markdown = String::new();
    markdown.push_str(&format!("# {}\n\n", markdown_line(&transcript.title)));
    markdown.push_str(&format!("- Project: `{}`\n", transcript.project_id));
    markdown.push_str(&format!("- Session: `{}`\n", transcript.agent_session_id));
    markdown.push_str(&format!("- Status: `{:?}`\n", transcript.status));
    markdown.push_str(&format!("- Scope: `{:?}`\n", payload.scope));
    markdown.push_str(&format!("- Generated: `{}`\n", payload.generated_at));
    if let Some(usage) = transcript.usage_totals.as_ref() {
        markdown.push_str(&format!("- Tokens: `{}` total\n", usage.total_tokens));
    }
    if !transcript.summary.trim().is_empty() {
        markdown.push_str(&format!("\n{}\n", markdown_line(&transcript.summary)));
    }
    if transcript.runs.is_empty() {
        markdown.push_str("\n_No runs recorded for this session._\n");
        return markdown;
    }

    let mut items_by_run: HashMap<&str, Vec<&SessionTranscriptItemDto>> = HashMap::new();
    for item in &transcript.items {
        items_by_run
            .entry(item.run_id.as_str())
            .or_default()
            .push(item);
    }

    for run in &transcript.runs {
        markdown.push_str(&format!("\n## Run `{}`\n\n", run.run_id));
        markdown.push_str(&format!("- Provider: `{}`\n", run.provider_id));
        markdown.push_str(&format!("- Model: `{}`\n", run.model_id));
        markdown.push_str(&format!("- Status: `{}`\n", run.status));
        markdown.push_str(&format!("- Started: `{}`\n", run.started_at));
        if let Some(completed_at) = run.completed_at.as_ref() {
            markdown.push_str(&format!("- Completed: `{completed_at}`\n"));
        }
        if let Some(usage) = run.usage_totals.as_ref() {
            markdown.push_str(&format!("- Tokens: `{}` total\n", usage.total_tokens));
        }

        let items = items_by_run
            .get(run.run_id.as_str())
            .cloned()
            .unwrap_or_default();
        if items.is_empty() {
            markdown.push_str("\n_No transcript items recorded for this run._\n");
            continue;
        }
        for item in items {
            markdown.push_str(&format!(
                "\n### {}. {}\n\n",
                item.sequence,
                markdown_line(
                    item.title
                        .as_deref()
                        .unwrap_or_else(|| item_kind_label(item))
                )
            ));
            markdown.push_str(&format!(
                "- Kind: `{:?}` · Actor: `{:?}` · Created: `{}`\n",
                item.kind, item.actor, item.created_at
            ));
            if let Some(tool_name) = item.tool_name.as_ref() {
                markdown.push_str(&format!("- Tool: `{}`\n", markdown_line(tool_name)));
            }
            if let Some(file_path) = item.file_path.as_ref() {
                markdown.push_str(&format!("- File: `{}`\n", markdown_line(file_path)));
            }
            if let Some(checkpoint_kind) = item.checkpoint_kind.as_ref() {
                markdown.push_str(&format!(
                    "- Checkpoint: `{}`\n",
                    markdown_line(checkpoint_kind)
                ));
            }
            if let Some(text) = item.text.as_ref().filter(|value| !value.trim().is_empty()) {
                markdown.push_str(&format!("\n{}\n", markdown_line(text)));
            }
            if let Some(summary) = item
                .summary
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                markdown.push_str(&format!("\n> {}\n", markdown_line(summary)));
            }
        }
    }
    markdown
}

fn markdown_line(value: &str) -> String {
    value.replace('\r', "").trim().to_string()
}

fn item_kind_label(item: &SessionTranscriptItemDto) -> &'static str {
    match item.kind {
        crate::commands::SessionTranscriptItemKindDto::Message => "Message",
        crate::commands::SessionTranscriptItemKindDto::Reasoning => "Reasoning",
        crate::commands::SessionTranscriptItemKindDto::ToolCall => "Tool call",
        crate::commands::SessionTranscriptItemKindDto::ToolResult => "Tool result",
        crate::commands::SessionTranscriptItemKindDto::FileChange => "File change",
        crate::commands::SessionTranscriptItemKindDto::Checkpoint => "Checkpoint",
        crate::commands::SessionTranscriptItemKindDto::ActionRequest => "Action request",
        crate::commands::SessionTranscriptItemKindDto::Activity => "Activity",
        crate::commands::SessionTranscriptItemKindDto::Complete => "Run completed",
        crate::commands::SessionTranscriptItemKindDto::Failure => "Run failed",
        crate::commands::SessionTranscriptItemKindDto::Usage => "Usage",
    }
}

fn suggested_export_file_name(
    payload: &SessionTranscriptExportPayloadDto,
    extension: &str,
) -> String {
    let scope = match payload.scope {
        SessionTranscriptScopeDto::Run => payload
            .transcript
            .runs
            .first()
            .map(|run| format!("run-{}", sanitize_file_segment(&run.run_id)))
            .unwrap_or_else(|| "run".into()),
        SessionTranscriptScopeDto::Session => "session".into(),
    };
    format!(
        "{}-{}-transcript.{}",
        sanitize_file_segment(&payload.transcript.title),
        scope,
        extension
    )
}

fn sanitize_file_segment(value: &str) -> String {
    let segment = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if segment.is_empty() {
        "cadence".into()
    } else {
        segment.chars().take(64).collect()
    }
}

#[derive(Debug, Clone)]
struct SearchRow {
    result_id: String,
    project_id: String,
    agent_session_id: String,
    run_id: String,
    item_id: String,
    archived: bool,
    matched_fields: Vec<String>,
    content: String,
    redaction: SessionContextRedactionDto,
}

fn build_search_rows(
    repo_root: &Path,
    request: &SearchSessionTranscriptsRequestDto,
    _prefetch_hint: usize,
) -> CommandResult<Vec<SearchRow>> {
    let sessions = project_store::list_agent_sessions(
        repo_root,
        &request.project_id,
        request.include_archived,
    )?;
    let mut rows = Vec::new();
    for session in sessions {
        if let Some(agent_session_id) = request.agent_session_id.as_deref() {
            if session.agent_session_id != agent_session_id {
                continue;
            }
        }
        let snapshots = load_search_snapshots(
            repo_root,
            &request.project_id,
            &session,
            request.run_id.as_deref(),
        )?;
        let run_transcripts = snapshots
            .iter()
            .map(|(snapshot, usage)| run_transcript_from_agent_snapshot(snapshot, usage.as_ref()))
            .collect::<Vec<_>>();
        let transcript = session_transcript_from_runs(&session, run_transcripts);
        append_session_search_rows(&mut rows, &session, &transcript);
    }
    Ok(rows)
}

fn load_search_snapshots(
    repo_root: &Path,
    project_id: &str,
    session: &AgentSessionRecord,
    run_id: Option<&str>,
) -> CommandResult<
    Vec<(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )>,
> {
    if let Some(run_id) = run_id {
        let snapshot = match project_store::load_agent_run(repo_root, project_id, run_id) {
            Ok(snapshot) => snapshot,
            Err(error) if error.code == "agent_run_not_found" => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        if snapshot.run.agent_session_id != session.agent_session_id {
            return Ok(Vec::new());
        }
        let usage = project_store::load_agent_usage(repo_root, project_id, run_id)?;
        return Ok(vec![(snapshot, usage)]);
    }
    project_store::load_agent_session_run_snapshots(
        repo_root,
        project_id,
        &session.agent_session_id,
    )
}

fn append_session_search_rows(
    rows: &mut Vec<SearchRow>,
    session: &AgentSessionRecord,
    transcript: &SessionTranscriptDto,
) {
    let archived = transcript.archived;
    if !transcript.title.trim().is_empty() {
        rows.push(search_row(
            format!("session:{}:title", transcript.agent_session_id),
            &transcript.project_id,
            &transcript.agent_session_id,
            "session",
            "session:title",
            archived,
            vec!["title".into()],
            transcript.title.clone(),
            transcript.redaction.clone(),
        ));
    }
    if !transcript.summary.trim().is_empty() {
        rows.push(search_row(
            format!("session:{}:summary", transcript.agent_session_id),
            &transcript.project_id,
            &transcript.agent_session_id,
            "session",
            "session:summary",
            archived,
            vec!["summary".into()],
            transcript.summary.clone(),
            transcript.redaction.clone(),
        ));
    }

    for item in &transcript.items {
        let mut content_parts = Vec::new();
        let mut fields = Vec::new();
        push_search_part(
            &mut content_parts,
            &mut fields,
            "title",
            item.title.as_deref(),
        );
        push_search_part(
            &mut content_parts,
            &mut fields,
            "text",
            item.text.as_deref(),
        );
        push_search_part(
            &mut content_parts,
            &mut fields,
            "summary",
            item.summary.as_deref(),
        );
        push_search_part(
            &mut content_parts,
            &mut fields,
            "tool",
            item.tool_name.as_deref(),
        );
        push_search_part(
            &mut content_parts,
            &mut fields,
            "file",
            item.file_path.as_deref(),
        );
        push_search_part(
            &mut content_parts,
            &mut fields,
            "checkpoint",
            item.checkpoint_kind.as_deref(),
        );
        if content_parts.is_empty() {
            continue;
        }
        rows.push(search_row(
            format!("item:{}:{}", item.run_id, item.item_id),
            &item.project_id,
            &item.agent_session_id,
            &item.run_id,
            &item.item_id,
            archived,
            fields,
            content_parts.join("\n"),
            item.redaction.clone(),
        ));
    }

    if rows.is_empty() && session.status == project_store::AgentSessionStatus::Archived {
        let (title, redaction) = redact_session_context_text(&session.title);
        rows.push(search_row(
            format!("session:{}:archived", session.agent_session_id),
            &session.project_id,
            &session.agent_session_id,
            "session",
            "session:archived",
            true,
            vec!["title".into()],
            title,
            redaction,
        ));
    }
}

fn push_search_part(
    content_parts: &mut Vec<String>,
    fields: &mut Vec<String>,
    field: &str,
    value: Option<&str>,
) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    content_parts.push(value.to_string());
    if !fields.iter().any(|candidate| candidate == field) {
        fields.push(field.to_string());
    }
}

#[allow(clippy::too_many_arguments)]
fn search_row(
    result_id: String,
    project_id: &str,
    agent_session_id: &str,
    run_id: &str,
    item_id: &str,
    archived: bool,
    matched_fields: Vec<String>,
    content: String,
    redaction: SessionContextRedactionDto,
) -> SearchRow {
    SearchRow {
        result_id,
        project_id: project_id.into(),
        agent_session_id: agent_session_id.into(),
        run_id: run_id.into(),
        item_id: item_id.into(),
        archived,
        matched_fields,
        content,
        redaction,
    }
}

fn search_rows_with_sqlite(
    query: &str,
    rows: &[SearchRow],
    limit: usize,
) -> Result<Vec<SessionTranscriptSearchResultSnippetDto>, rusqlite::Error> {
    let fts_query = fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    let connection = Connection::open_in_memory()?;
    connection.execute_batch(
        r#"
        CREATE VIRTUAL TABLE transcript_search USING fts5(
            result_id UNINDEXED,
            project_id UNINDEXED,
            agent_session_id UNINDEXED,
            run_id UNINDEXED,
            item_id UNINDEXED,
            archived UNINDEXED,
            matched_fields UNINDEXED,
            content
        );
        "#,
    )?;
    {
        let mut insert = connection.prepare(
            r#"
            INSERT INTO transcript_search (
                result_id,
                project_id,
                agent_session_id,
                run_id,
                item_id,
                archived,
                matched_fields,
                content
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )?;
        for row in rows {
            insert.execute(params![
                row.result_id.as_str(),
                row.project_id.as_str(),
                row.agent_session_id.as_str(),
                row.run_id.as_str(),
                row.item_id.as_str(),
                if row.archived { 1 } else { 0 },
                row.matched_fields.join(","),
                row.content.as_str(),
            ])?;
        }
    }

    let mut redactions = HashMap::new();
    for row in rows {
        redactions.insert(row.result_id.as_str(), row.redaction.clone());
    }

    let mut statement = connection.prepare(
        r#"
        SELECT
            result_id,
            project_id,
            agent_session_id,
            run_id,
            item_id,
            archived,
            matched_fields,
            snippet(transcript_search, 7, '', '', '...', 16) AS snippet,
            bm25(transcript_search) AS rank
        FROM transcript_search
        WHERE transcript_search MATCH ?1
        ORDER BY rank ASC, rowid ASC
        LIMIT ?2
        "#,
    )?;
    let mapped = statement.query_map(params![fts_query, limit as i64], |row| {
        let result_id: String = row.get(0)?;
        let archived: i64 = row.get(5)?;
        let matched_fields: String = row.get(6)?;
        let snippet: String = row.get(7)?;
        Ok(SessionTranscriptSearchResultSnippetDto {
            contract_version: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
            result_id: result_id.clone(),
            project_id: row.get(1)?,
            agent_session_id: row.get(2)?,
            run_id: row.get(3)?,
            item_id: row.get(4)?,
            archived: archived == 1,
            rank: 0,
            matched_fields: split_matched_fields(&matched_fields),
            snippet: normalize_snippet(&snippet),
            redaction: redactions
                .get(result_id.as_str())
                .cloned()
                .unwrap_or_else(SessionContextRedactionDto::public),
        })
    })?;
    mapped.collect()
}

fn search_rows_fallback(
    query: &str,
    rows: &[SearchRow],
    limit: usize,
) -> Vec<SessionTranscriptSearchResultSnippetDto> {
    let normalized_query = query.trim().to_ascii_lowercase();
    if normalized_query.is_empty() {
        return Vec::new();
    }
    let mut results = rows
        .iter()
        .filter_map(|row| {
            let content = row.content.to_ascii_lowercase();
            let position = content.find(&normalized_query)?;
            Some((position, row))
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.archived.cmp(&right.1.archived))
            .then_with(|| left.1.result_id.cmp(&right.1.result_id))
    });
    results
        .into_iter()
        .take(limit)
        .map(|(_, row)| SessionTranscriptSearchResultSnippetDto {
            contract_version: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
            result_id: row.result_id.clone(),
            project_id: row.project_id.clone(),
            agent_session_id: row.agent_session_id.clone(),
            run_id: row.run_id.clone(),
            item_id: row.item_id.clone(),
            archived: row.archived,
            rank: 0,
            matched_fields: row.matched_fields.clone(),
            snippet: fallback_snippet(&row.content, &normalized_query),
            redaction: row.redaction.clone(),
        })
        .collect()
}

fn fts_query(query: &str) -> String {
    let terms = query
        .split_whitespace()
        .map(|term| {
            term.trim_matches(|character: char| {
                !character.is_alphanumeric() && character != '_' && character != '-'
            })
        })
        .filter(|term| !term.is_empty())
        .take(8)
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    terms.join(" AND ")
}

fn split_matched_fields(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_snippet(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "Matched transcript item.".into()
    } else {
        trimmed.replace('\n', " ")
    }
}

fn fallback_snippet(content: &str, query: &str) -> String {
    let lower = content.to_ascii_lowercase();
    let start = lower
        .find(query)
        .map(|index| index.saturating_sub(60))
        .unwrap_or(0);
    let snippet = content
        .chars()
        .skip(start)
        .take(MAX_FALLBACK_SNIPPET_CHARS)
        .collect::<String>()
        .replace('\n', " ");
    let trimmed = snippet.trim();
    if trimmed.is_empty() {
        "Matched transcript item.".into()
    } else {
        trimmed.to_string()
    }
}
