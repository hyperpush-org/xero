//! idb gRPC client.
//!
//! This module has two build paths:
//!
//! 1. With the `ios-grpc` Cargo feature on, `tonic-build` compiles
//!    `proto/idb.proto` into `companion_service::CompanionServiceClient`
//!    and we wire real gRPC calls to the running `idb_companion` process.
//!    The client embeds a multi-threaded tokio runtime so the rest of the
//!    sync session code can keep blocking like it does today.
//!
//! 2. Without the feature, the client returns `ios_idb_proto_missing` for
//!    every streaming call. The session layer already falls back to a
//!    `simctl io screenshot` poll in that case, so the sidebar stays
//!    functional — it just doesn't stream H.264 frames.

use std::time::Duration;

use crate::commands::CommandError;

use super::input::HidEvent;

type NalCallback = Box<dyn FnMut(&[u8]) + Send>;

/// Stable, frontend-exposed handle to a running idb_companion.
pub struct IdbClient {
    grpc_port: u16,
    /// Simulator UDID — kept alongside the gRPC port so the non-gRPC
    /// HID fallback (AppleScript → Simulator.app) knows which device
    /// window to focus.
    udid: String,
    #[cfg(feature = "ios-grpc")]
    inner: grpc_impl::Runtime,
}

impl IdbClient {
    pub fn new(grpc_port: u16, udid: impl Into<String>) -> Self {
        let udid = udid.into();
        #[cfg(feature = "ios-grpc")]
        {
            let inner = grpc_impl::Runtime::connect(grpc_port);
            Self {
                grpc_port,
                udid,
                inner,
            }
        }
        #[cfg(not(feature = "ios-grpc"))]
        {
            Self { grpc_port, udid }
        }
    }

    pub fn grpc_port(&self) -> u16 {
        self.grpc_port
    }

    pub fn udid(&self) -> &str {
        &self.udid
    }

    /// Open a bidirectional `VideoStream` RPC and push raw H.264 NAL
    /// units into a callback. Returns a handle the caller drops to
    /// cancel the stream.
    pub fn start_video_stream(
        &self,
        fps: u32,
        on_nal: NalCallback,
    ) -> Result<VideoStreamHandle, CommandError> {
        #[cfg(feature = "ios-grpc")]
        {
            self.inner.start_video_stream(fps, on_nal)
        }
        #[cfg(not(feature = "ios-grpc"))]
        {
            let _ = (fps, on_nal);
            Err(grpc_unimplemented(
                "VideoStream",
                "enable the `ios-grpc` Cargo feature to link the vendored idb proto",
            ))
        }
    }

    /// Push a HID event into idb's client-streaming `hid` RPC. The RPC is
    /// opened once per session and kept open for its lifetime — every
    /// Touch Down / Move / Up shares one CoreSimulator HID session, which
    /// is what iOS needs to correlate a down/up pair into a tap.
    ///
    /// When the `ios-grpc` feature is off we route the common button /
    /// text events through an AppleScript fallback against Simulator.app
    /// so dev builds still get working Home / Lock / text-injection
    /// without a tonic rebuild. Touch / swipe still require the gRPC
    /// path because AppleScript mouse coordinates can't reliably map
    /// to device-pixel space.
    pub fn send_hid(&self, event: HidEvent) -> Result<(), CommandError> {
        #[cfg(feature = "ios-grpc")]
        {
            self.inner.send_hid(event)
        }
        #[cfg(not(feature = "ios-grpc"))]
        {
            send_hid_applescript(&self.udid, event)
        }
    }

    /// Pull the current accessibility tree as idb's native JSON. The
    /// caller maps it onto `emulator::automation::UiTree`.
    pub fn accessibility_tree(&self) -> Result<serde_json::Value, CommandError> {
        #[cfg(feature = "ios-grpc")]
        {
            self.inner.accessibility_tree()
        }
        #[cfg(not(feature = "ios-grpc"))]
        {
            Err(grpc_unimplemented(
                "AccessibilityInfo",
                "enable the `ios-grpc` Cargo feature to link the vendored idb proto",
            ))
        }
    }

