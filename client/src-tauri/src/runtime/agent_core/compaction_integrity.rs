use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::db::project_store::{
    AgentActionRequestRecord, AgentEventRecord, AgentFileChangeRecord, AgentMessageRecord,
    AgentMessageRole, AgentRunSnapshotRecord, AgentToolCallRecord,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompactionRunCoverage {
    WholeRun,
    ThroughTimestamp(String),
}

impl CompactionRunCoverage {
    pub(crate) fn includes_timestamp(&self, timestamp: &str) -> bool {
        match self {
            Self::WholeRun => true,
            Self::ThroughTimestamp(covered_through) => timestamp < covered_through.as_str(),
        }
    }

    pub(crate) fn includes_tool_call(&self, tool_call: &AgentToolCallRecord) -> bool {
        self.includes_timestamp(
            tool_call
                .completed_at
                .as_deref()
                .unwrap_or(tool_call.started_at.as_str()),
        )
    }

    pub(crate) fn includes_action(&self, action: &AgentActionRequestRecord) -> bool {
        self.includes_timestamp(
            action
                .resolved_at
                .as_deref()
                .unwrap_or(action.created_at.as_str()),
        )
    }

    pub(crate) fn includes_file_change(&self, file_change: &AgentFileChangeRecord) -> bool {
        self.includes_timestamp(&file_change.created_at)
    }

    pub(crate) fn includes_event(&self, event: &AgentEventRecord) -> bool {
        self.includes_timestamp(&event.created_at)
    }
}

pub(crate) fn compaction_run_coverage_for_snapshot(
    snapshot: &AgentRunSnapshotRecord,
    covered_messages: &[&AgentMessageRecord],
) -> Option<CompactionRunCoverage> {
    let covered_for_run = covered_messages
        .iter()
        .filter(|message| message.run_id == snapshot.run.run_id)
        .copied()
        .collect::<Vec<_>>();
    if covered_for_run.is_empty() {
        return None;
    }
    let compactable_message_count = snapshot
        .messages
        .iter()
        .filter(|message| message.role != AgentMessageRole::System)
        .count();
    if covered_for_run.len() == compactable_message_count {
        return Some(CompactionRunCoverage::WholeRun);
    }
    let covered_through = covered_for_run
        .into_iter()
        .max_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        })
        .map(|message| message.created_at.clone())?;
    Some(CompactionRunCoverage::ThroughTimestamp(covered_through))
}

