use super::*;
use crate::runtime::{AutonomousSubagentRole, AutonomousSubagentWriteScope};

const MAX_AUTOMATIC_MEMORY_CANDIDATES: u8 = 8;
const MIN_AUTOMATIC_MEMORY_CONFIDENCE: u8 = 50;

pub(crate) fn append_message(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    role: AgentMessageRole,
    content: String,
) -> CommandResult<AgentMessageRecord> {
    project_store::append_agent_message(
        repo_root,
        &NewAgentMessageRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            role,
            content,
            created_at: now_timestamp(),
            attachments: Vec::new(),
        },
    )
}

pub(crate) fn append_user_message_with_attachments(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    content: String,
    attachments: Vec<project_store::NewMessageAttachmentInput>,
) -> CommandResult<AgentMessageRecord> {
    project_store::append_agent_message(
        repo_root,
        &NewAgentMessageRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            role: AgentMessageRole::User,
            content,
            created_at: now_timestamp(),
            attachments,
        },
    )
}

pub(crate) fn provider_attachments_from_records(
    records: &[project_store::AgentMessageAttachmentRecord],
) -> Vec<MessageAttachment> {
    records
        .iter()
        .map(|record| MessageAttachment {
            kind: match record.kind {
                project_store::AgentMessageAttachmentKind::Image => MessageAttachmentKind::Image,
                project_store::AgentMessageAttachmentKind::Document => {
                    MessageAttachmentKind::Document
                }
                project_store::AgentMessageAttachmentKind::Text => MessageAttachmentKind::Text,
            },
            absolute_path: PathBuf::from(&record.storage_path),
            media_type: record.media_type.clone(),
            original_name: record.original_name.clone(),
            size_bytes: record.size_bytes,
            width: record.width,
            height: record.height,
        })
        .collect()
}

pub(crate) fn message_attachments_to_inputs(
    attachments: &[MessageAttachment],
) -> Vec<project_store::NewMessageAttachmentInput> {
    attachments
        .iter()
        .map(|attachment| project_store::NewMessageAttachmentInput {
            kind: match attachment.kind {
                MessageAttachmentKind::Image => project_store::AgentMessageAttachmentKind::Image,
                MessageAttachmentKind::Document => {
                    project_store::AgentMessageAttachmentKind::Document
                }
                MessageAttachmentKind::Text => project_store::AgentMessageAttachmentKind::Text,
            },
            storage_path: attachment.absolute_path.to_string_lossy().into_owned(),
            media_type: attachment.media_type.clone(),
            original_name: attachment.original_name.clone(),
            size_bytes: attachment.size_bytes,
            width: attachment.width,
            height: attachment.height,
        })
        .collect()
}

pub(crate) fn append_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    event_kind: AgentRunEventKind,
    payload: JsonValue,
) -> CommandResult<AgentEventRecord> {
    let payload_json = serde_json::to_string(&payload).map_err(|error| {
        CommandError::system_fault(
            "agent_event_serialize_failed",
            format!("Xero could not serialize owned-agent event payload: {error}"),
        )
    })?;
    let event = project_store::append_agent_event(
        repo_root,
        &NewAgentEventRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            event_kind,
            payload_json,
            created_at: now_timestamp(),
        },
    )?;
    publish_agent_event(event.clone());
    Ok(event)
}

