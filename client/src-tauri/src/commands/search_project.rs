use std::{fs, path::Path};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use regex::{Regex, RegexBuilder};
use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        backend_jobs::BackendCancellationToken,
        payload_budget::{
            estimate_serialized_payload_bytes, payload_budget_diagnostic,
            PROJECT_SEARCH_RESULTS_BUDGET_BYTES,
        },
        project_files::{is_skipped_project_directory_entry, resolve_project_root},
        validate_non_empty, CommandError, CommandResult, ReplaceInProjectRequestDto,
        ReplaceInProjectResponseDto, SearchFileResultDto, SearchMatchDto, SearchProjectRequestDto,
        SearchProjectResponseDto,
    },
    state::DesktopState,
};

/// Cap on total matches returned so a sloppy query (e.g. single-char search on
/// a huge repo) can't balloon the IPC payload. UI surfaces `truncated = true`
/// when we hit this.
const DEFAULT_MAX_RESULTS: u32 = 5000;
/// Hard cap so even explicit client-requested higher limits don't uncap us.
const HARD_MAX_RESULTS: u32 = 20_000;
/// Page search responses by files so the UI can render reviewed result sets
/// without waiting for an unbounded match payload.
const DEFAULT_MAX_RESULT_FILES: u32 = 40;
const HARD_MAX_RESULT_FILES: u32 = 250;
/// Per-file size cutoff — skip anything larger to keep worst-case grep fast
/// and avoid loading minified bundles into memory.
const MAX_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;
/// Preview width around the match. Each match ships this many bytes total so
/// the UI doesn't have to render 2KB lines from minified files.
const PREVIEW_MAX_LEN: usize = 240;

#[tauri::command]
pub async fn search_project<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SearchProjectRequestDto,
) -> CommandResult<SearchProjectResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.query, "query")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let pattern = build_pattern(
        &request.query,
        request.case_sensitive,
        request.whole_word,
        request.regex,
    )?;
    let include = build_globset(&request.include_globs, "includeGlobs")?;
    let exclude = build_globset(&request.exclude_globs, "excludeGlobs")?;
    let cursor = validate_search_cursor(request.cursor.as_deref())?;

    let cap = request
        .max_results
        .map(|v| v.min(HARD_MAX_RESULTS))
        .unwrap_or(DEFAULT_MAX_RESULTS);
    let file_cap = request
        .max_files
        .map(|v| v.clamp(1, HARD_MAX_RESULT_FILES))
        .unwrap_or(DEFAULT_MAX_RESULT_FILES);
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    drop(app);

    jobs.run_blocking_latest(
        "project-search:visible",
        "project search",
        move |cancellation| {
            search_project_at_root(
                SearchProjectJob {
                    project_root,
                    project_id,
                    pattern,
                    include,
                    exclude,
                    cap,
                    file_cap,
                    cursor,
                },
                cancellation,
            )
        },
    )
    .await
}

struct SearchProjectJob {
    project_root: std::path::PathBuf,
    project_id: String,
    pattern: Regex,
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
    cap: u32,
    file_cap: u32,
    cursor: Option<String>,
}

