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

fn is_sensitive_argument_name(argument: &str) -> bool {
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
        return Some("raw PTY byte sequences");
    }

    None
}
