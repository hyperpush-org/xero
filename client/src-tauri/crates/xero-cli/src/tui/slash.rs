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
const GROUP_HINT: &str = "subcommands";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SuggestionKind {
    Command,
    Group,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SlashSuggestion {
    id: &'static str,
    hint: &'static str,
    kind: SuggestionKind,
}

pub enum SelectedAction {
    Submit(String),
    Complete(String),
}

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

#[cfg(test)]
pub fn selected_submission(app: &App, submission: &str) -> Option<String> {
    match selected_action(app, submission)? {
        SelectedAction::Submit(submission) => Some(submission),
        SelectedAction::Complete(_) => None,
    }
}

pub fn selected_action(app: &App, submission: &str) -> Option<SelectedAction> {
    let raw_query = submission_query(submission)?;
    let command = raw_query.trim();
    let selected = selected_suggestion(app)?;
    if command.is_empty() {
        return Some(action_for_selected(selected));
    }
    if matches!(command, "help" | "commands" | "?") {
        return None;
    }
    let is_single_exact_executable =
        !raw_query.contains(char::is_whitespace) && is_executable_root(command);
    if selected.kind == SuggestionKind::Group {
        return Some(SelectedAction::Complete(format!("/{} ", selected.id)));
    }
    if selected.id == command || is_single_exact_executable {
        return None;
    }
    Some(SelectedAction::Submit(format!("/{}", selected.id)))
}

pub fn complete_selection(app: &mut App) -> bool {
    let Some(suggestion) = selected_suggestion(app) else {
        return false;
    };
    let completion = match suggestion.kind {
        SuggestionKind::Command => format!("/{}", suggestion.id),
        SuggestionKind::Group => format!("/{} ", suggestion.id),
    };
    app.replace_composer(completion);
    reset_selection(app);
    true
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

fn selected_suggestion(app: &App) -> Option<SlashSuggestion> {
    let matches = filtered_commands(app);
    matches
        .get(app.slash_selected.min(matches.len().saturating_sub(1)))
        .copied()
}

fn filtered_commands(app: &App) -> Vec<SlashSuggestion> {
    query(&app.composer)
        .map(filtered_suggestions)
        .unwrap_or_default()
}

fn filtered_suggestions(input: &str) -> Vec<SlashSuggestion> {
    let input = input.trim_start();
    if let Some((root, child_input)) = subcommand_context(input) {
        return child_suggestions(root, child_input);
    }
    top_level_suggestions()
        .into_iter()
        .filter(|suggestion| suggestion_matches(suggestion, input))
        .collect()
}

fn subcommand_context(input: &str) -> Option<(&str, &str)> {
    if input.is_empty() {
        return None;
    }
    if let Some(split_at) = input.find(char::is_whitespace) {
        let root = &input[..split_at];
        if has_subcommands(root) {
            return Some((root, input[split_at..].trim_start()));
        }
        return None;
    }
    has_subcommands(input).then_some((input, ""))
}

fn top_level_suggestions() -> Vec<SlashSuggestion> {
    let mut suggestions = Vec::new();
    let mut roots = Vec::new();
    for command in palette::commands() {
        if let Some((root, _)) = command.id.split_once(' ') {
            if palette::find_command(root).is_none() && !roots.contains(&root) {
                roots.push(root);
                suggestions.push(SlashSuggestion {
                    id: root,
                    hint: GROUP_HINT,
                    kind: SuggestionKind::Group,
                });
            }
            continue;
        }
        suggestions.push(SlashSuggestion {
            id: command.id,
            hint: command.hint,
            kind: SuggestionKind::Command,
        });
    }
    suggestions
}

fn child_suggestions(root: &str, child_input: &str) -> Vec<SlashSuggestion> {
    let prefix = format!("{root} ");
    palette::commands()
        .iter()
        .filter_map(|command| {
            let child = command.id.strip_prefix(&prefix)?;
            let suggestion = SlashSuggestion {
                id: command.id,
                hint: command.hint,
                kind: SuggestionKind::Command,
            };
            suggestion_matches_child(suggestion, child, child_input).then_some(suggestion)
        })
        .collect()
}

fn has_subcommands(root: &str) -> bool {
    let prefix = format!("{root} ");
    palette::commands()
        .iter()
        .any(|command| command.id.starts_with(&prefix))
}

fn suggestion_matches(suggestion: &SlashSuggestion, input: &str) -> bool {
    if input.trim().is_empty() {
        return true;
    }
    let needle = input.to_lowercase();
    suggestion.id.to_lowercase().starts_with(&needle)
}

fn suggestion_matches_child(suggestion: SlashSuggestion, child: &str, input: &str) -> bool {
    if input.trim().is_empty() {
        return true;
    }
    let needle = input.to_lowercase();
    child.to_lowercase().starts_with(&needle) || suggestion.id.to_lowercase().starts_with(&needle)
}

fn action_for_selected(suggestion: SlashSuggestion) -> SelectedAction {
    match suggestion.kind {
        SuggestionKind::Command => SelectedAction::Submit(format!("/{}", suggestion.id)),
        SuggestionKind::Group => SelectedAction::Complete(format!("/{} ", suggestion.id)),
    }
}

fn is_executable_root(command: &str) -> bool {
    palette::find_command(command).is_some() || super::app::slash_dialog_alias(command).is_some()
}

fn query(composer: &str) -> Option<&str> {
    let trimmed = composer.trim_start();
    let rest = trimmed.strip_prefix('/')?;
    Some(rest.lines().next().unwrap_or("").trim_start())
}

fn submission_query(submission: &str) -> Option<&str> {
    let trimmed = submission.trim_start();
    let rest = trimmed.strip_prefix('/')?;
    Some(rest.lines().next().unwrap_or("").trim_start())
}

fn stripe(bg: Color) -> Span<'static> {
    Span::styled(format!("{} ", theme::STRIPE_GLYPH), theme::accent().bg(bg))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(input: &str) -> Vec<&'static str> {
        filtered_suggestions(input)
            .into_iter()
            .map(|suggestion| suggestion.id)
            .collect()
    }

    #[test]
    fn root_suggestions_collapse_subcommands() {
        let ids = ids("");

        assert!(ids.contains(&"project"), "missing project parent");
        assert!(
            ids.contains(&"providers"),
            "missing top-level providers command"
        );
        assert!(
            !ids.contains(&"project list"),
            "root slash suggestions should hide project subcommands"
        );
        assert!(
            !ids.contains(&"provider login"),
            "root slash suggestions should hide provider subcommands"
        );
    }

    #[test]
    fn partial_parent_filter_keeps_subcommands_hidden() {
        let ids = ids("proj");

        assert!(ids.contains(&"project"));
        assert!(ids.contains(&"project-state"));
        assert!(
            !ids.contains(&"project list"),
            "partial parent should not reveal leaf subcommands"
        );
    }

    #[test]
    fn exact_parent_reveals_subcommands() {
        let ids = ids("project");

        assert!(ids.contains(&"project list"));
        assert!(ids.contains(&"project import"));
        assert!(
            !ids.contains(&"project"),
            "parent row should give way to children"
        );
    }

    #[test]
    fn parent_with_partial_child_filters_children() {
        let ids = ids("project l");

        assert_eq!(ids, vec!["project list"]);
    }

    #[test]
    fn parent_completion_keeps_composer_open_for_children() {
        let mut app = super::super::app::test_only_empty_app();
        app.replace_composer("/proj");

        assert!(complete_selection(&mut app));
        assert_eq!(app.composer, "/project ");
        assert_eq!(app.composer_cursor, "/project ".len());

        match selected_action(&app, "/project ") {
            Some(SelectedAction::Submit(command)) => assert_eq!(command, "/project list"),
            _ => panic!("expected parent context to submit the selected child"),
        }
    }
}
