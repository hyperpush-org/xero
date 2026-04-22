use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use crate::{
    auth::now_timestamp,
    db::project_store::{
        self, NotificationDispatchEnqueueStatus, RuntimeActionRequiredUpsertRecord,
    },
};

use super::{
    live_events::{append_live_event, persist_autonomous_live_event, sanitize_text_fragment},
    persistence::{persist_sidecar_runtime_error, protocol_diagnostic_into_record},
    PtyEventNormalizer, SidecarSharedState, SupervisorEventHub, INTERACTIVE_BOUNDARY_ACTION_TYPE,
    INTERACTIVE_BOUNDARY_CHECKPOINT_SUMMARY, INTERACTIVE_BOUNDARY_DETAIL,
    INTERACTIVE_BOUNDARY_TITLE,
};
use crate::runtime::protocol::SupervisorLiveEventPayload;

#[derive(Debug, Clone)]
pub(super) struct ActiveInteractiveBoundary {
    pub(super) boundary_id: String,
    pub(super) action_id: String,
    pub(super) action_type: String,
    pub(super) title: String,
    pub(super) detail: String,
    pub(super) detected_at: String,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct StructuredRuntimeBoundary<'a> {
    pub(super) action_id: &'a str,
    pub(super) boundary_id: &'a str,
    pub(super) action_type: &'a str,
    pub(super) title: &'a str,
    pub(super) detail: &'a str,
}

#[derive(Debug, Clone)]
struct RuntimeBoundaryCandidate {
    boundary_id: Option<String>,
    action_id: Option<String>,
    action_type: String,
    title: String,
    detail: String,
    checkpoint_summary: String,
}

impl PtyEventNormalizer {
    fn take_interactive_boundary_candidate(&mut self) -> Option<RuntimeBoundaryCandidate> {
        if self.pending.is_empty() {
            return None;
        }

        let pending = String::from_utf8(self.pending.clone()).ok()?;
        let fragment = pending.trim_end_matches(['\r', '\n']);
        let sanitized = match sanitize_text_fragment(fragment) {
            Ok(Some(text)) => text,
            Ok(None) | Err(()) => return None,
        };

        if !looks_like_interactive_boundary(&sanitized) {
            return None;
        }

        self.pending.clear();
        Some(default_interactive_boundary_candidate())
    }
}

fn default_interactive_boundary_candidate() -> RuntimeBoundaryCandidate {
    RuntimeBoundaryCandidate {
        boundary_id: None,
        action_id: None,
        action_type: INTERACTIVE_BOUNDARY_ACTION_TYPE.into(),
        title: INTERACTIVE_BOUNDARY_TITLE.into(),
        detail: INTERACTIVE_BOUNDARY_DETAIL.into(),
        checkpoint_summary: INTERACTIVE_BOUNDARY_CHECKPOINT_SUMMARY.into(),
    }
}

fn looks_like_interactive_boundary(fragment: &str) -> bool {
    let trimmed = fragment.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 160 {
        return false;
    }

    let normalized = trimmed.to_ascii_lowercase();
    if matches!(normalized.as_str(), "$" | "#" | "%" | ">") {
        return false;
    }

    let has_prompt_suffix = matches!(trimmed.chars().last(), Some(':' | '?' | '>' | ']' | ')'));
    if !has_prompt_suffix {
        return false;
    }

    let has_prompt_keyword = [
        "enter",
        "input",
        "provide",
        "type",
        "passphrase",
        "password",
        "token",
        "code",
        "continue",
        "confirm",
        "approve",
        "answer",
        "select",
        "choose",
        "name",
        "email",
        "y/n",
        "yes/no",
    ]
    .into_iter()
    .any(|keyword| normalized.contains(keyword));

    let looks_like_prompt_sentence = trimmed.contains(' ') || has_prompt_keyword;
    let looks_like_log_prefix = normalized.starts_with("error:")
        || normalized.starts_with("warning:")
        || normalized.starts_with("info:");

    looks_like_prompt_sentence && !looks_like_log_prefix
}

