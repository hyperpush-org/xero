use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::Path,
    path::PathBuf,
};

use rand::RngCore;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        context_budget_with_source, estimate_tokens, evaluate_compaction_policy,
        memory_policy_decision, redact_session_context_text, resolve_context_limit,
        run_transcript_from_agent_snapshot, session_compaction_record_dto,
        session_memory_diagnostic_dto, session_memory_record_dto, session_transcript_from_runs,
        usage_totals_from_agent_usage, validate_context_snapshot_contract,
        validate_export_payload_contract, validate_session_compaction_record_contract,
        validate_session_memory_record_contract, validate_session_transcript_contract,
        AgentSessionBranchResponseDto, AgentSessionLineageBoundaryKindDto,
        BranchAgentSessionRequestDto, BrowserControlPreferenceDto, CommandError, CommandResult,
        CompactSessionHistoryRequestDto, CompactSessionHistoryResponseDto,
        DeleteSessionMemoryRequestDto, ExportSessionTranscriptRequestDto,
        ExtractSessionMemoryCandidatesRequestDto, ExtractSessionMemoryCandidatesResponseDto,
        GetSessionContextSnapshotRequestDto, GetSessionTranscriptRequestDto,
        ListSessionMemoriesRequestDto, ListSessionMemoriesResponseDto,
        RewindAgentSessionRequestDto, SaveSessionTranscriptExportRequestDto,
        SearchSessionTranscriptsRequestDto, SearchSessionTranscriptsResponseDto,
        SessionCompactionPolicyInput, SessionContextCodeMapDto, SessionContextCodeSymbolDto,
        SessionContextContributorDto, SessionContextContributorKindDto,
        SessionContextDependencyManifestDto, SessionContextDispositionDto,
        SessionContextRedactionClassDto, SessionContextRedactionDto, SessionContextSnapshotDiffDto,
        SessionContextSnapshotDto, SessionContextTaskPhaseDto, SessionMemoryDiagnosticDto,
        SessionMemoryRecordDto, SessionMemoryReviewStateDto, SessionTranscriptDto,
        SessionTranscriptExportFormatDto, SessionTranscriptExportPayloadDto,
        SessionTranscriptExportResponseDto, SessionTranscriptItemDto, SessionTranscriptScopeDto,
        SessionTranscriptSearchResultSnippetDto, SessionUsageSourceDto, SessionUsageTotalsDto,
        UpdateSessionMemoryRequestDto, XERO_SESSION_CONTEXT_CONTRACT_VERSION,
    },
    db::project_store::{
        self, AgentCompactionTrigger, AgentMemoryKind, AgentMemoryListFilter,
        AgentMemoryReviewState, AgentMemoryScope, AgentMessageRecord, AgentMessageRole,
        AgentRunSnapshotRecord, AgentSessionBranchBoundary, AgentSessionBranchCreateRecord,
        AgentSessionRecord, NewAgentCompactionRecord, NewAgentMemoryRecord,
    },
    runtime::{
        agent_core::{
            compile_system_prompt_for_session, create_provider_adapter,
            provider_messages_from_snapshot, runtime_controls_from_request,
            skill_contexts_from_provider_messages, tool_registry_for_snapshot, PromptCompilation,
            PromptFragment, ProviderAdapter, ProviderCompactionRequest, ProviderMemoryCandidate,
            ProviderMemoryExtractionRequest, ToolRegistry, ToolRegistryOptions,
        },
        AgentToolDescriptor,
    },
    state::DesktopState,
};

use super::{
    agent_session::{agent_session_dto, agent_session_lineage_dto},
    runtime_support::{resolve_owned_agent_provider_config, resolve_project_root},
    validate_non_empty,
};

const DEFAULT_SEARCH_LIMIT: usize = 25;
const MAX_SEARCH_LIMIT: usize = 100;
const MAX_FALLBACK_SNIPPET_CHARS: usize = 220;
const CONTEXT_PREVIEW_CHARS: usize = 600;
const UNAVAILABLE_CONTEXT_ID: &str = "unavailable";
const DEFAULT_RAW_TAIL_MESSAGE_COUNT: u32 = 8;
const MAX_RAW_TAIL_MESSAGE_COUNT: u32 = 24;
const MAX_COMPACTION_SUMMARY_TOKENS: u64 = 1_500;
const MAX_MEMORY_CANDIDATES: u8 = 8;
const MIN_MEMORY_CONFIDENCE: u8 = 50;
const MAX_CODE_MAP_FILES: usize = 240;
const MAX_CODE_SYMBOLS: usize = 160;
const LARGE_CONTEXT_NODE_TOKENS: u64 = 700;

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
        contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
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
            format!("Xero could not create a valid transcript export: {details}"),
        )
    })?;

    let (content, mime_type, extension) = match request.format {
        SessionTranscriptExportFormatDto::Json => (
            serde_json::to_string_pretty(&payload).map_err(|error| {
                CommandError::system_fault(
                    "session_transcript_export_serialize_failed",
                    format!("Xero could not serialize the session transcript export: {error}"),
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
                    "Xero cannot save the transcript because `{}` does not exist.",
                    parent.display()
                ),
            ));
        }
    }

    fs::write(&path, request.content).map_err(|error| {
        CommandError::retryable(
            "session_transcript_export_write_failed",
            format!(
                "Xero could not write the transcript export to `{}`: {error}",
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

#[tauri::command]
pub fn compact_session_history<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CompactSessionHistoryRequestDto,
) -> CommandResult<CompactSessionHistoryResponseDto> {
    validate_transcript_request(
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
    )?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let provider_config = resolve_owned_agent_provider_config(&app, state.inner(), None)?;
    let provider = create_provider_adapter(provider_config)?;
    let compaction = compact_session_history_with_provider(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
        request.raw_tail_message_count,
        AgentCompactionTrigger::Manual,
        "manual_compact_requested",
        provider.as_ref(),
    )?;
    let context_snapshot = build_session_context_snapshot(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
        Some(provider.provider_id()),
        Some(provider.model_id()),
        None,
    )?;
    let response = CompactSessionHistoryResponseDto {
        compaction,
        context_snapshot,
    };
    validate_session_compaction_record_contract(&response.compaction).map_err(|details| {
        CommandError::system_fault(
            "session_compaction_invalid",
            format!("Xero generated an invalid session compaction record: {details}"),
        )
    })?;
    Ok(response)
}

#[tauri::command]
pub fn branch_agent_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: BranchAgentSessionRequestDto,
) -> CommandResult<AgentSessionBranchResponseDto> {
    validate_branch_request(
        &request.project_id,
        &request.source_agent_session_id,
        &request.source_run_id,
        request.title.as_deref(),
    )?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let record = project_store::create_agent_session_branch(
        &repo_root,
        &AgentSessionBranchCreateRecord {
            project_id: request.project_id,
            source_agent_session_id: request.source_agent_session_id,
            source_run_id: request.source_run_id,
            title: request.title,
            selected: request.selected,
            boundary: AgentSessionBranchBoundary::Run,
        },
    )?;
    Ok(agent_session_branch_response_dto(record))
}

#[tauri::command]
pub fn rewind_agent_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RewindAgentSessionRequestDto,
) -> CommandResult<AgentSessionBranchResponseDto> {
    validate_branch_request(
        &request.project_id,
        &request.source_agent_session_id,
        &request.source_run_id,
        request.title.as_deref(),
    )?;
    let boundary = rewind_boundary_from_request(&request)?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let record = project_store::create_agent_session_branch(
        &repo_root,
        &AgentSessionBranchCreateRecord {
            project_id: request.project_id,
            source_agent_session_id: request.source_agent_session_id,
            source_run_id: request.source_run_id,
            title: request.title,
            selected: request.selected,
            boundary,
        },
    )?;
    Ok(agent_session_branch_response_dto(record))
}

#[tauri::command]
pub fn list_session_memories<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListSessionMemoriesRequestDto,
) -> CommandResult<ListSessionMemoriesResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    if let Some(agent_session_id) = request.agent_session_id.as_deref() {
        validate_non_empty(agent_session_id, "agentSessionId")?;
    }
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let memories = project_store::list_agent_memories(
        &repo_root,
        &request.project_id,
        AgentMemoryListFilter {
            agent_session_id: request.agent_session_id.as_deref(),
            include_disabled: request.include_disabled,
            include_rejected: request.include_rejected,
        },
    )?
    .iter()
    .map(session_memory_record_dto)
    .collect::<Vec<_>>();
    for memory in &memories {
        validate_session_memory_record_contract(memory).map_err(|details| {
            CommandError::system_fault(
                "session_memory_invalid",
                format!("Xero projected an invalid memory record: {details}"),
            )
        })?;
    }
    Ok(ListSessionMemoriesResponseDto {
        project_id: request.project_id,
        agent_session_id: request.agent_session_id,
        memories,
    })
}

#[tauri::command]
pub fn extract_session_memory_candidates<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ExtractSessionMemoryCandidatesRequestDto,
) -> CommandResult<ExtractSessionMemoryCandidatesResponseDto> {
    validate_transcript_request(
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
    )?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let provider_config = resolve_owned_agent_provider_config(&app, state.inner(), None)?;
    let provider = create_provider_adapter(provider_config)?;
    extract_session_memory_candidates_with_provider(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
        request.run_id.as_deref(),
        provider.as_ref(),
    )
}

