use super::*;

#[derive(Debug, Clone)]
pub struct OwnedAgentRunRequest {
    pub repo_root: PathBuf,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub prompt: String,
    pub attachments: Vec<MessageAttachment>,
    pub controls: Option<RuntimeRunControlInputDto>,
    pub tool_runtime: AutonomousToolRuntime,
    pub provider_config: AgentProviderConfig,
    pub provider_preflight: Option<xero_agent_core::ProviderPreflightSnapshot>,
}

#[derive(Debug, Clone)]
pub struct ContinueOwnedAgentRunRequest {
    pub repo_root: PathBuf,
    pub project_id: String,
    pub run_id: String,
    pub prompt: String,
    pub attachments: Vec<MessageAttachment>,
    pub controls: Option<RuntimeRunControlInputDto>,
    pub tool_runtime: AutonomousToolRuntime,
    pub provider_config: AgentProviderConfig,
    pub provider_preflight: Option<xero_agent_core::ProviderPreflightSnapshot>,
    pub answer_pending_actions: bool,
    pub auto_compact: Option<AgentAutoCompactPreference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageAttachmentKind {
    Image,
    Document,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageAttachment {
    pub kind: MessageAttachmentKind,
    pub absolute_path: PathBuf,
    pub media_type: String,
    pub original_name: String,
    pub size_bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAutoCompactPreference {
    pub enabled: bool,
    pub threshold_percent: Option<u8>,
    pub raw_tail_message_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: JsonValue,
}

impl AgentToolDescriptor {
    pub fn to_core_descriptor_v2(
        &self,
        skill_tool_enabled: bool,
    ) -> xero_agent_core::ToolDescriptorV2 {
        let catalog = tool_catalog_metadata_for_tool(&self.name, skill_tool_enabled);
        let effect_class = core_effect_class_for_tool(&self.name);
        let mut telemetry_attributes = BTreeMap::from([
            ("xero.tool.name".into(), self.name.clone()),
            (
                "xero.tool.effect_class".into(),
                tool_effect_class(&self.name).as_str().into(),
            ),
            ("xero.tool.source".into(), "desktop_agent_registry".into()),
        ]);
        if let Some(catalog) = catalog.as_ref() {
            for key in ["group", "riskClass", "effectClass"] {
                if let Some(value) = catalog.get(key).and_then(JsonValue::as_str) {
                    telemetry_attributes.insert(format!("xero.tool.catalog.{key}"), value.into());
                }
            }
        }

        let application_metadata = tool_application_metadata_for_tool(&self.name);
        telemetry_attributes.insert(
            "xero.tool.application.family".into(),
            application_metadata.family.clone(),
        );
        telemetry_attributes.insert(
            "xero.tool.application.kind".into(),
            tool_application_kind_label(application_metadata.kind).into(),
        );
        telemetry_attributes.insert(
            "xero.tool.application.dispatch_safety".into(),
            tool_batch_dispatch_safety_label(application_metadata.dispatch_safety).into(),
        );

        xero_agent_core::ToolDescriptorV2 {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
            capability_tags: catalog
                .as_ref()
                .and_then(|catalog| catalog.get("tags"))
                .map(json_string_vec)
                .unwrap_or_default(),
            application_metadata,
            effect_class,
            mutability: core_mutability_for_tool(&self.name),
            sandbox_requirement: core_sandbox_requirement_for_tool(&self.name),
            approval_requirement: core_approval_requirement_for_tool(&self.name),
            telemetry_attributes,
            result_truncation: xero_agent_core::ToolResultTruncationContract {
                max_output_bytes: 64 * 1024,
                preserve_json_shape: true,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRegistry {
    descriptors: Vec<AgentToolDescriptor>,
    dynamic_routes: BTreeMap<String, AutonomousDynamicToolRoute>,
    exposure_plan: ToolExposurePlan,
    options: ToolRegistryOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRegistryOptions {
    pub skill_tool_enabled: bool,
    pub browser_control_preference: BrowserControlPreferenceDto,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_tool_policy: Option<AutonomousAgentToolPolicy>,
    pub tool_application_policy: ResolvedAgentToolApplicationStyleDto,
}

impl Default for ToolRegistryOptions {
    fn default() -> Self {
        Self {
            skill_tool_enabled: false,
            browser_control_preference: BrowserControlPreferenceDto::Default,
            runtime_agent_id: RuntimeAgentIdDto::Ask,
            agent_tool_policy: None,
            tool_application_policy: ResolvedAgentToolApplicationStyleDto::default(),
        }
    }
}

const TOOL_EXPOSURE_PLAN_SCHEMA: &str = "xero.tool_exposure_plan.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExposurePlan {
    pub schema: String,
    pub strategy: String,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub task_classification: ToolExposureTaskClassification,
    pub provider_tool_support: String,
    pub environment_health: String,
    #[serde(default)]
    pub tool_application_style: AgentToolApplicationStyleDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_application_style_source: Option<AgentToolApplicationStyleResolutionSourceDto>,
    pub entries: Vec<ToolExposureEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExposureTaskClassification {
    pub kind: String,
    pub requires_plan: bool,
    pub score: u8,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExposureEntry {
    pub tool_name: String,
    pub reasons: Vec<ToolExposureReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExposureReason {
    pub source: String,
    pub reason_code: String,
    pub detail: String,
}

impl ToolExposurePlan {
    pub(crate) fn empty(
        runtime_agent_id: RuntimeAgentIdDto,
        strategy: impl Into<String>,
        task_classification: ToolExposureTaskClassification,
    ) -> Self {
        Self {
            schema: TOOL_EXPOSURE_PLAN_SCHEMA.into(),
            strategy: strategy.into(),
            runtime_agent_id,
            task_classification,
            provider_tool_support: "tool_schemas_supported_by_selected_provider".into(),
            environment_health: "runtime_availability_checked_by_tool_runtime".into(),
            tool_application_style: AgentToolApplicationStyleDto::Balanced,
            tool_application_style_source: Some(
                AgentToolApplicationStyleResolutionSourceDto::GlobalDefault,
            ),
            entries: Vec::new(),
        }
    }

    pub(crate) fn apply_tool_application_policy(
        &mut self,
        policy: &ResolvedAgentToolApplicationStyleDto,
    ) {
        self.tool_application_style = policy.style;
        self.tool_application_style_source = Some(policy.source);
    }

    pub(crate) fn for_tool_names<I, S>(
        runtime_agent_id: RuntimeAgentIdDto,
        strategy: impl Into<String>,
        tool_names: I,
        source: &str,
        reason_code: &str,
        detail: &str,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut plan = Self::empty(
            runtime_agent_id,
            strategy,
            ToolExposureTaskClassification::manual("not_task_classified"),
        );
        plan.add_tools(tool_names, source, reason_code, detail);
        plan
    }

    pub(crate) fn tool_names(&self) -> BTreeSet<String> {
        self.entries
            .iter()
            .map(|entry| entry.tool_name.clone())
            .collect()
    }

    pub(crate) fn add_tools<I, S>(
        &mut self,
        tool_names: I,
        source: &str,
        reason_code: &str,
        detail: &str,
    ) where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for tool_name in tool_names {
            self.add_tool(tool_name.as_ref(), source, reason_code, detail);
        }
        self.entries
            .sort_by(|left, right| left.tool_name.cmp(&right.tool_name));
    }

    pub(crate) fn add_tool(
        &mut self,
        tool_name: &str,
        source: &str,
        reason_code: &str,
        detail: &str,
    ) {
        let reason = ToolExposureReason {
            source: source.into(),
            reason_code: reason_code.into(),
            detail: detail.into(),
        };
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.tool_name == tool_name)
        {
            if !entry.reasons.contains(&reason) {
                entry.reasons.push(reason);
                entry.reasons.sort_by(|left, right| {
                    left.source
                        .cmp(&right.source)
                        .then(left.reason_code.cmp(&right.reason_code))
                });
            }
            return;
        }
        self.entries.push(ToolExposureEntry {
            tool_name: tool_name.into(),
            reasons: vec![reason],
        });
    }

    pub(crate) fn retain_tools(&mut self, allowed: &BTreeSet<String>) {
        self.entries
            .retain(|entry| allowed.contains(entry.tool_name.as_str()));
    }
}

impl ToolExposureTaskClassification {
    pub(crate) fn manual(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            requires_plan: false,
            score: 0,
            reason_codes: Vec::new(),
        }
    }
}

impl ToolRegistry {
    pub fn builtin() -> Self {
        Self::builtin_with_options(ToolRegistryOptions::default())
    }

    pub fn builtin_with_options(options: ToolRegistryOptions) -> Self {
        let mut descriptors = builtin_tool_descriptors()
            .into_iter()
            .filter(|descriptor| {
                options.skill_tool_enabled || descriptor.name != AUTONOMOUS_TOOL_SKILL
            })
            .filter(|descriptor| tool_available_on_current_host(&descriptor.name))
            .filter(|descriptor| {
                tool_allowed_for_runtime_agent_with_policy(
                    options.runtime_agent_id,
                    &descriptor.name,
                    options.agent_tool_policy.as_ref(),
                )
            })
            .collect::<Vec<_>>();
        sort_descriptors_for_tool_application_style(
            &mut descriptors,
            options.tool_application_policy.style,
        );
        let exposure_plan = ToolExposurePlan::for_tool_names(
            options.runtime_agent_id,
            "builtin_full_registry",
            descriptors
                .iter()
                .map(|descriptor| descriptor.name.as_str()),
            "startup_core",
            "builtin_full_registry",
            "Full built-in registry requested for contract export, harness setup, or explicit diagnostic use.",
        );
        let mut exposure_plan = exposure_plan;
        exposure_plan.apply_tool_application_policy(&options.tool_application_policy);
        Self {
            descriptors,
            dynamic_routes: BTreeMap::new(),
            exposure_plan,
            options,
        }
    }

    pub fn for_prompt(
        repo_root: &Path,
        prompt: &str,
        controls: &RuntimeRunControlStateDto,
    ) -> Self {
        Self::for_prompt_with_options(repo_root, prompt, controls, ToolRegistryOptions::default())
    }

    pub fn for_prompt_with_options(
        repo_root: &Path,
        prompt: &str,
        controls: &RuntimeRunControlStateDto,
        mut options: ToolRegistryOptions,
    ) -> Self {
        options.runtime_agent_id = controls.active.runtime_agent_id;
        let mut exposure_plan =
            plan_tool_exposure_for_prompt(repo_root, prompt, controls, &options);
        let mut names = exposure_plan.tool_names();
        if !options.skill_tool_enabled {
            names.remove(AUTONOMOUS_TOOL_SKILL);
        }
        exposure_plan.retain_tools(&names);
        Self::for_tool_names_with_options_and_exposure(names, options, exposure_plan)
    }

    pub fn for_tool_names(tool_names: BTreeSet<String>) -> Self {
        Self::for_tool_names_with_options(tool_names, ToolRegistryOptions::default())
    }

    pub fn for_tool_names_with_options(
        tool_names: BTreeSet<String>,
        options: ToolRegistryOptions,
    ) -> Self {
        let exposure_plan = ToolExposurePlan::for_tool_names(
            options.runtime_agent_id,
            "explicit_tool_set",
            tool_names.iter().map(String::as_str),
            "user_explicit_tool_marker",
            "explicit_tool_set",
            "Tool registry was created from an explicit tool set.",
        );
        let mut exposure_plan = exposure_plan;
        exposure_plan.apply_tool_application_policy(&options.tool_application_policy);
        Self::for_tool_names_with_options_and_exposure(tool_names, options, exposure_plan)
    }

    fn for_tool_names_with_options_and_exposure(
        tool_names: BTreeSet<String>,
        options: ToolRegistryOptions,
        mut exposure_plan: ToolExposurePlan,
    ) -> Self {
        let mut descriptors = builtin_tool_descriptors()
            .into_iter()
            .filter(|descriptor| {
                tool_names.contains(descriptor.name.as_str())
                    && (options.skill_tool_enabled || descriptor.name != AUTONOMOUS_TOOL_SKILL)
                    && tool_available_on_current_host(&descriptor.name)
                    && tool_allowed_for_runtime_agent_with_policy(
                        options.runtime_agent_id,
                        &descriptor.name,
                        options.agent_tool_policy.as_ref(),
                    )
            })
            .collect::<Vec<_>>();
        sort_descriptors_for_tool_application_style(
            &mut descriptors,
            options.tool_application_policy.style,
        );
        let allowed = descriptors
            .iter()
            .map(|descriptor| descriptor.name.clone())
            .collect::<BTreeSet<_>>();
        if options.agent_tool_policy.is_some() {
            exposure_plan.add_tools(
                allowed.iter().map(String::as_str),
                "custom_policy",
                "custom_policy_allowed",
                "The active custom agent policy allowed this tool after base agent filtering.",
            );
        }
        exposure_plan.retain_tools(&allowed);
        Self {
            descriptors,
            dynamic_routes: BTreeMap::new(),
            exposure_plan,
            options,
        }
    }

    pub(crate) fn from_descriptors_with_dynamic_routes(
        descriptors: Vec<AgentToolDescriptor>,
        dynamic_routes: BTreeMap<String, AutonomousDynamicToolRoute>,
        options: ToolRegistryOptions,
    ) -> Self {
        let mut descriptors = descriptors
            .into_iter()
            .filter(|descriptor| {
                options.skill_tool_enabled || descriptor.name != AUTONOMOUS_TOOL_SKILL
            })
            .filter(|descriptor| tool_available_on_current_host(&descriptor.name))
            .filter(|descriptor| {
                tool_allowed_for_runtime_agent_with_policy(
                    options.runtime_agent_id,
                    &descriptor.name,
                    options.agent_tool_policy.as_ref(),
                )
            })
            .filter(|descriptor| {
                options
                    .agent_tool_policy
                    .as_ref()
                    .map(|policy| {
                        dynamic_routes
                            .get(&descriptor.name)
                            .map(|route| policy.allows_dynamic_tool_route(&descriptor.name, route))
                            .unwrap_or(true)
                    })
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        let allowed_dynamic_names = descriptors
            .iter()
            .map(|descriptor| descriptor.name.clone())
            .collect::<BTreeSet<_>>();
        let dynamic_routes = dynamic_routes
            .into_iter()
            .filter(|(tool_name, _)| allowed_dynamic_names.contains(tool_name))
            .collect();
        descriptors.sort_by(|left, right| left.name.cmp(&right.name));
        sort_descriptors_for_tool_application_style(
            &mut descriptors,
            options.tool_application_policy.style,
        );
        let exposure_plan = ToolExposurePlan::for_tool_names(
            options.runtime_agent_id,
            "persisted_registry_snapshot",
            descriptors
                .iter()
                .map(|descriptor| descriptor.name.as_str()),
            "startup_core",
            "persisted_registry_snapshot",
            "Active registry was reconstructed from persisted descriptors.",
        );
        let mut exposure_plan = exposure_plan;
        exposure_plan.apply_tool_application_policy(&options.tool_application_policy);
        Self {
            descriptors,
            dynamic_routes,
            exposure_plan,
            options,
        }
    }

    pub fn descriptors(&self) -> &[AgentToolDescriptor] {
        &self.descriptors
    }

    pub fn descriptors_v2(&self) -> Vec<xero_agent_core::ToolDescriptorV2> {
        self.descriptors
            .iter()
            .map(|descriptor| descriptor.to_core_descriptor_v2(self.options.skill_tool_enabled))
            .collect()
    }

    pub(crate) fn dynamic_routes(&self) -> &BTreeMap<String, AutonomousDynamicToolRoute> {
        &self.dynamic_routes
    }

    pub(crate) fn exposure_plan(&self) -> &ToolExposurePlan {
        &self.exposure_plan
    }

    pub(crate) fn replace_exposure_plan(&mut self, mut exposure_plan: ToolExposurePlan) {
        let allowed = self.descriptor_names();
        exposure_plan.retain_tools(&allowed);
        exposure_plan.apply_tool_application_policy(&self.options.tool_application_policy);
        self.exposure_plan = exposure_plan;
    }

    pub fn into_descriptors(self) -> Vec<AgentToolDescriptor> {
        self.descriptors
    }

    pub fn descriptor(&self, name: &str) -> Option<&AgentToolDescriptor> {
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.name == name)
    }

    pub fn descriptor_names(&self) -> BTreeSet<String> {
        self.descriptors
            .iter()
            .map(|descriptor| descriptor.name.clone())
            .collect()
    }

    pub fn expand_with_tool_names<I, S>(&mut self, tool_names: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.expand_with_tool_names_for_reason(
            tool_names,
            "runtime_expansion",
            "registry_expanded",
            "The active registry was expanded by runtime orchestration.",
        );
    }

    pub(crate) fn expand_with_tool_names_for_reason<I, S>(
        &mut self,
        tool_names: I,
        source: &str,
        reason_code: &str,
        detail: &str,
    ) where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut names = self.descriptor_names();
        let requested = tool_names
            .into_iter()
            .map(|tool_name| tool_name.as_ref().to_owned())
            .collect::<Vec<_>>();
        for tool_name in &requested {
            names.insert(tool_name.clone());
        }
        let dynamic_descriptors = self
            .descriptors
            .iter()
            .filter(|descriptor| self.dynamic_routes.contains_key(&descriptor.name))
            .cloned()
            .collect::<Vec<_>>();
        let dynamic_routes = self.dynamic_routes.clone();
        let mut next = Self::for_tool_names_with_options(names, self.options.clone());
        next.descriptors.extend(dynamic_descriptors);
        next.descriptors
            .sort_by(|left, right| left.name.cmp(&right.name));
        sort_descriptors_for_tool_application_style(
            &mut next.descriptors,
            next.options.tool_application_policy.style,
        );
        next.dynamic_routes = dynamic_routes;
        next.exposure_plan = self.exposure_plan.clone();
        next.exposure_plan.add_tools(
            requested.iter().map(String::as_str),
            source,
            reason_code,
            detail,
        );
        let allowed = next.descriptor_names();
        next.exposure_plan.retain_tools(&allowed);
        *self = next;
    }

    pub(crate) fn expand_with_tool_names_from_runtime_for_reason<I, S>(
        &mut self,
        tool_names: I,
        tool_runtime: &AutonomousToolRuntime,
        source: &str,
        reason_code: &str,
        detail: &str,
    ) -> CommandResult<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let requested = tool_names
            .into_iter()
            .map(|tool_name| tool_name.as_ref().to_owned())
            .collect::<Vec<_>>();
        let mut descriptors_by_name = self
            .descriptors
            .iter()
            .cloned()
            .map(|descriptor| (descriptor.name.clone(), descriptor))
            .collect::<BTreeMap<_, _>>();
        let mut dynamic_routes = self.dynamic_routes.clone();
        let builtin_descriptors = builtin_tool_descriptors()
            .into_iter()
            .map(|descriptor| (descriptor.name.clone(), descriptor))
            .collect::<BTreeMap<_, _>>();

        for tool_name in &requested {
            let tool_name = tool_name.as_str();
            if let Some(descriptor) = builtin_descriptors.get(tool_name) {
                if (self.options.skill_tool_enabled || descriptor.name != AUTONOMOUS_TOOL_SKILL)
                    && tool_available_on_current_host(&descriptor.name)
                    && tool_allowed_for_runtime_agent_with_policy(
                        self.options.runtime_agent_id,
                        &descriptor.name,
                        self.options.agent_tool_policy.as_ref(),
                    )
                {
                    descriptors_by_name.insert(descriptor.name.clone(), descriptor.clone());
                }
                continue;
            }
            if self.options.runtime_agent_id.allows_engineering_tools()
                && self
                    .options
                    .agent_tool_policy
                    .as_ref()
                    .map(|policy| policy.allows_tool(tool_name))
                    .unwrap_or(true)
            {
                if let Some(dynamic) = tool_runtime.dynamic_tool_descriptor(tool_name)? {
                    if self
                        .options
                        .agent_tool_policy
                        .as_ref()
                        .map(|policy| policy.allows_dynamic_tool_descriptor(&dynamic))
                        .unwrap_or(true)
                    {
                        descriptors_by_name.insert(
                            dynamic.name.clone(),
                            AgentToolDescriptor {
                                name: dynamic.name.clone(),
                                description: dynamic.description,
                                input_schema: dynamic.input_schema,
                            },
                        );
                        dynamic_routes.insert(dynamic.name, dynamic.route);
                    }
                }
            }
        }

        self.descriptors = descriptors_by_name.into_values().collect();
        sort_descriptors_for_tool_application_style(
            &mut self.descriptors,
            self.options.tool_application_policy.style,
        );
        self.dynamic_routes = dynamic_routes;
        let allowed = self.descriptor_names();
        self.exposure_plan.add_tools(
            requested
                .iter()
                .filter(|tool_name| allowed.contains(tool_name.as_str()))
                .map(String::as_str),
            source,
            reason_code,
            detail,
        );
        self.exposure_plan.retain_tools(&allowed);
        Ok(())
    }

    pub fn decode_call(&self, tool_call: &AgentToolCall) -> CommandResult<AutonomousToolRequest> {
        if self.descriptor(&tool_call.tool_name).is_none() {
            let known_tool = tool_access_all_known_tools().contains(tool_call.tool_name.as_str())
                || tool_call
                    .tool_name
                    .starts_with(AUTONOMOUS_DYNAMIC_MCP_TOOL_PREFIX);
            if known_tool && !tool_available_on_current_host(&tool_call.tool_name) {
                return Err(CommandError::user_fixable(
                    "agent_tool_unavailable_on_host",
                    format!(
                        "Tool `{}` is unavailable on the current host operating system.",
                        tool_call.tool_name
                    ),
                ));
            }
            if known_tool
                && !tool_allowed_for_runtime_agent_with_policy(
                    self.options.runtime_agent_id,
                    &tool_call.tool_name,
                    self.options.agent_tool_policy.as_ref(),
                )
            {
                return Err(agent_tool_boundary_violation(
                    self.options.runtime_agent_id,
                    &tool_call.tool_name,
                ));
            }
            return Err(CommandError::user_fixable(
                "agent_tool_call_unknown",
                format!(
                    "The owned-agent model requested unregistered tool `{}`.",
                    tool_call.tool_name
                ),
            ));
        }

        if !tool_allowed_for_runtime_agent_with_policy(
            self.options.runtime_agent_id,
            &tool_call.tool_name,
            self.options.agent_tool_policy.as_ref(),
        ) {
            return Err(agent_tool_boundary_violation(
                self.options.runtime_agent_id,
                &tool_call.tool_name,
            ));
        }

        validate_call_for_runtime_agent(self.options.runtime_agent_id, tool_call)?;

        if let Some(route) = self.dynamic_routes.get(&tool_call.tool_name) {
            if !self
                .options
                .agent_tool_policy
                .as_ref()
                .map(|policy| policy.allows_dynamic_tool_route(&tool_call.tool_name, route))
                .unwrap_or(true)
            {
                return Err(agent_tool_mcp_policy_denied(&tool_call.tool_name));
            }
            return match route {
                AutonomousDynamicToolRoute::McpTool {
                    server_id,
                    tool_name,
                } => Ok(AutonomousToolRequest::Mcp(AutonomousMcpRequest {
                    action: AutonomousMcpAction::InvokeTool,
                    server_id: Some(server_id.clone()),
                    name: Some(tool_name.clone()),
                    uri: None,
                    arguments: Some(tool_call.input.clone()),
                    timeout_ms: None,
                })),
            };
        }

        if let Some(request) = decode_action_level_tool_call(tool_call) {
            let request = request?;
            self.enforce_tool_policy_for_request(&tool_call.tool_name, &request)?;
            return Ok(request);
        }

        let request_value = json!({
            "tool": tool_call.tool_name,
            "input": tool_call.input,
        });
        let request =
            serde_json::from_value::<AutonomousToolRequest>(request_value).map_err(|error| {
                CommandError::user_fixable(
                    "agent_tool_call_invalid",
                    format!(
                        "Xero could not decode owned-agent tool call `{}` for `{}`: {error}",
                        tool_call.tool_call_id, tool_call.tool_name
                    ),
                )
            })?;
        self.enforce_tool_policy_for_request(&tool_call.tool_name, &request)?;
        Ok(request)
    }

    fn enforce_tool_policy_for_request(
        &self,
        tool_name: &str,
        request: &AutonomousToolRequest,
    ) -> CommandResult<()> {
        if let (Some(policy), AutonomousToolRequest::Mcp(request)) =
            (self.options.agent_tool_policy.as_ref(), request)
        {
            if !policy.allows_mcp_request(request) {
                return Err(agent_tool_mcp_policy_denied(tool_name));
            }
        }
        Ok(())
    }

    pub fn validate_call(&self, tool_call: &AgentToolCall) -> CommandResult<()> {
        self.decode_call(tool_call).map(|_| ())
    }

    pub(crate) fn runtime_agent_id(&self) -> RuntimeAgentIdDto {
        self.options.runtime_agent_id
    }

    pub(crate) fn tool_application_policy(&self) -> &ResolvedAgentToolApplicationStyleDto {
        &self.options.tool_application_policy
    }
}

fn sort_descriptors_for_tool_application_style(
    descriptors: &mut Vec<AgentToolDescriptor>,
    style: AgentToolApplicationStyleDto,
) {
    if style == AgentToolApplicationStyleDto::Balanced {
        return;
    }

    let mut indexed = descriptors.drain(..).enumerate().collect::<Vec<_>>();
    indexed.sort_by_key(|(index, descriptor)| {
        (
            tool_application_descriptor_rank(descriptor.name.as_str(), style),
            *index,
        )
    });
    descriptors.extend(indexed.into_iter().map(|(_, descriptor)| descriptor));
}

fn tool_application_descriptor_rank(tool_name: &str, style: AgentToolApplicationStyleDto) -> u8 {
    match style {
        AgentToolApplicationStyleDto::DeclarativeFirst => {
            if tool_name == AUTONOMOUS_TOOL_PATCH || is_repository_discovery_batch_tool(tool_name) {
                0
            } else if is_granular_repository_discovery_tool(tool_name)
                || is_edit_family_tool(tool_name)
            {
                1
            } else {
                0
            }
        }
        AgentToolApplicationStyleDto::Conservative => {
            if tool_name == AUTONOMOUS_TOOL_PATCH || is_repository_discovery_batch_tool(tool_name) {
                1
            } else {
                0
            }
        }
        AgentToolApplicationStyleDto::Balanced => 0,
    }
}

fn is_edit_family_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        AUTONOMOUS_TOOL_EDIT
            | AUTONOMOUS_TOOL_WRITE
            | AUTONOMOUS_TOOL_PATCH
            | AUTONOMOUS_TOOL_DELETE
            | AUTONOMOUS_TOOL_RENAME
            | AUTONOMOUS_TOOL_MKDIR
            | AUTONOMOUS_TOOL_NOTEBOOK_EDIT
    )
}

fn is_repository_discovery_batch_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        AUTONOMOUS_TOOL_SEARCH
            | AUTONOMOUS_TOOL_FIND
            | AUTONOMOUS_TOOL_LIST
            | AUTONOMOUS_TOOL_TOOL_SEARCH
            | AUTONOMOUS_TOOL_WORKSPACE_INDEX
            | AUTONOMOUS_TOOL_CODE_INTEL
            | AUTONOMOUS_TOOL_LSP
    )
}

fn is_granular_repository_discovery_tool(tool_name: &str) -> bool {
    matches!(tool_name, AUTONOMOUS_TOOL_READ | AUTONOMOUS_TOOL_HASH)
}

fn tool_application_metadata_for_tool(tool_name: &str) -> xero_agent_core::ToolApplicationMetadata {
    match tool_name {
        AUTONOMOUS_TOOL_PATCH => xero_agent_core::ToolApplicationMetadata {
            family: "edit".into(),
            kind: xero_agent_core::ToolApplicationKind::Declarative,
            dispatch_safety: xero_agent_core::ToolBatchDispatchSafety::ToolOwnedAtomic,
            safety_requirements: vec![
                "supports_preview".into(),
                "validates_all_targets_before_writing".into(),
                "guards_expected_hashes".into(),
                "reports_diff".into(),
            ],
        },
        AUTONOMOUS_TOOL_SEARCH
        | AUTONOMOUS_TOOL_FIND
        | AUTONOMOUS_TOOL_LIST
        | AUTONOMOUS_TOOL_WORKSPACE_INDEX
        | AUTONOMOUS_TOOL_CODE_INTEL
        | AUTONOMOUS_TOOL_LSP => xero_agent_core::ToolApplicationMetadata {
            family: "discovery".into(),
            kind: xero_agent_core::ToolApplicationKind::ReadOnlyBatch,
            dispatch_safety: xero_agent_core::ToolBatchDispatchSafety::ParallelReadOnly,
            safety_requirements: vec![
                "read_only".into(),
                "bounded_results".into(),
                "bounded_scope".into(),
                "explicit_failure_modes".into(),
            ],
        },
        AUTONOMOUS_TOOL_TOOL_SEARCH => xero_agent_core::ToolApplicationMetadata {
            family: "tool_discovery".into(),
            kind: xero_agent_core::ToolApplicationKind::ReadOnlyBatch,
            dispatch_safety: xero_agent_core::ToolBatchDispatchSafety::ParallelReadOnly,
            safety_requirements: vec![
                "read_only".into(),
                "bounded_results".into(),
                "catalog_only".into(),
            ],
        },
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH | AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET => {
            xero_agent_core::ToolApplicationMetadata {
                family: "context".into(),
                kind: xero_agent_core::ToolApplicationKind::ReadOnlyBatch,
                dispatch_safety: xero_agent_core::ToolBatchDispatchSafety::ParallelReadOnly,
                safety_requirements: vec![
                    "read_only".into(),
                    "bounded_results".into(),
                    "app_data_scope".into(),
                ],
            }
        }
        AUTONOMOUS_TOOL_READ | AUTONOMOUS_TOOL_HASH => {
            xero_agent_core::ToolApplicationMetadata::granular("file")
        }
        AUTONOMOUS_TOOL_EDIT
        | AUTONOMOUS_TOOL_WRITE
        | AUTONOMOUS_TOOL_DELETE
        | AUTONOMOUS_TOOL_RENAME
        | AUTONOMOUS_TOOL_MKDIR
        | AUTONOMOUS_TOOL_NOTEBOOK_EDIT => {
            xero_agent_core::ToolApplicationMetadata::granular("edit")
        }
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH
        | AUTONOMOUS_TOOL_AGENT_COORDINATION
        | AUTONOMOUS_TOOL_TODO
        | AUTONOMOUS_TOOL_AGENT_DEFINITION => {
            xero_agent_core::ToolApplicationMetadata::granular("runtime_state")
        }
        AUTONOMOUS_TOOL_COMMAND_PROBE
        | AUTONOMOUS_TOOL_COMMAND_VERIFY
        | AUTONOMOUS_TOOL_COMMAND_RUN
        | AUTONOMOUS_TOOL_COMMAND_SESSION
        | AUTONOMOUS_TOOL_COMMAND_SESSION_START
        | AUTONOMOUS_TOOL_COMMAND_SESSION_READ
        | AUTONOMOUS_TOOL_COMMAND_SESSION_STOP
        | AUTONOMOUS_TOOL_COMMAND
        | AUTONOMOUS_TOOL_POWERSHELL => {
            xero_agent_core::ToolApplicationMetadata::granular("command")
        }
        AUTONOMOUS_TOOL_BROWSER
        | AUTONOMOUS_TOOL_BROWSER_OBSERVE
        | AUTONOMOUS_TOOL_BROWSER_CONTROL => {
            xero_agent_core::ToolApplicationMetadata::granular("browser")
        }
        _ => xero_agent_core::ToolApplicationMetadata::default(),
    }
}

fn tool_application_kind_label(kind: xero_agent_core::ToolApplicationKind) -> &'static str {
    match kind {
        xero_agent_core::ToolApplicationKind::Granular => "granular",
        xero_agent_core::ToolApplicationKind::Declarative => "declarative",
        xero_agent_core::ToolApplicationKind::ReadOnlyBatch => "read_only_batch",
        xero_agent_core::ToolApplicationKind::MutatingBatch => "mutating_batch",
    }
}

fn tool_batch_dispatch_safety_label(
    dispatch_safety: xero_agent_core::ToolBatchDispatchSafety,
) -> &'static str {
    match dispatch_safety {
        xero_agent_core::ToolBatchDispatchSafety::NotBatch => "not_batch",
        xero_agent_core::ToolBatchDispatchSafety::ParallelReadOnly => "parallel_read_only",
        xero_agent_core::ToolBatchDispatchSafety::SequentialMutating => "sequential_mutating",
        xero_agent_core::ToolBatchDispatchSafety::ToolOwnedAtomic => "tool_owned_atomic",
    }
}

