# Debug Agent Implementation Plan

## Goal

Add a third agent composer option, `Debug`, alongside `Ask` and `Engineer`. Debug should behave like a rich investigation-focused engineering agent: it can inspect, edit, and verify, but its workflow should emphasize evidence capture, hypothesis testing, root-cause analysis, fix validation, and durable session memory.

## Constraints

- Use the existing ShadCN-based composer controls and runtime patterns.
- Do not add temporary debug or test-only UI.
- This is a Tauri app, so verification should use unit and Rust tests rather than browser checks.
- Run scoped tests and formatting only.
- Keep project state in the existing OS app-data backed stores; do not use `.xero/`.
- Preserve existing dirty worktree changes that are unrelated to this implementation.

## Implementation Steps

1. Add `debug` to the shared runtime-agent contract.
   - Extend TypeScript schemas and descriptors in `client/src/lib/xero-model/runtime.ts`.
   - Extend Rust `RuntimeAgentIdDto` parsing, labels, approval-mode rules, and plan/verification gate support.
   - Update SQLite runtime-agent `CHECK` constraints for new installs.

2. Add Debug to the composer UI.
   - Reuse existing ShadCN/Radix controls.
   - Add a distinct icon and labels for the Debug option.
   - Allow the same approval selector modes as Engineer.
   - Keep Ask observe-only behavior unchanged.

3. Add Debug runtime prompt and tool policy.
   - Give Debug full engineering tool access.
   - Require a structured debugging workflow: intake, reproduction, evidence ledger, hypotheses, experiments, root cause, fix, regression verification, and session summary.
   - Instruct Debug to preserve useful debugging facts for future retrieval while treating memory as lower-priority context.

4. Ensure durable LanceDB project records are useful for Debug sessions.
   - Keep the existing run-handoff persistence path that writes project records through the Lance-backed project record store.
   - Add Debug-specific structured content fields/tags where possible, so later retrieval can distinguish debugging sessions, fixes, root causes, verification evidence, and affected paths.
   - Avoid adding backwards compatibility shims unless needed for current tests.

5. Update targeted tests.
   - TypeScript runtime schema/descriptor tests for `debug`.
   - React composer tests for Debug selection and approval behavior.
   - Rust contract/prompt/tool filtering tests for Debug.
   - Scoped test commands only.

## Expected User-Facing Behavior

- The composer agent selector shows `Ask`, `Engineer`, and `Debug`.
- Selecting `Debug` keeps the rich model/thinking/approval controls available.
- Debug runs use a structured system prompt that pushes the agent to document evidence, hypotheses, fixes, and verification.
- Debug final summaries and run handoffs are saved as LanceDB-backed project records with `debug` runtime metadata and retrieval tags.

## Verification Plan

- Run focused TypeScript tests around runtime models and agent runtime composer.
- Run focused Rust tests around runtime contracts, tool descriptors, state-machine gates, persistence, and project-record parsing.
- Run scoped formatting for touched TypeScript/Rust files if the repo tooling supports it cleanly.
