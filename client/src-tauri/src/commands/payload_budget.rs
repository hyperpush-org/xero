use serde::Serialize;

use super::contracts::surface::PayloadBudgetDiagnosticDto;

pub const RUNTIME_STREAM_ITEM_BUDGET_BYTES: usize = 32 * 1024;
pub const REPOSITORY_STATUS_BUDGET_BYTES: usize = 384 * 1024;
pub const REPOSITORY_DIFF_BUDGET_BYTES: usize = 96 * 1024;
pub const PROJECT_TREE_BUDGET_BYTES: usize = 512 * 1024;
pub const PROJECT_TREE_NODE_BUDGET: usize = 5_000;
pub const PROJECT_FILE_INDEX_BUDGET_BYTES: usize = 768 * 1024;
pub const PROJECT_SEARCH_RESULTS_BUDGET_BYTES: usize = 1024 * 1024;
pub const BROWSER_EVENT_BUDGET_BYTES: usize = 8 * 1024;
pub const BROWSER_CONSOLE_EVENT_BUDGET_BYTES: usize = 16 * 1024;
pub const EMULATOR_FRAME_EVENT_BUDGET_BYTES: usize = 1024;
pub const PROVIDER_REGISTRY_BUDGET_BYTES: usize = 512 * 1024;
pub const SETTINGS_REGISTRY_BUDGET_BYTES: usize = 256 * 1024;

pub fn estimate_serialized_payload_bytes<T: Serialize>(payload: &T) -> usize {
    serde_json::to_vec(payload)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

pub fn payload_budget_diagnostic(
    key: &'static str,
    label: &'static str,
    budget_bytes: usize,
    observed_bytes: usize,
    truncated: bool,
    dropped: bool,
) -> Option<PayloadBudgetDiagnosticDto> {
    if observed_bytes <= budget_bytes && !truncated && !dropped {
        return None;
    }

    let action = if dropped {
        "dropped"
    } else if truncated {
        "truncated"
    } else {
        "exceeded"
    };

    Some(PayloadBudgetDiagnosticDto {
        key: key.into(),
        budget_bytes: budget_bytes.min(u32::MAX as usize) as u32,
        observed_bytes: observed_bytes.min(u32::MAX as usize) as u32,
        truncated,
        dropped,
        message: format!(
            "Xero {action} the {label} payload after observing {observed_bytes} bytes against a {budget_bytes} byte budget."
        ),
    })
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::{
        estimate_serialized_payload_bytes, payload_budget_diagnostic, PROJECT_TREE_BUDGET_BYTES,
    };

    #[derive(Serialize)]
    struct SamplePayload {
        label: String,
        values: Vec<String>,
    }

    #[test]
    fn estimates_serialized_payload_bytes() {
        let payload = SamplePayload {
            label: "project-tree".into(),
            values: vec!["src/main.rs".into(), "src/lib.rs".into()],
        };

        let observed = estimate_serialized_payload_bytes(&payload);

        assert!(observed > payload.label.len());
        assert!(observed < 256);
    }

    #[test]
    fn emits_diagnostic_when_budget_is_exceeded_or_truncated() {
        let exceeded = payload_budget_diagnostic(
            "project_tree",
            "project tree",
            PROJECT_TREE_BUDGET_BYTES,
            PROJECT_TREE_BUDGET_BYTES + 1,
            false,
            false,
        )
        .expect("over-budget payload should report a diagnostic");
        assert_eq!(exceeded.key, "project_tree");
        assert!(!exceeded.truncated);
        assert!(!exceeded.dropped);

        let truncated = payload_budget_diagnostic(
            "project_tree",
            "project tree",
            PROJECT_TREE_BUDGET_BYTES,
            128,
            true,
            false,
        )
        .expect("truncated payload should report a diagnostic");
        assert!(truncated.truncated);
    }
}
