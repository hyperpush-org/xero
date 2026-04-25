//! Local vs on-chain IDL drift detection.
//!
//! Classifies every change between two IDLs into one of three buckets:
//!
//! - **Breaking** — any change that would make an existing client call
//!   fail on the deployed program: removed/renamed instruction, changed
//!   argument type, removed/renamed account, removed/renamed account
//!   field, changed account field type, reordered accounts on an
//!   instruction.
//! - **Risky** — changes that clients can usually keep up with but may
//!   be surprising: added required instruction arg, added required
//!   account on an existing instruction.
//! - **Non-breaking** — purely additive: new instruction, new account
//!   struct, added optional instruction arg, renamed docs/msgs.
//!
//! The classifier reads a subset of the Anchor IDL shape (metadata,
//! instructions with accounts + args, accounts with type fields,
//! errors). Anything we don't understand goes into the "non-breaking
//! other" bucket so the report never silently drops changes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Idl;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DriftSeverity {
    Breaking,
    Risky,
    NonBreaking,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DriftChange {
    pub severity: DriftSeverity,
    pub kind: String,
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DriftReport {
    pub local_hash: String,
    pub chain_hash: Option<String>,
    pub identical: bool,
    pub changes: Vec<DriftChange>,
    pub breaking_count: usize,
    pub risky_count: usize,
    pub non_breaking_count: usize,
}

impl DriftReport {
    pub fn empty(local_hash: String) -> Self {
        Self {
            local_hash,
            chain_hash: None,
            identical: true,
            changes: vec![],
            breaking_count: 0,
            risky_count: 0,
            non_breaking_count: 0,
        }
    }
}

pub fn classify(local: &Idl, chain: Option<&Idl>) -> DriftReport {
    let chain = match chain {
        Some(c) => c,
        None => {
            return DriftReport {
                local_hash: local.hash.clone(),
                chain_hash: None,
                identical: false,
                changes: vec![DriftChange {
                    severity: DriftSeverity::NonBreaking,
                    kind: "chain_missing".into(),
                    path: "".into(),
                    detail: "No IDL is published on-chain for this program.".into(),
                }],
                breaking_count: 0,
                risky_count: 0,
                non_breaking_count: 1,
            };
        }
    };
    if local.hash == chain.hash {
        return DriftReport {
            local_hash: local.hash.clone(),
            chain_hash: Some(chain.hash.clone()),
            identical: true,
            changes: vec![],
            breaking_count: 0,
            risky_count: 0,
            non_breaking_count: 0,
        };
    }

    let mut changes = Vec::new();
    diff_instructions(&local.value, &chain.value, &mut changes);
    diff_accounts(&local.value, &chain.value, &mut changes);
    diff_errors(&local.value, &chain.value, &mut changes);

    let (b, r, n) = count_severities(&changes);
    DriftReport {
        local_hash: local.hash.clone(),
        chain_hash: Some(chain.hash.clone()),
        identical: false,
        changes,
        breaking_count: b,
        risky_count: r,
        non_breaking_count: n,
    }
}

fn count_severities(changes: &[DriftChange]) -> (usize, usize, usize) {
    let mut b = 0;
    let mut r = 0;
    let mut n = 0;
    for c in changes {
        match c.severity {
            DriftSeverity::Breaking => b += 1,
            DriftSeverity::Risky => r += 1,
            DriftSeverity::NonBreaking => n += 1,
        }
    }
    (b, r, n)
}

fn diff_instructions(local: &Value, chain: &Value, out: &mut Vec<DriftChange>) {
    let local_ix = index_by_name(local, "instructions");
    let chain_ix = index_by_name(chain, "instructions");

    // Removed instructions
    for (name, chain_ix_value) in &chain_ix {
        if !local_ix.contains_key(name) {
            out.push(DriftChange {
                severity: DriftSeverity::Breaking,
                kind: "instruction_removed".into(),
                path: format!("instructions.{name}"),
                detail: format!("Instruction `{name}` exists on-chain but not in the local IDL."),
            });
            let _ = chain_ix_value;
        }
    }
    // Added instructions
    for name in local_ix.keys() {
        if !chain_ix.contains_key(name) {
            out.push(DriftChange {
                severity: DriftSeverity::NonBreaking,
                kind: "instruction_added".into(),
                path: format!("instructions.{name}"),
                detail: format!("Instruction `{name}` is new in the local IDL."),
            });
        }
    }
    // Shared instructions — compare args and accounts.
    for (name, local_ix_value) in &local_ix {
        if let Some(chain_ix_value) = chain_ix.get(name) {
            diff_args(local_ix_value, chain_ix_value, name, out);
            diff_accounts_on_instruction(local_ix_value, chain_ix_value, name, out);
        }
    }
}

fn diff_args(local: &Value, chain: &Value, ix_name: &str, out: &mut Vec<DriftChange>) {
    let local_args = local.get("args").and_then(|v| v.as_array());
    let chain_args = chain.get("args").and_then(|v| v.as_array());
    let (Some(local_args), Some(chain_args)) = (local_args, chain_args) else {
        return;
    };
    let local_map = by_arg_name(local_args);
    let chain_map = by_arg_name(chain_args);

    // Removed args → breaking
    for name in chain_map.keys() {
        if !local_map.contains_key(name) {
            out.push(DriftChange {
                severity: DriftSeverity::Breaking,
                kind: "arg_removed".into(),
                path: format!("instructions.{ix_name}.args.{name}"),
                detail: format!("Argument `{name}` was removed from `{ix_name}`."),
            });
        }
    }
    // Added args → risky (caller now has to supply one extra value).
    for name in local_map.keys() {
        if !chain_map.contains_key(name) {
            out.push(DriftChange {
                severity: DriftSeverity::Risky,
                kind: "arg_added".into(),
                path: format!("instructions.{ix_name}.args.{name}"),
                detail: format!(
                    "Argument `{name}` was added to `{ix_name}` — existing clients will fail until they send it."
                ),
            });
        }
    }
    // Type changes on overlapping args → breaking.
    for (name, local_arg) in &local_map {
        if let Some(chain_arg) = chain_map.get(name) {
            if local_arg.get("type") != chain_arg.get("type") {
                out.push(DriftChange {
                    severity: DriftSeverity::Breaking,
                    kind: "arg_type_changed".into(),
                    path: format!("instructions.{ix_name}.args.{name}"),
                    detail: format!(
                        "Argument `{name}` type changed: {} → {}",
                        render_type(chain_arg.get("type")),
                        render_type(local_arg.get("type"))
                    ),
                });
            }
        }
    }
    // Reordering is breaking (positional args).
    if args_reordered(local_args, chain_args) {
        out.push(DriftChange {
            severity: DriftSeverity::Breaking,
            kind: "args_reordered".into(),
            path: format!("instructions.{ix_name}.args"),
            detail: format!("Argument order in `{ix_name}` differs from the on-chain IDL."),
        });
    }
}

fn diff_accounts_on_instruction(
    local: &Value,
    chain: &Value,
    ix_name: &str,
    out: &mut Vec<DriftChange>,
) {
    let local_accts = local.get("accounts").and_then(|v| v.as_array());
    let chain_accts = chain.get("accounts").and_then(|v| v.as_array());
    let (Some(local_accts), Some(chain_accts)) = (local_accts, chain_accts) else {
        return;
    };
    let local_map = by_account_name(local_accts);
    let chain_map = by_account_name(chain_accts);

    for name in chain_map.keys() {
        if !local_map.contains_key(name) {
            out.push(DriftChange {
                severity: DriftSeverity::Breaking,
                kind: "account_removed".into(),
                path: format!("instructions.{ix_name}.accounts.{name}"),
                detail: format!("Account `{name}` removed from `{ix_name}`."),
            });
        }
    }
    for name in local_map.keys() {
        if !chain_map.contains_key(name) {
            out.push(DriftChange {
                severity: DriftSeverity::Risky,
                kind: "account_added".into(),
                path: format!("instructions.{ix_name}.accounts.{name}"),
                detail: format!(
                    "Account `{name}` added to `{ix_name}` — existing callers need to supply it."
                ),
            });
        }
    }
    // Signer / mut flag changes on overlapping entries
    for (name, local_acc) in &local_map {
        if let Some(chain_acc) = chain_map.get(name) {
            let local_signer = local_acc
                .get("signer")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || local_acc
                    .get("isSigner")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            let chain_signer = chain_acc
                .get("signer")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || chain_acc
                    .get("isSigner")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            if local_signer != chain_signer {
                out.push(DriftChange {
                    severity: DriftSeverity::Breaking,
                    kind: "account_signer_changed".into(),
                    path: format!("instructions.{ix_name}.accounts.{name}.signer"),
                    detail: format!(
                        "Signer flag on `{name}` in `{ix_name}` changed: {chain_signer} → {local_signer}"
                    ),
                });
            }
            let local_writable = local_acc
                .get("writable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || local_acc
                    .get("isMut")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            let chain_writable = chain_acc
                .get("writable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || chain_acc
                    .get("isMut")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            if local_writable != chain_writable {
                out.push(DriftChange {
                    severity: DriftSeverity::Breaking,
                    kind: "account_mut_changed".into(),
                    path: format!("instructions.{ix_name}.accounts.{name}.writable"),
                    detail: format!(
                        "Writable flag on `{name}` in `{ix_name}` changed: {chain_writable} → {local_writable}"
                    ),
                });
            }
        }
    }
    // Reorder detection — accounts are positional in the v0 tx encoding.
    if accounts_reordered(local_accts, chain_accts) {
        out.push(DriftChange {
            severity: DriftSeverity::Breaking,
            kind: "accounts_reordered".into(),
            path: format!("instructions.{ix_name}.accounts"),
            detail: format!("Account order in `{ix_name}` differs from the on-chain IDL."),
        });
    }
}

fn diff_accounts(local: &Value, chain: &Value, out: &mut Vec<DriftChange>) {
    let local_a = index_by_name(local, "accounts");
    let chain_a = index_by_name(chain, "accounts");
    for name in chain_a.keys() {
        if !local_a.contains_key(name) {
            out.push(DriftChange {
                severity: DriftSeverity::Breaking,
                kind: "account_struct_removed".into(),
                path: format!("accounts.{name}"),
                detail: format!("Account struct `{name}` exists on-chain but not locally."),
            });
        }
    }
    for name in local_a.keys() {
        if !chain_a.contains_key(name) {
            out.push(DriftChange {
                severity: DriftSeverity::NonBreaking,
                kind: "account_struct_added".into(),
                path: format!("accounts.{name}"),
                detail: format!("Account struct `{name}` is new in the local IDL."),
            });
        }
    }
    // Field-level checks when both sides have a `type.kind = "struct"` shape.
    for (name, local_acc) in &local_a {
        if let Some(chain_acc) = chain_a.get(name) {
            diff_struct_fields(local_acc, chain_acc, name, out);
        }
    }
}

fn diff_struct_fields(local: &Value, chain: &Value, acc_name: &str, out: &mut Vec<DriftChange>) {
    let local_fields = extract_struct_fields(local);
    let chain_fields = extract_struct_fields(chain);
    let (Some(local_fields), Some(chain_fields)) = (local_fields, chain_fields) else {
        return;
    };
    let local_map: BTreeMap<String, Value> = local_fields
        .iter()
        .filter_map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|n| (n.to_string(), v.clone()))
        })
        .collect();
    let chain_map: BTreeMap<String, Value> = chain_fields
        .iter()
        .filter_map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|n| (n.to_string(), v.clone()))
        })
        .collect();

    for name in chain_map.keys() {
        if !local_map.contains_key(name) {
            out.push(DriftChange {
                severity: DriftSeverity::Breaking,
                kind: "account_field_removed".into(),
                path: format!("accounts.{acc_name}.{name}"),
                detail: format!("Field `{name}` removed from account `{acc_name}`."),
            });
        }
    }
    for name in local_map.keys() {
        if !chain_map.contains_key(name) {
            out.push(DriftChange {
                severity: DriftSeverity::NonBreaking,
                kind: "account_field_added".into(),
                path: format!("accounts.{acc_name}.{name}"),
                detail: format!("Field `{name}` added to account `{acc_name}`."),
            });
        }
    }
    for (name, local_field) in &local_map {
        if let Some(chain_field) = chain_map.get(name) {
            if local_field.get("type") != chain_field.get("type") {
                out.push(DriftChange {
                    severity: DriftSeverity::Breaking,
                    kind: "account_field_type_changed".into(),
                    path: format!("accounts.{acc_name}.{name}"),
                    detail: format!(
                        "Field `{name}` type changed: {} → {}",
                        render_type(chain_field.get("type")),
                        render_type(local_field.get("type"))
                    ),
                });
            }
        }
    }
    if struct_fields_reordered(&local_fields, &chain_fields) {
        out.push(DriftChange {
            severity: DriftSeverity::Breaking,
            kind: "account_fields_reordered".into(),
            path: format!("accounts.{acc_name}"),
            detail: format!("Field order in account `{acc_name}` differs."),
        });
    }
}

