# Tool extensions

Xero tool extensions are local executable bundles that add permissioned tools to the owned-agent Tool Registry V2. They are distinct from Skills and MCP servers.

## Managed location

The desktop app only discovers installed bundles beneath the Tauri OS app-data directory:

```text
<app-data>/tool-extensions/<extensionId>/
├── manifest.json
├── <executable>
└── installation.json
```

Repository-local `.xero/` state and arbitrary folders are never scanned. The Agent Tooling settings panel shows the resolved managed directory. A user explicitly selects a source bundle to install; Xero copies the manifest and executable into app data. Installation and upgrades always reset the extension to disabled.

## Manifest

`manifest.json` uses contract version 1. A minimal read-only extension looks like this:

```json
{
  "contractVersion": 1,
  "extensionId": "acme.release_notes",
  "toolName": "acme_release_notes",
  "label": "Release notes",
  "description": "Summarize approved changelog input.",
  "inputSchema": {
    "type": "object",
    "properties": { "text": { "type": "string" } },
    "required": ["text"]
  },
  "permission": {
    "permissionId": "acme_release_notes_read",
    "label": "Read changelog input",
    "effectClass": "observe",
    "riskClass": "low",
    "auditLabel": "acme_release_notes"
  },
  "mutability": "read_only",
  "sandboxRequirement": "read_only",
  "approvalRequirement": "policy",
  "capabilityTags": ["release_notes"],
  "testFixtures": [
    {
      "fixtureId": "basic",
      "input": { "text": "Fixed startup" },
      "expectedSummaryContains": "startup"
    }
  ],
  "runtime": {
    "kind": "process",
    "executable": "handler",
    "args": []
  }
}
```

The executable must be a regular, executable file in the bundle root. Symlinks and path traversal are rejected. Tool names and permission IDs must not collide with built-in or other installed capabilities.

## Process protocol

Xero writes one JSON request followed by a newline to stdin:

```json
{
  "contractVersion": 1,
  "extensionId": "acme.release_notes",
  "toolName": "acme_release_notes",
  "toolCallId": "call-1",
  "context": {
    "projectId": "project-1",
    "runId": "run-1",
    "turnIndex": 0,
    "contextEpoch": "turn-0",
    "telemetryAttributes": {}
  },
  "input": { "text": "Fixed startup" }
}
```

The executable must write one JSON response to stdout and exit successfully:

```json
{
  "summary": "Summarized startup change.",
  "output": { "text": "Fixed startup" }
}
```

## Trust and lifecycle

- Required fixtures run inside the standard OS sandbox before installation and again before enablement.
- Enabling requires an exact grant of the manifest's declared `permissionId` in the Agent Tooling UI.
- The manifest and executable are hashed. Tampering after verification disables registration and causes in-flight calls to fail closed.
- Calls use the standard central policy, sandbox, deadline, cancellation, panic-containment, audit, and output-truncation pipeline.
- Each call runs in a separate process. Deadlines and cancellation terminate the process tree.
- Disable and removal take effect when the registry reloads; execution also rechecks enablement and integrity immediately before launch.
- Reinstalling the same `extensionId` is an upgrade and resets permission grant and enablement.
- Arbitrary mutating extensions are currently rejected. Xero will not register them until the host can guarantee complete rollback and mutation quarantine in addition to killable process isolation.

