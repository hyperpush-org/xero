use std::{
    path::Path,
    sync::{mpsc::TrySendError, Arc, Mutex},
};

use crate::{
    auth::now_timestamp,
    db::project_store::{self, RuntimeRunStatus},
    runtime::autonomous_orchestrator,
};

use super::{
    boundary::{checkpoint_summary_for_runtime_boundary, emit_structured_runtime_boundary},
    persistence::{
        apply_pending_controls_at_boundary, persist_sidecar_checkpoint, PendingControlApplyOutcome,
    },
    BufferedSupervisorEvent, NormalizedPtyEvent, PtyEventNormalizer, SidecarSharedState,
    SupervisorEventHub, ACTIVITY_OUTPUT_PREFIX, LIVE_EVENT_RING_LIMIT,
    MAX_LIVE_EVENT_FRAGMENT_BYTES, MAX_LIVE_EVENT_TEXT_CHARS, REDACTED_LIVE_EVENT_DETAIL,
    SHELL_OUTPUT_PREFIX, STRUCTURED_EVENT_PREFIX,
};
use crate::runtime::protocol::{
    CommandToolResultSummary, FileToolResultSummary, GitToolResultSummary,
    SupervisorLiveEventPayload, SupervisorSkillCacheStatus, SupervisorSkillDiagnostic,
    SupervisorSkillLifecycleResult, SupervisorSkillLifecycleStage, SupervisorSkillSourceMetadata,
    SupervisorToolCallState, ToolResultSummary, WebToolResultSummary,
};

const CONTROL_APPLY_BOUNDARY_ACTIVITY_CODE: &str = "runtime_run_controls_apply_boundary";
const CONTROL_APPLIED_ACTIVITY_CODE: &str = "runtime_run_controls_applied";
const CONTROL_APPLIED_ACTIVITY_TITLE: &str = "Queued runtime controls applied";
const CONTROL_APPLY_FAILED_ACTIVITY_TITLE: &str = "Queued runtime controls still pending";

impl PtyEventNormalizer {
    pub(super) fn push_chunk(&mut self, chunk: &[u8]) -> Vec<NormalizedPtyEvent> {
        self.pending.extend_from_slice(chunk);
        self.drain_complete_lines(false)
    }

    pub(super) fn finish(&mut self) -> Vec<NormalizedPtyEvent> {
        self.drain_complete_lines(true)
    }

    fn drain_complete_lines(&mut self, flush_partial: bool) -> Vec<NormalizedPtyEvent> {
        let mut events = Vec::new();

        loop {
            let Some(newline_index) = self.pending.iter().position(|byte| *byte == b'\n') else {
                if self.pending.len() > MAX_LIVE_EVENT_FRAGMENT_BYTES {
                    self.pending.clear();
                    events.push(diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized detached PTY output fragment before replay.",
                    ));
                } else if flush_partial && !self.pending.is_empty() {
                    let remainder = std::mem::take(&mut self.pending);
                    events.extend(normalize_pty_line_bytes(&remainder));
                }
                break;
            };

            let mut line = self.pending.drain(..=newline_index).collect::<Vec<_>>();
            if matches!(line.last(), Some(b'\n')) {
                line.pop();
            }
            if matches!(line.last(), Some(b'\r')) {
                line.pop();
            }
            events.extend(normalize_pty_line_bytes(&line));
        }

        events
    }
}

fn normalize_pty_line_bytes(raw_line: &[u8]) -> Vec<NormalizedPtyEvent> {
    if raw_line.is_empty() {
        return Vec::new();
    }

    let line = match String::from_utf8(raw_line.to_vec()) {
        Ok(line) => line,
        Err(_) => {
            return vec![diagnostic_live_event(
                "runtime_supervisor_live_event_decode_failed",
                "Live output decode failed",
                "Cadence dropped a detached PTY output fragment that was not valid UTF-8.",
            )];
        }
    };

    normalize_pty_line(&line).into_iter().collect()
}

fn normalize_pty_line(raw_line: &str) -> Option<NormalizedPtyEvent> {
    let trimmed = raw_line.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(payload) = trimmed.strip_prefix(STRUCTURED_EVENT_PREFIX) {
        return Some(normalize_structured_event(payload));
    }

    let text = match sanitize_text_fragment(trimmed) {
        Ok(Some(text)) => text,
        Ok(None) => return None,
        Err(()) => {
            return Some(diagnostic_live_event(
                "runtime_supervisor_live_event_oversized",
                "Live output fragment dropped",
                "Cadence dropped an oversized detached PTY output fragment before replay.",
            ))
        }
    };

    if contains_prohibited_live_content(&text).is_some() {
        return Some(redacted_live_event());
    }

    Some(NormalizedPtyEvent {
        checkpoint_summary: summarize_pty_output(&text),
        item: SupervisorLiveEventPayload::Transcript { text },
    })
}

