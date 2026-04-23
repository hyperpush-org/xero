use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    thread,
    time::Duration,
};

use cadence_desktop_lib::{
    auth::{AnthropicAuthConfig, OpenRouterAuthConfig},
    commands::{
        provider_model_catalog::get_provider_model_catalog,
        provider_profiles::upsert_provider_profile, GetProviderModelCatalogRequestDto,
        ProviderModelCatalogSourceDto, ProviderModelThinkingEffortDto,
        UpsertProviderProfileRequestDto,
    },
    configure_builder_with_state,
    state::DesktopState,
};
use tauri::Manager;
use tempfile::TempDir;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> DesktopState {
    let app_data = root.path().join("app-data");
    DesktopState::default()
        .with_registry_file_override(app_data.join("project-registry.json"))
        .with_auth_store_file_override(app_data.join("openai-auth.json"))
        .with_provider_profiles_file_override(app_data.join("provider-profiles.json"))
        .with_provider_profile_credential_store_file_override(
            app_data.join("provider-profile-credentials.json"),
        )
        .with_provider_model_catalog_cache_file_override(
            app_data.join("provider-model-catalogs.json"),
        )
        .with_runtime_settings_file_override(app_data.join("runtime-settings.json"))
        .with_openrouter_credential_file_override(app_data.join("openrouter-credentials.json"))
}

fn catalog_cache_path(root: &TempDir) -> PathBuf {
    root.path()
        .join("app-data")
        .join("provider-model-catalogs.json")
}

fn openrouter_auth_config(models_url: String) -> OpenRouterAuthConfig {
    OpenRouterAuthConfig {
        models_url,
        timeout: Duration::from_secs(5),
    }
}

fn anthropic_auth_config(models_url: String) -> AnthropicAuthConfig {
    AnthropicAuthConfig {
        models_url,
        timeout: Duration::from_secs(5),
        ..AnthropicAuthConfig::default()
    }
}

fn spawn_static_http_server(status: u16, body: &str) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test http server");
    let address = listener.local_addr().expect("test http server addr");
    let body = body.to_owned();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept test http request");
        let mut reader = BufReader::new(stream.try_clone().expect("clone tcp stream"));
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line).expect("read request line");
            if bytes == 0 || line == "\r\n" {
                break;
            }
        }

        write!(
            stream,
            "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        )
        .expect("write test http response");
    });

    format!("http://{address}")
}

fn seed_openrouter_profile(
    app: &tauri::App<tauri::test::MockRuntime>,
    profile_id: &str,
    model_id: &str,
    api_key: &str,
) {
    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: profile_id.into(),
            provider_id: "openrouter".into(),
            label: "OpenRouter Work".into(),
            model_id: model_id.into(),
            openrouter_api_key: Some(api_key.into()),
            anthropic_api_key: None,
            activate: false,
        },
    )
    .expect("seed openrouter profile");
}

fn seed_anthropic_profile(
    app: &tauri::App<tauri::test::MockRuntime>,
    profile_id: &str,
    model_id: &str,
    api_key: Option<&str>,
) {
    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: profile_id.into(),
            provider_id: "anthropic".into(),
            label: "Anthropic Work".into(),
            model_id: model_id.into(),
            openrouter_api_key: None,
            anthropic_api_key: api_key.map(str::to_string),
            activate: false,
        },
    )
    .expect("seed anthropic profile");
}

#[test]
fn get_provider_model_catalog_rejects_blank_and_unknown_profile_ids() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));

    let blank = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "   ".into(),
            force_refresh: false,
        },
    )
    .expect_err("blank profile id should fail");
    assert_eq!(blank.code, "invalid_request");

    let missing = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "missing-profile".into(),
            force_refresh: false,
        },
    )
    .expect_err("unknown profile id should fail");
    assert_eq!(missing.code, "provider_profile_not_found");
}

#[test]
fn get_provider_model_catalog_projects_openai_codex_as_single_model_truth() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));

    let catalog = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "openai_codex-default".into(),
            force_refresh: true,
        },
    )
    .expect("project openai codex catalog");

    assert_eq!(catalog.source, ProviderModelCatalogSourceDto::Live);
    assert_eq!(catalog.profile_id, "openai_codex-default");
    assert_eq!(catalog.configured_model_id, "openai_codex");
    assert_eq!(catalog.models.len(), 1);
    assert_eq!(catalog.models[0].model_id, "openai_codex");
    assert!(catalog.models[0].thinking.supported);
    assert_eq!(
        catalog.models[0].thinking.default_effort,
        Some(ProviderModelThinkingEffortDto::Medium)
    );
}

