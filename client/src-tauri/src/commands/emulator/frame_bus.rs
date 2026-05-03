use std::sync::OnceLock;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Instant;

use arc_swap::ArcSwapOption;
use tauri::{AppHandle, Emitter, Runtime};

use super::events::{FramePayload, EMULATOR_FRAME_EVENT};

const FRAME_EVENT_MIN_INTERVAL_NS: u64 = 33_000_000;

/// A single rendered frame ready to be served by the `emulator://frame` URI
/// scheme. `bytes` is JPEG-encoded so the webview can paint it with its
/// native image decoder.
#[derive(Debug, Clone)]
pub struct Frame {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    pub bytes: Arc<Vec<u8>>,
}

/// Single-slot frame buffer shared between the producer (sidecar driver) and
/// consumer (URI scheme handler).
///
/// The producer writes the latest frame with `publish`; the consumer reads it
/// with `latest`. Readers always see a complete frame — `ArcSwapOption`
/// guarantees the pointer swap is atomic, so a concurrent reader will either
/// see the previous frame or the new one, never a partially-written one.
pub struct FrameBus {
    latest: ArcSwapOption<Frame>,
    last_event_ns: AtomicU64,
    seq: AtomicU64,
}

impl FrameBus {
    pub fn new() -> Self {
        Self {
            latest: ArcSwapOption::empty(),
            last_event_ns: AtomicU64::new(0),
            seq: AtomicU64::new(0),
        }
    }

    /// Returns the most recently published frame, if any.
    pub fn latest(&self) -> Option<Arc<Frame>> {
        self.latest.load_full()
    }

    /// Publish a new frame. Returns the sequence number assigned to it.
    /// The caller is responsible for emitting the `emulator:frame` event —
    /// usually via [`FrameBus::publish_and_emit`].
    pub fn publish(&self, width: u32, height: u32, bytes: Vec<u8>) -> u64 {
        let seq = self.seq.fetch_add(1, Ordering::Release).wrapping_add(1);
        let frame = Arc::new(Frame {
            seq,
            width,
            height,
            bytes: Arc::new(bytes),
        });
        self.latest.store(Some(frame));
        seq
    }

    /// Clear the bus. Called on `emulator_stop` so stale frames don't leak
    /// across sessions.
    pub fn clear(&self) {
        self.latest.store(None);
        self.last_event_ns.store(0, Ordering::Release);
    }

    fn should_emit_frame_event(&self) -> bool {
        self.should_emit_frame_event_at(monotonic_now_ns())
    }

    fn should_emit_frame_event_at(&self, now_ns: u64) -> bool {
        let now_ns = now_ns.max(1);
        let mut last_ns = self.last_event_ns.load(Ordering::Acquire);

        loop {
            if last_ns != 0 && now_ns.saturating_sub(last_ns) < FRAME_EVENT_MIN_INTERVAL_NS {
                return false;
            }

            match self.last_event_ns.compare_exchange(
                last_ns,
                now_ns,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(next_last_ns) => last_ns = next_last_ns,
            }
        }
    }
}

impl Default for FrameBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Publish a frame and rate-limit `emulator:frame` events so the frontend can
/// swap its `<img src>` without flooding WebKit's custom scheme machinery.
pub fn publish_and_emit<R: Runtime>(
    app: &AppHandle<R>,
    bus: &FrameBus,
    width: u32,
    height: u32,
    bytes: Vec<u8>,
) -> u64 {
    let seq = bus.publish(width, height, bytes);
    if !bus.should_emit_frame_event() {
        return seq;
    }

    if let Err(err) = app.emit(EMULATOR_FRAME_EVENT, FramePayload { seq, width, height }) {
        // We don't have a structured log surface here; stderr is enough
        // to diagnose the "never see the device" class of bug where the
        // webview bridge is dropping events before the listener attaches.
        eprintln!("[emulator] frame emit failed (seq {seq}): {err}");
    }
    seq
}

fn monotonic_now_ns() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let elapsed = START.get_or_init(Instant::now).elapsed().as_nanos();
    elapsed.min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_assigns_monotonic_seq() {
        let bus = FrameBus::new();
        assert_eq!(bus.publish(1, 1, vec![0]), 1);
        assert_eq!(bus.publish(1, 1, vec![0]), 2);
        assert_eq!(bus.publish(1, 1, vec![0]), 3);
    }

    #[test]
    fn latest_returns_most_recent_frame() {
        let bus = FrameBus::new();
        assert!(bus.latest().is_none());

        bus.publish(640, 480, vec![1, 2, 3]);
        bus.publish(640, 480, vec![4, 5, 6]);
        let latest = bus.latest().expect("latest frame");
        assert_eq!(latest.seq, 2);
        assert_eq!(latest.bytes.as_slice(), &[4, 5, 6]);
    }

    #[test]
    fn clear_removes_stored_frame() {
        let bus = FrameBus::new();
        bus.publish(1, 1, vec![0]);
        bus.clear();
        assert!(bus.latest().is_none());
    }

    #[test]
    fn frame_events_are_rate_limited() {
        let bus = FrameBus::new();

        assert!(bus.should_emit_frame_event_at(1));
        assert!(!bus.should_emit_frame_event_at(1 + FRAME_EVENT_MIN_INTERVAL_NS - 1));
        assert!(bus.should_emit_frame_event_at(1 + FRAME_EVENT_MIN_INTERVAL_NS));
    }

    #[test]
    fn rate_limited_bursts_still_keep_latest_frame() {
        let bus = FrameBus::new();

        assert_eq!(bus.publish(320, 240, vec![1]), 1);
        assert!(bus.should_emit_frame_event_at(1));

        for value in 2u8..=50 {
            let seq = bus.publish(320, 240, vec![value]);
            assert_eq!(seq, u64::from(value));
            assert!(!bus.should_emit_frame_event_at(1 + u64::from(value)));
        }

        let latest = bus.latest().expect("latest frame");
        assert_eq!(latest.seq, 50);
        assert_eq!(latest.bytes.as_slice(), &[50]);
    }

    #[test]
    fn clear_resets_frame_event_rate_limit() {
        let bus = FrameBus::new();

        assert!(bus.should_emit_frame_event_at(1));
        assert!(!bus.should_emit_frame_event_at(2));

        bus.clear();

        assert!(bus.should_emit_frame_event_at(2));
    }
}
