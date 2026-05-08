import { fireEvent, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { WorkflowAgentDetailDto } from '@/src/lib/xero-model/workflow-agents'

import { AgentVisualization } from './agent-visualization'

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
})
