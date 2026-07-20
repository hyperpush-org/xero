import { fireEvent, render, renderHook, screen, act } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import {
  WorkflowRunFloatingPanel,
  useChatWorkflowRunPanel,
} from '@/components/xero/workflow-run-panel'
import {
  workflowDefinitionSchema,
  type WorkflowDefinitionDto,
  type WorkflowNodeRunStatusDto,
  type WorkflowRunStatusDto,
} from '@/src/lib/xero-model/workflow-definition'
import type { WorkflowRunDto, WorkflowRunNodeDto } from '@/src/lib/xero-model/workflow-run'

function makeDefinition(
  overrides: { reviewResumePayloadSchema?: Record<string, unknown> | null } = {},
): WorkflowDefinitionDto {
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
        resumePayloadSchema: overrides.reviewResumePayloadSchema ?? null,
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

describe('WorkflowRunFloatingPanel', () => {
  it('shows the run header, step counter, and per-node statuses', () => {
    const run = makeRun('run-1', 'running')
    run.nodes = [
      makeRunNode('run-1', 'plan', 'succeeded'),
      makeRunNode('run-1', 'review', 'running'),
    ]

    render(<WorkflowRunFloatingPanel run={run} />)

    expect(
      screen.getByRole('complementary', { name: 'Workflow run status' }),
    ).toBeInTheDocument()
    expect(screen.getByText('Ship feature')).toBeInTheDocument()
    expect(screen.getAllByText('Running').length).toBeGreaterThan(0)
    expect(screen.getByText('Step 2 / 3')).toBeInTheDocument()
    const steps = screen.getByRole('list', { name: 'Workflow steps' })
    expect(steps).toHaveTextContent('Plan work')
    expect(steps).toHaveTextContent('Review plan')
    expect(steps).toHaveTextContent('Done')
  })

  it('resumes a paused checkpoint with the chosen decision', async () => {
    const run = makeRun('run-1', 'paused')
    const waitingNode = makeRunNode('run-1', 'review', 'waiting_on_gate')
    run.nodes = [makeRunNode('run-1', 'plan', 'succeeded'), waitingNode]
    const onResumeCheckpoint = vi.fn(async () => undefined)

    render(<WorkflowRunFloatingPanel run={run} onResumeCheckpoint={onResumeCheckpoint} />)

    expect(screen.getByText('Approve the plan?')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Revise' }))
    expect(onResumeCheckpoint).toHaveBeenCalledWith('run-1', waitingNode.id, 'revise', null)
  })

  it('routes payload-requiring checkpoints to the canvas', () => {
    const definition = makeDefinition({
      reviewResumePayloadSchema: { type: 'object' },
    })
    const run = makeRun('run-1', 'paused', { definitionSnapshot: definition })
    run.nodes = [makeRunNode('run-1', 'review', 'waiting_on_gate')]
    const onOpenCanvas = vi.fn()
    const onResumeCheckpoint = vi.fn()

    render(
      <WorkflowRunFloatingPanel
        run={run}
        onOpenCanvas={onOpenCanvas}
        onResumeCheckpoint={onResumeCheckpoint}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Resume on canvas' }))
    expect(onOpenCanvas).toHaveBeenCalledWith('run-1')
    expect(onResumeCheckpoint).not.toHaveBeenCalled()
  })

  it('exposes canvas, cancel, and per-node chat actions', () => {
    const run = makeRun('run-1', 'running')
    run.nodes = [
      makeRunNode('run-1', 'plan', 'running', { agentSessionId: 'session-plan' }),
    ]
    const onOpenCanvas = vi.fn()
    const onCancelRun = vi.fn(async () => undefined)
    const onOpenAgentSession = vi.fn()

    render(
      <WorkflowRunFloatingPanel
        run={run}
        onOpenCanvas={onOpenCanvas}
        onCancelRun={onCancelRun}
        onOpenAgentSession={onOpenAgentSession}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Open canvas' }))
    expect(onOpenCanvas).toHaveBeenCalledWith('run-1')
    fireEvent.click(screen.getByRole('button', { name: 'Cancel run' }))
    expect(onCancelRun).toHaveBeenCalledWith('run-1')
    fireEvent.click(screen.getByRole('button', { name: 'Open chat for Plan work' }))
    expect(onOpenAgentSession).toHaveBeenCalledWith('session-plan')
  })

  it('hides cancel once the run settled and supports collapse and dismiss', () => {
    const run = makeRun('run-1', 'completed', { terminalStatus: 'success' })
    const onCancelRun = vi.fn()
    const onDismiss = vi.fn()

    render(
      <WorkflowRunFloatingPanel run={run} onCancelRun={onCancelRun} onDismiss={onDismiss} />,
    )

    expect(screen.queryByRole('button', { name: 'Cancel run' })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Collapse workflow run panel' }))
    expect(screen.queryByRole('list', { name: 'Workflow steps' })).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Expand workflow run panel' }))
    expect(screen.getByRole('list', { name: 'Workflow steps' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Hide workflow run panel' }))
    expect(onDismiss).toHaveBeenCalledTimes(1)
  })
})

describe('useChatWorkflowRunPanel', () => {
  it('follows the active run, keeps it after settling, and honors dismissal', () => {
    const running = makeRun('run-1', 'running')
    const { result, rerender } = renderHook(
      ({ runs }: { runs: WorkflowRunDto[] }) => useChatWorkflowRunPanel(runs),
      { initialProps: { runs: [running] } },
    )
    expect(result.current.run?.id).toBe('run-1')

    const settled = makeRun('run-1', 'completed', { terminalStatus: 'success' })
    rerender({ runs: [settled] })
    expect(result.current.run?.status).toBe('completed')

    act(() => result.current.dismiss())
    expect(result.current.run).toBeNull()

    const next = makeRun('run-2', 'running', { startedAt: '2026-07-19T12:00:00Z' })
    rerender({ runs: [settled, next] })
    expect(result.current.run?.id).toBe('run-2')
  })

  it('stays hidden when there was never an active run', () => {
    const { result } = renderHook(() =>
      useChatWorkflowRunPanel([makeRun('run-1', 'completed')]),
    )
    expect(result.current.run).toBeNull()
  })
})
