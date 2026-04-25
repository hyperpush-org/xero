//! Anchor footgun static lints.
//!
//! Fast, zero-dependency Rust-source scan that flags the most common
//! Solana / Anchor security mistakes before anything else runs. We don't
//! try to be a full static analyzer — the goal is high-signal, low-noise
//! lints the user can fix in a few seconds. Deeper analysis is delegated
//! to `sec3.rs` (Sec3 / Soteria / Aderyn).
//!
//! Lints implemented:
//! - `missing_signer`: `#[derive(Accounts)]` struct has no `Signer` /
//!   `#[account(signer)]` field.
//! - `missing_owner_check`: raw `AccountInfo` field with no
//!   `#[account(owner = ...)]` attribute.
//! - `missing_has_one`: mutable `Account` / `Account<'_, T>` referencing
//!   another field without a `has_one` constraint.
//! - `unchecked_account_info`: `AccountInfo` / `UncheckedAccount` field with
//!   no `/// CHECK` safety comment.
//! - `arithmetic_overflow`: plain `+` / `-` / `*` between integer-looking
//!   expressions with no checked or saturating equivalent nearby.
//! - `realloc_without_rent`: `realloc(` call without a `Rent::get` or
//!   rent-exemption bump on the same span.
//! - `seed_spoof`: PDA derivation using seeds that include
//!   `AccountInfo::key` without a matching `seeds` / `bump` constraint.

use std::fs;
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