    /// Connect to the log stream. Each line is handed to the callback.
    pub fn start_log_stream(
        &self,
        on_line: Box<dyn FnMut(&str) + Send>,
    ) -> Result<LogStreamHandle, CommandError> {
        #[cfg(feature = "ios-grpc")]
        {
            self.inner.start_log_stream(on_line)
        }
        #[cfg(not(feature = "ios-grpc"))]
        {
            let _ = on_line;
            Err(grpc_unimplemented(
                "Log",
                "enable the `ios-grpc` Cargo feature to link the vendored idb proto",
            ))
        }
    }
}

/// Opaque handle returned by `start_video_stream`; dropping it cancels the
/// underlying gRPC stream.
pub struct VideoStreamHandle {
    #[cfg(feature = "ios-grpc")]
    cancel: tokio::sync::oneshot::Sender<()>,
    #[cfg(not(feature = "ios-grpc"))]
    _placeholder: (),
}

impl VideoStreamHandle {
    pub fn shutdown(self, _grace: Duration) {
        #[cfg(feature = "ios-grpc")]
        {
            let _ = self.cancel.send(());
        }
    }
}

/// Same shape as `VideoStreamHandle` but for the log stream.
pub struct LogStreamHandle {
    #[cfg(feature = "ios-grpc")]
    cancel: tokio::sync::oneshot::Sender<()>,
    #[cfg(not(feature = "ios-grpc"))]
    _placeholder: (),
}

impl LogStreamHandle {
    pub fn shutdown(self, _grace: Duration) {
        #[cfg(feature = "ios-grpc")]
        {
            let _ = self.cancel.send(());
        }
    }
}

#[cfg(not(feature = "ios-grpc"))]
fn grpc_unimplemented(method: &str, detail: &str) -> CommandError {
    CommandError::system_fault(
        "ios_idb_proto_missing",
        format!("idb gRPC `{method}` is not yet wired up in this Xero build. {detail}."),
    )
}

/// AppleScript-powered HID fallback for builds without the
/// `ios-grpc` feature. Handles Home, Lock, Siri, app-switcher, and
/// text; surfaces a typed `ios_input_unsupported` error for touch
/// gestures (those genuinely require the gRPC HID surface).
#[cfg(not(feature = "ios-grpc"))]
fn send_hid_applescript(udid: &str, event: HidEvent) -> Result<(), CommandError> {
    use super::input::HardwareButton;
    use super::xcrun::hid_fallback;

    let map_err = |err: std::io::Error| {
        CommandError::user_fixable("ios_input_fallback_failed", err.to_string())
    };

    match event {
        HidEvent::Home => hid_fallback::press_home(udid).map_err(map_err),
        HidEvent::Button { button } => match button {
            HardwareButton::Home => hid_fallback::press_home(udid).map_err(map_err),
            HardwareButton::Lock | HardwareButton::SideButton => {
                hid_fallback::press_lock(udid).map_err(map_err)
            }
            HardwareButton::Siri => hid_fallback::press_siri(udid).map_err(map_err),
            HardwareButton::VolumeUp | HardwareButton::VolumeDown => {
                Err(CommandError::user_fixable(
                    "ios_input_unsupported",
                    "Volume buttons aren't available in this build. Rebuild Xero with \
                     `--features ios-grpc` to route HID through idb_companion.",
                ))
            }
        },
        HidEvent::Text { text } => hid_fallback::type_text(udid, &text).map_err(map_err),
        HidEvent::Touch { .. } | HidEvent::Swipe { .. } => Err(CommandError::user_fixable(
            "ios_input_unsupported",
            "Touch and swipe gestures require idb_companion's HID RPC. Rebuild Xero \
             with `--features ios-grpc` to enable it.",
        )),
    }
}

// ---------- Real gRPC path -------------------------------------------------

#[cfg(feature = "ios-grpc")]
mod grpc_impl {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::NalCallback;

