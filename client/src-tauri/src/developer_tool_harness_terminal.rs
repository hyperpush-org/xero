use std::{
    env, fs,
    io::{self, Read},
    path::PathBuf,
    process::Command,
    time::Duration,
};

use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use serde_json::{json, Value as JsonValue};

use crate::{
    commands::{
        developer_tool_harness::{
            developer_tool_catalog_service, developer_tool_sequence_delete_service,
            developer_tool_sequence_list_service, developer_tool_sequence_upsert_service,
            developer_tool_terminal_dry_run, developer_tool_terminal_host_context,
            developer_tool_terminal_synthetic_run, ensure_terminal_tool_available,
            DeveloperToolCatalogServiceOptions, DeveloperToolHarnessHostKind,
        },
        CommandError, DeveloperToolDryRunRequestDto, DeveloperToolHarnessCallDto,
        DeveloperToolHarnessRunOptionsDto, DeveloperToolSequenceDeleteRequestDto,
        DeveloperToolSequenceRecordDto, DeveloperToolSequenceUpsertRequestDto,
        DeveloperToolSyntheticRunRequestDto,
    },
    developer_tool_harness_tui::{
        default_input_from_schema, pretty_json, render_harness_tui, HarnessTuiAction,
        HarnessTuiController, HarnessTuiEvent, HarnessTuiInputMode, HarnessTuiStatusKind,
    },
};

const APP_DATA_DIRECTORY_NAME: &str = "dev.sn0w.xero";
const FAKE_PROVIDER_ID: &str = "fake_provider";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessOutputMode {
    Text,
    Json,
}

#[derive(Debug, Clone)]
pub struct HarnessTerminalResponse {
    pub output_mode: HarnessOutputMode,
    pub text: String,
    pub json: JsonValue,
    emit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessTerminalError {
    pub code: String,
    pub message: String,
    pub exit_code: i32,
}

impl HarnessTerminalError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: "tool_harness_usage".into(),
            message: message.into(),
            exit_code: 2,
        }
    }

    fn user_fixable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            exit_code: 1,
        }
    }

    fn system_fault(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            exit_code: 1,
        }
    }
}

impl From<CommandError> for HarnessTerminalError {
    fn from(error: CommandError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            exit_code: 1,
        }
    }
}

#[derive(Debug, Clone)]
struct HarnessGlobalOptions {
    output_mode: HarnessOutputMode,
    app_data_dir: PathBuf,
}

pub fn run_from_env() -> i32 {
    let args = env::args().collect::<Vec<_>>();
    let output_mode = requested_output_mode(&args);
    match run_with_args(args) {
        Ok(response) => {
            if response.emit {
                emit_response(&response);
            }
            0
        }
        Err(error) => {
            emit_error(&error, output_mode);
            error.exit_code
        }
    }
}

pub fn run_with_args<I, S>(args: I) -> Result<HarnessTerminalResponse, HarnessTerminalError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let raw_args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let (globals, command_args) = parse_global_options(raw_args)?;
    dispatch(globals, command_args)
}

fn dispatch(
    globals: HarnessGlobalOptions,
    args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if args.is_empty()
        || args
            .first()
            .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(response(&globals, root_help(), root_help_json()));
    }

    match args.first().map(String::as_str) {
        Some("catalog") => command_catalog(globals, args[1..].to_vec()),
        Some("fixture") => command_fixture(globals, args[1..].to_vec()),
        Some("dry-run") => command_dry_run(globals, args[1..].to_vec()),
        Some("run") => command_run(globals, args[1..].to_vec()),
        Some("sequence") | Some("sequences") => command_sequence(globals, args[1..].to_vec()),
        Some("tui") => command_tui(globals, args[1..].to_vec()),
        Some("model-run") => command_model_run(globals, args[1..].to_vec()),
        Some(other) => Err(HarnessTerminalError::usage(format!(
            "Unknown tool-harness command `{other}`. Run `tool-harness --help`."
        ))),
        None => Ok(response(&globals, root_help(), root_help_json())),
    }
}

fn command_catalog(
    globals: HarnessGlobalOptions,
    mut args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness catalog [--group GROUP] [--search TEXT] [--include-skill-tool] [--json]",
            json!({ "command": "catalog" }),
        ));
    }
    let group = take_option(&mut args, "--group")?;
    let search = take_option(&mut args, "--search")?;
    let include_skill_tool = take_bool_flag(&mut args, "--include-skill-tool");
    reject_unknown_options(&args)?;

    let mut catalog = developer_tool_catalog_service(DeveloperToolCatalogServiceOptions {
        skill_tool_enabled: include_skill_tool,
        host_kind: DeveloperToolHarnessHostKind::Terminal,
    })?;
    catalog.entries.retain(|entry| {
        group
            .as_ref()
            .map(|group| entry.group == *group)
            .unwrap_or(true)
    });
    if let Some(search) = search.as_ref().map(|value| value.to_lowercase()) {
        catalog.entries.retain(|entry| {
            entry.tool_name.to_lowercase().contains(&search)
                || entry.description.to_lowercase().contains(&search)
                || entry
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&search))
        });
    }

    let text = if catalog.entries.is_empty() {
        "No matching tools.".into()
    } else {
        catalog
            .entries
            .iter()
            .map(|entry| {
                let availability = if entry.runtime_available {
                    "available".to_string()
                } else {
                    format!(
                        "unavailable: {}",
                        entry
                            .runtime_unavailable_reason
                            .as_deref()
                            .unwrap_or("not available in this runtime")
                    )
                };
                format!(
                    "{:<28} {:<18} {:<14} {}",
                    entry.tool_name, entry.group, entry.effect_class, availability
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    Ok(response(
        &globals,
        text,
        json!({
            "kind": "catalog",
            "catalog": catalog,
        }),
    ))
}

fn command_fixture(
    globals: HarnessGlobalOptions,
    mut args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness fixture [prepare] [--json]",
            json!({ "command": "fixture" }),
        ));
    }
    if args.first().is_some_and(|arg| arg == "prepare") {
        args.remove(0);
    }
    reject_unknown_options(&args)?;

    let context = developer_tool_terminal_host_context(&globals.app_data_dir)?;
    Ok(response(
        &globals,
        format!(
            "Prepared harness fixture `{}` at {}",
            context.fixture_project.display_name, context.fixture_project.root_path
        ),
        json!({
            "kind": "fixture",
            "project": context.fixture_project,
            "globalDbPath": context.global_db_path,
            "capabilities": context.capabilities,
        }),
    ))
}

