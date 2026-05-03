use rand::RngCore;
use sha2::{Digest, Sha256};

use super::*;

const CONTEXT_PACKAGE_SCHEMA: &str = "xero.provider_context_package.v1";
const MAX_RETRIEVAL_QUERY_CHARS: usize = 4_000;
const DEFAULT_RETRIEVAL_LIMIT: u32 = 6;

#[derive(Debug, Clone)]
pub(crate) struct ProviderContextPackage {
    pub system_prompt: String,
    pub manifest: project_store::AgentContextManifestRecord,
    pub compilation: PromptCompilation,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProviderContextPackageInput<'a> {
    pub repo_root: &'a Path,
    pub project_id: &'a str,
    pub agent_session_id: &'a str,
    pub run_id: &'a str,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: &'a str,
    pub agent_definition_version: u32,
    pub agent_definition_snapshot: Option<&'a JsonValue>,
    pub provider_id: &'a str,
    pub model_id: &'a str,
    pub turn_index: usize,
    pub browser_control_preference: BrowserControlPreferenceDto,
    pub soul_settings: Option<&'a SoulSettingsDto>,
    pub tools: &'a [AgentToolDescriptor],
    pub messages: &'a [ProviderMessage],
    pub owned_process_summary: Option<&'a str>,
}

