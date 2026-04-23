use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    thread,
    time::Duration,
};

use cadence_desktop_lib::{
    auth::{AnthropicAuthConfig, OpenAiCompatibleAuthConfig, OpenRouterAuthConfig},
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

fn openai_compatible_auth_config(github_models_catalog_url: String) -> OpenAiCompatibleAuthConfig {
    OpenAiCompatibleAuthConfig {
        github_models_catalog_url,
        timeout: Duration::from_secs(5),
        ..OpenAiCompatibleAuthConfig::default()
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
            runtime_kind: "openrouter".into(),
            label: "OpenRouter Work".into(),
            model_id: model_id.into(),
            preset_id: Some("openrouter".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some(api_key.into()),
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
            runtime_kind: "anthropic".into(),
            label: "Anthropic Work".into(),
            model_id: model_id.into(),
            preset_id: Some("anthropic".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: api_key.map(str::to_string),
            activate: false,
        },
    )
    .expect("seed anthropic profile");
}

fn seed_openai_compatible_profile(
    app: &tauri::App<tauri::test::MockRuntime>,
    profile_id: &str,
    provider_id: &str,
    runtime_kind: &str,
    model_id: &str,
    preset_id: Option<&str>,
    base_url: Option<&str>,
    api_version: Option<&str>,
    api_key: Option<&str>,
) {
    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: profile_id.into(),
            provider_id: provider_id.into(),
            runtime_kind: runtime_kind.into(),
            label: profile_id.into(),
            model_id: model_id.into(),
            preset_id: preset_id.map(str::to_string),
            base_url: base_url.map(str::to_string),
            api_version: api_version.map(str::to_string),
            region: None,
            project_id: None,
            api_key: api_key.map(str::to_string),
            activate: false,
        },
    )
    .expect("seed openai-compatible profile");
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
fn get_provider_model_catalog_discovers_anthropic_profile_with_truthful_thinking_and_secret_free_cache(
) {
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
fn get_provider_model_catalog_returns_cached_anthropic_snapshot_when_live_refresh_is_rate_limited()
{
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
fn get_provider_model_catalog_discovers_github_models_profile_with_live_catalog_truth() {
    let catalog_base_url = spawn_static_http_server(
        200,
        r#"[{"id":"openai/gpt-4.1","name":"OpenAI GPT-4.1","capabilities":["streaming","tool-calling"]},{"id":"meta/llama-3.3-70b-instruct","name":"Meta Llama 3.3 70B Instruct","capabilities":["streaming"]}]"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let state = create_state(&root).with_openai_compatible_auth_config_override(
        openai_compatible_auth_config(format!("{catalog_base_url}/catalog/models")),
    );
    let app = build_mock_app(state);
    let secret = "github_pat_test_secret";
    seed_openai_compatible_profile(
        &app,
        "github-models-work",
        "github_models",
        "openai_compatible",
        "openai/gpt-4.1",
        Some("github_models"),
        None,
        None,
        Some(secret),
    );

    let catalog = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "github-models-work".into(),
            force_refresh: true,
        },
    )
    .expect("discover github models catalog");

    assert_eq!(catalog.source, ProviderModelCatalogSourceDto::Live);
    assert_eq!(catalog.provider_id, "github_models");
    assert_eq!(catalog.configured_model_id, "openai/gpt-4.1");
    assert_eq!(catalog.models.len(), 2);

    let configured = catalog
        .models
        .iter()
        .find(|model| model.model_id == "openai/gpt-4.1")
        .expect("configured GitHub model should be present");
    assert_eq!(configured.display_name, "OpenAI GPT-4.1");
    assert!(!configured.thinking.supported);
    assert!(configured.thinking.default_effort.is_none());

    let cache = std::fs::read_to_string(catalog_cache_path(&root)).expect("read catalog cache");
    assert!(!cache.contains(secret));
    assert!(cache.contains("github_models"));
}

#[test]
fn get_provider_model_catalog_returns_unavailable_for_github_models_profile_without_token() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    seed_openai_compatible_profile(
        &app,
        "github-models-work",
        "github_models",
        "openai_compatible",
        "openai/gpt-4.1",
        Some("github_models"),
        None,
        None,
        None,
    );

    let catalog = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "github-models-work".into(),
            force_refresh: true,
        },
    )
    .expect("surface unavailable github catalog without token");

    assert_eq!(catalog.source, ProviderModelCatalogSourceDto::Unavailable);
    assert!(catalog.models.is_empty());
    assert_eq!(
        catalog
            .last_refresh_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("github_models_token_missing")
    );
}

