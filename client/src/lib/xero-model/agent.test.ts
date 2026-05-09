import { describe, expect, it } from 'vitest'
import {
  agentTraceExportSchema,
  agentRunSummarySchema,
  agentRunSchema,
  exportAgentTraceRequestSchema,
  listAgentRunsResponseSchema,
  mapAgentRun,
  resumeAgentRunRequestSchema,
  sendAgentMessageRequestSchema,
  startAgentTaskRequestSchema,
  subscribeAgentStreamResponseSchema,
} from './agent'

function makeAgentRunDto(overrides: Record<string, unknown> = {}) {
  return {
    runtimeAgentId: 'ask',
    agentDefinitionId: 'ask',
    agentDefinitionVersion: 1,
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runId: 'run-agent-1',
    traceId: '0123456789abcdef0123456789abcdef',
    lineageKind: 'top_level',
    parentRunId: null,
    parentTraceId: null,
    parentSubagentId: null,
    subagentRole: null,
    providerId: 'openai_codex',
    modelId: 'openai_codex',
    status: 'completed',
    prompt: 'Inspect the project.',
    systemPrompt: 'xero-owned-agent-v1',
    startedAt: '2026-04-24T12:00:00Z',
    lastHeartbeatAt: '2026-04-24T12:00:01Z',
    completedAt: '2026-04-24T12:00:02Z',
    cancelledAt: null,
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-24T12:00:02Z',
    messages: [
      {
        id: 1,
        projectId: 'project-1',
        runId: 'run-agent-1',
        role: 'user',
        content: 'Inspect the project.',
        createdAt: '2026-04-24T12:00:00Z',
      },
    ],
    events: [
      {
        id: 1,
        projectId: 'project-1',
        runId: 'run-agent-1',
        eventKind: 'run_completed',
        payload: { summary: 'Done.' },
        createdAt: '2026-04-24T12:00:02Z',
      },
    ],
    toolCalls: [
      {
        projectId: 'project-1',
        runId: 'run-agent-1',
        toolCallId: 'tool-call-1',
        toolName: 'read',
        input: { path: 'src/main.rs' },
        state: 'succeeded',
        result: { summary: 'Read file.' },
        error: null,
        startedAt: '2026-04-24T12:00:01Z',
        completedAt: '2026-04-24T12:00:02Z',
      },
    ],
    fileChanges: [
      {
        id: 1,
        projectId: 'project-1',
        runId: 'run-agent-1',
        traceId: '0123456789abcdef0123456789abcdef',
        topLevelRunId: 'run-agent-1',
        subagentId: null,
        subagentRole: null,
        path: 'src/main.rs',
        operation: 'edit',
        oldHash: '0'.repeat(64),
        newHash: '1'.repeat(64),
        createdAt: '2026-04-24T12:00:02Z',
      },
    ],
    checkpoints: [
      {
        id: 1,
        projectId: 'project-1',
        runId: 'run-agent-1',
        checkpointKind: 'tool',
        summary: 'Rollback data for src/main.rs.',
        payload: { path: 'src/main.rs' },
        createdAt: '2026-04-24T12:00:02Z',
      },
    ],
    actionRequests: [],
    ...overrides,
  }
}

function makeAgentRunSummaryDto(overrides: Record<string, unknown> = {}) {
  const run = makeAgentRunDto()

  return {
    runtimeAgentId: run.runtimeAgentId,
    agentDefinitionId: run.agentDefinitionId,
    agentDefinitionVersion: run.agentDefinitionVersion,
    projectId: run.projectId,
    agentSessionId: run.agentSessionId,
    runId: run.runId,
    traceId: run.traceId,
    lineageKind: run.lineageKind,
    parentRunId: run.parentRunId,
    parentTraceId: run.parentTraceId,
    parentSubagentId: run.parentSubagentId,
    subagentRole: run.subagentRole,
    providerId: run.providerId,
    modelId: run.modelId,
    status: run.status,
    prompt: run.prompt,
    startedAt: run.startedAt,
    completedAt: run.completedAt,
    cancelledAt: run.cancelledAt,
    lastErrorCode: run.lastErrorCode,
    lastError: run.lastError,
    updatedAt: run.updatedAt,
    ...overrides,
  }
}

