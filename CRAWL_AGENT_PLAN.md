# Crawl Agent Plan

Reader: internal Xero engineer implementing a new built-in owned-agent type.

Post-read action: add the `Crawl` agent so it is available only for brownfield projects, can map an existing repository without editing it, and persists useful project knowledge into app-data LanceDB project records for later agents.

Last reviewed: 2026-05-06.

## Decision

Add `Crawl` as a built-in reconnaissance agent for existing repositories. Crawl should answer one question: "What should a future Xero agent know before working safely and efficiently in this brownfield codebase?"

Crawl is not a build agent, debugging agent, security audit, dependency auditor, or migration tool. It should read, summarize, and persist durable project context. It must not mutate repository files, create temporary UI, run destructive commands, install dependencies, rewrite state, or use the legacy repo-local `.xero/` directory.

The main output should be a structured crawl report plus smaller retrievable project records in the existing LanceDB-backed project records store. The readable conversation response is secondary; the durable value is what Ask, Engineer, and Debug can retrieve later.

## Product Behavior

### When Crawl Appears

Crawl is shown only when the selected project is known to be brownfield.

Define brownfield from project creation source, not from heuristics:

- `Open existing` marks the project as brownfield.
- `Create new` marks the project as greenfield.
- Unknown source is not brownfield.
- Direct attempts to start Crawl on greenfield or unknown projects fail server-side with a user-fixable diagnostic.

This needs a persisted project-origin field in app-data state. UI hiding alone is not enough because runtime commands can be called directly.

### User-Facing Role

Label: `Crawl`

Short label: `Crawl`

Description: "Map an existing repository, identify stack, tests, commands, architecture, hot spots, and durable project facts without editing files."

Prompt placeholder when selected: "Map this project..."

Crawl should use the normal Agent tab runtime stream. Do not add a separate temporary progress UI. If a lightweight nudge is useful after importing an existing repo, use the existing ShadCN surfaces and make it a real user-facing action, not development-only UI.

### Run Shape

Crawl should be a manual one-shot run. It may be suggested after brownfield import, but it should not automatically perform a full scan without the user starting it.

The run should:

1. Confirm project origin is brownfield.
2. Check workspace index status and use it if fresh enough.
3. Read repo manifests, instruction files, package/workspace configuration, test configuration, and high-level source layout.
4. Use safe git metadata when available.
5. Identify unknowns and confidence instead of guessing.
6. Emit a structured final report.
7. Persist project records to LanceDB through runtime-owned ingestion.

By default Crawl should discover test commands, not run the entire test suite. Full test execution can be expensive and side-effectful in brownfield repos. If the user asks Crawl to verify commands, it should request/observe normal approval policy and prefer the smallest safe command.

## Agent Definition

### Runtime Identity

Add a built-in runtime agent id: `crawl`.

Add matching runtime enum/schema entries, display descriptors, agent-definition seed data, parse functions, handoff/retrieval filters, and tests wherever Ask, Engineer, Debug, Agent Create, and Test are enumerated.

### Capability Profile

Use a new least-privilege capability profile such as `repository_recon`.

Allowed capabilities:

- File and directory reading.
- Search/find/list.
- Git status, diff summary, branch/head metadata, and bounded git history/churn inspection.
- Workspace index status/query/explain, including related tests and change-impact summaries.
- Environment context and system diagnostics that are read-only.
- Read-only command discovery commands, when policy can classify them as non-mutating.
- Project-context retrieval.

Blocked capabilities:

- File writes, patching, deletes, renames, mkdir, notebook edits.
- Process control beyond bounded read-only commands.
- Browser, emulator, Solana, MCP invocation, and external service tools by default.
- Dependency installation, migrations, database resets, long-running dev servers, broad test execution, or app launch.
- Any write to `.xero/`.

For persistence, prefer runtime-owned report ingestion over giving the model a broad write tool. Crawl can produce structured output; the runtime validates, redacts, and writes records.

### Prompt Policy

Add a `crawl` prompt policy with these core instructions:

- You are Xero's Crawl agent for brownfield repository reconnaissance.
- Do not edit repository files or app state.
- Prioritize facts useful to future Ask, Engineer, and Debug runs.
- Prefer manifests, existing docs, instruction files, workspace index, and git metadata over assumptions.
- Record uncertainty and confidence.
- Never persist secrets or raw credential values.
- Do not run broad test/build/install commands unless the user explicitly asks and approval allows it.
- Produce the final response in the required Crawl report schema and a short human-readable summary.

### Output Contract

Add output contract `crawl_report`.

The model-facing final answer can be readable Markdown, but the runtime should require and parse a structured report payload. If the provider cannot emit clean structured output, the runtime should fail with a diagnostic rather than storing vague prose as authoritative project memory.

## What Crawl Collects

Crawl should collect enough to help future agents start fast, without becoming a full audit.

### Project Identity

- Project name and likely product/domain.
- Repository origin: brownfield.
- Git status summary, current branch/head, remotes when available.
- Monorepo or single-app shape.
- Generated, vendored, build-output, and ignored directories to avoid.
- Important instruction files and local project rules.

### Tech Stack

- Languages and major frameworks.
- Package managers and workspace tools.
- Build systems, task runners, linters, formatters, test frameworks.
- Desktop/mobile/server/runtime boundaries where relevant.
- Databases, queues, local services, and external integrations visible from manifests/config.
- Native toolchain prerequisites.

### Command Map

- Setup commands.
- Dev commands.
- Build commands.
- Lint/format/typecheck commands.
- Test commands.
- Scoped command patterns for common slices.
- Known command constraints, such as "only one Cargo command at a time."

Store whether each command is discovered from a manifest, docs, instruction file, or inferred. Inferred commands should have lower confidence.

### Test Map

- Test frameworks.
- Unit, integration, e2e, smoke, benchmark, fixture, and support test locations.
- How to run all tests and scoped tests.
- Tests requiring services, devices, network, credentials, or platform-specific tools.
- Slow or expensive suites when discoverable.
- Related-test patterns that future agents should use before broad runs.

The test map should be one of the most important Crawl artifacts.

### Architecture Map

- Main applications/packages/crates/services.
- Runtime entry points and process boundaries.
- UI, backend, persistence, command/IPC/API, background job, and tool/plugin boundaries.
- Shared libraries and cross-cutting modules.
- Data storage locations and state ownership rules.
- Generated artifacts and source-of-truth files.

This should be a map, not a deep code tour.

### Project Hot Spots

Hot spots should be explainable, not just ranked numbers. Combine lightweight signals:

- Recent git churn.
- Large or highly connected files/modules.
- Central shared code used by many areas.
- Files with many related failures, diagnostics, TODO/FIXME markers, or fragile comments.
- Areas touching auth, secrets, persistence, migrations, sandboxing, provider calls, process execution, or file mutation.
- Areas with sparse or unclear related tests.

Each hot spot should include why it matters, likely owner/domain if discoverable, related tests, and confidence.

### Constraints And Conventions

- Repo-specific agent instructions.
- Formatting and style constraints.
- Required local prerequisites.
- State/persistence constraints.
- Security and privacy constraints.
- Test execution caveats.
- "Do not touch" or "ask first" areas.

### Existing Durable Context

Crawl should query existing project records and approved memory before storing new records. If a previous crawl exists and is still fresh, it should avoid duplicating it. If facts changed, it should supersede older records.

## LanceDB Persistence

Use the existing per-project app-data LanceDB project records store. Do not create a new Lance table for the first version unless the existing `project_records` shape cannot support the report.

### Record Strategy

Persist one aggregate artifact plus topic records:

