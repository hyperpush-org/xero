use serde::de::DeserializeOwned;
use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        available_builtin_runtime_agent_descriptors, runtime_agent_descriptor, validate_non_empty,
        AgentAuthoringAvailabilityStatusDto, AgentAuthoringCatalogDiagnosticDto,
        AgentAuthoringCatalogDto, AgentAuthoringConstraintExplanationDto,
        AgentAuthoringCreationFlowDto, AgentAuthoringCreationFlowEntryKindDto,
        AgentAuthoringDbTableDto, AgentAuthoringPolicyControlDto,
        AgentAuthoringPolicyControlKindDto, AgentAuthoringPolicyControlValueKindDto,
        AgentAuthoringProfileAvailabilityDto, AgentAuthoringTemplateDto,
        AgentAuthoringToolCategoryDto, AgentAuthoringUpstreamArtifactDto, AgentConsumedArtifactDto,
        AgentDbTouchpointDetailDto, AgentDbTouchpointKindDto, AgentDbTouchpointsDto,
        AgentDefinitionBaseCapabilityProfileDto, AgentDefinitionLifecycleStateDto,
        AgentDefinitionScopeDto, AgentHeaderDto, AgentOutputContractDto, AgentOutputSectionDto,
        AgentPromptDto, AgentPromptRoleDto, AgentRefDto, AgentToolEffectClassDto,
        AgentToolPackCatalogDto, AgentToolPolicyDetailsDto, AgentToolSummaryDto,
        AgentTriggerRefDto, CommandError, CommandResult, GetAgentAuthoringCatalogRequestDto,
        GetAgentToolPackCatalogRequestDto, GetWorkflowAgentDetailRequestDto,
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
            deferred_tool_catalog, tool_access_group_descriptors, tool_allowed_for_runtime_agent,
            tool_effect_class, AutonomousToolEffectClass, AutonomousToolRuntime,
        },
    },
    state::DesktopState,
};
use xero_agent_core::domain_tool_pack_manifests;

use super::contracts::workflow_agents::{output_contract_description, output_contract_label};
use super::runtime_support::resolve_project_root;

const AGENT_AUTHORING_CATALOG_CONTRACT_VERSION: u32 = 1;

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

#[tauri::command]
pub fn get_agent_authoring_catalog<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentAuthoringCatalogRequestDto,
) -> CommandResult<AgentAuthoringCatalogDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let _repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;

    Ok(agent_authoring_catalog())
}

fn agent_authoring_catalog() -> AgentAuthoringCatalogDto {
    // Tools: full deferred catalog, exposed unfiltered. The picker will note
    // each tool's effect class so the canvas can warn when a chosen tool
    // exceeds the agent's base capability profile.
    let tools: Vec<AgentToolSummaryDto> = deferred_tool_catalog(true)
        .into_iter()
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
        .collect();

    // Tool categories: each access-group becomes a chunk the user can drag in
    // wholesale. We resolve each tool name in the group to the full summary
    // (with effect class, risk, etc.) so the canvas doesn't have to rejoin.
    let tools_by_name: std::collections::HashMap<String, &AgentToolSummaryDto> =
        tools.iter().map(|tool| (tool.name.clone(), tool)).collect();
    let tool_categories: Vec<AgentAuthoringToolCategoryDto> = tool_access_group_descriptors()
        .into_iter()
        .map(|group| {
            let category_tools: Vec<AgentToolSummaryDto> = group
                .tools
                .iter()
                .filter_map(|name| tools_by_name.get(name).map(|tool| (*tool).clone()))
                .collect();
            AgentAuthoringToolCategoryDto {
                id: group.name.clone(),
                label: humanize_tool_group(&group.name),
                description: group.description.clone(),
                tools: category_tools,
            }
        })
        // Skip categories where no tools resolved (e.g. internal-only groups).
        .filter(|category| !category.tools.is_empty())
        .collect();

    // DB tables: union of every built-in agent's static touchpoints. The same
    // table can appear under multiple agents (one as read, another as write);
    // we collapse those here on `table` and keep the longest purpose so the
    // picker shows useful descriptive text.
    let mut db_table_map: std::collections::BTreeMap<String, AgentAuthoringDbTableDto> =
        std::collections::BTreeMap::new();
    for descriptor in available_builtin_runtime_agent_descriptors() {
        let touchpoints = db_touchpoints_for_runtime_agent(descriptor.id);
        for entry in touchpoints.entries {
            let table = entry.table.to_string();
            let existing =
                db_table_map
                    .entry(table.clone())
                    .or_insert_with(|| AgentAuthoringDbTableDto {
                        table: table.clone(),
                        purpose: entry.purpose.to_string(),
                        columns: entry.columns.iter().map(|s| s.to_string()).collect(),
                    });
            if entry.purpose.len() > existing.purpose.len() {
                existing.purpose = entry.purpose.to_string();
            }
            // Merge column lists, dedup, preserve discovery order.
            for column in entry.columns.iter() {
                let column = column.to_string();
                if !existing.columns.iter().any(|c| c == &column) {
                    existing.columns.push(column);
                }
            }
        }
    }
    let db_tables: Vec<AgentAuthoringDbTableDto> = db_table_map.into_values().collect();

    // Upstream artifacts: each available built-in agent publishes one output
    // contract; downstream agents consume it. We surface (sourceAgent, contract,
    // sections) so the picker can offer "from <agent>" selections and pre-fill
    // the chosen contract's sections.
    let upstream_artifacts: Vec<AgentAuthoringUpstreamArtifactDto> =
        available_builtin_runtime_agent_descriptors()
            .into_iter()
            .map(|descriptor| {
                let contract = descriptor.output_contract;
                let sections: Vec<AgentOutputSectionDto> = output_sections_for(contract)
                    .iter()
                    .map(output_section_entry_to_dto)
                    .collect();
                AgentAuthoringUpstreamArtifactDto {
                    source_agent: descriptor.id,
                    source_agent_label: descriptor.label.clone(),
                    contract,
                    contract_label: output_contract_label(contract).to_string(),
                    label: format!("{} output", descriptor.label),
                    description: output_contract_description(contract).to_string(),
                    sections,
                }
            })
            .collect();

    let profile_availability =
        authoring_profile_availability(&tools, &db_tables, &upstream_artifacts);
    let policy_controls = authoring_policy_controls();
    let templates = authoring_templates();
    let creation_flows = authoring_creation_flows();
    let constraint_explanations =
        authoring_constraint_explanations(profile_availability.as_slice());

    let mut catalog = AgentAuthoringCatalogDto {
        contract_version: AGENT_AUTHORING_CATALOG_CONTRACT_VERSION,
        tools,
        tool_categories,
        db_tables,
        upstream_artifacts,
        policy_controls,
        templates,
        creation_flows,
        profile_availability,
        constraint_explanations,
        diagnostics: Vec::new(),
    };
    catalog.diagnostics = validate_agent_authoring_catalog(&catalog);
    catalog
}

