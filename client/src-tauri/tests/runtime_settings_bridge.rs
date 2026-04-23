use std::path::PathBuf;

use cadence_desktop_lib::{
    commands::{
        get_runtime_settings::get_runtime_settings,
        upsert_runtime_settings::upsert_runtime_settings, CommandError, RuntimeSettingsDto,
        UpsertRuntimeSettingsRequestDto,
    },
    configure_builder_with_state,
    provider_profiles::{
        ProviderProfileCredentialLink, ProviderProfileRecord, ProviderProfilesMetadataFile,
        ProviderProfilesMigrationState,
    },
    state::DesktopState,
};
use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

#[derive(Debug)]
struct TestPaths {
    provider_profiles_path: PathBuf,
    provider_profile_credentials_path: PathBuf,
    legacy_settings_path: PathBuf,
    legacy_openrouter_credentials_path: PathBuf,
    legacy_openai_auth_path: PathBuf,
}

fn create_state(root: &TempDir) -> (DesktopState, TestPaths) {
    let paths = TestPaths {
        provider_profiles_path: root.path().join("app-data").join("provider-profiles.json"),
        provider_profile_credentials_path: root
            .path()
            .join("app-data")
            .join("provider-profile-credentials.json"),
        legacy_settings_path: root.path().join("app-data").join("runtime-settings.json"),
        legacy_openrouter_credentials_path: root
            .path()
            .join("app-data")
            .join("openrouter-credentials.json"),
        legacy_openai_auth_path: root.path().join("app-data").join("openai-auth.json"),
    };

    (
        DesktopState::default()
            .with_provider_profiles_file_override(paths.provider_profiles_path.clone())
            .with_provider_profile_credential_store_file_override(
                paths.provider_profile_credentials_path.clone(),
            )
            .with_runtime_settings_file_override(paths.legacy_settings_path.clone())
            .with_openrouter_credential_file_override(
                paths.legacy_openrouter_credentials_path.clone(),
            )
            .with_auth_store_file_override(paths.legacy_openai_auth_path.clone()),
        paths,
    )
}

#[test]
fn get_runtime_settings_returns_redacted_default_when_no_files_exist() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    let settings = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect("load default runtime settings");

    assert_eq!(
        settings,
        RuntimeSettingsDto {
            provider_id: "openai_codex".into(),
            model_id: "openai_codex".into(),
            openrouter_api_key_configured: false,
            anthropic_api_key_configured: false,
        }
    );
    assert!(!paths.provider_profiles_path.exists());
    assert!(!paths.provider_profile_credentials_path.exists());
    assert!(!paths.legacy_settings_path.exists());
    assert!(!paths.legacy_openrouter_credentials_path.exists());
}

#[test]
fn upsert_runtime_settings_persists_redacted_provider_profile_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    let response = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4.1-mini".into(),
            openrouter_api_key: Some("credential-value-1".into()),
            anthropic_api_key: None,
        },
    )
    .expect("save runtime settings");

    assert_eq!(
        response,
        RuntimeSettingsDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4.1-mini".into(),
            openrouter_api_key_configured: true,
            anthropic_api_key_configured: false,
        }
    );

    let metadata_file =
        std::fs::read_to_string(&paths.provider_profiles_path).expect("read provider profiles");
    assert!(metadata_file.contains("\"activeProfileId\": \"openrouter-default\""));
    assert!(metadata_file.contains("\"profileId\": \"openrouter-default\""));
    assert!(!metadata_file.contains("credential-value-1"));

    let credential_file = std::fs::read_to_string(&paths.provider_profile_credentials_path)
        .expect("read provider profile credentials");
    assert!(credential_file.contains("\"apiKey\": \"credential-value-1\""));
    assert!(!paths.legacy_settings_path.exists());
    assert!(!paths.legacy_openrouter_credentials_path.exists());
}