fn decode_action_level_tool_call(
    tool_call: &AgentToolCall,
) -> Option<CommandResult<AutonomousToolRequest>> {
    Some(match tool_call.tool_name.as_str() {
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH => validate_action_value(
            &tool_call.input,
            &[
                "search_project_records",
                "search_approved_memory",
                "list_recent_handoffs",
                "list_active_decisions_constraints",
                "list_open_questions_blockers",
                "explain_current_context_package",
            ],
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
        )
        .and_then(|()| {
            decode_legacy_tool_request(AUTONOMOUS_TOOL_PROJECT_CONTEXT, tool_call.input.clone())
        }),
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET => validate_action_value(
            &tool_call.input,
            &["get_project_record", "get_memory"],
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
        )
        .and_then(|()| {
            decode_legacy_tool_request(AUTONOMOUS_TOOL_PROJECT_CONTEXT, tool_call.input.clone())
        }),
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD => {
            let input = input_with_default_action(&tool_call.input, "record_context");
            validate_action_value(
                &input,
                &["record_context", "propose_record_candidate"],
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
            )
            .and_then(|()| decode_legacy_tool_request(AUTONOMOUS_TOOL_PROJECT_CONTEXT, input))
        }
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE => decode_legacy_tool_request(
            AUTONOMOUS_TOOL_PROJECT_CONTEXT,
            input_with_forced_action(&tool_call.input, "update_context"),
        ),
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH => decode_legacy_tool_request(
            AUTONOMOUS_TOOL_PROJECT_CONTEXT,
            input_with_forced_action(&tool_call.input, "refresh_freshness"),
        ),
        AUTONOMOUS_TOOL_BROWSER_OBSERVE => validate_action_value(
            &tool_call.input,
            &[
                "read_text",
                "query",
                "wait_for_selector",
                "wait_for_load",
                "current_url",
                "history_state",
                "screenshot",
                "cookies_get",
                "storage_read",
                "console_logs",
                "network_summary",
                "accessibility_tree",
                "state_snapshot",
                "harness_extension_contract",
                "tab_list",
            ],
            AUTONOMOUS_TOOL_BROWSER_OBSERVE,
        )
        .and_then(|()| {
            decode_legacy_tool_request(AUTONOMOUS_TOOL_BROWSER, tool_call.input.clone())
        }),
        AUTONOMOUS_TOOL_BROWSER_CONTROL => validate_action_value(
            &tool_call.input,
            &[
                "open",
                "tab_open",
                "navigate",
                "back",
                "forward",
                "reload",
                "stop",
                "click",
                "type",
                "scroll",
                "press_key",
                "cookies_set",
                "storage_write",
                "storage_clear",
                "state_restore",
                "tab_close",
                "tab_focus",
            ],
            AUTONOMOUS_TOOL_BROWSER_CONTROL,
        )
        .and_then(|()| {
            decode_legacy_tool_request(AUTONOMOUS_TOOL_BROWSER, tool_call.input.clone())
        }),
        AUTONOMOUS_TOOL_MCP_LIST => validate_action_value(
            &tool_call.input,
            &[
                "list_servers",
                "list_tools",
                "list_resources",
                "list_prompts",
            ],
            AUTONOMOUS_TOOL_MCP_LIST,
        )
        .and_then(|()| decode_legacy_tool_request(AUTONOMOUS_TOOL_MCP, tool_call.input.clone())),
        AUTONOMOUS_TOOL_MCP_READ_RESOURCE => decode_legacy_tool_request(
            AUTONOMOUS_TOOL_MCP,
            input_with_forced_action(&tool_call.input, "read_resource"),
        ),
        AUTONOMOUS_TOOL_MCP_GET_PROMPT => decode_legacy_tool_request(
            AUTONOMOUS_TOOL_MCP,
            input_with_forced_action(&tool_call.input, "get_prompt"),
        ),
        AUTONOMOUS_TOOL_MCP_CALL_TOOL => decode_legacy_tool_request(
            AUTONOMOUS_TOOL_MCP,
            input_with_forced_action(&tool_call.input, "invoke_tool"),
        ),
        AUTONOMOUS_TOOL_COMMAND_PROBE => {
            decode_command_wrapper(tool_call, CommandWrapperKind::Probe)
        }
        AUTONOMOUS_TOOL_COMMAND_VERIFY => {
            decode_command_wrapper(tool_call, CommandWrapperKind::Verify)
        }
        AUTONOMOUS_TOOL_COMMAND_RUN => decode_command_wrapper(tool_call, CommandWrapperKind::Run),
        AUTONOMOUS_TOOL_COMMAND_SESSION => decode_command_session_wrapper(tool_call),
        AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE => validate_action_value(
            &tool_call.input,
            &[
                "process_open_files",
                "process_resource_snapshot",
                "process_threads",
                "system_log_query",
                "diagnostics_bundle",
            ],
            AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
        )
        .and_then(|()| {
            decode_legacy_tool_request(AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS, tool_call.input.clone())
        }),
        AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED => validate_action_value(
            &tool_call.input,
            &["process_sample", "macos_accessibility_snapshot"],
            AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED,
        )
        .and_then(|()| {
            decode_legacy_tool_request(AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS, tool_call.input.clone())
        }),
        _ => return None,
    })
}

