use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::{
    commands::CommandResult,
    db::project_store::{self, AgentMemoryKind, AgentMemoryScope},
};

const CODE_HISTORY_PROVENANCE_REQUIRED_CODE: &str =
    "session_memory_candidate_code_history_provenance_required";
const CODE_HISTORY_PROVENANCE_REQUIRED_MESSAGE: &str =
    "Xero skipped a memory candidate because it described code later changed by undo or session return without citing that history operation.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeHistoryMemoryDiagnostic {
    pub code: &'static str,
    pub message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeHistoryMemoryOperation {
    source_item_id: String,
    operation_id: String,
    operation_kind: CodeHistoryMemoryOperationKind,
    mode: String,
    status: String,
    affected_paths: Vec<String>,
    target_change_group_id: Option<String>,
    result_change_group_id: Option<String>,
    result_commit_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeHistoryMemoryOperationKind {
    CodeHistory,
    LegacyRollback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeHistoryMemoryGuard {
    operations: Vec<CodeHistoryMemoryOperation>,
    affected_terms: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodeHistoryMemoryGuardOutcome {
    Accepted {
        scope: AgentMemoryScope,
        kind: AgentMemoryKind,
        text: String,
        source_item_ids: Vec<String>,
    },
    Rejected(CodeHistoryMemoryDiagnostic),
}

impl CodeHistoryMemoryGuard {
    pub(crate) fn for_session(
        repo_root: &Path,
        project_id: &str,
        agent_session_id: &str,
        run_id: Option<&str>,
    ) -> CommandResult<Self> {
        let mut operations = Vec::new();
        for operation in project_store::list_code_history_operations_for_session(
            repo_root,
            project_id,
            agent_session_id,
            run_id,
        )? {
            if code_history_status_changed_workspace(&operation.status) {
                operations.push(CodeHistoryMemoryOperation::from_code_history(operation));
            }
        }
        for operation in project_store::list_code_rollback_operations_for_session(
            repo_root,
            project_id,
            agent_session_id,
            run_id,
        )? {
            if code_history_status_changed_workspace(&operation.status) {
                operations.push(CodeHistoryMemoryOperation::from_legacy_rollback(operation));
            }
        }
        Ok(Self::new(operations))
    }

    pub(crate) fn new(operations: Vec<CodeHistoryMemoryOperation>) -> Self {
        let affected_terms = operations
            .iter()
            .flat_map(|operation| operation.affected_paths.iter())
            .flat_map(|path| affected_path_terms(path))
            .collect::<BTreeSet<_>>();
        Self {
            operations,
            affected_terms,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub(crate) fn operation_lines(&self) -> Vec<(String, String)> {
        self.operations
            .iter()
            .map(|operation| (operation.source_item_id.clone(), operation.memory_line()))
            .collect()
    }

    pub(crate) fn apply(
        &self,
        scope: AgentMemoryScope,
        kind: AgentMemoryKind,
        text: String,
        explicit_source_item_ids: Vec<String>,
    ) -> CodeHistoryMemoryGuardOutcome {
        if self.is_empty() || !is_durable_code_memory(&scope, &kind) {
            return CodeHistoryMemoryGuardOutcome::Accepted {
                scope,
                kind,
                text,
                source_item_ids: explicit_source_item_ids,
            };
        }

        if self.has_candidate_provenance(&text, &explicit_source_item_ids) {
            let source_item_ids =
                self.with_history_provenance_source_ids(&text, explicit_source_item_ids);
            return CodeHistoryMemoryGuardOutcome::Accepted {
                scope,
                kind,
                text,
                source_item_ids,
            };
        }

        if self.text_mentions_affected_code(&text) {
            return CodeHistoryMemoryGuardOutcome::Rejected(CodeHistoryMemoryDiagnostic {
                code: CODE_HISTORY_PROVENANCE_REQUIRED_CODE,
                message: CODE_HISTORY_PROVENANCE_REQUIRED_MESSAGE,
            });
        }

        if looks_like_code_implementation_fact(&text) {
            let mut source_item_ids = explicit_source_item_ids;
            self.append_latest_operation_source_id(&mut source_item_ids);
            return CodeHistoryMemoryGuardOutcome::Accepted {
                scope: AgentMemoryScope::Session,
                kind: AgentMemoryKind::SessionSummary,
                text: format!("Historical before code undo: {text}"),
                source_item_ids,
            };
        }

        CodeHistoryMemoryGuardOutcome::Accepted {
            scope,
            kind,
            text,
            source_item_ids: explicit_source_item_ids,
        }
    }

    fn has_candidate_provenance(&self, text: &str, source_item_ids: &[String]) -> bool {
        source_item_ids
            .iter()
            .any(|source_item_id| self.is_operation_source_item_id(source_item_id))
            || text_mentions_history_provenance(text)
            || self
                .operations
                .iter()
                .any(|operation| operation.text_cites(text))
    }

    fn with_history_provenance_source_ids(
        &self,
        text: &str,
        mut source_item_ids: Vec<String>,
    ) -> Vec<String> {
        for operation in self.operations.iter().take(4) {
            if operation.text_cites(text) || text_mentions_history_provenance(text) {
                push_unique(&mut source_item_ids, operation.source_item_id.clone());
            }
        }
        source_item_ids
    }

    fn append_latest_operation_source_id(&self, source_item_ids: &mut Vec<String>) {
        if let Some(operation) = self.operations.last() {
            push_unique(source_item_ids, operation.source_item_id.clone());
        }
    }

    fn is_operation_source_item_id(&self, source_item_id: &str) -> bool {
        self.operations
            .iter()
            .any(|operation| operation.matches_source_item_id(source_item_id))
    }

    fn text_mentions_affected_code(&self, text: &str) -> bool {
        let normalized = normalize_for_matching(text);
        self.affected_terms
            .iter()
            .any(|term| normalized.contains(term.as_str()))
    }
}

impl CodeHistoryMemoryOperation {
    fn from_code_history(operation: project_store::CodeHistoryOperationRecord) -> Self {
        Self {
            source_item_id: format!("code_history_operation:{}", operation.operation_id),
            operation_id: operation.operation_id,
            operation_kind: CodeHistoryMemoryOperationKind::CodeHistory,
            mode: operation.mode,
            status: operation.status,
            affected_paths: operation.affected_paths,
            target_change_group_id: operation.target_change_group_id,
            result_change_group_id: operation.result_change_group_id,
            result_commit_id: operation.result_commit_id,
        }
    }

    fn from_legacy_rollback(operation: project_store::CodeRollbackOperationRecord) -> Self {
        let affected_paths = operation
            .affected_files
            .iter()
            .flat_map(|file| [file.path_before.as_deref(), file.path_after.as_deref()])
            .flatten()
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        Self {
            source_item_id: format!("code_rollback:{}", operation.operation_id),
            operation_id: operation.operation_id,
            operation_kind: CodeHistoryMemoryOperationKind::LegacyRollback,
            mode: "legacy_rollback".into(),
            status: operation.status,
            affected_paths,
            target_change_group_id: Some(operation.target_change_group_id),
            result_change_group_id: operation.result_change_group_id,
            result_commit_id: None,
        }
    }

    fn memory_line(&self) -> String {
        format!(
            "operationId={} mode={} status={} affectedPaths={} targetChangeGroupId={} resultChangeGroupId={} resultCommitId={}. Conversation history stayed append-only; any reverted implementation detail needs explicit code-history provenance before it can become durable memory.",
            self.operation_id,
            self.mode,
            self.status,
            if self.affected_paths.is_empty() {
                "none".into()
            } else {
                self.affected_paths.join(", ")
            },
            self.target_change_group_id.as_deref().unwrap_or("none"),
            self.result_change_group_id.as_deref().unwrap_or("none"),
            self.result_commit_id.as_deref().unwrap_or("none"),
        )
    }

    fn matches_source_item_id(&self, source_item_id: &str) -> bool {
        if source_item_id == self.source_item_id {
            return true;
        }
        match self.operation_kind {
            CodeHistoryMemoryOperationKind::CodeHistory => {
                source_item_id == format!("code_history_operations:{}", self.operation_id)
            }
            CodeHistoryMemoryOperationKind::LegacyRollback => {
                source_item_id == format!("code_rollback_operations:{}", self.operation_id)
            }
        }
    }

    fn text_cites(&self, text: &str) -> bool {
        let normalized = normalize_for_matching(text);
        [
            Some(self.operation_id.as_str()),
            self.target_change_group_id.as_deref(),
            self.result_change_group_id.as_deref(),
            self.result_commit_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|value| value.len() >= 6)
        .map(normalize_for_matching)
        .any(|id| normalized.contains(id.as_str()))
    }
}

fn code_history_status_changed_workspace(status: &str) -> bool {
    matches!(status, "completed" | "repair_needed")
}

fn is_durable_code_memory(scope: &AgentMemoryScope, kind: &AgentMemoryKind) -> bool {
    matches!(scope, AgentMemoryScope::Project)
        && matches!(
            kind,
            AgentMemoryKind::ProjectFact | AgentMemoryKind::Decision
        )
}

fn text_mentions_history_provenance(text: &str) -> bool {
    let normalized = normalize_for_matching(text);
    [
        "code undo",
        "undo operation",
        "code history operation",
        "history operation",
        "session rollback",
        "session return",
        "session returned",
        "return session to here",
        "rollback operation",
        "rolled back by code history",
        "reverted by code undo",
        "reverted by history operation",
        "historical before code undo",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}

fn looks_like_code_implementation_fact(text: &str) -> bool {
    let normalized = normalize_for_matching(text);
    [
        "src/",
        ".rs",
        ".ts",
        ".tsx",
        ".js",
        ".jsx",
        ".py",
        ".go",
        ".swift",
        ".java",
        ".json",
        ".toml",
        ".yaml",
        ".yml",
        "function",
        "component",
        "module",
        "route",
        "schema",
        "command",
        "crate",
        "package",
        "dependency",
        "migration",
        "api endpoint",
        "struct ",
        "enum ",
        "method",
        "provider",
        "adapter",
        "implementation",
        "implements",
        "implemented",
        "configured",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn affected_path_terms(path: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let normalized_path = normalize_for_matching(path);
    if !normalized_path.is_empty() {
        terms.insert(normalized_path);
    }

    let path_buf = PathBuf::from(path);
    if let Some(file_name) = path_buf.file_name().and_then(|value| value.to_str()) {
        insert_path_term(&mut terms, file_name);
    }
    if let Some(stem) = path_buf.file_stem().and_then(|value| value.to_str()) {
        insert_path_term(&mut terms, stem);
    }
    for component in path_buf.components() {
        if let Some(component) = component.as_os_str().to_str() {
            insert_path_term(&mut terms, component);
        }
    }
    terms
}

fn insert_path_term(terms: &mut BTreeSet<String>, value: &str) {
    let normalized = normalize_for_matching(value);
    if normalized.len() < 4 || is_generic_path_term(&normalized) {
        return;
    }
    terms.insert(normalized);
}

fn is_generic_path_term(value: &str) -> bool {
    matches!(
        value,
        "src"
            | "lib"
            | "mod"
            | "main"
            | "test"
            | "tests"
            | "index"
            | "client"
            | "server"
            | "components"
            | "commands"
            | "runtime"
    )
}

fn normalize_for_matching(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_project_fact_about_affected_path_without_history_provenance() {
        let guard = CodeHistoryMemoryGuard::new(vec![CodeHistoryMemoryOperation {
            source_item_id: "code_history_operation:history-op-1".into(),
            operation_id: "history-op-1".into(),
            operation_kind: CodeHistoryMemoryOperationKind::CodeHistory,
            mode: "selective_undo".into(),
            status: "completed".into(),
            affected_paths: vec!["src/tracked.txt".into()],
            target_change_group_id: Some("change-1".into()),
            result_change_group_id: Some("undo-1".into()),
            result_commit_id: Some("commit-1".into()),
        }]);

        let outcome = guard.apply(
            AgentMemoryScope::Project,
            AgentMemoryKind::ProjectFact,
            "src/tracked.txt uses the reverted implementation.".into(),
            Vec::new(),
        );

        assert!(matches!(
            outcome,
            CodeHistoryMemoryGuardOutcome::Rejected(CodeHistoryMemoryDiagnostic {
                code: CODE_HISTORY_PROVENANCE_REQUIRED_CODE,
                ..
            })
        ));
    }

    #[test]
    fn allows_project_fact_when_history_operation_provenance_is_explicit() {
        let guard = CodeHistoryMemoryGuard::new(vec![CodeHistoryMemoryOperation {
            source_item_id: "code_history_operation:history-op-1".into(),
            operation_id: "history-op-1".into(),
            operation_kind: CodeHistoryMemoryOperationKind::CodeHistory,
            mode: "selective_undo".into(),
            status: "completed".into(),
            affected_paths: vec!["src/tracked.txt".into()],
            target_change_group_id: Some("change-1".into()),
            result_change_group_id: Some("undo-1".into()),
            result_commit_id: Some("commit-1".into()),
        }]);

        let outcome = guard.apply(
            AgentMemoryScope::Project,
            AgentMemoryKind::ProjectFact,
            "Historical before code undo history-op-1: src/tracked.txt used the reverted implementation.".into(),
            Vec::new(),
        );

        let CodeHistoryMemoryGuardOutcome::Accepted {
            source_item_ids, ..
        } = outcome
        else {
            panic!("expected candidate to be accepted with provenance");
        };
        assert!(source_item_ids
            .iter()
            .any(|source_item_id| source_item_id == "code_history_operation:history-op-1"));
    }

    #[test]
    fn scopes_code_implementation_fact_as_historical_when_path_is_not_named() {
        let guard = CodeHistoryMemoryGuard::new(vec![CodeHistoryMemoryOperation {
            source_item_id: "code_history_operation:history-op-1".into(),
            operation_id: "history-op-1".into(),
            operation_kind: CodeHistoryMemoryOperationKind::CodeHistory,
            mode: "session_rollback".into(),
            status: "completed".into(),
            affected_paths: vec!["client/components/UndoPanel.tsx".into()],
            target_change_group_id: Some("change-1".into()),
            result_change_group_id: Some("undo-1".into()),
            result_commit_id: Some("commit-1".into()),
        }]);

        let outcome = guard.apply(
            AgentMemoryScope::Project,
            AgentMemoryKind::Decision,
            "The undo component implementation uses a menu action.".into(),
            Vec::new(),
        );

        let CodeHistoryMemoryGuardOutcome::Accepted {
            scope, kind, text, ..
        } = outcome
        else {
            panic!("expected candidate to be accepted as historical session memory");
        };
        assert_eq!(scope, AgentMemoryScope::Session);
        assert_eq!(kind, AgentMemoryKind::SessionSummary);
        assert!(text.starts_with("Historical before code undo:"));
    }
}
