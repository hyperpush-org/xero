use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto,
        RuntimeRunControlInputDto, RuntimeRunControlStateDto,
    },
    db::{self, project_store},
    git::repository::CanonicalRepository,
    runtime::{
        create_owned_agent_run, run_owned_agent_task, AgentProviderConfig,
        AutonomousProjectContextAction, AutonomousProjectContextRecordImportance,
        AutonomousProjectContextRecordKind, AutonomousProjectContextRequest, AutonomousToolOutput,
        AutonomousToolRequest, AutonomousToolRuntime, OwnedAgentRunRequest, ToolRegistry,
        ToolRegistryOptions, AUTONOMOUS_TOOL_PROJECT_CONTEXT,
    },
    state::DesktopState,
};

fn seed_project(root: &TempDir) -> (String, PathBuf) {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");
    let project_id = format!(
        "freshness-phase1-{}",
        root.path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
    );
    let repository = CanonicalRepository {
        project_id: project_id.clone(),
        repository_id: format!("{project_id}-repo"),
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

fn controls_for_agent(runtime_agent_id: RuntimeAgentIdDto) -> RuntimeRunControlInputDto {
    RuntimeRunControlInputDto {
        runtime_agent_id,
        agent_definition_id: None,
        provider_profile_id: None,
        model_id: "test-model".into(),
        thinking_effort: None,
        approval_mode: RuntimeRunApprovalModeDto::Suggest,
        plan_mode_required: false,
    }
}

fn control_state_for_agent(runtime_agent_id: RuntimeAgentIdDto) -> RuntimeRunControlStateDto {
    RuntimeRunControlStateDto {
        active: RuntimeRunActiveControlSnapshotDto {
            runtime_agent_id,
            agent_definition_id: None,
            agent_definition_version: None,
            provider_profile_id: None,
            model_id: "test-model".into(),
            thinking_effort: None,
            approval_mode: RuntimeRunApprovalModeDto::Suggest,
            plan_mode_required: false,
            revision: 1,
            applied_at: "2026-05-03T12:00:00Z".into(),
        },
        pending: None,
    }
}

fn seed_agent_run_for_agent(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    runtime_agent_id: RuntimeAgentIdDto,
) {
    project_store::insert_agent_run(
        repo_root,
        &project_store::NewAgentRunRecord {
            runtime_agent_id,
            agent_definition_id: None,
            agent_definition_version: None,
            project_id: project_id.into(),
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: run_id.into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            prompt: "Exercise the LanceDB freshness contract.".into(),
            system_prompt: "system".into(),
            now: "2026-05-03T12:00:00Z".into(),
        },
    )
    .expect("seed agent run");
}

fn write_repo_file(repo_root: &Path, relative_path: &str, contents: &str) -> String {
    let path = repo_root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(&path, contents).expect("write repo file");
    sha256_file(&path)
}

fn sha256_file(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(fs::read(path).expect("read file for sha256"));
    format!("{:x}", hasher.finalize())
}

fn seed_project_record(
    repo_root: &Path,
    project_id: &str,
    record_id: &str,
    title: &str,
    text: &str,
    related_paths: Vec<String>,
    created_at: &str,
) {
    project_store::insert_project_record(
        repo_root,
        &project_store::NewProjectRecordRecord {
            record_id: record_id.into(),
            project_id: project_id.into(),
            record_kind: project_store::ProjectRecordKind::Decision,
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "engineer".into(),
            agent_definition_version: project_store::BUILTIN_AGENT_DEFINITION_VERSION,
            agent_session_id: Some(project_store::DEFAULT_AGENT_SESSION_ID.into()),
            run_id: format!("{record_id}-run"),
            workflow_run_id: None,
            workflow_step_id: None,
            title: title.into(),
            summary: format!("{title} summary"),
            text: text.into(),
            content_json: Some(json!({"phase": "lancedb_freshness_phase1"})),
            schema_name: Some("xero.test.lancedb_freshness_phase1".into()),
            schema_version: 1,
            importance: project_store::ProjectRecordImportance::High,
            confidence: Some(0.94),
            tags: vec!["freshness-phase1".into()],
            source_item_ids: vec![format!("test:{record_id}")],
            related_paths,
            produced_artifact_refs: Vec::new(),
            redaction_state: project_store::ProjectRecordRedactionState::Clean,
            visibility: project_store::ProjectRecordVisibility::Retrieval,
            created_at: created_at.into(),
        },
    )
    .expect("seed project record");
}

fn search_context(
    repo_root: &Path,
    project_id: &str,
    query_id: &str,
    query_text: &str,
    search_scope: project_store::AgentRetrievalSearchScope,
) -> project_store::AgentContextRetrievalResponse {
    project_store::search_agent_context(
        repo_root,
        project_store::AgentContextRetrievalRequest {
            query_id: query_id.into(),
            project_id: project_id.into(),
            agent_session_id: Some(project_store::DEFAULT_AGENT_SESSION_ID.into()),
            run_id: None,
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "engineer".into(),
            agent_definition_version: project_store::BUILTIN_AGENT_DEFINITION_VERSION,
            query_text: query_text.into(),
            search_scope,
            filters: project_store::AgentContextRetrievalFilters::default(),
            limit_count: 10,
            allow_keyword_fallback: true,
            created_at: "2026-05-03T12:30:00Z".into(),
        },
    )
    .expect("search context")
}

fn result_by_source<'a>(
    response: &'a project_store::AgentContextRetrievalResponse,
    source_id: &str,
) -> &'a project_store::AgentContextRetrievalResult {
    response
        .results
        .iter()
        .find(|result| result.source_id == source_id)
        .unwrap_or_else(|| panic!("expected retrieval result for {source_id}"))
}

fn freshness_state(result: &project_store::AgentContextRetrievalResult) -> Option<&str> {
    result
        .metadata
        .get("freshness")
        .and_then(|freshness| freshness.get("state"))
        .and_then(serde_json::Value::as_str)
}

fn backfill_job_by_id<'a>(
    jobs: &'a [project_store::AgentEmbeddingBackfillJobRecord],
    job_id: &str,
) -> &'a project_store::AgentEmbeddingBackfillJobRecord {
    jobs.iter()
        .find(|job| job.job_id == job_id)
        .unwrap_or_else(|| panic!("expected backfill job {job_id}"))
}

fn backfill_diagnostic_str<'a>(
    job: &'a project_store::AgentEmbeddingBackfillJobRecord,
    field: &str,
) -> Option<&'a str> {
    job.diagnostic
        .as_ref()
        .and_then(|diagnostic| diagnostic.get(field))
        .and_then(serde_json::Value::as_str)
}

