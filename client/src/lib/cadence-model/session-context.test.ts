import { describe, expect, it } from 'vitest'
import {
  CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
  compactSessionHistoryRequestSchema,
  compactSessionHistoryResponseSchema,
  createContextBudget,
  createPublicSessionContextRedaction,
  createRedactedSessionContextText,
  deleteSessionMemoryRequestSchema,
  exportSessionTranscriptRequestSchema,
  extractSessionMemoryCandidatesRequestSchema,
  extractSessionMemoryCandidatesResponseSchema,
  getSessionContextSnapshotRequestSchema,
  getSessionTranscriptRequestSchema,
  listSessionMemoriesRequestSchema,
  listSessionMemoriesResponseSchema,
  runTranscriptSchema,
  saveSessionTranscriptExportRequestSchema,
  searchSessionTranscriptsRequestSchema,
  searchSessionTranscriptsResponseSchema,
  sessionCompactionRecordSchema,
  sessionContextContributorSchema,
  sessionContextPolicyDecisionSchema,
  sessionContextSnapshotSchema,
  sessionTranscriptExportResponseSchema,
  sessionMemoryDiagnosticSchema,
  sessionMemoryRecordSchema,
  sessionTranscriptExportPayloadSchema,
  sessionTranscriptSearchResultSnippetSchema,
  sessionTranscriptSchema,
  updateSessionMemoryRequestSchema,
  type RunTranscriptDto,
  type SessionContextContributorDto,
  type SessionContextPolicyDecisionDto,
} from './session-context'

const projectId = 'project-context'
const agentSessionId = 'agent-session-context'
const runId = 'run-context'
const providerId = 'openrouter'
const modelId = 'openai/gpt-5.4'
const createdAt = '2026-04-26T10:00:00Z'

