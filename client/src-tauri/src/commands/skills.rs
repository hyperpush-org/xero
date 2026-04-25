use std::{collections::BTreeMap, path::PathBuf};

use tauri::{AppHandle, Manager, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        CommandError, CommandResult, InstalledSkillDiagnosticDto, ListSkillRegistryRequestDto,
        RemoveSkillLocalRootRequestDto, RemoveSkillRequestDto, SetSkillEnabledRequestDto,
        SkillDiscoveryDiagnosticDto, SkillGithubSourceDto, SkillLocalRootDto,
        SkillProjectSourceDto, SkillRegistryDto, SkillRegistryEntryDto, SkillSourceKindDto,
        SkillSourceMetadataDto, SkillSourceScopeDto, SkillSourceSettingsDto, SkillSourceStateDto,
        SkillTrustStateDto, UpdateGithubSkillSourceRequestDto, UpdateProjectSkillSourceRequestDto,
        UpsertSkillLocalRootRequestDto,
    },
    db::project_store::{
        self, InstalledSkillDiagnosticRecord, InstalledSkillRecord, InstalledSkillScopeFilter,
    },
    runtime::{
        discover_bundled_skill_directory, discover_local_skill_directory,
        discover_project_skill_directory, resolve_imported_repo_root, CadenceDiscoveredSkill,
        CadenceSkillDirectoryDiscovery, CadenceSkillDiscoveryDiagnostic, CadenceSkillSourceKind,
        CadenceSkillSourceLocator, CadenceSkillSourceRecord, CadenceSkillSourceScope,
        CadenceSkillSourceState, CadenceSkillTrustState, SkillSourceSettings,
    },
    state::DesktopState,
};

#[tauri::command]
pub fn list_skill_registry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListSkillRegistryRequestDto,
) -> CommandResult<SkillRegistryDto> {
    load_skill_registry(&app, state.inner(), request)
}

#[tauri::command]
pub fn reload_skill_registry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListSkillRegistryRequestDto,
) -> CommandResult<SkillRegistryDto> {
    load_skill_registry(&app, state.inner(), request)
}

#[tauri::command]
pub fn set_skill_enabled<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SetSkillEnabledRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let project_id = validate_required(request.project_id, "projectId")?;
    let source_id = validate_required(request.source_id, "sourceId")?;
    let repo_root = resolve_imported_repo_root(&app, state.inner(), &project_id)?;

    if project_store::load_installed_skill_by_source_id(&repo_root, &source_id)?.is_some() {
        project_store::set_installed_skill_enabled(
            &repo_root,
            &source_id,
            request.enabled,
            now_timestamp(),
        )?;
    } else {
        let discovered = collect_discoverable_skills(&app, state.inner(), Some(&project_id))?
            .candidates
            .into_iter()
            .find(|candidate| candidate.source.source_id == source_id)
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "skill_source_not_found",
                    format!("Cadence could not find skill source `{source_id}`."),
                )
            })?;
        let trust = if request.enabled {
            approve_trust_for_user_enable(discovered.source.trust)
        } else {
            discovered.source.trust
        };
        let record = InstalledSkillRecord::from_discovered_skill(
            &discovered,
            if request.enabled {
                CadenceSkillSourceState::Enabled
            } else {
                CadenceSkillSourceState::Disabled
            },
            trust,
            now_timestamp(),
        )?;
        project_store::upsert_installed_skill(&repo_root, record)?;
    }

    load_skill_registry(
        &app,
        state.inner(),
        ListSkillRegistryRequestDto {
            project_id: Some(project_id),
            query: None,
            include_unavailable: true,
        },
    )
}

#[tauri::command]
pub fn remove_skill<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RemoveSkillRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let project_id = validate_required(request.project_id, "projectId")?;
    let source_id = validate_required(request.source_id, "sourceId")?;
    let repo_root = resolve_imported_repo_root(&app, state.inner(), &project_id)?;
    if !project_store::remove_installed_skill(&repo_root, &source_id)? {
        return Err(CommandError::user_fixable(
            "installed_skill_not_found",
            format!("Cadence could not find installed skill source `{source_id}`."),
        ));
    }

    load_skill_registry(
        &app,
        state.inner(),
        ListSkillRegistryRequestDto {
            project_id: Some(project_id),
            query: None,
            include_unavailable: true,
        },
    )
}

