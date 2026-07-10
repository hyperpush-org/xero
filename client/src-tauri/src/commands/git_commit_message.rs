use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        backend_jobs::BackendCancellationToken,
        runtime_support::resolve_owned_agent_provider_config, validate_non_empty, CommandError,
        CommandResult, GitGenerateCommitMessageRequestDto, GitGenerateCommitMessageResponseDto,
        RepositoryDiffFileDto, RepositoryDiffScope, RuntimeAgentIdDto,
        RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto, RuntimeRunControlInputDto,
        RuntimeRunControlStateDto,
    },
    git::diff,
    runtime::{
        create_provider_adapter, ProviderAdapter, ProviderMessage, ProviderStreamEvent,
        ProviderTurnOutcome, ProviderTurnRequest,
    },
    state::DesktopState,
};

const COMMIT_MESSAGE_SYSTEM_PROMPT: &str = "You write polished Git commit messages from staged diffs. Return only the commit message text, with no markdown, quotes, labels, or explanation. Use a concise Conventional Commit subject when the change clearly fits, such as feat:, fix:, refactor:, docs:, test:, or chore:. Use imperative mood and keep the first line at 72 characters or less. Add a short body only when it clarifies important behavior, risk, or migration context. If changes are broad or unrelated, use a neutral subject that reflects the dominant user-visible outcome. Do not mention AI, the prompt, the model, or the diff.";
const COMMIT_MESSAGE_MAX_DIFF_BYTES_TOTAL: usize = 192 * 1024;

#[tauri::command]
pub async fn git_generate_commit_message<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitGenerateCommitMessageRequestDto,
) -> CommandResult<GitGenerateCommitMessageResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.model_id, "modelId")?;

    let jobs = state.backend_jobs().clone();
    let state = state.inner().clone();
    let project_id = request.project_id.clone();

    jobs.run_blocking_latest(
        format!("git-commit-message:{project_id}"),
        "commit message generation",
        move |cancellation| git_generate_commit_message_blocking(app, state, request, cancellation),
    )
    .await
}

fn git_generate_commit_message_blocking<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: GitGenerateCommitMessageRequestDto,
    cancellation: BackendCancellationToken,
) -> CommandResult<GitGenerateCommitMessageResponseDto> {
    cancellation.check_cancelled("commit message generation")?;
    let registry_path = state.global_db_path(&app)?;
    cancellation.check_cancelled("commit message generation")?;
    let diff = diff::load_repository_diff_with_patch_budget(
        &request.project_id,
        RepositoryDiffScope::Staged,
        COMMIT_MESSAGE_MAX_DIFF_BYTES_TOTAL,
        &registry_path,
    )?;
    cancellation.check_cancelled("commit message generation")?;
    if diff.files.is_empty() {
        return Err(CommandError::user_fixable(
            "git_commit_message_no_staged_changes",
            "Stage changes before generating a commit message.",
        ));
    }

    let controls = RuntimeRunControlInputDto {
        runtime_agent_id: RuntimeAgentIdDto::Engineer,
        agent_definition_id: None,
        agent_definition_version: None,
        provider_profile_id: normalize_optional_text(request.provider_profile_id),
        model_id: request.model_id.trim().to_owned(),
        thinking_effort: request.thinking_effort.clone(),
        approval_mode: RuntimeRunApprovalModeDto::Yolo,
        plan_mode_required: false,
        auto_compact_enabled: false,
    };
    let provider_config = resolve_owned_agent_provider_config(&app, &state, Some(&controls))?;
    let provider = create_provider_adapter(provider_config)?;
    let provider_id = provider.provider_id().to_owned();
    let provider_model_id = provider.model_id().to_owned();
    let controls_state = RuntimeRunControlStateDto {
        active: RuntimeRunActiveControlSnapshotDto {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            provider_profile_id: controls.provider_profile_id.clone(),
            model_id: provider_model_id.clone(),
            thinking_effort: controls.thinking_effort.clone(),
            approval_mode: RuntimeRunApprovalModeDto::Yolo,
            plan_mode_required: false,
            auto_compact_enabled: false,
            revision: 1,
            applied_at: now_timestamp(),
        },
        pending: None,
    };

    let outcome = generate_commit_message_from_staged_diff(
        provider.as_ref(),
        &request.project_id,
        &diff.files,
        &diff.patch,
        diff.truncated,
        controls_state,
        &cancellation,
    )?;

    Ok(GitGenerateCommitMessageResponseDto {
        message: outcome.message,
        provider_id,
        model_id: provider_model_id,
        diff_truncated: outcome.diff_truncated,
    })
}

