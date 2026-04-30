use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::database_path_for_repo,
    runtime::{
        AutonomousSkillRegistryFailure, AutonomousSkillRegistryOperation,
        AutonomousSkillRegistrySink, AutonomousSkillRegistrySuccess, XeroDiscoveredSkill,
        XeroSkillSourceRecord, XeroSkillSourceScope, XeroSkillSourceState, XeroSkillTrustState,
    },
};

use super::open_project_database;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledSkillRecord {
    pub source: XeroSkillSourceRecord,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub cache_key: Option<String>,
    pub local_location: Option<String>,
    pub version_hash: Option<String>,
    pub installed_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
    pub last_diagnostic: Option<InstalledSkillDiagnosticRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledSkillDiagnosticRecord {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstalledSkillScopeFilter {
    All,
    Global,
    Project {
        project_id: String,
        include_global: bool,
    },
}

#[derive(Debug, Clone)]
pub struct ProjectStoreInstalledSkillRegistry {
    repo_root: PathBuf,
    scope: XeroSkillSourceScope,
}

struct RawInstalledSkillRow {
    source_id: String,
    scope_kind: String,
    project_id: Option<String>,
    contract_version: i64,
    skill_id: String,
    name: String,
    description: String,
    user_invocable: Option<i64>,
    source_state: String,
    trust_state: String,
    source_json: String,
    cache_key: Option<String>,
    local_location: Option<String>,
    version_hash: Option<String>,
    installed_at: String,
    updated_at: String,
    last_used_at: Option<String>,
    last_diagnostic_json: Option<String>,
}

impl InstalledSkillRecord {
    pub fn from_discovered_skill(
        candidate: &XeroDiscoveredSkill,
        state: XeroSkillSourceState,
        trust: XeroSkillTrustState,
        timestamp: impl Into<String>,
    ) -> CommandResult<Self> {
        let mut source = candidate.source.clone();
        source.state = state;
        source.trust = trust;
        let timestamp = timestamp.into();
        Self {
            source,
            skill_id: candidate.skill_id.clone(),
            name: candidate.name.clone(),
            description: candidate.description.clone(),
            user_invocable: candidate.user_invocable,
            cache_key: None,
            local_location: Some(candidate.local_location.clone()),
            version_hash: Some(candidate.version_hash.clone()),
            installed_at: timestamp.clone(),
            updated_at: timestamp,
            last_used_at: None,
            last_diagnostic: None,
        }
        .validate()
    }

    fn validate(self) -> CommandResult<Self> {
        let source = self.source.validate()?;
        if source.state == XeroSkillSourceState::Discoverable {
            return Err(CommandError::user_fixable(
                "installed_skill_state_invalid",
                "Xero durable installed-skill records cannot remain in the discoverable-only state.",
            ));
        }
        let skill_id = validate_required_text(self.skill_id, "skillId")?;
        if skill_id != source.locator.skill_id() {
            return Err(CommandError::user_fixable(
                "installed_skill_metadata_invalid",
                format!(
                    "Xero rejected installed skill metadata for `{skill_id}` because its source locator names `{}`.",
                    source.locator.skill_id()
                ),
            ));
        }
        let name = validate_required_text(self.name, "name")?;
        let description = validate_required_text(self.description, "description")?;
        let cache_key = normalize_optional_text(self.cache_key, "cacheKey")?;
        let local_location = normalize_optional_text(self.local_location, "localLocation")?;
        if cache_key.is_none() && local_location.is_none() {
            return Err(CommandError::user_fixable(
                "installed_skill_location_missing",
                "Xero requires installed skill records to keep a cache key or local location.",
            ));
        }
        if matches!(
            &source.locator,
            crate::runtime::XeroSkillSourceLocator::Github { .. }
        ) && cache_key.is_none()
        {
            return Err(CommandError::user_fixable(
                "installed_skill_location_missing",
                "Xero requires GitHub-backed installed skills to keep their autonomous cache key.",
            ));
        }
        let version_hash = normalize_optional_text(self.version_hash, "versionHash")?;
        let installed_at = validate_required_text(self.installed_at, "installedAt")?;
        let updated_at = validate_required_text(self.updated_at, "updatedAt")?;
        let last_used_at = normalize_optional_text(self.last_used_at, "lastUsedAt")?;
        let last_diagnostic = self.last_diagnostic.map(validate_diagnostic).transpose()?;

        Ok(Self {
            source,
            skill_id,
            name,
            description,
            user_invocable: self.user_invocable,
            cache_key,
            local_location,
            version_hash,
            installed_at,
            updated_at,
            last_used_at,
            last_diagnostic,
        })
    }
}

impl InstalledSkillScopeFilter {
    pub fn project(
        project_id: impl Into<String>,
        include_global: bool,
    ) -> CommandResult<InstalledSkillScopeFilter> {
        Ok(InstalledSkillScopeFilter::Project {
            project_id: validate_required_text(project_id.into(), "projectId")?,
            include_global,
        })
    }
}

impl ProjectStoreInstalledSkillRegistry {
    pub fn global(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            scope: XeroSkillSourceScope::global(),
        }
    }

    pub fn project(
        repo_root: impl Into<PathBuf>,
        project_id: impl Into<String>,
    ) -> CommandResult<Self> {
        Ok(Self {
            repo_root: repo_root.into(),
            scope: XeroSkillSourceScope::project(project_id.into())?,
        })
    }
}

impl AutonomousSkillRegistrySink for ProjectStoreInstalledSkillRegistry {
    fn record_success(&self, event: &AutonomousSkillRegistrySuccess) -> CommandResult<()> {
        let source = XeroSkillSourceRecord::github_autonomous(
            self.scope.clone(),
            &event.source,
            XeroSkillSourceState::Enabled,
            XeroSkillTrustState::Trusted,
        )?;
        let timestamp = now_timestamp();
        upsert_installed_skill(
            &self.repo_root,
            InstalledSkillRecord {
                source,
                skill_id: event.skill_id.clone(),
                name: event.name.clone(),
                description: event.description.clone(),
                user_invocable: event.user_invocable,
                cache_key: Some(event.cache_key.clone()),
                local_location: Some(event.cache_directory.clone()),
                version_hash: Some(event.source.tree_hash.clone()),
                installed_at: timestamp.clone(),
                updated_at: timestamp.clone(),
                last_used_at: if event.operation == AutonomousSkillRegistryOperation::Invoke {
                    Some(timestamp)
                } else {
                    None
                },
                last_diagnostic: None,
            },
        )?;
        Ok(())
    }

    fn record_failure(&self, event: &AutonomousSkillRegistryFailure) -> CommandResult<()> {
        record_installed_skill_failure(&self.repo_root, &self.scope, event)
    }
}

pub fn upsert_installed_skill(
    repo_root: &Path,
    record: InstalledSkillRecord,
) -> CommandResult<InstalledSkillRecord> {
    let record = record.validate()?;
    let connection = open_project_database(repo_root, &database_path_for_repo(repo_root))?;
    upsert_installed_skill_with_connection(&connection, &record)?;
    load_installed_skill_by_source_id(repo_root, &record.source.source_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "installed_skill_missing",
            "Xero persisted an installed skill but could not reload it.",
        )
    })
}

