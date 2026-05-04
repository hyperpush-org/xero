use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use git2::{Repository, Status};
use ignore::WalkBuilder;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        backend_jobs::BackendCancellationToken,
        project_files::{
            is_skipped_project_directory_name, metadata_modified_at, resolve_project_root,
        },
        validate_non_empty, CommandError, CommandResult, ProjectIdRequestDto,
        WorkspaceExplainRequestDto, WorkspaceExplainResponseDto, WorkspaceIndexDiagnosticDto,
        WorkspaceIndexRequestDto, WorkspaceIndexResponseDto, WorkspaceIndexStateDto,
        WorkspaceIndexStatusDto, WorkspaceQueryModeDto, WorkspaceQueryRequestDto,
        WorkspaceQueryResponseDto, WorkspaceQueryResultDto,
    },
    db::{database_path_for_repo, project_store, project_store::open_project_database},
    state::DesktopState,
};

const WORKSPACE_INDEX_VERSION: u32 = 1;
const DEFAULT_MAX_INDEX_FILES: u32 = 5_000;
const HARD_MAX_INDEX_FILES: u32 = 20_000;
const DEFAULT_QUERY_LIMIT: u32 = 12;
const HARD_QUERY_LIMIT: u32 = 50;
const MAX_INDEX_FILE_BYTES: u64 = 1_000_000;
const MAX_SNIPPET_CHARS: usize = 1_200;
const MAX_FEATURES: usize = 40;
const MAX_IMPORTS: usize = 32;
const MAX_SYMBOLS: usize = 64;
const MAX_TESTS: usize = 32;
const MAX_DIFF_SIGNALS: usize = 12;
const MAX_FAILURE_SIGNALS: usize = 12;

#[tauri::command]
pub async fn workspace_index<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: WorkspaceIndexRequestDto,
) -> CommandResult<WorkspaceIndexResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    drop(app);

    jobs.run_blocking_latest(
        format!("workspace-index:{}", request.project_id),
        "workspace index",
        move |cancellation| index_workspace_at_root(&repo_root, request, cancellation),
    )
    .await
}

#[tauri::command]
pub async fn workspace_status<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<WorkspaceIndexStatusDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    drop(app);

    jobs.run_blocking_latest(
        format!("workspace-status:{project_id}"),
        "workspace index status",
        move |cancellation| {
            cancellation.check_cancelled("workspace index status")?;
            workspace_status_at_root(&repo_root, &project_id)
        },
    )
    .await
}

#[tauri::command]
pub async fn workspace_query<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: WorkspaceQueryRequestDto,
) -> CommandResult<WorkspaceQueryResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.query, "query")?;

    let repo_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id.clone();
    drop(app);

    jobs.run_blocking_latest(
        format!("workspace-query:{project_id}"),
        "workspace index query",
        move |cancellation| {
            cancellation.check_cancelled("workspace index query")?;
            workspace_query_at_root(&repo_root, request)
        },
    )
    .await
}

#[tauri::command]
pub async fn workspace_explain<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: WorkspaceExplainRequestDto,
) -> CommandResult<WorkspaceExplainResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id.clone();
    drop(app);

    jobs.run_blocking_latest(
        format!("workspace-explain:{project_id}"),
        "workspace index explain",
        move |cancellation| {
            cancellation.check_cancelled("workspace index explain")?;
            workspace_explain_at_root(&repo_root, request)
        },
    )
    .await
}

#[tauri::command]
pub async fn workspace_reset<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<WorkspaceIndexStatusDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    drop(app);

    jobs.run_blocking_project_lane(
        project_id.clone(),
        "workspace-index",
        "workspace index reset",
        move || workspace_reset_at_root(&repo_root, &project_id),
    )
    .await
}

pub(crate) fn workspace_status_at_root(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<WorkspaceIndexStatusDto> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    let mut status = read_status_row(&connection, repo_root, project_id, &database_path)?
        .unwrap_or_else(|| empty_status(repo_root, project_id, &database_path));
    let scan = scan_workspace_candidates(
        repo_root,
        HARD_MAX_INDEX_FILES,
        &BackendCancellationToken::default(),
    )?;
    let indexed = read_indexed_file_fingerprints(&connection, project_id)?;
    let current_paths = scan
        .files
        .iter()
        .map(|candidate| candidate.virtual_path.clone())
        .collect::<BTreeSet<_>>();
    let stale_current = scan
        .files
        .iter()
        .filter(|candidate| {
            indexed
                .get(&candidate.virtual_path)
                .map(|fingerprint| {
                    fingerprint.modified_at != candidate.modified_at
                        || fingerprint.byte_length != candidate.byte_length
                })
                .unwrap_or(true)
        })
        .count();
    let removed = indexed
        .keys()
        .filter(|path| !current_paths.contains(*path))
        .count();
    status.total_files = scan.files.len() as u32;
    status.skipped_files = scan.skipped_files;
    status.stale_files = stale_current.saturating_add(removed) as u32;
    status.coverage_percent = coverage_percent(
        status.indexed_files.saturating_sub(removed as u32),
        status.total_files,
    );
    if status.indexed_files == 0 {
        status.state = WorkspaceIndexStateDto::Empty;
    } else if status.stale_files > 0 || status.head_sha != repository_head_sha(repo_root) {
        status.state = WorkspaceIndexStateDto::Stale;
    } else if status.state != WorkspaceIndexStateDto::Failed {
        status.state = WorkspaceIndexStateDto::Ready;
    }
    if scan.truncated {
        status.diagnostics.push(diagnostic(
            "warning",
            "workspace_index_status_scan_truncated",
            "Workspace status was estimated from the first indexed-file scan window.",
        ));
    }
    Ok(status)
}

pub(crate) fn workspace_query_at_root(
    repo_root: &Path,
    request: WorkspaceQueryRequestDto,
) -> CommandResult<WorkspaceQueryResponseDto> {
    let status = workspace_status_at_root(repo_root, &request.project_id)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    let query_embedding = project_store::embedding_for_storage(&query_embedding_text(&request))?;
    let query_tokens = tokenize_query(&request.query);
    let path_filters = normalized_path_filters(&request.paths);
    let limit = request
        .limit
        .map(|value| value.clamp(1, HARD_QUERY_LIMIT))
        .unwrap_or(DEFAULT_QUERY_LIMIT);
    let mut rows = read_workspace_file_rows(&connection, &request.project_id)?;
    if !path_filters.is_empty() {
        rows.retain(|row| {
            path_filters
                .iter()
                .any(|path| path_matches_filter(&row.path, path))
        });
    }

    let mut ranked = rows
        .into_iter()
        .filter_map(|row| {
            let embedding = serde_json::from_str::<Vec<f32>>(&row.embedding_json).ok()?;
            let semantic =
                project_store::cosine_similarity(&query_embedding.vector, embedding.as_slice());
            let lexical = lexical_score(&query_tokens, &row, request.mode);
            let score = score_for_mode(semantic, lexical.total, request.mode);
            if score <= 0.001 {
                return None;
            }
            Some(ScoredWorkspaceRow {
                row,
                score,
                reasons: lexical.reasons,
            })
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.row.path.cmp(&right.row.path))
    });

    let results = ranked
        .into_iter()
        .take(limit as usize)
        .enumerate()
        .map(|(index, scored)| row_to_query_result(index as u32 + 1, scored))
        .collect::<CommandResult<Vec<_>>>()?;
    let mut diagnostics = status.diagnostics.clone();
    if status.state == WorkspaceIndexStateDto::Empty {
        diagnostics.push(diagnostic(
            "warning",
            "workspace_index_empty",
            "No workspace index exists yet. Run workspace index before relying on semantic results.",
        ));
    } else if status.state == WorkspaceIndexStateDto::Stale {
        diagnostics.push(diagnostic(
            "warning",
            "workspace_index_stale",
            "Workspace index has stale or missing files. Results may omit recent changes.",
        ));
    }

    Ok(WorkspaceQueryResponseDto {
        project_id: request.project_id,
        query: request.query,
        mode: request.mode,
        result_count: results.len() as u32,
        stale: status.state == WorkspaceIndexStateDto::Stale,
        diagnostics,
        results,
    })
}

