//! End-to-end tests for multi-agent workflow run execution: the start
//! command, the reconcile engine advancing runs to terminal states, human
//! checkpoints pausing/resuming, and the background driver pushing
//! `workflow_run:updated` events.

use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use git2::Repository;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use tauri::{Listener, Manager};
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        self, CreateWorkflowDefinitionRequestDto, GetWorkflowRunRequestDto,
        ResumeWorkflowCheckpointRequestDto, StartWorkflowRunRequestDto, WorkflowConditionDto,
        WorkflowDefinitionDto, WorkflowDeliveryStateEntityTypeDto, WorkflowEdgeDto,
        WorkflowEdgeTypeDto, WorkflowHumanCheckpointTypeDto, WorkflowNodeDto,
        WorkflowNodeRunStatusDto, WorkflowRunDto, WorkflowRunPolicyDto, WorkflowRunStatusDto,
        WorkflowStateQueryDto, WorkflowTerminalStatusDto, WORKFLOW_RUN_UPDATED_EVENT,
    },
    db::{self, project_store},
    git::repository::CanonicalRepository,
    runtime::{workflow_orchestrator::driver, AgentProviderConfig, OpenAiCompatibleProviderConfig},
    state::{DesktopState, ImportFailpoints},
};

static SHARED_ROOT: OnceLock<TempDir> = OnceLock::new();

