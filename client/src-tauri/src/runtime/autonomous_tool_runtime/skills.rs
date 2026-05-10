use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};

use super::{
    AutonomousBundledSkillRoot, AutonomousLocalSkillRoot, AutonomousPluginRoot,
    AutonomousSkillToolCandidate, AutonomousSkillToolOutput, AutonomousSkillToolStatus,
    AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime, AUTONOMOUS_TOOL_SKILL,
};
use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::project_store::{
        self, InstalledSkillDiagnosticRecord, InstalledSkillRecord, InstalledSkillScopeFilter,
    },
    mcp::{load_mcp_registry_from_path, McpConnectionStatus, McpServerRecord},
    runtime::autonomous_skill_runtime::{
        compute_skill_directory_version_hash, decide_skill_tool_access,
        discover_bundled_skill_directory, discover_local_skill_directory, discover_plugin_roots,
        discover_plugin_skill_contribution, discover_project_skill_directory,
        load_discovered_skill_context, load_skill_context_from_directory,
        sanitize_skill_tool_model_text, skill_tool_diagnostic_from_command_error,
        validate_attached_skill_resolution_request, validate_skill_tool_context_payload,
        AutonomousSkillInstallRequest, AutonomousSkillInvokeRequest, AutonomousSkillResolveOutput,
        AutonomousSkillResolveRequest, AutonomousSkillSourceMetadata, XeroAttachedSkillDiagnostic,
        XeroAttachedSkillRef, XeroAttachedSkillRepairHint, XeroAttachedSkillResolutionReport,
        XeroAttachedSkillResolutionRequest, XeroAttachedSkillResolutionSnapshot,
        XeroAttachedSkillResolutionStatus, XeroAttachedSkillScope, XeroDiscoveredSkill,
        XeroPluginRoot, XeroResolvedAttachedSkill, XeroSkillDirectoryDiscovery,
        XeroSkillSourceLocator, XeroSkillSourceRecord, XeroSkillSourceScope, XeroSkillSourceState,
        XeroSkillToolAccessStatus, XeroSkillToolContextAsset, XeroSkillToolContextDocument,
        XeroSkillToolContextPayload, XeroSkillToolDiagnostic, XeroSkillToolDynamicAssetInput,
        XeroSkillToolInput, XeroSkillToolLifecycleEvent, XeroSkillToolLifecycleResult,
        XeroSkillToolOperation, XeroSkillTrustState,
        XERO_ATTACHED_SKILL_RESOLUTION_CONTRACT_VERSION, XERO_SKILL_TOOL_CONTRACT_VERSION,
    },
};

#[derive(Debug, Clone)]
pub(super) enum CachedSkillToolCandidate {
    Installed(InstalledSkillRecord),
    Discovered(XeroDiscoveredSkill),
    Github(ResolvedGithubSkillCandidate),
    Mcp(ResolvedMcpSkillCandidate),
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedGithubSkillCandidate {
    pub source: XeroSkillSourceRecord,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub github_source: AutonomousSkillSourceMetadata,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedMcpSkillCandidate {
    pub source: XeroSkillSourceRecord,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub version_hash: String,
    pub server_id: String,
    pub capability: McpSkillCapability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum McpSkillCapability {
    Resource { uri: String },
    Prompt { name: String },
}

#[derive(Debug, Clone)]
struct SkillToolSelection {
    entry: CachedSkillToolCandidate,
    public: AutonomousSkillToolCandidate,
}

impl AutonomousToolRuntime {
    pub fn skill(&self, input: XeroSkillToolInput) -> CommandResult<AutonomousToolResult> {
        let input = crate::runtime::autonomous_skill_runtime::validate_skill_tool_input(input)?;
        let operation = input.operation();
        let output = match input {
            XeroSkillToolInput::List {
                query,
                include_unavailable,
                limit,
            } => self.skill_list(operation, query, include_unavailable, limit)?,
            XeroSkillToolInput::Resolve {
                source_id,
                skill_id,
                include_unavailable,
            } => self.skill_resolve(operation, source_id, skill_id, include_unavailable)?,
            XeroSkillToolInput::Install {
                source_id,
                approval_grant_id,
            } => self.skill_install(operation, &source_id, approval_grant_id.as_deref())?,
            XeroSkillToolInput::Invoke {
                source_id,
                approval_grant_id,
                include_supporting_assets,
            } => self.skill_invoke(
                operation,
                &source_id,
                approval_grant_id.as_deref(),
                include_supporting_assets,
            )?,
            XeroSkillToolInput::Reload {
                source_id,
                source_kind,
            } => self.skill_reload(operation, source_id, source_kind)?,
            XeroSkillToolInput::CreateDynamic {
                skill_id,
                markdown,
                supporting_assets,
                source_run_id,
                source_artifact_id,
            } => self.skill_create_dynamic(
                operation,
                &skill_id,
                &markdown,
                supporting_assets,
                source_run_id,
                source_artifact_id,
            )?,
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_SKILL.into(),
            summary: output.message.clone(),
            command_result: None,
            output: AutonomousToolOutput::Skill(output),
        })
    }

    fn skill_list(
        &self,
        operation: XeroSkillToolOperation,
        query: Option<String>,
        include_unavailable: bool,
        limit: Option<usize>,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(skill_unavailable_output(operation));
        };
        let limit = limit.unwrap_or(crate::runtime::XERO_SKILL_TOOL_DEFAULT_LIMIT);
        let discovered = self.collect_skill_tool_candidates(
            skill_tool,
            query.as_deref(),
            include_unavailable,
            limit,
            operation,
        )?;
        self.cache_skill_candidates(&discovered.entries)?;
        let candidates = discovered
            .entries
            .into_iter()
            .map(|entry| public_skill_candidate(&entry, operation))
            .collect::<CommandResult<Vec<_>>>()?;
        let message = if candidates.is_empty() {
            "SkillTool found no configured skills for this request.".into()
        } else {
            format!(
                "SkillTool returned {} skill candidate(s).",
                candidates.len()
            )
        };

        Ok(AutonomousSkillToolOutput {
            operation,
            status: if candidates.is_empty() {
                AutonomousSkillToolStatus::Unavailable
            } else {
                AutonomousSkillToolStatus::Succeeded
            },
            message,
            candidates,
            selected: None,
            context: None,
            lifecycle_events: Vec::new(),
            diagnostics: discovered.diagnostics,
            truncated: discovered.truncated,
        })
    }

    fn skill_resolve(
        &self,
        operation: XeroSkillToolOperation,
        source_id: Option<String>,
        skill_id: Option<String>,
        include_unavailable: bool,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(skill_unavailable_output(operation));
        };
        let selection = if let Some(source_id) = source_id {
            self.find_skill_by_source_id(skill_tool, &source_id, include_unavailable, operation)?
        } else if let Some(skill_id) = skill_id {
            self.find_skill_by_skill_id(skill_tool, &skill_id, include_unavailable, operation)?
        } else {
            return Err(CommandError::invalid_request("sourceId or skillId"));
        };

        match selection {
            Some(selection) => {
                self.cache_skill_candidates(std::slice::from_ref(&selection.entry))?;
                let lifecycle = XeroSkillToolLifecycleEvent::succeeded(
                    operation,
                    Some(selection.public.source_id.clone()),
                    Some(selection.public.skill_id.clone()),
                    format!("Resolved skill `{}` for model use.", selection.public.skill_id),
                )?;
                Ok(AutonomousSkillToolOutput {
                    operation,
                    status: AutonomousSkillToolStatus::Succeeded,
                    message: format!("Resolved skill `{}`.", selection.public.skill_id),
                    candidates: vec![selection.public.clone()],
                    selected: Some(selection.public),
                    context: None,
                    lifecycle_events: vec![lifecycle],
                    diagnostics: Vec::new(),
                    truncated: false,
                })
            }
            None => Ok(AutonomousSkillToolOutput {
                operation,
                status: AutonomousSkillToolStatus::Unavailable,
                message: "SkillTool could not resolve a matching skill candidate.".into(),
                candidates: Vec::new(),
                selected: None,
                context: None,
                lifecycle_events: Vec::new(),
                diagnostics: vec![diagnostic(
                    "skill_tool_skill_not_found",
                    "Xero could not find that skill in the durable registry or configured skill sources.",
                    false,
                )],
                truncated: false,
            }),
        }
    }

    fn skill_install(
        &self,
        operation: XeroSkillToolOperation,
        source_id: &str,
        approval_grant_id: Option<&str>,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(skill_unavailable_output(operation));
        };
        let Some(selection) =
            self.find_skill_by_source_id(skill_tool, source_id, true, operation)?
        else {
            return Ok(skill_not_found_output(operation, source_id));
        };
        let access = access_for_operation(&selection.entry, operation, approval_grant_id)?;
        if access.status == XeroSkillToolAccessStatus::ApprovalRequired {
            return Ok(approval_required_output(
                operation,
                selection.public,
                access.reason,
            ));
        }
        if access.status == XeroSkillToolAccessStatus::Denied {
            return Ok(denied_output(operation, selection.public, access.reason));
        }

        let result = match selection.entry.clone() {
            CachedSkillToolCandidate::Github(candidate) => {
                self.install_github_skill(skill_tool, &candidate.github_source)
            }
            CachedSkillToolCandidate::Mcp(candidate) => self.install_mcp_skill(&candidate),
            CachedSkillToolCandidate::Discovered(candidate) => {
                self.install_discovered_skill(&candidate, approval_grant_id)
            }
            CachedSkillToolCandidate::Installed(record) => {
                self.install_existing_skill(skill_tool, record, approval_grant_id)
            }
        };

