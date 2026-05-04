use std::collections::BTreeSet;

use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use super::{
    AutonomousAgentRunContext, AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT,
};
use crate::{
    auth::now_timestamp,
    commands::{redact_session_context_text, CommandError, CommandResult, RuntimeAgentIdDto},
    db::project_store,
};

const DEFAULT_CONTEXT_LIMIT: u32 = 6;
const MAX_CONTEXT_LIMIT: u32 = 10;
const MAX_CONTEXT_TEXT_CHARS: usize = 4_000;
const PROJECT_CONTEXT_RECORD_SCHEMA: &str = "xero.project_context_tool.record.v1";
const PROJECT_CONTEXT_UPDATE_SCHEMA: &str = "xero.project_context_tool.record_update.v1";
const PROJECT_CONTEXT_RECORD_CANDIDATE_SCHEMA: &str =
    "xero.project_context_tool.record_candidate.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProjectContextAction {
    SearchProjectRecords,
    SearchApprovedMemory,
    GetProjectRecord,
    GetMemory,
    ListRecentHandoffs,
    ListActiveDecisionsConstraints,
    ListOpenQuestionsBlockers,
    ExplainCurrentContextPackage,
    RecordContext,
    UpdateContext,
    ProposeRecordCandidate,
    RefreshFreshness,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProjectContextRecordKind {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProjectContextRecordImportance {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProjectContextMemoryKind {
    ProjectFact,
    UserPreference,
    Decision,
    SessionSummary,
    Troubleshooting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProjectContextRequest {
    pub action: AutonomousProjectContextAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub record_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub record_kinds: Vec<AutonomousProjectContextRecordKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory_kinds: Vec<AutonomousProjectContextMemoryKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_importance: Option<AutonomousProjectContextRecordImportance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_kind: Option<AutonomousProjectContextRecordKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub importance: Option<AutonomousProjectContextRecordImportance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_item_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_json: Option<JsonValue>,
}

impl AutonomousProjectContextRequest {
    pub fn new(action: AutonomousProjectContextAction) -> Self {
        Self {
            action,
            query: None,
            record_id: None,
            memory_id: None,
            record_ids: Vec::new(),
            memory_ids: Vec::new(),
            record_kinds: Vec::new(),
            memory_kinds: Vec::new(),
            tags: Vec::new(),
            related_paths: Vec::new(),
            created_after: None,
            min_importance: None,
            limit: None,
            title: None,
            summary: None,
            text: None,
            record_kind: None,
            importance: None,
            confidence: None,
            source_item_ids: Vec::new(),
            content_json: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProjectContextOutput {
    pub action: AutonomousProjectContextAction,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_id: Option<String>,
    pub result_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<AutonomousProjectContextResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<AutonomousProjectContextRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<AutonomousProjectContextMemory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_record: Option<AutonomousProjectContextRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProjectContextResult {
    pub source_kind: String,
    pub source_id: String,
    pub rank: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<String>,
    pub snippet: String,
    pub redaction_state: String,
    pub citation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProjectContextRecord {
    pub record_id: String,
    pub source_kind: String,
    pub record_kind: String,
    pub title: String,
    pub summary: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_json: Option<JsonValue>,
    pub importance: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_item_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_paths: Vec<String>,
    pub runtime_agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    pub run_id: String,
    pub redaction_state: String,
    pub visibility: String,
    pub trust: JsonValue,
    pub citation: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProjectContextMemory {
    pub memory_id: String,
    pub scope: String,
    pub memory_kind: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_item_ids: Vec<String>,
    pub redaction_state: String,
    pub trust: JsonValue,
    pub citation: String,
    pub created_at: String,
    pub updated_at: String,
}

impl AutonomousToolRuntime {
    pub fn project_context(
        &self,
        request: AutonomousProjectContextRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let output = self.execute_project_context(request)?;
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_PROJECT_CONTEXT.into(),
            summary: output.message.clone(),
            command_result: None,
            output: AutonomousToolOutput::ProjectContext(output),
        })
    }

    fn execute_project_context(
        &self,
        request: AutonomousProjectContextRequest,
    ) -> CommandResult<AutonomousProjectContextOutput> {
        let run_context = self.require_agent_run_context()?.clone();
        let runtime_agent_id = self.active_runtime_agent_id();
        match request.action {
            AutonomousProjectContextAction::SearchProjectRecords => self.search_context(
                request,
                &run_context,
                runtime_agent_id,
                project_store::AgentRetrievalSearchScope::ProjectRecords,
                "project context",
            ),
            AutonomousProjectContextAction::SearchApprovedMemory => self.search_context(
                request,
                &run_context,
                runtime_agent_id,
                project_store::AgentRetrievalSearchScope::ApprovedMemory,
                "approved memory",
            ),
            AutonomousProjectContextAction::ListRecentHandoffs => self.search_context(
                request,
                &run_context,
                runtime_agent_id,
                project_store::AgentRetrievalSearchScope::Handoffs,
                "recent same-type agent handoffs",
            ),
            AutonomousProjectContextAction::ListActiveDecisionsConstraints => {
                let mut request = request;
                if request.record_kinds.is_empty() {
                    request.record_kinds = vec![
                        AutonomousProjectContextRecordKind::Decision,
                        AutonomousProjectContextRecordKind::Constraint,
                    ];
                }
                self.search_context(
                    request,
                    &run_context,
                    runtime_agent_id,
                    project_store::AgentRetrievalSearchScope::ProjectRecords,
                    "active decisions constraints project context",
                )
            }
            AutonomousProjectContextAction::ListOpenQuestionsBlockers => {
                let mut request = request;
                if request.record_kinds.is_empty() {
                    request.record_kinds = vec![
                        AutonomousProjectContextRecordKind::Question,
                        AutonomousProjectContextRecordKind::Diagnostic,
                    ];
                }
                self.search_context(
                    request,
                    &run_context,
                    runtime_agent_id,
                    project_store::AgentRetrievalSearchScope::ProjectRecords,
                    "open questions blockers unresolved risks",
                )
            }
            AutonomousProjectContextAction::GetProjectRecord => {
                self.get_project_record(request, &run_context, runtime_agent_id)
            }
            AutonomousProjectContextAction::GetMemory => {
                self.get_memory(request, &run_context, runtime_agent_id)
            }
            AutonomousProjectContextAction::ExplainCurrentContextPackage => {
                self.explain_current_context_package(request, &run_context, runtime_agent_id)
            }
            AutonomousProjectContextAction::RecordContext => {
                self.record_context(request, &run_context, runtime_agent_id)
            }
            AutonomousProjectContextAction::UpdateContext => {
                self.update_context(request, &run_context, runtime_agent_id)
            }
            AutonomousProjectContextAction::ProposeRecordCandidate => {
                self.propose_record_candidate(request, &run_context, runtime_agent_id)
            }
            AutonomousProjectContextAction::RefreshFreshness => {
                self.refresh_freshness(request, &run_context)
            }
        }
    }

    fn require_agent_run_context(&self) -> CommandResult<&AutonomousAgentRunContext> {
        self.agent_run_context.as_ref().ok_or_else(|| {
            CommandError::system_fault(
                "project_context_run_context_unavailable",
                "Xero could not use project_context because the active agent run context was not attached to the tool runtime.",
            )
        })
    }

    fn search_context(
        &self,
        request: AutonomousProjectContextRequest,
        run_context: &AutonomousAgentRunContext,
        runtime_agent_id: RuntimeAgentIdDto,
        search_scope: project_store::AgentRetrievalSearchScope,
        default_query: &str,
    ) -> CommandResult<AutonomousProjectContextOutput> {
        let query_text =
            optional_trimmed(request.query.as_deref()).unwrap_or_else(|| default_query.to_string());
        let limit = normalize_limit(request.limit);
        let run_snapshot = project_store::load_agent_run(
            &self.repo_root,
            &run_context.project_id,
            &run_context.run_id,
        )?;
        let response = project_store::search_agent_context(
            &self.repo_root,
            project_store::AgentContextRetrievalRequest {
                query_id: generated_project_context_query_id(&run_context.run_id),
                project_id: run_context.project_id.clone(),
                agent_session_id: Some(run_context.agent_session_id.clone()),
                run_id: Some(run_context.run_id.clone()),
                runtime_agent_id,
                agent_definition_id: run_snapshot.run.agent_definition_id,
                agent_definition_version: run_snapshot.run.agent_definition_version,
                query_text: query_text.clone(),
                search_scope,
                filters: retrieval_filters_from_request(&request),
                limit_count: limit,
                allow_keyword_fallback: true,
                created_at: now_timestamp(),
            },
        )?;
        let query_id = response.query.query_id.clone();
        let results = context_results_from_retrieval(&query_id, &response.results);
        Ok(AutonomousProjectContextOutput {
            action: request.action,
            message: format!(
                "project_context returned {} source-cited result(s) for `{}`.",
                results.len(),
                query_text
            ),
            query_id: Some(query_id),
            result_count: results.len(),
            results,
            record: None,
            memory: None,
            manifest: None,
            candidate_record: None,
        })
    }

    fn get_project_record(
        &self,
        request: AutonomousProjectContextRequest,
        run_context: &AutonomousAgentRunContext,
        runtime_agent_id: RuntimeAgentIdDto,
    ) -> CommandResult<AutonomousProjectContextOutput> {
        let record_id = required_text(request.record_id.as_deref(), "recordId")?;
        project_store::refresh_all_project_record_freshness(
            &self.repo_root,
            &run_context.project_id,
            &now_timestamp(),
        )?;
        let record = project_store::list_project_records(&self.repo_root, &run_context.project_id)?
            .into_iter()
            .find(|record| record.record_id == record_id)
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "project_context_record_not_found",
                    format!("Project record `{record_id}` was not found."),
                )
            })?;

        if record.redaction_state == project_store::ProjectRecordRedactionState::Blocked {
            return Err(CommandError::user_fixable(
                "project_context_record_blocked",
                format!("Project record `{record_id}` is blocked by redaction policy."),
            ));
        }
        if record.visibility == project_store::ProjectRecordVisibility::MemoryCandidate {
            return Err(CommandError::user_fixable(
                "project_context_record_candidate_unreviewed",
                format!(
                    "Project record `{record_id}` is a review-only candidate and is not retrievable as durable context."
                ),
            ));
        }

        let output_record = context_record_from_record(&record);
        let query_id = log_manual_retrieval(
            &self.repo_root,
            run_context,
            runtime_agent_id,
            format!("get project record {record_id}"),
            project_store::AgentRetrievalSearchScope::ProjectRecords,
            vec![ManualRetrievalSource::from_project_record(&record)],
        )?;
        Ok(AutonomousProjectContextOutput {
            action: request.action,
            message: format!("project_context read project record `{record_id}`."),
            query_id: Some(query_id),
            result_count: 1,
            results: Vec::new(),
            record: Some(output_record),
            memory: None,
            manifest: None,
            candidate_record: None,
        })
    }

    fn get_memory(
        &self,
        request: AutonomousProjectContextRequest,
        run_context: &AutonomousAgentRunContext,
        runtime_agent_id: RuntimeAgentIdDto,
    ) -> CommandResult<AutonomousProjectContextOutput> {
        let memory_id = required_text(request.memory_id.as_deref(), "memoryId")?;
        project_store::refresh_all_agent_memory_freshness(
            &self.repo_root,
            &run_context.project_id,
            &now_timestamp(),
        )?;
        let memory =
            project_store::get_agent_memory(&self.repo_root, &run_context.project_id, &memory_id)?;
        if memory.review_state != project_store::AgentMemoryReviewState::Approved || !memory.enabled
        {
            return Err(CommandError::user_fixable(
                "project_context_memory_not_approved",
                format!("Memory `{memory_id}` is not approved and enabled."),
            ));
        }

        let output_memory = context_memory_from_memory(&memory);
        let query_id = log_manual_retrieval(
            &self.repo_root,
            run_context,
            runtime_agent_id,
            format!("get approved memory {memory_id}"),
            project_store::AgentRetrievalSearchScope::ApprovedMemory,
            vec![ManualRetrievalSource::from_memory(&memory)],
        )?;
        Ok(AutonomousProjectContextOutput {
            action: request.action,
            message: format!("project_context read approved memory `{memory_id}`."),
            query_id: Some(query_id),
            result_count: 1,
            results: Vec::new(),
            record: None,
            memory: Some(output_memory),
            manifest: None,
            candidate_record: None,
        })
    }

    fn explain_current_context_package(
        &self,
        request: AutonomousProjectContextRequest,
        run_context: &AutonomousAgentRunContext,
        runtime_agent_id: RuntimeAgentIdDto,
    ) -> CommandResult<AutonomousProjectContextOutput> {
        let manifest = project_store::list_agent_context_manifests_for_run(
            &self.repo_root,
            &run_context.project_id,
            &run_context.run_id,
        )?
        .into_iter()
        .last()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "project_context_manifest_not_found",
                format!(
                    "No context manifest has been recorded yet for run `{}`.",
                    run_context.run_id
                ),
            )
        })?;
        let redacted_manifest = redact_json_value(&manifest.manifest);
        let query_id = log_manual_retrieval(
            &self.repo_root,
            run_context,
            runtime_agent_id,
            format!("explain current context package {}", manifest.manifest_id),
            project_store::AgentRetrievalSearchScope::ProjectRecords,
            vec![ManualRetrievalSource {
                source_kind: project_store::AgentRetrievalResultSourceKind::ContextManifest,
                source_id: manifest.manifest_id.clone(),
                snippet: format!(
                    "Context manifest `{}` used policy `{}` and estimated {} token(s).",
                    manifest.manifest_id, manifest.policy_reason_code, manifest.estimated_tokens
                ),
                redaction_state: manifest.redaction_state.clone(),
                metadata: Some(json!({
                    "manifestId": manifest.manifest_id,
                    "contextHash": manifest.context_hash,
                    "policyReasonCode": manifest.policy_reason_code,
                    "pressure": context_pressure_label(&manifest.pressure),
                    "citation": format!("agent_context_manifests:{}", manifest.id)
                })),
            }],
        )?;
        Ok(AutonomousProjectContextOutput {
            action: request.action,
            message: "project_context returned the latest source-cited context manifest.".into(),
            query_id: Some(query_id),
            result_count: 1,
            results: Vec::new(),
            record: None,
            memory: None,
            manifest: Some(redacted_manifest),
            candidate_record: None,
        })
    }

    fn propose_record_candidate(
        &self,
        request: AutonomousProjectContextRequest,
        run_context: &AutonomousAgentRunContext,
        runtime_agent_id: RuntimeAgentIdDto,
    ) -> CommandResult<AutonomousProjectContextOutput> {
        ensure_context_write_allowed(runtime_agent_id)?;
        let action = request.action;
        let record = self.insert_context_record(
            request,
            run_context,
            runtime_agent_id,
            project_store::ProjectRecordVisibility::MemoryCandidate,
            PROJECT_CONTEXT_RECORD_CANDIDATE_SCHEMA,
        )?;
        Ok(AutonomousProjectContextOutput {
            action,
            message: format!(
                "project_context proposed review-only candidate record `{}`.",
                record.record_id
            ),
            query_id: None,
            result_count: 1,
            results: Vec::new(),
            record: None,
            memory: None,
            manifest: None,
            candidate_record: Some(context_record_from_record(&record)),
        })
    }

    fn record_context(
        &self,
        request: AutonomousProjectContextRequest,
        run_context: &AutonomousAgentRunContext,
        runtime_agent_id: RuntimeAgentIdDto,
    ) -> CommandResult<AutonomousProjectContextOutput> {
        ensure_context_write_allowed(runtime_agent_id)?;
        let action = request.action;
        let record = self.insert_context_record(
            request,
            run_context,
            runtime_agent_id,
            project_store::ProjectRecordVisibility::Retrieval,
            PROJECT_CONTEXT_RECORD_SCHEMA,
        )?;
        Ok(AutonomousProjectContextOutput {
            action,
            message: format!(
                "project_context recorded durable context `{}`.",
                record.record_id
            ),
            query_id: None,
            result_count: 1,
            results: Vec::new(),
            record: Some(context_record_from_record(&record)),
            memory: None,
            manifest: None,
            candidate_record: None,
        })
    }

    fn update_context(
        &self,
        mut request: AutonomousProjectContextRequest,
        run_context: &AutonomousAgentRunContext,
        runtime_agent_id: RuntimeAgentIdDto,
    ) -> CommandResult<AutonomousProjectContextOutput> {
        ensure_context_write_allowed(runtime_agent_id)?;
        let action = request.action;
        let superseded_record_id = optional_trimmed(request.record_id.as_deref());
        let superseded_record = superseded_record_id
            .as_deref()
            .map(|record_id| self.load_update_target_record(&run_context.project_id, record_id))
            .transpose()?;
        if let Some(record) = superseded_record.as_ref() {
            if request
                .title
                .as_ref()
                .and_then(|value| optional_trimmed(Some(value.as_str())))
                .is_none()
            {
                request.title = Some(record.title.clone());
            }
            if request
                .summary
                .as_ref()
                .and_then(|value| optional_trimmed(Some(value.as_str())))
                .is_none()
            {
                request.summary =
                    Some(format!("Supersedes project record `{}`.", record.record_id));
            }
            if request.record_kind.is_none() {
                request.record_kind = Some(AutonomousProjectContextRecordKind::from_project_store(
                    &record.record_kind,
                ));
            }
            if request.importance.is_none() {
                request.importance = Some(
                    AutonomousProjectContextRecordImportance::from_project_store(
                        &record.importance,
                    ),
                );
            }
            if request.confidence.is_none() {
                request.confidence = record
                    .confidence
                    .map(|confidence| (confidence * 100.0).round().clamp(0.0, 100.0) as u8);
            }
            if request.tags.is_empty() {
                request.tags = record.tags.clone();
            }
            if request.related_paths.is_empty() {
                request.related_paths = record.related_paths.clone();
            }
            request.content_json = Some(update_record_content_json(
                request.content_json.take(),
                "supersedesRecordId",
                &record.record_id,
            ));
        } else if let Some(memory_id) = optional_trimmed(request.memory_id.as_deref()) {
            let memory = self.load_update_target_memory(&run_context.project_id, &memory_id)?;
            if request
                .title
                .as_ref()
                .and_then(|value| optional_trimmed(Some(value.as_str())))
                .is_none()
            {
                request.title = Some(format!("Correction for memory `{memory_id}`"));
            }
            if request
                .summary
                .as_ref()
                .and_then(|value| optional_trimmed(Some(value.as_str())))
                .is_none()
            {
                request.summary = Some(format!("Supersedes approved memory `{memory_id}`."));
            }
            if request.record_kind.is_none() {
                request.record_kind = Some(record_kind_for_memory(&memory.kind));
            }
            if request.importance.is_none() {
                request.importance = Some(AutonomousProjectContextRecordImportance::Normal);
            }
            if request.confidence.is_none() {
                request.confidence = memory.confidence;
            }
            if request.related_paths.is_empty() {
                request.related_paths =
                    project_store::source_fingerprint_paths(&memory.source_fingerprints_json)
                        .unwrap_or_default();
            }
            request
                .source_item_ids
                .push(format!("agent_memories:{}", memory.memory_id));
            request.content_json = Some(update_record_content_json(
                request.content_json.take(),
                "supersedesMemoryId",
                &memory.memory_id,
            ));
        }

        let mut record = self.insert_context_record(
            request,
            run_context,
            runtime_agent_id,
            project_store::ProjectRecordVisibility::Retrieval,
            PROJECT_CONTEXT_UPDATE_SCHEMA,
        )?;
        if let Some(record_id) = superseded_record_id {
            project_store::mark_project_record_superseded_by(
                &self.repo_root,
                &run_context.project_id,
                &record_id,
                &record.record_id,
                &now_timestamp(),
            )?;
            if let Some(updated_record) =
                project_store::list_project_records(&self.repo_root, &run_context.project_id)?
                    .into_iter()
                    .find(|candidate| candidate.record_id == record.record_id)
            {
                record = updated_record;
            }
        }
        Ok(AutonomousProjectContextOutput {
            action,
            message: format!(
                "project_context updated durable context `{}`.",
                record.record_id
            ),
            query_id: None,
            result_count: 1,
            results: Vec::new(),
            record: Some(context_record_from_record(&record)),
            memory: None,
            manifest: None,
            candidate_record: None,
        })
    }

    fn insert_context_record(
        &self,
        request: AutonomousProjectContextRequest,
        run_context: &AutonomousAgentRunContext,
        runtime_agent_id: RuntimeAgentIdDto,
        visibility: project_store::ProjectRecordVisibility,
        schema_name: &str,
    ) -> CommandResult<project_store::ProjectRecordRecord> {
        let title = required_text(request.title.as_deref(), "title")?;
        let summary = required_text(request.summary.as_deref(), "summary")?;
        let text = required_text(request.text.as_deref(), "text")?;
        if request
            .confidence
            .is_some_and(|confidence| confidence > 100)
        {
            return Err(CommandError::invalid_request("confidence"));
        }

        let (title, title_redacted) = redact_session_context_text(&title);
        let (summary, summary_redacted) = redact_session_context_text(&summary);
        let (text, text_redacted) = redact_session_context_text(&text);
        let content_json = request.content_json.as_ref().map(redact_json_value);
        let content_redacted = content_json
            .as_ref()
            .is_some_and(json_value_contains_redaction_marker);
        let redaction_state = if title_redacted.redacted
            || summary_redacted.redacted
            || text_redacted.redacted
            || content_redacted
        {
            project_store::ProjectRecordRedactionState::Redacted
        } else {
            project_store::ProjectRecordRedactionState::Clean
        };
        let tags = context_record_tags(&request.tags, runtime_agent_id, &visibility);
        let source_item_ids = candidate_source_item_ids(&request.source_item_ids, run_context);
        let run_snapshot = project_store::load_agent_run(
            &self.repo_root,
            &run_context.project_id,
            &run_context.run_id,
        )?;
        project_store::insert_project_record(
            &self.repo_root,
            &project_store::NewProjectRecordRecord {
                record_id: project_store::generate_project_record_id(),
                project_id: run_context.project_id.clone(),
                record_kind: request
                    .record_kind
                    .unwrap_or(AutonomousProjectContextRecordKind::ContextNote)
                    .to_project_store(),
                runtime_agent_id,
                agent_definition_id: run_snapshot.run.agent_definition_id,
                agent_definition_version: run_snapshot.run.agent_definition_version,
                agent_session_id: Some(run_context.agent_session_id.clone()),
                run_id: run_context.run_id.clone(),
                workflow_run_id: None,
                workflow_step_id: None,
                title: truncate_chars(title.trim(), 240),
                summary: truncate_chars(summary.trim(), 500),
                text: truncate_chars(text.trim(), MAX_CONTEXT_TEXT_CHARS),
                content_json,
                schema_name: Some(schema_name.into()),
                schema_version: 1,
                importance: request
                    .importance
                    .unwrap_or(AutonomousProjectContextRecordImportance::Normal)
                    .to_project_store(),
                confidence: request
                    .confidence
                    .map(|confidence| f64::from(confidence) / 100.0),
                tags,
                source_item_ids,
                related_paths: normalized_strings(&request.related_paths),
                produced_artifact_refs: Vec::new(),
                redaction_state,
                visibility,
                created_at: now_timestamp(),
            },
        )
    }

    fn load_update_target_record(
        &self,
        project_id: &str,
        record_id: &str,
    ) -> CommandResult<project_store::ProjectRecordRecord> {
        project_store::refresh_project_record_freshness_for_ids(
            &self.repo_root,
            project_id,
            &[record_id.to_string()],
            &now_timestamp(),
        )?;
        let record = project_store::list_project_records(&self.repo_root, project_id)?
            .into_iter()
            .find(|record| record.record_id == record_id)
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "project_context_update_record_not_found",
                    format!("Project record `{record_id}` was not found."),
                )
            })?;
        if record.redaction_state == project_store::ProjectRecordRedactionState::Blocked {
            return Err(CommandError::user_fixable(
                "project_context_update_record_blocked",
                format!("Project record `{record_id}` is blocked by redaction policy."),
            ));
        }
        if record.visibility == project_store::ProjectRecordVisibility::MemoryCandidate {
            return Err(CommandError::user_fixable(
                "project_context_update_record_candidate_unreviewed",
                format!(
                    "Project record `{record_id}` is a review-only candidate and cannot be superseded automatically."
                ),
            ));
        }
        Ok(record)
    }

    fn load_update_target_memory(
        &self,
        project_id: &str,
        memory_id: &str,
    ) -> CommandResult<project_store::AgentMemoryRecord> {
        project_store::refresh_agent_memory_freshness_for_ids(
            &self.repo_root,
            project_id,
            &[memory_id.to_string()],
            &now_timestamp(),
        )?;
        let memory = project_store::get_agent_memory(&self.repo_root, project_id, memory_id)?;
        if memory.review_state != project_store::AgentMemoryReviewState::Approved || !memory.enabled
        {
            return Err(CommandError::user_fixable(
                "project_context_update_memory_not_approved",
                format!("Memory `{memory_id}` is not approved and enabled."),
            ));
        }
        Ok(memory)
    }

    fn refresh_freshness(
        &self,
        request: AutonomousProjectContextRequest,
        run_context: &AutonomousAgentRunContext,
    ) -> CommandResult<AutonomousProjectContextOutput> {
        let checked_at = now_timestamp();
        let related_paths = normalized_strings(&request.related_paths);
        let record_ids = selected_ids(request.record_id.as_deref(), &request.record_ids);
        let memory_ids = selected_ids(request.memory_id.as_deref(), &request.memory_ids);
        let summary = if !record_ids.is_empty() || !memory_ids.is_empty() {
            let mut summary = project_store::refresh_project_record_freshness_for_ids(
                &self.repo_root,
                &run_context.project_id,
                &record_ids,
                &checked_at,
            )?;
            summary.merge(project_store::refresh_agent_memory_freshness_for_ids(
                &self.repo_root,
                &run_context.project_id,
                &memory_ids,
                &checked_at,
            )?);
            summary
        } else if related_paths.is_empty() {
            let mut summary = project_store::refresh_all_project_record_freshness(
                &self.repo_root,
                &run_context.project_id,
                &checked_at,
            )?;
            summary.merge(project_store::refresh_all_agent_memory_freshness(
                &self.repo_root,
                &run_context.project_id,
                &checked_at,
            )?);
            summary
        } else {
            let mut summary = project_store::refresh_project_record_freshness_for_paths(
                &self.repo_root,
                &run_context.project_id,
                &related_paths,
                &checked_at,
            )?;
            summary.merge(project_store::refresh_agent_memory_freshness_for_paths(
                &self.repo_root,
                &run_context.project_id,
                &related_paths,
                &checked_at,
            )?);
            summary
        };
        let result_count = summary.inspected_count;
        let manifest = summary.as_json();
        Ok(AutonomousProjectContextOutput {
            action: request.action,
            message: format!(
                "project_context refreshed freshness for {result_count} durable context row(s)."
            ),
            query_id: None,
            result_count,
            results: Vec::new(),
            record: None,
            memory: None,
            manifest: Some(manifest),
            candidate_record: None,
        })
    }
}

