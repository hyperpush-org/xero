use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use rusqlite::Connection;
use serde_json::json;
use tempfile::TempDir;
use xero_desktop_lib::{
    db::{self, project_store},
    runtime::{
        discover_plugin_roots, parse_plugin_manifest, AutonomousPluginRoot,
        AutonomousSkillCacheStore, AutonomousSkillRuntime, AutonomousSkillRuntimeConfig,
        AutonomousSkillSource, AutonomousSkillSourceError, AutonomousSkillSourceFileRequest,
        AutonomousSkillSourceFileResponse, AutonomousSkillSourceTreeRequest,
        AutonomousSkillSourceTreeResponse, AutonomousSkillToolStatus, AutonomousToolOutput,
        AutonomousToolRequest, AutonomousToolRuntime, FilesystemAutonomousSkillCacheStore,
        XeroPluginRoot, XeroSkillSourceKind, XeroSkillSourceLocator, XeroSkillSourceRecord,
        XeroSkillSourceScope, XeroSkillSourceState, XeroSkillToolAccessStatus, XeroSkillToolInput,
        XeroSkillTrustState,
    },
};

#[derive(Default)]
struct NoopSkillSource;

impl AutonomousSkillSource for NoopSkillSource {
    fn list_tree(
        &self,
        _request: &AutonomousSkillSourceTreeRequest,
    ) -> Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError> {
        Ok(AutonomousSkillSourceTreeResponse {
            entries: Vec::new(),
        })
    }

    fn fetch_file(
        &self,
        request: &AutonomousSkillSourceFileRequest,
    ) -> Result<AutonomousSkillSourceFileResponse, AutonomousSkillSourceError> {
        Err(AutonomousSkillSourceError::Status {
            status: 404,
            message: request.path.clone(),
        })
    }
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
            ("project-1", "Project", "Plugin source test project"),
        )
        .expect("seed project row");
}

fn skill_runtime(root: &TempDir) -> AutonomousSkillRuntime {
    AutonomousSkillRuntime::with_source_and_cache(
        AutonomousSkillRuntimeConfig::default(),
        Arc::new(NoopSkillSource),
        Arc::new(FilesystemAutonomousSkillCacheStore::new(
            root.path().join("skill-cache"),
        )) as Arc<dyn AutonomousSkillCacheStore>,
    )
}

fn runtime_with_plugin_root(
    root: &TempDir,
    repo_root: &Path,
    plugin_root: &Path,
) -> AutonomousToolRuntime {
    AutonomousToolRuntime::new(repo_root)
        .expect("tool runtime")
        .with_skill_tool_config(
            "project-1",
            skill_runtime(root),
            Vec::new(),
            Vec::new(),
            false,
            false,
            vec![AutonomousPluginRoot {
                root_id: "team-plugins".into(),
                root_path: plugin_root.to_path_buf(),
            }],
        )
}

fn write_plugin(
    plugin_root: &Path,
    plugin_id: &str,
    name: &str,
    trust: &str,
    skill_id: &str,
    command_id: Option<&str>,
    invalid_asset: bool,
) -> PathBuf {
    let plugin_dir = plugin_root.join(plugin_id.replace('.', "-"));
    let skill_dir = plugin_dir.join("skills").join(skill_id);
    fs::create_dir_all(&skill_dir).expect("create plugin skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {skill_id}\ndescription: {name} skill.\nuser-invocable: true\n---\n\n# {skill_id}\n"),
    )
    .expect("write plugin skill");
    if invalid_asset {
        fs::write(skill_dir.join("preview.png"), b"not model-safe text").expect("write bad asset");
    } else {
        fs::write(skill_dir.join("guide.md"), "# Guide\n").expect("write guide");
    }

    let commands = command_id
        .map(|id| {
            let command_dir = plugin_dir.join("commands");
            fs::create_dir_all(&command_dir).expect("create command dir");
            let entry = format!("commands/{id}.js");
            fs::write(
                plugin_dir.join(&entry),
                "export default function run() {}\n",
            )
            .expect("write command entry");
            vec![json!({
                "id": id,
                "label": "Open Panel",
                "description": "Opens the plugin panel.",
                "entry": entry,
                "availability": "project_open"
            })]
        })
        .unwrap_or_default();

    let manifest = json!({
        "schemaVersion": 1,
        "id": plugin_id,
        "name": name,
        "version": "1.2.3",
        "description": format!("{name} plugin."),
        "trustDeclaration": trust,
        "skills": [
            {
                "id": skill_id,
                "path": format!("skills/{skill_id}")
            }
        ],
        "commands": commands
    });
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    fs::write(
        plugin_dir.join("xero-plugin.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest json"),
    )
    .expect("write manifest");
    plugin_dir
}

