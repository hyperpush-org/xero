use std::path::Path;

use serde_json::{json, Value as JsonValue};

use super::persistence::AgentWorkspaceGuard;
use super::tool_dispatch::dispatch_tool_call_with_write_approval;
use super::types::{AgentToolCall, ToolRegistry, ToolRegistryOptions};
use crate::auth::now_timestamp;
use crate::commands::{CommandError, CommandResult, RuntimeAgentIdDto};
use crate::db::project_store::{self, AgentRunDiagnosticRecord, AgentRunStatus, NewAgentRunRecord};
use crate::runtime::AutonomousToolRuntime;

#[derive(Debug, Clone, Copy, Default)]
pub struct SyntheticDispatchOptions {
    pub stop_on_failure: bool,
    pub approve_writes: bool,
    pub operator_approve_all: bool,
}

#[derive(Debug, Clone)]
pub struct SyntheticDispatchResultEntry {
    pub tool_call_id: String,
    pub tool_name: String,
    pub ok: bool,
    pub summary: String,
    pub output: JsonValue,
}

#[derive(Debug, Clone)]
pub struct SyntheticDispatchResult {
    pub run_id: String,
    pub agent_session_id: String,
    pub results: Vec<SyntheticDispatchResultEntry>,
    pub stopped_early: bool,
    pub had_failure: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn dispatch_synthetic_tool_calls(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: String,
    run_id: String,
    provider_id: String,
    model_id: String,
    tool_runtime: AutonomousToolRuntime,
    calls: Vec<AgentToolCall>,
    options: SyntheticDispatchOptions,
) -> CommandResult<SyntheticDispatchResult> {
    if calls.is_empty() {
        return Err(CommandError::user_fixable(
            "developer_tool_harness_no_calls",
            "Synthetic harness runs require at least one tool call.",
        ));
    }

    let now = now_timestamp();
    let prompt_summary = format!(
        "Tool harness run with {} call{}",
        calls.len(),
        if calls.len() == 1 { "" } else { "s" }
    );

    project_store::insert_agent_run(
        repo_root,
        &NewAgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            project_id: project_id.into(),
            agent_session_id: agent_session_id.clone(),
            run_id: run_id.clone(),
            provider_id,
            model_id,
            prompt: prompt_summary,
            system_prompt: "Synthetic developer harness run.".into(),
            now: now.clone(),
        },
    )?;

    project_store::update_agent_run_status(
        repo_root,
        project_id,
        &run_id,
        AgentRunStatus::Running,
        None,
        &now,
    )?;

    let registry_options = ToolRegistryOptions {
        skill_tool_enabled: tool_runtime.skill_tool_enabled(),
        runtime_agent_id: RuntimeAgentIdDto::Engineer,
        ..ToolRegistryOptions::default()
    };
    let tool_registry = ToolRegistry::builtin_with_options(registry_options);
    let mut workspace_guard = AgentWorkspaceGuard::default();

    let mut results = Vec::with_capacity(calls.len());
    let mut stopped_early = false;
    let mut had_failure = false;

    for (index, mut call) in calls.into_iter().enumerate() {
        if let Some(error) = resolve_call_templates(&mut call.input, &results) {
            results.push(SyntheticDispatchResultEntry {
                tool_call_id: call.tool_call_id,
                tool_name: call.tool_name,
                ok: false,
                summary: error.clone(),
                output: json!({
                    "error": {
                        "code": "developer_tool_harness_template_unresolved",
                        "message": error,
                        "index": index,
                    },
                }),
            });
            had_failure = true;
            stopped_early = options.stop_on_failure;
            if options.stop_on_failure {
                break;
            } else {
                continue;
            }
        }
        let tool_name = call.tool_name.clone();
        let tool_call_id = call.tool_call_id.clone();
        let dispatch = dispatch_tool_call_with_write_approval(
            &tool_registry,
            &tool_runtime,
            repo_root,
            project_id,
            &run_id,
            &mut workspace_guard,
            call,
            options.approve_writes,
            options.operator_approve_all,
        );
        let entry = match dispatch {
            Ok(result) => SyntheticDispatchResultEntry {
                tool_call_id: result.tool_call_id,
                tool_name: result.tool_name,
                ok: result.ok,
                summary: result.summary,
                output: result.output,
            },
            Err(error) => SyntheticDispatchResultEntry {
                tool_call_id,
                tool_name,
                ok: false,
                summary: error.message.clone(),
                output: json!({
                    "error": {
                        "code": error.code,
                        "message": error.message,
                        "class": error.class,
                    },
                }),
            },
        };

        if !entry.ok {
            had_failure = true;
        }
        let stop = !entry.ok && options.stop_on_failure;
        results.push(entry);
        if stop {
            stopped_early = true;
            break;
        }
    }

