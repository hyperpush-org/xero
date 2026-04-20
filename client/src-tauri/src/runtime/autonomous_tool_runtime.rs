use std::{
    ffi::OsString,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use globset::{GlobBuilder, GlobMatcher};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};

use super::autonomous_web_runtime::{
    AutonomousWebConfig, AutonomousWebFetchContentKind, AutonomousWebFetchOutput,
    AutonomousWebFetchRequest, AutonomousWebRuntime, AutonomousWebSearchOutput,
    AutonomousWebSearchRequest, AUTONOMOUS_TOOL_WEB_FETCH, AUTONOMOUS_TOOL_WEB_SEARCH,
};

use crate::{
    commands::{
        validate_non_empty, BranchSummaryDto, CommandError, CommandErrorClass, CommandResult,
        RepositoryDiffScope, RepositoryStatusEntryDto,
    },
    db::project_store,
    git::{diff, status},
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};

pub const AUTONOMOUS_TOOL_READ: &str = "read";
pub const AUTONOMOUS_TOOL_SEARCH: &str = "search";
pub const AUTONOMOUS_TOOL_FIND: &str = "find";
pub const AUTONOMOUS_TOOL_GIT_STATUS: &str = "git_status";
pub const AUTONOMOUS_TOOL_GIT_DIFF: &str = "git_diff";
pub const AUTONOMOUS_TOOL_EDIT: &str = "edit";
pub const AUTONOMOUS_TOOL_WRITE: &str = "write";
pub const AUTONOMOUS_TOOL_COMMAND: &str = "command";

const SKIPPED_DIRECTORIES: &[&str] = &[
    ".git",
    ".cadence",
    "node_modules",
    "target",
    ".next",
    "dist",
    "build",
    "coverage",
    ".turbo",
    ".yarn",
    ".pnpm-store",
];
const DEFAULT_READ_LINE_COUNT: usize = 200;
const MAX_READ_LINE_COUNT: usize = 400;
const MAX_TEXT_FILE_BYTES: usize = 512 * 1024;
const MAX_SEARCH_QUERY_CHARS: usize = 256;
const MAX_SEARCH_RESULTS: usize = 100;
const MAX_SEARCH_PREVIEW_CHARS: usize = 200;
const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 5_000;
const MAX_COMMAND_TIMEOUT_MS: u64 = 60_000;
const MAX_COMMAND_CAPTURE_BYTES: usize = 8 * 1024;
const MAX_COMMAND_EXCERPT_CHARS: usize = 2_000;
const REDACTED_COMMAND_OUTPUT_SUMMARY: &str =
    "Command output was redacted before durable persistence.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomousToolRuntimeLimits {
    pub default_read_line_count: usize,
    pub max_read_line_count: usize,
    pub max_text_file_bytes: usize,
    pub max_search_query_chars: usize,
    pub max_search_results: usize,
    pub max_search_preview_chars: usize,
    pub default_command_timeout_ms: u64,
    pub max_command_timeout_ms: u64,
    pub max_command_capture_bytes: usize,
    pub max_command_excerpt_chars: usize,
}