        self.finish_install_result(operation, selection.public, result)
    }

    fn skill_invoke(
        &self,
        operation: XeroSkillToolOperation,
        source_id: &str,
        approval_grant_id: Option<&str>,
        include_supporting_assets: bool,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(skill_unavailable_output(operation));
        };
        let Some(selection) =
            self.find_skill_by_source_id(skill_tool, source_id, true, operation)?
        else {
            return Ok(skill_not_found_output(operation, source_id));
        };
        let access = access_for_operation(&selection.entry, operation, approval_grant_id)?;
        if access.status == XeroSkillToolAccessStatus::ApprovalRequired {
            return Ok(approval_required_output(
                operation,
                selection.public,
                access.reason,
            ));
        }
        if access.status == XeroSkillToolAccessStatus::Denied {
            return Ok(denied_output(operation, selection.public, access.reason));
        }

        let result = match selection.entry.clone() {
            CachedSkillToolCandidate::Github(candidate) => self.invoke_github_skill(
                skill_tool,
                &candidate.github_source,
                include_supporting_assets,
            ),
            CachedSkillToolCandidate::Mcp(candidate) => {
                self.invoke_mcp_skill(&candidate, include_supporting_assets)
            }
            CachedSkillToolCandidate::Discovered(candidate) => self.invoke_discovered_skill(
                &candidate,
                approval_grant_id,
                include_supporting_assets,
            ),
            CachedSkillToolCandidate::Installed(record) => self.invoke_installed_skill(
                skill_tool,
                record,
                approval_grant_id,
                include_supporting_assets,
            ),
        };

        self.finish_invoke_result(operation, selection.public, result)
    }

    fn skill_reload(
        &self,
        operation: XeroSkillToolOperation,
        source_id: Option<String>,
        source_kind: Option<crate::runtime::XeroSkillSourceKind>,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(skill_unavailable_output(operation));
        };
        let discovered =
            self.collect_skill_tool_candidates(skill_tool, None, true, usize::MAX / 2, operation)?;
        self.cache_skill_candidates(&discovered.entries)?;
        let mut candidates = discovered
            .entries
            .into_iter()
            .map(|entry| public_skill_candidate(&entry, operation))
            .collect::<CommandResult<Vec<_>>>()?;
        if let Some(source_id) = source_id {
            candidates.retain(|candidate| candidate.source_id == source_id);
        }
        if let Some(source_kind) = source_kind {
            candidates.retain(|candidate| candidate.source_kind == source_kind);
        }
        let lifecycle = XeroSkillToolLifecycleEvent::succeeded(
            operation,
            None,
            None,
            format!("Reloaded {} SkillTool candidate(s).", candidates.len()),
        )?;
        Ok(AutonomousSkillToolOutput {
            operation,
            status: AutonomousSkillToolStatus::Succeeded,
            message: format!(
                "SkillTool reload returned {} candidate(s).",
                candidates.len()
            ),
            candidates,
            selected: None,
            context: None,
            lifecycle_events: vec![lifecycle],
            diagnostics: discovered.diagnostics,
            truncated: false,
        })
    }

    fn skill_create_dynamic(
        &self,
        operation: XeroSkillToolOperation,
        skill_id: &str,
        markdown: &str,
        supporting_assets: Vec<XeroSkillToolDynamicAssetInput>,
        source_run_id: Option<String>,
        source_artifact_id: Option<String>,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(skill_unavailable_output(operation));
        };
        let run_id = source_run_id.unwrap_or_else(|| "model-created".into());
        let artifact_id = source_artifact_id
            .unwrap_or_else(|| format!("dynamic-{}", &sha256_hex(markdown.as_bytes())[..12]));
        let source = XeroSkillSourceRecord::new(
            XeroSkillSourceScope::project(skill_tool.project_id.clone())?,
            XeroSkillSourceLocator::Dynamic {
                run_id: run_id.clone(),
                artifact_id: artifact_id.clone(),
                skill_id: skill_id.to_owned(),
            },
            XeroSkillSourceState::Disabled,
            XeroSkillTrustState::Untrusted,
        )?;
        let directory_relative = PathBuf::from("dynamic-skills")
            .join(sanitize_path_segment(&run_id))
            .join(sanitize_path_segment(&artifact_id))
            .join(skill_id);
        let directory = skill_tool.project_app_data_dir.join(&directory_relative);
        fs::create_dir_all(&directory).map_err(|error| {
            CommandError::retryable(
                "skill_tool_dynamic_write_failed",
                format!(
                    "Xero could not create dynamic skill directory {}: {error}",
                    directory.display()
                ),
            )
        })?;
        self.write_dynamic_skill_files(&directory, markdown, &supporting_assets)?;
        let context =
            load_skill_context_from_directory(&source.source_id, skill_id, &directory, None, true)?;
        let metadata = frontmatter_metadata(markdown)?;
        let timestamp = now_timestamp();
        let record = InstalledSkillRecord {
            source: source.clone(),
            skill_id: skill_id.to_owned(),
            name: metadata.name,
            description: metadata.description,
            user_invocable: metadata.user_invocable,
            cache_key: None,
            local_location: Some(directory.display().to_string()),
            version_hash: Some(context_hash(&context)),
            installed_at: timestamp.clone(),
            updated_at: timestamp,
            last_used_at: None,
            last_diagnostic: None,
        };
        let record = project_store::upsert_installed_skill(&self.repo_root, record)?;
        let entry = CachedSkillToolCandidate::Installed(record);
        self.cache_skill_candidates(std::slice::from_ref(&entry))?;
        let public = public_skill_candidate(&entry, operation)?;
        let lifecycle = XeroSkillToolLifecycleEvent::succeeded(
            operation,
            Some(public.source_id.clone()),
            Some(public.skill_id.clone()),
            "Created an untrusted, disabled dynamic skill candidate for later user review.",
        )?;
        Ok(AutonomousSkillToolOutput {
            operation,
            status: AutonomousSkillToolStatus::Succeeded,
            message: format!(
                "Created dynamic skill candidate `{skill_id}` disabled and untrusted."
            ),
            candidates: vec![public.clone()],
            selected: Some(public),
            context: None,
            lifecycle_events: vec![lifecycle],
            diagnostics: Vec::new(),
            truncated: false,
        })
    }

    pub fn resolve_attached_skills(
        &self,
        request: XeroAttachedSkillResolutionRequest,
    ) -> CommandResult<XeroAttachedSkillResolutionReport> {
        let request = validate_attached_skill_resolution_request(request)?;
        let resolved_at = now_timestamp();

        if request.attached_skills.is_empty() {
            let snapshot = XeroAttachedSkillResolutionSnapshot {
                contract_version: XERO_ATTACHED_SKILL_RESOLUTION_CONTRACT_VERSION,
                project_id: request.project_id,
                run_id: request.run_id,
                resolved_at,
                attached_skills: Vec::new(),
            };
            let snapshot =
                project_store::persist_runtime_attached_skill_snapshot(&self.repo_root, &snapshot)?;
            return Ok(attached_skill_resolution_success(snapshot));
        }

        let Some(skill_tool) = self.skill_tool.as_ref() else {
            let diagnostics = request
                .attached_skills
                .iter()
                .map(|attachment| {
                    XeroAttachedSkillDiagnostic::user_fixable(
                        "attached_skill_registry_unavailable",
                        "Xero cannot resolve attached skills because the skill registry is unavailable for this run.",
                        attachment,
                        XeroAttachedSkillRepairHint::Retry,
                    )
                })
                .collect::<CommandResult<Vec<_>>>()?;
            return Ok(attached_skill_resolution_failure(diagnostics));
        };

        let discovered = self.collect_skill_tool_candidates(
            skill_tool,
            None,
            true,
            usize::MAX / 2,
            XeroSkillToolOperation::Invoke,
        )?;
        self.cache_skill_candidates(&discovered.entries)?;
        let entries = discovered
            .entries
            .into_iter()
            .map(|entry| (candidate_source_id(&entry), entry))
            .collect::<BTreeMap<_, _>>();

        let mut diagnostics = Vec::new();
        let mut resolved = Vec::with_capacity(request.attached_skills.len());
        for attachment in &request.attached_skills {
            let Some(entry) = entries.get(&attachment.source_id) else {
                diagnostics.push(XeroAttachedSkillDiagnostic::user_fixable(
                    "attached_skill_source_missing",
                    format!(
                        "Xero could not find attached skill source `{}` for configured skill `{}`.",
                        attachment.source_id, attachment.skill_id
                    ),
                    attachment,
                    XeroAttachedSkillRepairHint::RemoveAttachment,
                )?);
                continue;
            };

            match self.resolve_attached_skill_entry(skill_tool, attachment, entry) {
                Ok(skill) => resolved.push(skill),
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }

        if !diagnostics.is_empty() {
            return Ok(attached_skill_resolution_failure(diagnostics));
        }

        let snapshot = XeroAttachedSkillResolutionSnapshot {
            contract_version: XERO_ATTACHED_SKILL_RESOLUTION_CONTRACT_VERSION,
            project_id: request.project_id,
            run_id: request.run_id,
            resolved_at,
            attached_skills: resolved,
        };
        let snapshot =
            project_store::persist_runtime_attached_skill_snapshot(&self.repo_root, &snapshot)?;
        Ok(attached_skill_resolution_success(snapshot))
    }

    fn resolve_attached_skill_entry(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        attachment: &XeroAttachedSkillRef,
        entry: &CachedSkillToolCandidate,
    ) -> Result<XeroResolvedAttachedSkill, XeroAttachedSkillDiagnostic> {
        let record = source_record_for_entry(entry).map_err(|error| {
            attached_skill_error_diagnostic(
                attachment,
                &error,
                Some(XeroAttachedSkillRepairHint::RemoveAttachment),
            )
        })?;
        let source_kind = record.locator.kind();
        let scope = XeroAttachedSkillScope::from_source_scope(&record.scope);
        if source_kind != attachment.source_kind {
            return Err(
                XeroAttachedSkillDiagnostic::user_fixable(
                    "attached_skill_source_kind_mismatch",
                    format!(
                        "Attached skill `{}` expects source kind `{:?}`, but registry source `{}` is `{:?}`.",
                        attachment.id, attachment.source_kind, attachment.source_id, source_kind
                    ),
                    attachment,
                    XeroAttachedSkillRepairHint::RefreshPin,
                )
                .expect("static attached skill diagnostic should validate"),
            );
        }
        if scope != attachment.scope {
            return Err(XeroAttachedSkillDiagnostic::user_fixable(
                "attached_skill_scope_mismatch",
                format!(
                    "Attached skill `{}` expects scope `{:?}`, but registry source `{}` is `{:?}`.",
                    attachment.id, attachment.scope, attachment.source_id, scope
                ),
                attachment,
                XeroAttachedSkillRepairHint::RefreshPin,
            )
            .expect("static attached skill diagnostic should validate"));
        }

        if candidate_skill_id(entry) != attachment.skill_id {
            return Err(XeroAttachedSkillDiagnostic::user_fixable(
                "attached_skill_skill_id_mismatch",
                format!(
                    "Attached skill `{}` expects skill id `{}`, but source `{}` resolves to `{}`.",
                    attachment.id,
                    attachment.skill_id,
                    attachment.source_id,
                    candidate_skill_id(entry)
                ),
                attachment,
                XeroAttachedSkillRepairHint::RefreshPin,
            )
            .expect("static attached skill diagnostic should validate"));
        }

        if let Some(diagnostic) = attached_skill_state_diagnostic(attachment, record.state) {
            return Err(diagnostic);
        }
        if let Some(diagnostic) = attached_skill_trust_diagnostic(attachment, record.trust) {
            return Err(diagnostic);
        }

        let current_version_hash = self
            .current_attached_skill_version_hash(skill_tool, entry)
            .map_err(|error| {
                attached_skill_error_diagnostic(
                    attachment,
                    &error,
                    Some(if error.retryable {
                        XeroAttachedSkillRepairHint::Retry
                    } else {
                        XeroAttachedSkillRepairHint::RefreshPin
                    }),
                )
            })?;
        if current_version_hash != attachment.version_hash {
            return Err(
                XeroAttachedSkillDiagnostic::user_fixable(
                    "attached_skill_version_hash_mismatch",
                    format!(
                        "Attached skill `{}` is pinned to `{}`, but source `{}` currently resolves to `{}`.",
                        attachment.id, attachment.version_hash, attachment.source_id, current_version_hash
                    ),
                    attachment,
                    XeroAttachedSkillRepairHint::RefreshPin,
                )
                .expect("static attached skill diagnostic should validate"),
            );
        }

        let context = self
            .attached_skill_context_for_entry(
                skill_tool,
                entry,
                attachment.include_supporting_assets,
            )
            .map_err(|error| {
                attached_skill_error_diagnostic(
                    attachment,
                    &error,
                    Some(if error.retryable {
                        XeroAttachedSkillRepairHint::Retry
                    } else {
                        XeroAttachedSkillRepairHint::RefreshPin
                    }),
                )
            })?;

        Ok(XeroResolvedAttachedSkill {
            id: attachment.id.clone(),
            source_id: record.source_id,
            skill_id: candidate_skill_id(entry),
            name: sanitize_candidate_text(&candidate_name(entry), &attachment.name),
            description: sanitize_candidate_text(&candidate_description(entry), ""),
            source_kind,
            scope,
            version_hash: current_version_hash,
            include_supporting_assets: attachment.include_supporting_assets,
            required: true,
            content_hash: context_hash(&context),
            context,
        })
    }

    fn current_attached_skill_version_hash(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        entry: &CachedSkillToolCandidate,
    ) -> CommandResult<String> {
        match entry {
            CachedSkillToolCandidate::Installed(record) => match &record.source.locator {
                XeroSkillSourceLocator::Github { .. } => {
                    let source = record
                        .source
                        .locator
                        .to_autonomous_github_source()
                        .ok_or_else(|| {
                            CommandError::user_fixable(
                            "attached_skill_source_unsupported",
                            "Xero could not map this installed GitHub skill back to its source.",
                        )
                        })?;
                    let latest =
                        skill_tool
                            .github_runtime
                            .resolve(AutonomousSkillResolveRequest {
                                skill_id: record.skill_id.clone(),
                                timeout_ms: None,
                                source_repo: Some(source.repo),
                                source_ref: Some(source.reference),
                            })?;
                    Ok(latest.source.tree_hash)
                }
                XeroSkillSourceLocator::Dynamic { .. } => {
                    let context = load_installed_filesystem_context(record, true)?;
                    Ok(context_hash(&context))
                }
                XeroSkillSourceLocator::Mcp { .. } => {
                    record.version_hash.clone().ok_or_else(|| {
                        CommandError::user_fixable(
                            "attached_skill_version_hash_missing",
                            "Xero requires attached MCP skill records to carry a version hash.",
                        )
                    })
                }
                _ => {
                    let local_location = record.local_location.as_deref().ok_or_else(|| {
                        CommandError::user_fixable(
                            "attached_skill_location_missing",
                            format!(
                                "Installed skill `{}` does not have a local location.",
                                record.skill_id
                            ),
                        )
                    })?;
                    compute_skill_directory_version_hash(Path::new(local_location))
                }
            },
            CachedSkillToolCandidate::Discovered(candidate) => Ok(candidate.version_hash.clone()),
            CachedSkillToolCandidate::Github(candidate) => {
                let latest = skill_tool
                    .github_runtime
                    .resolve(AutonomousSkillResolveRequest {
                        skill_id: candidate.skill_id.clone(),
                        timeout_ms: None,
                        source_repo: Some(candidate.github_source.repo.clone()),
                        source_ref: Some(candidate.github_source.reference.clone()),
                    })?;
                Ok(latest.source.tree_hash)
            }
            CachedSkillToolCandidate::Mcp(candidate) => Ok(candidate.version_hash.clone()),
        }
    }

    fn attached_skill_context_for_entry(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        entry: &CachedSkillToolCandidate,
        include_supporting_assets: bool,
    ) -> CommandResult<XeroSkillToolContextPayload> {
        match entry.clone() {
            CachedSkillToolCandidate::Installed(record) => match &record.source.locator {
                XeroSkillSourceLocator::Github { .. } => {
                    let source = record.source.locator.to_autonomous_github_source().ok_or_else(|| {
                        CommandError::user_fixable(
                            "attached_skill_source_unsupported",
                            "Xero could not map this installed GitHub skill back to its source.",
                        )
                    })?;
                    self.invoke_github_skill(skill_tool, &source, include_supporting_assets)
                }
                XeroSkillSourceLocator::Mcp { .. } => Err(CommandError::user_fixable(
                    "attached_skill_mcp_installed_state_unsupported",
                    "MCP-provided attached skills must resolve from the connected MCP server rather than installed filesystem state.",
                )),
                _ => load_installed_filesystem_context(&record, include_supporting_assets),
            },
            CachedSkillToolCandidate::Discovered(candidate) => {
                load_discovered_skill_context(&candidate, include_supporting_assets)
            }
            CachedSkillToolCandidate::Github(candidate) => self.invoke_github_skill(
                skill_tool,
                &candidate.github_source,
                include_supporting_assets,
            ),
            CachedSkillToolCandidate::Mcp(candidate) => {
                self.invoke_mcp_skill(&candidate, include_supporting_assets)
            }
        }
    }

    fn collect_skill_tool_candidates(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        query: Option<&str>,
        include_unavailable: bool,
        limit: usize,
        operation: XeroSkillToolOperation,
    ) -> CommandResult<SkillToolDiscoveryResult> {
        let mut entries = BTreeMap::<String, CachedSkillToolCandidate>::new();
        let mut diagnostics = Vec::new();

        for record in project_store::list_installed_skills(
            &self.repo_root,
            InstalledSkillScopeFilter::project(skill_tool.project_id.clone(), true)?,
        )? {
            insert_candidate(
                &mut entries,
                CachedSkillToolCandidate::Installed(
                    self.apply_plugin_state_to_installed_skill(record),
                ),
            );
        }

        self.collect_project_skills(skill_tool, &mut entries, &mut diagnostics)?;
        self.collect_local_skills(skill_tool, &mut entries, &mut diagnostics)?;
        self.collect_bundled_skills(skill_tool, &mut entries, &mut diagnostics)?;
        self.collect_plugin_skills(skill_tool, &mut entries, &mut diagnostics)?;
        self.collect_mcp_skills(
            skill_tool,
            include_unavailable,
            &mut entries,
            &mut diagnostics,
        )?;
        if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
            self.collect_github_skills(skill_tool, query, &mut entries, &mut diagnostics)?;
        }

        if operation == XeroSkillToolOperation::Reload {
            self.reconcile_installed_candidates_for_reload(skill_tool, &mut entries)?;
        }

        let mut entries = entries
            .into_values()
            .filter(|entry| {
                (include_unavailable || candidate_model_visible(entry))
                    && query
                        .map(|query| candidate_matches_query(entry, query))
                        .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            candidate_rank(left, query)
                .cmp(&candidate_rank(right, query))
                .then_with(|| candidate_source_id(left).cmp(&candidate_source_id(right)))
        });
        let truncated = entries.len() > limit;
        if limit < entries.len() {
            entries.truncate(limit);
        }

        diagnostics.extend(
            entries
                .iter()
                .filter_map(|entry| public_skill_candidate(entry, operation).ok())
                .filter_map(|candidate| candidate.access.reason)
                .filter(|reason| reason.code != "skill_tool_user_approval_required"),
        );

        Ok(SkillToolDiscoveryResult {
            entries,
            diagnostics,
            truncated,
        })
    }

    fn collect_project_skills(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
        diagnostics: &mut Vec<XeroSkillToolDiagnostic>,
    ) -> CommandResult<()> {
        if !skill_tool.project_skills_enabled {
            return Ok(());
        }
        let discovered = discover_project_skill_directory(
            &skill_tool.project_id,
            &skill_tool.project_app_data_dir,
        )?;
        push_discovery(entries, diagnostics, discovered);
        Ok(())
    }

    fn collect_local_skills(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
        diagnostics: &mut Vec<XeroSkillToolDiagnostic>,
    ) -> CommandResult<()> {
        for AutonomousLocalSkillRoot { root_id, root_path } in &skill_tool.local_roots {
            let discovered = discover_local_skill_directory(root_id, root_path)?;
            push_discovery(entries, diagnostics, discovered);
        }
        Ok(())
    }

    fn collect_bundled_skills(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
        diagnostics: &mut Vec<XeroSkillToolDiagnostic>,
    ) -> CommandResult<()> {
        for AutonomousBundledSkillRoot {
            bundle_id,
            version,
            root_path,
        } in &skill_tool.bundled_roots
        {
            let discovered = discover_bundled_skill_directory(bundle_id, version, root_path)?;
            push_discovery(entries, diagnostics, discovered);
        }
        Ok(())
    }

    fn collect_plugin_skills(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
        diagnostics: &mut Vec<XeroSkillToolDiagnostic>,
    ) -> CommandResult<()> {
        if skill_tool.plugin_roots.is_empty() {
            return Ok(());
        }

        let roots =
            skill_tool
                .plugin_roots
                .iter()
                .map(
                    |AutonomousPluginRoot { root_id, root_path }| XeroPluginRoot {
                        root_id: root_id.clone(),
                        root_path: root_path.clone(),
                    },
                );
        let discovery = discover_plugin_roots(roots)?;
        diagnostics.extend(
            discovery
                .diagnostics
                .into_iter()
                .map(|diagnostic| plugin_diagnostic(diagnostic.code, diagnostic.message)),
        );
        let plugin_records =
            project_store::sync_discovered_plugins(&self.repo_root, &discovery.plugins, false)?;
        for plugin in plugin_records {
            for skill in &plugin.manifest.skills {
                let state = if plugin.state == XeroSkillSourceState::Enabled {
                    XeroSkillSourceState::Discoverable
                } else {
                    plugin.state
                };
                let discovered = discover_plugin_skill_contribution(
                    skill_tool.project_id.clone(),
                    plugin.plugin_id.clone(),
                    skill.id.clone(),
                    Path::new(&plugin.plugin_root_path),
                    skill.path.clone(),
                    state,
                    plugin.trust,
                )?;
                for candidate in &discovered.candidates {
                    if candidate.skill_id != skill.id {
                        diagnostics.push(plugin_diagnostic(
                            "xero_plugin_skill_id_mismatch",
                            format!(
                                "Xero skipped plugin `{}` skill contribution `{}` because SKILL.md declared `{}`.",
                                plugin.plugin_id, skill.id, candidate.skill_id
                            ),
                        ));
                        continue;
                    }
                    insert_candidate(
                        entries,
                        CachedSkillToolCandidate::Discovered(candidate.clone()),
                    );
                }
                diagnostics.extend(
                    discovered
                        .diagnostics
                        .into_iter()
                        .map(|diagnostic| plugin_diagnostic(diagnostic.code, diagnostic.message)),
                );
            }
        }
        Ok(())
    }

    fn collect_github_skills(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        query: &str,
        entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
        diagnostics: &mut Vec<XeroSkillToolDiagnostic>,
    ) -> CommandResult<()> {
        if !skill_tool.github_enabled {
            return Ok(());
        }
        let discovered = match skill_tool.github_runtime.discover(
            crate::runtime::AutonomousSkillDiscoverRequest {
                query: query.to_owned(),
                result_limit: Some(10),
                timeout_ms: None,
                source_repo: None,
                source_ref: None,
            },
        ) {
            Ok(output) => output,
            Err(error) => {
                diagnostics.push(skill_tool_diagnostic_from_command_error(&error));
                return Ok(());
            }
        };
        for candidate in discovered.candidates {
            match skill_tool
                .github_runtime
                .resolve(AutonomousSkillResolveRequest {
                    skill_id: candidate.skill_id,
                    timeout_ms: None,
                    source_repo: Some(candidate.source.repo.clone()),
                    source_ref: Some(candidate.source.reference.clone()),
                }) {
                Ok(resolved) => {
                    insert_candidate(
                        entries,
                        CachedSkillToolCandidate::Github(github_candidate(skill_tool, resolved)?),
                    );
                }
                Err(error) => diagnostics.push(skill_tool_diagnostic_from_command_error(&error)),
            }
        }
        Ok(())
    }

    fn collect_mcp_skills(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        include_unavailable: bool,
        entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
        diagnostics: &mut Vec<XeroSkillToolDiagnostic>,
    ) -> CommandResult<()> {
        let Some(registry_path) = self.mcp_registry_path.as_ref() else {
            return Ok(());
        };
        let registry = match load_mcp_registry_from_path(registry_path) {
            Ok(registry) => registry,
            Err(error) => {
                diagnostics.push(skill_tool_diagnostic_from_command_error(&error));
                return Ok(());
            }
        };
        let timeout = super::priority_tools::normalize_mcp_timeout(None)?;

        for server in &registry.servers {
            if server.connection.status != McpConnectionStatus::Connected {
                if include_unavailable {
                    diagnostics.push(mcp_server_unavailable_diagnostic(server));
                }
                continue;
            }

            collect_mcp_resource_skill_candidates(
                skill_tool,
                server,
                timeout,
                entries,
                diagnostics,
            );
            collect_mcp_prompt_skill_candidates(skill_tool, server, timeout, entries, diagnostics);
        }

        Ok(())
    }

    fn cache_skill_candidates(&self, entries: &[CachedSkillToolCandidate]) -> CommandResult<()> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(());
        };
        let mut cache = skill_tool.discovery_cache.lock().map_err(|_| {
            CommandError::system_fault(
                "skill_tool_cache_lock_failed",
                "Xero could not lock the SkillTool discovery cache.",
            )
        })?;
        for entry in entries {
            cache.insert(candidate_source_id(entry), entry.clone());
        }
        Ok(())
    }

    fn reconcile_installed_candidates_for_reload(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
    ) -> CommandResult<()> {
        let source_ids = entries.keys().cloned().collect::<Vec<_>>();
        for source_id in source_ids {
            let Some(CachedSkillToolCandidate::Installed(record)) =
                entries.get(&source_id).cloned()
            else {
                continue;
            };
            let reconciled = self.reconcile_installed_record_for_reload(skill_tool, record.clone());
            let reconciled = match reconciled {
                Ok(record) => record,
                Err(error) => mark_installed_record_stale(
                    record,
                    "skill_source_reload_failed",
                    format!("Xero could not reload this skill source: {}", error.message),
                    error.retryable,
                ),
            };
            if project_store::load_installed_skill_by_source_id(&self.repo_root, &source_id)?
                .as_ref()
                .is_some_and(|current| current == &reconciled)
            {
                continue;
            }
            let persisted = project_store::upsert_installed_skill(&self.repo_root, reconciled)?;
            entries.insert(source_id, CachedSkillToolCandidate::Installed(persisted));
        }
        Ok(())
    }

    fn reconcile_installed_record_for_reload(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        record: InstalledSkillRecord,
    ) -> CommandResult<InstalledSkillRecord> {
        if matches!(
            record.source.state,
            XeroSkillSourceState::Blocked | XeroSkillSourceState::Disabled
        ) {
            return Ok(record);
        }

        if let Some((code, message)) = reload_unavailable_reason(skill_tool, &record) {
            return Ok(mark_installed_record_stale(record, code, message, false));
        }

        if record.source.state == XeroSkillSourceState::Stale && record.last_diagnostic.is_none() {
            return Ok(mark_installed_record_stale(
                record,
                "skill_source_content_changed",
                "Xero marked this skill stale because the source content changed during reload.",
                false,
            ));
        }

        Ok(record)
    }

    fn apply_plugin_state_to_installed_skill(
        &self,
        mut record: InstalledSkillRecord,
    ) -> InstalledSkillRecord {
        let XeroSkillSourceLocator::Plugin { plugin_id, .. } = &record.source.locator else {
            return record;
        };
        match project_store::load_installed_plugin_by_id(&self.repo_root, plugin_id) {
            Ok(Some(plugin)) => {
                if plugin.state != XeroSkillSourceState::Enabled
                    || plugin.trust == XeroSkillTrustState::Blocked
                {
                    record.source.state = plugin.state;
                    record.source.trust = plugin.trust;
                    record.last_diagnostic =
                        plugin
                            .last_diagnostic
                            .map(|diagnostic| InstalledSkillDiagnosticRecord {
                                code: diagnostic.code,
                                message: diagnostic.message,
                                retryable: diagnostic.retryable,
                                recorded_at: diagnostic.recorded_at,
                            });
                }
                record
            }
            Ok(None) | Err(_) => {
                record.source.state = XeroSkillSourceState::Stale;
                record.last_diagnostic = Some(InstalledSkillDiagnosticRecord {
                    code: "xero_plugin_source_missing".into(),
                    message:
                        "Xero could not find the plugin that contributed this installed skill."
                            .into(),
                    retryable: false,
                    recorded_at: now_timestamp(),
                });
                record
            }
        }
    }

    fn cached_skill_candidate(
        &self,
        source_id: &str,
    ) -> CommandResult<Option<CachedSkillToolCandidate>> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(None);
        };
        let cache = skill_tool.discovery_cache.lock().map_err(|_| {
            CommandError::system_fault(
                "skill_tool_cache_lock_failed",
                "Xero could not lock the SkillTool discovery cache.",
            )
        })?;
        Ok(cache.get(source_id).cloned())
    }

    fn find_skill_by_source_id(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        source_id: &str,
        include_unavailable: bool,
        operation: XeroSkillToolOperation,
    ) -> CommandResult<Option<SkillToolSelection>> {
        if let Some(entry) = self.cached_skill_candidate(source_id)? {
            return Ok(Some(SkillToolSelection {
                public: public_skill_candidate(&entry, operation)?,
                entry,
            }));
        }
        let discovered = self.collect_skill_tool_candidates(
            skill_tool,
            None,
            include_unavailable,
            usize::MAX / 2,
            operation,
        )?;
        self.cache_skill_candidates(&discovered.entries)?;
        discovered
            .entries
            .into_iter()
            .find(|entry| candidate_source_id(entry) == source_id)
            .map(|entry| {
                Ok(SkillToolSelection {
                    public: public_skill_candidate(&entry, operation)?,
                    entry,
                })
            })
            .transpose()
    }

    fn find_skill_by_skill_id(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        skill_id: &str,
        include_unavailable: bool,
        operation: XeroSkillToolOperation,
    ) -> CommandResult<Option<SkillToolSelection>> {
        let discovered = self.collect_skill_tool_candidates(
            skill_tool,
            Some(skill_id),
            include_unavailable,
            usize::MAX / 2,
            operation,
        )?;
        self.cache_skill_candidates(&discovered.entries)?;
        let mut exact = discovered
            .entries
            .into_iter()
            .filter(|entry| candidate_skill_id(entry) == skill_id)
            .collect::<Vec<_>>();
        exact.sort_by(|left, right| {
            candidate_rank(left, Some(skill_id)).cmp(&candidate_rank(right, Some(skill_id)))
        });
        exact
            .into_iter()
            .next()
            .map(|entry| {
                Ok(SkillToolSelection {
                    public: public_skill_candidate(&entry, operation)?,
                    entry,
                })
            })
            .transpose()
    }

    fn install_github_skill(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        source: &AutonomousSkillSourceMetadata,
    ) -> CommandResult<()> {
        let latest = skill_tool
            .github_runtime
            .resolve(AutonomousSkillResolveRequest {
                skill_id: source
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(source.path.as_str())
                    .to_owned(),
                timeout_ms: None,
                source_repo: Some(source.repo.clone()),
                source_ref: Some(source.reference.clone()),
            })?;
        skill_tool
            .github_runtime
            .install(AutonomousSkillInstallRequest {
                source: latest.source.clone(),
                timeout_ms: None,
            })?;
        Ok(())
    }

    fn install_mcp_skill(&self, candidate: &ResolvedMcpSkillCandidate) -> CommandResult<()> {
        let _ = self.connected_mcp_skill_server(candidate)?;
        Ok(())
    }

    fn install_discovered_skill(
        &self,
        candidate: &XeroDiscoveredSkill,
        approval_grant_id: Option<&str>,
    ) -> CommandResult<()> {
        let trust = effective_trust(candidate.source.trust, approval_grant_id);
        let record = InstalledSkillRecord::from_discovered_skill(
            candidate,
            XeroSkillSourceState::Enabled,
            trust,
            now_timestamp(),
        )?;
        project_store::upsert_installed_skill(&self.repo_root, record)?;
        Ok(())
    }

    fn install_existing_skill(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        mut record: InstalledSkillRecord,
        approval_grant_id: Option<&str>,
    ) -> CommandResult<()> {
        match &record.source.locator {
            XeroSkillSourceLocator::Github { .. } => {
                let source = record.source.locator.to_autonomous_github_source().ok_or_else(|| {
                    CommandError::user_fixable(
                        "skill_tool_source_unsupported",
                        "Xero could not map this installed GitHub skill back to its source.",
                    )
                })?;
                self.install_github_skill(skill_tool, &source)
            }
            XeroSkillSourceLocator::Mcp { .. } => Ok(()),
            XeroSkillSourceLocator::Dynamic { .. }
                if record.source.state != XeroSkillSourceState::Enabled =>
            {
                Err(CommandError::user_fixable(
                    "skill_tool_dynamic_review_required",
                    "Dynamic skills must be explicitly reviewed and enabled before SkillTool can install or invoke them.",
                ))
            }
            _ => {
                if record.source.state != XeroSkillSourceState::Enabled {
                    record.source.state = XeroSkillSourceState::Enabled;
                    record.source.trust = effective_trust(record.source.trust, approval_grant_id);
                    record.updated_at = now_timestamp();
                    record.last_diagnostic = None;
                    project_store::upsert_installed_skill(&self.repo_root, record)?;
                }
                Ok(())
            }
        }
    }

    fn invoke_github_skill(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        source: &AutonomousSkillSourceMetadata,
        include_supporting_assets: bool,
    ) -> CommandResult<XeroSkillToolContextPayload> {
        let latest = skill_tool
            .github_runtime
            .resolve(AutonomousSkillResolveRequest {
                skill_id: source
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(source.path.as_str())
                    .to_owned(),
                timeout_ms: None,
                source_repo: Some(source.repo.clone()),
                source_ref: Some(source.reference.clone()),
            })?;
        let invoked = skill_tool
            .github_runtime
            .invoke(AutonomousSkillInvokeRequest {
                source: latest.source.clone(),
                timeout_ms: None,
            })?;
        let source_record = XeroSkillSourceRecord::github_autonomous(
            XeroSkillSourceScope::project(skill_tool.project_id.clone())?,
            &invoked.source,
            XeroSkillSourceState::Enabled,
            XeroSkillTrustState::Trusted,
        )?;
        skill_context_from_github_invocation(&source_record, invoked, include_supporting_assets)
    }

    fn invoke_mcp_skill(
        &self,
        candidate: &ResolvedMcpSkillCandidate,
        include_supporting_assets: bool,
    ) -> CommandResult<XeroSkillToolContextPayload> {
        let server = self.connected_mcp_skill_server(candidate)?;
        let timeout = super::priority_tools::normalize_mcp_timeout(None)?;
        match &candidate.capability {
            McpSkillCapability::Resource { uri } => {
                let result = super::priority_tools::invoke_mcp_server(
                    &server,
                    "resources/read",
                    json!({ "uri": uri }),
                    timeout,
                )?;
                mcp_resource_context(candidate, result, include_supporting_assets)
            }
            McpSkillCapability::Prompt { name } => {
                let result = super::priority_tools::invoke_mcp_server(
                    &server,
                    "prompts/get",
                    json!({
                        "name": name,
                        "arguments": {}
                    }),
                    timeout,
                )?;
                mcp_prompt_context(candidate, result)
            }
        }
    }

    fn connected_mcp_skill_server(
        &self,
        candidate: &ResolvedMcpSkillCandidate,
    ) -> CommandResult<McpServerRecord> {
        let registry_path = self.mcp_registry_path.as_ref().ok_or_else(|| {
            CommandError::user_fixable(
                "skill_tool_mcp_registry_unavailable",
                "Xero cannot invoke MCP-provided skills because no MCP registry path is wired.",
            )
        })?;
        let registry = load_mcp_registry_from_path(registry_path)?;
        let server =
            super::priority_tools::connected_mcp_server(&registry.servers, &candidate.server_id)?;
        Ok(server.clone())
    }

    fn invoke_discovered_skill(
        &self,
        candidate: &XeroDiscoveredSkill,
        approval_grant_id: Option<&str>,
        include_supporting_assets: bool,
    ) -> CommandResult<XeroSkillToolContextPayload> {
        self.install_discovered_skill(candidate, approval_grant_id)?;
        load_discovered_skill_context(candidate, include_supporting_assets)
    }

    fn invoke_installed_skill(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        mut record: InstalledSkillRecord,
        approval_grant_id: Option<&str>,
        include_supporting_assets: bool,
    ) -> CommandResult<XeroSkillToolContextPayload> {
        match &record.source.locator {
            XeroSkillSourceLocator::Github { .. } => {
                let source = record.source.locator.to_autonomous_github_source().ok_or_else(|| {
                    CommandError::user_fixable(
                        "skill_tool_source_unsupported",
                        "Xero could not map this installed GitHub skill back to its source.",
                    )
                })?;
                self.invoke_github_skill(skill_tool, &source, include_supporting_assets)
            }
            XeroSkillSourceLocator::Mcp { .. } => Err(CommandError::user_fixable(
                "skill_tool_mcp_installed_state_unsupported",
                "MCP-provided skills are invoked from the connected MCP server rather than from installed filesystem state.",
            )),
            XeroSkillSourceLocator::Dynamic { .. }
                if record.source.state != XeroSkillSourceState::Enabled =>
            {
                Err(CommandError::user_fixable(
                    "skill_tool_dynamic_review_required",
                    "Dynamic skills must be explicitly reviewed and enabled before SkillTool can invoke them.",
                ))
            }
            _ => {
                if record.source.state != XeroSkillSourceState::Enabled {
                    record.source.state = XeroSkillSourceState::Enabled;
                    record.source.trust = effective_trust(record.source.trust, approval_grant_id);
                }
                record.last_used_at = Some(now_timestamp());
                record.updated_at = now_timestamp();
                record.last_diagnostic = None;
                let record = project_store::upsert_installed_skill(&self.repo_root, record)?;
                load_installed_filesystem_context(&record, include_supporting_assets)
            }
        }
    }

    fn finish_install_result(
        &self,
        operation: XeroSkillToolOperation,
        public: AutonomousSkillToolCandidate,
        result: CommandResult<()>,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        match result {
            Ok(()) => {
                let selected = self
                    .refreshed_public_candidate(&public.source_id, operation)
                    .unwrap_or(public);
                let lifecycle = XeroSkillToolLifecycleEvent::succeeded(
                    operation,
                    Some(selected.source_id.clone()),
                    Some(selected.skill_id.clone()),
                    format!("Installed skill `{}`.", selected.skill_id),
                )?;
                Ok(AutonomousSkillToolOutput {
                    operation,
                    status: AutonomousSkillToolStatus::Succeeded,
                    message: format!("Installed skill `{}`.", selected.skill_id),
                    candidates: vec![selected.clone()],
                    selected: Some(selected),
                    context: None,
                    lifecycle_events: vec![lifecycle],
                    diagnostics: Vec::new(),
                    truncated: false,
                })
            }
            Err(error) => self.skill_failure_output(operation, public, "Install failed.", error),
        }
    }

    fn finish_invoke_result(
        &self,
        operation: XeroSkillToolOperation,
        public: AutonomousSkillToolCandidate,
        result: CommandResult<XeroSkillToolContextPayload>,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        match result {
            Ok(context) => {
                let selected = self
                    .refreshed_public_candidate(&public.source_id, operation)
                    .unwrap_or(public);
                let lifecycle = XeroSkillToolLifecycleEvent::succeeded(
                    operation,
                    Some(selected.source_id.clone()),
                    Some(selected.skill_id.clone()),
                    format!("Invoked skill `{}`.", selected.skill_id),
                )?;
                Ok(AutonomousSkillToolOutput {
                    operation,
                    status: AutonomousSkillToolStatus::Succeeded,
                    message: format!("Invoked skill `{}`.", selected.skill_id),
                    candidates: vec![selected.clone()],
                    selected: Some(selected),
                    context: Some(context),
                    lifecycle_events: vec![lifecycle],
                    diagnostics: Vec::new(),
                    truncated: false,
                })
            }
            Err(error) => self.skill_failure_output(operation, public, "Invoke failed.", error),
        }
    }

    fn skill_failure_output(
        &self,
        operation: XeroSkillToolOperation,
        public: AutonomousSkillToolCandidate,
        detail: &str,
        error: CommandError,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        persist_skill_failure(&self.repo_root, &public, &error);
        let lifecycle = XeroSkillToolLifecycleEvent::failed(
            operation,
            Some(public.source_id.clone()),
            Some(public.skill_id.clone()),
            detail,
            &error,
        )?;
        let diagnostic = skill_tool_diagnostic_from_command_error(&error);
        Ok(AutonomousSkillToolOutput {
            operation,
            status: AutonomousSkillToolStatus::Failed,
            message: format!("{detail} {}", diagnostic.message),
            candidates: vec![public.clone()],
            selected: Some(public),
            context: None,
            lifecycle_events: vec![lifecycle],
            diagnostics: vec![diagnostic],
            truncated: false,
        })
    }

    fn refreshed_public_candidate(
        &self,
        source_id: &str,
        operation: XeroSkillToolOperation,
    ) -> Option<AutonomousSkillToolCandidate> {
        project_store::load_installed_skill_by_source_id(&self.repo_root, source_id)
            .ok()
            .flatten()
            .and_then(|record| {
                public_skill_candidate(&CachedSkillToolCandidate::Installed(record), operation).ok()
            })
    }

    fn write_dynamic_skill_files(
        &self,
        directory: &Path,
        markdown: &str,
        supporting_assets: &[XeroSkillToolDynamicAssetInput],
    ) -> CommandResult<()> {
        let skill_path = self.dynamic_skill_write_path(directory, Path::new("SKILL.md"))?;
        fs::write(&skill_path, markdown).map_err(|error| {
            CommandError::retryable(
                "skill_tool_dynamic_write_failed",
                format!(
                    "Xero could not write dynamic skill document {}: {error}",
                    skill_path.display()
                ),
            )
        })?;
        let mut seen = BTreeSet::from(["SKILL.md".to_owned()]);
        for asset in supporting_assets {
            if !seen.insert(asset.relative_path.clone()) {
                return Err(CommandError::user_fixable(
                    "skill_tool_dynamic_duplicate_asset",
                    format!(
                        "Xero rejected dynamic skill asset `{}` because it was duplicated.",
                        asset.relative_path
                    ),
                ));
            }
            let path = self.dynamic_skill_write_path(directory, Path::new(&asset.relative_path))?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    CommandError::retryable(
                        "skill_tool_dynamic_write_failed",
                        format!("Xero could not create dynamic skill asset directory: {error}"),
                    )
                })?;
            }
            fs::write(&path, asset.content.as_bytes()).map_err(|error| {
                CommandError::retryable(
                    "skill_tool_dynamic_write_failed",
                    format!(
                        "Xero could not write dynamic skill asset {}: {error}",
                        path.display()
                    ),
                )
            })?;
        }
        Ok(())
    }

    fn dynamic_skill_write_path(
        &self,
        directory: &Path,
        relative_path: &Path,
    ) -> CommandResult<PathBuf> {
        if relative_path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        }) {
            return Err(CommandError::user_fixable(
                "skill_tool_dynamic_path_denied",
                "Xero requires dynamic skill asset paths to stay relative to their staged skill directory.",
            ));
        }
        let path = directory.join(relative_path);
        if path.starts_with(directory) {
            return Ok(path);
        }
        Err(CommandError::user_fixable(
            "skill_tool_dynamic_path_denied",
            "Xero requires dynamic skill assets to remain inside their staged skill directory.",
        ))
    }
}