fn list_skill_output(
    runtime: &AutonomousToolRuntime,
    query: Option<&str>,
    include_unavailable: bool,
) -> xero_desktop_lib::runtime::AutonomousSkillToolOutput {
    let result = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::List {
            query: query.map(str::to_owned),
            include_unavailable,
            limit: Some(20),
        }))
        .expect("list plugin skills");
    match result.output {
        AutonomousToolOutput::Skill(output) => output,
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn plugin_manifest_validation_accepts_strict_valid_manifests_and_rejects_bad_shapes() {
    let root = tempfile::tempdir().expect("temp dir");
    let plugin_root = root.path().join("plugins");
    let plugin_dir = write_plugin(
        &plugin_root,
        "com.acme.tools",
        "Acme Tools",
        "trusted",
        "review-kit",
        Some("open-panel"),
        false,
    );
    let manifest_path = plugin_dir.join("xero-plugin.json");
    let manifest_bytes = fs::read(&manifest_path).expect("read manifest");

    let manifest =
        parse_plugin_manifest(&manifest_bytes, &plugin_dir).expect("valid plugin manifest");
    assert_eq!(manifest.id, "com.acme.tools");
    assert_eq!(manifest.skills[0].id, "review-kit");
    assert_eq!(manifest.commands[0].id, "open-panel");

    let missing_id = json!({
        "schemaVersion": 1,
        "name": "Missing Id",
        "version": "1.0.0",
        "description": "Missing id.",
        "trustDeclaration": "trusted"
    });
    let error = parse_plugin_manifest(&serde_json::to_vec(&missing_id).expect("json"), &plugin_dir)
        .expect_err("missing id should fail");
    assert_eq!(error.code, "xero_plugin_manifest_invalid");

    let duplicate_skill = json!({
        "schemaVersion": 1,
        "id": "com.acme.tools",
        "name": "Duplicate Skill",
        "version": "1.0.0",
        "description": "Duplicate skill.",
        "trustDeclaration": "trusted",
        "skills": [
            { "id": "review-kit", "path": "skills/review-kit" },
            { "id": "review-kit", "path": "skills/review-kit" }
        ]
    });
    let error = parse_plugin_manifest(
        &serde_json::to_vec(&duplicate_skill).expect("json"),
        &plugin_dir,
    )
    .expect_err("duplicate skills should fail");
    assert_eq!(error.code, "xero_plugin_manifest_duplicate_id");

    let bad_version = json!({
        "schemaVersion": 1,
        "id": "com.acme.bad-version",
        "name": "Bad Version",
        "version": "1",
        "description": "Bad version.",
        "trustDeclaration": "trusted"
    });
    let error = parse_plugin_manifest(
        &serde_json::to_vec(&bad_version).expect("json"),
        &plugin_dir,
    )
    .expect_err("bad version should fail");
    assert_eq!(error.code, "xero_plugin_version_invalid");

    let unknown_field = json!({
        "schemaVersion": 1,
        "id": "com.acme.unknown",
        "name": "Unknown",
        "version": "1.0.0",
        "description": "Unknown field.",
        "trustDeclaration": "trusted",
        "extra": true
    });
    let error = parse_plugin_manifest(
        &serde_json::to_vec(&unknown_field).expect("json"),
        &plugin_dir,
    )
    .expect_err("unknown fields should fail");
    assert_eq!(error.code, "xero_plugin_manifest_invalid");

    let path_escape = json!({
        "schemaVersion": 1,
        "id": "com.acme.escape",
        "name": "Escape",
        "version": "1.0.0",
        "description": "Path escape.",
        "trustDeclaration": "trusted",
        "skills": [
            { "id": "escape", "path": "../escape" }
        ]
    });
    let error = parse_plugin_manifest(
        &serde_json::to_vec(&path_escape).expect("json"),
        &plugin_dir,
    )
    .expect_err("path escape should fail");
    assert_eq!(error.code, "xero_plugin_path_outside_root");
}

#[test]
fn plugin_registry_persists_disables_enables_removes_marks_stale_and_projects_commands() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root = root.path().join("repo");
    fs::create_dir_all(&repo_root).expect("repo root");
    init_project_state(&repo_root);
    let plugin_root = root.path().join("plugins");
    write_plugin(
        &plugin_root,
        "com.acme.tools",
        "Acme Tools",
        "trusted",
        "review-kit",
        Some("open-panel"),
        false,
    );

    let discovery = discover_plugin_roots(vec![XeroPluginRoot {
        root_id: "team-plugins".into(),
        root_path: plugin_root.clone(),
    }])
    .expect("discover plugins");
    assert_eq!(discovery.plugins.len(), 1);
    assert!(discovery.diagnostics.is_empty());

    let records = project_store::sync_discovered_plugins(&repo_root, &discovery.plugins, true)
        .expect("persist discovered plugins");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, XeroSkillSourceState::Enabled);

    let commands = project_store::plugin_command_descriptors(&records, false)
        .expect("project enabled plugin commands");
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0].command_id,
        "plugin:com.acme.tools:command:open-panel"
    );

    let source = XeroSkillSourceRecord::new(
        XeroSkillSourceScope::project("project-1").expect("project scope"),
        XeroSkillSourceLocator::Plugin {
            plugin_id: "com.acme.tools".into(),
            contribution_id: "review-kit".into(),
            skill_path: "skills/review-kit".into(),
            skill_id: "review-kit".into(),
        },
        XeroSkillSourceState::Enabled,
        XeroSkillTrustState::Trusted,
    )
    .expect("plugin source record");
    let source_id = source.source_id.clone();
    project_store::upsert_installed_skill(
        &repo_root,
        project_store::InstalledSkillRecord {
            source,
            skill_id: "review-kit".into(),
            name: "review-kit".into(),
            description: "Review kit.".into(),
            user_invocable: Some(true),
            cache_key: None,
            local_location: Some(
                plugin_root
                    .join("com-acme-tools")
                    .join("skills/review-kit")
                    .to_string_lossy()
                    .into_owned(),
            ),
            version_hash: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
            installed_at: "2026-04-25T12:00:00Z".into(),
            updated_at: "2026-04-25T12:00:00Z".into(),
            last_used_at: None,
            last_diagnostic: None,
        },
    )
    .expect("persist plugin skill");

    let disabled = project_store::set_installed_plugin_enabled(&repo_root, "com.acme.tools", false)
        .expect("disable plugin");
    assert_eq!(disabled.state, XeroSkillSourceState::Disabled);
    let disabled_records =
        project_store::list_installed_plugins(&repo_root).expect("list disabled plugins");
    assert!(
        project_store::plugin_command_descriptors(&disabled_records, false)
            .expect("enabled commands")
            .is_empty()
    );
    assert_eq!(
        project_store::plugin_command_descriptors(&disabled_records, true)
            .expect("all commands")
            .len(),
        1
    );
    let disabled_skill = project_store::load_installed_skill_by_source_id(&repo_root, &source_id)
        .expect("load disabled plugin skill")
        .expect("plugin skill");
    assert_eq!(disabled_skill.source.state, XeroSkillSourceState::Disabled);

    let enabled = project_store::set_installed_plugin_enabled(&repo_root, "com.acme.tools", true)
        .expect("enable plugin");
    assert_eq!(enabled.state, XeroSkillSourceState::Enabled);
    let enabled_skill = project_store::load_installed_skill_by_source_id(&repo_root, &source_id)
        .expect("load enabled plugin skill")
        .expect("plugin skill");
    assert_eq!(enabled_skill.source.state, XeroSkillSourceState::Enabled);

    let removed = project_store::mark_installed_plugin_removed(&repo_root, "com.acme.tools")
        .expect("remove plugin");
    assert_eq!(removed.state, XeroSkillSourceState::Stale);
    assert_eq!(
        removed
            .last_diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("xero_plugin_removed")
    );
    let removed_records =
        project_store::list_installed_plugins(&repo_root).expect("list removed plugin");
    assert!(
        project_store::plugin_command_descriptors(&removed_records, false)
            .expect("enabled commands after remove")
            .is_empty()
    );
    let removed_skill = project_store::load_installed_skill_by_source_id(&repo_root, &source_id)
        .expect("load removed plugin skill")
        .expect("plugin skill");
    assert_eq!(removed_skill.source.state, XeroSkillSourceState::Stale);

    fs::remove_file(plugin_root.join("com-acme-tools").join("xero-plugin.json"))
        .expect("remove manifest");
    let stale_records =
        project_store::sync_discovered_plugins(&repo_root, &[], true).expect("mark stale plugins");
    assert_eq!(stale_records[0].state, XeroSkillSourceState::Stale);
    assert_eq!(
        stale_records[0]
            .last_diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("xero_plugin_source_missing")
    );
}

