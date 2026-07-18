use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde_json::Value as JsonValue;

use crate::{
    commands::{
        contracts::{
            workflow_agents::AgentRefDto,
            workflows::{
                WorkflowArtifactContractDto, WorkflowConditionDto, WorkflowDefinitionDto,
                WorkflowEdgeDto, WorkflowEdgeTypeDto, WorkflowInputBindingDto, WorkflowNodeDto,
                WorkflowSubgraphDto, WorkflowTerminalStatusDto, WorkflowValidationDiagnosticDto,
                WorkflowValidationReportDto, WorkflowValidationSeverityDto,
                WorkflowValidationStatusDto,
            },
        },
        runtime_agent_descriptor,
    },
    db::project_store,
};

use super::command_policy::validate_workflow_command_policy;

pub fn validate_workflow_definition(
    definition: &WorkflowDefinitionDto,
) -> WorkflowValidationReportDto {
    let mut diagnostics = Vec::new();
    validate_required_fields(definition, &mut diagnostics);
    let mut subgraph_ids = BTreeSet::new();
    for (index, subgraph) in definition.subgraphs.iter().enumerate() {
        if !subgraph_ids.insert(subgraph.id.clone()) {
            diagnostics.push(error(
                "duplicate_subgraph_id",
                format!("subgraphs.{index}.id"),
                format!("Subgraph id `{}` is duplicated.", subgraph.id),
            ));
        }
    }

    validate_graph(
        definition,
        &definition.nodes,
        &definition.edges,
        &definition.start_node_id,
        "",
        None,
        &subgraph_ids,
        &mut diagnostics,
    );

    for (index, subgraph) in definition.subgraphs.iter().enumerate() {
        if subgraph.nodes.is_empty() {
            diagnostics.push(error(
                "subgraph_nodes_empty",
                format!("subgraphs.{index}.nodes"),
                format!("Subgraph `{}` must contain at least one node.", subgraph.id),
            ));
            continue;
        }
        validate_graph(
            definition,
            &subgraph.nodes,
            &subgraph.edges,
            &subgraph.start_node_id,
            &format!("subgraphs.{index}."),
            Some(subgraph),
            &subgraph_ids,
            &mut diagnostics,
        );
    }

    validate_subgraph_invocation_cycles(definition, &mut diagnostics);
    report_from_diagnostics(diagnostics)
}

