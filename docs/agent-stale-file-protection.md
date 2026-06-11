# Agent Stale-File Protection

This document is for engineers adding or reviewing agent-accessible mutation paths. After reading it, you should know which paths can change project files, what stale-file guard they use, and what contract new file mutations must follow.

## Contract

Owned agents must observe current file content before mutating an existing file. The observation can come from `read`, `read_many`, `search`, `find`, `list`, `list_tree`, `directory_digest`, `file_hash`, or a successful prior mutation output in the same run. Runtime preflight rejects owned-agent writes when an existing file has not been observed or when the file hash no longer matches the last observation.

Every owned-agent existing-file mutation must also carry a content guard in the tool request. Use `expectedHash` for direct file mutations and `expectedSourceHash` for copy source files. Create-only writes to missing files do not need a hash and must reject an `expectedHash`, because there is no existing file snapshot to guard.

Stale tool-level failures use stable code `autonomous_tool_stale_file` and include `currentHash` plus a required action to re-read current file evidence before retrying. Missing owned-agent hash guards use `autonomous_tool_expected_hash_required` and also include the current hash. Agent preflight can additionally fail with `agent_file_write_requires_observation` or `agent_file_changed_since_observed`; both require a fresh read/hash before retry.

Reservations are advisory ownership leases, not content freshness. A reservation now records `observedHash` and `observedAt` for existing regular files at claim time so reviewers can compare coordination ownership with the content snapshot, but reservation presence never replaces `expectedHash`.

## Mutation Inventory

| Path | Protection status |
| --- | --- |
| `edit` | Mandatory for owned-agent applies: `expectedHash` on existing file, optional line hashes, runtime observation preflight. Preview validates provided hashes but does not require one. |
| `write` | Mandatory for owned-agent overwrites: `expectedHash`, explicit `overwrite=true`, runtime observation preflight. Create-only missing-file writes are allowed without a hash. |
| `patch` | Mandatory for owned-agent applies: each operation must include `expectedHash`; multi-file plans validate all files before any write and roll back earlier file writes if persistence fails. |
| `json_edit`, `toml_edit`, `yaml_edit` | Mandatory for owned-agent applies: parser-backed edit requires `expectedHash` and runtime observation preflight. |
| `notebook_edit` | Mandatory for owned-agent applies: notebook file `expectedHash`; optional `expectedSource` is only a cell-level extra guard. |
| `delete` | Mandatory for owned-agent file deletes: `expectedHash`. Directory deletes use preview-derived `expectedDigest`. |
| `rename` | Mandatory for owned-agent file sources: `expectedHash`. Existing file targets require `expectedTargetHash` before overwrite. |
| `copy` | Mandatory for owned-agent file copies: `expectedSourceHash` for source file content and `expectedTargetHash` for existing file targets. Directory copies use preview-derived `expectedSourceDigest`. |
| `fs_transaction` | Mandatory for owned-agent apply operations through the underlying operation guards. Planning validates stale hashes before backup/write. Apply uses rollback backups and reports rollback status if an operation fails after mutation starts. |
| `mkdir` | Create-only directory mutation; no file content hash applies. Runtime path/epoch preflight still covers owned-agent path intent. |
| Command/shell tools | Not hash-guarded because the runtime cannot inspect arbitrary command semantics. Command policy separates probe/verify/general execution, and command results with changed files invalidate matching observed hashes; truncated changed-file reports clear all observed hashes. |
| MCP tool invocation | External capability invocation is policy-gated but content-hash protection is not mandatory because MCP tools are opaque. Treat MCP mutations like command mutations: re-read/hash files before later structured writes. |
| Project editor saves and code-history undo/revert | Human/system-side mutations advance code workspace history and invalidate overlapping reservations. Owned agents must acknowledge/refresh workspace epoch and re-read files before writing. |
| Agent definition and workflow definition writes | App-data definition mutations, not repo file edits. They are schema-validated and audit-recorded; repo stale-file hashes do not apply. |
| Project context, mailbox, todo, coordination writes | App-data state mutations, not repo file edits. They are protected by app-data validation and TTL/audit semantics rather than project-file hashes. |

## Retry Rule

Never retry a stale write by changing only the hash value from an error message. The agent must call `read`, `read_many`, or `file_hash` for the affected path, reconcile the current content with the intended change, then retry with the new guard. This protects against human edits, sibling-agent edits, command mutations, history undo/revert, and stale app-data snapshots.