use super::{Finding, FindingSeverity, FindingSource, SeverityCounts};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct StaticLintRequest {
    /// Absolute path to a cargo / anchor project root.
    pub project_root: String,
    /// When set, restrict the scan to rules with these ids. Defaults to
    /// all rules.
    #[serde(default)]
    pub rule_ids: Vec<String>,
    /// Extra glob-ish path prefixes to skip (on top of the defaults
    /// like `target/`, `.git/`, `node_modules/`).
    #[serde(default)]
    pub skip_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum StaticLintRule {
    MissingSigner,
    MissingOwnerCheck,
    MissingHasOne,
    UncheckedAccountInfo,
    ArithmeticOverflow,
    ReallocWithoutRent,
    SeedSpoof,
}

impl StaticLintRule {
    pub fn all() -> &'static [StaticLintRule] {
        &[
            StaticLintRule::MissingSigner,
            StaticLintRule::MissingOwnerCheck,
            StaticLintRule::MissingHasOne,
            StaticLintRule::UncheckedAccountInfo,
            StaticLintRule::ArithmeticOverflow,
            StaticLintRule::ReallocWithoutRent,
            StaticLintRule::SeedSpoof,
        ]
    }

    pub fn id(self) -> &'static str {
        match self {
            StaticLintRule::MissingSigner => "missing_signer",
            StaticLintRule::MissingOwnerCheck => "missing_owner_check",
            StaticLintRule::MissingHasOne => "missing_has_one",
            StaticLintRule::UncheckedAccountInfo => "unchecked_account_info",
            StaticLintRule::ArithmeticOverflow => "arithmetic_overflow",
            StaticLintRule::ReallocWithoutRent => "realloc_without_rent",
            StaticLintRule::SeedSpoof => "seed_spoof",
        }
    }

    pub fn severity(self) -> FindingSeverity {
        match self {
            StaticLintRule::MissingSigner => FindingSeverity::Critical,
            StaticLintRule::MissingOwnerCheck => FindingSeverity::High,
            StaticLintRule::MissingHasOne => FindingSeverity::High,
            StaticLintRule::UncheckedAccountInfo => FindingSeverity::Medium,
            StaticLintRule::ArithmeticOverflow => FindingSeverity::Medium,
            StaticLintRule::ReallocWithoutRent => FindingSeverity::High,
            StaticLintRule::SeedSpoof => FindingSeverity::High,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            StaticLintRule::MissingSigner => "Accounts struct missing a Signer",
            StaticLintRule::MissingOwnerCheck => "Raw AccountInfo without owner check",
            StaticLintRule::MissingHasOne => "Cross-account reference without has_one",
            StaticLintRule::UncheckedAccountInfo => {
                "UncheckedAccount / AccountInfo missing CHECK comment"
            }
            StaticLintRule::ArithmeticOverflow => "Unchecked integer arithmetic",
            StaticLintRule::ReallocWithoutRent => "realloc() without rent-exempt bump",
            StaticLintRule::SeedSpoof => "PDA seed may be user-spoofable",
        }
    }

    pub fn fix_hint(self) -> &'static str {
        match self {
            StaticLintRule::MissingSigner => {
                "Add a `Signer<'info>` field or mark an existing account with \
                 `#[account(signer)]` so the instruction can only be authorised \
                 by a real signer."
            }
            StaticLintRule::MissingOwnerCheck => {
                "Constrain the account with `#[account(owner = some_program::ID)]` \
                 or wrap it in a typed `Account<'_, T>` so Anchor enforces the \
                 program that owns it."
            }
            StaticLintRule::MissingHasOne => {
                "Add a `has_one = other_account` constraint so Anchor verifies \
                 the relationship between the two accounts before the handler \
                 runs."
            }
            StaticLintRule::UncheckedAccountInfo => {
                "Document the manual safety check with a `/// CHECK:` comment \
                 immediately above the field, or wrap the field in a typed \
                 `Account<'_, T>`."
            }
            StaticLintRule::ArithmeticOverflow => {
                "Use `checked_add` / `checked_sub` / `checked_mul` (or `saturating_*` \
                 / `wrapping_*` when the over-/underflow is intentional) so a \
                 hostile input cannot silently wrap integer state."
            }
            StaticLintRule::ReallocWithoutRent => {
                "After `realloc`, top up the account's lamports to the new rent \
                 minimum using `Rent::get()` + `rent.minimum_balance(new_len)` \
                 — otherwise the account becomes rent-delinquent and the next \
                 tx against it can fail."
            }
            StaticLintRule::SeedSpoof => {
                "Pin the PDA with `seeds = [...]` / `bump` on the Accounts \
                 struct. Bare `find_program_address` using user-provided \
                 `AccountInfo::key` is vulnerable to seed spoofing — the \
                 attacker can substitute their own account if the constraint \
                 isn't enforced declaratively."
            }
        }
    }

    pub fn reference(self) -> &'static str {
        match self {
            StaticLintRule::MissingSigner => {
                "https://book.anchor-lang.com/anchor_references/account_constraints.html"
            }
            StaticLintRule::MissingOwnerCheck => {
                "https://book.anchor-lang.com/anchor_references/account_types.html"
            }
            StaticLintRule::MissingHasOne => {
                "https://book.anchor-lang.com/anchor_in_depth/constraints.html#has-one-target"
            }
            StaticLintRule::UncheckedAccountInfo => {
                "https://book.anchor-lang.com/anchor_in_depth/the_program_module.html#unchecked-accounts"
            }
            StaticLintRule::ArithmeticOverflow => {
                "https://solana.com/developers/guides/advanced/exchange#integer-overflow"
            }
            StaticLintRule::ReallocWithoutRent => {
                "https://solana.com/docs/programs/limitations#rent-exempt-reserve"
            }
            StaticLintRule::SeedSpoof => {
                "https://solana.com/developers/guides/getstarted/intro-to-anchor#program-derived-addresses"
            }
        }
    }
}

/// An Anchor-specific finding emitted by a single rule. This is the
/// intermediate shape before we flatten into the unified `Finding`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnchorFinding {
    pub rule: StaticLintRule,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub snippet: String,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StaticLintReport {
    #[serde(default)]
    pub run_id: String,
    pub project_root: String,
    pub rules: Vec<String>,
    pub findings: Vec<Finding>,
    pub anchor_findings: Vec<AnchorFinding>,
    pub files_scanned: u32,
    pub elapsed_ms: u128,
    pub severity_counts: SeverityCounts,
}