#[tauri::command]
pub fn update_session_memory<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateSessionMemoryRequestDto,
) -> CommandResult<SessionMemoryRecordDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.memory_id, "memoryId")?;
    if request.review_state.is_none() && request.enabled.is_none() {
        return Err(CommandError::invalid_request("memoryUpdate"));
    }
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let review_state = request
        .review_state
        .as_ref()
        .map(agent_memory_review_state_from_dto);
    let enabled = match request.review_state {
        Some(SessionMemoryReviewStateDto::Approved) => Some(request.enabled.unwrap_or(true)),
        Some(SessionMemoryReviewStateDto::Candidate | SessionMemoryReviewStateDto::Rejected) => {
            Some(false)
        }
        None => request.enabled,
    };
    if review_state == Some(AgentMemoryReviewState::Approved) {
        let existing =
            project_store::get_agent_memory(&repo_root, &request.project_id, &request.memory_id)?;
        let (_text, redaction) = redact_session_context_text(&existing.text);
        if redaction.redacted {
            let (code, message) = memory_context_blocked_error(&redaction);
            return Err(CommandError::user_fixable(code, message));
        }
    }
    let record = project_store::update_agent_memory(
        &repo_root,
        &project_store::AgentMemoryUpdateRecord {
            project_id: request.project_id,
            memory_id: request.memory_id,
            review_state,
            enabled,
            diagnostic: None,
        },
    )?;
    let dto = session_memory_record_dto(&record);
    validate_session_memory_record_contract(&dto).map_err(|details| {
        CommandError::system_fault(
            "session_memory_invalid",
            format!("Xero projected an invalid memory record: {details}"),
        )
    })?;
    Ok(dto)
}

#[tauri::command]
pub fn delete_session_memory<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: DeleteSessionMemoryRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.memory_id, "memoryId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    project_store::delete_agent_memory(&repo_root, &request.project_id, &request.memory_id)
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

fn validate_branch_request(
    project_id: &str,
    source_agent_session_id: &str,
    source_run_id: &str,
    title: Option<&str>,
) -> CommandResult<()> {
    validate_non_empty(project_id, "projectId")?;
    validate_non_empty(source_agent_session_id, "sourceAgentSessionId")?;
    validate_non_empty(source_run_id, "sourceRunId")?;
    if let Some(title) = title {
        validate_non_empty(title, "title")?;
    }
    Ok(())
}

fn rewind_boundary_from_request(
    request: &RewindAgentSessionRequestDto,
) -> CommandResult<AgentSessionBranchBoundary> {
    match &request.boundary_kind {
        AgentSessionLineageBoundaryKindDto::Run => {
            Err(CommandError::invalid_request("boundaryKind"))
        }
        AgentSessionLineageBoundaryKindDto::Message => {
            if request.source_checkpoint_id.is_some() {
                return Err(CommandError::invalid_request("sourceCheckpointId"));
            }
            let message_id = request
                .source_message_id
                .filter(|message_id| *message_id > 0)
                .ok_or_else(|| CommandError::invalid_request("sourceMessageId"))?;
            Ok(AgentSessionBranchBoundary::Message { message_id })
        }
        AgentSessionLineageBoundaryKindDto::Checkpoint => {
            if request.source_message_id.is_some() {
                return Err(CommandError::invalid_request("sourceMessageId"));
            }
            let checkpoint_id = request
                .source_checkpoint_id
                .filter(|checkpoint_id| *checkpoint_id > 0)
                .ok_or_else(|| CommandError::invalid_request("sourceCheckpointId"))?;
            Ok(AgentSessionBranchBoundary::Checkpoint { checkpoint_id })
        }
    }
}

