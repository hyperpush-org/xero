use std::{
    cmp::Ordering,
    path::Path,
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{
    ipc::{Channel, JavaScriptChannelId},
    AppHandle, Runtime, State, Webview,
};

use crate::{
    commands::{
        validate_non_empty, BrowserComputerUseActionStatusDto, BrowserComputerUseSurfaceDto,
        BrowserComputerUseToolResultSummaryDto, CodePatchAvailabilityDto, CommandError,
        CommandResult, CommandToolResultSummaryDto, FileToolResultSummaryDto,
        GitToolResultScopeDto, GitToolResultSummaryDto, McpCapabilityKindDto,
        McpCapabilityToolResultSummaryDto, RuntimeActionAnswerShape,
        RuntimeActionRequiredOptionDto, RuntimeStreamIssueDto, RuntimeStreamItemDto,
        RuntimeStreamItemKind, RuntimeStreamPatchDto, RuntimeStreamPlanItemDto,
        RuntimeStreamPlanItemStatus, RuntimeStreamTranscriptRole, RuntimeStreamViewSnapshotDto,
        RuntimeStreamViewStatusDto, RuntimeToolCallState, SubscribeRuntimeStreamRequestDto,
        SubscribeRuntimeStreamResponseDto, ToolResultSummaryDto, WebToolResultContentKindDto,
        WebToolResultSummaryDto,
    },
    db::project_store::{
        self, AgentEventRecord, AgentRunEventKind, AgentRunStatus, RuntimeRunSnapshotRecord,
        RuntimeRunStatus,
    },
    runtime::{
        agent_core::serialize_model_visible_tool_result, subscribe_agent_events,
        AgentEventSubscription, AgentToolResult, OWNED_AGENT_SUPERVISOR_KIND,
    },
    state::DesktopState,
};

use super::runtime_support::{load_persisted_runtime_run, resolve_project_root};

const INCREMENTAL_RUNTIME_STREAM_REPLAY_LIMIT: usize = 200;
const RUNTIME_STREAM_VIEW_SCHEMA: &str = "xero.runtime_stream_view_snapshot.v1";
const RUNTIME_STREAM_PATCH_SCHEMA: &str = "xero.runtime_stream_patch.v1";
const MAX_RUNTIME_STREAM_ACTION_REQUIRED: usize = 10;
const OWNED_AGENT_REASONING_ACTIVITY_CODE: &str = "owned_agent_reasoning";
const RUNTIME_STREAM_IPC_MAX_BYTES: usize = 96 * 1024;
const RUNTIME_STREAM_IPC_PATCH_PREVIEW_CHARS: usize = 4_000;
const RUNTIME_STREAM_IPC_TIGHT_PREVIEW_CHARS: usize = 1_000;
const RUNTIME_STREAM_IPC_TEXT_CHARS: usize = 2_000;

#[derive(Debug, Clone)]
struct RuntimeStreamProjectionContext {
    project_id: String,
    agent_session_id: String,
    runtime_kind: String,
    run_id: String,
    session_id: String,
    flow_id: Option<String>,
    subscribed_item_kinds: Vec<RuntimeStreamItemKind>,
}

#[derive(Debug, Clone)]
struct RuntimeStreamProjection {
    context: RuntimeStreamProjectionContext,
    status: RuntimeStreamViewStatusDto,
    items: Vec<RuntimeStreamItemDto>,
    transcript_items: Vec<RuntimeStreamItemDto>,
    tool_calls: Vec<RuntimeStreamItemDto>,
    skill_items: Vec<RuntimeStreamItemDto>,
    activity_items: Vec<RuntimeStreamItemDto>,
    action_required: Vec<RuntimeStreamItemDto>,
    plan: Option<RuntimeStreamItemDto>,
    completion: Option<RuntimeStreamItemDto>,
    failure: Option<RuntimeStreamItemDto>,
    last_issue: Option<RuntimeStreamIssueDto>,
    last_item_at: Option<String>,
    last_sequence: Option<u64>,
}

impl RuntimeStreamProjection {
    fn new(context: RuntimeStreamProjectionContext) -> Self {
        Self {
            context,
            status: RuntimeStreamViewStatusDto::Subscribing,
            items: Vec::new(),
            transcript_items: Vec::new(),
            tool_calls: Vec::new(),
            skill_items: Vec::new(),
            activity_items: Vec::new(),
            action_required: Vec::new(),
            plan: None,
            completion: None,
            failure: None,
            last_issue: None,
            last_item_at: None,
            last_sequence: None,
        }
    }

    fn apply_item(&mut self, item: RuntimeStreamItemDto) -> RuntimeStreamPatchDto {
        let previous_timeline_item = latest_timeline_item(&self.items).cloned();
        let patch_item = item.clone();
        self.last_item_at = Some(item.created_at.clone());
        self.last_sequence = Some(item.sequence);

        match &item.kind {
            RuntimeStreamItemKind::Complete => {
                self.status = RuntimeStreamViewStatusDto::Complete;
                self.completion = Some(item.clone());
                self.failure = None;
                self.last_issue = None;
            }
            RuntimeStreamItemKind::Failure => {
                let retryable = item.retryable.unwrap_or(false);
                self.status = if retryable {
                    RuntimeStreamViewStatusDto::Stale
                } else {
                    RuntimeStreamViewStatusDto::Error
                };
                self.last_issue = Some(RuntimeStreamIssueDto {
                    code: item
                        .code
                        .clone()
                        .unwrap_or_else(|| "runtime_stream_failure".into()),
                    message: item
                        .message
                        .clone()
                        .unwrap_or_else(|| "Runtime stream failed.".into()),
                    retryable,
                    observed_at: item.created_at.clone(),
                });
                self.failure = Some(item.clone());
            }
            _ => {
                self.status = RuntimeStreamViewStatusDto::Live;
                self.failure = None;
                self.last_issue = None;
            }
        }

        let timeline_item = match item.kind.clone() {
            RuntimeStreamItemKind::Transcript => {
                self.transcript_items = merge_transcript_items(
                    &self.transcript_items,
                    item,
                    previous_timeline_item.as_ref(),
                );
                None
            }
            RuntimeStreamItemKind::Tool => {
                self.tool_calls = replace_tool_call_item(&self.tool_calls, item.clone());
                Some(item)
            }
            RuntimeStreamItemKind::Skill => {
                self.skill_items = push_item(&self.skill_items, item.clone());
                Some(item)
            }
            RuntimeStreamItemKind::Activity => {
                self.activity_items = push_item(&self.activity_items, item.clone());
                Some(item)
            }
            RuntimeStreamItemKind::ActionRequired => {
                self.action_required = cap_recent(
                    replace_action_required_item(&self.action_required, item.clone()),
                    MAX_RUNTIME_STREAM_ACTION_REQUIRED,
                );
                Some(item)
            }
            RuntimeStreamItemKind::Plan => {
                self.plan = Some(item.clone());
                Some(item)
            }
            RuntimeStreamItemKind::Complete | RuntimeStreamItemKind::Failure => Some(item),
            RuntimeStreamItemKind::SubagentLifecycle => Some(item),
        };

        self.items = project_timeline_items(
            &self.items,
            &self.transcript_items,
            timeline_item,
            previous_timeline_item.as_ref(),
        );

        RuntimeStreamPatchDto {
            schema: RUNTIME_STREAM_PATCH_SCHEMA.into(),
            item: patch_item,
            snapshot: self.snapshot(),
        }
    }

    fn snapshot(&self) -> RuntimeStreamViewSnapshotDto {
        RuntimeStreamViewSnapshotDto {
            schema: RUNTIME_STREAM_VIEW_SCHEMA.into(),
            project_id: self.context.project_id.clone(),
            agent_session_id: self.context.agent_session_id.clone(),
            runtime_kind: self.context.runtime_kind.clone(),
            run_id: self.context.run_id.clone(),
            session_id: self.context.session_id.clone(),
            flow_id: self.context.flow_id.clone(),
            subscribed_item_kinds: self.context.subscribed_item_kinds.clone(),
            status: self.status.clone(),
            items: self.items.clone(),
            transcript_items: self.transcript_items.clone(),
            tool_calls: self.tool_calls.clone(),
            skill_items: self.skill_items.clone(),
            activity_items: self.activity_items.clone(),
            action_required: self.action_required.clone(),
            plan: self.plan.clone(),
            completion: self.completion.clone(),
            failure: self.failure.clone(),
            last_issue: self.last_issue.clone(),
            last_item_at: self.last_item_at.clone(),
            last_sequence: self.last_sequence,
        }
    }
}

fn push_item(
    items: &[RuntimeStreamItemDto],
    item: RuntimeStreamItemDto,
) -> Vec<RuntimeStreamItemDto> {
    let mut next = Vec::with_capacity(items.len() + 1);
    next.extend(items.iter().cloned());
    next.push(item);
    next
}

fn cap_recent(mut items: Vec<RuntimeStreamItemDto>, limit: usize) -> Vec<RuntimeStreamItemDto> {
    if items.len() <= limit {
        return items;
    }

    let keep_from = items.len().saturating_sub(limit);
    items.split_off(keep_from)
}

fn runtime_timeline_update_sequence(item: &RuntimeStreamItemDto) -> u64 {
    item.updated_sequence.unwrap_or(item.sequence)
}

fn runtime_item_kind_sort_key(kind: &RuntimeStreamItemKind) -> u8 {
    match kind {
        RuntimeStreamItemKind::Activity => 0,
        RuntimeStreamItemKind::ActionRequired => 1,
        RuntimeStreamItemKind::Complete => 2,
        RuntimeStreamItemKind::Failure => 3,
        RuntimeStreamItemKind::Plan => 4,
        RuntimeStreamItemKind::Skill => 5,
        RuntimeStreamItemKind::SubagentLifecycle => 6,
        RuntimeStreamItemKind::Tool => 7,
        RuntimeStreamItemKind::Transcript => 8,
    }
}

fn compare_runtime_stream_items(
    left: &RuntimeStreamItemDto,
    right: &RuntimeStreamItemDto,
) -> Ordering {
    left.sequence
        .cmp(&right.sequence)
        .then_with(|| {
            runtime_item_kind_sort_key(&left.kind).cmp(&runtime_item_kind_sort_key(&right.kind))
        })
        .then_with(|| left.run_id.cmp(&right.run_id))
        .then_with(|| left.created_at.cmp(&right.created_at))
}

fn compare_runtime_stream_item_updates(
    left: &RuntimeStreamItemDto,
    right: &RuntimeStreamItemDto,
) -> Ordering {
    runtime_timeline_update_sequence(left)
        .cmp(&runtime_timeline_update_sequence(right))
        .then_with(|| compare_runtime_stream_items(left, right))
}

fn latest_timeline_item(items: &[RuntimeStreamItemDto]) -> Option<&RuntimeStreamItemDto> {
    items
        .iter()
        .max_by(|left, right| compare_runtime_stream_item_updates(left, right))
}

fn transcript_role_is_assistant(item: &RuntimeStreamItemDto) -> bool {
    matches!(
        item.transcript_role.as_ref(),
        None | Some(RuntimeStreamTranscriptRole::Assistant)
    )
}

fn same_runtime_timeline_identity(
    left: &RuntimeStreamItemDto,
    right: &RuntimeStreamItemDto,
) -> bool {
    left.kind.as_str() == right.kind.as_str()
        && left.run_id == right.run_id
        && left.sequence == right.sequence
}

fn merge_transcript_items(
    current_items: &[RuntimeStreamItemDto],
    next_item: RuntimeStreamItemDto,
    previous_timeline_item: Option<&RuntimeStreamItemDto>,
) -> Vec<RuntimeStreamItemDto> {
    if !transcript_role_is_assistant(&next_item) {
        return push_item(current_items, next_item);
    }

    let Some(previous_item) = current_items.last() else {
        return push_item(current_items, next_item);
    };

    let can_merge = previous_item.run_id == next_item.run_id
        && transcript_role_is_assistant(previous_item)
        && previous_timeline_item
            .map(|timeline_item| {
                matches!(&timeline_item.kind, RuntimeStreamItemKind::Transcript)
                    && same_runtime_timeline_identity(timeline_item, previous_item)
            })
            .unwrap_or(false);

    if !can_merge {
        return push_item(current_items, next_item);
    }

    let mut merged_item = previous_item.clone();
    merged_item.updated_sequence = Some(next_item.sequence);
    merged_item.text = Some(format!(
        "{}{}",
        previous_item.text.as_deref().unwrap_or_default(),
        next_item.text.as_deref().unwrap_or_default()
    ));

    let mut merged_items = current_items.to_vec();
    if let Some(last_item) = merged_items.last_mut() {
        *last_item = merged_item;
    }
    merged_items
}

fn replace_tool_call_item(
    current_items: &[RuntimeStreamItemDto],
    next_item: RuntimeStreamItemDto,
) -> Vec<RuntimeStreamItemDto> {
    let Some(tool_call_id) = next_item.tool_call_id.as_deref() else {
        return push_item(current_items, next_item);
    };

    let mut items = current_items
        .iter()
        .filter(|item| item.tool_call_id.as_deref() != Some(tool_call_id))
        .cloned()
        .collect::<Vec<_>>();
    items.push(next_item);
    items
}

fn replace_action_required_item(
    current_items: &[RuntimeStreamItemDto],
    next_item: RuntimeStreamItemDto,
) -> Vec<RuntimeStreamItemDto> {
    let Some(action_id) = next_item.action_id.as_deref() else {
        return push_item(current_items, next_item);
    };

    let mut items = current_items
        .iter()
        .filter(|item| {
            !(item.run_id == next_item.run_id && item.action_id.as_deref() == Some(action_id))
        })
        .cloned()
        .collect::<Vec<_>>();
    items.push(next_item);
    items
}

fn replace_plan_timeline_item(
    current_items: &[RuntimeStreamItemDto],
    next_item: RuntimeStreamItemDto,
) -> Vec<RuntimeStreamItemDto> {
    let Some(plan_id) = next_item.plan_id.as_deref() else {
        return push_item(current_items, next_item);
    };

    let mut items = current_items
        .iter()
        .filter(|item| {
            !(item.run_id == next_item.run_id && item.plan_id.as_deref() == Some(plan_id))
        })
        .cloned()
        .collect::<Vec<_>>();
    items.push(next_item);
    items
}

fn merge_timeline_tool_item(
    current_items: &[RuntimeStreamItemDto],
    next_item: RuntimeStreamItemDto,
) -> Vec<RuntimeStreamItemDto> {
    let Some(tool_call_id) = next_item.tool_call_id.as_deref() else {
        return push_item(current_items, next_item);
    };

    let Some(existing_item_index) = current_items.iter().position(|item| {
        matches!(&item.kind, RuntimeStreamItemKind::Tool)
            && item.tool_call_id.as_deref() == Some(tool_call_id)
    }) else {
        return push_item(current_items, next_item);
    };

    let existing_item = &current_items[existing_item_index];
    let updated_sequence = runtime_timeline_update_sequence(&next_item);
    let mut merged_item = next_item;
    merged_item.sequence = existing_item.sequence;
    merged_item.created_at = existing_item.created_at.clone();
    merged_item.updated_sequence = Some(updated_sequence);

    current_items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            if index == existing_item_index {
                merged_item.clone()
            } else {
                item.clone()
            }
        })
        .collect()
}

