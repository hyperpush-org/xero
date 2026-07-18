use tauri::Manager;
use xero_desktop_lib::{
    commands::{
        set_agent_default_model, AgentDefaultModelDto, AgentRefDto, ProviderModelThinkingEffortDto,
        RuntimeAgentIdDto, SetAgentDefaultModelRequestDto,
    },
    configure_builder_with_state,
    state::DesktopState,
};

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("build mock app")
}

fn model(provider_id: &str, model_id: &str) -> AgentDefaultModelDto {
    AgentDefaultModelDto {
        provider_id: provider_id.into(),
        provider_profile_id: Some("profile-main".into()),
        model_id: model_id.into(),
        selection_key: Some(format!("{provider_id}:{model_id}")),
        thinking_effort: Some(ProviderModelThinkingEffortDto::High),
    }
}

fn built_in_request(default_model: Option<AgentDefaultModelDto>) -> SetAgentDefaultModelRequestDto {
    SetAgentDefaultModelRequestDto {
        project_id: "project-agent-default-model".into(),
        r#ref: AgentRefDto::BuiltIn {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            version: 1,
        },
        default_model,
    }
}

#[test]
fn built_in_default_model_can_be_created_replaced_reset_and_validated() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(
        DesktopState::default()
            .with_global_db_path_override(root.path().join("app-data").join("xero.db")),
    );

    let first = model("provider-a", "model-a");
    let created = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        built_in_request(Some(first.clone())),
    )
    .expect("create built-in default model");
    assert_eq!(created.default_model, Some(first));

    let replacement = model("provider-b", "model-b");
    let replaced = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        built_in_request(Some(replacement.clone())),
    )
    .expect("replace built-in default model");
    assert_eq!(replaced.default_model, Some(replacement));

    let reset = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        built_in_request(None),
    )
    .expect("reset built-in default model");
    assert_eq!(reset.default_model, None);

    let invalid_models = [
        AgentDefaultModelDto {
            provider_id: " ".into(),
            ..model("provider", "model")
        },
        AgentDefaultModelDto {
            model_id: "".into(),
            ..model("provider", "model")
        },
        AgentDefaultModelDto {
            provider_profile_id: Some("\n".into()),
            ..model("provider", "model")
        },
        AgentDefaultModelDto {
            selection_key: Some(" ".into()),
            ..model("provider", "model")
        },
    ];
    for invalid_model in invalid_models {
        let error = set_agent_default_model(
            app.handle().clone(),
            app.state::<DesktopState>(),
            built_in_request(Some(invalid_model)),
        )
        .expect_err("blank default-model identifiers must fail validation");
        assert_eq!(error.code, "invalid_request");
    }

    let invalid_project = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SetAgentDefaultModelRequestDto {
            project_id: " ".into(),
            ..built_in_request(Some(model("provider", "model")))
        },
    )
    .expect_err("blank project ids must fail validation");
    assert_eq!(invalid_project.code, "invalid_request");

    let missing_custom_project = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SetAgentDefaultModelRequestDto {
            project_id: "missing-project".into(),
            r#ref: AgentRefDto::Custom {
                definition_id: "missing-agent".into(),
                version: 1,
            },
            default_model: Some(model("provider", "model")),
        },
    )
    .expect_err("custom defaults require a registered project");
    assert_ne!(missing_custom_project.code, "invalid_request");
}
