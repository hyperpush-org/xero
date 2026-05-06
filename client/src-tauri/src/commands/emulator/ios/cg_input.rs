//! Core Graphics input delivery for the iOS Simulator.
//!
//! Posts synthetic mouse, scroll, and keyboard events directly to
//! Simulator.app's process via `CGEventPostToPid`, bypassing the need
//! for `idb_companion`'s gRPC HID surface. Requires the user to grant
//! Accessibility permission to Xero (System Settings → Privacy &
//! Security → Accessibility).
//!
//! ## Coordinate system
//!
//! `CGPoint` / `CGEventPostToPid` take global-display coordinates with
//! a top-left origin in points — the same space AppKit / AX use. We
//! resolve Simulator.app's window bounds via `CGWindowListCopyWindowInfo`,
//! subtract the standard macOS titlebar height to get the device view,
//! then map normalized sidebar coordinates (0..1) onto that rect.
//!
//! ## Why `post_to_pid` and not `post`?
//!
//! Xero's own window sits on top of Simulator.app in the z-order
//! (that's why the sidebar shows screenshots, not the real window), so
//! a globally-posted event would hit Xero. `post_to_pid` bypasses
//! window-server hit-testing and delivers the event straight into the
//! target process's queue, which then routes it against its own
//! windows — regardless of whether those windows are visible.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType, ToVoid};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::{CGPoint, CGRect, CGSize};

use super::input::{HardwareButton, TouchPhase};
use crate::commands::CommandError;

/// How long a resolved Simulator window rect stays valid before we rescan.
/// Short enough that the user can drag Simulator.app and we pick up the new
/// position within a quarter-second; long enough that a burst of
/// `touch_move` events doesn't trigger a `CGWindowListCopyWindowInfo` call
/// per frame.
const CACHE_TTL: Duration = Duration::from_millis(250);

/// Standard macOS titled-window title-bar height. Simulator.app renders
/// the device view immediately below the title bar with no inset. The
/// value has been stable at 28pt across every macOS release since 11.
const TITLE_BAR_HEIGHT: f64 = 28.0;

#[derive(Debug, Clone, Copy)]
struct WindowInfo {
    pid: libc::pid_t,
    /// Device-viewport rect in screen coordinates, top-left origin, points.
    device_rect: CGRect,
}

struct Cache {
    /// Keyed by device display name; on device switch the cache naturally
    /// misses rather than returning a stale hit.
    entry: Option<(String, WindowInfo, Instant)>,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache { entry: None });

pub fn invalidate_cache() {
    if let Ok(mut cache) = CACHE.lock() {
        cache.entry = None;
    }
}

/// `true` when Xero has been granted Accessibility permission (which
/// macOS requires for `CGEventPostToPid` to actually deliver events to
/// another process). Checked without triggering any UI — safe to poll.
pub fn ax_permission_granted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// `true` when Xero has been granted Screen Recording permission (required
/// by ScreenCaptureKit to capture the Simulator window). Checked without
/// triggering any UI — safe to poll.
pub fn screen_recording_permission_granted() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Trigger macOS's Screen Recording permission prompt. Returns the
/// permission state after the call. On macOS 15+ this opens System
/// Settings → Privacy & Security → Screen Recording. Note: unlike the
/// AX prompt, this may require an app restart to take effect.
pub fn request_screen_recording_permission() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

