import { describe, expect, it } from 'vitest'

import {
  activeWorkflowAgentSessionId,
  buildWorkflowRunProgress,
  isTerminalWorkflowRunStatus,
  latestWorkflowRunNodesByNodeId,
  pickActiveWorkflowRun,
  workflowRunSessionIds,
} from '@/src/features/xero/workflow-run-selectors'
import {
  workflowDefinitionSchema,
  type WorkflowDefinitionDto,
  type WorkflowNodeRunStatusDto,
  type WorkflowRunStatusDto,
} from '@/src/lib/xero-model/workflow-definition'
import type { WorkflowRunDto, WorkflowRunNodeDto } from '@/src/lib/xero-model/workflow-run'

function makeDefinition(): WorkflowDefinitionDto {
  return workflowDefinitionSchema.parse({
    id: 'wf_ship',
    projectId: 'project-1',
    name: 'Ship feature',
    description: '',
    startNodeId: 'plan',
    nodes: [
      {
        id: 'plan',
        type: 'agent',
        title: 'Plan work',
        description: '',
        agentRef: { kind: 'built_in', runtimeAgentId: 'generalist', version: 1 },
      },
      {
        id: 'review',
        type: 'human_checkpoint',
        title: 'Review plan',
        description: '',
        checkpointType: 'decision',
        prompt: 'Approve the plan?',
        decisionOptions: ['approve', 'revise'],
      },
      {
        id: 'done',
        type: 'terminal',
        title: 'Done',
        description: '',
        terminalStatus: 'success',
      },
    ],
    edges: [
      { id: 'edge_plan_review', fromNodeId: 'plan', toNodeId: 'review', type: 'success' },
      { id: 'edge_review_done', fromNodeId: 'review', toNodeId: 'done', type: 'success' },
    ],
  })
}

let nodeRunSequence = 0

function makeRunNode(
  runId: string,
  nodeId: string,
  status: WorkflowNodeRunStatusDto,
  overrides: Partial<WorkflowRunNodeDto> = {},
): WorkflowRunNodeDto {
  nodeRunSequence += 1
  return {
    id: `${runId}:node:${nodeId}:${nodeRunSequence}`,
    workflowRunId: runId,
    nodeId,
    nodeType: 'agent',
    status,
    attemptNumber: 1,
    runtimeRunId: null,
    agentSessionId: null,
    failureClass: null,
    startedAt: '2026-07-19T10:00:00Z',
    updatedAt: '2026-07-19T10:00:00Z',
    completedAt: null,
    idempotencyKey: `${runId}:${nodeId}:${nodeRunSequence}`,
    ...overrides,
  }
}

function makeRun(
  id: string,
  status: WorkflowRunStatusDto,
  overrides: Partial<WorkflowRunDto> = {},
): WorkflowRunDto {
  const definition = makeDefinition()
  return {
    id,
    projectId: 'project-1',
    workflowVersionId: `${definition.id}:version:1`,
    workflowId: definition.id,
    workflowVersionNumber: 1,
    status,
    terminalStatus: null,
    definitionSnapshot: definition,
    initialInput: null,
    startedAt: '2026-07-19T10:00:00Z',
    updatedAt: '2026-07-19T10:00:00Z',
    completedAt: null,
    cancellationReason: null,
    nodes: [],
    edgeDecisions: [],
    artifacts: [],
    gateDecisions: [],
    loopAttempts: [],
    events: [],
    ...overrides,
  }
}

describe('isTerminalWorkflowRunStatus', () => {
  it('classifies statuses', () => {
    expect(isTerminalWorkflowRunStatus('completed')).toBe(true)
    expect(isTerminalWorkflowRunStatus('failed')).toBe(true)
    expect(isTerminalWorkflowRunStatus('cancelled')).toBe(true)
    expect(isTerminalWorkflowRunStatus('running')).toBe(false)
    expect(isTerminalWorkflowRunStatus('paused')).toBe(false)
    expect(isTerminalWorkflowRunStatus('queued')).toBe(false)
    expect(isTerminalWorkflowRunStatus('cancelling')).toBe(false)
  })
})