fn command_dry_run(
    globals: HarnessGlobalOptions,
    mut args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness dry-run TOOL [--input-json JSON | --input-file PATH | --stdin] [--operator-approved] [--json]",
            json!({ "command": "dry-run" }),
        ));
    }
    let operator_approved = take_bool_flag(&mut args, "--operator-approved");
    let tool_name = take_tool_name(&mut args, "dry-run")?;
    let input = take_input_json_or_default(&mut args, &tool_name)?;
    reject_unknown_options(&args)?;
    ensure_terminal_tool_available(&tool_name)?;
    let context = developer_tool_terminal_host_context(&globals.app_data_dir)?;
    let result = developer_tool_terminal_dry_run(
        &context,
        DeveloperToolDryRunRequestDto {
            project_id: context.project_id.clone(),
            tool_name: tool_name.clone(),
            input,
            tool_call_id: None,
            operator_approved: Some(operator_approved),
        },
    )?;
    let text = format!(
        "Dry-run `{}`: policy={} sandbox={}",
        result.tool_name,
        result.policy_decision.action,
        if result.sandbox_denied {
            "deny"
        } else {
            "allow"
        }
    );
    Ok(response(
        &globals,
        text,
        json!({ "kind": "dryRun", "result": result }),
    ))
}

fn command_run(
    globals: HarnessGlobalOptions,
    mut args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness run TOOL [--input-json JSON | --input-file PATH | --stdin] [--approve-writes] [--operator-approve-all] [--no-stop-on-failure] [--json]",
            json!({ "command": "run" }),
        ));
    }
    let approve_writes = take_bool_flag(&mut args, "--approve-writes");
    let operator_approve_all = take_bool_flag(&mut args, "--operator-approve-all");
    let stop_on_failure = !take_bool_flag(&mut args, "--no-stop-on-failure");
    let tool_name = take_tool_name(&mut args, "run")?;
    let input = take_input_json_or_default(&mut args, &tool_name)?;
    reject_unknown_options(&args)?;
    ensure_terminal_tool_available(&tool_name)?;
    let context = developer_tool_terminal_host_context(&globals.app_data_dir)?;
    let result = developer_tool_terminal_synthetic_run(
        &context,
        DeveloperToolSyntheticRunRequestDto {
            project_id: context.project_id.clone(),
            agent_session_id: None,
            calls: vec![DeveloperToolHarnessCallDto {
                tool_name: tool_name.clone(),
                input,
                tool_call_id: None,
            }],
            options: Some(DeveloperToolHarnessRunOptionsDto {
                stop_on_failure: Some(stop_on_failure),
                approve_writes: Some(approve_writes),
                operator_approve_all: Some(operator_approve_all),
            }),
        },
    )?;
    let text = format!(
        "Run {} finished: {} result(s), had_failure={}, stopped_early={}",
        result.run_id,
        result.results.len(),
        result.had_failure,
        result.stopped_early
    );
    Ok(response(
        &globals,
        text,
        json!({ "kind": "run", "result": result }),
    ))
}

fn command_sequence(
    globals: HarnessGlobalOptions,
    args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    match args.first().map(String::as_str) {
        Some("list") => command_sequence_list(globals, args[1..].to_vec()),
        Some("save") => command_sequence_save(globals, args[1..].to_vec()),
        Some("delete") => command_sequence_delete(globals, args[1..].to_vec()),
        Some("run") => command_sequence_run(globals, args[1..].to_vec()),
        Some("export") => command_sequence_export(globals, args[1..].to_vec()),
        Some(other) => Err(HarnessTerminalError::usage(format!(
            "Unknown sequence command `{other}`. Use list, save, delete, run, or export."
        ))),
        None => Err(HarnessTerminalError::usage(
            "Missing sequence command. Use list, save, delete, run, or export.",
        )),
    }
}

fn command_sequence_list(
    globals: HarnessGlobalOptions,
    args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness sequence list [--json]",
            json!({ "command": "sequence list" }),
        ));
    }
    reject_unknown_options(&args)?;
    let context = developer_tool_terminal_host_context(&globals.app_data_dir)?;
    let list = developer_tool_sequence_list_service(&context.global_db_path)?;
    let text = if list.sequences.is_empty() {
        "No saved sequences.".into()
    } else {
        list.sequences
            .iter()
            .map(|sequence| {
                format!(
                    "{}\t{}\t{} call(s)\t{}",
                    sequence.id,
                    sequence.name,
                    sequence.calls.len(),
                    sequence.updated_at
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "sequenceList", "sequences": list.sequences }),
    ))
}

