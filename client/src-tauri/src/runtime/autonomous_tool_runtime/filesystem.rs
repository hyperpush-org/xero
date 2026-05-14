use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, Metadata},
    io::{Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::SystemTime,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use globset::{GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use image::{GenericImageView, ImageFormat};
use regex::{Regex, RegexBuilder};
use serde_json::{json, Value as JsonValue};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::{
    repo_scope::{
        build_glob_matcher, normalize_glob_pattern, normalize_optional_relative_path,
        normalize_relative_path, path_to_forward_slash, scope_relative_match_path,
    },
    AutonomousCopyOmissions, AutonomousCopyOperation, AutonomousCopyOutput, AutonomousCopyRequest,
    AutonomousDeleteOutput, AutonomousDeleteRequest, AutonomousDirectoryDigestEntry,
    AutonomousDirectoryDigestHashMode, AutonomousDirectoryDigestOmissions,
    AutonomousDirectoryDigestOutput, AutonomousDirectoryDigestRequest, AutonomousEditOutput,
    AutonomousEditRequest, AutonomousFindMode, AutonomousFindOmissions, AutonomousFindOutput,
    AutonomousFindRequest, AutonomousFsTransactionAction, AutonomousFsTransactionOperation,
    AutonomousFsTransactionOperationResult, AutonomousFsTransactionOutput,
    AutonomousFsTransactionRequest, AutonomousFsTransactionRollbackAttempt,
    AutonomousFsTransactionRollbackStatus, AutonomousFsTransactionValidationSummary,
    AutonomousHashFileEntry, AutonomousHashOmissions, AutonomousHashOutput, AutonomousHashRequest,
    AutonomousLineEnding, AutonomousListEntry, AutonomousListOmissions, AutonomousListOutput,
    AutonomousListRequest, AutonomousListSortBy, AutonomousListSortDirection,
    AutonomousListTreeNode, AutonomousListTreeOmissions, AutonomousListTreeOutput,
    AutonomousListTreeRequest, AutonomousMkdirOutput, AutonomousMkdirRequest,
    AutonomousPatchChangedRange, AutonomousPatchFileOutput, AutonomousPatchGuardStatus,
    AutonomousPatchOperation, AutonomousPatchOutput, AutonomousPatchRequest,
    AutonomousReadContentKind, AutonomousReadLineHash, AutonomousReadManyError,
    AutonomousReadManyItem, AutonomousReadManyOutput, AutonomousReadManyRequest,
    AutonomousReadMode, AutonomousReadOutput, AutonomousReadRequest, AutonomousRenameOutput,
    AutonomousRenameRequest, AutonomousResultPageOutput, AutonomousResultPageRequest,
    AutonomousSearchContextLine, AutonomousSearchFileSummary, AutonomousSearchMatch,
    AutonomousSearchOmissions, AutonomousSearchOutput, AutonomousSearchRequest, AutonomousStatKind,
    AutonomousStatOutput, AutonomousStatPermissions, AutonomousStatRequest,
    AutonomousStructuredEditAction, AutonomousStructuredEditFormat,
    AutonomousStructuredEditFormattingMode, AutonomousStructuredEditOutput,
    AutonomousStructuredEditRequest, AutonomousToolOutput, AutonomousToolResult,
    AutonomousToolRuntime, AutonomousWriteOutput, AutonomousWriteRequest, AUTONOMOUS_TOOL_COPY,
    AUTONOMOUS_TOOL_DELETE, AUTONOMOUS_TOOL_DIRECTORY_DIGEST, AUTONOMOUS_TOOL_EDIT,
    AUTONOMOUS_TOOL_FIND, AUTONOMOUS_TOOL_FS_TRANSACTION, AUTONOMOUS_TOOL_HASH,
    AUTONOMOUS_TOOL_LIST, AUTONOMOUS_TOOL_LIST_TREE, AUTONOMOUS_TOOL_MKDIR, AUTONOMOUS_TOOL_PATCH,
    AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_READ_MANY, AUTONOMOUS_TOOL_RENAME,
    AUTONOMOUS_TOOL_RESULT_PAGE, AUTONOMOUS_TOOL_SEARCH, AUTONOMOUS_TOOL_STAT,
    AUTONOMOUS_TOOL_WRITE,
};

use crate::{
    commands::{
        validate_non_empty, CommandError, CommandErrorClass, CommandResult,
        RepositoryStatusEntryDto,
    },
    db::project_app_data_dir_for_repo,
    git::status,
};

const MAX_SEARCH_CONTEXT_LINES: usize = 5;
const MAX_BINARY_READ_BYTES: u64 = 20 * 1024 * 1024;
const MAX_BINARY_EXCERPT_BYTES: usize = 4 * 1024;
const IMAGE_PREVIEW_MAX_DIMENSION: u32 = 1024;
const READ_CURSOR_PREFIX: &str = "read:v1";
const SEARCH_CURSOR_PREFIX: &str = "search:v1";
const FIND_CURSOR_PREFIX: &str = "find:v1";
const LIST_CURSOR_PREFIX: &str = "list:v1";
const MAX_READ_AROUND_PATTERN_CHARS: usize = 256;
const GENERATED_TEXT_OMIT_MIN_BYTES: u64 = 16 * 1024;
const GENERATED_TEXT_MAX_LINES_FOR_OMIT: usize = 2;
const GENERATED_TEXT_LONG_LINE_CHARS: usize = 4 * 1024;
const MUTATION_DIFF_CONTEXT_LINES: usize = 3;
const MAX_MUTATION_DIFF_LINES: usize = 80;
const MAX_PATCH_OPERATIONS: usize = 64;
const MAX_PATCH_INLINE_DIFF_CHARS: usize = 8 * 1024;
const MAX_STAT_HASH_BYTES: u64 = 512 * 1024;
const DEFAULT_RESULT_PAGE_BYTES: usize = 16 * 1024;
const MAX_RESULT_PAGE_BYTES: usize = 64 * 1024;
const MAX_READ_MANY_PATHS: usize = 16;
const DEFAULT_READ_MANY_MAX_BYTES_PER_FILE: usize = 64 * 1024;
const DEFAULT_READ_MANY_MAX_TOTAL_BYTES: usize = 256 * 1024;
const DEFAULT_LIST_TREE_MAX_DEPTH: usize = 3;
const MAX_LIST_TREE_DEPTH: usize = 8;
const DEFAULT_LIST_TREE_MAX_ENTRIES: usize = 200;
const MAX_LIST_TREE_ENTRIES: usize = 1_000;
const DEFAULT_DIRECTORY_DIGEST_MAX_FILES: usize = 1_000;
const MAX_DIRECTORY_DIGEST_FILES: usize = 5_000;
const DEFAULT_HASH_MAX_FILES: usize = 1_000;
const MAX_HASH_FILES: usize = 5_000;
const MAX_HASH_INLINE_FILES: usize = 50;

enum ReadManyPathResult {
    Read {
        item: AutonomousReadManyItem,
        source_bytes: u64,
    },
    Omitted {
        item: AutonomousReadManyItem,
        source_bytes: u64,
    },
    Error(AutonomousReadManyItem),
}

struct ListTreeState<'a> {
    max_depth: usize,
    max_entries: usize,
    include_globs: Option<&'a GlobSet>,
    exclude_globs: Option<&'a GlobSet>,
    show_omitted: bool,
    entries_seen: usize,
    file_count: usize,
    directory_count: usize,
    symlink_count: usize,
    other_count: usize,
    omitted: AutonomousListTreeOmissions,
}

struct DirectoryDigestState<'a> {
    max_files: usize,
    hash_mode: AutonomousDirectoryDigestHashMode,
    include_globs: Option<&'a GlobSet>,
    exclude_globs: Option<&'a GlobSet>,
    files_seen: usize,
    file_count: usize,
    directory_count: usize,
    symlink_count: usize,
    other_count: usize,
    total_bytes: u64,
    omitted: AutonomousDirectoryDigestOmissions,
    manifest: Vec<AutonomousDirectoryDigestEntry>,
    digest_lines: Vec<String>,
}

struct HashState<'a> {
    max_files: usize,
    recursive: bool,
    include_globs: Option<&'a GlobSet>,
    exclude_globs: Option<&'a GlobSet>,
    files_seen: usize,
    total_bytes: u64,
    omitted: AutonomousHashOmissions,
    files: Vec<AutonomousHashFileEntry>,
    digest_lines: Vec<String>,
}

#[derive(Debug, Default)]
struct DeletePlan {
    file_count: usize,
    directory_count: usize,
    symlink_count: usize,
    other_count: usize,
    bytes_estimated: u64,
    digest_lines: Vec<String>,
}

impl DeletePlan {
    fn deleted_count(&self) -> usize {
        self.file_count + self.directory_count + self.symlink_count + self.other_count
    }

    fn digest(&self) -> String {
        let mut lines = self.digest_lines.clone();
        lines.sort();
        sha256_hex(lines.join("\n").as_bytes())
    }
}

#[derive(Debug)]
struct CopyPlan {
    source_kind: AutonomousStatKind,
    source_hash: Option<String>,
    source_digest: Option<String>,
    target_hash: Option<String>,
    copied_files: usize,
    copied_bytes: u64,
    created_directories: usize,
    overwritten: bool,
    omitted: CopyOmissions,
    operations: Vec<AutonomousCopyOperation>,
    digest_lines: Vec<String>,
}

#[derive(Debug, Default)]
struct CopyOmissions {
    symlinks: usize,
    existing_targets: usize,
    unsupported: usize,
}

impl From<CopyOmissions> for AutonomousCopyOmissions {
    fn from(value: CopyOmissions) -> Self {
        Self {
            symlinks: value.symlinks,
            existing_targets: value.existing_targets,
            unsupported: value.unsupported,
        }
    }
}

impl CopyPlan {
    fn new(source_kind: AutonomousStatKind) -> Self {
        Self {
            source_kind,
            source_hash: None,
            source_digest: None,
            target_hash: None,
            copied_files: 0,
            copied_bytes: 0,
            created_directories: 0,
            overwritten: false,
            omitted: CopyOmissions::default(),
            operations: Vec::new(),
            digest_lines: Vec::new(),
        }
    }

    fn digest(&self) -> String {
        let mut lines = self.digest_lines.clone();
        lines.sort();
        sha256_hex(lines.join("\n").as_bytes())
    }
}

const MAX_FS_TRANSACTION_OPERATIONS: usize = 32;
const MAX_STRUCTURED_EDIT_OPERATIONS: usize = 64;

#[derive(Debug, Clone)]
struct FsTransactionPlannedOperation {
    index: usize,
    id: Option<String>,
    action: AutonomousFsTransactionAction,
    request: FsTransactionApplyRequest,
    summary: String,
    changed_paths: Vec<String>,
    backup_paths: Vec<String>,
    diff: Option<String>,
    digest: Option<String>,
    source_digest: Option<String>,
}

#[derive(Debug, Clone)]
enum FsTransactionApplyRequest {
    Write(AutonomousWriteRequest),
    Edit(AutonomousEditRequest),
    Patch(AutonomousPatchRequest),
    Delete(AutonomousDeleteRequest),
    Rename(AutonomousRenameRequest),
    Copy(AutonomousCopyRequest),
    Mkdir(AutonomousMkdirRequest),
}

#[derive(Debug)]
struct FsTransactionBackupSet {
    _tempdir: tempfile::TempDir,
    entries: Vec<FsTransactionBackupEntry>,
}

#[derive(Debug)]
struct FsTransactionBackupEntry {
    display_path: String,
    resolved_path: PathBuf,
    backup_path: Option<PathBuf>,
}

#[derive(Debug)]
struct FsTransactionRollbackReport {
    attempts: Vec<AutonomousFsTransactionRollbackAttempt>,
}

impl FsTransactionRollbackReport {
    fn status(self) -> AutonomousFsTransactionRollbackStatus {
        let attempted = !self.attempts.is_empty();
        let succeeded = self.attempts.iter().all(|attempt| attempt.ok);
        AutonomousFsTransactionRollbackStatus {
            attempted,
            succeeded,
            attempts: self.attempts,
        }
    }
}

impl AutonomousToolRuntime {
    pub fn read(&self, request: AutonomousReadRequest) -> CommandResult<AutonomousToolResult> {
        self.read_with_approval(request, false)
    }