#[test]
fn get_provider_model_catalog_discovers_inactive_openrouter_profile_and_persists_secret_free_cache()
{
    let models_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"openai/o4-mini","name":"OpenAI o4-mini","supported_parameters":["reasoning"]},{"id":"anthropic/claude-3.7-sonnet","name":"Claude 3.7 Sonnet","supported_parameters":[]}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let state = create_state(&root).with_openrouter_auth_config_override(openrouter_auth_config(
        format!("{models_base_url}/api/v1/models"),
    ));
    let app = build_mock_app(state);
    let secret = "sk-or-v1-secret-value";
    seed_openrouter_profile(&app, "openrouter-work", "openai/o4-mini", secret);

    let catalog = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "openrouter-work".into(),
            force_refresh: true,
        },
    )
    .expect("discover inactive openrouter profile catalog");

    assert_eq!(catalog.source, ProviderModelCatalogSourceDto::Live);
    assert_eq!(catalog.profile_id, "openrouter-work");
    assert_eq!(catalog.configured_model_id, "openai/o4-mini");
    assert_eq!(catalog.models.len(), 2);

    let reasoning_model = catalog
        .models
        .iter()
        .find(|model| model.model_id == "openai/o4-mini")
        .expect("reasoning model should be present");
    assert!(reasoning_model.thinking.supported);
    assert!(reasoning_model
        .thinking
        .effort_options
        .contains(&ProviderModelThinkingEffortDto::XHigh));

    let non_reasoning_model = catalog
        .models
        .iter()
        .find(|model| model.model_id == "anthropic/claude-3.7-sonnet")
        .expect("non-reasoning model should be present");
    assert!(!non_reasoning_model.thinking.supported);

    let cache = std::fs::read_to_string(catalog_cache_path(&root)).expect("read catalog cache");
    assert!(!cache.contains(secret));
}

#[test]
fn get_provider_model_catalog_returns_cached_snapshot_when_live_refresh_fails() {
    let success_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"openai/o4-mini","name":"OpenAI o4-mini","supported_parameters":["reasoning"]}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let first_state = create_state(&root).with_openrouter_auth_config_override(
        openrouter_auth_config(format!("{success_base_url}/api/v1/models")),
    );
    let first_app = build_mock_app(first_state);
    seed_openrouter_profile(
        &first_app,
        "openrouter-work",
        "openai/o4-mini",
        "sk-or-v1-first",
    );

    let first = get_provider_model_catalog(
        first_app.handle().clone(),
        first_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "openrouter-work".into(),
            force_refresh: true,
        },
    )
    .expect("seed live catalog");

    let failing_base_url = spawn_static_http_server(503, r#"{"error":"down"}"#);
    let second_state = create_state(&root).with_openrouter_auth_config_override(
        openrouter_auth_config(format!("{failing_base_url}/api/v1/models")),
    );
    let second_app = build_mock_app(second_state);

    let cached = get_provider_model_catalog(
        second_app.handle().clone(),
        second_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "openrouter-work".into(),
            force_refresh: true,
        },
    )
    .expect("fall back to cached catalog");

    assert_eq!(cached.source, ProviderModelCatalogSourceDto::Cache);
    assert_eq!(cached.fetched_at, first.fetched_at);
    assert_eq!(cached.last_success_at, first.last_success_at);
    assert_eq!(
        cached
            .last_refresh_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("openrouter_provider_unavailable")
    );
}

