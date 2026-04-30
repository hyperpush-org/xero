use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tempfile::TempDir;
use xero_desktop_lib::{
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    runtime::{
        discover_bundled_skill_directory, discover_local_skill_directory,
        discover_project_skill_directory, load_discovered_skill_context,
        AutonomousSkillCacheStatus, AutonomousSkillRuntime, AutonomousSkillRuntimeConfig,
        AutonomousSkillSource, AutonomousSkillSourceEntryKind, AutonomousSkillSourceError,
        AutonomousSkillSourceFileRequest, AutonomousSkillSourceFileResponse,
        AutonomousSkillSourceMetadata, AutonomousSkillSourceTreeEntry,
        AutonomousSkillSourceTreeRequest, AutonomousSkillSourceTreeResponse,
        FilesystemAutonomousSkillCacheStore, XeroSkillSourceLocator, XeroSkillSourceRecord,
        XeroSkillSourceScope, XeroSkillSourceState, XeroSkillTrustState,
    },
    state::DesktopState,
};

#[derive(Clone, Default)]
struct FixtureSkillSource {
    state: Arc<Mutex<FixtureSkillSourceState>>,
}

#[derive(Default)]
struct FixtureSkillSourceState {
    tree_response: Option<Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError>>,
    file_responses: BTreeMap<
        (String, String, String),
        Result<AutonomousSkillSourceFileResponse, AutonomousSkillSourceError>,
    >,
}

impl FixtureSkillSource {
    fn set_tree_response(
        &self,
        response: Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError>,
    ) {
        self.state
            .lock()
            .expect("fixture source lock")
            .tree_response = Some(response);
    }

    fn set_file_text(&self, repo: &str, reference: &str, path: &str, content: &str) {
        self.state
            .lock()
            .expect("fixture source lock")
            .file_responses
            .insert(
                (repo.into(), reference.into(), path.into()),
                Ok(AutonomousSkillSourceFileResponse {
                    bytes: content.as_bytes().to_vec(),
                }),
            );
    }
}

impl AutonomousSkillSource for FixtureSkillSource {
    fn list_tree(
        &self,
        _request: &AutonomousSkillSourceTreeRequest,
    ) -> Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError> {
        self.state
            .lock()
            .expect("fixture source lock")
            .tree_response
            .clone()
            .expect("fixture tree response should exist")
    }

    fn fetch_file(
        &self,
        request: &AutonomousSkillSourceFileRequest,
    ) -> Result<AutonomousSkillSourceFileResponse, AutonomousSkillSourceError> {
        self.state
            .lock()
            .expect("fixture source lock")
            .file_responses
            .get(&(
                request.repo.clone(),
                request.reference.clone(),
                request.path.clone(),
            ))
            .cloned()
            .expect("fixture file response should exist")
    }
}

fn seed_project(root: &TempDir, project_id: &str) -> PathBuf {
    db::configure_project_database_paths(&root.path().join("app-data").join("xero.db"));
    let repo_root = root.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical repo root");
    let repository = CanonicalRepository {
        project_id: project_id.into(),
        repository_id: "repo-1".into(),
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
    db::import_project(&repository, DesktopState::default().import_failpoints())
        .expect("import project");
    canonical_root
}

fn runtime_config() -> AutonomousSkillRuntimeConfig {
    AutonomousSkillRuntimeConfig {
        default_source_repo: "vercel-labs/skills".into(),
        default_source_ref: "main".into(),
        default_source_root: "skills".into(),
        github_api_base_url: "https://api.github.com".into(),
        github_token: None,
        limits: Default::default(),
    }
}

fn skill_source_metadata(skill_id: &str, tree_hash: &str) -> AutonomousSkillSourceMetadata {
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
                hash: "1111111111111111111111111111111111111111".into(),
                bytes: Some(128),
            },
            AutonomousSkillSourceTreeEntry {
                path: format!("skills/{skill_id}/guide.md"),
                kind: AutonomousSkillSourceEntryKind::Blob,
                hash: "2222222222222222222222222222222222222222".into(),
                bytes: Some(64),
            },
        ],
    }
}