fn reasoning_activity_text(item: &RuntimeStreamItemDto) -> &str {
    item.text
        .as_deref()
        .or(item.detail.as_deref())
        .unwrap_or_default()
}

fn is_reasoning_activity_item(item: &RuntimeStreamItemDto) -> bool {
    matches!(&item.kind, RuntimeStreamItemKind::Activity)
        && item.code.as_deref() == Some(OWNED_AGENT_REASONING_ACTIVITY_CODE)
        && !reasoning_activity_text(item).trim().is_empty()
}

fn merge_reasoning_timeline_item(
    current_items: &[RuntimeStreamItemDto],
    next_item: RuntimeStreamItemDto,
    previous_timeline_item: Option<&RuntimeStreamItemDto>,
) -> Vec<RuntimeStreamItemDto> {
    let Some(previous_timeline_item) = previous_timeline_item else {
        return push_item(current_items, next_item);
    };

    if !is_reasoning_activity_item(previous_timeline_item)
        || previous_timeline_item.run_id != next_item.run_id
    {
        return push_item(current_items, next_item);
    }

    let Some(previous_item_index) = current_items
        .iter()
        .position(|item| same_runtime_timeline_identity(item, previous_timeline_item))
    else {
        return push_item(current_items, next_item);
    };

    let previous_item = &current_items[previous_item_index];
    if !is_reasoning_activity_item(previous_item) {
        return push_item(current_items, next_item);
    }

    let merged_text = format!(
        "{}{}",
        reasoning_activity_text(previous_item),
        reasoning_activity_text(&next_item)
    );
    let merged_detail = if merged_text.trim().is_empty() {
        previous_item
            .detail
            .clone()
            .or_else(|| next_item.detail.clone())
    } else {
        Some(merged_text.trim().to_owned())
    };
    let mut merged_item = previous_item.clone();
    merged_item.updated_sequence = Some(next_item.sequence);
    merged_item.text = Some(merged_text);
    merged_item.detail = merged_detail;

    current_items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            if index == previous_item_index {
                merged_item.clone()
            } else {
                item.clone()
            }
        })
        .collect()
}

fn project_timeline_items(
    current_items: &[RuntimeStreamItemDto],
    transcript_items: &[RuntimeStreamItemDto],
    next_item: Option<RuntimeStreamItemDto>,
    previous_timeline_item: Option<&RuntimeStreamItemDto>,
) -> Vec<RuntimeStreamItemDto> {
    let mut non_transcript_items = current_items
        .iter()
        .filter(|item| !matches!(&item.kind, RuntimeStreamItemKind::Transcript))
        .cloned()
        .collect::<Vec<_>>();

    if let Some(next_item) = next_item {
        non_transcript_items = match next_item.kind.clone() {
            RuntimeStreamItemKind::Tool => {
                merge_timeline_tool_item(&non_transcript_items, next_item)
            }
            RuntimeStreamItemKind::ActionRequired => {
                replace_action_required_item(&non_transcript_items, next_item)
            }
            RuntimeStreamItemKind::Plan => {
                replace_plan_timeline_item(&non_transcript_items, next_item)
            }
            RuntimeStreamItemKind::Activity if is_reasoning_activity_item(&next_item) => {
                merge_reasoning_timeline_item(
                    &non_transcript_items,
                    next_item,
                    previous_timeline_item,
                )
            }
            RuntimeStreamItemKind::Transcript => non_transcript_items,
            _ => push_item(&non_transcript_items, next_item),
        };
    }

    let mut projected_items =
        Vec::with_capacity(transcript_items.len() + non_transcript_items.len());
    projected_items.extend(transcript_items.iter().cloned());
    projected_items.extend(non_transcript_items);
    projected_items.sort_by(compare_runtime_stream_items);
    projected_items
}

#[tauri::command]
pub async fn subscribe_runtime_stream<R: Runtime + 'static>(
    app: AppHandle<R>,
    webview: Webview<R>,
    state: State<'_, DesktopState>,
    request: SubscribeRuntimeStreamRequestDto,
) -> CommandResult<SubscribeRuntimeStreamResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;

    let item_kinds = parse_requested_item_kinds(&request.item_kinds)?;
    let channel = resolve_channel(&webview, request.channel.as_deref())?;
    let state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let repo_root = resolve_project_root(&app, &state, &request.project_id)?;
        let runtime_run =
            load_persisted_runtime_run(&repo_root, &request.project_id, &request.agent_session_id)?
                .filter(|snapshot| snapshot.run.supervisor_kind == OWNED_AGENT_SUPERVISOR_KIND)
                .ok_or_else(|| {
                    CommandError::retryable(
                        "runtime_stream_run_unavailable",
                        "Xero cannot start a live runtime stream until the selected project has a Xero-owned agent run.",
                    )
                })?;

        subscribe_owned_runtime_stream(&repo_root, &request, runtime_run, item_kinds, channel)
    })
    .await
    .map_err(|error| {
        CommandError::system_fault(
            "runtime_stream_subscribe_task_failed",
            format!("Xero could not finish background runtime-stream subscription work: {error}"),
        )
    })?
}

fn parse_requested_item_kinds(item_kinds: &[String]) -> CommandResult<Vec<RuntimeStreamItemKind>> {
    if item_kinds.is_empty() {
        return Err(CommandError::user_fixable(
            "invalid_request",
            "Field `itemKinds` must contain at least one allowed runtime stream item kind.",
        ));
    }

    let mut parsed = Vec::with_capacity(item_kinds.len());
    for kind in item_kinds {
        let kind = parse_runtime_stream_item_kind(kind)?;
        if !parsed.contains(&kind) {
            parsed.push(kind);
        }
    }

    Ok(parsed)
}

fn parse_runtime_stream_item_kind(value: &str) -> CommandResult<RuntimeStreamItemKind> {
    match value {
        "transcript" => Ok(RuntimeStreamItemKind::Transcript),
        "tool" => Ok(RuntimeStreamItemKind::Tool),
        "skill" => Ok(RuntimeStreamItemKind::Skill),
        "activity" => Ok(RuntimeStreamItemKind::Activity),
        "action_required" => Ok(RuntimeStreamItemKind::ActionRequired),
        "plan" => Ok(RuntimeStreamItemKind::Plan),
        "complete" => Ok(RuntimeStreamItemKind::Complete),
        "failure" => Ok(RuntimeStreamItemKind::Failure),
        "subagent_lifecycle" => Ok(RuntimeStreamItemKind::SubagentLifecycle),
        other => Err(CommandError::user_fixable(
            "runtime_stream_item_kind_unsupported",
            format!(
                "Xero does not support runtime stream item kind `{other}`. Allowed kinds: {}.",
                RuntimeStreamItemDto::allowed_kind_names().join(", ")
            ),
        )),
    }
}

