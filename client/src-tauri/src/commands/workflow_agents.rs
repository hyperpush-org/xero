use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        available_builtin_runtime_agent_descriptors, runtime_agent_descriptor, validate_non_empty,
        AgentConsumedArtifactDto, AgentDbTouchpointDetailDto, AgentDbTouchpointKindDto,
        AgentDbTouchpointsDto, AgentDefinitionBaseCapabilityProfileDto,
        AgentDefinitionLifecycleStateDto, AgentDefinitionScopeDto, AgentHeaderDto,
        AgentOutputContractDto, AgentOutputSectionDto, AgentPromptDto, AgentPromptRoleDto,
        AgentRefDto, AgentToolEffectClassDto, AgentToolSummaryDto, AgentTriggerRefDto,
        CommandError, CommandResult, GetWorkflowAgentDetailRequestDto,
        ListWorkflowAgentsRequestDto, ListWorkflowAgentsResponseDto,
        RuntimeAgentBaseCapabilityProfileDto, RuntimeAgentDescriptorDto, RuntimeAgentIdDto,
        RuntimeAgentLifecycleStateDto, RuntimeAgentOutputContractDto, RuntimeAgentPromptPolicyDto,
        RuntimeAgentScopeDto, RuntimeRunApprovalModeDto, WorkflowAgentDetailDto,
        WorkflowAgentSummaryDto,
    },
    db::project_store,
    runtime::{
        agent_core::{
            base_policy_fragment, consumed_artifacts_for, db_touchpoints_for_runtime_agent,
            output_sections_for, ConsumedArtifactEntry, DbTouchpointEntry, OutputSectionEntry,
            TriggerRef,
        },
        autonomous_tool_runtime::{
            deferred_tool_catalog, tool_allowed_for_runtime_agent, tool_effect_class,
            AutonomousToolEffectClass,
        },
    },
    state::DesktopState,
};

use super::contracts::workflow_agents::{output_contract_description, output_contract_label};
use super::runtime_support::resolve_project_root;

#[tauri::command]
pub fn list_workflow_agents<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListWorkflowAgentsRequestDto,
) -> CommandResult<ListWorkflowAgentsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;

    let mut agents: Vec<WorkflowAgentSummaryDto> = available_builtin_runtime_agent_descriptors()
        .into_iter()
        .map(builtin_summary)
        .collect();

    let custom_records =
        project_store::list_agent_definitions(&repo_root, request.include_archived)?;
    for record in custom_records {
        if record.scope == "built_in" {
            // Built-ins are sourced from runtime descriptors above; skip any DB shadows
            // so the sidebar shows one row per built-in.
            continue;
        }
        agents.push(custom_summary(record));
    }

    Ok(ListWorkflowAgentsResponseDto { agents })
}

#[tauri::command]
pub fn get_workflow_agent_detail<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetWorkflowAgentDetailRequestDto,
) -> CommandResult<WorkflowAgentDetailDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;

    match request.r#ref {
        AgentRefDto::BuiltIn {
            runtime_agent_id,
            version,
        } => Ok(builtin_detail(runtime_agent_id, version)),
        AgentRefDto::Custom {
            definition_id,
            version,
        } => {
            let definition = project_store::load_agent_definition(&repo_root, &definition_id)?
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "workflow_agent_definition_missing",
                        format!("Xero could not find agent definition `{definition_id}`."),
                    )
                })?;
            let version_record =
                project_store::load_agent_definition_version(&repo_root, &definition_id, version)?
                    .ok_or_else(|| {
                        CommandError::user_fixable(
                            "workflow_agent_definition_version_missing",
                            format!(
                                "Xero could not find version {version} of agent definition `{definition_id}`."
                            ),
                        )
                    })?;
            Ok(custom_detail(definition, version_record))
        }
    }
}

fn builtin_summary(descriptor: RuntimeAgentDescriptorDto) -> WorkflowAgentSummaryDto {
    WorkflowAgentSummaryDto {
        r#ref: AgentRefDto::BuiltIn {
            runtime_agent_id: descriptor.id,
            version: descriptor.version,
        },
        display_name: descriptor.label,
        short_label: descriptor.short_label,
        description: descriptor.description,
        scope: scope_from_runtime(descriptor.scope),
        lifecycle_state: lifecycle_from_runtime(descriptor.lifecycle_state),
        base_capability_profile: base_capability_from_runtime(descriptor.base_capability_profile),
        last_used_at: None,
        use_count: 0,
    }
}