    use tokio::sync::mpsc;
    use tokio::sync::oneshot;
    use tokio_stream::wrappers::ReceiverStream;
    use tokio_stream::StreamExt;
    use tonic::transport::{Channel, Endpoint};

    use crate::commands::emulator::ios::input::{HardwareButton, HidEvent, TouchPhase};
    use crate::commands::CommandError;

    use super::{LogStreamHandle, VideoStreamHandle};

    // tonic-build emits generated code under `OUT_DIR/idb.rs` — we include
    // it under a submodule called `pb` so call-sites stay readable.
    #[allow(clippy::all, warnings)]
    pub(crate) mod pb {
        tonic::include_proto!("idb");
    }

    use pb::companion_service_client::CompanionServiceClient;
    use pb::hid_event::hid_press_action::Action as HidAction;
    use pb::hid_event::{
        Event as HidVariant, HidButton, HidButtonType, HidDirection, HidPress, HidPressAction,
        HidSwipe, HidTouch,
    };
    use pb::video_stream_request::{Control, Format as VideoFormat, Start as VideoStart};
    use pb::{
        AccessibilityInfoRequest, HidEvent as WireHidEvent, LogRequest, Point, VideoStreamRequest,
    };

    /// Tokio runtime + lazily-initialized tonic channel. The channel handshake
    /// is retried on first use so callers don't pay connection cost up front.
    pub struct Runtime {
        rt: Arc<tokio::runtime::Runtime>,
        channel: Channel,
        /// Long-lived sender feeding events into the single open `hid` RPC.
        /// The `hid` call is client-streaming: idb_companion holds its
        /// CoreSimulator HID session open for the lifetime of the stream,
        /// which is the only way a Touch Down + Touch Up pair are seen by
        /// the simulator as one continuous touch. Opening a fresh stream
        /// per event caused every gesture to land in its own disposable
        /// HID session — the simulator would see the Down get cancelled
        /// and the Up arrive with nothing to release, so no tap
        /// registered.
        hid_tx: Mutex<Option<mpsc::Sender<WireHidEvent>>>,
    }

    impl Runtime {
        pub fn connect(grpc_port: u16) -> Self {
            let rt = Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("idb-grpc")
                    .enable_all()
                    .build()
                    .expect("build idb-grpc tokio runtime"),
            );
            let endpoint = Endpoint::from_shared(format!("http://127.0.0.1:{grpc_port}"))
                .expect("valid endpoint")
                .timeout(Duration::from_secs(5))
                .tcp_keepalive(Some(Duration::from_secs(30)));
            let channel = rt.block_on(async { endpoint.connect_lazy() });
            Self {
                rt,
                channel,
                hid_tx: Mutex::new(None),
            }
        }

        fn client(&self) -> CompanionServiceClient<Channel> {
            CompanionServiceClient::new(self.channel.clone())
        }

        /// Return a sender into the long-lived HID stream, opening one if
        /// the previous stream died (e.g. idb_companion restarted). The
        /// background task drives the client-streaming RPC until the
        /// receiver is dropped or the server ends the call.
        fn ensure_hid_stream(&self) -> mpsc::Sender<WireHidEvent> {
            let mut guard = self.hid_tx.lock().expect("idb hid sender mutex");
            if let Some(existing) = guard.as_ref() {
                if !existing.is_closed() {
                    return existing.clone();
                }
            }
            // 64 slots leaves plenty of headroom for a burst of `touch_move`
            // events during a drag without the sync-context sender blocking
            // on backpressure.
            let (tx, rx) = mpsc::channel::<WireHidEvent>(64);
            let mut client = self.client();
            self.rt.spawn(async move {
                let _ = client.hid(ReceiverStream::new(rx)).await;
            });
            let sender = tx.clone();
            *guard = Some(tx);
            sender
        }

