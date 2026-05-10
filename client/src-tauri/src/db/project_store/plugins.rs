use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::database_path_for_repo,
    runtime::{
        plugin_command_stable_id, XeroDiscoveredPlugin, XeroPluginCommandAvailability,
        XeroPluginCommandContribution, XeroPluginManifest, XeroSkillSourceLocator,
        XeroSkillSourceState, XeroSkillTrustState,
    },
};

use super::{open_project_database, InstalledSkillDiagnosticRecord};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPluginRecord {
    pub plugin_id: String,
    pub root_id: String,
    pub root_path: String,
    pub plugin_root_path: String,
    pub manifest_path: String,
    pub manifest_hash: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub state: XeroSkillSourceState,
    pub trust: XeroSkillTrustState,
    pub manifest: XeroPluginManifest,
    pub installed_at: String,
    pub updated_at: String,
    pub last_reloaded_at: Option<String>,
    pub last_diagnostic: Option<InstalledPluginDiagnosticRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledPluginDiagnosticRecord {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommandRegistryRecord {
    pub command_id: String,
    pub plugin_id: String,
    pub contribution_id: String,
    pub label: String,
    pub description: String,
    pub entry: String,
    pub availability: XeroPluginCommandAvailability,
    pub risk_level: crate::runtime::XeroPluginCommandRiskLevel,
    pub approval_policy: crate::runtime::XeroPluginCommandApprovalPolicy,
    pub state_policy: crate::runtime::XeroPluginCommandStatePolicy,
    pub redaction_required: bool,
    pub state: XeroSkillSourceState,
    pub trust: XeroSkillTrustState,
    pub plugin_name: String,
    pub plugin_version: String,
}

struct RawInstalledPluginRow {
    plugin_id: String,
    root_id: String,
    root_path: String,
    plugin_root_path: String,
    manifest_path: String,
    manifest_hash: String,
    name: String,
    version: String,
    description: String,
    state: String,
    trust: String,
    manifest_json: String,
    installed_at: String,
    updated_at: String,
    last_reloaded_at: Option<String>,
    last_diagnostic_json: Option<String>,
}

impl InstalledPluginRecord {
    pub fn from_discovered(
        discovered: &XeroDiscoveredPlugin,
        timestamp: impl Into<String>,
    ) -> CommandResult<Self> {
        let timestamp = timestamp.into();
        let state = if discovered.trust == XeroSkillTrustState::Blocked {
            XeroSkillSourceState::Blocked
        } else {
            XeroSkillSourceState::Enabled
        };
        Self {
            plugin_id: discovered.plugin_id.clone(),
            root_id: discovered.root_id.clone(),
            root_path: discovered.root_path.clone(),
            plugin_root_path: discovered.plugin_root_path.display().to_string(),
            manifest_path: discovered.manifest_path.clone(),
            manifest_hash: discovered.manifest_hash.clone(),
            name: discovered.manifest.name.clone(),
            version: discovered.manifest.version.clone(),
            description: discovered.manifest.description.clone(),
            state,
            trust: discovered.trust,
            manifest: discovered.manifest.clone(),
            installed_at: timestamp.clone(),
            updated_at: timestamp.clone(),
            last_reloaded_at: Some(timestamp),
            last_diagnostic: None,
        }
        .validate()
    }

    fn merge_discovered(
        mut self,
        discovered: &XeroDiscoveredPlugin,
        timestamp: String,
    ) -> CommandResult<Self> {
        let previous_state = self.state;
        let previous_trust = self.trust;
        let installed_at = self.installed_at.clone();
        self = Self::from_discovered(discovered, timestamp.clone())?;
        self.installed_at = installed_at;
        self.state = match previous_state {
            XeroSkillSourceState::Disabled
            | XeroSkillSourceState::Blocked
            | XeroSkillSourceState::Stale => previous_state,
            _ => self.state,
        };
        self.trust = match previous_trust {
            XeroSkillTrustState::UserApproved | XeroSkillTrustState::Blocked => previous_trust,
            _ => self.trust,
        };
        self.updated_at = timestamp.clone();
        self.last_reloaded_at = Some(timestamp);
        self.last_diagnostic = None;
        self.validate()
    }

    fn validate(self) -> CommandResult<Self> {
        let plugin_id = crate::runtime::normalize_plugin_id(&self.plugin_id)?;
        let root_id = crate::runtime::normalize_plugin_contribution_id(&self.root_id)?;
        let root_path = validate_required_text(self.root_path, "rootPath")?;
        let plugin_root_path = validate_required_text(self.plugin_root_path, "pluginRootPath")?;
        let manifest_path = validate_required_text(self.manifest_path, "manifestPath")?;
        let manifest_hash = validate_hash(self.manifest_hash, "manifestHash")?;
        let name = validate_required_text(self.name, "name")?;
        let version = validate_required_text(self.version, "version")?;
        let description = validate_required_text(self.description, "description")?;
        if self.manifest.id != plugin_id {
            return Err(CommandError::user_fixable(
                "installed_plugin_manifest_mismatch",
                format!(
                    "Xero rejected installed plugin `{plugin_id}` because its manifest names `{}`.",
                    self.manifest.id
                ),
            ));
        }
        if self.state == XeroSkillSourceState::Discoverable {
            return Err(CommandError::user_fixable(
                "installed_plugin_state_invalid",
                "Xero durable plugin records cannot remain in the discoverable-only state.",
            ));
        }
        let installed_at = validate_required_text(self.installed_at, "installedAt")?;
        let updated_at = validate_required_text(self.updated_at, "updatedAt")?;
        let last_reloaded_at = self
            .last_reloaded_at
            .map(|value| validate_required_text(value, "lastReloadedAt"))
            .transpose()?;
        let last_diagnostic = self
            .last_diagnostic
            .map(validate_plugin_diagnostic)
            .transpose()?;
        Ok(Self {
            plugin_id,
            root_id,
            root_path,
            plugin_root_path,
            manifest_path,
            manifest_hash,
            name,
            version,
            description,
            state: self.state,
            trust: self.trust,
            manifest: self.manifest,
            installed_at,
            updated_at,
            last_reloaded_at,
            last_diagnostic,
        })
    }
}

pub fn upsert_installed_plugin(
    repo_root: &Path,
    record: InstalledPluginRecord,
) -> CommandResult<InstalledPluginRecord> {
    let record = record.validate()?;
    let connection = open_project_database(repo_root, &database_path_for_repo(repo_root))?;
    upsert_installed_plugin_with_connection(&connection, &record)?;
    load_installed_plugin_by_id(repo_root, &record.plugin_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "installed_plugin_missing",
            "Xero persisted an installed plugin but could not reload it.",
        )
    })
}

