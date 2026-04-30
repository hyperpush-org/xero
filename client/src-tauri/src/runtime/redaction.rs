use serde_json::{Map as JsonMap, Value as JsonValue};

const REDACTED_ARG: &str = "[REDACTED]";

pub(crate) fn render_command_for_persistence(argv: &[String]) -> String {
    redact_command_argv_for_persistence(argv).join(" ")
}

pub(crate) fn redact_command_argv_for_persistence(argv: &[String]) -> Vec<String> {
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

pub(crate) fn is_sensitive_argument_name(argument: &str) -> bool {
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

pub(crate) fn find_prohibited_persistence_content(value: &str) -> Option<&'static str> {
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
        || normalized.contains("oauth")
        || normalized.contains("password")
        || normalized.contains("private key")
        || normalized.contains("private_key")
        || normalized.contains("private-key")
        || normalized.contains("secret")
        || normalized.contains("session_token")
        || normalized.contains("session-token")
        || normalized.contains("sk-")
        || normalized.contains("-----begin")
        || normalized.contains("ghp_")
        || normalized.contains("gho_")
        || normalized.contains("ghu_")
        || normalized.contains("ghs_")
        || normalized.contains("github_pat_")
        || normalized.contains("glpat-")
        || normalized.contains("xoxb-")
        || normalized.contains("xoxp-")
        || normalized.contains("akia")
        || normalized.contains("aiza")
        || normalized.contains("ya29.")
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

pub(crate) fn redact_json_for_persistence(value: &JsonValue) -> (JsonValue, bool) {
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

fn high_confidence_secret_text(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("bearer ")
        || normalized.contains("sk-")
        || normalized.contains("ghp_")
        || normalized.contains("gho_")
        || normalized.contains("ghu_")
        || normalized.contains("ghs_")
        || normalized.contains("github_pat_")
        || normalized.contains("glpat-")
        || normalized.contains("xoxb-")
        || normalized.contains("xoxp-")
        || normalized.contains("-----begin")
        || normalized.contains("akia")
        || normalized.contains("aiza")
        || normalized.contains("ya29.")
        || find_prohibited_persistence_content(text).is_some()
            && (normalized.contains('=')
                || normalized.contains(':')
                || normalized.contains("token")
                || normalized.contains("password")
                || normalized.contains("private"))
}
