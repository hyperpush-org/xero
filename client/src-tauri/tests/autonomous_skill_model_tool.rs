use std::{
    collections::BTreeSet,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::Path,
    sync::{Arc, Mutex},
    thread,
};

use rusqlite::Connection;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto, RuntimeRunControlStateDto,
    },
    db::{self, project_store},
    mcp::{
        persist_mcp_registry, McpConnectionDiagnostic, McpConnectionState, McpConnectionStatus,
        McpEnvironmentReference, McpRegistry, McpServerRecord, McpTransport,
    },
    runtime::{
        AutonomousBundledSkillRoot, AutonomousLocalSkillRoot, AutonomousPluginRoot,
        AutonomousSkillCacheStore, AutonomousSkillRuntime, AutonomousSkillRuntimeConfig,
        AutonomousSkillSource, AutonomousSkillSourceEntryKind, AutonomousSkillSourceError,
        AutonomousSkillSourceFileRequest, AutonomousSkillSourceFileResponse,
        AutonomousSkillSourceMetadata, AutonomousSkillSourceTreeEntry,
        AutonomousSkillSourceTreeRequest, AutonomousSkillSourceTreeResponse,
        AutonomousSkillToolStatus, AutonomousToolAccessAction, AutonomousToolAccessRequest,
        AutonomousToolOutput, AutonomousToolRequest, AutonomousToolRuntime,
        AutonomousToolSearchRequest, FilesystemAutonomousSkillCacheStore, ToolRegistry,
        ToolRegistryOptions, XeroSkillSourceKind, XeroSkillSourceState,
        XeroSkillToolDynamicAssetInput, XeroSkillToolInput, XeroSkillTrustState,
    },
};

#[derive(Default)]
struct FixtureSkillSource {
    state: Mutex<FixtureSkillSourceState>,
}

#[derive(Default)]
struct FixtureSkillSourceState {
    tree_response: Option<Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError>>,
    files: Vec<(
        String,
        String,
        Result<AutonomousSkillSourceFileResponse, AutonomousSkillSourceError>,
    )>,
}

impl FixtureSkillSource {
    fn set_tree_response(
        &self,
        response: Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError>,
    ) {
        self.state.lock().expect("fixture lock").tree_response = Some(response);
    }

    fn set_file(&self, repo: &str, path: &str, content: &str) {
        self.state.lock().expect("fixture lock").files.push((
            repo.into(),
            path.into(),
            Ok(AutonomousSkillSourceFileResponse {
                bytes: content.as_bytes().to_vec(),
            }),
        ));
    }
}

impl AutonomousSkillSource for FixtureSkillSource {
    fn list_tree(
        &self,
        _request: &AutonomousSkillSourceTreeRequest,
    ) -> Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError> {
        self.state
            .lock()
            .expect("fixture lock")
            .tree_response
            .clone()
            .expect("fixture tree response")
    }

    fn fetch_file(
        &self,
        request: &AutonomousSkillSourceFileRequest,
    ) -> Result<AutonomousSkillSourceFileResponse, AutonomousSkillSourceError> {
        self.state
            .lock()
            .expect("fixture lock")
            .files
            .iter()
            .find(|(repo, path, _)| repo == &request.repo && path == &request.path)
            .map(|(_, _, response)| response.clone())
            .unwrap_or_else(|| {
                Err(AutonomousSkillSourceError::Status {
                    status: 404,
                    message: format!("{}:{}", request.repo, request.path),
                })
            })
    }
}

fn runtime_controls() -> RuntimeRunControlStateDto {
    RuntimeRunControlStateDto {
        active: RuntimeRunActiveControlSnapshotDto {
            provider_profile_id: None,
            model_id: "test-model".into(),
            thinking_effort: None,
            approval_mode: RuntimeRunApprovalModeDto::Yolo,
            plan_mode_required: false,
            revision: 1,
            applied_at: "2026-04-25T00:00:00Z".into(),
        },
        pending: None,
    }
}

fn skill_runtime(root: &TempDir, source: Arc<FixtureSkillSource>) -> AutonomousSkillRuntime {
    AutonomousSkillRuntime::with_source_and_cache(
        AutonomousSkillRuntimeConfig::default(),
        source,
        Arc::new(FilesystemAutonomousSkillCacheStore::new(
            root.path().join("skill-cache"),
        )) as Arc<dyn AutonomousSkillCacheStore>,
    )
}

fn runtime_with_skills(
    root: &TempDir,
    source: Arc<FixtureSkillSource>,
    local_root: &Path,
    bundled_root: &Path,
) -> AutonomousToolRuntime {
    AutonomousToolRuntime::new(root.path())
        .expect("tool runtime")
        .with_skill_tool(
            "project-1",
            skill_runtime(root, source),
            vec![AutonomousBundledSkillRoot {
                bundle_id: "xero".into(),
                version: "2026.04.25".into(),
                root_path: bundled_root.to_path_buf(),
            }],
            vec![AutonomousLocalSkillRoot {
                root_id: "personal".into(),
                root_path: local_root.to_path_buf(),
            }],
        )
}

