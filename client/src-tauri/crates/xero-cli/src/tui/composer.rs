//! Composer: textarea-style input with a gold left stripe and an agent
//! footer row inside the same elevated surface.
//!
//! The agent footer (`<agent> · <model> · <provider> · think:<level>`)
//! sits on the last row of the composer block — same surface, same lift.

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::{app::App, slash, theme};

const VISIBLE_INPUT_ROWS: usize = 2;
const MAX_INPUT_ROWS: usize = 6;
const INPUT_FOOTER_GAP_ROWS: u16 = 1;
/// Empty rows inserted above and below the content so text doesn't crowd
/// the composer's top/bottom edges.
const VERTICAL_PAD_ROWS: u16 = 1;

pub fn height(app: &App) -> u16 {
    let typed_rows = input_rows(&app.composer).len();
    let input_rows = typed_rows.clamp(VISIBLE_INPUT_ROWS, MAX_INPUT_ROWS) as u16;
    // top pad + input + slash suggestions + input/footer gap + agent footer + bottom pad
    input_rows + slash::visible_rows(app) + INPUT_FOOTER_GAP_ROWS + 1 + 2 * VERTICAL_PAD_ROWS
}

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    // Shrink the elevated surface by one column on the left so the gold
    // stripe sits flush against the surface — the column to its left
    // stays at the base bg.
    if area.width <= 1 || area.height == 0 {
        return;
    }
    let surface_area = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width - 1,
        height: area.height,
    };

    // Paint the elevated surface first so empty cells inside read as a
    // single block.
    let surface = Paragraph::new("").style(theme::composer_bg());
    frame.render_widget(surface, surface_area);

    let bg = theme::composer_bg_color();
    let typed_rows = input_rows(&app.composer);
    let cursor = app.composer_cursor();
    let cursor_row = cursor_row_index(&typed_rows, cursor);
    let footer_rows = u16::from(area.height >= 2);
    let top_pad_rows = u16::from(area.height >= 4);
    let bottom_pad_rows = u16::from(area.height >= 5);
    let input_footer_gap_rows = u16::from(
        footer_rows > 0
            && area.height
                >= footer_rows + top_pad_rows + bottom_pad_rows + INPUT_FOOTER_GAP_ROWS + 1,
    );
    let suggestion_rows =
        slash::visible_rows(app).min(area.height.saturating_sub(
            footer_rows + top_pad_rows + bottom_pad_rows + input_footer_gap_rows + 1,
        ));
    let available_input_rows = area
        .height
        .saturating_sub(
            footer_rows + top_pad_rows + bottom_pad_rows + suggestion_rows + input_footer_gap_rows,
        )
        .max(1) as usize;
    let visible_rows = typed_rows
        .len()
        .clamp(VISIBLE_INPUT_ROWS, MAX_INPUT_ROWS)
        .min(available_input_rows);
    let mut lines = Vec::with_capacity(visible_rows + 3);

    // Top padding — stripe-only row(s) so the gold mark spans the whole
    // elevated surface, not just the content rows.
    for _ in 0..top_pad_rows {
        lines.push(stripe_only_line(bg));
    }

    let first_visible_row = cursor_row
        .saturating_add(1)
        .saturating_sub(visible_rows)
        .min(typed_rows.len().saturating_sub(visible_rows));
    for visible_index in 0..visible_rows {
        let index = first_visible_row + visible_index;
        let mut spans = vec![Span::styled(
            format!("{} ", theme::STRIPE_GLYPH),
            theme::accent().bg(bg),
        )];
        let body_style = theme::fg().bg(bg);
        if index < typed_rows.len() {
            let row = typed_rows[index];
            if app.composer.is_empty() {
                spans.push(Span::styled(
                    placeholder_for(app).to_string(),
                    theme::dim().bg(bg),
                ));
            } else if index == cursor_row {
                push_cursor_row(&mut spans, row, cursor, body_style);
            } else {
                spans.push(Span::styled(row.text.to_string(), body_style));
            }
        }
        lines.push(Line::from(spans));
    }

    if suggestion_rows > 0 {
        lines.extend(slash::suggestion_lines(app, suggestion_rows as usize, bg));
    }

    for _ in 0..input_footer_gap_rows {
        lines.push(stripe_only_line(bg));
    }

    if footer_rows > 0 {
        lines.push(agent_footer_line(app));
    }

    // Bottom padding.
    for _ in 0..bottom_pad_rows {
        lines.push(stripe_only_line(bg));
    }

    let paragraph = Paragraph::new(lines)
        .style(theme::composer_bg())
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, surface_area);
}