describe('owned agent run schemas', () => {
  it('parses durable run snapshots and maps task status flags', () => {
    const parsed = agentRunSchema.parse(makeAgentRunDto())
    const view = mapAgentRun(parsed)

    expect(view.statusLabel).toBe('Completed')
    expect(view.isTerminal).toBe(true)
    expect(view.isActive).toBe(false)
    expect(view.latestEvent?.eventKind).toBe('run_completed')
    expect(view.toolCalls[0]).toMatchObject({
      toolCallId: 'tool-call-1',
      toolName: 'read',
      state: 'succeeded',
    })
    expect(view.fileChanges[0]).toMatchObject({
      path: 'src/main.rs',
      operation: 'edit',
    })
  })

  it('accepts durable tool registry snapshot events', () => {
    const parsed = agentRunSchema.parse(
      makeAgentRunDto({
        events: [
          {
            id: 1,
            projectId: 'project-1',
            runId: 'run-agent-1',
            eventKind: 'tool_registry_snapshot',
            payload: {
              kind: 'active_tool_registry',
              turnIndex: 0,
              toolNames: ['read', 'tool_search', 'todo'],
            },
            createdAt: '2026-04-24T12:00:02Z',
          },
        ],
      }),
    )

    expect(parsed.events[0].eventKind).toBe('tool_registry_snapshot')
  })

  it('accepts durable environment lifecycle events', () => {
    const parsed = agentRunSchema.parse(
      makeAgentRunDto({
        events: [
          {
            id: 1,
            projectId: 'project-1',
            runId: 'run-agent-1',
            eventKind: 'environment_lifecycle_update',
            payload: {
              environmentId: 'env-project-1-run-agent-1',
              state: 'ready',
              pendingMessageCount: 0,
            },
            createdAt: '2026-04-24T12:00:02Z',
          },
        ],
      }),
    )

    expect(parsed.events[0].eventKind).toBe('environment_lifecycle_update')
  })

  it('accepts durable policy decision events', () => {
    const parsed = agentRunSchema.parse(
      makeAgentRunDto({
        events: [
          {
            id: 1,
            projectId: 'project-1',
            runId: 'run-agent-1',
            eventKind: 'policy_decision',
            payload: {
              toolCallId: 'tool-call-1',
              toolName: 'command',
              action: 'require_approval',
              code: 'policy_escalated_approval_mode',
              explanation: 'Active approval mode requires operator review.',
            },
            createdAt: '2026-04-24T12:00:02Z',
          },
        ],
      }),
    )

    expect(parsed.events[0].eventKind).toBe('policy_decision')
  })

  it('accepts state-machine events and plan checkpoints', () => {
    const parsed = agentRunSchema.parse(
      makeAgentRunDto({
        status: 'paused',
        checkpoints: [
          {
            id: 1,
            projectId: 'project-1',
            runId: 'run-agent-1',
            checkpointKind: 'plan',
            summary: 'Structured plan updated with 2 item(s).',
            payload: { total: 2 },
            createdAt: '2026-04-24T12:00:02Z',
          },
        ],
        events: [
          {
            id: 1,
            projectId: 'project-1',
            runId: 'run-agent-1',
            eventKind: 'state_transition',
            payload: { to: 'plan', reason: 'Task requires a structured plan.' },
            createdAt: '2026-04-24T12:00:02Z',
          },
          {
            id: 2,
            projectId: 'project-1',
            runId: 'run-agent-1',
            eventKind: 'plan_updated',
            payload: { total: 2, completed: 0 },
            createdAt: '2026-04-24T12:00:03Z',
          },
          {
            id: 3,
            projectId: 'project-1',
            runId: 'run-agent-1',
            eventKind: 'verification_gate',
            payload: { status: 'required' },
            createdAt: '2026-04-24T12:00:04Z',
          },
          {
            id: 4,
            projectId: 'project-1',
            runId: 'run-agent-1',
            eventKind: 'run_paused',
            payload: { stopReason: 'waiting_for_approval' },
            createdAt: '2026-04-24T12:00:05Z',
          },
        ],
      }),
    )
    const view = mapAgentRun(parsed)

    expect(view.statusLabel).toBe('Paused')
    expect(view.isActive).toBe(false)
    expect(view.isTerminal).toBe(false)
    expect(parsed.events.map((event) => event.eventKind)).toEqual([
      'state_transition',
      'plan_updated',
      'verification_gate',
      'run_paused',
    ])
  })

  it('validates task-start controls and stream replay metadata', () => {
    const request = startAgentTaskRequestSchema.parse({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      prompt: 'Fix the failing test.',
      controls: {
        runtimeAgentId: 'engineer',
        agentDefinitionId: 'project_release_engineer',
        modelId: 'openai_codex',
        approvalMode: 'yolo',
      },
    })
    const subscription = subscribeAgentStreamResponseSchema.parse({
      runId: 'run-agent-1',
      replayedEventCount: 3,
    })

    expect(request.controls?.planModeRequired).toBe(false)
    expect(request.controls?.agentDefinitionId).toBe('project_release_engineer')
    expect(subscription.replayedEventCount).toBe(3)
  })

  it('validates object-shaped trace export contracts', () => {
    expect(
      exportAgentTraceRequestSchema.parse({
        runId: 'run-agent-1',
        includeSupportBundle: true,
      }).includeSupportBundle,
    ).toBe(true)

    const traceExport = agentTraceExportSchema.parse({
      trace: { traceId: '0123456789abcdef0123456789abcdef' },
      timeline: { events: [] },
      diagnostics: { diagnostics: [] },
      qualityGates: { gates: [] },
      productionReadiness: {
        traceId: '0123456789abcdef0123456789abcdef',
        status: 'ready',
      },
      markdownSummary: '# Trace summary',
      supportBundle: {
        traceId: '0123456789abcdef0123456789abcdef',
        run: { runId: 'run-agent-1' },
      },
      canonicalTrace: {
        traceId: '0123456789abcdef0123456789abcdef',
        generatedFrom: 'owned_agent_trace',
      },
    })

    expect(traceExport.qualityGates.gates).toEqual([])
    expect(() =>
      agentTraceExportSchema.parse({
        ...traceExport,
        qualityGates: 'not a quality-gate object',
      }),
    ).toThrow()
    expect(() =>
      agentTraceExportSchema.parse({
        ...traceExport,
        markdownSummary: ' ',
      }),
    ).toThrow()
  })

  it('parses pinned custom agent definition metadata on durable run snapshots', () => {
    const parsed = agentRunSchema.parse(
      makeAgentRunDto({
        runtimeAgentId: 'ask',
        agentDefinitionId: 'project_researcher',
        agentDefinitionVersion: 2,
      }),
    )

    expect(parsed.runtimeAgentId).toBe('ask')
    expect(parsed.agentDefinitionId).toBe('project_researcher')
    expect(parsed.agentDefinitionVersion).toBe(2)
  })

  it('rejects contradictory run lineage and nested run identity', () => {
    expect(() =>
      agentRunSchema.parse(
        makeAgentRunDto({
          parentRunId: 'run-parent',
        }),
      ),
    ).toThrow(/Top-level/)
    expect(() =>
      agentRunSchema.parse(
        makeAgentRunDto({
          lineageKind: 'subagent_child',
          parentRunId: 'run-parent',
          parentTraceId: 'fedcba9876543210fedcba9876543210',
          parentSubagentId: 'subagent-1',
          subagentRole: null,
        }),
      ),
    ).toThrow(/Subagent/)
    expect(() =>
      agentRunSchema.parse(
        makeAgentRunDto({
          messages: [
            {
              id: 1,
              projectId: 'project-1',
              runId: 'other-run',
              role: 'user',
              content: 'Inspect the project.',
              createdAt: '2026-04-24T12:00:00Z',
            },
          ],
        }),
      ),
    ).toThrow(/messages/)
    expect(() =>
      agentRunSchema.parse(
        makeAgentRunDto({
          messages: [
            {
              id: 1,
              projectId: 'project-1',
              runId: 'run-agent-1',
              role: 'user',
              content: 'Inspect the project.',
              createdAt: '2026-04-24T12:00:00Z',
            },
            {
              id: 1,
              projectId: 'project-1',
              runId: 'run-agent-1',
              role: 'assistant',
              content: 'Inspection complete.',
              createdAt: '2026-04-24T12:00:01Z',
            },
          ],
        }),
      ),
    ).toThrow(/message ids/)
    expect(() =>
      agentRunSchema.parse(
        makeAgentRunDto({
          messages: [
            {
              id: 1,
              projectId: 'project-1',
              runId: 'run-agent-1',
              role: 'user',
              content: 'Inspect the project.',
              createdAt: '2026-04-24T12:00:00Z',
              attachments: [
                {
                  id: 1,
                  messageId: 2,
                  kind: 'document',
                  absolutePath: '/tmp/context.md',
                  mediaType: 'text/markdown',
                  originalName: 'context.md',
                  sizeBytes: 42,
                  createdAt: '2026-04-24T12:00:00Z',
                },
              ],
            },
          ],
        }),
      ),
    ).toThrow(/enclosing message id/)
    expect(() =>
      agentRunSchema.parse(
        makeAgentRunDto({
          toolCalls: [
            {
              projectId: 'project-1',
              runId: 'run-agent-1',
              toolCallId: 'tool-call-1',
              toolName: 'read',
              input: { path: 'src/main.rs' },
              state: 'succeeded',
              result: { summary: 'Read file.' },
              error: null,
              startedAt: '2026-04-24T12:00:01Z',
              completedAt: '2026-04-24T12:00:02Z',
            },
            {
              projectId: 'project-1',
              runId: 'run-agent-1',
              toolCallId: 'tool-call-1',
              toolName: 'write',
              input: { path: 'src/main.rs' },
              state: 'pending',
              result: null,
              error: null,
              startedAt: '2026-04-24T12:00:02Z',
              completedAt: null,
            },
          ],
        }),
      ),
    ).toThrow(/tool call ids/)
  })

  it('rejects contradictory run summaries and duplicate list entries', () => {
    expect(() =>
      agentRunSummarySchema.parse(
        makeAgentRunSummaryDto({
          parentRunId: 'run-parent',
        }),
      ),
    ).toThrow(/Top-level/)

    const summary = agentRunSummarySchema.parse(makeAgentRunSummaryDto())

    expect(() =>
      listAgentRunsResponseSchema.parse({
        runs: [summary, { ...summary }],
      }),
    ).toThrow(/run ids/)
  })

  it('validates auto-compact preferences on owned-agent continuations', () => {
    expect(
      sendAgentMessageRequestSchema.parse({
        runId: 'run-agent-1',
        prompt: 'Continue after trimming old context.',
        autoCompact: {
          enabled: true,
          thresholdPercent: 85,
          rawTailMessageCount: 8,
        },
      }).autoCompact?.thresholdPercent,
    ).toBe(85)
    expect(
      resumeAgentRunRequestSchema.parse({
        runId: 'run-agent-1',
        response: 'Approved; continue.',
        autoCompact: {
          enabled: false,
        },
      }).autoCompact?.enabled,
    ).toBe(false)
    expect(() =>
      sendAgentMessageRequestSchema.parse({
        runId: 'run-agent-1',
        prompt: 'Continue.',
        autoCompact: {
          enabled: true,
          rawTailMessageCount: 1,
        },
      }),
    ).toThrow(/greater than or equal/)
  })

  it('rejects malformed event kinds and empty prompts', () => {
    const badRun = agentRunSchema.safeParse(
      makeAgentRunDto({
        events: [
          {
            id: 1,
            projectId: 'project-1',
            runId: 'run-agent-1',
            eventKind: 'debug_log',
            payload: {},
            createdAt: '2026-04-24T12:00:02Z',
          },
        ],
      }),
    )
    const badRequest = startAgentTaskRequestSchema.safeParse({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      prompt: '   ',
    })

    expect(badRun.success).toBe(false)
    expect(badRequest.success).toBe(false)
  })
})