pub fn load_installed_skill_by_source_id(
    repo_root: &Path,
    source_id: &str,
) -> CommandResult<Option<InstalledSkillRecord>> {
    let source_id = validate_required_text(source_id.to_owned(), "sourceId")?;
    let connection = open_project_database(repo_root, &database_path_for_repo(repo_root))?;
    let raw = connection
        .query_row(
            &installed_skill_select_sql_with_where("source_id = ?1"),
            [source_id],
            read_raw_installed_skill_row,
        )
        .optional()
        .map_err(map_installed_skill_read_error)?;
    raw.map(decode_installed_skill_row).transpose()
}

pub fn list_installed_skills(
    repo_root: &Path,
    filter: InstalledSkillScopeFilter,
) -> CommandResult<Vec<InstalledSkillRecord>> {
    let connection = open_project_database(repo_root, &database_path_for_repo(repo_root))?;
    match filter {
        InstalledSkillScopeFilter::All => query_installed_skill_rows(
            &connection,
            installed_skill_select_sql_with_where("1 = 1"),
            rusqlite::params![],
        ),
        InstalledSkillScopeFilter::Global => query_installed_skill_rows(
            &connection,
            installed_skill_select_sql_with_where("scope_kind = 'global'"),
            rusqlite::params![],
        ),
        InstalledSkillScopeFilter::Project {
            project_id,
            include_global,
        } => {
            let project_id = validate_required_text(project_id, "projectId")?;
            if include_global {
                query_installed_skill_rows(
                    &connection,
                    installed_skill_select_sql_with_where(
                        "scope_kind = 'global' OR (scope_kind = 'project' AND project_id = ?1)",
                    ),
                    [project_id],
                )
            } else {
                query_installed_skill_rows(
                    &connection,
                    installed_skill_select_sql_with_where(
                        "scope_kind = 'project' AND project_id = ?1",
                    ),
                    [project_id],
                )
            }
        }
    }
}

