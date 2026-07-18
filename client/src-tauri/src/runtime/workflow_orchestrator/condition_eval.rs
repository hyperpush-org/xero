use std::collections::BTreeMap;

use serde_json::{json, Value as JsonValue};

use crate::commands::contracts::workflows::{
    WorkflowConditionDto, WorkflowNodeRunStatusDto, WorkflowNumberCompareOperatorDto,
};

#[derive(Debug, Clone, Default)]
pub struct WorkflowConditionContext {
    pub node_statuses: BTreeMap<String, WorkflowNodeRunStatusDto>,
    pub artifacts: BTreeMap<String, JsonValue>,
    pub state_values: BTreeMap<String, JsonValue>,
    pub failure_classes: BTreeMap<String, String>,
    pub latest_failure_class: Option<String>,
    pub loop_attempts: BTreeMap<String, u32>,
    pub human_decisions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowConditionEvaluation {
    pub matched: bool,
    pub evidence: JsonValue,
}

pub fn evaluate_workflow_condition(
    condition: &WorkflowConditionDto,
    context: &WorkflowConditionContext,
) -> WorkflowConditionEvaluation {
    match condition {
        WorkflowConditionDto::Always => WorkflowConditionEvaluation {
            matched: true,
            evidence: json!({ "kind": "always", "matched": true }),
        },
        WorkflowConditionDto::All { conditions } => {
            let evaluations = conditions
                .iter()
                .map(|condition| evaluate_workflow_condition(condition, context))
                .collect::<Vec<_>>();
            let matched = evaluations.iter().all(|evaluation| evaluation.matched);
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "all",
                    "matched": matched,
                    "children": evaluations.into_iter().map(|evaluation| evaluation.evidence).collect::<Vec<_>>()
                }),
            }
        }
        WorkflowConditionDto::Any { conditions } => {
            let evaluations = conditions
                .iter()
                .map(|condition| evaluate_workflow_condition(condition, context))
                .collect::<Vec<_>>();
            let matched = evaluations.iter().any(|evaluation| evaluation.matched);
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "any",
                    "matched": matched,
                    "children": evaluations.into_iter().map(|evaluation| evaluation.evidence).collect::<Vec<_>>()
                }),
            }
        }
        WorkflowConditionDto::Not { condition } => {
            let evaluation = evaluate_workflow_condition(condition, context);
            WorkflowConditionEvaluation {
                matched: !evaluation.matched,
                evidence: json!({
                    "kind": "not",
                    "matched": !evaluation.matched,
                    "child": evaluation.evidence
                }),
            }
        }
        WorkflowConditionDto::NodeStatus { node_id, status } => {
            let actual = context.node_statuses.get(node_id).copied();
            let matched = actual == Some(*status);
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "node_status",
                    "nodeId": node_id,
                    "expected": status.as_str(),
                    "actual": actual.map(WorkflowNodeRunStatusDto::as_str),
                    "matched": matched
                }),
            }
        }
        WorkflowConditionDto::ArtifactExists { artifact_ref } => {
            let matched = context.artifacts.contains_key(artifact_ref);
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "artifact_exists",
                    "artifactRef": artifact_ref,
                    "matched": matched
                }),
            }
        }
        WorkflowConditionDto::ArtifactFieldEquals {
            artifact_ref,
            path,
            value,
        } => {
            let actual = context
                .artifacts
                .get(artifact_ref)
                .and_then(|artifact| json_path_lookup(artifact, path));
            let matched = actual == Some(value);
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "artifact_field_equals",
                    "artifactRef": artifact_ref,
                    "path": path,
                    "expected": value,
                    "actual": actual.cloned(),
                    "matched": matched
                }),
            }
        }
        WorkflowConditionDto::ArtifactFieldIn {
            artifact_ref,
            path,
            values,
        } => {
            let actual = context
                .artifacts
                .get(artifact_ref)
                .and_then(|artifact| json_path_lookup(artifact, path));
            let matched = actual
                .map(|actual| values.iter().any(|value| value == actual))
                .unwrap_or(false);
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "artifact_field_in",
                    "artifactRef": artifact_ref,
                    "path": path,
                    "values": values,
                    "actual": actual.cloned(),
                    "matched": matched
                }),
            }
        }
        WorkflowConditionDto::ArtifactFieldNumberCompare {
            artifact_ref,
            path,
            operator,
            value,
        } => {
            let actual = context
                .artifacts
                .get(artifact_ref)
                .and_then(|artifact| json_path_lookup(artifact, path))
                .and_then(JsonValue::as_f64);
            let matched = actual
                .map(|actual| compare_numbers(actual, *operator, *value))
                .unwrap_or(false);
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "artifact_field_number_compare",
                    "artifactRef": artifact_ref,
                    "path": path,
                    "operator": format!("{operator:?}"),
                    "expected": value,
                    "actual": actual,
                    "matched": matched
                }),
            }
        }
        WorkflowConditionDto::FailureClassIs {
            node_id,
            failure_class,
        } => {
            let actual = match node_id {
                Some(node_id) => context.failure_classes.get(node_id).cloned(),
                None => context.latest_failure_class.clone(),
            };
            let matched = actual.as_deref() == Some(failure_class.as_str());
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "failure_class_is",
                    "nodeId": node_id,
                    "expected": failure_class,
                    "actual": actual,
                    "matched": matched
                }),
            }
        }
        WorkflowConditionDto::LoopAttemptLt { loop_key, value } => {
            let actual = context.loop_attempts.get(loop_key).copied().unwrap_or(0);
            let matched = actual < *value;
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "loop_attempt_lt",
                    "loopKey": loop_key,
                    "expected": value,
                    "actual": actual,
                    "matched": matched
                }),
            }
        }
        WorkflowConditionDto::LoopAttemptGte { loop_key, value } => {
            let actual = context.loop_attempts.get(loop_key).copied().unwrap_or(0);
            let matched = actual >= *value;
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "loop_attempt_gte",
                    "loopKey": loop_key,
                    "expected": value,
                    "actual": actual,
                    "matched": matched
                }),
            }
        }
        WorkflowConditionDto::HumanDecisionIs {
            checkpoint_node_id,
            decision,
        } => {
            let actual = context.human_decisions.get(checkpoint_node_id);
            let matched = actual.map(String::as_str) == Some(decision.as_str());
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "human_decision_is",
                    "checkpointNodeId": checkpoint_node_id,
                    "expected": decision,
                    "actual": actual,
                    "matched": matched
                }),
            }
        }
        WorkflowConditionDto::StateFieldEquals {
            state_ref,
            path,
            value,
        } => {
            let actual = context
                .state_values
                .get(state_ref)
                .and_then(|state| json_path_lookup(state, path));
            let matched = actual == Some(value);
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "state_field_equals",
                    "stateRef": state_ref,
                    "path": path,
                    "expected": value,
                    "actual": actual.cloned(),
                    "matched": matched
                }),
            }
        }
        WorkflowConditionDto::StateCollectionCountCompare {
            state_ref,
            operator,
            value,
        } => {
            let actual = context.state_values.get(state_ref).and_then(|state| {
                state
                    .get("records")
                    .and_then(JsonValue::as_array)
                    .map(|records| records.len() as f64)
                    .or_else(|| state.as_array().map(|records| records.len() as f64))
            });
            let matched = actual
                .map(|actual| compare_numbers(actual, *operator, *value))
                .unwrap_or(false);
            WorkflowConditionEvaluation {
                matched,
                evidence: json!({
                    "kind": "state_collection_count_compare",
                    "stateRef": state_ref,
                    "operator": format!("{operator:?}"),
                    "expected": value,
                    "actual": actual,
                    "matched": matched
                }),
            }
        }
    }
}