fn command_sequence_save(
    globals: HarnessGlobalOptions,
    mut args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness sequence save --name NAME (--calls-json JSON | --input-file PATH | --stdin) [--approve-writes] [--operator-approve-all] [--no-stop-on-failure] [--json]",
            json!({ "command": "sequence save" }),
        ));
    }
    let id = take_option(&mut args, "--id")?;
    let name = take_option(&mut args, "--name")?
        .or_else(|| (!args.is_empty()).then(|| args.remove(0)))
        .ok_or_else(|| HarnessTerminalError::usage("Missing sequence name."))?;
    let approve_writes = take_bool_flag(&mut args, "--approve-writes");
    let operator_approve_all = take_bool_flag(&mut args, "--operator-approve-all");
    let stop_on_failure = !take_bool_flag(&mut args, "--no-stop-on-failure");
    let calls = take_sequence_calls(&mut args)?;
    reject_unknown_options(&args)?;
    if calls.is_empty() {
        return Err(HarnessTerminalError::user_fixable(
            "developer_tool_sequence_no_calls",
            "Sequences must contain at least one tool call.",
        ));
    }
    for call in &calls {
        ensure_terminal_tool_available(&call.tool_name)?;
    }
    let context = developer_tool_terminal_host_context(&globals.app_data_dir)?;
    let record = developer_tool_sequence_upsert_service(
        &context.global_db_path,
        DeveloperToolSequenceUpsertRequestDto {
            id,
            name,
            calls,
            options: Some(DeveloperToolHarnessRunOptionsDto {
                stop_on_failure: Some(stop_on_failure),
                approve_writes: Some(approve_writes),
                operator_approve_all: Some(operator_approve_all),
            }),
        },
    )?;
    Ok(response(
        &globals,
        format!(
            "Saved sequence `{}` ({} call(s)).",
            record.name,
            record.calls.len()
        ),
        json!({ "kind": "sequenceSave", "sequence": record }),
    ))
}

fn command_sequence_delete(
    globals: HarnessGlobalOptions,
    mut args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness sequence delete ID_OR_NAME [--json]",
            json!({ "command": "sequence delete" }),
        ));
    }
    let selector = take_sequence_selector(&mut args)?;
    reject_unknown_options(&args)?;
    let context = developer_tool_terminal_host_context(&globals.app_data_dir)?;
    let records = developer_tool_sequence_list_service(&context.global_db_path)?.sequences;
    let sequence = find_sequence(&records, &selector)?;
    let list = developer_tool_sequence_delete_service(
        &context.global_db_path,
        DeveloperToolSequenceDeleteRequestDto { id: sequence.id },
    )?;
    Ok(response(
        &globals,
        format!("Deleted sequence `{}`.", selector),
        json!({ "kind": "sequenceDelete", "sequences": list.sequences }),
    ))
}

fn command_sequence_run(
    globals: HarnessGlobalOptions,
    mut args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness sequence run ID_OR_NAME [--json]",
            json!({ "command": "sequence run" }),
        ));
    }
    let selector = take_sequence_selector(&mut args)?;
    reject_unknown_options(&args)?;
    let context = developer_tool_terminal_host_context(&globals.app_data_dir)?;
    let records = developer_tool_sequence_list_service(&context.global_db_path)?.sequences;
    let sequence = find_sequence(&records, &selector)?;
    for call in &sequence.calls {
        ensure_terminal_tool_available(&call.tool_name)?;
    }
    let result = developer_tool_terminal_synthetic_run(
        &context,
        DeveloperToolSyntheticRunRequestDto {
            project_id: context.project_id.clone(),
            agent_session_id: None,
            calls: sequence.calls.clone(),
            options: sequence.options.clone(),
        },
    )?;
    Ok(response(
        &globals,
        format!(
            "Sequence `{}` replayed as run {}. had_failure={}",
            sequence.name, result.run_id, result.had_failure
        ),
        json!({ "kind": "sequenceRun", "sequence": sequence, "result": result }),
    ))
}

fn command_sequence_export(
    globals: HarnessGlobalOptions,
    mut args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness sequence export ID_OR_NAME [--json]",
            json!({ "command": "sequence export" }),
        ));
    }
    let selector = take_sequence_selector(&mut args)?;
    reject_unknown_options(&args)?;
    let context = developer_tool_terminal_host_context(&globals.app_data_dir)?;
    let records = developer_tool_sequence_list_service(&context.global_db_path)?.sequences;
    let sequence = find_sequence(&records, &selector)?;
    Ok(response(
        &globals,
        pretty_json(&json!(sequence)),
        json!({ "kind": "sequenceExport", "sequence": sequence }),
    ))
}

fn command_tui(
    globals: HarnessGlobalOptions,
    args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness tui",
            json!({ "command": "tui" }),
        ));
    }
    reject_unknown_options(&args)?;
    run_tui(globals.app_data_dir)?;
    Ok(HarnessTerminalResponse {
        output_mode: globals.output_mode,
        text: String::new(),
        json: json!({ "kind": "tui", "status": "closed" }),
        emit: false,
    })
}

