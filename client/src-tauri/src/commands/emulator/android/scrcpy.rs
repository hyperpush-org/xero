//! Scrcpy wire protocol driver.
//!
//! The flow:
//! 1. Push the bundled `scrcpy-server.jar` to `/data/local/tmp` on the device.
//! 2. Install a reverse tunnel: device `localabstract:scrcpy_XXXXXXXX` →
//!    host `127.0.0.1:<port>`.
//! 3. Start a TCP listener on that port. *Then* spawn the server via
//!    `app_process`; it will connect back as soon as it's up.
//! 4. Accept two inbound sockets: video first, then control.
//! 5. Parse the video stream (`DEVICE_META`, then frame headers + H.264 NALs)
//!    and forward NALs to the decoder.
//! 6. Write control messages (touch, key, text, scroll, clipboard) on the
//!    control socket.
//!
//! Reference: scrcpy server sources at
//! https://github.com/Genymobile/scrcpy/tree/master/server/src/main/java/com/genymobile/scrcpy

use std::io::{Read, Result};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use byteorder::{BigEndian, ReadBytesExt};

use super::adb::Adb;

/// scrcpy-server protocol version we ship. Update in lockstep with the bundled
/// JAR. 2.7 matches the file the bundler drops into
/// `resources/scrcpy-server-v2.7.jar`.
pub const SCRCPY_VERSION: &str = "2.7";

/// Where on the device we land the server jar. `/data/local/tmp` is the only
/// location always writable by `shell` on AOSP.
pub const DEVICE_JAR_PATH: &str = "/data/local/tmp/scrcpy-server.jar";

/// Scrcpy lets us pick a socket name; we randomize to avoid collisions when
/// multiple scrcpy instances run on the same host.
fn random_socket_name() -> String {
    let salt = AtomicU64::new(0);
    let next = salt.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    format!("scrcpy_{:08x}{:04x}", nanos, next & 0xffff)
}

/// Metadata announced at the top of the video socket. scrcpy 2.x emits this
/// as: 4 bytes codec id, 4 bytes width (BE u32), 4 bytes height (BE u32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceMeta {
    pub codec: CodecId,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecId {
    H264,
    H265,
    Av1,
    Unknown(u32),
}

impl CodecId {
    fn from_raw(raw: u32) -> Self {
        // scrcpy packs the codec name into a u32 as ASCII bytes: 'h' '2' '6' '4'
        match raw {
            0x6832_3634 => CodecId::H264, // "h264"
            0x6832_3635 => CodecId::H265, // "h265"
            0x6176_3031 => CodecId::Av1,  // "av01"
            other => CodecId::Unknown(other),
        }
    }
}

/// Frame header layout from scrcpy 2.x: 64 bits.
/// - 1 bit: config packet flag (SPS/PPS, no display yet)
/// - 1 bit: keyframe flag
/// - 62 bits: PTS in microseconds
///   Followed by: 4 bytes big-endian packet size, then the payload bytes.
#[derive(Debug, Clone, Copy)]
pub struct FrameHeader {
    pub is_config: bool,
    pub is_keyframe: bool,
    pub pts_us: u64,
    pub size: u32,
}

impl FrameHeader {
    /// Parse the 12-byte frame header.
    pub fn parse(mut bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 12 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "short scrcpy frame header",
            ));
        }
        let flags_and_pts = bytes.read_u64::<BigEndian>()?;
        let size = bytes.read_u32::<BigEndian>()?;

        const CONFIG_FLAG: u64 = 1 << 63;
        const KEYFRAME_FLAG: u64 = 1 << 62;
        const PTS_MASK: u64 = (1 << 62) - 1;

        Ok(Self {
            is_config: flags_and_pts & CONFIG_FLAG != 0,
            is_keyframe: flags_and_pts & KEYFRAME_FLAG != 0,
            pts_us: flags_and_pts & PTS_MASK,
            size,
        })
    }
}

/// Push the jar, reverse-tunnel, and launch the server. Returns the handshake
/// sockets once both are connected.
pub struct ScrcpyConnection {
    pub meta: DeviceMeta,
    pub video: TcpStream,
    pub control: TcpStream,
    pub device_name: String,
}