const GSD_AUTO_DEFINITION_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../test-fixtures/workflows/gsd_auto.definition.json"
));
const GSD_AUTO_LLM_RESPONSE_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../test-fixtures/workflows/gsd_auto.llm-responses.json"
));

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowLlmResponseFixtureSet {
    schema: String,
    responses: Vec<WorkflowLlmResponseFixture>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowLlmResponseFixture {
    node_title: String,
    agent_role: WorkflowFixtureAgentRole,
    expected_calls: usize,
    content: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorkflowFixtureAgentRole {
    Ask,
    Plan,
    Engineer,
    Debug,
    Generalist,
}

impl WorkflowFixtureAgentRole {
    fn turn_count(self) -> usize {
        match self {
            Self::Ask | Self::Generalist => 1,
            Self::Plan => 3,
            Self::Engineer | Self::Debug => 5,
        }
    }
}

struct WorkflowFixtureProviderServer {
    base_url: String,
    observed_titles: Arc<Mutex<Vec<String>>>,
    server_error: Arc<Mutex<Option<String>>>,
    handle: thread::JoinHandle<Result<Vec<String>, String>>,
}

impl WorkflowFixtureProviderServer {
    fn start(fixture_set: WorkflowLlmResponseFixtureSet) -> Self {
        assert_eq!(
            fixture_set.schema, "xero.workflow_llm_response_fixtures.v1",
            "unexpected Workflow LLM response fixture schema"
        );
        let expected_node_call_count = fixture_set
            .responses
            .iter()
            .map(|fixture| fixture.expected_calls)
            .sum::<usize>();
        let expected_request_count = fixture_set
            .responses
            .iter()
            .map(|fixture| fixture.expected_calls * fixture.agent_role.turn_count())
            .sum::<usize>()
            .saturating_add(expected_node_call_count);
        assert!(
            expected_node_call_count > 0,
            "provider fixture must expect calls"
        );

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind Workflow fixture provider");
        listener
            .set_nonblocking(true)
            .expect("make Workflow fixture provider nonblocking");
        let address = listener
            .local_addr()
            .expect("Workflow fixture provider address");
        let observed_titles = Arc::new(Mutex::new(Vec::with_capacity(expected_node_call_count)));
        let server_error = Arc::new(Mutex::new(None));
        let thread_observed_titles = Arc::clone(&observed_titles);
        let thread_server_error = Arc::clone(&server_error);
        let handle = thread::spawn(move || {
            let result = (|| -> Result<Vec<String>, String> {
                let deadline = Instant::now() + Duration::from_secs(120);
                let mut seen_titles = Vec::with_capacity(expected_node_call_count);
                let mut title_turns = BTreeMap::<String, usize>::new();
                let mut received_request_count = 0_usize;
                let mut memory_request_count = 0_usize;
                while seen_titles.len() < expected_node_call_count
                    || memory_request_count < expected_node_call_count
                {
                    let (mut stream, _) = match listener.accept() {
                        Ok(accepted) => accepted,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if Instant::now() >= deadline {
                                return Err(format!(
                                    "fixture provider received {}/{} expected requests: {:?}",
                                    received_request_count, expected_request_count, seen_titles
                                ));
                            }
                            thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        Err(error) => {
                            return Err(format!("accept fixture provider request: {error}"))
                        }
                    };
                    stream.set_nonblocking(false).map_err(|error| {
                        format!("make fixture provider stream blocking: {error}")
                    })?;
                    let request = match read_http_request(&mut stream) {
                        Ok(request) => request,
                        Err(error) => {
                            let diagnostic = json!({
                                "error": {
                                    "message": format!("Workflow fixture provider could not read the request: {error}"),
                                }
                            })
                            .to_string();
                            write_http_response(
                                &mut stream,
                                "400 Bad Request",
                                "application/json",
                                &diagnostic,
                            )?;
                            return Err(error);
                        }
                    };
                    received_request_count = received_request_count.saturating_add(1);
                    if request.contains(
                        "Extract durable memory candidates from this Xero coding-agent transcript",
                    ) {
                        memory_request_count = memory_request_count.saturating_add(1);
                        if memory_request_count > expected_node_call_count {
                            return Err(format!(
                                "fixture provider received more than {expected_node_call_count} memory extraction requests"
                            ));
                        }
                        let stream_body = format!(
                            "data: {}\n\ndata: [DONE]\n\n",
                            json!({"choices": [{"delta": {"content": "[]"}}]})
                        );
                        write_http_response(
                            &mut stream,
                            "200 OK",
                            "text/event-stream",
                            &stream_body,
                        )?;
                        continue;
                    }
                    let Some(fixture) = fixture_set.responses.iter().find(|fixture| {
                        request.contains(&format!("Current node: {}", fixture.node_title))
                    }) else {
                        let body = request_body(&request);
                        let bounded_body = body.chars().take(4_000).collect::<String>();
                        let diagnostic = json!({
                            "error": {
                                "message": "No Workflow LLM fixture matched the request.",
                                "requestBody": bounded_body,
                            }
                        })
                        .to_string();
                        write_http_response(
                            &mut stream,
                            "400 Bad Request",
                            "application/json",
                            &diagnostic,
                        )?;
                        return Err(format!(
                            "no Workflow LLM fixture matched request body: {}",
                            bounded_body
                        ));
                    };
                    let title_turn = title_turns.entry(fixture.node_title.clone()).or_default();
                    let turn_count = fixture.agent_role.turn_count();
                    let fixture_call_index = *title_turn / turn_count;
                    if fixture_call_index >= fixture.expected_calls {
                        return Err(format!(
                            "fixture `{}` received more than {} expected node calls; recent messages: {}",
                            fixture.node_title,
                            fixture.expected_calls,
                            request_message_diagnostics(&request)
                        ));
                    }
                    let turn_index = *title_turn % turn_count;
                    let (stream_body, completed_node_call) =
                        workflow_fixture_response(fixture, fixture_call_index, turn_index);
                    *title_turn = title_turn.saturating_add(1);
                    if completed_node_call {
                        seen_titles.push(fixture.node_title.clone());
                        thread_observed_titles
                            .lock()
                            .map_err(|_| "lock observed Workflow fixture titles".to_string())?
                            .push(fixture.node_title.clone());
                    }
                    write_http_response(&mut stream, "200 OK", "text/event-stream", &stream_body)?;
                }
                Ok(seen_titles)
            })();
            if let Err(error) = &result {
                *thread_server_error
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error.clone());
            }
            result
        });

        Self {
            base_url: format!("http://{address}/v1"),
            observed_titles,
            server_error,
            handle,
        }
    }

    fn diagnostics(&self) -> JsonValue {
        json!({
            "observedTitles": self
                .observed_titles
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
            "serverError": self
                .server_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        })
    }

    fn finish(self) -> Vec<String> {
        self.handle
            .join()
            .expect("join Workflow fixture provider")
            .expect("Workflow fixture provider completed")
    }
}

fn workflow_fixture_response(
    fixture: &WorkflowLlmResponseFixture,
    fixture_call_index: usize,
    turn_index: usize,
) -> (String, bool) {
    let fixture_id = fixture
        .node_title
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    let call_id = |suffix: &str| {
        format!(
            "fixture-{fixture_id}-{}-{suffix}",
            fixture_call_index.saturating_add(1)
        )
    };
    let target_path = format!(
        "src/workflow-fixture-{}.txt",
        fixture_call_index.saturating_add(1)
    );
    let tool_call = |id: String, name: &str, arguments: JsonValue| {
        json!({
            "index": 0,
            "id": id,
            "function": {
                "name": name,
                "arguments": arguments.to_string(),
            }
        })
    };

    let (delta, completed_node_call) = match (fixture.agent_role, turn_index) {
        (WorkflowFixtureAgentRole::Plan, 0) => (
            json!({
                "tool_calls": [tool_call(call_id("read"), "read", json!({
                    "path": "AGENTS.md"
                }))]
            }),
            false,
        ),
        (WorkflowFixtureAgentRole::Plan, 1) => (
            json!({
                "tool_calls": [tool_call(call_id("plan-draft"), "todo", json!({
                    "action": "upsert",
                    "id": "plan_draft",
                    "title": "Workflow fixture plan is ready",
                    "status": "completed",
                    "evidence": "The deterministic Workflow fixture supplied the required artifact plan.",
                    "phaseId": "draft",
                    "phaseTitle": "Draft"
                }))]
            }),
            false,
        ),
        (WorkflowFixtureAgentRole::Engineer, 0) => (
            json!({
                "tool_calls": [
                    tool_call(call_id("read-instructions"), "read", json!({
                        "path": "AGENTS.md"
                    })),
                    {
                        "index": 1,
                        "id": call_id("read-target"),
                        "function": {
                            "name": "read",
                            "arguments": json!({"path": target_path}).to_string()
                        }
                    }
                ]
            }),
            false,
        ),
        (WorkflowFixtureAgentRole::Engineer, 1) => (
            json!({
                "tool_calls": [tool_call(call_id("implementation-plan"), "todo", json!({
                    "action": "upsert",
                    "id": "implementation_plan",
                    "title": "Workflow fixture implementation plan",
                    "status": "completed",
                    "evidence": "The target and verification contract were inspected.",
                    "phaseId": "plan",
                    "phaseTitle": "Plan"
                }))]
            }),
            false,
        ),
        (WorkflowFixtureAgentRole::Engineer, 2) => (
            json!({
                "tool_calls": [tool_call(call_id("edit-apply"), "edit", json!({
                    "path": target_path,
                    "startLine": 1,
                    "endLine": 1,
                    "expected": "before\n",
                    "replacement": "after\n",
                    "expectedHash": "9160d4be34c8695bd172a76c7c7966587ea5a4d991ad22c87b2b91af54aa9ebb",
                    "preview": false
                }))]
            }),
            false,
        ),
        (WorkflowFixtureAgentRole::Debug, 0) => (
            json!({
                "tool_calls": [
                    tool_call(call_id("reproduction"), "todo", json!({
                        "action": "upsert",
                        "id": "reproduction_steps",
                        "title": "Reproduced fixture failure",
                        "status": "completed",
                        "evidence": "The fixture failure path was reproduced.",
                        "phaseId": "reproduce",
                        "phaseTitle": "Reproduce"
                    })),
                    {
                        "index": 1,
                        "id": call_id("read-target"),
                        "function": {
                            "name": "read",
                            "arguments": json!({"path": target_path}).to_string()
                        }
                    }
                ]
            }),
            false,
        ),
        (WorkflowFixtureAgentRole::Debug, 1) => (
            json!({
                "tool_calls": [tool_call(call_id("hypothesis"), "todo", json!({
                    "action": "upsert",
                    "id": "hypothesis",
                    "title": "Fixture hypothesis",
                    "status": "completed",
                    "evidence": "The deterministic response explains the fixture failure.",
                    "phaseId": "hypothesize",
                    "phaseTitle": "Hypothesize"
                }))]
            }),
            false,
        ),
        (WorkflowFixtureAgentRole::Debug, 2) => (
            json!({
                "tool_calls": [tool_call(call_id("edit-apply"), "edit", json!({
                    "path": target_path,
                    "startLine": 1,
                    "endLine": 1,
                    "expected": "before\n",
                    "replacement": "after\n",
                    "expectedHash": "9160d4be34c8695bd172a76c7c7966587ea5a4d991ad22c87b2b91af54aa9ebb",
                    "preview": false
                }))]
            }),
            false,
        ),
        (WorkflowFixtureAgentRole::Engineer | WorkflowFixtureAgentRole::Debug, 3) => (
            json!({
                "tool_calls": [tool_call(call_id("verification"), "command_verify", json!({
                    "argv": ["git", "diff", "--check"],
                    "timeoutMs": 5_000
                }))]
            }),
            false,
        ),
        _ => (json!({"content": fixture.content}), true),
    };
    let stream_body = format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices": [{"delta": delta}]})
    );
    (stream_body, completed_node_call)
}

