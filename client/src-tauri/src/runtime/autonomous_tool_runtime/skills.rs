use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::{
    AutonomousBundledSkillRoot, AutonomousLocalSkillRoot, AutonomousSkillToolCandidate,
    AutonomousSkillToolOutput, AutonomousSkillToolStatus, AutonomousToolOutput,
    AutonomousToolResult, AutonomousToolRuntime, AUTONOMOUS_TOOL_SKILL,
};
use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::project_store::{
        self, InstalledSkillDiagnosticRecord, InstalledSkillRecord, InstalledSkillScopeFilter,
    },
    runtime::autonomous_skill_runtime::{
        decide_skill_tool_access, discover_bundled_skill_directory, discover_local_skill_directory,
        discover_project_skill_directory, load_discovered_skill_context,
        load_skill_context_from_directory, skill_tool_diagnostic_from_command_error,
        validate_skill_tool_context_payload, AutonomousSkillInstallRequest,
        AutonomousSkillInvokeRequest, AutonomousSkillResolveOutput, AutonomousSkillResolveRequest,
        AutonomousSkillSourceMetadata, CadenceDiscoveredSkill, CadenceSkillDirectoryDiscovery,
        CadenceSkillSourceLocator, CadenceSkillSourceRecord, CadenceSkillSourceScope,
        CadenceSkillSourceState, CadenceSkillToolAccessStatus, CadenceSkillToolContextAsset,
        CadenceSkillToolContextDocument, CadenceSkillToolContextPayload,
        CadenceSkillToolDiagnostic, CadenceSkillToolDynamicAssetInput, CadenceSkillToolInput,
        CadenceSkillToolLifecycleEvent, CadenceSkillToolLifecycleResult, CadenceSkillToolOperation,
        CadenceSkillTrustState, CADENCE_SKILL_TOOL_CONTRACT_VERSION,
    },
};