fn command_model_run(
    globals: HarnessGlobalOptions,
    mut args: Vec<String>,
) -> Result<HarnessTerminalResponse, HarnessTerminalError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: tool-harness model-run TOOL --prompt TEXT --provider ID --model ID [--allow-fake-provider-fixture] [--json]",
            json!({ "command": "model-run" }),
        ));
    }
    let prompt = take_option(&mut args, "--prompt")?;
    let provider = take_option(&mut args, "--provider")?.ok_or_else(|| {
        HarnessTerminalError::usage(
            "Missing --provider. Terminal model-run requires explicit provider intent.",
        )
    })?;
    let model = take_option(&mut args, "--model")?.ok_or_else(|| {
        HarnessTerminalError::usage(
            "Missing --model. Terminal model-run requires explicit model intent.",
        )
    })?;
    let allow_fake_provider_fixture = take_bool_flag(&mut args, "--allow-fake-provider-fixture");
    let tool_name = take_tool_name(&mut args, "model-run")?;
    reject_unknown_options(&args)?;
    if provider == FAKE_PROVIDER_ID && !allow_fake_provider_fixture {
        return Err(HarnessTerminalError::user_fixable(
            "developer_tool_model_fake_provider_requires_flag",
            "`--provider fake_provider` is fixture-only. Add `--allow-fake-provider-fixture` to make that intent explicit.",
        ));
    }
    if provider != FAKE_PROVIDER_ID && allow_fake_provider_fixture {
        return Err(HarnessTerminalError::usage(
            "`--allow-fake-provider-fixture` can only be used with `--provider fake_provider`.",
        ));
    }
    ensure_terminal_tool_available(&tool_name)?;
    let prompt = prompt
        .or_else(|| (!args.is_empty()).then(|| args.join(" ")))
        .ok_or_else(|| HarnessTerminalError::usage("Missing --prompt for model-run."))?;
    let context = developer_tool_terminal_host_context(&globals.app_data_dir)?;
    let model_prompt = format!(
        "Tool harness (terminal mode A): exercise the `{}` tool. {}",
        tool_name, prompt
    );
    let cli_args = vec![
        "xero".to_string(),
        "--state-dir".into(),
        globals.app_data_dir.to_string_lossy().into_owned(),
        "agent".into(),
        "exec".into(),
        "--project-id".into(),
        context.project_id,
        "--provider".into(),
        provider,
        "--model".into(),
        model,
        "--prompt".into(),
        model_prompt,
    ];
    let cli_response = xero_cli::run_with_args(cli_args).map_err(|error| HarnessTerminalError {
        code: error.code,
        message: error.message,
        exit_code: error.exit_code,
    })?;
    Ok(response(
        &globals,
        cli_response.text,
        json!({ "kind": "modelRun", "agent": cli_response.json }),
    ))
}

fn run_tui(app_data_dir: PathBuf) -> Result<(), HarnessTerminalError> {
    let context = developer_tool_terminal_host_context(&app_data_dir)?;
    let catalog = developer_tool_catalog_service(DeveloperToolCatalogServiceOptions {
        skill_tool_enabled: false,
        host_kind: DeveloperToolHarnessHostKind::Terminal,
    })?;
    let sequences = developer_tool_sequence_list_service(&context.global_db_path)?.sequences;
    let mut controller = HarnessTuiController::new(catalog, sequences);

    enable_raw_mode().map_err(io_error("developer_tool_tui_raw_mode_failed"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(io_error("developer_tool_tui_enter_screen_failed"))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(io_error("developer_tool_tui_terminal_failed"))?;

    let result = run_tui_event_loop(&mut terminal, &context, &mut controller);

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
    result
}

fn run_tui_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    context: &crate::commands::developer_tool_harness::DeveloperToolTerminalHostContext,
    controller: &mut HarnessTuiController,
) -> Result<(), HarnessTerminalError> {
    loop {
        terminal
            .draw(|frame| render_harness_tui(frame, &controller.state))
            .map_err(io_error("developer_tool_tui_render_failed"))?;
        if !event::poll(Duration::from_millis(200))
            .map_err(io_error("developer_tool_tui_event_failed"))?
        {
            continue;
        }
        let CrosstermEvent::Key(key) =
            event::read().map_err(io_error("developer_tool_tui_event_failed"))?
        else {
            continue;
        };
        let Some(tui_event) = key_to_tui_event(key, controller.state.input_mode) else {
            continue;
        };
        let action = controller.apply(tui_event);
        if handle_tui_action(action, context, controller)? {
            break Ok(());
        }
    }
}

fn key_to_tui_event(key: KeyEvent, mode: HarnessTuiInputMode) -> Option<HarnessTuiEvent> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(HarnessTuiEvent::Quit);
    }
    if mode != HarnessTuiInputMode::Normal {
        return match key.code {
            KeyCode::Esc => Some(HarnessTuiEvent::CancelInput),
            KeyCode::Enter => Some(HarnessTuiEvent::CommitInput),
            KeyCode::Backspace => Some(HarnessTuiEvent::Backspace),
            KeyCode::Char(ch) => Some(HarnessTuiEvent::InputChar(ch)),
            _ => None,
        };
    }
    match key.code {
        KeyCode::Char('q') => Some(HarnessTuiEvent::Quit),
        KeyCode::Char('?') => Some(HarnessTuiEvent::ToggleHelp),
        KeyCode::Esc => Some(HarnessTuiEvent::ToggleHelp),
        KeyCode::Up | KeyCode::Char('k') => Some(HarnessTuiEvent::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(HarnessTuiEvent::Down),
        KeyCode::PageUp => Some(HarnessTuiEvent::PageUp),
        KeyCode::PageDown => Some(HarnessTuiEvent::PageDown),
        KeyCode::Char('/') => Some(HarnessTuiEvent::SearchStart),
        KeyCode::Char('g') => Some(HarnessTuiEvent::CycleGroup),
        KeyCode::Char('c') => Some(HarnessTuiEvent::ClearFilters),
        KeyCode::Char('a') => Some(HarnessTuiEvent::ToggleApproveWrites),
        KeyCode::Char('o') => Some(HarnessTuiEvent::ToggleOperatorApproval),
        KeyCode::Char('e') => Some(HarnessTuiEvent::EditInput),
        KeyCode::Char('d') => Some(HarnessTuiEvent::DryRun),
        KeyCode::Char('r') => Some(HarnessTuiEvent::Run),
        KeyCode::Char('n') => Some(HarnessTuiEvent::AddSelectedToSequence),
        KeyCode::Char('s') => Some(HarnessTuiEvent::PromptSequenceName),
        KeyCode::Char('[') => Some(HarnessTuiEvent::SelectSequencePrev),
        KeyCode::Char(']') => Some(HarnessTuiEvent::SelectSequenceNext),
        KeyCode::Char('p') => Some(HarnessTuiEvent::ReplaySequence),
        KeyCode::Char('x') => Some(HarnessTuiEvent::DeleteSequence),
        _ => None,
    }
}

