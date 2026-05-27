use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use serde::Serialize;
use serde_json::json;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use xero_desktop_control_ipc::{
    hash_session_token, DesktopSidecarActor, DesktopSidecarAuth, DesktopSidecarAuthScheme,
    DesktopSidecarCapabilities, DesktopSidecarHandshake, DesktopSidecarOperation,
    DesktopSidecarPermissionsPayload, DesktopSidecarRequest, DesktopSidecarResponse,
    DesktopSidecarStreamCapabilitiesPayload, DesktopSidecarStreamPayload,
    DesktopSidecarStreamStatus, DesktopSidecarStreamTransport, DESKTOP_SIDECAR_PROTOCOL,
    DESKTOP_SIDECAR_SCHEMA_VERSION,
};

struct SidecarHarness {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    session_id: String,
    token: String,
}

impl SidecarHarness {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_xero-desktop-sidecar"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sidecar binary");
        let stdin = child.stdin.take().expect("sidecar stdin");
        let stdout = BufReader::new(child.stdout.take().expect("sidecar stdout"));
        let mut harness = Self {
            child,
            stdin,
            stdout,
            session_id: "integration-session".into(),
            token: "integration-token".into(),
        };
        let handshake = DesktopSidecarHandshake {
            schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
            protocol: DESKTOP_SIDECAR_PROTOCOL.into(),
            session_id: harness.session_id.clone(),
            run_id: Some("integration-run".into()),
            token_sha256: hash_session_token(&harness.token),
            allowed_operations: DesktopSidecarOperation::all_contract_operations(),
            expires_at: future(120),
        };
        harness.write_json(&handshake);
        let response = harness.read_response();
        assert!(response.ok, "handshake failed: {:?}", response.error);
        harness
    }

    fn request(
        &mut self,
        operation: DesktopSidecarOperation,
        payload: serde_json::Value,
    ) -> DesktopSidecarResponse {
        self.request_with_auth(operation, payload, self.token.clone(), future(30))
    }

    fn request_with_auth(
        &mut self,
        operation: DesktopSidecarOperation,
        payload: serde_json::Value,
        token: String,
        expires_at: String,
    ) -> DesktopSidecarResponse {
        let request = DesktopSidecarRequest {
            schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
            protocol: DESKTOP_SIDECAR_PROTOCOL.into(),
            request_id: format!("req_{operation:?}"),
            session_id: self.session_id.clone(),
            run_id: Some("integration-run".into()),
            actor: DesktopSidecarActor::Agent,
            operation,
            payload,
            policy_decision_id: "policy_integration".into(),
            auth: DesktopSidecarAuth {
                scheme: DesktopSidecarAuthScheme::BearerSessionToken,
                token,
            },
            expires_at,
        };
        self.write_json(&request);
        self.read_response()
    }

    fn write_json<T: Serialize>(&mut self, value: &T) {
        serde_json::to_writer(&mut self.stdin, value).expect("write sidecar json");
        self.stdin.write_all(b"\n").expect("write newline");
        self.stdin.flush().expect("flush sidecar stdin");
    }

    fn read_response(&mut self) -> DesktopSidecarResponse {
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        assert!(!line.trim().is_empty(), "sidecar closed stdout");
        serde_json::from_str(&line).expect("decode sidecar response")
    }
}

impl Drop for SidecarHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn future(seconds: i64) -> String {
    (OffsetDateTime::now_utc() + Duration::seconds(seconds))
        .format(&Rfc3339)
        .expect("format timestamp")
}

fn past(seconds: i64) -> String {
    (OffsetDateTime::now_utc() - Duration::seconds(seconds))
        .format(&Rfc3339)
        .expect("format timestamp")
}

