use tauri::{AppHandle, Runtime};

use crate::{
    auth::{
        bind_anthropic_runtime_session, bind_openai_compatible_runtime_session,
        bind_openrouter_runtime_session, load_latest_openai_codex_session,
        load_openai_codex_session_for_profile_link, reconcile_anthropic_runtime_session,
        reconcile_openai_compatible_runtime_session, reconcile_openrouter_runtime_session,
        refresh_provider_auth_session, remove_openai_codex_session, sync_openai_profile_link,
        AnthropicBindOutcome, AnthropicReconcileOutcome, AnthropicRuntimeSessionBinding,
        AuthDiagnostic, AuthFlowError, OpenAiCompatibleBindOutcome,
        OpenAiCompatibleReconcileOutcome, OpenAiCompatibleRuntimeSessionBinding,
        OpenRouterBindOutcome, OpenRouterReconcileOutcome, OpenRouterRuntimeSessionBinding,
        RuntimeAuthSession,
    },
    commands::{get_runtime_settings::RuntimeSettingsSnapshot, RuntimeAuthPhase},
    provider_profiles::{ProviderProfileCredentialLink, ProviderProfilesSnapshot},
    state::DesktopState,
};

pub const OPENAI_CODEX_PROVIDER_ID: &str = "openai_codex";
pub const OPENROUTER_PROVIDER_ID: &str = "openrouter";
pub const ANTHROPIC_PROVIDER_ID: &str = "anthropic";
pub const OPENAI_API_PROVIDER_ID: &str = "openai_api";
pub const AZURE_OPENAI_PROVIDER_ID: &str = "azure_openai";
pub const GEMINI_AI_STUDIO_PROVIDER_ID: &str = "gemini_ai_studio";
pub const OPENAI_COMPATIBLE_RUNTIME_KIND: &str = "openai_compatible";
pub const GEMINI_RUNTIME_KIND: &str = "gemini";
pub const OPENAI_CODEX_AUTH_STORE_FILE_NAME: &str = "openai-auth.json";
pub const OPENROUTER_AUTH_STORE_FILE_NAME: &str = "openrouter-credentials.json";
pub const ANTHROPIC_AUTH_STORE_FILE_NAME: &str = "provider-profile-credentials.json";

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RuntimeProviderFamily {
    OpenAiCodex,
    OpenRouter,
    Anthropic,
    OpenAiCompatible,
    Gemini,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RuntimeProvider {
    OpenAiCodex,
    OpenRouter,
    Anthropic,
    OpenAiApi,
    AzureOpenAi,
    GeminiAiStudio,
}

impl RuntimeProvider {
    pub const fn family(self) -> RuntimeProviderFamily {
        match self {
            Self::OpenAiCodex => RuntimeProviderFamily::OpenAiCodex,
            Self::OpenRouter => RuntimeProviderFamily::OpenRouter,
            Self::Anthropic => RuntimeProviderFamily::Anthropic,
            Self::OpenAiApi | Self::AzureOpenAi => RuntimeProviderFamily::OpenAiCompatible,
            Self::GeminiAiStudio => RuntimeProviderFamily::Gemini,
        }
    }

    pub const fn resolve(self) -> ResolvedRuntimeProvider {
        match self {
            Self::OpenAiCodex => ResolvedRuntimeProvider {
                provider: Self::OpenAiCodex,
                family: RuntimeProviderFamily::OpenAiCodex,
                provider_id: OPENAI_CODEX_PROVIDER_ID,
                runtime_kind: OPENAI_CODEX_PROVIDER_ID,
                auth_store_file_name: OPENAI_CODEX_AUTH_STORE_FILE_NAME,
            },
            Self::OpenRouter => ResolvedRuntimeProvider {
                provider: Self::OpenRouter,
                family: RuntimeProviderFamily::OpenRouter,
                provider_id: OPENROUTER_PROVIDER_ID,
                runtime_kind: OPENROUTER_PROVIDER_ID,
                auth_store_file_name: OPENROUTER_AUTH_STORE_FILE_NAME,
            },
            Self::Anthropic => ResolvedRuntimeProvider {
                provider: Self::Anthropic,
                family: RuntimeProviderFamily::Anthropic,
                provider_id: ANTHROPIC_PROVIDER_ID,
                runtime_kind: ANTHROPIC_PROVIDER_ID,
                auth_store_file_name: ANTHROPIC_AUTH_STORE_FILE_NAME,
            },
            Self::OpenAiApi => ResolvedRuntimeProvider {
                provider: Self::OpenAiApi,
                family: RuntimeProviderFamily::OpenAiCompatible,
                provider_id: OPENAI_API_PROVIDER_ID,
                runtime_kind: OPENAI_COMPATIBLE_RUNTIME_KIND,
                auth_store_file_name: ANTHROPIC_AUTH_STORE_FILE_NAME,
            },
            Self::AzureOpenAi => ResolvedRuntimeProvider {
                provider: Self::AzureOpenAi,
                family: RuntimeProviderFamily::OpenAiCompatible,
                provider_id: AZURE_OPENAI_PROVIDER_ID,
                runtime_kind: OPENAI_COMPATIBLE_RUNTIME_KIND,
                auth_store_file_name: ANTHROPIC_AUTH_STORE_FILE_NAME,
            },
            Self::GeminiAiStudio => ResolvedRuntimeProvider {
                provider: Self::GeminiAiStudio,
                family: RuntimeProviderFamily::Gemini,
                provider_id: GEMINI_AI_STUDIO_PROVIDER_ID,
                runtime_kind: GEMINI_RUNTIME_KIND,
                auth_store_file_name: ANTHROPIC_AUTH_STORE_FILE_NAME,
            },
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ResolvedRuntimeProvider {
    pub provider: RuntimeProvider,
    pub family: RuntimeProviderFamily,
    pub provider_id: &'static str,
    pub runtime_kind: &'static str,
    pub auth_store_file_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeProviderSessionBinding {
    pub provider: ResolvedRuntimeProvider,
    pub session_id: String,
    pub account_id: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeProviderBindOutcome {
    Ready(RuntimeProviderSessionBinding),
    RefreshRequired(RuntimeProviderSessionBinding),
    SignedOut(AuthDiagnostic),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeProviderReconcileOutcome {
    Authenticated(RuntimeProviderSessionBinding),
    SignedOut(AuthDiagnostic),
}

pub const fn openai_codex_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::OpenAiCodex.resolve()
}

pub const fn openrouter_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::OpenRouter.resolve()
}

pub const fn anthropic_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::Anthropic.resolve()
}

pub const fn openai_api_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::OpenAiApi.resolve()
}

pub const fn azure_openai_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::AzureOpenAi.resolve()
}

pub const fn gemini_ai_studio_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::GeminiAiStudio.resolve()
}

pub const fn default_runtime_provider() -> ResolvedRuntimeProvider {
    openai_codex_provider()
}

pub fn resolve_runtime_provider_identity(
    provider_id: Option<&str>,
    runtime_kind: Option<&str>,
) -> Result<ResolvedRuntimeProvider, AuthDiagnostic> {
    let provider_id = normalize_input(provider_id);
    let runtime_kind = normalize_input(runtime_kind);

    let provider_from_id = provider_id.map(parse_provider_id).transpose()?;
    let runtime_from_kind = runtime_kind.map(parse_runtime_reference).transpose()?;

    match (provider_from_id, runtime_from_kind) {
        (Some(provider), Some(reference)) => {
            let resolved = provider.resolve();
            let family_matches = resolved.family == reference.family();
            let actual_matches = reference
                .actual_provider_id()
                .is_none_or(|actual_provider_id| actual_provider_id == resolved.provider_id);
            if !family_matches || !actual_matches {
                return Err(AuthDiagnostic {
                    code: "runtime_provider_mismatch".into(),
                    message: format!(
                        "Cadence rejected the runtime provider identity because providerId `{}` does not match runtimeKind `{}`.",
                        provider_id.unwrap_or_default(),
                        runtime_kind.unwrap_or_default()
                    ),
                    retryable: false,
                });
            }
            Ok(resolved)
        }
        (Some(provider), None) => Ok(provider.resolve()),
        (None, Some(RuntimeReference::Actual(provider))) => Ok(provider.resolve()),
        (None, Some(RuntimeReference::Family(_))) => Err(AuthDiagnostic {
            code: "runtime_provider_missing".into(),
            message: format!(
                "Cadence could not resolve the runtime provider because runtimeKind `{}` requires a non-blank providerId.",
                runtime_kind.unwrap_or_default()
            ),
            retryable: false,
        }),
        (None, None) => Err(AuthDiagnostic {
            code: "runtime_provider_missing".into(),
            message: "Cadence could not resolve the runtime provider because both providerId and runtimeKind were blank.".into(),
            retryable: false,
        }),
    }
}

pub(crate) fn bind_provider_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    account_id: Option<&str>,
    settings: Option<&RuntimeSettingsSnapshot>,
    provider_profiles: Option<&ProviderProfilesSnapshot>,
) -> Result<RuntimeProviderBindOutcome, AuthFlowError> {
    match provider.provider {
        RuntimeProvider::OpenAiCodex => {
            bind_openai_codex_runtime_session(app, state, provider, account_id, provider_profiles)
        }
        RuntimeProvider::OpenRouter => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    "Cadence could not bind the selected OpenRouter runtime because the app-global runtime settings snapshot was missing.",
                )
            })?;

            match bind_openrouter_runtime_session(app, state, settings)? {
                OpenRouterBindOutcome::Ready(binding) => {
                    Ok(RuntimeProviderBindOutcome::Ready(binding.into()))
                }
                OpenRouterBindOutcome::SignedOut(diagnostic) => {
                    Ok(RuntimeProviderBindOutcome::SignedOut(diagnostic))
                }
            }
        }
        RuntimeProvider::Anthropic => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    "Cadence could not bind the selected Anthropic runtime because the app-global runtime settings snapshot was missing.",
                )
            })?;

            match bind_anthropic_runtime_session(app, state, settings)? {
                AnthropicBindOutcome::Ready(binding) => {
                    Ok(RuntimeProviderBindOutcome::Ready(binding.into()))
                }
                AnthropicBindOutcome::SignedOut(diagnostic) => {
                    Ok(RuntimeProviderBindOutcome::SignedOut(diagnostic))
                }
            }
        }
        RuntimeProvider::OpenAiApi
        | RuntimeProvider::AzureOpenAi
        | RuntimeProvider::GeminiAiStudio => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    format!(
                        "Cadence could not bind the selected {} runtime because the app-global runtime settings snapshot was missing.",
                        provider.provider_id
                    ),
                )
            })?;

            match bind_openai_compatible_runtime_session(
                provider,
                settings,
                &state.openai_compatible_auth_config(),
            )? {
                OpenAiCompatibleBindOutcome::Ready(binding) => {
                    Ok(RuntimeProviderBindOutcome::Ready(binding.into()))
                }
                OpenAiCompatibleBindOutcome::SignedOut(diagnostic) => {
                    Ok(RuntimeProviderBindOutcome::SignedOut(diagnostic))
                }
            }
        }
    }
}