fn agent_session_branch_response_dto(
    record: project_store::AgentSessionBranchRecord,
) -> AgentSessionBranchResponseDto {
    AgentSessionBranchResponseDto {
        session: agent_session_dto(&record.session),
        replay_run_id: record.replay_run.run.run_id.clone(),
        lineage: agent_session_lineage_dto(&record.lineage),
    }
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
            format!("Xero projected an invalid session transcript: {details}"),
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
        .or(request_provider_id)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(session.last_provider_id.as_deref())
        .unwrap_or(UNAVAILABLE_CONTEXT_ID)
        .to_string();
    let model_id = latest_snapshot
        .map(|snapshot| snapshot.run.model_id.as_str())
        .or(request_model_id)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(UNAVAILABLE_CONTEXT_ID)
        .to_string();
    let generated_at = now_timestamp();
    let approved_memories =
        project_store::list_approved_agent_memories(repo_root, project_id, Some(agent_session_id))?
            .iter()
            .map(session_memory_record_dto)
            .collect::<Vec<_>>();
    let mut contributors = Vec::new();
    let (prompt_compilation, active_tool_descriptors) = compile_prompt_context_for_snapshot(
        repo_root,
        project_id,
        agent_session_id,
        latest_snapshot,
        pending_prompt,
    )?;
    append_prompt_fragment_contributors(
        &mut contributors,
        project_id,
        agent_session_id,
        latest_snapshot.map(|snapshot| snapshot.run.run_id.as_str()),
        &prompt_compilation,
    );
    if let Some(snapshot) = latest_snapshot {
        append_tool_descriptor_contributors(
            &mut contributors,
            project_id,
            agent_session_id,
            snapshot,
            active_tool_descriptors.as_slice(),
        )?;
    }
    let active_compaction =
        project_store::load_active_agent_compaction(repo_root, project_id, agent_session_id)?
            .filter(|compaction| {
                run_id
                    .map(|run_id| compaction.covers_run(run_id))
                    .unwrap_or(true)
            });
    append_compaction_summary_contributor(
        &mut contributors,
        project_id,
        agent_session_id,
        active_compaction.as_ref(),
    );
    append_history_contributors(
        &mut contributors,
        project_id,
        agent_session_id,
        &snapshots,
        active_compaction.as_ref(),
    );
    append_run_artifact_contributors(&mut contributors, project_id, agent_session_id, &snapshots);
    append_file_observation_contributors(
        &mut contributors,
        project_id,
        agent_session_id,
        &snapshots,
    );
    append_usage_contributors(&mut contributors, project_id, agent_session_id, &snapshots);
    append_pending_prompt_contributor(
        &mut contributors,
        project_id,
        agent_session_id,
        run_id.or_else(|| latest_snapshot.map(|snapshot| snapshot.run.run_id.as_str())),
        pending_prompt,
    );
    let code_map = build_project_code_map(repo_root)?;
    append_code_map_contributors(
        &mut contributors,
        project_id,
        agent_session_id,
        run_id.or_else(|| latest_snapshot.map(|snapshot| snapshot.run.run_id.as_str())),
        &code_map,
    );

    let usage_totals = context_usage_totals(&session, &snapshots, run_id);
    let context_limit = resolve_context_limit(&provider_id, &model_id);
    let budget_tokens = context_limit.effective_input_budget_tokens;
    rank_and_plan_context(
        &mut contributors,
        budget_tokens,
        pending_prompt,
        latest_snapshot.map(|snapshot| snapshot.run.run_id.as_str()),
    );
    let estimated_tokens = contributors
        .iter()
        .filter(|contributor| contributor.included && contributor.model_visible)
        .fold(0_u64, |total, contributor| {
            total.saturating_add(contributor.estimated_tokens)
        });
    let deferred_token_estimate = contributors
        .iter()
        .filter(|contributor| !contributor.model_visible)
        .fold(0_u64, |total, contributor| {
            total.saturating_add(contributor.estimated_tokens)
        });
    let estimation_source = if usage_totals.is_some() {
        SessionUsageSourceDto::Mixed
    } else if contributors.is_empty() {
        SessionUsageSourceDto::Unavailable
    } else {
        SessionUsageSourceDto::Estimated
    };
    let budget = context_budget_with_source(estimated_tokens, context_limit, estimation_source);
    let provider_request_hash = provider_request_hash(&prompt_compilation, &contributors);
    let diff = context_snapshot_diff(
        project_id,
        agent_session_id,
        run_id.or_else(|| latest_snapshot.map(|snapshot| snapshot.run.run_id.as_str())),
        &contributors,
    );
    let mut policy_decisions = vec![evaluate_compaction_policy(SessionCompactionPolicyInput {
        manual_requested: false,
        auto_enabled: false,
        provider_supports_compaction: true,
        active_compaction_present: active_compaction.is_some(),
        estimated_tokens,
        budget_tokens,
        threshold_percent: Some(80),
    })];
    policy_decisions.push(if approved_memories.is_empty() {
        memory_policy_decision(
            "memory:approved:none",
            crate::commands::SessionContextPolicyActionDto::ExcludeMemory,
            "approved_memory_absent",
            "No reviewed memory is currently enabled for this session.",
            false,
        )
    } else {
        memory_policy_decision(
            "memory:approved:inject",
            crate::commands::SessionContextPolicyActionDto::InjectMemory,
            "approved_memory_enabled",
            "Approved memory is included in the next provider system prompt.",
            true,
        )
    });
    let redaction = strongest_context_redaction(
        contributors
            .iter()
            .map(|contributor| &contributor.redaction)
            .chain(policy_decisions.iter().map(|decision| &decision.redaction)),
    );
    let snapshot = SessionContextSnapshotDto {
        contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
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
        provider_request_hash,
        included_token_estimate: estimated_tokens,
        deferred_token_estimate,
        code_map,
        diff,
        contributors,
        policy_decisions,
        usage_totals,
        redaction,
    };

    validate_context_snapshot_contract(&snapshot).map_err(|details| {
        CommandError::system_fault(
            "session_context_snapshot_invalid",
            format!("Xero projected an invalid context snapshot: {details}"),
        )
    })?;
    Ok(snapshot)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compact_session_history_with_provider(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
    raw_tail_message_count: Option<u32>,
    trigger: AgentCompactionTrigger,
    policy_reason: &str,
    provider: &dyn ProviderAdapter,
) -> CommandResult<crate::commands::SessionCompactionRecordDto> {
    let session = project_store::get_agent_session(repo_root, project_id, agent_session_id)?
        .ok_or_else(|| missing_session_error(project_id, agent_session_id))?;
    let snapshots = load_context_snapshots(repo_root, project_id, agent_session_id, run_id)?;
    if snapshots.is_empty() {
        return Err(CommandError::user_fixable(
            "session_compaction_no_runs",
            format!(
                "Xero cannot compact session `{}` because it does not have any owned-agent runs yet.",
                session.agent_session_id
            ),
        ));
    }

    let latest_snapshot = snapshots
        .iter()
        .map(|(snapshot, _)| snapshot)
        .max_by(|left, right| {
            left.run
                .started_at
                .cmp(&right.run.started_at)
                .then_with(|| left.run.run_id.cmp(&right.run.run_id))
        })
        .expect("snapshots is non-empty");
    if provider.provider_id() != latest_snapshot.run.provider_id
        || provider.model_id() != latest_snapshot.run.model_id
    {
        return Err(CommandError::user_fixable(
            "session_compaction_provider_mismatch",
            format!(
                "Xero cannot compact session `{agent_session_id}` with `{}/{}` because the selected run uses `{}/{}`.",
                provider.provider_id(),
                provider.model_id(),
                latest_snapshot.run.provider_id,
                latest_snapshot.run.model_id
            ),
        ));
    }

    let raw_tail_message_count = raw_tail_message_count
        .unwrap_or(DEFAULT_RAW_TAIL_MESSAGE_COUNT)
        .clamp(2, MAX_RAW_TAIL_MESSAGE_COUNT);
    let source = build_compaction_source(&snapshots, raw_tail_message_count)?;
    if source.covered_messages.is_empty() {
        return Err(CommandError::user_fixable(
            "session_compaction_not_needed",
            "Xero needs more recorded conversation before a manual compaction would reduce replay context.",
        ));
    }

    let input_tokens = estimate_tokens(&source.transcript);
    let request = ProviderCompactionRequest {
        project_id: project_id.into(),
        agent_session_id: agent_session_id.into(),
        run_id: run_id.map(ToOwned::to_owned),
        provider_id: provider.provider_id().into(),
        model_id: provider.model_id().into(),
        transcript: source.transcript.clone(),
        max_summary_tokens: MAX_COMPACTION_SUMMARY_TOKENS,
    };
    let mut ignored_stream_event = |_event| Ok(());
    let outcome = provider.compact_transcript(&request, &mut ignored_stream_event)?;
    let (summary, summary_redaction) = redact_session_context_text(&outcome.summary);
    if summary.trim().is_empty() {
        return Err(CommandError::retryable(
            "session_compaction_empty_summary",
            "The provider returned an empty compaction summary. Try again or use a different provider profile.",
        ));
    }
    if summary_redaction.redacted {
        return Err(CommandError::retryable(
            "session_compaction_summary_redacted",
            "The provider returned a compaction summary that looked secret-bearing, so Xero refused to save it.",
        ));
    }

    let now = now_timestamp();
    let compaction_id = format!(
        "session-compact:{}:{}:{}:{}",
        agent_session_id,
        now,
        source.source_hash.chars().take(12).collect::<String>(),
        random_hex_suffix()
    );
    let record = project_store::insert_agent_compaction(
        repo_root,
        &NewAgentCompactionRecord {
            compaction_id,
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            source_run_id: latest_snapshot.run.run_id.clone(),
            provider_id: provider.provider_id().into(),
            model_id: provider.model_id().into(),
            summary: summary.clone(),
            covered_run_ids: source.covered_run_ids.clone(),
            covered_message_start_id: source.covered_message_start_id,
            covered_message_end_id: source.covered_message_end_id,
            covered_event_start_id: source.covered_event_start_id,
            covered_event_end_id: source.covered_event_end_id,
            source_hash: source.source_hash.clone(),
            input_tokens,
            summary_tokens: estimate_tokens(&summary),
            raw_tail_message_count,
            policy_reason: policy_reason.into(),
            trigger,
            diagnostic: None,
            created_at: now.clone(),
        },
    )?;

    Ok(session_compaction_record_dto(&record))
}

fn extract_session_memory_candidates_with_provider(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
    provider: &dyn ProviderAdapter,
) -> CommandResult<ExtractSessionMemoryCandidatesResponseDto> {
    let _session = project_store::get_agent_session(repo_root, project_id, agent_session_id)?
        .ok_or_else(|| missing_session_error(project_id, agent_session_id))?;
    let snapshots = load_context_snapshots(repo_root, project_id, agent_session_id, run_id)?;
    let completed_snapshots = snapshots
        .iter()
        .filter(|(snapshot, _)| snapshot.run.status == project_store::AgentRunStatus::Completed)
        .cloned()
        .collect::<Vec<_>>();
    if completed_snapshots.is_empty() {
        return Err(CommandError::user_fixable(
            "session_memory_no_completed_runs",
            "Xero needs at least one completed owned-agent run before it can propose reviewed memory.",
        ));
    }

    let source = build_memory_extraction_source(&completed_snapshots)?;
    let existing_memories = project_store::list_agent_memories(
        repo_root,
        project_id,
        AgentMemoryListFilter {
            agent_session_id: Some(agent_session_id),
            include_disabled: true,
            include_rejected: false,
        },
    )?;
    let existing_texts = existing_memories
        .iter()
        .map(|memory| memory.text.clone())
        .collect::<Vec<_>>();
    let request = ProviderMemoryExtractionRequest {
        project_id: project_id.into(),
        agent_session_id: agent_session_id.into(),
        run_id: run_id.map(ToOwned::to_owned),
        provider_id: provider.provider_id().into(),
        model_id: provider.model_id().into(),
        transcript: source.transcript.clone(),
        existing_memories: existing_texts,
        max_candidates: MAX_MEMORY_CANDIDATES,
    };
    let mut ignored_stream_event = |_event| Ok(());
    let outcome = provider.extract_memory_candidates(&request, &mut ignored_stream_event)?;

    let mut created = Vec::new();
    let mut diagnostics = Vec::new();
    let mut skipped_duplicate_count = 0_usize;
    let mut rejected_count = 0_usize;
    let now = now_timestamp();

    for candidate in outcome
        .candidates
        .into_iter()
        .take(MAX_MEMORY_CANDIDATES as usize)
    {
        match prepare_new_memory_candidate(
            project_id,
            agent_session_id,
            &source,
            candidate,
            now.as_str(),
        ) {
            Ok(record) => {
                let text_hash = project_store::agent_memory_text_hash(&record.text);
                if project_store::find_active_agent_memory_by_hash(
                    repo_root,
                    project_id,
                    &record.scope,
                    record.agent_session_id.as_deref(),
                    &record.kind,
                    &text_hash,
                )?
                .is_some()
                {
                    skipped_duplicate_count = skipped_duplicate_count.saturating_add(1);
                    continue;
                }
                let persisted = project_store::insert_agent_memory(repo_root, &record)?;
                created.push(session_memory_record_dto(&persisted));
            }
            Err(diagnostic) => {
                rejected_count = rejected_count.saturating_add(1);
                diagnostics.push(diagnostic);
            }
        }
    }

    for memory in &created {
        validate_session_memory_record_contract(memory).map_err(|details| {
            CommandError::system_fault(
                "session_memory_invalid",
                format!("Xero projected an invalid memory record: {details}"),
            )
        })?;
    }
    let memories = project_store::list_agent_memories(
        repo_root,
        project_id,
        AgentMemoryListFilter {
            agent_session_id: Some(agent_session_id),
            include_disabled: true,
            include_rejected: false,
        },
    )?
    .iter()
    .map(session_memory_record_dto)
    .collect::<Vec<_>>();

    Ok(ExtractSessionMemoryCandidatesResponseDto {
        project_id: project_id.into(),
        agent_session_id: agent_session_id.into(),
        memories,
        created_count: created.len(),
        skipped_duplicate_count,
        rejected_count,
        diagnostics,
    })
}

struct MemoryExtractionSource {
    transcript: String,
    source_run_id: Option<String>,
    source_item_ids: Vec<String>,
}

fn build_memory_extraction_source(
    snapshots: &[(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )],
) -> CommandResult<MemoryExtractionSource> {
    let run_transcripts = snapshots
        .iter()
        .map(|(snapshot, usage)| run_transcript_from_agent_snapshot(snapshot, usage.as_ref()))
        .collect::<Vec<_>>();
    let source_run_id = run_transcripts.last().map(|run| run.run_id.clone());
    let mut source_item_ids = Vec::new();
    let mut transcript = String::from(
        "Review this completed Xero session transcript for durable memory candidates.\n",
    );
    for run in &run_transcripts {
        transcript.push_str(&format!(
            "\nRun {} provider={} model={} status={}\n",
            run.run_id, run.provider_id, run.model_id, run.status
        ));
        for item in &run.items {
            source_item_ids.push(item.item_id.clone());
            let text = item
                .text
                .as_deref()
                .or(item.summary.as_deref())
                .unwrap_or_default();
            if text.trim().is_empty() {
                continue;
            }
            let (text, _redaction) = redact_session_context_text(text);
            transcript.push_str(&format!(
                "- [{}] {} {}: {}\n",
                item.item_id,
                item_kind_label(item),
                item.actor_label(),
                preview_context_text(&text)
            ));
        }
    }
    Ok(MemoryExtractionSource {
        transcript,
        source_run_id,
        source_item_ids,
    })
}

trait SessionTranscriptItemActorLabel {
    fn actor_label(&self) -> &'static str;
}

impl SessionTranscriptItemActorLabel for SessionTranscriptItemDto {
    fn actor_label(&self) -> &'static str {
        match self.actor {
            crate::commands::SessionTranscriptActorDto::System => "system",
            crate::commands::SessionTranscriptActorDto::Developer => "developer",
            crate::commands::SessionTranscriptActorDto::User => "user",
            crate::commands::SessionTranscriptActorDto::Assistant => "assistant",
            crate::commands::SessionTranscriptActorDto::Tool => "tool",
            crate::commands::SessionTranscriptActorDto::Runtime => "runtime",
            crate::commands::SessionTranscriptActorDto::Xero => "xero",
            crate::commands::SessionTranscriptActorDto::Operator => "operator",
        }
    }
}