fn normalize_structured_event(payload: &str) -> NormalizedPtyEvent {
    if payload.trim().is_empty() {
        return diagnostic_live_event(
            "runtime_supervisor_live_event_blank",
            "Live output fragment dropped",
            "Cadence dropped a blank structured live-event payload before replay.",
        );
    }

    if payload.len() > MAX_LIVE_EVENT_FRAGMENT_BYTES {
        return diagnostic_live_event(
            "runtime_supervisor_live_event_oversized",
            "Live output fragment dropped",
            "Cadence dropped an oversized structured live-event payload before replay.",
        );
    }

    let value = match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(value) => value,
        Err(_) => {
            return diagnostic_live_event(
                "runtime_supervisor_live_event_invalid",
                "Live output fragment dropped",
                "Cadence dropped a malformed structured live-event payload before replay.",
            );
        }
    };

    let Some(kind) = value.get("kind").and_then(serde_json::Value::as_str) else {
        return diagnostic_live_event(
            "runtime_supervisor_live_event_invalid",
            "Live output fragment dropped",
            "Cadence dropped a structured live-event payload without a kind.",
        );
    };

    match kind {
        "transcript" => {
            let Some(text) = value.get("text").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured transcript payload without text.",
                );
            };
            let text =
                match sanitize_text_fragment(text) {
                    Ok(Some(text)) => text,
                    Ok(None) => {
                        return diagnostic_live_event(
                            "runtime_supervisor_live_event_blank",
                            "Live output fragment dropped",
                            "Cadence dropped a blank structured transcript payload before replay.",
                        )
                    }
                    Err(()) => return diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized structured transcript payload before replay.",
                    ),
                };
            if contains_prohibited_live_content(&text).is_some() {
                redacted_live_event()
            } else {
                NormalizedPtyEvent {
                    checkpoint_summary: summarize_pty_output(&text),
                    item: SupervisorLiveEventPayload::Transcript { text },
                }
            }
        }
        "tool" => {
            let Some(tool_call_id) = value
                .get("tool_call_id")
                .and_then(serde_json::Value::as_str)
            else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured tool payload without a tool_call_id.",
                );
            };
            let Some(tool_name) = value.get("tool_name").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured tool payload without a tool_name.",
                );
            };
            let Some(tool_state) = value.get("tool_state").and_then(serde_json::Value::as_str)
            else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured tool payload without a tool_state.",
                );
            };
            let Some(tool_call_id) = sanitize_identifier_fragment(tool_call_id) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured tool payload with a blank tool_call_id.",
                );
            };
            let Some(tool_name) = sanitize_identifier_fragment(tool_name) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured tool payload with a blank tool_name.",
                );
            };
            let tool_state = match tool_state {
                "pending" => SupervisorToolCallState::Pending,
                "running" => SupervisorToolCallState::Running,
                "succeeded" => SupervisorToolCallState::Succeeded,
                "failed" => SupervisorToolCallState::Failed,
                _ => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_unsupported",
                        "Live output fragment dropped",
                        "Cadence dropped a structured tool payload with an unsupported tool_state.",
                    );
                }
            };
            let detail = value
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .map(sanitize_text_fragment)
                .transpose();
            let detail = match detail {
                Ok(detail) => detail.flatten(),
                Err(_) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized structured tool detail before replay.",
                    );
                }
            };
            let tool_summary = value
                .get("tool_summary")
                .map(sanitize_tool_result_summary_value)
                .transpose();
            let tool_summary = match tool_summary {
                Ok(tool_summary) => tool_summary,
                Err(ToolSummaryDecodeError::Oversized) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized structured tool summary before replay.",
                    );
                }
                Err(ToolSummaryDecodeError::Unsupported) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_unsupported",
                        "Live output fragment dropped",
                        "Cadence dropped a structured tool payload with an unsupported tool_summary kind.",
                    );
                }
                Err(ToolSummaryDecodeError::Invalid) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_invalid",
                        "Live output fragment dropped",
                        "Cadence dropped a structured tool payload with invalid tool_summary metadata.",
                    );
                }
            };
            if [
                Some(tool_call_id.as_str()),
                Some(tool_name.as_str()),
                detail.as_deref(),
            ]
            .into_iter()
            .flatten()
            .chain(
                tool_summary
                    .as_ref()
                    .into_iter()
                    .flat_map(tool_result_summary_text_fragments),
            )
            .any(|value| contains_prohibited_live_content(value).is_some())
            {
                redacted_live_event()
            } else {
                NormalizedPtyEvent {
                    checkpoint_summary: Some(tool_checkpoint_summary(&tool_name, &tool_state)),
                    item: SupervisorLiveEventPayload::Tool {
                        tool_call_id,
                        tool_name,
                        tool_state,
                        detail,
                        tool_summary,
                    },
                }
            }
        }
        "activity" => {
            let Some(code) = value.get("code").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured activity payload without a code.",
                );
            };
            let Some(title) = value.get("title").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured activity payload without a title.",
                );
            };
            let Some(code) = sanitize_identifier_fragment(code) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured activity payload with a blank code.",
                );
            };
            let title = match sanitize_text_fragment(title) {
                Ok(Some(title)) => title,
                Ok(None) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_blank",
                        "Live output fragment dropped",
                        "Cadence dropped a structured activity payload with a blank title.",
                    )
                }
                Err(()) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized structured activity title before replay.",
                    )
                }
            };
            let detail = value
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .map(sanitize_text_fragment)
                .transpose();
            let detail = match detail {
                Ok(detail) => detail.flatten(),
                Err(_) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized structured activity detail before replay.",
                    );
                }
            };
            if [Some(code.as_str()), Some(title.as_str()), detail.as_deref()]
                .into_iter()
                .flatten()
                .any(|value| contains_prohibited_live_content(value).is_some())
            {
                redacted_live_event()
            } else {
                NormalizedPtyEvent {
                    checkpoint_summary: Some(activity_checkpoint_summary(&code, &title)),
                    item: SupervisorLiveEventPayload::Activity {
                        code,
                        title,
                        detail,
                    },
                }
            }
        }
        "skill" => {
            let Some(skill_id) = value.get("skill_id").and_then(serde_json::Value::as_str) else {
                return diagnostic_skill_live_event(SkillLiveEventDecodeError::Missing("skill_id"));
            };
            let Some(stage) = value.get("stage").and_then(serde_json::Value::as_str) else {
                return diagnostic_skill_live_event(SkillLiveEventDecodeError::Missing("stage"));
            };
            let Some(result) = value.get("result").and_then(serde_json::Value::as_str) else {
                return diagnostic_skill_live_event(SkillLiveEventDecodeError::Missing("result"));
            };
            let Some(detail) = value.get("detail").and_then(serde_json::Value::as_str) else {
                return diagnostic_skill_live_event(SkillLiveEventDecodeError::Missing("detail"));
            };
            let Some(source_value) = value.get("source") else {
                return diagnostic_skill_live_event(SkillLiveEventDecodeError::Missing("source"));
            };

            let Some(skill_id) = sanitize_skill_id_fragment(skill_id) else {
                return diagnostic_skill_live_event(SkillLiveEventDecodeError::Blank("skill_id"));
            };
            let stage = match stage {
                "discovery" => SupervisorSkillLifecycleStage::Discovery,
                "install" => SupervisorSkillLifecycleStage::Install,
                "invoke" => SupervisorSkillLifecycleStage::Invoke,
                _ => {
                    return diagnostic_skill_live_event(SkillLiveEventDecodeError::Unsupported(
                        "stage",
                    ))
                }
            };
            let result = match result {
                "succeeded" => SupervisorSkillLifecycleResult::Succeeded,
                "failed" => SupervisorSkillLifecycleResult::Failed,
                _ => {
                    return diagnostic_skill_live_event(SkillLiveEventDecodeError::Unsupported(
                        "result",
                    ))
                }
            };
            let detail = match sanitize_text_fragment(detail) {
                Ok(Some(detail)) => detail,
                Ok(None) => {
                    return diagnostic_skill_live_event(SkillLiveEventDecodeError::Blank("detail"))
                }
                Err(()) => {
                    return diagnostic_skill_live_event(SkillLiveEventDecodeError::Oversized(
                        "detail",
                    ))
                }
            };
            let source = match sanitize_skill_source_metadata(source_value) {
                Ok(source) => source,
                Err(error) => return diagnostic_skill_live_event(error),
            };
            let cache_status = match value.get("cache_status") {
                Some(raw_cache_status) if raw_cache_status.is_null() => None,
                Some(raw_cache_status) => {
                    let Some(cache_status) = raw_cache_status.as_str() else {
                        return diagnostic_skill_live_event(SkillLiveEventDecodeError::Invalid(
                            "cache_status",
                        ));
                    };
                    let cache_status = match cache_status {
                        "miss" => SupervisorSkillCacheStatus::Miss,
                        "hit" => SupervisorSkillCacheStatus::Hit,
                        "refreshed" => SupervisorSkillCacheStatus::Refreshed,
                        _ => {
                            return diagnostic_skill_live_event(
                                SkillLiveEventDecodeError::Unsupported("cache_status"),
                            )
                        }
                    };
                    Some(cache_status)
                }
                None => None,
            };
            let diagnostic = match value.get("diagnostic") {
                Some(raw_diagnostic) if raw_diagnostic.is_null() => None,
                Some(raw_diagnostic) => match sanitize_skill_diagnostic(raw_diagnostic) {
                    Ok(diagnostic) => Some(diagnostic),
                    Err(error) => return diagnostic_skill_live_event(error),
                },
                None => None,
            };

            if matches!(stage, SupervisorSkillLifecycleStage::Discovery) && cache_status.is_some() {
                return diagnostic_skill_live_event(SkillLiveEventDecodeError::Invalid(
                    "cache_status",
                ));
            }
            if matches!(
                stage,
                SupervisorSkillLifecycleStage::Install | SupervisorSkillLifecycleStage::Invoke
            ) && matches!(result, SupervisorSkillLifecycleResult::Succeeded)
                && cache_status.is_none()
            {
                return diagnostic_skill_live_event(SkillLiveEventDecodeError::Missing(
                    "cache_status",
                ));
            }
            match (&result, diagnostic.as_ref()) {
                (SupervisorSkillLifecycleResult::Succeeded, Some(_)) => {
                    return diagnostic_skill_live_event(SkillLiveEventDecodeError::Invalid(
                        "diagnostic",
                    ))
                }
                (SupervisorSkillLifecycleResult::Failed, None) => {
                    return diagnostic_skill_live_event(SkillLiveEventDecodeError::Missing(
                        "diagnostic",
                    ))
                }
                _ => {}
            }

            if [
                Some(skill_id.as_str()),
                Some(detail.as_str()),
                Some(source.repo.as_str()),
                Some(source.path.as_str()),
                Some(source.reference.as_str()),
                Some(source.tree_hash.as_str()),
            ]
            .into_iter()
            .flatten()
            .chain(diagnostic.as_ref().into_iter().flat_map(|diagnostic| {
                [diagnostic.code.as_str(), diagnostic.message.as_str()].into_iter()
            }))
            .any(|value| contains_prohibited_live_content(value).is_some())
            {
                redacted_live_event()
            } else {
                NormalizedPtyEvent {
                    checkpoint_summary: Some(skill_checkpoint_summary(&skill_id, &stage, &result)),
                    item: SupervisorLiveEventPayload::Skill {
                        skill_id,
                        stage,
                        result,
                        detail,
                        source,
                        cache_status,
                        diagnostic,
                    },
                }
            }
        }
        "action_required" => {
            let Some(action_id) = value.get("action_id").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload without an action_id.",
                );
            };
            let Some(boundary_id) = value.get("boundary_id").and_then(serde_json::Value::as_str)
            else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload without a boundary_id.",
                );
            };
            let Some(action_type) = value.get("action_type").and_then(serde_json::Value::as_str)
            else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload without an action_type.",
                );
            };
            let Some(title) = value.get("title").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload without a title.",
                );
            };
            let Some(detail) = value.get("detail").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload without detail.",
                );
            };

            let Some(action_id) = sanitize_identifier_fragment(action_id) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload with a blank action_id.",
                );
            };
            let Some(boundary_id) = sanitize_identifier_fragment(boundary_id) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload with a blank boundary_id.",
                );
            };
            let Some(action_type) = sanitize_identifier_fragment(action_type) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload with a blank action_type.",
                );
            };
            let title = match sanitize_text_fragment(title) {
                Ok(Some(title)) => title,
                Ok(None) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_blank",
                        "Live output fragment dropped",
                        "Cadence dropped a structured action-required payload with a blank title.",
                    )
                }
                Err(()) => return diagnostic_live_event(
                    "runtime_supervisor_live_event_oversized",
                    "Live output fragment dropped",
                    "Cadence dropped an oversized structured action-required title before replay.",
                ),
            };
            let detail = match sanitize_text_fragment(detail) {
                Ok(Some(detail)) => detail,
                Ok(None) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_blank",
                        "Live output fragment dropped",
                        "Cadence dropped a structured action-required payload with blank detail.",
                    )
                }
                Err(()) => return diagnostic_live_event(
                    "runtime_supervisor_live_event_oversized",
                    "Live output fragment dropped",
                    "Cadence dropped an oversized structured action-required detail before replay.",
                ),
            };
            if [
                Some(action_id.as_str()),
                Some(boundary_id.as_str()),
                Some(action_type.as_str()),
                Some(title.as_str()),
                Some(detail.as_str()),
            ]
            .into_iter()
            .flatten()
            .any(|value| contains_prohibited_live_content(value).is_some())
            {
                redacted_live_event()
            } else {
                NormalizedPtyEvent {
                    checkpoint_summary: Some(checkpoint_summary_for_runtime_boundary(
                        &action_type,
                        &title,
                    )),
                    item: SupervisorLiveEventPayload::ActionRequired {
                        action_id,
                        boundary_id,
                        action_type,
                        title,
                        detail,
                    },
                }
            }
        }
        _ => diagnostic_live_event(
            "runtime_supervisor_live_event_unsupported",
            "Live output fragment dropped",
            "Cadence dropped a structured live-event payload with an unsupported kind.",
        ),
    }
}

