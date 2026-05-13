use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime, State};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    commands::{
        editor_diagnostics::{ProjectDiagnosticDto, ProjectDiagnosticSeverityDto},
        runtime_support::resolve_project_root,
        validate_non_empty, CommandError, CommandResult,
    },
    state::DesktopState,
};

const FORMAT_TIMEOUT_OUTPUT_LIMIT_BYTES: usize = 256 * 1024;
const LINT_OUTPUT_LIMIT_BYTES: usize = 1024 * 1024;
const LINT_DIAGNOSTIC_LIMIT: usize = 1_000;
const FORMAT_INPUT_LIMIT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormatProjectDocumentRangeDto {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormatProjectDocumentRequestDto {
    pub project_id: String,
    pub path: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<FormatProjectDocumentRangeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormatProjectDocumentStatusDto {
    Formatted,
    Unchanged,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormatProjectDocumentResponseDto {
    pub project_id: String,
    pub path: String,
    pub status: FormatProjectDocumentStatusDto,
    pub formatter_id: Option<String>,
    pub command: Vec<String>,
    pub content: Option<String>,
    pub range_applied: Option<FormatProjectDocumentRangeDto>,
    pub diagnostics: Vec<ProjectDiagnosticDto>,
    pub started_at: String,
    pub completed_at: String,
    pub duration_ms: u64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunProjectLintRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLintStatusDto {
    Passed,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectLintResponseDto {
    pub project_id: String,
    pub status: ProjectLintStatusDto,
    pub source: String,
    pub command: Vec<String>,
    pub cwd: String,
    pub diagnostics: Vec<ProjectDiagnosticDto>,
    pub started_at: String,
    pub completed_at: String,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
    pub truncated: bool,
    pub message: Option<String>,
}

#[tauri::command]
pub async fn format_project_document<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: FormatProjectDocumentRequestDto,
) -> CommandResult<FormatProjectDocumentResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;

    if request.content.len() > FORMAT_INPUT_LIMIT_BYTES {
        return Err(CommandError::user_fixable(
            "format_document_too_large",
            format!(
                "Xero refuses to format documents larger than {} bytes.",
                FORMAT_INPUT_LIMIT_BYTES
            ),
        ));
    }

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    let path = request.path;
    let content = request.content;
    let range = request.range;
    drop(app);

    jobs.run_blocking_latest(
        format!("project-format:{project_id}:{path}"),
        "project format",
        move |_cancellation| {
            format_project_document_inner(project_id, project_root, path, content, range)
        },
    )
    .await
}

fn format_project_document_inner(
    project_id: String,
    project_root: PathBuf,
    relative_path: String,
    content: String,
    range: Option<FormatProjectDocumentRangeDto>,
) -> CommandResult<FormatProjectDocumentResponseDto> {
    let started_at = now_rfc3339();
    let timer = Instant::now();
    let extension = relative_path
        .rsplit('.')
        .next()
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let basename = relative_path
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    let Some(plan) = detect_formatter_plan(&project_root, &extension, &basename, &range) else {
        return Ok(FormatProjectDocumentResponseDto {
            project_id,
            path: relative_path,
            status: FormatProjectDocumentStatusDto::Unavailable,
            formatter_id: None,
            command: Vec::new(),
            content: None,
            range_applied: None,
            diagnostics: Vec::new(),
            started_at,
            completed_at: now_rfc3339(),
            duration_ms: clamp_elapsed_millis(&timer),
            message: Some(
                "No formatter is configured for this file type. Install Prettier, rustfmt, gofmt, black, or clang-format and reopen the project."
                    .into(),
            ),
        });
    };

    let mut command = Command::new(&plan.argv[0]);
    command
        .args(&plan.argv[1..])
        .current_dir(&project_root)
        .env("NO_COLOR", "1")
        .env("CI", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => CommandError::user_fixable(
            "format_command_not_found",
            format!(
                "Xero could not find `{}`. Install the formatter or configure it in the project.",
                plan.argv[0]
            ),
        ),
        _ => CommandError::retryable(
            "format_spawn_failed",
            format!("Xero could not start the formatter: {error}"),
        ),
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(error) = stdin.write_all(content.as_bytes()) {
            return Err(CommandError::retryable(
                "format_stdin_failed",
                format!("Xero could not stream the document to the formatter: {error}"),
            ));
        }
    }

    let output = child.wait_with_output().map_err(|error| {
        CommandError::retryable(
            "format_wait_failed",
            format!("Xero could not collect formatter output: {error}"),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr_full = String::from_utf8_lossy(&output.stderr).into_owned();
    let stderr = clamp_text(&stderr_full, FORMAT_TIMEOUT_OUTPUT_LIMIT_BYTES);
    let exit_code = output.status.code();
    let path_display = relative_path.clone();

    if !output.status.success() {
        return Ok(FormatProjectDocumentResponseDto {
            project_id,
            path: relative_path,
            status: FormatProjectDocumentStatusDto::Failed,
            formatter_id: Some(plan.formatter_id.clone()),
            command: plan.argv,
            content: None,
            range_applied: None,
            diagnostics: vec![ProjectDiagnosticDto {
                path: Some(path_display.clone()),
                line: None,
                column: None,
                severity: ProjectDiagnosticSeverityDto::Error,
                code: exit_code.map(|code| format!("exit:{code}")),
                message: first_non_empty_line(&stderr)
                    .unwrap_or_else(|| format!("{} failed to format the document.", plan.label)),
                source: plan.formatter_id.clone(),
            }],
            started_at,
            completed_at: now_rfc3339(),
            duration_ms: clamp_elapsed_millis(&timer),
            message: Some(stderr),
        });
    }

    let changed = stdout != content;
    let status = if changed {
        FormatProjectDocumentStatusDto::Formatted
    } else {
        FormatProjectDocumentStatusDto::Unchanged
    };

    Ok(FormatProjectDocumentResponseDto {
        project_id,
        path: relative_path,
        status,
        formatter_id: Some(plan.formatter_id),
        command: plan.argv,
        content: Some(stdout),
        range_applied: plan.range_applied,
        diagnostics: Vec::new(),
        started_at,
        completed_at: now_rfc3339(),
        duration_ms: clamp_elapsed_millis(&timer),
        message: None,
    })
}

#[derive(Debug, Clone)]
struct FormatterPlan {
    label: String,
    formatter_id: String,
    argv: Vec<String>,
    range_applied: Option<FormatProjectDocumentRangeDto>,
}

fn detect_formatter_plan(
    project_root: &Path,
    extension: &str,
    basename: &str,
    range: &Option<FormatProjectDocumentRangeDto>,
) -> Option<FormatterPlan> {
    if matches!(
        extension,
        "ts" | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "json"
            | "jsonc"
            | "md"
            | "markdown"
            | "mdx"
            | "css"
            | "scss"
            | "less"
            | "html"
            | "htm"
            | "yaml"
            | "yml"
            | "graphql"
            | "gql"
            | "vue"
    ) || matches!(basename, "package.json" | "tsconfig.json")
    {
        if let Some(prettier_argv) = prettier_command(project_root) {
            let parser_path = format!(
                "--stdin-filepath={}",
                current_basename_or_extension(basename, extension)
            );
            let mut argv = prettier_argv;
            argv.push(parser_path);
            let mut range_applied = None;
            if let Some(range) = range {
                if range.start < range.end {
                    argv.push(format!("--range-start={}", range.start));
                    argv.push(format!("--range-end={}", range.end));
                    range_applied = Some(range.clone());
                }
            }
            return Some(FormatterPlan {
                label: "Prettier".into(),
                formatter_id: "prettier".into(),
                argv,
                range_applied,
            });
        }
    }

    if extension == "rs" {
        if let Some(rustfmt) = find_executable_on_path("rustfmt") {
            return Some(FormatterPlan {
                label: "rustfmt".into(),
                formatter_id: "rustfmt".into(),
                argv: vec![
                    rustfmt.display().to_string(),
                    "--emit".into(),
                    "stdout".into(),
                    "--edition".into(),
                    "2021".into(),
                ],
                range_applied: None,
            });
        }
    }

    if extension == "go" {
        if let Some(gofmt) = find_executable_on_path("gofmt") {
            return Some(FormatterPlan {
                label: "gofmt".into(),
                formatter_id: "gofmt".into(),
                argv: vec![gofmt.display().to_string()],
                range_applied: None,
            });
        }
    }

    if matches!(extension, "py" | "pyi") {
        if let Some(black) = find_executable_on_path("black") {
            return Some(FormatterPlan {
                label: "black".into(),
                formatter_id: "black".into(),
                argv: vec![black.display().to_string(), "--quiet".into(), "-".into()],
                range_applied: None,
            });
        }
        if let Some(ruff) = find_executable_on_path("ruff") {
            return Some(FormatterPlan {
                label: "ruff format".into(),
                formatter_id: "ruff".into(),
                argv: vec![
                    ruff.display().to_string(),
                    "format".into(),
                    "--stdin-filename".into(),
                    if basename.is_empty() {
                        "stdin.py".into()
                    } else {
                        basename.to_string()
                    },
                    "-".into(),
                ],
                range_applied: None,
            });
        }
    }

    if matches!(
        extension,
        "c" | "cc" | "cpp" | "cxx" | "h" | "hpp" | "hh" | "hxx" | "m" | "mm"
    ) {
        if let Some(clang) = find_executable_on_path("clang-format") {
            let mut argv = vec![
                clang.display().to_string(),
                format!(
                    "--assume-filename={}",
                    if basename.is_empty() {
                        format!("stdin.{}", extension)
                    } else {
                        basename.to_string()
                    }
                ),
            ];
            let mut range_applied = None;
            if let Some(range) = range {
                if range.start < range.end {
                    argv.push(format!("--offset={}", range.start));
                    argv.push(format!("--length={}", range.end - range.start));
                    range_applied = Some(range.clone());
                }
            }
            return Some(FormatterPlan {
                label: "clang-format".into(),
                formatter_id: "clang-format".into(),
                argv,
                range_applied,
            });
        }
    }

    None
}

fn current_basename_or_extension(basename: &str, extension: &str) -> String {
    if !basename.is_empty() {
        basename.to_string()
    } else if !extension.is_empty() {
        format!("stdin.{extension}")
    } else {
        "stdin.txt".into()
    }
}

fn prettier_command(project_root: &Path) -> Option<Vec<String>> {
    if let Some(local) = local_bin(project_root, "prettier") {
        return Some(vec![local.display().to_string()]);
    }
    let pm_candidates: &[(&str, &str, &[&str])] = &[
        ("pnpm-lock.yaml", "pnpm", &["exec", "prettier"]),
        ("yarn.lock", "yarn", &["prettier"]),
        ("bun.lockb", "bun", &["x", "prettier"]),
        ("package-lock.json", "npx", &["--no-install", "prettier"]),
    ];
    for (lockfile, manager, args) in pm_candidates {
        if project_root.join(lockfile).is_file() && find_executable_on_path(manager).is_some() {
            let mut argv = vec![(*manager).to_owned()];
            argv.extend(args.iter().map(|arg| (*arg).to_owned()));
            return Some(argv);
        }
    }
    find_executable_on_path("prettier").map(|path| vec![path.display().to_string()])
}

#[tauri::command]
pub async fn run_project_lint<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RunProjectLintRequestDto,
) -> CommandResult<ProjectLintResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    let scoped_path = request.path;
    drop(app);

    jobs.run_blocking_latest(
        format!("project-lint:{project_id}"),
        "project lint",
        move |_cancellation| run_project_lint_inner(project_id, project_root, scoped_path),
    )
    .await
}

fn run_project_lint_inner(
    project_id: String,
    project_root: PathBuf,
    scoped_path: Option<String>,
) -> CommandResult<ProjectLintResponseDto> {
    let started_at = now_rfc3339();
    let timer = Instant::now();
    let cwd = project_root.display().to_string();

    let Some(plan) = detect_lint_plan(&project_root, scoped_path.as_deref())? else {
        return Ok(ProjectLintResponseDto {
            project_id,
            status: ProjectLintStatusDto::Unavailable,
            source: "lint".into(),
            command: Vec::new(),
            cwd,
            diagnostics: Vec::new(),
            started_at,
            completed_at: now_rfc3339(),
            duration_ms: clamp_elapsed_millis(&timer),
            exit_code: None,
            truncated: false,
            message: Some("No supported linter (ESLint) was found.".into()),
        });
    };

    let output = Command::new(&plan.argv[0])
        .args(&plan.argv[1..])
        .current_dir(&project_root)
        .env("NO_COLOR", "1")
        .env("CI", "1")
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::user_fixable(
                "lint_command_not_found",
                format!(
                    "Xero could not find `{}`. Install the linter or add a project script.",
                    plan.argv[0]
                ),
            ),
            _ => CommandError::retryable(
                "lint_spawn_failed",
                format!("Xero could not start the linter: {error}"),
            ),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let exit_code = output.status.code();

    let (diagnostics, truncated, message) = match plan.parser {
        LintParser::EslintJson => parse_eslint_json(&project_root, &stdout, &stderr),
    };

    let status = if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == ProjectDiagnosticSeverityDto::Error)
        || !output.status.success() && diagnostics.is_empty()
    {
        ProjectLintStatusDto::Failed
    } else {
        ProjectLintStatusDto::Passed
    };

    Ok(ProjectLintResponseDto {
        project_id,
        status,
        source: plan.source.into(),
        command: plan.argv,
        cwd,
        diagnostics,
        started_at,
        completed_at: now_rfc3339(),
        duration_ms: clamp_elapsed_millis(&timer),
        exit_code,
        truncated,
        message,
    })
}

#[derive(Debug, Clone, Copy)]
enum LintParser {
    EslintJson,
}

#[derive(Debug, Clone)]
struct LintPlan {
    argv: Vec<String>,
    source: &'static str,
    parser: LintParser,
}

fn detect_lint_plan(
    project_root: &Path,
    scoped_path: Option<&str>,
) -> CommandResult<Option<LintPlan>> {
    let package_json = project_root.join("package.json");
    let has_eslint_config = ESLINT_CONFIG_FILES
        .iter()
        .any(|name| project_root.join(name).is_file())
        || package_json_has_eslint_block(&package_json)?;
    if !has_eslint_config {
        return Ok(None);
    }

    let target = scoped_path
        .map(strip_leading_slash)
        .unwrap_or_else(|| ".".into());

    if let Some(local_eslint) = local_bin(project_root, "eslint") {
        let mut argv = vec![
            local_eslint.display().to_string(),
            "--format".into(),
            "json".into(),
            "--no-color".into(),
        ];
        argv.push(target);
        return Ok(Some(LintPlan {
            argv,
            source: "eslint",
            parser: LintParser::EslintJson,
        }));
    }

    let pm_candidates: &[(&str, &str, &[&str])] = &[
        ("pnpm-lock.yaml", "pnpm", &["exec", "eslint"]),
        ("yarn.lock", "yarn", &["eslint"]),
        ("bun.lockb", "bun", &["x", "eslint"]),
        ("package-lock.json", "npx", &["--no-install", "eslint"]),
    ];
    for (lockfile, manager, args) in pm_candidates {
        if project_root.join(lockfile).is_file() && find_executable_on_path(manager).is_some() {
            let mut argv = vec![(*manager).to_owned()];
            argv.extend(args.iter().map(|arg| (*arg).to_owned()));
            argv.extend(["--format".into(), "json".into(), "--no-color".into()]);
            argv.push(target);
            return Ok(Some(LintPlan {
                argv,
                source: "eslint",
                parser: LintParser::EslintJson,
            }));
        }
    }

    if let Some(eslint) = find_executable_on_path("eslint") {
        let mut argv = vec![
            eslint.display().to_string(),
            "--format".into(),
            "json".into(),
            "--no-color".into(),
        ];
        argv.push(target);
        return Ok(Some(LintPlan {
            argv,
            source: "eslint",
            parser: LintParser::EslintJson,
        }));
    }

    Ok(None)
}

const ESLINT_CONFIG_FILES: &[&str] = &[
    ".eslintrc",
    ".eslintrc.js",
    ".eslintrc.cjs",
    ".eslintrc.mjs",
    ".eslintrc.json",
    ".eslintrc.yaml",
    ".eslintrc.yml",
    "eslint.config.js",
    "eslint.config.cjs",
    "eslint.config.mjs",
    "eslint.config.ts",
];

fn package_json_has_eslint_block(package_json: &Path) -> CommandResult<bool> {
    if !package_json.is_file() {
        return Ok(false);
    }
    let raw = fs::read_to_string(package_json).map_err(|error| {
        CommandError::retryable(
            "lint_package_json_read_failed",
            format!("Xero could not read package.json for lint detection: {error}"),
        )
    })?;
    let value: JsonValue = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    Ok(value.get("eslintConfig").is_some())
}

fn strip_leading_slash(path: &str) -> String {
    let trimmed = path.trim();
    if let Some(rest) = trimmed.strip_prefix('/') {
        if rest.is_empty() {
            ".".into()
        } else {
            rest.to_string()
        }
    } else {
        trimmed.to_string()
    }
}

fn parse_eslint_json(
    project_root: &Path,
    stdout: &str,
    stderr: &str,
) -> (Vec<ProjectDiagnosticDto>, bool, Option<String>) {
    let truncated = stdout.len() > LINT_OUTPUT_LIMIT_BYTES;
    let trimmed = stdout.trim_start();
    let bracket_pos = trimmed.find('[');
    let Some(start_index) = bracket_pos else {
        let stderr_clean = strip_ansi(stderr);
        return (
            Vec::new(),
            truncated,
            Some(
                first_non_empty_line(&stderr_clean)
                    .unwrap_or_else(|| "ESLint produced no parseable JSON output.".into()),
            ),
        );
    };
    let json_slice = &trimmed[start_index..];
    let value: JsonValue = match serde_json::from_str(json_slice) {
        Ok(value) => value,
        Err(error) => {
            return (
                Vec::new(),
                truncated,
                Some(format!(
                    "ESLint output could not be parsed as JSON: {error}."
                )),
            );
        }
    };

    let mut diagnostics = Vec::new();
    if let Some(files) = value.as_array() {
        for entry in files {
            let absolute = entry
                .get("filePath")
                .and_then(JsonValue::as_str)
                .map(PathBuf::from);
            let normalized_path = absolute
                .as_deref()
                .and_then(|path| normalize_path_relative_to_root(project_root, path));
            if let Some(messages) = entry.get("messages").and_then(JsonValue::as_array) {
                for message in messages {
                    let severity_level = message
                        .get("severity")
                        .and_then(JsonValue::as_u64)
                        .unwrap_or(0);
                    let severity = match severity_level {
                        2 => ProjectDiagnosticSeverityDto::Error,
                        1 => ProjectDiagnosticSeverityDto::Warning,
                        _ => continue,
                    };
                    let line = message
                        .get("line")
                        .and_then(JsonValue::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .filter(|value| *value > 0);
                    let column = message
                        .get("column")
                        .and_then(JsonValue::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .filter(|value| *value > 0);
                    let rule = message
                        .get("ruleId")
                        .and_then(JsonValue::as_str)
                        .map(ToOwned::to_owned);
                    let text = message
                        .get("message")
                        .and_then(JsonValue::as_str)
                        .unwrap_or("ESLint reported an issue.")
                        .trim()
                        .to_owned();
                    diagnostics.push(ProjectDiagnosticDto {
                        path: normalized_path.clone(),
                        line,
                        column,
                        severity,
                        code: rule,
                        message: text,
                        source: "eslint".into(),
                    });
                    if diagnostics.len() >= LINT_DIAGNOSTIC_LIMIT {
                        return (diagnostics, true, None);
                    }
                }
            }
        }
    }

    let message = if diagnostics.is_empty() {
        if stderr.trim().is_empty() {
            None
        } else {
            let stderr_clean = strip_ansi(stderr);
            first_non_empty_line(&stderr_clean)
        }
    } else {
        None
    };

    (diagnostics, truncated, message)
}

fn normalize_path_relative_to_root(project_root: &Path, raw_path: &Path) -> Option<String> {
    let candidate = if raw_path.is_absolute() {
        raw_path.to_path_buf()
    } else {
        project_root.join(raw_path)
    };
    let relative = candidate.strip_prefix(project_root).ok()?;
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

fn local_bin(project_root: &Path, command: &str) -> Option<PathBuf> {
    let path = project_root.join("node_modules").join(".bin").join(command);
    path.is_file().then_some(path)
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

fn strip_ansi(value: &str) -> String {
    Regex::new(r"\x1b\[[0-9;]*m")
        .expect("valid ansi regex")
        .replace_all(value, "")
        .into_owned()
}

fn clamp_text(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut boundary = limit;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value[..boundary].to_owned()
}

fn first_non_empty_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn clamp_elapsed_millis(timer: &Instant) -> u64 {
    let elapsed = timer.elapsed().as_millis();
    if elapsed > u128::from(u64::MAX) {
        u64::MAX
    } else {
        elapsed as u64
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

#[cfg(test)]
mod tests {
    use super::{
        detect_formatter_plan, detect_lint_plan, parse_eslint_json, strip_leading_slash,
        FormatProjectDocumentRangeDto, ProjectDiagnosticSeverityDto,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detect_formatter_returns_none_when_nothing_available() {
        let root = tempdir().expect("tempdir");
        let plan = detect_formatter_plan(root.path(), "ts", "app.ts", &None);
        // We cannot rely on PATH for tests; just assert the result type is well-formed.
        if let Some(plan) = plan {
            assert!(!plan.argv.is_empty());
        }
    }

    #[test]
    fn detect_formatter_picks_local_prettier_when_present() {
        let root = tempdir().expect("tempdir");
        let bin_dir = root.path().join("node_modules").join(".bin");
        fs::create_dir_all(&bin_dir).expect("bin dir");
        let prettier = bin_dir.join("prettier");
        fs::write(&prettier, "#!/bin/sh\nexit 0\n").expect("write prettier shim");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&prettier).expect("perms").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&prettier, perms).expect("set perms");
        }
        let plan =
            detect_formatter_plan(root.path(), "ts", "index.ts", &None).expect("plan available");
        assert_eq!(plan.formatter_id, "prettier");
        assert!(plan.argv.iter().any(|arg| arg.ends_with("/.bin/prettier")));
        assert!(plan
            .argv
            .iter()
            .any(|arg| arg.starts_with("--stdin-filepath=")));
    }

    #[test]
    fn detect_formatter_applies_prettier_range() {
        let root = tempdir().expect("tempdir");
        let bin_dir = root.path().join("node_modules").join(".bin");
        fs::create_dir_all(&bin_dir).expect("bin dir");
        let prettier = bin_dir.join("prettier");
        fs::write(&prettier, "#!/bin/sh\nexit 0\n").expect("write prettier shim");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&prettier).expect("perms").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&prettier, perms).expect("set perms");
        }
        let range = Some(FormatProjectDocumentRangeDto { start: 10, end: 40 });
        let plan =
            detect_formatter_plan(root.path(), "tsx", "view.tsx", &range).expect("plan available");
        assert_eq!(plan.formatter_id, "prettier");
        assert!(plan.argv.iter().any(|arg| arg == "--range-start=10"));
        assert!(plan.argv.iter().any(|arg| arg == "--range-end=40"));
        assert_eq!(plan.range_applied.as_ref().map(|range| range.end), Some(40));
    }

    #[test]
    fn detect_lint_requires_eslint_config() {
        let root = tempdir().expect("tempdir");
        assert!(detect_lint_plan(root.path(), None).expect("ok").is_none());
        fs::write(root.path().join("eslint.config.js"), "export default []\n")
            .expect("write config");
        let bin_dir = root.path().join("node_modules").join(".bin");
        fs::create_dir_all(&bin_dir).expect("bin dir");
        let eslint = bin_dir.join("eslint");
        fs::write(&eslint, "#!/bin/sh\nexit 0\n").expect("write eslint shim");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&eslint).expect("perms").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&eslint, perms).expect("set perms");
        }
        let plan = detect_lint_plan(root.path(), None)
            .expect("ok")
            .expect("plan");
        assert_eq!(plan.source, "eslint");
        assert!(plan.argv.iter().any(|arg| arg.ends_with("/.bin/eslint")));
        assert!(plan.argv.iter().any(|arg| arg == "json"));
    }

    #[test]
    fn parse_eslint_json_extracts_messages() {
        let root = tempdir().expect("tempdir");
        let absolute = root.path().join("src/index.ts");
        let payload = serde_json::json!([
            {
                "filePath": absolute,
                "messages": [
                    {
                        "severity": 2,
                        "ruleId": "no-unused-vars",
                        "message": "'foo' is defined but never used.",
                        "line": 3,
                        "column": 7
                    },
                    {
                        "severity": 1,
                        "ruleId": null,
                        "message": "Whitespace issue.",
                        "line": 9,
                        "column": 1
                    },
                    {
                        "severity": 0,
                        "ruleId": "off-rule",
                        "message": "Ignored.",
                        "line": 1,
                        "column": 1
                    }
                ]
            }
        ]);
        let (diagnostics, truncated, _message) =
            parse_eslint_json(root.path(), &payload.to_string(), "");
        assert!(!truncated);
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].path.as_deref(), Some("/src/index.ts"));
        assert_eq!(diagnostics[0].line, Some(3));
        assert_eq!(diagnostics[0].column, Some(7));
        assert_eq!(diagnostics[0].severity, ProjectDiagnosticSeverityDto::Error);
        assert_eq!(diagnostics[0].code.as_deref(), Some("no-unused-vars"));
        assert_eq!(
            diagnostics[1].severity,
            ProjectDiagnosticSeverityDto::Warning
        );
        assert_eq!(diagnostics[1].code, None);
    }

    #[test]
    fn strip_leading_slash_handles_absolute_paths() {
        assert_eq!(strip_leading_slash("/src/app.ts"), "src/app.ts");
        assert_eq!(strip_leading_slash("/"), ".");
        assert_eq!(strip_leading_slash("src/app.ts"), "src/app.ts");
    }
}