fn attached_skill_resolution_success(
    snapshot: XeroAttachedSkillResolutionSnapshot,
) -> XeroAttachedSkillResolutionReport {
    XeroAttachedSkillResolutionReport {
        contract_version: XERO_ATTACHED_SKILL_RESOLUTION_CONTRACT_VERSION,
        status: XeroAttachedSkillResolutionStatus::Succeeded,
        snapshot: Some(snapshot),
        diagnostics: Vec::new(),
    }
}

fn attached_skill_resolution_failure(
    diagnostics: Vec<XeroAttachedSkillDiagnostic>,
) -> XeroAttachedSkillResolutionReport {
    XeroAttachedSkillResolutionReport {
        contract_version: XERO_ATTACHED_SKILL_RESOLUTION_CONTRACT_VERSION,
        status: XeroAttachedSkillResolutionStatus::Failed,
        snapshot: None,
        diagnostics,
    }
}

fn attached_skill_state_diagnostic(
    attachment: &XeroAttachedSkillRef,
    state: XeroSkillSourceState,
) -> Option<XeroAttachedSkillDiagnostic> {
    let (code, repair_hint, message) = match state {
        XeroSkillSourceState::Enabled => return None,
        XeroSkillSourceState::Stale => (
            "attached_skill_source_stale",
            XeroAttachedSkillRepairHint::RefreshPin,
            format!(
                "Attached skill source `{}` is stale. Refresh the attachment pin before starting this agent.",
                attachment.source_id
            ),
        ),
        XeroSkillSourceState::Blocked => (
            "attached_skill_source_blocked",
            XeroAttachedSkillRepairHint::RemoveAttachment,
            format!(
                "Attached skill source `{}` is blocked and cannot be injected.",
                attachment.source_id
            ),
        ),
        XeroSkillSourceState::Disabled => (
            "attached_skill_source_disabled",
            XeroAttachedSkillRepairHint::EnableSource,
            format!(
                "Attached skill source `{}` is disabled. Enable the source or remove the attachment.",
                attachment.source_id
            ),
        ),
        XeroSkillSourceState::Failed => (
            "attached_skill_source_failed",
            XeroAttachedSkillRepairHint::Retry,
            format!(
                "Attached skill source `{}` is in a failed state. Reload the source or remove the attachment.",
                attachment.source_id
            ),
        ),
        XeroSkillSourceState::Discoverable | XeroSkillSourceState::Installed => (
            "attached_skill_source_not_enabled",
            XeroAttachedSkillRepairHint::EnableSource,
            format!(
                "Attached skill source `{}` must be enabled before it can be injected.",
                attachment.source_id
            ),
        ),
    };
    Some(
        XeroAttachedSkillDiagnostic::user_fixable(code, message, attachment, repair_hint)
            .expect("static attached skill diagnostic should validate"),
    )
}

