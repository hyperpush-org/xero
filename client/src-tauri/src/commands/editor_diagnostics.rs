use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime, State};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    commands::{
        backend_jobs::BackendCancellationToken, runtime_support::resolve_project_root,
        validate_non_empty, CommandError, CommandResult,
    },
    state::DesktopState,
};

const TYPECHECK_OUTPUT_LIMIT_BYTES: usize = 256 * 1024;
const TYPECHECK_DIAGNOSTIC_LIMIT: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunProjectTypecheckRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectDiagnosticSeverityDto {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectDiagnosticDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub severity: ProjectDiagnosticSeverityDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTypecheckStatusDto {
    Passed,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectTypecheckResponseDto {
    pub project_id: String,
    pub status: ProjectTypecheckStatusDto,
    pub source: String,
    pub command: Vec<String>,
    pub cwd: String,
    pub diagnostics: Vec<ProjectDiagnosticDto>,
    pub started_at: String,
    pub completed_at: String,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub lsp_servers: Vec<EditorLspServerStatusDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorLspServerStatusDto {
    pub server_id: String,
    pub language: String,
    pub command: String,
    pub args: Vec<String>,
    pub available: bool,
    pub supports_diagnostics: bool,
    pub supports_symbols: bool,
    pub supports_hover: bool,
    pub supports_completion: bool,
    pub supports_definition: bool,
    pub supports_references: bool,
    pub supports_rename: bool,
    pub supports_code_actions: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_suggestion: Option<EditorLspInstallSuggestionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorLspInstallSuggestionDto {
    pub reason: String,
    pub candidate_commands: Vec<EditorLspInstallCommandDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorLspInstallCommandDto {
    pub label: String,
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct LspServerDescriptor {
    server_id: &'static str,
    language: &'static str,
    command: &'static str,
    args: &'static [&'static str],
}

const LSP_SERVER_DESCRIPTORS: &[LspServerDescriptor] = &[
    LspServerDescriptor {
        server_id: "typescript_language_server",
        language: "TypeScript/JavaScript",
        command: "typescript-language-server",
        args: &["--stdio"],
    },
    LspServerDescriptor {
        server_id: "rust_analyzer",
        language: "Rust",
        command: "rust-analyzer",
        args: &[],
    },
    LspServerDescriptor {
        server_id: "pyright",
        language: "Python",
        command: "pyright-langserver",
        args: &["--stdio"],
    },
    LspServerDescriptor {
        server_id: "gopls",
        language: "Go",
        command: "gopls",
        args: &["serve"],
    },
    LspServerDescriptor {
        server_id: "vscode_json_language_server",
        language: "JSON",
        command: "vscode-json-language-server",
        args: &["--stdio"],
    },
    LspServerDescriptor {
        server_id: "clangd",
        language: "C/C++",
        command: "clangd",
        args: &[],
    },
];

#[tauri::command]
pub async fn run_project_typecheck<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RunProjectTypecheckRequestDto,
) -> CommandResult<ProjectTypecheckResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    drop(app);

    jobs.run_blocking_latest(
        format!("project-typecheck:{project_id}"),
        "project typecheck",
        move |cancellation| run_project_typecheck_at_path(project_id, project_root, cancellation),
    )
    .await
}

fn run_project_typecheck_at_path(
    project_id: String,
    project_root: PathBuf,
    cancellation: BackendCancellationToken,
) -> CommandResult<ProjectTypecheckResponseDto> {
    cancellation.check_cancelled("project typecheck")?;
    let started = now_rfc3339();
    let timer = Instant::now();
    let cwd = project_root.display().to_string();
    let lsp_servers = lsp_server_statuses();

    let Some(plan) = detect_typecheck_command(&project_root)? else {
        return Ok(ProjectTypecheckResponseDto {
            project_id,
            status: ProjectTypecheckStatusDto::Unavailable,
            source: "typescript".into(),
            command: Vec::new(),
            cwd,
            diagnostics: Vec::new(),
            started_at: started,
            completed_at: now_rfc3339(),
            duration_ms: timer.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            exit_code: None,
            truncated: false,
            message: Some("No tsconfig.json or package.json typecheck script was found.".into()),
            lsp_servers,
        });
    };

    cancellation.check_cancelled("project typecheck")?;
    let output = Command::new(&plan.argv[0])
        .args(&plan.argv[1..])
        .current_dir(&project_root)
        .env("NO_COLOR", "1")
        .env("CI", "1")
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CommandError::user_fixable(
                    "project_typecheck_command_not_found",
                    format!(
                        "Xero could not find `{}`. Install it or configure a project typecheck script.",
                        plan.argv[0]
                    ),
                )
            } else {
                CommandError::retryable(
                    "project_typecheck_spawn_failed",
                    format!("Xero could not start the project typecheck: {error}"),
                )
            }
        })?;

    cancellation.check_cancelled("project typecheck")?;
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    if !output.stderr.is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    let (diagnostics, output_truncated) = parse_tsc_diagnostics(&project_root, &combined);
    let exit_code = output.status.code();
    let status = if output.status.success() {
        ProjectTypecheckStatusDto::Passed
    } else {
        ProjectTypecheckStatusDto::Failed
    };
    let mut diagnostics = diagnostics;
    let mut truncated = output_truncated;

    if !output.status.success() && diagnostics.is_empty() {
        diagnostics.push(ProjectDiagnosticDto {
            path: None,
            line: None,
            column: None,
            severity: ProjectDiagnosticSeverityDto::Error,
            code: Some("typecheck_failed".into()),
            message: first_non_empty_line(&combined)
                .unwrap_or_else(|| "Typecheck failed without a parseable diagnostic.".into()),
            source: "typescript".into(),
        });
    }
    if diagnostics.len() > TYPECHECK_DIAGNOSTIC_LIMIT {
        diagnostics.truncate(TYPECHECK_DIAGNOSTIC_LIMIT);
        truncated = true;
    }

    Ok(ProjectTypecheckResponseDto {
        project_id,
        status,
        source: "typescript".into(),
        command: plan.argv,
        cwd,
        diagnostics,
        started_at: started,
        completed_at: now_rfc3339(),
        duration_ms: timer.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        exit_code,
        truncated,
        message: Some(plan.label),
        lsp_servers,
    })
}