    pub fn read_many(
        &self,
        request: AutonomousReadManyRequest,
    ) -> CommandResult<AutonomousToolResult> {
        if request.paths.is_empty() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_many_paths_empty",
                "Xero requires read_many paths to contain at least one path.",
            ));
        }
        if request.paths.len() > MAX_READ_MANY_PATHS {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_many_paths_too_many",
                format!("Xero supports at most {MAX_READ_MANY_PATHS} paths per read_many call."),
            ));
        }
        for path in &request.paths {
            validate_non_empty(path, "paths[]")?;
        }

        let max_bytes_per_file = request
            .max_bytes_per_file
            .unwrap_or(DEFAULT_READ_MANY_MAX_BYTES_PER_FILE)
            .min(self.limits.max_text_file_bytes);
        if max_bytes_per_file == 0 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_many_file_byte_limit_invalid",
                "Xero requires maxBytesPerFile to be at least 1.",
            ));
        }
        let max_total_bytes = request
            .max_total_bytes
            .unwrap_or(DEFAULT_READ_MANY_MAX_TOTAL_BYTES)
            .min(self.limits.max_text_file_bytes);
        if max_total_bytes == 0 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_many_total_byte_limit_invalid",
                "Xero requires maxTotalBytes to be at least 1.",
            ));
        }

        let mut total_bytes = 0_u64;
        let mut omitted_bytes = 0_u64;
        let mut ok_files = 0_usize;
        let mut error_files = 0_usize;
        let mut omitted_files = 0_usize;
        let mut results = Vec::with_capacity(request.paths.len());

        for path in &request.paths {
            let item = self.read_many_path(
                path,
                &request,
                max_bytes_per_file,
                max_total_bytes,
                total_bytes,
            );
            match item {
                ReadManyPathResult::Read { item, source_bytes } => {
                    total_bytes = total_bytes.saturating_add(source_bytes);
                    ok_files += 1;
                    results.push(item);
                }
                ReadManyPathResult::Omitted { item, source_bytes } => {
                    omitted_bytes = omitted_bytes.saturating_add(source_bytes);
                    error_files += 1;
                    omitted_files += 1;
                    results.push(item);
                }
                ReadManyPathResult::Error(item) => {
                    error_files += 1;
                    results.push(item);
                }
            }
        }

        let truncated = omitted_files > 0;
        let summary = if error_files == 0 {
            format!("Read {ok_files} file(s) in one bounded batch.")
        } else {
            format!(
                "Read {ok_files} file(s) in one bounded batch; {error_files} file(s) returned per-file errors."
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_READ_MANY.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::ReadMany(AutonomousReadManyOutput {
                paths: request.paths,
                results,
                total_files: ok_files + error_files,
                ok_files,
                error_files,
                omitted_files,
                total_bytes,
                omitted_bytes,
                truncated,
                max_bytes_per_file,
                max_total_bytes,
            }),
        })
    }

    pub fn result_page(
        &self,
        request: AutonomousResultPageRequest,
    ) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.artifact_path, "artifactPath")?;
        let byte_offset = request.byte_offset.unwrap_or(0);
        let max_bytes = request.max_bytes.unwrap_or(DEFAULT_RESULT_PAGE_BYTES);
        if max_bytes == 0 || max_bytes > MAX_RESULT_PAGE_BYTES {
            return Err(CommandError::user_fixable(
                "autonomous_tool_result_page_max_bytes_invalid",
                format!(
                    "Xero supports result_page maxBytes between 1 and {MAX_RESULT_PAGE_BYTES}."
                ),
            ));
        }

        let requested_path = PathBuf::from(request.artifact_path.trim());
        if !requested_path.is_absolute() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_result_page_path_invalid",
                "Xero requires result_page artifactPath to be an absolute app-data artifact path returned by another tool.",
            ));
        }
        let artifact_root = project_app_data_dir_for_repo(&self.repo_root).join("tool-artifacts");
        let artifact_root = artifact_root.canonicalize().map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_result_page_artifact_root_missing",
                format!(
                    "Xero could not open the project artifact directory {}: {error}",
                    artifact_root.display()
                ),
            )
        })?;
        let artifact_path = requested_path.canonicalize().map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_result_page_artifact_missing",
                format!(
                    "Xero could not open result artifact {}: {error}",
                    requested_path.display()
                ),
            )
        })?;
        if !artifact_path.starts_with(&artifact_root) {
            return Err(CommandError::policy_denied(
                "Xero refused to read a result artifact outside this project's app-data tool-artifacts directory.",
            ));
        }

        let metadata = fs::metadata(&artifact_path).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_result_page_metadata_failed",
                format!(
                    "Xero could not inspect result artifact {}: {error}",
                    artifact_path.display()
                ),
            )
        })?;
        if !metadata.is_file() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_result_page_not_file",
                "Xero requires result_page artifactPath to point to a file artifact.",
            ));
        }
        let total_bytes = metadata.len();
        let mut file = File::open(&artifact_path).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_result_page_open_failed",
                format!(
                    "Xero could not open result artifact {}: {error}",
                    artifact_path.display()
                ),
            )
        })?;
        file.seek(SeekFrom::Start(byte_offset)).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_result_page_seek_failed",
                format!(
                    "Xero could not seek result artifact {}: {error}",
                    artifact_path.display()
                ),
            )
        })?;
        let mut buffer = vec![0_u8; max_bytes.saturating_add(1)];
        let read_bytes = file.read(&mut buffer).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_result_page_read_failed",
                format!(
                    "Xero could not read result artifact {}: {error}",
                    artifact_path.display()
                ),
            )
        })?;
        buffer.truncate(read_bytes.min(max_bytes));
        let truncated = byte_offset.saturating_add(buffer.len() as u64) < total_bytes;
        let next_byte_offset = truncated.then_some(byte_offset.saturating_add(buffer.len() as u64));
        let output = AutonomousResultPageOutput {
            artifact_path: artifact_path.display().to_string(),
            byte_offset,
            byte_count: buffer.len(),
            total_bytes,
            truncated,
            next_byte_offset,
            content: String::from_utf8_lossy(&buffer).into_owned(),
            encoding: "utf-8-lossy".into(),
        };
        let summary = format!(
            "Read {} byte(s) from result artifact `{}` at offset {}{}.",
            output.byte_count,
            output.artifact_path,
            output.byte_offset,
            output
                .next_byte_offset
                .map(|offset| format!("; next offset {offset}"))
                .unwrap_or_default()
        );
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_RESULT_PAGE.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::ResultPage(output),
        })
    }

    pub fn stat(&self, request: AutonomousStatRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let target = self.resolve_stat_target(&request.path)?;
        let display_path = target.display_path;
        let path = target.path;

        let link_metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if request.strict {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_stat_path_not_found",
                        format!(
                            "Xero could not find `{display_path}` inside the imported repository."
                        ),
                    ));
                }
                return Ok(AutonomousToolResult {
                    tool_name: AUTONOMOUS_TOOL_STAT.into(),
                    summary: format!("`{display_path}` does not exist."),
                    command_result: None,
                    output: AutonomousToolOutput::Stat(AutonomousStatOutput {
                        path: display_path,
                        path_kind: AutonomousStatKind::Missing,
                        exists: false,
                        size: None,
                        modified_at: None,
                        permissions: None,
                        symlink_target: None,
                        resolved_path: None,
                        sha256: None,
                        hash_omitted_reason: None,
                        follow_symlinks: request.follow_symlinks,
                        include_git_status: request.include_git_status,
                        git_status: Vec::new(),
                    }),
                });
            }
            Err(error) => {
                return Err(CommandError::retryable(
                    "autonomous_tool_stat_metadata_failed",
                    format!("Xero could not inspect {}: {error}", path.display()),
                ));
            }
        };

        let symlink_target = if link_metadata.file_type().is_symlink() {
            fs::read_link(&path)
                .ok()
                .map(|target| target.to_string_lossy().into_owned())
        } else {
            None
        };

        let (metadata, resolved_path) =
            if request.follow_symlinks && link_metadata.file_type().is_symlink() {
                let resolved = fs::canonicalize(&path).map_err(|error| {
                    CommandError::retryable(
                        "autonomous_tool_stat_symlink_resolve_failed",
                        format!("Xero could not resolve symlink {}: {error}", path.display()),
                    )
                })?;
                let repo_relative = self.repo_relative_path(&resolved)?;
                let metadata = fs::metadata(&resolved).map_err(|error| {
                    CommandError::retryable(
                        "autonomous_tool_stat_metadata_failed",
                        format!("Xero could not inspect {}: {error}", resolved.display()),
                    )
                })?;
                (metadata, Some(path_to_forward_slash(&repo_relative)))
            } else {
                (link_metadata, None)
            };

        let kind = stat_kind(&metadata);
        let (sha256, hash_omitted_reason) =
            self.stat_hash(&path, &metadata, kind, request.include_hash)?;
        let git_status = if request.include_git_status {
            self.git_status_for_path(&display_path, kind)?
        } else {
            Vec::new()
        };
        let summary = stat_summary(
            &display_path,
            kind,
            metadata.len(),
            request.include_hash,
            sha256.as_deref(),
            &hash_omitted_reason,
            request.include_git_status,
            git_status.len(),
        );

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_STAT.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Stat(AutonomousStatOutput {
                path: display_path,
                path_kind: kind,
                exists: true,
                size: stat_size(&metadata, kind),
                modified_at: metadata.modified().ok().and_then(system_time_to_rfc3339),
                permissions: Some(stat_permissions(&metadata)),
                symlink_target,
                resolved_path,
                sha256,
                hash_omitted_reason,
                follow_symlinks: request.follow_symlinks,
                include_git_status: request.include_git_status,
                git_status,
            }),
        })
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
        validate_read_request_shape(&request)?;
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
        let read_metadata = read_file_metadata(&metadata);

        let mode = request.mode.unwrap_or(AutonomousReadMode::Auto);
        if request.byte_offset.is_some() || request.byte_count.is_some() {
            return self.read_byte_range(request, target, read_metadata, metadata.len(), mode);
        }

        if metadata.len() > MAX_BINARY_READ_BYTES {
            return Ok(self.binary_metadata_result(
                target.display_path,
                read_metadata,
                metadata.len(),
                None,
                Vec::new(),
                true,
            ));
        }

        let bytes = read_file_bytes(&target.path, "autonomous_tool_read_failed")?;
        let is_supported_image = is_supported_image_path(&target.path);
        if matches!(mode, AutonomousReadMode::Image) {
            return self.image_result(target.display_path, read_metadata, bytes, true);
        }
        if matches!(mode, AutonomousReadMode::Auto) && is_supported_image {
            if let Ok(result) = self.image_result(
                target.display_path.clone(),
                read_metadata.clone(),
                bytes.clone(),
                false,
            ) {
                return Ok(result);
            }
        }

        if matches!(mode, AutonomousReadMode::BinaryMetadata) {
            return Ok(self.binary_metadata_result(
                target.display_path,
                read_metadata,
                metadata.len(),
                Some(sha256_hex(&bytes)),
                bytes,
                false,
            ));
        }

        match decode_text_bytes(bytes.clone()) {
            Ok(decoded) => self.text_read_result(
                request,
                target.display_path,
                read_metadata,
                decoded,
                metadata.len(),
            ),
            Err(_) if matches!(mode, AutonomousReadMode::Text) => Err(CommandError::user_fixable(
                "autonomous_tool_file_not_text",
                format!(
                    "Xero refused to read `{}` as text because it is not valid UTF-8 text.",
                    target.display_path
                ),
            )),
            Err(_) => {
                if !is_supported_image {
                    if let Ok(result) = self.image_result(
                        target.display_path.clone(),
                        read_metadata.clone(),
                        bytes.clone(),
                        false,
                    ) {
                        return Ok(result);
                    }
                }
                Ok(self.binary_metadata_result(
                    target.display_path,
                    read_metadata,
                    metadata.len(),
                    Some(sha256_hex(&bytes)),
                    bytes,
                    false,
                ))
            }
        }
    }

    fn read_many_path(
        &self,
        path: &str,
        request: &AutonomousReadManyRequest,
        max_bytes_per_file: usize,
        max_total_bytes: usize,
        current_total_bytes: u64,
    ) -> ReadManyPathResult {
        let relative_path = match normalize_relative_path(path, "paths[]") {
            Ok(path) => path,
            Err(error) => {
                return ReadManyPathResult::Error(read_many_error_item(
                    path.to_string(),
                    error,
                    None,
                ));
            }
        };
        let display_path = path_to_forward_slash(&relative_path);
        let resolved_path = match self.resolve_existing_path(&relative_path) {
            Ok(path) => path,
            Err(error) => {
                return ReadManyPathResult::Error(read_many_error_item(display_path, error, None));
            }
        };
        let metadata = match fs::metadata(&resolved_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                return ReadManyPathResult::Error(read_many_error_item(
                    display_path,
                    CommandError::retryable(
                        "autonomous_tool_read_many_metadata_failed",
                        format!(
                            "Xero could not inspect {}: {error}",
                            resolved_path.display()
                        ),
                    ),
                    None,
                ));
            }
        };
        let source_bytes = if metadata.is_file() {
            metadata.len()
        } else {
            0
        };
        if metadata.is_file() && source_bytes > max_bytes_per_file as u64 {
            return ReadManyPathResult::Omitted {
                item: read_many_error_item(
                    display_path,
                    CommandError::user_fixable(
                        "autonomous_tool_read_many_file_too_large",
                        format!(
                            "Xero skipped this file because it is {source_bytes} byte(s), above the {max_bytes_per_file} byte per-file read_many limit."
                        ),
                    ),
                    Some(source_bytes),
                ),
                source_bytes,
            };
        }
        if metadata.is_file()
            && current_total_bytes.saturating_add(source_bytes) > max_total_bytes as u64
        {
            return ReadManyPathResult::Omitted {
                item: read_many_error_item(
                    display_path,
                    CommandError::user_fixable(
                        "autonomous_tool_read_many_total_limit_reached",
                        format!(
                            "Xero skipped this file because reading it would exceed the {max_total_bytes} byte total read_many limit."
                        ),
                    ),
                    Some(source_bytes),
                ),
                source_bytes,
            };
        }

        let read_request = AutonomousReadRequest {
            path: display_path.clone(),
            system_path: false,
            mode: request.mode,
            start_line: request.start_line,
            line_count: request.line_count,
            cursor: None,
            around_pattern: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: request.include_line_hashes,
        };
        match self.read_with_approval(read_request, false) {
            Ok(result) => match result.output {
                AutonomousToolOutput::Read(read) => ReadManyPathResult::Read {
                    item: AutonomousReadManyItem {
                        path: display_path,
                        ok: true,
                        read: Some(read),
                        error: None,
                        omitted_bytes: None,
                    },
                    source_bytes,
                },
                _ => ReadManyPathResult::Error(read_many_error_item(
                    display_path,
                    CommandError::system_fault(
                        "autonomous_tool_read_many_unexpected_output",
                        "Xero read_many expected the read tool to return a read output.",
                    ),
                    None,
                )),
            },
            Err(error) => {
                ReadManyPathResult::Error(read_many_error_item(display_path, error, None))
            }
        }
    }

    fn text_read_result(
        &self,
        request: AutonomousReadRequest,
        display_path: String,
        metadata: ReadResultMetadata,
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
        let sha256 = decoded.raw_sha256;
        let start_line = read_start_line(&request, &text, total_lines, line_count, &sha256)?;
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

        let omitted_reason =
            if should_omit_generated_text(&request, &text, total_bytes, total_lines) {
                Some("minified_or_generated".to_string())
            } else {
                None
            };
        let current_cursor = (total_lines > 0).then(|| read_cursor(&sha256, start_line));
        if let Some(reason) = omitted_reason {
            let next_cursor = (total_lines > 0).then(|| read_cursor(&sha256, 1));
            let summary = format!(
                "Read metadata for `{display_path}` and omitted content because it appears to be {reason}."
            );
            return Ok(AutonomousToolResult {
                tool_name: AUTONOMOUS_TOOL_READ.into(),
                summary,
                command_result: None,
                output: AutonomousToolOutput::Read(AutonomousReadOutput {
                    path: display_path,
                    path_kind: metadata.path_kind,
                    size: metadata.size,
                    modified_at: metadata.modified_at,
                    start_line: 1,
                    line_count: 0,
                    total_lines,
                    truncated: true,
                    content: String::new(),
                    cursor: next_cursor.clone(),
                    next_cursor,
                    content_omitted_reason: Some(reason),
                    content_kind: Some(AutonomousReadContentKind::Text),
                    total_bytes: Some(total_bytes),
                    byte_offset: None,
                    byte_count: None,
                    sha256: Some(sha256),
                    line_hashes: Vec::new(),
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
            });
        }

        let (content, actual_line_count, truncated) = slice_lines(&text, start_line, line_count)?;
        let line_hashes = if request.include_line_hashes {
            line_hashes_for_content(&content, start_line)
        } else {
            Vec::new()
        };
        let next_cursor = if truncated {
            Some(read_cursor(&sha256, start_line + actual_line_count))
        } else {
            None
        };
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
                path_kind: metadata.path_kind,
                size: metadata.size,
                modified_at: metadata.modified_at,
                start_line,
                line_count: actual_line_count,
                total_lines,
                truncated,
                content,
                cursor: current_cursor,
                next_cursor,
                content_omitted_reason: None,
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

        let mut search_options =
            SearchOptions::from_request(&request, self.limits.max_search_results)?;
        let scope_string = scope
            .as_ref()
            .map(|path| path_to_forward_slash(path.as_path()));
        let cursor_fingerprint = search_cursor_fingerprint(
            &request.query,
            scope_string.as_deref(),
            &request.include_globs,
            &request.exclude_globs,
            &search_options,
        );
        if let Some(cursor) = request.cursor.as_deref() {
            let (cursor_fingerprint_value, offset) = parse_search_cursor(cursor)?;
            if cursor_fingerprint_value != cursor_fingerprint {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_search_cursor_mismatch",
                    "Xero refused the search cursor because it does not match the current search query and options.",
                ));
            }
            search_options.cursor_offset = offset;
        }
        let search_result =
            self.search_scope(&scope_path, request.query.as_str(), &search_options)?;

        let matched_files = search_result.matched_files.len();
        let next_cursor = if search_result.truncated {
            Some(search_cursor(
                &cursor_fingerprint,
                search_options
                    .cursor_offset
                    .saturating_add(search_result.returned_matches),
            ))
        } else {
            None
        };
        let files = search_file_summaries(search_result.files);
        let summary = if search_result.returned_matches == 0 {
            match scope_string.as_deref() {
                Some(scope) => format!("Found 0 matches for `{}` under `{scope}`.", request.query),
                None => format!("Found 0 matches for `{}` in the repository.", request.query),
            }
        } else if search_result.truncated {
            format!(
                "Found {} match(es) for `{}` across {} file(s); page truncated at {} returned match(es).",
                search_result.returned_matches,
                request.query,
                matched_files,
                search_options.max_results
            )
        } else {
            format!(
                "Found {} match(es) for `{}` across {} file(s).",
                search_result.returned_matches, request.query, matched_files
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_SEARCH.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Search(AutonomousSearchOutput {
                query: request.query,
                scope: scope_string,
                files,
                matches: search_result.matches,
                scanned_files: search_result.scanned_files,
                truncated: search_result.truncated,
                cursor: request.cursor,
                next_cursor,
                files_only: search_options.files_only,
                returned_matches: search_result.returned_matches,
                skipped_matches: search_options.cursor_offset,
                total_matches: Some(search_result.total_matches),
                matched_files: Some(matched_files),
                omissions: search_result.omissions,
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

        let mode = request.mode.unwrap_or(AutonomousFindMode::Glob);
        let normalized_pattern = normalize_find_pattern(&request.pattern, mode)?;
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
        let max_depth = request
            .max_depth
            .map(|depth| depth.min(MAX_LIST_TREE_DEPTH));
        let max_results = request
            .max_results
            .unwrap_or(self.limits.max_search_results)
            .min(self.limits.max_search_results);
        if max_results == 0 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_find_max_results_invalid",
                "Xero requires find maxResults to be at least 1.",
            ));
        }

        let scope_string = scope
            .as_ref()
            .map(|path| path_to_forward_slash(path.as_path()));
        let mut options = FindOptions {
            mode,
            pattern: normalized_pattern.clone(),
            glob_matcher: if mode == AutonomousFindMode::Glob {
                Some(build_glob_matcher(&normalized_pattern)?)
            } else {
                None
            },
            scope_relative,
            scope_is_file,
            max_depth,
            max_results,
            cursor_offset: 0,
        };
        let cursor_fingerprint = find_cursor_fingerprint(
            &normalized_pattern,
            mode,
            scope_string.as_deref(),
            max_depth,
            max_results,
        );
        if let Some(cursor) = request.cursor.as_deref() {
            let (fingerprint, offset) = parse_find_cursor(cursor)?;
            if fingerprint != cursor_fingerprint {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_find_cursor_mismatch",
                    "Xero refused the find cursor because it does not match the current find query and options.",
                ));
            }
            options.cursor_offset = offset;
        }

        let mut find_result = FindResult::default();
        self.find_scope(&scope_path, 0, &options, &mut find_result)?;

        let traversal_truncated = find_result.omissions.depth_limited_directories > 0;
        let output_truncated = find_result.truncated || traversal_truncated;
        let next_cursor = if find_result.truncated {
            Some(find_cursor(
                &cursor_fingerprint,
                options
                    .cursor_offset
                    .saturating_add(find_result.returned_matches),
            ))
        } else {
            None
        };
        let summary = if find_result.returned_matches == 0 {
            match scope_string.as_deref() {
                Some(scope) => {
                    format!("Found 0 path(s) matching `{normalized_pattern}` under `{scope}`.")
                }
                None => {
                    format!("Found 0 path(s) matching `{normalized_pattern}` in the repository.")
                }
            }
        } else if find_result.truncated {
            format!(
                "Found {} path(s) matching `{normalized_pattern}`; page truncated at {} returned path(s).",
                find_result.returned_matches,
                max_results
            )
        } else if traversal_truncated {
            format!(
                "Found {} path(s) matching `{normalized_pattern}`; traversal omitted {} directorie(s) at maxDepth.",
                find_result.returned_matches,
                find_result.omissions.depth_limited_directories
            )
        } else {
            format!(
                "Found {} path(s) matching `{normalized_pattern}`.",
                find_result.returned_matches
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_FIND.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Find(AutonomousFindOutput {
                pattern: normalized_pattern,
                mode,
                scope: scope_string,
                matches: find_result.matches,
                scanned_files: find_result.scanned_files,
                truncated: output_truncated,
                cursor: request.cursor,
                next_cursor,
                returned_matches: find_result.returned_matches,
                skipped_matches: options.cursor_offset,
                file_count: find_result.file_count,
                directory_count: find_result.directory_count,
                symlink_count: find_result.symlink_count,
                other_count: find_result.other_count,
                omissions: find_result.omissions,
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
                format!(
                    "Xero refused to apply the edit because the requested line range no longer matches the expected text. Current nearby lines and line hashes:\n{}",
                    edit_conflict_context(&existing, request.start_line, request.end_line)
                ),
            ));
        }

        let replacement =
            normalize_replacement_line_endings(&request.replacement, decoded.line_ending);
        let mut updated = String::with_capacity(existing.len() - current.len() + replacement.len());
        updated.push_str(&existing[..start_byte]);
        updated.push_str(&replacement);
        updated.push_str(&existing[end_byte..]);
        let updated_bytes = encode_text_bytes(&updated, decoded.has_bom);

        if !request.preview {
            fs::write(&resolved_path, &updated_bytes).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_edit_write_failed",
                    format!(
                        "Xero could not persist the edit to {}: {error}",
                        resolved_path.display()
                    ),
                )
            })?;
        }

        let display_path = path_to_forward_slash(&relative_path);
        let old_hash = sha256_hex(&decoded.raw_bytes);
        let new_hash = sha256_hex(&updated_bytes);
        let diff = compact_text_diff(&display_path, &existing, &updated);
        let verb = if request.preview {
            "Previewed"
        } else {
            "Updated"
        };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_EDIT.into(),
            summary: format!(
                "{verb} lines {}-{} in `{display_path}`.",
                request.start_line, request.end_line
            ),
            command_result: None,
            output: AutonomousToolOutput::Edit(AutonomousEditOutput {
                path: display_path,
                start_line: request.start_line,
                end_line: request.end_line,
                replacement_len: replacement.chars().count(),
                applied: !request.preview,
                preview: request.preview,
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
        let display_path = path_to_forward_slash(&relative_path);
        let target_path = self.repo_root.join(&relative_path);
        if fs::symlink_metadata(&target_path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_write_symlink_refused",
                "Xero refused to write through a symlink path.",
            ));
        }
        let resolved_path = self.resolve_writable_path(&relative_path)?;
        let existing_bytes = if resolved_path.exists() {
            if !resolved_path.is_file() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_write_file_required",
                    "Xero requires write targets that already exist to be regular files.",
                ));
            }
            if request.create_only {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_write_create_only_exists",
                    format!("Xero refused to write `{display_path}` because createOnly=true and the file already exists."),
                ));
            }
            match request.overwrite {
                Some(true) => {}
                Some(false) => {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_write_overwrite_refused",
                        format!(
                            "Xero refused to replace `{display_path}` because overwrite=false."
                        ),
                    ));
                }
                None => {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_write_overwrite_required",
                        format!("Xero requires overwrite=true before replacing existing file `{display_path}`."),
                    ));
                }
            }
            Some(read_file_bytes(
                &resolved_path,
                "autonomous_tool_write_read_failed",
            )?)
        } else {
            if let Some(expected_hash) = request.expected_hash.as_deref() {
                validate_sha256(expected_hash, "expectedHash")?;
                return Err(CommandError::user_fixable(
                    "autonomous_tool_write_expected_hash_missing_target",
                    format!("Xero refused to use expectedHash for `{display_path}` because the file does not exist yet."),
                ));
            }
            None
        };
        let created = existing_bytes.is_none();
        let new_bytes = request.content.as_bytes().to_vec();
        let new_hash = sha256_hex(&new_bytes);
        let old_hash = existing_bytes.as_deref().map(sha256_hex);
        if let Some(existing_bytes) = existing_bytes.as_deref() {
            validate_expected_hash_for_bytes(
                request.expected_hash.as_deref(),
                existing_bytes,
                "autonomous_tool_write_expected_hash_mismatch",
            )?;
        }
        let diff = if let Some(existing_bytes) = existing_bytes.as_ref() {
            let decoded = decode_text_bytes(existing_bytes.clone()).map_err(|_| {
                CommandError::user_fixable(
                    "autonomous_tool_write_existing_file_not_text",
                    format!("Xero refused to replace `{display_path}` because the existing file is not valid UTF-8 text."),
                )
            })?;
            Some(compact_text_diff(
                &display_path,
                &decoded.text,
                &request.content,
            ))
        } else {
            None
        };

        if !request.preview {
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

            fs::write(&resolved_path, &new_bytes).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_write_failed",
                    format!("Xero could not write {}: {error}", resolved_path.display()),
                )
            })?;
        }

        let verb = match (request.preview, created) {
            (true, true) => "Previewed create for",
            (true, false) => "Previewed replace for",
            (false, true) => "Created",
            (false, false) => "Replaced",
        };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_WRITE.into(),
            summary: format!("{verb} `{display_path}` with {} byte(s).", new_bytes.len()),
            command_result: None,
            output: AutonomousToolOutput::Write(AutonomousWriteOutput {
                path: display_path,
                created,
                bytes_written: new_bytes.len(),
                applied: !request.preview,
                preview: request.preview,
                old_hash,
                new_hash: Some(new_hash),
                diff,
                line_count: Some(count_lines(&request.content)),
                content_bytes: Some(new_bytes.len()),
            }),
        })
    }

    pub fn patch(&self, request: AutonomousPatchRequest) -> CommandResult<AutonomousToolResult> {
        let preview = request.preview;
        let operations = normalize_patch_operations(request)?;
        let planned_files = self.plan_patch_files(&operations)?;

        let rollback_status = if preview {
            patch_no_rollback_status()
        } else {
            self.write_patch_files_atomically(&planned_files)?
        };

        let files = planned_files
            .iter()
            .map(|file| AutonomousPatchFileOutput {
                path: file.display_path.clone(),
                replacements: file.replacements,
                bytes_written: file.updated_bytes.len(),
                old_hash: file.old_hash.clone(),
                new_hash: file.new_hash.clone(),
                diff: file.diff.clone(),
                guard_status: file.guard_status.clone(),
                changed_ranges: file.changed_ranges.clone(),
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
        let full_diff = files
            .iter()
            .map(|file| file.diff.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let artifact_path = if full_diff.len() > MAX_PATCH_INLINE_DIFF_CHARS {
            Some(self.write_patch_diff_artifact(&full_diff)?)
        } else {
            None
        };
        let diff_truncated = artifact_path.is_some();
        let diff = if files.is_empty() {
            None
        } else if diff_truncated {
            Some(truncate_chars(&full_diff, MAX_PATCH_INLINE_DIFF_CHARS))
        } else {
            Some(full_diff)
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
                rollback_status,
                old_hash,
                new_hash,
                diff,
                diff_truncated,
                artifact_path,
                line_ending,
                bom_preserved,
            }),
        })
    }

    pub fn copy(&self, request: AutonomousCopyRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.from, "from")?;
        validate_non_empty(&request.to, "to")?;
        let from_relative = normalize_relative_path(&request.from, "from")?;
        let to_relative = normalize_relative_path(&request.to, "to")?;
        let from_display = path_to_forward_slash(&from_relative);
        let to_display = path_to_forward_slash(&to_relative);
        let from_candidate = self.repo_root.join(&from_relative);
        if fs::symlink_metadata(&from_candidate)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_copy_symlink_refused",
                "Xero refused to copy through a symlink source path.",
            ));
        }
        let to_candidate = self.repo_root.join(&to_relative);
        if fs::symlink_metadata(&to_candidate)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_copy_target_symlink_refused",
                "Xero refused to replace a symlink copy target path.",
            ));
        }
        let from_resolved = self.resolve_existing_path(&from_relative)?;
        let to_resolved = self.resolve_writable_path(&to_relative)?;
        let source_metadata = fs::symlink_metadata(&from_resolved).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_copy_stat_failed",
                format!(
                    "Xero could not inspect copy source {}: {error}",
                    from_resolved.display()
                ),
            )
        })?;
        let source_kind = stat_kind(&source_metadata);
        if source_kind == AutonomousStatKind::Directory && !request.recursive {
            return Err(CommandError::user_fixable(
                "autonomous_tool_copy_recursive_required",
                "Xero requires recursive=true before copying a directory.",
            ));
        }
        if source_kind != AutonomousStatKind::File && request.expected_source_hash.is_some() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_copy_expected_source_hash_invalid",
                "Xero only accepts expectedSourceHash for file copy sources.",
            ));
        }
        if source_kind != AutonomousStatKind::Directory && request.expected_source_digest.is_some()
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_copy_expected_source_digest_invalid",
                "Xero only accepts expectedSourceDigest for directory copy sources.",
            ));
        }

        let mut plan = CopyPlan::new(source_kind);
        match source_kind {
            AutonomousStatKind::File => {
                self.plan_copy_file(
                    &from_resolved,
                    &to_resolved,
                    &from_display,
                    &to_display,
                    &request,
                    &mut plan,
                )?;
            }
            AutonomousStatKind::Directory => {
                if to_resolved.exists() {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_copy_directory_target_exists",
                        format!("Xero refused to copy directory `{from_display}` because target `{to_display}` already exists."),
                    ));
                }
                if request.expected_target_hash.is_some() {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_copy_expected_target_hash_invalid",
                        "Xero only accepts expectedTargetHash when overwriting a file target.",
                    ));
                }
                self.plan_copy_tree(
                    &from_resolved,
                    &to_resolved,
                    &from_display,
                    &to_display,
                    &mut plan,
                )?;
                let source_digest = plan.digest();
                if let Some(expected_digest) = request.expected_source_digest.as_deref() {
                    validate_sha256(expected_digest, "expectedSourceDigest")?;
                    if expected_digest.trim() != source_digest {
                        return Err(CommandError::user_fixable(
                            "autonomous_tool_copy_expected_source_digest_mismatch",
                            "Xero refused to copy the directory because expectedSourceDigest no longer matches.",
                        ));
                    }
                } else if !request.preview {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_copy_expected_source_digest_required",
                        "Xero requires expectedSourceDigest from a copy preview before recursively copying a directory.",
                    ));
                }
                plan.source_digest = Some(source_digest);
            }
            AutonomousStatKind::Symlink => {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_copy_symlink_refused",
                    "Xero refused to copy a symlink source.",
                ));
            }
            AutonomousStatKind::Other | AutonomousStatKind::Missing => {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_copy_source_unsupported",
                    "Xero only supports copying regular files and directories.",
                ));
            }
        }

        if !request.preview {
            self.apply_copy_plan(&plan)?;
        }

        let verb = if request.preview {
            "Previewed copy"
        } else {
            "Copied"
        };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_COPY.into(),
            summary: format!(
                "{verb} `{from_display}` to `{to_display}` with {} file(s) and {} byte(s).",
                plan.copied_files, plan.copied_bytes
            ),
            command_result: None,
            output: AutonomousToolOutput::Copy(AutonomousCopyOutput {
                from_path: from_display,
                to_path: to_display,
                recursive: request.recursive,
                applied: !request.preview,
                preview: request.preview,
                overwritten: plan.overwritten,
                copied_files: plan.copied_files,
                copied_bytes: plan.copied_bytes,
                created_directories: plan.created_directories,
                source_kind: plan.source_kind,
                source_hash: plan.source_hash,
                source_digest: plan.source_digest,
                target_hash: plan.target_hash,
                omitted: plan.omitted.into(),
                operations: plan.operations,
            }),
        })
    }

    fn plan_copy_file(
        &self,
        from: &Path,
        to: &Path,
        from_display: &str,
        to_display: &str,
        request: &AutonomousCopyRequest,
        plan: &mut CopyPlan,
    ) -> CommandResult<()> {
        let bytes = read_file_bytes(from, "autonomous_tool_copy_read_failed")?;
        let source_hash = sha256_hex(&bytes);
        if let Some(expected_hash) = request.expected_source_hash.as_deref() {
            validate_sha256(expected_hash, "expectedSourceHash")?;
            if expected_hash.trim() != source_hash {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_copy_expected_source_hash_mismatch",
                    "Xero refused to copy because expectedSourceHash no longer matches.",
                ));
            }
        }
        plan.source_hash = Some(source_hash);
        let overwritten = if to.exists() {
            if !to.is_file() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_copy_target_file_required",
                    "Xero only supports overwriting regular file copy targets.",
                ));
            }
            match request.overwrite {
                Some(true) => {
                    let Some(expected_target_hash) = request.expected_target_hash.as_deref() else {
                        return Err(CommandError::user_fixable(
                            "autonomous_tool_copy_expected_target_hash_required",
                            "Xero requires expectedTargetHash before overwriting a copy target.",
                        ));
                    };
                    let target_bytes =
                        read_file_bytes(to, "autonomous_tool_copy_target_read_failed")?;
                    let target_hash = sha256_hex(&target_bytes);
                    validate_sha256(expected_target_hash, "expectedTargetHash")?;
                    if expected_target_hash.trim() != target_hash {
                        return Err(CommandError::user_fixable(
                            "autonomous_tool_copy_expected_target_hash_mismatch",
                            "Xero refused to overwrite the copy target because expectedTargetHash no longer matches.",
                        ));
                    }
                    plan.target_hash = Some(target_hash);
                    true
                }
                Some(false) | None => {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_copy_target_exists",
                        format!(
                            "Xero refused to copy because target `{to_display}` already exists."
                        ),
                    ));
                }
            }
        } else {
            if request.expected_target_hash.is_some() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_copy_expected_target_hash_missing_target",
                    "Xero refused expectedTargetHash because the copy target does not exist.",
                ));
            }
            false
        };
        plan.overwritten = overwritten;
        plan.copied_files += 1;
        plan.copied_bytes = plan.copied_bytes.saturating_add(bytes.len() as u64);
        plan.operations.push(AutonomousCopyOperation {
            action: "copy_file".into(),
            from_path: Some(from_display.into()),
            to_path: to_display.into(),
            bytes: Some(bytes.len() as u64),
            overwritten,
        });
        Ok(())
    }

    fn plan_copy_tree(
        &self,
        from: &Path,
        to: &Path,
        from_display: &str,
        to_display: &str,
        plan: &mut CopyPlan,
    ) -> CommandResult<()> {
        let metadata = fs::symlink_metadata(from).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_copy_plan_failed",
                format!(
                    "Xero could not inspect copy source {}: {error}",
                    from.display()
                ),
            )
        })?;
        let kind = stat_kind(&metadata);
        match kind {
            AutonomousStatKind::Directory => {
                if to.exists() {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_copy_target_exists",
                        format!(
                            "Xero refused to copy because target `{to_display}` already exists."
                        ),
                    ));
                }
                plan.created_directories += 1;
                plan.operations.push(AutonomousCopyOperation {
                    action: "create_directory".into(),
                    from_path: Some(from_display.into()),
                    to_path: to_display.into(),
                    bytes: None,
                    overwritten: false,
                });
                plan.digest_lines.push(format!(
                    "dir|{from_display}|{}",
                    metadata
                        .modified()
                        .ok()
                        .and_then(system_time_to_rfc3339)
                        .unwrap_or_default()
                ));
                let mut entries = fs::read_dir(from)
                    .map_err(|error| {
                        CommandError::retryable(
                            "autonomous_tool_copy_plan_failed",
                            format!(
                                "Xero could not enumerate copy source {}: {error}",
                                from.display()
                            ),
                        )
                    })?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| {
                        CommandError::retryable(
                            "autonomous_tool_copy_plan_failed",
                            format!(
                                "Xero could not enumerate copy source {}: {error}",
                                from.display()
                            ),
                        )
                    })?;
                entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
                for entry in entries {
                    let child_from = entry.path();
                    let child_relative = self.repo_relative_path(&child_from)?;
                    let child_from_display = path_to_forward_slash(&child_relative);
                    let child_to = to.join(entry.file_name());
                    let child_to_relative = self.repo_relative_path(&child_to)?;
                    let child_to_display = path_to_forward_slash(&child_to_relative);
                    self.plan_copy_tree(
                        &child_from,
                        &child_to,
                        &child_from_display,
                        &child_to_display,
                        plan,
                    )?;
                }
            }
            AutonomousStatKind::File => {
                if to.exists() {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_copy_target_exists",
                        format!(
                            "Xero refused to copy because target `{to_display}` already exists."
                        ),
                    ));
                }
                let bytes = read_file_bytes(from, "autonomous_tool_copy_read_failed")?;
                let hash = sha256_hex(&bytes);
                plan.digest_lines
                    .push(format!("file|{from_display}|{}|{hash}", bytes.len()));
                plan.copied_files += 1;
                plan.copied_bytes = plan.copied_bytes.saturating_add(bytes.len() as u64);
                plan.operations.push(AutonomousCopyOperation {
                    action: "copy_file".into(),
                    from_path: Some(from_display.into()),
                    to_path: to_display.into(),
                    bytes: Some(bytes.len() as u64),
                    overwritten: false,
                });
            }
            AutonomousStatKind::Symlink => {
                plan.omitted.symlinks += 1;
            }
            AutonomousStatKind::Other | AutonomousStatKind::Missing => {
                plan.omitted.unsupported += 1;
            }
        }
        Ok(())
    }

    fn apply_copy_plan(&self, plan: &CopyPlan) -> CommandResult<()> {
        for operation in &plan.operations {
            let to_relative = normalize_relative_path(&operation.to_path, "toPath")?;
            let to_path = self.resolve_writable_path(&to_relative)?;
            match operation.action.as_str() {
                "create_directory" => {
                    if let Some(parent) = to_path.parent() {
                        fs::create_dir_all(parent).map_err(|error| {
                            CommandError::retryable(
                                "autonomous_tool_copy_prepare_directory_failed",
                                format!(
                                    "Xero could not prepare copy directory parent {}: {error}",
                                    parent.display()
                                ),
                            )
                        })?;
                    }
                    fs::create_dir(&to_path).map_err(|error| {
                        CommandError::retryable(
                            "autonomous_tool_copy_create_directory_failed",
                            format!(
                                "Xero could not create copy directory {}: {error}",
                                to_path.display()
                            ),
                        )
                    })?;
                }
                "copy_file" => {
                    let Some(from_path) = operation.from_path.as_deref() else {
                        return Err(CommandError::system_fault(
                            "autonomous_tool_copy_plan_invalid",
                            "Xero copy plan was missing a source path.",
                        ));
                    };
                    let from_relative = normalize_relative_path(from_path, "fromPath")?;
                    let from_resolved = self.resolve_existing_path(&from_relative)?;
                    if let Some(parent) = to_path.parent() {
                        fs::create_dir_all(parent).map_err(|error| {
                            CommandError::retryable(
                                "autonomous_tool_copy_prepare_failed",
                                format!(
                                    "Xero could not prepare copy target {}: {error}",
                                    parent.display()
                                ),
                            )
                        })?;
                    }
                    fs::copy(&from_resolved, &to_path).map_err(|error| {
                        CommandError::retryable(
                            "autonomous_tool_copy_failed",
                            format!(
                                "Xero could not copy {} to {}: {error}",
                                from_resolved.display(),
                                to_path.display()
                            ),
                        )
                    })?;
                    if let Ok(permissions) =
                        fs::metadata(&from_resolved).map(|metadata| metadata.permissions())
                    {
                        let _ = fs::set_permissions(&to_path, permissions);
                    }
                }
                _ => {
                    return Err(CommandError::system_fault(
                        "autonomous_tool_copy_plan_invalid",
                        "Xero copy plan contained an unknown operation.",
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn fs_transaction(
        &self,
        request: AutonomousFsTransactionRequest,
    ) -> CommandResult<AutonomousToolResult> {
        if request.operations.is_empty() || request.operations.len() > MAX_FS_TRANSACTION_OPERATIONS
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_fs_transaction_operation_count_invalid",
                format!(
                    "Xero requires fs_transaction operations to include 1..={MAX_FS_TRANSACTION_OPERATIONS} item(s)."
                ),
            ));
        }

        let mut planned = Vec::new();
        let mut validation_errors = Vec::new();
        for (index, operation) in request.operations.iter().enumerate() {
            match self.plan_fs_transaction_operation(index, operation, request.preview) {
                Ok(plan) => planned.push(plan),
                Err(error) => {
                    validation_errors.push(fs_transaction_error_result(
                        index,
                        operation.id.clone(),
                        operation.action,
                        "validation_failed",
                        error,
                    ));
                    if request.stop_on_first_error {
                        break;
                    }
                }
            }
        }

        if validation_errors.is_empty() {
            if let Some(error) = validate_fs_transaction_path_conflicts(&planned) {
                validation_errors.push(error);
            }
        }

        if !validation_errors.is_empty() {
            let output = fs_transaction_output(
                request.preview,
                false,
                request.operations.len(),
                planned.len(),
                validation_errors.clone(),
                planned.iter().map(fs_transaction_planned_result).collect(),
                Vec::new(),
                AutonomousFsTransactionRollbackStatus {
                    attempted: false,
                    succeeded: true,
                    attempts: Vec::new(),
                },
            );
            return Ok(AutonomousToolResult {
                tool_name: AUTONOMOUS_TOOL_FS_TRANSACTION.into(),
                summary: format!(
                    "fs_transaction validation failed for {} of {} operation(s).",
                    validation_errors.len(),
                    request.operations.len()
                ),
                command_result: None,
                output: AutonomousToolOutput::FsTransaction(output),
            });
        }

        let planned_results = planned
            .iter()
            .map(fs_transaction_planned_result)
            .collect::<Vec<_>>();
        if request.preview {
            let output = fs_transaction_output(
                true,
                false,
                request.operations.len(),
                planned.len(),
                Vec::new(),
                planned_results,
                Vec::new(),
                AutonomousFsTransactionRollbackStatus {
                    attempted: false,
                    succeeded: true,
                    attempts: Vec::new(),
                },
            );
            return Ok(AutonomousToolResult {
                tool_name: AUTONOMOUS_TOOL_FS_TRANSACTION.into(),
                summary: format!(
                    "Previewed fs_transaction with {} operation(s) and {} changed path(s).",
                    output.operation_count,
                    output.changed_paths.len()
                ),
                command_result: None,
                output: AutonomousToolOutput::FsTransaction(output),
            });
        }

        let backup = self.create_fs_transaction_backup(&planned)?;
        let mut results = Vec::new();
        for operation in &planned {
            match self.apply_fs_transaction_request(operation.request.clone()) {
                Ok(_) => results.push(fs_transaction_applied_result(operation)),
                Err(error) => {
                    results.push(fs_transaction_apply_error_result(operation, error));
                    let rollback_status = self.rollback_fs_transaction_backup(backup).status();
                    let output = fs_transaction_output(
                        false,
                        false,
                        request.operations.len(),
                        planned.len(),
                        Vec::new(),
                        planned_results,
                        results,
                        rollback_status,
                    );
                    return Ok(AutonomousToolResult {
                        tool_name: AUTONOMOUS_TOOL_FS_TRANSACTION.into(),
                        summary: format!(
                            "fs_transaction failed during operation #{} and recorded rollback attempts.",
                            operation.index + 1
                        ),
                        command_result: None,
                        output: AutonomousToolOutput::FsTransaction(output),
                    });
                }
            }
        }

        let output = fs_transaction_output(
            false,
            true,
            request.operations.len(),
            planned.len(),
            Vec::new(),
            planned_results,
            results,
            AutonomousFsTransactionRollbackStatus {
                attempted: false,
                succeeded: true,
                attempts: Vec::new(),
            },
        );
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_FS_TRANSACTION.into(),
            summary: format!(
                "Applied fs_transaction with {} operation(s) and {} changed path(s).",
                output.operation_count,
                output.changed_paths.len()
            ),
            command_result: None,
            output: AutonomousToolOutput::FsTransaction(output),
        })
    }

    fn plan_fs_transaction_operation(
        &self,
        index: usize,
        operation: &AutonomousFsTransactionOperation,
        transaction_preview: bool,
    ) -> CommandResult<FsTransactionPlannedOperation> {
        let request = fs_transaction_apply_request(operation, transaction_preview)?;
        let preview_request = fs_transaction_request_with_preview(request.clone(), true);
        let preview_result = self.apply_fs_transaction_request(preview_request)?;
        if !transaction_preview {
            validate_fs_transaction_apply_guards(operation, &preview_result.output)?;
        }
        let changed_paths = fs_transaction_changed_paths_from_output(&preview_result.output);
        let backup_paths = self.fs_transaction_backup_paths_from_output(&preview_result.output)?;
        Ok(FsTransactionPlannedOperation {
            index,
            id: operation.id.clone(),
            action: operation.action,
            request: fs_transaction_request_with_preview(request, false),
            summary: preview_result.summary,
            changed_paths,
            backup_paths,
            diff: fs_transaction_diff_from_output(&preview_result.output),
            digest: fs_transaction_digest_from_output(&preview_result.output),
            source_digest: fs_transaction_source_digest_from_output(&preview_result.output),
        })
    }

    fn apply_fs_transaction_request(
        &self,
        request: FsTransactionApplyRequest,
    ) -> CommandResult<AutonomousToolResult> {
        match request {
            FsTransactionApplyRequest::Write(request) => self.write(request),
            FsTransactionApplyRequest::Edit(request) => self.edit(request),
            FsTransactionApplyRequest::Patch(request) => self.patch(request),
            FsTransactionApplyRequest::Delete(request) => self.delete(request),
            FsTransactionApplyRequest::Rename(request) => self.rename(request),
            FsTransactionApplyRequest::Copy(request) => self.copy(request),
            FsTransactionApplyRequest::Mkdir(request) => self.mkdir(request),
        }
    }

    fn fs_transaction_backup_paths_from_output(
        &self,
        output: &AutonomousToolOutput,
    ) -> CommandResult<Vec<String>> {
        let mut paths = Vec::new();
        match output {
            AutonomousToolOutput::Write(output) => {
                if output.created {
                    paths.extend(self.missing_parent_paths_for_transaction(&output.path)?);
                }
                paths.push(output.path.clone());
            }
            AutonomousToolOutput::Rename(output) => {
                paths.push(output.from_path.clone());
                paths.extend(self.missing_parent_paths_for_transaction(&output.to_path)?);
                paths.push(output.to_path.clone());
            }
            AutonomousToolOutput::Copy(output) => {
                paths.extend(self.missing_parent_paths_for_transaction(&output.to_path)?);
                paths.push(output.to_path.clone());
            }
            AutonomousToolOutput::Mkdir(output) => {
                if output.created_paths.is_empty() {
                    paths.push(output.path.clone());
                } else {
                    paths.extend(output.created_paths.clone());
                }
            }
            _ => paths.extend(fs_transaction_changed_paths_from_output(output)),
        }
        Ok(deduplicate_preserving_order(paths))
    }

    fn missing_parent_paths_for_transaction(&self, path: &str) -> CommandResult<Vec<String>> {
        let relative = normalize_relative_path(path, "path")?;
        let resolved = self.resolve_writable_path(&relative)?;
        let mut missing = Vec::new();
        let Some(mut current) = resolved.parent() else {
            return Ok(Vec::new());
        };
        while !current.exists() {
            missing.push(current.to_path_buf());
            current = current.parent().ok_or_else(|| {
                CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    "Xero denied a transaction path that escaped the imported repository.",
                    false,
                )
            })?;
        }
        missing.reverse();
        missing
            .into_iter()
            .map(|path| {
                self.repo_relative_path(&path)
                    .map(|relative| path_to_forward_slash(&relative))
            })
            .collect()
    }

    fn create_fs_transaction_backup(
        &self,
        planned: &[FsTransactionPlannedOperation],
    ) -> CommandResult<FsTransactionBackupSet> {
        let tempdir = tempfile::Builder::new()
            .prefix("xero-fs-transaction-")
            .tempdir()
            .map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_fs_transaction_backup_failed",
                    format!("Xero could not create transaction backup storage: {error}"),
                )
            })?;
        let mut entries = Vec::new();
        let mut seen = BTreeSet::new();
        for display_path in planned
            .iter()
            .flat_map(|operation| operation.backup_paths.iter())
        {
            if !seen.insert(display_path.clone()) {
                continue;
            }
            let relative = normalize_relative_path(display_path, "path")?;
            let resolved_path = self.resolve_writable_path(&relative)?;
            let backup_path = if resolved_path.exists() {
                let backup_path = tempdir.path().join(format!("entry-{}", entries.len()));
                copy_fs_transaction_backup_entry(&resolved_path, &backup_path)?;
                Some(backup_path)
            } else {
                None
            };
            entries.push(FsTransactionBackupEntry {
                display_path: display_path.clone(),
                resolved_path,
                backup_path,
            });
        }
        Ok(FsTransactionBackupSet {
            _tempdir: tempdir,
            entries,
        })
    }

    fn rollback_fs_transaction_backup(
        &self,
        backup: FsTransactionBackupSet,
    ) -> FsTransactionRollbackReport {
        let mut attempts = Vec::new();
        for entry in backup.entries.into_iter().rev() {
            let result = restore_fs_transaction_backup_entry(&entry);
            match result {
                Ok(action) => attempts.push(AutonomousFsTransactionRollbackAttempt {
                    path: entry.display_path,
                    action,
                    ok: true,
                    error: None,
                }),
                Err(error) => attempts.push(AutonomousFsTransactionRollbackAttempt {
                    path: entry.display_path,
                    action: "restore".into(),
                    ok: false,
                    error: Some(error.into()),
                }),
            }
        }
        FsTransactionRollbackReport { attempts }
    }

    pub fn structured_edit(
        &self,
        request: AutonomousStructuredEditRequest,
        format: AutonomousStructuredEditFormat,
        tool_name: &'static str,
    ) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        if request.operations.is_empty()
            || request.operations.len() > MAX_STRUCTURED_EDIT_OPERATIONS
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_structured_edit_operation_count_invalid",
                format!(
                    "Xero requires structured edit operations to include 1..={MAX_STRUCTURED_EDIT_OPERATIONS} item(s)."
                ),
            ));
        }
        if request.formatting_mode != AutonomousStructuredEditFormattingMode::Normalize {
            return Err(CommandError::user_fixable(
                "autonomous_tool_structured_edit_formatting_mode_unsupported",
                "Xero currently supports formattingMode=normalize for structured edits.",
            ));
        }

        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_existing_path(&relative_path)?;
        let display_path = path_to_forward_slash(&relative_path);
        let decoded = self.read_decoded_text_file(&resolved_path)?;
        validate_expected_hash_for_bytes(
            request.expected_hash.as_deref(),
            &decoded.raw_bytes,
            "autonomous_tool_structured_edit_expected_hash_mismatch",
        )?;
        let original_text = decoded.text;
        let mut document = parse_structured_document(&original_text, format)?;
        let mut semantic_changes = Vec::new();
        for operation in &request.operations {
            apply_structured_edit_operation(&mut document, operation, &mut semantic_changes)?;
        }
        let updated_text = serialize_structured_document(&document, format)?;
        let updated = normalize_replacement_line_endings(&updated_text, decoded.line_ending);
        let updated_bytes = encode_text_bytes(&updated, decoded.has_bom);
        if !request.preview {
            fs::write(&resolved_path, &updated_bytes).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_structured_edit_write_failed",
                    format!(
                        "Xero could not persist the structured edit to {}: {error}",
                        resolved_path.display()
                    ),
                )
            })?;
        }

        let old_hash = sha256_hex(&decoded.raw_bytes);
        let new_hash = sha256_hex(&updated_bytes);
        let diff = compact_text_diff(&display_path, &original_text, &updated);
        let verb = if request.preview {
            "Previewed"
        } else {
            "Applied"
        };
        Ok(AutonomousToolResult {
            tool_name: tool_name.into(),
            summary: format!(
                "{verb} {format:?} structured edit for `{display_path}` with {} operation(s).",
                request.operations.len()
            ),
            command_result: None,
            output: match format {
                AutonomousStructuredEditFormat::Json => {
                    AutonomousToolOutput::JsonEdit(AutonomousStructuredEditOutput {
                        path: display_path,
                        format,
                        operations_applied: request.operations.len(),
                        applied: !request.preview,
                        preview: request.preview,
                        formatting_mode: request.formatting_mode,
                        old_hash,
                        new_hash,
                        diff: Some(diff),
                        line_ending: decoded.line_ending,
                        bom_preserved: decoded.has_bom,
                        semantic_changes,
                    })
                }
                AutonomousStructuredEditFormat::Toml => {
                    AutonomousToolOutput::TomlEdit(AutonomousStructuredEditOutput {
                        path: display_path,
                        format,
                        operations_applied: request.operations.len(),
                        applied: !request.preview,
                        preview: request.preview,
                        formatting_mode: request.formatting_mode,
                        old_hash,
                        new_hash,
                        diff: Some(diff),
                        line_ending: decoded.line_ending,
                        bom_preserved: decoded.has_bom,
                        semantic_changes,
                    })
                }
                AutonomousStructuredEditFormat::Yaml => {
                    AutonomousToolOutput::YamlEdit(AutonomousStructuredEditOutput {
                        path: display_path,
                        format,
                        operations_applied: request.operations.len(),
                        applied: !request.preview,
                        preview: request.preview,
                        formatting_mode: request.formatting_mode,
                        old_hash,
                        new_hash,
                        diff: Some(diff),
                        line_ending: decoded.line_ending,
                        bom_preserved: decoded.has_bom,
                        semantic_changes,
                    })
                }
            },
        })
    }

    pub fn delete(&self, request: AutonomousDeleteRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let target_path = self.repo_root.join(&relative_path);
        if fs::symlink_metadata(&target_path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_delete_symlink_refused",
                "Xero refused to delete through a symlink path.",
            ));
        }
        let resolved_path = self.resolve_existing_path(&relative_path)?;
        let display_path = path_to_forward_slash(&relative_path);
        let metadata = fs::symlink_metadata(&resolved_path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_delete_stat_failed",
                format!(
                    "Xero could not inspect {}: {error}",
                    resolved_path.display()
                ),
            )
        })?;
        let path_kind = stat_kind(&metadata);
        if resolved_path == self.repo_root {
            return Err(CommandError::user_fixable(
                "autonomous_tool_delete_repo_root_refused",
                "Xero refused to delete the imported repository root.",
            ));
        }
        if path_kind == AutonomousStatKind::Directory && !request.recursive {
            return Err(CommandError::user_fixable(
                "autonomous_tool_delete_recursive_required",
                "Xero requires recursive=true before deleting a directory.",
            ));
        }
        if path_kind == AutonomousStatKind::File {
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
        if path_kind != AutonomousStatKind::Directory && request.expected_digest.is_some() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_delete_expected_digest_invalid",
                "Xero only accepts expectedDigest for recursive directory deletes.",
            ));
        }

        let mut plan = DeletePlan::default();
        self.collect_delete_plan(&resolved_path, &display_path, &mut plan)?;
        let digest = if path_kind == AutonomousStatKind::Directory {
            Some(plan.digest())
        } else {
            None
        };
        if path_kind == AutonomousStatKind::Directory && request.recursive {
            match request.expected_digest.as_deref() {
                Some(expected_digest) => {
                    validate_sha256(expected_digest, "expectedDigest")?;
                    if digest.as_deref() != Some(expected_digest.trim()) {
                        return Err(CommandError::user_fixable(
                            "autonomous_tool_delete_expected_digest_mismatch",
                            format!("Xero refused to delete `{display_path}` because expectedDigest no longer matches the current directory plan digest."),
                        ));
                    }
                }
                None if !request.preview => {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_delete_expected_digest_required",
                        format!("Xero requires expectedDigest from a delete preview before recursively deleting `{display_path}`."),
                    ));
                }
                None => {}
            }
        }

        if !request.preview {
            if path_kind == AutonomousStatKind::Directory {
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
        }

        let verb = if request.preview {
            "Previewed delete for"
        } else {
            "Deleted"
        };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_DELETE.into(),
            summary: format!(
                "{verb} `{display_path}` with {} path(s) and {} byte(s).",
                plan.deleted_count(),
                plan.bytes_estimated
            ),
            command_result: None,
            output: AutonomousToolOutput::Delete(AutonomousDeleteOutput {
                path: display_path,
                recursive: request.recursive,
                existed: true,
                applied: !request.preview,
                preview: request.preview,
                deleted_count: plan.deleted_count(),
                file_count: plan.file_count,
                directory_count: plan.directory_count,
                symlink_count: plan.symlink_count,
                other_count: plan.other_count,
                bytes_estimated: plan.bytes_estimated,
                bytes_remaining: if request.preview {
                    plan.bytes_estimated
                } else {
                    0
                },
                digest,
            }),
        })
    }

    fn collect_delete_plan(
        &self,
        path: &Path,
        display_path: &str,
        plan: &mut DeletePlan,
    ) -> CommandResult<()> {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_delete_plan_failed",
                format!(
                    "Xero could not inspect delete target {}: {error}",
                    path.display()
                ),
            )
        })?;
        let path_kind = stat_kind(&metadata);
        match path_kind {
            AutonomousStatKind::File => {
                plan.file_count += 1;
                plan.bytes_estimated = plan.bytes_estimated.saturating_add(metadata.len());
            }
            AutonomousStatKind::Directory => {
                plan.directory_count += 1;
            }
            AutonomousStatKind::Symlink => {
                plan.symlink_count += 1;
            }
            AutonomousStatKind::Other | AutonomousStatKind::Missing => {
                plan.other_count += 1;
            }
        }
        let modified_at = metadata.modified().ok().and_then(system_time_to_rfc3339);
        plan.digest_lines.push(format!(
            "delete|{display_path}|{path_kind:?}|{}|{}",
            metadata.len(),
            modified_at.as_deref().unwrap_or("")
        ));
        if path_kind == AutonomousStatKind::Directory {
            let entries = fs::read_dir(path).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_delete_plan_failed",
                    format!(
                        "Xero could not enumerate delete target {}: {error}",
                        path.display()
                    ),
                )
            })?;
            let mut entries = entries.collect::<Result<Vec<_>, _>>().map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_delete_plan_failed",
                    format!(
                        "Xero could not enumerate delete target {}: {error}",
                        path.display()
                    ),
                )
            })?;
            entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
            for entry in entries {
                let child_path = entry.path();
                let repo_relative = self.repo_relative_path(&child_path)?;
                let child_display = path_to_forward_slash(&repo_relative);
                self.collect_delete_plan(&child_path, &child_display, plan)?;
            }
        }
        Ok(())
    }

    pub fn rename(&self, request: AutonomousRenameRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.from_path, "fromPath")?;
        validate_non_empty(&request.to_path, "toPath")?;
        let from_relative = normalize_relative_path(&request.from_path, "fromPath")?;
        let to_relative = normalize_relative_path(&request.to_path, "toPath")?;
        let from_candidate = self.repo_root.join(&from_relative);
        if fs::symlink_metadata(&from_candidate)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_rename_symlink_refused",
                "Xero refused to rename through a symlink source path.",
            ));
        }
        let to_candidate = self.repo_root.join(&to_relative);
        if fs::symlink_metadata(&to_candidate)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_rename_target_symlink_refused",
                "Xero refused to replace a symlink target path.",
            ));
        }
        let from_resolved = self.resolve_existing_path(&from_relative)?;
        let to_resolved = self.resolve_writable_path(&to_relative)?;
        let from_metadata = fs::symlink_metadata(&from_resolved).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_rename_stat_failed",
                format!(
                    "Xero could not inspect rename source {}: {error}",
                    from_resolved.display()
                ),
            )
        })?;
        let source_kind = stat_kind(&from_metadata);
        let source_bytes = stat_size(&from_metadata, source_kind);
        let source_hash = if source_kind == AutonomousStatKind::File {
            let existing = read_file_bytes(&from_resolved, "autonomous_tool_rename_read_failed")?;
            validate_expected_hash_for_bytes(
                request.expected_hash.as_deref(),
                &existing,
                "autonomous_tool_rename_expected_hash_mismatch",
            )?;
            Some(sha256_hex(&existing))
        } else {
            if request.expected_hash.is_some() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_rename_expected_hash_invalid",
                    "Xero only accepts expectedHash for file rename sources.",
                ));
            }
            None
        };
        let target_metadata = if to_resolved.exists() {
            Some(fs::symlink_metadata(&to_resolved).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_rename_target_stat_failed",
                    format!(
                        "Xero could not inspect rename target {}: {error}",
                        to_resolved.display()
                    ),
                )
            })?)
        } else {
            None
        };
        let target_kind = target_metadata.as_ref().map(stat_kind);
        let target_bytes = target_metadata
            .as_ref()
            .and_then(|metadata| target_kind.map(|kind| stat_size(metadata, kind)))
            .flatten();
        let target_hash = if target_kind == Some(AutonomousStatKind::File) {
            let bytes = read_file_bytes(&to_resolved, "autonomous_tool_rename_target_read_failed")?;
            Some(sha256_hex(&bytes))
        } else {
            None
        };
        let target_existed = target_metadata.is_some();
        let overwritten = if target_existed {
            match request.overwrite {
                Some(true) => {
                    if source_kind != AutonomousStatKind::File
                        || target_kind != Some(AutonomousStatKind::File)
                    {
                        return Err(CommandError::user_fixable(
                            "autonomous_tool_rename_overwrite_file_required",
                            "Xero only supports guarded rename overwrite when both source and target are files.",
                        ));
                    }
                    let Some(expected_target_hash) = request.expected_target_hash.as_deref() else {
                        return Err(CommandError::user_fixable(
                            "autonomous_tool_rename_expected_target_hash_required",
                            "Xero requires expectedTargetHash before overwriting a rename target.",
                        ));
                    };
                    validate_sha256(expected_target_hash, "expectedTargetHash")?;
                    if target_hash.as_deref() != Some(expected_target_hash.trim()) {
                        return Err(CommandError::user_fixable(
                            "autonomous_tool_rename_expected_target_hash_mismatch",
                            "Xero refused to overwrite the rename target because expectedTargetHash no longer matches.",
                        ));
                    }
                    true
                }
                Some(false) | None => {
                    return Err(CommandError::user_fixable(
                        "autonomous_tool_rename_target_exists",
                        format!(
                            "Xero refused to rename because `{}` already exists.",
                            path_to_forward_slash(&to_relative)
                        ),
                    ));
                }
            }
        } else {
            if request.expected_target_hash.is_some() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_rename_expected_target_hash_missing_target",
                    "Xero refused expectedTargetHash because the rename target does not exist.",
                ));
            }
            false
        };
        if !request.preview {
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
        }

        let from_path = path_to_forward_slash(&from_relative);
        let to_path = path_to_forward_slash(&to_relative);
        let verb = if request.preview {
            "Previewed rename"
        } else {
            "Renamed"
        };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_RENAME.into(),
            summary: format!("{verb} `{from_path}` to `{to_path}`."),
            command_result: None,
            output: AutonomousToolOutput::Rename(AutonomousRenameOutput {
                from_path,
                to_path,
                applied: !request.preview,
                preview: request.preview,
                overwritten,
                source_kind,
                source_bytes,
                source_hash,
                target_existed,
                target_kind,
                target_bytes,
                target_hash,
            }),
        })
    }

    pub fn mkdir(&self, request: AutonomousMkdirRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_writable_path(&relative_path)?;
        let parents = request.parents.unwrap_or(true);
        let exist_ok = request.exist_ok.unwrap_or(true);
        let display_path = path_to_forward_slash(&relative_path);
        if resolved_path.exists() {
            if !resolved_path.is_dir() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_mkdir_directory_required",
                    format!("Xero refused mkdir because `{display_path}` already exists but is not a directory."),
                ));
            }
            if !exist_ok {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_mkdir_exists",
                    format!("Xero refused mkdir because `{display_path}` already exists and existOk=false."),
                ));
            }
        }
        let created_paths = self.planned_mkdir_paths(&resolved_path)?;
        if !parents && created_paths.len() > 1 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_mkdir_parent_missing",
                "Xero requires parents=true before creating missing parent directories.",
            ));
        }
        let created = !created_paths.is_empty();
        if !request.preview && created {
            if parents {
                fs::create_dir_all(&resolved_path).map_err(|error| {
                    CommandError::retryable(
                        "autonomous_tool_mkdir_failed",
                        format!(
                            "Xero could not create directory {}: {error}",
                            resolved_path.display()
                        ),
                    )
                })?;
            } else {
                fs::create_dir(&resolved_path).map_err(|error| {
                    CommandError::retryable(
                        "autonomous_tool_mkdir_failed",
                        format!(
                            "Xero could not create directory {}: {error}",
                            resolved_path.display()
                        ),
                    )
                })?;
            }
        }
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_MKDIR.into(),
            summary: if request.preview {
                format!(
                    "Previewed mkdir for `{display_path}` with {} path(s) to create.",
                    created_paths.len()
                )
            } else if created {
                format!(
                    "Created directory `{display_path}` with {} path(s).",
                    created_paths.len()
                )
            } else {
                format!("Directory `{display_path}` already existed.")
            },
            command_result: None,
            output: AutonomousToolOutput::Mkdir(AutonomousMkdirOutput {
                path: display_path,
                created,
                applied: !request.preview,
                preview: request.preview,
                parents,
                exist_ok,
                created_paths,
            }),
        })
    }

    fn planned_mkdir_paths(&self, resolved_path: &Path) -> CommandResult<Vec<String>> {
        let mut missing = Vec::new();
        let mut current = resolved_path;
        while !current.exists() {
            missing.push(current.to_path_buf());
            current = current.parent().ok_or_else(|| {
                CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    "Xero denied a mkdir path that escaped the imported repository.",
                    false,
                )
            })?;
        }
        if !current.is_dir() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_mkdir_parent_not_directory",
                format!(
                    "Xero refused mkdir because `{}` is not a directory.",
                    current.display()
                ),
            ));
        }
        missing.reverse();
        missing
            .into_iter()
            .map(|path| {
                self.repo_relative_path(&path)
                    .map(|relative| path_to_forward_slash(&relative))
            })
            .collect()
    }

    pub fn list(&self, request: AutonomousListRequest) -> CommandResult<AutonomousToolResult> {
        let relative_path = normalize_optional_relative_path(request.path.as_deref(), "path")?;
        let scope = match relative_path.as_ref() {
            Some(path) => self.resolve_existing_path(path)?,
            None => self.repo_root.clone(),
        };
        let max_depth = request.max_depth.unwrap_or(2).min(MAX_LIST_TREE_DEPTH);
        let max_results = request
            .max_results
            .unwrap_or(self.limits.max_search_results)
            .min(MAX_LIST_TREE_ENTRIES);
        if max_results == 0 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_list_max_results_invalid",
                "Xero requires list maxResults to be at least 1.",
            ));
        }
        let sort_by = request.sort_by.unwrap_or(AutonomousListSortBy::Path);
        let sort_direction = request
            .sort_direction
            .unwrap_or(AutonomousListSortDirection::Asc);
        let mut options = ListOptions {
            max_depth,
            cursor_offset: 0,
        };
        let display_path = relative_path
            .as_ref()
            .map(|path| path_to_forward_slash(path))
            .unwrap_or_else(|| ".".into());
        let cursor_fingerprint = list_cursor_fingerprint(
            &display_path,
            max_depth,
            max_results,
            sort_by,
            sort_direction,
        );
        if let Some(cursor) = request.cursor.as_deref() {
            let (fingerprint, offset) = parse_list_cursor(cursor)?;
            if fingerprint != cursor_fingerprint {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_list_cursor_mismatch",
                    "Xero refused the list cursor because it does not match the current list scope and options.",
                ));
            }
            options.cursor_offset = offset;
        }
        let mut collection = ListCollection::default();
        let omit_scope_entry = scope.is_dir();
        self.collect_list_scope(&scope, 0, omit_scope_entry, &options, &mut collection)?;
        sort_list_candidates(&mut collection.candidates, sort_by, sort_direction);
        let total_candidates = collection.candidates.len();
        let entries = collection
            .candidates
            .into_iter()
            .skip(options.cursor_offset)
            .take(max_results)
            .map(|candidate| candidate.entry)
            .collect::<Vec<_>>();
        let next_offset = options.cursor_offset.saturating_add(entries.len());
        let page_truncated = next_offset < total_candidates;
        let truncated = page_truncated
            || collection.omitted.depth > 0
            || collection.omitted.entry_cap > 0
            || collection.omitted.ignored_directory > 0
            || collection.omitted.permission > 0;
        let next_cursor = if page_truncated {
            Some(list_cursor(&cursor_fingerprint, next_offset))
        } else {
            None
        };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_LIST.into(),
            summary: format!("Listed {} item(s) under `{display_path}`.", entries.len()),
            command_result: None,
            output: AutonomousToolOutput::List(AutonomousListOutput {
                path: display_path,
                entries,
                truncated,
                max_depth,
                max_results,
                sort_by,
                sort_direction,
                cursor: request.cursor,
                next_cursor,
                returned_entries: next_offset.saturating_sub(options.cursor_offset),
                skipped_entries: options.cursor_offset,
                file_count: collection.file_count,
                directory_count: collection.directory_count,
                symlink_count: collection.symlink_count,
                other_count: collection.other_count,
                omitted: collection.omitted,
            }),
        })
    }

    pub fn list_tree(
        &self,
        request: AutonomousListTreeRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let relative_path = normalize_optional_relative_path(request.path.as_deref(), "path")?;
        let scope = match relative_path.as_ref() {
            Some(path) => self.resolve_existing_path(path)?,
            None => self.repo_root.clone(),
        };
        let display_path = relative_path
            .as_ref()
            .map(|path| path_to_forward_slash(path.as_path()))
            .unwrap_or_else(|| ".".into());
        let max_depth = request
            .max_depth
            .unwrap_or(DEFAULT_LIST_TREE_MAX_DEPTH)
            .min(MAX_LIST_TREE_DEPTH);
        let max_entries = request
            .max_entries
            .unwrap_or(DEFAULT_LIST_TREE_MAX_ENTRIES)
            .min(MAX_LIST_TREE_ENTRIES);
        if max_entries == 0 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_list_tree_max_entries_invalid",
                "Xero requires list_tree maxEntries to be at least 1.",
            ));
        }
        let include_globs = build_search_globset(&request.include_globs, "includeGlobs")?;
        let exclude_globs = build_search_globset(&request.exclude_globs, "excludeGlobs")?;
        let git_status = if request.include_git_status {
            self.git_status_for_path(&display_path, AutonomousStatKind::Directory)?
        } else {
            Vec::new()
        };
        let mut state = ListTreeState {
            max_depth,
            max_entries,
            include_globs: include_globs.as_ref(),
            exclude_globs: exclude_globs.as_ref(),
            show_omitted: request.show_omitted,
            entries_seen: 0,
            file_count: 0,
            directory_count: 0,
            symlink_count: 0,
            other_count: 0,
            omitted: AutonomousListTreeOmissions::default(),
        };
        let root = self.list_tree_node(&scope, &display_path, 0, &mut state)?;
        let truncated = state.omitted.depth > 0
            || state.omitted.entry_cap > 0
            || state.omitted.ignored_directory > 0
            || state.omitted.permission > 0
            || state.omitted.filtered > 0;
        let summary = if truncated {
            format!(
                "Listed tree for `{display_path}` with {} file(s), {} directorie(s), and omissions.",
                state.file_count, state.directory_count
            )
        } else {
            format!(
                "Listed tree for `{display_path}` with {} file(s) and {} directorie(s).",
                state.file_count, state.directory_count
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_LIST_TREE.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::ListTree(AutonomousListTreeOutput {
                path: display_path,
                root,
                file_count: state.file_count,
                directory_count: state.directory_count,
                symlink_count: state.symlink_count,
                other_count: state.other_count,
                max_depth,
                max_entries,
                truncated,
                omitted: state.omitted,
                git_status,
            }),
        })
    }

    fn list_tree_node(
        &self,
        path: &Path,
        display_path: &str,
        depth: usize,
        state: &mut ListTreeState<'_>,
    ) -> CommandResult<AutonomousListTreeNode> {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_list_tree_metadata_failed",
                format!("Xero could not inspect {}: {error}", path.display()),
            )
        })?;
        let path_kind = stat_kind(&metadata);
        match path_kind {
            AutonomousStatKind::File => state.file_count += 1,
            AutonomousStatKind::Directory => state.directory_count += 1,
            AutonomousStatKind::Symlink => state.symlink_count += 1,
            AutonomousStatKind::Other | AutonomousStatKind::Missing => state.other_count += 1,
        }
        let mut node = AutonomousListTreeNode {
            name: list_tree_name(display_path),
            path: display_path.to_string(),
            path_kind,
            size: stat_size(&metadata, path_kind),
            children: Vec::new(),
        };

        if path_kind != AutonomousStatKind::Directory {
            return Ok(node);
        }
        if depth >= state.max_depth {
            if state.show_omitted {
                state.omitted.depth = state.omitted.depth.saturating_add(1);
            }
            return Ok(node);
        }

        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(_) => {
                if state.show_omitted {
                    state.omitted.permission = state.omitted.permission.saturating_add(1);
                }
                return Ok(node);
            }
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));

        for entry in entries {
            let child_path = entry.path();
            if self.should_skip_directory(&child_path) {
                if state.show_omitted {
                    state.omitted.ignored_directory =
                        state.omitted.ignored_directory.saturating_add(1);
                }
                continue;
            }
            let repo_relative = self.repo_relative_path(&child_path)?;
            let child_display = path_to_forward_slash(&repo_relative);
            if !list_tree_matches_filters(
                child_display.as_str(),
                entry
                    .file_type()
                    .ok()
                    .is_some_and(|file_type| file_type.is_dir()),
                state.include_globs,
                state.exclude_globs,
            ) {
                if state.show_omitted {
                    state.omitted.filtered = state.omitted.filtered.saturating_add(1);
                }
                continue;
            }
            if state.entries_seen >= state.max_entries {
                if state.show_omitted {
                    state.omitted.entry_cap = state.omitted.entry_cap.saturating_add(1);
                }
                continue;
            }
            state.entries_seen += 1;
            node.children.push(self.list_tree_node(
                &child_path,
                &child_display,
                depth + 1,
                state,
            )?);
        }

        Ok(node)
    }

    pub fn directory_digest(
        &self,
        request: AutonomousDirectoryDigestRequest,
    ) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let scope = self.resolve_existing_path(&relative_path)?;
        let display_path = path_to_forward_slash(&relative_path);
        let max_files = request
            .max_files
            .unwrap_or(DEFAULT_DIRECTORY_DIGEST_MAX_FILES)
            .min(MAX_DIRECTORY_DIGEST_FILES);
        if max_files == 0 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_directory_digest_max_files_invalid",
                "Xero requires directory_digest maxFiles to be at least 1.",
            ));
        }
        let hash_mode = request
            .hash_mode
            .unwrap_or(AutonomousDirectoryDigestHashMode::MetadataOnly);
        let include_globs = build_search_globset(&request.include_globs, "includeGlobs")?;
        let exclude_globs = build_search_globset(&request.exclude_globs, "excludeGlobs")?;
        let mut state = DirectoryDigestState {
            max_files,
            hash_mode,
            include_globs: include_globs.as_ref(),
            exclude_globs: exclude_globs.as_ref(),
            files_seen: 0,
            file_count: 0,
            directory_count: 0,
            symlink_count: 0,
            other_count: 0,
            total_bytes: 0,
            omitted: AutonomousDirectoryDigestOmissions::default(),
            manifest: Vec::new(),
            digest_lines: Vec::new(),
        };
        self.directory_digest_visit(&scope, &display_path, &mut state)?;
        if hash_mode == AutonomousDirectoryDigestHashMode::GitIndexAware {
            let scope_kind = fs::symlink_metadata(&scope)
                .map(|metadata| stat_kind(&metadata))
                .unwrap_or(AutonomousStatKind::Directory);
            for entry in self.git_status_for_path(&display_path, scope_kind)? {
                state.digest_lines.push(format!(
                    "git|{}|{:?}|{:?}|{}",
                    entry.path, entry.staged, entry.unstaged, entry.untracked
                ));
            }
        }
        state.digest_lines.sort();
        let digest = sha256_hex(state.digest_lines.join("\n").as_bytes());
        let truncated = state.omitted.max_files > 0
            || state.omitted.ignored_directory > 0
            || state.omitted.permission > 0
            || state.omitted.filtered > 0;
        let summary = if truncated {
            format!(
                "Computed {hash_mode:?} digest for `{display_path}` with {} file(s) and omissions.",
                state.file_count
            )
        } else {
            format!(
                "Computed {hash_mode:?} digest for `{display_path}` with {} file(s).",
                state.file_count
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_DIRECTORY_DIGEST.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::DirectoryDigest(AutonomousDirectoryDigestOutput {
                path: display_path,
                digest,
                algorithm: "xero.directory_digest.v1.sha256".into(),
                hash_mode,
                file_count: state.file_count,
                directory_count: state.directory_count,
                symlink_count: state.symlink_count,
                other_count: state.other_count,
                total_bytes: state.total_bytes,
                max_files,
                truncated,
                omitted: state.omitted,
                manifest: state.manifest,
            }),
        })
    }

    fn directory_digest_visit(
        &self,
        path: &Path,
        display_path: &str,
        state: &mut DirectoryDigestState<'_>,
    ) -> CommandResult<()> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(_) => {
                state.omitted.permission = state.omitted.permission.saturating_add(1);
                return Ok(());
            }
        };
        let path_kind = stat_kind(&metadata);
        match path_kind {
            AutonomousStatKind::File => {
                if !list_tree_matches_filters(
                    display_path,
                    false,
                    state.include_globs,
                    state.exclude_globs,
                ) {
                    state.omitted.filtered = state.omitted.filtered.saturating_add(1);
                    return Ok(());
                }
                if state.files_seen >= state.max_files {
                    state.omitted.max_files = state.omitted.max_files.saturating_add(1);
                    return Ok(());
                }
                state.files_seen += 1;
                state.file_count += 1;
                state.total_bytes = state.total_bytes.saturating_add(metadata.len());
            }
            AutonomousStatKind::Directory => {
                if self.should_skip_directory(path) {
                    state.omitted.ignored_directory =
                        state.omitted.ignored_directory.saturating_add(1);
                    return Ok(());
                }
                if let Some(globs) = state.exclude_globs {
                    if globs.is_match(display_path) {
                        state.omitted.filtered = state.omitted.filtered.saturating_add(1);
                        return Ok(());
                    }
                }
                state.directory_count += 1;
            }
            AutonomousStatKind::Symlink => state.symlink_count += 1,
            AutonomousStatKind::Other | AutonomousStatKind::Missing => state.other_count += 1,
        }

        let sha256 = if path_kind == AutonomousStatKind::File
            && matches!(
                state.hash_mode,
                AutonomousDirectoryDigestHashMode::ContentHash
                    | AutonomousDirectoryDigestHashMode::GitIndexAware
            ) {
            match fs::read(path) {
                Ok(bytes) => Some(sha256_hex(&bytes)),
                Err(_) => {
                    state.omitted.permission = state.omitted.permission.saturating_add(1);
                    None
                }
            }
        } else {
            None
        };
        let modified_at = metadata.modified().ok().and_then(system_time_to_rfc3339);
        state.manifest.push(AutonomousDirectoryDigestEntry {
            path: display_path.to_string(),
            path_kind,
            size: stat_size(&metadata, path_kind),
            modified_at: modified_at.clone(),
            sha256: sha256.clone(),
        });
        state.digest_lines.push(format!(
            "entry|{display_path}|{path_kind:?}|{}|{}|{}",
            metadata.len(),
            modified_at.as_deref().unwrap_or(""),
            sha256.as_deref().unwrap_or("")
        ));

        if path_kind == AutonomousStatKind::Directory {
            let entries = match fs::read_dir(path) {
                Ok(entries) => entries,
                Err(_) => {
                    state.omitted.permission = state.omitted.permission.saturating_add(1);
                    return Ok(());
                }
            };
            let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
            entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
            for entry in entries {
                let child_path = entry.path();
                let repo_relative = self.repo_relative_path(&child_path)?;
                let child_display = path_to_forward_slash(&repo_relative);
                self.directory_digest_visit(&child_path, &child_display, state)?;
            }
        }
        Ok(())
    }

    pub fn hash(&self, request: AutonomousHashRequest) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let resolved_path = self.resolve_existing_path(&relative_path)?;
        let metadata = fs::symlink_metadata(&resolved_path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_hash_metadata_failed",
                format!(
                    "Xero could not inspect {}: {error}",
                    resolved_path.display()
                ),
            )
        })?;
        let path_kind = stat_kind(&metadata);
        let display_path = path_to_forward_slash(&relative_path);
        let max_files = request.max_files.unwrap_or(DEFAULT_HASH_MAX_FILES);
        if max_files == 0 || max_files > MAX_HASH_FILES {
            return Err(CommandError::user_fixable(
                "autonomous_tool_hash_max_files_invalid",
                format!("Xero requires file_hash maxFiles to be between 1 and {MAX_HASH_FILES}."),
            ));
        }

        let include_globs = build_search_globset(&request.include_globs, "includeGlobs")?;
        let exclude_globs = build_search_globset(&request.exclude_globs, "excludeGlobs")?;
        let file_set_mode = path_kind == AutonomousStatKind::Directory
            || request.recursive
            || !request.include_globs.is_empty()
            || !request.exclude_globs.is_empty();
        let mut state = HashState {
            max_files,
            recursive: file_set_mode,
            include_globs: include_globs.as_ref(),
            exclude_globs: exclude_globs.as_ref(),
            files_seen: 0,
            total_bytes: 0,
            omitted: AutonomousHashOmissions::default(),
            files: Vec::new(),
            digest_lines: Vec::new(),
        };

        self.hash_visit(&resolved_path, &display_path, &mut state)?;
        state.digest_lines.sort();
        let sha256 = if !file_set_mode && state.files.len() == 1 {
            state.files[0].sha256.clone()
        } else {
            sha256_hex(state.digest_lines.join("\n").as_bytes())
        };
        let truncated = state.omitted.max_files > 0
            || state.omitted.ignored_directory > 0
            || state.omitted.permission > 0
            || state.omitted.unsupported > 0;
        let write_manifest =
            request.manifest || state.files.len() > MAX_HASH_INLINE_FILES || truncated;
        let artifact_path = if write_manifest && !state.files.is_empty() {
            Some(self.write_hash_manifest_artifact(
                &display_path,
                path_kind,
                &sha256,
                max_files,
                truncated,
                &state,
            )?)
        } else {
            None
        };
        let visible_files = state
            .files
            .iter()
            .take(MAX_HASH_INLINE_FILES)
            .cloned()
            .collect::<Vec<_>>();
        let file_count = state.files.len();
        let total_bytes = state.total_bytes;
        let mode = if file_set_mode {
            "file_set"
        } else {
            "single_file"
        };
        let summary = if file_set_mode {
            if artifact_path.is_some() {
                format!(
                    "Hashed `{display_path}` as a SHA-256 file set digest `{sha256}` across {file_count} file(s) with a manifest artifact."
                )
            } else {
                format!(
                    "Hashed `{display_path}` as a SHA-256 file set digest `{sha256}` across {file_count} file(s)."
                )
            }
        } else {
            format!("Hashed `{display_path}` as SHA-256 `{sha256}`.")
        };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_HASH.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Hash(AutonomousHashOutput {
                path: display_path,
                path_kind,
                algorithm: "sha256".into(),
                mode: mode.into(),
                sha256,
                bytes: total_bytes,
                file_count,
                max_files,
                truncated,
                files: visible_files,
                omitted: state.omitted,
                artifact_path,
            }),
        })
    }

    fn hash_visit(
        &self,
        path: &Path,
        display_path: &str,
        state: &mut HashState<'_>,
    ) -> CommandResult<()> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(_) => {
                state.omitted.permission = state.omitted.permission.saturating_add(1);
                return Ok(());
            }
        };
        let path_kind = stat_kind(&metadata);
        match path_kind {
            AutonomousStatKind::File => {
                if !list_tree_matches_filters(
                    display_path,
                    false,
                    state.include_globs,
                    state.exclude_globs,
                ) {
                    state.omitted.filtered = state.omitted.filtered.saturating_add(1);
                    return Ok(());
                }
                if state.files_seen >= state.max_files {
                    state.omitted.max_files = state.omitted.max_files.saturating_add(1);
                    return Ok(());
                }
                state.files_seen = state.files_seen.saturating_add(1);
                let bytes = match fs::read(path) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        state.omitted.permission = state.omitted.permission.saturating_add(1);
                        return Ok(());
                    }
                };
                let sha256 = sha256_hex(&bytes);
                let byte_count = bytes.len() as u64;
                state.total_bytes = state.total_bytes.saturating_add(byte_count);
                state
                    .digest_lines
                    .push(format!("file|{display_path}|{byte_count}|{sha256}"));
                state.files.push(AutonomousHashFileEntry {
                    path: display_path.to_owned(),
                    sha256,
                    bytes: byte_count,
                });
            }
            AutonomousStatKind::Directory => {
                if self.should_skip_directory(path) {
                    state.omitted.ignored_directory =
                        state.omitted.ignored_directory.saturating_add(1);
                    return Ok(());
                }
                if let Some(globs) = state.exclude_globs {
                    if globs.is_match(display_path) {
                        state.omitted.filtered = state.omitted.filtered.saturating_add(1);
                        return Ok(());
                    }
                }
                if !state.recursive {
                    state.omitted.unsupported = state.omitted.unsupported.saturating_add(1);
                    return Ok(());
                }
                let entries = match self
                    .read_sorted_directory_entries(path, "autonomous_tool_hash_read_dir_failed")
                {
                    Ok(entries) => entries,
                    Err(error) if error.class == CommandErrorClass::Retryable => {
                        state.omitted.permission = state.omitted.permission.saturating_add(1);
                        return Ok(());
                    }
                    Err(error) => return Err(error),
                };
                for entry in entries {
                    let child_path = entry.path();
                    let repo_relative = self.repo_relative_path(&child_path)?;
                    let child_display = path_to_forward_slash(&repo_relative);
                    self.hash_visit(&child_path, &child_display, state)?;
                }
            }
            AutonomousStatKind::Symlink
            | AutonomousStatKind::Other
            | AutonomousStatKind::Missing => {
                state.omitted.unsupported = state.omitted.unsupported.saturating_add(1);
            }
        }
        Ok(())
    }

    fn write_hash_manifest_artifact(
        &self,
        display_path: &str,
        path_kind: AutonomousStatKind,
        digest: &str,
        max_files: usize,
        truncated: bool,
        state: &HashState<'_>,
    ) -> CommandResult<String> {
        let artifact_dir = crate::db::project_app_data_dir_for_repo(&self.repo_root)
            .join("tool-artifacts")
            .join("file-hash");
        fs::create_dir_all(&artifact_dir).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_hash_manifest_artifact_failed",
                format!(
                    "Xero could not create file_hash manifest artifact directory {}: {error}",
                    artifact_dir.display()
                ),
            )
        })?;
        let artifact_path = artifact_dir.join(format!("manifest-{digest}.json"));
        let manifest = json!({
            "schemaVersion": "xero.file_hash_manifest.v1",
            "path": display_path,
            "pathKind": path_kind,
            "algorithm": "sha256",
            "digest": digest,
            "bytes": state.total_bytes,
            "fileCount": state.files.len(),
            "maxFiles": max_files,
            "truncated": truncated,
            "omitted": state.omitted,
            "files": state.files,
        });
        let bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_hash_manifest_encode_failed",
                format!("Xero could not encode file_hash manifest artifact: {error}"),
            )
        })?;
        fs::write(&artifact_path, bytes).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_hash_manifest_artifact_failed",
                format!(
                    "Xero could not write file_hash manifest artifact {}: {error}",
                    artifact_path.display()
                ),
            )
        })?;
        Ok(artifact_path.to_string_lossy().into_owned())
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
        let ignored_directories = Arc::new(AtomicUsize::new(0));
        let ignored_directories_for_filter = Arc::clone(&ignored_directories);

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
                let should_skip = entry
                    .file_type()
                    .is_some_and(|file_type| file_type.is_dir())
                    && should_skip_directory_for_root(&repo_root, entry.path());
                if should_skip {
                    ignored_directories_for_filter.fetch_add(1, Ordering::Relaxed);
                }
                !should_skip
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
                    result.omissions.filtered_files =
                        result.omissions.filtered_files.saturating_add(1);
                    continue;
                }
            }
            if let Some(globs) = exclude_globs {
                if globs.is_match(display_path.as_str()) {
                    result.omissions.filtered_files =
                        result.omissions.filtered_files.saturating_add(1);
                    continue;
                }
            }

            result.scanned_files = result.scanned_files.saturating_add(1);
            let decoded = match self.read_decoded_text_file(path) {
                Ok(decoded) => decoded,
                Err(error) if should_skip_search_file_error(&error) => {
                    record_search_file_omission(&mut result.omissions, &error);
                    continue;
                }
                Err(error) => return Err(error),
            };
            let lines = decoded.text.lines().collect::<Vec<_>>();
            let mut file_matched = false;

            for (line_index, line) in lines.iter().enumerate() {
                for found in regex.find_iter(line) {
                    result.total_matches = result.total_matches.saturating_add(1);
                    if result.total_matches <= options.cursor_offset {
                        file_matched = true;
                        continue;
                    }
                    if result.returned_matches >= options.max_results {
                        result.truncated = true;
                        break 'walk;
                    }

                    let line_number = line_index + 1;
                    let match_text = found.as_str();
                    matched_files.insert(display_path.clone());
                    let preview = build_search_preview(
                        line,
                        found.start(),
                        found.end(),
                        self.limits.max_search_preview_chars,
                    );
                    let summary = result.files.entry(display_path.clone()).or_default();
                    summary.match_count = summary.match_count.saturating_add(1);
                    summary.first_line.get_or_insert(line_number);
                    summary.first_preview.get_or_insert_with(|| preview.clone());
                    result.returned_matches = result.returned_matches.saturating_add(1);
                    if options.files_only {
                        file_matched = true;
                        continue;
                    }

                    result.matches.push(AutonomousSearchMatch {
                        path: display_path.clone(),
                        line: line_number,
                        column: utf8_char_col(line, found.start()),
                        preview,
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

        result.omissions.ignored_directories = ignored_directories.load(Ordering::Relaxed);
        result.matched_files = matched_files;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    fn find_scope(
        &self,
        scope: &Path,
        depth: usize,
        options: &FindOptions,
        result: &mut FindResult,
    ) -> CommandResult<()> {
        if result.truncated {
            return Ok(());
        }

        let metadata = fs::symlink_metadata(scope).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_find_metadata_failed",
                format!("Xero could not inspect {}: {error}", scope.display()),
            )
        })?;
        let path_kind = stat_kind(&metadata);
        if metadata.is_dir() {
            if self.should_skip_directory(scope) {
                result.omissions.ignored_directories =
                    result.omissions.ignored_directories.saturating_add(1);
                return Ok(());
            }
            self.record_find_candidate(scope, path_kind, options, result)?;
            if options.max_depth.is_some_and(|limit| depth >= limit) {
                result.omissions.depth_limited_directories =
                    result.omissions.depth_limited_directories.saturating_add(1);
                return Ok(());
            }
            let entries = match self
                .read_sorted_directory_entries(scope, "autonomous_tool_find_read_dir_failed")
            {
                Ok(entries) => entries,
                Err(error) if error.class == CommandErrorClass::Retryable => {
                    result.omissions.permission_denied =
                        result.omissions.permission_denied.saturating_add(1);
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            for entry in entries {
                self.find_scope(&entry.path(), depth.saturating_add(1), options, result)?;
                if result.truncated {
                    break;
                }
            }
            return Ok(());
        }

        if metadata.is_file() {
            result.scanned_files = result.scanned_files.saturating_add(1);
        }
        self.record_find_candidate(scope, path_kind, options, result)
    }

    fn record_find_candidate(
        &self,
        path: &Path,
        path_kind: AutonomousStatKind,
        options: &FindOptions,
        result: &mut FindResult,
    ) -> CommandResult<()> {
        if path_kind == AutonomousStatKind::Symlink {
            return Ok(());
        }
        let repo_relative = self.repo_relative_path(path)?;
        let display_path = path_to_forward_slash(&repo_relative);
        let candidate = scope_relative_match_path(
            repo_relative.as_path(),
            options.scope_relative.as_deref(),
            options.scope_is_file,
        )
        .map(|path| path_to_forward_slash(&path))?;
        if !find_candidate_matches(&display_path, &candidate, path, options) {
            return Ok(());
        }
        result.total_matches = result.total_matches.saturating_add(1);
        match path_kind {
            AutonomousStatKind::File => result.file_count = result.file_count.saturating_add(1),
            AutonomousStatKind::Directory => {
                result.directory_count = result.directory_count.saturating_add(1)
            }
            AutonomousStatKind::Symlink => {
                result.symlink_count = result.symlink_count.saturating_add(1)
            }
            AutonomousStatKind::Missing | AutonomousStatKind::Other => {
                result.other_count = result.other_count.saturating_add(1)
            }
        }
        if result.total_matches <= options.cursor_offset {
            return Ok(());
        }
        if result.returned_matches >= options.max_results {
            result.truncated = true;
            return Ok(());
        }
        result.matches.push(display_path);
        result.returned_matches = result.returned_matches.saturating_add(1);
        Ok(())
    }

    fn collect_list_scope(
        &self,
        path: &Path,
        depth: usize,
        is_root: bool,
        options: &ListOptions,
        collection: &mut ListCollection,
    ) -> CommandResult<()> {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_list_metadata_failed",
                format!("Xero could not inspect {}: {error}", path.display()),
            )
        })?;
        if metadata.is_dir() && self.should_skip_directory(path) {
            collection.omitted.ignored_directory =
                collection.omitted.ignored_directory.saturating_add(1);
            return Ok(());
        }

        let kind = stat_kind(&metadata);
        if !is_root {
            match kind {
                AutonomousStatKind::File => {
                    collection.file_count = collection.file_count.saturating_add(1)
                }
                AutonomousStatKind::Directory => {
                    collection.directory_count = collection.directory_count.saturating_add(1)
                }
                AutonomousStatKind::Symlink => {
                    collection.symlink_count = collection.symlink_count.saturating_add(1)
                }
                AutonomousStatKind::Missing | AutonomousStatKind::Other => {
                    collection.other_count = collection.other_count.saturating_add(1)
                }
            }
            if collection.candidates.len() >= MAX_LIST_TREE_ENTRIES {
                collection.omitted.entry_cap = collection.omitted.entry_cap.saturating_add(1);
            } else {
                let display_path = path_to_forward_slash(&self.repo_relative_path(path)?);
                let modified_at = metadata.modified().ok().and_then(system_time_to_rfc3339);
                collection.candidates.push(ListCandidate {
                    name: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| display_path.clone()),
                    modified_at_sort: modified_at.clone(),
                    kind,
                    entry: AutonomousListEntry {
                        path: display_path,
                        kind: stat_kind_label(kind).into(),
                        bytes: metadata.is_file().then_some(metadata.len()),
                        modified_at,
                    },
                });
            }
        }

        if metadata.is_file() {
            return Ok(());
        }
        if kind == AutonomousStatKind::Symlink {
            return Ok(());
        }
        if depth >= options.max_depth {
            if metadata.is_dir() {
                collection.omitted.depth = collection.omitted.depth.saturating_add(1);
            }
            return Ok(());
        }

        let entries = match self
            .read_sorted_directory_entries(path, "autonomous_tool_list_read_dir_failed")
        {
            Ok(entries) => entries,
            Err(error) if error.class == CommandErrorClass::Retryable => {
                collection.omitted.permission = collection.omitted.permission.saturating_add(1);
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        for entry in entries {
            self.collect_list_scope(
                &entry.path(),
                depth.saturating_add(1),
                false,
                options,
                collection,
            )?;
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

    fn resolve_stat_target(&self, path: &str) -> CommandResult<ReadTarget> {
        let relative_path = normalize_optional_relative_path(Some(path), "path")?;
        let Some(relative_path) = relative_path else {
            return Ok(ReadTarget {
                display_path: ".".into(),
                path: self.repo_root.clone(),
            });
        };

        let candidate = self.repo_root.join(&relative_path);
        match fs::symlink_metadata(&candidate) {
            Ok(_) => {
                let parent = relative_path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty());
                let parent_resolved = match parent {
                    Some(parent) => self.resolve_existing_path(parent)?,
                    None => self.repo_root.clone(),
                };
                let file_name = relative_path.file_name().ok_or_else(|| {
                    CommandError::new(
                        "autonomous_tool_path_denied",
                        CommandErrorClass::PolicyDenied,
                        "Xero denied a stat path that escaped the imported repository.",
                        false,
                    )
                })?;
                return Ok(ReadTarget {
                    display_path: path_to_forward_slash(&relative_path),
                    path: parent_resolved.join(file_name),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(CommandError::retryable(
                    "autonomous_tool_stat_metadata_failed",
                    format!("Xero could not inspect {}: {error}", candidate.display()),
                ));
            }
        }

        Ok(ReadTarget {
            display_path: path_to_forward_slash(&relative_path),
            path: self.resolve_writable_path(&relative_path)?,
        })
    }

    fn stat_hash(
        &self,
        path: &Path,
        metadata: &Metadata,
        kind: AutonomousStatKind,
        include_hash: bool,
    ) -> CommandResult<(Option<String>, Option<String>)> {
        if !include_hash {
            return Ok((None, None));
        }
        if kind != AutonomousStatKind::File {
            return Ok((
                None,
                Some("hashes are only available for regular files".into()),
            ));
        }
        if metadata.len() > MAX_STAT_HASH_BYTES {
            return Ok((
                None,
                Some(format!(
                    "file is {} byte(s), above the {} byte stat hash limit",
                    metadata.len(),
                    MAX_STAT_HASH_BYTES
                )),
            ));
        }

        let bytes = read_file_bytes(path, "autonomous_tool_stat_hash_read_failed")?;
        Ok((Some(sha256_hex(&bytes)), None))
    }

    fn git_status_for_path(
        &self,
        display_path: &str,
        kind: AutonomousStatKind,
    ) -> CommandResult<Vec<RepositoryStatusEntryDto>> {
        let response = status::load_repository_status_from_root(&self.repo_root)?;
        let prefix = if display_path == "." {
            None
        } else {
            Some(format!("{display_path}/"))
        };
        Ok(response
            .entries
            .into_iter()
            .filter(|entry| {
                entry.path == display_path
                    || (kind == AutonomousStatKind::Directory
                        && prefix
                            .as_ref()
                            .is_some_and(|prefix| entry.path.starts_with(prefix)))
            })
            .collect())
    }

    fn read_byte_range(
        &self,
        request: AutonomousReadRequest,
        target: ReadTarget,
        metadata: ReadResultMetadata,
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
                metadata,
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
                path_kind: metadata.path_kind,
                size: metadata.size,
                modified_at: metadata.modified_at,
                start_line: 1,
                line_count: total_lines,
                total_lines,
                truncated,
                content: decoded.text,
                cursor: None,
                next_cursor: None,
                content_omitted_reason: None,
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
        metadata: ReadResultMetadata,
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
                path_kind: metadata.path_kind,
                size: metadata.size,
                modified_at: metadata.modified_at,
                start_line: 0,
                line_count: 0,
                total_lines: 0,
                truncated: false,
                content: String::new(),
                cursor: None,
                next_cursor: None,
                content_omitted_reason: None,
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
        metadata: ReadResultMetadata,
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
                path_kind: metadata.path_kind,
                size: metadata.size,
                modified_at: metadata.modified_at,
                start_line: 0,
                line_count: 0,
                total_lines: 0,
                truncated,
                content: String::new(),
                cursor: None,
                next_cursor: None,
                content_omitted_reason: None,
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
            let expected_hashes = group
                .operations
                .iter()
                .filter_map(|operation| operation.expected_hash.as_ref())
                .map(|hash| hash.trim().to_owned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();

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
            let changed_ranges = changed_line_ranges(&original_text, &updated);
            planned_files.push(PlannedPatchFile {
                display_path,
                resolved_path,
                original_bytes: decoded.raw_bytes,
                updated_bytes,
                replacements,
                guard_status: AutonomousPatchGuardStatus {
                    expected_hashes,
                    current_hash: decoded.raw_sha256.clone(),
                    matched: true,
                },
                old_hash: decoded.raw_sha256,
                new_hash,
                diff,
                changed_ranges,
                line_ending: decoded.line_ending,
                bom_preserved: decoded.has_bom,
            });
        }

        Ok(planned_files)
    }

    fn write_patch_files_atomically(
        &self,
        planned_files: &[PlannedPatchFile],
    ) -> CommandResult<AutonomousFsTransactionRollbackStatus> {
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
        Ok(patch_no_rollback_status())
    }

    fn write_patch_diff_artifact(&self, diff: &str) -> CommandResult<String> {
        let digest = sha256_hex(diff.as_bytes());
        let artifact_dir = crate::db::project_app_data_dir_for_repo(&self.repo_root)
            .join("tool-artifacts")
            .join("patch");
        fs::create_dir_all(&artifact_dir).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_patch_artifact_failed",
                format!(
                    "Xero could not create patch artifact directory {}: {error}",
                    artifact_dir.display()
                ),
            )
        })?;
        let artifact_path = artifact_dir.join(format!("diff-{digest}.diff"));
        fs::write(&artifact_path, diff).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_patch_artifact_failed",
                format!(
                    "Xero could not write patch artifact {}: {error}",
                    artifact_path.display()
                ),
            )
        })?;
        Ok(artifact_path.to_string_lossy().into_owned())
    }
}

