use tauri::{AppHandle, Runtime};

use crate::{
    auth::{
        auth_flow_error_from_command_error, bind_anthropic_runtime_session,
        bind_openai_compatible_runtime_session, bind_openrouter_runtime_session,
        load_latest_openai_codex_session, load_latest_xai_session,
        load_openai_codex_session_for_profile_link, load_xai_session_for_profile_link,
        reconcile_anthropic_runtime_session, reconcile_openai_compatible_runtime_session,
        reconcile_openrouter_runtime_session, refresh_provider_auth_session, remove_xai_session,
        sync_openai_profile_link, AnthropicBindOutcome, AnthropicReconcileOutcome,
        AnthropicRuntimeSessionBinding, AuthDiagnostic, AuthFlowError, OpenAiCompatibleBindOutcome,
        OpenAiCompatibleReconcileOutcome, OpenAiCompatibleRuntimeSessionBinding,
        OpenRouterBindOutcome, OpenRouterReconcileOutcome, OpenRouterRuntimeSessionBinding,
        RuntimeAuthSession,
    },
    commands::{get_runtime_settings::RuntimeSettingsSnapshot, RuntimeAuthPhase},
    provider_credentials::{
        ProviderCredentialLink, ProviderCredentialProfile, ProviderCredentialsView,
        CURSOR_DEFAULT_PROFILE_ID, XAI_DEFAULT_PROFILE_ID,
    },
    state::DesktopState,
};

pub const OPENAI_CODEX_PROVIDER_ID: &str = "openai_codex";
pub const OPENROUTER_PROVIDER_ID: &str = "openrouter";
pub const ANTHROPIC_PROVIDER_ID: &str = "anthropic";
pub const GITHUB_MODELS_PROVIDER_ID: &str = "github_models";
pub const OPENAI_API_PROVIDER_ID: &str = "openai_api";
pub const DEEPSEEK_PROVIDER_ID: &str = "deepseek";
pub const XAI_PROVIDER_ID: &str = "xai";
pub const CURSOR_PROVIDER_ID: &str = "external_cursor_sdk";
pub const OLLAMA_PROVIDER_ID: &str = "ollama";
pub const AZURE_OPENAI_PROVIDER_ID: &str = "azure_openai";
pub const GEMINI_AI_STUDIO_PROVIDER_ID: &str = "gemini_ai_studio";
pub const BEDROCK_PROVIDER_ID: &str = "bedrock";
pub const VERTEX_PROVIDER_ID: &str = "vertex";
pub const OPENAI_COMPATIBLE_RUNTIME_KIND: &str = "openai_compatible";
pub const DEEPSEEK_RUNTIME_KIND: &str = DEEPSEEK_PROVIDER_ID;
pub const XAI_RUNTIME_KIND: &str = XAI_PROVIDER_ID;
pub const CURSOR_RUNTIME_KIND: &str = "cursor_sdk";
pub const GEMINI_RUNTIME_KIND: &str = "gemini";
pub const ANTHROPIC_RUNTIME_KIND: &str = ANTHROPIC_PROVIDER_ID;
pub const OPENAI_CODEX_DEFAULT_MODEL_ID: &str = "gpt-5.5";
pub const XAI_DEFAULT_MODEL_ID: &str = "grok-4.3";
pub const CURSOR_DEFAULT_MODEL_ID: &str = "composer-latest";
pub const XAI_SUPPORTED_TEXT_MODEL_IDS: &[&str] = &["grok-4.3", "grok-4.3-latest"];
pub const OPENAI_CODEX_SUPPORTED_MODEL_IDS: &[&str] = &[
    "gpt-5.2",
    "gpt-5.3-codex",
    "gpt-5.3-codex-spark",
    "gpt-5.4",
    "gpt-5.5",
];
const XAI_API_KEY_SESSION_ID: &str = "xai-api-key";
const XAI_API_KEY_ACCOUNT_ID: &str = "xai-api-key";
const CURSOR_API_KEY_SESSION_ID: &str = "cursor-api-key";
const CURSOR_API_KEY_ACCOUNT_ID: &str = "cursor-api-key";