describe('pickActiveWorkflowRun', () => {
  it('returns null when every run settled', () => {
    expect(
      pickActiveWorkflowRun([makeRun('run-1', 'completed'), makeRun('run-2', 'failed')]),
    ).toBeNull()
  })

  it('picks the most recently started non-terminal run', () => {
    const older = makeRun('run-1', 'running', { startedAt: '2026-07-19T09:00:00Z' })
    const newer = makeRun('run-2', 'paused', { startedAt: '2026-07-19T11:00:00Z' })
    const settled = makeRun('run-3', 'completed', { startedAt: '2026-07-19T12:00:00Z' })
    expect(pickActiveWorkflowRun([older, settled, newer])?.id).toBe('run-2')
  })
})

describe('latestWorkflowRunNodesByNodeId', () => {
  it('keeps the highest attempt per definition node', () => {
    const run = makeRun('run-1', 'running')
    run.nodes = [
      makeRunNode('run-1', 'plan', 'failed', { attemptNumber: 1 }),
      makeRunNode('run-1', 'plan', 'running', { attemptNumber: 2 }),
      makeRunNode('run-1', 'review', 'pending'),
    ]
    const byNodeId = latestWorkflowRunNodesByNodeId(run)
    expect(byNodeId.get('plan')?.status).toBe('running')
    expect(byNodeId.get('review')?.status).toBe('pending')
  })
})

describe('activeWorkflowAgentSessionId', () => {
  it('prefers the running node session over waiting and settled ones', () => {
    const run = makeRun('run-1', 'running')
    run.nodes = [
      makeRunNode('run-1', 'plan', 'succeeded', {
        agentSessionId: 'session-plan',
        updatedAt: '2026-07-19T12:00:00Z',
      }),
      makeRunNode('run-1', 'review', 'running', {
        agentSessionId: 'session-review',
        updatedAt: '2026-07-19T10:00:00Z',
      }),
    ]
    expect(activeWorkflowAgentSessionId(run)).toBe('session-review')
  })

  it('falls back to the most recently updated node with a session', () => {
    const run = makeRun('run-1', 'paused')
    run.nodes = [
      makeRunNode('run-1', 'plan', 'succeeded', {
        agentSessionId: 'session-old',
        updatedAt: '2026-07-19T09:00:00Z',
      }),
      makeRunNode('run-1', 'review', 'failed', {
        agentSessionId: 'session-new',
        updatedAt: '2026-07-19T11:00:00Z',
      }),
      makeRunNode('run-1', 'done', 'pending'),
    ]
    expect(activeWorkflowAgentSessionId(run)).toBe('session-new')
  })

  it('returns null when no node has a session', () => {
    const run = makeRun('run-1', 'running')
    run.nodes = [makeRunNode('run-1', 'plan', 'running')]
    expect(activeWorkflowAgentSessionId(run)).toBeNull()
  })
})

describe('workflowRunSessionIds', () => {
  it('collects every agent session attached to the run', () => {
    const run = makeRun('run-1', 'running')
    run.nodes = [
      makeRunNode('run-1', 'plan', 'succeeded', { agentSessionId: 'session-a' }),
      makeRunNode('run-1', 'review', 'running', { agentSessionId: 'session-b' }),
      makeRunNode('run-1', 'done', 'pending'),
    ]
    expect([...workflowRunSessionIds(run)].sort()).toEqual(['session-a', 'session-b'])
  })
})

describe('buildWorkflowRunProgress', () => {
  it('joins definition order with latest node-run status', () => {
    const run = makeRun('run-1', 'running')
    run.nodes = [
      makeRunNode('run-1', 'plan', 'succeeded'),
      makeRunNode('run-1', 'review', 'running'),
    ]
    const progress = buildWorkflowRunProgress(run)
    expect(progress.entries.map((entry) => entry.node.id)).toEqual([
      'plan',
      'review',
      'done',
    ])
    expect(progress.entries.map((entry) => entry.status)).toEqual([
      'succeeded',
      'running',
      'pending',
    ])
    expect(progress.totalCount).toBe(3)
    expect(progress.settledCount).toBe(1)
    expect(progress.activeEntry?.node.id).toBe('review')
    expect(progress.waitingEntry).toBeNull()
  })

  it('surfaces the waiting checkpoint entry', () => {
    const run = makeRun('run-1', 'paused')
    run.nodes = [
      makeRunNode('run-1', 'plan', 'succeeded'),
      makeRunNode('run-1', 'review', 'waiting_on_gate'),
    ]
    const progress = buildWorkflowRunProgress(run)
    expect(progress.waitingEntry?.node.id).toBe('review')
    expect(progress.activeEntry?.node.id).toBe('review')
  })
})