pub(crate) fn assemble_provider_context_package(
    input: ProviderContextPackageInput<'_>,
    skill_contexts: Vec<XeroSkillToolContextPayload>,
) -> CommandResult<ProviderContextPackage> {
    let created_at = now_timestamp();
    let retrieved_project_context = retrieve_project_context(&input, &created_at)?;
    let compilation = PromptCompiler::new(
        input.repo_root,
        Some(input.project_id),
        Some(input.agent_session_id),
        input.runtime_agent_id,
        input.browser_control_preference,
        input.tools,
    )
    .with_soul_settings(input.soul_settings)
    .with_agent_definition_snapshot(input.agent_definition_snapshot)
    .with_owned_process_summary(input.owned_process_summary)
    .with_skill_contexts(skill_contexts)
    .with_retrieved_project_context(Some(retrieved_project_context.clone()))
    .compile()?;

    let (included_contributors, prompt_fragments_json, prompt_redacted) =
        prompt_fragment_manifest_entries(&compilation.fragments);
    let (message_contributors, messages_json, messages_redacted) =
        provider_message_manifest_entries(input.messages)?;
    let (tool_contributors, tool_descriptors_json) = tool_descriptor_manifest_entries(input.tools)?;
    let mut included = included_contributors;
    included.extend(message_contributors);
    included.extend(tool_contributors);

    let mut excluded = Vec::new();
    append_empty_context_exclusions(
        &mut excluded,
        &compilation.fragments,
        &retrieved_project_context,
    );

    let estimated_tokens = included.iter().fold(0_u64, |total, contributor| {
        total.saturating_add(contributor.estimated_tokens)
    });
    let context_limit = resolve_context_limit(input.provider_id, input.model_id);
    let budget_tokens = context_limit.effective_input_budget_tokens;
    let active_compaction = project_store::load_active_agent_compaction(
        input.repo_root,
        input.project_id,
        input.agent_session_id,
    )?;
    let settings = project_store::load_agent_context_policy_settings(
        input.repo_root,
        input.project_id,
        Some(input.agent_session_id),
    )?;
    let policy_decision =
        project_store::evaluate_agent_context_policy(project_store::AgentContextPolicyInput {
            runtime_agent_id: input.runtime_agent_id,
            estimated_tokens,
            budget_tokens,
            provider_supports_compaction: true,
            active_compaction_present: active_compaction.is_some(),
            compaction_current: active_compaction.is_some(),
            settings,
        });
    let context_hash = provider_context_hash(&compilation, input.messages, input.tools)?;
    let retrieval_query_ids = vec![retrieved_project_context.query.query_id.clone()];
    let retrieval_result_ids = retrieved_project_context
        .result_logs
        .iter()
        .map(|result| result.result_id.clone())
        .collect::<Vec<_>>();
    let retrieval_json = json!({
        "queryIds": retrieval_query_ids,
        "resultIds": retrieval_result_ids,
        "method": retrieved_project_context.method.clone(),
        "diagnostic": retrieved_project_context.diagnostic.clone(),
        "resultCount": retrieved_project_context.results.len(),
        "results": retrieved_project_context.results.iter().map(retrieval_result_manifest_json).collect::<Vec<_>>(),
    });
    let redaction_state = if prompt_redacted
        || messages_redacted
        || retrieved_project_context.results.iter().any(|result| {
            result.redaction_state != project_store::AgentContextRedactionState::Clean
        }) {
        project_store::AgentContextRedactionState::Redacted
    } else {
        project_store::AgentContextRedactionState::Clean
    };
    let manifest_json = json!({
        "kind": "provider_context_package",
        "schema": CONTEXT_PACKAGE_SCHEMA,
        "schemaVersion": 1,
        "promptVersion": SYSTEM_PROMPT_VERSION,
        "projectId": input.project_id,
        "agentSessionId": input.agent_session_id,
        "runId": input.run_id,
        "runtimeAgentId": input.runtime_agent_id.as_str(),
        "agentDefinitionId": input.agent_definition_id,
        "agentDefinitionVersion": input.agent_definition_version,
        "providerId": input.provider_id,
        "modelId": input.model_id,
        "turnIndex": input.turn_index,
        "contextHash": context_hash.clone(),
        "budgetTokens": budget_tokens,
        "contextWindowTokens": context_limit.context_window_tokens,
        "effectiveInputBudgetTokens": context_limit.effective_input_budget_tokens,
        "maxOutputTokens": context_limit.max_output_tokens,
        "outputReserveTokens": context_limit.output_reserve_tokens,
        "safetyReserveTokens": context_limit.safety_reserve_tokens,
        "limitSource": context_limit.source,
        "limitConfidence": context_limit.confidence,
        "limitDiagnostic": context_limit.diagnostic,
        "limitFetchedAt": context_limit.fetched_at,
        "estimatedTokens": estimated_tokens,
        "policy": {
            "action": context_policy_action_label(&policy_decision.action),
            "reasonCode": policy_decision.reason_code.clone(),
            "pressure": context_pressure_label(&policy_decision.pressure),
            "pressurePercent": policy_decision.pressure_percent,
            "targetRuntimeAgentId": policy_decision.target_runtime_agent_id.map(|id| id.as_str()),
        },
        "contributors": {
            "included": included.iter().map(manifest_contributor_json).collect::<Vec<_>>(),
            "excluded": excluded.iter().map(manifest_contributor_json).collect::<Vec<_>>(),
        },
        "promptFragments": prompt_fragments_json,
        "messages": messages_json,
        "toolDescriptors": tool_descriptors_json,
        "retrieval": retrieval_json,
        "compactionId": active_compaction.as_ref().map(|compaction| compaction.compaction_id.as_str()),
        "redactionState": redaction_state_label(&redaction_state),
    });

    let retrieval_query_ids = vec![retrieved_project_context.query.query_id.clone()];
    let retrieval_result_ids = retrieved_project_context
        .result_logs
        .iter()
        .map(|result| result.result_id.clone())
        .collect::<Vec<_>>();
    let manifest = project_store::insert_agent_context_manifest(
        input.repo_root,
        &project_store::NewAgentContextManifestRecord {
            manifest_id: generated_context_id("context-manifest", input.run_id, input.turn_index),
            project_id: input.project_id.to_string(),
            agent_session_id: input.agent_session_id.to_string(),
            run_id: Some(input.run_id.to_string()),
            runtime_agent_id: input.runtime_agent_id,
            agent_definition_id: input.agent_definition_id.to_string(),
            agent_definition_version: input.agent_definition_version,
            provider_id: Some(input.provider_id.to_string()),
            model_id: Some(input.model_id.to_string()),
            request_kind: project_store::AgentContextManifestRequestKind::ProviderTurn,
            policy_action: policy_decision.action,
            policy_reason_code: policy_decision.reason_code,
            budget_tokens,
            estimated_tokens,
            pressure: policy_decision.pressure,
            context_hash,
            included_contributors: included,
            excluded_contributors: excluded,
            retrieval_query_ids,
            retrieval_result_ids,
            compaction_id: active_compaction.map(|compaction| compaction.compaction_id),
            handoff_id: None,
            redaction_state,
            manifest: manifest_json,
            created_at,
        },
    )?;

    Ok(ProviderContextPackage {
        system_prompt: compilation.prompt.clone(),
        manifest,
        compilation,
    })
}

