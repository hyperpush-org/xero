# Dialog Refactor Inventory

Issue: [#14](https://github.com/hyperpush-org/xero/issues/14)

The shared dialog surface lives in `@xero/ui/components/base-dialog` and is exported from `@xero/ui`.

## Variants

- `confirmation`: safe confirmation flows with cancel-first footer order.
- `destructive-confirmation`: irreversible or high-risk flows with cancel-first footer order and destructive primary action styling.
- `alert` / `info`: informational dialogs with a single acknowledgement action.
- `form`: input-driven dialogs that submit or save user-entered data.
- `editor`: larger review/edit dialogs with scrollable custom body content.
- `custom`: dialogs that need fully specialized header/body composition while still sharing root/content/footer behavior.

## Client Dialog Groups

- Form/editor: new file, rename file, add project, start targets, developer tool, memory correction, project record supersede, workflow start, agent default model.
- Destructive confirmation: delete file, remove project, remove plugin, remove skill, wipe project data, wipe all data, delete project record, delete custom agent, close agent pane.
- Confirmation: replace start targets, enable Closed-Lid Mode, restore project state, approve agent save, unsaved changes, disk conflict.
- Custom content: settings full-screen dialog, create entity dialogs, handoff context summary, editor navigation, Git hunk review, agent edit preview.

## Cloud Consumption

The cloud PWA install-instructions dialogs consume the shared `BaseDialog` through `@xero/ui/components/base-dialog`, verifying the shared export path for non-client frontend apps.
