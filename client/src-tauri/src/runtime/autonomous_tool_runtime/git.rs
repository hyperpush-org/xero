use crate::{
    commands::{BranchSummaryDto, CommandResult, RepositoryDiffScope},
    git::{diff, status},
};

use super::{
    AutonomousGitDiffOutput, AutonomousGitDiffRequest, AutonomousGitStatusOutput,
    AutonomousGitStatusRequest, AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime,
    AUTONOMOUS_TOOL_GIT_DIFF, AUTONOMOUS_TOOL_GIT_STATUS,
};

impl AutonomousToolRuntime {
    pub fn git_status(
        &self,
        _request: AutonomousGitStatusRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let response = status::load_repository_status_from_root(&self.repo_root)?;
        let branch_label = display_branch_name(response.branch.as_ref());
        let changed_files = response.entries.len();
        let summary = if changed_files == 0 {
            format!("Git status reports a clean worktree on `{branch_label}`.")
        } else {
            format!(
                "Git status reports {changed_files} changed file(s) on `{branch_label}` (staged: {}, unstaged: {}, untracked: {}).",
                response.has_staged_changes,
                response.has_unstaged_changes,
                response.has_untracked_changes
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_GIT_STATUS.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::GitStatus(AutonomousGitStatusOutput {
                branch: response.branch,
                entries: response.entries,
                changed_files,
                has_staged_changes: response.has_staged_changes,
                has_unstaged_changes: response.has_unstaged_changes,
                has_untracked_changes: response.has_untracked_changes,
            }),
        })
    }

    pub fn git_diff(
        &self,
        request: AutonomousGitDiffRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let projection = diff::load_repository_diff_from_root(&self.repo_root, request.scope)?;
        let response = projection.response;
        let branch_label = display_branch_name(projection.branch.as_ref());
        let truncation_suffix = if response.truncated {
            format!("; patch truncated at {} byte(s)", diff::MAX_PATCH_BYTES)
        } else {
            String::new()
        };
        let summary = format!(
            "Rendered {} git diff for {} changed file(s) on `{branch_label}`{}.",
            scope_label(response.scope),
            projection.changed_files,
            truncation_suffix
        );

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_GIT_DIFF.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::GitDiff(AutonomousGitDiffOutput {
                scope: response.scope,
                branch: projection.branch,
                changed_files: projection.changed_files,
                patch: response.patch,
                truncated: response.truncated,
                base_revision: response.base_revision,
            }),
        })
    }
}

fn display_branch_name(branch: Option<&BranchSummaryDto>) -> String {
    branch
        .map(|branch| branch.name.clone())
        .filter(|branch| !branch.trim().is_empty())
        .unwrap_or_else(|| "HEAD".into())
}

fn scope_label(scope: RepositoryDiffScope) -> &'static str {
    match scope {
        RepositoryDiffScope::Staged => "staged",
        RepositoryDiffScope::Unstaged => "unstaged",
        RepositoryDiffScope::Worktree => "worktree",
    }
}