fn validate_agent_authoring_catalog(
    catalog: &AgentAuthoringCatalogDto,
) -> Vec<AgentAuthoringCatalogDiagnosticDto> {
    let mut diagnostics = Vec::new();

    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_tool_name",
        "Authoring catalog tool names must be unique.",
        &["tools"],
        catalog
            .tools
            .iter()
            .enumerate()
            .map(|(index, tool)| (index, tool.name.clone()))
            .collect(),
    );
    let tools_by_name = catalog
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<std::collections::HashSet<_>>();

    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_tool_category_id",
        "Authoring catalog tool category ids must be unique.",
        &["toolCategories"],
        catalog
            .tool_categories
            .iter()
            .enumerate()
            .map(|(index, category)| (index, category.id.clone()))
            .collect(),
    );
    for (category_index, category) in catalog.tool_categories.iter().enumerate() {
        push_duplicate_key_diagnostics(
            &mut diagnostics,
            "authoring_catalog_duplicate_category_tool_name",
            "Authoring catalog category tool names must be unique.",
            &["toolCategories", &category_index.to_string(), "tools"],
            category
                .tools
                .iter()
                .enumerate()
                .map(|(tool_index, tool)| (tool_index, tool.name.clone()))
                .collect(),
        );
        for (tool_index, tool) in category.tools.iter().enumerate() {
            if !tools_by_name.contains(tool.name.as_str()) {
                diagnostics.push(authoring_catalog_diagnostic(
                    "authoring_catalog_unknown_category_tool",
                    "Authoring catalog category tools must reference catalog tools.",
                    &[
                        "toolCategories",
                        &category_index.to_string(),
                        "tools",
                        &tool_index.to_string(),
                        "name",
                    ],
                ));
            }
        }
    }

    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_database_table",
        "Authoring catalog database tables must be unique.",
        &["dbTables"],
        catalog
            .db_tables
            .iter()
            .enumerate()
            .map(|(index, table)| (index, table.table.clone()))
            .collect(),
    );

    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_upstream_artifact",
        "Authoring catalog upstream artifacts must be unique per source and contract.",
        &["upstreamArtifacts"],
        catalog
            .upstream_artifacts
            .iter()
            .enumerate()
            .map(|(index, artifact)| {
                (
                    index,
                    format!(
                        "{}:{}",
                        artifact.source_agent.as_str(),
                        output_contract_id(artifact.contract)
                    ),
                )
            })
            .collect(),
    );

    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_policy_control_id",
        "Authoring catalog policy control ids must be unique.",
        &["policyControls"],
        catalog
            .policy_controls
            .iter()
            .enumerate()
            .map(|(index, control)| (index, control.id.clone()))
            .collect(),
    );
    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_policy_control_snapshot_path",
        "Authoring catalog policy control snapshot paths must be unique.",
        &["policyControls"],
        catalog
            .policy_controls
            .iter()
            .enumerate()
            .map(|(index, control)| (index, control.snapshot_path.clone()))
            .collect(),
    );

    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_template_id",
        "Authoring template ids must be unique.",
        &["templates"],
        catalog
            .templates
            .iter()
            .enumerate()
            .map(|(index, template)| (index, template.id.clone()))
            .collect(),
    );
    let templates_by_id = catalog
        .templates
        .iter()
        .map(|template| (template.id.as_str(), template))
        .collect::<std::collections::HashMap<_, _>>();

    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_creation_flow_id",
        "Authoring creation flow ids must be unique.",
        &["creationFlows"],
        catalog
            .creation_flows
            .iter()
            .enumerate()
            .map(|(index, flow)| (index, flow.id.clone()))
            .collect(),
    );
    for (flow_index, flow) in catalog.creation_flows.iter().enumerate() {
        if flow.template_ids.is_empty() {
            diagnostics.push(authoring_catalog_diagnostic(
                "authoring_catalog_empty_creation_flow_templates",
                "Authoring creation flows must reference at least one template.",
                &["creationFlows", &flow_index.to_string(), "templateIds"],
            ));
            continue;
        }

        let mut has_known_template = false;
        let mut has_compatible_template = false;
        for (template_index, template_id) in flow.template_ids.iter().enumerate() {
            let Some(template) = templates_by_id.get(template_id.as_str()) else {
                diagnostics.push(authoring_catalog_diagnostic(
                    "authoring_catalog_unknown_creation_flow_template",
                    "Authoring creation flow references an unknown template id.",
                    &[
                        "creationFlows",
                        &flow_index.to_string(),
                        "templateIds",
                        &template_index.to_string(),
                    ],
                ));
                continue;
            };
            has_known_template = true;
            if template.task_kind == flow.task_kind
                && template.base_capability_profile == flow.base_capability_profile
                && template_output_contract_id(template)
                    == Some(output_contract_id(flow.expected_output_contract))
            {
                has_compatible_template = true;
            }
        }
        if has_known_template && !has_compatible_template {
            diagnostics.push(authoring_catalog_diagnostic(
                "authoring_catalog_incompatible_creation_flow_template",
                "Authoring creation flow must reference a template matching its task kind, base capability profile, and expected output contract.",
                &["creationFlows", &flow_index.to_string(), "templateIds"],
            ));
        }
    }

    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_profile_availability",
        "Authoring profile availability entries must be unique per subject and profile.",
        &["profileAvailability"],
        catalog
            .profile_availability
            .iter()
            .enumerate()
            .map(|(index, availability)| (index, authoring_profile_availability_key(availability)))
            .collect(),
    );
    let availability_by_key = catalog
        .profile_availability
        .iter()
        .map(|availability| {
            (
                authoring_profile_availability_key(availability),
                availability,
            )
        })
        .collect::<std::collections::HashMap<_, _>>();

    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_constraint_explanation_id",
        "Authoring constraint explanation ids must be unique.",
        &["constraintExplanations"],
        catalog
            .constraint_explanations
            .iter()
            .enumerate()
            .map(|(index, explanation)| (index, explanation.id.clone()))
            .collect(),
    );
    push_duplicate_key_diagnostics(
        &mut diagnostics,
        "authoring_catalog_duplicate_constraint_explanation_subject",
        "Authoring constraint explanations must be unique per subject and profile.",
        &["constraintExplanations"],
        catalog
            .constraint_explanations
            .iter()
            .enumerate()
            .map(|(index, explanation)| (index, authoring_constraint_explanation_key(explanation)))
            .collect(),
    );
    for (index, explanation) in catalog.constraint_explanations.iter().enumerate() {
        let key = authoring_constraint_explanation_key(explanation);
        let Some(availability) = availability_by_key.get(&key) else {
            diagnostics.push(authoring_catalog_diagnostic(
                "authoring_catalog_orphan_constraint_explanation",
                "Authoring constraint explanation must reference profile availability.",
                &["constraintExplanations", &index.to_string()],
            ));
            continue;
        };
        if availability.status != explanation.status {
            diagnostics.push(authoring_catalog_diagnostic(
                "authoring_catalog_constraint_status_mismatch",
                "Authoring constraint explanation status must match profile availability.",
                &["constraintExplanations", &index.to_string(), "status"],
            ));
        }
        if availability.required_profile != explanation.required_profile {
            diagnostics.push(authoring_catalog_diagnostic(
                "authoring_catalog_constraint_required_profile_mismatch",
                "Authoring constraint explanation required profile must match profile availability.",
                &["constraintExplanations", &index.to_string(), "requiredProfile"],
            ));
        }
    }

    diagnostics
}

fn push_duplicate_key_diagnostics(
    diagnostics: &mut Vec<AgentAuthoringCatalogDiagnosticDto>,
    code: &str,
    message: &str,
    path: &[&str],
    values: Vec<(usize, String)>,
) {
    let mut seen = std::collections::HashSet::new();
    for (index, value) in values {
        if !seen.insert(value) {
            let mut duplicate_path = path
                .iter()
                .map(|segment| (*segment).to_string())
                .collect::<Vec<_>>();
            duplicate_path.push(index.to_string());
            diagnostics.push(AgentAuthoringCatalogDiagnosticDto {
                severity: "error".into(),
                code: code.into(),
                message: message.into(),
                path: duplicate_path,
            });
        }
    }
}

fn authoring_catalog_diagnostic(
    code: &str,
    message: &str,
    path: &[&str],
) -> AgentAuthoringCatalogDiagnosticDto {
    AgentAuthoringCatalogDiagnosticDto {
        severity: "error".into(),
        code: code.into(),
        message: message.into(),
        path: path.iter().map(|segment| (*segment).to_string()).collect(),
    }
}

fn template_output_contract_id(template: &AgentAuthoringTemplateDto) -> Option<&str> {
    template
        .definition
        .get("output")
        .and_then(|output| output.get("contract"))
        .and_then(JsonValue::as_str)
}

fn authoring_profile_availability_key(
    availability: &AgentAuthoringProfileAvailabilityDto,
) -> String {
    format!(
        "{}:{}:{}",
        availability.subject_kind,
        availability.subject_id,
        base_capability_profile_id(&availability.base_capability_profile)
    )
}

fn authoring_constraint_explanation_key(
    explanation: &AgentAuthoringConstraintExplanationDto,
) -> String {
    format!(
        "{}:{}:{}",
        explanation.subject_kind,
        explanation.subject_id,
        base_capability_profile_id(&explanation.base_capability_profile)
    )
}

#[tauri::command]
pub fn get_agent_tool_pack_catalog<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentToolPackCatalogRequestDto,
) -> CommandResult<AgentToolPackCatalogDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let runtime = AutonomousToolRuntime::for_project(&app, state.inner(), &request.project_id)?;

    Ok(agent_tool_pack_catalog(request.project_id, &runtime))
}

fn agent_tool_pack_catalog(
    project_id: String,
    runtime: &AutonomousToolRuntime,
) -> AgentToolPackCatalogDto {
    let health_reports = runtime.tool_pack_health_reports();
    let available_pack_ids = health_reports
        .iter()
        .filter(|report| report.enabled_by_policy)
        .map(|report| report.pack_id.clone())
        .collect();

    AgentToolPackCatalogDto {
        schema: "xero.agent_tool_pack_catalog.v1".into(),
        project_id,
        tool_packs: domain_tool_pack_manifests(),
        available_pack_ids,
        health_reports,
        ui_deferred: true,
    }
}