fn attached_skill_trust_diagnostic(
    attachment: &XeroAttachedSkillRef,
    trust: XeroSkillTrustState,
) -> Option<XeroAttachedSkillDiagnostic> {
    let (code, repair_hint, message) = match trust {
        XeroSkillTrustState::Trusted | XeroSkillTrustState::UserApproved => return None,
        XeroSkillTrustState::ApprovalRequired => (
            "attached_skill_approval_required",
            XeroAttachedSkillRepairHint::ApproveSource,
            format!(
                "Attached skill source `{}` requires user approval before it can be injected.",
                attachment.source_id
            ),
        ),
        XeroSkillTrustState::Untrusted => (
            "attached_skill_source_untrusted",
            XeroAttachedSkillRepairHint::ApproveSource,
            format!(
                "Attached skill source `{}` is untrusted. Approve the source or remove the attachment.",
                attachment.source_id
            ),
        ),
        XeroSkillTrustState::Blocked => (
            "attached_skill_trust_blocked",
            XeroAttachedSkillRepairHint::RemoveAttachment,
            format!(
                "Attached skill source `{}` is blocked by trust policy and cannot be injected.",
                attachment.source_id
            ),
        ),
    };
    Some(
        XeroAttachedSkillDiagnostic::user_fixable(code, message, attachment, repair_hint)
            .expect("static attached skill diagnostic should validate"),
    )
}

