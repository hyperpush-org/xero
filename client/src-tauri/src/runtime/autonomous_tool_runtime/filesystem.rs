use std::{fs, path::Path};

use globset::GlobMatcher;

use super::{
    repo_scope::{
        build_glob_matcher, normalize_glob_pattern, normalize_relative_path, path_to_forward_slash,
        scope_relative_match_path, WalkErrorCodes, WalkState,
    },
    AutonomousEditOutput, AutonomousEditRequest, AutonomousFindOutput, AutonomousFindRequest,
    AutonomousReadOutput, AutonomousReadRequest, AutonomousSearchMatch, AutonomousSearchOutput,
    AutonomousSearchRequest, AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime,
    AutonomousWriteOutput, AutonomousWriteRequest, AUTONOMOUS_TOOL_EDIT, AUTONOMOUS_TOOL_FIND,
    AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_SEARCH, AUTONOMOUS_TOOL_WRITE,
};

use crate::commands::{validate_non_empty, CommandError, CommandResult};

impl AutonomousToolRuntime {
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
