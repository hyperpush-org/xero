use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::AuthFlowError,
    commands::{
        get_runtime_settings::runtime_settings_file_from_request, CommandError, CommandResult,
        ProviderProfileDto, ProviderProfileReadinessDto, ProviderProfileReadinessStatusDto,
        ProviderProfilesDto, ProviderProfilesMigrationDto, SetActiveProviderProfileRequestDto,
        UpsertProviderProfileRequestDto,
    },
    provider_profiles::{
        load_or_migrate_provider_profiles_from_paths, persist_provider_profiles_snapshot,
        OpenRouterProfileCredentialEntry, ProviderProfileCredentialLink,
        ProviderProfileReadinessStatus, ProviderProfileRecord, ProviderProfilesSnapshot,
    },
    runtime::{OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID},
    state::DesktopState,
};

#[tauri::command]
pub fn list_provider_profiles<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<ProviderProfilesDto> {
    let snapshot = load_provider_profiles_snapshot(&app, state.inner())?;
    Ok(provider_profiles_dto_from_snapshot(&snapshot))
}

#[tauri::command]
pub fn upsert_provider_profile<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertProviderProfileRequestDto,
) -> CommandResult<ProviderProfilesDto> {
    let provider_profiles_path = state.provider_profiles_file(&app)?;
    let provider_profile_credentials_path = state.provider_profile_credential_store_file(&app)?;
    let current = load_provider_profiles_snapshot(&app, state.inner())?;
    let next = apply_provider_profile_upsert(&current, &request)?;
    persist_provider_profiles_snapshot(
        &provider_profiles_path,
        &provider_profile_credentials_path,
        &next,
    )?;
    Ok(provider_profiles_dto_from_snapshot(&next))
}

#[tauri::command]
pub fn set_active_provider_profile<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SetActiveProviderProfileRequestDto,
) -> CommandResult<ProviderProfilesDto> {
    let provider_profiles_path = state.provider_profiles_file(&app)?;
    let provider_profile_credentials_path = state.provider_profile_credential_store_file(&app)?;
    let current = load_provider_profiles_snapshot(&app, state.inner())?;
    let next = apply_active_profile_switch(&current, &request.profile_id)?;
    persist_provider_profiles_snapshot(
        &provider_profiles_path,
        &provider_profile_credentials_path,
        &next,
    )?;
    Ok(provider_profiles_dto_from_snapshot(&next))
}

pub(crate) fn load_provider_profiles_snapshot<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<ProviderProfilesSnapshot> {
    let provider_profiles_path = state.provider_profiles_file(app)?;
    let provider_profile_credentials_path = state.provider_profile_credential_store_file(app)?;
    let legacy_settings_path = state.runtime_settings_file(app)?;
    let legacy_openrouter_credentials_path = state.openrouter_credential_file(app)?;
    let legacy_openai_auth_path = state
        .auth_store_file(app)
        .map_err(map_auth_store_error_to_command_error)?;

    load_or_migrate_provider_profiles_from_paths(
        &provider_profiles_path,
        &provider_profile_credentials_path,
        &legacy_settings_path,
        &legacy_openrouter_credentials_path,
        &legacy_openai_auth_path,
    )
}

pub(crate) fn provider_profiles_dto_from_snapshot(
    snapshot: &ProviderProfilesSnapshot,
) -> ProviderProfilesDto {
    let mut profiles = snapshot
        .metadata
        .profiles
        .iter()
        .map(|profile| provider_profile_dto(snapshot, profile))
        .collect::<Vec<_>>();
    profiles.sort_by(|left, right| left.profile_id.cmp(&right.profile_id));

    ProviderProfilesDto {
        active_profile_id: snapshot.metadata.active_profile_id.clone(),
        profiles,
        migration: snapshot.metadata.migration.as_ref().map(|migration| {
            ProviderProfilesMigrationDto {
                source: migration.source.clone(),
                migrated_at: migration.migrated_at.clone(),
                runtime_settings_updated_at: migration.runtime_settings_updated_at.clone(),
                openrouter_credentials_updated_at: migration
                    .openrouter_credentials_updated_at
                    .clone(),
                openai_auth_updated_at: migration.openai_auth_updated_at.clone(),
                openrouter_model_inferred: migration.openrouter_model_inferred,
            }
        }),
    }
}

fn provider_profile_dto(
    snapshot: &ProviderProfilesSnapshot,
    profile: &ProviderProfileRecord,
) -> ProviderProfileDto {
    let readiness = profile.readiness(&snapshot.credentials);
    ProviderProfileDto {
        profile_id: profile.profile_id.clone(),
        provider_id: profile.provider_id.clone(),
        label: profile.label.clone(),
        model_id: profile.model_id.clone(),
        active: profile.profile_id == snapshot.metadata.active_profile_id,
        readiness: ProviderProfileReadinessDto {
            ready: readiness.ready,
            status: map_readiness_status(readiness.status),
            credential_updated_at: readiness.credential_updated_at,
        },
        migrated_from_legacy: profile.migrated_from_legacy,
        migrated_at: profile.migrated_at.clone(),
    }
}

fn map_readiness_status(
    status: ProviderProfileReadinessStatus,
) -> ProviderProfileReadinessStatusDto {
    match status {
        ProviderProfileReadinessStatus::Ready => ProviderProfileReadinessStatusDto::Ready,
        ProviderProfileReadinessStatus::Missing => ProviderProfileReadinessStatusDto::Missing,
        ProviderProfileReadinessStatus::Malformed => ProviderProfileReadinessStatusDto::Malformed,
    }
}