fn execute_project_context(
    runtime: &AutonomousToolRuntime,
    request: AutonomousProjectContextRequest,
) -> xero_desktop_lib::runtime::AutonomousProjectContextOutput {
    let output = runtime
        .execute(AutonomousToolRequest::ProjectContext(request))
        .expect("execute project_context tool");
    match output.output {
        AutonomousToolOutput::ProjectContext(output) => output,
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn lancedb_freshness_phase1_marks_related_path_current_then_stale_after_hash_change() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let related_path = "src/auth_flow.rs";
    let initial_hash = write_repo_file(&repo_root, related_path, "pub fn auth_flow() {}\n");
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-current-stale-record",
        "freshcontract auth flow decision",
        "freshcontract auth flow lives in src/auth_flow.rs.",
        vec![related_path.into()],
        "2026-05-03T12:01:00Z",
    );

    let before = search_context(
        &repo_root,
        &project_id,
        "fresh-current-before",
        "freshcontract auth flow",
        project_store::AgentRetrievalSearchScope::ProjectRecords,
    );
    let before_result = result_by_source(&before, "fresh-current-stale-record");
    assert_eq!(freshness_state(before_result), Some("current"));
    assert_eq!(
        before_result.metadata["freshness"]["sourceFingerprints"][0]["path"],
        related_path
    );
    assert_eq!(
        before_result.metadata["freshness"]["sourceFingerprints"][0]["hash"],
        initial_hash
    );

    let changed_hash = write_repo_file(
        &repo_root,
        related_path,
        "pub fn auth_flow() { /* refactored */ }\n",
    );
    let after = search_context(
        &repo_root,
        &project_id,
        "fresh-current-after",
        "freshcontract auth flow",
        project_store::AgentRetrievalSearchScope::ProjectRecords,
    );
    let after_result = result_by_source(&after, "fresh-current-stale-record");
    assert_eq!(freshness_state(after_result), Some("stale"));
    assert_eq!(
        after_result.metadata["freshness"]["sourceFingerprints"][0]["currentHash"],
        changed_hash
    );
    assert!(
        after_result.metadata["freshness"]["staleReason"]
            .as_str()
            .is_some_and(|reason| reason.contains("hash changed")),
        "stale records must explain the source hash mismatch"
    );
}

#[test]
fn lancedb_freshness_phase1_marks_deleted_related_path_source_missing() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let related_path = "src/deleted_module.rs";
    let initial_hash = write_repo_file(&repo_root, related_path, "pub fn deleted_module() {}\n");
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-source-missing-record",
        "freshcontract deleted module decision",
        "freshcontract deleted module still describes src/deleted_module.rs.",
        vec![related_path.into()],
        "2026-05-03T12:02:00Z",
    );
    fs::remove_file(repo_root.join(related_path)).expect("delete related path");

    let response = search_context(
        &repo_root,
        &project_id,
        "fresh-source-missing",
        "freshcontract deleted module",
        project_store::AgentRetrievalSearchScope::ProjectRecords,
    );
    let result = result_by_source(&response, "fresh-source-missing-record");
    assert_eq!(freshness_state(result), Some("source_missing"));
    assert_eq!(
        result.metadata["freshness"]["sourceFingerprints"][0]["hash"],
        initial_hash
    );
    assert_eq!(
        result.metadata["freshness"]["sourceFingerprints"][0]["exists"],
        false
    );
    assert!(
        result.metadata["freshness"]["staleReason"]
            .as_str()
            .is_some_and(|reason| reason.contains(related_path)),
        "missing-source evidence must name the absent path"
    );
}

#[test]
fn lancedb_freshness_phase1_supersedes_older_record_with_same_fact_key() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let related_path = "src/credential_helper.rs";
    write_repo_file(&repo_root, related_path, "pub fn credential_helper() {}\n");
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-superseded-old",
        "freshcontract credential helper decision",
        "freshcontract credential helper lives in the legacy auth module.",
        vec![related_path.into()],
        "2026-05-03T12:03:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-superseded-new",
        "freshcontract credential helper decision",
        "freshcontract credential helper now lives in the app-data auth module.",
        vec![related_path.into()],
        "2026-05-03T12:04:00Z",
    );

    let response = search_context(
        &repo_root,
        &project_id,
        "fresh-supersession",
        "freshcontract credential helper",
        project_store::AgentRetrievalSearchScope::ProjectRecords,
    );
    let old = result_by_source(&response, "fresh-superseded-old");
    let new = result_by_source(&response, "fresh-superseded-new");
    assert_eq!(freshness_state(new), Some("current"));
    assert_eq!(freshness_state(old), Some("superseded"));
    assert_eq!(
        old.metadata["freshness"]["supersededById"],
        "fresh-superseded-new"
    );
    assert_eq!(
        old.metadata["freshness"]["factKey"], new.metadata["freshness"]["factKey"],
        "records with the same stable fact key should link supersession"
    );
}

#[test]
fn lancedb_freshness_phase1_marks_approved_memory_stale_after_source_file_change() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let source_path = "src/memory_source.rs";
    let initial_hash = write_repo_file(&repo_root, source_path, "pub fn memory_source() {}\n");
    seed_agent_run_for_agent(
        &repo_root,
        &project_id,
        "fresh-memory-source-run",
        RuntimeAgentIdDto::Engineer,
    );
    let file_change = project_store::append_agent_file_change(
        &repo_root,
        &project_store::NewAgentFileChangeRecord {
            project_id: project_id.clone(),
            run_id: "fresh-memory-source-run".into(),
            path: source_path.into(),
            operation: "write".into(),
            old_hash: None,
            new_hash: Some(initial_hash.clone()),
            created_at: "2026-05-03T12:05:00Z".into(),
        },
    )
    .expect("record source file change");
    project_store::insert_agent_memory(
        &repo_root,
        &project_store::NewAgentMemoryRecord {
            memory_id: "fresh-stale-memory".into(),
            project_id: project_id.clone(),
            agent_session_id: Some(project_store::DEFAULT_AGENT_SESSION_ID.into()),
            scope: project_store::AgentMemoryScope::Session,
            kind: project_store::AgentMemoryKind::ProjectFact,
            text: "freshcontract approved memory derives from memory_source.rs.".into(),
            review_state: project_store::AgentMemoryReviewState::Approved,
            enabled: true,
            confidence: Some(95),
            source_run_id: Some("fresh-memory-source-run".into()),
            source_item_ids: vec![format!("agent_file_changes:{}", file_change.id)],
            diagnostic: None,
            created_at: "2026-05-03T12:06:00Z".into(),
        },
    )
    .expect("insert approved memory");

    let changed_hash = write_repo_file(
        &repo_root,
        source_path,
        "pub fn memory_source() { /* changed */ }\n",
    );
    let response = search_context(
        &repo_root,
        &project_id,
        "fresh-memory-stale",
        "freshcontract approved memory",
        project_store::AgentRetrievalSearchScope::ApprovedMemory,
    );
    let result = result_by_source(&response, "fresh-stale-memory");
    assert_eq!(freshness_state(result), Some("stale"));
    assert_eq!(
        result.metadata["freshness"]["sourceFingerprints"][0]["path"],
        source_path
    );
    assert_eq!(
        result.metadata["freshness"]["sourceFingerprints"][0]["hash"],
        initial_hash
    );
    assert_eq!(
        result.metadata["freshness"]["sourceFingerprints"][0]["currentHash"],
        changed_hash
    );
    let memory = project_store::get_agent_memory(&repo_root, &project_id, "fresh-stale-memory")
        .expect("load stale approved memory");
    assert_eq!(
        memory.review_state,
        project_store::AgentMemoryReviewState::Approved
    );
    assert!(memory.enabled);
    assert_eq!(memory.freshness_state, "stale");
}

