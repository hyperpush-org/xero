use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use cadence_desktop_lib::{
    configure_builder_with_state,
    runtime::{
        AutonomousSkillCacheError, AutonomousSkillCacheManifest, AutonomousSkillCacheStatus,
        AutonomousSkillCacheStore, AutonomousSkillDiscoverRequest, AutonomousSkillRuntime,
        AutonomousSkillRuntimeConfig, AutonomousSkillSource, AutonomousSkillSourceEntryKind,
        AutonomousSkillSourceError, AutonomousSkillSourceFileRequest,
        AutonomousSkillSourceFileResponse, AutonomousSkillSourceMetadata,
        AutonomousSkillSourceTreeEntry, AutonomousSkillSourceTreeRequest,
        AutonomousSkillSourceTreeResponse, FilesystemAutonomousSkillCacheStore,
    },
    state::DesktopState,
};
use tauri::Manager;
use tempfile::TempDir;

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
    tree_requests: Vec<AutonomousSkillSourceTreeRequest>,
    file_requests: Vec<AutonomousSkillSourceFileRequest>,
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

    fn tree_request_count(&self) -> usize {
        self.state
            .lock()
            .expect("fixture source lock")
            .tree_requests
            .len()
    }

    fn file_request_count(&self) -> usize {
        self.state
            .lock()
            .expect("fixture source lock")
            .file_requests
            .len()
    }
}

impl AutonomousSkillSource for FixtureSkillSource {
    fn list_tree(
        &self,
        request: &AutonomousSkillSourceTreeRequest,
    ) -> Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError> {
        let mut state = self.state.lock().expect("fixture source lock");
        state.tree_requests.push(request.clone());
        state
            .tree_response
            .clone()
            .expect("fixture tree response should exist")
    }

