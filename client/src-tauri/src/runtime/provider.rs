use tauri::{AppHandle, Runtime};

use crate::{
    auth::{
        bind_openrouter_runtime_session, load_latest_openai_codex_session,
        load_openai_codex_session, reconcile_openrouter_runtime_session,
        refresh_provider_auth_session, remove_openai_codex_session, AuthDiagnostic,
        AuthFlowError, OpenRouterBindOutcome, OpenRouterReconcileOutcome,
        OpenRouterRuntimeSessionBinding, RuntimeAuthSession,
    },
    commands::get_runtime_settings::RuntimeSettingsSnapshot,
    state::DesktopState,
};

pub const OPENAI_CODEX_PROVIDER_ID: &str = "openai_codex";
pub const OPENROUTER_PROVIDER_ID: &str = "openrouter";
pub const OPENAI_CODEX_AUTH_STORE_FILE_NAME: &str = "openai-auth.json";
pub const OPENROUTER_AUTH_STORE_FILE_NAME: &str = "openrouter-credentials.json";

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RuntimeProvider {
    OpenAiCodex,
    OpenRouter,
}

impl RuntimeProvider {
    pub const fn resolve(self) -> ResolvedRuntimeProvider {
        match self {
            Self::OpenAiCodex => ResolvedRuntimeProvider {
                provider: Self::OpenAiCodex,
                provider_id: OPENAI_CODEX_PROVIDER_ID,
                runtime_kind: OPENAI_CODEX_PROVIDER_ID,
                auth_store_file_name: OPENAI_CODEX_AUTH_STORE_FILE_NAME,
            },
            Self::OpenRouter => ResolvedRuntimeProvider {
                provider: Self::OpenRouter,
                provider_id: OPENROUTER_PROVIDER_ID,
                runtime_kind: OPENROUTER_PROVIDER_ID,
                auth_store_file_name: OPENROUTER_AUTH_STORE_FILE_NAME,
            },
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ResolvedRuntimeProvider {
    pub provider: RuntimeProvider,
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

pub const fn default_runtime_provider() -> ResolvedRuntimeProvider {
    openai_codex_provider()
}

pub fn resolve_runtime_provider_identity(
    provider_id: Option<&str>,
    runtime_kind: Option<&str>,
) -> Result<ResolvedRuntimeProvider, AuthDiagnostic> {
    let provider_id = normalize_input(provider_id);
    let runtime_kind = normalize_input(runtime_kind);

    let provider_from_id = provider_id.map(parse_provider).transpose()?;
    let provider_from_kind = runtime_kind.map(parse_provider).transpose()?;

    match (provider_from_id, provider_from_kind) {
        (Some(provider_from_id), Some(provider_from_kind)) if provider_from_id != provider_from_kind => {
            Err(AuthDiagnostic {
                code: "runtime_provider_mismatch".into(),
                message: format!(
                    "Cadence rejected the runtime provider identity because providerId `{}` does not match runtimeKind `{}`.",
                    provider_id.unwrap_or_default(),
                    runtime_kind.unwrap_or_default()
                ),
                retryable: false,
            })
        }
        (Some(provider), _) | (_, Some(provider)) => Ok(provider.resolve()),
        (None, None) => Err(AuthDiagnostic {
            code: "runtime_provider_missing".into(),
            message: "Cadence could not resolve the runtime provider because both providerId and runtimeKind were blank.".into(),
            retryable: false,
        }),
    }
}

pub fn bind_provider_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    account_id: Option<&str>,
    settings: Option<&RuntimeSettingsSnapshot>,
) -> Result<RuntimeProviderBindOutcome, AuthFlowError> {
    match provider.provider {
        RuntimeProvider::OpenAiCodex => {
            bind_openai_codex_runtime_session(app, state, provider, account_id)
        }
        RuntimeProvider::OpenRouter => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    crate::commands::RuntimeAuthPhase::Failed,
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
    }
}

pub fn reconcile_provider_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    account_id: Option<&str>,
    session_id: Option<&str>,
    settings: Option<&RuntimeSettingsSnapshot>,
) -> Result<RuntimeProviderReconcileOutcome, AuthFlowError> {
    match provider.provider {
        RuntimeProvider::OpenAiCodex => {
            reconcile_openai_codex_runtime_session(app, state, provider, account_id, session_id)
        }
        RuntimeProvider::OpenRouter => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    crate::commands::RuntimeAuthPhase::Failed,
                    "Cadence could not reconcile the selected OpenRouter runtime because the app-global runtime settings snapshot was missing.",
                )
            })?;