pub(crate) fn touch_agent_run_heartbeat(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<()> {
    project_store::touch_agent_run_heartbeat(repo_root, project_id, run_id, &now_timestamp())
}

pub(crate) fn capture_project_record_for_run(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    capture_terminal_summary_record(repo_root, snapshot)?;
    capture_final_answer_record(repo_root, snapshot)?;
    capture_latest_plan_record(repo_root, snapshot)?;
    capture_decision_records(repo_root, snapshot)?;
    capture_verification_record(repo_root, snapshot)?;
    capture_diagnostic_record(repo_root, snapshot)?;
    capture_debug_finding_record(repo_root, snapshot)?;
    Ok(())
}

fn capture_terminal_summary_record(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    let latest_assistant_message = snapshot
        .messages
        .iter()
        .rev()
        .find(|message| message.role == AgentMessageRole::Assistant);
    let file_paths = snapshot
        .file_changes
        .iter()
        .map(|change| change.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let final_text = latest_assistant_message
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty());
    let (record_kind, title, raw_text, visibility) = match final_text {
        Some(text) => (
            project_store::ProjectRecordKind::AgentHandoff,
            format!("{} run handoff", snapshot.run.runtime_agent_id.label()),
            text.to_string(),
            project_store::ProjectRecordVisibility::Retrieval,
        ),
        None => (
            project_store::ProjectRecordKind::Diagnostic,
            format!("{} run diagnostic", snapshot.run.runtime_agent_id.label()),
            "Run completed without a final assistant message to summarize.".to_string(),
            project_store::ProjectRecordVisibility::Diagnostic,
        ),
    };
    let (text, redaction) = redact_session_context_text(&raw_text);
    if text.trim().is_empty() {
        return Ok(());
    }
    let summary = trim_project_record_summary(&text);
    let content = json!({
        "schema": "xero.project_record.run_handoff.v1",
        "runtimeAgentId": snapshot.run.runtime_agent_id.as_str(),
        "providerId": snapshot.run.provider_id.as_str(),
        "modelId": snapshot.run.model_id.as_str(),
        "status": format!("{:?}", snapshot.run.status),
        "fileChanges": file_paths,
        "messageId": latest_assistant_message.map(|message| message.id),
    });
    project_store::insert_project_record(
        repo_root,
        &project_store::NewProjectRecordRecord {
            record_id: project_store::generate_project_record_id(),
            project_id: snapshot.run.project_id.clone(),
            record_kind,
            runtime_agent_id: snapshot.run.runtime_agent_id,
            agent_definition_id: snapshot.run.agent_definition_id.clone(),
            agent_definition_version: snapshot.run.agent_definition_version,
            agent_session_id: Some(snapshot.run.agent_session_id.clone()),
            run_id: snapshot.run.run_id.clone(),
            workflow_run_id: None,
            workflow_step_id: None,
            title,
            summary,
            text,
            content_json: Some(content),
            schema_name: Some("xero.project_record.run_handoff.v1".into()),
            schema_version: 1,
            importance: project_store::ProjectRecordImportance::Normal,
            confidence: Some(0.8),
            tags: vec![snapshot.run.runtime_agent_id.as_str().into()],
            source_item_ids: latest_assistant_message
                .map(|message| vec![format!("agent_messages:{}", message.id)])
                .unwrap_or_default(),
            related_paths: snapshot
                .file_changes
                .iter()
                .map(|change| change.path.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            produced_artifact_refs: Vec::new(),
            redaction_state: if redaction.redacted {
                project_store::ProjectRecordRedactionState::Redacted
            } else {
                project_store::ProjectRecordRedactionState::Clean
            },
            visibility,
            created_at: now_timestamp(),
        },
    )?;
    Ok(())
}

fn capture_final_answer_record(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    let Some(message) = snapshot
        .messages
        .iter()
        .rev()
        .find(|message| message.role == AgentMessageRole::Assistant)
    else {
        return Ok(());
    };
    let text = message.content.trim();
    if text.is_empty() {
        return Ok(());
    }
    insert_runtime_project_record(
        repo_root,
        snapshot,
        RuntimeProjectRecordDraft {
            record_kind: project_store::ProjectRecordKind::ContextNote,
            title: format!("{} final answer", snapshot.run.runtime_agent_id.label()),
            summary: trim_project_record_summary(text),
            text: text.to_string(),
            content_json: json!({
                "schema": "xero.project_record.final_answer.v1",
                "runtimeAgentId": snapshot.run.runtime_agent_id.as_str(),
                "status": format!("{:?}", snapshot.run.status),
                "messageId": message.id,
            }),
            schema_name: "xero.project_record.final_answer.v1",
            importance: project_store::ProjectRecordImportance::Normal,
            confidence: Some(0.82),
            tags: vec![
                snapshot.run.runtime_agent_id.as_str().into(),
                "final-answer".into(),
                "phase5".into(),
            ],
            source_item_ids: vec![format!("agent_messages:{}", message.id)],
            related_paths: run_related_paths(snapshot),
            visibility: project_store::ProjectRecordVisibility::Retrieval,
        },
    )
}

fn capture_latest_plan_record(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    let Some((event, payload)) = latest_event_payload(snapshot, AgentRunEventKind::PlanUpdated)
    else {
        return Ok(());
    };
    let text = serde_json::to_string_pretty(&payload).map_err(|error| {
        CommandError::system_fault(
            "agent_plan_record_serialize_failed",
            format!("Xero could not serialize the latest agent plan for persistence: {error}"),
        )
    })?;
    let summary = payload
        .get("summary")
        .and_then(JsonValue::as_str)
        .or_else(|| payload.get("title").and_then(JsonValue::as_str))
        .map(trim_project_record_summary)
        .unwrap_or_else(|| "Latest structured plan captured from the run.".into());
    insert_runtime_project_record(
        repo_root,
        snapshot,
        RuntimeProjectRecordDraft {
            record_kind: project_store::ProjectRecordKind::Plan,
            title: format!("{} run plan", snapshot.run.runtime_agent_id.label()),
            summary,
            text,
            content_json: json!({
                "schema": "xero.project_record.plan_capture.v1",
                "eventId": event.id,
                "payload": payload,
            }),
            schema_name: "xero.project_record.plan_capture.v1",
            importance: project_store::ProjectRecordImportance::Normal,
            confidence: Some(0.86),
            tags: vec![
                snapshot.run.runtime_agent_id.as_str().into(),
                "plan".into(),
                "phase5".into(),
            ],
            source_item_ids: vec![format!("agent_events:{}", event.id)],
            related_paths: run_related_paths(snapshot),
            visibility: project_store::ProjectRecordVisibility::Retrieval,
        },
    )
}

fn capture_decision_records(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    for (index, decision) in extract_marked_lines(snapshot, "Decision:", 6)
        .into_iter()
        .enumerate()
    {
        insert_runtime_project_record(
            repo_root,
            snapshot,
            RuntimeProjectRecordDraft {
                record_kind: project_store::ProjectRecordKind::Decision,
                title: format!(
                    "{} decision {}",
                    snapshot.run.runtime_agent_id.label(),
                    index + 1
                ),
                summary: trim_project_record_summary(&decision.text),
                text: decision.text,
                content_json: json!({
                    "schema": "xero.project_record.decision_capture.v1",
                    "source": decision.source,
                    "marker": "Decision:",
                }),
                schema_name: "xero.project_record.decision_capture.v1",
                importance: project_store::ProjectRecordImportance::High,
                confidence: Some(0.78),
                tags: vec![
                    snapshot.run.runtime_agent_id.as_str().into(),
                    "decision".into(),
                    "phase5".into(),
                ],
                source_item_ids: vec![decision.source],
                related_paths: run_related_paths(snapshot),
                visibility: project_store::ProjectRecordVisibility::Retrieval,
            },
        )?;
    }
    Ok(())
}

fn capture_verification_record(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    let verification_events = snapshot
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.event_kind,
                AgentRunEventKind::VerificationGate | AgentRunEventKind::ValidationCompleted
            )
        })
        .collect::<Vec<_>>();
    if verification_events.is_empty() {
        return Ok(());
    }
    let evidence = verification_events
        .iter()
        .filter_map(|event| {
            serde_json::from_str::<JsonValue>(&event.payload_json)
                .ok()
                .map(|payload| {
                    json!({
                        "eventId": event.id,
                        "eventKind": event.event_kind.clone(),
                        "createdAt": event.created_at.clone(),
                        "payload": payload,
                    })
                })
        })
        .collect::<Vec<_>>();
    let text = serde_json::to_string_pretty(&evidence).map_err(|error| {
        CommandError::system_fault(
            "agent_verification_record_serialize_failed",
            format!("Xero could not serialize verification evidence for persistence: {error}"),
        )
    })?;
    insert_runtime_project_record(
        repo_root,
        snapshot,
        RuntimeProjectRecordDraft {
            record_kind: project_store::ProjectRecordKind::Verification,
            title: format!(
                "{} verification evidence",
                snapshot.run.runtime_agent_id.label()
            ),
            summary: format!(
                "{} verification event{} captured.",
                verification_events.len(),
                if verification_events.len() == 1 {
                    ""
                } else {
                    "s"
                }
            ),
            text,
            content_json: json!({
                "schema": "xero.project_record.verification_capture.v1",
                "events": evidence,
            }),
            schema_name: "xero.project_record.verification_capture.v1",
            importance: project_store::ProjectRecordImportance::High,
            confidence: Some(0.9),
            tags: vec![
                snapshot.run.runtime_agent_id.as_str().into(),
                "verification".into(),
                "phase5".into(),
            ],
            source_item_ids: verification_events
                .iter()
                .map(|event| format!("agent_events:{}", event.id))
                .collect(),
            related_paths: run_related_paths(snapshot),
            visibility: project_store::ProjectRecordVisibility::Retrieval,
        },
    )
}

fn capture_diagnostic_record(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    let Some(error) = snapshot.run.last_error.as_ref() else {
        return Ok(());
    };
    insert_runtime_project_record(
        repo_root,
        snapshot,
        RuntimeProjectRecordDraft {
            record_kind: project_store::ProjectRecordKind::Diagnostic,
            title: format!("{} run diagnostic", snapshot.run.runtime_agent_id.label()),
            summary: format!(
                "{}: {}",
                error.code,
                trim_project_record_summary(&error.message)
            ),
            text: format!("{}: {}", error.code, error.message),
            content_json: json!({
                "schema": "xero.project_record.run_diagnostic.v1",
                "status": format!("{:?}", snapshot.run.status),
                "code": error.code,
                "message": error.message,
            }),
            schema_name: "xero.project_record.run_diagnostic.v1",
            importance: project_store::ProjectRecordImportance::High,
            confidence: Some(0.95),
            tags: vec![
                snapshot.run.runtime_agent_id.as_str().into(),
                "diagnostic".into(),
                "phase5".into(),
            ],
            source_item_ids: vec![format!("agent_runs:{}", snapshot.run.run_id)],
            related_paths: run_related_paths(snapshot),
            visibility: project_store::ProjectRecordVisibility::Diagnostic,
        },
    )
}

fn capture_debug_finding_record(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    if snapshot.run.runtime_agent_id != RuntimeAgentIdDto::Debug {
        return Ok(());
    }
    let Some(message) = snapshot
        .messages
        .iter()
        .rev()
        .find(|message| message.role == AgentMessageRole::Assistant)
    else {
        return Ok(());
    };
    let text = message.content.trim();
    if text.is_empty() {
        return Ok(());
    }
    let lowered = text.to_ascii_lowercase();
    if !["root cause", "finding", "fix", "verified", "verification"]
        .iter()
        .any(|needle| lowered.contains(needle))
    {
        return Ok(());
    }
    insert_runtime_project_record(
        repo_root,
        snapshot,
        RuntimeProjectRecordDraft {
            record_kind: project_store::ProjectRecordKind::Finding,
            title: "Debug finding".into(),
            summary: trim_project_record_summary(text),
            text: text.to_string(),
            content_json: json!({
                "schema": "xero.project_record.debug_finding.v1",
                "messageId": message.id,
                "status": format!("{:?}", snapshot.run.status),
            }),
            schema_name: "xero.project_record.debug_finding.v1",
            importance: project_store::ProjectRecordImportance::High,
            confidence: Some(0.84),
            tags: vec![
                "debug".into(),
                "finding".into(),
                "root-cause".into(),
                "phase5".into(),
            ],
            source_item_ids: vec![format!("agent_messages:{}", message.id)],
            related_paths: run_related_paths(snapshot),
            visibility: project_store::ProjectRecordVisibility::Retrieval,
        },
    )
}