fn runtime_with_bundled_version(
    root: &TempDir,
    source: Arc<FixtureSkillSource>,
    bundled_root: &Path,
    version: &str,
) -> AutonomousToolRuntime {
    AutonomousToolRuntime::new(root.path())
        .expect("tool runtime")
        .with_skill_tool(
            "project-1",
            skill_runtime(root, source),
            vec![AutonomousBundledSkillRoot {
                bundle_id: "xero".into(),
                version: version.into(),
                root_path: bundled_root.to_path_buf(),
            }],
            Vec::new(),
        )
}

fn write_skill(root: &Path, directory: &str, name: &str, description: &str) {
    let skill_dir = root.join(directory);
    fs::create_dir_all(&skill_dir).expect("create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
    )
    .expect("write skill");
    fs::write(skill_dir.join("guide.md"), "# Guide\n").expect("write guide");
}

fn init_project_state(repo_root: &Path) {
    db::configure_project_database_paths(&repo_root.join("app-data").join("xero.db"));
    let database_path = db::database_path_for_repo(repo_root);
    fs::create_dir_all(database_path.parent().expect("project state parent"))
        .expect("create project state dir");
    let mut connection = Connection::open(database_path).expect("open project state db");
    xero_desktop_lib::db::migrations::migrations()
        .to_latest(&mut connection)
        .expect("migrate project state db");
    connection
        .execute(
            "INSERT OR IGNORE INTO projects (id, name, description) VALUES (?1, ?2, ?3)",
            ("project-1", "Project", "SkillTool test project"),
        )
        .expect("seed project row");
}

fn github_source_metadata(skill_id: &str, tree_hash: &str) -> AutonomousSkillSourceMetadata {
    AutonomousSkillSourceMetadata {
        repo: "vercel-labs/skills".into(),
        path: format!("skills/{skill_id}"),
        reference: "main".into(),
        tree_hash: tree_hash.into(),
    }
}

fn standard_skill_tree(skill_id: &str, tree_hash: &str) -> AutonomousSkillSourceTreeResponse {
    AutonomousSkillSourceTreeResponse {
        entries: vec![
            AutonomousSkillSourceTreeEntry {
                path: format!("skills/{skill_id}"),
                kind: AutonomousSkillSourceEntryKind::Tree,
                hash: tree_hash.into(),
                bytes: None,
            },
            AutonomousSkillSourceTreeEntry {
                path: format!("skills/{skill_id}/SKILL.md"),
                kind: AutonomousSkillSourceEntryKind::Blob,
                hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                bytes: Some(80),
            },
            AutonomousSkillSourceTreeEntry {
                path: format!("skills/{skill_id}/guide.md"),
                kind: AutonomousSkillSourceEntryKind::Blob,
                hash: "cccccccccccccccccccccccccccccccccccccccc".into(),
                bytes: Some(20),
            },
        ],
    }
}

fn connected_mcp_connection() -> McpConnectionState {
    McpConnectionState {
        status: McpConnectionStatus::Connected,
        diagnostic: None,
        last_checked_at: Some("2026-04-25T00:00:00Z".into()),
        last_healthy_at: Some("2026-04-25T00:00:00Z".into()),
    }
}

fn unavailable_mcp_connection(status: McpConnectionStatus, code: &str) -> McpConnectionState {
    McpConnectionState {
        status,
        diagnostic: Some(McpConnectionDiagnostic {
            code: code.into(),
            message: format!("MCP server is unavailable for test code {code}."),
            retryable: true,
        }),
        last_checked_at: Some("2026-04-25T00:00:00Z".into()),
        last_healthy_at: None,
    }
}

fn mcp_server_record(
    id: &str,
    name: &str,
    url: &str,
    connection: McpConnectionState,
) -> McpServerRecord {
    McpServerRecord {
        id: id.into(),
        name: name.into(),
        transport: McpTransport::Http { url: url.into() },
        env: Vec::new(),
        cwd: None,
        connection,
        updated_at: "2026-04-25T00:00:00Z".into(),
    }
}

fn persist_mcp_servers(path: &Path, servers: Vec<McpServerRecord>) {
    persist_mcp_registry(
        path,
        &McpRegistry {
            version: 1,
            servers,
            updated_at: "2026-04-25T00:00:00Z".into(),
        },
    )
    .expect("persist MCP registry");
}

