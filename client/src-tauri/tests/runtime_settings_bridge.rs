use std::path::PathBuf;

use cadence_desktop_lib::{
    commands::{
        get_runtime_settings::get_runtime_settings,
        upsert_runtime_settings::upsert_runtime_settings, CommandError, RuntimeSettingsDto,
        UpsertRuntimeSettingsRequestDto,
    },
    configure_builder_with_state,
    state::DesktopState,
};
use serde_json::{json, Value};
use tauri::Manager;
use tempfile::TempDir;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> (DesktopState, PathBuf, PathBuf) {
    let settings_path = root.path().join("app-data").join("runtime-settings.json");
    let credentials_path = root
        .path()
        .join("app-data")
        .join("openrouter-credentials.json");

    (
        DesktopState::default()
            .with_runtime_settings_file_override(settings_path.clone())
            .with_openrouter_credential_file_override(credentials_path.clone()),
        settings_path,
        credentials_path,
    )
}

#[test]
fn get_runtime_settings_returns_redacted_default_when_no_files_exist() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, settings_path, credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    let settings = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect("load default runtime settings");

    assert_eq!(
        settings,
        RuntimeSettingsDto {
            provider_id: "openai_codex".into(),
            model_id: "openai_codex".into(),
            openrouter_api_key_configured: false,
        }
    );
    assert!(!settings_path.exists());
    assert!(!credentials_path.exists());
}

#[test]
fn upsert_runtime_settings_persists_redacted_settings_without_secret_in_settings_file() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, settings_path, credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    let response = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("credential-value-1".into()),
        },
    )
    .expect("save runtime settings");

    assert_eq!(
        response,
        RuntimeSettingsDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key_configured: true,
        }
    );

    let settings_file = std::fs::read_to_string(&settings_path).expect("read settings file");
    assert!(settings_file.contains("\"openrouterApiKeyConfigured\": true"));
    assert!(!settings_file.contains("credential-value-1"));

    let credential_file = std::fs::read_to_string(&credentials_path).expect("read credentials file");
    assert!(credential_file.contains("\"apiKey\": \"credential-value-1\""));
}

#[test]
fn upsert_runtime_settings_preserves_existing_openrouter_key_when_request_omits_it() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _settings_path, credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("credential-value-1".into()),
        },
    )
    .expect("seed runtime settings");

    let response = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4.1-mini".into(),
            openrouter_api_key: None,
        },
    )
    .expect("preserve runtime credential");

    assert_eq!(response.provider_id, "openrouter");
    assert_eq!(response.model_id, "openai/gpt-4.1-mini");
    assert!(response.openrouter_api_key_configured);

    let credential_file = std::fs::read_to_string(&credentials_path).expect("read credentials file");
    assert!(credential_file.contains("credential-value-1"));
}

#[test]
fn upsert_runtime_settings_clears_openrouter_key_when_request_uses_empty_string() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _settings_path, credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("credential-value-1".into()),
        },
    )
    .expect("seed runtime settings");

    let response = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("   ".into()),
        },
    )
    .expect("clear runtime credential");

    assert_eq!(
        response,
        RuntimeSettingsDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key_configured: false,
        }
    );
    assert!(!credentials_path.exists());
}

#[test]
fn upsert_runtime_settings_rolls_back_settings_when_credential_write_fails() {
    let root = tempfile::tempdir().expect("temp dir");
    let blocked_parent = root.path().join("blocked-parent");
    std::fs::write(&blocked_parent, "not-a-directory").expect("create blocking file");

    let settings_path = root.path().join("app-data").join("runtime-settings.json");
    let credentials_path = blocked_parent.join("openrouter-credentials.json");
    let state = DesktopState::default()
        .with_runtime_settings_file_override(settings_path.clone())
        .with_openrouter_credential_file_override(credentials_path);
    let app = build_mock_app(state);

    let error = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("credential-value-rollback".into()),
        },
    )
    .expect_err("credential write failure should roll back settings");

    assert_eq!(error.code, "openrouter_credentials_directory_unavailable");

    let settings = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect("settings load after rollback");
    assert_eq!(
        settings,
        RuntimeSettingsDto {
            provider_id: "openai_codex".into(),
            model_id: "openai_codex".into(),
            openrouter_api_key_configured: false,
        }
    );
    assert!(!settings_path.exists());
}

