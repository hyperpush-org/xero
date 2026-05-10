use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Duration,
};

use reqwest::{blocking::Client, header::USER_AGENT};
use serde::Deserialize;
use tauri::{AppHandle, Manager, Runtime, State};
use url::Url;

use crate::{
    auth::now_timestamp,
    commands::{
        AgentAuthoringSkillSearchResultDto, CommandError, CommandResult,
        InstalledSkillDiagnosticDto, ListSkillRegistryRequestDto, PluginCommandApprovalPolicyDto,
        PluginCommandAvailabilityDto, PluginCommandContributionDto, PluginCommandRiskLevelDto,
        PluginCommandStatePolicyDto, PluginDiagnosticDto, PluginRegistryEntryDto, PluginRootDto,
        PluginSkillContributionDto, RemovePluginRequestDto, RemovePluginRootRequestDto,
        RemoveSkillLocalRootRequestDto, RemoveSkillRequestDto,
        ResolveAgentAuthoringSkillRequestDto, SearchAgentAuthoringSkillsRequestDto,
        SearchAgentAuthoringSkillsResponseDto, SetPluginEnabledRequestDto,
        SetSkillEnabledRequestDto, SkillDiscoveryDiagnosticDto, SkillGithubSourceDto,
        SkillLocalRootDto, SkillProjectSourceDto, SkillRegistryContractDiagnosticDto,
        SkillRegistryDto, SkillRegistryEntryDto, SkillSourceKindDto, SkillSourceMetadataDto,
        SkillSourceScopeDto, SkillSourceSettingsDto, SkillSourceStateDto, SkillTrustStateDto,
        UpdateGithubSkillSourceRequestDto, UpdateProjectSkillSourceRequestDto,
        UpsertPluginRootRequestDto, UpsertSkillLocalRootRequestDto,
    },
    db::project_store::{
        self, InstalledPluginDiagnosticRecord, InstalledPluginRecord,
        InstalledSkillDiagnosticRecord, InstalledSkillRecord, InstalledSkillScopeFilter,
        PluginCommandRegistryRecord,
    },
    runtime::{
        discover_bundled_skill_directory, discover_local_skill_directory, discover_plugin_roots,
        discover_plugin_skill_contribution, discover_project_skill_directory,
        resolve_imported_repo_root, AutonomousSkillDiscoverRequest,
        AutonomousSkillDiscoveryCandidate, AutonomousSkillResolveOutput,
        AutonomousSkillResolveRequest, AutonomousSkillRuntime, AutonomousSkillRuntimeConfig,
        SkillSourceSettings, XeroDiscoveredSkill, XeroPluginDiscoveryDiagnostic, XeroPluginRoot,
        XeroSkillDirectoryDiscovery, XeroSkillDiscoveryDiagnostic, XeroSkillSourceKind,
        XeroSkillSourceLocator, XeroSkillSourceRecord, XeroSkillSourceScope, XeroSkillSourceState,
        XeroSkillTrustState,
    },
    state::DesktopState,
};

const SKILLS_SH_API_BASE_URL: &str = "https://skills.sh";
const SKILLS_SH_USER_AGENT: &str = "xero-desktop/skills";
const SKILLS_SH_REQUEST_TIMEOUT_MS: u64 = 8_000;
const SKILLS_SH_ALL_TIME_PAGE_SIZE: usize = 200;
const ONLINE_SKILL_DISCOVER_LIMIT: usize = 256;
const ONLINE_SKILL_SOURCE_ROOTS: &[&str] = &["skills", "skills/.curated", "skills/.system"];
const ONLINE_SKILL_SOURCE_REFS: &[&str] = &["main", "master"];

#[tauri::command]
pub fn list_skill_registry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListSkillRegistryRequestDto,
) -> CommandResult<SkillRegistryDto> {
    load_skill_registry(&app, state.inner(), request, false)
}

#[tauri::command]
pub fn reload_skill_registry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListSkillRegistryRequestDto,
) -> CommandResult<SkillRegistryDto> {
    load_skill_registry(&app, state.inner(), request, true)
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
                    format!("Xero could not find skill source `{source_id}`."),
                )
            })?;
        if request.enabled
            && (discovered.source.state == XeroSkillSourceState::Blocked
                || discovered.source.trust == XeroSkillTrustState::Blocked)
        {
            return Err(CommandError::user_fixable(
                "skill_source_blocked",
                format!("Xero cannot enable blocked skill source `{source_id}`."),
            ));
        }
        let trust = if request.enabled {
            approve_trust_for_user_enable(discovered.source.trust)
        } else {
            discovered.source.trust
        };
        let record = InstalledSkillRecord::from_discovered_skill(
            &discovered,
            if request.enabled {
                XeroSkillSourceState::Enabled
            } else {
                XeroSkillSourceState::Disabled
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
        false,
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
            format!("Xero could not find installed skill source `{source_id}`."),
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
        false,
    )
}

#[tauri::command]
pub fn upsert_skill_local_root<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertSkillLocalRootRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let path = state.global_db_path(&app)?;
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
        false,
    )
}

#[tauri::command]
pub fn remove_skill_local_root<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RemoveSkillLocalRootRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let path = state.global_db_path(&app)?;
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
        false,
    )
}

#[tauri::command]
pub fn update_project_skill_source<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateProjectSkillSourceRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let project_id = validate_required(request.project_id, "projectId")?;
    let path = state.global_db_path(&app)?;
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
        false,
    )
}

#[tauri::command]
pub fn update_github_skill_source<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateGithubSkillSourceRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let path = state.global_db_path(&app)?;
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
        false,
    )
}

#[tauri::command]
pub fn upsert_plugin_root<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertPluginRootRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let path = state.global_db_path(&app)?;
    let settings = load_settings(&app, state.inner())?.upsert_plugin_root(
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
        true,
    )
}