pub fn load_installed_plugin_by_id(
    repo_root: &Path,
    plugin_id: &str,
) -> CommandResult<Option<InstalledPluginRecord>> {
    let plugin_id = crate::runtime::normalize_plugin_id(plugin_id)?;
    let connection = open_project_database(repo_root, &database_path_for_repo(repo_root))?;
    connection
        .query_row(
            &installed_plugin_select_sql_with_where("plugin_id = ?1"),
            [plugin_id],
            read_raw_installed_plugin_row,
        )
        .optional()
        .map_err(map_installed_plugin_read_error)?
        .map(decode_installed_plugin_row)
        .transpose()
}

pub fn list_installed_plugins(repo_root: &Path) -> CommandResult<Vec<InstalledPluginRecord>> {
    let connection = open_project_database(repo_root, &database_path_for_repo(repo_root))?;
    query_installed_plugin_rows(
        &connection,
        installed_plugin_select_sql_with_where("1 = 1"),
        rusqlite::params![],
    )
}

pub fn sync_discovered_plugins(
    repo_root: &Path,
    discovered: &[XeroDiscoveredPlugin],
    mark_missing_stale: bool,
) -> CommandResult<Vec<InstalledPluginRecord>> {
    let timestamp = now_timestamp();
    let mut records = Vec::with_capacity(discovered.len());
    for plugin in discovered {
        let next = match load_installed_plugin_by_id(repo_root, &plugin.plugin_id)? {
            Some(existing) => existing.merge_discovered(plugin, timestamp.clone())?,
            None => InstalledPluginRecord::from_discovered(plugin, timestamp.clone())?,
        };
        records.push(upsert_installed_plugin(repo_root, next)?);
    }

    if mark_missing_stale {
        let discovered_ids = discovered
            .iter()
            .map(|plugin| plugin.plugin_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for mut record in list_installed_plugins(repo_root)? {
            if discovered_ids.contains(record.plugin_id.as_str()) {
                continue;
            }
            if record.state != XeroSkillSourceState::Blocked {
                record.state = XeroSkillSourceState::Stale;
            }
            record.updated_at = timestamp.clone();
            record.last_diagnostic = Some(InstalledPluginDiagnosticRecord {
                code: "xero_plugin_source_missing".into(),
                message: "Xero marked this plugin stale because it was not found during reload."
                    .into(),
                retryable: false,
                recorded_at: timestamp.clone(),
            });
            let plugin_id = record.plugin_id.clone();
            upsert_installed_plugin(repo_root, record)?;
            mark_plugin_contributed_skills_unavailable(
                repo_root,
                &plugin_id,
                XeroSkillSourceState::Stale,
                Some("xero_plugin_source_missing"),
                Some(
                    "Xero marked this plugin skill stale because its plugin was not found during reload.",
                ),
            )?;
        }
    }

    list_installed_plugins(repo_root)
}

pub fn set_installed_plugin_enabled(
    repo_root: &Path,
    plugin_id: &str,
    enabled: bool,
) -> CommandResult<InstalledPluginRecord> {
    let mut record = load_installed_plugin_by_id(repo_root, plugin_id)?.ok_or_else(|| {
        CommandError::user_fixable(
            "installed_plugin_not_found",
            format!("Xero could not find installed plugin `{plugin_id}`."),
        )
    })?;
    if record.trust == XeroSkillTrustState::Blocked && enabled {
        return Err(CommandError::user_fixable(
            "installed_plugin_blocked",
            format!("Xero cannot enable blocked plugin `{plugin_id}`."),
        ));
    }
    record.state = if enabled {
        XeroSkillSourceState::Enabled
    } else {
        XeroSkillSourceState::Disabled
    };
    record.updated_at = now_timestamp();
    record.last_diagnostic = None;
    let record = upsert_installed_plugin(repo_root, record)?;
    mark_plugin_contributed_skills_unavailable(
        repo_root,
        &record.plugin_id,
        if enabled {
            XeroSkillSourceState::Enabled
        } else {
            XeroSkillSourceState::Disabled
        },
        None,
        None,
    )?;
    Ok(record)
}

pub fn mark_installed_plugin_removed(
    repo_root: &Path,
    plugin_id: &str,
) -> CommandResult<InstalledPluginRecord> {
    let mut record = load_installed_plugin_by_id(repo_root, plugin_id)?.ok_or_else(|| {
        CommandError::user_fixable(
            "installed_plugin_not_found",
            format!("Xero could not find installed plugin `{plugin_id}`."),
        )
    })?;
    let timestamp = now_timestamp();
    record.state = XeroSkillSourceState::Stale;
    record.updated_at = timestamp.clone();
    record.last_diagnostic = Some(InstalledPluginDiagnosticRecord {
        code: "xero_plugin_removed".into(),
        message: "Xero marked this plugin unavailable at the user's request.".into(),
        retryable: false,
        recorded_at: timestamp,
    });
    let record = upsert_installed_plugin(repo_root, record)?;
    mark_plugin_contributed_skills_unavailable(
        repo_root,
        &record.plugin_id,
        XeroSkillSourceState::Stale,
        Some("xero_plugin_removed"),
        Some("Xero marked this plugin skill unavailable because the plugin was removed."),
    )?;
    Ok(record)
}

pub fn plugin_command_descriptors(
    records: &[InstalledPluginRecord],
    include_unavailable: bool,
) -> CommandResult<Vec<PluginCommandRegistryRecord>> {
    let mut commands = Vec::new();
    for record in records {
        if !include_unavailable
            && (record.state != XeroSkillSourceState::Enabled
                || record.trust == XeroSkillTrustState::Blocked)
        {
            continue;
        }
        for command in &record.manifest.commands {
            commands.push(plugin_command_descriptor(record, command)?);
        }
    }
    commands.sort_by(|left, right| left.command_id.cmp(&right.command_id));
    Ok(commands)
}

fn plugin_command_descriptor(
    record: &InstalledPluginRecord,
    command: &XeroPluginCommandContribution,
) -> CommandResult<PluginCommandRegistryRecord> {
    Ok(PluginCommandRegistryRecord {
        command_id: plugin_command_stable_id(&record.plugin_id, &command.id)?,
        plugin_id: record.plugin_id.clone(),
        contribution_id: command.id.clone(),
        label: command.label.clone(),
        description: command.description.clone(),
        entry: command.entry.clone(),
        availability: command.availability.clone(),
        risk_level: command.risk_level.clone(),
        approval_policy: command.approval_policy.clone(),
        state_policy: command.state_policy.clone(),
        redaction_required: command.redaction_required,
        state: record.state,
        trust: record.trust,
        plugin_name: record.name.clone(),
        plugin_version: record.version.clone(),
    })
}

fn mark_plugin_contributed_skills_unavailable(
    repo_root: &Path,
    plugin_id: &str,
    next_state: XeroSkillSourceState,
    diagnostic_code: Option<&str>,
    diagnostic_message: Option<&str>,
) -> CommandResult<()> {
    let mut records =
        super::list_installed_skills(repo_root, super::InstalledSkillScopeFilter::All)?;
    let timestamp = now_timestamp();
    for mut record in records.drain(..) {
        let XeroSkillSourceLocator::Plugin {
            plugin_id: source_plugin_id,
            ..
        } = &record.source.locator
        else {
            continue;
        };
        if source_plugin_id != plugin_id {
            continue;
        }
        if record.source.state == XeroSkillSourceState::Blocked {
            continue;
        }
        record.source.state = next_state;
        record.updated_at = timestamp.clone();
        record.last_diagnostic = match (diagnostic_code, diagnostic_message) {
            (Some(code), Some(message)) => Some(InstalledSkillDiagnosticRecord {
                code: code.into(),
                message: message.into(),
                retryable: false,
                recorded_at: timestamp.clone(),
            }),
            _ => None,
        };
        super::upsert_installed_skill(repo_root, record)?;
    }
    Ok(())
}

fn upsert_installed_plugin_with_connection(
    connection: &Connection,
    record: &InstalledPluginRecord,
) -> CommandResult<()> {
    let manifest_json = serde_json::to_string(&record.manifest).map_err(|error| {
        CommandError::system_fault(
            "installed_plugin_encode_failed",
            format!(
                "Xero could not encode installed plugin manifest `{}`: {error}",
                record.plugin_id
            ),
        )
    })?;
    let last_diagnostic_json = record
        .last_diagnostic
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "installed_plugin_encode_failed",
                format!(
                    "Xero could not encode installed plugin diagnostic `{}`: {error}",
                    record.plugin_id
                ),
            )
        })?;
    connection
        .execute(
            r#"
            INSERT INTO installed_plugin_records (
                plugin_id,
                root_id,
                root_path,
                plugin_root_path,
                manifest_path,
                manifest_hash,
                name,
                version,
                description,
                plugin_state,
                trust_state,
                manifest_json,
                installed_at,
                updated_at,
                last_reloaded_at,
                last_diagnostic_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(plugin_id) DO UPDATE SET
                root_id = excluded.root_id,
                root_path = excluded.root_path,
                plugin_root_path = excluded.plugin_root_path,
                manifest_path = excluded.manifest_path,
                manifest_hash = excluded.manifest_hash,
                name = excluded.name,
                version = excluded.version,
                description = excluded.description,
                plugin_state = excluded.plugin_state,
                trust_state = excluded.trust_state,
                manifest_json = excluded.manifest_json,
                updated_at = excluded.updated_at,
                last_reloaded_at = excluded.last_reloaded_at,
                last_diagnostic_json = excluded.last_diagnostic_json
            "#,
            params![
                record.plugin_id,
                record.root_id,
                record.root_path,
                record.plugin_root_path,
                record.manifest_path,
                record.manifest_hash,
                record.name,
                record.version,
                record.description,
                skill_state_sql_value(record.state)?,
                trust_state_sql_value(record.trust)?,
                manifest_json,
                record.installed_at,
                record.updated_at,
                record.last_reloaded_at,
                last_diagnostic_json,
            ],
        )
        .map_err(map_installed_plugin_write_error)?;
    Ok(())
}