#[test]
fn get_runtime_settings_rejects_invalid_settings_json() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, settings_path, _credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    let parent = settings_path.parent().expect("settings parent");
    std::fs::create_dir_all(parent).expect("create settings parent");
    std::fs::write(&settings_path, "{not-json").expect("write malformed settings");

    let error = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("malformed settings json should fail");
    assert_eq!(error.code, "runtime_settings_decode_failed");
}

#[test]
fn get_runtime_settings_rejects_invalid_openrouter_credentials_json() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _settings_path, credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    let parent = credentials_path.parent().expect("credentials parent");
    std::fs::create_dir_all(parent).expect("create credentials parent");
    std::fs::write(&credentials_path, "{not-json").expect("write malformed credentials");

    let error = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("malformed credentials json should fail");
    assert_eq!(error.code, "openrouter_credentials_decode_failed");
}

#[test]
fn get_runtime_settings_rejects_credentials_without_matching_settings_file() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _settings_path, credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    let parent = credentials_path.parent().expect("credentials parent");
    std::fs::create_dir_all(parent).expect("create credentials parent");
    std::fs::write(
        &credentials_path,
        serde_json::to_vec_pretty(&json!({
            "apiKey": "credential-value-1",
            "updatedAt": "2026-04-19T21:00:00Z"
        }))
        .expect("serialize credentials json"),
    )
    .expect("write credentials file");

    let error = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("credentials without settings should fail closed");
    assert_eq!(error.code, "runtime_settings_contract_failed");
}

#[test]
fn get_runtime_settings_rejects_mismatched_redacted_key_state() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, settings_path, _credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    let parent = settings_path.parent().expect("settings parent");
    std::fs::create_dir_all(parent).expect("create settings parent");
    std::fs::write(
        &settings_path,
        serde_json::to_vec_pretty(&json!({
            "providerId": "openrouter",
            "modelId": "openai/gpt-4o-mini",
            "openrouterApiKeyConfigured": true,
            "updatedAt": "2026-04-19T21:00:00Z"
        }))
        .expect("serialize settings json"),
    )
    .expect("write settings file");

    let error = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("missing credential file should fail closed");
    assert_eq!(error.code, "runtime_settings_contract_failed");
}

#[test]
fn upsert_runtime_settings_rejects_blank_provider_id() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _settings_path, _credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    let error = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "   ".into(),
            model_id: "openai_codex".into(),
            openrouter_api_key: None,
        },
    )
    .expect_err("blank provider id should fail");

    assert_eq!(error, CommandError::invalid_request("providerId"));
}

#[test]
fn upsert_runtime_settings_rejects_blank_model_id() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _settings_path, _credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    let error = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openai_codex".into(),
            model_id: "   ".into(),
            openrouter_api_key: None,
        },
    )
    .expect_err("blank model id should fail");

    assert_eq!(error, CommandError::invalid_request("modelId"));
}

#[test]
fn upsert_runtime_settings_rejects_unsupported_provider_id() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _settings_path, _credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    let error = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "azure_openai".into(),
            model_id: "azure_openai".into(),
            openrouter_api_key: None,
        },
    )
    .expect_err("unsupported provider should fail closed");

    assert_eq!(error.code, "runtime_settings_request_invalid");
}

#[test]
fn upsert_runtime_settings_rejects_invalid_runtime_settings_file_on_preserve() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, settings_path, _credentials_path) = create_state(&root);
    let app = build_mock_app(state);

    let parent = settings_path.parent().expect("settings parent");
    std::fs::create_dir_all(parent).expect("create settings parent");
    std::fs::write(
        &settings_path,
        serde_json::to_vec_pretty(&Value::Object(
            [
                ("providerId".into(), Value::String("openrouter".into())),
                ("modelId".into(), Value::String("openai/gpt-4o-mini".into())),
                ("openrouterApiKeyConfigured".into(), Value::Bool(true)),
                ("updatedAt".into(), Value::String("2026-04-19T21:00:00Z".into())),
            ]
            .into_iter()
            .collect(),
        ))
        .expect("serialize settings json"),
    )
    .expect("write settings file");

    let error = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: None,
        },
    )
    .expect_err("preserve should fail when current state is mismatched");

    assert_eq!(error.code, "runtime_settings_contract_failed");
}