describe('session context contract', () => {
  it('accepts a strict run transcript and rejects malformed or unordered payloads', () => {
    const transcript = runTranscriptSchema.parse(makeRunTranscript())

    expect(transcript.items).toHaveLength(2)
    expect(transcript.items[0]).toMatchObject({
      itemId: 'message:1',
      actor: 'user',
      kind: 'message',
    })

    expect(() =>
      runTranscriptSchema.parse({
        ...transcript,
        items: [
          transcript.items[0],
          {
            ...transcript.items[1],
            sequence: transcript.items[0].sequence,
          },
        ],
      }),
    ).toThrow(/strictly increasing/)

    expect(() =>
      runTranscriptSchema.parse({
        ...transcript,
        items: [
          {
            ...transcript.items[0],
            unexpected: true,
          },
        ],
      }),
    ).toThrow()
  })

  it('accepts archived empty sessions and export/search-adjacent shapes', () => {
    const transcript = sessionTranscriptSchema.parse({
      contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
      projectId,
      agentSessionId,
      title: 'Archived context',
      summary: '',
      status: 'archived',
      archived: true,
      archivedAt: '2026-04-26T10:05:00Z',
      runs: [],
      items: [],
      usageTotals: null,
      redaction: createPublicSessionContextRedaction(),
    })

    expect(transcript.archived).toBe(true)
    expect(
      sessionTranscriptExportPayloadSchema.parse({
        contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
        exportId: 'export-1',
        generatedAt: '2026-04-26T10:06:00Z',
        scope: 'session',
        format: 'json',
        transcript,
        contextSnapshot: null,
        redaction: createPublicSessionContextRedaction(),
      }).transcript.items,
    ).toEqual([])

    expect(() =>
      sessionTranscriptSchema.parse({
        ...transcript,
        archivedAt: null,
      }),
    ).toThrow(/archivedAt/)
  })

  it('validates context snapshots, contributors, and compaction policy invariants', () => {
    const contributor = makeContributor('memory:project-decision', 1)
    const instruction = makeContributor('instruction:AGENTS.md', 2, {
      kind: 'instruction_file',
      text: 'Use tests rather than temporary UI.',
    })
    const policy: SessionContextPolicyDecisionDto = sessionContextPolicyDecisionSchema.parse({
      contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
      decisionId: 'compaction:auto:ready',
      kind: 'compaction',
      action: 'compact_now',
      trigger: 'auto',
      reasonCode: 'auto_compact_threshold_reached',
      message: 'Auto-compact should run before the next provider turn.',
      rawTranscriptPreserved: true,
      modelVisible: false,
      redaction: createPublicSessionContextRedaction(),
    })

    const snapshot = sessionContextSnapshotSchema.parse({
      contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
      snapshotId: 'context-snapshot-1',
      projectId,
      agentSessionId,
      runId,
      providerId,
      modelId,
      generatedAt: '2026-04-26T10:10:00Z',
      budget: createContextBudget(800, 1000),
      contributors: [contributor, instruction],
      policyDecisions: [policy],
      usageTotals: null,
      redaction: createPublicSessionContextRedaction(),
    })

    expect(snapshot.budget.pressure).toBe('high')
    expect(snapshot.budget.estimationSource).toBe('estimated')
    expect(() =>
      sessionContextSnapshotSchema.parse({
        ...snapshot,
        contributors: [instruction, contributor],
      }),
    ).toThrow(/strictly increasing/)

    expect(() =>
      sessionContextPolicyDecisionSchema.parse({
        ...policy,
        rawTranscriptPreserved: false,
      }),
    ).toThrow(/preserve raw transcript/)

    expect(() =>
      sessionContextContributorSchema.parse({
        ...contributor,
        included: false,
      }),
    ).toThrow(/Model-visible/)
  })

  it('keeps approved memory schema explicit and redacts secret-bearing text helpers', () => {
    const redacted = createRedactedSessionContextText('Use api_key=sk-context-secret')
    expect(redacted.value).toBe('Cadence redacted sensitive session-context text.')
    expect(redacted.redaction).toMatchObject({
      redactionClass: 'secret',
      redacted: true,
    })

    const memory = sessionMemoryRecordSchema.parse({
      contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
      memoryId: 'memory-1',
      projectId,
      agentSessionId: null,
      scope: 'project',
      kind: 'decision',
      text: redacted.value,
      reviewState: 'approved',
      enabled: true,
      confidence: 95,
      sourceRunId: runId,
      sourceItemIds: ['message:1'],
      createdAt,
      updatedAt: createdAt,
      diagnostic: null,
      redaction: redacted.redaction,
    })
    const diagnostic = sessionMemoryDiagnosticSchema.parse({
      code: 'memory_source_deleted',
      message: 'The source run was deleted.',
      redaction: createPublicSessionContextRedaction(),
    })
    const candidate = sessionMemoryRecordSchema.parse({
      contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
      memoryId: 'memory-candidate',
      projectId,
      agentSessionId,
      scope: 'session',
      kind: 'session_summary',
      text: 'The session established the reviewed memory workflow.',
      reviewState: 'candidate',
      enabled: false,
      confidence: 72,
      sourceRunId: runId,
      sourceItemIds: ['message:1'],
      createdAt,
      updatedAt: createdAt,
      diagnostic,
      redaction: createPublicSessionContextRedaction(),
    })

    const serialized = JSON.stringify(memory)
    expect(serialized).not.toContain('sk-context-secret')
    expect(memory.reviewState).toBe('approved')
    expect(candidate.diagnostic?.code).toBe('memory_source_deleted')
    expect(() => sessionMemoryRecordSchema.parse({ ...candidate, enabled: true })).toThrow(/Only approved/)
    expect(() => sessionMemoryRecordSchema.parse({ ...memory, agentSessionId, scope: 'project' })).toThrow(
      /Project memory/,
    )
    expect(
      listSessionMemoriesRequestSchema.parse({
        projectId,
        agentSessionId,
        includeDisabled: true,
        includeRejected: false,
      }),
    ).toEqual({ projectId, agentSessionId, includeDisabled: true, includeRejected: false })
    expect(
      listSessionMemoriesResponseSchema.parse({
        projectId,
        agentSessionId,
        memories: [memory, candidate],
      }).memories,
    ).toHaveLength(2)
    expect(
      extractSessionMemoryCandidatesRequestSchema.parse({
        projectId,
        agentSessionId,
        runId,
      }),
    ).toEqual({ projectId, agentSessionId, runId })
    expect(
      extractSessionMemoryCandidatesResponseSchema.parse({
        projectId,
        agentSessionId,
        memories: [memory, candidate],
        createdCount: 2,
        skippedDuplicateCount: 1,
        rejectedCount: 1,
        diagnostics: [diagnostic],
      }).skippedDuplicateCount,
    ).toBe(1)
    expect(
      updateSessionMemoryRequestSchema.parse({
        projectId,
        memoryId: memory.memoryId,
        reviewState: 'approved',
        enabled: true,
      }).enabled,
    ).toBe(true)
    expect(
      deleteSessionMemoryRequestSchema.parse({
        projectId,
        memoryId: memory.memoryId,
      }).memoryId,
    ).toBe(memory.memoryId)
  })

  it('validates transcript command request, export, save, and search DTOs', () => {
    const transcript = sessionTranscriptSchema.parse({
      contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
      projectId,
      agentSessionId,
      title: 'History session',
      summary: 'Exportable session history',
      status: 'active',
      archived: false,
      archivedAt: null,
      runs: [],
      items: [],
      usageTotals: null,
      redaction: createPublicSessionContextRedaction(),
    })

    expect(
      getSessionTranscriptRequestSchema.parse({
        projectId,
        agentSessionId,
        runId: runId,
      }),
    ).toEqual({ projectId, agentSessionId, runId })
    expect(
      getSessionContextSnapshotRequestSchema.parse({
        projectId,
        agentSessionId,
        runId,
        providerId,
        modelId,
        pendingPrompt: 'Continue with verification.',
      }),
    ).toEqual({
      projectId,
      agentSessionId,
      runId,
      providerId,
      modelId,
      pendingPrompt: 'Continue with verification.',
    })
    expect(() => getSessionTranscriptRequestSchema.parse({ projectId, agentSessionId, extra: true })).toThrow()
    expect(
      exportSessionTranscriptRequestSchema.parse({
        projectId,
        agentSessionId,
        runId: null,
        format: 'markdown',
      }).format,
    ).toBe('markdown')
    expect(
      saveSessionTranscriptExportRequestSchema.parse({
        path: '/tmp/history.md',
        content: '# History',
      }).path,
    ).toBe('/tmp/history.md')

    const exportResponse = sessionTranscriptExportResponseSchema.parse({
      payload: {
        contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
        exportId: 'export-history-1',
        generatedAt: '2026-04-26T10:06:00Z',
        scope: 'session',
        format: 'json',
        transcript,
        contextSnapshot: null,
        redaction: createPublicSessionContextRedaction(),
      },
      content: JSON.stringify({ ok: true }),
      mimeType: 'application/json',
      suggestedFileName: 'history-session-transcript.json',
    })
    expect(exportResponse.payload.transcript.title).toBe('History session')

    const result = sessionTranscriptSearchResultSnippetSchema.parse({
      contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
      resultId: 'item:run-history-1:message:1',
      projectId,
      agentSessionId,
      runId,
      itemId: 'message:1',
      archived: false,
      rank: 0,
      matchedFields: ['text'],
      snippet: 'Matched validation transcript item.',
      redaction: createPublicSessionContextRedaction(),
    })
    expect(
      searchSessionTranscriptsRequestSchema.parse({
        projectId,
        query: 'validation',
        includeArchived: true,
        limit: 12,
      }).includeArchived,
    ).toBe(true)
    expect(
      searchSessionTranscriptsResponseSchema.parse({
        projectId,
        query: 'validation',
        results: [result],
        total: 1,
        truncated: false,
      }).results[0].snippet,
    ).toContain('validation')
  })

  it('validates manual compaction request and response DTOs', () => {
    const compaction = sessionCompactionRecordSchema.parse({
      contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
      compactionId: 'compact-1',
      projectId,
      agentSessionId,
      sourceRunId: runId,
      providerId,
      modelId,
      summary: 'Earlier session history was compacted without deleting raw transcript rows.',
      coveredRunIds: [runId],
      coveredMessageStartId: 1,
      coveredMessageEndId: 4,
      coveredEventStartId: 1,
      coveredEventEndId: 3,
      sourceHash: 'a'.repeat(64),
      inputTokens: 1000,
      summaryTokens: 80,
      rawTailMessageCount: 8,
      policyReason: 'manual_compact_requested',
      trigger: 'manual',
      active: true,
      diagnostic: null,
      createdAt,
      supersededAt: null,
      redaction: createPublicSessionContextRedaction(),
    })

    expect(
      compactSessionHistoryRequestSchema.parse({
        projectId,
        agentSessionId,
        runId,
        rawTailMessageCount: 8,
      }),
    ).toEqual({ projectId, agentSessionId, runId, rawTailMessageCount: 8 })
    expect(() =>
      compactSessionHistoryRequestSchema.parse({
        projectId,
        agentSessionId,
        runId,
        rawTailMessageCount: 30,
      }),
    ).toThrow(/less than or equal/)
    expect(() =>
      sessionCompactionRecordSchema.parse({
        ...compaction,
        coveredMessageStartId: 4,
        coveredMessageEndId: 1,
      }),
    ).toThrow(/ordered/)

    const snapshot = sessionContextSnapshotSchema.parse({
      contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
      snapshotId: 'context-snapshot-compacted',
      projectId,
      agentSessionId,
      runId,
      providerId,
      modelId,
      generatedAt: '2026-04-26T10:10:00Z',
      budget: createContextBudget(480, 1000),
      contributors: [
        makeContributor('compaction_summary:compact-1', 1, {
          kind: 'compaction_summary',
          label: 'Compacted history summary',
          text: compaction.summary,
        }),
      ],
      policyDecisions: [],
      usageTotals: null,
      redaction: createPublicSessionContextRedaction(),
    })
    expect(compactSessionHistoryResponseSchema.parse({ compaction, contextSnapshot: snapshot }).compaction.active).toBe(true)
  })
})