#[test]
fn lancedb_freshness_phase1_provider_turn_prompts_do_not_preload_raw_memory_or_records() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let related_path = "src/provider_prompt_source.rs";
    write_repo_file(
        &repo_root,
        related_path,
        "pub fn provider_prompt_source() {}\n",
    );
    project_store::insert_agent_memory(
        &repo_root,
        &project_store::NewAgentMemoryRecord {
            memory_id: "fresh-raw-memory".into(),
            project_id: project_id.clone(),
            agent_session_id: None,
            scope: project_store::AgentMemoryScope::Project,
            kind: project_store::AgentMemoryKind::ProjectFact,
            text: "FRESHNESS_RAW_MEMORY_SHOULD_NOT_APPEAR".into(),
            review_state: project_store::AgentMemoryReviewState::Approved,
            enabled: true,
            confidence: Some(95),
            source_run_id: Some("fresh-raw-source-run".into()),
            source_item_ids: vec!["fresh:raw-memory".into()],
            diagnostic: None,
            created_at: "2026-05-03T12:07:00Z".into(),
        },
    )
    .expect("insert raw memory");
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-raw-project-record",
        "freshcontract provider prompt record",
        "FRESHNESS_RAW_PROJECT_RECORD_SHOULD_NOT_APPEAR",
        vec![related_path.into()],
        "2026-05-03T12:08:00Z",
    );

    let tool_runtime = AutonomousToolRuntime::new(&repo_root).expect("tool runtime");
    let created = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "fresh-provider-prompt-run".into(),
        prompt: "Use freshcontract provider prompt record and explain remembered project context."
            .into(),
        attachments: Vec::new(),
        controls: Some(controls_for_agent(RuntimeAgentIdDto::Ask)),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("create run");

    for raw_text in [
        "FRESHNESS_RAW_MEMORY_SHOULD_NOT_APPEAR",
        "FRESHNESS_RAW_PROJECT_RECORD_SHOULD_NOT_APPEAR",
    ] {
        assert!(
            !created.run.system_prompt.contains(raw_text),
            "provider prompts should advertise context tools instead of preloading `{raw_text}`"
        );
    }

    let manifests = project_store::list_agent_context_manifests_for_run(
        &repo_root,
        &project_id,
        "fresh-provider-prompt-run",
    )
    .expect("list context manifests");
    let manifest_text = serde_json::to_string(&manifests[0].manifest).expect("manifest json");
    assert!(!manifest_text.contains("FRESHNESS_RAW_MEMORY_SHOULD_NOT_APPEAR"));
    assert!(!manifest_text.contains("FRESHNESS_RAW_PROJECT_RECORD_SHOULD_NOT_APPEAR"));
    assert_eq!(
        manifests[0].manifest["retrieval"]["deliveryModel"],
        "tool_mediated"
    );
}

#[test]
fn lancedb_freshness_phase1_all_agents_can_read_record_and_update_context() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    for runtime_agent_id in [
        RuntimeAgentIdDto::Ask,
        RuntimeAgentIdDto::Engineer,
        RuntimeAgentIdDto::Debug,
    ] {
        let registry = ToolRegistry::builtin_with_options(ToolRegistryOptions {
            runtime_agent_id,
            ..ToolRegistryOptions::default()
        });
        let descriptor = registry
            .descriptor(AUTONOMOUS_TOOL_PROJECT_CONTEXT)
            .expect("project_context descriptor");
        let actions = descriptor.input_schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<BTreeSet<_>>();
        for expected in [
            "search_project_records",
            "search_approved_memory",
            "get_project_record",
            "get_memory",
            "record_context",
            "update_context",
            "refresh_freshness",
        ] {
            assert!(
                actions.contains(expected),
                "{runtime_agent_id:?} should receive durable-context action `{expected}`"
            );
        }

        seed_agent_run_for_agent(
            &repo_root,
            &project_id,
            &format!("fresh-tool-{}-run", runtime_agent_id.as_str()),
            runtime_agent_id,
        );
        let runtime = AutonomousToolRuntime::new(&repo_root)
            .expect("tool runtime")
            .with_runtime_run_controls(control_state_for_agent(runtime_agent_id))
            .with_agent_run_context(
                &project_id,
                project_store::DEFAULT_AGENT_SESSION_ID,
                format!("fresh-tool-{}-run", runtime_agent_id.as_str()),
            );
        let mut request =
            AutonomousProjectContextRequest::new(AutonomousProjectContextAction::RecordContext);
        request.title = Some(format!("freshcontract {:?} context", runtime_agent_id));
        request.summary = Some("All agents can record durable context corrections.".into());
        request.text = Some("freshcontract durable-context update from agent tool.".into());
        request.record_kind = Some(AutonomousProjectContextRecordKind::ContextNote);
        request.importance = Some(AutonomousProjectContextRecordImportance::Normal);
        let output = runtime
            .execute(AutonomousToolRequest::ProjectContext(request))
            .unwrap_or_else(|error| {
                panic!(
                    "{runtime_agent_id:?} should be allowed to record/update durable context: {error:?}"
                )
            });
        let output = match output.output {
            AutonomousToolOutput::ProjectContext(output) => output,
            other => panic!("unexpected output: {other:?}"),
        };
        let record = output.record.expect("record_context output record");
        assert_eq!(record.visibility, "retrieval");
    }
}

#[test]
fn lancedb_freshness_phase7_update_context_supersedes_target_record_automatically() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let related_path = "src/phase7_update.rs";
    write_repo_file(&repo_root, related_path, "pub fn phase7_update() {}\n");
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-phase7-update-old",
        "freshcontract phase7 update target",
        "freshcontract phase7 update target still says the old implementation owns this path.",
        vec![related_path.into()],
        "2026-05-03T12:21:00Z",
    );
    seed_agent_run_for_agent(
        &repo_root,
        &project_id,
        "fresh-phase7-update-run",
        RuntimeAgentIdDto::Engineer,
    );
    let runtime = AutonomousToolRuntime::new(&repo_root)
        .expect("tool runtime")
        .with_runtime_run_controls(control_state_for_agent(RuntimeAgentIdDto::Engineer))
        .with_agent_run_context(
            &project_id,
            project_store::DEFAULT_AGENT_SESSION_ID,
            "fresh-phase7-update-run",
        );
    let mut request =
        AutonomousProjectContextRequest::new(AutonomousProjectContextAction::UpdateContext);
    request.record_id = Some("fresh-phase7-update-old".into());
    request.text =
        Some("freshcontract phase7 update target now records the newer implementation.".into());
    let output = runtime
        .execute(AutonomousToolRequest::ProjectContext(request))
        .expect("update context");
    let output = match output.output {
        AutonomousToolOutput::ProjectContext(output) => output,
        other => panic!("unexpected output: {other:?}"),
    };
    let new_record = output.record.expect("update output record");
    assert_eq!(new_record.visibility, "retrieval");
    assert_eq!(new_record.trust["supersedesId"], "fresh-phase7-update-old");

    let response = search_context(
        &repo_root,
        &project_id,
        "fresh-phase7-update-search",
        "freshcontract phase7 update target",
        project_store::AgentRetrievalSearchScope::ProjectRecords,
    );
    let old = result_by_source(&response, "fresh-phase7-update-old");
    let new = result_by_source(&response, &new_record.record_id);
    assert_eq!(freshness_state(old), Some("superseded"));
    assert_eq!(freshness_state(new), Some("current"));
    assert_eq!(
        old.metadata["freshness"]["supersededById"],
        new_record.record_id
    );
}

