use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

use super::super::AuthFlowError;
use crate::commands::RuntimeAuthPhase;

const JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";

pub(super) fn extract_account_id(access_token: &str) -> Result<String, AuthFlowError> {
    let payload = decode_jwt_payload(access_token)?;
    payload
        .get(JWT_CLAIM_PATH)
        .and_then(|value| value.get("chatgpt_account_id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AuthFlowError::terminal(
                "account_id_missing",
                RuntimeAuthPhase::Failed,
                "Xero could not extract the OpenAI account id from the access token.",
            )
        })
}

fn decode_jwt_payload(access_token: &str) -> Result<serde_json::Value, AuthFlowError> {
    let parts = access_token.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(AuthFlowError::terminal(
            "access_token_malformed",
            RuntimeAuthPhase::Failed,
            "Xero received a malformed OpenAI access token.",
        ));
    }

    let payload = URL_SAFE_NO_PAD.decode(parts[1]).map_err(|_| {
        AuthFlowError::terminal(
            "access_token_malformed",
            RuntimeAuthPhase::Failed,
            "Xero could not base64url-decode the OpenAI access token payload.",
        )
    })?;

    serde_json::from_slice(&payload).map_err(|_| {
        AuthFlowError::terminal(
            "access_token_malformed",
            RuntimeAuthPhase::Failed,
            "Xero could not parse the OpenAI access token payload JSON.",
        )
    })
}