pub fn set_installed_skill_enabled(
    repo_root: &Path,
    source_id: &str,
    enabled: bool,
    updated_at: impl Into<String>,
) -> CommandResult<InstalledSkillRecord> {
    let mut record = load_installed_skill_by_source_id(repo_root, source_id)?.ok_or_else(|| {
        CommandError::user_fixable(
            "installed_skill_not_found",
            format!("Xero could not find installed skill source `{source_id}`."),
        )
    })?;
    if enabled
        && (record.source.state == XeroSkillSourceState::Blocked
            || record.source.trust == XeroSkillTrustState::Blocked)
    {
        return Err(CommandError::user_fixable(
            "installed_skill_blocked",
            format!("Xero cannot enable blocked skill source `{source_id}`."),
        ));
    }
    let next_state = if enabled {
        XeroSkillSourceState::Enabled
    } else {
        XeroSkillSourceState::Disabled
    };
    crate::runtime::validate_skill_source_state_transition(record.source.state, next_state)?;
    record.source.state = next_state;
    record.updated_at = updated_at.into();
    record.last_diagnostic = None;
    upsert_installed_skill(repo_root, record)
}

pub fn remove_installed_skill(repo_root: &Path, source_id: &str) -> CommandResult<bool> {
    let source_id = validate_required_text(source_id.to_owned(), "sourceId")?;
    let connection = open_project_database(repo_root, &database_path_for_repo(repo_root))?;
    let deleted = connection
        .execute(
            "DELETE FROM installed_skill_records WHERE source_id = ?1",
            [source_id],
        )
        .map_err(map_installed_skill_write_error)?;
    Ok(deleted > 0)
}

fn record_installed_skill_failure(
    repo_root: &Path,
    scope: &XeroSkillSourceScope,
    event: &AutonomousSkillRegistryFailure,
) -> CommandResult<()> {
    let source = XeroSkillSourceRecord::github_autonomous(
        scope.clone(),
        &event.source,
        XeroSkillSourceState::Failed,
        XeroSkillTrustState::Trusted,
    )?;
    let timestamp = now_timestamp();
    let diagnostic = InstalledSkillDiagnosticRecord {
        code: event.diagnostic.code.clone(),
        message: event.diagnostic.message.clone(),
        retryable: event.diagnostic.retryable,
        recorded_at: timestamp.clone(),
    };
    let mut record = load_installed_skill_by_source_id(repo_root, &source.source_id)?.unwrap_or(
        InstalledSkillRecord {
            source: source.clone(),
            skill_id: event.skill_id.clone(),
            name: event.skill_id.clone(),
            description: "Skill install failed before Xero resolved metadata.".into(),
            user_invocable: None,
            cache_key: Some(event.cache_key.clone()),
            local_location: None,
            version_hash: Some(event.source.tree_hash.clone()),
            installed_at: timestamp.clone(),
            updated_at: timestamp.clone(),
            last_used_at: None,
            last_diagnostic: None,
        },
    );
    record.source = source;
    if record.cache_key.is_none() {
        record.cache_key = Some(event.cache_key.clone());
    }
    record.version_hash = Some(event.source.tree_hash.clone());
    record.updated_at = timestamp;
    record.last_diagnostic = Some(diagnostic);
    upsert_installed_skill(repo_root, record)?;
    Ok(())
}