fn humanize_tool_group(group: &str) -> String {
    // "harness_runner" → "Harness Runner". Falls back to the raw value when
    // there's nothing to title-case.
    group
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn authoring_policy_controls() -> Vec<AgentAuthoringPolicyControlDto> {
    vec![
        policy_control(
            "context.recordKinds",
            AgentAuthoringPolicyControlKindDto::Context,
            "Context Record Kinds",
            "Project record kinds that this custom agent may use for durable context.",
            "projectDataPolicy.recordKinds",
            AgentAuthoringPolicyControlValueKindDto::StringArray,
            json!([
                "project_fact",
                "decision",
                "constraint",
                "plan",
                "question",
                "context_note",
                "diagnostic"
            ]),
            "Limits durable project records that can be considered relevant for this agent.",
            false,
        ),
        policy_control(
            "context.structuredSchemas",
            AgentAuthoringPolicyControlKindDto::Context,
            "Structured Context Schemas",
            "Structured project-record schemas that this custom agent expects.",
            "projectDataPolicy.structuredSchemas",
            AgentAuthoringPolicyControlValueKindDto::StringArray,
            json!(["xero.project_record.v1"]),
            "Guides schema-cited durable context and consumed-artifact matching.",
            false,
        ),
        policy_control(
            "memory.memoryKinds",
            AgentAuthoringPolicyControlKindDto::Memory,
            "Memory Kinds",
            "Approved memory kinds this custom agent may retrieve or propose.",
            "memoryCandidatePolicy.memoryKinds",
            AgentAuthoringPolicyControlValueKindDto::StringArray,
            json!([
                "project_fact",
                "user_preference",
                "decision",
                "session_summary",
                "troubleshooting"
            ]),
            "Constrains memory candidates and approved-memory retrieval for this agent.",
            false,
        ),
        policy_control(
            "memory.reviewRequired",
            AgentAuthoringPolicyControlKindDto::Memory,
            "Memory Review Required",
            "Whether new memory candidates require review before becoming retrievable.",
            "memoryCandidatePolicy.reviewRequired",
            AgentAuthoringPolicyControlValueKindDto::Boolean,
            json!(true),
            "Keeps memory writes in review until explicitly approved.",
            true,
        ),
        policy_control(
            "retrieval.enabled",
            AgentAuthoringPolicyControlKindDto::Retrieval,
            "Retrieval Enabled",
            "Whether first-turn working-set retrieval is enabled for this custom agent.",
            "retrievalDefaults.enabled",
            AgentAuthoringPolicyControlValueKindDto::Boolean,
            json!(true),
            "Controls whether relevant durable context can seed the first provider turn.",
            false,
        ),
        policy_control(
            "retrieval.limit",
            AgentAuthoringPolicyControlKindDto::Retrieval,
            "Retrieval Limit",
            "Maximum durable context records considered for first-turn working-set retrieval.",
            "retrievalDefaults.limit",
            AgentAuthoringPolicyControlValueKindDto::PositiveInteger,
            json!(6),
            "Bounds retrieval fan-in for initial context packages and manifests.",
            false,
        ),
        policy_control(
            "retrieval.recordKinds",
            AgentAuthoringPolicyControlKindDto::Retrieval,
            "Retrieval Record Kinds",
            "Project record kinds eligible for automatic retrieval.",
            "retrievalDefaults.recordKinds",
            AgentAuthoringPolicyControlValueKindDto::StringArray,
            json!([
                "project_fact",
                "decision",
                "constraint",
                "plan",
                "finding",
                "question",
                "context_note",
                "diagnostic"
            ]),
            "Filters project records before first-turn working-set summary construction.",
            false,
        ),
        policy_control(
            "retrieval.memoryKinds",
            AgentAuthoringPolicyControlKindDto::Retrieval,
            "Retrieval Memory Kinds",
            "Approved memory kinds eligible for retrieval.",
            "retrievalDefaults.memoryKinds",
            AgentAuthoringPolicyControlValueKindDto::StringArray,
            json!([
                "project_fact",
                "user_preference",
                "decision",
                "session_summary",
                "troubleshooting"
            ]),
            "Filters approved memory before first-turn working-set summary construction.",
            false,
        ),
        policy_control(
            "handoff.enabled",
            AgentAuthoringPolicyControlKindDto::Handoff,
            "Handoff Enabled",
            "Whether this custom agent can preserve handoff context during context exhaustion.",
            "handoffPolicy.enabled",
            AgentAuthoringPolicyControlValueKindDto::Boolean,
            json!(true),
            "Allows context exhaustion policy to prepare handoff bundles for continuation.",
            false,
        ),
        policy_control(
            "handoff.preserveDefinitionVersion",
            AgentAuthoringPolicyControlKindDto::Handoff,
            "Preserve Definition Version",
            "Whether handoff targets should keep the source run's pinned custom-agent version.",
            "handoffPolicy.preserveDefinitionVersion",
            AgentAuthoringPolicyControlValueKindDto::Boolean,
            json!(true),
            "Prevents handoff drift when the agent definition changes mid-run.",
            false,
        ),
    ]
}

fn policy_control(
    id: &str,
    kind: AgentAuthoringPolicyControlKindDto,
    label: &str,
    description: &str,
    snapshot_path: &str,
    value_kind: AgentAuthoringPolicyControlValueKindDto,
    default_value: JsonValue,
    runtime_effect: &str,
    review_required: bool,
) -> AgentAuthoringPolicyControlDto {
    AgentAuthoringPolicyControlDto {
        id: id.to_string(),
        kind,
        label: label.to_string(),
        description: description.to_string(),
        snapshot_path: snapshot_path.to_string(),
        value_kind,
        default_value,
        runtime_effect: runtime_effect.to_string(),
        review_required,
    }
}

fn authoring_templates() -> Vec<AgentAuthoringTemplateDto> {
    vec![
        authoring_template(
            "engineering_patch",
            "Engineering Patch",
            "Inspect, edit, verify, and summarize a bounded implementation task.",
            "engineering",
            AgentDefinitionBaseCapabilityProfileDto::Engineering,
            RuntimeAgentOutputContractDto::EngineeringSummary,
            &[
                "read",
                "search",
                "git_status",
                "git_diff",
                "write",
                "patch",
                "command_probe",
                "command_verify",
            ],
            &[
                "Fix the failing parser case and run the focused parser test.",
                "Implement the next accepted plan slice with scoped verification.",
                "Refactor the small helper and summarize changed files.",
            ],
        ),
        authoring_template(
            "debug_root_cause",
            "Debug Root Cause",
            "Reproduce, diagnose, fix, and verify a reported defect with evidence.",
            "debugging",
            AgentDefinitionBaseCapabilityProfileDto::Debugging,
            RuntimeAgentOutputContractDto::DebugSummary,
            &[
                "read",
                "search",
                "git_status",
                "git_diff",
                "command_probe",
                "command_verify",
                "command_session",
            ],
            &[
                "Find the root cause of this failing login flow.",
                "Reproduce the intermittent test failure and propose the smallest fix.",
                "Diagnose why the local command hangs and record evidence.",
            ],
        ),
        authoring_template(
            "planning_pack",
            "Planning Pack",
            "Turn ambiguous work into an accepted plan without mutating repository files.",
            "planning",
            AgentDefinitionBaseCapabilityProfileDto::Planning,
            RuntimeAgentOutputContractDto::PlanPack,
            &[
                "read",
                "search",
                "git_status",
                "git_diff",
                "project_context_search",
                "project_context_get",
                "project_context_record",
                "todo",
            ],
            &[
                "Make a build plan for the next feature without editing files.",
                "Break this migration into safe implementation slices.",
                "Clarify open questions and produce an Engineer handoff.",
            ],
        ),
        authoring_template(
            "repository_recon",
            "Repository Recon",
            "Map a repository's stack, commands, architecture, and unknowns without mutation.",
            "repository_recon",
            AgentDefinitionBaseCapabilityProfileDto::RepositoryRecon,
            RuntimeAgentOutputContractDto::CrawlReport,
            &[
                "read",
                "search",
                "find",
                "git_status",
                "git_diff",
                "command_probe",
                "workspace_index",
            ],
            &[
                "Map this repository and identify useful scoped commands.",
                "Find the main app boundaries and test strategy.",
                "Summarize architectural hotspots with source paths.",
            ],
        ),
        authoring_template(
            "support_triage",
            "Support Triage",
            "Answer support questions from reviewed project context without changing files.",
            "support_triage",
            AgentDefinitionBaseCapabilityProfileDto::ObserveOnly,
            RuntimeAgentOutputContractDto::Answer,
            &[
                "read",
                "search",
                "project_context_search",
                "project_context_get",
                "tool_search",
            ],
            &[
                "Explain whether this reported behavior is a known issue.",
                "Summarize the user-visible workaround from approved context.",
                "List what evidence is still missing before escalation.",
            ],
        ),
        authoring_template(
            "agent_builder",
            "Agent Builder",
            "Draft and validate custom agent definitions through the registry path.",
            "agent_builder",
            AgentDefinitionBaseCapabilityProfileDto::AgentBuilder,
            RuntimeAgentOutputContractDto::AgentDefinitionDraft,
            &[
                "agent_definition",
                "project_context_search",
                "project_context_get",
                "tool_search",
            ],
            &[
                "Draft a narrow release-notes helper agent.",
                "Validate this custom agent definition and explain denied tools.",
                "Clone an existing agent and narrow its retrieval policy.",
            ],
        ),
    ]
}

fn authoring_template(
    id: &str,
    label: &str,
    description: &str,
    task_kind: &str,
    base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
    output_contract: RuntimeAgentOutputContractDto,
    tools: &[&str],
    examples: &[&str],
) -> AgentAuthoringTemplateDto {
    AgentAuthoringTemplateDto {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        task_kind: task_kind.to_string(),
        base_capability_profile,
        definition: template_definition(
            id,
            label,
            description,
            base_capability_profile,
            output_contract,
            tools,
            examples,
        ),
        examples: examples
            .iter()
            .map(|example| (*example).to_string())
            .collect(),
    }
}

fn template_definition(
    id: &str,
    label: &str,
    description: &str,
    base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
    output_contract: RuntimeAgentOutputContractDto,
    tools: &[&str],
    examples: &[&str],
) -> JsonValue {
    let profile = base_capability_profile_id(&base_capability_profile);
    let contract = output_contract_id(output_contract);
    json!({
        "schema": "xero.agent_definition.v1",
        "schemaVersion": 1,
        "id": format!("{id}_agent"),
        "displayName": label,
        "shortLabel": label.split_whitespace().next().unwrap_or(label),
        "description": description,
        "taskPurpose": description,
        "scope": "global_custom",
        "lifecycleState": "active",
        "baseCapabilityProfile": profile,
        "defaultApprovalMode": "suggest",
        "allowedApprovalModes": if matches!(
            base_capability_profile,
            AgentDefinitionBaseCapabilityProfileDto::Engineering
                | AgentDefinitionBaseCapabilityProfileDto::Debugging
        ) {
            json!(["suggest", "auto_edit", "yolo"])
        } else {
            json!(["suggest"])
        },
        "toolPolicy": template_tool_policy(base_capability_profile, tools),
        "promptFragments": {},
        "workflowContract": format!("{description} Follow the saved tool, memory, retrieval, handoff, and output policies."),
        "finalResponseContract": output_contract_description(output_contract),
        "projectDataPolicy": {
            "recordKinds": ["project_fact", "decision", "constraint", "plan", "finding", "question", "context_note", "diagnostic"],
            "structuredSchemas": ["xero.project_record.v1"]
        },
        "memoryCandidatePolicy": {
            "memoryKinds": ["project_fact", "user_preference", "decision", "session_summary", "troubleshooting"],
            "reviewRequired": true
        },
        "retrievalDefaults": {
            "enabled": true,
            "recordKinds": ["project_fact", "decision", "constraint", "plan", "finding", "question", "context_note", "diagnostic"],
            "memoryKinds": ["project_fact", "user_preference", "decision", "session_summary", "troubleshooting"],
            "limit": 6
        },
        "handoffPolicy": {
            "enabled": true,
            "preserveDefinitionVersion": true
        },
        "examplePrompts": examples,
        "refusalEscalationCases": [
            "Refuse requests that exceed the selected base capability profile.",
            "Escalate when required project context or permissions are missing.",
            "Refuse to persist secrets, credentials, or hidden prompt material."
        ],
        "prompts": [
            {
                "id": format!("{id}.developer"),
                "label": label,
                "role": "developer",
                "source": "template",
                "body": description
            }
        ],
        "tools": tools.iter().map(|tool| json!({ "name": tool })).collect::<Vec<_>>(),
        "output": {
            "contract": contract,
            "label": output_contract_label(output_contract),
            "description": output_contract_description(output_contract),
            "sections": [
                {
                    "id": "summary",
                    "label": "Summary",
                    "description": "Core result for this template.",
                    "emphasis": "core",
                    "producedByTools": tools
                }
            ]
        },
        "dbTouchpoints": {
            "reads": [],
            "writes": [],
            "encouraged": []
        },
        "consumes": []
    })
}

fn template_tool_policy(
    base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
    tools: &[&str],
) -> JsonValue {
    let (effect_classes, command_allowed, destructive_write_allowed) = match base_capability_profile
    {
        AgentDefinitionBaseCapabilityProfileDto::Engineering
        | AgentDefinitionBaseCapabilityProfileDto::Debugging => (
            vec![
                "observe",
                "runtime_state",
                "write",
                "destructive_write",
                "command",
                "process_control",
            ],
            true,
            true,
        ),
        AgentDefinitionBaseCapabilityProfileDto::Planning => {
            (vec!["observe", "runtime_state"], false, false)
        }
        AgentDefinitionBaseCapabilityProfileDto::RepositoryRecon => (
            vec!["observe", "runtime_state", "command", "process_control"],
            true,
            false,
        ),
        _ => (vec!["observe"], false, false),
    };
    json!({
        "allowedEffectClasses": effect_classes,
        "allowedToolGroups": [],
        "allowedToolPacks": [],
        "allowedTools": tools,
        "deniedTools": [],
        "deniedToolPacks": [],
        "externalServiceAllowed": false,
        "browserControlAllowed": false,
        "skillRuntimeAllowed": false,
        "subagentAllowed": false,
        "commandAllowed": command_allowed,
        "destructiveWriteAllowed": destructive_write_allowed
    })
}

fn authoring_creation_flows() -> Vec<AgentAuthoringCreationFlowDto> {
    vec![
        creation_flow(
            "start_from_engineering_task",
            "Start From Engineering Task",
            "Create a custom implementation agent from a bounded engineering task.",
            AgentAuthoringCreationFlowEntryKindDto::Template,
            "engineering",
            &["engineering_patch"],
            "Describe the implementation task, expected verification, and any files or constraints the agent should respect.",
            RuntimeAgentOutputContractDto::EngineeringSummary,
            AgentDefinitionBaseCapabilityProfileDto::Engineering,
        ),
        creation_flow(
            "start_from_debugging_task",
            "Start From Debugging Task",
            "Create a custom debugging agent around reproducible evidence and root-cause reporting.",
            AgentAuthoringCreationFlowEntryKindDto::Template,
            "debugging",
            &["debug_root_cause"],
            "Describe the symptom, expected behavior, reproduction hints, and evidence the agent should preserve.",
            RuntimeAgentOutputContractDto::DebugSummary,
            AgentDefinitionBaseCapabilityProfileDto::Debugging,
        ),
        creation_flow(
            "describe_planning_intent",
            "Describe Planning Intent",
            "Turn natural-language planning needs into an ordinary custom planning definition.",
            AgentAuthoringCreationFlowEntryKindDto::DescribeIntent,
            "planning",
            &["planning_pack"],
            "Describe the planning outcome, non-goals, acceptance criteria, and handoff expectations.",
            RuntimeAgentOutputContractDto::PlanPack,
            AgentDefinitionBaseCapabilityProfileDto::Planning,
        ),
        creation_flow(
            "compose_recon_and_support",
            "Compose Recon And Support",
            "Compose read-only repository reconnaissance with support-triage answer behavior.",
            AgentAuthoringCreationFlowEntryKindDto::ComposeTemplates,
            "support_triage",
            &["repository_recon", "support_triage"],
            "Describe what support questions the agent should answer and which repository facts it must cite.",
            RuntimeAgentOutputContractDto::Answer,
            AgentDefinitionBaseCapabilityProfileDto::ObserveOnly,
        ),
        creation_flow(
            "build_agent_builder_helper",
            "Build Agent-Builder Helper",
            "Create an agent-builder helper that drafts or narrows custom definitions through the registry.",
            AgentAuthoringCreationFlowEntryKindDto::Template,
            "agent_builder",
            &["agent_builder"],
            "Describe the kinds of custom agents this helper should draft, validate, or narrow.",
            RuntimeAgentOutputContractDto::AgentDefinitionDraft,
            AgentDefinitionBaseCapabilityProfileDto::AgentBuilder,
        ),
    ]
}

fn creation_flow(
    id: &str,
    label: &str,
    description: &str,
    entry_kind: AgentAuthoringCreationFlowEntryKindDto,
    task_kind: &str,
    template_ids: &[&str],
    intent_prompt: &str,
    expected_output_contract: RuntimeAgentOutputContractDto,
    base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
) -> AgentAuthoringCreationFlowDto {
    AgentAuthoringCreationFlowDto {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        entry_kind,
        task_kind: task_kind.to_string(),
        template_ids: template_ids
            .iter()
            .map(|template_id| (*template_id).to_string())
            .collect(),
        intent_prompt: intent_prompt.to_string(),
        expected_output_contract,
        base_capability_profile,
    }
}

fn authoring_profile_availability(
    tools: &[AgentToolSummaryDto],
    db_tables: &[AgentAuthoringDbTableDto],
    upstream_artifacts: &[AgentAuthoringUpstreamArtifactDto],
) -> Vec<AgentAuthoringProfileAvailabilityDto> {
    let profiles = authoring_profile_runtimes();
    let mut availability = Vec::new();

    for tool in tools {
        let allowed_profiles = profiles
            .iter()
            .filter(|(_, runtime_agent_id)| {
                tool_allowed_for_runtime_agent(*runtime_agent_id, &tool.name)
            })
            .map(|(profile, _)| profile.clone())
            .collect::<Vec<_>>();
        availability.extend(availability_for_subject(
            "tool",
            &tool.name,
            &profiles,
            &allowed_profiles,
        ));
    }

    for db_table in db_tables {
        let allowed_profiles = profiles
            .iter()
            .filter(|(_, runtime_agent_id)| {
                db_touchpoints_for_runtime_agent(*runtime_agent_id)
                    .entries
                    .iter()
                    .any(|entry| entry.table == db_table.table)
            })
            .map(|(profile, _)| profile.clone())
            .collect::<Vec<_>>();
        availability.extend(availability_for_subject(
            "db_touchpoint",
            &db_table.table,
            &profiles,
            &allowed_profiles,
        ));
    }

    for artifact in upstream_artifacts {
        let subject_id = format!(
            "{}:{}",
            artifact.source_agent.as_str(),
            output_contract_id(artifact.contract)
        );
        let allowed_profiles = profiles
            .iter()
            .filter(|(_, runtime_agent_id)| {
                consumed_artifacts_for(*runtime_agent_id)
                    .iter()
                    .any(|entry| {
                        entry.source_agent == artifact.source_agent
                            && entry.contract == artifact.contract
                    })
            })
            .map(|(profile, _)| profile.clone())
            .collect::<Vec<_>>();
        availability.extend(availability_for_subject(
            "upstream_artifact",
            &subject_id,
            &profiles,
            &allowed_profiles,
        ));
    }

    for contract in unique_output_contracts() {
        let allowed_profiles = profiles
            .iter()
            .filter(|(_, runtime_agent_id)| {
                runtime_agent_descriptor(*runtime_agent_id).output_contract == contract
            })
            .map(|(profile, _)| profile.clone())
            .collect::<Vec<_>>();
        availability.extend(availability_for_subject(
            "output_contract",
            output_contract_id(contract),
            &profiles,
            &allowed_profiles,
        ));
    }

    let mut effect_classes = tools
        .iter()
        .map(|tool| tool.effect_class)
        .collect::<Vec<_>>();
    effect_classes.sort_by_key(|effect| effect_class_id(*effect));
    effect_classes.dedup();
    for effect_class in effect_classes {
        let allowed_profiles = profiles
            .iter()
            .filter(|(_, runtime_agent_id)| {
                tools.iter().any(|tool| {
                    tool.effect_class == effect_class
                        && tool_allowed_for_runtime_agent(*runtime_agent_id, &tool.name)
                })
            })
            .map(|(profile, _)| profile.clone())
            .collect::<Vec<_>>();
        availability.extend(availability_for_subject(
            "capability_control",
            effect_class_id(effect_class),
            &profiles,
            &allowed_profiles,
        ));
    }

    availability
}

fn authoring_profile_runtimes() -> Vec<(AgentDefinitionBaseCapabilityProfileDto, RuntimeAgentIdDto)>
{
    let mut profiles = Vec::new();
    for descriptor in available_builtin_runtime_agent_descriptors() {
        let profile = base_capability_from_runtime(descriptor.base_capability_profile);
        if !profiles.iter().any(
            |(existing, _): &(AgentDefinitionBaseCapabilityProfileDto, RuntimeAgentIdDto)| {
                existing == &profile
            },
        ) {
            profiles.push((profile, descriptor.id));
        }
    }
    profiles
}

fn availability_for_subject(
    subject_kind: &str,
    subject_id: &str,
    profiles: &[(AgentDefinitionBaseCapabilityProfileDto, RuntimeAgentIdDto)],
    allowed_profiles: &[AgentDefinitionBaseCapabilityProfileDto],
) -> Vec<AgentAuthoringProfileAvailabilityDto> {
    profiles
        .iter()
        .map(|(profile, _)| {
            let (status, required_profile) =
                if allowed_profiles.iter().any(|allowed| allowed == profile) {
                    (AgentAuthoringAvailabilityStatusDto::Available, None)
                } else if let Some(required) = allowed_profiles.first() {
                    (
                        AgentAuthoringAvailabilityStatusDto::RequiresProfileChange,
                        Some(required.clone()),
                    )
                } else {
                    (AgentAuthoringAvailabilityStatusDto::Unavailable, None)
                };
            AgentAuthoringProfileAvailabilityDto {
                subject_kind: subject_kind.to_string(),
                subject_id: subject_id.to_string(),
                base_capability_profile: profile.clone(),
                status,
                reason: availability_reason(subject_kind, status, required_profile.as_ref()),
                required_profile,
            }
        })
        .collect()
}

fn availability_reason(
    subject_kind: &str,
    status: AgentAuthoringAvailabilityStatusDto,
    required_profile: Option<&AgentDefinitionBaseCapabilityProfileDto>,
) -> String {
    match (status, required_profile) {
        (AgentAuthoringAvailabilityStatusDto::Available, _) => {
            format!("{subject_kind} is available for this base capability profile.")
        }
        (AgentAuthoringAvailabilityStatusDto::RequiresProfileChange, Some(profile)) => {
            format!(
                "{subject_kind} requires the `{}` base capability profile.",
                base_capability_profile_id(profile)
            )
        }
        (AgentAuthoringAvailabilityStatusDto::RequiresProfileChange, None) => {
            format!("{subject_kind} requires a different base capability profile.")
        }
        (AgentAuthoringAvailabilityStatusDto::Unavailable, _) => {
            format!("{subject_kind} is not exposed by any current runtime profile.")
        }
    }
}

fn authoring_constraint_explanations(
    availability: &[AgentAuthoringProfileAvailabilityDto],
) -> Vec<AgentAuthoringConstraintExplanationDto> {
    availability
        .iter()
        .filter(|entry| entry.status != AgentAuthoringAvailabilityStatusDto::Available)
        .map(|entry| {
            let profile = base_capability_profile_id(&entry.base_capability_profile);
            let subject_label = authoring_constraint_subject_label(
                entry.subject_kind.as_str(),
                entry.subject_id.as_str(),
            );
            let required_profile = entry
                .required_profile
                .as_ref()
                .map(base_capability_profile_id);
            let (message, resolution) = match (entry.status, required_profile) {
                (AgentAuthoringAvailabilityStatusDto::RequiresProfileChange, Some(required)) => (
                    format!(
                        "{subject_label} is not available on `{profile}` because that profile cannot safely run the required capability."
                    ),
                    format!(
                        "Switch the agent base capability profile to `{required}` or remove `{}` before saving.",
                        entry.subject_id
                    ),
                ),
                (AgentAuthoringAvailabilityStatusDto::RequiresProfileChange, None) => (
                    format!(
                        "{subject_label} is not available on `{profile}` because it needs a broader base capability profile."
                    ),
                    format!(
                        "Choose a compatible base capability profile or remove `{}` before saving.",
                        entry.subject_id
                    ),
                ),
                (AgentAuthoringAvailabilityStatusDto::Unavailable, _) => (
                    format!(
                        "{subject_label} is not available on `{profile}` because no current runtime profile exposes it."
                    ),
                    format!(
                        "Remove `{}` or install/enable a runtime capability that explicitly exposes it.",
                        entry.subject_id
                    ),
                ),
                (AgentAuthoringAvailabilityStatusDto::Available, _) => {
                    unreachable!("available entries are filtered out")
                }
            };
            AgentAuthoringConstraintExplanationDto {
                id: format!(
                    "{}:{}:{}",
                    entry.subject_kind,
                    entry.subject_id,
                    base_capability_profile_id(&entry.base_capability_profile)
                ),
                subject_kind: entry.subject_kind.clone(),
                subject_id: entry.subject_id.clone(),
                base_capability_profile: entry.base_capability_profile,
                status: entry.status,
                message,
                resolution,
                required_profile: entry.required_profile,
                source: "profileAvailability".into(),
            }
        })
        .collect()
}

fn authoring_constraint_subject_label(subject_kind: &str, subject_id: &str) -> String {
    match subject_kind {
        "tool" => format!("Tool `{subject_id}`"),
        "db_touchpoint" => format!("Database touchpoint `{subject_id}`"),
        "upstream_artifact" => format!("Upstream artifact `{subject_id}`"),
        "output_contract" => format!("Output contract `{subject_id}`"),
        "capability_control" => format!("Capability `{subject_id}`"),
        _ => format!("{subject_kind} `{subject_id}`"),
    }
}

fn unique_output_contracts() -> Vec<RuntimeAgentOutputContractDto> {
    let mut contracts = available_builtin_runtime_agent_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.output_contract)
        .collect::<Vec<_>>();
    contracts.sort_by_key(|contract| output_contract_id(*contract));
    contracts.dedup();
    contracts
}