    fn fetch_file(
        &self,
        request: &AutonomousSkillSourceFileRequest,
    ) -> Result<AutonomousSkillSourceFileResponse, AutonomousSkillSourceError> {
        let mut state = self.state.lock().expect("fixture source lock");
        state.file_requests.push(request.clone());
        state
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

#[derive(Debug)]
struct FailingCacheStore;

impl AutonomousSkillCacheStore for FailingCacheStore {
    fn load_manifest(
        &self,
        _cache_key: &str,
    ) -> Result<Option<AutonomousSkillCacheManifest>, AutonomousSkillCacheError> {
        Ok(None)
    }

    fn verify_manifest(
        &self,
        _cache_key: &str,
        _manifest: &AutonomousSkillCacheManifest,
    ) -> Result<String, AutonomousSkillCacheError> {
        unreachable!("verify_manifest is never called for a failing cache store")
    }

    fn install(
        &self,
        _cache_key: &str,
        _manifest: &AutonomousSkillCacheManifest,
        _files: &[cadence_desktop_lib::runtime::AutonomousSkillCacheInstallFile],
    ) -> Result<String, AutonomousSkillCacheError> {
        Err(AutonomousSkillCacheError::Write(
            "simulated cache write failure".into(),
        ))
    }

    fn load_text_file(
        &self,
        _cache_key: &str,
        _tree_hash: &str,
        _relative_path: &str,
    ) -> Result<String, AutonomousSkillCacheError> {
        unreachable!("load_text_file is never called for a failing cache store")
    }
}

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
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

fn runtime_with_cache_root(
    cache_root: &Path,
    source: FixtureSkillSource,
) -> AutonomousSkillRuntime {
    AutonomousSkillRuntime::with_source_and_cache(
        runtime_config(),
        Arc::new(source),
        Arc::new(FilesystemAutonomousSkillCacheStore::new(
            cache_root.to_path_buf(),
        )),
    )
}

fn runtime_with_state(
    root: &TempDir,
    source: FixtureSkillSource,
) -> (
    tauri::App<tauri::test::MockRuntime>,
    AutonomousSkillRuntime,
    PathBuf,
) {
    let state = DesktopState::default().with_autonomous_skill_cache_dir_override(
        root.path().join("app-data").join("autonomous-skills"),
    );
    let app = build_mock_app(state);
    let cache_root = app
        .state::<DesktopState>()
        .autonomous_skill_cache_dir(&app.handle().clone())
        .expect("autonomous skill cache dir");
    let runtime = runtime_with_cache_root(&cache_root, source);
    (app, runtime, cache_root)
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
                bytes: Some(256),
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

fn read_manifest(cache_root: &Path, cache_key: &str) -> AutonomousSkillCacheManifest {
    let manifest_path = cache_root.join(cache_key).join("manifest.json");
    let contents = std::fs::read_to_string(&manifest_path).expect("read manifest file");
    serde_json::from_str(&contents).expect("decode manifest")
}

#[test]
fn skill_runtime_discovers_candidates_from_source_tree() {
    let source = FixtureSkillSource::default();
    source.set_tree_response(Ok(AutonomousSkillSourceTreeResponse {
        entries: vec![
            AutonomousSkillSourceTreeEntry {
                path: "skills/find-skills".into(),
                kind: AutonomousSkillSourceEntryKind::Tree,
                hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                bytes: None,
            },
            AutonomousSkillSourceTreeEntry {
                path: "skills/test-helper".into(),
                kind: AutonomousSkillSourceEntryKind::Tree,
                hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                bytes: None,
            },
        ],
    }));

    let root = tempfile::tempdir().expect("temp dir");
    let runtime = runtime_with_cache_root(root.path(), source.clone());
    let output = runtime
        .discover(AutonomousSkillDiscoverRequest {
            query: "find".into(),
            result_limit: Some(5),
            timeout_ms: Some(1_000),
            source_repo: None,
            source_ref: None,
        })
        .expect("discovery should succeed");

    assert_eq!(output.source_repo, "vercel-labs/skills");
    assert_eq!(output.source_ref, "main");
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].skill_id, "find-skills");
    assert_eq!(
        output.candidates[0].source,
        skill_source_metadata("find-skills", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert!(!output.truncated);
    assert_eq!(source.tree_request_count(), 1);
    assert_eq!(source.file_request_count(), 0);
}

#[test]
fn skill_runtime_installs_into_cadence_owned_cache_and_reuses_existing_tree() {
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
        "# Supporting Guide\nUse this for discovery.\n",
    );

    let root = tempfile::tempdir().expect("temp dir");
    let (_app, runtime, cache_root) = runtime_with_state(&root, source.clone());
    let source_metadata =
        skill_source_metadata("find-skills", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    let install = runtime
        .install(
            cadence_desktop_lib::runtime::AutonomousSkillInstallRequest {
                source: source_metadata.clone(),
                timeout_ms: Some(1_000),
            },
        )
        .expect("initial install should succeed");

    assert_eq!(install.cache_status, AutonomousSkillCacheStatus::Miss);
    assert!(install
        .cache_directory
        .starts_with(cache_root.to_string_lossy().as_ref()));
    let manifest = read_manifest(&cache_root, &install.cache_key);
    assert_eq!(manifest.source, source_metadata);
    assert_eq!(manifest.skill_id, "find-skills");
    assert_eq!(manifest.files.len(), 2);
    assert_eq!(source.tree_request_count(), 1);
    assert_eq!(source.file_request_count(), 2);

    let invoke = runtime
        .invoke(cadence_desktop_lib::runtime::AutonomousSkillInvokeRequest {
            source: skill_source_metadata(
                "find-skills",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            timeout_ms: Some(1_000),
        })
        .expect("repeat invoke should reuse cache");

    assert_eq!(invoke.cache_status, AutonomousSkillCacheStatus::Hit);
    assert_eq!(invoke.skill_id, "find-skills");
    assert!(invoke.skill_markdown.contains("# Find Skills"));
    assert_eq!(invoke.supporting_assets.len(), 1);
    assert_eq!(invoke.supporting_assets[0].relative_path, "guide.md");
    assert_eq!(source.tree_request_count(), 1);
    assert_eq!(source.file_request_count(), 2);
}

#[test]
fn skill_runtime_refreshes_cache_when_tree_hash_changes() {
    let source = FixtureSkillSource::default();
    source.set_tree_response(Ok(standard_skill_tree(
        "find-skills",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )));
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/SKILL.md",
        "---\nname: find-skills\ndescription: First revision.\n---\n\n# First\n",
    );
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/guide.md",
        "first guide\n",
    );

    let root = tempfile::tempdir().expect("temp dir");
    let (_app, runtime, cache_root) = runtime_with_state(&root, source.clone());
    let initial = runtime
        .install(
            cadence_desktop_lib::runtime::AutonomousSkillInstallRequest {
                source: skill_source_metadata(
                    "find-skills",
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                ),
                timeout_ms: Some(1_000),
            },
        )
        .expect("first install should succeed");

    assert_eq!(initial.cache_status, AutonomousSkillCacheStatus::Miss);
    assert_eq!(
        PathBuf::from(&initial.cache_directory),
        cache_root
            .join(&initial.cache_key)
            .join("trees")
            .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );

    source.set_tree_response(Ok(standard_skill_tree(
        "find-skills",
        "cccccccccccccccccccccccccccccccccccccccc",
    )));
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/SKILL.md",
        "---\nname: find-skills\ndescription: Second revision.\n---\n\n# Second\n",
    );
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/guide.md",
        "second guide\n",
    );

