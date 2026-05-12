use std::sync::Arc;

use tauri::http;
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use super::frame_bus::FrameBus;
use super::EmulatorState;

pub const URI_SCHEME: &str = "emulator";

/// Handles `emulator://...` requests from the webview.
///
/// Paths served:
/// - `/frame`: returns the JPEG bytes of the latest frame from the
///   [`FrameBus`]. The ETag header is set to the sequence number so the
///   webview can cheaply skip redraws when the bus hasn't advanced.
///
/// Anything else currently returns 404. The path is matched loosely because
/// different WebView implementations prefix the host differently (see the
/// tauri docs on custom URI schemes).
pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let path = request.uri().path().to_string();
    let app = ctx.app_handle().clone();

    // Serve from a worker thread so we don't block the webview IO loop when
    // a future JPEG encode becomes expensive.
    std::thread::spawn(move || {
        let state = match app.try_state::<EmulatorState>() {
            Some(state) => state,
            None => {
                responder.respond(not_found("emulator state unavailable"));
                return;
            }
        };
        let bus: Arc<FrameBus> = state.frame_bus();

        if path.trim_end_matches('/').ends_with("/frame") || path == "/frame" {
            match bus.latest() {
                Some(frame) => {
                    let response = http::Response::builder()
                        .status(http::StatusCode::OK)
                        .header(http::header::CONTENT_TYPE, "image/jpeg")
                        .header(http::header::CACHE_CONTROL, "no-cache, no-store")
                        .header(http::header::PRAGMA, "no-cache")
                        .header(http::header::EXPIRES, "0")
                        .header(http::header::CONTENT_LENGTH, frame.bytes.len().to_string())
                        .header(http::header::ETAG, format!("\"{}\"", frame.seq))
                        .header("X-Frame-Seq", frame.seq.to_string())
                        .header("X-Frame-Width", frame.width.to_string())
                        .header("X-Frame-Height", frame.height.to_string())
                        .body(frame.bytes.as_ref().clone())
                        .unwrap_or_else(|_| {
                            empty_response(http::StatusCode::INTERNAL_SERVER_ERROR)
                        });
                    responder.respond(response);
                }
                None => {
                    responder.respond(empty_response(http::StatusCode::NO_CONTENT));
                }
            }
            return;
        }

        responder.respond(not_found(&format!("unknown emulator path: {path}")));
    });
}

fn not_found(message: &str) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(http::StatusCode::NOT_FOUND)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .unwrap_or_else(|_| empty_response(http::StatusCode::NOT_FOUND))
}

fn empty_response(status: http::StatusCode) -> http::Response<Vec<u8>> {
    // Safe fallback: constructing an empty response with an arbitrary status
    // code never fails for the combinations we use here.
    let mut resp = http::Response::new(Vec::new());
    *resp.status_mut() = status;
    resp
}
