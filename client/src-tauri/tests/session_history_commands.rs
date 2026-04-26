use std::{
    fs,
    path::{Path, PathBuf},
};

use cadence_desktop_lib::{
    commands::{
        export_session_transcript, get_session_context_snapshot, get_session_transcript,
        save_session_transcript_export, search_session_transcripts,
        validate_context_snapshot_contract, validate_export_payload_contract,
        validate_session_transcript_contract, ExportSessionTranscriptRequestDto,
        GetSessionContextSnapshotRequestDto, GetSessionTranscriptRequestDto,
        SaveSessionTranscriptExportRequestDto, SearchSessionTranscriptsRequestDto,
        SessionContextContributorKindDto, SessionTranscriptExportFormatDto,
        SessionTranscriptExportPayloadDto, SessionTranscriptScopeDto, SessionUsageSourceDto,
    },
    configure_builder_with_state,
    db::{self, project_store},
    git::repository::{ensure_cadence_excluded, CanonicalRepository},
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};
use git2::{IndexAddOption, Repository, Signature};
use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;

const PROVIDER_ID: &str = "openrouter";
const MODEL_ID: &str = "openai/gpt-5.4";
const SESSION_ID: &str = project_store::DEFAULT_AGENT_SESSION_ID;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> DesktopState {
    DesktopState::default()
        .with_registry_file_override(root.path().join("app-data").join("project-registry.json"))
}

fn seed_project(root: &TempDir, app: &tauri::App<tauri::test::MockRuntime>) -> (String, PathBuf) {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(repo_root.join("src")).expect("create repo src");
    fs::write(repo_root.join("src").join("tracked.txt"), "alpha\nbeta\n")
        .expect("seed tracked file");
    fs::write(
        repo_root.join("AGENTS.md"),
        "- Keep transcripts redacted.\n",
    )
    .expect("seed repo instructions");

    let git_repository = Repository::init(&repo_root).expect("init git repo");
    commit_all(&git_repository, "initial commit");

    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");
    let root_path_string = canonical_root.to_string_lossy().into_owned();
    let repository = CanonicalRepository {
        project_id: "project-session-history".into(),
        repository_id: "repo-session-history".into(),
        root_path: canonical_root.clone(),
        root_path_string: root_path_string.clone(),
        common_git_dir: canonical_root.join(".git"),
        display_name: "repo".into(),
        branch_name: current_branch_name(&canonical_root),
        head_sha: current_head_sha(&canonical_root),
        branch: None,
        last_commit: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
        additions: 0,
        deletions: 0,
    };

    ensure_cadence_excluded(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("exclude .cadence from seeded repo git status");
    db::import_project(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("import project into repo-local db");

    let registry_path = app
        .state::<DesktopState>()
        .registry_file(&app.handle().clone())
        .expect("registry path");
    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: repository.project_id.clone(),
            repository_id: repository.repository_id.clone(),
            root_path: root_path_string,
        }],
    )
    .expect("persist registry entry");

    (repository.project_id, canonical_root)
}

fn commit_all(repository: &Repository, message: &str) {
    let mut index = repository.index().expect("repo index");
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .expect("stage files");
    index.write().expect("write index");

    let tree_id = index.write_tree().expect("write tree");
    let tree = repository.find_tree(tree_id).expect("find tree");
    let signature = Signature::now("Cadence", "cadence@example.com").expect("signature");

    repository
        .commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
        .expect("commit");
}

fn current_branch_name(repo_root: &Path) -> Option<String> {
    Repository::open(repo_root).ok().and_then(|repository| {
        repository
            .head()
            .ok()
            .and_then(|head| head.shorthand().map(ToOwned::to_owned))
    })
}

fn current_head_sha(repo_root: &Path) -> Option<String> {
    Repository::open(repo_root).ok().and_then(|repository| {
        repository
            .head()
            .ok()
            .and_then(|head| head.target().map(|oid| oid.to_string()))
    })
}

