use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::commands::get_runtime_settings::write_json_file_atomically;

use super::inspection::validate_cache_manifest;
use super::source::AutonomousSkillSourceMetadata;

pub(crate) const CACHE_VERSION: u32 = 1;
const CACHE_MANIFEST_FILE_NAME: &str = "manifest.json";
const CACHE_TREES_DIRECTORY_NAME: &str = "trees";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillCacheStatus {
    Miss,
    Hit,
    Refreshed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousSkillCacheError {
    Setup(String),
    Read(String),
    Write(String),
    Decode(String),
    Contract(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillCacheManifest {
    pub version: u32,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub source: AutonomousSkillSourceMetadata,
    pub cached_at: String,
    pub files: Vec<AutonomousSkillCacheManifestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillCacheManifestFile {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousSkillCacheInstallFile {
    pub relative_path: String,
    pub bytes: Vec<u8>,
}

pub trait AutonomousSkillCacheStore: Send + Sync {
    fn load_manifest(
        &self,
        cache_key: &str,
    ) -> Result<Option<AutonomousSkillCacheManifest>, AutonomousSkillCacheError>;

    fn verify_manifest(
        &self,
        cache_key: &str,
        manifest: &AutonomousSkillCacheManifest,
    ) -> Result<String, AutonomousSkillCacheError>;

    fn install(
        &self,
        cache_key: &str,
        manifest: &AutonomousSkillCacheManifest,
        files: &[AutonomousSkillCacheInstallFile],
    ) -> Result<String, AutonomousSkillCacheError>;

    fn load_text_file(
        &self,
        cache_key: &str,
        tree_hash: &str,
        relative_path: &str,
    ) -> Result<String, AutonomousSkillCacheError>;
}

#[derive(Debug, Clone)]
pub struct FilesystemAutonomousSkillCacheStore {
    root: PathBuf,
}

impl FilesystemAutonomousSkillCacheStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn cache_directory(&self, cache_key: &str) -> PathBuf {
        self.root.join(cache_key)
    }

    fn manifest_path(&self, cache_key: &str) -> PathBuf {
        self.cache_directory(cache_key)
            .join(CACHE_MANIFEST_FILE_NAME)
    }

    fn tree_directory(&self, cache_key: &str, tree_hash: &str) -> PathBuf {
        self.cache_directory(cache_key)
            .join(CACHE_TREES_DIRECTORY_NAME)
            .join(tree_hash)
    }
}

impl AutonomousSkillCacheStore for FilesystemAutonomousSkillCacheStore {
    fn load_manifest(
        &self,
        cache_key: &str,
    ) -> Result<Option<AutonomousSkillCacheManifest>, AutonomousSkillCacheError> {
        let manifest_path = self.manifest_path(cache_key);
        if !manifest_path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&manifest_path).map_err(|error| {
            AutonomousSkillCacheError::Read(format!(
                "Xero could not read the autonomous skill cache manifest at {}: {error}",
                manifest_path.display()
            ))
        })?;

        let manifest =
            serde_json::from_str::<AutonomousSkillCacheManifest>(&contents).map_err(|error| {
                AutonomousSkillCacheError::Decode(format!(
                    "Xero could not decode the autonomous skill cache manifest at {}: {error}",
                    manifest_path.display()
                ))
            })?;

        validate_cache_manifest(&manifest)?;
        Ok(Some(manifest))
    }

    fn verify_manifest(
        &self,
        cache_key: &str,
        manifest: &AutonomousSkillCacheManifest,
    ) -> Result<String, AutonomousSkillCacheError> {
        validate_cache_manifest(manifest)?;
        let tree_directory = self.tree_directory(cache_key, &manifest.source.tree_hash);
        if !tree_directory.is_dir() {
            return Err(AutonomousSkillCacheError::Contract(format!(
                "Xero expected an autonomous skill cache tree at {} but it was missing.",
                tree_directory.display()
            )));
        }

        let actual_paths = collect_relative_files(&tree_directory)?;
        let expected_paths = manifest
            .files
            .iter()
            .map(|record| record.relative_path.clone())
            .collect::<BTreeSet<_>>();
        if actual_paths != expected_paths {
            return Err(AutonomousSkillCacheError::Contract(format!(
                "Xero detected autonomous skill cache drift for `{}` because the cached file set no longer matches the manifest.",
                manifest.skill_id
            )));
        }

        for record in &manifest.files {
            let path = tree_directory.join(&record.relative_path);
            let bytes = fs::read(&path).map_err(|error| {
                AutonomousSkillCacheError::Read(format!(
                    "Xero could not read cached autonomous skill file {}: {error}",
                    path.display()
                ))
            })?;
            let digest = sha256_hex(&bytes);
            if digest != record.sha256 || bytes.len() != record.bytes {
                return Err(AutonomousSkillCacheError::Contract(format!(
                    "Xero detected autonomous skill cache drift for `{}` at `{}`.",
                    manifest.skill_id, record.relative_path
                )));
            }
        }

        Ok(tree_directory.display().to_string())
    }

    fn install(
        &self,
        cache_key: &str,
        manifest: &AutonomousSkillCacheManifest,
        files: &[AutonomousSkillCacheInstallFile],
    ) -> Result<String, AutonomousSkillCacheError> {
        validate_cache_manifest(manifest)?;
        let cache_directory = self.cache_directory(cache_key);
        let tree_directory = self.tree_directory(cache_key, &manifest.source.tree_hash);
        fs::create_dir_all(&cache_directory).map_err(|error| {
            AutonomousSkillCacheError::Write(format!(
                "Xero could not prepare the autonomous skill cache directory at {}: {error}",
                cache_directory.display()
            ))
        })?;

        if tree_directory.exists() {
            fs::remove_dir_all(&tree_directory).map_err(|error| {
                AutonomousSkillCacheError::Write(format!(
                    "Xero could not clear the staged autonomous skill cache tree at {}: {error}",
                    tree_directory.display()
                ))
            })?;
        }
        fs::create_dir_all(&tree_directory).map_err(|error| {
            AutonomousSkillCacheError::Write(format!(
                "Xero could not create the autonomous skill cache tree at {}: {error}",
                tree_directory.display()
            ))
        })?;

        for file in files {
            let path = tree_directory.join(&file.relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    AutonomousSkillCacheError::Write(format!(
                        "Xero could not prepare the autonomous skill cache subdirectory at {}: {error}",
                        parent.display()
                    ))
                })?;
            }
            fs::write(&path, &file.bytes).map_err(|error| {
                AutonomousSkillCacheError::Write(format!(
                    "Xero could not write cached autonomous skill file {}: {error}",
                    path.display()
                ))
            })?;
        }

        self.verify_manifest(cache_key, manifest)?;

        let manifest_bytes = serde_json::to_vec_pretty(manifest).map_err(|error| {
            AutonomousSkillCacheError::Write(format!(
                "Xero could not serialize the autonomous skill cache manifest for `{}`: {error}",
                manifest.skill_id
            ))
        })?;
        write_json_file_atomically(
            &self.manifest_path(cache_key),
            &manifest_bytes,
            "autonomous_skill_cache",
        )
        .map_err(|error| {
            AutonomousSkillCacheError::Write(format!(
                "Xero could not persist the autonomous skill cache manifest for `{}`: {error}",
                manifest.skill_id
            ))
        })?;

        Ok(tree_directory.display().to_string())
    }

    fn load_text_file(
        &self,
        cache_key: &str,
        tree_hash: &str,
        relative_path: &str,
    ) -> Result<String, AutonomousSkillCacheError> {
        let path = self
            .tree_directory(cache_key, tree_hash)
            .join(relative_path);
        let bytes = fs::read(&path).map_err(|error| {
            AutonomousSkillCacheError::Read(format!(
                "Xero could not read cached autonomous skill file {}: {error}",
                path.display()
            ))
        })?;
        String::from_utf8(bytes).map_err(|error| {
            AutonomousSkillCacheError::Decode(format!(
                "Xero could not decode cached autonomous skill file {} as UTF-8: {error}",
                path.display()
            ))
        })
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn collect_relative_files(root: &Path) -> Result<BTreeSet<String>, AutonomousSkillCacheError> {
    let mut files = BTreeSet::new();
    collect_relative_files_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_relative_files_inner(
    root: &Path,
    current: &Path,
    files: &mut BTreeSet<String>,
) -> Result<(), AutonomousSkillCacheError> {
    let entries = fs::read_dir(current).map_err(|error| {
        AutonomousSkillCacheError::Read(format!(
            "Xero could not enumerate cached autonomous skill directory {}: {error}",
            current.display()
        ))
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            AutonomousSkillCacheError::Read(format!(
                "Xero could not inspect a cached autonomous skill directory entry under {}: {error}",
                current.display()
            ))
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_relative_files_inner(root, &path, files)?;
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|error| {
            AutonomousSkillCacheError::Read(format!(
                "Xero could not normalize cached autonomous skill path {}: {error}",
                path.display()
            ))
        })?;
        let normalized = path_to_forward_slash(relative);
        files.insert(normalized);
    }

    Ok(())
}

fn path_to_forward_slash(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(segment) => segment.to_str().map(ToOwned::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
