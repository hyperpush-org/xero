use std::{collections::HashSet, path::Path};

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{CommandResult, ListProjectsResponseDto, HARNESS_FIXTURE_PROJECT_ID},
    db, registry,
    state::DesktopState,
};

pub(crate) fn load_projects_from_registry(
    registry_path: &Path,
) -> CommandResult<ListProjectsResponseDto> {
    let projects = load_visible_project_summaries_from_registry(registry_path)?
        .into_iter()
        .map(|record| record.project)
        .collect();

    Ok(ListProjectsResponseDto { projects })
}

pub(crate) fn load_visible_project_summaries_from_registry(
    registry_path: &Path,
) -> CommandResult<Vec<registry::RegistryProjectSummaryRecord>> {
    db::configure_project_database_paths(registry_path);
    let registry_projects = registry::read_project_summaries(registry_path)?;

    let mut projects = Vec::new();
    let mut seen_project_ids = HashSet::new();
    let mut seen_root_paths = HashSet::new();
    let mut live_root_records = Vec::new();
    let mut pruned_stale_roots = false;

    for record in registry_projects {
        if record.project.id == HARNESS_FIXTURE_PROJECT_ID {
            live_root_records.push(record.registry.clone());
            continue;
        }

        if !Path::new(&record.registry.root_path).is_dir() {
            pruned_stale_roots = true;
            continue;
        }

        live_root_records.push(record.registry.clone());

        if seen_project_ids.insert(record.project.id.clone())
            && seen_root_paths.insert(record.registry.root_path.clone())
        {
            projects.push(record);
        }
    }

    if pruned_stale_roots {
        let _ = registry::replace_projects(registry_path, live_root_records);
    }

    Ok(projects)
}

#[tauri::command]
pub fn list_projects<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<ListProjectsResponseDto> {
    let registry_path = state.global_db_path(&app)?;
    load_projects_from_registry(&registry_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_record(
        project_id: &str,
        repository_id: &str,
        root_path: &Path,
    ) -> registry::RegistryProjectRecord {
        registry::RegistryProjectRecord {
            project_id: project_id.into(),
            repository_id: repository_id.into(),
            root_path: root_path.to_string_lossy().into_owned(),
        }
    }

    #[test]
    fn visible_project_summaries_match_client_project_list() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let registry_path = tempdir.path().join("xero.db");
        let visible_root = tempdir.path().join("clippster-mono");
        let fixture_root = tempdir
            .path()
            .join("developer-tool-harness")
            .join("fixture");
        let stale_root = tempdir.path().join("stale");
        std::fs::create_dir_all(&visible_root).expect("visible root");
        std::fs::create_dir_all(&fixture_root).expect("fixture root");

        registry::replace_projects(
            &registry_path,
            vec![
                registry_record(HARNESS_FIXTURE_PROJECT_ID, "repo-fixture", &fixture_root),
                registry_record("project-clippster", "repo-clippster", &visible_root),
                registry_record("project-stale", "repo-stale", &stale_root),
            ],
        )
        .expect("seed registry");

        let visible = load_visible_project_summaries_from_registry(&registry_path)
            .expect("visible summaries");
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].project.id, "project-clippster");
        assert_eq!(visible[0].project.name, "clippster-mono");

        let client_projects = load_projects_from_registry(&registry_path)
            .expect("client project list")
            .projects;
        assert_eq!(client_projects.len(), 1);
        assert_eq!(client_projects[0].id, "project-clippster");

        let registry = registry::read_registry(&registry_path).expect("registry");
        assert!(
            registry
                .projects
                .iter()
                .any(|project| project.project_id == HARNESS_FIXTURE_PROJECT_ID),
            "the hidden harness fixture should stay registered for harness callers"
        );
        assert!(
            !registry
                .projects
                .iter()
                .any(|project| project.project_id == "project-stale"),
            "stale user projects should still be pruned"
        );
    }
}
