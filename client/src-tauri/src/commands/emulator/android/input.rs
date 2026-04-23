//! Scrcpy control-socket message encoding.
//!
//! All messages start with a 1-byte type tag followed by a fixed-length
//! payload. Wire format reference:
//! https://github.com/Genymobile/scrcpy/blob/master/server/src/main/java/com/genymobile/scrcpy/control/ControlMessage.java

use std::io::{Result, Write};
use std::net::TcpStream;

use byteorder::{BigEndian, WriteBytesExt};

/// Scrcpy control message type tags.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum MessageType {
    InjectKeycode = 0,
    InjectText = 1,
    InjectTouchEvent = 2,
    InjectScrollEvent = 3,
    BackOrScreenOn = 4,
    ExpandNotificationPanel = 5,
    ExpandSettingsPanel = 6,
    CollapsePanels = 7,
    GetClipboard = 8,
    SetClipboard = 9,
    SetScreenPowerMode = 10,
    RotateDevice = 11,
}

/// Android `MotionEvent` action constants.
#[repr(i32)]
#[derive(Debug, Clone, Copy)]
pub enum MotionAction {
    Down = 0,
    Up = 1,
    Move = 2,
}

/// Android `KeyEvent` action constants.
#[repr(i32)]
#[derive(Debug, Clone, Copy)]
pub enum KeyAction {
    Down = 0,
    Up = 1,
}

/// Keycode subset we route agent commands through. Values mirror
/// `android.view.KeyEvent` constants so they're stable across AOSP versions.
#[repr(i32)]
#[derive(Debug, Clone, Copy)]
pub enum Keycode {
    Home = 3,
    Back = 4,
    VolumeUp = 24,
    VolumeDown = 25,
    Power = 26,
    Enter = 66,
    Del = 67,
    Tab = 61,
    Menu = 82,
    AppSwitch = 187,
    Search = 84,
    Escape = 111,
    DpadLeft = 21,
    DpadRight = 22,
    DpadUp = 19,
    DpadDown = 20,
}

/// Finger id we claim for primary touch. scrcpy uses u64 pointer ids to
/// differentiate multi-touch; we only inject single-finger gestures so a
/// constant is fine.
const PRIMARY_POINTER_ID: u64 = 0x1234_5678_dead_beef;

/// Scrcpy passes pressure as a `u16 fixed-point` in the range [0, 1].
const PRESSURE_FIXED: u16 = 0xFFFF;

/// scrcpy button mask: 0 = no buttons for touch events, AMOTION_EVENT_BUTTON_PRIMARY = 1.
const BUTTON_PRIMARY: u32 = 1;

/// Write a touch event. Coordinates are in device pixels (not normalized).
pub fn send_touch(
    stream: &mut TcpStream,
    action: MotionAction,
    x: i32,
    y: i32,
    width: u16,
    height: u16,
) -> Result<()> {
    let mut buf = Vec::with_capacity(32);
    buf.write_u8(MessageType::InjectTouchEvent as u8)?;
    buf.write_u8(action as u8)?;
    buf.write_u64::<BigEndian>(PRIMARY_POINTER_ID)?;
    buf.write_i32::<BigEndian>(x)?;
    buf.write_i32::<BigEndian>(y)?;
    buf.write_u16::<BigEndian>(width)?;
    buf.write_u16::<BigEndian>(height)?;
    buf.write_u16::<BigEndian>(PRESSURE_FIXED)?;
    buf.write_u32::<BigEndian>(match action {
        MotionAction::Up => 0,
        _ => BUTTON_PRIMARY,
    })?;
    buf.write_u32::<BigEndian>(BUTTON_PRIMARY)?;
    stream.write_all(&buf)?;
    stream.flush()
}