#[test]
fn lancedb_freshness_phase7_refresh_freshness_targets_selected_record_ids() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let first_path = "src/phase7_first.rs";
    let second_path = "src/phase7_second.rs";
    write_repo_file(&repo_root, first_path, "pub fn phase7_first() {}\n");
    write_repo_file(&repo_root, second_path, "pub fn phase7_second() {}\n");
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-phase7-refresh-first",
        "freshcontract phase7 targeted first",
        "freshcontract phase7 targeted refresh first record.",
        vec![first_path.into()],
        "2026-05-03T12:22:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-phase7-refresh-second",
        "freshcontract phase7 targeted second",
        "freshcontract phase7 targeted refresh second record.",
        vec![second_path.into()],
        "2026-05-03T12:23:00Z",
    );
    write_repo_file(
        &repo_root,
        first_path,
        "pub fn phase7_first() { /* changed */ }\n",
    );
    write_repo_file(
        &repo_root,
        second_path,
        "pub fn phase7_second() { /* changed */ }\n",
    );
    seed_agent_run_for_agent(
        &repo_root,
        &project_id,
        "fresh-phase7-refresh-run",
        RuntimeAgentIdDto::Debug,
    );
    let runtime = AutonomousToolRuntime::new(&repo_root)
        .expect("tool runtime")
        .with_runtime_run_controls(control_state_for_agent(RuntimeAgentIdDto::Debug))
        .with_agent_run_context(
            &project_id,
            project_store::DEFAULT_AGENT_SESSION_ID,
            "fresh-phase7-refresh-run",
        );
    let mut request =
        AutonomousProjectContextRequest::new(AutonomousProjectContextAction::RefreshFreshness);
    request.record_ids = vec!["fresh-phase7-refresh-first".into()];
    let output = runtime
        .execute(AutonomousToolRequest::ProjectContext(request))
        .expect("targeted freshness refresh");
    let output = match output.output {
        AutonomousToolOutput::ProjectContext(output) => output,
        other => panic!("unexpected output: {other:?}"),
    };
    assert_eq!(output.result_count, 1);
    assert_eq!(output.manifest.as_ref().unwrap()["staleCount"], 1);

    let records =
        project_store::list_project_records(&repo_root, &project_id).expect("list records");
    let first = records
        .iter()
        .find(|record| record.record_id == "fresh-phase7-refresh-first")
        .expect("first record");
    let second = records
        .iter()
        .find(|record| record.record_id == "fresh-phase7-refresh-second")
        .expect("second record");
    assert_eq!(first.freshness_state, "stale");
    assert_eq!(second.freshness_state, "current");
}

#[test]
fn lancedb_freshness_phase1_tool_guidance_requires_read_before_prior_work_and_record_after_findings(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    for runtime_agent_id in [
        RuntimeAgentIdDto::Ask,
        RuntimeAgentIdDto::Engineer,
        RuntimeAgentIdDto::Debug,
    ] {
        let tool_runtime = AutonomousToolRuntime::new(&repo_root).expect("tool runtime");
        let run_id = format!("fresh-guidance-{}-run", runtime_agent_id.as_str());
        let created = create_owned_agent_run(&OwnedAgentRunRequest {
            repo_root: repo_root.clone(),
            project_id: project_id.clone(),
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id,
            prompt: "Last time we changed this subsystem; continue from prior work.".into(),
            attachments: Vec::new(),
            controls: Some(controls_for_agent(runtime_agent_id)),
            tool_runtime,
            provider_config: AgentProviderConfig::Fake,
        })
        .expect("create run");
        assert!(
            created
                .run
                .system_prompt
                .contains("read context before prior-work-sensitive tasks"),
            "{runtime_agent_id:?} prompt should require context reads before prior-work-sensitive tasks"
        );
        assert!(
            created
                .run
                .system_prompt
                .contains("record/update context after durable findings"),
            "{runtime_agent_id:?} prompt should require context recording after durable findings"
        );
        let descriptor = ToolRegistry::builtin_with_options(ToolRegistryOptions {
            runtime_agent_id,
            ..ToolRegistryOptions::default()
        })
        .descriptor(AUTONOMOUS_TOOL_PROJECT_CONTEXT)
        .expect("project_context descriptor")
        .clone();
        assert!(descriptor.description.contains("freshness evidence"));
        assert!(descriptor.description.contains("stale"));
        assert!(descriptor.description.contains("record/update"));
    }
}

#[test]
fn lancedb_freshness_phase1_project_context_tool_returns_every_freshness_state() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    seed_agent_run_for_agent(
        &repo_root,
        &project_id,
        "fresh-state-tool-run",
        RuntimeAgentIdDto::Debug,
    );
    let current_path = "src/current.rs";
    let stale_path = "src/stale.rs";
    let missing_path = "src/missing.rs";
    let superseded_path = "src/superseded.rs";
    write_repo_file(&repo_root, current_path, "pub fn current() {}\n");
    write_repo_file(&repo_root, stale_path, "pub fn stale() {}\n");
    write_repo_file(&repo_root, missing_path, "pub fn missing() {}\n");
    write_repo_file(&repo_root, superseded_path, "pub fn superseded() {}\n");
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-state-current",
        "freshcontract state current",
        "freshcontract state current result.",
        vec![current_path.into()],
        "2026-05-03T12:09:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-state-stale",
        "freshcontract state stale",
        "freshcontract state stale result.",
        vec![stale_path.into()],
        "2026-05-03T12:10:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-state-missing",
        "freshcontract state missing",
        "freshcontract state missing result.",
        vec![missing_path.into()],
        "2026-05-03T12:11:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-state-unknown",
        "freshcontract state unknown",
        "freshcontract state unknown result.",
        Vec::new(),
        "2026-05-03T12:12:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-state-superseded-old",
        "freshcontract state superseded",
        "freshcontract state superseded old result.",
        vec![superseded_path.into()],
        "2026-05-03T12:13:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-state-superseded-new",
        "freshcontract state superseded",
        "freshcontract state superseded new result.",
        vec![superseded_path.into()],
        "2026-05-03T12:14:00Z",
    );
    write_repo_file(&repo_root, stale_path, "pub fn stale() { /* changed */ }\n");
    fs::remove_file(repo_root.join(missing_path)).expect("remove missing path");

    let runtime = AutonomousToolRuntime::new(&repo_root)
        .expect("tool runtime")
        .with_runtime_run_controls(control_state_for_agent(RuntimeAgentIdDto::Debug))
        .with_agent_run_context(
            &project_id,
            project_store::DEFAULT_AGENT_SESSION_ID,
            "fresh-state-tool-run",
        );
    let mut request =
        AutonomousProjectContextRequest::new(AutonomousProjectContextAction::SearchProjectRecords);
    request.query = Some("freshcontract state".into());
    request.limit = Some(10);
    let output = runtime
        .execute(AutonomousToolRequest::ProjectContext(request))
        .expect("project context search");
    let output = match output.output {
        AutonomousToolOutput::ProjectContext(output) => output,
        other => panic!("unexpected output: {other:?}"),
    };
    let states = output
        .results
        .iter()
        .filter_map(|result| {
            result
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("freshness"))
                .and_then(|freshness| freshness.get("state"))
                .and_then(serde_json::Value::as_str)
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        states,
        BTreeSet::from([
            "current",
            "source_unknown",
            "stale",
            "source_missing",
            "superseded",
        ])
    );
    assert!(output.results.iter().all(|result| {
        result
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("freshness"))
            .and_then(|freshness| freshness.get("checkedAt"))
            .and_then(serde_json::Value::as_str)
            .is_some()
    }));
}