fn retrieve_project_context(
    input: &ProviderContextPackageInput<'_>,
    created_at: &str,
) -> CommandResult<project_store::AgentContextRetrievalResponse> {
    project_store::search_agent_context(
        input.repo_root,
        project_store::AgentContextRetrievalRequest {
            query_id: generated_context_id("context-retrieval", input.run_id, input.turn_index),
            project_id: input.project_id.to_string(),
            agent_session_id: Some(input.agent_session_id.to_string()),
            run_id: Some(input.run_id.to_string()),
            runtime_agent_id: input.runtime_agent_id,
            agent_definition_id: input.agent_definition_id.to_string(),
            agent_definition_version: input.agent_definition_version,
            query_text: context_retrieval_query_text(input.runtime_agent_id, input.messages),
            search_scope: project_store::AgentRetrievalSearchScope::ProjectRecords,
            filters: project_store::AgentContextRetrievalFilters::default(),
            limit_count: DEFAULT_RETRIEVAL_LIMIT,
            allow_keyword_fallback: true,
            created_at: created_at.to_string(),
        },
    )
}

fn prompt_fragment_manifest_entries(
    fragments: &[PromptFragment],
) -> (
    Vec<project_store::AgentContextManifestContributorRecord>,
    Vec<JsonValue>,
    bool,
) {
    let mut contributors = Vec::with_capacity(fragments.len());
    let mut fragments_json = Vec::with_capacity(fragments.len());
    let mut redacted = false;
    for fragment in fragments {
        contributors.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: fragment.id.clone(),
            kind: prompt_fragment_context_kind(fragment).into(),
            source_id: Some(fragment.provenance.clone()),
            estimated_tokens: fragment.token_estimate,
            reason: Some(format!("included_by_priority_{}", fragment.priority)),
        });
        let body = fragment.body.clone();
        let body_redacted = body.contains("[redacted]") || body.contains("[REDACTED]");
        redacted |= body_redacted;
        fragments_json.push(json!({
            "id": fragment.id,
            "priority": fragment.priority,
            "title": fragment.title,
            "provenance": fragment.provenance,
            "sha256": fragment.sha256,
            "tokenEstimate": fragment.token_estimate,
            "body": body,
            "bodyRedacted": body_redacted,
        }));
    }
    (contributors, fragments_json, redacted)
}

fn provider_message_manifest_entries(
    messages: &[ProviderMessage],
) -> CommandResult<(
    Vec<project_store::AgentContextManifestContributorRecord>,
    Vec<JsonValue>,
    bool,
)> {
    let mut contributors = Vec::with_capacity(messages.len());
    let mut messages_json = Vec::with_capacity(messages.len());
    let mut redacted = false;
    for (index, message) in messages.iter().enumerate() {
        let serialized = serde_json::to_string(message).map_err(|error| {
            CommandError::system_fault(
                "agent_context_package_message_serialize_failed",
                format!("Xero could not serialize provider message context: {error}"),
            )
        })?;
        let (body, redaction) = redact_session_context_text(&serialized);
        redacted |= redaction.redacted;
        let role = provider_message_role(message);
        contributors.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: format!("provider_message:{index}"),
            kind: format!("raw_tail_{role}_message"),
            source_id: Some(format!("provider_messages[{index}]")),
            estimated_tokens: estimate_tokens(&serialized),
            reason: Some("included_in_provider_turn".into()),
        });
        messages_json.push(json!({
            "index": index,
            "role": role,
            "tokenEstimate": estimate_tokens(&serialized),
            "body": body,
            "bodyRedacted": redaction.redacted,
        }));
    }
    Ok((contributors, messages_json, redacted))
}

fn tool_descriptor_manifest_entries(
    tools: &[AgentToolDescriptor],
) -> CommandResult<(
    Vec<project_store::AgentContextManifestContributorRecord>,
    Vec<JsonValue>,
)> {
    let mut contributors = Vec::with_capacity(tools.len());
    let mut tools_json = Vec::with_capacity(tools.len());
    for tool in tools {
        let serialized = serde_json::to_string(tool).map_err(|error| {
            CommandError::system_fault(
                "agent_context_package_tool_serialize_failed",
                format!("Xero could not serialize tool descriptor context: {error}"),
            )
        })?;
        contributors.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: format!("tool_descriptor:{}", tool.name),
            kind: "tool_descriptor".into(),
            source_id: Some(tool.name.clone()),
            estimated_tokens: estimate_tokens(&serialized),
            reason: Some("included_as_active_tool_descriptor".into()),
        });
        tools_json.push(json!({
            "name": tool.name,
            "tokenEstimate": estimate_tokens(&serialized),
            "description": tool.description,
            "inputSchema": tool.input_schema,
        }));
    }
    Ok((contributors, tools_json))
}