fn query_installed_plugin_rows<P>(
    connection: &Connection,
    sql: String,
    params: P,
) -> CommandResult<Vec<InstalledPluginRecord>>
where
    P: rusqlite::Params,
{
    let mut statement = connection
        .prepare(&sql)
        .map_err(map_installed_plugin_read_error)?;
    let rows = statement
        .query_map(params, read_raw_installed_plugin_row)
        .map_err(map_installed_plugin_read_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_installed_plugin_read_error)?;
    rows.into_iter().map(decode_installed_plugin_row).collect()
}

fn installed_plugin_select_sql_with_where(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            plugin_id,
            root_id,
            root_path,
            plugin_root_path,
            manifest_path,
            manifest_hash,
            name,
            version,
            description,
            plugin_state,
            trust_state,
            manifest_json,
            installed_at,
            updated_at,
            last_reloaded_at,
            last_diagnostic_json
        FROM installed_plugin_records
        WHERE {where_clause}
        ORDER BY plugin_id ASC
        "#
    )
}

fn read_raw_installed_plugin_row(row: &Row<'_>) -> rusqlite::Result<RawInstalledPluginRow> {
    Ok(RawInstalledPluginRow {
        plugin_id: row.get(0)?,
        root_id: row.get(1)?,
        root_path: row.get(2)?,
        plugin_root_path: row.get(3)?,
        manifest_path: row.get(4)?,
        manifest_hash: row.get(5)?,
        name: row.get(6)?,
        version: row.get(7)?,
        description: row.get(8)?,
        state: row.get(9)?,
        trust: row.get(10)?,
        manifest_json: row.get(11)?,
        installed_at: row.get(12)?,
        updated_at: row.get(13)?,
        last_reloaded_at: row.get(14)?,
        last_diagnostic_json: row.get(15)?,
    })
}