impl Default for AutonomousToolRuntimeLimits {
    fn default() -> Self {
        Self {
            default_read_line_count: DEFAULT_READ_LINE_COUNT,
            max_read_line_count: MAX_READ_LINE_COUNT,
            max_text_file_bytes: MAX_TEXT_FILE_BYTES,
            max_search_query_chars: MAX_SEARCH_QUERY_CHARS,
            max_search_results: MAX_SEARCH_RESULTS,
            max_search_preview_chars: MAX_SEARCH_PREVIEW_CHARS,
            default_command_timeout_ms: DEFAULT_COMMAND_TIMEOUT_MS,
            max_command_timeout_ms: MAX_COMMAND_TIMEOUT_MS,
            max_command_capture_bytes: MAX_COMMAND_CAPTURE_BYTES,
            max_command_excerpt_chars: MAX_COMMAND_EXCERPT_CHARS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AutonomousToolRuntime {
    repo_root: PathBuf,
    limits: AutonomousToolRuntimeLimits,
    web_runtime: AutonomousWebRuntime,
}

impl AutonomousToolRuntime {
    pub fn new(repo_root: impl AsRef<Path>) -> CommandResult<Self> {
        Self::with_limits_and_web_config(
            repo_root,
            AutonomousToolRuntimeLimits::default(),
            AutonomousWebConfig::for_platform(),
        )
    }

    pub fn with_limits(
        repo_root: impl AsRef<Path>,
        limits: AutonomousToolRuntimeLimits,
    ) -> CommandResult<Self> {
        Self::with_limits_and_web_config(repo_root, limits, AutonomousWebConfig::for_platform())
    }

    pub fn with_limits_and_web_config(
        repo_root: impl AsRef<Path>,
        limits: AutonomousToolRuntimeLimits,
        web_config: AutonomousWebConfig,
    ) -> CommandResult<Self> {
        let repo_root = repo_root.as_ref();
        let canonical_root = fs::canonicalize(repo_root).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::project_not_found(),
            _ => CommandError::system_fault(
                "autonomous_tool_repo_root_unavailable",
                format!(
                    "Cadence could not access the imported repository root at {}: {error}",
                    repo_root.display()
                ),
            ),
        })?;

        if !canonical_root.is_dir() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_repo_root_invalid",
                format!(
                    "Imported repository root {} is not a directory.",
                    canonical_root.display()
                ),
            ));
        }

        Ok(Self {
            repo_root: canonical_root,
            limits,
            web_runtime: AutonomousWebRuntime::new(web_config),
        })
    }

    pub fn for_project<R: Runtime>(
        app: &AppHandle<R>,
        state: &DesktopState,
        project_id: &str,
    ) -> CommandResult<Self> {
        let repo_root = resolve_imported_repo_root(app, state, project_id)?;
        Self::with_limits_and_web_config(
            repo_root,
            AutonomousToolRuntimeLimits::default(),
            state.autonomous_web_config(),
        )
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn limits(&self) -> AutonomousToolRuntimeLimits {
        self.limits
    }

    pub fn execute(&self, request: AutonomousToolRequest) -> CommandResult<AutonomousToolResult> {
        match request {
            AutonomousToolRequest::Read(request) => self.read(request),
            AutonomousToolRequest::Search(request) => self.search(request),
            AutonomousToolRequest::Find(request) => self.find(request),
            AutonomousToolRequest::GitStatus(request) => self.git_status(request),
            AutonomousToolRequest::GitDiff(request) => self.git_diff(request),
            AutonomousToolRequest::WebSearch(request) => self.web_search(request),
            AutonomousToolRequest::WebFetch(request) => self.web_fetch(request),
            AutonomousToolRequest::Edit(request) => self.edit(request),
            AutonomousToolRequest::Write(request) => self.write(request),
            AutonomousToolRequest::Command(request) => self.command(request),
        }
    }

    pub fn read(&self, request: AutonomousReadRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_existing_path(&relative_path)?;
        let text = self.read_text_file(&resolved_path)?;
        let line_count = request
            .line_count
            .unwrap_or(self.limits.default_read_line_count);
        if line_count == 0 || line_count > self.limits.max_read_line_count {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_line_count_invalid",
                format!(
                    "Cadence requires read line_count to be between 1 and {}.",
                    self.limits.max_read_line_count
                ),
            ));
        }

        let total_lines = count_lines(&text);
        let start_line = request.start_line.unwrap_or(1);
        if total_lines == 0 {
            if start_line != 1 {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_read_range_invalid",
                    "Cadence cannot start reading past the end of an empty file.",
                ));
            }
        } else if start_line == 0 || start_line > total_lines {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_range_invalid",
                format!(
                    "Cadence requires read start_line to stay within the file's 1..={total_lines} line range."
                ),
            ));
        }

        let (content, actual_line_count, truncated) = slice_lines(&text, start_line, line_count)?;
        let display_path = path_to_forward_slash(&relative_path);
        let summary = if truncated {
            format!(
                "Read {actual_line_count} line(s) from `{display_path}` starting at line {start_line} (truncated from {total_lines} total lines)."
            )
        } else {
            format!(
                "Read {actual_line_count} line(s) from `{display_path}` starting at line {start_line}."
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_READ.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Read(AutonomousReadOutput {
                path: display_path,
                start_line,
                line_count: actual_line_count,
                total_lines,
                truncated,
                content,
            }),
        })
    }

    pub fn search(&self, request: AutonomousSearchRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.query, "query")?;
        if request.query.chars().count() > self.limits.max_search_query_chars {
            return Err(CommandError::user_fixable(
                "autonomous_tool_search_query_too_large",
                format!(
                    "Cadence requires search queries to be {} characters or fewer.",
                    self.limits.max_search_query_chars
                ),
            ));
        }

        let scope = request
            .path
            .as_deref()
            .map(|path| {
                validate_non_empty(path, "path")?;
                normalize_relative_path(path, "path")
            })
            .transpose()?;

        let scope_path = match scope.as_ref() {
            Some(scope) => self.resolve_existing_path(scope)?,
            None => self.repo_root.clone(),
        };

        let mut matches = Vec::new();
        let mut walk = WalkState::default();
        self.search_scope(&scope_path, request.query.as_str(), &mut matches, &mut walk)?;

        let scope_string = scope
            .as_ref()
            .map(|path| path_to_forward_slash(path.as_path()));
        let matched_files = matches
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let summary = if matches.is_empty() {
            match scope_string.as_deref() {
                Some(scope) => format!("Found 0 matches for `{}` under `{scope}`.", request.query),
                None => format!("Found 0 matches for `{}` in the repository.", request.query),
            }
        } else if walk.truncated {
            format!(
                "Found {} match(es) for `{}` across {} file(s); results truncated at {} match(es).",
                matches.len(),
                request.query,
                matched_files,
                self.limits.max_search_results
            )
        } else {
            format!(
                "Found {} match(es) for `{}` across {} file(s).",
                matches.len(),
                request.query,
                matched_files
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_SEARCH.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Search(AutonomousSearchOutput {
                query: request.query,
                scope: scope_string,
                matches,
                scanned_files: walk.scanned_files,
                truncated: walk.truncated,
            }),
        })
    }

    pub fn find(&self, request: AutonomousFindRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.pattern, "pattern")?;
        if request.pattern.chars().count() > self.limits.max_search_query_chars {
            return Err(CommandError::user_fixable(
                "autonomous_tool_find_pattern_too_large",
                format!(
                    "Cadence requires find patterns to be {} characters or fewer.",
                    self.limits.max_search_query_chars
                ),
            ));
        }

        let normalized_pattern = normalize_glob_pattern(&request.pattern)?;
        let matcher = build_glob_matcher(&normalized_pattern)?;
        let scope = request
            .path
            .as_deref()
            .map(|path| {
                validate_non_empty(path, "path")?;
                normalize_relative_path(path, "path")
            })
            .transpose()?;

        let scope_path = match scope.as_ref() {
            Some(scope) => self.resolve_existing_path(scope)?,
            None => self.repo_root.clone(),
        };
        let scope_is_file = scope_path.is_file();
        let scope_relative = if scope_path == self.repo_root {
            None
        } else {
            Some(self.repo_relative_path(&scope_path)?)
        };

        let mut matches = Vec::new();
        let mut walk = WalkState::default();
        self.find_scope(
            &scope_path,
            scope_relative.as_deref(),
            scope_is_file,
            &matcher,
            &mut matches,
            &mut walk,
        )?;

        let scope_string = scope
            .as_ref()
            .map(|path| path_to_forward_slash(path.as_path()));
        let summary = if matches.is_empty() {
            match scope_string.as_deref() {
                Some(scope) => {
                    format!("Found 0 path(s) matching `{normalized_pattern}` under `{scope}`.")
                }
                None => {
                    format!("Found 0 path(s) matching `{normalized_pattern}` in the repository.")
                }
            }
        } else if walk.truncated {
            format!(
                "Found {} path(s) matching `{normalized_pattern}`; results truncated at {} path(s).",
                matches.len(),
                self.limits.max_search_results
            )
        } else {
            format!(
                "Found {} path(s) matching `{normalized_pattern}`.",
                matches.len()
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_FIND.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Find(AutonomousFindOutput {
                pattern: normalized_pattern,
                scope: scope_string,
                matches,
                scanned_files: walk.scanned_files,
                truncated: walk.truncated,
            }),
        })
    }

    pub fn git_status(
        &self,
        _request: AutonomousGitStatusRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let response = status::load_repository_status_from_root(&self.repo_root)?;
        let branch_label = display_branch_name(response.branch.as_ref());
        let changed_files = response.entries.len();
        let summary = if changed_files == 0 {
            format!("Git status reports a clean worktree on `{branch_label}`.")
        } else {
            format!(
                "Git status reports {changed_files} changed file(s) on `{branch_label}` (staged: {}, unstaged: {}, untracked: {}).",
                response.has_staged_changes,
                response.has_unstaged_changes,
                response.has_untracked_changes
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_GIT_STATUS.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::GitStatus(AutonomousGitStatusOutput {
                branch: response.branch,
                entries: response.entries,
                changed_files,
                has_staged_changes: response.has_staged_changes,
                has_unstaged_changes: response.has_unstaged_changes,
                has_untracked_changes: response.has_untracked_changes,
            }),
        })
    }

    pub fn git_diff(
        &self,
        request: AutonomousGitDiffRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let projection = diff::load_repository_diff_from_root(&self.repo_root, request.scope)?;
        let response = projection.response;
        let branch_label = display_branch_name(projection.branch.as_ref());
        let truncation_suffix = if response.truncated {
            format!("; patch truncated at {} byte(s)", diff::MAX_PATCH_BYTES)
        } else {
            String::new()
        };
        let summary = format!(
            "Rendered {} git diff for {} changed file(s) on `{branch_label}`{}.",
            scope_label(response.scope),
            projection.changed_files,
            truncation_suffix
        );

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_GIT_DIFF.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::GitDiff(AutonomousGitDiffOutput {
                scope: response.scope,
                branch: projection.branch,
                changed_files: projection.changed_files,
                patch: response.patch,
                truncated: response.truncated,
                base_revision: response.base_revision,
            }),
        })
    }

    pub fn web_search(
        &self,
        request: AutonomousWebSearchRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let output = self.web_runtime.search(request)?;
        let result_count = output.results.len();
        let summary = if result_count == 0 {
            format!("Web search returned 0 result(s) for `{}`.", output.query)
        } else if output.truncated {
            format!(
                "Web search returned {result_count} result(s) for `{}` (truncated).",
                output.query
            )
        } else {
            format!(
                "Web search returned {result_count} result(s) for `{}`.",
                output.query
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_WEB_SEARCH.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::WebSearch(output),
        })
    }

    pub fn web_fetch(
        &self,
        request: AutonomousWebFetchRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let output = self.web_runtime.fetch(request)?;
        let kind = match output.content_kind {
            AutonomousWebFetchContentKind::Html => "HTML",
            AutonomousWebFetchContentKind::PlainText => "plain-text",
        };
        let summary = if output.truncated {
            format!(
                "Fetched {kind} content from `{}` via `{}` (truncated).",
                output.url, output.final_url
            )
        } else {
            format!(
                "Fetched {kind} content from `{}` via `{}`.",
                output.url, output.final_url
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_WEB_FETCH.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::WebFetch(output),
        })
    }

    pub fn edit(&self, request: AutonomousEditRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        validate_non_empty(&request.expected, "expected")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_existing_path(&relative_path)?;

        if request.start_line == 0 || request.end_line == 0 || request.end_line < request.start_line
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_edit_range_invalid",
                "Cadence requires edit start_line/end_line to describe a non-empty inclusive range.",
            ));
        }

        let existing = self.read_text_file(&resolved_path)?;
        let total_lines = count_lines(&existing);
        if total_lines == 0 || request.start_line > total_lines || request.end_line > total_lines {
            return Err(CommandError::user_fixable(
                "autonomous_tool_edit_range_invalid",
                format!(
                    "Cadence requires edit ranges to stay within the file's 1..={total_lines} line range."
                ),
            ));
        }

        let (start_byte, end_byte) =
            line_byte_range(&existing, request.start_line, request.end_line)?;
        let current = &existing[start_byte..end_byte];
        if current != request.expected {
            return Err(CommandError::user_fixable(
                "autonomous_tool_edit_expected_text_mismatch",
                "Cadence refused to apply the edit because the requested line range no longer matches the expected text.",
            ));
        }

        let mut updated =
            String::with_capacity(existing.len() - current.len() + request.replacement.len());
        updated.push_str(&existing[..start_byte]);
        updated.push_str(&request.replacement);
        updated.push_str(&existing[end_byte..]);

        fs::write(&resolved_path, updated.as_bytes()).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_edit_write_failed",
                format!(
                    "Cadence could not persist the edit to {}: {error}",
                    resolved_path.display()
                ),
            )
        })?;

        let display_path = path_to_forward_slash(&relative_path);
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_EDIT.into(),
            summary: format!(
                "Updated lines {}-{} in `{display_path}`.",
                request.start_line, request.end_line
            ),
            command_result: None,
            output: AutonomousToolOutput::Edit(AutonomousEditOutput {
                path: display_path,
                start_line: request.start_line,
                end_line: request.end_line,
                replacement_len: request.replacement.chars().count(),
            }),
        })
    }

    pub fn write(&self, request: AutonomousWriteRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_writable_path(&relative_path)?;
        let created = !resolved_path.exists();

        if let Some(parent) = resolved_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_write_prepare_failed",
                    format!(
                        "Cadence could not prepare the parent directory for {}: {error}",
                        resolved_path.display()
                    ),
                )
            })?;
        }

        fs::write(&resolved_path, request.content.as_bytes()).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_write_failed",
                format!(
                    "Cadence could not write {}: {error}",
                    resolved_path.display()
                ),
            )
        })?;

        let display_path = path_to_forward_slash(&relative_path);
        let verb = if created { "Created" } else { "Wrote" };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_WRITE.into(),
            summary: format!(
                "{verb} `{display_path}` with {} byte(s).",
                request.content.as_bytes().len()
            ),
            command_result: None,
            output: AutonomousToolOutput::Write(AutonomousWriteOutput {
                path: display_path,
                created,
                bytes_written: request.content.as_bytes().len(),
            }),
        })
    }

    pub fn command(
        &self,
        request: AutonomousCommandRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let argv = normalize_command_argv(&request.argv)?;
        let cwd_relative = request
            .cwd
            .as_deref()
            .map(|value| {
                validate_non_empty(value, "cwd")?;
                normalize_relative_path(value, "cwd")
            })
            .transpose()?;
        let cwd = match cwd_relative.as_ref() {
            Some(path) => self.resolve_existing_directory(path)?,
            None => self.repo_root.clone(),
        };
        let timeout = normalize_timeout_ms(request.timeout_ms, self.limits.max_command_timeout_ms)?;

        let mut command = Command::new(&argv[0]);
        command
            .args(argv.iter().skip(1))
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::user_fixable(
                "autonomous_tool_command_not_found",
                format!("Cadence could not find command `{}`.", argv[0]),
            ),
            _ => CommandError::system_fault(
                "autonomous_tool_command_spawn_failed",
                format!("Cadence could not launch command `{}`: {error}", argv[0]),
            ),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_command_stdout_missing",
                "Cadence could not capture command stdout.",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_command_stderr_missing",
                "Cadence could not capture command stderr.",
            )
        })?;

        let stdout_handle = spawn_capture(stdout, self.limits.max_command_capture_bytes);
        let stderr_handle = spawn_capture(stderr, self.limits.max_command_capture_bytes);
        let started_at = Instant::now();
        let timeout_duration = Duration::from_millis(timeout);

        let (status, timed_out) = loop {
            match child.try_wait() {
                Ok(Some(status)) => break (status, false),
                Ok(None) if started_at.elapsed() >= timeout_duration => {
                    let _ = child.kill();
                    let status = child.wait().map_err(|error| {
                        CommandError::system_fault(
                            "autonomous_tool_command_wait_failed",
                            format!(
                                "Cadence could not stop timed-out command `{}`: {error}",
                                argv[0]
                            ),
                        )
                    })?;
                    break (status, true);
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CommandError::system_fault(
                        "autonomous_tool_command_wait_failed",
                        format!(
                            "Cadence could not observe command `{}` while it was running: {error}",
                            argv[0]
                        ),
                    ));
                }
            }
        };

        let stdout_capture = join_capture(stdout_handle)?;
        let stderr_capture = join_capture(stderr_handle)?;

        if timed_out {
            return Err(CommandError::retryable(
                "autonomous_tool_command_timeout",
                format!(
                    "Cadence timed out command `{}` after {timeout}ms.",
                    render_command_for_summary(&argv)
                ),
            ));
        }

        let stdout_excerpt = sanitize_command_output(
            stdout_capture.excerpt.as_slice(),
            stdout_capture.truncated,
            self.limits.max_command_excerpt_chars,
        );
        let stderr_excerpt = sanitize_command_output(
            stderr_capture.excerpt.as_slice(),
            stderr_capture.truncated,
            self.limits.max_command_excerpt_chars,
        );

        let exit_code = status.code();
        let command_result = AutonomousToolCommandResult {
            exit_code,
            timed_out: false,
            summary: command_result_summary(&argv, exit_code),
        };
        let summary = match exit_code {
            Some(0) => format!(
                "Command `{}` exited successfully in `{}`.",
                render_command_for_summary(&argv),
                display_relative_or_root(&self.repo_root, &cwd)
            ),
            Some(code) => format!(
                "Command `{}` exited with code {code} in `{}`.",
                render_command_for_summary(&argv),
                display_relative_or_root(&self.repo_root, &cwd)
            ),
            None => format!(
                "Command `{}` terminated without an exit code in `{}`.",
                render_command_for_summary(&argv),
                display_relative_or_root(&self.repo_root, &cwd)
            ),
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_COMMAND.into(),
            summary,
            command_result: Some(command_result.clone()),
            output: AutonomousToolOutput::Command(AutonomousCommandOutput {
                argv,
                cwd: display_relative_or_root(&self.repo_root, &cwd),
                stdout: stdout_excerpt.text,
                stderr: stderr_excerpt.text,
                stdout_truncated: stdout_excerpt.truncated,
                stderr_truncated: stderr_excerpt.truncated,
                stdout_redacted: stdout_excerpt.redacted,
                stderr_redacted: stderr_excerpt.redacted,
                exit_code,
                timed_out: false,
            }),
        })
    }

    fn search_scope(
        &self,
        scope: &Path,
        query: &str,
        matches: &mut Vec<AutonomousSearchMatch>,
        walk: &mut WalkState,
    ) -> CommandResult<()> {
        self.walk_scope(
            scope,
            WalkErrorCodes {
                metadata_failed: "autonomous_tool_search_metadata_failed",
                read_dir_failed: "autonomous_tool_search_read_dir_failed",
            },
            walk,
            &mut |path, walk| {
                let text = match self.read_text_file(path) {
                    Ok(text) => text,
                    Err(error) if should_skip_search_file_error(&error) => return Ok(()),
                    Err(error) => return Err(error),
                };
                let relative = path_to_forward_slash(&self.repo_relative_path(path)?);

                for (line_index, line) in text.lines().enumerate() {
                    for (byte_offset, _) in line.match_indices(query) {
                        if !push_bounded(
                            matches,
                            AutonomousSearchMatch {
                                path: relative.clone(),
                                line: line_index + 1,
                                column: byte_offset + 1,
                                preview: truncate_chars(
                                    line.trim(),
                                    self.limits.max_search_preview_chars,
                                ),
                            },
                            self.limits.max_search_results,
                            &mut walk.truncated,
                        ) {
                            return Ok(());
                        }
                    }
                }

                Ok(())
            },
        )
    }

    fn find_scope(
        &self,
        scope: &Path,
        scope_relative: Option<&Path>,
        scope_is_file: bool,
        matcher: &GlobMatcher,
        matches: &mut Vec<String>,
        walk: &mut WalkState,
    ) -> CommandResult<()> {
        self.walk_scope(
            scope,
            WalkErrorCodes {
                metadata_failed: "autonomous_tool_find_metadata_failed",
                read_dir_failed: "autonomous_tool_find_read_dir_failed",
            },
            walk,
            &mut |path, walk| {
                let repo_relative = self.repo_relative_path(path)?;
                let candidate = scope_relative_match_path(
                    repo_relative.as_path(),
                    scope_relative,
                    scope_is_file,
                )?;
                if matcher.is_match(&path_to_forward_slash(&candidate))
                    && !push_bounded(
                        matches,
                        path_to_forward_slash(&repo_relative),
                        self.limits.max_search_results,
                        &mut walk.truncated,
                    )
                {
                    return Ok(());
                }

                Ok(())
            },
        )
    }

    fn walk_scope<F>(
        &self,
        scope: &Path,
        error_codes: WalkErrorCodes,
        walk: &mut WalkState,
        visit_file: &mut F,
    ) -> CommandResult<()>
    where
        F: FnMut(&Path, &mut WalkState) -> CommandResult<()>,
    {
        if walk.truncated {
            return Ok(());
        }

        let metadata = fs::symlink_metadata(scope).map_err(|error| {
            CommandError::retryable(
                error_codes.metadata_failed,
                format!("Cadence could not inspect {}: {error}", scope.display()),
            )
        })?;

        if metadata.file_type().is_symlink() {
            return Ok(());
        }

        if metadata.is_dir() {
            if self.should_skip_directory(scope) {
                return Ok(());
            }

            for entry in self.read_sorted_directory_entries(scope, error_codes.read_dir_failed)? {
                if walk.truncated {
                    break;
                }
                self.walk_scope(&entry.path(), error_codes, walk, visit_file)?;
            }
            return Ok(());
        }

        walk.scanned_files = walk.scanned_files.saturating_add(1);
        visit_file(scope, walk)
    }

    fn read_sorted_directory_entries(
        &self,
        scope: &Path,
        error_code: &'static str,
    ) -> CommandResult<Vec<fs::DirEntry>> {
        let mut entries = fs::read_dir(scope)
            .map_err(|error| {
                CommandError::retryable(
                    error_code,
                    format!(
                        "Cadence could not enumerate directory {}: {error}",
                        scope.display()
                    ),
                )
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                CommandError::retryable(
                    error_code,
                    format!(
                        "Cadence could not enumerate directory {}: {error}",
                        scope.display()
                    ),
                )
            })?;
        entries.sort_by(|left, right| {
            left.file_name()
                .to_string_lossy()
                .cmp(&right.file_name().to_string_lossy())
        });
        Ok(entries)
    }

    fn repo_relative_path(&self, path: &Path) -> CommandResult<PathBuf> {
        path.strip_prefix(&self.repo_root)
            .map(|relative| relative.to_path_buf())
            .map_err(|_| {
                CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    format!(
                        "Cadence denied access to `{}` because it resolves outside the imported repository root.",
                        path.display()
                    ),
                    false,
                )
            })
    }

    fn should_skip_directory(&self, path: &Path) -> bool {
        path != self.repo_root
            && path
                .file_name()
                .is_some_and(|name| SKIPPED_DIRECTORIES.contains(&name.to_string_lossy().as_ref()))
    }

    fn resolve_existing_path(&self, relative_path: &Path) -> CommandResult<PathBuf> {
        let candidate = self.repo_root.join(relative_path);
        if !candidate.exists() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_path_not_found",
                format!(
                    "Cadence could not find `{}` inside the imported repository.",
                    path_to_forward_slash(relative_path)
                ),
            ));
        }

        let resolved = fs::canonicalize(&candidate).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_path_resolve_failed",
                format!("Cadence could not resolve {}: {error}", candidate.display()),
            )
        })?;
        self.ensure_inside_root(&resolved, relative_path)
    }

    fn resolve_existing_directory(&self, relative_path: &Path) -> CommandResult<PathBuf> {
        let resolved = self.resolve_existing_path(relative_path)?;
        if !resolved.is_dir() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_directory_required",
                format!(
                    "Cadence requires `{}` to resolve to a directory inside the imported repository.",
                    path_to_forward_slash(relative_path)
                ),
            ));
        }
        Ok(resolved)
    }

    fn resolve_writable_path(&self, relative_path: &Path) -> CommandResult<PathBuf> {
        let candidate = self.repo_root.join(relative_path);
        if candidate.exists() {
            let resolved = fs::canonicalize(&candidate).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_path_resolve_failed",
                    format!("Cadence could not resolve {}: {error}", candidate.display()),
                )
            })?;
            return self.ensure_inside_root(&resolved, relative_path);
        }

        let mut missing_components = Vec::<OsString>::new();
        let mut ancestor = candidate.as_path();
        while !ancestor.exists() {
            let Some(file_name) = ancestor.file_name() else {
                return Err(CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    "Cadence denied a path that escaped the imported repository.",
                    false,
                ));
            };
            missing_components.push(file_name.to_os_string());
            ancestor = ancestor.parent().ok_or_else(|| {
                CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    "Cadence denied a path that escaped the imported repository.",
                    false,
                )
            })?;
        }

        let resolved_ancestor = fs::canonicalize(ancestor).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_path_resolve_failed",
                format!("Cadence could not resolve {}: {error}", ancestor.display()),
            )
        })?;
        let mut resolved = self.ensure_inside_root(&resolved_ancestor, relative_path)?;
        for component in missing_components.into_iter().rev() {
            resolved.push(component);
        }
        Ok(resolved)
    }

    fn ensure_inside_root(&self, resolved: &Path, relative_path: &Path) -> CommandResult<PathBuf> {
        if resolved == self.repo_root || resolved.starts_with(&self.repo_root) {
            return Ok(resolved.to_path_buf());
        }

        Err(CommandError::new(
            "autonomous_tool_path_denied",
            CommandErrorClass::PolicyDenied,
            format!(
                "Cadence denied access to `{}` because it resolves outside the imported repository root.",
                path_to_forward_slash(relative_path)
            ),
            false,
        ))
    }

    fn read_text_file(&self, path: &Path) -> CommandResult<String> {
        let bytes = fs::read(path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_read_failed",
                format!("Cadence could not read {}: {error}", path.display()),
            )
        })?;
        if bytes.len() > self.limits.max_text_file_bytes {
            return Err(CommandError::user_fixable(
                "autonomous_tool_file_too_large",
                format!(
                    "Cadence refused to read {} because it exceeds the {} byte text limit.",
                    path.display(),
                    self.limits.max_text_file_bytes
                ),
            ));
        }
        String::from_utf8(bytes).map_err(|_| {
            CommandError::user_fixable(
                "autonomous_tool_file_not_text",
                format!(
                    "Cadence refused to read {} because it is not valid UTF-8 text.",
                    path.display()
                ),
            )
        })
    }
}

