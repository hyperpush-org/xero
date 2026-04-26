import { describe, expect, it } from 'vitest'
import {
  mapRuntimeRun,
  runtimeRunSchema,
  startRuntimeRunRequestSchema,
  updateRuntimeRunControlsRequestSchema,
} from './runtime'

function makeRuntimeRunDto(overrides: Record<string, unknown> = {}) {
  return {
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runId: 'run-project-1',
    runtimeKind: 'openai_codex',
    providerId: 'openrouter',
    supervisorKind: 'detached_pty',
    status: 'running',
    transport: {
      kind: 'tcp',
      endpoint: '127.0.0.1:4455',
      liveness: 'reachable',
    },
    controls: {
      active: {
        modelId: 'openai/gpt-4.1-mini',
        thinkingEffort: 'medium',
        approvalMode: 'suggest',
        planModeRequired: true,
        revision: 1,
        appliedAt: '2026-04-15T20:00:00Z',
      },
      pending: {
        modelId: 'anthropic/claude-3.5-haiku',
        thinkingEffort: 'low',
        approvalMode: 'auto_edit',
        planModeRequired: true,
        revision: 2,
        queuedAt: '2026-04-15T20:01:00Z',
        queuedPrompt: 'Review the diff before continuing.',
        queuedPromptAt: '2026-04-15T20:01:00Z',
      },
    },
    startedAt: '2026-04-15T20:00:00Z',
    lastHeartbeatAt: '2026-04-15T20:00:05Z',
    lastCheckpointSequence: 1,
    lastCheckpointAt: '2026-04-15T20:00:06Z',
    stoppedAt: null,
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-15T20:01:00Z',
    checkpoints: [],
    ...overrides,
  }
}

describe('runtime run control schemas', () => {
  it('maps durable active and pending control snapshots into a selected pending projection', () => {
    const parsed = runtimeRunSchema.parse(makeRuntimeRunDto())
    const view = mapRuntimeRun(parsed)

    expect(view.controls.active).toMatchObject({
      modelId: 'openai/gpt-4.1-mini',
      thinkingEffort: 'medium',
      approvalMode: 'suggest',
      planModeRequired: true,
      revision: 1,
    })
    expect(view.controls.pending).toMatchObject({
      modelId: 'anthropic/claude-3.5-haiku',
      thinkingEffort: 'low',
      approvalMode: 'auto_edit',
      planModeRequired: true,
      revision: 2,
      queuedPrompt: 'Review the diff before continuing.',
      hasQueuedPrompt: true,
    })
    expect(view.controls.selected).toMatchObject({
      source: 'pending',
      modelId: 'anthropic/claude-3.5-haiku',
      thinkingEffort: 'low',
      approvalMode: 'auto_edit',
      planModeRequired: true,
      revision: 2,
      queuedPrompt: 'Review the diff before continuing.',
      hasQueuedPrompt: true,
    })
  })

  it('rejects runtime runs missing durable control snapshots', () => {
    const parsed = runtimeRunSchema.safeParse({
      ...makeRuntimeRunDto(),
      controls: undefined,
    })

    expect(parsed.success).toBe(false)
    if (parsed.success) {
      throw new Error('Expected runtimeRunSchema to reject missing controls.')
    }
    expect(parsed.error.issues.some((issue) => issue.path.join('.') === 'controls')).toBe(true)
  })

  it('rejects malformed pending prompt timestamps and unsupported approval modes', () => {
    const parsed = runtimeRunSchema.safeParse({
      ...makeRuntimeRunDto(),
      controls: {
        active: {
          modelId: 'openai/gpt-4.1-mini',
          thinkingEffort: 'medium',
          approvalMode: 'suggest',
          planModeRequired: true,
          revision: 1,
          appliedAt: '2026-04-15T20:00:00Z',
        },
        pending: {
          modelId: 'anthropic/claude-3.5-haiku',
          thinkingEffort: 'low',
          approvalMode: 'ship_it',
          planModeRequired: true,
          revision: 2,
          queuedAt: '2026-04-15T20:01:00Z',
          queuedPrompt: 'Review the diff before continuing.',
          queuedPromptAt: null,
        },
      },
    })

    expect(parsed.success).toBe(false)
    if (parsed.success) {
      throw new Error('Expected runtimeRunSchema to reject malformed pending controls.')
    }
    expect(parsed.error.issues.some((issue) => issue.path.join('.') === 'controls.pending.approvalMode')).toBe(true)
  })

  it('requires at least one control delta or prompt when queueing runtime-run changes', () => {
    const emptyUpdate = updateRuntimeRunControlsRequestSchema.safeParse({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: 'run-project-1',
    })
    const validStart = startRuntimeRunRequestSchema.parse({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      initialControls: {
        modelId: 'openai/gpt-4.1-mini',
        thinkingEffort: 'high',
        approvalMode: 'yolo',
        planModeRequired: true,
      },
      initialPrompt: 'Continue with the next verifier step.',
    })

    expect(emptyUpdate.success).toBe(false)
    expect(validStart).toMatchObject({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      initialControls: {
        modelId: 'openai/gpt-4.1-mini',
        thinkingEffort: 'high',
        approvalMode: 'yolo',
        planModeRequired: true,
      },
      initialPrompt: 'Continue with the next verifier step.',
    })

    const compactingUpdate = updateRuntimeRunControlsRequestSchema.parse({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: 'run-project-1',
      prompt: 'Continue after compaction.',
      autoCompact: {
        enabled: true,
        thresholdPercent: 85,
        rawTailMessageCount: 8,
      },
    })
    expect(compactingUpdate.autoCompact).toEqual({
      enabled: true,
      thresholdPercent: 85,
      rawTailMessageCount: 8,
    })

    expect(() =>
      updateRuntimeRunControlsRequestSchema.parse({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        runId: 'run-project-1',
        prompt: 'Continue.',
        autoCompact: {
          enabled: true,
          thresholdPercent: 101,
          rawTailMessageCount: 8,
        },
      }),
    ).toThrow(/less than or equal/)
  })

  it('defaults planModeRequired to false and rejects malformed plan mode values', () => {
    const defaulted = startRuntimeRunRequestSchema.parse({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      initialControls: {
        modelId: 'openai/gpt-4.1-mini',
        approvalMode: 'suggest',
      },
    })

    const malformed = runtimeRunSchema.safeParse(
      makeRuntimeRunDto({
        controls: {
          active: {
            modelId: 'openai/gpt-4.1-mini',
            thinkingEffort: 'medium',
            approvalMode: 'suggest',
            planModeRequired: 'yes',
            revision: 1,
            appliedAt: '2026-04-15T20:00:00Z',
          },
        },
      }),
    )
    const malformedRequest = updateRuntimeRunControlsRequestSchema.safeParse({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: 'run-project-1',
      controls: {
        modelId: 'openai/gpt-4.1-mini',
        approvalMode: 'suggest',
        planModeRequired: 'true',
      },
    })

    expect(defaulted.initialControls?.planModeRequired).toBe(false)
    expect(malformed.success).toBe(false)
    expect(malformedRequest.success).toBe(false)
    if (malformed.success) {
      throw new Error('Expected runtimeRunSchema to reject non-boolean planModeRequired values.')
    }
    expect(malformed.error.issues.some((issue) => issue.path.join('.') === 'controls.active.planModeRequired')).toBe(true)

    if (malformedRequest.success) {
      throw new Error('Expected updateRuntimeRunControlsRequestSchema to reject non-boolean planModeRequired values.')
    }
    expect(malformedRequest.error.issues.some((issue) => issue.path.join('.') === 'controls.planModeRequired')).toBe(true)
  })
})