pub fn is_supported_xai_text_model_id(model_id: &str) -> bool {
    let model_id = model_id
        .trim()
        .rsplit('/')
        .next()
        .unwrap_or(model_id)
        .to_ascii_lowercase();
    XAI_SUPPORTED_TEXT_MODEL_IDS
        .iter()
        .any(|supported| model_id == *supported)
}

pub fn normalize_openai_codex_model_id(model_id: &str) -> String {
    let model_id = model_id.trim();
    if model_id.is_empty() || model_id == OPENAI_CODEX_PROVIDER_ID {
        OPENAI_CODEX_DEFAULT_MODEL_ID.into()
    } else {
        model_id.into()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RuntimeProviderFamily {
    OpenAiCodex,
    OpenRouter,
    Anthropic,
    OpenAiCompatible,
    DeepSeek,
    Xai,
    Cursor,
    Gemini,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RuntimeProvider {
    OpenAiCodex,
    OpenRouter,
    Anthropic,
    GitHubModels,
    OpenAiApi,
    DeepSeek,
    Xai,
    Cursor,
    Ollama,
    AzureOpenAi,
    GeminiAiStudio,
    Bedrock,
    Vertex,
}

impl RuntimeProvider {
    pub const fn family(self) -> RuntimeProviderFamily {
        match self {
            Self::OpenAiCodex => RuntimeProviderFamily::OpenAiCodex,
            Self::OpenRouter => RuntimeProviderFamily::OpenRouter,
            Self::Anthropic => RuntimeProviderFamily::Anthropic,
            Self::GitHubModels | Self::OpenAiApi | Self::Ollama | Self::AzureOpenAi => {
                RuntimeProviderFamily::OpenAiCompatible
            }
            Self::DeepSeek => RuntimeProviderFamily::DeepSeek,
            Self::Xai => RuntimeProviderFamily::Xai,
            Self::Cursor => RuntimeProviderFamily::Cursor,
            Self::GeminiAiStudio => RuntimeProviderFamily::Gemini,
            Self::Bedrock | Self::Vertex => RuntimeProviderFamily::Anthropic,
        }
    }

    pub const fn resolve(self) -> ResolvedRuntimeProvider {
        match self {
            Self::OpenAiCodex => ResolvedRuntimeProvider {
                provider: Self::OpenAiCodex,
                family: RuntimeProviderFamily::OpenAiCodex,
                provider_id: OPENAI_CODEX_PROVIDER_ID,
                runtime_kind: OPENAI_CODEX_PROVIDER_ID,
            },
            Self::OpenRouter => ResolvedRuntimeProvider {
                provider: Self::OpenRouter,
                family: RuntimeProviderFamily::OpenRouter,
                provider_id: OPENROUTER_PROVIDER_ID,
                runtime_kind: OPENROUTER_PROVIDER_ID,
            },
            Self::Anthropic => ResolvedRuntimeProvider {
                provider: Self::Anthropic,
                family: RuntimeProviderFamily::Anthropic,
                provider_id: ANTHROPIC_PROVIDER_ID,
                runtime_kind: ANTHROPIC_PROVIDER_ID,
            },
            Self::GitHubModels => ResolvedRuntimeProvider {
                provider: Self::GitHubModels,
                family: RuntimeProviderFamily::OpenAiCompatible,
                provider_id: GITHUB_MODELS_PROVIDER_ID,
                runtime_kind: OPENAI_COMPATIBLE_RUNTIME_KIND,
            },
            Self::OpenAiApi => ResolvedRuntimeProvider {
                provider: Self::OpenAiApi,
                family: RuntimeProviderFamily::OpenAiCompatible,
                provider_id: OPENAI_API_PROVIDER_ID,
                runtime_kind: OPENAI_COMPATIBLE_RUNTIME_KIND,
            },
            Self::DeepSeek => ResolvedRuntimeProvider {
                provider: Self::DeepSeek,
                family: RuntimeProviderFamily::DeepSeek,
                provider_id: DEEPSEEK_PROVIDER_ID,
                runtime_kind: DEEPSEEK_RUNTIME_KIND,
            },
            Self::Xai => ResolvedRuntimeProvider {
                provider: Self::Xai,
                family: RuntimeProviderFamily::Xai,
                provider_id: XAI_PROVIDER_ID,
                runtime_kind: XAI_RUNTIME_KIND,
            },
            Self::Cursor => ResolvedRuntimeProvider {
                provider: Self::Cursor,
                family: RuntimeProviderFamily::Cursor,
                provider_id: CURSOR_PROVIDER_ID,
                runtime_kind: CURSOR_RUNTIME_KIND,
            },
            Self::Ollama => ResolvedRuntimeProvider {
                provider: Self::Ollama,
                family: RuntimeProviderFamily::OpenAiCompatible,
                provider_id: OLLAMA_PROVIDER_ID,
                runtime_kind: OPENAI_COMPATIBLE_RUNTIME_KIND,
            },
            Self::AzureOpenAi => ResolvedRuntimeProvider {
                provider: Self::AzureOpenAi,
                family: RuntimeProviderFamily::OpenAiCompatible,
                provider_id: AZURE_OPENAI_PROVIDER_ID,
                runtime_kind: OPENAI_COMPATIBLE_RUNTIME_KIND,
            },
            Self::GeminiAiStudio => ResolvedRuntimeProvider {
                provider: Self::GeminiAiStudio,
                family: RuntimeProviderFamily::Gemini,
                provider_id: GEMINI_AI_STUDIO_PROVIDER_ID,
                runtime_kind: GEMINI_RUNTIME_KIND,
            },
            Self::Bedrock => ResolvedRuntimeProvider {
                provider: Self::Bedrock,
                family: RuntimeProviderFamily::Anthropic,
                provider_id: BEDROCK_PROVIDER_ID,
                runtime_kind: ANTHROPIC_RUNTIME_KIND,
            },
            Self::Vertex => ResolvedRuntimeProvider {
                provider: Self::Vertex,
                family: RuntimeProviderFamily::Anthropic,
                provider_id: VERTEX_PROVIDER_ID,
                runtime_kind: ANTHROPIC_RUNTIME_KIND,
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

pub const fn github_models_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::GitHubModels.resolve()
}

pub const fn openai_api_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::OpenAiApi.resolve()
}

pub const fn deepseek_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::DeepSeek.resolve()
}

pub const fn xai_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::Xai.resolve()
}

pub const fn cursor_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::Cursor.resolve()
}

pub const fn ollama_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::Ollama.resolve()
}

pub const fn azure_openai_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::AzureOpenAi.resolve()
}

