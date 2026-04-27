use std::time::Duration;

use reqwest::blocking::Client;

use crate::{commands::RuntimeAuthPhase, runtime::default_openai_callback_policy};

use super::super::AuthFlowError;

const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const DEFAULT_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const DEFAULT_SCOPE: &str = "openid profile email offline_access";
const DEFAULT_ORIGINATOR: &str = "pi";

#[derive(Debug, Clone)]
pub struct OpenAiCodexAuthConfig {
    pub client_id: String,
    pub authorize_url: String,
    pub token_url: String,
    pub callback_host: String,
    pub callback_port: u16,
    pub callback_path: String,
    pub scope: String,
    pub originator: String,
    pub timeout: Duration,
}

impl Default for OpenAiCodexAuthConfig {
    fn default() -> Self {
        let callback_policy = default_openai_callback_policy();
        Self {
            client_id: DEFAULT_CLIENT_ID.into(),
            authorize_url: DEFAULT_AUTHORIZE_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            callback_host: callback_policy.host,
            callback_port: callback_policy.preferred_port,
            callback_path: callback_policy.path,
            scope: DEFAULT_SCOPE.into(),
            originator: DEFAULT_ORIGINATOR.into(),
            timeout: Duration::from_secs(30),
        }
    }
}

impl OpenAiCodexAuthConfig {
    pub fn for_platform() -> Self {
        Self::default()
    }

    pub(super) fn http_client(&self) -> Result<Client, AuthFlowError> {
        Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| {
                AuthFlowError::terminal(
                    "oauth_http_client_unavailable",
                    RuntimeAuthPhase::Failed,
                    format!("Cadence could not build the OpenAI OAuth HTTP client: {error}"),
                )
            })
    }
}
