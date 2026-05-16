//! Footer: a single row with cwd:branch on the left and ctrl+p hint (or
//! token/approval hint) on the right. No border, no separator.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::{app::App, theme};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.width < 4 {
        return;
    }
    let left = left_segment(app);
    let right = right_segment(app);

    let total_width = area.width as usize;
    let left_width = visible_width(&left);
    let right_width = visible_width(&right);
    let gap = total_width
        .saturating_sub(left_width)
        .saturating_sub(right_width);
    let mut spans = Vec::with_capacity(left.len() + right.len() + 1);
    spans.extend(left);
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);
    let paragraph = Paragraph::new(Line::from(spans)).style(theme::base());
    frame.render_widget(paragraph, area);
}

fn left_segment(app: &App) -> Vec<Span<'static>> {
    let path = app.project.display_path.clone();
    let branch = app.project.branch.clone();
    let mut spans = vec![Span::raw(" "), Span::styled(path, theme::muted())];
    if let Some(branch) = branch {
        spans.push(Span::styled(":", theme::dim()));
        spans.push(Span::styled(branch, theme::muted()));
    }
    if let Some(status) = app.status.clone() {
        spans.push(Span::styled("  ", theme::dim()));
        spans.push(Span::styled(status, theme::dim()));
    }
    spans
}

fn right_segment(app: &App) -> Vec<Span<'static>> {
    let hint_label = if app.run_detail.is_some() {
        "ctrl+p /commands"
    } else {
        "tab agents   ctrl+p /commands"
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    if app.run_detail.is_none() {
        spans.push(Span::styled(hint_label.to_owned(), theme::dim()));
        spans.push(Span::styled("   ", theme::dim()));
        spans.push(Span::styled(VERSION.to_owned(), theme::dim()));
    } else {
        if let Some((tokens_label, percent_label)) = token_segment(app) {
            spans.push(Span::styled(tokens_label, theme::dim()));
            if let Some(percent_label) = percent_label {
                spans.push(Span::styled(" ", theme::dim()));
                spans.push(Span::styled(percent_label, theme::dim()));
            }
            spans.push(Span::styled("   ", theme::dim()));
        }
        spans.push(Span::styled(hint_label.to_owned(), theme::dim()));
    }
    spans.push(Span::raw(" "));
    spans
}

fn token_segment(app: &App) -> Option<(String, Option<String>)> {
    let detail = app.run_detail.as_ref()?;
    let tokens = detail.tokens_used?;
    let percent = detail.context_window.and_then(|window| {
        if window == 0 {
            None
        } else {
            let pct = (tokens as f64 / window as f64) * 100.0;
            Some(format!("({}%)", pct.round() as u64))
        }
    });
    Some((format_tokens(tokens), percent))
}

fn format_tokens(value: u64) -> String {
    if value < 1_000 {
        format!("{value}")
    } else if value < 1_000_000 {
        let kilo = value as f64 / 1_000.0;
        if kilo >= 100.0 {
            format!("{:.0}K", kilo)
        } else {
            format!("{:.1}K", kilo)
        }
    } else {
        let mega = value as f64 / 1_000_000.0;
        format!("{:.1}M", mega)
    }
}

fn visible_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(|span| span.content.chars().count()).sum()
}