#[test]
fn lancedb_freshness_phase1_context_manifests_record_tool_retrieval_and_freshness_diagnostics() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let related_path = "src/manifest_freshness.rs";
    write_repo_file(&repo_root, related_path, "pub fn manifest_freshness() {}\n");
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-manifest-record",
        "freshcontract manifest diagnostics",
        "freshcontract manifest diagnostics result.",
        vec![related_path.into()],
        "2026-05-03T12:15:00Z",
    );

    let tool_runtime = AutonomousToolRuntime::new(&repo_root).expect("tool runtime");
    run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "fresh-manifest-run".into(),
        prompt: "Use freshcontract manifest diagnostics from durable context.".into(),
        attachments: Vec::new(),
        controls: Some(controls_for_agent(RuntimeAgentIdDto::Engineer)),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("run provider turn");
    let manifests = project_store::list_agent_context_manifests_for_run(
        &repo_root,
        &project_id,
        "fresh-manifest-run",
    )
    .expect("list context manifests");
    let manifest = &manifests[0].manifest;
    assert_eq!(manifest["retrieval"]["deliveryModel"], "tool_mediated");
    assert_eq!(manifest["retrieval"]["rawContextInjected"], false);
    assert_eq!(
        manifest["retrieval"]["freshnessDiagnostics"]["currentCount"],
        1
    );
    assert!(manifest["retrieval"]["freshnessDiagnostics"]
        .get("staleCount")
        .is_some());
    assert!(manifest["retrieval"]["toolAvailability"]["project_context"].is_boolean());
}

#[test]
fn lancedb_freshness_phase1_diagnostics_can_inspect_blocked_records_without_model_exposure() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-visible-record",
        "freshcontract visible diagnostic",
        "freshcontract visible diagnostic result.",
        Vec::new(),
        "2026-05-03T12:16:00Z",
    );
    project_store::insert_project_record(
        &repo_root,
        &project_store::NewProjectRecordRecord {
            record_id: "fresh-blocked-record".into(),
            project_id: project_id.clone(),
            record_kind: project_store::ProjectRecordKind::Diagnostic,
            runtime_agent_id: RuntimeAgentIdDto::Debug,
            agent_definition_id: "debug".into(),
            agent_definition_version: project_store::BUILTIN_AGENT_DEFINITION_VERSION,
            agent_session_id: Some(project_store::DEFAULT_AGENT_SESSION_ID.into()),
            run_id: "fresh-blocked-record-run".into(),
            workflow_run_id: None,
            workflow_step_id: None,
            title: "freshcontract blocked diagnostic".into(),
            summary: "Blocked records stay out of model-visible context.".into(),
            text: "freshcontract blocked diagnostic api_key=sk-blocked-secret".into(),
            content_json: Some(json!({"diagnostic": "blocked"})),
            schema_name: Some("xero.test.blocked_diagnostic".into()),
            schema_version: 1,
            importance: project_store::ProjectRecordImportance::High,
            confidence: Some(0.9),
            tags: vec!["freshness-phase1".into()],
            source_item_ids: vec!["test:blocked".into()],
            related_paths: Vec::new(),
            produced_artifact_refs: Vec::new(),
            redaction_state: project_store::ProjectRecordRedactionState::Blocked,
            visibility: project_store::ProjectRecordVisibility::Diagnostic,
            created_at: "2026-05-03T12:17:00Z".into(),
        },
    )
    .expect("insert blocked record");

    let response = search_context(
        &repo_root,
        &project_id,
        "fresh-blocked-search",
        "freshcontract diagnostic",
        project_store::AgentRetrievalSearchScope::ProjectRecords,
    );
    assert!(!response
        .results
        .iter()
        .any(|result| result.source_id == "fresh-blocked-record"));
    assert_eq!(
        response
            .diagnostic
            .as_ref()
            .and_then(|diagnostic| diagnostic.get("freshnessDiagnostics"))
            .and_then(|freshness| freshness.get("blockedExcludedCount"))
            .and_then(serde_json::Value::as_u64),
        Some(1),
        "internal diagnostics should count blocked rows without returning them to model-visible results"
    );
    let diagnostic_rows = project_store::list_project_records(&repo_root, &project_id)
        .expect("developer diagnostic list");
    assert!(diagnostic_rows
        .iter()
        .any(|record| record.record_id == "fresh-blocked-record"
            && record.redaction_state == project_store::ProjectRecordRedactionState::Blocked));
}

#[test]
fn lancedb_freshness_phase1_embedding_backfill_skips_hash_mismatched_jobs() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-backfill-record",
        "freshcontract backfill hash mismatch",
        "freshcontract embedding backfill should skip stale source hashes.",
        Vec::new(),
        "2026-05-03T12:18:00Z",
    );
    project_store::enqueue_agent_embedding_backfill_job(
        &repo_root,
        &project_store::NewAgentEmbeddingBackfillJobRecord {
            job_id: "fresh-backfill-mismatch-job".into(),
            project_id: project_id.clone(),
            source_kind: project_store::AgentEmbeddingBackfillSourceKind::ProjectRecord,
            source_id: "fresh-backfill-record".into(),
            source_hash: "f".repeat(64),
            embedding_model: project_store::DEFAULT_AGENT_EMBEDDING_MODEL.into(),
            embedding_dimension: project_store::AGENT_RETRIEVAL_EMBEDDING_DIM,
            embedding_version: project_store::DEFAULT_AGENT_EMBEDDING_VERSION.into(),
            created_at: "2026-05-03T12:19:00Z".into(),
        },
    )
    .expect("enqueue mismatched job");

    let run = project_store::run_agent_embedding_backfill_jobs(
        &repo_root,
        &project_id,
        5,
        "2026-05-03T12:20:00Z",
    )
    .expect("run backfill");
    assert_eq!(run.queued_count, 1);
    assert_eq!(run.skipped_count, 1);
    assert_eq!(run.succeeded_count, 0);
    let jobs = project_store::list_agent_embedding_backfill_jobs(&repo_root, &project_id)
        .expect("list backfill jobs");
    let job = jobs
        .iter()
        .find(|job| job.job_id == "fresh-backfill-mismatch-job")
        .expect("mismatch job");
    assert_eq!(
        job.status,
        project_store::AgentEmbeddingBackfillStatus::Skipped
    );
    assert_eq!(
        job.diagnostic
            .as_ref()
            .and_then(|diagnostic| diagnostic.get("code"))
            .and_then(serde_json::Value::as_str),
        Some("agent_embedding_backfill_source_hash_mismatch")
    );
}

