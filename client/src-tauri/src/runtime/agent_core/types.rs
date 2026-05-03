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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRegistry {
    descriptors: Vec<AgentToolDescriptor>,
    dynamic_routes: BTreeMap<String, AutonomousDynamicToolRoute>,
    options: ToolRegistryOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRegistryOptions {
    pub skill_tool_enabled: bool,
    pub browser_control_preference: BrowserControlPreferenceDto,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_tool_policy: Option<AutonomousAgentToolPolicy>,
}

impl Default for ToolRegistryOptions {
    fn default() -> Self {
        Self {
            skill_tool_enabled: false,
            browser_control_preference: BrowserControlPreferenceDto::Default,
            runtime_agent_id: RuntimeAgentIdDto::Ask,
            agent_tool_policy: None,
        }
    }
}

impl ToolRegistry {
    pub fn builtin() -> Self {
        Self::builtin_with_options(ToolRegistryOptions::default())
    }

    pub fn builtin_with_options(options: ToolRegistryOptions) -> Self {
        Self {
            descriptors: builtin_tool_descriptors()
                .into_iter()
                .filter(|descriptor| {
                    options.skill_tool_enabled || descriptor.name != AUTONOMOUS_TOOL_SKILL
                })
                .filter(|descriptor| {
                    tool_allowed_for_runtime_agent_with_policy(
                        options.runtime_agent_id,
                        &descriptor.name,
                        options.agent_tool_policy.as_ref(),
                    )
                })
                .collect(),
            dynamic_routes: BTreeMap::new(),
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
        let mut names = select_tool_names_for_prompt(repo_root, prompt, controls, &options);
        if !options.skill_tool_enabled {
            names.remove(AUTONOMOUS_TOOL_SKILL);
        }
        Self::for_tool_names_with_options(names, options)
    }

    pub fn for_tool_names(tool_names: BTreeSet<String>) -> Self {
        Self::for_tool_names_with_options(tool_names, ToolRegistryOptions::default())
    }

    pub fn for_tool_names_with_options(
        tool_names: BTreeSet<String>,
        options: ToolRegistryOptions,
    ) -> Self {
        let descriptors = builtin_tool_descriptors()
            .into_iter()
            .filter(|descriptor| {
                tool_names.contains(descriptor.name.as_str())
                    && (options.skill_tool_enabled || descriptor.name != AUTONOMOUS_TOOL_SKILL)
                    && tool_allowed_for_runtime_agent_with_policy(
                        options.runtime_agent_id,
                        &descriptor.name,
                        options.agent_tool_policy.as_ref(),
                    )
            })
            .collect();
        Self {
            descriptors,
            dynamic_routes: BTreeMap::new(),
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
            .filter(|descriptor| {
                tool_allowed_for_runtime_agent_with_policy(
                    options.runtime_agent_id,
                    &descriptor.name,
                    options.agent_tool_policy.as_ref(),
                )
            })
            .collect::<Vec<_>>();
        descriptors.sort_by(|left, right| left.name.cmp(&right.name));
        Self {
            descriptors,
            dynamic_routes,
            options,
        }
    }

    pub fn descriptors(&self) -> &[AgentToolDescriptor] {
        &self.descriptors
    }

    pub(crate) fn dynamic_routes(&self) -> &BTreeMap<String, AutonomousDynamicToolRoute> {
        &self.dynamic_routes
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
        let mut names = self.descriptor_names();
        for tool_name in tool_names {
            names.insert(tool_name.as_ref().to_owned());
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
        next.dynamic_routes = dynamic_routes;
        *self = next;
    }

    pub(crate) fn expand_with_tool_names_from_runtime<I, S>(
        &mut self,
        tool_names: I,
        tool_runtime: &AutonomousToolRuntime,
    ) -> CommandResult<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
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

        for tool_name in tool_names {
            let tool_name = tool_name.as_ref();
            if let Some(descriptor) = builtin_descriptors.get(tool_name) {
                if (self.options.skill_tool_enabled || descriptor.name != AUTONOMOUS_TOOL_SKILL)
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

        self.descriptors = descriptors_by_name.into_values().collect();
        self.dynamic_routes = dynamic_routes;
        Ok(())
    }

    pub fn decode_call(&self, tool_call: &AgentToolCall) -> CommandResult<AutonomousToolRequest> {
        if self.descriptor(&tool_call.tool_name).is_none() {
            let known_tool = tool_access_all_known_tools().contains(tool_call.tool_name.as_str())
                || tool_call
                    .tool_name
                    .starts_with(AUTONOMOUS_DYNAMIC_MCP_TOOL_PREFIX);
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

        if let Some(route) = self.dynamic_routes.get(&tool_call.tool_name) {
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

        let request_value = json!({
            "tool": tool_call.tool_name,
            "input": tool_call.input,
        });
        serde_json::from_value::<AutonomousToolRequest>(request_value).map_err(|error| {
            CommandError::user_fixable(
                "agent_tool_call_invalid",
                format!(
                    "Xero could not decode owned-agent tool call `{}` for `{}`: {error}",
                    tool_call.tool_call_id, tool_call.tool_name
                ),
            )
        })
    }

    pub fn validate_call(&self, tool_call: &AgentToolCall) -> CommandResult<()> {
        self.decode_call(tool_call).map(|_| ())
    }

    pub(crate) fn runtime_agent_id(&self) -> RuntimeAgentIdDto {
        self.options.runtime_agent_id
    }
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
            ProviderTurnOutcome::Complete { message, usage } => Ok(ProviderCompactionOutcome {
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
            "Extract durable memory candidates from this Xero coding-agent transcript. Return only a JSON array. Each item must contain scope, kind, text, confidence, and sourceItemIds. scope must be project or session. kind must be project_fact, user_preference, decision, session_summary, or troubleshooting. Do not include secrets. Do not include duplicates of existing approved or candidate memories.\n\nExisting memories:\n{existing}\n\nTranscript:\n{}",
            request.transcript
        );
        let turn = ProviderTurnRequest {
            system_prompt: "You propose durable context candidates for a coding-agent desktop app. Return strict JSON only, never markdown. Capture stable project facts, user preferences, decisions, session summaries, and troubleshooting facts. Prefer no item over a weak item. Never include secrets.".into(),
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
            ProviderTurnOutcome::Complete { message, usage } => {
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
        usage: Option<ProviderUsage>,
    },
    ToolCalls {
        message: String,
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
                .any(|descriptor| descriptor.name == AUTONOMOUS_TOOL_COMMAND)
            && !request.messages.iter().any(|message| {
                matches!(
                    message,
                    ProviderMessage::Tool { tool_name, .. }
                        if tool_name == AUTONOMOUS_TOOL_COMMAND
                )
            })
        {
            let message = "Xero fake provider is recording verification evidence.".to_string();
            emit(ProviderStreamEvent::MessageDelta(message.clone()))?;
            return Ok(ProviderTurnOutcome::ToolCalls {
                message,
                tool_calls: vec![AgentToolCall {
                    tool_call_id: format!("fake-tool-call-verify-{}", request.turn_index),
                    tool_name: AUTONOMOUS_TOOL_COMMAND.into(),
                    input: json!({ "argv": ["echo", "xero-verification-ok"] }),
                }],
                usage: Some(ProviderUsage::default()),
            });
        }

        if request
            .messages
            .iter()
            .any(|message| matches!(message, ProviderMessage::Tool { .. }))
        {
            let message =
                "Owned agent run completed through the Xero model-loop scaffold.".to_string();
            emit(ProviderStreamEvent::MessageDelta(message.clone()))?;
            return Ok(ProviderTurnOutcome::Complete {
                message,
                usage: Some(ProviderUsage::default()),
            });
        }

        let user_prompt = request
            .messages
            .iter()
            .filter_map(|message| match message {
                ProviderMessage::User { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let tool_calls = parse_fake_tool_directives(&user_prompt);
        let message = "Xero owned-agent runtime accepted the task.".to_string();
        emit(ProviderStreamEvent::MessageDelta(message.clone()))?;
        if tool_calls.is_empty() {
            Ok(ProviderTurnOutcome::Complete {
                message,
                usage: Some(ProviderUsage::default()),
            })
        } else {
            Ok(ProviderTurnOutcome::ToolCalls {
                message,
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
                AUTONOMOUS_TOOL_COMMAND.to_string(),
                AUTONOMOUS_TOOL_SUBAGENT.to_string(),
            ]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                ..ToolRegistryOptions::default()
            },
        );

        assert!(registry.descriptor(AUTONOMOUS_TOOL_READ).is_some());
        assert!(registry.descriptor(AUTONOMOUS_TOOL_COMMAND).is_none());
        assert!(registry.descriptor(AUTONOMOUS_TOOL_SUBAGENT).is_none());

        let error = registry
            .decode_call(&AgentToolCall {
                tool_call_id: "call-command".into(),
                tool_name: AUTONOMOUS_TOOL_COMMAND.into(),
                input: json!({"argv": ["echo", "hello"]}),
            })
            .expect_err("Ask must reject command calls even if the provider emits one");

        assert_eq!(error.code, "agent_tool_boundary_violation");
    }
}
