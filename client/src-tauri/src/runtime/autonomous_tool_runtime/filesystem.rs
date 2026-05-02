use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use globset::{GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use image::{GenericImageView, ImageFormat};
use regex::{Regex, RegexBuilder};

use super::{
    repo_scope::{
        build_glob_matcher, normalize_glob_pattern, normalize_optional_relative_path,
        normalize_relative_path, path_to_forward_slash, scope_relative_match_path, WalkErrorCodes,
        WalkState,
    },
    AutonomousDeleteOutput, AutonomousDeleteRequest, AutonomousEditOutput, AutonomousEditRequest,
    AutonomousFindOutput, AutonomousFindRequest, AutonomousHashOutput, AutonomousHashRequest,
    AutonomousLineEnding, AutonomousListEntry, AutonomousListOutput, AutonomousListRequest,
    AutonomousMkdirOutput, AutonomousMkdirRequest, AutonomousPatchFileOutput,
    AutonomousPatchOperation, AutonomousPatchOutput, AutonomousPatchRequest,
    AutonomousReadContentKind, AutonomousReadLineHash, AutonomousReadMode, AutonomousReadOutput,
    AutonomousReadRequest, AutonomousRenameOutput, AutonomousRenameRequest,
    AutonomousSearchContextLine, AutonomousSearchMatch, AutonomousSearchOutput,
    AutonomousSearchRequest, AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime,
    AutonomousWriteOutput, AutonomousWriteRequest, AUTONOMOUS_TOOL_DELETE, AUTONOMOUS_TOOL_EDIT,
    AUTONOMOUS_TOOL_FIND, AUTONOMOUS_TOOL_HASH, AUTONOMOUS_TOOL_LIST, AUTONOMOUS_TOOL_MKDIR,
    AUTONOMOUS_TOOL_PATCH, AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_RENAME, AUTONOMOUS_TOOL_SEARCH,
    AUTONOMOUS_TOOL_WRITE,
};

use crate::commands::{validate_non_empty, CommandError, CommandErrorClass, CommandResult};

const MAX_SEARCH_CONTEXT_LINES: usize = 5;
const MAX_BINARY_READ_BYTES: u64 = 20 * 1024 * 1024;
const MAX_BINARY_EXCERPT_BYTES: usize = 4 * 1024;
const IMAGE_PREVIEW_MAX_DIMENSION: u32 = 1024;
const MUTATION_DIFF_CONTEXT_LINES: usize = 3;
const MAX_MUTATION_DIFF_LINES: usize = 80;
const MAX_PATCH_OPERATIONS: usize = 64;

impl AutonomousToolRuntime {
    pub fn read(&self, request: AutonomousReadRequest) -> CommandResult<AutonomousToolResult> {
        self.read_with_approval(request, false)
    }

    pub fn read_with_operator_approval(
        &self,
        request: AutonomousReadRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.read_with_approval(request, true)
    }

    fn read_with_approval(
        &self,
        request: AutonomousReadRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let target = self.resolve_read_target(&request, operator_approved)?;
        let metadata = fs::metadata(&target.path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_read_metadata_failed",
                format!("Xero could not inspect {}: {error}", target.path.display()),
            )
        })?;
        if metadata.is_dir() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_directory",
                format!(
                    "Xero cannot read `{}` because it is a directory.",
                    target.display_path
                ),
            ));
        }

        let mode = request.mode.unwrap_or(AutonomousReadMode::Auto);
        if request.byte_offset.is_some() || request.byte_count.is_some() {
            return self.read_byte_range(request, target, metadata.len(), mode);
        }

        if metadata.len() > MAX_BINARY_READ_BYTES {
            return Ok(self.binary_metadata_result(
                target.display_path,
                metadata.len(),
                None,
                Vec::new(),
                true,
            ));
        }

        let bytes = read_file_bytes(&target.path, "autonomous_tool_read_failed")?;
        let is_supported_image = is_supported_image_path(&target.path);
        if matches!(mode, AutonomousReadMode::Image) {
            return self.image_result(target.display_path, bytes, true);
        }
        if matches!(mode, AutonomousReadMode::Auto) && is_supported_image {
            if let Ok(result) = self.image_result(target.display_path.clone(), bytes.clone(), false)
            {
                return Ok(result);
            }
        }

        if matches!(mode, AutonomousReadMode::BinaryMetadata) {
            return Ok(self.binary_metadata_result(
                target.display_path,
                metadata.len(),
                Some(sha256_hex(&bytes)),
                bytes,
                false,
            ));
        }

        match decode_text_bytes(bytes.clone()) {
            Ok(decoded) => {
                self.text_read_result(request, target.display_path, decoded, metadata.len())
            }
            Err(_) if matches!(mode, AutonomousReadMode::Text) => Err(CommandError::user_fixable(
                "autonomous_tool_file_not_text",
                format!(
                    "Xero refused to read `{}` as text because it is not valid UTF-8 text.",
                    target.display_path
                ),
            )),
            Err(_) => {
                if !is_supported_image {
                    if let Ok(result) =
                        self.image_result(target.display_path.clone(), bytes.clone(), false)
                    {
                        return Ok(result);
                    }
                }
                Ok(self.binary_metadata_result(
                    target.display_path,
                    metadata.len(),
                    Some(sha256_hex(&bytes)),
                    bytes,
                    false,
                ))
            }
        }
    }

    fn text_read_result(
        &self,
        request: AutonomousReadRequest,
        display_path: String,
        decoded: DecodedText,
        total_bytes: u64,
    ) -> CommandResult<AutonomousToolResult> {
        let text = decoded.text;
        let line_count = request
            .line_count
            .unwrap_or(self.limits.default_read_line_count);
        if line_count == 0 || line_count > self.limits.max_read_line_count {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_line_count_invalid",
                format!(
                    "Xero requires read line_count to be between 1 and {}.",
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
                    "Xero cannot start reading past the end of an empty file.",
                ));
            }
        } else if start_line == 0 || start_line > total_lines {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_range_invalid",
                format!(
                    "Xero requires read start_line to stay within the file's 1..={total_lines} line range."
                ),
            ));
        }

        let (content, actual_line_count, truncated) = slice_lines(&text, start_line, line_count)?;
        let line_hashes = if request.include_line_hashes {
            line_hashes_for_content(&content, start_line)
        } else {
            Vec::new()
        };
        let sha256 = decoded.raw_sha256;
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
                content_kind: Some(AutonomousReadContentKind::Text),
                total_bytes: Some(total_bytes),
                byte_offset: None,
                byte_count: None,
                sha256: Some(sha256),
                line_hashes,
                encoding: Some("utf-8".into()),
                line_ending: Some(decoded.line_ending),
                has_bom: Some(decoded.has_bom),
                media_type: Some("text/plain; charset=utf-8".into()),
                image_width: None,
                image_height: None,
                preview_base64: None,
                preview_bytes: None,
                binary_excerpt_base64: None,
            }),
        })
    }

    pub fn search(&self, request: AutonomousSearchRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.query, "query")?;
        if request.query.chars().count() > self.limits.max_search_query_chars {
            return Err(CommandError::user_fixable(
                "autonomous_tool_search_query_too_large",
                format!(
                    "Xero requires search queries to be {} characters or fewer.",
                    self.limits.max_search_query_chars
                ),
            ));
        }

        let scope = normalize_optional_relative_path(request.path.as_deref(), "path")?;

        let scope_path = match scope.as_ref() {
            Some(scope) => self.resolve_existing_path(scope)?,
            None => self.repo_root.clone(),
        };

        let search_options = SearchOptions::from_request(&request, self.limits.max_search_results)?;
        let search_result =
            self.search_scope(&scope_path, request.query.as_str(), &search_options)?;

        let scope_string = scope
            .as_ref()
            .map(|path| path_to_forward_slash(path.as_path()));
        let matched_files = search_result.matched_files.len();
        let summary = if search_result.matches.is_empty() {
            match scope_string.as_deref() {
                Some(scope) => format!("Found 0 matches for `{}` under `{scope}`.", request.query),
                None => format!("Found 0 matches for `{}` in the repository.", request.query),
            }
        } else if search_result.truncated {
            format!(
                "Found {} match(es) for `{}` across {} file(s); results truncated at {} match(es).",
                search_result.matches.len(),
                request.query,
                matched_files,
                search_options.max_results
            )
        } else {
            format!(
                "Found {} match(es) for `{}` across {} file(s).",
                search_result.matches.len(),
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
                matches: search_result.matches,
                scanned_files: search_result.scanned_files,
                truncated: search_result.truncated,
                total_matches: Some(search_result.total_matches),
                matched_files: Some(matched_files),
                engine: Some("ignore-walk-regex".into()),
                regex: search_options.regex,
                ignore_case: search_options.ignore_case,
                include_hidden: search_options.include_hidden,
                include_ignored: search_options.include_ignored,
                include_globs: request.include_globs,
                exclude_globs: request.exclude_globs,
                context_lines: search_options.context_lines,
            }),
        })
    }

    pub fn find(&self, request: AutonomousFindRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.pattern, "pattern")?;
        if request.pattern.chars().count() > self.limits.max_search_query_chars {
            return Err(CommandError::user_fixable(
                "autonomous_tool_find_pattern_too_large",
                format!(
                    "Xero requires find patterns to be {} characters or fewer.",
                    self.limits.max_search_query_chars
                ),
            ));
        }

        let normalized_pattern = normalize_glob_pattern(&request.pattern)?;
        let matcher = build_glob_matcher(&normalized_pattern)?;
        let scope = normalize_optional_relative_path(request.path.as_deref(), "path")?;

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
                "Xero requires edit start_line/end_line to describe a non-empty inclusive range.",
            ));
        }

        let decoded = self.read_decoded_text_file(&resolved_path)?;
        validate_expected_hash_for_bytes(
            request.expected_hash.as_deref(),
            &decoded.raw_bytes,
            "autonomous_tool_edit_expected_hash_mismatch",
        )?;
        let existing = decoded.text;
        let total_lines = count_lines(&existing);
        if total_lines == 0 || request.start_line > total_lines || request.end_line > total_lines {
            return Err(CommandError::user_fixable(
                "autonomous_tool_edit_range_invalid",
                format!(
                    "Xero requires edit ranges to stay within the file's 1..={total_lines} line range."
                ),
            ));
        }
        validate_optional_line_hash(
            request.start_line_hash.as_deref(),
            &existing,
            request.start_line,
            "startLineHash",
            "autonomous_tool_edit_line_hash_mismatch",
        )?;
        validate_optional_line_hash(
            request.end_line_hash.as_deref(),
            &existing,
            request.end_line,
            "endLineHash",
            "autonomous_tool_edit_line_hash_mismatch",
        )?;

        let (start_byte, end_byte) =
            line_byte_range(&existing, request.start_line, request.end_line)?;
        let current = &existing[start_byte..end_byte];
        if current != request.expected {
            return Err(CommandError::user_fixable(
                "autonomous_tool_edit_expected_text_mismatch",
                "Xero refused to apply the edit because the requested line range no longer matches the expected text.",
            ));
        }

        let replacement =
            normalize_replacement_line_endings(&request.replacement, decoded.line_ending);
        let mut updated = String::with_capacity(existing.len() - current.len() + replacement.len());
        updated.push_str(&existing[..start_byte]);
        updated.push_str(&replacement);
        updated.push_str(&existing[end_byte..]);
        let updated_bytes = encode_text_bytes(&updated, decoded.has_bom);

        fs::write(&resolved_path, &updated_bytes).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_edit_write_failed",
                format!(
                    "Xero could not persist the edit to {}: {error}",
                    resolved_path.display()
                ),
            )
        })?;

        let display_path = path_to_forward_slash(&relative_path);
        let old_hash = sha256_hex(&decoded.raw_bytes);
        let new_hash = sha256_hex(&updated_bytes);
        let diff = compact_text_diff(&display_path, &existing, &updated);
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
                replacement_len: replacement.chars().count(),
                old_hash: Some(old_hash),
                new_hash: Some(new_hash),
                diff: Some(diff),
                line_ending: Some(decoded.line_ending),
                bom_preserved: Some(decoded.has_bom),
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
                        "Xero could not prepare the parent directory for {}: {error}",
                        resolved_path.display()
                    ),
                )
            })?;
        }

        fs::write(&resolved_path, request.content.as_bytes()).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_write_failed",
                format!("Xero could not write {}: {error}", resolved_path.display()),
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
        let preview = request.preview;
        let operations = normalize_patch_operations(request)?;
        let planned_files = self.plan_patch_files(&operations)?;

        if !preview {
            self.write_patch_files_atomically(&planned_files)?;
        }

        let files = planned_files
            .iter()
            .map(|file| AutonomousPatchFileOutput {
                path: file.display_path.clone(),
                replacements: file.replacements,
                bytes_written: file.updated_bytes.len(),
                old_hash: file.old_hash.clone(),
                new_hash: file.new_hash.clone(),
                diff: file.diff.clone(),
                line_ending: file.line_ending,
                bom_preserved: file.bom_preserved,
            })
            .collect::<Vec<_>>();
        let replacements = files.iter().map(|file| file.replacements).sum::<usize>();
        let bytes_written = files.iter().map(|file| file.bytes_written).sum::<usize>();
        let first_file = files.first();
        let path = if files.len() == 1 {
            first_file.map(|file| file.path.clone()).unwrap_or_default()
        } else {
            format!("{} files", files.len())
        };
        let old_hash = single_file_field(&files, |file| file.old_hash.clone());
        let new_hash = single_file_field(&files, |file| file.new_hash.clone());
        let diff = if files.is_empty() {
            None
        } else {
            Some(
                files
                    .iter()
                    .map(|file| file.diff.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        };
        let line_ending = single_file_field(&files, |file| file.line_ending);
        let bom_preserved = single_file_field(&files, |file| file.bom_preserved);
        let verb = if preview { "Previewed" } else { "Patched" };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_PATCH.into(),
            summary: if files.len() == 1 {
                format!(
                    "{verb} `{}` with {replacements} replacement(s).",
                    files[0].path
                )
            } else {
                format!(
                    "{verb} {} file(s) with {replacements} total replacement(s).",
                    files.len()
                )
            },
            command_result: None,
            output: AutonomousToolOutput::Patch(AutonomousPatchOutput {
                path,
                replacements,
                bytes_written,
                applied: !preview,
                preview,
                files,
                failure: None,
                old_hash,
                new_hash,
                diff,
                line_ending,
                bom_preserved,
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
                "Xero requires recursive=true before deleting a directory.",
            ));
        }
        if resolved_path.is_file() {
            let existing = read_file_bytes(&resolved_path, "autonomous_tool_delete_read_failed")?;
            validate_expected_hash_for_bytes(
                request.expected_hash.as_deref(),
                &existing,
                "autonomous_tool_delete_expected_hash_mismatch",
            )?;
        } else if request.expected_hash.is_some() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_delete_expected_hash_invalid",
                "Xero only accepts expectedHash for file deletes.",
            ));
        }

        if resolved_path.is_dir() {
            fs::remove_dir_all(&resolved_path).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_delete_failed",
                    format!("Xero could not delete {}: {error}", resolved_path.display()),
                )
            })?;
        } else {
            fs::remove_file(&resolved_path).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_delete_failed",
                    format!("Xero could not delete {}: {error}", resolved_path.display()),
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
                    "Xero refused to rename because `{}` already exists.",
                    path_to_forward_slash(&to_relative)
                ),
            ));
        }
        if from_resolved.is_file() {
            let existing = read_file_bytes(&from_resolved, "autonomous_tool_rename_read_failed")?;
            validate_expected_hash_for_bytes(
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
                        "Xero could not prepare the target directory for {}: {error}",
                        to_resolved.display()
                    ),
                )
            })?;
        }
        fs::rename(&from_resolved, &to_resolved).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_rename_failed",
                format!(
                    "Xero could not rename {} to {}: {error}",
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
                    "Xero could not create directory {}: {error}",
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
        let relative_path = normalize_optional_relative_path(request.path.as_deref(), "path")?;
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
                format!("Xero could not hash {}: {error}", resolved_path.display()),
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
        options: &SearchOptions,
    ) -> CommandResult<SearchResult> {
        let regex = build_search_regex(query, options.regex, options.ignore_case)?;
        let mut result = SearchResult::default();
        let mut matched_files = BTreeSet::new();

        let mut builder = WalkBuilder::new(scope);
        let repo_root = self.repo_root.clone();
        builder
            .hidden(!options.include_hidden)
            .git_ignore(!options.include_ignored)
            .git_exclude(!options.include_ignored)
            .git_global(!options.include_ignored)
            .parents(true)
            .follow_links(false)
            .sort_by_file_name(|left, right| left.cmp(right))
            .filter_entry(move |entry| {
                !(entry
                    .file_type()
                    .is_some_and(|file_type| file_type.is_dir())
                    && should_skip_directory_for_root(&repo_root, entry.path()))
            });

        let include_globs = options.include_globs.as_ref();
        let exclude_globs = options.exclude_globs.as_ref();

        'walk: for entry in builder.build() {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if !entry
                .file_type()
                .is_some_and(|file_type| file_type.is_file())
            {
                continue;
            }

            let repo_relative = self.repo_relative_path(path)?;
            let display_path = path_to_forward_slash(&repo_relative);
            if let Some(globs) = include_globs {
                if !globs.is_match(display_path.as_str()) {
                    continue;
                }
            }
            if let Some(globs) = exclude_globs {
                if globs.is_match(display_path.as_str()) {
                    continue;
                }
            }

            result.scanned_files = result.scanned_files.saturating_add(1);
            let decoded = match self.read_decoded_text_file(path) {
                Ok(decoded) => decoded,
                Err(error) if should_skip_search_file_error(&error) => continue,
                Err(error) => return Err(error),
            };
            let lines = decoded.text.lines().collect::<Vec<_>>();
            let mut file_matched = false;

            for (line_index, line) in lines.iter().enumerate() {
                for found in regex.find_iter(line) {
                    result.total_matches = result.total_matches.saturating_add(1);
                    if result.matches.len() >= options.max_results {
                        result.truncated = true;
                        break 'walk;
                    }

                    let line_number = line_index + 1;
                    let match_text = found.as_str();
                    matched_files.insert(display_path.clone());
                    result.matches.push(AutonomousSearchMatch {
                        path: display_path.clone(),
                        line: line_number,
                        column: utf8_char_col(line, found.start()),
                        preview: build_search_preview(
                            line,
                            found.start(),
                            found.end(),
                            self.limits.max_search_preview_chars,
                        ),
                        end_column: Some(utf8_char_col(line, found.end())),
                        match_text: Some(truncate_chars(
                            match_text,
                            self.limits.max_search_preview_chars,
                        )),
                        line_hash: Some(line_hash(line)),
                        context_before: context_lines_before(
                            &lines,
                            line_index,
                            options.context_lines,
                            self.limits.max_search_preview_chars,
                        ),
                        context_after: context_lines_after(
                            &lines,
                            line_index,
                            options.context_lines,
                            self.limits.max_search_preview_chars,
                        ),
                    });
                    file_matched = true;
                }
            }

            if file_matched {
                matched_files.insert(display_path);
            }
        }

        result.matched_files = matched_files;
        Ok(result)
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
                format!("Xero could not inspect {}: {error}", path.display()),
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

    fn resolve_read_target(
        &self,
        request: &AutonomousReadRequest,
        operator_approved: bool,
    ) -> CommandResult<ReadTarget> {
        if request.system_path {
            if !operator_approved {
                return Err(CommandError::new(
                    "autonomous_tool_system_read_requires_approval",
                    CommandErrorClass::PolicyDenied,
                    "Xero requires operator approval before reading an absolute system path outside the imported repository.",
                    false,
                ));
            }
            let expanded = expand_system_path(&request.path)?;
            if !expanded.is_absolute() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_system_read_path_invalid",
                    "Xero requires system read paths to be absolute or `~`-relative.",
                ));
            }
            let resolved = fs::canonicalize(&expanded).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_system_read_resolve_failed",
                    format!(
                        "Xero could not resolve system path {}: {error}",
                        expanded.display()
                    ),
                )
            })?;
            return Ok(ReadTarget {
                display_path: resolved.display().to_string(),
                path: resolved,
            });
        }

        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_existing_path(&relative_path)?;
        Ok(ReadTarget {
            display_path: path_to_forward_slash(&relative_path),
            path: resolved_path,
        })
    }

    fn read_byte_range(
        &self,
        request: AutonomousReadRequest,
        target: ReadTarget,
        total_bytes: u64,
        mode: AutonomousReadMode,
    ) -> CommandResult<AutonomousToolResult> {
        let byte_offset = request.byte_offset.unwrap_or(0);
        if byte_offset > total_bytes {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_byte_offset_invalid",
                format!(
                    "Xero requires byteOffset to stay within the file's 0..={total_bytes} byte range."
                ),
            ));
        }
        let requested_count = request
            .byte_count
            .unwrap_or(self.limits.max_text_file_bytes)
            .min(self.limits.max_text_file_bytes);
        if requested_count == 0 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_byte_count_invalid",
                "Xero requires byteCount to be at least 1.",
            ));
        }
        let bytes = read_file_byte_range(&target.path, byte_offset, requested_count)?;
        let truncated = byte_offset + (bytes.len() as u64) < total_bytes;

        if matches!(mode, AutonomousReadMode::BinaryMetadata) {
            return Ok(self.binary_metadata_result(
                target.display_path,
                total_bytes,
                None,
                bytes,
                truncated,
            ));
        }

        let decoded = decode_text_bytes(bytes.clone()).map_err(|_| {
            CommandError::user_fixable(
                "autonomous_tool_file_not_text",
                "Xero refused to decode the requested byte range because it is not valid UTF-8 text.",
            )
        })?;
        let total_lines = count_lines(&decoded.text);
        let line_hashes = if request.include_line_hashes {
            line_hashes_for_content(&decoded.text, 1)
        } else {
            Vec::new()
        };
        let actual_byte_count = bytes.len();
        let summary = format!(
            "Read {actual_byte_count} byte(s) from `{}` starting at byte {byte_offset}.",
            target.display_path
        );
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_READ.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Read(AutonomousReadOutput {
                path: target.display_path,
                start_line: 1,
                line_count: total_lines,
                total_lines,
                truncated,
                content: decoded.text,
                content_kind: Some(AutonomousReadContentKind::Text),
                total_bytes: Some(total_bytes),
                byte_offset: Some(byte_offset),
                byte_count: Some(actual_byte_count),
                sha256: Some(decoded.raw_sha256),
                line_hashes,
                encoding: Some("utf-8".into()),
                line_ending: Some(decoded.line_ending),
                has_bom: Some(decoded.has_bom),
                media_type: Some("text/plain; charset=utf-8".into()),
                image_width: None,
                image_height: None,
                preview_base64: None,
                preview_bytes: None,
                binary_excerpt_base64: None,
            }),
        })
    }

    fn image_result(
        &self,
        display_path: String,
        bytes: Vec<u8>,
        strict: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let image = image::load_from_memory(&bytes).map_err(|error| {
            let message = format!("Xero could not decode `{display_path}` as an image: {error}");
            if strict {
                CommandError::user_fixable("autonomous_tool_image_decode_failed", message)
            } else {
                CommandError::user_fixable("autonomous_tool_file_not_image", message)
            }
        })?;
        let (width, height) = image.dimensions();
        let preview = image.thumbnail(IMAGE_PREVIEW_MAX_DIMENSION, IMAGE_PREVIEW_MAX_DIMENSION);
        let mut encoded = Cursor::new(Vec::new());
        preview
            .write_to(&mut encoded, ImageFormat::Png)
            .map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_image_preview_failed",
                    format!("Xero could not encode an image preview for `{display_path}`: {error}"),
                )
            })?;
        let preview = encoded.into_inner();
        let preview_len = preview.len();
        let summary =
            format!("Read image metadata and preview for `{display_path}` ({width}x{height}).");
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_READ.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Read(AutonomousReadOutput {
                path: display_path,
                start_line: 0,
                line_count: 0,
                total_lines: 0,
                truncated: false,
                content: String::new(),
                content_kind: Some(AutonomousReadContentKind::Image),
                total_bytes: Some(bytes.len() as u64),
                byte_offset: None,
                byte_count: None,
                sha256: Some(sha256_hex(&bytes)),
                line_hashes: Vec::new(),
                encoding: None,
                line_ending: None,
                has_bom: None,
                media_type: Some("image/png".into()),
                image_width: Some(width),
                image_height: Some(height),
                preview_base64: Some(BASE64_STANDARD.encode(preview)),
                preview_bytes: Some(preview_len),
                binary_excerpt_base64: None,
            }),
        })
    }

    fn binary_metadata_result(
        &self,
        display_path: String,
        total_bytes: u64,
        sha256: Option<String>,
        bytes: Vec<u8>,
        truncated: bool,
    ) -> AutonomousToolResult {
        let excerpt = if bytes.is_empty() {
            None
        } else {
            Some(BASE64_STANDARD.encode(&bytes[..bytes.len().min(MAX_BINARY_EXCERPT_BYTES)]))
        };
        AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_READ.into(),
            summary: format!("Read binary metadata for `{display_path}` ({total_bytes} byte(s))."),
            command_result: None,
            output: AutonomousToolOutput::Read(AutonomousReadOutput {
                path: display_path,
                start_line: 0,
                line_count: 0,
                total_lines: 0,
                truncated,
                content: String::new(),
                content_kind: Some(AutonomousReadContentKind::BinaryMetadata),
                total_bytes: Some(total_bytes),
                byte_offset: None,
                byte_count: None,
                sha256,
                line_hashes: Vec::new(),
                encoding: None,
                line_ending: None,
                has_bom: None,
                media_type: Some("application/octet-stream".into()),
                image_width: None,
                image_height: None,
                preview_base64: None,
                preview_bytes: None,
                binary_excerpt_base64: excerpt,
            }),
        }
    }

    fn read_decoded_text_file(&self, path: &Path) -> CommandResult<DecodedText> {
        let metadata = fs::metadata(path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_read_metadata_failed",
                format!("Xero could not inspect {}: {error}", path.display()),
            )
        })?;
        if metadata.len() as usize > self.limits.max_text_file_bytes {
            return Err(CommandError::user_fixable(
                "autonomous_tool_file_too_large",
                format!(
                    "Xero refused to read {} because it exceeds the {} byte text limit.",
                    path.display(),
                    self.limits.max_text_file_bytes
                ),
            ));
        }
        let bytes = read_file_bytes(path, "autonomous_tool_read_failed")?;
        decode_text_bytes(bytes).map_err(|_| {
            CommandError::user_fixable(
                "autonomous_tool_file_not_text",
                format!(
                    "Xero refused to read {} because it is not valid UTF-8 text.",
                    path.display()
                ),
            )
        })
    }

    fn plan_patch_files(
        &self,
        operations: &[NormalizedPatchOperation],
    ) -> CommandResult<Vec<PlannedPatchFile>> {
        let mut grouped = BTreeMap::<String, GroupedPatchOperations<'_>>::new();
        for operation in operations {
            grouped
                .entry(operation.display_path.clone())
                .and_modify(|group| group.operations.push(operation))
                .or_insert_with(|| GroupedPatchOperations {
                    relative_path: operation.relative_path.clone(),
                    operations: vec![operation],
                });
        }

        let mut planned_files = Vec::with_capacity(grouped.len());
        for (display_path, group) in grouped {
            let resolved_path = self.resolve_existing_path(&group.relative_path)?;
            let decoded = self.read_decoded_text_file(&resolved_path)?;
            let original_text = decoded.text;
            let mut updated = original_text.clone();
            let mut replacements = 0_usize;

            for operation in group.operations {
                validate_patch_expected_hash(operation, &decoded.raw_bytes)?;
                let matches = updated.matches(operation.search.as_str()).count();
                if matches == 0 {
                    return Err(patch_operation_error(
                        operation,
                        "autonomous_tool_patch_search_not_found",
                        "search text was not found in the current file contents",
                    ));
                }
                if matches > 1 && !operation.replace_all {
                    return Err(patch_operation_error(
                        operation,
                        "autonomous_tool_patch_search_ambiguous",
                        "search text matched more than once; set replaceAll=true or use a more specific search string",
                    ));
                }

                let replace =
                    normalize_replacement_line_endings(&operation.replace, decoded.line_ending);
                let applied = if operation.replace_all { matches } else { 1 };
                updated = if operation.replace_all {
                    updated.replace(operation.search.as_str(), replace.as_str())
                } else {
                    updated.replacen(operation.search.as_str(), replace.as_str(), 1)
                };
                replacements = replacements.saturating_add(applied);
            }

            let updated_bytes = encode_text_bytes(&updated, decoded.has_bom);
            let new_hash = sha256_hex(&updated_bytes);
            let diff = compact_text_diff(&display_path, &original_text, &updated);
            planned_files.push(PlannedPatchFile {
                display_path,
                resolved_path,
                original_bytes: decoded.raw_bytes,
                updated_bytes,
                replacements,
                old_hash: decoded.raw_sha256,
                new_hash,
                diff,
                line_ending: decoded.line_ending,
                bom_preserved: decoded.has_bom,
            });
        }

        Ok(planned_files)
    }

    fn write_patch_files_atomically(
        &self,
        planned_files: &[PlannedPatchFile],
    ) -> CommandResult<()> {
        let mut written_files = Vec::new();
        for file in planned_files {
            if let Err(error) = fs::write(&file.resolved_path, &file.updated_bytes) {
                let rollback_message = rollback_written_patch_files(&written_files);
                return Err(CommandError::retryable(
                    "autonomous_tool_patch_write_failed",
                    format!(
                        "Xero could not persist the patch to {}: {error}. {rollback_message}",
                        file.resolved_path.display()
                    ),
                ));
            }
            written_files.push(file);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ReadTarget {
    display_path: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct DecodedText {
    text: String,
    raw_bytes: Vec<u8>,
    raw_sha256: String,
    has_bom: bool,
    line_ending: AutonomousLineEnding,
}

#[derive(Debug)]
struct NormalizedPatchOperation {
    operation_index: usize,
    relative_path: PathBuf,
    display_path: String,
    search: String,
    replace: String,
    replace_all: bool,
    expected_hash: Option<String>,
}

#[derive(Debug)]
struct GroupedPatchOperations<'a> {
    relative_path: PathBuf,
    operations: Vec<&'a NormalizedPatchOperation>,
}

#[derive(Debug)]
struct PlannedPatchFile {
    display_path: String,
    resolved_path: PathBuf,
    original_bytes: Vec<u8>,
    updated_bytes: Vec<u8>,
    replacements: usize,
    old_hash: String,
    new_hash: String,
    diff: String,
    line_ending: AutonomousLineEnding,
    bom_preserved: bool,
}

#[derive(Debug)]
struct SearchOptions {
    regex: bool,
    ignore_case: bool,
    include_hidden: bool,
    include_ignored: bool,
    include_globs: Option<GlobSet>,
    exclude_globs: Option<GlobSet>,
    context_lines: usize,
    max_results: usize,
}

impl SearchOptions {
    fn from_request(
        request: &AutonomousSearchRequest,
        default_max_results: usize,
    ) -> CommandResult<Self> {
        let context_lines = request.context_lines.unwrap_or(0);
        if context_lines > MAX_SEARCH_CONTEXT_LINES {
            return Err(CommandError::user_fixable(
                "autonomous_tool_search_context_too_large",
                format!(
                    "Xero requires search contextLines to be between 0 and {MAX_SEARCH_CONTEXT_LINES}."
                ),
            ));
        }
        let max_results = request.max_results.unwrap_or(default_max_results);
        if max_results == 0 || max_results > default_max_results {
            return Err(CommandError::user_fixable(
                "autonomous_tool_search_max_results_invalid",
                format!(
                    "Xero requires search maxResults to be between 1 and {default_max_results}."
                ),
            ));
        }

        Ok(Self {
            regex: request.regex,
            ignore_case: request.ignore_case,
            include_hidden: request.include_hidden,
            include_ignored: request.include_ignored,
            include_globs: build_search_globset(&request.include_globs, "includeGlobs")?,
            exclude_globs: build_search_globset(&request.exclude_globs, "excludeGlobs")?,
            context_lines,
            max_results,
        })
    }
}

#[derive(Debug, Default)]
struct SearchResult {
    matches: Vec<AutonomousSearchMatch>,
    matched_files: BTreeSet<String>,
    scanned_files: usize,
    total_matches: usize,
    truncated: bool,
}

fn slice_lines(
    text: &str,
    start_line: usize,
    requested_line_count: usize,
) -> CommandResult<(String, usize, bool)> {
    if requested_line_count == 0 {
        return Err(CommandError::user_fixable(
            "autonomous_tool_read_line_count_invalid",
            "Xero requires read line_count to be at least 1.",
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
                "Xero requires read start_line to stay within the file's 1..={total_lines} line range."
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
                "Xero requires edit ranges to stay within the file's 1..={total_lines} line range."
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

fn read_file_bytes(path: &Path, error_code: &'static str) -> CommandResult<Vec<u8>> {
    fs::read(path).map_err(|error| {
        CommandError::retryable(
            error_code,
            format!("Xero could not read {}: {error}", path.display()),
        )
    })
}

fn read_file_byte_range(path: &Path, offset: u64, byte_count: usize) -> CommandResult<Vec<u8>> {
    let mut file = File::open(path).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_read_failed",
            format!("Xero could not open {}: {error}", path.display()),
        )
    })?;
    file.seek(SeekFrom::Start(offset)).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_read_seek_failed",
            format!("Xero could not seek in {}: {error}", path.display()),
        )
    })?;
    let mut buffer = vec![0_u8; byte_count];
    let read = file.read(&mut buffer).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_read_failed",
            format!("Xero could not read {}: {error}", path.display()),
        )
    })?;
    buffer.truncate(read);
    Ok(buffer)
}