fn upsert_installed_skill_with_connection(
    connection: &Connection,
    record: &InstalledSkillRecord,
) -> CommandResult<()> {
    let source_json = serde_json::to_string(&record.source).map_err(|error| {
        CommandError::system_fault(
            "installed_skill_encode_failed",
            format!(
                "Xero could not encode installed skill source `{}`: {error}",
                record.source.source_id
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
                "installed_skill_encode_failed",
                format!(
                    "Xero could not encode installed skill diagnostic `{}`: {error}",
                    record.source.source_id
                ),
            )
        })?;
    let scope_kind = scope_kind_sql_value(&record.source.scope);
    let project_id = scope_project_id(&record.source.scope);
    let source_state = skill_state_sql_value(record.source.state)?;
    let trust_state = trust_state_sql_value(record.source.trust)?;
    let user_invocable = record.user_invocable.map(|value| if value { 1 } else { 0 });

    connection
        .execute(
            r#"
            INSERT INTO installed_skill_records (
                source_id,
                scope_kind,
                project_id,
                contract_version,
                skill_id,
                name,
                description,
                user_invocable,
                source_state,
                trust_state,
                source_json,
                cache_key,
                local_location,
                version_hash,
                installed_at,
                updated_at,
                last_used_at,
                last_diagnostic_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
            ON CONFLICT(source_id) DO UPDATE SET
                scope_kind = excluded.scope_kind,
                project_id = excluded.project_id,
                contract_version = excluded.contract_version,
                skill_id = excluded.skill_id,
                name = excluded.name,
                description = excluded.description,
                user_invocable = excluded.user_invocable,
                source_state = excluded.source_state,
                trust_state = excluded.trust_state,
                source_json = excluded.source_json,
                cache_key = excluded.cache_key,
                local_location = excluded.local_location,
                version_hash = excluded.version_hash,
                updated_at = excluded.updated_at,
                last_used_at = COALESCE(excluded.last_used_at, installed_skill_records.last_used_at),
                last_diagnostic_json = excluded.last_diagnostic_json
            "#,
            params![
                record.source.source_id,
                scope_kind,
                project_id,
                record.source.contract_version as i64,
                record.skill_id,
                record.name,
                record.description,
                user_invocable,
                source_state,
                trust_state,
                source_json,
                record.cache_key,
                record.local_location,
                record.version_hash,
                record.installed_at,
                record.updated_at,
                record.last_used_at,
                last_diagnostic_json,
            ],
        )
        .map_err(map_installed_skill_write_error)?;
    Ok(())
}

fn query_installed_skill_rows<P>(
    connection: &Connection,
    sql: String,
    params: P,
) -> CommandResult<Vec<InstalledSkillRecord>>
where
    P: rusqlite::Params,
{
    let mut statement = connection
        .prepare(&sql)
        .map_err(map_installed_skill_read_error)?;
    let rows = statement
        .query_map(params, read_raw_installed_skill_row)
        .map_err(map_installed_skill_read_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_installed_skill_read_error)?;
    rows.into_iter().map(decode_installed_skill_row).collect()
}

fn installed_skill_select_sql_with_where(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            source_id,
            scope_kind,
            project_id,
            contract_version,
            skill_id,
            name,
            description,
            user_invocable,
            source_state,
            trust_state,
            source_json,
            cache_key,
            local_location,
            version_hash,
            installed_at,
            updated_at,
            last_used_at,
            last_diagnostic_json
        FROM installed_skill_records
        WHERE {where_clause}
        ORDER BY scope_kind ASC, project_id ASC, skill_id ASC, source_id ASC
        "#
    )
}

fn read_raw_installed_skill_row(row: &Row<'_>) -> rusqlite::Result<RawInstalledSkillRow> {
    Ok(RawInstalledSkillRow {
        source_id: row.get(0)?,
        scope_kind: row.get(1)?,
        project_id: row.get(2)?,
        contract_version: row.get(3)?,
        skill_id: row.get(4)?,
        name: row.get(5)?,
        description: row.get(6)?,
        user_invocable: row.get(7)?,
        source_state: row.get(8)?,
        trust_state: row.get(9)?,
        source_json: row.get(10)?,
        cache_key: row.get(11)?,
        local_location: row.get(12)?,
        version_hash: row.get(13)?,
        installed_at: row.get(14)?,
        updated_at: row.get(15)?,
        last_used_at: row.get(16)?,
        last_diagnostic_json: row.get(17)?,
    })
}

fn decode_installed_skill_row(raw: RawInstalledSkillRow) -> CommandResult<InstalledSkillRecord> {
    let source =
        serde_json::from_str::<XeroSkillSourceRecord>(&raw.source_json).map_err(|error| {
            CommandError::system_fault(
                "installed_skill_record_corrupt",
                format!(
                    "Xero could not decode installed skill source `{}`: {error}",
                    raw.source_id
                ),
            )
        })?;
    let source = source.validate()?;
    if source.source_id != raw.source_id
        || source.contract_version as i64 != raw.contract_version
        || scope_kind_sql_value(&source.scope) != raw.scope_kind
        || scope_project_id(&source.scope) != raw.project_id
        || skill_state_sql_value(source.state)? != raw.source_state
        || trust_state_sql_value(source.trust)? != raw.trust_state
    {
        return Err(CommandError::system_fault(
            "installed_skill_record_corrupt",
            format!(
                "Xero rejected installed skill row `{}` because its indexed columns no longer match the source contract.",
                raw.source_id
            ),
        ));
    }
    let last_diagnostic = raw
        .last_diagnostic_json
        .map(|json| serde_json::from_str::<InstalledSkillDiagnosticRecord>(&json))
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "installed_skill_record_corrupt",
                format!(
                    "Xero could not decode installed skill diagnostic `{}`: {error}",
                    raw.source_id
                ),
            )
        })?;
    InstalledSkillRecord {
        source,
        skill_id: raw.skill_id,
        name: raw.name,
        description: raw.description,
        user_invocable: raw.user_invocable.map(|value| value == 1),
        cache_key: raw.cache_key,
        local_location: raw.local_location,
        version_hash: raw.version_hash,
        installed_at: raw.installed_at,
        updated_at: raw.updated_at,
        last_used_at: raw.last_used_at,
        last_diagnostic,
    }
    .validate()
}