fn fs_transaction_apply_request(
    operation: &AutonomousFsTransactionOperation,
    transaction_preview: bool,
) -> CommandResult<FsTransactionApplyRequest> {
    match operation.action {
        AutonomousFsTransactionAction::CreateFile => {
            let path = required_transaction_field(operation.path.as_deref(), "path")?;
            let content = required_transaction_field(operation.content.as_deref(), "content")?;
            Ok(FsTransactionApplyRequest::Write(AutonomousWriteRequest {
                path,
                content,
                expected_hash: None,
                create_only: true,
                overwrite: Some(false),
                preview: transaction_preview,
            }))
        }
        AutonomousFsTransactionAction::ReplaceFile => {
            let path = required_transaction_field(operation.path.as_deref(), "path")?;
            let content = required_transaction_field(operation.content.as_deref(), "content")?;
            let expected_hash = Some(required_transaction_field(
                operation.expected_hash.as_deref(),
                "expectedHash",
            )?);
            Ok(FsTransactionApplyRequest::Write(AutonomousWriteRequest {
                path,
                content,
                expected_hash,
                create_only: false,
                overwrite: Some(true),
                preview: transaction_preview,
            }))
        }
        AutonomousFsTransactionAction::EditFile => {
            let path = required_transaction_field(operation.path.as_deref(), "path")?;
            if let Some(search) = operation.search.as_ref() {
                let replace = operation
                    .replace
                    .clone()
                    .or_else(|| operation.replacement.clone())
                    .unwrap_or_default();
                Ok(FsTransactionApplyRequest::Patch(AutonomousPatchRequest {
                    path: None,
                    search: None,
                    replace: None,
                    replace_all: false,
                    expected_hash: None,
                    preview: transaction_preview,
                    operations: vec![AutonomousPatchOperation {
                        path,
                        search: search.clone(),
                        replace,
                        replace_all: operation.replace_all,
                        expected_hash: operation.expected_hash.clone(),
                    }],
                }))
            } else {
                Ok(FsTransactionApplyRequest::Edit(AutonomousEditRequest {
                    path,
                    start_line: required_transaction_usize(operation.start_line, "startLine")?,
                    end_line: required_transaction_usize(operation.end_line, "endLine")?,
                    expected: required_transaction_field(
                        operation.expected.as_deref(),
                        "expected",
                    )?,
                    replacement: required_transaction_field(
                        operation.replacement.as_deref(),
                        "replacement",
                    )?,
                    expected_hash: operation.expected_hash.clone(),
                    start_line_hash: None,
                    end_line_hash: None,
                    preview: transaction_preview,
                }))
            }
        }
        AutonomousFsTransactionAction::DeleteFile => {
            let path = required_transaction_field(operation.path.as_deref(), "path")?;
            Ok(FsTransactionApplyRequest::Delete(AutonomousDeleteRequest {
                path,
                recursive: false,
                expected_hash: operation.expected_hash.clone(),
                expected_digest: None,
                preview: transaction_preview,
            }))
        }
        AutonomousFsTransactionAction::DeleteDirectory => {
            let path = required_transaction_field(operation.path.as_deref(), "path")?;
            Ok(FsTransactionApplyRequest::Delete(AutonomousDeleteRequest {
                path,
                recursive: true,
                expected_hash: None,
                expected_digest: operation.expected_digest.clone(),
                preview: transaction_preview,
            }))
        }
        AutonomousFsTransactionAction::Rename => {
            let from_path = required_transaction_field(
                operation.from_path.as_deref().or(operation.from.as_deref()),
                "fromPath",
            )?;
            let to_path = required_transaction_field(
                operation.to_path.as_deref().or(operation.to.as_deref()),
                "toPath",
            )?;
            Ok(FsTransactionApplyRequest::Rename(AutonomousRenameRequest {
                from_path,
                to_path,
                expected_hash: operation.expected_hash.clone(),
                expected_target_hash: operation.expected_target_hash.clone(),
                overwrite: operation.overwrite,
                preview: transaction_preview,
            }))
        }
        AutonomousFsTransactionAction::Copy => {
            let from = required_transaction_field(operation.from.as_deref(), "from")?;
            let to = required_transaction_field(operation.to.as_deref(), "to")?;
            Ok(FsTransactionApplyRequest::Copy(AutonomousCopyRequest {
                from,
                to,
                recursive: operation.recursive,
                expected_source_hash: operation.expected_source_hash.clone(),
                expected_source_digest: operation.expected_source_digest.clone(),
                overwrite: operation.overwrite,
                expected_target_hash: operation.expected_target_hash.clone(),
                preview: transaction_preview,
            }))
        }
        AutonomousFsTransactionAction::Mkdir => {
            let path = required_transaction_field(operation.path.as_deref(), "path")?;
            Ok(FsTransactionApplyRequest::Mkdir(AutonomousMkdirRequest {
                path,
                parents: operation.parents,
                exist_ok: operation.exist_ok,
                preview: transaction_preview,
            }))
        }
    }
}

