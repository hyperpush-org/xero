use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use cadence_desktop_lib::{
    auth::{
        cancel_openai_codex_flow, complete_openai_codex_flow, persist_openai_codex_session,
        refresh_openai_codex_session, start_openai_codex_flow, OpenAiCodexAuthConfig,
        StoredOpenAiCodexSession,
    },
    commands::RuntimeAuthPhase,
    configure_builder_with_state,
    state::DesktopState,
};
use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> (DesktopState, PathBuf) {
    let auth_store_path = root.path().join("app-data").join("openai-auth.json");
    (
        DesktopState::default().with_auth_store_file_override(auth_store_path.clone()),
        auth_store_path,
    )
}

fn auth_config(server: &TestHttpServer) -> OpenAiCodexAuthConfig {
    let mut config = OpenAiCodexAuthConfig::default();
    config.token_url = server.url("/oauth/token");
    config.callback_port = 0;
    config.originator = "cadence-tests".into();
    config.timeout = Duration::from_secs(5);
    config
}

fn jwt_with_account_id(account_id: &str) -> String {
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(
        json!({
            "https://api.openai.com/auth": {
                "chatgpt_account_id": account_id,
            }
        })
        .to_string(),
    );
    format!("{header}.{payload}.")
}

fn send_callback(redirect_uri: &str, state: &str, code: &str) {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("callback client");
    let response = client
        .get(redirect_uri)
        .query(&[("code", code), ("state", state)])
        .send()
        .expect("callback response");
    assert!(
        response.status().is_success(),
        "callback should return success html"
    );
}

#[test]
fn callback_flow_persists_tokens_only_to_app_local_store_and_exposes_redacted_snapshot() {
    let server = TestHttpServer::spawn(|form| {
        let code = form.get("code").cloned().unwrap_or_default();
        let account_id = format!("acct-{code}");
        TestHttpResponse::json(
            200,
            json!({
                "access_token": jwt_with_account_id(&account_id),
                "refresh_token": format!("refresh-{code}"),
                "expires_in": 3600,
            })
            .to_string(),
        )
    });
    let root = tempfile::tempdir().expect("temp dir");
    let (state, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);

    let config = auth_config(&server);
    let started = start_openai_codex_flow(
        &app.state::<DesktopState>(),
        config.clone(),
        Some("cadence-tests"),
    )
    .expect("start auth flow");
    assert!(started.callback_bound);
    assert_eq!(started.phase, RuntimeAuthPhase::AwaitingBrowserCallback);
    assert!(started
        .authorization_url
        .contains("code_challenge_method=S256"));
    assert!(started
        .authorization_url
        .contains("codex_cli_simplified_flow=true"));
    assert!(started
        .authorization_url
        .contains("originator=cadence-tests"));

    let initial_snapshot = app
        .state::<DesktopState>()
        .active_auth_flows()
        .snapshot(&started.flow_id)
        .expect("active flow snapshot");
    assert_eq!(
        initial_snapshot.phase,
        RuntimeAuthPhase::AwaitingBrowserCallback
    );
    assert!(initial_snapshot.session_id.is_none());
    assert!(initial_snapshot.account_id.is_none());
    assert!(initial_snapshot.last_error_code.is_none());

    send_callback(
        &started.redirect_uri,
        &started.expected_state,
        "browser-code",
    );

    let session = complete_openai_codex_flow(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        &started.flow_id,
        None,
        &config,
    )
    .expect("complete auth flow");
    assert_eq!(session.account_id, "acct-browser-code");

    let stored = std::fs::read_to_string(&auth_store_path).expect("auth store contents");
    assert!(stored.contains("refresh-browser-code"));
    assert!(stored.contains("acct-browser-code"));
    assert!(stored.contains("openai_codex"));
    assert!(!root.path().join(".cadence").join("state.db").exists());

    let final_snapshot = app
        .state::<DesktopState>()
        .active_auth_flows()
        .snapshot(&started.flow_id)
        .expect("final flow snapshot");
    assert_eq!(final_snapshot.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(
        final_snapshot.account_id.as_deref(),
        Some("acct-browser-code")
    );
    assert!(final_snapshot.last_error_code.is_none());
    assert!(!serde_json::to_string(&final_snapshot)
        .expect("snapshot json")
        .contains("refresh-browser-code"));

    let request = server.single_request();
    assert_eq!(
        request.get("grant_type").map(String::as_str),
        Some("authorization_code")
    );
    assert_eq!(
        request.get("code").map(String::as_str),
        Some("browser-code")
    );
    assert_eq!(
        request.get("redirect_uri").map(String::as_str),
        Some(started.redirect_uri.as_str())
    );
    assert!(request
        .get("code_verifier")
        .is_some_and(|value| !value.trim().is_empty()));
}

#[test]
fn manual_fallback_is_used_when_callback_port_cannot_bind() {
    let occupied = TcpListener::bind(("127.0.0.1", 0)).expect("occupied port");
    let occupied_port = occupied.local_addr().expect("occupied addr").port();
    let server = TestHttpServer::spawn(|_| {
        TestHttpResponse::json(
            200,
            json!({
                "access_token": jwt_with_account_id("acct-manual"),
                "refresh_token": "refresh-manual",
                "expires_in": 3600,
            })
            .to_string(),
        )
    });
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _) = create_state(&root);
    let app = build_mock_app(state);

    let mut config = auth_config(&server);
    config.callback_port = occupied_port;
    let started = start_openai_codex_flow(&app.state::<DesktopState>(), config.clone(), None)
        .expect("start auth flow");
    assert!(!started.callback_bound);
    assert_eq!(started.phase, RuntimeAuthPhase::AwaitingManualInput);
    assert_eq!(
        started.last_error_code.as_deref(),
        Some("callback_listener_bind_failed")
    );

    let fallback_snapshot = app
        .state::<DesktopState>()
        .active_auth_flows()
        .snapshot(&started.flow_id)
        .expect("manual fallback flow snapshot");
    assert_eq!(
        fallback_snapshot.last_error_code.as_deref(),
        Some("callback_listener_bind_failed")
    );
    let fallback_diagnostic = fallback_snapshot
        .last_error
        .expect("manual fallback should persist bind diagnostic");
    assert!(
        fallback_diagnostic
            .message
            .contains(occupied_port.to_string().as_str()),
        "expected fallback diagnostic to include occupied callback port"
    );

    let manual_input = format!(
        "{}?code=manual-code&state={}",
        started.redirect_uri, started.expected_state
    );
    let session = complete_openai_codex_flow(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        &started.flow_id,
        Some(&manual_input),
        &config,
    )
    .expect("manual completion should succeed");
    assert_eq!(session.account_id, "acct-manual");

    drop(occupied);
}