impl AutonomousProjectContextRecordKind {
    fn to_project_store(self) -> project_store::ProjectRecordKind {
        match self {
            Self::AgentHandoff => project_store::ProjectRecordKind::AgentHandoff,
            Self::ProjectFact => project_store::ProjectRecordKind::ProjectFact,
            Self::Decision => project_store::ProjectRecordKind::Decision,
            Self::Constraint => project_store::ProjectRecordKind::Constraint,
            Self::Plan => project_store::ProjectRecordKind::Plan,
            Self::Finding => project_store::ProjectRecordKind::Finding,
            Self::Verification => project_store::ProjectRecordKind::Verification,
            Self::Question => project_store::ProjectRecordKind::Question,
            Self::Artifact => project_store::ProjectRecordKind::Artifact,
            Self::ContextNote => project_store::ProjectRecordKind::ContextNote,
            Self::Diagnostic => project_store::ProjectRecordKind::Diagnostic,
        }
    }

    fn from_project_store(kind: &project_store::ProjectRecordKind) -> Self {
        match kind {
            project_store::ProjectRecordKind::AgentHandoff => Self::AgentHandoff,
            project_store::ProjectRecordKind::ProjectFact => Self::ProjectFact,
            project_store::ProjectRecordKind::Decision => Self::Decision,
            project_store::ProjectRecordKind::Constraint => Self::Constraint,
            project_store::ProjectRecordKind::Plan => Self::Plan,
            project_store::ProjectRecordKind::Finding => Self::Finding,
            project_store::ProjectRecordKind::Verification => Self::Verification,
            project_store::ProjectRecordKind::Question => Self::Question,
            project_store::ProjectRecordKind::Artifact => Self::Artifact,
            project_store::ProjectRecordKind::ContextNote => Self::ContextNote,
            project_store::ProjectRecordKind::Diagnostic => Self::Diagnostic,
        }
    }
}