        pub fn accessibility_tree(&self) -> Result<serde_json::Value, CommandError> {
            let mut client = self.client();
            let rt = Arc::clone(&self.rt);
            let response = rt
                .block_on(async move {
                    client
                        .accessibility_info(AccessibilityInfoRequest {
                            point: None,
                            format: pb::accessibility_info_request::Format::Nested as i32,
                        })
                        .await
                })
                .map_err(|status| {
                    CommandError::system_fault(
                        "ios_idb_accessibility_failed",
                        format!("idb accessibility_info: {status}"),
                    )
                })?;

            let body = response.into_inner().json;
            serde_json::from_str(&body).map_err(|e| {
                CommandError::system_fault(
                    "ios_idb_accessibility_bad_json",
                    format!("idb accessibility tree json parse: {e}"),
                )
            })
        }

        pub fn send_hid(&self, event: HidEvent) -> Result<(), CommandError> {
            let wire_events = translate_hid_event(event);
            let rt = Arc::clone(&self.rt);

            // Try once with the existing stream; if the sender has been
            // closed (server ended the RPC, connection died), reopen and
            // retry a single time. Anything beyond that is a hard failure
            // — surface as `ios_idb_hid_failed` so the caller can fall
            // back to the Core Graphics path.
            let mut attempts = 0;
            loop {
                let tx = self.ensure_hid_stream();
                let events = wire_events.clone();
                let send_result: Result<(), mpsc::error::SendError<WireHidEvent>> =
                    rt.block_on(async {
                        for ev in events {
                            tx.send(ev).await?;
                        }
                        Ok(())
                    });

                match send_result {
                    Ok(()) => return Ok(()),
                    Err(_) if attempts == 0 => {
                        // Stream died mid-send — drop the stale sender so
                        // `ensure_hid_stream` opens a fresh RPC next pass.
                        self.hid_tx.lock().expect("idb hid sender mutex").take();
                        attempts += 1;
                    }
                    Err(_) => {
                        return Err(CommandError::system_fault(
                            "ios_idb_hid_failed",
                            "idb hid stream closed; reconnect attempt failed".to_string(),
                        ));
                    }
                }
            }
        }

        pub fn start_video_stream(
            &self,
            fps: u32,
            mut on_nal: NalCallback,
        ) -> Result<VideoStreamHandle, CommandError> {
            let mut client = self.client();
            let rt = Arc::clone(&self.rt);
            let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

            rt.spawn(async move {
                let (req_tx, req_rx) = mpsc::channel::<VideoStreamRequest>(2);
                let start = VideoStart {
                    file_path: String::new(),
                    fps: fps as u64,
                    format: VideoFormat::H264 as i32,
                    compression_quality: 0.8,
                    scale_factor: 1.0,
                    avg_bitrate: 4_000_000.0,
                    key_frame_rate: 1.0,
                };
                let _ = req_tx
                    .send(VideoStreamRequest {
                        control: Some(Control::Start(start)),
                    })
                    .await;

                let response = match client.video_stream(ReceiverStream::new(req_rx)).await {
                    Ok(resp) => resp,
                    Err(_) => return,
                };
                let mut stream = response.into_inner();

                tokio::pin!(cancel_rx);
                loop {
                    tokio::select! {
                        _ = &mut cancel_rx => break,
                        msg = stream.next() => {
                            match msg {
                                Some(Ok(resp)) => {
                                    if let Some(pb::video_stream_response::Output::Payload(payload)) = resp.output {
                                        if let Some(pb::payload::Source::Data(bytes)) = payload.source {
                                            on_nal(&bytes);
                                        }
                                    }
                                }
                                Some(Err(_)) | None => break,
                            }
                        }
                    }
                }

                let _ = req_tx
                    .send(VideoStreamRequest {
                        control: Some(Control::Stop(Default::default())),
                    })
                    .await;
            });

            Ok(VideoStreamHandle { cancel: cancel_tx })
        }