pub(crate) fn capture_memory_candidates_for_run(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
    provider: &dyn ProviderAdapter,
    trigger: &str,
) -> CommandResult<()> {
    let source = build_runtime_memory_extraction_source(snapshot);
    if source.transcript.trim().is_empty() {
        return Ok(());
    }
    let existing_memories = project_store::list_agent_memories(
        repo_root,
        &snapshot.run.project_id,
        project_store::AgentMemoryListFilter {
            agent_session_id: Some(&snapshot.run.agent_session_id),
            include_disabled: true,
            include_rejected: false,
        },
    )?;
    let request = ProviderMemoryExtractionRequest {
        project_id: snapshot.run.project_id.clone(),
        agent_session_id: snapshot.run.agent_session_id.clone(),
        run_id: Some(snapshot.run.run_id.clone()),
        provider_id: provider.provider_id().into(),
        model_id: provider.model_id().into(),
        transcript: source.transcript.clone(),
        existing_memories: existing_memories
            .iter()
            .map(|memory| memory.text.clone())
            .collect(),
        max_candidates: MAX_AUTOMATIC_MEMORY_CANDIDATES,
    };
    let mut ignored_stream_event = |_event| Ok(());
    let outcome = match provider.extract_memory_candidates(&request, &mut ignored_stream_event) {
        Ok(outcome) => outcome,
        Err(error) => {
            record_memory_extraction_diagnostics(
                repo_root,
                snapshot,
                trigger,
                0,
                0,
                &[project_store::AgentRunDiagnosticRecord {
                    code: error.code.clone(),
                    message: error.message.clone(),
                }],
            )?;
            append_event(
                repo_root,
                &snapshot.run.project_id,
                &snapshot.run.run_id,
                AgentRunEventKind::ValidationCompleted,
                json!({
                    "label": "memory_extraction",
                    "outcome": "failed",
                    "trigger": trigger,
                    "code": error.code,
                    "message": error.message,
                }),
            )?;
            return Ok(());
        }
    };

    let mut created_count = 0_usize;
    let mut skipped_duplicate_count = 0_usize;
    let mut diagnostics = Vec::new();
    let now = now_timestamp();
    for candidate in outcome
        .candidates
        .into_iter()
        .take(MAX_AUTOMATIC_MEMORY_CANDIDATES as usize)
    {
        match prepare_automatic_memory_candidate(
            &snapshot.run.project_id,
            &snapshot.run.agent_session_id,
            &source,
            candidate,
            now.as_str(),
        ) {
            Ok(record) => {
                let text_hash = project_store::agent_memory_text_hash(&record.text);
                if project_store::find_active_agent_memory_by_hash(
                    repo_root,
                    &snapshot.run.project_id,
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
                project_store::insert_agent_memory(repo_root, &record)?;
                created_count = created_count.saturating_add(1);
            }
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }

    append_event(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.run_id,
        AgentRunEventKind::ValidationCompleted,
        json!({
            "label": "memory_extraction",
            "outcome": "passed",
            "trigger": trigger,
            "createdCount": created_count,
            "skippedDuplicateCount": skipped_duplicate_count,
            "rejectedCount": diagnostics.len(),
        }),
    )?;
    if !diagnostics.is_empty() {
        record_memory_extraction_diagnostics(
            repo_root,
            snapshot,
            trigger,
            created_count,
            skipped_duplicate_count,
            &diagnostics,
        )?;
    }
    Ok(())
}

fn trim_project_record_summary(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= 240 {
        return trimmed.to_string();
    }
    let mut summary = trimmed.chars().take(240).collect::<String>();
    summary.push_str("...");
    summary
}

fn truncate_memory_source_text(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut truncated = trimmed.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

struct RuntimeProjectRecordDraft {
    record_kind: project_store::ProjectRecordKind,
    title: String,
    summary: String,
    text: String,
    content_json: JsonValue,
    schema_name: &'static str,
    importance: project_store::ProjectRecordImportance,
    confidence: Option<f64>,
    tags: Vec<String>,
    source_item_ids: Vec<String>,
    related_paths: Vec<String>,
    visibility: project_store::ProjectRecordVisibility,
}

fn insert_runtime_project_record(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
    draft: RuntimeProjectRecordDraft,
) -> CommandResult<()> {
    let (text, redaction) = redact_session_context_text(&draft.text);
    if text.trim().is_empty() {
        return Ok(());
    }
    let (summary, summary_redaction) = redact_session_context_text(&draft.summary);
    let (content_json, content_redacted) = redact_runtime_project_record_json(draft.content_json);
    let redaction_state = if redaction.redacted || summary_redaction.redacted || content_redacted {
        project_store::ProjectRecordRedactionState::Redacted
    } else {
        project_store::ProjectRecordRedactionState::Clean
    };
    project_store::insert_project_record(
        repo_root,
        &project_store::NewProjectRecordRecord {
            record_id: project_store::generate_project_record_id(),
            project_id: snapshot.run.project_id.clone(),
            record_kind: draft.record_kind,
            runtime_agent_id: snapshot.run.runtime_agent_id,
            agent_definition_id: snapshot.run.agent_definition_id.clone(),
            agent_definition_version: snapshot.run.agent_definition_version,
            agent_session_id: Some(snapshot.run.agent_session_id.clone()),
            run_id: snapshot.run.run_id.clone(),
            workflow_run_id: None,
            workflow_step_id: None,
            title: draft.title,
            summary: trim_project_record_summary(&summary),
            text,
            content_json: Some(content_json),
            schema_name: Some(draft.schema_name.into()),
            schema_version: 1,
            importance: draft.importance,
            confidence: draft.confidence,
            tags: draft.tags,
            source_item_ids: draft.source_item_ids,
            related_paths: draft.related_paths,
            produced_artifact_refs: Vec::new(),
            redaction_state,
            visibility: draft.visibility,
            created_at: now_timestamp(),
        },
    )?;
    Ok(())
}

fn redact_runtime_project_record_json(value: JsonValue) -> (JsonValue, bool) {
    redact_runtime_project_record_json_value(value, None)
}

fn redact_runtime_project_record_json_value(
    value: JsonValue,
    parent_key: Option<&str>,
) -> (JsonValue, bool) {
    match value {
        JsonValue::String(text) => {
            if parent_key.is_some_and(is_project_record_metadata_json_key) {
                return (JsonValue::String(text), false);
            }
            let (text, redaction) = redact_session_context_text(&text);
            (JsonValue::String(text), redaction.redacted)
        }
        JsonValue::Array(items) => {
            let mut redacted_any = false;
            let items = items
                .into_iter()
                .map(|item| {
                    let (item, redacted) =
                        redact_runtime_project_record_json_value(item, parent_key);
                    redacted_any |= redacted;
                    item
                })
                .collect();
            (JsonValue::Array(items), redacted_any)
        }
        JsonValue::Object(entries) => {
            let mut redacted_any = false;
            let entries = entries
                .into_iter()
                .map(|(key, value)| {
                    let (value, redacted) =
                        redact_runtime_project_record_json_value(value, Some(key.as_str()));
                    redacted_any |= redacted;
                    (key, value)
                })
                .collect();
            (JsonValue::Object(entries), redacted_any)
        }
        value => (value, false),
    }
}

fn is_project_record_metadata_json_key(key: &str) -> bool {
    matches!(
        key,
        "actionId"
            | "code"
            | "createdAt"
            | "eventId"
            | "eventKind"
            | "marker"
            | "messageId"
            | "modelId"
            | "providerId"
            | "runtimeAgentId"
            | "schema"
            | "source"
            | "status"
            | "trigger"
    )
}

fn run_related_paths(snapshot: &AgentRunSnapshotRecord) -> Vec<String> {
    snapshot
        .file_changes
        .iter()
        .map(|change| change.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn latest_event_payload(
    snapshot: &AgentRunSnapshotRecord,
    event_kind: AgentRunEventKind,
) -> Option<(&AgentEventRecord, JsonValue)> {
    snapshot
        .events
        .iter()
        .rev()
        .find(|event| event.event_kind == event_kind)
        .and_then(|event| {
            serde_json::from_str::<JsonValue>(&event.payload_json)
                .ok()
                .map(|payload| (event, payload))
        })
}

struct MarkedLine {
    source: String,
    text: String,
}

fn extract_marked_lines(
    snapshot: &AgentRunSnapshotRecord,
    marker: &str,
    limit: usize,
) -> Vec<MarkedLine> {
    let mut lines = Vec::new();
    for message in snapshot.messages.iter().rev() {
        if !matches!(
            message.role,
            AgentMessageRole::Assistant | AgentMessageRole::Developer | AgentMessageRole::User
        ) {
            continue;
        }
        for line in message.content.lines() {
            let Some(text) = text_after_marker(line, marker) else {
                continue;
            };
            lines.push(MarkedLine {
                source: format!("agent_messages:{}", message.id),
                text: text.to_string(),
            });
            if lines.len() >= limit {
                return lines;
            }
        }
    }
    for event in snapshot.events.iter().rev() {
        let Some(text) = text_after_marker(&event.payload_json, marker) else {
            continue;
        };
        lines.push(MarkedLine {
            source: format!("agent_events:{}", event.id),
            text: text.to_string(),
        });
        if lines.len() >= limit {
            break;
        }
    }
    lines
}

fn text_after_marker<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let lowered = line.to_ascii_lowercase();
    let marker = marker.to_ascii_lowercase();
    let index = lowered.find(&marker)?;
    line.get(index.saturating_add(marker.len())..)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

struct RuntimeMemoryExtractionSource {
    transcript: String,
    source_run_id: String,
    source_item_ids: Vec<String>,
}

fn build_runtime_memory_extraction_source(
    snapshot: &AgentRunSnapshotRecord,
) -> RuntimeMemoryExtractionSource {
    let transcript = crate::commands::run_transcript_from_agent_snapshot(snapshot, None);
    let mut source_item_ids = Vec::new();
    let mut text = format!(
        "Review this Xero owned-agent run for durable memory candidates. Run {} provider={} model={} status={:?}.\n",
        snapshot.run.run_id,
        snapshot.run.provider_id,
        snapshot.run.model_id,
        snapshot.run.status,
    );
    for item in &transcript.items {
        source_item_ids.push(item.item_id.clone());
        let body = item
            .text
            .as_deref()
            .or(item.summary.as_deref())
            .unwrap_or_default()
            .trim();
        if body.is_empty() {
            continue;
        }
        text.push_str(&format!(
            "- [{}] {:?} {:?}: {}\n",
            item.item_id,
            item.kind,
            item.actor,
            truncate_memory_source_text(body, 600)
        ));
    }
    RuntimeMemoryExtractionSource {
        transcript: text,
        source_run_id: snapshot.run.run_id.clone(),
        source_item_ids,
    }
}

fn prepare_automatic_memory_candidate(
    project_id: &str,
    agent_session_id: &str,
    source: &RuntimeMemoryExtractionSource,
    candidate: ProviderMemoryCandidate,
    created_at: &str,
) -> Result<project_store::NewAgentMemoryRecord, project_store::AgentRunDiagnosticRecord> {
    let scope = agent_memory_scope_from_provider(&candidate.scope).ok_or_else(|| {
        agent_memory_candidate_diagnostic(
            "session_memory_candidate_scope_invalid",
            "A provider memory candidate used an unsupported scope.",
        )
    })?;
    let kind = agent_memory_kind_from_provider(&candidate.kind).ok_or_else(|| {
        agent_memory_candidate_diagnostic(
            "session_memory_candidate_kind_invalid",
            "A provider memory candidate used an unsupported kind.",
        )
    })?;
    let text = candidate.text.trim().to_string();
    if text.is_empty() {
        return Err(agent_memory_candidate_diagnostic(
            "session_memory_candidate_empty",
            "A provider memory candidate did not include text.",
        ));
    }
    let confidence = candidate.confidence.unwrap_or(0).min(100);
    if confidence < MIN_AUTOMATIC_MEMORY_CONFIDENCE {
        return Err(agent_memory_candidate_diagnostic(
            "session_memory_candidate_low_confidence",
            "Xero skipped a low-confidence memory candidate.",
        ));
    }
    let (_redacted_text, redaction) = redact_session_context_text(&text);
    if redaction.redacted {
        return Err(memory_candidate_blocked_diagnostic(&redaction));
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
    Ok(project_store::NewAgentMemoryRecord {
        memory_id: project_store::generate_agent_memory_id(),
        project_id: project_id.into(),
        agent_session_id: match scope {
            project_store::AgentMemoryScope::Project => None,
            project_store::AgentMemoryScope::Session => Some(agent_session_id.into()),
        },
        scope,
        kind,
        text,
        review_state: project_store::AgentMemoryReviewState::Approved,
        enabled: true,
        confidence: Some(confidence),
        source_run_id: Some(source.source_run_id.clone()),
        source_item_ids,
        diagnostic: None,
        created_at: created_at.into(),
    })
}

fn record_memory_extraction_diagnostics(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
    trigger: &str,
    created_count: usize,
    skipped_duplicate_count: usize,
    diagnostics: &[project_store::AgentRunDiagnosticRecord],
) -> CommandResult<()> {
    if diagnostics.is_empty() {
        return Ok(());
    }
    let text = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>()
        .join("\n");
    insert_runtime_project_record(
        repo_root,
        snapshot,
        RuntimeProjectRecordDraft {
            record_kind: project_store::ProjectRecordKind::Diagnostic,
            title: "Memory extraction diagnostics".into(),
            summary: format!(
                "{} candidate{} rejected during {trigger} extraction.",
                diagnostics.len(),
                if diagnostics.len() == 1 { "" } else { "s" }
            ),
            text,
            content_json: json!({
                "schema": "xero.memory_extraction.diagnostics.v1",
                "trigger": trigger,
                "createdCount": created_count,
                "skippedDuplicateCount": skipped_duplicate_count,
                "rejectedCount": diagnostics.len(),
                "diagnostics": diagnostics.iter().map(|diagnostic| json!({
                    "code": diagnostic.code,
                    "message": diagnostic.message,
                })).collect::<Vec<_>>(),
            }),
            schema_name: "xero.memory_extraction.diagnostics.v1",
            importance: project_store::ProjectRecordImportance::Normal,
            confidence: Some(1.0),
            tags: vec![
                snapshot.run.runtime_agent_id.as_str().into(),
                "memory-extraction".into(),
                "diagnostic".into(),
                "phase5".into(),
            ],
            source_item_ids: vec![format!("agent_runs:{}", snapshot.run.run_id)],
            related_paths: run_related_paths(snapshot),
            visibility: project_store::ProjectRecordVisibility::Diagnostic,
        },
    )
}

fn agent_memory_candidate_diagnostic(
    code: impl Into<String>,
    message: impl Into<String>,
) -> project_store::AgentRunDiagnosticRecord {
    project_store::AgentRunDiagnosticRecord {
        code: code.into(),
        message: message.into(),
    }
}

fn memory_candidate_blocked_diagnostic(
    redaction: &crate::commands::SessionContextRedactionDto,
) -> project_store::AgentRunDiagnosticRecord {
    if redaction
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("prompt-injection"))
    {
        agent_memory_candidate_diagnostic(
            "session_memory_candidate_integrity",
            "Xero skipped a memory candidate because it looked like an instruction-override attempt.",
        )
    } else {
        agent_memory_candidate_diagnostic(
            "session_memory_candidate_secret",
            "Xero skipped a memory candidate because its text looked credential-like.",
        )
    }
}

fn agent_memory_scope_from_provider(value: &str) -> Option<project_store::AgentMemoryScope> {
    match value.trim().to_ascii_lowercase().as_str() {
        "project" => Some(project_store::AgentMemoryScope::Project),
        "session" => Some(project_store::AgentMemoryScope::Session),
        _ => None,
    }
}

fn agent_memory_kind_from_provider(value: &str) -> Option<project_store::AgentMemoryKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "project_fact" | "project fact" | "fact" => {
            Some(project_store::AgentMemoryKind::ProjectFact)
        }
        "user_preference" | "user preference" | "preference" => {
            Some(project_store::AgentMemoryKind::UserPreference)
        }
        "decision" => Some(project_store::AgentMemoryKind::Decision),
        "session_summary" | "session summary" | "summary" => {
            Some(project_store::AgentMemoryKind::SessionSummary)
        }
        "troubleshooting" | "troubleshooting_fact" | "troubleshooting fact" => {
            Some(project_store::AgentMemoryKind::Troubleshooting)
        }
        _ => None,
    }
}

pub(crate) fn repo_fingerprint(repo_root: &Path) -> JsonValue {
    match git2::Repository::discover(repo_root) {
        Ok(repository) => {
            let head = repository
                .head()
                .ok()
                .and_then(|head| head.target())
                .map(|oid| oid.to_string());
            let dirty = repository
                .statuses(None)
                .map(|statuses| !statuses.is_empty())
                .unwrap_or(false);
            json!({
                "kind": "git",
                "head": head,
                "dirty": dirty,
            })
        }
        Err(_) => json!({ "kind": "filesystem" }),
    }
}

pub(crate) fn record_file_change_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    write_observations: &[AgentWorkspaceWriteObservation],
    output: &AutonomousToolOutput,
) -> CommandResult<()> {
    if let AutonomousToolOutput::Patch(output) = output {
        if !output.applied {
            return Ok(());
        }
        for file in &output.files {
            record_single_file_change_event(
                repo_root,
                project_id,
                run_id,
                FileChangeEvent {
                    operation: "patch",
                    path: file.path.as_str(),
                    to_path: None,
                    old_hash: Some(file.old_hash.clone()),
                    new_hash: Some(file.new_hash.clone()),
                },
            )?;
        }
        return Ok(());
    }

    let (operation, path) = match output {
        AutonomousToolOutput::Write(output) => (
            if output.created { "create" } else { "write" },
            output.path.as_str(),
        ),
        AutonomousToolOutput::Edit(output) => ("edit", output.path.as_str()),
        AutonomousToolOutput::NotebookEdit(output) => ("notebook_edit", output.path.as_str()),
        AutonomousToolOutput::Delete(output) => ("delete", output.path.as_str()),
        AutonomousToolOutput::Rename(output) => ("rename", output.from_path.as_str()),
        AutonomousToolOutput::Mkdir(output) => ("mkdir", output.path.as_str()),
        _ => return Ok(()),
    };

    let new_hash_path = match output {
        AutonomousToolOutput::Rename(output) => output.to_path.as_str(),
        _ => path,
    };
    let new_hash = file_hash_if_present(repo_root, new_hash_path)?;
    let to_path = match output {
        AutonomousToolOutput::Rename(output) => Some(output.to_path.clone()),
        _ => None,
    };
    let old_hash = old_hash_for_path(write_observations, path);
    record_single_file_change_event(
        repo_root,
        project_id,
        run_id,
        FileChangeEvent {
            operation,
            path,
            to_path,
            old_hash,
            new_hash,
        },
    )
}

struct FileChangeEvent<'a> {
    operation: &'a str,
    path: &'a str,
    to_path: Option<String>,
    old_hash: Option<String>,
    new_hash: Option<String>,
}

fn record_single_file_change_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    change: FileChangeEvent<'_>,
) -> CommandResult<()> {
    project_store::append_agent_file_change(
        repo_root,
        &NewAgentFileChangeRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            path: change.path.into(),
            operation: change.operation.into(),
            old_hash: change.old_hash.clone(),
            new_hash: change.new_hash.clone(),
            created_at: now_timestamp(),
        },
    )?;
    let mut touched_paths = vec![change.path.to_string()];
    if let Some(to_path) = change.to_path.as_ref() {
        touched_paths.push(to_path.clone());
    }
    let freshness_checked_at = now_timestamp();
    project_store::refresh_project_record_freshness_for_paths(
        repo_root,
        project_id,
        &touched_paths,
        &freshness_checked_at,
    )?;
    project_store::refresh_agent_memory_freshness_for_paths(
        repo_root,
        project_id,
        &touched_paths,
        &freshness_checked_at,
    )?;

    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::FileChanged,
        json!({
            "path": change.path,
            "operation": change.operation,
            "toPath": change.to_path,
            "oldHash": change.old_hash,
            "newHash": change.new_hash,
        }),
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct AgentWorkspaceWriteObservation {
    path: String,
    old_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentRollbackCheckpoint {
    path: String,
    operation: String,
    old_hash: Option<String>,
    old_content_base64: Option<String>,
    old_content_omitted_reason: Option<String>,
    old_content_bytes: Option<u64>,
}

pub(crate) fn rollback_checkpoints_for_request(
    repo_root: &Path,
    request: &AutonomousToolRequest,
    observations: &[AgentWorkspaceWriteObservation],
) -> CommandResult<Vec<AgentRollbackCheckpoint>> {
    let mut checkpoints = Vec::new();
    for (path, operation) in planned_file_change_operations(request) {
        let Some(path_key) = relative_path_key(path) else {
            continue;
        };
        let old_hash = old_hash_for_path(observations, &path_key);
        let operation = if matches!(request, AutonomousToolRequest::Write(_)) {
            if old_hash.is_some() {
                "write"
            } else {
                "create"
            }
        } else {
            operation
        };
        let old_content = match old_hash.as_deref() {
            Some(_) => capture_rollback_content(repo_root, &path_key)?,
            None => RollbackContentCapture::NotNeeded,
        };
        let (old_content_base64, old_content_omitted_reason, old_content_bytes) = match old_content
        {
            RollbackContentCapture::Captured { base64, bytes } => (Some(base64), None, Some(bytes)),
            RollbackContentCapture::Omitted { reason, bytes } => (None, Some(reason), bytes),
            RollbackContentCapture::NotNeeded => (None, None, None),
        };

        checkpoints.push(AgentRollbackCheckpoint {
            path: path_key,
            operation: operation.into(),
            old_hash,
            old_content_base64,
            old_content_omitted_reason,
            old_content_bytes,
        });
    }
    Ok(checkpoints)
}

pub(crate) fn record_rollback_checkpoints(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_call_id: &str,
    checkpoints: &[AgentRollbackCheckpoint],
) -> CommandResult<()> {
    for checkpoint in checkpoints {
        let payload_json = serde_json::to_string(&json!({
            "kind": "file_rollback",
            "toolCallId": tool_call_id,
            "path": checkpoint.path.clone(),
            "operation": checkpoint.operation.clone(),
            "oldHash": checkpoint.old_hash.clone(),
            "oldContentBase64": checkpoint.old_content_base64.clone(),
            "oldContentOmittedReason": checkpoint.old_content_omitted_reason.clone(),
            "oldContentBytes": checkpoint.old_content_bytes,
        }))
        .map_err(|error| {
            CommandError::system_fault(
                "agent_checkpoint_payload_serialize_failed",
                format!("Xero could not serialize owned-agent rollback checkpoint: {error}"),
            )
        })?;

        project_store::append_agent_checkpoint(
            repo_root,
            &NewAgentCheckpointRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                checkpoint_kind: "tool".into(),
                summary: format!("Rollback data for `{}`.", checkpoint.path),
                payload_json: Some(payload_json),
                created_at: now_timestamp(),
            },
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RollbackContentCapture {
    Captured { base64: String, bytes: u64 },
    Omitted { reason: String, bytes: Option<u64> },
    NotNeeded,
}

fn capture_rollback_content(
    repo_root: &Path,
    repo_relative_path: &str,
) -> CommandResult<RollbackContentCapture> {
    use base64::Engine as _;

    let Some(relative_path) = safe_relative_path(repo_relative_path) else {
        return Err(CommandError::new(
            "agent_file_path_invalid",
            CommandErrorClass::PolicyDenied,
            format!(
                "Xero refused to capture rollback data for `{repo_relative_path}` because it is not a safe repo-relative path."
            ),
            false,
        ));
    };
    let path = repo_root.join(relative_path);
    if is_sensitive_rollback_path(repo_relative_path) {
        return Ok(RollbackContentCapture::Omitted {
            reason: "sensitive_path".into(),
            bytes: fs::metadata(&path).ok().map(|metadata| metadata.len()),
        });
    }
    let metadata = fs::metadata(&path).map_err(|error| {
        CommandError::retryable(
            "agent_rollback_read_failed",
            format!(
                "Xero could not inspect rollback data for {}: {error}",
                path.display()
            ),
        )
    })?;
    if metadata.len() > MAX_ROLLBACK_CONTENT_BYTES {
        return Ok(RollbackContentCapture::Omitted {
            reason: "file_too_large".into(),
            bytes: Some(metadata.len()),
        });
    }
    let bytes = fs::read(&path).map_err(|error| {
        CommandError::retryable(
            "agent_rollback_read_failed",
            format!(
                "Xero could not capture rollback data for {}: {error}",
                path.display()
            ),
        )
    })?;
    let text = String::from_utf8_lossy(&bytes);
    if find_prohibited_persistence_content(&text).is_some() {
        return Ok(RollbackContentCapture::Omitted {
            reason: "sensitive_content".into(),
            bytes: Some(bytes.len() as u64),
        });
    }
    Ok(RollbackContentCapture::Captured {
        base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        bytes: bytes.len() as u64,
    })
}

fn is_sensitive_rollback_path(repo_relative_path: &str) -> bool {
    let normalized = repo_relative_path.replace('\\', "/").to_ascii_lowercase();
    let file_name = Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(normalized.as_str());

    file_name == ".env"
        || file_name.starts_with(".env.")
        || matches!(
            file_name,
            "id_rsa"
                | "id_dsa"
                | "id_ecdsa"
                | "id_ed25519"
                | "credentials"
                | "credentials.json"
                | "secrets.json"
                | "service-account.json"
        )
        || normalized.contains("/.ssh/")
        || normalized.contains("/.aws/")
        || normalized.contains("/.gnupg/")
        || normalized.contains("secret")
        || normalized.contains("credential")
        || normalized.contains("private_key")
        || normalized.contains("private-key")
        || normalized.ends_with(".pem")
        || normalized.ends_with(".key")
        || normalized.ends_with(".p12")
        || normalized.ends_with(".pfx")
}

pub(crate) fn record_command_output_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    output: &AutonomousToolOutput,
) -> CommandResult<()> {
    match output {
        AutonomousToolOutput::Command(output) => {
            let argv = redact_command_argv_for_persistence(&output.argv);
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::CommandOutput,
                json!({
                    "argv": argv.clone(),
                    "cwd": output.cwd.clone(),
                    "stdout": output.stdout.clone(),
                    "stderr": output.stderr.clone(),
                    "stdoutTruncated": output.stdout_truncated,
                    "stderrTruncated": output.stderr_truncated,
                    "stdoutRedacted": output.stdout_redacted,
                    "stderrRedacted": output.stderr_redacted,
                    "exitCode": output.exit_code,
                    "timedOut": output.timed_out,
                    "spawned": output.spawned,
                    "policy": output.policy.clone(),
                }),
            )?;

            if !output.spawned {
                record_command_action_required(
                    repo_root,
                    project_id,
                    run_id,
                    "command",
                    &argv,
                    &output.policy.reason,
                    &output.policy.code,
                )?;
            }
        }
        AutonomousToolOutput::CommandSession(output) => {
            let argv = redact_command_argv_for_persistence(&output.argv);
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::CommandOutput,
                json!({
                    "operation": output.operation.clone(),
                    "sessionId": output.session_id.clone(),
                    "argv": argv.clone(),
                    "cwd": output.cwd.clone(),
                    "running": output.running,
                    "exitCode": output.exit_code,
                    "spawned": output.spawned,
                    "chunks": output.chunks.clone(),
                    "nextSequence": output.next_sequence,
                    "policy": output.policy.clone(),
                }),
            )?;

            if !output.spawned {
                if let Some(policy) = output.policy.as_ref() {
                    record_command_action_required(
                        repo_root,
                        project_id,
                        run_id,
                        "command_session_start",
                        &argv,
                        &policy.reason,
                        &policy.code,
                    )?;
                }
            }
        }
        AutonomousToolOutput::ProcessManager(output) => {
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::CommandOutput,
                json!({
                    "operation": output.action.clone(),
                    "processId": output.process_id.clone(),
                    "spawned": output.spawned,
                    "processes": output.processes.clone(),
                    "systemPorts": output.system_ports.clone(),
                    "chunks": output.chunks.clone(),
                    "nextCursor": output.next_cursor,
                    "policy": output.policy.clone(),
                }),
            )?;

            if !output.spawned
                && matches!(
                    output.action,
                    AutonomousProcessManagerAction::Start
                        | AutonomousProcessManagerAction::AsyncStart
                        | AutonomousProcessManagerAction::SystemSignal
                        | AutonomousProcessManagerAction::SystemKillTree
                )
            {
                if let Some(process) = output.processes.first() {
                    let argv = redact_command_argv_for_persistence(&process.command.argv);
                    record_command_action_required(
                        repo_root,
                        project_id,
                        run_id,
                        "process_manager",
                        &argv,
                        &output.policy.reason,
                        &output.policy.code,
                    )?;
                }
            }
        }
        AutonomousToolOutput::SystemDiagnostics(output) => {
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::CommandOutput,
                json!({
                    "operation": output.action.clone(),
                    "performed": output.performed,
                    "platformSupported": output.platform_supported,
                    "target": output.target.clone(),
                    "rows": output.rows.clone(),
                    "truncated": output.truncated,
                    "redacted": output.redacted,
                    "artifact": output.artifact.clone(),
                    "diagnostics": output.diagnostics.clone(),
                    "policy": output.policy.clone(),
                }),
            )?;

            if !output.performed && output.policy.approval_required {
                record_system_diagnostics_action_required(repo_root, project_id, run_id, output)?;
            }
        }
        AutonomousToolOutput::MacosAutomation(output) => {
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::CommandOutput,
                json!({
                    "operation": output.action.clone(),
                    "performed": output.performed,
                    "platformSupported": output.platform_supported,
                    "apps": output.apps.clone(),
                    "windows": output.windows.clone(),
                    "permissions": output.permissions.clone(),
                    "screenshot": output.screenshot.clone(),
                    "policy": output.policy.clone(),
                }),
            )?;

            if !output.performed && output.policy.approval_required {
                record_macos_action_required(repo_root, project_id, run_id, output)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn record_system_diagnostics_action_required(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    output: &AutonomousSystemDiagnosticsOutput,
) -> CommandResult<()> {
    let action_id = system_diagnostics_action_approval_id(output);
    record_action_request(
        repo_root,
        project_id,
        run_id,
        &action_id,
        "system_diagnostics_approval",
        "System diagnostics requires review",
        &output.policy.reason,
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ActionRequired,
        json!({
            "actionId": sanitize_action_id(&action_id),
            "actionType": "system_diagnostics_approval",
            "title": "System diagnostics requires review",
            "reason": output.policy.reason,
            "code": output.policy.code,
            "toolName": "system_diagnostics",
            "operation": output.action,
            "target": output.target,
        }),
    )?;
    Ok(())
}

fn record_macos_action_required(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    output: &AutonomousMacosAutomationOutput,
) -> CommandResult<()> {
    let action_id = macos_action_approval_id(output);
    record_action_request(
        repo_root,
        project_id,
        run_id,
        &action_id,
        "os_automation_approval",
        "macOS automation requires review",
        &output.policy.reason,
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ActionRequired,
        json!({
            "actionId": sanitize_action_id(&action_id),
            "actionType": "os_automation_approval",
            "title": "macOS automation requires review",
            "reason": output.policy.reason,
            "code": output.policy.code,
            "toolName": "macos_automation",
            "operation": output.action,
        }),
    )?;
    Ok(())
}

pub(crate) fn macos_action_approval_id(output: &AutonomousMacosAutomationOutput) -> String {
    format!("macos-{}", macos_action_label(output.action))
}

fn macos_action_label(action: AutonomousMacosAutomationAction) -> &'static str {
    match action {
        AutonomousMacosAutomationAction::MacPermissions => "mac_permissions",
        AutonomousMacosAutomationAction::MacAppList => "mac_app_list",
        AutonomousMacosAutomationAction::MacAppLaunch => "mac_app_launch",
        AutonomousMacosAutomationAction::MacAppActivate => "mac_app_activate",
        AutonomousMacosAutomationAction::MacAppQuit => "mac_app_quit",
        AutonomousMacosAutomationAction::MacWindowList => "mac_window_list",
        AutonomousMacosAutomationAction::MacWindowFocus => "mac_window_focus",
        AutonomousMacosAutomationAction::MacScreenshot => "mac_screenshot",
    }
}