#[allow(clippy::too_many_arguments)]
fn validate_graph<'a>(
    definition: &'a WorkflowDefinitionDto,
    nodes: &'a [WorkflowNodeDto],
    edges: &'a [WorkflowEdgeDto],
    start_node_id: &str,
    path_prefix: &str,
    subgraph: Option<&WorkflowSubgraphDto>,
    subgraph_ids: &BTreeSet<String>,
    diagnostics: &mut Vec<WorkflowValidationDiagnosticDto>,
) {
    let mut node_ids = BTreeSet::new();
    let mut produced_artifacts = BTreeSet::new();
    let mut artifact_contracts_by_ref: BTreeMap<String, &WorkflowArtifactContractDto> =
        BTreeMap::new();

    for (index, node) in nodes.iter().enumerate() {
        let id = node.id();
        if !node_ids.insert(id.to_string()) {
            diagnostics.push(error(
                "duplicate_node_id",
                format!("{path_prefix}nodes.{index}.id"),
                format!("Node id `{id}` is duplicated."),
            ));
        }
        let Some(artifact_type) = node.produced_artifact_type() else {
            continue;
        };
        let artifact_ref = format!("{id}.{artifact_type}");
        produced_artifacts.insert(artifact_ref.clone());
        let schema_version = node
            .output_contract()
            .map_or(1, |contract| contract.schema_version);
        let artifact_contract = definition.artifact_contracts.iter().find(|candidate| {
            candidate.artifact_type == artifact_type && candidate.schema_version == schema_version
        });
        if let Some(contract) = node.output_contract() {
            let contract_path = format!("{path_prefix}nodes.{index}.outputContract");
            if contract.artifact_type.trim().is_empty() {
                diagnostics.push(error(
                    "output_artifact_type_empty",
                    format!("{contract_path}.artifactType"),
                    "Workflow outputs must name their artifact type.",
                ));
            }
            if artifact_contract.is_none()
                && contract.extraction
                    != crate::commands::contracts::workflows::WorkflowOutputExtractionDto::GenericText
            {
                diagnostics.push(error(
                    "artifact_contract_missing",
                    &contract_path,
                    format!(
                        "JSON artifact `{}` v{} must declare an artifact contract.",
                        contract.artifact_type, contract.schema_version
                    ),
                ));
            }
            if let Some(render_text_path) = contract.render_text_path.as_deref() {
                if !render_text_path.trim().starts_with('$') {
                    diagnostics.push(error(
                        "render_text_path_invalid",
                        format!("{contract_path}.renderTextPath"),
                        "Render paths must use a JSON path that starts with `$`.",
                    ));
                } else if artifact_contract
                    .and_then(|candidate| candidate.json_schema.as_ref())
                    .is_some_and(|schema| !json_schema_allows_path(schema, render_text_path))
                {
                    diagnostics.push(error(
                        "render_text_path_not_in_schema",
                        format!("{contract_path}.renderTextPath"),
                        format!(
                            "Render path `{render_text_path}` is not allowed by the `{}` artifact schema.",
                            contract.artifact_type
                        ),
                    ));
                }
            }
        }
        if let Some(artifact_contract) = artifact_contract {
            artifact_contracts_by_ref.insert(artifact_ref, artifact_contract);
        }
    }

    if !node_ids.contains(start_node_id) {
        let (code, message) = match subgraph {
            Some(subgraph) => (
                "subgraph_start_node_missing",
                format!(
                    "Subgraph `{}` references a missing start node.",
                    subgraph.id
                ),
            ),
            None => (
                "start_node_missing",
                "The start node must exist.".to_string(),
            ),
        };
        diagnostics.push(error(code, format!("{path_prefix}startNodeId"), message));
    }

    let mut edge_ids = BTreeSet::new();
    let mut outgoing_defaults: BTreeMap<String, &str> = BTreeMap::new();
    let mut outgoing_edges: BTreeMap<&str, Vec<&WorkflowEdgeDto>> = BTreeMap::new();
    let loop_keys = edges
        .iter()
        .filter_map(|edge| edge.loop_policy.as_ref())
        .map(|policy| policy.loop_key.as_str())
        .collect::<BTreeSet<_>>();
    for (index, edge) in edges.iter().enumerate() {
        if !edge_ids.insert(edge.id.clone()) {
            diagnostics.push(error(
                "duplicate_edge_id",
                format!("{path_prefix}edges.{index}.id"),
                format!("Edge id `{}` is duplicated.", edge.id),
            ));
        }
        if !node_ids.contains(&edge.from_node_id) {
            let code = if subgraph.is_some() {
                "subgraph_edge_source_missing"
            } else {
                "edge_source_missing"
            };
            diagnostics.push(error(
                code,
                format!("{path_prefix}edges.{index}.fromNodeId"),
                format!("Edge `{}` references a missing source node.", edge.id),
            ));
        }
        if !node_ids.contains(&edge.to_node_id) {
            let code = if subgraph.is_some() {
                "subgraph_edge_target_missing"
            } else {
                "edge_target_missing"
            };
            diagnostics.push(error(
                code,
                format!("{path_prefix}edges.{index}.toNodeId"),
                format!("Edge `{}` references a missing target node.", edge.id),
            ));
        }
        if matches!(edge.condition, WorkflowConditionDto::Always) {
            let buckets = default_edge_buckets(edge.r#type);
            let conflicts = buckets.iter().any(|bucket| {
                outgoing_defaults.contains_key(&format!("{}:{bucket}", edge.from_node_id))
            });
            if conflicts {
                diagnostics.push(error(
                    "duplicate_default_edge",
                    format!("{path_prefix}edges.{index}.condition"),
                    format!(
                        "Node `{}` has more than one default else edge.",
                        edge.from_node_id
                    ),
                ));
            } else {
                for bucket in buckets {
                    outgoing_defaults
                        .insert(format!("{}:{bucket}", edge.from_node_id), edge.id.as_str());
                }
            }
        }
        if matches!(edge.r#type, WorkflowEdgeTypeDto::Loop) || edge.loop_policy.is_some() {
            match edge.loop_policy.as_ref() {
                Some(policy) => {
                    if policy.max_attempts == 0 {
                        diagnostics.push(error(
                            "loop_max_attempts_invalid",
                            format!("{path_prefix}edges.{index}.loopPolicy.maxAttempts"),
                            format!("Loop edge `{}` must allow at least one attempt.", edge.id),
                        ));
                    }
                    if !node_ids.contains(&policy.on_exhausted) {
                        diagnostics.push(error(
                            "loop_exhaustion_target_missing",
                            format!("{path_prefix}edges.{index}.loopPolicy.onExhausted"),
                            format!(
                                "Loop edge `{}` must route exhaustion to an existing node.",
                                edge.id
                            ),
                        ));
                    }
                    if policy.loop_key.trim().is_empty() {
                        diagnostics.push(error(
                            "loop_key_empty",
                            format!("{path_prefix}edges.{index}.loopPolicy.loopKey"),
                            "Loop policies must declare a loop key.",
                        ));
                    }
                    for (artifact_index, artifact_ref) in
                        policy.selected_artifact_refs.iter().enumerate()
                    {
                        if !produced_artifacts.contains(artifact_ref) {
                            diagnostics.push(error(
                                "loop_artifact_ref_missing",
                                format!(
                                    "{path_prefix}edges.{index}.loopPolicy.selectedArtifactRefs.{artifact_index}"
                                ),
                                format!(
                                    "Loop policy references missing artifact `{artifact_ref}`."
                                ),
                            ));
                        }
                    }
                }
                None => diagnostics.push(error(
                    "loop_policy_missing",
                    format!("{path_prefix}edges.{index}.loopPolicy"),
                    format!("Loop edge `{}` must declare a loop policy.", edge.id),
                )),
            }
        }
        validate_condition_semantics(
            &edge.condition,
            format!("{path_prefix}edges.{index}.condition"),
            &node_ids,
            &produced_artifacts,
            &artifact_contracts_by_ref,
            &loop_keys,
            diagnostics,
        );

        outgoing_edges
            .entry(edge.from_node_id.as_str())
            .or_default()
            .push(edge);
    }

    for (node_id, node_edges) in &outgoing_edges {
        let mut conditional_buckets = BTreeSet::new();
        for edge in node_edges {
            if !matches!(edge.condition, WorkflowConditionDto::Always) {
                conditional_buckets.extend(default_edge_buckets(edge.r#type));
            }
        }
        for bucket in conditional_buckets {
            if !outgoing_defaults.contains_key(&format!("{node_id}:all"))
                && !outgoing_defaults.contains_key(&format!("{node_id}:{bucket}"))
            {
                diagnostics.push(error(
                    "conditional_route_fallback_missing",
                    format!("{path_prefix}nodes.{node_id}"),
                    format!(
                        "Node `{node_id}` has conditional {bucket} routes but no `always` fallback for that outcome. Route the fallback to a Human Checkpoint or Terminal node."
                    ),
                ));
            }
        }
    }

    for (index, node) in nodes.iter().enumerate() {
        let node_path = format!("{path_prefix}nodes.{index}");
        if subgraph.is_some()
            && matches!(
                node,
                WorkflowNodeDto::Terminal {
                    terminal_status: WorkflowTerminalStatusDto::NeedsHuman,
                    ..
                }
            )
        {
            diagnostics.push(error(
                "subgraph_needs_human_terminal_unsupported",
                format!("{node_path}.terminalStatus"),
                "Subgraphs cannot pause through a Needs Human Terminal. Route to a Human Checkpoint so the paused run has a resumable gate.",
            ));
        }
        if let Some(input_bindings) = node_input_bindings(node) {
            for (binding_index, binding) in input_bindings.iter().enumerate() {
                let binding_path = format!("{node_path}.inputBindings.{binding_index}");
                let path = match binding {
                    WorkflowInputBindingDto::RunInput { path, .. }
                    | WorkflowInputBindingDto::Artifact { path, .. }
                    | WorkflowInputBindingDto::State { path, .. } => path,
                };
                if path
                    .as_deref()
                    .is_some_and(|value| !value.trim().starts_with('$'))
                {
                    diagnostics.push(error(
                        "input_binding_path_invalid",
                        format!("{binding_path}.path"),
                        "Input binding paths must use a JSON path that starts with `$`.",
                    ));
                }
                if let WorkflowInputBindingDto::Artifact { artifact_ref, .. } = binding {
                    if !produced_artifacts.contains(artifact_ref) {
                        diagnostics.push(error(
                            "artifact_ref_missing",
                            format!("{binding_path}.artifactRef"),
                            format!(
                                "Artifact reference `{artifact_ref}` is not produced by any agent node."
                            ),
                        ));
                    }
                }
                if let WorkflowInputBindingDto::State { state_ref, .. } = binding {
                    if !produced_artifacts.contains(state_ref) {
                        diagnostics.push(error(
                            "state_ref_missing",
                            format!("{binding_path}.stateRef"),
                            format!(
                                "State reference `{state_ref}` is not produced by any state-capable node."
                            ),
                        ));
                    }
                }
            }
        }
        match node {
            WorkflowNodeDto::StateRead { query, .. }
            | WorkflowNodeDto::StateQuery { query, .. } => {
                validate_state_query(query, format!("{node_path}.query"), diagnostics);
            }
            WorkflowNodeDto::StateWrite { operation, .. }
            | WorkflowNodeDto::StatePatch { operation, .. } => {
                validate_state_write_operation(
                    operation,
                    format!("{node_path}.operation"),
                    diagnostics,
                    true,
                );
            }
            WorkflowNodeDto::CollectionLoop {
                collection,
                controls,
                sort_key,
                max_item_count,
                ..
            } => {
                validate_state_query(collection, format!("{node_path}.collection"), diagnostics);
                if *max_item_count == 0 {
                    diagnostics.push(error(
                        "collection_loop_max_item_count_invalid",
                        format!("{node_path}.maxItemCount"),
                        "Collection loops must allow at least one item.",
                    ));
                }
                if sort_key
                    .as_deref()
                    .is_some_and(|path| !path.trim().starts_with('$'))
                {
                    diagnostics.push(error(
                        "collection_loop_sort_path_invalid",
                        format!("{node_path}.sortKey"),
                        "Collection loop sort keys must use a JSON path that starts with `$`.",
                    ));
                }
                for (field, path) in [
                    ("fromInputPath", controls.from_input_path.as_deref()),
                    ("toInputPath", controls.to_input_path.as_deref()),
                    ("onlyInputPath", controls.only_input_path.as_deref()),
                ] {
                    if path.is_some_and(|path| !workflow_resume_control_input_path_valid(path)) {
                        diagnostics.push(error(
                            "collection_loop_control_input_path_invalid",
                            format!("{node_path}.controls.{field}"),
                            "Collection loop resume controls must use `$` or an object-field JSON path such as `$.phase.from`; array indexes are not supported.",
                        ));
                    }
                }
            }
            WorkflowNodeDto::Subgraph { subgraph_id, .. } => {
                if !subgraph_ids.contains(subgraph_id) {
                    diagnostics.push(error(
                        "subgraph_ref_missing",
                        format!("{node_path}.subgraphId"),
                        format!("Subgraph node references missing subgraph `{subgraph_id}`."),
                    ));
                }
            }
            WorkflowNodeDto::Command {
                command,
                args,
                allowed_commands,
                timeout_seconds,
                ..
            } => {
                if command.trim().is_empty() {
                    diagnostics.push(error(
                        "command_empty",
                        format!("{node_path}.command"),
                        "Command nodes must declare a command.",
                    ));
                }
                if *timeout_seconds == 0 {
                    diagnostics.push(error(
                        "command_timeout_invalid",
                        format!("{node_path}.timeoutSeconds"),
                        "Command node timeout must be at least one second.",
                    ));
                }
                if allowed_commands.is_empty() {
                    diagnostics.push(error(
                        "command_allowlist_empty",
                        format!("{node_path}.allowedCommands"),
                        "Command nodes must declare an allowlist.",
                    ));
                } else if !allowed_commands.iter().any(|allowed| allowed == command) {
                    diagnostics.push(error(
                        "command_not_in_allowlist",
                        format!("{node_path}.allowedCommands"),
                        format!("Command `{command}` must appear in the command node allowlist."),
                    ));
                }
                if !command.trim().is_empty() {
                    if let Err(violation) = validate_workflow_command_policy(command, args) {
                        diagnostics.push(error(
                            violation.code,
                            if violation.code
                                == "workflow_command_arguments_not_allowed_by_app_policy"
                            {
                                format!("{node_path}.args")
                            } else {
                                format!("{node_path}.command")
                            },
                            violation.message,
                        ));
                    }
                }
            }
            WorkflowNodeDto::HumanCheckpoint {
                decision_options,
                resume_payload_schema,
                state_updates,
                ..
            } => {
                let mut seen = BTreeSet::new();
                for (option_index, option) in decision_options.iter().enumerate() {
                    let option = option.trim();
                    if option.is_empty() {
                        diagnostics.push(error(
                            "checkpoint_decision_empty",
                            format!("{node_path}.decisionOptions.{option_index}"),
                            "Human checkpoint decision options cannot be blank.",
                        ));
                    } else if !seen.insert(option.to_string()) {
                        diagnostics.push(error(
                            "checkpoint_decision_duplicate",
                            format!("{node_path}.decisionOptions.{option_index}"),
                            format!("Human checkpoint decision `{option}` is duplicated."),
                        ));
                    }
                }
                if resume_payload_schema
                    .as_ref()
                    .is_some_and(|schema| !schema.is_object())
                {
                    diagnostics.push(error(
                        "checkpoint_payload_schema_invalid",
                        format!("{node_path}.resumePayloadSchema"),
                        "Human checkpoint resume payload schemas must be JSON Schema objects.",
                    ));
                }
                for (update_index, operation) in state_updates.iter().enumerate() {
                    validate_state_write_operation(
                        operation,
                        format!("{node_path}.stateUpdates.{update_index}"),
                        diagnostics,
                        false,
                    );
                }
            }
            WorkflowNodeDto::Merge {
                wait_policy,
                quorum,
                ..
            } => {
                if wait_policy
                    == &crate::commands::contracts::workflows::WorkflowMergeWaitPolicyDto::Quorum
                    && quorum.unwrap_or(0) == 0
                {
                    diagnostics.push(error(
                        "merge_quorum_missing",
                        format!("{node_path}.quorum"),
                        "Quorum merge nodes must declare a quorum.",
                    ));
                }
            }
            WorkflowNodeDto::Gate {
                required_checks,
                on_blocked,
                ..
            }
            | WorkflowNodeDto::StateCheckpoint {
                required_checks,
                on_blocked,
                ..
            } => {
                if !matches!(on_blocked.as_str(), "pause" | "fail") {
                    diagnostics.push(error(
                        "gate_on_blocked_invalid",
                        format!("{node_path}.onBlocked"),
                        "Gate onBlocked behavior must be `pause` or `fail`.",
                    ));
                }
                for (check_index, condition) in required_checks.iter().enumerate() {
                    validate_condition_semantics(
                        condition,
                        format!("{node_path}.requiredChecks.{check_index}"),
                        &node_ids,
                        &produced_artifacts,
                        &artifact_contracts_by_ref,
                        &loop_keys,
                        diagnostics,
                    );
                }
            }
            _ => {}
        }
    }

    diagnostics.extend(detect_unbounded_cycles(
        nodes,
        start_node_id,
        &outgoing_edges,
        format!("{path_prefix}edges"),
    ));
}

fn workflow_resume_control_input_path_valid(path: &str) -> bool {
    let path = path.trim();
    if path == "$" {
        return true;
    }
    let Some(path) = path.strip_prefix("$.") else {
        return false;
    };
    !path.is_empty()
        && path.split('.').all(|segment| {
            !segment.is_empty()
                && !segment
                    .chars()
                    .any(|character| character.is_whitespace() || matches!(character, '[' | ']'))
        })
}

pub fn validate_workflow_definition_with_registry(
    repo_root: &Path,
    definition: &WorkflowDefinitionDto,
) -> WorkflowValidationReportDto {
    let mut report = validate_workflow_definition(definition);
    for (index, node) in definition.nodes.iter().enumerate() {
        let WorkflowNodeDto::Agent { agent_ref, .. } = node else {
            continue;
        };
        validate_agent_ref(
            repo_root,
            &format!("nodes[{index}].agentRef"),
            agent_ref,
            &mut report.diagnostics,
        );
    }
    for (subgraph_index, subgraph) in definition.subgraphs.iter().enumerate() {
        for (node_index, node) in subgraph.nodes.iter().enumerate() {
            let WorkflowNodeDto::Agent { agent_ref, .. } = node else {
                continue;
            };
            validate_agent_ref(
                repo_root,
                &format!("subgraphs[{subgraph_index}].nodes[{node_index}].agentRef"),
                agent_ref,
                &mut report.diagnostics,
            );
        }
    }
    report_from_diagnostics(report.diagnostics)
}

fn validate_agent_ref(
    repo_root: &Path,
    path: &str,
    agent_ref: &AgentRefDto,
    diagnostics: &mut Vec<WorkflowValidationDiagnosticDto>,
) {
    match agent_ref {
        AgentRefDto::BuiltIn {
            runtime_agent_id,
            version,
        } => {
            let descriptor = runtime_agent_descriptor(*runtime_agent_id);
            if *version == 0 {
                diagnostics.push(error(
                    "agent_ref_builtin_version_required",
                    format!("{path}.version"),
                    "Built-in agent refs must declare a supported version.",
                ));
            } else if *version != descriptor.version {
                diagnostics.push(error(
                    "agent_ref_builtin_version_unsupported",
                    format!("{path}.version"),
                    format!(
                        "Built-in agent `{}` supports version {}, but the Workflow requested version {}.",
                        runtime_agent_id.as_str(),
                        descriptor.version,
                        version
                    ),
                ));
            }
        }
        AgentRefDto::Custom {
            definition_id,
            version,
        } => {
            if definition_id.trim().is_empty() {
                diagnostics.push(error(
                    "agent_ref_custom_definition_required",
                    format!("{path}.definitionId"),
                    "Custom agent refs must declare definitionId.",
                ));
                return;
            }
            if *version == 0 {
                diagnostics.push(error(
                    "agent_ref_custom_version_required",
                    format!("{path}.version"),
                    "Custom agent refs must declare a requested version.",
                ));
                return;
            }
            if let Err(err) = project_store::resolve_agent_definition_version_for_run(
                repo_root,
                Some(definition_id),
                Some(*version),
                crate::commands::default_runtime_agent_id(),
            ) {
                let (code, path) = match err.code.as_str() {
                    "agent_definition_not_found" => (
                        "agent_ref_custom_definition_missing",
                        format!("{path}.definitionId"),
                    ),
                    "agent_definition_inactive" => (
                        "agent_ref_custom_definition_inactive",
                        format!("{path}.definitionId"),
                    ),
                    "agent_definition_version_required" => (
                        "agent_ref_custom_version_required",
                        format!("{path}.version"),
                    ),
                    "agent_definition_version_missing" => (
                        "agent_ref_custom_version_missing",
                        format!("{path}.version"),
                    ),
                    "agent_definition_activation_preflight_failed" => (
                        "agent_ref_custom_activation_preflight_failed",
                        format!("{path}.version"),
                    ),
                    _ => (
                        "agent_ref_custom_unavailable",
                        format!("{path}.definitionId"),
                    ),
                };
                diagnostics.push(error(code, path, err.message));
            }
        }
    }
}

fn report_from_diagnostics(
    diagnostics: Vec<WorkflowValidationDiagnosticDto>,
) -> WorkflowValidationReportDto {
    WorkflowValidationReportDto {
        status: if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == WorkflowValidationSeverityDto::Error)
        {
            WorkflowValidationStatusDto::Invalid
        } else {
            WorkflowValidationStatusDto::Valid
        },
        diagnostics,
    }
}

fn validate_state_query(
    query: &crate::commands::contracts::workflows::WorkflowStateQueryDto,
    path: String,
    diagnostics: &mut Vec<WorkflowValidationDiagnosticDto>,
) {
    for (index, filter) in query.filters.iter().enumerate() {
        if !filter.path.trim().starts_with('$') {
            diagnostics.push(error(
                "state_query_filter_path_invalid",
                format!("{path}.filters.{index}.path"),
                "State query filter paths must use a JSON path that starts with `$`.",
            ));
        }
    }
    if query
        .order_by
        .as_deref()
        .is_some_and(|order_by| !order_by.trim().starts_with('$'))
    {
        diagnostics.push(error(
            "state_query_order_path_invalid",
            format!("{path}.orderBy"),
            "State query order paths must use a JSON path that starts with `$`.",
        ));
    }
}

fn validate_state_write_operation(
    operation: &crate::commands::contracts::workflows::WorkflowStateWriteOperationDto,
    path: String,
    diagnostics: &mut Vec<WorkflowValidationDiagnosticDto>,
    require_output_artifact: bool,
) {
    if require_output_artifact && operation.output_artifact_type.trim().is_empty() {
        diagnostics.push(error(
            "state_write_output_artifact_empty",
            format!("{path}.outputArtifactType"),
            "State write nodes must name their output artifact.",
        ));
    }
    if operation
        .idempotency_key
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        diagnostics.push(error(
            "state_write_idempotency_key_empty",
            format!("{path}.idempotencyKey"),
            "State write idempotency keys cannot be blank.",
        ));
    }
    if operation
        .target_id
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        diagnostics.push(error(
            "state_write_target_id_empty",
            format!("{path}.targetId"),
            "State write target ids cannot be blank.",
        ));
    }
}