fn subscribe_owned_runtime_stream(
    repo_root: &Path,
    request: &SubscribeRuntimeStreamRequestDto,
    runtime_run: RuntimeRunSnapshotRecord,
    item_kinds: Vec<RuntimeStreamItemKind>,
    channel: Channel<serde_json::Value>,
) -> CommandResult<SubscribeRuntimeStreamResponseDto> {
    let run_id = runtime_run.run.run_id.clone();
    let runtime_kind = runtime_run.run.runtime_kind.clone();
    let runtime_terminal = matches!(
        runtime_run.run.status,
        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
    );
    let session_id = format!("owned-agent:{run_id}");
    let subscription = subscribe_agent_events(&request.project_id, &run_id);
    let mut projection = RuntimeStreamProjection::new(RuntimeStreamProjectionContext {
        project_id: request.project_id.clone(),
        agent_session_id: request.agent_session_id.clone(),
        runtime_kind: runtime_kind.clone(),
        run_id: run_id.clone(),
        session_id: session_id.clone(),
        flow_id: None,
        subscribed_item_kinds: item_kinds.clone(),
    });
    let (last_event_id, terminal) = replay_owned_agent_events(
        repo_root,
        &request.project_id,
        &run_id,
        &session_id,
        &item_kinds,
        &channel,
        &mut projection,
        request.after_sequence,
        request.replay_limit,
    )?;

    if !terminal && !runtime_terminal {
        let requested_item_kinds = item_kinds.clone();
        let project_id = request.project_id.clone();
        let run_id_for_thread = run_id.clone();
        let session_id_for_thread = session_id.clone();
        thread::spawn(move || {
            stream_live_owned_agent_events(
                subscription,
                channel,
                project_id,
                run_id_for_thread,
                session_id_for_thread,
                requested_item_kinds,
                projection,
                last_event_id,
            );
        });
    }

    Ok(SubscribeRuntimeStreamResponseDto {
        project_id: request.project_id.clone(),
        agent_session_id: request.agent_session_id.clone(),
        runtime_kind,
        run_id,
        session_id,
        flow_id: None,
        subscribed_item_kinds: item_kinds,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "stream replay needs the subscription cursor and channel context together"
)]
fn replay_owned_agent_events(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    session_id: &str,
    item_kinds: &[RuntimeStreamItemKind],
    channel: &Channel<serde_json::Value>,
    projection: &mut RuntimeStreamProjection,
    after_sequence: Option<u64>,
    replay_limit: Option<u16>,
) -> CommandResult<(i64, bool)> {
    let started = Instant::now();
    let run = match project_store::load_agent_run_record(repo_root, project_id, run_id) {
        Ok(run) => run,
        Err(error) if error.code == "agent_run_not_found" => return Ok((0, false)),
        Err(error) => return Err(error),
    };
    let terminal = matches!(
        run.status,
        AgentRunStatus::Paused
            | AgentRunStatus::Cancelled
            | AgentRunStatus::HandedOff
            | AgentRunStatus::Completed
            | AgentRunStatus::Failed
    );
    let incremental_replay_limit = replay_limit
        .map(usize::from)
        .unwrap_or(INCREMENTAL_RUNTIME_STREAM_REPLAY_LIMIT);
    let after_event_id = after_sequence
        .and_then(|sequence| i64::try_from(sequence).ok())
        .unwrap_or(0);
    let (events, replay_mode) = load_owned_agent_replay_events(
        repo_root,
        project_id,
        run_id,
        after_sequence,
        replay_limit,
        incremental_replay_limit,
    )?;
    let mut last_event_id = after_event_id;
    let replayed_count = events.len();
    let mut delivered_patch = false;
    let mut last_projected_patch: Option<RuntimeStreamPatchDto> = None;
    for event in events {
        last_event_id = last_event_id.max(event.id);
        if let Some(item) = owned_agent_event_runtime_item(event, session_id, None) {
            if should_emit_owned_runtime_item(item_kinds, &item.kind) {
                let should_deliver_patch = after_sequence
                    .map(|sequence| item.sequence > sequence)
                    .unwrap_or(true);
                let patch = projection.apply_item(item);
                if should_deliver_patch {
                    send_runtime_stream_patch(channel, patch)?;
                    delivered_patch = true;
                } else {
                    last_projected_patch = Some(patch);
                }
            }
        }
    }
    if !delivered_patch {
        if let (Some(_), Some(patch)) = (after_sequence, last_projected_patch) {
            send_runtime_stream_patch(channel, patch)?;
        }
    }
    eprintln!(
        "[runtime-latency] subscribe_runtime_stream replay project_id={project_id} run_id={run_id} after_event_id={after_event_id} mode={replay_mode} incremental_limit={incremental_replay_limit} replayed_count={replayed_count} last_event_id={last_event_id} duration_ms={}",
        started.elapsed().as_millis()
    );
    Ok((last_event_id, terminal))
}

fn load_owned_agent_replay_events(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    after_sequence: Option<u64>,
    replay_limit: Option<u16>,
    incremental_replay_limit: usize,
) -> CommandResult<(Vec<AgentEventRecord>, &'static str)> {
    if after_sequence.is_none() && replay_limit.is_none() {
        return Ok((
            project_store::read_all_agent_events(repo_root, project_id, run_id)?,
            "full",
        ));
    }

    let replay_mode = if after_sequence.is_some() {
        "incremental"
    } else {
        "limited-full"
    };
    Ok((
        project_store::read_latest_agent_events(
            repo_root,
            project_id,
            run_id,
            incremental_replay_limit,
        )?,
        replay_mode,
    ))
}

fn stream_live_owned_agent_events(
    subscription: AgentEventSubscription,
    channel: Channel<serde_json::Value>,
    project_id: String,
    run_id: String,
    session_id: String,
    item_kinds: Vec<RuntimeStreamItemKind>,
    mut projection: RuntimeStreamProjection,
    mut last_event_id: i64,
) {
    const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
    loop {
        let event = match subscription.recv_timeout(IDLE_TIMEOUT) {
            Ok(event) => event,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if event.project_id != project_id || event.run_id != run_id || event.id <= last_event_id {
            continue;
        }
        let terminal = matches!(
            event.event_kind,
            AgentRunEventKind::RunPaused
                | AgentRunEventKind::RunCompleted
                | AgentRunEventKind::RunFailed
        );
        last_event_id = event.id;
        if let Some(item) = owned_agent_event_runtime_item(event, &session_id, None) {
            if should_emit_owned_runtime_item(&item_kinds, &item.kind) {
                let patch = projection.apply_item(item);
                if send_runtime_stream_patch(&channel, patch).is_err() {
                    return;
                }
            }
        }
        if terminal {
            break;
        }
    }
}

fn send_runtime_stream_patch(
    channel: &Channel<serde_json::Value>,
    patch: RuntimeStreamPatchDto,
) -> CommandResult<()> {
    let payload = runtime_stream_patch_payload_for_ipc(patch)?;
    channel.send(payload).map_err(|error| {
        CommandError::retryable(
            "runtime_stream_channel_closed",
            format!(
                "Xero could not deliver an owned-agent runtime stream replay patch because the desktop channel closed: {error}"
            ),
        )
    })
}

fn runtime_stream_patch_payload_for_ipc(
    patch: RuntimeStreamPatchDto,
) -> CommandResult<serde_json::Value> {
    let mut compact_patch = patch;
    compact_runtime_stream_patch_for_ipc(
        &mut compact_patch,
        RUNTIME_STREAM_IPC_PATCH_PREVIEW_CHARS,
    );
    if estimated_ipc_payload_bytes(&compact_patch) <= RUNTIME_STREAM_IPC_MAX_BYTES {
        return runtime_stream_payload_value(compact_patch);
    }

    compact_runtime_stream_patch_for_ipc(
        &mut compact_patch,
        RUNTIME_STREAM_IPC_TIGHT_PREVIEW_CHARS,
    );
    if estimated_ipc_payload_bytes(&compact_patch) <= RUNTIME_STREAM_IPC_MAX_BYTES {
        return runtime_stream_payload_value(compact_patch);
    }

    compact_runtime_stream_patch_for_ipc(&mut compact_patch, 0);
    if estimated_ipc_payload_bytes(&compact_patch) <= RUNTIME_STREAM_IPC_MAX_BYTES {
        return runtime_stream_payload_value(compact_patch);
    }

    let mut item = compact_patch.item;
    compact_runtime_stream_item_for_ipc(&mut item, RUNTIME_STREAM_IPC_TIGHT_PREVIEW_CHARS);
    if estimated_ipc_payload_bytes(&item) > RUNTIME_STREAM_IPC_MAX_BYTES {
        compact_runtime_stream_item_for_ipc(&mut item, 0);
    }
    runtime_stream_payload_value(item)
}

fn runtime_stream_payload_value<T: Serialize>(payload: T) -> CommandResult<serde_json::Value> {
    serde_json::to_value(payload).map_err(|error| {
        CommandError::system_fault(
            "runtime_stream_payload_encode_failed",
            format!("Xero could not encode a runtime stream channel payload: {error}"),
        )
    })
}

fn estimated_ipc_payload_bytes<T: Serialize>(payload: &T) -> usize {
    serde_json::to_string(payload)
        .map(|json| json.len().saturating_mul(2))
        .unwrap_or(usize::MAX)
}

fn compact_runtime_stream_patch_for_ipc(patch: &mut RuntimeStreamPatchDto, preview_max: usize) {
    compact_runtime_stream_item_for_ipc(&mut patch.item, preview_max);
    compact_runtime_stream_items_for_ipc(&mut patch.snapshot.items, preview_max);
    compact_runtime_stream_items_for_ipc(&mut patch.snapshot.transcript_items, preview_max);
    compact_runtime_stream_items_for_ipc(&mut patch.snapshot.tool_calls, preview_max);
    compact_runtime_stream_items_for_ipc(&mut patch.snapshot.skill_items, preview_max);
    compact_runtime_stream_items_for_ipc(&mut patch.snapshot.activity_items, preview_max);
    compact_runtime_stream_items_for_ipc(&mut patch.snapshot.action_required, preview_max);
    if let Some(item) = &mut patch.snapshot.plan {
        compact_runtime_stream_item_for_ipc(item, preview_max);
    }
    if let Some(item) = &mut patch.snapshot.completion {
        compact_runtime_stream_item_for_ipc(item, preview_max);
    }
    if let Some(item) = &mut patch.snapshot.failure {
        compact_runtime_stream_item_for_ipc(item, preview_max);
    }
}

fn compact_runtime_stream_items_for_ipc(items: &mut [RuntimeStreamItemDto], preview_max: usize) {
    for item in items {
        compact_runtime_stream_item_for_ipc(item, preview_max);
    }
}

fn compact_runtime_stream_item_for_ipc(item: &mut RuntimeStreamItemDto, preview_max: usize) {
    truncate_optional_runtime_text(&mut item.tool_result_preview, preview_max);
    if item.kind != RuntimeStreamItemKind::Transcript {
        truncate_optional_runtime_text(&mut item.text, RUNTIME_STREAM_IPC_TEXT_CHARS);
        truncate_optional_runtime_text(&mut item.detail, RUNTIME_STREAM_IPC_TEXT_CHARS);
        truncate_optional_runtime_text(&mut item.message, RUNTIME_STREAM_IPC_TEXT_CHARS);
    }
    truncate_optional_runtime_text(&mut item.subagent_prompt, RUNTIME_STREAM_IPC_TEXT_CHARS);
    truncate_optional_runtime_text(
        &mut item.subagent_result_summary,
        RUNTIME_STREAM_IPC_TEXT_CHARS,
    );
    if let Some(options) = &mut item.options {
        for option in options {
            truncate_optional_runtime_text(&mut option.description, RUNTIME_STREAM_IPC_TEXT_CHARS);
        }
    }
    if let Some(plan_items) = &mut item.plan_items {
        for plan_item in plan_items {
            truncate_optional_runtime_text(&mut plan_item.notes, RUNTIME_STREAM_IPC_TEXT_CHARS);
            truncate_optional_runtime_text(
                &mut plan_item.handoff_note,
                RUNTIME_STREAM_IPC_TEXT_CHARS,
            );
        }
    }
}

fn truncate_optional_runtime_text(value: &mut Option<String>, max_chars: usize) {
    if value.is_none() {
        return;
    }

    if max_chars == 0 {
        *value = None;
        return;
    }

    if let Some(current) = value.as_mut() {
        if current.chars().count() > max_chars {
            *current = truncate_chars(current, max_chars);
        }
    }
}

fn owned_agent_event_runtime_item(
    event: AgentEventRecord,
    session_id: &str,
    flow_id: Option<String>,
) -> Option<RuntimeStreamItemDto> {
    let event_id = event.id;
    let event_kind = event.event_kind.clone();
    let project_id = event.project_id.clone();
    let payload = serde_json::from_str::<serde_json::Value>(&event.payload_json).unwrap_or_else(
        |error| {
            serde_json::json!({
                "code": "owned_agent_event_decode_failed",
                "message": format!("Xero could not decode owned-agent event payload {event_id}: {error}"),
                "retryable": false,
            })
        },
    );
    let mut item = RuntimeStreamItemDto {
        kind: RuntimeStreamItemKind::Activity,
        run_id: event.run_id.clone(),
        sequence: event_id.max(0) as u64,
        updated_sequence: None,
        session_id: Some(session_id.to_string()),
        flow_id,
        text: None,
        transcript_role: None,
        tool_call_id: None,
        tool_name: None,
        tool_state: None,
        code_change_group_id: None,
        code_commit_id: None,
        code_workspace_epoch: None,
        code_patch_availability: None,
        tool_summary: None,
        tool_result_preview: None,
        skill_id: None,
        skill_stage: None,
        skill_result: None,
        skill_source: None,
        skill_cache_status: None,
        skill_diagnostic: None,
        action_id: None,
        boundary_id: None,
        action_type: None,
        answer_shape: None,
        options: None,
        allow_multiple: None,
        title: None,
        detail: None,
        plan_id: None,
        plan_items: None,
        plan_last_changed_id: None,
        code: None,
        message: None,
        retryable: None,
        subagent_id: None,
        subagent_role: None,
        subagent_role_label: None,
        subagent_run_id: None,
        subagent_status: None,
        subagent_used_tool_calls: None,
        subagent_max_tool_calls: None,
        subagent_used_tokens: None,
        subagent_max_tokens: None,
        subagent_used_cost_micros: None,
        subagent_max_cost_micros: None,
        subagent_result_summary: None,
        subagent_prompt: None,
        created_at: event.created_at,
    };
    item.code_change_group_id = code_change_group_id_from_payload(&payload);
    item.code_commit_id = code_commit_id_from_payload(&payload);
    item.code_workspace_epoch = code_workspace_epoch_from_payload(&payload);
    item.code_patch_availability = code_patch_availability_from_payload(
        &payload,
        &project_id,
        item.code_change_group_id.as_deref(),
    );
    item.subagent_id = payload_string(&payload, "subagentId");
    item.subagent_role = payload_string(&payload, "subagentRole");
    item.subagent_role_label = payload_string(&payload, "subagentRoleLabel");

    match event_kind {
        AgentRunEventKind::RunStarted => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_run_started".into());
            item.title = Some("Run started".into());
            item.detail = payload_string(&payload, "message")
                .or_else(|| Some("Owned agent run started.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::MessageDelta => {
            item.kind = RuntimeStreamItemKind::Transcript;
            item.text = payload_verbatim_string(&payload, "text");
            item.transcript_role = payload_transcript_role(&payload);
        }
        AgentRunEventKind::ReasoningSummary => {
            item.kind = RuntimeStreamItemKind::Activity;
            if payload.get("usage").is_some() {
                item.code = Some("owned_agent_usage".into());
                item.title = Some("Provider usage".into());
                item.detail = payload_string(&payload, "summary")
                    .or_else(|| Some("Provider usage updated.".into()));
                item.text = item.detail.clone();
            } else {
                item.code = Some("owned_agent_reasoning".into());
                item.title = Some("Reasoning".into());
                item.text = payload_verbatim_string(&payload, "summary");
                item.detail = payload_string(&payload, "summary")
                    .or_else(|| Some("Owned agent reasoning summary updated.".into()));
            }
        }
        AgentRunEventKind::ToolStarted => {
            item.kind = RuntimeStreamItemKind::Tool;
            item.tool_call_id = payload_string(&payload, "toolCallId");
            item.tool_name = payload_string(&payload, "toolName");
            item.tool_state = Some(RuntimeToolCallState::Running);
            item.detail = payload
                .get("input")
                .and_then(|input| tool_started_detail(item.tool_name.as_deref(), input))
                .or_else(|| {
                    item.tool_name
                        .as_ref()
                        .map(|tool_name| format!("Started `{tool_name}`."))
                });
            item.text = item
                .tool_name
                .as_ref()
                .map(|tool_name| format!("Started `{tool_name}`."));
        }
        AgentRunEventKind::ToolDelta => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_tool_delta".into());
            item.title = Some("Tool arguments".into());
            item.tool_call_id = payload_string(&payload, "toolCallId");
            item.tool_name = payload_string(&payload, "toolName");
            item.detail = payload_string(&payload, "argumentsDelta")
                .or_else(|| Some("Provider streamed tool-call arguments.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ToolCompleted => {
            item.kind = RuntimeStreamItemKind::Tool;
            item.tool_call_id = payload_string(&payload, "toolCallId");
            item.tool_name = payload_string(&payload, "toolName");
            let ok = payload_bool(&payload, "ok").unwrap_or(false);
            item.tool_state = Some(if ok {
                RuntimeToolCallState::Succeeded
            } else {
                RuntimeToolCallState::Failed
            });
            item.detail = payload_string(&payload, "summary")
                .or_else(|| payload_string(&payload, "message"))
                .or_else(|| {
                    item.tool_name
                        .as_ref()
                        .map(|name| format!("Completed `{name}`."))
                });
            item.text = item.detail.clone();
            if ok {
                if let Some(output) = payload.get("output") {
                    let model_visible_result =
                        model_visible_tool_result_from_completed_payload(&payload);
                    let model_visible_output = model_visible_result
                        .as_deref()
                        .and_then(model_visible_tool_result_output);
                    let summary_output = model_visible_output.as_ref().unwrap_or(output);
                    item.tool_summary = tool_result_summary_from_output(summary_output, ok);
                    item.tool_result_preview = model_visible_result
                        .and_then(truncate_result_preview)
                        .or_else(|| tool_result_preview_from_output(output));
                }
            }
            item.code = payload_string(&payload, "code");
            item.message = payload_string(&payload, "message");
        }
        AgentRunEventKind::FileChanged => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_file_changed".into());
            let operation =
                payload_string(&payload, "operation").unwrap_or_else(|| "change".into());
            let path = payload_string(&payload, "path").unwrap_or_else(|| "unknown path".into());
            let actor = payload_string(&payload, "subagentId")
                .zip(payload_string(&payload, "subagentRole"))
                .map(|(subagent_id, role)| format!("{role} {subagent_id}"));
            item.title = Some("File changed".into());
            item.detail = payload_string(&payload, "toPath")
                .map(|to_path| format!("{operation}: {path} -> {to_path}"))
                .or_else(|| Some(format!("{operation}: {path}")))
                .map(|detail| match actor {
                    Some(actor) => format!("{detail} · {actor}"),
                    None => detail,
                });
            item.text = item.detail.clone();
        }
        AgentRunEventKind::SubagentLifecycle => {
            item.kind = RuntimeStreamItemKind::SubagentLifecycle;
            item.subagent_id = payload_string(&payload, "subagentId");
            item.subagent_role = payload_string(&payload, "subagentRole");
            item.subagent_role_label = payload_string(&payload, "subagentRoleLabel");
            item.subagent_run_id = payload_string(&payload, "subagentRunId");
            item.subagent_status = payload_string(&payload, "subagentStatus");
            item.subagent_used_tool_calls = payload_u64(&payload, "subagentUsedToolCalls");
            item.subagent_max_tool_calls = payload_u64(&payload, "subagentMaxToolCalls");
            item.subagent_used_tokens = payload_u64(&payload, "subagentUsedTokens");
            item.subagent_max_tokens = payload_u64(&payload, "subagentMaxTokens");
            item.subagent_used_cost_micros = payload_u64(&payload, "subagentUsedCostMicros");
            item.subagent_max_cost_micros = payload_u64(&payload, "subagentMaxCostMicros");
            item.subagent_result_summary = payload_string(&payload, "subagentResultSummary");
            item.subagent_prompt = payload_string(&payload, "subagentPrompt");
            let role_label = item
                .subagent_role_label
                .clone()
                .or_else(|| item.subagent_role.clone())
                .unwrap_or_else(|| "subagent".into());
            let status = item
                .subagent_status
                .clone()
                .unwrap_or_else(|| "running".into());
            item.title = Some(format!("Subagent · {role_label}"));
            item.detail = Some(format!("{role_label} subagent is {status}."));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::CommandOutput => {
            item.tool_call_id = payload_string(&payload, "toolCallId");
            item.tool_name = payload_string(&payload, "toolName");
            if item.tool_call_id.is_some() {
                item.kind = RuntimeStreamItemKind::Tool;
                item.tool_state = Some(if payload_bool(&payload, "partial").unwrap_or(false) {
                    RuntimeToolCallState::Running
                } else if payload_bool(&payload, "spawned").unwrap_or(false)
                    && payload.get("exitCode").is_some()
                {
                    RuntimeToolCallState::Succeeded
                } else {
                    RuntimeToolCallState::Running
                });
                item.detail = Some(command_output_summary(&payload));
                item.tool_result_preview = command_output_result_preview(&payload);
                item.text = item.detail.clone();
            } else {
                item.kind = RuntimeStreamItemKind::Activity;
                item.code = Some("owned_agent_command_output".into());
                item.title = Some("Command output".into());
                item.detail = Some(command_output_summary(&payload));
                item.text = item.detail.clone();
            }
        }
        AgentRunEventKind::ValidationStarted => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_validation_started".into());
            item.title = Some("Validation started".into());
            item.detail = payload_string(&payload, "label")
                .map(|label| format!("Validation started: {label}."))
                .or_else(|| Some("Owned agent validation started.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ValidationCompleted => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_validation_completed".into());
            let label = payload_string(&payload, "label").unwrap_or_else(|| "validation".into());
            let outcome = payload_string(&payload, "outcome").unwrap_or_else(|| "completed".into());
            item.title = Some("Validation completed".into());
            item.detail = Some(format!("Validation {outcome}: {label}."));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ToolRegistrySnapshot => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_tool_registry_snapshot".into());
            item.title = Some("Tool registry".into());
            let count = payload
                .get("toolNames")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            let turn = payload
                .get("turnIndex")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default();
            item.detail = Some(format!(
                "Provider turn {turn} has {count} active tool descriptor(s)."
            ));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::PolicyDecision => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = payload_string(&payload, "code")
                .or_else(|| Some("owned_agent_policy_decision".into()));
            item.title = Some("Policy decision".into());
            let action = payload_string(&payload, "action").unwrap_or_else(|| "allow".into());
            let tool = payload_string(&payload, "toolName").unwrap_or_else(|| "tool".into());
            let explanation = payload_string(&payload, "explanation")
                .unwrap_or_else(|| "Central safety policy evaluated the tool call.".into());
            item.detail = Some(format!("{action}: {tool}: {explanation}"));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::StateTransition => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_state_transition".into());
            item.title = Some("Agent state".into());
            let to = payload_string(&payload, "to").unwrap_or_else(|| "unknown".into());
            let reason =
                payload_string(&payload, "reason").unwrap_or_else(|| "State changed.".into());
            item.detail = Some(format!("{to}: {reason}"));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::PlanUpdated => {
            item.kind = RuntimeStreamItemKind::Plan;
            item.code = Some("owned_agent_plan_updated".into());
            item.title = Some("Plan updated".into());
            let total = payload
                .get("total")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default();
            let completed = payload
                .get("completed")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default();
            item.detail = Some(format!(
                "Structured plan has {total} item(s), {completed} completed."
            ));
            item.text = item.detail.clone();
            item.plan_id = Some(format!("run:{}", event.run_id));
            item.plan_items = plan_items_from_payload(&payload);
            item.plan_last_changed_id = payload
                .get("changedItem")
                .and_then(|changed| changed.get("id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
        }
        AgentRunEventKind::VerificationGate => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_verification_gate".into());
            item.title = Some("Verification gate".into());
            item.detail = payload_string(&payload, "message")
                .or_else(|| Some("Completion verification gate evaluated.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ContextManifestRecorded => {
            item.kind = RuntimeStreamItemKind::Tool;
            item.code = Some("owned_agent_context_manifest_recorded".into());
            item.tool_call_id = Some(format!("runtime-project-context:{event_id}:manifest"));
            item.tool_name = Some("project_context".into());
            item.tool_state = Some(RuntimeToolCallState::Succeeded);
            item.detail = context_event_tool_detail(
                &payload,
                "context_manifest",
                "Context manifest recorded.",
            );
            item.tool_result_preview = context_event_tool_result_preview(&payload);
            item.text = item.detail.clone();
        }
        AgentRunEventKind::RetrievalPerformed => {
            item.kind = RuntimeStreamItemKind::Tool;
            item.code = Some("owned_agent_retrieval_performed".into());
            item.tool_call_id = Some(format!("runtime-project-context:{event_id}:retrieval"));
            item.tool_name = Some("project_context".into());
            item.tool_state = Some(RuntimeToolCallState::Succeeded);
            item.detail = context_event_tool_detail(
                &payload,
                "retrieval",
                "Durable context retrieval performed.",
            );
            item.tool_result_preview = context_event_tool_result_preview(&payload);
            item.text = item.detail.clone();
        }
        AgentRunEventKind::MemoryCandidateCaptured => {
            item.kind = RuntimeStreamItemKind::Tool;
            item.code = Some("owned_agent_memory_candidate_captured".into());
            item.tool_call_id = Some(format!(
                "runtime-project-context:{event_id}:memory-candidate"
            ));
            item.tool_name = Some("project_context".into());
            item.tool_state = Some(RuntimeToolCallState::Succeeded);
            item.detail = context_event_tool_detail(
                &payload,
                "memory_candidate",
                "Memory candidate captured.",
            );
            item.tool_result_preview = context_event_tool_result_preview(&payload);
            item.text = item.detail.clone();
        }
        AgentRunEventKind::EnvironmentLifecycleUpdate => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = payload
                .get("diagnostic")
                .and_then(|diagnostic| payload_string(diagnostic, "code"))
                .or_else(|| Some("owned_agent_environment_lifecycle".into()));
            item.title = Some("Environment".into());
            let state = payload_string(&payload, "state").unwrap_or_else(|| "starting".into());
            item.detail = payload_string(&payload, "detail")
                .or_else(|| Some(format!("Environment lifecycle: {state}.")));
            item.message = payload
                .get("diagnostic")
                .and_then(|diagnostic| payload_string(diagnostic, "message"));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::SandboxLifecycleUpdate => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_sandbox_lifecycle".into());
            item.title = Some("Sandbox".into());
            let state = payload_string(&payload, "state").unwrap_or_else(|| "updated".into());
            item.detail = payload_string(&payload, "detail")
                .or_else(|| Some(format!("Sandbox lifecycle: {state}.")));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ActionRequired | AgentRunEventKind::ApprovalRequired => {
            item.kind = RuntimeStreamItemKind::ActionRequired;
            item.action_id = payload_string(&payload, "actionId")
                .or_else(|| Some(format!("owned-agent-action-{event_id}")));
            item.boundary_id = Some("owned_agent".into());
            item.action_type =
                payload_string(&payload, "actionType").or_else(|| Some("operator_review".into()));
            item.title =
                payload_string(&payload, "title").or_else(|| Some("Action required".into()));
            item.detail = payload_string(&payload, "detail")
                .or_else(|| payload_string(&payload, "message"))
                .or_else(|| payload_string(&payload, "reason"))
                .or_else(|| Some("Owned agent requires operator input before continuing.".into()));
            item.code = payload_string(&payload, "code");
            item.message = payload_string(&payload, "message");
            item.answer_shape = payload_string(&payload, "answerShape")
                .as_deref()
                .and_then(runtime_action_answer_shape_from_str);
            item.options = action_required_options_from_payload(&payload);
            item.allow_multiple = payload
                .get("allowMultiple")
                .and_then(serde_json::Value::as_bool);
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ToolPermissionGrant => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_tool_permission_grant".into());
            item.title = Some("Tool permission".into());
            item.detail = payload_string(&payload, "summary")
                .or_else(|| Some("Tool permission grant changed.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ProviderModelChanged => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_provider_model_changed".into());
            item.title = Some("Provider model".into());
            item.detail = payload_string(&payload, "summary")
                .or_else(|| Some("Provider model changed.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::RuntimeSettingsChanged => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_runtime_settings_changed".into());
            item.title = Some("Runtime settings".into());
            item.detail = payload_string(&payload, "summary")
                .or_else(|| Some("Runtime settings changed.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::RunPaused => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code =
                payload_string(&payload, "code").or_else(|| Some("owned_agent_paused".into()));
            item.title = Some("Run paused".into());
            item.detail = payload_string(&payload, "message")
                .or_else(|| Some("Owned agent run paused.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::RunCompleted => {
            item.kind = RuntimeStreamItemKind::Complete;
            item.detail = payload_string(&payload, "summary")
                .or_else(|| Some("Owned agent run completed.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::RunFailed => {
            item.kind = RuntimeStreamItemKind::Failure;
            item.code =
                payload_string(&payload, "code").or_else(|| Some("owned_agent_failed".into()));
            item.message = payload_string(&payload, "message")
                .or_else(|| Some("Owned agent run failed.".into()));
            item.retryable = payload_bool(&payload, "retryable").or(Some(false));
            item.text = item.message.clone();
        }
    }

    Some(item)
}

fn should_emit_owned_runtime_item(
    requested: &[RuntimeStreamItemKind],
    kind: &RuntimeStreamItemKind,
) -> bool {
    kind == &RuntimeStreamItemKind::Failure || requested.contains(kind)
}

fn tool_started_detail(tool_name: Option<&str>, input: &serde_json::Value) -> Option<String> {
    let mut parts = Vec::new();

    match tool_name.unwrap_or_default() {
        "read" => {
            push_value_part(&mut parts, "path", input, "path");
            push_value_part(&mut parts, "startLine", input, "startLine");
            push_value_part(&mut parts, "lineCount", input, "lineCount");
            push_value_part(&mut parts, "mode", input, "mode");
        }
        "search" => {
            push_value_part(&mut parts, "query", input, "query");
            push_value_part(&mut parts, "path", input, "path");
            push_value_part(&mut parts, "maxResults", input, "maxResults");
        }
        "find" => {
            push_value_part(&mut parts, "pattern", input, "pattern");
            push_value_part(&mut parts, "path", input, "path");
        }
        "list" => {
            push_value_part(&mut parts, "path", input, "path");
            push_value_part(&mut parts, "maxDepth", input, "maxDepth");
        }
        "command" | "command_session_start" => {
            push_value_part(&mut parts, "cwd", input, "cwd");
            push_value_part(&mut parts, "cmd", input, "argv");
            push_value_part(&mut parts, "timeoutMs", input, "timeoutMs");
        }
        "command_session_read" | "command_session_stop" => {
            push_value_part(&mut parts, "sessionId", input, "sessionId");
        }
        "git_diff" => {
            push_value_part(&mut parts, "scope", input, "scope");
        }
        "web_search" | "web_search_only" => {
            push_value_part(&mut parts, "query", input, "query");
            push_value_part(&mut parts, "resultCount", input, "resultCount");
        }
        "web_fetch" => {
            push_value_part(&mut parts, "url", input, "url");
            push_value_part(&mut parts, "maxChars", input, "maxChars");
        }
        _ => push_generic_input_parts(&mut parts, input),
    }

    if parts.is_empty() {
        push_generic_input_parts(&mut parts, input);
    }

    render_tool_detail_parts(parts)
}

fn push_generic_input_parts(parts: &mut Vec<String>, input: &serde_json::Value) {
    for (label, key) in [
        ("path", "path"),
        ("fromPath", "fromPath"),
        ("toPath", "toPath"),
        ("pattern", "pattern"),
        ("query", "query"),
        ("url", "url"),
        ("scope", "scope"),
        ("cwd", "cwd"),
        ("cmd", "argv"),
        ("action", "action"),
        ("serverId", "serverId"),
        ("name", "name"),
        ("uri", "uri"),
    ] {
        push_value_part(parts, label, input, key);
        if parts.len() >= 3 {
            break;
        }
    }
}

fn push_value_part(parts: &mut Vec<String>, label: &str, payload: &serde_json::Value, key: &str) {
    if let Some(value) = payload.get(key).and_then(render_json_scalar) {
        parts.push(format!("{label}: {value}"));
    }
}

fn render_json_scalar(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.trim().to_owned()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        serde_json::Value::Array(values) => {
            let joined = values
                .iter()
                .filter_map(|value| value.as_str().map(str::trim))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            Some(joined)
        }
        _ => None,
    }
    .filter(|value| !value.is_empty())
    .map(|value| truncate_chars(&value, 160))
}

fn render_tool_detail_parts(parts: Vec<String>) -> Option<String> {
    if parts.is_empty() {
        return None;
    }

    Some(truncate_chars(&parts.join(", "), 240))
}

fn context_event_tool_detail(
    payload: &serde_json::Value,
    action: &str,
    fallback: &str,
) -> Option<String> {
    let mut parts = vec![format!("action: {action}")];

    for (label, key) in [
        ("queryId", "queryId"),
        ("manifestId", "manifestId"),
        ("candidateId", "candidateId"),
        ("candidateKind", "candidateKind"),
        ("memoryId", "memoryId"),
        ("recordId", "recordId"),
        ("turnIndex", "turnIndex"),
        ("resultCount", "resultCount"),
        ("contextHash", "contextHash"),
    ] {
        push_value_part(&mut parts, label, payload, key);
    }

    payload_string(payload, "summary")
        .or_else(|| payload_string(payload, "message"))
        .or_else(|| Some(fallback.into()))
        .map(|summary| {
            let mut detail = render_tool_detail_parts(parts).unwrap_or_default();
            if detail.is_empty() {
                detail = summary;
            } else {
                detail.push_str(" · ");
                detail.push_str(&truncate_chars(&summary, 180));
            }
            truncate_chars(&detail, 320)
        })
}

fn context_event_tool_result_preview(payload: &serde_json::Value) -> Option<String> {
    serde_json::to_string_pretty(payload)
        .ok()
        .and_then(truncate_result_preview)
}

fn model_visible_tool_result_from_completed_payload(payload: &serde_json::Value) -> Option<String> {
    let result = AgentToolResult {
        tool_call_id: payload_string(payload, "toolCallId")?,
        tool_name: payload_string(payload, "toolName")?,
        ok: payload_bool(payload, "ok").unwrap_or(false),
        summary: payload_string(payload, "summary")
            .or_else(|| payload_string(payload, "message"))
            .unwrap_or_default(),
        output: payload.get("output")?.clone(),
        persistence: None,
        parent_assistant_message_id: None,
    };

    serialize_model_visible_tool_result(&result).ok()
}

fn model_visible_tool_result_output(serialized: &str) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(serialized)
        .ok()?
        .get("output")
        .cloned()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    let keep_chars = max_chars.saturating_sub(3);
    format!("{}...", value.chars().take(keep_chars).collect::<String>())
}

const TOOL_RESULT_PREVIEW_MAX_CHARS: usize = 24_000;

fn normalized_tool_output(output: &serde_json::Value) -> &serde_json::Value {
    if output.get("kind").is_some() {
        return output;
    }

    output
        .get("output")
        .filter(|nested| nested.get("kind").is_some())
        .unwrap_or(output)
}

fn truncate_result_preview(value: String) -> Option<String> {
    let trimmed = value.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    Some(truncate_chars(trimmed, TOOL_RESULT_PREVIEW_MAX_CHARS))
}

fn tool_result_preview_from_output(output: &serde_json::Value) -> Option<String> {
    let output = normalized_tool_output(output);
    match payload_string(output, "kind")?.as_str() {
        "read" => payload_verbatim_string(output, "content").and_then(truncate_result_preview),
        "command" => command_result_preview(output),
        "command_session" => command_session_result_preview(output),
        "git_diff" => payload_verbatim_string(output, "patch").and_then(truncate_result_preview),
        "web_fetch" => payload_verbatim_string(output, "content").and_then(truncate_result_preview),
        "search" => search_result_preview(output),
        "find" => find_result_preview(output),
        "list" => list_result_preview(output),
        "project_context" => project_context_result_preview(output),
        "edit" | "patch" => {
            payload_verbatim_string(output, "diff").and_then(truncate_result_preview)
        }
        _ => serde_json::to_string_pretty(output)
            .ok()
            .and_then(truncate_result_preview),
    }
}

fn command_result_preview(output: &serde_json::Value) -> Option<String> {
    let mut parts = Vec::new();

    if payload_bool(output, "stdoutRedacted").unwrap_or(false) {
        parts.push("[stdout redacted]".to_owned());
    } else if let Some(stdout) = payload_verbatim_string(output, "stdout") {
        parts.push(format!("stdout:\n{stdout}"));
    }

    if payload_bool(output, "stderrRedacted").unwrap_or(false) {
        parts.push("[stderr redacted]".to_owned());
    } else if let Some(stderr) = payload_verbatim_string(output, "stderr") {
        parts.push(format!("stderr:\n{stderr}"));
    }

    truncate_result_preview(parts.join("\n\n"))
}

fn command_session_result_preview(output: &serde_json::Value) -> Option<String> {
    let chunks = output.get("chunks")?.as_array()?;
    let mut parts = Vec::new();

    for chunk in chunks {
        let stream = payload_string(chunk, "stream").unwrap_or_else(|| "output".into());
        if payload_bool(chunk, "redacted").unwrap_or(false) {
            parts.push(format!("[{stream} redacted]"));
            continue;
        }

        if let Some(text) = payload_verbatim_string(chunk, "text") {
            parts.push(format!("{stream}:\n{text}"));
        }
    }

    truncate_result_preview(parts.join("\n\n"))
}

fn search_result_preview(output: &serde_json::Value) -> Option<String> {
    let matches = output.get("matches")?.as_array()?;
    let mut rows = Vec::new();

    for item in matches {
        let has_path = item.get("path").is_some();
        let preview = payload_verbatim_string(item, "preview").unwrap_or_default();
        if !has_path && preview.trim().is_empty() {
            continue;
        }

        let path = payload_string(item, "path").unwrap_or_else(|| "unknown path".into());
        let line = payload_usize(item, "line").unwrap_or_default();
        let column = payload_usize(item, "column").unwrap_or_default();
        rows.push(format!("{path}:{line}:{column}: {preview}"));
    }

    truncate_result_preview(rows.join("\n"))
}

fn find_result_preview(output: &serde_json::Value) -> Option<String> {
    let matches = output.get("matches")?.as_array()?;
    let rows = matches
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>()
        .join("\n");

    truncate_result_preview(rows)
}

fn list_result_preview(output: &serde_json::Value) -> Option<String> {
    let entries = output.get("entries")?.as_array()?;
    let mut rows = Vec::new();

    for entry in entries {
        let kind = payload_string(entry, "kind").unwrap_or_else(|| "entry".into());
        let path = payload_string(entry, "path").unwrap_or_else(|| "unknown path".into());
        let bytes = payload_usize(entry, "bytes")
            .map(|value| format!(" · {value} bytes"))
            .unwrap_or_default();
        rows.push(format!("{kind} {path}{bytes}"));
    }

    truncate_result_preview(rows.join("\n"))
}

fn project_context_result_preview(output: &serde_json::Value) -> Option<String> {
    let mut sections = Vec::new();

    if let Some(message) = payload_verbatim_string(output, "message") {
        sections.push(message);
    }

    if let Some(results) = output.get("results").and_then(serde_json::Value::as_array) {
        let rows = results
            .iter()
            .map(|result| {
                let rank = payload_usize(result, "rank")
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "?".into());
                let source_kind =
                    payload_string(result, "sourceKind").unwrap_or_else(|| "context".into());
                let source_id =
                    payload_string(result, "sourceId").unwrap_or_else(|| "unknown".into());
                let score = payload_string(result, "score")
                    .map(|score| format!(" · score {score}"))
                    .unwrap_or_default();
                let snippet = payload_verbatim_string(result, "snippet").unwrap_or_default();
                let citation = payload_string(result, "citation")
                    .map(|citation| format!("\n  citation: {citation}"))
                    .unwrap_or_default();

                format!("#{rank} {source_kind} {source_id}{score}\n  {snippet}{citation}")
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        if !rows.trim().is_empty() {
            sections.push(format!("results:\n{rows}"));
        }
    }

    if let Some(record) = output.get("record").filter(|value| value.is_object()) {
        sections.push(format!(
            "record: {}\n{}\n{}",
            payload_string(record, "title").unwrap_or_else(|| "Untitled record".into()),
            payload_verbatim_string(record, "summary").unwrap_or_default(),
            payload_verbatim_string(record, "text").unwrap_or_default()
        ));
    }

    if let Some(memory) = output.get("memory").filter(|value| value.is_object()) {
        sections.push(format!(
            "memory: {}\n{}",
            payload_string(memory, "memoryKind").unwrap_or_else(|| "approved_memory".into()),
            payload_verbatim_string(memory, "text").unwrap_or_default()
        ));
    }

    if let Some(candidate) = output
        .get("candidateRecord")
        .filter(|value| value.is_object())
    {
        sections.push(format!(
            "candidate record: {}\n{}\n{}",
            payload_string(candidate, "title").unwrap_or_else(|| "Untitled candidate".into()),
            payload_verbatim_string(candidate, "summary").unwrap_or_default(),
            payload_verbatim_string(candidate, "text").unwrap_or_default()
        ));
    }

    if let Some(manifest) = output.get("manifest").filter(|value| !value.is_null()) {
        if let Some(preview) = project_context_manifest_result_preview(manifest) {
            sections.push(preview);
        } else if let Ok(serialized) = serde_json::to_string_pretty(manifest) {
            sections.push(format!("manifest:\n{serialized}"));
        }
    }

    if sections.is_empty() {
        return serde_json::to_string_pretty(output)
            .ok()
            .and_then(truncate_result_preview);
    }

    truncate_result_preview(sections.join("\n\n"))
}

fn project_context_manifest_result_preview(manifest: &serde_json::Value) -> Option<String> {
    if payload_string(manifest, "kind").as_deref() != Some("provider_context_package_summary") {
        return None;
    }

    let budget = manifest.get("budget").unwrap_or(&serde_json::Value::Null);
    let policy = manifest.get("policy").unwrap_or(&serde_json::Value::Null);
    let contributors = manifest
        .get("contributors")
        .unwrap_or(&serde_json::Value::Null);
    let retrieval = manifest
        .get("retrieval")
        .unwrap_or(&serde_json::Value::Null);
    let tools = manifest.get("tools").unwrap_or(&serde_json::Value::Null);
    let fragments = manifest
        .get("promptFragments")
        .unwrap_or(&serde_json::Value::Null);
    let omitted = manifest.get("omitted").unwrap_or(&serde_json::Value::Null);

    let manifest_id = payload_string(manifest, "manifestId").unwrap_or_else(|| "unknown".into());
    let estimated_tokens = payload_usize(budget, "estimatedTokens")
        .map(|tokens| format!("{tokens} token(s)"))
        .unwrap_or_else(|| "unknown token count".into());
    let pressure = payload_string(policy, "pressure").unwrap_or_else(|| "unknown".into());
    let action = payload_string(policy, "action").unwrap_or_else(|| "unknown".into());

    let mut rows = vec![format!(
        "manifest: {manifest_id} · {estimated_tokens} · pressure {pressure} · action {action}"
    )];

    if let Some(context_hash) = payload_string(manifest, "contextHash") {
        rows.push(format!("contextHash: {context_hash}"));
    }

    rows.push(format!(
        "contributors: {} included, {} excluded",
        payload_usize(contributors, "includedCount").unwrap_or_default(),
        payload_usize(contributors, "excludedCount").unwrap_or_default()
    ));

    let raw_context_injected = payload_bool(retrieval, "rawContextInjected")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    rows.push(format!(
        "retrieval: {} · rawContextInjected={} · results={}",
        payload_string(retrieval, "deliveryModel").unwrap_or_else(|| "unknown".into()),
        raw_context_injected,
        payload_usize(retrieval, "resultCount").unwrap_or_default()
    ));

    let tool_names = json_string_array_preview(tools.get("names"), 10).unwrap_or_default();
    rows.push(format!(
        "tools: {} active{}",
        payload_usize(tools, "count").unwrap_or_default(),
        if tool_names.is_empty() {
            String::new()
        } else {
            format!(" ({tool_names})")
        }
    ));

    let fragment_ids = fragments
        .get("items")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| payload_string(item, "id"))
                .take(8)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    rows.push(format!(
        "prompt fragments: {}{}",
        payload_usize(fragments, "count").unwrap_or_default(),
        if fragment_ids.is_empty() {
            String::new()
        } else {
            format!(" ({fragment_ids})")
        }
    ));

    let original_bytes = payload_usize(omitted, "originalBytes").unwrap_or_default();
    let returned_bytes = payload_usize(omitted, "returnedBytes").unwrap_or_default();
    if original_bytes > 0 || returned_bytes > 0 {
        rows.push(format!(
            "compacted: {original_bytes} -> {returned_bytes} bytes; full manifest remains persisted"
        ));
    }

    if let Some(citation) = payload_string(manifest, "citation") {
        rows.push(format!("citation: {citation}"));
    }

    Some(rows.join("\n"))
}

fn json_string_array_preview(
    value: Option<&serde_json::Value>,
    max_items: usize,
) -> Option<String> {
    let values = value.as_ref()?.as_array()?;
    let mut items = values
        .iter()
        .filter_map(serde_json::Value::as_str)
        .take(max_items)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if items.is_empty() {
        return None;
    }
    if values.len() > max_items {
        items.push("...".into());
    }
    Some(items.join(", "))
}

fn tool_result_summary_from_output(
    output: &serde_json::Value,
    ok: bool,
) -> Option<ToolResultSummaryDto> {
    let output = normalized_tool_output(output);
    match payload_string(output, "kind")?.as_str() {
        "read" => Some(file_tool_summary(
            payload_string(output, "path"),
            None,
            payload_usize(output, "lineCount"),
            None,
            payload_bool(output, "truncated").unwrap_or(false),
        )),
        "search" => Some(file_tool_summary(
            None,
            payload_string(output, "scope"),
            None,
            payload_usize(output, "totalMatches").or_else(|| payload_array_len(output, "matches")),
            payload_bool(output, "truncated").unwrap_or(false),
        )),
        "find" => Some(file_tool_summary(
            None,
            payload_string(output, "scope"),
            None,
            payload_array_len(output, "matches"),
            payload_bool(output, "truncated").unwrap_or(false),
        )),
        "edit" => Some(file_tool_summary(
            payload_string(output, "path"),
            None,
            None,
            payload_usize(output, "replacementLen"),
            false,
        )),
        "write" | "delete" | "mkdir" | "hash" => Some(file_tool_summary(
            payload_string(output, "path"),
            None,
            None,
            None,
            false,
        )),
        "patch" => Some(file_tool_summary(
            payload_string(output, "path").or_else(|| first_file_path(output)),
            None,
            None,
            payload_usize(output, "replacements"),
            false,
        )),
        "rename" => Some(file_tool_summary(
            payload_string(output, "fromPath"),
            payload_string(output, "toPath"),
            None,
            None,
            false,
        )),
        "list" => Some(file_tool_summary(
            payload_string(output, "path"),
            None,
            None,
            payload_array_len(output, "entries"),
            payload_bool(output, "truncated").unwrap_or(false),
        )),
        "command" => Some(command_tool_summary(output)),
        "command_session" => Some(command_session_tool_summary(output)),
        "git_status" => Some(ToolResultSummaryDto::Git(GitToolResultSummaryDto {
            scope: None,
            changed_files: payload_usize(output, "changedFiles").unwrap_or_default(),
            truncated: false,
            base_revision: None,
        })),
        "git_diff" => Some(ToolResultSummaryDto::Git(GitToolResultSummaryDto {
            scope: payload_string(output, "scope").and_then(|scope| git_scope_from_str(&scope)),
            changed_files: payload_usize(output, "changedFiles").unwrap_or_default(),
            truncated: payload_bool(output, "truncated").unwrap_or(false),
            base_revision: payload_string(output, "baseRevision"),
        })),
        "web_search" => Some(ToolResultSummaryDto::Web(WebToolResultSummaryDto {
            target: payload_string(output, "query")?,
            result_count: payload_array_len(output, "results"),
            final_url: None,
            content_kind: None,
            content_type: None,
            truncated: payload_bool(output, "truncated").unwrap_or(false),
        })),
        "web_fetch" => Some(ToolResultSummaryDto::Web(WebToolResultSummaryDto {
            target: payload_string(output, "url")?,
            result_count: None,
            final_url: payload_string(output, "finalUrl"),
            content_kind: payload_string(output, "contentKind")
                .and_then(|kind| web_content_kind_from_str(&kind)),
            content_type: payload_string(output, "contentType"),
            truncated: payload_bool(output, "truncated").unwrap_or(false),
        })),
        "browser" => Some(ToolResultSummaryDto::BrowserComputerUse(
            BrowserComputerUseToolResultSummaryDto {
                surface: BrowserComputerUseSurfaceDto::Browser,
                action: payload_string(output, "action")?,
                status: browser_status_from_ok(ok),
                target: payload_string(output, "url"),
                outcome: None,
            },
        )),
        "mcp" => mcp_capability_summary_from_output(output),
        _ => None,
    }
}

fn file_tool_summary(
    path: Option<String>,
    scope: Option<String>,
    line_count: Option<usize>,
    match_count: Option<usize>,
    truncated: bool,
) -> ToolResultSummaryDto {
    ToolResultSummaryDto::File(FileToolResultSummaryDto {
        path,
        scope,
        line_count,
        match_count,
        truncated,
    })
}

fn command_tool_summary(output: &serde_json::Value) -> ToolResultSummaryDto {
    ToolResultSummaryDto::Command(CommandToolResultSummaryDto {
        exit_code: payload_i32(output, "exitCode"),
        timed_out: payload_bool(output, "timedOut").unwrap_or(false),
        stdout_truncated: payload_bool(output, "stdoutTruncated").unwrap_or(false),
        stderr_truncated: payload_bool(output, "stderrTruncated").unwrap_or(false),
        stdout_redacted: payload_bool(output, "stdoutRedacted").unwrap_or(false),
        stderr_redacted: payload_bool(output, "stderrRedacted").unwrap_or(false),
    })
}

fn command_session_tool_summary(output: &serde_json::Value) -> ToolResultSummaryDto {
    ToolResultSummaryDto::Command(CommandToolResultSummaryDto {
        exit_code: payload_i32(output, "exitCode"),
        timed_out: false,
        stdout_truncated: command_session_stream_bool(output, "stdout", "truncated"),
        stderr_truncated: command_session_stream_bool(output, "stderr", "truncated"),
        stdout_redacted: command_session_stream_bool(output, "stdout", "redacted"),
        stderr_redacted: command_session_stream_bool(output, "stderr", "redacted"),
    })
}

fn command_session_stream_bool(output: &serde_json::Value, stream: &str, key: &str) -> bool {
    output
        .get("chunks")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .any(|chunk| {
            payload_string(chunk, "stream").as_deref() == Some(stream)
                && payload_bool(chunk, key).unwrap_or(false)
        })
}

fn mcp_capability_summary_from_output(output: &serde_json::Value) -> Option<ToolResultSummaryDto> {
    let action = payload_string(output, "action")?;
    let capability_kind = match action.as_str() {
        "invoke_tool" => McpCapabilityKindDto::Tool,
        "read_resource" => McpCapabilityKindDto::Resource,
        "get_prompt" => McpCapabilityKindDto::Prompt,
        _ => return None,
    };
    let capability_name = payload_string(output, "capabilityName")?;

    Some(ToolResultSummaryDto::McpCapability(
        McpCapabilityToolResultSummaryDto {
            server_id: payload_string(output, "serverId")?,
            capability_kind,
            capability_id: capability_name.clone(),
            capability_name: Some(capability_name),
        },
    ))
}

fn first_file_path(output: &serde_json::Value) -> Option<String> {
    output
        .get("files")
        .and_then(serde_json::Value::as_array)?
        .first()
        .and_then(|file| payload_string(file, "path"))
}

fn payload_usize(payload: &serde_json::Value, key: &str) -> Option<usize> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn payload_i32(payload: &serde_json::Value, key: &str) -> Option<i32> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

fn payload_array_len(payload: &serde_json::Value, key: &str) -> Option<usize> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
}

fn git_scope_from_str(scope: &str) -> Option<GitToolResultScopeDto> {
    match scope {
        "staged" => Some(GitToolResultScopeDto::Staged),
        "unstaged" => Some(GitToolResultScopeDto::Unstaged),
        "worktree" => Some(GitToolResultScopeDto::Worktree),
        _ => None,
    }
}

fn web_content_kind_from_str(kind: &str) -> Option<WebToolResultContentKindDto> {
    match kind {
        "html" => Some(WebToolResultContentKindDto::Html),
        "plain_text" => Some(WebToolResultContentKindDto::PlainText),
        _ => None,
    }
}

fn browser_status_from_ok(ok: bool) -> BrowserComputerUseActionStatusDto {
    if ok {
        BrowserComputerUseActionStatusDto::Succeeded
    } else {
        BrowserComputerUseActionStatusDto::Failed
    }
}

fn payload_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn code_change_group_id_from_payload(payload: &serde_json::Value) -> Option<String> {
    code_history_payload_string(payload, "codeChangeGroupId", "xero.code_change_group_id")
}

fn code_commit_id_from_payload(payload: &serde_json::Value) -> Option<String> {
    code_history_payload_string(payload, "codeCommitId", "xero.code_commit_id")
}

fn code_workspace_epoch_from_payload(payload: &serde_json::Value) -> Option<u64> {
    payload
        .get("codeWorkspaceEpoch")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            code_history_payload_string(payload, "codeWorkspaceEpoch", "xero.code_workspace_epoch")
                .and_then(|value| value.parse::<u64>().ok())
        })
}

fn code_patch_availability_from_payload(
    payload: &serde_json::Value,
    project_id: &str,
    change_group_id: Option<&str>,
) -> Option<CodePatchAvailabilityDto> {
    if let Some(value) = payload.get("codePatchAvailability") {
        if let Ok(availability) = serde_json::from_value::<CodePatchAvailabilityDto>(value.clone())
        {
            return Some(availability);
        }
    }

    let available =
        code_history_payload_bool(payload, "codePatchAvailable", "xero.code_patch_available")?;
    let target_change_group_id = change_group_id?.to_string();
    let affected_paths = code_patch_affected_paths_from_payload(payload);
    let file_change_count = code_history_payload_u32(
        payload,
        "codePatchFileChangeCount",
        "xero.code_patch_file_change_count",
    )
    .unwrap_or_else(|| affected_paths.len().try_into().unwrap_or(u32::MAX));
    let text_hunk_count = code_history_payload_u32(
        payload,
        "codePatchTextHunkCount",
        "xero.code_patch_text_hunk_count",
    )
    .unwrap_or(0);
    let unavailable_reason = code_history_payload_string(
        payload,
        "codePatchUnavailableReason",
        "xero.code_patch_unavailable_reason",
    );

    Some(CodePatchAvailabilityDto {
        project_id: project_id.to_string(),
        target_change_group_id,
        available,
        affected_paths,
        file_change_count,
        text_hunk_count,
        text_hunks: Vec::new(),
        unavailable_reason,
    })
}

fn code_patch_affected_paths_from_payload(payload: &serde_json::Value) -> Vec<String> {
    if let Some(paths) = payload
        .get("codePatchAffectedPaths")
        .and_then(serde_json::Value::as_array)
    {
        return paths
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(ToOwned::to_owned)
            .collect();
    }
    code_history_payload_string(
        payload,
        "codePatchAffectedPaths",
        "xero.code_patch_affected_paths",
    )
    .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
    .unwrap_or_default()
    .into_iter()
    .map(|path| path.trim().to_string())
    .filter(|path| !path.is_empty())
    .collect()
}

fn code_history_payload_string(
    payload: &serde_json::Value,
    direct_key: &str,
    telemetry_key: &str,
) -> Option<String> {
    payload_string(payload, direct_key).or_else(|| {
        payload
            .pointer(&format!("/dispatch/telemetry/{telemetry_key}"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn code_history_payload_bool(
    payload: &serde_json::Value,
    direct_key: &str,
    telemetry_key: &str,
) -> Option<bool> {
    payload
        .get(direct_key)
        .and_then(serde_json::Value::as_bool)
        .or_else(|| {
            code_history_payload_string(payload, direct_key, telemetry_key).and_then(|value| {
                match value.as_str() {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                }
            })
        })
}

fn code_history_payload_u32(
    payload: &serde_json::Value,
    direct_key: &str,
    telemetry_key: &str,
) -> Option<u32> {
    payload
        .get(direct_key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .or_else(|| {
            code_history_payload_string(payload, direct_key, telemetry_key)
                .and_then(|value| value.parse::<u32>().ok())
        })
}

fn payload_verbatim_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn payload_bool(payload: &serde_json::Value, key: &str) -> Option<bool> {
    payload.get(key).and_then(|value| value.as_bool())
}

fn payload_u64(payload: &serde_json::Value, key: &str) -> Option<u64> {
    payload.get(key).and_then(|value| value.as_u64())
}

fn payload_transcript_role(payload: &serde_json::Value) -> Option<RuntimeStreamTranscriptRole> {
    match payload_string(payload, "role")?.as_str() {
        "user" => Some(RuntimeStreamTranscriptRole::User),
        "assistant" => Some(RuntimeStreamTranscriptRole::Assistant),
        "system" => Some(RuntimeStreamTranscriptRole::System),
        "tool" => Some(RuntimeStreamTranscriptRole::Tool),
        _ => None,
    }
}

fn plan_items_from_payload(payload: &serde_json::Value) -> Option<Vec<RuntimeStreamPlanItemDto>> {
    let items = payload.get("items")?.as_array()?;
    let mut projected = Vec::with_capacity(items.len());
    for entry in items {
        let id = payload_string(entry, "id")?;
        let title = payload_string(entry, "title").unwrap_or_else(|| id.clone());
        let notes = payload_string(entry, "notes");
        let status = match entry.get("status").and_then(serde_json::Value::as_str) {
            Some("in_progress") => RuntimeStreamPlanItemStatus::InProgress,
            Some("completed") => RuntimeStreamPlanItemStatus::Completed,
            _ => RuntimeStreamPlanItemStatus::Pending,
        };
        let updated_at = payload_string(entry, "updatedAt").unwrap_or_default();
        projected.push(RuntimeStreamPlanItemDto {
            id,
            title,
            notes,
            status,
            updated_at,
            phase_id: payload_string(entry, "phaseId"),
            phase_title: payload_string(entry, "phaseTitle"),
            slice_id: payload_string(entry, "sliceId"),
            handoff_note: payload_string(entry, "handoffNote"),
        });
    }
    Some(projected)
}

fn runtime_action_answer_shape_from_str(value: &str) -> Option<RuntimeActionAnswerShape> {
    match value {
        "plain_text" => Some(RuntimeActionAnswerShape::PlainText),
        "terminal_input" => Some(RuntimeActionAnswerShape::TerminalInput),
        "single_choice" => Some(RuntimeActionAnswerShape::SingleChoice),
        "multi_choice" => Some(RuntimeActionAnswerShape::MultiChoice),
        "short_text" => Some(RuntimeActionAnswerShape::ShortText),
        "long_text" => Some(RuntimeActionAnswerShape::LongText),
        "number" => Some(RuntimeActionAnswerShape::Number),
        "date" => Some(RuntimeActionAnswerShape::Date),
        _ => None,
    }
}

fn action_required_options_from_payload(
    payload: &serde_json::Value,
) -> Option<Vec<RuntimeActionRequiredOptionDto>> {
    let options = payload.get("options")?.as_array()?;
    let mut projected = Vec::with_capacity(options.len());
    for option in options {
        let id = payload_string(option, "id")?;
        let label = payload_string(option, "label").unwrap_or_else(|| id.clone());
        projected.push(RuntimeActionRequiredOptionDto {
            id,
            label,
            description: payload_string(option, "description"),
        });
    }
    (!projected.is_empty()).then_some(projected)
}

fn command_output_result_preview(payload: &serde_json::Value) -> Option<String> {
    if payload_bool(payload, "partial").unwrap_or(false) {
        let stream = payload_string(payload, "stream").unwrap_or_else(|| "output".into());
        if payload_bool(payload, "redacted").unwrap_or(false) {
            return truncate_result_preview(format!("[{stream} redacted]"));
        }
        if let Some(text) = payload_verbatim_string(payload, "text") {
            return truncate_result_preview(format!("{stream}:\n{text}"));
        }
        return None;
    }

    if payload.get("stdout").is_some()
        || payload.get("stderr").is_some()
        || payload_bool(payload, "stdoutRedacted").unwrap_or(false)
        || payload_bool(payload, "stderrRedacted").unwrap_or(false)
    {
        return command_result_preview(payload);
    }

    None
}

fn command_output_summary(payload: &serde_json::Value) -> String {
    if payload_bool(payload, "partial").unwrap_or(false) {
        let stream = payload_string(payload, "stream").unwrap_or_else(|| "output".into());
        return format!("Command {stream} streamed.");
    }

    let argv = payload
        .get("argv")
        .and_then(|value| value.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|command| !command.trim().is_empty())
        .unwrap_or_else(|| "command".into());
    if let Some(operation) = payload_string(payload, "operation") {
        return format!("Command session {operation}: {argv}.");
    }
    if payload_bool(payload, "timedOut").unwrap_or(false) {
        return format!("Command timed out: {argv}.");
    }
    match payload.get("exitCode").and_then(|value| value.as_i64()) {
        Some(code) => format!("Command exited with status {code}: {argv}."),
        None => format!("Command output: {argv}."),
    }
}

fn resolve_channel<R: Runtime>(
    webview: &Webview<R>,
    raw_channel: Option<&str>,
) -> CommandResult<Channel<serde_json::Value>> {
    let Some(raw_channel) = raw_channel else {
        return Err(CommandError::user_fixable(
            "runtime_stream_channel_missing",
            "Xero requires a runtime stream channel before it can start streaming selected-project runtime items.",
        ));
    };

    let channel_id = JavaScriptChannelId::from_str(raw_channel).map_err(|_| {
        CommandError::user_fixable(
            "runtime_stream_channel_invalid",
            "Xero received an invalid runtime stream channel handle from the desktop shell.",
        )
    })?;

    Ok(channel_id.channel_on(webview.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(event_kind: AgentRunEventKind, payload_json: &str) -> AgentEventRecord {
        event_with_id(42, event_kind, payload_json)
    }

    fn event_with_id(
        id: i64,
        event_kind: AgentRunEventKind,
        payload_json: &str,
    ) -> AgentEventRecord {
        AgentEventRecord {
            id,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            event_kind,
            payload_json: payload_json.into(),
            created_at: "2026-04-24T00:00:00Z".into(),
        }
    }

    fn projection_context() -> RuntimeStreamProjectionContext {
        RuntimeStreamProjectionContext {
            project_id: "project-1".into(),
            agent_session_id: "agent-session-1".into(),
            runtime_kind: "openai_codex".into(),
            run_id: "run-1".into(),
            session_id: "owned-agent:run-1".into(),
            flow_id: None,
            subscribed_item_kinds: vec![
                RuntimeStreamItemKind::Transcript,
                RuntimeStreamItemKind::Tool,
                RuntimeStreamItemKind::Activity,
                RuntimeStreamItemKind::ActionRequired,
                RuntimeStreamItemKind::Complete,
                RuntimeStreamItemKind::Failure,
            ],
        }
    }

    fn seed_replay_project(root: &tempfile::TempDir) -> std::path::PathBuf {
        let repo_root = root.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("create replay repo root");
        let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical replay repo root");
        let repository = crate::git::repository::CanonicalRepository {
            project_id: "project-1".into(),
            repository_id: "repo-1".into(),
            root_path: canonical_root.clone(),
            root_path_string: canonical_root.to_string_lossy().into_owned(),
            common_git_dir: canonical_root.join(".git"),
            display_name: "repo".into(),
            branch_name: Some("main".into()),
            head_sha: Some("abc123".into()),
            branch: None,
            last_commit: None,
            status_entries: Vec::new(),
            has_staged_changes: false,
            has_unstaged_changes: false,
            has_untracked_changes: false,
            additions: 0,
            deletions: 0,
        };

        crate::db::configure_project_database_paths(&root.path().join("app-data").join("xero.db"));
        let state = crate::state::DesktopState::default();
        crate::db::import_project(&repository, state.import_failpoints()).expect("import project");
        canonical_root
    }

    fn seed_replay_run(repo_root: &std::path::Path, event_count: usize) {
        let session = project_store::create_agent_session(
            repo_root,
            &project_store::AgentSessionCreateRecord {
                project_id: "project-1".into(),
                title: "Replay test session".into(),
                summary: String::new(),
                selected: true,
            },
        )
        .expect("create replay test session");

        project_store::insert_agent_run(
            repo_root,
            &project_store::NewAgentRunRecord {
                runtime_agent_id: crate::commands::RuntimeAgentIdDto::Engineer,
                agent_definition_id: Some("engineer".into()),
                agent_definition_version: Some(project_store::BUILTIN_AGENT_DEFINITION_VERSION),
                project_id: "project-1".into(),
                agent_session_id: session.agent_session_id,
                run_id: "run-1".into(),
                provider_id: "fake".into(),
                model_id: "fake-model".into(),
                prompt: "test prompt".into(),
                system_prompt: "test system".into(),
                now: "2026-04-24T00:00:00Z".into(),
            },
        )
        .expect("insert replay test run");

        for index in 0..event_count {
            project_store::append_agent_event(
                repo_root,
                &project_store::NewAgentEventRecord {
                    project_id: "project-1".into(),
                    run_id: "run-1".into(),
                    event_kind: AgentRunEventKind::MessageDelta,
                    payload_json: serde_json::json!({
                        "delta": format!("chunk-{index}")
                    })
                    .to_string(),
                    created_at: "2026-04-24T00:00:00Z".into(),
                },
            )
            .expect("append replay test event");
        }
    }

    #[test]
    fn fresh_runtime_stream_subscription_replays_full_run_history() {
        let root = tempfile::tempdir().expect("temp dir");
        let repo_root = seed_replay_project(&root);
        let event_count = INCREMENTAL_RUNTIME_STREAM_REPLAY_LIMIT + 5;
        seed_replay_run(&repo_root, event_count);

        let (full_events, full_mode) = load_owned_agent_replay_events(
            &repo_root,
            "project-1",
            "run-1",
            None,
            None,
            INCREMENTAL_RUNTIME_STREAM_REPLAY_LIMIT,
        )
        .expect("load full replay events");
        assert_eq!(full_mode, "full");
        assert_eq!(full_events.len(), event_count);
        assert_eq!(full_events.first().expect("first event").id, 1);
        assert_eq!(
            full_events.last().expect("last event").id,
            event_count as i64
        );

        let (limited_events, limited_mode) =
            load_owned_agent_replay_events(&repo_root, "project-1", "run-1", None, Some(10), 10)
                .expect("load limited replay events");
        assert_eq!(limited_mode, "limited-full");
        assert_eq!(limited_events.len(), 10);
        assert_eq!(
            limited_events.first().expect("first limited event").id,
            event_count as i64 - 9
        );
    }

    #[test]
    fn oversized_tool_result_stream_payload_falls_back_under_ipc_budget() {
        let large_content = "x".repeat(180_000);
        let payload = serde_json::json!({
            "toolCallId": "call-large-read",
            "toolName": "read",
            "ok": true,
            "summary": "read returned a large file",
            "output": {
                "kind": "read",
                "path": "src/large.ts",
                "content": large_content,
                "lineCount": 6000,
                "truncated": false
            }
        });
        let item = owned_agent_event_runtime_item(
            event(AgentRunEventKind::ToolCompleted, &payload.to_string()),
            "owned-agent:run-1",
            None,
        )
        .expect("tool item");
        let mut projection = RuntimeStreamProjection::new(projection_context());
        let patch = projection.apply_item(item);
        let channel_payload =
            runtime_stream_patch_payload_for_ipc(patch).expect("encode stream payload");

        assert!(
            estimated_ipc_payload_bytes(&channel_payload) <= RUNTIME_STREAM_IPC_MAX_BYTES,
            "stream payload should fit the frontend runtime-stream IPC budget"
        );
        let serialized = serde_json::to_string(&channel_payload).expect("serialize payload");
        assert!(!serialized.contains(&"x".repeat(20_000)));
        assert!(
            channel_payload.get("schema").is_some() || channel_payload.get("kind").is_some(),
            "payload remains either a patch or an item envelope"
        );
    }

    #[test]
    fn owned_agent_event_projection_maps_tool_and_action_items() {
        let tool = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-1","toolName":"read","ok":false,"code":"tool_failed","message":"nope"}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("tool item");
        assert_eq!(tool.kind, RuntimeStreamItemKind::Tool);
        assert_eq!(tool.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(tool.tool_state, Some(RuntimeToolCallState::Failed));
        assert_eq!(tool.code.as_deref(), Some("tool_failed"));
        assert_eq!(tool.detail.as_deref(), Some("nope"));

        let code_tool = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-code","toolName":"write","ok":true,"dispatch":{"telemetry":{"xero.code_change_group_id":"code-change-1","xero.code_commit_id":"code-commit-1","xero.code_workspace_epoch":"7","xero.code_patch_available":"true","xero.code_patch_affected_paths":"[\"src/app.ts\"]","xero.code_patch_file_change_count":"1","xero.code_patch_text_hunk_count":"2"}}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("code tool item");
        assert_eq!(
            code_tool.code_change_group_id.as_deref(),
            Some("code-change-1")
        );
        assert_eq!(code_tool.code_commit_id.as_deref(), Some("code-commit-1"));
        assert_eq!(code_tool.code_workspace_epoch, Some(7));
        assert_eq!(
            code_tool
                .code_patch_availability
                .as_ref()
                .map(|availability| availability.affected_paths.clone()),
            Some(vec!["src/app.ts".to_string()])
        );

        let action = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ActionRequired,
                r#"{"actionId":"plan-mode-before-tools","actionType":"plan_mode","title":"Plan required","message":"pause"}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("action item");
        assert_eq!(action.kind, RuntimeStreamItemKind::ActionRequired);
        assert_eq!(action.action_id.as_deref(), Some("plan-mode-before-tools"));
        assert_eq!(action.boundary_id.as_deref(), Some("owned_agent"));
        assert_eq!(action.action_type.as_deref(), Some("plan_mode"));
        assert_eq!(action.detail.as_deref(), Some("pause"));

        let fallback_action = owned_agent_event_runtime_item(
            event(AgentRunEventKind::ActionRequired, r#"{}"#),
            "owned-agent:run-1",
            None,
        )
        .expect("fallback action item");
        assert_eq!(
            fallback_action.detail.as_deref(),
            Some("Owned agent requires operator input before continuing.")
        );
    }

    #[test]
    fn owned_agent_command_output_projection_streams_partial_chunks_as_running_tools() {
        let output = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::CommandOutput,
                r#"{"toolCallId":"call-command","toolName":"command","stream":"stdout","text":"running test 1\n","partial":true}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("command output item");

        assert_eq!(output.kind, RuntimeStreamItemKind::Tool);
        assert_eq!(output.tool_call_id.as_deref(), Some("call-command"));
        assert_eq!(output.tool_name.as_deref(), Some("command"));
        assert_eq!(output.tool_state, Some(RuntimeToolCallState::Running));
        assert_eq!(output.detail.as_deref(), Some("Command stdout streamed."));
        assert_eq!(
            output.tool_result_preview.as_deref(),
            Some("stdout:\nrunning test 1")
        );
    }

    #[test]
    fn owned_agent_event_projection_keeps_reasoning_text_visible() {
        let reasoning = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ReasoningSummary,
                r#"{"summary":"I should inspect the latest build output"}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("reasoning item");

        assert_eq!(reasoning.kind, RuntimeStreamItemKind::Activity);
        assert_eq!(reasoning.code.as_deref(), Some("owned_agent_reasoning"));
        assert_eq!(reasoning.title.as_deref(), Some("Reasoning"));
        assert_eq!(
            reasoning.text.as_deref(),
            Some("I should inspect the latest build output")
        );
        assert_eq!(
            reasoning.detail.as_deref(),
            Some("I should inspect the latest build output")
        );

        let whitespace_delta = owned_agent_event_runtime_item(
            event(AgentRunEventKind::ReasoningSummary, r#"{"summary":"\n\n"}"#),
            "owned-agent:run-1",
            None,
        )
        .expect("reasoning whitespace item");

        assert_eq!(
            whitespace_delta.code.as_deref(),
            Some("owned_agent_reasoning")
        );
        assert_eq!(whitespace_delta.text.as_deref(), Some("\n\n"));
        assert_eq!(
            whitespace_delta.detail.as_deref(),
            Some("Owned agent reasoning summary updated.")
        );

        let usage = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ReasoningSummary,
                r#"{"summary":"Provider usage updated.","usage":{"totalTokens":12}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("usage activity item");

        assert_eq!(usage.kind, RuntimeStreamItemKind::Activity);
        assert_eq!(usage.code.as_deref(), Some("owned_agent_usage"));
        assert_eq!(usage.title.as_deref(), Some("Provider usage"));
    }

    #[test]
    fn runtime_stream_projection_coalesces_transcript_reasoning_and_replaces_tools() {
        let mut projection = RuntimeStreamProjection::new(projection_context());

        let first_transcript = owned_agent_event_runtime_item(
            event_with_id(
                1,
                AgentRunEventKind::MessageDelta,
                r#"{"role":"assistant","text":"Hello "}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("first transcript item");
        projection.apply_item(first_transcript);
        let second_transcript = owned_agent_event_runtime_item(
            event_with_id(
                2,
                AgentRunEventKind::MessageDelta,
                r#"{"role":"assistant","text":"world"}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("second transcript item");
        let transcript_patch = projection.apply_item(second_transcript);

        assert_eq!(transcript_patch.schema, RUNTIME_STREAM_PATCH_SCHEMA);
        assert_eq!(transcript_patch.snapshot.schema, RUNTIME_STREAM_VIEW_SCHEMA);
        assert_eq!(transcript_patch.snapshot.transcript_items.len(), 1);
        let transcript = &transcript_patch.snapshot.transcript_items[0];
        assert_eq!(transcript.sequence, 1);
        assert_eq!(transcript.updated_sequence, Some(2));
        assert_eq!(transcript.text.as_deref(), Some("Hello world"));

        let tool_started = owned_agent_event_runtime_item(
            event_with_id(
                3,
                AgentRunEventKind::ToolStarted,
                r#"{"toolCallId":"call-read","toolName":"read","input":{"path":"client/src/App.tsx"}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("tool started item");
        projection.apply_item(tool_started);
        let tool_completed = owned_agent_event_runtime_item(
            event_with_id(
                4,
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-read","toolName":"read","ok":true,"summary":"Read App.","output":{"kind":"read","path":"client/src/App.tsx","lineCount":2,"truncated":false,"content":"export function App() {}\n"}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("tool completed item");
        let tool_patch = projection.apply_item(tool_completed);

        assert_eq!(tool_patch.snapshot.tool_calls.len(), 1);
        assert_eq!(
            tool_patch.snapshot.tool_calls[0].tool_state,
            Some(RuntimeToolCallState::Succeeded)
        );
        let timeline_tool = tool_patch
            .snapshot
            .items
            .iter()
            .find(|item| item.tool_call_id.as_deref() == Some("call-read"))
            .expect("timeline tool");
        assert_eq!(timeline_tool.sequence, 3);
        assert_eq!(timeline_tool.updated_sequence, Some(4));
        assert_eq!(timeline_tool.detail.as_deref(), Some("Read App."));

        let reasoning_one = owned_agent_event_runtime_item(
            event_with_id(
                5,
                AgentRunEventKind::ReasoningSummary,
                r#"{"summary":"Inspecting"}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("first reasoning item");
        projection.apply_item(reasoning_one);
        let reasoning_two = owned_agent_event_runtime_item(
            event_with_id(
                6,
                AgentRunEventKind::ReasoningSummary,
                r#"{"summary":" files"}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("second reasoning item");
        let reasoning_patch = projection.apply_item(reasoning_two);
        let reasoning = reasoning_patch
            .snapshot
            .items
            .iter()
            .filter(|item| item.code.as_deref() == Some(OWNED_AGENT_REASONING_ACTIVITY_CODE))
            .collect::<Vec<_>>();

        assert_eq!(reasoning.len(), 1);
        assert_eq!(reasoning[0].sequence, 5);
        assert_eq!(reasoning[0].updated_sequence, Some(6));
        assert_eq!(reasoning[0].text.as_deref(), Some("Inspecting files"));
        assert_eq!(reasoning_patch.snapshot.last_sequence, Some(6));
    }

    #[test]
    fn runtime_stream_projection_retains_full_replay_beyond_recent_tail() {
        let mut projection = RuntimeStreamProjection::new(projection_context());
        let retained_count = 25;

        for index in 0..retained_count {
            let transcript = owned_agent_event_runtime_item(
                event_with_id(
                    (index + 1) as i64,
                    AgentRunEventKind::MessageDelta,
                    &serde_json::json!({
                        "role": "user",
                        "text": format!("turn-{index}")
                    })
                    .to_string(),
                ),
                "owned-agent:run-1",
                None,
            )
            .expect("transcript item");
            projection.apply_item(transcript);
        }

        for index in 0..retained_count {
            let tool = owned_agent_event_runtime_item(
                event_with_id(
                    (100 + index) as i64,
                    AgentRunEventKind::ToolCompleted,
                    &serde_json::json!({
                        "toolCallId": format!("call-read-{index}"),
                        "toolName": "read",
                        "ok": true,
                        "summary": format!("Read file {index}.")
                    })
                    .to_string(),
                ),
                "owned-agent:run-1",
                None,
            )
            .expect("tool item");
            projection.apply_item(tool);
        }

        let snapshot = projection.snapshot();
        assert_eq!(snapshot.transcript_items.len(), retained_count);
        assert_eq!(snapshot.tool_calls.len(), retained_count);
        assert_eq!(snapshot.transcript_items[0].text.as_deref(), Some("turn-0"));
        assert_eq!(
            snapshot
                .tool_calls
                .first()
                .and_then(|item| item.tool_call_id.as_deref()),
            Some("call-read-0")
        );
        assert_eq!(
            snapshot
                .items
                .iter()
                .filter(|item| matches!(&item.kind, RuntimeStreamItemKind::Transcript))
                .count(),
            retained_count
        );
        assert_eq!(
            snapshot
                .items
                .iter()
                .filter(|item| matches!(&item.kind, RuntimeStreamItemKind::Tool))
                .count(),
            retained_count
        );
    }

    #[test]
    fn owned_agent_tool_started_projection_carries_concise_input_detail() {
        let read = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolStarted,
                r#"{"toolCallId":"call-read","toolName":"read","input":{"path":"client/components/xero/agent-runtime.tsx","startLine":12,"lineCount":40,"token":"[REDACTED]"}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("read tool item");
        assert_eq!(
            read.detail.as_deref(),
            Some("path: client/components/xero/agent-runtime.tsx, startLine: 12, lineCount: 40")
        );

        let command = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolStarted,
                r#"{"toolCallId":"call-command","toolName":"command","input":{"cwd":"client","argv":["pnpm","test","--run","agent-runtime.test.tsx"],"timeoutMs":120000}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("command tool item");
        assert_eq!(
            command.detail.as_deref(),
            Some("cwd: client, cmd: pnpm test --run agent-runtime.test.tsx, timeoutMs: 120000")
        );
    }

    #[test]
    fn owned_agent_tool_completed_projection_maps_summary_into_detail() {
        let tool = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-1","toolName":"read","ok":true,"summary":"Read 2 line(s) from `client/src/lib.rs`.","output":{"kind":"read","path":"client/src/lib.rs","lineCount":2,"truncated":false}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("tool item");

        assert_eq!(
            tool.detail.as_deref(),
            Some("Read 2 line(s) from `client/src/lib.rs`.")
        );
        assert_eq!(tool.text, tool.detail);
    }

    #[test]
    fn owned_agent_tool_completed_projection_uses_model_visible_result_preview() {
        let payload = r#"{"toolCallId":"call-command","toolName":"command","ok":true,"summary":"command","output":{"kind":"command","argv":["pnpm","test"],"cwd":"client","stdout":"ok","stderr":"","exitCode":0,"timedOut":false,"stdoutTruncated":false,"stderrTruncated":false,"stdoutRedacted":false,"stderrRedacted":false,"spawned":false,"policy":{"approvalRequired":false},"sandbox":{"profile":"danger-full-access"}}}"#;
        let tool = owned_agent_event_runtime_item(
            event(AgentRunEventKind::ToolCompleted, payload),
            "owned-agent:run-1",
            None,
        )
        .expect("tool item");
        let payload_json =
            serde_json::from_str::<serde_json::Value>(payload).expect("decode fixture payload");
        let expected = model_visible_tool_result_from_completed_payload(&payload_json)
            .expect("model visible fixture result");

        assert_eq!(tool.tool_result_preview.as_deref(), Some(expected.as_str()));
        assert!(!expected.contains("\"policy\""));
        assert!(!expected.contains("\"sandbox\""));
        assert!(expected.contains("[BEGIN stdout]\nok\n[END stdout]"));
        assert!(expected.contains("xeroCompact: schema=xero.model_visible_tool_result.v1"));
        assert!(serde_json::from_str::<serde_json::Value>(&expected).is_err());
    }

    #[test]
    fn owned_agent_tool_completed_projection_derives_file_summaries() {
        let read = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-read","toolName":"read","ok":true,"summary":"read","output":{"kind":"read","path":"client/src/lib.rs","lineCount":2,"truncated":false,"content":"pub fn run() {}\n"}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("read tool item");
        assert_eq!(
            read.tool_summary,
            Some(ToolResultSummaryDto::File(FileToolResultSummaryDto {
                path: Some("client/src/lib.rs".into()),
                scope: None,
                line_count: Some(2),
                match_count: None,
                truncated: false,
            }))
        );
        let read_preview = read
            .tool_result_preview
            .as_deref()
            .expect("model-visible read preview");
        assert!(read_preview.contains("tool result: read call call-read ok=true"));
        assert!(read_preview.contains("[BEGIN read content: client/src/lib.rs]\npub fn run() {}\n"));
        assert!(read_preview.contains("xeroCompact: schema=xero.model_visible_tool_result.v1"));
        assert!(!read_preview.contains("\\n"));

        let search = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-search","toolName":"search","ok":true,"summary":"search","output":{"kind":"search","query":"appendTranscriptDelta","scope":"client","matches":[{"path":"client/a.ts","line":4,"column":2,"preview":"appendTranscriptDelta()"},{}],"totalMatches":4,"truncated":true}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("search tool item");
        assert_eq!(
            search.tool_summary,
            Some(ToolResultSummaryDto::File(FileToolResultSummaryDto {
                path: None,
                scope: Some("client".into()),
                line_count: None,
                match_count: Some(4),
                truncated: true,
            }))
        );
        let search_preview = serde_json::from_str::<serde_json::Value>(
            search
                .tool_result_preview
                .as_deref()
                .expect("model-visible search preview"),
        )
        .expect("decode search preview");
        assert_eq!(
            search_preview["toolCallId"],
            serde_json::json!("call-search")
        );
        assert_eq!(
            search_preview["output"]["kind"],
            serde_json::json!("search")
        );

        let find = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-find","toolName":"find","ok":true,"summary":"find","output":{"kind":"find","pattern":"*.rs","scope":"client/src-tauri","matches":["src/lib.rs","src/main.rs"],"truncated":false}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("find tool item");
        assert_eq!(
            find.tool_summary,
            Some(ToolResultSummaryDto::File(FileToolResultSummaryDto {
                path: None,
                scope: Some("client/src-tauri".into()),
                line_count: None,
                match_count: Some(2),
                truncated: false,
            }))
        );
    }

    #[test]
    fn owned_agent_tool_completed_projection_derives_command_git_and_web_summaries() {
        let command = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-command","toolName":"command","ok":true,"summary":"command","output":{"kind":"command","argv":["pnpm","test"],"cwd":"client","exitCode":0,"timedOut":false,"stdoutTruncated":true,"stderrTruncated":false,"stdoutRedacted":false,"stderrRedacted":true}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("command tool item");
        assert_eq!(
            command.tool_summary,
            Some(ToolResultSummaryDto::Command(CommandToolResultSummaryDto {
                exit_code: Some(0),
                timed_out: false,
                stdout_truncated: true,
                stderr_truncated: false,
                stdout_redacted: false,
                stderr_redacted: true,
            }))
        );

        let git = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-git","toolName":"git_diff","ok":true,"summary":"git","output":{"kind":"git_diff","scope":"worktree","changedFiles":3,"truncated":true,"baseRevision":"HEAD~1"}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("git tool item");
        assert_eq!(
            git.tool_summary,
            Some(ToolResultSummaryDto::Git(GitToolResultSummaryDto {
                scope: Some(GitToolResultScopeDto::Worktree),
                changed_files: 3,
                truncated: true,
                base_revision: Some("HEAD~1".into()),
            }))
        );

        let web = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-web","toolName":"web_fetch","ok":true,"summary":"web","output":{"kind":"web_fetch","url":"https://example.com","finalUrl":"https://www.example.com/","contentKind":"html","contentType":"text/html","truncated":false}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("web tool item");
        assert_eq!(
            web.tool_summary,
            Some(ToolResultSummaryDto::Web(WebToolResultSummaryDto {
                target: "https://example.com".into(),
                result_count: None,
                final_url: Some("https://www.example.com/".into()),
                content_kind: Some(WebToolResultContentKindDto::Html),
                content_type: Some("text/html".into()),
                truncated: false,
            }))
        );
    }

    #[test]
    fn owned_agent_tool_completed_projection_previews_project_context_results() {
        let context = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-context","toolName":"project_context","ok":true,"summary":"project_context returned 1 source-cited result(s).","output":{"kind":"project_context","action":"search_approved_memory","message":"project_context returned 1 source-cited result(s) for `lancedb memory`.","queryId":"query-1","resultCount":1,"results":[{"sourceKind":"approved_memory","sourceId":"memory-1","rank":1,"score":"0.9132","snippet":"LanceDB stores approved memory for later retrieval.","redactionState":"clean","citation":"agent_retrieval_results:query-1:1:memory-1"}]}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("project_context tool item");

        assert_eq!(context.kind, RuntimeStreamItemKind::Tool);
        assert_eq!(context.tool_name.as_deref(), Some("project_context"));
        let preview = serde_json::from_str::<serde_json::Value>(
            context
                .tool_result_preview
                .as_deref()
                .expect("model-visible project_context preview"),
        )
        .expect("decode project_context preview");
        assert_eq!(preview["toolCallId"], serde_json::json!("call-context"));
        assert_eq!(
            preview["output"]["kind"],
            serde_json::json!("project_context")
        );
        assert_eq!(
            preview["output"]["results"][0]["snippet"],
            serde_json::json!("LanceDB stores approved memory for later retrieval.")
        );
    }

    #[test]
    fn owned_agent_tool_completed_projection_previews_project_context_manifest_summary() {
        let context = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-context","toolName":"project_context","ok":true,"summary":"project_context returned the latest source-cited context manifest.","output":{"kind":"project_context","action":"explain_current_context_package","message":"project_context returned the latest source-cited context manifest.","resultCount":1,"manifest":{"kind":"provider_context_package_summary","manifestId":"manifest-1","contextHash":"abc123","citation":"agent_context_manifests:7","budget":{"estimatedTokens":4323},"policy":{"pressure":"low","action":"continue_now"},"contributors":{"includedCount":18,"excludedCount":1},"retrieval":{"deliveryModel":"tool_mediated","rawContextInjected":false,"resultCount":0},"tools":{"count":3,"names":["read","search","project_context"]},"promptFragments":{"count":2,"items":[{"id":"xero.soul"},{"id":"project.code_map"}]},"omitted":{"originalBytes":50000,"returnedBytes":2000,"fullManifestPersisted":true}}}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("project_context manifest tool item");

        let preview_text = context
            .tool_result_preview
            .as_deref()
            .expect("model-visible manifest preview");
        assert!(preview_text.contains("tool result: project_context call call-context ok=true"));
        assert!(preview_text.contains("action: explain_current_context_package"));
        assert!(preview_text.contains("estimated 4323 token(s)"));
        assert!(preview_text.contains("Active tools: read, search, project_context"));
        assert!(preview_text.contains("xeroCompact: schema=xero.model_visible_tool_result.v1"));
        assert!(serde_json::from_str::<serde_json::Value>(preview_text).is_err());
        assert!(!preview_text.contains("inputSchema"));
        assert!(!preview_text.contains("\\nBudget:"));
    }

    #[test]
    fn owned_agent_tool_completed_projection_previews_workspace_index_status() {
        let workspace_index = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-index","toolName":"workspace_index","ok":true,"summary":"Workspace index is Empty with 0 of 159 files indexed.","output":{"kind":"workspace_index","action":"status","message":"Workspace index is Empty with 0 of 159 files indexed.","status":{"projectId":"project_e77f0b6c2a26c565a4e5d4508f03ea51","state":"empty","indexVersion":1,"rootPath":"/Users/sn0w/Documents/dev/ahoy","storagePath":"/Users/sn0w/Library/Application Support/dev.sn0w.xero/projects/project_e77f0b6c2a26c565a4e5d4508f03ea51","totalFiles":159,"indexedFiles":0,"skippedFiles":34,"staleFiles":159,"symbolCount":0,"indexedBytes":0,"coveragePercent":0.0,"headSha":"88fd5bd86f9946771c2598bc62c9da6c969bc008","diagnostics":[{"severity":"warning","code":"workspace_index_empty","message":"Index is empty."}]},"results":[],"signals":[]}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("workspace index tool item");

        assert_eq!(workspace_index.kind, RuntimeStreamItemKind::Tool);
        assert_eq!(
            workspace_index.tool_name.as_deref(),
            Some("workspace_index")
        );
        let preview_text = workspace_index
            .tool_result_preview
            .as_deref()
            .expect("model-visible workspace index preview");
        assert!(preview_text.contains("tool result: workspace_index call call-index ok=true"));
        assert!(preview_text.contains("action: status"));
        assert!(preview_text.contains("status: state=empty; indexedFiles=0/159; skippedFiles=34; staleFiles=159; symbolCount=0; indexedBytes=0; coverage=0.0%; indexVersion=1"));
        assert!(preview_text.contains("root: /Users/sn0w/Documents/dev/ahoy"));
        assert!(
            preview_text.contains("diagnostics:\n- warning workspace_index_empty: Index is empty.")
        );
        assert!(preview_text.contains("xeroCompact: schema=xero.model_visible_tool_result.v1"));
        assert!(serde_json::from_str::<serde_json::Value>(preview_text).is_err());
        assert!(!preview_text.contains("storagePath"));
        assert!(!preview_text.contains("project_e77f0b6c2a26c565a4e5d4508f03ea51"));
    }

    #[test]
    fn owned_agent_context_events_project_as_project_context_tools() {
        let retrieval = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::RetrievalPerformed,
                r#"{"queryId":"query-1","resultCount":2,"summary":"Retrieved durable context from LanceDB."}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("retrieval tool item");

        assert_eq!(retrieval.kind, RuntimeStreamItemKind::Tool);
        assert_eq!(retrieval.tool_name.as_deref(), Some("project_context"));
        assert_eq!(
            retrieval.tool_call_id.as_deref(),
            Some("runtime-project-context:42:retrieval")
        );
        assert_eq!(retrieval.tool_state, Some(RuntimeToolCallState::Succeeded));
        assert_eq!(
            retrieval.detail.as_deref(),
            Some("action: retrieval, queryId: query-1, resultCount: 2 · Retrieved durable context from LanceDB.")
        );
        assert!(retrieval
            .tool_result_preview
            .as_deref()
            .is_some_and(|preview| preview.contains("\"queryId\": \"query-1\"")));

        let manifest = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ContextManifestRecorded,
                r#"{"manifestId":"manifest-1","turnIndex":3,"contextHash":"abc123"}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("manifest tool item");
        assert_eq!(manifest.kind, RuntimeStreamItemKind::Tool);
        assert_eq!(manifest.tool_name.as_deref(), Some("project_context"));
        assert_eq!(
            manifest.tool_call_id.as_deref(),
            Some("runtime-project-context:42:manifest")
        );
        assert!(manifest
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("action: context_manifest")));

        let memory = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::MemoryCandidateCaptured,
                r#"{"candidateId":"candidate-1","candidateKind":"project_fact","summary":"Captured a project memory candidate."}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("memory candidate tool item");
        assert_eq!(memory.kind, RuntimeStreamItemKind::Tool);
        assert_eq!(memory.tool_name.as_deref(), Some("project_context"));
        assert_eq!(
            memory.tool_call_id.as_deref(),
            Some("runtime-project-context:42:memory-candidate")
        );
        assert!(memory
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("action: memory_candidate")));
    }

    #[test]
    fn owned_agent_event_projection_populates_strict_activity_and_complete_fields() {
        let activity = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ReasoningSummary,
                r#"{"summary":"Checked repository instructions."}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("activity item");
        assert_eq!(activity.kind, RuntimeStreamItemKind::Activity);
        assert_eq!(activity.code.as_deref(), Some("owned_agent_reasoning"));
        assert_eq!(activity.title.as_deref(), Some("Reasoning"));
        assert_eq!(
            activity.detail.as_deref(),
            Some("Checked repository instructions.")
        );

        let complete = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::RunCompleted,
                r#"{"summary":"Owned agent run completed."}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("complete item");
        assert_eq!(complete.kind, RuntimeStreamItemKind::Complete);
        assert_eq!(
            complete.detail.as_deref(),
            Some("Owned agent run completed.")
        );
    }

    #[test]
    fn owned_agent_event_projection_preserves_transcript_role() {
        let user = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::MessageDelta,
                r#"{"role":"user","text":"Review this diff."}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("user transcript item");

        assert_eq!(user.kind, RuntimeStreamItemKind::Transcript);
        assert_eq!(
            user.transcript_role,
            Some(RuntimeStreamTranscriptRole::User)
        );
        assert_eq!(user.text.as_deref(), Some("Review this diff."));

        let assistant = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::MessageDelta,
                r#"{"role":"assistant","text":"I'll take a look."}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("assistant transcript item");

        assert_eq!(
            assistant.transcript_role,
            Some(RuntimeStreamTranscriptRole::Assistant)
        );
    }

    #[test]
    fn owned_agent_event_projection_preserves_transcript_delta_whitespace() {
        let assistant = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::MessageDelta,
                r#"{"role":"assistant","text":" instructions and natural wrapping "}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("assistant transcript item");

        assert_eq!(
            assistant.text.as_deref(),
            Some(" instructions and natural wrapping ")
        );

        let space_only = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::MessageDelta,
                r#"{"role":"assistant","text":" "}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("assistant transcript item");

        assert_eq!(space_only.text.as_deref(), Some(" "));
    }
}