pub(crate) fn reconcile_provider_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    account_id: Option<&str>,
    session_id: Option<&str>,
    settings: Option<&RuntimeSettingsSnapshot>,
    provider_profiles: Option<&ProviderProfilesSnapshot>,
) -> Result<RuntimeProviderReconcileOutcome, AuthFlowError> {
    match provider.provider {
        RuntimeProvider::OpenAiCodex => reconcile_openai_codex_runtime_session(
            app,
            state,
            provider,
            account_id,
            session_id,
            provider_profiles,
        ),
        RuntimeProvider::OpenRouter => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    "Cadence could not reconcile the selected OpenRouter runtime because the app-global runtime settings snapshot was missing.",
                )
            })?;

            match reconcile_openrouter_runtime_session(
                app, state, account_id, session_id, settings,
            )? {
                OpenRouterReconcileOutcome::Authenticated(binding) => Ok(
                    RuntimeProviderReconcileOutcome::Authenticated(binding.into()),
                ),
                OpenRouterReconcileOutcome::SignedOut(diagnostic) => {
                    Ok(RuntimeProviderReconcileOutcome::SignedOut(diagnostic))
                }
            }
        }
        RuntimeProvider::Anthropic => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    "Cadence could not reconcile the selected Anthropic runtime because the app-global runtime settings snapshot was missing.",
                )
            })?;

            match reconcile_anthropic_runtime_session(app, state, account_id, session_id, settings)?
            {
                AnthropicReconcileOutcome::Authenticated(binding) => Ok(
                    RuntimeProviderReconcileOutcome::Authenticated(binding.into()),
                ),
                AnthropicReconcileOutcome::SignedOut(diagnostic) => {
                    Ok(RuntimeProviderReconcileOutcome::SignedOut(diagnostic))
                }
            }
        }
        RuntimeProvider::OpenAiApi
        | RuntimeProvider::AzureOpenAi
        | RuntimeProvider::GeminiAiStudio => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    format!(
                        "Cadence could not reconcile the selected {} runtime because the app-global runtime settings snapshot was missing.",
                        provider.provider_id
                    ),
                )
            })?;

            match reconcile_openai_compatible_runtime_session(
                provider,
                account_id,
                session_id,
                settings,
                &state.openai_compatible_auth_config(),
            )? {
                OpenAiCompatibleReconcileOutcome::Authenticated(binding) => Ok(
                    RuntimeProviderReconcileOutcome::Authenticated(binding.into()),
                ),
                OpenAiCompatibleReconcileOutcome::SignedOut(diagnostic) => {
                    Ok(RuntimeProviderReconcileOutcome::SignedOut(diagnostic))
                }
            }
        }
    }
}

