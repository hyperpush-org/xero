//! Resolve the current working directory to a Xero project at startup.
//!
//! 1. cwd → `git rev-parse --show-toplevel` (fallback to cwd itself).
//! 2. Look up the registered project whose `rootPath` matches the resolved root.
//! 3. If no project exists yet, import the resolved root into app-data.

use std::{env, path::PathBuf, process::Command};

use serde_json::Value as JsonValue;

use crate::GlobalOptions;

use super::app::invoke_json;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ResolvedProject {
    pub project_id: Option<String>,
    pub root: PathBuf,
    pub branch: Option<String>,
    pub display_path: String,
    pub registered: bool,
}

pub fn resolve(globals: &GlobalOptions) -> ResolvedProject {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root = git_toplevel(&cwd).unwrap_or_else(|| cwd.clone());
    let branch = git_branch(&root);
    resolve_root(globals, root, branch)
}

fn resolve_root(globals: &GlobalOptions, root: PathBuf, branch: Option<String>) -> ResolvedProject {
    let project_id =
        lookup_registered_project(globals, &root).or_else(|| auto_import_project(globals, &root));
    let display_path = display_path_for(&root);
    let registered = project_id.is_some();
    ResolvedProject {
        project_id,
        root,
        branch,
        display_path,
        registered,
    }
}

fn git_toplevel(path: &std::path::Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if line.is_empty() {
        return None;
    }
    Some(PathBuf::from(line))
}

fn git_branch(path: &std::path::Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("symbolic-ref")
        .arg("--short")
        .arg("HEAD")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
}

fn lookup_registered_project(globals: &GlobalOptions, root: &std::path::Path) -> Option<String> {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let value = match invoke_json(globals, &["project", "list"]) {
        Ok(value) => value,
        Err(_) => return None,
    };
    let projects = value.get("projects").and_then(JsonValue::as_array)?;
    projects.iter().find_map(|project| {
        let registered_root = project.get("rootPath").and_then(JsonValue::as_str)?;
        let registered = PathBuf::from(registered_root);
        let registered_canonical = registered.canonicalize().unwrap_or(registered);
        if registered_canonical == canonical {
            project
                .get("projectId")
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
        } else {
            None
        }
    })
}

fn auto_import_project(globals: &GlobalOptions, root: &std::path::Path) -> Option<String> {
    let root = root.to_string_lossy().into_owned();
    let value = invoke_json(globals, &["project", "import", "--path", &root]).ok()?;
    project_id_from_import_response(&value)
}

fn project_id_from_import_response(value: &JsonValue) -> Option<String> {
    value
        .get("project")
        .and_then(|project| project.get("projectId"))
        .or_else(|| value.get("projectId"))
        .and_then(JsonValue::as_str)
        .filter(|project_id| !project_id.trim().is_empty())
        .map(str::to_owned)
}

fn display_path_for(root: &std::path::Path) -> String {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if let Some(home) = home_dir() {
        if let Ok(suffix) = canonical.strip_prefix(&home) {
            let suffix = suffix.to_string_lossy();
            return if suffix.is_empty() {
                "~".to_owned()
            } else {
                format!("~/{}", suffix)
            };
        }
    }
    canonical.to_string_lossy().into_owned()
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CliError, GlobalOptions, OutputMode, TuiCommandAdapter};
    use serde_json::json;
    use std::{
        path::Path,
        sync::{Arc, Mutex},
    };

    #[derive(Default)]
    struct AutoImportAdapter {
        calls: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl TuiCommandAdapter for AutoImportAdapter {
        fn invoke_json(
            &self,
            _state_dir: &Path,
            args: &[String],
        ) -> Option<Result<JsonValue, CliError>> {
            self.calls.lock().expect("record calls").push(args.to_vec());
            match args.first().map(String::as_str) {
                Some("project") if args.get(1).map(String::as_str) == Some("list") => {
                    Some(Ok(json!({ "projects": [] })))
                }
                Some("project") if args.get(1).map(String::as_str) == Some("import") => {
                    Some(Ok(json!({
                        "kind": "projectImport",
                        "project": {
                            "projectId": "project-auto"
                        }
                    })))
                }
                _ => None,
            }
        }
    }

    fn globals_with_adapter(adapter: Arc<dyn TuiCommandAdapter>) -> GlobalOptions {
        GlobalOptions {
            output_mode: OutputMode::Json,
            ci: false,
            state_dir: PathBuf::from("/tmp/xero-project-test"),
            tui_adapter: Some(adapter),
        }
    }

    #[test]
    fn resolve_root_imports_current_root_when_not_registered() {
        let adapter = AutoImportAdapter::default();
        let calls = Arc::clone(&adapter.calls);
        let globals = globals_with_adapter(Arc::new(adapter));

        let project = resolve_root(&globals, PathBuf::from("/tmp/xero-auto-root"), None);

        assert_eq!(project.project_id.as_deref(), Some("project-auto"));
        assert!(project.registered);
        let calls = calls.lock().expect("recorded calls");
        assert_eq!(
            calls.get(1),
            Some(&vec![
                "project".to_owned(),
                "import".to_owned(),
                "--path".to_owned(),
                "/tmp/xero-auto-root".to_owned()
            ])
        );
    }
}