impl AutonomousProjectContextRecordImportance {
    fn to_project_store(self) -> project_store::ProjectRecordImportance {
        match self {
            Self::Low => project_store::ProjectRecordImportance::Low,
            Self::Normal => project_store::ProjectRecordImportance::Normal,
            Self::High => project_store::ProjectRecordImportance::High,
            Self::Critical => project_store::ProjectRecordImportance::Critical,
        }
    }

    fn from_project_store(importance: &project_store::ProjectRecordImportance) -> Self {
        match importance {
            project_store::ProjectRecordImportance::Low => Self::Low,
            project_store::ProjectRecordImportance::Normal => Self::Normal,
            project_store::ProjectRecordImportance::High => Self::High,
            project_store::ProjectRecordImportance::Critical => Self::Critical,
        }
    }
}

impl AutonomousProjectContextMemoryKind {
    fn to_project_store(self) -> project_store::AgentMemoryKind {
        match self {
            Self::ProjectFact => project_store::AgentMemoryKind::ProjectFact,
            Self::UserPreference => project_store::AgentMemoryKind::UserPreference,
            Self::Decision => project_store::AgentMemoryKind::Decision,
            Self::SessionSummary => project_store::AgentMemoryKind::SessionSummary,
            Self::Troubleshooting => project_store::AgentMemoryKind::Troubleshooting,
        }
    }
}

