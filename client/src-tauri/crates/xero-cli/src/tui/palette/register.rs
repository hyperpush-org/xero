//! `register` — register the resolved project root with Xero. One-shot
//! action; closes the palette and sets a status line either way.

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    string_field, OpenOutcome,
};

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    if app.project.registered {
        return OpenOutcome::Closed {
            status: Some("This directory is already registered.".to_owned()),
        };
    }
    let root = app.project.root.to_string_lossy().into_owned();
    let mut name = root.rsplit('/').next().unwrap_or("xero-project").to_owned();
    if name.is_empty() {
        name = "xero-project".into();
    }
    let args = vec![
        "project",
        "create",
        "--root",
        root.as_str(),
        "--name",
        name.as_str(),
    ];
    match invoke_json(globals, &args) {
        Ok(value) => {
            let project_id = string_field(&value, "projectId");
            if !project_id.is_empty() {
                app.project.project_id = Some(project_id.clone());
                app.project.registered = true;
            }
            OpenOutcome::Closed {
                status: Some(format!("Registered project {} (id: {}).", name, project_id)),
            }
        }
        Err(error) => OpenOutcome::Closed {
            status: Some(format!(
                "Could not register this directory: {} ({})",
                error.message, error.code
            )),
        },
    }
}
