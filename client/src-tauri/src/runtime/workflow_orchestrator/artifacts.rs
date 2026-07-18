use std::collections::BTreeMap;

use serde_json::{json, Value as JsonValue};

use crate::{
    commands::{
        contracts::workflows::{
            WorkflowArtifactRecordDto, WorkflowInputBindingDto, WorkflowOutputContractDto,
            WorkflowOutputExtractionDto,
        },
        CommandError,
    },
    db::project_store::{AgentMessageRole, AgentRunSnapshotRecord},
};

use super::condition_eval::{json_path_lookup, lookup_run_input_binding};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowArtifactValidationDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

pub fn final_assistant_text(snapshot: &AgentRunSnapshotRecord) -> Option<String> {
    snapshot
        .messages
        .iter()
        .rev()
        .find(|message| {
            message.role == AgentMessageRole::Assistant && !message.content.trim().is_empty()
        })
        .map(|message| message.content.trim().to_string())
}

pub fn extract_workflow_artifact_payload(
    contract: &WorkflowOutputContractDto,
    json_schema: Option<&JsonValue>,
    final_text: &str,
) -> Result<
    (
        JsonValue,
        Option<String>,
        Vec<WorkflowArtifactValidationDiagnostic>,
    ),
    CommandError,
> {
    match contract.extraction {
        WorkflowOutputExtractionDto::GenericText => Ok((
            json!({ "text": final_text }),
            Some(final_text.to_string()),
            Vec::new(),
        )),
        WorkflowOutputExtractionDto::JsonObject => {
            let value = parse_json_output(final_text)?;
            if !value.is_object() {
                return Err(CommandError::user_fixable(
                    "workflow_artifact_extraction_failed",
                    "Xero expected the agent output to be a JSON object for this typed artifact.",
                ));
            }
            let diagnostics = validate_json_schema(&value, json_schema)?;
            let render_text = render_text_for_payload(&value, contract.render_text_path.as_deref());
            validate_render_text_path(&value, contract.render_text_path.as_deref())?;
            Ok((value, render_text, diagnostics))
        }
        WorkflowOutputExtractionDto::JsonArray => {
            let value = parse_json_output(final_text)?;
            if !value.is_array() {
                return Err(CommandError::user_fixable(
                    "workflow_artifact_extraction_failed",
                    "Xero expected the agent output to be a JSON array for this typed artifact.",
                ));
            }
            let diagnostics = validate_json_schema(&value, json_schema)?;
            let render_text = render_text_for_payload(&value, contract.render_text_path.as_deref());
            validate_render_text_path(&value, contract.render_text_path.as_deref())?;
            Ok((value, render_text, diagnostics))
        }
    }
}

pub fn validate_workflow_artifact_payload(
    contract: &WorkflowOutputContractDto,
    json_schema: Option<&JsonValue>,
    payload: &JsonValue,
) -> Result<(Option<String>, Vec<WorkflowArtifactValidationDiagnostic>), CommandError> {
    match contract.extraction {
        WorkflowOutputExtractionDto::GenericText => {}
        WorkflowOutputExtractionDto::JsonObject if !payload.is_object() => {
            return Err(CommandError::user_fixable(
                "workflow_artifact_schema_invalid",
                format!(
                    "Xero expected `{}` to be a JSON object.",
                    contract.artifact_type
                ),
            ));
        }
        WorkflowOutputExtractionDto::JsonArray if !payload.is_array() => {
            return Err(CommandError::user_fixable(
                "workflow_artifact_schema_invalid",
                format!(
                    "Xero expected `{}` to be a JSON array.",
                    contract.artifact_type
                ),
            ));
        }
        WorkflowOutputExtractionDto::JsonObject | WorkflowOutputExtractionDto::JsonArray => {}
    }

    let diagnostics = validate_json_schema(payload, json_schema)?;
    let render_text = render_text_for_payload(payload, contract.render_text_path.as_deref());
    validate_render_text_path(payload, contract.render_text_path.as_deref())?;
    Ok((render_text, diagnostics))
}