#[derive(Debug)]
struct CommitMessageGenerationOutcome {
    message: String,
    diff_truncated: bool,
}

fn generate_commit_message_from_staged_diff(
    provider: &dyn ProviderAdapter,
    project_id: &str,
    staged_files: &[RepositoryDiffFileDto],
    staged_patch: &str,
    diff_truncated: bool,
    controls: RuntimeRunControlStateDto,
    cancellation: &BackendCancellationToken,
) -> CommandResult<CommitMessageGenerationOutcome> {
    cancellation.check_cancelled("commit message generation")?;
    let output_allowance =
        provider.resolve_turn_output_allowance(None, controls.active.thinking_effort.as_ref())?;
    let turn = ProviderTurnRequest {
        system_prompt: COMMIT_MESSAGE_SYSTEM_PROMPT.into(),
        messages: vec![ProviderMessage::User {
            content: build_commit_message_prompt(
                project_id,
                staged_files,
                staged_patch,
                diff_truncated,
            ),
            attachments: Vec::new(),
        }],
        tools: Vec::new(),
        turn_index: 0,
        output_allowance,
        controls,
    };
    let mut emit = |_event: ProviderStreamEvent| Ok(());
    match provider.stream_turn(&turn, &mut emit)? {
        ProviderTurnOutcome::Complete { message, .. } => Ok(CommitMessageGenerationOutcome {
            message: sanitize_provider_commit_message(&message)?,
            diff_truncated,
        }),
        ProviderTurnOutcome::ToolCalls { .. } => Err(CommandError::retryable(
            "git_commit_message_provider_requested_tools",
            "Xero asked the selected model to generate a commit message without tools, but the provider requested tool calls.",
        )),
    }
}

fn build_commit_message_prompt(
    project_id: &str,
    files: &[RepositoryDiffFileDto],
    staged_patch: &str,
    diff_truncated: bool,
) -> String {
    let file_overview = staged_file_overview(files);
    let patch_excerpt = staged_patch_excerpt_for_prompt(staged_patch);
    let truncated_label = if diff_truncated { "yes" } else { "no" };

    format!(
        "Generate a Git commit message for the staged changes in project `{}`.\nThe staged file list below is complete. Xero inspected a bounded staged patch excerpt before this provider turn. Use only these staged-change details. If the excerpt is truncated or omits a file's content, keep the message broad and avoid unsupported behavior-specific claims.\n\nStaged files ({}):\n{}\n\nStaged patch excerpt bytes: {}. Truncated: {}.\n--- BEGIN STAGED PATCH EXCERPT ---\n{}\n--- END STAGED PATCH EXCERPT ---",
        project_id.trim(),
        files.len(),
        file_overview,
        staged_patch.len(),
        truncated_label,
        patch_excerpt
    )
}