        pub fn start_log_stream(
            &self,
            mut on_line: Box<dyn FnMut(&str) + Send>,
        ) -> Result<LogStreamHandle, CommandError> {
            let mut client = self.client();
            let rt = Arc::clone(&self.rt);
            let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

            rt.spawn(async move {
                let response = match client
                    .log(LogRequest {
                        arguments: Vec::new(),
                        source: pb::log_request::Source::Target as i32,
                    })
                    .await
                {
                    Ok(resp) => resp,
                    Err(_) => return,
                };
                let mut stream = response.into_inner();
                let mut remainder = String::new();

                tokio::pin!(cancel_rx);
                loop {
                    tokio::select! {
                        _ = &mut cancel_rx => break,
                        msg = stream.next() => {
                            match msg {
                                Some(Ok(resp)) => {
                                    let chunk = String::from_utf8_lossy(&resp.output);
                                    remainder.push_str(&chunk);
                                    while let Some(newline) = remainder.find('\n') {
                                        let line = remainder[..newline].to_string();
                                        remainder.drain(..=newline);
                                        on_line(&line);
                                    }
                                }
                                Some(Err(_)) | None => break,
                            }
                        }
                    }
                }
                if !remainder.is_empty() {
                    on_line(&remainder);
                }
            });

            Ok(LogStreamHandle { cancel: cancel_tx })
        }
    }

    fn translate_hid_event(event: HidEvent) -> Vec<WireHidEvent> {
        match event {
            HidEvent::Touch { phase, x, y } => {
                let direction = match phase {
                    TouchPhase::Began | TouchPhase::Moved => HidDirection::Down,
                    TouchPhase::Ended | TouchPhase::Cancelled => HidDirection::Up,
                };
                let press = HidPress {
                    action: Some(HidPressAction {
                        action: Some(HidAction::Touch(HidTouch {
                            point: Some(Point {
                                x: x as f64,
                                y: y as f64,
                            }),
                        })),
                    }),
                    direction: direction as i32,
                };
                vec![WireHidEvent {
                    event: Some(HidVariant::Press(press)),
                }]
            }
            HidEvent::Swipe {
                from_x,
                from_y,
                to_x,
                to_y,
                duration_ms,
            } => {
                let swipe = HidSwipe {
                    start: Some(Point {
                        x: from_x as f64,
                        y: from_y as f64,
                    }),
                    end: Some(Point {
                        x: to_x as f64,
                        y: to_y as f64,
                    }),
                    delta: 0.0,
                    duration: duration_ms as f64 / 1000.0,
                };
                vec![WireHidEvent {
                    event: Some(HidVariant::Swipe(swipe)),
                }]
            }
            HidEvent::Text { text } => {
                let mut out = Vec::new();
                for ch in text.chars() {
                    for direction in [HidDirection::Down, HidDirection::Up] {
                        out.push(WireHidEvent {
                            event: Some(HidVariant::Press(HidPress {
                                action: Some(HidPressAction {
                                    action: Some(HidAction::Key(pb::hid_event::HidKey {
                                        keycode: ch as u64,
                                    })),
                                }),
                                direction: direction as i32,
                            })),
                        });
                    }
                }
                out
            }
            HidEvent::Button { button } => {
                let wire_button = match button {
                    HardwareButton::Home => HidButtonType::Home,
                    HardwareButton::Lock => HidButtonType::Lock,
                    HardwareButton::VolumeUp | HardwareButton::VolumeDown => HidButtonType::Lock,
                    HardwareButton::Siri => HidButtonType::Siri,
                    HardwareButton::SideButton => HidButtonType::SideButton,
                };
                let make = |direction: HidDirection| WireHidEvent {
                    event: Some(HidVariant::Press(HidPress {
                        action: Some(HidPressAction {
                            action: Some(HidAction::Button(HidButton {
                                button: wire_button as i32,
                            })),
                        }),
                        direction: direction as i32,
                    })),
                };
                vec![make(HidDirection::Down), make(HidDirection::Up)]
            }
            HidEvent::Home => {
                let make = |direction: HidDirection| WireHidEvent {
                    event: Some(HidVariant::Press(HidPress {
                        action: Some(HidPressAction {
                            action: Some(HidAction::Button(HidButton {
                                button: HidButtonType::Home as i32,
                            })),
                        }),
                        direction: direction as i32,
                    })),
                };
                vec![make(HidDirection::Down), make(HidDirection::Up)]
            }
        }
    }
}
