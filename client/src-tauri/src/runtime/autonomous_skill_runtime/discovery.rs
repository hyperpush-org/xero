use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::commands::{CommandError, CommandResult};

use super::{
    cache::sha256_hex,
    contract::{
        XeroSkillSourceLocator, XeroSkillSourceRecord, XeroSkillSourceScope, XeroSkillSourceState,
        XeroSkillTrustState,
    },
    inspection::{
        normalize_relative_source_path, parse_skill_frontmatter, validate_skill_asset_path,
    },
    runtime::{MAX_SKILL_FILES, MAX_SKILL_FILE_BYTES, MAX_TOTAL_SKILL_BYTES},
    skill_tool::{
        validate_skill_tool_context_payload, XeroSkillToolContextAsset,
        XeroSkillToolContextDocument, XeroSkillToolContextPayload,
        XERO_SKILL_TOOL_CONTRACT_VERSION,
    },
};

pub const PROJECT_SKILL_DIRECTORY: &str = "skills";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XeroSkillDirectoryDiscovery {
    pub candidates: Vec<XeroDiscoveredSkill>,
    pub diagnostics: Vec<XeroSkillDiscoveryDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XeroDiscoveredSkill {
    pub source: XeroSkillSourceRecord,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub local_location: String,
    pub version_hash: String,
    pub asset_paths: Vec<String>,
    pub total_bytes: usize,
    skill_directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XeroSkillDiscoveryDiagnostic {
    pub code: String,
    pub message: String,
    pub relative_path: Option<String>,
}

enum SkillDiscoveryRoot {
    Local {
        root_id: String,
        root_path: PathBuf,
    },
    Project {
        project_id: String,
        project_app_data_dir: PathBuf,
    },
    Bundled {
        bundle_id: String,
        version: String,
        root_path: PathBuf,
    },
    Plugin {
        project_id: String,
        plugin_id: String,
        contribution_id: String,
        skill_path: String,
        root_path: PathBuf,
        source_state: XeroSkillSourceState,
        trust: XeroSkillTrustState,
    },
}

impl SkillDiscoveryRoot {
    fn scan_root(&self) -> PathBuf {
        match self {
            Self::Local { root_path, .. }
            | Self::Bundled { root_path, .. }
            | Self::Plugin { root_path, .. } => root_path.clone(),
            Self::Project {
                project_app_data_dir,
                ..
            } => project_app_data_dir.join(PROJECT_SKILL_DIRECTORY),
        }
    }

    fn source_record(
        &self,
        scan_root: &Path,
        root_canonical: &Path,
        skill_directory: &Path,
        skill_id: &str,
    ) -> CommandResult<XeroSkillSourceRecord> {
        let source_relative_path = path_to_relative_source_path(scan_root, skill_directory)?;
        match self {
            Self::Local { root_id, .. } => XeroSkillSourceRecord::new(
                XeroSkillSourceScope::global(),
                XeroSkillSourceLocator::Local {
                    root_id: root_id.clone(),
                    root_path: root_canonical.display().to_string(),
                    relative_path: source_relative_path,
                    skill_id: skill_id.to_owned(),
                },
                XeroSkillSourceState::Discoverable,
                XeroSkillTrustState::ApprovalRequired,
            ),
            Self::Project { project_id, .. } => XeroSkillSourceRecord::new(
                XeroSkillSourceScope::project(project_id.clone())?,
                XeroSkillSourceLocator::Project {
                    relative_path: normalize_relative_source_path(&format!(
                        "{PROJECT_SKILL_DIRECTORY}/{source_relative_path}"
                    ))?,
                    skill_id: skill_id.to_owned(),
                },
                XeroSkillSourceState::Discoverable,
                XeroSkillTrustState::ApprovalRequired,
            ),
            Self::Bundled {
                bundle_id, version, ..
            } => XeroSkillSourceRecord::new(
                XeroSkillSourceScope::global(),
                XeroSkillSourceLocator::Bundled {
                    bundle_id: bundle_id.clone(),
                    skill_id: skill_id.to_owned(),
                    version: version.clone(),
                },
                XeroSkillSourceState::Discoverable,
                XeroSkillTrustState::Trusted,
            ),
            Self::Plugin {
                project_id,
                plugin_id,
                contribution_id,
                skill_path,
                source_state,
                trust,
                ..
            } => XeroSkillSourceRecord::new(
                XeroSkillSourceScope::project(project_id.clone())?,
                XeroSkillSourceLocator::Plugin {
                    plugin_id: plugin_id.clone(),
                    contribution_id: contribution_id.clone(),
                    skill_path: skill_path.clone(),
                    skill_id: skill_id.to_owned(),
                },
                *source_state,
                *trust,
            ),
        }
    }
}

pub fn discover_local_skill_directory(
    root_id: impl Into<String>,
    root_path: impl AsRef<Path>,
) -> CommandResult<XeroSkillDirectoryDiscovery> {
    let root_id = normalize_required(root_id.into(), "rootId")?;
    discover_skill_directory(SkillDiscoveryRoot::Local {
        root_id,
        root_path: root_path.as_ref().to_path_buf(),
    })
}

pub fn discover_project_skill_directory(
    project_id: impl Into<String>,
    project_app_data_dir: impl AsRef<Path>,
) -> CommandResult<XeroSkillDirectoryDiscovery> {
    let project_id = normalize_required(project_id.into(), "projectId")?;
    discover_skill_directory(SkillDiscoveryRoot::Project {
        project_id,
        project_app_data_dir: project_app_data_dir.as_ref().to_path_buf(),
    })
}

pub fn discover_bundled_skill_directory(
    bundle_id: impl Into<String>,
    version: impl Into<String>,
    root_path: impl AsRef<Path>,
) -> CommandResult<XeroSkillDirectoryDiscovery> {
    let bundle_id = normalize_required(bundle_id.into(), "bundleId")?;
    let version = normalize_required(version.into(), "version")?;
    discover_skill_directory(SkillDiscoveryRoot::Bundled {
        bundle_id,
        version,
        root_path: root_path.as_ref().to_path_buf(),
    })
}

pub fn discover_plugin_skill_contribution(
    project_id: impl Into<String>,
    plugin_id: impl Into<String>,
    contribution_id: impl Into<String>,
    plugin_root: impl AsRef<Path>,
    skill_path: impl Into<String>,
    source_state: XeroSkillSourceState,
    trust: XeroSkillTrustState,
) -> CommandResult<XeroSkillDirectoryDiscovery> {
    let project_id = normalize_required(project_id.into(), "projectId")?;
    let plugin_id = normalize_required(plugin_id.into(), "pluginId")?;
    let contribution_id = normalize_required(contribution_id.into(), "contributionId")?;
    let skill_path = normalize_relative_source_path(&skill_path.into())?;
    let plugin_root = plugin_root.as_ref().to_path_buf();
    let root = SkillDiscoveryRoot::Plugin {
        project_id,
        plugin_id,
        contribution_id,
        skill_path: skill_path.clone(),
        root_path: plugin_root.clone(),
        source_state,
        trust,
    };
    let mut diagnostics = Vec::new();
    if !plugin_root.is_dir() {
        diagnostics.push(XeroSkillDiscoveryDiagnostic {
            code: "xero_plugin_root_unavailable".into(),
            message: format!(
                "Xero could not scan plugin skill root {} because it is not available.",
                plugin_root.display()
            ),
            relative_path: None,
        });
        return Ok(XeroSkillDirectoryDiscovery {
            candidates: Vec::new(),
            diagnostics,
        });
    }
    let root_canonical = fs::canonicalize(&plugin_root).map_err(|error| {
        CommandError::retryable(
            "xero_plugin_root_unavailable",
            format!(
                "Xero could not resolve plugin skill root {}: {error}",
                plugin_root.display()
            ),
        )
    })?;
    let skill_directory = plugin_root.join(&skill_path);
    if !skill_directory.is_dir() {
        diagnostics.push(XeroSkillDiscoveryDiagnostic {
            code: "xero_plugin_skill_unavailable".into(),
            message: format!(
                "Xero could not find plugin skill contribution `{skill_path}` under {}.",
                plugin_root.display()
            ),
            relative_path: Some(skill_path),
        });
        return Ok(XeroSkillDirectoryDiscovery {
            candidates: Vec::new(),
            diagnostics,
        });
    }

    match inspect_filesystem_skill(&root, &plugin_root, &root_canonical, &skill_directory) {
        Ok(candidate) => Ok(XeroSkillDirectoryDiscovery {
            candidates: vec![candidate],
            diagnostics,
        }),
        Err(error) => {
            diagnostics.push(XeroSkillDiscoveryDiagnostic {
                code: error.code,
                message: error.message,
                relative_path: path_to_relative_source_path(&plugin_root, &skill_directory).ok(),
            });
            Ok(XeroSkillDirectoryDiscovery {
                candidates: Vec::new(),
                diagnostics,
            })
        }
    }
}

pub fn load_discovered_skill_context(
    candidate: &XeroDiscoveredSkill,
    include_supporting_assets: bool,
) -> CommandResult<XeroSkillToolContextPayload> {
    load_skill_context_from_directory(
        &candidate.source.source_id,
        &candidate.skill_id,
        &candidate.skill_directory,
        Some(&candidate.asset_paths),
        include_supporting_assets,
    )
}

pub fn load_skill_context_from_directory(
    source_id: &str,
    skill_id: &str,
    skill_directory: impl AsRef<Path>,
    expected_asset_paths: Option<&[String]>,
    include_supporting_assets: bool,
) -> CommandResult<XeroSkillToolContextPayload> {
    let skill_directory = skill_directory.as_ref();
    let asset_paths = match expected_asset_paths {
        Some(paths) => paths.to_vec(),
        None => {
            let mut files = Vec::new();
            collect_skill_files(skill_directory, skill_directory, &mut files)?;
            files.sort_by(|left, right| left.0.cmp(&right.0));
            files
                .into_iter()
                .map(|(relative_path, _)| relative_path)
                .collect()
        }
    };
    let markdown_path = skill_directory.join("SKILL.md");
    let markdown_bytes = fs::read(&markdown_path).map_err(|error| {
        CommandError::retryable(
            "autonomous_skill_document_read_failed",
            format!(
                "Xero could not read discovered skill document {}: {error}",
                markdown_path.display()
            ),
        )
    })?;
    let markdown_content = String::from_utf8(markdown_bytes.clone()).map_err(|error| {
        CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            format!(
                "Xero rejected discovered skill `{}` because SKILL.md was not valid UTF-8 text: {error}",
                skill_id
            ),
        )
    })?;
    let frontmatter = parse_skill_frontmatter(&markdown_content)?;
    if frontmatter.name != skill_id {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            format!(
                "Xero rejected discovered skill `{skill_id}` because SKILL.md frontmatter name `{}` did not match.",
                frontmatter.name
            ),
        ));
    }

    let mut supporting_assets = Vec::new();
    if include_supporting_assets {
        for relative_path in asset_paths
            .iter()
            .filter(|path| path.as_str() != "SKILL.md")
        {
            validate_skill_asset_path(relative_path)?;
            let path = skill_directory.join(relative_path);
            let bytes = fs::read(&path).map_err(|error| {
                CommandError::retryable(
                    "autonomous_skill_asset_read_failed",
                    format!(
                        "Xero could not read discovered skill asset {}: {error}",
                        path.display()
                    ),
                )
            })?;
            let content = String::from_utf8(bytes.clone()).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_skill_layout_unsupported",
                    format!(
                        "Xero rejected discovered skill asset `{relative_path}` because it was not valid UTF-8 text: {error}"
                    ),
                )
            })?;
            supporting_assets.push(XeroSkillToolContextAsset {
                relative_path: relative_path.clone(),
                sha256: sha256_hex(&bytes),
                bytes: bytes.len(),
                content,
            });
        }
    }

    validate_skill_tool_context_payload(XeroSkillToolContextPayload {
        contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
        source_id: source_id.to_owned(),
        skill_id: skill_id.to_owned(),
        markdown: XeroSkillToolContextDocument {
            relative_path: "SKILL.md".into(),
            sha256: sha256_hex(&markdown_bytes),
            bytes: markdown_bytes.len(),
            content: markdown_content,
        },
        supporting_assets,
    })
}