function makeRunTranscript(): RunTranscriptDto {
  return {
    contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
    projectId,
    agentSessionId,
    runId,
    providerId,
    modelId,
    status: 'completed',
    sourceKind: 'owned_agent',
    startedAt: createdAt,
    completedAt: '2026-04-26T10:01:00Z',
    usageTotals: {
      projectId,
      runId,
      providerId,
      modelId,
      inputTokens: 100,
      outputTokens: 50,
      totalTokens: 150,
      estimatedCostMicros: 10,
      source: 'provider',
      updatedAt: '2026-04-26T10:01:00Z',
    },
    items: [
      {
        contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
        itemId: 'message:1',
        projectId,
        agentSessionId,
        runId,
        providerId,
        modelId,
        sourceKind: 'owned_agent',
        sourceTable: 'agent_messages',
        sourceId: '1',
        sequence: 1,
        createdAt,
        kind: 'message',
        actor: 'user',
        title: 'User message',
        text: 'Implement priority 4 phase 0.',
        summary: null,
        toolCallId: null,
        toolName: null,
        toolState: null,
        filePath: null,
        checkpointKind: null,
        actionId: null,
        redaction: createPublicSessionContextRedaction(),
      },
      {
        contractVersion: CADENCE_SESSION_CONTEXT_CONTRACT_VERSION,
        itemId: 'tool_call:1',
        projectId,
        agentSessionId,
        runId,
        providerId,
        modelId,
        sourceKind: 'owned_agent',
        sourceTable: 'agent_tool_calls',
        sourceId: 'tool-1',
        sequence: 2,
        createdAt: '2026-04-26T10:00:10Z',
        kind: 'tool_call',
        actor: 'tool',
        title: 'Tool call',
        text: null,
        summary: 'read succeeded.',
        toolCallId: 'tool-1',
        toolName: 'read',
        toolState: 'succeeded',
        filePath: null,
        checkpointKind: null,
        actionId: null,
        redaction: createPublicSessionContextRedaction(),
      },
    ],
    redaction: createPublicSessionContextRedaction(),
  }
}

function makeContributor(
  contributorId: string,
  sequence: number,
  overrides: Partial<SessionContextContributorDto> = {},
): SessionContextContributorDto {
  return {
    contributorId,
    kind: 'approved_memory',
    label: 'Project decision',
    projectId,
    agentSessionId,
    runId,
    sourceId: contributorId,
    sequence,
    estimatedTokens: 20,
    estimatedChars: 80,
    included: true,
    modelVisible: true,
    text: 'Use the native owned-agent runtime.',
    redaction: createPublicSessionContextRedaction(),
    ...overrides,
  }
}