fn decode_installed_plugin_row(raw: RawInstalledPluginRow) -> CommandResult<InstalledPluginRecord> {
    let manifest =
        serde_json::from_str::<XeroPluginManifest>(&raw.manifest_json).map_err(|error| {
            CommandError::system_fault(
                "installed_plugin_record_corrupt",
                format!(
                    "Xero could not decode installed plugin manifest `{}`: {error}",
                    raw.plugin_id
                ),
            )
        })?;
    let state = skill_state_from_sql_value(&raw.state)?;
    let trust = trust_state_from_sql_value(&raw.trust)?;
    let last_diagnostic = raw
        .last_diagnostic_json
        .map(|json| serde_json::from_str::<InstalledPluginDiagnosticRecord>(&json))
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "installed_plugin_record_corrupt",
                format!(
                    "Xero could not decode installed plugin diagnostic `{}`: {error}",
                    raw.plugin_id
                ),
            )
        })?;
    InstalledPluginRecord {
        plugin_id: raw.plugin_id,
        root_id: raw.root_id,
        root_path: raw.root_path,
        plugin_root_path: raw.plugin_root_path,
        manifest_path: raw.manifest_path,
        manifest_hash: raw.manifest_hash,
        name: raw.name,
        version: raw.version,
        description: raw.description,
        state,
        trust,
        manifest,
        installed_at: raw.installed_at,
        updated_at: raw.updated_at,
        last_reloaded_at: raw.last_reloaded_at,
        last_diagnostic,
    }
    .validate()
}