fn decode_text_bytes(bytes: Vec<u8>) -> Result<DecodedText, std::string::FromUtf8Error> {
    let raw_sha256 = sha256_hex(&bytes);
    let has_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let text_bytes = if has_bom {
        bytes[3..].to_vec()
    } else {
        bytes.clone()
    };
    let text = String::from_utf8(text_bytes)?;
    let line_ending = detect_line_ending(&text);
    Ok(DecodedText {
        text,
        raw_bytes: bytes,
        raw_sha256,
        has_bom,
        line_ending,
    })
}

fn encode_text_bytes(text: &str, has_bom: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len() + if has_bom { 3 } else { 0 });
    if has_bom {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

fn detect_line_ending(text: &str) -> AutonomousLineEnding {
    let bytes = text.as_bytes();
    let lf = bytes.iter().filter(|byte| **byte == b'\n').count();
    if lf == 0 {
        return AutonomousLineEnding::None;
    }
    let crlf = bytes.windows(2).filter(|window| *window == b"\r\n").count();
    match (crlf, lf) {
        (0, _) => AutonomousLineEnding::Lf,
        (crlf, lf) if crlf == lf => AutonomousLineEnding::Crlf,
        _ => AutonomousLineEnding::Mixed,
    }
}

fn normalize_replacement_line_endings(
    replacement: &str,
    line_ending: AutonomousLineEnding,
) -> String {
    match line_ending {
        AutonomousLineEnding::Crlf => replacement.replace("\r\n", "\n").replace('\n', "\r\n"),
        AutonomousLineEnding::Lf => replacement.replace("\r\n", "\n"),
        AutonomousLineEnding::Mixed | AutonomousLineEnding::None => replacement.to_string(),
    }
}

fn build_search_regex(query: &str, is_regex: bool, ignore_case: bool) -> CommandResult<Regex> {
    let pattern = if is_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(ignore_case)
        .build()
        .map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_search_regex_invalid",
                format!("Xero could not compile search regex `{query}`: {error}"),
            )
        })
}