fn read_http_request(stream: &mut TcpStream) -> Result<String, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("set fixture provider read timeout: {error}"))?;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    let header_end = loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| format!("read fixture provider request headers: {error}"))?;
        if read == 0 {
            return Err("fixture provider request ended before headers completed".into());
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
        if request.len() > 1024 * 1024 {
            return Err("fixture provider request headers exceeded 1 MiB".into());
        }
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .ok_or_else(|| "fixture provider request omitted Content-Length".to_string())?;
    let total_length = header_end
        .checked_add(content_length)
        .ok_or_else(|| "fixture provider request length overflowed".to_string())?;
    if total_length > 4 * 1024 * 1024 {
        return Err("fixture provider request exceeded 4 MiB".into());
    }
    while request.len() < total_length {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| format!("read fixture provider request body: {error}"))?;
        if read == 0 {
            return Err("fixture provider request body ended early".into());
        }
        request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).map_err(|error| format!("decode fixture provider request: {error}"))
}

fn request_body(request: &str) -> &str {
    request.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

fn request_message_diagnostics(request: &str) -> String {
    let Ok(payload) = serde_json::from_str::<JsonValue>(request_body(request)) else {
        return "request body was not JSON".into();
    };
    let Some(messages) = payload.get("messages").and_then(JsonValue::as_array) else {
        return "request body omitted messages".into();
    };
    serde_json::to_string(&json!({
        "messageCount": messages.len(),
        "messages": messages
            .iter()
            .map(|message| {
                let content = message
                    .get("content")
                    .and_then(JsonValue::as_str)
                    .map(|content| {
                        let character_count = content.chars().count();
                        if character_count <= 3_000 {
                            return content.to_owned();
                        }
                        let start = content.chars().take(1_500).collect::<String>();
                        let end = content
                            .chars()
                            .rev()
                            .take(1_500)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect::<String>();
                        format!("{start}…[{} characters omitted]…{end}", character_count - 3_000)
                    });
                json!({
                    "role": message.get("role"),
                    "content": content,
                    "toolCalls": message.get("tool_calls"),
                })
            })
            .collect::<Vec<_>>(),
    }))
    .unwrap_or_else(|error| format!("could not serialize messages: {error}"))
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("write fixture provider response: {error}"))
}

