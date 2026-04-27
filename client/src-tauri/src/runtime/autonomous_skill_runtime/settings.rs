use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    auth::now_timestamp,
    commands::{
        get_runtime_settings::{remove_file_if_exists, write_json_file_atomically},
        CommandError, CommandResult,
    },
};

use super::{
    inspection::{normalize_relative_source_path, normalize_source_ref, normalize_source_repo},
    runtime::{
        AUTONOMOUS_SKILL_SOURCE_REF, AUTONOMOUS_SKILL_SOURCE_REPO, AUTONOMOUS_SKILL_SOURCE_ROOT,
    },
};

pub const SKILL_SOURCE_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillSourceSettings {
    #[serde(default = "skill_source_settings_schema_version")]
    pub version: u32,
    #[serde(default)]
    pub local_roots: Vec<SkillLocalRootSetting>,
    #[serde(default)]
    pub plugin_roots: Vec<SkillPluginRootSetting>,
    #[serde(default)]
    pub github: SkillGithubSourceSetting,
    #[serde(default)]
    pub projects: Vec<SkillProjectSourceSetting>,
    #[serde(default = "now_timestamp")]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillLocalRootSetting {
    pub root_id: String,
    pub path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "now_timestamp")]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillPluginRootSetting {
    pub root_id: String,
    pub path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "now_timestamp")]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillGithubSourceSetting {
    pub repo: String,
    pub reference: String,
    pub root: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "now_timestamp")]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillProjectSourceSetting {
    pub project_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "now_timestamp")]
    pub updated_at: String,
}

impl Default for SkillGithubSourceSetting {
    fn default() -> Self {
        Self {
            repo: AUTONOMOUS_SKILL_SOURCE_REPO.into(),
            reference: AUTONOMOUS_SKILL_SOURCE_REF.into(),
            root: AUTONOMOUS_SKILL_SOURCE_ROOT.into(),
            enabled: true,
            updated_at: now_timestamp(),
        }
    }
}

impl Default for SkillSourceSettings {
    fn default() -> Self {
        Self {
            version: SKILL_SOURCE_SETTINGS_SCHEMA_VERSION,
            local_roots: default_local_root_settings(),
            plugin_roots: default_plugin_root_settings(),
            github: SkillGithubSourceSetting::default(),
            projects: Vec::new(),
            updated_at: now_timestamp(),
        }
    }
}

impl SkillSourceSettings {
    pub fn project_discovery_enabled(&self, project_id: &str) -> bool {
        self.projects
            .iter()
            .find(|project| project.project_id == project_id.trim())
            .map(|project| project.enabled)
            .unwrap_or(true)
    }

    pub fn enabled_local_roots(&self) -> Vec<SkillLocalRootSetting> {
        self.local_roots
            .iter()
            .filter(|root| root.enabled)
            .cloned()
            .collect()
    }

    pub fn enabled_plugin_roots(&self) -> Vec<SkillPluginRootSetting> {
        self.plugin_roots
            .iter()
            .filter(|root| root.enabled)
            .cloned()
            .collect()
    }

    pub fn upsert_local_root(
        mut self,
        root_id: Option<String>,
        path: String,
        enabled: bool,
    ) -> CommandResult<Self> {
        let root = normalize_local_root_setting(root_id, path, enabled)?;
        match self
            .local_roots
            .iter_mut()
            .find(|current| current.root_id == root.root_id)
        {
            Some(current) => *current = root,
            None => self.local_roots.push(root),
        }
        self.updated_at = now_timestamp();
        self.validate()
    }

    pub fn remove_local_root(mut self, root_id: &str) -> CommandResult<Self> {
        let root_id = normalize_required(root_id, "rootId")?;
        let before = self.local_roots.len();
        self.local_roots.retain(|root| root.root_id != root_id);
        if self.local_roots.len() == before {
            return Err(CommandError::user_fixable(
                "skill_source_local_root_not_found",
                format!("Cadence could not find local skill root `{root_id}`."),
            ));
        }
        self.updated_at = now_timestamp();
        self.validate()
    }

    pub fn upsert_plugin_root(
        mut self,
        root_id: Option<String>,
        path: String,
        enabled: bool,
    ) -> CommandResult<Self> {
        let root = normalize_plugin_root_setting(root_id, path, enabled)?;
        match self
            .plugin_roots
            .iter_mut()
            .find(|current| current.root_id == root.root_id)
        {
            Some(current) => *current = root,
            None => self.plugin_roots.push(root),
        }
        self.updated_at = now_timestamp();
        self.validate()
    }