#[test]
fn malformed_callback_host_fails_closed_before_browser_url_is_opened() {
    let server = TestHttpServer::spawn(|_| {
        TestHttpResponse::json(
            200,
            json!({
                "access_token": jwt_with_account_id("acct-unused"),
                "refresh_token": "refresh-unused",
                "expires_in": 3600,
            })
            .to_string(),
        )
    });
    let root = tempfile::tempdir().expect("temp dir");
    let (state, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);

    let mut config = auth_config(&server);
    config.callback_host = "127.0.0.1:9999".into();
    let error = start_openai_codex_flow(&app.state::<DesktopState>(), config, None)
        .expect_err("malformed callback host should fail before flow starts");
    assert_eq!(error.code, "callback_listener_config_invalid");
    assert_eq!(error.phase, RuntimeAuthPhase::Starting);
    assert!(!auth_store_path.exists());
}

#[test]
fn callback_code_wins_over_manual_paste() {
    let server = TestHttpServer::spawn(|form| {
        let code = form.get("code").cloned().unwrap_or_default();
        TestHttpResponse::json(
            200,
            json!({
                "access_token": jwt_with_account_id(&format!("acct-{code}")),
                "refresh_token": format!("refresh-{code}"),
                "expires_in": 3600,
            })
            .to_string(),
        )
    });
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _) = create_state(&root);
    let app = build_mock_app(state);
    let config = auth_config(&server);

    let started = start_openai_codex_flow(&app.state::<DesktopState>(), config.clone(), None)
        .expect("start auth flow");
    send_callback(
        &started.redirect_uri,
        &started.expected_state,
        "browser-code",
    );

    let manual_input = format!(
        "{}?code=manual-code&state={}",
        started.redirect_uri, started.expected_state
    );
    let session = complete_openai_codex_flow(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        &started.flow_id,
        Some(&manual_input),
        &config,
    )
    .expect("callback should win");
    assert_eq!(session.account_id, "acct-browser-code");

    let request = server.single_request();
    assert_eq!(
        request.get("code").map(String::as_str),
        Some("browser-code")
    );
}

