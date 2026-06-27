use serde_json::{Map as JsonMap, Value as JsonValue};

const REDACTED_ARG: &str = "[REDACTED]";

pub fn render_command_for_persistence(argv: &[String]) -> String {
    redact_command_argv_for_persistence(argv).join(" ")
}

pub fn redact_command_argv_for_persistence(argv: &[String]) -> Vec<String> {
    let mut redact_next = false;
    argv.iter()
        .map(|argument| {
            if redact_next {
                redact_next = false;
                return REDACTED_ARG.into();
            }

            if let Some((key, _value)) = argument.split_once('=') {
                if is_sensitive_argument_name(key) {
                    return format!("{key}={REDACTED_ARG}");
                }
            }

            if is_sensitive_argument_name(argument) {
                redact_next = true;
                return argument.clone();
            }

            if find_prohibited_persistence_content(argument).is_some() {
                return REDACTED_ARG.into();
            }

            argument.clone()
        })
        .collect()
}

pub fn is_sensitive_argument_name(argument: &str) -> bool {
    let name = argument
        .trim()
        .trim_start_matches('-')
        .to_ascii_lowercase()
        .replace('-', "_");
    matches!(
        name.as_str(),
        "access_token"
            | "api_key"
            | "apikey"
            | "auth_token"
            | "authorization"
            | "bearer"
            | "client_secret"
            | "password"
            | "private_key"
            | "refresh_token"
            | "secret"
            | "session_token"
            | "token"
            | "x_api_key"
    )
}

pub fn find_prohibited_persistence_content(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();

    if normalized.contains("access_token")
        || normalized.contains("refresh_token")
        || normalized.contains("api_key")
        || normalized.contains("api-key")
        || normalized.contains("apikey")
        || normalized.contains("api key")
        || normalized.contains("auth token")
        || normalized.contains("authtoken")
        || normalized.contains("_auth")
        || normalized.contains("authorization:")
        || normalized.contains("bearer ")
        || normalized.contains("client_secret")
        || normalized.contains("client-secret")
        || normalized.contains("oauth_token")
        || normalized.contains("oauth-token")
        || normalized.contains("oauth token")
        || normalized.contains("password")
        || normalized.contains("private key")
        || normalized.contains("private_key")
        || normalized.contains("private-key")
        || normalized.contains("secret")
        || normalized.contains("session_token")
        || normalized.contains("session-token")
        || normalized.contains("-----begin")
        || contains_prefixed_credential_token(&normalized)
    {
        return Some("OAuth or API token material");
    }

    if normalized.contains("transcript") {
        return Some("runtime transcript text");
    }

    if normalized.contains("tool_payload")
        || normalized.contains("tool payload")
        || normalized.contains("raw payload")
    {
        return Some("tool raw payload data");
    }

    if normalized.contains("redirect_uri")
        || normalized.contains("authorization_url")
        || normalized.contains("/auth/callback")
        || normalized.contains("127.0.0.1:")
        || normalized.contains("localhost:")
    {
        return Some("OAuth redirect URL data");
    }

    if normalized.contains("chatgpt_account_id")
        || (normalized.contains("session_id") && normalized.contains("provider_id"))
    {
        return Some("auth-store contents");
    }

    if value.contains('\u{1b}')
        || value.contains('\0')
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Some("raw terminal byte sequences");
    }

    None
}

pub fn redact_json_for_persistence(value: &JsonValue) -> (JsonValue, bool) {
    match value {
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => (value.clone(), false),
        JsonValue::String(text) => {
            if high_confidence_secret_text(text) {
                (JsonValue::String(REDACTED_ARG.into()), true)
            } else {
                (value.clone(), false)
            }
        }
        JsonValue::Array(items) => {
            let mut redacted = false;
            let items = items
                .iter()
                .map(|item| {
                    let (item, item_redacted) = redact_json_for_persistence(item);
                    redacted |= item_redacted;
                    item
                })
                .collect();
            (JsonValue::Array(items), redacted)
        }
        JsonValue::Object(fields) => {
            let mut redacted = false;
            let mut output = JsonMap::new();
            for (key, field_value) in fields {
                if is_sensitive_argument_name(key) {
                    output.insert(key.clone(), JsonValue::String(REDACTED_ARG.into()));
                    redacted = true;
                    continue;
                }
                let (field_value, field_redacted) = redact_json_for_persistence(field_value);
                redacted |= field_redacted;
                output.insert(key.clone(), field_value);
            }
            (JsonValue::Object(output), redacted)
        }
    }
}