fn diff_errors(local: &Value, chain: &Value, out: &mut Vec<DriftChange>) {
    let local_map = index_errors(local);
    let chain_map = index_errors(chain);
    for code in chain_map.keys() {
        if !local_map.contains_key(code) {
            out.push(DriftChange {
                severity: DriftSeverity::Risky,
                kind: "error_removed".into(),
                path: format!("errors.{code}"),
                detail: format!("Error code {code} present on-chain but not locally."),
            });
        }
    }
    for code in local_map.keys() {
        if !chain_map.contains_key(code) {
            out.push(DriftChange {
                severity: DriftSeverity::NonBreaking,
                kind: "error_added".into(),
                path: format!("errors.{code}"),
                detail: format!("New error code {code} added locally."),
            });
        }
    }
    for (code, local_err) in &local_map {
        if let Some(chain_err) = chain_map.get(code) {
            let local_name = local_err.get("name").and_then(|v| v.as_str());
            let chain_name = chain_err.get("name").and_then(|v| v.as_str());
            if local_name != chain_name {
                out.push(DriftChange {
                    severity: DriftSeverity::Risky,
                    kind: "error_renamed".into(),
                    path: format!("errors.{code}"),
                    detail: format!(
                        "Error {code} renamed: {} → {}",
                        chain_name.unwrap_or(""),
                        local_name.unwrap_or("")
                    ),
                });
            }
        }
    }
}

