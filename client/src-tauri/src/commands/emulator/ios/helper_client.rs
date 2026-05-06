//! UDS client for communicating with the Swift helper binary.
//!
//! Binary framing protocol:
//!   [1 byte type][4 bytes payload length BE][payload bytes]
//!
//! Types:
//!   0x01 — JSON control message (UTF-8)
//!   0x02 — Frame: payload = [4B width BE][4B height BE][JPEG bytes]

use std::collections::HashMap;
use std::io::{self, BufWriter, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{json, Value};

use crate::commands::emulator::ios::input::{HardwareButton, TouchPhase};
use crate::commands::CommandError;

// Message type constants.
const MSG_TYPE_JSON: u8 = 0x01;
const MSG_TYPE_FRAME: u8 = 0x02;

/// A single captured frame from the Swift helper.
pub struct FrameData {
    pub width: u32,
    pub height: u32,
    pub jpeg: Vec<u8>,
}

/// Client that communicates with xero-ios-helper over a Unix domain socket.
pub struct HelperClient {
    writer: Mutex<BufWriter<UnixStream>>,
    frame_rx: Mutex<Option<mpsc::Receiver<FrameData>>>,
    responses: Arc<Mutex<HashMap<u64, mpsc::Sender<Value>>>>,
    request_id: AtomicU64,
    _reader_thread: JoinHandle<()>,
}

impl HelperClient {
    /// Connect to the helper's UDS and spawn the reader thread.
    pub fn connect(socket_path: &Path, timeout: Duration) -> io::Result<Self> {
        let stream = connect_with_timeout(socket_path, timeout)?;
        let read_stream = stream.try_clone()?;

        let (frame_tx, frame_rx) = mpsc::channel::<FrameData>();
        let responses: Arc<Mutex<HashMap<u64, mpsc::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let responses_clone = Arc::clone(&responses);

        let reader_thread = thread::Builder::new()
            .name("ios-helper-reader".into())
            .spawn(move || {
                reader_loop(read_stream, frame_tx, responses_clone);
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(Self {
            writer: Mutex::new(BufWriter::new(stream)),
            frame_rx: Mutex::new(Some(frame_rx)),
            responses,
            request_id: AtomicU64::new(1),
            _reader_thread: reader_thread,
        })
    }

    /// Take ownership of the frame receiver. Called once by the frame pump
    /// thread. Returns `None` if already taken.
    pub fn take_frame_rx(&self) -> Option<mpsc::Receiver<FrameData>> {
        self.frame_rx.lock().unwrap().take()
    }

    // MARK: - Public API

    /// Start frame capture at the given FPS. Returns (width, height).
    pub fn start_capture(&self, fps: u32) -> Result<(u32, u32), CommandError> {
        let resp = self.send_request("start_capture", json!({ "fps": fps }))?;
        let w = resp["width"].as_u64().unwrap_or(0) as u32;
        let h = resp["height"].as_u64().unwrap_or(0) as u32;
        Ok((w, h))
    }

    /// Stop frame capture.
    pub fn stop_capture(&self) -> Result<(), CommandError> {
        self.send_request("stop_capture", json!({}))?;
        Ok(())
    }

    /// Send a touch event to the simulator.
    pub fn send_touch(
        &self,
        phase: TouchPhase,
        x: i32,
        y: i32,
    ) -> Result<(), CommandError> {
        let phase_str = match phase {
            TouchPhase::Began => "began",
            TouchPhase::Moved => "moved",
            TouchPhase::Ended => "ended",
            TouchPhase::Cancelled => "cancelled",
        };
        self.send_request(
            "hid_touch",
            json!({ "phase": phase_str, "x": x, "y": y }),
        )?;
        Ok(())
    }

    /// Send a swipe gesture.
    pub fn send_swipe(
        &self,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        duration_ms: u32,
    ) -> Result<(), CommandError> {
        self.send_request(
            "hid_swipe",
            json!({
                "from_x": from_x, "from_y": from_y,
                "to_x": to_x, "to_y": to_y,
                "duration_ms": duration_ms,
            }),
        )?;
        Ok(())
    }

    /// Inject text into the simulator.
    pub fn send_text(&self, text: &str) -> Result<(), CommandError> {
        self.send_request("hid_text", json!({ "text": text }))?;
        Ok(())
    }

    /// Press a hardware button.
    pub fn send_button(&self, button: HardwareButton) -> Result<(), CommandError> {
        let name = match button {
            HardwareButton::Home => "home",
            HardwareButton::Lock => "lock",
            HardwareButton::VolumeUp => "volume_up",
            HardwareButton::VolumeDown => "volume_down",
            HardwareButton::Siri => "siri",
            HardwareButton::SideButton => "side_button",
        };
        self.send_request("hid_button", json!({ "button": name }))?;
        Ok(())
    }

    /// Get the accessibility tree from the Simulator process via AXUIElement.
    /// Returns the raw JSON tree that `ios_ui::normalize_tree()` can parse.
    pub fn accessibility_tree(&self) -> Result<serde_json::Value, CommandError> {
        let resp = self.send_request("accessibility_tree", json!({}))?;
        resp.get("tree")
            .cloned()
            .ok_or_else(|| {
                CommandError::system_fault(
                    "ios_helper_ax_no_tree",
                    "helper returned no tree in accessibility_tree response".to_string(),
                )
            })
    }

    /// Health check.
    pub fn ping(&self) -> Result<(), CommandError> {
        self.send_request("ping", json!({}))?;
        Ok(())
    }

    // MARK: - Request/response correlation

    /// Send a raw request to the helper and return the JSON response.
    /// Public so `emulator_inspector_element_at` can call AX inspection
    /// directly without a typed wrapper.
    pub fn send_request_raw(&self, method: &str, params: Value) -> Result<Value, CommandError> {
        self.send_request(method, params)
    }

    fn send_request(&self, method: &str, params: Value) -> Result<Value, CommandError> {
        let id = self.request_id.fetch_add(1, Ordering::Relaxed);

        let msg = json!({
            "id": id,
            "method": method,
            "params": params,
        });

        let payload = serde_json::to_vec(&msg).map_err(|e| {
            CommandError::system_fault("ios_helper_serialize", format!("json encode: {e}"))
        })?;

        // Create a one-shot channel for the response.
        let (tx, rx) = mpsc::channel::<Value>();
        {
            let mut map = self.responses.lock().unwrap();
            map.insert(id, tx);
        }

        // Write the framed message.
        {
            let mut writer = self.writer.lock().unwrap();
            write_message(&mut *writer, MSG_TYPE_JSON, &payload).map_err(|e| {
                CommandError::system_fault("ios_helper_write", format!("socket write: {e}"))
            })?;
            writer.flush().map_err(|e| {
                CommandError::system_fault("ios_helper_flush", format!("socket flush: {e}"))
            })?;
        }

        // Wait for response with timeout.
        let response = rx.recv_timeout(Duration::from_secs(10)).map_err(|_| {
            // Clean up the pending entry.
            self.responses.lock().unwrap().remove(&id);
            CommandError::system_fault(
                "ios_helper_timeout",
                format!("no response for {method} (id={id}) within 10s"),
            )
        })?;

        // Check for error in response.
        if let Some(err) = response.get("error") {
            let code = err["code"].as_str().unwrap_or("unknown");
            let message = err["message"].as_str().unwrap_or("helper error");
            return Err(CommandError::system_fault(
                &format!("ios_helper_{code}"),
                message.to_string(),
            ));
        }

        Ok(response)
    }
}

// MARK: - Reader thread

fn reader_loop(
    mut stream: UnixStream,
    frame_tx: mpsc::Sender<FrameData>,
    responses: Arc<Mutex<HashMap<u64, mpsc::Sender<Value>>>>,
) {
    loop {
        let (msg_type, payload) = match read_message(&mut stream) {
            Ok(msg) => msg,
            Err(_) => break, // Connection closed or error.
        };

        match msg_type {
            MSG_TYPE_JSON => {
                if let Ok(value) = serde_json::from_slice::<Value>(&payload) {
                    // Check if this is a response (has `id` field).
                    if let Some(id) = value["id"].as_u64() {
                        let tx = {
                            let mut map = responses.lock().unwrap();
                            map.remove(&id)
                        };
                        if let Some(tx) = tx {
                            // Events (no `id`) can be logged or handled here.
                            // For now, error events are logged to stderr.
                            if value.get("event").is_some() {
                                let code = value["code"].as_str().unwrap_or("unknown");
                                let message = value["message"].as_str().unwrap_or("");
                                eprintln!("ios-helper event: {code}: {message}");
                            }
                            let _ = tx.send(value);
                            continue;
                        }
                    }
                    // Events without an id (async events from helper).
                    if value.get("event").is_some() {
                        let code = value["code"].as_str().unwrap_or("unknown");
                        let message = value["message"].as_str().unwrap_or("");
                        eprintln!("ios-helper event: {code}: {message}");
                    }
                }
            }
            MSG_TYPE_FRAME => {
                if payload.len() >= 8 {
                    let width = u32::from_be_bytes([
                        payload[0], payload[1], payload[2], payload[3],
                    ]);
                    let height = u32::from_be_bytes([
                        payload[4], payload[5], payload[6], payload[7],
                    ]);
                    let jpeg = payload[8..].to_vec();
                    let _ = frame_tx.send(FrameData { width, height, jpeg });
                }
            }
            _ => {} // Unknown type; ignore.
        }
    }
}

// MARK: - Framing primitives

fn write_message(w: &mut impl Write, msg_type: u8, payload: &[u8]) -> io::Result<()> {
    // Header: [1 byte type][4 bytes length BE]
    w.write_all(&[msg_type])?;
    w.write_all(&(payload.len() as u32).to_be_bytes())?;
    w.write_all(payload)?;
    Ok(())
}

fn read_message(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    // Read header: 1 byte type + 4 bytes length.
    let mut header = [0u8; 5];
    r.read_exact(&mut header)?;

    let msg_type = header[0];
    let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;

    // Sanity limit: 50MB.
    if length > 50_000_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {length}"),
        ));
    }

    let mut payload = vec![0u8; length];
    r.read_exact(&mut payload)?;
    Ok((msg_type, payload))
}