/// Trigger macOS's Accessibility permission prompt. If Xero is already
/// registered in System Settings → Privacy & Security → Accessibility,
/// this is a no-op and returns the current state. Otherwise a system
/// dialog appears ("Xero would like to control this computer using
/// accessibility features") with a button that deep-links to the settings
/// pane. Returns the permission state *after* the call — will still be
/// `false` if the user just dismissed the prompt, since granting
/// requires a toggle in System Settings.
pub fn request_ax_permission() -> bool {
    // The option key is a CFStringRef constant exported from
    // HIServices.framework, but its string value is stable
    // ("AXTrustedCheckOptionPrompt") and CFDictionary compares keys by
    // content, so a CFString with the same literal works identically and
    // spares us an extern-static binding.
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let value = CFBoolean::true_value();
    let pairs = [(key.as_CFType(), value.as_CFType())];
    let options = CFDictionary::from_CFType_pairs(&pairs);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

/// Gate input dispatch on Accessibility permission. Without it,
/// `CGEventPostToPid` silently drops every event — returning a typed
/// error here gives the frontend something concrete to react to instead
/// of the user staring at a frozen sidebar.
fn require_ax_permission() -> Result<(), CommandError> {
    if ax_permission_granted() {
        return Ok(());
    }
    Err(CommandError::user_fixable(
        "ios_ax_permission_denied",
        "Xero needs Accessibility permission to drive the iOS Simulator. \
         Open System Settings → Privacy & Security → Accessibility and \
         enable Xero, then try again.",
    ))
}

/// Post a touch event at normalized `(nx, ny)` on the simulator viewport.
///
/// On macOS 26 + Xcode 26 the raw `CGEventPostToPid` path is silently
/// dropped (the events never show up in SpringBoard's event delivery
/// logs), and the bundled idb_companion's CoreSim HID bridge is broken
/// for the same SDK. What still works reliably is `System Events`'
/// process-targeted `click at {x, y}`, which dispatches through the
/// Accessibility API — iOS treats an AX `press` action on a home-screen
/// icon or UIKit button as a tap, so the user can still open apps,
/// press buttons, enter text fields, etc.
///
/// The trade-off is that non-AX-exposed targets (games, custom-drawn
/// views, arbitrary pixel positions) won't register a tap — there's no
/// element under the coordinates for AX to press. For those, the user
/// falls back to clicking the real Simulator.app window.
///
/// Only `Began` triggers a click; `Moved`/`Ended`/`Cancelled` are
/// no-ops because AppleScript's `click at` is atomic (down + up in one
/// synthesized event), so re-firing on `Ended` would double-tap.
pub fn send_touch(
    device_name: &str,
    phase: TouchPhase,
    nx: f32,
    ny: f32,
) -> Result<(), CommandError> {
    require_ax_permission()?;
    let info = resolve_window(device_name)?;
    let point = normalized_to_screen(&info, nx, ny);

    match phase {
        TouchPhase::Began => click_via_applescript(point),
        TouchPhase::Moved | TouchPhase::Ended | TouchPhase::Cancelled => Ok(()),
    }
}

/// Dispatch a one-shot click at screen-space `point` by handing
/// `System Events` the coordinate and letting it resolve the AX element
/// under it. Requires Accessibility permission (already checked by the
/// caller) and for the Simulator window to expose the target via AX.
fn click_via_applescript(point: CGPoint) -> Result<(), CommandError> {
    use std::process::Command;
    // `tell process "Simulator" to click at {x, y}` works even when the
    // Simulator window is obscured by Xero — the AX tree is process-
    // scoped, not window-server scoped. Rounding to i64 avoids surfacing
    // AppleScript `{12.5, 30.1}` literals which parse inconsistently
    // across macOS releases.
    let x = point.x.round() as i64;
    let y = point.y.round() as i64;
    let script = format!(
        r#"tell application "System Events"
  tell process "Simulator"
    click at {{{x}, {y}}}
  end tell
end tell"#
    );
    let output = Command::new("osascript").arg("-e").arg(&script).output();
    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            // 1743 / "not authorized" is AppleScript's typed code for
            // missing Accessibility permission. Surface it the same way
            // the CGEvent path does so the existing sidebar banner
            // triggers instead of a generic error.
            if stderr.contains("1743") || stderr.to_lowercase().contains("not authorized") {
                Err(CommandError::user_fixable(
                    "ios_ax_permission_denied",
                    "Xero needs Accessibility permission to drive the iOS Simulator. \
                     Open System Settings → Privacy & Security → Accessibility and enable \
                     Xero, then try again.",
                ))
            } else {
                Err(CommandError::system_fault(
                    "ios_ax_click_failed",
                    format!("osascript click failed: {stderr}"),
                ))
            }
        }
        Err(err) => Err(CommandError::system_fault(
            "ios_ax_click_failed",
            format!("could not invoke osascript: {err}"),
        )),
    }
}