fn retrieval_filters_from_request(
    request: &AutonomousProjectContextRequest,
) -> project_store::AgentContextRetrievalFilters {
    project_store::AgentContextRetrievalFilters {
        record_kinds: request
            .record_kinds
            .iter()
            .map(|kind| kind.to_project_store())
            .collect(),
        memory_kinds: request
            .memory_kinds
            .iter()
            .map(|kind| kind.to_project_store())
            .collect(),
        tags: normalized_strings(&request.tags),
        related_paths: normalized_strings(&request.related_paths),
        runtime_agent_id: None,
        agent_session_id: None,
        created_after: optional_trimmed(request.created_after.as_deref()),
        min_importance: request
            .min_importance
            .map(|importance| importance.to_project_store()),
    }
}

fn context_results_from_retrieval(
    query_id: &str,
    results: &[project_store::AgentContextRetrievalResult],
) -> Vec<AutonomousProjectContextResult> {
    results
        .iter()
        .filter(|result| {
            result.redaction_state != project_store::AgentContextRedactionState::Blocked
        })
        .map(|result| AutonomousProjectContextResult {
            source_kind: retrieval_source_kind_label(&result.source_kind).into(),
            source_id: result.source_id.clone(),
            rank: result.rank,
            score: result.score.map(|score| format!("{score:.4}")),
            snippet: sanitize_text_for_output(&result.snippet),
            redaction_state: context_redaction_state_label(&result.redaction_state).into(),
            citation: format!(
                "agent_retrieval_results:{query_id}:{}:{}",
                result.rank, result.source_id
            ),
            metadata: Some(redact_json_value(&result.metadata)),
        })
        .collect()
}