fn spawn_mcp_skill_server() -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mcp skill server");
    let address = listener.local_addr().expect("mcp skill server address");
    thread::spawn(move || {
        for _ in 0..48 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let body = read_http_request_body(&mut stream);
            let value: serde_json::Value =
                serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({}));
            let method = value
                .get("method")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let id = value.get("id").and_then(serde_json::Value::as_i64);
            if id.is_none() {
                write!(
                    stream,
                    "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n"
                )
                .expect("write notification response");
                continue;
            }
            let result = match method {
                "initialize" => serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {}
                }),
                "resources/list" => serde_json::json!({
                    "resources": [
                        {
                            "uri": "skill://workspace/review-skill/SKILL.md",
                            "name": "review-skill",
                            "description": "Review skill from an MCP resource.",
                            "mimeType": "text/markdown",
                            "metadata": {
                                "xeroSkill": true,
                                "userInvocable": true
                            }
                        },
                        {
                            "uri": "file://workspace/notes.txt",
                            "name": "notes",
                            "description": "Not a skill."
                        }
                    ]
                }),
                "prompts/list" => serde_json::json!({
                    "prompts": [
                        {
                            "name": "skill:deploy-helper",
                            "description": "Deployment helper prompt.",
                            "_meta": { "xeroSkill": true }
                        },
                        {
                            "name": "generic-prompt",
                            "description": "Not a skill."
                        }
                    ]
                }),
                "resources/read" => serde_json::json!({
                    "contents": [
                        {
                            "uri": "skill://workspace/review-skill/SKILL.md",
                            "mimeType": "text/markdown",
                            "text": "---\nname: review-skill\ndescription: Review skill from MCP.\nuser-invocable: true\n---\n\n# Review skill\n"
                        },
                        {
                            "uri": "skill://workspace/review-skill/guide.md",
                            "mimeType": "text/markdown",
                            "text": "# MCP guide\n"
                        }
                    ]
                }),
                "prompts/get" => serde_json::json!({
                    "messages": [
                        {
                            "role": "user",
                            "content": {
                                "type": "text",
                                "text": "Use the deployment checklist from the MCP prompt."
                            }
                        }
                    ]
                }),
                other => serde_json::json!({ "echoed": other }),
            };
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nmcp-session-id: test-session\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
                response.len(),
                response,
            )
            .expect("write mcp skill response");
        }
    });
    format!("http://{address}/mcp")
}

fn spawn_mcp_error_server(status: u16) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mcp error server");
    let address = listener.local_addr().expect("mcp error server address");
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let _ = read_http_request_body(&mut stream);
        let body = "{\"error\":\"auth required\"}";
        write!(
            stream,
            "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        )
        .expect("write mcp error response");
    });
    format!("http://{address}/mcp")
}

fn read_http_request_body(stream: &mut impl Read) -> String {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let mut content_length = 0_usize;
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).expect("read request header");
        if bytes == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().expect("content length");
            }
        }
    }
    let mut body = vec![0_u8; content_length];
    reader.read_exact(&mut body).expect("read request body");
    String::from_utf8(body).expect("utf8 request body")
}

fn runtime_with_mcp_skills(
    root: &TempDir,
    source: Arc<FixtureSkillSource>,
    mcp_registry_path: &Path,
) -> AutonomousToolRuntime {
    AutonomousToolRuntime::new(root.path())
        .expect("runtime")
        .with_mcp_registry_path(mcp_registry_path)
        .with_skill_tool_config(
            "project-1",
            skill_runtime(root, source),
            Vec::new(),
            Vec::new(),
            false,
            false,
            Vec::new(),
        )
}

