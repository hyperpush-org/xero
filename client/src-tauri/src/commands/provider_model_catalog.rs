use std::collections::BTreeSet;

use tauri::{AppHandle, Runtime, State};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    commands::{
        validate_non_empty, CommandResult, GetProviderModelCatalogRequestDto,
        ProviderModelCatalogContractDiagnosticDto, ProviderModelCatalogDiagnosticDto,
        ProviderModelCatalogDto, ProviderModelCatalogSourceDto, ProviderModelDto,
        ProviderModelThinkingCapabilityDto, ProviderModelThinkingEffortDto,
    },
    provider_models::{
        catalog_age_seconds, load_provider_model_catalog, provider_capability_catalog_for_catalog,
        provider_capability_catalog_for_model, ProviderModelCatalog,
        ProviderModelCatalogDiagnostic, ProviderModelCatalogSource, ProviderModelRecord,
        ProviderModelThinkingCapability, ProviderModelThinkingEffort,
    },
    state::DesktopState,
};

#[tauri::command]
pub async fn get_provider_model_catalog<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetProviderModelCatalogRequestDto,
) -> CommandResult<ProviderModelCatalogDto> {
    validate_non_empty(&request.profile_id, "profileId")?;
    let jobs = state.backend_jobs().clone();
    let desktop_state = state.inner().clone();
    let profile_id = request.profile_id;
    let force_refresh = request.force_refresh;

    jobs.run_blocking_latest(
        format!("provider-model-catalog:{profile_id}"),
        "provider model catalog",
        move |cancellation| {
            cancellation.check_cancelled("provider model catalog")?;
            let catalog =
                load_provider_model_catalog(&app, &desktop_state, &profile_id, force_refresh)?;
            Ok(map_provider_model_catalog(catalog))
        },
    )
    .await
}

pub(crate) fn map_provider_model_catalog(catalog: ProviderModelCatalog) -> ProviderModelCatalogDto {
    let capabilities = provider_capability_catalog_for_catalog(&catalog, None);
    let cache_age_seconds = catalog.fetched_at.as_deref().and_then(catalog_age_seconds);
    let models = catalog
        .models
        .iter()
        .map(|model| map_provider_model(&catalog, model))
        .collect();
    let contract_diagnostics = validate_provider_model_catalog_contract(&catalog);

    ProviderModelCatalogDto {
        contract_version: 1,
        profile_id: catalog.profile_id,
        provider_id: catalog.provider_id,
        configured_model_id: catalog.configured_model_id,
        source: map_catalog_source(catalog.source),
        capabilities,
        fetched_at: catalog.fetched_at,
        last_success_at: catalog.last_success_at,
        cache_age_seconds,
        cache_ttl_seconds: xero_agent_core::DEFAULT_PROVIDER_CATALOG_TTL_SECONDS,
        last_refresh_error: catalog.last_refresh_error.map(map_catalog_diagnostic),
        models,
        contract_diagnostics,
    }
}

fn validate_provider_model_catalog_contract(
    catalog: &ProviderModelCatalog,
) -> Vec<ProviderModelCatalogContractDiagnosticDto> {
    let mut diagnostics = Vec::new();
    if catalog.provider_id == "openai_codex" && catalog.configured_model_id.trim().is_empty() {
        diagnostics.push(provider_catalog_contract_diagnostic(
            "provider_model_catalog_configured_model_required",
            "Xero requires a configured model id for provider `openai_codex`.",
            &["configuredModelId"],
        ));
    }

    let mut model_ids = BTreeSet::new();
    for (index, model) in catalog.models.iter().enumerate() {
        if !model_ids.insert(model.model_id.as_str()) {
            diagnostics.push(provider_catalog_contract_diagnostic(
                "provider_model_catalog_duplicate_model_id",
                &format!(
                    "Provider-model catalog rows must not duplicate model id `{}`.",
                    model.model_id
                ),
                &["models", &index.to_string(), "modelId"],
            ));
        }

        if catalog.provider_id == "openai_codex" && model.model_id.trim().is_empty() {
            diagnostics.push(provider_catalog_contract_diagnostic(
                "provider_model_catalog_model_id_required",
                "Xero requires model ids for provider `openai_codex`.",
                &["models", &index.to_string(), "modelId"],
            ));
        }

        diagnostics.extend(validate_thinking_capability_contract(
            index,
            &model.thinking,
        ));
    }

    if catalog.source == ProviderModelCatalogSource::Unavailable {
        if catalog.fetched_at.is_some() {
            diagnostics.push(provider_catalog_contract_diagnostic(
                "provider_model_catalog_unavailable_fetched_at",
                "Unavailable provider-model catalogs must not expose `fetchedAt`.",
                &["fetchedAt"],
            ));
        }
        if catalog.last_success_at.is_some() {
            diagnostics.push(provider_catalog_contract_diagnostic(
                "provider_model_catalog_unavailable_last_success_at",
                "Unavailable provider-model catalogs must not expose `lastSuccessAt`.",
                &["lastSuccessAt"],
            ));
        }
        if !catalog.models.is_empty() {
            diagnostics.push(provider_catalog_contract_diagnostic(
                "provider_model_catalog_unavailable_models",
                "Unavailable provider-model catalogs must not expose discovered models.",
                &["models"],
            ));
        }
        return diagnostics;
    }

    if catalog.fetched_at.is_none() {
        diagnostics.push(provider_catalog_contract_diagnostic(
            "provider_model_catalog_missing_fetched_at",
            "Provider-model catalogs must expose `fetchedAt` unless unavailable.",
            &["fetchedAt"],
        ));
    }
    if catalog.last_success_at.is_none() {
        diagnostics.push(provider_catalog_contract_diagnostic(
            "provider_model_catalog_missing_last_success_at",
            "Provider-model catalogs must expose `lastSuccessAt` unless unavailable.",
            &["lastSuccessAt"],
        ));
    }
    if last_success_after_fetch(
        catalog.last_success_at.as_deref(),
        catalog.fetched_at.as_deref(),
    ) == Some(true)
    {
        diagnostics.push(provider_catalog_contract_diagnostic(
            "provider_model_catalog_last_success_after_fetch",
            "Provider-model `lastSuccessAt` must not be newer than `fetchedAt`.",
            &["lastSuccessAt"],
        ));
    }

    diagnostics
}