fn apply_provider_profile_upsert(
    current: &ProviderProfilesSnapshot,
    request: &UpsertProviderProfileRequestDto,
) -> CommandResult<ProviderProfilesSnapshot> {
    let profile_id = request.profile_id.trim();
    if profile_id.is_empty() {
        return Err(CommandError::invalid_request("profileId"));
    }

    let label = request.label.trim();
    if label.is_empty() {
        return Err(CommandError::invalid_request("label"));
    }

    let validated =
        runtime_settings_file_from_request(&request.provider_id, &request.model_id, false)?;
    let now = crate::auth::now_timestamp();
    let current_profile = current.profile(profile_id).cloned();
    let current_openrouter_secret = current.openrouter_credential(profile_id).cloned();
    let requested_openrouter_key = request
        .openrouter_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let explicit_openrouter_clear = request
        .openrouter_api_key
        .as_deref()
        .is_some_and(|value| value.trim().is_empty());

    let next_openrouter_secret = if validated.provider_id == OPENROUTER_PROVIDER_ID {
        if explicit_openrouter_clear {
            None
        } else if let Some(api_key) = requested_openrouter_key {
            Some(OpenRouterProfileCredentialEntry {
                profile_id: profile_id.to_owned(),
                api_key: api_key.to_owned(),
                updated_at: openrouter_secret_updated_at(
                    current_openrouter_secret.as_ref(),
                    Some(api_key),
                ),
            })
        } else {
            current_openrouter_secret.clone()
        }
    } else {
        None
    };

    let next_credential_link =
        match validated.provider_id.as_str() {
            OPENAI_CODEX_PROVIDER_ID => current_profile.as_ref().and_then(|profile| match profile
                .credential_link
                .as_ref()
            {
                Some(ProviderProfileCredentialLink::OpenAiCodex { .. }) => {
                    profile.credential_link.clone()
                }
                _ => None,
            }),
            OPENROUTER_PROVIDER_ID => next_openrouter_secret.as_ref().map(|entry| {
                ProviderProfileCredentialLink::OpenRouter {
                    updated_at: entry.updated_at.clone(),
                }
            }),
            _ => None,
        };

    let mut next = current.clone();
    let profile_updated_at = current_profile
        .as_ref()
        .filter(|profile| {
            profile.provider_id == validated.provider_id
                && profile.label == label
                && profile.model_id == validated.model_id
                && profile.credential_link == next_credential_link
        })
        .map(|profile| profile.updated_at.clone())
        .unwrap_or_else(|| now.clone());

    upsert_profile(
        &mut next.metadata.profiles,
        ProviderProfileRecord {
            profile_id: profile_id.to_owned(),
            provider_id: validated.provider_id.clone(),
            label: label.to_owned(),
            model_id: if validated.provider_id == OPENROUTER_PROVIDER_ID {
                validated.model_id.clone()
            } else {
                OPENAI_CODEX_PROVIDER_ID.into()
            },
            credential_link: next_credential_link,
            migrated_from_legacy: current_profile
                .as_ref()
                .is_some_and(|profile| profile.migrated_from_legacy),
            migrated_at: current_profile
                .as_ref()
                .and_then(|profile| profile.migrated_at.clone()),
            updated_at: profile_updated_at,
        },
    );

    if let Some(secret) = next_openrouter_secret {
        upsert_openrouter_secret(&mut next, secret);
    } else {
        next.credentials
            .openrouter_api_keys
            .retain(|entry| entry.profile_id != profile_id);
    }

    if request.activate {
        next.metadata.active_profile_id = profile_id.to_owned();
    }

    if next == *current {
        return Ok(current.clone());
    }

    next.metadata.updated_at = now;
    Ok(next)
}

fn apply_active_profile_switch(
    current: &ProviderProfilesSnapshot,
    profile_id: &str,
) -> CommandResult<ProviderProfilesSnapshot> {
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return Err(CommandError::invalid_request("profileId"));
    }

    if !current
        .metadata
        .profiles
        .iter()
        .any(|profile| profile.profile_id == profile_id)
    {
        return Err(CommandError::user_fixable(
            "provider_profile_not_found",
            format!("Cadence could not find provider profile `{profile_id}`."),
        ));
    }

    if current.metadata.active_profile_id == profile_id {
        return Ok(current.clone());
    }

    let mut next = current.clone();
    next.metadata.active_profile_id = profile_id.to_owned();
    next.metadata.updated_at = crate::auth::now_timestamp();
    Ok(next)
}

fn upsert_profile(profiles: &mut Vec<ProviderProfileRecord>, next: ProviderProfileRecord) {
    if let Some(existing) = profiles
        .iter_mut()
        .find(|profile| profile.profile_id == next.profile_id)
    {
        *existing = next;
    } else {
        profiles.push(next);
    }
}

fn upsert_openrouter_secret(
    snapshot: &mut ProviderProfilesSnapshot,
    next: OpenRouterProfileCredentialEntry,
) {
    if let Some(existing) = snapshot
        .credentials
        .openrouter_api_keys
        .iter_mut()
        .find(|entry| entry.profile_id == next.profile_id)
    {
        *existing = next;
    } else {
        snapshot.credentials.openrouter_api_keys.push(next);
    }
}

fn openrouter_secret_updated_at(
    current: Option<&OpenRouterProfileCredentialEntry>,
    next_api_key: Option<&str>,
) -> String {
    match (current, next_api_key) {
        (Some(current), Some(next_api_key)) if current.api_key == next_api_key => {
            current.updated_at.clone()
        }
        _ => crate::auth::now_timestamp(),
    }
}

pub(crate) fn map_auth_store_error_to_command_error(error: AuthFlowError) -> CommandError {
    if error.retryable {
        CommandError::retryable(error.code, error.message)
    } else {
        CommandError::user_fixable(error.code, error.message)
    }
}
