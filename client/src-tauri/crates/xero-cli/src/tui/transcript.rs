//! Single-column transcript.
//!
//! - User prompt rows fill the viewport width on the elevated bg.
//! - Assistant text is rendered through a pulldown-cmark pipeline so
//!   markdown shows up styled instead of as raw `**foo**` characters.
//! - Each tool call renders as its own pill row with a short detail
//!   summary (path, argv, byte count) parsed from the call arguments.
//! - The streaming "thinking" indicator is a single-cell quadrant glyph
//!   that flips between ▚ and ▞ — the brand mark from the desktop app,
//!   minimized.

use std::time::Duration;

use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use serde_json::Value as JsonValue;

use super::{
    app::{App, ToolCallRow},
    theme,
};

/// Block row count for the welcome banner — logo + identity row + version
/// row. Callers use this to clamp `target_height` so the banner always
/// fits, even on a tight terminal.
pub fn welcome_logo_height() -> u16 {
    LOGO_ROWS.len() as u16 + 2
}

/// Build the welcome-banner lines for the scrollback. The logo is centered
/// horizontally inside `width` and vertically inside `target_height` rows
/// so the first paint fills the terminal instead of dropping a sliver of
/// logo into the cursor's current position.
///
/// Rendered top-to-bottom:
///   1. ASCII logo (gold)
///   2. Identity row — `@handle` when signed in, dim `not signed in · /login`
///      otherwise
///   3. Version label (dim) — centered under the identity row
pub fn welcome_banner_lines(
    width: u16,
    target_height: u16,
    version: &str,
    signed_in_handle: Option<&str>,
) -> Vec<Line<'static>> {
    let logo_width = LOGO_ROWS
        .iter()
        .map(|row| row.chars().count())
        .max()
        .unwrap_or(0);
    let logo_leading = (width as usize).saturating_sub(logo_width) / 2;
    let block_rows = welcome_logo_height();
    // Center vertically inside the target height; bias a hair high so the
    // logo doesn't feel mashed against the composer below.
    let total = target_height.max(block_rows);
    let top_pad = total.saturating_sub(block_rows) / 2;
    let bottom_pad = total.saturating_sub(block_rows + top_pad);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(total as usize);
    for _ in 0..top_pad {
        lines.push(Line::raw(""));
    }
    for row in LOGO_ROWS {
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(logo_leading)),
            Span::styled((*row).to_string(), theme::accent()),
        ]));
    }
    let identity = signed_in_handle.unwrap_or("not signed in · /login");
    let identity_width = identity.chars().count();
    let identity_leading = (width as usize).saturating_sub(identity_width) / 2;
    let identity_style = if signed_in_handle.is_some() {
        theme::fg()
    } else {
        theme::dim()
    };
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(identity_leading)),
        Span::styled(identity.to_string(), identity_style),
    ]));
    let version_label = format!("v{version}");
    let version_width = version_label.chars().count();
    let version_leading = (width as usize).saturating_sub(version_width) / 2;
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(version_leading)),
        Span::styled(version_label, theme::dim()),
    ]));
    for _ in 0..bottom_pad {
        lines.push(Line::raw(""));
    }
    lines
}

/// Render a single visible message (user / assistant / thinking) into
/// transcript lines. Used by [`super::app::commit_pending_history`] to
/// emit each message into the terminal's scrollback.
pub fn message_lines(message: &super::app::RuntimeMessageRow, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(thinking) = message.thinking.as_deref() {
        push_thinking_text(&mut lines, thinking, width);
        lines.push(Line::raw(""));
    }
    let role = message.role.as_str();
    let is_assistant = matches!(role, "assistant" | "model");
    // Intermediate assistant turns that are purely tool calls have empty
    // content. Don't emit an assistant block for them — only the tool
    // pills below should appear in scrollback.
    if is_assistant && message.content.trim().is_empty() {
        return lines;
    }
    match role {
        "user" => push_user(&mut lines, &message.content, width),
        "assistant" | "model" => push_assistant_markdown(&mut lines, &message.content),
        "thinking" => push_thinking_text(&mut lines, &message.content, width),
        other if other.contains("thinking") => {
            push_thinking_text(&mut lines, &message.content, width)
        }
        _ => push_assistant_markdown(&mut lines, &message.content),
    }
    lines
}