fn write_skill(root: &Path, directory: &str, name: &str, description: &str) -> PathBuf {
    let skill_dir = root.join(directory);
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\nuser-invocable: false\n---\n\n# {name}\n"),
    )
    .expect("write skill markdown");
    skill_dir
}

fn registry_record(
    source: AutonomousSkillSourceMetadata,
    state: XeroSkillSourceState,
    timestamp: &str,
) -> project_store::InstalledSkillRecord {
    project_store::InstalledSkillRecord {
        source: XeroSkillSourceRecord::github_autonomous(
            XeroSkillSourceScope::global(),
            &source,
            state,
            XeroSkillTrustState::Trusted,
        )
        .expect("source record"),
        skill_id: "find-skills".into(),
        name: "find-skills".into(),
        description: "Find installable skills.".into(),
        user_invocable: Some(false),
        cache_key: Some("find-skills-cache".into()),
        local_location: Some("/tmp/xero-cache/find-skills".into()),
        version_hash: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
        installed_at: timestamp.into(),
        updated_at: timestamp.into(),
        last_used_at: None,
        last_diagnostic: None,
    }
}

#[test]
fn installed_skill_registry_persists_updates_scopes_and_rejects_corrupt_rows() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root = seed_project(&root, "project-1");
    let timestamp = "2026-04-25T12:00:00Z";
    let source = skill_source_metadata("find-skills", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    let installed = project_store::upsert_installed_skill(
        &repo_root,
        registry_record(source.clone(), XeroSkillSourceState::Enabled, timestamp),
    )
    .expect("upsert installed skill");
    assert_eq!(installed.source.state, XeroSkillSourceState::Enabled);

    let project_skill = write_skill(
        &db::project_app_data_dir_for_repo(&repo_root).join("skills"),
        "project-helper",
        "project-helper",
        "Project-local helper.",
    );
    let discovered = discover_project_skill_directory(
        "project-1",
        db::project_app_data_dir_for_repo(&repo_root),
    )
    .expect("discover project skill");
    assert_eq!(discovered.candidates.len(), 1);
    let project_record = project_store::InstalledSkillRecord::from_discovered_skill(
        &discovered.candidates[0],
        XeroSkillSourceState::Installed,
        XeroSkillTrustState::UserApproved,
        "2026-04-25T12:01:00Z",
    )
    .expect("build project installed skill");
    assert!(Path::new(
        project_record
            .local_location
            .as_ref()
            .expect("project local location")
    )
    .starts_with(
        std::fs::canonicalize(project_skill.parent().expect("project skill parent"))
            .expect("canonical project skill parent")
    ));
    project_store::upsert_installed_skill(&repo_root, project_record)
        .expect("upsert project skill");

    let global = project_store::list_installed_skills(
        &repo_root,
        project_store::InstalledSkillScopeFilter::Global,
    )
    .expect("list global skills");
    assert_eq!(global.len(), 1);
    let project_only = project_store::list_installed_skills(
        &repo_root,
        project_store::InstalledSkillScopeFilter::project("project-1", false)
            .expect("project filter"),
    )
    .expect("list project skills");
    assert_eq!(project_only.len(), 1);
    assert_eq!(project_only[0].skill_id, "project-helper");
    let project_with_global = project_store::list_installed_skills(
        &repo_root,
        project_store::InstalledSkillScopeFilter::project("project-1", true)
            .expect("project filter"),
    )
    .expect("list project plus global skills");
    assert_eq!(project_with_global.len(), 2);

    let refreshed_source =
        skill_source_metadata("find-skills", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let mut refreshed = registry_record(
        refreshed_source,
        XeroSkillSourceState::Enabled,
        "2026-04-25T12:02:00Z",
    );
    refreshed.version_hash = Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into());
    let refreshed = project_store::upsert_installed_skill(&repo_root, refreshed)
        .expect("upsert duplicate source");
    assert_eq!(
        refreshed.version_hash.as_deref(),
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
    assert_eq!(
        project_store::list_installed_skills(
            &repo_root,
            project_store::InstalledSkillScopeFilter::Global
        )
        .expect("list globals after refresh")
        .len(),
        1
    );

    let disabled = project_store::set_installed_skill_enabled(
        &repo_root,
        &installed.source.source_id,
        false,
        "2026-04-25T12:03:00Z",
    )
    .expect("disable skill");
    assert_eq!(disabled.source.state, XeroSkillSourceState::Disabled);
    let reenabled = project_store::set_installed_skill_enabled(
        &repo_root,
        &installed.source.source_id,
        true,
        "2026-04-25T12:04:00Z",
    )
    .expect("re-enable skill");
    assert_eq!(reenabled.source.state, XeroSkillSourceState::Enabled);

    assert!(
        project_store::remove_installed_skill(&repo_root, &installed.source.source_id)
            .expect("remove skill")
    );
    assert!(project_store::load_installed_skill_by_source_id(
        &repo_root,
        &installed.source.source_id
    )
    .expect("load removed skill")
    .is_none());

    let corrupt = project_store::upsert_installed_skill(
        &repo_root,
        registry_record(
            source,
            XeroSkillSourceState::Enabled,
            "2026-04-25T12:05:00Z",
        ),
    )
    .expect("upsert before corruption");
    let connection =
        rusqlite::Connection::open(database_path_for_repo(&repo_root)).expect("open state db");
    connection
        .execute(
            "UPDATE installed_skill_records SET source_json = json_set(source_json, '$.state', 'mystery') WHERE source_id = ?1",
            [corrupt.source.source_id.as_str()],
        )
        .expect("corrupt source json");
    let error = project_store::list_installed_skills(
        &repo_root,
        project_store::InstalledSkillScopeFilter::Global,
    )
    .expect_err("corrupt installed skill rows should fail closed");
    assert_eq!(error.code, "installed_skill_record_corrupt");
}

#[test]
fn installed_skill_registry_refuses_blocked_source_reenable() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root = seed_project(&root, "project-1");
    let timestamp = "2026-04-25T12:00:00Z";
    let mut record = registry_record(
        skill_source_metadata("find-skills", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        XeroSkillSourceState::Disabled,
        timestamp,
    );
    record.source.trust = XeroSkillTrustState::Blocked;
    let persisted =
        project_store::upsert_installed_skill(&repo_root, record).expect("persist blocked skill");
    assert_eq!(persisted.source.trust, XeroSkillTrustState::Blocked);

    let error = project_store::set_installed_skill_enabled(
        &repo_root,
        &persisted.source.source_id,
        true,
        "2026-04-25T12:01:00Z",
    )
    .expect_err("blocked skills cannot be re-enabled");
    assert_eq!(error.code, "installed_skill_blocked");

    let unchanged =
        project_store::load_installed_skill_by_source_id(&repo_root, &persisted.source.source_id)
            .expect("reload blocked skill")
            .expect("blocked skill");
    assert_eq!(unchanged.source.state, XeroSkillSourceState::Disabled);
    assert_eq!(unchanged.source.trust, XeroSkillTrustState::Blocked);
}

#[test]
fn github_autonomous_skill_runtime_registers_cache_hits_refreshes_and_failures() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root = seed_project(&root, "project-1");
    let cache_root = root.path().join("cache");
    let source = FixtureSkillSource::default();
    source.set_tree_response(Ok(standard_skill_tree(
        "find-skills",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )));
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/SKILL.md",
        "---\nname: find-skills\ndescription: Discover installable skills.\nuser-invocable: false\n---\n\n# Find Skills\n",
    );
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/guide.md",
        "# Guide\n",
    );

    let runtime = AutonomousSkillRuntime::with_source_and_cache(
        runtime_config(),
        Arc::new(source.clone()),
        Arc::new(FilesystemAutonomousSkillCacheStore::new(cache_root)),
    )
    .with_installed_skill_registry(Arc::new(
        project_store::ProjectStoreInstalledSkillRegistry::global(repo_root.clone()),
    ));
    let initial_source =
        skill_source_metadata("find-skills", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    let miss = runtime
        .install(xero_desktop_lib::runtime::AutonomousSkillInstallRequest {
            source: initial_source.clone(),
            timeout_ms: Some(1_000),
        })
        .expect("cache miss install");
    assert_eq!(miss.cache_status, AutonomousSkillCacheStatus::Miss);
    let hit = runtime
        .install(xero_desktop_lib::runtime::AutonomousSkillInstallRequest {
            source: initial_source.clone(),
            timeout_ms: Some(1_000),
        })
        .expect("cache hit install");
    assert_eq!(hit.cache_status, AutonomousSkillCacheStatus::Hit);
    let invoked = runtime
        .invoke(xero_desktop_lib::runtime::AutonomousSkillInvokeRequest {
            source: initial_source,
            timeout_ms: Some(1_000),
        })
        .expect("cache hit invoke");
    assert_eq!(invoked.cache_status, AutonomousSkillCacheStatus::Hit);

    source.set_tree_response(Ok(standard_skill_tree(
        "find-skills",
        "cccccccccccccccccccccccccccccccccccccccc",
    )));
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/SKILL.md",
        "---\nname: find-skills\ndescription: Discover installable skills refreshed.\nuser-invocable: false\n---\n\n# Find Skills\n",
    );
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/guide.md",
        "# Refreshed Guide\n",
    );
    let refreshed = runtime
        .install(xero_desktop_lib::runtime::AutonomousSkillInstallRequest {
            source: skill_source_metadata(
                "find-skills",
                "cccccccccccccccccccccccccccccccccccccccc",
            ),
            timeout_ms: Some(1_000),
        })
        .expect("cache refresh install");
    assert_eq!(
        refreshed.cache_status,
        AutonomousSkillCacheStatus::Refreshed
    );

    let records = project_store::list_installed_skills(
        &repo_root,
        project_store::InstalledSkillScopeFilter::Global,
    )
    .expect("list registered skills");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].source.state, XeroSkillSourceState::Enabled);
    assert_eq!(
        records[0].version_hash.as_deref(),
        Some("cccccccccccccccccccccccccccccccccccccccc")
    );
    assert!(records[0].last_used_at.is_some());

    source.set_tree_response(Err(AutonomousSkillSourceError::Timeout(
        "simulated timeout".into(),
    )));
    let failed = runtime
        .install(xero_desktop_lib::runtime::AutonomousSkillInstallRequest {
            source: skill_source_metadata(
                "find-skills",
                "dddddddddddddddddddddddddddddddddddddddd",
            ),
            timeout_ms: Some(1_000),
        })
        .expect_err("failed install should surface source error");
    assert_eq!(failed.code, "autonomous_skill_source_timeout");
    let failed_record = project_store::list_installed_skills(
        &repo_root,
        project_store::InstalledSkillScopeFilter::Global,
    )
    .expect("list failed registered skill")
    .pop()
    .expect("failed skill record");
    assert_eq!(failed_record.source.state, XeroSkillSourceState::Failed);
    assert_eq!(
        failed_record
            .last_diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("autonomous_skill_source_timeout")
    );
}