    pub fn remove_plugin_root(mut self, root_id: &str) -> CommandResult<Self> {
        let root_id = normalize_required(root_id, "rootId")?;
        let before = self.plugin_roots.len();
        self.plugin_roots.retain(|root| root.root_id != root_id);
        if self.plugin_roots.len() == before {
            return Err(CommandError::user_fixable(
                "plugin_source_root_not_found",
                format!("Cadence could not find plugin root `{root_id}`."),
            ));
        }
        self.updated_at = now_timestamp();
        self.validate()
    }

    pub fn update_project(mut self, project_id: String, enabled: bool) -> CommandResult<Self> {
        let project_id = normalize_required(&project_id, "projectId")?;
        let updated_at = now_timestamp();
        match self
            .projects
            .iter_mut()
            .find(|project| project.project_id == project_id)
        {
            Some(project) => {
                project.enabled = enabled;
                project.updated_at = updated_at;
            }
            None => self.projects.push(SkillProjectSourceSetting {
                project_id,
                enabled,
                updated_at,
            }),
        }
        self.updated_at = now_timestamp();
        self.validate()
    }

    pub fn update_github(
        mut self,
        repo: String,
        reference: String,
        root: String,
        enabled: bool,
    ) -> CommandResult<Self> {
        self.github = SkillGithubSourceSetting {
            repo: normalize_source_repo(&repo)?,
            reference: normalize_source_ref(&reference)?,
            root: normalize_relative_source_path(&root)?,
            enabled,
            updated_at: now_timestamp(),
        };
        self.updated_at = now_timestamp();
        self.validate()
    }

    pub fn validate(self) -> CommandResult<Self> {
        if self.version != SKILL_SOURCE_SETTINGS_SCHEMA_VERSION {
            return Err(CommandError::user_fixable(
                "skill_source_settings_version_unsupported",
                format!(
                    "Cadence rejected skill source settings version `{}` because only version `{SKILL_SOURCE_SETTINGS_SCHEMA_VERSION}` is supported.",
                    self.version
                ),
            ));
        }

        let mut root_ids = BTreeSet::new();
        let mut root_paths = BTreeSet::new();
        let mut local_roots = Vec::with_capacity(self.local_roots.len());
        for root in self.local_roots {
            let root = normalize_local_root_setting_with_updated_at(
                Some(root.root_id),
                root.path,
                root.enabled,
                Some(root.updated_at),
            )?;
            if !root_ids.insert(root.root_id.clone()) {
                return Err(CommandError::user_fixable(
                    "skill_source_settings_duplicate_root",
                    format!(
                        "Cadence rejected duplicate local skill root id `{}`.",
                        root.root_id
                    ),
                ));
            }
            if !root_paths.insert(root.path.clone()) {
                return Err(CommandError::user_fixable(
                    "skill_source_settings_duplicate_root",
                    format!(
                        "Cadence rejected duplicate local skill root path `{}`.",
                        root.path
                    ),
                ));
            }
            local_roots.push(root);
        }
        local_roots.sort_by(|left, right| left.root_id.cmp(&right.root_id));

        let mut plugin_root_ids = BTreeSet::new();
        let mut plugin_root_paths = BTreeSet::new();
        let mut plugin_roots = Vec::with_capacity(self.plugin_roots.len());
        for root in self.plugin_roots {
            let root = normalize_plugin_root_setting_with_updated_at(
                Some(root.root_id),
                root.path,
                root.enabled,
                Some(root.updated_at),
            )?;
            if !plugin_root_ids.insert(root.root_id.clone()) {
                return Err(CommandError::user_fixable(
                    "plugin_source_settings_duplicate_root",
                    format!(
                        "Cadence rejected duplicate plugin root id `{}`.",
                        root.root_id
                    ),
                ));
            }
            if !plugin_root_paths.insert(root.path.clone()) {
                return Err(CommandError::user_fixable(
                    "plugin_source_settings_duplicate_root",
                    format!(
                        "Cadence rejected duplicate plugin root path `{}`.",
                        root.path
                    ),
                ));
            }
            plugin_roots.push(root);
        }
        plugin_roots.sort_by(|left, right| left.root_id.cmp(&right.root_id));

        let mut project_ids = BTreeSet::new();
        let mut projects = Vec::with_capacity(self.projects.len());
        for project in self.projects {
            let project_id = normalize_required(&project.project_id, "projectId")?;
            if !project_ids.insert(project_id.clone()) {
                return Err(CommandError::user_fixable(
                    "skill_source_settings_duplicate_project",
                    format!(
                        "Cadence rejected duplicate project skill source setting `{project_id}`."
                    ),
                ));
            }
            projects.push(SkillProjectSourceSetting {
                project_id,
                enabled: project.enabled,
                updated_at: normalize_required(&project.updated_at, "project.updatedAt")?,
            });
        }
        projects.sort_by(|left, right| left.project_id.cmp(&right.project_id));

        Ok(Self {
            version: SKILL_SOURCE_SETTINGS_SCHEMA_VERSION,
            local_roots,
            plugin_roots,
            github: SkillGithubSourceSetting {
                repo: normalize_source_repo(&self.github.repo)?,
                reference: normalize_source_ref(&self.github.reference)?,
                root: normalize_relative_source_path(&self.github.root)?,
                enabled: self.github.enabled,
                updated_at: normalize_required(&self.github.updated_at, "github.updatedAt")?,
            },
            projects,
            updated_at: normalize_required(&self.updated_at, "updatedAt")?,
        })
    }
}

