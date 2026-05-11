use serde_json::Value as JsonValue;

use super::Diagnostics;

const KNOWN_RECORD_KINDS: &[&str] = &[
    "agent_handoff",
    "project_fact",
    "decision",
    "constraint",
    "plan",
    "finding",
    "verification",
    "question",
    "artifact",
    "context_note",
    "diagnostic",
];

const KNOWN_MEMORY_KINDS: &[&str] = &[
    "project_fact",
    "user_preference",
    "decision",
    "session_summary",
    "troubleshooting",
];

pub(super) fn validate(value: Option<&JsonValue>, diagnostics: &mut Diagnostics) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_retrieval_defaults_invalid",
            "retrievalDefaults must be an object when provided.",
            "retrievalDefaults",
        ));
        return;
    };

    if object
        .get("enabled")
        .is_some_and(|value| value.as_bool().is_none())
    {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_retrieval_defaults_field_invalid",
            "retrievalDefaults.enabled must be a boolean.",
            "retrievalDefaults.enabled",
        ));
    }
    validate_known_strings(
        object.get("recordKinds"),
        "retrievalDefaults.recordKinds",
        KNOWN_RECORD_KINDS,
        "agent_definition_retrieval_record_kind_unknown",
        diagnostics,
    );
    validate_known_strings(
        object.get("memoryKinds"),
        "retrievalDefaults.memoryKinds",
        KNOWN_MEMORY_KINDS,
        "agent_definition_retrieval_memory_kind_unknown",
        diagnostics,
    );
    if object
        .get("limit")
        .is_some_and(|value| value.as_u64().filter(|limit| *limit > 0).is_none())
    {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_retrieval_limit_invalid",
            "retrievalDefaults.limit must be a positive integer.",
            "retrievalDefaults.limit",
        ));
    }
}

fn validate_known_strings(
    value: Option<&JsonValue>,
    path: &'static str,
    known: &[&str],
    code: &'static str,
    diagnostics: &mut Diagnostics,
) {
    let Some(value) = value else {
        return;
    };
    let Some(items) = value.as_array() else {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_retrieval_defaults_field_invalid",
            format!("{path} must be an array."),
            path,
        ));
        return;
    };
    for item in items {
        let Some(kind) = item.as_str().map(str::trim).filter(|kind| !kind.is_empty()) else {
            diagnostics.push(super::super::diagnostic(
                "agent_definition_retrieval_defaults_field_invalid",
                format!("{path} entries must be non-empty strings."),
                path,
            ));
            continue;
        };
        if !known.contains(&kind) {
            diagnostics.push(super::super::diagnostic(
                code,
                format!("Retrieval kind `{kind}` is not known to Xero."),
                path,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_retrieval_codes_in_isolation() {
        let retrieval = json!({
            "enabled": "yes",
            "recordKinds": ["not_a_record_kind"],
            "memoryKinds": ["not_a_memory_kind"],
            "limit": 0
        });

        let mut diagnostics = Vec::new();
        super::validate(Some(&retrieval), &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert!(codes.contains(&"agent_definition_retrieval_defaults_field_invalid".into()));
        assert!(codes.contains(&"agent_definition_retrieval_record_kind_unknown".into()));
        assert!(codes.contains(&"agent_definition_retrieval_memory_kind_unknown".into()));
        assert!(codes.contains(&"agent_definition_retrieval_limit_invalid".into()));
    }
}