fn validate_diagnostic(
    diagnostic: InstalledSkillDiagnosticRecord,
) -> CommandResult<InstalledSkillDiagnosticRecord> {
    Ok(InstalledSkillDiagnosticRecord {
        code: validate_required_text(diagnostic.code, "diagnostic.code")?,
        message: validate_required_text(diagnostic.message, "diagnostic.message")?,
        retryable: diagnostic.retryable,
        recorded_at: validate_required_text(diagnostic.recorded_at, "diagnostic.recordedAt")?,
    })
}

fn normalize_optional_text(
    value: Option<String>,
    field: &'static str,
) -> CommandResult<Option<String>> {
    value
        .map(|value| validate_required_text(value, field))
        .transpose()
}

fn validate_required_text(value: String, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(trimmed.to_owned())
}

fn scope_kind_sql_value(scope: &XeroSkillSourceScope) -> &'static str {
    match scope {
        XeroSkillSourceScope::Global => "global",
        XeroSkillSourceScope::Project { .. } => "project",
    }
}

fn scope_project_id(scope: &XeroSkillSourceScope) -> Option<String> {
    match scope {
        XeroSkillSourceScope::Global => None,
        XeroSkillSourceScope::Project { project_id } => Some(project_id.clone()),
    }
}

fn skill_state_sql_value(state: XeroSkillSourceState) -> CommandResult<String> {
    serde_json_string_value(state, "skill source state")
}

fn trust_state_sql_value(trust: XeroSkillTrustState) -> CommandResult<String> {
    serde_json_string_value(trust, "skill trust state")
}

fn serde_json_string_value<T: Serialize>(value: T, label: &'static str) -> CommandResult<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| {
            CommandError::system_fault(
                "installed_skill_encode_failed",
                format!("Xero could not encode {label} as a stable string."),
            )
        })
}

fn map_installed_skill_read_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "installed_skill_read_failed",
        format!("Xero could not read installed skill records: {error}"),
    )
}

fn map_installed_skill_write_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "installed_skill_write_failed",
        format!("Xero could not persist installed skill records: {error}"),
    )
}