fn search_project_at_root(
    job: SearchProjectJob,
    cancellation: BackendCancellationToken,
) -> CommandResult<SearchProjectResponseDto> {
    let SearchProjectJob {
        project_root,
        project_id,
        pattern,
        include,
        exclude,
        cap,
        file_cap,
        cursor,
    } = job;
    let mut total_matches: u32 = 0;
    let mut total_files: u32 = 0;
    let mut truncated = false;
    let mut files: Vec<SearchFileResultDto> = Vec::new();

    let mut next_cursor: Option<String> = None;
    let walker = project_walk_builder(&project_root).build();

    'walk: for entry in walker {
        cancellation.check_cancelled("project search")?;
        let Ok(entry) = entry else { continue };
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }

        let abs_path = entry.path();
        let Some(virtual_path) = to_virtual_path(&project_root, abs_path) else {
            continue;
        };
        if let Some(ref cursor) = cursor {
            if virtual_path <= *cursor {
                continue;
            }
        }

        if let Some(ref set) = include {
            if !set.is_match(virtual_path.trim_start_matches('/')) {
                continue;
            }
        }
        if let Some(ref set) = exclude {
            if set.is_match(virtual_path.trim_start_matches('/')) {
                continue;
            }
        }

        if let Ok(meta) = abs_path.metadata() {
            if meta.len() > MAX_FILE_SIZE_BYTES {
                continue;
            }
        }

        let Ok(contents) = fs::read_to_string(abs_path) else {
            // Non-UTF-8 / binary — skip silently.
            continue;
        };
        cancellation.check_cancelled("project search")?;

        let mut matches_in_file: Vec<SearchMatchDto> = Vec::new();
        for (line_idx, line) in contents.lines().enumerate() {
            if line_idx % 128 == 0 {
                cancellation.check_cancelled("project search")?;
            }
            if total_matches >= cap {
                truncated = true;
                if !matches_in_file.is_empty() {
                    files.push(SearchFileResultDto {
                        path: virtual_path.clone(),
                        matches: std::mem::take(&mut matches_in_file),
                    });
                    total_files += 1;
                }
                break 'walk;
            }

            for m in pattern.find_iter(line) {
                if total_matches >= cap {
                    truncated = true;
                    break;
                }
                let column = utf8_char_col(line, m.start());
                let (prefix, matched, suffix) = build_preview(line, m.start(), m.end());
                matches_in_file.push(SearchMatchDto {
                    line: (line_idx as u32) + 1,
                    column,
                    preview_prefix: prefix,
                    preview_match: matched,
                    preview_suffix: suffix,
                });
                total_matches += 1;
            }
        }

        if !matches_in_file.is_empty() {
            files.push(SearchFileResultDto {
                path: virtual_path.clone(),
                matches: matches_in_file,
            });
            total_files += 1;
            if total_files >= file_cap {
                truncated = true;
                next_cursor = Some(virtual_path);
                break;
            }
            if total_matches >= cap {
                truncated = true;
                break;
            }
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    let mut response = SearchProjectResponseDto {
        project_id,
        total_matches,
        total_files,
        truncated,
        next_cursor,
        payload_budget: None,
        files,
    };
    let observed_bytes = estimate_serialized_payload_bytes(&response);
    response.payload_budget = payload_budget_diagnostic(
        "project_search_results",
        "project search results",
        PROJECT_SEARCH_RESULTS_BUDGET_BYTES,
        observed_bytes,
        response.truncated,
        false,
    );

    Ok(response)
}

#[tauri::command]
pub async fn replace_in_project<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ReplaceInProjectRequestDto,
) -> CommandResult<ReplaceInProjectResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.query, "query")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let pattern = build_pattern(
        &request.query,
        request.case_sensitive,
        request.whole_word,
        request.regex,
    )?;
    let include = build_globset(&request.include_globs, "includeGlobs")?;
    let exclude = build_globset(&request.exclude_globs, "excludeGlobs")?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id.clone();
    drop(app);

    jobs.run_blocking_project_lane(project_id, "file", "project replace", move || {
        replace_in_project_at_root(project_root, request, pattern, include, exclude)
    })
    .await
}

fn replace_in_project_at_root(
    project_root: std::path::PathBuf,
    request: ReplaceInProjectRequestDto,
    pattern: Regex,
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
) -> CommandResult<ReplaceInProjectResponseDto> {
    // Explicit target set wins over globs for scoping — the UI uses this to
    // apply replacements to the exact files the user previewed.
    let targets: Option<std::collections::HashSet<String>> = request
        .target_paths
        .as_ref()
        .map(|paths| paths.iter().cloned().collect());

    let mut files_changed: u32 = 0;
    let mut total_replacements: u32 = 0;

    let walker = project_walk_builder(&project_root).build();

    for entry in walker {
        let Ok(entry) = entry else { continue };
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }

        let abs_path = entry.path();
        let Some(virtual_path) = to_virtual_path(&project_root, abs_path) else {
            continue;
        };

        if let Some(ref set) = targets {
            if !set.contains(&virtual_path) {
                continue;
            }
        } else {
            if let Some(ref set) = include {
                if !set.is_match(virtual_path.trim_start_matches('/')) {
                    continue;
                }
            }
            if let Some(ref set) = exclude {
                if set.is_match(virtual_path.trim_start_matches('/')) {
                    continue;
                }
            }
        }

        if let Ok(meta) = abs_path.metadata() {
            if meta.len() > MAX_FILE_SIZE_BYTES {
                continue;
            }
        }

        let Ok(contents) = fs::read_to_string(abs_path) else {
            continue;
        };

        let count = pattern.find_iter(&contents).count() as u32;
        if count == 0 {
            continue;
        }

        let replaced = if request.regex {
            pattern
                .replace_all(&contents, request.replacement.as_str())
                .into_owned()
        } else {
            // Literal replacement: expand only captures if user provided them?
            // For literal mode we treat the replacement string verbatim.
            pattern
                .replace_all(&contents, NoExpand(&request.replacement))
                .into_owned()
        };

        if replaced != contents {
            fs::write(abs_path, &replaced).map_err(|error| {
                CommandError::retryable(
                    "project_file_write_failed",
                    format!(
                        "Xero could not write replacements to `{}`: {error}",
                        virtual_path
                    ),
                )
            })?;
            files_changed += 1;
            total_replacements += count;
        }
    }

    Ok(ReplaceInProjectResponseDto {
        project_id: request.project_id,
        files_changed,
        total_replacements,
    })
}