#[test]
fn lancedb_freshness_phase8_embedding_backfill_skips_non_current_project_records() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let stale_path = "src/backfill_stale.rs";
    let missing_path = "src/backfill_missing.rs";
    let superseded_path = "src/backfill_superseded.rs";
    let stale_text = "freshcontract embedding backfill should skip stale project records.";
    let missing_text = "freshcontract embedding backfill should skip source-missing records.";
    let superseded_old_text =
        "freshcontract embedding backfill should skip superseded old records.";
    write_repo_file(&repo_root, stale_path, "pub fn stale_backfill() {}\n");
    write_repo_file(
        &repo_root,
        superseded_path,
        "pub fn superseded_backfill() {}\n",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-backfill-stale-record",
        "freshcontract backfill stale",
        stale_text,
        vec![stale_path.into()],
        "2026-05-03T12:21:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-backfill-missing-record",
        "freshcontract backfill missing",
        missing_text,
        vec![missing_path.into()],
        "2026-05-03T12:22:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-backfill-superseded-old",
        "freshcontract backfill superseded",
        superseded_old_text,
        vec![superseded_path.into()],
        "2026-05-03T12:23:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-backfill-superseded-new",
        "freshcontract backfill superseded",
        "freshcontract embedding backfill superseding record stays current.",
        vec![superseded_path.into()],
        "2026-05-03T12:24:00Z",
    );
    write_repo_file(
        &repo_root,
        stale_path,
        "pub fn stale_backfill() { /* changed */ }\n",
    );

    for (job_id, source_id, source_hash, created_at) in [
        (
            "fresh-backfill-stale-job",
            "fresh-backfill-stale-record",
            project_store::project_record_text_hash(stale_text),
            "2026-05-03T12:25:00Z",
        ),
        (
            "fresh-backfill-missing-job",
            "fresh-backfill-missing-record",
            project_store::project_record_text_hash(missing_text),
            "2026-05-03T12:26:00Z",
        ),
        (
            "fresh-backfill-superseded-job",
            "fresh-backfill-superseded-old",
            project_store::project_record_text_hash(superseded_old_text),
            "2026-05-03T12:27:00Z",
        ),
    ] {
        project_store::enqueue_agent_embedding_backfill_job(
            &repo_root,
            &project_store::NewAgentEmbeddingBackfillJobRecord {
                job_id: job_id.into(),
                project_id: project_id.clone(),
                source_kind: project_store::AgentEmbeddingBackfillSourceKind::ProjectRecord,
                source_id: source_id.into(),
                source_hash,
                embedding_model: project_store::DEFAULT_AGENT_EMBEDDING_MODEL.into(),
                embedding_dimension: project_store::AGENT_RETRIEVAL_EMBEDDING_DIM,
                embedding_version: project_store::DEFAULT_AGENT_EMBEDDING_VERSION.into(),
                created_at: created_at.into(),
            },
        )
        .expect("enqueue non-current backfill job");
    }

    let run = project_store::run_agent_embedding_backfill_jobs(
        &repo_root,
        &project_id,
        10,
        "2026-05-03T12:28:00Z",
    )
    .expect("run non-current backfill jobs");
    assert_eq!(run.queued_count, 3);
    assert_eq!(run.skipped_count, 3);
    assert_eq!(run.succeeded_count, 0);

    let jobs = project_store::list_agent_embedding_backfill_jobs(&repo_root, &project_id)
        .expect("list backfill jobs");
    let stale_job = backfill_job_by_id(&jobs, "fresh-backfill-stale-job");
    assert_eq!(
        backfill_diagnostic_str(stale_job, "code"),
        Some("agent_embedding_backfill_source_not_fresh")
    );
    assert_eq!(
        backfill_diagnostic_str(stale_job, "freshnessState"),
        Some("stale")
    );
    assert!(backfill_diagnostic_str(stale_job, "staleReason")
        .expect("stale reason")
        .contains(stale_path));

    let missing_job = backfill_job_by_id(&jobs, "fresh-backfill-missing-job");
    assert_eq!(
        backfill_diagnostic_str(missing_job, "freshnessState"),
        Some("source_missing")
    );

    let superseded_job = backfill_job_by_id(&jobs, "fresh-backfill-superseded-job");
    assert_eq!(
        backfill_diagnostic_str(superseded_job, "freshnessState"),
        Some("superseded")
    );
    assert_eq!(
        backfill_diagnostic_str(superseded_job, "supersededById"),
        Some("fresh-backfill-superseded-new")
    );
}

#[test]
fn lancedb_freshness_phase8_embedding_backfill_skips_stale_approved_memory() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let source_path = "src/backfill_memory_source.rs";
    let initial_hash = write_repo_file(&repo_root, source_path, "pub fn memory_backfill() {}\n");
    seed_agent_run_for_agent(
        &repo_root,
        &project_id,
        "fresh-backfill-memory-run",
        RuntimeAgentIdDto::Engineer,
    );
    let file_change = project_store::append_agent_file_change(
        &repo_root,
        &project_store::NewAgentFileChangeRecord {
            project_id: project_id.clone(),
            run_id: "fresh-backfill-memory-run".into(),
            path: source_path.into(),
            operation: "write".into(),
            old_hash: None,
            new_hash: Some(initial_hash),
            created_at: "2026-05-03T12:29:00Z".into(),
        },
    )
    .expect("record memory source file change");
    let memory_text = "freshcontract embedding backfill should skip stale approved memory.";
    project_store::insert_agent_memory(
        &repo_root,
        &project_store::NewAgentMemoryRecord {
            memory_id: "fresh-backfill-stale-memory".into(),
            project_id: project_id.clone(),
            agent_session_id: None,
            scope: project_store::AgentMemoryScope::Project,
            kind: project_store::AgentMemoryKind::ProjectFact,
            text: memory_text.into(),
            review_state: project_store::AgentMemoryReviewState::Approved,
            enabled: true,
            confidence: Some(91),
            source_run_id: Some("fresh-backfill-memory-run".into()),
            source_item_ids: vec![format!("agent_file_changes:{}", file_change.id)],
            diagnostic: None,
            created_at: "2026-05-03T12:30:00Z".into(),
        },
    )
    .expect("insert stale-memory source");
    write_repo_file(
        &repo_root,
        source_path,
        "pub fn memory_backfill() { /* changed */ }\n",
    );
    project_store::enqueue_agent_embedding_backfill_job(
        &repo_root,
        &project_store::NewAgentEmbeddingBackfillJobRecord {
            job_id: "fresh-backfill-stale-memory-job".into(),
            project_id: project_id.clone(),
            source_kind: project_store::AgentEmbeddingBackfillSourceKind::ApprovedMemory,
            source_id: "fresh-backfill-stale-memory".into(),
            source_hash: project_store::agent_memory_text_hash(memory_text),
            embedding_model: project_store::DEFAULT_AGENT_EMBEDDING_MODEL.into(),
            embedding_dimension: project_store::AGENT_RETRIEVAL_EMBEDDING_DIM,
            embedding_version: project_store::DEFAULT_AGENT_EMBEDDING_VERSION.into(),
            created_at: "2026-05-03T12:31:00Z".into(),
        },
    )
    .expect("enqueue stale memory backfill job");

    let run = project_store::run_agent_embedding_backfill_jobs(
        &repo_root,
        &project_id,
        5,
        "2026-05-03T12:32:00Z",
    )
    .expect("run stale memory backfill job");
    assert_eq!(run.queued_count, 1);
    assert_eq!(run.skipped_count, 1);
    assert_eq!(run.succeeded_count, 0);
    let jobs = project_store::list_agent_embedding_backfill_jobs(&repo_root, &project_id)
        .expect("list memory backfill jobs");
    let job = backfill_job_by_id(&jobs, "fresh-backfill-stale-memory-job");
    assert_eq!(
        backfill_diagnostic_str(job, "code"),
        Some("agent_embedding_backfill_source_not_fresh")
    );
    assert_eq!(
        backfill_diagnostic_str(job, "freshnessState"),
        Some("stale")
    );
    assert!(backfill_diagnostic_str(job, "staleReason")
        .expect("stale memory reason")
        .contains(source_path));
}