pub(super) fn checkpoint_summary_for_runtime_boundary(action_type: &str, title: &str) -> String {
    if action_type == INTERACTIVE_BOUNDARY_ACTION_TYPE {
        INTERACTIVE_BOUNDARY_CHECKPOINT_SUMMARY.into()
    } else {
        format!("Detached runtime blocked on `{title}` and is awaiting operator approval.")
    }
}

pub(super) fn emit_interactive_boundary_if_detected(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    normalizer: &mut PtyEventNormalizer,
) {
    let Some(candidate) = normalizer.take_interactive_boundary_candidate() else {
        return;
    };

    emit_runtime_boundary_candidate(repo_root, shared, event_hub, persistence_lock, candidate);
}

pub(super) fn emit_structured_runtime_boundary(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    boundary: StructuredRuntimeBoundary<'_>,
) {
    emit_runtime_boundary_candidate(
        repo_root,
        shared,
        event_hub,
        persistence_lock,
        RuntimeBoundaryCandidate {
            boundary_id: Some(boundary.boundary_id.to_string()),
            action_id: Some(boundary.action_id.to_string()),
            action_type: boundary.action_type.to_string(),
            title: boundary.title.to_string(),
            detail: boundary.detail.to_string(),
            checkpoint_summary: checkpoint_summary_for_runtime_boundary(
                boundary.action_type,
                boundary.title,
            ),
        },
    );
}