/// Post a drag gesture from `(from_*)` to `(to_*)` over `duration_ms` ms.
/// Interpolates intermediate points so gesture recognizers read it as a
/// real swipe instead of a teleport.
pub fn send_swipe(
    device_name: &str,
    from_nx: f32,
    from_ny: f32,
    to_nx: f32,
    to_ny: f32,
    duration_ms: u32,
) -> Result<(), CommandError> {
    require_ax_permission()?;
    let info = resolve_window(device_name)?;
    let from = normalized_to_screen(&info, from_nx, from_ny);
    let to = normalized_to_screen(&info, to_nx, to_ny);
    // Aim for ~60 Hz sampling during the gesture.
    let total = duration_ms.max(50);
    let steps = ((total / 16).max(6)) as usize;
    let step_sleep = Duration::from_millis((total as u64 / steps as u64).max(1));

    post_mouse(info.pid, from, CGEventType::LeftMouseDown)?;

    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let p = CGPoint::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        post_mouse(info.pid, p, CGEventType::LeftMouseDragged)?;
        std::thread::sleep(step_sleep);
    }

    post_mouse(info.pid, to, CGEventType::LeftMouseUp)?;
    Ok(())
}

/// Inject a Unicode text payload as keyboard events. Uses
/// `CGEventKeyboardSetUnicodeString` so we don't have to translate into
/// virtual keycodes — Simulator.app's text fields accept whatever Unicode
/// string the event carries.
pub fn send_text(device_name: &str, text: &str) -> Result<(), CommandError> {
    if text.is_empty() {
        return Ok(());
    }
    require_ax_permission()?;
    let info = resolve_window(device_name)?;

    for ch in text.chars() {
        // Encode this char as UTF-16 (up to 2 code units for astral chars).
        let mut buf = [0u16; 2];
        let encoded = ch.encode_utf16(&mut buf);

        let source_down = event_source()?;
        let down = CGEvent::new_keyboard_event(source_down, 0, true).map_err(|_| keyboard_err())?;
        down.set_string_from_utf16_unchecked(encoded);
        down.post_to_pid(info.pid);

        let source_up = event_source()?;
        let up = CGEvent::new_keyboard_event(source_up, 0, false).map_err(|_| keyboard_err())?;
        up.set_string_from_utf16_unchecked(encoded);
        up.post_to_pid(info.pid);
    }
    Ok(())
}

/// Press a hardware button by driving the Simulator.app menu shortcut that
/// triggers it. Volume buttons aren't bound to a shortcut, so they return
/// `ios_input_unsupported` — matching the existing AppleScript fallback
/// behaviour.
pub fn send_hardware_button(device_name: &str, button: HardwareButton) -> Result<(), CommandError> {
    require_ax_permission()?;
    // Virtual keycodes (Carbon/HIToolbox): kVK_ANSI_H = 4, kVK_ANSI_L = 37,
    // kVK_ANSI_S = 1. Stable since OS X.
    let (keycode, flags) = match button {
        HardwareButton::Home => (
            4_u16,
            CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagShift,
        ),
        HardwareButton::Lock | HardwareButton::SideButton => {
            (37_u16, CGEventFlags::CGEventFlagCommand)
        }
        HardwareButton::Siri => (
            1_u16,
            CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagShift,
        ),
        HardwareButton::VolumeUp | HardwareButton::VolumeDown => {
            return Err(CommandError::user_fixable(
                "ios_input_unsupported",
                "Volume buttons aren't available through the CGEvent input path. \
                 Install idb_companion to route volume HID events.",
            ));
        }
    };

    let info = resolve_window(device_name)?;
    post_key(info.pid, keycode, flags, true)?;
    post_key(info.pid, keycode, flags, false)?;
    Ok(())
}

/// Convenience: the Home-indicator swipe-up gesture maps to the same
/// menu shortcut as the Home button on every Simulator release.
pub fn send_home(device_name: &str) -> Result<(), CommandError> {
    send_hardware_button(device_name, HardwareButton::Home)
}

fn post_mouse(
    pid: libc::pid_t,
    point: CGPoint,
    event_type: CGEventType,
) -> Result<(), CommandError> {
    let source = event_source()?;
    let event =
        CGEvent::new_mouse_event(source, event_type, point, CGMouseButton::Left).map_err(|_| {
            CommandError::system_fault(
                "ios_cg_mouse_event_failed",
                "Could not build a CGEvent mouse event.",
            )
        })?;
    event.post_to_pid(pid);
    Ok(())
}