fn record_command_action_required(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_name: &str,
    argv: &[String],
    reason: &str,
    code: &str,
) -> CommandResult<()> {
    record_action_request(
        repo_root,
        project_id,
        run_id,
        &format!("command-{}", argv.join("-")),
        "command_approval",
        "Command requires review",
        reason,
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ActionRequired,
        json!({
            "reason": reason,
            "code": code,
            "toolName": tool_name,
            "argv": argv,
        }),
    )?;
    Ok(())
}

pub(crate) fn record_action_request(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    action_id: &str,
    action_type: &str,
    title: &str,
    detail: &str,
) -> CommandResult<()> {
    project_store::append_agent_action_request(
        repo_root,
        &NewAgentActionRequestRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            action_id: sanitize_action_id(action_id),
            action_type: action_type.into(),
            title: title.into(),
            detail: detail.into(),
            created_at: now_timestamp(),
        },
    )?;
    Ok(())
}

pub(crate) fn sanitize_action_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[derive(Debug, Default)]
pub(crate) struct AgentWorkspaceGuard {
    observed_hashes: BTreeMap<String, Option<String>>,
    subagent_write_scope: Option<AutonomousSubagentWriteScope>,
}

impl AgentWorkspaceGuard {
    pub(crate) fn new(subagent_write_scope: Option<AutonomousSubagentWriteScope>) -> Self {
        Self {
            observed_hashes: BTreeMap::new(),
            subagent_write_scope,
        }
    }