fn fs_transaction_request_with_preview(
    mut request: FsTransactionApplyRequest,
    preview: bool,
) -> FsTransactionApplyRequest {
    match &mut request {
        FsTransactionApplyRequest::Write(request) => request.preview = preview,
        FsTransactionApplyRequest::Edit(request) => request.preview = preview,
        FsTransactionApplyRequest::Patch(request) => request.preview = preview,
        FsTransactionApplyRequest::Delete(request) => request.preview = preview,
        FsTransactionApplyRequest::Rename(request) => request.preview = preview,
        FsTransactionApplyRequest::Copy(request) => request.preview = preview,
        FsTransactionApplyRequest::Mkdir(request) => request.preview = preview,
    }
    request
}

fn validate_fs_transaction_apply_guards(
    operation: &AutonomousFsTransactionOperation,
    output: &AutonomousToolOutput,
) -> CommandResult<()> {
    match (&operation.action, output) {
        (
            AutonomousFsTransactionAction::DeleteDirectory,
            AutonomousToolOutput::Delete(output),
        ) if output.recursive && operation.expected_digest.is_none() => {
            Err(CommandError::user_fixable(
                "autonomous_tool_fs_transaction_expected_digest_required",
                "Xero requires expectedDigest from a preview before applying a transaction directory delete.",
            ))
        }
        (AutonomousFsTransactionAction::Copy, AutonomousToolOutput::Copy(output))
            if output.source_kind == AutonomousStatKind::Directory
                && operation.expected_source_digest.is_none() =>
        {
            Err(CommandError::user_fixable(
                "autonomous_tool_fs_transaction_expected_source_digest_required",
                "Xero requires expectedSourceDigest from a preview before applying a transaction directory copy.",
            ))
        }
        _ => Ok(()),
    }
}

