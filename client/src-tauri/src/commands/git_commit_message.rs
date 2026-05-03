use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        runtime_support::resolve_owned_agent_provider_config, validate_non_empty, CommandError,
        CommandResult, GitGenerateCommitMessageRequestDto, GitGenerateCommitMessageResponseDto,
        RepositoryDiffScope, RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto,
        RuntimeRunApprovalModeDto, RuntimeRunControlInputDto, RuntimeRunControlStateDto,
    },
    git::diff,
    runtime::{
        create_provider_adapter, ProviderMessage, ProviderStreamEvent, ProviderTurnOutcome,
        ProviderTurnRequest,
    },
    state::DesktopState,
};

const COMMIT_MESSAGE_SYSTEM_PROMPT: &str = "You write polished Git commit messages from staged diffs. Return only the commit message text. Prefer a concise Conventional Commit subject when the change clearly fits, such as feat:, fix:, refactor:, docs:, test:, or chore:. Use imperative mood, keep the first line at 72 characters or less, and include a short body only when it explains important context, risk, or behavior. Do not mention AI, the prompt, or the diff.";

#[tauri::command]
pub fn git_generate_commit_message<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitGenerateCommitMessageRequestDto,
) -> CommandResult<GitGenerateCommitMessageResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.model_id, "modelId")?;

    let registry_path = state.global_db_path(&app)?;
    let diff = diff::load_repository_diff(
        &request.project_id,
        RepositoryDiffScope::Staged,
        &registry_path,
    )?;
    if diff.patch.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "git_commit_message_no_staged_changes",
            "Stage changes before generating a commit message.",
        ));
    }

    let controls = RuntimeRunControlInputDto {
        runtime_agent_id: RuntimeAgentIdDto::Engineer,
        agent_definition_id: None,
        provider_profile_id: normalize_optional_text(request.provider_profile_id),
        model_id: request.model_id.trim().to_owned(),
        thinking_effort: request.thinking_effort.clone(),
        approval_mode: RuntimeRunApprovalModeDto::Yolo,
        plan_mode_required: false,
    };
    let provider_config =
        resolve_owned_agent_provider_config(&app, state.inner(), Some(&controls))?;
    let provider = create_provider_adapter(provider_config)?;
    let provider_id = provider.provider_id().to_owned();
    let provider_model_id = provider.model_id().to_owned();

    let user_prompt = build_commit_message_prompt(&request.project_id, &diff.patch, diff.truncated);
    let turn = ProviderTurnRequest {
        system_prompt: COMMIT_MESSAGE_SYSTEM_PROMPT.into(),
        messages: vec![ProviderMessage::User {
            content: user_prompt,
            attachments: Vec::new(),
        }],
        tools: Vec::new(),
        turn_index: 0,
        controls: RuntimeRunControlStateDto {
            active: RuntimeRunActiveControlSnapshotDto {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                provider_profile_id: controls.provider_profile_id.clone(),
                model_id: provider_model_id.clone(),
                thinking_effort: controls.thinking_effort.clone(),
                approval_mode: RuntimeRunApprovalModeDto::Yolo,
                plan_mode_required: false,
                revision: 1,
                applied_at: now_timestamp(),
            },
            pending: None,
        },
    };

    let mut emit = |_event: ProviderStreamEvent| Ok(());
    let message = match provider.stream_turn(&turn, &mut emit)? {
        ProviderTurnOutcome::Complete { message, .. } => message,
        ProviderTurnOutcome::ToolCalls { .. } => {
            return Err(CommandError::user_fixable(
                "git_commit_message_provider_requested_tools",
                "Xero asked the selected model for a commit message, but the model requested tools instead.",
            ));
        }
    };
    let message = sanitize_provider_commit_message(&message)?;

    Ok(GitGenerateCommitMessageResponseDto {
        message,
        provider_id,
        model_id: provider_model_id,
        diff_truncated: diff.truncated,
    })
}

fn build_commit_message_prompt(project_id: &str, patch: &str, truncated: bool) -> String {
    let truncation_note = if truncated {
        "The staged diff was truncated to fit the commit-message context window. Base the message on the visible staged changes and avoid claiming details that are not shown."
    } else {
        "The full staged diff is shown."
    };

    format!(
        "Generate a Git commit message for the staged changes in project `{}`.\n{}\n\nStaged diff:\n```diff\n{}\n```",
        project_id.trim(),
        truncation_note,
        patch
    )
}

fn sanitize_provider_commit_message(message: &str) -> CommandResult<String> {
    let mut text = strip_markdown_fence(message);
    text = strip_label_prefix(&text);
    text = strip_wrapping_quotes(&text);
    text = collapse_excess_blank_lines(&text);
    let text = text.trim().to_owned();

    if text.is_empty() {
        return Err(CommandError::retryable(
            "git_commit_message_empty",
            "The selected model returned an empty commit message.",
        ));
    }

    Ok(text)
}

fn strip_markdown_fence(message: &str) -> String {
    let trimmed = message.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_owned();
    }

    let mut lines: Vec<&str> = trimmed.lines().collect();
    if lines
        .first()
        .is_some_and(|line| line.trim_start().starts_with("```"))
    {
        lines.remove(0);
    }
    if lines.last().is_some_and(|line| line.trim_end() == "```") {
        lines.pop();
    }
    lines.join("\n").trim().to_owned()
}

fn strip_label_prefix(message: &str) -> String {
    let trimmed = message.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["commit message:", "commit:", "message:"] {
        if lower.starts_with(prefix) {
            return trimmed[prefix.len()..].trim_start().to_owned();
        }
    }
    trimmed.to_owned()
}

fn strip_wrapping_quotes(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.len() < 2 || trimmed.contains('\n') {
        return trimmed.to_owned();
    }

    let pairs = [('"', '"'), ('\'', '\''), ('`', '`')];
    for (left, right) in pairs {
        if trimmed.starts_with(left) && trimmed.ends_with(right) {
            return trimmed[1..trimmed.len() - 1].trim().to_owned();
        }
    }

    trimmed.to_owned()
}

fn collapse_excess_blank_lines(message: &str) -> String {
    let mut output = Vec::new();
    let mut blank_count = 0usize;
    for line in message.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                output.push(String::new());
            }
            continue;
        }
        blank_count = 0;
        output.push(line.trim_end().to_owned());
    }
    output.join("\n")
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::sanitize_provider_commit_message;

    #[test]
    fn sanitizes_common_model_wrappers() {
        let message = "```gitcommit\nCommit message: \"feat: add source control helper\"\n```";
        assert_eq!(
            sanitize_provider_commit_message(message).expect("message is valid"),
            "feat: add source control helper"
        );
    }

    #[test]
    fn preserves_body_while_collapsing_extra_blank_lines() {
        let message = "fix: generate commit messages\n\n\n\nUse the staged diff only.";
        assert_eq!(
            sanitize_provider_commit_message(message).expect("message is valid"),
            "fix: generate commit messages\n\nUse the staged diff only."
        );
    }
}