#[test]
fn get_provider_model_catalog_rejects_malformed_live_payload_and_preserves_cached_snapshot() {
    let success_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"openai/o4-mini","name":"OpenAI o4-mini","supported_parameters":["reasoning"]}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let first_state = create_state(&root).with_openrouter_auth_config_override(
        openrouter_auth_config(format!("{success_base_url}/api/v1/models")),
    );
    let first_app = build_mock_app(first_state);
    seed_openrouter_profile(
        &first_app,
        "openrouter-work",
        "openai/o4-mini",
        "sk-or-v1-first",
    );

    get_provider_model_catalog(
        first_app.handle().clone(),
        first_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "openrouter-work".into(),
            force_refresh: true,
        },
    )
    .expect("seed live catalog");

    let malformed_base_url = spawn_static_http_server(200, r#"{"data":[{"id":"openai/o4-mini"}]}"#);
    let second_state = create_state(&root).with_openrouter_auth_config_override(
        openrouter_auth_config(format!("{malformed_base_url}/api/v1/models")),
    );
    let second_app = build_mock_app(second_state);

    let cached = get_provider_model_catalog(
        second_app.handle().clone(),
        second_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "openrouter-work".into(),
            force_refresh: true,
        },
    )
    .expect("fall back to cached catalog after malformed payload");

    assert_eq!(cached.source, ProviderModelCatalogSourceDto::Cache);
    assert_eq!(
        cached
            .last_refresh_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("openrouter_models_decode_failed")
    );
}

#[test]
fn get_provider_model_catalog_ignores_corrupt_cache_row_and_stays_read_only_until_repaired() {
    let models_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"openai/o4-mini","name":"OpenAI o4-mini","supported_parameters":["reasoning"]}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let state = create_state(&root).with_openrouter_auth_config_override(openrouter_auth_config(
        format!("{models_base_url}/api/v1/models"),
    ));
    let app = build_mock_app(state);
    seed_openrouter_profile(
        &app,
        "openrouter-work",
        "missing/from-provider",
        "sk-or-v1-first",
    );

    std::fs::create_dir_all(root.path().join("app-data")).expect("create app-data dir");
    let corrupt = r#"{
  "version": 1,
  "catalogs": {
    "openrouter-work": {
      "providerId": "openrouter",
      "fetchedAt": "2026-04-21T12:00:00Z"
    }
  }
}"#;
    std::fs::write(catalog_cache_path(&root), corrupt).expect("write corrupt cache file");

    let catalog = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "openrouter-work".into(),
            force_refresh: false,
        },
    )
    .expect("return live catalog even with corrupt cache row");

    assert_eq!(catalog.source, ProviderModelCatalogSourceDto::Live);
    assert_eq!(
        catalog
            .last_refresh_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("provider_model_catalog_cache_decode_failed")
    );

    let cache = std::fs::read_to_string(catalog_cache_path(&root)).expect("read cache file");
    assert_eq!(cache, corrupt);
}

#[test]
fn get_provider_model_catalog_discovers_anthropic_profile_with_truthful_thinking_and_secret_free_cache() {
    let models_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"claude-3-7-sonnet-latest","display_name":"Claude 3.7 Sonnet","capabilities":{"effort":{"supported":true,"low":{"supported":true},"medium":{"supported":true},"high":{"supported":true},"xhigh":{"supported":true}}}},{"id":"claude-3-5-haiku-latest","display_name":"Claude 3.5 Haiku"}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let state = create_state(&root).with_anthropic_auth_config_override(anthropic_auth_config(
        format!("{models_base_url}/v1/models"),
    ));
    let app = build_mock_app(state);
    let secret = "sk-ant-api03-secret-value";
    seed_anthropic_profile(
        &app,
        "anthropic-work",
        "claude-3-7-sonnet-latest",
        Some(secret),
    );

    let catalog = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "anthropic-work".into(),
            force_refresh: true,
        },
    )
    .expect("discover anthropic profile catalog");

    assert_eq!(catalog.source, ProviderModelCatalogSourceDto::Live);
    assert_eq!(catalog.provider_id, "anthropic");
    assert_eq!(catalog.configured_model_id, "claude-3-7-sonnet-latest");
    assert_eq!(catalog.models.len(), 2);

    let sonnet = catalog
        .models
        .iter()
        .find(|model| model.model_id == "claude-3-7-sonnet-latest")
        .expect("claude sonnet should be present");
    assert!(sonnet.thinking.supported);
    assert_eq!(
        sonnet.thinking.default_effort,
        Some(ProviderModelThinkingEffortDto::Medium)
    );
    assert!(sonnet
        .thinking
        .effort_options
        .contains(&ProviderModelThinkingEffortDto::XHigh));

    let haiku = catalog
        .models
        .iter()
        .find(|model| model.model_id == "claude-3-5-haiku-latest")
        .expect("claude haiku should be present");
    assert!(!haiku.thinking.supported);

    let cache = std::fs::read_to_string(catalog_cache_path(&root)).expect("read catalog cache");
    assert!(!cache.contains(secret));
}