fn shared_root() -> &'static TempDir {
    SHARED_ROOT.get_or_init(|| TempDir::new().expect("create shared temp root"))
}

fn registry_path() -> PathBuf {
    shared_root().path().join("app-data").join("xero.db")
}

fn seed_project(suffix: &str) -> (String, PathBuf) {
    let repo_root = shared_root().path().join(format!("repo-{suffix}"));
    fs::create_dir_all(repo_root.join("src")).expect("create repo root");
    fs::write(
        repo_root.join("AGENTS.md"),
        "- Keep Workflow fixture work deterministic.\n",
    )
    .expect("seed Workflow fixture instructions");
    for phase in 1..=3 {
        fs::write(
            repo_root.join(format!("src/workflow-fixture-{phase}.txt")),
            "before\n",
        )
        .expect("seed Workflow fixture target");
    }
    Repository::init(&repo_root).expect("initialize fixture git repository");
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
    build_app_with_state(DesktopState::default())
}

fn build_app_with_state(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    xero_desktop_lib::configure_builder_with_state(
        tauri::test::mock_builder(),
        state.with_global_db_path_override(registry_path()),
    )
    .build(tauri::generate_context!())
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

fn gsd_auto_definition(project_id: &str) -> WorkflowDefinitionDto {
    let mut definition = serde_json::from_str::<WorkflowDefinitionDto>(GSD_AUTO_DEFINITION_FIXTURE)
        .expect("deserialize GSD Auto definition fixture");
    definition.project_id = project_id.into();
    definition
}

fn gsd_auto_llm_response_fixtures() -> WorkflowLlmResponseFixtureSet {
    serde_json::from_str(GSD_AUTO_LLM_RESPONSE_FIXTURE)
        .expect("deserialize GSD Auto LLM response fixtures")
}

fn fixture_call_counts<'a>(
    titles: impl IntoIterator<Item = &'a String>,
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for title in titles {
        *counts.entry(title.clone()).or_default() += 1;
    }
    counts
}

