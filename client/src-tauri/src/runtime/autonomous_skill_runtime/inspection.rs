use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

use crate::commands::{CommandError, CommandResult};

use super::{
    cache::{
        sha256_hex, AutonomousSkillCacheError, AutonomousSkillCacheInstallFile,
        AutonomousSkillCacheManifest, AutonomousSkillCacheManifestFile, CACHE_VERSION,
    },
    runtime::{AutonomousSkillDiscoveryCandidate, ALLOWED_TEXT_EXTENSIONS, MAX_SKILL_FILE_BYTES},
    source::{
        AutonomousSkillSourceEntryKind, AutonomousSkillSourceMetadata,
        AutonomousSkillSourceTreeResponse,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectedSkill {
    pub(crate) skill_id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) user_invocable: Option<bool>,
    pub(crate) source: AutonomousSkillSourceMetadata,
    pub(crate) assets: Vec<InspectedSkillAsset>,
    pub(crate) skill_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectedSkillAsset {
    pub(crate) relative_path: String,
    pub(crate) source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeSkillInspection {
    pub(crate) skill_id: String,
    pub(crate) source: AutonomousSkillSourceMetadata,
    pub(crate) assets: Vec<InspectedSkillAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillFrontmatter {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) user_invocable: Option<bool>,
}

pub(crate) fn build_cache_manifest(
    inspected: &InspectedSkill,
    files: &[AutonomousSkillCacheInstallFile],
) -> AutonomousSkillCacheManifest {
    AutonomousSkillCacheManifest {
        version: CACHE_VERSION,
        skill_id: inspected.skill_id.clone(),
        name: inspected.name.clone(),
        description: inspected.description.clone(),
        user_invocable: inspected.user_invocable,
        source: inspected.source.clone(),
        cached_at: crate::auth::now_timestamp(),
        files: files
            .iter()
            .map(|file| AutonomousSkillCacheManifestFile {
                relative_path: file.relative_path.clone(),
                sha256: sha256_hex(&file.bytes),
                bytes: file.bytes.len(),
            })
            .collect(),
    }
}

pub(crate) fn discover_candidates_from_tree(
    tree: &AutonomousSkillSourceTreeResponse,
    repo: &str,
    reference: &str,
    source_root: &str,
) -> CommandResult<Vec<AutonomousSkillDiscoveryCandidate>> {
    let source_root_prefix = format!("{source_root}/");
    let mut candidates = Vec::new();

    for entry in &tree.entries {
        if entry.kind != AutonomousSkillSourceEntryKind::Tree {
            continue;
        }
        if !entry.path.starts_with(&source_root_prefix) {
            continue;
        }
        let relative = entry
            .path
            .strip_prefix(&source_root_prefix)
            .unwrap_or_default();
        if relative.is_empty() || relative.contains('/') {
            continue;
        }
        let skill_id = normalize_skill_id(relative)?;
        candidates.push(AutonomousSkillDiscoveryCandidate {
            skill_id,
            source: AutonomousSkillSourceMetadata {
                repo: repo.to_owned(),
                path: entry.path.clone(),
                reference: reference.to_owned(),
                tree_hash: entry.hash.clone(),
            },
        });
    }

    Ok(candidates)
}

pub(crate) fn inspect_tree_for_skill(
    tree: &AutonomousSkillSourceTreeResponse,
    repo: &str,
    reference: &str,
    skill_path: &str,
    expected_tree_hash: Option<&str>,
    max_skill_files: usize,
) -> CommandResult<TreeSkillInspection> {
    let skill_path = normalize_relative_source_path(skill_path)?;
    let skill_id = skill_path
        .rsplit('/')
        .next()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_skill_source_metadata_invalid",
                "Xero requires autonomous skill source paths to include a skill id.",
            )
        })?
        .to_owned();
    normalize_skill_id(&skill_id)?;

    let root_entry = tree
        .entries
        .iter()
        .find(|entry| {
            entry.kind == AutonomousSkillSourceEntryKind::Tree && entry.path == skill_path
        })
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_skill_not_found",
                format!("Xero could not find autonomous skill `{skill_id}` at `{skill_path}`."),
            )
        })?;

    if let Some(expected_tree_hash) = expected_tree_hash {
        if root_entry.hash != expected_tree_hash {
            return Err(CommandError::user_fixable(
                "autonomous_skill_source_changed",
                format!(
                    "Xero resolved `{skill_id}` at tree hash `{expected_tree_hash}`, but the latest source now reports `{}`.",
                    root_entry.hash
                ),
            ));
        }
    }

    let mut assets = Vec::new();
    let asset_prefix = format!("{skill_path}/");
    for entry in &tree.entries {
        if entry.kind != AutonomousSkillSourceEntryKind::Blob {
            continue;
        }
        if !entry.path.starts_with(&asset_prefix) {
            continue;
        }
        let relative_path = entry.path.strip_prefix(&asset_prefix).unwrap_or_default();
        validate_skill_asset_path(relative_path)?;
        if let Some(bytes) = entry.bytes {
            if bytes > MAX_SKILL_FILE_BYTES {
                return Err(CommandError::user_fixable(
                    "autonomous_skill_layout_unsupported",
                    format!(
                        "Xero rejected `{skill_id}` because asset `{relative_path}` exceeded the {} byte per-file limit.",
                        MAX_SKILL_FILE_BYTES
                    ),
                ));
            }
        }
        assets.push(InspectedSkillAsset {
            relative_path: relative_path.to_owned(),
            source_path: entry.path.clone(),
        });
    }

    if assets.is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_missing",
            format!(
                "Xero rejected `{skill_id}` because the source path `{skill_path}` contained no files."
            ),
        ));
    }
    if assets.len() > max_skill_files {
        return Err(CommandError::user_fixable(
            "autonomous_skill_layout_unsupported",
            format!(
                "Xero rejected `{skill_id}` because it exceeded the {} file limit for autonomous skill assets.",
                max_skill_files
            ),
        ));
    }
    if !assets.iter().any(|asset| asset.relative_path == "SKILL.md") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_missing",
            format!("Xero rejected `{skill_id}` because SKILL.md was missing from `{skill_path}`."),
        ));
    }

    assets.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    Ok(TreeSkillInspection {
        skill_id,
        source: AutonomousSkillSourceMetadata {
            repo: repo.to_owned(),
            path: skill_path,
            reference: reference.to_owned(),
            tree_hash: root_entry.hash.clone(),
        },
        assets,
    })
}

