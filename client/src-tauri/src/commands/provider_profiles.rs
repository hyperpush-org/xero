use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::{clear_openai_codex_sessions, sync_openai_profile_link, AuthFlowError},
    commands::{
        get_runtime_settings::runtime_settings_file_from_request, CommandError, CommandResult,
        LogoutProviderProfileRequestDto, ProviderProfileDto, ProviderProfileReadinessDto,
        ProviderProfileReadinessProofDto, ProviderProfileReadinessStatusDto, ProviderProfilesDto,
        ProviderProfilesMigrationDto, SetActiveProviderProfileRequestDto,
        UpsertProviderProfileRequestDto,
    },
    provider_profiles::{
        load_or_migrate_provider_profiles_from_paths, persist_provider_profiles_snapshot,
        ProviderApiKeyCredentialEntry, ProviderProfileCredentialLink,
        ProviderProfileReadinessProof, ProviderProfileReadinessStatus, ProviderProfileRecord,
        ProviderProfilesSnapshot,
    },
    runtime::{
        normalize_openai_codex_model_id, openai_codex_provider, resolve_runtime_provider_identity,
        BEDROCK_PROVIDER_ID, OLLAMA_PROVIDER_ID, OPENAI_API_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID,
        VERTEX_PROVIDER_ID,
    },
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

