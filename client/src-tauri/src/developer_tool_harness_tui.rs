use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use serde_json::{json, Map, Number, Value as JsonValue};

use crate::commands::{
    DeveloperToolCatalogEntryDto, DeveloperToolCatalogResponseDto, DeveloperToolDryRunResponseDto,
    DeveloperToolHarnessCallDto, DeveloperToolHarnessRunOptionsDto, DeveloperToolSequenceRecordDto,
    DeveloperToolSyntheticRunResponseDto,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessTuiInputMode {
    Normal,
    Search,
    SequenceName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessTuiStatusKind {
    Idle,
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessTuiAction {
    None,
    Quit,
    DryRun,
    Run,
    EditInput,
    SaveSequence(String),
    ReplaySequence,
    DeleteSequence,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HarnessTuiEvent {
    Up,
    Down,
    PageUp,
    PageDown,
    SearchStart,
    PromptSequenceName,
    InputChar(char),
    Backspace,
    CommitInput,
    CancelInput,
    CycleGroup,
    ClearFilters,
    ToggleHelp,
    ToggleApproveWrites,
    ToggleOperatorApproval,
    DryRun,
    Run,
    EditInput,
    Quit,
    AddSelectedToSequence,
    SelectSequencePrev,
    SelectSequenceNext,
    ReplaySequence,
    DeleteSequence,
    SetCatalog(DeveloperToolCatalogResponseDto),
    SetSequences(Vec<DeveloperToolSequenceRecordDto>),
    SetInputJson(String),
    SetDryRunResult(Box<DeveloperToolDryRunResponseDto>),
    SetRunResult(DeveloperToolSyntheticRunResponseDto),
    SetError(String),
    SetStatus(HarnessTuiStatusKind, String),
}

#[derive(Debug, Clone)]
pub struct HarnessTuiState {
    pub catalog: DeveloperToolCatalogResponseDto,
    pub sequences: Vec<DeveloperToolSequenceRecordDto>,
    pub selected: usize,
    pub search: String,
    pub group_filter: Option<String>,
    pub input_mode: HarnessTuiInputMode,
    pub prompt_buffer: String,
    pub input_json: String,
    pub approve_writes: bool,
    pub operator_approve_all: bool,
    pub current_sequence: Vec<DeveloperToolHarnessCallDto>,
    pub selected_sequence: usize,
    pub result_title: String,
    pub result_json: Option<JsonValue>,
    pub status_kind: HarnessTuiStatusKind,
    pub status_message: String,
    pub help_visible: bool,
}

impl HarnessTuiState {
    pub fn new(
        catalog: DeveloperToolCatalogResponseDto,
        sequences: Vec<DeveloperToolSequenceRecordDto>,
    ) -> Self {
        let mut state = Self {
            catalog,
            sequences,
            selected: 0,
            search: String::new(),
            group_filter: None,
            input_mode: HarnessTuiInputMode::Normal,
            prompt_buffer: String::new(),
            input_json: "{}".into(),
            approve_writes: false,
            operator_approve_all: false,
            current_sequence: Vec::new(),
            selected_sequence: 0,
            result_title: "Result".into(),
            result_json: None,
            status_kind: HarnessTuiStatusKind::Idle,
            status_message: "Ready".into(),
            help_visible: false,
        };
        state.refresh_input_from_selection();
        state
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        let search = self.search.trim().to_lowercase();
        self.catalog
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                self.group_filter
                    .as_ref()
                    .map(|group| entry.group == *group)
                    .unwrap_or(true)
            })
            .filter(|(_, entry)| {
                if search.is_empty() {
                    return true;
                }
                entry.tool_name.to_lowercase().contains(&search)
                    || entry.description.to_lowercase().contains(&search)
                    || entry
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&search))
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn groups(&self) -> Vec<String> {
        let mut groups = self
            .catalog
            .entries
            .iter()
            .map(|entry| entry.group.clone())
            .collect::<Vec<_>>();
        groups.sort();
        groups.dedup();
        groups
    }

    pub fn selected_entry(&self) -> Option<&DeveloperToolCatalogEntryDto> {
        let indices = self.filtered_indices();
        let catalog_index = indices.get(self.selected).copied()?;
        self.catalog.entries.get(catalog_index)
    }

    pub fn selected_sequence(&self) -> Option<&DeveloperToolSequenceRecordDto> {
        self.sequences.get(self.selected_sequence)
    }

    fn selected_tool_available(&self) -> Result<(), String> {
        let entry = self
            .selected_entry()
            .ok_or_else(|| "No catalog entry is selected.".to_string())?;
        if entry.runtime_available {
            Ok(())
        } else {
            Err(entry.runtime_unavailable_reason.clone().unwrap_or_else(|| {
                format!("`{}` is unavailable in this runtime.", entry.tool_name)
            }))
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.filtered_indices().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
        if self.selected_sequence >= self.sequences.len() {
            self.selected_sequence = self.sequences.len().saturating_sub(1);
        }
    }

    fn refresh_input_from_selection(&mut self) {
        self.clamp_selection();
        if let Some(entry) = self.selected_entry() {
            self.input_json = pretty_json(&default_input_from_schema(entry.input_schema.as_ref()));
        }
    }

    fn set_status(&mut self, kind: HarnessTuiStatusKind, message: impl Into<String>) {
        self.status_kind = kind;
        self.status_message = message.into();
    }

    fn parse_input_json(&self) -> Result<JsonValue, String> {
        serde_json::from_str(&self.input_json)
            .map_err(|error| format!("Invalid JSON input: {error}"))
    }

    pub fn current_run_options(&self) -> DeveloperToolHarnessRunOptionsDto {
        DeveloperToolHarnessRunOptionsDto {
            stop_on_failure: Some(true),
            approve_writes: Some(self.approve_writes),
            operator_approve_all: Some(self.operator_approve_all),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HarnessTuiController {
    pub state: HarnessTuiState,
}

impl HarnessTuiController {
    pub fn new(
        catalog: DeveloperToolCatalogResponseDto,
        sequences: Vec<DeveloperToolSequenceRecordDto>,
    ) -> Self {
        Self {
            state: HarnessTuiState::new(catalog, sequences),
        }
    }

    pub fn apply(&mut self, event: HarnessTuiEvent) -> HarnessTuiAction {
        match event {
            HarnessTuiEvent::Up => {
                self.state.selected = self.state.selected.saturating_sub(1);
                self.state.refresh_input_from_selection();
                HarnessTuiAction::None
            }
            HarnessTuiEvent::Down => {
                self.state.selected = self.state.selected.saturating_add(1);
                self.state.refresh_input_from_selection();
                HarnessTuiAction::None
            }
            HarnessTuiEvent::PageUp => {
                self.state.selected = self.state.selected.saturating_sub(8);
                self.state.refresh_input_from_selection();
                HarnessTuiAction::None
            }
            HarnessTuiEvent::PageDown => {
                self.state.selected = self.state.selected.saturating_add(8);
                self.state.refresh_input_from_selection();
                HarnessTuiAction::None
            }
            HarnessTuiEvent::SearchStart => {
                self.state.input_mode = HarnessTuiInputMode::Search;
                self.state.prompt_buffer = self.state.search.clone();
                self.state
                    .set_status(HarnessTuiStatusKind::Info, "Search catalog");
                HarnessTuiAction::None
            }
            HarnessTuiEvent::PromptSequenceName => {
                if self.state.current_sequence.is_empty() {
                    self.state.set_status(
                        HarnessTuiStatusKind::Warning,
                        "Add at least one call before saving a sequence.",
                    );
                } else {
                    self.state.input_mode = HarnessTuiInputMode::SequenceName;
                    self.state.prompt_buffer.clear();
                    self.state
                        .set_status(HarnessTuiStatusKind::Info, "Sequence name");
                }
                HarnessTuiAction::None
            }
            HarnessTuiEvent::InputChar(ch) => {
                match self.state.input_mode {
                    HarnessTuiInputMode::Normal => {}
                    HarnessTuiInputMode::Search | HarnessTuiInputMode::SequenceName => {
                        self.state.prompt_buffer.push(ch);
                    }
                }
                HarnessTuiAction::None
            }
            HarnessTuiEvent::Backspace => {
                if self.state.input_mode != HarnessTuiInputMode::Normal {
                    self.state.prompt_buffer.pop();
                }
                HarnessTuiAction::None
            }
            HarnessTuiEvent::CommitInput => match self.state.input_mode {
                HarnessTuiInputMode::Normal => HarnessTuiAction::None,
                HarnessTuiInputMode::Search => {
                    self.state.search = self.state.prompt_buffer.trim().to_owned();
                    self.state.input_mode = HarnessTuiInputMode::Normal;
                    self.state.prompt_buffer.clear();
                    self.state.selected = 0;
                    self.state.refresh_input_from_selection();
                    self.state
                        .set_status(HarnessTuiStatusKind::Info, "Search applied.");
                    HarnessTuiAction::None
                }
                HarnessTuiInputMode::SequenceName => {
                    let name = self.state.prompt_buffer.trim().to_owned();
                    self.state.input_mode = HarnessTuiInputMode::Normal;
                    self.state.prompt_buffer.clear();
                    if name.is_empty() {
                        self.state.set_status(
                            HarnessTuiStatusKind::Error,
                            "Sequence name cannot be empty.",
                        );
                        HarnessTuiAction::None
                    } else {
                        HarnessTuiAction::SaveSequence(name)
                    }
                }
            },
            HarnessTuiEvent::CancelInput => {
                self.state.input_mode = HarnessTuiInputMode::Normal;
                self.state.prompt_buffer.clear();
                self.state
                    .set_status(HarnessTuiStatusKind::Info, "Cancelled.");
                HarnessTuiAction::None
            }
            HarnessTuiEvent::CycleGroup => {
                let groups = self.state.groups();
                self.state.group_filter = match self.state.group_filter.as_ref() {
                    None => groups.first().cloned(),
                    Some(current) => groups
                        .iter()
                        .position(|group| group == current)
                        .and_then(|index| groups.get(index + 1).cloned()),
                };
                self.state.selected = 0;
                self.state.refresh_input_from_selection();
                self.state
                    .set_status(HarnessTuiStatusKind::Info, "Group filter changed.");
                HarnessTuiAction::None
            }
            HarnessTuiEvent::ClearFilters => {
                self.state.search.clear();
                self.state.group_filter = None;
                self.state.selected = 0;
                self.state.refresh_input_from_selection();
                self.state
                    .set_status(HarnessTuiStatusKind::Info, "Filters cleared.");
                HarnessTuiAction::None
            }
            HarnessTuiEvent::ToggleHelp => {
                self.state.help_visible = !self.state.help_visible;
                HarnessTuiAction::None
            }
            HarnessTuiEvent::ToggleApproveWrites => {
                self.state.approve_writes = !self.state.approve_writes;
                self.state.set_status(
                    HarnessTuiStatusKind::Info,
                    format!("Write approval: {}", on_off(self.state.approve_writes)),
                );
                HarnessTuiAction::None
            }
            HarnessTuiEvent::ToggleOperatorApproval => {
                self.state.operator_approve_all = !self.state.operator_approve_all;
                self.state.set_status(
                    HarnessTuiStatusKind::Info,
                    format!(
                        "Operator approval: {}",
                        on_off(self.state.operator_approve_all)
                    ),
                );
                HarnessTuiAction::None
            }
            HarnessTuiEvent::DryRun => self.action_for_selected(HarnessTuiAction::DryRun),
            HarnessTuiEvent::Run => self.action_for_selected(HarnessTuiAction::Run),
            HarnessTuiEvent::EditInput => {
                if self.state.selected_entry().is_none() {
                    self.state
                        .set_status(HarnessTuiStatusKind::Warning, "No selected tool.");
                    HarnessTuiAction::None
                } else {
                    HarnessTuiAction::EditInput
                }
            }
            HarnessTuiEvent::Quit => HarnessTuiAction::Quit,
            HarnessTuiEvent::AddSelectedToSequence => {
                if let Err(message) = self.state.selected_tool_available() {
                    self.state
                        .set_status(HarnessTuiStatusKind::Warning, message);
                    return HarnessTuiAction::None;
                }
                let Some(entry) = self.state.selected_entry() else {
                    self.state
                        .set_status(HarnessTuiStatusKind::Warning, "No selected tool.");
                    return HarnessTuiAction::None;
                };
                match self.state.parse_input_json() {
                    Ok(input) => {
                        let tool_name = entry.tool_name.clone();
                        self.state
                            .current_sequence
                            .push(DeveloperToolHarnessCallDto {
                                tool_name: tool_name.clone(),
                                input,
                                tool_call_id: None,
                            });
                        self.state.set_status(
                            HarnessTuiStatusKind::Success,
                            format!("Added `{tool_name}` to the current sequence."),
                        );
                    }
                    Err(message) => self.state.set_status(HarnessTuiStatusKind::Error, message),
                }
                HarnessTuiAction::None
            }
            HarnessTuiEvent::SelectSequencePrev => {
                self.state.selected_sequence = self.state.selected_sequence.saturating_sub(1);
                HarnessTuiAction::None
            }
            HarnessTuiEvent::SelectSequenceNext => {
                self.state.selected_sequence = self.state.selected_sequence.saturating_add(1);
                self.state.clamp_selection();
                HarnessTuiAction::None
            }
            HarnessTuiEvent::ReplaySequence => {
                if self.state.selected_sequence().is_some() {
                    HarnessTuiAction::ReplaySequence
                } else {
                    self.state
                        .set_status(HarnessTuiStatusKind::Warning, "No saved sequence selected.");
                    HarnessTuiAction::None
                }
            }
            HarnessTuiEvent::DeleteSequence => {
                if self.state.selected_sequence().is_some() {
                    HarnessTuiAction::DeleteSequence
                } else {
                    self.state
                        .set_status(HarnessTuiStatusKind::Warning, "No saved sequence selected.");
                    HarnessTuiAction::None
                }
            }
            HarnessTuiEvent::SetCatalog(catalog) => {
                self.state.catalog = catalog;
                self.state.selected = 0;
                self.state.refresh_input_from_selection();
                HarnessTuiAction::None
            }
            HarnessTuiEvent::SetSequences(sequences) => {
                self.state.sequences = sequences;
                self.state.clamp_selection();
                HarnessTuiAction::None
            }
            HarnessTuiEvent::SetInputJson(input) => {
                self.state.input_json = input;
                self.state
                    .set_status(HarnessTuiStatusKind::Success, "Input updated.");
                HarnessTuiAction::None
            }
            HarnessTuiEvent::SetDryRunResult(result) => {
                let kind = if result.sandbox_denied || result.policy_decision.action == "deny" {
                    HarnessTuiStatusKind::Warning
                } else {
                    HarnessTuiStatusKind::Success
                };
                self.state.result_title = "Dry-run result".into();
                self.state.result_json = Some(json!(result));
                self.state.set_status(kind, "Dry-run completed.");
                HarnessTuiAction::None
            }
            HarnessTuiEvent::SetRunResult(result) => {
                let kind = if result.had_failure {
                    HarnessTuiStatusKind::Error
                } else {
                    HarnessTuiStatusKind::Success
                };
                self.state.result_title = "Run result".into();
                self.state.result_json = Some(json!(result));
                self.state.set_status(kind, "Run completed.");
                HarnessTuiAction::None
            }
            HarnessTuiEvent::SetError(message) => {
                self.state.result_title = "Error".into();
                self.state.result_json = Some(json!({ "error": message }));
                self.state.set_status(HarnessTuiStatusKind::Error, message);
                HarnessTuiAction::None
            }
            HarnessTuiEvent::SetStatus(kind, message) => {
                self.state.set_status(kind, message);
                HarnessTuiAction::None
            }
        }
    }

    fn action_for_selected(&mut self, action: HarnessTuiAction) -> HarnessTuiAction {
        if let Err(message) = self.state.selected_tool_available() {
            self.state
                .set_status(HarnessTuiStatusKind::Warning, message);
            return HarnessTuiAction::None;
        }
        match self.state.parse_input_json() {
            Ok(_) => action,
            Err(message) => {
                self.state.set_status(HarnessTuiStatusKind::Error, message);
                HarnessTuiAction::None
            }
        }
    }
}

pub fn default_input_from_schema(schema: Option<&JsonValue>) -> JsonValue {
    let Some(schema) = schema else {
        return json!({});
    };
    if let Some(default) = schema.get("default") {
        return default.clone();
    }
    if let Some(values) = schema.get("enum").and_then(JsonValue::as_array) {
        if let Some(first) = values.first() {
            return first.clone();
        }
    }
    match schema.get("type").and_then(JsonValue::as_str) {
        Some("object") => {
            let mut object = Map::new();
            if let Some(properties) = schema.get("properties").and_then(JsonValue::as_object) {
                for (key, child) in properties {
                    object.insert(key.clone(), default_input_from_schema(Some(child)));
                }
            }
            JsonValue::Object(object)
        }
        Some("array") => JsonValue::Array(Vec::new()),
        Some("boolean") => JsonValue::Bool(false),
        Some("integer") => JsonValue::Number(Number::from(0)),
        Some("number") => JsonValue::Number(Number::from(0)),
        Some("string") => JsonValue::String(String::new()),
        _ => JsonValue::Object(Map::new()),
    }
}

pub fn pretty_json(value: &JsonValue) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into())
}

pub fn render_harness_tui(frame: &mut Frame<'_>, state: &HarnessTuiState) {
    let root = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(9),
            Constraint::Length(1),
        ])
        .split(root);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(vertical[0]);
    render_catalog(frame, top[0], state);
    render_detail(frame, top[1], state);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(vertical[1]);
    render_input(frame, bottom[0], state);
    render_result(frame, bottom[1], state);
    render_footer(frame, vertical[2], state);

    if state.help_visible {
        let area = centered_rect(74, 58, root);
        frame.render_widget(Clear, area);
        let help = Paragraph::new(help_lines())
            .block(Block::default().title("Help").borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        frame.render_widget(help, area);
    }
}

fn render_catalog(frame: &mut Frame<'_>, area: Rect, state: &HarnessTuiState) {
    let indices = state.filtered_indices();
    let items = indices
        .iter()
        .enumerate()
        .map(|(row, catalog_index)| {
            let entry = &state.catalog.entries[*catalog_index];
            let marker = if row == state.selected { "> " } else { "  " };
            let availability = if entry.runtime_available {
                ""
            } else {
                " unavailable"
            };
            let style = if row == state.selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if entry.runtime_available {
                Style::default()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::styled(
                    entry.tool_name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" [{}]{}", entry.group, availability)),
            ]))
            .style(style)
        })
        .collect::<Vec<_>>();
    let title = format!(
        "Catalog {} tool{}",
        indices.len(),
        if indices.len() == 1 { "" } else { "s" }
    );
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &HarnessTuiState) {
    let lines = match state.selected_entry() {
        Some(entry) => detail_lines(entry, state),
        None => vec![Line::from("No tools match the current filters.")],
    };
    let paragraph = Paragraph::new(lines)
        .block(Block::default().title("Details").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn render_input(frame: &mut Frame<'_>, area: Rect, state: &HarnessTuiState) {
    let title = format!(
        "Input JSON | writes: {} | operator: {}",
        on_off(state.approve_writes),
        on_off(state.operator_approve_all)
    );
    let paragraph = Paragraph::new(state.input_json.clone())
        .block(Block::default().title(title).borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_result(frame: &mut Frame<'_>, area: Rect, state: &HarnessTuiState) {
    let mut body = state
        .result_json
        .as_ref()
        .map(pretty_json)
        .unwrap_or_else(|| {
            let sequence = if state.current_sequence.is_empty() {
                "No current sequence calls.".to_string()
            } else {
                format!(
                    "Current sequence: {} call(s).",
                    state.current_sequence.len()
                )
            };
            let saved = state
                .selected_sequence()
                .map(|record| format!("Selected saved sequence: {}", record.name))
                .unwrap_or_else(|| "No saved sequence selected.".into());
            format!("{sequence}\n{saved}")
        });
    if body.len() > 8_000 {
        body.truncate(8_000);
        body.push_str("\n... output truncated in TUI view; use --json for full output.");
    }
    let paragraph = Paragraph::new(body)
        .block(
            Block::default()
                .title(state.result_title.clone())
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &HarnessTuiState) {
    let filter = if state.search.is_empty() {
        "search: all".into()
    } else {
        format!("search: {}", state.search)
    };
    let group = state
        .group_filter
        .as_ref()
        .map(|group| format!("group: {group}"))
        .unwrap_or_else(|| "group: all".into());
    let mode = match state.input_mode {
        HarnessTuiInputMode::Normal => "normal".to_string(),
        HarnessTuiInputMode::Search => format!("search> {}", state.prompt_buffer),
        HarnessTuiInputMode::SequenceName => format!("save> {}", state.prompt_buffer),
    };
    let status_style = match state.status_kind {
        HarnessTuiStatusKind::Idle | HarnessTuiStatusKind::Info => Style::default(),
        HarnessTuiStatusKind::Success => Style::default().fg(Color::Green),
        HarnessTuiStatusKind::Warning => Style::default().fg(Color::Yellow),
        HarnessTuiStatusKind::Error => Style::default().fg(Color::Red),
    };
    let footer = Paragraph::new(Line::from(vec![
        Span::raw(format!("{mode} | {filter} | {group} | ")),
        Span::styled(state.status_message.clone(), status_style),
        Span::raw(" | ? help | q quit"),
    ]));
    frame.render_widget(footer, area);
}

fn detail_lines(
    entry: &DeveloperToolCatalogEntryDto,
    state: &HarnessTuiState,
) -> Vec<Line<'static>> {
    let availability = if entry.runtime_available {
        "available".to_string()
    } else {
        entry
            .runtime_unavailable_reason
            .clone()
            .unwrap_or_else(|| "unavailable".into())
    };
    let packs = if entry.tool_packs.is_empty() {
        "none".into()
    } else {
        entry
            .tool_packs
            .iter()
            .map(|pack| pack.pack_id.clone())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let sequence_label = format!(
        "Current sequence calls: {} | Saved sequences: {}",
        state.current_sequence.len(),
        state.sequences.len()
    );
    vec![
        Line::from(vec![
            Span::styled(
                entry.tool_name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  {}", entry.description)),
        ]),
        Line::from(format!(
            "Group: {} | Risk: {} | Effect: {}",
            entry.group, entry.risk_class, entry.effect_class
        )),
        Line::from(format!("Runtime: {availability}")),
        Line::from(format!(
            "Agents: {}",
            if entry.allowed_runtime_agents.is_empty() {
                "none".into()
            } else {
                entry.allowed_runtime_agents.join(", ")
            }
        )),
        Line::from(format!(
            "Activation groups: {}",
            if entry.activation_groups.is_empty() {
                "none".into()
            } else {
                entry.activation_groups.join(", ")
            }
        )),
        Line::from(format!("Tool packs: {packs}")),
        Line::from(format!(
            "Schema fields: {}",
            if entry.schema_fields.is_empty() {
                "none".into()
            } else {
                entry.schema_fields.join(", ")
            }
        )),
        Line::from(sequence_label),
        Line::from("Keys: / search, g group, e edit, d dry-run, r run, n add, s save, p replay"),
    ]
}

fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("Navigation: up/down or j/k move, page up/down jumps."),
        Line::from("Filtering: / searches tool names, descriptions, and tags. g cycles groups. c clears filters."),
        Line::from("Execution: e opens JSON input in $VISUAL or $EDITOR. d dry-runs. r runs."),
        Line::from("Approvals: a toggles write approval. o toggles operator approval."),
        Line::from("Sequences: n adds selected call, s saves current calls, [ and ] choose saved, p replays, x deletes."),
        Line::from("Results: policy denials, sandbox denials, invalid JSON, and tool failures stay visible in the result pane."),
        Line::from("Press Esc to close prompts/help. Press q to quit."),
    ]
}

fn centered_rect(percent_x: u16, percent_y: u16, rect: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(rect);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn catalog() -> DeveloperToolCatalogResponseDto {
        DeveloperToolCatalogResponseDto {
            host_os: "macos".into(),
            host_os_label: "macOS".into(),
            skill_tool_enabled: true,
            entries: vec![
                DeveloperToolCatalogEntryDto {
                    tool_name: "read".into(),
                    group: "core".into(),
                    description: "Read a file.".into(),
                    tags: vec!["file".into()],
                    schema_fields: vec!["path".into()],
                    examples: vec![],
                    risk_class: "observe".into(),
                    effect_class: "observe".into(),
                    runtime_available: true,
                    allowed_runtime_agents: vec!["engineer".into()],
                    activation_groups: vec!["core".into()],
                    tool_packs: vec![],
                    input_schema: Some(json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" }
                        }
                    })),
                    runtime_unavailable_reason: None,
                },
                DeveloperToolCatalogEntryDto {
                    tool_name: "browser_control".into(),
                    group: "browser".into(),
                    description: "Control the browser.".into(),
                    tags: vec!["browser".into()],
                    schema_fields: vec![],
                    examples: vec![],
                    risk_class: "browser".into(),
                    effect_class: "browser_control".into(),
                    runtime_available: false,
                    allowed_runtime_agents: vec!["engineer".into()],
                    activation_groups: vec!["browser".into()],
                    tool_packs: vec![],
                    input_schema: Some(json!({"type": "object"})),
                    runtime_unavailable_reason: Some("Desktop browser executor missing.".into()),
                },
            ],
        }
    }

    #[test]
    fn controller_filters_and_tracks_selection() {
        let mut controller = HarnessTuiController::new(catalog(), Vec::new());
        assert_eq!(controller.state.selected_entry().unwrap().tool_name, "read");

        controller.apply(HarnessTuiEvent::SearchStart);
        controller.apply(HarnessTuiEvent::InputChar('b'));
        controller.apply(HarnessTuiEvent::CommitInput);

        assert_eq!(
            controller.state.selected_entry().unwrap().tool_name,
            "browser_control"
        );
        assert_eq!(controller.state.filtered_indices().len(), 1);
    }

    #[test]
    fn unavailable_tool_blocks_run_action() {
        let mut controller = HarnessTuiController::new(catalog(), Vec::new());
        controller.apply(HarnessTuiEvent::Down);

        let action = controller.apply(HarnessTuiEvent::Run);

        assert_eq!(action, HarnessTuiAction::None);
        assert_eq!(controller.state.status_kind, HarnessTuiStatusKind::Warning);
        assert!(controller
            .state
            .status_message
            .contains("Desktop browser executor"));
    }

    #[test]
    fn add_selected_call_to_sequence_uses_current_json_input() {
        let mut controller = HarnessTuiController::new(catalog(), Vec::new());
        controller.apply(HarnessTuiEvent::SetInputJson(
            r#"{"path":"README.md"}"#.into(),
        ));

        controller.apply(HarnessTuiEvent::AddSelectedToSequence);

        assert_eq!(controller.state.current_sequence.len(), 1);
        assert_eq!(controller.state.current_sequence[0].tool_name, "read");
    }

    #[test]
    fn default_input_synthesizes_object_properties() {
        let value = default_input_from_schema(Some(&json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "recursive": { "type": "boolean" },
                "count": { "type": "integer" }
            }
        })));

        assert_eq!(value, json!({"path": "", "recursive": false, "count": 0}));
    }

    #[test]
    fn renderer_includes_main_catalog_and_help_text() {
        let mut controller = HarnessTuiController::new(catalog(), Vec::new());
        controller.apply(HarnessTuiEvent::ToggleHelp);
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| render_harness_tui(frame, &controller.state))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Catalog"));
        assert!(rendered.contains("read"));
        assert!(rendered.contains("Execution:"));
    }
}