pub(crate) fn workspace_explain_at_root(
    repo_root: &Path,
    request: WorkspaceExplainRequestDto,
) -> CommandResult<WorkspaceExplainResponseDto> {
    let status = workspace_status_at_root(repo_root, &request.project_id)?;
    let mut top_signals = Vec::new();
    let mut diagnostics = status.diagnostics.clone();
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;

    if let Some(path) = request.path.as_deref().and_then(normalize_virtual_path) {
        if let Some(row) = read_workspace_file_row(&connection, &request.project_id, &path)? {
            top_signals.push(format!("{} is indexed as {}.", row.path, row.language));
            top_signals.push(row.summary.clone());
            let symbols = decode_string_array(&row.symbols_json, "symbols")?;
            if !symbols.is_empty() {
                top_signals.push(format!("Symbols: {}.", symbols.join(", ")));
            }
            let imports = decode_string_array(&row.imports_json, "imports")?;
            if !imports.is_empty() {
                top_signals.push(format!(
                    "Imports: {}.",
                    imports.into_iter().take(8).collect::<Vec<_>>().join(", ")
                ));
            }
            let diffs = decode_string_array(&row.diffs_json, "diffs")?;
            if !diffs.is_empty() {
                top_signals.push(format!("Recent diffs: {}.", diffs.join(", ")));
            }
            let failures = decode_string_array(&row.failures_json, "failures")?;
            if !failures.is_empty() {
                top_signals.push(format!(
                    "Recent failures: {}.",
                    failures.into_iter().take(3).collect::<Vec<_>>().join(" | ")
                ));
            }
        } else {
            diagnostics.push(diagnostic(
                "warning",
                "workspace_index_path_missing",
                "The requested path is not present in the workspace index.",
            ));
        }
    }

    if let Some(query) = request
        .query
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        let query_response = workspace_query_at_root(
            repo_root,
            WorkspaceQueryRequestDto {
                project_id: request.project_id.clone(),
                query: query.clone(),
                mode: WorkspaceQueryModeDto::Auto,
                limit: Some(5),
                paths: Vec::new(),
            },
        )?;
        for result in query_response.results {
            top_signals.push(format!(
                "{} scored {:.3}: {}",
                result.path,
                result.score,
                result.reasons.join("; ")
            ));
        }
        diagnostics.extend(query_response.diagnostics);
    }

    if top_signals.is_empty() {
        top_signals.push(format!(
            "Index state is {:?}; {} of {} files are indexed.",
            status.state, status.indexed_files, status.total_files
        ));
    }

    let summary = match status.state {
        WorkspaceIndexStateDto::Ready => "Workspace index is fresh and queryable.",
        WorkspaceIndexStateDto::Stale => "Workspace index is queryable but has stale coverage.",
        WorkspaceIndexStateDto::Empty => "Workspace index has not been built yet.",
        WorkspaceIndexStateDto::Indexing => "Workspace index is currently being rebuilt.",
        WorkspaceIndexStateDto::Failed => "Workspace index failed during the previous rebuild.",
    }
    .to_string();

    Ok(WorkspaceExplainResponseDto {
        project_id: request.project_id,
        summary,
        status,
        top_signals,
        diagnostics,
    })
}