pub fn start(adb: &Adb, local_jar: &Path) -> Result<ScrcpyConnection> {
    let socket_name = random_socket_name();
    let local_port = pick_free_port()?;

    // 1. Push the jar.
    adb.push(local_jar, DEVICE_JAR_PATH)?;

    // 2. Reverse tunnel for inbound connections.
    adb.reverse(
        &format!("localabstract:{socket_name}"),
        &format!("tcp:{local_port}"),
    )?;

    // 3. Open a listener *before* launching the server.
    let listener = TcpListener::bind(("127.0.0.1", local_port))?;
    listener.set_nonblocking(false)?;

    // 4. Spawn the server process.
    let command = build_server_command(&socket_name);
    let mut spawn = adb.shell_spawn(command.iter().map(|s| s.as_str()))?;

    // 5. Accept two sockets within a reasonable window.
    listener.set_nonblocking(false)?;
    let (video, meta) = accept_video(&listener, Duration::from_secs(20))?;
    let (control, _) = accept_control(&listener, Duration::from_secs(10))?;

    // Detach the scrcpy-server's adb shell child — the server speaks entirely
    // over the sockets we hold, and letting the adb shell stay tied to our
    // process means any future `.wait()` would block indefinitely. We keep
    // the handle around only long enough to release.
    let _ = spawn.try_wait();
    drop(spawn);

    let device_name = read_device_name(&video).unwrap_or_default();

    Ok(ScrcpyConnection {
        meta,
        video,
        control,
        device_name,
    })
}

fn build_server_command(socket_name: &str) -> Vec<String> {
    // Single shell pipeline — must be one big string because adb shell
    // interprets each argument as a separate shell token only when the
    // command is `shell -- <binary> <args>`, which we don't use here.
    let parts = vec![
        "CLASSPATH=".to_string() + DEVICE_JAR_PATH,
        "app_process".to_string(),
        "/".to_string(),
        "com.genymobile.scrcpy.Server".to_string(),
        SCRCPY_VERSION.to_string(),
        format!("scid={}", random_scid()),
        "log_level=warn".to_string(),
        "video=true".to_string(),
        "audio=false".to_string(),
        "control=true".to_string(),
        "tunnel_forward=false".to_string(),
        "cleanup=true".to_string(),
        "send_device_meta=true".to_string(),
        "send_frame_meta=true".to_string(),
        format!("socket_name={socket_name}"),
    ];
    parts
}

fn random_scid() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{:08x}", nanos & 0x7fff_ffff)
}

fn accept_video(listener: &TcpListener, timeout: Duration) -> Result<(TcpStream, DeviceMeta)> {
    let (mut stream, _peer) = accept_within(listener, timeout)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;

    // 12-byte header: codec id (u32 BE), width (u32 BE), height (u32 BE)
    let mut buf = [0u8; 12];
    stream.read_exact(&mut buf)?;
    let codec = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let width = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let height = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);

    Ok((
        stream,
        DeviceMeta {
            codec: CodecId::from_raw(codec),
            width,
            height,
        },
    ))
}

fn accept_control(listener: &TcpListener, timeout: Duration) -> Result<(TcpStream, DeviceMeta)> {
    let (stream, _peer) = accept_within(listener, timeout)?;
    // Control socket has no metadata — it's a straight push channel.
    Ok((
        stream,
        DeviceMeta {
            codec: CodecId::Unknown(0),
            width: 0,
            height: 0,
        },
    ))
}

fn accept_within(
    listener: &TcpListener,
    timeout: Duration,
) -> Result<(TcpStream, std::net::SocketAddr)> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok(conn) => {
                let (stream, peer) = conn;
                stream.set_nonblocking(false)?;
                return Ok((stream, peer));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "timed out waiting for scrcpy server to connect back",
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e),
        }
    }
}

fn read_device_name(stream: &TcpStream) -> Result<String> {
    // scrcpy optionally sends a 64-byte NUL-terminated device name after the
    // metadata. We don't strictly need it; if it's missing we just return
    // an empty string.
    let mut buf = [0u8; 64];
    let mut tmp = stream;
    match tmp.read_exact(&mut buf) {
        Ok(()) => {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            Ok(String::from_utf8_lossy(&buf[..end]).into_owned())
        }
        Err(_) => Ok(String::new()),
    }
}