#[test]
fn transcript_export_and_search_cover_active_archived_and_deleted_sessions() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    project_store::update_agent_session(
        &repo_root,
        &project_store::AgentSessionUpdateRecord {
            project_id: project_id.clone(),
            agent_session_id: SESSION_ID.into(),
            title: Some("History export session".into()),
            summary: Some("Durable validation and export coverage.".into()),
            selected: None,
        },
    )
    .expect("rename default session for transcript");

    seed_history_run(
        &repo_root,
        &project_id,
        "run-history-1",
        "2026-04-26T10:00:00Z",
        "Inspect the durable validation path.",
        Some(project_store::AgentUsageRecord {
            project_id: project_id.clone(),
            run_id: "run-history-1".into(),
            provider_id: PROVIDER_ID.into(),
            model_id: MODEL_ID.into(),
            input_tokens: 120,
            output_tokens: 40,
            total_tokens: 160,
            estimated_cost_micros: 42,
            updated_at: "2026-04-26T10:05:00Z".into(),
        }),
    );
    seed_history_run(
        &repo_root,
        &project_id,
        "run-history-2",
        "2026-04-26T11:00:00Z",
        "Summarize the completed export work.",
        None,
    );

    let transcript = get_session_transcript(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetSessionTranscriptRequestDto {
            project_id: project_id.clone(),
            agent_session_id: SESSION_ID.into(),
            run_id: None,
        },
    )
    .expect("project session transcript");
    validate_session_transcript_contract(&transcript).expect("valid session transcript");

    assert_eq!(
        transcript
            .runs
            .iter()
            .map(|run| run.run_id.as_str())
            .collect::<Vec<_>>(),
        vec!["run-history-1", "run-history-2"]
    );
    assert_eq!(transcript.usage_totals.as_ref().unwrap().total_tokens, 160);
    assert!(transcript.items.iter().any(|item| {
        item.source_table == "agent_runs"
            && item.title.as_deref() == Some("Run prompt")
            && item.text.as_deref() == Some("Inspect the durable validation path.")
    }));
    assert!(transcript.items.iter().any(|item| {
        item.source_table == "agent_checkpoints"
            && item.summary.as_deref() == Some("Validation passed after cargo test.")
    }));
    assert!(transcript.items.iter().any(|item| {
        item.source_table == "agent_file_changes"
            && item.file_path.as_deref() == Some("[redacted-path]")
    }));
    let tool_summary = transcript
        .items
        .iter()
        .find(|item| item.source_table == "agent_tool_calls")
        .and_then(|item| item.summary.as_deref())
        .expect("tool summary");
    assert!(tool_summary.contains("read ended succeeded."));
    assert!(tool_summary.contains("..."));
    assert!(tool_summary.len() < 380);

    let serialized = serde_json::to_string(&transcript).expect("serialize transcript");
    assert!(!serialized.contains("sk-history-secret"));
    assert!(!serialized.contains("/Users/sn0w/.config"));

    let context_snapshot = get_session_context_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetSessionContextSnapshotRequestDto {
            project_id: project_id.clone(),
            agent_session_id: SESSION_ID.into(),
            run_id: Some("run-history-1".into()),
            provider_id: None,
            model_id: None,
            pending_prompt: Some("Continue with final checks.".into()),
        },
    )
    .expect("run context snapshot");
    validate_context_snapshot_contract(&context_snapshot).expect("valid context snapshot");
    assert_eq!(context_snapshot.run_id.as_deref(), Some("run-history-1"));
    assert_eq!(
        context_snapshot.budget.estimation_source,
        SessionUsageSourceDto::Mixed
    );
    assert_eq!(
        context_snapshot.usage_totals.as_ref().unwrap().total_tokens,
        160
    );
    assert!(context_snapshot.budget.known_provider_budget);
    assert!(context_snapshot
        .contributors
        .iter()
        .any(|contributor| { contributor.kind == SessionContextContributorKindDto::SystemPrompt }));
    assert!(context_snapshot.contributors.iter().any(|contributor| {
        contributor.kind == SessionContextContributorKindDto::InstructionFile
            && contributor.source_id.as_deref() == Some("AGENTS.md")
    }));
    assert!(context_snapshot.contributors.iter().any(|contributor| {
        contributor.kind == SessionContextContributorKindDto::ToolDescriptor
    }));
    assert!(context_snapshot.contributors.iter().any(|contributor| {
        contributor.kind == SessionContextContributorKindDto::ConversationTail
            && contributor.label == "Pending prompt"
    }));
    assert!(context_snapshot.contributors.iter().any(|contributor| {
        contributor.kind == SessionContextContributorKindDto::ToolResult
            && contributor.label == "Tool result: read"
    }));
    assert!(context_snapshot.contributors.iter().any(|contributor| {
        contributor.kind == SessionContextContributorKindDto::ProviderUsage
            && !contributor.model_visible
            && !contributor.included
    }));
    let context_json = serde_json::to_string(&context_snapshot).expect("serialize context");
    assert!(!context_json.contains("sk-history-secret"));
    assert!(!context_json.contains("/Users/sn0w/.config"));

    let markdown_export = export_session_transcript(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ExportSessionTranscriptRequestDto {
            project_id: project_id.clone(),
            agent_session_id: SESSION_ID.into(),
            run_id: None,
            format: SessionTranscriptExportFormatDto::Markdown,
        },
    )
    .expect("markdown export");
    assert_eq!(markdown_export.mime_type, "text/markdown");
    assert!(markdown_export.content.contains("# History export session"));
    assert!(markdown_export.content.contains("## Run `run-history-1`"));
    assert!(markdown_export.content.contains("Run prompt"));
    assert!(markdown_export
        .content
        .contains("Inspect the durable validation path."));
    assert!(markdown_export
        .content
        .contains("Validation passed after cargo test."));
    assert!(!markdown_export.content.contains("sk-history-secret"));
    assert!(!markdown_export.content.contains("/Users/sn0w/.config"));

    let export_path = root.path().join("session-history-export.md");
    save_session_transcript_export(SaveSessionTranscriptExportRequestDto {
        path: export_path.to_string_lossy().into_owned(),
        content: markdown_export.content.clone(),
    })
    .expect("save transcript export");
    assert_eq!(
        fs::read_to_string(export_path).expect("read saved export"),
        markdown_export.content
    );

    let run_export = export_session_transcript(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ExportSessionTranscriptRequestDto {
            project_id: project_id.clone(),
            agent_session_id: SESSION_ID.into(),
            run_id: Some("run-history-2".into()),
            format: SessionTranscriptExportFormatDto::Json,
        },
    )
    .expect("run json export");
    let payload: SessionTranscriptExportPayloadDto =
        serde_json::from_str(&run_export.content).expect("parse json export payload");
    validate_export_payload_contract(&payload).expect("valid export payload");
    assert_eq!(payload.scope, SessionTranscriptScopeDto::Run);
    assert_eq!(payload.transcript.runs.len(), 1);
    assert!(payload
        .transcript
        .items
        .iter()
        .all(|item| item.run_id == "run-history-2"));

    let active_search = search_session_transcripts(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SearchSessionTranscriptsRequestDto {
            project_id: project_id.clone(),
            query: "validation".into(),
            agent_session_id: None,
            run_id: None,
            include_archived: false,
            limit: Some(10),
        },
    )
    .expect("active session search");
    assert!(!active_search.results.is_empty());
    assert!(active_search.results.iter().all(|result| !result.archived));
    assert!(active_search
        .results
        .iter()
        .any(|result| result.snippet.to_ascii_lowercase().contains("validation")));

    project_store::archive_agent_session(&repo_root, &project_id, SESSION_ID)
        .expect("archive session");
    let archived_transcript = get_session_transcript(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetSessionTranscriptRequestDto {
            project_id: project_id.clone(),
            agent_session_id: SESSION_ID.into(),
            run_id: None,
        },
    )
    .expect("archived transcript remains readable");
    assert!(archived_transcript.archived);
    assert!(archived_transcript.archived_at.is_some());

    let hidden_archived_search = search_session_transcripts(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SearchSessionTranscriptsRequestDto {
            project_id: project_id.clone(),
            query: "validation".into(),
            agent_session_id: None,
            run_id: None,
            include_archived: false,
            limit: Some(10),
        },
    )
    .expect("active-only search after archive");
    assert!(hidden_archived_search.results.is_empty());

    let visible_archived_search = search_session_transcripts(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SearchSessionTranscriptsRequestDto {
            project_id: project_id.clone(),
            query: "validation".into(),
            agent_session_id: None,
            run_id: None,
            include_archived: true,
            limit: Some(10),
        },
    )
    .expect("archived-inclusive search");
    assert!(visible_archived_search
        .results
        .iter()
        .any(|result| result.archived));
    let search_json = serde_json::to_string(&visible_archived_search).expect("serialize search");
    assert!(!search_json.contains("sk-history-secret"));
    assert!(!search_json.contains("/Users/sn0w/.config"));

    project_store::delete_agent_session(&repo_root, &project_id, SESSION_ID)
        .expect("delete archived session");
    let deleted_search = search_session_transcripts(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SearchSessionTranscriptsRequestDto {
            project_id,
            query: "validation".into(),
            agent_session_id: None,
            run_id: None,
            include_archived: true,
            limit: Some(10),
        },
    )
    .expect("search after session delete");
    assert!(deleted_search.results.is_empty());
}

