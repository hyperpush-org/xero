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
    let name = root
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("xero-project")
        .to_owned();
    let args = vec!["project", "import", "--path", root.as_str()];
    match invoke_json(globals, &args) {
        Ok(value) => {
            let project_id = value
                .get("project")
                .map(|project| string_field(project, "projectId"))
                .unwrap_or_else(|| string_field(&value, "projectId"));
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
