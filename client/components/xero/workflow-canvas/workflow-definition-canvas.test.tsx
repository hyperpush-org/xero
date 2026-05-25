import type { ReactNode } from 'react'

import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import {
  WORKFLOW_TEMPLATE_DEFAULT_NODE_POSITIONS,
  instantiateBlankWorkflow,
  instantiateWorkflowTemplate,
} from '@/src/lib/xero-model/workflow-templates'
import type { WorkflowDefinitionDto } from '@/src/lib/xero-model/workflow-definition'
import type { WorkflowRunDto } from '@/src/lib/xero-model/workflow-run'

const fitViewSpy = vi.hoisted(() => vi.fn())
const updateNodeInternalsSpy = vi.hoisted(() => vi.fn())

vi.mock('@xyflow/react', async () => {
  const React = await vi.importActual<typeof import('react')>('react')
  return {
  ConnectionMode: { Loose: 'loose', Strict: 'strict' },
  ControlButton: ({
    children,
    ...props
  }: {
    children: ReactNode
    [key: string]: unknown
  }) => <button type="button" {...props}>{children}</button>,
  Controls: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  Handle: ({
    id,
    type,
    position,
  }: {
    id?: string
    type?: string
    position?: string
  }) => (
    <span
      data-testid="workflow-handle"
      data-handle-id={id ?? ''}
      data-handle-type={type ?? ''}
      data-handle-position={position ?? ''}
    />
  ),
  MarkerType: { ArrowClosed: 'arrowclosed' },
  Position: { Top: 'top', Right: 'right', Bottom: 'bottom', Left: 'left' },
  ReactFlowProvider: ({ children }: { children: ReactNode }) => <>{children}</>,
  ReactFlow: ({
    children,
    edges = [],
    nodes,
    fitViewOptions,
    onNodeClick,
    onNodesChange,
  }: {
    children: ReactNode
    edges?: Array<{
      id: string
      source?: string
      target?: string
      className?: string
      selectable?: boolean
      data?: {
        edgeColor?: string
        labelBorderColor?: string
        labelTextColor?: string
        label?: string
        targetClearance?: number
        visualOnly?: boolean
      }
      markerEnd?: { color?: string; type?: string; width?: number; height?: number } | string
      sourceHandle?: string | null
      targetHandle?: string | null
    }>
    nodes: Array<{ id: string; position?: { x: number; y: number } }>
    fitViewOptions?: { maxZoom?: number }
    onNodeClick?: (event: unknown, node: { id: string }) => void
    onNodesChange?: (changes: Array<{ id: string; type: 'position'; position: { x: number; y: number } }>) => void
  }) => (
    <div
      data-testid="workflow-react-flow"
      data-node-count={nodes.length}
      data-fit-max-zoom={fitViewOptions?.maxZoom ?? ''}
    >
      {nodes.map((node) => (
        <button
          key={node.id}
          type="button"
          data-testid={`workflow-node-${node.id}`}
          data-position-x={node.position?.x ?? ''}
          data-position-y={node.position?.y ?? ''}
          onClick={(event) => onNodeClick?.(event, node)}
        >
          {node.id}
        </button>
      ))}
      {nodes[0] ? (
        <button
          type="button"
          data-testid="simulate-workflow-node-move"
          onClick={() =>
            onNodesChange?.([
              {
                id: nodes[0].id,
                type: 'position',
                position: { x: 440, y: 777 },
              },
            ])
          }
        >
          move node
        </button>
      ) : null}
      {edges.map((edge) => (
        <span
          key={edge.id}
          data-testid={`workflow-edge-${edge.id}`}
          data-source={edge.source ?? ''}
          data-target={edge.target ?? ''}
          data-class-name={edge.className ?? ''}
          data-selectable={String(edge.selectable ?? true)}
          data-marker-end={typeof edge.markerEnd === 'string' ? edge.markerEnd : edge.markerEnd?.type ?? ''}
          data-marker-width={typeof edge.markerEnd === 'string' ? '' : edge.markerEnd?.width ?? ''}
          data-marker-height={typeof edge.markerEnd === 'string' ? '' : edge.markerEnd?.height ?? ''}
          data-marker-color={typeof edge.markerEnd === 'string' ? '' : edge.markerEnd?.color ?? ''}
          data-edge-color={edge.data?.edgeColor ?? ''}
          data-label={edge.data?.label ?? ''}
          data-label-border-color={edge.data?.labelBorderColor ?? ''}
          data-label-text-color={edge.data?.labelTextColor ?? ''}
          data-target-clearance={edge.data?.targetClearance ?? ''}
          data-visual-only={String(edge.data?.visualOnly ?? false)}
          data-source-handle={edge.sourceHandle ?? ''}
          data-target-handle={edge.targetHandle ?? ''}
        />
      ))}
      {children}
    </div>
  ),
  useReactFlow: () => ({
    fitView: fitViewSpy,
    getViewport: () => ({ x: 0, y: 0, zoom: 1 }),
    zoomIn: vi.fn(),
    zoomOut: vi.fn(),
  }),
  useUpdateNodeInternals: () => updateNodeInternalsSpy,
  applyNodeChanges: <NodeType extends { id: string; position?: { x: number; y: number }; selected?: boolean }>(
    changes: Array<{ id: string; type: string; position?: { x: number; y: number }; selected?: boolean }>,
    nodes: NodeType[],
  ) =>
    nodes
      .filter((node) => !changes.some((change) => change.type === 'remove' && change.id === node.id))
      .map((node) => {
        const change = changes.find((candidate) => candidate.id === node.id)
        if (!change) return node
        if (change.type === 'position' && change.position) {
          return { ...node, position: change.position }
        }
        if (change.type === 'select') {
          return { ...node, selected: change.selected ?? false }
        }
        return node
      }),
  useNodesState: <NodeType,>(initialNodes: NodeType[]) => React.useState(initialNodes),
  useOnViewportChange: () => undefined,
  }
})