fn custom_summary(record: project_store::AgentDefinitionRecord) -> WorkflowAgentSummaryDto {
    WorkflowAgentSummaryDto {
        r#ref: AgentRefDto::Custom {
            definition_id: record.definition_id.clone(),
            version: record.current_version,
        },
        display_name: record.display_name,
        short_label: record.short_label,
        description: record.description,
        scope: scope_from_str(&record.scope),
        lifecycle_state: lifecycle_from_str(&record.lifecycle_state),
        base_capability_profile: base_capability_from_str(&record.base_capability_profile),
        last_used_at: None,
        use_count: 0,
    }
}

fn builtin_detail(runtime_agent_id: RuntimeAgentIdDto, version: u32) -> WorkflowAgentDetailDto {
    let descriptor = runtime_agent_descriptor(runtime_agent_id);
    let prompts = vec![system_prompt_for_runtime_agent(
        runtime_agent_id,
        descriptor.prompt_policy,
    )];
    let tools = builtin_tools_for_runtime_agent(runtime_agent_id);
    let touchpoints = db_touchpoints_dto(runtime_agent_id);
    let output = output_contract_dto(descriptor.output_contract);
    let consumes = consumed_artifacts_dto(runtime_agent_id);

    WorkflowAgentDetailDto {
        r#ref: AgentRefDto::BuiltIn {
            runtime_agent_id,
            version,
        },
        header: AgentHeaderDto {
            display_name: descriptor.label.clone(),
            short_label: descriptor.short_label.clone(),
            description: descriptor.description.clone(),
            task_purpose: descriptor.task_purpose.clone(),
            scope: scope_from_runtime(descriptor.scope),
            lifecycle_state: lifecycle_from_runtime(descriptor.lifecycle_state),
            base_capability_profile: base_capability_from_runtime(
                descriptor.base_capability_profile,
            ),
            default_approval_mode: descriptor.default_approval_mode.clone(),
            allowed_approval_modes: descriptor.allowed_approval_modes.clone(),
            allow_plan_gate: descriptor.allow_plan_gate,
            allow_verification_gate: descriptor.allow_verification_gate,
            allow_auto_compact: descriptor.allow_auto_compact,
        },
        prompt_policy: Some(descriptor.prompt_policy),
        tool_policy: Some(descriptor.tool_policy),
        prompts,
        tools,
        db_touchpoints: touchpoints,
        output,
        consumes,
    }
}

fn custom_detail(
    record: project_store::AgentDefinitionRecord,
    version: project_store::AgentDefinitionVersionRecord,
) -> WorkflowAgentDetailDto {
    let runtime_agent_id = project_store::runtime_agent_id_for_base_capability_profile(
        &record.base_capability_profile,
    );
    let runtime_descriptor = runtime_agent_descriptor(runtime_agent_id);
    let snapshot = &version.snapshot;

    let prompts = custom_prompts_from_snapshot(snapshot, runtime_agent_id);
    let tools = builtin_tools_for_runtime_agent(runtime_agent_id);
    let touchpoints = db_touchpoints_dto(runtime_agent_id);
    let output = output_contract_dto(runtime_descriptor.output_contract);
    let consumes = consumed_artifacts_dto(runtime_agent_id);

    let task_purpose = snapshot
        .get("taskPurpose")
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| record.description.clone());

    let default_approval_mode = snapshot
        .get("defaultApprovalMode")
        .and_then(JsonValue::as_str)
        .and_then(parse_approval_mode_label)
        .unwrap_or_else(|| runtime_descriptor.default_approval_mode.clone());

    let allowed_approval_modes = snapshot
        .get("allowedApprovalModes")
        .and_then(JsonValue::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .filter_map(parse_approval_mode_label)
                .collect::<Vec<_>>()
        })
        .filter(|modes| !modes.is_empty())
        .unwrap_or_else(|| runtime_descriptor.allowed_approval_modes.clone());

    WorkflowAgentDetailDto {
        r#ref: AgentRefDto::Custom {
            definition_id: record.definition_id.clone(),
            version: version.version,
        },
        header: AgentHeaderDto {
            display_name: record.display_name.clone(),
            short_label: record.short_label.clone(),
            description: record.description.clone(),
            task_purpose,
            scope: scope_from_str(&record.scope),
            lifecycle_state: lifecycle_from_str(&record.lifecycle_state),
            base_capability_profile: base_capability_from_str(&record.base_capability_profile),
            default_approval_mode,
            allowed_approval_modes,
            allow_plan_gate: runtime_descriptor.allow_plan_gate,
            allow_verification_gate: runtime_descriptor.allow_verification_gate,
            allow_auto_compact: runtime_descriptor.allow_auto_compact,
        },
        prompt_policy: Some(runtime_descriptor.prompt_policy),
        tool_policy: Some(runtime_descriptor.tool_policy),
        prompts,
        tools,
        db_touchpoints: touchpoints,
        output,
        consumes,
    }
}

