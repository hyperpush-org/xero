import { fireEvent, render } from '@testing-library/react'
import type { Node } from '@xyflow/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { WorkflowAgentDetailDto } from '@/src/lib/xero-model/workflow-agents'

import {
  AgentVisualization,
  applyLaneDragPositionChanges,
  estimateExpandedCardHeight,
  getLaneDragMemberIds,
} from './agent-visualization'

const originalElementFromPoint = document.elementFromPoint

function installResizeObserverStub() {
  if ((globalThis as { ResizeObserver?: unknown }).ResizeObserver) return
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  ;(globalThis as { ResizeObserver?: unknown }).ResizeObserver = ResizeObserverStub
}

afterEach(() => {
  vi.restoreAllMocks()
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

describe('AgentVisualization', () => {
  it('estimates expanded card height from stable chrome plus natural body height', () => {
    expect(estimateExpandedCardHeight(48, 0, 128, true)).toBe(176)
    expect(estimateExpandedCardHeight(176, 128, 128, false)).toBe(48)
    expect(estimateExpandedCardHeight(64, 96, 128, true)).toBe(128)
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

  it('mounts inside a ReactFlow provider without throwing', () => {
    // jsdom doesn't implement ResizeObserver — provide a stub before render.
    installResizeObserverStub()

    const { container, unmount } = render(<AgentVisualization detail={detail()} />)
    // ReactFlow renders into a div with class "react-flow".
    expect(container.querySelector('.react-flow')).not.toBeNull()
    unmount()
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

    const { container } = render(<AgentVisualization detail={detail()} />)
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

    expect(canvas).not.toBeNull()
    expect(headerNode).not.toBeNull()
    expect(toolNode).not.toBeNull()

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

    fireEvent.wheel(canvas!, { deltaY: -120 })

    expect(canvas!.classList.contains('is-focusing')).toBe(false)
    expect(headerNode!.classList.contains('is-focused')).toBe(false)
    expect(toolNode!.classList.contains('is-focused')).toBe(false)

    fireEvent.pointerMove(canvas!, { buttons: 1, clientX: 12, clientY: 12 })

    expect(canvas!.classList.contains('is-focusing')).toBe(false)
    expect(headerNode!.classList.contains('is-focused')).toBe(false)
    if (toolEdge) {
      expect(toolEdge.classList.contains('is-active')).toBe(false)
    }
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