fn validate_condition_semantics(
    condition: &WorkflowConditionDto,
    path: String,
    node_ids: &BTreeSet<String>,
    produced_artifacts: &BTreeSet<String>,
    artifact_contracts_by_ref: &BTreeMap<String, &WorkflowArtifactContractDto>,
    loop_keys: &BTreeSet<&str>,
    diagnostics: &mut Vec<WorkflowValidationDiagnosticDto>,
) {
    validate_condition_shape(condition, path.clone(), diagnostics);
    for artifact_ref in condition_artifact_refs(condition) {
        if !produced_artifacts.contains(&artifact_ref) {
            diagnostics.push(error(
                "condition_artifact_ref_missing",
                &path,
                format!("Condition references missing artifact `{artifact_ref}`."),
            ));
        }
    }
    for (artifact_ref, json_path) in condition_artifact_field_refs(condition) {
        if artifact_contracts_by_ref
            .get(&artifact_ref)
            .and_then(|contract| contract.json_schema.as_ref())
            .is_some_and(|schema| !json_schema_allows_path(schema, &json_path))
        {
            diagnostics.push(error(
                "condition_artifact_path_not_in_schema",
                &path,
                format!(
                    "Condition references `{artifact_ref}{json_path}`, but that field is not allowed by the artifact schema."
                ),
            ));
        }
    }
    for state_ref in condition_state_refs(condition) {
        if !produced_artifacts.contains(&state_ref) {
            diagnostics.push(error(
                "condition_state_ref_missing",
                &path,
                format!("Condition references missing state value `{state_ref}`."),
            ));
        }
    }
    for node_ref in condition_node_refs(condition) {
        if !node_ids.contains(&node_ref) {
            diagnostics.push(error(
                "condition_node_ref_missing",
                &path,
                format!("Condition references missing node `{node_ref}`."),
            ));
        }
    }
    for loop_key in condition_loop_keys(condition) {
        if !loop_keys.contains(loop_key.as_str()) {
            diagnostics.push(error(
                "condition_loop_key_missing",
                &path,
                format!("Condition references missing loop key `{loop_key}`."),
            ));
        }
    }
}