fn validate_plugin_diagnostic(
    diagnostic: InstalledPluginDiagnosticRecord,
) -> CommandResult<InstalledPluginDiagnosticRecord> {
    Ok(InstalledPluginDiagnosticRecord {
        code: validate_required_text(diagnostic.code, "diagnostic.code")?,
        message: validate_required_text(diagnostic.message, "diagnostic.message")?,
        retryable: diagnostic.retryable,
        recorded_at: validate_required_text(diagnostic.recorded_at, "diagnostic.recordedAt")?,
    })
}

fn validate_required_text(value: String, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(trimmed.to_owned())
}

fn validate_hash(value: String, field: &'static str) -> CommandResult<String> {
    let value = validate_required_text(value, field)?;
    if value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(CommandError::user_fixable(
            "installed_plugin_hash_invalid",
            format!("Xero requires `{field}` to be a 64-character hex digest."),
        ));
    }
    Ok(value.to_ascii_lowercase())
}

fn skill_state_sql_value(state: XeroSkillSourceState) -> CommandResult<String> {
    serde_json_string_value(state, "plugin state")
}

fn trust_state_sql_value(trust: XeroSkillTrustState) -> CommandResult<String> {
    serde_json_string_value(trust, "plugin trust state")
}

fn skill_state_from_sql_value(value: &str) -> CommandResult<XeroSkillSourceState> {
    serde_json::from_str::<XeroSkillSourceState>(&format!("\"{value}\"")).map_err(|error| {
        CommandError::system_fault(
            "installed_plugin_record_corrupt",
            format!("Xero could not decode plugin state `{value}`: {error}"),
        )
    })
}

fn trust_state_from_sql_value(value: &str) -> CommandResult<XeroSkillTrustState> {
    serde_json::from_str::<XeroSkillTrustState>(&format!("\"{value}\"")).map_err(|error| {
        CommandError::system_fault(
            "installed_plugin_record_corrupt",
            format!("Xero could not decode plugin trust state `{value}`: {error}"),
        )
    })
}

fn serde_json_string_value<T: Serialize>(value: T, label: &'static str) -> CommandResult<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| {
            CommandError::system_fault(
                "installed_plugin_encode_failed",
                format!("Xero could not encode {label} as a stable string."),
            )
        })
}

fn map_installed_plugin_read_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "installed_plugin_read_failed",
        format!("Xero could not read installed plugin records: {error}"),
    )
}

fn map_installed_plugin_write_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "installed_plugin_write_failed",
        format!("Xero could not persist installed plugin records: {error}"),
    )
}
