import { describe, expect, it } from 'vitest'

import { createRuntimeStreamView, mergeRuntimeStreamEvent, runtimeStreamItemSchema } from './runtime-stream'

describe('runtime stream contracts', () => {
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
})
