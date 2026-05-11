- Use ShadCN for all UI where possible
- NEVER add temporary debug or test UI during development. Use unit/e2e test only. The only UI you should ever add is user facing only.
- When executing python commands, ALWAYS use python3
- You CANNOT open this app in a browers, this is a Tauri app
- Only run one Cargo caommand at a time to avoid the lock
- `.xero/` is legacy repo-local state. New project state belongs under the OS app-data directory.
- This is a new application, backwards compatability is prohibited unless asked for
- Build prerequisite: `protoc` must be on PATH (the LanceDB-backed agent memory store pulls lance-* crates whose build scripts compile vendored .proto files). On macOS: `brew install protobuf`.
- Run scooped tests and format instead of repo wide when working with rust to save time and storage
- Dont create branches or stash unless user asks, there may be multiple agents working at the same time and doing this will break things

## Stages vs. Workflow (terminology)

Two distinct concepts share the agent-canvas surface — keep them separate when naming things:

- **Stages** — gated phases *inside a single agent run*. The runtime enforces per-stage tool allowlists and required-check gates via `enforce_agent_workflow_before_tool` (`client/src-tauri/src/runtime/autonomous_tool_runtime/mod.rs`). The on-wire DTO is still `CustomAgentWorkflowPhaseDto` / `workflowStructure.phases` (legacy name; do not rename without a coordinated migration).
- **Workflow** — reserved for the future *multi-agent pipeline* feature (Agent A's output feeds Agent B). Not implemented yet.

User-facing strings on the canvas say "Stages" / "Stage." The word "Workflow" in the UI must only refer to either the top-bar Workflow tab (the canvas itself) or the future multi-agent feature. Don't reintroduce the collision by labeling per-run phases as "workflow phases" anywhere a user can see it.