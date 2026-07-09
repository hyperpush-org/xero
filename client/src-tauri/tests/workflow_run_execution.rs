//! End-to-end tests for multi-agent workflow run execution: the start
//! command, the reconcile engine advancing runs to terminal states, human
//! checkpoints pausing/resuming, and the background driver pushing
//! `workflow_run:updated` events.

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use tauri::{Listener, Manager};
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        self, CreateWorkflowDefinitionRequestDto, GetWorkflowRunRequestDto,
        ResumeWorkflowCheckpointRequestDto, StartWorkflowRunRequestDto, WorkflowConditionDto,
        WorkflowDefinitionDto, WorkflowEdgeDto, WorkflowEdgeTypeDto,
        WorkflowHumanCheckpointTypeDto, WorkflowNodeDto, WorkflowNodeRunStatusDto,
        WorkflowRunPolicyDto, WorkflowRunStatusDto, WorkflowTerminalStatusDto,
        WORKFLOW_RUN_UPDATED_EVENT,
    },
    db::{self, project_store},
    git::repository::CanonicalRepository,
    runtime::workflow_orchestrator::driver,
    state::{DesktopState, ImportFailpoints},
};

static SHARED_ROOT: OnceLock<TempDir> = OnceLock::new();

fn shared_root() -> &'static TempDir {
    SHARED_ROOT.get_or_init(|| TempDir::new().expect("create shared temp root"))
}

fn registry_path() -> PathBuf {
    shared_root().path().join("app-data").join("xero.db")
}

fn seed_project(suffix: &str) -> (String, PathBuf) {
    let repo_root = shared_root().path().join(format!("repo-{suffix}"));
    fs::create_dir_all(repo_root.join("src")).expect("create repo root");
    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");
    let project_id = format!("project-workflow-{suffix}");
    let repository_id = format!("repo-workflow-{suffix}");
    let repository = CanonicalRepository {
        project_id: project_id.clone(),
        repository_id: repository_id.clone(),
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

    db::configure_project_database_paths(&registry_path());
    db::import_project(&repository, DesktopState::default().import_failpoints())
        .expect("import project");
    xero_desktop_lib::registry::upsert_project(
        &registry_path(),
        xero_desktop_lib::registry::RegistryProjectRecord {
            project_id: project_id.clone(),
            repository_id,
            root_path: canonical_root.to_string_lossy().into_owned(),
            is_git_repo: true,
        },
        &ImportFailpoints::default(),
    )
    .expect("register project");
    (project_id, canonical_root)
}

fn build_app() -> tauri::App<tauri::test::MockRuntime> {
    xero_desktop_lib::configure_builder_with_state(
        tauri::test::mock_builder(),
        DesktopState::default().with_global_db_path_override(registry_path()),
    )
    .build(tauri::test::mock_context(tauri::test::noop_assets()))
    .expect("build app")
}

fn terminal_node(id: &str) -> WorkflowNodeDto {
    WorkflowNodeDto::Terminal {
        id: id.into(),
        title: id.into(),
        description: String::new(),
        position: Default::default(),
        terminal_status: WorkflowTerminalStatusDto::Success,
    }
}

fn checkpoint_node(id: &str) -> WorkflowNodeDto {
    WorkflowNodeDto::HumanCheckpoint {
        id: id.into(),
        title: id.into(),
        description: String::new(),
        position: Default::default(),
        checkpoint_type: WorkflowHumanCheckpointTypeDto::Decision,
        prompt: "Approve this run?".into(),
        decision_options: vec!["approve".into(), "reject".into()],
        resume_payload_schema: None,
        state_updates: Vec::new(),
    }
}

fn edge(id: &str, from: &str, to: &str) -> WorkflowEdgeDto {
    WorkflowEdgeDto {
        id: id.into(),
        from_node_id: from.into(),
        to_node_id: to.into(),
        r#type: WorkflowEdgeTypeDto::Success,
        label: String::new(),
        priority: 10,
        condition: WorkflowConditionDto::Always,
        loop_policy: None,
    }
}

fn definition(
    project_id: &str,
    workflow_id: &str,
    name: &str,
    start_node_id: &str,
    nodes: Vec<WorkflowNodeDto>,
    edges: Vec<WorkflowEdgeDto>,
) -> WorkflowDefinitionDto {
    WorkflowDefinitionDto {
        schema: "xero.workflow_definition.v1".into(),
        id: workflow_id.into(),
        project_id: project_id.into(),
        name: name.into(),
        description: String::new(),
        version: 1,
        start_node_id: start_node_id.into(),
        nodes,
        edges,
        subgraphs: Vec::new(),
        artifact_contracts: Vec::new(),
        run_policy: WorkflowRunPolicyDto::default(),
        created_at: None,
        updated_at: None,
    }
}

fn create_definition(
    app: &tauri::App<tauri::test::MockRuntime>,
    definition: WorkflowDefinitionDto,
) -> WorkflowDefinitionDto {
    commands::workflows::create_workflow_definition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        CreateWorkflowDefinitionRequestDto { definition },
    )
    .expect("create workflow definition")
    .definition
}