fn required_transaction_field(value: Option<&str>, field: &'static str) -> CommandResult<String> {
    let value = value.map(str::to_owned).unwrap_or_default();
    validate_non_empty(&value, field)?;
    Ok(value)
}

fn required_transaction_usize(value: Option<usize>, field: &'static str) -> CommandResult<usize> {
    value.ok_or_else(|| {
        CommandError::user_fixable(
            "autonomous_tool_fs_transaction_field_required",
            format!("Xero requires fs_transaction operation field `{field}` for this action."),
        )
    })
}

fn validate_fs_transaction_path_conflicts(
    planned: &[FsTransactionPlannedOperation],
) -> Option<AutonomousFsTransactionOperationResult> {
    let mut owners = BTreeMap::<String, usize>::new();
    for operation in planned {
        for path in &operation.backup_paths {
            if let Some(previous) = owners.insert(path.clone(), operation.index) {
                return Some(fs_transaction_error_result(
                    operation.index,
                    operation.id.clone(),
                    operation.action,
                    "validation_failed",
                    CommandError::user_fixable(
                        "autonomous_tool_fs_transaction_path_conflict",
                        format!(
                            "Xero refused fs_transaction because operation #{} and #{} both modify `{path}`.",
                            previous + 1,
                            operation.index + 1
                        ),
                    ),
                ));
            }
        }
    }
    None
}

