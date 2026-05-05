use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, GetProviderModelCatalogRequestDto,
        ProviderModelCatalogDiagnosticDto, ProviderModelCatalogDto, ProviderModelCatalogSourceDto,
        ProviderModelDto, ProviderModelThinkingCapabilityDto, ProviderModelThinkingEffortDto,
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

    ProviderModelCatalogDto {
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
    }
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