fn sanitize_identifier_fragment(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    if value.chars().count() > MAX_LIVE_EVENT_TEXT_CHARS {
        return None;
    }
    Some(value.to_string())
}

pub(super) fn sanitize_text_fragment(raw: &str) -> Result<Option<String>, ()> {
    let sanitized = raw
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\t' => ' ',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect::<String>();
    let collapsed = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_LIVE_EVENT_TEXT_CHARS {
        return Err(());
    }
    Ok(Some(trimmed.to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillLiveEventDecodeError {
    Missing(&'static str),
    Blank(&'static str),
    Invalid(&'static str),
    Oversized(&'static str),
    Unsupported(&'static str),
}

fn diagnostic_skill_live_event(error: SkillLiveEventDecodeError) -> NormalizedPtyEvent {
    let (code, detail) = match error {
        SkillLiveEventDecodeError::Missing(field) => (
            "runtime_supervisor_skill_event_invalid",
            format!("Cadence dropped a structured skill payload without a {field}."),
        ),
        SkillLiveEventDecodeError::Blank(field) => (
            "runtime_supervisor_skill_event_blank",
            format!("Cadence dropped a structured skill payload with a blank {field}."),
        ),
        SkillLiveEventDecodeError::Invalid(field) => (
            "runtime_supervisor_skill_event_invalid",
            format!("Cadence dropped a structured skill payload with invalid {field} metadata."),
        ),
        SkillLiveEventDecodeError::Oversized(field) => (
            "runtime_supervisor_skill_event_oversized",
            format!("Cadence dropped an oversized structured skill {field} before replay."),
        ),
        SkillLiveEventDecodeError::Unsupported(field) => (
            "runtime_supervisor_skill_event_unsupported",
            format!("Cadence dropped a structured skill payload with an unsupported {field}."),
        ),
    };

    diagnostic_live_event(code, "Skill event dropped", &detail)
}

fn sanitize_skill_id_fragment(raw: &str) -> Option<String> {
    let value = sanitize_identifier_fragment(raw)?;
    if value.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    }) {
        Some(value)
    } else {
        None
    }
}

fn sanitize_skill_source_text_field(
    value: Option<&serde_json::Value>,
    field: &'static str,
) -> Result<String, SkillLiveEventDecodeError> {
    let Some(value) = value.and_then(serde_json::Value::as_str) else {
        return Err(SkillLiveEventDecodeError::Missing(field));
    };

    match sanitize_text_fragment(value) {
        Ok(Some(sanitized)) => Ok(sanitized),
        Ok(None) => Err(SkillLiveEventDecodeError::Blank(field)),
        Err(()) => Err(SkillLiveEventDecodeError::Oversized(field)),
    }
}

fn sanitize_skill_source_metadata(
    value: &serde_json::Value,
) -> Result<SupervisorSkillSourceMetadata, SkillLiveEventDecodeError> {
    let Some(source) = value.as_object() else {
        return Err(SkillLiveEventDecodeError::Invalid("source"));
    };

    let tree_hash = sanitize_skill_source_text_field(source.get("tree_hash"), "source.tree_hash")?;
    if tree_hash.len() != 40
        || tree_hash
            .chars()
            .any(|character| !character.is_ascii_hexdigit() || character.is_ascii_uppercase())
    {
        return Err(SkillLiveEventDecodeError::Invalid("source.tree_hash"));
    }

    Ok(SupervisorSkillSourceMetadata {
        repo: sanitize_skill_source_text_field(source.get("repo"), "source.repo")?,
        path: sanitize_skill_source_text_field(source.get("path"), "source.path")?,
        reference: sanitize_skill_source_text_field(source.get("reference"), "source.reference")?,
        tree_hash,
    })
}

fn sanitize_skill_diagnostic(
    value: &serde_json::Value,
) -> Result<SupervisorSkillDiagnostic, SkillLiveEventDecodeError> {
    let Some(diagnostic) = value.as_object() else {
        return Err(SkillLiveEventDecodeError::Invalid("diagnostic"));
    };

    let code = sanitize_skill_source_text_field(diagnostic.get("code"), "diagnostic.code")?;
    let message =
        sanitize_skill_source_text_field(diagnostic.get("message"), "diagnostic.message")?;
    let Some(retryable) = diagnostic
        .get("retryable")
        .and_then(serde_json::Value::as_bool)
    else {
        return Err(SkillLiveEventDecodeError::Missing("diagnostic.retryable"));
    };

    Ok(SupervisorSkillDiagnostic {
        code,
        message,
        retryable,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolSummaryDecodeError {
    Invalid,
    Oversized,
    Unsupported,
}

fn sanitize_tool_result_summary_value(
    value: &serde_json::Value,
) -> Result<ToolResultSummary, ToolSummaryDecodeError> {
    let parsed = serde_json::from_value::<ToolResultSummary>(value.clone()).map_err(|error| {
        let details = error.to_string();
        if details.contains("unknown variant") {
            ToolSummaryDecodeError::Unsupported
        } else {
            ToolSummaryDecodeError::Invalid
        }
    })?;
    sanitize_tool_result_summary(parsed)
}

fn sanitize_tool_result_summary(
    summary: ToolResultSummary,
) -> Result<ToolResultSummary, ToolSummaryDecodeError> {
    match summary {
        ToolResultSummary::Command(summary) => Ok(ToolResultSummary::Command(summary)),
        ToolResultSummary::File(summary) => Ok(ToolResultSummary::File(FileToolResultSummary {
            path: sanitize_optional_tool_summary_text(summary.path)?,
            scope: sanitize_optional_tool_summary_text(summary.scope)?,
            line_count: summary.line_count,
            match_count: summary.match_count,
            truncated: summary.truncated,
        })),
        ToolResultSummary::Git(summary) => Ok(ToolResultSummary::Git(GitToolResultSummary {
            scope: summary.scope,
            changed_files: summary.changed_files,
            truncated: summary.truncated,
            base_revision: sanitize_optional_tool_summary_text(summary.base_revision)?,
        })),
        ToolResultSummary::Web(summary) => Ok(ToolResultSummary::Web(WebToolResultSummary {
            target: sanitize_required_tool_summary_text(summary.target)?,
            result_count: summary.result_count,
            final_url: sanitize_optional_tool_summary_text(summary.final_url)?,
            content_kind: summary.content_kind,
            content_type: sanitize_optional_tool_summary_text(summary.content_type)?,
            truncated: summary.truncated,
        })),
    }
}

fn sanitize_optional_tool_summary_text(
    value: Option<String>,
) -> Result<Option<String>, ToolSummaryDecodeError> {
    value
        .map(|value| match sanitize_text_fragment(&value) {
            Ok(sanitized) => Ok(sanitized),
            Err(()) => Err(ToolSummaryDecodeError::Oversized),
        })
        .transpose()
        .map(|value| value.flatten())
}

fn sanitize_required_tool_summary_text(value: String) -> Result<String, ToolSummaryDecodeError> {
    match sanitize_text_fragment(&value) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(ToolSummaryDecodeError::Invalid),
        Err(()) => Err(ToolSummaryDecodeError::Oversized),
    }
}

fn tool_result_summary_text_fragments(summary: &ToolResultSummary) -> Vec<&str> {
    match summary {
        ToolResultSummary::Command(CommandToolResultSummary { .. }) => Vec::new(),
        ToolResultSummary::File(summary) => [summary.path.as_deref(), summary.scope.as_deref()]
            .into_iter()
            .flatten()
            .collect(),
        ToolResultSummary::Git(GitToolResultSummary { base_revision, .. }) => {
            base_revision.iter().map(String::as_str).collect()
        }
        ToolResultSummary::Web(summary) => [
            Some(summary.target.as_str()),
            summary.final_url.as_deref(),
            summary.content_type.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect(),
    }
}

fn contains_prohibited_live_content(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();

    if normalized.contains("access_token")
        || normalized.contains("refresh_token")
        || normalized.contains("bearer ")
        || normalized.contains("oauth")
        || normalized.contains("sk-")
    {
        return Some("OAuth or API token material");
    }

    if normalized.contains("redirect_uri")
        || normalized.contains("authorization_url")
        || normalized.contains("/auth/callback")
        || normalized.contains("127.0.0.1:")
        || normalized.contains("localhost:")
    {
        return Some("OAuth redirect URL data");
    }

    if normalized.contains("chatgpt_account_id")
        || (normalized.contains("session_id") && normalized.contains("provider_id"))
    {
        return Some("auth-store contents");
    }

    None
}

fn redacted_live_event() -> NormalizedPtyEvent {
    diagnostic_live_event(
        "runtime_supervisor_live_event_redacted",
        "Live output redacted",
        REDACTED_LIVE_EVENT_DETAIL,
    )
}

pub(super) fn diagnostic_live_event(code: &str, title: &str, detail: &str) -> NormalizedPtyEvent {
    NormalizedPtyEvent {
        checkpoint_summary: Some(activity_checkpoint_summary(code, title)),
        item: SupervisorLiveEventPayload::Activity {
            code: code.into(),
            title: title.into(),
            detail: Some(detail.into()),
        },
    }
}

fn tool_checkpoint_summary(tool_name: &str, tool_state: &SupervisorToolCallState) -> String {
    let state = match tool_state {
        SupervisorToolCallState::Pending => "pending",
        SupervisorToolCallState::Running => "running",
        SupervisorToolCallState::Succeeded => "succeeded",
        SupervisorToolCallState::Failed => "failed",
    };
    format!("Tool `{tool_name}` {state}.")
}

fn skill_checkpoint_summary(
    skill_id: &str,
    stage: &SupervisorSkillLifecycleStage,
    result: &SupervisorSkillLifecycleResult,
) -> String {
    let stage = match stage {
        SupervisorSkillLifecycleStage::Discovery => "discovery",
        SupervisorSkillLifecycleStage::Install => "install",
        SupervisorSkillLifecycleStage::Invoke => "invoke",
    };
    let result = match result {
        SupervisorSkillLifecycleResult::Succeeded => "succeeded",
        SupervisorSkillLifecycleResult::Failed => "failed",
    };

    format!("Skill `{skill_id}` {stage} {result}.")
}

fn activity_checkpoint_summary(code: &str, title: &str) -> String {
    format!("{ACTIVITY_OUTPUT_PREFIX} {code}: {title}")
}

pub(super) fn emit_normalized_events(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    events: Vec<NormalizedPtyEvent>,
) {
    for event in events {
        if let SupervisorLiveEventPayload::ActionRequired {
            action_id,
            boundary_id,
            action_type,
            title,
            detail,
        } = &event.item
        {
            emit_structured_runtime_boundary(
                repo_root,
                shared,
                event_hub,
                persistence_lock,
                action_id,
                boundary_id,
                action_type,
                title,
                detail,
            );
            continue;
        }

        let buffered = append_live_event(shared, event_hub, &event.item);
        if let Some(summary) = event
            .checkpoint_summary
            .filter(|summary| should_persist_live_event_checkpoint(&buffered, summary))
        {
            let _ = persist_sidecar_checkpoint(
                repo_root,
                shared,
                persistence_lock,
                RuntimeRunStatus::Running,
                project_store::RuntimeRunCheckpointKind::State,
                summary,
            );
        }

        let should_persist_autonomous_event = match &event.item {
            SupervisorLiveEventPayload::Tool { .. } | SupervisorLiveEventPayload::Skill { .. } => {
                true
            }
            SupervisorLiveEventPayload::Activity { code, .. } => code.contains("policy_denied"),
            _ => false,
        };
        if should_persist_autonomous_event {
            persist_autonomous_live_event(
                repo_root,
                shared,
                event_hub,
                persistence_lock,
                &event.item,
            );
        }

        maybe_apply_queued_controls_at_boundary(
            repo_root,
            shared,
            event_hub,
            persistence_lock,
            &event.item,
        );
    }
}

fn maybe_apply_queued_controls_at_boundary(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    event: &SupervisorLiveEventPayload,
) {
    let SupervisorLiveEventPayload::Activity { code, .. } = event else {
        return;
    };
    if code != CONTROL_APPLY_BOUNDARY_ACTIVITY_CODE {
        return;
    }

    match apply_pending_controls_at_boundary(repo_root, shared, persistence_lock) {
        PendingControlApplyOutcome::NoPending => {}
        PendingControlApplyOutcome::Applied(applied) => {
            let detail = if applied.prompt_consumed {
                format!(
                    "Cadence applied queued runtime-run controls and one queued prompt at model-call boundary revision {}.",
                    applied.revision
                )
            } else {
                format!(
                    "Cadence applied queued runtime-run controls at model-call boundary revision {}.",
                    applied.revision
                )
            };
            append_checkpointed_activity(
                repo_root,
                shared,
                event_hub,
                persistence_lock,
                CONTROL_APPLIED_ACTIVITY_CODE,
                CONTROL_APPLIED_ACTIVITY_TITLE,
                Some(detail),
            );
        }
        PendingControlApplyOutcome::PersistFailed { code, message } => {
            append_checkpointed_activity(
                repo_root,
                shared,
                event_hub,
                persistence_lock,
                &code,
                CONTROL_APPLY_FAILED_ACTIVITY_TITLE,
                Some(message),
            );
        }
    }
}

fn append_checkpointed_activity(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    code: &str,
    title: &str,
    detail: Option<String>,
) {
    let item = SupervisorLiveEventPayload::Activity {
        code: code.into(),
        title: title.into(),
        detail,
    };
    let summary = activity_checkpoint_summary(code, title);
    let buffered = append_live_event(shared, event_hub, &item);
    if should_persist_live_event_checkpoint(&buffered, &summary) {
        let _ = persist_sidecar_checkpoint(
            repo_root,
            shared,
            persistence_lock,
            RuntimeRunStatus::Running,
            project_store::RuntimeRunCheckpointKind::State,
            summary,
        );
    }
}

pub(super) fn persist_autonomous_live_event(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    event: &SupervisorLiveEventPayload,
) {
    let project_id = {
        shared
            .lock()
            .expect("sidecar state lock poisoned")
            .project_id
            .clone()
    };

    let Err(error) =
        autonomous_orchestrator::persist_supervisor_event(repo_root, &project_id, event)
    else {
        return;
    };

    let detail = format!(
        "Cadence kept the prior durable autonomous snapshot after rejecting live-event persistence: [{}] {}",
        error.code, error.message,
    );
    append_live_event(
        shared,
        event_hub,
        &SupervisorLiveEventPayload::Activity {
            code: "autonomous_live_event_persist_failed".into(),
            title: "Autonomous live-event persistence deferred".into(),
            detail: Some(detail),
        },
    );
    let _ = persist_sidecar_checkpoint(
        repo_root,
        shared,
        persistence_lock,
        RuntimeRunStatus::Running,
        project_store::RuntimeRunCheckpointKind::State,
        activity_checkpoint_summary(
            "autonomous_live_event_persist_failed",
            "Autonomous live-event persistence deferred",
        ),
    );
}

fn should_persist_live_event_checkpoint(event: &BufferedSupervisorEvent, _summary: &str) -> bool {
    event.sequence == 1
        || event.sequence.is_multiple_of(16)
        || matches!(
            event.item,
            SupervisorLiveEventPayload::Tool { .. }
                | SupervisorLiveEventPayload::Skill { .. }
                | SupervisorLiveEventPayload::Activity { .. }
                | SupervisorLiveEventPayload::ActionRequired { .. }
        )
}

pub(super) fn append_live_event(
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    item: &SupervisorLiveEventPayload,
) -> BufferedSupervisorEvent {
    let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
    let mut hub = event_hub.lock().expect("event hub lock poisoned");
    hub.next_sequence = hub.next_sequence.saturating_add(1);
    let event = BufferedSupervisorEvent {
        project_id: snapshot.project_id,
        run_id: snapshot.run_id,
        sequence: hub.next_sequence,
        created_at: now_timestamp(),
        item: item.clone(),
    };

    if hub.ring.len() == LIVE_EVENT_RING_LIMIT {
        hub.ring.pop_front();
    }
    hub.ring.push_back(event.clone());

    let mut stale_subscribers = Vec::new();
    for (subscriber_id, sender) in &hub.subscribers {
        match sender.try_send(event.clone()) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                stale_subscribers.push(*subscriber_id);
            }
        }
    }

    for subscriber_id in stale_subscribers {
        hub.subscribers.remove(&subscriber_id);
    }

    event
}

fn summarize_pty_output(raw: &str) -> Option<String> {
    let sanitized = raw
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\t' => ' ',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect::<String>();
    let collapsed = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return None;
    }

    let bounded = if trimmed.chars().count() > 220 {
        let mut tail = trimmed
            .chars()
            .rev()
            .take(219)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        tail.insert(0, '…');
        tail
    } else {
        trimmed.to_string()
    };

    Some(format!("{SHELL_OUTPUT_PREFIX} {bounded}"))
}