pub fn compute_skill_directory_version_hash(
    skill_directory: impl AsRef<Path>,
) -> CommandResult<String> {
    let skill_directory = skill_directory.as_ref();
    let mut files = Vec::new();
    collect_skill_files(skill_directory, skill_directory, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    if files.is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_missing",
            "Xero rejected discovered skill because the skill directory contained no files.",
        ));
    }
    if files.len() > MAX_SKILL_FILES {
        return Err(CommandError::user_fixable(
            "autonomous_skill_layout_unsupported",
            format!(
                "Xero rejected discovered skill because it exceeded the {MAX_SKILL_FILES} file limit."
            ),
        ));
    }

    let mut total_bytes = 0usize;
    let mut has_skill_markdown = false;
    let mut hash_input = Vec::new();
    for (relative_path, path) in &files {
        validate_skill_asset_path(relative_path)?;
        let bytes = fs::read(path).map_err(|error| {
            CommandError::retryable(
                "autonomous_skill_asset_read_failed",
                format!(
                    "Xero could not read skill asset {}: {error}",
                    path.display()
                ),
            )
        })?;
        if bytes.is_empty() || bytes.len() > MAX_SKILL_FILE_BYTES {
            return Err(CommandError::user_fixable(
                "autonomous_skill_layout_unsupported",
                format!(
                    "Xero rejected discovered skill asset `{relative_path}` because assets must be between 1 and {MAX_SKILL_FILE_BYTES} bytes."
                ),
            ));
        }
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_TOTAL_SKILL_BYTES {
            return Err(CommandError::user_fixable(
                "autonomous_skill_layout_unsupported",
                format!(
                    "Xero rejected discovered skill because it exceeded the {MAX_TOTAL_SKILL_BYTES} byte total skill budget."
                ),
            ));
        }
        if relative_path == "SKILL.md" {
            has_skill_markdown = true;
            let markdown = String::from_utf8(bytes.clone()).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_skill_document_invalid",
                    format!(
                        "Xero rejected discovered skill because SKILL.md was not valid UTF-8 text: {error}"
                    ),
                )
            })?;
            parse_skill_frontmatter(&markdown)?;
        } else {
            String::from_utf8(bytes.clone()).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_skill_layout_unsupported",
                    format!(
                        "Xero rejected discovered skill asset `{relative_path}` because it was not valid UTF-8 text: {error}"
                    ),
                )
            })?;
        }
        hash_input.extend_from_slice(relative_path.as_bytes());
        hash_input.push(0);
        hash_input.extend_from_slice(&bytes);
        hash_input.push(0);
    }

    if !has_skill_markdown {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_missing",
            "Xero rejected discovered skill because SKILL.md was missing.",
        ));
    }

    Ok(sha256_hex(&hash_input))
}