pub const fn gemini_ai_studio_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::GeminiAiStudio.resolve()
}

pub const fn bedrock_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::Bedrock.resolve()
}

pub const fn vertex_provider() -> ResolvedRuntimeProvider {
    RuntimeProvider::Vertex.resolve()
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
                        "Xero rejected the runtime provider identity because providerId `{}` does not match runtimeKind `{}`.",
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
                "Xero could not resolve the runtime provider because runtimeKind `{}` requires a non-blank providerId.",
                runtime_kind.unwrap_or_default()
            ),
            retryable: false,
        }),
        (None, None) => Err(AuthDiagnostic {
            code: "runtime_provider_missing".into(),
            message: "Xero could not resolve the runtime provider because both providerId and runtimeKind were blank.".into(),
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
    provider_profiles: Option<&ProviderCredentialsView>,
) -> Result<RuntimeProviderBindOutcome, AuthFlowError> {
    match provider.provider {
        RuntimeProvider::OpenAiCodex => {
            bind_openai_codex_runtime_session(app, state, provider, account_id, provider_profiles)
        }
        RuntimeProvider::Xai => bind_xai_runtime_session(app, state, provider, provider_profiles),
        RuntimeProvider::Cursor => bind_cursor_runtime_session(provider, provider_profiles),
        RuntimeProvider::OpenRouter => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    "Xero could not bind the selected OpenRouter runtime because the app-global runtime settings snapshot was missing.",
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
        RuntimeProvider::Anthropic | RuntimeProvider::Bedrock | RuntimeProvider::Vertex => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    "Xero could not bind the selected Anthropic runtime because the app-global runtime settings snapshot was missing.",
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
        | RuntimeProvider::DeepSeek
        | RuntimeProvider::Ollama
        | RuntimeProvider::AzureOpenAi
        | RuntimeProvider::GitHubModels
        | RuntimeProvider::GeminiAiStudio => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    format!(
                        "Xero could not bind the selected {} runtime because the app-global runtime settings snapshot was missing.",
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
    provider_profiles: Option<&ProviderCredentialsView>,
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
        RuntimeProvider::Xai => {
            reconcile_xai_runtime_session(app, state, provider, provider_profiles)
        }
        RuntimeProvider::Cursor => reconcile_cursor_runtime_session(provider, provider_profiles),
        RuntimeProvider::OpenRouter => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    "Xero could not reconcile the selected OpenRouter runtime because the app-global runtime settings snapshot was missing.",
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
        RuntimeProvider::Anthropic | RuntimeProvider::Bedrock | RuntimeProvider::Vertex => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    "Xero could not reconcile the selected Anthropic runtime because the app-global runtime settings snapshot was missing.",
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
        | RuntimeProvider::DeepSeek
        | RuntimeProvider::Ollama
        | RuntimeProvider::AzureOpenAi
        | RuntimeProvider::GitHubModels
        | RuntimeProvider::GeminiAiStudio => {
            let settings = settings.ok_or_else(|| {
                AuthFlowError::terminal(
                    "runtime_settings_missing",
                    RuntimeAuthPhase::Failed,
                    format!(
                        "Xero could not reconcile the selected {} runtime because the app-global runtime settings snapshot was missing.",
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
    if account_id.is_empty() && provider.provider != RuntimeProvider::OpenAiCodex {
        return Ok(());
    }

    match provider.provider {
        RuntimeProvider::OpenAiCodex => {
            let auth_store_path = state
                .global_db_path(app)
                .map_err(auth_flow_error_from_command_error)?;
            crate::auth::clear_openai_codex_sessions(&auth_store_path)?;
            sync_openai_profile_link(app, state, None, None)
        }
        RuntimeProvider::Xai => {
            if account_id == XAI_API_KEY_ACCOUNT_ID {
                return Ok(());
            }
            let auth_store_path = state
                .global_db_path(app)
                .map_err(auth_flow_error_from_command_error)?;
            remove_xai_session(&auth_store_path)
        }
        RuntimeProvider::Cursor => Ok(()),
        RuntimeProvider::OpenRouter
        | RuntimeProvider::Anthropic
        | RuntimeProvider::GitHubModels
        | RuntimeProvider::OpenAiApi
        | RuntimeProvider::DeepSeek
        | RuntimeProvider::Ollama
        | RuntimeProvider::AzureOpenAi
        | RuntimeProvider::GeminiAiStudio
        | RuntimeProvider::Bedrock
        | RuntimeProvider::Vertex => Ok(()),
    }
}

fn bind_openai_codex_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    _account_id: Option<&str>,
    provider_profiles: Option<&ProviderCredentialsView>,
) -> Result<RuntimeProviderBindOutcome, AuthFlowError> {
    let profile = active_openai_profile(provider_profiles)?;
    let auth_store_path = state
        .global_db_path(app)
        .map_err(auth_flow_error_from_command_error)?;
    let Some(stored) = load_global_openai_codex_session_for_profile(
        &auth_store_path,
        profile.credential_link.as_ref(),
    )?
    else {
        return Ok(RuntimeProviderBindOutcome::SignedOut(AuthDiagnostic {
            code: "auth_session_not_found".into(),
            message: "Xero does not have a global app-local OpenAI auth session available.".into(),
            retryable: false,
        }));
    };
    sync_openai_profile_link(app, state, Some(&profile.profile_id), Some(&stored))?;

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
    _account_id: Option<&str>,
    _session_id: Option<&str>,
    provider_profiles: Option<&ProviderCredentialsView>,
) -> Result<RuntimeProviderReconcileOutcome, AuthFlowError> {
    let profile = active_openai_profile(provider_profiles)?;
    let auth_store_path = state
        .global_db_path(app)
        .map_err(auth_flow_error_from_command_error)?;
    let stored = match load_global_openai_codex_session_for_profile(
        &auth_store_path,
        profile.credential_link.as_ref(),
    )? {
        Some(stored) => stored,
        None => {
            return Ok(RuntimeProviderReconcileOutcome::SignedOut(AuthDiagnostic {
                code: "auth_session_not_found".into(),
                message: "Xero no longer has a global app-local OpenAI auth session available."
                    .into(),
                retryable: false,
            }));
        }
    };
    sync_openai_profile_link(app, state, Some(&profile.profile_id), Some(&stored))?;

    Ok(RuntimeProviderReconcileOutcome::Authenticated(
        binding_from_stored_openai_session(
            provider,
            &stored.session_id,
            &stored.account_id,
            &stored.updated_at,
        ),
    ))
}

fn bind_xai_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    provider_profiles: Option<&ProviderCredentialsView>,
) -> Result<RuntimeProviderBindOutcome, AuthFlowError> {
    let Some(profile) = active_xai_profile(provider_profiles)? else {
        return Ok(RuntimeProviderBindOutcome::SignedOut(
            missing_xai_session_diagnostic(),
        ));
    };

    match profile.credential_link.as_ref() {
        Some(ProviderCredentialLink::ApiKey { updated_at }) => Ok(
            RuntimeProviderBindOutcome::Ready(binding_from_stored_xai_session(
                provider,
                XAI_API_KEY_SESSION_ID,
                XAI_API_KEY_ACCOUNT_ID,
                updated_at,
            )),
        ),
        Some(ProviderCredentialLink::Xai { .. }) => {
            let auth_store_path = state
                .global_db_path(app)
                .map_err(auth_flow_error_from_command_error)?;
            let Some(stored) = load_global_xai_session_for_profile(
                &auth_store_path,
                profile.credential_link.as_ref(),
            )?
            else {
                return Ok(RuntimeProviderBindOutcome::SignedOut(
                    missing_xai_session_diagnostic(),
                ));
            };
            let binding = binding_from_stored_xai_session(
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
        _ => Ok(RuntimeProviderBindOutcome::SignedOut(
            invalid_xai_profile_diagnostic(),
        )),
    }
}

fn reconcile_xai_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: ResolvedRuntimeProvider,
    provider_profiles: Option<&ProviderCredentialsView>,
) -> Result<RuntimeProviderReconcileOutcome, AuthFlowError> {
    let Some(profile) = active_xai_profile(provider_profiles)? else {
        return Ok(RuntimeProviderReconcileOutcome::SignedOut(
            missing_xai_session_diagnostic(),
        ));
    };

    match profile.credential_link.as_ref() {
        Some(ProviderCredentialLink::ApiKey { updated_at }) => Ok(
            RuntimeProviderReconcileOutcome::Authenticated(binding_from_stored_xai_session(
                provider,
                XAI_API_KEY_SESSION_ID,
                XAI_API_KEY_ACCOUNT_ID,
                updated_at,
            )),
        ),
        Some(ProviderCredentialLink::Xai { .. }) => {
            let auth_store_path = state
                .global_db_path(app)
                .map_err(auth_flow_error_from_command_error)?;
            let Some(stored) = load_global_xai_session_for_profile(
                &auth_store_path,
                profile.credential_link.as_ref(),
            )?
            else {
                return Ok(RuntimeProviderReconcileOutcome::SignedOut(
                    missing_xai_session_diagnostic(),
                ));
            };
            Ok(RuntimeProviderReconcileOutcome::Authenticated(
                binding_from_stored_xai_session(
                    provider,
                    &stored.session_id,
                    &stored.account_id,
                    &stored.updated_at,
                ),
            ))
        }
        _ => Ok(RuntimeProviderReconcileOutcome::SignedOut(
            invalid_xai_profile_diagnostic(),
        )),
    }
}

fn bind_cursor_runtime_session(
    provider: ResolvedRuntimeProvider,
    provider_profiles: Option<&ProviderCredentialsView>,
) -> Result<RuntimeProviderBindOutcome, AuthFlowError> {
    let Some(profile) = active_cursor_profile(provider_profiles)? else {
        return Ok(RuntimeProviderBindOutcome::SignedOut(
            missing_cursor_session_diagnostic(),
        ));
    };

    match profile.credential_link.as_ref() {
        Some(ProviderCredentialLink::ApiKey { updated_at }) => Ok(
            RuntimeProviderBindOutcome::Ready(binding_from_stored_cursor_session(
                provider,
                CURSOR_API_KEY_SESSION_ID,
                CURSOR_API_KEY_ACCOUNT_ID,
                updated_at,
            )),
        ),
        _ => Ok(RuntimeProviderBindOutcome::SignedOut(
            invalid_cursor_profile_diagnostic(),
        )),
    }
}

fn reconcile_cursor_runtime_session(
    provider: ResolvedRuntimeProvider,
    provider_profiles: Option<&ProviderCredentialsView>,
) -> Result<RuntimeProviderReconcileOutcome, AuthFlowError> {
    let Some(profile) = active_cursor_profile(provider_profiles)? else {
        return Ok(RuntimeProviderReconcileOutcome::SignedOut(
            missing_cursor_session_diagnostic(),
        ));
    };

    match profile.credential_link.as_ref() {
        Some(ProviderCredentialLink::ApiKey { updated_at }) => Ok(
            RuntimeProviderReconcileOutcome::Authenticated(binding_from_stored_cursor_session(
                provider,
                CURSOR_API_KEY_SESSION_ID,
                CURSOR_API_KEY_ACCOUNT_ID,
                updated_at,
            )),
        ),
        _ => Ok(RuntimeProviderReconcileOutcome::SignedOut(
            invalid_cursor_profile_diagnostic(),
        )),
    }
}

fn active_openai_profile(
    provider_profiles: Option<&ProviderCredentialsView>,
) -> Result<&ProviderCredentialProfile, AuthFlowError> {
    let provider_profiles = provider_profiles.ok_or_else(|| {
        AuthFlowError::terminal(
            "provider_credentials_missing",
            RuntimeAuthPhase::Failed,
            "Xero could not resolve the active OpenAI credential because the provider credential snapshot was missing.",
        )
    })?;

    let profile = provider_profiles.active_profile().ok_or_else(|| {
        AuthFlowError::terminal(
            "provider_credentials_invalid",
            RuntimeAuthPhase::Failed,
            "Xero could not resolve an OpenAI credential. Sign in to OpenAI Codex from Providers settings.",
        )
    })?;

    if profile.provider_id != OPENAI_CODEX_PROVIDER_ID {
        return Err(AuthFlowError::terminal(
            "runtime_provider_mismatch",
            RuntimeAuthPhase::Failed,
            format!(
                "Xero rejected OpenAI runtime reconciliation because provider `{}` is not `{}`.",
                profile.provider_id, OPENAI_CODEX_PROVIDER_ID
            ),
        ));
    }

    Ok(profile)
}

fn active_xai_profile(
    provider_profiles: Option<&ProviderCredentialsView>,
) -> Result<Option<&ProviderCredentialProfile>, AuthFlowError> {
    let provider_profiles = provider_profiles.ok_or_else(|| {
        AuthFlowError::terminal(
            "provider_credentials_missing",
            RuntimeAuthPhase::Failed,
            "Xero could not resolve the active xAI credential because the provider credential snapshot was missing.",
        )
    })?;

    Ok(provider_profiles
        .profile(XAI_DEFAULT_PROFILE_ID)
        .or_else(|| {
            provider_profiles
                .profiles()
                .iter()
                .find(|profile| profile.provider_id == XAI_PROVIDER_ID)
        }))
}

fn active_cursor_profile(
    provider_profiles: Option<&ProviderCredentialsView>,
) -> Result<Option<&ProviderCredentialProfile>, AuthFlowError> {
    let provider_profiles = provider_profiles.ok_or_else(|| {
        AuthFlowError::terminal(
            "provider_credentials_missing",
            RuntimeAuthPhase::Failed,
            "Xero could not resolve the active Cursor credential because the provider credential snapshot was missing.",
        )
    })?;

    Ok(provider_profiles
        .profile(CURSOR_DEFAULT_PROFILE_ID)
        .or_else(|| {
            provider_profiles
                .profiles()
                .iter()
                .find(|profile| profile.provider_id == CURSOR_PROVIDER_ID)
        }))
}

fn load_global_openai_codex_session_for_profile(
    auth_store_path: &std::path::Path,
    link: Option<&ProviderCredentialLink>,
) -> Result<Option<crate::auth::StoredOpenAiCodexSession>, AuthFlowError> {
    match link {
        Some(link) => load_openai_codex_session_for_profile_link(auth_store_path, link),
        None => load_latest_openai_codex_session(auth_store_path),
    }
}

fn load_global_xai_session_for_profile(
    auth_store_path: &std::path::Path,
    link: Option<&ProviderCredentialLink>,
) -> Result<Option<crate::auth::StoredXaiSession>, AuthFlowError> {
    match link {
        Some(link @ ProviderCredentialLink::Xai { .. }) => {
            load_xai_session_for_profile_link(auth_store_path, link)
        }
        Some(ProviderCredentialLink::ApiKey { .. }) => Ok(None),
        _ => load_latest_xai_session(auth_store_path),
    }
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

fn binding_from_stored_xai_session(
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

fn binding_from_stored_cursor_session(
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

fn missing_xai_session_diagnostic() -> AuthDiagnostic {
    AuthDiagnostic {
        code: "auth_session_not_found".into(),
        message:
            "Xero does not have an app-local xAI credential. Sign in to xAI or save an xAI API key from Providers settings."
                .into(),
        retryable: false,
    }
}

fn invalid_xai_profile_diagnostic() -> AuthDiagnostic {
    AuthDiagnostic {
        code: "provider_credentials_invalid".into(),
        message:
            "Xero rejected the active xAI provider profile because it does not contain an xAI OAuth session or API key."
                .into(),
        retryable: false,
    }
}

fn missing_cursor_session_diagnostic() -> AuthDiagnostic {
    AuthDiagnostic {
        code: "auth_session_not_found".into(),
        message:
            "Xero does not have an app-local Cursor credential. Save a Cursor API key from Providers settings."
                .into(),
        retryable: false,
    }
}

fn invalid_cursor_profile_diagnostic() -> AuthDiagnostic {
    AuthDiagnostic {
        code: "provider_credentials_invalid".into(),
        message:
            "Xero rejected the active Cursor provider profile because it does not contain a Cursor API key."
                .into(),
        retryable: false,
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
        GITHUB_MODELS_PROVIDER_ID => Ok(RuntimeProvider::GitHubModels),
        OPENAI_API_PROVIDER_ID => Ok(RuntimeProvider::OpenAiApi),
        DEEPSEEK_PROVIDER_ID => Ok(RuntimeProvider::DeepSeek),
        XAI_PROVIDER_ID => Ok(RuntimeProvider::Xai),
        CURSOR_PROVIDER_ID => Ok(RuntimeProvider::Cursor),
        OLLAMA_PROVIDER_ID => Ok(RuntimeProvider::Ollama),
        AZURE_OPENAI_PROVIDER_ID => Ok(RuntimeProvider::AzureOpenAi),
        GEMINI_AI_STUDIO_PROVIDER_ID => Ok(RuntimeProvider::GeminiAiStudio),
        BEDROCK_PROVIDER_ID => Ok(RuntimeProvider::Bedrock),
        VERTEX_PROVIDER_ID => Ok(RuntimeProvider::Vertex),
        other => Err(unknown_runtime_provider_diagnostic(other)),
    }
}

fn parse_runtime_reference(value: &str) -> Result<RuntimeReference, AuthDiagnostic> {
    match value {
        OPENAI_CODEX_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::OpenAiCodex)),
        OPENROUTER_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::OpenRouter)),
        ANTHROPIC_PROVIDER_ID => Ok(RuntimeReference::Family(RuntimeProviderFamily::Anthropic)),
        OPENAI_API_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::OpenAiApi)),
        DEEPSEEK_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::DeepSeek)),
        XAI_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::Xai)),
        CURSOR_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::Cursor)),
        OLLAMA_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::Ollama)),
        AZURE_OPENAI_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::AzureOpenAi)),
        GEMINI_AI_STUDIO_PROVIDER_ID => {
            Ok(RuntimeReference::Actual(RuntimeProvider::GeminiAiStudio))
        }
        BEDROCK_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::Bedrock)),
        VERTEX_PROVIDER_ID => Ok(RuntimeReference::Actual(RuntimeProvider::Vertex)),
        OPENAI_COMPATIBLE_RUNTIME_KIND => Ok(RuntimeReference::Family(
            RuntimeProviderFamily::OpenAiCompatible,
        )),
        GEMINI_RUNTIME_KIND => Ok(RuntimeReference::Family(RuntimeProviderFamily::Gemini)),
        CURSOR_RUNTIME_KIND => Ok(RuntimeReference::Family(RuntimeProviderFamily::Cursor)),
        other => Err(unknown_runtime_provider_diagnostic(other)),
    }
}

