# Conversation Image Output Implementation Plan

## Goal

Conversations in both the desktop client and cloud app should display images returned by agent tools. Agents with access to image-producing tools should be able to show those images to the user as first-class conversation content, not as raw JSON, base64 text, or opaque artifact paths.

This plan covers tool-result images produced by agents. User-uploaded image attachments are partially supported already and should be reused where possible.

## Current State

### Already Supported

- User image attachments can be staged under OS app-data project state.
- User attachments are stored in `agent_message_attachments`.
- The shared transcript model has `ConversationMessageAttachment`.
- User message rows render image attachments when `previewSrc` is present.
- Anthropic provider serialization supports user image/document/text attachments.

### Missing

- Assistant message rendering ignores attachments in the default transcript row.
- Tool output is text-only in the runtime stream contract through `toolResultPreview`.
- Rust runtime stream DTOs expose `tool_result_preview: Option<String>` but no structured media field.
- Cloud relay projection maps transcript and tool events into text-only `ConversationTurn`s.
- Historical session transcript DTOs do not carry media attachments or tool artifacts.
- Image-producing tools already exist, but their image outputs are not surfaced as renderable media:
  - browser screenshots return base64 PNG strings
  - macOS screenshots write PNG artifacts
  - image file reads can include `previewBase64`
  - MCP tools may return image/blob content or large result artifacts

## Design Principles

- Keep image output as typed media metadata, not markdown string tricks.
- Store generated images under OS app-data, not `.xero/`.
- Do not add backwards-compatibility glue. This is a new app surface.
- Keep raw base64 out of long-lived transcript text and IPC-heavy string previews.
- Make desktop and cloud consume the same shared UI shape.
- Treat MCP and web-originated image content as untrusted data with explicit media metadata.

## Proposed Data Model

Add a shared media attachment contract for runtime stream items:

```ts
type RuntimeStreamMediaAttachment = {
  id: string
  kind: "image"
  mediaType: "image/png" | "image/jpeg" | "image/gif" | "image/webp"
  title?: string | null
  alt?: string | null
  sizeBytes?: number | null
  width?: number | null
  height?: number | null
  source:
    | { kind: "app_data_path"; absolutePath: string }
    | { kind: "artifact"; artifactId: string; absolutePath?: string | null }
    | { kind: "data_url"; dataUrl: string }
    | { kind: "remote_artifact"; artifactId: string; computerId: string; sessionId: string }
}
```

Add optional arrays to runtime stream item DTOs:

```ts
mediaAttachments?: RuntimeStreamMediaAttachment[]
```

Use the same shape on:

- tool stream items
- assistant transcript items, if future provider output includes image parts
- session transcript items for history/reload
- cloud relay payloads

## Implementation Phases

### Phase 1: Shared Contracts

1. Add Zod schemas and TypeScript types in `packages/ui/src/model/runtime-stream.ts`.
2. Add Rust DTO structs in `client/src-tauri/src/commands/contracts/runtime.rs`.
3. Add media fields to `RuntimeStreamViewItem` and byte-budget estimation.
4. Add media fields to `SessionTranscriptItemDto` in Rust and TypeScript.
5. Add schema tests for valid image media and malformed payload rejection.

### Phase 2: Runtime Extraction

Extract image media before falling back to `tool_result_preview`.

Initial extractors:

1. Browser screenshot output:
   - detect browser screenshot base64 PNG output
   - write it to an app-data tool artifact
   - emit a media attachment with `mediaType: "image/png"`

2. macOS screenshot output:
   - reuse the existing screenshot artifact path
   - emit a media attachment referencing the artifact path

3. Image file read output:
   - detect `contentKind: "image"` and `previewBase64`
   - write preview bytes to an app-data artifact or emit a bounded data URL
   - preserve metadata such as width, height, sha256, and media type

4. MCP output:
   - detect structured MCP image/blob content
   - normalize supported image media types
   - write bytes to app-data tool artifacts
   - keep `xeroBoundary` and untrusted-source treatment

### Phase 3: Desktop Render URLs

1. Add a small resolver that converts allowed app-data artifact paths to WebView-renderable URLs.
2. Restrict resolution to known project app-data roots and supported image media types.
3. Reuse existing Tauri asset serving patterns where possible.
4. Avoid exposing arbitrary local paths.

### Phase 4: Shared Transcript UI

1. Extend `ConversationTurn` attachments/media to support assistant and tool rows.
2. Render assistant message images in `AssistantMessage`.
3. Render tool images in expanded tool detail rows.
4. Keep text output copy affordances for text rows.
5. Use ShadCN dialog primitives for full-size image preview.
6. Keep dense mode compact: show filename/thumbnail metadata, not a large preview.

### Phase 5: Cloud Relay

1. Extend remote runtime event envelopes to include media attachments.
2. Add a remote artifact fetch path for cloud:
   - cloud receives metadata
   - desktop serves bytes on demand through the relay
   - cloud creates object URLs or signed transient URLs
3. Ensure remote images respect size limits and media-type validation.
4. Update `cloud/src/lib/relay/stream-projection.ts` to preserve media attachments.

### Phase 6: Session History

1. Persist tool media references in app-data session state.
2. Include media attachments in `get_session_transcript` responses.
3. Update historical projection so reloaded conversations show prior images.
4. Keep export behavior explicit:
   - markdown export can link to artifact metadata
   - JSON export includes structured media metadata

## Test Plan

### Rust

- Browser screenshot output emits a media attachment and no giant base64 preview.
- macOS screenshot output emits artifact-backed media.
- image read output emits media metadata from `previewBase64`.
- MCP image/blob output emits app-data artifact-backed media.
- unsupported media types are ignored or surfaced as text metadata.
- IPC compaction keeps media metadata while bounding text previews.

### TypeScript Model

- runtime stream schema accepts valid media attachments.
- runtime stream schema rejects invalid paths, media types, or empty ids.
- byte-budget estimation accounts for media metadata.
- session transcript schema carries media attachments.

### Shared UI

- assistant message images render with alt text.
- tool image outputs render in expanded tool rows.
- text-only tool outputs still render as `<pre>`.
- dense mode does not layout-shift or overflow.
- full-size preview dialog opens and closes.

### Cloud

- cloud projection preserves runtime media attachments.
- remote artifact metadata renders as an image once resolved.
- missing remote artifacts render a recoverable unavailable state.

## Acceptance Criteria

- An agent tool can return an image and the desktop conversation displays it inline.
- The same image appears in cloud conversations through the relay.
- Tool image output is not exposed as raw base64 or opaque JSON in the main transcript.
- Historical session reloads preserve image visibility.
- Existing user attachment rendering continues to work.
- Tests cover desktop stream projection, shared UI rendering, cloud projection, and Rust extraction.

## Open Decisions

- Whether desktop should always write image outputs to app-data artifacts, or allow bounded `data_url` payloads for tiny images.
- Whether remote cloud artifacts should be streamed through the relay every time or cached per session.
- Whether non-image binary outputs should share the same attachment contract later.
- Whether markdown `![alt](...)` rendering should be supported separately from structured agent/tool media.
