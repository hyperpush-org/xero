//! Small end-of-input editing primitives shared by TUI text fields.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextEdit {
    Backspace,
    Delete,
    DeletePreviousWord,
    DeleteToLineStart,
    Insert(char),
    MoveLeft,
    MoveRight,
    MovePreviousWord,
    MoveNextWord,
    MoveUp,
    MoveDown,
    MoveToLineStart,
    MoveToLineEnd,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EditOutcome {
    pub changed: bool,
    pub text_changed: bool,
}

pub(super) fn edit_for_key(key: KeyEvent) -> TextEdit {
    match key.code {
        KeyCode::Left if has_command_modifier(key.modifiers) => TextEdit::MoveToLineStart,
        KeyCode::Right if has_command_modifier(key.modifiers) => TextEdit::MoveToLineEnd,
        KeyCode::Left if has_option_modifier(key.modifiers) => TextEdit::MovePreviousWord,
        KeyCode::Right if has_option_modifier(key.modifiers) => TextEdit::MoveNextWord,
        KeyCode::Left => TextEdit::MoveLeft,
        KeyCode::Right => TextEdit::MoveRight,
        KeyCode::Up => TextEdit::MoveUp,
        KeyCode::Down => TextEdit::MoveDown,
        KeyCode::Home => TextEdit::MoveToLineStart,
        KeyCode::End => TextEdit::MoveToLineEnd,
        KeyCode::Backspace if has_command_modifier(key.modifiers) => TextEdit::DeleteToLineStart,
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::ALT) => {
            TextEdit::DeletePreviousWord
        }
        KeyCode::Backspace => TextEdit::Backspace,
        KeyCode::Delete => TextEdit::Delete,
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            TextEdit::DeleteToLineStart
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            TextEdit::DeletePreviousWord
        }
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            TextEdit::MoveToLineStart
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            TextEdit::MoveToLineEnd
        }
        KeyCode::Char('b') if has_option_modifier(key.modifiers) => TextEdit::MovePreviousWord,
        KeyCode::Char('f') if has_option_modifier(key.modifiers) => TextEdit::MoveNextWord,
        KeyCode::Char(ch) if inserts_text(key.modifiers) => TextEdit::Insert(ch),
        KeyCode::Char(_) => TextEdit::Ignore,
        _ => TextEdit::Ignore,
    }
}

#[cfg(test)]
fn apply_edit(input: &mut String, edit: TextEdit) -> bool {
    let mut cursor = input.len();
    let mut desired_column = None;
    apply_edit_at_cursor(input, &mut cursor, &mut desired_column, edit).changed
}