pub fn run(project_root: &Path, request: &StaticLintRequest) -> CommandResult<StaticLintReport> {
    let start = std::time::Instant::now();
    let mut files_scanned: u32 = 0;
    let mut anchor_findings: Vec<AnchorFinding> = Vec::new();

    let rule_filter: Option<Vec<StaticLintRule>> = if request.rule_ids.is_empty() {
        None
    } else {
        Some(
            StaticLintRule::all()
                .iter()
                .copied()
                .filter(|r| request.rule_ids.iter().any(|id| id == r.id()))
                .collect(),
        )
    };

    let rules: Vec<StaticLintRule> = rule_filter.unwrap_or_else(|| StaticLintRule::all().to_vec());

    let mut skip_suffixes = Vec::new();
    for s in &request.skip_paths {
        skip_suffixes.push(s.clone());
    }

    walk(project_root, &skip_suffixes, &mut |path, text| {
        files_scanned += 1;
        scan_file(path, text, &rules, &mut anchor_findings);
    })?;

    let mut findings: Vec<Finding> = anchor_findings.iter().map(lift_finding).collect();

    findings.sort_by(|a, b| a.severity.rank().cmp(&b.severity.rank()));
    let severity_counts = SeverityCounts::from_findings(&findings);

    Ok(StaticLintReport {
        run_id: String::new(),
        project_root: project_root.display().to_string(),
        rules: rules.iter().map(|r| r.id().to_string()).collect(),
        findings,
        anchor_findings,
        files_scanned,
        elapsed_ms: start.elapsed().as_millis(),
        severity_counts,
    })
}

fn lift_finding(finding: &AnchorFinding) -> Finding {
    let mut f = Finding::new(
        FindingSource::AnchorLints,
        finding.rule.id(),
        finding.rule.severity(),
        finding.rule.title(),
        finding.context.clone(),
    )
    .with_file(finding.file.clone())
    .with_location(finding.line, finding.column)
    .with_fix_hint(finding.rule.fix_hint())
    .with_reference(finding.rule.reference());
    f.id = format!(
        "{}:{}:{}:{}",
        FindingSource::AnchorLints.as_str(),
        finding.rule.id(),
        finding.file,
        finding.line
    );
    f
}

fn walk<F>(dir: &Path, skip_suffixes: &[String], on_file: &mut F) -> CommandResult<()>
where
    F: FnMut(&Path, &str),
{
    let entries = match fs::read_dir(dir) {
        Ok(it) => it,
        Err(err) => {
            return Err(CommandError::system_fault(
                "solana_audit_static_read_dir_failed",
                format!("Could not read {}: {err}", dir.display()),
            ))
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if should_skip(&path, skip_suffixes) {
            continue;
        }
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_dir() {
            walk(&path, skip_suffixes, on_file)?;
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
        if let Ok(text) = fs::read_to_string(&path) {
            on_file(&path, &text);
        }
    }
    Ok(())
}

fn should_skip(path: &Path, extra: &[String]) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return true,
    };
    if matches!(
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
    ) {
        return true;
    }
    let s = path.to_string_lossy();
    for suffix in extra {
        if s.contains(suffix.as_str()) {
            return true;
        }
    }
    false
}

fn scan_file(path: &Path, text: &str, rules: &[StaticLintRule], findings: &mut Vec<AnchorFinding>) {
    let lines: Vec<&str> = text.lines().collect();

    if rules.contains(&StaticLintRule::MissingSigner)
        || rules.contains(&StaticLintRule::UncheckedAccountInfo)
        || rules.contains(&StaticLintRule::MissingOwnerCheck)
        || rules.contains(&StaticLintRule::MissingHasOne)
    {
        scan_accounts_structs(path, &lines, rules, findings);
    }

    if rules.contains(&StaticLintRule::ArithmeticOverflow) {
        scan_arithmetic(path, &lines, findings);
    }

    if rules.contains(&StaticLintRule::ReallocWithoutRent) {
        scan_realloc(path, &lines, findings);
    }

    if rules.contains(&StaticLintRule::SeedSpoof) {
        scan_seed_spoof(path, &lines, findings);
    }
}