/// Build one tool-pill row per call. Calls are no longer grouped — each
/// invocation gets its own row so the user can see what was actually
/// touched. Returns `[pill, blank, pill, blank, ..., pill, blank]` —
/// the caller in `commit_pending_history` is responsible for the
/// leading separator so it composes cleanly with the message-commit
/// idx blank when no thinking/content precedes the pills.
pub fn tool_pill_lines(calls: &[ToolCallRow]) -> Vec<Line<'static>> {
    if calls.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::with_capacity(calls.len() * 2);
    for (idx, call) in calls.iter().enumerate() {
        if idx > 0 {
            lines.push(Line::raw(""));
        }
        push_tool_pill(&mut lines, call);
    }
    lines.push(Line::raw(""));
    lines
}

/// Render arbitrary assistant-side markdown text into transcript lines.
/// Used to render finalized assistant content and any un-streamed
/// remainder once the message finalizes.
pub fn assistant_markdown_lines(content: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    push_assistant_markdown(&mut lines, content);
    lines
}

/// Render assistant markdown and wrap it into terminal-width rows.
/// Streaming uses this to decide which rows are complete enough to
/// reveal while keeping the unfinished trailing row hidden.
pub fn assistant_markdown_wrapped_lines(content: &str, width: u16) -> Vec<Line<'static>> {
    wrap_rendered_lines(render_markdown(content), width)
}

pub fn is_hidden_role(role: &str) -> bool {
    matches!(role, "system" | "tool")
}

/// Streaming-only indicator rendered inside the inline viewport (not the
/// scrollback). Renders, anchored to the bottom of `area`:
///   1. an "in-flight tool" row showing the next un-finished tool call
///      (when there is one), with a braille spinner that ticks every
///      ~80ms;
///   2. the brand-mark row with `Thinking…`.
///
/// Extra rows above read as padding between the previous scrollback and the
/// spinner.
pub fn render_inline_thinking(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }
    let elapsed = app
        .run_detail
        .as_ref()
        .and_then(|detail| detail.started_at)
        .map(|start| start.elapsed())
        .unwrap_or_default();
    let frame_idx = frame_for(elapsed);
    let thinking_line = Line::from(vec![
        spinner_span(frame_idx),
        Span::raw("  "),
        Span::styled("Thinking…", theme::dim()),
    ]);
    let bottom = Rect {
        x: area.x,
        y: area.y + area.height - 1,
        width: area.width,
        height: 1,
    };
    frame.render_widget(Paragraph::new(thinking_line).style(theme::base()), bottom);

    if area.height >= 2 {
        // Priority: in-flight tool pill (currently executing). Streamed
        // assistant text is revealed through scrollback only after a
        // terminal row is complete, so the inline area never leaks raw
        // token-by-token deltas.
        let preview_line = in_flight_pill_line(app, elapsed);
        if let Some(line) = preview_line {
            let preview_row = Rect {
                x: area.x,
                y: area.y + area.height - 2,
                width: area.width,
                height: 1,
            };
            frame.render_widget(Paragraph::new(line).style(theme::base()), preview_row);
        }
    }
}

const TOOL_SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Build a single-row pill for the next pending tool call in the
/// in-flight assistant message. `None` if nothing is pending.
fn in_flight_pill_line(app: &App, elapsed: Duration) -> Option<Line<'static>> {
    let detail = app.run_detail.as_ref()?;
    let message = detail.messages.last()?;
    let pending: Vec<&ToolCallRow> = message
        .tool_calls
        .iter()
        .filter(|tool| tool.completed_duration.is_none())
        .collect();
    let first = pending.first()?;
    let frame_idx = (elapsed.as_millis() / 80) as usize % TOOL_SPINNER_FRAMES.len();
    let glyph = TOOL_SPINNER_FRAMES[frame_idx];
    let mut spans = vec![
        Span::raw("   "),
        Span::styled(format!("{glyph} "), theme::accent()),
        Span::styled(first.name.clone(), theme::muted()),
    ];
    if let Some(text) = first.detail.as_deref().filter(|s| !s.is_empty()) {
        spans.push(Span::styled(" · ", theme::dim()));
        spans.push(Span::styled(
            truncate(text, TOOL_DETAIL_MAX_CHARS),
            theme::dim(),
        ));
    }
    if pending.len() > 1 {
        spans.push(Span::styled(
            format!("  (+{} more)", pending.len() - 1),
            theme::dim(),
        ));
    }
    Some(Line::from(spans))
}