#[test]
fn owned_agent_skill_descriptor_and_tool_search_are_gated_by_skill_support() {
    let root = tempfile::tempdir().expect("temp dir");
    init_project_state(root.path());
    let controls = runtime_controls();
    let disabled = ToolRegistry::for_prompt(root.path(), "Use skills for this task.", &controls);
    assert!(!disabled.descriptor_names().contains("skill"));

    let enabled = ToolRegistry::for_prompt_with_options(
        root.path(),
        "Use skills for this task.",
        &controls,
        ToolRegistryOptions {
            skill_tool_enabled: true,
        },
    );
    assert!(enabled.descriptor_names().contains("skill"));

    let disabled_runtime = AutonomousToolRuntime::new(root.path()).expect("runtime");
    let disabled_search = disabled_runtime
        .tool_search(AutonomousToolSearchRequest {
            query: "skill".into(),
            limit: None,
        })
        .expect("search tools");
    match disabled_search.output {
        AutonomousToolOutput::ToolSearch(output) => {
            assert!(!output.matches.iter().any(|item| item.tool_name == "skill"));
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let source = Arc::new(FixtureSkillSource::default());
    source.set_tree_response(Ok(standard_skill_tree(
        "find-skills",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )));
    let enabled_runtime = AutonomousToolRuntime::new(root.path())
        .expect("runtime")
        .with_skill_tool(
            "project-1",
            skill_runtime(&root, source),
            Vec::new(),
            Vec::new(),
        );
    let enabled_search = enabled_runtime
        .tool_search(AutonomousToolSearchRequest {
            query: "skill".into(),
            limit: None,
        })
        .expect("search tools");
    match enabled_search.output {
        AutonomousToolOutput::ToolSearch(output) => {
            assert!(output
                .matches
                .iter()
                .any(|item| item.tool_name == "skill" && item.group == "skills"));
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let access = enabled_runtime
        .tool_access(AutonomousToolAccessRequest {
            action: AutonomousToolAccessAction::List,
            groups: Vec::new(),
            tools: Vec::new(),
            reason: None,
        })
        .expect("tool access list");
    match access.output {
        AutonomousToolOutput::ToolAccess(output) => {
            assert!(output
                .available_groups
                .iter()
                .any(|group| group.name == "skills"));
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn skill_tool_merges_sources_filters_trust_and_invokes_validated_context() {
    let root = tempfile::tempdir().expect("temp dir");
    init_project_state(root.path());
    let local_root = root.path().join("local-skills");
    let bundled_root = root.path().join("bundled-skills");
    write_skill(&local_root, "local-skill", "local-skill", "Local skill.");
    write_skill(
        &bundled_root,
        "bundled-skill",
        "bundled-skill",
        "Bundled skill.",
    );
    write_skill(
        &db::project_app_data_dir_for_repo(root.path()).join("skills"),
        "project-skill",
        "project-skill",
        "Project skill.",
    );

    let source = Arc::new(FixtureSkillSource::default());
    let tree_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    source.set_tree_response(Ok(standard_skill_tree("find-skills", tree_hash)));
    source.set_file(
        "vercel-labs/skills",
        "skills/find-skills/SKILL.md",
        "---\nname: find-skills\ndescription: Find skills.\n---\n\n# Find Skills\n",
    );
    source.set_file(
        "vercel-labs/skills",
        "skills/find-skills/guide.md",
        "# GitHub guide\n",
    );
    let runtime = runtime_with_skills(&root, source, &local_root, &bundled_root);

    let list = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: Some("skill".into()),
            include_unavailable: false,
            limit: Some(10),
        }))
        .expect("list skills");
    let candidates = match list.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
            output.candidates
        }
        other => panic!("unexpected output: {other:?}"),
    };
    let ids = candidates
        .iter()
        .map(|candidate| candidate.skill_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids,
        BTreeSet::from([
            "bundled-skill",
            "find-skills",
            "local-skill",
            "project-skill"
        ])
    );
    assert!(candidates
        .iter()
        .any(|candidate| candidate.source_kind == XeroSkillSourceKind::Github));

    let bundled_source_id = candidates
        .iter()
        .find(|candidate| candidate.skill_id == "bundled-skill")
        .expect("bundled candidate")
        .source_id
        .clone();
    let local_source_id = candidates
        .iter()
        .find(|candidate| candidate.skill_id == "local-skill")
        .expect("local candidate")
        .source_id
        .clone();

    let approval_required = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: local_source_id.clone(),
            approval_grant_id: None,
            include_supporting_assets: true,
        }))
        .expect("invoke local skill without approval returns a typed boundary");
    match approval_required.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::ApprovalRequired);
            assert!(output.context.is_none());
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let bundled = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: bundled_source_id,
            approval_grant_id: None,
            include_supporting_assets: true,
        }))
        .expect("invoke bundled skill");
    match bundled.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
            let context = output.context.expect("bundled context");
            assert_eq!(context.skill_id, "bundled-skill");
            assert!(context.markdown.content.contains("# bundled-skill"));
            assert_eq!(context.supporting_assets.len(), 1);
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let local = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: local_source_id.clone(),
            approval_grant_id: Some("approval-1".into()),
            include_supporting_assets: true,
        }))
        .expect("invoke approved local skill");
    match local.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
            assert_eq!(
                output.selected.as_ref().map(|candidate| candidate.trust),
                Some(XeroSkillTrustState::UserApproved)
            );
            assert_eq!(
                output.selected.as_ref().map(|candidate| candidate.state),
                Some(XeroSkillSourceState::Enabled)
            );
            assert!(output
                .context
                .expect("local context")
                .markdown
                .content
                .contains("# local-skill"));
        }
        other => panic!("unexpected output: {other:?}"),
    }
    project_store::set_installed_skill_enabled(
        root.path(),
        &local_source_id,
        false,
        "2026-04-25T01:00:00Z",
    )
    .expect("disable local skill");
    let hidden = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: Some("local".into()),
            include_unavailable: false,
            limit: Some(10),
        }))
        .expect("list visible skills");
    match hidden.output {
        AutonomousToolOutput::Skill(output) => {
            assert!(output
                .candidates
                .iter()
                .all(|candidate| candidate.skill_id != "local-skill"));
        }
        other => panic!("unexpected output: {other:?}"),
    }
    let visible_for_diagnostics = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: Some("local".into()),
            include_unavailable: true,
            limit: Some(10),
        }))
        .expect("list unavailable skills");
    match visible_for_diagnostics.output {
        AutonomousToolOutput::Skill(output) => {
            let local = output
                .candidates
                .iter()
                .find(|candidate| candidate.skill_id == "local-skill")
                .expect("disabled local skill appears with diagnostics requested");
            assert_eq!(local.state, XeroSkillSourceState::Disabled);
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let github_source = xero_desktop_lib::runtime::XeroSkillSourceRecord::github_autonomous(
        xero_desktop_lib::runtime::XeroSkillSourceScope::project("project-1").unwrap(),
        &github_source_metadata("find-skills", tree_hash),
        XeroSkillSourceState::Discoverable,
        XeroSkillTrustState::Trusted,
    )
    .expect("github source")
    .source_id;
    let github = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: github_source,
            approval_grant_id: None,
            include_supporting_assets: true,
        }))
        .expect("invoke github skill from discovery cache");
    match github.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
            assert_eq!(
                output.context.expect("github context").skill_id,
                "find-skills"
            );
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn skill_tool_redacts_candidate_metadata_and_source_diagnostics_before_model_projection() {
    let root = tempfile::tempdir().expect("temp dir");
    init_project_state(root.path());
    let local_root = root.path().join("local-skills");
    let bundled_root = root.path().join("bundled-skills");
    write_skill(
        &local_root,
        "leaky-skill",
        "leaky-skill",
        "Reads /Users/sn0w/.config/xero with github_pat_1234567890.",
    );

    let source = Arc::new(FixtureSkillSource::default());
    source.set_tree_response(Ok(AutonomousSkillSourceTreeResponse {
        entries: Vec::new(),
    }));
    let runtime = AutonomousToolRuntime::new(root.path())
        .expect("runtime")
        .with_skill_tool_config(
            "project-1",
            skill_runtime(&root, source),
            vec![AutonomousBundledSkillRoot {
                bundle_id: "xero".into(),
                version: "2026.04.25".into(),
                root_path: bundled_root,
            }],
            vec![AutonomousLocalSkillRoot {
                root_id: "personal".into(),
                root_path: local_root,
            }],
            true,
            true,
            vec![AutonomousPluginRoot {
                root_id: "missing-plugins".into(),
                root_path: root.path().join("missing-plugins"),
            }],
        );

    let list = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: Some("leaky".into()),
            include_unavailable: true,
            limit: Some(20),
        }))
        .expect("list redacted skills");
    match list.output {
        AutonomousToolOutput::Skill(output) => {
            let leaky = output
                .candidates
                .iter()
                .find(|candidate| candidate.skill_id == "leaky-skill")
                .expect("leaky skill candidate");
            assert!(!leaky.description.contains("/Users/sn0w"));
            assert!(!leaky.description.contains("github_pat"));
            assert!(leaky.description.contains("[redacted-path]"));
            assert!(leaky.description.contains("[redacted]"));

            let plugin_diagnostic = output
                .diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code == "xero_plugin_root_unavailable")
                .expect("plugin root diagnostic");
            assert!(plugin_diagnostic.redacted);
            assert!(!plugin_diagnostic
                .message
                .contains(root.path().to_string_lossy().as_ref()));
            assert!(plugin_diagnostic.message.contains("[redacted-path]"));
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn skill_tool_refreshes_stale_bundled_skill_before_invocation() {
    let root = tempfile::tempdir().expect("temp dir");
    init_project_state(root.path());
    let bundled_root = root.path().join("bundled-skills");
    write_skill(
        &bundled_root,
        "bundled-skill",
        "bundled-skill",
        "Bundled skill v1.",
    );

    let source = Arc::new(FixtureSkillSource::default());
    source.set_tree_response(Ok(AutonomousSkillSourceTreeResponse {
        entries: Vec::new(),
    }));
    let runtime_v1 =
        runtime_with_bundled_version(&root, source.clone(), &bundled_root, "2026.04.25");

    let list_v1 = runtime_v1
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: Some("bundled".into()),
            include_unavailable: false,
            limit: Some(10),
        }))
        .expect("list bundled skill");
    let source_id = match list_v1.output {
        AutonomousToolOutput::Skill(output) => output
            .candidates
            .iter()
            .find(|candidate| candidate.skill_id == "bundled-skill")
            .expect("bundled candidate")
            .source_id
            .clone(),
        other => panic!("unexpected output: {other:?}"),
    };

    runtime_v1
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: source_id.clone(),
            approval_grant_id: None,
            include_supporting_assets: false,
        }))
        .expect("invoke bundled v1");

    write_skill(
        &bundled_root,
        "bundled-skill",
        "bundled-skill",
        "Bundled skill v2.",
    );
    let runtime_v2 = runtime_with_bundled_version(&root, source, &bundled_root, "2026.04.26");
    let stale = runtime_v2
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: Some("bundled".into()),
            include_unavailable: false,
            limit: Some(10),
        }))
        .expect("list stale bundled skill");
    match stale.output {
        AutonomousToolOutput::Skill(output) => {
            let candidate = output
                .candidates
                .iter()
                .find(|candidate| candidate.skill_id == "bundled-skill")
                .expect("stale bundled candidate");
            assert_eq!(candidate.state, XeroSkillSourceState::Stale);
            assert_eq!(candidate.source_kind, XeroSkillSourceKind::Bundled);
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let refreshed = runtime_v2
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id,
            approval_grant_id: None,
            include_supporting_assets: false,
        }))
        .expect("refresh stale bundled skill");
    match refreshed.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
            assert!(output
                .context
                .expect("refreshed context")
                .markdown
                .content
                .contains("Bundled skill v2."));
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn skill_tool_reload_marks_changed_and_deleted_filesystem_sources_stale_idempotently() {
    let root = tempfile::tempdir().expect("temp dir");
    init_project_state(root.path());
    let bundled_root = root.path().join("bundled-skills");
    write_skill(
        &bundled_root,
        "bundled-skill",
        "bundled-skill",
        "Bundled skill v1.",
    );

    let source = Arc::new(FixtureSkillSource::default());
    source.set_tree_response(Ok(AutonomousSkillSourceTreeResponse {
        entries: Vec::new(),
    }));
    let runtime = runtime_with_bundled_version(&root, source.clone(), &bundled_root, "2026.04.25");
    let list = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: Some("bundled".into()),
            include_unavailable: false,
            limit: Some(10),
        }))
        .expect("list bundled skill");
    let source_id = match list.output {
        AutonomousToolOutput::Skill(output) => output.candidates[0].source_id.clone(),
        other => panic!("unexpected output: {other:?}"),
    };
    runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: source_id.clone(),
            approval_grant_id: None,
            include_supporting_assets: false,
        }))
        .expect("install bundled skill through invoke");
    let installed_v1 = project_store::load_installed_skill_by_source_id(root.path(), &source_id)
        .expect("load installed bundled skill")
        .expect("installed skill");
    assert_eq!(installed_v1.source.state, XeroSkillSourceState::Enabled);

    write_skill(
        &bundled_root,
        "bundled-skill",
        "bundled-skill",
        "Bundled skill v2.",
    );
    let runtime_v2 = runtime_with_bundled_version(&root, source, &bundled_root, "2026.04.25");
    let reloaded = runtime_v2
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Reload {
            source_id: Some(source_id.clone()),
            source_kind: None,
        }))
        .expect("reload changed bundled skill");
    match reloaded.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.candidates.len(), 1);
            assert_eq!(output.candidates[0].state, XeroSkillSourceState::Stale);
        }
        other => panic!("unexpected output: {other:?}"),
    }
    let changed = project_store::load_installed_skill_by_source_id(root.path(), &source_id)
        .expect("load changed bundled skill")
        .expect("changed skill");
    assert_eq!(changed.source.state, XeroSkillSourceState::Stale);
    assert_ne!(changed.version_hash, installed_v1.version_hash);
    assert_eq!(
        changed
            .last_diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("skill_source_content_changed")
    );

    let repeated = runtime_v2
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Reload {
            source_id: Some(source_id.clone()),
            source_kind: None,
        }))
        .expect("repeat reload");
    match repeated.output {
        AutonomousToolOutput::Skill(output) => assert_eq!(output.candidates.len(), 1),
        other => panic!("unexpected output: {other:?}"),
    }
    let installed = project_store::list_installed_skills(
        root.path(),
        project_store::InstalledSkillScopeFilter::project("project-1", true)
            .expect("project filter"),
    )
    .expect("list installed skills");
    assert_eq!(
        installed
            .iter()
            .filter(|record| record.source.source_id == source_id)
            .count(),
        1
    );

    fs::remove_file(bundled_root.join("bundled-skill").join("SKILL.md"))
        .expect("remove bundled skill");
    runtime_v2
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Reload {
            source_id: Some(source_id.clone()),
            source_kind: None,
        }))
        .expect("reload deleted bundled skill");
    let deleted = project_store::load_installed_skill_by_source_id(root.path(), &source_id)
        .expect("load deleted bundled skill")
        .expect("deleted skill");
    assert_eq!(deleted.source.state, XeroSkillSourceState::Stale);
    assert_eq!(
        deleted
            .last_diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("skill_source_content_missing")
    );
}