#[test]
fn start_workflow_run_completes_terminal_only_workflow() {
    let (project_id, _repo_root) = seed_project("terminal");
    let app = build_app();
    let stored = create_definition(
        &app,
        definition(
            &project_id,
            "workflow-terminal",
            "Terminal only",
            "done",
            vec![terminal_node("done")],
            Vec::new(),
        ),
    );

    let run = commands::workflows::start_workflow_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartWorkflowRunRequestDto {
            project_id: project_id.clone(),
            workflow_id: stored.id.clone(),
            initial_input: None,
        },
    )
    .expect("start workflow run")
    .run;

    assert_eq!(run.status, WorkflowRunStatusDto::Completed);
    assert_eq!(
        run.terminal_status,
        Some(WorkflowTerminalStatusDto::Success)
    );
    let event_types: Vec<&str> = run
        .events
        .iter()
        .map(|event| event.event_type.as_str())
        .collect();
    assert!(event_types.contains(&"workflow_started"));
    assert!(event_types.contains(&"workflow_completed"));
}

#[test]
fn checkpoint_pauses_run_and_resume_completes_it() {
    let (project_id, _repo_root) = seed_project("checkpoint");
    let app = build_app();
    let stored = create_definition(
        &app,
        definition(
            &project_id,
            "workflow-checkpoint",
            "Checkpoint then finish",
            "gate",
            vec![checkpoint_node("gate"), terminal_node("done")],
            vec![edge("edge-1", "gate", "done")],
        ),
    );

    let run = commands::workflows::start_workflow_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartWorkflowRunRequestDto {
            project_id: project_id.clone(),
            workflow_id: stored.id.clone(),
            initial_input: None,
        },
    )
    .expect("start workflow run")
    .run;

    assert_eq!(run.status, WorkflowRunStatusDto::Paused);
    let waiting_node = run
        .nodes
        .iter()
        .find(|node| node.status == WorkflowNodeRunStatusDto::WaitingOnGate)
        .expect("checkpoint node waiting on decision");

    let resumed = commands::workflows::resume_workflow_checkpoint(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeWorkflowCheckpointRequestDto {
            project_id: project_id.clone(),
            run_id: run.id.clone(),
            node_run_id: waiting_node.id.clone(),
            decision: "approve".into(),
            payload: None,
        },
    )
    .expect("resume workflow checkpoint")
    .run;

    assert_eq!(resumed.status, WorkflowRunStatusDto::Completed);
    assert_eq!(
        resumed.terminal_status,
        Some(WorkflowTerminalStatusDto::Success)
    );

    let refreshed = commands::workflows::get_workflow_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetWorkflowRunRequestDto {
            project_id: project_id.clone(),
            run_id: run.id.clone(),
        },
    )
    .expect("get workflow run")
    .run;
    assert_eq!(refreshed.status, WorkflowRunStatusDto::Completed);
}

#[test]
fn driver_advances_queued_run_and_emits_updates() {
    let (project_id, repo_root) = seed_project("driver");
    let app = build_app();
    let stored = create_definition(
        &app,
        definition(
            &project_id,
            "workflow-driver",
            "Driver advances",
            "done",
            vec![terminal_node("done")],
            Vec::new(),
        ),
    );

    // Create the run queued without reconciling so only the driver can
    // advance it.
    let run = project_store::create_workflow_run(&repo_root, &project_id, &stored.id, None)
        .expect("create queued workflow run");
    assert_eq!(run.status, WorkflowRunStatusDto::Queued);

    let observed: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = observed.clone();
    app.listen(WORKFLOW_RUN_UPDATED_EVENT, move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            sink.lock().expect("observed lock").push(payload);
        }
    });

    driver::ensure_workflow_run_driver(&app.handle().clone(), &project_id, &run.id);

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let current = project_store::get_workflow_run(&repo_root, &project_id, &run.id)
            .expect("load workflow run")
            .expect("workflow run exists");
        if current.status == WorkflowRunStatusDto::Completed {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "driver did not complete the run in time (status: {:?})",
            current.status
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let payloads = observed.lock().expect("observed lock");
        if payloads.iter().any(|payload| {
            payload["projectId"] == serde_json::json!(project_id)
                && payload["run"]["id"] == serde_json::json!(run.id)
                && payload["run"]["status"] == serde_json::json!("completed")
        }) {
            break;
        }
        drop(payloads);
        assert!(
            Instant::now() < deadline,
            "driver did not emit a completed workflow_run:updated event"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