fn prepare_new_memory_candidate(
    project_id: &str,
    agent_session_id: &str,
    source: &MemoryExtractionSource,
    candidate: ProviderMemoryCandidate,
    created_at: &str,
) -> Result<NewAgentMemoryRecord, SessionMemoryDiagnosticDto> {
    let scope = agent_memory_scope_from_provider(&candidate.scope).ok_or_else(|| {
        session_memory_diagnostic_dto(
            "session_memory_candidate_scope_invalid",
            "A provider memory candidate used an unsupported scope.",
        )
    })?;
    let kind = agent_memory_kind_from_provider(&candidate.kind).ok_or_else(|| {
        session_memory_diagnostic_dto(
            "session_memory_candidate_kind_invalid",
            "A provider memory candidate used an unsupported kind.",
        )
    })?;
    let text = candidate.text.trim().to_string();
    if text.is_empty() {
        return Err(session_memory_diagnostic_dto(
            "session_memory_candidate_empty",
            "A provider memory candidate did not include text.",
        ));
    }
    let confidence = candidate.confidence.unwrap_or(0).min(100);
    if confidence < MIN_MEMORY_CONFIDENCE {
        return Err(session_memory_diagnostic_dto(
            "session_memory_candidate_low_confidence",
            "Xero skipped a low-confidence memory candidate.",
        ));
    }
    let (_redacted_text, redaction) = redact_session_context_text(&text);
    if redaction.redacted {
        let (code, message) = memory_candidate_blocked_diagnostic(&redaction);
        return Err(session_memory_diagnostic_dto(code, message));
    }
    let mut source_item_ids = candidate
        .source_item_ids
        .into_iter()
        .map(|item_id| item_id.trim().to_string())
        .filter(|item_id| !item_id.is_empty())
        .collect::<Vec<_>>();
    if source_item_ids.is_empty() {
        source_item_ids = source.source_item_ids.iter().take(8).cloned().collect();
    }
    Ok(NewAgentMemoryRecord {
        memory_id: project_store::generate_agent_memory_id(),
        project_id: project_id.into(),
        agent_session_id: match scope {
            AgentMemoryScope::Project => None,
            AgentMemoryScope::Session => Some(agent_session_id.into()),
        },
        scope,
        kind,
        text,
        review_state: AgentMemoryReviewState::Candidate,
        enabled: false,
        confidence: Some(confidence),
        source_run_id: source.source_run_id.clone(),
        source_item_ids,
        diagnostic: None,
        created_at: created_at.into(),
    })
}

fn memory_context_blocked_error(
    redaction: &SessionContextRedactionDto,
) -> (&'static str, &'static str) {
    if redaction
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("prompt-injection"))
    {
        (
            "session_memory_integrity_blocked",
            "Xero will not approve memory text that tries to override system, developer, or tool instructions.",
        )
    } else {
        (
            "session_memory_secret_blocked",
            "Xero will not approve memory text that looks secret-bearing.",
        )
    }
}

fn memory_candidate_blocked_diagnostic(
    redaction: &SessionContextRedactionDto,
) -> (&'static str, &'static str) {
    if redaction
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("prompt-injection"))
    {
        (
            "session_memory_candidate_integrity",
            "Xero skipped a memory candidate because it looked like an instruction-override attempt.",
        )
    } else {
        (
            "session_memory_candidate_secret",
            "Xero skipped a memory candidate because its text looked secret-bearing.",
        )
    }
}

fn agent_memory_scope_from_provider(value: &str) -> Option<AgentMemoryScope> {
    match value.trim().to_ascii_lowercase().as_str() {
        "project" => Some(AgentMemoryScope::Project),
        "session" => Some(AgentMemoryScope::Session),
        _ => None,
    }
}

fn agent_memory_kind_from_provider(value: &str) -> Option<AgentMemoryKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "project_fact" | "project fact" | "fact" => Some(AgentMemoryKind::ProjectFact),
        "user_preference" | "user preference" | "preference" => {
            Some(AgentMemoryKind::UserPreference)
        }
        "decision" => Some(AgentMemoryKind::Decision),
        "session_summary" | "session summary" | "summary" => Some(AgentMemoryKind::SessionSummary),
        "troubleshooting" | "troubleshooting_fact" | "troubleshooting fact" => {
            Some(AgentMemoryKind::Troubleshooting)
        }
        _ => None,
    }
}

fn agent_memory_review_state_from_dto(
    review_state: &SessionMemoryReviewStateDto,
) -> AgentMemoryReviewState {
    match review_state {
        SessionMemoryReviewStateDto::Candidate => AgentMemoryReviewState::Candidate,
        SessionMemoryReviewStateDto::Approved => AgentMemoryReviewState::Approved,
        SessionMemoryReviewStateDto::Rejected => AgentMemoryReviewState::Rejected,
    }
}

struct CompactionSource<'a> {
    transcript: String,
    covered_messages: Vec<&'a AgentMessageRecord>,
    covered_run_ids: Vec<String>,
    covered_message_start_id: Option<i64>,
    covered_message_end_id: Option<i64>,
    covered_event_start_id: Option<i64>,
    covered_event_end_id: Option<i64>,
    source_hash: String,
}

fn build_compaction_source(
    snapshots: &[(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )],
    raw_tail_message_count: u32,
) -> CommandResult<CompactionSource<'_>> {
    let mut messages = snapshots
        .iter()
        .flat_map(|(snapshot, _)| snapshot.messages.iter())
        .filter(|message| message.role != AgentMessageRole::System)
        .collect::<Vec<_>>();
    messages.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    let tail_start = compaction_tail_start_index(&messages, raw_tail_message_count as usize);
    let covered_messages = messages.into_iter().take(tail_start).collect::<Vec<_>>();
    let covered_run_ids = covered_messages
        .iter()
        .map(|message| message.run_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let covered_run_id_set = covered_run_ids.iter().cloned().collect::<BTreeSet<_>>();
    let covered_message_start_id = covered_messages.iter().map(|message| message.id).min();
    let covered_message_end_id = covered_messages.iter().map(|message| message.id).max();

    let mut covered_events = snapshots
        .iter()
        .flat_map(|(snapshot, _)| snapshot.events.iter())
        .filter(|event| covered_run_id_set.contains(&event.run_id))
        .collect::<Vec<_>>();
    covered_events.sort_by(|left, right| left.id.cmp(&right.id));
    let covered_event_start_id = covered_events.iter().map(|event| event.id).min();
    let covered_event_end_id = covered_events.iter().map(|event| event.id).max();
    let transcript =
        render_compaction_transcript(snapshots, &covered_run_id_set, &covered_messages)?;
    let source_hash = compaction_source_hash(
        snapshots,
        &covered_run_id_set,
        &covered_messages,
        &covered_events,
    )?;

    Ok(CompactionSource {
        transcript,
        covered_messages,
        covered_run_ids,
        covered_message_start_id,
        covered_message_end_id,
        covered_event_start_id,
        covered_event_end_id,
        source_hash,
    })
}

fn compaction_tail_start_index(messages: &[&AgentMessageRecord], raw_tail_count: usize) -> usize {
    if messages.len() <= raw_tail_count {
        return messages.len();
    }
    let mut start = messages.len().saturating_sub(raw_tail_count);
    while start > 0 && messages[start].role == AgentMessageRole::Tool {
        start = start.saturating_sub(1);
    }
    start
}

