//! Integration coverage for the usage aggregation queries and the
//! `get_project_usage_summary` Tauri command. Builds a real on-disk repo,
//! seeds two runs across two different models, and asserts the rolled-up
//! totals + per-model breakdown match what the SQL is supposed to return.

use std::{fs, path::Path};

use git2::{IndexAddOption, Repository, Signature};
use tauri::Manager;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{get_project_usage_summary::get_project_usage_summary, ProjectIdRequestDto},
    configure_builder_with_state,
    db::{self, project_store},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};

const PROJECT_ID: &str = "project-usage-summary";
const REPO_ID: &str = "repo-usage-summary";
const SESSION_ID: &str = project_store::DEFAULT_AGENT_SESSION_ID;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("build mock app")
}

fn create_state(root: &TempDir) -> DesktopState {
    DesktopState::default()
        .with_global_db_path_override(root.path().join("app-data").join("xero.db"))
}

fn seed_project(root: &TempDir, app: &tauri::App<tauri::test::MockRuntime>) -> std::path::PathBuf {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(&repo_root).expect("create repo dir");
    fs::write(repo_root.join("README.md"), "# usage test\n").expect("seed readme");

    let git_repository = Repository::init(&repo_root).expect("init git");
    let mut index = git_repository.index().expect("index");
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .expect("stage");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write tree");
    let tree = git_repository.find_tree(tree_id).expect("tree");
    let signature = Signature::now("Xero", "xero@example.com").expect("sig");
    git_repository
        .commit(Some("HEAD"), &signature, &signature, "init", &tree, &[])
        .expect("commit");

    let canonical = fs::canonicalize(&repo_root).expect("canonical");
    let canonical_string = canonical.to_string_lossy().into_owned();
    let repository = CanonicalRepository {
        project_id: PROJECT_ID.into(),
        repository_id: REPO_ID.into(),
        root_path: canonical.clone(),
        root_path_string: canonical_string.clone(),
        common_git_dir: canonical.join(".git"),
        display_name: "repo".into(),
        branch_name: None,
        head_sha: None,
        branch: None,
        last_commit: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
        additions: 0,
        deletions: 0,
    };
    let registry_path = app
        .state::<DesktopState>()
        .global_db_path(&app.handle().clone())
        .expect("registry path");
    db::configure_project_database_paths(&registry_path);
    db::import_project(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("import project");

    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: PROJECT_ID.into(),
            repository_id: REPO_ID.into(),
            root_path: canonical_string,
        }],
    )
    .expect("persist registry");

    canonical
}

fn seed_run(repo_root: &Path, run_id: &str, provider_id: &str, model_id: &str, started_at: &str) {
    project_store::insert_agent_run(
        repo_root,
        &project_store::NewAgentRunRecord {
            runtime_agent_id: xero_desktop_lib::commands::RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            project_id: PROJECT_ID.into(),
            agent_session_id: SESSION_ID.into(),
            run_id: run_id.into(),
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            prompt: "test".into(),
            system_prompt: "You are Xero.".into(),
            now: started_at.into(),
        },
    )
    .expect("insert run");
}

#[allow(clippy::too_many_arguments)]
fn seed_usage(
    repo_root: &Path,
    run_id: &str,
    provider_id: &str,
    model_id: &str,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    cost_micros: u64,
    updated_at: &str,
) {
    project_store::upsert_agent_usage(
        repo_root,
        &project_store::AgentUsageRecord {
            project_id: PROJECT_ID.into(),
            run_id: run_id.into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: project_store::BUILTIN_AGENT_DEFINITION_VERSION,
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output + cache_read + cache_write,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_write,
            estimated_cost_micros: cost_micros,
            updated_at: updated_at.into(),
        },
    )
    .expect("upsert usage");
}