#[test]
fn lancedb_freshness_phase9_project_store_matrix_covers_schema_insert_list_update_and_supersession()
{
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let related_path = "src/phase9_store.rs";
    write_repo_file(&repo_root, related_path, "pub fn phase9_store() {}\n");

    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-phase9-store-old",
        "freshcontract phase9 store matrix",
        "freshcontract phase9 store matrix old durable fact.",
        vec![related_path.into()],
        "2026-05-03T12:33:00Z",
    );
    let inserted = project_store::list_project_records(&repo_root, &project_id)
        .expect("list inserted records")
        .into_iter()
        .find(|record| record.record_id == "fresh-phase9-store-old")
        .expect("inserted project record");
    assert_eq!(inserted.freshness_state, "current");
    assert_eq!(
        inserted.freshness_checked_at.as_deref(),
        Some("2026-05-03T12:33:00Z")
    );
    assert!(inserted.fact_key.is_some());
    assert!(inserted.supersedes_id.is_none());
    assert!(inserted.superseded_by_id.is_none());
    let fingerprints: serde_json::Value =
        serde_json::from_str(&inserted.source_fingerprints_json).expect("source fingerprints json");
    assert_eq!(fingerprints["schemaVersion"], 1);
    assert_eq!(
        fingerprints["fingerprints"][0]["path"].as_str(),
        Some(related_path)
    );
    assert!(fingerprints["fingerprints"][0]["hash"].as_str().is_some());

    write_repo_file(
        &repo_root,
        related_path,
        "pub fn phase9_store() { /* changed */ }\n",
    );
    let summary = project_store::refresh_project_record_freshness_for_ids(
        &repo_root,
        &project_id,
        &["fresh-phase9-store-old".into()],
        "2026-05-03T12:34:00Z",
    )
    .expect("refresh selected project record");
    assert_eq!(summary.inspected_count, 1);
    assert_eq!(summary.stale_count, 1);
    let stale = project_store::list_project_records(&repo_root, &project_id)
        .expect("list stale records")
        .into_iter()
        .find(|record| record.record_id == "fresh-phase9-store-old")
        .expect("stale project record");
    assert_eq!(stale.freshness_state, "stale");
    assert_eq!(
        stale.invalidated_at.as_deref(),
        Some("2026-05-03T12:34:00Z")
    );
    assert!(stale
        .stale_reason
        .as_deref()
        .is_some_and(|reason| reason.contains(related_path)));

    let superseded_path = "src/phase9_store_supersession.rs";
    write_repo_file(
        &repo_root,
        superseded_path,
        "pub fn phase9_store_supersession() {}\n",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-phase9-store-superseded",
        "freshcontract phase9 store supersession",
        "freshcontract phase9 store supersession old durable fact.",
        vec![superseded_path.into()],
        "2026-05-03T12:35:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-phase9-store-superseding",
        "freshcontract phase9 store supersession",
        "freshcontract phase9 store supersession new durable fact.",
        vec![superseded_path.into()],
        "2026-05-03T12:36:00Z",
    );
    let records =
        project_store::list_project_records(&repo_root, &project_id).expect("list supersession");
    let old_stale = records
        .iter()
        .find(|record| record.record_id == "fresh-phase9-store-old")
        .expect("stale record");
    assert_eq!(
        old_stale.freshness_state, "stale",
        "freshness updates keep changed-source rows stale until a correction explicitly supersedes them"
    );
    let superseded = records
        .iter()
        .find(|record| record.record_id == "fresh-phase9-store-superseded")
        .expect("superseded record");
    let superseding = records
        .iter()
        .find(|record| record.record_id == "fresh-phase9-store-superseding")
        .expect("superseding record");
    assert_eq!(superseding.freshness_state, "current");
    assert_eq!(
        superseded.freshness_state, "superseded",
        "a newer accepted current row with the same fact key supersedes the older current row"
    );
    assert_eq!(
        superseded.superseded_by_id.as_deref(),
        Some("fresh-phase9-store-superseding")
    );
    assert_eq!(superseded.fact_key, superseding.fact_key);
}

#[test]
fn lancedb_freshness_phase9_retrieval_ranks_current_rows_ahead_of_stale_rows_when_relevance_matches(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let stale_path = "src/phase9_rank_stale.rs";
    let current_path = "src/phase9_rank_current.rs";
    write_repo_file(&repo_root, stale_path, "pub fn phase9_rank_stale() {}\n");
    write_repo_file(
        &repo_root,
        current_path,
        "pub fn phase9_rank_current() {}\n",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-phase9-rank-stale",
        "freshcontract phase9 ranking tie",
        "freshcontract phase9 ranking tie equal keyword body.",
        vec![stale_path.into()],
        "2026-05-03T12:36:00Z",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-phase9-rank-current",
        "freshcontract phase9 ranking tie",
        "freshcontract phase9 ranking tie equal keyword body.",
        vec![current_path.into()],
        "2026-05-03T12:35:00Z",
    );
    write_repo_file(
        &repo_root,
        stale_path,
        "pub fn phase9_rank_stale() { /* changed */ }\n",
    );

    let response = search_context(
        &repo_root,
        &project_id,
        "fresh-phase9-ranking-search",
        "freshcontract phase9 ranking tie equal keyword body",
        project_store::AgentRetrievalSearchScope::ProjectRecords,
    );
    let current_rank = result_by_source(&response, "fresh-phase9-rank-current").rank;
    let stale_rank = result_by_source(&response, "fresh-phase9-rank-stale").rank;
    assert!(
        current_rank < stale_rank,
        "current context should outrank stale context when relevance is otherwise equal"
    );
    assert_eq!(
        freshness_state(result_by_source(&response, "fresh-phase9-rank-current")),
        Some("current")
    );
    assert_eq!(
        freshness_state(result_by_source(&response, "fresh-phase9-rank-stale")),
        Some("stale")
    );
}