#[test]
fn local_and_project_skill_scanning_returns_candidates_and_typed_diagnostics() {
    let root = tempfile::tempdir().expect("temp dir");
    let local_root = root.path().join("local-skills");
    std::fs::create_dir_all(&local_root).expect("create local root");
    let valid = write_skill(&local_root, "write-docs", "write-docs", "Write docs.");
    std::fs::write(valid.join("guide.md"), "# Guide\n").expect("write guide");
    write_skill(&local_root, "dupe-a", "duplicate-skill", "First duplicate.");
    write_skill(
        &local_root,
        "dupe-b",
        "duplicate-skill",
        "Second duplicate.",
    );
    let invalid = local_root.join("missing-frontmatter");
    std::fs::create_dir_all(&invalid).expect("create invalid skill");
    std::fs::write(invalid.join("SKILL.md"), "# Missing frontmatter\n")
        .expect("write invalid markdown");
    let unsupported = write_skill(
        &local_root,
        "unsupported-asset",
        "unsupported-asset",
        "Unsupported asset.",
    );
    std::fs::write(unsupported.join("image.png"), b"not really a png").expect("write png");
    let oversized = local_root.join("oversized");
    std::fs::create_dir_all(&oversized).expect("create oversized skill");
    std::fs::write(
        oversized.join("SKILL.md"),
        format!(
            "---\nname: oversized\ndescription: Oversized.\n---\n\n{}",
            "x".repeat(129 * 1024)
        ),
    )
    .expect("write oversized markdown");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.path(), local_root.join("outside-link"))
        .expect("create outside symlink");

    let discovered =
        discover_local_skill_directory("personal", &local_root).expect("discover local skills");
    let skill_ids = discovered
        .candidates
        .iter()
        .map(|candidate| candidate.skill_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(skill_ids, vec!["duplicate-skill", "write-docs"]);
    assert!(discovered.candidates.iter().any(|candidate| matches!(
        candidate.source.locator,
        XeroSkillSourceLocator::Local { .. }
    )));
    let diagnostic_codes = discovered
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(diagnostic_codes.contains(&"autonomous_skill_duplicate_id"));
    assert!(diagnostic_codes.contains(&"autonomous_skill_document_invalid"));
    assert!(diagnostic_codes.contains(&"autonomous_skill_layout_unsupported"));
    assert!(diagnostic_codes.contains(&"autonomous_skill_path_outside_root"));

    db::configure_project_database_paths(&root.path().join("app-data").join("xero.db"));
    let project_root = root.path().join("project");
    std::fs::create_dir_all(&project_root).expect("create project root");
    let project_skill_root = db::project_app_data_dir_for_repo(&project_root).join("skills");
    write_skill(
        &project_skill_root,
        "project-helper",
        "project-helper",
        "Project helper.",
    );
    let project_discovery = discover_project_skill_directory(
        "project-1",
        db::project_app_data_dir_for_repo(&project_root),
    )
    .expect("project discovery");
    assert_eq!(project_discovery.diagnostics, Vec::new());
    assert_eq!(project_discovery.candidates.len(), 1);
    assert_eq!(project_discovery.candidates[0].skill_id, "project-helper");
    assert!(matches!(
        project_discovery.candidates[0].source.scope,
        XeroSkillSourceScope::Project { .. }
    ));
    assert!(project_discovery.candidates[0]
        .source
        .source_id
        .contains("skills/project-helper"));
}

