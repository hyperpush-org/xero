use std::time::Instant;

/// Lightweight structured timing for hot paths. Disabled unless XERO_PERF_LOG
/// is set, so production builds keep the measurement hooks without paying log
/// volume during normal use.
#[derive(Debug)]
pub struct PerfSpan {
    name: &'static str,
    started_at: Instant,
    fields: Vec<(&'static str, String)>,
}

impl PerfSpan {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            started_at: Instant::now(),
            fields: Vec::new(),
        }
    }

    pub fn field(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.fields.push((key, value.into()));
        self
    }
}

impl Drop for PerfSpan {
    fn drop(&mut self) {
        if std::env::var_os("XERO_PERF_LOG").is_none() {
            return;
        }

        let mut fields = serde_json::Map::new();
        for (key, value) in &self.fields {
            fields.insert((*key).into(), serde_json::Value::String(value.clone()));
        }

        let payload = serde_json::json!({
            "name": self.name,
            "durationMs": self.started_at.elapsed().as_secs_f64() * 1000.0,
            "fields": fields,
        });
        eprintln!("[xero-perf] {payload}");
    }
}