import { WorkflowDefinitionCanvas } from './workflow-definition-canvas'

describe('WorkflowDefinitionCanvas', () => {
  it('opens blank workflow drafts as an empty canvas with full-size fit zoom', () => {
    const definition = instantiateBlankWorkflow({ projectId: 'project-1' })

    render(
      <WorkflowDefinitionCanvas
        definition={definition}
        initialMode="edit"
        isCreating
        onSaveDefinition={vi.fn()}
      />,
    )

    expect(screen.getByTestId('workflow-react-flow')).toHaveAttribute('data-node-count', '0')
    expect(screen.getByTestId('workflow-react-flow')).toHaveAttribute('data-fit-max-zoom', '1')
    expect(screen.queryByText('Done')).toBeNull()
    expect(screen.queryByText('Workflow Health')).toBeNull()
    const emptyState = screen.getByRole('region', { name: 'Blank workflow start' })
    expect(within(emptyState).getByText('Start with an agent')).toBeInTheDocument()
    expect(within(emptyState).getByRole('button', { name: 'Add first agent step' })).toBeInTheDocument()
    expect(within(emptyState).queryByRole('button', { name: 'Add router' })).toBeNull()
  })

  it('makes the first blank-canvas node the workflow start node', async () => {
    const onCanvasStatusChange = vi.fn()
    const definition = instantiateBlankWorkflow({ projectId: 'project-1' })

    render(
      <WorkflowDefinitionCanvas
        definition={definition}
        initialMode="edit"
        isCreating
        onSaveDefinition={vi.fn()}
        onCanvasStatusChange={onCanvasStatusChange}
      />,
    )

    const addAgentButton = screen.getByRole('button', { name: 'Add first agent step' })
    expect(addAgentButton).not.toBeNull()
    fireEvent.click(addAgentButton as HTMLButtonElement)

    await waitFor(() =>
      expect(screen.getByTestId('workflow-react-flow')).toHaveAttribute('data-node-count', '1'),
    )
    await waitFor(() => {
      const latestStatus = onCanvasStatusChange.mock.calls
        .map((call) => call[0])
        .filter(Boolean)
        .at(-1)
      expect(latestStatus.definition.startNodeId).toBe('agent')
    })
  })

  it('keeps live drag moves off the workflow definition status path', async () => {
    const onCanvasStatusChange = vi.fn()
    const definition = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'continuous_delivery',
    })

    render(
      <WorkflowDefinitionCanvas
        definition={definition}
        initialMode="edit"
        onSaveDefinition={vi.fn()}
        onCanvasStatusChange={onCanvasStatusChange}
      />,
    )

    await waitFor(() => expect(onCanvasStatusChange).toHaveBeenCalled())
    await waitFor(() => expect(updateNodeInternalsSpy).toHaveBeenCalled())
    await new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()))
    updateNodeInternalsSpy.mockClear()
    const initialStatusCalls = onCanvasStatusChange.mock.calls.length
    expect(screen.getByTestId('workflow-edge-goal_to_plan')).toHaveAttribute(
      'data-source-handle',
      'workflow-right-success',
    )

    fireEvent.click(screen.getByTestId('simulate-workflow-node-move'))

    await waitFor(() => {
      expect(screen.getByTestId('workflow-node-goal_intake')).toHaveAttribute('data-position-x', '440')
    })
    expect(screen.getByTestId('workflow-node-goal_intake')).toHaveAttribute('data-position-y', '777')
    expect(screen.getByTestId('workflow-edge-goal_to_plan')).toHaveAttribute(
      'data-source-handle',
      'workflow-top-success',
    )
    expect(screen.getByTestId('workflow-edge-goal_to_plan')).toHaveAttribute(
      'data-target-handle',
      'workflow-bottom-success',
    )
    await waitFor(() => expect(updateNodeInternalsSpy).toHaveBeenCalled())
    expect(updateNodeInternalsSpy).toHaveBeenCalledWith(expect.arrayContaining(['goal_intake', 'plan']))
    expect(onCanvasStatusChange).toHaveBeenCalledTimes(initialStatusCalls)
  })

  it('routes workflow edges through the nearest facing node sides', () => {
    const definition = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'continuous_delivery',
    })

    const view = render(<WorkflowDefinitionCanvas definition={definition} />)

    expect(screen.getByTestId('workflow-edge-plan_to_work')).toHaveAttribute(
      'data-source-handle',
      'workflow-right-success',
    )
    expect(screen.getByTestId('workflow-edge-plan_to_work')).toHaveAttribute(
      'data-target-handle',
      'workflow-left-success',
    )
    expect(screen.getByTestId('workflow-edge-plan_to_work')).toHaveAttribute('data-marker-end', 'arrowclosed')
    expect(screen.getByTestId('workflow-edge-plan_to_work')).toHaveAttribute('data-marker-width', '18')
    expect(screen.getByTestId('workflow-edge-plan_to_work')).toHaveAttribute('data-marker-height', '18')
    expect(screen.getByTestId('workflow-edge-plan_to_work')).toHaveAttribute('data-target-clearance', '0')
    expect(screen.getByTestId('workflow-edge-plan_to_work').getAttribute('data-edge-color')).toContain('--color-emerald-500')
    expect(screen.getByTestId('workflow-edge-plan_to_work').getAttribute('data-marker-color')).toContain('--color-emerald-500')
    expect(screen.getByTestId('workflow-edge-verification_passed').getAttribute('data-edge-color')).toContain('--color-sky-500')
    expect(screen.getByTestId('workflow-edge-verification_passed').getAttribute('data-marker-color')).toContain('--color-sky-500')
    expect(screen.getByTestId('workflow-edge-verification_passed').getAttribute('data-label-border-color')).toContain('--color-sky-500')
    expect(screen.getByTestId('workflow-edge-verification_passed').getAttribute('data-label-text-color')).toContain('--color-sky-500')
    expect(screen.getByTestId('workflow-edge-work_failed_to_debug')).toHaveAttribute(
      'data-source-handle',
      'workflow-bottom-recovery',
    )
    expect(screen.getByTestId('workflow-edge-work_failed_to_debug')).toHaveAttribute(
      'data-target-handle',
      'workflow-top-recovery',
    )
    expect(screen.getByTestId('workflow-edge-debug_to_work')).toHaveAttribute(
      'data-source-handle',
      'workflow-top-loop',
    )
    expect(screen.getByTestId('workflow-edge-debug_to_work')).toHaveAttribute(
      'data-target-handle',
      'workflow-bottom-loop',
    )
    expect(screen.getByTestId('workflow-edge-verification_gaps')).toHaveAttribute(
      'data-source-handle',
      'workflow-bottom-conditional',
    )
    expect(screen.getByTestId('workflow-edge-verification_gaps')).toHaveAttribute(
      'data-target-handle',
      'workflow-top-conditional',
    )
    expect(screen.getByTestId('workflow-edge-gap_back_to_work__exhausted')).toHaveAttribute(
      'data-source',
      'gap_closure',
    )
    expect(screen.getByTestId('workflow-edge-gap_back_to_work__exhausted')).toHaveAttribute(
      'data-target',
      'human_verify',
    )
    expect(screen.getByTestId('workflow-edge-gap_back_to_work__exhausted')).toHaveAttribute(
      'data-label',
      'exhausted',
    )
    expect(screen.getByTestId('workflow-edge-gap_back_to_work__exhausted')).toHaveAttribute(
      'data-visual-only',
      'true',
    )
    expect(screen.getByTestId('workflow-edge-gap_back_to_work__exhausted')).toHaveAttribute(
      'data-selectable',
      'false',
    )
    expect(screen.getByTestId('workflow-edge-gap_back_to_work__exhausted').getAttribute('data-class-name')).toContain('workflow-definition-edge--loop')
    expect(screen.getByTestId('workflow-edge-debug_to_work__exhausted')).toHaveAttribute(
      'data-target',
      'human_verify',
    )
    expect(screen.getByTestId('workflow-edge-fix_back_to_review__exhausted')).toHaveAttribute(
      'data-target',
      'human_verify',
    )

    view.unmount()

    render(
      <WorkflowDefinitionCanvas
        definition={definition}
        initialMode="edit"
        onSaveDefinition={vi.fn()}
      />,
    )

    expect(screen.getByTestId('workflow-edge-plan_to_work')).toHaveAttribute('data-target-clearance', '15')
  })

  it('opens workflow definitions in the same compact layout produced by reset', () => {
    const definition = distortWorkflowPositions(
      instantiateWorkflowTemplate({
        projectId: 'project-1',
        templateId: 'gsd_auto',
      }),
    )

    const { rerender } = render(<WorkflowDefinitionCanvas definition={definition} />)
    const openedPositions = workflowNodePositions([
      'load_milestones',
      'phase_router',
      'success',
      'needs_human',
    ])
    const gsdDefaults = WORKFLOW_TEMPLATE_DEFAULT_NODE_POSITIONS.gsd_auto

    expect(openedPositions).toEqual({
      load_milestones: gsdDefaults.load_milestones,
      phase_router: gsdDefaults.phase_router,
      success: gsdDefaults.success,
      needs_human: gsdDefaults.needs_human,
    })

    rerender(
      <WorkflowDefinitionCanvas
        definition={definition}
        initialMode="edit"
        onSaveDefinition={vi.fn()}
      />,
    )
    const resetButton = screen.getByLabelText('Reset layout')
    const editableInitialPositions = workflowNodePositions([
      'load_milestones',
      'phase_router',
      'success',
      'needs_human',
    ])

    fireEvent.click(resetButton)

    expect(workflowNodePositions([
      'load_milestones',
      'phase_router',
      'success',
      'needs_human',
    ])).toEqual(editableInitialPositions)
    expect(editableInitialPositions).toEqual(openedPositions)
  })

  it('shows workflow configuration details when inspecting a readonly node', () => {
    const definition = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'gsd_auto',
    })

    render(<WorkflowDefinitionCanvas definition={definition} />)

    fireEvent.click(screen.getByTestId('workflow-node-next_phase'))

    expect(screen.getByText('Node ID')).toBeInTheDocument()
    expect(screen.getAllByText('next_phase').length).toBeGreaterThan(1)
    expect(screen.getByText('Filters')).toBeInTheDocument()
    expect(screen.getByText(/\$\.status not in complete, archived/)).toBeInTheDocument()
    expect(screen.getByText('Window controls')).toBeInTheDocument()
    expect(screen.getByText(/only <- \$\.only/)).toBeInTheDocument()
    expect(screen.getByText(/from <- \$\.from/)).toBeInTheDocument()
    expect(screen.getByText(/to <- \$\.to/)).toBeInTheDocument()
    expect(screen.getByText('Connections')).toBeInTheDocument()
    expect(screen.getByText('Query incomplete phases -> select')).toBeInTheDocument()
    expect(screen.getByText('route -> Phase route')).toBeInTheDocument()
  })

  it('wraps long workflow binding summaries in the properties panel', () => {
    const definition = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'gsd_auto',
    })

    render(
      <WorkflowDefinitionCanvas
        definition={definition}
        initialMode="edit"
        onSaveDefinition={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByTestId('workflow-node-archive_milestone'))

    const longBindingSummaries = screen.getAllByText(
      /Archive Milestone state for \{\{state\.reload_milestones/,
    )
    expect(longBindingSummaries.length).toBeGreaterThanOrEqual(2)
    for (const summary of longBindingSummaries) {
      expect(summary.className).toContain('[overflow-wrap:anywhere]')
    }
  })

  it('marks incoming and outgoing workflow edges as related when selecting a node', () => {
    const definition = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'gsd_auto',
    })

    render(<WorkflowDefinitionCanvas definition={definition} />)

    fireEvent.click(screen.getByTestId('workflow-node-project_ideation'))

    expect(screen.getByTestId('workflow-edge-milestone_missing').getAttribute('data-class-name')).toContain(
      'workflow-definition-edge--related',
    )
    expect(screen.getByTestId('workflow-edge-ideation_to_requirements').getAttribute('data-class-name')).toContain(
      'workflow-definition-edge--related',
    )
    expect(screen.getByTestId('workflow-edge-ideation_failed').getAttribute('data-class-name')).toContain(
      'workflow-definition-edge--related',
    )
    expect(screen.getByTestId('workflow-edge-requirements_to_roadmap').getAttribute('data-class-name')).not.toContain(
      'workflow-definition-edge--related',
    )
  })

  it('collects required run input before starting a workflow', async () => {
    const onCanvasStatusChange = vi.fn()
    const onStartRun = vi.fn(async () => undefined)
    const definition = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'gsd_auto',
    })

    render(
      <WorkflowDefinitionCanvas
        definition={definition}
        onCanvasStatusChange={onCanvasStatusChange}
        onStartRun={onStartRun}
      />,
    )

    await waitFor(() => {
      const latestStatus = onCanvasStatusChange.mock.calls
        .map((call) => call[0])
        .filter(Boolean)
        .at(-1)
      expect(latestStatus).toBeTruthy()
      latestStatus.start()
    })

    const startButton = screen.getByRole('button', { name: 'Start' })
    expect(startButton).toBeDisabled()
    expect(screen.getByLabelText('Only phase (optional)')).toBeInTheDocument()
    expect(screen.getByLabelText('From phase (optional)')).toBeInTheDocument()
    expect(screen.getByLabelText('To phase (optional)')).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Goal'), {
      target: { value: 'Ship canvas parity' },
    })
    fireEvent.change(screen.getByLabelText('Only phase (optional)'), {
      target: { value: '2' },
    })
    expect(startButton).not.toBeDisabled()
    fireEvent.click(startButton)

    await waitFor(() =>
      expect(onStartRun).toHaveBeenCalledWith(definition.id, {
        goal: 'Ship canvas parity',
        only: '2',
      }),
    )
  })

  it('shows child node runs when inspecting a subgraph node', () => {
    const now = '2026-01-01T00:00:00Z'
    const definition = workflowWithSubgraph()
    const run: WorkflowRunDto = {
      id: 'run-1',
      projectId: 'project-1',
      workflowVersionId: 'workflow-version-1',
      workflowId: definition.id,
      workflowVersionNumber: 1,
      status: 'running',
      terminalStatus: null,
      definitionSnapshot: definition,
      initialInput: null,
      startedAt: now,
      updatedAt: now,
      completedAt: null,
      cancellationReason: null,
      nodes: [
        workflowRunNode('run-1:node:invoke:attempt:0', 'invoke', 'subgraph', 'running', 0, now),
        workflowRunNode('run-1:node:invoke::plan:attempt:0', 'invoke::plan', 'agent', 'succeeded', 0, now),
        workflowRunNode('run-1:node:invoke::verify:attempt:0', 'invoke::verify', 'command', 'failed', 0, now, 'workflow_command_failed'),
      ],
      edgeDecisions: [],
      artifacts: [
        {
          id: 'artifact-verify',
          workflowRunId: 'run-1',
          producerNodeRunId: 'run-1:node:invoke::verify:attempt:0',
          artifactType: 'command_result',
          schemaVersion: 1,
          payload: { status: 'failed', stdout: '', stderr: 'check failed' },
          renderText: 'check failed',
          createdAt: now,
        },
      ],
      gateDecisions: [],
      loopAttempts: [],
      events: [],
    }

    render(<WorkflowDefinitionCanvas definition={definition} run={run} />)

    fireEvent.click(screen.getByTestId('workflow-node-invoke'))

    expect(screen.getByText('Subgraph Runs')).toBeInTheDocument()
    expect(screen.getByText('plan')).toBeInTheDocument()
    expect(screen.getByText('verify')).toBeInTheDocument()
    expect(screen.getByText('workflow_command_failed')).toBeInTheDocument()
    expect(screen.getByText('check failed')).toBeInTheDocument()
  })

  it('exposes blocker explanation and support bundle recovery actions for failed runs', async () => {
    const now = '2026-01-01T00:00:00Z'
    const definition = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'gsd_auto',
    })
    const run: WorkflowRunDto = {
      id: 'run-1',
      projectId: 'project-1',
      workflowVersionId: 'workflow-version-1',
      workflowId: definition.id,
      workflowVersionNumber: 1,
      status: 'failed',
      terminalStatus: 'failure',
      definitionSnapshot: definition,
      initialInput: null,
      startedAt: now,
      updatedAt: now,
      completedAt: now,
      cancellationReason: null,
      nodes: [
        workflowRunNode('run-1:node:process_phase_flow:attempt:0', 'process_phase_flow', 'subgraph', 'failed', 0, now, 'workflow_subgraph_failed'),
      ],
      edgeDecisions: [],
      artifacts: [],
      gateDecisions: [],
      loopAttempts: [],
      events: [],
    }
    const onExplainRunBlocker = vi.fn(async () => ({
      status: 'failed',
      summary: 'Subgraph failed during verification.',
      nodeId: 'process_phase_flow',
      nodeRunId: 'run-1:node:process_phase_flow:attempt:0',
      failureClass: 'workflow_subgraph_failed',
      event: { nodeId: 'process_phase_flow' },
    }))
    const onExportRunBundle = vi.fn(async () => ({
      bundle: {
        schema: 'xero.workflow_run_bundle.v1',
        runId: 'run-1',
        blocker: { summary: 'Subgraph failed during verification.' },
        run: {
          status: 'failed',
          nodes: run.nodes,
          events: Array.from({ length: 250 }, (_, index) => ({ id: `event-${index}` })),
          artifacts: [],
          edgeDecisions: [],
          gateDecisions: [],
          loopAttempts: [],
        },
        deliveryState: {
          delivery_phase: [{ id: 'phase-2' }],
        },
      },
    }))
    const onResumeNextIncompletePhase = vi.fn(async () => ({
      ...run,
      id: 'run-2',
      status: 'running' as const,
      terminalStatus: null,
      completedAt: null,
      initialInput: { goal: 'ship', from: '2' },
    }))

    render(
      <WorkflowDefinitionCanvas
        definition={definition}
        run={run}
        onExplainRunBlocker={onExplainRunBlocker}
        onExportRunBundle={onExportRunBundle}
        onResumeNextIncompletePhase={onResumeNextIncompletePhase}
      />,
    )

    expect(screen.getByText('Run recovery')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /Explain/ }))

    await waitFor(() => expect(onExplainRunBlocker).toHaveBeenCalledWith('run-1'))
    expect(await screen.findByText('Subgraph failed during verification.')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Bundle/ }))

    await waitFor(() => expect(onExportRunBundle).toHaveBeenCalledWith('run-1'))
    expect(await screen.findByText('Support bundle')).toBeInTheDocument()
    expect(screen.getByText(/xero.workflow_run_bundle.v1/)).toBeInTheDocument()
    expect(screen.getByText(/eventCount/)).toBeInTheDocument()
    expect(screen.getByText(/250/)).toBeInTheDocument()
    expect(screen.queryByText(/event-249/)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /Resume phase/ }))

    await waitFor(() => expect(onResumeNextIncompletePhase).toHaveBeenCalledWith('run-1'))
    expect(await screen.findByText('Resume scheduled')).toBeInTheDocument()
  })
})