pub fn high_confidence_secret_text(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("-----begin")
        || contains_prefixed_credential_token(&normalized)
        || contains_sensitive_assignment_marker(&normalized)
}

fn contains_sensitive_assignment_marker(normalized: &str) -> bool {
    let has_sensitive_name = [
        "access_token",
        "refresh_token",
        "api_key",
        "api-key",
        "apikey",
        "api key",
        "auth token",
        "authtoken",
        "authorization",
        "client_secret",
        "client-secret",
        "oauth_token",
        "oauth-token",
        "oauth token",
        "password",
        "private key",
        "private_key",
        "private-key",
        "session_token",
        "session-token",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));

    has_sensitive_name && (normalized.contains('=') || normalized.contains(':'))
}

fn contains_prefixed_credential_token(normalized: &str) -> bool {
    contains_bearer_token(normalized)
        || contains_prefixed_token(normalized, "sk-", 10)
        || contains_prefixed_token(normalized, "ghp_", 16)
        || contains_prefixed_token(normalized, "gho_", 16)
        || contains_prefixed_token(normalized, "ghu_", 16)
        || contains_prefixed_token(normalized, "ghs_", 16)
        || contains_prefixed_token(normalized, "github_pat_", 16)
        || contains_prefixed_token(normalized, "glpat-", 16)
        || contains_prefixed_token(normalized, "xoxb-", 16)
        || contains_prefixed_token(normalized, "xoxp-", 16)
        || contains_prefixed_token(normalized, "akia", 16)
        || contains_prefixed_token(normalized, "aiza", 20)
        || contains_prefixed_token(normalized, "ya29.", 12)
}

fn contains_bearer_token(normalized: &str) -> bool {
    let marker = "bearer ";
    let mut search_from = 0;
    while let Some(offset) = normalized[search_from..].find(marker) {
        let suffix_start = search_from + offset + marker.len();
        let token_len = normalized[suffix_start..]
            .chars()
            .take_while(|character| is_secret_token_character(*character))
            .count();
        if token_len >= 10 {
            return true;
        }
        search_from = suffix_start;
    }
    false
}

fn contains_prefixed_token(normalized: &str, prefix: &str, min_suffix_len: usize) -> bool {
    let mut search_from = 0;
    while let Some(offset) = normalized[search_from..].find(prefix) {
        let start = search_from + offset;
        let suffix_start = start + prefix.len();
        if has_token_boundary(normalized, start) {
            let suffix_len = normalized[suffix_start..]
                .chars()
                .take_while(|character| is_secret_token_character(*character))
                .count();
            if suffix_len >= min_suffix_len {
                return true;
            }
        }
        search_from = suffix_start;
    }
    false
}

fn has_token_boundary(value: &str, start: usize) -> bool {
    match value[..start].chars().next_back() {
        Some(character) => !is_secret_token_character(character),
        None => true,
    }
}

fn is_secret_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_authorize_url_is_not_secret_like_by_itself() {
        let value = "https://discord.com/oauth2/authorize?client_id=123456789012345678&permissions=8&scope=bot%20applications.commands";

        assert_eq!(find_prohibited_persistence_content(value), None);
    }

    #[test]
    fn oauth_token_markers_remain_secret_like() {
        assert_eq!(
            find_prohibited_persistence_content(
                "oauth_token=sk-live-secret-value-that-is-long-enough"
            ),
            Some("OAuth or API token material")
        );
    }

    #[test]
    fn css_mask_properties_are_not_secret_like_tokens() {
        let css = ".grain::before { -webkit-mask-image: radial-gradient(circle, black, transparent); mask-image: linear-gradient(black, transparent); }";

        assert_eq!(find_prohibited_persistence_content(css), None);
        assert!(!high_confidence_secret_text(css));
    }

    #[test]
    fn token_prefixes_require_token_like_boundaries() {
        assert!(!high_confidence_secret_text(
            "mask-image: linear-gradient(black, transparent);"
        ));
        assert!(high_confidence_secret_text("use token sk-test-secret"));
        assert!(high_confidence_secret_text(
            "Authorization: Bearer abcdefghijklmnopqrstuvwxyz"
        ));
    }
}
