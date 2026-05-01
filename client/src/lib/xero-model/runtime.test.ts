import { describe, expect, it } from 'vitest'
import {
  getRuntimeAgentDescriptor,
  mapRuntimeRun,
  RUNTIME_AGENT_DESCRIPTORS,
  runtimeAgentIdSchema,
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
    supervisorKind: 'owned_agent',
    status: 'running',
    transport: {
      kind: 'internal',
      endpoint: 'xero://owned-agent',
      liveness: 'reachable',
    },
    controls: {
      active: {
        runtimeAgentId: 'engineer',
        modelId: 'openai/gpt-4.1-mini',
        thinkingEffort: 'medium',
        approvalMode: 'suggest',
        planModeRequired: true,
        revision: 1,
        appliedAt: '2026-04-15T20:00:00Z',
      },
      pending: {
        runtimeAgentId: 'engineer',
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
  it('registers Debug as an engineering-capable runtime agent', () => {
    expect(runtimeAgentIdSchema.parse('debug')).toBe('debug')
    expect(RUNTIME_AGENT_DESCRIPTORS.map((agent) => agent.id)).toEqual(['ask', 'engineer', 'debug'])

    expect(getRuntimeAgentDescriptor('debug')).toMatchObject({
      id: 'debug',
      label: 'Debug',
      toolPolicy: 'engineering',
      outputContract: 'debug_summary',
      allowPlanGate: true,
      allowVerificationGate: true,
      allowedApprovalModes: ['suggest', 'auto_edit', 'yolo'],
    })
  })

  it('maps durable active and pending control snapshots into a selected pending projection', () => {
    const parsed = runtimeRunSchema.parse(makeRuntimeRunDto())
    const view = mapRuntimeRun(parsed)

    expect(view.controls.active).toMatchObject({
      runtimeAgentId: 'engineer',
      modelId: 'openai/gpt-4.1-mini',
      thinkingEffort: 'medium',
      approvalMode: 'suggest',
      planModeRequired: true,
      revision: 1,
    })
    expect(view.controls.pending).toMatchObject({
      runtimeAgentId: 'engineer',
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
      runtimeAgentId: 'engineer',
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

  it('requires runtimeAgentId on durable controls and control updates', () => {
    const missingSnapshotAgent = runtimeRunSchema.safeParse({
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
      },
    })
    const missingUpdateAgent = updateRuntimeRunControlsRequestSchema.safeParse({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: 'run-project-1',
      controls: {
        modelId: 'openai/gpt-4.1-mini',
        approvalMode: 'suggest',
      },
    })

    expect(missingSnapshotAgent.success).toBe(false)
    expect(missingUpdateAgent.success).toBe(false)
    if (missingSnapshotAgent.success || missingUpdateAgent.success) {
      throw new Error('Expected runtime controls without runtimeAgentId to be rejected.')
    }
    expect(
      missingSnapshotAgent.error.issues.some((issue) => issue.path.join('.') === 'controls.active.runtimeAgentId'),
    ).toBe(true)
    expect(missingUpdateAgent.error.issues.some((issue) => issue.path.join('.') === 'controls.runtimeAgentId')).toBe(true)
  })

  it('rejects malformed pending prompt timestamps and unsupported approval modes', () => {
    const parsed = runtimeRunSchema.safeParse({
      ...makeRuntimeRunDto(),
      controls: {
        active: {
          runtimeAgentId: 'engineer',
          modelId: 'openai/gpt-4.1-mini',
          thinkingEffort: 'medium',
          approvalMode: 'suggest',
          planModeRequired: true,
          revision: 1,
          appliedAt: '2026-04-15T20:00:00Z',
        },
        pending: {
          runtimeAgentId: 'engineer',
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
        runtimeAgentId: 'engineer',
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
        runtimeAgentId: 'engineer',
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
        runtimeAgentId: 'ask',
        modelId: 'openai/gpt-4.1-mini',
        approvalMode: 'suggest',
      },
    })

    const malformed = runtimeRunSchema.safeParse(
      makeRuntimeRunDto({
        controls: {
          active: {
            runtimeAgentId: 'ask',
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
        runtimeAgentId: 'ask',
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