function workflowWithSubgraph(): WorkflowDefinitionDto {
  const definition = instantiateBlankWorkflow({ projectId: 'project-1' })
  const outputContract = {
    artifactType: 'subgraph_result',
    schemaVersion: 1,
    extraction: 'json_object' as const,
    required: true,
    renderTextPath: '$.summary',
  }
  return {
    ...definition,
    id: 'workflow-subgraph',
    name: 'Subgraph workflow',
    startNodeId: 'invoke',
    nodes: [
      {
        id: 'invoke',
        title: 'Invoke phase',
        description: '',
        position: { x: 0, y: 0 },
        type: 'subgraph',
        subgraphId: 'phase_flow',
        inputBindings: [],
        outputContract,
      },
    ],
    subgraphs: [
      {
        id: 'phase_flow',
        title: 'Phase flow',
        description: '',
        startNodeId: 'plan',
        inputBindings: [],
        outputContract,
        nodes: [
          {
            id: 'plan',
            title: 'Plan',
            description: '',
            position: { x: 0, y: 0 },
            type: 'agent',
            agentRef: { kind: 'built_in', runtimeAgentId: 'engineer', version: 2 },
            displayLabel: null,
            inputBindings: [],
            outputContract: {
              artifactType: 'text_output',
              schemaVersion: 1,
              extraction: 'generic_text',
              required: true,
              renderTextPath: null,
            },
            runOverrides: null,
            resourceScopes: [],
            failurePolicy: {
              quotaFailureClasses: [],
              transientFailureClasses: [],
            },
          },
          {
            id: 'verify',
            title: 'Verify',
            description: '',
            position: { x: 180, y: 0 },
            type: 'command',
            command: 'git',
            args: ['status', '--short'],
            allowedCommands: ['git'],
            workingDirectory: null,
            timeoutSeconds: 120,
            successExitCodes: [0],
            outputContract: {
              artifactType: 'command_result',
              schemaVersion: 1,
              extraction: 'json_object',
              required: true,
              renderTextPath: '$.stdout',
            },
            parser: {
              extraction: 'generic_text',
              renderTextPath: '$.stdout',
            },
          },
        ],
        edges: [],
      },
    ],
  }
}