fn append_empty_context_exclusions(
    excluded: &mut Vec<project_store::AgentContextManifestContributorRecord>,
    fragments: &[PromptFragment],
    retrieved_project_context: &project_store::AgentContextRetrievalResponse,
) {
    if fragments.iter().any(|fragment| {
        fragment.id == "xero.approved_memory" && fragment.body.contains("\n(none)\n")
    }) {
        excluded.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: "xero.approved_memory:none".into(),
            kind: "approved_memory".into(),
            source_id: None,
            estimated_tokens: 0,
            reason: Some("no_approved_enabled_memory_for_project_or_session".into()),
        });
    }
    if retrieved_project_context.results.is_empty() {
        excluded.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: "xero.relevant_project_records:none".into(),
            kind: "relevant_project_record".into(),
            source_id: Some(retrieved_project_context.query.query_id.clone()),
            estimated_tokens: 0,
            reason: Some("no_relevant_project_records_found_for_turn_query".into()),
        });
    }
}

fn provider_context_hash(
    compilation: &PromptCompilation,
    messages: &[ProviderMessage],
    tools: &[AgentToolDescriptor],
) -> CommandResult<String> {
    let mut hasher = Sha256::new();
    hasher.update(CONTEXT_PACKAGE_SCHEMA.as_bytes());
    hasher.update(b"\0system_prompt\0");
    hasher.update(compilation.prompt.as_bytes());
    hasher.update(b"\0messages\0");
    for message in messages {
        let bytes = serde_json::to_vec(message).map_err(|error| {
            CommandError::system_fault(
                "agent_context_package_hash_message_failed",
                format!("Xero could not hash provider message context: {error}"),
            )
        })?;
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    hasher.update(b"\0tools\0");
    for tool in tools {
        let bytes = serde_json::to_vec(tool).map_err(|error| {
            CommandError::system_fault(
                "agent_context_package_hash_tool_failed",
                format!("Xero could not hash tool descriptor context: {error}"),
            )
        })?;
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn context_retrieval_query_text(
    runtime_agent_id: RuntimeAgentIdDto,
    messages: &[ProviderMessage],
) -> String {
    let mut selected = Vec::new();
    let mut used = 0_usize;
    for message in messages.iter().rev() {
        let (role, content) = match message {
            ProviderMessage::User { content, .. } => ("user", content.as_str()),
            ProviderMessage::Assistant { content, .. } => ("assistant", content.as_str()),
            ProviderMessage::Tool { .. } => continue,
        };
        let trimmed = content.trim();
        if trimmed.is_empty() {
            continue;
        }
        let remaining = MAX_RETRIEVAL_QUERY_CHARS.saturating_sub(used);
        if remaining == 0 {
            break;
        }
        let query_text =
            if project_store::find_prohibited_runtime_persistence_content(trimmed).is_some() {
                "[redacted]"
            } else {
                trimmed
            };
        let excerpt = truncate_chars(query_text, remaining);
        used = used.saturating_add(excerpt.chars().count());
        selected.push(format!("{role}: {excerpt}"));
    }
    selected.reverse();
    let query = selected.join("\n");
    if query.trim().is_empty() {
        format!(
            "{} provider turn durable project context",
            runtime_agent_id.label()
        )
    } else {
        query
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn generated_context_id(prefix: &str, run_id: &str, turn_index: usize) -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    let suffix = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{prefix}-{run_id}-{turn_index}-{suffix}")
}

fn prompt_fragment_context_kind(fragment: &PromptFragment) -> &'static str {
    match fragment.id.as_str() {
        "xero.soul" => "soul",
        "xero.system_policy" => "runtime_policy",
        "xero.tool_policy" => "tool_policy",
        "xero.agent_definition_policy" => "agent_definition_policy",
        "project.code_map" => "code_map",
        "xero.owned_process_state" => "process_state",
        "xero.approved_memory" => "approved_memory",
        "xero.relevant_project_records" => "relevant_project_records",
        id if id.starts_with("project.instructions.") => "repository_instructions",
        id if id.starts_with("skill.context.") => "skill_context",
        _ => "prompt_fragment",
    }
}

fn provider_message_role(message: &ProviderMessage) -> &'static str {
    match message {
        ProviderMessage::User { .. } => "user",
        ProviderMessage::Assistant { .. } => "assistant",
        ProviderMessage::Tool { .. } => "tool",
    }
}

fn manifest_contributor_json(
    contributor: &project_store::AgentContextManifestContributorRecord,
) -> JsonValue {
    json!({
        "contributorId": contributor.contributor_id,
        "kind": contributor.kind,
        "sourceId": contributor.source_id,
        "estimatedTokens": contributor.estimated_tokens,
        "reason": contributor.reason,
    })
}

fn retrieval_result_manifest_json(
    result: &project_store::AgentContextRetrievalResult,
) -> JsonValue {
    json!({
        "resultId": result.result_id,
        "sourceKind": retrieval_source_kind_label(&result.source_kind),
        "sourceId": result.source_id,
        "rank": result.rank,
        "score": result.score,
        "redactionState": redaction_state_label(&result.redaction_state),
        "metadata": result.metadata,
    })
}

fn context_policy_action_label(action: &project_store::AgentContextPolicyAction) -> &'static str {
    match action {
        project_store::AgentContextPolicyAction::ContinueNow => "continue_now",
        project_store::AgentContextPolicyAction::CompactNow => "compact_now",
        project_store::AgentContextPolicyAction::RecompactNow => "recompact_now",
        project_store::AgentContextPolicyAction::HandoffNow => "handoff_now",
        project_store::AgentContextPolicyAction::Blocked => "blocked",
    }
}

fn context_pressure_label(pressure: &project_store::AgentContextBudgetPressure) -> &'static str {
    match pressure {
        project_store::AgentContextBudgetPressure::Unknown => "unknown",
        project_store::AgentContextBudgetPressure::Low => "low",
        project_store::AgentContextBudgetPressure::Medium => "medium",
        project_store::AgentContextBudgetPressure::High => "high",
        project_store::AgentContextBudgetPressure::Over => "over",
    }
}

