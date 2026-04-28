# Cadence Skills And Plugins

Cadence skills are model-visible instruction bundles. Plugins are manifest-validated source packages that can contribute skills and Cadence commands without becoming a second runtime.

## User Workflow

Users manage skills and plugins from Settings.

- Skills: inspect installed and discoverable skills, filter by source type, enable or disable sources, remove installed records, and view diagnostics.
- Skill sources: configure local skill roots, project skill discovery, and the GitHub skill source repository/ref/root.
- Plugins: configure plugin roots, reload plugin discovery, enable or disable installed plugins, remove stale plugin records, and inspect contributed skills and commands.

Local and plugin roots must be absolute directories. Cadence canonicalizes each root before saving it and rejects duplicate root ids or duplicate canonical paths.

Project skills live under `projects/<project-id>/skills` in the OS app-data directory. Dynamic skills are staged under `projects/<project-id>/dynamic-skills` and start disabled and untrusted.

## Agent Workflow

Owned agents use the `skill` tool when skill support is enabled for a run.

Supported operations:

- `list`: return ranked candidates across durable installed records plus configured bundled, local, project, GitHub, plugin, and MCP sources.
- `resolve`: select a specific candidate by `sourceId` or `skillId`.
- `install`: persist a source as an installed skill when policy allows it.
- `invoke`: return validated `SKILL.md` content, plus text supporting assets when requested.
- `reload`: refresh source discovery, mark changed or missing filesystem-backed records stale, and preserve diagnostics.
- `create_dynamic`: stage a disabled, untrusted project-scoped candidate from a completed run artifact or approved model output.

The model receives canonical `skill-source:v1` ids. Local absolute paths are not required for model execution and are redacted from model-facing diagnostics and candidate text.

## Trust States

- `trusted`: Cadence-controlled or already approved by policy. Agents may install, reload, and invoke it.
- `user_approved`: a user explicitly approved the source. Agents may install, reload, and invoke it.
- `approval_required`: agents may discover the source, but install/invoke/reload requires a user approval grant.
- `untrusted`: agents may discover the source, but it must be reviewed before use.
- `blocked`: the source fails closed. It is not model-visible and cannot be enabled.

Blocked skill records and blocked plugin records cannot be re-enabled through the registry. Disabled sources stay hidden from normal model discovery but remain visible when diagnostic flows request unavailable entries.

## Source Contract

Every skill source has a kind, scope, locator, source state, trust state, and canonical source id.

- `bundled`: global, Cadence-owned, discovered from app resources.
- `local`: global, discovered from configured absolute local roots.
- `project`: project-scoped, discovered from `projects/<project-id>/skills`.
- `github`: global or project-scoped, backed by the autonomous GitHub skill cache.
- `dynamic`: project-scoped, staged from run artifacts under `projects/<project-id>/dynamic-skills`.
- `mcp`: project-scoped, projected from approved connected MCP servers.
- `plugin`: project-scoped, contributed by a validated plugin manifest.

New source implementations must validate paths before discovery, never follow symlinks outside the declared root, emit typed diagnostics, and use existing `CadenceSkillSourceRecord` constructors so duplicate identities converge.

## Plugin Contract

Plugins use `cadence-plugin.json` or `.cadence-plugin/plugin.json`.

Required manifest fields:

- `schemaVersion`
- `id`
- `name`
- `version`
- `description`
- `trustDeclaration`

Contributed `skills`, `commands`, and `entryLocations` must point inside the plugin root. Unsupported manifest fields fail closed. Disabling a plugin disables its contributed skills and commands without deleting durable records. Removing or losing a plugin marks its contributions stale.

## Troubleshooting

- `skill_tool_user_approval_required`: approve the source in Settings or rerun with a valid approval grant.
- `skill_tool_source_not_enabled`: enable the installed skill or plugin before invoking it.
- `skill_source_content_changed`: reload found a changed `SKILL.md` or supporting asset; review and reinstall or invoke after approval.
- `skill_source_content_missing`: the installed filesystem location no longer contains `SKILL.md`.
- `skill_source_root_unavailable`: the local, project, bundled, or plugin source root is no longer configured or available.
- `cadence_plugin_root_unavailable`: the configured plugin root cannot be read.
- `cadence_plugin_manifest_invalid`: fix the manifest shape, remove unsupported fields, or restore required fields.
- `cadence_plugin_path_outside_root`: keep contributed plugin entries inside the plugin root.

When a diagnostic mentions a redacted path or secret in a model-facing response, use the Settings surface for full non-secret source metadata and fix the root or manifest from there.