fn context_record_from_record(
    record: &project_store::ProjectRecordRecord,
) -> AutonomousProjectContextRecord {
    let (title, title_redaction) = redact_session_context_text(&record.title);
    let (summary, summary_redaction) = redact_session_context_text(&record.summary);
    let (text, text_redaction) = redact_session_context_text(&record.text);
    let redaction_state = strongest_record_redaction_state(
        &record.redaction_state,
        title_redaction.redacted || summary_redaction.redacted || text_redaction.redacted,
    );
    AutonomousProjectContextRecord {
        record_id: record.record_id.clone(),
        source_kind: if record.record_kind == project_store::ProjectRecordKind::AgentHandoff {
            "handoff".into()
        } else {
            "project_record".into()
        },
        record_kind: project_record_kind_label(&record.record_kind).into(),
        title: truncate_chars(title.trim(), 240),
        summary: truncate_chars(summary.trim(), 500),
        text: truncate_chars(text.trim(), MAX_CONTEXT_TEXT_CHARS),
        content_json: record.content_json.as_ref().map(redact_json_value),
        importance: project_record_importance_label(&record.importance).into(),
        confidence: record
            .confidence
            .map(|confidence| format!("{confidence:.2}")),
        tags: record.tags.clone(),
        source_item_ids: record.source_item_ids.clone(),
        related_paths: record.related_paths.clone(),
        runtime_agent_id: record.runtime_agent_id.as_str().into(),
        agent_session_id: record.agent_session_id.clone(),
        run_id: record.run_id.clone(),
        redaction_state: project_record_redaction_state_label(&redaction_state).into(),
        visibility: project_record_visibility_label(&record.visibility).into(),
        trust: project_record_trust_envelope(record),
        citation: format!("project_records:{}", record.record_id),
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
    }
}