pub(crate) fn parse_skill_frontmatter(markdown: &str) -> CommandResult<SkillFrontmatter> {
    let mut lines = markdown.lines();
    if lines.next() != Some("---") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Xero requires autonomous skill SKILL.md files to start with YAML frontmatter delimited by `---`.",
        ));
    }

    let mut entries = BTreeMap::new();
    let mut found_closing = false;
    for line in lines {
        if line.trim() == "---" {
            found_closing = true;
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once(':') else {
            return Err(CommandError::user_fixable(
                "autonomous_skill_document_invalid",
                format!(
                    "Xero rejected SKILL.md because frontmatter line `{}` was not `key: value`.",
                    line.trim()
                ),
            ));
        };
        let key = raw_key.trim().to_owned();
        let value = raw_value.trim().to_owned();
        if key.is_empty() || value.is_empty() {
            return Err(CommandError::user_fixable(
                "autonomous_skill_document_invalid",
                format!(
                    "Xero rejected SKILL.md because frontmatter line `{}` was missing a key or value.",
                    line.trim()
                ),
            ));
        }
        entries.insert(key, strip_wrapping_quotes(&value));
    }

    if !found_closing {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Xero rejected SKILL.md because the YAML frontmatter block was not closed.",
        ));
    }

    let name = entries.remove("name").ok_or_else(|| {
        CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Xero rejected SKILL.md because frontmatter `name` was missing.",
        )
    })?;
    let description = entries.remove("description").ok_or_else(|| {
        CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Xero rejected SKILL.md because frontmatter `description` was missing.",
        )
    })?;
    let name = normalize_skill_id(&name)?;
    if description.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Xero rejected SKILL.md because frontmatter `description` was blank.",
        ));
    }

    let user_invocable = match entries.remove("user-invocable") {
        Some(value) => Some(parse_frontmatter_bool(&value)?),
        None => None,
    };

    Ok(SkillFrontmatter {
        name,
        description: description.trim().to_owned(),
        user_invocable,
    })
}

