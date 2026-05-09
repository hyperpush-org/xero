import { describe, expect, it } from 'vitest'
import {
  ALL_RUNTIME_AGENT_DESCRIPTORS,
  getRuntimeAgentAvailability,
  getRuntimeAgentDescriptor,
  getRuntimeAgentDescriptorsForAvailability,
  getRuntimeAgentDescriptorsForProjectOrigin,
  mapRuntimeRun,
  RUNTIME_AGENT_DESCRIPTORS,
  runtimeAgentIsSelectableForProjectOrigin,
  runtimeAgentIdSchema,
  runtimeProviderIdSchema,
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
        agentDefinitionId: 'phase4_builder',
        agentDefinitionVersion: 3,
        modelId: 'openai/gpt-4.1-mini',
        thinkingEffort: 'medium',
        approvalMode: 'suggest',
        planModeRequired: true,
        revision: 1,
        appliedAt: '2026-04-15T20:00:00Z',
      },
      pending: {
        runtimeAgentId: 'engineer',
        agentDefinitionId: 'phase4_builder',
        agentDefinitionVersion: 4,
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
  it('accepts DeepSeek as a first-party runtime provider id', () => {
    expect(runtimeProviderIdSchema.parse('deepseek')).toBe('deepseek')
  })

  it('registers built-in runtime agents as descriptor-backed entries', () => {
    expect(runtimeAgentIdSchema.parse('debug')).toBe('debug')
    expect(runtimeAgentIdSchema.parse('plan')).toBe('plan')
    expect(runtimeAgentIdSchema.parse('crawl')).toBe('crawl')
    expect(runtimeAgentIdSchema.parse('agent_create')).toBe('agent_create')
    expect(runtimeAgentIdSchema.parse('test')).toBe('test')
    expect(ALL_RUNTIME_AGENT_DESCRIPTORS.map((agent) => agent.id)).toEqual([
      'ask',
      'plan',
      'engineer',
      'debug',
      'crawl',
      'agent_create',
      'test',
    ])
    expect(RUNTIME_AGENT_DESCRIPTORS.map((agent) => agent.id)).toContain('test')

    expect(getRuntimeAgentDescriptor('debug')).toMatchObject({
      id: 'debug',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'debugging',
      label: 'Debug',
      toolPolicy: 'engineering',
      outputContract: 'debug_summary',
      allowPlanGate: true,
      allowVerificationGate: true,
      allowedApprovalModes: ['suggest', 'auto_edit', 'yolo'],
    })
    expect(getRuntimeAgentDescriptor('plan')).toMatchObject({
      id: 'plan',
      label: 'Plan',
      shortLabel: 'Plan',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'planning',
      promptPolicy: 'plan',
      toolPolicy: 'planning',
      outputContract: 'plan_pack',
      defaultApprovalMode: 'suggest',
      allowPlanGate: false,
      allowVerificationGate: false,
      allowAutoCompact: true,
      allowedApprovalModes: ['suggest'],
    })
    expect(getRuntimeAgentDescriptor('crawl')).toMatchObject({
      id: 'crawl',
      label: 'Crawl',
      shortLabel: 'Crawl',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'repository_recon',
      promptPolicy: 'crawl',
      toolPolicy: 'repository_recon',
      outputContract: 'crawl_report',
      allowPlanGate: false,
      allowVerificationGate: false,
      allowedApprovalModes: ['suggest'],
    })
    expect(getRuntimeAgentDescriptor('agent_create')).toMatchObject({
      id: 'agent_create',
      label: 'Agent Create',
      shortLabel: 'Create',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'agent_builder',
      promptPolicy: 'agent_create',
      toolPolicy: 'agent_builder',
      outputContract: 'agent_definition_draft',
      allowPlanGate: false,
      allowVerificationGate: false,
      allowedApprovalModes: ['suggest'],
    })
    expect(getRuntimeAgentDescriptor('test')).toMatchObject({
      id: 'test',
      label: 'Test',
      shortLabel: 'Test',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'harness_test',
      promptPolicy: 'harness_test',
      toolPolicy: 'harness_test',
      outputContract: 'harness_test_report',
      defaultApprovalMode: 'suggest',
      allowPlanGate: false,
      allowVerificationGate: false,
      allowAutoCompact: false,
      allowedApprovalModes: ['suggest'],
    })
  })

  it('filters Test from visible descriptors outside dev, test, CI, or explicit harness opt-in', () => {
    expect(getRuntimeAgentAvailability({ DEV: false, MODE: 'production' })).toEqual({
      testAgentEnabled: false,
    })
    expect(
      getRuntimeAgentDescriptorsForAvailability({ testAgentEnabled: false }).map(
        (agent) => agent.id,
      ),
    ).toEqual(['ask', 'plan', 'engineer', 'debug', 'crawl', 'agent_create'])
    expect(
      getRuntimeAgentDescriptorsForAvailability({ testAgentEnabled: true }).map(
        (agent) => agent.id,
      ),
    ).toEqual(['ask', 'plan', 'engineer', 'debug', 'crawl', 'agent_create', 'test'])

    expect(
      getRuntimeAgentAvailability({ DEV: true, MODE: 'development' }).testAgentEnabled,
    ).toBe(true)
    expect(getRuntimeAgentAvailability({ DEV: false, MODE: 'test' }).testAgentEnabled).toBe(true)
    expect(
      getRuntimeAgentAvailability({
        DEV: false,
        MODE: 'production',
        VITE_CI: 'true',
      }).testAgentEnabled,
    ).toBe(true)
    expect(
      getRuntimeAgentAvailability({
        DEV: false,
        MODE: 'production',
        VITE_XERO_ENABLE_TEST_AGENT: 'enabled',
      }).testAgentEnabled,
    ).toBe(true)
  })

  it('shows Crawl only for brownfield project origins', () => {
    expect(runtimeAgentIsSelectableForProjectOrigin('crawl', 'brownfield')).toBe(true)
    expect(runtimeAgentIsSelectableForProjectOrigin('crawl', 'greenfield')).toBe(false)
    expect(runtimeAgentIsSelectableForProjectOrigin('crawl', 'unknown')).toBe(false)
    expect(runtimeAgentIsSelectableForProjectOrigin('plan', 'greenfield')).toBe(true)
    expect(runtimeAgentIsSelectableForProjectOrigin('plan', 'brownfield')).toBe(true)
    expect(runtimeAgentIsSelectableForProjectOrigin('ask', 'unknown')).toBe(true)

    expect(
      getRuntimeAgentDescriptorsForProjectOrigin('greenfield', { testAgentEnabled: false }).map(
        (agent) => agent.id,
      ),
    ).toEqual(['ask', 'plan', 'engineer', 'debug', 'agent_create'])
    expect(
      getRuntimeAgentDescriptorsForProjectOrigin('brownfield', { testAgentEnabled: false }).map(
        (agent) => agent.id,
      ),
    ).toEqual(['ask', 'plan', 'engineer', 'debug', 'crawl', 'agent_create'])
  })

  it('maps durable active and pending control snapshots into a selected pending projection', () => {
    const parsed = runtimeRunSchema.parse(makeRuntimeRunDto())
    const view = mapRuntimeRun(parsed)

    expect(view.controls.active).toMatchObject({
      runtimeAgentId: 'engineer',
      agentDefinitionId: 'phase4_builder',
      agentDefinitionVersion: 3,
      modelId: 'openai/gpt-4.1-mini',
      thinkingEffort: 'medium',
      approvalMode: 'suggest',
      planModeRequired: true,
      revision: 1,
    })
    expect(view.controls.pending).toMatchObject({
      runtimeAgentId: 'engineer',
      agentDefinitionId: 'phase4_builder',
      agentDefinitionVersion: 4,
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
      agentDefinitionId: 'phase4_builder',
      agentDefinitionVersion: 4,
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
        agentDefinitionId: 'project_release_engineer',
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
        agentDefinitionId: 'project_release_engineer',
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