#[tauri::command]
pub fn logout_provider_profile<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: LogoutProviderProfileRequestDto,
) -> CommandResult<ProviderProfilesDto> {
    let profile_id = request.profile_id.trim();
    if profile_id.is_empty() {
        return Err(CommandError::invalid_request("profileId"));
    }

    let current = load_provider_profiles_snapshot(&app, state.inner())?;
    let profile = current.profile(profile_id).ok_or_else(|| {
        CommandError::user_fixable(
            "provider_profile_not_found",
            format!("Cadence could not find provider profile `{profile_id}`."),
        )
    })?;

    if profile.provider_id != OPENAI_CODEX_PROVIDER_ID {
        return Err(CommandError::user_fixable(
            "provider_profile_logout_unavailable",
            format!(
                "Cadence can only sign out browser-auth provider profiles. Profile `{profile_id}` belongs to provider `{}`.",
                profile.provider_id
            ),
        ));
    }

    if let Some(link) = profile.credential_link.as_ref() {
        if !matches!(link, ProviderProfileCredentialLink::OpenAiCodex { .. }) {
            return Err(CommandError::user_fixable(
                "provider_profiles_invalid",
                format!(
                    "Cadence rejected provider profile `{profile_id}` because the OpenAI profile uses a non-OpenAI credential link."
                ),
            ));
        }
    }

    let auth_store_path = state
        .auth_store_file_for_provider(&app, openai_codex_provider())
        .map_err(map_auth_store_error_to_command_error)?;
    clear_openai_codex_sessions(&auth_store_path).map_err(map_auth_store_error_to_command_error)?;
    sync_openai_profile_link(&app, state.inner(), Some(profile_id), None)
        .map_err(map_auth_store_error_to_command_error)?;

    let next = load_provider_profiles_snapshot(&app, state.inner())?;
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
        runtime_kind: profile.runtime_kind.clone(),
        label: profile.label.clone(),
        model_id: profile.model_id.clone(),
        preset_id: profile.preset_id.clone(),
        base_url: profile.base_url.clone(),
        api_version: profile.api_version.clone(),
        region: profile.region.clone(),
        project_id: profile.project_id.clone(),
        active: profile.profile_id == snapshot.metadata.active_profile_id,
        readiness: ProviderProfileReadinessDto {
            ready: readiness.ready,
            status: map_readiness_status(readiness.status),
            proof: readiness.proof.map(map_readiness_proof),
            proof_updated_at: readiness.proof_updated_at,
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

fn map_readiness_proof(proof: ProviderProfileReadinessProof) -> ProviderProfileReadinessProofDto {
    match proof {
        ProviderProfileReadinessProof::OAuthSession => {
            ProviderProfileReadinessProofDto::OAuthSession
        }
        ProviderProfileReadinessProof::StoredSecret => {
            ProviderProfileReadinessProofDto::StoredSecret
        }
        ProviderProfileReadinessProof::Local => ProviderProfileReadinessProofDto::Local,
        ProviderProfileReadinessProof::Ambient => ProviderProfileReadinessProofDto::Ambient,
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

    let provider_id = request.provider_id.trim();
    if provider_id.is_empty() {
        return Err(CommandError::invalid_request("providerId"));
    }

    let runtime_kind = request.runtime_kind.trim();
    if runtime_kind.is_empty() {
        return Err(CommandError::invalid_request("runtimeKind"));
    }

    let label = request.label.trim();
    if label.is_empty() {
        return Err(CommandError::invalid_request("label"));
    }

    let model_id = request.model_id.trim();
    if model_id.is_empty() {
        return Err(CommandError::invalid_request("modelId"));
    }

    let provider = resolve_runtime_provider_identity(Some(provider_id), Some(runtime_kind))
        .map_err(|diagnostic| {
            CommandError::user_fixable("provider_profiles_invalid", diagnostic.message)
        })?;

    if provider.provider_id == OPENAI_CODEX_PROVIDER_ID {
        let _ = runtime_settings_file_from_request(provider_id, model_id, false)?;
        if request
            .api_key
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(CommandError::invalid_request("apiKey"));
        }
    }

    let normalized_model_id = if provider.provider_id == OPENAI_CODEX_PROVIDER_ID {
        normalize_openai_codex_model_id(model_id)
    } else {
        model_id.to_owned()
    };

    let supports_api_key = !matches!(
        provider.provider_id,
        "openai_codex" | "ollama" | "bedrock" | "vertex"
    );
    if !supports_api_key
        && request
            .api_key
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(CommandError::invalid_request("apiKey"));
    }

    let now = crate::auth::now_timestamp();
    let current_profile = current.profile(profile_id).cloned();
    let current_api_key_secret = current.api_key_credential(profile_id).cloned();
    let current_openai_auth_link =
        current.metadata.profiles.iter().find_map(|profile| {
            match profile.credential_link.as_ref() {
                Some(ProviderProfileCredentialLink::OpenAiCodex { .. })
                    if profile.provider_id == OPENAI_CODEX_PROVIDER_ID =>
                {
                    profile.credential_link.clone()
                }
                _ => None,
            }
        });
    let requested_api_key = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let explicit_api_key_clear = request
        .api_key
        .as_deref()
        .is_some_and(|value| value.trim().is_empty());

    if explicit_api_key_clear
        && current_profile.is_none()
        && provider.provider_id != OPENAI_CODEX_PROVIDER_ID
    {
        return Err(CommandError::invalid_request("apiKey"));
    }

    let next_api_key_secret = if !supports_api_key
        || provider.provider_id == OPENAI_CODEX_PROVIDER_ID
        || explicit_api_key_clear
    {
        None
    } else if let Some(api_key) = requested_api_key {
        Some(ProviderApiKeyCredentialEntry {
            profile_id: profile_id.to_owned(),
            api_key: api_key.to_owned(),
            updated_at: api_key_updated_at(current_api_key_secret.as_ref(), Some(api_key)),
        })
    } else if current_profile
        .as_ref()
        .is_some_and(|profile| profile.provider_id != OPENAI_CODEX_PROVIDER_ID)
    {
        current_api_key_secret.clone()
    } else {
        None
    };

    let next_credential_link = next_provider_profile_credential_link(
        provider.provider_id,
        current_profile.as_ref(),
        next_api_key_secret.as_ref(),
        request.base_url.as_deref(),
        current_openai_auth_link.as_ref(),
    );

    let mut next = current.clone();
    let next_profile = ProviderProfileRecord {
        profile_id: profile_id.to_owned(),
        provider_id: provider.provider_id.to_owned(),
        runtime_kind: provider.runtime_kind.to_owned(),
        label: label.to_owned(),
        model_id: normalized_model_id.clone(),
        preset_id: normalize_optional_text(request.preset_id.clone()),
        base_url: normalize_optional_text(request.base_url.clone()),
        api_version: normalize_optional_text(request.api_version.clone()),
        region: normalize_optional_text(request.region.clone()),
        project_id: normalize_optional_text(request.project_id.clone()),
        credential_link: next_credential_link,
        migrated_from_legacy: current_profile
            .as_ref()
            .is_some_and(|profile| profile.migrated_from_legacy),
        migrated_at: current_profile
            .as_ref()
            .and_then(|profile| profile.migrated_at.clone()),
        updated_at: current_profile
            .as_ref()
            .filter(|profile| {
                profile.provider_id == provider.provider_id
                    && profile.runtime_kind == provider.runtime_kind
                    && profile.label == label
                    && profile.model_id == normalized_model_id
                    && profile.preset_id == normalize_optional_text(request.preset_id.clone())
                    && profile.base_url == normalize_optional_text(request.base_url.clone())
                    && profile.api_version == normalize_optional_text(request.api_version.clone())
                    && profile.region == normalize_optional_text(request.region.clone())
                    && profile.project_id == normalize_optional_text(request.project_id.clone())
                    && profile.credential_link
                        == next_provider_profile_credential_link(
                            provider.provider_id,
                            current_profile.as_ref(),
                            next_api_key_secret.as_ref(),
                            request.base_url.as_deref(),
                            current_openai_auth_link.as_ref(),
                        )
            })
            .map(|profile| profile.updated_at.clone())
            .unwrap_or_else(|| now.clone()),
    };

    upsert_profile(&mut next.metadata.profiles, next_profile);

    if let Some(secret) = next_api_key_secret {
        upsert_api_key_secret(&mut next, secret);
    } else {
        next.credentials
            .api_keys
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

fn next_provider_profile_credential_link(
    provider_id: &str,
    current_profile: Option<&ProviderProfileRecord>,
    next_api_key_secret: Option<&ProviderApiKeyCredentialEntry>,
    base_url: Option<&str>,
    current_openai_auth_link: Option<&ProviderProfileCredentialLink>,
) -> Option<ProviderProfileCredentialLink> {
    if provider_id == OPENAI_CODEX_PROVIDER_ID {
        return current_profile
            .and_then(|profile| match profile.credential_link.as_ref() {
                Some(ProviderProfileCredentialLink::OpenAiCodex { .. }) => {
                    profile.credential_link.clone()
                }
                _ => None,
            })
            .or_else(|| current_openai_auth_link.cloned());
    }

    if provider_uses_local_readiness(provider_id, base_url) && next_api_key_secret.is_none() {
        let updated_at = current_profile
            .and_then(|profile| match profile.credential_link.as_ref() {
                Some(ProviderProfileCredentialLink::Local { updated_at }) => {
                    Some(updated_at.clone())
                }
                _ => None,
            })
            .unwrap_or_else(crate::auth::now_timestamp);
        return Some(ProviderProfileCredentialLink::Local { updated_at });
    }

    if provider_uses_ambient_readiness(provider_id) {
        let updated_at = current_profile
            .and_then(|profile| match profile.credential_link.as_ref() {
                Some(ProviderProfileCredentialLink::Ambient { updated_at }) => {
                    Some(updated_at.clone())
                }
                _ => None,
            })
            .unwrap_or_else(crate::auth::now_timestamp);
        return Some(ProviderProfileCredentialLink::Ambient { updated_at });
    }

    next_api_key_secret.map(|entry| ProviderProfileCredentialLink::ApiKey {
        updated_at: entry.updated_at.clone(),
    })
}

fn provider_uses_local_readiness(provider_id: &str, base_url: Option<&str>) -> bool {
    provider_id == OLLAMA_PROVIDER_ID
        || (provider_id == OPENAI_API_PROVIDER_ID && base_url.is_some_and(is_local_openai_base_url))
}

fn provider_uses_ambient_readiness(provider_id: &str) -> bool {
    matches!(provider_id, BEDROCK_PROVIDER_ID | VERTEX_PROVIDER_ID)
}

fn is_local_openai_base_url(base_url: &str) -> bool {
    url::Url::parse(base_url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_ascii_lowercase()))
        .is_some_and(|host| matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"))
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

fn upsert_api_key_secret(
    snapshot: &mut ProviderProfilesSnapshot,
    next: ProviderApiKeyCredentialEntry,
) {
    if let Some(existing) = snapshot
        .credentials
        .api_keys
        .iter_mut()
        .find(|entry| entry.profile_id == next.profile_id)
    {
        *existing = next;
    } else {
        snapshot.credentials.api_keys.push(next);
    }
}

fn api_key_updated_at(
    current: Option<&ProviderApiKeyCredentialEntry>,
    next_api_key: Option<&str>,
) -> String {
    match (current, next_api_key) {
        (Some(current), Some(next_api_key)) if current.api_key == next_api_key => {
            current.updated_at.clone()
        }
        _ => crate::auth::now_timestamp(),
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

pub(crate) fn map_auth_store_error_to_command_error(error: AuthFlowError) -> CommandError {
    if error.retryable {
        CommandError::retryable(error.code, error.message)
    } else {
        CommandError::user_fixable(error.code, error.message)
    }
}