pub(crate) fn validate_source_metadata(
    source: &AutonomousSkillSourceMetadata,
) -> CommandResult<AutonomousSkillSourceMetadata> {
    let repo = normalize_source_repo(&source.repo)?;
    let path = normalize_relative_source_path(&source.path)?;
    let reference = normalize_source_ref(&source.reference)?;
    let tree_hash = normalize_tree_hash(&source.tree_hash)?;
    Ok(AutonomousSkillSourceMetadata {
        repo,
        path,
        reference,
        tree_hash,
    })
}

pub(crate) fn validate_cache_manifest(
    manifest: &AutonomousSkillCacheManifest,
) -> Result<(), AutonomousSkillCacheError> {
    if manifest.version != CACHE_VERSION {
        return Err(AutonomousSkillCacheError::Contract(format!(
            "Xero rejected cached autonomous skill manifest version `{}` because only version `{CACHE_VERSION}` is supported.",
            manifest.version
        )));
    }
    normalize_skill_id(&manifest.skill_id).map_err(command_error_to_cache_contract)?;
    validate_source_metadata(&manifest.source).map_err(command_error_to_cache_contract)?;
    if manifest.name.trim().is_empty() || manifest.description.trim().is_empty() {
        return Err(AutonomousSkillCacheError::Contract(
            "Xero rejected a cached autonomous skill manifest because name/description were blank."
                .into(),
        ));
    }
    let mut seen_paths = BTreeSet::new();
    for record in &manifest.files {
        let normalized = normalize_relative_source_path(&record.relative_path)
            .map_err(command_error_to_cache_contract)?;
        if !seen_paths.insert(normalized.clone()) {
            return Err(AutonomousSkillCacheError::Contract(format!(
                "Xero rejected cached autonomous skill manifest for `{}` because `{normalized}` was duplicated.",
                manifest.skill_id
            )));
        }
        if record.sha256.trim().is_empty() || record.bytes == 0 {
            return Err(AutonomousSkillCacheError::Contract(format!(
                "Xero rejected cached autonomous skill manifest for `{}` because `{normalized}` had incomplete file metadata.",
                manifest.skill_id
            )));
        }
    }
    if !seen_paths.contains("SKILL.md") {
        return Err(AutonomousSkillCacheError::Contract(format!(
            "Xero rejected cached autonomous skill manifest for `{}` because SKILL.md was missing.",
            manifest.skill_id
        )));
    }
    Ok(())
}

pub(crate) fn normalize_skill_id(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("skillId"));
    }
    if !trimmed
        .chars()
        .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == '-')
    {
        return Err(CommandError::user_fixable(
            "autonomous_skill_id_invalid",
            "Xero requires autonomous skill ids to be lowercase kebab-case values.",
        ));
    }
    Ok(trimmed.to_owned())
}