#[test]
fn run_scoped_projection_rejects_mismatched_sessions() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let second_session = project_store::create_agent_session(
        &repo_root,
        &project_store::AgentSessionCreateRecord {
            project_id: project_id.clone(),
            title: "Parallel session".into(),
            summary: String::new(),
            selected: false,
        },
    )
    .expect("create second session");

    seed_minimal_run(
        &repo_root,
        &project_id,
        &second_session.agent_session_id,
        "run-parallel-history",
        "2026-04-26T12:00:00Z",
        "Inspect the parallel run.",
    );

    let mismatch = get_session_transcript(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetSessionTranscriptRequestDto {
            project_id: project_id.clone(),
            agent_session_id: SESSION_ID.into(),
            run_id: Some("run-parallel-history".into()),
        },
    )
    .expect_err("run scoped transcript should reject another session's run");
    assert_eq!(mismatch.code, "agent_run_session_mismatch");

    let context_mismatch = get_session_context_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetSessionContextSnapshotRequestDto {
            project_id: project_id.clone(),
            agent_session_id: SESSION_ID.into(),
            run_id: Some("run-parallel-history".into()),
            provider_id: None,
            model_id: None,
            pending_prompt: None,
        },
    )
    .expect_err("run scoped context should reject another session's run");
    assert_eq!(context_mismatch.code, "agent_run_session_mismatch");

    let transcript = get_session_transcript(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetSessionTranscriptRequestDto {
            project_id,
            agent_session_id: second_session.agent_session_id,
            run_id: Some("run-parallel-history".into()),
        },
    )
    .expect("run scoped transcript");
    assert_eq!(transcript.runs.len(), 1);
    assert!(transcript
        .items
        .iter()
        .all(|item| item.run_id == "run-parallel-history"));
}