    let final_now = now_timestamp();
    let final_status = if had_failure {
        AgentRunStatus::Failed
    } else {
        AgentRunStatus::Completed
    };
    let diagnostic = if had_failure {
        Some(AgentRunDiagnosticRecord {
            code: "developer_tool_harness_call_failed".into(),
            message: "One or more tool calls failed during the synthetic harness run.".into(),
        })
    } else {
        None
    };
    project_store::update_agent_run_status(
        repo_root,
        project_id,
        &run_id,
        final_status,
        diagnostic,
        &final_now,
    )?;

    Ok(SyntheticDispatchResult {
        run_id,
        agent_session_id,
        results,
        stopped_early,
        had_failure,
    })
}

/// Walks a JSON value and replaces `{{call[N].result.<json-pointer>}}` tokens with
/// values pulled from `previous_results[N].output`. Returns Some(error message) when
/// any token cannot be resolved; mutates the value in place otherwise.
fn resolve_call_templates(
    value: &mut JsonValue,
    previous_results: &[SyntheticDispatchResultEntry],
) -> Option<String> {
    let mut error: Option<String> = None;
    walk_resolve(value, previous_results, &mut error);
    error
}

fn walk_resolve(
    value: &mut JsonValue,
    previous_results: &[SyntheticDispatchResultEntry],
    error: &mut Option<String>,
) {
    if error.is_some() {
        return;
    }
    match value {
        JsonValue::String(text) => {
            if let Some(replacement) = resolve_template_string(text, previous_results, error) {
                *value = replacement;
            }
        }
        JsonValue::Array(items) => {
            for item in items {
                walk_resolve(item, previous_results, error);
                if error.is_some() {
                    return;
                }
            }
        }
        JsonValue::Object(map) => {
            for (_, child) in map.iter_mut() {
                walk_resolve(child, previous_results, error);
                if error.is_some() {
                    return;
                }
            }
        }
        _ => {}
    }
}

fn resolve_template_string(
    text: &str,
    previous_results: &[SyntheticDispatchResultEntry],
    error: &mut Option<String>,
) -> Option<JsonValue> {
    if !text.contains("{{") {
        return None;
    }

    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("{{") {
        if let Some(token) = rest.strip_suffix("}}") {
            // Token is the full string — preserve original JSON type.
            let token = token.trim();
            return match resolve_token(token, previous_results) {
                Ok(value) => Some(value),
                Err(message) => {
                    *error = Some(message);
                    None
                }
            };
        }
    }

    // Embedded substitution — coerce result to string.
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    let bytes = text.as_bytes();
    while cursor < bytes.len() {
        match text[cursor..].find("{{") {
            Some(start) => {
                output.push_str(&text[cursor..cursor + start]);
                let after_open = cursor + start + 2;
                let close = match text[after_open..].find("}}") {
                    Some(offset) => after_open + offset,
                    None => {
                        *error =
                            Some("Unterminated `{{...}}` template token in tool input.".into());
                        return None;
                    }
                };
                let token = text[after_open..close].trim();
                match resolve_token(token, previous_results) {
                    Ok(value) => output.push_str(&value_as_str(&value)),
                    Err(message) => {
                        *error = Some(message);
                        return None;
                    }
                }
                cursor = close + 2;
            }
            None => {
                output.push_str(&text[cursor..]);
                break;
            }
        }
    }

    Some(JsonValue::String(output))
}