    let refreshed = runtime
        .install(
            cadence_desktop_lib::runtime::AutonomousSkillInstallRequest {
                source: skill_source_metadata(
                    "find-skills",
                    "cccccccccccccccccccccccccccccccccccccccc",
                ),
                timeout_ms: Some(1_000),
            },
        )
        .expect("refresh install should succeed");

    assert_eq!(
        refreshed.cache_status,
        AutonomousSkillCacheStatus::Refreshed
    );
    assert_eq!(refreshed.cache_key, initial.cache_key);
    assert_eq!(
        PathBuf::from(&refreshed.cache_directory),
        cache_root
            .join(&refreshed.cache_key)
            .join("trees")
            .join("cccccccccccccccccccccccccccccccccccccccc")
    );
    assert!(
        cache_root
            .join(&refreshed.cache_key)
            .join("trees")
            .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .is_dir(),
        "expected the original tree revision to remain available under the Cadence cache key"
    );
    assert!(
        cache_root
            .join(&refreshed.cache_key)
            .join("trees")
            .join("cccccccccccccccccccccccccccccccccccccccc")
            .is_dir(),
        "expected the refreshed tree revision to be written under the same Cadence cache key"
    );
    let manifest = read_manifest(&cache_root, &refreshed.cache_key);
    assert_eq!(
        manifest.source.tree_hash,
        "cccccccccccccccccccccccccccccccccccccccc"
    );
    assert_eq!(manifest.description, "Second revision.");
    assert_eq!(source.tree_request_count(), 2);
    assert_eq!(source.file_request_count(), 4);
}

#[test]
fn skill_runtime_rejects_malformed_skill_documents_during_invoke() {
    let source = FixtureSkillSource::default();
    source.set_tree_response(Ok(AutonomousSkillSourceTreeResponse {
        entries: vec![
            AutonomousSkillSourceTreeEntry {
                path: "skills/bad-skill".into(),
                kind: AutonomousSkillSourceEntryKind::Tree,
                hash: "dddddddddddddddddddddddddddddddddddddddd".into(),
                bytes: None,
            },
            AutonomousSkillSourceTreeEntry {
                path: "skills/bad-skill/SKILL.md".into(),
                kind: AutonomousSkillSourceEntryKind::Blob,
                hash: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into(),
                bytes: Some(64),
            },
        ],
    }));
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/bad-skill/SKILL.md",
        "---\nname: bad-skill\n---\n\n# Broken\n",
    );

    let root = tempfile::tempdir().expect("temp dir");
    let runtime = runtime_with_cache_root(root.path(), source);
    let error = runtime
        .invoke(cadence_desktop_lib::runtime::AutonomousSkillInvokeRequest {
            source: skill_source_metadata("bad-skill", "dddddddddddddddddddddddddddddddddddddddd"),
            timeout_ms: Some(1_000),
        })
        .expect_err("malformed skill document should fail closed");

    assert_eq!(error.code, "autonomous_skill_document_invalid");
    assert!(!error.retryable);
}