fn build_search_globset(
    patterns: &[String],
    field: &'static str,
) -> CommandResult<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for raw in patterns {
        let pattern = normalize_glob_pattern(raw).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_search_glob_invalid",
                format!(
                    "Xero could not parse {field} entry `{raw}`: {}",
                    error.message
                ),
            )
        })?;
        let glob = GlobBuilder::new(&pattern)
            .literal_separator(true)
            .build()
            .map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_tool_search_glob_invalid",
                    format!("Xero could not parse {field} entry `{raw}`: {error}"),
                )
            })?;
        builder.add(glob);
    }
    let set = builder.build().map_err(|error| {
        CommandError::user_fixable(
            "autonomous_tool_search_glob_invalid",
            format!("Xero could not build {field}: {error}"),
        )
    })?;
    Ok(Some(set))
}

fn build_search_preview(line: &str, match_start: usize, match_end: usize, limit: usize) -> String {
    if line.chars().count() <= limit {
        return line.trim().to_string();
    }
    let left_budget = limit / 3;
    let right_budget = limit.saturating_sub(left_budget).saturating_sub(1);
    let start = snap_char_boundary(line, match_start.saturating_sub(left_budget));
    let end = snap_char_boundary(line, (match_end + right_budget).min(line.len()));
    let mut preview = line[start..end].trim().to_string();
    if start > 0 {
        preview.insert(0, '…');
    }
    if end < line.len() {
        preview.push('…');
    }
    preview
}

