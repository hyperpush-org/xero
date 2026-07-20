use std::path::Path;

use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        agent_session::agent_session_dto, default_runtime_agent_id,
        remote_bridge::publish_agent_session_remote_state,
        runtime_support::resolve_owned_agent_provider_config,
        runtime_support::resolve_project_root, validate_non_empty, AgentSessionDto,
        AutoNameAgentSessionRequestDto, CommandError, CommandResult,
        ProviderModelThinkingEffortDto, RuntimeRunActiveControlSnapshotDto,
        RuntimeRunApprovalModeDto, RuntimeRunControlInputDto, RuntimeRunControlStateDto,
    },
    db::project_store::{self, AgentSessionUpdateRecord},
    runtime::{
        create_provider_adapter, ProviderMessage, ProviderStreamEvent, ProviderTurnOutcome,
        ProviderTurnRequest,
    },
    state::DesktopState,
};

const SESSION_TITLE_SYSTEM_PROMPT: &str = "You name developer-assistant chat sessions from the conversation so far. Return only one concise title. Use 2 to 6 words. No markdown, no quotes, no trailing punctuation, no generic labels like Main, New Chat, Session, or Conversation. Capture the user's current goal as it has evolved, not the assistant's behavior.";
const MAX_TITLE_CHARS: usize = 64;
const MAX_PROMPT_CHARS: usize = 4_000;
const MAX_TITLE_CONTEXT_CHARS: usize = 8_000;
const MAX_TITLE_CONTEXT_MESSAGES: usize = 12;
const MAX_TITLE_MESSAGE_CHARS: usize = 900;

#[tauri::command]
pub async fn auto_name_agent_session<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: AutoNameAgentSessionRequestDto,
) -> CommandResult<AgentSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.prompt, "prompt")?;

    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        auto_name_agent_session_blocking(app, state, request)
    })
    .await
    .map_err(|error| {
        CommandError::system_fault(
            "agent_session_title_task_failed",
            format!("Xero could not finish background session-title generation: {error}"),
        )
    })?
}

fn auto_name_agent_session_blocking<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: AutoNameAgentSessionRequestDto,
) -> CommandResult<AgentSessionDto> {
    let repo_root = resolve_project_root(&app, &state, &request.project_id)?;
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

    let original_title = existing.title.clone();
    let title_context = build_session_title_context(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
        &original_title,
        &request.prompt,
    );
    let generated_title = title_or_prompt_fallback(
        generate_session_title(
            &app,
            &state,
            request.controls.as_ref(),
            &title_context,
            &request.prompt,
        ),
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

    if current.title.as_str() != original_title.as_str()
        || titles_match(&current.title, &generated_title)
    {
        return Ok(agent_session_dto(&current));
    }

    let updated = project_store::update_agent_session(
        &repo_root,
        &AgentSessionUpdateRecord {
            project_id: request.project_id.clone(),
            agent_session_id: request.agent_session_id.clone(),
            title: Some(generated_title),
            summary: None,
            selected: None,
        },
    )?;
    publish_agent_session_remote_state(&app, &state, &request.project_id, &updated);

    Ok(agent_session_dto(&updated))
}

fn generate_session_title<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    controls: Option<&RuntimeRunControlInputDto>,
    title_context: &str,
    fallback_prompt: &str,
) -> CommandResult<String> {
    let title_controls = controls.map(title_generation_controls);
    let provider_config = resolve_owned_agent_provider_config(app, state, title_controls.as_ref())?;
    let provider = create_provider_adapter(provider_config)?;
    let provider_model_id = provider.model_id().to_owned();
    let turn_controls =
        title_generation_control_state(title_controls.as_ref(), provider_model_id.clone());
    let output_allowance = provider
        .resolve_turn_output_allowance(None, turn_controls.active.thinking_effort.as_ref())?;
    let turn = ProviderTurnRequest {
        system_prompt: SESSION_TITLE_SYSTEM_PROMPT.into(),
        messages: vec![ProviderMessage::User {
            content: build_session_title_prompt(title_context),
            attachments: Vec::new(),
        }],
        tools: Vec::new(),
        turn_index: 0,
        output_allowance,
        controls: turn_controls,
    };

    let mut emit = |_event: ProviderStreamEvent| Ok(());
    let cancellation = crate::runtime::AgentRunCancellationToken::default();
    let message = match provider.stream_turn(&turn, &cancellation, &mut emit)? {
        ProviderTurnOutcome::Complete { message, .. } => message,
        ProviderTurnOutcome::ToolCalls { .. } => {
            return Err(CommandError::retryable(
                "agent_session_title_provider_requested_tools",
                "Xero asked the selected model for a session name, but the model requested tools instead.",
            ));
        }
    };

    sanitize_provider_session_title(&message)
        .or_else(|| fallback_title_from_prompt(fallback_prompt))
        .ok_or_else(|| {
            CommandError::retryable(
                "agent_session_title_empty",
                "The selected model returned an empty session name.",
            )
        })
}

