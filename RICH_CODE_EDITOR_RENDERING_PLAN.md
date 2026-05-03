# Rich Code Editor Rendering Plan

## Reader and Outcome

Reader: an internal Xero engineer implementing the editor rendering work.

Post-read action: update the Xero editor so opening common previewable project files renders them in-app, close to the way VS Code handles source plus previews, while preserving CodeMirror editing for text files.

This plan covers SVG, raster images, Markdown, PDF, audio/video, CSV, and unsupported binary fallbacks. It does not add temporary debug UI. Any visible UI described here is user-facing.

## Current State

- The editor path is text-only. Project files are read as UTF-8 strings and rejected when they are not valid text.
- The frontend schema expects a single string content field, so the React editor cannot distinguish text, binary, previewable assets, and unsupported files.
- The main editor view always mounts CodeMirror for files.
- File tree nodes know only file vs folder. Language detection maps some previewable formats, such as SVG, as source languages.
- Tauri already has app-local URI schemes for other large payloads, so there is an existing pattern for serving bytes without pushing them through IPC.

## Product Rules

| File family | Default behavior | Source behavior |
| --- | --- | --- |
| Text/code | Open in CodeMirror | Editable |
| SVG | Open rendered preview | Toggle to XML source, editable |
| Raster images | Open rendered preview | No source editor in v1 |
| Markdown | Open source with preview toggle | Editable; preview can be side-by-side later |
| PDF | Open preview when the WebView supports it | No source editor |
| Audio/video | Open media preview with native controls | No source editor |
| CSV/TSV | Open source with table preview toggle | Editable as text |
| HTML | Open source by default | Preview is opt-in, sandboxed, and scriptless |
| Unknown binary | Show metadata and safe actions | Not editable |

Preview state is per open tab. If a text-backed preview has unsaved edits, the preview should render the current unsaved snapshot rather than forcing a save first.

## Architecture

### 1. Content Classification

Replace the current text-only file read contract with a discriminated project file content response. This app is new, so do not preserve the old response shape with compatibility adapters.

The response should include:

- `kind`: `text`, `renderable`, or `unsupported`
- `path`, `byteLength`, `modifiedAt`, and a content hash or ETag
- `mimeType` and a normalized renderer kind when detected
- `text` only for files classified as text and under the configured text-size limit
- `previewUrl` only for renderable files served through the app-local project asset protocol
- `reason` for unsupported files, such as binary, too large for text editing, directory, or unknown type

Classification should combine extension-based MIME lookup with light byte sniffing. Add direct Rust dependencies where needed rather than relying on transitive lockfile entries. Keep the existing project-root, symlink, and virtual-path protections.

### 2. Project Asset URI Scheme

Add a project-scoped asset URI scheme for preview bytes. The frontend should receive short-lived preview URLs instead of base64 file contents for binary and large assets.

Rules:

- Resolve every request through the selected project root and the existing virtual path rules.
- Deny symlinks, directory reads, root escapes, and paths outside the selected project.
- Serve accurate `Content-Type`, `Content-Length`, `ETag`, and `Cache-Control` headers.
- Support HTTP range requests for PDF, video, and audio.
- Prefer capability tokens over raw path-only URLs so arbitrary web content cannot guess project files.
- Revoke or expire tokens when a tab closes, a project switches, or the underlying file changes.

### 3. Frontend Content Model

Update the TypeScript schemas to mirror the Rust discriminated response. The workspace controller should cache content by path and document version, but dirty state should only apply to text-backed source editors.

Suggested model:

```ts
type ProjectFileContent =
  | {
      kind: 'text'
      path: string
      text: string
      byteLength: number
      mimeType: string
      rendererKind: 'code' | 'svg' | 'markdown' | 'csv' | 'html'
    }
  | {
      kind: 'renderable'
      path: string
      previewUrl: string
      byteLength: number
      mimeType: string
      rendererKind: 'image' | 'pdf' | 'audio' | 'video'
    }
  | {
      kind: 'unsupported'
      path: string
      byteLength: number
      mimeType: string | null
      reason: string
    }
```

### 4. Renderer Registry

Introduce a small renderer registry in the editor surface:

- Code editor: current CodeMirror editor
- Image preview: `img` with zoom, fit, actual-size, transparent-background checkerboard, dimensions, file size
- SVG preview: `img` backed by either the project asset URL or an unsaved Blob URL
- Markdown preview: CommonMark/GFM renderer with sanitization and relative asset URL resolution
- PDF preview: `iframe` or `object` with fallback copy/open actions when the platform cannot render it
- Media preview: native audio/video controls with range-backed URLs
- CSV preview: virtualized table preview for large files, source toggle for editing
- Unsupported preview: metadata, copy path, and open external action

Use ShadCN components for toolbar buttons, toggle groups, dropdowns, tooltips, dialogs, separators, scroll areas, and skeleton/loading states.

## Security Rules