fn render_compaction_transcript(
    snapshots: &[(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )],
    covered_run_ids: &BTreeSet<String>,
    covered_messages: &[&AgentMessageRecord],
) -> CommandResult<String> {
    let mut output = String::new();
    output.push_str("Compact the following Xero session history for replay.\n");
    output.push_str("Raw transcript rows stay durable; unresolved actions remain unresolved.\n\n");
    for (snapshot, usage) in snapshots {
        if !covered_run_ids.contains(&snapshot.run.run_id) {
            continue;
        }
        output.push_str(&format!(
            "Run {} provider={} model={} status={:?}\n",
            snapshot.run.run_id,
            snapshot.run.provider_id,
            snapshot.run.model_id,
            snapshot.run.status
        ));
        if let Some(usage) = usage {
            output.push_str(&format!(
                "Usage: {} input + {} output = {} total tokens.\n",
                usage.input_tokens, usage.output_tokens, usage.total_tokens
            ));
        }
        for message in covered_messages
            .iter()
            .filter(|message| message.run_id == snapshot.run.run_id)
        {
            let (text, _) = redact_session_context_text(&message.content);
            output.push_str(&format!(
                "- {} message {}: {}\n",
                message_role_label(&message.role),
                message.id,
                preview_context_text(&text)
            ));
        }
        for tool_call in &snapshot.tool_calls {
            let (input, _) = redact_session_context_text(&tool_call.input_json);
            let result = tool_call
                .result_json
                .as_deref()
                .map(redact_session_context_text)
                .map(|(value, _)| preview_context_text(&value))
                .unwrap_or_else(|| "(no result recorded)".into());
            output.push_str(&format!(
                "- Tool {} {} state={:?} input={} result={}\n",
                tool_call.tool_name,
                tool_call.tool_call_id,
                tool_call.state,
                preview_context_text(&input),
                result
            ));
        }
        for action in &snapshot.action_requests {
            let (detail, _) = redact_session_context_text(&action.detail);
            output.push_str(&format!(
                "- Action {} type={} status={} detail={}\n",
                action.action_id,
                action.action_type,
                action.status,
                preview_context_text(&detail)
            ));
        }
        for checkpoint in &snapshot.checkpoints {
            let (summary, _) = redact_session_context_text(&checkpoint.summary);
            output.push_str(&format!(
                "- Checkpoint {}: {}\n",
                checkpoint.checkpoint_kind,
                preview_context_text(&summary)
            ));
        }
        for file_change in &snapshot.file_changes {
            let (path, _) = redact_session_context_text(&file_change.path);
            output.push_str(&format!(
                "- File change {}: {}\n",
                file_change.operation,
                preview_context_text(&path)
            ));
        }
        output.push('\n');
    }
    Ok(output)
}

fn compaction_source_hash(
    snapshots: &[(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )],
    covered_run_ids: &BTreeSet<String>,
    covered_messages: &[&AgentMessageRecord],
    covered_events: &[&project_store::AgentEventRecord],
) -> CommandResult<String> {
    let mut hasher = Sha256::new();
    for (snapshot, _usage) in snapshots {
        if !covered_run_ids.contains(&snapshot.run.run_id) {
            continue;
        }
        hasher.update(snapshot.run.run_id.as_bytes());
        hasher.update(snapshot.run.provider_id.as_bytes());
        hasher.update(snapshot.run.model_id.as_bytes());
        hasher.update(snapshot.run.prompt.as_bytes());
        for message in covered_messages
            .iter()
            .filter(|message| message.run_id == snapshot.run.run_id)
        {
            hasher.update(message.id.to_string().as_bytes());
            hasher.update(format!("{:?}", message.role).as_bytes());
            hasher.update(message.content.as_bytes());
        }
    }
    for event in covered_events {
        hasher.update(event.id.to_string().as_bytes());
        hasher.update(event.run_id.as_bytes());
        hasher.update(format!("{:?}", event.event_kind).as_bytes());
        hasher.update(event.payload_json.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
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
        let snapshot = match project_store::load_agent_run(repo_root, project_id, run_id) {
            Ok(snapshot) => snapshot,
            Err(error) if error.code == "agent_run_not_found" => {
                return project_store::load_agent_session_run_snapshots(
                    repo_root,
                    project_id,
                    agent_session_id,
                );
            }
            Err(error) => return Err(error),
        };
        ensure_run_belongs_to_session(&snapshot, project_id, agent_session_id)?;
        let usage = project_store::load_agent_usage(repo_root, project_id, run_id)?;
        return Ok(vec![(snapshot, usage)]);
    }

    project_store::load_agent_session_run_snapshots(repo_root, project_id, agent_session_id)
}

fn compile_prompt_context_for_snapshot(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    latest_snapshot: Option<&AgentRunSnapshotRecord>,
    pending_prompt: Option<&str>,
) -> CommandResult<(PromptCompilation, Vec<AgentToolDescriptor>)> {
    let mut controls = runtime_controls_from_request(None);
    if let Some(snapshot) = latest_snapshot {
        controls.active.runtime_agent_id = snapshot.run.runtime_agent_id;
    }
    let descriptors = if let Some(snapshot) = latest_snapshot {
        tool_registry_for_snapshot(
            repo_root,
            snapshot,
            &controls,
            true,
            BrowserControlPreferenceDto::Default,
            None,
        )?
        .into_descriptors()
    } else {
        ToolRegistry::for_prompt_with_options(
            repo_root,
            pending_prompt.unwrap_or_default(),
            &controls,
            ToolRegistryOptions {
                skill_tool_enabled: true,
                browser_control_preference: BrowserControlPreferenceDto::Default,
                runtime_agent_id: controls.active.runtime_agent_id,
                agent_tool_policy: None,
            },
        )
        .into_descriptors()
    };
    let skill_contexts = if let Some(snapshot) = latest_snapshot {
        let provider_messages = provider_messages_from_snapshot(repo_root, snapshot)?;
        skill_contexts_from_provider_messages(&provider_messages)?
    } else {
        Vec::new()
    };
    let compilation = compile_system_prompt_for_session(
        repo_root,
        Some(project_id),
        Some(agent_session_id),
        controls.active.runtime_agent_id,
        BrowserControlPreferenceDto::Default,
        descriptors.as_slice(),
        None,
        None,
        None,
        skill_contexts,
    )?;
    Ok((compilation, descriptors))
}

fn append_prompt_fragment_contributors(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
    compilation: &PromptCompilation,
) {
    for fragment in &compilation.fragments {
        append_context_contributor(
            contributors,
            ContextContributorParts {
                contributor_id: format!("prompt_fragment:{}", fragment.id),
                kind: prompt_fragment_contributor_kind(fragment),
                label: fragment.title.clone(),
                project_id: Some(project_id),
                agent_session_id: Some(agent_session_id),
                run_id,
                source_id: Some(prompt_fragment_source_id(fragment)),
                raw_text: Some(fragment.body.as_str()),
                estimate_text: Some(fragment.body.as_str()),
                estimated_tokens: Some(fragment.token_estimate),
                included: true,
                model_visible: true,
                prompt_fragment_id: Some(fragment.id.as_str()),
                prompt_fragment_priority: Some(fragment.priority),
                prompt_fragment_hash: Some(fragment.sha256.as_str()),
                prompt_fragment_provenance: Some(fragment.provenance.as_str()),
            },
        );
    }
}

fn prompt_fragment_contributor_kind(fragment: &PromptFragment) -> SessionContextContributorKindDto {
    if fragment.id.starts_with("project.instructions.") {
        SessionContextContributorKindDto::InstructionFile
    } else if fragment.id.starts_with("skill.context.") {
        SessionContextContributorKindDto::SkillContext
    } else if fragment.id == "xero.approved_memory" {
        SessionContextContributorKindDto::ApprovedMemory
    } else {
        SessionContextContributorKindDto::SystemPrompt
    }
}

fn prompt_fragment_source_id(fragment: &PromptFragment) -> &str {
    fragment
        .provenance
        .strip_prefix("project:")
        .unwrap_or(fragment.provenance.as_str())
}

fn append_tool_descriptor_contributors(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    snapshot: &AgentRunSnapshotRecord,
    descriptors: &[AgentToolDescriptor],
) -> CommandResult<()> {
    let mut descriptors = descriptors.to_vec();
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
                estimated_tokens: None,
                included: true,
                model_visible: true,
                prompt_fragment_id: None,
                prompt_fragment_priority: None,
                prompt_fragment_hash: None,
                prompt_fragment_provenance: None,
            },
        );
    }
    Ok(())
}

