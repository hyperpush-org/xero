//! `git` — show `xero git status` for the current project.

use crossterm::event::KeyEvent;

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    body_detail, empty_detail, error_detail, DetailOutcome, DetailState, OpenOutcome,
};

const ID: &str = "git";

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let mut args = vec!["git", "status"];
    if let Some(project_id) = app.project.project_id.as_deref() {
        args.push("--project-id");
        args.push(project_id);
    } else if let Some(root) = app.project.root.to_str() {
        args.push("--repo");
        args.push(root);
    }
    let value = match invoke_json(globals, &args) {
        Ok(value) => value,
        Err(error) => return error_detail(ID, "Git status", error),
    };
    let text = value
        .get("output")
        .and_then(|out| out.as_str())
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("text")
                .and_then(|out| out.as_str())
                .map(str::to_owned)
        });
    let Some(text) = text else {
        return empty_detail(ID, "Git status", "No git status output.");
    };
    if text.trim().is_empty() {
        return empty_detail(ID, "Git status", "Working tree clean.");
    }
    let body = text.lines().map(|line| line.to_owned()).collect::<Vec<_>>();
    body_detail(ID, "Git status", Some("read-only · esc back"), body)
}

pub fn handle_key(
    _app: &mut App,
    _detail: &mut DetailState,
    _key: KeyEvent,
    _globals: &GlobalOptions,
) -> DetailOutcome {
    DetailOutcome::Stay
}