fn discover_skill_directory(
    root: SkillDiscoveryRoot,
) -> CommandResult<XeroSkillDirectoryDiscovery> {
    let scan_root = root.scan_root();
    let mut diagnostics = Vec::new();
    if !scan_root.is_dir() {
        diagnostics.push(XeroSkillDiscoveryDiagnostic {
            code: "autonomous_skill_directory_unavailable".into(),
            message: format!(
                "Xero could not scan skill directory {} because it is not available.",
                scan_root.display()
            ),
            relative_path: None,
        });
        return Ok(XeroSkillDirectoryDiscovery {
            candidates: Vec::new(),
            diagnostics,
        });
    }

    let root_canonical = fs::canonicalize(&scan_root).map_err(|error| {
        CommandError::retryable(
            "autonomous_skill_directory_unavailable",
            format!(
                "Xero could not resolve skill directory {}: {error}",
                scan_root.display()
            ),
        )
    })?;

    let mut skill_directories = Vec::new();
    collect_skill_directories(
        &scan_root,
        &root_canonical,
        &scan_root,
        &mut skill_directories,
        &mut diagnostics,
    )?;

    skill_directories.sort();
    let mut seen_skill_ids = BTreeSet::new();
    let mut candidates = Vec::new();
    for skill_directory in skill_directories {
        match inspect_filesystem_skill(&root, &scan_root, &root_canonical, &skill_directory) {
            Ok(candidate) => {
                if !seen_skill_ids.insert(candidate.skill_id.clone()) {
                    diagnostics.push(XeroSkillDiscoveryDiagnostic {
                        code: "autonomous_skill_duplicate_id".into(),
                        message: format!(
                            "Xero skipped duplicate discovered skill id `{}` under {}.",
                            candidate.skill_id,
                            scan_root.display()
                        ),
                        relative_path: path_to_relative_source_path(&scan_root, &skill_directory)
                            .ok(),
                    });
                    continue;
                }
                candidates.push(candidate);
            }
            Err(error) => diagnostics.push(XeroSkillDiscoveryDiagnostic {
                code: error.code,
                message: error.message,
                relative_path: path_to_relative_source_path(&scan_root, &skill_directory).ok(),
            }),
        }
    }

    candidates.sort_by(|left, right| left.source.source_id.cmp(&right.source.source_id));
    Ok(XeroSkillDirectoryDiscovery {
        candidates,
        diagnostics,
    })
}