fn handle_tui_action(
    action: HarnessTuiAction,
    context: &crate::commands::developer_tool_harness::DeveloperToolTerminalHostContext,
    controller: &mut HarnessTuiController,
) -> Result<bool, HarnessTerminalError> {
    match action {
        HarnessTuiAction::None => Ok(false),
        HarnessTuiAction::Quit => Ok(true),
        HarnessTuiAction::EditInput => {
            match edit_json_in_external_editor(&controller.state.input_json) {
                Ok(input) => {
                    controller.apply(HarnessTuiEvent::SetInputJson(input));
                }
                Err(error) => {
                    controller.apply(HarnessTuiEvent::SetError(error.message));
                }
            }
            Ok(false)
        }
        HarnessTuiAction::DryRun => {
            let Some(entry) = controller.state.selected_entry().cloned() else {
                return Ok(false);
            };
            let input_json = controller.state.input_json.clone();
            let input = parse_json_for_tui(&input_json, controller)?;
            let result = developer_tool_terminal_dry_run(
                context,
                DeveloperToolDryRunRequestDto {
                    project_id: context.project_id.clone(),
                    tool_name: entry.tool_name,
                    input,
                    tool_call_id: None,
                    operator_approved: Some(controller.state.operator_approve_all),
                },
            );
            match result {
                Ok(result) => {
                    controller.apply(HarnessTuiEvent::SetDryRunResult(Box::new(result)));
                }
                Err(error) => {
                    controller.apply(HarnessTuiEvent::SetError(error.message));
                }
            }
            Ok(false)
        }
        HarnessTuiAction::Run => {
            let Some(entry) = controller.state.selected_entry().cloned() else {
                return Ok(false);
            };
            let input_json = controller.state.input_json.clone();
            let input = parse_json_for_tui(&input_json, controller)?;
            let result = developer_tool_terminal_synthetic_run(
                context,
                DeveloperToolSyntheticRunRequestDto {
                    project_id: context.project_id.clone(),
                    agent_session_id: None,
                    calls: vec![DeveloperToolHarnessCallDto {
                        tool_name: entry.tool_name,
                        input,
                        tool_call_id: None,
                    }],
                    options: Some(controller.state.current_run_options()),
                },
            );
            match result {
                Ok(result) => {
                    controller.apply(HarnessTuiEvent::SetRunResult(result));
                }
                Err(error) => {
                    controller.apply(HarnessTuiEvent::SetError(error.message));
                }
            }
            Ok(false)
        }
        HarnessTuiAction::SaveSequence(name) => {
            let record = developer_tool_sequence_upsert_service(
                &context.global_db_path,
                DeveloperToolSequenceUpsertRequestDto {
                    id: None,
                    name: name.clone(),
                    calls: controller.state.current_sequence.clone(),
                    options: Some(controller.state.current_run_options()),
                },
            )?;
            let sequences =
                developer_tool_sequence_list_service(&context.global_db_path)?.sequences;
            controller.apply(HarnessTuiEvent::SetSequences(sequences));
            controller.state.current_sequence.clear();
            controller.apply(HarnessTuiEvent::SetStatus(
                HarnessTuiStatusKind::Success,
                format!("Saved sequence `{}`.", record.name),
            ));
            Ok(false)
        }
        HarnessTuiAction::ReplaySequence => {
            let Some(record) = controller.state.selected_sequence().cloned() else {
                return Ok(false);
            };
            let result = developer_tool_terminal_synthetic_run(
                context,
                DeveloperToolSyntheticRunRequestDto {
                    project_id: context.project_id.clone(),
                    agent_session_id: None,
                    calls: record.calls,
                    options: record.options,
                },
            );
            match result {
                Ok(result) => {
                    controller.apply(HarnessTuiEvent::SetRunResult(result));
                }
                Err(error) => {
                    controller.apply(HarnessTuiEvent::SetError(error.message));
                }
            }
            Ok(false)
        }
        HarnessTuiAction::DeleteSequence => {
            let Some(record) = controller.state.selected_sequence().cloned() else {
                return Ok(false);
            };
            let list = developer_tool_sequence_delete_service(
                &context.global_db_path,
                DeveloperToolSequenceDeleteRequestDto { id: record.id },
            )?;
            controller.apply(HarnessTuiEvent::SetSequences(list.sequences));
            controller.apply(HarnessTuiEvent::SetStatus(
                HarnessTuiStatusKind::Success,
                format!("Deleted sequence `{}`.", record.name),
            ));
            Ok(false)
        }
    }
}

