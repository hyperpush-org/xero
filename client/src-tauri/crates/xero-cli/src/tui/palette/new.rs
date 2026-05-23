//! `new` — start a fresh session. One-shot: creates a session through the
//! CLI and dismisses the palette. The next prompt the user sends will use
//! the new session.

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    string_field, OpenOutcome,
};

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let Some(project_id) = app.project.project_id.clone() else {
        return OpenOutcome::Closed {
            status: Some("No project bound — `register` this directory first.".to_owned()),
        };
    };
    match invoke_json(globals, &["session", "create", "--project-id", &project_id]) {
        Ok(value) => {
            let session = value.get("session").cloned().unwrap_or(value);
            let session_id = string_field(&session, "agentSessionId");
            if let Err(error) = app.discard_pending_attachments(globals) {
                return OpenOutcome::Closed {
                    status: Some(format!(
                        "Could not clear pending attachments: {} ({})",
                        error.message, error.code
                    )),
                };
            }
            app.reset_for_new_session((!session_id.is_empty()).then_some(session_id.clone()));
            if !session_id.is_empty() {
                super::super::app::sync_active_session_to_cloud_best_effort(globals, app);
            }
            OpenOutcome::Closed {
                status: Some(if session_id.is_empty() {
                    "New session.".to_owned()
                } else {
                    format!("New session: {}", session_id)
                }),
            }
        }
        Err(error) => OpenOutcome::Closed {
            status: Some(format!(
                "Could not start a new session: {} ({})",
                error.message, error.code
            )),
        },
    }
}
