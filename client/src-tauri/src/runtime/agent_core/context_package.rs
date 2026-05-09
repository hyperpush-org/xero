use rand::RngCore;
use sha2::{Digest, Sha256};

use super::*;

const CONTEXT_PACKAGE_SCHEMA: &str = "xero.provider_context_package.v1";
const MAX_RETRIEVAL_QUERY_CHARS: usize = 4_000;
const DEFAULT_RETRIEVAL_LIMIT: u32 = 6;
const MAX_FIRST_TURN_RETRIEVAL_LIMIT: u32 = 12;

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
    pub tool_exposure_plan: Option<&'a ToolExposurePlan>,
    pub messages: &'a [ProviderMessage],
    pub owned_process_summary: Option<&'a str>,
    pub provider_preflight: Option<&'a xero_agent_core::ProviderPreflightSnapshot>,
}

pub(crate) fn assemble_provider_context_package(
    input: ProviderContextPackageInput<'_>,
    skill_contexts: Vec<XeroSkillToolContextPayload>,
) -> CommandResult<ProviderContextPackage> {
    let created_at = now_timestamp();
    let first_turn_context_policy =
        FirstTurnContextPolicy::from_agent_definition_snapshot(input.agent_definition_snapshot);
    let retrieved_project_context =
        retrieve_project_context(&input, &created_at, &first_turn_context_policy)?;
    let working_set_context = if first_turn_context_policy.auto_summary_enabled {
        source_cited_working_set_context(&retrieved_project_context)
    } else {
        None
    };
    let active_coordination_context = project_store::active_agent_coordination_context(
        input.repo_root,
        input.project_id,
        input.run_id,
        &created_at,
    )?;
    let handoff_lineage = project_store::get_agent_handoff_lineage_by_target_run(
        input.repo_root,
        input.project_id,
        input.run_id,
    )?;
    let active_coordination_summary =
        active_coordination_prompt_summary(&active_coordination_context);
    let context_limit = resolve_context_limit(input.provider_id, input.model_id);
    let budget_tokens = context_limit.effective_input_budget_tokens;
    let relevant_paths = prompt_relevant_paths_from_provider_messages(input.messages);
    let runtime_metadata = provider_context_runtime_metadata(&input, &created_at);
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
    .with_active_coordination_summary(active_coordination_summary.as_deref())
    .with_working_set_summary(
        working_set_context
            .as_ref()
            .map(|context| context.prompt_summary.as_str()),
    )
    .with_skill_contexts(skill_contexts)
    .with_relevant_paths(relevant_paths.iter().map(String::as_str))
    .with_prompt_budget_tokens(budget_tokens)
    .with_runtime_metadata(runtime_metadata)
    .compile()?;

    let (included_contributors, prompt_fragments_json, prompt_redacted) =
        prompt_fragment_manifest_entries(&compilation.fragments);
    let (excluded_prompt_contributors, prompt_fragment_exclusions_json) =
        prompt_fragment_exclusion_manifest_entries(&compilation.excluded_fragments);
    let (message_contributors, messages_json, messages_redacted) =
        provider_message_manifest_entries(input.messages)?;
    let (tool_contributors, tool_descriptors_json) = tool_descriptor_manifest_entries(input.tools)?;
    let consumed_artifact_preflight = consumed_artifact_preflight(
        input.repo_root,
        input.project_id,
        input.agent_definition_snapshot,
    )?;
    let (agent_definition_contributors, agent_definition_json, agent_definition_redacted) =
        agent_definition_manifest_entries(
            input.agent_definition_snapshot,
            consumed_artifact_preflight,
        )?;
    let (coordination_contributors, coordination_json) =
        active_coordination_manifest_entries(&active_coordination_context, input.tools);
    let mut included = included_contributors;
    included.extend(message_contributors);
    included.extend(tool_contributors);
    included.extend(agent_definition_contributors);
    included.extend(coordination_contributors);
    if let Some(lineage) = handoff_lineage.as_ref() {
        included.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: format!("handoff_lineage:{}", lineage.handoff_id),
            kind: "handoff_lineage".into(),
            source_id: Some(lineage.handoff_id.clone()),
            estimated_tokens: estimate_tokens(&lineage.source_context_hash),
            reason: Some("included_as_first_turn_handoff_context".into()),
        });
    }
    if let Some(context) = working_set_context.as_ref() {
        included.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: format!(
                "working_set_summary:{}",
                retrieved_project_context.query.query_id
            ),
            kind: "working_set_summary".into(),
            source_id: Some(retrieved_project_context.query.query_id.clone()),
            estimated_tokens: estimate_tokens(&context.prompt_summary),
            reason: Some("admitted_source_cited_working_set_summary".into()),
        });
    }

    let mut excluded = excluded_prompt_contributors;
    append_empty_context_exclusions(
        &mut excluded,
        &compilation.fragments,
        &retrieved_project_context,
    );

    let estimated_tokens = included.iter().fold(0_u64, |total, contributor| {
        total.saturating_add(contributor.estimated_tokens)
    });
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
    let freshness_diagnostics = retrieved_project_context
        .diagnostic
        .as_ref()
        .and_then(|diagnostic| diagnostic.get("freshnessDiagnostics"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let stale_context_rows_available = freshness_count(&freshness_diagnostics, "staleCount");
    let source_missing_context_rows_available =
        freshness_count(&freshness_diagnostics, "sourceMissingCount");
    let superseded_context_rows_available =
        freshness_count(&freshness_diagnostics, "supersededCount");
    let retrieval_json = json!({
        "queryIds": retrieval_query_ids,
        "resultIds": retrieval_result_ids,
        "deliveryModel": "tool_mediated",
        "rawContextInjected": false,
        "firstTurnPolicy": first_turn_context_policy.manifest_json(),
        "method": retrieved_project_context.method.clone(),
        "diagnostic": retrieved_project_context.diagnostic.clone(),
        "freshnessDiagnostics": freshness_diagnostics,
        "staleContextRowsAvailable": stale_context_rows_available,
        "sourceMissingContextRowsAvailable": source_missing_context_rows_available,
        "supersededContextRowsAvailable": superseded_context_rows_available,
        "toolAvailability": {
            "project_context": input.tools.iter().any(|tool| matches!(
                tool.name.as_str(),
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH | AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET
            )),
        },
        "resultCount": retrieved_project_context.results.len(),
        "results": retrieved_project_context.results.iter().map(retrieval_result_manifest_json).collect::<Vec<_>>(),
    });
    let working_set_json = working_set_context
        .as_ref()
        .map(|context| {
            json!({
                "schema": "xero.source_cited_working_set.v1",
                "deliveryModel": "admitted_source_cited_summary",
                "rawDurableContextInjected": false,
                "promptFragmentId": "xero.working_set_context",
                "sourceQueryId": retrieved_project_context.query.query_id.clone(),
                "policy": first_turn_context_policy.manifest_json(),
                "citationCount": context.citations.len(),
                "citations": context.citations,
            })
        })
        .unwrap_or_else(|| {
            json!({
                "schema": "xero.source_cited_working_set.v1",
                "deliveryModel": "none",
                "rawDurableContextInjected": false,
                "promptFragmentId": null,
                "sourceQueryId": retrieved_project_context.query.query_id.clone(),
                "policy": first_turn_context_policy.manifest_json(),
                "citationCount": 0,
                "citations": [],
            })
        });
    let redaction_state = if prompt_redacted
        || messages_redacted
        || agent_definition_redacted
        || retrieved_project_context.results.iter().any(|result| {
            result.redaction_state != project_store::AgentContextRedactionState::Clean
        }) {
        project_store::AgentContextRedactionState::Redacted
    } else {
        project_store::AgentContextRedactionState::Clean
    };
    let mut required_provider_features =
        xero_agent_core::ProviderPreflightRequiredFeatures::owned_agent_text_turn();
    required_provider_features.attachments = input.messages.iter().any(|message| match message {
        ProviderMessage::User { attachments, .. } => !attachments.is_empty(),
        ProviderMessage::Assistant { .. } | ProviderMessage::Tool { .. } => false,
    });
    let provider_preflight = input.provider_preflight.cloned().unwrap_or_else(|| {
        crate::provider_preflight::static_provider_preflight_snapshot(
            input.provider_id,
            input.model_id,
            required_provider_features,
        )
    });
    let admitted_provider_preflight_hash = stable_provider_preflight_hash(&provider_preflight);
    let prompt_diff = prompt_diff_since_previous_manifest(
        input.repo_root,
        input.project_id,
        input.run_id,
        input.turn_index,
        &compilation.fragments,
        input.tools,
        active_compaction
            .as_ref()
            .map(|compaction| compaction.compaction_id.as_str()),
    )?;
    let policy_json = json!({
        "action": context_policy_action_label(&policy_decision.action),
        "reasonCode": policy_decision.reason_code.clone(),
        "pressure": context_pressure_label(&policy_decision.pressure),
        "pressurePercent": policy_decision.pressure_percent,
        "targetRuntimeAgentId": policy_decision.target_runtime_agent_id.map(|id| id.as_str()),
    });
    let contributors_json = json!({
        "included": included.iter().map(manifest_contributor_json).collect::<Vec<_>>(),
        "excluded": excluded.iter().map(manifest_contributor_json).collect::<Vec<_>>(),
    });
    let prompt_assembly_json = json!({
        "strategy": "priority_budget_pipeline_v1",
        "sort": "priority_desc_id_asc_provenance_asc",
        "promptBudgetTokens": compilation.prompt_budget_tokens,
        "estimatedPromptTokens": compilation.estimated_prompt_tokens,
        "relevantPaths": relevant_paths.iter().collect::<Vec<_>>(),
        "includedFragmentCount": compilation.fragments.len(),
        "excludedFragmentCount": compilation.excluded_fragments.len(),
    });
    let mut manifest_fields = serde_json::Map::new();
    manifest_fields.insert("kind".into(), json!("provider_context_package"));
    manifest_fields.insert("schema".into(), json!(CONTEXT_PACKAGE_SCHEMA));
    manifest_fields.insert("schemaVersion".into(), json!(1));
    manifest_fields.insert("promptVersion".into(), json!(SYSTEM_PROMPT_VERSION));
    manifest_fields.insert("projectId".into(), json!(input.project_id));
    manifest_fields.insert("agentSessionId".into(), json!(input.agent_session_id));
    manifest_fields.insert("runId".into(), json!(input.run_id));
    manifest_fields.insert(
        "runtimeAgentId".into(),
        json!(input.runtime_agent_id.as_str()),
    );
    manifest_fields.insert("agentDefinitionId".into(), json!(input.agent_definition_id));
    manifest_fields.insert(
        "agentDefinitionVersion".into(),
        json!(input.agent_definition_version),
    );
    manifest_fields.insert("providerId".into(), json!(input.provider_id));
    manifest_fields.insert("modelId".into(), json!(input.model_id));
    manifest_fields.insert("turnIndex".into(), json!(input.turn_index));
    manifest_fields.insert("contextHash".into(), json!(context_hash.clone()));
    manifest_fields.insert("budgetTokens".into(), json!(budget_tokens));
    manifest_fields.insert(
        "contextWindowTokens".into(),
        json!(context_limit.context_window_tokens),
    );
    manifest_fields.insert(
        "effectiveInputBudgetTokens".into(),
        json!(context_limit.effective_input_budget_tokens),
    );
    manifest_fields.insert(
        "maxOutputTokens".into(),
        json!(context_limit.max_output_tokens),
    );
    manifest_fields.insert(
        "outputReserveTokens".into(),
        json!(context_limit.output_reserve_tokens),
    );
    manifest_fields.insert(
        "safetyReserveTokens".into(),
        json!(context_limit.safety_reserve_tokens),
    );
    manifest_fields.insert("limitSource".into(), json!(context_limit.source));
    manifest_fields.insert("limitConfidence".into(), json!(context_limit.confidence));
    manifest_fields.insert("limitDiagnostic".into(), json!(context_limit.diagnostic));
    manifest_fields.insert("limitFetchedAt".into(), json!(context_limit.fetched_at));
    manifest_fields.insert("providerPreflight".into(), json!(provider_preflight));
    manifest_fields.insert(
        "admittedProviderPreflightHash".into(),
        json!(admitted_provider_preflight_hash),
    );
    manifest_fields.insert("estimatedTokens".into(), json!(estimated_tokens));
    manifest_fields.insert("policy".into(), policy_json);
    manifest_fields.insert("contributors".into(), contributors_json);
    manifest_fields.insert(
        "promptFragments".into(),
        JsonValue::Array(prompt_fragments_json),
    );
    manifest_fields.insert(
        "promptFragmentExclusions".into(),
        JsonValue::Array(prompt_fragment_exclusions_json),
    );
    manifest_fields.insert("promptAssembly".into(), prompt_assembly_json);
    manifest_fields.insert("messages".into(), JsonValue::Array(messages_json));
    manifest_fields.insert(
        "toolDescriptors".into(),
        JsonValue::Array(tool_descriptors_json),
    );
    manifest_fields.insert("toolExposurePlan".into(), json!(input.tool_exposure_plan));
    manifest_fields.insert("agentDefinition".into(), agent_definition_json);
    manifest_fields.insert("promptDiff".into(), prompt_diff);
    manifest_fields.insert("retrieval".into(), retrieval_json);
    manifest_fields.insert("workingSet".into(), working_set_json);
    manifest_fields.insert("coordination".into(), coordination_json);
    manifest_fields.insert(
        "handoff".into(),
        handoff_lineage
            .as_ref()
            .map(handoff_lineage_manifest_json)
            .unwrap_or(JsonValue::Null),
    );
    manifest_fields.insert(
        "compactionId".into(),
        json!(active_compaction
            .as_ref()
            .map(|compaction| compaction.compaction_id.as_str())),
    );
    manifest_fields.insert(
        "redactionState".into(),
        json!(redaction_state_label(&redaction_state)),
    );
    let manifest_json = JsonValue::Object(manifest_fields);

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
            handoff_id: handoff_lineage.map(|lineage| lineage.handoff_id),
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
    policy: &FirstTurnContextPolicy,
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
            query_text: context_retrieval_query_text(
                input.runtime_agent_id,
                input.messages,
                input.agent_definition_snapshot,
            ),
            search_scope: policy.search_scope.clone(),
            filters: policy.filters.clone(),
            limit_count: policy.limit_count,
            allow_keyword_fallback: true,
            created_at: created_at.to_string(),
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FirstTurnContextPolicy {
    auto_summary_enabled: bool,
    source: &'static str,
    search_scope: project_store::AgentRetrievalSearchScope,
    filters: project_store::AgentContextRetrievalFilters,
    limit_count: u32,
    requested_record_kinds: Vec<String>,
    requested_memory_kinds: Vec<String>,
    ignored_record_kinds: Vec<String>,
    ignored_memory_kinds: Vec<String>,
}

impl FirstTurnContextPolicy {
    fn from_agent_definition_snapshot(snapshot: Option<&JsonValue>) -> Self {
        let Some(defaults) = snapshot
            .and_then(|snapshot| snapshot.get("retrievalDefaults"))
            .and_then(JsonValue::as_object)
        else {
            return Self::default_project_records();
        };

        let auto_summary_enabled = defaults
            .get("enabled")
            .and_then(JsonValue::as_bool)
            .unwrap_or(true);
        let (record_kinds, requested_record_kinds, ignored_record_kinds) = defaults
            .get("recordKinds")
            .map(parse_project_record_kind_policy_array)
            .unwrap_or_default();
        let (memory_kinds, requested_memory_kinds, ignored_memory_kinds) = defaults
            .get("memoryKinds")
            .map(parse_agent_memory_kind_policy_array)
            .unwrap_or_default();
        let limit_count = defaults
            .get("limit")
            .and_then(JsonValue::as_u64)
            .map(|limit| {
                u32::try_from(limit)
                    .unwrap_or(MAX_FIRST_TURN_RETRIEVAL_LIMIT)
                    .max(1)
                    .min(MAX_FIRST_TURN_RETRIEVAL_LIMIT)
            })
            .unwrap_or(DEFAULT_RETRIEVAL_LIMIT);
        let search_scope =
            first_turn_search_scope(record_kinds.as_slice(), memory_kinds.as_slice());

        Self {
            auto_summary_enabled,
            source: "agent_definition.retrievalDefaults",
            search_scope,
            filters: project_store::AgentContextRetrievalFilters {
                record_kinds,
                memory_kinds,
                ..project_store::AgentContextRetrievalFilters::default()
            },
            limit_count,
            requested_record_kinds,
            requested_memory_kinds,
            ignored_record_kinds,
            ignored_memory_kinds,
        }
    }

    fn default_project_records() -> Self {
        Self {
            auto_summary_enabled: true,
            source: "runtime_default",
            search_scope: project_store::AgentRetrievalSearchScope::ProjectRecords,
            filters: project_store::AgentContextRetrievalFilters::default(),
            limit_count: DEFAULT_RETRIEVAL_LIMIT,
            requested_record_kinds: Vec::new(),
            requested_memory_kinds: Vec::new(),
            ignored_record_kinds: Vec::new(),
            ignored_memory_kinds: Vec::new(),
        }
    }

    fn manifest_json(&self) -> JsonValue {
        json!({
            "schema": "xero.first_turn_context_policy.v1",
            "source": self.source,
            "autoSummaryEnabled": self.auto_summary_enabled,
            "bulkDurableContextDelivery": "tool_mediated",
            "searchScope": retrieval_search_scope_label(&self.search_scope),
            "limitCount": self.limit_count,
            "recordKinds": self.filters.record_kinds.iter().map(project_record_kind_policy_label).collect::<Vec<_>>(),
            "memoryKinds": self.filters.memory_kinds.iter().map(agent_memory_kind_policy_label).collect::<Vec<_>>(),
            "requestedRecordKinds": self.requested_record_kinds,
            "requestedMemoryKinds": self.requested_memory_kinds,
            "ignoredRecordKinds": self.ignored_record_kinds,
            "ignoredMemoryKinds": self.ignored_memory_kinds,
        })
    }
}

fn first_turn_search_scope(
    record_kinds: &[project_store::ProjectRecordKind],
    memory_kinds: &[project_store::AgentMemoryKind],
) -> project_store::AgentRetrievalSearchScope {
    match (record_kinds.is_empty(), memory_kinds.is_empty()) {
        (false, false) => project_store::AgentRetrievalSearchScope::HybridContext,
        (true, false) => project_store::AgentRetrievalSearchScope::ApprovedMemory,
        _ => project_store::AgentRetrievalSearchScope::ProjectRecords,
    }
}

fn parse_project_record_kind_policy_array(
    value: &JsonValue,
) -> (
    Vec<project_store::ProjectRecordKind>,
    Vec<String>,
    Vec<String>,
) {
    let mut kinds = Vec::new();
    let mut requested = Vec::new();
    let mut ignored = Vec::new();
    let Some(values) = value.as_array() else {
        return (kinds, requested, ignored);
    };
    for item in values {
        let Some(raw) = item
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        requested.push(raw.to_string());
        match parse_project_record_kind_policy(raw) {
            Some(kind) if !kinds.contains(&kind) => kinds.push(kind),
            Some(_) => {}
            None => ignored.push(raw.to_string()),
        }
    }
    (kinds, requested, ignored)
}

fn parse_agent_memory_kind_policy_array(
    value: &JsonValue,
) -> (
    Vec<project_store::AgentMemoryKind>,
    Vec<String>,
    Vec<String>,
) {
    let mut kinds = Vec::new();
    let mut requested = Vec::new();
    let mut ignored = Vec::new();
    let Some(values) = value.as_array() else {
        return (kinds, requested, ignored);
    };
    for item in values {
        let Some(raw) = item
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        requested.push(raw.to_string());
        match parse_agent_memory_kind_policy(raw) {
            Some(kind) if !kinds.contains(&kind) => kinds.push(kind),
            Some(_) => {}
            None => ignored.push(raw.to_string()),
        }
    }
    (kinds, requested, ignored)
}

fn parse_project_record_kind_policy(value: &str) -> Option<project_store::ProjectRecordKind> {
    Some(match value {
        "agent_handoff" => project_store::ProjectRecordKind::AgentHandoff,
        "project_fact" => project_store::ProjectRecordKind::ProjectFact,
        "decision" => project_store::ProjectRecordKind::Decision,
        "constraint" => project_store::ProjectRecordKind::Constraint,
        "plan" => project_store::ProjectRecordKind::Plan,
        "finding" => project_store::ProjectRecordKind::Finding,
        "verification" => project_store::ProjectRecordKind::Verification,
        "question" => project_store::ProjectRecordKind::Question,
        "artifact" => project_store::ProjectRecordKind::Artifact,
        "context_note" => project_store::ProjectRecordKind::ContextNote,
        "diagnostic" => project_store::ProjectRecordKind::Diagnostic,
        _ => return None,
    })
}

fn parse_agent_memory_kind_policy(value: &str) -> Option<project_store::AgentMemoryKind> {
    Some(match value {
        "project_fact" => project_store::AgentMemoryKind::ProjectFact,
        "user_preference" => project_store::AgentMemoryKind::UserPreference,
        "decision" => project_store::AgentMemoryKind::Decision,
        "session_summary" => project_store::AgentMemoryKind::SessionSummary,
        "troubleshooting" => project_store::AgentMemoryKind::Troubleshooting,
        _ => return None,
    })
}

fn retrieval_search_scope_label(scope: &project_store::AgentRetrievalSearchScope) -> &'static str {
    match scope {
        project_store::AgentRetrievalSearchScope::ProjectRecords => "project_records",
        project_store::AgentRetrievalSearchScope::ApprovedMemory => "approved_memory",
        project_store::AgentRetrievalSearchScope::HybridContext => "hybrid_context",
        project_store::AgentRetrievalSearchScope::Handoffs => "handoffs",
    }
}

fn project_record_kind_policy_label(kind: &project_store::ProjectRecordKind) -> &'static str {
    match kind {
        project_store::ProjectRecordKind::AgentHandoff => "agent_handoff",
        project_store::ProjectRecordKind::ProjectFact => "project_fact",
        project_store::ProjectRecordKind::Decision => "decision",
        project_store::ProjectRecordKind::Constraint => "constraint",
        project_store::ProjectRecordKind::Plan => "plan",
        project_store::ProjectRecordKind::Finding => "finding",
        project_store::ProjectRecordKind::Verification => "verification",
        project_store::ProjectRecordKind::Question => "question",
        project_store::ProjectRecordKind::Artifact => "artifact",
        project_store::ProjectRecordKind::ContextNote => "context_note",
        project_store::ProjectRecordKind::Diagnostic => "diagnostic",
    }
}