#[tauri::command]
pub fn remove_plugin_root<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RemovePluginRootRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let path = state.global_db_path(&app)?;
    let settings = load_settings(&app, state.inner())?.remove_plugin_root(&request.root_id)?;
    persist_settings(&path, settings)?;
    load_skill_registry(
        &app,
        state.inner(),
        ListSkillRegistryRequestDto {
            project_id: request.project_id,
            query: None,
            include_unavailable: true,
        },
        true,
    )
}

#[tauri::command]
pub fn set_plugin_enabled<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SetPluginEnabledRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let project_id = validate_required(request.project_id, "projectId")?;
    let plugin_id = validate_required(request.plugin_id, "pluginId")?;
    let repo_root = resolve_imported_repo_root(&app, state.inner(), &project_id)?;
    project_store::set_installed_plugin_enabled(&repo_root, &plugin_id, request.enabled)?;
    load_skill_registry(
        &app,
        state.inner(),
        ListSkillRegistryRequestDto {
            project_id: Some(project_id),
            query: None,
            include_unavailable: true,
        },
        false,
    )
}

#[tauri::command]
pub fn remove_plugin<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RemovePluginRequestDto,
) -> CommandResult<SkillRegistryDto> {
    let project_id = validate_required(request.project_id, "projectId")?;
    let plugin_id = validate_required(request.plugin_id, "pluginId")?;
    let repo_root = resolve_imported_repo_root(&app, state.inner(), &project_id)?;
    project_store::mark_installed_plugin_removed(&repo_root, &plugin_id)?;
    load_skill_registry(
        &app,
        state.inner(),
        ListSkillRegistryRequestDto {
            project_id: Some(project_id),
            query: None,
            include_unavailable: true,
        },
        false,
    )
}

pub(crate) fn load_skill_registry<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    request: ListSkillRegistryRequestDto,
    reload_plugins: bool,
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
    let mut plugin_records = Vec::new();
    let mut plugin_commands = Vec::new();

    if let Some(project_id) = project_id.as_deref() {
        let repo_root = resolve_imported_repo_root(app, state, project_id)?;
        let plugin_snapshot = collect_plugin_registry(app, state, project_id, reload_plugins)?;
        diagnostics.extend(
            plugin_snapshot
                .diagnostics
                .into_iter()
                .map(plugin_discovery_diagnostic_dto),
        );
        plugin_commands = project_store::plugin_command_descriptors(
            &plugin_snapshot.records,
            request.include_unavailable,
        )?;
        plugin_records = plugin_snapshot.records;
        for record in project_store::list_installed_skills(
            &repo_root,
            InstalledSkillScopeFilter::project(project_id.to_owned(), true)?,
        )? {
            insert_entry(
                &mut entries,
                skill_entry_from_installed(&apply_plugin_state_to_installed_skill(
                    record,
                    &plugin_records,
                ))?,
            );
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
    push_github_skill_search_entries(
        &mut entries,
        &mut diagnostics,
        app,
        state,
        &settings,
        project_id.as_deref(),
        query.as_deref(),
    )?;

    let should_filter_by_query = query
        .as_deref()
        .map(|query| !is_all_skills_query(query))
        .unwrap_or(false);
    let mut entries = entries
        .into_values()
        .filter(|entry| request.include_unavailable || model_visible_in_settings(entry))
        .filter(|entry| {
            if !should_filter_by_query {
                return true;
            }
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
        contract_version: 1,
        project_id,
        contract_diagnostics: validate_skill_registry_contract(&entries),
        entries,
        plugins: plugin_records
            .iter()
            .map(plugin_registry_entry_dto)
            .collect::<CommandResult<Vec<_>>>()?,
        plugin_commands: plugin_commands
            .iter()
            .map(plugin_command_dto)
            .collect::<CommandResult<Vec<_>>>()?,
        sources: skill_source_settings_dto(&settings),
        diagnostics,
        reloaded_at: now_timestamp(),
    })
}

fn validate_skill_registry_contract(
    entries: &[SkillRegistryEntryDto],
) -> Vec<SkillRegistryContractDiagnosticDto> {
    let mut diagnostics = Vec::new();
    let mut source_ids = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        if !source_ids.insert(entry.source_id.as_str()) {
            diagnostics.push(SkillRegistryContractDiagnosticDto {
                code: "skill_registry_duplicate_source_id".into(),
                message: format!(
                    "Skill registry cannot include duplicate source id `{}`.",
                    entry.source_id
                ),
                severity: "error".into(),
                path: vec!["entries".into(), index.to_string(), "sourceId".into()],
            });
        }
    }
    diagnostics
}

struct PluginRegistrySnapshot {
    records: Vec<InstalledPluginRecord>,
    diagnostics: Vec<XeroPluginDiscoveryDiagnostic>,
}