#[test]
fn upsert_runtime_settings_preserves_existing_openrouter_key_when_request_omits_it() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("credential-value-1".into()),
            anthropic_api_key: None,
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
            anthropic_api_key: None,
        },
    )
    .expect("preserve runtime credential");

    assert_eq!(response.provider_id, "openrouter");
    assert_eq!(response.model_id, "openai/gpt-4.1-mini");
    assert!(response.openrouter_api_key_configured);

    let credential_file = std::fs::read_to_string(&paths.provider_profile_credentials_path)
        .expect("read credential file");
    assert!(credential_file.contains("credential-value-1"));
}

#[test]
fn upsert_runtime_settings_persists_redacted_anthropic_provider_profile_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    let response = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "anthropic".into(),
            model_id: "claude-3-5-sonnet-latest".into(),
            openrouter_api_key: None,
            anthropic_api_key: Some("anthropic-secret-value-1".into()),
        },
    )
    .expect("save anthropic runtime settings");

    assert_eq!(
        response,
        RuntimeSettingsDto {
            provider_id: "anthropic".into(),
            model_id: "claude-3-5-sonnet-latest".into(),
            openrouter_api_key_configured: false,
            anthropic_api_key_configured: true,
        }
    );

    let metadata_file =
        std::fs::read_to_string(&paths.provider_profiles_path).expect("read provider profiles");
    assert!(metadata_file.contains("\"activeProfileId\": \"anthropic-default\""));
    assert!(metadata_file.contains("\"profileId\": \"anthropic-default\""));
    assert!(!metadata_file.contains("anthropic-secret-value-1"));

    let credential_file = std::fs::read_to_string(&paths.provider_profile_credentials_path)
        .expect("read provider profile credentials");
    assert!(credential_file.contains("\"apiKeys\""));
    assert!(credential_file.contains("\"apiKey\": \"anthropic-secret-value-1\""));

    let reloaded = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect("reload anthropic runtime settings");
    assert_eq!(reloaded, response);
}

#[test]
fn upsert_runtime_settings_clears_anthropic_key_when_request_uses_empty_string() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "anthropic".into(),
            model_id: "claude-3-5-sonnet-latest".into(),
            openrouter_api_key: None,
            anthropic_api_key: Some("anthropic-secret-value-1".into()),
        },
    )
    .expect("seed anthropic runtime settings");

    let response = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "anthropic".into(),
            model_id: "claude-3-5-sonnet-latest".into(),
            openrouter_api_key: None,
            anthropic_api_key: Some("   ".into()),
        },
    )
    .expect("clear anthropic credential");

    assert_eq!(
        response,
        RuntimeSettingsDto {
            provider_id: "anthropic".into(),
            model_id: "claude-3-5-sonnet-latest".into(),
            openrouter_api_key_configured: false,
            anthropic_api_key_configured: false,
        }
    );

    assert!(!paths.provider_profile_credentials_path.exists());
}

#[test]
fn upsert_runtime_settings_clears_openrouter_key_when_request_uses_empty_string() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("credential-value-1".into()),
            anthropic_api_key: None,
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
            anthropic_api_key: None,
        },
    )
    .expect("clear runtime credential");

    assert_eq!(
        response,
        RuntimeSettingsDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key_configured: false,
            anthropic_api_key_configured: false,
        }
    );
    assert!(!paths.provider_profile_credentials_path.exists());
}

#[test]
fn upsert_runtime_settings_rolls_back_metadata_when_profile_credential_write_fails() {
    let root = tempfile::tempdir().expect("temp dir");
    let blocked_parent = root.path().join("blocked-parent");
    std::fs::write(&blocked_parent, "not-a-directory").expect("create blocking file");

    let provider_profiles_path = root.path().join("app-data").join("provider-profiles.json");
    let state = DesktopState::default()
        .with_provider_profiles_file_override(provider_profiles_path.clone())
        .with_provider_profile_credential_store_file_override(
            blocked_parent.join("provider-profile-credentials.json"),
        )
        .with_runtime_settings_file_override(
            root.path().join("app-data").join("runtime-settings.json"),
        )
        .with_openrouter_credential_file_override(
            root.path()
                .join("app-data")
                .join("openrouter-credentials.json"),
        )
        .with_auth_store_file_override(root.path().join("app-data").join("openai-auth.json"));
    let app = build_mock_app(state);

    let error = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4.1-mini".into(),
            openrouter_api_key: Some("credential-value-rollback".into()),
            anthropic_api_key: None,
        },
    )
    .expect_err("credential write failure should roll back metadata");

    assert_eq!(
        error.code,
        "provider_profile_credentials_directory_unavailable"
    );

    let settings = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect("settings load after rollback");
    assert_eq!(
        settings,
        RuntimeSettingsDto {
            provider_id: "openai_codex".into(),
            model_id: "openai_codex".into(),
            openrouter_api_key_configured: false,
            anthropic_api_key_configured: false,
        }
    );
    assert!(!provider_profiles_path.exists());
}

