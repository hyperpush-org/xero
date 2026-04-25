//! iOS HID input encoding.
//!
//! Mirrors the subset of `idb.proto`'s `HIDEvent` union that we drive from
//! the sidebar and the automation surface. Once the proto is vendored these
//! types map 1:1 to the generated `HIDEvent` oneof variants.
//!
//! The normalized-coords helper keeps the upper-level API identical between
//! Android and iOS: callers provide `(x, y) in [0, 1]`, we translate to
//! device-pixels using the latest known simulator dimensions.

use serde::Serialize;

/// Hardware buttons we expose to the sidebar and agents. Values don't map
/// directly to idb integers (the proto uses an enum) — we normalize so the
/// frontend never has to know platform specifics.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareButton {
    Home,
    Lock,
    VolumeUp,
    VolumeDown,
    Siri,
    SideButton,
}

/// Simulator touch phases. Matches UIKit's `UITouch.Phase` semantics.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchPhase {
    Began,
    Moved,
    Ended,
    Cancelled,
}

/// Serialized form of the HID RPC payload. The frontend treats this as
/// opaque; the backend will forward it through `idb_client::send_hid`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum HidEvent {
    /// Touch event at device-pixel `(x, y)` with the given phase.
    Touch { phase: TouchPhase, x: i32, y: i32 },
    /// Two-finger swipe — represented as a pair of touch events delivered
    /// together to produce a simultaneous gesture.
    Swipe {
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        duration_ms: u32,
    },
    /// UTF-8 text for keyboard injection.
    Text { text: String },
    /// Hardware button press.
    Button { button: HardwareButton },
    /// Home-indicator swipe-up gesture — equivalent to the home button on
    /// Face ID devices.
    Home,
}

/// Translate a normalized point `(0..1, 0..1)` to simulator device pixels.
pub fn denormalize(x: f32, y: f32, width: u32, height: u32) -> (i32, i32) {
    let px = (x.clamp(0.0, 1.0) * width as f32).round() as i32;
    let py = (y.clamp(0.0, 1.0) * height as f32).round() as i32;
    (
        px.min(width.saturating_sub(1) as i32).max(0),
        py.min(height.saturating_sub(1) as i32).max(0),
    )
}

/// Map a cross-platform hardware key name (e.g. `"home"`, `"lock"`) to an
/// iOS button. Returns `None` for Android-only keys like `"back"`.
pub fn parse_hardware_button(name: &str) -> Option<HardwareButton> {
    match name {
        "home" => Some(HardwareButton::Home),
        "lock" | "power" | "side" | "side_button" => Some(HardwareButton::SideButton),
        "vol_up" | "volume_up" => Some(HardwareButton::VolumeUp),
        "vol_down" | "volume_down" => Some(HardwareButton::VolumeDown),
        "siri" => Some(HardwareButton::Siri),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denormalize_clamps_and_rounds() {
        assert_eq!(denormalize(0.5, 0.5, 100, 200), (50, 100));
        assert_eq!(denormalize(-0.1, 1.5, 100, 200), (0, 199));
        assert_eq!(denormalize(1.0, 0.0, 100, 200), (99, 0));
    }

    #[test]
    fn parse_hardware_button_covers_common_names() {
        assert!(matches!(
            parse_hardware_button("home"),
            Some(HardwareButton::Home)
        ));
        assert!(matches!(
            parse_hardware_button("lock"),
            Some(HardwareButton::SideButton)
        ));
        assert!(parse_hardware_button("back").is_none());
    }
}