fn pick_free_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Read one complete video packet (header + payload) from the video socket.
/// Returns `None` on EOF so the caller can exit cleanly when the device
/// disconnects.
pub fn read_video_packet(stream: &mut TcpStream) -> Result<Option<VideoPacket>> {
    let mut header_buf = [0u8; 12];
    if !(read_exact_maybe_eof(stream, &mut header_buf)?) {
        return Ok(None);
    }
    let header = FrameHeader::parse(&header_buf)?;
    if header.size == 0 {
        return Ok(Some(VideoPacket {
            header,
            payload: Vec::new(),
        }));
    }
    if header.size > 32 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("implausible scrcpy packet size: {}", header.size),
        ));
    }
    let mut payload = vec![0u8; header.size as usize];
    stream.read_exact(&mut payload)?;
    Ok(Some(VideoPacket { header, payload }))
}

pub struct VideoPacket {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

fn read_exact_maybe_eof(stream: &mut TcpStream, buf: &mut [u8]) -> Result<bool> {
    let mut read = 0;
    while read < buf.len() {
        match stream.read(&mut buf[read..]) {
            Ok(0) if read == 0 => return Ok(false),
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "mid-header EOF from scrcpy socket",
                ));
            }
            Ok(n) => read += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

/// Resolve the bundled scrcpy-server.jar path relative to the Tauri resource
/// directory. Returns `Err` if the resource isn't bundled.
pub fn bundled_jar_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf> {
    use tauri::{path::BaseDirectory, Manager};

    let candidates = [
        format!("resources/scrcpy-server-v{}.jar", SCRCPY_VERSION),
        format!("scrcpy-server-v{}.jar", SCRCPY_VERSION),
        "resources/scrcpy-server.jar".to_string(),
        "scrcpy-server.jar".to_string(),
    ];
    for rel in candidates {
        if let Ok(path) = app.path().resolve(&rel, BaseDirectory::Resource) {
            if path.exists() {
                return Ok(path);
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "scrcpy-server.jar not bundled with Xero. Drop the jar into client/src-tauri/resources/.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frame_header_config_flag() {
        // Bits: config=1, keyframe=0, pts=0 — size=42
        let mut bytes = [0u8; 12];
        bytes[0] = 0x80;
        bytes[8..12].copy_from_slice(&42u32.to_be_bytes());
        let header = FrameHeader::parse(&bytes).expect("parse");
        assert!(header.is_config);
        assert!(!header.is_keyframe);
        assert_eq!(header.pts_us, 0);
        assert_eq!(header.size, 42);
    }

    #[test]
    fn parse_frame_header_keyframe() {
        let mut bytes = [0u8; 12];
        bytes[0] = 0x40;
        bytes[8..12].copy_from_slice(&512u32.to_be_bytes());
        let header = FrameHeader::parse(&bytes).expect("parse");
        assert!(!header.is_config);
        assert!(header.is_keyframe);
        assert_eq!(header.size, 512);
    }

    #[test]
    fn parse_frame_header_pts() {
        let mut pts = 0x0000_1234_5678_9abc_u64;
        pts &= (1 << 62) - 1;
        let mut bytes = [0u8; 12];
        bytes[..8].copy_from_slice(&pts.to_be_bytes());
        let header = FrameHeader::parse(&bytes).expect("parse");
        assert_eq!(header.pts_us, pts);
    }

    #[test]
    fn codec_id_recognizes_h264() {
        assert_eq!(CodecId::from_raw(0x6832_3634), CodecId::H264);
        assert_eq!(CodecId::from_raw(0x6832_3635), CodecId::H265);
        match CodecId::from_raw(0xDEAD_BEEF) {
            CodecId::Unknown(v) => assert_eq!(v, 0xDEAD_BEEF),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn frame_header_short_buffer_errors() {
        let bytes = [0u8; 4];
        FrameHeader::parse(&bytes).unwrap_err();
    }
}