fn tool_descriptor_estimate_text(descriptor: &AgentToolDescriptor) -> CommandResult<String> {
    serde_json::to_string(descriptor).map_err(|error| {
        CommandError::system_fault(
            "session_context_tool_descriptor_serialize_failed",
            format!("Xero could not estimate a tool descriptor context contribution: {error}"),
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
    active_compaction: Option<&project_store::AgentCompactionRecord>,
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
            active_compaction,
        );
        let mut messages = snapshot.messages.iter().collect::<Vec<_>>();
        messages.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        for message in messages {
            if active_compaction.is_some_and(|compaction| {
                compaction.covers_run(&message.run_id) && compaction.covers_message_id(message.id)
            }) {
                continue;
            }
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
                            prompt_fragment_id: None,
                            prompt_fragment_priority: None,
                            prompt_fragment_hash: None,
                            prompt_fragment_provenance: None,
                            project_id: Some(project_id),
                            agent_session_id: Some(agent_session_id),
                            run_id: Some(message.run_id.as_str()),
                            source_id: Some(message_id.as_str()),
                            raw_text: Some(message.content.as_str()),
                            estimate_text: Some(message.content.as_str()),
                            estimated_tokens: None,
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
                            prompt_fragment_id: None,
                            prompt_fragment_priority: None,
                            prompt_fragment_hash: None,
                            prompt_fragment_provenance: None,
                            project_id: Some(project_id),
                            agent_session_id: Some(agent_session_id),
                            run_id: Some(message.run_id.as_str()),
                            source_id: Some(message_id.as_str()),
                            raw_text: Some(message.content.as_str()),
                            estimate_text: Some(message.content.as_str()),
                            estimated_tokens: None,
                            included: true,
                            model_visible: true,
                        },
                    );
                }
            }
        }
    }
}

fn append_compaction_summary_contributor(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    active_compaction: Option<&project_store::AgentCompactionRecord>,
) {
    let Some(compaction) = active_compaction else {
        return;
    };
    append_context_contributor(
        contributors,
        ContextContributorParts {
            contributor_id: format!("compaction_summary:{}", compaction.compaction_id),
            kind: SessionContextContributorKindDto::CompactionSummary,
            label: "Compacted history summary".into(),
            prompt_fragment_id: None,
            prompt_fragment_priority: None,
            prompt_fragment_hash: None,
            prompt_fragment_provenance: None,
            project_id: Some(project_id),
            agent_session_id: Some(agent_session_id),
            run_id: Some(compaction.source_run_id.as_str()),
            source_id: Some(compaction.compaction_id.as_str()),
            raw_text: Some(compaction.summary.as_str()),
            estimate_text: Some(compaction.summary.as_str()),
            estimated_tokens: None,
            included: true,
            model_visible: true,
        },
    );
}

fn append_run_prompt_contributor_if_needed(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    snapshot: &AgentRunSnapshotRecord,
    active_compaction: Option<&project_store::AgentCompactionRecord>,
) {
    if snapshot.run.prompt.trim().is_empty() {
        return;
    }
    if active_compaction.is_some_and(|compaction| compaction.covers_run(&snapshot.run.run_id)) {
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
            prompt_fragment_id: None,
            prompt_fragment_priority: None,
            prompt_fragment_hash: None,
            prompt_fragment_provenance: None,
            project_id: Some(project_id),
            agent_session_id: Some(agent_session_id),
            run_id: Some(snapshot.run.run_id.as_str()),
            source_id: Some(snapshot.run.run_id.as_str()),
            raw_text: Some(snapshot.run.prompt.as_str()),
            estimate_text: Some(snapshot.run.prompt.as_str()),
            estimated_tokens: None,
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
                prompt_fragment_id: None,
                prompt_fragment_priority: None,
                prompt_fragment_hash: None,
                prompt_fragment_provenance: None,
                project_id: Some(project_id),
                agent_session_id: Some(agent_session_id),
                run_id: Some(snapshot.run.run_id.as_str()),
                source_id: Some(snapshot.run.run_id.as_str()),
                raw_text: Some(text.as_str()),
                estimate_text: None,
                estimated_tokens: None,
                included: false,
                model_visible: false,
            },
        );
    }
}

fn append_run_artifact_contributors(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    snapshots: &[(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )],
) {
    for (snapshot, _) in snapshots {
        for checkpoint in &snapshot.checkpoints {
            let checkpoint_id = checkpoint.id.to_string();
            append_context_contributor(
                contributors,
                ContextContributorParts {
                    contributor_id: format!(
                        "run_artifact:checkpoint:{}:{}",
                        snapshot.run.run_id, checkpoint.id
                    ),
                    kind: SessionContextContributorKindDto::RunArtifact,
                    label: format!("Checkpoint: {}", checkpoint.checkpoint_kind),
                    prompt_fragment_id: None,
                    prompt_fragment_priority: None,
                    prompt_fragment_hash: None,
                    prompt_fragment_provenance: None,
                    project_id: Some(project_id),
                    agent_session_id: Some(agent_session_id),
                    run_id: Some(snapshot.run.run_id.as_str()),
                    source_id: Some(checkpoint_id.as_str()),
                    raw_text: Some(checkpoint.summary.as_str()),
                    estimate_text: Some(checkpoint.summary.as_str()),
                    estimated_tokens: None,
                    included: true,
                    model_visible: true,
                },
            );
        }
        for action in &snapshot.action_requests {
            let text = format!("{} [{}]: {}", action.title, action.status, action.detail);
            append_context_contributor(
                contributors,
                ContextContributorParts {
                    contributor_id: format!(
                        "run_artifact:action:{}:{}",
                        snapshot.run.run_id, action.action_id
                    ),
                    kind: SessionContextContributorKindDto::RunArtifact,
                    label: format!("Action request: {}", action.title),
                    prompt_fragment_id: None,
                    prompt_fragment_priority: None,
                    prompt_fragment_hash: None,
                    prompt_fragment_provenance: None,
                    project_id: Some(project_id),
                    agent_session_id: Some(agent_session_id),
                    run_id: Some(snapshot.run.run_id.as_str()),
                    source_id: Some(action.action_id.as_str()),
                    raw_text: Some(text.as_str()),
                    estimate_text: Some(text.as_str()),
                    estimated_tokens: None,
                    included: true,
                    model_visible: true,
                },
            );
        }
    }
}

fn append_file_observation_contributors(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    snapshots: &[(
        AgentRunSnapshotRecord,
        Option<project_store::AgentUsageRecord>,
    )],
) {
    for (snapshot, _) in snapshots {
        for change in &snapshot.file_changes {
            let change_id = change.id.to_string();
            let (path, _path_redaction) = redact_session_context_text(&change.path);
            let text = format!(
                "{} {} old={} new={}",
                change.operation,
                path,
                change.old_hash.as_deref().unwrap_or("none"),
                change.new_hash.as_deref().unwrap_or("none")
            );
            append_context_contributor(
                contributors,
                ContextContributorParts {
                    contributor_id: format!(
                        "file_observation:{}:{}",
                        snapshot.run.run_id, change.id
                    ),
                    kind: SessionContextContributorKindDto::FileObservation,
                    label: format!("File observation: {path}"),
                    prompt_fragment_id: None,
                    prompt_fragment_priority: None,
                    prompt_fragment_hash: None,
                    prompt_fragment_provenance: None,
                    project_id: Some(project_id),
                    agent_session_id: Some(agent_session_id),
                    run_id: Some(snapshot.run.run_id.as_str()),
                    source_id: Some(change_id.as_str()),
                    raw_text: Some(text.as_str()),
                    estimate_text: Some(text.as_str()),
                    estimated_tokens: None,
                    included: true,
                    model_visible: true,
                },
            );
        }
    }
}