fn fs_transaction_changed_paths_from_output(output: &AutonomousToolOutput) -> Vec<String> {
    let paths = match output {
        AutonomousToolOutput::Write(output) => vec![output.path.clone()],
        AutonomousToolOutput::Edit(output) => vec![output.path.clone()],
        AutonomousToolOutput::Patch(output) => output
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>(),
        AutonomousToolOutput::Delete(output) => vec![output.path.clone()],
        AutonomousToolOutput::Rename(output) => {
            vec![output.from_path.clone(), output.to_path.clone()]
        }
        AutonomousToolOutput::Copy(output) => output
            .operations
            .iter()
            .map(|operation| operation.to_path.clone())
            .chain(std::iter::once(output.to_path.clone()))
            .collect(),
        AutonomousToolOutput::Mkdir(output) => {
            if output.created_paths.is_empty() {
                vec![output.path.clone()]
            } else {
                output.created_paths.clone()
            }
        }
        _ => Vec::new(),
    };
    deduplicate_preserving_order(paths)
}

fn fs_transaction_diff_from_output(output: &AutonomousToolOutput) -> Option<String> {
    match output {
        AutonomousToolOutput::Write(output) => output.diff.clone(),
        AutonomousToolOutput::Edit(output) => output.diff.clone(),
        AutonomousToolOutput::Patch(output) => output.diff.clone(),
        _ => None,
    }
}