fn system_prompt_for_runtime_agent(
    runtime_agent_id: RuntimeAgentIdDto,
    policy: RuntimeAgentPromptPolicyDto,
) -> AgentPromptDto {
    let body = base_policy_fragment(runtime_agent_id);
    AgentPromptDto {
        id: format!("xero.system_policy.{}", runtime_agent_id.as_str()),
        label: "System policy".to_string(),
        role: AgentPromptRoleDto::System,
        policy: Some(policy),
        source: "xero-runtime".to_string(),
        body,
    }
}

fn custom_prompts_from_snapshot(
    snapshot: &JsonValue,
    runtime_agent_id: RuntimeAgentIdDto,
) -> Vec<AgentPromptDto> {
    let runtime_descriptor = runtime_agent_descriptor(runtime_agent_id);
    let mut prompts = vec![system_prompt_for_runtime_agent(
        runtime_agent_id,
        runtime_descriptor.prompt_policy,
    )];

    if let Some(fragments) = snapshot
        .get("promptFragments")
        .and_then(JsonValue::as_array)
    {
        for (index, fragment) in fragments.iter().enumerate() {
            let id = fragment
                .get("id")
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("custom_prompt.{index}"));
            let label = fragment
                .get("title")
                .or_else(|| fragment.get("label"))
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| id.clone());
            let body = fragment
                .get("body")
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
                .unwrap_or_default();
            if body.trim().is_empty() {
                continue;
            }
            prompts.push(AgentPromptDto {
                id,
                label,
                role: AgentPromptRoleDto::Developer,
                policy: None,
                source: "agent_definition.snapshot.promptFragments".to_string(),
                body,
            });
        }
    }

    if let Some(workflow) = snapshot
        .get("workflowContract")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        prompts.push(AgentPromptDto {
            id: "agent_definition.workflowContract".to_string(),
            label: "Workflow contract".to_string(),
            role: AgentPromptRoleDto::Developer,
            policy: None,
            source: "agent_definition.snapshot.workflowContract".to_string(),
            body: workflow.to_string(),
        });
    }

    if let Some(final_response) = snapshot
        .get("finalResponseContract")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        prompts.push(AgentPromptDto {
            id: "agent_definition.finalResponseContract".to_string(),
            label: "Final response contract".to_string(),
            role: AgentPromptRoleDto::Developer,
            policy: None,
            source: "agent_definition.snapshot.finalResponseContract".to_string(),
            body: final_response.to_string(),
        });
    }

    prompts
}

fn builtin_tools_for_runtime_agent(
    runtime_agent_id: RuntimeAgentIdDto,
) -> Vec<AgentToolSummaryDto> {
    let catalog = deferred_tool_catalog(true);
    catalog
        .into_iter()
        .filter(|entry| tool_allowed_for_runtime_agent(runtime_agent_id, entry.tool_name))
        .map(|entry| AgentToolSummaryDto {
            name: entry.tool_name.to_string(),
            group: entry.group.to_string(),
            description: entry.description.to_string(),
            effect_class: effect_class_from_runtime(tool_effect_class(entry.tool_name)),
            risk_class: entry.risk_class.to_string(),
            tags: entry.tags.iter().map(|s| s.to_string()).collect(),
            schema_fields: entry.schema_fields.iter().map(|s| s.to_string()).collect(),
            examples: entry.examples.iter().map(|s| s.to_string()).collect(),
        })
        .collect()
}