fn validate_subgraph_invocation_cycles(
    definition: &WorkflowDefinitionDto,
    diagnostics: &mut Vec<WorkflowValidationDiagnosticDto>,
) {
    for (subgraph_index, subgraph) in definition.subgraphs.iter().enumerate() {
        for (node_index, node) in subgraph.nodes.iter().enumerate() {
            let WorkflowNodeDto::Subgraph { subgraph_id, .. } = node else {
                continue;
            };
            if !definition
                .subgraphs
                .iter()
                .any(|candidate| candidate.id == *subgraph_id)
            {
                continue;
            }
            let mut visiting = BTreeSet::new();
            if subgraph_invokes_target(definition, subgraph_id, &subgraph.id, &mut visiting) {
                diagnostics.push(error(
                    "recursive_subgraph_invocation",
                    format!("subgraphs.{subgraph_index}.nodes.{node_index}.subgraphId"),
                    format!(
                        "Subgraph `{}` recursively invokes `{subgraph_id}`; recursive subgraph invocation is unsupported.",
                        subgraph.id
                    ),
                ));
            }
        }
    }
}

fn subgraph_invokes_target(
    definition: &WorkflowDefinitionDto,
    current_id: &str,
    target_id: &str,
    visiting: &mut BTreeSet<String>,
) -> bool {
    if current_id == target_id {
        return true;
    }
    if !visiting.insert(current_id.to_string()) {
        return false;
    }
    let reaches_target = definition
        .subgraphs
        .iter()
        .find(|subgraph| subgraph.id == current_id)
        .is_some_and(|subgraph| {
            subgraph.nodes.iter().any(|node| {
                let WorkflowNodeDto::Subgraph { subgraph_id, .. } = node else {
                    return false;
                };
                subgraph_invokes_target(definition, subgraph_id, target_id, visiting)
            })
        });
    visiting.remove(current_id);
    reaches_target
}

fn node_input_bindings(node: &WorkflowNodeDto) -> Option<&Vec<WorkflowInputBindingDto>> {
    match node {
        WorkflowNodeDto::Agent { input_bindings, .. }
        | WorkflowNodeDto::StateWrite { input_bindings, .. }
        | WorkflowNodeDto::StatePatch { input_bindings, .. }
        | WorkflowNodeDto::Subgraph { input_bindings, .. } => Some(input_bindings),
        _ => None,
    }
}

fn default_edge_buckets(edge_type: WorkflowEdgeTypeDto) -> Vec<&'static str> {
    match edge_type {
        WorkflowEdgeTypeDto::Success => vec!["success"],
        WorkflowEdgeTypeDto::Failure | WorkflowEdgeTypeDto::Recovery => vec!["failure"],
        WorkflowEdgeTypeDto::Conditional
        | WorkflowEdgeTypeDto::Loop
        | WorkflowEdgeTypeDto::ManualOverride => vec!["all"],
    }
}

fn condition_artifact_field_refs(condition: &WorkflowConditionDto) -> Vec<(String, String)> {
    match condition {
        WorkflowConditionDto::ArtifactFieldEquals {
            artifact_ref, path, ..
        }
        | WorkflowConditionDto::ArtifactFieldIn {
            artifact_ref, path, ..
        }
        | WorkflowConditionDto::ArtifactFieldNumberCompare {
            artifact_ref, path, ..
        } => vec![(artifact_ref.clone(), path.clone())],
        WorkflowConditionDto::All { conditions } | WorkflowConditionDto::Any { conditions } => {
            conditions
                .iter()
                .flat_map(condition_artifact_field_refs)
                .collect()
        }
        WorkflowConditionDto::Not { condition } => condition_artifact_field_refs(condition),
        _ => Vec::new(),
    }
}

fn json_schema_allows_path(schema: &JsonValue, path: &str) -> bool {
    if path == "$" {
        return true;
    }
    let Some(remainder) = path.strip_prefix("$.") else {
        return false;
    };
    let mut cursor = schema;
    for raw_segment in remainder.split('.') {
        let Some((field, indexes)) = parse_schema_path_segment(raw_segment) else {
            return false;
        };
        if !schema_type_allows_object(cursor) {
            return false;
        }
        let Some(properties) = cursor.get("properties").and_then(JsonValue::as_object) else {
            return false;
        };
        let Some(next) = properties.get(field) else {
            return false;
        };
        cursor = next;
        for _ in 0..indexes {
            let Some(items) = cursor.get("items") else {
                return false;
            };
            cursor = items;
        }
    }
    true
}

fn parse_schema_path_segment(segment: &str) -> Option<(&str, usize)> {
    let field_end = segment.find('[').unwrap_or(segment.len());
    let field = &segment[..field_end];
    if field.is_empty() {
        return None;
    }
    let mut indexes = 0;
    let mut rest = &segment[field_end..];
    while !rest.is_empty() {
        let inner = rest.strip_prefix('[')?;
        let close = inner.find(']')?;
        inner[..close].parse::<usize>().ok()?;
        indexes += 1;
        rest = &inner[close + 1..];
    }
    Some((field, indexes))
}

fn schema_type_allows_object(schema: &JsonValue) -> bool {
    match schema.get("type") {
        Some(JsonValue::String(value)) => value == "object",
        Some(JsonValue::Array(values)) => values.iter().any(|value| value == "object"),
        _ => true,
    }
}

fn validate_required_fields(
    definition: &WorkflowDefinitionDto,
    diagnostics: &mut Vec<WorkflowValidationDiagnosticDto>,
) {
    if definition.schema != "xero.workflow_definition.v1" {
        diagnostics.push(error(
            "schema_unsupported",
            "schema",
            "Workflow definitions must use schema `xero.workflow_definition.v1`.",
        ));
    }
    for (field, value) in [
        ("id", definition.id.as_str()),
        ("projectId", definition.project_id.as_str()),
        ("name", definition.name.as_str()),
        ("startNodeId", definition.start_node_id.as_str()),
    ] {
        if value.trim().is_empty() {
            diagnostics.push(error(
                "required_field_empty",
                field,
                format!("Workflow field `{field}` cannot be empty."),
            ));
        }
    }
    if definition.nodes.is_empty() {
        diagnostics.push(error(
            "nodes_empty",
            "nodes",
            "A Workflow must contain at least one node.",
        ));
    }
    if definition.run_policy.concurrency_limit == 0 {
        diagnostics.push(error(
            "concurrency_limit_invalid",
            "runPolicy.concurrencyLimit",
            "Workflow concurrency limit must be at least 1.",
        ));
    }
}

fn validate_condition_shape(
    condition: &WorkflowConditionDto,
    path: String,
    diagnostics: &mut Vec<WorkflowValidationDiagnosticDto>,
) {
    match condition {
        WorkflowConditionDto::All { conditions } | WorkflowConditionDto::Any { conditions } => {
            if conditions.is_empty() {
                diagnostics.push(error(
                    "condition_children_empty",
                    path.clone(),
                    "Composite Workflow conditions must contain at least one child condition.",
                ));
            }
            for (index, child) in conditions.iter().enumerate() {
                validate_condition_shape(child, format!("{path}.conditions.{index}"), diagnostics);
            }
        }
        WorkflowConditionDto::Not { condition } => {
            validate_condition_shape(condition, format!("{path}.condition"), diagnostics);
        }
        WorkflowConditionDto::ArtifactFieldEquals {
            path: json_path, ..
        }
        | WorkflowConditionDto::ArtifactFieldIn {
            path: json_path, ..
        }
        | WorkflowConditionDto::ArtifactFieldNumberCompare {
            path: json_path, ..
        }
        | WorkflowConditionDto::StateFieldEquals {
            path: json_path, ..
        } => {
            if !json_path.starts_with('$') {
                diagnostics.push(error(
                    "condition_json_path_invalid",
                    path,
                    "Workflow field conditions must use a JSON path that starts with `$`.",
                ));
            }
        }
        _ => {}
    }
}

fn detect_unbounded_cycles(
    nodes: &[WorkflowNodeDto],
    start_node_id: &str,
    outgoing_edges: &BTreeMap<&str, Vec<&WorkflowEdgeDto>>,
    edges_path: String,
) -> Vec<WorkflowValidationDiagnosticDto> {
    let mut detector = CycleDetector {
        outgoing_edges,
        edges_path,
        visiting: BTreeSet::new(),
        visited: BTreeSet::new(),
        stack: Vec::new(),
        reported_cycles: BTreeSet::new(),
        diagnostics: Vec::new(),
    };
    if nodes.iter().any(|node| node.id() == start_node_id) {
        detector.visit(start_node_id);
    }
    detector.diagnostics
}

