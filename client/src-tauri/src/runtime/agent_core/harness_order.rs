use super::*;
use sha2::{Digest, Sha256};

const HARNESS_MANIFEST_VERSION: &str = "harness_test_manifest_v1";
const HARNESS_RUNNER_SCHEMA: &str = "xero.harness_runner.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum HarnessStepStatus {
    Pending,
    Passed,
    Failed,
    SkippedWithReason,
}

impl HarnessStepStatus {
    const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::SkippedWithReason => "skipped_with_reason",
        }
    }

    const fn is_terminal(&self) -> bool {
        matches!(self, Self::Passed | Self::Failed | Self::SkippedWithReason)
    }

    fn from_report_cell(value: &str) -> Option<Self> {
        match value.trim() {
            "passed" => Some(Self::Passed),
            "failed" => Some(Self::Failed),
            "skipped_with_reason" => Some(Self::SkippedWithReason),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct HarnessManifestItem {
    item_id: String,
    order: usize,
    step_id: &'static str,
    target: String,
    effect_class: String,
    safe_input: &'static str,
    pass_condition: &'static str,
    skip_condition: &'static str,
    cleanup_requirement: &'static str,
    status: HarnessStepStatus,
    observed_tool_call_id: Option<String>,
    observed_tool_name: Option<String>,
    evidence: Option<String>,
    skip_reason: Option<String>,
}

impl HarnessManifestItem {
    fn pending(order: usize, step: &HarnessStepDefinition, target: impl Into<String>) -> Self {
        let target = target.into();
        let profile = target_manifest_profile(step, target.as_str());
        Self {
            item_id: format!("{}.{}", step.step_id, target),
            order,
            step_id: step.step_id,
            effect_class: tool_effect_class(&target).as_str().into(),
            target,
            safe_input: profile.safe_input,
            pass_condition: profile.pass_condition,
            skip_condition: profile.skip_condition,
            cleanup_requirement: profile.cleanup_requirement,
            status: HarnessStepStatus::Pending,
            observed_tool_call_id: None,
            observed_tool_name: None,
            evidence: None,
            skip_reason: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HarnessStepDefinition {
    step_id: &'static str,
    targets: &'static [&'static str],
    safe_input: &'static str,
    pass_condition: &'static str,
    skip_condition: &'static str,
    cleanup_requirement: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HarnessTargetProfile {
    safe_input: &'static str,
    pass_condition: &'static str,
    skip_condition: &'static str,
    cleanup_requirement: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HarnessToolAssignment {
    tool_call_id: String,
    item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HarnessToolBatchDecision {
    Allow {
        assignments: Vec<HarnessToolAssignment>,
    },
    Reprompt {
        message: String,
    },
}

#[derive(Debug, Clone, Default)]
pub(crate) struct HarnessTestOrderGate {
    items: Vec<HarnessManifestItem>,
    manifest_signature: Option<String>,
    manifest_revision: u32,
}

impl HarnessTestOrderGate {
    pub(crate) fn for_controls(controls: &RuntimeRunControlStateDto) -> Option<Self> {
        (controls.active.runtime_agent_id == RuntimeAgentIdDto::Test).then(Self::default)
    }

    pub(crate) fn refresh_manifest(
        &mut self,
        repo_root: &Path,
        project_id: &str,
        run_id: &str,
        registry: &ToolRegistry,
    ) -> CommandResult<()> {
        let mut next_items = canonical_manifest_items(registry);
        for item in &mut next_items {
            if let Some(previous) = self
                .items
                .iter()
                .find(|previous| previous.item_id == item.item_id)
            {
                item.status = previous.status.clone();
                item.observed_tool_call_id = previous.observed_tool_call_id.clone();
                item.observed_tool_name = previous.observed_tool_name.clone();
                item.evidence = previous.evidence.clone();
                item.skip_reason = previous.skip_reason.clone();
            }
        }
        self.items = next_items;

        let signature = manifest_signature(&self.items)?;
        if self.manifest_signature.as_deref() == Some(signature.as_str()) {
            return Ok(());
        }
        self.manifest_signature = Some(signature.clone());
        self.manifest_revision = self.manifest_revision.saturating_add(1);
        append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::ValidationStarted,
            json!({
                "kind": "harness_test_manifest",
                "label": "harness_test_manifest",
                "manifestVersion": HARNESS_MANIFEST_VERSION,
                "revision": self.manifest_revision,
                "signature": signature,
                "status": "active",
                "expected": self.next_pending_manifest_item_json(),
                "items": self.items.clone(),
                "terminalCounts": self.terminal_counts_json(),
            }),
        )?;
        Ok(())
    }

    pub(crate) fn evaluate_tool_calls(
        &self,
        repo_root: &Path,
        project_id: &str,
        run_id: &str,
        tool_calls: &[AgentToolCall],
    ) -> CommandResult<HarnessToolBatchDecision> {
        let Some(first_pending_index) = self.next_pending_tool_item_index() else {
            let observed = tool_calls
                .iter()
                .map(|call| call.tool_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let message = self.reprompt_message(
                "The harness manifest has no pending tool steps. Produce the final report instead of calling more tools.",
            );
            self.record_gate_block(
                repo_root,
                project_id,
                run_id,
                "unexpected_tool_after_manifest_satisfied",
                Some(observed),
                message.as_str(),
            )?;
            return Ok(HarnessToolBatchDecision::Reprompt { message });
        };

        let mut assignments = Vec::with_capacity(tool_calls.len());
        for (offset, tool_call) in tool_calls.iter().enumerate() {
            let Some(expected) = self.items.get(first_pending_index + offset) else {
                let observed = tool_calls
                    .iter()
                    .map(|call| call.tool_name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let message = self.reprompt_message(
                    "The harness tool batch is longer than the remaining ordered manifest.",
                );
                self.record_gate_block(
                    repo_root,
                    project_id,
                    run_id,
                    "tool_batch_exceeds_manifest",
                    Some(observed),
                    message.as_str(),
                )?;
                return Ok(HarnessToolBatchDecision::Reprompt { message });
            };
            if expected.status.is_terminal() || expected.target != tool_call.tool_name {
                let message = self.reprompt_message(&format!(
                    "Expected `{}` for step `{}` next, but observed `{}`.",
                    expected.target, expected.step_id, tool_call.tool_name
                ));
                self.record_gate_block(
                    repo_root,
                    project_id,
                    run_id,
                    "out_of_order_tool_call",
                    Some(tool_call.tool_name.clone()),
                    message.as_str(),
                )?;
                return Ok(HarnessToolBatchDecision::Reprompt { message });
            }
            assignments.push(HarnessToolAssignment {
                tool_call_id: tool_call.tool_call_id.clone(),
                item_id: expected.item_id.clone(),
            });
        }

        append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::ValidationStarted,
            json!({
                "kind": "harness_test_order_gate",
                "label": "harness_test_order_gate",
                "outcome": "allowed",
                "assignments": assignments.iter().map(|assignment| {
                    json!({
                        "toolCallId": assignment.tool_call_id,
                        "itemId": assignment.item_id,
                    })
                }).collect::<Vec<_>>(),
            }),
        )?;

        Ok(HarnessToolBatchDecision::Allow { assignments })
    }

    pub(crate) fn record_tool_outcomes(
        &mut self,
        repo_root: &Path,
        project_id: &str,
        run_id: &str,
        assignments: &[HarnessToolAssignment],
        snapshot: &AgentRunSnapshotRecord,
    ) -> CommandResult<()> {
        for assignment in assignments {
            let Some(tool_call) = snapshot
                .tool_calls
                .iter()
                .find(|tool_call| tool_call.tool_call_id == assignment.tool_call_id)
            else {
                continue;
            };
            let Some(item) = self
                .items
                .iter_mut()
                .find(|item| item.item_id == assignment.item_id)
            else {
                continue;
            };
            item.observed_tool_call_id = Some(tool_call.tool_call_id.clone());
            item.observed_tool_name = Some(tool_call.tool_name.clone());
            match tool_call.state {
                AgentToolCallState::Succeeded => {
                    item.status = HarnessStepStatus::Passed;
                    item.evidence = Some(tool_result_summary(tool_call));
                    item.skip_reason = None;
                }
                AgentToolCallState::Failed => {
                    item.status = HarnessStepStatus::Failed;
                    item.evidence = tool_call
                        .error
                        .as_ref()
                        .map(|error| format!("{}: {}", error.code, error.message))
                        .or_else(|| Some("Tool call failed without persisted diagnostic.".into()));
                    item.skip_reason = None;
                }
                AgentToolCallState::Pending | AgentToolCallState::Running => continue,
            }
            let item = item.clone();
            self.record_step_event(repo_root, project_id, run_id, &item)?;
        }
        Ok(())
    }

    pub(crate) fn evaluate_completion(
        &mut self,
        repo_root: &Path,
        project_id: &str,
        run_id: &str,
        message: &str,
    ) -> CommandResult<Option<String>> {
        self.apply_final_report_skips(repo_root, project_id, run_id, message)?;
        if self.next_pending_tool_item_index().is_none() {
            if let Some(final_report) = self
                .items
                .iter_mut()
                .find(|item| item.step_id == "final_report")
            {
                final_report.status = HarnessStepStatus::Passed;
                final_report.evidence =
                    Some("Final report accepted after manifest satisfaction.".into());
                final_report.skip_reason = None;
                let final_report = final_report.clone();
                self.record_step_event(repo_root, project_id, run_id, &final_report)?;
            }
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::ValidationCompleted,
                json!({
                    "kind": "harness_test_manifest",
                    "label": "harness_test_manifest",
                    "manifestVersion": HARNESS_MANIFEST_VERSION,
                    "outcome": "satisfied",
                    "items": self.items.clone(),
                    "terminalCounts": self.terminal_counts_json(),
                }),
            )?;
            return Ok(None);
        }

        let message = self.reprompt_message(
            "The provider returned a final response before the harness manifest was satisfied.",
        );
        self.record_gate_block(
            repo_root,
            project_id,
            run_id,
            "final_response_before_manifest_satisfied",
            None,
            message.as_str(),
        )?;
        Ok(Some(message))
    }

    fn apply_final_report_skips(
        &mut self,
        repo_root: &Path,
        project_id: &str,
        run_id: &str,
        message: &str,
    ) -> CommandResult<()> {
        for row in parse_final_report_rows(message) {
            if row.status != HarnessStepStatus::SkippedWithReason {
                continue;
            }
            if row.skip_reason.trim().is_empty() || row.skip_reason.trim() == "none" {
                continue;
            }
            let Some(item) = self.items.iter_mut().find(|item| {
                item.step_id == row.step_id
                    && item.target == row.target
                    && !item.status.is_terminal()
            }) else {
                continue;
            };
            item.status = HarnessStepStatus::SkippedWithReason;
            item.evidence = (!row.evidence.trim().is_empty() && row.evidence.trim() != "none")
                .then(|| row.evidence.clone());
            item.skip_reason = Some(row.skip_reason.clone());
            let item = item.clone();
            self.record_step_event(repo_root, project_id, run_id, &item)?;
        }
        Ok(())
    }

    fn next_pending_tool_item_index(&self) -> Option<usize> {
        self.items
            .iter()
            .position(|item| !item.status.is_terminal() && item.step_id != "final_report")
    }

    fn next_pending_manifest_item_json(&self) -> JsonValue {
        self.next_pending_tool_item_index()
            .and_then(|index| self.items.get(index))
            .map(|item| {
                json!({
                    "itemId": item.item_id,
                    "stepId": item.step_id,
                    "target": item.target,
                    "status": item.status.as_str(),
                })
            })
            .unwrap_or_else(|| {
                json!({
                    "stepId": "final_report",
                    "target": "final_report",
                    "status": "pending",
                })
            })
    }

    fn terminal_counts_json(&self) -> JsonValue {
        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;
        let mut pending = 0usize;
        for item in &self.items {
            match item.status {
                HarnessStepStatus::Passed => passed += 1,
                HarnessStepStatus::Failed => failed += 1,
                HarnessStepStatus::SkippedWithReason => skipped += 1,
                HarnessStepStatus::Pending => pending += 1,
            }
        }
        json!({
            "passed": passed,
            "failed": failed,
            "skipped": skipped,
            "pending": pending,
        })
    }

    fn reprompt_message(&self, reason: &str) -> String {
        let expected = self.next_pending_manifest_item_json();
        let expected_step = expected
            .get("stepId")
            .and_then(JsonValue::as_str)
            .unwrap_or("final_report");
        let expected_target = expected
            .get("target")
            .and_then(JsonValue::as_str)
            .unwrap_or("final_report");
        format!(
            "Xero harness order gate: {reason}\n\nNext required manifest item: step `{expected_step}`, target `{expected_target}`. Continue the Test-agent harness in canonical order. If this item cannot be safely exercised, include a final-report table row for this exact step and target with status `skipped_with_reason` and a concrete skip reason; otherwise call the expected tool next."
        )
    }

    fn record_gate_block(
        &self,
        repo_root: &Path,
        project_id: &str,
        run_id: &str,
        code: &str,
        observed: Option<String>,
        message: &str,
    ) -> CommandResult<()> {
        append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::ValidationCompleted,
            json!({
                "kind": "harness_test_order_gate",
                "label": "harness_test_order_gate",
                "outcome": "blocked",
                "code": code,
                "observed": observed,
                "expected": self.next_pending_manifest_item_json(),
                "message": message,
                "terminalCounts": self.terminal_counts_json(),
            }),
        )?;
        Ok(())
    }

    fn record_step_event(
        &self,
        repo_root: &Path,
        project_id: &str,
        run_id: &str,
        item: &HarnessManifestItem,
    ) -> CommandResult<()> {
        append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::ValidationCompleted,
            json!({
                "kind": "harness_test_step",
                "label": "harness_test_step",
                "manifestVersion": HARNESS_MANIFEST_VERSION,
                "item": item,
                "outcome": item.status.as_str(),
                "terminalCounts": self.terminal_counts_json(),
            }),
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FinalReportRow {
    step_id: String,
    target: String,
    status: HarnessStepStatus,
    evidence: String,
    skip_reason: String,
}

pub(crate) fn harness_runner_tool_output(
    registry: &ToolRegistry,
    request: &AutonomousHarnessRunnerRequest,
) -> CommandResult<(String, JsonValue)> {
    let items = canonical_manifest_items(registry);
    let manifest_signature = manifest_signature(&items)?;
    let comparison = match request.action {
        AutonomousHarnessRunnerAction::Manifest => harness_runner_manifest_comparison(),
        AutonomousHarnessRunnerAction::CompareReport => {
            let report = request.final_report.as_deref().ok_or_else(|| {
                CommandError::user_fixable(
                    "harness_runner_final_report_required",
                    "harness_runner compare_report requires finalReport markdown.",
                )
            })?;
            harness_runner_compare_report(&items, report)
        }
    };
    let passed = comparison
        .get("passed")
        .and_then(JsonValue::as_bool)
        .unwrap_or(true);
    let summary = match request.action {
        AutonomousHarnessRunnerAction::Manifest => format!(
            "Harness runner exported {} canonical manifest item(s).",
            items.len()
        ),
        AutonomousHarnessRunnerAction::CompareReport if passed => {
            "Harness runner matched the model-driven report against the canonical manifest.".into()
        }
        AutonomousHarnessRunnerAction::CompareReport => {
            "Harness runner found differences between the model-driven report and canonical manifest."
                .into()
        }
    };
    let item_values = items
        .iter()
        .map(|item| serde_json::to_value(item).unwrap_or(JsonValue::Null))
        .collect::<Vec<_>>();
    Ok((
        summary.clone(),
        json!({
            "schema": HARNESS_RUNNER_SCHEMA,
            "kind": "harness_runner",
            "action": request.action,
            "passed": passed,
            "summary": summary,
            "manifestVersion": HARNESS_MANIFEST_VERSION,
            "manifestSignature": manifest_signature,
            "itemCount": items.len(),
            "comparison": comparison,
            "items": item_values,
        }),
    ))
}

fn harness_runner_manifest_comparison() -> JsonValue {
    json!({
        "passed": true,
        "mode": "manifest_only",
        "missingRows": [],
        "unexpectedRows": [],
        "outOfOrderRows": [],
        "unsafeRows": [],
    })
}

fn harness_runner_compare_report(items: &[HarnessManifestItem], final_report: &str) -> JsonValue {
    let expected = items
        .iter()
        .filter(|item| item.step_id != "final_report")
        .collect::<Vec<_>>();
    let rows = parse_final_report_rows(final_report);
    let mut missing_rows = Vec::new();
    let mut unexpected_rows = Vec::new();
    let mut out_of_order_rows = Vec::new();
    let mut unsafe_rows = Vec::new();

    for expected_item in &expected {
        if !rows
            .iter()
            .any(|row| row.step_id == expected_item.step_id && row.target == expected_item.target)
        {
            missing_rows.push(json!({
                "stepId": expected_item.step_id,
                "target": expected_item.target,
            }));
        }
    }

    for (index, row) in rows.iter().enumerate() {
        let expected_at_index = expected.get(index);
        if expected_at_index
            .is_none_or(|item| item.step_id != row.step_id.as_str() || item.target != row.target)
        {
            out_of_order_rows.push(json!({
                "index": index,
                "stepId": row.step_id,
                "target": row.target,
                "expectedStepId": expected_at_index.map(|item| item.step_id),
                "expectedTarget": expected_at_index.map(|item| item.target.as_str()),
            }));
        }

        if !expected
            .iter()
            .any(|item| item.step_id == row.step_id.as_str() && item.target == row.target)
        {
            unexpected_rows.push(json!({
                "stepId": row.step_id,
                "target": row.target,
            }));
        }

        match row.status {
            HarnessStepStatus::Passed
                if row.evidence.trim().is_empty() || row.evidence.trim() == "none" =>
            {
                unsafe_rows.push(json!({
                    "stepId": row.step_id,
                    "target": row.target,
                    "reason": "passed_row_requires_evidence",
                }));
            }
            HarnessStepStatus::SkippedWithReason
                if row.skip_reason.trim().is_empty() || row.skip_reason.trim() == "none" =>
            {
                unsafe_rows.push(json!({
                    "stepId": row.step_id,
                    "target": row.target,
                    "reason": "skipped_row_requires_reason",
                }));
            }
            HarnessStepStatus::Failed | HarnessStepStatus::Pending => {}
            HarnessStepStatus::Passed | HarnessStepStatus::SkippedWithReason => {}
        }
    }

    let passed = missing_rows.is_empty()
        && unexpected_rows.is_empty()
        && out_of_order_rows.is_empty()
        && unsafe_rows.is_empty();
    json!({
        "passed": passed,
        "mode": "compare_report",
        "expectedRowCount": expected.len(),
        "observedRowCount": rows.len(),
        "missingRows": missing_rows,
        "unexpectedRows": unexpected_rows,
        "outOfOrderRows": out_of_order_rows,
        "unsafeRows": unsafe_rows,
    })
}

fn parse_final_report_rows(message: &str) -> Vec<FinalReportRow> {
    message
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with('|') {
                return None;
            }
            let cells = trimmed
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect::<Vec<_>>();
            if cells.len() < 5
                || cells[0].eq_ignore_ascii_case("step")
                || cells[0].starts_with("---")
            {
                return None;
            }
            Some(FinalReportRow {
                step_id: cells[0].to_owned(),
                target: cells[1].to_owned(),
                status: HarnessStepStatus::from_report_cell(cells[2])?,
                evidence: cells[3].to_owned(),
                skip_reason: cells[4].to_owned(),
            })
        })
        .collect()
}

fn manifest_signature(items: &[HarnessManifestItem]) -> CommandResult<String> {
    let serializable = items
        .iter()
        .map(|item| {
            json!({
                "itemId": item.item_id,
                "status": item.status.as_str(),
                "observedToolCallId": item.observed_tool_call_id,
                "evidence": item.evidence,
                "skipReason": item.skip_reason,
            })
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&serializable).map_err(|error| {
        CommandError::system_fault(
            "harness_test_manifest_hash_failed",
            format!("Xero could not hash the Test-agent harness manifest: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn tool_result_summary(tool_call: &project_store::AgentToolCallRecord) -> String {
    let Some(result_json) = tool_call.result_json.as_deref() else {
        return "Tool call succeeded without persisted result payload.".into();
    };
    serde_json::from_str::<JsonValue>(result_json)
        .ok()
        .and_then(|value| {
            value
                .get("summary")
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    value
                        .get("output")
                        .and_then(|output| output.get("summary"))
                        .and_then(JsonValue::as_str)
                        .map(str::to_owned)
                })
        })
        .filter(|summary| !summary.trim().is_empty())
        .unwrap_or_else(|| "Tool call succeeded and persisted a result payload.".into())
}

fn target_manifest_profile(step: &HarnessStepDefinition, target: &str) -> HarnessTargetProfile {
    let default = profile(
        step.safe_input,
        step.pass_condition,
        step.skip_condition,
        step.cleanup_requirement,
    );
    match (step.step_id, target) {
        ("deterministic_runner", AUTONOMOUS_TOOL_HARNESS_RUNNER) => profile(
            r#"{"action":"manifest"}"#,
            "Harness runner exports the canonical machine-readable manifest.",
            "harness_runner is absent from the active registry.",
            "None.",
        ),
        ("registry_discovery", AUTONOMOUS_TOOL_TOOL_SEARCH) => profile(
            r#"{"query":"harness registry discovery","limit":10}"#,
            "Tool search returns persisted registry/catalog matches for the active harness surface.",
            "tool_search is absent from the active registry.",
            "None.",
        ),
        ("registry_discovery", AUTONOMOUS_TOOL_TOOL_ACCESS) => profile(
            r#"{"action":"list"}"#,
            "Tool access list returns persisted available groups, tool packs, and health metadata.",
            "tool_access is absent from the active registry.",
            "None.",
        ),
        ("repo_inspection", AUTONOMOUS_TOOL_GIT_STATUS) => profile(
            r#"{}"#,
            "Git status result is persisted without mutating the repository.",
            "git_status is absent or the imported root is not a git repository.",
            "None.",
        ),
        ("repo_inspection", AUTONOMOUS_TOOL_GIT_DIFF) => profile(
            r#"{"scope":"worktree"}"#,
            "Worktree diff result is persisted without mutating the repository.",
            "git_diff is absent or the imported root is not a git repository.",
            "None.",
        ),
        ("repo_inspection", AUTONOMOUS_TOOL_FIND) => profile(
            r#"{"pattern":"TEST_AGENT_IMPLEMENTATION_PLAN.md","path":"."}"#,
            "Find result is persisted for a repo-scoped fixture path.",
            "find is absent or no safe repo fixture path exists.",
            "None.",
        ),
        ("repo_inspection", AUTONOMOUS_TOOL_SEARCH) => profile(
            r#"{"query":"Canonical Tool Test Sequence","path":"TEST_AGENT_IMPLEMENTATION_PLAN.md","regex":false,"maxResults":5}"#,
            "Search result is persisted for a bounded repo-scoped text query.",
            "search is absent or no safe readable text fixture exists.",
            "None.",
        ),
        ("repo_inspection", AUTONOMOUS_TOOL_READ) => profile(
            r#"{"path":"TEST_AGENT_IMPLEMENTATION_PLAN.md","startLine":1,"lineCount":40}"#,
            "Read result is persisted for a bounded repo-scoped text range.",
            "read is absent or no safe readable text fixture exists.",
            "None.",
        ),
        ("repo_inspection", AUTONOMOUS_TOOL_LIST) => profile(
            r#"{"path":".","maxDepth":1}"#,
            "List result is persisted for a bounded repo-scoped directory listing.",
            "list is absent or the imported root cannot be listed safely.",
            "None.",
        ),
        ("repo_inspection", AUTONOMOUS_TOOL_HASH) => profile(
            r#"{"path":"TEST_AGENT_IMPLEMENTATION_PLAN.md"}"#,
            "File hash result is persisted for a repo-scoped fixture file.",
            "file_hash is absent or no safe fixture file exists.",
            "None.",
        ),
        ("planning_runtime_state", AUTONOMOUS_TOOL_TODO) => profile(
            r#"{"action":"list"}"#,
            "Todo list result is persisted through runtime-owned planning state.",
            "todo is absent or runtime planning state is unavailable.",
            "No todo mutation is required for this probe.",
        ),
        ("planning_runtime_state", AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH) => profile(
            r#"{"action":"explain_current_context_package","limit":1}"#,
            "Project context explanation is persisted from app-data-backed runtime state.",
            "project_context_search is absent or no agent run context is available.",
            "None.",
        ),
        ("planning_runtime_state", AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET) => profile(
            r#"{"action":"get_project_record","recordId":"<safe project record id from project_context_search>"}"#,
            "Project context record retrieval is persisted from app-data-backed runtime state.",
            "project_context_get is absent or no safe project record id exists.",
            "None.",
        ),
        ("planning_runtime_state", AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD) => profile(
            r#"{"action":"propose_record_candidate","title":"Harness context probe","summary":"Harness context probe only.","text":"Harness context probe only.","recordKind":"context_note"}"#,
            "Project context record proposal is persisted without changing repository files.",
            "project_context_record is absent or durable-context writes are not safe for this harness run.",
            "Remove or ignore only harness-created context candidates if created.",
        ),
        ("planning_runtime_state", AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE) => profile(
            r#"{"recordId":"<safe harness-created project record id>","summary":"Updated harness context probe."}"#,
            "Project context update is persisted only for a harness-created context item.",
            "project_context_update is absent or no harness-created context item exists.",
            "Remove or restore only harness-created context state if created.",
        ),
        ("planning_runtime_state", AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH) => profile(
            r#"{"recordIds":["<safe project record id>"]}"#,
            "Project context freshness refresh is persisted for app-data-backed runtime state.",
            "project_context_refresh is absent or no safe context id exists.",
            "None.",
        ),
        ("scratch_mutation", AUTONOMOUS_TOOL_MKDIR) => profile(
            r#"{"path":"xero-harness-scratch"}"#,
            "Scratch directory creation result is persisted.",
            "mkdir is absent or the repo scratch path cannot be created safely.",
            "Remove xero-harness-scratch in cleanup_scratch.",
        ),
        ("scratch_mutation", AUTONOMOUS_TOOL_WRITE) => profile(
            r#"{"path":"xero-harness-scratch/probe.txt","content":"xero harness scratch\n"}"#,
            "Scratch file write result is persisted.",
            "write is absent or the repo scratch path cannot be written safely.",
            "Remove xero-harness-scratch in cleanup_scratch.",
        ),
        ("scratch_mutation", AUTONOMOUS_TOOL_EDIT) => profile(
            r#"{"path":"xero-harness-scratch/probe.txt","startLine":1,"endLine":1,"expected":"xero harness scratch\n","replacement":"xero harness scratch edited\n"}"#,
            "Scratch file edit result is persisted with an exact expected-text guard.",
            "edit is absent or the scratch file was not created.",
            "Remove xero-harness-scratch in cleanup_scratch.",
        ),
        ("scratch_mutation", AUTONOMOUS_TOOL_RENAME) => profile(
            r#"{"fromPath":"xero-harness-scratch/probe.txt","toPath":"xero-harness-scratch/probe-renamed.txt"}"#,
            "Scratch file rename result is persisted.",
            "rename is absent or the scratch file was not created.",
            "Remove xero-harness-scratch in cleanup_scratch.",
        ),
        ("scratch_mutation", AUTONOMOUS_TOOL_DELETE) => profile(
            r#"{"path":"xero-harness-scratch/probe-renamed.txt"}"#,
            "Scratch file delete result is persisted.",
            "delete is absent or the scratch file was not created.",
            "Remove any remaining xero-harness-scratch state in cleanup_scratch.",
        ),
        ("commands", AUTONOMOUS_TOOL_COMMAND_PROBE) => profile(
            r#"{"argv":["echo","xero-harness-command-ok"],"timeoutMs":5000}"#,
            "Short harmless probe command result is persisted.",
            "command_probe is absent or safe command execution is unavailable.",
            "None.",
        ),
        ("commands", AUTONOMOUS_TOOL_COMMAND_VERIFY) => profile(
            r#"{"argv":["cargo","test","--help"],"timeoutMs":5000}"#,
            "Verification command result is persisted through the narrowed command_verify policy.",
            "command_verify is absent, Cargo is unavailable, or safe verification execution is unavailable.",
            "None.",
        ),
        ("commands", AUTONOMOUS_TOOL_COMMAND_RUN) => profile(
            r#"{"argv":["printf","xero-harness-command-ok"],"timeoutMs":5000}"#,
            "General command result is persisted.",
            "command_run is absent or safe command execution is unavailable.",
            "None.",
        ),
        ("commands", AUTONOMOUS_TOOL_COMMAND_SESSION) => profile(
            r#"{"action":"start","argv":["sh","-c","printf xero-harness-session-ok; sleep 30"],"timeoutMs":5000}"#,
            "Command session wrapper start/read/stop lifecycle is persisted with a session id and no leftover process.",
            "command_session is absent or safe session execution is unavailable.",
            "No harness command session remains running.",
        ),
        ("process_manager", AUTONOMOUS_TOOL_PROCESS_MANAGER) => profile(
            r#"{"action":"list"}"#,
            "Process-manager list result is persisted for Xero-owned runtime processes.",
            "process_manager is absent or no Xero-owned process fixture exists.",
            "Do not leave processes running.",
        ),
        ("environment_diagnostics", AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT) => profile(
            r#"{"action":"summary"}"#,
            "Redacted environment summary result is persisted without secrets.",
            "environment_context is absent or the app-data environment profile is unavailable.",
            "None.",
        ),
        ("environment_diagnostics", AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE) => profile(
            r#"{"action":"system_log_query","lastMs":60000,"limit":20,"messageContains":"xero"}"#,
            "Bounded diagnostics result is persisted without secrets or unbounded artifacts.",
            "system_diagnostics_observe is absent or the platform probe is unsupported or unsafe.",
            "Remove only harness-created diagnostic artifacts, if any.",
        ),
        ("environment_diagnostics", AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED) => profile(
            r#"{"action":"process_sample","pid":0,"durationMs":1000,"sampleCount":1}"#,
            "Privileged diagnostics approval or result is persisted with action-level metadata.",
            "system_diagnostics_privileged is absent or no safe approved target process exists.",
            "Remove only harness-created diagnostic artifacts, if any.",
        ),
        ("browser_tools", AUTONOMOUS_TOOL_BROWSER_OBSERVE) => profile(
            r#"{"action":"current_url"}"#,
            "Browser observation result is persisted without navigating away from user state.",
            "browser_observe is absent, closed, or no local/safe target is available.",
            "Restore or close only harness-created browser state when possible.",
        ),
        ("browser_tools", AUTONOMOUS_TOOL_BROWSER_CONTROL) => profile(
            r#"{"action":"reload"}"#,
            "Browser control result is persisted only for a local or fixture-safe target.",
            "browser_control is absent or no local/safe target is available.",
            "Restore or close only harness-created browser state when possible.",
        ),
        ("mcp_tools", AUTONOMOUS_TOOL_MCP_LIST) => profile(
            r#"{"action":"list_servers"}"#,
            "MCP server list result is persisted without invoking external tools.",
            "mcp_list is absent or no MCP registry is configured.",
            "None.",
        ),
        ("mcp_tools", AUTONOMOUS_TOOL_MCP_READ_RESOURCE) => profile(
            r#"{"serverId":"<safe fixture MCP server id>","uri":"<safe fixture resource uri>"}"#,
            "MCP resource read result is persisted only for a safe fixture resource.",
            "mcp_read_resource is absent or no safe fixture resource exists.",
            "None.",
        ),
        ("mcp_tools", AUTONOMOUS_TOOL_MCP_GET_PROMPT) => profile(
            r#"{"serverId":"<safe fixture MCP server id>","name":"<safe fixture prompt name>","arguments":{}}"#,
            "MCP prompt retrieval result is persisted only for a safe fixture prompt.",
            "mcp_get_prompt is absent or no safe fixture prompt exists.",
            "None.",
        ),
        ("mcp_tools", AUTONOMOUS_TOOL_MCP_CALL_TOOL) => profile(
            r#"{"serverId":"<safe fixture MCP server id>","name":"<safe fixture tool name>","arguments":{}}"#,
            "MCP tool invocation result is persisted only for a safe fixture tool.",
            "mcp_call_tool is absent or no safe fixture tool exists.",
            "None.",
        ),
        ("skills", AUTONOMOUS_TOOL_SKILL) => profile(
            r#"{"operation":"list","query":"harness","includeUnavailable":true,"limit":5}"#,
            "Skill discovery result is persisted for local safe metadata only.",
            "skill is absent or no safe skill metadata source exists.",
            "Do not install, create, reload, or invoke unsafe skills.",
        ),
        ("emulator_tools", AUTONOMOUS_TOOL_EMULATOR) => profile(
            r#"{"action":"sdk_status"}"#,
            "Managed emulator fixture status result is persisted.",
            "emulator is absent or no managed emulator fixture is available.",
            "Stop or leave the managed fixture in a known idle state.",
        ),
        ("solana_tools", _) if target.starts_with("solana_") => profile(
            "Use the tool's local, devnet, read-only, or fixture-backed status/list/explain probe only.",
            "Solana result is persisted without mainnet mutation or secret exposure.",
            "The Solana tool is absent, external-chain access is unsafe, or no local/devnet fixture exists.",
            "Do not leave external-chain side effects.",
        ),
        ("macos_automation", AUTONOMOUS_TOOL_MACOS_AUTOMATION) => profile(
            r#"{"action":"mac_permissions"}"#,
            "macOS permissions/status result is persisted without desktop control.",
            "macos_automation is absent, unsupported, or control is not explicitly safe.",
            "Do not change user desktop state unless fixture-safe.",
        ),
        ("cleanup_scratch", AUTONOMOUS_TOOL_DELETE) => profile(
            r#"{"path":"xero-harness-scratch","recursive":true}"#,
            "Harness scratch directory is removed or verified absent.",
            "delete is absent or no harness scratch state was created.",
            "All harness-created scratch state removed.",
        ),
        ("mcp_tools", _) if target.starts_with(AUTONOMOUS_DYNAMIC_MCP_TOOL_PREFIX) => profile(
            "Invoke only if the dynamic MCP tool is a configured safe fixture; otherwise skip with reason.",
            "Dynamic MCP fixture result is persisted through Tool Registry V2 dispatch.",
            "No safe fixture-backed dynamic MCP invocation exists.",
            "None.",
        ),
        ("final_report", "final_report") => profile(
            "Produce the exact harness test report shape.",
            "Final report is accepted after all manifest items are terminal.",
            "Final report cannot be skipped.",
            "Scratch cleanup must already be terminal.",
        ),
        _ => default,
    }
}

const fn profile(
    safe_input: &'static str,
    pass_condition: &'static str,
    skip_condition: &'static str,
    cleanup_requirement: &'static str,
) -> HarnessTargetProfile {
    HarnessTargetProfile {
        safe_input,
        pass_condition,
        skip_condition,
        cleanup_requirement,
    }
}

fn canonical_manifest_items(registry: &ToolRegistry) -> Vec<HarnessManifestItem> {
    let active = registry.descriptor_names();
    let dynamic_mcp_targets = registry
        .descriptors()
        .iter()
        .filter(|descriptor| {
            descriptor
                .name
                .starts_with(AUTONOMOUS_DYNAMIC_MCP_TOOL_PREFIX)
        })
        .map(|descriptor| descriptor.name.clone())
        .collect::<BTreeSet<_>>();
    let mut items = Vec::new();
    for step in canonical_step_definitions() {
        for target in step.targets {
            if active.contains(*target) {
                let order = items.len() + 1;
                items.push(HarnessManifestItem::pending(order, step, *target));
            }
        }
        if step.step_id == "mcp_tools" {
            let step = dynamic_mcp_step_definition();
            for target in &dynamic_mcp_targets {
                let order = items.len() + 1;
                items.push(HarnessManifestItem::pending(order, &step, target.clone()));
            }
        }
    }
    let final_step = final_report_step_definition();
    let order = items.len() + 1;
    items.push(HarnessManifestItem::pending(
        order,
        &final_step,
        "final_report",
    ));
    items
}

fn canonical_step_definitions() -> &'static [HarnessStepDefinition] {
    &[
        HarnessStepDefinition {
            step_id: "deterministic_runner",
            targets: &[AUTONOMOUS_TOOL_HARNESS_RUNNER],
            safe_input: "Export the canonical machine-readable harness manifest.",
            pass_condition: "Harness runner manifest output is persisted.",
            skip_condition: "harness_runner is absent from the active registry.",
            cleanup_requirement: "None.",
        },
        HarnessStepDefinition {
            step_id: "registry_discovery",
            targets: &[AUTONOMOUS_TOOL_TOOL_SEARCH, AUTONOMOUS_TOOL_TOOL_ACCESS],
            safe_input: "Inspect available tools and active registry metadata.",
            pass_condition: "Tool discovery result is persisted.",
            skip_condition: "Discovery tool is absent from the active registry.",
            cleanup_requirement: "None.",
        },
        HarnessStepDefinition {
            step_id: "repo_inspection",
            targets: &[
                AUTONOMOUS_TOOL_GIT_STATUS,
                AUTONOMOUS_TOOL_GIT_DIFF,
                AUTONOMOUS_TOOL_FIND,
                AUTONOMOUS_TOOL_SEARCH,
                AUTONOMOUS_TOOL_READ,
                AUTONOMOUS_TOOL_LIST,
                AUTONOMOUS_TOOL_HASH,
            ],
            safe_input: "Use read-only repo-scoped inspection inputs.",
            pass_condition: "Read-only repo inspection result is persisted.",
            skip_condition: "Inspection tool is absent or no safe repo fixture exists.",
            cleanup_requirement: "None.",
        },
        HarnessStepDefinition {
            step_id: "planning_runtime_state",
            targets: &[
                AUTONOMOUS_TOOL_TODO,
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE,
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH,
            ],
            safe_input: "Use runtime-owned planning or durable-context read/list actions.",
            pass_condition: "Runtime-state interaction result is persisted.",
            skip_condition: "Runtime-state tool is absent or has no harmless action.",
            cleanup_requirement: "Remove temporary todo/context scratch records if created.",
        },
        HarnessStepDefinition {
            step_id: "scratch_mutation",
            targets: &[
                AUTONOMOUS_TOOL_MKDIR,
                AUTONOMOUS_TOOL_WRITE,
                AUTONOMOUS_TOOL_EDIT,
                AUTONOMOUS_TOOL_RENAME,
                AUTONOMOUS_TOOL_DELETE,
            ],
            safe_input: "Use only repo-scoped harness scratch paths.",
            pass_condition: "Scratch-only mutation result is persisted.",
            skip_condition: "Mutation tool is absent or cannot be made scratch-only.",
            cleanup_requirement: "Cleanup scratch state in cleanup_scratch.",
        },
        HarnessStepDefinition {
            step_id: "commands",
            targets: &[
                AUTONOMOUS_TOOL_COMMAND_PROBE,
                AUTONOMOUS_TOOL_COMMAND_VERIFY,
                AUTONOMOUS_TOOL_COMMAND_RUN,
                AUTONOMOUS_TOOL_COMMAND_SESSION,
            ],
            safe_input: "Use short harmless commands and bounded session probes.",
            pass_condition: "Command result is persisted.",
            skip_condition: "Command tool is absent or safe command execution is unavailable.",
            cleanup_requirement: "Stop command sessions.",
        },
        HarnessStepDefinition {
            step_id: "process_manager",
            targets: &[AUTONOMOUS_TOOL_PROCESS_MANAGER],
            safe_input: "List or inspect only Xero-owned or harmless processes.",
            pass_condition: "Process-manager result is persisted.",
            skip_condition: "Process manager is absent or no Xero-owned process fixture exists.",
            cleanup_requirement: "Do not leave processes running.",
        },
        HarnessStepDefinition {
            step_id: "environment_diagnostics",
            targets: &[
                AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
                AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
                AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED,
            ],
            safe_input: "Read redacted environment and bounded diagnostics.",
            pass_condition: "Diagnostics result is persisted without secrets.",
            skip_condition: "Diagnostics tool is absent or platform probe is unsafe.",
            cleanup_requirement: "None.",
        },
        HarnessStepDefinition {
            step_id: "browser_tools",
            targets: &[
                AUTONOMOUS_TOOL_BROWSER_OBSERVE,
                AUTONOMOUS_TOOL_BROWSER_CONTROL,
            ],
            safe_input: "Observe first; control only a local or fixture-safe target.",
            pass_condition: "Browser result is persisted.",
            skip_condition: "Browser tool is absent or no local/safe target is available.",
            cleanup_requirement: "Restore or close harness-created browser state when possible.",
        },
        HarnessStepDefinition {
            step_id: "mcp_tools",
            targets: &[
                AUTONOMOUS_TOOL_MCP_LIST,
                AUTONOMOUS_TOOL_MCP_READ_RESOURCE,
                AUTONOMOUS_TOOL_MCP_GET_PROMPT,
                AUTONOMOUS_TOOL_MCP_CALL_TOOL,
            ],
            safe_input: "List MCP servers/resources/tools; invoke only safe fixtures.",
            pass_condition: "MCP result is persisted.",
            skip_condition: "MCP tool is absent or no safe fixture server/tool exists.",
            cleanup_requirement: "None.",
        },
        HarnessStepDefinition {
            step_id: "skills",
            targets: &[AUTONOMOUS_TOOL_SKILL],
            safe_input: "Discover/list/load safe local skill metadata only.",
            pass_condition: "Skill metadata result is persisted.",
            skip_condition: "Skill tool is absent or no safe fixture skill exists.",
            cleanup_requirement: "Do not install or invoke unsafe skills.",
        },
        HarnessStepDefinition {
            step_id: "emulator_tools",
            targets: &[AUTONOMOUS_TOOL_EMULATOR],
            safe_input: "Use only managed emulator fixtures.",
            pass_condition: "Emulator result is persisted.",
            skip_condition: "Emulator tool is absent or no managed fixture exists.",
            cleanup_requirement: "Stop or leave managed fixture in known idle state.",
        },
        HarnessStepDefinition {
            step_id: "solana_tools",
            targets: &[
                AUTONOMOUS_TOOL_SOLANA_CLUSTER,
                AUTONOMOUS_TOOL_SOLANA_LOGS,
                AUTONOMOUS_TOOL_SOLANA_TX,
                AUTONOMOUS_TOOL_SOLANA_SIMULATE,
                AUTONOMOUS_TOOL_SOLANA_EXPLAIN,
                AUTONOMOUS_TOOL_SOLANA_ALT,
                AUTONOMOUS_TOOL_SOLANA_IDL,
                AUTONOMOUS_TOOL_SOLANA_CODAMA,
                AUTONOMOUS_TOOL_SOLANA_PDA,
                AUTONOMOUS_TOOL_SOLANA_PROGRAM,
                AUTONOMOUS_TOOL_SOLANA_DEPLOY,
                AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK,
                AUTONOMOUS_TOOL_SOLANA_SQUADS,
                AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD,
                AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC,
                AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL,
                AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ,
                AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE,
                AUTONOMOUS_TOOL_SOLANA_REPLAY,
                AUTONOMOUS_TOOL_SOLANA_INDEXER,
                AUTONOMOUS_TOOL_SOLANA_SECRETS,
                AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT,
                AUTONOMOUS_TOOL_SOLANA_COST,
                AUTONOMOUS_TOOL_SOLANA_DOCS,
            ],
            safe_input: "Use local, devnet, read-only, or fixture-backed probes only.",
            pass_condition: "Solana tool result is persisted.",
            skip_condition: "Solana tool is absent or no safe local/devnet fixture exists.",
            cleanup_requirement: "Do not leave external-chain side effects.",
        },
        HarnessStepDefinition {
            step_id: "macos_automation",
            targets: &[AUTONOMOUS_TOOL_MACOS_AUTOMATION],
            safe_input: "Check permissions/status/read-only probes first.",
            pass_condition: "macOS automation result is persisted.",
            skip_condition: "macOS tool is absent or control probe is not explicitly safe.",
            cleanup_requirement: "Do not change user desktop state unless fixture-safe.",
        },
        HarnessStepDefinition {
            step_id: "cleanup_scratch",
            targets: &[AUTONOMOUS_TOOL_DELETE],
            safe_input: "Delete only harness-created scratch state.",
            pass_condition: "Scratch cleanup result is persisted or verified unnecessary.",
            skip_condition: "Delete tool is absent or no harness scratch state was created.",
            cleanup_requirement: "All harness-created scratch state removed.",
        },
    ]
}

fn dynamic_mcp_step_definition() -> HarnessStepDefinition {
    HarnessStepDefinition {
        step_id: "mcp_tools",
        targets: &[],
        safe_input: "Invoke only dynamic MCP tools backed by configured safe fixtures.",
        pass_condition: "Dynamic MCP result is persisted.",
        skip_condition: "No safe fixture-backed dynamic MCP invocation exists.",
        cleanup_requirement: "None.",
    }
}

fn final_report_step_definition() -> HarnessStepDefinition {
    HarnessStepDefinition {
        step_id: "final_report",
        targets: &[],
        safe_input: "Produce the exact harness test report shape.",
        pass_condition: "Final report is accepted after all manifest items are terminal.",
        skip_condition: "Final report cannot be skipped.",
        cleanup_requirement: "Scratch cleanup must already be terminal.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry(tool_names: &[&str]) -> ToolRegistry {
        ToolRegistry::for_tool_names_with_options(
            tool_names.iter().map(|tool| (*tool).to_owned()).collect(),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Test,
                skill_tool_enabled: true,
                ..ToolRegistryOptions::default()
            },
        )
    }

    fn test_registry_with_dynamic_mcp(
        tool_names: &[&str],
        dynamic_tool_name: &str,
    ) -> ToolRegistry {
        let mut descriptors = test_registry(tool_names).into_descriptors();
        descriptors.push(AgentToolDescriptor {
            name: dynamic_tool_name.into(),
            description: "Safe fixture MCP echo tool.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                }
            }),
        });
        ToolRegistry::from_descriptors_with_dynamic_routes(
            descriptors,
            BTreeMap::from([(
                dynamic_tool_name.into(),
                AutonomousDynamicToolRoute::McpTool {
                    server_id: "fixture".into(),
                    tool_name: "echo".into(),
                },
            )]),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::Test,
                skill_tool_enabled: true,
                ..ToolRegistryOptions::default()
            },
        )
    }

    fn controls_for_agent(runtime_agent_id: RuntimeAgentIdDto) -> RuntimeRunControlStateDto {
        RuntimeRunControlStateDto {
            active: RuntimeRunActiveControlSnapshotDto {
                runtime_agent_id,
                agent_definition_id: None,
                agent_definition_version: None,
                provider_profile_id: None,
                model_id: OPENAI_CODEX_PROVIDER_ID.into(),
                thinking_effort: None,
                approval_mode: RuntimeRunApprovalModeDto::Suggest,
                plan_mode_required: false,
                revision: 1,
                applied_at: "2026-05-05T00:00:00Z".into(),
            },
            pending: None,
        }
    }

    #[test]
    fn order_gate_attaches_only_to_test_agent_controls() {
        assert!(
            HarnessTestOrderGate::for_controls(&controls_for_agent(RuntimeAgentIdDto::Test))
                .is_some()
        );

        for runtime_agent_id in [
            RuntimeAgentIdDto::Ask,
            RuntimeAgentIdDto::Engineer,
            RuntimeAgentIdDto::Debug,
            RuntimeAgentIdDto::Crawl,
            RuntimeAgentIdDto::AgentCreate,
        ] {
            assert!(
                HarnessTestOrderGate::for_controls(&controls_for_agent(runtime_agent_id)).is_none(),
                "{runtime_agent_id:?} must not receive Test-agent harness ordering behavior"
            );
        }
    }

    #[test]
    fn manifest_orders_active_tools_by_canonical_harness_sequence() {
        let registry = test_registry(&[
            AUTONOMOUS_TOOL_HARNESS_RUNNER,
            AUTONOMOUS_TOOL_READ,
            AUTONOMOUS_TOOL_TOOL_ACCESS,
            AUTONOMOUS_TOOL_TOOL_SEARCH,
            AUTONOMOUS_TOOL_GIT_STATUS,
        ]);
        let items = canonical_manifest_items(&registry);
        let targets = items
            .iter()
            .map(|item| item.target.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            targets,
            vec![
                AUTONOMOUS_TOOL_HARNESS_RUNNER,
                AUTONOMOUS_TOOL_TOOL_SEARCH,
                AUTONOMOUS_TOOL_TOOL_ACCESS,
                AUTONOMOUS_TOOL_GIT_STATUS,
                AUTONOMOUS_TOOL_READ,
                "final_report",
            ]
        );
    }

    #[test]
    fn manifest_covers_phase6_groups_with_item_level_safety_metadata() {
        let dynamic_mcp = "mcp__fixture__echo__000000000000";
        let registry = test_registry_with_dynamic_mcp(
            &[
                AUTONOMOUS_TOOL_HARNESS_RUNNER,
                AUTONOMOUS_TOOL_TOOL_SEARCH,
                AUTONOMOUS_TOOL_TOOL_ACCESS,
                AUTONOMOUS_TOOL_GIT_STATUS,
                AUTONOMOUS_TOOL_TODO,
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
                AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
                AUTONOMOUS_TOOL_MKDIR,
                AUTONOMOUS_TOOL_WRITE,
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
                AUTONOMOUS_TOOL_MCP_CALL_TOOL,
                AUTONOMOUS_TOOL_SKILL,
                AUTONOMOUS_TOOL_EMULATOR,
                AUTONOMOUS_TOOL_SOLANA_CLUSTER,
                AUTONOMOUS_TOOL_MACOS_AUTOMATION,
            ],
            dynamic_mcp,
        );

        let items = canonical_manifest_items(&registry);
        let mut step_ids = items.iter().map(|item| item.step_id).collect::<Vec<_>>();
        step_ids.dedup();

        assert_eq!(
            step_ids,
            vec![
                "deterministic_runner",
                "registry_discovery",
                "repo_inspection",
                "planning_runtime_state",
                "scratch_mutation",
                "commands",
                "process_manager",
                "environment_diagnostics",
                "browser_tools",
                "mcp_tools",
                "skills",
                "emulator_tools",
                "solana_tools",
                "macos_automation",
                "cleanup_scratch",
                "final_report",
            ]
        );

        let target_index = |target: &str| {
            items
                .iter()
                .position(|item| item.target == target)
                .expect("manifest target")
        };
        assert!(target_index(AUTONOMOUS_TOOL_MCP_LIST) < target_index(dynamic_mcp));
        assert!(target_index(dynamic_mcp) < target_index(AUTONOMOUS_TOOL_SKILL));

        let runner = items
            .iter()
            .find(|item| {
                item.step_id == "deterministic_runner"
                    && item.target == AUTONOMOUS_TOOL_HARNESS_RUNNER
            })
            .expect("harness runner item");
        assert_eq!(runner.safe_input, r#"{"action":"manifest"}"#);
        assert_eq!(runner.effect_class, "observe");

        for item in &items {
            assert_eq!(
                item.order,
                items
                    .iter()
                    .position(|candidate| candidate == item)
                    .unwrap()
                    + 1
            );
            assert!(!item.item_id.trim().is_empty());
            assert!(!item.effect_class.trim().is_empty());
            assert!(!item.safe_input.trim().is_empty());
            assert!(!item.pass_condition.trim().is_empty());
            assert!(!item.skip_condition.trim().is_empty());
            assert!(!item.cleanup_requirement.trim().is_empty());
        }

        let write = items
            .iter()
            .find(|item| item.step_id == "scratch_mutation" && item.target == AUTONOMOUS_TOOL_WRITE)
            .expect("scratch write item");
        assert!(write.safe_input.contains("xero-harness-scratch/probe.txt"));
        assert_eq!(write.effect_class, "write");

        let cleanup = items
            .iter()
            .find(|item| item.step_id == "cleanup_scratch" && item.target == AUTONOMOUS_TOOL_DELETE)
            .expect("cleanup delete item");
        assert!(cleanup.safe_input.contains(r#""recursive":true"#));
        assert!(cleanup
            .cleanup_requirement
            .contains("All harness-created scratch state removed"));
    }

    #[test]
    fn final_report_rows_parse_skipped_manifest_items() {
        let rows = parse_final_report_rows(
            r#"# Harness Test Report
| Step | Target | Status | Evidence | Skip reason |
| --- | --- | --- | --- | --- |
| browser_tools | browser_observe | skipped_with_reason | none | no local target |
"#,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].step_id, "browser_tools");
        assert_eq!(rows[0].target, "browser_observe");
        assert_eq!(rows[0].status, HarnessStepStatus::SkippedWithReason);
        assert_eq!(rows[0].skip_reason, "no local target");
    }

    #[test]
    fn harness_runner_compares_model_report_to_machine_manifest() {
        let registry = test_registry(&[
            AUTONOMOUS_TOOL_HARNESS_RUNNER,
            AUTONOMOUS_TOOL_TOOL_SEARCH,
            AUTONOMOUS_TOOL_TOOL_ACCESS,
        ]);
        let report = r#"# Harness Test Report
| Step | Target | Status | Evidence | Skip reason |
| --- | --- | --- | --- | --- |
| deterministic_runner | harness_runner | passed | persisted manifest | none |
| registry_discovery | tool_search | passed | persisted search | none |
| registry_discovery | tool_access | passed | persisted access | none |
"#;
        let (_, output) = harness_runner_tool_output(
            &registry,
            &AutonomousHarnessRunnerRequest {
                action: AutonomousHarnessRunnerAction::CompareReport,
                final_report: Some(report.into()),
            },
        )
        .expect("compare report");

        assert_eq!(output["passed"], json!(true));

        let bad_report = r#"# Harness Test Report
| Step | Target | Status | Evidence | Skip reason |
| --- | --- | --- | --- | --- |
| registry_discovery | tool_search | passed | none | none |
"#;
        let (_, output) = harness_runner_tool_output(
            &registry,
            &AutonomousHarnessRunnerRequest {
                action: AutonomousHarnessRunnerAction::CompareReport,
                final_report: Some(bad_report.into()),
            },
        )
        .expect("compare bad report");

        assert_eq!(output["passed"], json!(false));
        assert!(output["comparison"]["missingRows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()));
        assert!(output["comparison"]["unsafeRows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()));
    }
}