fn attached_skill_error_diagnostic(
    attachment: &XeroAttachedSkillRef,
    error: &CommandError,
    repair_hint: Option<XeroAttachedSkillRepairHint>,
) -> XeroAttachedSkillDiagnostic {
    let mut diagnostic = skill_tool_diagnostic_from_command_error(error);
    if diagnostic.code == "skill_tool_failed" {
        diagnostic.code = "attached_skill_resolution_failed".into();
    }
    XeroAttachedSkillDiagnostic::from_skill_tool_diagnostic(diagnostic, attachment, repair_hint)
}

fn collect_mcp_resource_skill_candidates(
    skill_tool: &super::AutonomousSkillToolRuntime,
    server: &McpServerRecord,
    timeout: u64,
    entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
    diagnostics: &mut Vec<XeroSkillToolDiagnostic>,
) {
    let result = match super::priority_tools::invoke_mcp_server(
        server,
        "resources/list",
        json!({}),
        timeout,
    ) {
        Ok(result) => result,
        Err(error) if mcp_capability_absent(&error) => return,
        Err(error) => {
            diagnostics.push(mcp_projection_error_diagnostic(
                server,
                "resources/list",
                &error,
            ));
            return;
        }
    };
    let Some(resources) = result.get("resources").and_then(JsonValue::as_array) else {
        diagnostics.push(diagnostic(
            "skill_tool_mcp_projection_invalid",
            format!(
                "Xero could not project MCP skills from server `{}` because resources/list did not return a resources array.",
                server.id
            ),
            false,
        ));
        return;
    };

    for resource in resources {
        match mcp_resource_skill_candidate(skill_tool, server, resource) {
            Ok(Some(candidate)) => {
                insert_candidate(entries, CachedSkillToolCandidate::Mcp(candidate))
            }
            Ok(None) => {}
            Err(error) => diagnostics.push(skill_tool_diagnostic_from_command_error(&error)),
        }
    }
}

fn collect_mcp_prompt_skill_candidates(
    skill_tool: &super::AutonomousSkillToolRuntime,
    server: &McpServerRecord,
    timeout: u64,
    entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
    diagnostics: &mut Vec<XeroSkillToolDiagnostic>,
) {
    let result = match super::priority_tools::invoke_mcp_server(
        server,
        "prompts/list",
        json!({}),
        timeout,
    ) {
        Ok(result) => result,
        Err(error) if mcp_capability_absent(&error) => return,
        Err(error) => {
            diagnostics.push(mcp_projection_error_diagnostic(
                server,
                "prompts/list",
                &error,
            ));
            return;
        }
    };
    let Some(prompts) = result.get("prompts").and_then(JsonValue::as_array) else {
        diagnostics.push(diagnostic(
            "skill_tool_mcp_projection_invalid",
            format!(
                "Xero could not project MCP skills from server `{}` because prompts/list did not return a prompts array.",
                server.id
            ),
            false,
        ));
        return;
    };

    for prompt in prompts {
        match mcp_prompt_skill_candidate(skill_tool, server, prompt) {
            Ok(Some(candidate)) => {
                insert_candidate(entries, CachedSkillToolCandidate::Mcp(candidate))
            }
            Ok(None) => {}
            Err(error) => diagnostics.push(skill_tool_diagnostic_from_command_error(&error)),
        }
    }
}