#[test]
fn sidecar_ipc_handles_authenticated_capabilities_and_stream_fallback() {
    let mut sidecar = SidecarHarness::spawn();

    let capabilities_response = sidecar.request(DesktopSidecarOperation::Capabilities, json!({}));
    assert!(capabilities_response.ok, "capabilities response");
    let capabilities = serde_json::from_value::<DesktopSidecarCapabilities>(
        capabilities_response.result.expect("capabilities payload"),
    )
    .expect("decode capabilities");
    assert!(capabilities.screenshot);

    let permissions_response =
        sidecar.request(DesktopSidecarOperation::PermissionsStatus, json!({}));
    assert!(permissions_response.ok, "permissions response");
    let permissions = serde_json::from_value::<DesktopSidecarPermissionsPayload>(
        permissions_response.result.expect("permissions payload"),
    )
    .expect("decode permissions");
    assert!(permissions
        .permissions
        .iter()
        .any(|permission| permission.name == "Input Monitoring"));
    assert!(permissions
        .permissions
        .iter()
        .any(|permission| permission.name == "Remote Desktop Portal"));

    let stream_capabilities_response =
        sidecar.request(DesktopSidecarOperation::StreamCapabilities, json!({}));
    assert!(
        stream_capabilities_response.ok,
        "stream capabilities response"
    );
    let stream_capabilities = serde_json::from_value::<DesktopSidecarStreamCapabilitiesPayload>(
        stream_capabilities_response
            .result
            .expect("stream capabilities payload"),
    )
    .expect("decode stream capabilities");
    assert_eq!(stream_capabilities.webrtc_stream, cfg!(target_os = "macos"));
    assert_eq!(
        stream_capabilities.native_video_track,
        cfg!(target_os = "macos")
    );
    assert_eq!(
        stream_capabilities.preferred_codec.as_deref(),
        Some("video/H264")
    );

    let stream_start_response = sidecar.request(
        DesktopSidecarOperation::StreamStart,
        json!({
            "sessionId": "integration-session",
            "runId": "integration-run",
            "streamId": "stream-integration",
            "maxWidth": 1280,
            "maxFrameRate": 24,
            "includeCursor": true,
            "quality": "balanced"
        }),
    );
    if !cfg!(target_os = "macos") {
        assert!(
            !stream_start_response.ok,
            "unsupported host should reject native stream start"
        );
        assert_eq!(
            stream_start_response
                .error
                .expect("stream start unsupported error")
                .code,
            "stream_native_publisher_unavailable"
        );
        return;
    }
    assert!(stream_start_response.ok, "stream start should offer");
    let stream_start = serde_json::from_value::<DesktopSidecarStreamPayload>(
        stream_start_response.result.expect("stream start payload"),
    )
    .expect("decode stream start");
    assert_eq!(
        stream_start.transport,
        DesktopSidecarStreamTransport::WebRtc
    );
    assert_eq!(stream_start.status, DesktopSidecarStreamStatus::Starting);
    assert_eq!(
        stream_start
            .session_description
            .as_ref()
            .map(|description| description.sdp_type.as_str()),
        Some("offer")
    );
    assert!(stream_start
        .session_description
        .as_ref()
        .is_some_and(|description| description.sdp.contains("m=video")));
    assert!(!stream_start
        .session_description
        .as_ref()
        .is_some_and(|description| description.sdp.contains("m=application")));

    let stream_answer_response = sidecar.request(
        DesktopSidecarOperation::StreamAnswer,
        json!({
            "sessionId": "integration-session",
            "runId": "integration-run",
            "streamId": "stream-integration",
            "sessionDescription": {
                "type": "answer",
                "sdp": "v=0"
            }
        }),
    );
    assert!(
        !stream_answer_response.ok,
        "fake stream answer should be rejected by the native WebRTC backend"
    );
    assert_eq!(
        stream_answer_response
            .error
            .expect("stream answer error")
            .code,
        "stream_signaling_failed"
    );

    let stream_stop_response = sidecar.request(
        DesktopSidecarOperation::StreamStop,
        json!({
            "sessionId": "integration-session",
            "runId": "integration-run",
            "streamId": "stream-integration"
        }),
    );
    assert!(stream_stop_response.ok, "stream stop response");
    let stream_stop = serde_json::from_value::<DesktopSidecarStreamPayload>(
        stream_stop_response.result.expect("stream stop payload"),
    )
    .expect("decode stream stop");
    assert_eq!(stream_stop.status, DesktopSidecarStreamStatus::Stopped);
}

#[test]
fn sidecar_ipc_rejects_bad_request_token() {
    let mut sidecar = SidecarHarness::spawn();

    let response = sidecar.request_with_auth(
        DesktopSidecarOperation::Health,
        json!({}),
        "wrong-token".into(),
        future(30),
    );

    assert!(!response.ok, "bad token should fail");
    assert_eq!(
        response.error.expect("auth error").code,
        "sidecar_auth_failed"
    );
}

#[test]
fn sidecar_ipc_rejects_expired_request() {
    let mut sidecar = SidecarHarness::spawn();

    let token = sidecar.token.clone();
    let response =
        sidecar.request_with_auth(DesktopSidecarOperation::Health, json!({}), token, past(30));

    assert!(!response.ok, "expired request should fail");
    assert_eq!(
        response.error.expect("expired error").code,
        "sidecar_request_expired"
    );
}

#[test]
fn sidecar_ipc_rejects_shell_like_payload_keys() {
    let mut sidecar = SidecarHarness::spawn();

    let response = sidecar.request(
        DesktopSidecarOperation::MouseClick,
        json!({ "command": "rm -rf ~" }),
    );

    assert!(!response.ok, "forbidden payload should fail");
    assert_eq!(
        response.error.expect("forbidden payload error").code,
        "sidecar_forbidden_payload"
    );
}