            match reconcile_openrouter_runtime_session(app, state, account_id, session_id, settings)? {
                OpenRouterReconcileOutcome::Authenticated(binding) => {
                    Ok(RuntimeProviderReconcileOutcome::Authenticated(binding.into()))
                }
                OpenRouterReconcileOutcome::SignedOut(diagnostic) => {
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
            remove_openai_codex_session(&auth_store_path, account_id)
        }
        RuntimeProvider::OpenRouter => Ok(()),
    }
}

fn bind_openai_codex_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    account_id: Option<&str>,
) -> Result<RuntimeProviderBindOutcome, AuthFlowError> {
    let auth_store_path = state.auth_store_file_for_provider(app, provider)?;
    let stored = match normalize_input(account_id) {
        Some(account_id) => load_openai_codex_session(&auth_store_path, account_id)?,
        None => load_latest_openai_codex_session(&auth_store_path)?,
    };

    let Some(stored) = stored else {
        return Ok(RuntimeProviderBindOutcome::SignedOut(AuthDiagnostic {
            code: "auth_session_not_found".into(),
            message: "Cadence does not have an app-local OpenAI auth session for the selected runtime provider.".into(),
            retryable: false,
        }));
    };

    let binding = binding_from_stored_openai_session(provider, &stored.session_id, &stored.account_id, &stored.updated_at);
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
) -> Result<RuntimeProviderReconcileOutcome, AuthFlowError> {
    let account_id = normalize_input(account_id).ok_or_else(|| {
        AuthFlowError::terminal(
            "runtime_account_missing",
            crate::commands::RuntimeAuthPhase::Failed,
            "Cadence could not reconcile the authenticated runtime session because the bound account id was missing.",
        )
    })?;
    let session_id = normalize_input(session_id).ok_or_else(|| {
        AuthFlowError::terminal(
            "auth_session_stale",
            crate::commands::RuntimeAuthPhase::Failed,
            "Cadence could not reconcile the authenticated runtime session because the bound session id was missing.",
        )
    })?;

    let auth_store_path = state.auth_store_file_for_provider(app, provider)?;
    let stored = match load_openai_codex_session(&auth_store_path, account_id)? {
        Some(stored) => stored,
        None => {
            return Ok(RuntimeProviderReconcileOutcome::SignedOut(AuthDiagnostic {
                code: "auth_session_not_found".into(),
                message: format!(
                    "Cadence no longer has an app-local OpenAI auth session for account `{account_id}`."
                ),
                retryable: false,
            }))
        }
    };

    if stored.session_id != session_id {
        return Ok(RuntimeProviderReconcileOutcome::SignedOut(AuthDiagnostic {
            code: "auth_session_stale".into(),
            message: format!(
                "Cadence rejected the authenticated OpenAI runtime binding because the saved app-local auth session for account `{account_id}` changed. Rebind the runtime session."
            ),
            retryable: false,
        }));
    }

    Ok(RuntimeProviderReconcileOutcome::Authenticated(
        binding_from_stored_openai_session(provider, &stored.session_id, &stored.account_id, &stored.updated_at),
    ))
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

fn parse_provider(value: &str) -> Result<RuntimeProvider, AuthDiagnostic> {
    match value {
        OPENAI_CODEX_PROVIDER_ID => Ok(RuntimeProvider::OpenAiCodex),
        OPENROUTER_PROVIDER_ID => Ok(RuntimeProvider::OpenRouter),
        other => Err(AuthDiagnostic {
            code: "runtime_provider_unknown".into(),
            message: format!(
                "Cadence does not support runtime provider `{other}`. Allowed providers: {OPENAI_CODEX_PROVIDER_ID}, {OPENROUTER_PROVIDER_ID}."
            ),
            retryable: false,
        }),
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