pub fn refresh_provider_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    account_id: &str,
) -> Result<RuntimeProviderSessionBinding, AuthFlowError> {
    let refreshed = refresh_provider_auth_session(app, state, provider.provider, account_id)?;
    Ok(binding_from_runtime_auth_session(provider, refreshed))
}

pub fn logout_provider_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    account_id: &str,
) -> Result<(), AuthFlowError> {
    let account_id = account_id.trim();
    if account_id.is_empty() {
        return Ok(());
    }

    match provider.provider {
        RuntimeProvider::OpenAiCodex => {
            let auth_store_path = state.auth_store_file_for_provider(app, provider)?;
            remove_openai_codex_session(&auth_store_path, account_id)?;
            let latest = load_latest_openai_codex_session(&auth_store_path)?;
            sync_openai_profile_link(app, state, None, latest.as_ref())
        }
        RuntimeProvider::OpenRouter
        | RuntimeProvider::Anthropic
        | RuntimeProvider::OpenAiApi
        | RuntimeProvider::AzureOpenAi
        | RuntimeProvider::GeminiAiStudio => Ok(()),
    }
}

fn bind_openai_codex_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    _account_id: Option<&str>,
    provider_profiles: Option<&ProviderProfilesSnapshot>,
) -> Result<RuntimeProviderBindOutcome, AuthFlowError> {
    let profile = active_openai_profile(provider_profiles)?;
    let auth_store_path = state.auth_store_file_for_provider(app, provider)?;
    let link = profile.credential_link.as_ref().ok_or_else(|| {
        AuthFlowError::terminal(
            "auth_session_not_found",
            RuntimeAuthPhase::Idle,
            format!(
                "Cadence does not have an app-local OpenAI auth session linked to active provider profile `{}`.",
                profile.profile_id
            ),
        )
    })?;

    let Some(stored) = load_openai_codex_session_for_profile_link(&auth_store_path, link)? else {
        return Ok(RuntimeProviderBindOutcome::SignedOut(AuthDiagnostic {
            code: "auth_session_not_found".into(),
            message: format!(
                "Cadence does not have an app-local OpenAI auth session linked to active provider profile `{}`.",
                profile.profile_id
            ),
            retryable: false,
        }));
    };

    let binding = binding_from_stored_openai_session(
        provider,
        &stored.session_id,
        &stored.account_id,
        &stored.updated_at,
    );
    if stored.expires_at <= current_unix_timestamp() {
        return Ok(RuntimeProviderBindOutcome::RefreshRequired(binding));
    }

    Ok(RuntimeProviderBindOutcome::Ready(binding))
}

