//! `skills` — list installed skills and plugins side by side.

use crossterm::event::KeyEvent;

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    array_field, empty_detail, error_detail, rows_detail, string_field, DetailOutcome, DetailRow,
    DetailState, OpenOutcome,
};

const ID: &str = "skills";

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let mut rows: Vec<DetailRow> = Vec::new();

    let mut skills_args = vec!["skill", "list"];
    if let Some(project_id) = app.project.project_id.as_deref() {
        skills_args.push("--project-id");
        skills_args.push(project_id);
    }
    match invoke_json(globals, &skills_args) {
        Ok(value) => {
            for skill in array_field(&value, "skills") {
                let name = string_field(skill, "name");
                let skill_id = string_field(skill, "skillId");
                let trust = string_field(skill, "trustState");
                rows.push(DetailRow {
                    title: format!(
                        "skill · {}",
                        if name.is_empty() {
                            skill_id.clone()
                        } else {
                            name
                        }
                    ),
                    subtitle: Some(format!(
                        "{}{}",
                        skill_id,
                        if trust.is_empty() {
                            String::new()
                        } else {
                            format!(" · {}", trust)
                        }
                    )),
                    payload: skill.clone(),
                });
            }
        }
        Err(error) => {
            return error_detail(ID, "Skills & plugins", error);
        }
    }

    let mut plugins_args = vec!["plugin", "list"];
    if let Some(project_id) = app.project.project_id.as_deref() {
        plugins_args.push("--project-id");
        plugins_args.push(project_id);
    }
    if let Ok(value) = invoke_json(globals, &plugins_args) {
        for plugin in array_field(&value, "plugins") {
            let name = string_field(plugin, "name");
            let plugin_id = string_field(plugin, "pluginId");
            let version = string_field(plugin, "version");
            let state = string_field(plugin, "pluginState");
            rows.push(DetailRow {
                title: format!(
                    "plugin · {}",
                    if name.is_empty() {
                        plugin_id.clone()
                    } else {
                        name
                    }
                ),
                subtitle: Some(format!(
                    "{}{}{}",
                    plugin_id,
                    if version.is_empty() {
                        String::new()
                    } else {
                        format!(" · v{}", version)
                    },
                    if state.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", state)
                    }
                )),
                payload: plugin.clone(),
            });
        }
    }

    if rows.is_empty() {
        return empty_detail(ID, "Skills & plugins", "No skills or plugins installed.");
    }
    rows_detail(ID, "Skills & plugins", Some("esc back"), rows)
}

pub fn handle_key(
    _app: &mut App,
    _detail: &mut DetailState,
    _key: KeyEvent,
    _globals: &GlobalOptions,
) -> DetailOutcome {
    DetailOutcome::Stay
}