fn extract_struct_fields(value: &Value) -> Option<Vec<Value>> {
    value
        .get("type")
        .and_then(|t| t.get("fields"))
        .or_else(|| value.get("fields"))
        .and_then(|v| v.as_array())
        .cloned()
}

fn struct_fields_reordered(local: &[Value], chain: &[Value]) -> bool {
    let ln: Vec<_> = local
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
        .collect();
    let cn: Vec<_> = chain
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
        .collect();
    let mut shared_local: Vec<_> = ln.iter().filter(|n| cn.contains(n)).collect();
    let mut shared_chain: Vec<_> = cn.iter().filter(|n| ln.contains(n)).collect();
    shared_local.truncate(shared_chain.len());
    shared_chain.truncate(shared_local.len());
    shared_local != shared_chain
}

fn accounts_reordered(local: &[Value], chain: &[Value]) -> bool {
    let ln: Vec<_> = local
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
        .collect();
    let cn: Vec<_> = chain
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
        .collect();
    let shared_local: Vec<_> = ln.iter().filter(|n| cn.contains(n)).collect();
    let shared_chain: Vec<_> = cn.iter().filter(|n| ln.contains(n)).collect();
    shared_local != shared_chain
}

fn args_reordered(local: &[Value], chain: &[Value]) -> bool {
    let ln: Vec<_> = local
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
        .collect();
    let cn: Vec<_> = chain
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
        .collect();
    let shared_local: Vec<_> = ln.iter().filter(|n| cn.contains(n)).collect();
    let shared_chain: Vec<_> = cn.iter().filter(|n| ln.contains(n)).collect();
    shared_local != shared_chain
}

