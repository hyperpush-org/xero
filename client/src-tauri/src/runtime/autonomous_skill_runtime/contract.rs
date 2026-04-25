use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

use super::{
    inspection::{
        normalize_relative_source_path, normalize_skill_id, normalize_source_ref,
        normalize_source_repo, normalize_tree_hash,
    },
    source::AutonomousSkillSourceMetadata,
};

pub const CADENCE_SKILL_SOURCE_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CadenceSkillSourceKind {
    Bundled,
    Local,
    Project,
    Github,
    Dynamic,
    Mcp,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CadenceSkillSourceScope {
    Global,
    Project { project_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CadenceSkillSourceLocator {
    Bundled {
        bundle_id: String,
        skill_id: String,
        version: String,
    },
    Local {
        root_id: String,
        root_path: String,
        relative_path: String,
        skill_id: String,
    },
    Project {
        relative_path: String,
        skill_id: String,
    },
    Github {
        repo: String,
        reference: String,
        path: String,
        tree_hash: String,
    },
    Dynamic {
        run_id: String,
        artifact_id: String,
        skill_id: String,
    },
    Mcp {
        server_id: String,
        capability_id: String,
        skill_id: String,
    },
    Plugin {
        plugin_id: String,
        contribution_id: String,
        skill_path: String,
        skill_id: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CadenceSkillSourceState {
    Discoverable,
    Installed,
    Enabled,
    Disabled,
    Stale,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CadenceSkillTrustState {
    Trusted,
    UserApproved,
    ApprovalRequired,
    Untrusted,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CadenceSkillSourceRecord {
    pub contract_version: u32,
    pub source_id: String,
    pub scope: CadenceSkillSourceScope,
    pub locator: CadenceSkillSourceLocator,
    pub state: CadenceSkillSourceState,
    pub trust: CadenceSkillTrustState,
}

impl CadenceSkillSourceScope {
    pub fn global() -> Self {
        Self::Global
    }

    pub fn project(project_id: impl Into<String>) -> CommandResult<Self> {
        let project_id = normalize_required(project_id.into(), "projectId")?;
        Ok(Self::Project { project_id })
    }

    fn normalize(self) -> CommandResult<Self> {
        match self {
            Self::Global => Ok(Self::Global),
            Self::Project { project_id } => Ok(Self::Project {
                project_id: normalize_required(project_id, "projectId")?,
            }),
        }
    }

    fn id_segment(&self) -> String {
        match self {
            Self::Global => "global".into(),
            Self::Project { project_id } => format!("project:{project_id}"),
        }
    }

    fn is_project(&self) -> bool {
        matches!(self, Self::Project { .. })
    }
}

impl CadenceSkillSourceLocator {
    pub fn kind(&self) -> CadenceSkillSourceKind {
        match self {
            Self::Bundled { .. } => CadenceSkillSourceKind::Bundled,
            Self::Local { .. } => CadenceSkillSourceKind::Local,
            Self::Project { .. } => CadenceSkillSourceKind::Project,
            Self::Github { .. } => CadenceSkillSourceKind::Github,
            Self::Dynamic { .. } => CadenceSkillSourceKind::Dynamic,
            Self::Mcp { .. } => CadenceSkillSourceKind::Mcp,
            Self::Plugin { .. } => CadenceSkillSourceKind::Plugin,
        }
    }

    pub fn from_autonomous_github_source(source: &AutonomousSkillSourceMetadata) -> Self {
        Self::Github {
            repo: source.repo.clone(),
            reference: source.reference.clone(),
            path: source.path.clone(),
            tree_hash: source.tree_hash.clone(),
        }
    }

    pub fn to_autonomous_github_source(&self) -> Option<AutonomousSkillSourceMetadata> {
        match self {
            Self::Github {
                repo,
                reference,
                path,
                tree_hash,
            } => Some(AutonomousSkillSourceMetadata {
                repo: repo.clone(),
                path: path.clone(),
                reference: reference.clone(),
                tree_hash: tree_hash.clone(),
            }),
            _ => None,
        }
    }

    fn normalize(self) -> CommandResult<Self> {
        match self {
            Self::Bundled {
                bundle_id,
                skill_id,
                version,
            } => Ok(Self::Bundled {
                bundle_id: normalize_required(bundle_id, "bundleId")?,
                skill_id: normalize_skill_id(&skill_id)?,
                version: normalize_required(version, "version")?,
            }),
            Self::Local {
                root_id,
                root_path,
                relative_path,
                skill_id,
            } => Ok(Self::Local {
                root_id: normalize_required(root_id, "rootId")?,
                root_path: normalize_required(root_path, "rootPath")?,
                relative_path: normalize_relative_source_path(&relative_path)?,
                skill_id: normalize_skill_id(&skill_id)?,
            }),
            Self::Project {
                relative_path,
                skill_id,
            } => Ok(Self::Project {
                relative_path: normalize_relative_source_path(&relative_path)?,
                skill_id: normalize_skill_id(&skill_id)?,
            }),
            Self::Github {
                repo,
                reference,
                path,
                tree_hash,
            } => Ok(Self::Github {
                repo: normalize_source_repo(&repo)?,
                reference: normalize_source_ref(&reference)?,
                path: normalize_relative_source_path(&path)?,
                tree_hash: normalize_tree_hash(&tree_hash)?,
            }),
            Self::Dynamic {
                run_id,
                artifact_id,
                skill_id,
            } => Ok(Self::Dynamic {
                run_id: normalize_required(run_id, "runId")?,
                artifact_id: normalize_required(artifact_id, "artifactId")?,
                skill_id: normalize_skill_id(&skill_id)?,
            }),
            Self::Mcp {
                server_id,
                capability_id,
                skill_id,
            } => Ok(Self::Mcp {
                server_id: normalize_required(server_id, "serverId")?,
                capability_id: normalize_required(capability_id, "capabilityId")?,
                skill_id: normalize_skill_id(&skill_id)?,
            }),
            Self::Plugin {
                plugin_id,
                contribution_id,
                skill_path,
                skill_id,
            } => Ok(Self::Plugin {
                plugin_id: normalize_required(plugin_id, "pluginId")?,
                contribution_id: normalize_required(contribution_id, "contributionId")?,
                skill_path: normalize_relative_source_path(&skill_path)?,
                skill_id: normalize_skill_id(&skill_id)?,
            }),
        }
    }

    fn identity_segment(&self) -> String {
        match self {
            Self::Bundled {
                bundle_id,
                skill_id,
                ..
            } => format!("bundled:{bundle_id}:{skill_id}"),
            Self::Local {
                root_id,
                relative_path,
                skill_id,
                ..
            } => format!("local:{root_id}:{relative_path}:{skill_id}"),
            Self::Project {
                relative_path,
                skill_id,
            } => format!("project:{relative_path}:{skill_id}"),
            Self::Github {
                repo,
                reference,
                path,
                ..
            } => format!("github:{repo}:{reference}:{path}"),
            Self::Dynamic {
                run_id,
                artifact_id,
                skill_id,
            } => format!("dynamic:{run_id}:{artifact_id}:{skill_id}"),
            Self::Mcp {
                server_id,
                capability_id,
                skill_id,
            } => format!("mcp:{server_id}:{capability_id}:{skill_id}"),
            Self::Plugin {
                plugin_id,
                contribution_id,
                skill_path,
                skill_id,
            } => format!("plugin:{plugin_id}:{contribution_id}:{skill_path}:{skill_id}"),
        }
    }

    fn revision_fingerprint(&self) -> Option<&str> {
        match self {
            Self::Bundled { version, .. } => Some(version.as_str()),
            Self::Github { tree_hash, .. } => Some(tree_hash.as_str()),
            _ => None,
        }
    }
}

impl CadenceSkillSourceState {
    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }

        matches!(
            (self, next),
            (
                Self::Discoverable,
                Self::Installed | Self::Disabled | Self::Stale | Self::Failed | Self::Blocked
            ) | (
                Self::Installed,
                Self::Enabled | Self::Disabled | Self::Stale | Self::Failed | Self::Blocked
            ) | (
                Self::Enabled,
                Self::Disabled | Self::Stale | Self::Failed | Self::Blocked
            ) | (
                Self::Disabled,
                Self::Installed | Self::Enabled | Self::Stale | Self::Failed | Self::Blocked
            ) | (
                Self::Stale,
                Self::Discoverable
                    | Self::Installed
                    | Self::Disabled
                    | Self::Failed
                    | Self::Blocked
            ) | (
                Self::Failed,
                Self::Discoverable | Self::Installed | Self::Disabled | Self::Blocked
            ) | (Self::Blocked, Self::Disabled | Self::Failed)
        )
    }

    fn merge(self, other: Self) -> Self {
        if self == Self::Blocked || other == Self::Blocked {
            return Self::Blocked;
        }
        if self == Self::Failed || other == Self::Failed {
            return Self::Failed;
        }
        if self == Self::Stale || other == Self::Stale {
            return Self::Stale;
        }
        if self == Self::Disabled || other == Self::Disabled {
            return Self::Disabled;
        }
        if self == Self::Enabled || other == Self::Enabled {
            return Self::Enabled;
        }
        if self == Self::Installed || other == Self::Installed {
            return Self::Installed;
        }
        Self::Discoverable
    }
}

impl CadenceSkillTrustState {
    fn merge(self, other: Self) -> Self {
        if self == Self::Blocked || other == Self::Blocked {
            return Self::Blocked;
        }
        if self == Self::Untrusted || other == Self::Untrusted {
            return Self::Untrusted;
        }
        if self == Self::ApprovalRequired || other == Self::ApprovalRequired {
            return Self::ApprovalRequired;
        }
        if self == Self::UserApproved || other == Self::UserApproved {
            return Self::UserApproved;
        }
        Self::Trusted
    }
}

impl CadenceSkillSourceRecord {
    pub fn new(
        scope: CadenceSkillSourceScope,
        locator: CadenceSkillSourceLocator,
        state: CadenceSkillSourceState,
        trust: CadenceSkillTrustState,
    ) -> CommandResult<Self> {
        let scope = scope.normalize()?;
        let locator = locator.normalize()?;
        validate_scope_for_locator(&scope, &locator)?;
        let source_id = source_id_for(&scope, &locator);
        Ok(Self {
            contract_version: CADENCE_SKILL_SOURCE_CONTRACT_VERSION,
            source_id,
            scope,
            locator,
            state,
            trust,
        })
    }

    pub fn github_autonomous(
        scope: CadenceSkillSourceScope,
        source: &AutonomousSkillSourceMetadata,
        state: CadenceSkillSourceState,
        trust: CadenceSkillTrustState,
    ) -> CommandResult<Self> {
        Self::new(
            scope,
            CadenceSkillSourceLocator::from_autonomous_github_source(source),
            state,
            trust,
        )
    }

    pub fn validate(self) -> CommandResult<Self> {
        if self.contract_version != CADENCE_SKILL_SOURCE_CONTRACT_VERSION {
            return Err(CommandError::user_fixable(
                "skill_source_contract_version_unsupported",
                format!(
                    "Cadence rejected skill source contract version `{}` because only version `{CADENCE_SKILL_SOURCE_CONTRACT_VERSION}` is supported.",
                    self.contract_version
                ),
            ));
        }

        let normalized = Self::new(self.scope, self.locator, self.state, self.trust)?;
        if self.source_id.trim() != normalized.source_id {
            return Err(CommandError::user_fixable(
                "skill_source_id_invalid",
                format!(
                    "Cadence rejected skill source id `{}` because the canonical id is `{}`.",
                    self.source_id, normalized.source_id
                ),
            ));
        }
        Ok(normalized)
    }

    fn has_revision_mismatch(&self, other: &Self) -> bool {
        match (
            self.locator.revision_fingerprint(),
            other.locator.revision_fingerprint(),
        ) {
            (Some(left), Some(right)) => left != right,
            _ => false,
        }
    }

    fn merge_duplicate(&mut self, other: Self) {
        let revision_mismatch = self.has_revision_mismatch(&other);
        self.state = self.state.merge(other.state);
        self.trust = self.trust.merge(other.trust);

        if other.locator.revision_fingerprint() > self.locator.revision_fingerprint() {
            self.locator = other.locator;
        }

        if revision_mismatch
            && !matches!(
                self.state,
                CadenceSkillSourceState::Blocked | CadenceSkillSourceState::Failed
            )
        {
            self.state = CadenceSkillSourceState::Stale;
        }
    }
}

pub fn validate_skill_source_state_transition(
    from: CadenceSkillSourceState,
    to: CadenceSkillSourceState,
) -> CommandResult<()> {
    if from.can_transition_to(to) {
        return Ok(());
    }

    Err(CommandError::user_fixable(
        "skill_source_state_transition_unsupported",
        format!(
            "Cadence cannot transition a skill source directly from `{:?}` to `{:?}`.",
            from, to
        ),
    ))
}

pub fn merge_skill_source_records(
    records: impl IntoIterator<Item = CadenceSkillSourceRecord>,
) -> CommandResult<Vec<CadenceSkillSourceRecord>> {
    let mut by_id = BTreeMap::new();

    for record in records {
        let record = record.validate()?;
        by_id
            .entry(record.source_id.clone())
            .and_modify(|existing: &mut CadenceSkillSourceRecord| {
                existing.merge_duplicate(record.clone());
            })
            .or_insert(record);
    }

    Ok(by_id.into_values().collect())
}

fn validate_scope_for_locator(
    scope: &CadenceSkillSourceScope,
    locator: &CadenceSkillSourceLocator,
) -> CommandResult<()> {
    match locator.kind() {
        CadenceSkillSourceKind::Bundled if scope.is_project() => Err(CommandError::user_fixable(
            "skill_source_scope_invalid",
            "Cadence bundled skill sources are global because they ship with the application.",
        )),
        CadenceSkillSourceKind::Project | CadenceSkillSourceKind::Dynamic
            if !scope.is_project() =>
        {
            Err(CommandError::user_fixable(
                "skill_source_scope_invalid",
                "Cadence project and dynamic skill sources must be project-scoped.",
            ))
        }
        _ => Ok(()),
    }
}

fn source_id_for(scope: &CadenceSkillSourceScope, locator: &CadenceSkillSourceLocator) -> String {
    format!(
        "skill-source:v{}:{}:{}",
        CADENCE_SKILL_SOURCE_CONTRACT_VERSION,
        scope.id_segment(),
        locator.identity_segment()
    )
}

fn normalize_required(value: String, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(trimmed.to_owned())
}