fn scan_accounts_structs(
    path: &Path,
    lines: &[&str],
    rules: &[StaticLintRule],
    findings: &mut Vec<AnchorFinding>,
) {
    let derive_re = Regex::new(r"#\s*\[\s*derive\s*\([^\)]*\bAccounts\b[^\)]*\)\s*\]").unwrap();
    let struct_open_re = Regex::new(r"^\s*(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap();

    let mut i = 0;
    while i < lines.len() {
        if derive_re.is_match(lines[i]) {
            // Walk forward until we find `struct <Name> {`
            let mut j = i;
            while j < lines.len() && !struct_open_re.is_match(lines[j]) {
                j += 1;
            }
            if j >= lines.len() {
                break;
            }
            let struct_name = struct_open_re
                .captures(lines[j])
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            // Find the matching `{` on this line or the next few.
            let mut k = j;
            while k < lines.len() && !lines[k].contains('{') {
                k += 1;
            }
            if k >= lines.len() {
                i = j + 1;
                continue;
            }
            // Collect body until matching `}` at column 0.
            let body_start = k + 1;
            let mut end = body_start;
            while end < lines.len() {
                if lines[end].trim_start().starts_with('}') && !lines[end].contains('{') {
                    break;
                }
                end += 1;
            }
            let body_lines = &lines[body_start..end];
            analyse_accounts_body(
                path,
                &struct_name,
                j,
                body_start,
                body_lines,
                rules,
                findings,
            );
            i = end + 1;
            continue;
        }
        i += 1;
    }
}

fn analyse_accounts_body(
    path: &Path,
    struct_name: &str,
    struct_line: usize,
    body_start: usize,
    body: &[&str],
    rules: &[StaticLintRule],
    findings: &mut Vec<AnchorFinding>,
) {
    let account_info_re = Regex::new(r"\b(?:AccountInfo|UncheckedAccount)\b").unwrap();
    let signer_any_re = Regex::new(r"\bSigner\b|#\[account\([^\]]*\bsigner\b").unwrap();
    let account_attr_re = Regex::new(r"#\s*\[\s*account\s*\((?P<args>[^\]]*)\)\s*\]").unwrap();
    let check_comment_re = Regex::new(r"///\s*CHECK\b").unwrap();
    let field_re =
        Regex::new(r"^\s*(?:pub\s+)?(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*:\s*(?P<ty>[^,]+)").unwrap();
    let has_one_re = Regex::new(r"\bhas_one\s*=").unwrap();
    let typed_account_re =
        Regex::new(r"\bAccount\s*<\s*'?\w*\s*,\s*([A-Za-z_][A-Za-z0-9_]*)\s*>").unwrap();
    let owner_re = Regex::new(r"\bowner\s*=").unwrap();
    let seeds_re = Regex::new(r"\bseeds\s*=").unwrap();

    let has_signer = body.iter().any(|l| signer_any_re.is_match(l));

    // Collect the typed-account field names so we can warn when they
    // have no has_one constraint referencing the other typed account.
    let mut typed_fields: Vec<(String, String, usize)> = Vec::new();

    let mut idx = 0;
    while idx < body.len() {
        let line = body[idx];
        if let Some(cap) = field_re.captures(line) {
            let field_name = cap["name"].to_string();
            let field_ty = cap["ty"].trim().trim_end_matches(',').to_string();
            // Aggregate the attributes immediately above this field.
            let mut attr_block = String::new();
            let mut has_check_comment = false;
            let mut back = idx;
            while back > 0 {
                back -= 1;
                let prev = body[back].trim();
                if prev.is_empty() {
                    continue;
                }
                if prev.starts_with("///") {
                    if check_comment_re.is_match(body[back]) {
                        has_check_comment = true;
                    }
                    continue;
                }
                if prev.starts_with("//") {
                    continue;
                }
                if prev.starts_with('#') {
                    attr_block.push_str(prev);
                    attr_block.push('\n');
                    continue;
                }
                break;
            }

            let line_no = (body_start + idx + 1) as u32;
            let column = 1u32;

            if account_info_re.is_match(&field_ty) {
                // Unchecked account info — must have CHECK comment + owner.
                if rules.contains(&StaticLintRule::UncheckedAccountInfo) && !has_check_comment {
                    findings.push(AnchorFinding {
                        rule: StaticLintRule::UncheckedAccountInfo,
                        file: path.display().to_string(),
                        line: line_no,
                        column,
                        snippet: line.trim().to_string(),
                        context: format!(
                            "Field `{field_name}` in `{struct_name}` is a raw AccountInfo/UncheckedAccount without a `/// CHECK:` safety comment."
                        ),
                    });
                }
                if rules.contains(&StaticLintRule::MissingOwnerCheck)
                    && !owner_re.is_match(&attr_block)
                    && !account_attr_re
                        .captures(&attr_block)
                        .and_then(|c| c.name("args"))
                        .map(|a| owner_re.is_match(a.as_str()))
                        .unwrap_or(false)
                {
                    findings.push(AnchorFinding {
                        rule: StaticLintRule::MissingOwnerCheck,
                        file: path.display().to_string(),
                        line: line_no,
                        column,
                        snippet: line.trim().to_string(),
                        context: format!(
                            "Field `{field_name}` is a raw AccountInfo in `{struct_name}` with no `owner = ...` constraint — the program it belongs to is unverified."
                        ),
                    });
                }
            }

            if let Some(t) = typed_account_re.captures(&field_ty) {
                typed_fields.push((field_name.clone(), t[1].to_string(), idx));
                if rules.contains(&StaticLintRule::MissingHasOne)
                    && attr_block.contains("mut")
                    && !has_one_re.is_match(&attr_block)
                    && typed_fields.len() >= 2
                {
                    findings.push(AnchorFinding {
                        rule: StaticLintRule::MissingHasOne,
                        file: path.display().to_string(),
                        line: line_no,
                        column,
                        snippet: line.trim().to_string(),
                        context: format!(
                            "Mutable typed account `{field_name}` in `{struct_name}` has no `has_one` constraint cross-referencing the other typed accounts on this instruction."
                        ),
                    });
                }
            }

            // seeds without bump is not a hard error, but we already
            // have a dedicated rule for seed-spoof below.
            let _ = seeds_re; // silence unused-warning when this rule is off.
        }
        idx += 1;
    }

    if rules.contains(&StaticLintRule::MissingSigner) && !has_signer {
        findings.push(AnchorFinding {
            rule: StaticLintRule::MissingSigner,
            file: path.display().to_string(),
            line: (struct_line + 1) as u32,
            column: 1,
            snippet: format!("struct {struct_name}"),
            context: format!(
                "Accounts struct `{struct_name}` has no `Signer` field or `#[account(signer)]` — any key can invoke this instruction."
            ),
        });
    }
}

fn scan_arithmetic(path: &Path, lines: &[&str], findings: &mut Vec<AnchorFinding>) {
    // Heuristic: look for `<ident>.<field>? <op> <ident|literal>` with
    // `+`, `-`, `*` where neither side is obviously a `checked_*`,
    // `saturating_*`, `wrapping_*`, `as` cast, or a comparison/assign.
    // This is intentionally loose; the severity is medium so the cost
    // of a false positive is a one-click dismissal in the UI.
    let arith_re = Regex::new(
        r"(?P<lhs>[A-Za-z_][A-Za-z0-9_\.]*)\s*(?P<op>[\+\-\*])\s*(?P<rhs>[A-Za-z_0-9][A-Za-z0-9_\.]*)",
    )
    .unwrap();
    let safe_hint_re = Regex::new(
        r"checked_(?:add|sub|mul|div)|saturating_(?:add|sub|mul)|wrapping_(?:add|sub|mul)",
    )
    .unwrap();

    for (idx, raw) in lines.iter().enumerate() {
        let line = raw.trim_start();
        if line.starts_with("//") || line.starts_with("///") {
            continue;
        }
        // Skip lines that look like attributes / derives / use items /
        // doc comments (they often have `-` in paths but never perform
        // arithmetic).
        if line.starts_with('#') || line.starts_with("use ") {
            continue;
        }
        // Skip any line that already uses a checked/saturating form.
        if safe_hint_re.is_match(raw) {
            continue;
        }
        // Skip attributes / lookalikes.
        if raw.contains("=>") || raw.contains(";") && raw.contains("const ") {
            // keep going, const fold is fine
        }
        // Must contain `+`, `-`, or `*` used as a binary arithmetic op.
        // Exclude obvious false positives: lambda ` -> `, borrow `&mut`,
        // generic bounds `T: Add`, range literals `0..=x`.
        if raw.contains("->") || raw.contains("..=") || raw.contains("..") {
            continue;
        }
        if let Some(cap) = arith_re.captures(raw) {
            // Ignore pointer deref / negative literals: `-1`, `* self`,
            // `&*x`, `*mut T`.
            let op = &cap["op"];
            // Reject cases where the operator is adjacent to `*mut` /
            // `*const` / `&*`.
            if op == "*" && (raw.contains("*mut ") || raw.contains("*const ") || raw.contains("&*"))
            {
                continue;
            }
            // Reject clearly-non-numeric identifiers (PascalCase types).
            let lhs = &cap["lhs"];
            if lhs
                .chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false)
                && !lhs.contains('.')
            {
                continue;
            }
            // Must actually look like it's inside an expression statement:
            // heuristic — the line contains `=` on the LHS of the match
            // or the match is inside parens / a function call.
            if !raw.contains('=') && !raw.contains('(') && !raw.contains("return ") {
                continue;
            }

            let whole = cap.get(0).unwrap();
            let column = whole.start() + 1;
            findings.push(AnchorFinding {
                rule: StaticLintRule::ArithmeticOverflow,
                file: path.display().to_string(),
                line: (idx + 1) as u32,
                column: column as u32,
                snippet: raw.trim().to_string(),
                context: format!(
                    "Binary `{op}` on `{lhs}` and `{}` — consider `checked_{op_name}` to avoid silent overflow.",
                    &cap["rhs"],
                    op_name = match op {
                        "+" => "add",
                        "-" => "sub",
                        _ => "mul",
                    }
                ),
            });
        }
    }
}