#[derive(Debug, Clone)]
pub(super) enum CachedSkillToolCandidate {
    Installed(InstalledSkillRecord),
    Discovered(CadenceDiscoveredSkill),
    Github(ResolvedGithubSkillCandidate),
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedGithubSkillCandidate {
    pub source: CadenceSkillSourceRecord,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub github_source: AutonomousSkillSourceMetadata,
}

#[derive(Debug, Clone)]
struct SkillToolSelection {
    entry: CachedSkillToolCandidate,
    public: AutonomousSkillToolCandidate,
}

impl AutonomousToolRuntime {
    pub fn skill(&self, input: CadenceSkillToolInput) -> CommandResult<AutonomousToolResult> {
        let input = crate::runtime::autonomous_skill_runtime::validate_skill_tool_input(input)?;
        let operation = input.operation();
        let output = match input {
            CadenceSkillToolInput::List {
                query,
                include_unavailable,
                limit,
            } => self.skill_list(operation, query, include_unavailable, limit)?,
            CadenceSkillToolInput::Resolve {
                source_id,
                skill_id,
                include_unavailable,
            } => self.skill_resolve(operation, source_id, skill_id, include_unavailable)?,
            CadenceSkillToolInput::Install {
                source_id,
                approval_grant_id,
            } => self.skill_install(operation, &source_id, approval_grant_id.as_deref())?,
            CadenceSkillToolInput::Invoke {
                source_id,
                approval_grant_id,
                include_supporting_assets,
            } => self.skill_invoke(
                operation,
                &source_id,
                approval_grant_id.as_deref(),
                include_supporting_assets,
            )?,
            CadenceSkillToolInput::Reload {
                source_id,
                source_kind,
            } => self.skill_reload(operation, source_id, source_kind)?,
            CadenceSkillToolInput::CreateDynamic {
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
        operation: CadenceSkillToolOperation,
        query: Option<String>,
        include_unavailable: bool,
        limit: Option<usize>,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(skill_unavailable_output(operation));
        };
        let limit = limit.unwrap_or(crate::runtime::CADENCE_SKILL_TOOL_DEFAULT_LIMIT);
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
        operation: CadenceSkillToolOperation,
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
                let lifecycle = CadenceSkillToolLifecycleEvent::succeeded(
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
                    "Cadence could not find that skill in the durable registry or configured skill sources.",
                    false,
                )],
                truncated: false,
            }),
        }
    }

    fn skill_install(
        &self,
        operation: CadenceSkillToolOperation,
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
        if access.status == CadenceSkillToolAccessStatus::ApprovalRequired {
            return Ok(approval_required_output(
                operation,
                selection.public,
                access.reason,
            ));
        }
        if access.status == CadenceSkillToolAccessStatus::Denied {
            return Ok(denied_output(operation, selection.public, access.reason));
        }

        let result = match selection.entry.clone() {
            CachedSkillToolCandidate::Github(candidate) => {
                self.install_github_skill(skill_tool, &candidate.github_source)
            }
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
        operation: CadenceSkillToolOperation,
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
        if access.status == CadenceSkillToolAccessStatus::ApprovalRequired {
            return Ok(approval_required_output(
                operation,
                selection.public,
                access.reason,
            ));
        }
        if access.status == CadenceSkillToolAccessStatus::Denied {
            return Ok(denied_output(operation, selection.public, access.reason));
        }

        let result = match selection.entry.clone() {
            CachedSkillToolCandidate::Github(candidate) => self.invoke_github_skill(
                skill_tool,
                &candidate.github_source,
                include_supporting_assets,
            ),
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
        operation: CadenceSkillToolOperation,
        source_id: Option<String>,
        source_kind: Option<crate::runtime::CadenceSkillSourceKind>,
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
        let lifecycle = CadenceSkillToolLifecycleEvent::succeeded(
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
        operation: CadenceSkillToolOperation,
        skill_id: &str,
        markdown: &str,
        supporting_assets: Vec<CadenceSkillToolDynamicAssetInput>,
        source_run_id: Option<String>,
        source_artifact_id: Option<String>,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(skill_unavailable_output(operation));
        };
        let run_id = source_run_id.unwrap_or_else(|| "model-created".into());
        let artifact_id = source_artifact_id
            .unwrap_or_else(|| format!("dynamic-{}", &sha256_hex(markdown.as_bytes())[..12]));
        let source = CadenceSkillSourceRecord::new(
            CadenceSkillSourceScope::project(skill_tool.project_id.clone())?,
            CadenceSkillSourceLocator::Dynamic {
                run_id: run_id.clone(),
                artifact_id: artifact_id.clone(),
                skill_id: skill_id.to_owned(),
            },
            CadenceSkillSourceState::Disabled,
            CadenceSkillTrustState::Untrusted,
        )?;
        let directory_relative = PathBuf::from(".cadence")
            .join("dynamic-skills")
            .join(sanitize_path_segment(&run_id))
            .join(sanitize_path_segment(&artifact_id))
            .join(skill_id);
        let directory = self.resolve_writable_path(&directory_relative)?;
        let expected_dynamic_root = self.repo_root.join(".cadence").join("dynamic-skills");
        if !directory.starts_with(&expected_dynamic_root) {
            return Err(CommandError::user_fixable(
                "skill_tool_dynamic_path_denied",
                "Cadence requires dynamic skill files to remain inside .cadence/dynamic-skills.",
            ));
        }
        fs::create_dir_all(&directory).map_err(|error| {
            CommandError::retryable(
                "skill_tool_dynamic_write_failed",
                format!(
                    "Cadence could not create dynamic skill directory {}: {error}",
                    directory.display()
                ),
            )
        })?;
        self.write_dynamic_skill_files(
            &directory_relative,
            &directory,
            markdown,
            &supporting_assets,
        )?;
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
        let lifecycle = CadenceSkillToolLifecycleEvent::succeeded(
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

    fn collect_skill_tool_candidates(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        query: Option<&str>,
        include_unavailable: bool,
        limit: usize,
        operation: CadenceSkillToolOperation,
    ) -> CommandResult<SkillToolDiscoveryResult> {
        let mut entries = BTreeMap::<String, CachedSkillToolCandidate>::new();
        let mut diagnostics = Vec::new();

        for record in project_store::list_installed_skills(
            &self.repo_root,
            InstalledSkillScopeFilter::project(skill_tool.project_id.clone(), true)?,
        )? {
            insert_candidate(&mut entries, CachedSkillToolCandidate::Installed(record));
        }

        self.collect_project_skills(skill_tool, &mut entries, &mut diagnostics)?;
        self.collect_local_skills(skill_tool, &mut entries, &mut diagnostics)?;
        self.collect_bundled_skills(skill_tool, &mut entries, &mut diagnostics)?;
        if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
            self.collect_github_skills(skill_tool, query, &mut entries, &mut diagnostics)?;
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
        diagnostics: &mut Vec<CadenceSkillToolDiagnostic>,
    ) -> CommandResult<()> {
        if !skill_tool.project_skills_enabled {
            return Ok(());
        }
        let discovered = discover_project_skill_directory(&skill_tool.project_id, &self.repo_root)?;
        push_discovery(entries, diagnostics, discovered);
        Ok(())
    }

    fn collect_local_skills(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
        diagnostics: &mut Vec<CadenceSkillToolDiagnostic>,
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
        diagnostics: &mut Vec<CadenceSkillToolDiagnostic>,
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

    fn collect_github_skills(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        query: &str,
        entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
        diagnostics: &mut Vec<CadenceSkillToolDiagnostic>,
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

    fn cache_skill_candidates(&self, entries: &[CachedSkillToolCandidate]) -> CommandResult<()> {
        let Some(skill_tool) = self.skill_tool.as_ref() else {
            return Ok(());
        };
        let mut cache = skill_tool.discovery_cache.lock().map_err(|_| {
            CommandError::system_fault(
                "skill_tool_cache_lock_failed",
                "Cadence could not lock the SkillTool discovery cache.",
            )
        })?;
        for entry in entries {
            cache.insert(candidate_source_id(entry), entry.clone());
        }
        Ok(())
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
                "Cadence could not lock the SkillTool discovery cache.",
            )
        })?;
        Ok(cache.get(source_id).cloned())
    }

    fn find_skill_by_source_id(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        source_id: &str,
        include_unavailable: bool,
        operation: CadenceSkillToolOperation,
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
        Ok(discovered
            .entries
            .into_iter()
            .find(|entry| candidate_source_id(entry) == source_id)
            .map(|entry| {
                Ok(SkillToolSelection {
                    public: public_skill_candidate(&entry, operation)?,
                    entry,
                })
            })
            .transpose()?)
    }

    fn find_skill_by_skill_id(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        skill_id: &str,
        include_unavailable: bool,
        operation: CadenceSkillToolOperation,
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

    fn install_discovered_skill(
        &self,
        candidate: &CadenceDiscoveredSkill,
        approval_grant_id: Option<&str>,
    ) -> CommandResult<()> {
        let trust = effective_trust(candidate.source.trust, approval_grant_id);
        let record = InstalledSkillRecord::from_discovered_skill(
            candidate,
            CadenceSkillSourceState::Enabled,
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
            CadenceSkillSourceLocator::Github { .. } => {
                let source = record.source.locator.to_autonomous_github_source().ok_or_else(|| {
                    CommandError::user_fixable(
                        "skill_tool_source_unsupported",
                        "Cadence could not map this installed GitHub skill back to its source.",
                    )
                })?;
                self.install_github_skill(skill_tool, &source)
            }
            CadenceSkillSourceLocator::Dynamic { .. }
                if record.source.state != CadenceSkillSourceState::Enabled =>
            {
                Err(CommandError::user_fixable(
                    "skill_tool_dynamic_review_required",
                    "Dynamic skills must be explicitly reviewed and enabled before SkillTool can install or invoke them.",
                ))
            }
            _ => {
                if record.source.state != CadenceSkillSourceState::Enabled {
                    record.source.state = CadenceSkillSourceState::Enabled;
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
    ) -> CommandResult<CadenceSkillToolContextPayload> {
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
        let source_record = CadenceSkillSourceRecord::github_autonomous(
            CadenceSkillSourceScope::project(skill_tool.project_id.clone())?,
            &invoked.source,
            CadenceSkillSourceState::Enabled,
            CadenceSkillTrustState::Trusted,
        )?;
        skill_context_from_github_invocation(&source_record, invoked, include_supporting_assets)
    }

    fn invoke_discovered_skill(
        &self,
        candidate: &CadenceDiscoveredSkill,
        approval_grant_id: Option<&str>,
        include_supporting_assets: bool,
    ) -> CommandResult<CadenceSkillToolContextPayload> {
        self.install_discovered_skill(candidate, approval_grant_id)?;
        load_discovered_skill_context(candidate, include_supporting_assets)
    }

    fn invoke_installed_skill(
        &self,
        skill_tool: &super::AutonomousSkillToolRuntime,
        mut record: InstalledSkillRecord,
        approval_grant_id: Option<&str>,
        include_supporting_assets: bool,
    ) -> CommandResult<CadenceSkillToolContextPayload> {
        match &record.source.locator {
            CadenceSkillSourceLocator::Github { .. } => {
                let source = record.source.locator.to_autonomous_github_source().ok_or_else(|| {
                    CommandError::user_fixable(
                        "skill_tool_source_unsupported",
                        "Cadence could not map this installed GitHub skill back to its source.",
                    )
                })?;
                self.invoke_github_skill(skill_tool, &source, include_supporting_assets)
            }
            CadenceSkillSourceLocator::Dynamic { .. }
                if record.source.state != CadenceSkillSourceState::Enabled =>
            {
                Err(CommandError::user_fixable(
                    "skill_tool_dynamic_review_required",
                    "Dynamic skills must be explicitly reviewed and enabled before SkillTool can invoke them.",
                ))
            }
            _ => {
                if record.source.state != CadenceSkillSourceState::Enabled {
                    record.source.state = CadenceSkillSourceState::Enabled;
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
        operation: CadenceSkillToolOperation,
        public: AutonomousSkillToolCandidate,
        result: CommandResult<()>,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        match result {
            Ok(()) => {
                let selected = self
                    .refreshed_public_candidate(&public.source_id, operation)
                    .unwrap_or(public);
                let lifecycle = CadenceSkillToolLifecycleEvent::succeeded(
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
        operation: CadenceSkillToolOperation,
        public: AutonomousSkillToolCandidate,
        result: CommandResult<CadenceSkillToolContextPayload>,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        match result {
            Ok(context) => {
                let selected = self
                    .refreshed_public_candidate(&public.source_id, operation)
                    .unwrap_or(public);
                let lifecycle = CadenceSkillToolLifecycleEvent::succeeded(
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
        operation: CadenceSkillToolOperation,
        public: AutonomousSkillToolCandidate,
        detail: &str,
        error: CommandError,
    ) -> CommandResult<AutonomousSkillToolOutput> {
        persist_skill_failure(&self.repo_root, &public, &error);
        let lifecycle = CadenceSkillToolLifecycleEvent::failed(
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
        operation: CadenceSkillToolOperation,
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
        directory_relative: &Path,
        directory: &Path,
        markdown: &str,
        supporting_assets: &[CadenceSkillToolDynamicAssetInput],
    ) -> CommandResult<()> {
        let skill_path =
            self.dynamic_skill_write_path(directory_relative, directory, Path::new("SKILL.md"))?;
        fs::write(&skill_path, markdown).map_err(|error| {
            CommandError::retryable(
                "skill_tool_dynamic_write_failed",
                format!(
                    "Cadence could not write dynamic skill document {}: {error}",
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
                        "Cadence rejected dynamic skill asset `{}` because it was duplicated.",
                        asset.relative_path
                    ),
                ));
            }
            let path = self.dynamic_skill_write_path(
                directory_relative,
                directory,
                Path::new(&asset.relative_path),
            )?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    CommandError::retryable(
                        "skill_tool_dynamic_write_failed",
                        format!("Cadence could not create dynamic skill asset directory: {error}"),
                    )
                })?;
            }
            fs::write(&path, asset.content.as_bytes()).map_err(|error| {
                CommandError::retryable(
                    "skill_tool_dynamic_write_failed",
                    format!(
                        "Cadence could not write dynamic skill asset {}: {error}",
                        path.display()
                    ),
                )
            })?;
        }
        Ok(())
    }

    fn dynamic_skill_write_path(
        &self,
        directory_relative: &Path,
        directory: &Path,
        relative_path: &Path,
    ) -> CommandResult<PathBuf> {
        let path = self.resolve_writable_path(&directory_relative.join(relative_path))?;
        if path.starts_with(directory) {
            return Ok(path);
        }
        Err(CommandError::user_fixable(
            "skill_tool_dynamic_path_denied",
            "Cadence requires dynamic skill assets to remain inside their staged skill directory.",
        ))
    }
}

struct SkillToolDiscoveryResult {
    entries: Vec<CachedSkillToolCandidate>,
    diagnostics: Vec<CadenceSkillToolDiagnostic>,
    truncated: bool,
}

fn push_discovery(
    entries: &mut BTreeMap<String, CachedSkillToolCandidate>,
    diagnostics: &mut Vec<CadenceSkillToolDiagnostic>,
    discovered: CadenceSkillDirectoryDiscovery,
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
        CadenceSkillSourceState::Disabled
            | CadenceSkillSourceState::Failed
            | CadenceSkillSourceState::Blocked
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
    record.source.state = CadenceSkillSourceState::Stale;
    record.name = candidate_name(&discovered);
    record.description = candidate_description(&discovered);
    record.user_invocable = candidate_user_invocable(&discovered);
    record.local_location = candidate_local_location(&discovered).or(record.local_location);
    record.version_hash = Some(discovered_version);
    Ok(record)
}

fn public_skill_candidate(
    entry: &CachedSkillToolCandidate,
    operation: CadenceSkillToolOperation,
) -> CommandResult<AutonomousSkillToolCandidate> {
    let record = source_record_for_entry(entry)?;
    let access = decide_skill_tool_access(&record, operation)?;
    let state = record.state;
    let trust = record.trust;
    Ok(AutonomousSkillToolCandidate {
        source_id: record.source_id,
        skill_id: candidate_skill_id(entry),
        name: candidate_name(entry),
        description: candidate_description(entry),
        source_kind: record.locator.kind(),
        state,
        trust,
        enabled: state == CadenceSkillSourceState::Enabled,
        installed: matches!(entry, CachedSkillToolCandidate::Installed(_)),
        user_invocable: candidate_user_invocable(entry),
        version_hash: candidate_version_hash(entry),
        cache_key: candidate_cache_key(entry),
        access,
    })
}

fn source_record_for_entry(
    entry: &CachedSkillToolCandidate,
) -> CommandResult<CadenceSkillSourceRecord> {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.source.clone().validate(),
        CachedSkillToolCandidate::Discovered(candidate) => candidate.source.clone().validate(),
        CachedSkillToolCandidate::Github(candidate) => candidate.source.clone().validate(),
    }
}

fn candidate_source_id(entry: &CachedSkillToolCandidate) -> String {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.source.source_id.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => candidate.source.source_id.clone(),
        CachedSkillToolCandidate::Github(candidate) => candidate.source.source_id.clone(),
    }
}

fn candidate_skill_id(entry: &CachedSkillToolCandidate) -> String {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.skill_id.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => candidate.skill_id.clone(),
        CachedSkillToolCandidate::Github(candidate) => candidate.skill_id.clone(),
    }
}

fn candidate_name(entry: &CachedSkillToolCandidate) -> String {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.name.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => candidate.name.clone(),
        CachedSkillToolCandidate::Github(candidate) => candidate.name.clone(),
    }
}

fn candidate_description(entry: &CachedSkillToolCandidate) -> String {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.description.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => candidate.description.clone(),
        CachedSkillToolCandidate::Github(candidate) => candidate.description.clone(),
    }
}

fn candidate_user_invocable(entry: &CachedSkillToolCandidate) -> Option<bool> {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.user_invocable,
        CachedSkillToolCandidate::Discovered(candidate) => candidate.user_invocable,
        CachedSkillToolCandidate::Github(candidate) => candidate.user_invocable,
    }
}

fn candidate_version_hash(entry: &CachedSkillToolCandidate) -> Option<String> {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.version_hash.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => Some(candidate.version_hash.clone()),
        CachedSkillToolCandidate::Github(candidate) => {
            Some(candidate.github_source.tree_hash.clone())
        }
    }
}

fn candidate_cache_key(entry: &CachedSkillToolCandidate) -> Option<String> {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.cache_key.clone(),
        CachedSkillToolCandidate::Discovered(_) | CachedSkillToolCandidate::Github(_) => None,
    }
}

fn candidate_local_location(entry: &CachedSkillToolCandidate) -> Option<String> {
    match entry {
        CachedSkillToolCandidate::Installed(record) => record.local_location.clone(),
        CachedSkillToolCandidate::Discovered(candidate) => Some(candidate.local_location.clone()),
        CachedSkillToolCandidate::Github(_) => None,
    }
}

fn candidate_model_visible(entry: &CachedSkillToolCandidate) -> bool {
    source_record_for_entry(entry)
        .ok()
        .and_then(|record| decide_skill_tool_access(&record, CadenceSkillToolOperation::List).ok())
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
        .unwrap_or(CadenceSkillSourceState::Failed);
    let trust = source_record_for_entry(entry)
        .map(|record| record.trust)
        .unwrap_or(CadenceSkillTrustState::Blocked);
    (
        if exact { 0 } else { 1 },
        match state {
            CadenceSkillSourceState::Enabled => 0,
            CadenceSkillSourceState::Installed => 1,
            CadenceSkillSourceState::Discoverable => 2,
            CadenceSkillSourceState::Stale => 3,
            CadenceSkillSourceState::Disabled => 4,
            CadenceSkillSourceState::Failed => 5,
            CadenceSkillSourceState::Blocked => 6,
        },
        match trust {
            CadenceSkillTrustState::Trusted => 0,
            CadenceSkillTrustState::UserApproved => 1,
            CadenceSkillTrustState::ApprovalRequired => 2,
            CadenceSkillTrustState::Untrusted => 3,
            CadenceSkillTrustState::Blocked => 4,
        },
        match entry {
            CachedSkillToolCandidate::Discovered(candidate) => {
                match candidate.source.locator.kind() {
                    crate::runtime::CadenceSkillSourceKind::Bundled => 0,
                    crate::runtime::CadenceSkillSourceKind::Project => 1,
                    crate::runtime::CadenceSkillSourceKind::Local => 2,
                    _ => 3,
                }
            }
            CachedSkillToolCandidate::Github(_) => 3,
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
        CadenceSkillTrustState::Trusted
    } else {
        CadenceSkillTrustState::ApprovalRequired
    };
    let source = CadenceSkillSourceRecord::github_autonomous(
        CadenceSkillSourceScope::project(skill_tool.project_id.clone())?,
        &resolved.source,
        CadenceSkillSourceState::Discoverable,
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
    operation: CadenceSkillToolOperation,
    approval_grant_id: Option<&str>,
) -> CommandResult<crate::runtime::CadenceSkillToolAccessDecision> {
    let mut record = source_record_for_entry(entry)?;
    let effective_operation = if operation == CadenceSkillToolOperation::Invoke
        && (!matches!(entry, CachedSkillToolCandidate::Installed(_))
            || record.state == CadenceSkillSourceState::Stale)
    {
        CadenceSkillToolOperation::Install
    } else {
        operation
    };
    if approval_grant_id.is_some()
        && !matches!(
            record.trust,
            CadenceSkillTrustState::Blocked | CadenceSkillTrustState::Trusted
        )
    {
        record.trust = CadenceSkillTrustState::UserApproved;
    }
    decide_skill_tool_access(&record, effective_operation)
}

fn effective_trust(
    trust: CadenceSkillTrustState,
    approval_grant_id: Option<&str>,
) -> CadenceSkillTrustState {
    if approval_grant_id.is_some()
        && !matches!(
            trust,
            CadenceSkillTrustState::Blocked | CadenceSkillTrustState::Trusted
        )
    {
        CadenceSkillTrustState::UserApproved
    } else {
        trust
    }
}

fn load_installed_filesystem_context(
    record: &InstalledSkillRecord,
    include_supporting_assets: bool,
) -> CommandResult<CadenceSkillToolContextPayload> {
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
    source_record: &CadenceSkillSourceRecord,
    invoked: crate::runtime::AutonomousSkillInvokeOutput,
    include_supporting_assets: bool,
) -> CommandResult<CadenceSkillToolContextPayload> {
    let markdown_bytes = invoked.skill_markdown.as_bytes();
    let supporting_assets = if include_supporting_assets {
        invoked
            .supporting_assets
            .into_iter()
            .map(|asset| CadenceSkillToolContextAsset {
                relative_path: asset.relative_path,
                sha256: asset.sha256,
                bytes: asset.bytes,
                content: asset.content,
            })
            .collect()
    } else {
        Vec::new()
    };
    validate_skill_tool_context_payload(CadenceSkillToolContextPayload {
        contract_version: CADENCE_SKILL_TOOL_CONTRACT_VERSION,
        source_id: source_record.source_id.clone(),
        skill_id: invoked.skill_id,
        markdown: CadenceSkillToolContextDocument {
            relative_path: "SKILL.md".into(),
            sha256: sha256_hex(markdown_bytes),
            bytes: markdown_bytes.len(),
            content: invoked.skill_markdown,
        },
        supporting_assets,
    })
}

fn skill_unavailable_output(operation: CadenceSkillToolOperation) -> AutonomousSkillToolOutput {
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
            "Cadence did not configure skill support for this owned-agent run.",
            false,
        )],
        truncated: false,
    }
}

fn skill_not_found_output(
    operation: CadenceSkillToolOperation,
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
            "Cadence could not find that source in the durable registry, configured sources, or current discovery cache.",
            false,
        )],
        truncated: false,
    }
}

