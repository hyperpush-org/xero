import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import { act, fireEvent, render, within } from '@testing-library/react'
import type { Node } from '@xyflow/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { WorkflowAgentDetailDto } from '@/src/lib/xero-model/workflow-agents'
import { WorkflowCanvasEmptyState } from '../workflow-canvas-empty-state'

const updateNodeInternalsSpy = vi.hoisted(() => vi.fn())
const fitViewSpy = vi.hoisted(() => vi.fn())
const setViewportSpy = vi.hoisted(() => vi.fn())
const getViewportSpy = vi.hoisted(() => vi.fn(() => ({ x: 0, y: 0, zoom: 1 })))

vi.mock('@xyflow/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@xyflow/react')>()
  return {
    ...actual,
    useReactFlow: () => ({
      fitView: fitViewSpy,
      setViewport: setViewportSpy,
      getViewport: getViewportSpy,
    }),
    useUpdateNodeInternals: () => updateNodeInternalsSpy,
  }
})

import {
  AgentVisualization,
  applyKnownNodeDimensions,
  applyLaneDragPositionChanges,
  estimateDbTableCardHeight,
  estimateExpandedCardHeight,
  getLaneDragMemberIds,
} from './agent-visualization'

const originalElementFromPoint = document.elementFromPoint
const AGENT_VISUALIZATION_CSS = readFileSync(
  resolve(process.cwd(), 'components/xero/workflow-canvas/agent-visualization.css'),
  'utf8',
)
const REACT_FLOW_CSS = readFileSync(
  resolve(process.cwd(), 'node_modules/@xyflow/react/dist/style.css'),
  'utf8',
)

function installResizeObserverStub() {
  if ((globalThis as { ResizeObserver?: unknown }).ResizeObserver) return
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  ;(globalThis as { ResizeObserver?: unknown }).ResizeObserver = ResizeObserverStub
}

function zIndexForSelector(css: string, selector: string): number {
  const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const match = css.match(new RegExp(`${escapedSelector}\\s*\\{[^}]*z-index:\\s*(-?\\d+)`, 'm'))
  expect(match).not.toBeNull()
  return Number.parseInt(match![1], 10)
}

afterEach(() => {
  vi.useRealTimers()
  vi.restoreAllMocks()
  updateNodeInternalsSpy.mockClear()
  fitViewSpy.mockClear()
  setViewportSpy.mockClear()
  getViewportSpy.mockClear()
  if (originalElementFromPoint) {
    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: originalElementFromPoint,
    })
  } else {
    delete (document as Partial<Document>).elementFromPoint
  }
})

function detail(): WorkflowAgentDetailDto {
  return {
    ref: { kind: 'built_in', runtimeAgentId: 'engineer', version: 1 },
    header: {
      displayName: 'Engineer',
      shortLabel: 'Build',
      description: 'Implements repository changes.',
      taskPurpose: 'Inspect, plan, edit, verify.',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'engineering',
      defaultApprovalMode: 'suggest',
      allowedApprovalModes: ['suggest'],
      allowPlanGate: true,
      allowVerificationGate: true,
      allowAutoCompact: true,
    },
    promptPolicy: 'engineer',
    toolPolicy: 'engineering',
    prompts: [
      {
        id: 'sys',
        label: 'System policy',
        role: 'system',
        policy: 'engineer',
        source: 'xero-runtime',
        body: 'You are Engineer.',
      },
    ],
    tools: [
      {
        name: 'Read',
        group: 'core',
        description: 'Read file.',
        effectClass: 'observe',
        riskClass: 'observe',
        tags: [],
        schemaFields: [],
        examples: [],
      },
    ],
    dbTouchpoints: {
      reads: [
        {
          table: 'agent_runs',
          kind: 'read',
          purpose: 'reads run state',
          triggers: [],
          columns: [],
        },
      ],
      writes: [
        {
          table: 'agent_runs',
          kind: 'write',
          purpose: 'persists run state',
          triggers: [],
          columns: [],
        },
      ],
      encouraged: [],
    },
    output: {
      contract: 'engineering_summary',
      label: 'Engineering Summary',
      description: 'Summary text.',
      sections: [
        {
          id: 'files_changed',
          label: 'Files Changed',
          description: 'Per-file summary.',
          emphasis: 'core',
          producedByTools: [],
        },
      ],
    },
    consumes: [
      {
        id: 'plan_pack',
        label: 'Plan Pack',
        description: 'Accepted plan from Plan agent.',
        sourceAgent: 'plan',
        contract: 'plan_pack',
        sections: ['slices'],
        required: true,
      },
    ],
  }
}