#[test]
fn skill_runtime_rejects_unsupported_asset_layouts() {
    let source = FixtureSkillSource::default();
    source.set_tree_response(Ok(AutonomousSkillSourceTreeResponse {
        entries: vec![
            AutonomousSkillSourceTreeEntry {
                path: "skills/find-skills".into(),
                kind: AutonomousSkillSourceEntryKind::Tree,
                hash: "ffffffffffffffffffffffffffffffffffffffff".into(),
                bytes: None,
            },
            AutonomousSkillSourceTreeEntry {
                path: "skills/find-skills/SKILL.md".into(),
                kind: AutonomousSkillSourceEntryKind::Blob,
                hash: "1111111111111111111111111111111111111111".into(),
                bytes: Some(64),
            },
            AutonomousSkillSourceTreeEntry {
                path: "skills/find-skills/icon.png".into(),
                kind: AutonomousSkillSourceEntryKind::Blob,
                hash: "2222222222222222222222222222222222222222".into(),
                bytes: Some(32),
            },
        ],
    }));

    let root = tempfile::tempdir().expect("temp dir");
    let runtime = runtime_with_cache_root(root.path(), source);
    let error = runtime
        .invoke(cadence_desktop_lib::runtime::AutonomousSkillInvokeRequest {
            source: skill_source_metadata(
                "find-skills",
                "ffffffffffffffffffffffffffffffffffffffff",
            ),
            timeout_ms: Some(1_000),
        })
        .expect_err("unsupported asset layout should fail closed");

    assert_eq!(error.code, "autonomous_skill_layout_unsupported");
    assert!(!error.retryable);
}

#[test]
fn skill_runtime_surfaces_typed_discovery_timeouts() {
    let source = FixtureSkillSource::default();
    source.set_tree_response(Err(AutonomousSkillSourceError::Timeout(
        "simulated source timeout".into(),
    )));

    let root = tempfile::tempdir().expect("temp dir");
    let runtime = runtime_with_cache_root(root.path(), source);
    let error = runtime
        .discover(AutonomousSkillDiscoverRequest {
            query: "find".into(),
            result_limit: None,
            timeout_ms: Some(1_000),
            source_repo: None,
            source_ref: None,
        })
        .expect_err("discovery timeout should be surfaced");

    assert_eq!(error.code, "autonomous_skill_discovery_timeout");
    assert!(error.retryable);
}

#[test]
fn skill_runtime_surfaces_typed_cache_write_failures() {
    let source = FixtureSkillSource::default();
    source.set_tree_response(Ok(standard_skill_tree(
        "find-skills",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )));
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/SKILL.md",
        "---\nname: find-skills\ndescription: Discover installable skills.\n---\n\n# Find Skills\n",
    );
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/guide.md",
        "# Supporting Guide\n",
    );

    let runtime = AutonomousSkillRuntime::with_source_and_cache(
        runtime_config(),
        Arc::new(source),
        Arc::new(FailingCacheStore),
    );
    let error = runtime
        .install(
            cadence_desktop_lib::runtime::AutonomousSkillInstallRequest {
                source: skill_source_metadata(
                    "find-skills",
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                ),
                timeout_ms: Some(1_000),
            },
        )
        .expect_err("cache write failures should be surfaced");

    assert_eq!(error.code, "autonomous_skill_cache_write_failed");
    assert!(error.retryable);
}