fn context_lines_before(
    lines: &[&str],
    line_index: usize,
    context_lines: usize,
    limit: usize,
) -> Vec<AutonomousSearchContextLine> {
    if context_lines == 0 {
        return Vec::new();
    }
    let start = line_index.saturating_sub(context_lines);
    lines[start..line_index]
        .iter()
        .enumerate()
        .map(|(offset, text)| AutonomousSearchContextLine {
            line: start + offset + 1,
            text: truncate_chars(text.trim(), limit),
        })
        .collect()
}

fn context_lines_after(
    lines: &[&str],
    line_index: usize,
    context_lines: usize,
    limit: usize,
) -> Vec<AutonomousSearchContextLine> {
    if context_lines == 0 {
        return Vec::new();
    }
    let start = (line_index + 1).min(lines.len());
    let end = (start + context_lines).min(lines.len());
    lines[start..end]
        .iter()
        .enumerate()
        .map(|(offset, text)| AutonomousSearchContextLine {
            line: start + offset + 1,
            text: truncate_chars(text.trim(), limit),
        })
        .collect()
}

fn utf8_char_col(line: &str, byte_offset: usize) -> usize {
    let clamped = snap_char_boundary(line, byte_offset.min(line.len()));
    line[..clamped].chars().count() + 1
}