#[test]
fn get_provider_model_catalog_returns_unavailable_for_anthropic_profile_without_api_key() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    seed_anthropic_profile(&app, "anthropic-work", "claude-3-7-sonnet-latest", None);

    let catalog = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "anthropic-work".into(),
            force_refresh: true,
        },
    )
    .expect("surface unavailable anthropic catalog without api key");

    assert_eq!(catalog.source, ProviderModelCatalogSourceDto::Unavailable);
    assert!(catalog.models.is_empty());
    assert_eq!(
        catalog
            .last_refresh_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("anthropic_api_key_missing")
    );
}

#[test]
fn get_provider_model_catalog_returns_cached_anthropic_snapshot_when_live_refresh_is_rate_limited() {
    let success_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"claude-3-7-sonnet-latest","display_name":"Claude 3.7 Sonnet","capabilities":{"effort":{"supported":true,"medium":{"supported":true},"high":{"supported":true}}}}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let first_state = create_state(&root).with_anthropic_auth_config_override(
        anthropic_auth_config(format!("{success_base_url}/v1/models")),
    );
    let first_app = build_mock_app(first_state);
    seed_anthropic_profile(
        &first_app,
        "anthropic-work",
        "claude-3-7-sonnet-latest",
        Some("sk-ant-api03-first"),
    );

    let first = get_provider_model_catalog(
        first_app.handle().clone(),
        first_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "anthropic-work".into(),
            force_refresh: true,
        },
    )
    .expect("seed live anthropic catalog");

    let failing_base_url = spawn_static_http_server(429, r#"{"error":"rate limited"}"#);
    let second_state = create_state(&root).with_anthropic_auth_config_override(
        anthropic_auth_config(format!("{failing_base_url}/v1/models")),
    );
    let second_app = build_mock_app(second_state);

    let cached = get_provider_model_catalog(
        second_app.handle().clone(),
        second_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "anthropic-work".into(),
            force_refresh: true,
        },
    )
    .expect("fall back to cached anthropic catalog");

    assert_eq!(cached.source, ProviderModelCatalogSourceDto::Cache);
    assert_eq!(cached.fetched_at, first.fetched_at);
    assert_eq!(cached.last_success_at, first.last_success_at);
    assert_eq!(
        cached
            .last_refresh_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("anthropic_rate_limited")
    );
}

#[test]
fn get_provider_model_catalog_rejects_malformed_anthropic_live_payload_and_preserves_cached_snapshot() {
    let success_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"claude-3-7-sonnet-latest","display_name":"Claude 3.7 Sonnet","capabilities":{"effort":{"supported":true,"medium":{"supported":true},"high":{"supported":true}}}}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let first_state = create_state(&root).with_anthropic_auth_config_override(
        anthropic_auth_config(format!("{success_base_url}/v1/models")),
    );
    let first_app = build_mock_app(first_state);
    seed_anthropic_profile(
        &first_app,
        "anthropic-work",
        "claude-3-7-sonnet-latest",
        Some("sk-ant-api03-first"),
    );

    get_provider_model_catalog(
        first_app.handle().clone(),
        first_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "anthropic-work".into(),
            force_refresh: true,
        },
    )
    .expect("seed live anthropic catalog");

    let malformed_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"claude-3-7-sonnet-latest","display_name":"Claude 3.7 Sonnet","capabilities":{"thinking":{"supported":true}}}]}"#,
    );
    let second_state = create_state(&root).with_anthropic_auth_config_override(
        anthropic_auth_config(format!("{malformed_base_url}/v1/models")),
    );
    let second_app = build_mock_app(second_state);

    let cached = get_provider_model_catalog(
        second_app.handle().clone(),
        second_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "anthropic-work".into(),
            force_refresh: true,
        },
    )
    .expect("fall back to cached anthropic catalog after malformed payload");

    assert_eq!(cached.source, ProviderModelCatalogSourceDto::Cache);
    assert_eq!(
        cached
            .last_refresh_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("anthropic_models_decode_failed")
    );
}