pub(super) fn apply_edit_at_cursor(
    input: &mut String,
    cursor: &mut usize,
    desired_column: &mut Option<usize>,
    edit: TextEdit,
) -> EditOutcome {
    *cursor = clamped_cursor(input, *cursor);
    match edit {
        TextEdit::Backspace => {
            *desired_column = None;
            let Some(start) = previous_char_boundary(input, *cursor) else {
                return unchanged();
            };
            input.drain(start..*cursor);
            *cursor = start;
            changed_text()
        }
        TextEdit::Delete => {
            *desired_column = None;
            let Some(end) = next_char_boundary(input, *cursor) else {
                return unchanged();
            };
            input.drain(*cursor..end);
            changed_text()
        }
        TextEdit::DeletePreviousWord => {
            *desired_column = None;
            let previous = input.len();
            let start = previous_word_start(input, *cursor);
            input.drain(start..*cursor);
            *cursor = start;
            outcome(input.len() != previous, input.len() != previous)
        }
        TextEdit::DeleteToLineStart => {
            *desired_column = None;
            let previous = input.len();
            let start = line_start(input, *cursor);
            input.drain(start..*cursor);
            *cursor = start;
            outcome(input.len() != previous, input.len() != previous)
        }
        TextEdit::Insert(ch) => {
            *desired_column = None;
            input.insert(*cursor, ch);
            *cursor += ch.len_utf8();
            changed_text()
        }
        TextEdit::MoveLeft => {
            *desired_column = None;
            let Some(next) = previous_char_boundary(input, *cursor) else {
                return unchanged();
            };
            *cursor = next;
            changed_cursor()
        }
        TextEdit::MoveRight => {
            *desired_column = None;
            let Some(next) = next_char_boundary(input, *cursor) else {
                return unchanged();
            };
            *cursor = next;
            changed_cursor()
        }
        TextEdit::MovePreviousWord => {
            *desired_column = None;
            let next = previous_word_start(input, *cursor);
            if next == *cursor {
                return unchanged();
            }
            *cursor = next;
            changed_cursor()
        }
        TextEdit::MoveNextWord => {
            *desired_column = None;
            let next = next_word_end(input, *cursor);
            if next == *cursor {
                return unchanged();
            }
            *cursor = next;
            changed_cursor()
        }
        TextEdit::MoveUp => {
            let column = desired_column.unwrap_or_else(|| column_at_cursor(input, *cursor));
            let next = vertical_cursor(input, *cursor, column, VerticalDirection::Up);
            if next == *cursor {
                return unchanged();
            }
            *cursor = next;
            *desired_column = Some(column);
            changed_cursor()
        }
        TextEdit::MoveDown => {
            let column = desired_column.unwrap_or_else(|| column_at_cursor(input, *cursor));
            let next = vertical_cursor(input, *cursor, column, VerticalDirection::Down);
            if next == *cursor {
                return unchanged();
            }
            *cursor = next;
            *desired_column = Some(column);
            changed_cursor()
        }
        TextEdit::MoveToLineStart => {
            *desired_column = None;
            let next = line_start(input, *cursor);
            if next == *cursor {
                return unchanged();
            }
            *cursor = next;
            changed_cursor()
        }
        TextEdit::MoveToLineEnd => {
            *desired_column = None;
            let next = line_end(input, *cursor);
            if next == *cursor {
                return unchanged();
            }
            *cursor = next;
            changed_cursor()
        }
        TextEdit::Ignore => unchanged(),
    }
}

pub(super) fn clamped_cursor(input: &str, cursor: usize) -> usize {
    let cursor = cursor.min(input.len());
    if input.is_char_boundary(cursor) {
        return cursor;
    }
    input
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index < cursor)
        .last()
        .unwrap_or(0)
}

fn has_command_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::SUPER | KeyModifiers::META)
}

fn has_option_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::ALT)
}

fn inserts_text(modifiers: KeyModifiers) -> bool {
    !modifiers.intersects(
        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META,
    )
}

fn previous_word_start(input: &str, cursor: usize) -> usize {
    let cursor = clamped_cursor(input, cursor);
    let end = input[..cursor].trim_end_matches(char::is_whitespace).len();
    if end == 0 {
        return 0;
    }

    let Some(last_char) = input[..end].chars().next_back() else {
        return 0;
    };
    let delete_word = is_word_char(last_char);
    let mut start = end;
    for (index, ch) in input[..end].char_indices().rev() {
        let same_class = if delete_word {
            is_word_char(ch)
        } else {
            !ch.is_whitespace() && !is_word_char(ch)
        };
        if !same_class {
            break;
        }
        start = index;
    }
    start
}

fn next_word_end(input: &str, cursor: usize) -> usize {
    let mut start = clamped_cursor(input, cursor);
    while let Some((ch, next)) = char_at(input, start) {
        if !ch.is_whitespace() {
            break;
        }
        start = next;
    }

    let Some((first_char, _)) = char_at(input, start) else {
        return input.len();
    };
    let move_word = is_word_char(first_char);
    let mut end = start;
    while let Some((ch, next)) = char_at(input, end) {
        let same_class = if move_word {
            is_word_char(ch)
        } else {
            !ch.is_whitespace() && !is_word_char(ch)
        };
        if !same_class {
            break;
        }
        end = next;
    }
    end
}