fn expected_fixture_call_counts(
    fixtures: &WorkflowLlmResponseFixtureSet,
) -> BTreeMap<String, usize> {
    fixtures
        .responses
        .iter()
        .map(|fixture| (fixture.node_title.clone(), fixture.expected_calls))
        .collect()
}

fn wait_for_workflow_status(
    repo_root: &std::path::Path,
    project_id: &str,
    run_id: &str,
    expected: WorkflowRunStatusDto,
    timeout: Duration,
    fixture_provider: Option<&WorkflowFixtureProviderServer>,
) -> WorkflowRunDto {
    let deadline = Instant::now() + timeout;
    loop {
        let current = project_store::get_workflow_run(repo_root, project_id, run_id)
            .expect("load Workflow run while waiting")
            .expect("Workflow run exists while waiting");
        if current.status == expected {
            return current;
        }
        if matches!(
            current.status,
            WorkflowRunStatusDto::Failed
                | WorkflowRunStatusDto::Cancelled
                | WorkflowRunStatusDto::Completed
        ) || current.terminal_status.is_some()
        {
            panic!(
                "Workflow reached {:?} while waiting for {:?}: {}",
                current.status,
                expected,
                serde_json::to_string_pretty(&workflow_wait_diagnostics(
                    repo_root,
                    project_id,
                    &current,
                    fixture_provider,
                ))
                .expect("serialize Workflow failure diagnostics")
            );
        }
        if Instant::now() >= deadline {
            panic!(
                "Workflow did not reach {expected:?} within {timeout:?}: {}",
                serde_json::to_string_pretty(&workflow_wait_diagnostics(
                    repo_root,
                    project_id,
                    &current,
                    fixture_provider,
                ))
                .expect("serialize Workflow timeout diagnostics")
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn workflow_wait_diagnostics(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    fixture_provider: Option<&WorkflowFixtureProviderServer>,
) -> JsonValue {
    let noteworthy_nodes = run
        .nodes
        .iter()
        .filter(|node| {
            !matches!(
                node.status,
                WorkflowNodeRunStatusDto::Pending | WorkflowNodeRunStatusDto::Succeeded
            )
        })
        .map(|node| {
            let agent_run = node.runtime_run_id.as_deref().and_then(|runtime_run_id| {
                project_store::load_agent_run(repo_root, project_id, runtime_run_id).ok()
            });
            let agent_events = agent_run
                .as_ref()
                .map(|snapshot| {
                    snapshot
                        .events
                        .iter()
                        .rev()
                        .filter_map(|event| {
                            let event_type =
                                project_store::agent_event_kind_sql_value(&event.event_kind);
                            if matches!(event_type, "tool_registry_snapshot" | "context_manifest") {
                                return None;
                            }
                            let payload = serde_json::from_str::<JsonValue>(&event.payload_json)
                                .unwrap_or(JsonValue::Null);
                            json!({
                                "type": event_type,
                                "payload": payload,
                            })
                            .into()
                        })
                        .take(8)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            json!({
                "nodeId": node.node_id,
                "nodeRunId": node.id,
                "status": node.status,
                "failureClass": node.failure_class,
                "runtimeRunId": node.runtime_run_id,
                "agentStatus": agent_run
                    .as_ref()
                    .map(|snapshot| format!("{:?}", snapshot.run.status)),
                "providerId": agent_run.as_ref().map(|snapshot| &snapshot.run.provider_id),
                "modelId": agent_run.as_ref().map(|snapshot| &snapshot.run.model_id),
                "agentError": agent_run.as_ref().and_then(|snapshot| snapshot.run.last_error.as_ref()),
                "agentEvents": agent_events,
            })
        })
        .collect::<Vec<_>>();
    let recent_events = run
        .events
        .iter()
        .rev()
        .take(30)
        .map(|event| {
            json!({
                "type": event.event_type,
                "nodeRunId": event.node_run_id,
                "event": event.event,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "status": run.status,
        "terminalStatus": run.terminal_status,
        "noteworthyNodes": noteworthy_nodes,
        "recentEvents": recent_events,
        "fixtureProvider": fixture_provider.map(WorkflowFixtureProviderServer::diagnostics),
    })
}

fn delivery_state_records(
    repo_root: &std::path::Path,
    project_id: &str,
    entity_type: WorkflowDeliveryStateEntityTypeDto,
) -> Vec<JsonValue> {
    let state = project_store::query_delivery_state(
        repo_root,
        project_id,
        &WorkflowStateQueryDto {
            entity_type,
            filters: Vec::new(),
            order_by: None,
            limit: None,
            include_archived: true,
        },
    )
    .expect("query Workflow delivery state");
    state["records"]
        .as_array()
        .expect("delivery state records")
        .clone()
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
            idempotency_key: "start-terminal-workflow".into(),
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
    let (project_id, repo_root) = seed_project("checkpoint");
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
            idempotency_key: "start-checkpoint-workflow".into(),
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

    assert_eq!(resumed.status, WorkflowRunStatusDto::Running);

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let refreshed = project_store::get_workflow_run(&repo_root, &project_id, &run.id)
            .expect("load resumed workflow run")
            .expect("resumed workflow run exists");
        if refreshed.status == WorkflowRunStatusDto::Completed {
            assert_eq!(
                refreshed.terminal_status,
                Some(WorkflowTerminalStatusDto::Success)
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "resumed workflow did not complete in time (status: {:?})",
            refreshed.status
        );
        std::thread::sleep(Duration::from_millis(50));
    }

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

#[test]
fn gsd_auto_runs_all_phases_with_fixture_llm_responses_and_archives_the_milestone() {
    let (project_id, repo_root) = seed_project("gsd-auto-fixture");
    let llm_fixtures = gsd_auto_llm_response_fixtures();
    let expected_calls = expected_fixture_call_counts(&llm_fixtures);
    let fixture_provider = WorkflowFixtureProviderServer::start(llm_fixtures);
    let app = build_app_with_state(
        DesktopState::default().with_owned_agent_provider_config_override(
            AgentProviderConfig::OpenAiCompatible(OpenAiCompatibleProviderConfig {
                provider_id: "openai_api".into(),
                model_id: "gpt-4.1".into(),
                base_url: fixture_provider.base_url.clone(),
                api_key: Some("workflow-fixture-key".into()),
                api_version: None,
                timeout_ms: 10_000,
            }),
        ),
    );
    let definition = gsd_auto_definition(&project_id);
    let validation = commands::workflows::validate_workflow_definition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        CreateWorkflowDefinitionRequestDto {
            definition: definition.clone(),
        },
    )
    .expect("validate GSD Auto Workflow");
    assert!(
        validation.diagnostics.is_empty(),
        "Rust rejected the frontend GSD Auto fixture: {}",
        serde_json::to_string_pretty(&validation).expect("serialize validation diagnostics")
    );
    let stored = create_definition(&app, definition);

    let started = commands::workflows::start_workflow_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartWorkflowRunRequestDto {
            project_id: project_id.clone(),
            workflow_id: stored.id.clone(),
            idempotency_key: "start-gsd-auto-fixture".into(),
            initial_input: Some(json!({
                "goal": "Exercise GSD Auto end to end with deterministic LLM fixtures"
            })),
        },
    )
    .expect("start GSD Auto Workflow")
    .run;
    assert_eq!(started.status, WorkflowRunStatusDto::Running);

    let paused = wait_for_workflow_status(
        &repo_root,
        &project_id,
        &started.id,
        WorkflowRunStatusDto::Paused,
        Duration::from_secs(120),
        Some(&fixture_provider),
    );
    let waiting_checkpoint = paused
        .nodes
        .iter()
        .find(|node| node.status == WorkflowNodeRunStatusDto::WaitingOnGate)
        .unwrap_or_else(|| {
            panic!(
                "GSD Auto paused without a waiting Workflow checkpoint: {}",
                serde_json::to_string_pretty(&json!({
                    "nodes": paused.nodes,
                    "recentEvents": paused.events.iter().rev().take(20).collect::<Vec<_>>(),
                }))
                .expect("serialize paused GSD diagnostics")
            )
        });
    assert_eq!(waiting_checkpoint.node_id, "next_milestone_offer");

    let seen_titles = fixture_provider.finish();
    assert_eq!(fixture_call_counts(&seen_titles), expected_calls);

    for node_id in [
        "smart_discuss",
        "phase_plan",
        "phase_execute",
        "verify_command",
        "phase_verify",
        "phase_review",
        "mark_phase_complete",
    ] {
        let succeeded_count = paused
            .nodes
            .iter()
            .filter(|node| {
                node.node_id == node_id && node.status == WorkflowNodeRunStatusDto::Succeeded
            })
            .count();
        assert_eq!(
            succeeded_count, 3,
            "GSD Auto should complete `{node_id}` once for each seeded phase"
        );
    }
    assert_eq!(
        paused
            .loop_attempts
            .iter()
            .find(|attempt| attempt.loop_key == "delivery_phase_iteration")
            .map(|attempt| attempt.attempt_count),
        Some(3)
    );

    let phases = delivery_state_records(
        &repo_root,
        &project_id,
        WorkflowDeliveryStateEntityTypeDto::DeliveryPhase,
    );
    assert_eq!(phases.len(), 3);
    assert!(phases.iter().all(|phase| phase["status"] == "complete"));
    assert_eq!(
        delivery_state_records(
            &repo_root,
            &project_id,
            WorkflowDeliveryStateEntityTypeDto::PhaseContext,
        )
        .len(),
        3
    );
    assert_eq!(
        delivery_state_records(
            &repo_root,
            &project_id,
            WorkflowDeliveryStateEntityTypeDto::PhasePlan,
        )
        .len(),
        3
    );
    assert_eq!(
        delivery_state_records(
            &repo_root,
            &project_id,
            WorkflowDeliveryStateEntityTypeDto::PhaseSummary,
        )
        .len(),
        3
    );
    assert_eq!(
        delivery_state_records(
            &repo_root,
            &project_id,
            WorkflowDeliveryStateEntityTypeDto::VerificationEvidence,
        )
        .len(),
        3
    );

    let requirements = delivery_state_records(
        &repo_root,
        &project_id,
        WorkflowDeliveryStateEntityTypeDto::Requirement,
    );
    assert_eq!(requirements.len(), 1);
    assert_eq!(requirements[0]["status"], "satisfied");
    for phase in 1..=3 {
        assert_eq!(
            fs::read_to_string(repo_root.join(format!("src/workflow-fixture-{phase}.txt")))
                .expect("read applied Workflow fixture edit"),
            "after\n",
            "the Engineer fixture must apply one guarded edit for every delivery phase"
        );
    }
    let milestones = delivery_state_records(
        &repo_root,
        &project_id,
        WorkflowDeliveryStateEntityTypeDto::Milestone,
    );
    assert_eq!(milestones.len(), 1);
    assert_eq!(milestones[0]["status"], "archived");
    assert_eq!(
        delivery_state_records(
            &repo_root,
            &project_id,
            WorkflowDeliveryStateEntityTypeDto::MilestoneArchive,
        )
        .len(),
        1
    );

    let resumed = commands::workflows::resume_workflow_checkpoint(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeWorkflowCheckpointRequestDto {
            project_id: project_id.clone(),
            run_id: paused.id.clone(),
            node_run_id: waiting_checkpoint.id.clone(),
            decision: "finish".into(),
            payload: None,
        },
    )
    .expect("finish GSD Auto Workflow")
    .run;
    assert_eq!(resumed.status, WorkflowRunStatusDto::Running);

    let completed = wait_for_workflow_status(
        &repo_root,
        &project_id,
        &paused.id,
        WorkflowRunStatusDto::Completed,
        Duration::from_secs(10),
        None,
    );
    assert_eq!(
        completed.terminal_status,
        Some(WorkflowTerminalStatusDto::Success)
    );
    assert!(completed.events.iter().any(|event| {
        event.event_type == "workflow_completed"
            && event.event["terminalStatus"] == JsonValue::String("success".into())
    }));
}