fn snap_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn line_hashes_for_content(content: &str, start_line: usize) -> Vec<AutonomousReadLineHash> {
    content
        .lines()
        .enumerate()
        .map(|(offset, line)| AutonomousReadLineHash {
            line: start_line + offset,
            hash: line_hash(line),
        })
        .collect()
}

fn line_hash(line: &str) -> String {
    sha256_hex(line.as_bytes())
}

fn validate_optional_line_hash(
    expected_hash: Option<&str>,
    text: &str,
    line: usize,
    field: &'static str,
    error_code: &'static str,
) -> CommandResult<()> {
    let Some(expected_hash) = expected_hash else {
        return Ok(());
    };
    validate_sha256(expected_hash, field)?;
    let line_text = line_content_without_ending(text, line)?;
    let actual = line_hash(line_text);
    if actual != expected_hash {
        return Err(CommandError::user_fixable(
            error_code,
            format!("Xero refused the edit because {field} no longer matches line {line}."),
        ));
    }
    Ok(())
}

fn line_content_without_ending(text: &str, line: usize) -> CommandResult<&str> {
    let (start, end) = line_byte_range(text, line, line)?;
    Ok(text[start..end]
        .trim_end_matches('\n')
        .trim_end_matches('\r'))
}

fn is_supported_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg"
            )
        })
        .unwrap_or(false)
}