fn output_contract_id(contract: RuntimeAgentOutputContractDto) -> &'static str {
    match contract {
        RuntimeAgentOutputContractDto::Answer => "answer",
        RuntimeAgentOutputContractDto::PlanPack => "plan_pack",
        RuntimeAgentOutputContractDto::CrawlReport => "crawl_report",
        RuntimeAgentOutputContractDto::EngineeringSummary => "engineering_summary",
        RuntimeAgentOutputContractDto::DebugSummary => "debug_summary",
        RuntimeAgentOutputContractDto::AgentDefinitionDraft => "agent_definition_draft",
        RuntimeAgentOutputContractDto::HarnessTestReport => "harness_test_report",
    }
}

fn effect_class_id(effect_class: AgentToolEffectClassDto) -> &'static str {
    match effect_class {
        AgentToolEffectClassDto::Observe => "observe",
        AgentToolEffectClassDto::RuntimeState => "runtime_state",
        AgentToolEffectClassDto::Write => "write",
        AgentToolEffectClassDto::DestructiveWrite => "destructive_write",
        AgentToolEffectClassDto::Command => "command",
        AgentToolEffectClassDto::ProcessControl => "process_control",
        AgentToolEffectClassDto::BrowserControl => "browser_control",
        AgentToolEffectClassDto::DeviceControl => "device_control",
        AgentToolEffectClassDto::ExternalService => "external_service",
        AgentToolEffectClassDto::SkillRuntime => "skill_runtime",
        AgentToolEffectClassDto::AgentDelegation => "agent_delegation",
        AgentToolEffectClassDto::Unknown => "unknown",
    }
}