fn scan_realloc(path: &Path, lines: &[&str], findings: &mut Vec<AnchorFinding>) {
    let realloc_re = Regex::new(r"\brealloc\s*\(").unwrap();
    let rent_re = Regex::new(r"Rent\s*::\s*get|minimum_balance\s*\(|rent_exempt|rent\.").unwrap();

    for (idx, raw) in lines.iter().enumerate() {
        if !realloc_re.is_match(raw) {
            continue;
        }
        let start = idx.saturating_sub(6);
        let end = (idx + 6).min(lines.len());
        let window = lines[start..end].join("\n");
        if rent_re.is_match(&window) {
            continue;
        }
        findings.push(AnchorFinding {
            rule: StaticLintRule::ReallocWithoutRent,
            file: path.display().to_string(),
            line: (idx + 1) as u32,
            column: 1,
            snippet: raw.trim().to_string(),
            context:
                "`realloc(...)` call has no rent-exempt top-up within ±6 lines — the account may become rent-delinquent after the resize."
                    .to_string(),
        });
    }
}

fn scan_seed_spoof(path: &Path, lines: &[&str], findings: &mut Vec<AnchorFinding>) {
    // `find_program_address(&[<seeds>], ...)` where a seed looks like
    // user-controlled `Pubkey` material:
    //   `<expr>.key().as_ref()`           (direct)
    //   `<ident>.as_ref()` where `ident`  (indirect) — tracked via
    //   earlier bindings of `<ident> = ctx.accounts.*.key()` or
    //   `<ident> = *.key()`.
    // Suppressed when a declarative `seeds = [...]` pin appears within
    // ±40 lines (typical Anchor struct + handler distance).
    let find_re =
        Regex::new(r"find_program_address\s*\(\s*&\[(?P<seeds>(?:[^\[\]]|\[[^\[\]]*\])*)\]")
            .unwrap();
    let direct_key_re = Regex::new(r"\.key\s*\(\s*\)\s*\.\s*as_ref\s*\(\s*\)").unwrap();
    let as_ref_ident_re = Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\s*\.\s*as_ref\s*\(\s*\)").unwrap();
    let binding_key_re =
        Regex::new(r"let\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?::\s*[^=]+)?\s*=\s*[^;]*\.key\s*\(")
            .unwrap();
    let decl_seeds_re = Regex::new(r"\bseeds\s*=").unwrap();

    // Collect bindings proactively — `let user = ...key()` captures any
    // identifier treated as user-supplied Pubkey material.
    let mut key_bindings: std::collections::HashSet<String> = std::collections::HashSet::new();
    for raw in lines {
        for cap in binding_key_re.captures_iter(raw) {
            key_bindings.insert(cap[1].to_string());
        }
    }
    // Well-known constants / literal seeds that are safe.
    let safe_idents: &[&str] = &["b", "BUMP", "_bump", "seed", "SEED"];

    for (idx, raw) in lines.iter().enumerate() {
        let caps = match find_re.captures(raw) {
            Some(c) => c,
            None => continue,
        };
        let seed_expr = &caps["seeds"];
        let mut spoofable = direct_key_re.is_match(seed_expr);
        if !spoofable {
            for cap in as_ref_ident_re.captures_iter(seed_expr) {
                let ident = &cap[1];
                if safe_idents.contains(&ident) {
                    continue;
                }
                if key_bindings.contains(ident) {
                    spoofable = true;
                    break;
                }
            }
        }
        if !spoofable {
            continue;
        }
        // Lookahead/behind a window for a declarative pin on the same
        // Accounts struct.
        let start = idx.saturating_sub(40);
        let end = (idx + 40).min(lines.len());
        let window = lines[start..end].join("\n");
        if decl_seeds_re.is_match(&window) {
            continue;
        }
        let whole = caps.get(0).unwrap();
        findings.push(AnchorFinding {
            rule: StaticLintRule::SeedSpoof,
            file: path.display().to_string(),
            line: (idx + 1) as u32,
            column: (whole.start() + 1) as u32,
            snippet: raw.trim().to_string(),
            context:
                "PDA derivation uses user-controlled `AccountInfo::key` without a declarative `seeds = [...]` / `bump` pin. The seed source can be substituted by the caller."
                    .to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn touch(dir: &Path, rel: &str, body: &str) -> PathBuf {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn missing_signer_flagged_for_empty_accounts_struct() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "programs/p/src/lib.rs",
            r#"
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub vault: AccountInfo<'info>,
}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["missing_signer".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "missing_signer"),
            "expected missing_signer, got: {:?}",
            report.findings
        );
    }

    #[test]
    fn signer_field_clears_missing_signer_lint() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
#[derive(Accounts)]
pub struct Withdraw<'info> {
    pub authority: Signer<'info>,
    #[account(mut)]
    pub vault: Account<'info, Vault>,
}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["missing_signer".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.rule_id == "missing_signer"),
            "should not flag missing_signer when Signer is present"
        );
    }

    #[test]
    fn unchecked_account_info_flags_missing_check_comment() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