fn build_pattern(
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    is_regex: bool,
) -> CommandResult<Regex> {
    let core = if is_regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    let pattern_string = if whole_word {
        format!(r"\b(?:{core})\b")
    } else {
        core
    };
    RegexBuilder::new(&pattern_string)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|error| {
            CommandError::user_fixable(
                "search_invalid_pattern",
                format!("Xero could not compile the search pattern: {error}"),
            )
        })
}

fn build_globset(globs: &[String], field: &'static str) -> CommandResult<Option<GlobSet>> {
    if globs.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for raw in globs {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let glob = Glob::new(trimmed).map_err(|error| {
            CommandError::user_fixable(
                "search_invalid_glob",
                format!("Xero could not parse the `{field}` pattern `{trimmed}`: {error}"),
            )
        })?;
        builder.add(glob);
    }
    let set = builder.build().map_err(|error| {
        CommandError::user_fixable(
            "search_invalid_glob",
            format!("Xero could not build the `{field}` pattern set: {error}"),
        )
    })?;
    if set.is_empty() {
        Ok(None)
    } else {
        Ok(Some(set))
    }
}

fn project_walk_builder(project_root: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(project_root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .parents(true)
        .follow_links(false)
        .filter_entry(|entry| !is_skipped_project_directory_entry(entry))
        .sort_by_file_path(|left, right| left.cmp(right));
    builder
}

fn validate_search_cursor(raw: Option<&str>) -> CommandResult<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed == "/" || !trimmed.starts_with('/') || trimmed.contains('\\') {
        return Err(CommandError::invalid_request("cursor"));
    }

    let stripped = trimmed.trim_start_matches('/');
    if stripped.is_empty()
        || stripped
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(CommandError::invalid_request("cursor"));
    }

    Ok(Some(trimmed.into()))
}

/// Convert an absolute path beneath `root` into the `/relative/path` form the
/// rest of the app uses. Returns None for paths that aren't inside the root
/// (shouldn't happen with our walker, but paranoia is free).
fn to_virtual_path(root: &Path, abs: &Path) -> Option<String> {
    let rel = abs.strip_prefix(root).ok()?;
    let as_str = rel.to_string_lossy();
    if as_str.is_empty() {
        return Some("/".into());
    }
    // Normalize separators to `/` on Windows-like paths.
    let normalized = as_str.replace('\\', "/");
    Some(format!("/{}", normalized))
}

/// Report column as a 1-based char offset, not byte offset, so UI jumps land
/// on the right character for multi-byte content.
fn utf8_char_col(line: &str, byte_offset: usize) -> u32 {
    let clamped = byte_offset.min(line.len());
    let prefix = &line[..clamped];
    (prefix.chars().count() as u32) + 1
}

/// Build a preview split into (prefix, match, suffix). Trims leading
/// whitespace and truncates long surrounding text so each row is compact.
fn build_preview(line: &str, match_start: usize, match_end: usize) -> (String, String, String) {
    let trimmed_start = line.len() - line.trim_start().len();
    let window_start = snap_char_boundary(line, match_start.saturating_sub(60).max(trimmed_start));
    let window_end = snap_char_boundary(line, (match_end + 120).min(line.len()));

    let mut prefix = line[window_start..match_start].to_string();
    let matched = line[match_start..match_end].to_string();
    let mut suffix = line[match_end..window_end].to_string();

    if window_start > trimmed_start {
        prefix.insert(0, '…');
    }
    if window_end < line.len() {
        suffix.push('…');
    }

    // Defensive cap on prefix/suffix independently so a single giant match
    // (e.g. minified file) can't blow out the payload.
    if prefix.len() > PREVIEW_MAX_LEN {
        let cut = snap_char_boundary(&prefix, prefix.len() - PREVIEW_MAX_LEN);
        prefix = format!("…{}", &prefix[cut..]);
    }
    if suffix.len() > PREVIEW_MAX_LEN {
        let cut = snap_char_boundary(&suffix, PREVIEW_MAX_LEN);
        suffix.truncate(cut);
        suffix.push('…');
    }

    (prefix, matched, suffix)
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

/// Wrapper for `regex::NoExpand` so we can pass replacements verbatim in
/// literal mode (no `$1` expansion surprises).
struct NoExpand<'a>(&'a str);