#[test]
fn lancedb_freshness_phase9_project_context_direct_reads_include_stale_evidence_and_exclude_blocked(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    seed_agent_run_for_agent(
        &repo_root,
        &project_id,
        "fresh-phase9-direct-run",
        RuntimeAgentIdDto::Debug,
    );
    let runtime = AutonomousToolRuntime::new(&repo_root)
        .expect("tool runtime")
        .with_runtime_run_controls(control_state_for_agent(RuntimeAgentIdDto::Debug))
        .with_agent_run_context(
            &project_id,
            project_store::DEFAULT_AGENT_SESSION_ID,
            "fresh-phase9-direct-run",
        );

    let record_path = "src/phase9_direct_record.rs";
    write_repo_file(
        &repo_root,
        record_path,
        "pub fn phase9_direct_record() {}\n",
    );
    seed_project_record(
        &repo_root,
        &project_id,
        "fresh-phase9-direct-record",
        "freshcontract phase9 direct record",
        "freshcontract phase9 direct project record evidence.",
        vec![record_path.into()],
        "2026-05-03T12:37:00Z",
    );
    write_repo_file(
        &repo_root,
        record_path,
        "pub fn phase9_direct_record() { /* changed */ }\n",
    );
    let mut request =
        AutonomousProjectContextRequest::new(AutonomousProjectContextAction::GetProjectRecord);
    request.record_id = Some("fresh-phase9-direct-record".into());
    let output = execute_project_context(&runtime, request);
    let record = output.record.expect("direct project record");
    assert_eq!(record.trust["freshnessState"], "stale");
    assert!(record.trust["staleReason"]
        .as_str()
        .is_some_and(|reason| reason.contains(record_path)));
    assert_eq!(
        record.trust["sourceFingerprints"][0]["path"].as_str(),
        Some(record_path)
    );

    project_store::insert_project_record(
        &repo_root,
        &project_store::NewProjectRecordRecord {
            record_id: "fresh-phase9-direct-blocked".into(),
            project_id: project_id.clone(),
            record_kind: project_store::ProjectRecordKind::Diagnostic,
            runtime_agent_id: RuntimeAgentIdDto::Debug,
            agent_definition_id: "debug".into(),
            agent_definition_version: project_store::BUILTIN_AGENT_DEFINITION_VERSION,
            agent_session_id: Some(project_store::DEFAULT_AGENT_SESSION_ID.into()),
            run_id: "fresh-phase9-direct-blocked-run".into(),
            workflow_run_id: None,
            workflow_step_id: None,
            title: "freshcontract phase9 direct blocked".into(),
            summary: "Blocked direct records stay excluded.".into(),
            text: "freshcontract phase9 direct blocked secret.".into(),
            content_json: Some(json!({"diagnostic": "blocked"})),
            schema_name: Some("xero.test.phase9.blocked".into()),
            schema_version: 1,
            importance: project_store::ProjectRecordImportance::High,
            confidence: Some(0.9),
            tags: vec!["freshness-phase9".into()],
            source_item_ids: vec!["test:phase9-blocked".into()],
            related_paths: Vec::new(),
            produced_artifact_refs: Vec::new(),
            redaction_state: project_store::ProjectRecordRedactionState::Blocked,
            visibility: project_store::ProjectRecordVisibility::Diagnostic,
            created_at: "2026-05-03T12:38:00Z".into(),
        },
    )
    .expect("insert blocked direct record");
    let mut blocked_request =
        AutonomousProjectContextRequest::new(AutonomousProjectContextAction::GetProjectRecord);
    blocked_request.record_id = Some("fresh-phase9-direct-blocked".into());
    assert!(
        runtime
            .execute(AutonomousToolRequest::ProjectContext(blocked_request))
            .is_err(),
        "blocked records must not be directly exposed through the project context tool"
    );
}

#[test]
fn lancedb_freshness_phase9_direct_memory_read_preserves_review_state_while_annotating_staleness() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let source_path = "src/phase9_direct_memory.rs";
    let initial_hash = write_repo_file(&repo_root, source_path, "pub fn phase9_memory() {}\n");
    seed_agent_run_for_agent(
        &repo_root,
        &project_id,
        "fresh-phase9-memory-source-run",
        RuntimeAgentIdDto::Engineer,
    );
    let file_change = project_store::append_agent_file_change(
        &repo_root,
        &project_store::NewAgentFileChangeRecord {
            project_id: project_id.clone(),
            run_id: "fresh-phase9-memory-source-run".into(),
            path: source_path.into(),
            operation: "write".into(),
            old_hash: None,
            new_hash: Some(initial_hash),
            created_at: "2026-05-03T12:39:00Z".into(),
        },
    )
    .expect("record memory source file change");
    project_store::insert_agent_memory(
        &repo_root,
        &project_store::NewAgentMemoryRecord {
            memory_id: "fresh-phase9-direct-memory".into(),
            project_id: project_id.clone(),
            agent_session_id: None,
            scope: project_store::AgentMemoryScope::Project,
            kind: project_store::AgentMemoryKind::ProjectFact,
            text: "freshcontract phase9 direct approved memory evidence.".into(),
            review_state: project_store::AgentMemoryReviewState::Approved,
            enabled: true,
            confidence: Some(93),
            source_run_id: Some("fresh-phase9-memory-source-run".into()),
            source_item_ids: vec![format!("agent_file_changes:{}", file_change.id)],
            diagnostic: None,
            created_at: "2026-05-03T12:40:00Z".into(),
        },
    )
    .expect("insert phase9 approved memory");
    write_repo_file(
        &repo_root,
        source_path,
        "pub fn phase9_memory() { /* changed */ }\n",
    );
    seed_agent_run_for_agent(
        &repo_root,
        &project_id,
        "fresh-phase9-direct-memory-run",
        RuntimeAgentIdDto::Ask,
    );
    let runtime = AutonomousToolRuntime::new(&repo_root)
        .expect("tool runtime")
        .with_runtime_run_controls(control_state_for_agent(RuntimeAgentIdDto::Ask))
        .with_agent_run_context(
            &project_id,
            project_store::DEFAULT_AGENT_SESSION_ID,
            "fresh-phase9-direct-memory-run",
        );
    let mut request =
        AutonomousProjectContextRequest::new(AutonomousProjectContextAction::GetMemory);
    request.memory_id = Some("fresh-phase9-direct-memory".into());
    let output = execute_project_context(&runtime, request);
    let memory = output.memory.expect("direct approved memory");
    assert_eq!(memory.trust["freshnessState"], "stale");
    assert_eq!(
        memory.trust["sourceFingerprints"][0]["path"].as_str(),
        Some(source_path)
    );

    let stored =
        project_store::get_agent_memory(&repo_root, &project_id, "fresh-phase9-direct-memory")
            .expect("load direct memory after freshness refresh");
    assert_eq!(
        stored.review_state,
        project_store::AgentMemoryReviewState::Approved
    );
    assert!(stored.enabled);
    assert_eq!(stored.freshness_state, "stale");
}