#[derive(Accounts)]
pub struct Foo<'info> {
    pub authority: Signer<'info>,
    pub leaky: UncheckedAccount<'info>,
}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["unchecked_account_info".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(report
            .findings
            .iter()
            .any(|f| f.rule_id == "unchecked_account_info"));
    }

    #[test]
    fn check_comment_silences_unchecked_account_info() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
#[derive(Accounts)]
pub struct Foo<'info> {
    pub authority: Signer<'info>,
    /// CHECK: validated manually below.
    pub custom: UncheckedAccount<'info>,
}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["unchecked_account_info".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(!report
            .findings
            .iter()
            .any(|f| f.rule_id == "unchecked_account_info"));
    }

    #[test]
    fn arithmetic_overflow_flagged_without_checked() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
pub fn tally(a: u64, b: u64) -> u64 {
    let total = a + b;
    total
}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["arithmetic_overflow".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "arithmetic_overflow"),
            "expected arithmetic_overflow, got: {:?}",
            report.findings
        );
    }

    #[test]
    fn arithmetic_overflow_cleared_with_checked_add() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
pub fn tally(a: u64, b: u64) -> u64 {
    let total = a.checked_add(b).unwrap();
    total
}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["arithmetic_overflow".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(report
            .findings
            .iter()
            .all(|f| f.rule_id != "arithmetic_overflow"));
    }

    #[test]
    fn realloc_without_rent_flagged() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