#[tauri::command]
pub fn upsert_skill_local_root<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertSkillLocalRootRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let path = state.skill_source_settings_file(&app)?;
    let settings = load_settings(&app, state.inner())?.upsert_local_root(
        request.root_id,
        request.path,
        request.enabled,
    )?;
    persist_settings(&path, settings)?;
    load_skill_registry(
        &app,
        state.inner(),
        ListSkillRegistryRequestDto {
            project_id: request.project_id,
            query: None,
            include_unavailable: true,
        },
    )
}

#[tauri::command]
pub fn remove_skill_local_root<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RemoveSkillLocalRootRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let path = state.skill_source_settings_file(&app)?;
    let settings = load_settings(&app, state.inner())?.remove_local_root(&request.root_id)?;
    persist_settings(&path, settings)?;
    load_skill_registry(
        &app,
        state.inner(),
        ListSkillRegistryRequestDto {
            project_id: request.project_id,
            query: None,
            include_unavailable: true,
        },
    )
}

#[tauri::command]
pub fn update_project_skill_source<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateProjectSkillSourceRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let project_id = validate_required(request.project_id, "projectId")?;
    let path = state.skill_source_settings_file(&app)?;
    let settings =
        load_settings(&app, state.inner())?.update_project(project_id.clone(), request.enabled)?;
    persist_settings(&path, settings)?;
    load_skill_registry(
        &app,
        state.inner(),
        ListSkillRegistryRequestDto {
            project_id: Some(project_id),
            query: None,
            include_unavailable: true,
        },
    )
}

#[tauri::command]
pub fn update_github_skill_source<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateGithubSkillSourceRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let path = state.skill_source_settings_file(&app)?;
    let settings = load_settings(&app, state.inner())?.update_github(
        request.repo,
        request.reference,
        request.root,
        request.enabled,
    )?;
    persist_settings(&path, settings)?;
    load_skill_registry(
        &app,
        state.inner(),
        ListSkillRegistryRequestDto {
            project_id: request.project_id,
            query: None,
            include_unavailable: true,
        },
    )
}

fn load_skill_registry<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    request: ListSkillRegistryRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let project_id = request
        .project_id
        .as_deref()
        .map(|value| validate_required(value.to_owned(), "projectId"))
        .transpose()?;
    let settings = load_settings(app, state)?;
    let query = request.query.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    });

    let mut entries = BTreeMap::<String, SkillRegistryEntryDto>::new();
    let mut diagnostics = Vec::new();

    if let Some(project_id) = project_id.as_deref() {
        let repo_root = resolve_imported_repo_root(app, state, project_id)?;
        for record in project_store::list_installed_skills(
            &repo_root,
            InstalledSkillScopeFilter::project(project_id.to_owned(), true)?,
        )? {
            insert_entry(&mut entries, skill_entry_from_installed(&record)?);
        }
    }

    let discovery = collect_discoverable_skills(app, state, project_id.as_deref())?;
    diagnostics.extend(
        discovery
            .diagnostics
            .into_iter()
            .map(discovery_diagnostic_dto),
    );
    for candidate in discovery.candidates {
        let source_id = candidate.source.source_id.clone();
        entries
            .entry(source_id)
            .or_insert(skill_entry_from_discovered(&candidate)?);
    }

    let mut entries = entries
        .into_values()
        .filter(|entry| request.include_unavailable || model_visible_in_settings(entry))
        .filter(|entry| {
            query
                .as_deref()
                .map(|query| skill_entry_matches_query(entry, query))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        skill_entry_rank(left)
            .cmp(&skill_entry_rank(right))
            .then_with(|| left.skill_id.cmp(&right.skill_id))
            .then_with(|| left.source_id.cmp(&right.source_id))
    });

    Ok(SkillRegistryDto {
        project_id,
        entries,
        sources: skill_source_settings_dto(&settings),
        diagnostics,
        reloaded_at: now_timestamp(),
    })
}

struct DiscoveredSkillSnapshot {
    candidates: Vec<CadenceDiscoveredSkill>,
    diagnostics: Vec<CadenceSkillDiscoveryDiagnostic>,
}

fn collect_discoverable_skills<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: Option<&str>,
) -> CommandResult<DiscoveredSkillSnapshot> {
    let settings = load_settings(app, state)?;
    let mut candidates = Vec::new();
    let mut diagnostics = Vec::new();

    if let Some(project_id) =
        project_id.filter(|project_id| settings.project_discovery_enabled(project_id))
    {
        let repo_root = resolve_imported_repo_root(app, state, project_id)?;
        push_discovery(
            &mut candidates,
            &mut diagnostics,
            discover_project_skill_directory(project_id, &repo_root)?,
        );
    }

    for root in settings.enabled_local_roots() {
        push_discovery(
            &mut candidates,
            &mut diagnostics,
            discover_local_skill_directory(root.root_id, root.path)?,
        );
    }

    for root in bundled_skill_roots(app) {
        push_discovery(
            &mut candidates,
            &mut diagnostics,
            discover_bundled_skill_directory(root.bundle_id, root.version, root.root_path)?,
        );
    }

    candidates.sort_by(|left, right| left.source.source_id.cmp(&right.source.source_id));
    Ok(DiscoveredSkillSnapshot {
        candidates,
        diagnostics,
    })
}

