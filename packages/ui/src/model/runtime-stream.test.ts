import { describe, expect, it } from 'vitest'

import {
  createRuntimeStreamView,
  createRuntimeStreamViewFromSnapshot,
  mergeRuntimeStreamEvent,
  type RuntimeStreamMediaAttachmentDto,
  runtimeStreamItemSchema,
  runtimeStreamPatchSchema,
} from './runtime-stream'

function makeRuntimeImageAttachment(
  overrides: Partial<RuntimeStreamMediaAttachmentDto> = {},
): RuntimeStreamMediaAttachmentDto {
  return {
    id: 'media-browser-screenshot',
    kind: 'image',
    mediaType: 'image/png',
    title: 'Browser screenshot',
    alt: 'Screenshot from browser automation',
    sizeBytes: 67,
    width: 1,
    height: 1,
    source: {
      kind: 'app_data_path',
      absolutePath: '/Users/sn0w/Library/Application Support/Xero/tool-artifacts/conversation-media/media-browser-screenshot.png',
    },
    renderUrl: 'xero-project-asset://preview/media-browser-screenshot',
    ...overrides,
  }
}

describe('runtime stream contracts', () => {
  it('hydrates projected runtime stream snapshots from Rust patches', () => {
    const patch = runtimeStreamPatchSchema.parse({
      schema: 'xero.runtime_stream_patch.v1',
      item: {
        kind: 'tool',
        runId: 'run-projected-1',
        sequence: 4,
        sessionId: 'owned-agent:run-projected-1',
        toolCallId: 'call-read',
        toolName: 'read',
        toolState: 'succeeded',
        detail: 'Read App.',
        createdAt: '2026-05-06T12:03:03Z',
      },
      snapshot: {
        schema: 'xero.runtime_stream_view_snapshot.v1',
        projectId: 'project-1',
        agentSessionId: 'agent-session-1',
        runtimeKind: 'openai_codex',
        runId: 'run-projected-1',
        sessionId: 'owned-agent:run-projected-1',
        flowId: null,
        subscribedItemKinds: ['transcript', 'tool', 'activity'],
        status: 'live',
        items: [
          {
            kind: 'transcript',
            runId: 'run-projected-1',
            sequence: 1,
            updatedSequence: 2,
            sessionId: 'owned-agent:run-projected-1',
            text: 'Hello world',
            transcriptRole: 'assistant',
            createdAt: '2026-05-06T12:03:00Z',
          },
          {
            kind: 'tool',
            runId: 'run-projected-1',
            sequence: 3,
            updatedSequence: 4,
            sessionId: 'owned-agent:run-projected-1',
            toolCallId: 'call-read',
            toolName: 'read',
            toolState: 'succeeded',
            detail: 'Read App.',
            createdAt: '2026-05-06T12:03:02Z',
          },
        ],
        transcriptItems: [
          {
            kind: 'transcript',
            runId: 'run-projected-1',
            sequence: 1,
            updatedSequence: 2,
            sessionId: 'owned-agent:run-projected-1',
            text: 'Hello world',
            transcriptRole: 'assistant',
            createdAt: '2026-05-06T12:03:00Z',
          },
        ],
        toolCalls: [
          {
            kind: 'tool',
            runId: 'run-projected-1',
            sequence: 4,
            sessionId: 'owned-agent:run-projected-1',
            toolCallId: 'call-read',
            toolName: 'read',
            toolState: 'succeeded',
            detail: 'Read App.',
            createdAt: '2026-05-06T12:03:03Z',
          },
        ],
        skillItems: [],
        activityItems: [],
        actionRequired: [],
        plan: null,
        completion: null,
        failure: null,
        lastIssue: null,
        lastItemAt: '2026-05-06T12:03:03Z',
        lastSequence: 4,
      },
    })

    const stream = createRuntimeStreamViewFromSnapshot(patch.snapshot)

    expect(stream.status).toBe('live')
    expect(stream.transcriptItems[0]).toMatchObject({
      sequence: 1,
      updatedSequence: 2,
      text: 'Hello world',
    })
    expect(stream.items[1]).toMatchObject({
      kind: 'tool',
      sequence: 3,
      updatedSequence: 4,
      toolCallId: 'call-read',
    })
    expect(stream.toolCalls[0]).toMatchObject({
      sequence: 4,
      toolState: 'succeeded',
    })
  })

  it('accepts planning input shapes and phase-aware plan items', () => {
    const actionItem = runtimeStreamItemSchema.parse({
      kind: 'action_required',
      runId: 'run-plan-1',
      sequence: 1,
      actionId: 'plan-question-1',
      actionType: 'short_text_required',
      answerShape: 'short_text',
      title: 'Name the plan',
      detail: 'Choose a concise title.',
      createdAt: '2026-05-06T12:00:00Z',
    })
    const planItem = runtimeStreamItemSchema.parse({
      kind: 'plan',
      runId: 'run-plan-1',
      sequence: 2,
      planId: 'plan-pack-1',
      planItems: [
        {
          id: 'P0-S1',
          title: 'Contract and naming',
          notes: 'First implementation slice.',
          status: 'in_progress',
          updatedAt: '2026-05-06T12:01:00Z',
          phaseId: 'P0',
          phaseTitle: 'Foundation',
          sliceId: 'P0-S1',
          handoffNote: 'Start with runtime descriptors.',
        },
      ],
      createdAt: '2026-05-06T12:01:00Z',
    })

    expect(actionItem.answerShape).toBe('short_text')
    expect(planItem.planItems?.[0]?.phaseTitle).toBe('Foundation')
  })

  it('preserves phase-aware plan item metadata in the runtime stream view', () => {
    const base = createRuntimeStreamView({
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-plan-1',
      sessionId: 'owned-agent:run-plan-1',
      subscribedItemKinds: ['plan'],
    })

    const stream = mergeRuntimeStreamEvent(base, {
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-plan-1',
      sessionId: 'owned-agent:run-plan-1',
      flowId: null,
      subscribedItemKinds: ['plan'],
      item: {
        kind: 'plan',
        runId: 'run-plan-1',
        sequence: 1,
        planId: 'plan-pack-1',
        planItems: [
          {
            id: 'P0-S1',
            title: 'Contract and naming',
            notes: 'First implementation slice.',
            status: 'in_progress',
            updatedAt: '2026-05-06T12:01:00Z',
            phaseId: 'P0',
            phaseTitle: 'Foundation',
            sliceId: 'P0-S1',
            handoffNote: 'Start with runtime descriptors.',
          },
        ],
        createdAt: '2026-05-06T12:01:00Z',
      },
    })

    expect(stream.plan?.items[0]).toMatchObject({
      phaseId: 'P0',
      phaseTitle: 'Foundation',
      sliceId: 'P0-S1',
      handoffNote: 'Start with runtime descriptors.',
    })
  })

  it('keeps a completed stream complete when replay includes post-run bookkeeping activity', () => {
    let stream = createRuntimeStreamView({
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-complete-1',
      sessionId: 'owned-agent:run-complete-1',
      subscribedItemKinds: ['transcript', 'activity', 'complete'],
    })

    stream = mergeRuntimeStreamEvent(stream, {
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-complete-1',
      sessionId: 'owned-agent:run-complete-1',
      flowId: null,
      subscribedItemKinds: ['transcript', 'activity', 'complete'],
      item: {
        kind: 'transcript',
        runId: 'run-complete-1',
        sequence: 1,
        sessionId: 'owned-agent:run-complete-1',
        text: 'What is this project?',
        transcriptRole: 'user',
        createdAt: '2026-05-19T10:00:00Z',
      },
    })
    stream = mergeRuntimeStreamEvent(stream, {
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-complete-1',
      sessionId: 'owned-agent:run-complete-1',
      flowId: null,
      subscribedItemKinds: ['transcript', 'activity', 'complete'],
      item: {
        kind: 'complete',
        runId: 'run-complete-1',
        sequence: 2,
        sessionId: 'owned-agent:run-complete-1',
        detail: 'Owned agent run completed.',
        createdAt: '2026-05-19T10:00:01Z',
      },
    })
    stream = mergeRuntimeStreamEvent(stream, {
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-complete-1',
      sessionId: 'owned-agent:run-complete-1',
      flowId: null,
      subscribedItemKinds: ['transcript', 'activity', 'complete'],
      item: {
        kind: 'activity',
        runId: 'run-complete-1',
        sequence: 3,
        sessionId: 'owned-agent:run-complete-1',
        code: 'owned_agent_validation_completed',
        title: 'Validation completed',
        detail: 'Validation completed: memory_extraction.',
        createdAt: '2026-05-19T10:00:02Z',
      },
    })

    expect(stream.status).toBe('complete')
    expect(stream.completion?.sequence).toBe(2)
  })

  it('reopens a completed stream when a new user transcript starts the next turn', () => {
    let stream = createRuntimeStreamView({
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-complete-2',
      sessionId: 'owned-agent:run-complete-2',
      subscribedItemKinds: ['transcript', 'complete'],
    })

    stream = mergeRuntimeStreamEvent(stream, {
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-complete-2',
      sessionId: 'owned-agent:run-complete-2',
      flowId: null,
      subscribedItemKinds: ['transcript', 'complete'],
      item: {
        kind: 'complete',
        runId: 'run-complete-2',
        sequence: 1,
        sessionId: 'owned-agent:run-complete-2',
        detail: 'Owned agent run completed.',
        createdAt: '2026-05-19T10:01:00Z',
      },
    })
    stream = mergeRuntimeStreamEvent(stream, {
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-complete-2',
      sessionId: 'owned-agent:run-complete-2',
      flowId: null,
      subscribedItemKinds: ['transcript', 'complete'],
      item: {
        kind: 'transcript',
        runId: 'run-complete-2',
        sequence: 2,
        sessionId: 'owned-agent:run-complete-2',
        text: 'What changed next?',
        transcriptRole: 'user',
        createdAt: '2026-05-19T10:01:01Z',
      },
    })

    expect(stream.status).toBe('live')
    expect(stream.completion).toBeNull()
  })

  it('parses code history metadata and keeps it available on stream items', () => {
    const item = runtimeStreamItemSchema.parse({
      kind: 'activity',
      runId: 'run-code-1',
      sequence: 1,
      code: 'owned_agent_file_changed',
      title: 'File changed',
      detail: 'write: src/app.ts',
      codeChangeGroupId: 'code-change-1',
      codeCommitId: 'code-commit-1',
      codeWorkspaceEpoch: 7,
      codePatchAvailability: {
        projectId: 'project-1',
        targetChangeGroupId: 'code-change-1',
        available: true,
        affectedPaths: ['src/app.ts'],
        fileChangeCount: 1,
        textHunkCount: 2,
        textHunks: [
          {
            hunkId: 'hunk-1',
            patchFileId: 'patch-file-1',
            filePath: 'src/app.ts',
            hunkIndex: 0,
            baseStartLine: 4,
            baseLineCount: 1,
            resultStartLine: 4,
            resultLineCount: 2,
          },
        ],
        unavailableReason: null,
      },
      createdAt: '2026-05-06T12:02:00Z',
    })
    expect(item.codeCommitId).toBe('code-commit-1')
    expect(item.codePatchAvailability?.textHunks[0]?.hunkId).toBe('hunk-1')

    const stream = mergeRuntimeStreamEvent(null, {
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-code-1',
      sessionId: 'owned-agent:run-code-1',
      flowId: null,
      subscribedItemKinds: ['activity'],
      item,
    })

    expect(stream.activityItems[0]).toMatchObject({
      codeChangeGroupId: 'code-change-1',
      codeCommitId: 'code-commit-1',
      codeWorkspaceEpoch: 7,
      codePatchAvailability: {
        available: true,
        affectedPaths: ['src/app.ts'],
      },
    })
  })

  it('accepts image media attachments on runtime tool items', () => {
    const item = runtimeStreamItemSchema.parse({
      kind: 'tool',
      runId: 'run-media-1',
      sequence: 3,
      toolCallId: 'call-browser-screenshot',
      toolName: 'browser',
      toolState: 'succeeded',
      detail: 'Browser screenshot captured.',
      mediaAttachments: [makeRuntimeImageAttachment()],
      createdAt: '2026-05-06T12:02:00Z',
    })

    expect(item.mediaAttachments?.[0]).toMatchObject({
      id: 'media-browser-screenshot',
      kind: 'image',
      mediaType: 'image/png',
      source: {
        kind: 'app_data_path',
      },
    })
  })

  it('rejects unsupported image data URL media sources', () => {
    const parsed = runtimeStreamItemSchema.safeParse({
      kind: 'transcript',
      runId: 'run-media-2',
      sequence: 1,
      transcriptRole: 'assistant',
      text: null,
      mediaAttachments: [
        makeRuntimeImageAttachment({
          id: 'media-svg',
          source: {
            kind: 'data_url',
            dataUrl: 'data:image/svg+xml;base64,PHN2Zy8+',
          },
        }),
      ],
      createdAt: '2026-05-06T12:02:00Z',
    })

    expect(parsed.success).toBe(false)
    if (parsed.success) {
      throw new Error('Unsupported image data URL unexpectedly parsed.')
    }
    expect(parsed.error.issues.some((issue) => issue.path.join('.') === 'mediaAttachments.0.source.dataUrl')).toBe(true)
  })

  it('normalizes media-only assistant transcript items and merges later deltas', () => {
    let stream = createRuntimeStreamView({
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-media-3',
      sessionId: 'owned-agent:run-media-3',
      subscribedItemKinds: ['transcript'],
    })

    stream = mergeRuntimeStreamEvent(stream, {
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-media-3',
      sessionId: 'owned-agent:run-media-3',
      flowId: null,
      subscribedItemKinds: ['transcript'],
      item: {
        kind: 'transcript',
        runId: 'run-media-3',
        sequence: 1,
        sessionId: 'owned-agent:run-media-3',
        text: null,
        transcriptRole: 'assistant',
        mediaAttachments: [makeRuntimeImageAttachment()],
        createdAt: '2026-05-06T12:03:00Z',
      },
    })
    stream = mergeRuntimeStreamEvent(stream, {
      projectId: 'project-1',
      agentSessionId: 'agent-session-1',
      runtimeKind: 'openai_codex',
      runId: 'run-media-3',
      sessionId: 'owned-agent:run-media-3',
      flowId: null,
      subscribedItemKinds: ['transcript'],
      item: {
        kind: 'transcript',
        runId: 'run-media-3',
        sequence: 2,
        sessionId: 'owned-agent:run-media-3',
        text: 'Screenshot captured.',
        transcriptRole: 'assistant',
        mediaAttachments: [
          makeRuntimeImageAttachment({
            id: 'media-browser-screenshot-2',
            title: 'Second screenshot',
          }),
        ],
        createdAt: '2026-05-06T12:03:01Z',
      },
    })

    expect(stream.transcriptItems).toHaveLength(1)
    expect(stream.transcriptItems[0]).toMatchObject({
      sequence: 1,
      updatedSequence: 2,
      text: 'Screenshot captured.',
    })
    expect(stream.transcriptItems[0].mediaAttachments.map((attachment) => attachment.id)).toEqual([
      'media-browser-screenshot',
      'media-browser-screenshot-2',
    ])
  })
})