fn staged_patch_excerpt_for_prompt(staged_patch: &str) -> String {
    let trimmed = staged_patch.trim_end();
    if trimmed.is_empty() {
        "(No textual staged patch content was captured within the budget.)".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn staged_file_overview(files: &[RepositoryDiffFileDto]) -> String {
    if files.is_empty() {
        return "No staged files were reported.".to_owned();
    }

    files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            format!(
                "{}. [{}] {}",
                index + 1,
                change_kind_label(&file.status),
                file_display_path(file)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn file_display_path(file: &RepositoryDiffFileDto) -> String {
    match (&file.old_path, &file.new_path) {
        (Some(old_path), Some(new_path)) if old_path != new_path => {
            format!("{old_path} -> {new_path}")
        }
        _ => file.display_path.clone(),
    }
}

fn change_kind_label(kind: &crate::commands::ChangeKind) -> &'static str {
    match kind {
        crate::commands::ChangeKind::Added => "added",
        crate::commands::ChangeKind::Modified => "modified",
        crate::commands::ChangeKind::Deleted => "deleted",
        crate::commands::ChangeKind::Renamed => "renamed",
        crate::commands::ChangeKind::Copied => "copied",
        crate::commands::ChangeKind::TypeChange => "type changed",
        crate::commands::ChangeKind::Conflicted => "conflicted",
    }
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
    use std::{collections::VecDeque, sync::Mutex};

    use serde_json::json;

    use super::{
        build_commit_message_prompt, generate_commit_message_from_staged_diff,
        sanitize_provider_commit_message,
    };
    use crate::runtime::agent_core::ProviderTurnOutputAllowance;
    use crate::{
        commands::{
            backend_jobs::BackendCancellationToken, ChangeKind, RepositoryDiffFileDto,
            RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto,
            RuntimeRunControlStateDto,
        },
        runtime::{
            AgentToolCall, ProviderAdapter, ProviderMessage, ProviderStreamEvent,
            ProviderTurnOutcome, ProviderTurnRequest,
        },
    };

    struct ScriptedCommitMessageProvider {
        outcomes: Mutex<VecDeque<ProviderTurnOutcome>>,
        requests: Mutex<Vec<ProviderTurnRequest>>,
    }

    impl ScriptedCommitMessageProvider {
        fn new(outcomes: Vec<ProviderTurnOutcome>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes.into()),
                requests: Mutex::new(Vec::new()),
            }
        }

        fn captured_requests(&self) -> Vec<ProviderTurnRequest> {
            self.requests
                .lock()
                .expect("scripted commit-message request lock")
                .clone()
        }
    }

    impl ProviderAdapter for ScriptedCommitMessageProvider {
        fn provider_id(&self) -> &str {
            "test_provider"
        }

        fn model_id(&self) -> &str {
            "test-model"
        }

        fn resolve_turn_output_allowance(
            &self,
            _provider_preflight: Option<&xero_agent_core::ProviderPreflightSnapshot>,
            _thinking_effort: Option<&crate::commands::ProviderModelThinkingEffortDto>,
        ) -> crate::commands::CommandResult<ProviderTurnOutputAllowance> {
            ProviderTurnOutputAllowance::unified(1_024)
        }

        fn stream_turn(
            &self,
            request: &ProviderTurnRequest,
            _emit: &mut dyn FnMut(ProviderStreamEvent) -> crate::commands::CommandResult<()>,
        ) -> crate::commands::CommandResult<ProviderTurnOutcome> {
            self.requests
                .lock()
                .expect("scripted commit-message request lock")
                .push(request.clone());
            Ok(self
                .outcomes
                .lock()
                .expect("scripted commit-message outcome lock")
                .pop_front()
                .unwrap_or(ProviderTurnOutcome::Complete {
                    message: "fix: update staged changes".into(),
                    reasoning_content: None,
                    reasoning_details: None,
                    usage: None,
                }))
        }
    }

    #[test]
    fn preserves_body_while_collapsing_extra_blank_lines() {
        let message = "fix: generate commit messages\n\n\n\nUse the staged diff only.";
        assert_eq!(
            sanitize_provider_commit_message(message).expect("message is valid"),
            "fix: generate commit messages\n\nUse the staged diff only."
        );
    }

    #[test]
    fn commit_message_prompt_lists_files_and_includes_bounded_diff() {
        let prompt = build_commit_message_prompt(
            "project-1",
            &[
                staged_file("included.rs", ChangeKind::Modified),
                staged_file("omitted.rs", ChangeKind::Modified),
            ],
            "diff --git a/included.rs b/included.rs\n+visible\n",
            true,
        );

        assert!(prompt.contains("Staged files (2):"));
        assert!(prompt.contains("[modified] included.rs"));
        assert!(prompt.contains("[modified] omitted.rs"));
        assert!(prompt.contains("The staged file list below is complete"));
        assert!(prompt.contains("Xero inspected a bounded staged patch excerpt"));
        assert!(prompt.contains("Staged patch excerpt bytes: 48. Truncated: yes."));
        assert!(prompt.contains("diff --git a/included.rs b/included.rs"));
        assert!(!prompt.contains("read_staged_diff"));
    }

    #[test]
    fn commit_message_generation_sends_one_toolless_provider_turn() {
        let provider = ScriptedCommitMessageProvider::new(vec![ProviderTurnOutcome::Complete {
            message: "commit message: fix: stabilize commit messages".into(),
            reasoning_content: None,
            reasoning_details: None,
            usage: None,
        }]);
        let files = vec![staged_file(
            "client/src-tauri/src/commands/git_commit_message.rs",
            ChangeKind::Modified,
        )];

        let outcome = generate_commit_message_from_staged_diff(
            &provider,
            "project-1",
            &files,
            "diff --git a/client/src-tauri/src/commands/git_commit_message.rs b/client/src-tauri/src/commands/git_commit_message.rs\n+stable\n",
            true,
            test_controls(),
            &BackendCancellationToken::default(),
        )
        .expect("commit message generation should complete");

        assert_eq!(outcome.message, "fix: stabilize commit messages");
        assert!(outcome.diff_truncated);
        let requests = provider.captured_requests();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].tools.is_empty());
        assert_eq!(requests[0].turn_index, 0);
        assert_eq!(requests[0].messages.len(), 1);
        let ProviderMessage::User { content, .. } = &requests[0].messages[0] else {
            panic!("expected user prompt");
        };
        assert!(content.contains("bounded staged patch excerpt"));
        assert!(content.contains("+stable"));
    }

    #[test]
    fn commit_message_generation_rejects_provider_tool_calls_without_looping() {
        let provider = ScriptedCommitMessageProvider::new(vec![ProviderTurnOutcome::ToolCalls {
            message: "I will inspect the diff.".into(),
            reasoning_content: None,
            reasoning_details: None,
            tool_calls: vec![AgentToolCall {
                tool_call_id: "call-1".into(),
                tool_name: "read_staged_diff".into(),
                input: json!({ "paths": ["file.rs"] }),
            }],
            usage: None,
        }]);
        let files = vec![staged_file("file.rs", ChangeKind::Modified)];

        let error = generate_commit_message_from_staged_diff(
            &provider,
            "project-1",
            &files,
            "diff --git a/file.rs b/file.rs\n+stable\n",
            false,
            test_controls(),
            &BackendCancellationToken::default(),
        )
        .expect_err("tool calls are invalid for commit-message generation");

        assert_eq!(error.code, "git_commit_message_provider_requested_tools");
        let requests = provider.captured_requests();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].tools.is_empty());
    }

    fn staged_file(path: &str, status: ChangeKind) -> RepositoryDiffFileDto {
        RepositoryDiffFileDto {
            old_path: Some(path.into()),
            new_path: Some(path.into()),
            display_path: path.into(),
            status,
            hunks: Vec::new(),
            patch: String::new(),
            truncated: false,
            cache_key: path.into(),
        }
    }

    fn test_controls() -> RuntimeRunControlStateDto {
        RuntimeRunControlStateDto {
            active: RuntimeRunActiveControlSnapshotDto {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                provider_profile_id: None,
                model_id: "test-model".into(),
                thinking_effort: None,
                approval_mode: RuntimeRunApprovalModeDto::Yolo,
                plan_mode_required: false,
                auto_compact_enabled: false,
                revision: 1,
                applied_at: "2026-06-05T00:00:00Z".into(),
            },
            pending: None,
        }
    }
}
