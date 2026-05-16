//! Inline slash-command suggestions rendered as part of the composer.
//!
//! This deliberately does not use [`super::palette::PaletteState`]: Ctrl+P is a
//! modal overlay, while `/` is composer-local completion/search.

use ratatui::{
    style::Color,
    text::{Line, Span},
};

use super::{app::App, palette, theme};

const MAX_VISIBLE_ROWS: usize = 5;
const COMMAND_COLUMN_WIDTH: usize = 22;

pub fn is_visible(app: &App) -> bool {
    app.palette.is_none() && query(&app.composer).is_some()
}

pub fn visible_rows(app: &App) -> u16 {
    if !is_visible(app) {
        return 0;
    }
    filtered_commands(app).len().clamp(1, MAX_VISIBLE_ROWS) as u16
}

pub fn reset_selection(app: &mut App) {
    app.slash_selected = 0;
}

pub fn move_selection(app: &mut App, delta: isize) {
    let count = filtered_commands(app).len();
    if count == 0 {
        app.slash_selected = 0;
        return;
    }
    app.slash_selected = app
        .slash_selected
        .saturating_add_signed(delta)
        .min(count - 1);
}

pub fn clamp_selection(app: &mut App) {
    let count = filtered_commands(app).len();
    if count == 0 {
        app.slash_selected = 0;
    } else {
        app.slash_selected = app.slash_selected.min(count - 1);
    }
}

pub fn selected_submission(app: &App, submission: &str) -> Option<String> {
    let command = submission.trim().strip_prefix('/')?.trim();
    if command.is_empty() {
        return selected_command(app).map(|command| format!("/{}", command.id));
    }
    if matches!(command, "help" | "commands" | "?") {
        return None;
    }
    if command.contains(char::is_whitespace) || palette::find_command(command).is_some() {
        return None;
    }
    selected_command(app).map(|command| format!("/{}", command.id))
}

pub fn suggestion_lines(app: &App, visible_rows: usize, bg: Color) -> Vec<Line<'static>> {
    let matches = filtered_commands(app);
    let visible_rows = visible_rows.max(1);
    if matches.is_empty() {
        return vec![Line::from(vec![
            stripe(bg),
            Span::styled("No slash commands match.", theme::dim().bg(bg)),
        ])];
    }

    let selected = app.slash_selected.min(matches.len() - 1);
    let start = selected.saturating_sub(visible_rows.saturating_sub(1));
    let end = (start + visible_rows).min(matches.len());
    matches
        .iter()
        .enumerate()
        .skip(start)
        .take(end - start)
        .map(|(idx, command)| {
            let selected = idx == selected;
            let marker_style = if selected {
                theme::accent().bg(bg)
            } else {
                theme::dim().bg(bg)
            };
            let command_style = if selected {
                theme::accent().bg(bg)
            } else {
                theme::fg().bg(bg)
            };
            Line::from(vec![
                stripe(bg),
                Span::styled(if selected { "> " } else { "  " }, marker_style),
                Span::styled(
                    format!(
                        "{:<width$}",
                        format!("/{}", command.id),
                        width = COMMAND_COLUMN_WIDTH
                    ),
                    command_style,
                ),
                Span::styled(command.hint, theme::muted().bg(bg)),
            ])
        })
        .collect()
}

fn selected_command(app: &App) -> Option<&'static palette::Command> {
    let matches = filtered_commands(app);
    matches
        .get(app.slash_selected.min(matches.len().saturating_sub(1)))
        .copied()
}

fn filtered_commands(app: &App) -> Vec<&'static palette::Command> {
    query(&app.composer)
        .map(palette::filtered)
        .unwrap_or_default()
}

fn query(composer: &str) -> Option<&str> {
    let trimmed = composer.trim_start();
    let rest = trimmed.strip_prefix('/')?;
    Some(rest.lines().next().unwrap_or("").trim_start())
}

fn stripe(bg: Color) -> Span<'static> {
    Span::styled(format!("{} ", theme::STRIPE_GLYPH), theme::accent().bg(bg))
}
