//! Log + IDL → structured-English tx trace.
//!
//! Takes whatever the validator / RPC hands us (raw stderr lines from
//! `simulateTransaction`, the log array of a `getTransaction` response,
//! or a failure `err` blob) and turns it into a deterministic structured
//! explanation. The explanation names the instruction that failed, the
//! accounts it touched, the IDL error variant (when we have the program
//! IDL cached), and a human-readable message.
//!
//! The decoder is deliberately a pure function of its inputs so both the
//! UI's tx inspector panel and the autonomous `solana_explain` tool can
//! reuse it without duplicating parsing logic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::cpi_resolver::known_program_label;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DecodedLogEntry {
    /// `Program Pxxx invoke [depth]`
    Invoke {
        program_id: String,
        program_label: Option<String>,
        depth: u32,
    },
    /// `Program Pxxx success`
    Success { program_id: String },
    /// `Program Pxxx failed: custom program error: 0x<hex>`
    Failure {
        program_id: String,
        program_label: Option<String>,
        code: Option<u32>,
        idl_variant: Option<String>,
        raw: String,
    },
    /// `Program log: <msg>` — free-form log output from the program.
    Log {
        program_id: Option<String>,
        message: String,
    },
    /// `Program data: <base64>` — emitted events (Anchor `emit!` etc.)
    Data {
        program_id: Option<String>,
        base64: String,
    },
    /// `Program <pid> consumed X of Y compute units` — CU accounting.
    ComputeUsage {
        program_id: String,
        consumed: u64,
        allocated: u64,
    },
    /// Anything that didn't match a known prefix — kept verbatim.
    Unparsed { raw: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DecodedLogs {
    pub entries: Vec<DecodedLogEntry>,
    pub programs_invoked: Vec<String>,
    pub total_compute_units: u64,
}

/// IDL error map: `program_id -> code -> IDL error variant name`.
pub type IdlErrorMap = BTreeMap<String, BTreeMap<u32, String>>;

/// Parse a raw list of `Program log:` style lines into structured entries.
/// `idl_errors` is optional — when present we annotate failure entries
/// with the matching IDL variant name.
pub fn decode_logs(lines: &[String], idl_errors: Option<&IdlErrorMap>) -> DecodedLogs {
    let mut entries = Vec::with_capacity(lines.len());
    let mut invoke_stack: Vec<String> = Vec::new();
    let mut programs_invoked: Vec<String> = Vec::new();
    let mut total_compute_units: u64 = 0;

    for raw in lines {
        let trimmed = raw.trim();
        if let Some(entry) = parse_invoke(trimmed) {
            if let DecodedLogEntry::Invoke { program_id, .. } = &entry {
                invoke_stack.push(program_id.clone());
                if !programs_invoked.contains(program_id) {
                    programs_invoked.push(program_id.clone());
                }
            }
            entries.push(entry);
            continue;
        }
        if let Some(entry) = parse_success(trimmed) {
            if let DecodedLogEntry::Success { .. } = &entry {
                invoke_stack.pop();
            }
            entries.push(entry);
            continue;
        }
        if let Some(entry) = parse_failure(trimmed, idl_errors) {
            if let DecodedLogEntry::Failure { .. } = &entry {
                invoke_stack.pop();
            }
            entries.push(entry);
            continue;
        }
        if let Some(entry) = parse_compute(trimmed) {
            if let DecodedLogEntry::ComputeUsage { consumed, .. } = &entry {
                total_compute_units = total_compute_units.saturating_add(*consumed);
            }
            entries.push(entry);
            continue;
        }
        if let Some(entry) = parse_program_log(trimmed, invoke_stack.last()) {
            entries.push(entry);
            continue;
        }
        if let Some(entry) = parse_program_data(trimmed, invoke_stack.last()) {
            entries.push(entry);
            continue;
        }
        entries.push(DecodedLogEntry::Unparsed {
            raw: raw.to_string(),
        });
    }

    DecodedLogs {
        entries,
        programs_invoked,
        total_compute_units,
    }
}

fn parse_invoke(line: &str) -> Option<DecodedLogEntry> {
    let rest = line.strip_prefix("Program ")?;
    let (pid, tail) = rest.split_once(' ')?;
    let tail = tail.trim();
    let depth_part = tail.strip_prefix("invoke [")?;
    let depth_str = depth_part.strip_suffix(']')?;
    let depth: u32 = depth_str.trim().parse().ok()?;
    Some(DecodedLogEntry::Invoke {
        program_id: pid.to_string(),
        program_label: known_program_label(pid).map(String::from),
        depth,
    })
}

fn parse_success(line: &str) -> Option<DecodedLogEntry> {
    let rest = line.strip_prefix("Program ")?;
    let (pid, tail) = rest.split_once(' ')?;
    if tail.trim() != "success" {
        return None;
    }
    Some(DecodedLogEntry::Success {
        program_id: pid.to_string(),
    })
}

fn parse_failure(line: &str, idl_errors: Option<&IdlErrorMap>) -> Option<DecodedLogEntry> {
    let rest = line.strip_prefix("Program ")?;
    let (pid, tail) = rest.split_once(' ')?;
    let tail = tail.trim();
    if !tail.starts_with("failed") {
        return None;
    }
    let code = extract_code(tail);
    let idl_variant = match (code, idl_errors) {
        (Some(c), Some(map)) => map.get(pid).and_then(|m| m.get(&c).cloned()),
        _ => None,
    };
    Some(DecodedLogEntry::Failure {
        program_id: pid.to_string(),
        program_label: known_program_label(pid).map(String::from),
        code,
        idl_variant,
        raw: line.to_string(),
    })
}

fn extract_code(tail: &str) -> Option<u32> {
    // "failed: custom program error: 0x1770" → 0x1770
    let marker = tail.find("0x")?;
    let hex_part: String = tail[marker + 2..]
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    u32::from_str_radix(&hex_part, 16).ok()
}

fn parse_compute(line: &str) -> Option<DecodedLogEntry> {
    let rest = line.strip_prefix("Program ")?;
    let (pid, tail) = rest.split_once(' ')?;
    let tail = tail.trim();
    let consumed_part = tail.strip_prefix("consumed ")?;
    let (consumed_str, remainder) = consumed_part.split_once(' ')?;
    let consumed: u64 = consumed_str.trim().parse().ok()?;
    // Pattern: "N of M compute units"
    let mut parts = remainder.split_whitespace();
    let of = parts.next()?;
    if of != "of" {
        return None;
    }
    let allocated_str = parts.next()?;
    let allocated: u64 = allocated_str.trim().parse().ok()?;
    Some(DecodedLogEntry::ComputeUsage {
        program_id: pid.to_string(),
        consumed,
        allocated,
    })
}

fn parse_program_log(line: &str, current_program: Option<&String>) -> Option<DecodedLogEntry> {
    let rest = line.strip_prefix("Program log: ")?;
    Some(DecodedLogEntry::Log {
        program_id: current_program.cloned(),
        message: rest.to_string(),
    })
}

fn parse_program_data(line: &str, current_program: Option<&String>) -> Option<DecodedLogEntry> {
    let rest = line.strip_prefix("Program data: ")?;
    Some(DecodedLogEntry::Data {
        program_id: current_program.cloned(),
        base64: rest.to_string(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Explanation {
    pub ok: bool,
    pub summary: String,
    pub primary_error: Option<ErrorDetail>,
    pub decoded_logs: DecodedLogs,
    pub affected_programs: Vec<String>,
    pub compute_units_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDetail {
    pub program_id: String,
    pub program_label: Option<String>,
    pub code: Option<u32>,
    pub idl_variant: Option<String>,
    pub raw: String,
}

/// Turn a simulation result (logs + optional `err` blob) into a human
/// Explanation. `err_blob` is the JSON-RPC `err` field, which is `null`
/// for a successful simulation.
pub fn explain_simulation(
    logs: &[String],
    err_blob: Option<&serde_json::Value>,
    idl_errors: Option<&IdlErrorMap>,
) -> Explanation {
    let decoded = decode_logs(logs, idl_errors);
    let primary_error = decoded.entries.iter().rev().find_map(|entry| match entry {
        DecodedLogEntry::Failure {
            program_id,
            program_label,
            code,
            idl_variant,
            raw,
        } => Some(ErrorDetail {
            program_id: program_id.clone(),
            program_label: program_label.clone(),
            code: *code,
            idl_variant: idl_variant.clone(),
            raw: raw.clone(),
        }),
        _ => None,
    });

    let ok = err_blob.map(|v| v.is_null()).unwrap_or(true) && primary_error.is_none();

    let summary = match (&primary_error, err_blob) {
        (Some(err), _) => match (&err.idl_variant, err.code) {
            (Some(variant), Some(code)) => format!(
                "{} (0x{:x}) from {}",
                variant,
                code,
                err.program_label.as_deref().unwrap_or(&err.program_id)
            ),
            (None, Some(code)) => format!(
                "program error 0x{:x} from {}",
                code,
                err.program_label.as_deref().unwrap_or(&err.program_id)
            ),
            _ => format!(
                "{} reported a failure",
                err.program_label.as_deref().unwrap_or(&err.program_id)
            ),
        },
        (None, Some(blob)) if !blob.is_null() => format!("simulation reported err: {blob}"),
        _ => format!(
            "transaction ok — {} program invocation(s), {} CU",
            decoded.programs_invoked.len(),
            decoded.total_compute_units
        ),
    };

    Explanation {
        ok,
        summary,
        primary_error,
        affected_programs: decoded.programs_invoked.clone(),
        compute_units_total: decoded.total_compute_units,
        decoded_logs: decoded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lines(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_invoke_and_success_pair() {
        let logs = lines(&[
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [1]",
            "Program log: Instruction: Transfer",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 2345 of 200000 compute units",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
        ]);
        let decoded = decode_logs(&logs, None);
        assert_eq!(decoded.programs_invoked.len(), 1);
        assert_eq!(decoded.total_compute_units, 2345);
        assert!(matches!(decoded.entries[0], DecodedLogEntry::Invoke { .. }));
        assert!(matches!(
            decoded.entries[3],
            DecodedLogEntry::Success { .. }
        ));
    }

    #[test]
    fn parses_custom_program_error_with_hex_code() {
        let logs = lines(&[
            "Program GovER5Lthms3bLBqWub97yVrMmEogzX7xNjdXpPPCVZw invoke [1]",
            "Program GovER5Lthms3bLBqWub97yVrMmEogzX7xNjdXpPPCVZw failed: custom program error: 0x1770",
        ]);
        let decoded = decode_logs(&logs, None);
        match &decoded.entries[1] {
            DecodedLogEntry::Failure {
                code,
                program_label,
                ..
            } => {
                assert_eq!(*code, Some(0x1770));
                assert_eq!(program_label.as_deref(), Some("SPL Governance"));
            }
            other => panic!("expected Failure, got {other:?}"),
        }
    }

    #[test]
    fn idl_map_annotates_failure_with_variant_name() {
        let mut map = IdlErrorMap::new();
        let mut inner = BTreeMap::new();
        inner.insert(0x1770, "InvalidVoteRecord".to_string());
        map.insert(
            "GovER5Lthms3bLBqWub97yVrMmEogzX7xNjdXpPPCVZw".to_string(),
            inner,
        );
        let logs = lines(&[
            "Program GovER5Lthms3bLBqWub97yVrMmEogzX7xNjdXpPPCVZw invoke [1]",
            "Program GovER5Lthms3bLBqWub97yVrMmEogzX7xNjdXpPPCVZw failed: custom program error: 0x1770",
        ]);
        let decoded = decode_logs(&logs, Some(&map));
        match &decoded.entries[1] {
            DecodedLogEntry::Failure {
                idl_variant, code, ..
            } => {
                assert_eq!(*code, Some(0x1770));
                assert_eq!(idl_variant.as_deref(), Some("InvalidVoteRecord"));
            }
            other => panic!("expected Failure, got {other:?}"),
        }
    }

    #[test]
    fn program_log_attributes_to_current_invocation() {
        let logs = lines(&[
            "Program P111 invoke [1]",
            "Program log: hello world",
            "Program P111 success",
        ]);
        let decoded = decode_logs(&logs, None);
        match &decoded.entries[1] {
            DecodedLogEntry::Log {
                program_id,
                message,
            } => {
                assert_eq!(program_id.as_deref(), Some("P111"));
                assert_eq!(message, "hello world");
            }
            other => panic!("expected Log, got {other:?}"),
        }
    }

    #[test]
    fn unparsed_lines_are_preserved_verbatim() {
        let logs = lines(&["Whatever this is"]);
        let decoded = decode_logs(&logs, None);
        match &decoded.entries[0] {
            DecodedLogEntry::Unparsed { raw } => {
                assert_eq!(raw, "Whatever this is");
            }
            other => panic!("expected Unparsed, got {other:?}"),
        }
    }

    #[test]
    fn explain_summary_includes_idl_variant_when_available() {
        let mut map = IdlErrorMap::new();
        let mut inner = BTreeMap::new();
        inner.insert(0x1770, "InvalidVoteRecord".to_string());
        map.insert("Gov111".to_string(), inner);
        let logs = lines(&[
            "Program Gov111 invoke [1]",
            "Program Gov111 failed: custom program error: 0x1770",
        ]);
        let expl = explain_simulation(
            &logs,
            Some(&json!({"InstructionError": [0, "Custom"]})),
            Some(&map),
        );
        assert!(!expl.ok);
        assert!(expl.summary.contains("InvalidVoteRecord"));
    }

    #[test]
    fn explain_successful_simulation_produces_ok_summary() {
        let logs = lines(&[
            "Program P111 invoke [1]",
            "Program P111 consumed 1234 of 200000 compute units",
            "Program P111 success",
        ]);
        let expl = explain_simulation(&logs, Some(&serde_json::Value::Null), None);
        assert!(expl.ok);
        assert!(expl.summary.contains("1234"));
        assert_eq!(expl.compute_units_total, 1234);
    }

    #[test]
    fn explain_ok_false_when_err_blob_is_non_null_even_without_failure_log() {
        let expl = explain_simulation(&lines(&[]), Some(&json!({"BlockhashNotFound": null})), None);
        assert!(!expl.ok);
        assert!(expl.summary.contains("BlockhashNotFound") || expl.summary.contains("err"));
    }
}