fn parse_json_for_tui(
    input: &str,
    controller: &mut HarnessTuiController,
) -> Result<JsonValue, HarnessTerminalError> {
    serde_json::from_str(input).map_err(|error| {
        let message = format!("Invalid JSON input: {error}");
        controller.apply(HarnessTuiEvent::SetError(message.clone()));
        HarnessTerminalError::user_fixable("developer_tool_tui_invalid_json", message)
    })
}

fn edit_json_in_external_editor(input: &str) -> Result<String, HarnessTerminalError> {
    let editor = env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .ok_or_else(|| {
            HarnessTerminalError::user_fixable(
                "developer_tool_tui_editor_missing",
                "Set VISUAL or EDITOR to edit JSON input from the TUI.",
            )
        })?;
    let mut file = tempfile::Builder::new()
        .prefix("xero-tool-harness-input-")
        .suffix(".json")
        .tempfile()
        .map_err(io_error("developer_tool_tui_editor_file_failed"))?;
    std::io::Write::write_all(&mut file, input.as_bytes())
        .map_err(io_error("developer_tool_tui_editor_file_failed"))?;
    let status = Command::new(editor)
        .arg(file.path())
        .status()
        .map_err(io_error("developer_tool_tui_editor_failed"))?;
    if !status.success() {
        return Err(HarnessTerminalError::user_fixable(
            "developer_tool_tui_editor_failed",
            format!("The configured editor exited with status {status}."),
        ));
    }
    fs::read_to_string(file.path()).map_err(io_error("developer_tool_tui_editor_read_failed"))
}

fn take_input_json_or_default(
    args: &mut Vec<String>,
    tool_name: &str,
) -> Result<JsonValue, HarnessTerminalError> {
    match take_input_json(args)? {
        Some(value) => Ok(value),
        None => default_input_for_tool(tool_name),
    }
}

fn take_input_json(args: &mut Vec<String>) -> Result<Option<JsonValue>, HarnessTerminalError> {
    let inline = take_option(args, "--input-json")?;
    let file = take_option(args, "--input-file")?;
    let stdin = take_bool_flag(args, "--stdin");
    let selected = inline.is_some() as u8 + file.is_some() as u8 + stdin as u8;
    if selected > 1 {
        return Err(HarnessTerminalError::usage(
            "Use only one of --input-json, --input-file, or --stdin.",
        ));
    }
    let raw = if let Some(value) = inline {
        Some(value)
    } else if let Some(path) = file {
        Some(fs::read_to_string(path).map_err(io_error("developer_tool_input_file_failed"))?)
    } else if stdin {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .map_err(io_error("developer_tool_input_stdin_failed"))?;
        Some(buffer)
    } else {
        None
    };
    let Some(raw) = raw else {
        return Ok(None);
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|error| HarnessTerminalError::usage(format!("Input JSON is invalid: {error}")))
}

fn default_input_for_tool(tool_name: &str) -> Result<JsonValue, HarnessTerminalError> {
    let catalog = developer_tool_catalog_service(DeveloperToolCatalogServiceOptions {
        skill_tool_enabled: false,
        host_kind: DeveloperToolHarnessHostKind::Terminal,
    })?;
    let entry = catalog
        .entries
        .iter()
        .find(|entry| entry.tool_name == tool_name)
        .ok_or_else(|| {
            HarnessTerminalError::user_fixable(
                "developer_tool_unknown",
                format!("Unknown harness tool `{tool_name}`."),
            )
        })?;
    Ok(default_input_from_schema(entry.input_schema.as_ref()))
}

fn take_sequence_calls(
    args: &mut Vec<String>,
) -> Result<Vec<DeveloperToolHarnessCallDto>, HarnessTerminalError> {
    let calls_json = take_option(args, "--calls-json")?;
    let input_file = take_option(args, "--input-file")?;
    let stdin = take_bool_flag(args, "--stdin");
    let selected = calls_json.is_some() as u8 + input_file.is_some() as u8 + stdin as u8;
    if selected != 1 {
        return Err(HarnessTerminalError::usage(
            "Provide exactly one of --calls-json, --input-file, or --stdin.",
        ));
    }
    let raw = if let Some(value) = calls_json {
        value
    } else if let Some(path) = input_file {
        fs::read_to_string(path).map_err(io_error("developer_tool_sequence_file_failed"))?
    } else {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .map_err(io_error("developer_tool_sequence_stdin_failed"))?;
        buffer
    };
    let value: JsonValue = serde_json::from_str(&raw).map_err(|error| {
        HarnessTerminalError::usage(format!("Sequence JSON is invalid: {error}"))
    })?;
    if value.is_array() {
        serde_json::from_value(value).map_err(|error| {
            HarnessTerminalError::usage(format!("Sequence calls JSON is invalid: {error}"))
        })
    } else {
        match serde_json::from_value::<DeveloperToolSequenceUpsertRequestDto>(value.clone()) {
            Ok(record) => Ok(record.calls),
            Err(upsert_error) => {
                let record: DeveloperToolSequenceRecordDto = serde_json::from_value(value)
                    .map_err(|record_error| {
                        HarnessTerminalError::usage(format!(
                            "Sequence JSON is invalid: {upsert_error}; exported record parse also failed: {record_error}"
                        ))
                    })?;
                Ok(record.calls)
            }
        }
    }
}

fn take_sequence_selector(args: &mut Vec<String>) -> Result<String, HarnessTerminalError> {
    if args.is_empty() {
        Err(HarnessTerminalError::usage("Missing sequence id or name."))
    } else {
        Ok(args.remove(0))
    }
}