pub fn resolve_imported_repo_root<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
) -> CommandResult<PathBuf> {
    let registry_path = state.registry_file(app)?;
    resolve_imported_repo_root_from_registry(&registry_path, project_id)
}

pub fn resolve_imported_repo_root_from_registry(
    registry_path: &Path,
    project_id: &str,
) -> CommandResult<PathBuf> {
    let registry = registry::read_registry(registry_path)?;
    let mut live_root_records = Vec::new();
    let mut candidates = Vec::new();
    let mut pruned_stale_roots = false;

    for record in registry.projects {
        if !Path::new(&record.root_path).is_dir() {
            pruned_stale_roots = true;
            continue;
        }

        if record.project_id == project_id {
            candidates.push(record.clone());
        }
        live_root_records.push(record);
    }

    if pruned_stale_roots {
        let _ = registry::replace_projects(registry_path, live_root_records);
    }

    if candidates.is_empty() {
        return Err(CommandError::project_not_found());
    }

    let mut first_error: Option<CommandError> = None;
    for RegistryProjectRecord {
        project_id,
        root_path,
        ..
    } in candidates
    {
        match project_store::load_project_summary(Path::new(&root_path), &project_id) {
            Ok(_) => return Ok(PathBuf::from(root_path)),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(CommandError::project_not_found))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "tool", content = "input")]