fn validate_thinking_capability_contract(
    model_index: usize,
    capability: &ProviderModelThinkingCapability,
) -> Vec<ProviderModelCatalogContractDiagnosticDto> {
    let mut diagnostics = Vec::new();
    let mut efforts = BTreeSet::new();
    for (effort_index, effort) in capability.effort_options.iter().enumerate() {
        if !efforts.insert(effort) {
            diagnostics.push(provider_catalog_contract_diagnostic(
                "provider_model_catalog_duplicate_thinking_effort",
                "Provider-model thinking effort options must be unique.",
                &[
                    "models",
                    &model_index.to_string(),
                    "thinking",
                    "effortOptions",
                    &effort_index.to_string(),
                ],
            ));
        }
    }

    if !capability.supported {
        if !capability.effort_options.is_empty() {
            diagnostics.push(provider_catalog_contract_diagnostic(
                "provider_model_catalog_unsupported_thinking_efforts",
                "Unsupported provider-model thinking capability must not expose effort options.",
                &[
                    "models",
                    &model_index.to_string(),
                    "thinking",
                    "effortOptions",
                ],
            ));
        }
        if capability.default_effort.is_some() {
            diagnostics.push(provider_catalog_contract_diagnostic(
                "provider_model_catalog_unsupported_default_effort",
                "Unsupported provider-model thinking capability must not expose a default effort.",
                &[
                    "models",
                    &model_index.to_string(),
                    "thinking",
                    "defaultEffort",
                ],
            ));
        }
        return diagnostics;
    }

    if capability.effort_options.is_empty() {
        diagnostics.push(provider_catalog_contract_diagnostic(
            "provider_model_catalog_missing_thinking_efforts",
            "Supported provider-model thinking capability must expose at least one effort option.",
            &[
                "models",
                &model_index.to_string(),
                "thinking",
                "effortOptions",
            ],
        ));
    }
    if let Some(default_effort) = capability.default_effort.as_ref() {
        if !efforts.contains(default_effort) {
            diagnostics.push(provider_catalog_contract_diagnostic(
                "provider_model_catalog_default_effort_not_allowed",
                "Provider-model thinking default effort must be included in `effortOptions`.",
                &[
                    "models",
                    &model_index.to_string(),
                    "thinking",
                    "defaultEffort",
                ],
            ));
        }
    }

    diagnostics
}

fn provider_catalog_contract_diagnostic(
    code: &str,
    message: &str,
    path: &[&str],
) -> ProviderModelCatalogContractDiagnosticDto {
    ProviderModelCatalogContractDiagnosticDto {
        code: code.into(),
        message: message.into(),
        severity: "error".into(),
        path: path.iter().map(|segment| (*segment).into()).collect(),
    }
}

fn last_success_after_fetch(
    last_success_at: Option<&str>,
    fetched_at: Option<&str>,
) -> Option<bool> {
    let last_success_at = OffsetDateTime::parse(last_success_at?, &Rfc3339).ok()?;
    let fetched_at = OffsetDateTime::parse(fetched_at?, &Rfc3339).ok()?;
    Some(last_success_at > fetched_at)
}