pub fn load_skill_source_settings_from_path(path: &Path) -> CommandResult<SkillSourceSettings> {
    let connection = crate::global_db::open_global_database(path)?;

    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM skill_sources WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .ok();

    let Some(payload) = payload else {
        return Ok(SkillSourceSettings::default());
    };

    let parsed = serde_json::from_str::<SkillSourceSettings>(&payload).map_err(|error| {
        CommandError::user_fixable(
            "skill_source_settings_decode_failed",
            format!(
                "Cadence could not decode skill source settings stored in the global database: {error}"
            ),
        )
    })?;
    parsed.validate()
}

pub fn persist_skill_source_settings(
    path: &Path,
    settings: SkillSourceSettings,
) -> CommandResult<SkillSourceSettings> {
    let normalized = settings.validate()?;
    let payload = serde_json::to_string(&normalized).map_err(|error| {
        CommandError::system_fault(
            "skill_source_settings_serialize_failed",
            format!("Cadence could not serialize the skill source settings update: {error}"),
        )
    })?;

    let connection = crate::global_db::open_global_database(path)?;
    connection
        .execute(
            "INSERT INTO skill_sources (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                payload = excluded.payload,
                updated_at = excluded.updated_at",
            rusqlite::params![payload, normalized.updated_at],
        )
        .map_err(|error| {
            CommandError::retryable(
                "skill_source_settings_write_failed",
                format!("Cadence could not persist skill source settings: {error}"),
            )
        })?;

    Ok(normalized)
}

fn default_local_root_settings() -> Vec<SkillLocalRootSetting> {
    dirs::home_dir()
        .map(|home| home.join(".cadence").join("skills"))
        .filter(|root| root.is_dir())
        .and_then(|root| {
            normalize_local_root_setting(None, root.to_string_lossy().into_owned(), true).ok()
        })
        .into_iter()
        .collect()
}

fn default_plugin_root_settings() -> Vec<SkillPluginRootSetting> {
    dirs::home_dir()
        .map(|home| home.join(".cadence").join("plugins"))
        .filter(|root| root.is_dir())
        .and_then(|root| {
            normalize_plugin_root_setting(None, root.to_string_lossy().into_owned(), true).ok()
        })
        .into_iter()
        .collect()
}

fn normalize_local_root_setting(
    root_id: Option<String>,
    path: String,
    enabled: bool,
) -> CommandResult<SkillLocalRootSetting> {
    normalize_local_root_setting_with_updated_at(root_id, path, enabled, None)
}