fn base_capability_profile_id(profile: &AgentDefinitionBaseCapabilityProfileDto) -> &'static str {
    match profile {
        AgentDefinitionBaseCapabilityProfileDto::ObserveOnly => "observe_only",
        AgentDefinitionBaseCapabilityProfileDto::Planning => "planning",
        AgentDefinitionBaseCapabilityProfileDto::RepositoryRecon => "repository_recon",
        AgentDefinitionBaseCapabilityProfileDto::Engineering => "engineering",
        AgentDefinitionBaseCapabilityProfileDto::Debugging => "debugging",
        AgentDefinitionBaseCapabilityProfileDto::AgentBuilder => "agent_builder",
        AgentDefinitionBaseCapabilityProfileDto::HarnessTest => "harness_test",
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
        tool_policy_details: None,
        prompts,
        tools,
        db_touchpoints: touchpoints,
        output,
        consumes,
        authoring_graph: None,
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

    let tool_policy_details = tool_policy_details_from_snapshot(snapshot);
    let prompts = snapshot_vec::<AgentPromptDto>(snapshot, "prompts")
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| custom_prompts_from_snapshot(snapshot, runtime_agent_id));
    let tools = snapshot_vec::<AgentToolSummaryDto>(snapshot, "tools")
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| {
            custom_tools_from_policy_or_runtime(runtime_agent_id, tool_policy_details.as_ref())
        });
    let touchpoints = snapshot_dto::<AgentDbTouchpointsDto>(snapshot, "dbTouchpoints")
        .unwrap_or_else(|| db_touchpoints_dto(runtime_agent_id));
    let output = snapshot_dto::<AgentOutputContractDto>(snapshot, "output")
        .unwrap_or_else(|| output_contract_dto(runtime_descriptor.output_contract));
    let consumes = snapshot_vec::<AgentConsumedArtifactDto>(snapshot, "consumes")
        .unwrap_or_else(|| consumed_artifacts_dto(runtime_agent_id));

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
        tool_policy_details,
        prompts,
        tools,
        db_touchpoints: touchpoints,
        output,
        consumes,
        authoring_graph: Some(authoring_graph_from_snapshot(&record, &version)),
    }
}