fn resolve_token(
    token: &str,
    previous_results: &[SyntheticDispatchResultEntry],
) -> Result<JsonValue, String> {
    let mut rest = token;
    if let Some(stripped) = rest.strip_prefix("call[") {
        rest = stripped;
    } else {
        return Err(format!(
            "Unsupported template token `{token}`. Expected the form `call[N].result.<path>`."
        ));
    }
    let close = rest
        .find(']')
        .ok_or_else(|| format!("Template token `{token}` is missing a closing `]`."))?;
    let index_text = &rest[..close];
    let index: usize = index_text.parse().map_err(|_| {
        format!("Template token `{token}` has a non-numeric call index `{index_text}`.")
    })?;
    rest = rest[close + 1..].trim_start();
    rest = rest.strip_prefix('.').ok_or_else(|| {
        format!("Template token `{token}` is missing the leading `.` after `[..]`.")
    })?;
    let mut segments = rest.split('.');
    let scope = segments
        .next()
        .ok_or_else(|| format!("Template token `{token}` is missing a scope (use `result`)."))?;
    if scope != "result" {
        return Err(format!(
            "Template token `{token}` references `{scope}`. Only `.result.*` is supported."
        ));
    }
    let entry = previous_results.get(index).ok_or_else(|| {
        format!(
            "Template token `{token}` references call[{index}] but only {} prior result(s) are available.",
            previous_results.len()
        )
    })?;
    let mut current: &JsonValue = &entry.output;
    let mut visited: Vec<String> = vec!["result".into()];
    for segment in segments {
        if segment.is_empty() {
            continue;
        }
        visited.push(segment.into());
        match current {
            JsonValue::Object(map) => {
                current = map
                    .get(segment)
                    .ok_or_else(|| format!("Template path `{}` not found.", visited.join(".")))?;
            }
            JsonValue::Array(items) => {
                let parsed: usize = segment.parse().map_err(|_| {
                    format!(
                        "Template path `{}` expected a numeric array index but got `{segment}`.",
                        visited.join(".")
                    )
                })?;
                current = items.get(parsed).ok_or_else(|| {
                    format!("Template path `{}` is out of bounds.", visited.join("."))
                })?;
            }
            _ => {
                return Err(format!(
                    "Template path `{}` traverses through a non-object/non-array value.",
                    visited.join(".")
                ));
            }
        }
    }
    Ok(current.clone())
}

fn value_as_str(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => text.clone(),
        JsonValue::Null => "".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(output: JsonValue) -> SyntheticDispatchResultEntry {
        SyntheticDispatchResultEntry {
            tool_call_id: "call-0".into(),
            tool_name: "fake".into(),
            ok: true,
            summary: "ok".into(),
            output,
        }
    }

    #[test]
    fn resolves_full_token_preserving_json_type() {
        let mut value = json!("{{call[0].result.sessionId}}");
        let prior = vec![entry(json!({"sessionId": 42}))];
        assert!(resolve_call_templates(&mut value, &prior).is_none());
        assert_eq!(value, json!(42));
    }

    #[test]
    fn resolves_embedded_token_into_string() {
        let mut value = json!({"path": "logs/{{call[0].result.name}}.txt"});
        let prior = vec![entry(json!({"name": "harness"}))];
        assert!(resolve_call_templates(&mut value, &prior).is_none());
        assert_eq!(value, json!({"path": "logs/harness.txt"}));
    }

    #[test]
    fn errors_when_call_index_out_of_range() {
        let mut value = json!("{{call[3].result.sessionId}}");
        let prior = vec![entry(json!({"sessionId": "x"}))];
        let error = resolve_call_templates(&mut value, &prior).expect("should error");
        assert!(error.contains("call[3]"));
    }

    #[test]
    fn errors_on_unsupported_scope() {
        let mut value = json!("{{call[0].input.foo}}");
        let prior = vec![entry(json!({"foo": "bar"}))];
        let error = resolve_call_templates(&mut value, &prior).expect("should error");
        assert!(error.contains("input"));
    }

    #[test]
    fn skips_strings_without_template_markers() {
        let mut value = json!({"path": "logs/static"});
        let prior: Vec<SyntheticDispatchResultEntry> = Vec::new();
        let original = value.clone();
        assert!(resolve_call_templates(&mut value, &prior).is_none());
        assert_eq!(value, original);
    }
}