#[test]
fn plugin_command_registry_uses_stable_ids_and_reports_duplicate_plugin_conflicts() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root = root.path().join("repo");
    fs::create_dir_all(&repo_root).expect("repo root");
    init_project_state(&repo_root);
    let plugin_root = root.path().join("plugins");
    write_plugin(
        &plugin_root,
        "com.acme.tools",
        "Acme Tools",
        "trusted",
        "review-kit",
        Some("open-panel"),
        false,
    );
    write_plugin(
        &plugin_root,
        "com.other.tools",
        "Other Tools",
        "trusted",
        "ship-kit",
        Some("open-panel"),
        false,
    );
    let duplicate_dir = write_plugin(
        &plugin_root,
        "com.acme.duplicate",
        "Acme Duplicate",
        "trusted",
        "duplicate-kit",
        Some("open-panel"),
        false,
    );
    let duplicate_manifest_path = duplicate_dir.join("xero-plugin.json");
    let mut duplicate_manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&duplicate_manifest_path).expect("read duplicate plugin manifest"),
    )
    .expect("decode duplicate plugin manifest");
    duplicate_manifest["id"] = json!("com.acme.tools");
    fs::write(
        &duplicate_manifest_path,
        serde_json::to_vec_pretty(&duplicate_manifest).expect("encode duplicate plugin manifest"),
    )
    .expect("rewrite duplicate plugin manifest");

    let discovery = discover_plugin_roots(vec![XeroPluginRoot {
        root_id: "team-plugins".into(),
        root_path: plugin_root,
    }])
    .expect("discover plugin roots");
    assert_eq!(discovery.plugins.len(), 2);
    assert!(discovery
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "xero_plugin_duplicate_id"));

    let records = project_store::sync_discovered_plugins(&repo_root, &discovery.plugins, true)
        .expect("persist discovered plugins");
    let commands =
        project_store::plugin_command_descriptors(&records, false).expect("project commands");
    let command_ids = commands
        .iter()
        .map(|command| command.command_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(command_ids.len(), 2);
    assert!(command_ids.contains("plugin:com.acme.tools:command:open-panel"));
    assert!(command_ids.contains("plugin:com.other.tools:command:open-panel"));
}