fn normalize_local_root_setting_with_updated_at(
    root_id: Option<String>,
    path: String,
    enabled: bool,
    updated_at: Option<String>,
) -> CommandResult<SkillLocalRootSetting> {
    let raw_path = normalize_required(&path, "path")?;
    let path = PathBuf::from(&raw_path);
    if !path.is_absolute() {
        return Err(CommandError::user_fixable(
            "skill_source_path_unsafe",
            "Cadence requires local skill directories to use absolute paths.",
        ));
    }
    let canonical = fs::canonicalize(&path).map_err(|error| {
        CommandError::user_fixable(
            "skill_source_path_unavailable",
            format!(
                "Cadence could not resolve local skill directory {}: {error}",
                path.display()
            ),
        )
    })?;
    if !canonical.is_dir() {
        return Err(CommandError::user_fixable(
            "skill_source_path_unsafe",
            format!(
                "Cadence rejected local skill source {} because it is not a directory.",
                canonical.display()
            ),
        ));
    }
    let canonical_path = canonical.to_string_lossy().into_owned();
    let root_id = match root_id.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }) {
        Some(value) => normalize_root_id(&value)?,
        None => local_root_id_for_path(&canonical_path),
    };

    Ok(SkillLocalRootSetting {
        root_id,
        path: canonical_path,
        enabled,
        updated_at: updated_at
            .map(|value| normalize_required(&value, "localRoot.updatedAt"))
            .transpose()?
            .unwrap_or_else(now_timestamp),
    })
}

fn normalize_plugin_root_setting(
    root_id: Option<String>,
    path: String,
    enabled: bool,
) -> CommandResult<SkillPluginRootSetting> {
    normalize_plugin_root_setting_with_updated_at(root_id, path, enabled, None)
}

fn normalize_plugin_root_setting_with_updated_at(
    root_id: Option<String>,
    path: String,
    enabled: bool,
    updated_at: Option<String>,
) -> CommandResult<SkillPluginRootSetting> {
    let raw_path = normalize_required(&path, "path")?;
    let path = PathBuf::from(&raw_path);
    if !path.is_absolute() {
        return Err(CommandError::user_fixable(
            "plugin_source_path_unsafe",
            "Cadence requires plugin directories to use absolute paths.",
        ));
    }
    let canonical = fs::canonicalize(&path).map_err(|error| {
        CommandError::user_fixable(
            "plugin_source_path_unavailable",
            format!("Cadence could not access plugin directory `{raw_path}`: {error}"),
        )
    })?;
    if !canonical.is_dir() {
        return Err(CommandError::user_fixable(
            "plugin_source_path_unavailable",
            format!(
                "Cadence requires plugin root `{}` to be a directory.",
                canonical.display()
            ),
        ));
    }

    let path_string = canonical.to_string_lossy().into_owned();
    let root_id = match root_id.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }) {
        Some(value) => normalize_root_id(&value)?,
        None => plugin_root_id_for_path(&path_string),
    };
    Ok(SkillPluginRootSetting {
        root_id,
        path: path_string,
        enabled,
        updated_at: updated_at
            .map(|value| normalize_required(&value, "pluginRoot.updatedAt"))
            .transpose()?
            .unwrap_or_else(now_timestamp),
    })
}

fn normalize_root_id(value: &str) -> CommandResult<String> {
    let trimmed = normalize_required(value, "rootId")?;
    if !trimmed
        .chars()
        .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == '-')
    {
        return Err(CommandError::user_fixable(
            "skill_source_root_id_invalid",
            "Cadence requires source root ids to be lowercase kebab-case values.",
        ));
    }
    Ok(trimmed)
}

fn local_root_id_for_path(path: &str) -> String {
    format!("local-{}", root_id_hash(path))
}

fn plugin_root_id_for_path(path: &str) -> String {
    format!("plugin-{}", root_id_hash(path))
}

fn root_id_hash(path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    hex[..12].to_owned()
}

fn snapshot_existing_file(path: &Path) -> CommandResult<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CommandError::retryable(
            "skill_source_settings_read_failed",
            format!(
                "Cadence could not snapshot the app-local skill source settings file at {} before updating it: {error}",
                path.display()
            ),
        )),
    }
}

fn restore_file_snapshot(path: &Path, snapshot: Option<&[u8]>) -> CommandResult<()> {
    match snapshot {
        Some(bytes) => write_json_file_atomically(path, bytes, "skill_source_settings_rollback"),
        None => remove_file_if_exists(path, "skill_source_settings_rollback"),
    }
}

fn normalize_required(value: &str, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(trimmed.to_owned())
}

const fn skill_source_settings_schema_version() -> u32 {
    SKILL_SOURCE_SETTINGS_SCHEMA_VERSION
}

const fn default_true() -> bool {
    true
}