fn inspect_filesystem_skill(
    root: &SkillDiscoveryRoot,
    scan_root: &Path,
    root_canonical: &Path,
    skill_directory: &Path,
) -> CommandResult<XeroDiscoveredSkill> {
    let skill_canonical = fs::canonicalize(skill_directory).map_err(|error| {
        CommandError::retryable(
            "autonomous_skill_directory_unavailable",
            format!(
                "Xero could not resolve discovered skill directory {}: {error}",
                skill_directory.display()
            ),
        )
    })?;
    if !skill_canonical.starts_with(root_canonical) {
        return Err(CommandError::user_fixable(
            "autonomous_skill_path_outside_root",
            format!(
                "Xero rejected discovered skill directory {} because it resolves outside the declared skill root.",
                skill_directory.display()
            ),
        ));
    }

    let mut files = Vec::new();
    collect_skill_files(skill_directory, skill_directory, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    if files.is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_missing",
            "Xero rejected discovered skill because the skill directory contained no files.",
        ));
    }
    if files.len() > MAX_SKILL_FILES {
        return Err(CommandError::user_fixable(
            "autonomous_skill_layout_unsupported",
            format!(
                "Xero rejected discovered skill because it exceeded the {MAX_SKILL_FILES} file limit."
            ),
        ));
    }

    let mut asset_paths = Vec::new();
    let mut total_bytes = 0usize;
    let mut has_skill_markdown = false;
    let mut skill_markdown = None;
    let mut hash_input = Vec::new();

    for (relative_path, path) in &files {
        validate_skill_asset_path(relative_path)?;
        let bytes = fs::read(path).map_err(|error| {
            CommandError::retryable(
                "autonomous_skill_asset_read_failed",
                format!(
                    "Xero could not read skill asset {}: {error}",
                    path.display()
                ),
            )
        })?;
        if bytes.is_empty() || bytes.len() > MAX_SKILL_FILE_BYTES {
            return Err(CommandError::user_fixable(
                "autonomous_skill_layout_unsupported",
                format!(
                    "Xero rejected discovered skill asset `{relative_path}` because assets must be between 1 and {MAX_SKILL_FILE_BYTES} bytes."
                ),
            ));
        }
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_TOTAL_SKILL_BYTES {
            return Err(CommandError::user_fixable(
                "autonomous_skill_layout_unsupported",
                format!(
                    "Xero rejected discovered skill because it exceeded the {MAX_TOTAL_SKILL_BYTES} byte total skill budget."
                ),
            ));
        }
        if relative_path == "SKILL.md" {
            has_skill_markdown = true;
            skill_markdown = Some(String::from_utf8(bytes.clone()).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_skill_document_invalid",
                    format!(
                        "Xero rejected discovered skill because SKILL.md was not valid UTF-8 text: {error}"
                    ),
                )
            })?);
        } else {
            String::from_utf8(bytes.clone()).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_skill_layout_unsupported",
                    format!(
                        "Xero rejected discovered skill asset `{relative_path}` because it was not valid UTF-8 text: {error}"
                    ),
                )
            })?;
        }
        hash_input.extend_from_slice(relative_path.as_bytes());
        hash_input.push(0);
        hash_input.extend_from_slice(&bytes);
        hash_input.push(0);
        asset_paths.push(relative_path.clone());
    }

    if !has_skill_markdown {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_missing",
            "Xero rejected discovered skill because SKILL.md was missing.",
        ));
    }

    let frontmatter = parse_skill_frontmatter(
        skill_markdown
            .as_deref()
            .expect("checked that SKILL.md exists"),
    )?;
    let source = root.source_record(
        scan_root,
        root_canonical,
        skill_directory,
        &frontmatter.name,
    )?;

    Ok(XeroDiscoveredSkill {
        skill_id: frontmatter.name.clone(),
        name: frontmatter.name,
        description: frontmatter.description,
        user_invocable: frontmatter.user_invocable,
        local_location: skill_canonical.display().to_string(),
        version_hash: sha256_hex(&hash_input),
        asset_paths,
        total_bytes,
        source,
        skill_directory: skill_directory.to_path_buf(),
    })
}