fn db_touchpoints_dto(runtime_agent_id: RuntimeAgentIdDto) -> AgentDbTouchpointsDto {
    let touchpoints = db_touchpoints_for_runtime_agent(runtime_agent_id);
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    let mut encouraged = Vec::new();
    for entry in touchpoints.entries {
        let detail = touchpoint_entry_to_dto(entry);
        match entry.kind {
            AgentDbTouchpointKindDto::Read => reads.push(detail),
            AgentDbTouchpointKindDto::Write => writes.push(detail),
            AgentDbTouchpointKindDto::Encouraged => encouraged.push(detail),
        }
    }
    AgentDbTouchpointsDto {
        reads,
        writes,
        encouraged,
    }
}

fn touchpoint_entry_to_dto(entry: &DbTouchpointEntry) -> AgentDbTouchpointDetailDto {
    AgentDbTouchpointDetailDto {
        table: entry.table.to_string(),
        kind: entry.kind,
        purpose: entry.purpose.to_string(),
        triggers: entry.triggers.iter().map(trigger_ref_to_dto).collect(),
        columns: entry.columns.iter().map(|s| s.to_string()).collect(),
    }
}

fn trigger_ref_to_dto(trigger: &TriggerRef) -> AgentTriggerRefDto {
    match trigger {
        TriggerRef::Tool(name) => AgentTriggerRefDto::Tool {
            name: (*name).to_string(),
        },
        TriggerRef::OutputSection(id) => AgentTriggerRefDto::OutputSection {
            id: (*id).to_string(),
        },
        TriggerRef::Lifecycle(event) => AgentTriggerRefDto::Lifecycle { event: *event },
        TriggerRef::UpstreamArtifact(id) => AgentTriggerRefDto::UpstreamArtifact {
            id: (*id).to_string(),
        },
    }
}

fn output_contract_dto(contract: RuntimeAgentOutputContractDto) -> AgentOutputContractDto {
    AgentOutputContractDto {
        contract,
        label: output_contract_label(contract).to_string(),
        description: output_contract_description(contract).to_string(),
        sections: output_sections_for(contract)
            .iter()
            .map(output_section_entry_to_dto)
            .collect(),
    }
}