#[allow(clippy::too_many_arguments)]
pub fn build_agent_node_prompt(
    workflow_name: &str,
    node_title: &str,
    prompt_preface: Option<&str>,
    output_contract: &WorkflowOutputContractDto,
    json_schema: Option<&JsonValue>,
    initial_input: Option<&JsonValue>,
    input_bindings: &[WorkflowInputBindingDto],
    artifacts: &[WorkflowArtifactRecordDto],
) -> Result<String, CommandError> {
    let mut lines = Vec::new();
    if let Some(preface) = prompt_preface
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(preface.to_string());
        lines.push(String::new());
    }
    lines.push(format!("Workflow: {workflow_name}"));
    lines.push(format!("Current node: {node_title}"));
    lines.push(String::new());
    lines.push("Use the Workflow inputs below as the contract for this handoff.".into());

    let artifact_index = artifact_index(artifacts);
    for binding in input_bindings {
        let (name, required, label, value) = match binding {
            WorkflowInputBindingDto::RunInput {
                name,
                required,
                path,
                prompt_label,
            } => {
                let value = lookup_run_input_binding(initial_input, name, path.as_deref()).cloned();
                (
                    name,
                    *required,
                    prompt_label.as_deref().unwrap_or(name),
                    value,
                )
            }
            WorkflowInputBindingDto::Artifact {
                name,
                required,
                artifact_ref,
                path,
                prompt_label,
            } => {
                let value =
                    artifact_index
                        .get(artifact_ref)
                        .and_then(|artifact| match path.as_deref() {
                            Some(path) => json_path_lookup(&artifact.payload, path).cloned(),
                            None => Some(artifact.payload.clone()),
                        });
                (
                    name,
                    *required,
                    prompt_label.as_deref().unwrap_or(name),
                    value,
                )
            }
            WorkflowInputBindingDto::State {
                name,
                required,
                state_ref,
                path,
                prompt_label,
            } => {
                let value =
                    artifact_index
                        .get(state_ref)
                        .and_then(|artifact| match path.as_deref() {
                            Some(path) => json_path_lookup(&artifact.payload, path).cloned(),
                            None => Some(artifact.payload.clone()),
                        });
                (
                    name,
                    *required,
                    prompt_label.as_deref().unwrap_or(name),
                    value,
                )
            }
        };
        let Some(value) = value else {
            if required {
                return Err(CommandError::user_fixable(
                    "workflow_required_input_missing",
                    format!("Workflow node `{node_title}` cannot start because input `{name}` is missing."),
                ));
            }
            continue;
        };
        lines.push(String::new());
        lines.push(format!("## {label}"));
        lines.push(render_binding_value(&value));
    }

    if input_bindings.is_empty() {
        if let Some(input) = initial_input {
            lines.push(String::new());
            lines.push("## Workflow input".into());
            lines.push(render_binding_value(input));
        }
    }

    lines.push(String::new());
    lines.push("## Final response contract".into());
    lines.push(format!(
        "Return exactly one `{}` artifact, schema version {}.",
        output_contract.artifact_type, output_contract.schema_version
    ));
    match output_contract.extraction {
        WorkflowOutputExtractionDto::GenericText => {
            lines.push("Respond with the final user-facing text only.".into());
        }
        WorkflowOutputExtractionDto::JsonObject => {
            lines.push("Respond with a single JSON object and no prose outside the JSON.".into());
        }
        WorkflowOutputExtractionDto::JsonArray => {
            lines.push("Respond with a single JSON array and no prose outside the JSON.".into());
        }
    }
    if let Some(schema) = json_schema {
        lines.push("The JSON must satisfy this JSON Schema:".into());
        lines.push(render_binding_value(schema));
    }
    if let Some(render_text_path) = output_contract.render_text_path.as_deref() {
        lines.push(format!(
            "The render text path `{render_text_path}` must exist when the artifact is JSON."
        ));
    }

    Ok(lines.join("\n"))
}