fn collect_skill_directories(
    scan_root: &Path,
    root_canonical: &Path,
    current: &Path,
    skill_directories: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<XeroSkillDiscoveryDiagnostic>,
) -> CommandResult<()> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| {
            CommandError::retryable(
                "autonomous_skill_directory_read_failed",
                format!("Xero could not enumerate {}: {error}", current.display()),
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CommandError::retryable(
                "autonomous_skill_directory_read_failed",
                format!(
                    "Xero could not inspect an entry under {}: {error}",
                    current.display()
                ),
            )
        })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            CommandError::retryable(
                "autonomous_skill_directory_read_failed",
                format!("Xero could not inspect {}: {error}", path.display()),
            )
        })?;
        if metadata.file_type().is_symlink() {
            diagnostics.push(XeroSkillDiscoveryDiagnostic {
                code: "autonomous_skill_path_outside_root".into(),
                message: format!(
                    "Xero skipped {} because skill scanning does not follow symlinks.",
                    path.display()
                ),
                relative_path: path_to_relative_source_path(scan_root, &path).ok(),
            });
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        let canonical = fs::canonicalize(&path).map_err(|error| {
            CommandError::retryable(
                "autonomous_skill_directory_unavailable",
                format!("Xero could not resolve {}: {error}", path.display()),
            )
        })?;
        if !canonical.starts_with(root_canonical) {
            diagnostics.push(XeroSkillDiscoveryDiagnostic {
                code: "autonomous_skill_path_outside_root".into(),
                message: format!(
                    "Xero skipped {} because it resolves outside the declared skill root.",
                    path.display()
                ),
                relative_path: path_to_relative_source_path(scan_root, &path).ok(),
            });
            continue;
        }
        if path.join("SKILL.md").is_file() {
            skill_directories.push(path);
        } else {
            collect_skill_directories(
                scan_root,
                root_canonical,
                &path,
                skill_directories,
                diagnostics,
            )?;
        }
    }

    Ok(())
}