fn authoring_graph_from_snapshot(
    record: &project_store::AgentDefinitionRecord,
    version: &project_store::AgentDefinitionVersionRecord,
) -> JsonValue {
    let snapshot = &version.snapshot;
    json!({
        "schema": "xero.agent_authoring_graph.v1",
        "source": {
            "kind": "agent_definition_version",
            "definitionId": record.definition_id,
            "version": version.version,
            "scope": record.scope,
            "lifecycleState": record.lifecycle_state,
            "baseCapabilityProfile": record.base_capability_profile,
            "createdAt": version.created_at,
            "generatedBy": infer_authoring_graph_source(snapshot),
            "uiDeferred": true
        },
        "editableFields": [
            "prompts",
            "tools",
            "toolPolicy",
            "output",
            "dbTouchpoints",
            "consumes",
            "workflowStructure",
            "projectDataPolicy",
            "memoryCandidatePolicy",
            "retrievalDefaults",
            "handoffPolicy"
        ],
        "canonicalGraph": {
            "schema": snapshot.get("schema").cloned().unwrap_or(JsonValue::Null),
            "schemaVersion": snapshot.get("schemaVersion").cloned().unwrap_or(JsonValue::Null),
            "id": snapshot.get("id").cloned().unwrap_or(JsonValue::Null),
            "version": snapshot.get("version").cloned().unwrap_or(JsonValue::Null),
            "displayName": snapshot.get("displayName").cloned().unwrap_or(JsonValue::Null),
            "shortLabel": snapshot.get("shortLabel").cloned().unwrap_or(JsonValue::Null),
            "description": snapshot.get("description").cloned().unwrap_or(JsonValue::Null),
            "taskPurpose": snapshot.get("taskPurpose").cloned().unwrap_or(JsonValue::Null),
            "scope": snapshot.get("scope").cloned().unwrap_or(JsonValue::Null),
            "lifecycleState": snapshot.get("lifecycleState").cloned().unwrap_or(JsonValue::Null),
            "baseCapabilityProfile": snapshot.get("baseCapabilityProfile").cloned().unwrap_or(JsonValue::Null),
            "defaultApprovalMode": snapshot.get("defaultApprovalMode").cloned().unwrap_or(JsonValue::Null),
            "allowedApprovalModes": snapshot.get("allowedApprovalModes").cloned().unwrap_or(JsonValue::Array(Vec::new())),
            "promptFragments": snapshot.get("promptFragments").cloned().unwrap_or_else(|| json!({})),
            "prompts": snapshot.get("prompts").cloned().unwrap_or(JsonValue::Array(Vec::new())),
            "toolPolicy": snapshot.get("toolPolicy").cloned().unwrap_or(JsonValue::Null),
            "tools": snapshot.get("tools").cloned().unwrap_or(JsonValue::Array(Vec::new())),
            "output": snapshot.get("output").cloned().unwrap_or(JsonValue::Null),
            "dbTouchpoints": snapshot.get("dbTouchpoints").cloned().unwrap_or(JsonValue::Null),
            "consumes": snapshot.get("consumes").cloned().unwrap_or(JsonValue::Array(Vec::new())),
            "workflowContract": snapshot.get("workflowContract").cloned().unwrap_or(JsonValue::Null),
            "workflowStructure": snapshot.get("workflowStructure").cloned().unwrap_or(JsonValue::Null),
            "finalResponseContract": snapshot.get("finalResponseContract").cloned().unwrap_or(JsonValue::Null),
            "projectDataPolicy": snapshot.get("projectDataPolicy").cloned().unwrap_or(JsonValue::Null),
            "memoryCandidatePolicy": snapshot.get("memoryCandidatePolicy").cloned().unwrap_or(JsonValue::Null),
            "retrievalDefaults": snapshot.get("retrievalDefaults").cloned().unwrap_or(JsonValue::Null),
            "handoffPolicy": snapshot.get("handoffPolicy").cloned().unwrap_or(JsonValue::Null),
            "examplePrompts": snapshot.get("examplePrompts").cloned().unwrap_or(JsonValue::Array(Vec::new())),
            "refusalEscalationCases": snapshot.get("refusalEscalationCases").cloned().unwrap_or(JsonValue::Array(Vec::new()))
        }
    })
}