fn unknown_runtime_provider_diagnostic(value: &str) -> AuthDiagnostic {
    AuthDiagnostic {
        code: "runtime_provider_unknown".into(),
        message: format!(
            "Xero does not support runtime provider `{value}`. Allowed providers: {OPENAI_CODEX_PROVIDER_ID}, {OPENROUTER_PROVIDER_ID}, {ANTHROPIC_PROVIDER_ID}, {GITHUB_MODELS_PROVIDER_ID}, {OPENAI_API_PROVIDER_ID}, {DEEPSEEK_PROVIDER_ID}, {XAI_PROVIDER_ID}, {CURSOR_PROVIDER_ID}, {OLLAMA_PROVIDER_ID}, {AZURE_OPENAI_PROVIDER_ID}, {GEMINI_AI_STUDIO_PROVIDER_ID}, {BEDROCK_PROVIDER_ID}, {VERTEX_PROVIDER_ID}. Allowed runtime kinds: {OPENAI_CODEX_PROVIDER_ID}, {OPENROUTER_PROVIDER_ID}, {ANTHROPIC_RUNTIME_KIND}, {OPENAI_COMPATIBLE_RUNTIME_KIND}, {DEEPSEEK_RUNTIME_KIND}, {XAI_RUNTIME_KIND}, {CURSOR_RUNTIME_KIND}, {GEMINI_RUNTIME_KIND}."
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
        let provider = resolve_runtime_provider_identity(Some(binding.provider_id.as_str()), None)
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
        let provider = resolve_runtime_provider_identity(
            Some(binding.provider_id.as_str()),
            Some(ANTHROPIC_RUNTIME_KIND),
        )
        .expect("anthropic-family binding provider id should resolve");

        Self {
            provider,
            session_id: binding.session_id,
            account_id: binding.account_id,
            updated_at: binding.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_provider_identity_resolves_as_first_party_runtime() {
        let provider = resolve_runtime_provider_identity(
            Some(DEEPSEEK_PROVIDER_ID),
            Some(DEEPSEEK_RUNTIME_KIND),
        )
        .expect("DeepSeek provider should resolve");

        assert_eq!(provider.provider_id, DEEPSEEK_PROVIDER_ID);
        assert_eq!(provider.runtime_kind, DEEPSEEK_RUNTIME_KIND);
        assert_eq!(provider.family, RuntimeProviderFamily::DeepSeek);
    }

    #[test]
    fn deepseek_provider_identity_rejects_mismatched_runtime_kind() {
        let diagnostic = resolve_runtime_provider_identity(
            Some(DEEPSEEK_PROVIDER_ID),
            Some(OPENAI_COMPATIBLE_RUNTIME_KIND),
        )
        .expect_err("DeepSeek should not resolve through the generic runtime kind");

        assert_eq!(diagnostic.code, "runtime_provider_mismatch");
    }

    #[test]
    fn xai_provider_identity_resolves_as_native_runtime() {
        let provider =
            resolve_runtime_provider_identity(Some(XAI_PROVIDER_ID), Some(XAI_RUNTIME_KIND))
                .expect("xAI provider should resolve");

        assert_eq!(provider.provider_id, XAI_PROVIDER_ID);
        assert_eq!(provider.runtime_kind, XAI_RUNTIME_KIND);
        assert_eq!(provider.family, RuntimeProviderFamily::Xai);
    }

    #[test]
    fn xai_provider_identity_rejects_openai_compatible_runtime_kind() {
        let diagnostic = resolve_runtime_provider_identity(
            Some(XAI_PROVIDER_ID),
            Some(OPENAI_COMPATIBLE_RUNTIME_KIND),
        )
        .expect_err("xAI should not resolve through the generic runtime kind");

        assert_eq!(diagnostic.code, "runtime_provider_mismatch");
    }

    #[test]
    fn cursor_provider_identity_resolves_as_external_runtime() {
        let provider =
            resolve_runtime_provider_identity(Some(CURSOR_PROVIDER_ID), Some(CURSOR_RUNTIME_KIND))
                .expect("Cursor provider should resolve");

        assert_eq!(provider.provider_id, CURSOR_PROVIDER_ID);
        assert_eq!(provider.runtime_kind, CURSOR_RUNTIME_KIND);
        assert_eq!(provider.family, RuntimeProviderFamily::Cursor);
    }
}