- Never inline untrusted SVG. Render SVG through `img` so scripts do not execute.
- Never use unsanitized `dangerouslySetInnerHTML` for Markdown or HTML previews.
- HTML preview is sandboxed, scriptless, and source-first.
- Resolve Markdown image/link references through the same project asset path rules.
- Revoke Blob URLs on preview unmount and source changes.
- Keep previews inside the editor surface; do not open the Tauri app in a browser for verification.

## Phase 1 - Backend Content Pipeline

Goal: make the backend able to classify and serve previewable files safely.

Tasks:

1. Replace the text-only project file response with a discriminated content response.
2. Add MIME/type detection for text, SVG, image, Markdown, PDF, audio, video, CSV, HTML, and unknown binary.
3. Add text-size and preview-size limits with clear unsupported reasons.
4. Add the project asset URI scheme with token validation, content headers, and range support.
5. Update command exports and frontend command schemas to the new contract.
6. Add Rust tests for UTF-8 text, non-UTF-8 binary, SVG, raster image, symlink denial, path traversal denial, large file limits, and range responses.

Done when:

- Opening a binary project file no longer errors as "not UTF-8"; it returns a classified response.
- Preview URLs serve bytes without IPC payload growth.
- Existing path safety behavior remains intact.

## Phase 2 - Editor Surface Split

Goal: separate "file open" from "CodeMirror source editor" so the active tab can render the correct surface.

Tasks:

1. Introduce a file editor host that receives the classified content response and chooses a renderer.
2. Keep the existing CodeMirror component as the source editor for text content.
3. Add a ShadCN toolbar control for source/preview mode when both modes exist.
4. Preserve save, revert, dirty tracking, cursor stats, line counts, find/replace, and tab behavior for source mode.
5. Show metadata instead of cursor stats for non-text previews.
6. Add Vitest coverage for renderer selection, tab mode state, dirty state isolation, save/revert behavior, and unsupported binary fallback.

Done when:

- Text editing works exactly through the new host.
- Image/SVG/Markdown files route to a preview-capable surface without losing source editing where source exists.

## Phase 3 - Core Renderers

Goal: ship the first user-visible preview set.

Tasks:

1. Raster image preview with fit, zoom, actual-size, background toggle, dimensions, and file-size display.
2. SVG preview using safe image rendering plus source toggle.
3. Markdown preview with sanitized GFM rendering, code highlighting, table support, relative image support, and source toggle.
4. CSV/TSV table preview with a large-file row/column budget and source toggle.
5. Empty/loading/error states using existing Xero styling and ShadCN primitives.
6. Accessibility pass for keyboard focus, labels, alt text fallback, and toolbar control names.

Done when:

- SVG, PNG/JPEG/WebP/GIF, Markdown with relative images, and CSV can be opened and rendered from the editor tab.
- Editing SVG/Markdown/CSV source and switching back to preview reflects unsaved edits.

## Phase 4 - Large and Platform-Sensitive Renderers

Goal: add common heavy previews without destabilizing the editor.

Tasks:

1. PDF preview with platform fallback when the WebView cannot render inline PDF.
2. Audio/video previews with native controls and range-backed streaming.
3. File metadata panel for unsupported binary files.
4. Cache invalidation when the file changes on disk, the project switches, or the tab closes.
5. Performance tests around large images, long Markdown documents, large CSVs, and media range requests.

Done when:

- Large previews do not exceed IPC budgets.
- Unsupported files fail gracefully and explain why they cannot render.

## Phase 5 - Hardening and Verification

Goal: make the feature robust enough to become the default editor behavior.

Tasks:

1. Add focused frontend tests for every renderer.
2. Add scoped Rust tests for the project file command and asset protocol. Run only one Cargo command at a time.
3. Add Tauri-level or command-contract tests for the full classify -> URL -> bytes flow.
4. Verify macOS WebView behavior for PDF, video, SVG, and Blob URL cleanup.
5. Review CSP and allowed custom schemes for the new asset protocol.
6. Run scoped formatting and tests for touched Rust and frontend files.

Done when:

- The editor can open common text, rendered, and unsupported files without crashing or corrupting content.
- Security tests cover path traversal, symlink denial, unsafe SVG, unsafe Markdown HTML, and expired preview tokens.
- No temporary UI was added.

## Acceptance Checklist

- SVG files render visually and remain editable as XML source.
- Raster images render without attempting to decode them through UTF-8 text.
- Markdown preview supports relative images and sanitized links.
- CSV preview handles large files without freezing the editor.
- PDF and media use URL/range serving rather than base64 IPC.
- Unknown binary files show a user-facing unsupported state.
- Existing save/revert/tab/find behavior still works for source files.
- All new controls use ShadCN where possible.
- State that must persist is stored under OS app-data, not `.xero/`.

## Out of Scope for v1

- VS Code extension compatibility.
- Rich WYSIWYG editing for Markdown, SVG, HTML, or images.
- Executing arbitrary project HTML or JavaScript.
- Hex editing binary files.
- Browser-based manual verification.