fn push_discovery(
    candidates: &mut Vec<CadenceDiscoveredSkill>,
    diagnostics: &mut Vec<CadenceSkillDiscoveryDiagnostic>,
    discovered: CadenceSkillDirectoryDiscovery,
) {
    candidates.extend(discovered.candidates);
    diagnostics.extend(discovered.diagnostics);
}

struct BundledSkillRoot {
    bundle_id: String,
    version: String,
    root_path: PathBuf,
}

fn bundled_skill_roots<R: Runtime>(app: &AppHandle<R>) -> Vec<BundledSkillRoot> {
    app.path()
        .resource_dir()
        .ok()
        .map(|root| root.join("skills"))
        .filter(|root| root.is_dir())
        .map(|root| {
            vec![BundledSkillRoot {
                bundle_id: "cadence".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                root_path: root,
            }]
        })
        .unwrap_or_default()
}

fn load_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<SkillSourceSettings> {
    crate::runtime::load_skill_source_settings_from_path(&state.skill_source_settings_file(app)?)
}

fn persist_settings(path: &std::path::Path, settings: SkillSourceSettings) -> CommandResult<()> {
    crate::runtime::persist_skill_source_settings(path, settings)?;
    Ok(())
}

fn insert_entry(
    entries: &mut BTreeMap<String, SkillRegistryEntryDto>,
    entry: SkillRegistryEntryDto,
) {
    entries
        .entry(entry.source_id.clone())
        .and_modify(|current| {
            if !current.installed && entry.installed {
                *current = entry.clone();
            }
        })
        .or_insert(entry);
}

fn skill_entry_from_installed(
    record: &InstalledSkillRecord,
) -> CommandResult<SkillRegistryEntryDto> {
    let source = record.source.clone().validate()?;
    Ok(SkillRegistryEntryDto {
        source_id: source.source_id.clone(),
        skill_id: record.skill_id.clone(),
        name: record.name.clone(),
        description: record.description.clone(),
        source_kind: source_kind_dto(source.locator.kind()),
        scope: source_scope_dto(&source.scope),
        project_id: source_project_id(&source.scope),
        source_state: source_state_dto(source.state),
        trust_state: trust_state_dto(source.trust),
        enabled: source.state == CadenceSkillSourceState::Enabled,
        installed: true,
        user_invocable: record.user_invocable,
        version_hash: record.version_hash.clone(),
        last_used_at: record.last_used_at.clone(),
        last_diagnostic: record
            .last_diagnostic
            .as_ref()
            .map(installed_diagnostic_dto),
        source: source_metadata_dto(&source),
    })
}

fn skill_entry_from_discovered(
    candidate: &CadenceDiscoveredSkill,
) -> CommandResult<SkillRegistryEntryDto> {
    let source = candidate.source.clone().validate()?;
    Ok(SkillRegistryEntryDto {
        source_id: source.source_id.clone(),
        skill_id: candidate.skill_id.clone(),
        name: candidate.name.clone(),
        description: candidate.description.clone(),
        source_kind: source_kind_dto(source.locator.kind()),
        scope: source_scope_dto(&source.scope),
        project_id: source_project_id(&source.scope),
        source_state: source_state_dto(source.state),
        trust_state: trust_state_dto(source.trust),
        enabled: source.state == CadenceSkillSourceState::Enabled,
        installed: false,
        user_invocable: candidate.user_invocable,
        version_hash: Some(candidate.version_hash.clone()),
        last_used_at: None,
        last_diagnostic: None,
        source: source_metadata_dto(&source),
    })
}