| Schema | Record kind | Purpose |
| --- | --- | --- |
| `xero.project_crawl.report.v1` | `artifact` | Full structured crawl report and freshness manifest. |
| `xero.project_crawl.project_overview.v1` | `project_fact` | Identity, repo shape, source, high-level summary. |
| `xero.project_crawl.tech_stack.v1` | `project_fact` | Languages, frameworks, package managers, services. |
| `xero.project_crawl.command_map.v1` | `context_note` | Setup, dev, build, lint, format, and scoped command guidance. |
| `xero.project_crawl.test_map.v1` | `verification` | Test locations, frameworks, run commands, caveats. |
| `xero.project_crawl.architecture_map.v1` | `context_note` | Major boundaries, entry points, state ownership. |
| `xero.project_crawl.hotspots.v1` | `finding` | Ranked hot spots with reasons and related tests. |
| `xero.project_crawl.constraints.v1` | `constraint` | Durable project rules and local caveats. |
| `xero.project_crawl.unknowns.v1` | `question` | Important missing or uncertain information. |
| `xero.project_crawl.freshness.v1` | `diagnostic` | Scan coverage, source fingerprints, stale reasons. |

Use stable `fact_key` values such as `crawl:tech-stack` and `crawl:test-map` so recrawls can supersede old records cleanly.

### Record Metadata

Every Crawl record should include:

- `runtime_agent_id = crawl`.
- `agent_definition_id = crawl`.
- `visibility = retrieval` for durable facts and `diagnostic` for crawl diagnostics.
- Confidence score.
- Importance, with test map, constraints, and overview at high importance.
- Tags such as `crawl`, `brownfield`, `tests`, `tech-stack`, `hotspots`, `commands`.
- Related paths for source manifests, test roots, key modules, and docs.
- Source item ids when derived from tool calls.
- Source fingerprints for freshness.
- Embedding metadata via the existing embedding backfill path.

### Redaction

Run existing redaction before storage. Crawl must not store:

- Raw secret values.
- Full env files.
- Private key material.
- Long raw logs.
- Full source files.
- Large package lock contents.
- Personal absolute paths unless already part of project state metadata and needed for diagnostics.

Prefer summaries, hashes, path references, and source fingerprints.

### Freshness

A crawl record becomes stale when important source fingerprints change:

- Project origin or repository identity.
- Git head, when available.
- Package/workspace manifests and lockfiles.
- Test config and test roots.
- Build/dev scripts.
- Instruction files.
- Major docs used as sources.

Recrawl should supersede records rather than append duplicates.

## Structured Report Shape

The parsed report should include:

```json
{
  "schema": "xero.project_crawl.report.v1",
  "projectId": "string",
  "repoRoot": "redacted-or-project-relative-string",
  "repoHead": "string-or-null",
  "generatedAt": "iso-timestamp",
  "coverage": {
    "status": "complete|partial|blocked",
    "confidence": 0.0,
    "scannedSourceCount": 0,
    "skippedReasons": []
  },
  "overview": {},
  "techStack": [],
  "commands": [],
  "tests": [],
  "architecture": [],
  "hotspots": [],
  "constraints": [],
  "unknowns": [],
  "freshness": {
    "sourceFingerprints": [],
    "staleWhen": []
  }
}
```

Use stricter typed DTOs in implementation; this sketch is only the planning shape.

## Runtime Flow

1. User selects `Crawl` on a brownfield project.
2. Frontend sends normal owned-agent start request with `runtimeAgentId = crawl`.
3. Backend validates project origin and agent availability.
4. Runtime builds a Crawl-specific prompt and read-only/recon tool registry.
5. Environment lifecycle checks workspace index. If the index is empty or stale, the run can continue with a diagnostic, but the final report must mark affected sections lower confidence.
6. Provider loop executes normally.
7. On completion, runtime parses the Crawl report.
8. Runtime validates schema, redacts, chunks into project records, writes to LanceDB, queues embeddings if needed, and records retrieval/freshness metadata.
9. Conversation shows a short summary and links the run to the stored crawl artifact.

Database write failures should block "successful" completion. A Crawl run that cannot persist its report should end failed or partial with a clear diagnostic, not pretend the project was mapped.

## Implementation Slices

### Slice 1: Project Origin

- Add persisted project origin: brownfield, greenfield, unknown.
- Set brownfield from existing-repository import.
- Set greenfield from create-new-project flow.
- Include origin in project summaries consumed by the frontend.
- Add backend availability helper that resolves Crawl availability from project origin.
- Add focused tests for import/create origin and greenfield rejection.