fn append_code_map_contributors(
    contributors: &mut Vec<SessionContextContributorDto>,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
    code_map: &SessionContextCodeMapDto,
) {
    for manifest in &code_map.package_manifests {
        let text = format!(
            "{} manifest {} package={} dependencies={}",
            manifest.ecosystem,
            manifest.path,
            manifest.package_name.as_deref().unwrap_or("unknown"),
            manifest.dependency_count
        );
        append_context_contributor(
            contributors,
            ContextContributorParts {
                contributor_id: format!("dependency_metadata:{}", manifest.path),
                kind: SessionContextContributorKindDto::DependencyMetadata,
                label: format!("Dependency metadata: {}", manifest.path),
                prompt_fragment_id: None,
                prompt_fragment_priority: None,
                prompt_fragment_hash: None,
                prompt_fragment_provenance: None,
                project_id: Some(project_id),
                agent_session_id: Some(agent_session_id),
                run_id,
                source_id: Some(manifest.path.as_str()),
                raw_text: Some(text.as_str()),
                estimate_text: Some(text.as_str()),
                estimated_tokens: None,
                included: true,
                model_visible: true,
            },
        );
    }
    for symbol in &code_map.symbols {
        let text = format!(
            "{} {} at {}:{}",
            symbol.kind, symbol.name, symbol.path, symbol.line
        );
        append_context_contributor(
            contributors,
            ContextContributorParts {
                contributor_id: format!("code_symbol:{}", symbol.symbol_id),
                kind: SessionContextContributorKindDto::CodeSymbol,
                label: format!("{} {}", symbol.kind, symbol.name),
                prompt_fragment_id: None,
                prompt_fragment_priority: None,
                prompt_fragment_hash: None,
                prompt_fragment_provenance: None,
                project_id: Some(project_id),
                agent_session_id: Some(agent_session_id),
                run_id,
                source_id: Some(symbol.symbol_id.as_str()),
                raw_text: Some(text.as_str()),
                estimate_text: Some(text.as_str()),
                estimated_tokens: Some(symbol.estimated_tokens),
                included: true,
                model_visible: true,
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
            prompt_fragment_id: None,
            prompt_fragment_priority: None,
            prompt_fragment_hash: None,
            prompt_fragment_provenance: None,
            project_id: Some(project_id),
            agent_session_id: Some(agent_session_id),
            run_id,
            source_id: Some("pending_prompt"),
            raw_text: Some(pending_prompt),
            estimate_text: Some(pending_prompt),
            estimated_tokens: None,
            included: true,
            model_visible: true,
        },
    );
}

fn rank_and_plan_context(
    contributors: &mut [SessionContextContributorDto],
    budget_tokens: Option<u64>,
    pending_prompt: Option<&str>,
    latest_run_id: Option<&str>,
) {
    let max_sequence = contributors
        .iter()
        .map(|contributor| contributor.sequence)
        .max()
        .unwrap_or(1);
    let relevance_terms = pending_prompt
        .unwrap_or_default()
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && character != '_' && character != '-'
        })
        .filter(|term| term.len() > 2)
        .map(str::to_ascii_lowercase)
        .collect::<BTreeSet<_>>();
    for contributor in contributors.iter_mut() {
        contributor.authority_score = authority_score(&contributor.kind);
        contributor.recency_score = recency_score(contributor.sequence, max_sequence);
        contributor.relevance_score = relevance_score(contributor, &relevance_terms, latest_run_id);
        contributor.task_phase = task_phase_for_kind(&contributor.kind);
        contributor.rank_score = (contributor.authority_score as u16 * 4)
            .saturating_add(contributor.relevance_score as u16 * 3)
            .saturating_add(contributor.recency_score as u16 * 2);
        if contributor.estimated_tokens >= LARGE_CONTEXT_NODE_TOKENS
            && matches!(
                contributor.kind,
                SessionContextContributorKindDto::ToolResult
                    | SessionContextContributorKindDto::ConversationTail
            )
        {
            contributor.disposition = SessionContextDispositionDto::Summarize;
            contributor.summary = contributor
                .text
                .as_deref()
                .map(summary_for_large_context_node)
                .filter(|summary| !summary.trim().is_empty());
        } else if contributor.included {
            contributor.disposition = SessionContextDispositionDto::Include;
        } else {
            contributor.disposition = SessionContextDispositionDto::RetrieveOnDemand;
        }
    }

    let Some(budget_tokens) = budget_tokens else {
        return;
    };
    let planning_budget = budget_tokens;
    let mut ranked = contributors
        .iter()
        .enumerate()
        .filter(|(_, contributor)| contributor.included && contributor.model_visible)
        .map(|(index, contributor)| (index, contributor.rank_score, contributor.estimated_tokens))
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut spent = 0_u64;
    let mut include = BTreeSet::new();
    for (index, _, tokens) in ranked {
        if spent.saturating_add(tokens) <= planning_budget
            || is_required_context(&contributors[index].kind)
        {
            spent = spent.saturating_add(tokens);
            include.insert(index);
        }
    }
    for (index, contributor) in contributors.iter_mut().enumerate() {
        if !contributor.model_visible {
            continue;
        }
        if include.contains(&index) {
            continue;
        }
        contributor.model_visible = false;
        contributor.included = false;
        contributor.disposition = if contributor.summary.is_some() {
            SessionContextDispositionDto::Summarize
        } else {
            SessionContextDispositionDto::Defer
        };
        contributor.omitted_reason = Some(format!(
            "Deferred by context budget planner: rank {} would exceed {} planned tokens.",
            contributor.rank_score, planning_budget
        ));
    }
}

fn authority_score(kind: &SessionContextContributorKindDto) -> u8 {
    match kind {
        SessionContextContributorKindDto::SystemPrompt => 100,
        SessionContextContributorKindDto::InstructionFile => 88,
        SessionContextContributorKindDto::ApprovedMemory => 82,
        SessionContextContributorKindDto::CompactionSummary => 78,
        SessionContextContributorKindDto::FileObservation => 76,
        SessionContextContributorKindDto::DependencyMetadata => 70,
        SessionContextContributorKindDto::CodeSymbol => 66,
        SessionContextContributorKindDto::ConversationTail => 62,
        SessionContextContributorKindDto::ToolSummary => 60,
        SessionContextContributorKindDto::ToolDescriptor => 58,
        SessionContextContributorKindDto::RunArtifact => 56,
        SessionContextContributorKindDto::ToolResult => 52,
        SessionContextContributorKindDto::SkillContext => 50,
        SessionContextContributorKindDto::ProviderUsage => 20,
    }
}

fn recency_score(sequence: u64, max_sequence: u64) -> u8 {
    if max_sequence <= 1 {
        return 100;
    }
    ((sequence.saturating_mul(100)) / max_sequence).min(100) as u8
}

fn relevance_score(
    contributor: &SessionContextContributorDto,
    terms: &BTreeSet<String>,
    latest_run_id: Option<&str>,
) -> u8 {
    let mut score: u8 = if is_required_context(&contributor.kind) {
        70
    } else {
        35
    };
    if contributor
        .run_id
        .as_deref()
        .zip(latest_run_id)
        .is_some_and(|(left, right)| left == right)
    {
        score += 20;
    }
    let haystack = format!(
        "{} {} {}",
        contributor.label,
        contributor.source_id.as_deref().unwrap_or_default(),
        contributor.text.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    let matches = terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count() as u8;
    score.saturating_add(matches.saturating_mul(8)).min(100)
}

fn task_phase_for_kind(kind: &SessionContextContributorKindDto) -> SessionContextTaskPhaseDto {
    match kind {
        SessionContextContributorKindDto::SystemPrompt
        | SessionContextContributorKindDto::InstructionFile
        | SessionContextContributorKindDto::ApprovedMemory
        | SessionContextContributorKindDto::SkillContext
        | SessionContextContributorKindDto::DependencyMetadata
        | SessionContextContributorKindDto::CodeSymbol => SessionContextTaskPhaseDto::ContextGather,
        SessionContextContributorKindDto::FileObservation
        | SessionContextContributorKindDto::ToolResult
        | SessionContextContributorKindDto::ToolSummary => SessionContextTaskPhaseDto::Execute,
        SessionContextContributorKindDto::CompactionSummary
        | SessionContextContributorKindDto::ConversationTail => SessionContextTaskPhaseDto::Intake,
        SessionContextContributorKindDto::RunArtifact => SessionContextTaskPhaseDto::RunArtifact,
        SessionContextContributorKindDto::ToolDescriptor => SessionContextTaskPhaseDto::Plan,
        SessionContextContributorKindDto::ProviderUsage => SessionContextTaskPhaseDto::Summarize,
    }
}

fn is_required_context(kind: &SessionContextContributorKindDto) -> bool {
    matches!(
        kind,
        SessionContextContributorKindDto::SystemPrompt
            | SessionContextContributorKindDto::InstructionFile
            | SessionContextContributorKindDto::ApprovedMemory
            | SessionContextContributorKindDto::CompactionSummary
            | SessionContextContributorKindDto::FileObservation
    )
}

fn summary_for_large_context_node(text: &str) -> String {
    let first_line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Large context node");
    format!(
        "{}... ({} chars summarized by context planner)",
        first_line.chars().take(180).collect::<String>(),
        text.chars().count()
    )
}

fn provider_request_hash(
    compilation: &PromptCompilation,
    contributors: &[SessionContextContributorDto],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(compilation.prompt.as_bytes());
    for contributor in contributors
        .iter()
        .filter(|contributor| contributor.included && contributor.model_visible)
    {
        hasher.update(contributor.contributor_id.as_bytes());
        hasher.update(contributor.estimated_tokens.to_le_bytes());
        if let Some(hash) = contributor.prompt_fragment_hash.as_ref() {
            hasher.update(hash.as_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

fn context_snapshot_diff(
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
    contributors: &[SessionContextContributorDto],
) -> Option<SessionContextSnapshotDiffDto> {
    let run_id = run_id?;
    let current_ids = contributors
        .iter()
        .filter(|contributor| contributor.run_id.as_deref() == Some(run_id))
        .map(|contributor| contributor.contributor_id.clone())
        .collect::<Vec<_>>();
    if current_ids.is_empty() {
        return None;
    }
    let estimated_tokens = contributors
        .iter()
        .filter(|contributor| contributor.run_id.as_deref() == Some(run_id))
        .map(|contributor| contributor.estimated_tokens as i64)
        .sum();
    Some(SessionContextSnapshotDiffDto {
        previous_snapshot_id: Some(format!(
            "context:{project_id}:{agent_session_id}:{run_id}:previous"
        )),
        added_contributor_ids: current_ids,
        removed_contributor_ids: Vec::new(),
        changed_contributor_ids: Vec::new(),
        estimated_token_delta: estimated_tokens,
        redaction: SessionContextRedactionDto::public(),
    })
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

fn build_project_code_map(repo_root: &Path) -> CommandResult<SessionContextCodeMapDto> {
    let mut source_roots = BTreeSet::new();
    let mut package_manifests = Vec::new();
    let mut symbols = Vec::new();
    collect_project_code_map(
        repo_root,
        repo_root,
        &mut source_roots,
        &mut package_manifests,
        &mut symbols,
        &mut 0,
    )?;
    let (root, root_redaction) = sanitize_context_path(repo_root.to_string_lossy().as_ref());
    let redaction = strongest_context_redaction(
        package_manifests
            .iter()
            .map(|manifest| &manifest.redaction)
            .chain(symbols.iter().map(|symbol| &symbol.redaction))
            .chain(std::iter::once(&root_redaction)),
    );
    Ok(SessionContextCodeMapDto {
        generated_from_root: root,
        source_roots: source_roots.into_iter().collect(),
        package_manifests,
        symbols,
        redaction,
    })
}

fn collect_project_code_map(
    repo_root: &Path,
    dir: &Path,
    source_roots: &mut BTreeSet<String>,
    package_manifests: &mut Vec<SessionContextDependencyManifestDto>,
    symbols: &mut Vec<SessionContextCodeSymbolDto>,
    files_seen: &mut usize,
) -> CommandResult<()> {
    if *files_seen >= MAX_CODE_MAP_FILES {
        return Ok(());
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        if *files_seen >= MAX_CODE_MAP_FILES {
            break;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip_code_map_path(&name) {
            continue;
        }
        if path.is_dir() {
            collect_project_code_map(
                repo_root,
                &path,
                source_roots,
                package_manifests,
                symbols,
                files_seen,
            )?;
            continue;
        }
        *files_seen = (*files_seen).saturating_add(1);
        if let Some(manifest) = dependency_manifest_from_path(repo_root, &path)? {
            package_manifests.push(manifest);
        }
        if symbols.len() >= MAX_CODE_SYMBOLS {
            continue;
        }
        if !is_symbol_source_file(&path) {
            continue;
        }
        if let Some(parent) = path
            .parent()
            .and_then(|parent| repo_relative_path(repo_root, parent))
        {
            if is_likely_source_root(&parent) {
                source_roots.insert(parent);
            }
        }
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        append_symbols_from_source(repo_root, &path, &content, symbols);
    }
    Ok(())
}

fn should_skip_code_map_path(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "target" | ".next" | "dist" | "build" | ".xero"
    )
}

fn dependency_manifest_from_path(
    repo_root: &Path,
    path: &Path,
) -> CommandResult<Option<SessionContextDependencyManifestDto>> {
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return Ok(None);
    };
    let ecosystem = match file_name {
        "package.json" => "node",
        "Cargo.toml" => "rust",
        "pyproject.toml" => "python",
        "requirements.txt" => "python",
        _ => return Ok(None),
    };
    let raw = fs::read_to_string(path).unwrap_or_default();
    let package_name = if file_name == "package.json" {
        serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|value| {
                value
                    .get("name")
                    .and_then(|name| name.as_str())
                    .map(str::to_string)
            })
    } else {
        manifest_name_from_toml_like(&raw)
    };
    let dependency_count = dependency_count_from_manifest(file_name, &raw);
    let (relative_path, redaction) = repo_relative_path(repo_root, path)
        .map(|value| sanitize_context_path(&value))
        .unwrap_or_else(|| sanitize_context_path(path.to_string_lossy().as_ref()));
    Ok(Some(SessionContextDependencyManifestDto {
        path: relative_path,
        ecosystem: ecosystem.into(),
        package_name,
        dependency_count,
        redaction,
    }))
}

fn manifest_name_from_toml_like(raw: &str) -> Option<String> {
    raw.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("name")
            .and_then(|rest| rest.trim_start().strip_prefix('='))
            .map(|value| value.trim().trim_matches('"').to_string())
            .filter(|value| !value.is_empty())
    })
}

fn dependency_count_from_manifest(file_name: &str, raw: &str) -> u64 {
    if file_name == "package.json" {
        return serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .map(|value| {
                [
                    "dependencies",
                    "devDependencies",
                    "peerDependencies",
                    "optionalDependencies",
                ]
                .iter()
                .filter_map(|key| value.get(*key).and_then(|deps| deps.as_object()))
                .map(|deps| deps.len() as u64)
                .sum()
            })
            .unwrap_or(0);
    }
    raw.lines()
        .filter(|line| line.trim_start().starts_with('[') && line.contains("dependencies"))
        .count() as u64
}

fn is_symbol_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("rs" | "ts" | "tsx" | "js" | "jsx")
    )
}