fn source_metadata_dto(source: &CadenceSkillSourceRecord) -> SkillSourceMetadataDto {
    match &source.locator {
        CadenceSkillSourceLocator::Bundled {
            bundle_id,
            skill_id,
            version,
        } => SkillSourceMetadataDto {
            label: format!("Bundled `{skill_id}` from {bundle_id} {version}"),
            repo: None,
            reference: Some(version.clone()),
            path: None,
            root_id: None,
            root_path: None,
            relative_path: None,
            bundle_id: Some(bundle_id.clone()),
            plugin_id: None,
            server_id: None,
        },
        CadenceSkillSourceLocator::Local {
            root_id,
            root_path,
            relative_path,
            ..
        } => SkillSourceMetadataDto {
            label: format!("Local root {root_id} · {relative_path}"),
            repo: None,
            reference: None,
            path: Some(root_path.clone()),
            root_id: Some(root_id.clone()),
            root_path: Some(root_path.clone()),
            relative_path: Some(relative_path.clone()),
            bundle_id: None,
            plugin_id: None,
            server_id: None,
        },
        CadenceSkillSourceLocator::Project { relative_path, .. } => SkillSourceMetadataDto {
            label: format!("Project skill {relative_path}"),
            repo: None,
            reference: None,
            path: Some(relative_path.clone()),
            root_id: None,
            root_path: None,
            relative_path: Some(relative_path.clone()),
            bundle_id: None,
            plugin_id: None,
            server_id: None,
        },
        CadenceSkillSourceLocator::Github {
            repo,
            reference,
            path,
            ..
        } => SkillSourceMetadataDto {
            label: format!("{repo} · {path} @ {reference}"),
            repo: Some(repo.clone()),
            reference: Some(reference.clone()),
            path: Some(path.clone()),
            root_id: None,
            root_path: None,
            relative_path: Some(path.clone()),
            bundle_id: None,
            plugin_id: None,
            server_id: None,
        },
        CadenceSkillSourceLocator::Dynamic {
            run_id,
            artifact_id,
            ..
        } => SkillSourceMetadataDto {
            label: format!("Dynamic artifact {artifact_id} from run {run_id}"),
            repo: None,
            reference: Some(run_id.clone()),
            path: Some(artifact_id.clone()),
            root_id: None,
            root_path: None,
            relative_path: None,
            bundle_id: None,
            plugin_id: None,
            server_id: None,
        },
        CadenceSkillSourceLocator::Mcp {
            server_id,
            capability_id,
            ..
        } => SkillSourceMetadataDto {
            label: format!("MCP server {server_id} · {capability_id}"),
            repo: None,
            reference: None,
            path: Some(capability_id.clone()),
            root_id: None,
            root_path: None,
            relative_path: None,
            bundle_id: None,
            plugin_id: None,
            server_id: Some(server_id.clone()),
        },
        CadenceSkillSourceLocator::Plugin {
            plugin_id,
            contribution_id,
            skill_path,
            ..
        } => SkillSourceMetadataDto {
            label: format!("Plugin {plugin_id} · {contribution_id}"),
            repo: None,
            reference: None,
            path: Some(skill_path.clone()),
            root_id: None,
            root_path: None,
            relative_path: Some(skill_path.clone()),
            bundle_id: None,
            plugin_id: Some(plugin_id.clone()),
            server_id: None,
        },
    }
}

fn skill_source_settings_dto(settings: &SkillSourceSettings) -> SkillSourceSettingsDto {
    SkillSourceSettingsDto {
        local_roots: settings
            .local_roots
            .iter()
            .map(|root| SkillLocalRootDto {
                root_id: root.root_id.clone(),
                path: root.path.clone(),
                enabled: root.enabled,
                updated_at: root.updated_at.clone(),
            })
            .collect(),
        github: SkillGithubSourceDto {
            repo: settings.github.repo.clone(),
            reference: settings.github.reference.clone(),
            root: settings.github.root.clone(),
            enabled: settings.github.enabled,
            updated_at: settings.github.updated_at.clone(),
        },
        projects: settings
            .projects
            .iter()
            .map(|project| SkillProjectSourceDto {
                project_id: project.project_id.clone(),
                enabled: project.enabled,
                updated_at: project.updated_at.clone(),
            })
            .collect(),
        updated_at: settings.updated_at.clone(),
    }
}

fn installed_diagnostic_dto(
    diagnostic: &InstalledSkillDiagnosticRecord,
) -> InstalledSkillDiagnosticDto {
    InstalledSkillDiagnosticDto {
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        retryable: diagnostic.retryable,
        recorded_at: diagnostic.recorded_at.clone(),
    }
}

fn discovery_diagnostic_dto(
    diagnostic: CadenceSkillDiscoveryDiagnostic,
) -> SkillDiscoveryDiagnosticDto {
    SkillDiscoveryDiagnosticDto {
        code: diagnostic.code,
        message: diagnostic.message,
        relative_path: diagnostic.relative_path,
    }
}