#[derive(Debug, Clone)]
struct TypecheckPlan {
    label: String,
    argv: Vec<String>,
}

fn detect_typecheck_command(project_root: &Path) -> CommandResult<Option<TypecheckPlan>> {
    let package_json = project_root.join("package.json");
    if package_json.is_file() {
        let raw = fs::read_to_string(&package_json).map_err(|error| {
            CommandError::retryable(
                "project_typecheck_package_json_read_failed",
                format!("Xero could not read package.json for typecheck detection: {error}"),
            )
        })?;
        let value: JsonValue = serde_json::from_str(&raw).map_err(|error| {
            CommandError::user_fixable(
                "project_typecheck_package_json_invalid",
                format!("Xero could not parse package.json for typecheck detection: {error}"),
            )
        })?;
        if value
            .get("scripts")
            .and_then(|scripts| scripts.get("typecheck"))
            .and_then(JsonValue::as_str)
            .map(|script| !script.trim().is_empty())
            .unwrap_or(false)
        {
            if let Some(argv) = package_manager_typecheck_argv(project_root) {
                return Ok(Some(TypecheckPlan {
                    label: "package.json script `typecheck`".into(),
                    argv,
                }));
            }
        }
    }

    if !project_root.join("tsconfig.json").is_file() {
        return Ok(None);
    }

    if let Some(local_tsc) = local_bin(project_root, "tsc") {
        return Ok(Some(TypecheckPlan {
            label: "local TypeScript compiler".into(),
            argv: vec![
                local_tsc.display().to_string(),
                "--noEmit".into(),
                "--pretty".into(),
                "false".into(),
            ],
        }));
    }

    if find_executable_on_path("tsc").is_some() {
        return Ok(Some(TypecheckPlan {
            label: "TypeScript compiler on PATH".into(),
            argv: vec![
                "tsc".into(),
                "--noEmit".into(),
                "--pretty".into(),
                "false".into(),
            ],
        }));
    }

    Ok(None)
}