    pub(crate) fn validate_write_intent(
        &self,
        repo_root: &Path,
        request: &AutonomousToolRequest,
        approved_existing_write: bool,
    ) -> CommandResult<Vec<AgentWorkspaceWriteObservation>> {
        let paths = planned_file_change_paths(request);
        let mut observations = Vec::new();
        let mut seen_paths = BTreeSet::new();
        for path in paths {
            let Some(path_key) = relative_path_key(path) else {
                return Err(CommandError::new(
                    "agent_file_path_invalid",
                    CommandErrorClass::PolicyDenied,
                    format!(
                        "Xero refused to modify `{path}` because it is not a safe repo-relative path."
                    ),
                    false,
                ));
            };
            if !seen_paths.insert(path_key.clone()) {
                continue;
            }
            self.validate_subagent_write_scope(&path_key)?;

            let current_hash = file_hash_if_present(repo_root, &path_key)?;
            if approved_existing_write {
                observations.push(AgentWorkspaceWriteObservation {
                    path: path_key,
                    old_hash: current_hash,
                });
                continue;
            }
            match (&current_hash, self.observed_hashes.get(&path_key)) {
                (None, _) => observations.push(AgentWorkspaceWriteObservation {
                    path: path_key,
                    old_hash: None,
                }),
                (Some(_), None) => {
                    return Err(CommandError::new(
                        "agent_file_write_requires_observation",
                        CommandErrorClass::PolicyDenied,
                        format!(
                            "Xero refused to modify `{path_key}` because the owned agent has not read this existing file during the run."
                        ),
                        false,
                    ));
                }
                (Some(current_hash), Some(observed_hash))
                    if observed_hash.as_ref() == Some(current_hash) =>
                {
                    observations.push(AgentWorkspaceWriteObservation {
                        path: path_key,
                        old_hash: Some(current_hash.clone()),
                    });
                }
                (Some(_), Some(observed_hash)) => {
                    return Err(CommandError::new(
                        "agent_file_changed_since_observed",
                        CommandErrorClass::PolicyDenied,
                        format!(
                            "Xero refused to modify `{path_key}` because the file changed after the owned agent last observed it (last observed hash: {}).",
                            observed_hash
                                .as_deref()
                                .unwrap_or("absent")
                        ),
                        false,
                    ));
                }
            }
        }
        Ok(observations)
    }