fn context_memory_from_memory(
    memory: &project_store::AgentMemoryRecord,
) -> AutonomousProjectContextMemory {
    let (text, redaction) = redact_session_context_text(&memory.text);
    AutonomousProjectContextMemory {
        memory_id: memory.memory_id.clone(),
        scope: agent_memory_scope_label(&memory.scope).into(),
        memory_kind: agent_memory_kind_label(&memory.kind).into(),
        text: truncate_chars(text.trim(), MAX_CONTEXT_TEXT_CHARS),
        confidence: memory.confidence,
        agent_session_id: memory.agent_session_id.clone(),
        source_run_id: memory.source_run_id.clone(),
        source_item_ids: memory.source_item_ids.clone(),
        redaction_state: if redaction.redacted {
            "redacted"
        } else {
            "clean"
        }
        .into(),
        trust: memory_trust_envelope(memory),
        citation: format!("agent_memories:{}", memory.memory_id),
        created_at: memory.created_at.clone(),
        updated_at: memory.updated_at.clone(),
    }
}

fn project_record_freshness_metadata(record: &project_store::ProjectRecordRecord) -> JsonValue {
    project_store::freshness_metadata_json(project_store::FreshnessMetadata {
        freshness_state: &record.freshness_state,
        freshness_checked_at: record.freshness_checked_at.as_deref(),
        stale_reason: record.stale_reason.as_deref(),
        source_fingerprints_json: &record.source_fingerprints_json,
        supersedes_id: record.supersedes_id.as_deref(),
        superseded_by_id: record.superseded_by_id.as_deref(),
        invalidated_at: record.invalidated_at.as_deref(),
        fact_key: record.fact_key.as_deref(),
    })
    .unwrap_or_else(|error| {
        json!({
            "state": record.freshness_state.clone(),
            "checkedAt": record.freshness_checked_at.clone(),
            "staleReason": record.stale_reason.clone(),
            "sourceFingerprints": [],
            "supersedesId": record.supersedes_id.clone(),
            "supersededById": record.superseded_by_id.clone(),
            "invalidatedAt": record.invalidated_at.clone(),
            "factKey": record.fact_key.clone(),
            "diagnostic": {
                "code": error.code,
                "message": error.message,
            },
        })
    })
}

fn memory_freshness_metadata(memory: &project_store::AgentMemoryRecord) -> JsonValue {
    project_store::freshness_metadata_json(project_store::FreshnessMetadata {
        freshness_state: &memory.freshness_state,
        freshness_checked_at: memory.freshness_checked_at.as_deref(),
        stale_reason: memory.stale_reason.as_deref(),
        source_fingerprints_json: &memory.source_fingerprints_json,
        supersedes_id: memory.supersedes_id.as_deref(),
        superseded_by_id: memory.superseded_by_id.as_deref(),
        invalidated_at: memory.invalidated_at.as_deref(),
        fact_key: memory.fact_key.as_deref(),
    })
    .unwrap_or_else(|error| {
        json!({
            "state": memory.freshness_state.clone(),
            "checkedAt": memory.freshness_checked_at.clone(),
            "staleReason": memory.stale_reason.clone(),
            "sourceFingerprints": [],
            "supersedesId": memory.supersedes_id.clone(),
            "supersededById": memory.superseded_by_id.clone(),
            "invalidatedAt": memory.invalidated_at.clone(),
            "factKey": memory.fact_key.clone(),
            "diagnostic": {
                "code": error.code,
                "message": error.message,
            },
        })
    })
}

fn project_record_trust_envelope(record: &project_store::ProjectRecordRecord) -> JsonValue {
    let freshness = project_record_freshness_metadata(record);
    json!({
        "freshnessState": record.freshness_state.clone(),
        "staleReason": record.stale_reason.clone(),
        "checkedAt": record.freshness_checked_at.clone(),
        "sourceFingerprints": freshness.get("sourceFingerprints").cloned().unwrap_or(JsonValue::Array(Vec::new())),
        "supersedesId": record.supersedes_id.clone(),
        "supersededById": record.superseded_by_id.clone(),
        "invalidatedAt": record.invalidated_at.clone(),
        "factKey": record.fact_key.clone(),
        "confidence": record.confidence,
        "sourceRunId": record.run_id.clone(),
        "sourceItemIds": record.source_item_ids.clone(),
        "relatedPaths": record.related_paths.clone(),
    })
}

