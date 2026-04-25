//! Scan Rust source for PDA-derivation sites.
//!
//! Heuristic parser that walks the crate sources under a project root and
//! flags every call that looks like it derives a PDA. Focuses on the
//! canonical entry points used in Anchor + native Solana programs:
//!
//! - `Pubkey::find_program_address(&[...], program_id)`
//! - `Pubkey::create_program_address(&[...], program_id)`
//! - `anchor_lang::solana_program::pubkey::Pubkey::find_program_address(...)`
//! - Bare `find_program_address(...)` calls (Anchor macro expansion or
//!   brought into scope with `use solana_program::pubkey::*`).
//!
//! The scanner is deliberately regex-based rather than syn-based:
//! - The project surface is small (a single Anchor program directory).
//! - syn would require pinning a version that matches the target code.
//! - We flag *candidates*, not rewrites — false positives are fine.
//!
//! For each site we record the file, line, the raw seed expression, and
//! whether the caller passes `&[...]` (canonical-bump derivation) vs
//! `&[..., &[bump]]` with a literal (non-canonical, potentially unsafe).

use std::fs;
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

const MAX_FILE_BYTES: u64 = 512 * 1024; // 512 KB guard for absurd .rs files

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PdaSite {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub call: String,
    pub seed_expression: String,
    pub seed_kind: PdaSiteSeedKind,
    pub has_literal_bump: bool,
    pub hardcoded_bump: Option<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PdaSiteSeedKind {
    /// `find_program_address` — canonical-bump derivation.
    FindProgramAddress,
    /// `create_program_address` — caller-supplied bump; worth flagging
    /// so the reviewer can verify it's not hardcoded.
    CreateProgramAddress,
}

pub fn scan_project(project_root: &Path) -> CommandResult<Vec<PdaSite>> {
    if !project_root.is_dir() {
        return Err(CommandError::user_fixable(
            "solana_pda_scan_bad_root",
            format!(
                "PDA scan root {} is not a directory.",
                project_root.display()
            ),
        ));
    }
    let mut sites = Vec::new();
    walk(project_root, &mut sites)?;
    Ok(sites)
}

fn walk(dir: &Path, sites: &mut Vec<PdaSite>) -> CommandResult<()> {
    let entries = match fs::read_dir(dir) {
        Ok(it) => it,
        Err(err) => {
            return Err(CommandError::system_fault(
                "solana_pda_scan_read_dir_failed",
                format!("Could not read {}: {err}", dir.display()),
            ))
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if should_skip_entry(&path) {
            continue;
        }
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_dir() {
            walk(&path, sites)?;
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() > MAX_FILE_BYTES {
                continue;
            }
        }
        if let Ok(contents) = fs::read_to_string(&path) {
            scan_text(&path, &contents, sites);
        }
    }
    Ok(())
}

fn should_skip_entry(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return true,
    };
    matches!(
        name,
        "target"
            | ".git"
            | "node_modules"
            | ".anchor"
            | ".cargo"
            | "dist"
            | "build"
            | ".next"
            | ".svelte-kit"
    )
}

fn scan_text(path: &Path, text: &str, sites: &mut Vec<PdaSite>) {
    // Two regexes so we can cleanly tag the kind. Both match calls
    // where the closing paren is on the same line — we deliberately
    // skip multi-line invocations because parsing balanced parens in
    // Rust source from a regex is a losing game. The typical Anchor
    // program keeps PDA calls on one or two lines.
    // Seed lists may contain one level of nested brackets (the bump
    // byte is commonly written as `&[bump]`). The inner group matches
    // either non-bracket chars or a balanced single-level `[...]` group.
    let find_re = Regex::new(
        r"find_program_address\s*\(\s*&\[(?P<seeds>(?:[^\[\]]|\[[^\[\]]*\])*)\]\s*,\s*(?P<prog>[^\)]+)\)",
    )
    .unwrap();
    let create_re = Regex::new(
        r"create_program_address\s*\(\s*&\[(?P<seeds>(?:[^\[\]]|\[[^\[\]]*\])*)\]\s*,\s*(?P<prog>[^\)]+)\)",
    )
    .unwrap();

    for (line_idx, line) in text.lines().enumerate() {
        if let Some(m) = find_re.captures(line) {
            let (seed_expr, has_lit_bump, hardcoded) = classify_seeds(&m["seeds"]);
            let whole = m.get(0).unwrap();
            sites.push(PdaSite {
                path: path.display().to_string(),
                line: line_idx + 1,
                column: whole.start() + 1,
                call: whole.as_str().to_string(),
                seed_expression: seed_expr,
                seed_kind: PdaSiteSeedKind::FindProgramAddress,
                has_literal_bump: has_lit_bump,
                hardcoded_bump: hardcoded,
            });
        }
        if let Some(m) = create_re.captures(line) {
            let (seed_expr, has_lit_bump, hardcoded) = classify_seeds(&m["seeds"]);
            let whole = m.get(0).unwrap();
            sites.push(PdaSite {
                path: path.display().to_string(),
                line: line_idx + 1,
                column: whole.start() + 1,
                call: whole.as_str().to_string(),
                seed_expression: seed_expr,
                seed_kind: PdaSiteSeedKind::CreateProgramAddress,
                // `create_program_address` always takes an explicit bump
                // argument in at least one seed component — flagging it
                // as "has a literal bump" is noisy; instead we flag the
                // kind so reviewers know this is the bump-risky variant.
                has_literal_bump: has_lit_bump,
                hardcoded_bump: hardcoded,
            });
        }
    }
}

/// Look at the seeds list (the inside of the `&[...]`) and check for a
/// hardcoded bump byte at the tail. A literal bump looks like
/// `&[42u8][..]` or `&[0u8]`. Safer call sites either omit the bump
/// (find_program_address path) or use `&[bump_seed]` where `bump_seed` is
/// loaded from the Anchor context `ctx.bumps.foo`.
fn classify_seeds(expr: &str) -> (String, bool, Option<u8>) {
    let trimmed = expr.trim().trim_end_matches(',').trim();
    // Cheap check: does the trailing seed component look like a literal?
    let tail = trimmed.rsplit(',').next().unwrap_or(trimmed).trim();
    let maybe_literal = extract_literal_u8(tail);
    let has_literal = maybe_literal.is_some();
    (trimmed.to_string(), has_literal, maybe_literal)
}

fn extract_literal_u8(s: &str) -> Option<u8> {
    // Accept forms:
    //   &[42u8]
    //   &[42]
    //   &[0xff]
    //   &[bump_const]
    // Only the first three are "literal" — the last resolves via a
    // const. We detect the integer-literal forms.
    let stripped = s
        .trim()
        .trim_start_matches('&')
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim()
        .trim_end_matches("_u8")
        .trim_end_matches("u8");
    if let Some(hex) = stripped
        .strip_prefix("0x")
        .or_else(|| stripped.strip_prefix("0X"))
    {
        return u8::from_str_radix(hex, 16).ok();
    }
    stripped.parse::<u8>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn finds_basic_find_program_address_call() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("lib.rs");
        fs::write(
            &path,
            r#"
            fn x() {
                let (pda, bump) = Pubkey::find_program_address(&[b"vault", user.key.as_ref()], &PROGRAM_ID);
            }
            "#,
        )
        .unwrap();
        let sites = scan_project(tmp.path()).unwrap();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].seed_kind, PdaSiteSeedKind::FindProgramAddress);
        assert!(!sites[0].has_literal_bump);
    }

    #[test]
    fn flags_create_program_address_as_bump_risky() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("lib.rs");
        fs::write(
            &path,
            "let pda = Pubkey::create_program_address(&[b\"vault\", &[42u8]], &PROGRAM_ID);",
        )
        .unwrap();
        let sites = scan_project(tmp.path()).unwrap();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].seed_kind, PdaSiteSeedKind::CreateProgramAddress);
        assert!(sites[0].has_literal_bump);
        assert_eq!(sites[0].hardcoded_bump, Some(42));
    }

    #[test]
    fn skips_target_directories() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("target/debug");
        fs::create_dir_all(&target).unwrap();
        let decoy = target.join("junk.rs");
        fs::write(
            &decoy,
            "let (p, b) = Pubkey::find_program_address(&[b\"x\"], &ID);",
        )
        .unwrap();
        let sites = scan_project(tmp.path()).unwrap();
        assert!(sites.is_empty(), "sites inside target/ should be ignored");
    }

    #[test]
    fn scans_nested_source_tree() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("programs/demo/src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("lib.rs"),
            r#"
            pub fn init() {
                let _ = find_program_address(&[b"foo"], &ID);
                let _ = create_program_address(&[b"foo", &[0xff]], &ID);
            }
            "#,
        )
        .unwrap();
        let sites = scan_project(tmp.path()).unwrap();
        assert_eq!(sites.len(), 2);
        let kinds: Vec<_> = sites.iter().map(|s| s.seed_kind).collect();
        assert!(kinds.contains(&PdaSiteSeedKind::FindProgramAddress));
        assert!(kinds.contains(&PdaSiteSeedKind::CreateProgramAddress));
    }

    #[test]
    fn returns_error_for_missing_root() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("nope");
        let err = scan_project(&missing).unwrap_err();
        assert_eq!(err.code, "solana_pda_scan_bad_root");
    }

    #[test]
    fn extracts_literal_bump_in_various_forms() {
        assert_eq!(extract_literal_u8("&[0u8]"), Some(0));
        assert_eq!(extract_literal_u8("&[42]"), Some(42));
        assert_eq!(extract_literal_u8("&[0xff]"), Some(255));
        assert_eq!(extract_literal_u8("&[bump_seed]"), None);
    }
}
