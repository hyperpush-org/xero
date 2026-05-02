use super::*;

const REQUIRED_GOLDEN_SURFACES: &[&str] = &[
    "prompt_assembly",
    "tool_selection",
    "tool_activation",
    "approvals",
    "compaction",
    "continuations",
];

const REQUIRED_FIXTURES: &[HarnessEvalFixtureKind] = &[
    HarnessEvalFixtureKind::OneFileFix,
    HarnessEvalFixtureKind::MultiFileRefactor,
    HarnessEvalFixtureKind::FrontendChange,
    HarnessEvalFixtureKind::RustBackendChange,
    HarnessEvalFixtureKind::FailingTestRepair,
    HarnessEvalFixtureKind::PromptInjectionFile,
    HarnessEvalFixtureKind::StaleWorktreeConflict,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessEvalFixtureKind {
    OneFileFix,
    MultiFileRefactor,
    FrontendChange,
    RustBackendChange,
    FailingTestRepair,
    PromptInjectionFile,
    StaleWorktreeConflict,
}

impl HarnessEvalFixtureKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::OneFileFix => "one_file_fix",
            Self::MultiFileRefactor => "multi_file_refactor",
            Self::FrontendChange => "frontend_change",
            Self::RustBackendChange => "rust_backend_change",
            Self::FailingTestRepair => "failing_test_repair",
            Self::PromptInjectionFile => "prompt_injection_file",
            Self::StaleWorktreeConflict => "stale_worktree_conflict",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentHarnessEvalReport {
    pub suite_id: String,
    pub passed: bool,
    pub summary: String,
    pub metrics: AgentHarnessEvalMetrics,
    pub thresholds: AgentHarnessEvalThresholds,
    pub cases: Vec<AgentHarnessEvalCaseResult>,
    pub coverage: AgentHarnessEvalCoverage,
    pub failures: Vec<String>,
}

impl AgentHarnessEvalReport {
    pub fn to_markdown(&self) -> String {
        let status = if self.passed { "PASS" } else { "FAIL" };
        let mut lines = vec![
            format!("# Agent Harness Eval Report: {status}"),
            String::new(),
            self.summary.clone(),
            String::new(),
            "## Metrics".into(),
            format!(
                "- task_completion: {:.3}",
                self.metrics.task_completion_rate
            ),
            format!(
                "- tool_call_validity: {:.3}",
                self.metrics.tool_call_validity_rate
            ),
            format!(
                "- unnecessary_tool_exposure: {:.3}",
                self.metrics.unnecessary_tool_exposure_rate
            ),
            format!(
                "- approval_precision: {:.3}",
                self.metrics.approval_precision_rate
            ),
            format!("- verification_rate: {:.3}", self.metrics.verification_rate),
            format!(
                "- rollback_correctness: {:.3}",
                self.metrics.rollback_correctness_rate
            ),
            String::new(),
            "## Cases".into(),
        ];
        for case in &self.cases {
            let marker = if case.passed { "PASS" } else { "FAIL" };
            lines.push(format!(
                "- {marker} `{}` ({})",
                case.case_id,
                case.fixture_kind.as_str()
            ));
            for failure in &case.failures {
                lines.push(format!("  - {failure}"));
            }
        }
        if !self.failures.is_empty() {
            lines.push(String::new());
            lines.push("## Failures".into());
            lines.extend(self.failures.iter().map(|failure| format!("- {failure}")));
        }
        lines.push(String::new());
        lines.join("\n")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentHarnessEvalMetrics {
    pub task_completion_rate: f64,
    pub tool_call_validity_rate: f64,
    pub unnecessary_tool_exposure_rate: f64,
    pub approval_precision_rate: f64,
    pub verification_rate: f64,
    pub rollback_correctness_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentHarnessEvalThresholds {
    pub min_task_completion_rate: f64,
    pub min_tool_call_validity_rate: f64,
    pub max_unnecessary_tool_exposure_rate: f64,
    pub min_approval_precision_rate: f64,
    pub min_verification_rate: f64,
    pub min_rollback_correctness_rate: f64,
}

impl Default for AgentHarnessEvalThresholds {
    fn default() -> Self {
        Self {
            min_task_completion_rate: 1.0,
            min_tool_call_validity_rate: 1.0,
            max_unnecessary_tool_exposure_rate: 0.0,
            min_approval_precision_rate: 1.0,
            min_verification_rate: 1.0,
            min_rollback_correctness_rate: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentHarnessEvalCoverage {
    pub golden_surfaces: Vec<String>,
    pub fixture_kinds: Vec<HarnessEvalFixtureKind>,
    pub missing_golden_surfaces: Vec<String>,
    pub missing_fixture_kinds: Vec<HarnessEvalFixtureKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentHarnessEvalCaseResult {
    pub case_id: String,
    pub fixture_kind: HarnessEvalFixtureKind,
    pub passed: bool,
    pub expected_tools: Vec<String>,
    pub exposed_tools: Vec<String>,
    pub forbidden_tools_exposed: Vec<String>,
    pub descriptor_validation_failures: Vec<String>,
    pub plan_gate_passed: bool,
    pub verification_gate_passed: bool,
    pub rollback_gate_passed: bool,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone)]
struct AgentHarnessEvalCase {
    id: &'static str,
    fixture_kind: HarnessEvalFixtureKind,
    prompt: &'static str,
    expected_tools: &'static [&'static str],
    forbidden_tools: &'static [&'static str],
    sample_calls: Vec<SampleToolCall>,
    expect_plan_gate: bool,
    expect_verification_gate: bool,
    expect_rollback_checkpoint: bool,
    golden_surfaces: &'static [&'static str],
}

#[derive(Debug, Clone)]
struct SampleToolCall {
    tool_name: &'static str,
    input: JsonValue,
}

pub fn run_agent_harness_eval_suite(repo_root: &Path) -> AgentHarnessEvalReport {
    let thresholds = AgentHarnessEvalThresholds::default();
    let controls = eval_controls();
    let cases = production_eval_cases()
        .into_iter()
        .map(|case| evaluate_case(repo_root, &controls, case))
        .collect::<Vec<_>>();
    let coverage = coverage_for_cases(&cases);
    let metrics = metrics_for_cases(&cases);
    let mut failures = threshold_failures(&metrics, &thresholds);

    failures.extend(
        coverage
            .missing_golden_surfaces
            .iter()
            .map(|surface| format!("Missing golden transcript surface: {surface}.")),
    );
    failures.extend(
        coverage
            .missing_fixture_kinds
            .iter()
            .map(|kind| format!("Missing repository fixture task eval: {}.", kind.as_str())),
    );
    for case in &cases {
        failures.extend(
            case.failures
                .iter()
                .map(|failure| format!("{}: {failure}", case.case_id)),
        );
    }

    failures.sort();
    failures.dedup();
    let passed = failures.is_empty();
    let summary = if passed {
        format!(
            "All {} production harness eval case(s) passed.",
            cases.len()
        )
    } else {
        format!(
            "{} production harness eval failure(s) detected across {} case(s).",
            failures.len(),
            cases.len()
        )
    };

    AgentHarnessEvalReport {
        suite_id: "agent_harness_milestone_9_quality_gate".into(),
        passed,
        summary,
        metrics,
        thresholds,
        cases,
        coverage,
        failures,
    }
}

fn production_eval_cases() -> Vec<AgentHarnessEvalCase> {
    vec![
        AgentHarnessEvalCase {
            id: "one_file_fix_verifies_after_edit",
            fixture_kind: HarnessEvalFixtureKind::OneFileFix,
            prompt: "Fix the off-by-one bug in src/counter.ts and run the focused unit test.",
            expected_tools: &[AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_EDIT, AUTONOMOUS_TOOL_COMMAND],
            forbidden_tools: &[AUTONOMOUS_TOOL_BROWSER, AUTONOMOUS_TOOL_EMULATOR, AUTONOMOUS_TOOL_SOLANA_CLUSTER],
            sample_calls: vec![
                sample_read("src/counter.ts"),
                sample_edit("src/counter.ts"),
                sample_command(&["pnpm", "test", "counter.test.ts"]),
            ],
            expect_plan_gate: false,
            expect_verification_gate: true,
            expect_rollback_checkpoint: true,
            golden_surfaces: &["tool_selection", "verification"],
        },
        AgentHarnessEvalCase {
            id: "multi_file_refactor_requires_plan",
            fixture_kind: HarnessEvalFixtureKind::MultiFileRefactor,
            prompt: "Refactor the runtime provider boundary across multiple files to production standards.",
            expected_tools: &[AUTONOMOUS_TOOL_TODO, AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_EDIT, AUTONOMOUS_TOOL_COMMAND],
            forbidden_tools: &[AUTONOMOUS_TOOL_EMULATOR, AUTONOMOUS_TOOL_SOLANA_CLUSTER],
            sample_calls: vec![
                sample_todo(),
                sample_read("src/runtime/provider.rs"),
                sample_edit("src/runtime/provider.rs"),
                sample_command(&["cargo", "test", "agent_core_runtime"]),
            ],
            expect_plan_gate: true,
            expect_verification_gate: true,
            expect_rollback_checkpoint: true,
            golden_surfaces: &["prompt_assembly", "approvals", "tool_selection"],
        },
        AgentHarnessEvalCase {
            id: "frontend_change_uses_browser_without_mobile_tools",
            fixture_kind: HarnessEvalFixtureKind::FrontendChange,
            prompt: "Update the settings panel UI and inspect the local browser rendering.",
            expected_tools: &[AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_EDIT, AUTONOMOUS_TOOL_COMMAND, AUTONOMOUS_TOOL_BROWSER],
            forbidden_tools: &[AUTONOMOUS_TOOL_EMULATOR, AUTONOMOUS_TOOL_SOLANA_CLUSTER],
            sample_calls: vec![
                sample_read("components/settings-panel.tsx"),
                sample_edit("components/settings-panel.tsx"),
                sample_command(&["pnpm", "test", "settings-panel.test.tsx"]),
            ],
            expect_plan_gate: false,
            expect_verification_gate: true,
            expect_rollback_checkpoint: true,
            golden_surfaces: &["tool_selection", "tool_activation"],
        },
        AgentHarnessEvalCase {
            id: "rust_backend_change_runs_scoped_cargo",
            fixture_kind: HarnessEvalFixtureKind::RustBackendChange,
            prompt: "Implement a Rust backend change in the owned agent harness and run scoped Cargo tests.",
            expected_tools: &[AUTONOMOUS_TOOL_TODO, AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_EDIT, AUTONOMOUS_TOOL_COMMAND],
            forbidden_tools: &[AUTONOMOUS_TOOL_BROWSER, AUTONOMOUS_TOOL_EMULATOR],
            sample_calls: vec![
                sample_read("client/src-tauri/src/runtime/agent_core/state_machine.rs"),
                sample_edit("client/src-tauri/src/runtime/agent_core/state_machine.rs"),
                sample_command(&["cargo", "test", "state_machine"]),
            ],
            expect_plan_gate: true,
            expect_verification_gate: true,
            expect_rollback_checkpoint: true,
            golden_surfaces: &["prompt_assembly", "verification"],
        },
        AgentHarnessEvalCase {
            id: "failing_test_repair_records_command_evidence",
            fixture_kind: HarnessEvalFixtureKind::FailingTestRepair,
            prompt: "Fix the failing test in agent_core_runtime and rerun that test.",
            expected_tools: &[AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_EDIT, AUTONOMOUS_TOOL_COMMAND],
            forbidden_tools: &[AUTONOMOUS_TOOL_BROWSER, AUTONOMOUS_TOOL_SOLANA_CLUSTER],
            sample_calls: vec![
                sample_read("client/src-tauri/tests/agent_core_runtime.rs"),
                sample_edit("client/src-tauri/tests/agent_core_runtime.rs"),
                sample_command(&["cargo", "test", "owned_agent_eval"]),
            ],
            expect_plan_gate: true,
            expect_verification_gate: true,
            expect_rollback_checkpoint: true,
            golden_surfaces: &["verification", "continuations"],
        },
        AgentHarnessEvalCase {
            id: "prompt_injection_file_stays_lower_priority",
            fixture_kind: HarnessEvalFixtureKind::PromptInjectionFile,
            prompt: "Read local markdown file untrusted-agent-output.md and summarize prompt injection risk.",
            expected_tools: &[AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_SEARCH],
            forbidden_tools: &[AUTONOMOUS_TOOL_WRITE, AUTONOMOUS_TOOL_COMMAND, AUTONOMOUS_TOOL_BROWSER],
            sample_calls: vec![sample_read("docs/untrusted-agent-output.md")],
            expect_plan_gate: false,
            expect_verification_gate: false,
            expect_rollback_checkpoint: false,
            golden_surfaces: &["prompt_assembly", "tool_selection"],
        },
        AgentHarnessEvalCase {
            id: "stale_worktree_conflict_pauses_for_boundary",
            fixture_kind: HarnessEvalFixtureKind::StaleWorktreeConflict,
            prompt: "Update src/tracked.txt after checking the dirty worktree and handle stale conflicts safely.",
            expected_tools: &[AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_GIT_STATUS, AUTONOMOUS_TOOL_WRITE],
            forbidden_tools: &[AUTONOMOUS_TOOL_BROWSER, AUTONOMOUS_TOOL_EMULATOR],
            sample_calls: vec![
                sample_read("src/tracked.txt"),
                sample_git_status(),
                sample_write("src/tracked.txt"),
            ],
            expect_plan_gate: false,
            expect_verification_gate: true,
            expect_rollback_checkpoint: true,
            golden_surfaces: &["approvals", "continuations", "compaction"],
        },
    ]
}

fn evaluate_case(
    repo_root: &Path,
    controls: &RuntimeRunControlStateDto,
    case: AgentHarnessEvalCase,
) -> AgentHarnessEvalCaseResult {
    let registry = ToolRegistry::for_prompt(repo_root, case.prompt, controls);
    let exposed = registry.descriptor_names();
    let mut failures = Vec::new();

    let expected_tools = case
        .expected_tools
        .iter()
        .map(|tool| (*tool).to_string())
        .collect::<Vec<_>>();
    for expected in case.expected_tools {
        if !exposed.contains(*expected) {
            failures.push(format!("Expected tool `{expected}` was not exposed."));
        }
    }

    let forbidden_tools_exposed = case
        .forbidden_tools
        .iter()
        .filter(|tool| exposed.contains(**tool))
        .map(|tool| (*tool).to_string())
        .collect::<Vec<_>>();
    for forbidden in &forbidden_tools_exposed {
        failures.push(format!("Forbidden tool `{forbidden}` was exposed."));
    }

    let descriptor_validation_failures = case
        .sample_calls
        .iter()
        .filter_map(|sample| {
            registry
                .validate_call(&AgentToolCall {
                    tool_call_id: format!("eval-{}-{}", case.id, sample.tool_name),
                    tool_name: sample.tool_name.into(),
                    input: sample.input.clone(),
                })
                .err()
                .map(|error| format!("{}: {}", sample.tool_name, error.message))
        })
        .collect::<Vec<_>>();
    failures.extend(
        descriptor_validation_failures
            .iter()
            .map(|failure| format!("Descriptor validation failed for {failure}")),
    );

    let plan_gate_passed = plan_gate_matches(case.prompt, controls, case.expect_plan_gate);
    if !plan_gate_passed {
        failures.push(format!(
            "Plan gate did not match expected state `{}`.",
            case.expect_plan_gate
        ));
    }

    let verification_gate_passed = verification_gate_matches(case.expect_verification_gate);
    if !verification_gate_passed {
        failures.push("Verification gate did not require fresh evidence correctly.".into());
    }

    let rollback_gate_passed = rollback_gate_matches(case.expect_rollback_checkpoint);
    if !rollback_gate_passed {
        failures.push("Rollback checkpoint expectation was not covered.".into());
    }

    let mut exposed_tools = exposed.into_iter().collect::<Vec<_>>();
    exposed_tools.sort();
    AgentHarnessEvalCaseResult {
        case_id: case.id.into(),
        fixture_kind: case.fixture_kind,
        passed: failures.is_empty(),
        expected_tools,
        exposed_tools,
        forbidden_tools_exposed,
        descriptor_validation_failures,
        plan_gate_passed,
        verification_gate_passed,
        rollback_gate_passed,
        failures,
    }
}

fn plan_gate_matches(
    prompt: &str,
    controls: &RuntimeRunControlStateDto,
    expect_plan_gate: bool,
) -> bool {
    let classification = classify_agent_task(prompt, controls);
    let gate = evaluate_tool_batch_gate(
        &empty_eval_snapshot(),
        controls,
        &classification,
        &[AgentToolCall {
            tool_call_id: "eval-execution-tool".into(),
            tool_name: AUTONOMOUS_TOOL_EDIT.into(),
            input: json!({ "path": "src/lib.rs", "oldText": "old", "newText": "new" }),
        }],
    );
    matches!(gate, ToolBatchGate::RequirePlan { .. }) == expect_plan_gate
}

fn verification_gate_matches(expect_verification_gate: bool) -> bool {
    if !expect_verification_gate {
        let decision = evaluate_completion_gate(&empty_eval_snapshot(), "Done.");
        return decision.status == VerificationGateStatus::NotRequired;
    }

    let mut missing = eval_snapshot_with_file_change();
    let required = evaluate_completion_gate(&missing, "Done.");
    missing.events.push(project_store::AgentEventRecord {
        id: 2,
        project_id: "eval-project".into(),
        run_id: "eval-run".into(),
        event_kind: AgentRunEventKind::CommandOutput,
        payload_json:
            r#"{"argv":["cargo","test","agent_core_runtime"],"spawned":true,"exitCode":0}"#.into(),
        created_at: "2026-04-30T00:00:02Z".into(),
    });
    let satisfied = evaluate_completion_gate(&missing, "Done.");
    required.status == VerificationGateStatus::Required
        && satisfied.status == VerificationGateStatus::Satisfied
        && satisfied
            .evidence
            .as_deref()
            .is_some_and(|evidence| evidence.contains("exited with code 0"))
}

fn rollback_gate_matches(expect_rollback_checkpoint: bool) -> bool {
    if !expect_rollback_checkpoint {
        return true;
    }
    let snapshot = eval_snapshot_with_file_change_and_rollback();
    let file_changes_have_hashes = snapshot.file_changes.iter().all(|change| {
        change
            .old_hash
            .as_deref()
            .is_some_and(|hash| hash.len() == 64)
            && change
                .new_hash
                .as_deref()
                .is_some_and(|hash| hash.len() == 64)
    });
    let has_rollback_checkpoint = snapshot.checkpoints.iter().any(|checkpoint| {
        checkpoint.checkpoint_kind == "tool"
            && checkpoint
                .payload_json
                .as_deref()
                .is_some_and(|payload| payload.contains("\"kind\":\"file_rollback\""))
    });
    file_changes_have_hashes && has_rollback_checkpoint
}

fn coverage_for_cases(cases: &[AgentHarnessEvalCaseResult]) -> AgentHarnessEvalCoverage {
    let definitions = production_eval_cases();
    let mut golden_surfaces = BTreeSet::new();
    let mut fixture_kinds = BTreeSet::new();
    for definition in &definitions {
        golden_surfaces.extend(
            definition
                .golden_surfaces
                .iter()
                .map(|surface| (*surface).into()),
        );
        fixture_kinds.insert(definition.fixture_kind);
    }
    fixture_kinds.extend(cases.iter().map(|case| case.fixture_kind));
    let missing_golden_surfaces = REQUIRED_GOLDEN_SURFACES
        .iter()
        .filter(|surface| !golden_surfaces.contains(**surface))
        .map(|surface| (*surface).to_string())
        .collect::<Vec<_>>();
    let missing_fixture_kinds = REQUIRED_FIXTURES
        .iter()
        .copied()
        .filter(|kind| !fixture_kinds.contains(kind))
        .collect::<Vec<_>>();
    AgentHarnessEvalCoverage {
        golden_surfaces: golden_surfaces.into_iter().collect(),
        fixture_kinds: fixture_kinds.into_iter().collect(),
        missing_golden_surfaces,
        missing_fixture_kinds,
    }
}

fn metrics_for_cases(cases: &[AgentHarnessEvalCaseResult]) -> AgentHarnessEvalMetrics {
    let case_count = cases.len().max(1) as f64;
    let task_completion_rate = cases.iter().filter(|case| case.passed).count() as f64 / case_count;
    let sample_count = production_eval_cases()
        .iter()
        .map(|case| case.sample_calls.len())
        .sum::<usize>()
        .max(1) as f64;
    let descriptor_failures = cases
        .iter()
        .map(|case| case.descriptor_validation_failures.len())
        .sum::<usize>() as f64;
    let forbidden_checks = production_eval_cases()
        .iter()
        .map(|case| case.forbidden_tools.len())
        .sum::<usize>()
        .max(1) as f64;
    let forbidden_exposures = cases
        .iter()
        .map(|case| case.forbidden_tools_exposed.len())
        .sum::<usize>() as f64;

    AgentHarnessEvalMetrics {
        task_completion_rate,
        tool_call_validity_rate: (sample_count - descriptor_failures) / sample_count,
        unnecessary_tool_exposure_rate: forbidden_exposures / forbidden_checks,
        approval_precision_rate: cases.iter().filter(|case| case.plan_gate_passed).count() as f64
            / case_count,
        verification_rate: cases
            .iter()
            .filter(|case| case.verification_gate_passed)
            .count() as f64
            / case_count,
        rollback_correctness_rate: cases
            .iter()
            .filter(|case| case.rollback_gate_passed)
            .count() as f64
            / case_count,
    }
}

fn threshold_failures(
    metrics: &AgentHarnessEvalMetrics,
    thresholds: &AgentHarnessEvalThresholds,
) -> Vec<String> {
    let mut failures = Vec::new();
    if metrics.task_completion_rate < thresholds.min_task_completion_rate {
        failures.push(format!(
            "task_completion_rate {:.3} is below threshold {:.3}.",
            metrics.task_completion_rate, thresholds.min_task_completion_rate
        ));
    }
    if metrics.tool_call_validity_rate < thresholds.min_tool_call_validity_rate {
        failures.push(format!(
            "tool_call_validity_rate {:.3} is below threshold {:.3}.",
            metrics.tool_call_validity_rate, thresholds.min_tool_call_validity_rate
        ));
    }
    if metrics.unnecessary_tool_exposure_rate > thresholds.max_unnecessary_tool_exposure_rate {
        failures.push(format!(
            "unnecessary_tool_exposure_rate {:.3} is above threshold {:.3}.",
            metrics.unnecessary_tool_exposure_rate, thresholds.max_unnecessary_tool_exposure_rate
        ));
    }
    if metrics.approval_precision_rate < thresholds.min_approval_precision_rate {
        failures.push(format!(
            "approval_precision_rate {:.3} is below threshold {:.3}.",
            metrics.approval_precision_rate, thresholds.min_approval_precision_rate
        ));
    }
    if metrics.verification_rate < thresholds.min_verification_rate {
        failures.push(format!(
            "verification_rate {:.3} is below threshold {:.3}.",
            metrics.verification_rate, thresholds.min_verification_rate
        ));
    }
    if metrics.rollback_correctness_rate < thresholds.min_rollback_correctness_rate {
        failures.push(format!(
            "rollback_correctness_rate {:.3} is below threshold {:.3}.",
            metrics.rollback_correctness_rate, thresholds.min_rollback_correctness_rate
        ));
    }
    failures
}

fn eval_controls() -> RuntimeRunControlStateDto {
    RuntimeRunControlStateDto {
        active: RuntimeRunActiveControlSnapshotDto {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            provider_profile_id: None,
            model_id: "eval-model".into(),
            thinking_effort: None,
            approval_mode: RuntimeRunApprovalModeDto::Yolo,
            plan_mode_required: false,
            revision: 1,
            applied_at: "2026-04-30T00:00:00Z".into(),
        },
        pending: None,
    }
}

fn empty_eval_snapshot() -> AgentRunSnapshotRecord {
    AgentRunSnapshotRecord {
        run: project_store::AgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "engineer".into(),
            agent_definition_version: project_store::BUILTIN_AGENT_DEFINITION_VERSION,
            project_id: "eval-project".into(),
            agent_session_id: "eval-session".into(),
            run_id: "eval-run".into(),
            provider_id: "fake".into(),
            model_id: "fake".into(),
            status: AgentRunStatus::Running,
            prompt: "eval prompt".into(),
            system_prompt: "eval system".into(),
            started_at: "2026-04-30T00:00:00Z".into(),
            last_heartbeat_at: None,
            completed_at: None,
            cancelled_at: None,
            last_error: None,
            updated_at: "2026-04-30T00:00:00Z".into(),
        },
        messages: Vec::new(),
        events: Vec::new(),
        tool_calls: Vec::new(),
        file_changes: Vec::new(),
        checkpoints: Vec::new(),
        action_requests: Vec::new(),
    }
}

fn eval_snapshot_with_file_change() -> AgentRunSnapshotRecord {
    let mut snapshot = empty_eval_snapshot();
    snapshot
        .file_changes
        .push(project_store::AgentFileChangeRecord {
            id: 1,
            project_id: "eval-project".into(),
            run_id: "eval-run".into(),
            path: "src/lib.rs".into(),
            operation: "edit".into(),
            old_hash: None,
            new_hash: None,
            created_at: "2026-04-30T00:00:01Z".into(),
        });
    snapshot.events.push(project_store::AgentEventRecord {
        id: 1,
        project_id: "eval-project".into(),
        run_id: "eval-run".into(),
        event_kind: AgentRunEventKind::FileChanged,
        payload_json: "{}".into(),
        created_at: "2026-04-30T00:00:01Z".into(),
    });
    snapshot
}

fn eval_snapshot_with_file_change_and_rollback() -> AgentRunSnapshotRecord {
    let mut snapshot = eval_snapshot_with_file_change();
    snapshot.file_changes[0].old_hash =
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into());
    snapshot.file_changes[0].new_hash =
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into());
    snapshot
        .checkpoints
        .push(project_store::AgentCheckpointRecord {
            id: 1,
            project_id: "eval-project".into(),
            run_id: "eval-run".into(),
            checkpoint_kind: "tool".into(),
            summary: "Rollback checkpoint for src/lib.rs.".into(),
            payload_json: Some(
                json!({
                    "kind": "file_rollback",
                    "path": "src/lib.rs",
                    "operation": "edit",
                    "oldHash": snapshot.file_changes[0].old_hash,
                    "newHash": snapshot.file_changes[0].new_hash,
                    "oldContentBase64": "b2xk",
                })
                .to_string(),
            ),
            created_at: "2026-04-30T00:00:02Z".into(),
        });
    snapshot
}

fn sample_read(path: &'static str) -> SampleToolCall {
    SampleToolCall {
        tool_name: AUTONOMOUS_TOOL_READ,
        input: json!({ "path": path }),
    }
}

fn sample_edit(path: &'static str) -> SampleToolCall {
    SampleToolCall {
        tool_name: AUTONOMOUS_TOOL_EDIT,
        input: json!({
            "path": path,
            "startLine": 1,
            "endLine": 1,
            "expected": "old",
            "replacement": "new",
        }),
    }
}

fn sample_write(path: &'static str) -> SampleToolCall {
    SampleToolCall {
        tool_name: AUTONOMOUS_TOOL_WRITE,
        input: json!({ "path": path, "content": "new\n" }),
    }
}

fn sample_command(argv: &[&'static str]) -> SampleToolCall {
    SampleToolCall {
        tool_name: AUTONOMOUS_TOOL_COMMAND,
        input: json!({ "argv": argv }),
    }
}

fn sample_git_status() -> SampleToolCall {
    SampleToolCall {
        tool_name: AUTONOMOUS_TOOL_GIT_STATUS,
        input: json!({}),
    }
}

fn sample_todo() -> SampleToolCall {
    SampleToolCall {
        tool_name: AUTONOMOUS_TOOL_TODO,
        input: json!({
            "action": "upsert",
            "id": "plan-1",
            "title": "Inspect and patch the scoped files",
            "status": "in_progress"
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_harness_eval_suite_passes_quality_gate() {
        let root = tempfile::tempdir().expect("temp dir");
        let report = run_agent_harness_eval_suite(root.path());

        assert!(report.passed, "{:#?}", report.failures);
        assert!(report.coverage.missing_fixture_kinds.is_empty());
        assert!(report.coverage.missing_golden_surfaces.is_empty());
        assert_eq!(report.metrics.task_completion_rate, 1.0);
        assert_eq!(report.metrics.tool_call_validity_rate, 1.0);
        assert_eq!(report.metrics.unnecessary_tool_exposure_rate, 0.0);
    }
}