fn approval_required_output(
    operation: CadenceSkillToolOperation,
    candidate: AutonomousSkillToolCandidate,
    reason: Option<CadenceSkillToolDiagnostic>,
) -> AutonomousSkillToolOutput {
    let diagnostic = reason.unwrap_or_else(|| {
        diagnostic(
            "skill_tool_user_approval_required",
            "Cadence requires user approval before this skill source can be installed or invoked.",
            false,
        )
    });
    let lifecycle = CadenceSkillToolLifecycleEvent {
        contract_version: CADENCE_SKILL_TOOL_CONTRACT_VERSION,
        operation,
        result: CadenceSkillToolLifecycleResult::ApprovalRequired,
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
    operation: CadenceSkillToolOperation,
    candidate: AutonomousSkillToolCandidate,
    reason: Option<CadenceSkillToolDiagnostic>,
) -> AutonomousSkillToolOutput {
    let diagnostic = reason.unwrap_or_else(|| {
        diagnostic(
            "skill_tool_source_denied",
            "Cadence denied this SkillTool operation for the selected source.",
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
) -> CadenceSkillToolDiagnostic {
    CadenceSkillToolDiagnostic {
        code: code.into(),
        message: message.into(),
        retryable,
        redacted: false,
    }
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
    record.source.state = CadenceSkillSourceState::Failed;
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
            "Cadence requires dynamic SKILL.md files to start with frontmatter.",
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

fn context_hash(context: &CadenceSkillToolContextPayload) -> String {
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