fn infer_authoring_graph_source(snapshot: &JsonValue) -> &'static str {
    let prompt_sources = snapshot
        .get("prompts")
        .and_then(JsonValue::as_array)
        .map(|prompts| {
            prompts
                .iter()
                .filter_map(|prompt| prompt.get("source").and_then(JsonValue::as_str))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if prompt_sources
        .iter()
        .any(|source| *source == "agent_builder")
    {
        return "agent_builder";
    }
    if prompt_sources.iter().any(|source| *source == "template") {
        return "template";
    }
    "saved_definition"
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

fn snapshot_dto<T>(snapshot: &JsonValue, field: &'static str) -> Option<T>
where
    T: DeserializeOwned,
{
    snapshot
        .get(field)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

fn snapshot_vec<T>(snapshot: &JsonValue, field: &'static str) -> Option<Vec<T>>
where
    T: DeserializeOwned,
{
    snapshot_dto(snapshot, field)
}

fn custom_tools_from_policy_or_runtime(
    runtime_agent_id: RuntimeAgentIdDto,
    policy: Option<&AgentToolPolicyDetailsDto>,
) -> Vec<AgentToolSummaryDto> {
    let catalog = deferred_tool_catalog(true);
    let Some(policy) = policy else {
        return builtin_tools_for_runtime_agent(runtime_agent_id);
    };
    if policy.allowed_tools.is_empty() {
        return builtin_tools_for_runtime_agent(runtime_agent_id);
    }
    catalog
        .into_iter()
        .filter(|entry| {
            policy
                .allowed_tools
                .iter()
                .any(|tool_name| tool_name == entry.tool_name)
        })
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

fn tool_policy_details_from_snapshot(snapshot: &JsonValue) -> Option<AgentToolPolicyDetailsDto> {
    let object = snapshot.get("toolPolicy")?.as_object()?;
    Some(AgentToolPolicyDetailsDto {
        allowed_tools: string_array_from_object(object, "allowedTools"),
        denied_tools: string_array_from_object(object, "deniedTools"),
        allowed_tool_packs: string_array_from_object(object, "allowedToolPacks"),
        denied_tool_packs: string_array_from_object(object, "deniedToolPacks"),
        allowed_tool_groups: string_array_from_object(object, "allowedToolGroups"),
        denied_tool_groups: string_array_from_object(object, "deniedToolGroups"),
        allowed_effect_classes: string_array_from_object(object, "allowedEffectClasses")
            .into_iter()
            .filter_map(|value| effect_class_from_str(&value))
            .collect(),
        external_service_allowed: bool_from_object(object, "externalServiceAllowed"),
        browser_control_allowed: bool_from_object(object, "browserControlAllowed"),
        skill_runtime_allowed: bool_from_object(object, "skillRuntimeAllowed"),
        subagent_allowed: bool_from_object(object, "subagentAllowed"),
        allowed_subagent_roles: string_array_from_object(object, "allowedSubagentRoles"),
        denied_subagent_roles: string_array_from_object(object, "deniedSubagentRoles"),
        command_allowed: bool_from_object(object, "commandAllowed"),
        destructive_write_allowed: bool_from_object(object, "destructiveWriteAllowed"),
    })
}

fn string_array_from_object(
    object: &serde_json::Map<String, JsonValue>,
    key: &'static str,
) -> Vec<String> {
    object
        .get(key)
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn bool_from_object(object: &serde_json::Map<String, JsonValue>, key: &'static str) -> bool {
    object
        .get(key)
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
}

fn effect_class_from_str(value: &str) -> Option<AgentToolEffectClassDto> {
    match value {
        "observe" => Some(AgentToolEffectClassDto::Observe),
        "runtime_state" => Some(AgentToolEffectClassDto::RuntimeState),
        "write" => Some(AgentToolEffectClassDto::Write),
        "destructive_write" => Some(AgentToolEffectClassDto::DestructiveWrite),
        "command" => Some(AgentToolEffectClassDto::Command),
        "process_control" => Some(AgentToolEffectClassDto::ProcessControl),
        "browser_control" => Some(AgentToolEffectClassDto::BrowserControl),
        "device_control" => Some(AgentToolEffectClassDto::DeviceControl),
        "external_service" => Some(AgentToolEffectClassDto::ExternalService),
        "skill_runtime" => Some(AgentToolEffectClassDto::SkillRuntime),
        "agent_delegation" => Some(AgentToolEffectClassDto::AgentDelegation),
        "unknown" => Some(AgentToolEffectClassDto::Unknown),
        _ => None,
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
        RuntimeAgentLifecycleStateDto::Valid => AgentDefinitionLifecycleStateDto::Valid,
        RuntimeAgentLifecycleStateDto::Active => AgentDefinitionLifecycleStateDto::Active,
        RuntimeAgentLifecycleStateDto::Archived => AgentDefinitionLifecycleStateDto::Archived,
        RuntimeAgentLifecycleStateDto::Blocked => AgentDefinitionLifecycleStateDto::Blocked,
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
        "valid" => AgentDefinitionLifecycleStateDto::Valid,
        "archived" => AgentDefinitionLifecycleStateDto::Archived,
        "blocked" => AgentDefinitionLifecycleStateDto::Blocked,
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

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn custom_detail_hydrates_saved_graph_before_runtime_defaults() {
        let record = project_store::AgentDefinitionRecord {
            definition_id: "release_notes_helper".into(),
            current_version: 2,
            display_name: "Release Notes Helper".into(),
            short_label: "Release".into(),
            description: "Draft release notes from reviewed context.".into(),
            scope: "project_custom".into(),
            lifecycle_state: "active".into(),
            base_capability_profile: "engineering".into(),
            created_at: "2026-05-01T12:00:00Z".into(),
            updated_at: "2026-05-01T12:03:00Z".into(),
        };
        let version = project_store::AgentDefinitionVersionRecord {
            definition_id: "release_notes_helper".into(),
            version: 2,
            created_at: "2026-05-01T12:03:00Z".into(),
            validation_report: None,
            snapshot: json!({
                "id": "release_notes_helper",
                "displayName": "Release Notes Helper",
                "shortLabel": "Release",
                "description": "Draft release notes from reviewed context.",
                "taskPurpose": "Retrieve only approved release context.",
                "scope": "project_custom",
                "lifecycleState": "active",
                "baseCapabilityProfile": "engineering",
                "defaultApprovalMode": "suggest",
                "allowedApprovalModes": ["suggest"],
                "toolPolicy": {
                    "allowedTools": ["read"],
                    "deniedTools": ["write"],
                    "allowedToolPacks": ["release_notes_pack"],
                    "deniedToolPacks": ["external_network"],
                    "allowedToolGroups": ["core"],
                    "deniedToolGroups": ["browser_control"],
                    "allowedEffectClasses": ["observe"],
                    "browserControlAllowed": false,
                    "externalServiceAllowed": false,
                    "skillRuntimeAllowed": false,
                    "subagentAllowed": false,
                    "commandAllowed": false,
                    "destructiveWriteAllowed": false
                },
                "workflowContract": "Use approved context only.",
                "finalResponseContract": "Return release notes sections.",
                "prompts": [
                    {
                        "id": "release_prompt",
                        "label": "Release prompt",
                        "role": "system",
                        "source": "custom",
                        "body": "Draft release notes."
                    }
                ],
                "tools": [
                    {
                        "name": "read",
                        "group": "core",
                        "description": "Read file content.",
                        "effectClass": "observe",
                        "riskClass": "observe",
                        "tags": ["file"],
                        "schemaFields": ["path"],
                        "examples": ["Read CHANGELOG.md"]
                    }
                ],
                "output": {
                    "contract": "answer",
                    "label": "Release answer",
                    "description": "Release notes with risks.",
                    "sections": [
                        {
                            "id": "changes",
                            "label": "Changes",
                            "description": "User-visible changes.",
                            "emphasis": "core",
                            "producedByTools": ["read"]
                        }
                    ]
                },
                "dbTouchpoints": {
                    "reads": [
                        {
                            "table": "project_context_records",
                            "kind": "read",
                            "purpose": "Read approved release records.",
                            "triggers": [{"kind": "tool", "name": "read"}],
                            "columns": ["record_id", "summary"]
                        }
                    ],
                    "writes": [],
                    "encouraged": []
                },
                "consumes": [
                    {
                        "id": "plan_pack",
                        "label": "Plan Pack",
                        "description": "Optional plan context.",
                        "sourceAgent": "plan",
                        "contract": "plan_pack",
                        "sections": ["decisions"],
                        "required": false
                    }
                ]
            }),
        };

        let detail = custom_detail(record, version);

        assert_eq!(detail.prompts.len(), 1);
        assert_eq!(detail.prompts[0].id, "release_prompt");
        assert_eq!(detail.tools.len(), 1);
        assert_eq!(detail.tools[0].name, "read");
        assert_eq!(detail.output.label, "Release answer");
        assert_eq!(detail.output.sections[0].id, "changes");
        assert_eq!(
            detail.db_touchpoints.reads[0].table,
            "project_context_records"
        );
        assert_eq!(detail.consumes[0].id, "plan_pack");
        let policy = detail.tool_policy_details.expect("granular tool policy");
        assert_eq!(policy.allowed_tools, vec!["read"]);
        assert_eq!(policy.denied_tools, vec!["write"]);
        assert_eq!(policy.allowed_tool_packs, vec!["release_notes_pack"]);
        assert_eq!(policy.denied_tool_groups, vec!["browser_control"]);
        assert_eq!(
            policy.allowed_effect_classes,
            vec![AgentToolEffectClassDto::Observe]
        );
    }

    #[test]
    fn s13_custom_detail_returns_agent_builder_definition_as_editable_authoring_graph() {
        let record = project_store::AgentDefinitionRecord {
            definition_id: "agent_builder_generated".into(),
            current_version: 1,
            display_name: "Agent Builder Generated".into(),
            short_label: "BuilderGen".into(),
            description: "Generated by Agent Builder for editing.".into(),
            scope: "project_custom".into(),
            lifecycle_state: "active".into(),
            base_capability_profile: "engineering".into(),
            created_at: "2026-05-01T12:00:00Z".into(),
            updated_at: "2026-05-01T12:00:00Z".into(),
        };
        let version = project_store::AgentDefinitionVersionRecord {
            definition_id: "agent_builder_generated".into(),
            version: 1,
            created_at: "2026-05-01T12:00:00Z".into(),
            validation_report: None,
            snapshot: json!({
                "schema": "xero.agent_definition.v1",
                "schemaVersion": 1,
                "id": "agent_builder_generated",
                "version": 1,
                "displayName": "Agent Builder Generated",
                "shortLabel": "BuilderGen",
                "description": "Generated by Agent Builder for editing.",
                "taskPurpose": "Exercise editable graph hydration.",
                "scope": "project_custom",
                "lifecycleState": "active",
                "baseCapabilityProfile": "engineering",
                "defaultApprovalMode": "suggest",
                "allowedApprovalModes": ["suggest", "auto_edit", "yolo"],
                "promptFragments": {},
                "prompts": [
                    {
                        "id": "agent_builder_generated.prompt",
                        "label": "Generated prompt",
                        "role": "developer",
                        "source": "agent_builder",
                        "body": "Generated agent-builder prompt body."
                    }
                ],
                "toolPolicy": {
                    "allowedEffectClasses": ["observe", "runtime_state", "write", "command"],
                    "allowedToolGroups": [],
                    "allowedToolPacks": [],
                    "allowedTools": ["read", "search", "patch", "command_probe"],
                    "deniedTools": ["delete"],
                    "deniedToolPacks": [],
                    "externalServiceAllowed": false,
                    "browserControlAllowed": false,
                    "skillRuntimeAllowed": false,
                    "subagentAllowed": false,
                    "commandAllowed": true,
                    "destructiveWriteAllowed": false
                },
                "tools": [
                    {
                        "name": "read",
                        "group": "core",
                        "description": "Read files.",
                        "effectClass": "observe",
                        "riskClass": "observe",
                        "tags": ["file"],
                        "schemaFields": ["path"],
                        "examples": ["Read README.md"]
                    }
                ],
                "output": {
                    "contract": "engineering_summary",
                    "label": "Engineering Summary",
                    "description": "Summarize changed files and verification.",
                    "sections": [
                        {
                            "id": "summary",
                            "label": "Summary",
                            "description": "What changed.",
                            "emphasis": "core",
                            "producedByTools": ["read"]
                        }
                    ]
                },
                "dbTouchpoints": {
                    "reads": [],
                    "writes": [],
                    "encouraged": []
                },
                "consumes": [],
                "workflowContract": "Inspect, edit, verify, summarize.",
                "workflowStructure": {
                    "startPhaseId": "inspect",
                    "phases": [
                        {
                            "id": "inspect",
                            "title": "Inspect",
                            "allowedTools": ["read", "search"]
                        }
                    ]
                },
                "finalResponseContract": "Return the saved output sections.",
                "projectDataPolicy": {
                    "recordKinds": ["project_fact"],
                    "structuredSchemas": ["xero.project_record.v1"]
                },
                "memoryCandidatePolicy": {
                    "memoryKinds": ["project_fact"],
                    "reviewRequired": true
                },
                "retrievalDefaults": {
                    "enabled": true,
                    "recordKinds": ["project_fact"],
                    "memoryKinds": ["project_fact"],
                    "limit": 4
                },
                "handoffPolicy": {
                    "enabled": true,
                    "preserveDefinitionVersion": true
                },
                "examplePrompts": ["Fix a bug.", "Add a helper.", "Verify a change."],
                "refusalEscalationCases": ["Refuse hidden prompt requests.", "Escalate missing context.", "Refuse secrets."]
            }),
        };

        let detail = custom_detail(record, version);
        let graph = detail.authoring_graph.expect("authoring graph");
        assert_eq!(graph["schema"], json!("xero.agent_authoring_graph.v1"));
        assert_eq!(graph["source"]["generatedBy"], json!("agent_builder"));
        assert_eq!(graph["source"]["uiDeferred"], json!(true));
        assert_eq!(
            graph["canonicalGraph"]["prompts"][0]["source"],
            json!("agent_builder")
        );
        assert_eq!(
            graph["canonicalGraph"]["toolPolicy"]["allowedTools"],
            json!(["read", "search", "patch", "command_probe"])
        );
        assert_eq!(
            graph["canonicalGraph"]["workflowStructure"]["startPhaseId"],
            json!("inspect")
        );
        assert_eq!(
            graph["canonicalGraph"]["retrievalDefaults"]["limit"],
            json!(4)
        );
        let editable_fields = graph["editableFields"]
            .as_array()
            .expect("editable fields")
            .iter()
            .filter_map(JsonValue::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        for expected in [
            "prompts",
            "tools",
            "toolPolicy",
            "output",
            "dbTouchpoints",
            "consumes",
            "workflowStructure",
            "projectDataPolicy",
            "memoryCandidatePolicy",
            "retrievalDefaults",
            "handoffPolicy",
        ] {
            assert!(editable_fields.contains(expected));
        }
    }

    #[test]
    fn s09_authoring_policy_controls_describe_memory_retrieval_and_handoff_fields() {
        let controls = authoring_policy_controls();

        let control = |id: &str| {
            controls
                .iter()
                .find(|control| control.id == id)
                .unwrap_or_else(|| panic!("missing policy control `{id}`"))
        };
        let memory_review = control("memory.reviewRequired");
        assert_eq!(
            memory_review.kind,
            AgentAuthoringPolicyControlKindDto::Memory
        );
        assert_eq!(
            memory_review.value_kind,
            AgentAuthoringPolicyControlValueKindDto::Boolean
        );
        assert_eq!(
            memory_review.snapshot_path,
            "memoryCandidatePolicy.reviewRequired"
        );
        assert_eq!(memory_review.default_value, json!(true));
        assert!(memory_review.review_required);

        let retrieval_limit = control("retrieval.limit");
        assert_eq!(
            retrieval_limit.kind,
            AgentAuthoringPolicyControlKindDto::Retrieval
        );
        assert_eq!(
            retrieval_limit.value_kind,
            AgentAuthoringPolicyControlValueKindDto::PositiveInteger
        );
        assert_eq!(retrieval_limit.default_value, json!(6));
        assert!(retrieval_limit.runtime_effect.contains("retrieval"));

        let handoff_version = control("handoff.preserveDefinitionVersion");
        assert_eq!(
            handoff_version.kind,
            AgentAuthoringPolicyControlKindDto::Handoff
        );
        assert_eq!(
            handoff_version.snapshot_path,
            "handoffPolicy.preserveDefinitionVersion"
        );
        assert_eq!(handoff_version.default_value, json!(true));

        let context_kinds = control("context.recordKinds");
        assert_eq!(
            context_kinds.kind,
            AgentAuthoringPolicyControlKindDto::Context
        );
        assert!(context_kinds
            .default_value
            .as_array()
            .expect("record kinds")
            .iter()
            .any(|kind| kind == "project_fact"));
    }

    #[test]
    fn s12_authoring_templates_are_canonical_custom_agent_starters() {
        let templates = authoring_templates();
        let template_ids = templates
            .iter()
            .map(|template| template.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for expected in [
            "engineering_patch",
            "debug_root_cause",
            "planning_pack",
            "repository_recon",
            "support_triage",
            "agent_builder",
        ] {
            assert!(
                template_ids.contains(expected),
                "missing authoring template `{expected}`"
            );
        }

        for template in templates {
            assert_eq!(
                template.definition["schema"],
                json!("xero.agent_definition.v1")
            );
            assert_eq!(template.definition["schemaVersion"], json!(1));
            assert_eq!(
                template.definition["baseCapabilityProfile"],
                json!(base_capability_profile_id(
                    &template.base_capability_profile
                ))
            );
            assert!(template.definition["prompts"]
                .as_array()
                .expect("template prompts")
                .iter()
                .any(|prompt| prompt["source"] == json!("template")));
            assert!(template.definition["tools"].as_array().is_some());
            assert!(template.definition["output"].is_object());
            assert!(template.definition["dbTouchpoints"].is_object());
            assert!(template.definition["consumes"].as_array().is_some());
            assert!(
                template
                    .definition
                    .get("examplePrompts")
                    .and_then(JsonValue::as_array)
                    .is_some_and(|examples| examples.len() >= 3),
                "template `{}` should include at least three examples",
                template.id
            );
        }
    }

    #[test]
    fn s63_creation_flows_reference_canonical_templates_and_runtime_contracts() {
        let templates = authoring_templates()
            .into_iter()
            .map(|template| (template.id.clone(), template))
            .collect::<std::collections::BTreeMap<_, _>>();
        let flows = authoring_creation_flows();
        assert!(flows.iter().any(|flow| {
            flow.entry_kind == AgentAuthoringCreationFlowEntryKindDto::DescribeIntent
        }));
        assert!(flows.iter().any(|flow| {
            flow.entry_kind == AgentAuthoringCreationFlowEntryKindDto::ComposeTemplates
                && flow.template_ids.len() > 1
        }));

        for flow in flows {
            assert!(!flow.intent_prompt.trim().is_empty());
            for template_id in &flow.template_ids {
                let template = templates
                    .get(template_id)
                    .unwrap_or_else(|| panic!("flow references missing template `{template_id}`"));
                assert_eq!(
                    template.definition["schema"],
                    json!("xero.agent_definition.v1")
                );
                assert_eq!(
                    template.base_capability_profile,
                    template
                        .definition
                        .get("baseCapabilityProfile")
                        .and_then(JsonValue::as_str)
                        .map(base_capability_from_str)
                        .expect("template base profile")
                );
            }
            assert!(templates.values().any(|template| {
                template.base_capability_profile == flow.base_capability_profile
                    && template
                        .definition
                        .get("output")
                        .and_then(|output| output.get("contract"))
                        .and_then(JsonValue::as_str)
                        == Some(output_contract_id(flow.expected_output_contract))
            }));
        }
    }

    #[test]
    fn s07_authoring_catalog_profile_availability_marks_constraints_and_upgrade_paths() {
        let tools: Vec<AgentToolSummaryDto> = deferred_tool_catalog(true)
            .into_iter()
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
            .collect();
        let profiles = authoring_profile_runtimes();
        let restricted_tool = tools
            .iter()
            .find(|tool| {
                !tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Ask, &tool.name)
                    && profiles
                        .iter()
                        .any(|(_, runtime)| tool_allowed_for_runtime_agent(*runtime, &tool.name))
            })
            .expect("tool requiring another profile");
        let first_table = available_builtin_runtime_agent_descriptors()
            .into_iter()
            .find_map(|descriptor| {
                db_touchpoints_for_runtime_agent(descriptor.id)
                    .entries
                    .first()
                    .map(|entry| AgentAuthoringDbTableDto {
                        table: entry.table.to_string(),
                        purpose: entry.purpose.to_string(),
                        columns: entry
                            .columns
                            .iter()
                            .map(|column| column.to_string())
                            .collect(),
                    })
            })
            .expect("profile-scoped db touchpoint");
        let upstream_artifacts = available_builtin_runtime_agent_descriptors()
            .into_iter()
            .map(|descriptor| AgentAuthoringUpstreamArtifactDto {
                source_agent: descriptor.id,
                source_agent_label: descriptor.label.clone(),
                contract: descriptor.output_contract,
                contract_label: output_contract_label(descriptor.output_contract).to_string(),
                label: format!("{} output", descriptor.label),
                description: output_contract_description(descriptor.output_contract).to_string(),
                sections: Vec::new(),
            })
            .collect::<Vec<_>>();

        let availability =
            authoring_profile_availability(&tools, &[first_table], &upstream_artifacts);

        let ask_restricted_tool = availability
            .iter()
            .find(|entry| {
                entry.subject_kind == "tool"
                    && entry.subject_id == restricted_tool.name
                    && entry.base_capability_profile
                        == AgentDefinitionBaseCapabilityProfileDto::ObserveOnly
            })
            .expect("Ask tool availability");
        assert_eq!(
            ask_restricted_tool.status,
            AgentAuthoringAvailabilityStatusDto::RequiresProfileChange
        );
        assert!(ask_restricted_tool.required_profile.is_some());
        assert!(ask_restricted_tool.reason.contains("requires"));

        assert!(availability.iter().any(|entry| {
            entry.subject_kind == "tool"
                && entry.subject_id == restricted_tool.name
                && entry.status == AgentAuthoringAvailabilityStatusDto::Available
        }));
        assert!(availability.iter().any(|entry| {
            entry.subject_kind == "db_touchpoint"
                && entry.status == AgentAuthoringAvailabilityStatusDto::Available
        }));
        assert!(availability.iter().any(|entry| {
            entry.subject_kind == "upstream_artifact"
                && entry.status != AgentAuthoringAvailabilityStatusDto::Unavailable
        }));
        assert!(availability.iter().any(|entry| {
            entry.subject_kind == "output_contract"
                && entry.subject_id == "engineering_summary"
                && entry.base_capability_profile
                    == AgentDefinitionBaseCapabilityProfileDto::Engineering
                && entry.status == AgentAuthoringAvailabilityStatusDto::Available
        }));
        assert!(availability.iter().any(|entry| {
            entry.subject_kind == "capability_control"
                && entry.subject_id == effect_class_id(restricted_tool.effect_class)
                && entry.status != AgentAuthoringAvailabilityStatusDto::Unavailable
        }));
    }

    #[test]
    fn s21_tool_pack_catalog_exposes_manifests_health_and_ui_deferral() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path()).expect("runtime");

        let catalog = agent_tool_pack_catalog("project-1".into(), &runtime);

        assert_eq!(catalog.schema, "xero.agent_tool_pack_catalog.v1");
        assert_eq!(catalog.project_id, "project-1");
        assert!(catalog.ui_deferred);
        assert!(!catalog.tool_packs.is_empty());
        assert_eq!(catalog.health_reports.len(), catalog.tool_packs.len());
        assert!(catalog.tool_packs.iter().any(
            |pack| !pack.review_requirements.is_empty() && !pack.approval_boundaries.is_empty()
        ));

        let manifest_ids = catalog
            .tool_packs
            .iter()
            .map(|pack| pack.pack_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(catalog
            .health_reports
            .iter()
            .all(|report| manifest_ids.contains(report.pack_id.as_str())));
        assert!(catalog
            .available_pack_ids
            .iter()
            .all(|pack_id| manifest_ids.contains(pack_id.as_str())));
    }

    #[test]
    fn s62_authoring_catalog_emits_contract_metadata_and_no_builder_diagnostics() {
        let catalog = agent_authoring_catalog();

        assert_eq!(
            catalog.contract_version,
            AGENT_AUTHORING_CATALOG_CONTRACT_VERSION
        );
        assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
        assert!(!catalog.tools.is_empty());
        assert!(!catalog.creation_flows.is_empty());
        assert!(!catalog.profile_availability.is_empty());
    }

    #[test]
    fn s62_authoring_catalog_validation_reports_uniqueness_and_reference_drift() {
        let mut catalog = agent_authoring_catalog();
        catalog.tools.push(catalog.tools[0].clone());
        catalog.tool_categories[0].tools[0].name = "missing_tool".into();
        catalog.creation_flows[0].template_ids = vec!["missing_template".into()];
        catalog.constraint_explanations[0].subject_id = "missing_subject".into();

        let diagnostics = validate_agent_authoring_catalog(&catalog);
        let codes = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<std::collections::BTreeSet<_>>();

        assert!(codes.contains("authoring_catalog_duplicate_tool_name"));
        assert!(codes.contains("authoring_catalog_unknown_category_tool"));
        assert!(codes.contains("authoring_catalog_unknown_creation_flow_template"));
        assert!(codes.contains("authoring_catalog_orphan_constraint_explanation"));
    }

    #[test]
    fn s62_authoring_constraint_explanations_are_specific_and_actionable() {
        let availability = vec![
            AgentAuthoringProfileAvailabilityDto {
                subject_kind: "tool".into(),
                subject_id: "write".into(),
                base_capability_profile: AgentDefinitionBaseCapabilityProfileDto::ObserveOnly,
                status: AgentAuthoringAvailabilityStatusDto::RequiresProfileChange,
                reason: "tool requires the `engineering` base capability profile.".into(),
                required_profile: Some(AgentDefinitionBaseCapabilityProfileDto::Engineering),
            },
            AgentAuthoringProfileAvailabilityDto {
                subject_kind: "capability_control".into(),
                subject_id: "external_service".into(),
                base_capability_profile: AgentDefinitionBaseCapabilityProfileDto::ObserveOnly,
                status: AgentAuthoringAvailabilityStatusDto::Unavailable,
                reason: "capability is not exposed by any current runtime profile.".into(),
                required_profile: None,
            },
        ];

        let explanations = authoring_constraint_explanations(&availability);

        let write_explanation = explanations
            .iter()
            .find(|entry| entry.subject_kind == "tool" && entry.subject_id == "write")
            .expect("write constraint explanation");
        assert_eq!(
            write_explanation.required_profile,
            Some(AgentDefinitionBaseCapabilityProfileDto::Engineering)
        );
        assert!(write_explanation.message.contains("Tool `write`"));
        assert!(write_explanation.message.contains("observe_only"));
        assert!(write_explanation.resolution.contains("engineering"));
        assert!(write_explanation.resolution.contains("remove `write`"));

        let unavailable = explanations
            .iter()
            .find(|entry| {
                entry.subject_kind == "capability_control" && entry.subject_id == "external_service"
            })
            .expect("unavailable constraint explanation");
        assert!(unavailable.message.contains("no current runtime profile"));
        assert!(unavailable
            .resolution
            .contains("install/enable a runtime capability"));
    }
}