fn fs_transaction_digest_from_output(output: &AutonomousToolOutput) -> Option<String> {
    match output {
        AutonomousToolOutput::Delete(output) => output.digest.clone(),
        _ => None,
    }
}

fn fs_transaction_source_digest_from_output(output: &AutonomousToolOutput) -> Option<String> {
    match output {
        AutonomousToolOutput::Copy(output) => output.source_digest.clone(),
        _ => None,
    }
}

fn fs_transaction_output(
    preview: bool,
    applied: bool,
    operation_count: usize,
    validated_operations: usize,
    validation_errors: Vec<AutonomousFsTransactionOperationResult>,
    planned_operations: Vec<AutonomousFsTransactionOperationResult>,
    results: Vec<AutonomousFsTransactionOperationResult>,
    rollback_status: AutonomousFsTransactionRollbackStatus,
) -> AutonomousFsTransactionOutput {
    let changed_paths = planned_operations
        .iter()
        .flat_map(|operation| operation.changed_paths.iter().cloned())
        .collect::<Vec<_>>();
    AutonomousFsTransactionOutput {
        applied,
        preview,
        operation_count,
        validation: AutonomousFsTransactionValidationSummary {
            ok: validation_errors.is_empty(),
            validated_operations,
            errors: validation_errors,
        },
        changed_paths: deduplicate_preserving_order(changed_paths),
        diff: fs_transaction_join_diffs(&planned_operations),
        planned_operations,
        rollback_status,
        results,
    }
}

fn fs_transaction_join_diffs(
    planned_operations: &[AutonomousFsTransactionOperationResult],
) -> Option<String> {
    let diffs = planned_operations
        .iter()
        .filter_map(|operation| operation.diff.clone())
        .collect::<Vec<_>>();
    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("\n"))
    }
}

fn fs_transaction_planned_result(
    operation: &FsTransactionPlannedOperation,
) -> AutonomousFsTransactionOperationResult {
    AutonomousFsTransactionOperationResult {
        index: operation.index,
        id: operation.id.clone(),
        action: operation.action,
        ok: true,
        status: "planned".into(),
        summary: operation.summary.clone(),
        changed_paths: operation.changed_paths.clone(),
        diff: operation.diff.clone(),
        digest: operation.digest.clone(),
        source_digest: operation.source_digest.clone(),
        error: None,
    }
}

fn fs_transaction_applied_result(
    operation: &FsTransactionPlannedOperation,
) -> AutonomousFsTransactionOperationResult {
    AutonomousFsTransactionOperationResult {
        index: operation.index,
        id: operation.id.clone(),
        action: operation.action,
        ok: true,
        status: "applied".into(),
        summary: operation.summary.clone(),
        changed_paths: operation.changed_paths.clone(),
        diff: operation.diff.clone(),
        digest: operation.digest.clone(),
        source_digest: operation.source_digest.clone(),
        error: None,
    }
}

fn fs_transaction_apply_error_result(
    operation: &FsTransactionPlannedOperation,
    error: CommandError,
) -> AutonomousFsTransactionOperationResult {
    AutonomousFsTransactionOperationResult {
        index: operation.index,
        id: operation.id.clone(),
        action: operation.action,
        ok: false,
        status: "apply_failed".into(),
        summary: format!(
            "Operation #{} failed during apply; rollback was attempted.",
            operation.index + 1
        ),
        changed_paths: operation.changed_paths.clone(),
        diff: operation.diff.clone(),
        digest: operation.digest.clone(),
        source_digest: operation.source_digest.clone(),
        error: Some(error.into()),
    }
}

fn fs_transaction_error_result(
    index: usize,
    id: Option<String>,
    action: AutonomousFsTransactionAction,
    status: &str,
    error: CommandError,
) -> AutonomousFsTransactionOperationResult {
    AutonomousFsTransactionOperationResult {
        index,
        id,
        action,
        ok: false,
        status: status.into(),
        summary: error.message.clone(),
        changed_paths: Vec::new(),
        diff: None,
        digest: None,
        source_digest: None,
        error: Some(error.into()),
    }
}

fn deduplicate_preserving_order(paths: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for path in paths {
        if seen.insert(path.clone()) {
            unique.push(path);
        }
    }
    unique
}

fn copy_fs_transaction_backup_entry(from: &Path, to: &Path) -> CommandResult<()> {
    let metadata = fs::symlink_metadata(from).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_fs_transaction_backup_failed",
            format!(
                "Xero could not inspect transaction backup path {}: {error}",
                from.display()
            ),
        )
    })?;
    match stat_kind(&metadata) {
        AutonomousStatKind::File => {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    CommandError::retryable(
                        "autonomous_tool_fs_transaction_backup_failed",
                        format!(
                            "Xero could not create backup directory {}: {error}",
                            parent.display()
                        ),
                    )
                })?;
            }
            fs::copy(from, to).map(|_| ()).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_fs_transaction_backup_failed",
                    format!("Xero could not back up file {}: {error}", from.display()),
                )
            })
        }
        AutonomousStatKind::Directory => {
            fs::create_dir_all(to).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_fs_transaction_backup_failed",
                    format!(
                        "Xero could not create backup directory {}: {error}",
                        to.display()
                    ),
                )
            })?;
            let mut entries = fs::read_dir(from)
                .map_err(|error| {
                    CommandError::retryable(
                        "autonomous_tool_fs_transaction_backup_failed",
                        format!(
                            "Xero could not enumerate backup directory {}: {error}",
                            from.display()
                        ),
                    )
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    CommandError::retryable(
                        "autonomous_tool_fs_transaction_backup_failed",
                        format!(
                            "Xero could not enumerate backup directory {}: {error}",
                            from.display()
                        ),
                    )
                })?;
            entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
            for entry in entries {
                copy_fs_transaction_backup_entry(&entry.path(), &to.join(entry.file_name()))?;
            }
            Ok(())
        }
        AutonomousStatKind::Symlink | AutonomousStatKind::Other | AutonomousStatKind::Missing => {
            Err(CommandError::user_fixable(
                "autonomous_tool_fs_transaction_backup_unsupported",
                format!(
                    "Xero refused fs_transaction because rollback backup does not support {}.",
                    from.display()
                ),
            ))
        }
    }
}

fn restore_fs_transaction_backup_entry(entry: &FsTransactionBackupEntry) -> CommandResult<String> {
    if entry.resolved_path.exists() {
        remove_fs_transaction_path(&entry.resolved_path)?;
    }
    if let Some(backup_path) = entry.backup_path.as_ref() {
        copy_fs_transaction_backup_entry(backup_path, &entry.resolved_path)?;
        Ok("restore".into())
    } else {
        Ok("remove_created".into())
    }
}

fn remove_fs_transaction_path(path: &Path) -> CommandResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_fs_transaction_rollback_failed",
            format!(
                "Xero could not inspect rollback path {}: {error}",
                path.display()
            ),
        )
    })?;
    match stat_kind(&metadata) {
        AutonomousStatKind::Directory => fs::remove_dir_all(path),
        _ => fs::remove_file(path),
    }
    .map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_fs_transaction_rollback_failed",
            format!(
                "Xero could not remove rollback path {}: {error}",
                path.display()
            ),
        )
    })
}

fn parse_structured_document(
    text: &str,
    format: AutonomousStructuredEditFormat,
) -> CommandResult<JsonValue> {
    match format {
        AutonomousStructuredEditFormat::Json => serde_json::from_str(text).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_structured_edit_parse_failed",
                format!("Xero could not parse JSON for structured edit: {error}"),
            )
        }),
        AutonomousStructuredEditFormat::Toml => {
            let value = toml::from_str::<toml::Value>(text).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_parse_failed",
                    format!("Xero could not parse TOML for structured edit: {error}"),
                )
            })?;
            serde_json::to_value(value).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_parse_failed",
                    format!("Xero could not normalize TOML for structured edit: {error}"),
                )
            })
        }
        AutonomousStructuredEditFormat::Yaml => {
            let value = serde_yaml::from_str::<serde_yaml::Value>(text).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_parse_failed",
                    format!("Xero could not parse YAML for structured edit: {error}"),
                )
            })?;
            serde_json::to_value(value).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_parse_failed",
                    format!(
                        "Xero only supports YAML documents with string-compatible mapping keys: {error}"
                    ),
                )
            })
        }
    }
}

fn serialize_structured_document(
    value: &JsonValue,
    format: AutonomousStructuredEditFormat,
) -> CommandResult<String> {
    let rendered = match format {
        AutonomousStructuredEditFormat::Json => {
            serde_json::to_string_pretty(value).map_err(|error| {
                CommandError::system_fault(
                    "autonomous_tool_structured_edit_serialize_failed",
                    format!("Xero could not serialize JSON after structured edit: {error}"),
                )
            })?
        }
        AutonomousStructuredEditFormat::Toml => {
            let value = serde_json::from_value::<toml::Value>(value.clone()).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_serialize_failed",
                    format!("Xero could not convert edited data to TOML: {error}"),
                )
            })?;
            toml::to_string_pretty(&value).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_serialize_failed",
                    format!("Xero could not serialize TOML after structured edit: {error}"),
                )
            })?
        }
        AutonomousStructuredEditFormat::Yaml => {
            let value =
                serde_json::from_value::<serde_yaml::Value>(value.clone()).map_err(|error| {
                    CommandError::user_fixable(
                        "autonomous_tool_structured_edit_serialize_failed",
                        format!("Xero could not convert edited data to YAML: {error}"),
                    )
                })?;
            serde_yaml::to_string(&value).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_serialize_failed",
                    format!("Xero could not serialize YAML after structured edit: {error}"),
                )
            })?
        }
    };
    Ok(ensure_trailing_newline(rendered))
}

fn apply_structured_edit_operation(
    document: &mut JsonValue,
    operation: &super::AutonomousStructuredEditOperation,
    semantic_changes: &mut Vec<String>,
) -> CommandResult<()> {
    let tokens = parse_json_pointer(&operation.pointer)?;
    match operation.action {
        AutonomousStructuredEditAction::Set => {
            let value = operation.value.clone().ok_or_else(|| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_value_required",
                    "Xero requires value for structured set operations.",
                )
            })?;
            let target = pointer_parent_mut(document, &tokens, true)?;
            let key = tokens.last().ok_or_else(|| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_pointer_invalid",
                    "Xero requires set pointers to target a field or array index.",
                )
            })?;
            set_pointer_child(target, key, value)?;
            semantic_changes.push(format!("set {}", operation.pointer));
        }
        AutonomousStructuredEditAction::Delete => {
            let target = pointer_parent_mut(document, &tokens, false)?;
            let key = tokens.last().ok_or_else(|| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_pointer_invalid",
                    "Xero requires delete pointers to target a field or array index.",
                )
            })?;
            delete_pointer_child(target, key)?;
            semantic_changes.push(format!("delete {}", operation.pointer));
        }
        AutonomousStructuredEditAction::AppendUnique => {
            let value = operation.value.clone().ok_or_else(|| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_value_required",
                    "Xero requires value for appendUnique operations.",
                )
            })?;
            let target = pointer_value_mut(document, &tokens, false)?;
            let Some(items) = target.as_array_mut() else {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_structured_edit_array_required",
                    format!(
                        "Xero requires appendUnique pointer `{}` to target an array.",
                        operation.pointer
                    ),
                ));
            };
            if !items.iter().any(|item| item == &value) {
                items.push(value);
            }
            semantic_changes.push(format!("append_unique {}", operation.pointer));
        }
        AutonomousStructuredEditAction::SortKeys => {
            let target = pointer_value_mut(document, &tokens, false)?;
            sort_json_value_keys(target);
            semantic_changes.push(format!("sort_keys {}", operation.pointer));
        }
    }
    Ok(())
}

fn parse_json_pointer(pointer: &str) -> CommandResult<Vec<String>> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        return Err(CommandError::user_fixable(
            "autonomous_tool_structured_edit_pointer_invalid",
            "Xero requires structured edit pointers to use JSON Pointer syntax such as /scripts/build.",
        ));
    }
    pointer
        .split('/')
        .skip(1)
        .map(|segment| {
            let segment = segment.replace("~1", "/").replace("~0", "~");
            if segment.is_empty() {
                Err(CommandError::user_fixable(
                    "autonomous_tool_structured_edit_pointer_invalid",
                    "Xero does not support empty JSON Pointer path segments.",
                ))
            } else {
                Ok(segment)
            }
        })
        .collect()
}

fn pointer_parent_mut<'a>(
    value: &'a mut JsonValue,
    tokens: &[String],
    create_missing: bool,
) -> CommandResult<&'a mut JsonValue> {
    if tokens.is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_structured_edit_pointer_invalid",
            "Xero requires this structured edit operation to target a child path.",
        ));
    }
    pointer_value_mut(value, &tokens[..tokens.len() - 1], create_missing)
}

fn pointer_value_mut<'a>(
    mut value: &'a mut JsonValue,
    tokens: &[String],
    create_missing: bool,
) -> CommandResult<&'a mut JsonValue> {
    for token in tokens {
        match value {
            JsonValue::Object(map) => {
                if create_missing {
                    value = map.entry(token.clone()).or_insert_with(|| json!({}));
                } else {
                    value = map.get_mut(token).ok_or_else(|| {
                        CommandError::user_fixable(
                            "autonomous_tool_structured_edit_pointer_missing",
                            format!("Xero could not find structured edit path segment `{token}`."),
                        )
                    })?;
                }
            }
            JsonValue::Array(items) => {
                let index = token.parse::<usize>().map_err(|_| {
                    CommandError::user_fixable(
                        "autonomous_tool_structured_edit_pointer_invalid",
                        format!(
                            "Xero requires array pointer segment `{token}` to be a numeric index."
                        ),
                    )
                })?;
                value = items.get_mut(index).ok_or_else(|| {
                    CommandError::user_fixable(
                        "autonomous_tool_structured_edit_pointer_missing",
                        format!("Xero could not find array index `{index}`."),
                    )
                })?;
            }
            _ => {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_structured_edit_container_required",
                    format!("Xero could not descend through non-container segment `{token}`."),
                ));
            }
        }
    }
    Ok(value)
}

fn set_pointer_child(parent: &mut JsonValue, key: &str, value: JsonValue) -> CommandResult<()> {
    match parent {
        JsonValue::Object(map) => {
            map.insert(key.into(), value);
            Ok(())
        }
        JsonValue::Array(items) => {
            if key == "-" {
                items.push(value);
                return Ok(());
            }
            let index = key.parse::<usize>().map_err(|_| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_pointer_invalid",
                    format!(
                        "Xero requires array set segment `{key}` to be a numeric index or `-`."
                    ),
                )
            })?;
            if index > items.len() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_structured_edit_pointer_missing",
                    format!("Xero cannot set array index `{index}` beyond the array length."),
                ));
            }
            if index == items.len() {
                items.push(value);
            } else {
                items[index] = value;
            }
            Ok(())
        }
        _ => Err(CommandError::user_fixable(
            "autonomous_tool_structured_edit_container_required",
            "Xero requires set parent paths to resolve to an object or array.",
        )),
    }
}

fn delete_pointer_child(parent: &mut JsonValue, key: &str) -> CommandResult<()> {
    match parent {
        JsonValue::Object(map) => {
            if map.remove(key).is_none() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_structured_edit_pointer_missing",
                    format!("Xero could not delete missing object key `{key}`."),
                ));
            }
            Ok(())
        }
        JsonValue::Array(items) => {
            let index = key.parse::<usize>().map_err(|_| {
                CommandError::user_fixable(
                    "autonomous_tool_structured_edit_pointer_invalid",
                    format!("Xero requires array delete segment `{key}` to be a numeric index."),
                )
            })?;
            if index >= items.len() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_structured_edit_pointer_missing",
                    format!("Xero could not delete missing array index `{index}`."),
                ));
            }
            items.remove(index);
            Ok(())
        }
        _ => Err(CommandError::user_fixable(
            "autonomous_tool_structured_edit_container_required",
            "Xero requires delete parent paths to resolve to an object or array.",
        )),
    }
}

fn sort_json_value_keys(value: &mut JsonValue) {
    match value {
        JsonValue::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut entries = std::mem::take(map).into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            for (key, mut value) in entries {
                sort_json_value_keys(&mut value);
                sorted.insert(key, value);
            }
            *map = sorted;
        }
        JsonValue::Array(items) => {
            for item in items {
                sort_json_value_keys(item);
            }
        }
        _ => {}
    }
}

fn ensure_trailing_newline(mut value: String) -> String {
    if !value.ends_with('\n') {
        value.push('\n');
    }
    value
}

fn read_many_error_item(
    path: String,
    error: CommandError,
    omitted_bytes: Option<u64>,
) -> AutonomousReadManyItem {
    AutonomousReadManyItem {
        path,
        ok: false,
        read: None,
        error: Some(AutonomousReadManyError::from(error)),
        omitted_bytes,
    }
}

fn list_tree_name(display_path: &str) -> String {
    if display_path == "." {
        ".".into()
    } else {
        display_path
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or(display_path)
            .into()
    }
}

fn list_tree_matches_filters(
    display_path: &str,
    is_directory: bool,
    include_globs: Option<&GlobSet>,
    exclude_globs: Option<&GlobSet>,
) -> bool {
    if let Some(globs) = exclude_globs {
        if globs.is_match(display_path) {
            return false;
        }
    }
    if let Some(globs) = include_globs {
        if is_directory {
            return true;
        }
        if !globs.is_match(display_path) {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone)]
struct ReadTarget {
    display_path: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct ReadResultMetadata {
    path_kind: AutonomousStatKind,
    size: Option<u64>,
    modified_at: Option<String>,
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
    guard_status: AutonomousPatchGuardStatus,
    changed_ranges: Vec<AutonomousPatchChangedRange>,
    line_ending: AutonomousLineEnding,
    bom_preserved: bool,
}

#[derive(Debug)]
struct SearchOptions {
    regex: bool,
    ignore_case: bool,
    include_hidden: bool,
    include_ignored: bool,
    files_only: bool,
    include_globs: Option<GlobSet>,
    exclude_globs: Option<GlobSet>,
    context_lines: usize,
    max_results: usize,
    cursor_offset: usize,
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
            files_only: request.files_only,
            include_globs: build_search_globset(&request.include_globs, "includeGlobs")?,
            exclude_globs: build_search_globset(&request.exclude_globs, "excludeGlobs")?,
            context_lines,
            max_results,
            cursor_offset: 0,
        })
    }
}

#[derive(Debug, Default)]
struct SearchResult {
    matches: Vec<AutonomousSearchMatch>,
    files: BTreeMap<String, SearchFileSummaryState>,
    matched_files: BTreeSet<String>,
    scanned_files: usize,
    total_matches: usize,
    returned_matches: usize,
    truncated: bool,
    omissions: AutonomousSearchOmissions,
}

#[derive(Debug, Default)]
struct SearchFileSummaryState {
    match_count: usize,
    first_line: Option<usize>,
    first_preview: Option<String>,
}

#[derive(Debug)]
struct FindOptions {
    mode: AutonomousFindMode,
    pattern: String,
    glob_matcher: Option<GlobMatcher>,
    scope_relative: Option<PathBuf>,
    scope_is_file: bool,
    max_depth: Option<usize>,
    max_results: usize,
    cursor_offset: usize,
}

#[derive(Debug, Default)]
struct FindResult {
    matches: Vec<String>,
    scanned_files: usize,
    returned_matches: usize,
    total_matches: usize,
    file_count: usize,
    directory_count: usize,
    symlink_count: usize,
    other_count: usize,
    truncated: bool,
    omissions: AutonomousFindOmissions,
}

#[derive(Debug)]
struct ListOptions {
    max_depth: usize,
    cursor_offset: usize,
}

#[derive(Debug, Clone)]
struct ListCandidate {
    entry: AutonomousListEntry,
    name: String,
    kind: AutonomousStatKind,
    modified_at_sort: Option<String>,
}

