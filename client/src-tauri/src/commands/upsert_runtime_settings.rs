use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        provider_profiles::load_provider_profiles_snapshot, CommandError, CommandResult,
        RuntimeSettingsDto, UpsertRuntimeSettingsRequestDto,
    },
    provider_profiles::{
        build_anthropic_default_profile, build_openai_default_profile,
        build_openrouter_default_profile, persist_provider_profiles_snapshot,
        ProviderApiKeyCredentialEntry, ProviderProfileCredentialLink, ProviderProfileRecord,
        ProviderProfilesSnapshot, ANTHROPIC_DEFAULT_PROFILE_ID, OPENAI_CODEX_DEFAULT_PROFILE_ID,
        OPENROUTER_DEFAULT_PROFILE_ID, OPENROUTER_FALLBACK_MODEL_ID,
    },
    runtime::{ANTHROPIC_PROVIDER_ID, OPENROUTER_PROVIDER_ID},
    state::DesktopState,
};

use super::get_runtime_settings::runtime_settings_file_from_request;

#[tauri::command]
pub fn upsert_runtime_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertRuntimeSettingsRequestDto,
) -> CommandResult<RuntimeSettingsDto> {
    let provider_profiles_path = state.provider_profiles_file(&app)?;
    let provider_profile_credentials_path = state.provider_profile_credential_store_file(&app)?;

    let current = load_provider_profiles_snapshot(&app, state.inner())?;

    let next = apply_runtime_settings_update(&current, &request)?;
    persist_provider_profiles_snapshot(
        &provider_profiles_path,
        &provider_profile_credentials_path,
        &next,
    )?;

    runtime_settings_dto_from_provider_profiles(&next)
}