fn workspace_reset_at_root(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<WorkspaceIndexStatusDto> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    connection
        .execute(
            "DELETE FROM workspace_index_files WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(|error| sqlite_error("workspace_index_reset_failed", error))?;
    connection
        .execute(
            "DELETE FROM workspace_index_metadata WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(|error| sqlite_error("workspace_index_reset_failed", error))?;
    Ok(empty_status(repo_root, project_id, &database_path))
}

fn index_workspace_at_root(
    repo_root: &Path,
    request: WorkspaceIndexRequestDto,
    cancellation: BackendCancellationToken,
) -> CommandResult<WorkspaceIndexResponseDto> {
    let project_id = request.project_id.clone();
    let result = index_workspace_at_root_inner(repo_root, request, cancellation);
    if let Err(error) = &result {
        let _ = mark_index_failed(repo_root, &project_id, error);
    }
    result
}

fn index_workspace_at_root_inner(
    repo_root: &Path,
    request: WorkspaceIndexRequestDto,
    cancellation: BackendCancellationToken,
) -> CommandResult<WorkspaceIndexResponseDto> {
    let started = Instant::now();
    let started_at = now_timestamp();
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_project_database(repo_root, &database_path)?;
    mark_indexing(
        &connection,
        repo_root,
        &request.project_id,
        &database_path,
        &started_at,
    )?;
    let max_files = request
        .max_files
        .map(|value| value.clamp(1, HARD_MAX_INDEX_FILES))
        .unwrap_or(DEFAULT_MAX_INDEX_FILES);
    let scan = scan_workspace_candidates(repo_root, max_files, &cancellation)?;
    let existing = read_indexed_file_fingerprints(&connection, &request.project_id)?;
    let external_signals = workspace_external_signals(&connection, repo_root, &request.project_id)?;
    let candidate_paths = scan
        .files
        .iter()
        .map(|candidate| candidate.virtual_path.clone())
        .collect::<BTreeSet<_>>();
    let mut rows = Vec::new();
    let mut changed_files = 0_u32;
    let mut unchanged_files = 0_u32;
    let mut indexed_bytes = 0_u64;
    let mut symbol_count = 0_u32;
    let now = now_timestamp();

    for candidate in scan.files {
        cancellation.check_cancelled("workspace index")?;
        let unchanged = !request.force
            && existing
                .get(&candidate.virtual_path)
                .map(|fingerprint| {
                    fingerprint.modified_at == candidate.modified_at
                        && fingerprint.byte_length == candidate.byte_length
                })
                .unwrap_or(false);
        if unchanged {
            unchanged_files += 1;
            continue;
        }
        let Some(row) = index_candidate(candidate, &request.project_id, &now, &external_signals)?
        else {
            continue;
        };
        indexed_bytes = indexed_bytes.saturating_add(row.byte_length as u64);
        symbol_count = symbol_count.saturating_add(row.symbols.len() as u32);
        changed_files += 1;
        rows.push(row);
    }

    let removed_paths = existing
        .keys()
        .filter(|path| !candidate_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    let removed_files = removed_paths.len() as u32;
    let tx = connection
        .transaction()
        .map_err(|error| sqlite_error("workspace_index_transaction_failed", error))?;
    for row in &rows {
        upsert_index_row(&tx, row)?;
    }
    for path in &removed_paths {
        tx.execute(
            "DELETE FROM workspace_index_files WHERE project_id = ?1 AND path = ?2",
            params![request.project_id, path],
        )
        .map_err(|error| sqlite_error("workspace_index_delete_removed_failed", error))?;
    }
    tx.commit()
        .map_err(|error| sqlite_error("workspace_index_commit_failed", error))?;

    let indexed_files = count_indexed_files(&connection, &request.project_id)?;
    if unchanged_files > 0 {
        let unchanged_stats = read_index_stats(&connection, &request.project_id)?;
        indexed_bytes = unchanged_stats.indexed_bytes;
        symbol_count = unchanged_stats.symbol_count;
    }
    let total_files = candidate_paths.len() as u32;
    let mut diagnostics = scan.diagnostics;
    if scan.truncated {
        diagnostics.push(diagnostic(
            "warning",
            "workspace_index_file_cap_reached",
            "Workspace indexing stopped at the configured file cap. Increase maxFiles for broader coverage.",
        ));
    }
    let status_state = if indexed_files == 0 {
        WorkspaceIndexStateDto::Empty
    } else if scan.truncated {
        WorkspaceIndexStateDto::Stale
    } else {
        WorkspaceIndexStateDto::Ready
    };
    let completed_at = now_timestamp();
    write_status_row(
        &connection,
        &StatusWrite {
            project_id: &request.project_id,
            state: status_state,
            repo_root,
            database_path: &database_path,
            total_files,
            indexed_files,
            skipped_files: scan.skipped_files,
            stale_files: if scan.truncated { 1 } else { 0 },
            symbol_count,
            indexed_bytes,
            diagnostics: &diagnostics,
            started_at: Some(&started_at),
            completed_at: Some(&completed_at),
            error: None,
        },
    )?;

    let status = read_status_row(&connection, repo_root, &request.project_id, &database_path)?
        .unwrap_or_else(|| empty_status(repo_root, &request.project_id, &database_path));

    Ok(WorkspaceIndexResponseDto {
        status,
        changed_files,
        unchanged_files,
        removed_files,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn mark_index_failed(
    repo_root: &Path,
    project_id: &str,
    error: &CommandError,
) -> CommandResult<()> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    let completed_at = now_timestamp();
    write_status_row(
        &connection,
        &StatusWrite {
            project_id,
            state: WorkspaceIndexStateDto::Failed,
            repo_root,
            database_path: &database_path,
            total_files: 0,
            indexed_files: count_indexed_files(&connection, project_id).unwrap_or(0),
            skipped_files: 0,
            stale_files: 0,
            symbol_count: 0,
            indexed_bytes: 0,
            diagnostics: &[],
            started_at: None,
            completed_at: Some(&completed_at),
            error: Some((&error.code, &error.message)),
        },
    )
}

#[derive(Debug, Clone)]
struct WorkspaceCandidate {
    absolute_path: PathBuf,
    virtual_path: String,
    language: String,
    modified_at: String,
    byte_length: i64,
}

#[derive(Debug, Clone)]
struct WorkspaceScan {
    files: Vec<WorkspaceCandidate>,
    skipped_files: u32,
    truncated: bool,
    diagnostics: Vec<WorkspaceIndexDiagnosticDto>,
}

#[derive(Debug, Clone)]
struct IndexedFingerprint {
    modified_at: String,
    byte_length: i64,
}

#[derive(Debug, Clone)]
struct IndexedWorkspaceRow {
    project_id: String,
    path: String,
    language: String,
    content_hash: String,
    modified_at: String,
    byte_length: i64,
    summary: String,
    snippet: String,
    symbols: Vec<String>,
    imports: Vec<String>,
    tests: Vec<String>,
    routes: Vec<String>,
    commands: Vec<String>,
    diffs: Vec<String>,
    failures: Vec<String>,
    embedding_json: String,
    embedding_model: String,
    embedding_version: String,
    indexed_at: String,
}

#[derive(Debug, Clone)]
struct StoredWorkspaceRow {
    path: String,
    language: String,
    content_hash: String,
    summary: String,
    snippet: String,
    symbols_json: String,
    imports_json: String,
    tests_json: String,
    routes_json: String,
    commands_json: String,
    diffs_json: String,
    failures_json: String,
    embedding_json: String,
    indexed_at: String,
}

#[derive(Debug, Clone)]
struct LexicalScore {
    total: f64,
    reasons: Vec<String>,
}

#[derive(Debug, Clone)]
struct ScoredWorkspaceRow {
    row: StoredWorkspaceRow,
    score: f64,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct IndexStats {
    indexed_bytes: u64,
    symbol_count: u32,
}

struct StatusWrite<'a> {
    project_id: &'a str,
    state: WorkspaceIndexStateDto,
    repo_root: &'a Path,
    database_path: &'a Path,
    total_files: u32,
    indexed_files: u32,
    skipped_files: u32,
    stale_files: u32,
    symbol_count: u32,
    indexed_bytes: u64,
    diagnostics: &'a [WorkspaceIndexDiagnosticDto],
    started_at: Option<&'a str>,
    completed_at: Option<&'a str>,
    error: Option<(&'a str, &'a str)>,
}

#[derive(Debug, Default)]
struct WorkspaceIndexExternalSignals {
    diffs_by_path: BTreeMap<String, Vec<String>>,
    failure_snippets: Vec<String>,
}

impl WorkspaceIndexExternalSignals {
    fn diff_signals_for_path(&self, path: &str) -> Vec<String> {
        self.diffs_by_path.get(path).cloned().unwrap_or_default()
    }

    fn failure_signals_for_path(&self, path: &str) -> Vec<String> {
        let path_key = path.trim_start_matches('/').to_lowercase();
        let file_name = Path::new(path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_lowercase();
        self.failure_snippets
            .iter()
            .filter(|snippet| {
                let lower = snippet.to_lowercase();
                lower.contains(&path_key) || (!file_name.is_empty() && lower.contains(&file_name))
            })
            .take(MAX_FAILURE_SIGNALS)
            .cloned()
            .collect()
    }
}

fn workspace_external_signals(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<WorkspaceIndexExternalSignals> {
    Ok(WorkspaceIndexExternalSignals {
        diffs_by_path: recent_diff_signals(repo_root),
        failure_snippets: read_recent_failure_snippets(connection, project_id)?,
    })
}

fn scan_workspace_candidates(
    repo_root: &Path,
    max_files: u32,
    cancellation: &BackendCancellationToken,
) -> CommandResult<WorkspaceScan> {
    let mut files = Vec::new();
    let mut skipped_files = 0_u32;
    let mut truncated = false;
    let mut diagnostics = Vec::new();
    let walker = WalkBuilder::new(repo_root)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .filter_entry(should_visit_workspace_entry)
        .build();

    for entry in walker {
        cancellation.check_cancelled("workspace index")?;
        let Ok(entry) = entry else {
            skipped_files = skipped_files.saturating_add(1);
            continue;
        };
        let Some(file_type) = entry.file_type() else {
            skipped_files = skipped_files.saturating_add(1);
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(language) = workspace_language(path) else {
            skipped_files = skipped_files.saturating_add(1);
            continue;
        };
        let Ok(metadata) = fs::metadata(path) else {
            skipped_files = skipped_files.saturating_add(1);
            continue;
        };
        if metadata.len() > MAX_INDEX_FILE_BYTES {
            skipped_files = skipped_files.saturating_add(1);
            continue;
        }
        let Some(virtual_path) = to_virtual_path(repo_root, path) else {
            skipped_files = skipped_files.saturating_add(1);
            continue;
        };
        if files.len() >= max_files as usize {
            truncated = true;
            break;
        }
        files.push(WorkspaceCandidate {
            absolute_path: path.to_path_buf(),
            virtual_path,
            language,
            modified_at: metadata_modified_at(&metadata),
            byte_length: metadata.len() as i64,
        });
    }
    files.sort_by(|left, right| left.virtual_path.cmp(&right.virtual_path));
    if skipped_files > 0 {
        diagnostics.push(diagnostic(
            "info",
            "workspace_index_skipped_files",
            format!("Skipped {skipped_files} non-source, oversized, ignored, or unreadable files."),
        ));
    }
    Ok(WorkspaceScan {
        files,
        skipped_files,
        truncated,
        diagnostics,
    })
}

fn should_visit_workspace_entry(entry: &ignore::DirEntry) -> bool {
    if !entry
        .file_type()
        .is_some_and(|file_type| file_type.is_dir())
    {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    !is_skipped_project_directory_name(&name) && name != ".xero"
}

fn index_candidate(
    candidate: WorkspaceCandidate,
    project_id: &str,
    indexed_at: &str,
    external_signals: &WorkspaceIndexExternalSignals,
) -> CommandResult<Option<IndexedWorkspaceRow>> {
    let Ok(content) = fs::read_to_string(&candidate.absolute_path) else {
        return Ok(None);
    };
    let mut features = extract_features(&candidate, &content);
    features.diffs = external_signals.diff_signals_for_path(&candidate.virtual_path);
    features.failures = external_signals.failure_signals_for_path(&candidate.virtual_path);
    let summary = summarize_file(&candidate, &features);
    let snippet = content.chars().take(MAX_SNIPPET_CHARS).collect::<String>();
    let content_hash = sha256_text(&content);
    let embedding_text = [
        candidate.virtual_path.as_str(),
        candidate.language.as_str(),
        summary.as_str(),
        &features.symbols.join(" "),
        &features.imports.join(" "),
        &features.tests.join(" "),
        &features.routes.join(" "),
        &features.commands.join(" "),
        &features.diffs.join(" "),
        &features.failures.join(" "),
        &snippet,
    ]
    .join("\n");
    let embedding = project_store::embedding_for_storage(&embedding_text)?;
    let embedding_json = serde_json::to_string(&embedding.vector).map_err(|error| {
        CommandError::system_fault(
            "workspace_index_embedding_serialize_failed",
            format!("Xero could not serialize a workspace-index embedding: {error}"),
        )
    })?;

    Ok(Some(IndexedWorkspaceRow {
        project_id: project_id.to_owned(),
        path: candidate.virtual_path,
        language: candidate.language,
        content_hash,
        modified_at: candidate.modified_at,
        byte_length: candidate.byte_length,
        summary,
        snippet,
        symbols: features.symbols,
        imports: features.imports,
        tests: features.tests,
        routes: features.routes,
        commands: features.commands,
        diffs: features.diffs,
        failures: features.failures,
        embedding_json,
        embedding_model: embedding.model,
        embedding_version: embedding.version,
        indexed_at: indexed_at.to_owned(),
    }))
}

#[derive(Debug, Default)]
struct FileFeatures {
    symbols: Vec<String>,
    imports: Vec<String>,
    tests: Vec<String>,
    routes: Vec<String>,
    commands: Vec<String>,
    diffs: Vec<String>,
    failures: Vec<String>,
}

fn extract_features(candidate: &WorkspaceCandidate, content: &str) -> FileFeatures {
    let mut features = FileFeatures::default();
    let mut previous_tauri_command = false;
    if candidate.virtual_path.contains("/routes/")
        || candidate.virtual_path.contains("/app/")
        || candidate.virtual_path.contains("/pages/")
    {
        features.routes.push(candidate.virtual_path.clone());
    }
    if is_test_path(&candidate.virtual_path) {
        features
            .tests
            .push(format!("test file {}", candidate.virtual_path));
    }

    for (line_index, raw_line) in content.lines().enumerate() {
        if line_index > 2_000
            && features.symbols.len() >= MAX_SYMBOLS
            && features.imports.len() >= MAX_IMPORTS
            && features.tests.len() >= MAX_TESTS
        {
            break;
        }
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if features.imports.len() < MAX_IMPORTS {
            if let Some(import) = import_from_line(line) {
                push_unique(&mut features.imports, import, MAX_IMPORTS);
            }
        }
        if features.tests.len() < MAX_TESTS {
            if let Some(test) = test_from_line(line, line_index + 1) {
                push_unique(&mut features.tests, test, MAX_TESTS);
            }
        }
        if line.contains("#[tauri::command]") {
            previous_tauri_command = true;
            push_unique(
                &mut features.commands,
                format!("tauri command marker at line {}", line_index + 1),
                MAX_FEATURES,
            );
            continue;
        }
        if features.symbols.len() < MAX_SYMBOLS {
            if let Some((kind, name)) = symbol_from_line(line) {
                let symbol = format!("{kind} {name}:{}", line_index + 1);
                if previous_tauri_command {
                    push_unique(
                        &mut features.commands,
                        format!("{name} at line {}", line_index + 1),
                        MAX_FEATURES,
                    );
                }
                push_unique(&mut features.symbols, symbol, MAX_SYMBOLS);
                previous_tauri_command = false;
            }
        }
    }
    features
}

fn import_from_line(line: &str) -> Option<String> {
    let keep = line
        .strip_prefix("import ")
        .or_else(|| line.strip_prefix("export "))
        .or_else(|| line.strip_prefix("use "))
        .or_else(|| line.strip_prefix("mod "))
        .or_else(|| line.strip_prefix("from "))?;
    let trimmed = keep.trim().trim_end_matches(';');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(160).collect())
    }
}

fn test_from_line(line: &str, line_number: usize) -> Option<String> {
    if line.starts_with("#[test]")
        || line.starts_with("#[tokio::test]")
        || line.starts_with("describe(")
        || line.starts_with("it(")
        || line.starts_with("test(")
        || line.contains("vitest")
    {
        Some(format!("test signal at line {line_number}"))
    } else {
        None
    }
}

fn symbol_from_line(line: &str) -> Option<(&'static str, String)> {
    let normalized = line
        .strip_prefix("pub ")
        .or_else(|| line.strip_prefix("export default "))
        .or_else(|| line.strip_prefix("export "))
        .or_else(|| line.strip_prefix("async "))
        .unwrap_or(line);
    for (prefix, kind) in [
        ("async fn ", "function"),
        ("fn ", "function"),
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("impl ", "impl"),
        ("function ", "function"),
        ("class ", "class"),
        ("interface ", "interface"),
        ("type ", "type"),
        ("const ", "constant"),
        ("let ", "binding"),
        ("def ", "function"),
        ("class ", "class"),
    ] {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            let name = rest
                .split(|character: char| {
                    character.is_whitespace()
                        || matches!(character, '(' | '<' | ':' | '=' | '{' | ';' | ',')
                })
                .next()
                .unwrap_or_default()
                .trim()
                .trim_matches(|character: char| !character.is_alphanumeric() && character != '_')
                .to_string();
            if !name.is_empty() {
                return Some((kind, name));
            }
        }
    }
    None
}

fn summarize_file(candidate: &WorkspaceCandidate, features: &FileFeatures) -> String {
    let mut parts = vec![format!(
        "{} source at {}",
        candidate.language, candidate.virtual_path
    )];
    if !features.symbols.is_empty() {
        parts.push(format!(
            "defines {}",
            features
                .symbols
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !features.imports.is_empty() {
        parts.push(format!(
            "imports {}",
            features
                .imports
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !features.tests.is_empty() {
        parts.push("contains test signals".into());
    }
    if !features.routes.is_empty() {
        parts.push("looks route/component-facing".into());
    }
    if !features.commands.is_empty() {
        parts.push("exposes Tauri command signals".into());
    }
    if !features.diffs.is_empty() {
        parts.push("has recent working-tree diff signals".into());
    }
    if !features.failures.is_empty() {
        parts.push("has recent build/test failure snippets".into());
    }
    parts.join("; ")
}

fn upsert_index_row(connection: &Connection, row: &IndexedWorkspaceRow) -> CommandResult<()> {
    connection
        .execute(
            "INSERT INTO workspace_index_files (
                project_id, path, language, content_hash, modified_at, byte_length,
                summary, snippet, symbols_json, imports_json, tests_json, routes_json,
                commands_json, diffs_json, failures_json, embedding_json, embedding_model,
                embedding_version, indexed_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            ON CONFLICT(project_id, path) DO UPDATE SET
                language = excluded.language,
                content_hash = excluded.content_hash,
                modified_at = excluded.modified_at,
                byte_length = excluded.byte_length,
                summary = excluded.summary,
                snippet = excluded.snippet,
                symbols_json = excluded.symbols_json,
                imports_json = excluded.imports_json,
                tests_json = excluded.tests_json,
                routes_json = excluded.routes_json,
                commands_json = excluded.commands_json,
                diffs_json = excluded.diffs_json,
                failures_json = excluded.failures_json,
                embedding_json = excluded.embedding_json,
                embedding_model = excluded.embedding_model,
                embedding_version = excluded.embedding_version,
                indexed_at = excluded.indexed_at",
            params![
                &row.project_id,
                &row.path,
                &row.language,
                &row.content_hash,
                &row.modified_at,
                row.byte_length,
                &row.summary,
                &row.snippet,
                json_array(&row.symbols)?,
                json_array(&row.imports)?,
                json_array(&row.tests)?,
                json_array(&row.routes)?,
                json_array(&row.commands)?,
                json_array(&row.diffs)?,
                json_array(&row.failures)?,
                &row.embedding_json,
                &row.embedding_model,
                &row.embedding_version,
                &row.indexed_at,
            ],
        )
        .map_err(|error| sqlite_error("workspace_index_file_write_failed", error))?;
    Ok(())
}

fn read_indexed_file_fingerprints(
    connection: &Connection,
    project_id: &str,
) -> CommandResult<BTreeMap<String, IndexedFingerprint>> {
    let mut stmt = connection
        .prepare(
            "SELECT path, modified_at, byte_length FROM workspace_index_files WHERE project_id = ?1",
        )
        .map_err(|error| sqlite_error("workspace_index_read_failed", error))?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                IndexedFingerprint {
                    modified_at: row.get(1)?,
                    byte_length: row.get(2)?,
                },
            ))
        })
        .map_err(|error| sqlite_error("workspace_index_read_failed", error))?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (path, fingerprint) =
            row.map_err(|error| sqlite_error("workspace_index_read_failed", error))?;
        map.insert(path, fingerprint);
    }
    Ok(map)
}

fn read_workspace_file_rows(
    connection: &Connection,
    project_id: &str,
) -> CommandResult<Vec<StoredWorkspaceRow>> {
    let mut stmt = connection
        .prepare(
            "SELECT path, language, content_hash, summary, snippet, symbols_json, imports_json,
                    tests_json, routes_json, commands_json, diffs_json, failures_json,
                    embedding_json, indexed_at
             FROM workspace_index_files WHERE project_id = ?1",
        )
        .map_err(|error| sqlite_error("workspace_index_query_prepare_failed", error))?;
    let rows = stmt
        .query_map(params![project_id], stored_row_from_sql)
        .map_err(|error| sqlite_error("workspace_index_query_failed", error))?;
    let mut output = Vec::new();
    for row in rows {
        output.push(row.map_err(|error| sqlite_error("workspace_index_query_failed", error))?);
    }
    Ok(output)
}

fn read_workspace_file_row(
    connection: &Connection,
    project_id: &str,
    path: &str,
) -> CommandResult<Option<StoredWorkspaceRow>> {
    connection
        .query_row(
            "SELECT path, language, content_hash, summary, snippet, symbols_json, imports_json,
                    tests_json, routes_json, commands_json, diffs_json, failures_json,
                    embedding_json, indexed_at
             FROM workspace_index_files WHERE project_id = ?1 AND path = ?2",
            params![project_id, path],
            stored_row_from_sql,
        )
        .optional()
        .map_err(|error| sqlite_error("workspace_index_query_failed", error))
}

fn stored_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredWorkspaceRow> {
    Ok(StoredWorkspaceRow {
        path: row.get(0)?,
        language: row.get(1)?,
        content_hash: row.get(2)?,
        summary: row.get(3)?,
        snippet: row.get(4)?,
        symbols_json: row.get(5)?,
        imports_json: row.get(6)?,
        tests_json: row.get(7)?,
        routes_json: row.get(8)?,
        commands_json: row.get(9)?,
        diffs_json: row.get(10)?,
        failures_json: row.get(11)?,
        embedding_json: row.get(12)?,
        indexed_at: row.get(13)?,
    })
}

fn count_indexed_files(connection: &Connection, project_id: &str) -> CommandResult<u32> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM workspace_index_files WHERE project_id = ?1",
            params![project_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count.max(0) as u32)
        .map_err(|error| sqlite_error("workspace_index_count_failed", error))
}

fn read_index_stats(connection: &Connection, project_id: &str) -> CommandResult<IndexStats> {
    let mut stmt = connection
        .prepare(
            "SELECT byte_length, symbols_json FROM workspace_index_files WHERE project_id = ?1",
        )
        .map_err(|error| sqlite_error("workspace_index_stats_failed", error))?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| sqlite_error("workspace_index_stats_failed", error))?;
    let mut stats = IndexStats {
        indexed_bytes: 0,
        symbol_count: 0,
    };
    for row in rows {
        let (bytes, symbols_json) =
            row.map_err(|error| sqlite_error("workspace_index_stats_failed", error))?;
        stats.indexed_bytes = stats.indexed_bytes.saturating_add(bytes.max(0) as u64);
        stats.symbol_count = stats
            .symbol_count
            .saturating_add(decode_string_array(&symbols_json, "symbols")?.len() as u32);
    }
    Ok(stats)
}

fn mark_indexing(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    database_path: &Path,
    started_at: &str,
) -> CommandResult<()> {
    write_status_row(
        connection,
        &StatusWrite {
            project_id,
            state: WorkspaceIndexStateDto::Indexing,
            repo_root,
            database_path,
            total_files: 0,
            indexed_files: count_indexed_files(connection, project_id).unwrap_or(0),
            skipped_files: 0,
            stale_files: 0,
            symbol_count: 0,
            indexed_bytes: 0,
            diagnostics: &[],
            started_at: Some(started_at),
            completed_at: None,
            error: None,
        },
    )
}

fn read_status_row(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    database_path: &Path,
) -> CommandResult<Option<WorkspaceIndexStatusDto>> {
    connection
        .query_row(
            "SELECT status, index_version, root_path, storage_path, head_sha, total_files,
                    indexed_files, skipped_files, stale_files, symbol_count, indexed_bytes,
                    coverage_percent, diagnostics_json, last_error_code, last_error_message,
                    started_at, completed_at, updated_at
             FROM workspace_index_metadata WHERE project_id = ?1",
            params![project_id],
            |row| {
                let diagnostics_json: String = row.get(12)?;
                let mut diagnostics =
                    serde_json::from_str::<Vec<WorkspaceIndexDiagnosticDto>>(&diagnostics_json)
                        .unwrap_or_default();
                let last_error_code: Option<String> = row.get(13)?;
                let last_error_message: Option<String> = row.get(14)?;
                if let (Some(code), Some(message)) = (last_error_code, last_error_message) {
                    diagnostics.push(WorkspaceIndexDiagnosticDto {
                        severity: "error".into(),
                        code,
                        message,
                    });
                }
                Ok(WorkspaceIndexStatusDto {
                    project_id: project_id.to_owned(),
                    state: parse_state(row.get::<_, String>(0)?.as_str()),
                    index_version: row.get::<_, i64>(1)?.max(1) as u32,
                    root_path: row.get(2)?,
                    storage_path: row.get(3)?,
                    head_sha: row.get(4)?,
                    total_files: row.get::<_, i64>(5)?.max(0) as u32,
                    indexed_files: row.get::<_, i64>(6)?.max(0) as u32,
                    skipped_files: row.get::<_, i64>(7)?.max(0) as u32,
                    stale_files: row.get::<_, i64>(8)?.max(0) as u32,
                    symbol_count: row.get::<_, i64>(9)?.max(0) as u32,
                    indexed_bytes: row.get::<_, i64>(10)?.max(0) as u64,
                    coverage_percent: row.get(11)?,
                    diagnostics,
                    started_at: row.get(15)?,
                    completed_at: row.get(16)?,
                    updated_at: row.get(17)?,
                })
            },
        )
        .optional()
        .map(|value| value.or_else(|| Some(empty_status(repo_root, project_id, database_path))))
        .map_err(|error| sqlite_error("workspace_index_status_read_failed", error))
}

fn write_status_row(connection: &Connection, write: &StatusWrite<'_>) -> CommandResult<()> {
    let diagnostics_json = serde_json::to_string(write.diagnostics).map_err(|error| {
        CommandError::system_fault(
            "workspace_index_diagnostics_serialize_failed",
            format!("Xero could not serialize workspace-index diagnostics: {error}"),
        )
    })?;
    let updated_at = now_timestamp();
    let state = state_label(write.state);
    let coverage_percent = coverage_percent(write.indexed_files, write.total_files);
    let root_path = write.repo_root.display().to_string();
    let storage_path = write
        .database_path
        .parent()
        .unwrap_or(write.database_path)
        .display()
        .to_string();
    let head_sha = repository_head_sha(write.repo_root);
    let fingerprint = workspace_fingerprint(write.repo_root);
    let (last_error_code, last_error_message) = write
        .error
        .map(|(code, message)| (Some(code.to_owned()), Some(message.to_owned())))
        .unwrap_or((None, None));
    connection
        .execute(
            "INSERT INTO workspace_index_metadata (
                project_id, status, index_version, root_path, storage_path, head_sha,
                worktree_fingerprint, total_files, indexed_files, skipped_files, stale_files,
                symbol_count, indexed_bytes, coverage_percent, diagnostics_json,
                last_error_code, last_error_message, started_at, completed_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
            ON CONFLICT(project_id) DO UPDATE SET
                status = excluded.status,
                index_version = excluded.index_version,
                root_path = excluded.root_path,
                storage_path = excluded.storage_path,
                head_sha = excluded.head_sha,
                worktree_fingerprint = excluded.worktree_fingerprint,
                total_files = excluded.total_files,
                indexed_files = excluded.indexed_files,
                skipped_files = excluded.skipped_files,
                stale_files = excluded.stale_files,
                symbol_count = excluded.symbol_count,
                indexed_bytes = excluded.indexed_bytes,
                coverage_percent = excluded.coverage_percent,
                diagnostics_json = excluded.diagnostics_json,
                last_error_code = excluded.last_error_code,
                last_error_message = excluded.last_error_message,
                started_at = excluded.started_at,
                completed_at = excluded.completed_at,
                updated_at = excluded.updated_at",
            params![
                write.project_id,
                state,
                WORKSPACE_INDEX_VERSION,
                root_path,
                storage_path,
                head_sha,
                fingerprint,
                write.total_files,
                write.indexed_files,
                write.skipped_files,
                write.stale_files,
                write.symbol_count,
                write.indexed_bytes as i64,
                coverage_percent,
                diagnostics_json,
                last_error_code,
                last_error_message,
                write.started_at,
                write.completed_at,
                updated_at,
            ],
        )
        .map_err(|error| sqlite_error("workspace_index_status_write_failed", error))?;
    Ok(())
}

fn empty_status(
    repo_root: &Path,
    project_id: &str,
    database_path: &Path,
) -> WorkspaceIndexStatusDto {
    WorkspaceIndexStatusDto {
        project_id: project_id.to_owned(),
        state: WorkspaceIndexStateDto::Empty,
        index_version: WORKSPACE_INDEX_VERSION,
        root_path: repo_root.display().to_string(),
        storage_path: database_path
            .parent()
            .unwrap_or(database_path)
            .display()
            .to_string(),
        total_files: 0,
        indexed_files: 0,
        skipped_files: 0,
        stale_files: 0,
        symbol_count: 0,
        indexed_bytes: 0,
        coverage_percent: 0.0,
        head_sha: repository_head_sha(repo_root),
        started_at: None,
        completed_at: None,
        updated_at: None,
        diagnostics: Vec::new(),
    }
}

fn row_to_query_result(
    rank: u32,
    scored: ScoredWorkspaceRow,
) -> CommandResult<WorkspaceQueryResultDto> {
    Ok(WorkspaceQueryResultDto {
        rank,
        path: scored.row.path,
        score: round_score(scored.score),
        language: scored.row.language,
        summary: scored.row.summary,
        snippet: scored.row.snippet,
        symbols: decode_string_array(&scored.row.symbols_json, "symbols")?,
        imports: decode_string_array(&scored.row.imports_json, "imports")?,
        tests: decode_string_array(&scored.row.tests_json, "tests")?,
        diffs: decode_string_array(&scored.row.diffs_json, "diffs")?,
        failures: decode_string_array(&scored.row.failures_json, "failures")?,
        reasons: scored.reasons,
        content_hash: scored.row.content_hash,
        indexed_at: scored.row.indexed_at,
    })
}

fn lexical_score(
    tokens: &[String],
    row: &StoredWorkspaceRow,
    mode: WorkspaceQueryModeDto,
) -> LexicalScore {
    if tokens.is_empty() {
        return LexicalScore {
            total: 0.0,
            reasons: vec!["empty query token set".into()],
        };
    }
    let symbols = decode_string_array(&row.symbols_json, "symbols").unwrap_or_default();
    let imports = decode_string_array(&row.imports_json, "imports").unwrap_or_default();
    let tests = decode_string_array(&row.tests_json, "tests").unwrap_or_default();
    let routes = decode_string_array(&row.routes_json, "routes").unwrap_or_default();
    let commands = decode_string_array(&row.commands_json, "commands").unwrap_or_default();
    let diffs = decode_string_array(&row.diffs_json, "diffs").unwrap_or_default();
    let failures = decode_string_array(&row.failures_json, "failures").unwrap_or_default();
    let mut score = 0.0_f64;
    let mut reasons = Vec::new();
    let path_l = row.path.to_lowercase();
    let summary_l = row.summary.to_lowercase();
    let symbols_l = symbols.join(" ").to_lowercase();
    let imports_l = imports.join(" ").to_lowercase();
    let tests_l = tests.join(" ").to_lowercase();
    let feature_l = [
        routes.join(" "),
        commands.join(" "),
        diffs.join(" "),
        failures.join(" "),
    ]
    .join(" ")
    .to_lowercase();

    for token in tokens {
        if path_l.contains(token) {
            score += 0.24;
            push_reason(&mut reasons, format!("path matches `{token}`"));
        }
        if symbols_l.contains(token) {
            score += 0.22;
            push_reason(&mut reasons, format!("symbol matches `{token}`"));
        }
        if summary_l.contains(token) {
            score += 0.12;
            push_reason(&mut reasons, format!("summary matches `{token}`"));
        }
        if imports_l.contains(token) {
            score += 0.1;
            push_reason(&mut reasons, format!("import/dependency matches `{token}`"));
        }
        if tests_l.contains(token) {
            score += 0.12;
            push_reason(&mut reasons, format!("test signal matches `{token}`"));
        }
        if feature_l.contains(token) {
            score += 0.1;
            push_reason(
                &mut reasons,
                format!("route/command/diff/failure signal matches `{token}`"),
            );
        }
    }
    match mode {
        WorkspaceQueryModeDto::Symbol if !symbols.is_empty() => {
            score += 0.15;
            push_reason(&mut reasons, "symbol-aware lookup boost".into());
        }
        WorkspaceQueryModeDto::RelatedTests if !tests.is_empty() || is_test_path(&row.path) => {
            score += 0.25;
            push_reason(&mut reasons, "related test discovery boost".into());
        }
        WorkspaceQueryModeDto::Impact => {
            if !imports.is_empty() {
                score += 0.12;
                push_reason(&mut reasons, "change-impact import graph signal".into());
            }
            if !diffs.is_empty() {
                score += 0.18;
                push_reason(&mut reasons, "recent diff impact signal".into());
            }
        }
        _ => {}
    }
    if !failures.is_empty() {
        score += 0.1;
        push_reason(&mut reasons, "recent build/test failure signal".into());
    }
    if reasons.is_empty() {
        reasons.push("semantic embedding similarity".into());
    }
    LexicalScore {
        total: score.min(1.0),
        reasons,
    }
}

fn score_for_mode(semantic: f64, lexical: f64, mode: WorkspaceQueryModeDto) -> f64 {
    let (semantic_weight, lexical_weight) = match mode {
        WorkspaceQueryModeDto::Semantic => (0.82, 0.18),
        WorkspaceQueryModeDto::Symbol => (0.4, 0.6),
        WorkspaceQueryModeDto::RelatedTests | WorkspaceQueryModeDto::Impact => (0.48, 0.52),
        WorkspaceQueryModeDto::Auto => (0.62, 0.38),
    };
    (semantic * semantic_weight + lexical * lexical_weight).min(1.0)
}

fn query_embedding_text(request: &WorkspaceQueryRequestDto) -> String {
    match request.mode {
        WorkspaceQueryModeDto::RelatedTests => {
            format!("tests specs verification related to {}", request.query)
        }
        WorkspaceQueryModeDto::Impact => {
            format!("change impact imports dependents {}", request.query)
        }
        WorkspaceQueryModeDto::Symbol => format!("symbol definition lookup {}", request.query),
        WorkspaceQueryModeDto::Semantic | WorkspaceQueryModeDto::Auto => request.query.clone(),
    }
}

fn recent_diff_signals(repo_root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut signals = BTreeMap::new();
    let Ok(repository) = Repository::discover(repo_root) else {
        return signals;
    };
    let Some(workdir) = repository.workdir() else {
        return signals;
    };
    let Ok(statuses) = repository.statuses(None) else {
        return signals;
    };
    for entry in statuses.iter() {
        let status = entry.status();
        if status.is_empty() || status.contains(Status::IGNORED) {
            continue;
        }
        let Some(path) = entry.path() else {
            continue;
        };
        let absolute_path = workdir.join(path);
        let Some(virtual_path) = to_virtual_path(repo_root, &absolute_path) else {
            continue;
        };
        let label = status_signal(status);
        push_unique_map_signal(
            &mut signals,
            virtual_path,
            format!("recent diff signal: {label}"),
            MAX_DIFF_SIGNALS,
        );
    }
    signals
}

fn status_signal(status: Status) -> String {
    let mut labels = Vec::new();
    for (flag, label) in [
        (Status::INDEX_NEW, "staged new"),
        (Status::INDEX_MODIFIED, "staged modified"),
        (Status::INDEX_DELETED, "staged deleted"),
        (Status::INDEX_RENAMED, "staged renamed"),
        (Status::WT_NEW, "worktree new"),
        (Status::WT_MODIFIED, "worktree modified"),
        (Status::WT_DELETED, "worktree deleted"),
        (Status::WT_RENAMED, "worktree renamed"),
        (Status::CONFLICTED, "conflicted"),
    ] {
        if status.contains(flag) {
            labels.push(label);
        }
    }
    if labels.is_empty() {
        "changed".into()
    } else {
        labels.join(", ")
    }
}

fn read_recent_failure_snippets(
    connection: &Connection,
    project_id: &str,
) -> CommandResult<Vec<String>> {
    let mut snippets = Vec::new();
    let mut tool_stmt = connection
        .prepare(
            "SELECT tool_name, error_message
             FROM agent_tool_calls
             WHERE project_id = ?1 AND state = 'failed' AND error_message IS NOT NULL
             ORDER BY COALESCE(completed_at, started_at) DESC
             LIMIT 60",
        )
        .map_err(|error| sqlite_error("workspace_index_failure_signal_read_failed", error))?;
    let tool_rows = tool_stmt
        .query_map(params![project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| sqlite_error("workspace_index_failure_signal_read_failed", error))?;
    for row in tool_rows {
        let (tool_name, message) =
            row.map_err(|error| sqlite_error("workspace_index_failure_signal_read_failed", error))?;
        if looks_like_build_or_test_failure(&message) {
            push_unique(
                &mut snippets,
                format!(
                    "recent failed tool `{tool_name}`: {}",
                    compact_signal_text(&message)
                ),
                MAX_FAILURE_SIGNALS * 4,
            );
        }
    }

    let mut checkpoint_stmt = connection
        .prepare(
            "SELECT summary, payload_json
             FROM agent_checkpoints
             WHERE project_id = ?1 AND checkpoint_kind IN ('validation', 'verification', 'failure')
             ORDER BY created_at DESC
             LIMIT 60",
        )
        .map_err(|error| sqlite_error("workspace_index_failure_signal_read_failed", error))?;
    let checkpoint_rows = checkpoint_stmt
        .query_map(params![project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|error| sqlite_error("workspace_index_failure_signal_read_failed", error))?;
    for row in checkpoint_rows {
        let (summary, payload) =
            row.map_err(|error| sqlite_error("workspace_index_failure_signal_read_failed", error))?;
        let combined = payload
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("{summary}\n{value}"))
            .unwrap_or(summary);
        if looks_like_build_or_test_failure(&combined) {
            push_unique(
                &mut snippets,
                format!(
                    "recent validation signal: {}",
                    compact_signal_text(&combined)
                ),
                MAX_FAILURE_SIGNALS * 4,
            );
        }
    }

    let mut event_stmt = connection
        .prepare(
            "SELECT event_kind, payload_json
             FROM agent_events
             WHERE project_id = ?1
               AND event_kind IN ('command_output', 'validation_completed', 'run_failed')
             ORDER BY id DESC
             LIMIT 80",
        )
        .map_err(|error| sqlite_error("workspace_index_failure_signal_read_failed", error))?;
    let event_rows = event_stmt
        .query_map(params![project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| sqlite_error("workspace_index_failure_signal_read_failed", error))?;
    for row in event_rows {
        let (event_kind, payload) =
            row.map_err(|error| sqlite_error("workspace_index_failure_signal_read_failed", error))?;
        if looks_like_build_or_test_failure(&payload) {
            push_unique(
                &mut snippets,
                format!(
                    "recent {event_kind} signal: {}",
                    compact_signal_text(&payload)
                ),
                MAX_FAILURE_SIGNALS * 4,
            );
        }
    }

    Ok(snippets)
}

fn looks_like_build_or_test_failure(value: &str) -> bool {
    let lower = value.to_lowercase();
    let has_failure = lower.contains("fail")
        || lower.contains("error")
        || lower.contains("panic")
        || lower.contains("exit code")
        || lower.contains("non-zero");
    let has_build_or_test = lower.contains("test")
        || lower.contains("build")
        || lower.contains("cargo")
        || lower.contains("pnpm")
        || lower.contains("npm")
        || lower.contains("tsc")
        || lower.contains("vitest")
        || lower.contains("clippy")
        || lower.contains("rustc");
    has_failure && has_build_or_test
}

fn compact_signal_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(360)
        .collect()
}

fn tokenize_query(query: &str) -> Vec<String> {
    query
        .split(|character: char| {
            !character.is_alphanumeric() && character != '_' && character != '-'
        })
        .map(|token| token.trim().to_lowercase())
        .filter(|token| token.len() >= 2)
        .take(24)
        .collect()
}

fn normalized_path_filters(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| normalize_virtual_path(path))
        .collect()
}

fn normalize_virtual_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let prefixed = if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    };
    Some(prefixed.trim_end_matches('/').to_owned())
}

fn path_matches_filter(path: &str, filter: &str) -> bool {
    path == filter
        || path
            .strip_prefix(filter)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn workspace_language(path: &Path) -> Option<String> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if matches!(
        file_name,
        "Cargo.toml"
            | "package.json"
            | "tsconfig.json"
            | "vite.config.ts"
            | "next.config.mjs"
            | "README.md"
            | "AGENTS.md"
    ) {
        return Some(file_name.to_owned());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())?
        .to_lowercase();
    let language = match extension.as_str() {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "typescript-react",
        "js" => "javascript",
        "jsx" => "javascript-react",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "ex" | "exs" => "elixir",
        "svelte" => "svelte",
        "vue" => "vue",
        "md" | "mdx" => "markdown",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "graphql" | "gql" => "graphql",
        "sql" => "sql",
        "sh" | "bash" | "zsh" => "shell",
        _ => return None,
    };
    Some(language.into())
}

fn is_test_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.contains("/tests/")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_test.py")
}

fn to_virtual_path(repo_root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(repo_root).ok()?;
    let parts = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(format!("/{}", parts.join("/")))
    }
}

fn sha256_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn json_array(values: &[String]) -> CommandResult<String> {
    serde_json::to_string(values).map_err(|error| {
        CommandError::system_fault(
            "workspace_index_json_serialize_failed",
            format!("Xero could not serialize workspace-index features: {error}"),
        )
    })
}

fn decode_string_array(value: &str, label: &'static str) -> CommandResult<Vec<String>> {
    serde_json::from_str::<Vec<String>>(value).map_err(|error| {
        CommandError::system_fault(
            "workspace_index_json_decode_failed",
            format!("Xero could not decode workspace-index {label}: {error}"),
        )
    })
}

fn push_unique(values: &mut Vec<String>, value: String, limit: usize) {
    if values.len() >= limit || values.iter().any(|existing| existing == &value) {
        return;
    }
    values.push(value);
}

fn push_unique_map_signal(
    values: &mut BTreeMap<String, Vec<String>>,
    path: String,
    value: String,
    limit: usize,
) {
    let entry = values.entry(path).or_default();
    push_unique(entry, value, limit);
}

fn push_reason(values: &mut Vec<String>, value: String) {
    push_unique(values, value, 8);
}

fn coverage_percent(indexed_files: u32, total_files: u32) -> f64 {
    if total_files == 0 {
        0.0
    } else {
        ((indexed_files as f64 / total_files as f64) * 100.0).clamp(0.0, 100.0)
    }
}

fn round_score(score: f64) -> f64 {
    (score * 1000.0).round() / 1000.0
}

fn parse_state(value: &str) -> WorkspaceIndexStateDto {
    match value {
        "indexing" => WorkspaceIndexStateDto::Indexing,
        "ready" => WorkspaceIndexStateDto::Ready,
        "stale" => WorkspaceIndexStateDto::Stale,
        "failed" => WorkspaceIndexStateDto::Failed,
        _ => WorkspaceIndexStateDto::Empty,
    }
}

fn state_label(state: WorkspaceIndexStateDto) -> &'static str {
    match state {
        WorkspaceIndexStateDto::Empty => "empty",
        WorkspaceIndexStateDto::Indexing => "indexing",
        WorkspaceIndexStateDto::Ready => "ready",
        WorkspaceIndexStateDto::Stale => "stale",
        WorkspaceIndexStateDto::Failed => "failed",
    }
}