fn expand_system_path(value: &str) -> CommandResult<PathBuf> {
    let trimmed = value.trim();
    if trimmed == "~" || trimmed.starts_with("~/") {
        let home = dirs::home_dir().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_system_read_home_unavailable",
                "Xero could not resolve the current user's home directory.",
            )
        })?;
        if trimmed == "~" {
            return Ok(home);
        }
        return Ok(home.join(&trimmed[2..]));
    }
    Ok(PathBuf::from(trimmed))
}

fn should_skip_directory_for_root(repo_root: &Path, path: &Path) -> bool {
    path != repo_root
        && path.file_name().is_some_and(|name| {
            [
                ".git",
                "node_modules",
                "target",
                ".next",
                "dist",
                "build",
                "coverage",
                ".turbo",
                ".yarn",
                ".pnpm-store",
            ]
            .contains(&name.to_string_lossy().as_ref())
        })
}

fn should_skip_search_file_error(error: &CommandError) -> bool {
    matches!(
        error.code.as_str(),
        "autonomous_tool_file_not_text"
            | "autonomous_tool_file_too_large"
            | "autonomous_tool_read_failed"
    )
}

fn normalize_patch_operations(
    request: AutonomousPatchRequest,
) -> CommandResult<Vec<NormalizedPatchOperation>> {
    let has_legacy_fields =
        request.path.is_some() || request.search.is_some() || request.replace.is_some();
    if has_legacy_fields && !request.operations.is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_patch_request_invalid",
            "Xero requires patch requests to use either path/search/replace or operations, not both.",
        ));
    }

    let operations = if request.operations.is_empty() {
        vec![AutonomousPatchOperation {
            path: required_patch_field(request.path, "path")?,
            search: required_patch_field(request.search, "search")?,
            replace: request.replace.unwrap_or_default(),
            replace_all: request.replace_all,
            expected_hash: request.expected_hash,
        }]
    } else {
        request.operations
    };

    if operations.is_empty() || operations.len() > MAX_PATCH_OPERATIONS {
        return Err(CommandError::user_fixable(
            "autonomous_tool_patch_operation_count_invalid",
            format!(
                "Xero requires patch requests to include 1..={MAX_PATCH_OPERATIONS} operation(s)."
            ),
        ));
    }

    operations
        .into_iter()
        .enumerate()
        .map(|(index, operation)| {
            validate_non_empty(&operation.path, "path")?;
            validate_non_empty(&operation.search, "search")?;
            let relative_path = normalize_relative_path(&operation.path, "path")?;
            let display_path = path_to_forward_slash(&relative_path);
            Ok(NormalizedPatchOperation {
                operation_index: index,
                relative_path,
                display_path,
                search: operation.search,
                replace: operation.replace,
                replace_all: operation.replace_all,
                expected_hash: operation.expected_hash,
            })
        })
        .collect()
}

fn required_patch_field(value: Option<String>, field: &'static str) -> CommandResult<String> {
    match value {
        Some(value) => Ok(value),
        None => Err(CommandError::user_fixable(
            "autonomous_tool_patch_request_invalid",
            format!("Xero requires patch field `{field}` when operations is not provided."),
        )),
    }
}

fn validate_patch_expected_hash(
    operation: &NormalizedPatchOperation,
    current_bytes: &[u8],
) -> CommandResult<()> {
    let Some(expected_hash) = operation.expected_hash.as_deref() else {
        return Ok(());
    };
    validate_sha256(expected_hash, "expectedHash")?;
    let actual = sha256_hex(current_bytes);
    if actual != expected_hash.trim() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_patch_expected_hash_mismatch",
            format!(
                "Xero refused patch operation #{} for `{}` because expectedHash `{}` no longer matches the current file hash `{actual}`.",
                operation.operation_index + 1,
                operation.display_path,
                expected_hash.trim()
            ),
        ));
    }
    Ok(())
}

fn patch_operation_error(
    operation: &NormalizedPatchOperation,
    code: &'static str,
    reason: &str,
) -> CommandError {
    CommandError::user_fixable(
        code,
        format!(
            "Xero refused patch operation #{} for `{}` because {reason}.",
            operation.operation_index + 1,
            operation.display_path
        ),
    )
}

fn rollback_written_patch_files(written_files: &[&PlannedPatchFile]) -> String {
    if written_files.is_empty() {
        return "No earlier patch writes needed rollback.".into();
    }

    let mut failed = Vec::new();
    for file in written_files.iter().rev() {
        if let Err(error) = fs::write(&file.resolved_path, &file.original_bytes) {
            failed.push(format!("{} ({error})", file.display_path));
        }
    }

    if failed.is_empty() {
        format!(
            "Rolled back {} earlier patch write(s) from memory.",
            written_files.len()
        )
    } else {
        format!(
            "Rollback attempted for {} earlier patch write(s), but {} restore(s) failed: {}.",
            written_files.len(),
            failed.len(),
            failed.join(", ")
        )
    }
}

fn single_file_field<T>(
    files: &[AutonomousPatchFileOutput],
    field: impl FnOnce(&AutonomousPatchFileOutput) -> T,
) -> Option<T> {
    if files.len() == 1 {
        Some(field(&files[0]))
    } else {
        None
    }
}