fn source_kind_dto(kind: CadenceSkillSourceKind) -> SkillSourceKindDto {
    match kind {
        CadenceSkillSourceKind::Bundled => SkillSourceKindDto::Bundled,
        CadenceSkillSourceKind::Local => SkillSourceKindDto::Local,
        CadenceSkillSourceKind::Project => SkillSourceKindDto::Project,
        CadenceSkillSourceKind::Github => SkillSourceKindDto::Github,
        CadenceSkillSourceKind::Dynamic => SkillSourceKindDto::Dynamic,
        CadenceSkillSourceKind::Mcp => SkillSourceKindDto::Mcp,
        CadenceSkillSourceKind::Plugin => SkillSourceKindDto::Plugin,
    }
}

fn source_scope_dto(scope: &CadenceSkillSourceScope) -> SkillSourceScopeDto {
    match scope {
        CadenceSkillSourceScope::Global => SkillSourceScopeDto::Global,
        CadenceSkillSourceScope::Project { .. } => SkillSourceScopeDto::Project,
    }
}

fn source_project_id(scope: &CadenceSkillSourceScope) -> Option<String> {
    match scope {
        CadenceSkillSourceScope::Global => None,
        CadenceSkillSourceScope::Project { project_id } => Some(project_id.clone()),
    }
}

fn source_state_dto(state: CadenceSkillSourceState) -> SkillSourceStateDto {
    match state {
        CadenceSkillSourceState::Discoverable => SkillSourceStateDto::Discoverable,
        CadenceSkillSourceState::Installed => SkillSourceStateDto::Installed,
        CadenceSkillSourceState::Enabled => SkillSourceStateDto::Enabled,
        CadenceSkillSourceState::Disabled => SkillSourceStateDto::Disabled,
        CadenceSkillSourceState::Stale => SkillSourceStateDto::Stale,
        CadenceSkillSourceState::Failed => SkillSourceStateDto::Failed,
        CadenceSkillSourceState::Blocked => SkillSourceStateDto::Blocked,
    }
}

fn trust_state_dto(trust: CadenceSkillTrustState) -> SkillTrustStateDto {
    match trust {
        CadenceSkillTrustState::Trusted => SkillTrustStateDto::Trusted,
        CadenceSkillTrustState::UserApproved => SkillTrustStateDto::UserApproved,
        CadenceSkillTrustState::ApprovalRequired => SkillTrustStateDto::ApprovalRequired,
        CadenceSkillTrustState::Untrusted => SkillTrustStateDto::Untrusted,
        CadenceSkillTrustState::Blocked => SkillTrustStateDto::Blocked,
    }
}

fn approve_trust_for_user_enable(trust: CadenceSkillTrustState) -> CadenceSkillTrustState {
    match trust {
        CadenceSkillTrustState::Blocked | CadenceSkillTrustState::Trusted => trust,
        CadenceSkillTrustState::UserApproved
        | CadenceSkillTrustState::ApprovalRequired
        | CadenceSkillTrustState::Untrusted => CadenceSkillTrustState::UserApproved,
    }
}

fn model_visible_in_settings(entry: &SkillRegistryEntryDto) -> bool {
    !matches!(
        entry.source_state,
        SkillSourceStateDto::Disabled | SkillSourceStateDto::Blocked
    )
}

fn skill_entry_matches_query(entry: &SkillRegistryEntryDto, query: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return true;
    }
    [
        entry.skill_id.as_str(),
        entry.name.as_str(),
        entry.description.as_str(),
        entry.source.label.as_str(),
    ]
    .iter()
    .any(|value| value.to_ascii_lowercase().contains(&query))
}

fn skill_entry_rank(entry: &SkillRegistryEntryDto) -> (u8, u8, String) {
    let state_rank = match entry.source_state {
        SkillSourceStateDto::Enabled => 0,
        SkillSourceStateDto::Installed => 1,
        SkillSourceStateDto::Discoverable => 2,
        SkillSourceStateDto::Stale => 3,
        SkillSourceStateDto::Disabled => 4,
        SkillSourceStateDto::Failed => 5,
        SkillSourceStateDto::Blocked => 6,
    };
    let source_rank = match entry.source_kind {
        SkillSourceKindDto::Bundled => 0,
        SkillSourceKindDto::Project => 1,
        SkillSourceKindDto::Local => 2,
        SkillSourceKindDto::Github => 3,
        SkillSourceKindDto::Dynamic => 4,
        SkillSourceKindDto::Mcp => 5,
        SkillSourceKindDto::Plugin => 6,
    };
    (state_rank, source_rank, entry.skill_id.clone())
}

fn validate_required(value: String, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(trimmed.to_owned())
}