fn repository_head_sha(repo_root: &Path) -> Option<String> {
    let repository = Repository::discover(repo_root).ok()?;
    let head = repository.head().ok()?;
    head.target().map(|oid| oid.to_string())
}

fn workspace_fingerprint(repo_root: &Path) -> Option<String> {
    let mut hasher = Sha256::new();
    hasher.update(repo_root.display().to_string().as_bytes());
    if let Some(head) = repository_head_sha(repo_root) {
        hasher.update(head.as_bytes());
    }
    Some(format!("{:x}", hasher.finalize()))
}

fn diagnostic(
    severity: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
) -> WorkspaceIndexDiagnosticDto {
    WorkspaceIndexDiagnosticDto {
        severity: severity.into(),
        code: code.into(),
        message: message.into(),
    }
}

fn sqlite_error(code: &'static str, error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        code,
        format!("Xero workspace index storage failed: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{configure_connection, migrations::migrations};
    use tempfile::tempdir;

    #[test]
    fn workspace_index_builds_and_queries_symbols() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("lib.rs"),
            "use crate::runtime;\n\npub fn search_workspace_index() {}\n",
        )
        .unwrap();
        let db_path = root.path().join("state.db");
        let mut connection = Connection::open(&db_path).unwrap();
        configure_connection(&connection).unwrap();
        migrations().to_latest(&mut connection).unwrap();
        connection
            .execute(
                "INSERT INTO projects (id, name) VALUES ('project-1', 'Project')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO repositories (id, project_id, root_path, display_name)
                 VALUES ('repo-1', 'project-1', ?1, 'Project')",
                params![root.path().display().to_string()],
            )
            .unwrap();
        crate::db::register_project_database_path_for_tests(root.path(), db_path);

        let response = index_workspace_at_root(
            root.path(),
            WorkspaceIndexRequestDto {
                project_id: "project-1".into(),
                force: false,
                max_files: None,
            },
            BackendCancellationToken::default(),
        )
        .unwrap();
        assert_eq!(response.status.indexed_files, 1);

        let query = workspace_query_at_root(
            root.path(),
            WorkspaceQueryRequestDto {
                project_id: "project-1".into(),
                query: "search workspace index".into(),
                mode: WorkspaceQueryModeDto::Symbol,
                limit: Some(5),
                paths: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(query.results[0].path, "/lib.rs");
    }
}