fn title_or_prompt_fallback(
    generated_title: CommandResult<String>,
    fallback_prompt: &str,
) -> CommandResult<String> {
    match generated_title {
        Ok(title) => Ok(title),
        Err(_) => fallback_title_from_prompt(fallback_prompt).ok_or_else(|| {
            CommandError::retryable(
                "agent_session_title_fallback_empty",
                "Xero could not derive a session name from the latest prompt.",
            )
        }),
    }
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
                .unwrap_or_else(default_runtime_agent_id),
            agent_definition_id: controls.and_then(|controls| controls.agent_definition_id.clone()),
            agent_definition_version: None,
            provider_profile_id: controls.and_then(|controls| controls.provider_profile_id.clone()),
            model_id,
            thinking_effort: controls.and_then(|controls| controls.thinking_effort.clone()),
            approval_mode: RuntimeRunApprovalModeDto::Suggest,
            plan_mode_required: false,
            auto_compact_enabled: false,
            revision: 1,
            applied_at: now_timestamp(),
        },
        pending: None,
    }
}

fn build_session_title_prompt(title_context: &str) -> String {
    format!(
        "Create a sidebar title for this developer-assistant conversation. Prefer the latest user goal when the topic has shifted. Do not preserve a generic current title such as Main, New Chat, Session, or Conversation.\n\n{}",
        truncate_text(title_context.trim(), MAX_TITLE_CONTEXT_CHARS, "[Conversation truncated for title generation.]")
    )
}