fn compare_numbers(actual: f64, operator: WorkflowNumberCompareOperatorDto, expected: f64) -> bool {
    match operator {
        WorkflowNumberCompareOperatorDto::Eq => (actual - expected).abs() < f64::EPSILON,
        WorkflowNumberCompareOperatorDto::Neq => (actual - expected).abs() >= f64::EPSILON,
        WorkflowNumberCompareOperatorDto::Gt => actual > expected,
        WorkflowNumberCompareOperatorDto::Gte => actual >= expected,
        WorkflowNumberCompareOperatorDto::Lt => actual < expected,
        WorkflowNumberCompareOperatorDto::Lte => actual <= expected,
    }
}

pub fn json_path_lookup<'a>(value: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    let mut cursor = value;
    let trimmed = path.trim();
    if trimmed == "$" {
        return Some(cursor);
    }
    let mut remainder = trimmed.strip_prefix('$')?;
    if remainder.starts_with('[') {
        let segment_end = remainder.find('.').unwrap_or(remainder.len());
        let indexes = parse_index_suffix(&remainder[..segment_end])?;
        if indexes.is_empty() {
            return None;
        }
        for index in indexes {
            cursor = cursor.get(index)?;
        }
        if segment_end == remainder.len() {
            return Some(cursor);
        }
        remainder = &remainder[segment_end + 1..];
        if remainder.is_empty() {
            return None;
        }
    } else {
        remainder = remainder.strip_prefix('.')?;
    }
    for segment in remainder.split('.') {
        if segment.is_empty() {
            return None;
        }
        let (field, indexes) = parse_path_segment(segment)?;
        cursor = cursor.get(field)?;
        for index in indexes {
            cursor = cursor.get(index)?;
        }
    }
    Some(cursor)
}

