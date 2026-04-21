use reqwest::blocking::Response;
use serde::Deserialize;

use super::super::AuthFlowError;
use super::config::OpenAiCodexAuthConfig;
use crate::commands::RuntimeAuthPhase;

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

#[derive(Debug)]
pub(super) struct TokenSuccess {
    pub(super) access_token: String,
    pub(super) refresh_token: String,
    pub(super) expires_at: i64,
}

pub(super) fn exchange_authorization_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    config: &OpenAiCodexAuthConfig,
) -> Result<TokenSuccess, AuthFlowError> {
    let client = config.http_client()?;
    let response = client
        .post(&config.token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", config.client_id.as_str()),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .map_err(|error| {
            map_http_error(error, RuntimeAuthPhase::ExchangingCode, "token_exchange")
        })?;

    parse_token_response(response, RuntimeAuthPhase::ExchangingCode, "token_exchange")
}

pub(super) fn refresh_access_token(
    refresh_token: &str,
    config: &OpenAiCodexAuthConfig,
) -> Result<TokenSuccess, AuthFlowError> {
    let client = config.http_client()?;
    let response = client
        .post(&config.token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", config.client_id.as_str()),
        ])
        .send()
        .map_err(|error| map_http_error(error, RuntimeAuthPhase::Refreshing, "token_refresh"))?;

    parse_token_response(response, RuntimeAuthPhase::Refreshing, "token_refresh")
}

fn parse_token_response(
    response: Response,
    phase: RuntimeAuthPhase,
    prefix: &str,
) -> Result<TokenSuccess, AuthFlowError> {
    let status = response.status();
    if !status.is_success() {
        let code = if status.is_server_error() {
            format!("{prefix}_server_error")
        } else {
            format!("{prefix}_rejected")
        };
        let retryable = status.is_server_error();
        let body = response.text().unwrap_or_default();
        return Err(AuthFlowError::new(
            code,
            phase,
            format!(
                "OpenAI returned HTTP {} during {}.{}",
                status.as_u16(),
                prefix.replace('_', " "),
                if body.trim().is_empty() {
                    String::new()
                } else {
                    format!(" Response: {}", body.trim())
                }
            ),
            retryable,
        ));
    }

    let payload: TokenResponse = response.json().map_err(|error| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase.clone(),
            format!(
                "Cadence could not decode the OpenAI {} response: {error}",
                prefix.replace('_', " ")
            ),
        )
    })?;

    let access_token = payload.access_token.ok_or_else(|| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase.clone(),
            format!(
                "Cadence rejected the OpenAI {} response because access_token was missing.",
                prefix.replace('_', " ")
            ),
        )
    })?;
    let refresh_token = payload.refresh_token.ok_or_else(|| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase.clone(),
            format!(
                "Cadence rejected the OpenAI {} response because refresh_token was missing.",
                prefix.replace('_', " ")
            ),
        )
    })?;
    let expires_in = payload.expires_in.ok_or_else(|| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase,
            format!(
                "Cadence rejected the OpenAI {} response because expires_in was missing.",
                prefix.replace('_', " ")
            ),
        )
    })?;

    Ok(TokenSuccess {
        access_token,
        refresh_token,
        expires_at: current_unix_timestamp() + expires_in,
    })
}

fn map_http_error(error: reqwest::Error, phase: RuntimeAuthPhase, prefix: &str) -> AuthFlowError {
    if error.is_timeout() {
        return AuthFlowError::retryable(
            format!("{prefix}_timeout"),
            phase,
            format!("OpenAI {} timed out.", prefix.replace('_', " ")),
        );
    }

    AuthFlowError::retryable(
        format!("{prefix}_request_failed"),
        phase,
        format!(
            "Cadence could not complete the OpenAI {} request: {error}",
            prefix.replace('_', " ")
        ),
    )
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}