fn post_key(
    pid: libc::pid_t,
    keycode: u16,
    flags: CGEventFlags,
    down: bool,
) -> Result<(), CommandError> {
    let source = event_source()?;
    let event = CGEvent::new_keyboard_event(source, keycode, down).map_err(|_| keyboard_err())?;
    event.set_flags(flags);
    event.post_to_pid(pid);
    Ok(())
}

fn keyboard_err() -> CommandError {
    CommandError::system_fault(
        "ios_cg_key_event_failed",
        "Could not build a CGEvent keyboard event.",
    )
}

fn event_source() -> Result<CGEventSource, CommandError> {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
        CommandError::system_fault(
            "ios_cg_event_source_failed",
            "Could not create a CGEventSource. Grant Accessibility permission to Xero \
             (System Settings → Privacy & Security → Accessibility) and try again.",
        )
    })
}

fn normalized_to_screen(info: &WindowInfo, nx: f32, ny: f32) -> CGPoint {
    let nx = nx.clamp(0.0, 1.0) as f64;
    let ny = ny.clamp(0.0, 1.0) as f64;
    CGPoint::new(
        info.device_rect.origin.x + nx * info.device_rect.size.width,
        info.device_rect.origin.y + ny * info.device_rect.size.height,
    )
}

fn resolve_window(device_name: &str) -> Result<WindowInfo, CommandError> {
    let now = Instant::now();
    {
        let cache = CACHE.lock().expect("cg_input cache poisoned");
        if let Some((ref key, info, stale_at)) = cache.entry {
            if key == device_name && now < stale_at {
                return Ok(info);
            }
        }
    }

    let found = find_window(device_name).ok_or_else(|| {
        CommandError::user_fixable(
            "ios_simulator_window_not_found",
            format!(
                "Could not find the Simulator.app window for `{device_name}`. \
                 Make sure the iOS Simulator window is open — it can be positioned \
                 anywhere on screen (including behind Xero), but it must not be \
                 minimized or hidden by ⌘H."
            ),
        )
    })?;

    if let Ok(mut cache) = CACHE.lock() {
        cache.entry = Some((device_name.to_string(), found, now + CACHE_TTL));
    }
    Ok(found)
}

fn find_window(device_name: &str) -> Option<WindowInfo> {
    // `kCGWindowListOptionOnScreenOnly` excludes minimized / hidden windows
    // but includes those merely *obscured* by a window from another app —
    // exactly what we need so the user can bury Simulator.app behind
    // Xero.
    const KCG_ON_SCREEN_ONLY: u32 = 1 << 0;
    const KCG_EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
    let options = KCG_ON_SCREEN_ONLY | KCG_EXCLUDE_DESKTOP_ELEMENTS;

    // The raw CGWindowList API returns an array of CFDictionaries whose
    // keys are CFStrings and values are CFTypes. We keep the array's
    // type parameters at the default `(*const c_void, *const c_void)` so
    // we don't fight `ConcreteCFType` bounds — every access goes through
    // a small `find_*` helper that downcasts via CFType.
    let list: CFArray<CFDictionary> = unsafe {
        let raw = CGWindowListCopyWindowInfo(options, 0);
        if raw.is_null() {
            return None;
        }
        CFArray::wrap_under_create_rule(raw)
    };

    // Keep a fallback — the first Simulator-owned normal window we saw —
    // in case the window-title match fails (different Simulator.app locales,
    // beta builds renaming titles, etc.).
    let mut fallback: Option<WindowInfo> = None;

    for i in 0..list.len() {
        let dict = list.get(i)?;

        let owner = find_string(&dict, "kCGWindowOwnerName")?;
        if owner != "Simulator" {
            continue;
        }

        // Filter out Simulator.app's auxiliary windows (menubar helpers,
        // window-server shadows, etc.). Layer 0 is the normal application
        // window layer.
        let layer = find_i64(&dict, "kCGWindowLayer").unwrap_or(-1);
        if layer != 0 {
            continue;
        }

        let pid = find_i64(&dict, "kCGWindowOwnerPID")? as libc::pid_t;
        let bounds = find_bounds(&dict, "kCGWindowBounds")?;
        let device_rect = CGRect::new(
            &CGPoint::new(bounds.origin.x, bounds.origin.y + TITLE_BAR_HEIGHT),
            &CGSize::new(
                bounds.size.width,
                (bounds.size.height - TITLE_BAR_HEIGHT).max(0.0),
            ),
        );

        // Simulator windows without a size (0x0) show up occasionally for
        // the app's off-screen helper windows — discard them.
        if device_rect.size.width < 10.0 || device_rect.size.height < 10.0 {
            continue;
        }

        let info = WindowInfo { pid, device_rect };
        let title_match = find_string(&dict, "kCGWindowName")
            .map(|title| title_head_matches(&title, device_name))
            .unwrap_or(false);
        if title_match {
            return Some(info);
        }
        if fallback.is_none() {
            fallback = Some(info);
        }
    }

    fallback
}