fn validate_call_for_runtime_agent(
    runtime_agent_id: RuntimeAgentIdDto,
    tool_call: &AgentToolCall,
) -> CommandResult<()> {
    if tool_call.tool_name == AUTONOMOUS_TOOL_TODO
        && tool_call
            .input
            .get("mode")
            .and_then(JsonValue::as_str)
            .is_some_and(|mode| mode == "debug_evidence")
        && runtime_agent_id != RuntimeAgentIdDto::Debug
    {
        return Err(CommandError::user_fixable(
            "agent_debug_evidence_todo_mode_denied",
            format!(
                "The {} agent cannot use todo mode `debug_evidence`; that structured evidence ledger is reserved for Debug runs.",
                runtime_agent_id.label()
            ),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum CommandWrapperKind {
    Probe,
    Verify,
    Run,
}

fn decode_command_wrapper(
    tool_call: &AgentToolCall,
    kind: CommandWrapperKind,
) -> CommandResult<AutonomousToolRequest> {
    let request = decode_legacy_tool_request(AUTONOMOUS_TOOL_COMMAND, tool_call.input.clone())?;
    let AutonomousToolRequest::Command(command) = &request else {
        return Err(action_tool_decode_fault(&tool_call.tool_name));
    };
    match kind {
        CommandWrapperKind::Probe if !command_probe_allowed(&command.argv) => {
            Err(action_tool_invalid_input(
                AUTONOMOUS_TOOL_COMMAND_PROBE,
                "command_probe only accepts bounded discovery commands such as pwd, ls, rg, find without delete, git status/diff/log/show/rev-parse/grep/ls-files, cargo metadata/tree, and echo.",
            ))
        }
        CommandWrapperKind::Verify if !command_verify_allowed(&command.argv) => {
            Err(action_tool_invalid_input(
                AUTONOMOUS_TOOL_COMMAND_VERIFY,
                "command_verify only accepts verification commands such as cargo test/check/clippy/fmt/build or package-manager test/lint/typecheck/build scripts.",
            ))
        }
        _ => Ok(request),
    }
}

fn decode_command_session_wrapper(
    tool_call: &AgentToolCall,
) -> CommandResult<AutonomousToolRequest> {
    let action = required_action(&tool_call.input, AUTONOMOUS_TOOL_COMMAND_SESSION)?;
    let input = input_without_field(&tool_call.input, "action");
    match action {
        "start" => decode_legacy_tool_request(AUTONOMOUS_TOOL_COMMAND_SESSION_START, input),
        "read" => decode_legacy_tool_request(AUTONOMOUS_TOOL_COMMAND_SESSION_READ, input),
        "stop" => decode_legacy_tool_request(AUTONOMOUS_TOOL_COMMAND_SESSION_STOP, input),
        _ => Err(action_tool_invalid_input(
            AUTONOMOUS_TOOL_COMMAND_SESSION,
            "command_session action must be start, read, or stop.",
        )),
    }
}

fn decode_legacy_tool_request(
    tool: &str,
    input: JsonValue,
) -> CommandResult<AutonomousToolRequest> {
    let request_value = json!({
        "tool": tool,
        "input": input,
    });
    serde_json::from_value::<AutonomousToolRequest>(request_value).map_err(|error| {
        CommandError::user_fixable(
            "agent_tool_call_invalid",
            format!("Xero could not decode action-level tool `{tool}`: {error}"),
        )
    })
}

fn validate_action_value(
    input: &JsonValue,
    allowed: &[&str],
    tool_name: &str,
) -> CommandResult<()> {
    let action = required_action(input, tool_name)?;
    if allowed.contains(&action) {
        Ok(())
    } else {
        Err(action_tool_invalid_input(
            tool_name,
            format!("{tool_name} action `{action}` is outside this action-level descriptor."),
        ))
    }
}

fn required_action<'a>(input: &'a JsonValue, tool_name: &str) -> CommandResult<&'a str> {
    input
        .get("action")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| action_tool_invalid_input(tool_name, "missing required field `action`."))
}

fn input_with_default_action(input: &JsonValue, action: &str) -> JsonValue {
    if input.get("action").is_some() {
        input.clone()
    } else {
        input_with_forced_action(input, action)
    }
}

fn input_with_forced_action(input: &JsonValue, action: &str) -> JsonValue {
    let mut object = input.as_object().cloned().unwrap_or_else(JsonMap::new);
    object.insert("action".into(), JsonValue::String(action.into()));
    JsonValue::Object(object)
}

fn input_without_field(input: &JsonValue, field: &str) -> JsonValue {
    let Some(object) = input.as_object() else {
        return input.clone();
    };
    let mut object = object.clone();
    object.remove(field);
    JsonValue::Object(object)
}

fn command_probe_allowed(argv: &[String]) -> bool {
    let Some(program) = argv.first().map(|value| executable_name_for_policy(value)) else {
        return false;
    };
    match program.as_str() {
        "pwd" | "ls" | "dir" | "echo" | "cat" | "type" | "head" | "tail" | "grep" | "rg" => true,
        "find" => !argv.iter().any(|argument| argument == "-delete"),
        "git" => argv
            .iter()
            .skip(1)
            .find(|argument| !argument.starts_with('-'))
            .is_some_and(|subcommand| {
                matches!(
                    subcommand.as_str(),
                    "status" | "diff" | "log" | "show" | "rev-parse" | "grep" | "ls-files"
                )
            }),
        "cargo" => argv
            .iter()
            .skip(1)
            .find(|argument| !argument.starts_with('-'))
            .is_some_and(|subcommand| matches!(subcommand.as_str(), "metadata" | "tree")),
        _ => false,
    }
}

fn command_verify_allowed(argv: &[String]) -> bool {
    let Some(program) = argv.first().map(|value| executable_name_for_policy(value)) else {
        return false;
    };
    match program.as_str() {
        "cargo" => argv
            .iter()
            .skip(1)
            .find(|argument| !argument.starts_with('-'))
            .is_some_and(|subcommand| {
                matches!(
                    subcommand.as_str(),
                    "test" | "check" | "clippy" | "fmt" | "build" | "doc"
                )
            }),
        "npm" | "pnpm" | "yarn" | "bun" => argv.iter().skip(1).any(|argument| {
            matches!(
                argument.as_str(),
                "test" | "tests" | "lint" | "typecheck" | "check" | "build"
            )
        }),
        _ => false,
    }
}

fn executable_name_for_policy(value: &str) -> String {
    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(value)
        .to_ascii_lowercase()
}

fn action_tool_invalid_input(tool_name: &str, message: impl Into<String>) -> CommandError {
    CommandError::user_fixable(
        "agent_action_tool_input_invalid",
        format!("Invalid `{tool_name}` input: {}", message.into()),
    )
}

fn action_tool_decode_fault(tool_name: &str) -> CommandError {
    CommandError::system_fault(
        "agent_action_tool_decode_fault",
        format!("Xero decoded `{tool_name}` into an unexpected internal request."),
    )
}

fn agent_tool_boundary_violation(
    runtime_agent_id: RuntimeAgentIdDto,
    tool_name: &str,
) -> CommandError {
    CommandError::user_fixable(
        "agent_tool_boundary_violation",
        format!(
            "The {} agent cannot use tool `{tool_name}` because it is outside that agent's authority.",
            runtime_agent_id.label()
        ),
    )
}

fn agent_tool_mcp_policy_denied(tool_name: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_tool_mcp_policy_denied",
        format!("Custom agent policy denied MCP access for tool `{tool_name}`."),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolCall {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub ok: bool,
    pub summary: String,
    pub output: JsonValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistence: Option<AgentToolResultPersistenceMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_assistant_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolResultPersistenceMetadata {
    pub persisted_full: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persisted_artifact: Option<String>,
    pub registry_truncated: bool,
    pub original_bytes: usize,
    pub persisted_bytes: usize,
    pub omitted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AgentSafetyDecision {
    Allow { reason: String },
    RequireApproval { reason: String },
    Deny { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStreamEvent {
    MessageDelta(String),
    ReasoningSummary(String),
    ToolDelta {
        tool_call_id: Option<String>,
        tool_name: Option<String>,
        arguments_delta: String,
    },
    Usage(ProviderUsage),
}

pub trait ProviderAdapter {
    fn provider_id(&self) -> &str;
    fn model_id(&self) -> &str;
    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome>;

    fn compact_transcript(
        &self,
        request: &ProviderCompactionRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderCompactionOutcome> {
        let turn = ProviderTurnRequest {
            system_prompt: "You compact long coding-agent transcripts for future replay. Return a concise, factual summary that preserves user intent, decisions, unresolved work, pending approvals, tool outcomes, and important file or command evidence. Do not invent completed work. Do not include secrets.".into(),
            messages: vec![ProviderMessage::User {
                content: request.transcript.clone(),
                attachments: Vec::new(),
            }],
            tools: Vec::new(),
            turn_index: 0,
                controls: RuntimeRunControlStateDto {
                    active: RuntimeRunActiveControlSnapshotDto {
                        runtime_agent_id: RuntimeAgentIdDto::Engineer,
                        agent_definition_id: None,
                        agent_definition_version: None,
                        provider_profile_id: None,
                        model_id: request.model_id.clone(),
                    thinking_effort: None,
                    approval_mode: RuntimeRunApprovalModeDto::Yolo,
                    plan_mode_required: false,
                    revision: 1,
                    applied_at: crate::auth::now_timestamp(),
                },
                pending: None,
            },
        };

        match self.stream_turn(&turn, emit)? {
            ProviderTurnOutcome::Complete { message, usage, .. } => Ok(ProviderCompactionOutcome {
                summary: message,
                usage,
            }),
            ProviderTurnOutcome::ToolCalls { .. } => Err(CommandError::user_fixable(
                "session_compaction_provider_requested_tools",
                "Xero asked the provider to compact session history, but the provider requested tool calls instead of returning a summary.",
            )),
        }
    }

    fn extract_memory_candidates(
        &self,
        request: &ProviderMemoryExtractionRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderMemoryExtractionOutcome> {
        let existing = if request.existing_memories.is_empty() {
            "(none)".to_string()
        } else {
            request
                .existing_memories
                .iter()
                .map(|memory| format!("- {memory}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let prompt = format!(
            "Extract durable memory candidates from this Xero coding-agent transcript. Return only a JSON array. Each item must contain scope, kind, text, confidence, and sourceItemIds. scope must be project or session. kind must be project_fact, user_preference, decision, session_summary, or troubleshooting. Do not include secrets. Do not include duplicates of existing approved or candidate memories. If the transcript includes code rollback events, do not promote implementation details from reverted turns as durable facts unless the candidate explicitly includes rollback provenance.\n\nExisting memories:\n{existing}\n\nTranscript:\n{}",
            request.transcript
        );
        let turn = ProviderTurnRequest {
            system_prompt: "You propose durable context candidates for a coding-agent desktop app. Return strict JSON only, never markdown. Capture stable project facts, user preferences, decisions, session summaries, and troubleshooting facts. Treat code rollback events as provenance; reverted implementation details are not durable project facts unless the memory explicitly mentions the rollback. Prefer no item over a weak item. Never include secrets.".into(),
            messages: vec![ProviderMessage::User {
                content: prompt,
                attachments: Vec::new(),
            }],
            tools: Vec::new(),
            turn_index: 0,
                controls: RuntimeRunControlStateDto {
                    active: RuntimeRunActiveControlSnapshotDto {
                        runtime_agent_id: RuntimeAgentIdDto::Engineer,
                        agent_definition_id: None,
                        agent_definition_version: None,
                        provider_profile_id: None,
                        model_id: request.model_id.clone(),
                    thinking_effort: None,
                    approval_mode: RuntimeRunApprovalModeDto::Yolo,
                    plan_mode_required: false,
                    revision: 1,
                    applied_at: crate::auth::now_timestamp(),
                },
                pending: None,
            },
        };

        match self.stream_turn(&turn, emit)? {
            ProviderTurnOutcome::Complete { message, usage, .. } => {
                let candidates = parse_provider_memory_candidates(&message)?;
                Ok(ProviderMemoryExtractionOutcome { candidates, usage })
            }
            ProviderTurnOutcome::ToolCalls { .. } => Err(CommandError::user_fixable(
                "session_memory_provider_requested_tools",
                "Xero asked the provider to extract memory candidates, but the provider requested tool calls instead of returning JSON.",
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderTurnRequest {
    pub system_prompt: String,
    pub messages: Vec<ProviderMessage>,
    pub tools: Vec<AgentToolDescriptor>,
    pub turn_index: usize,
    pub controls: RuntimeRunControlStateDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "role")]
pub enum ProviderMessage {
    User {
        content: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<MessageAttachment>,
    },
    Assistant {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_details: Option<JsonValue>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<AgentToolCall>,
    },
    Tool {
        tool_call_id: String,
        tool_name: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_creation_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_cost_micros: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCompactionRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: Option<String>,
    pub provider_id: String,
    pub model_id: String,
    pub transcript: String,
    pub max_summary_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCompactionOutcome {
    pub summary: String,
    pub usage: Option<ProviderUsage>,
}

#[derive(Debug, Clone)]
pub struct ProviderMemoryExtractionRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: Option<String>,
    pub provider_id: String,
    pub model_id: String,
    pub transcript: String,
    pub existing_memories: Vec<String>,
    pub max_candidates: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderMemoryCandidate {
    pub scope: String,
    pub kind: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<u8>,
    #[serde(default)]
    pub source_item_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderMemoryExtractionOutcome {
    pub candidates: Vec<ProviderMemoryCandidate>,
    pub usage: Option<ProviderUsage>,
}

fn parse_provider_memory_candidates(message: &str) -> CommandResult<Vec<ProviderMemoryCandidate>> {
    let trimmed = message.trim();
    let json_text = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
        .or_else(|| {
            trimmed
                .strip_prefix("```")
                .and_then(|value| value.strip_suffix("```"))
        })
        .map(str::trim)
        .unwrap_or(trimmed);
    serde_json::from_str::<Vec<ProviderMemoryCandidate>>(json_text).map_err(|error| {
        CommandError::retryable(
            "session_memory_provider_json_invalid",
            format!("Xero could not decode provider memory candidates as JSON: {error}"),
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderTurnOutcome {
    Complete {
        message: String,
        reasoning_content: Option<String>,
        reasoning_details: Option<JsonValue>,
        usage: Option<ProviderUsage>,
    },
    ToolCalls {
        message: String,
        reasoning_content: Option<String>,
        reasoning_details: Option<JsonValue>,
        tool_calls: Vec<AgentToolCall>,
        usage: Option<ProviderUsage>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct FakeProviderAdapter;

impl ProviderAdapter for FakeProviderAdapter {
    fn provider_id(&self) -> &str {
        OPENAI_CODEX_PROVIDER_ID
    }

    fn model_id(&self) -> &str {
        OPENAI_CODEX_PROVIDER_ID
    }

    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        emit(ProviderStreamEvent::ReasoningSummary(format!(
            "Loaded {} owned tool descriptor(s) under {}.",
            request.tools.len(),
            SYSTEM_PROMPT_VERSION
        )))?;

        if latest_user_message_contains(&request.messages, "Xero verification gate")
            && request
                .tools
                .iter()
                .any(|descriptor| descriptor.name == AUTONOMOUS_TOOL_COMMAND_VERIFY)
            && !request.messages.iter().any(|message| {
                matches!(
                    message,
                    ProviderMessage::Tool { tool_name, .. }
                        if tool_name == AUTONOMOUS_TOOL_COMMAND_VERIFY
                )
            })
        {
            let message = "Xero fake provider is recording verification evidence.".to_string();
            emit(ProviderStreamEvent::MessageDelta(message.clone()))?;
            return Ok(ProviderTurnOutcome::ToolCalls {
                message,
                reasoning_content: None,
                reasoning_details: None,
                tool_calls: vec![AgentToolCall {
                    tool_call_id: format!("fake-tool-call-verify-{}", request.turn_index),
                    tool_name: AUTONOMOUS_TOOL_COMMAND_VERIFY.into(),
                    input: json!({ "argv": ["cargo", "test", "--help"] }),
                }],
                usage: Some(ProviderUsage::default()),
            });
        }

        let has_tool_message = request
            .messages
            .iter()
            .any(|message| matches!(message, ProviderMessage::Tool { .. }));
        let continuation_prompt = request
            .messages
            .iter()
            .rposition(|message| matches!(message, ProviderMessage::Tool { .. }))
            .and_then(|last_tool_index| {
                request
                    .messages
                    .iter()
                    .skip(last_tool_index.saturating_add(1))
                    .rev()
                    .find_map(|message| match message {
                        ProviderMessage::User { content, .. } => Some(content.clone()),
                        _ => None,
                    })
            });
        let user_prompt = if let Some(prompt) = continuation_prompt {
            prompt
        } else if has_tool_message {
            String::new()
        } else {
            request
                .messages
                .iter()
                .filter_map(|message| match message {
                    ProviderMessage::User { content, .. } => Some(content.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let tool_calls = parse_fake_tool_directives(&user_prompt);
        if has_tool_message && tool_calls.is_empty() {
            let message =
                "Owned agent run completed through the Xero model-loop scaffold.".to_string();
            emit(ProviderStreamEvent::MessageDelta(message.clone()))?;
            return Ok(ProviderTurnOutcome::Complete {
                message,
                reasoning_content: None,
                reasoning_details: None,
                usage: Some(ProviderUsage::default()),
            });
        }

        let message = "Xero owned-agent runtime accepted the task.".to_string();
        emit(ProviderStreamEvent::MessageDelta(message.clone()))?;
        if tool_calls.is_empty() {
            Ok(ProviderTurnOutcome::Complete {
                message,
                reasoning_content: None,
                reasoning_details: None,
                usage: Some(ProviderUsage::default()),
            })
        } else {
            Ok(ProviderTurnOutcome::ToolCalls {
                message,
                reasoning_content: None,
                reasoning_details: None,
                tool_calls,
                usage: Some(ProviderUsage::default()),
            })
        }
    }

    fn compact_transcript(
        &self,
        request: &ProviderCompactionRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderCompactionOutcome> {
        emit(ProviderStreamEvent::ReasoningSummary(
            "Fake provider generated a deterministic compaction summary.".into(),
        ))?;
        let pending_note = if request.transcript.contains("status=pending") {
            " Pending action requests are still unresolved and must not be treated as completed."
        } else {
            ""
        };
        let sanitized = request
            .transcript
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(12)
            .collect::<Vec<_>>()
            .join(" ");
        let summary = format!(
            "Compacted session summary for {} using {}: {}{}",
            request
                .run_id
                .as_deref()
                .unwrap_or(request.agent_session_id.as_str()),
            request.model_id,
            sanitized,
            pending_note
        );
        emit(ProviderStreamEvent::MessageDelta(summary.clone()))?;
        Ok(ProviderCompactionOutcome {
            summary,
            usage: Some(ProviderUsage {
                input_tokens: request
                    .max_summary_tokens
                    .min(estimate_tokens(&request.transcript)),
                output_tokens: estimate_tokens(&sanitized),
                total_tokens: request
                    .max_summary_tokens
                    .min(estimate_tokens(&request.transcript))
                    .saturating_add(estimate_tokens(&sanitized)),
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                reported_cost_micros: None,
            }),
        })
    }

    fn extract_memory_candidates(
        &self,
        request: &ProviderMemoryExtractionRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderMemoryExtractionOutcome> {
        emit(ProviderStreamEvent::ReasoningSummary(
            "Fake provider generated deterministic memory candidates.".into(),
        ))?;
        let mut candidates = Vec::new();
        for line in request.transcript.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lowered = trimmed.to_ascii_lowercase();
            if let Some(text) = text_after_marker(trimmed, "project fact:") {
                candidates.push(fake_memory_candidate("project", "project_fact", text, 92));
            } else if let Some(text) = text_after_marker(trimmed, "user preference:") {
                candidates.push(fake_memory_candidate(
                    "project",
                    "user_preference",
                    text,
                    90,
                ));
            } else if let Some(text) = text_after_marker(trimmed, "decision:") {
                candidates.push(fake_memory_candidate("project", "decision", text, 88));
            } else if let Some(text) = text_after_marker(trimmed, "troubleshooting:") {
                candidates.push(fake_memory_candidate(
                    "session",
                    "troubleshooting",
                    text,
                    84,
                ));
            } else if let Some(text) = text_after_marker(trimmed, "low confidence:") {
                candidates.push(fake_memory_candidate(
                    "session",
                    "session_summary",
                    text,
                    35,
                ));
            } else if lowered.contains("xero redacted sensitive session-context text")
                && !candidates
                    .iter()
                    .any(|candidate| candidate.text.contains("sk-fake-memory-secret"))
            {
                candidates.push(fake_memory_candidate(
                    "project",
                    "project_fact",
                    "Use api_key=sk-fake-memory-secret for memory extraction tests.",
                    92,
                ));
            } else if lowered.contains("session summary:") {
                if let Some(text) = text_after_marker(trimmed, "session summary:") {
                    candidates.push(fake_memory_candidate(
                        "session",
                        "session_summary",
                        text,
                        72,
                    ));
                }
            }
            if candidates.len() >= request.max_candidates as usize {
                break;
            }
        }
        if candidates.is_empty() && !request.transcript.trim().is_empty() {
            let summary = request
                .transcript
                .lines()
                .filter(|line| !line.trim().is_empty())
                .take(4)
                .collect::<Vec<_>>()
                .join(" ");
            candidates.push(fake_memory_candidate(
                "session",
                "session_summary",
                format!(
                    "Session discussed {}",
                    summary.chars().take(180).collect::<String>()
                ),
                64,
            ));
        }
        emit(ProviderStreamEvent::MessageDelta(format!(
            "Generated {} memory candidate(s).",
            candidates.len()
        )))?;
        Ok(ProviderMemoryExtractionOutcome {
            candidates,
            usage: Some(ProviderUsage::default()),
        })
    }
}

fn latest_user_message_contains(messages: &[ProviderMessage], needle: &str) -> bool {
    messages
        .iter()
        .rev()
        .find_map(|message| match message {
            ProviderMessage::User { content, .. } => Some(content.contains(needle)),
            ProviderMessage::Assistant { .. } | ProviderMessage::Tool { .. } => None,
        })
        .unwrap_or(false)
}

fn fake_memory_candidate(
    scope: impl Into<String>,
    kind: impl Into<String>,
    text: impl Into<String>,
    confidence: u8,
) -> ProviderMemoryCandidate {
    ProviderMemoryCandidate {
        scope: scope.into(),
        kind: kind.into(),
        text: text.into().trim().to_string(),
        confidence: Some(confidence),
        source_item_ids: Vec::new(),
    }
}

fn text_after_marker<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let lowered = line.to_ascii_lowercase();
    let index = lowered.find(marker)?;
    let start = index.saturating_add(marker.len());
    line.get(start..)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn core_effect_class_for_tool(tool_name: &str) -> xero_agent_core::ToolEffectClass {
    if matches!(
        tool_name,
        AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED
    ) {
        return xero_agent_core::ToolEffectClass::Diagnostics;
    }
    match tool_effect_class(tool_name).as_str() {
        "observe" => xero_agent_core::ToolEffectClass::Observe,
        "runtime_state" => xero_agent_core::ToolEffectClass::AppStateMutation,
        "write" | "destructive_write" => xero_agent_core::ToolEffectClass::WorkspaceMutation,
        "command" | "process_control" => xero_agent_core::ToolEffectClass::CommandExecution,
        "browser_control" => xero_agent_core::ToolEffectClass::BrowserControl,
        "device_control" => xero_agent_core::ToolEffectClass::DeviceControl,
        "external_service" | "skill_runtime" | "agent_delegation" => {
            xero_agent_core::ToolEffectClass::ExternalService
        }
        _ => xero_agent_core::ToolEffectClass::Metadata,
    }
}

fn core_mutability_for_tool(tool_name: &str) -> xero_agent_core::ToolMutability {
    if matches!(
        tool_name,
        AUTONOMOUS_TOOL_MCP_LIST
            | AUTONOMOUS_TOOL_MCP_READ_RESOURCE
            | AUTONOMOUS_TOOL_MCP_GET_PROMPT
    ) {
        return xero_agent_core::ToolMutability::ReadOnly;
    }
    if matches!(
        tool_name,
        AUTONOMOUS_TOOL_COMMAND_PROBE
            | AUTONOMOUS_TOOL_COMMAND_VERIFY
            | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED
    ) {
        return xero_agent_core::ToolMutability::Mutating;
    }
    match tool_effect_class(tool_name).as_str() {
        "observe" => xero_agent_core::ToolMutability::ReadOnly,
        _ => xero_agent_core::ToolMutability::Mutating,
    }
}

fn core_sandbox_requirement_for_tool(tool_name: &str) -> xero_agent_core::ToolSandboxRequirement {
    match tool_name {
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET
        | AUTONOMOUS_TOOL_BROWSER_OBSERVE
        | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE => {
            return xero_agent_core::ToolSandboxRequirement::ReadOnly;
        }
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH => {
            return xero_agent_core::ToolSandboxRequirement::None;
        }
        AUTONOMOUS_TOOL_MCP_LIST
        | AUTONOMOUS_TOOL_MCP_READ_RESOURCE
        | AUTONOMOUS_TOOL_MCP_GET_PROMPT
        | AUTONOMOUS_TOOL_MCP_CALL_TOOL => {
            return xero_agent_core::ToolSandboxRequirement::Network;
        }
        AUTONOMOUS_TOOL_COMMAND_PROBE | AUTONOMOUS_TOOL_COMMAND_VERIFY => {
            return xero_agent_core::ToolSandboxRequirement::WorkspaceWrite;
        }
        AUTONOMOUS_TOOL_COMMAND_RUN
        | AUTONOMOUS_TOOL_COMMAND_SESSION
        | AUTONOMOUS_TOOL_BROWSER_CONTROL
        | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED => {
            return xero_agent_core::ToolSandboxRequirement::FullLocal;
        }
        _ => {}
    }
    match tool_effect_class(tool_name).as_str() {
        "observe" => xero_agent_core::ToolSandboxRequirement::ReadOnly,
        "runtime_state" => xero_agent_core::ToolSandboxRequirement::None,
        "write" | "destructive_write" => xero_agent_core::ToolSandboxRequirement::WorkspaceWrite,
        "external_service" => xero_agent_core::ToolSandboxRequirement::Network,
        _ => xero_agent_core::ToolSandboxRequirement::FullLocal,
    }
}

fn core_approval_requirement_for_tool(tool_name: &str) -> xero_agent_core::ToolApprovalRequirement {
    if matches!(
        tool_name,
        AUTONOMOUS_TOOL_MCP_LIST
            | AUTONOMOUS_TOOL_MCP_READ_RESOURCE
            | AUTONOMOUS_TOOL_MCP_GET_PROMPT
    ) {
        return xero_agent_core::ToolApprovalRequirement::Never;
    }
    if matches!(
        tool_name,
        AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED | AUTONOMOUS_TOOL_BROWSER_CONTROL
    ) {
        return xero_agent_core::ToolApprovalRequirement::Policy;
    }
    match tool_effect_class(tool_name).as_str() {
        "observe" => xero_agent_core::ToolApprovalRequirement::Never,
        "destructive_write" => xero_agent_core::ToolApprovalRequirement::Always,
        _ => xero_agent_core::ToolApprovalRequirement::Policy,
    }
}

fn json_string_vec(value: &JsonValue) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_registry_decodes_dynamic_mcp_tool_routes() {
        let tool_name = "mcp__workspace__playwright_click__0123456789".to_string();
        let registry = ToolRegistry::from_descriptors_with_dynamic_routes(
            vec![AgentToolDescriptor {
                name: tool_name.clone(),
                description: "MCP tool `playwright_click` from server `workspace`.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "selector": { "type": "string" }
                    },
                    "required": ["selector"]
                }),
            }],
            BTreeMap::from([(
                tool_name.clone(),
                AutonomousDynamicToolRoute::McpTool {
                    server_id: "workspace".into(),
                    tool_name: "playwright_click".into(),
                },
            )]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                ..ToolRegistryOptions::default()
            },
        );

        let request = registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-1".into(),
                tool_name,
                input: json!({ "selector": "#submit" }),
            })
            .expect("decode dynamic tool call");

        match request {
            AutonomousToolRequest::Mcp(request) => {
                assert_eq!(request.action, AutonomousMcpAction::InvokeTool);
                assert_eq!(request.server_id.as_deref(), Some("workspace"));
                assert_eq!(request.name.as_deref(), Some("playwright_click"));
                assert_eq!(request.arguments, Some(json!({ "selector": "#submit" })));
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn ask_registry_filters_and_denies_forbidden_tools_at_decode() {
        let registry = ToolRegistry::for_tool_names_with_options(
            BTreeSet::from([
                AUTONOMOUS_TOOL_READ.to_string(),
                AUTONOMOUS_TOOL_COMMAND_RUN.to_string(),
                AUTONOMOUS_TOOL_SUBAGENT.to_string(),
            ]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                ..ToolRegistryOptions::default()
            },
        );

        assert!(registry.descriptor(AUTONOMOUS_TOOL_READ).is_some());
        assert!(registry.descriptor(AUTONOMOUS_TOOL_COMMAND_RUN).is_none());
        assert!(registry.descriptor(AUTONOMOUS_TOOL_SUBAGENT).is_none());

        let error = registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-command".into(),
                tool_name: AUTONOMOUS_TOOL_COMMAND_RUN.into(),
                input: json!({"argv": ["echo", "hello"]}),
            })
            .expect_err("Ask must reject command calls even if the provider emits one");

        assert_eq!(error.code, "agent_tool_boundary_violation");
    }

    #[test]
    fn ask_registry_exposes_project_context_reads_but_not_writes() {
        let registry = ToolRegistry::for_tool_names_with_options(
            BTreeSet::from([
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH.to_string(),
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD.to_string(),
            ]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                ..ToolRegistryOptions::default()
            },
        );

        assert!(registry
            .descriptor(AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH)
            .is_some());
        assert!(registry
            .descriptor(AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD)
            .is_none());

        let error = registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-project-context-record".into(),
                tool_name: AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD.into(),
                input: json!({
                    "title": "Ask should not write context",
                    "summary": "Ask should not write context",
                    "text": "Ask should not write context"
                }),
            })
            .expect_err("Ask must reject write-capable durable context actions");

        assert_eq!(error.code, "agent_tool_boundary_violation");
    }

    #[test]
    fn registry_filters_tools_that_do_not_match_the_current_host_os() {
        let registry = ToolRegistry::for_tool_names_with_options(
            BTreeSet::from([
                AUTONOMOUS_TOOL_MACOS_AUTOMATION.to_string(),
                AUTONOMOUS_TOOL_POWERSHELL.to_string(),
                AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED.to_string(),
            ]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                ..ToolRegistryOptions::default()
            },
        );
        let names = registry.descriptor_names();

        assert_eq!(
            names.contains(AUTONOMOUS_TOOL_MACOS_AUTOMATION),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            names.contains(AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            names.contains(AUTONOMOUS_TOOL_POWERSHELL),
            cfg!(target_os = "windows")
        );

        let unavailable_tool = if cfg!(target_os = "windows") {
            AUTONOMOUS_TOOL_MACOS_AUTOMATION
        } else {
            AUTONOMOUS_TOOL_POWERSHELL
        };
        let error = registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-os-specific".into(),
                tool_name: unavailable_tool.into(),
                input: json!({}),
            })
            .expect_err("host-unavailable tools must fail closed");

        assert_eq!(error.code, "agent_tool_unavailable_on_host");
    }

    #[test]
    fn action_level_wrappers_reject_actions_outside_their_surface() {
        let registry = ToolRegistry::for_tool_names_with_options(
            BTreeSet::from([
                AUTONOMOUS_TOOL_BROWSER_OBSERVE.to_string(),
                AUTONOMOUS_TOOL_MCP_LIST.to_string(),
                AUTONOMOUS_TOOL_COMMAND_PROBE.to_string(),
                AUTONOMOUS_TOOL_COMMAND_VERIFY.to_string(),
                AUTONOMOUS_TOOL_COMMAND_RUN.to_string(),
            ]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                ..ToolRegistryOptions::default()
            },
        );

        let browser_error = registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-browser-click".into(),
                tool_name: AUTONOMOUS_TOOL_BROWSER_OBSERVE.into(),
                input: json!({ "action": "click", "selector": "#submit" }),
            })
            .expect_err("browser_observe must not control the page");
        assert_eq!(browser_error.code, "agent_action_tool_input_invalid");

        let mcp_error = registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-mcp-invoke".into(),
                tool_name: AUTONOMOUS_TOOL_MCP_LIST.into(),
                input: json!({ "action": "invoke_tool", "serverId": "fixture", "name": "echo" }),
            })
            .expect_err("mcp_list must not invoke tools");
        assert_eq!(mcp_error.code, "agent_action_tool_input_invalid");

        let probe_error = registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-probe-test".into(),
                tool_name: AUTONOMOUS_TOOL_COMMAND_PROBE.into(),
                input: json!({ "argv": ["cargo", "test"] }),
            })
            .expect_err("command_probe must be narrower than test execution");
        assert_eq!(probe_error.code, "agent_action_tool_input_invalid");

        let verify_error = registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-verify-echo".into(),
                tool_name: AUTONOMOUS_TOOL_COMMAND_VERIFY.into(),
                input: json!({ "argv": ["echo", "hello"] }),
            })
            .expect_err("command_verify must be narrower than general commands");
        assert_eq!(verify_error.code, "agent_action_tool_input_invalid");

        registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-run-echo".into(),
                tool_name: AUTONOMOUS_TOOL_COMMAND_RUN.into(),
                input: json!({ "argv": ["echo", "hello"] }),
            })
            .expect("command_run keeps the broader command surface");
    }

    #[test]
    fn debug_evidence_todo_mode_is_reserved_for_debug_agent() {
        let debug_registry = ToolRegistry::for_tool_names_with_options(
            BTreeSet::from([AUTONOMOUS_TOOL_TODO.to_string()]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Debug,
                ..ToolRegistryOptions::default()
            },
        );

        let request = debug_registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-debug-ledger".into(),
                tool_name: AUTONOMOUS_TOOL_TODO.into(),
                input: json!({
                    "action": "upsert",
                    "title": "Reproduced panic",
                    "mode": "debug_evidence",
                    "debugStage": "reproduction",
                    "evidence": "cargo test reproduced the panic"
                }),
            })
            .expect("Debug may use structured evidence ledger todos");
        assert!(matches!(request, AutonomousToolRequest::Todo(_)));

        let engineer_registry = ToolRegistry::for_tool_names_with_options(
            BTreeSet::from([AUTONOMOUS_TOOL_TODO.to_string()]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                ..ToolRegistryOptions::default()
            },
        );
        let error = engineer_registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-engineer-ledger".into(),
                tool_name: AUTONOMOUS_TOOL_TODO.into(),
                input: json!({
                    "action": "upsert",
                    "title": "Engineer should not use debug ledger",
                    "mode": "debug_evidence",
                    "debugStage": "hypothesis"
                }),
            })
            .expect_err("debug evidence mode is Debug-only");

        assert_eq!(error.code, "agent_debug_evidence_todo_mode_denied");
    }

    #[test]
    fn engineer_registry_excludes_removed_test_harness_and_agent_definition_mutation() {
        let registry = ToolRegistry::builtin_with_options(ToolRegistryOptions {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            skill_tool_enabled: true,
            ..ToolRegistryOptions::default()
        });
        let names = registry.descriptor_names();

        for expected in [
            AUTONOMOUS_TOOL_TOOL_SEARCH,
            AUTONOMOUS_TOOL_TOOL_ACCESS,
            AUTONOMOUS_TOOL_GIT_STATUS,
            AUTONOMOUS_TOOL_GIT_DIFF,
            AUTONOMOUS_TOOL_READ,
            AUTONOMOUS_TOOL_HASH,
            AUTONOMOUS_TOOL_TODO,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH,
            AUTONOMOUS_TOOL_MKDIR,
            AUTONOMOUS_TOOL_WRITE,
            AUTONOMOUS_TOOL_EDIT,
            AUTONOMOUS_TOOL_RENAME,
            AUTONOMOUS_TOOL_DELETE,
            AUTONOMOUS_TOOL_COMMAND_PROBE,
            AUTONOMOUS_TOOL_COMMAND_VERIFY,
            AUTONOMOUS_TOOL_COMMAND_RUN,
            AUTONOMOUS_TOOL_COMMAND_SESSION,
            AUTONOMOUS_TOOL_PROCESS_MANAGER,
            AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
            AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
            AUTONOMOUS_TOOL_BROWSER_OBSERVE,
            AUTONOMOUS_TOOL_BROWSER_CONTROL,
            AUTONOMOUS_TOOL_MCP_LIST,
            AUTONOMOUS_TOOL_MCP_READ_RESOURCE,
            AUTONOMOUS_TOOL_MCP_GET_PROMPT,
            AUTONOMOUS_TOOL_MCP_CALL_TOOL,
            AUTONOMOUS_TOOL_SKILL,
            AUTONOMOUS_TOOL_EMULATOR,
            AUTONOMOUS_TOOL_SOLANA_CLUSTER,
        ] {
            assert!(
                names.contains(expected),
                "Engineer registry should expose `{expected}`"
            );
        }
        assert!(
            !names.contains(AUTONOMOUS_TOOL_HARNESS_RUNNER),
            "harness_runner is reserved for the removed Test agent"
        );
        assert_eq!(
            names.contains(AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED),
            cfg!(target_os = "macos"),
            "macOS-only privileged diagnostics should match host availability"
        );
        assert_eq!(
            names.contains(AUTONOMOUS_TOOL_MACOS_AUTOMATION),
            cfg!(target_os = "macos"),
            "macOS automation should match host availability"
        );
        assert_eq!(
            names.contains(AUTONOMOUS_TOOL_POWERSHELL),
            cfg!(target_os = "windows"),
            "PowerShell should match host availability"
        );
        assert!(
            !names.contains(AUTONOMOUS_TOOL_AGENT_DEFINITION),
            "Engineer must not inherit Agent Create's definition-registry mutation tool"
        );

        let agent_create_registry = ToolRegistry::for_tool_names_with_options(
            BTreeSet::from([
                AUTONOMOUS_TOOL_AGENT_DEFINITION.to_string(),
                AUTONOMOUS_TOOL_WRITE.to_string(),
                AUTONOMOUS_TOOL_COMMAND_RUN.to_string(),
            ]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::AgentCreate,
                ..ToolRegistryOptions::default()
            },
        );
        let agent_create_names = agent_create_registry.descriptor_names();

        assert!(agent_create_names.contains(AUTONOMOUS_TOOL_AGENT_DEFINITION));
        assert!(!agent_create_names.contains(AUTONOMOUS_TOOL_WRITE));
        assert!(!agent_create_names.contains(AUTONOMOUS_TOOL_COMMAND_RUN));
    }

    #[test]
    fn descriptors_v2_preserve_policy_metadata_for_existing_tools() {
        let registry = ToolRegistry::for_tool_names_with_options(
            BTreeSet::from([
                AUTONOMOUS_TOOL_READ.to_string(),
                AUTONOMOUS_TOOL_PATCH.to_string(),
                AUTONOMOUS_TOOL_COMMAND_VERIFY.to_string(),
            ]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                ..ToolRegistryOptions::default()
            },
        );

        let descriptors = registry.descriptors_v2();
        let read = descriptors
            .iter()
            .find(|descriptor| descriptor.name == AUTONOMOUS_TOOL_READ)
            .expect("read descriptor");
        let patch = descriptors
            .iter()
            .find(|descriptor| descriptor.name == AUTONOMOUS_TOOL_PATCH)
            .expect("patch descriptor");
        let command = descriptors
            .iter()
            .find(|descriptor| descriptor.name == AUTONOMOUS_TOOL_COMMAND_VERIFY)
            .expect("command descriptor");

        assert_eq!(read.mutability, xero_agent_core::ToolMutability::ReadOnly);
        assert_eq!(patch.mutability, xero_agent_core::ToolMutability::Mutating);
        assert_eq!(
            command.effect_class,
            xero_agent_core::ToolEffectClass::CommandExecution
        );
        assert_eq!(
            command.sandbox_requirement,
            xero_agent_core::ToolSandboxRequirement::WorkspaceWrite
        );
        assert!(read.capability_tags.iter().any(|tag| tag == "file"));
        assert_eq!(
            patch.sandbox_requirement,
            xero_agent_core::ToolSandboxRequirement::WorkspaceWrite
        );
    }

    #[test]
    fn tool_application_style_orders_discovery_and_edit_families() {
        let registry_for_style = |style| {
            ToolRegistry::for_tool_names_with_options(
                BTreeSet::from([
                    AUTONOMOUS_TOOL_READ.to_string(),
                    AUTONOMOUS_TOOL_SEARCH.to_string(),
                    AUTONOMOUS_TOOL_CODE_INTEL.to_string(),
                    AUTONOMOUS_TOOL_EDIT.to_string(),
                    AUTONOMOUS_TOOL_PATCH.to_string(),
                ]),
                ToolRegistryOptions {
                    runtime_agent_id: RuntimeAgentIdDto::Engineer,
                    tool_application_policy: ResolvedAgentToolApplicationStyleDto {
                        style,
                        ..ResolvedAgentToolApplicationStyleDto::default()
                    },
                    ..ToolRegistryOptions::default()
                },
            )
        };
        let index = |registry: &ToolRegistry, tool_name: &str| {
            registry
                .descriptors()
                .iter()
                .position(|descriptor| descriptor.name == tool_name)
                .unwrap_or_else(|| panic!("missing descriptor {tool_name}"))
        };

        let balanced = registry_for_style(AgentToolApplicationStyleDto::Balanced);
        assert!(index(&balanced, AUTONOMOUS_TOOL_READ) < index(&balanced, AUTONOMOUS_TOOL_SEARCH));
        assert!(index(&balanced, AUTONOMOUS_TOOL_EDIT) < index(&balanced, AUTONOMOUS_TOOL_PATCH));

        let conservative = registry_for_style(AgentToolApplicationStyleDto::Conservative);
        assert!(
            index(&conservative, AUTONOMOUS_TOOL_READ)
                < index(&conservative, AUTONOMOUS_TOOL_SEARCH)
        );
        assert!(
            index(&conservative, AUTONOMOUS_TOOL_EDIT)
                < index(&conservative, AUTONOMOUS_TOOL_PATCH)
        );

        let declarative = registry_for_style(AgentToolApplicationStyleDto::DeclarativeFirst);
        assert!(
            index(&declarative, AUTONOMOUS_TOOL_SEARCH) < index(&declarative, AUTONOMOUS_TOOL_READ)
        );
        assert!(
            index(&declarative, AUTONOMOUS_TOOL_CODE_INTEL)
                < index(&declarative, AUTONOMOUS_TOOL_READ)
        );
        assert!(
            index(&declarative, AUTONOMOUS_TOOL_PATCH) < index(&declarative, AUTONOMOUS_TOOL_EDIT)
        );
    }

    #[test]
    fn descriptors_v2_mark_discovery_tools_as_bounded_read_only_batches() {
        let registry = ToolRegistry::for_tool_names_with_options(
            BTreeSet::from([
                AUTONOMOUS_TOOL_SEARCH.to_string(),
                AUTONOMOUS_TOOL_FIND.to_string(),
                AUTONOMOUS_TOOL_WORKSPACE_INDEX.to_string(),
                AUTONOMOUS_TOOL_CODE_INTEL.to_string(),
                AUTONOMOUS_TOOL_LSP.to_string(),
                AUTONOMOUS_TOOL_PATCH.to_string(),
            ]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                ..ToolRegistryOptions::default()
            },
        );
        let descriptors = registry.descriptors_v2();
        let descriptor = |name: &str| {
            descriptors
                .iter()
                .find(|descriptor| descriptor.name == name)
                .unwrap_or_else(|| panic!("missing descriptor {name}"))
        };

        for name in [
            AUTONOMOUS_TOOL_SEARCH,
            AUTONOMOUS_TOOL_FIND,
            AUTONOMOUS_TOOL_WORKSPACE_INDEX,
            AUTONOMOUS_TOOL_CODE_INTEL,
            AUTONOMOUS_TOOL_LSP,
        ] {
            let descriptor = descriptor(name);
            assert_eq!(descriptor.application_metadata.family, "discovery");
            assert_eq!(
                descriptor.application_metadata.kind,
                xero_agent_core::ToolApplicationKind::ReadOnlyBatch
            );
            assert_eq!(
                descriptor.application_metadata.dispatch_safety,
                xero_agent_core::ToolBatchDispatchSafety::ParallelReadOnly
            );
            assert_eq!(
                descriptor.mutability,
                xero_agent_core::ToolMutability::ReadOnly
            );
            assert!(descriptor
                .application_metadata
                .safety_requirements
                .iter()
                .any(|requirement| requirement == "bounded_results"));
            assert!(descriptor
                .application_metadata
                .safety_requirements
                .iter()
                .any(|requirement| requirement == "explicit_failure_modes"));
        }

        let patch = descriptor(AUTONOMOUS_TOOL_PATCH);
        assert_eq!(
            patch.application_metadata.kind,
            xero_agent_core::ToolApplicationKind::Declarative
        );
        assert!(patch
            .application_metadata
            .safety_requirements
            .iter()
            .any(|requirement| requirement == "supports_preview"));
    }
}