fn map_catalog_source(source: ProviderModelCatalogSource) -> ProviderModelCatalogSourceDto {
    match source {
        ProviderModelCatalogSource::Live => ProviderModelCatalogSourceDto::Live,
        ProviderModelCatalogSource::Cache => ProviderModelCatalogSourceDto::Cache,
        ProviderModelCatalogSource::Manual => ProviderModelCatalogSourceDto::Manual,
        ProviderModelCatalogSource::Unavailable => ProviderModelCatalogSourceDto::Unavailable,
    }
}

fn map_catalog_diagnostic(
    diagnostic: ProviderModelCatalogDiagnostic,
) -> ProviderModelCatalogDiagnosticDto {
    ProviderModelCatalogDiagnosticDto {
        code: diagnostic.code,
        message: diagnostic.message,
        retryable: diagnostic.retryable,
    }
}

fn map_provider_model(
    catalog: &ProviderModelCatalog,
    model: &ProviderModelRecord,
) -> ProviderModelDto {
    ProviderModelDto {
        model_id: model.model_id.clone(),
        display_name: model.display_name.clone(),
        thinking: map_thinking_capability(model.thinking.clone()),
        context_window_tokens: model.context_window_tokens,
        max_output_tokens: model.max_output_tokens,
        context_limit_source: model.context_limit_source.clone(),
        context_limit_confidence: model.context_limit_confidence.clone(),
        context_limit_fetched_at: model.context_limit_fetched_at.clone(),
        capabilities: provider_capability_catalog_for_model(catalog, model),
    }
}

fn map_thinking_capability(
    capability: ProviderModelThinkingCapability,
) -> ProviderModelThinkingCapabilityDto {
    ProviderModelThinkingCapabilityDto {
        supported: capability.supported,
        effort_options: capability
            .effort_options
            .into_iter()
            .map(map_thinking_effort)
            .collect(),
        default_effort: capability.default_effort.map(map_thinking_effort),
    }
}

fn map_thinking_effort(effort: ProviderModelThinkingEffort) -> ProviderModelThinkingEffortDto {
    match effort {
        ProviderModelThinkingEffort::Minimal => ProviderModelThinkingEffortDto::Minimal,
        ProviderModelThinkingEffort::Low => ProviderModelThinkingEffortDto::Low,
        ProviderModelThinkingEffort::Medium => ProviderModelThinkingEffortDto::Medium,
        ProviderModelThinkingEffort::High => ProviderModelThinkingEffortDto::High,
        ProviderModelThinkingEffort::XHigh => ProviderModelThinkingEffortDto::XHigh,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(model_id: &str, thinking: ProviderModelThinkingCapability) -> ProviderModelRecord {
        ProviderModelRecord {
            model_id: model_id.into(),
            display_name: model_id.into(),
            thinking,
            context_window_tokens: None,
            max_output_tokens: None,
            context_limit_source: None,
            context_limit_confidence: None,
            context_limit_fetched_at: None,
        }
    }

    #[test]
    fn provider_model_catalog_emits_contract_metadata_and_diagnostics() {
        let catalog = ProviderModelCatalog {
            profile_id: "openrouter-default".into(),
            provider_id: "openrouter".into(),
            configured_model_id: "openai/gpt-5.4".into(),
            source: ProviderModelCatalogSource::Live,
            fetched_at: Some("2026-04-21T12:00:00Z".into()),
            last_success_at: Some("2026-04-21T12:01:00Z".into()),
            last_refresh_error: None,
            models: vec![
                model(
                    "openai/gpt-5.4",
                    ProviderModelThinkingCapability {
                        supported: true,
                        effort_options: vec![
                            ProviderModelThinkingEffort::High,
                            ProviderModelThinkingEffort::High,
                        ],
                        default_effort: Some(ProviderModelThinkingEffort::Low),
                    },
                ),
                model(
                    "openai/gpt-5.4",
                    ProviderModelThinkingCapability {
                        supported: false,
                        effort_options: vec![ProviderModelThinkingEffort::Low],
                        default_effort: Some(ProviderModelThinkingEffort::Low),
                    },
                ),
            ],
        };

        let dto = map_provider_model_catalog(catalog);
        let codes = dto
            .contract_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(dto.contract_version, 1);
        assert!(codes.contains("provider_model_catalog_duplicate_model_id"));
        assert!(codes.contains("provider_model_catalog_duplicate_thinking_effort"));
        assert!(codes.contains("provider_model_catalog_default_effort_not_allowed"));
        assert!(codes.contains("provider_model_catalog_unsupported_thinking_efforts"));
        assert!(codes.contains("provider_model_catalog_unsupported_default_effort"));
        assert!(codes.contains("provider_model_catalog_last_success_after_fetch"));
    }
}