pub(crate) fn normalize_relative_source_path(value: &str) -> CommandResult<String> {
    let path = Path::new(value.trim());
    if value.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_metadata_invalid",
            "Xero requires autonomous skill source paths to be non-empty relative paths.",
        ));
    }

    let mut normalized = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => {
                let segment = segment.to_str().ok_or_else(|| {
                    CommandError::user_fixable(
                        "autonomous_skill_source_metadata_invalid",
                        "Xero requires autonomous skill source paths to be valid UTF-8.",
                    )
                })?;
                if segment.is_empty() {
                    return Err(CommandError::user_fixable(
                        "autonomous_skill_source_metadata_invalid",
                        "Xero rejected an autonomous skill source path with an empty segment.",
                    ));
                }
                normalized.push(segment.to_owned());
            }
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(CommandError::user_fixable(
                    "autonomous_skill_source_metadata_invalid",
                    "Xero requires autonomous skill source paths to stay within a relative skill root.",
                ));
            }
        }
    }

    if normalized.is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_metadata_invalid",
            "Xero requires autonomous skill source paths to be non-empty relative paths.",
        ));
    }

    Ok(normalized.join("/"))
}

pub(crate) fn normalize_source_repo(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("sourceRepo"));
    }
    if trimmed.contains("://") || trimmed.starts_with("git@") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_repo_invalid",
            "Xero currently supports only GitHub `owner/repo` shorthand for autonomous skill sources.",
        ));
    }
    let mut segments = trimmed.split('/');
    let Some(owner) = segments.next() else {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_repo_invalid",
            "Xero requires autonomous skill source repositories to use `owner/repo` format.",
        ));
    };
    let Some(repo) = segments.next() else {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_repo_invalid",
            "Xero requires autonomous skill source repositories to use `owner/repo` format.",
        ));
    };
    if segments.next().is_some() || owner.trim().is_empty() || repo.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_repo_invalid",
            "Xero requires autonomous skill source repositories to use `owner/repo` format.",
        ));
    }
    Ok(format!("{}/{}", owner.trim(), repo.trim()))
}

pub(crate) fn normalize_source_ref(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("sourceRef"));
    }
    if trimmed.contains("://") || trimmed.starts_with("refs/") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_ref_invalid",
            "Xero requires autonomous skill source refs to be a branch, tag, or commit identifier, not a URL or raw `refs/...` path.",
        ));
    }
    Ok(trimmed.to_owned())
}

pub(crate) fn normalize_tree_hash(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.len() != 40 || !trimmed.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_metadata_invalid",
            "Xero requires autonomous skill source tree_hash values to be 40-character hexadecimal Git tree hashes.",
        ));
    }
    Ok(trimmed.to_ascii_lowercase())
}

pub(crate) fn join_relative_path(prefix: &str, leaf: &str) -> String {
    format!("{prefix}/{leaf}")
}

pub(crate) fn skill_matches_query(skill_id: &str, query: &str) -> bool {
    let haystack = skill_id.replace('-', " ").to_ascii_lowercase();
    query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .all(|term| haystack.contains(&term))
}

fn parse_frontmatter_bool(value: &str) -> CommandResult<bool> {
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            format!(
                "Xero rejected SKILL.md because frontmatter boolean value `{other}` was invalid."
            ),
        )),
    }
}

pub(crate) fn validate_skill_asset_path(relative_path: &str) -> CommandResult<()> {
    let normalized = normalize_relative_source_path(relative_path)?;
    if normalized == "SKILL.md" {
        return Ok(());
    }
    if normalized.ends_with("/SKILL.md") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_layout_unsupported",
            format!(
                "Xero rejected skill layout because nested SKILL.md file `{normalized}` is not supported."
            ),
        ));
    }
    let extension = Path::new(&normalized)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_skill_layout_unsupported",
                format!(
                    "Xero rejected skill asset `{normalized}` because extensionless supporting files are not supported."
                ),
            )
        })?;
    if !ALLOWED_TEXT_EXTENSIONS.contains(&extension.as_str()) {
        return Err(CommandError::user_fixable(
            "autonomous_skill_layout_unsupported",
            format!(
                "Xero rejected skill asset `{normalized}` because `.{extension}` files are not supported in the autonomous skill cache."
            ),
        ));
    }

    Ok(())
}

fn strip_wrapping_quotes(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        trimmed[1..trimmed.len() - 1].to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn command_error_to_cache_contract(error: CommandError) -> AutonomousSkillCacheError {
    AutonomousSkillCacheError::Contract(error.message)
}