fn package_manager_typecheck_argv(project_root: &Path) -> Option<Vec<String>> {
    let candidates: &[(&str, &str, &[&str])] = &[
        ("pnpm-lock.yaml", "pnpm", &["run", "typecheck"]),
        ("yarn.lock", "yarn", &["typecheck"]),
        ("bun.lockb", "bun", &["run", "typecheck"]),
        ("package-lock.json", "npm", &["run", "typecheck"]),
    ];
    for (lockfile, command, args) in candidates {
        if project_root.join(lockfile).is_file() && find_executable_on_path(command).is_some() {
            let mut argv = vec![(*command).to_owned()];
            argv.extend(args.iter().map(|arg| (*arg).to_owned()));
            return Some(argv);
        }
    }

    find_executable_on_path("npm").map(|_| vec!["npm".into(), "run".into(), "typecheck".into()])
}

fn local_bin(project_root: &Path, command: &str) -> Option<PathBuf> {
    let path = project_root.join("node_modules").join(".bin").join(command);
    path.is_file().then_some(path)
}

fn parse_tsc_diagnostics(project_root: &Path, output: &str) -> (Vec<ProjectDiagnosticDto>, bool) {
    let truncated = output.len() > TYPECHECK_OUTPUT_LIMIT_BYTES;
    let capped = if truncated {
        let mut boundary = TYPECHECK_OUTPUT_LIMIT_BYTES;
        while boundary > 0 && !output.is_char_boundary(boundary) {
            boundary -= 1;
        }
        &output[..boundary]
    } else {
        output
    };
    let clean = strip_ansi(capped);
    let location_pattern = Regex::new(
        r"^(?P<path>.+?)\((?P<line>\d+),(?P<column>\d+)\): (?P<severity>error|warning) TS(?P<code>\d+): (?P<message>.*)$",
    )
    .expect("valid TypeScript diagnostic regex");
    let global_pattern =
        Regex::new(r"^(?P<severity>error|warning) TS(?P<code>\d+): (?P<message>.*)$")
            .expect("valid TypeScript global diagnostic regex");
    let mut diagnostics = Vec::new();

    for line in clean.lines() {
        if let Some(captures) = location_pattern.captures(line) {
            diagnostics.push(ProjectDiagnosticDto {
                path: normalize_diagnostic_path(project_root, &captures["path"]),
                line: captures["line"].parse::<u32>().ok(),
                column: captures["column"].parse::<u32>().ok(),
                severity: parse_severity(&captures["severity"]),
                code: Some(format!("TS{}", &captures["code"])),
                message: captures["message"].trim().to_owned(),
                source: "typescript".into(),
            });
            continue;
        }

        if let Some(captures) = global_pattern.captures(line) {
            diagnostics.push(ProjectDiagnosticDto {
                path: None,
                line: None,
                column: None,
                severity: parse_severity(&captures["severity"]),
                code: Some(format!("TS{}", &captures["code"])),
                message: captures["message"].trim().to_owned(),
                source: "typescript".into(),
            });
            continue;
        }

        if let Some(previous) = diagnostics.last_mut() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                previous.message.push('\n');
                previous.message.push_str(trimmed);
            }
        }
    }

    (diagnostics, truncated)
}

fn strip_ansi(value: &str) -> String {
    Regex::new(r"\x1b\[[0-9;]*m")
        .expect("valid ansi regex")
        .replace_all(value, "")
        .into_owned()
}

fn parse_severity(value: &str) -> ProjectDiagnosticSeverityDto {
    match value {
        "warning" => ProjectDiagnosticSeverityDto::Warning,
        _ => ProjectDiagnosticSeverityDto::Error,
    }
}

fn normalize_diagnostic_path(project_root: &Path, raw_path: &str) -> Option<String> {
    let path = PathBuf::from(raw_path.trim());
    let absolute = if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    };
    let relative = absolute.strip_prefix(project_root).ok()?;
    let path = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str().map(ToOwned::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    if path.is_empty() {
        None
    } else {
        Some(format!("/{path}"))
    }
}