#[test]
fn skill_tool_dynamic_candidates_start_disabled_untrusted_and_non_invocable() {
    let root = tempfile::tempdir().expect("temp dir");
    init_project_state(root.path());
    let source = Arc::new(FixtureSkillSource::default());
    source.set_tree_response(Ok(AutonomousSkillSourceTreeResponse {
        entries: Vec::new(),
    }));
    let runtime = AutonomousToolRuntime::new(root.path())
        .expect("runtime")
        .with_skill_tool(
            "project-1",
            skill_runtime(&root, source),
            Vec::new(),
            Vec::new(),
        );

    let created = runtime
        .execute(AutonomousToolRequest::Skill(
            XeroSkillToolInput::CreateDynamic {
                skill_id: "dynamic-skill".into(),
                markdown:
                    "---\nname: dynamic-skill\ndescription: Dynamic skill.\n---\n\n# Dynamic\n"
                        .into(),
                supporting_assets: vec![XeroSkillToolDynamicAssetInput {
                    relative_path: "notes.md".into(),
                    content: "# Notes\n".into(),
                }],
                source_run_id: Some("run-1".into()),
                source_artifact_id: Some("artifact-1".into()),
            },
        ))
        .expect("create dynamic skill");
    let source_id = match created.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
            let selected = output.selected.expect("dynamic candidate");
            assert_eq!(selected.state, XeroSkillSourceState::Disabled);
            assert_eq!(selected.trust, XeroSkillTrustState::Untrusted);
            selected.source_id
        }
        other => panic!("unexpected output: {other:?}"),
    };

    let duplicate = runtime
        .execute(AutonomousToolRequest::Skill(
            XeroSkillToolInput::CreateDynamic {
                skill_id: "dynamic-skill".into(),
                markdown: "---\nname: dynamic-skill\ndescription: Dynamic skill updated.\n---\n\n# Dynamic\n"
                    .into(),
                supporting_assets: vec![XeroSkillToolDynamicAssetInput {
                    relative_path: "notes.md".into(),
                    content: "# Notes updated\n".into(),
                }],
                source_run_id: Some("run-1".into()),
                source_artifact_id: Some("artifact-1".into()),
            },
        ))
        .expect("merge duplicate dynamic skill");
    match duplicate.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
            assert_eq!(
                output
                    .selected
                    .as_ref()
                    .map(|candidate| candidate.source_id.as_str()),
                Some(source_id.as_str())
            );
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let hidden = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: Some("dynamic".into()),
            include_unavailable: false,
            limit: Some(10),
        }))
        .expect("list visible dynamic skills");
    match hidden.output {
        AutonomousToolOutput::Skill(output) => assert!(output.candidates.is_empty()),
        other => panic!("unexpected output: {other:?}"),
    }

    let diagnostic = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: Some("dynamic".into()),
            include_unavailable: true,
            limit: Some(10),
        }))
        .expect("list unavailable dynamic skills");
    match diagnostic.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.candidates.len(), 1);
            assert_eq!(
                output.candidates[0].source_kind,
                XeroSkillSourceKind::Dynamic
            );
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let rejected = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id,
            approval_grant_id: Some("approval-1".into()),
            include_supporting_assets: true,
        }))
        .expect("dynamic invoke returns typed failure");
    match rejected.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Failed);
            assert!(output.context.is_none());
            assert!(output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "skill_tool_source_not_enabled"));
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn skill_tool_projects_mcp_resource_and_prompt_skills_from_connected_servers_only() {
    let root = tempfile::tempdir().expect("temp dir");
    init_project_state(root.path());
    let source = Arc::new(FixtureSkillSource::default());
    source.set_tree_response(Ok(AutonomousSkillSourceTreeResponse {
        entries: Vec::new(),
    }));
    let mcp_url = spawn_mcp_skill_server();
    let mcp_registry_path = root.path().join("mcp-registry.json");
    persist_mcp_servers(
        &mcp_registry_path,
        vec![
            mcp_server_record(
                "skills-mcp",
                "Skills MCP",
                &mcp_url,
                connected_mcp_connection(),
            ),
            mcp_server_record(
                "blocked-mcp",
                "Blocked MCP",
                &mcp_url,
                unavailable_mcp_connection(McpConnectionStatus::Blocked, "blocked"),
            ),
            mcp_server_record(
                "stale-mcp",
                "Stale MCP",
                &mcp_url,
                unavailable_mcp_connection(McpConnectionStatus::Stale, "stale"),
            ),
            mcp_server_record(
                "misconfigured-mcp",
                "Misconfigured MCP",
                &mcp_url,
                unavailable_mcp_connection(McpConnectionStatus::Misconfigured, "misconfigured"),
            ),
        ],
    );
    let runtime = runtime_with_mcp_skills(&root, source, &mcp_registry_path);

    let list = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: None,
            include_unavailable: true,
            limit: Some(20),
        }))
        .expect("list mcp skills");
    match list.output {
        AutonomousToolOutput::Skill(output) => {
            let ids = output
                .candidates
                .iter()
                .map(|candidate| candidate.skill_id.as_str())
                .collect::<BTreeSet<_>>();
            assert_eq!(ids, BTreeSet::from(["deploy-helper", "review-skill"]));
            assert!(output.candidates.iter().all(|candidate| {
                candidate.source_kind == XeroSkillSourceKind::Mcp
                    && candidate.trust == XeroSkillTrustState::Trusted
                    && candidate.description.contains("Skills MCP")
                    && candidate.description.contains("skills-mcp")
            }));
            assert_eq!(
                output
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code == "skill_tool_mcp_server_unavailable")
                    .count(),
                3
            );
            assert!(output.candidates.iter().all(|candidate| !candidate
                .source_id
                .contains("blocked-mcp")
                && !candidate.source_id.contains("stale-mcp")
                && !candidate.source_id.contains("misconfigured-mcp")));
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn skill_tool_invokes_mcp_skills_and_keeps_them_out_of_installed_state() {
    let root = tempfile::tempdir().expect("temp dir");
    init_project_state(root.path());
    let source = Arc::new(FixtureSkillSource::default());
    source.set_tree_response(Ok(AutonomousSkillSourceTreeResponse {
        entries: Vec::new(),
    }));
    let mcp_url = spawn_mcp_skill_server();
    let mcp_registry_path = root.path().join("mcp-registry.json");
    persist_mcp_servers(
        &mcp_registry_path,
        vec![mcp_server_record(
            "skills-mcp",
            "Skills MCP",
            &mcp_url,
            connected_mcp_connection(),
        )],
    );
    let runtime = runtime_with_mcp_skills(&root, source, &mcp_registry_path);

    let list = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: None,
            include_unavailable: false,
            limit: Some(20),
        }))
        .expect("list mcp skills");
    let candidates = match list.output {
        AutonomousToolOutput::Skill(output) => output.candidates,
        other => panic!("unexpected output: {other:?}"),
    };
    let review_source_id = candidates
        .iter()
        .find(|candidate| candidate.skill_id == "review-skill")
        .expect("resource skill")
        .source_id
        .clone();
    let prompt_source_id = candidates
        .iter()
        .find(|candidate| candidate.skill_id == "deploy-helper")
        .expect("prompt skill")
        .source_id
        .clone();

    let resource = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: review_source_id,
            approval_grant_id: None,
            include_supporting_assets: true,
        }))
        .expect("invoke mcp resource skill");
    match resource.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
            let context = output.context.expect("resource context");
            assert_eq!(context.skill_id, "review-skill");
            assert!(context.markdown.content.contains("# Review skill"));
            assert_eq!(context.supporting_assets.len(), 1);
            assert_eq!(output.lifecycle_events.len(), 1);
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let prompt = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: prompt_source_id,
            approval_grant_id: None,
            include_supporting_assets: true,
        }))
        .expect("invoke mcp prompt skill");
    match prompt.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
            let context = output.context.expect("prompt context");
            assert_eq!(context.skill_id, "deploy-helper");
            assert!(context.markdown.content.contains("name: deploy-helper"));
            assert!(context
                .markdown
                .content
                .contains("Use the deployment checklist from the MCP prompt."));
            assert!(context.supporting_assets.is_empty());
            assert_eq!(output.lifecycle_events.len(), 1);
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let installed = project_store::list_installed_skills(
        root.path(),
        project_store::InstalledSkillScopeFilter::project("project-1", true)
            .expect("project filter"),
    )
    .expect("list installed skills");
    assert!(installed.is_empty());
}