// ---------------------------------------------------------------------------
// Welcome logo
// ---------------------------------------------------------------------------

const LOGO_ROWS: &[&str] = &[
    "██╗  ██╗ ███████╗ ██████╗   ██████╗ ",
    "╚██╗██╔╝ ██╔════╝ ██╔══██╗ ██╔═══██╗",
    " ╚███╔╝  █████╗   ██████╔╝ ██║   ██║",
    " ██╔██╗  ██╔══╝   ██╔══██╗ ██║   ██║",
    "██╔╝ ██╗ ███████╗ ██║  ██║ ╚██████╔╝",
    "╚═╝  ╚═╝ ╚══════╝ ╚═╝  ╚═╝  ╚═════╝ ",
];

// ---------------------------------------------------------------------------
// Thinking spinner
// ---------------------------------------------------------------------------
//
// One cell wide, gold, rapidly cycling through dense punctuation
// glyphs. Reads as a "static crackle" — clearly different from the
// braille fill-spinner used on in-flight tool pills and visibly alive.

const GLITCH_FRAMES: &[&str] = &[
    "#", "&", "%", "@", "$", "*", "+", "=", "?", "!", "<", ">", "^", "~", "/", "\\", "|", ";", ":",
    "{", "}", "[", "]", "(", ")", "§", "¶", "¤", "°", "±", "÷", "×",
];

fn spinner_span(frame: usize) -> Span<'static> {
    let glyph = GLITCH_FRAMES[frame % GLITCH_FRAMES.len()];
    Span::styled(glyph, theme::accent())
}

fn frame_for(elapsed: Duration) -> usize {
    // 12ms per step (~80 fps target). Each redraw advances the cycle
    // by several frames, so the glyph reads as pure shimmer/static.
    (elapsed.as_millis() / 12) as usize
}

// ---------------------------------------------------------------------------
// User prompt — full-width bg
// ---------------------------------------------------------------------------

fn push_user(lines: &mut Vec<Line<'static>>, content: &str, width: u16) {
    let bg = theme::composer_bg_color();
    let stripe_style = theme::accent().bg(bg);
    let body_style = theme::fg().bg(bg);
    let total_width = width as usize;
    let push_blank = |lines: &mut Vec<Line<'static>>| {
        let pad_len = total_width.saturating_sub(2);
        let mut spans = vec![Span::styled(
            format!("{} ", theme::STRIPE_GLYPH),
            stripe_style,
        )];
        if pad_len > 0 {
            spans.push(Span::styled(" ".repeat(pad_len), body_style));
        }
        lines.push(Line::from(spans));
    };

    push_blank(lines);
    for segment in content.split('\n') {
        let leading = 2 + segment.chars().count();
        let pad_len = total_width.saturating_sub(leading);
        let mut spans = vec![
            Span::styled(format!("{} ", theme::STRIPE_GLYPH), stripe_style),
            Span::styled(segment.to_string(), body_style),
        ];
        if pad_len > 0 {
            spans.push(Span::styled(" ".repeat(pad_len), body_style));
        }
        lines.push(Line::from(spans));
    }
    push_blank(lines);
}

// ---------------------------------------------------------------------------
// Thinking text (inline reasoning content carried on assistant messages)
// ---------------------------------------------------------------------------

fn push_thinking_text(lines: &mut Vec<Line<'static>>, text: &str, width: u16) {
    for line in assistant_thinking_lines(text, width) {
        lines.push(line);
    }
}

const THINKING_PREFIX_WIDTH: usize = 3; // " ▎ "

/// Render reasoning ("thinking") content through the markdown pipeline
/// so `**bold**` etc. resolves visually, then drape every output row in
/// the thinking style (gold left-stripe, dim italic body) and wrap
/// long lines ourselves so each wrapped row carries the stripe. The
/// global `Thinking…` spinner on the inline footer is the only place
/// that ever shows the word "Thinking" — scrollback rows lean on the
/// stripe + italic dim styling to communicate the same thing.
pub fn assistant_thinking_lines(content: &str, width: u16) -> Vec<Line<'static>> {
    let rendered = render_markdown(content);
    let mut out = Vec::with_capacity(rendered.len());
    let total = (width as usize).max(THINKING_PREFIX_WIDTH + 1);
    let cont_width = total - THINKING_PREFIX_WIDTH;
    for line in rendered {
        if line.spans.iter().all(|s| s.content.trim().is_empty()) {
            out.push(Line::raw(""));
            continue;
        }
        let segments = wrap_spans(&line.spans, cont_width, cont_width);
        for segment in segments.into_iter() {
            let mut row_spans = Vec::with_capacity(segment.len() + 1);
            row_spans.push(Span::styled(
                format!(" {} ", theme::STRIPE_GLYPH),
                theme::accent(),
            ));
            for span in segment {
                let new_style = span.style.fg(theme::DIM).add_modifier(Modifier::ITALIC);
                row_spans.push(Span::styled(span.content.into_owned(), new_style));
            }
            out.push(Line::from(row_spans));
        }
    }
    out
}

