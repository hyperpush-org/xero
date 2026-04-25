use std::{fs, path::Path};

use globset::GlobMatcher;

use super::{
    repo_scope::{
        build_glob_matcher, normalize_glob_pattern, normalize_relative_path, path_to_forward_slash,
        scope_relative_match_path, WalkErrorCodes, WalkState,
    },
    AutonomousDeleteOutput, AutonomousDeleteRequest, AutonomousEditOutput, AutonomousEditRequest,
    AutonomousFindOutput, AutonomousFindRequest, AutonomousHashOutput, AutonomousHashRequest,
    AutonomousListEntry, AutonomousListOutput, AutonomousListRequest, AutonomousMkdirOutput,
    AutonomousMkdirRequest, AutonomousPatchOutput, AutonomousPatchRequest, AutonomousReadOutput,
    AutonomousReadRequest, AutonomousRenameOutput, AutonomousRenameRequest, AutonomousSearchMatch,
    AutonomousSearchOutput, AutonomousSearchRequest, AutonomousToolOutput, AutonomousToolResult,
    AutonomousToolRuntime, AutonomousWriteOutput, AutonomousWriteRequest, AUTONOMOUS_TOOL_DELETE,
    AUTONOMOUS_TOOL_EDIT, AUTONOMOUS_TOOL_FIND, AUTONOMOUS_TOOL_HASH, AUTONOMOUS_TOOL_LIST,
    AUTONOMOUS_TOOL_MKDIR, AUTONOMOUS_TOOL_PATCH, AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_RENAME,
    AUTONOMOUS_TOOL_SEARCH, AUTONOMOUS_TOOL_WRITE,
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
                request.content.len()
            ),
            command_result: None,
            output: AutonomousToolOutput::Write(AutonomousWriteOutput {
                path: display_path,
                created,
                bytes_written: request.content.len(),
            }),
        })
    }

    pub fn patch(&self, request: AutonomousPatchRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        validate_non_empty(&request.search, "search")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_existing_path(&relative_path)?;
        let existing = self.read_text_file(&resolved_path)?;
        validate_expected_hash(
            request.expected_hash.as_deref(),
            &existing,
            "autonomous_tool_patch_expected_hash_mismatch",
        )?;

        let matches = existing.matches(&request.search).count();
        if matches == 0 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_patch_search_not_found",
                "Cadence refused to patch the file because the search text was not found.",
            ));
        }
        if matches > 1 && !request.replace_all {
            return Err(CommandError::user_fixable(
                "autonomous_tool_patch_search_ambiguous",
                "Cadence refused to patch the file because the search text matched more than once. Set replaceAll to true or use a more specific search string.",
            ));
        }

        let updated = if request.replace_all {
            existing.replace(&request.search, &request.replace)
        } else {
            existing.replacen(&request.search, &request.replace, 1)
        };
        fs::write(&resolved_path, updated.as_bytes()).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_patch_write_failed",
                format!(
                    "Cadence could not persist the patch to {}: {error}",
                    resolved_path.display()
                ),
            )
        })?;

        let display_path = path_to_forward_slash(&relative_path);
        let replacements = if request.replace_all { matches } else { 1 };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_PATCH.into(),
            summary: format!("Patched `{display_path}` with {replacements} replacement(s)."),
            command_result: None,
            output: AutonomousToolOutput::Patch(AutonomousPatchOutput {
                path: display_path,
                replacements,
                bytes_written: updated.len(),
            }),
        })
    }

    pub fn delete(&self, request: AutonomousDeleteRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_existing_path(&relative_path)?;
        if resolved_path.is_dir() && !request.recursive {
            return Err(CommandError::user_fixable(
                "autonomous_tool_delete_recursive_required",
                "Cadence requires recursive=true before deleting a directory.",
            ));
        }
        if resolved_path.is_file() {
            let existing = self.read_text_file(&resolved_path)?;
            validate_expected_hash(
                request.expected_hash.as_deref(),
                &existing,
                "autonomous_tool_delete_expected_hash_mismatch",
            )?;
        } else if request.expected_hash.is_some() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_delete_expected_hash_invalid",
                "Cadence only accepts expectedHash for file deletes.",
            ));
        }

        if resolved_path.is_dir() {
            fs::remove_dir_all(&resolved_path).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_delete_failed",
                    format!(
                        "Cadence could not delete {}: {error}",
                        resolved_path.display()
                    ),
                )
            })?;
        } else {
            fs::remove_file(&resolved_path).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_delete_failed",
                    format!(
                        "Cadence could not delete {}: {error}",
                        resolved_path.display()
                    ),
                )
            })?;
        }

        let display_path = path_to_forward_slash(&relative_path);
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_DELETE.into(),
            summary: format!("Deleted `{display_path}`."),
            command_result: None,
            output: AutonomousToolOutput::Delete(AutonomousDeleteOutput {
                path: display_path,
                recursive: request.recursive,
                existed: true,
            }),
        })
    }

    pub fn rename(&self, request: AutonomousRenameRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.from_path, "fromPath")?;
        validate_non_empty(&request.to_path, "toPath")?;
        let from_relative = normalize_relative_path(&request.from_path, "fromPath")?;
        let to_relative = normalize_relative_path(&request.to_path, "toPath")?;
        let from_resolved = self.resolve_existing_path(&from_relative)?;
        let to_resolved = self.resolve_writable_path(&to_relative)?;
        if to_resolved.exists() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_rename_target_exists",
                format!(
                    "Cadence refused to rename because `{}` already exists.",
                    path_to_forward_slash(&to_relative)
                ),
            ));
        }
        if from_resolved.is_file() {
            let existing = self.read_text_file(&from_resolved)?;
            validate_expected_hash(
                request.expected_hash.as_deref(),
                &existing,
                "autonomous_tool_rename_expected_hash_mismatch",
            )?;
        }
        if let Some(parent) = to_resolved.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_rename_prepare_failed",
                    format!(
                        "Cadence could not prepare the target directory for {}: {error}",
                        to_resolved.display()
                    ),
                )
            })?;
        }
        fs::rename(&from_resolved, &to_resolved).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_rename_failed",
                format!(
                    "Cadence could not rename {} to {}: {error}",
                    from_resolved.display(),
                    to_resolved.display()
                ),
            )
        })?;

        let from_path = path_to_forward_slash(&from_relative);
        let to_path = path_to_forward_slash(&to_relative);
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_RENAME.into(),
            summary: format!("Renamed `{from_path}` to `{to_path}`."),
            command_result: None,
            output: AutonomousToolOutput::Rename(AutonomousRenameOutput { from_path, to_path }),
        })
    }

    pub fn mkdir(&self, request: AutonomousMkdirRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_writable_path(&relative_path)?;
        let created = !resolved_path.exists();
        fs::create_dir_all(&resolved_path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_mkdir_failed",
                format!(
                    "Cadence could not create directory {}: {error}",
                    resolved_path.display()
                ),
            )
        })?;
        let display_path = path_to_forward_slash(&relative_path);
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_MKDIR.into(),
            summary: if created {
                format!("Created directory `{display_path}`.")
            } else {
                format!("Directory `{display_path}` already existed.")
            },
            command_result: None,
            output: AutonomousToolOutput::Mkdir(AutonomousMkdirOutput {
                path: display_path,
                created,
            }),
        })
    }

    pub fn list(&self, request: AutonomousListRequest) -> CommandResult<AutonomousToolResult> {
        let relative_path = request
            .path
            .as_deref()
            .map(|path| normalize_relative_path(path, "path"))
            .transpose()?;
        let scope = match relative_path.as_ref() {
            Some(path) => self.resolve_existing_path(path)?,
            None => self.repo_root.clone(),
        };
        let max_depth = request.max_depth.unwrap_or(2).min(8);
        let mut entries = Vec::new();
        let mut walk = WalkState::default();
        let omit_scope_entry = scope.is_dir();
        self.list_scope(
            &scope,
            0,
            max_depth,
            omit_scope_entry,
            &mut walk,
            &mut entries,
        )?;
        let display_path = relative_path
            .as_ref()
            .map(|path| path_to_forward_slash(path))
            .unwrap_or_else(|| ".".into());
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_LIST.into(),
            summary: format!("Listed {} item(s) under `{display_path}`.", entries.len()),
            command_result: None,
            output: AutonomousToolOutput::List(AutonomousListOutput {
                path: display_path,
                entries,
                truncated: walk.truncated,
            }),
        })
    }

    pub fn hash(&self, request: AutonomousHashRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_existing_path(&relative_path)?;
        let bytes = fs::read(&resolved_path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_hash_read_failed",
                format!(
                    "Cadence could not hash {}: {error}",
                    resolved_path.display()
                ),
            )
        })?;
        let display_path = path_to_forward_slash(&relative_path);
        let sha256 = sha256_hex(&bytes);
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_HASH.into(),
            summary: format!("Hashed `{display_path}` as SHA-256 `{sha256}`."),
            command_result: None,
            output: AutonomousToolOutput::Hash(AutonomousHashOutput {
                path: display_path,
                sha256,
                bytes: bytes.len() as u64,
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
                if matcher.is_match(path_to_forward_slash(&candidate))
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

    fn list_scope(
        &self,
        path: &Path,
        depth: usize,
        max_depth: usize,
        is_root: bool,
        walk: &mut WalkState,
        entries: &mut Vec<AutonomousListEntry>,
    ) -> CommandResult<()> {
        if walk.truncated {
            return Ok(());
        }

        let metadata = fs::symlink_metadata(path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_list_metadata_failed",
                format!("Cadence could not inspect {}: {error}", path.display()),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Ok(());
        }
        if metadata.is_dir() && self.should_skip_directory(path) {
            return Ok(());
        }

        if !is_root
            && !push_bounded(
                entries,
                AutonomousListEntry {
                    path: path_to_forward_slash(&self.repo_relative_path(path)?),
                    kind: if metadata.is_dir() {
                        "directory"
                    } else {
                        "file"
                    }
                    .into(),
                    bytes: metadata.is_file().then_some(metadata.len()),
                },
                self.limits.max_search_results,
                &mut walk.truncated,
            )
        {
            return Ok(());
        }

        if metadata.is_file() {
            walk.scanned_files = walk.scanned_files.saturating_add(1);
            return Ok(());
        }
        if depth >= max_depth {
            if metadata.is_dir() {
                walk.truncated = true;
            }
            return Ok(());
        }

        for entry in
            self.read_sorted_directory_entries(path, "autonomous_tool_list_read_dir_failed")?
        {
            self.list_scope(
                &entry.path(),
                depth.saturating_add(1),
                max_depth,
                false,
                walk,
                entries,
            )?;
            if walk.truncated {
                break;
            }
        }
        Ok(())
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

fn validate_expected_hash(
    expected_hash: Option<&str>,
    current_text: &str,
    error_code: &'static str,
) -> CommandResult<()> {
    let Some(expected_hash) = expected_hash else {
        return Ok(());
    };
    let expected_hash = expected_hash.trim();
    if expected_hash.len() != 64
        || !expected_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CommandError::user_fixable(
            "autonomous_tool_expected_hash_invalid",
            "Cadence requires expectedHash to be a lowercase SHA-256 hex digest.",
        ));
    }
    let actual = sha256_hex(current_text.as_bytes());
    if actual != expected_hash {
        return Err(CommandError::user_fixable(
            error_code,
            "Cadence refused the file operation because expectedHash no longer matches the current file contents.",
        ));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to String should not fail");
    }
    output
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