#[test]
fn bundled_skill_discovery_is_deterministic_and_loads_invocation_context_without_network() {
    let root = tempfile::tempdir().expect("temp dir");
    let bundled_root = root.path().join("bundled");
    let beta = write_skill(
        &bundled_root,
        "beta-skill",
        "beta-skill",
        "Beta bundled skill.",
    );
    std::fs::write(beta.join("guide.md"), "# Beta guide\n").expect("write beta guide");
    write_skill(
        &bundled_root,
        "alpha-skill",
        "alpha-skill",
        "Alpha bundled skill.",
    );

    let discovered = discover_bundled_skill_directory("xero", "2026.04.25", &bundled_root)
        .expect("discover bundled skills");
    assert_eq!(discovered.diagnostics, Vec::new());
    let skill_ids = discovered
        .candidates
        .iter()
        .map(|candidate| candidate.skill_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(skill_ids, vec!["alpha-skill", "beta-skill"]);
    assert!(discovered
        .candidates
        .iter()
        .all(|candidate| candidate.source.trust == XeroSkillTrustState::Trusted));
    assert!(discovered
        .candidates
        .iter()
        .all(|candidate| !candidate.version_hash.is_empty()));

    let beta_candidate = discovered
        .candidates
        .iter()
        .find(|candidate| candidate.skill_id == "beta-skill")
        .expect("beta candidate");
    let context =
        load_discovered_skill_context(beta_candidate, true).expect("load bundled context");
    assert_eq!(context.skill_id, "beta-skill");
    assert!(context.markdown.content.contains("# beta-skill"));
    assert_eq!(context.supporting_assets.len(), 1);
    assert_eq!(context.supporting_assets[0].relative_path, "guide.md");
}