fn memory_trust_envelope(memory: &project_store::AgentMemoryRecord) -> JsonValue {
    let freshness = memory_freshness_metadata(memory);
    let related_paths = freshness
        .get("sourceFingerprints")
        .and_then(JsonValue::as_array)
        .map(|fingerprints| {
            fingerprints
                .iter()
                .filter_map(|fingerprint| fingerprint.get("path").and_then(JsonValue::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "freshnessState": memory.freshness_state.clone(),
        "staleReason": memory.stale_reason.clone(),
        "checkedAt": memory.freshness_checked_at.clone(),
        "sourceFingerprints": freshness.get("sourceFingerprints").cloned().unwrap_or(JsonValue::Array(Vec::new())),
        "supersedesId": memory.supersedes_id.clone(),
        "supersededById": memory.superseded_by_id.clone(),
        "invalidatedAt": memory.invalidated_at.clone(),
        "factKey": memory.fact_key.clone(),
        "confidence": memory.confidence,
        "sourceRunId": memory.source_run_id.clone(),
        "sourceItemIds": memory.source_item_ids.clone(),
        "relatedPaths": related_paths,
    })
}

struct ManualRetrievalSource {
    source_kind: project_store::AgentRetrievalResultSourceKind,
    source_id: String,
    snippet: String,
    redaction_state: project_store::AgentContextRedactionState,
    metadata: Option<JsonValue>,
}

impl ManualRetrievalSource {
    fn from_project_record(record: &project_store::ProjectRecordRecord) -> Self {
        let (snippet, redaction) = redact_session_context_text(&record.text);
        Self {
            source_kind: if record.record_kind == project_store::ProjectRecordKind::AgentHandoff {
                project_store::AgentRetrievalResultSourceKind::Handoff
            } else {
                project_store::AgentRetrievalResultSourceKind::ProjectRecord
            },
            source_id: record.record_id.clone(),
            snippet: non_empty_snippet(&snippet),
            redaction_state: project_record_to_context_redaction(
                &record.redaction_state,
                redaction.redacted,
            ),
            metadata: Some(json!({
                "title": record.title.clone(),
                "recordKind": project_record_kind_label(&record.record_kind),
                "freshness": project_record_freshness_metadata(record),
                "trust": project_record_trust_envelope(record),
                "confidence": record.confidence,
                "sourceRunId": record.run_id.clone(),
                "sourceItemIds": record.source_item_ids.clone(),
                "relatedPaths": record.related_paths.clone(),
                "citation": format!("project_records:{}", record.record_id)
            })),
        }
    }

    fn from_memory(memory: &project_store::AgentMemoryRecord) -> Self {
        let (snippet, redaction) = redact_session_context_text(&memory.text);
        Self {
            source_kind: project_store::AgentRetrievalResultSourceKind::ApprovedMemory,
            source_id: memory.memory_id.clone(),
            snippet: non_empty_snippet(&snippet),
            redaction_state: if redaction.redacted {
                project_store::AgentContextRedactionState::Redacted
            } else {
                project_store::AgentContextRedactionState::Clean
            },
            metadata: Some(json!({
                "memoryKind": agent_memory_kind_label(&memory.kind),
                "freshness": memory_freshness_metadata(memory),
                "trust": memory_trust_envelope(memory),
                "confidence": memory.confidence,
                "sourceRunId": memory.source_run_id.clone(),
                "sourceItemIds": memory.source_item_ids.clone(),
                "citation": format!("agent_memories:{}", memory.memory_id)
            })),
        }
    }
}

fn log_manual_retrieval(
    repo_root: &std::path::Path,
    run_context: &AutonomousAgentRunContext,
    runtime_agent_id: RuntimeAgentIdDto,
    query_text: String,
    search_scope: project_store::AgentRetrievalSearchScope,
    sources: Vec<ManualRetrievalSource>,
) -> CommandResult<String> {
    let now = now_timestamp();
    let query_id = generated_project_context_query_id(&run_context.run_id);
    let run_snapshot =
        project_store::load_agent_run(repo_root, &run_context.project_id, &run_context.run_id)?;
    project_store::insert_agent_retrieval_query_log(
        repo_root,
        &project_store::NewAgentRetrievalQueryLogRecord {
            query_id: query_id.clone(),
            project_id: run_context.project_id.clone(),
            agent_session_id: Some(run_context.agent_session_id.clone()),
            run_id: Some(run_context.run_id.clone()),
            runtime_agent_id,
            agent_definition_id: run_snapshot.run.agent_definition_id,
            agent_definition_version: run_snapshot.run.agent_definition_version,
            query_text,
            search_scope,
            filters: json!({"tool": AUTONOMOUS_TOOL_PROJECT_CONTEXT}),
            limit_count: sources.len().max(1) as u32,
            status: project_store::AgentRetrievalQueryStatus::Succeeded,
            diagnostic: None,
            created_at: now.clone(),
            completed_at: Some(now.clone()),
        },
    )?;
    for (index, source) in sources.into_iter().enumerate() {
        let rank = (index as u32) + 1;
        project_store::insert_agent_retrieval_result_log(
            repo_root,
            &project_store::NewAgentRetrievalResultLogRecord {
                project_id: run_context.project_id.clone(),
                query_id: query_id.clone(),
                result_id: format!("{query_id}-result-{rank}"),
                source_kind: source.source_kind,
                source_id: source.source_id,
                rank,
                score: None,
                snippet: non_empty_snippet(&sanitize_text_for_output(&source.snippet)),
                redaction_state: source.redaction_state,
                metadata: source.metadata.map(|metadata| redact_json_value(&metadata)),
                created_at: now.clone(),
            },
        )?;
    }
    Ok(query_id)
}

fn generated_project_context_query_id(run_id: &str) -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    let suffix = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("project-context-{run_id}-{suffix}")
}

fn ensure_context_write_allowed(runtime_agent_id: RuntimeAgentIdDto) -> CommandResult<()> {
    if runtime_agent_id == RuntimeAgentIdDto::AgentCreate {
        return Err(CommandError::user_fixable(
            "project_context_write_forbidden_for_agent_create",
            "Agent Create can search and read durable project context, but records agent definitions through the agent_definition tool.",
        ));
    }
    Ok(())
}

fn context_record_tags(
    tags: &[String],
    runtime_agent_id: RuntimeAgentIdDto,
    visibility: &project_store::ProjectRecordVisibility,
) -> Vec<String> {
    let mut values = normalized_strings(tags);
    values.push("project-context-tool".into());
    if *visibility == project_store::ProjectRecordVisibility::MemoryCandidate {
        values.push("candidate".into());
    } else {
        values.push("automatic".into());
    }
    values.push(format!("runtime-agent:{}", runtime_agent_id.as_str()));
    dedupe_strings(values)
}

fn candidate_source_item_ids(
    source_item_ids: &[String],
    run_context: &AutonomousAgentRunContext,
) -> Vec<String> {
    let mut values = normalized_strings(source_item_ids);
    values.push(format!("agent_runs:{}", run_context.run_id));
    dedupe_strings(values)
}

fn selected_ids(single: Option<&str>, many: &[String]) -> Vec<String> {
    let mut values = normalized_strings(many);
    if let Some(value) = optional_trimmed(single) {
        values.push(value);
    }
    dedupe_strings(values)
}

fn update_record_content_json(
    content_json: Option<JsonValue>,
    supersession_key: &str,
    supersession_id: &str,
) -> JsonValue {
    let mut object = JsonMap::new();
    object.insert(
        supersession_key.into(),
        JsonValue::String(supersession_id.into()),
    );
    if let Some(content_json) = content_json {
        object.insert("update".into(), content_json);
    }
    JsonValue::Object(object)
}

fn record_kind_for_memory(
    kind: &project_store::AgentMemoryKind,
) -> AutonomousProjectContextRecordKind {
    match kind {
        project_store::AgentMemoryKind::ProjectFact => {
            AutonomousProjectContextRecordKind::ProjectFact
        }
        project_store::AgentMemoryKind::UserPreference => {
            AutonomousProjectContextRecordKind::ContextNote
        }
        project_store::AgentMemoryKind::Decision => AutonomousProjectContextRecordKind::Decision,
        project_store::AgentMemoryKind::SessionSummary => {
            AutonomousProjectContextRecordKind::AgentHandoff
        }
        project_store::AgentMemoryKind::Troubleshooting => {
            AutonomousProjectContextRecordKind::Finding
        }
    }
}

fn normalize_limit(limit: Option<u32>) -> u32 {
    limit
        .unwrap_or(DEFAULT_CONTEXT_LIMIT)
        .clamp(1, MAX_CONTEXT_LIMIT)
}

fn required_text(value: Option<&str>, field: &'static str) -> CommandResult<String> {
    optional_trimmed(value).ok_or_else(|| CommandError::invalid_request(field))
}

fn optional_trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalized_strings(values: &[String]) -> Vec<String> {
    dedupe_strings(
        values
            .iter()
            .filter_map(|value| optional_trimmed(Some(value)))
            .collect(),
    )
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn redact_json_value(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => value.clone(),
        JsonValue::String(text) => JsonValue::String(sanitize_text_for_output(text)),
        JsonValue::Array(items) => JsonValue::Array(items.iter().map(redact_json_value).collect()),
        JsonValue::Object(fields) => {
            let mut output = JsonMap::new();
            for (key, value) in fields {
                let key_is_sensitive =
                    crate::runtime::redaction::is_sensitive_argument_name(key.as_str());
                output.insert(
                    key.clone(),
                    if key_is_sensitive {
                        JsonValue::String("[redacted]".into())
                    } else {
                        redact_json_value(value)
                    },
                );
            }
            JsonValue::Object(output)
        }
    }
}

fn json_value_contains_redaction_marker(value: &JsonValue) -> bool {
    match value {
        JsonValue::String(text) => text.contains("[redacted]") || text.contains("[REDACTED]"),
        JsonValue::Array(items) => items.iter().any(json_value_contains_redaction_marker),
        JsonValue::Object(fields) => fields.values().any(json_value_contains_redaction_marker),
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => false,
    }
}

fn sanitize_text_for_output(value: &str) -> String {
    let (text, _redaction) = redact_session_context_text(value);
    text.replace("--- BEGIN", "[retrieved boundary marker: BEGIN]")
        .replace("--- END", "[retrieved boundary marker: END]")
}

fn non_empty_snippet(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "[empty]".into()
    } else {
        truncate_chars(trimmed, MAX_CONTEXT_TEXT_CHARS)
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

fn strongest_record_redaction_state(
    stored: &project_store::ProjectRecordRedactionState,
    text_redacted: bool,
) -> project_store::ProjectRecordRedactionState {
    if *stored == project_store::ProjectRecordRedactionState::Blocked {
        project_store::ProjectRecordRedactionState::Blocked
    } else if *stored == project_store::ProjectRecordRedactionState::Redacted || text_redacted {
        project_store::ProjectRecordRedactionState::Redacted
    } else {
        project_store::ProjectRecordRedactionState::Clean
    }
}

fn project_record_to_context_redaction(
    stored: &project_store::ProjectRecordRedactionState,
    text_redacted: bool,
) -> project_store::AgentContextRedactionState {
    match strongest_record_redaction_state(stored, text_redacted) {
        project_store::ProjectRecordRedactionState::Blocked => {
            project_store::AgentContextRedactionState::Blocked
        }
        project_store::ProjectRecordRedactionState::Redacted => {
            project_store::AgentContextRedactionState::Redacted
        }
        project_store::ProjectRecordRedactionState::Clean => {
            project_store::AgentContextRedactionState::Clean
        }
    }
}

fn project_record_kind_label(kind: &project_store::ProjectRecordKind) -> &'static str {
    project_store::project_record_kind_sql_value(kind)
}

fn project_record_importance_label(
    importance: &project_store::ProjectRecordImportance,
) -> &'static str {
    match importance {
        project_store::ProjectRecordImportance::Low => "low",
        project_store::ProjectRecordImportance::Normal => "normal",
        project_store::ProjectRecordImportance::High => "high",
        project_store::ProjectRecordImportance::Critical => "critical",
    }
}