#[test]
fn manual_completion_rejects_empty_missing_mismatched_and_malformed_inputs() {
    let server = TestHttpServer::spawn(|_| {
        TestHttpResponse::json(
            200,
            json!({
                "access_token": jwt_with_account_id("acct-unused"),
                "refresh_token": "refresh-unused",
                "expires_in": 3600,
            })
            .to_string(),
        )
    });
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _) = create_state(&root);
    let app = build_mock_app(state);
    let config = auth_config(&server);

    let empty = start_openai_codex_flow(&app.state::<DesktopState>(), config.clone(), None)
        .expect("empty start");
    let error = complete_openai_codex_flow(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        &empty.flow_id,
        Some("   "),
        &config,
    )
    .expect_err("empty manual input should fail");
    assert_eq!(error.code, "empty_auth_state");

    let missing = start_openai_codex_flow(&app.state::<DesktopState>(), config.clone(), None)
        .expect("missing start");
    let error = complete_openai_codex_flow(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        &missing.flow_id,
        Some("state=abc123"),
        &config,
    )
    .expect_err("missing code should fail");
    assert_eq!(error.code, "authorization_code_missing");

    let mismatched = start_openai_codex_flow(&app.state::<DesktopState>(), config.clone(), None)
        .expect("mismatch start");
    let mismatched_input = format!(
        "{}?code=wrong-code&state=wrong-state",
        mismatched.redirect_uri
    );
    let error = complete_openai_codex_flow(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        &mismatched.flow_id,
        Some(&mismatched_input),
        &config,
    )
    .expect_err("state mismatch should fail");
    assert_eq!(error.code, "callback_state_mismatch");

    let malformed = start_openai_codex_flow(&app.state::<DesktopState>(), config.clone(), None)
        .expect("malformed start");
    let error = complete_openai_codex_flow(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        &malformed.flow_id,
        Some("https://not valid"),
        &config,
    )
    .expect_err("malformed redirect should fail");
    assert_eq!(error.code, "malformed_redirect_url");
}

#[test]
fn token_exchange_failures_and_decode_errors_do_not_persist_credentials() {
    let rejected_server = TestHttpServer::spawn(|_| TestHttpResponse::plain(401, "denied"));
    let decode_server = TestHttpServer::spawn(|_| {
        TestHttpResponse::json(
            200,
            json!({
                "access_token": jwt_with_account_id("acct-decode"),
                "expires_in": 3600,
            })
            .to_string(),
        )
    });

    let rejected_root = tempfile::tempdir().expect("temp dir");
    let (rejected_state, rejected_store) = create_state(&rejected_root);
    let rejected_app = build_mock_app(rejected_state);
    let rejected_config = auth_config(&rejected_server);
    let rejected = start_openai_codex_flow(
        &rejected_app.state::<DesktopState>(),
        rejected_config.clone(),
        None,
    )
    .expect("rejected start");
    let rejected_input = format!(
        "{}?code=bad-code&state={}",
        rejected.redirect_uri, rejected.expected_state
    );
    let error = complete_openai_codex_flow(
        &rejected_app.handle().clone(),
        &rejected_app.state::<DesktopState>(),
        &rejected.flow_id,
        Some(&rejected_input),
        &rejected_config,
    )
    .expect_err("401 response should fail");
    assert_eq!(error.code, "token_exchange_rejected");
    assert!(!rejected_store.exists());

    let decode_root = tempfile::tempdir().expect("temp dir");
    let (decode_state, decode_store) = create_state(&decode_root);
    let decode_app = build_mock_app(decode_state);
    let decode_config = auth_config(&decode_server);
    let decode = start_openai_codex_flow(
        &decode_app.state::<DesktopState>(),
        decode_config.clone(),
        None,
    )
    .expect("decode start");
    let decode_input = format!(
        "{}?code=decode-code&state={}",
        decode.redirect_uri, decode.expected_state
    );
    let error = complete_openai_codex_flow(
        &decode_app.handle().clone(),
        &decode_app.state::<DesktopState>(),
        &decode.flow_id,
        Some(&decode_input),
        &decode_config,
    )
    .expect_err("malformed token response should fail");
    assert_eq!(error.code, "token_exchange_decode_failed");
    assert!(!decode_store.exists());
}