fn emit_runtime_boundary_candidate(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    candidate: RuntimeBoundaryCandidate,
) {
    let (
        project_id,
        run_id,
        runtime_kind,
        session_id,
        flow_id,
        transport_endpoint,
        started_at,
        last_heartbeat_at,
        last_error,
        boundary,
    ) = {
        let mut state = shared.lock().expect("sidecar state lock poisoned");
        if let Some(active_boundary) = state.active_boundary.as_ref() {
            let same_boundary = candidate
                .boundary_id
                .as_deref()
                .is_some_and(|boundary_id| boundary_id == active_boundary.boundary_id);
            let same_action = candidate
                .action_id
                .as_deref()
                .is_some_and(|action_id| action_id == active_boundary.action_id);
            if same_boundary && same_action {
                return;
            }
            return;
        }

        let boundary_id = match candidate.boundary_id.as_ref() {
            Some(boundary_id) => boundary_id.clone(),
            None => {
                state.next_boundary_serial = state.next_boundary_serial.saturating_add(1);
                format!("boundary-{}", state.next_boundary_serial)
            }
        };
        let boundary = ActiveInteractiveBoundary {
            boundary_id,
            action_id: String::new(),
            action_type: candidate.action_type.clone(),
            title: candidate.title.clone(),
            detail: candidate.detail.clone(),
            detected_at: now_timestamp(),
        };
        (
            state.project_id.clone(),
            state.run_id.clone(),
            state.runtime_kind.clone(),
            state.session_id.clone(),
            state.flow_id.clone(),
            state.endpoint.clone(),
            state.started_at.clone(),
            state.last_heartbeat_at.clone(),
            state
                .last_error
                .clone()
                .map(protocol_diagnostic_into_record),
            boundary,
        )
    };

    if let Some(expected_action_id) = candidate.action_id.as_deref() {
        match project_store::derive_runtime_action_id(
            &session_id,
            flow_id.as_deref(),
            &run_id,
            &boundary.boundary_id,
            &boundary.action_type,
        ) {
            Ok(canonical_action_id) if canonical_action_id == expected_action_id => {}
            Ok(_) => {
                reject_runtime_boundary_candidate(
                    repo_root,
                    shared,
                    event_hub,
                    persistence_lock,
                    "runtime_action_identity_invalid",
                    "Cadence rejected the structured runtime boundary because its action identity did not match the canonical run and boundary scope.",
                );
                return;
            }
            Err(error) => {
                reject_runtime_boundary_candidate(
                    repo_root,
                    shared,
                    event_hub,
                    persistence_lock,
                    &error.code,
                    "Cadence rejected the structured runtime boundary because its action identity could not be validated safely.",
                );
                return;
            }
        }
    }

    let persisted = {
        let _guard = persistence_lock
            .lock()
            .expect("runtime supervisor persistence lock poisoned");
        project_store::upsert_runtime_action_required(
            repo_root,
            &RuntimeActionRequiredUpsertRecord {
                project_id,
                run_id,
                runtime_kind,
                session_id,
                flow_id,
                transport_endpoint,
                started_at,
                last_heartbeat_at,
                last_error,
                boundary_id: boundary.boundary_id.clone(),
                action_type: boundary.action_type.clone(),
                title: boundary.title.clone(),
                detail: boundary.detail.clone(),
                checkpoint_summary: candidate.checkpoint_summary.clone(),
                created_at: boundary.detected_at.clone(),
            },
        )
    };

    match persisted {
        Ok(persisted) => {
            let action_id = persisted.approval_request.action_id.clone();
            let boundary_id = boundary.boundary_id.clone();
            let notification_dispatch_outcome = persisted.notification_dispatch_outcome.clone();
            {
                let mut state = shared.lock().expect("sidecar state lock poisoned");
                state.active_boundary = Some(ActiveInteractiveBoundary {
                    action_id: action_id.clone(),
                    ..boundary.clone()
                });
                state.last_checkpoint_sequence = persisted.runtime_run.last_checkpoint_sequence;
                state.last_checkpoint_at = persisted.runtime_run.last_checkpoint_at.clone();
            }

            append_live_event(
                shared,
                event_hub,
                &SupervisorLiveEventPayload::ActionRequired {
                    action_id: action_id.clone(),
                    boundary_id,
                    action_type: boundary.action_type.clone(),
                    title: boundary.title.clone(),
                    detail: boundary.detail.clone(),
                },
            );
            persist_autonomous_live_event(
                repo_root,
                shared,
                event_hub,
                persistence_lock,
                &SupervisorLiveEventPayload::ActionRequired {
                    action_id: action_id.clone(),
                    boundary_id: boundary.boundary_id.clone(),
                    action_type: boundary.action_type.clone(),
                    title: boundary.title.clone(),
                    detail: boundary.detail.clone(),
                },
            );

            match notification_dispatch_outcome.status {
                NotificationDispatchEnqueueStatus::Enqueued => {
                    append_live_event(
                        shared,
                        event_hub,
                        &SupervisorLiveEventPayload::Activity {
                            code: notification_dispatch_outcome
                                .code
                                .unwrap_or_else(|| "notification_dispatch_enqueued".into()),
                            title: "Notification dispatch fan-out enqueued".into(),
                            detail: Some(format!(
                                "Cadence enqueued {} notification dispatch route(s) for pending action `{action_id}`.",
                                notification_dispatch_outcome.dispatch_count
                            )),
                        },
                    );
                }
                NotificationDispatchEnqueueStatus::Skipped => {
                    append_live_event(
                        shared,
                        event_hub,
                        &SupervisorLiveEventPayload::Activity {
                            code: notification_dispatch_outcome
                                .code
                                .unwrap_or_else(|| "notification_dispatch_enqueue_skipped".into()),
                            title: "Notification dispatch fan-out skipped".into(),
                            detail: Some(
                                notification_dispatch_outcome
                                    .message
                                    .unwrap_or_else(|| {
                                        "Cadence skipped notification dispatch fan-out after persisting the pending runtime boundary."
                                            .into()
                                    }),
                            ),
                        },
                    );
                }
            }
        }
        Err(error) => reject_runtime_boundary_candidate(
            repo_root,
            shared,
            event_hub,
            persistence_lock,
            &error.code,
            "Cadence could not persist the runtime boundary, so the last truthful runtime snapshot remains active.",
        ),
    }
}

fn reject_runtime_boundary_candidate(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    code: &str,
    safe_detail: &str,
) {
    append_live_event(
        shared,
        event_hub,
        &SupervisorLiveEventPayload::Activity {
            code: code.into(),
            title: "Runtime boundary persistence failed".into(),
            detail: Some(safe_detail.into()),
        },
    );
    let _ = persist_sidecar_runtime_error(repo_root, shared, persistence_lock, code, safe_detail);
}