struct CycleDetector<'a> {
    outgoing_edges: &'a BTreeMap<&'a str, Vec<&'a WorkflowEdgeDto>>,
    edges_path: String,
    visiting: BTreeSet<String>,
    visited: BTreeSet<String>,
    stack: Vec<&'a WorkflowEdgeDto>,
    reported_cycles: BTreeSet<String>,
    diagnostics: Vec<WorkflowValidationDiagnosticDto>,
}

impl<'a> CycleDetector<'a> {
    fn visit(&mut self, node_id: &str) {
        if self.visiting.contains(node_id) {
            let start_index = self
                .stack
                .iter()
                .position(|edge| edge.from_node_id == node_id)
                .unwrap_or(0);
            let cycle = &self.stack[start_index..];
            let cycle_key = cycle
                .iter()
                .map(|edge| edge.id.as_str())
                .collect::<Vec<_>>()
                .join(" -> ");
            if !cycle.iter().any(|edge| {
                matches!(edge.r#type, WorkflowEdgeTypeDto::Loop) && edge.loop_policy.is_some()
            }) && self.reported_cycles.insert(cycle_key.clone())
            {
                self.diagnostics.push(error(
                    "cycle_without_loop_policy",
                    &self.edges_path,
                    format!("Cycle `{cycle_key}` must include an explicit bounded loop edge."),
                ));
            }
            return;
        }
        if self.visited.contains(node_id) {
            return;
        }

        self.visiting.insert(node_id.to_string());
        if let Some(edges) = self.outgoing_edges.get(node_id) {
            for edge in edges {
                self.stack.push(edge);
                self.visit(&edge.to_node_id);
                self.stack.pop();
            }
        }
        self.visiting.remove(node_id);
        self.visited.insert(node_id.to_string());
    }
}

fn condition_artifact_refs(condition: &WorkflowConditionDto) -> Vec<String> {
    match condition {
        WorkflowConditionDto::ArtifactExists { artifact_ref }
        | WorkflowConditionDto::ArtifactFieldEquals { artifact_ref, .. }
        | WorkflowConditionDto::ArtifactFieldIn { artifact_ref, .. }
        | WorkflowConditionDto::ArtifactFieldNumberCompare { artifact_ref, .. } => {
            vec![artifact_ref.clone()]
        }
        WorkflowConditionDto::All { conditions } | WorkflowConditionDto::Any { conditions } => {
            conditions
                .iter()
                .flat_map(condition_artifact_refs)
                .collect()
        }
        WorkflowConditionDto::Not { condition } => condition_artifact_refs(condition),
        _ => Vec::new(),
    }
}

fn condition_state_refs(condition: &WorkflowConditionDto) -> Vec<String> {
    match condition {
        WorkflowConditionDto::StateFieldEquals { state_ref, .. }
        | WorkflowConditionDto::StateCollectionCountCompare { state_ref, .. } => {
            vec![state_ref.clone()]
        }
        WorkflowConditionDto::All { conditions } | WorkflowConditionDto::Any { conditions } => {
            conditions.iter().flat_map(condition_state_refs).collect()
        }
        WorkflowConditionDto::Not { condition } => condition_state_refs(condition),
        _ => Vec::new(),
    }
}

fn condition_node_refs(condition: &WorkflowConditionDto) -> Vec<String> {
    match condition {
        WorkflowConditionDto::NodeStatus { node_id, .. } => vec![node_id.clone()],
        WorkflowConditionDto::FailureClassIs {
            node_id: Some(node_id),
            ..
        } => vec![node_id.clone()],
        WorkflowConditionDto::HumanDecisionIs {
            checkpoint_node_id, ..
        } => vec![checkpoint_node_id.clone()],
        WorkflowConditionDto::All { conditions } | WorkflowConditionDto::Any { conditions } => {
            conditions.iter().flat_map(condition_node_refs).collect()
        }
        WorkflowConditionDto::Not { condition } => condition_node_refs(condition),
        _ => Vec::new(),
    }
}

fn condition_loop_keys(condition: &WorkflowConditionDto) -> Vec<String> {
    match condition {
        WorkflowConditionDto::LoopAttemptLt { loop_key, .. }
        | WorkflowConditionDto::LoopAttemptGte { loop_key, .. } => vec![loop_key.clone()],
        WorkflowConditionDto::All { conditions } | WorkflowConditionDto::Any { conditions } => {
            conditions.iter().flat_map(condition_loop_keys).collect()
        }
        WorkflowConditionDto::Not { condition } => condition_loop_keys(condition),
        _ => Vec::new(),
    }
}

fn error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> WorkflowValidationDiagnosticDto {
    WorkflowValidationDiagnosticDto {
        severity: WorkflowValidationSeverityDto::Error,
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::contracts::{
        runtime::RuntimeAgentIdDto,
        workflow_agents::AgentRefDto,
        workflows::{
            WorkflowArtifactContractDto, WorkflowEdgeDto, WorkflowEdgeTypeDto,
            WorkflowInputBindingDto, WorkflowNodeDto, WorkflowOutputContractDto,
            WorkflowOutputExtractionDto, WorkflowRunPolicyDto, WorkflowTerminalStatusDto,
            WorkflowValidationStatusDto,
        },
    };
    use crate::db::{
        configure_connection, database_path_for_project_in_app_data,
        migrations::migrations,
        project_store::{self, NewAgentDefinitionRecord},
    };
    use rusqlite::{params, Connection};
    use serde_json::json;
    use std::{fs, path::PathBuf};
    use tempfile::TempDir;

    fn linear_definition() -> WorkflowDefinitionDto {
        WorkflowDefinitionDto {
            schema: "xero.workflow_definition.v1".into(),
            id: "workflow-linear".into(),
            project_id: "project-1".into(),
            name: "Linear".into(),
            description: String::new(),
            version: 1,
            start_node_id: "agent-a".into(),
            nodes: vec![
                WorkflowNodeDto::Agent {
                    id: "agent-a".into(),
                    title: "Agent A".into(),
                    description: String::new(),
                    position: Default::default(),
                    agent_ref: AgentRefDto::BuiltIn {
                        runtime_agent_id: RuntimeAgentIdDto::Engineer,
                        version: 2,
                    },
                    display_label: None,
                    input_bindings: Vec::new(),
                    output_contract: WorkflowOutputContractDto::default(),
                    run_overrides: None,
                    resource_scopes: Vec::new(),
                    failure_policy: Default::default(),
                },
                WorkflowNodeDto::Agent {
                    id: "agent-b".into(),
                    title: "Agent B".into(),
                    description: String::new(),
                    position: Default::default(),
                    agent_ref: AgentRefDto::Custom {
                        definition_id: "custom-work".into(),
                        version: 1,
                    },
                    display_label: None,
                    input_bindings: vec![WorkflowInputBindingDto::Artifact {
                        name: "handoff".into(),
                        required: true,
                        artifact_ref: "agent-a.text_output".into(),
                        path: None,
                        prompt_label: None,
                    }],
                    output_contract: WorkflowOutputContractDto {
                        artifact_type: "implementation_summary".into(),
                        ..WorkflowOutputContractDto::default()
                    },
                    run_overrides: None,
                    resource_scopes: Vec::new(),
                    failure_policy: Default::default(),
                },
                WorkflowNodeDto::Terminal {
                    id: "done".into(),
                    title: "Done".into(),
                    description: String::new(),
                    position: Default::default(),
                    terminal_status: WorkflowTerminalStatusDto::Success,
                },
            ],
            edges: vec![
                WorkflowEdgeDto {
                    id: "edge-a-b".into(),
                    from_node_id: "agent-a".into(),
                    to_node_id: "agent-b".into(),
                    r#type: WorkflowEdgeTypeDto::Success,
                    label: String::new(),
                    priority: 10,
                    condition: WorkflowConditionDto::Always,
                    loop_policy: None,
                },
                WorkflowEdgeDto {
                    id: "edge-b-done".into(),
                    from_node_id: "agent-b".into(),
                    to_node_id: "done".into(),
                    r#type: WorkflowEdgeTypeDto::Success,
                    label: String::new(),
                    priority: 10,
                    condition: WorkflowConditionDto::Always,
                    loop_policy: None,
                },
            ],
            artifact_contracts: Vec::new(),
            subgraphs: Vec::new(),
            run_policy: WorkflowRunPolicyDto::default(),
            created_at: None,
            updated_at: None,
        }
    }

    fn repo_with_database(project_id: &str) -> (TempDir, PathBuf) {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let app_data_dir = repo_root.parent().expect("repo parent").join("app-data");
        let database_path = database_path_for_project_in_app_data(&app_data_dir, project_id);
        fs::create_dir_all(database_path.parent().expect("database parent"))
            .expect("create database dir");
        let mut connection = Connection::open(&database_path).expect("open project database");
        configure_connection(&connection).expect("configure project database");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate project database");
        connection
            .execute(
                "INSERT INTO projects (id, name, description, milestone) VALUES (?1, 'Project', '', '')",
                params![project_id],
            )
            .expect("insert project");
        connection
            .execute(
                r#"
                INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo)
                VALUES ('repo-1', ?1, ?2, 'Project', 'main', 'abc123', 1)
                "#,
                params![project_id, repo_root.to_string_lossy().as_ref()],
            )
            .expect("insert repository");
        crate::db::register_project_database_path_for_tests(&repo_root, database_path);
        (tempdir, repo_root)
    }

    fn valid_custom_definition(definition_id: &str, version: u32) -> NewAgentDefinitionRecord {
        NewAgentDefinitionRecord {
            definition_id: definition_id.into(),
            version,
            display_name: "Project Researcher".into(),
            short_label: "Research".into(),
            description: "Answer project questions using observe-only context.".into(),
            scope: "project_custom".into(),
            lifecycle_state: "active".into(),
            base_capability_profile: "observe_only".into(),
            snapshot: json!({
                "schema": "xero.agent_definition.v1",
                "schemaVersion": 3,
                "id": definition_id,
                "version": version,
                "displayName": "Project Researcher",
                "shortLabel": "Research",
                "description": "Answer project questions using observe-only context.",
                "taskPurpose": "Answer project questions using observe-only context.",
                "scope": "project_custom",
                "lifecycleState": "active",
                "baseCapabilityProfile": "observe_only",
                "defaultApprovalMode": "suggest",
                "allowedApprovalModes": ["suggest"],
                "toolPolicy": {
                    "allowedEffectClasses": ["observe"],
                    "allowedTools": ["project_context_search"],
                    "deniedTools": [],
                    "allowedToolGroups": ["project_context"],
                    "deniedToolGroups": []
                },
                "workflowContract": "Use reviewed project context to answer the user's question.",
                "finalResponseContract": "Return a concise answer with uncertainty called out.",
                "prompts": [{
                    "id": "project-researcher-intent",
                    "label": "Project Researcher Intent",
                    "role": "developer",
                    "source": "test",
                    "body": "Answer project questions using only observe-only context."
                }],
                "tools": [],
                "output": {
                    "contract": "answer",
                    "label": "Answer",
                    "description": "Answer the user's project question.",
                    "sections": [{
                        "id": "answer",
                        "label": "Answer",
                        "description": "Direct answer.",
                        "emphasis": "core",
                        "producedByTools": ["project_context_search"]
                    }]
                },
                "dbTouchpoints": {
                    "reads": [{
                        "table": "project_records",
                        "kind": "read",
                        "purpose": "Retrieve reviewed project context.",
                        "triggers": [],
                        "columns": ["text"]
                    }],
                    "writes": [],
                    "encouraged": []
                },
                "consumes": [],
                "projectDataPolicy": {
                    "recordKinds": ["artifact", "context_note"],
                    "structuredSchemas": [],
                    "unstructuredScopes": ["project"]
                },
                "memoryCandidatePolicy": {
                    "memoryKinds": ["project_fact"],
                    "reviewRequired": true
                },
                "retrievalDefaults": {
                    "enabled": true,
                    "limit": 4,
                    "recordKinds": ["artifact", "context_note"],
                    "memoryKinds": ["project_fact"]
                },
                "handoffPolicy": {
                    "enabled": true,
                    "routingMode": "same_agent",
                    "allowedTargets": [],
                    "preserveDefinitionVersion": true,
                    "carrySummary": true,
                    "includeDurableContext": true
                },
                "attachedSkills": []
            }),
            validation_report: Some(json!({
                "status": "valid",
                "source": "workflow_validator_test"
            })),
            created_at: "2026-05-01T12:00:00Z".into(),
            updated_at: "2026-05-01T12:00:00Z".into(),
        }
    }

    fn insert_custom_definition(repo_root: &std::path::Path, record: NewAgentDefinitionRecord) {
        project_store::insert_agent_definition(repo_root, &record).expect("insert custom agent");
    }

    fn set_second_agent_ref(definition: &mut WorkflowDefinitionDto, agent_ref: AgentRefDto) {
        let WorkflowNodeDto::Agent {
            agent_ref: existing,
            ..
        } = &mut definition.nodes[1]
        else {
            panic!("expected agent node");
        };
        *existing = agent_ref;
    }

    fn diagnostic_codes(report: &WorkflowValidationReportDto) -> Vec<&str> {
        report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect()
    }

    #[test]
    fn validator_accepts_linear_custom_agent_workflow() {
        let report = validate_workflow_definition(&linear_definition());

        assert_eq!(report.status, WorkflowValidationStatusDto::Valid);
    }

    #[test]
    fn validator_requires_an_always_fallback_for_conditional_routes() {
        let mut definition = linear_definition();
        definition.edges[0].condition = WorkflowConditionDto::NodeStatus {
            node_id: "agent-a".into(),
            status: crate::commands::contracts::workflows::WorkflowNodeRunStatusDto::Succeeded,
        };

        let report = validate_workflow_definition(&definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(diagnostic_codes(&report).contains(&"conditional_route_fallback_missing"));
    }

    #[test]
    fn validator_allows_global_and_status_specific_fallbacks_together() {
        let mut definition = linear_definition();
        definition.edges[0].condition = WorkflowConditionDto::NodeStatus {
            node_id: "agent-a".into(),
            status: crate::commands::contracts::workflows::WorkflowNodeRunStatusDto::Succeeded,
        };
        definition.edges.extend([
            WorkflowEdgeDto {
                id: "edge-a-success-fallback".into(),
                from_node_id: "agent-a".into(),
                to_node_id: "agent-b".into(),
                r#type: WorkflowEdgeTypeDto::Success,
                label: "success fallback".into(),
                priority: 80,
                condition: WorkflowConditionDto::Always,
                loop_policy: None,
            },
            WorkflowEdgeDto {
                id: "edge-a-global-fallback".into(),
                from_node_id: "agent-a".into(),
                to_node_id: "done".into(),
                r#type: WorkflowEdgeTypeDto::Conditional,
                label: "global fallback".into(),
                priority: 90,
                condition: WorkflowConditionDto::Always,
                loop_policy: None,
            },
        ]);

        let report = validate_workflow_definition(&definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Valid);
        assert!(!diagnostic_codes(&report).contains(&"duplicate_default_edge"));
    }

    #[test]
    fn validator_rejects_unresumable_needs_human_terminal_inside_subgraph() {
        let mut definition = linear_definition();
        definition.subgraphs.push(WorkflowSubgraphDto {
            id: "review-subgraph".into(),
            title: "Review".into(),
            description: String::new(),
            start_node_id: "needs-human".into(),
            nodes: vec![WorkflowNodeDto::Terminal {
                id: "needs-human".into(),
                title: "Needs human".into(),
                description: String::new(),
                position: Default::default(),
                terminal_status: WorkflowTerminalStatusDto::NeedsHuman,
            }],
            edges: Vec::new(),
            input_bindings: Vec::new(),
            output_contract: WorkflowOutputContractDto::default(),
        });

        let report = validate_workflow_definition(&definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(diagnostic_codes(&report).contains(&"subgraph_needs_human_terminal_unsupported"));
    }

    #[test]
    fn validator_enforces_the_app_owned_git_status_policy() {
        let command_node = |command: &str, args: Vec<&str>| {
            serde_json::from_value::<WorkflowNodeDto>(json!({
                "id": "agent-b",
                "type": "command",
                "title": "Repository status",
                "description": "",
                "position": { "x": 0, "y": 0 },
                "command": command,
                "args": args,
                "allowedCommands": [command],
                "timeoutSeconds": 30,
                "successExitCodes": [0],
                "outputContract": {
                    "artifactType": "implementation_summary",
                    "schemaVersion": 1,
                    "extraction": "generic_text",
                    "required": true
                },
                "parser": { "extraction": "generic_text" }
            }))
            .expect("command node")
        };

        let mut definition = linear_definition();
        definition.nodes[1] = command_node("git", vec!["status", "--short"]);
        assert_eq!(
            validate_workflow_definition(&definition).status,
            WorkflowValidationStatusDto::Valid
        );

        definition.nodes[1] = command_node("pnpm", vec!["test"]);
        let report = validate_workflow_definition(&definition);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "workflow_command_not_allowed_by_app_policy"
                && diagnostic.path == "nodes.1.command"
        }));

        definition.nodes[1] = command_node("git", vec!["push"]);
        let report = validate_workflow_definition(&definition);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "workflow_command_arguments_not_allowed_by_app_policy"
                && diagnostic.path == "nodes.1.args"
        }));
    }

    #[test]
    fn registry_validator_rejects_missing_custom_agent() {
        let (_tempdir, repo_root) = repo_with_database("project-1");

        let report = validate_workflow_definition_with_registry(&repo_root, &linear_definition());

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(diagnostic_codes(&report).contains(&"agent_ref_custom_definition_missing"));
        assert_eq!(report.diagnostics[0].path, "nodes[1].agentRef.definitionId");
    }

    #[test]
    fn registry_validator_rejects_inactive_custom_agent() {
        let (_tempdir, repo_root) = repo_with_database("project-1");
        let mut record = valid_custom_definition("custom-work", 1);
        record.lifecycle_state = "archived".into();
        record.snapshot["lifecycleState"] = json!("archived");
        insert_custom_definition(&repo_root, record);

        let report = validate_workflow_definition_with_registry(&repo_root, &linear_definition());

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(diagnostic_codes(&report).contains(&"agent_ref_custom_definition_inactive"));
    }

    #[test]
    fn registry_validator_rejects_missing_custom_version() {
        let (_tempdir, repo_root) = repo_with_database("project-1");
        insert_custom_definition(&repo_root, valid_custom_definition("custom-work", 2));

        let report = validate_workflow_definition_with_registry(&repo_root, &linear_definition());

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(diagnostic_codes(&report).contains(&"agent_ref_custom_version_missing"));
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path == "nodes[1].agentRef.version"));
    }

    #[test]
    fn registry_validator_accepts_stale_but_existing_pinned_custom_version() {
        let (_tempdir, repo_root) = repo_with_database("project-1");
        insert_custom_definition(&repo_root, valid_custom_definition("custom-work", 1));
        insert_custom_definition(&repo_root, valid_custom_definition("custom-work", 2));

        let report = validate_workflow_definition_with_registry(&repo_root, &linear_definition());

        assert_eq!(report.status, WorkflowValidationStatusDto::Valid);
    }

    #[test]
    fn registry_validator_accepts_valid_pinned_custom_version() {
        let (_tempdir, repo_root) = repo_with_database("project-1");
        insert_custom_definition(&repo_root, valid_custom_definition("custom-work", 1));

        let report = validate_workflow_definition_with_registry(&repo_root, &linear_definition());

        assert_eq!(report.status, WorkflowValidationStatusDto::Valid);
    }

    #[test]
    fn registry_validator_rejects_activation_invalid_custom_version() {
        let (_tempdir, repo_root) = repo_with_database("project-1");
        let mut record = valid_custom_definition("custom-work", 1);
        record.validation_report = Some(json!({
            "status": "invalid",
            "diagnostics": [{
                "severity": "error",
                "code": "test_invalid",
                "path": "toolPolicy",
                "message": "invalid for test"
            }]
        }));
        insert_custom_definition(&repo_root, record);

        let report = validate_workflow_definition_with_registry(&repo_root, &linear_definition());

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(diagnostic_codes(&report).contains(&"agent_ref_custom_activation_preflight_failed"));
    }

    #[test]
    fn registry_validator_rejects_invalid_builtin_version() {
        let (_tempdir, repo_root) = repo_with_database("project-1");
        let mut definition = linear_definition();
        set_second_agent_ref(
            &mut definition,
            AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                version: 999,
            },
        );

        let report = validate_workflow_definition_with_registry(&repo_root, &definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(diagnostic_codes(&report).contains(&"agent_ref_builtin_version_unsupported"));
    }

    #[test]
    fn registry_validator_accepts_valid_builtin_refs() {
        let (_tempdir, repo_root) = repo_with_database("project-1");
        let mut definition = linear_definition();
        set_second_agent_ref(
            &mut definition,
            AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                version: 2,
            },
        );

        let report = validate_workflow_definition_with_registry(&repo_root, &definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Valid);
    }

    #[test]
    fn validator_rejects_cycle_without_loop_policy() {
        let mut definition = linear_definition();
        definition.edges.push(WorkflowEdgeDto {
            id: "edge-b-a".into(),
            from_node_id: "agent-b".into(),
            to_node_id: "agent-a".into(),
            r#type: WorkflowEdgeTypeDto::Conditional,
            label: "retry".into(),
            priority: 20,
            condition: WorkflowConditionDto::Always,
            loop_policy: None,
        });

        let report = validate_workflow_definition(&definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "cycle_without_loop_policy"));
    }

    #[test]
    fn validator_accepts_bounded_loop_with_exhaustion_route() {
        let mut definition = linear_definition();
        definition.nodes.push(WorkflowNodeDto::HumanCheckpoint {
            id: "human".into(),
            title: "Human".into(),
            description: String::new(),
            position: Default::default(),
            checkpoint_type:
                crate::commands::contracts::workflows::WorkflowHumanCheckpointTypeDto::Decision,
            prompt: "Choose a route.".into(),
            decision_options: vec!["retry".into(), "stop".into()],
            resume_payload_schema: None,
            state_updates: Vec::new(),
        });
        definition.edges.push(WorkflowEdgeDto {
            id: "edge-b-a".into(),
            from_node_id: "agent-b".into(),
            to_node_id: "agent-a".into(),
            r#type: WorkflowEdgeTypeDto::Loop,
            label: "retry".into(),
            priority: 20,
            condition: WorkflowConditionDto::LoopAttemptLt {
                loop_key: "retry".into(),
                value: 2,
            },
            loop_policy: Some(
                crate::commands::contracts::workflows::WorkflowLoopPolicyDto {
                    loop_key: "retry".into(),
                    max_attempts: 2,
                    attempt_scope: Default::default(),
                    carryover_policy: Default::default(),
                    selected_artifact_refs: Vec::new(),
                    reset_policy: Default::default(),
                    stall_detector: None,
                    on_exhausted: "human".into(),
                },
            ),
        });
        definition.edges.push(WorkflowEdgeDto {
            id: "edge-b-human-fallback".into(),
            from_node_id: "agent-b".into(),
            to_node_id: "human".into(),
            r#type: WorkflowEdgeTypeDto::Conditional,
            label: "fallback".into(),
            priority: 90,
            condition: WorkflowConditionDto::Always,
            loop_policy: None,
        });

        let report = validate_workflow_definition(&definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Valid);
    }

    #[test]
    fn validator_applies_full_semantics_inside_subgraphs() {
        let mut definition = linear_definition();
        definition.subgraphs.push(
            serde_json::from_value(json!({
                "id": "local-flow",
                "title": "Local flow",
                "description": "",
                "startNodeId": "source",
                "nodes": [
                    {
                        "id": "source",
                        "type": "agent",
                        "title": "Source",
                        "description": "",
                        "position": { "x": 0, "y": 0 },
                        "agentRef": {
                            "kind": "built_in",
                            "runtimeAgentId": "engineer",
                            "version": 2
                        },
                        "inputBindings": [],
                        "outputContract": {
                            "artifactType": "text_output",
                            "schemaVersion": 1,
                            "extraction": "generic_text",
                            "required": true
                        }
                    },
                    {
                        "id": "sink",
                        "type": "agent",
                        "title": "Sink",
                        "description": "",
                        "position": { "x": 0, "y": 0 },
                        "agentRef": {
                            "kind": "built_in",
                            "runtimeAgentId": "engineer",
                            "version": 2
                        },
                        "inputBindings": [{
                            "source": "artifact",
                            "name": "missing",
                            "required": true,
                            "artifactRef": "ghost.text_output"
                        }],
                        "outputContract": {
                            "artifactType": "text_output",
                            "schemaVersion": 1,
                            "extraction": "generic_text",
                            "required": true
                        }
                    },
                    {
                        "id": "state",
                        "type": "state_read",
                        "title": "Read state",
                        "description": "",
                        "position": { "x": 0, "y": 0 },
                        "query": {
                            "entityType": "milestone",
                            "filters": [{
                                "path": "invalid",
                                "operator": "eq",
                                "value": "open",
                                "values": []
                            }],
                            "includeArchived": false
                        },
                        "outputArtifactType": "state_result"
                    },
                    {
                        "id": "command",
                        "type": "command",
                        "title": "Command",
                        "description": "",
                        "position": { "x": 0, "y": 0 },
                        "command": "pnpm",
                        "args": [],
                        "allowedCommands": ["npm"],
                        "timeoutSeconds": 30,
                        "successExitCodes": [0],
                        "outputContract": {
                            "artifactType": "command_result",
                            "schemaVersion": 1,
                            "extraction": "generic_text",
                            "required": true
                        },
                        "parser": { "extraction": "generic_text" }
                    }
                ],
                "edges": [
                    {
                        "id": "source-to-sink",
                        "fromNodeId": "source",
                        "toNodeId": "sink",
                        "type": "success",
                        "priority": 10,
                        "condition": {
                            "kind": "node_status",
                            "nodeId": "ghost",
                            "status": "succeeded"
                        }
                    },
                    {
                        "id": "sink-to-source",
                        "fromNodeId": "sink",
                        "toNodeId": "source",
                        "type": "loop",
                        "priority": 20,
                        "condition": {
                            "kind": "loop_attempt_lt",
                            "loopKey": "missing-loop",
                            "value": 2
                        },
                        "loopPolicy": {
                            "loopKey": "local-loop",
                            "maxAttempts": 2,
                            "attemptScope": "run",
                            "carryoverPolicy": "selected",
                            "selectedArtifactRefs": ["ghost.text_output"],
                            "resetPolicy": "never",
                            "onExhausted": "ghost"
                        }
                    }
                ],
                "inputBindings": [],
                "outputContract": {
                    "artifactType": "subgraph_result",
                    "schemaVersion": 1,
                    "extraction": "generic_text",
                    "required": true
                }
            }))
            .expect("subgraph fixture"),
        );

        let report = validate_workflow_definition(&definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        for (code, path) in [
            (
                "artifact_ref_missing",
                "subgraphs.0.nodes.1.inputBindings.0.artifactRef",
            ),
            (
                "state_query_filter_path_invalid",
                "subgraphs.0.nodes.2.query.filters.0.path",
            ),
            (
                "command_not_in_allowlist",
                "subgraphs.0.nodes.3.allowedCommands",
            ),
            (
                "condition_node_ref_missing",
                "subgraphs.0.edges.0.condition",
            ),
            (
                "condition_loop_key_missing",
                "subgraphs.0.edges.1.condition",
            ),
            (
                "loop_exhaustion_target_missing",
                "subgraphs.0.edges.1.loopPolicy.onExhausted",
            ),
            (
                "loop_artifact_ref_missing",
                "subgraphs.0.edges.1.loopPolicy.selectedArtifactRefs.0",
            ),
        ] {
            assert!(report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code && diagnostic.path == path));
        }
    }

    #[test]
    fn validator_accepts_acyclic_nested_subgraphs_and_rejects_recursion() {
        let mut definition = linear_definition();
        definition.subgraphs = serde_json::from_value(json!([
            {
                "id": "inner",
                "title": "Inner",
                "description": "",
                "startNodeId": "done",
                "nodes": [{
                    "id": "done",
                    "type": "terminal",
                    "title": "Done",
                    "description": "",
                    "position": { "x": 0, "y": 0 },
                    "terminalStatus": "success"
                }],
                "edges": [],
                "inputBindings": [],
                "outputContract": {
                    "artifactType": "subgraph_result",
                    "schemaVersion": 1,
                    "extraction": "generic_text",
                    "required": true
                }
            },
            {
                "id": "outer",
                "title": "Outer",
                "description": "",
                "startNodeId": "invoke-inner",
                "nodes": [
                    {
                        "id": "invoke-inner",
                        "type": "subgraph",
                        "title": "Invoke inner",
                        "description": "",
                        "position": { "x": 0, "y": 0 },
                        "subgraphId": "inner",
                        "inputBindings": [],
                        "outputContract": {
                            "artifactType": "subgraph_result",
                            "schemaVersion": 1,
                            "extraction": "generic_text",
                            "required": true
                        }
                    },
                    {
                        "id": "outer-done",
                        "type": "terminal",
                        "title": "Done",
                        "description": "",
                        "position": { "x": 0, "y": 0 },
                        "terminalStatus": "success"
                    }
                ],
                "edges": [{
                    "id": "inner-to-done",
                    "fromNodeId": "invoke-inner",
                    "toNodeId": "outer-done",
                    "type": "success",
                    "priority": 10,
                    "condition": { "kind": "always" }
                }],
                "inputBindings": [],
                "outputContract": {
                    "artifactType": "subgraph_result",
                    "schemaVersion": 1,
                    "extraction": "generic_text",
                    "required": true
                }
            }
        ]))
        .expect("subgraph fixtures");

        assert_eq!(
            validate_workflow_definition(&definition).status,
            WorkflowValidationStatusDto::Valid
        );

        let WorkflowNodeDto::Subgraph { subgraph_id, .. } = &mut definition.subgraphs[1].nodes[0]
        else {
            panic!("expected subgraph node");
        };
        *subgraph_id = "outer".into();
        let report = validate_workflow_definition(&definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "recursive_subgraph_invocation"
                && diagnostic.path == "subgraphs.1.nodes.0.subgraphId"
        }));
    }

    #[test]
    fn registry_validator_checks_agent_refs_inside_subgraphs() {
        let (_tempdir, repo_root) = repo_with_database("project-1");
        let mut definition = linear_definition();
        set_second_agent_ref(
            &mut definition,
            AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                version: 2,
            },
        );
        definition.subgraphs.push(
            serde_json::from_value(json!({
                "id": "local-flow",
                "title": "Local flow",
                "description": "",
                "startNodeId": "local-agent",
                "nodes": [{
                    "id": "local-agent",
                    "type": "agent",
                    "title": "Local agent",
                    "description": "",
                    "position": { "x": 0, "y": 0 },
                    "agentRef": {
                        "kind": "custom",
                        "definitionId": "missing-local-agent",
                        "version": 1
                    },
                    "inputBindings": [],
                    "outputContract": {
                        "artifactType": "text_output",
                        "schemaVersion": 1,
                        "extraction": "generic_text",
                        "required": true
                    }
                }],
                "edges": [],
                "inputBindings": [],
                "outputContract": {
                    "artifactType": "subgraph_result",
                    "schemaVersion": 1,
                    "extraction": "generic_text",
                    "required": true
                }
            }))
            .expect("subgraph fixture"),
        );

        let report = validate_workflow_definition_with_registry(&repo_root, &definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "agent_ref_custom_definition_missing"
                && diagnostic.path == "subgraphs[0].nodes[0].agentRef.definitionId"
        }));
    }

    #[test]
    fn validator_rejects_condition_path_outside_artifact_schema() {
        let mut definition = linear_definition();
        if let WorkflowNodeDto::Agent {
            output_contract, ..
        } = &mut definition.nodes[0]
        {
            output_contract.artifact_type = "verification_result".into();
            output_contract.extraction = WorkflowOutputExtractionDto::JsonObject;
        }
        if let WorkflowNodeDto::Agent { input_bindings, .. } = &mut definition.nodes[1] {
            *input_bindings = vec![WorkflowInputBindingDto::Artifact {
                name: "handoff".into(),
                required: true,
                artifact_ref: "agent-a.verification_result".into(),
                path: None,
                prompt_label: None,
            }];
        }
        definition
            .artifact_contracts
            .push(WorkflowArtifactContractDto {
                artifact_type: "verification_result".into(),
                schema_version: 1,
                json_schema: Some(json!({
                    "type": "object",
                    "required": ["status"],
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["passed", "gaps_found", "human_needed"]
                        }
                    },
                    "additionalProperties": false
                })),
                display_name: "Verification result".into(),
                description: String::new(),
            });
        definition.edges[0].condition = WorkflowConditionDto::ArtifactFieldEquals {
            artifact_ref: "agent-a.verification_result".into(),
            path: "$.missing".into(),
            value: json!("passed"),
        };

        let report = validate_workflow_definition(&definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Invalid);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "condition_artifact_path_not_in_schema"));
    }

    #[test]
    fn validator_accepts_condition_path_declared_by_artifact_schema() {
        let mut definition = linear_definition();
        if let WorkflowNodeDto::Agent {
            output_contract, ..
        } = &mut definition.nodes[0]
        {
            output_contract.artifact_type = "verification_result".into();
            output_contract.extraction = WorkflowOutputExtractionDto::JsonObject;
        }
        if let WorkflowNodeDto::Agent { input_bindings, .. } = &mut definition.nodes[1] {
            *input_bindings = vec![WorkflowInputBindingDto::Artifact {
                name: "handoff".into(),
                required: true,
                artifact_ref: "agent-a.verification_result".into(),
                path: None,
                prompt_label: None,
            }];
        }
        definition
            .artifact_contracts
            .push(WorkflowArtifactContractDto {
                artifact_type: "verification_result".into(),
                schema_version: 1,
                json_schema: Some(json!({
                    "type": "object",
                    "required": ["status"],
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["passed", "gaps_found", "human_needed"]
                        }
                    },
                    "additionalProperties": false
                })),
                display_name: "Verification result".into(),
                description: String::new(),
            });
        definition.edges[0].condition = WorkflowConditionDto::ArtifactFieldEquals {
            artifact_ref: "agent-a.verification_result".into(),
            path: "$.status".into(),
            value: json!("passed"),
        };
        definition.edges.push(WorkflowEdgeDto {
            id: "edge-a-safe-fallback".into(),
            from_node_id: "agent-a".into(),
            to_node_id: "done".into(),
            r#type: WorkflowEdgeTypeDto::Success,
            label: "fallback".into(),
            priority: 90,
            condition: WorkflowConditionDto::Always,
            loop_policy: None,
        });

        let report = validate_workflow_definition(&definition);

        assert_eq!(report.status, WorkflowValidationStatusDto::Valid);
    }

    #[test]
    fn validator_rejects_collection_resume_paths_the_runtime_cannot_write() {
        for path in ["$", "$.from", "$.phase.from", "  $.phase.from  "] {
            assert!(workflow_resume_control_input_path_valid(path), "{path}");
        }
        for path in ["from", "$.", "$.phase..from", "$.phase[0]", "$.phase from"] {
            assert!(!workflow_resume_control_input_path_valid(path), "{path}");
        }

        let mut definition = linear_definition();
        definition.nodes.push(
            serde_json::from_value(json!({
                "id": "invalid-loop",
                "type": "collection_loop",
                "title": "Invalid loop",
                "description": "",
                "position": { "x": 0, "y": 0 },
                "collection": {
                    "entityType": "delivery_phase",
                    "filters": [],
                    "includeArchived": false
                },
                "itemArtifactType": "collection_item",
                "itemVariableName": "item",
                "sortKey": "$.sortOrder",
                "afterItemRequery": true,
                "maxItemCount": 10,
                "controls": { "fromInputPath": "$.phases[0]" }
            }))
            .expect("collection loop fixture"),
        );

        let report = validate_workflow_definition(&definition);

        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "collection_loop_control_input_path_invalid"
                && diagnostic.path == "nodes.3.controls.fromInputPath"
        }));
    }
}