fn mcp_resource_skill_candidate(
    skill_tool: &super::AutonomousSkillToolRuntime,
    server: &McpServerRecord,
    resource: &JsonValue,
) -> CommandResult<Option<ResolvedMcpSkillCandidate>> {
    if !mcp_resource_is_skill(resource) {
        return Ok(None);
    }
    let uri = required_json_string(resource, "uri", "skill_tool_mcp_resource_invalid")?;
    let skill_id = mcp_skill_id_from_value(resource)
        .or_else(|| mcp_skill_id_from_text(resource.get("name").and_then(JsonValue::as_str)))
        .or_else(|| mcp_skill_id_from_uri(&uri))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "skill_tool_mcp_skill_id_invalid",
                format!(
                    "Xero could not derive a kebab-case skill id for MCP resource `{uri}` from server `{}`.",
                    server.id
                ),
            )
        })?;
    let name = resource
        .get("name")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&skill_id)
        .to_owned();
    let base_description = resource
        .get("description")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("MCP resource skill.");
    let capability_id = mcp_capability_id("resource", &skill_id, &uri);
    let source = XeroSkillSourceRecord::new(
        XeroSkillSourceScope::project(skill_tool.project_id.clone())?,
        XeroSkillSourceLocator::Mcp {
            server_id: server.id.clone(),
            capability_id,
            skill_id: skill_id.clone(),
        },
        XeroSkillSourceState::Discoverable,
        XeroSkillTrustState::Trusted,
    )?;
    Ok(Some(ResolvedMcpSkillCandidate {
        source,
        skill_id,
        name,
        description: mcp_candidate_description(base_description, server),
        user_invocable: mcp_user_invocable(resource),
        version_hash: mcp_candidate_hash(server, "resource", &uri, base_description),
        server_id: server.id.clone(),
        capability: McpSkillCapability::Resource { uri },
    }))
}

fn mcp_prompt_skill_candidate(
    skill_tool: &super::AutonomousSkillToolRuntime,
    server: &McpServerRecord,
    prompt: &JsonValue,
) -> CommandResult<Option<ResolvedMcpSkillCandidate>> {
    if !mcp_prompt_is_skill(prompt) {
        return Ok(None);
    }
    let prompt_name = required_json_string(prompt, "name", "skill_tool_mcp_prompt_invalid")?;
    let skill_id = mcp_skill_id_from_value(prompt)
        .or_else(|| mcp_skill_id_from_prompt_name(&prompt_name))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "skill_tool_mcp_skill_id_invalid",
                format!(
                    "Xero could not derive a kebab-case skill id for MCP prompt `{prompt_name}` from server `{}`.",
                    server.id
                ),
            )
        })?;
    let base_description = prompt
        .get("description")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("MCP prompt skill.");
    let capability_id = mcp_capability_id("prompt", &skill_id, &prompt_name);
    let source = XeroSkillSourceRecord::new(
        XeroSkillSourceScope::project(skill_tool.project_id.clone())?,
        XeroSkillSourceLocator::Mcp {
            server_id: server.id.clone(),
            capability_id,
            skill_id: skill_id.clone(),
        },
        XeroSkillSourceState::Discoverable,
        XeroSkillTrustState::Trusted,
    )?;
    Ok(Some(ResolvedMcpSkillCandidate {
        source,
        skill_id: skill_id.clone(),
        name: skill_id.replace('-', " "),
        description: mcp_candidate_description(base_description, server),
        user_invocable: mcp_user_invocable(prompt),
        version_hash: mcp_candidate_hash(server, "prompt", &prompt_name, base_description),
        server_id: server.id.clone(),
        capability: McpSkillCapability::Prompt { name: prompt_name },
    }))
}

struct SkillToolDiscoveryResult {
    entries: Vec<CachedSkillToolCandidate>,
    diagnostics: Vec<XeroSkillToolDiagnostic>,
    truncated: bool,
}

fn push_discovery(
    entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
    diagnostics: &mut Vec<XeroSkillToolDiagnostic>,
    discovered: XeroSkillDirectoryDiscovery,
) {
    for candidate in discovered.candidates {
        insert_candidate(entries, CachedSkillToolCandidate::Discovered(candidate));
    }
    diagnostics.extend(discovered.diagnostics.into_iter().map(|diagnostic| {
        let error = CommandError::user_fixable(diagnostic.code, diagnostic.message);
        skill_tool_diagnostic_from_command_error(&error)
    }));
}

fn insert_candidate(
    entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
    candidate: CachedSkillToolCandidate,
) {
    let source_id = candidate_source_id(&candidate);
    match entries.remove(&source_id) {
        Some(CachedSkillToolCandidate::Installed(record)) => {
            let fallback = record.clone();
            entries.insert(
                source_id,
                CachedSkillToolCandidate::Installed(
                    merge_installed_candidate(record, candidate).unwrap_or(fallback),
                ),
            );
        }
        Some(existing) if matches!(candidate, CachedSkillToolCandidate::Installed(_)) => {
            let CachedSkillToolCandidate::Installed(record) = candidate else {
                unreachable!("checked installed candidate");
            };
            let fallback = record.clone();
            entries.insert(
                source_id,
                CachedSkillToolCandidate::Installed(
                    merge_installed_candidate(record, existing).unwrap_or(fallback),
                ),
            );
        }
        Some(existing) => {
            entries.insert(source_id, existing);
        }
        None => {
            entries.insert(source_id, candidate);
        }
    }
}

fn merge_installed_candidate(
    mut record: InstalledSkillRecord,
    discovered: CachedSkillToolCandidate,
) -> CommandResult<InstalledSkillRecord> {
    if matches!(
        record.source.state,
        XeroSkillSourceState::Disabled
            | XeroSkillSourceState::Failed
            | XeroSkillSourceState::Blocked
    ) {
        return Ok(record);
    }

    let Some(discovered_version) = candidate_version_hash(&discovered) else {
        return Ok(record);
    };
    if record.version_hash.as_deref() == Some(discovered_version.as_str()) {
        return Ok(record);
    }

    let discovered_source = source_record_for_entry(&discovered)?;
    record.source.locator = discovered_source.locator;
    record.source.state = XeroSkillSourceState::Stale;
    record.name = candidate_name(&discovered);
    record.description = candidate_description(&discovered);
    record.user_invocable = candidate_user_invocable(&discovered);
    record.local_location = candidate_local_location(&discovered).or(record.local_location);
    record.version_hash = Some(discovered_version);
    record.updated_at = now_timestamp();
    if record.last_diagnostic.is_none() {
        record.last_diagnostic = Some(InstalledSkillDiagnosticRecord {
            code: "skill_source_content_changed".into(),
            message:
                "Xero marked this skill stale because the source content changed during reload."
                    .into(),
            retryable: false,
            recorded_at: now_timestamp(),
        });
    }
    Ok(record)
}

fn public_skill_candidate(
    entry: &CachedSkillToolCandidate,
    operation: XeroSkillToolOperation,
) -> CommandResult<AutonomousSkillToolCandidate> {
    let record = source_record_for_entry(entry)?;
    let access = decide_skill_tool_access(&record, operation)?;
    let state = record.state;
    let trust = record.trust;
    Ok(AutonomousSkillToolCandidate {
        source_id: record.source_id,
        skill_id: candidate_skill_id(entry),
        name: sanitize_candidate_text(&candidate_name(entry), &candidate_skill_id(entry)),
        description: sanitize_candidate_text(&candidate_description(entry), ""),
        source_kind: record.locator.kind(),
        state,
        trust,
        enabled: state == XeroSkillSourceState::Enabled,
        installed: matches!(entry, CachedSkillToolCandidate::Installed(_)),
        user_invocable: candidate_user_invocable(entry),
        version_hash: candidate_version_hash(entry),
        cache_key: candidate_cache_key(entry),
        access,
    })
}

fn sanitize_candidate_text(value: &str, fallback: &str) -> String {
    let (sanitized, _redacted) = sanitize_skill_tool_model_text(value);
    if sanitized.trim().is_empty() {
        fallback.to_owned()
    } else {
        sanitized
    }
}

fn source_record_for_entry(
    entry: &CachedSkillToolCandidate,
) -> CommandResult<XeroSkillSourceRecord> {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.source.clone().validate(),
        CachedSkillToolCandidate::Discovered(candidate) => candidate.source.clone().validate(),
        CachedSkillToolCandidate::Github(candidate) => candidate.source.clone().validate(),
        CachedSkillToolCandidate::Mcp(candidate) => candidate.source.clone().validate(),
    }
}

fn candidate_source_id(entry: &CachedSkillToolCandidate) -> String {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.source.source_id.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => candidate.source.source_id.clone(),
        CachedSkillToolCandidate::Github(candidate) => candidate.source.source_id.clone(),
        CachedSkillToolCandidate::Mcp(candidate) => candidate.source.source_id.clone(),
    }
}

fn candidate_skill_id(entry: &CachedSkillToolCandidate) -> String {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.skill_id.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => candidate.skill_id.clone(),
        CachedSkillToolCandidate::Github(candidate) => candidate.skill_id.clone(),
        CachedSkillToolCandidate::Mcp(candidate) => candidate.skill_id.clone(),
    }
}

fn candidate_name(entry: &CachedSkillToolCandidate) -> String {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.name.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => candidate.name.clone(),
        CachedSkillToolCandidate::Github(candidate) => candidate.name.clone(),
        CachedSkillToolCandidate::Mcp(candidate) => candidate.name.clone(),
    }
}

fn candidate_description(entry: &CachedSkillToolCandidate) -> String {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.description.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => candidate.description.clone(),
        CachedSkillToolCandidate::Github(candidate) => candidate.description.clone(),
        CachedSkillToolCandidate::Mcp(candidate) => candidate.description.clone(),
    }
}

fn candidate_user_invocable(entry: &CachedSkillToolCandidate) -> Option<bool> {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.user_invocable,
        CachedSkillToolCandidate::Discovered(candidate) => candidate.user_invocable,
        CachedSkillToolCandidate::Github(candidate) => candidate.user_invocable,
        CachedSkillToolCandidate::Mcp(candidate) => candidate.user_invocable,
    }
}

fn candidate_version_hash(entry: &CachedSkillToolCandidate) -> Option<String> {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.version_hash.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => Some(candidate.version_hash.clone()),
        CachedSkillToolCandidate::Github(candidate) => {
            Some(candidate.github_source.tree_hash.clone())
        }
        CachedSkillToolCandidate::Mcp(candidate) => Some(candidate.version_hash.clone()),
    }
}

fn candidate_cache_key(entry: &CachedSkillToolCandidate) -> Option<String> {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.cache_key.clone(),
        CachedSkillToolCandidate::Discovered(_)
        | CachedSkillToolCandidate::Github(_)
        | CachedSkillToolCandidate::Mcp(_) => None,
    }
}

fn candidate_local_location(entry: &CachedSkillToolCandidate) -> Option<String> {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.local_location.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => Some(candidate.local_location.clone()),
        CachedSkillToolCandidate::Github(_) | CachedSkillToolCandidate::Mcp(_) => None,
    }
}

fn reload_unavailable_reason(
    skill_tool: &super::AutonomousSkillToolRuntime,
    record: &InstalledSkillRecord,
) -> Option<(&'static str, String)> {
    match &record.source.locator {
        XeroSkillSourceLocator::Local { root_id, .. } => {
            if !skill_tool
                .local_roots
                .iter()
                .any(|root| root.root_id == *root_id)
            {
                return Some((
                    "skill_source_root_unavailable",
                    format!(
                        "Xero marked this skill stale because local skill root `{root_id}` is no longer configured for this run."
                    ),
                ));
            }
            filesystem_skill_location_unavailable(record)
        }
        XeroSkillSourceLocator::Project { .. } => {
            if !skill_tool.project_skills_enabled {
                return Some((
                    "skill_source_root_unavailable",
                    "Xero marked this skill stale because project skill discovery is disabled for this run.".into(),
                ));
            }
            filesystem_skill_location_unavailable(record)
        }
        XeroSkillSourceLocator::Bundled { bundle_id, .. } => {
            if !skill_tool
                .bundled_roots
                .iter()
                .any(|root| root.bundle_id == *bundle_id)
            {
                return Some((
                    "skill_source_root_unavailable",
                    format!(
                        "Xero marked this skill stale because bundled skill root `{bundle_id}` is no longer available."
                    ),
                ));
            }
            filesystem_skill_location_unavailable(record)
        }
        XeroSkillSourceLocator::Dynamic { .. } | XeroSkillSourceLocator::Plugin { .. } => {
            filesystem_skill_location_unavailable(record)
        }
        XeroSkillSourceLocator::Github { .. } | XeroSkillSourceLocator::Mcp { .. } => None,
    }
}

fn filesystem_skill_location_unavailable(
    record: &InstalledSkillRecord,
) -> Option<(&'static str, String)> {
    let Some(local_location) = record.local_location.as_deref() else {
        return Some((
            "skill_source_content_missing",
            "Xero marked this skill stale because its installed filesystem location is missing from durable state.".into(),
        ));
    };
    let path = Path::new(local_location);
    if !path.join("SKILL.md").is_file() {
        return Some((
            "skill_source_content_missing",
            "Xero marked this skill stale because SKILL.md was not found at the installed source location.".into(),
        ));
    }
    None
}