#[test]
fn refresh_failure_preserves_existing_credentials() {
    let server = TestHttpServer::spawn(|_| TestHttpResponse::plain(500, "boom"));
    let root = tempfile::tempdir().expect("temp dir");
    let (state, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);

    persist_openai_codex_session(
        &auth_store_path,
        StoredOpenAiCodexSession {
            provider_id: "openai_codex".into(),
            session_id: "session-1".into(),
            account_id: "acct-refresh".into(),
            access_token: jwt_with_account_id("acct-refresh"),
            refresh_token: "refresh-existing".into(),
            expires_at: 10,
            updated_at: "2026-01-01T00:00:00Z".into(),
        },
    )
    .expect("persist seed session");
    let before = std::fs::read_to_string(&auth_store_path).expect("seed contents");

    let error = refresh_openai_codex_session(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        "acct-refresh",
        &auth_config(&server),
    )
    .expect_err("refresh should fail");
    assert_eq!(error.code, "token_refresh_server_error");
    let after = std::fs::read_to_string(&auth_store_path).expect("post-refresh contents");
    assert_eq!(
        before, after,
        "failed refresh should not rewrite stored tokens"
    );
}

#[test]
fn cancelled_login_rejects_completion() {
    let server = TestHttpServer::spawn(|_| {
        TestHttpResponse::json(
            200,
            json!({
                "access_token": jwt_with_account_id("acct-cancelled"),
                "refresh_token": "refresh-cancelled",
                "expires_in": 3600,
            })
            .to_string(),
        )
    });
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _) = create_state(&root);
    let app = build_mock_app(state);
    let config = auth_config(&server);

    let started = start_openai_codex_flow(&app.state::<DesktopState>(), config.clone(), None)
        .expect("start auth flow");
    let cancelled = cancel_openai_codex_flow(&app.state::<DesktopState>(), &started.flow_id)
        .expect("cancel snapshot");
    assert_eq!(cancelled.phase, RuntimeAuthPhase::Cancelled);

    let manual_input = format!(
        "{}?code=should-not-complete&state={}",
        started.redirect_uri, started.expected_state
    );
    let error = complete_openai_codex_flow(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        &started.flow_id,
        Some(&manual_input),
        &config,
    )
    .expect_err("cancelled flow should fail");
    assert_eq!(error.code, "auth_flow_cancelled");
}

struct TestHttpServer {
    base_url: String,
    requests: Arc<Mutex<Vec<HashMap<String, String>>>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl TestHttpServer {
    fn spawn(
        responder: impl Fn(HashMap<String, String>) -> TestHttpResponse + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind token server");
        listener
            .set_nonblocking(true)
            .expect("nonblocking token server");
        let address = listener.local_addr().expect("token server addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let responder = Arc::new(responder);
        let requests_for_thread = Arc::clone(&requests);
        let shutdown_for_thread = Arc::clone(&shutdown);

        let handle = thread::spawn(move || {
            while !shutdown_for_thread.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let Some((request_line, _headers, body)) = read_http_request(&mut stream)
                        else {
                            continue;
                        };
                        if !request_line.starts_with("POST ") {
                            let _ = write_http_response(
                                &mut stream,
                                TestHttpResponse::plain(404, "not found"),
                            );
                            continue;
                        }

                        let form = url::form_urlencoded::parse(body.as_bytes())
                            .into_owned()
                            .collect::<HashMap<_, _>>();
                        requests_for_thread
                            .lock()
                            .expect("request log lock")
                            .push(form.clone());
                        let response = responder(form);
                        let _ = write_http_response(&mut stream, response);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            base_url: format!("http://{}", address),
            requests,
            shutdown,
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn single_request(&self) -> HashMap<String, String> {
        let requests = self.requests.lock().expect("request log lock");
        assert_eq!(requests.len(), 1, "expected exactly one token request");
        requests[0].clone()
    }
}

impl Drop for TestHttpServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct TestHttpResponse {
    status: u16,
    body: String,
    content_type: &'static str,
}

impl TestHttpResponse {
    fn json(status: u16, body: String) -> Self {
        Self {
            status,
            body,
            content_type: "application/json",
        }
    }

    fn plain(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.into(),
            content_type: "text/plain",
        }
    }
}

fn read_http_request(
    stream: &mut std::net::TcpStream,
) -> Option<(String, HashMap<String, String>, String)> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0_u8; content_length];
    reader.read_exact(&mut body).ok()?;

    Some((
        request_line.trim().to_owned(),
        headers,
        String::from_utf8_lossy(&body).into_owned(),
    ))
}

fn write_http_response(
    stream: &mut std::net::TcpStream,
    response: TestHttpResponse,
) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        reason,
        response.content_type,
        response.body.len(),
        response.body
    )
}