fn reconcile_openai_codex_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    account_id: Option<&str>,
    session_id: Option<&str>,
    provider_profiles: Option<&ProviderProfilesSnapshot>,
) -> Result<RuntimeProviderReconcileOutcome, AuthFlowError> {
    let account_id = normalize_input(account_id).ok_or_else(|| {
        AuthFlowError::terminal(
            "runtime_account_missing",
            RuntimeAuthPhase::Failed,
            "Cadence could not reconcile the authenticated runtime session because the bound account id was missing.",
        )
    })?;
    let session_id = normalize_input(session_id).ok_or_else(|| {
        AuthFlowError::terminal(
            "auth_session_stale",
            RuntimeAuthPhase::Failed,
            "Cadence could not reconcile the authenticated runtime session because the bound session id was missing.",
        )
    })?;

    let profile = active_openai_profile(provider_profiles)?;
    let link = profile.credential_link.as_ref().ok_or_else(|| {
        AuthFlowError::terminal(
            "auth_session_stale",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence rejected the authenticated OpenAI runtime because active provider profile `{}` is no longer linked to an auth session. Rebind the runtime session.",
                profile.profile_id
            ),
        )
    })?;

    let ProviderProfileCredentialLink::OpenAiCodex {
        account_id: linked_account_id,
        session_id: linked_session_id,
        ..
    } = link
    else {
        return Err(AuthFlowError::terminal(
            "provider_profiles_invalid",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence rejected active provider profile `{}` because OpenAI runtime reconciliation requires an OpenAI auth link.",
                profile.profile_id
            ),
        ));
    };

    if linked_account_id != account_id || linked_session_id != session_id {
        return Ok(RuntimeProviderReconcileOutcome::SignedOut(AuthDiagnostic {
            code: "auth_session_stale".into(),
            message: format!(
                "Cadence rejected the authenticated OpenAI runtime binding because active provider profile `{}` now points to a different auth session. Rebind the runtime session.",
                profile.profile_id
            ),
            retryable: false,
        }));
    }

    let auth_store_path = state.auth_store_file_for_provider(app, provider)?;
    let stored = match load_openai_codex_session_for_profile_link(&auth_store_path, link)? {
        Some(stored) => stored,
        None => {
            return Ok(RuntimeProviderReconcileOutcome::SignedOut(AuthDiagnostic {
                code: "auth_session_not_found".into(),
                message: format!(
                    "Cadence no longer has the app-local OpenAI auth session linked to active provider profile `{}`.",
                    profile.profile_id
                ),
                retryable: false,
            }))
        }
    };

    Ok(RuntimeProviderReconcileOutcome::Authenticated(
        binding_from_stored_openai_session(
            provider,
            &stored.session_id,
            &stored.account_id,
            &stored.updated_at,
        ),
    ))
}