fn char_at(input: &str, cursor: usize) -> Option<(char, usize)> {
    let cursor = clamped_cursor(input, cursor);
    let ch = input[cursor..].chars().next()?;
    Some((ch, cursor + ch.len_utf8()))
}

fn previous_char_boundary(input: &str, cursor: usize) -> Option<usize> {
    let cursor = clamped_cursor(input, cursor);
    if cursor == 0 {
        return None;
    }
    input[..cursor]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
}

fn next_char_boundary(input: &str, cursor: usize) -> Option<usize> {
    let cursor = clamped_cursor(input, cursor);
    if cursor == input.len() {
        return None;
    }
    let mut chars = input[cursor..].char_indices();
    let _ = chars.next()?;
    Some(
        chars
            .next()
            .map(|(index, _)| cursor + index)
            .unwrap_or(input.len()),
    )
}

fn line_start(input: &str, cursor: usize) -> usize {
    let cursor = clamped_cursor(input, cursor);
    input[..cursor]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn line_end(input: &str, cursor: usize) -> usize {
    let cursor = clamped_cursor(input, cursor);
    input[cursor..]
        .find('\n')
        .map(|index| cursor + index)
        .unwrap_or(input.len())
}

fn column_at_cursor(input: &str, cursor: usize) -> usize {
    let cursor = clamped_cursor(input, cursor);
    input[line_start(input, cursor)..cursor].chars().count()
}

fn cursor_for_column(input: &str, start: usize, end: usize, column: usize) -> usize {
    input[start..end]
        .char_indices()
        .map(|(offset, _)| start + offset)
        .nth(column)
        .unwrap_or(end)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerticalDirection {
    Up,
    Down,
}

fn vertical_cursor(
    input: &str,
    cursor: usize,
    column: usize,
    direction: VerticalDirection,
) -> usize {
    let cursor = clamped_cursor(input, cursor);
    let current_start = line_start(input, cursor);
    match direction {
        VerticalDirection::Up if current_start == 0 => cursor,
        VerticalDirection::Up => {
            let previous_end = current_start - 1;
            let previous_start = line_start(input, previous_end);
            cursor_for_column(input, previous_start, previous_end, column)
        }
        VerticalDirection::Down => {
            let current_end = line_end(input, cursor);
            if current_end == input.len() {
                return cursor;
            }
            let next_start = current_end + 1;
            let next_end = line_end(input, next_start);
            cursor_for_column(input, next_start, next_end, column)
        }
    }
}

fn outcome(changed: bool, text_changed: bool) -> EditOutcome {
    EditOutcome {
        changed,
        text_changed,
    }
}

fn changed_text() -> EditOutcome {
    outcome(true, true)
}

fn changed_cursor() -> EditOutcome {
    outcome(true, false)
}

fn unchanged() -> EditOutcome {
    outcome(false, false)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_backspace_deletes_previous_word() {
        let mut input = "run tests now".to_owned();

        apply_edit(&mut input, TextEdit::DeletePreviousWord);

        assert_eq!(input, "run tests ");
    }

    #[test]
    fn previous_word_delete_also_removes_trailing_space() {
        let mut input = "run tests now   ".to_owned();

        apply_edit(&mut input, TextEdit::DeletePreviousWord);

        assert_eq!(input, "run tests ");
    }

    #[test]
    fn previous_word_delete_stops_at_punctuation_boundary() {
        let mut input = "open src/main.rs".to_owned();

        apply_edit(&mut input, TextEdit::DeletePreviousWord);

        assert_eq!(input, "open src/main.");
    }

    #[test]
    fn command_backspace_deletes_to_current_line_start() {
        let mut input = "first line\nsecond line".to_owned();

        apply_edit(&mut input, TextEdit::DeleteToLineStart);

        assert_eq!(input, "first line\n");
    }

    #[test]
    fn command_backspace_clears_single_line_input() {
        let mut input = "single line".to_owned();

        apply_edit(&mut input, TextEdit::DeleteToLineStart);

        assert!(input.is_empty());
    }

    #[test]
    fn left_arrow_then_insert_edits_at_cursor() {
        let mut input = "helo".to_owned();
        let mut cursor = input.len();
        let mut desired_column = None;

        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::MoveLeft,
        );
        let outcome = apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::Insert('l'),
        );

        assert!(outcome.text_changed);
        assert_eq!(input, "hello");
        assert_eq!(cursor, "hell".len());
    }

    #[test]
    fn backspace_deletes_character_before_cursor() {
        let mut input = "abc".to_owned();
        let mut cursor = 2;
        let mut desired_column = None;

        let outcome = apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::Backspace,
        );

        assert!(outcome.text_changed);
        assert_eq!(input, "ac");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn vertical_arrows_preserve_column_across_short_rows() {
        let mut input = "abcdef\ngh\nijklmn".to_owned();
        let mut cursor = "abcdef".len();
        let mut desired_column = None;

        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::MoveDown,
        );
        assert_eq!(cursor, "abcdef\ngh".len());

        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::MoveDown,
        );
        assert_eq!(cursor, "abcdef\ngh\nijklmn".len());
    }

    #[test]
    fn option_arrows_move_by_word() {
        let mut input = "run tests now".to_owned();
        let mut cursor = input.len();
        let mut desired_column = None;

        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::MovePreviousWord,
        );
        assert_eq!(cursor, "run tests ".len());

        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::MovePreviousWord,
        );
        assert_eq!(cursor, "run ".len());

        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::MoveNextWord,
        );
        assert_eq!(cursor, "run tests".len());
    }

    #[test]
    fn command_arrows_move_to_line_boundaries() {
        let mut input = "first line\nsecond line\nthird".to_owned();
        let mut cursor = "first line\nsecond".len();
        let mut desired_column = None;

        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::MoveToLineStart,
        );
        assert_eq!(cursor, "first line\n".len());

        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::MoveToLineEnd,
        );
        assert_eq!(cursor, "first line\nsecond line".len());
    }

    #[test]
    fn cursor_movement_respects_utf8_boundaries() {
        let mut input = "aé🙂b".to_owned();
        let mut cursor = input.len();
        let mut desired_column = None;

        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::MoveLeft,
        );
        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::MoveLeft,
        );
        apply_edit_at_cursor(
            &mut input,
            &mut cursor,
            &mut desired_column,
            TextEdit::Insert('x'),
        );

        assert_eq!(input, "aéx🙂b");
    }

    #[test]
    fn control_fallbacks_map_to_text_edits() {
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            TextEdit::DeletePreviousWord
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            TextEdit::DeleteToLineStart
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            TextEdit::MoveToLineStart
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            TextEdit::MoveToLineEnd
        );
    }

    #[test]
    fn mac_modifiers_map_to_backspace_edits() {
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT)),
            TextEdit::DeletePreviousWord
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::SUPER)),
            TextEdit::DeleteToLineStart
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::META)),
            TextEdit::DeleteToLineStart
        );
    }

    #[test]
    fn mac_modifiers_map_to_arrow_shortcuts() {
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT)),
            TextEdit::MovePreviousWord
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT)),
            TextEdit::MoveNextWord
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT)),
            TextEdit::MovePreviousWord
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT)),
            TextEdit::MoveNextWord
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SUPER)),
            TextEdit::MoveToLineStart
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::META)),
            TextEdit::MoveToLineEnd
        );
    }

    #[test]
    fn modified_characters_are_not_inserted_as_text() {
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT)),
            TextEdit::Ignore
        );
        assert_eq!(
            edit_for_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::SUPER)),
            TextEdit::Ignore
        );
    }
}
