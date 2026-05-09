use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const DOMAIN_TOOL_PACK_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackManifest {
    pub contract_version: u32,
    pub pack_id: String,
    pub label: String,
    pub summary: String,
    pub policy_profile: String,
    pub tool_groups: Vec<String>,
    pub tools: Vec<String>,
    pub capabilities: Vec<String>,
    pub allowed_effect_classes: Vec<String>,
    pub denied_effect_classes: Vec<String>,
    pub review_requirements: Vec<DomainToolPackReviewRequirement>,
    pub prerequisites: Vec<DomainToolPackPrerequisite>,
    pub health_checks: Vec<DomainToolPackCheckDescriptor>,
    pub scenario_checks: Vec<DomainToolPackScenarioDescriptor>,
    pub ui_affordances: Vec<DomainToolPackUiAffordance>,
    pub cli_commands: Vec<String>,
    pub approval_boundaries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackReviewRequirement {
    pub requirement_id: String,
    pub label: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackPrerequisite {
    pub prerequisite_id: String,
    pub label: String,
    pub kind: String,
    pub required: bool,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackCheckDescriptor {
    pub check_id: String,
    pub label: String,
    pub description: String,
    pub prerequisite_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackScenarioDescriptor {
    pub scenario_id: String,
    pub label: String,
    pub description: String,
    pub tool_names: Vec<String>,
    pub mutating: bool,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackUiAffordance {
    pub surface: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DomainToolPackHealthStatus {
    Passed,
    Warning,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackHealthDiagnostic {
    pub code: String,
    pub message: String,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackHealthCheck {
    pub check_id: String,
    pub label: String,
    pub status: DomainToolPackHealthStatus,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<DomainToolPackHealthDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackScenarioCheck {
    pub scenario_id: String,
    pub label: String,
    pub status: DomainToolPackHealthStatus,
    pub summary: String,
    pub tool_names: Vec<String>,
    pub mutating: bool,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackHealthReport {
    pub contract_version: u32,
    pub pack_id: String,
    pub label: String,
    pub enabled_by_policy: bool,
    pub status: DomainToolPackHealthStatus,
    pub checked_at: String,
    pub checks: Vec<DomainToolPackHealthCheck>,
    pub scenario_checks: Vec<DomainToolPackScenarioCheck>,
    pub missing_prerequisites: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainToolPackHealthInput {
    pub pack_id: String,
    pub enabled_by_policy: bool,
    pub available_prerequisites: Vec<String>,
    pub checked_at: String,
}

pub fn domain_tool_pack_manifests() -> Vec<DomainToolPackManifest> {
    vec![
        browser_pack_manifest(),
        emulator_pack_manifest(),
        solana_pack_manifest(),
        os_automation_pack_manifest(),
        project_context_pack_manifest(),
    ]
}

pub fn domain_tool_pack_manifest(pack_id: &str) -> Option<DomainToolPackManifest> {
    domain_tool_pack_manifests()
        .into_iter()
        .find(|manifest| manifest.pack_id == pack_id)
}

pub fn domain_tool_pack_ids() -> Vec<String> {
    domain_tool_pack_manifests()
        .into_iter()
        .map(|manifest| manifest.pack_id)
        .collect()
}

pub fn domain_tool_pack_tools(pack_id: &str) -> Option<Vec<String>> {
    domain_tool_pack_manifest(pack_id).map(|manifest| manifest.tools)
}

pub fn domain_tool_pack_ids_for_tool(tool_name: &str) -> Vec<String> {
    domain_tool_pack_manifests()
        .into_iter()
        .filter(|manifest| manifest.tools.iter().any(|tool| tool == tool_name))
        .map(|manifest| manifest.pack_id)
        .collect()
}

pub fn domain_tool_pack_health_report(
    manifest: &DomainToolPackManifest,
    input: &DomainToolPackHealthInput,
) -> DomainToolPackHealthReport {
    if !input.enabled_by_policy {
        return DomainToolPackHealthReport {
            contract_version: DOMAIN_TOOL_PACK_CONTRACT_VERSION,
            pack_id: manifest.pack_id.clone(),
            label: manifest.label.clone(),
            enabled_by_policy: false,
            status: DomainToolPackHealthStatus::Skipped,
            checked_at: input.checked_at.clone(),
            checks: vec![DomainToolPackHealthCheck {
                check_id: "tool_pack_policy_disabled".into(),
                label: "Agent policy".into(),
                status: DomainToolPackHealthStatus::Skipped,
                summary: format!(
                    "Tool pack `{}` is disabled by the active agent policy.",
                    manifest.pack_id
                ),
                diagnostic: None,
            }],
            scenario_checks: manifest
                .scenario_checks
                .iter()
                .map(|scenario| DomainToolPackScenarioCheck {
                    scenario_id: scenario.scenario_id.clone(),
                    label: scenario.label.clone(),
                    status: DomainToolPackHealthStatus::Skipped,
                    summary: "Scenario check skipped because the pack is disabled by policy."
                        .into(),
                    tool_names: scenario.tool_names.clone(),
                    mutating: scenario.mutating,
                    requires_approval: scenario.requires_approval,
                })
                .collect(),
            missing_prerequisites: Vec::new(),
        };
    }

    let available = input
        .available_prerequisites
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut missing_prerequisites = Vec::new();
    let checks = manifest
        .prerequisites
        .iter()
        .map(|prerequisite| {
            let present = available.contains(prerequisite.prerequisite_id.as_str());
            let status = if present {
                DomainToolPackHealthStatus::Passed
            } else if prerequisite.required {
                missing_prerequisites.push(prerequisite.prerequisite_id.clone());
                DomainToolPackHealthStatus::Failed
            } else {
                DomainToolPackHealthStatus::Warning
            };
            let diagnostic = (!present).then(|| DomainToolPackHealthDiagnostic {
                code: format!("tool_pack_{}_missing", prerequisite.prerequisite_id),
                message: format!(
                    "Tool pack `{}` is missing prerequisite `{}`.",
                    manifest.pack_id, prerequisite.label
                ),
                remediation: prerequisite.remediation.clone(),
            });
            DomainToolPackHealthCheck {
                check_id: prerequisite.prerequisite_id.clone(),
                label: prerequisite.label.clone(),
                status,
                summary: if present {
                    format!("Prerequisite `{}` is available.", prerequisite.label)
                } else if prerequisite.required {
                    format!("Required prerequisite `{}` is missing.", prerequisite.label)
                } else {
                    format!("Optional prerequisite `{}` is missing.", prerequisite.label)
                },
                diagnostic,
            }
        })
        .collect::<Vec<_>>();

    let status = summarize_pack_status(&checks);
    let scenario_checks = manifest
        .scenario_checks
        .iter()
        .map(|scenario| DomainToolPackScenarioCheck {
            scenario_id: scenario.scenario_id.clone(),
            label: scenario.label.clone(),
            status: if status == DomainToolPackHealthStatus::Failed {
                DomainToolPackHealthStatus::Failed
            } else {
                DomainToolPackHealthStatus::Passed
            },
            summary: if status == DomainToolPackHealthStatus::Failed {
                "Scenario cannot run until required prerequisites are repaired.".into()
            } else {
                "Scenario prerequisites are available; workflow can be checked with the listed tools."
                    .into()
            },
            tool_names: scenario.tool_names.clone(),
            mutating: scenario.mutating,
            requires_approval: scenario.requires_approval,
        })
        .collect();

    DomainToolPackHealthReport {
        contract_version: DOMAIN_TOOL_PACK_CONTRACT_VERSION,
        pack_id: manifest.pack_id.clone(),
        label: manifest.label.clone(),
        enabled_by_policy: true,
        status,
        checked_at: input.checked_at.clone(),
        checks,
        scenario_checks,
        missing_prerequisites,
    }
}

fn summarize_pack_status(checks: &[DomainToolPackHealthCheck]) -> DomainToolPackHealthStatus {
    if checks
        .iter()
        .any(|check| check.status == DomainToolPackHealthStatus::Failed)
    {
        DomainToolPackHealthStatus::Failed
    } else if checks
        .iter()
        .any(|check| check.status == DomainToolPackHealthStatus::Warning)
    {
        DomainToolPackHealthStatus::Warning
    } else {
        DomainToolPackHealthStatus::Passed
    }
}

fn browser_pack_manifest() -> DomainToolPackManifest {
    manifest(
        "browser",
        "Browser",
        "Observe and control the in-app browser with screenshots, DOM snapshots, accessibility, console, network, storage, and tab state.",
        "browser_control_with_observe_split",
        &["browser_observe", "browser_control", "web"],
        &["browser_observe", "browser_control"],
        &[
            "observe_control_split",
            "screenshot_capture",
            "interaction_trace",
            "dom_snapshot_tools",
            "browser_state_restore",
        ],
        &["observe", "browser_control"],
        &[
            "destructive_write",
            "command",
            "device_control",
            "agent_delegation",
        ],
        &[review(
            "browser_control_operator_intent",
            "Browser control approval",
            "Typing, clicking, navigation, cookie, and storage actions require clear operator-visible intent.",
            true,
        )],
        &[
            prereq(
                "desktop_browser_executor",
                "In-app browser executor",
                "service",
                true,
                "Start Xero's desktop runtime with the in-app browser bridge enabled.",
            ),
            prereq(
                "webview_runtime",
                "WebView runtime",
                "service",
                true,
                "Install or repair the platform WebView runtime used by Tauri.",
            ),
        ],
        &[
            scenario(
                "browser_observe_page",
                "Observe page state",
                "Open or focus a tab, read page text, capture a screenshot, and inspect accessibility output.",
                &["browser_observe"],
                false,
                false,
            ),
            scenario(
                "browser_interaction_trace",
                "Interaction trace",
                "Navigate, click, type, and collect console or network evidence for the interaction.",
                &["browser_control", "browser_observe"],
                true,
                true,
            ),
        ],
        &[ui("browser_sidebar", "Browser sidebar"), ui("activity_trace", "Interaction trace")],
        &["xero tool-pack doctor browser"],
        &[
            "Typing, clicking, cookies, storage writes, and navigation can affect remote or local web state.",
            "Use observe actions before control actions whenever possible.",
        ],
    )
}

fn emulator_pack_manifest() -> DomainToolPackManifest {
    manifest(
        "emulator",
        "Emulator",
        "Drive Android and iOS emulator workflows with device lifecycle, app install or launch, frame capture, gestures, input, location, push, and logs.",
        "device_control",
        &["emulator"],
        &["emulator"],
        &[
            "device_lifecycle",
            "app_install_launch",
            "frame_capture",
            "gesture_input",
            "log_capture",
        ],
        &["observe", "device_control"],
        &[
            "browser_control",
            "destructive_write",
            "external_service",
            "agent_delegation",
        ],
        &[review(
            "device_control_operator_intent",
            "Device control approval",
            "Launching apps, installing builds, gestures, input, location, and push actions require operator-visible intent.",
            true,
        )],
        &[
            prereq(
                "desktop_emulator_executor",
                "Desktop emulator executor",
                "service",
                true,
                "Start Xero's desktop runtime so emulator commands can reach the device bridge.",
            ),
            prereq(
                "adb",
                "Android Debug Bridge",
                "binary",
                false,
                "Install Android platform-tools or let Xero provision the Android SDK.",
            ),
            prereq(
                "xcrun",
                "Xcode command tools",
                "binary",
                false,
                "Install Xcode command line tools for iOS Simulator support.",
            ),
        ],
        &[
            scenario(
                "emulator_launch_capture_logs",
                "Launch, capture, and inspect logs",
                "Start or select a device, launch an app, capture a frame, perform a basic gesture, and fetch logs.",
                &["emulator"],
                true,
                true,
            ),
            scenario(
                "emulator_repro_evidence",
                "Mobile repro evidence",
                "Collect screen evidence and logs around a focused app reproduction.",
                &["emulator"],
                false,
                false,
            ),
        ],
        &[
            ui("emulator_sidebar", "Emulator sidebar"),
            ui("device_frame", "Live device frame"),
        ],
        &["xero tool-pack doctor emulator"],
        &[
            "Device input and app lifecycle actions can change simulator state.",
            "Installing apps and sending notifications require operator-visible intent.",
        ],
    )
}

fn solana_pack_manifest() -> DomainToolPackManifest {
    manifest(
        "solana",
        "Solana",
        "Inspect and operate Solana project workflows with wallet boundaries, network selection, simulation, program build or deploy checks, audit tooling, and guarded signing.",
        "chain_safe_external_service",
        &["solana"],
        &[
            "solana_cluster",
            "solana_logs",
            "solana_tx",
            "solana_simulate",
            "solana_explain",
            "solana_alt",
            "solana_idl",
            "solana_codama",
            "solana_pda",
            "solana_program",
            "solana_deploy",
            "solana_upgrade_check",
            "solana_squads",
            "solana_verified_build",
            "solana_audit_static",
            "solana_audit_external",
            "solana_audit_fuzz",
            "solana_audit_coverage",
            "solana_replay",
            "solana_indexer",
            "solana_secrets",
            "solana_cluster_drift",
            "solana_cost",
            "solana_docs",
        ],
        &[
            "wallet_safety_boundaries",
            "network_selection",
            "transaction_simulation",
            "program_test_workflow",
            "explicit_signing_approval",
        ],
        &["observe", "external_service", "command"],
        &["browser_control", "device_control", "agent_delegation"],
        &[review(
            "chain_mutation_explicit_approval",
            "Chain mutation approval",
            "Signing, deploy, transfer, upgrade, or value-moving paths require explicit user approval after simulation.",
            true,
        )],
        &[
            prereq(
                "solana_state_executor",
                "Solana desktop state executor",
                "service",
                true,
                "Start Xero's desktop runtime with the Solana workbench state initialized.",
            ),
            prereq(
                "solana",
                "Solana CLI",
                "binary",
                false,
                "Install the Solana CLI for local validator, keypair, and program workflows.",
            ),
            prereq(
                "anchor",
                "Anchor CLI",
                "binary",
                false,
                "Install Anchor when working on Anchor programs.",
            ),
        ],
        &[
            scenario(
                "solana_simulate_before_send",
                "Simulate before send",
                "Resolve network and wallet scope, simulate a transaction, and explain logs before any send path.",
                &["solana_simulate", "solana_explain", "solana_tx"],
                false,
                false,
            ),
            scenario(
                "solana_guarded_program_workflow",
                "Guarded program workflow",
                "Build or inspect a program, check upgrade safety, run audit evidence, and require approval for deploy or signing.",
                &[
                    "solana_program",
                    "solana_upgrade_check",
                    "solana_audit_static",
                    "solana_deploy",
                ],
                true,
                true,
            ),
        ],
        &[
            ui("solana_workbench", "Solana workbench"),
            ui("wallet_safety_panel", "Wallet safety panel"),
        ],
        &["xero tool-pack doctor solana"],
        &[
            "Signing, deploy, transfer, and value-moving paths require explicit user approval.",
            "Simulation and read-only inspection should precede chain-affecting actions.",
        ],
    )
}

fn os_automation_pack_manifest() -> DomainToolPackManifest {
    manifest(
        "os_automation",
        "OS Automation",
        "Inspect and control local OS surfaces through macOS app or window automation, process diagnostics, screenshots, and bounded process management.",
        "local_os_control",
        &[
            "macos",
            "system_diagnostics_observe",
            "system_diagnostics_privileged",
            "process_manager",
        ],
        &[
            "macos_automation",
            "system_diagnostics_observe",
            "system_diagnostics_privileged",
            "process_manager",
        ],
        &[
            "permission_check",
            "app_window_control",
            "screenshot_capture",
            "process_diagnostics",
            "approval_gated_external_signals",
        ],
        &["observe", "process_control", "command"],
        &["browser_control", "device_control", "agent_delegation"],
        &[review(
            "os_control_operator_intent",
            "OS control approval",
            "App focus, screenshots, process signaling, and privileged diagnostics require clear operator-visible intent.",
            true,
        )],
        &[
            prereq(
                "desktop_runtime",
                "Desktop runtime",
                "service",
                true,
                "Run OS automation from the Xero desktop app.",
            ),
            prereq(
                "macos_platform",
                "macOS platform",
                "platform",
                true,
                "Use macOS for the current OS automation pack, or add a platform-specific pack.",
            ),
            prereq(
                "accessibility_permission",
                "Accessibility permission",
                "permission",
                false,
                "Grant Accessibility permission in System Settings when app or window control is needed.",
            ),
            prereq(
                "screen_recording_permission",
                "Screen recording permission",
                "permission",
                false,
                "Grant Screen Recording permission when screenshots are needed.",
            ),
        ],
        &[scenario(
            "os_focus_and_capture",
            "Focus and capture",
            "Check permissions, list apps and windows, focus a target, and capture a screenshot after approval.",
            &["macos_automation", "system_diagnostics_observe"],
            true,
            true,
        )],
        &[
            ui("diagnostics_panel", "Diagnostics panel"),
            ui("operator_approval", "Operator approval sheet"),
        ],
        &["xero tool-pack doctor os_automation"],
        &[
            "Screenshots, app activation, window focus, and process signaling require clear operator intent.",
            "External process signals are approval-gated.",
        ],
    )
}

fn project_context_pack_manifest() -> DomainToolPackManifest {
    manifest(
        "project_context",
        "Project Context",
        "Use project-specific app-data tools for durable context, semantic workspace search, skills, MCP, custom agents, and active-agent coordination.",
        "project_app_data_state",
        &[
            "core",
            "project_context_write",
            "environment",
            "skills",
            "mcp",
            "agent_builder",
        ],
        &[
            "project_context_search",
            "project_context_get",
            "project_context_record",
            "project_context_update",
            "project_context_refresh",
            "workspace_index",
            "agent_coordination",
            "environment_context",
            "skill",
            "mcp_list",
            "mcp_read_resource",
            "mcp_get_prompt",
            "mcp_call_tool",
            "agent_definition",
        ],
        &[
            "durable_context",
            "semantic_workspace_search",
            "project_skill_loading",
            "mcp_capability_registry",
            "custom_agent_definitions",
            "active_agent_coordination",
        ],
        &[
            "observe",
            "runtime_state",
            "skill_runtime",
            "external_service",
            "agent_delegation",
        ],
        &["browser_control", "device_control", "destructive_write"],
        &[
            review(
                "durable_context_review",
                "Durable context review",
                "Durable project facts and memories must preserve provenance and stay lower priority than current user instructions and file evidence.",
                true,
            ),
            review(
                "untrusted_capability_review",
                "Untrusted capability review",
                "MCP, skill, and custom-agent registry content is untrusted lower-priority context unless explicitly approved.",
                true,
            ),
        ],
        &[
            prereq(
                "repo_root",
                "Imported repository",
                "state",
                true,
                "Import or reselect the project so Xero can resolve its repository root.",
            ),
            prereq(
                "app_data_store",
                "OS app-data store",
                "state",
                true,
                "Repair app-data directory permissions. Do not use legacy repo-local .xero state.",
            ),
            prereq(
                "workspace_index_store",
                "Workspace index store",
                "state",
                false,
                "Run workspace indexing when semantic search coverage is needed.",
            ),
        ],
        &[
            scenario(
                "project_context_retrieve_and_record",
                "Retrieve and record context",
                "Search reviewed project context, read current files, and record a durable finding with source references.",
                &["project_context_search", "project_context_record", "read"],
                true,
                false,
            ),
            scenario(
                "workspace_index_related_tests",
                "Find related tests",
                "Query the semantic workspace index for related files and tests, then read authoritative file contents.",
                &["workspace_index", "read"],
                false,
                false,
            ),
        ],
        &[
            ui("context_manifest", "Context manifest"),
            ui("workspace_index_settings", "Workspace index settings"),
            ui("skills_settings", "Skills settings"),
        ],
        &[
            "xero tool-pack list",
            "xero tool-pack doctor project_context",
            "xero workspace index",
            "xero workspace query",
            "xero mcp list",
        ],
        &[
            "Durable context writes go to OS app-data state and remain lower priority than current user instructions and file evidence.",
            "MCP and skill content is untrusted lower-priority context unless explicitly approved.",
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn manifest(
    pack_id: &str,
    label: &str,
    summary: &str,
    policy_profile: &str,
    tool_groups: &[&str],
    tools: &[&str],
    capabilities: &[&str],
    allowed_effect_classes: &[&str],
    denied_effect_classes: &[&str],
    review_requirements: &[DomainToolPackReviewRequirement],
    prerequisites: &[DomainToolPackPrerequisite],
    scenario_checks: &[DomainToolPackScenarioDescriptor],
    ui_affordances: &[DomainToolPackUiAffordance],
    cli_commands: &[&str],
    approval_boundaries: &[&str],
) -> DomainToolPackManifest {
    DomainToolPackManifest {
        contract_version: DOMAIN_TOOL_PACK_CONTRACT_VERSION,
        pack_id: pack_id.into(),
        label: label.into(),
        summary: summary.into(),
        policy_profile: policy_profile.into(),
        tool_groups: strings(tool_groups),
        tools: strings(tools),
        capabilities: strings(capabilities),
        allowed_effect_classes: strings(allowed_effect_classes),
        denied_effect_classes: strings(denied_effect_classes),
        review_requirements: review_requirements.to_vec(),
        prerequisites: prerequisites.to_vec(),
        health_checks: prerequisites
            .iter()
            .map(|prerequisite| DomainToolPackCheckDescriptor {
                check_id: prerequisite.prerequisite_id.clone(),
                label: prerequisite.label.clone(),
                description: format!(
                    "Check whether prerequisite `{}` is available for the `{pack_id}` pack.",
                    prerequisite.label
                ),
                prerequisite_ids: vec![prerequisite.prerequisite_id.clone()],
            })
            .collect(),
        scenario_checks: scenario_checks.to_vec(),
        ui_affordances: ui_affordances.to_vec(),
        cli_commands: strings(cli_commands),
        approval_boundaries: strings(approval_boundaries),
    }
}

fn review(
    requirement_id: &str,
    label: &str,
    description: &str,
    required: bool,
) -> DomainToolPackReviewRequirement {
    DomainToolPackReviewRequirement {
        requirement_id: requirement_id.into(),
        label: label.into(),
        description: description.into(),
        required,
    }
}

fn prereq(
    prerequisite_id: &str,
    label: &str,
    kind: &str,
    required: bool,
    remediation: &str,
) -> DomainToolPackPrerequisite {
    DomainToolPackPrerequisite {
        prerequisite_id: prerequisite_id.into(),
        label: label.into(),
        kind: kind.into(),
        required,
        remediation: remediation.into(),
    }
}

fn scenario(
    scenario_id: &str,
    label: &str,
    description: &str,
    tool_names: &[&str],
    mutating: bool,
    requires_approval: bool,
) -> DomainToolPackScenarioDescriptor {
    DomainToolPackScenarioDescriptor {
        scenario_id: scenario_id.into(),
        label: label.into(),
        description: description.into(),
        tool_names: strings(tool_names),
        mutating,
        requires_approval,
    }
}

fn ui(surface: &str, label: &str) -> DomainToolPackUiAffordance {
    DomainToolPackUiAffordance {
        surface: surface.into(),
        label: label.into(),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_tool_pack_manifests_cover_xero_domain_surfaces() {
        let manifests = domain_tool_pack_manifests();
        let ids = manifests
            .iter()
            .map(|manifest| manifest.pack_id.as_str())
            .collect::<BTreeSet<_>>();

        assert!(ids.contains("browser"));
        assert!(ids.contains("emulator"));
        assert!(ids.contains("solana"));
        assert!(ids.contains("os_automation"));
        assert!(ids.contains("project_context"));

        for manifest in manifests {
            assert_eq!(manifest.contract_version, DOMAIN_TOOL_PACK_CONTRACT_VERSION);
            assert!(!manifest.tools.is_empty());
            assert!(!manifest.allowed_effect_classes.is_empty());
            assert!(!manifest.review_requirements.is_empty());
            assert!(!manifest.prerequisites.is_empty());
            assert!(!manifest.health_checks.is_empty());
            assert!(!manifest.scenario_checks.is_empty());
            assert!(!manifest.ui_affordances.is_empty());
        }
    }

    #[test]
    fn s21_domain_tool_pack_manifests_declare_policy_boundaries() {
        let browser = domain_tool_pack_manifest("browser").expect("browser pack");
        assert!(browser
            .allowed_effect_classes
            .contains(&"browser_control".to_string()));
        assert!(browser
            .denied_effect_classes
            .contains(&"destructive_write".to_string()));
        assert!(browser
            .review_requirements
            .iter()
            .any(|requirement| requirement.required
                && requirement.requirement_id == "browser_control_operator_intent"));

        let project_context = domain_tool_pack_manifest("project_context").expect("context pack");
        assert!(project_context
            .allowed_effect_classes
            .contains(&"runtime_state".to_string()));
        assert!(project_context
            .review_requirements
            .iter()
            .any(|requirement| requirement.requirement_id == "durable_context_review"));
        assert!(project_context
            .review_requirements
            .iter()
            .all(|requirement| !requirement.description.trim().is_empty()));
    }

    #[test]
    fn domain_tool_pack_health_reports_missing_required_prerequisites() {
        let manifest = domain_tool_pack_manifest("browser").expect("browser pack");
        let report = domain_tool_pack_health_report(
            &manifest,
            &DomainToolPackHealthInput {
                pack_id: "browser".into(),
                enabled_by_policy: true,
                available_prerequisites: vec!["webview_runtime".into()],
                checked_at: "2026-05-04T00:00:00Z".into(),
            },
        );

        assert_eq!(report.status, DomainToolPackHealthStatus::Failed);
        assert!(report
            .missing_prerequisites
            .contains(&"desktop_browser_executor".to_string()));
        assert!(report.checks.iter().any(|check| check
            .diagnostic
            .as_ref()
            .is_some_and(|diagnostic| diagnostic.code.contains("desktop_browser_executor"))));
    }

    #[test]
    fn domain_tool_pack_health_respects_policy_disabled_state() {
        let manifest = domain_tool_pack_manifest("solana").expect("solana pack");
        let report = domain_tool_pack_health_report(
            &manifest,
            &DomainToolPackHealthInput {
                pack_id: "solana".into(),
                enabled_by_policy: false,
                available_prerequisites: Vec::new(),
                checked_at: "2026-05-04T00:00:00Z".into(),
            },
        );

        assert_eq!(report.status, DomainToolPackHealthStatus::Skipped);
        assert!(report.checks[0]
            .summary
            .contains("disabled by the active agent policy"));
        assert!(report
            .scenario_checks
            .iter()
            .all(|check| check.status == DomainToolPackHealthStatus::Skipped));
    }

    #[test]
    fn maps_tools_back_to_domain_pack_ids() {
        assert_eq!(
            domain_tool_pack_ids_for_tool("browser_observe"),
            vec!["browser"]
        );
        assert_eq!(
            domain_tool_pack_ids_for_tool("browser_control"),
            vec!["browser"]
        );
        assert_eq!(domain_tool_pack_ids_for_tool("emulator"), vec!["emulator"]);
        assert_eq!(
            domain_tool_pack_ids_for_tool("solana_simulate"),
            vec!["solana"]
        );
        assert!(domain_tool_pack_ids_for_tool("read").is_empty());
    }
}