fn output_section_entry_to_dto(entry: &OutputSectionEntry) -> AgentOutputSectionDto {
    AgentOutputSectionDto {
        id: entry.id.to_string(),
        label: entry.label.to_string(),
        description: entry.description.to_string(),
        emphasis: entry.emphasis,
        produced_by_tools: entry
            .produced_by_tools
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

fn consumed_artifacts_dto(runtime_agent_id: RuntimeAgentIdDto) -> Vec<AgentConsumedArtifactDto> {
    consumed_artifacts_for(runtime_agent_id)
        .iter()
        .map(consumed_artifact_entry_to_dto)
        .collect()
}

fn consumed_artifact_entry_to_dto(entry: &ConsumedArtifactEntry) -> AgentConsumedArtifactDto {
    AgentConsumedArtifactDto {
        id: entry.id.to_string(),
        label: entry.label.to_string(),
        description: entry.description.to_string(),
        source_agent: entry.source_agent,
        contract: entry.contract,
        sections: entry.sections.iter().map(|s| s.to_string()).collect(),
        required: entry.required,
    }
}

fn scope_from_runtime(scope: RuntimeAgentScopeDto) -> AgentDefinitionScopeDto {
    match scope {
        RuntimeAgentScopeDto::BuiltIn => AgentDefinitionScopeDto::BuiltIn,
        RuntimeAgentScopeDto::GlobalCustom => AgentDefinitionScopeDto::GlobalCustom,
        RuntimeAgentScopeDto::ProjectCustom => AgentDefinitionScopeDto::ProjectCustom,
    }
}

fn lifecycle_from_runtime(
    state: RuntimeAgentLifecycleStateDto,
) -> AgentDefinitionLifecycleStateDto {
    match state {
        RuntimeAgentLifecycleStateDto::Draft => AgentDefinitionLifecycleStateDto::Draft,
        RuntimeAgentLifecycleStateDto::Active => AgentDefinitionLifecycleStateDto::Active,
        RuntimeAgentLifecycleStateDto::Archived => AgentDefinitionLifecycleStateDto::Archived,
    }
}

fn base_capability_from_runtime(
    profile: RuntimeAgentBaseCapabilityProfileDto,
) -> AgentDefinitionBaseCapabilityProfileDto {
    match profile {
        RuntimeAgentBaseCapabilityProfileDto::ObserveOnly => {
            AgentDefinitionBaseCapabilityProfileDto::ObserveOnly
        }
        RuntimeAgentBaseCapabilityProfileDto::Planning => {
            AgentDefinitionBaseCapabilityProfileDto::Planning
        }
        RuntimeAgentBaseCapabilityProfileDto::RepositoryRecon => {
            AgentDefinitionBaseCapabilityProfileDto::RepositoryRecon
        }
        RuntimeAgentBaseCapabilityProfileDto::Engineering => {
            AgentDefinitionBaseCapabilityProfileDto::Engineering
        }
        RuntimeAgentBaseCapabilityProfileDto::Debugging => {
            AgentDefinitionBaseCapabilityProfileDto::Debugging
        }
        RuntimeAgentBaseCapabilityProfileDto::AgentBuilder => {
            AgentDefinitionBaseCapabilityProfileDto::AgentBuilder
        }
        RuntimeAgentBaseCapabilityProfileDto::HarnessTest => {
            AgentDefinitionBaseCapabilityProfileDto::HarnessTest
        }
    }
}

fn scope_from_str(value: &str) -> AgentDefinitionScopeDto {
    match value {
        "global_custom" => AgentDefinitionScopeDto::GlobalCustom,
        "project_custom" => AgentDefinitionScopeDto::ProjectCustom,
        _ => AgentDefinitionScopeDto::BuiltIn,
    }
}

fn lifecycle_from_str(value: &str) -> AgentDefinitionLifecycleStateDto {
    match value {
        "draft" => AgentDefinitionLifecycleStateDto::Draft,
        "archived" => AgentDefinitionLifecycleStateDto::Archived,
        _ => AgentDefinitionLifecycleStateDto::Active,
    }
}

fn base_capability_from_str(value: &str) -> AgentDefinitionBaseCapabilityProfileDto {
    match value {
        "planning" => AgentDefinitionBaseCapabilityProfileDto::Planning,
        "repository_recon" => AgentDefinitionBaseCapabilityProfileDto::RepositoryRecon,
        "engineering" => AgentDefinitionBaseCapabilityProfileDto::Engineering,
        "debugging" => AgentDefinitionBaseCapabilityProfileDto::Debugging,
        "agent_builder" => AgentDefinitionBaseCapabilityProfileDto::AgentBuilder,
        "harness_test" => AgentDefinitionBaseCapabilityProfileDto::HarnessTest,
        _ => AgentDefinitionBaseCapabilityProfileDto::ObserveOnly,
    }
}

fn parse_approval_mode_label(value: &str) -> Option<RuntimeRunApprovalModeDto> {
    match value {
        "suggest" => Some(RuntimeRunApprovalModeDto::Suggest),
        "auto_edit" => Some(RuntimeRunApprovalModeDto::AutoEdit),
        "yolo" => Some(RuntimeRunApprovalModeDto::Yolo),
        _ => None,
    }
}

fn effect_class_from_runtime(class: AutonomousToolEffectClass) -> AgentToolEffectClassDto {
    match class {
        AutonomousToolEffectClass::Observe => AgentToolEffectClassDto::Observe,
        AutonomousToolEffectClass::RuntimeState => AgentToolEffectClassDto::RuntimeState,
        AutonomousToolEffectClass::Write => AgentToolEffectClassDto::Write,
        AutonomousToolEffectClass::DestructiveWrite => AgentToolEffectClassDto::DestructiveWrite,
        AutonomousToolEffectClass::Command => AgentToolEffectClassDto::Command,
        AutonomousToolEffectClass::ProcessControl => AgentToolEffectClassDto::ProcessControl,
        AutonomousToolEffectClass::BrowserControl => AgentToolEffectClassDto::BrowserControl,
        AutonomousToolEffectClass::DeviceControl => AgentToolEffectClassDto::DeviceControl,
        AutonomousToolEffectClass::ExternalService => AgentToolEffectClassDto::ExternalService,
        AutonomousToolEffectClass::SkillRuntime => AgentToolEffectClassDto::SkillRuntime,
        AutonomousToolEffectClass::AgentDelegation => AgentToolEffectClassDto::AgentDelegation,
        AutonomousToolEffectClass::Unknown => AgentToolEffectClassDto::Unknown,
    }
}
