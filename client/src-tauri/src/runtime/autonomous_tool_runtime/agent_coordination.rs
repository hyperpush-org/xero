use serde::{Deserialize, Serialize};

use super::{AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime};
use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::project_store,
    runtime::AUTONOMOUS_TOOL_AGENT_COORDINATION,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousAgentCoordinationAction {
    ListActiveAgents,
    ListReservations,
    CheckConflicts,
    ClaimReservation,
    ReleaseReservation,
    ExplainActivity,
    PublishMessage,
    ReadInbox,
    Acknowledge,
    Reply,
    MarkResolved,
    PromoteToContextCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousAgentCoordinationRequest {
    pub action: AutonomousAgentCoordinationAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<project_store::AgentCoordinationReservationOperation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reservation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<project_store::AgentMailboxItemType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<project_store::AgentMailboxPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousAgentCoordinationOutput {
    pub action: AutonomousAgentCoordinationAction,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_agents: Vec<project_store::AgentCoordinationPresenceRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reservations: Vec<project_store::AgentFileReservationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<project_store::AgentFileReservationConflictRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<project_store::AgentCoordinationEventRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mailbox: Vec<project_store::AgentMailboxDeliveryRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mailbox_item: Option<project_store::AgentMailboxItemRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_record_id: Option<String>,
    pub override_recorded: bool,
}

impl Eq for AutonomousAgentCoordinationOutput {}

impl AutonomousToolRuntime {
    pub fn agent_coordination(
        &self,
        request: AutonomousAgentCoordinationRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let output = self.execute_agent_coordination(request)?;
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_AGENT_COORDINATION.into(),
            summary: output.message.clone(),
            command_result: None,
            output: AutonomousToolOutput::AgentCoordination(output),
        })
    }

    fn execute_agent_coordination(
        &self,
        request: AutonomousAgentCoordinationRequest,
    ) -> CommandResult<AutonomousAgentCoordinationOutput> {
        let run_context = self.agent_run_context().cloned().ok_or_else(|| {
            CommandError::system_fault(
                "agent_coordination_run_context_missing",
                "Agent coordination requires an active owned-agent run context.",
            )
        })?;
        let now = now_timestamp();
        match request.action {
            AutonomousAgentCoordinationAction::ListActiveAgents => {
                let active_agents = project_store::list_active_agent_coordination_presence(
                    self.repo_root(),
                    &run_context.project_id,
                    Some(&run_context.run_id),
                    &now,
                    request.limit.unwrap_or(25),
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!("Found {} active agent(s).", active_agents.len()),
                    active_agents,
                    reservations: Vec::new(),
                    conflicts: Vec::new(),
                    events: Vec::new(),
                    mailbox: Vec::new(),
                    mailbox_item: None,
                    promoted_record_id: None,
                    override_recorded: false,
                })
            }
            AutonomousAgentCoordinationAction::ListReservations => {
                let reservations = project_store::list_active_agent_file_reservations(
                    self.repo_root(),
                    &run_context.project_id,
                    Some(&run_context.run_id),
                    &now,
                    request.limit.unwrap_or(50),
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!("Found {} active file reservation(s).", reservations.len()),
                    active_agents: Vec::new(),
                    reservations,
                    conflicts: Vec::new(),
                    events: Vec::new(),
                    mailbox: Vec::new(),
                    mailbox_item: None,
                    promoted_record_id: None,
                    override_recorded: false,
                })
            }
            AutonomousAgentCoordinationAction::CheckConflicts => {
                let paths = coordination_paths_from_request(&request)?;
                let conflicts = project_store::check_agent_file_reservation_conflicts(
                    self.repo_root(),
                    &run_context.project_id,
                    &run_context.run_id,
                    &paths,
                    &now,
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!("Found {} file reservation conflict(s).", conflicts.len()),
                    active_agents: Vec::new(),
                    reservations: Vec::new(),
                    conflicts,
                    events: Vec::new(),
                    mailbox: Vec::new(),
                    mailbox_item: None,
                    promoted_record_id: None,
                    override_recorded: false,
                })
            }
            AutonomousAgentCoordinationAction::ClaimReservation => {
                let paths = coordination_paths_from_request(&request)?;
                let claim = project_store::claim_agent_file_reservations(
                    self.repo_root(),
                    &project_store::ClaimAgentFileReservationRequest {
                        project_id: run_context.project_id.clone(),
                        owner_run_id: run_context.run_id.clone(),
                        paths,
                        operation: request.operation.unwrap_or(
                            project_store::AgentCoordinationReservationOperation::Editing,
                        ),
                        note: request.note.clone(),
                        override_reason: request.override_reason.clone(),
                        claimed_at: now,
                        lease_seconds: None,
                    },
                )?;
                let conflict_count = claim.conflicts.len();
                let claimed_count = claim.claimed.len();
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: if conflict_count == 0 {
                        format!("Claimed {claimed_count} file reservation(s).")
                    } else {
                        format!(
                            "Found {conflict_count} conflict(s); claimed {claimed_count} reservation(s)."
                        )
                    },
                    active_agents: Vec::new(),
                    reservations: claim.claimed,
                    conflicts: claim.conflicts,
                    events: Vec::new(),
                    mailbox: Vec::new(),
                    mailbox_item: None,
                    promoted_record_id: None,
                    override_recorded: claim.override_recorded,
                })
            }
            AutonomousAgentCoordinationAction::ReleaseReservation => {
                let released = project_store::release_agent_file_reservations(
                    self.repo_root(),
                    &project_store::ReleaseAgentFileReservationRequest {
                        project_id: run_context.project_id,
                        owner_run_id: run_context.run_id,
                        reservation_id: request.reservation_id.clone(),
                        paths: coordination_paths_from_optional_request(&request)?,
                        release_reason: request
                            .release_reason
                            .clone()
                            .unwrap_or_else(|| "released_by_agent".into()),
                        released_at: now,
                    },
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!("Released {} file reservation(s).", released.len()),
                    active_agents: Vec::new(),
                    reservations: released,
                    conflicts: Vec::new(),
                    events: Vec::new(),
                    mailbox: Vec::new(),
                    mailbox_item: None,
                    promoted_record_id: None,
                    override_recorded: false,
                })
            }
            AutonomousAgentCoordinationAction::ExplainActivity => {
                let context = project_store::active_agent_coordination_context(
                    self.repo_root(),
                    &run_context.project_id,
                    &run_context.run_id,
                    &now,
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!(
                        "{} active agent(s), {} reservation(s), {} recent event(s), {} mailbox item(s).",
                        context.presence.len(),
                        context.reservations.len(),
                        context.events.len(),
                        context.mailbox.len()
                    ),
                    active_agents: context.presence,
                    reservations: context.reservations,
                    conflicts: Vec::new(),
                    events: context.events,
                    mailbox: context.mailbox,
                    mailbox_item: None,
                    promoted_record_id: None,
                    override_recorded: false,
                })
            }
            AutonomousAgentCoordinationAction::PublishMessage => {
                let item = project_store::publish_agent_mailbox_item(
                    self.repo_root(),
                    &project_store::NewAgentMailboxItemRecord {
                        project_id: run_context.project_id,
                        sender_run_id: run_context.run_id,
                        item_type: request
                            .item_type
                            .unwrap_or(project_store::AgentMailboxItemType::HeadsUp),
                        parent_item_id: request.item_id.clone(),
                        target_agent_session_id: request.target_agent_session_id.clone(),
                        target_run_id: request.target_run_id.clone(),
                        target_role: request.target_role.clone(),
                        title: required_mailbox_text(request.title.as_deref(), "title")?,
                        body: required_mailbox_text(request.body.as_deref(), "body")?,
                        related_paths: coordination_paths_from_optional_request(&request)?,
                        priority: request
                            .priority
                            .unwrap_or(project_store::AgentMailboxPriority::Normal),
                        created_at: now,
                        ttl_seconds: request.ttl_seconds,
                    },
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!("Published temporary mailbox item `{}`.", item.item_id),
                    active_agents: Vec::new(),
                    reservations: Vec::new(),
                    conflicts: Vec::new(),
                    events: Vec::new(),
                    mailbox: Vec::new(),
                    mailbox_item: Some(item),
                    promoted_record_id: None,
                    override_recorded: false,
                })
            }
            AutonomousAgentCoordinationAction::ReadInbox => {
                let mailbox = project_store::list_agent_mailbox_inbox(
                    self.repo_root(),
                    &run_context.project_id,
                    &run_context.run_id,
                    &now,
                    request.limit.unwrap_or(25),
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!("Found {} temporary mailbox item(s).", mailbox.len()),
                    active_agents: Vec::new(),
                    reservations: Vec::new(),
                    conflicts: Vec::new(),
                    events: Vec::new(),
                    mailbox,
                    mailbox_item: None,
                    promoted_record_id: None,
                    override_recorded: false,
                })
            }
            AutonomousAgentCoordinationAction::Acknowledge => {
                let item_id = required_mailbox_text(request.item_id.as_deref(), "itemId")?;
                let item = project_store::acknowledge_agent_mailbox_item(
                    self.repo_root(),
                    &run_context.project_id,
                    &run_context.run_id,
                    &item_id,
                    &now,
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!("Acknowledged mailbox item `{}`.", item.item_id),
                    active_agents: Vec::new(),
                    reservations: Vec::new(),
                    conflicts: Vec::new(),
                    events: Vec::new(),
                    mailbox: Vec::new(),
                    mailbox_item: Some(item),
                    promoted_record_id: None,
                    override_recorded: false,
                })
            }
            AutonomousAgentCoordinationAction::Reply => {
                let item = project_store::reply_agent_mailbox_item(
                    self.repo_root(),
                    &project_store::ReplyAgentMailboxItemRecord {
                        project_id: run_context.project_id,
                        sender_run_id: run_context.run_id,
                        parent_item_id: required_mailbox_text(
                            request.item_id.as_deref(),
                            "itemId",
                        )?,
                        item_type: request.item_type,
                        title: request.title.clone(),
                        body: required_mailbox_text(request.body.as_deref(), "body")?,
                        related_paths: coordination_paths_from_optional_request(&request)?,
                        priority: request.priority,
                        created_at: now,
                        ttl_seconds: request.ttl_seconds,
                    },
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!("Replied with mailbox item `{}`.", item.item_id),
                    active_agents: Vec::new(),
                    reservations: Vec::new(),
                    conflicts: Vec::new(),
                    events: Vec::new(),
                    mailbox: Vec::new(),
                    mailbox_item: Some(item),
                    promoted_record_id: None,
                    override_recorded: false,
                })
            }
            AutonomousAgentCoordinationAction::MarkResolved => {
                let item = project_store::resolve_agent_mailbox_item(
                    self.repo_root(),
                    &project_store::ResolveAgentMailboxItemRecord {
                        project_id: run_context.project_id,
                        resolver_run_id: run_context.run_id,
                        item_id: required_mailbox_text(request.item_id.as_deref(), "itemId")?,
                        resolve_reason: request
                            .release_reason
                            .clone()
                            .or(request.note.clone())
                            .unwrap_or_else(|| "resolved_by_agent".into()),
                        resolved_at: now,
                    },
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!("Resolved mailbox item `{}`.", item.item_id),
                    active_agents: Vec::new(),
                    reservations: Vec::new(),
                    conflicts: Vec::new(),
                    events: Vec::new(),
                    mailbox: Vec::new(),
                    mailbox_item: Some(item),
                    promoted_record_id: None,
                    override_recorded: false,
                })
            }
            AutonomousAgentCoordinationAction::PromoteToContextCandidate => {
                let promotion = project_store::promote_agent_mailbox_item(
                    self.repo_root(),
                    &project_store::PromoteAgentMailboxItemRecord {
                        project_id: run_context.project_id,
                        promoter_run_id: run_context.run_id,
                        item_id: required_mailbox_text(request.item_id.as_deref(), "itemId")?,
                        title: request.title.clone(),
                        summary: request.summary.clone(),
                        promoted_at: now,
                    },
                )?;
                Ok(AutonomousAgentCoordinationOutput {
                    action: request.action,
                    message: format!(
                        "Promoted mailbox item `{}` to durable-context candidate `{}`.",
                        promotion.item.item_id, promotion.promoted_record_id
                    ),
                    active_agents: Vec::new(),
                    reservations: Vec::new(),
                    conflicts: Vec::new(),
                    events: Vec::new(),
                    mailbox: Vec::new(),
                    mailbox_item: Some(promotion.item),
                    promoted_record_id: Some(promotion.promoted_record_id),
                    override_recorded: false,
                })
            }
        }
    }
}

fn coordination_paths_from_request(
    request: &AutonomousAgentCoordinationRequest,
) -> CommandResult<Vec<String>> {
    let paths = coordination_paths_from_optional_request(request)?;
    if paths.is_empty() {
        return Err(CommandError::invalid_request("paths"));
    }
    Ok(paths)
}

fn coordination_paths_from_optional_request(
    request: &AutonomousAgentCoordinationRequest,
) -> CommandResult<Vec<String>> {
    let mut paths = request.paths.clone();
    if let Some(path) = request.path.clone() {
        paths.push(path);
    }
    Ok(paths
        .into_iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect())
}

fn required_mailbox_text(value: Option<&str>, field: &'static str) -> CommandResult<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| CommandError::invalid_request(field))
}