pub(super) fn lookup_run_input_binding<'a>(
    initial_input: Option<&'a JsonValue>,
    name: &str,
    path: Option<&str>,
) -> Option<&'a JsonValue> {
    let input = initial_input?;
    match path {
        Some(path) => json_path_lookup(input, path),
        None => json_path_lookup(input, &format!("$.{name}")),
    }
}

fn parse_path_segment(segment: &str) -> Option<(&str, Vec<usize>)> {
    let field_end = segment.find('[').unwrap_or(segment.len());
    let field = &segment[..field_end];
    if field.is_empty() {
        return None;
    }
    let indexes = parse_index_suffix(&segment[field_end..])?;
    Some((field, indexes))
}

fn parse_index_suffix(mut rest: &str) -> Option<Vec<usize>> {
    let mut indexes = Vec::new();
    while !rest.is_empty() {
        let inner = rest.strip_prefix('[')?;
        let close = inner.find(']')?;
        let index = inner[..close].parse::<usize>().ok()?;
        indexes.push(index);
        rest = &inner[close + 1..];
    }
    Some(indexes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::contracts::workflows::{
        WorkflowConditionDto, WorkflowNodeRunStatusDto, WorkflowNumberCompareOperatorDto,
    };

    #[test]
    fn condition_eval_matches_artifact_field() {
        let mut context = WorkflowConditionContext::default();
        context.artifacts.insert(
            "verify.verification_result".into(),
            json!({ "status": "gaps_found", "gaps": [{ "id": "a" }] }),
        );

        let result = evaluate_workflow_condition(
            &WorkflowConditionDto::ArtifactFieldEquals {
                artifact_ref: "verify.verification_result".into(),
                path: "$.status".into(),
                value: json!("gaps_found"),
            },
            &context,
        );

        assert!(result.matched);
    }

    #[test]
    fn condition_eval_compares_loop_attempts() {
        let mut context = WorkflowConditionContext::default();
        context.loop_attempts.insert("gap_closure".into(), 1);

        let result = evaluate_workflow_condition(
            &WorkflowConditionDto::LoopAttemptLt {
                loop_key: "gap_closure".into(),
                value: 2,
            },
            &context,
        );

        assert!(result.matched);
    }

    #[test]
    fn condition_eval_matches_node_status() {
        let mut context = WorkflowConditionContext::default();
        context
            .node_statuses
            .insert("work".into(), WorkflowNodeRunStatusDto::Succeeded);

        let result = evaluate_workflow_condition(
            &WorkflowConditionDto::NodeStatus {
                node_id: "work".into(),
                status: WorkflowNodeRunStatusDto::Succeeded,
            },
            &context,
        );

        assert!(result.matched);
    }

    #[test]
    fn condition_eval_reads_array_json_path() {
        let value = json!({ "findings": [{ "severity": "high" }] });

        let actual = json_path_lookup(&value, "$.findings[0].severity");

        assert_eq!(actual, Some(&json!("high")));
    }

    #[test]
    fn condition_eval_compares_numbers() {
        let mut context = WorkflowConditionContext::default();
        context
            .artifacts
            .insert("review.review_findings".into(), json!({ "high_count": 0 }));

        let result = evaluate_workflow_condition(
            &WorkflowConditionDto::ArtifactFieldNumberCompare {
                artifact_ref: "review.review_findings".into(),
                path: "$.high_count".into(),
                operator: WorkflowNumberCompareOperatorDto::Eq,
                value: 0.0,
            },
            &context,
        );

        assert!(result.matched);
    }

    #[test]
    fn condition_eval_composes_all_any_and_not_conditions() {
        let mut context = WorkflowConditionContext::default();
        context.artifacts.insert("plan.output".into(), json!({}));

        let result = evaluate_workflow_condition(
            &WorkflowConditionDto::All {
                conditions: vec![
                    WorkflowConditionDto::Always,
                    WorkflowConditionDto::Any {
                        conditions: vec![
                            WorkflowConditionDto::ArtifactExists {
                                artifact_ref: "missing.output".into(),
                            },
                            WorkflowConditionDto::ArtifactExists {
                                artifact_ref: "plan.output".into(),
                            },
                        ],
                    },
                    WorkflowConditionDto::Not {
                        condition: Box::new(WorkflowConditionDto::ArtifactExists {
                            artifact_ref: "missing.output".into(),
                        }),
                    },
                ],
            },
            &context,
        );

        assert!(result.matched);
        assert_eq!(result.evidence["kind"], "all");
        assert_eq!(result.evidence["children"][1]["kind"], "any");
        assert_eq!(result.evidence["children"][2]["kind"], "not");

        let unmatched = evaluate_workflow_condition(
            &WorkflowConditionDto::Any {
                conditions: vec![WorkflowConditionDto::ArtifactExists {
                    artifact_ref: "missing.output".into(),
                }],
            },
            &context,
        );
        assert!(!unmatched.matched);
    }

    #[test]
    fn condition_eval_matches_membership_human_decisions_and_state() {
        let mut context = WorkflowConditionContext::default();
        context
            .artifacts
            .insert("review.result".into(), json!({ "status": "needs_changes" }));
        context
            .human_decisions
            .insert("approval".into(), "continue".into());
        context.state_values.insert(
            "state.items".into(),
            json!({ "status": "ready", "records": [{}, {}] }),
        );
        context
            .state_values
            .insert("state.array".into(), json!([1, 2, 3]));

        for condition in [
            WorkflowConditionDto::ArtifactFieldIn {
                artifact_ref: "review.result".into(),
                path: "$.status".into(),
                values: vec![json!("approved"), json!("needs_changes")],
            },
            WorkflowConditionDto::HumanDecisionIs {
                checkpoint_node_id: "approval".into(),
                decision: "continue".into(),
            },
            WorkflowConditionDto::StateFieldEquals {
                state_ref: "state.items".into(),
                path: "$.status".into(),
                value: json!("ready"),
            },
            WorkflowConditionDto::StateCollectionCountCompare {
                state_ref: "state.items".into(),
                operator: WorkflowNumberCompareOperatorDto::Eq,
                value: 2.0,
            },
            WorkflowConditionDto::StateCollectionCountCompare {
                state_ref: "state.array".into(),
                operator: WorkflowNumberCompareOperatorDto::Gte,
                value: 3.0,
            },
        ] {
            assert!(evaluate_workflow_condition(&condition, &context).matched);
        }

        assert!(
            !evaluate_workflow_condition(
                &WorkflowConditionDto::ArtifactFieldIn {
                    artifact_ref: "review.result".into(),
                    path: "$.missing".into(),
                    values: vec![JsonValue::Null],
                },
                &context,
            )
            .matched
        );
        assert!(
            !evaluate_workflow_condition(
                &WorkflowConditionDto::StateCollectionCountCompare {
                    state_ref: "missing".into(),
                    operator: WorkflowNumberCompareOperatorDto::Eq,
                    value: 0.0,
                },
                &context,
            )
            .matched
        );
    }

    #[test]
    fn failure_class_condition_respects_explicit_node_scope() {
        let mut context = WorkflowConditionContext::default();
        context
            .failure_classes
            .insert("build".into(), "compile_failed".into());
        context.latest_failure_class = Some("compile_failed".into());

        let unrelated_node = evaluate_workflow_condition(
            &WorkflowConditionDto::FailureClassIs {
                node_id: Some("test".into()),
                failure_class: "compile_failed".into(),
            },
            &context,
        );
        assert!(!unrelated_node.matched);
        assert!(unrelated_node.evidence["actual"].is_null());

        let explicit_node = evaluate_workflow_condition(
            &WorkflowConditionDto::FailureClassIs {
                node_id: Some("build".into()),
                failure_class: "compile_failed".into(),
            },
            &context,
        );
        assert!(explicit_node.matched);

        let latest = evaluate_workflow_condition(
            &WorkflowConditionDto::FailureClassIs {
                node_id: None,
                failure_class: "compile_failed".into(),
            },
            &context,
        );
        assert!(latest.matched);
    }

    #[test]
    fn condition_eval_covers_loop_boundaries_and_number_operators() {
        let mut context = WorkflowConditionContext::default();
        context.loop_attempts.insert("retry".into(), 2);

        assert!(
            evaluate_workflow_condition(
                &WorkflowConditionDto::LoopAttemptGte {
                    loop_key: "retry".into(),
                    value: 2,
                },
                &context,
            )
            .matched
        );
        assert!(
            !evaluate_workflow_condition(
                &WorkflowConditionDto::LoopAttemptLt {
                    loop_key: "retry".into(),
                    value: 2,
                },
                &context,
            )
            .matched
        );
        assert!(
            evaluate_workflow_condition(
                &WorkflowConditionDto::LoopAttemptLt {
                    loop_key: "missing".into(),
                    value: 1,
                },
                &context,
            )
            .matched
        );

        for (operator, expected, matched) in [
            (WorkflowNumberCompareOperatorDto::Eq, 2.0, true),
            (WorkflowNumberCompareOperatorDto::Neq, 3.0, true),
            (WorkflowNumberCompareOperatorDto::Gt, 1.0, true),
            (WorkflowNumberCompareOperatorDto::Gte, 2.0, true),
            (WorkflowNumberCompareOperatorDto::Lt, 3.0, true),
            (WorkflowNumberCompareOperatorDto::Lte, 2.0, true),
            (WorkflowNumberCompareOperatorDto::Eq, 3.0, false),
        ] {
            assert_eq!(compare_numbers(2.0, operator, expected), matched);
        }
    }

    #[test]
    fn json_path_lookup_and_run_input_binding_reject_malformed_paths() {
        let value = json!({
            "goal": "ship",
            "nested": { "items": [{ "name": "first" }] }
        });

        assert_eq!(json_path_lookup(&value, "$"), Some(&value));
        assert_eq!(
            json_path_lookup(&value, "$.nested.items[0].name"),
            Some(&json!("first"))
        );
        assert_eq!(
            json_path_lookup(&json!([{ "name": "first" }]), "$[0].name"),
            Some(&json!("first"))
        );
        for path in [
            "goal",
            "$.nested..items",
            "$.[0]",
            "$.nested.items[nope]",
            "$.nested.items[0",
            "$.nested.items[0]tail",
            "$.nested.items[9]",
            "$[0].",
            "$[nope]",
            "$[0]tail",
        ] {
            assert_eq!(json_path_lookup(&value, path), None, "path {path}");
        }

        assert_eq!(
            lookup_run_input_binding(Some(&value), "goal", None),
            Some(&json!("ship"))
        );
        assert_eq!(
            lookup_run_input_binding(Some(&value), "ignored", Some("$.nested.items[0]")),
            Some(&json!({ "name": "first" }))
        );
        assert_eq!(lookup_run_input_binding(None, "goal", None), None);
    }
}