fn mark_installed_record_stale(
    mut record: InstalledSkillRecord,
    code: impl Into<String>,
    message: impl Into<String>,
    retryable: bool,
) -> InstalledSkillRecord {
    if record.source.state != XeroSkillSourceState::Blocked {
        record.source.state = XeroSkillSourceState::Stale;
    }
    let timestamp = now_timestamp();
    record.updated_at = timestamp.clone();
    record.last_diagnostic = Some(InstalledSkillDiagnosticRecord {
        code: code.into(),
        message: message.into(),
        retryable,
        recorded_at: timestamp,
    });
    record
}

fn candidate_model_visible(entry: &CachedSkillToolCandidate) -> bool {
    source_record_for_entry(entry)
        .ok()
        .and_then(|record| decide_skill_tool_access(&record, XeroSkillToolOperation::List).ok())
        .is_some_and(|decision| decision.model_visible)
}

fn candidate_matches_query(entry: &CachedSkillToolCandidate, query: &str) -> bool {
    let terms = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {}",
        candidate_skill_id(entry),
        candidate_name(entry),
        candidate_description(entry)
    )
    .replace('-', " ")
    .to_ascii_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}

fn candidate_rank(entry: &CachedSkillToolCandidate, query: Option<&str>) -> (u8, u8, u8, u8) {
    let exact = query
        .map(|query| candidate_skill_id(entry) == query.trim())
        .unwrap_or(false);
    let state = source_record_for_entry(entry)
        .map(|record| record.state)
        .unwrap_or(XeroSkillSourceState::Failed);
    let trust = source_record_for_entry(entry)
        .map(|record| record.trust)
        .unwrap_or(XeroSkillTrustState::Blocked);
    (
        if exact { 0 } else { 1 },
        match state {
            XeroSkillSourceState::Enabled => 0,
            XeroSkillSourceState::Installed => 1,
            XeroSkillSourceState::Discoverable => 2,
            XeroSkillSourceState::Stale => 3,
            XeroSkillSourceState::Disabled => 4,
            XeroSkillSourceState::Failed => 5,
            XeroSkillSourceState::Blocked => 6,
        },
        match trust {
            XeroSkillTrustState::Trusted => 0,
            XeroSkillTrustState::UserApproved => 1,
            XeroSkillTrustState::ApprovalRequired => 2,
            XeroSkillTrustState::Untrusted => 3,
            XeroSkillTrustState::Blocked => 4,
        },
        match entry {
            CachedSkillToolCandidate::Discovered(candidate) => {
                match candidate.source.locator.kind() {
                    crate::runtime::XeroSkillSourceKind::Bundled => 0,
                    crate::runtime::XeroSkillSourceKind::Project => 1,
                    crate::runtime::XeroSkillSourceKind::Local => 2,
                    _ => 3,
                }
            }
            CachedSkillToolCandidate::Github(_) => 3,
            CachedSkillToolCandidate::Mcp(_) => 4,
            CachedSkillToolCandidate::Installed(_) => 0,
        },
    )
}

fn github_candidate(
    skill_tool: &super::AutonomousSkillToolRuntime,
    resolved: AutonomousSkillResolveOutput,
) -> CommandResult<ResolvedGithubSkillCandidate> {
    let trust = if resolved.source.repo == skill_tool.github_runtime.config().default_source_repo
        && resolved.source.reference == skill_tool.github_runtime.config().default_source_ref
    {
        XeroSkillTrustState::Trusted
    } else {
        XeroSkillTrustState::UserApproved
    };
    let source = XeroSkillSourceRecord::github_autonomous(
        XeroSkillSourceScope::project(skill_tool.project_id.clone())?,
        &resolved.source,
        XeroSkillSourceState::Enabled,
        trust,
    )?;
    Ok(ResolvedGithubSkillCandidate {
        source,
        skill_id: resolved.skill_id,
        name: resolved.name,
        description: resolved.description,
        user_invocable: resolved.user_invocable,
        github_source: resolved.source,
    })
}

fn access_for_operation(
    entry: &CachedSkillToolCandidate,
    operation: XeroSkillToolOperation,
    approval_grant_id: Option<&str>,
) -> CommandResult<crate::runtime::XeroSkillToolAccessDecision> {
    let mut record = source_record_for_entry(entry)?;
    let effective_operation = if operation == XeroSkillToolOperation::Invoke
        && (!matches!(entry, CachedSkillToolCandidate::Installed(_))
            || record.state == XeroSkillSourceState::Stale)
    {
        XeroSkillToolOperation::Install
    } else {
        operation
    };
    if approval_grant_id.is_some()
        && !matches!(
            record.trust,
            XeroSkillTrustState::Blocked | XeroSkillTrustState::Trusted
        )
    {
        record.trust = XeroSkillTrustState::UserApproved;
    }
    decide_skill_tool_access(&record, effective_operation)
}

fn effective_trust(
    trust: XeroSkillTrustState,
    approval_grant_id: Option<&str>,
) -> XeroSkillTrustState {
    if approval_grant_id.is_some()
        && !matches!(
            trust,
            XeroSkillTrustState::Blocked | XeroSkillTrustState::Trusted
        )
    {
        XeroSkillTrustState::UserApproved
    } else {
        trust
    }
}

fn load_installed_filesystem_context(
    record: &InstalledSkillRecord,
    include_supporting_assets: bool,
) -> CommandResult<XeroSkillToolContextPayload> {
    let local_location = record.local_location.as_deref().ok_or_else(|| {
        CommandError::user_fixable(
            "skill_tool_location_missing",
            format!(
                "Installed skill `{}` does not have a local location.",
                record.skill_id
            ),
        )
    })?;
    load_skill_context_from_directory(
        &record.source.source_id,
        &record.skill_id,
        Path::new(local_location),
        None,
        include_supporting_assets,
    )
}

fn skill_context_from_github_invocation(
    source_record: &XeroSkillSourceRecord,
    invoked: crate::runtime::AutonomousSkillInvokeOutput,
    include_supporting_assets: bool,
) -> CommandResult<XeroSkillToolContextPayload> {
    let markdown_bytes = invoked.skill_markdown.as_bytes();
    let supporting_assets = if include_supporting_assets {
        invoked
            .supporting_assets
            .into_iter()
            .map(|asset| XeroSkillToolContextAsset {
                relative_path: asset.relative_path,
                sha256: asset.sha256,
                bytes: asset.bytes,
                content: asset.content,
            })
            .collect()
    } else {
        Vec::new()
    };
    validate_skill_tool_context_payload(XeroSkillToolContextPayload {
        contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
        source_id: source_record.source_id.clone(),
        skill_id: invoked.skill_id,
        markdown: XeroSkillToolContextDocument {
            relative_path: "SKILL.md".into(),
            sha256: sha256_hex(markdown_bytes),
            bytes: markdown_bytes.len(),
            content: invoked.skill_markdown,
        },
        supporting_assets,
    })
}

fn mcp_resource_context(
    candidate: &ResolvedMcpSkillCandidate,
    result: JsonValue,
    include_supporting_assets: bool,
) -> CommandResult<XeroSkillToolContextPayload> {
    let contents = result
        .get("contents")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "skill_tool_mcp_resource_invalid",
                format!(
                    "Xero expected MCP resource skill `{}` from server `{}` to return a contents array.",
                    candidate.skill_id, candidate.server_id
                ),
            )
        })?;
    let mut markdown = None;
    let mut assets = Vec::new();

    for (index, content) in contents.iter().enumerate() {
        let text = content.get("text").and_then(JsonValue::as_str);
        let Some(text) = text else {
            if content.get("blob").is_some() {
                return Err(CommandError::user_fixable(
                    "skill_tool_mcp_resource_binary_unsupported",
                    format!(
                        "Xero rejected MCP resource skill `{}` because it returned binary blob content.",
                        candidate.skill_id
                    ),
                ));
            }
            continue;
        };
        if markdown.is_none() && mcp_content_is_skill_markdown(content, text, contents.len()) {
            markdown = Some(text.to_owned());
            continue;
        }
        if include_supporting_assets {
            let relative_path = mcp_content_asset_path(content, index);
            assets.push(XeroSkillToolContextAsset {
                relative_path,
                sha256: sha256_hex(text.as_bytes()),
                bytes: text.len(),
                content: text.to_owned(),
            });
        }
    }

    let markdown = markdown.ok_or_else(|| {
        CommandError::user_fixable(
            "skill_tool_mcp_resource_missing_skill_markdown",
            format!(
                "Xero could not find text SKILL.md content in MCP resource skill `{}` from server `{}`.",
                candidate.skill_id, candidate.server_id
            ),
        )
    })?;
    assert_mcp_markdown_matches_candidate(candidate, &markdown)?;
    skill_context_from_mcp_markdown(candidate, markdown, assets)
}

fn mcp_prompt_context(
    candidate: &ResolvedMcpSkillCandidate,
    result: JsonValue,
) -> CommandResult<XeroSkillToolContextPayload> {
    let prompt_text = mcp_prompt_text(&result)?;
    let markdown = if prompt_text.trim_start().starts_with("---") {
        assert_mcp_markdown_matches_candidate(candidate, &prompt_text)?;
        prompt_text
    } else {
        synthesize_mcp_prompt_skill_markdown(candidate, &prompt_text)
    };
    skill_context_from_mcp_markdown(candidate, markdown, Vec::new())
}

fn skill_context_from_mcp_markdown(
    candidate: &ResolvedMcpSkillCandidate,
    markdown: String,
    supporting_assets: Vec<XeroSkillToolContextAsset>,
) -> CommandResult<XeroSkillToolContextPayload> {
    validate_skill_tool_context_payload(XeroSkillToolContextPayload {
        contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
        source_id: candidate.source.source_id.clone(),
        skill_id: candidate.skill_id.clone(),
        markdown: XeroSkillToolContextDocument {
            relative_path: "SKILL.md".into(),
            sha256: sha256_hex(markdown.as_bytes()),
            bytes: markdown.len(),
            content: markdown,
        },
        supporting_assets,
    })
}

fn skill_unavailable_output(operation: XeroSkillToolOperation) -> AutonomousSkillToolOutput {
    AutonomousSkillToolOutput {
        operation,
        status: AutonomousSkillToolStatus::Unavailable,
        message: "SkillTool is not enabled for this owned-agent run.".into(),
        candidates: Vec::new(),
        selected: None,
        context: None,
        lifecycle_events: Vec::new(),
        diagnostics: vec![diagnostic(
            "skill_tool_unavailable",
            "Xero did not configure skill support for this owned-agent run.",
            false,
        )],
        truncated: false,
    }
}

fn skill_not_found_output(
    operation: XeroSkillToolOperation,
    source_id: &str,
) -> AutonomousSkillToolOutput {
    AutonomousSkillToolOutput {
        operation,
        status: AutonomousSkillToolStatus::Unavailable,
        message: format!("SkillTool could not find source `{source_id}`."),
        candidates: Vec::new(),
        selected: None,
        context: None,
        lifecycle_events: Vec::new(),
        diagnostics: vec![diagnostic(
            "skill_tool_skill_not_found",
            "Xero could not find that source in the durable registry, configured sources, or current discovery cache.",
            false,
        )],
        truncated: false,
    }
}

fn approval_required_output(
    operation: XeroSkillToolOperation,
    candidate: AutonomousSkillToolCandidate,
    reason: Option<XeroSkillToolDiagnostic>,
) -> AutonomousSkillToolOutput {
    let diagnostic = reason.unwrap_or_else(|| {
        diagnostic(
            "skill_tool_user_approval_required",
            "Xero requires user approval before this skill source can be installed or invoked.",
            false,
        )
    });
    let lifecycle = XeroSkillToolLifecycleEvent {
        contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
        operation,
        result: XeroSkillToolLifecycleResult::ApprovalRequired,
        source_id: Some(candidate.source_id.clone()),
        skill_id: Some(candidate.skill_id.clone()),
        detail: format!("Skill `{}` requires user approval.", candidate.skill_id),
        diagnostic: Some(diagnostic.clone()),
    };
    AutonomousSkillToolOutput {
        operation,
        status: AutonomousSkillToolStatus::ApprovalRequired,
        message: diagnostic.message.clone(),
        candidates: vec![candidate.clone()],
        selected: Some(candidate),
        context: None,
        lifecycle_events: vec![lifecycle],
        diagnostics: vec![diagnostic],
        truncated: false,
    }
}

