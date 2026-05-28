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

use super::condition_eval::json_path_lookup;

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
                let value = match (initial_input, path.as_deref()) {
                    (Some(value), Some(path)) => json_path_lookup(value, path).cloned(),
                    (Some(value), None) => Some(value.clone()),
                    (None, _) => None,
                };
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
                let value = artifact_index.get(artifact_ref).and_then(|artifact| {
                    path.as_deref()
                        .and_then(|path| json_path_lookup(&artifact.payload, path).cloned())
                        .or_else(|| Some(artifact.payload.clone()))
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
                let value = artifact_index.get(state_ref).and_then(|artifact| {
                    path.as_deref()
                        .and_then(|path| json_path_lookup(&artifact.payload, path).cloned())
                        .or_else(|| Some(artifact.payload.clone()))
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
}