pub fn artifact_ref_for_record(
    node_id_by_run_id: &BTreeMap<String, String>,
    artifact: &WorkflowArtifactRecordDto,
) -> Option<String> {
    node_id_by_run_id
        .get(&artifact.producer_node_run_id)
        .map(|node_id| format!("{node_id}.{}", artifact.artifact_type))
}

pub fn render_text_for_payload(
    payload: &JsonValue,
    render_text_path: Option<&str>,
) -> Option<String> {
    render_text_path
        .and_then(|path| json_path_lookup(payload, path))
        .and_then(|value| match value {
            JsonValue::String(text) => Some(text.clone()),
            value if value.is_null() => None,
            value => serde_json::to_string_pretty(value).ok(),
        })
}

fn artifact_index(
    artifacts: &[WorkflowArtifactRecordDto],
) -> BTreeMap<String, &WorkflowArtifactRecordDto> {
    let mut index = BTreeMap::new();
    for artifact in artifacts {
        if let Some(node_id) = node_id_from_node_run_id(&artifact.producer_node_run_id) {
            let artifact_ref = format!("{node_id}.{}", artifact.artifact_type);
            index.insert(artifact_ref, artifact);
        }
    }
    index
}

fn node_id_from_node_run_id(node_run_id: &str) -> Option<&str> {
    let after_node = node_run_id.split(":node:").nth(1)?;
    after_node.split(":attempt:").next()
}

fn render_binding_value(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => text.clone(),
        _ => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
    }
}

fn parse_json_output(text: &str) -> Result<JsonValue, CommandError> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<JsonValue>(trimmed) {
        return Ok(value);
    }
    if let Some(fenced) = extract_fenced_json(trimmed) {
        if let Ok(value) = serde_json::from_str::<JsonValue>(fenced.trim()) {
            return Ok(value);
        }
    }
    Err(CommandError::user_fixable(
        "workflow_artifact_extraction_failed",
        "Xero could not extract valid JSON from the agent output.",
    ))
}

fn validate_render_text_path(
    payload: &JsonValue,
    render_text_path: Option<&str>,
) -> Result<(), CommandError> {
    let Some(path) = render_text_path else {
        return Ok(());
    };
    if json_path_lookup(payload, path).is_some() {
        return Ok(());
    }
    Err(CommandError::user_fixable(
        "workflow_artifact_extraction_failed",
        format!(
            "Xero could not render the typed artifact because render path `{path}` was missing."
        ),
    ))
}