#[derive(Clone, Copy)]
struct InputRow<'a> {
    start: usize,
    end: usize,
    text: &'a str,
}

fn input_rows(input: &str) -> Vec<InputRow<'_>> {
    if input.is_empty() {
        return vec![InputRow {
            start: 0,
            end: 0,
            text: "",
        }];
    }

    let mut rows = Vec::new();
    let mut start = 0;
    for segment in input.split_inclusive('\n') {
        let content_len = segment
            .strip_suffix('\n')
            .map(str::len)
            .unwrap_or(segment.len());
        let end = start + content_len;
        rows.push(InputRow {
            start,
            end,
            text: &input[start..end],
        });
        start += segment.len();
    }
    if input.ends_with('\n') {
        rows.push(InputRow {
            start: input.len(),
            end: input.len(),
            text: "",
        });
    }
    rows
}

fn cursor_row_index(rows: &[InputRow<'_>], cursor: usize) -> usize {
    rows.iter()
        .position(|row| cursor >= row.start && cursor <= row.end)
        .unwrap_or_else(|| rows.len().saturating_sub(1))
}

fn push_cursor_row(spans: &mut Vec<Span<'static>>, row: InputRow<'_>, cursor: usize, style: Style) {
    let cursor = cursor.clamp(row.start, row.end);
    let split_at = cursor - row.start;
    if split_at > 0 {
        spans.push(Span::styled(row.text[..split_at].to_string(), style));
    }
    if split_at < row.text.len() {
        let cursor_char_end = row.text[split_at..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| split_at + index)
            .unwrap_or(row.text.len());
        spans.push(Span::styled(
            row.text[split_at..cursor_char_end].to_string(),
            cursor_style(),
        ));
        if cursor_char_end < row.text.len() {
            spans.push(Span::styled(row.text[cursor_char_end..].to_string(), style));
        }
    } else {
        spans.push(Span::styled(" ", cursor_style()));
    }
}

fn cursor_style() -> Style {
    Style::default()
        .fg(theme::composer_bg_color())
        .bg(theme::FG)
}

fn stripe_only_line(bg: ratatui::style::Color) -> Line<'static> {
    Line::from(Span::styled(
        format!("{} ", theme::STRIPE_GLYPH),
        theme::accent().bg(bg),
    ))
}

fn placeholder_for(app: &App) -> &'static str {
    if app.run_detail.is_some() {
        "Ask anything..."
    } else {
        "Ask anything... \"Fix broken tests\""
    }
}

fn agent_footer_line(app: &App) -> Line<'static> {
    let bg = theme::composer_bg_color();
    let agent = app.selected_agent_label().to_owned();
    let model = app
        .selected_model_id()
        .map(str::to_owned)
        .unwrap_or_else(|| "no-model".into());
    let profile = if app.fake_provider_fixture {
        "fake_provider".to_owned()
    } else {
        app.selected_provider_id()
            .map(str::to_owned)
            .unwrap_or_else(|| "no-provider".into())
    };
    let profile_style = if app.selected_is_paid_tier() {
        theme::paid().bg(bg)
    } else {
        theme::muted().bg(bg)
    };
    let effort = format!("think:{}", app.thinking_effort.label());

    Line::from(vec![
        Span::styled(format!("{} ", theme::STRIPE_GLYPH), theme::accent().bg(bg)),
        Span::styled(agent, theme::accent().bg(bg)),
        Span::styled(" · ", theme::dim().bg(bg)),
        Span::styled(model, theme::muted().bg(bg)),
        Span::styled(" · ", theme::dim().bg(bg)),
        Span::styled(profile, profile_style),
        Span::styled(" · ", theme::dim().bg(bg)),
        Span::styled(effort, theme::muted().bg(bg)),
    ])
}
