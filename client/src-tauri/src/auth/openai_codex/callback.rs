use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

use url::Url;

use super::super::AuthDiagnostic;
use super::flow::ActiveOpenAiCodexFlow;
use crate::commands::RuntimeAuthPhase;

const SUCCESS_HTML: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\" /><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" /><title>Authentication successful</title></head><body><p>Authentication successful. Return to Cadence to continue.</p></body></html>";

pub(super) fn spawn_callback_listener(listener: TcpListener, flow: ActiveOpenAiCodexFlow) {
    thread::spawn(move || {
        if listener.set_nonblocking(true).is_err() {
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_listener_configuration_failed".into(),
                    message: "Cadence could not configure the local OpenAI callback listener."
                        .into(),
                    retryable: false,
                },
                RuntimeAuthPhase::AwaitingManualInput,
            );
            return;
        }

        loop {
            if flow.is_cancelled() {
                break;
            }

            match listener.accept() {
                Ok((stream, _)) => {
                    if handle_callback_connection(stream, &flow) {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => {
                    flow.set_callback_diagnostic(
                        AuthDiagnostic {
                            code: "callback_listener_accept_failed".into(),
                            message: format!(
                                "Cadence could not accept the OpenAI callback connection: {error}"
                            ),
                            retryable: false,
                        },
                        RuntimeAuthPhase::AwaitingManualInput,
                    );
                    break;
                }
            }
        }
    });
}

fn handle_callback_connection(mut stream: TcpStream, flow: &ActiveOpenAiCodexFlow) -> bool {
    let mut request_line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        if reader.read_line(&mut request_line).is_err() {
            let _ = write_plain_response(&mut stream, 500, "Internal error");
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_request_read_failed".into(),
                    message: "Cadence could not read the OpenAI callback request.".into(),
                    retryable: false,
                },
                RuntimeAuthPhase::AwaitingBrowserCallback,
            );
            return false;
        }

        let mut discard = String::new();
        while reader
            .read_line(&mut discard)
            .ok()
            .filter(|_| discard != "\r\n")
            .is_some()
        {
            discard.clear();
        }
    }

    let target = match request_line.split_whitespace().nth(1) {
        Some(value) => value,
        None => {
            let _ = write_plain_response(&mut stream, 400, "Bad request");
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_request_malformed".into(),
                    message: "Cadence received a malformed OpenAI callback request line.".into(),
                    retryable: false,
                },
                RuntimeAuthPhase::AwaitingBrowserCallback,
            );
            return false;
        }
    };

    let url = match Url::parse(&format!("http://localhost{target}")) {
        Ok(url) => url,
        Err(_) => {
            let _ = write_plain_response(&mut stream, 400, "Bad callback URL");
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_query_malformed".into(),
                    message: "Cadence received a malformed OpenAI callback query string.".into(),
                    retryable: false,
                },
                RuntimeAuthPhase::AwaitingBrowserCallback,
            );
            return false;
        }
    };

    if url.path() != flow.callback_path() {
        let _ = write_plain_response(&mut stream, 404, "Not found");
        return false;
    }

    let returned_state = url
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()));
    if returned_state.as_deref() != Some(flow.expected_state()) {
        let _ = write_plain_response(&mut stream, 400, "State mismatch");
        flow.set_callback_diagnostic(
            AuthDiagnostic {
                code: "callback_state_mismatch".into(),
                message:
                    "Cadence rejected the OpenAI callback because the OAuth state did not match."
                        .into(),
                retryable: false,
            },
            RuntimeAuthPhase::AwaitingBrowserCallback,
        );
        return false;
    }

    let code = url
        .query_pairs()
        .find_map(|(key, value)| (key == "code").then(|| value.into_owned()));
    let Some(code) = code else {
        let _ = write_plain_response(&mut stream, 400, "Missing authorization code");
        flow.set_callback_diagnostic(
            AuthDiagnostic {
                code: "callback_code_missing".into(),
                message: "Cadence rejected the OpenAI callback because the authorization code was missing.".into(),
                retryable: false,
            },
            RuntimeAuthPhase::AwaitingBrowserCallback,
        );
        return false;
    };

    flow.store_callback_code(code);
    let _ = write_html_response(&mut stream, SUCCESS_HTML);
    true
}

fn write_html_response(stream: &mut TcpStream, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn write_plain_response(
    stream: &mut TcpStream,
    status_code: u16,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status_code {
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_code,
        reason,
        body.len(),
        body
    )
}
