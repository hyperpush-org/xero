# Environment Profile Delivery To Agents

Issue: <https://github.com/hyperpush-org/xero/issues/11>

Date: 2026-05-30

## Decision

Keep developer environment facts behind the existing `environment_context` tool. Do not inject a raw installed-tool list into default prompts or every provider turn. Improve discoverability by making the same tool searchable as `fetch_dev_tools` for installed developer tool availability, without adding a second tool name, state migration, or compatibility layer.

## Lifecycle

Startup is non-blocking. `client/src/App.tsx` calls `refreshEnvironmentDiscovery()` once after boot, catches failures, and leaves diagnostics to later surfaces.

The Tauri command boundary is `client/src-tauri/src/commands/environment_discovery.rs`. It resolves the OS app-data global database path through `DesktopState::global_db_path`, then calls `environment::service`.

`environment::service::start_environment_discovery_with_policy` uses an in-process active-discovery set keyed by database path, persists a marker `environment_profile` row with status `probing`, then spawns a worker thread. The profile is stale after seven days, or immediately stale for `pending`, `probing`, and `failed` rows.

The worker builds the profile in `environment::probe`: it records platform and PATH fingerprint metadata, probes built-in and user-added tool catalog entries with bounded command executions, derives capabilities such as `node_project_ready`, `tauri_desktop_build`, and `protobuf_build_ready`, validates the payload/summary, and persists both full payload and redacted summary to the global `environment_profile` table.

Persistence is global app-data, not repo-local `.xero/` state. The schema stores one row (`id = 1`) with `payload_json`, `summary_json`, `permission_requests_json`, diagnostics, and timestamps.

Redaction happens before agent delivery. Summary tool paths are display paths such as `~/bin/node`; raw absolute paths remain in the persisted payload but are not part of the summary delivered to agents. Validation rejects secret-like serialized strings and summary absolute paths.

Permission requests are persisted on the profile and can be resolved through `resolve_environment_permission_requests`; pending requests are surfaced in onboarding. The current built-in probe path does not request mandatory OS permissions.

Diagnostics are available through status/summary commands and `doctor_report`; stale or failed app-data state should be wiped/rebuilt rather than handled with backwards-compatible glue unless compatibility is explicitly requested.

## Agent Delivery Paths

Default prompt fragments do not include environment facts. `PromptCompiler` assembles runtime policy, metadata, repository instructions, workspace manifest, optional process/working-set/coordination summaries, durable-context guidance, and active tool names. The focused test `prompt_compiler_does_not_include_environment_facts_by_default` asserts `environment_context`, `fetch_dev_tools`, `protoc`, and `node_project_ready` are absent by default.

Provider tool descriptors are separate from prompt text. Default Ask startup does not activate `environment_context`; Crawl activates it for repository reconnaissance. Explicit `tool:environment_context` and `tool:fetch_dev_tools` markers can activate the descriptor, and `tool_search`/`tool_access` can discover or grant it when a task needs installed tool facts.

Tool search exposes metadata only: tool name, group, description, aliases/keywords, schema field names, and risk class. It does not expose the installed-tool list or profile payload.

`environment_context` is the only agent tool that reads profile facts. It supports `summary`, `tool`, `category`, `capability`, and `refresh` actions and returns redacted summary facts from global app-data.

Model-visible tool results are compacted through provider-loop projection format `environment_context_summary_json`. The full result remains persisted; the provider sees the compact projection. Focused tests now sample summary/category/tool/capability outputs and enforce byte budgets.

Context manifests do not inject raw environment facts. Provider context manifests record active tool names/counts, prompt fragment IDs, retrieval policy, and `rawContextInjected: false`; they do not include profile payloads or installed-tool rows unless a prior tool result is part of normal model-visible transcript history.

## Token And Privacy Tradeoffs

Raw list in every session: highest discoverability, but repeats every turn, grows with custom tools, and expands privacy surface through local tool inventory and path hints. It is not justified while tool-mediated access exists.

Current on-demand `environment_context`: no default prompt cost, bounded cost only when needed, redacted display paths, refresh/staleness metadata, and scoped actions for small calls. Unit-test budgets cap representative model-visible projections at 24 KB for summary (~6k tokens; current sampled catalog is about 20.5 KB), 8 KB for category (~2k tokens), 4 KB for specific tools (~1k tokens), and 3 KB for capabilities (~750 tokens).

New or renamed `fetch_dev_tools`: clearer name for one use case, but it duplicates the existing boundary or requires migration across descriptors, allowlists, compaction, tests, and saved instructions. An alias in discovery gives most of the discoverability benefit without a second runtime surface.

## Recommendation

Use `environment_context` as the default boundary. Keep environment facts out of default prompt context and provider context manifests. Let agents discover it through `tool_search` with terms like `fetch_dev_tools`, request the exact tool or environment group through `tool_access`, then call the narrowest action that answers the task.

Add a new tool only if future usage data shows agents consistently fail to discover `environment_context` despite the alias and catalog wording.