#[derive(Debug, Default)]
struct ListCollection {
    candidates: Vec<ListCandidate>,
    file_count: usize,
    directory_count: usize,
    symlink_count: usize,
    other_count: usize,
    omitted: AutonomousListOmissions,
}

fn stat_kind(metadata: &Metadata) -> AutonomousStatKind {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        AutonomousStatKind::Symlink
    } else if metadata.is_file() {
        AutonomousStatKind::File
    } else if metadata.is_dir() {
        AutonomousStatKind::Directory
    } else {
        AutonomousStatKind::Other
    }
}

fn stat_size(metadata: &Metadata, kind: AutonomousStatKind) -> Option<u64> {
    matches!(
        kind,
        AutonomousStatKind::File | AutonomousStatKind::Symlink | AutonomousStatKind::Other
    )
    .then_some(metadata.len())
}

fn stat_kind_label(kind: AutonomousStatKind) -> &'static str {
    match kind {
        AutonomousStatKind::File => "file",
        AutonomousStatKind::Directory => "directory",
        AutonomousStatKind::Symlink => "symlink",
        AutonomousStatKind::Missing => "missing",
        AutonomousStatKind::Other => "other",
    }
}

fn read_file_metadata(metadata: &Metadata) -> ReadResultMetadata {
    let path_kind = stat_kind(metadata);
    ReadResultMetadata {
        path_kind,
        size: stat_size(metadata, path_kind),
        modified_at: metadata.modified().ok().and_then(system_time_to_rfc3339),
    }
}

fn stat_permissions(metadata: &Metadata) -> AutonomousStatPermissions {
    AutonomousStatPermissions {
        readonly: metadata.permissions().readonly(),
        unix_mode: unix_mode(metadata),
    }
}

#[cfg(unix)]
fn unix_mode(metadata: &Metadata) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;

    Some(format!("{:04o}", metadata.permissions().mode() & 0o7777))
}

#[cfg(not(unix))]
fn unix_mode(_metadata: &Metadata) -> Option<String> {
    None
}

fn system_time_to_rfc3339(value: SystemTime) -> Option<String> {
    let datetime = OffsetDateTime::from(value);
    datetime.format(&Rfc3339).ok()
}

fn stat_summary(
    path: &str,
    kind: AutonomousStatKind,
    size: u64,
    include_hash: bool,
    sha256: Option<&str>,
    hash_omitted_reason: &Option<String>,
    include_git_status: bool,
    git_status_count: usize,
) -> String {
    let kind_label = match kind {
        AutonomousStatKind::File => "file",
        AutonomousStatKind::Directory => "directory",
        AutonomousStatKind::Symlink => "symlink",
        AutonomousStatKind::Missing => "missing path",
        AutonomousStatKind::Other => "filesystem entry",
    };
    let size_fragment = if kind == AutonomousStatKind::Directory {
        String::new()
    } else {
        format!(" ({size} byte(s))")
    };
    let hash_fragment = if let Some(sha256) = sha256 {
        format!(" SHA-256 `{sha256}`.")
    } else if include_hash {
        format!(
            " Hash omitted: {}.",
            hash_omitted_reason
                .as_deref()
                .unwrap_or("not available for this path")
        )
    } else {
        String::new()
    };
    let git_fragment = if include_git_status {
        format!(" Git status matched {git_status_count} status item(s).")
    } else {
        String::new()
    };
    format!(
        "Stat inspected `{path}` as a {kind_label}{size_fragment}.{hash_fragment}{git_fragment}"
    )
}

fn validate_read_request_shape(request: &AutonomousReadRequest) -> CommandResult<()> {
    if request.cursor.is_some() && request.start_line.is_some() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_read_cursor_conflict",
            "Xero requires either cursor or startLine for read, not both.",
        ));
    }
    if request.cursor.is_some() && request.around_pattern.is_some() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_read_cursor_conflict",
            "Xero requires either cursor or aroundPattern for read, not both.",
        ));
    }
    if request.around_pattern.is_some() && request.start_line.is_some() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_read_around_pattern_conflict",
            "Xero requires either aroundPattern or startLine for read, not both.",
        ));
    }
    if (request.cursor.is_some() || request.around_pattern.is_some())
        && (request.byte_offset.is_some() || request.byte_count.is_some())
    {
        return Err(CommandError::user_fixable(
            "autonomous_tool_read_range_mode_conflict",
            "Xero cannot combine cursor or aroundPattern with byteOffset/byteCount reads.",
        ));
    }
    if let Some(pattern) = request.around_pattern.as_deref() {
        validate_non_empty(pattern, "aroundPattern")?;
        if pattern.chars().count() > MAX_READ_AROUND_PATTERN_CHARS {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_around_pattern_too_large",
                format!(
                    "Xero requires read aroundPattern to be at most {MAX_READ_AROUND_PATTERN_CHARS} characters."
                ),
            ));
        }
    }
    Ok(())
}

fn read_start_line(
    request: &AutonomousReadRequest,
    text: &str,
    total_lines: usize,
    line_count: usize,
    sha256: &str,
) -> CommandResult<usize> {
    if let Some(cursor) = request.cursor.as_deref() {
        let (cursor_sha256, start_line) = parse_read_cursor(cursor)?;
        if cursor_sha256 != sha256 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_read_cursor_stale",
                "Xero refused the read cursor because the file hash no longer matches.",
            ));
        }
        return Ok(start_line);
    }

    if let Some(pattern) = request.around_pattern.as_deref() {
        let matched_line = text
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains(pattern))
            .map(|(index, _)| index + 1)
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "autonomous_tool_read_around_pattern_not_found",
                    "Xero could not find aroundPattern in the requested file.",
                )
            })?;
        if total_lines == 0 || line_count >= total_lines {
            return Ok(1);
        }
        let half_window = line_count / 2;
        let mut start_line = matched_line.saturating_sub(half_window).max(1);
        let last_start = total_lines.saturating_sub(line_count).saturating_add(1);
        if start_line > last_start {
            start_line = last_start;
        }
        return Ok(start_line.max(1));
    }

    Ok(request.start_line.unwrap_or(1))
}

fn read_cursor(sha256: &str, start_line: usize) -> String {
    format!("{READ_CURSOR_PREFIX}:{sha256}:{start_line}")
}

fn parse_read_cursor(cursor: &str) -> CommandResult<(String, usize)> {
    let parts = cursor.split(':').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != "read" || parts[1] != "v1" {
        return Err(CommandError::user_fixable(
            "autonomous_tool_read_cursor_invalid",
            "Xero requires read cursor values returned by a previous read result.",
        ));
    }
    let sha256 = parts[2].trim();
    validate_sha256(sha256, "cursor")?;
    let start_line = parts[3].parse::<usize>().map_err(|_| {
        CommandError::user_fixable(
            "autonomous_tool_read_cursor_invalid",
            "Xero requires read cursor values returned by a previous read result.",
        )
    })?;
    if start_line == 0 {
        return Err(CommandError::user_fixable(
            "autonomous_tool_read_cursor_invalid",
            "Xero requires read cursor start lines to be at least 1.",
        ));
    }
    Ok((sha256.to_string(), start_line))
}

fn search_cursor_fingerprint(
    query: &str,
    scope: Option<&str>,
    include_globs: &[String],
    exclude_globs: &[String],
    options: &SearchOptions,
) -> String {
    sha256_hex(
        format!(
            "search:v1\0query={query}\0scope={}\0regex={}\0ignore_case={}\0include_hidden={}\0include_ignored={}\0files_only={}\0context_lines={}\0max_results={}\0include_globs={}\0exclude_globs={}",
            scope.unwrap_or("."),
            options.regex,
            options.ignore_case,
            options.include_hidden,
            options.include_ignored,
            options.files_only,
            options.context_lines,
            options.max_results,
            include_globs.join("\u{1f}"),
            exclude_globs.join("\u{1f}")
        )
        .as_bytes(),
    )
}

fn search_cursor(fingerprint: &str, offset: usize) -> String {
    format!("{SEARCH_CURSOR_PREFIX}:{fingerprint}:{offset}")
}

fn parse_search_cursor(cursor: &str) -> CommandResult<(String, usize)> {
    let parts = cursor.split(':').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != "search" || parts[1] != "v1" {
        return Err(CommandError::user_fixable(
            "autonomous_tool_search_cursor_invalid",
            "Xero requires search cursor values returned by a previous search result.",
        ));
    }
    let fingerprint = parts[2].trim();
    validate_sha256(fingerprint, "cursor")?;
    let offset = parts[3].parse::<usize>().map_err(|_| {
        CommandError::user_fixable(
            "autonomous_tool_search_cursor_invalid",
            "Xero requires search cursor values returned by a previous search result.",
        )
    })?;
    Ok((fingerprint.to_string(), offset))
}

fn search_file_summaries(
    files: BTreeMap<String, SearchFileSummaryState>,
) -> Vec<AutonomousSearchFileSummary> {
    files
        .into_iter()
        .map(|(path, summary)| AutonomousSearchFileSummary {
            path,
            match_count: summary.match_count,
            first_line: summary.first_line,
            first_preview: summary.first_preview,
        })
        .collect()
}

fn record_search_file_omission(omissions: &mut AutonomousSearchOmissions, error: &CommandError) {
    match error.code.as_str() {
        "autonomous_tool_file_not_text" => {
            omissions.binary_files = omissions.binary_files.saturating_add(1);
        }
        "autonomous_tool_file_too_large" => {
            omissions.oversized_files = omissions.oversized_files.saturating_add(1);
        }
        _ => {
            omissions.unreadable_files = omissions.unreadable_files.saturating_add(1);
        }
    }
}

fn normalize_find_pattern(pattern: &str, mode: AutonomousFindMode) -> CommandResult<String> {
    let trimmed = pattern.trim();
    validate_non_empty(trimmed, "pattern")?;
    match mode {
        AutonomousFindMode::Glob => normalize_glob_pattern(trimmed),
        AutonomousFindMode::Name => Ok(trimmed.to_string()),
        AutonomousFindMode::Extension => Ok(trimmed.trim_start_matches('.').to_string()),
        AutonomousFindMode::PathPrefix => {
            normalize_relative_path(trimmed, "pattern").map(|path| path_to_forward_slash(&path))
        }
    }
}

fn find_candidate_matches(
    display_path: &str,
    candidate: &str,
    path: &Path,
    options: &FindOptions,
) -> bool {
    match options.mode {
        AutonomousFindMode::Glob => options
            .glob_matcher
            .as_ref()
            .is_some_and(|matcher| matcher.is_match(candidate)),
        AutonomousFindMode::Name => path
            .file_name()
            .is_some_and(|name| name.to_string_lossy() == options.pattern),
        AutonomousFindMode::Extension => path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy() == options.pattern),
        AutonomousFindMode::PathPrefix => {
            display_path == options.pattern
                || display_path
                    .strip_prefix(options.pattern.as_str())
                    .is_some_and(|suffix| suffix.starts_with('/'))
        }
    }
}

fn find_cursor_fingerprint(
    pattern: &str,
    mode: AutonomousFindMode,
    scope: Option<&str>,
    max_depth: Option<usize>,
    max_results: usize,
) -> String {
    sha256_hex(
        format!(
            "find:v1\0pattern={pattern}\0mode={mode:?}\0scope={}\0max_depth={}\0max_results={max_results}",
            scope.unwrap_or("."),
            max_depth
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".into())
        )
        .as_bytes(),
    )
}

fn find_cursor(fingerprint: &str, offset: usize) -> String {
    format!("{FIND_CURSOR_PREFIX}:{fingerprint}:{offset}")
}

fn parse_find_cursor(cursor: &str) -> CommandResult<(String, usize)> {
    let parts = cursor.split(':').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != "find" || parts[1] != "v1" {
        return Err(CommandError::user_fixable(
            "autonomous_tool_find_cursor_invalid",
            "Xero requires find cursor values returned by a previous find result.",
        ));
    }
    let fingerprint = parts[2].trim();
    validate_sha256(fingerprint, "cursor")?;
    let offset = parts[3].parse::<usize>().map_err(|_| {
        CommandError::user_fixable(
            "autonomous_tool_find_cursor_invalid",
            "Xero requires find cursor values returned by a previous find result.",
        )
    })?;
    Ok((fingerprint.to_string(), offset))
}

fn list_cursor_fingerprint(
    path: &str,
    max_depth: usize,
    max_results: usize,
    sort_by: AutonomousListSortBy,
    sort_direction: AutonomousListSortDirection,
) -> String {
    sha256_hex(
        format!(
            "list:v1\0path={path}\0max_depth={max_depth}\0max_results={max_results}\0sort_by={sort_by:?}\0sort_direction={sort_direction:?}"
        )
        .as_bytes(),
    )
}

fn list_cursor(fingerprint: &str, offset: usize) -> String {
    format!("{LIST_CURSOR_PREFIX}:{fingerprint}:{offset}")
}

fn parse_list_cursor(cursor: &str) -> CommandResult<(String, usize)> {
    let parts = cursor.split(':').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != "list" || parts[1] != "v1" {
        return Err(CommandError::user_fixable(
            "autonomous_tool_list_cursor_invalid",
            "Xero requires list cursor values returned by a previous list result.",
        ));
    }
    let fingerprint = parts[2].trim();
    validate_sha256(fingerprint, "cursor")?;
    let offset = parts[3].parse::<usize>().map_err(|_| {
        CommandError::user_fixable(
            "autonomous_tool_list_cursor_invalid",
            "Xero requires list cursor values returned by a previous list result.",
        )
    })?;
    Ok((fingerprint.to_string(), offset))
}

fn sort_list_candidates(
    candidates: &mut [ListCandidate],
    sort_by: AutonomousListSortBy,
    sort_direction: AutonomousListSortDirection,
) {
    candidates.sort_by(|left, right| {
        let ordering = match sort_by {
            AutonomousListSortBy::Path => left.entry.path.cmp(&right.entry.path),
            AutonomousListSortBy::Name => left
                .name
                .cmp(&right.name)
                .then_with(|| left.entry.path.cmp(&right.entry.path)),
            AutonomousListSortBy::Kind => stat_kind_label(left.kind)
                .cmp(stat_kind_label(right.kind))
                .then_with(|| left.entry.path.cmp(&right.entry.path)),
            AutonomousListSortBy::Size => left
                .entry
                .bytes
                .unwrap_or(0)
                .cmp(&right.entry.bytes.unwrap_or(0))
                .then_with(|| left.entry.path.cmp(&right.entry.path)),
            AutonomousListSortBy::Modified => left
                .modified_at_sort
                .cmp(&right.modified_at_sort)
                .then_with(|| left.entry.path.cmp(&right.entry.path)),
        };
        if sort_direction == AutonomousListSortDirection::Desc {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

fn should_omit_generated_text(
    request: &AutonomousReadRequest,
    text: &str,
    total_bytes: u64,
    total_lines: usize,
) -> bool {
    if request.start_line.is_some()
        || request.cursor.is_some()
        || request.around_pattern.is_some()
        || total_bytes < GENERATED_TEXT_OMIT_MIN_BYTES
    {
        return false;
    }
    let longest_line = text
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    total_lines <= GENERATED_TEXT_MAX_LINES_FOR_OMIT
        || longest_line >= GENERATED_TEXT_LONG_LINE_CHARS
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

fn patch_no_rollback_status() -> AutonomousFsTransactionRollbackStatus {
    AutonomousFsTransactionRollbackStatus {
        attempted: false,
        succeeded: true,
        attempts: Vec::new(),
    }
}

fn changed_line_ranges(before: &str, after: &str) -> Vec<AutonomousPatchChangedRange> {
    let before_lines = before.lines().collect::<Vec<_>>();
    let after_lines = after.lines().collect::<Vec<_>>();
    let max_lines = before_lines.len().max(after_lines.len());
    let mut ranges = Vec::new();
    let mut start = None;

    for index in 0..max_lines {
        let changed = before_lines.get(index) != after_lines.get(index);
        match (changed, start) {
            (true, None) => start = Some(index + 1),
            (false, Some(start_line)) => {
                ranges.push(AutonomousPatchChangedRange {
                    start_line,
                    end_line: index,
                });
                start = None;
            }
            _ => {}
        }
    }

    if let Some(start_line) = start {
        ranges.push(AutonomousPatchChangedRange {
            start_line,
            end_line: max_lines.max(start_line),
        });
    }

    ranges
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

fn edit_conflict_context(text: &str, start_line: usize, end_line: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return "(file has no lines)".into();
    }
    let context_start = start_line.saturating_sub(2).max(1);
    let context_end = (end_line + 2).min(lines.len());
    (context_start..=context_end)
        .filter_map(|line_number| {
            let line = lines.get(line_number.saturating_sub(1))?;
            Some(format!(
                "line {line_number}: sha256={} text={}",
                line_hash(line),
                truncate_chars(line, 240)
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
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
            files_only: false,
            cursor: None,
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
            files_only: false,
            cursor: None,
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
    fn result_page_reads_only_project_app_data_tool_artifacts() {
        let tempdir = tempdir().expect("tempdir");
        let root = tempdir.path();
        let runtime = AutonomousToolRuntime::new(root).expect("runtime");
        let artifact_dir = project_app_data_dir_for_repo(root).join("tool-artifacts/command");
        fs::create_dir_all(&artifact_dir).expect("artifact dir");
        let artifact_path = artifact_dir.join("output.json");
        fs::write(&artifact_path, "alpha\nbeta\ngamma\n").expect("artifact");

        let output = runtime
            .result_page(AutonomousResultPageRequest {
                artifact_path: artifact_path.display().to_string(),
                byte_offset: Some(0),
                max_bytes: Some(6),
            })
            .expect("result page");
        let AutonomousToolOutput::ResultPage(output) = output.output else {
            panic!("expected result_page output");
        };
        assert_eq!(output.content, "alpha\n");
        assert_eq!(output.byte_count, 6);
        assert!(output.truncated);
        assert_eq!(output.next_byte_offset, Some(6));

        let outside_path = tempdir.path().join("outside-artifact.txt");
        fs::write(&outside_path, "outside").expect("outside artifact");
        let outside_error = runtime
            .result_page(AutonomousResultPageRequest {
                artifact_path: outside_path.display().to_string(),
                byte_offset: None,
                max_bytes: Some(6),
            })
            .expect_err("outside artifact denied");
        assert_eq!(outside_error.code, "policy_denied");

        let _ = fs::remove_dir_all(project_app_data_dir_for_repo(root));
    }

    #[test]
    fn observe_tools_treat_blank_optional_paths_as_repo_root() {
        let tempdir = tempdir().expect("tempdir");
        let root = tempdir.path();
        fs::create_dir_all(root.join("src")).expect("src dir");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("source");

        let runtime = AutonomousToolRuntime::new(root).expect("runtime");
        let blank_list_output = list_output(runtime.list(AutonomousListRequest {
            path: Some("  ".into()),
            max_depth: Some(1),
            max_results: None,
            sort_by: None,
            sort_direction: None,
            cursor: None,
        }));
        assert_eq!(blank_list_output.path, ".");
        assert!(blank_list_output
            .entries
            .iter()
            .any(|entry| entry.path == "src"));

        let dot_list_output = list_output(runtime.list(AutonomousListRequest {
            path: Some(".".into()),
            max_depth: Some(1),
            max_results: None,
            sort_by: None,
            sort_direction: None,
            cursor: None,
        }));
        assert_eq!(dot_list_output.path, ".");
        assert!(dot_list_output
            .entries
            .iter()
            .any(|entry| entry.path == "src"));

        let search_output = search_output(runtime.search(AutonomousSearchRequest {
            query: "main".into(),
            path: Some(".".into()),
            regex: false,
            ignore_case: false,
            include_hidden: false,
            include_ignored: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            context_lines: None,
            max_results: None,
            files_only: false,
            cursor: None,
        }));
        assert_eq!(search_output.scope, None);
        assert_eq!(search_output.matches[0].path, "src/main.rs");

        let find_output = find_output(runtime.find(AutonomousFindRequest {
            pattern: "**/*.rs".into(),
            mode: None,
            path: Some(".".into()),
            max_depth: None,
            max_results: None,
            cursor: None,
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
            preview: false,
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
                preview: false,
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
                cursor: None,
                around_pattern: None,
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
            cursor: None,
            around_pattern: None,
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
        let alpha_hash = sha256_hex(b"one\ntwo\n");
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
                    expected_hash: Some(alpha_hash.clone()),
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
        assert!(!preview.rollback_status.attempted);
        assert_eq!(
            preview.files[0].guard_status.expected_hashes,
            vec![alpha_hash]
        );
        assert!(preview.files[0].guard_status.matched);
        assert_eq!(preview.files[0].changed_ranges[0].start_line, 2);
        assert_eq!(preview.files[0].changed_ranges[0].end_line, 2);
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
        assert!(!applied.rollback_status.attempted);
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
            cursor: None,
            around_pattern: None,
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
