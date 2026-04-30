use std::{fs, path::Path};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use regex::{Regex, RegexBuilder};
use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        project_files::resolve_project_root, validate_non_empty, CommandError, CommandResult,
        ReplaceInProjectRequestDto, ReplaceInProjectResponseDto, SearchFileResultDto,
        SearchMatchDto, SearchProjectRequestDto, SearchProjectResponseDto,
    },
    state::DesktopState,
};

/// Cap on total matches returned so a sloppy query (e.g. single-char search on
/// a huge repo) can't balloon the IPC payload. UI surfaces `truncated = true`
/// when we hit this.
const DEFAULT_MAX_RESULTS: u32 = 5000;
/// Hard cap so even explicit client-requested higher limits don't uncap us.
const HARD_MAX_RESULTS: u32 = 20_000;
/// Per-file size cutoff — skip anything larger to keep worst-case grep fast
/// and avoid loading minified bundles into memory.
const MAX_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;
/// Preview width around the match. Each match ships this many bytes total so
/// the UI doesn't have to render 2KB lines from minified files.
const PREVIEW_MAX_LEN: usize = 240;

#[tauri::command]
pub fn search_project<R: Runtime>(
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

    let cap = request
        .max_results
        .map(|v| v.min(HARD_MAX_RESULTS))
        .unwrap_or(DEFAULT_MAX_RESULTS);

    let mut total_matches: u32 = 0;
    let mut total_files: u32 = 0;
    let mut truncated = false;
    let mut files: Vec<SearchFileResultDto> = Vec::new();

    let walker = WalkBuilder::new(&project_root)
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .parents(true)
        .follow_links(false)
        .build();

    'walk: for entry in walker {
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

        let mut matches_in_file: Vec<SearchMatchDto> = Vec::new();
        for (line_idx, line) in contents.lines().enumerate() {
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
                path: virtual_path,
                matches: matches_in_file,
            });
            total_files += 1;
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(SearchProjectResponseDto {
        project_id: request.project_id,
        total_matches,
        total_files,
        truncated,
        files,
    })
}

#[tauri::command]
pub fn replace_in_project<R: Runtime>(
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

    // Explicit target set wins over globs for scoping — the UI uses this to
    // apply replacements to the exact files the user previewed.
    let targets: Option<std::collections::HashSet<String>> = request
        .target_paths
        .as_ref()
        .map(|paths| paths.iter().cloned().collect());

    let mut files_changed: u32 = 0;
    let mut total_replacements: u32 = 0;

    let walker = WalkBuilder::new(&project_root)
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .parents(true)
        .follow_links(false)
        .build();

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
    use super::{build_pattern, build_preview, to_virtual_path, utf8_char_col};
    use std::path::PathBuf;

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
        let root = PathBuf::from("/tmp/project");
        let abs = PathBuf::from("/tmp/project/src/foo.ts");
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
}