function workflowRunNode(
  id: string,
  nodeId: string,
  nodeType: string,
  status: WorkflowRunDto['nodes'][number]['status'],
  attemptNumber: number,
  now: string,
  failureClass?: string,
): WorkflowRunDto['nodes'][number] {
  return {
    id,
    workflowRunId: 'run-1',
    nodeId,
    nodeType,
    status,
    attemptNumber,
    runtimeRunId: null,
    agentSessionId: null,
    failureClass: failureClass ?? null,
    startedAt: now,
    updatedAt: now,
    completedAt: status === 'running' ? null : now,
    idempotencyKey: `${id}:idempotency`,
  }
}

function distortWorkflowPositions(definition: WorkflowDefinitionDto): WorkflowDefinitionDto {
  return {
    ...definition,
    nodes: definition.nodes.map((node, index) => ({
      ...node,
      position: node.id === 'success'
        ? { x: 9000, y: -1200 }
        : { x: 9000 - index * 37, y: -900 + index * 91 },
    })),
  }
}

function workflowNodePositions<T extends string>(nodeIds: readonly T[]): Record<T, { x: number; y: number }> {
  return Object.fromEntries(
    nodeIds.map((nodeId) => {
      const element = screen.getByTestId(`workflow-node-${nodeId}`)
      return [
        nodeId,
        {
          x: Number(element.getAttribute('data-position-x')),
          y: Number(element.getAttribute('data-position-y')),
        },
      ]
    }),
  ) as Record<T, { x: number; y: number }>
}