#[test]
fn project_with_no_runs_returns_zeroed_totals() {
    let root = tempfile::tempdir().expect("tempdir");
    let state = create_state(&root);
    let app = build_mock_app(state);
    let _ = seed_project(&root, &app);

    let response = get_project_usage_summary(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: PROJECT_ID.into(),
        },
    )
    .expect("usage summary");

    assert_eq!(response.project_id, PROJECT_ID);
    assert_eq!(response.totals.run_count, 0);
    assert_eq!(response.totals.total_tokens, 0);
    assert_eq!(response.totals.estimated_cost_micros, 0);
    assert!(response.by_model.is_empty());
    assert!(response.totals.last_updated_at.is_none());
}

#[test]
fn aggregates_totals_and_breakdown_across_models() {
    let root = tempfile::tempdir().expect("tempdir");
    let state = create_state(&root);
    let app = build_mock_app(state);
    let repo_root = seed_project(&root, &app);

    // Two runs on Anthropic Sonnet
    seed_run(
        &repo_root,
        "run-a1",
        "anthropic",
        "claude-sonnet-4-6",
        "2026-04-26T10:00:00Z",
    );
    seed_usage(
        &repo_root,
        "run-a1",
        "anthropic",
        "claude-sonnet-4-6",
        100_000,
        50_000,
        20_000,
        5_000,
        2_000_000, // $2.00
        "2026-04-26T10:05:00Z",
    );
    seed_run(
        &repo_root,
        "run-a2",
        "anthropic",
        "claude-sonnet-4-6",
        "2026-04-26T11:00:00Z",
    );
    seed_usage(
        &repo_root,
        "run-a2",
        "anthropic",
        "claude-sonnet-4-6",
        50_000,
        25_000,
        10_000,
        2_500,
        1_000_000, // $1.00
        "2026-04-26T11:10:00Z",
    );

    // One run on OpenAI Codex (cheaper model)
    seed_run(
        &repo_root,
        "run-b1",
        "openai_codex",
        "gpt-5.1",
        "2026-04-26T12:00:00Z",
    );
    seed_usage(
        &repo_root,
        "run-b1",
        "openai_codex",
        "gpt-5.1",
        200_000,
        100_000,
        0,
        0,
        100_000, // $0.10
        "2026-04-26T12:01:00Z",
    );

    let response = get_project_usage_summary(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: PROJECT_ID.into(),
        },
    )
    .expect("usage summary");

    assert_eq!(response.project_id, PROJECT_ID);
    assert_eq!(response.totals.run_count, 3);
    assert_eq!(response.totals.input_tokens, 350_000);
    assert_eq!(response.totals.output_tokens, 175_000);
    assert_eq!(response.totals.cache_read_tokens, 30_000);
    assert_eq!(response.totals.cache_creation_tokens, 7_500);
    assert_eq!(response.totals.estimated_cost_micros, 3_100_000);
    assert_eq!(
        response.totals.last_updated_at.as_deref(),
        Some("2026-04-26T12:01:00Z")
    );

    // Breakdown is sorted by cost descending — Anthropic ($3.00 combined)
    // should outrank Codex ($0.10).
    assert_eq!(response.by_model.len(), 2);
    let top = &response.by_model[0];
    assert_eq!(top.provider_id, "anthropic");
    assert_eq!(top.model_id, "claude-sonnet-4-6");
    assert_eq!(top.run_count, 2);
    assert_eq!(top.estimated_cost_micros, 3_000_000);
    assert_eq!(top.input_tokens, 150_000);
    assert_eq!(top.output_tokens, 75_000);

    let second = &response.by_model[1];
    assert_eq!(second.provider_id, "openai_codex");
    assert_eq!(second.model_id, "gpt-5.1");
    assert_eq!(second.run_count, 1);
    assert_eq!(second.estimated_cost_micros, 100_000);
}

#[test]
fn unknown_project_id_yields_project_not_found() {
    let root = tempfile::tempdir().expect("tempdir");
    let state = create_state(&root);
    let app = build_mock_app(state);

    let result = get_project_usage_summary(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: "does-not-exist".into(),
        },
    );

    let error = result.expect_err("missing project should error");
    assert_eq!(error.code, "project_not_found");
}