impl regex::Replacer for NoExpand<'_> {
    fn replace_append(&mut self, _caps: &regex::Captures<'_>, dst: &mut String) {
        dst.push_str(self.0);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::commands::backend_jobs::BackendCancellationToken;

    use super::{
        build_pattern, build_preview, search_project_at_root, to_virtual_path, utf8_char_col,
        validate_search_cursor, SearchProjectJob,
    };

    #[test]
    fn pattern_literal_case_insensitive() {
        let re = build_pattern("Foo", false, false, false).unwrap();
        assert!(re.is_match("contains foo somewhere"));
        assert!(re.is_match("Foo at the front"));
    }

    #[test]
    fn pattern_whole_word() {
        let re = build_pattern("foo", true, true, false).unwrap();
        assert!(re.is_match("say foo please"));
        assert!(!re.is_match("foobar"));
    }

    #[test]
    fn pattern_regex_compiles() {
        let re = build_pattern("fo.+", true, false, true).unwrap();
        assert!(re.is_match("food"));
    }

    #[test]
    fn virtual_path_strips_root() {
        let root = std::env::temp_dir().join("project");
        let abs = root.join("src").join("foo.ts");
        assert_eq!(to_virtual_path(&root, &abs).as_deref(), Some("/src/foo.ts"));
    }

    #[test]
    fn preview_keeps_match_visible() {
        let line = "    const greeting = \"hello world\";";
        let (_prefix, matched, _suffix) = build_preview(line, 22, 27);
        assert_eq!(matched, "hello");
    }

    #[test]
    fn utf8_column_counts_chars_not_bytes() {
        // 'é' is 2 bytes in UTF-8; the 'x' after it is char 3 (1-based: 4 for 'x').
        let line = "a é x";
        // Byte 3 is the start of space after 'é' (é = bytes 2-3, so byte 4 is 'x').
        let col = utf8_char_col(line, 4);
        assert_eq!(col, 4);
    }

    #[test]
    fn project_search_pages_results_by_file_with_stable_cursor() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        fs::write(temp_dir.path().join("a.txt"), "needle\n").expect("a");
        fs::write(temp_dir.path().join("b.txt"), "needle\n").expect("b");
        fs::write(temp_dir.path().join("c.txt"), "needle\n").expect("c");
        let pattern = build_pattern("needle", true, false, false).expect("pattern");

        let first = search_project_at_root(
            SearchProjectJob {
                project_root: temp_dir.path().to_path_buf(),
                project_id: "project-1".into(),
                pattern: pattern.clone(),
                include: None,
                exclude: None,
                cap: 100,
                file_cap: 2,
                cursor: None,
            },
            BackendCancellationToken::default(),
        )
        .expect("first page");
        let second = search_project_at_root(
            SearchProjectJob {
                project_root: temp_dir.path().to_path_buf(),
                project_id: "project-1".into(),
                pattern,
                include: None,
                exclude: None,
                cap: 100,
                file_cap: 2,
                cursor: first.next_cursor.clone(),
            },
            BackendCancellationToken::default(),
        )
        .expect("second page");

        assert_eq!(
            first
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["/a.txt", "/b.txt"]
        );
        assert_eq!(first.next_cursor.as_deref(), Some("/b.txt"));
        assert!(first.truncated);
        assert_eq!(
            second
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["/c.txt"]
        );
    }

    #[test]
    fn project_search_uses_same_ignore_rules_as_project_tree() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        fs::create_dir(temp_dir.path().join(".git")).expect("git dir");
        fs::write(temp_dir.path().join(".gitignore"), "ignored.txt\n").expect("gitignore");
        fs::write(temp_dir.path().join("visible.txt"), "needle").expect("visible");
        fs::write(temp_dir.path().join("ignored.txt"), "needle").expect("ignored");
        fs::create_dir(temp_dir.path().join("node_modules")).expect("node_modules");
        fs::write(
            temp_dir.path().join("node_modules").join("dep.txt"),
            "needle",
        )
        .expect("dep");
        let pattern = build_pattern("needle", true, false, false).expect("pattern");

        let response = search_project_at_root(
            SearchProjectJob {
                project_root: temp_dir.path().to_path_buf(),
                project_id: "project-1".into(),
                pattern,
                include: None,
                exclude: None,
                cap: 100,
                file_cap: 10,
                cursor: None,
            },
            BackendCancellationToken::default(),
        )
        .expect("search");

        assert_eq!(
            response
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["/visible.txt"]
        );
    }

    #[test]
    fn project_search_rejects_unsafe_cursors() {
        assert_eq!(
            validate_search_cursor(Some("/src/main.rs")).expect("cursor"),
            Some("/src/main.rs".into())
        );
        assert!(validate_search_cursor(Some("../escape")).is_err());
        assert!(validate_search_cursor(Some("/src/../escape")).is_err());
        assert!(validate_search_cursor(Some("/")).is_err());
    }
}
