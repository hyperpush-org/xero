use url::Url;

use super::super::AuthFlowError;

pub(super) struct ParsedAuthorizationInput {
    pub(super) code: String,
    pub(super) state: Option<String>,
}

pub(super) fn parse_authorization_input(
    input: &str,
) -> Result<ParsedAuthorizationInput, AuthFlowError> {
    let value = input.trim();
    if value.is_empty() {
        return Err(AuthFlowError::invalid_manual_input(
            "empty_auth_state",
            "The pasted OpenAI authorization value was empty.",
        ));
    }

    if value.contains("://") {
        let url = Url::parse(value).map_err(|_| {
            AuthFlowError::invalid_manual_input(
                "malformed_redirect_url",
                "Cadence could not parse the pasted OpenAI redirect URL.",
            )
        })?;

        let code = url
            .query_pairs()
            .find_map(|(key, value)| (key == "code").then(|| value.into_owned()));
        let state = url
            .query_pairs()
            .find_map(|(key, value)| (key == "state").then(|| value.into_owned()));

        return code
            .map(|code| ParsedAuthorizationInput { code, state })
            .ok_or_else(|| {
                AuthFlowError::invalid_manual_input(
                    "authorization_code_missing",
                    "The pasted OpenAI redirect URL did not include an authorization code.",
                )
            });
    }

    if value.contains('#') {
        let mut parts = value.splitn(2, '#');
        let code = parts.next().unwrap_or_default().trim();
        let state = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if code.is_empty() {
            return Err(AuthFlowError::invalid_manual_input(
                "authorization_code_missing",
                "The pasted OpenAI authorization value did not include an authorization code.",
            ));
        }

        return Ok(ParsedAuthorizationInput {
            code: code.into(),
            state: state.map(ToOwned::to_owned),
        });
    }

    if value.contains('=') {
        let params = url::form_urlencoded::parse(value.as_bytes());
        let mut code = None;
        let mut state = None;
        for (key, value) in params {
            match key.as_ref() {
                "code" => code = Some(value.into_owned()),
                "state" => state = Some(value.into_owned()),
                _ => {}
            }
        }

        return code
            .map(|code| ParsedAuthorizationInput { code, state })
            .ok_or_else(|| {
                AuthFlowError::invalid_manual_input(
                    "authorization_code_missing",
                    "The pasted OpenAI parameters did not include an authorization code.",
                )
            });
    }

    Ok(ParsedAuthorizationInput {
        code: value.into(),
        state: None,
    })
}