fn agent_memory_kind_policy_label(kind: &project_store::AgentMemoryKind) -> &'static str {
    match kind {
        project_store::AgentMemoryKind::ProjectFact => "project_fact",
        project_store::AgentMemoryKind::UserPreference => "user_preference",
        project_store::AgentMemoryKind::Decision => "decision",
        project_store::AgentMemoryKind::SessionSummary => "session_summary",
        project_store::AgentMemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn provider_context_runtime_metadata(
    input: &ProviderContextPackageInput<'_>,
    fallback_timestamp: &str,
) -> RuntimeHostMetadata {
    let timestamp = project_store::load_agent_run(input.repo_root, input.project_id, input.run_id)
        .map(|snapshot| snapshot.run.started_at)
        .unwrap_or_else(|_| fallback_timestamp.to_string());
    let mut metadata = runtime_host_metadata();
    metadata.date_utc = timestamp
        .split_once('T')
        .map(|(date, _)| date)
        .unwrap_or(timestamp.as_str())
        .to_owned();
    metadata.timestamp_utc = timestamp;
    metadata
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
            reason: Some(fragment.inclusion_reason.clone()),
        });
        let body = fragment.body.clone();
        let body_redacted = body.contains("[redacted]") || body.contains("[REDACTED]");
        redacted |= body_redacted;
        fragments_json.push(json!({
            "id": fragment.id,
            "priority": fragment.priority,
            "title": fragment.title,
            "provenance": fragment.provenance,
            "budgetPolicy": fragment.budget_policy.as_str(),
            "inclusionReason": fragment.inclusion_reason,
            "sha256": fragment.sha256,
            "tokenEstimate": fragment.token_estimate,
            "body": body,
            "bodyRedacted": body_redacted,
        }));
    }
    (contributors, fragments_json, redacted)
}

fn prompt_fragment_exclusion_manifest_entries(
    exclusions: &[PromptFragmentExclusion],
) -> (
    Vec<project_store::AgentContextManifestContributorRecord>,
    Vec<JsonValue>,
) {
    let mut contributors = Vec::with_capacity(exclusions.len());
    let mut exclusions_json = Vec::with_capacity(exclusions.len());
    for exclusion in exclusions {
        contributors.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: exclusion.id.clone(),
            kind: prompt_exclusion_context_kind(exclusion).into(),
            source_id: Some(exclusion.provenance.clone()),
            estimated_tokens: exclusion.token_estimate,
            reason: Some(exclusion.reason.clone()),
        });
        exclusions_json.push(json!({
            "id": exclusion.id,
            "priority": exclusion.priority,
            "title": exclusion.title,
            "provenance": exclusion.provenance,
            "budgetPolicy": exclusion.budget_policy.as_str(),
            "sha256": exclusion.sha256,
            "tokenEstimate": exclusion.token_estimate,
            "reason": exclusion.reason,
        }));
    }
    (contributors, exclusions_json)
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