struct DiscoveredSkillSnapshot {
    candidates: Vec<XeroDiscoveredSkill>,
    diagnostics: Vec<XeroSkillDiscoveryDiagnostic>,
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
        let project_app_data_dir = crate::db::project_app_data_dir_for_repo(&repo_root);
        push_discovery(
            &mut candidates,
            &mut diagnostics,
            discover_project_skill_directory(project_id, project_app_data_dir)?,
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

    if let Some(project_id) = project_id {
        let plugin_snapshot = collect_plugin_registry(app, state, project_id, false)?;
        for plugin in plugin_snapshot.records {
            for skill in &plugin.manifest.skills {
                let state = if plugin.state == XeroSkillSourceState::Enabled {
                    XeroSkillSourceState::Discoverable
                } else {
                    plugin.state
                };
                let discovered = discover_plugin_skill_contribution(
                    project_id.to_owned(),
                    plugin.plugin_id.clone(),
                    skill.id.clone(),
                    &plugin.plugin_root_path,
                    skill.path.clone(),
                    state,
                    plugin.trust,
                )?;
                for candidate in discovered.candidates {
                    if candidate.skill_id == skill.id {
                        candidates.push(candidate);
                    } else {
                        diagnostics.push(XeroSkillDiscoveryDiagnostic {
                            code: "xero_plugin_skill_id_mismatch".into(),
                            message: format!(
                                "Xero skipped plugin `{}` skill contribution `{}` because SKILL.md declared `{}`.",
                                plugin.plugin_id, skill.id, candidate.skill_id
                            ),
                            relative_path: Some(skill.path.clone()),
                        });
                    }
                }
                diagnostics.extend(discovered.diagnostics);
            }
        }
        diagnostics.extend(plugin_snapshot.diagnostics.into_iter().map(|diagnostic| {
            XeroSkillDiscoveryDiagnostic {
                code: diagnostic.code,
                message: diagnostic.message,
                relative_path: diagnostic.relative_path,
            }
        }));
    }

    candidates.sort_by(|left, right| left.source.source_id.cmp(&right.source.source_id));
    Ok(DiscoveredSkillSnapshot {
        candidates,
        diagnostics,
    })
}

fn push_github_skill_search_entries<R: Runtime>(
    entries: &mut BTreeMap<String, SkillRegistryEntryDto>,
    diagnostics: &mut Vec<SkillDiscoveryDiagnosticDto>,
    app: &AppHandle<R>,
    state: &DesktopState,
    settings: &SkillSourceSettings,
    project_id: Option<&str>,
    query: Option<&str>,
) -> CommandResult<()> {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if !settings.github.enabled {
        return Ok(());
    }

    let platform_config = AutonomousSkillRuntimeConfig::for_platform();
    let mut skill_runtime_config = AutonomousSkillRuntimeConfig {
        default_source_repo: settings.github.repo.clone(),
        default_source_ref: settings.github.reference.clone(),
        default_source_root: settings.github.root.clone(),
        ..platform_config.clone()
    };
    skill_runtime_config.limits.default_discover_result_limit = 256;
    skill_runtime_config.limits.max_discover_result_limit = 256;
    let skill_runtime =
        AutonomousSkillRuntime::new(skill_runtime_config, state.autonomous_skill_cache_dir(app)?);

    let discovered = match skill_runtime.discover(AutonomousSkillDiscoverRequest {
        query: query.to_owned(),
        result_limit: Some(256),
        timeout_ms: None,
        source_repo: Some(settings.github.repo.clone()),
        source_ref: Some(settings.github.reference.clone()),
    }) {
        Ok(discovered) => discovered,
        Err(error) => {
            diagnostics.push(skill_search_diagnostic(error, None));
            return Ok(());
        }
    };

    for candidate in discovered.candidates {
        let skill_id = candidate.skill_id.clone();
        let source = candidate.source.clone();
        match skill_runtime.resolve(AutonomousSkillResolveRequest {
            skill_id,
            timeout_ms: None,
            source_repo: Some(source.repo.clone()),
            source_ref: Some(source.reference.clone()),
        }) {
            Ok(resolved) => {
                let trust = if resolved.source.repo == platform_config.default_source_repo
                    && resolved.source.reference == platform_config.default_source_ref
                {
                    XeroSkillTrustState::Trusted
                } else {
                    XeroSkillTrustState::UserApproved
                };
                let scope = match project_id {
                    Some(project_id) => XeroSkillSourceScope::project(project_id.to_owned())?,
                    None => XeroSkillSourceScope::global(),
                };
                let source = XeroSkillSourceRecord::github_autonomous(
                    scope,
                    &resolved.source,
                    XeroSkillSourceState::Enabled,
                    trust,
                )?;
                let entry = SkillRegistryEntryDto {
                    source_id: source.source_id.clone(),
                    skill_id: resolved.skill_id,
                    name: resolved.name,
                    description: resolved.description,
                    source_kind: source_kind_dto(source.locator.kind()),
                    scope: source_scope_dto(&source.scope),
                    project_id: source_project_id(&source.scope),
                    source_state: source_state_dto(source.state),
                    trust_state: trust_state_dto(source.trust),
                    enabled: true,
                    installed: false,
                    user_invocable: resolved.user_invocable,
                    version_hash: Some(resolved.source.tree_hash.clone()),
                    last_used_at: None,
                    last_diagnostic: None,
                    source: source_metadata_dto(&source),
                };
                insert_entry(entries, entry);
            }
            Err(error) => diagnostics.push(skill_search_diagnostic(error, Some(source.path))),
        }
    }

    Ok(())
}

fn skill_search_diagnostic(
    error: CommandError,
    relative_path: Option<String>,
) -> SkillDiscoveryDiagnosticDto {
    SkillDiscoveryDiagnosticDto {
        code: error.code,
        message: error.message,
        relative_path,
    }
}

pub(crate) fn search_agent_authoring_skill_summaries(
    request: &SearchAgentAuthoringSkillsRequestDto,
) -> CommandResult<SearchAgentAuthoringSkillsResponseDto> {
    let query = request.query.as_deref().unwrap_or("").trim();
    let offset = request.offset;
    let limit = request.limit.clamp(1, 50);
    let page = fetch_skills_sh_catalog_page(query, offset, limit)?;
    let local_offset = if query.is_empty() || is_all_skills_query(query) {
        offset % SKILLS_SH_ALL_TIME_PAGE_SIZE
    } else {
        offset
    };

    let filtered = page
        .skills
        .into_iter()
        .filter(|skill| skills_sh_github_repo(&skill.source).is_some())
        .collect::<Vec<_>>();
    let entries = filtered
        .iter()
        .skip(local_offset)
        .take(limit)
        .map(skill_search_result_from_skills_sh)
        .collect::<Vec<_>>();
    let consumed = local_offset.saturating_add(entries.len());
    let total_known_has_more = page
        .count
        .map(|count| offset.saturating_add(entries.len()) < count)
        .unwrap_or(false);
    let has_more = filtered.len() > consumed
        || total_known_has_more
        || ((query.is_empty() || is_all_skills_query(query))
            && page.raw_count >= SKILLS_SH_ALL_TIME_PAGE_SIZE
            && entries.len() == limit);
    let next_offset = if has_more && !entries.is_empty() {
        Some(offset.saturating_add(entries.len()))
    } else {
        None
    };

    Ok(SearchAgentAuthoringSkillsResponseDto {
        entries,
        offset,
        limit,
        next_offset,
        has_more,
    })
}

pub(crate) fn resolve_agent_authoring_skill_registry_entry<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    request: &ResolveAgentAuthoringSkillRequestDto,
) -> CommandResult<SkillRegistryEntryDto> {
    let project_id = validate_required(request.project_id.clone(), "projectId")?;
    let source = validate_required(request.source.clone(), "source")?;
    let skill_id = validate_required(request.skill_id.clone(), "skillId")?;
    let settings = load_settings(app, state)?;
    if !settings.github.enabled {
        return Err(CommandError::user_fixable(
            "agent_authoring_skill_resolve_unavailable",
            "Online skill sources are disabled in skill settings.",
        ));
    }
    let Some(repo) = skills_sh_github_repo(&source) else {
        return Err(CommandError::invalid_request("source"));
    };

    let skill = SkillsShSkill {
        source,
        skill_id: skill_id.clone(),
        name: skill_id,
        description: String::new(),
        installs: None,
        is_official: false,
    };
    let cache_dir = state.autonomous_skill_cache_dir(app)?;
    let platform_config = AutonomousSkillRuntimeConfig::for_platform();
    let source_keys = online_source_keys(&repo, &settings);
    let mut discovered_cache = BTreeMap::new();
    let mut resolved_cache = BTreeMap::new();
    let Some(resolved) = resolve_skills_sh_skill(
        &skill,
        &source_keys,
        &platform_config,
        &cache_dir,
        &mut discovered_cache,
        &mut resolved_cache,
    ) else {
        return Err(CommandError::user_fixable(
            "agent_authoring_skill_resolve_failed",
            format!(
                "Xero could not resolve the online skill \"{}\" from {}.",
                skill.skill_id, skill.source
            ),
        ));
    };

    skill_entry_from_online_resolved(
        resolved,
        Some(&project_id),
        &platform_config,
        skill.is_official,
    )
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SkillsShCatalogResponse {
    #[serde(default)]
    skills: Vec<SkillsShSkill>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    total_skills: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SkillsShSkill {
    source: String,
    skill_id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    installs: Option<u64>,
    #[serde(default)]
    is_official: bool,
}

struct SkillsShCatalogPage {
    skills: Vec<SkillsShSkill>,
    count: Option<usize>,
    raw_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct OnlineSkillSourceKey {
    repo: String,
    reference: String,
    root: String,
}

fn fetch_skills_sh_catalog_page(
    query: &str,
    offset: usize,
    limit: usize,
) -> CommandResult<SkillsShCatalogPage> {
    let query = query.trim();
    let is_all = query.is_empty() || is_all_skills_query(query);
    let mut url = Url::parse(SKILLS_SH_API_BASE_URL).map_err(|error| {
        CommandError::system_fault(
            "skills_sh_search_unavailable",
            format!("Xero could not parse the Skills directory API URL: {error}"),
        )
    })?;
    {
        let mut segments = url.path_segments_mut().map_err(|_| {
            CommandError::system_fault(
                "skills_sh_search_unavailable",
                "Xero could not build the Skills directory API URL.",
            )
        })?;
        segments.pop_if_empty();
        if is_all {
            let page = offset / SKILLS_SH_ALL_TIME_PAGE_SIZE;
            let page_segment = page.to_string();
            segments.extend(["api", "skills", "all-time"]);
            segments.push(&page_segment);
        } else {
            segments.extend(["api", "search"]);
        }
    }
    if !is_all {
        let fetch_limit = offset.saturating_add(limit).saturating_add(1).clamp(1, 100);
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("limit", &fetch_limit.to_string());
    }

    let client = Client::builder()
        .timeout(Duration::from_millis(SKILLS_SH_REQUEST_TIMEOUT_MS))
        .build()
        .map_err(|error| {
            CommandError::system_fault(
                "skills_sh_search_unavailable",
                format!("Xero could not initialize the Skills directory client: {error}"),
            )
        })?;
    let response = client
        .get(url)
        .header(USER_AGENT, SKILLS_SH_USER_AGENT)
        .send()
        .map_err(|error| {
            CommandError::retryable(
                "skills_sh_search_unavailable",
                format!("Xero could not contact the Skills directory: {error}"),
            )
        })?;
    let status = response.status().as_u16();
    if !(200..=299).contains(&status) {
        return Err(CommandError::retryable(
            "skills_sh_search_unavailable",
            format!("Xero received HTTP {status} from the Skills directory."),
        ));
    }
    let payload = response.text().map_err(|error| {
        CommandError::retryable(
            "skills_sh_search_unavailable",
            format!("Xero could not read the Skills directory response: {error}"),
        )
    })?;
    let response = serde_json::from_str::<SkillsShCatalogResponse>(&payload).map_err(|error| {
        CommandError::retryable(
            "skills_sh_search_unavailable",
            format!("Xero could not decode the Skills directory response: {error}"),
        )
    })?;
    let raw_count = response.skills.len();
    Ok(SkillsShCatalogPage {
        skills: response.skills,
        count: response.count.or(response.total_skills),
        raw_count,
    })
}

fn skill_search_result_from_skills_sh(skill: &SkillsShSkill) -> AgentAuthoringSkillSearchResultDto {
    AgentAuthoringSkillSearchResultDto {
        source: skill.source.trim().to_owned(),
        skill_id: skill.skill_id.trim().to_owned(),
        name: if skill.name.trim().is_empty() {
            skill.skill_id.trim().to_owned()
        } else {
            skill.name.trim().to_owned()
        },
        description: skill.description.trim().to_owned(),
        installs: skill.installs,
        is_official: skill.is_official,
    }
}

fn skills_sh_github_repo(source: &str) -> Option<String> {
    let source = source.trim();
    if source.starts_with("http://") || source.starts_with("https://") {
        return None;
    }
    let mut parts = source.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if parts.next().is_some() || owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

fn online_source_keys(repo: &str, settings: &SkillSourceSettings) -> Vec<OnlineSkillSourceKey> {
    let mut keys = BTreeSet::new();
    let mut references = Vec::new();
    if repo == settings.github.repo {
        references.push(settings.github.reference.clone());
    }
    references.extend(
        ONLINE_SKILL_SOURCE_REFS
            .iter()
            .map(|value| (*value).to_owned()),
    );

    let mut roots = Vec::new();
    if repo == settings.github.repo {
        roots.push(settings.github.root.clone());
    }
    roots.extend(
        ONLINE_SKILL_SOURCE_ROOTS
            .iter()
            .map(|value| (*value).to_owned()),
    );

    for reference in references {
        for root in &roots {
            keys.insert(OnlineSkillSourceKey {
                repo: repo.to_owned(),
                reference: reference.clone(),
                root: root.clone(),
            });
        }
    }
    keys.into_iter().collect()
}

fn resolve_skills_sh_skill(
    skill: &SkillsShSkill,
    source_keys: &[OnlineSkillSourceKey],
    platform_config: &AutonomousSkillRuntimeConfig,
    cache_dir: &Path,
    discovered_cache: &mut BTreeMap<OnlineSkillSourceKey, Vec<AutonomousSkillDiscoveryCandidate>>,
    resolved_cache: &mut BTreeMap<
        (OnlineSkillSourceKey, String),
        Option<AutonomousSkillResolveOutput>,
    >,
) -> Option<AutonomousSkillResolveOutput> {
    for key in source_keys {
        let mut runtime_config = platform_config.clone();
        runtime_config.default_source_repo = key.repo.clone();
        runtime_config.default_source_ref = key.reference.clone();
        runtime_config.default_source_root = key.root.clone();
        runtime_config.limits.default_discover_result_limit = ONLINE_SKILL_DISCOVER_LIMIT;
        runtime_config.limits.max_discover_result_limit = ONLINE_SKILL_DISCOVER_LIMIT;
        let runtime = AutonomousSkillRuntime::new(runtime_config, cache_dir);

        if !discovered_cache.contains_key(key) {
            let discovered = match runtime.discover(AutonomousSkillDiscoverRequest {
                query: "*".into(),
                result_limit: Some(ONLINE_SKILL_DISCOVER_LIMIT),
                timeout_ms: None,
                source_repo: Some(key.repo.clone()),
                source_ref: Some(key.reference.clone()),
            }) {
                Ok(discovered) => discovered.candidates,
                Err(_) => Vec::new(),
            };
            discovered_cache.insert(key.clone(), discovered);
        }

        let mut candidates = discovered_cache.get(key).cloned().unwrap_or_default();
        candidates.sort_by(|left, right| {
            skills_sh_candidate_rank(skill, &left.skill_id)
                .cmp(&skills_sh_candidate_rank(skill, &right.skill_id))
                .then_with(|| left.skill_id.cmp(&right.skill_id))
        });

        for candidate in candidates {
            if let Some(resolved) =
                resolve_online_candidate(key, &runtime, &candidate, resolved_cache)
            {
                if skills_sh_resolved_matches(skill, &candidate.skill_id, &resolved) {
                    return Some(resolved);
                }
            }
        }
    }
    None
}

fn resolve_online_candidate(
    key: &OnlineSkillSourceKey,
    runtime: &AutonomousSkillRuntime,
    candidate: &AutonomousSkillDiscoveryCandidate,
    resolved_cache: &mut BTreeMap<
        (OnlineSkillSourceKey, String),
        Option<AutonomousSkillResolveOutput>,
    >,
) -> Option<AutonomousSkillResolveOutput> {
    let cache_key = (key.clone(), candidate.skill_id.clone());
    if let Some(cached) = resolved_cache.get(&cache_key) {
        return cached.clone();
    }
    let resolved = runtime
        .resolve(AutonomousSkillResolveRequest {
            skill_id: candidate.skill_id.clone(),
            timeout_ms: None,
            source_repo: Some(key.repo.clone()),
            source_ref: Some(key.reference.clone()),
        })
        .ok();
    resolved_cache.insert(cache_key, resolved.clone());
    resolved
}

fn skills_sh_candidate_rank(skill: &SkillsShSkill, candidate_skill_id: &str) -> u8 {
    let expected = skill.skill_id.to_ascii_lowercase();
    let candidate = candidate_skill_id.to_ascii_lowercase();
    if candidate == expected {
        0
    } else if expected.ends_with(&candidate) {
        1
    } else if candidate.contains(&expected) || expected.contains(&candidate) {
        2
    } else {
        3
    }
}

fn skills_sh_resolved_matches(
    skill: &SkillsShSkill,
    candidate_skill_id: &str,
    resolved: &AutonomousSkillResolveOutput,
) -> bool {
    [
        resolved.skill_id.as_str(),
        resolved.name.as_str(),
        candidate_skill_id,
    ]
    .iter()
    .any(|value| skills_sh_label_matches(value, &skill.skill_id))
        || [resolved.skill_id.as_str(), resolved.name.as_str()]
            .iter()
            .any(|value| skills_sh_label_matches(value, &skill.name))
}

fn skills_sh_label_matches(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn skill_entry_from_online_resolved(
    resolved: AutonomousSkillResolveOutput,
    project_id: Option<&str>,
    platform_config: &AutonomousSkillRuntimeConfig,
    is_official: bool,
) -> CommandResult<SkillRegistryEntryDto> {
    let trust = if is_official
        || (resolved.source.repo == platform_config.default_source_repo
            && resolved.source.reference == platform_config.default_source_ref)
    {
        XeroSkillTrustState::Trusted
    } else {
        XeroSkillTrustState::UserApproved
    };
    let scope = match project_id {
        Some(project_id) => XeroSkillSourceScope::project(project_id.to_owned())?,
        None => XeroSkillSourceScope::global(),
    };
    let source = XeroSkillSourceRecord::github_autonomous(
        scope,
        &resolved.source,
        XeroSkillSourceState::Enabled,
        trust,
    )?;
    Ok(SkillRegistryEntryDto {
        source_id: source.source_id.clone(),
        skill_id: resolved.skill_id,
        name: resolved.name,
        description: resolved.description,
        source_kind: source_kind_dto(source.locator.kind()),
        scope: source_scope_dto(&source.scope),
        project_id: source_project_id(&source.scope),
        source_state: source_state_dto(source.state),
        trust_state: trust_state_dto(source.trust),
        enabled: true,
        installed: false,
        user_invocable: resolved.user_invocable,
        version_hash: Some(resolved.source.tree_hash.clone()),
        last_used_at: None,
        last_diagnostic: None,
        source: source_metadata_dto(&source),
    })
}

fn collect_plugin_registry<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    reload_plugins: bool,
) -> CommandResult<PluginRegistrySnapshot> {
    let settings = load_settings(app, state)?;
    let repo_root = resolve_imported_repo_root(app, state, project_id)?;
    let roots = settings
        .enabled_plugin_roots()
        .into_iter()
        .map(|root| XeroPluginRoot {
            root_id: root.root_id,
            root_path: PathBuf::from(root.path),
        });
    let discovery = discover_plugin_roots(roots)?;
    let records =
        project_store::sync_discovered_plugins(&repo_root, &discovery.plugins, reload_plugins)?;
    Ok(PluginRegistrySnapshot {
        records,
        diagnostics: discovery.diagnostics,
    })
}

fn push_discovery(
    candidates: &mut Vec<XeroDiscoveredSkill>,
    diagnostics: &mut Vec<XeroSkillDiscoveryDiagnostic>,
    discovered: XeroSkillDirectoryDiscovery,
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
                bundle_id: "xero".into(),
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
    crate::runtime::load_skill_source_settings_from_path(&state.global_db_path(app)?)
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

fn apply_plugin_state_to_installed_skill(
    mut record: InstalledSkillRecord,
    plugins: &[InstalledPluginRecord],
) -> InstalledSkillRecord {
    let XeroSkillSourceLocator::Plugin { plugin_id, .. } = &record.source.locator else {
        return record;
    };
    let Some(plugin) = plugins.iter().find(|plugin| plugin.plugin_id == *plugin_id) else {
        record.source.state = XeroSkillSourceState::Stale;
        return record;
    };
    if plugin.state != XeroSkillSourceState::Enabled || plugin.trust == XeroSkillTrustState::Blocked
    {
        record.source.state = plugin.state;
        record.source.trust = plugin.trust;
        record.last_diagnostic =
            plugin
                .last_diagnostic
                .as_ref()
                .map(|diagnostic| InstalledSkillDiagnosticRecord {
                    code: diagnostic.code.clone(),
                    message: diagnostic.message.clone(),
                    retryable: diagnostic.retryable,
                    recorded_at: diagnostic.recorded_at.clone(),
                });
    }
    record
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
        enabled: source.state == XeroSkillSourceState::Enabled,
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

fn plugin_registry_entry_dto(
    record: &InstalledPluginRecord,
) -> CommandResult<PluginRegistryEntryDto> {
    let commands = project_store::plugin_command_descriptors(std::slice::from_ref(record), true)?
        .iter()
        .map(plugin_command_dto)
        .collect::<CommandResult<Vec<_>>>()?;
    let skills = record
        .manifest
        .skills
        .iter()
        .map(|skill| {
            Ok::<_, CommandError>(PluginSkillContributionDto {
                contribution_id: skill.id.clone(),
                skill_id: skill.id.clone(),
                path: skill.path.clone(),
                source_id: None,
            })
        })
        .collect::<CommandResult<Vec<_>>>()?;
    Ok(PluginRegistryEntryDto {
        plugin_id: record.plugin_id.clone(),
        name: record.name.clone(),
        version: record.version.clone(),
        description: record.description.clone(),
        root_id: record.root_id.clone(),
        root_path: record.root_path.clone(),
        plugin_root_path: record.plugin_root_path.clone(),
        manifest_path: record.manifest_path.clone(),
        manifest_hash: record.manifest_hash.clone(),
        state: source_state_dto(record.state),
        trust: trust_state_dto(record.trust),
        enabled: record.state == XeroSkillSourceState::Enabled,
        skill_count: record.manifest.skills.len(),
        command_count: record.manifest.commands.len(),
        skills,
        commands,
        last_reloaded_at: record.last_reloaded_at.clone(),
        last_diagnostic: record.last_diagnostic.as_ref().map(plugin_diagnostic_dto),
    })
}

fn plugin_command_dto(
    command: &PluginCommandRegistryRecord,
) -> CommandResult<PluginCommandContributionDto> {
    Ok(PluginCommandContributionDto {
        command_id: command.command_id.clone(),
        plugin_id: command.plugin_id.clone(),
        contribution_id: command.contribution_id.clone(),
        label: command.label.clone(),
        description: command.description.clone(),
        entry: command.entry.clone(),
        availability: match &command.availability {
            crate::runtime::XeroPluginCommandAvailability::Always => {
                PluginCommandAvailabilityDto::Always
            }
            crate::runtime::XeroPluginCommandAvailability::ProjectOpen => {
                PluginCommandAvailabilityDto::ProjectOpen
            }
        },
        risk_level: match &command.risk_level {
            crate::runtime::XeroPluginCommandRiskLevel::Observe => {
                PluginCommandRiskLevelDto::Observe
            }
            crate::runtime::XeroPluginCommandRiskLevel::ProjectRead => {
                PluginCommandRiskLevelDto::ProjectRead
            }
            crate::runtime::XeroPluginCommandRiskLevel::ProjectWrite => {
                PluginCommandRiskLevelDto::ProjectWrite
            }
            crate::runtime::XeroPluginCommandRiskLevel::RunOwned => {
                PluginCommandRiskLevelDto::RunOwned
            }
            crate::runtime::XeroPluginCommandRiskLevel::Network => {
                PluginCommandRiskLevelDto::Network
            }
            crate::runtime::XeroPluginCommandRiskLevel::SystemRead => {
                PluginCommandRiskLevelDto::SystemRead
            }
            crate::runtime::XeroPluginCommandRiskLevel::OsAutomation => {
                PluginCommandRiskLevelDto::OsAutomation
            }
            crate::runtime::XeroPluginCommandRiskLevel::SignalExternal => {
                PluginCommandRiskLevelDto::SignalExternal
            }
        },
        approval_policy: match &command.approval_policy {
            crate::runtime::XeroPluginCommandApprovalPolicy::NeverForObserveOnly => {
                PluginCommandApprovalPolicyDto::NeverForObserveOnly
            }
            crate::runtime::XeroPluginCommandApprovalPolicy::Required => {
                PluginCommandApprovalPolicyDto::Required
            }
            crate::runtime::XeroPluginCommandApprovalPolicy::PerInvocation => {
                PluginCommandApprovalPolicyDto::PerInvocation
            }
            crate::runtime::XeroPluginCommandApprovalPolicy::Blocked => {
                PluginCommandApprovalPolicyDto::Blocked
            }
        },
        state_policy: match &command.state_policy {
            crate::runtime::XeroPluginCommandStatePolicy::Ephemeral => {
                PluginCommandStatePolicyDto::Ephemeral
            }
            crate::runtime::XeroPluginCommandStatePolicy::Project => {
                PluginCommandStatePolicyDto::Project
            }
            crate::runtime::XeroPluginCommandStatePolicy::Plugin => {
                PluginCommandStatePolicyDto::Plugin
            }
            crate::runtime::XeroPluginCommandStatePolicy::External => {
                PluginCommandStatePolicyDto::External
            }
        },
        redaction_required: command.redaction_required,
        state: source_state_dto(command.state),
        trust: trust_state_dto(command.trust),
    })
}

fn plugin_diagnostic_dto(diagnostic: &InstalledPluginDiagnosticRecord) -> PluginDiagnosticDto {
    PluginDiagnosticDto {
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        retryable: diagnostic.retryable,
        recorded_at: diagnostic.recorded_at.clone(),
    }
}

fn skill_entry_from_discovered(
    candidate: &XeroDiscoveredSkill,
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
        enabled: source.state == XeroSkillSourceState::Enabled,
        installed: false,
        user_invocable: candidate.user_invocable,
        version_hash: Some(candidate.version_hash.clone()),
        last_used_at: None,
        last_diagnostic: None,
        source: source_metadata_dto(&source),
    })
}

fn source_metadata_dto(source: &XeroSkillSourceRecord) -> SkillSourceMetadataDto {
    match &source.locator {
        XeroSkillSourceLocator::Bundled {
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
        XeroSkillSourceLocator::Local {
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
        XeroSkillSourceLocator::Project { relative_path, .. } => SkillSourceMetadataDto {
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
        XeroSkillSourceLocator::Github {
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
        XeroSkillSourceLocator::Dynamic {
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
        XeroSkillSourceLocator::Mcp {
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
        XeroSkillSourceLocator::Plugin {
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
        plugin_roots: settings
            .plugin_roots
            .iter()
            .map(|root| PluginRootDto {
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
    diagnostic: XeroSkillDiscoveryDiagnostic,
) -> SkillDiscoveryDiagnosticDto {
    SkillDiscoveryDiagnosticDto {
        code: diagnostic.code,
        message: diagnostic.message,
        relative_path: diagnostic.relative_path,
    }
}

fn plugin_discovery_diagnostic_dto(
    diagnostic: XeroPluginDiscoveryDiagnostic,
) -> SkillDiscoveryDiagnosticDto {
    SkillDiscoveryDiagnosticDto {
        code: diagnostic.code,
        message: diagnostic.message,
        relative_path: diagnostic.relative_path,
    }
}

fn source_kind_dto(kind: XeroSkillSourceKind) -> SkillSourceKindDto {
    match kind {
        XeroSkillSourceKind::Bundled => SkillSourceKindDto::Bundled,
        XeroSkillSourceKind::Local => SkillSourceKindDto::Local,
        XeroSkillSourceKind::Project => SkillSourceKindDto::Project,
        XeroSkillSourceKind::Github => SkillSourceKindDto::Github,
        XeroSkillSourceKind::Dynamic => SkillSourceKindDto::Dynamic,
        XeroSkillSourceKind::Mcp => SkillSourceKindDto::Mcp,
        XeroSkillSourceKind::Plugin => SkillSourceKindDto::Plugin,
    }
}

fn source_scope_dto(scope: &XeroSkillSourceScope) -> SkillSourceScopeDto {
    match scope {
        XeroSkillSourceScope::Global => SkillSourceScopeDto::Global,
        XeroSkillSourceScope::Project { .. } => SkillSourceScopeDto::Project,
    }
}

fn source_project_id(scope: &XeroSkillSourceScope) -> Option<String> {
    match scope {
        XeroSkillSourceScope::Global => None,
        XeroSkillSourceScope::Project { project_id } => Some(project_id.clone()),
    }
}

fn source_state_dto(state: XeroSkillSourceState) -> SkillSourceStateDto {
    match state {
        XeroSkillSourceState::Discoverable => SkillSourceStateDto::Discoverable,
        XeroSkillSourceState::Installed => SkillSourceStateDto::Installed,
        XeroSkillSourceState::Enabled => SkillSourceStateDto::Enabled,
        XeroSkillSourceState::Disabled => SkillSourceStateDto::Disabled,
        XeroSkillSourceState::Stale => SkillSourceStateDto::Stale,
        XeroSkillSourceState::Failed => SkillSourceStateDto::Failed,
        XeroSkillSourceState::Blocked => SkillSourceStateDto::Blocked,
    }
}

fn trust_state_dto(trust: XeroSkillTrustState) -> SkillTrustStateDto {
    match trust {
        XeroSkillTrustState::Trusted => SkillTrustStateDto::Trusted,
        XeroSkillTrustState::UserApproved => SkillTrustStateDto::UserApproved,
        XeroSkillTrustState::ApprovalRequired => SkillTrustStateDto::ApprovalRequired,
        XeroSkillTrustState::Untrusted => SkillTrustStateDto::Untrusted,
        XeroSkillTrustState::Blocked => SkillTrustStateDto::Blocked,
    }
}

fn approve_trust_for_user_enable(trust: XeroSkillTrustState) -> XeroSkillTrustState {
    match trust {
        XeroSkillTrustState::Blocked | XeroSkillTrustState::Trusted => trust,
        XeroSkillTrustState::UserApproved
        | XeroSkillTrustState::ApprovalRequired
        | XeroSkillTrustState::Untrusted => XeroSkillTrustState::UserApproved,
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
    if query.is_empty() || is_all_skills_query(&query) {
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

fn is_all_skills_query(query: &str) -> bool {
    let query = query.trim();
    query == "*" || query.eq_ignore_ascii_case("__all__")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn skill_entry(source_id: &str) -> SkillRegistryEntryDto {
        SkillRegistryEntryDto {
            source_id: source_id.into(),
            skill_id: "skill-a".into(),
            name: "Skill A".into(),
            description: "Test skill".into(),
            source_kind: SkillSourceKindDto::Bundled,
            scope: SkillSourceScopeDto::Global,
            project_id: None,
            source_state: SkillSourceStateDto::Enabled,
            trust_state: SkillTrustStateDto::Trusted,
            enabled: true,
            installed: true,
            user_invocable: Some(true),
            version_hash: None,
            last_used_at: None,
            last_diagnostic: None,
            source: SkillSourceMetadataDto {
                label: "Skill A".into(),
                repo: None,
                reference: None,
                path: None,
                root_id: None,
                root_path: None,
                relative_path: None,
                bundle_id: None,
                plugin_id: None,
                server_id: None,
            },
        }
    }

    #[test]
    fn skill_registry_contract_validation_reports_duplicate_source_ids() {
        let diagnostics =
            validate_skill_registry_contract(&[skill_entry("source-a"), skill_entry("source-a")]);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "skill_registry_duplicate_source_id");
        assert_eq!(
            diagnostics[0].path,
            vec![
                "entries".to_string(),
                "1".to_string(),
                "sourceId".to_string()
            ]
        );
    }

    #[test]
    fn skill_registry_query_wildcard_matches_all_entries() {
        let entry = skill_entry("source-a");

        assert!(skill_entry_matches_query(&entry, "*"));
        assert!(skill_entry_matches_query(&entry, "__all__"));
        assert!(skill_entry_matches_query(&entry, "__ALL__"));
    }

    #[test]
    fn skills_sh_catalog_accepts_github_repo_sources_only() {
        assert_eq!(
            skills_sh_github_repo("vercel-labs/agent-skills").as_deref(),
            Some("vercel-labs/agent-skills")
        );
        assert_eq!(skills_sh_github_repo("example.com"), None);
        assert_eq!(skills_sh_github_repo("https://github.com/x/y"), None);
    }

    #[test]
    fn skills_sh_matching_handles_prefixed_registry_names() {
        let skill = SkillsShSkill {
            source: "vercel-labs/agent-skills".into(),
            skill_id: "vercel-react-best-practices".into(),
            name: "vercel-react-best-practices".into(),
            description: String::new(),
            installs: None,
            is_official: true,
        };
        let resolved = AutonomousSkillResolveOutput {
            skill_id: "vercel-react-best-practices".into(),
            name: "vercel-react-best-practices".into(),
            description: "React guidance".into(),
            user_invocable: Some(true),
            source: crate::runtime::AutonomousSkillSourceMetadata {
                repo: "vercel-labs/agent-skills".into(),
                path: "skills/react-best-practices".into(),
                reference: "main".into(),
                tree_hash: "0123456789abcdef0123456789abcdef01234567".into(),
            },
            asset_paths: Vec::new(),
        };

        assert_eq!(skills_sh_candidate_rank(&skill, "react-best-practices"), 1);
        assert!(skills_sh_resolved_matches(
            &skill,
            "react-best-practices",
            &resolved
        ));
    }
}