fn validate_expected_hash_for_bytes(
    expected_hash: Option<&str>,
    current_bytes: &[u8],
    error_code: &'static str,
) -> CommandResult<()> {
    let Some(expected_hash) = expected_hash else {
        return Ok(());
    };
    validate_sha256(expected_hash, "expectedHash")?;
    let actual = sha256_hex(current_bytes);
    if actual != expected_hash.trim() {
        return Err(CommandError::user_fixable(
            error_code,
            "Xero refused the file operation because expectedHash no longer matches the current file contents.",
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &'static str) -> CommandResult<()> {
    let expected_hash = value.trim();
    if expected_hash.len() != 64
        || !expected_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CommandError::user_fixable(
            "autonomous_tool_expected_hash_invalid",
            format!("Xero requires {field} to be a lowercase SHA-256 hex digest."),
        ));
    }
    Ok(())
}

fn compact_text_diff(path: &str, before: &str, after: &str) -> String {
    if before == after {
        return format!("--- {path}\n+++ {path}\n");
    }

    let before_lines = before.lines().collect::<Vec<_>>();
    let after_lines = after.lines().collect::<Vec<_>>();
    let mut prefix = 0;
    while prefix < before_lines.len()
        && prefix < after_lines.len()
        && before_lines[prefix] == after_lines[prefix]
    {
        prefix += 1;
    }

    let mut before_suffix = before_lines.len();
    let mut after_suffix = after_lines.len();
    while before_suffix > prefix
        && after_suffix > prefix
        && before_lines[before_suffix - 1] == after_lines[after_suffix - 1]
    {
        before_suffix -= 1;
        after_suffix -= 1;
    }

    let context_start = prefix.saturating_sub(MUTATION_DIFF_CONTEXT_LINES);
    let context_end = (before_suffix + MUTATION_DIFF_CONTEXT_LINES).min(before_lines.len());
    let old_start = context_start + 1;
    let old_count = context_end.saturating_sub(context_start);
    let new_count = (after_suffix + MUTATION_DIFF_CONTEXT_LINES)
        .min(after_lines.len())
        .saturating_sub(context_start.min(after_lines.len()));

    let mut output = format!(
        "--- {path}\n+++ {path}\n@@ -{old_start},{old_count} +{old_start},{new_count} @@\n"
    );
    let mut emitted = 0;
    for line in &before_lines[context_start..prefix] {
        if emitted >= MAX_MUTATION_DIFF_LINES {
            output.push_str(" ...\n");
            return output;
        }
        output.push(' ');
        output.push_str(&truncate_chars(line, 240));
        output.push('\n');
        emitted += 1;
    }
    for line in &before_lines[prefix..before_suffix] {
        if emitted >= MAX_MUTATION_DIFF_LINES {
            output.push_str(" ...\n");
            return output;
        }
        output.push('-');
        output.push_str(&truncate_chars(line, 240));
        output.push('\n');
        emitted += 1;
    }
    for line in &after_lines[prefix..after_suffix] {
        if emitted >= MAX_MUTATION_DIFF_LINES {
            output.push_str(" ...\n");
            return output;
        }
        output.push('+');
        output.push_str(&truncate_chars(line, 240));
        output.push('\n');
        emitted += 1;
    }
    for line in &before_lines[before_suffix..context_end] {
        if emitted >= MAX_MUTATION_DIFF_LINES {
            output.push_str(" ...\n");
            return output;
        }
        output.push(' ');
        output.push_str(&truncate_chars(line, 240));
        output.push('\n');
        emitted += 1;
    }
    output
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn search_supports_regex_context_globs_hidden_and_gitignore_controls() {
        let tempdir = tempdir().expect("tempdir");
        let root = tempdir.path();
        fs::create_dir_all(root.join("src")).expect("src dir");
        fs::write(root.join(".gitignore"), "ignored.txt\n").expect("gitignore");
        fs::write(root.join("src/main.rs"), "alpha\nBeta 123\nomega\n").expect("source");
        fs::write(root.join("ignored.txt"), "Beta 999\n").expect("ignored");
        fs::write(root.join(".hidden.txt"), "Beta 777\n").expect("hidden");

        let runtime = AutonomousToolRuntime::new(root).expect("runtime");
        let output = search_output(runtime.search(AutonomousSearchRequest {
            query: "beta\\s+\\d+".into(),
            path: None,
            regex: true,
            ignore_case: true,
            include_hidden: false,
            include_ignored: false,
            include_globs: vec!["src/*.rs".into()],
            exclude_globs: Vec::new(),
            context_lines: Some(1),
            max_results: None,
        }));

        assert_eq!(output.matches.len(), 1);
        assert_eq!(output.matches[0].path, "src/main.rs");
        assert_eq!(output.matches[0].line, 2);
        assert_eq!(output.matches[0].context_before[0].text, "alpha");
        assert_eq!(output.matches[0].context_after[0].text, "omega");
        assert_eq!(output.matched_files, Some(1));
        assert_eq!(output.context_lines, 1);

        let output = search_output(runtime.search(AutonomousSearchRequest {
            query: "Beta".into(),
            path: None,
            regex: false,
            ignore_case: false,
            include_hidden: true,
            include_ignored: true,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            context_lines: None,
            max_results: None,
        }));
        let paths = output
            .matches
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<BTreeSet<_>>();
        assert!(paths.contains(".hidden.txt"));
        assert!(paths.contains("ignored.txt"));
        assert!(paths.contains("src/main.rs"));
    }

    #[test]
    fn observe_tools_treat_blank_optional_paths_as_repo_root() {
        let tempdir = tempdir().expect("tempdir");
        let root = tempdir.path();
        fs::create_dir_all(root.join("src")).expect("src dir");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("source");

        let runtime = AutonomousToolRuntime::new(root).expect("runtime");
        let list_output = list_output(runtime.list(AutonomousListRequest {
            path: Some("  ".into()),
            max_depth: Some(1),
        }));
        assert_eq!(list_output.path, ".");
        assert!(list_output.entries.iter().any(|entry| entry.path == "src"));

        let search_output = search_output(runtime.search(AutonomousSearchRequest {
            query: "main".into(),
            path: Some("".into()),
            regex: false,
            ignore_case: false,
            include_hidden: false,
            include_ignored: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            context_lines: None,
            max_results: None,
        }));
        assert_eq!(search_output.scope, None);
        assert_eq!(search_output.matches[0].path, "src/main.rs");

        let find_output = find_output(runtime.find(AutonomousFindRequest {
            pattern: "**/*.rs".into(),
            path: Some("".into()),
        }));
        assert_eq!(find_output.scope, None);
        assert!(find_output.matches.iter().any(|path| path == "src/main.rs"));
    }

    #[test]
    fn read_supports_images_binary_metadata_byte_ranges_and_line_hashes() {
        let tempdir = tempdir().expect("tempdir");
        let root = tempdir.path();
        fs::write(root.join("log.txt"), "0123456789abcdef\nsecond\n").expect("log");
        fs::write(root.join("blob.bin"), [0, 159, 146, 150]).expect("blob");

        let image = image::RgbImage::from_pixel(2, 1, image::Rgb([255, 0, 0]));
        image.save(root.join("pixel.png")).expect("png");

        let runtime = AutonomousToolRuntime::new(root).expect("runtime");
        let image_output = read_output(runtime.read(read_request("pixel.png")));
        assert_eq!(
            image_output.content_kind,
            Some(AutonomousReadContentKind::Image)
        );
        assert_eq!(image_output.image_width, Some(2));
        assert_eq!(image_output.image_height, Some(1));
        assert!(image_output.preview_base64.is_some());

        let binary_output = read_output(runtime.read(read_request("blob.bin")));
        assert_eq!(
            binary_output.content_kind,
            Some(AutonomousReadContentKind::BinaryMetadata)
        );
        assert_eq!(binary_output.total_bytes, Some(4));
        assert!(binary_output.binary_excerpt_base64.is_some());

        let mut range_request = read_request("log.txt");
        range_request.byte_offset = Some(4);
        range_request.byte_count = Some(4);
        range_request.include_line_hashes = true;
        let range_output = read_output(runtime.read(range_request));
        assert_eq!(range_output.content, "4567");
        assert_eq!(range_output.byte_offset, Some(4));
        assert_eq!(range_output.byte_count, Some(4));
        assert_eq!(range_output.line_hashes.len(), 1);
    }

    #[test]
    fn edit_uses_line_hash_anchors_and_preserves_bom_crlf_with_diff() {
        let tempdir = tempdir().expect("tempdir");
        let root = tempdir.path();
        let path = root.join("notes.txt");
        fs::write(&path, b"\xEF\xBB\xBFone\r\ntwo\r\nthree\r\n").expect("notes");

        let runtime = AutonomousToolRuntime::new(root).expect("runtime");
        let mut read = read_request("notes.txt");
        read.line_count = Some(3);
        read.include_line_hashes = true;
        let read_output = read_output(runtime.read(read));
        let line_two_hash = read_output
            .line_hashes
            .iter()
            .find(|entry| entry.line == 2)
            .expect("line 2 hash")
            .hash
            .clone();

        let edit_output = edit_output(runtime.edit(AutonomousEditRequest {
            path: "notes.txt".into(),
            start_line: 2,
            end_line: 2,
            expected: "two\r\n".into(),
            replacement: "TWO\n".into(),
            expected_hash: read_output.sha256.clone(),
            start_line_hash: Some(line_two_hash.clone()),
            end_line_hash: Some(line_two_hash),
        }));

        let bytes = fs::read(&path).expect("updated bytes");
        assert!(bytes.starts_with(b"\xEF\xBB\xBF"));
        assert!(String::from_utf8(bytes).unwrap().contains("TWO\r\n"));
        assert_ne!(edit_output.old_hash, edit_output.new_hash);
        assert_eq!(edit_output.line_ending, Some(AutonomousLineEnding::Crlf));
        assert!(edit_output.diff.unwrap().contains("+TWO"));

        let err = runtime
            .edit(AutonomousEditRequest {
                path: "notes.txt".into(),
                start_line: 2,
                end_line: 2,
                expected: "TWO\r\n".into(),
                replacement: "two\r\n".into(),
                expected_hash: None,
                start_line_hash: Some("0".repeat(64)),
                end_line_hash: None,
            })
            .expect_err("line hash mismatch");
        assert_eq!(err.code, "autonomous_tool_edit_line_hash_mismatch");
    }

    #[test]
    fn system_read_requires_operator_approval() {
        let repo = tempdir().expect("repo");
        let outside = tempdir().expect("outside");
        let outside_file = outside.path().join("outside.txt");
        fs::write(&outside_file, "outside\n").expect("outside");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");

        let denied = runtime
            .read(AutonomousReadRequest {
                path: outside_file.display().to_string(),
                system_path: true,
                mode: Some(AutonomousReadMode::Text),
                start_line: None,
                line_count: None,
                byte_offset: None,
                byte_count: None,
                include_line_hashes: false,
            })
            .expect_err("approval required");
        assert_eq!(denied.class, CommandErrorClass::PolicyDenied);

        let approved = read_output(runtime.read_with_operator_approval(AutonomousReadRequest {
            path: outside_file.display().to_string(),
            system_path: true,
            mode: Some(AutonomousReadMode::Text),
            start_line: None,
            line_count: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        }));
        assert_eq!(approved.content, "outside\n");
    }

    #[test]
    fn patch_supports_preview_and_multi_file_apply() {
        let tempdir = tempdir().expect("tempdir");
        let root = tempdir.path();
        fs::write(root.join("alpha.txt"), "one\ntwo\n").expect("alpha");
        fs::write(root.join("beta.txt"), "red\nblue\n").expect("beta");

        let runtime = AutonomousToolRuntime::new(root).expect("runtime");
        let preview = patch_output(runtime.patch(AutonomousPatchRequest {
            path: None,
            search: None,
            replace: None,
            replace_all: false,
            expected_hash: None,
            preview: true,
            operations: vec![
                AutonomousPatchOperation {
                    path: "alpha.txt".into(),
                    search: "two\n".into(),
                    replace: "TWO\n".into(),
                    replace_all: false,
                    expected_hash: None,
                },
                AutonomousPatchOperation {
                    path: "beta.txt".into(),
                    search: "blue\n".into(),
                    replace: "BLUE\n".into(),
                    replace_all: false,
                    expected_hash: None,
                },
            ],
        }));

        assert!(preview.preview);
        assert!(!preview.applied);
        assert_eq!(preview.files.len(), 2);
        assert_eq!(preview.replacements, 2);
        assert_eq!(
            fs::read_to_string(root.join("alpha.txt")).unwrap(),
            "one\ntwo\n"
        );

        let applied = patch_output(runtime.patch(AutonomousPatchRequest {
            preview: false,
            ..preview_request()
        }));

        assert!(applied.applied);
        assert!(!applied.preview);
        assert_eq!(applied.files.len(), 2);
        assert_eq!(
            fs::read_to_string(root.join("alpha.txt")).unwrap(),
            "one\nTWO\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("beta.txt")).unwrap(),
            "red\nBLUE\n"
        );
    }

    #[test]
    fn patch_reports_exact_operation_diagnostics() {
        let tempdir = tempdir().expect("tempdir");
        let root = tempdir.path();
        fs::write(root.join("notes.txt"), "same\nsame\n").expect("notes");

        let runtime = AutonomousToolRuntime::new(root).expect("runtime");
        let err = runtime
            .patch(AutonomousPatchRequest {
                path: None,
                search: None,
                replace: None,
                replace_all: false,
                expected_hash: None,
                preview: false,
                operations: vec![AutonomousPatchOperation {
                    path: "notes.txt".into(),
                    search: "same\n".into(),
                    replace: "changed\n".into(),
                    replace_all: false,
                    expected_hash: None,
                }],
            })
            .expect_err("ambiguous patch");

        assert_eq!(err.code, "autonomous_tool_patch_search_ambiguous");
        assert!(err.message.contains("operation #1"));
        assert!(err.message.contains("notes.txt"));
    }

    fn read_request(path: &str) -> AutonomousReadRequest {
        AutonomousReadRequest {
            path: path.into(),
            system_path: false,
            mode: None,
            start_line: None,
            line_count: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        }
    }

    fn read_output(result: CommandResult<AutonomousToolResult>) -> AutonomousReadOutput {
        match result.expect("read").output {
            AutonomousToolOutput::Read(output) => output,
            output => panic!("unexpected output: {output:?}"),
        }
    }

    fn search_output(result: CommandResult<AutonomousToolResult>) -> AutonomousSearchOutput {
        match result.expect("search").output {
            AutonomousToolOutput::Search(output) => output,
            output => panic!("unexpected output: {output:?}"),
        }
    }

    fn find_output(result: CommandResult<AutonomousToolResult>) -> AutonomousFindOutput {
        match result.expect("find").output {
            AutonomousToolOutput::Find(output) => output,
            output => panic!("unexpected output: {output:?}"),
        }
    }

    fn list_output(result: CommandResult<AutonomousToolResult>) -> AutonomousListOutput {
        match result.expect("list").output {
            AutonomousToolOutput::List(output) => output,
            output => panic!("unexpected output: {output:?}"),
        }
    }

    fn edit_output(result: CommandResult<AutonomousToolResult>) -> AutonomousEditOutput {
        match result.expect("edit").output {
            AutonomousToolOutput::Edit(output) => output,
            output => panic!("unexpected output: {output:?}"),
        }
    }

    fn patch_output(result: CommandResult<AutonomousToolResult>) -> AutonomousPatchOutput {
        match result.expect("patch").output {
            AutonomousToolOutput::Patch(output) => output,
            output => panic!("unexpected output: {output:?}"),
        }
    }

    fn preview_request() -> AutonomousPatchRequest {
        AutonomousPatchRequest {
            path: None,
            search: None,
            replace: None,
            replace_all: false,
            expected_hash: None,
            preview: false,
            operations: vec![
                AutonomousPatchOperation {
                    path: "alpha.txt".into(),
                    search: "two\n".into(),
                    replace: "TWO\n".into(),
                    replace_all: false,
                    expected_hash: None,
                },
                AutonomousPatchOperation {
                    path: "beta.txt".into(),
                    search: "blue\n".into(),
                    replace: "BLUE\n".into(),
                    replace_all: false,
                    expected_hash: None,
                },
            ],
        }
    }
}