pub enum AutonomousToolRequest {
    Read(AutonomousReadRequest),
    Search(AutonomousSearchRequest),
    Find(AutonomousFindRequest),
    GitStatus(AutonomousGitStatusRequest),
    GitDiff(AutonomousGitDiffRequest),
    WebSearch(AutonomousWebSearchRequest),
    WebFetch(AutonomousWebFetchRequest),
    Edit(AutonomousEditRequest),
    Write(AutonomousWriteRequest),
    Command(AutonomousCommandRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousReadRequest {
    pub path: String,
    pub start_line: Option<usize>,
    pub line_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchRequest {
    pub query: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFindRequest {
    pub pattern: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousGitStatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousGitDiffRequest {
    pub scope: RepositoryDiffScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousEditRequest {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub expected: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWriteRequest {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandRequest {
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolCommandResult {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolResult {
    pub tool_name: String,
    pub summary: String,
    pub command_result: Option<AutonomousToolCommandResult>,
    pub output: AutonomousToolOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutonomousToolOutput {
    Read(AutonomousReadOutput),
    Search(AutonomousSearchOutput),
    Find(AutonomousFindOutput),
    GitStatus(AutonomousGitStatusOutput),
    GitDiff(AutonomousGitDiffOutput),
    WebSearch(AutonomousWebSearchOutput),
    WebFetch(AutonomousWebFetchOutput),
    Edit(AutonomousEditOutput),
    Write(AutonomousWriteOutput),
    Command(AutonomousCommandOutput),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousReadOutput {
    pub path: String,
    pub start_line: usize,
    pub line_count: usize,
    pub total_lines: usize,
    pub truncated: bool,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchOutput {
    pub query: String,
    pub scope: Option<String>,
    pub matches: Vec<AutonomousSearchMatch>,
    pub scanned_files: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFindOutput {
    pub pattern: String,
    pub scope: Option<String>,
    pub matches: Vec<String>,
    pub scanned_files: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchMatch {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousGitStatusOutput {
    pub branch: Option<BranchSummaryDto>,
    pub entries: Vec<RepositoryStatusEntryDto>,
    pub changed_files: usize,
    pub has_staged_changes: bool,
    pub has_unstaged_changes: bool,
    pub has_untracked_changes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousGitDiffOutput {
    pub scope: RepositoryDiffScope,
    pub branch: Option<BranchSummaryDto>,
    pub changed_files: usize,
    pub patch: String,
    pub truncated: bool,
    pub base_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousEditOutput {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub replacement_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWriteOutput {
    pub path: String,
    pub created: bool,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandOutput {
    pub argv: Vec<String>,
    pub cwd: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_redacted: bool,
    pub stderr_redacted: bool,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

#[derive(Debug, Default)]
struct WalkState {
    scanned_files: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Copy)]
struct WalkErrorCodes {
    metadata_failed: &'static str,
    read_dir_failed: &'static str,
}

fn display_branch_name(branch: Option<&BranchSummaryDto>) -> String {
    branch
        .map(|branch| branch.name.clone())
        .filter(|branch| !branch.trim().is_empty())
        .unwrap_or_else(|| "HEAD".into())
}

fn scope_label(scope: RepositoryDiffScope) -> &'static str {
    match scope {
        RepositoryDiffScope::Staged => "staged",
        RepositoryDiffScope::Unstaged => "unstaged",
        RepositoryDiffScope::Worktree => "worktree",
    }
}

fn normalize_relative_path(value: &str, field: &'static str) -> CommandResult<PathBuf> {
    validate_non_empty(value, field)?;
    let mut normalized = PathBuf::new();
    for component in Path::new(value.trim()).components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => continue,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    format!(
                        "Cadence denied `{}` because autonomous tools may only access paths relative to the imported repository root.",
                        value.trim()
                    ),
                    false,
                ));
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(CommandError::invalid_request(field));
    }

    Ok(normalized)
}

fn normalize_glob_pattern(value: &str) -> CommandResult<String> {
    validate_non_empty(value, "pattern")?;
    let normalized = value.trim().replace('\\', "/");
    let mut segments = Vec::new();

    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." {
            return Err(CommandError::user_fixable(
                "autonomous_tool_find_pattern_invalid",
                format!(
                    "Cadence requires glob pattern `{}` to use non-empty repo-relative segments.",
                    value.trim()
                ),
            ));
        }

        if segment == ".." {
            return Err(CommandError::new(
                "autonomous_tool_path_denied",
                CommandErrorClass::PolicyDenied,
                format!(
                    "Cadence denied glob pattern `{}` because autonomous tools may only access paths relative to the imported repository root.",
                    value.trim()
                ),
                false,
            ));
        }

        segments.push(segment);
    }

    let normalized = segments.join("/");
    if normalized.is_empty() {
        return Err(CommandError::invalid_request("pattern"));
    }

    Ok(normalized)
}

fn build_glob_matcher(pattern: &str) -> CommandResult<GlobMatcher> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_find_pattern_invalid",
                format!("Cadence could not parse glob pattern `{pattern}`: {error}"),
            )
        })
}

fn normalize_command_argv(argv: &[String]) -> CommandResult<Vec<String>> {
    if argv.is_empty() || argv[0].trim().is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_command_invalid",
            "Cadence requires autonomous command requests to include a non-empty argv[0].",
        ));
    }

    if argv.iter().any(|argument| argument.contains('\0')) {
        return Err(CommandError::user_fixable(
            "autonomous_tool_command_invalid",
            "Cadence refused a command that contained a NUL byte.",
        ));
    }

    Ok(argv
        .iter()
        .map(|argument| argument.trim().to_string())
        .collect())
}

fn normalize_timeout_ms(timeout_ms: Option<u64>, max_timeout_ms: u64) -> CommandResult<u64> {
    let timeout = timeout_ms.unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS);
    if timeout == 0 || timeout > max_timeout_ms {
        return Err(CommandError::user_fixable(
            "autonomous_tool_command_timeout_invalid",
            format!("Cadence requires command timeout_ms to be between 1 and {max_timeout_ms}."),
        ));
    }
    Ok(timeout)
}

fn slice_lines(
    text: &str,
    start_line: usize,
    requested_line_count: usize,
) -> CommandResult<(String, usize, bool)> {
    if requested_line_count == 0 {
        return Err(CommandError::user_fixable(
            "autonomous_tool_read_line_count_invalid",
            "Cadence requires read line_count to be at least 1.",
        ));
    }

    if text.is_empty() {
        return Ok((String::new(), 0, false));
    }

    let total_lines = count_lines(text);
    if start_line == 0 || start_line > total_lines {
        return Err(CommandError::user_fixable(
            "autonomous_tool_read_range_invalid",
            format!(
                "Cadence requires read start_line to stay within the file's 1..={total_lines} line range."
            ),
        ));
    }

    let end_line = (start_line + requested_line_count - 1).min(total_lines);
    let (start_byte, end_byte) = line_byte_range(text, start_line, end_line)?;
    Ok((
        text[start_byte..end_byte].to_string(),
        end_line - start_line + 1,
        end_line < total_lines,
    ))
}

fn line_byte_range(
    text: &str,
    start_line: usize,
    end_line: usize,
) -> CommandResult<(usize, usize)> {
    let starts = line_start_indices(text);
    let total_lines = starts.len();
    if start_line == 0 || end_line == 0 || start_line > end_line || end_line > total_lines {
        return Err(CommandError::user_fixable(
            "autonomous_tool_edit_range_invalid",
            format!(
                "Cadence requires edit ranges to stay within the file's 1..={total_lines} line range."
            ),
        ));
    }

    let start_byte = starts[start_line - 1];
    let end_byte = if end_line == total_lines {
        text.len()
    } else {
        starts[end_line]
    };
    Ok((start_byte, end_byte))
}

fn line_start_indices(text: &str) -> Vec<usize> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut starts = vec![0];
    for (index, character) in text.char_indices() {
        if character == '\n' && index + 1 < text.len() {
            starts.push(index + 1);
        }
    }
    starts
}

fn count_lines(text: &str) -> usize {
    line_start_indices(text).len()
}

fn path_to_forward_slash(path: &Path) -> String {
    let parts = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(segment) => Some(segment.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    parts.join("/")
}

fn display_relative_or_root(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .map(path_to_forward_slash)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ".".into())
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }

    let truncated = value
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}

fn scope_relative_match_path(
    repo_relative: &Path,
    scope_relative: Option<&Path>,
    scope_is_file: bool,
) -> CommandResult<PathBuf> {
    if scope_is_file {
        return repo_relative
            .file_name()
            .map(PathBuf::from)
            .ok_or_else(|| CommandError::invalid_request("path"));
    }

    match scope_relative {
        Some(scope_relative) => repo_relative
            .strip_prefix(scope_relative)
            .map(|relative| relative.to_path_buf())
            .map_err(|_| {
                CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    format!(
                        "Cadence denied access to `{}` because it escaped the scoped search root.",
                        path_to_forward_slash(repo_relative)
                    ),
                    false,
                )
            }),
        None => Ok(repo_relative.to_path_buf()),
    }
}

fn should_skip_search_file_error(error: &CommandError) -> bool {
    matches!(
        error.code.as_str(),
        "autonomous_tool_file_not_text"
            | "autonomous_tool_file_too_large"
            | "autonomous_tool_read_failed"
    )
}

fn push_bounded<T>(results: &mut Vec<T>, value: T, limit: usize, truncated: &mut bool) -> bool {
    if results.len() >= limit {
        *truncated = true;
        return false;
    }

    results.push(value);
    if results.len() >= limit {
        *truncated = true;
        return false;
    }

    true
}

fn render_command_for_summary(argv: &[String]) -> String {
    argv.join(" ")
}

fn command_result_summary(argv: &[String], exit_code: Option<i32>) -> String {
    match exit_code {
        Some(0) => format!(
            "Command `{}` exited successfully.",
            render_command_for_summary(argv)
        ),
        Some(code) => format!(
            "Command `{}` exited with code {code}.",
            render_command_for_summary(argv)
        ),
        None => format!(
            "Command `{}` terminated without an exit code.",
            render_command_for_summary(argv)
        ),
    }
}

#[derive(Debug)]
struct OutputCapture {
    excerpt: Vec<u8>,
    truncated: bool,
}

fn spawn_capture(
    mut reader: impl Read + Send + 'static,
    max_capture_bytes: usize,
) -> thread::JoinHandle<std::io::Result<OutputCapture>> {
    thread::spawn(move || {
        let mut excerpt = Vec::new();
        let mut truncated = false;
        let mut buffer = [0_u8; 4096];

        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }

            let remaining = max_capture_bytes.saturating_sub(excerpt.len());
            if remaining > 0 {
                let to_copy = remaining.min(read);
                excerpt.extend_from_slice(&buffer[..to_copy]);
                if to_copy < read {
                    truncated = true;
                }
            } else {
                truncated = true;
            }
        }

        Ok(OutputCapture { excerpt, truncated })
    })
}

fn join_capture(
    handle: thread::JoinHandle<std::io::Result<OutputCapture>>,
) -> CommandResult<OutputCapture> {
    match handle.join() {
        Ok(Ok(capture)) => Ok(capture),
        Ok(Err(error)) => Err(CommandError::system_fault(
            "autonomous_tool_command_output_failed",
            format!("Cadence could not capture command output: {error}"),
        )),
        Err(_) => Err(CommandError::system_fault(
            "autonomous_tool_command_output_failed",
            "Cadence could not join the command output capture thread.",
        )),
    }
}

#[derive(Debug)]
struct SanitizedCommandOutput {
    text: Option<String>,
    truncated: bool,
    redacted: bool,
}

fn sanitize_command_output(
    bytes: &[u8],
    truncated: bool,
    excerpt_chars: usize,
) -> SanitizedCommandOutput {
    if bytes.is_empty() {
        return SanitizedCommandOutput {
            text: None,
            truncated,
            redacted: false,
        };
    }

    let decoded = String::from_utf8_lossy(bytes).into_owned();
    if find_prohibited_persistence_content(&decoded).is_some() {
        return SanitizedCommandOutput {
            text: Some(REDACTED_COMMAND_OUTPUT_SUMMARY.into()),
            truncated,
            redacted: true,
        };
    }

    let collapsed = decoded.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return SanitizedCommandOutput {
            text: None,
            truncated,
            redacted: false,
        };
    }

    SanitizedCommandOutput {
        text: Some(truncate_chars(trimmed, excerpt_chars)),
        truncated,
        redacted: false,
    }
}

fn find_prohibited_persistence_content(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();

    if normalized.contains("access_token")
        || normalized.contains("refresh_token")
        || normalized.contains("bearer ")
        || normalized.contains("oauth")
        || normalized.contains("sk-")
    {
        return Some("OAuth or API token material");
    }

    if normalized.contains("transcript") {
        return Some("runtime transcript text");
    }

    if normalized.contains("tool_payload")
        || normalized.contains("tool payload")
        || normalized.contains("raw payload")
    {
        return Some("tool raw payload data");
    }

    if normalized.contains("redirect_uri")
        || normalized.contains("authorization_url")
        || normalized.contains("/auth/callback")
        || normalized.contains("127.0.0.1:")
        || normalized.contains("localhost:")
    {
        return Some("OAuth redirect URL data");
    }

    if normalized.contains("chatgpt_account_id")
        || (normalized.contains("session_id") && normalized.contains("provider_id"))
    {
        return Some("auth-store contents");
    }

    if value.contains('\u{1b}')
        || value.contains('\0')
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Some("raw PTY byte sequences");
    }

    None
}