fn active_openai_profile(
    provider_profiles: Option<&ProviderProfilesSnapshot>,
) -> Result<&crate::provider_profiles::ProviderProfileRecord, AuthFlowError> {
    let provider_profiles = provider_profiles.ok_or_else(|| {
        AuthFlowError::terminal(
            "provider_profiles_missing",
            RuntimeAuthPhase::Failed,
            "Cadence could not resolve the active OpenAI provider profile because the provider-profile snapshot was missing.",
        )
    })?;

    let profile = provider_profiles.active_profile().ok_or_else(|| {
        AuthFlowError::terminal(
            "provider_profiles_invalid",
            RuntimeAuthPhase::Failed,
            "Cadence could not resolve the active OpenAI provider profile because activeProfileId did not match a stored profile.",
        )
    })?;

    if profile.provider_id != OPENAI_CODEX_PROVIDER_ID {
        return Err(AuthFlowError::terminal(
            "runtime_provider_mismatch",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence rejected OpenAI runtime reconciliation because active provider profile `{}` belongs to provider `{}` instead of `{}`.",
                profile.profile_id, profile.provider_id, OPENAI_CODEX_PROVIDER_ID
            ),
        ));
    }

    Ok(profile)
}

fn binding_from_stored_openai_session(
    provider: ResolvedRuntimeProvider,
    session_id: &str,
    account_id: &str,
    updated_at: &str,
) -> RuntimeProviderSessionBinding {
    RuntimeProviderSessionBinding {
        provider,
        session_id: session_id.to_owned(),
        account_id: account_id.to_owned(),
        updated_at: updated_at.to_owned(),
    }
}