fn agent_definition_manifest_entries(
    snapshot: Option<&JsonValue>,
    consumed_artifact_preflight: JsonValue,
) -> CommandResult<(
    Vec<project_store::AgentContextManifestContributorRecord>,
    JsonValue,
    bool,
)> {
    let Some(snapshot) = snapshot else {
        return Ok((Vec::new(), JsonValue::Null, false));
    };
    let definition_id = snapshot
        .get("id")
        .and_then(JsonValue::as_str)
        .unwrap_or("custom_agent");
    let definition_version = snapshot
        .get("version")
        .and_then(JsonValue::as_u64)
        .unwrap_or(1);
    let scope = snapshot
        .get("scope")
        .and_then(JsonValue::as_str)
        .unwrap_or("custom");
    let db_touchpoints = compact_agent_definition_db_touchpoints(snapshot.get("dbTouchpoints"));
    let touchpoint_count = agent_definition_db_touchpoint_count(&db_touchpoints);
    let consumes = compact_agent_definition_consumed_artifacts(snapshot.get("consumes"));
    let consumed_artifact_count = consumes.as_array().map(Vec::len).unwrap_or_default();
    let required_consumed_artifact_count = consumes
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter(|artifact| {
            artifact
                .get("required")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false)
        })
        .count();
    let manifest = json!({
        "id": definition_id,
        "version": definition_version,
        "schema": snapshot.get("schema").cloned().unwrap_or(JsonValue::Null),
        "schemaVersion": snapshot.get("schemaVersion").cloned().unwrap_or(JsonValue::Null),
        "scope": scope,
        "dbTouchpoints": db_touchpoints,
        "dbTouchpointCount": touchpoint_count,
        "consumes": consumes,
        "consumedArtifactCount": consumed_artifact_count,
        "requiredConsumedArtifactCount": required_consumed_artifact_count,
        "consumedArtifactPreflight": consumed_artifact_preflight,
    });
    let (manifest, manifest_redacted) =
        crate::runtime::redaction::redact_json_for_persistence(&manifest);
    let mut contributors = Vec::new();
    if scope != "built_in" && touchpoint_count > 0 {
        let serialized = serde_json::to_string(&manifest).map_err(|error| {
            CommandError::system_fault(
                "agent_context_package_definition_manifest_serialize_failed",
                format!("Xero could not serialize custom definition manifest context: {error}"),
            )
        })?;
        contributors.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: "agent_definition:db_touchpoints".into(),
            kind: "agent_definition_db_touchpoints".into(),
            source_id: Some(format!("{definition_id}@{definition_version}")),
            estimated_tokens: estimate_tokens(&serialized),
            reason: Some("saved_custom_agent_db_touchpoints_runtime_guidance".into()),
        });
    }
    if scope != "built_in" && consumed_artifact_count > 0 {
        let serialized = serde_json::to_string(&manifest).map_err(|error| {
            CommandError::system_fault(
                "agent_context_package_definition_manifest_serialize_failed",
                format!("Xero could not serialize custom definition manifest context: {error}"),
            )
        })?;
        contributors.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: "agent_definition:consumed_artifacts".into(),
            kind: "agent_definition_consumed_artifacts".into(),
            source_id: Some(format!("{definition_id}@{definition_version}")),
            estimated_tokens: estimate_tokens(&serialized),
            reason: Some("saved_custom_agent_consumed_artifacts_runtime_expectation".into()),
        });
    }
    Ok((contributors, manifest, manifest_redacted))
}

fn compact_agent_definition_db_touchpoints(value: Option<&JsonValue>) -> JsonValue {
    let mut output = serde_json::Map::new();
    let object = value.and_then(JsonValue::as_object);
    for kind in ["reads", "writes", "encouraged"] {
        let items = object
            .and_then(|object| object.get(kind))
            .and_then(JsonValue::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(compact_agent_definition_db_touchpoint)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        output.insert(kind.into(), JsonValue::Array(items));
    }
    JsonValue::Object(output)
}

fn compact_agent_definition_db_touchpoint(value: &JsonValue) -> JsonValue {
    json!({
        "table": value.get("table").cloned().unwrap_or(JsonValue::Null),
        "kind": value.get("kind").cloned().unwrap_or(JsonValue::Null),
        "purpose": value.get("purpose").cloned().unwrap_or(JsonValue::Null),
        "columns": value.get("columns").cloned().unwrap_or(JsonValue::Array(Vec::new())),
        "triggers": value.get("triggers").cloned().unwrap_or(JsonValue::Array(Vec::new())),
    })
}

fn agent_definition_db_touchpoint_count(value: &JsonValue) -> usize {
    ["reads", "writes", "encouraged"]
        .into_iter()
        .filter_map(|kind| value.get(kind).and_then(JsonValue::as_array))
        .map(Vec::len)
        .sum()
}

fn compact_agent_definition_consumed_artifacts(value: Option<&JsonValue>) -> JsonValue {
    value
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .map(compact_agent_definition_consumed_artifact)
                .collect::<Vec<_>>()
        })
        .map(JsonValue::Array)
        .unwrap_or_else(|| JsonValue::Array(Vec::new()))
}

fn compact_agent_definition_consumed_artifact(value: &JsonValue) -> JsonValue {
    json!({
        "id": value.get("id").cloned().unwrap_or(JsonValue::Null),
        "label": value.get("label").cloned().unwrap_or(JsonValue::Null),
        "description": value.get("description").cloned().unwrap_or(JsonValue::Null),
        "sourceAgent": value.get("sourceAgent").cloned().unwrap_or(JsonValue::Null),
        "contract": value.get("contract").cloned().unwrap_or(JsonValue::Null),
        "sections": value.get("sections").cloned().unwrap_or(JsonValue::Array(Vec::new())),
        "required": value.get("required").cloned().unwrap_or(JsonValue::Bool(false)),
    })
}

fn consumed_artifact_preflight(
    repo_root: &Path,
    project_id: &str,
    snapshot: Option<&JsonValue>,
) -> CommandResult<JsonValue> {
    let Some(snapshot) = snapshot else {
        return Ok(json!({
            "status": "not_applicable",
            "reason": "no_agent_definition_snapshot",
        }));
    };
    if snapshot
        .get("scope")
        .and_then(JsonValue::as_str)
        .is_some_and(|scope| scope == "built_in")
    {
        return Ok(json!({
            "status": "not_applicable",
            "reason": "built_in_definition",
        }));
    }
    let consumes = compact_agent_definition_consumed_artifacts(snapshot.get("consumes"));
    let required = consumes
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter(|artifact| {
            artifact
                .get("required")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    if required.is_empty() {
        return Ok(json!({
            "status": "no_required_artifacts",
            "requiredCount": 0,
            "matched": [],
            "missingRequired": [],
        }));
    }

    let records = project_store::list_project_records(repo_root, project_id)?;
    let mut matched = Vec::new();
    let mut missing = Vec::new();
    for artifact in required {
        if let Some(record) = records
            .iter()
            .find(|record| project_record_satisfies_consumed_artifact(record, &artifact))
        {
            matched.push(json!({
                "id": artifact.get("id").cloned().unwrap_or(JsonValue::Null),
                "label": artifact.get("label").cloned().unwrap_or(JsonValue::Null),
                "contract": artifact.get("contract").cloned().unwrap_or(JsonValue::Null),
                "recordId": record.record_id.clone(),
                "recordKind": project_store::project_record_kind_sql_value(&record.record_kind),
                "schemaName": record.schema_name.clone(),
            }));
        } else {
            missing.push(artifact);
        }
    }

    Ok(json!({
        "status": if missing.is_empty() { "ready" } else { "missing_required" },
        "requiredCount": matched.len() + missing.len(),
        "matched": matched,
        "missingRequired": missing,
    }))
}

fn project_record_satisfies_consumed_artifact(
    record: &project_store::ProjectRecordRecord,
    artifact: &JsonValue,
) -> bool {
    if record.redaction_state == project_store::ProjectRecordRedactionState::Blocked {
        return false;
    }
    let id = agent_definition_manifest_string_field(artifact, "id").unwrap_or("");
    let contract = agent_definition_manifest_string_field(artifact, "contract").unwrap_or("");
    if !id.is_empty()
        && record
            .produced_artifact_refs
            .iter()
            .any(|reference| reference == id || reference == &format!("artifact:{id}"))
    {
        return true;
    }
    if !id.is_empty()
        && record
            .tags
            .iter()
            .any(|tag| tag == id || tag == &format!("artifact:{id}"))
    {
        return true;
    }
    if !contract.is_empty()
        && record
            .tags
            .iter()
            .any(|tag| tag == contract || tag == &format!("contract:{contract}"))
    {
        return true;
    }

    let Some(schema_name) = consumed_artifact_contract_schema_name(contract) else {
        return false;
    };
    if record.schema_name.as_deref() == Some(schema_name) {
        return true;
    }
    if record
        .content_json
        .as_ref()
        .and_then(|content| content.get("schema"))
        .and_then(JsonValue::as_str)
        == Some(schema_name)
    {
        return true;
    }
    contract == "plan_pack" && record.record_kind == project_store::ProjectRecordKind::Plan
}

fn consumed_artifact_contract_schema_name(contract: &str) -> Option<&'static str> {
    match contract {
        "plan_pack" => Some("xero.plan_pack.v1"),
        "crawl_report" => Some("xero.project_crawl.report.v1"),
        "harness_test_report" => Some("xero.harness_test_report.v1"),
        _ => None,
    }
}

fn agent_definition_manifest_string_field<'a>(
    value: &'a JsonValue,
    field: &str,
) -> Option<&'a str> {
    value
        .get(field)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn active_coordination_manifest_entries(
    context: &project_store::AgentCoordinationContext,
    tools: &[AgentToolDescriptor],
) -> (
    Vec<project_store::AgentContextManifestContributorRecord>,
    JsonValue,
) {
    let mut contributors = Vec::new();
    for presence in &context.presence {
        let summary = format!(
            "{} {} {} {}",
            presence.run_id, presence.status, presence.current_phase, presence.activity_summary
        );
        contributors.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: format!("agent_coordination_presence:{}", presence.run_id),
            kind: "active_agent_presence".into(),
            source_id: Some(presence.run_id.clone()),
            estimated_tokens: estimate_tokens(&summary),
            reason: Some("active_same_project_session_recent".into()),
        });
    }
    for reservation in &context.reservations {
        let summary = format!(
            "{} {} {}",
            reservation.path,
            reservation.operation.as_str(),
            reservation
                .owner_child_run_id
                .as_deref()
                .unwrap_or(&reservation.owner_run_id)
        );
        contributors.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: format!("agent_file_reservation:{}", reservation.reservation_id),
            kind: "active_file_reservation".into(),
            source_id: Some(reservation.reservation_id.clone()),
            estimated_tokens: estimate_tokens(&summary),
            reason: Some("active_same_project_file_reservation".into()),
        });
    }
    for event in &context.events {
        let is_history_event = is_history_coordination_event(&event.event_kind);
        let summary = if is_history_event {
            format!(
                "{} {} {} {}",
                event.run_id,
                event.event_kind,
                event.summary,
                coordination_path_preview(&history_event_affected_paths(event))
            )
        } else {
            format!("{} {} {}", event.run_id, event.event_kind, event.summary)
        };
        contributors.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: format!("agent_coordination_event:{}", event.id),
            kind: if is_history_event {
                "active_code_history_coordination_event"
            } else {
                "active_agent_coordination_event"
            }
            .into(),
            source_id: Some(event.id.to_string()),
            estimated_tokens: estimate_tokens(&summary),
            reason: Some(
                if is_history_event {
                    "recent_code_history_coordination_event"
                } else {
                    "recent_active_coordination_event"
                }
                .into(),
            ),
        });
    }
    for delivery in &context.mailbox {
        let item = &delivery.item;
        let is_history_mailbox = is_history_mailbox_item_type(item.item_type);
        let summary = format!(
            "{} {} {} {}",
            item.item_id,
            item.item_type.as_str(),
            item.priority.as_str(),
            item.title
        );
        contributors.push(project_store::AgentContextManifestContributorRecord {
            contributor_id: format!("agent_mailbox_item:{}", item.item_id),
            kind: if is_history_mailbox {
                "active_code_history_mailbox_notice"
            } else {
                "active_agent_mailbox_item"
            }
            .into(),
            source_id: Some(item.item_id.clone()),
            estimated_tokens: estimate_tokens(&summary),
            reason: Some(
                if is_history_mailbox {
                    "temporary_code_history_mailbox_notice"
                } else {
                    "temporary_swarm_mailbox_delivery"
                }
                .into(),
            ),
        });
    }
    let history_notices = coordination_history_notice_manifest_entries(context);
    let stale_paths = coordination_stale_paths(context);
    let history_notice_types = coordination_history_notice_types(context);

    let manifest = json!({
        "deliveryModel": "prompt_fragment_and_tool",
        "rawDurableMemoryInjected": false,
        "promptFragmentId": if context.presence.is_empty() && context.reservations.is_empty() && context.events.is_empty() && context.mailbox.is_empty() {
            JsonValue::Null
        } else {
            JsonValue::String("xero.active_coordination".into())
        },
        "toolAvailability": {
            "agent_coordination": tools.iter().any(|tool| tool.name == AUTONOMOUS_TOOL_AGENT_COORDINATION),
        },
        "presenceCount": context.presence.len(),
        "reservationCount": context.reservations.len(),
        "eventCount": context.events.len(),
        "mailboxCount": context.mailbox.len(),
        "historyNoticeCount": history_notices.len(),
        "historyNoticeTypes": history_notice_types,
        "stalePathCount": stale_paths.len(),
        "stalePaths": stale_paths,
        "stalePathGuidance": if history_notices.is_empty() {
            JsonValue::Null
        } else {
            JsonValue::String("History notices are temporary coordination state; re-read current files before overlapping writes on affected paths.".into())
        },
        "presence": context.presence.iter().map(coordination_presence_manifest_json).collect::<Vec<_>>(),
        "reservations": context.reservations.iter().map(coordination_reservation_manifest_json).collect::<Vec<_>>(),
        "events": context.events.iter().map(coordination_event_manifest_json).collect::<Vec<_>>(),
        "mailbox": context.mailbox.iter().map(coordination_mailbox_manifest_json).collect::<Vec<_>>(),
        "historyNotices": history_notices,
    });
    (contributors, manifest)
}