#[test]
fn skill_tool_projects_plugin_skills_trust_disable_and_asset_diagnostics() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root = root.path().join("repo");
    fs::create_dir_all(&repo_root).expect("repo root");
    init_project_state(&repo_root);
    let plugin_root = root.path().join("plugins");
    write_plugin(
        &plugin_root,
        "com.acme.tools",
        "Acme Tools",
        "trusted",
        "review-kit",
        Some("open-panel"),
        false,
    );
    write_plugin(
        &plugin_root,
        "com.acme.untrusted",
        "Acme Untrusted",
        "untrusted",
        "untrusted-kit",
        None,
        false,
    );
    write_plugin(
        &plugin_root,
        "com.acme.bad",
        "Acme Bad",
        "trusted",
        "bad-kit",
        None,
        true,
    );

    let runtime = runtime_with_plugin_root(&root, &repo_root, &plugin_root);
    let output = list_skill_output(&runtime, Some("kit"), false);
    assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "autonomous_skill_layout_unsupported"));

    let trusted = output
        .candidates
        .iter()
        .find(|candidate| candidate.skill_id == "review-kit")
        .expect("trusted plugin skill");
    assert_eq!(trusted.source_kind, XeroSkillSourceKind::Plugin);
    assert_eq!(trusted.trust, XeroSkillTrustState::Trusted);
    assert_eq!(trusted.access.status, XeroSkillToolAccessStatus::Allowed);

    let untrusted = output
        .candidates
        .iter()
        .find(|candidate| candidate.skill_id == "untrusted-kit")
        .expect("untrusted plugin skill");
    assert_eq!(untrusted.trust, XeroSkillTrustState::Untrusted);

    let invoke_untrusted = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: untrusted.source_id.clone(),
            approval_grant_id: None,
            include_supporting_assets: true,
        }))
        .expect("invoke untrusted plugin skill");
    match invoke_untrusted.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::ApprovalRequired);
            assert!(output.context.is_none());
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let invoke_trusted = runtime
        .execute(AutonomousToolRequest::Skill(XeroSkillToolInput::Invoke {
            source_id: trusted.source_id.clone(),
            approval_grant_id: None,
            include_supporting_assets: true,
        }))
        .expect("invoke trusted plugin skill");
    match invoke_trusted.output {
        AutonomousToolOutput::Skill(output) => {
            assert_eq!(output.status, AutonomousSkillToolStatus::Succeeded);
            let context = output.context.expect("trusted plugin context");
            assert_eq!(context.skill_id, "review-kit");
            assert!(context.markdown.content.contains("# review-kit"));
            assert_eq!(context.supporting_assets.len(), 1);
        }
        other => panic!("unexpected output: {other:?}"),
    }

    project_store::set_installed_plugin_enabled(&repo_root, "com.acme.tools", false)
        .expect("disable trusted plugin");
    let available = list_skill_output(&runtime, Some("review"), false);
    assert!(!available
        .candidates
        .iter()
        .any(|candidate| candidate.skill_id == "review-kit"));
    let unavailable = list_skill_output(&runtime, Some("review"), true);
    let disabled = unavailable
        .candidates
        .iter()
        .find(|candidate| candidate.skill_id == "review-kit")
        .expect("disabled plugin skill visible to diagnostics");
    assert_eq!(disabled.state, XeroSkillSourceState::Disabled);
}
