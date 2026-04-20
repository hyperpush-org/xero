// Compatibility shim for older verification gates that still reference
// `cargo test --test autonomous_skill_runtime`.
//
// The canonical integration test coverage now lives in `autonomous_tool_runtime.rs`.
include!("autonomous_tool_runtime.rs");