fn coordination_history_notice_manifest_entries(
    context: &project_store::AgentCoordinationContext,
) -> Vec<JsonValue> {
    let mut notices = Vec::new();
    for event in context
        .events
        .iter()
        .filter(|event| is_history_coordination_event(&event.event_kind))
    {
        let affected_paths = history_event_affected_paths(event);
        notices.push(json!({
            "source": "coordination_event",
            "eventId": event.id,
            "eventKind": event.event_kind,
            "operationId": event.payload.get("operationId").and_then(JsonValue::as_str),
            "mode": event.payload.get("mode").and_then(JsonValue::as_str),
            "status": event.payload.get("status").and_then(JsonValue::as_str),
            "affectedPaths": affected_paths,
            "summary": event.summary,
            "guidance": history_event_guidance(event),
            "createdAt": event.created_at,
            "expiresAt": event.expires_at,
        }));
    }
    for delivery in context
        .mailbox
        .iter()
        .filter(|delivery| is_history_mailbox_item_type(delivery.item.item_type))
    {
        let item = &delivery.item;
        notices.push(json!({
            "source": "mailbox",
            "itemId": item.item_id,
            "itemType": item.item_type.as_str(),
            "priority": item.priority.as_str(),
            "title": item.title,
            "relatedPaths": item.related_paths,
            "guidance": history_mailbox_guidance(item.item_type),
            "acknowledgedAt": delivery.acknowledged_at,
            "createdAt": item.created_at,
            "expiresAt": item.expires_at,
        }));
    }
    notices
}

fn coordination_stale_paths(context: &project_store::AgentCoordinationContext) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for event in context
        .events
        .iter()
        .filter(|event| is_history_coordination_event(&event.event_kind))
    {
        paths.extend(history_event_affected_paths(event));
    }
    for delivery in context
        .mailbox
        .iter()
        .filter(|delivery| is_history_mailbox_item_type(delivery.item.item_type))
    {
        paths.extend(delivery.item.related_paths.iter().cloned());
    }
    paths.into_iter().collect()
}

fn coordination_history_notice_types(
    context: &project_store::AgentCoordinationContext,
) -> Vec<String> {
    let mut notice_types = BTreeSet::new();
    for event in context
        .events
        .iter()
        .filter(|event| is_history_coordination_event(&event.event_kind))
    {
        notice_types.insert(event.event_kind.clone());
    }
    for delivery in context
        .mailbox
        .iter()
        .filter(|delivery| is_history_mailbox_item_type(delivery.item.item_type))
    {
        notice_types.insert(delivery.item.item_type.as_str().to_string());
    }
    notice_types.into_iter().collect()
}

fn is_history_coordination_event(event_kind: &str) -> bool {
    matches!(
        event_kind,
        "history_rewrite_notice"
            | "undo_conflict_notice"
            | "history_operation_failed"
            | "history_operation_repair_needed"
    )
}

fn is_history_mailbox_item_type(item_type: project_store::AgentMailboxItemType) -> bool {
    matches!(
        item_type,
        project_store::AgentMailboxItemType::HistoryRewriteNotice
            | project_store::AgentMailboxItemType::UndoConflictNotice
            | project_store::AgentMailboxItemType::WorkspaceEpochAdvanced
            | project_store::AgentMailboxItemType::ReservationInvalidated
    )
}

fn history_event_affected_paths(
    event: &project_store::AgentCoordinationEventRecord,
) -> Vec<String> {
    event
        .payload
        .get("affectedPaths")
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(JsonValue::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

fn history_event_guidance(event: &project_store::AgentCoordinationEventRecord) -> &'static str {
    match event
        .payload
        .get("status")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
    {
        "conflicted" => {
            "The undo/session rollback conflicted before writing; inspect current files before overlapping work."
        }
        "failed" => "The history operation failed; inspect current workspace state before acting on affected paths.",
        "repair_needed" => {
            "The history operation needs repair; re-read current files before overlapping writes on affected paths."
        }
        _ => "Re-read current files before overlapping writes on affected paths.",
    }
}

fn history_mailbox_guidance(item_type: project_store::AgentMailboxItemType) -> &'static str {
    match item_type {
        project_store::AgentMailboxItemType::UndoConflictNotice => {
            "The undo/session rollback conflicted before writing; inspect current files before overlapping work."
        }
        project_store::AgentMailboxItemType::WorkspaceEpochAdvanced => {
            "Workspace epoch advanced; refresh context before writing affected paths."
        }
        project_store::AgentMailboxItemType::ReservationInvalidated => {
            "Existing reservations on affected paths are stale; re-read current files before renewing or writing."
        }
        project_store::AgentMailboxItemType::HistoryRewriteNotice => {
            "Re-read current files before overlapping writes on affected paths."
        }
        _ => "Treat this mailbox item as temporary coordination state.",
    }
}

fn coordination_path_preview(paths: &[String]) -> String {
    if paths.is_empty() {
        return "general work".into();
    }
    let mut preview = paths.iter().take(6).cloned().collect::<Vec<_>>();
    if paths.len() > preview.len() {
        preview.push(format!("and {} more", paths.len() - preview.len()));
    }
    preview.join(", ")
}

fn active_coordination_prompt_summary(
    context: &project_store::AgentCoordinationContext,
) -> Option<String> {
    if context.presence.is_empty()
        && context.reservations.is_empty()
        && context.events.is_empty()
        && context.mailbox.is_empty()
    {
        return None;
    }
    let mut lines = vec![
        "--- BEGIN ACTIVE AGENT COORDINATION ---".to_string(),
        "Use as advisory same-project coordination state; current files and current tool output remain higher priority.".to_string(),
    ];
    if !context.presence.is_empty() {
        lines.push("Active sibling agents:".into());
        for presence in &context.presence {
            let role = presence.role.as_deref().unwrap_or("agent");
            lines.push(format!(
                "- {} ({role}, {}, phase {}, updated {}): {}",
                presence.run_id,
                presence.status,
                presence.current_phase,
                presence.updated_at,
                presence.activity_summary
            ));
        }
    }
    if !context.reservations.is_empty() {
        lines.push("Active file reservations:".into());
        for reservation in &context.reservations {
            let owner = reservation
                .owner_child_run_id
                .as_deref()
                .unwrap_or(&reservation.owner_run_id);
            let note = reservation
                .note
                .as_deref()
                .map(|note| format!(" note: {note}"))
                .unwrap_or_default();
            lines.push(format!(
                "- {} reserved for {} by {} until {}.{}",
                reservation.path,
                reservation.operation.as_str(),
                owner,
                reservation.expires_at,
                note
            ));
        }
    }
    let history_event_count = context
        .events
        .iter()
        .filter(|event| is_history_coordination_event(&event.event_kind))
        .count();
    let history_mailbox_count = context
        .mailbox
        .iter()
        .filter(|delivery| is_history_mailbox_item_type(delivery.item.item_type))
        .count();
    if history_event_count > 0 || history_mailbox_count > 0 {
        lines.push("Code history notices:".into());
        for event in context
            .events
            .iter()
            .filter(|event| is_history_coordination_event(&event.event_kind))
        {
            let operation_id = event
                .payload
                .get("operationId")
                .and_then(JsonValue::as_str)
                .unwrap_or("unknown-operation");
            let mode = event
                .payload
                .get("mode")
                .and_then(JsonValue::as_str)
                .unwrap_or("history_operation");
            let status = event
                .payload
                .get("status")
                .and_then(JsonValue::as_str)
                .unwrap_or("unknown");
            let affected_paths = history_event_affected_paths(event);
            lines.push(format!(
                "- {} {} {} `{}` status {} affected {}: {} {}",
                event.created_at,
                event.run_id,
                mode,
                operation_id,
                status,
                coordination_path_preview(&affected_paths),
                event.summary,
                history_event_guidance(event)
            ));
        }
        for delivery in context
            .mailbox
            .iter()
            .filter(|delivery| is_history_mailbox_item_type(delivery.item.item_type))
        {
            let item = &delivery.item;
            lines.push(format!(
                "- {} {} from {} priority {} affected {}: {} {}",
                item.created_at,
                item.item_type.as_str(),
                item.sender_run_id,
                item.priority.as_str(),
                coordination_path_preview(&item.related_paths),
                item.title,
                history_mailbox_guidance(item.item_type)
            ));
        }
        lines.push(
            "History mailbox notices are temporary coordination state, not durable memory; current files remain authoritative."
                .into(),
        );
    }
    if context
        .events
        .iter()
        .any(|event| !is_history_coordination_event(&event.event_kind))
    {
        lines.push("Recent activity events:".into());
        for event in context
            .events
            .iter()
            .filter(|event| !is_history_coordination_event(&event.event_kind))
        {
            lines.push(format!(
                "- {} {} {}: {}",
                event.created_at, event.run_id, event.event_kind, event.summary
            ));
        }
    }
    if context
        .mailbox
        .iter()
        .any(|delivery| !is_history_mailbox_item_type(delivery.item.item_type))
    {
        lines.push("Temporary swarm mailbox:".into());
        for delivery in context
            .mailbox
            .iter()
            .filter(|delivery| !is_history_mailbox_item_type(delivery.item.item_type))
        {
            let item = &delivery.item;
            lines.push(format!(
                "- {} {} from {} priority {} about {}: {}",
                item.created_at,
                item.item_type.as_str(),
                item.sender_run_id,
                item.priority.as_str(),
                item.related_paths
                    .first()
                    .map(String::as_str)
                    .unwrap_or("general work"),
                item.title
            ));
        }
    }
    lines.push("--- END ACTIVE AGENT COORDINATION ---".into());
    Some(lines.join("\n"))
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

fn prompt_diff_since_previous_manifest(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    turn_index: usize,
    current_fragments: &[PromptFragment],
    current_tools: &[AgentToolDescriptor],
    current_compaction_id: Option<&str>,
) -> CommandResult<JsonValue> {
    let previous =
        project_store::list_agent_context_manifests_for_run(repo_root, project_id, run_id)?
            .into_iter()
            .rfind(|record| {
                record
                    .manifest
                    .get("turnIndex")
                    .and_then(JsonValue::as_u64)
                    .is_some_and(|previous_turn| previous_turn < turn_index as u64)
            });
    let Some(previous) = previous else {
        return Ok(json!({
            "kind": "prompt_fragment_diff",
            "schema": "xero.prompt_fragment_diff.v1",
            "basis": "no_previous_provider_context_manifest",
            "previousManifestId": JsonValue::Null,
            "added": [],
            "removed": [],
            "changed": [],
            "unchangedCount": current_fragments.len(),
            "suspectedCauses": [],
        }));
    };

    let previous_fragments = previous_prompt_fragment_hashes(&previous.manifest);
    let current_fragments_by_id = current_fragments
        .iter()
        .map(|fragment| (fragment.id.as_str(), fragment))
        .collect::<BTreeMap<_, _>>();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged_count = 0_usize;

    for (id, fragment) in &current_fragments_by_id {
        match previous_fragments.get(*id) {
            None => added.push(prompt_diff_item(
                id,
                &fragment.sha256,
                fragment.title.as_str(),
            )),
            Some(previous_sha) if previous_sha != &fragment.sha256 => {
                changed.push(json!({
                    "id": id,
                    "previousSha256": previous_sha,
                    "currentSha256": fragment.sha256,
                    "title": fragment.title,
                }));
            }
            Some(_) => unchanged_count = unchanged_count.saturating_add(1),
        }
    }
    for (id, previous_sha) in &previous_fragments {
        if !current_fragments_by_id.contains_key(id.as_str()) {
            removed.push(prompt_diff_item(
                id,
                previous_sha,
                "previous prompt fragment",
            ));
        }
    }

    let current_tool_names = current_tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<BTreeSet<_>>();
    let previous_tool_names = previous_tool_names(&previous.manifest);
    let mut suspected_causes = prompt_diff_suspected_causes(
        &added,
        &removed,
        &changed,
        current_tool_names != previous_tool_names,
    );
    let previous_compaction_id = previous
        .manifest
        .get("compactionId")
        .and_then(JsonValue::as_str);
    if previous_compaction_id != current_compaction_id {
        suspected_causes.insert("compaction".to_string());
    }

    Ok(json!({
        "kind": "prompt_fragment_diff",
        "schema": "xero.prompt_fragment_diff.v1",
        "basis": "previous_provider_context_manifest",
        "previousManifestId": previous.manifest_id,
        "added": added,
        "removed": removed,
        "changed": changed,
        "unchangedCount": unchanged_count,
        "suspectedCauses": suspected_causes.into_iter().collect::<Vec<_>>(),
    }))
}

fn previous_prompt_fragment_hashes(manifest: &JsonValue) -> BTreeMap<String, String> {
    manifest
        .get("promptFragments")
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|fragment| {
            let id = fragment.get("id").and_then(JsonValue::as_str)?;
            let sha = fragment.get("sha256").and_then(JsonValue::as_str)?;
            Some((id.to_owned(), sha.to_owned()))
        })
        .collect()
}