    fn validate_subagent_write_scope(&self, path_key: &str) -> CommandResult<()> {
        let Some(scope) = &self.subagent_write_scope else {
            return Ok(());
        };
        if scope.role != AutonomousSubagentRole::Worker {
            return Err(CommandError::new(
                "agent_subagent_readonly_write_denied",
                CommandErrorClass::PolicyDenied,
                format!(
                    "Xero refused to modify `{path_key}` because this subagent role is read-only."
                ),
                false,
            ));
        }
        if scope
            .write_set
            .iter()
            .any(|owned| path_is_inside_subagent_write_set(path_key, owned))
        {
            return Ok(());
        }
        Err(CommandError::new(
            "agent_subagent_write_set_denied",
            CommandErrorClass::PolicyDenied,
            format!(
                "Xero refused to modify `{path_key}` because it is outside this worker subagent's writeSet."
            ),
            false,
        ))
    }

    pub(crate) fn record_tool_output(
        &mut self,
        repo_root: &Path,
        output: &AutonomousToolOutput,
    ) -> CommandResult<()> {
        for path in observed_paths_from_output(output) {
            self.record_path_observation(repo_root, &path)?;
        }
        if let AutonomousToolOutput::Rename(output) = output {
            self.record_path_observation(repo_root, &output.to_path)?;
        }
        Ok(())
    }