#[test]
fn skill_tool_reports_mcp_invocation_failures_without_corrupting_installed_state() {
    let root = tempfile::tempdir().expect("temp dir");
    init_project_state(root.path());
    let source = Arc::new(FixtureSkillSource::default());
    source.set_tree_response(Ok(AutonomousSkillSourceTreeResponse {
        entries: Vec::new(),
    }));
    let mcp_url = spawn_mcp_skill_server();
    let mcp_registry_path = root.path().join("mcp-registry.json");
    persist_mcp_servers(
        &mcp_registry_path,
        vec![mcp_server_record(
            "skills-mcp",
            "Skills MCP",
            &mcp_url,
            connected_mcp_connection(),
        )],
    );
    let runtime = runtime_with_mcp_skills(&root, source, &mcp_registry_path);
    let list = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: Some("review".into()),
            include_unavailable: false,
            limit: Some(20),
        }))
        .expect("list mcp skills");
    let source_id = match list.output {
        AutonomousToolOutput::Skill(output) => output
            .candidates
            .iter()
            .find(|candidate| candidate.skill_id == "review-skill")
            .expect("resource skill")
            .source_id
            .clone(),
        other => panic!("unexpected output: {other:?}"),
    };

    persist_mcp_servers(
        &mcp_registry_path,
        vec![McpServerRecord {
            id: "skills-mcp".into(),
            name: "Skills MCP".into(),
            transport: McpTransport::Stdio {
                command: "python3".into(),
                args: Vec::new(),
            },
            env: vec![McpEnvironmentReference {
                key: "MCP_TOKEN".into(),
                from_env: "__XERO_TEST_MISSING_MCP_SKILL_TOKEN__".into(),
            }],
            cwd: None,
            connection: connected_mcp_connection(),
            updated_at: "2026-04-25T00:00:01Z".into(),
        }],
    );
    let auth_failure = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: source_id.clone(),
            approval_grant_id: None,
            include_supporting_assets: false,
        }))
        .expect("mcp auth failure is typed output");
    match auth_failure.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Failed);
            assert!(output.context.is_none());
            assert!(output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "autonomous_tool_mcp_env_missing"));
            assert_eq!(output.lifecycle_events.len(), 1);
        }
        other => panic!("unexpected output: {other:?}"),
    }

    persist_mcp_servers(
        &mcp_registry_path,
        vec![mcp_server_record(
            "skills-mcp",
            "Skills MCP",
            &mcp_url,
            unavailable_mcp_connection(McpConnectionStatus::Stale, "stale"),
        )],
    );
    let disconnected = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: source_id.clone(),
            approval_grant_id: None,
            include_supporting_assets: false,
        }))
        .expect("mcp disconnected failure is typed output");
    match disconnected.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Failed);
            assert!(output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "autonomous_tool_mcp_server_not_connected"));
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let error_url = spawn_mcp_error_server(401);
    persist_mcp_servers(
        &mcp_registry_path,
        vec![mcp_server_record(
            "skills-mcp",
            "Skills MCP",
            &error_url,
            connected_mcp_connection(),
        )],
    );
    let transport_failure = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id,
            approval_grant_id: None,
            include_supporting_assets: false,
        }))
        .expect("mcp transport failure is typed output");
    match transport_failure.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Failed);
            assert!(output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "autonomous_tool_mcp_http_status"));
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let installed = project_store::list_installed_skills(
        root.path(),
        project_store::InstalledSkillScopeFilter::project("project-1", true)
            .expect("project filter"),
    )
    .expect("list installed skills");
    assert!(installed.is_empty());
}