fn validate_json_schema(
    value: &JsonValue,
    schema: Option<&JsonValue>,
) -> Result<Vec<WorkflowArtifactValidationDiagnostic>, CommandError> {
    let Some(schema) = schema else {
        return Ok(Vec::new());
    };
    let mut diagnostics = Vec::new();
    validate_schema_value(value, schema, "$", &mut diagnostics);
    if diagnostics.is_empty() {
        Ok(diagnostics)
    } else {
        Err(CommandError::user_fixable(
            "workflow_artifact_schema_invalid",
            format!(
                "Xero rejected the typed artifact because it failed JSON Schema validation: {}",
                diagnostics
                    .iter()
                    .take(3)
                    .map(|diagnostic| format!("{} {}", diagnostic.path, diagnostic.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ))
    }
}

fn validate_schema_value(
    value: &JsonValue,
    schema: &JsonValue,
    path: &str,
    diagnostics: &mut Vec<WorkflowArtifactValidationDiagnostic>,
) {
    let Some(schema_object) = schema.as_object() else {
        return;
    };

    if let Some(enum_values) = schema_object.get("enum").and_then(JsonValue::as_array) {
        if !enum_values.iter().any(|candidate| candidate == value) {
            diagnostics.push(schema_error(
                "schema_enum_mismatch",
                path,
                "must be one of the allowed values",
            ));
            return;
        }
    }

    if let Some(schema_type) = schema_object.get("type") {
        if !schema_type_matches(value, schema_type) {
            diagnostics.push(schema_error(
                "schema_type_mismatch",
                path,
                format!("must be {}", schema_type_label(schema_type)),
            ));
            return;
        }
    }

    if let Some(min_length) = schema_object.get("minLength").and_then(JsonValue::as_u64) {
        if value.as_str().map(|text| text.chars().count()).unwrap_or(0) < min_length as usize {
            diagnostics.push(schema_error(
                "schema_min_length",
                path,
                format!("must contain at least {min_length} characters"),
            ));
        }
    }

    if let Some(min_items) = schema_object.get("minItems").and_then(JsonValue::as_u64) {
        if value.as_array().map(Vec::len).unwrap_or(0) < min_items as usize {
            diagnostics.push(schema_error(
                "schema_min_items",
                path,
                format!("must contain at least {min_items} items"),
            ));
        }
    }

    if let Some(required) = schema_object.get("required").and_then(JsonValue::as_array) {
        if let Some(object) = value.as_object() {
            for field in required.iter().filter_map(JsonValue::as_str) {
                if !object.contains_key(field) {
                    diagnostics.push(schema_error(
                        "schema_required_missing",
                        format!("{path}.{field}"),
                        "is required",
                    ));
                }
            }
        }
    }

    if let (Some(properties), Some(object)) = (
        schema_object
            .get("properties")
            .and_then(JsonValue::as_object),
        value.as_object(),
    ) {
        for (field, field_schema) in properties {
            if let Some(field_value) = object.get(field) {
                validate_schema_value(
                    field_value,
                    field_schema,
                    &format!("{path}.{field}"),
                    diagnostics,
                );
            }
        }
        if schema_object
            .get("additionalProperties")
            .and_then(JsonValue::as_bool)
            == Some(false)
        {
            for field in object.keys() {
                if !properties.contains_key(field) {
                    diagnostics.push(schema_error(
                        "schema_additional_property",
                        format!("{path}.{field}"),
                        "is not allowed by this artifact contract",
                    ));
                }
            }
        }
    }

    if let (Some(items_schema), Some(items)) = (schema_object.get("items"), value.as_array()) {
        for (index, item) in items.iter().enumerate() {
            validate_schema_value(item, items_schema, &format!("{path}[{index}]"), diagnostics);
        }
    }
}

fn schema_type_matches(value: &JsonValue, schema_type: &JsonValue) -> bool {
    if let Some(type_name) = schema_type.as_str() {
        return single_schema_type_matches(value, type_name);
    }
    schema_type
        .as_array()
        .map(|types| {
            types
                .iter()
                .filter_map(JsonValue::as_str)
                .any(|type_name| single_schema_type_matches(value, type_name))
        })
        .unwrap_or(true)
}

fn single_schema_type_matches(value: &JsonValue, type_name: &str) -> bool {
    match type_name {
        "array" => value.is_array(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "object" => value.is_object(),
        "string" => value.is_string(),
        _ => true,
    }
}

fn schema_type_label(schema_type: &JsonValue) -> String {
    schema_type
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| {
            schema_type.as_array().map(|types| {
                types
                    .iter()
                    .filter_map(JsonValue::as_str)
                    .collect::<Vec<_>>()
                    .join(" or ")
            })
        })
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| "the declared type".into())
}

fn schema_error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> WorkflowArtifactValidationDiagnostic {
    WorkflowArtifactValidationDiagnostic {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn extract_fenced_json(text: &str) -> Option<&str> {
    let start = text.find("```")?;
    let after_open = &text[start + 3..];
    let content_start = after_open.find('\n').map(|index| index + 1).unwrap_or(0);
    let content = &after_open[content_start..];
    let end = content.find("```")?;
    Some(&content[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::contracts::workflows::{
        WorkflowOutputContractDto, WorkflowOutputExtractionDto,
    };

    fn artifact(
        node_id: &str,
        artifact_type: &str,
        payload: JsonValue,
    ) -> WorkflowArtifactRecordDto {
        WorkflowArtifactRecordDto {
            id: format!("artifact-{node_id}-{artifact_type}"),
            workflow_run_id: "workflow-run".into(),
            producer_node_run_id: format!("workflow-run:node:{node_id}:attempt:1"),
            artifact_type: artifact_type.into(),
            schema_version: 1,
            payload,
            render_text: None,
            created_at: "2026-07-17T00:00:00Z".into(),
        }
    }

    #[test]
    fn artifact_extraction_accepts_generic_text() {
        let (payload, render_text, diagnostics) =
            extract_workflow_artifact_payload(&WorkflowOutputContractDto::default(), None, "done")
                .expect("extract generic text");

        assert_eq!(payload, json!({ "text": "done" }));
        assert_eq!(render_text.as_deref(), Some("done"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn artifact_extraction_accepts_fenced_json_object() {
        let contract = WorkflowOutputContractDto {
            extraction: WorkflowOutputExtractionDto::JsonObject,
            render_text_path: Some("$.summary".into()),
            ..WorkflowOutputContractDto::default()
        };

        let (payload, render_text, diagnostics) = extract_workflow_artifact_payload(
            &contract,
            None,
            "```json\n{\"summary\":\"ok\"}\n```",
        )
        .expect("extract JSON object");

        assert_eq!(payload, json!({ "summary": "ok" }));
        assert_eq!(render_text.as_deref(), Some("ok"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn artifact_extraction_rejects_wrong_json_schema_shape() {
        let contract = WorkflowOutputContractDto {
            extraction: WorkflowOutputExtractionDto::JsonObject,
            ..WorkflowOutputContractDto::default()
        };
        let schema = json!({
            "type": "object",
            "required": ["status"],
            "properties": {
                "status": { "type": "string", "enum": ["passed", "gaps_found", "human_needed"] }
            },
            "additionalProperties": false
        });

        let error =
            extract_workflow_artifact_payload(&contract, Some(&schema), r#"{"status":"maybe"}"#)
                .expect_err("invalid status should fail schema validation");

        assert_eq!(error.code, "workflow_artifact_schema_invalid");
        assert!(error.message.contains("$.status"));
    }

    #[test]
    fn artifact_extraction_rejects_missing_render_path() {
        let contract = WorkflowOutputContractDto {
            extraction: WorkflowOutputExtractionDto::JsonObject,
            render_text_path: Some("$.summary".into()),
            ..WorkflowOutputContractDto::default()
        };

        let error = extract_workflow_artifact_payload(&contract, None, r#"{"status":"passed"}"#)
            .expect_err("missing render path should fail");

        assert_eq!(error.code, "workflow_artifact_extraction_failed");
        assert!(error.message.contains("$.summary"));
    }

    #[test]
    fn agent_prompt_resolves_omitted_run_input_path_from_binding_name() {
        let prompt = build_agent_node_prompt(
            "Release",
            "Plan",
            None,
            &WorkflowOutputContractDto::default(),
            None,
            Some(&json!({
                "goal": "Ship it",
                "internal": "must not leak into this binding"
            })),
            &[WorkflowInputBindingDto::RunInput {
                name: "goal".into(),
                required: true,
                path: None,
                prompt_label: Some("Goal".into()),
            }],
            &[],
        )
        .expect("build agent prompt");

        assert_eq!(
            prompt,
            "Workflow: Release\nCurrent node: Plan\n\nUse the Workflow inputs below as the contract for this handoff.\n\n## Goal\nShip it\n\n## Final response contract\nReturn exactly one `text_output` artifact, schema version 1.\nRespond with the final user-facing text only."
        );
    }

    #[test]
    fn artifact_extraction_covers_array_shape_and_invalid_json_errors() {
        let array_contract = WorkflowOutputContractDto {
            extraction: WorkflowOutputExtractionDto::JsonArray,
            render_text_path: Some("$[0]".into()),
            ..WorkflowOutputContractDto::default()
        };
        let (payload, render_text, diagnostics) = extract_workflow_artifact_payload(
            &array_contract,
            Some(&json!({ "type": "array", "minItems": 1, "items": { "type": "string" } })),
            "[\"first\"]",
        )
        .expect("extract JSON array");
        assert_eq!(payload, json!(["first"]));
        assert_eq!(render_text.as_deref(), Some("first"));
        assert!(diagnostics.is_empty());

        let object_contract = WorkflowOutputContractDto {
            extraction: WorkflowOutputExtractionDto::JsonObject,
            ..WorkflowOutputContractDto::default()
        };
        for (contract, output) in [(&object_contract, "[]"), (&array_contract, "{}")] {
            assert_eq!(
                extract_workflow_artifact_payload(contract, None, output)
                    .expect_err("wrong JSON shape must fail")
                    .code,
                "workflow_artifact_extraction_failed"
            );
        }
        assert_eq!(
            extract_workflow_artifact_payload(&object_contract, None, "not json")
                .expect_err("invalid JSON must fail")
                .code,
            "workflow_artifact_extraction_failed"
        );
        assert_eq!(
            extract_workflow_artifact_payload(&object_contract, None, "```json\n{invalid}\n```",)
                .expect_err("invalid fenced JSON must fail")
                .code,
            "workflow_artifact_extraction_failed"
        );
    }

    #[test]
    fn artifact_payload_validation_enforces_shapes_and_render_paths() {
        let object_contract = WorkflowOutputContractDto {
            extraction: WorkflowOutputExtractionDto::JsonObject,
            render_text_path: Some("$.summary".into()),
            ..WorkflowOutputContractDto::default()
        };
        assert_eq!(
            validate_workflow_artifact_payload(&object_contract, None, &json!([]))
                .expect_err("object contract rejects arrays")
                .code,
            "workflow_artifact_schema_invalid"
        );

        let array_contract = WorkflowOutputContractDto {
            extraction: WorkflowOutputExtractionDto::JsonArray,
            ..WorkflowOutputContractDto::default()
        };
        assert_eq!(
            validate_workflow_artifact_payload(&array_contract, None, &json!({}))
                .expect_err("array contract rejects objects")
                .code,
            "workflow_artifact_schema_invalid"
        );

        let (render_text, diagnostics) = validate_workflow_artifact_payload(
            &object_contract,
            None,
            &json!({ "summary": { "status": "ok" } }),
        )
        .expect("valid object payload");
        assert_eq!(render_text.as_deref(), Some("{\n  \"status\": \"ok\"\n}"));
        assert!(diagnostics.is_empty());

        assert_eq!(
            render_text_for_payload(&json!({ "value": null }), Some("$.value")),
            None
        );
        assert_eq!(render_text_for_payload(&json!({}), None), None);
    }

    #[test]
    fn agent_prompt_honors_explicit_artifact_and_state_paths() {
        let artifacts = vec![
            artifact(
                "plan",
                "plan_output",
                json!({ "summary": "Ship it", "private": "secret" }),
            ),
            artifact("state", "state_items", json!({ "records": [1, 2] })),
        ];
        let contract = WorkflowOutputContractDto {
            artifact_type: "verification_result".into(),
            extraction: WorkflowOutputExtractionDto::JsonArray,
            render_text_path: Some("$[0].summary".into()),
            ..WorkflowOutputContractDto::default()
        };
        let schema = json!({ "type": "array" });
        let prompt = build_agent_node_prompt(
            "Release",
            "Verify",
            Some("  Check the implementation.  "),
            &contract,
            Some(&schema),
            None,
            &[
                WorkflowInputBindingDto::Artifact {
                    name: "plan".into(),
                    required: true,
                    artifact_ref: "plan.plan_output".into(),
                    path: Some("$.summary".into()),
                    prompt_label: Some("Plan summary".into()),
                },
                WorkflowInputBindingDto::State {
                    name: "items".into(),
                    required: true,
                    state_ref: "state.state_items".into(),
                    path: None,
                    prompt_label: None,
                },
                WorkflowInputBindingDto::RunInput {
                    name: "optional".into(),
                    required: false,
                    path: None,
                    prompt_label: None,
                },
            ],
            &artifacts,
        )
        .expect("build typed prompt");

        assert!(prompt.starts_with("Check the implementation.\n\nWorkflow: Release"));
        assert!(prompt.contains("## Plan summary\nShip it"));
        assert!(!prompt.contains("secret"));
        assert!(prompt.contains("## items\n{\n  \"records\": ["));
        assert!(prompt.contains("Respond with a single JSON array"));
        assert!(prompt.contains("The JSON must satisfy this JSON Schema"));
        assert!(prompt.contains("render text path `$[0].summary`"));
    }

    #[test]
    fn agent_prompt_rejects_missing_explicit_binding_path_without_payload_fallback() {
        let artifacts = vec![artifact(
            "plan",
            "plan_output",
            json!({ "private": "must not become the requested summary" }),
        )];

        for binding in [
            WorkflowInputBindingDto::Artifact {
                name: "summary".into(),
                required: true,
                artifact_ref: "plan.plan_output".into(),
                path: Some("$.summary".into()),
                prompt_label: None,
            },
            WorkflowInputBindingDto::State {
                name: "summary".into(),
                required: true,
                state_ref: "plan.plan_output".into(),
                path: Some("$.summary".into()),
                prompt_label: None,
            },
        ] {
            let error = build_agent_node_prompt(
                "Release",
                "Verify",
                None,
                &WorkflowOutputContractDto::default(),
                None,
                None,
                &[binding],
                &artifacts,
            )
            .expect_err("missing explicit path must remain missing");
            assert_eq!(error.code, "workflow_required_input_missing");
        }
    }

    #[test]
    fn agent_prompt_includes_unbound_workflow_input() {
        let prompt = build_agent_node_prompt(
            "Release",
            "Plan",
            Some("   "),
            &WorkflowOutputContractDto::default(),
            None,
            Some(&json!({ "goal": "ship" })),
            &[],
            &[],
        )
        .expect("build prompt");

        assert!(prompt.contains("## Workflow input\n{\n  \"goal\": \"ship\"\n}"));
    }

    #[test]
    fn artifact_references_require_known_producer_runs() {
        let record = artifact("plan", "plan_output", json!({}));
        let mut node_ids = BTreeMap::new();
        assert_eq!(artifact_ref_for_record(&node_ids, &record), None);
        node_ids.insert(record.producer_node_run_id.clone(), "plan".into());
        assert_eq!(
            artifact_ref_for_record(&node_ids, &record).as_deref(),
            Some("plan.plan_output")
        );
        assert_eq!(node_id_from_node_run_id("malformed"), None);
    }

    #[test]
    fn schema_validation_reports_each_supported_constraint() {
        let contract = WorkflowOutputContractDto {
            extraction: WorkflowOutputExtractionDto::JsonObject,
            ..WorkflowOutputContractDto::default()
        };
        let schema = json!({
            "type": "object",
            "required": ["name", "items"],
            "properties": {
                "name": { "type": "string", "minLength": 3 },
                "items": {
                    "type": "array",
                    "minItems": 2,
                    "items": { "type": ["string", "null"] }
                }
            },
            "additionalProperties": false
        });
        let error = validate_workflow_artifact_payload(
            &contract,
            Some(&schema),
            &json!({ "name": "x", "items": [false], "extra": true }),
        )
        .expect_err("all schema constraints should be enforced");
        assert_eq!(error.code, "workflow_artifact_schema_invalid");
        assert!(error.message.contains("$.name"));
        assert!(error.message.contains("$.items"));

        let missing_error = validate_workflow_artifact_payload(
            &contract,
            Some(&schema),
            &json!({ "name": "valid" }),
        )
        .expect_err("required property should be enforced");
        assert!(missing_error.message.contains("$.items"));

        assert!(validate_workflow_artifact_payload(
            &contract,
            Some(&json!(true)),
            &json!({ "anything": true }),
        )
        .is_ok());
        assert!(validate_workflow_artifact_payload(
            &contract,
            Some(&json!({ "type": "future_type" })),
            &json!({}),
        )
        .is_ok());
        assert_eq!(schema_type_label(&json!([])), "the declared type");
        assert_eq!(
            schema_type_label(&json!(["string", "null"])),
            "string or null"
        );
    }
}
