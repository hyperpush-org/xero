use rusqlite::params;
use tauri::Manager;
use xero_desktop_lib::{
    commands::{
        agent_tooling_settings, agent_tooling_update_settings, AgentToolApplicationStyleDto,
        UpsertAgentToolingModelOverrideRequestDto, UpsertAgentToolingSettingsRequestDto,
    },
    configure_builder_with_state, global_db,
    state::DesktopState,
};

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("build mock app")
}

fn override_request(
    provider_id: &str,
    model_id: &str,
    style: Option<AgentToolApplicationStyleDto>,
) -> UpsertAgentToolingModelOverrideRequestDto {
    UpsertAgentToolingModelOverrideRequestDto {
        provider_id: provider_id.into(),
        model_id: model_id.into(),
        style,
    }
}

#[test]
fn tooling_settings_commands_cover_defaults_overrides_removal_and_corrupt_state() {
    let root = tempfile::tempdir().expect("temp dir");
    let database_path = root.path().join("app-data").join("xero.db");
    let app =
        build_mock_app(DesktopState::default().with_global_db_path_override(database_path.clone()));

    let defaults = agent_tooling_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect("load default tooling settings");
    assert_eq!(
        defaults.global_default,
        AgentToolApplicationStyleDto::Balanced
    );
    assert!(defaults.model_overrides.is_empty());
    assert!(defaults.updated_at.is_some());

    let updated = agent_tooling_update_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertAgentToolingSettingsRequestDto {
            global_default: Some(AgentToolApplicationStyleDto::Conservative),
            model_overrides: vec![
                override_request(
                    " provider-z ",
                    " model-z ",
                    Some(AgentToolApplicationStyleDto::DeclarativeFirst),
                ),
                override_request(
                    "provider-a",
                    "model-a",
                    Some(AgentToolApplicationStyleDto::Balanced),
                ),
            ],
        },
    )
    .expect("persist tooling settings");
    assert_eq!(
        updated.global_default,
        AgentToolApplicationStyleDto::Conservative
    );
    assert_eq!(updated.model_overrides.len(), 2);
    assert_eq!(updated.model_overrides[0].provider_id, "provider-a");
    assert_eq!(updated.model_overrides[1].provider_id, "provider-z");
    assert_eq!(updated.model_overrides[1].model_id, "model-z");
    assert_eq!(
        updated.model_overrides[1].style,
        AgentToolApplicationStyleDto::DeclarativeFirst
    );

    let reloaded = agent_tooling_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect("reload persisted tooling settings");
    assert_eq!(reloaded, updated);

    let removed = agent_tooling_update_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertAgentToolingSettingsRequestDto {
            global_default: None,
            model_overrides: vec![override_request("provider-a", "model-a", None)],
        },
    )
    .expect("remove one model override");
    assert_eq!(
        removed.global_default,
        AgentToolApplicationStyleDto::Conservative
    );
    assert_eq!(removed.model_overrides.len(), 1);
    assert_eq!(removed.model_overrides[0].provider_id, "provider-z");

    let duplicate = agent_tooling_update_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertAgentToolingSettingsRequestDto {
            global_default: None,
            model_overrides: vec![
                override_request(
                    "provider-duplicate",
                    "model-duplicate",
                    Some(AgentToolApplicationStyleDto::Balanced),
                ),
                override_request(
                    " provider-duplicate ",
                    " model-duplicate ",
                    Some(AgentToolApplicationStyleDto::Conservative),
                ),
            ],
        },
    )
    .expect_err("normalized duplicate override keys must fail closed");
    assert_eq!(duplicate.code, "agent_tooling_settings_request_invalid");
    let after_duplicate = agent_tooling_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect("duplicate request must not change persisted settings");
    assert_eq!(after_duplicate, removed);

    for invalid in [
        override_request(" ", "model", Some(AgentToolApplicationStyleDto::Balanced)),
        override_request(
            "provider",
            "\n",
            Some(AgentToolApplicationStyleDto::Balanced),
        ),
    ] {
        let error = agent_tooling_update_settings(
            app.handle().clone(),
            app.state::<DesktopState>(),
            UpsertAgentToolingSettingsRequestDto {
                global_default: None,
                model_overrides: vec![invalid],
            },
        )
        .expect_err("blank model override identifiers must fail validation");
        assert_eq!(error.code, "agent_tooling_settings_request_invalid");
    }

    let connection = global_db::open_global_database(&database_path).expect("open global database");
    connection
        .execute(
            "UPDATE agent_tooling_settings SET payload = ?1 WHERE id = 1",
            params![serde_json::json!({
                "schemaVersion": 2,
                "globalDefault": "balanced",
                "modelOverrides": [],
                "updatedAt": "2026-07-17T20:00:00Z"
            })
            .to_string()],
        )
        .expect("seed unsupported settings schema");
    let unsupported = agent_tooling_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("unsupported persisted settings schema must fail closed");
    assert_eq!(unsupported.code, "agent_tooling_settings_decode_failed");

    connection
        .execute(
            "UPDATE agent_tooling_settings SET payload = ?1 WHERE id = 1",
            params![serde_json::json!({
                "schemaVersion": 1,
                "globalDefault": "balanced",
                "modelOverrides": [
                    {
                        "providerId": "provider",
                        "modelId": "model",
                        "style": "balanced",
                        "updatedAt": ""
                    },
                    {
                        "providerId": " provider ",
                        "modelId": " model ",
                        "style": "conservative",
                        "updatedAt": ""
                    }
                ],
                "updatedAt": ""
            })
            .to_string()],
        )
        .expect("seed duplicate persisted override keys");
    let duplicate_state = agent_tooling_settings(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("duplicate persisted override keys must fail closed");
    assert_eq!(duplicate_state.code, "agent_tooling_settings_decode_failed");
}