#[test]
fn get_provider_model_catalog_returns_cached_github_snapshot_when_refresh_fails() {
    let success_base_url =
        spawn_static_http_server(200, r#"[{"id":"openai/gpt-4.1","name":"OpenAI GPT-4.1"}]"#);
    let root = tempfile::tempdir().expect("temp dir");
    let first_state = create_state(&root).with_openai_compatible_auth_config_override(
        openai_compatible_auth_config(format!("{success_base_url}/catalog/models")),
    );
    let first_app = build_mock_app(first_state);
    seed_openai_compatible_profile(
        &first_app,
        "github-models-work",
        "github_models",
        "openai_compatible",
        "openai/gpt-4.1",
        Some("github_models"),
        None,
        None,
        Some("github_pat_first"),
    );

    let first = get_provider_model_catalog(
        first_app.handle().clone(),
        first_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "github-models-work".into(),
            force_refresh: true,
        },
    )
    .expect("seed live github catalog");

    let failing_base_url = spawn_static_http_server(503, r#"{"error":"down"}"#);
    let second_state = create_state(&root).with_openai_compatible_auth_config_override(
        openai_compatible_auth_config(format!("{failing_base_url}/catalog/models")),
    );
    let second_app = build_mock_app(second_state);

    let cached = get_provider_model_catalog(
        second_app.handle().clone(),
        second_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "github-models-work".into(),
            force_refresh: true,
        },
    )
    .expect("fall back to cached github catalog");

    assert_eq!(cached.source, ProviderModelCatalogSourceDto::Cache);
    assert_eq!(cached.fetched_at, first.fetched_at);
    assert_eq!(cached.last_success_at, first.last_success_at);
    assert_eq!(
        cached
            .last_refresh_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("github_models_provider_unavailable")
    );
}

#[test]
fn get_provider_model_catalog_rejects_malformed_github_catalog_and_preserves_cached_snapshot() {
    let success_base_url =
        spawn_static_http_server(200, r#"[{"id":"openai/gpt-4.1","name":"OpenAI GPT-4.1"}]"#);
    let root = tempfile::tempdir().expect("temp dir");
    let first_state = create_state(&root).with_openai_compatible_auth_config_override(
        openai_compatible_auth_config(format!("{success_base_url}/catalog/models")),
    );
    let first_app = build_mock_app(first_state);
    seed_openai_compatible_profile(
        &first_app,
        "github-models-work",
        "github_models",
        "openai_compatible",
        "openai/gpt-4.1",
        Some("github_models"),
        None,
        None,
        Some("github_pat_first"),
    );

    get_provider_model_catalog(
        first_app.handle().clone(),
        first_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "github-models-work".into(),
            force_refresh: true,
        },
    )
    .expect("seed live github catalog");

    let malformed_base_url = spawn_static_http_server(
        200,
        r#"[{"id":"openai/gpt-4.1","name":"OpenAI GPT-4.1"},{"id":"   ","name":"Broken"}]"#,
    );
    let second_state = create_state(&root).with_openai_compatible_auth_config_override(
        openai_compatible_auth_config(format!("{malformed_base_url}/catalog/models")),
    );
    let second_app = build_mock_app(second_state);

    let cached = get_provider_model_catalog(
        second_app.handle().clone(),
        second_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "github-models-work".into(),
            force_refresh: true,
        },
    )
    .expect("fall back to cached github catalog after malformed payload");

    assert_eq!(cached.source, ProviderModelCatalogSourceDto::Cache);
    assert_eq!(
        cached
            .last_refresh_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("github_models_models_decode_failed")
    );
}