fn project_record_redaction_state_label(
    redaction_state: &project_store::ProjectRecordRedactionState,
) -> &'static str {
    match redaction_state {
        project_store::ProjectRecordRedactionState::Clean => "clean",
        project_store::ProjectRecordRedactionState::Redacted => "redacted",
        project_store::ProjectRecordRedactionState::Blocked => "blocked",
    }
}

fn project_record_visibility_label(
    visibility: &project_store::ProjectRecordVisibility,
) -> &'static str {
    match visibility {
        project_store::ProjectRecordVisibility::Workflow => "workflow",
        project_store::ProjectRecordVisibility::Retrieval => "retrieval",
        project_store::ProjectRecordVisibility::MemoryCandidate => "memory_candidate",
        project_store::ProjectRecordVisibility::Diagnostic => "diagnostic",
    }
}

fn agent_memory_scope_label(scope: &project_store::AgentMemoryScope) -> &'static str {
    match scope {
        project_store::AgentMemoryScope::Project => "project",
        project_store::AgentMemoryScope::Session => "session",
    }
}

fn agent_memory_kind_label(kind: &project_store::AgentMemoryKind) -> &'static str {
    match kind {
        project_store::AgentMemoryKind::ProjectFact => "project_fact",
        project_store::AgentMemoryKind::UserPreference => "user_preference",
        project_store::AgentMemoryKind::Decision => "decision",
        project_store::AgentMemoryKind::SessionSummary => "session_summary",
        project_store::AgentMemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn retrieval_source_kind_label(
    source_kind: &project_store::AgentRetrievalResultSourceKind,
) -> &'static str {
    match source_kind {
        project_store::AgentRetrievalResultSourceKind::ProjectRecord => "project_record",
        project_store::AgentRetrievalResultSourceKind::ApprovedMemory => "approved_memory",
        project_store::AgentRetrievalResultSourceKind::Handoff => "handoff",
        project_store::AgentRetrievalResultSourceKind::ContextManifest => "context_manifest",
    }
}

fn context_redaction_state_label(
    redaction_state: &project_store::AgentContextRedactionState,
) -> &'static str {
    match redaction_state {
        project_store::AgentContextRedactionState::Clean => "clean",
        project_store::AgentContextRedactionState::Redacted => "redacted",
        project_store::AgentContextRedactionState::Blocked => "blocked",
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
