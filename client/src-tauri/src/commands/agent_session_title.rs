use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        agent_session::agent_session_dto, runtime_support::resolve_owned_agent_provider_config,
        runtime_support::resolve_project_root, validate_non_empty, AgentSessionDto,
        AutoNameAgentSessionRequestDto, CommandError, CommandResult,
        ProviderModelThinkingEffortDto, RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto,
        RuntimeRunApprovalModeDto, RuntimeRunControlInputDto, RuntimeRunControlStateDto,
    },
    db::project_store::{self, AgentSessionUpdateRecord, DEFAULT_AGENT_SESSION_TITLE},
    runtime::{
        create_provider_adapter, ProviderMessage, ProviderStreamEvent, ProviderTurnOutcome,
        ProviderTurnRequest,
    },
    state::DesktopState,
};

const SESSION_TITLE_SYSTEM_PROMPT: &str = "You name developer-assistant chat sessions from the user's first prompt. Return only one concise title. Use 2 to 6 words. No markdown, no quotes, no trailing punctuation, no generic labels like New Chat. Capture the user's intent, not the assistant's behavior.";
const MAX_TITLE_CHARS: usize = 64;
const MAX_PROMPT_CHARS: usize = 4_000;

#[tauri::command]
pub fn auto_name_agent_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: AutoNameAgentSessionRequestDto,
) -> CommandResult<AgentSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.prompt, "prompt")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let existing = project_store::get_agent_session(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "agent_session_missing",
            format!(
                "Xero could not find agent session `{}` for project `{}`.",
                request.agent_session_id, request.project_id
            ),
        )
    })?;

    if !is_default_session_title(&existing.title) {
        return Ok(agent_session_dto(&existing));
    }

    let generated_title = generate_session_title(
        &app,
        state.inner(),
        request.controls.as_ref(),
        &request.prompt,
    )?;

    let current = project_store::get_agent_session(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "agent_session_missing",
            format!(
                "Xero could not find agent session `{}` for project `{}`.",
                request.agent_session_id, request.project_id
            ),
        )
    })?;

    if !is_default_session_title(&current.title) {
        return Ok(agent_session_dto(&current));
    }

    let updated = project_store::update_agent_session(
        &repo_root,
        &AgentSessionUpdateRecord {
            project_id: request.project_id,
            agent_session_id: request.agent_session_id,
            title: Some(generated_title),
            summary: None,
            selected: None,
        },
    )?;

    Ok(agent_session_dto(&updated))
}

fn generate_session_title<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    controls: Option<&RuntimeRunControlInputDto>,
    prompt: &str,
) -> CommandResult<String> {
    let title_controls = controls.map(title_generation_controls);
    let provider_config = resolve_owned_agent_provider_config(app, state, title_controls.as_ref())?;
    let provider = create_provider_adapter(provider_config)?;
    let provider_model_id = provider.model_id().to_owned();
    let turn = ProviderTurnRequest {
        system_prompt: SESSION_TITLE_SYSTEM_PROMPT.into(),
        messages: vec![ProviderMessage::User {
            content: build_session_title_prompt(prompt),
            attachments: Vec::new(),
        }],
        tools: Vec::new(),
        turn_index: 0,
        controls: title_generation_control_state(
            title_controls.as_ref(),
            provider_model_id.clone(),
        ),
    };

    let mut emit = |_event: ProviderStreamEvent| Ok(());
    let message = match provider.stream_turn(&turn, &mut emit)? {
        ProviderTurnOutcome::Complete { message, .. } => message,
        ProviderTurnOutcome::ToolCalls { .. } => {
            return Err(CommandError::retryable(
                "agent_session_title_provider_requested_tools",
                "Xero asked the selected model for a session name, but the model requested tools instead.",
            ));
        }
    };

    sanitize_provider_session_title(&message)
        .or_else(|| fallback_title_from_prompt(prompt))
        .ok_or_else(|| {
            CommandError::retryable(
                "agent_session_title_empty",
                "The selected model returned an empty session name.",
            )
        })
}

fn title_generation_controls(controls: &RuntimeRunControlInputDto) -> RuntimeRunControlInputDto {
    let mut title_controls = controls.clone();
    if title_controls.thinking_effort.is_some() {
        title_controls.thinking_effort = Some(ProviderModelThinkingEffortDto::Minimal);
    }
    title_controls.approval_mode = RuntimeRunApprovalModeDto::Suggest;
    title_controls.plan_mode_required = false;
    title_controls
}

fn title_generation_control_state(
    controls: Option<&RuntimeRunControlInputDto>,
    model_id: String,
) -> RuntimeRunControlStateDto {
    RuntimeRunControlStateDto {
        active: RuntimeRunActiveControlSnapshotDto {
            runtime_agent_id: controls
                .map(|controls| controls.runtime_agent_id)
                .unwrap_or(RuntimeAgentIdDto::Ask),
            agent_definition_id: controls.and_then(|controls| controls.agent_definition_id.clone()),
            agent_definition_version: None,
            provider_profile_id: controls.and_then(|controls| controls.provider_profile_id.clone()),
            model_id,
            thinking_effort: controls.and_then(|controls| controls.thinking_effort.clone()),
            approval_mode: RuntimeRunApprovalModeDto::Suggest,
            plan_mode_required: false,
            revision: 1,
            applied_at: now_timestamp(),
        },
        pending: None,
    }
}