    fn record_path_observation(&mut self, repo_root: &Path, path: &str) -> CommandResult<()> {
        let Some(path_key) = relative_path_key(path) else {
            return Ok(());
        };
        let hash = file_hash_if_present(repo_root, &path_key)?;
        self.observed_hashes.insert(path_key, hash);
        Ok(())
    }
}

fn path_is_inside_subagent_write_set(path: &str, owned: &str) -> bool {
    path == owned
        || path
            .strip_prefix(owned)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn planned_file_change_paths(request: &AutonomousToolRequest) -> Vec<&str> {
    match request {
        AutonomousToolRequest::Edit(request) => vec![request.path.as_str()],
        AutonomousToolRequest::Write(request) => vec![request.path.as_str()],
        AutonomousToolRequest::Patch(request) => request
            .operations
            .iter()
            .map(|operation| operation.path.as_str())
            .chain(request.path.as_deref())
            .collect(),
        AutonomousToolRequest::NotebookEdit(request) => vec![request.path.as_str()],
        AutonomousToolRequest::Delete(request) => vec![request.path.as_str()],
        AutonomousToolRequest::Rename(request) => vec![request.from_path.as_str()],
        _ => Vec::new(),
    }
}

fn planned_file_change_operations(request: &AutonomousToolRequest) -> Vec<(&str, &'static str)> {
    match request {
        AutonomousToolRequest::Edit(request) => vec![(request.path.as_str(), "edit")],
        AutonomousToolRequest::Write(request) => {
            vec![(request.path.as_str(), "write")]
        }
        AutonomousToolRequest::Patch(request) => request
            .operations
            .iter()
            .map(|operation| operation.path.as_str())
            .chain(request.path.as_deref())
            .map(|path| (path, "patch"))
            .collect(),
        AutonomousToolRequest::NotebookEdit(request) => {
            vec![(request.path.as_str(), "notebook_edit")]
        }
        AutonomousToolRequest::Delete(request) => vec![(request.path.as_str(), "delete")],
        AutonomousToolRequest::Rename(request) => vec![(request.from_path.as_str(), "rename")],
        _ => Vec::new(),
    }
}

fn old_hash_for_path(
    observations: &[AgentWorkspaceWriteObservation],
    path: &str,
) -> Option<String> {
    let path_key = relative_path_key(path)?;
    observations
        .iter()
        .find(|observation| observation.path == path_key)
        .and_then(|observation| observation.old_hash.clone())
}

fn observed_paths_from_output(output: &AutonomousToolOutput) -> Vec<String> {
    match output {
        AutonomousToolOutput::Read(output) => vec![output.path.clone()],
        AutonomousToolOutput::Search(output) => output
            .matches
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        AutonomousToolOutput::Find(output) => output.matches.clone(),
        AutonomousToolOutput::List(output) => output
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect(),
        AutonomousToolOutput::Edit(output) => vec![output.path.clone()],
        AutonomousToolOutput::Write(output) => vec![output.path.clone()],
        AutonomousToolOutput::Patch(output) => {
            if output.files.is_empty() {
                vec![output.path.clone()]
            } else {
                output.files.iter().map(|file| file.path.clone()).collect()
            }
        }
        AutonomousToolOutput::NotebookEdit(output) => vec![output.path.clone()],
        AutonomousToolOutput::Delete(output) => vec![output.path.clone()],
        AutonomousToolOutput::Rename(output) => vec![output.from_path.clone()],
        AutonomousToolOutput::Hash(output) => vec![output.path.clone()],
        _ => Vec::new(),
    }
}

fn relative_path_key(value: &str) -> Option<String> {
    let relative = safe_relative_path(value)?;
    Some(
        relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(segment) => segment.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn file_hash_if_present(
    repo_root: &Path,
    repo_relative_path: &str,
) -> CommandResult<Option<String>> {
    let Some(relative_path) = safe_relative_path(repo_relative_path) else {
        return Ok(None);
    };
    let path = repo_root.join(relative_path);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(sha256_hex(&bytes))),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::IsADirectory
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(CommandError::retryable(
            "agent_file_hash_read_failed",
            format!(
                "Xero could not hash owned-agent file change target {}: {error}",
                path.display()
            ),
        )),
    }
}

fn safe_relative_path(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return None;
    }

    let mut sanitized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => sanitized.push(segment),
            Component::CurDir => {}
            _ => return None,
        }
    }

    (!sanitized.as_os_str().is_empty()).then_some(sanitized)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to String should not fail");
    }
    output
}