/// Connect to a UDS with a retry loop + timeout.
fn connect_with_timeout(path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match UnixStream::connect(path) {
            Ok(stream) => return Ok(stream),
            Err(e) if std::time::Instant::now() >= deadline => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

// MARK: - Tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn write_read_json_message_roundtrip() {
        let payload = b"{\"id\":1,\"ok\":true}";
        let mut buf = Vec::new();
        write_message(&mut buf, MSG_TYPE_JSON, payload).unwrap();

        let mut cursor = Cursor::new(buf);
        let (msg_type, data) = read_message(&mut cursor).unwrap();
        assert_eq!(msg_type, MSG_TYPE_JSON);
        assert_eq!(data, payload);
    }

    #[test]
    fn write_read_frame_message_roundtrip() {
        let width: u32 = 1179;
        let height: u32 = 2556;
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];

        // Build frame payload: [4B width BE][4B height BE][JPEG bytes]
        let mut payload = Vec::new();
        payload.extend_from_slice(&width.to_be_bytes());
        payload.extend_from_slice(&height.to_be_bytes());
        payload.extend_from_slice(&jpeg);

        let mut buf = Vec::new();
        write_message(&mut buf, MSG_TYPE_FRAME, &payload).unwrap();

        let mut cursor = Cursor::new(buf);
        let (msg_type, data) = read_message(&mut cursor).unwrap();
        assert_eq!(msg_type, MSG_TYPE_FRAME);
        assert!(data.len() >= 8);

        let w = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let h = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let j = &data[8..];
        assert_eq!(w, 1179);
        assert_eq!(h, 2556);
        assert_eq!(j, &jpeg);
    }

    #[test]
    fn rejects_oversized_message() {
        // Construct a header claiming 100MB payload.
        let mut buf = vec![MSG_TYPE_JSON];
        buf.extend_from_slice(&100_000_001u32.to_be_bytes());
        buf.extend_from_slice(&[0u8; 10]); // Not enough data.

        let mut cursor = Cursor::new(buf);
        let result = read_message(&mut cursor);
        assert!(result.is_err());
    }
}
