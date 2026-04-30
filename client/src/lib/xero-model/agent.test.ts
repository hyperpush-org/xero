import { describe, expect, it } from 'vitest'
import {
  agentRunSchema,
  mapAgentRun,
  resumeAgentRunRequestSchema,
  sendAgentMessageRequestSchema,
  startAgentTaskRequestSchema,
  subscribeAgentStreamResponseSchema,
} from './agent'

function makeAgentRunDto(overrides: Record<string, unknown> = {}) {
  return {
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runId: 'run-agent-1',
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
        modelId: 'openai_codex',
        approvalMode: 'yolo',
      },
    })
    const subscription = subscribeAgentStreamResponseSchema.parse({
      runId: 'run-agent-1',
      replayedEventCount: 3,
    })

    expect(request.controls?.planModeRequired).toBe(false)
    expect(subscription.replayedEventCount).toBe(3)
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