fn binding_from_runtime_auth_session(
    provider: ResolvedRuntimeProvider,
    session: RuntimeAuthSession,
) -> RuntimeProviderSessionBinding {
    RuntimeProviderSessionBinding {
        provider,
        session_id: session.session_id,
        account_id: session.account_id,
        updated_at: session.updated_at,
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum RuntimeReference {
    Actual(RuntimeProvider),
    Family(RuntimeProviderFamily),
}

impl RuntimeReference {
    const fn family(self) -> RuntimeProviderFamily {
        match self {
            Self::Actual(provider) => provider.family(),
            Self::Family(family) => family,
        }
    }

    const fn actual_provider_id(self) -> Option<&'static str> {
        match self {
            Self::Actual(provider) => Some(provider.resolve().provider_id),
            Self::Family(_) => None,
        }
    }
}

fn parse_provider_id(value: &str) -> Result<RuntimeProvider, AuthDiagnostic> {
    match value {
        OPENAI_CODEX_PROVIDER_ID => Ok(RuntimeProvider::OpenAiCodex),
        OPENROUTER_PROVIDER_ID => Ok(RuntimeProvider::OpenRouter),
        ANTHROPIC_PROVIDER_ID => Ok(RuntimeProvider::Anthropic),
        OPENAI_API_PROVIDER_ID => Ok(RuntimeProvider::OpenAiApi),
        AZURE_OPENAI_PROVIDER_ID => Ok(RuntimeProvider::AzureOpenAi),
        GEMINI_AI_STUDIO_PROVIDER_ID => Ok(RuntimeProvider::GeminiAiStudio),
        other => Err(unknown_runtime_provider_diagnostic(other)),
    }
}

fn parse_runtime_reference(value: &str) -> Result<RuntimeReference, AuthDiagnostic> {
    match value {
        OPENAI_CODEX_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::OpenAiCodex)),
        OPENROUTER_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::OpenRouter)),
        ANTHROPIC_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::Anthropic)),
        OPENAI_API_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::OpenAiApi)),
        AZURE_OPENAI_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::AzureOpenAi)),
        GEMINI_AI_STUDIO_PROVIDER_ID => {
            Ok(RuntimeReference::Actual(RuntimeProvider::GeminiAiStudio))
        }
        OPENAI_COMPATIBLE_RUNTIME_KIND => {
            Ok(RuntimeReference::Family(RuntimeProviderFamily::OpenAiCompatible))
        }
        GEMINI_RUNTIME_KIND => Ok(RuntimeReference::Family(RuntimeProviderFamily::Gemini)),
        other => Err(unknown_runtime_provider_diagnostic(other)),
    }
}

fn unknown_runtime_provider_diagnostic(value: &str) -> AuthDiagnostic {
    AuthDiagnostic {
        code: "runtime_provider_unknown".into(),
        message: format!(
            "Cadence does not support runtime provider `{value}`. Allowed providers: {OPENAI_CODEX_PROVIDER_ID}, {OPENROUTER_PROVIDER_ID}, {ANTHROPIC_PROVIDER_ID}, {OPENAI_API_PROVIDER_ID}, {AZURE_OPENAI_PROVIDER_ID}, {GEMINI_AI_STUDIO_PROVIDER_ID}. Allowed runtime kinds: {OPENAI_CODEX_PROVIDER_ID}, {OPENROUTER_PROVIDER_ID}, {ANTHROPIC_PROVIDER_ID}, {OPENAI_COMPATIBLE_RUNTIME_KIND}, {GEMINI_RUNTIME_KIND}."
        ),
        retryable: false,
    }
}

fn normalize_input(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}

impl From<OpenAiCompatibleRuntimeSessionBinding> for RuntimeProviderSessionBinding {
    fn from(binding: OpenAiCompatibleRuntimeSessionBinding) -> Self {
        let provider = resolve_runtime_provider_identity(
            Some(binding.provider_id.as_str()),
            Some(binding.provider_id.as_str()),
        )
        .expect("openai-compatible binding provider id should resolve");

        Self {
            provider,
            session_id: binding.session_id,
            account_id: binding.account_id,
            updated_at: binding.updated_at,
        }
    }
}

impl From<OpenRouterRuntimeSessionBinding> for RuntimeProviderSessionBinding {
    fn from(binding: OpenRouterRuntimeSessionBinding) -> Self {
        Self {
            provider: openrouter_provider(),
            session_id: binding.session_id,
            account_id: binding.account_id,
            updated_at: binding.updated_at,
        }
    }
}

impl From<AnthropicRuntimeSessionBinding> for RuntimeProviderSessionBinding {
    fn from(binding: AnthropicRuntimeSessionBinding) -> Self {
        Self {
            provider: anthropic_provider(),
            session_id: binding.session_id,
            account_id: binding.account_id,
            updated_at: binding.updated_at,
        }
    }
}