fn wrap_rendered_lines(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let total = (width as usize).max(1);
    let mut out = Vec::new();
    for line in lines {
        if line.spans.iter().all(|s| s.content.trim().is_empty()) {
            out.push(Line::raw(""));
            continue;
        }
        for segment in wrap_spans(&line.spans, total, total) {
            if segment.is_empty() {
                out.push(Line::raw(""));
            } else {
                out.push(Line::from(segment));
            }
        }
    }
    out
}

/// Greedy word-wrapper for a single rendered markdown line. Splits
/// each span into alternating whitespace and word atoms, then packs
/// atoms into rows respecting `first_width` (e.g. label-shortened
/// first row) and `cont_width` for the remainder. Whitespace runs at
/// the start of a wrapped row are dropped so wrapping doesn't dump a
/// leading space onto continuation rows.
fn wrap_spans(
    spans: &[Span<'static>],
    first_width: usize,
    cont_width: usize,
) -> Vec<Vec<Span<'static>>> {
    let mut atoms: Vec<(Style, String)> = Vec::new();
    for span in spans {
        let style = span.style;
        let mut chars = span.content.chars().peekable();
        while let Some(&c) = chars.peek() {
            let is_ws = c.is_whitespace();
            let mut buf = String::new();
            while let Some(&peek) = chars.peek() {
                if peek.is_whitespace() == is_ws {
                    buf.push(peek);
                    chars.next();
                } else {
                    break;
                }
            }
            if !buf.is_empty() {
                atoms.push((style, buf));
            }
        }
    }

    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current_line: Vec<Span<'static>> = Vec::new();
    let mut current_width: usize = 0;

    for (style, text) in atoms {
        let is_ws = text.chars().all(char::is_whitespace);
        let mut remaining = text.as_str();
        while !remaining.is_empty() {
            let limit = if lines.is_empty() {
                first_width
            } else {
                cont_width
            }
            .max(1);
            if is_ws && current_width == 0 {
                break;
            }
            let available = limit.saturating_sub(current_width);
            if available == 0 {
                lines.push(std::mem::take(&mut current_line));
                current_width = 0;
                continue;
            }
            let remaining_width = remaining.chars().count();
            if remaining_width <= available {
                current_line.push(Span::styled(remaining.to_owned(), style));
                current_width += remaining_width;
                break;
            }
            if !is_ws && current_width > 0 {
                lines.push(std::mem::take(&mut current_line));
                current_width = 0;
                continue;
            }
            let (head, tail) = split_at_char_count(remaining, available);
            if !head.is_empty() {
                current_line.push(Span::styled(head.to_owned(), style));
                current_width += head.chars().count();
            }
            if !current_line.is_empty() {
                lines.push(std::mem::take(&mut current_line));
                current_width = 0;
            }
            remaining = tail;
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

fn split_at_char_count(value: &str, count: usize) -> (&str, &str) {
    if count == 0 {
        return ("", value);
    }
    match value.char_indices().nth(count) {
        Some((idx, _)) => value.split_at(idx),
        None => (value, ""),
    }
}

// ---------------------------------------------------------------------------
// Tool pill (one per call)
// ---------------------------------------------------------------------------

const TOOL_DETAIL_MAX_CHARS: usize = 60;

fn push_tool_pill(lines: &mut Vec<Line<'static>>, call: &ToolCallRow) {
    let mut spans = vec![
        Span::raw("   "),
        Span::styled(format!("{} ", theme::TOOL_DOT), theme::accent()),
        Span::styled(call.name.clone(), theme::muted()),
    ];
    if let Some(detail) = call.detail.as_deref().filter(|s| !s.is_empty()) {
        spans.push(Span::styled(" · ", theme::dim()));
        spans.push(Span::styled(
            truncate(detail, TOOL_DETAIL_MAX_CHARS),
            theme::dim(),
        ));
    }
    lines.push(Line::from(spans));
}

/// Short human-readable summary of a tool call's arguments. Each known
/// headless tool gets a custom format so the pill row carries useful
/// signal (path, argv, byte count) instead of just the tool name.
pub fn summarize_tool_arguments(name: &str, args: &JsonValue) -> Option<String> {
    let str_arg = |key: &str| args.get(key).and_then(JsonValue::as_str);
    match name {
        "read" | "list" | "delete" => str_arg("path").map(str::to_owned),
        "write" => {
            let path = str_arg("path")?;
            match str_arg("content") {
                Some(content) => Some(format!("{path} · {}b", content.len())),
                None => Some(path.to_owned()),
            }
        }
        "patch" => {
            let patch = str_arg("patch")?;
            let added = patch
                .lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                .count();
            let removed = patch
                .lines()
                .filter(|l| l.starts_with('-') && !l.starts_with("---"))
                .count();
            Some(format!("+{added} -{removed}"))
        }
        "move" => {
            let from = str_arg("from")?;
            let to = str_arg("to")?;
            Some(format!("{from} → {to}"))
        }
        "replace" => match (str_arg("path"), str_arg("search")) {
            (Some(path), Some(search)) => Some(format!("{path} · \"{}\"", truncate(search, 30))),
            (Some(path), None) => Some(path.to_owned()),
            (None, Some(search)) => Some(format!("\"{}\"", truncate(search, 40))),
            (None, None) => None,
        },
        "command" => {
            let argv = args.get("argv").and_then(JsonValue::as_array)?;
            let joined = argv
                .iter()
                .filter_map(JsonValue::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        }
        _ => None,
    }
}

fn truncate(input: &str, max_chars: usize) -> String {
    let count = input.chars().count();
    if count <= max_chars {
        return input.to_owned();
    }
    let cutoff = max_chars.saturating_sub(1);
    let head: String = input.chars().take(cutoff).collect();
    format!("{head}…")
}

// ---------------------------------------------------------------------------
// Markdown rendering
// ---------------------------------------------------------------------------

fn push_assistant_markdown(lines: &mut Vec<Line<'static>>, content: &str) {
    // No prefix mark on completed assistant content — the brand spinner
    // is reserved for the live in-progress indicator inside the inline
    // viewport. Past responses are plain text in scrollback.
    for line in render_markdown(content) {
        lines.push(line);
    }
}

fn render_markdown(source: &str) -> Vec<Line<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    let parser = Parser::new_ext(source, options);

    let mut state = MdRenderer::default();
    for event in parser {
        state.handle(event);
    }
    state.flush_line();

    // Collapse runs of 2+ blank lines down to one — block-end events plus
    // explicit blank-line separators in the source otherwise stack up and
    // make the output feel airy.
    let mut squashed: Vec<Line<'static>> = Vec::with_capacity(state.lines.len());
    let mut prev_blank = true; // skip leading blanks
    for line in state.lines.into_iter() {
        let is_blank = is_blank_line(&line);
        if is_blank && prev_blank {
            continue;
        }
        squashed.push(line);
        prev_blank = is_blank;
    }
    while squashed.last().map(is_blank_line).unwrap_or(false) {
        squashed.pop();
    }
    if squashed.is_empty() {
        squashed.push(Line::raw(""));
    }
    squashed
}

fn is_blank_line(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .all(|span| span.content.chars().all(char::is_whitespace))
}

#[derive(Default)]
struct MdRenderer {
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    style_stack: Vec<Style>,
    list_stack: Vec<ListState>,
    link_url_stack: Vec<String>,
    quote_depth: usize,
    code_block: Option<String>,
    table: Option<TableBuf>,
}

#[derive(Default)]
struct TableBuf {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<Vec<Span<'static>>>>,
    current_row: Vec<Vec<Span<'static>>>,
    current_cell: Vec<Span<'static>>,
    has_head: bool,
}

#[derive(Debug, Clone, Copy)]
enum ListKind {
    Ordered(u64),
    Unordered,
}

#[derive(Debug, Clone, Copy)]
struct ListState {
    kind: ListKind,
}

impl MdRenderer {
    fn current_style(&self) -> Style {
        let mut style = Style::default().fg(theme::FG);
        for layer in &self.style_stack {
            style = style.patch(*layer);
        }
        style
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.table.is_some() {
            let style = self.current_style();
            self.table
                .as_mut()
                .unwrap()
                .current_cell
                .push(Span::styled(text.to_owned(), style));
            return;
        }
        if let Some(block) = self.code_block.as_mut() {
            block.push_str(text);
            return;
        }
        let style = self.current_style();
        self.current.push(Span::styled(text.to_owned(), style));
    }

    fn flush_line(&mut self) {
        if !self.current.is_empty() {
            let mut spans = self.prefix_spans();
            spans.append(&mut self.current);
            self.lines.push(Line::from(spans));
        }
    }

    fn break_line(&mut self) {
        if self.current.is_empty() {
            self.lines.push(Line::raw(""));
        } else {
            self.flush_line();
        }
    }

    fn prefix_spans(&self) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        for _ in 0..self.quote_depth {
            spans.push(Span::styled(
                format!("{} ", theme::STRIPE_GLYPH),
                theme::accent(),
            ));
        }
        spans
    }

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(end) => self.end(end),
            Event::Text(text) => self.push_text(text.as_ref()),
            Event::Code(code) => {
                let style = self
                    .current_style()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD);
                self.current.push(Span::styled(format!("`{code}`"), style));
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                self.current
                    .push(Span::styled(html.into_string(), theme::muted()));
            }
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => self.break_line(),
            Event::Rule => {
                self.flush_line();
                self.lines.push(Line::from(Span::styled(
                    "──────────".to_owned(),
                    theme::dim(),
                )));
            }
            Event::FootnoteReference(label) => {
                let style = self.current_style().fg(theme::DIM);
                self.current
                    .push(Span::styled(format!("[^{label}]"), style));
            }
            Event::TaskListMarker(done) => {
                let glyph = if done { "[x] " } else { "[ ] " };
                self.current
                    .push(Span::styled(glyph.to_owned(), theme::accent()));
            }
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                self.push_text(text.as_ref());
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.flush_line();
                self.style_stack.push(
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                );
                let hash = "#".repeat(level as usize);
                self.current
                    .push(Span::styled(format!("{hash} "), theme::accent()));
            }
            Tag::BlockQuote(_) => {
                self.flush_line();
                self.quote_depth += 1;
                self.style_stack.push(Style::default().fg(theme::DIM));
            }
            Tag::CodeBlock(_) => {
                self.flush_line();
                // Fence lines are dropped in favour of a left-stripe
                // styling applied per body row in `TagEnd::CodeBlock`.
                self.code_block = Some(String::new());
                self.lines.push(Line::raw(""));
            }
            Tag::List(start) => {
                self.flush_line();
                let kind = match start {
                    Some(n) => ListKind::Ordered(n),
                    None => ListKind::Unordered,
                };
                self.list_stack.push(ListState { kind });
            }
            Tag::Item => {
                self.flush_line();
                let depth = self.list_stack.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                self.current.push(Span::raw(indent));
                if let Some(state) = self.list_stack.last_mut() {
                    match state.kind {
                        ListKind::Ordered(n) => {
                            self.current
                                .push(Span::styled(format!("{n}. "), theme::accent()));
                            state.kind = ListKind::Ordered(n + 1);
                        }
                        ListKind::Unordered => {
                            self.current.push(Span::styled("• ", theme::accent()));
                        }
                    }
                }
            }
            Tag::Emphasis => {
                self.style_stack
                    .push(Style::default().add_modifier(Modifier::ITALIC));
            }
            Tag::Strong => {
                self.style_stack
                    .push(Style::default().add_modifier(Modifier::BOLD));
            }
            Tag::Strikethrough => {
                self.style_stack
                    .push(Style::default().add_modifier(Modifier::CROSSED_OUT));
            }
            Tag::Link { dest_url, .. } => {
                self.link_url_stack.push(dest_url.into_string());
                self.style_stack.push(
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::UNDERLINED),
                );
            }
            Tag::Image { dest_url, .. } => {
                self.current.push(Span::styled(
                    format!("[image: {}]", dest_url),
                    theme::muted(),
                ));
            }
            Tag::Table(alignments) => {
                self.flush_line();
                self.table = Some(TableBuf {
                    alignments,
                    ..TableBuf::default()
                });
            }
            Tag::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.has_head = true;
                }
            }
            Tag::TableRow | Tag::TableCell => {}
            Tag::FootnoteDefinition(_) => {
                self.style_stack.push(Style::default().fg(theme::DIM));
            }
            Tag::HtmlBlock
            | Tag::MetadataBlock(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition => {}
        }
    }

    fn end(&mut self, end: TagEnd) {
        match end {
            TagEnd::Paragraph => {
                self.flush_line();
                self.lines.push(Line::raw(""));
            }
            TagEnd::Heading(_) => {
                self.flush_line();
                self.style_stack.pop();
                self.lines.push(Line::raw(""));
            }
            TagEnd::BlockQuote(_) => {
                self.flush_line();
                self.style_stack.pop();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                if let Some(body) = self.code_block.take() {
                    let trimmed = body.strip_suffix('\n').unwrap_or(&body);
                    for raw in trimmed.split('\n') {
                        self.lines.push(Line::from(vec![
                            Span::styled(format!("{} ", theme::STRIPE_GLYPH), theme::accent()),
                            Span::styled(raw.to_owned(), Style::default().fg(theme::ACCENT)),
                        ]));
                    }
                    self.lines.push(Line::raw(""));
                }
            }
            TagEnd::List(_) => {
                self.flush_line();
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.lines.push(Line::raw(""));
                }
            }
            TagEnd::Item => {
                self.flush_line();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.style_stack.pop();
            }
            TagEnd::Link => {
                self.style_stack.pop();
                if let Some(url) = self.link_url_stack.pop() {
                    self.current
                        .push(Span::styled(format!(" ({url})"), theme::dim()));
                }
            }
            TagEnd::Image => {}
            TagEnd::Table => {
                if let Some(table) = self.table.take() {
                    self.render_table(table);
                }
            }
            TagEnd::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    let row = std::mem::take(&mut table.current_row);
                    if !row.is_empty() {
                        table.rows.push(row);
                    }
                }
            }
            TagEnd::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    let row = std::mem::take(&mut table.current_row);
                    if !row.is_empty() {
                        table.rows.push(row);
                    }
                }
            }
            TagEnd::TableCell => {
                if let Some(table) = self.table.as_mut() {
                    let cell = std::mem::take(&mut table.current_cell);
                    table.current_row.push(cell);
                }
            }
            TagEnd::FootnoteDefinition => {
                self.style_stack.pop();
            }
            TagEnd::HtmlBlock
            | TagEnd::MetadataBlock(_)
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition => {}
        }
    }

    fn render_table(&mut self, table: TableBuf) {
        let cols = table.rows.iter().map(|row| row.len()).max().unwrap_or(0);
        if cols == 0 {
            return;
        }
        let widths: Vec<usize> = (0..cols)
            .map(|col| {
                table
                    .rows
                    .iter()
                    .map(|row| {
                        row.get(col)
                            .map(|spans| spans_visible_len(spans))
                            .unwrap_or(0)
                    })
                    .max()
                    .unwrap_or(0)
                    .max(3)
            })
            .collect();
        for (row_idx, row) in table.rows.iter().enumerate() {
            let mut spans = Vec::new();
            for (col, width) in widths.iter().enumerate().take(cols) {
                let cell_spans = row.get(col);
                let used = cell_spans.map(|s| spans_visible_len(s)).unwrap_or(0);
                let pad = width.saturating_sub(used);
                let align = table
                    .alignments
                    .get(col)
                    .copied()
                    .unwrap_or(Alignment::Left);
                let (left_pad, right_pad) = match align {
                    Alignment::Left | Alignment::None => (0, pad),
                    Alignment::Right => (pad, 0),
                    Alignment::Center => (pad / 2, pad - pad / 2),
                };
                spans.push(Span::raw(" ".repeat(left_pad)));
                if let Some(cell) = cell_spans {
                    for span in cell {
                        spans.push(span.clone());
                    }
                }
                spans.push(Span::raw(" ".repeat(right_pad)));
                if col + 1 < cols {
                    spans.push(Span::styled(" │ ".to_owned(), theme::dim()));
                }
            }
            self.lines.push(Line::from(spans));
            if row_idx == 0 && table.has_head {
                let divider: String = widths
                    .iter()
                    .map(|w| "─".repeat(*w))
                    .collect::<Vec<_>>()
                    .join("─┼─");
                self.lines
                    .push(Line::from(Span::styled(divider, theme::dim())));
            }
        }
        self.lines.push(Line::raw(""));
    }
}

fn spans_visible_len(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.chars().count()).sum()
}
