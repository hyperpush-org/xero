use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use cadence_desktop_lib::runtime::{
    AutonomousWebConfig, AutonomousWebFetchContentKind, AutonomousWebFetchRequest,
    AutonomousWebRuntime, AutonomousWebRuntimeLimits, AutonomousWebSearchProviderConfig,
    AutonomousWebSearchRequest, AutonomousWebTransport, AutonomousWebTransportError,
    AutonomousWebTransportRequest, AutonomousWebTransportResponse,
};
use serde_json::json;

#[derive(Clone, Default)]
struct FixtureTransport {
    requests: Arc<Mutex<Vec<AutonomousWebTransportRequest>>>,
    responses:
        Arc<Mutex<VecDeque<Result<AutonomousWebTransportResponse, AutonomousWebTransportError>>>>,
}

impl FixtureTransport {
    fn push_response(
        &self,
        response: Result<AutonomousWebTransportResponse, AutonomousWebTransportError>,
    ) {
        self.responses
            .lock()
            .expect("responses lock")
            .push_back(response);
    }

    fn take_requests(&self) -> Vec<AutonomousWebTransportRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

impl AutonomousWebTransport for FixtureTransport {
    fn execute(
        &self,
        request: &AutonomousWebTransportRequest,
    ) -> Result<AutonomousWebTransportResponse, AutonomousWebTransportError> {
        self.requests
            .lock()
            .expect("requests lock")
            .push(request.clone());
        self.responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .expect("fixture response should exist")
    }
}

fn search_runtime(transport: &FixtureTransport) -> AutonomousWebRuntime {
    AutonomousWebRuntime::with_transport(
        AutonomousWebConfig {
            search_provider: Some(AutonomousWebSearchProviderConfig::new(
                "https://search.example/api/search",
            )),
            limits: AutonomousWebRuntimeLimits::default(),
        },
        Arc::new(transport.clone()),
    )
}

fn fetch_runtime(transport: &FixtureTransport) -> AutonomousWebRuntime {
    AutonomousWebRuntime::with_transport(
        AutonomousWebConfig {
            search_provider: None,
            limits: AutonomousWebRuntimeLimits::default(),
        },
        Arc::new(transport.clone()),
    )
}

#[test]
fn web_search_returns_bounded_results_and_captures_truncation() {
    let transport = FixtureTransport::default();
    transport.push_response(Ok(AutonomousWebTransportResponse {
        status: 200,
        final_url: "https://search.example/api/search?q=rust&limit=2".into(),
        content_type: Some("application/json".into()),
        body: serde_json::to_vec(&json!({
            "results": [
                {
                    "title": "  Rust Search  ",
                    "url": "https://example.com/rust",
                    "snippet": "Alpha &amp; beta"
                },
                {
                    "title": "Second result",
                    "url": "https://example.com/second",
                    "snippet": "x".repeat(500)
                },
                {
                    "title": "Third result",
                    "url": "https://example.com/third",
                    "snippet": null
                }
            ]
        }))
        .expect("serialize search fixture"),
        body_truncated: false,
    }));

    let runtime = search_runtime(&transport);
    let output = runtime
        .search(AutonomousWebSearchRequest {
            query: "rust".into(),
            result_count: Some(2),
            timeout_ms: Some(1_500),
        })
        .expect("web search should succeed");

    let requests = transport.take_requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.contains("q=rust"));
    assert!(requests[0].url.contains("limit=2"));
    assert_eq!(requests[0].timeout_ms, 1_500);

    assert_eq!(output.results.len(), 2);
    assert_eq!(output.results[0].title, "Rust Search");
    assert_eq!(output.results[0].url, "https://example.com/rust");
    assert_eq!(output.results[0].snippet.as_deref(), Some("Alpha & beta"));
    assert!(output.truncated);
}

#[test]
fn web_search_fails_closed_without_backend_provider_config() {
    let runtime = AutonomousWebRuntime::new(AutonomousWebConfig::default());

    let error = runtime
        .search(AutonomousWebSearchRequest {
            query: "rust".into(),
            result_count: None,
            timeout_ms: None,
        })
        .expect_err("missing backend provider config should fail closed");

    assert_eq!(error.code, "autonomous_web_search_provider_unavailable");
    assert!(!error.retryable);
}

#[test]
fn web_search_rejects_oversized_and_malformed_provider_payloads() {
    let oversized_transport = FixtureTransport::default();
    oversized_transport.push_response(Ok(AutonomousWebTransportResponse {
        status: 200,
        final_url: "https://search.example/api/search?q=rust&limit=5".into(),
        content_type: Some("application/json".into()),
        body: b"{\"results\":[]}".to_vec(),
        body_truncated: true,
    }));
    let oversized_runtime = search_runtime(&oversized_transport);
    let oversized = oversized_runtime
        .search(AutonomousWebSearchRequest {
            query: "rust".into(),
            result_count: None,
            timeout_ms: None,
        })
        .expect_err("truncated provider payload should fail closed");
    assert_eq!(oversized.code, "autonomous_web_search_response_too_large");

    let malformed_transport = FixtureTransport::default();
    malformed_transport.push_response(Ok(AutonomousWebTransportResponse {
        status: 200,
        final_url: "https://search.example/api/search?q=rust&limit=5".into(),
        content_type: Some("application/json".into()),
        body: b"{\"results\": [{\"title\": 7}]}".to_vec(),
        body_truncated: false,
    }));
    let malformed_runtime = search_runtime(&malformed_transport);
    let malformed = malformed_runtime
        .search(AutonomousWebSearchRequest {
            query: "rust".into(),
            result_count: None,
            timeout_ms: None,
        })
        .expect_err("malformed provider payload should fail closed");
    assert_eq!(malformed.code, "autonomous_web_search_decode_failed");
}