pub fn grow(info: &AccountInfo) -> Result<()> {
    info.realloc(info.data_len() + 64, false)?;
    Ok(())
}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["realloc_without_rent".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "realloc_without_rent"),
            "expected realloc_without_rent; got {:?}",
            report.findings
        );
    }

    #[test]
    fn realloc_cleared_when_rent_topup_present() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
pub fn grow(info: &AccountInfo) -> Result<()> {
    let new_len = info.data_len() + 64;
    let rent = Rent::get()?;
    let required = rent.minimum_balance(new_len);
    info.realloc(new_len, false)?;
    let delta = required.saturating_sub(info.lamports());
    Ok(())
}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["realloc_without_rent".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(report
            .findings
            .iter()
            .all(|f| f.rule_id != "realloc_without_rent"));
    }

    #[test]
    fn seed_spoof_flagged_without_declarative_pin() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
pub fn compute(ctx: Context<Foo>) -> Result<()> {
    let user = ctx.accounts.user.key();
    let (pda, _bump) = Pubkey::find_program_address(&[user.as_ref()], &crate::ID);
    msg!("{}", pda);
    Ok(())
}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["seed_spoof".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(
            report.findings.iter().any(|f| f.rule_id == "seed_spoof"),
            "expected seed_spoof; got {:?}",
            report.findings
        );
    }

    #[test]
    fn seed_spoof_cleared_by_declarative_pin() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
