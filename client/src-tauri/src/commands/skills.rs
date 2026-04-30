use std::{collections::BTreeMap, path::PathBuf};

use tauri::{AppHandle, Manager, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        CommandError, CommandResult, InstalledSkillDiagnosticDto, ListSkillRegistryRequestDto,
        PluginCommandApprovalPolicyDto, PluginCommandAvailabilityDto, PluginCommandContributionDto,
        PluginCommandRiskLevelDto, PluginCommandStatePolicyDto, PluginDiagnosticDto,
        PluginRegistryEntryDto, PluginRootDto, PluginSkillContributionDto, RemovePluginRequestDto,
        RemovePluginRootRequestDto, RemoveSkillLocalRootRequestDto, RemoveSkillRequestDto,
        SetPluginEnabledRequestDto, SetSkillEnabledRequestDto, SkillDiscoveryDiagnosticDto,
        SkillGithubSourceDto, SkillLocalRootDto, SkillProjectSourceDto, SkillRegistryDto,
        SkillRegistryEntryDto, SkillSourceKindDto, SkillSourceMetadataDto, SkillSourceScopeDto,
        SkillSourceSettingsDto, SkillSourceStateDto, SkillTrustStateDto,
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
        resolve_imported_repo_root, SkillSourceSettings, XeroDiscoveredSkill,
        XeroPluginDiscoveryDiagnostic, XeroPluginRoot, XeroSkillDirectoryDiscovery,
        XeroSkillDiscoveryDiagnostic, XeroSkillSourceKind, XeroSkillSourceLocator,
        XeroSkillSourceRecord, XeroSkillSourceScope, XeroSkillSourceState, XeroSkillTrustState,
    },
    state::DesktopState,
};

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

fn load_skill_registry<R: Runtime>(
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