pub(crate) fn canonical_compaction_source_hash<'a>(
    snapshots: impl IntoIterator<Item = &'a AgentRunSnapshotRecord>,
    covered_messages: &[&AgentMessageRecord],
    covered_events: &[&AgentEventRecord],
    run_coverage: &HashMap<String, CompactionRunCoverage>,
) -> String {
    let mut snapshots = snapshots
        .into_iter()
        .filter(|snapshot| run_coverage.contains_key(&snapshot.run.run_id))
        .collect::<Vec<_>>();
    snapshots.sort_by(|left, right| {
        left.run
            .started_at
            .cmp(&right.run.started_at)
            .then_with(|| left.run.run_id.cmp(&right.run.run_id))
    });

    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "schema", "xero.compaction_source.v2");
    for snapshot in snapshots {
        let Some(coverage) = run_coverage.get(&snapshot.run.run_id) else {
            continue;
        };
        hash_field(&mut hasher, "record", "run");
        hash_field(&mut hasher, "run_id", &snapshot.run.run_id);
        hash_field(&mut hasher, "provider_id", &snapshot.run.provider_id);
        hash_field(&mut hasher, "model_id", &snapshot.run.model_id);
        hash_field(&mut hasher, "prompt", &snapshot.run.prompt);

        let mut messages = covered_messages
            .iter()
            .filter(|message| message.run_id == snapshot.run.run_id)
            .copied()
            .collect::<Vec<_>>();
        messages.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        for message in messages {
            hash_field(&mut hasher, "record", "message");
            hash_i64(&mut hasher, "id", message.id);
            hash_field(&mut hasher, "run_id", &message.run_id);
            hash_field(&mut hasher, "role", &format!("{:?}", message.role));
            hash_field(&mut hasher, "content", &message.content);
            hash_field(&mut hasher, "created_at", &message.created_at);
        }

        let mut events = covered_events
            .iter()
            .filter(|event| event.run_id == snapshot.run.run_id && coverage.includes_event(event))
            .copied()
            .collect::<Vec<_>>();
        events.sort_by(|left, right| left.id.cmp(&right.id));
        for event in events {
            hash_field(&mut hasher, "record", "event");
            hash_i64(&mut hasher, "id", event.id);
            hash_field(&mut hasher, "run_id", &event.run_id);
            hash_field(
                &mut hasher,
                "event_kind",
                &format!("{:?}", event.event_kind),
            );
            hash_field(&mut hasher, "payload_json", &event.payload_json);
            hash_field(&mut hasher, "created_at", &event.created_at);
        }

        let mut tool_calls = snapshot
            .tool_calls
            .iter()
            .filter(|tool_call| coverage.includes_tool_call(tool_call))
            .collect::<Vec<_>>();
        tool_calls.sort_by(|left, right| {
            left.started_at
                .cmp(&right.started_at)
                .then_with(|| left.tool_call_id.cmp(&right.tool_call_id))
        });
        for tool_call in tool_calls {
            hash_field(&mut hasher, "record", "tool_call");
            hash_field(&mut hasher, "project_id", &tool_call.project_id);
            hash_field(&mut hasher, "run_id", &tool_call.run_id);
            hash_field(&mut hasher, "tool_call_id", &tool_call.tool_call_id);
            hash_field(&mut hasher, "tool_name", &tool_call.tool_name);
            hash_field(&mut hasher, "input_json", &tool_call.input_json);
            hash_field(&mut hasher, "state", &format!("{:?}", tool_call.state));
            hash_optional_field(&mut hasher, "result_json", tool_call.result_json.as_deref());
            hash_optional_field(
                &mut hasher,
                "error_code",
                tool_call.error.as_ref().map(|error| error.code.as_str()),
            );
            hash_optional_field(
                &mut hasher,
                "error_message",
                tool_call.error.as_ref().map(|error| error.message.as_str()),
            );
            hash_field(&mut hasher, "started_at", &tool_call.started_at);
            hash_optional_field(
                &mut hasher,
                "completed_at",
                tool_call.completed_at.as_deref(),
            );
        }

        let mut actions = snapshot
            .action_requests
            .iter()
            .filter(|action| coverage.includes_action(action))
            .collect::<Vec<_>>();
        actions.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.action_id.cmp(&right.action_id))
        });
        for action in actions {
            hash_field(&mut hasher, "record", "action");
            hash_field(&mut hasher, "project_id", &action.project_id);
            hash_field(&mut hasher, "run_id", &action.run_id);
            hash_field(&mut hasher, "action_id", &action.action_id);
            hash_field(&mut hasher, "action_type", &action.action_type);
            hash_field(&mut hasher, "title", &action.title);
            hash_field(&mut hasher, "detail", &action.detail);
            hash_field(&mut hasher, "status", &action.status);
            hash_field(&mut hasher, "created_at", &action.created_at);
            hash_optional_field(&mut hasher, "resolved_at", action.resolved_at.as_deref());
            hash_optional_field(&mut hasher, "response", action.response.as_deref());
        }

        let mut checkpoints = snapshot
            .checkpoints
            .iter()
            .filter(|checkpoint| coverage.includes_timestamp(&checkpoint.created_at))
            .collect::<Vec<_>>();
        checkpoints.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        for checkpoint in checkpoints {
            hash_field(&mut hasher, "record", "checkpoint");
            hash_i64(&mut hasher, "id", checkpoint.id);
            hash_field(&mut hasher, "project_id", &checkpoint.project_id);
            hash_field(&mut hasher, "run_id", &checkpoint.run_id);
            hash_field(&mut hasher, "checkpoint_kind", &checkpoint.checkpoint_kind);
            hash_field(&mut hasher, "summary", &checkpoint.summary);
            hash_optional_field(
                &mut hasher,
                "payload_json",
                checkpoint.payload_json.as_deref(),
            );
            hash_field(&mut hasher, "created_at", &checkpoint.created_at);
        }

        let mut file_changes = snapshot
            .file_changes
            .iter()
            .filter(|file_change| coverage.includes_file_change(file_change))
            .collect::<Vec<_>>();
        file_changes.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        for file_change in file_changes {
            hash_field(&mut hasher, "record", "file_change");
            hash_i64(&mut hasher, "id", file_change.id);
            hash_field(&mut hasher, "project_id", &file_change.project_id);
            hash_field(&mut hasher, "run_id", &file_change.run_id);
            hash_field(&mut hasher, "trace_id", &file_change.trace_id);
            hash_field(
                &mut hasher,
                "top_level_run_id",
                &file_change.top_level_run_id,
            );
            hash_optional_field(
                &mut hasher,
                "subagent_id",
                file_change.subagent_id.as_deref(),
            );
            hash_optional_field(
                &mut hasher,
                "subagent_role",
                file_change.subagent_role.as_deref(),
            );
            hash_optional_field(
                &mut hasher,
                "change_group_id",
                file_change.change_group_id.as_deref(),
            );
            hash_field(&mut hasher, "path", &file_change.path);
            hash_field(&mut hasher, "operation", &file_change.operation);
            hash_optional_field(&mut hasher, "old_hash", file_change.old_hash.as_deref());
            hash_optional_field(&mut hasher, "new_hash", file_change.new_hash.as_deref());
            hash_field(&mut hasher, "created_at", &file_change.created_at);
        }
    }
    format!("{:x}", hasher.finalize())
}

fn hash_i64(hasher: &mut Sha256, label: &str, value: i64) {
    hash_field(hasher, label, &value.to_string());
}

fn hash_optional_field(hasher: &mut Sha256, label: &str, value: Option<&str>) {
    match value {
        Some(value) => {
            hash_field(hasher, &format!("{label}.present"), "true");
            hash_field(hasher, label, value);
        }
        None => hash_field(hasher, &format!("{label}.present"), "false"),
    }
}

fn hash_field(hasher: &mut Sha256, label: &str, value: &str) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label.as_bytes());
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}