#[test]
fn get_runtime_settings_rejects_invalid_legacy_settings_json() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    let parent = paths
        .legacy_settings_path
        .parent()
        .expect("settings parent");
    std::fs::create_dir_all(parent).expect("create settings parent");
    std::fs::write(&paths.legacy_settings_path, "{not-json").expect("write malformed settings");

    let error = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("malformed settings json should fail");
    assert_eq!(error.code, "runtime_settings_decode_failed");
    assert!(!paths.provider_profiles_path.exists());
}

#[test]
fn get_runtime_settings_rejects_invalid_legacy_openrouter_credentials_json() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    let parent = paths
        .legacy_openrouter_credentials_path
        .parent()
        .expect("credentials parent");
    std::fs::create_dir_all(parent).expect("create credentials parent");
    std::fs::write(&paths.legacy_openrouter_credentials_path, "{not-json")
        .expect("write malformed credentials");

    let error = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("malformed credentials json should fail");
    assert_eq!(error.code, "provider_profiles_migration_contract_failed");
}

#[test]
fn get_runtime_settings_rejects_legacy_credentials_without_matching_settings_file() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    let parent = paths
        .legacy_openrouter_credentials_path
        .parent()
        .expect("credentials parent");
    std::fs::create_dir_all(parent).expect("create credentials parent");
    std::fs::write(
        &paths.legacy_openrouter_credentials_path,
        serde_json::to_vec_pretty(&json!({
            "apiKey": "credential-value-1",
            "updatedAt": "2026-04-19T21:00:00Z"
        }))
        .expect("serialize credentials json"),
    )
    .expect("write credentials file");

    let error = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("credentials without settings should fail closed");
    assert_eq!(error.code, "provider_profiles_migration_contract_failed");
}

#[test]
fn get_runtime_settings_rejects_legacy_mismatched_redacted_key_state() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    let parent = paths
        .legacy_settings_path
        .parent()
        .expect("settings parent");
    std::fs::create_dir_all(parent).expect("create settings parent");
    std::fs::write(
        &paths.legacy_settings_path,
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
fn get_runtime_settings_rejects_blank_provider_id_in_legacy_settings() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    let parent = paths
        .legacy_settings_path
        .parent()
        .expect("settings parent");
    std::fs::create_dir_all(parent).expect("create settings parent");
    std::fs::write(
        &paths.legacy_settings_path,
        serde_json::to_vec_pretty(&json!({
            "providerId": "   ",
            "modelId": "openai_codex",
            "openrouterApiKeyConfigured": false,
            "updatedAt": "2026-04-19T21:00:00Z"
        }))
        .expect("serialize settings json"),
    )
    .expect("write settings file");

    let error = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("blank provider id should fail");
    assert_eq!(error.code, "runtime_settings_decode_failed");
}

#[test]
fn get_runtime_settings_rejects_blank_model_id_in_legacy_settings() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    let parent = paths
        .legacy_settings_path
        .parent()
        .expect("settings parent");
    std::fs::create_dir_all(parent).expect("create settings parent");
    std::fs::write(
        &paths.legacy_settings_path,
        serde_json::to_vec_pretty(&json!({
            "providerId": "openai_codex",
            "modelId": "   ",
            "openrouterApiKeyConfigured": false,
            "updatedAt": "2026-04-19T21:00:00Z"
        }))
        .expect("serialize settings json"),
    )
    .expect("write settings file");

    let error = get_runtime_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("blank model id should fail");
    assert_eq!(error.code, "runtime_settings_decode_failed");
}