fn find_sequence(
    records: &[DeveloperToolSequenceRecordDto],
    selector: &str,
) -> Result<DeveloperToolSequenceRecordDto, HarnessTerminalError> {
    records
        .iter()
        .find(|record| record.id == selector || record.name == selector)
        .cloned()
        .ok_or_else(|| {
            HarnessTerminalError::user_fixable(
                "developer_tool_sequence_not_found",
                format!("No developer tool sequence named or identified by `{selector}` exists."),
            )
        })
}

fn parse_global_options(
    raw_args: Vec<String>,
) -> Result<(HarnessGlobalOptions, Vec<String>), HarnessTerminalError> {
    let mut output_mode = HarnessOutputMode::Text;
    let mut app_data_dir = None;
    let mut command_args = Vec::new();
    let mut iter = raw_args.into_iter();
    let _program = iter.next();

    while let Some(arg) = iter.next() {
        if arg == "--json" {
            output_mode = HarnessOutputMode::Json;
        } else if arg == "--app-data-dir" {
            let value = iter
                .next()
                .ok_or_else(|| HarnessTerminalError::usage("Missing value for --app-data-dir."))?;
            app_data_dir = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--app-data-dir=") {
            app_data_dir = Some(PathBuf::from(value));
        } else {
            command_args.push(arg);
        }
    }

    Ok((
        HarnessGlobalOptions {
            output_mode,
            app_data_dir: app_data_dir.unwrap_or(default_app_data_dir()?),
        },
        command_args,
    ))
}

fn requested_output_mode(args: &[String]) -> HarnessOutputMode {
    if args.iter().any(|arg| arg == "--json") {
        HarnessOutputMode::Json
    } else {
        HarnessOutputMode::Text
    }
}

fn response(
    globals: &HarnessGlobalOptions,
    text: impl Into<String>,
    json_value: JsonValue,
) -> HarnessTerminalResponse {
    HarnessTerminalResponse {
        output_mode: globals.output_mode,
        text: text.into(),
        json: json_value,
        emit: true,
    }
}

fn emit_response(response: &HarnessTerminalResponse) {
    match response.output_mode {
        HarnessOutputMode::Text => println!("{}", response.text),
        HarnessOutputMode::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&response.json)
                    .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".into())
            );
        }
    }
}

fn emit_error(error: &HarnessTerminalError, mode: HarnessOutputMode) {
    match mode {
        HarnessOutputMode::Text => eprintln!("{}: {}", error.code, error.message),
        HarnessOutputMode::Json => eprintln!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "error": {
                    "code": error.code,
                    "message": error.message,
                }
            }))
            .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".into())
        ),
    }
}

fn root_help() -> String {
    [
        "Usage: tool-harness [--app-data-dir PATH] [--json] COMMAND",
        "",
        "Commands:",
        "  catalog                 List harness tools with terminal availability",
        "  fixture [prepare]       Seed and register the app-data fixture project",
        "  dry-run TOOL            Inspect policy and sandbox decisions",
        "  run TOOL                Execute one synthetic tool call",
        "  sequence list|save|delete|run|export",
        "  tui                     Open the interactive terminal harness",
        "  model-run TOOL          Start explicit model-driven harness mode",
    ]
    .join("\n")
}

fn root_help_json() -> JsonValue {
    json!({
        "commands": ["catalog", "fixture", "dry-run", "run", "sequence", "tui", "model-run"]
    })
}

fn take_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

fn take_tool_name(args: &mut Vec<String>, command: &str) -> Result<String, HarnessTerminalError> {
    let tool_name = take_option(args, "--tool")?
        .or_else(|| (!args.is_empty()).then(|| args.remove(0)))
        .ok_or_else(|| HarnessTerminalError::usage(format!("Missing tool name for {command}.")))?;
    if tool_name.trim().is_empty() {
        return Err(HarnessTerminalError::usage("Tool name cannot be empty."));
    }
    Ok(tool_name)
}

fn take_option(args: &mut Vec<String>, name: &str) -> Result<Option<String>, HarnessTerminalError> {
    if let Some(index) = args
        .iter()
        .position(|arg| arg == name || arg.starts_with(&format!("{name}=")))
    {
        let arg = args.remove(index);
        if let Some((_, value)) = arg.split_once('=') {
            return Ok(Some(value.to_owned()));
        }
        if index >= args.len() {
            return Err(HarnessTerminalError::usage(format!(
                "Missing value for {name}."
            )));
        }
        return Ok(Some(args.remove(index)));
    }
    Ok(None)
}

fn take_bool_flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == name) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn reject_unknown_options(args: &[String]) -> Result<(), HarnessTerminalError> {
    if let Some(arg) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(HarnessTerminalError::usage(format!(
            "Unknown option `{arg}`."
        )));
    }
    if !args.is_empty() {
        return Err(HarnessTerminalError::usage(format!(
            "Unexpected argument `{}`.",
            args[0]
        )));
    }
    Ok(())
}