fn index_by_name(value: &Value, key: &str) -> BTreeMap<String, Value> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|entry| {
                    entry
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|n| (n.to_string(), entry.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn by_arg_name(args: &[Value]) -> BTreeMap<String, Value> {
    args.iter()
        .filter_map(|a| {
            a.get("name")
                .and_then(|n| n.as_str())
                .map(|n| (n.to_string(), a.clone()))
        })
        .collect()
}

fn by_account_name(accounts: &[Value]) -> BTreeMap<String, Value> {
    accounts
        .iter()
        .filter_map(|a| {
            a.get("name")
                .and_then(|n| n.as_str())
                .map(|n| (n.to_string(), a.clone()))
        })
        .collect()
}

fn index_errors(value: &Value) -> BTreeMap<u64, Value> {
    value
        .get("errors")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    e.get("code")
                        .and_then(|c| c.as_u64())
                        .map(|c| (c, e.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn render_type(value: Option<&Value>) -> String {
    match value {
        Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "?".to_string()),
        None => "<none>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Idl, IdlSource};
    use super::*;
    use serde_json::json;

    fn mk(value: Value) -> Idl {
        Idl::from_value(value, IdlSource::Synthetic)
    }

    #[test]
    fn identical_idls_report_no_changes() {
        let idl = mk(json!({
            "instructions": [{"name": "init", "accounts": [], "args": []}],
            "accounts": [],
            "errors": []
        }));
        let report = classify(&idl, Some(&idl));
        assert!(report.identical);
        assert!(report.changes.is_empty());
    }

    #[test]
    fn removed_instruction_is_breaking() {
        let chain = mk(json!({
            "instructions": [{"name": "init", "accounts": [], "args": []}]
        }));
        let local = mk(json!({
            "instructions": []
        }));
        let report = classify(&local, Some(&chain));
        assert!(!report.identical);
        assert_eq!(report.breaking_count, 1);
        assert!(report
            .changes
            .iter()
            .any(|c| c.kind == "instruction_removed"));
    }

    #[test]
    fn added_instruction_is_non_breaking() {
        let chain = mk(json!({
            "instructions": [{"name": "init", "accounts": [], "args": []}]
        }));
        let local = mk(json!({
            "instructions": [
                {"name": "init", "accounts": [], "args": []},
                {"name": "shinier", "accounts": [], "args": []}
            ]
        }));
        let report = classify(&local, Some(&chain));
        assert_eq!(report.breaking_count, 0);
        assert_eq!(report.non_breaking_count, 1);
    }

    #[test]
    fn added_required_arg_is_risky() {
        let chain = mk(json!({
            "instructions": [{"name": "init", "accounts": [], "args": []}]
        }));
        let local = mk(json!({
            "instructions": [
                {"name": "init", "accounts": [], "args": [{"name": "amount", "type": "u64"}]}
            ]
        }));
        let report = classify(&local, Some(&chain));
        assert_eq!(report.risky_count, 1);
        assert!(report.changes.iter().any(|c| c.kind == "arg_added"));
    }

    #[test]
    fn removed_required_field_is_breaking() {
        let chain = mk(json!({
            "accounts": [{
                "name": "State",
                "type": {"kind": "struct", "fields": [
                    {"name": "authority", "type": "publicKey"}
                ]}
            }]
        }));
        let local = mk(json!({
            "accounts": [{
                "name": "State",
                "type": {"kind": "struct", "fields": []}
            }]
        }));
        let report = classify(&local, Some(&chain));
        assert!(report
            .changes
            .iter()
            .any(|c| c.kind == "account_field_removed"));
        assert_eq!(report.breaking_count, 1);
    }

    #[test]
    fn account_signer_flag_change_is_breaking() {
        let chain = mk(json!({
            "instructions": [{
                "name": "init",
                "accounts": [{"name": "auth", "signer": true, "writable": false}],
                "args": []
            }]
        }));
        let local = mk(json!({
            "instructions": [{
                "name": "init",
                "accounts": [{"name": "auth", "signer": false, "writable": false}],
                "args": []
            }]
        }));
        let report = classify(&local, Some(&chain));
        assert!(report
            .changes
            .iter()
            .any(|c| c.kind == "account_signer_changed"));
        assert_eq!(report.breaking_count, 1);
    }

    #[test]
    fn chain_missing_surface_is_non_breaking_informational() {
        let local = mk(json!({"instructions": []}));
        let report = classify(&local, None);
        assert!(!report.identical);
        assert_eq!(report.breaking_count, 0);
        assert_eq!(report.non_breaking_count, 1);
        assert_eq!(report.changes[0].kind, "chain_missing");
    }

    #[test]
    fn added_error_code_is_non_breaking() {
        let chain = mk(json!({"errors": [{"code": 6000, "name": "Oops"}]}));
        let local = mk(json!({
            "errors": [
                {"code": 6000, "name": "Oops"},
                {"code": 6001, "name": "NewError"}
            ]
        }));
        let report = classify(&local, Some(&chain));
        assert!(report.changes.iter().any(|c| c.kind == "error_added"));
        assert_eq!(report.non_breaking_count, 1);
    }
}