#[test]
fn get_provider_model_catalog_discovers_openai_compatible_profile_with_live_models() {
    let base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"gpt-4.1-mini","display_name":"GPT-4.1 Mini","capabilities":{"reasoning":{"supported":true,"effortOptions":["low","medium","high"],"defaultEffort":"medium"}}},{"id":"gpt-4.1-nano","display_name":"GPT-4.1 Nano"}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let secret = "sk-openai-live-secret";
    seed_openai_compatible_profile(
        &app,
        "openai-compatible-work",
        "openai_api",
        "openai_compatible",
        "gpt-4.1-mini",
        Some("openai_api"),
        Some(&base_url),
        Some("2025-03-01-preview"),
        Some(secret),
    );

    let catalog = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "openai-compatible-work".into(),
            force_refresh: true,
        },
    )
    .expect("discover openai-compatible profile catalog");

    assert_eq!(catalog.source, ProviderModelCatalogSourceDto::Live);
    assert_eq!(catalog.provider_id, "openai_api");
    assert_eq!(catalog.configured_model_id, "gpt-4.1-mini");
    assert_eq!(catalog.models.len(), 2);
    assert!(
        catalog
            .models
            .iter()
            .find(|model| model.model_id == "gpt-4.1-mini")
            .expect("configured model should be present")
            .thinking
            .supported
    );

    let cache = std::fs::read_to_string(catalog_cache_path(&root)).expect("read catalog cache");
    assert!(!cache.contains(secret));
}

#[test]
fn get_provider_model_catalog_invalidates_openai_compatible_cache_when_endpoint_metadata_changes() {
    let first_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"gpt-4.1-mini","displayName":"First Endpoint"}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let first_app = build_mock_app(create_state(&root));
    seed_openai_compatible_profile(
        &first_app,
        "openai-compatible-work",
        "openai_api",
        "openai_compatible",
        "gpt-4.1-mini",
        Some("openai_api"),
        Some(&first_base_url),
        None,
        Some("sk-openai-first"),
    );

    let first = get_provider_model_catalog(
        first_app.handle().clone(),
        first_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "openai-compatible-work".into(),
            force_refresh: true,
        },
    )
    .expect("seed first openai-compatible catalog");
    assert_eq!(first.models[0].display_name, "First Endpoint");

    let second_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"gpt-4.1-mini","displayName":"Second Endpoint"}]}"#,
    );
    let second_app = build_mock_app(create_state(&root));
    seed_openai_compatible_profile(
        &second_app,
        "openai-compatible-work",
        "openai_api",
        "openai_compatible",
        "gpt-4.1-mini",
        Some("openai_api"),
        Some(&second_base_url),
        Some("2026-04-01-preview"),
        Some("sk-openai-second"),
    );

    let refreshed = get_provider_model_catalog(
        second_app.handle().clone(),
        second_app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "openai-compatible-work".into(),
            force_refresh: false,
        },
    )
    .expect("refresh catalog after endpoint metadata change");

    assert_eq!(refreshed.source, ProviderModelCatalogSourceDto::Live);
    assert_eq!(refreshed.models[0].display_name, "Second Endpoint");
    assert_ne!(refreshed.fetched_at, first.fetched_at);
}

#[test]
fn get_provider_model_catalog_projects_manual_azure_model_truth_when_list_models_is_unsupported() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    seed_openai_compatible_profile(
        &app,
        "azure-work",
        "azure_openai",
        "openai_compatible",
        "gpt-4.1-mini",
        Some("azure_openai"),
        Some("https://azure.example.invalid/openai/deployments/work"),
        Some("2025-04-01-preview"),
        Some("azure-secret"),
    );

    let catalog = get_provider_model_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetProviderModelCatalogRequestDto {
            profile_id: "azure-work".into(),
            force_refresh: true,
        },
    )
    .expect("project manual azure catalog");

    assert_eq!(catalog.source, ProviderModelCatalogSourceDto::Manual);
    assert_eq!(catalog.provider_id, "azure_openai");
    assert_eq!(catalog.models.len(), 1);
    assert_eq!(catalog.models[0].model_id, "gpt-4.1-mini");
    assert!(!catalog.models[0].thinking.supported);
}