fn apply_runtime_settings_update(
    current: &ProviderProfilesSnapshot,
    request: &UpsertRuntimeSettingsRequestDto,
) -> CommandResult<ProviderProfilesSnapshot> {
    let validated_request =
        runtime_settings_file_from_request(&request.provider_id, &request.model_id, false)?;
    let now = crate::auth::now_timestamp();
    let requested_openrouter_key = request
        .openrouter_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let explicit_openrouter_clear = request
        .openrouter_api_key
        .as_deref()
        .is_some_and(|value| value.trim().is_empty());
    let requested_anthropic_key = request
        .anthropic_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let explicit_anthropic_clear = request
        .anthropic_api_key
        .as_deref()
        .is_some_and(|value| value.trim().is_empty());

    let current_openai = current.profile(OPENAI_CODEX_DEFAULT_PROFILE_ID).cloned();
    let current_openrouter = current.profile(OPENROUTER_DEFAULT_PROFILE_ID).cloned();
    let current_anthropic = current.profile(ANTHROPIC_DEFAULT_PROFILE_ID).cloned();
    let current_openrouter_secret = current
        .api_key_credential(OPENROUTER_DEFAULT_PROFILE_ID)
        .cloned();
    let current_anthropic_secret = current
        .api_key_credential(ANTHROPIC_DEFAULT_PROFILE_ID)
        .cloned();

    let next_openrouter_secret = if explicit_openrouter_clear {
        None
    } else if let Some(api_key) = requested_openrouter_key {
        Some(ProviderApiKeyCredentialEntry {
            profile_id: OPENROUTER_DEFAULT_PROFILE_ID.into(),
            api_key: api_key.to_owned(),
            updated_at: api_key_updated_at(current_openrouter_secret.as_ref(), Some(api_key)),
        })
    } else {
        current_openrouter_secret.clone()
    };
    let next_openrouter_link =
        next_openrouter_secret
            .as_ref()
            .map(|entry| ProviderProfileCredentialLink::ApiKey {
                updated_at: entry.updated_at.clone(),
            });

    let next_anthropic_secret = if explicit_anthropic_clear {
        None
    } else if let Some(api_key) = requested_anthropic_key {
        Some(ProviderApiKeyCredentialEntry {
            profile_id: ANTHROPIC_DEFAULT_PROFILE_ID.into(),
            api_key: api_key.to_owned(),
            updated_at: api_key_updated_at(current_anthropic_secret.as_ref(), Some(api_key)),
        })
    } else {
        current_anthropic_secret.clone()
    };
    let next_anthropic_link =
        next_anthropic_secret
            .as_ref()
            .map(|entry| ProviderProfileCredentialLink::ApiKey {
                updated_at: entry.updated_at.clone(),
            });

    let openai_profile = merge_profile(
        current_openai.as_ref(),
        build_openai_default_profile(
            current_openai
                .as_ref()
                .and_then(|profile| profile.credential_link.clone()),
            None,
            &now,
        ),
    );

    let include_openrouter_profile = current_openrouter.is_some()
        || validated_request.provider_id == OPENROUTER_PROVIDER_ID
        || next_openrouter_link.is_some();
    let include_anthropic_profile = current_anthropic.is_some()
        || validated_request.provider_id == ANTHROPIC_PROVIDER_ID
        || next_anthropic_link.is_some();

    let openrouter_model_id = if validated_request.provider_id == OPENROUTER_PROVIDER_ID {
        validated_request.model_id.clone()
    } else {
        current_openrouter
            .as_ref()
            .map(|profile| profile.model_id.clone())
            .unwrap_or_else(|| OPENROUTER_FALLBACK_MODEL_ID.into())
    };
    let anthropic_model_id = if validated_request.provider_id == ANTHROPIC_PROVIDER_ID {
        validated_request.model_id.clone()
    } else {
        current_anthropic
            .as_ref()
            .map(|profile| profile.model_id.clone())
            .unwrap_or_default()
    };

    let openrouter_profile = include_openrouter_profile.then(|| {
        merge_profile(
            current_openrouter.as_ref(),
            build_openrouter_default_profile(
                &openrouter_model_id,
                next_openrouter_link.clone(),
                None,
                &now,
            ),
        )
    });
    let anthropic_profile = include_anthropic_profile.then(|| {
        merge_profile(
            current_anthropic.as_ref(),
            build_anthropic_default_profile(
                &anthropic_model_id,
                next_anthropic_link.clone(),
                None,
                &now,
            ),
        )
    });

    let mut next = current.clone();
    upsert_profile(&mut next.metadata.profiles, openai_profile);
    if let Some(openrouter_profile) = openrouter_profile {
        upsert_profile(&mut next.metadata.profiles, openrouter_profile);
    }
    if let Some(anthropic_profile) = anthropic_profile {
        upsert_profile(&mut next.metadata.profiles, anthropic_profile);
    }

    next.metadata.active_profile_id = match validated_request.provider_id.as_str() {
        OPENROUTER_PROVIDER_ID => OPENROUTER_DEFAULT_PROFILE_ID.into(),
        ANTHROPIC_PROVIDER_ID => ANTHROPIC_DEFAULT_PROFILE_ID.into(),
        _ => OPENAI_CODEX_DEFAULT_PROFILE_ID.into(),
    };

    if let Some(secret) = next_openrouter_secret {
        upsert_api_key_secret(&mut next, secret);
    } else {
        next.credentials
            .api_keys
            .retain(|entry| entry.profile_id != OPENROUTER_DEFAULT_PROFILE_ID);
    }

    if let Some(secret) = next_anthropic_secret {
        upsert_api_key_secret(&mut next, secret);
    } else {
        next.credentials
            .api_keys
            .retain(|entry| entry.profile_id != ANTHROPIC_DEFAULT_PROFILE_ID);
    }

    if next == *current {
        return Ok(current.clone());
    }

    next.metadata.updated_at = now;
    Ok(next)
}

fn runtime_settings_dto_from_provider_profiles(
    provider_profiles: &ProviderProfilesSnapshot,
) -> CommandResult<RuntimeSettingsDto> {
    let active_profile = provider_profiles.active_profile().ok_or_else(|| {
        CommandError::user_fixable(
            "provider_profiles_invalid",
            "Cadence could not project runtime settings because the active provider profile was missing.",
        )
    })?;

    Ok(RuntimeSettingsDto {
        provider_id: active_profile.provider_id.clone(),
        model_id: active_profile.model_id.clone(),
        openrouter_api_key_configured: provider_profiles.any_openrouter_api_key_configured(),
        anthropic_api_key_configured: provider_profiles.any_anthropic_api_key_configured(),
    })
}

fn merge_profile(
    existing: Option<&ProviderProfileRecord>,
    mut next: ProviderProfileRecord,
) -> ProviderProfileRecord {
    if let Some(existing) = existing {
        next.label = existing.label.clone();
        next.migrated_from_legacy = existing.migrated_from_legacy;
        next.migrated_at = existing.migrated_at.clone();
        if existing.provider_id == next.provider_id
            && existing.runtime_kind == next.runtime_kind
            && existing.model_id == next.model_id
            && existing.preset_id == next.preset_id
            && existing.base_url == next.base_url
            && existing.api_version == next.api_version
            && existing.region == next.region
            && existing.project_id == next.project_id
            && existing.credential_link == next.credential_link
        {
            next.updated_at = existing.updated_at.clone();
        }
    }

    next
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