fn previous_tool_names(manifest: &JsonValue) -> BTreeSet<&str> {
    manifest
        .get("toolDescriptors")
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|tool| tool.get("name").and_then(JsonValue::as_str))
        .collect()
}

fn prompt_diff_item(id: &str, sha256: &str, title: &str) -> JsonValue {
    json!({
        "id": id,
        "sha256": sha256,
        "title": title,
    })
}

fn prompt_diff_suspected_causes(
    added: &[JsonValue],
    removed: &[JsonValue],
    changed: &[JsonValue],
    tool_names_changed: bool,
) -> BTreeSet<String> {
    let mut causes = BTreeSet::new();
    if tool_names_changed
        || changed
            .iter()
            .any(|item| item.get("id").and_then(JsonValue::as_str) == Some("xero.tool_policy"))
    {
        causes.insert("tool_activation".into());
    }
    for item in added.iter().chain(removed).chain(changed) {
        let Some(id) = item.get("id").and_then(JsonValue::as_str) else {
            continue;
        };
        if id == "xero.owned_process_state" {
            causes.insert("process_state".into());
        } else if id == "xero.active_coordination" {
            causes.insert("coordination".into());
        } else if id.starts_with("skill.context.") {
            causes.insert("skills".into());
        } else if id.starts_with("project.instructions.") {
            causes.insert("repository_instruction_scope".into());
        } else if id == "project.workspace_manifest" {
            causes.insert("workspace_manifest".into());
        }
    }
    causes
}

fn stable_provider_preflight_hash(snapshot: &xero_agent_core::ProviderPreflightSnapshot) -> String {
    let serialized = serde_json::to_string(snapshot).unwrap_or_else(|_| "unserializable".into());
    xero_agent_core::runtime_trace_id("provider-preflight", &[&serialized])
}

fn context_retrieval_query_text(
    runtime_agent_id: RuntimeAgentIdDto,
    messages: &[ProviderMessage],
    agent_definition_snapshot: Option<&JsonValue>,
) -> String {
    let mut selected = Vec::new();
    let mut used = 0_usize;
    if let Some(artifact_query) = consumed_artifact_retrieval_query_text(agent_definition_snapshot)
    {
        push_retrieval_query_part(
            &mut selected,
            &mut used,
            "consumed_artifacts",
            &artifact_query,
        );
    }
    for message in messages.iter().rev() {
        let (role, content) = match message {
            ProviderMessage::User { content, .. } => ("user", content.as_str()),
            ProviderMessage::Assistant { content, .. } => ("assistant", content.as_str()),
            ProviderMessage::Tool { .. } => continue,
        };
        if !push_retrieval_query_part(&mut selected, &mut used, role, content) {
            break;
        }
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

fn push_retrieval_query_part(
    selected: &mut Vec<String>,
    used: &mut usize,
    role: &str,
    content: &str,
) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }
    let remaining = MAX_RETRIEVAL_QUERY_CHARS.saturating_sub(*used);
    if remaining == 0 {
        return false;
    }
    let query_text =
        if project_store::find_prohibited_runtime_persistence_content(trimmed).is_some() {
            "[redacted]"
        } else {
            trimmed
        };
    let excerpt = truncate_chars(query_text, remaining);
    *used = (*used).saturating_add(excerpt.chars().count());
    selected.push(format!("{role}: {excerpt}"));
    *used < MAX_RETRIEVAL_QUERY_CHARS
}

fn consumed_artifact_retrieval_query_text(snapshot: Option<&JsonValue>) -> Option<String> {
    let snapshot = snapshot?;
    if snapshot
        .get("scope")
        .and_then(JsonValue::as_str)
        .is_some_and(|scope| scope == "built_in")
    {
        return None;
    }
    let artifacts = snapshot.get("consumes").and_then(JsonValue::as_array)?;
    let lines = artifacts
        .iter()
        .filter_map(|artifact| {
            let id = agent_definition_manifest_string_field(artifact, "id")?;
            let label = agent_definition_manifest_string_field(artifact, "label").unwrap_or(id);
            let contract =
                agent_definition_manifest_string_field(artifact, "contract").unwrap_or("unknown");
            let source_agent = agent_definition_manifest_string_field(artifact, "sourceAgent")
                .unwrap_or("unknown");
            let required = artifact
                .get("required")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false);
            Some(format!(
                "- {label} (`{id}`), contract={contract}, source={source_agent}, required={required}"
            ))
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        None
    } else {
        Some(format!(
            "Custom agent consumed artifact expectations:\n{}",
            lines.join("\n")
        ))
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
        "xero.runtime_metadata" => "runtime_metadata",
        "xero.tool_policy" => "tool_policy",
        "xero.agent_definition_policy" => "agent_definition_policy",
        "project.workspace_manifest" => "workspace_manifest",
        "xero.owned_process_state" => "process_state",
        "xero.code_rollback_state" => "code_rollback_state",
        "xero.durable_context_tools" => "durable_context_tool_instruction",
        "xero.working_set_context" => "working_set_context",
        "xero.active_coordination" => "active_agent_coordination",
        id if id.starts_with("project.instructions.") => "repository_instructions",
        id if id.starts_with("skill.context.") => "skill_context",
        _ => "prompt_fragment",
    }
}