#[derive(Accounts)]
pub struct Foo<'info> {
    #[account(seeds = [user.key().as_ref()], bump)]
    pub vault: Account<'info, Vault>,
    pub user: Signer<'info>,
}

pub fn compute(ctx: Context<Foo>) -> Result<()> {
    let user = ctx.accounts.user.key();
    let (pda, _bump) = Pubkey::find_program_address(&[user.as_ref()], &crate::ID);
    Ok(())
}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["seed_spoof".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(
            report.findings.iter().all(|f| f.rule_id != "seed_spoof"),
            "seed_spoof should be silent when declarative pin is present"
        );
    }

    #[test]
    fn skip_paths_filters_out_target() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "target/debug/generated.rs",
            r#"
#[derive(Accounts)]
pub struct Bogus<'info> {
    pub x: UncheckedAccount<'info>,
}
"#,
        );
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
pub fn ok() {}
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec![],
                skip_paths: vec![],
            },
        )
        .unwrap();
        // target/ is skipped by default — no findings from generated.rs.
        assert!(report.findings.iter().all(|f| !f
            .file
            .as_deref()
            .unwrap_or("")
            .contains("generated.rs")));
    }

    #[test]
    fn rule_ids_filter_limits_rules_applied() {
        let tmp = TempDir::new().unwrap();
        touch(
            tmp.path(),
            "src/lib.rs",
            r#"
#[derive(Accounts)]
pub struct Foo<'info> {
    pub leaky: UncheckedAccount<'info>,
}

pub fn add(a: u64, b: u64) -> u64 { a + b }
"#,
        );
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec!["unchecked_account_info".into()],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert!(report
            .findings
            .iter()
            .all(|f| f.rule_id == "unchecked_account_info"));
    }

    #[test]
    fn report_includes_files_scanned_count() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "src/a.rs", "pub fn a() {}\n");
        touch(tmp.path(), "src/b.rs", "pub fn b() {}\n");
        let report = run(
            tmp.path(),
            &StaticLintRequest {
                project_root: tmp.path().display().to_string(),
                rule_ids: vec![],
                skip_paths: vec![],
            },
        )
        .unwrap();
        assert_eq!(report.files_scanned, 2);
    }
}