#[test]
fn upsert_runtime_settings_rejects_blank_provider_id() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _paths) = create_state(&root);
    let app = build_mock_app(state);

    let error = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "   ".into(),
            model_id: "openai_codex".into(),
            openrouter_api_key: None,
            anthropic_api_key: None,
        },
    )
    .expect_err("blank provider id should fail");

    assert_eq!(error, CommandError::invalid_request("providerId"));
}

#[test]
fn upsert_runtime_settings_rejects_blank_model_id() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _paths) = create_state(&root);
    let app = build_mock_app(state);

    let error = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openai_codex".into(),
            model_id: "   ".into(),
            openrouter_api_key: None,
            anthropic_api_key: None,
        },
    )
    .expect_err("blank model id should fail");

    assert_eq!(error, CommandError::invalid_request("modelId"));
}

#[test]
fn upsert_runtime_settings_rejects_unsupported_provider_id() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _paths) = create_state(&root);
    let app = build_mock_app(state);

    let error = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "azure_openai".into(),
            model_id: "azure_openai".into(),
            openrouter_api_key: None,
            anthropic_api_key: None,
        },
    )
    .expect_err("unsupported provider should fail closed");

    assert_eq!(error.code, "runtime_settings_request_invalid");
}

#[test]
fn upsert_runtime_settings_rejects_github_models_compatibility_write() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _paths) = create_state(&root);
    let app = build_mock_app(state);

    let error = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "github_models".into(),
            model_id: "openai/gpt-4.1".into(),
            openrouter_api_key: None,
            anthropic_api_key: None,
        },
    )
    .expect_err("github models should stay provider-profile only");

    assert_eq!(error.code, "runtime_settings_request_invalid");
    assert!(error.message.contains("github_models"));
}

#[test]
fn upsert_runtime_settings_treats_missing_api_key_linkage_as_unconfigured() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, paths) = create_state(&root);
    let app = build_mock_app(state);

    let parent = paths
        .provider_profiles_path
        .parent()
        .expect("provider profile parent");
    std::fs::create_dir_all(parent).expect("create provider profile parent");
    std::fs::write(
        &paths.provider_profiles_path,
        serde_json::to_vec_pretty(&ProviderProfilesMetadataFile {
            version: 2,
            active_profile_id: "openrouter-default".into(),
            profiles: vec![ProviderProfileRecord {
                profile_id: "openrouter-default".into(),
                provider_id: "openrouter".into(),
                runtime_kind: "openrouter".into(),
                label: "OpenRouter".into(),
                model_id: "openai/gpt-4.1-mini".into(),
                preset_id: Some("openrouter".into()),
                base_url: None,
                api_version: None,
                credential_link: Some(ProviderProfileCredentialLink::ApiKey {
                    updated_at: "2026-04-21T01:00:00Z".into(),
                }),
                migrated_from_legacy: false,
                migrated_at: None,
                updated_at: "2026-04-21T01:00:00Z".into(),
            }],
            updated_at: "2026-04-21T01:00:00Z".into(),
            migration: Some(ProviderProfilesMigrationState {
                source: "legacy_runtime_settings_v1".into(),
                migrated_at: "2026-04-21T01:00:00Z".into(),
                runtime_settings_updated_at: None,
                openrouter_credentials_updated_at: Some("2026-04-21T01:00:00Z".into()),
                openai_auth_updated_at: None,
                openrouter_model_inferred: Some(false),
            }),
        })
        .expect("serialize provider profiles"),
    )
    .expect("write provider profiles file");

    let response = upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4.1-mini".into(),
            openrouter_api_key: None,
            anthropic_api_key: None,
        },
    )
    .expect("preserve should tolerate missing credential linkage");

    assert_eq!(
        response,
        RuntimeSettingsDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4.1-mini".into(),
            openrouter_api_key_configured: false,
            anthropic_api_key_configured: false,
        }
    );
}