fn first_non_empty_line(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn lsp_server_statuses() -> Vec<EditorLspServerStatusDto> {
    LSP_SERVER_DESCRIPTORS
        .iter()
        .map(|descriptor| {
            let available = find_executable_on_path(descriptor.command).is_some();
            EditorLspServerStatusDto {
                server_id: descriptor.server_id.into(),
                language: descriptor.language.into(),
                command: descriptor.command.into(),
                args: descriptor
                    .args
                    .iter()
                    .map(|arg| (*arg).to_owned())
                    .collect(),
                available,
                supports_diagnostics: true,
                supports_symbols: true,
                supports_hover: true,
                supports_completion: true,
                supports_definition: true,
                supports_references: true,
                supports_rename: true,
                supports_code_actions: true,
                install_suggestion: (!available).then(|| lsp_install_suggestion(descriptor)),
            }
        })
        .collect()
}

fn lsp_install_suggestion(descriptor: &LspServerDescriptor) -> EditorLspInstallSuggestionDto {
    EditorLspInstallSuggestionDto {
        reason: format!(
            "`{}` was not found on PATH. Xero will not install it automatically.",
            descriptor.command
        ),
        candidate_commands: lsp_install_commands(descriptor),
    }
}

fn lsp_install_commands(descriptor: &LspServerDescriptor) -> Vec<EditorLspInstallCommandDto> {
    match descriptor.server_id {
        "typescript_language_server" => vec![install_command(
            "npm global",
            &[
                "npm",
                "install",
                "-g",
                "typescript",
                "typescript-language-server",
            ],
        )],
        "rust_analyzer" => vec![
            install_command(
                "rustup component",
                &["rustup", "component", "add", "rust-analyzer"],
            ),
            install_command("Homebrew", &["brew", "install", "rust-analyzer"]),
        ],
        "pyright" => vec![install_command(
            "npm global",
            &["npm", "install", "-g", "pyright"],
        )],
        "gopls" => vec![install_command(
            "go install",
            &["go", "install", "golang.org/x/tools/gopls@latest"],
        )],
        "vscode_json_language_server" => vec![install_command(
            "npm global",
            &["npm", "install", "-g", "vscode-langservers-extracted"],
        )],
        "clangd" => vec![
            install_command("Homebrew", &["brew", "install", "llvm"]),
            install_command("winget", &["winget", "install", "LLVM.LLVM"]),
        ],
        _ => Vec::new(),
    }
}

fn install_command(label: &str, argv: &[&str]) -> EditorLspInstallCommandDto {
    EditorLspInstallCommandDto {
        label: label.into(),
        argv: argv.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

fn find_executable_on_path(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }

    let paths = std::env::var_os("PATH")?;
    for root in std::env::split_paths(&paths) {
        let candidate = root.join(command);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = root.join(format!("{command}.exe"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

#[cfg(test)]
mod tests {
    use super::{parse_tsc_diagnostics, ProjectDiagnosticSeverityDto};

    #[test]
    fn parses_tsc_file_and_global_diagnostics() {
        let root = tempfile::tempdir().expect("tempdir");
        let output = "src/app.ts(3,7): error TS2322: Type 'string' is not assignable to type 'number'.\nerror TS18003: No inputs were found in config file.\n";

        let (diagnostics, truncated) = parse_tsc_diagnostics(root.path(), output);

        assert!(!truncated);
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].path.as_deref(), Some("/src/app.ts"));
        assert_eq!(diagnostics[0].line, Some(3));
        assert_eq!(diagnostics[0].column, Some(7));
        assert_eq!(diagnostics[0].severity, ProjectDiagnosticSeverityDto::Error);
        assert_eq!(diagnostics[0].code.as_deref(), Some("TS2322"));
        assert_eq!(diagnostics[1].path, None);
        assert_eq!(diagnostics[1].code.as_deref(), Some("TS18003"));
    }
}