pub(crate) fn validate_prompt(prompt: &str) -> CommandResult<()> {
    if prompt.trim().is_empty() {
        return Err(CommandError::invalid_request("prompt"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        AutonomousPatchOperation, AutonomousPatchRequest, AutonomousSearchMatch,
        AutonomousSearchOutput,
    };
    use tempfile::tempdir;

    #[test]
    fn workspace_guard_treats_search_results_as_file_observations() {
        let tempdir = tempdir().expect("tempdir");
        let root = tempdir.path();
        fs::create_dir_all(root.join("src")).expect("src");
        fs::write(root.join("src/lib.rs"), "fn before() {}\n").expect("source");

        let mut guard = AgentWorkspaceGuard::default();
        guard
            .record_tool_output(
                root,
                &AutonomousToolOutput::Search(AutonomousSearchOutput {
                    query: "before".into(),
                    scope: None,
                    matches: vec![AutonomousSearchMatch {
                        path: "src/lib.rs".into(),
                        line: 1,
                        column: 4,
                        preview: "fn before() {}".into(),
                        end_column: Some(10),
                        match_text: Some("before".into()),
                        line_hash: None,
                        context_before: Vec::new(),
                        context_after: Vec::new(),
                    }],
                    scanned_files: 1,
                    truncated: false,
                    total_matches: Some(1),
                    matched_files: Some(1),
                    engine: Some("test".into()),
                    regex: false,
                    ignore_case: false,
                    include_hidden: false,
                    include_ignored: false,
                    include_globs: Vec::new(),
                    exclude_globs: Vec::new(),
                    context_lines: 0,
                }),
            )
            .expect("record search observation");

        let observations = guard
            .validate_write_intent(
                root,
                &AutonomousToolRequest::Patch(AutonomousPatchRequest {
                    path: None,
                    search: None,
                    replace: None,
                    replace_all: false,
                    expected_hash: None,
                    preview: false,
                    operations: vec![AutonomousPatchOperation {
                        path: "src/lib.rs".into(),
                        search: "before".into(),
                        replace: "after".into(),
                        replace_all: false,
                        expected_hash: None,
                    }],
                }),
                false,
            )
            .expect("search-observed file can be patched");

        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].path, "src/lib.rs");
        assert!(observations[0].old_hash.is_some());
    }
}