fn redaction_state_label(state: &project_store::AgentContextRedactionState) -> &'static str {
    match state {
        project_store::AgentContextRedactionState::Clean => "clean",
        project_store::AgentContextRedactionState::Redacted => "redacted",
        project_store::AgentContextRedactionState::Blocked => "blocked",
    }
}

fn retrieval_source_kind_label(
    kind: &project_store::AgentRetrievalResultSourceKind,
) -> &'static str {
    match kind {
        project_store::AgentRetrievalResultSourceKind::ProjectRecord => "project_record",
        project_store::AgentRetrievalResultSourceKind::ApprovedMemory => "approved_memory",
        project_store::AgentRetrievalResultSourceKind::Handoff => "handoff",
        project_store::AgentRetrievalResultSourceKind::ContextManifest => "context_manifest",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use super::*;
    use crate::{
        db::{self, project_store},
        git::repository::CanonicalRepository,
        state::DesktopState,
    };

    fn seed_project(root: &tempfile::TempDir) -> (String, PathBuf) {
        let repo_root = root.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        fs::write(
            repo_root.join("AGENTS.md"),
            "- Persist context manifests before provider turns.\n",
        )
        .expect("write instructions");
        let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");
        let project_id = "project-context-package".to_string();
        let repository = CanonicalRepository {
            project_id: project_id.clone(),
            repository_id: "repo-context-package".into(),
            root_path: canonical_root.clone(),
            root_path_string: canonical_root.to_string_lossy().into_owned(),
            common_git_dir: canonical_root.join(".git"),
            display_name: "repo".into(),
            branch_name: Some("main".into()),
            head_sha: Some("abc123".into()),
            branch: None,
            last_commit: None,
            status_entries: Vec::new(),
            has_staged_changes: false,
            has_unstaged_changes: false,
            has_untracked_changes: false,
            additions: 0,
            deletions: 0,
        };

        db::configure_project_database_paths(&root.path().join("app-data").join("xero.db"));
        let state = DesktopState::default();
        db::import_project(&repository, state.import_failpoints()).expect("import project");
        (project_id, canonical_root)
    }

    fn seed_run(repo_root: &Path, project_id: &str) {
        project_store::insert_agent_run(
            repo_root,
            &project_store::NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                project_id: project_id.into(),
                agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: "run-context-package".into(),
                provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                model_id: OPENAI_CODEX_PROVIDER_ID.into(),
                prompt: "Use phase3 context package project records.".into(),
                system_prompt: "system".into(),
                now: "2026-05-01T12:00:00Z".into(),
            },
        )
        .expect("seed agent run");
    }

    fn seed_retrievable_context(repo_root: &Path, project_id: &str) {
        project_store::insert_project_record(
            repo_root,
            &project_store::NewProjectRecordRecord {
                record_id: "project-record-phase3".into(),
                project_id: project_id.into(),
                record_kind: project_store::ProjectRecordKind::Decision,
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: "engineer".into(),
                agent_definition_version: project_store::BUILTIN_AGENT_DEFINITION_VERSION,
                agent_session_id: Some(project_store::DEFAULT_AGENT_SESSION_ID.into()),
                run_id: "run-context-package".into(),
                workflow_run_id: None,
                workflow_step_id: None,
                title: "Phase 3 context package decision".into(),
                summary: "Provider turns must cite retrieved project records.".into(),
                text: "Decision: phase3 context package assembly retrieves project records before provider turns.".into(),
                content_json: None,
                schema_name: Some("xero.test.phase3".into()),
                schema_version: 1,
                importance: project_store::ProjectRecordImportance::High,
                confidence: Some(0.95),
                tags: vec!["phase3".into(), "context-package".into()],
                source_item_ids: Vec::new(),
                related_paths: vec!["client/src-tauri/src/runtime/agent_core/context_package.rs".into()],
                produced_artifact_refs: Vec::new(),
                redaction_state: project_store::ProjectRecordRedactionState::Clean,
                visibility: project_store::ProjectRecordVisibility::Retrieval,
                created_at: "2026-05-01T12:01:00Z".into(),
            },
        )
        .expect("seed project record");
        project_store::insert_agent_memory(
            repo_root,
            &project_store::NewAgentMemoryRecord {
                memory_id: "memory-phase3".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: project_store::AgentMemoryScope::Project,
                kind: project_store::AgentMemoryKind::ProjectFact,
                text: "Phase 3 approved memory is injected for every runtime agent.".into(),
                review_state: project_store::AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(95),
                source_run_id: Some("run-context-package".into()),
                source_item_ids: Vec::new(),
                diagnostic: None,
                created_at: "2026-05-01T12:02:00Z".into(),
            },
        )
        .expect("seed approved memory");
    }

    #[test]
    fn provider_context_package_hash_is_reproducible_from_same_db_state() {
        let root = tempfile::tempdir().expect("temp dir");
        let (project_id, repo_root) = seed_project(&root);
        seed_run(&repo_root, &project_id);
        seed_retrievable_context(&repo_root, &project_id);
        let messages = vec![ProviderMessage::User {
            content: "Use phase3 context package project records.".into(),
            attachments: Vec::new(),
        }];
        let input = ProviderContextPackageInput {
            repo_root: &repo_root,
            project_id: &project_id,
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID,
            run_id: "run-context-package",
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "engineer",
            agent_definition_version: project_store::BUILTIN_AGENT_DEFINITION_VERSION,
            agent_definition_snapshot: None,
            provider_id: OPENAI_CODEX_PROVIDER_ID,
            model_id: OPENAI_CODEX_PROVIDER_ID,
            turn_index: 0,
            browser_control_preference: BrowserControlPreferenceDto::Default,
            soul_settings: None,
            tools: &[],
            messages: &messages,
            owned_process_summary: None,
        };

        let first = assemble_provider_context_package(input, Vec::new()).expect("first package");
        let second = assemble_provider_context_package(input, Vec::new()).expect("second package");

        assert_eq!(first.manifest.context_hash, second.manifest.context_hash);
        assert!(first
            .system_prompt
            .contains("Phase 3 approved memory is injected"));
        assert!(first
            .system_prompt
            .contains("phase3 context package assembly retrieves project records"));
        assert!(first.compilation.fragments.iter().any(|fragment| {
            fragment.id == "xero.relevant_project_records" && fragment.priority == 225
        }));
        assert!(!first.manifest.retrieval_result_ids.is_empty());
    }
}