function detailWithTriggerLabels(): WorkflowAgentDetailDto {
  const next = detail()
  next.output = {
    ...next.output,
    sections: next.output.sections.map((section) =>
      section.id === 'files_changed'
        ? { ...section, producedByTools: ['Read'] }
        : section,
    ),
  }
  next.dbTouchpoints = {
    ...next.dbTouchpoints,
    encouraged: [
      {
        table: 'project_context_records',
        kind: 'encouraged',
        purpose: 'captures handoff context',
        triggers: [{ kind: 'output_section', id: 'files_changed' }],
        columns: [],
      },
    ],
  }
  return next
}

describe('AgentVisualization', () => {
  it('estimates expanded card height from stable chrome plus natural body height', () => {
    expect(estimateExpandedCardHeight(48, 0, 128, true)).toBe(176)
    expect(estimateExpandedCardHeight(176, 128, 128, false)).toBe(48)
    expect(estimateExpandedCardHeight(64, 96, 128, true)).toBe(128)
  })

  it('reserves enough initial height for wrapped DB trigger chips', () => {
    const triggers = [
      { kind: 'tool' as const, name: 'Edit' },
      { kind: 'tool' as const, name: 'Write' },
      { kind: 'tool' as const, name: 'NotebookEdit' },
      { kind: 'lifecycle' as const, event: 'file_edit' as const },
    ]
    const badgeOnlyHeight = estimateDbTableCardHeight({
      purpose: 'appends one operation row per file mutation for diff and rollback',
      triggers,
      columns: [],
    })
    const collapsedWithColumnsHeight = estimateDbTableCardHeight({
      purpose: 'appends one operation row per file mutation for diff and rollback',
      triggers,
      columns: ['id', 'operation_json', 'created_at'],
    })
    const expandedWithColumnsHeight = estimateDbTableCardHeight({
      purpose: 'appends one operation row per file mutation for diff and rollback',
      triggers,
      columns: ['id', 'operation_json', 'created_at'],
      expanded: true,
    })

    expect(badgeOnlyHeight).toBeGreaterThan(116)
    expect(collapsedWithColumnsHeight).toBeGreaterThan(badgeOnlyHeight)
    expect(expandedWithColumnsHeight).toBeGreaterThan(collapsedWithColumnsHeight)
  })

  it('moves a tool lane label and its tool frames by the same drag delta', () => {
    const graphNodes = [
      {
        id: 'lane:tool',
        type: 'lane-label',
        position: { x: 100, y: 40 },
        data: { label: 'Tools', count: 1 },
      },
      {
        id: 'tool-group-frame:core',
        type: 'tool-group-frame',
        position: { x: 120, y: 90 },
        data: { label: 'Core', count: 1 },
      },
      {
        id: 'tool:Read',
        type: 'tool',
        parentId: 'tool-group-frame:core',
        position: { x: 12, y: 26 },
        data: { tool: { name: 'Read' } },
      },
      {
        id: 'db:write:agent_runs',
        type: 'db-table',
        position: { x: 500, y: 300 },
        data: { table: 'agent_runs' },
      },
    ] as Node[]

    const memberIds = getLaneDragMemberIds(graphNodes, 'lane:tool')
    expect(memberIds.has('lane:tool')).toBe(true)
    expect(memberIds.has('tool-group-frame:core')).toBe(true)
    expect(memberIds.has('tool:Read')).toBe(false)

    const next = applyLaneDragPositionChanges(graphNodes, [
      {
        id: 'lane:tool',
        type: 'position',
        position: { x: 130, y: 75 },
        dragging: true,
      },
    ])

    expect(next.find((node) => node.id === 'lane:tool')?.position).toEqual({
      x: 130,
      y: 75,
    })
    expect(next.find((node) => node.id === 'tool-group-frame:core')?.position).toEqual({
      x: 150,
      y: 125,
    })
    expect(next.find((node) => node.id === 'tool:Read')?.position).toEqual({
      x: 12,
      y: 26,
    })
    expect(next.find((node) => node.id === 'db:write:agent_runs')?.position).toEqual({
      x: 500,
      y: 300,
    })
  })

  it('seeds card geometry without pinning the wrapper away from the real card size', () => {
    const nodes = applyKnownNodeDimensions(
      [
        {
          id: 'db:write:agent_runs',
          type: 'db-table',
          position: { x: 0, y: 0 },
          data: {
            table: 'agent_runs',
            touchpoint: 'write',
            purpose: 'records run state',
            triggers: [],
            columns: [],
          },
        },
        {
          id: 'tool-group-frame:core',
          type: 'tool-group-frame',
          position: { x: 0, y: 0 },
          data: { label: 'Core', count: 1, order: 0, sourceGroups: ['core'] },
        },
      ],
      new Map([
        ['db:write:agent_runs', { width: 260, height: 140 }],
        ['tool-group-frame:core', { width: 280, height: 96 }],
      ]),
    ) as Node[]

    const dbNode = nodes.find((node) => node.id === 'db:write:agent_runs')
    const frameNode = nodes.find((node) => node.id === 'tool-group-frame:core')

    expect(dbNode?.width).toBeUndefined()
    expect(dbNode?.height).toBeUndefined()
    expect(dbNode?.initialWidth).toBe(260)
    expect(dbNode?.initialHeight).toBe(140)
    expect(dbNode?.handles?.[0]).toMatchObject({
      type: 'target',
      position: 'left',
      y: 66,
    })
    expect(frameNode?.width).toBe(280)
    expect(frameNode?.height).toBe(96)
  })

  it('mounts inside a ReactFlow provider without throwing', () => {
    // jsdom doesn't implement ResizeObserver — provide a stub before render.
    installResizeObserverStub()

    const { container, unmount } = render(<AgentVisualization detail={detail()} />)
    // ReactFlow renders into a div with class "react-flow".
    expect(container.querySelector('.react-flow')).not.toBeNull()
    unmount()
  })

  it('renders the empty workflow state inside the same ReactFlow canvas', () => {
    installResizeObserverStub()
    vi.useFakeTimers()

    const { container, queryByLabelText, getByText } = render(
      <AgentVisualization
        detail={null}
        emptyState={<div>Start with a workflow</div>}
      />,
    )

    expect(container.querySelector('.react-flow')).not.toBeNull()
    expect(container.querySelector('.agent-visualization__dots')).not.toBeNull()
    expect(container.querySelector('.agent-visualization__empty-state.is-hidden')).not.toBeNull()
    act(() => {
      vi.advanceTimersByTime(50)
    })
    expect(container.querySelector('.agent-visualization__empty-state.is-visible')).not.toBeNull()
    expect(getByText('Start with a workflow')).toBeTruthy()
    expect(queryByLabelText('Reset layout')).toBeNull()
  })

  it('layers the empty workflow state above the canvas renderer so actions receive clicks', () => {
    installResizeObserverStub()
    vi.useFakeTimers()
    const onCreateWorkflow = vi.fn()

    const { container, getByRole } = render(
      <AgentVisualization
        detail={null}
        emptyState={<WorkflowCanvasEmptyState onCreateWorkflow={onCreateWorkflow} />}
      />,
    )

    act(() => {
      vi.advanceTimersByTime(50)
    })

    const emptyStateLayer = container.querySelector('.agent-visualization__empty-state')
    const rendererLayer = container.querySelector('.react-flow__renderer')
    expect(emptyStateLayer).not.toBeNull()
    expect(rendererLayer).not.toBeNull()
    expect(
      zIndexForSelector(AGENT_VISUALIZATION_CSS, '.agent-visualization__empty-state'),
    ).toBeGreaterThan(zIndexForSelector(REACT_FLOW_CSS, '.react-flow__renderer'))

    const createWorkflow = getByRole('button', { name: 'Create workflow' })
    fireEvent.pointerDown(createWorkflow)
    fireEvent.click(createWorkflow)

    expect(onCreateWorkflow).toHaveBeenCalledTimes(1)
  })

  it('keeps the selected graph mounted while closing back to the empty canvas', () => {
    installResizeObserverStub()
    vi.useFakeTimers()

    const emptyState = <div>Start with a workflow</div>
    const { container, rerender, getByText } = render(
      <AgentVisualization
        detail={detail()}
        emptyState={emptyState}
        emptyStateVisible={false}
      />,
    )

    expect(container.querySelector('.react-flow__node[data-id="agent-header"]')).not.toBeNull()

    rerender(
      <AgentVisualization
        detail={null}
        emptyState={emptyState}
        emptyStateVisible
      />,
    )

    expect(container.querySelector('.agent-visualization.is-agent-exiting')).not.toBeNull()
    expect(container.querySelector('.react-flow__node[data-id="agent-header"]')).not.toBeNull()
    expect(container.querySelector('.agent-visualization__empty-state.is-hidden')).not.toBeNull()

    act(() => {
      vi.advanceTimersByTime(50)
    })

    expect(container.querySelector('.agent-visualization__empty-state.is-visible')).not.toBeNull()
    expect(getByText('Start with a workflow')).toBeTruthy()

    act(() => {
      vi.advanceTimersByTime(250)
    })

    expect(container.querySelector('.agent-visualization.is-agent-exiting')).toBeNull()
    expect(container.querySelector('.react-flow__node[data-id="agent-header"]')).toBeNull()
  })

  it('resets layout without changing the viewport', () => {
    installResizeObserverStub()

    const { getByLabelText } = render(<AgentVisualization detail={detail()} />)
    fitViewSpy.mockClear()

    const resetButton = getByLabelText('Reset layout')
    fireEvent.click(resetButton)

    expect(fitViewSpy).not.toHaveBeenCalled()
    expect(resetButton.getAttribute('title')).toBeNull()
  })

  it('refreshes node internals after mount so seeded handles cannot persist', () => {
    installResizeObserverStub()

    render(<AgentVisualization detail={detail()} />)

    expect(updateNodeInternalsSpy).toHaveBeenCalledWith(
      expect.arrayContaining(['agent-header', 'prompt:0:sys', 'agent-output']),
    )
    expect(updateNodeInternalsSpy.mock.calls[0]?.[0]).not.toContain('lane:prompt')
  })

  it('shows all DB trigger badges without expansion when there are no columns', () => {
    installResizeObserverStub()

    const next = detail()
    next.dbTouchpoints = {
      reads: [],
      writes: [
        {
          table: 'code_history_operations',
          kind: 'write',
          purpose: 'appends one operation row per file mutation for diff and rollback',
          triggers: [
            { kind: 'tool', name: 'Edit' },
            { kind: 'tool', name: 'Write' },
            { kind: 'tool', name: 'NotebookEdit' },
            { kind: 'lifecycle', event: 'file_edit' },
          ],
          columns: [],
        },
      ],
      encouraged: [],
    }

    const { container } = render(<AgentVisualization detail={next} />)
    const dbNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="db:write:code_history_operations"]',
    )

    expect(dbNode).not.toBeNull()
    const db = within(dbNode!)
    expect(db.queryByRole('button', { name: /more/i })).toBeNull()
    expect(db.queryByText('+1 more')).toBeNull()
    expect(db.queryByText('Tool: Notebook Edit')).not.toBeNull()
    expect(db.queryByText('on file edit')).not.toBeNull()
  })

  it('focuses connected graph elements with DOM classes on hover', () => {
    installResizeObserverStub()
    const requestAnimationFrameSpy = vi
      .spyOn(window, 'requestAnimationFrame')
      .mockImplementation((callback) => {
        callback(0)
        return 1
      })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    const { container } = render(<AgentVisualization detail={detailWithTriggerLabels()} />)
    const canvas = container.querySelector<HTMLElement>('.agent-visualization')
    const headerNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="agent-header"]',
    )
    const toolNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool:Read"]',
    )
    const toolEdge = container.querySelector<SVGElement>('.react-flow__edge.agent-edge-tool')
    const outputSectionEdge = container.querySelector<SVGElement>(
      '.react-flow__edge.agent-edge-output-section',
    )
    const producedEdgeId = 'e:trigger:tool:Read->output-section:files_changed'
    const encouragedEdgeId =
      'e:trigger:output-section:files_changed->db:encouraged:project_context_records'
    const producedTriggerEdge = Array.from(
      container.querySelectorAll<SVGElement>('.react-flow__edge.agent-edge-trigger'),
    ).find((edge) => edge.getAttribute('data-id') === producedEdgeId)
    const encouragedTriggerEdge = Array.from(
      container.querySelectorAll<SVGElement>('.react-flow__edge.agent-edge-trigger'),
    ).find((edge) => edge.getAttribute('data-id') === encouragedEdgeId)
    const producedTriggerLabel = Array.from(
      container.querySelectorAll<HTMLElement>('.agent-edge-trigger-label'),
    ).find((label) => label.getAttribute('data-edge-id') === producedEdgeId)
    const encouragedTriggerLabel = Array.from(
      container.querySelectorAll<HTMLElement>('.agent-edge-trigger-label'),
    ).find((label) => label.getAttribute('data-edge-id') === encouragedEdgeId)

    expect(canvas).not.toBeNull()
    expect(headerNode).not.toBeNull()
    expect(toolNode).not.toBeNull()
    expect(producedTriggerEdge).toBeDefined()
    expect(encouragedTriggerEdge).toBeDefined()
    expect(producedTriggerLabel).toBeDefined()
    expect(encouragedTriggerLabel).toBeDefined()

    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: vi.fn(() => toolNode),
    })
    fireEvent.pointerMove(canvas!, { buttons: 0, clientX: 10, clientY: 10 })

    expect(requestAnimationFrameSpy).toHaveBeenCalled()
    expect(canvas!.classList.contains('is-focusing')).toBe(true)
    expect(headerNode!.classList.contains('is-focused')).toBe(true)
    expect(toolNode!.classList.contains('is-focused')).toBe(true)
    if (toolEdge) {
      expect(toolEdge.classList.contains('is-active')).toBe(true)
    }
    if (outputSectionEdge) {
      expect(outputSectionEdge.classList.contains('is-active')).toBe(false)
    }
    expect(producedTriggerEdge!.classList.contains('is-active')).toBe(true)
    expect(encouragedTriggerEdge!.classList.contains('is-active')).toBe(false)
    expect(producedTriggerLabel!.classList.contains('is-active')).toBe(true)
    expect(encouragedTriggerLabel!.classList.contains('is-active')).toBe(false)

    fireEvent.wheel(canvas!, { deltaY: -120 })

    expect(canvas!.classList.contains('is-interacting')).toBe(false)
    expect(canvas!.classList.contains('is-focusing')).toBe(false)
    expect(producedTriggerLabel!.isConnected).toBe(true)
    expect(encouragedTriggerLabel!.isConnected).toBe(true)
    expect(container.querySelectorAll('.agent-edge-trigger-label')).toHaveLength(2)
    expect(headerNode!.classList.contains('is-focused')).toBe(false)
    expect(toolNode!.classList.contains('is-focused')).toBe(false)

    fireEvent.pointerMove(canvas!, { buttons: 1, clientX: 12, clientY: 12 })

    expect(canvas!.classList.contains('is-focusing')).toBe(false)
    expect(headerNode!.classList.contains('is-focused')).toBe(false)
    if (toolEdge) {
      expect(toolEdge.classList.contains('is-active')).toBe(false)
    }
    expect(producedTriggerEdge!.classList.contains('is-active')).toBe(false)
    expect(producedTriggerLabel!.classList.contains('is-active')).toBe(false)
  })

  it('does not render invisible edge hit paths because canvas edges are not interactive', () => {
    installResizeObserverStub()

    const { container } = render(<AgentVisualization detail={detailWithTriggerLabels()} />)

    expect(container.querySelector('.react-flow__edge')).not.toBeNull()
    expect(container.querySelector('.react-flow__edge-interaction')).toBeNull()
  })

  it('does not render custom or native node hover tooltips', () => {
    installResizeObserverStub()

    const { container } = render(<AgentVisualization detail={detailWithTriggerLabels()} />)

    expect(container.querySelector('.agent-canvas-tooltip')).toBeNull()
    expect(container.querySelector('[role="tooltip"]')).toBeNull()
    expect(container.querySelector('.react-flow__node [title]')).toBeNull()
  })

  it('hides tool handles when the tool is only connected through its category', () => {
    installResizeObserverStub()

    const { container } = render(<AgentVisualization detail={detail()} />)
    const toolNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool:Read"]',
    )

    expect(toolNode).not.toBeNull()
    expect(toolNode!.querySelectorAll('.react-flow__handle')).toHaveLength(0)
  })

  it('shows only the connected side handle for direct tool trigger edges', () => {
    installResizeObserverStub()

    const { container } = render(<AgentVisualization detail={detailWithTriggerLabels()} />)
    const toolNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool:Read"]',
    )

    expect(toolNode).not.toBeNull()
    expect(toolNode!.querySelector('.react-flow__handle-right.source')).not.toBeNull()
    expect(toolNode!.querySelector('.react-flow__handle-left.target')).toBeNull()
  })

  it('uses the pointer target for hover focus before falling back to hit-testing', () => {
    installResizeObserverStub()
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    const elementFromPoint = vi.fn(() => null)
    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: elementFromPoint,
    })

    const { container } = render(<AgentVisualization detail={detail()} />)
    const canvas = container.querySelector<HTMLElement>('.agent-visualization')
    const headerNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="agent-header"]',
    )
    const toolNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool:Read"]',
    )

    expect(canvas).not.toBeNull()
    expect(headerNode).not.toBeNull()
    expect(toolNode).not.toBeNull()

    fireEvent.pointerMove(toolNode!, { buttons: 0, clientX: 10, clientY: 10 })

    expect(elementFromPoint).not.toHaveBeenCalled()
    expect(canvas!.classList.contains('is-focusing')).toBe(true)
    expect(headerNode!.classList.contains('is-focused')).toBe(true)
    expect(toolNode!.classList.contains('is-focused')).toBe(true)
  })

  it('keeps tool hover specific even when the tool is inside a category frame', () => {
    installResizeObserverStub()
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    const multiToolDetail = detail()
    multiToolDetail.tools = [
      ...multiToolDetail.tools,
      {
        name: 'Grep',
        group: 'core',
        description: 'Search files.',
        effectClass: 'observe',
        riskClass: 'observe',
        tags: [],
        schemaFields: [],
        examples: [],
      },
    ]

    const { container } = render(<AgentVisualization detail={multiToolDetail} />)
    const frameNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool-group-frame:core"]',
    )
    const readButton = container.querySelector<HTMLButtonElement>(
      '.react-flow__node[data-id="tool:Read"] button',
    )
    const readNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool:Read"]',
    )
    const grepNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool:Grep"]',
    )

    expect(frameNode).not.toBeNull()
    expect(readButton).not.toBeNull()
    expect(readNode).not.toBeNull()
    expect(grepNode).not.toBeNull()
    expect(container.querySelector('.agent-tool-group-frame__drag-surface')).toBeNull()
    expect(frameNode!.style.pointerEvents).toBe('none')
    expect(readNode!.style.pointerEvents).toBe('all')

    fireEvent.pointerMove(readButton!, { buttons: 0, clientX: 10, clientY: 10 })

    expect(frameNode!.classList.contains('is-focused')).toBe(true)
    expect(readNode!.classList.contains('is-focused')).toBe(true)
    expect(grepNode!.classList.contains('is-focused')).toBe(false)
  })

  it('focuses a tool category frame and its related tools on hover', () => {
    installResizeObserverStub()
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    const { container } = render(<AgentVisualization detail={detail()} />)
    const canvas = container.querySelector<HTMLElement>('.agent-visualization')
    const headerNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="agent-header"]',
    )
    const frameNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool-group-frame:core"]',
    )
    const frameLabel = frameNode?.querySelector<HTMLElement>('.agent-tool-group-frame__label')
    const toolNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool:Read"]',
    )
    const toolEdge = container.querySelector<SVGElement>('.react-flow__edge.agent-edge-tool')

    expect(canvas).not.toBeNull()
    expect(headerNode).not.toBeNull()
    expect(frameNode).not.toBeNull()
    expect(frameLabel).not.toBeNull()
    expect(toolNode).not.toBeNull()

    fireEvent.pointerMove(frameLabel!, { buttons: 0, clientX: 10, clientY: 10 })

    expect(canvas!.classList.contains('is-focusing')).toBe(true)
    expect(headerNode!.classList.contains('is-focused')).toBe(true)
    expect(frameNode!.classList.contains('is-focused')).toBe(true)
    expect(toolNode!.classList.contains('is-focused')).toBe(true)
    if (toolEdge) {
      expect(toolEdge.classList.contains('is-active')).toBe(true)
    }
  })

  it('expands a tool card when its row is clicked inside a category frame', () => {
    installResizeObserverStub()

    const { container } = render(<AgentVisualization detail={detail()} />)
    const toolButton = container.querySelector<HTMLButtonElement>(
      '.react-flow__node[data-id="tool:Read"] button',
    )
    const toolCard = toolButton?.closest<HTMLElement>('.agent-card')

    expect(toolButton).not.toBeNull()
    expect(toolCard).not.toBeNull()
    expect(toolButton!.closest<HTMLElement>('.react-flow__node')?.style.pointerEvents).toBe(
      'all',
    )
    expect(toolCard!.classList.contains('is-card-expanded')).toBe(false)

    fireEvent.pointerDown(toolButton!, { button: 0 })
    fireEvent.click(toolButton!)

    expect(toolButton!.classList.contains('nodrag')).toBe(true)
    expect(toolButton!.classList.contains('nopan')).toBe(true)
    expect(toolCard!.classList.contains('is-card-expanded')).toBe(true)
  })
})