/// Emit a scroll event. Values are half-step counts as signed i16 fixed-point.
pub fn send_scroll(
    stream: &mut TcpStream,
    x: i32,
    y: i32,
    width: u16,
    height: u16,
    h_scroll: i16,
    v_scroll: i16,
) -> Result<()> {
    let mut buf = Vec::with_capacity(24);
    buf.write_u8(MessageType::InjectScrollEvent as u8)?;
    buf.write_i32::<BigEndian>(x)?;
    buf.write_i32::<BigEndian>(y)?;
    buf.write_u16::<BigEndian>(width)?;
    buf.write_u16::<BigEndian>(height)?;
    buf.write_i16::<BigEndian>(h_scroll)?;
    buf.write_i16::<BigEndian>(v_scroll)?;
    buf.write_u32::<BigEndian>(0)?;
    stream.write_all(&buf)?;
    stream.flush()
}

/// Inject a keycode. `meta_state` mirrors `KeyEvent.getMetaState()`
/// (e.g. 0x01 for shift) — pass 0 for the common case.
pub fn send_key(
    stream: &mut TcpStream,
    action: KeyAction,
    keycode: Keycode,
    meta_state: u32,
    repeat: u32,
) -> Result<()> {
    let mut buf = Vec::with_capacity(16);
    buf.write_u8(MessageType::InjectKeycode as u8)?;
    buf.write_u8(action as u8)?;
    buf.write_i32::<BigEndian>(keycode as i32)?;
    buf.write_u32::<BigEndian>(repeat)?;
    buf.write_u32::<BigEndian>(meta_state)?;
    stream.write_all(&buf)?;
    stream.flush()
}

/// Send a UTF-8 string for injection. scrcpy limits strings to 300 bytes per
/// message; callers should split long text.
pub fn send_text(stream: &mut TcpStream, text: &str) -> Result<()> {
    let bytes = text.as_bytes();
    if bytes.len() > 300 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "scrcpy text events are limited to 300 bytes; split the payload",
        ));
    }
    let mut buf = Vec::with_capacity(1 + 4 + bytes.len());
    buf.write_u8(MessageType::InjectText as u8)?;
    buf.write_u32::<BigEndian>(bytes.len() as u32)?;
    buf.extend_from_slice(bytes);
    stream.write_all(&buf)?;
    stream.flush()
}

/// scrcpy's combined back/screen-on message — useful as a home-key press
/// replacement when the screen is off.
pub fn send_back_or_screen_on(stream: &mut TcpStream) -> Result<()> {
    let buf = [MessageType::BackOrScreenOn as u8, KeyAction::Down as u8];
    stream.write_all(&buf)?;
    stream.flush()
}

/// Rotate the device; scrcpy forwards this to Android's WindowManager.
/// Accepted values: 0 (natural), 1 (90° CCW), 2 (180°), 3 (90° CW).
pub fn send_rotate(stream: &mut TcpStream, rotation: u8) -> Result<()> {
    let buf = [MessageType::RotateDevice as u8, rotation & 0b11];
    stream.write_all(&buf)?;
    stream.flush()
}

/// Push a clipboard payload to the device so the next paste uses it. scrcpy
/// caps the body at 262143 bytes; longer strings return InvalidInput.
pub fn send_set_clipboard(stream: &mut TcpStream, text: &str, paste: bool) -> Result<()> {
    let bytes = text.as_bytes();
    if bytes.len() > 262_143 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "clipboard payload exceeds scrcpy's 256 KiB limit",
        ));
    }
    let mut buf = Vec::with_capacity(1 + 8 + 1 + 4 + bytes.len());
    buf.write_u8(MessageType::SetClipboard as u8)?;
    buf.write_u64::<BigEndian>(0)?; // sequence number (0 = don't ack)
    buf.write_u8(if paste { 1 } else { 0 })?;
    buf.write_u32::<BigEndian>(bytes.len() as u32)?;
    buf.extend_from_slice(bytes);
    stream.write_all(&buf)?;
    stream.flush()
}

/// Map a normalized point `(0..1, 0..1)` to device pixels.
pub fn denormalize(x: f32, y: f32, width: u32, height: u32) -> (i32, i32) {
    let px = (x.clamp(0.0, 1.0) * width as f32).round() as i32;
    let py = (y.clamp(0.0, 1.0) * height as f32).round() as i32;
    (
        px.min(width as i32 - 1).max(0),
        py.min(height as i32 - 1).max(0),
    )
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
}