fn title_head_matches(title: &str, device_name: &str) -> bool {
    // Simulator titles look like "iPhone 17 Pro — iOS 26.0" (em-dash) or
    // occasionally "iPhone 17 Pro - iOS 26.0". Compare only the portion
    // before the separator so we don't accidentally match
    // "iPhone 17 Pro" against "iPhone 17 Pro Max".
    let head = title
        .split(['\u{2014}', '\u{2013}', '-'])
        .next()
        .unwrap_or("")
        .trim();
    head == device_name
}

fn find_cf(dict: &CFDictionary, key: &str) -> Option<CFType> {
    let key_cf = CFString::new(key);
    let raw = dict.find(key_cf.to_void())?;
    let type_ref = *raw as CFTypeRef;
    if type_ref.is_null() {
        return None;
    }
    unsafe { Some(CFType::wrap_under_get_rule(type_ref)) }
}

fn find_string(dict: &CFDictionary, key: &str) -> Option<String> {
    find_cf(dict, key)?
        .downcast::<CFString>()
        .map(|s| s.to_string())
}

fn find_i64(dict: &CFDictionary, key: &str) -> Option<i64> {
    find_cf(dict, key)?
        .downcast::<CFNumber>()
        .and_then(|n| n.to_i64())
}

fn find_f64(dict: &CFDictionary, key: &str) -> Option<f64> {
    find_cf(dict, key)?
        .downcast::<CFNumber>()
        .and_then(|n| n.to_f64())
}

fn find_bounds(dict: &CFDictionary, key: &str) -> Option<CGRect> {
    let value = find_cf(dict, key)?;
    let bounds = value.downcast::<CFDictionary>()?;
    let x = find_f64(&bounds, "X")?;
    let y = find_f64(&bounds, "Y")?;
    let w = find_f64(&bounds, "Width")?;
    let h = find_f64(&bounds, "Height")?;
    Some(CGRect::new(&CGPoint::new(x, y), &CGSize::new(w, h)))
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CFArrayRef;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

// Accessibility permission lives in HIServices.framework, which is
// re-exported from ApplicationServices.framework. Linking the umbrella
// keeps us compatible across macOS SDKs that have moved the symbols
// between subframeworks.
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_maps_to_device_rect() {
        let info = WindowInfo {
            pid: 0,
            device_rect: CGRect::new(&CGPoint::new(100.0, 200.0), &CGSize::new(300.0, 500.0)),
        };
        let p = normalized_to_screen(&info, 0.0, 0.0);
        assert!((p.x - 100.0).abs() < 1e-6);
        assert!((p.y - 200.0).abs() < 1e-6);
        let p = normalized_to_screen(&info, 1.0, 1.0);
        assert!((p.x - 400.0).abs() < 1e-6);
        assert!((p.y - 700.0).abs() < 1e-6);
        let p = normalized_to_screen(&info, 0.5, 0.5);
        assert!((p.x - 250.0).abs() < 1e-6);
        assert!((p.y - 450.0).abs() < 1e-6);
    }

    #[test]
    fn normalize_clamps_out_of_range() {
        let info = WindowInfo {
            pid: 0,
            device_rect: CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(100.0, 100.0)),
        };
        let p = normalized_to_screen(&info, -0.5, 1.5);
        assert!((p.x - 0.0).abs() < 1e-6);
        assert!((p.y - 100.0).abs() < 1e-6);
    }
}