### Slice 2: Built-In Agent Contract

- Add `crawl` to runtime agent ids, frontend schemas, backend DTOs, descriptors, labels, defaults, and parse functions.
- Add prompt policy, tool policy, output contract, and base capability profile.
- Seed built-in agent definition version for Crawl.
- Ensure handoff preserves Crawl as Crawl if same-type handoff ever applies.
- Add prompt/tool registry contract tests.

### Slice 3: Recon Tool Policy

- Define the `repository_recon` capability profile.
- Expose read/search/list/git/workspace-index/environment-context/project-context retrieval.
- Expose only bounded read-only command discovery where policy can prove the command is safe.
- Deny repository mutation tools and broad external/device/browser surfaces.
- Add tests that Crawl cannot access edit/write/delete/patch or mutating commands.

### Slice 4: Crawl Report Ingestion

- Add typed Crawl report DTO validation.
- Add runtime completion hook for `crawl_report`.
- Convert one report into aggregate and topic project records.
- Use stable fact keys and supersession on recrawl.
- Add redaction and blocked-content behavior before LanceDB writes.
- Add LanceDB persistence tests for record kinds, schemas, tags, related paths, freshness, embeddings, and supersession.

### Slice 5: Frontend Availability

- Add Crawl descriptor to the frontend model.
- Filter it from selectors unless the selected project is brownfield.
- Add a direct disabled/hidden state test for greenfield projects.
- Add a real user-facing import nudge only if it fits the existing Agent tab UX.
- Use ShadCN components for any UI changes.

### Slice 6: Crawl Prompt And Fixtures

- Add a prompt contract test using representative brownfield fixture repos.
- Include fixtures for Node/Vite, Rust/Cargo, Phoenix/Mix, and mixed monorepo shapes.
- Verify Crawl identifies test locations and scoped commands without running broad suites.
- Verify unknowns are explicit when manifests are missing or contradictory.

## Focused Verification

Prefer scoped tests and one Cargo command at a time.

Rust examples:

```bash
cargo test --manifest-path client/src-tauri/Cargo.toml runtime_agent_crawl
cargo test --manifest-path client/src-tauri/Cargo.toml crawl_report
cargo test --manifest-path client/src-tauri/Cargo.toml agent_definition
```

Frontend examples:

```bash
pnpm --dir client test -- runtime.test.ts
pnpm --dir client test -- agent-runtime.test.tsx
pnpm --dir client test -- project.test.ts
```

Run formatting only where code changed. Do not open the Tauri app in a browser for verification.

## Acceptance Criteria

- Crawl is visible and selectable only for brownfield projects.
- Backend rejects Crawl for greenfield and unknown-origin projects.
- Crawl cannot mutate repository files or use broad engineering tools.
- Crawl produces a structured report with overview, stack, commands, tests, architecture, hot spots, constraints, unknowns, and freshness.
- The report is persisted into LanceDB-backed project records with stable schemas and fact keys.
- Recrawl supersedes stale Crawl records instead of duplicating them.
- Redaction blocks or sanitizes secret-like content before storage.
- Ask, Engineer, and Debug can retrieve Crawl records through existing project context retrieval.
- Focused Rust and TypeScript tests cover availability, tool policy, report parsing, persistence, and frontend filtering.

## Non-Goals

- Running every test suite by default.
- Measuring code coverage.
- Performing vulnerability or license audits.
- Producing a complete architecture document for humans.
- Creating a new LanceDB table in the first version.
- Maintaining compatibility with legacy `.xero/` state.
- Adding temporary debug UI.

## Open Questions

- Should Crawl be allowed to run a tiny command probe, such as manifest script listing, without prompting? Recommendation: only if the tool policy can classify the command as read-only and bounded.
- Should a successful import show a one-click "Crawl this repo" suggestion? Recommendation: yes, but only as durable product UI, not as a separate development scaffold.
- Should recrawl be manually triggered or suggested when records are stale? Recommendation: manual trigger first, stale suggestion later.
