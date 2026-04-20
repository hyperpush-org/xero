use crate::commands::{CommandError, CommandResult};

pub(super) fn decode_utf8_body(
    body: &[u8],
    was_truncated: bool,
    error_code: &'static str,
    message: &'static str,
) -> CommandResult<String> {
    match std::str::from_utf8(body) {
        Ok(value) => Ok(value.to_string()),
        Err(error) if was_truncated && error.valid_up_to() > 0 => {
            Ok(std::str::from_utf8(&body[..error.valid_up_to()])
                .unwrap_or_default()
                .to_string())
        }
        Err(_) => Err(CommandError::user_fixable(error_code, message)),
    }
}

pub(super) fn truncate_chars_with_flag(value: &str, max_chars: usize) -> (String, bool) {
    if max_chars == 0 {
        return (String::new(), !value.is_empty());
    }

    let truncated = value.chars().count() > max_chars;
    let truncated_value = value.chars().take(max_chars).collect::<String>();
    (truncated_value, truncated)
}

pub(super) fn normalize_extracted_text(value: &str) -> String {
    let mut normalized_lines = Vec::new();
    let mut previous_blank = false;

    for raw_line in value.lines() {
        let line = raw_line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            if !previous_blank && !normalized_lines.is_empty() {
                normalized_lines.push(String::new());
            }
            previous_blank = true;
            continue;
        }

        previous_blank = false;
        normalized_lines.push(line);
    }

    normalized_lines.join("\n").trim().to_string()
}

pub(super) fn extract_html_title(html: &str) -> Option<String> {
    let lowercase = html.to_ascii_lowercase();
    let title_start = lowercase.find("<title")?;
    let after_open = html[title_start..].find('>')? + title_start + 1;
    let title_end = lowercase[after_open..].find("</title>")? + after_open;
    let title = decode_html_entities(&html[after_open..title_end]);
    let title = normalize_extracted_text(&title);
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

pub(super) fn extract_html_text(html: &str) -> String {
    let lowercase = html.to_ascii_lowercase();
    let mut cursor = 0;
    let mut extracted = String::new();

    while let Some(relative_lt) = html[cursor..].find('<') {
        let lt = cursor + relative_lt;
        extracted.push_str(&decode_html_entities(&html[cursor..lt]));

        let Some(relative_gt) = html[lt..].find('>') else {
            cursor = lt;
            break;
        };
        let gt = lt + relative_gt;
        let raw_tag = html[lt + 1..gt].trim();
        let tag_name = raw_tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();

        if tag_name == "script" || tag_name == "style" {
            let closing_tag = format!("</{tag_name}>");
            if let Some(relative_end) = lowercase[gt + 1..].find(&closing_tag) {
                cursor = gt + 1 + relative_end + closing_tag.len();
                continue;
            }

            break;
        }

        if matches!(
            tag_name.as_str(),
            "br" | "p"
                | "div"
                | "li"
                | "tr"
                | "td"
                | "th"
                | "section"
                | "article"
                | "header"
                | "footer"
                | "main"
                | "aside"
                | "nav"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "title"
        ) {
            extracted.push('\n');
        }

        cursor = gt + 1;
    }

    if cursor < html.len() {
        extracted.push_str(&decode_html_entities(&html[cursor..]));
    }

    normalize_extracted_text(&extracted)
}

pub(super) fn decode_html_entities(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    let mut decoded = String::with_capacity(value.len());
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '&' {
            decoded.push(chars[index]);
            index += 1;
            continue;
        }

        let mut end = index + 1;
        while end < chars.len() && end - index <= 10 && chars[end] != ';' {
            end += 1;
        }

        if end < chars.len() && chars[end] == ';' {
            let entity = chars[index + 1..end].iter().collect::<String>();
            if let Some(character) = decode_html_entity(&entity) {
                decoded.push(character);
                index = end + 1;
                continue;
            }
        }

        decoded.push(chars[index]);
        index += 1;
    }

    decoded
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            if let Some(hex) = entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(decimal) = entity.strip_prefix('#') {
                decimal.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                None
            }
        }
    }
}