fn default_app_data_dir() -> Result<PathBuf, HarnessTerminalError> {
    if let Some(path) = env::var_os("XERO_APP_DATA_DIR") {
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }

    #[cfg(target_os = "macos")]
    {
        home_dir()
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join(APP_DATA_DIRECTORY_NAME)
            })
            .ok_or_else(home_dir_error)
    }

    #[cfg(target_os = "windows")]
    {
        env::var_os("APPDATA")
            .or_else(|| env::var_os("LOCALAPPDATA"))
            .map(|root| PathBuf::from(root).join(APP_DATA_DIRECTORY_NAME))
            .ok_or_else(|| {
                HarnessTerminalError::system_fault(
                    "developer_tool_app_data_unavailable",
                    "APPDATA or LOCALAPPDATA is required to locate Xero app-data state.",
                )
            })
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(path) = env::var_os("XDG_DATA_HOME") {
            if !path.is_empty() {
                return Ok(PathBuf::from(path).join(APP_DATA_DIRECTORY_NAME));
            }
        }
        home_dir()
            .map(|home| {
                home.join(".local")
                    .join("share")
                    .join(APP_DATA_DIRECTORY_NAME)
            })
            .ok_or_else(home_dir_error)
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

fn home_dir_error() -> HarnessTerminalError {
    HarnessTerminalError::system_fault(
        "developer_tool_app_data_unavailable",
        "HOME is required to locate Xero app-data state.",
    )
}

fn io_error(code: &'static str) -> impl FnOnce(std::io::Error) -> HarnessTerminalError + Copy {
    move |error| HarnessTerminalError::system_fault(code, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn args(app_data: &Path, extra: &[&str]) -> Vec<String> {
        let mut args = vec![
            "tool-harness".to_string(),
            "--app-data-dir".into(),
            app_data.to_string_lossy().into_owned(),
        ];
        args.extend(extra.iter().map(|value| (*value).to_string()));
        args
    }

    #[test]
    fn catalog_json_marks_terminal_unavailable_tools() {
        let tempdir = tempfile::tempdir().expect("tempdir");

        let response =
            run_with_args(args(tempdir.path(), &["--json", "catalog"])).expect("catalog response");

        assert_eq!(response.output_mode, HarnessOutputMode::Json);
        let entries = response.json["catalog"]["entries"]
            .as_array()
            .expect("entries");
        let browser = entries
            .iter()
            .find(|entry| entry["toolName"] == json!("browser_control"))
            .expect("browser tool");
        assert_eq!(browser["runtimeAvailable"], json!(false));
        assert!(browser["runtimeUnavailableReason"]
            .as_str()
            .unwrap_or_default()
            .contains("Tauri desktop browser executor"));
    }

    #[test]
    fn fixture_command_uses_app_data_storage() {
        let tempdir = tempfile::tempdir().expect("tempdir");

        let response =
            run_with_args(args(tempdir.path(), &["--json", "fixture"])).expect("fixture");

        let root = response.json["project"]["rootPath"].as_str().expect("root");
        assert!(root.starts_with(tempdir.path().to_str().unwrap()));
        assert!(!root.contains("/.xero/"));
        assert!(Path::new(root).join("README.md").is_file());
    }

    #[test]
    fn dry_run_read_smoke_uses_synthesized_default_input() {
        let tempdir = tempfile::tempdir().expect("tempdir");

        let response = run_with_args(args(
            tempdir.path(),
            &[
                "--json",
                "dry-run",
                "read",
                "--input-json",
                r#"{"path":"README.md"}"#,
            ],
        ))
        .expect("dry-run");

        assert_eq!(response.json["kind"], json!("dryRun"));
        assert_eq!(response.json["result"]["toolName"], json!("read"));
    }

    #[test]
    fn run_read_smoke_returns_synthetic_result() {
        let tempdir = tempfile::tempdir().expect("tempdir");

        let response = run_with_args(args(
            tempdir.path(),
            &[
                "--json",
                "run",
                "read",
                "--input-json",
                r#"{"path":"README.md"}"#,
            ],
        ))
        .expect("run");

        assert_eq!(response.json["kind"], json!("run"));
        assert_eq!(response.json["result"]["hadFailure"], json!(false));
    }

    #[test]
    fn sequence_save_list_run_delete_round_trip() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let calls = r#"[{"toolName":"read","input":{"path":"README.md"}}]"#;

        let save = run_with_args(args(
            tempdir.path(),
            &[
                "--json",
                "sequence",
                "save",
                "--name",
                "smoke",
                "--calls-json",
                calls,
            ],
        ))
        .expect("save");
        assert_eq!(save.json["sequence"]["name"], json!("smoke"));

        let list =
            run_with_args(args(tempdir.path(), &["--json", "sequence", "list"])).expect("list");
        assert_eq!(list.json["sequences"].as_array().unwrap().len(), 1);

        let replay = run_with_args(args(
            tempdir.path(),
            &["--json", "sequence", "run", "smoke"],
        ))
        .expect("run sequence");
        assert_eq!(replay.json["result"]["hadFailure"], json!(false));

        let delete = run_with_args(args(
            tempdir.path(),
            &["--json", "sequence", "delete", "smoke"],
        ))
        .expect("delete");
        assert_eq!(delete.json["sequences"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn unavailable_tool_rejects_before_dispatch() {
        let tempdir = tempfile::tempdir().expect("tempdir");

        let error = run_with_args(args(
            tempdir.path(),
            &[
                "run",
                "browser_control",
                "--input-json",
                r#"{"action":"noop"}"#,
            ],
        ))
        .expect_err("unavailable");

        assert_eq!(error.code, "developer_tool_terminal_tool_unavailable");
    }

    #[test]
    fn model_run_fake_provider_requires_explicit_fixture_flag() {
        let tempdir = tempfile::tempdir().expect("tempdir");

        let error = run_with_args(args(
            tempdir.path(),
            &[
                "model-run",
                "read",
                "--prompt",
                "Inspect README.md",
                "--provider",
                "fake_provider",
                "--model",
                "fake-model",
            ],
        ))
        .expect_err("fake provider must be explicit");

        assert_eq!(
            error.code,
            "developer_tool_model_fake_provider_requires_flag"
        );
    }
}