#[test]
fn web_search_maps_timeout_and_retryable_status_failures() {
    let timeout_transport = FixtureTransport::default();
    timeout_transport.push_response(Err(AutonomousWebTransportError::Timeout(
        "provider timed out".into(),
    )));
    let timeout_runtime = search_runtime(&timeout_transport);
    let timeout = timeout_runtime
        .search(AutonomousWebSearchRequest {
            query: "rust".into(),
            result_count: None,
            timeout_ms: None,
        })
        .expect_err("transport timeout should be surfaced");
    assert_eq!(timeout.code, "autonomous_web_timeout");
    assert!(timeout.retryable);

    let status_transport = FixtureTransport::default();
    status_transport.push_response(Ok(AutonomousWebTransportResponse {
        status: 503,
        final_url: "https://search.example/api/search?q=rust&limit=5".into(),
        content_type: Some("application/json".into()),
        body: b"service unavailable".to_vec(),
        body_truncated: false,
    }));
    let status_runtime = search_runtime(&status_transport);
    let status = status_runtime
        .search(AutonomousWebSearchRequest {
            query: "rust".into(),
            result_count: None,
            timeout_ms: None,
        })
        .expect_err("retryable provider status should be surfaced");
    assert_eq!(status.code, "autonomous_web_search_provider_unavailable");
    assert!(status.retryable);
}

#[test]
fn web_fetch_extracts_html_and_truncates_bounded_content() {
    let transport = FixtureTransport::default();
    transport.push_response(Ok(AutonomousWebTransportResponse {
        status: 200,
        final_url: "https://example.com/page".into(),
        content_type: Some("text/html; charset=utf-8".into()),
        body: br#"<!doctype html><html><head><title>Example Page</title></head><body><main><h1>Heading</h1><p>Alpha &amp; beta</p><script>ignored()</script><p>Gamma delta epsilon zeta eta theta</p></main></body></html>"#.to_vec(),
        body_truncated: false,
    }));

    let runtime = fetch_runtime(&transport);
    let output = runtime
        .fetch(AutonomousWebFetchRequest {
            url: "https://example.com/page".into(),
            max_chars: Some(40),
            timeout_ms: None,
        })
        .expect("html fetch should succeed");

    assert_eq!(output.final_url, "https://example.com/page");
    assert_eq!(output.content_type.as_deref(), Some("text/html"));
    assert_eq!(output.content_kind, AutonomousWebFetchContentKind::Html);
    assert_eq!(output.title.as_deref(), Some("Example Page"));
    assert!(output.content.contains("Heading"));
    assert!(output.content.contains("Alpha & beta"));
    assert!(output.truncated);
}

#[test]
fn web_fetch_rejects_bad_urls_and_unsupported_content_types() {
    let runtime = AutonomousWebRuntime::new(AutonomousWebConfig::default());
    let bad_scheme = runtime
        .fetch(AutonomousWebFetchRequest {
            url: "mailto:test@example.com".into(),
            max_chars: None,
            timeout_ms: None,
        })
        .expect_err("unsupported URL schemes should fail closed");
    assert_eq!(bad_scheme.code, "autonomous_web_fetch_scheme_unsupported");

    let transport = FixtureTransport::default();
    transport.push_response(Ok(AutonomousWebTransportResponse {
        status: 200,
        final_url: "https://example.com/data".into(),
        content_type: Some("application/json".into()),
        body: br#"{"message":"hello"}"#.to_vec(),
        body_truncated: false,
    }));
    let runtime = fetch_runtime(&transport);
    let unsupported = runtime
        .fetch(AutonomousWebFetchRequest {
            url: "https://example.com/data".into(),
            max_chars: None,
            timeout_ms: None,
        })
        .expect_err("unsupported content types should fail closed");
    assert_eq!(
        unsupported.code,
        "autonomous_web_fetch_content_type_unsupported"
    );
}

#[test]
fn web_fetch_surfaces_timeout_and_decode_failures() {
    let timeout_transport = FixtureTransport::default();
    timeout_transport.push_response(Err(AutonomousWebTransportError::Timeout(
        "fetch timed out".into(),
    )));
    let timeout_runtime = fetch_runtime(&timeout_transport);
    let timeout = timeout_runtime
        .fetch(AutonomousWebFetchRequest {
            url: "https://example.com/page".into(),
            max_chars: None,
            timeout_ms: None,
        })
        .expect_err("transport timeout should be surfaced");
    assert_eq!(timeout.code, "autonomous_web_timeout");
    assert!(timeout.retryable);

    let decode_transport = FixtureTransport::default();
    decode_transport.push_response(Ok(AutonomousWebTransportResponse {
        status: 200,
        final_url: "https://example.com/page".into(),
        content_type: Some("text/plain".into()),
        body: vec![0xff, 0xfe, 0xfd],
        body_truncated: false,
    }));
    let decode_runtime = fetch_runtime(&decode_transport);
    let decode = decode_runtime
        .fetch(AutonomousWebFetchRequest {
            url: "https://example.com/page".into(),
            max_chars: None,
            timeout_ms: None,
        })
        .expect_err("undecodable bodies should fail closed");
    assert_eq!(decode.code, "autonomous_web_fetch_decode_failed");
}
