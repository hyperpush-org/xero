use crate::commands::{validate_non_empty, CommandError, CommandResult};

use super::{
    extract::{
        decode_utf8_body, extract_html_text, extract_html_title, normalize_extracted_text,
        truncate_chars_with_flag,
    },
    is_success_status, normalize_bounded_usize, normalize_timeout_ms, parse_http_url,
    transport::AutonomousWebTransportRequest,
    AutonomousWebFetchContentKind, AutonomousWebFetchOutput, AutonomousWebFetchRequest,
    AutonomousWebRuntime,
};

impl AutonomousWebRuntime {
    pub fn fetch(
        &self,
        request: AutonomousWebFetchRequest,
    ) -> CommandResult<AutonomousWebFetchOutput> {
        validate_non_empty(&request.url, "url")?;
        let url = parse_http_url(
            &request.url,
            "autonomous_web_fetch_url_invalid",
            "Xero requires `web_fetch` URLs to be valid absolute HTTP or HTTPS URLs.",
        )?;
        let timeout_ms = normalize_timeout_ms(
            request.timeout_ms,
            self.config.limits.default_timeout_ms,
            self.config.limits.max_timeout_ms,
            "autonomous_web_fetch_timeout_invalid",
            "web fetch timeout_ms",
        )?;
        let max_chars = normalize_bounded_usize(
            request.max_chars,
            self.config.limits.default_fetch_max_chars,
            self.config.limits.max_fetch_max_chars,
            "autonomous_web_fetch_max_chars_invalid",
            "web fetch maxChars",
        )?;

        let response = self.execute_transport(AutonomousWebTransportRequest {
            url: url.to_string(),
            headers: Vec::new(),
            timeout_ms,
            max_response_bytes: self.config.limits.max_response_bytes,
        })?;

        if !is_success_status(response.status) {
            return Err(map_fetch_status_error(response.status));
        }

        let content_type = normalize_content_type(response.content_type.as_deref());
        let content_kind = classify_fetch_content_kind(content_type.as_deref())?;
        let decoded_body = decode_utf8_body(
            &response.body,
            response.body_truncated,
            "autonomous_web_fetch_decode_failed",
            "Xero could not decode the fetched response body as UTF-8 text.",
        )?;

        let (title, extracted_content) = match content_kind {
            AutonomousWebFetchContentKind::Html => (
                extract_html_title(&decoded_body),
                extract_html_text(&decoded_body),
            ),
            AutonomousWebFetchContentKind::PlainText => {
                (None, normalize_extracted_text(&decoded_body))
            }
        };

        let (content, content_truncated) = truncate_chars_with_flag(&extracted_content, max_chars);
        Ok(AutonomousWebFetchOutput {
            url: request.url,
            final_url: response.final_url,
            content_type,
            content_kind,
            title,
            content,
            truncated: response.body_truncated || content_truncated,
        })
    }
}

fn map_fetch_status_error(status: u16) -> CommandError {
    match status {
        408 | 429 => CommandError::retryable(
            "autonomous_web_fetch_rate_limited",
            format!("Xero received HTTP {status} while fetching the requested URL."),
        ),
        500..=599 => CommandError::retryable(
            "autonomous_web_fetch_provider_unavailable",
            format!("Xero received HTTP {status} while fetching the requested URL."),
        ),
        _ => CommandError::user_fixable(
            "autonomous_web_fetch_status_error",
            format!("Xero received HTTP {status} while fetching the requested URL."),
        ),
    }
}

fn normalize_content_type(content_type: Option<&str>) -> Option<String> {
    content_type.and_then(|value| {
        let normalized = value
            .split(';')
            .next()
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    })
}

fn classify_fetch_content_kind(
    content_type: Option<&str>,
) -> CommandResult<AutonomousWebFetchContentKind> {
    match content_type {
        Some("text/html") | Some("application/xhtml+xml") => {
            Ok(AutonomousWebFetchContentKind::Html)
        }
        Some("text/plain") | None => Ok(AutonomousWebFetchContentKind::PlainText),
        Some(other) => Err(CommandError::user_fixable(
            "autonomous_web_fetch_content_type_unsupported",
            format!(
                "Xero only supports `text/plain`, `text/html`, or `application/xhtml+xml` responses for `web_fetch`; received `{other}`."
            ),
        )),
    }
}