fn denied_output(
    operation: XeroSkillToolOperation,
    candidate: AutonomousSkillToolCandidate,
    reason: Option<XeroSkillToolDiagnostic>,
) -> AutonomousSkillToolOutput {
    let diagnostic = reason.unwrap_or_else(|| {
        diagnostic(
            "skill_tool_source_denied",
            "Xero denied this SkillTool operation for the selected source.",
            false,
        )
    });
    AutonomousSkillToolOutput {
        operation,
        status: AutonomousSkillToolStatus::Failed,
        message: diagnostic.message.clone(),
        candidates: vec![candidate.clone()],
        selected: Some(candidate),
        context: None,
        lifecycle_events: Vec::new(),
        diagnostics: vec![diagnostic],
        truncated: false,
    }
}

fn diagnostic(
    code: impl Into<String>,
    message: impl Into<String>,
    retryable: bool,
) -> XeroSkillToolDiagnostic {
    let (message, redacted) = sanitize_skill_tool_model_text(&message.into());
    XeroSkillToolDiagnostic {
        code: code.into(),
        message,
        retryable,
        redacted,
    }
}

fn plugin_diagnostic(
    code: impl Into<String>,
    message: impl Into<String>,
) -> XeroSkillToolDiagnostic {
    diagnostic(code, message, false)
}

fn mcp_server_unavailable_diagnostic(server: &McpServerRecord) -> XeroSkillToolDiagnostic {
    let status = format!("{:?}", server.connection.status).to_ascii_lowercase();
    let retryable = server
        .connection
        .diagnostic
        .as_ref()
        .map(|diagnostic| diagnostic.retryable)
        .unwrap_or(true);
    let message = server
        .connection
        .diagnostic
        .as_ref()
        .map(|diagnostic| diagnostic.message.clone())
        .unwrap_or_else(|| {
            format!(
                "MCP server `{}` is {status}; Xero exposes MCP-provided skills only from connected servers.",
                server.id
            )
        });
    diagnostic("skill_tool_mcp_server_unavailable", message, retryable)
}

fn mcp_projection_error_diagnostic(
    server: &McpServerRecord,
    method: &str,
    error: &CommandError,
) -> XeroSkillToolDiagnostic {
    let mut diagnostic = skill_tool_diagnostic_from_command_error(error);
    diagnostic.code = "skill_tool_mcp_projection_failed".into();
    let (message, redacted) = sanitize_skill_tool_model_text(&format!(
        "Xero could not project MCP skills from server `{}` with {method}: {}",
        server.id, diagnostic.message
    ));
    diagnostic.message = message;
    diagnostic.redacted |= redacted;
    diagnostic
}

fn mcp_capability_absent(error: &CommandError) -> bool {
    error.code == "autonomous_tool_mcp_error"
        && (error.message.contains("-32601")
            || error
                .message
                .to_ascii_lowercase()
                .contains("method not found"))
}

fn mcp_resource_is_skill(resource: &JsonValue) -> bool {
    if mcp_has_skill_marker(resource) {
        return true;
    }
    let uri = resource
        .get("uri")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    uri.starts_with("skill://")
        || uri.starts_with("xero-skill://")
        || uri.ends_with("/skill.md")
        || uri.ends_with(":skill.md")
}

fn mcp_prompt_is_skill(prompt: &JsonValue) -> bool {
    if mcp_has_skill_marker(prompt) {
        return true;
    }
    prompt
        .get("name")
        .and_then(JsonValue::as_str)
        .map(|name| {
            let normalized = name.trim().to_ascii_lowercase();
            normalized.starts_with("skill:") || normalized.starts_with("xero.skill.")
        })
        .unwrap_or(false)
}

fn mcp_has_skill_marker(value: &JsonValue) -> bool {
    for container in [
        Some(value),
        value.get("metadata"),
        value.get("annotations"),
        value.get("_meta"),
    ]
    .into_iter()
    .flatten()
    {
        if container
            .get("xeroSkill")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
            || container
                .get("skill")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false)
        {
            return true;
        }
        if container
            .get("kind")
            .or_else(|| container.get("type"))
            .and_then(JsonValue::as_str)
            .map(|kind| kind.eq_ignore_ascii_case("skill"))
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

fn required_json_string(
    value: &JsonValue,
    field: &str,
    code: &'static str,
) -> CommandResult<String> {
    value
        .get(field)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            CommandError::user_fixable(
                code,
                format!(
                    "Xero expected MCP skill metadata field `{field}` to be a non-empty string."
                ),
            )
        })
}

fn mcp_skill_id_from_value(value: &JsonValue) -> Option<String> {
    for container in [
        Some(value),
        value.get("metadata"),
        value.get("annotations"),
        value.get("_meta"),
    ]
    .into_iter()
    .flatten()
    {
        for key in ["skillId", "skill_id", "xeroSkillId"] {
            if let Some(skill_id) =
                mcp_skill_id_from_text(container.get(key).and_then(JsonValue::as_str))
            {
                return Some(skill_id);
            }
        }
    }
    None
}

fn mcp_skill_id_from_uri(uri: &str) -> Option<String> {
    let trimmed = uri
        .split(['?', '#'])
        .next()
        .unwrap_or(uri)
        .trim_end_matches('/');
    let mut segments = trimmed
        .split(['/', ':'])
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>();
    if segments
        .last()
        .map(|segment| segment.eq_ignore_ascii_case("SKILL.md"))
        .unwrap_or(false)
    {
        segments.pop();
    }
    segments
        .last()
        .and_then(|segment| mcp_skill_id_from_text(Some(segment)))
}

fn mcp_skill_id_from_prompt_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    let lower = trimmed.to_ascii_lowercase();
    let normalized = if lower.starts_with("skill:") {
        &trimmed["skill:".len()..]
    } else if lower.starts_with("xero.skill.") {
        &trimmed["xero.skill.".len()..]
    } else {
        trimmed
    };
    mcp_skill_id_from_text(Some(normalized))
}

fn mcp_skill_id_from_text(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut last_was_dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    let out = out.trim_matches('-').to_owned();
    (!out.is_empty()).then_some(out)
}

fn mcp_user_invocable(value: &JsonValue) -> Option<bool> {
    for container in [
        Some(value),
        value.get("metadata"),
        value.get("annotations"),
        value.get("_meta"),
    ]
    .into_iter()
    .flatten()
    {
        for key in ["userInvocable", "user-invocable", "user_invocable"] {
            if let Some(value) = container.get(key).and_then(JsonValue::as_bool) {
                return Some(value);
            }
        }
    }
    None
}

fn mcp_capability_id(kind: &str, skill_id: &str, stable_value: &str) -> String {
    format!(
        "{kind}-{skill_id}-{}",
        &sha256_hex(stable_value.as_bytes())[..12]
    )
}

fn mcp_candidate_hash(
    server: &McpServerRecord,
    kind: &str,
    stable_value: &str,
    description: &str,
) -> String {
    sha256_hex(
        format!(
            "{}\0{}\0{}\0{}\0{}",
            server.id, server.updated_at, kind, stable_value, description
        )
        .as_bytes(),
    )
}

fn mcp_candidate_description(description: &str, server: &McpServerRecord) -> String {
    format!(
        "{} Source: MCP server `{}` ({})",
        description.trim(),
        server.name,
        server.id
    )
}

fn mcp_content_is_skill_markdown(content: &JsonValue, text: &str, content_count: usize) -> bool {
    let uri = content
        .get("uri")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    uri.ends_with("/skill.md")
        || uri.ends_with(":skill.md")
        || content
            .get("name")
            .and_then(JsonValue::as_str)
            .map(|name| name.eq_ignore_ascii_case("SKILL.md"))
            .unwrap_or(false)
        || (content_count == 1 && text.trim_start().starts_with("---"))
}

fn mcp_content_asset_path(content: &JsonValue, index: usize) -> String {
    let raw = content
        .get("uri")
        .and_then(JsonValue::as_str)
        .or_else(|| content.get("name").and_then(JsonValue::as_str))
        .unwrap_or("asset.md");
    let leaf = raw
        .split(['?', '#'])
        .next()
        .unwrap_or(raw)
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("asset.md");
    let mut sanitized = sanitize_path_segment(leaf);
    if sanitized.is_empty() || sanitized.eq_ignore_ascii_case("SKILL.md") {
        sanitized = format!("asset-{index}.md");
    }
    if !sanitized.contains('.') {
        sanitized.push_str(".md");
    }
    sanitized
}

fn assert_mcp_markdown_matches_candidate(
    candidate: &ResolvedMcpSkillCandidate,
    markdown: &str,
) -> CommandResult<()> {
    let metadata = frontmatter_metadata(markdown)?;
    if metadata.name != candidate.skill_id {
        return Err(CommandError::user_fixable(
            "skill_tool_mcp_skill_id_mismatch",
            format!(
                "Xero rejected MCP skill `{}` from server `{}` because SKILL.md declared `{}`.",
                candidate.skill_id, candidate.server_id, metadata.name
            ),
        ));
    }
    Ok(())
}

fn mcp_prompt_text(result: &JsonValue) -> CommandResult<String> {
    let Some(messages) = result.get("messages").and_then(JsonValue::as_array) else {
        return Err(CommandError::user_fixable(
            "skill_tool_mcp_prompt_invalid",
            "Xero expected MCP prompt skill invocation to return a messages array.",
        ));
    };
    let parts = messages
        .iter()
        .filter_map(|message| match message.get("content") {
            Some(JsonValue::String(text)) => Some(text.clone()),
            Some(JsonValue::Object(content)) => {
                if content
                    .get("type")
                    .and_then(JsonValue::as_str)
                    .map(|kind| kind == "text")
                    .unwrap_or(false)
                {
                    content
                        .get("text")
                        .and_then(JsonValue::as_str)
                        .map(ToOwned::to_owned)
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Err(CommandError::user_fixable(
            "skill_tool_mcp_prompt_invalid",
            "Xero rejected MCP prompt skill invocation because it returned no text content.",
        ));
    }
    Ok(parts.join("\n\n"))
}

fn synthesize_mcp_prompt_skill_markdown(
    candidate: &ResolvedMcpSkillCandidate,
    prompt_text: &str,
) -> String {
    let description = candidate.description.replace('\n', " ");
    format!(
        "---\nname: {}\ndescription: {}\n---\n\n# {}\n\n{}\n",
        candidate.skill_id,
        description.trim(),
        candidate.name,
        prompt_text.trim()
    )
}

fn persist_skill_failure(
    repo_root: &Path,
    candidate: &AutonomousSkillToolCandidate,
    error: &CommandError,
) {
    let Ok(Some(mut record)) =
        project_store::load_installed_skill_by_source_id(repo_root, &candidate.source_id)
    else {
        return;
    };
    record.source.state = XeroSkillSourceState::Failed;
    record.updated_at = now_timestamp();
    record.last_diagnostic = Some(InstalledSkillDiagnosticRecord {
        code: error.code.clone(),
        message: skill_tool_diagnostic_from_command_error(error).message,
        retryable: error.retryable,
        recorded_at: now_timestamp(),
    });
    let _ = project_store::upsert_installed_skill(repo_root, record);
}

#[derive(Debug, Clone)]
struct SkillFrontmatterLite {
    name: String,
    description: String,
    user_invocable: Option<bool>,
}

fn frontmatter_metadata(markdown: &str) -> CommandResult<SkillFrontmatterLite> {
    let mut lines = markdown.lines();
    if lines.next() != Some("---") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Xero requires dynamic SKILL.md files to start with frontmatter.",
        ));
    }
    let mut values = BTreeMap::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            values.insert(
                key.trim().to_owned(),
                value.trim().trim_matches('"').to_owned(),
            );
        }
    }
    let name = values
        .remove("name")
        .ok_or_else(|| CommandError::invalid_request("name"))?;
    let description = values
        .remove("description")
        .ok_or_else(|| CommandError::invalid_request("description"))?;
    let user_invocable = values.remove("user-invocable").map(|value| value == "true");
    Ok(SkillFrontmatterLite {
        name,
        description,
        user_invocable,
    })
}

fn context_hash(context: &XeroSkillToolContextPayload) -> String {
    let mut hasher = Sha256::new();
    hasher.update(context.markdown.sha256.as_bytes());
    for asset in &context.supporting_assets {
        hasher.update(asset.sha256.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sanitize_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}