fn build_session_title_prompt(prompt: &str) -> String {
    format!(
        "Create a sidebar title for this first user prompt:\n\n{}",
        truncate_prompt(prompt.trim(), MAX_PROMPT_CHARS)
    )
}

fn sanitize_provider_session_title(message: &str) -> Option<String> {
    let mut title = strip_markdown_fence(message);
    title = strip_label_prefix(&title);
    title = strip_wrapping_quotes(&title);
    title = title
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_owned();
    title = collapse_title_whitespace(&title);
    title = trim_trailing_title_punctuation(&title);
    title = truncate_title(&title, MAX_TITLE_CHARS);

    if is_usable_generated_title(&title) {
        Some(title)
    } else {
        None
    }
}

fn fallback_title_from_prompt(prompt: &str) -> Option<String> {
    let cleaned = collapse_title_whitespace(
        prompt
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or(""),
    );
    let cleaned = cleaned
        .trim_start_matches(|character: char| {
            matches!(character, '#' | '-' | '*' | '>' | '"' | '\'' | '`')
        })
        .trim();
    let words = cleaned
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    let title = truncate_title(&trim_trailing_title_punctuation(&words), MAX_TITLE_CHARS);

    if is_usable_generated_title(&title) {
        Some(title)
    } else {
        None
    }
}

fn is_default_session_title(title: &str) -> bool {
    title
        .trim()
        .eq_ignore_ascii_case(DEFAULT_AGENT_SESSION_TITLE)
}

fn is_usable_generated_title(title: &str) -> bool {
    let trimmed = title.trim();
    !trimmed.is_empty()
        && !is_default_session_title(trimmed)
        && !trimmed.eq_ignore_ascii_case("untitled")
        && !trimmed.eq_ignore_ascii_case("chat")
}

fn truncate_prompt(prompt: &str, max_chars: usize) -> String {
    if prompt.chars().count() <= max_chars {
        return prompt.to_owned();
    }

    let mut truncated = prompt.chars().take(max_chars).collect::<String>();
    truncated.push_str("\n\n[Prompt truncated for title generation.]");
    truncated
}

fn truncate_title(title: &str, max_chars: usize) -> String {
    let trimmed = title.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }

    let mut output = String::new();
    for word in trimmed.split_whitespace() {
        let next_len =
            output.chars().count() + if output.is_empty() { 0 } else { 1 } + word.chars().count();
        if next_len > max_chars {
            break;
        }
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(word);
    }

    if output.is_empty() {
        trimmed.chars().take(max_chars).collect()
    } else {
        output
    }
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
    for prefix in ["session title:", "title:", "name:", "session name:"] {
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

    for (left, right) in [('"', '"'), ('\'', '\''), ('`', '`')] {
        if trimmed.starts_with(left) && trimmed.ends_with(right) {
            return trimmed[1..trimmed.len() - 1].trim().to_owned();
        }
    }

    trimmed.to_owned()
}

fn collapse_title_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_trailing_title_punctuation(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(|character: char| {
            matches!(
                character,
                '.' | ',' | ':' | ';' | '!' | '?' | '-' | '_' | '"' | '\'' | '`'
            )
        })
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        fallback_title_from_prompt, sanitize_provider_session_title, title_generation_controls,
    };
    use crate::commands::{
        ProviderModelThinkingEffortDto, RuntimeAgentIdDto, RuntimeRunApprovalModeDto,
        RuntimeRunControlInputDto,
    };

    #[test]
    fn sanitizes_model_title_wrappers() {
        let title = "```text\nTitle: \"System Prompt Investigation.\"\n```";
        assert_eq!(
            sanitize_provider_session_title(title).as_deref(),
            Some("System Prompt Investigation")
        );
    }

    #[test]
    fn rejects_generic_titles_and_uses_prompt_fallback() {
        assert_eq!(sanitize_provider_session_title("New Chat"), None);
        assert_eq!(
            fallback_title_from_prompt("How does the system prompt get built for the Ask agent?")
                .as_deref(),
            Some("How does the system prompt get")
        );
    }

    #[test]
    fn lowers_thinking_only_for_thinking_capable_controls() {
        let controls = RuntimeRunControlInputDto {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            provider_profile_id: Some("openai-codex-default".into()),
            model_id: "gpt-5.4".into(),
            thinking_effort: Some(ProviderModelThinkingEffortDto::High),
            approval_mode: RuntimeRunApprovalModeDto::AutoEdit,
            plan_mode_required: true,
        };

        let title_controls = title_generation_controls(&controls);

        assert_eq!(
            title_controls.thinking_effort,
            Some(ProviderModelThinkingEffortDto::Minimal)
        );
        assert_eq!(
            title_controls.approval_mode,
            RuntimeRunApprovalModeDto::Suggest
        );
        assert!(!title_controls.plan_mode_required);
    }
}
