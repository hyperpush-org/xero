//! Composer: textarea-style input with a gold left stripe and an agent
//! footer row inside the same elevated surface.
//!
//! The agent footer (`<agent> · <model> · <provider> · think:<level>`)
//! sits on the last row of the composer block — same surface, same lift.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::{app::App, slash, theme};

const VISIBLE_INPUT_ROWS: usize = 2;
const MAX_INPUT_ROWS: usize = 6;
/// Empty rows inserted above and below the content so text doesn't crowd
/// the composer's top/bottom edges.
const VERTICAL_PAD_ROWS: u16 = 1;

pub fn height(app: &App) -> u16 {
    let typed_rows = app.composer.split('\n').count().max(1);
    let input_rows = typed_rows.clamp(VISIBLE_INPUT_ROWS, MAX_INPUT_ROWS) as u16;
    // top pad + input + slash suggestions + agent footer + bottom pad
    input_rows + slash::visible_rows(app) + 1 + 2 * VERTICAL_PAD_ROWS
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
    let typed_rows = app.composer.split('\n').collect::<Vec<_>>();
    let footer_rows = u16::from(area.height >= 2);
    let top_pad_rows = u16::from(area.height >= 4);
    let bottom_pad_rows = u16::from(area.height >= 5);
    let suggestion_rows = slash::visible_rows(app).min(
        area.height
            .saturating_sub(footer_rows + top_pad_rows + bottom_pad_rows + 1),
    );
    let available_input_rows = area
        .height
        .saturating_sub(footer_rows + top_pad_rows + bottom_pad_rows + suggestion_rows)
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

    for index in 0..visible_rows {
        let mut spans = vec![Span::styled(
            format!("{} ", theme::STRIPE_GLYPH),
            theme::accent().bg(bg),
        )];
        let body_style = theme::fg().bg(bg);
        if index < typed_rows.len() {
            let row = typed_rows[index];
            if index == typed_rows.len() - 1 {
                spans.push(Span::styled(format!("{}\u{2581}", row), body_style));
            } else {
                spans.push(Span::styled(row.to_string(), body_style));
            }
        } else if index == 0 && app.composer.is_empty() {
            spans.push(Span::styled(
                placeholder_for(app).to_string(),
                theme::dim().bg(bg),
            ));
        }
        lines.push(Line::from(spans));
    }

    if suggestion_rows > 0 {
        lines.extend(slash::suggestion_lines(app, suggestion_rows as usize, bg));
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