fn build_session_title_context(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    current_title: &str,
    latest_prompt: &str,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Current sidebar title: {}", current_title.trim()));

    let mut messages = recent_session_title_messages(repo_root, project_id, agent_session_id);
    let latest_prompt = latest_prompt.trim();
    if !latest_prompt.is_empty()
        && !messages.iter().rev().any(|message| {
            message.role == TitleContextRole::User && message.content == latest_prompt
        })
    {
        messages.push(TitleContextMessage {
            role: TitleContextRole::User,
            content: latest_prompt.to_owned(),
        });
    }

    if messages.is_empty() {
        lines.push(String::from("Latest user prompt:"));
        lines.push(truncate_text(
            latest_prompt,
            MAX_PROMPT_CHARS,
            "[Prompt truncated for title generation.]",
        ));
        return lines.join("\n");
    }

    lines.push(String::from("Recent conversation:"));
    let start = messages.len().saturating_sub(MAX_TITLE_CONTEXT_MESSAGES);
    for message in messages.iter().skip(start) {
        lines.push(format!(
            "{}: {}",
            message.role.label(),
            truncate_text(
                &message.content,
                MAX_TITLE_MESSAGE_CHARS,
                "[Message truncated for title generation.]",
            )
        ));
    }

    lines.join("\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TitleContextRole {
    User,
    Assistant,
}

impl TitleContextRole {
    fn label(self) -> &'static str {
        match self {
            TitleContextRole::User => "User",
            TitleContextRole::Assistant => "Assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TitleContextMessage {
    role: TitleContextRole,
    content: String,
}

fn recent_session_title_messages(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Vec<TitleContextMessage> {
    let Ok(snapshots) =
        project_store::load_agent_session_run_snapshots(repo_root, project_id, agent_session_id)
    else {
        return Vec::new();
    };

    let mut messages = Vec::new();
    for (snapshot, _) in snapshots {
        for message in snapshot.messages {
            let role = match message.role {
                project_store::AgentMessageRole::User => TitleContextRole::User,
                project_store::AgentMessageRole::Assistant => TitleContextRole::Assistant,
                project_store::AgentMessageRole::System
                | project_store::AgentMessageRole::Developer
                | project_store::AgentMessageRole::Tool => continue,
            };
            let content = collapse_title_whitespace(&message.content);
            if content.is_empty() {
                continue;
            }
            messages.push(TitleContextMessage { role, content });
        }
    }
    messages
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
    let title = truncate_title(&trim_trailing_title_punctuation(cleaned), MAX_TITLE_CHARS);

    if is_usable_generated_title(&title) {
        Some(title)
    } else {
        None
    }
}

fn is_usable_generated_title(title: &str) -> bool {
    let trimmed = title.trim();
    !trimmed.is_empty() && !is_generic_session_title(trimmed)
}

fn is_generic_session_title(title: &str) -> bool {
    let normalized = collapse_title_whitespace(
        &title
            .trim()
            .trim_matches(|character: char| matches!(character, '"' | '\'' | '`'))
            .to_ascii_lowercase(),
    );
    matches!(
        normalized.as_str(),
        "main"
            | "new chat"
            | "new session"
            | "untitled"
            | "untitled session"
            | "chat"
            | "session"
            | "conversation"
            | "developer conversation"
            | "developer assistant conversation"
    )
}

fn truncate_text(value: &str, max_chars: usize, marker: &str) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("\n\n");
    truncated.push_str(marker);
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

fn titles_match(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tauri::Manager;

    use super::*;
    use crate::{
        commands::RuntimeAgentIdDto,
        db,
        git::repository::CanonicalRepository,
        registry::RegistryProjectRecord,
        runtime::AgentProviderConfig,
    };

    #[test]
    fn rejects_generic_titles_and_uses_prompt_fallback() {
        assert_eq!(sanitize_provider_session_title("New Chat"), None);
        assert_eq!(sanitize_provider_session_title("Main"), None);
        assert_eq!(sanitize_provider_session_title("Conversation"), None);
        assert_eq!(
            fallback_title_from_prompt("How does the system prompt get built for the Ask agent?")
                .as_deref(),
            Some("How does the system prompt get built for the Ask agent")
        );
        assert_eq!(
            fallback_title_from_prompt("how do I write fizz buzz in c#?").as_deref(),
            Some("how do I write fizz buzz in c#")
        );
    }

    #[test]
    fn prompt_fallback_survives_title_generation_errors() {
        let title = title_or_prompt_fallback(
            Err(CommandError::retryable(
                "agent_session_title_provider_unavailable",
                "Provider unavailable.",
            )),
            "What is this project about?",
        )
        .expect("prompt fallback should create a usable title");

        assert_eq!(title, "What is this project about");
    }

    #[test]
    fn title_prompt_targets_progressing_conversation() {
        let context = build_session_title_context(
            std::path::Path::new("/definitely/missing"),
            "project-1",
            "agent-session-main",
            "System Prompt Investigation",
            "Actually focus the title on the runtime reconnect bug.",
        );
        let prompt = build_session_title_prompt(&context);

        assert!(prompt.contains("conversation"));
        assert!(prompt.contains("Current sidebar title: System Prompt Investigation"));
        assert!(prompt.contains("Actually focus the title on the runtime reconnect bug."));
        assert!(!prompt.contains("first user prompt"));
        assert!(prompt.contains("Do not preserve a generic current title"));
    }

    #[test]
    fn lowers_thinking_only_for_thinking_capable_controls() {
        let controls = RuntimeRunControlInputDto {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            provider_profile_id: Some("openai-codex-default".into()),
            model_id: "gpt-5.4".into(),
            thinking_effort: Some(ProviderModelThinkingEffortDto::High),
            approval_mode: RuntimeRunApprovalModeDto::AutoEdit,
            plan_mode_required: true,
            auto_compact_enabled: true,
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

    #[test]
    fn session_title_helper_fixture_covers_sanitization_truncation_controls_and_context_roles() {
        for (raw, expected) in [
            ("```text\nTitle: Rust Build Storage\n```", Some("Rust Build Storage")),
            ("'Workflow Recovery'", Some("Workflow Recovery")),
            ("`Agent Harness Tests`", Some("Agent Harness Tests")),
            ("Name: Provider Adapter Audit!", Some("Provider Adapter Audit")),
            ("\nfirst useful line\nignored line", Some("first useful line")),
            ("Untitled Session", None),
            ("   ", None),
        ] {
            assert_eq!(sanitize_provider_session_title(raw).as_deref(), expected);
        }
        assert_eq!(fallback_title_from_prompt("# Ship the Rust tests!!!").as_deref(), Some("Ship the Rust tests"));
        assert_eq!(fallback_title_from_prompt("new session"), None);
        assert!(titles_match("  Rust Tests ", "rust tests"));
        assert!(!titles_match("Rust", "Stages"));
        assert_eq!(TitleContextRole::User.label(), "User");
        assert_eq!(TitleContextRole::Assistant.label(), "Assistant");
        assert!(truncate_text(&"x".repeat(MAX_PROMPT_CHARS + 1), MAX_PROMPT_CHARS, "cut")
            .ends_with("cut"));
        assert_eq!(truncate_text("short", 10, "cut"), "short");
        assert!(truncate_title(&format!("{} tail", "x".repeat(MAX_TITLE_CHARS + 1)), MAX_TITLE_CHARS)
            .chars()
            .count()
            <= MAX_TITLE_CHARS);
        assert_eq!(strip_markdown_fence("plain"), "plain");
        assert_eq!(strip_wrapping_quotes("\"multi\nline\""), "\"multi\nline\"");
        assert_eq!(strip_label_prefix("plain title"), "plain title");

        let controls = title_generation_control_state(None, "fixture-model".into());
        assert_eq!(controls.active.model_id, "fixture-model");
        assert_eq!(controls.active.approval_mode, RuntimeRunApprovalModeDto::Suggest);
        assert!(!controls.active.auto_compact_enabled);
        assert_eq!(
            title_or_prompt_fallback(Ok("Direct title".into()), "fallback").unwrap(),
            "Direct title"
        );
        assert_eq!(
            title_or_prompt_fallback(
                Err(CommandError::retryable("fixture", "failed")),
                "new chat",
            )
            .expect_err("generic fallback is unusable")
            .code,
            "agent_session_title_fallback_empty"
        );
    }

    #[test]
    fn auto_name_session_fixture_uses_recent_transcript_and_fake_provider_idempotently() {
        let fixture = tempfile::tempdir().expect("title fixture");
        let repo_root = fixture.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create title repository");
        let repo_root = fs::canonicalize(repo_root).expect("canonical title repository");
        let project_id = "project-session-title";
        let registry_path = fixture.path().join("app-data/global.db");
        let state = DesktopState::default()
            .with_global_db_path_override(registry_path.clone())
            .with_owned_agent_provider_config_override(AgentProviderConfig::Fake);
        let app = crate::configure_builder_with_state(tauri::test::mock_builder(), state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build title fixture app");
        db::configure_project_database_paths(&registry_path);
        let repository = CanonicalRepository {
            project_id: project_id.into(),
            repository_id: "repository-session-title".into(),
            root_path: repo_root.clone(),
            root_path_string: repo_root.to_string_lossy().into_owned(),
            common_git_dir: repo_root.join(".git"),
            display_name: "Session title fixture".into(),
            branch_name: Some("main".into()),
            head_sha: Some("abc123".into()),
            branch: None,
            last_commit: None,
            status_entries: Vec::new(),
            has_staged_changes: false,
            has_unstaged_changes: false,
            has_untracked_changes: false,
            additions: 0,
            deletions: 0,
        };
        db::import_project(
            &repository,
            app.state::<DesktopState>().import_failpoints(),
        )
        .expect("import title fixture project");
        crate::registry::replace_projects(
            &registry_path,
            vec![RegistryProjectRecord {
                project_id: project_id.into(),
                repository_id: repository.repository_id,
                root_path: repo_root.to_string_lossy().into_owned(),
                is_git_repo: true,
            }],
        )
        .expect("seed title fixture registry");

        let run_id = "run-session-title";
        project_store::insert_agent_run(
            &repo_root,
            &project_store::NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                project_id: project_id.into(),
                agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: run_id.into(),
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
                prompt: "Audit the workflow runtime.".into(),
                system_prompt: "fixture".into(),
                now: "2026-07-18T00:00:00Z".into(),
            },
        )
        .expect("insert title fixture run");
        for (role, content) in [
            (project_store::AgentMessageRole::System, "hidden system"),
            (project_store::AgentMessageRole::User, "Audit the workflow runtime."),
            (project_store::AgentMessageRole::Assistant, "I found a scheduler issue."),
            (project_store::AgentMessageRole::Tool, "hidden tool output"),
        ] {
            project_store::append_agent_message(
                &repo_root,
                &project_store::NewAgentMessageRecord {
                    project_id: project_id.into(),
                    run_id: run_id.into(),
                    role,
                    content: content.into(),
                    provider_metadata_json: None,
                    created_at: "2026-07-18T00:00:01Z".into(),
                    attachments: Vec::new(),
                },
            )
            .expect("append title context message");
        }
        let context = build_session_title_context(
            &repo_root,
            project_id,
            project_store::DEFAULT_AGENT_SESSION_ID,
            project_store::DEFAULT_AGENT_SESSION_TITLE,
            "Now expand Stage coverage.",
        );
        assert!(context.contains("User: Audit the workflow runtime."));
        assert!(context.contains("Assistant: I found a scheduler issue."));
        assert!(context.contains("User: Now expand Stage coverage."));
        assert!(!context.contains("hidden system"));
        assert!(!context.contains("hidden tool output"));

        let request = AutoNameAgentSessionRequestDto {
            project_id: project_id.into(),
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
            prompt: "Now expand Stage coverage.".into(),
            controls: None,
        };
        let named = auto_name_agent_session_blocking(
            app.handle().clone(),
            app.state::<DesktopState>().inner().clone(),
            request.clone(),
        )
        .expect("auto-name fixture session");
        assert!(!is_generic_session_title(&named.title));
        let replay = auto_name_agent_session_blocking(
            app.handle().clone(),
            app.state::<DesktopState>().inner().clone(),
            request,
        )
        .expect("replay auto-name fixture session");
        assert_eq!(replay.title, named.title);

        let missing = auto_name_agent_session_blocking(
            app.handle().clone(),
            app.state::<DesktopState>().inner().clone(),
            AutoNameAgentSessionRequestDto {
                project_id: project_id.into(),
                agent_session_id: "missing".into(),
                prompt: "Name this".into(),
                controls: None,
            },
        )
        .expect_err("missing title session is typed");
        assert_eq!(missing.code, "agent_session_missing");
    }
}
