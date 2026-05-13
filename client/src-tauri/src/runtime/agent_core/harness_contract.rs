use super::*;
use sha2::{Digest, Sha256};

pub const HARNESS_CONTRACT_SCHEMA: &str = "xero.harness_contract.v1";
pub const HARNESS_CONTRACT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarnessContractExportOptions {
    pub skill_tool_enabled: bool,
}

impl Default for HarnessContractExportOptions {
    fn default() -> Self {
        Self {
            skill_tool_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessContractExport {
    pub schema: String,
    pub schema_version: u32,
    pub prompt_version: String,
    pub built_in_agents: Vec<crate::commands::RuntimeAgentDescriptorDto>,
    pub tool_groups: Vec<AutonomousToolAccessGroup>,
    pub tools: Vec<HarnessToolCapabilitySpec>,
    pub tool_packs: Vec<xero_agent_core::DomainToolPackManifest>,
    pub agent_access: Vec<HarnessAgentToolAccessSnapshot>,
    pub prompt_snapshots: Vec<HarnessPromptSnapshot>,
    pub tool_registry_snapshots: Vec<HarnessToolRegistrySnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessToolCapabilitySpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descriptor: Option<AgentToolDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v2_descriptor: Option<xero_agent_core::ToolDescriptorV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<HarnessToolCatalogSnapshot>,
    pub effect_class: String,
    pub effect_class_known: bool,
    pub schema_sha256: String,
    pub action_values: Vec<String>,
    pub access_groups: Vec<String>,
    pub tool_pack_ids: Vec<String>,
    pub allowed_runtime_agents: Vec<String>,
    pub runtime_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessToolCatalogSnapshot {
    pub tool_name: String,
    pub group: String,
    pub description: String,
    pub tags: Vec<String>,
    pub schema_fields: Vec<String>,
    pub examples: Vec<String>,
    pub risk_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessAgentToolAccessSnapshot {
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub scenario: String,
    pub descriptor_count: usize,
    pub tool_names: Vec<String>,
    pub descriptors_v2_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessPromptSnapshot {
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub scenario: String,
    pub prompt: String,
    pub prompt_sha256: String,
    pub fragment_count: usize,
    pub fragment_ids: Vec<String>,
    pub fragments: Vec<HarnessPromptFragmentSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessPromptFragmentSnapshot {
    pub id: String,
    pub priority: u16,
    pub title: String,
    pub provenance: String,
    pub budget_policy: String,
    pub inclusion_reason: String,
    pub content: String,
    pub sha256: String,
    pub token_estimate: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessToolRegistrySnapshot {
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub scenario: String,
    pub descriptor_count: usize,
    pub descriptor_names: Vec<String>,
    pub descriptors_v2_sha256: String,
    pub descriptors_v2: Vec<xero_agent_core::ToolDescriptorV2>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessContractDrift {
    pub tool_name: String,
    pub missing_surfaces: Vec<String>,
}

pub fn export_harness_contract(
    repo_root: &Path,
    options: HarnessContractExportOptions,
) -> CommandResult<HarnessContractExport> {
    let built_in_agents = crate::commands::builtin_runtime_agent_descriptors();
    let tool_groups = tool_access_group_descriptors();
    let tools = tool_capability_specs(options.skill_tool_enabled);
    let agent_access = built_in_agents
        .iter()
        .map(|agent| {
            agent_tool_access_snapshot(
                agent.id,
                "builtin_full",
                ToolRegistryOptions {
                    runtime_agent_id: agent.id,
                    skill_tool_enabled: options.skill_tool_enabled,
                    ..ToolRegistryOptions::default()
                },
            )
        })
        .collect::<CommandResult<Vec<_>>>()?;
    let prompt_snapshots = prompt_contract_snapshots(repo_root, &built_in_agents, options)?;
    let tool_registry_snapshots = tool_registry_contract_snapshots(&built_in_agents, options)?;

    Ok(HarnessContractExport {
        schema: HARNESS_CONTRACT_SCHEMA.into(),
        schema_version: HARNESS_CONTRACT_SCHEMA_VERSION,
        prompt_version: SYSTEM_PROMPT_VERSION.into(),
        built_in_agents,
        tool_groups,
        tools,
        tool_packs: xero_agent_core::domain_tool_pack_manifests(),
        agent_access,
        prompt_snapshots,
        tool_registry_snapshots,
    })
}

pub fn harness_contract_drift(contract: &HarnessContractExport) -> Vec<HarnessContractDrift> {
    contract
        .tools
        .iter()
        .filter_map(|tool| {
            let mut missing = Vec::new();
            if tool.descriptor.is_none() {
                missing.push("descriptor".into());
            }
            if !tool.effect_class_known {
                missing.push("effect_class".into());
            }
            if tool.access_groups.is_empty() {
                missing.push("access_group".into());
            }
            if tool.v2_descriptor.is_none() {
                missing.push("v2_descriptor".into());
            }
            if tool.catalog.is_none() {
                missing.push("catalog".into());
            }
            (!missing.is_empty()).then(|| HarnessContractDrift {
                tool_name: tool.name.clone(),
                missing_surfaces: missing,
            })
        })
        .collect()
}

fn tool_capability_specs(skill_tool_enabled: bool) -> Vec<HarnessToolCapabilitySpec> {
    let descriptors = builtin_tool_descriptors()
        .into_iter()
        .filter(|descriptor| skill_tool_enabled || descriptor.name != AUTONOMOUS_TOOL_SKILL)
        .map(|descriptor| (descriptor.name.clone(), descriptor))
        .collect::<BTreeMap<_, _>>();
    let catalog = deferred_tool_catalog(skill_tool_enabled)
        .into_iter()
        .map(|entry| (entry.tool_name.to_owned(), entry))
        .collect::<BTreeMap<_, _>>();
    let groups = tool_access_group_descriptors();
    let group_tool_names = tool_access_all_known_tools()
        .into_iter()
        .filter(|tool| skill_tool_enabled || *tool != AUTONOMOUS_TOOL_SKILL)
        .map(str::to_owned);
    let mut tool_names = descriptors.keys().cloned().collect::<BTreeSet<_>>();
    tool_names.extend(catalog.keys().cloned());
    tool_names.extend(group_tool_names);

    tool_names
        .into_iter()
        .map(|name| {
            let descriptor = descriptors.get(&name).cloned();
            let v2_descriptor = descriptor
                .as_ref()
                .map(|descriptor| descriptor.to_core_descriptor_v2(skill_tool_enabled));
            let schema = descriptor
                .as_ref()
                .map(|descriptor| descriptor.input_schema.clone())
                .unwrap_or_else(|| json!({}));
            let effect_class = tool_effect_class(&name);
            let catalog_snapshot = catalog.get(&name).map(catalog_snapshot);
            let tool_pack_ids = xero_agent_core::domain_tool_pack_ids_for_tool(&name);
            let allowed_runtime_agents = allowed_agents_for_tool(&name);
            let access_groups = groups
                .iter()
                .filter(|group| group.tools.iter().any(|tool| tool == &name))
                .map(|group| group.name.clone())
                .collect::<Vec<_>>();
            HarnessToolCapabilitySpec {
                name,
                descriptor,
                v2_descriptor,
                catalog: catalog_snapshot,
                effect_class: effect_class.as_str().into(),
                effect_class_known: effect_class != AutonomousToolEffectClass::Unknown,
                schema_sha256: stable_json_sha256(&schema),
                action_values: schema_action_values(&schema),
                access_groups,
                tool_pack_ids,
                allowed_runtime_agents,
                runtime_available: true,
            }
        })
        .collect()
}

fn catalog_snapshot(entry: &AutonomousToolCatalogEntry) -> HarnessToolCatalogSnapshot {
    HarnessToolCatalogSnapshot {
        tool_name: entry.tool_name.into(),
        group: entry.group.into(),
        description: entry.description.into(),
        tags: entry.tags.iter().map(|tag| (*tag).to_owned()).collect(),
        schema_fields: entry
            .schema_fields
            .iter()
            .map(|field| (*field).to_owned())
            .collect(),
        examples: entry
            .examples
            .iter()
            .map(|example| (*example).to_owned())
            .collect(),
        risk_class: entry.risk_class.into(),
    }
}

fn allowed_agents_for_tool(tool_name: &str) -> Vec<String> {
    crate::commands::builtin_runtime_agent_descriptors()
        .into_iter()
        .filter(|agent| tool_allowed_for_runtime_agent(agent.id, tool_name))
        .map(|agent| agent.id.as_str().to_owned())
        .collect()
}

fn agent_tool_access_snapshot(
    runtime_agent_id: RuntimeAgentIdDto,
    scenario: &str,
    options: ToolRegistryOptions,
) -> CommandResult<HarnessAgentToolAccessSnapshot> {
    let registry = ToolRegistry::builtin_with_options(options);
    let descriptors_v2 = registry.descriptors_v2();
    validate_v2_descriptors(&descriptors_v2)?;
    let tool_names = sorted_descriptor_names(&registry);
    Ok(HarnessAgentToolAccessSnapshot {
        runtime_agent_id,
        scenario: scenario.into(),
        descriptor_count: tool_names.len(),
        tool_names,
        descriptors_v2_sha256: stable_json_sha256(&descriptors_v2),
    })
}

fn prompt_contract_snapshots(
    repo_root: &Path,
    agents: &[crate::commands::RuntimeAgentDescriptorDto],
    options: HarnessContractExportOptions,
) -> CommandResult<Vec<HarnessPromptSnapshot>> {
    let mut snapshots = Vec::new();
    for agent in agents {
        snapshots.push(prompt_snapshot_for_agent(
            repo_root,
            agent.id,
            "base",
            options,
            None,
            Vec::new(),
            None,
            None,
        )?);
        snapshots.push(prompt_snapshot_for_agent(
            repo_root,
            agent.id,
            "custom_policy_skill_process_coordination",
            options,
            Some(&custom_agent_definition_snapshot(agent.id)),
            vec![sample_skill_context()],
            Some("contract-process: ready on pid 12345"),
            Some("contract-coordination: no active write reservations"),
        )?);
    }
    Ok(snapshots)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Contract snapshots intentionally vary independent prompt inputs."
)]
fn prompt_snapshot_for_agent(
    repo_root: &Path,
    runtime_agent_id: RuntimeAgentIdDto,
    scenario: &str,
    options: HarnessContractExportOptions,
    agent_definition_snapshot: Option<&JsonValue>,
    skill_contexts: Vec<XeroSkillToolContextPayload>,
    owned_process_summary: Option<&str>,
    active_coordination_summary: Option<&str>,
) -> CommandResult<HarnessPromptSnapshot> {
    let registry = ToolRegistry::builtin_with_options(ToolRegistryOptions {
        runtime_agent_id,
        skill_tool_enabled: options.skill_tool_enabled,
        ..ToolRegistryOptions::default()
    });
    let compilation = PromptCompiler::new(
        repo_root,
        Some("contract-project"),
        None,
        runtime_agent_id,
        BrowserControlPreferenceDto::Default,
        registry.descriptors(),
    )
    .with_agent_definition_snapshot(agent_definition_snapshot)
    .with_skill_contexts(skill_contexts)
    .with_owned_process_summary(owned_process_summary)
    .with_active_coordination_summary(active_coordination_summary)
    .with_runtime_metadata(contract_snapshot_runtime_metadata())
    .compile()?;

    let fragment_ids = compilation
        .fragments
        .iter()
        .map(|fragment| fragment.id.clone())
        .collect::<Vec<_>>();
    let fragments = compilation
        .fragments
        .iter()
        .map(prompt_fragment_snapshot)
        .collect::<Vec<_>>();
    Ok(HarnessPromptSnapshot {
        runtime_agent_id,
        scenario: scenario.into(),
        prompt: compilation.prompt.clone(),
        prompt_sha256: stable_text_sha256(&compilation.prompt),
        fragment_count: fragments.len(),
        fragment_ids,
        fragments,
    })
}

fn prompt_fragment_snapshot(fragment: &PromptFragment) -> HarnessPromptFragmentSnapshot {
    HarnessPromptFragmentSnapshot {
        id: fragment.id.clone(),
        priority: fragment.priority,
        title: fragment.title.clone(),
        provenance: fragment.provenance.clone(),
        budget_policy: fragment.budget_policy.as_str().into(),
        inclusion_reason: fragment.inclusion_reason.clone(),
        content: fragment.body.clone(),
        sha256: fragment.sha256.clone(),
        token_estimate: fragment.token_estimate,
    }
}

fn contract_snapshot_runtime_metadata() -> RuntimeHostMetadata {
    RuntimeHostMetadata {
        timestamp_utc: "2026-05-01T00:00:00Z".into(),
        date_utc: "2026-05-01".into(),
        operating_system: std::env::consts::OS.into(),
        operating_system_label: match std::env::consts::OS {
            "macos" => "macOS",
            "windows" => "Windows",
            "linux" => "Linux",
            "ios" => "iOS",
            "android" => "Android",
            _ => "Other",
        }
        .into(),
        architecture: std::env::consts::ARCH.into(),
        family: std::env::consts::FAMILY.into(),
    }
}

fn tool_registry_contract_snapshots(
    agents: &[crate::commands::RuntimeAgentDescriptorDto],
    options: HarnessContractExportOptions,
) -> CommandResult<Vec<HarnessToolRegistrySnapshot>> {
    let mut snapshots = Vec::new();
    for agent in agents {
        snapshots.push(tool_registry_snapshot(
            agent.id,
            "builtin_full",
            ToolRegistryOptions {
                runtime_agent_id: agent.id,
                skill_tool_enabled: options.skill_tool_enabled,
                ..ToolRegistryOptions::default()
            },
        )?);
    }
    for (scenario, runtime_agent_id, policy) in [
        (
            "custom_observe_only",
            RuntimeAgentIdDto::Engineer,
            custom_policy_snapshot("observe_only"),
        ),
        (
            "custom_engineering",
            RuntimeAgentIdDto::Engineer,
            custom_policy_snapshot("engineering"),
        ),
        (
            "custom_agent_builder",
            RuntimeAgentIdDto::AgentCreate,
            custom_policy_snapshot("agent_builder"),
        ),
    ] {
        snapshots.push(tool_registry_snapshot(
            runtime_agent_id,
            scenario,
            ToolRegistryOptions {
                runtime_agent_id,
                skill_tool_enabled: options.skill_tool_enabled,
                agent_tool_policy: AutonomousAgentToolPolicy::from_definition_snapshot(&policy),
                ..ToolRegistryOptions::default()
            },
        )?);
    }
    Ok(snapshots)
}

fn tool_registry_snapshot(
    runtime_agent_id: RuntimeAgentIdDto,
    scenario: &str,
    options: ToolRegistryOptions,
) -> CommandResult<HarnessToolRegistrySnapshot> {
    let registry = ToolRegistry::builtin_with_options(options);
    let descriptors_v2 = registry.descriptors_v2();
    validate_v2_descriptors(&descriptors_v2)?;
    let descriptor_names = sorted_descriptor_names(&registry);
    Ok(HarnessToolRegistrySnapshot {
        runtime_agent_id,
        scenario: scenario.into(),
        descriptor_count: descriptor_names.len(),
        descriptor_names,
        descriptors_v2_sha256: stable_json_sha256(&descriptors_v2),
        descriptors_v2,
    })
}

fn validate_v2_descriptors(descriptors: &[xero_agent_core::ToolDescriptorV2]) -> CommandResult<()> {
    for descriptor in descriptors {
        descriptor.validate().map_err(|error| {
            CommandError::system_fault(
                "harness_contract_v2_descriptor_invalid",
                format!(
                    "Tool Registry V2 descriptor `{}` failed contract validation: {error}",
                    descriptor.name
                ),
            )
        })?;
    }
    Ok(())
}

fn sorted_descriptor_names(registry: &ToolRegistry) -> Vec<String> {
    registry.descriptor_names().into_iter().collect()
}

fn schema_action_values(schema: &JsonValue) -> Vec<String> {
    let mut values = BTreeSet::new();
    collect_schema_action_values(schema, &mut values);
    values.into_iter().collect()
}

fn collect_schema_action_values(schema: &JsonValue, values: &mut BTreeSet<String>) {
    if let Some(properties) = schema.get("properties").and_then(JsonValue::as_object) {
        if let Some(action_schema) = properties.get("action") {
            collect_enum_values(action_schema, values);
        }
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(items) = schema.get(key).and_then(JsonValue::as_array) {
            for item in items {
                collect_schema_action_values(item, values);
            }
        }
    }
}

fn collect_enum_values(schema: &JsonValue, values: &mut BTreeSet<String>) {
    if let Some(items) = schema.get("enum").and_then(JsonValue::as_array) {
        values.extend(
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_owned),
        );
    }
}

fn stable_json_sha256<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .map(|serialized| stable_text_sha256(&serialized))
        .unwrap_or_else(|_| stable_text_sha256("unserializable"))
}

fn stable_text_sha256(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn custom_agent_definition_snapshot(runtime_agent_id: RuntimeAgentIdDto) -> JsonValue {
    json!({
        "id": format!("contract_{}_agent", runtime_agent_id.as_str()),
        "version": 1,
        "scope": "project_custom",
        "displayName": format!("Contract {} Agent", runtime_agent_id.label()),
        "description": "Synthetic contract snapshot definition.",
        "taskPurpose": "Freeze prompt behavior for harness contract tests.",
        "workflowContract": ["inspect", "act_within_policy", "summarize"],
        "finalResponseContract": "Return concise contract evidence.",
        "promptFragments": ["Synthetic custom policy fragment."],
        "capabilities": ["contract_snapshot"],
        "safetyLimits": ["Do not expand beyond base runtime policy."],
        "retrievalDefaults": { "projectContext": true },
        "memoryCandidatePolicy": { "recordDurableFindings": true },
        "handoffPolicy": { "handoffWhenBlocked": true },
        "examplePrompts": ["Freeze the harness contract."],
        "refusalEscalationCases": ["Requests outside the selected base runtime profile."]
    })
}

fn custom_policy_snapshot(label: &str) -> JsonValue {
    json!({
        "toolPolicy": label
    })
}

fn sample_skill_context() -> XeroSkillToolContextPayload {
    XeroSkillToolContextPayload {
        contract_version: 1,
        source_id: "contract:skill".into(),
        skill_id: "contract-skill".into(),
        markdown: crate::runtime::XeroSkillToolContextDocument {
            relative_path: "SKILL.md".into(),
            sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            bytes: 58,
            content: "# Contract Skill\nKeep this bounded as skill context.\n".into(),
        },
        supporting_assets: vec![crate::runtime::XeroSkillToolContextAsset {
            relative_path: "guide.md".into(),
            sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            bytes: 35,
            content: "# Guide\nSynthetic snapshot asset.\n".into(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_export_covers_every_enabled_tool_surface() {
        let root = tempfile::tempdir().expect("temp dir");
        let contract =
            export_harness_contract(root.path(), HarnessContractExportOptions::default())
                .expect("export harness contract");

        let drift = harness_contract_drift(&contract);

        assert!(drift.is_empty(), "harness contract drift: {drift:#?}");
    }

    #[test]
    fn prompt_contract_snapshot_hashes_freeze_builtin_agent_policy() {
        let root = tempfile::tempdir().expect("temp dir");
        let contract =
            export_harness_contract(root.path(), HarnessContractExportOptions::default())
                .expect("export harness contract");
        let actual = contract
            .prompt_snapshots
            .iter()
            .map(|snapshot| {
                (
                    format!(
                        "{}:{}",
                        snapshot.runtime_agent_id.as_str(),
                        snapshot.scenario
                    ),
                    snapshot.prompt_sha256.as_str(),
                )
            })
            .collect::<Vec<_>>();
        let expected = vec![
            (
                "generalist:base".to_string(),
                "15ee630c38ef9d910ba1e6c8e9ba80b4bb8b7787563d6f592ad6f1c6eb9aeb7c",
            ),
            (
                "generalist:custom_policy_skill_process_coordination".to_string(),
                "9311241f86239793c39305022cfefe4b206cb474000763acbaffa51e9d2efa2c",
            ),
            (
                "ask:base".to_string(),
                "c92cb8fe6ce7393e465e2ef4afb3ee3b732e6fd57bcfffc45ff9c2510317ac99",
            ),
            (
                "ask:custom_policy_skill_process_coordination".to_string(),
                "d317c07a9ceabf383ca5fdf39c24824907da367d1eb4e9d143491673bbcae857",
            ),
            (
                "plan:base".to_string(),
                "91942dad115b33c0780069fce3f665eba8c69064615134a2c038ba1404ceab09",
            ),
            (
                "plan:custom_policy_skill_process_coordination".to_string(),
                "b3cd66f12594415f79d218acac79b181e632f7ff7d1f30c7dafabdb7f494cc31",
            ),
            (
                "engineer:base".to_string(),
                "afca8ef8c12b5d4f6ecc521136459ec09dcb01dee7e062f10607f2e6e1ebc6d5",
            ),
            (
                "engineer:custom_policy_skill_process_coordination".to_string(),
                "4bae5bbacacafd7b10709826c55baf31cc9f5e2b82aa375ef9462d366fa7dbce",
            ),
            (
                "debug:base".to_string(),
                "6c3466f66c46059affe8311c666760cf9d072e69222cdc8c597a2b9ec70dc25a",
            ),
            (
                "debug:custom_policy_skill_process_coordination".to_string(),
                "e79effd107d9e3815bad4b5fdc00bb8d1baed53698214c674b53c501d6520c7b",
            ),
            (
                "crawl:base".to_string(),
                "f94fd60d204db92608e5eb2f67528579d52a7661860c0d8ad4c9fd89b695f482",
            ),
            (
                "crawl:custom_policy_skill_process_coordination".to_string(),
                "86cda45dbdbaa44fe2e6b70352fb34e6ecaad3387255092311ed8ae149723d57",
            ),
            (
                "agent_create:base".to_string(),
                "885d2007ec85d4128f8367f793d70c9654a8f1241d59550d27dc86fec6009996",
            ),
            (
                "agent_create:custom_policy_skill_process_coordination".to_string(),
                "93b13c9fedfd683a35f6ce37c4ef01fef6e0c225e395fab82c87d6e8b2723f0e",
            ),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn tool_registry_v2_snapshot_hashes_freeze_builtin_and_custom_policy_access() {
        let root = tempfile::tempdir().expect("temp dir");
        let contract =
            export_harness_contract(root.path(), HarnessContractExportOptions::default())
                .expect("export harness contract");
        let actual = contract
            .tool_registry_snapshots
            .iter()
            .map(|snapshot| {
                (
                    format!(
                        "{}:{}",
                        snapshot.runtime_agent_id.as_str(),
                        snapshot.scenario
                    ),
                    snapshot.descriptors_v2_sha256.as_str(),
                )
            })
            .collect::<Vec<_>>();
        let expected = vec![
            (
                "generalist:builtin_full".to_string(),
                "bca7c1f189383eb464fe7c943084131c6ca3f5f6eb1c119007daca20133d0048",
            ),
            (
                "ask:builtin_full".to_string(),
                "b87b7fdfcba585073f0b539e5b155150a3ca64cef195fd9f99fbb56f8debc981",
            ),
            (
                "plan:builtin_full".to_string(),
                "ff632d12bac671b9c394701aad701a35850b47d71a9c8cc73ba8501aa6821c95",
            ),
            (
                "engineer:builtin_full".to_string(),
                "bca7c1f189383eb464fe7c943084131c6ca3f5f6eb1c119007daca20133d0048",
            ),
            (
                "debug:builtin_full".to_string(),
                "bca7c1f189383eb464fe7c943084131c6ca3f5f6eb1c119007daca20133d0048",
            ),
            (
                "crawl:builtin_full".to_string(),
                "d1dd724b9ae7d758f28fceb29569393e2a7fb838442561bb6a88c74a92bdbc1f",
            ),
            (
                "agent_create:builtin_full".to_string(),
                "ab51cda758dab0a43452b2ae48e3817638467bfe5cf390bee9acef9087084a1e",
            ),
            (
                "engineer:custom_observe_only".to_string(),
                "dfaedd4c424bee8b90f94f0f9ed208ee4d2ff560268a2a279e7ab24d90835364",
            ),
            (
                "engineer:custom_engineering".to_string(),
                "bca7c1f189383eb464fe7c943084131c6ca3f5f6eb1c119007daca20133d0048",
            ),
            (
                "agent_create:custom_agent_builder".to_string(),
                "ab51cda758dab0a43452b2ae48e3817638467bfe5cf390bee9acef9087084a1e",
            ),
        ];

        assert_eq!(actual, expected);
    }
}