fn is_likely_source_root(path: &str) -> bool {
    path == "src" || path.ends_with("/src") || path.contains("/src/")
}

fn append_symbols_from_source(
    repo_root: &Path,
    path: &Path,
    content: &str,
    symbols: &mut Vec<SessionContextCodeSymbolDto>,
) {
    let relative_path =
        repo_relative_path(repo_root, path).unwrap_or_else(|| path.to_string_lossy().to_string());
    for (index, line) in content.lines().enumerate() {
        if symbols.len() >= MAX_CODE_SYMBOLS {
            return;
        }
        let trimmed = line.trim_start();
        let Some((kind, name)) = symbol_from_line(trimmed) else {
            continue;
        };
        let symbol_id = format!("{}:{}:{}", relative_path, index + 1, name);
        let (path, path_redaction) = sanitize_context_path(&relative_path);
        symbols.push(SessionContextCodeSymbolDto {
            symbol_id,
            name,
            kind,
            path,
            line: index as u64 + 1,
            estimated_tokens: estimate_tokens(trimmed),
            redaction: path_redaction,
        });
    }
}

fn symbol_from_line(line: &str) -> Option<(String, String)> {
    let normalized = line
        .strip_prefix("pub ")
        .or_else(|| line.strip_prefix("export "))
        .unwrap_or(line);
    for (prefix, kind) in [
        ("async fn ", "function"),
        ("fn ", "function"),
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("impl ", "impl"),
        ("mod ", "module"),
        ("function ", "function"),
        ("class ", "class"),
        ("interface ", "interface"),
        ("type ", "type"),
        ("const ", "constant"),
    ] {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            let name = rest
                .split(|character: char| {
                    character.is_whitespace()
                        || matches!(character, '(' | '<' | ':' | '=' | '{' | ';')
                })
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            if !name.is_empty() {
                return Some((kind.into(), name));
            }
        }
    }
    None
}

fn repo_relative_path(repo_root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(repo_root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .filter(|value| !value.is_empty())
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
    prompt_fragment_id: Option<&'a str>,
    prompt_fragment_priority: Option<u16>,
    prompt_fragment_hash: Option<&'a str>,
    prompt_fragment_provenance: Option<&'a str>,
    project_id: Option<&'a str>,
    agent_session_id: Option<&'a str>,
    run_id: Option<&'a str>,
    source_id: Option<&'a str>,
    raw_text: Option<&'a str>,
    estimate_text: Option<&'a str>,
    estimated_tokens: Option<u64>,
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
        prompt_fragment_id: parts.prompt_fragment_id.map(ToOwned::to_owned),
        prompt_fragment_priority: parts.prompt_fragment_priority,
        prompt_fragment_hash: parts.prompt_fragment_hash.map(ToOwned::to_owned),
        prompt_fragment_provenance: parts.prompt_fragment_provenance.map(ToOwned::to_owned),
        project_id: parts.project_id.map(ToOwned::to_owned),
        agent_session_id: parts.agent_session_id.map(ToOwned::to_owned),
        run_id: parts.run_id.map(ToOwned::to_owned),
        source_id: parts.source_id.map(ToOwned::to_owned),
        sequence: contributors.len() as u64 + 1,
        estimated_tokens: parts
            .estimated_tokens
            .or_else(|| parts.estimate_text.map(estimate_tokens))
            .unwrap_or(0),
        estimated_chars: char_text.chars().count() as u64,
        recency_score: 0,
        relevance_score: 0,
        authority_score: 0,
        rank_score: 0,
        task_phase: SessionContextTaskPhaseDto::ContextGather,
        disposition: if parts.included {
            SessionContextDispositionDto::Include
        } else {
            SessionContextDispositionDto::RetrieveOnDemand
        },
        included: parts.included,
        model_visible: parts.model_visible,
        summary: None,
        omitted_reason: None,
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

fn random_hex_suffix() -> String {
    let mut bytes = [0_u8; 4];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
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

fn sanitize_context_path(value: &str) -> (String, SessionContextRedactionDto) {
    let normalized = value.trim().replace('\\', "/");
    let lowered = normalized.to_ascii_lowercase();
    if normalized.starts_with("/Users/")
        || normalized.starts_with("/home/")
        || lowered.contains(":/users/")
        || lowered.contains(":/programdata/")
        || lowered.contains(":/windows/temp/")
        || lowered.starts_with("%appdata%/")
        || lowered.starts_with("%localappdata%/")
        || normalized.contains("/.ssh/")
        || normalized.contains("/.aws/")
    {
        return (
            normalized
                .split('/')
                .filter(|segment| !segment.is_empty())
                .rev()
                .take(3)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("/"),
            SessionContextRedactionDto {
                redaction_class: SessionContextRedactionClassDto::LocalPath,
                redacted: true,
                reason: Some("Local absolute path shortened for context metadata.".into()),
            },
        );
    }
    (normalized, SessionContextRedactionDto::public())
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
            "Xero found owned-agent run `{}` but it does not belong to session `{agent_session_id}`.",
            snapshot.run.run_id
        ),
    ))
}

fn missing_session_error(project_id: &str, agent_session_id: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_session_not_found",
        format!(
            "Xero could not find agent session `{agent_session_id}` for project `{project_id}`."
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
        "xero".into()
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
            contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
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
            contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
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