fn collect_skill_files(
    skill_root: &Path,
    current: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> CommandResult<()> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| {
            CommandError::retryable(
                "autonomous_skill_directory_read_failed",
                format!("Xero could not enumerate {}: {error}", current.display()),
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CommandError::retryable(
                "autonomous_skill_directory_read_failed",
                format!(
                    "Xero could not inspect an entry under {}: {error}",
                    current.display()
                ),
            )
        })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            CommandError::retryable(
                "autonomous_skill_directory_read_failed",
                format!("Xero could not inspect {}: {error}", path.display()),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(CommandError::user_fixable(
                "autonomous_skill_path_outside_root",
                format!(
                    "Xero rejected discovered skill because asset {} is a symlink.",
                    path.display()
                ),
            ));
        }
        if metadata.is_dir() {
            collect_skill_files(skill_root, &path, files)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let relative_path = path_to_relative_source_path(skill_root, &path)?;
        files.push((relative_path, path));
    }

    Ok(())
}

fn path_to_relative_source_path(root: &Path, path: &Path) -> CommandResult<String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        CommandError::user_fixable(
            "autonomous_skill_path_outside_root",
            format!(
                "Xero rejected skill path {} because it is outside declared root {}.",
                path.display(),
                root.display()
            ),
        )
    })?;
    let text = relative.to_string_lossy().replace('\\', "/");
    normalize_relative_source_path(&text)
}

fn normalize_required(value: String, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(trimmed.to_owned())
}