#[test]
fn context_snapshot_handles_sessions_without_runs() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let empty_session = project_store::create_agent_session(
        &repo_root,
        &project_store::AgentSessionCreateRecord {
            project_id: project_id.clone(),
            title: "Empty context session".into(),
            summary: String::new(),
            selected: false,
        },
    )
    .expect("create empty session");

    let snapshot = get_session_context_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetSessionContextSnapshotRequestDto {
            project_id,
            agent_session_id: empty_session.agent_session_id,
            run_id: None,
            provider_id: Some(PROVIDER_ID.into()),
            model_id: Some(MODEL_ID.into()),
            pending_prompt: None,
        },
    )
    .expect("empty session context snapshot");

    validate_context_snapshot_contract(&snapshot).expect("valid empty context snapshot");
    assert!(snapshot.run_id.is_none());
    assert_eq!(snapshot.provider_id, PROVIDER_ID);
    assert_eq!(snapshot.model_id, MODEL_ID);
    assert!(snapshot.usage_totals.is_none());
    assert!(snapshot.budget.known_provider_budget);
    assert!(snapshot.contributors.iter().any(|contributor| {
        contributor.kind == SessionContextContributorKindDto::InstructionFile
            && contributor.model_visible
    }));
}

fn seed_history_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    started_at: &str,
    prompt: &str,
    usage: Option<project_store::AgentUsageRecord>,
) {
    seed_minimal_run(
        repo_root, project_id, SESSION_ID, run_id, started_at, prompt,
    );
    project_store::append_agent_message(
        repo_root,
        &project_store::NewAgentMessageRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            role: project_store::AgentMessageRole::User,
            content: "Use api_key=sk-history-secret for the fixture.".into(),
            created_at: plus_seconds(started_at, 1),
        },
    )
    .expect("append redacted user message");
    project_store::append_agent_message(
        repo_root,
        &project_store::NewAgentMessageRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            role: project_store::AgentMessageRole::Assistant,
            content: "The durable validation path is ready for export.".into(),
            created_at: plus_seconds(started_at, 2),
        },
    )
    .expect("append assistant message");
    project_store::append_agent_message(
        repo_root,
        &project_store::NewAgentMessageRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            role: project_store::AgentMessageRole::Tool,
            content: json!({
                "toolCallId": format!("{run_id}-tool-read"),
                "toolName": "read",
                "ok": true,
                "summary": "Read tracked file.",
                "output": { "stdout": "alpha\nbeta\n" }
            })
            .to_string(),
            created_at: plus_seconds(started_at, 3),
        },
    )
    .expect("append tool result message");
    project_store::append_agent_event(
        repo_root,
        &project_store::NewAgentEventRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            event_kind: project_store::AgentRunEventKind::ReasoningSummary,
            payload_json: json!({
                "summary": "Reasoning through validation ordering.",
                "detail": "Keep events chronological."
            })
            .to_string(),
            created_at: plus_seconds(started_at, 4),
        },
    )
    .expect("append reasoning event");
    project_store::start_agent_tool_call(
        repo_root,
        &project_store::AgentToolCallStartRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            tool_call_id: format!("{run_id}-tool-read"),
            tool_name: "read".into(),
            input_json: json!({ "path": "src/tracked.txt" }).to_string(),
            started_at: plus_seconds(started_at, 5),
        },
    )
    .expect("start tool call");
    project_store::finish_agent_tool_call(
        repo_root,
        &project_store::AgentToolCallFinishRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            tool_call_id: format!("{run_id}-tool-read"),
            state: project_store::AgentToolCallState::Succeeded,
            result_json: Some(json!({ "stdout": "x".repeat(800) }).to_string()),
            error: None,
            completed_at: plus_seconds(started_at, 6),
        },
    )
    .expect("finish tool call");
    project_store::append_agent_file_change(
        repo_root,
        &project_store::NewAgentFileChangeRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            path: "/Users/sn0w/.config/cadence/token.json".into(),
            operation: "write".into(),
            old_hash: None,
            new_hash: None,
            created_at: plus_seconds(started_at, 7),
        },
    )
    .expect("append file change");
    project_store::append_agent_checkpoint(
        repo_root,
        &project_store::NewAgentCheckpointRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            checkpoint_kind: "validation".into(),
            summary: "Validation passed after cargo test.".into(),
            payload_json: Some(json!({ "command": "cargo test" }).to_string()),
            created_at: plus_seconds(started_at, 8),
        },
    )
    .expect("append checkpoint");
    project_store::append_agent_action_request(
        repo_root,
        &project_store::NewAgentActionRequestRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            action_id: format!("{run_id}-approval"),
            action_type: "operator_review".into(),
            title: "Review export".into(),
            detail: "Confirm the transcript export is safe.".into(),
            created_at: plus_seconds(started_at, 9),
        },
    )
    .expect("append action request");
    if let Some(record) = usage.as_ref() {
        project_store::upsert_agent_usage(repo_root, record).expect("upsert usage");
    }

    let _ = project_store::update_agent_run_status(
        repo_root,
        project_id,
        run_id,
        project_store::AgentRunStatus::Completed,
        None,
        &plus_seconds(started_at, 10),
    )
    .expect("complete run");

    let snapshot = project_store::load_agent_run(repo_root, project_id, run_id).expect("load run");
    assert_eq!(snapshot.run.prompt, prompt);
}

fn seed_minimal_run(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: &str,
    started_at: &str,
    prompt: &str,
) {
    project_store::insert_agent_run(
        repo_root,
        &project_store::NewAgentRunRecord {
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            run_id: run_id.into(),
            provider_id: PROVIDER_ID.into(),
            model_id: MODEL_ID.into(),
            prompt: prompt.into(),
            system_prompt: "You are Cadence.".into(),
            now: started_at.into(),
        },
    )
    .expect("insert agent run");
}

fn plus_seconds(timestamp: &str, seconds: u32) -> String {
    let prefix = timestamp
        .strip_suffix("00Z")
        .expect("fixture timestamp should end in 00Z");
    format!("{prefix}{seconds:02}Z")
}