fn prompt_exclusion_context_kind(exclusion: &PromptFragmentExclusion) -> &'static str {
    match exclusion.id.as_str() {
        id if id.starts_with("project.instructions.") => "repository_instructions",
        id if id.starts_with("skill.context.") => "skill_context",
        "project.workspace_manifest" => "workspace_manifest",
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

fn handoff_lineage_manifest_json(lineage: &project_store::AgentHandoffLineageRecord) -> JsonValue {
    json!({
        "schema": "xero.provider_context_handoff_lineage.v1",
        "handoffId": &lineage.handoff_id,
        "status": format!("{:?}", lineage.status).to_ascii_lowercase(),
        "sourceRunId": &lineage.source_run_id,
        "targetRunId": &lineage.target_run_id,
        "handoffRecordId": &lineage.handoff_record_id,
        "sourceContextHash": &lineage.source_context_hash,
        "firstTurnContext": {
            "bundleDeliveredInDeveloperMessage": true,
            "pendingPromptDeliveredInUserMessage": true,
            "workingSetSummaryIncluded": lineage.bundle.get("workingSetSummary").is_some(),
            "sourceCitedContinuityRecordCount": lineage
                .bundle
                .get("sourceCitedContinuityRecords")
                .and_then(JsonValue::as_array)
                .map(|records| records.len())
                .unwrap_or(0),
        },
    })
}

fn coordination_presence_manifest_json(
    presence: &project_store::AgentCoordinationPresenceRecord,
) -> JsonValue {
    json!({
        "runId": presence.run_id,
        "agentSessionId": presence.agent_session_id,
        "traceId": presence.trace_id,
        "lineageKind": presence.lineage_kind,
        "parentRunId": presence.parent_run_id,
        "parentSubagentId": presence.parent_subagent_id,
        "role": presence.role,
        "paneId": presence.pane_id,
        "status": presence.status,
        "currentPhase": presence.current_phase,
        "activitySummary": presence.activity_summary,
        "lastEventId": presence.last_event_id,
        "lastEventKind": presence.last_event_kind,
        "lastHeartbeatAt": presence.last_heartbeat_at,
        "updatedAt": presence.updated_at,
        "expiresAt": presence.expires_at,
    })
}

fn coordination_reservation_manifest_json(
    reservation: &project_store::AgentFileReservationRecord,
) -> JsonValue {
    json!({
        "reservationId": reservation.reservation_id,
        "path": reservation.path,
        "pathKind": reservation.path_kind,
        "operation": reservation.operation.as_str(),
        "ownerAgentSessionId": reservation.owner_agent_session_id,
        "ownerRunId": reservation.owner_run_id,
        "ownerChildRunId": reservation.owner_child_run_id,
        "ownerRole": reservation.owner_role,
        "ownerPaneId": reservation.owner_pane_id,
        "ownerTraceId": reservation.owner_trace_id,
        "note": reservation.note,
        "overridePresent": reservation.override_reason.is_some(),
        "claimedAt": reservation.claimed_at,
        "lastHeartbeatAt": reservation.last_heartbeat_at,
        "expiresAt": reservation.expires_at,
    })
}

fn coordination_event_manifest_json(
    event: &project_store::AgentCoordinationEventRecord,
) -> JsonValue {
    json!({
        "id": event.id,
        "runId": event.run_id,
        "traceId": event.trace_id,
        "eventKind": event.event_kind,
        "summary": event.summary,
        "createdAt": event.created_at,
        "expiresAt": event.expires_at,
    })
}

fn coordination_mailbox_manifest_json(
    delivery: &project_store::AgentMailboxDeliveryRecord,
) -> JsonValue {
    let item = &delivery.item;
    json!({
        "itemId": item.item_id,
        "itemType": item.item_type.as_str(),
        "parentItemId": item.parent_item_id,
        "senderAgentSessionId": item.sender_agent_session_id,
        "senderRunId": item.sender_run_id,
        "senderChildRunId": item.sender_child_run_id,
        "senderRole": item.sender_role,
        "senderTraceId": item.sender_trace_id,
        "targetAgentSessionId": item.target_agent_session_id,
        "targetRunId": item.target_run_id,
        "targetRole": item.target_role,
        "title": item.title,
        "relatedPaths": item.related_paths,
        "priority": item.priority.as_str(),
        "status": item.status.as_str(),
        "acknowledgedAt": delivery.acknowledged_at,
        "createdAt": item.created_at,
        "expiresAt": item.expires_at,
        "promotedRecordId": item.promoted_record_id,
    })
}

#[derive(Debug, Clone, PartialEq)]
struct SourceCitedWorkingSetContext {
    prompt_summary: String,
    citations: Vec<JsonValue>,
}

fn source_cited_working_set_context(
    response: &project_store::AgentContextRetrievalResponse,
) -> Option<SourceCitedWorkingSetContext> {
    if response.results.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    let mut citations = Vec::new();
    for result in response.results.iter().take(3) {
        let citation = result.metadata.get("citation").unwrap_or(&JsonValue::Null);
        let source_kind = citation
            .get("sourceKind")
            .and_then(JsonValue::as_str)
            .unwrap_or_else(|| retrieval_source_kind_label(&result.source_kind));
        let source_id = citation
            .get("sourceId")
            .and_then(JsonValue::as_str)
            .unwrap_or(result.source_id.as_str());
        let title = citation
            .get("title")
            .and_then(JsonValue::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Untitled durable context item");
        let citation_label = format!("{source_kind}:{source_id}");
        lines.push(format!(
            "- rank {} `{}`: {}. Retrieve exact content with `project_context_get` before relying on details.",
            result.rank,
            citation_label,
            truncate_chars(title, 120)
        ));
        citations.push(json!({
            "resultId": result.result_id,
            "rank": result.rank,
            "sourceKind": source_kind,
            "sourceId": source_id,
            "title": title,
            "score": result.score,
            "redactionState": redaction_state_label(&result.redaction_state),
        }));
    }
    Some(SourceCitedWorkingSetContext {
        prompt_summary: lines.join("\n"),
        citations,
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
        "metadata": retrieval_result_manifest_metadata(&result.metadata),
    })
}

fn retrieval_result_manifest_metadata(metadata: &JsonValue) -> JsonValue {
    json!({
        "freshness": metadata.get("freshness").cloned().unwrap_or(JsonValue::Null),
        "trust": metadata.get("trust").cloned().unwrap_or(JsonValue::Null),
        "recordKind": metadata.get("recordKind").cloned().unwrap_or(JsonValue::Null),
        "memoryKind": metadata.get("memoryKind").cloned().unwrap_or(JsonValue::Null),
        "scope": metadata.get("scope").cloned().unwrap_or(JsonValue::Null),
        "runtimeAgentId": metadata.get("runtimeAgentId").cloned().unwrap_or(JsonValue::Null),
        "agentSessionId": metadata.get("agentSessionId").cloned().unwrap_or(JsonValue::Null),
        "runId": metadata.get("runId").cloned().unwrap_or(JsonValue::Null),
        "sourceRunId": metadata.get("sourceRunId").cloned().unwrap_or(JsonValue::Null),
        "sourceItemIds": metadata.get("sourceItemIds").cloned().unwrap_or(JsonValue::Null),
        "relatedPaths": metadata.get("relatedPaths").cloned().unwrap_or(JsonValue::Null),
        "confidence": metadata.get("confidence").cloned().unwrap_or(JsonValue::Null),
        "embeddingPresent": metadata.get("embeddingPresent").cloned().unwrap_or(JsonValue::Null),
        "embeddingModel": metadata.get("embeddingModel").cloned().unwrap_or(JsonValue::Null),
        "embeddingDimension": metadata.get("embeddingDimension").cloned().unwrap_or(JsonValue::Null),
        "embeddingVersion": metadata.get("embeddingVersion").cloned().unwrap_or(JsonValue::Null),
    })
}

fn freshness_count(freshness_diagnostics: &JsonValue, key: &str) -> u64 {
    freshness_diagnostics
        .get(key)
        .and_then(JsonValue::as_u64)
        .unwrap_or(0)
}

pub(crate) fn context_policy_action_label(
    action: &project_store::AgentContextPolicyAction,
) -> &'static str {
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

    fn retrieval_policy_snapshot(
        id: &str,
        enabled: bool,
        record_kinds: Vec<&str>,
        memory_kinds: Vec<&str>,
        limit: u32,
    ) -> JsonValue {
        json!({
            "schema": "xero.agent_definition.v1",
            "schemaVersion": 1,
            "id": id,
            "version": 1,
            "scope": "project_custom",
            "displayName": id,
            "taskPurpose": "Exercise first-turn context policy.",
            "retrievalDefaults": {
                "enabled": enabled,
                "recordKinds": record_kinds,
                "memoryKinds": memory_kinds,
                "limit": limit
            }
        })
    }

    fn assemble_snapshot_context_package(
        repo_root: &Path,
        project_id: &str,
        snapshot: &JsonValue,
        messages: &[ProviderMessage],
    ) -> ProviderContextPackage {
        let definition_id = snapshot
            .get("id")
            .and_then(JsonValue::as_str)
            .expect("snapshot definition id");
        let definition_version = snapshot
            .get("version")
            .and_then(JsonValue::as_u64)
            .expect("snapshot definition version") as u32;
        assemble_provider_context_package(
            ProviderContextPackageInput {
                repo_root,
                project_id,
                agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID,
                run_id: "run-context-package",
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: definition_id,
                agent_definition_version: definition_version,
                agent_definition_snapshot: Some(snapshot),
                provider_id: OPENAI_CODEX_PROVIDER_ID,
                model_id: OPENAI_CODEX_PROVIDER_ID,
                turn_index: 0,
                browser_control_preference: BrowserControlPreferenceDto::Default,
                soul_settings: None,
                tools: &[],
                tool_exposure_plan: None,
                messages,
                owned_process_summary: None,
                provider_preflight: None,
            },
            Vec::new(),
        )
        .expect("assemble snapshot context package")
    }

    fn seed_custom_definition(repo_root: &Path, snapshot: &JsonValue) {
        let definition_id = snapshot
            .get("id")
            .and_then(JsonValue::as_str)
            .expect("snapshot definition id");
        let definition_version = snapshot
            .get("version")
            .and_then(JsonValue::as_u64)
            .expect("snapshot definition version") as u32;
        let display_name = snapshot
            .get("displayName")
            .and_then(JsonValue::as_str)
            .unwrap_or(definition_id);
        project_store::insert_agent_definition(
            repo_root,
            &project_store::NewAgentDefinitionRecord {
                definition_id: definition_id.into(),
                version: definition_version,
                display_name: display_name.into(),
                short_label: display_name.chars().take(2).collect(),
                description: "Exercise first-turn context policy.".into(),
                scope: "project_custom".into(),
                lifecycle_state: "active".into(),
                base_capability_profile: "engineering".into(),
                snapshot: snapshot.clone(),
                validation_report: Some(json!({ "status": "valid" })),
                created_at: "2026-05-01T12:00:00Z".into(),
                updated_at: "2026-05-01T12:00:00Z".into(),
            },
        )
        .expect("seed custom definition");
    }

    #[test]
    fn s26_provider_context_package_admits_source_cited_working_set_summary() {
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
            tool_exposure_plan: None,
            messages: &messages,
            owned_process_summary: None,
            provider_preflight: None,
        };

        let first = assemble_provider_context_package(input, Vec::new()).expect("first package");
        let second = assemble_provider_context_package(input, Vec::new()).expect("second package");

        assert_eq!(first.manifest.context_hash, second.manifest.context_hash);
        assert!(first.system_prompt.contains("Durable project context is"));
        assert!(first
            .system_prompt
            .contains("Source-cited working set for this turn"));
        assert!(first
            .system_prompt
            .contains("project_record:project-record-phase3"));
        assert!(!first
            .system_prompt
            .contains("Phase 3 approved memory is injected"));
        assert!(!first
            .system_prompt
            .contains("phase3 context package assembly retrieves project records"));
        assert!(first.compilation.fragments.iter().any(|fragment| {
            fragment.id == "xero.durable_context_tools" && fragment.priority == 240
        }));
        assert!(first.compilation.fragments.iter().any(|fragment| {
            fragment.id == "xero.working_set_context" && fragment.priority == 245
        }));
        assert_eq!(
            first.manifest.manifest["retrieval"]["deliveryModel"],
            "tool_mediated"
        );
        assert_eq!(
            first.manifest.manifest["retrieval"]["rawContextInjected"],
            false
        );
        assert_eq!(
            first.manifest.manifest["workingSet"]["deliveryModel"],
            "admitted_source_cited_summary"
        );
        assert_eq!(
            first.manifest.manifest["workingSet"]["rawDurableContextInjected"],
            false
        );
        assert_eq!(
            first.manifest.manifest["workingSet"]["promptFragmentId"],
            "xero.working_set_context"
        );
        assert!(first.manifest.manifest["workingSet"]["citations"]
            .as_array()
            .expect("working set citations")
            .iter()
            .any(|citation| citation["sourceId"] == "project-record-phase3"));
        assert!(!first.manifest.retrieval_result_ids.is_empty());
    }

    #[test]
    fn s27_provider_context_package_honors_per_agent_first_turn_context_policy() {
        let root = tempfile::tempdir().expect("temp dir");
        let (project_id, repo_root) = seed_project(&root);
        seed_run(&repo_root, &project_id);
        seed_retrievable_context(&repo_root, &project_id);
        let messages = vec![ProviderMessage::User {
            content: "Use Phase 3 approved memory and context package decision.".into(),
            attachments: Vec::new(),
        }];
        let record_snapshot =
            retrieval_policy_snapshot("record-first-turn", true, vec!["decision"], Vec::new(), 1);
        let memory_snapshot = retrieval_policy_snapshot(
            "memory-first-turn",
            true,
            Vec::new(),
            vec!["project_fact"],
            1,
        );
        let disabled_snapshot = retrieval_policy_snapshot(
            "tool-mediated-only",
            false,
            vec!["decision"],
            vec!["project_fact"],
            2,
        );
        seed_custom_definition(&repo_root, &record_snapshot);
        seed_custom_definition(&repo_root, &memory_snapshot);
        seed_custom_definition(&repo_root, &disabled_snapshot);

        let record_package =
            assemble_snapshot_context_package(&repo_root, &project_id, &record_snapshot, &messages);
        let memory_package =
            assemble_snapshot_context_package(&repo_root, &project_id, &memory_snapshot, &messages);
        let disabled_package = assemble_snapshot_context_package(
            &repo_root,
            &project_id,
            &disabled_snapshot,
            &messages,
        );

        assert_eq!(
            record_package.manifest.manifest["retrieval"]["firstTurnPolicy"]["searchScope"],
            json!("project_records")
        );
        assert_eq!(
            record_package.manifest.manifest["retrieval"]["firstTurnPolicy"]["recordKinds"],
            json!(["decision"])
        );
        assert_eq!(
            record_package.manifest.manifest["workingSet"]["citations"][0]["sourceKind"],
            json!("project_record")
        );
        assert_eq!(
            record_package.manifest.manifest["workingSet"]["citations"][0]["sourceId"],
            json!("project-record-phase3")
        );
        assert!(!record_package
            .system_prompt
            .contains("Phase 3 approved memory is injected"));

        assert_eq!(
            memory_package.manifest.manifest["retrieval"]["firstTurnPolicy"]["searchScope"],
            json!("approved_memory")
        );
        assert_eq!(
            memory_package.manifest.manifest["retrieval"]["firstTurnPolicy"]["memoryKinds"],
            json!(["project_fact"])
        );
        assert_eq!(
            memory_package.manifest.manifest["workingSet"]["citations"][0]["sourceKind"],
            json!("approved_memory")
        );
        assert_eq!(
            memory_package.manifest.manifest["workingSet"]["citations"][0]["sourceId"],
            json!("memory-phase3")
        );
        assert!(memory_package
            .system_prompt
            .contains("approved_memory:memory-phase3"));
        assert!(!memory_package
            .system_prompt
            .contains("Phase 3 approved memory is injected"));

        assert_eq!(
            disabled_package.manifest.manifest["retrieval"]["firstTurnPolicy"]
                ["autoSummaryEnabled"],
            json!(false)
        );
        assert_eq!(
            disabled_package.manifest.manifest["retrieval"]["firstTurnPolicy"]["searchScope"],
            json!("hybrid_context")
        );
        assert_eq!(
            disabled_package.manifest.manifest["workingSet"]["deliveryModel"],
            json!("none")
        );
        assert_eq!(
            disabled_package.manifest.manifest["workingSet"]["citationCount"],
            json!(0)
        );
        assert!(!disabled_package
            .compilation
            .fragments
            .iter()
            .any(|fragment| fragment.id == "xero.working_set_context"));
        assert!(!disabled_package
            .system_prompt
            .contains("Source-cited working set for this turn"));
    }

    #[test]
    fn s15_provider_context_manifest_records_custom_database_touchpoints() {
        let root = tempfile::tempdir().expect("temp dir");
        let (project_id, repo_root) = seed_project(&root);
        seed_run(&repo_root, &project_id);
        let messages = vec![ProviderMessage::User {
            content: "Use the saved database touchpoints.".into(),
            attachments: Vec::new(),
        }];
        let snapshot = json!({
            "schema": "xero.agent_definition.v1",
            "schemaVersion": 1,
            "id": "db-scribe",
            "version": 4,
            "scope": "project_custom",
            "displayName": "DB Scribe",
            "taskPurpose": "Use durable project context tables deliberately.",
            "dbTouchpoints": {
                "reads": [
                    {
                        "table": "project_context_records",
                        "kind": "read",
                        "purpose": "Read reviewed project facts before answering.",
                        "columns": ["record_kind", "summary", "text"],
                        "triggers": [{"kind": "lifecycle", "event": "run_start"}]
                    }
                ],
                "writes": [
                    {
                        "table": "agent_context_manifests",
                        "kind": "write",
                        "purpose": "Persist provider context audit data.",
                        "columns": ["manifest", "context_hash"],
                        "triggers": [{"kind": "lifecycle", "event": "message_persisted"}]
                    }
                ],
                "encouraged": []
            }
        });
        project_store::insert_agent_definition(
            &repo_root,
            &project_store::NewAgentDefinitionRecord {
                definition_id: "db-scribe".into(),
                version: 4,
                display_name: "DB Scribe".into(),
                short_label: "DB".into(),
                description: "Use durable project context tables deliberately.".into(),
                scope: "project_custom".into(),
                lifecycle_state: "active".into(),
                base_capability_profile: "engineering".into(),
                snapshot: snapshot.clone(),
                validation_report: None,
                created_at: "2026-05-01T12:00:00Z".into(),
                updated_at: "2026-05-01T12:00:00Z".into(),
            },
        )
        .expect("seed custom definition");
        let input = ProviderContextPackageInput {
            repo_root: &repo_root,
            project_id: &project_id,
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID,
            run_id: "run-context-package",
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "db-scribe",
            agent_definition_version: 4,
            agent_definition_snapshot: Some(&snapshot),
            provider_id: OPENAI_CODEX_PROVIDER_ID,
            model_id: OPENAI_CODEX_PROVIDER_ID,
            turn_index: 0,
            browser_control_preference: BrowserControlPreferenceDto::Default,
            soul_settings: None,
            tools: &[],
            tool_exposure_plan: None,
            messages: &messages,
            owned_process_summary: None,
            provider_preflight: None,
        };

        let package =
            assemble_provider_context_package(input, Vec::new()).expect("assemble context package");

        assert!(package
            .system_prompt
            .contains("Database touchpoints:\nreads:"));
        assert_eq!(
            package.manifest.manifest["agentDefinition"]["dbTouchpointCount"],
            json!(2)
        );
        assert_eq!(
            package.manifest.manifest["agentDefinition"]["dbTouchpoints"]["reads"][0]["table"],
            json!("project_context_records")
        );
        assert_eq!(
            package.manifest.manifest["agentDefinition"]["dbTouchpoints"]["writes"][0]["table"],
            json!("agent_context_manifests")
        );
        assert!(package.manifest.manifest["contributors"]["included"]
            .as_array()
            .expect("included contributors")
            .iter()
            .any(
                |contributor| contributor["kind"] == "agent_definition_db_touchpoints"
                    && contributor["sourceId"] == "db-scribe@4"
            ));
    }

    #[test]
    fn s16_provider_context_manifest_records_consumed_artifact_preflight() {
        let root = tempfile::tempdir().expect("temp dir");
        let (project_id, repo_root) = seed_project(&root);
        seed_run(&repo_root, &project_id);
        project_store::insert_project_record(
            &repo_root,
            &project_store::NewProjectRecordRecord {
                record_id: "project-record-plan-pack".into(),
                project_id: project_id.clone(),
                record_kind: project_store::ProjectRecordKind::Plan,
                runtime_agent_id: RuntimeAgentIdDto::Plan,
                agent_definition_id: "plan".into(),
                agent_definition_version: project_store::BUILTIN_AGENT_DEFINITION_VERSION,
                agent_session_id: Some(project_store::DEFAULT_AGENT_SESSION_ID.into()),
                run_id: "run-plan-pack".into(),
                workflow_run_id: None,
                workflow_step_id: None,
                title: "Accepted Plan Pack".into(),
                summary: "Accepted plan with implementation slices.".into(),
                text: "Accepted xero.plan_pack.v1: build the custom agent runtime slices.".into(),
                content_json: Some(json!({
                    "schema": "xero.plan_pack.v1",
                    "status": "accepted",
                    "slices": ["S16"],
                    "build_handoff": "Continue from S16."
                })),
                schema_name: Some("xero.plan_pack.v1".into()),
                schema_version: 1,
                importance: project_store::ProjectRecordImportance::High,
                confidence: Some(0.99),
                tags: vec!["artifact:plan_pack".into(), "contract:plan_pack".into()],
                source_item_ids: Vec::new(),
                related_paths: Vec::new(),
                produced_artifact_refs: vec!["plan_pack".into()],
                redaction_state: project_store::ProjectRecordRedactionState::Clean,
                visibility: project_store::ProjectRecordVisibility::Retrieval,
                created_at: "2026-05-01T12:03:00Z".into(),
            },
        )
        .expect("seed accepted plan pack");
        let snapshot = json!({
            "schema": "xero.agent_definition.v1",
            "schemaVersion": 1,
            "id": "handoff-engineer",
            "version": 3,
            "scope": "project_custom",
            "displayName": "Handoff Engineer",
            "taskPurpose": "Continue implementation from accepted plan artifacts.",
            "consumes": [
                {
                    "id": "plan_pack",
                    "label": "Accepted Plan Pack",
                    "description": "The accepted xero.plan_pack.v1 with slices and build handoff.",
                    "sourceAgent": "plan",
                    "contract": "plan_pack",
                    "sections": ["decisions", "slices", "build_handoff"],
                    "required": true
                }
            ]
        });
        project_store::insert_agent_definition(
            &repo_root,
            &project_store::NewAgentDefinitionRecord {
                definition_id: "handoff-engineer".into(),
                version: 3,
                display_name: "Handoff Engineer".into(),
                short_label: "HE".into(),
                description: "Continue from accepted plan artifacts.".into(),
                scope: "project_custom".into(),
                lifecycle_state: "active".into(),
                base_capability_profile: "engineering".into(),
                snapshot: snapshot.clone(),
                validation_report: None,
                created_at: "2026-05-01T12:00:00Z".into(),
                updated_at: "2026-05-01T12:00:00Z".into(),
            },
        )
        .expect("seed custom definition");
        let messages = vec![ProviderMessage::User {
            content: "Continue from the saved plan.".into(),
            attachments: Vec::new(),
        }];
        let input = ProviderContextPackageInput {
            repo_root: &repo_root,
            project_id: &project_id,
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID,
            run_id: "run-context-package",
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "handoff-engineer",
            agent_definition_version: 3,
            agent_definition_snapshot: Some(&snapshot),
            provider_id: OPENAI_CODEX_PROVIDER_ID,
            model_id: OPENAI_CODEX_PROVIDER_ID,
            turn_index: 0,
            browser_control_preference: BrowserControlPreferenceDto::Default,
            soul_settings: None,
            tools: &[],
            tool_exposure_plan: None,
            messages: &messages,
            owned_process_summary: None,
            provider_preflight: None,
        };

        let package =
            assemble_provider_context_package(input, Vec::new()).expect("assemble context package");

        assert!(package.system_prompt.contains("Consumed artifacts:"));
        assert_eq!(
            package.manifest.manifest["agentDefinition"]["consumedArtifactCount"],
            json!(1)
        );
        assert_eq!(
            package.manifest.manifest["agentDefinition"]["consumes"][0]["id"],
            json!("plan_pack")
        );
        assert_eq!(
            package.manifest.manifest["agentDefinition"]["consumedArtifactPreflight"]["status"],
            json!("ready")
        );
        assert_eq!(
            package.manifest.manifest["agentDefinition"]["consumedArtifactPreflight"]["matched"][0]
                ["recordId"],
            json!("project-record-plan-pack")
        );
        assert!(package.manifest.manifest["contributors"]["included"]
            .as_array()
            .expect("included contributors")
            .iter()
            .any(
                |contributor| contributor["kind"] == "agent_definition_consumed_artifacts"
                    && contributor["sourceId"] == "handoff-engineer@3"
            ));
    }

    #[test]
    fn s54_provider_context_manifest_redacts_custom_definition_metadata() {
        let root = tempfile::tempdir().expect("temp dir");
        let (project_id, repo_root) = seed_project(&root);
        seed_run(&repo_root, &project_id);
        let snapshot = json!({
            "schema": "xero.agent_definition.v1",
            "schemaVersion": 1,
            "id": "secret-scribe",
            "version": 1,
            "scope": "project_custom",
            "displayName": "Secret Scribe",
            "taskPurpose": "Verify manifest redaction.",
            "dbTouchpoints": {
                "reads": [
                    {
                        "table": "project_context_records",
                        "kind": "read",
                        "purpose": "Never persist api_key=sk-test-secret-value in manifests.",
                        "columns": [],
                        "triggers": []
                    }
                ],
                "writes": [],
                "encouraged": []
            },
            "consumes": []
        });
        project_store::insert_agent_definition(
            &repo_root,
            &project_store::NewAgentDefinitionRecord {
                definition_id: "secret-scribe".into(),
                version: 1,
                display_name: "Secret Scribe".into(),
                short_label: "SS".into(),
                description: "Verify manifest redaction.".into(),
                scope: "project_custom".into(),
                lifecycle_state: "active".into(),
                base_capability_profile: "engineering".into(),
                snapshot: snapshot.clone(),
                validation_report: Some(json!({ "status": "valid" })),
                created_at: "2026-05-01T12:00:00Z".into(),
                updated_at: "2026-05-01T12:00:00Z".into(),
            },
        )
        .expect("seed custom definition");
        let messages = vec![ProviderMessage::User {
            content: "Check redaction coverage.".into(),
            attachments: Vec::new(),
        }];
        let input = ProviderContextPackageInput {
            repo_root: &repo_root,
            project_id: &project_id,
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID,
            run_id: "run-context-package",
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "secret-scribe",
            agent_definition_version: 1,
            agent_definition_snapshot: Some(&snapshot),
            provider_id: OPENAI_CODEX_PROVIDER_ID,
            model_id: OPENAI_CODEX_PROVIDER_ID,
            turn_index: 0,
            browser_control_preference: BrowserControlPreferenceDto::Default,
            soul_settings: None,
            tools: &[],
            tool_exposure_plan: None,
            messages: &messages,
            owned_process_summary: None,
            provider_preflight: None,
        };

        let package =
            assemble_provider_context_package(input, Vec::new()).expect("assemble context package");

        assert_eq!(
            package.manifest.manifest["redactionState"],
            json!("redacted")
        );
        assert_eq!(
            package.manifest.manifest["agentDefinition"]["dbTouchpoints"]["reads"][0]["purpose"],
            json!("[REDACTED]")
        );
        assert!(!package
            .manifest
            .manifest
            .to_string()
            .contains("sk-test-secret-value"));
    }

    #[test]
    fn provider_context_manifest_explains_path_scoped_repository_instructions() {
        let root = tempfile::tempdir().expect("temp dir");
        let (project_id, repo_root) = seed_project(&root);
        seed_run(&repo_root, &project_id);
        fs::create_dir_all(repo_root.join("client/src")).expect("create nested source dir");
        fs::write(
            repo_root.join("client").join("AGENTS.md"),
            "Use client rules.\n",
        )
        .expect("write client instructions");

        let first_messages = vec![ProviderMessage::User {
            content: "Inspect the project before choosing files.".into(),
            attachments: Vec::new(),
        }];
        let first = assemble_provider_context_package(
            ProviderContextPackageInput {
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
                tool_exposure_plan: None,
                messages: &first_messages,
                owned_process_summary: None,
                provider_preflight: None,
            },
            Vec::new(),
        )
        .expect("assemble first package");

        assert!(!first
            .system_prompt
            .contains("--- BEGIN PROJECT INSTRUCTIONS: client/AGENTS.md ---"));
        assert!(first.manifest.manifest["promptFragmentExclusions"]
            .as_array()
            .expect("prompt exclusions")
            .iter()
            .any(|exclusion| {
                exclusion["id"] == "project.instructions.client.AGENTS.md"
                    && exclusion["reason"]
                        == "nested_repository_instruction_deferred_until_path_scope_exists"
            }));

        let second_messages = vec![
            ProviderMessage::User {
                content: "Inspect the project before choosing files.".into(),
                attachments: Vec::new(),
            },
            ProviderMessage::Assistant {
                content: String::new(),
                reasoning_content: None,
                reasoning_details: None,
                tool_calls: vec![AgentToolCall {
                    tool_call_id: "read-client-main".into(),
                    tool_name: AUTONOMOUS_TOOL_READ.into(),
                    input: json!({"path": "client/src/main.rs"}),
                }],
            },
        ];
        let second = assemble_provider_context_package(
            ProviderContextPackageInput {
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
                turn_index: 1,
                browser_control_preference: BrowserControlPreferenceDto::Default,
                soul_settings: None,
                tools: &[],
                tool_exposure_plan: None,
                messages: &second_messages,
                owned_process_summary: None,
                provider_preflight: None,
            },
            Vec::new(),
        )
        .expect("assemble second package");

        assert!(second
            .system_prompt
            .contains("--- BEGIN PROJECT INSTRUCTIONS: client/AGENTS.md ---"));
        assert_eq!(
            second.manifest.manifest["promptAssembly"]["relevantPaths"][0],
            "client/src/main.rs"
        );
        assert!(second.manifest.manifest["promptDiff"]["suspectedCauses"]
            .as_array()
            .expect("prompt diff causes")
            .iter()
            .any(|cause| cause == "repository_instruction_scope"));
    }

    #[test]
    fn provider_context_package_includes_active_coordination_contributors() {
        let root = tempfile::tempdir().expect("temp dir");
        let (project_id, repo_root) = seed_project(&root);
        seed_run(&repo_root, &project_id);
        project_store::insert_agent_run(
            &repo_root,
            &project_store::NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                project_id: project_id.clone(),
                agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: "run-sibling".into(),
                provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                model_id: OPENAI_CODEX_PROVIDER_ID.into(),
                prompt: "Edit sibling path.".into(),
                system_prompt: "system".into(),
                now: "2026-05-01T12:00:00Z".into(),
            },
        )
        .expect("seed sibling run");
        let presence = project_store::upsert_agent_coordination_presence(
            &repo_root,
            &project_store::UpsertAgentCoordinationPresenceRecord {
                project_id: project_id.clone(),
                run_id: "run-sibling".into(),
                pane_id: Some("pane-2".into()),
                status: "running".into(),
                current_phase: "editing".into(),
                activity_summary: "Sibling run is editing src/shared.rs.".into(),
                last_event_id: None,
                last_event_kind: None,
                updated_at: "2099-05-01T12:01:00Z".into(),
                lease_seconds: Some(600),
            },
        )
        .expect("publish sibling presence");
        let reservation = project_store::claim_agent_file_reservations(
            &repo_root,
            &project_store::ClaimAgentFileReservationRequest {
                project_id: project_id.clone(),
                owner_run_id: "run-sibling".into(),
                paths: vec!["src/shared.rs".into()],
                operation: project_store::AgentCoordinationReservationOperation::Editing,
                note: Some("Sibling edit in progress.".into()),
                override_reason: None,
                claimed_at: "2099-05-01T12:01:00Z".into(),
                lease_seconds: Some(600),
            },
        )
        .expect("claim sibling reservation")
        .claimed
        .into_iter()
        .next()
        .expect("reservation");
        let event = project_store::append_agent_coordination_event(
            &repo_root,
            &project_store::NewAgentCoordinationEventRecord {
                project_id: project_id.clone(),
                run_id: "run-sibling".into(),
                event_kind: "tool_call_started".into(),
                summary: "Sibling started edit.".into(),
                payload: json!({"toolName": "edit"}),
                created_at: "2099-05-01T12:01:00Z".into(),
                lease_seconds: Some(600),
            },
        )
        .expect("append sibling event");
        let mailbox = project_store::publish_agent_mailbox_item(
            &repo_root,
            &project_store::NewAgentMailboxItemRecord {
                project_id: project_id.clone(),
                sender_run_id: "run-sibling".into(),
                item_type: project_store::AgentMailboxItemType::Question,
                parent_item_id: None,
                target_agent_session_id: Some(project_store::DEFAULT_AGENT_SESSION_ID.into()),
                target_run_id: Some("run-context-package".into()),
                target_role: None,
                title: "Can current run avoid src/shared.rs?".into(),
                body: "Sibling work has an active edit reservation on src/shared.rs.".into(),
                related_paths: vec!["src/shared.rs".into()],
                priority: project_store::AgentMailboxPriority::High,
                created_at: "2099-05-01T12:02:00Z".into(),
                ttl_seconds: Some(600),
            },
        )
        .expect("publish mailbox item");

        let messages = vec![ProviderMessage::User {
            content: "Coordinate with active sibling work.".into(),
            attachments: Vec::new(),
        }];
        let tools = builtin_tool_descriptors()
            .into_iter()
            .filter(|tool| tool.name == AUTONOMOUS_TOOL_AGENT_COORDINATION)
            .collect::<Vec<_>>();
        let package = assemble_provider_context_package(
            ProviderContextPackageInput {
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
                tools: &tools,
                tool_exposure_plan: None,
                messages: &messages,
                owned_process_summary: None,
                provider_preflight: None,
            },
            Vec::new(),
        )
        .expect("assemble provider package");

        assert!(package.system_prompt.contains("Active sibling agents:"));
        assert!(package.system_prompt.contains("Temporary swarm mailbox:"));
        assert!(package
            .compilation
            .fragments
            .iter()
            .any(|fragment| fragment.id == "xero.active_coordination"));
        let included = package.manifest.manifest["contributors"]["included"]
            .as_array()
            .expect("included contributors");
        assert!(included.iter().any(|contributor| {
            contributor["contributorId"]
                == format!("agent_coordination_presence:{}", presence.run_id)
        }));
        assert!(included.iter().any(|contributor| {
            contributor["contributorId"]
                == format!("agent_file_reservation:{}", reservation.reservation_id)
        }));
        assert!(included.iter().any(|contributor| {
            contributor["contributorId"] == format!("agent_coordination_event:{}", event.id)
        }));
        assert!(included.iter().any(|contributor| {
            contributor["contributorId"] == format!("agent_mailbox_item:{}", mailbox.item_id)
        }));
        assert_eq!(
            package.manifest.manifest["coordination"]["presenceCount"],
            1
        );
        assert_eq!(
            package.manifest.manifest["coordination"]["reservationCount"],
            1
        );
        assert_eq!(package.manifest.manifest["coordination"]["eventCount"], 1);
        assert_eq!(package.manifest.manifest["coordination"]["mailboxCount"], 1);
    }

    #[test]
    fn provider_context_package_includes_history_notices_without_durable_memory_promotion() {
        let root = tempfile::tempdir().expect("temp dir");
        let (project_id, repo_root) = seed_project(&root);
        seed_run(&repo_root, &project_id);
        project_store::insert_agent_run(
            &repo_root,
            &project_store::NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                project_id: project_id.clone(),
                agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: "run-history-owner".into(),
                provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                model_id: OPENAI_CODEX_PROVIDER_ID.into(),
                prompt: "Apply a code history operation.".into(),
                system_prompt: "system".into(),
                now: "2026-05-01T12:00:00Z".into(),
            },
        )
        .expect("seed history owner run");
        let event = project_store::append_agent_coordination_event(
            &repo_root,
            &project_store::NewAgentCoordinationEventRecord {
                project_id: project_id.clone(),
                run_id: "run-history-owner".into(),
                event_kind: "history_rewrite_notice".into(),
                summary: "Session rollback completed across one path.".into(),
                payload: json!({
                    "operationId": "history-op-context",
                    "mode": "session_rollback",
                    "status": "completed",
                    "affectedPaths": ["src/shared.rs"],
                    "resultCommitId": "code-commit-history-context",
                }),
                created_at: "2099-05-01T12:01:00Z".into(),
                lease_seconds: Some(600),
            },
        )
        .expect("append history event");
        let mailbox = project_store::publish_agent_mailbox_item(
            &repo_root,
            &project_store::NewAgentMailboxItemRecord {
                project_id: project_id.clone(),
                sender_run_id: "run-history-owner".into(),
                item_type: project_store::AgentMailboxItemType::ReservationInvalidated,
                parent_item_id: None,
                target_agent_session_id: Some(project_store::DEFAULT_AGENT_SESSION_ID.into()),
                target_run_id: Some("run-context-package".into()),
                target_role: None,
                title: "Session rollback changed your reserved path".into(),
                body: "Code history operation `history-op-context` completed. Re-read current files before overlapping writes.".into(),
                related_paths: vec!["src/shared.rs".into()],
                priority: project_store::AgentMailboxPriority::High,
                created_at: "2099-05-01T12:02:00Z".into(),
                ttl_seconds: Some(600),
            },
        )
        .expect("publish history mailbox item");

        let messages = vec![ProviderMessage::User {
            content: "Continue after any code history changes.".into(),
            attachments: Vec::new(),
        }];
        let tools = builtin_tool_descriptors()
            .into_iter()
            .filter(|tool| tool.name == AUTONOMOUS_TOOL_AGENT_COORDINATION)
            .collect::<Vec<_>>();
        let package = assemble_provider_context_package(
            ProviderContextPackageInput {
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
                tools: &tools,
                tool_exposure_plan: None,
                messages: &messages,
                owned_process_summary: None,
                provider_preflight: None,
            },
            Vec::new(),
        )
        .expect("assemble provider package");

        assert!(package.system_prompt.contains("Code history notices:"));
        assert!(package.system_prompt.contains("history-op-context"));
        assert!(package.system_prompt.contains("src/shared.rs"));
        assert!(package
            .system_prompt
            .contains("Re-read current files before overlapping writes"));
        assert!(package
            .system_prompt
            .contains("temporary coordination state, not durable memory"));
        assert_eq!(
            package.manifest.manifest["coordination"]["rawDurableMemoryInjected"],
            false
        );
        assert_eq!(
            package.manifest.manifest["coordination"]["historyNoticeCount"],
            2
        );
        assert_eq!(
            package.manifest.manifest["coordination"]["stalePaths"],
            json!(["src/shared.rs"])
        );
        assert_eq!(
            package.manifest.manifest["coordination"]["stalePathGuidance"],
            "History notices are temporary coordination state; re-read current files before overlapping writes on affected paths."
        );
        assert!(package.manifest.manifest["coordination"]["historyNotices"]
            .as_array()
            .expect("history notices")
            .iter()
            .any(|notice| notice["eventId"] == event.id));
        assert!(package.manifest.manifest["coordination"]["historyNotices"]
            .as_array()
            .expect("history notices")
            .iter()
            .any(|notice| notice["itemId"] == mailbox.item_id));

        let included = package.manifest.manifest["contributors"]["included"]
            .as_array()
            .expect("included contributors");
        let mailbox_contributor = included
            .iter()
            .find(|contributor| {
                contributor["contributorId"] == format!("agent_mailbox_item:{}", mailbox.item_id)
            })
            .expect("history mailbox contributor");
        assert_eq!(
            mailbox_contributor["kind"],
            "active_code_history_mailbox_notice"
        );
        assert_eq!(
            mailbox_contributor["reason"],
            "temporary_code_history_mailbox_notice"
        );
    }

    #[test]
    fn provider_context_package_includes_mailbox_only_swarm_context() {
        let root = tempfile::tempdir().expect("temp dir");
        let (project_id, repo_root) = seed_project(&root);
        seed_run(&repo_root, &project_id);
        project_store::insert_agent_run(
            &repo_root,
            &project_store::NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                project_id: project_id.clone(),
                agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: "run-mailbox-sibling".into(),
                provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                model_id: OPENAI_CODEX_PROVIDER_ID.into(),
                prompt: "Ask a coordination question.".into(),
                system_prompt: "system".into(),
                now: "2026-05-01T12:00:00Z".into(),
            },
        )
        .expect("seed sibling run");
        let mailbox = project_store::publish_agent_mailbox_item(
            &repo_root,
            &project_store::NewAgentMailboxItemRecord {
                project_id: project_id.clone(),
                sender_run_id: "run-mailbox-sibling".into(),
                item_type: project_store::AgentMailboxItemType::Blocker,
                parent_item_id: None,
                target_agent_session_id: Some(project_store::DEFAULT_AGENT_SESSION_ID.into()),
                target_run_id: Some("run-context-package".into()),
                target_role: None,
                title: "Generated bindings are mid-update".into(),
                body: "Avoid src/generated until the sibling run finishes verification.".into(),
                related_paths: vec!["src/generated".into()],
                priority: project_store::AgentMailboxPriority::Urgent,
                created_at: "2099-05-01T12:02:00Z".into(),
                ttl_seconds: Some(600),
            },
        )
        .expect("publish mailbox item");

        let messages = vec![ProviderMessage::User {
            content: "Use swarm mailbox context.".into(),
            attachments: Vec::new(),
        }];
        let tools = builtin_tool_descriptors()
            .into_iter()
            .filter(|tool| tool.name == AUTONOMOUS_TOOL_AGENT_COORDINATION)
            .collect::<Vec<_>>();
        let package = assemble_provider_context_package(
            ProviderContextPackageInput {
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
                tools: &tools,
                tool_exposure_plan: None,
                messages: &messages,
                owned_process_summary: None,
                provider_preflight: None,
            },
            Vec::new(),
        )
        .expect("assemble provider package");

        assert!(package.system_prompt.contains("Temporary swarm mailbox:"));
        assert!(package
            .compilation
            .fragments
            .iter()
            .any(|fragment| fragment.id == "xero.active_coordination"));
        assert_eq!(
            package.manifest.manifest["coordination"]["presenceCount"],
            0
        );
        assert_eq!(package.manifest.manifest["coordination"]["mailboxCount"], 1);
        assert_eq!(
            package.manifest.manifest["coordination"]["promptFragmentId"],
            "xero.active_coordination"
        );
        let included = package.manifest.manifest["contributors"]["included"]
            .as_array()
            .expect("included contributors");
        assert!(included.iter().any(|contributor| {
            contributor["contributorId"] == format!("agent_mailbox_item:{}", mailbox.item_id)
        }));
    }
}
