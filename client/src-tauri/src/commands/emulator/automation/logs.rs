//! Log streaming.
//!
//! Android: spawn `adb logcat -v threadtime`; parse line-by-line.
//! iOS: call idb's `Log` streaming RPC (stubbed until proto is vendored).
//!
//! Both platforms funnel entries through `emulator:log` Tauri events.
//! A ring buffer of the last 10 000 entries is retained in-memory so
//! `emulator_logs_get_recent` returns a history even for late subscribers.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use tauri::{AppHandle, Emitter, Runtime};

use crate::commands::emulator::android::adb::Adb;
use crate::commands::emulator::process::ChildGuard;

use super::{LogEntry, EMULATOR_LOG_EVENT};

const RING_CAPACITY: usize = 10_000;

/// Shared log collector. Cheap to clone (only an `Arc<Mutex>` inside).
#[derive(Clone)]
pub struct LogCollector {
    ring: Arc<Mutex<VecDeque<LogEntry>>>,
}

impl LogCollector {
    pub fn new() -> Self {
        Self {
            ring: Arc::new(Mutex::new(VecDeque::with_capacity(RING_CAPACITY))),
        }
    }

    pub fn push(&self, entry: LogEntry) {
        let mut ring = self.ring.lock().expect("log ring poisoned");
        if ring.len() == RING_CAPACITY {
            ring.pop_front();
        }
        ring.push_back(entry);
    }

    pub fn recent(&self, limit: usize) -> Vec<LogEntry> {
        let ring = self.ring.lock().expect("log ring poisoned");
        let take = limit.min(ring.len());
        ring.iter().rev().take(take).cloned().rev().collect()
    }

    pub fn clear(&self) {
        self.ring.lock().expect("log ring poisoned").clear();
    }
}

impl Default for LogCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Owns the `adb logcat` child + reader thread. Dropping joins cleanly.
pub struct AndroidLogStream {
    shutdown: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
    _guard: ChildGuard,
}

impl AndroidLogStream {
    pub fn spawn<R: Runtime>(
        app: AppHandle<R>,
        adb: &Adb,
        collector: LogCollector,
    ) -> std::io::Result<Self> {
        let mut child = adb.shell_spawn(["logcat", "-v", "threadtime"])?;
        let stdout = child.stdout.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "logcat stdout missing")
        })?;
        let guard = ChildGuard::new("adb-logcat", child);

        let shutdown = Arc::new(AtomicBool::new(false));
        let reader_shutdown = Arc::clone(&shutdown);

        let reader = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if reader_shutdown.load(Ordering::Relaxed) {
                    break;
                }
                if line.trim().is_empty() {
                    continue;
                }
                if let Some(entry) = parse_logcat_line(&line) {
                    collector.push(entry.clone());
                    let _ = app.emit(EMULATOR_LOG_EVENT, entry);
                }
            }
        });

        Ok(Self {
            shutdown,
            reader: Some(reader),
            _guard: guard,
        })
    }

    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AndroidLogStream {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Parse a `logcat -v threadtime` line:
/// ```text
/// 03-21 10:10:00.123  1234  5678 I MyTag: something happened
/// ```
pub fn parse_logcat_line(line: &str) -> Option<LogEntry> {
    // Walk whitespace-delimited fields — logcat may use 1 or 2 spaces between
    // columns. The last field (message) is everything after the level.
    let mut fields = line.split_whitespace();
    let _mmdd = fields.next()?;
    let hms = fields.next()?;
    let _pid = fields.next()?;
    let _tid = fields.next()?;
    let level_char = fields.next()?;

    let level = match level_char {
        "V" => "verbose",
        "D" => "debug",
        "I" => "info",
        "W" => "warn",
        "E" => "error",
        "F" => "fatal",
        _ => return None,
    };

    // Locate the position after the level char to capture the rest of the
    // line verbatim (message text may contain tabs, multiple spaces, etc.).
    let level_pos = line.find(&format!(" {level_char} "))?;
    let rest_start = level_pos + level_char.len() + 2;
    if rest_start >= line.len() {
        return None;
    }
    let rest = line[rest_start..].trim_end_matches(['\r', '\n']);

    let (tag, message) = rest
        .split_once(": ")
        .map(|(t, m)| (t.trim().to_string(), m.to_string()))
        .unwrap_or_else(|| (String::new(), rest.to_string()));

    Some(LogEntry {
        timestamp_ms: parse_hms_ms(hms).unwrap_or(0),
        level: level.to_string(),
        tag,
        message,
    })
}

fn parse_hms_ms(hms: &str) -> Option<u64> {
    // Format is HH:MM:SS.mmm
    let (clock, millis) = hms.split_once('.')?;
    let mut parts = clock.split(':');
    let hh: u64 = parts.next()?.parse().ok()?;
    let mm: u64 = parts.next()?.parse().ok()?;
    let ss: u64 = parts.next()?.parse().ok()?;
    let ms: u64 = millis.parse().ok()?;
    Some(((hh * 3600 + mm * 60 + ss) * 1000) + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_threadtime_line() {
        let line = "03-21 10:10:00.123  1234  5678 I MyTag: something happened";
        let entry = parse_logcat_line(line).expect("parsed");
        assert_eq!(entry.level, "info");
        assert_eq!(entry.tag, "MyTag");
        assert_eq!(entry.message, "something happened");
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let collector = LogCollector::new();
        for i in 0..RING_CAPACITY + 5 {
            collector.push(LogEntry {
                timestamp_ms: i as u64,
                level: "info".to_string(),
                tag: String::new(),
                message: i.to_string(),
            });
        }
        let recent = collector.recent(10);
        assert_eq!(recent.len(), 10);
        let first_retained = recent.first().expect("non-empty");
        assert_eq!(first_retained.timestamp_ms, (RING_CAPACITY as u64) - 5);
    }
}
