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

#![cfg(target_os = "macos")]

use std::time::Duration;

use crate::commands::CommandError;

use super::input::HidEvent;

/// Stable, frontend-exposed handle to a running idb_companion.
pub struct IdbClient {
    grpc_port: u16,
    #[cfg(feature = "ios-grpc")]
    inner: grpc_impl::Runtime,
}

impl IdbClient {
    pub fn new(grpc_port: u16) -> Self {
        #[cfg(feature = "ios-grpc")]
        {
            let inner = grpc_impl::Runtime::connect(grpc_port);
            Self { grpc_port, inner }
        }
        #[cfg(not(feature = "ios-grpc"))]
        {
            Self { grpc_port }
        }
    }

    pub fn grpc_port(&self) -> u16 {
        self.grpc_port
    }

    /// Open a bidirectional `VideoStream` RPC and push raw H.264 NAL
    /// units into a callback. Returns a handle the caller drops to
    /// cancel the stream.
    pub fn start_video_stream(
        &self,
        fps: u32,
        on_nal: Box<dyn FnMut(&[u8]) + Send>,
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

    /// Send a single HID event over the bidirectional HID RPC. We open
    /// one short-lived stream per event; idb accepts this pattern and
    /// it keeps the call signature synchronous.
    pub fn send_hid(&self, event: HidEvent) -> Result<(), CommandError> {
        #[cfg(feature = "ios-grpc")]
        {
            self.inner.send_hid(event)
        }
        #[cfg(not(feature = "ios-grpc"))]
        {
            let _ = event;
            Err(grpc_unimplemented(
                "HID.inject",
                "enable the `ios-grpc` Cargo feature to link the vendored idb proto",
            ))
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
        format!("idb gRPC `{method}` is not yet wired up in this Cadence build. {detail}."),
    )
}

// ---------- Real gRPC path -------------------------------------------------

#[cfg(feature = "ios-grpc")]
mod grpc_impl {
    use std::sync::Arc;
    use std::time::Duration;

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
            Self { rt, channel }
        }

        fn client(&self) -> CompanionServiceClient<Channel> {
            CompanionServiceClient::new(self.channel.clone())
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
            let mut client = self.client();
            let rt = Arc::clone(&self.rt);
            let wire_events = translate_hid_event(event);

            rt.block_on(async move {
                let (tx, rx) = mpsc::channel::<WireHidEvent>(4);
                for ev in wire_events {
                    let _ = tx.send(ev).await;
                }
                drop(tx);
                client
                    .hid(ReceiverStream::new(rx))
                    .await
                    .map(|_| ())
                    .map_err(|status| {
                        CommandError::system_fault(
                            "ios_idb_hid_failed",
                            format!("idb hid: {status}"),
                        )
                    })
            })
        }

        pub fn start_video_stream(
            &self,
            fps: u32,
            mut on_nal: Box<dyn FnMut(&[u8]) + Send>,
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
                                    if let Some(output) = resp.output {
                                        if let pb::video_stream_response::Output::Payload(payload) = output {
                                            if let Some(pb::payload::Source::Data(bytes)) = payload.source {
                                                on_nal(&bytes);
                                            }
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
