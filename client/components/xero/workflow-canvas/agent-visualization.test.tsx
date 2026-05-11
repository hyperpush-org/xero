import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { Activity } from 'react'

import { act, fireEvent, render, waitFor, within } from '@testing-library/react'
import type { Node } from '@xyflow/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { WorkflowAgentDetailDto } from '@/src/lib/xero-model/workflow-agents'
import { WorkflowCanvasEmptyState } from '../workflow-canvas-empty-state'

const updateNodeInternalsSpy = vi.hoisted(() => vi.fn())
const fitViewSpy = vi.hoisted(() => vi.fn())
const zoomInSpy = vi.hoisted(() => vi.fn())
const zoomOutSpy = vi.hoisted(() => vi.fn())
const setViewportSpy = vi.hoisted(() => vi.fn())
const getViewportSpy = vi.hoisted(() => vi.fn(() => ({ x: 0, y: 0, zoom: 1 })))

vi.mock('@xyflow/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@xyflow/react')>()
  return {
    ...actual,
    useReactFlow: () => ({
      fitView: fitViewSpy,
      zoomIn: zoomInSpy,
      zoomOut: zoomOutSpy,
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
  selectedNodeFocusCenter,
} from './agent-visualization'
import {
  AGENT_GRAPH_HEADER_HANDLES,
  AGENT_GRAPH_HEADER_LEFT_HANDLE_RATIOS,
  AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS,
  AGENT_GRAPH_TRIGGER_HANDLES,
  buildAgentGraph,
} from './build-agent-graph'

const originalElementFromPoint = document.elementFromPoint
const AGENT_VISUALIZATION_CSS = readFileSync(
  resolve(process.cwd(), 'components/xero/workflow-canvas/agent-visualization.css'),
  'utf8',
)
const REACT_FLOW_CSS = readFileSync(
  resolve(process.cwd(), 'node_modules/@xyflow/react/dist/style.css'),
  'utf8',
)

const resizeObserverInstances: ResizeObserverStub[] = []

class ResizeObserverStub {
  private target: Element | null = null

  constructor(
    private readonly callback: ResizeObserverCallback,
  ) {
    resizeObserverInstances.push(this)
  }

  observe(target: Element) {
    this.target = target
  }

  unobserve() {
    this.target = null
  }

  disconnect() {
    this.target = null
  }

  trigger(width: number, height: number) {
    if (!this.target) return
    this.callback(
      [
        {
          target: this.target,
          contentRect: {
            width,
            height,
            x: 0,
            y: 0,
            top: 0,
            left: 0,
            right: width,
            bottom: height,
            toJSON: () => ({}),
          },
        } as ResizeObserverEntry,
      ],
      this as unknown as ResizeObserver,
    )
  }
}

function installResizeObserverStub() {
  ;(globalThis as { ResizeObserver?: unknown }).ResizeObserver = ResizeObserverStub
}

function triggerResizeObserver(width: number, height: number) {
  for (const observer of resizeObserverInstances) {
    observer.trigger(width, height)
  }
}

function mockElementBounds(width: number, height: number) {
  return vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    width,
    height,
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: width,
    bottom: height,
    toJSON: () => ({}),
  })
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
  zoomInSpy.mockClear()
  zoomOutSpy.mockClear()
  setViewportSpy.mockClear()
  getViewportSpy.mockClear()
  resizeObserverInstances.length = 0
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
    attachedSkills: [],
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

function detailWithAttachedSkill(): WorkflowAgentDetailDto {
  const next = detail()
  next.attachedSkills = [
    {
      id: 'rust-best-practices',
      sourceId: 'skill-source:v1:global:bundled:xero:rust-best-practices',
      skillId: 'rust-best-practices',
      name: 'Rust Best Practices',
      description: 'Guide for writing idiomatic Rust.',
      sourceKind: 'bundled',
      scope: 'global',
      versionHash: 'hash-rust',
      includeSupportingAssets: false,
      required: true,
      sourceState: 'enabled',
      trustState: 'trusted',
      availabilityStatus: 'available',
      availabilityReason: 'Skill source is enabled, trusted, and pinned for attachment.',
    },
  ]
  return next
}

describe('AgentVisualization', () => {
  it('estimates expanded card height from stable chrome plus natural body height', () => {
    expect(estimateExpandedCardHeight(48, 0, 128, true)).toBe(176)
    expect(estimateExpandedCardHeight(176, 128, 128, false)).toBe(48)
    expect(estimateExpandedCardHeight(64, 96, 128, true)).toBe(128)
  })

  it('centers selected nodes from their React Flow absolute viewport position', () => {
    const node = {
      id: 'tool:read',
      type: 'tool',
      position: { x: 24, y: 18 },
      measured: { width: 240, height: 36 },
      internals: {
        positionAbsolute: { x: 1_400, y: 520 },
      },
      data: {},
    } as unknown as Node
    const center = selectedNodeFocusCenter(
      node,
      [node],
      320,
      1.05,
    )

    expect(center.x).toBeCloseTo(1_400 + 120 - 320 / 2 / 1.05)
    expect(center.y).toBeCloseTo(520 + 18)
  })

  it('centers selected grouped tool nodes from their parent frame position', () => {
    const frame = {
      id: 'tool-group-frame:test_harness',
      type: 'tool-group-frame',
      position: { x: 1_600, y: 320 },
      measured: { width: 280, height: 110 },
      data: {},
    } as unknown as Node
    const tool = {
      id: 'tool:Harness Runner',
      type: 'tool',
      parentId: frame.id,
      position: { x: 16, y: 38 },
      measured: { width: 240, height: 36 },
      data: {},
    } as unknown as Node

    const center = selectedNodeFocusCenter(tool, [frame, tool], 320, 1.05)

    expect(center.x).toBeCloseTo(1_600 + 16 + 120 - 320 / 2 / 1.05)
    expect(center.y).toBeCloseTo(320 + 38 + 18)
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

  it('renders handles larger and clips them into half circles by side', () => {
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /\.agent-visualization \.react-flow__handle\s*\{[^}]*width:\s*16px;[^}]*height:\s*16px;[^}]*border-radius:\s*999px;/m,
    )
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /\.agent-visualization \.react-flow__handle-left\s*\{[^}]*clip-path:\s*inset\(0 50% 0 0\);/m,
    )
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /\.agent-visualization \.react-flow__handle-right\s*\{[^}]*clip-path:\s*inset\(0 0 0 50%\);/m,
    )
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /\.agent-visualization \.react-flow__handle-top\s*\{[^}]*clip-path:\s*inset\(0 0 50% 0\);/m,
    )
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /\.agent-visualization \.react-flow__handle-bottom\s*\{[^}]*clip-path:\s*inset\(50% 0 0 0\);/m,
    )
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /\.agent-visualization\.is-editing \.react-flow__handle\s*\{[^}]*width:\s*18px !important;[^}]*height:\s*18px !important;/m,
    )
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /\.agent-visualization\.is-editing \.react-flow__handle\s*\{[^}]*transform-origin:\s*center center;[^}]*scale:\s*1;/m,
    )
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /transition:\s*scale 120ms ease,/m,
    )
    // Hover-only state (no drag in progress) keeps a subtle ring so handles
    // read as grabable; the full glow is reserved for the active drag's
    // source and for valid hovered targets.
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /\.agent-visualization\.is-editing:not\(\[data-drag-role\]\) \.react-flow__handle:hover\s*\{[^}]*scale:\s*1\.08;/m,
    )
    // The grabbed source handle and any valid hovered target handle get
    // the full glow. Crucially, .connecting (every handle during a drag)
    // and .connectingto (without .valid) are NOT in this rule — they stay
    // at base, which is the whole point of the honest-highlights change.
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /\.agent-visualization\.is-editing \.react-flow__handle\.connectingfrom,[^{]*\.agent-visualization\.is-editing \.react-flow__handle\.connectingto\.valid\s*\{[^}]*scale:\s*1\.12;/m,
    )
    expect(AGENT_VISUALIZATION_CSS).not.toMatch(
      /\.agent-visualization\.is-editing \.react-flow__handle:hover,[^{]*\{[^}]*transform:/m,
    )
  })

  it('does not put important Tailwind sizing classes on React Flow handles', () => {
    installResizeObserverStub()

    const { container } = render(<AgentVisualization detail={null} editing mode="create" />)
    const handles = Array.from(container.querySelectorAll<HTMLElement>('.react-flow__handle'))

    expect(handles.length).toBeGreaterThan(0)
    for (const handle of handles) {
      expect(handle.classList.contains('!w-2')).toBe(false)
      expect(handle.classList.contains('!h-2')).toBe(false)
    }
  })

  it('uses trigger-purple handles for individual tool connections in view mode', () => {
    installResizeObserverStub()

    const { container } = render(<AgentVisualization detail={detailWithTriggerLabels()} />)
    const handles = Array.from(
      container.querySelectorAll<HTMLElement>(
        '.react-flow__node[data-id="tool:Read"] .react-flow__handle',
      ),
    )

    expect(handles).toHaveLength(1)
    for (const handle of handles) {
      expect(handle.classList.contains('!bg-fuchsia-500')).toBe(true)
      expect(handle.classList.contains('!bg-sky-500')).toBe(false)
    }
  })

  it('uses trigger-purple handles for individual tool connections in edit mode', () => {
    installResizeObserverStub()

    const { container } = render(
      <AgentVisualization
        detail={null}
        editing
        mode="edit"
        initialDetail={detail()}
      />,
    )
    const handles = Array.from(
      container.querySelectorAll<HTMLElement>(
        '.react-flow__node[data-id="tool:Read"] .react-flow__handle',
      ),
    )

    expect(handles).toHaveLength(2)
    for (const handle of handles) {
      expect(handle.classList.contains('!bg-fuchsia-500')).toBe(true)
      expect(handle.classList.contains('!bg-sky-500')).toBe(false)
    }
  })

  it('uses trigger-purple handles where trigger edges meet output sections and databases in view mode', () => {
    installResizeObserverStub()

    const { container } = render(<AgentVisualization detail={detailWithTriggerLabels()} />)
    const outputSectionHandles = Array.from(
      container.querySelectorAll<HTMLElement>(
        '.react-flow__node[data-id="output-section:files_changed"] .react-flow__handle',
      ),
    )
    const dbHandles = Array.from(
      container.querySelectorAll<HTMLElement>(
        '.react-flow__node[data-id="db:encouraged:project_context_records"] .react-flow__handle',
      ),
    )

    expect(
      outputSectionHandles.filter((handle) =>
        handle.classList.contains('!bg-fuchsia-500'),
      ),
    ).toHaveLength(2)
    expect(
      outputSectionHandles.filter((handle) =>
        handle.classList.contains('!bg-foreground'),
      ),
    ).toHaveLength(1)
    expect(
      dbHandles.filter((handle) => handle.classList.contains('!bg-fuchsia-500')),
    ).toHaveLength(1)
    expect(
      dbHandles.filter((handle) => handle.classList.contains('!bg-emerald-500')),
    ).toHaveLength(1)
  })

  it('uses trigger-purple handles where trigger edges meet output sections and databases in edit mode', () => {
    installResizeObserverStub()

    const { container } = render(
      <AgentVisualization
        detail={null}
        editing
        mode="edit"
        initialDetail={detail()}
      />,
    )
    const outputSectionHandles = Array.from(
      container.querySelectorAll<HTMLElement>(
        '.react-flow__node[data-id="output-section:files_changed"] .react-flow__handle',
      ),
    )
    const dbHandles = Array.from(
      container.querySelectorAll<HTMLElement>(
        '.react-flow__node[data-id="db:read:agent_runs"] .react-flow__handle',
      ),
    )

    expect(
      outputSectionHandles.filter((handle) =>
        handle.classList.contains('!bg-fuchsia-500'),
      ),
    ).toHaveLength(2)
    expect(
      outputSectionHandles.filter((handle) =>
        handle.classList.contains('!bg-foreground'),
      ),
    ).toHaveLength(1)
    expect(
      dbHandles.filter((handle) => handle.classList.contains('!bg-fuchsia-500')),
    ).toHaveLength(1)
    expect(
      dbHandles.filter((handle) => handle.classList.contains('!bg-emerald-500')),
    ).toHaveLength(2)
  })

  it('routes every trigger edge through trigger-purple handles', () => {
    const { edges } = buildAgentGraph(detailWithTriggerLabels())
    const triggerEdges = edges.filter(
      (edge) => edge.className === 'agent-edge agent-edge-trigger',
    )

    expect(triggerEdges.length).toBeGreaterThan(0)
    for (const edge of triggerEdges) {
      expect(edge.sourceHandle).toBe(AGENT_GRAPH_TRIGGER_HANDLES.source)
      expect(edge.targetHandle).toBe(AGENT_GRAPH_TRIGGER_HANDLES.target)
    }
  })

  it('renders attached skills as skills nodes, not tool nodes', () => {
    installResizeObserverStub()

    const { container } = render(<AgentVisualization detail={detailWithAttachedSkill()} />)
    const skillNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="skills:rust-best-practices"]',
    )

    expect(skillNode).not.toBeNull()
    expect(container.querySelector('.react-flow__node[data-id="tool:rust-best-practices"]')).toBeNull()
    const skill = within(skillNode!)
    expect(skill.getByText('Rust Best Practices')).toBeVisible()
    expect(skill.getByText('rust-best-practices')).toBeVisible()
    expect(skill.getByText('Pinned')).toBeVisible()
    expect(skill.getByText('Required')).toBeVisible()
    expect(skill.getByText('hash-rust')).toBeVisible()
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
    const agentDetail = detail()
    const nodes = applyKnownNodeDimensions(
      [
        {
          id: 'agent-header',
          type: 'agent-header',
          position: { x: 0, y: 0 },
          data: {
            header: agentDetail.header,
            summary: {
              prompts: 1,
              tools: 1,
              dbTables: 1,
              outputSections: 1,
              consumes: 0,
              attachedSkills: 0,
            },
            advanced: {
              workflowContract: '',
              finalResponseContract: '',
              examplePrompts: [],
              refusalEscalationCases: [],
              allowedEffectClasses: [],
              deniedTools: [],
              allowedToolPacks: [],
              deniedToolPacks: [],
              allowedToolGroups: [],
              deniedToolGroups: [],
              allowedMcpServers: [],
              deniedMcpServers: [],
              allowedDynamicTools: [],
              deniedDynamicTools: [],
              allowedSubagentRoles: [],
              deniedSubagentRoles: [],
              externalServiceAllowed: false,
              browserControlAllowed: false,
              skillRuntimeAllowed: false,
              subagentAllowed: false,
              commandAllowed: false,
              destructiveWriteAllowed: false,
            },
          },
        },
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
        ['agent-header', { width: 300, height: 210 }],
        ['db:write:agent_runs', { width: 260, height: 140 }],
        ['tool-group-frame:core', { width: 280, height: 96 }],
      ]),
    ) as Node[]

    const headerNode = nodes.find((node) => node.id === 'agent-header')
    const dbNode = nodes.find((node) => node.id === 'db:write:agent_runs')
    const frameNode = nodes.find((node) => node.id === 'tool-group-frame:core')
    const toolHandle = headerNode?.handles?.find(
      (handle) => handle.id === AGENT_GRAPH_HEADER_HANDLES.tool,
    )
    const dbHandle = headerNode?.handles?.find(
      (handle) => handle.id === AGENT_GRAPH_HEADER_HANDLES.db,
    )
    const consumedHandle = headerNode?.handles?.find(
      (handle) => handle.id === AGENT_GRAPH_HEADER_HANDLES.consumed,
    )
    const skillsHandle = headerNode?.handles?.find(
      (handle) => handle.id === AGENT_GRAPH_HEADER_HANDLES.skills,
    )

    expect((toolHandle?.y ?? 0) + (toolHandle?.height ?? 0) / 2).toBeCloseTo(
      210 * AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS.tool,
    )
    expect((dbHandle?.y ?? 0) + (dbHandle?.height ?? 0) / 2).toBeCloseTo(
      210 * AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS.db,
    )
    expect(consumedHandle?.position).toBe('left')
    expect(skillsHandle?.position).toBe('left')
    expect((consumedHandle?.y ?? 0) + (consumedHandle?.height ?? 0) / 2).toBeCloseTo(
      210 * AGENT_GRAPH_HEADER_LEFT_HANDLE_RATIOS.consumed,
    )
    expect((skillsHandle?.y ?? 0) + (skillsHandle?.height ?? 0) / 2).toBeCloseTo(
      210 * AGENT_GRAPH_HEADER_LEFT_HANDLE_RATIOS.skills,
    )
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

  it('lets editing DB nodes keep DOM-measured handles after drags', () => {
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
            columns: ['id', 'status'],
          },
        },
        {
          id: 'prompt:0:sys',
          type: 'prompt',
          position: { x: 0, y: 0 },
          data: {
            prompt: {
              id: 'sys',
              label: 'System',
              role: 'system',
              source: 'custom',
              body: 'Act carefully.',
            },
          },
        },
      ],
      new Map([
        ['db:write:agent_runs', { width: 260, height: 140 }],
        ['prompt:0:sys', { width: 300, height: 96 }],
      ]),
      { domMeasuredHandleNodeTypes: new Set(['db-table']) },
    ) as Node[]

    const dbNode = nodes.find((node) => node.id === 'db:write:agent_runs')
    const promptNode = nodes.find((node) => node.id === 'prompt:0:sys')

    expect(dbNode?.initialWidth).toBe(260)
    expect(dbNode?.initialHeight).toBe(140)
    expect(dbNode?.handles).toBeUndefined()
    expect(promptNode?.handles?.[0]).toMatchObject({
      type: 'target',
      position: 'bottom',
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
    const onCreateAgent = vi.fn()

    const { container, getByRole } = render(
      <AgentVisualization
        detail={null}
        emptyState={<WorkflowCanvasEmptyState onCreateAgent={onCreateAgent} />}
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

    const createAgent = getByRole('button', { name: 'Create agent' })
    fireEvent.pointerDown(createAgent)
    fireEvent.click(createAgent)

    expect(onCreateAgent).toHaveBeenCalledTimes(1)
  })

  it('marks workflow actions as coming soon in the empty state', () => {
    const onBrowseWorkflows = vi.fn()

    const { getAllByRole, getAllByText, getByRole } = render(
      <WorkflowCanvasEmptyState
        onCreateAgent={vi.fn()}
        onBrowseWorkflows={onBrowseWorkflows}
      />,
    )

    const actions = getAllByRole('button')
    expect(actions[0]).toHaveTextContent('Create agent')
    expect(actions[1]).toHaveTextContent('Create workflow')
    expect(actions[2]).toHaveTextContent('Run an existing workflow')

    expect(getByRole('button', { name: /Create workflow/i })).toBeDisabled()
    const runExistingWorkflow = getByRole('button', {
      name: /Run an existing workflow/i,
    })
    expect(runExistingWorkflow).toBeDisabled()
    expect(getAllByText('Coming soon')).toHaveLength(2)

    fireEvent.click(runExistingWorkflow)
    expect(onBrowseWorkflows).not.toHaveBeenCalled()
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

  it('omits layout mutation controls in read mode', () => {
    installResizeObserverStub()

    const { queryByLabelText } = render(<AgentVisualization detail={detail()} />)

    expect(queryByLabelText('Lock canvas')).toBeNull()
    expect(queryByLabelText(/snap to grid/i)).toBeNull()
    expect(queryByLabelText('Reset layout')).toBeNull()
  })

  it('renders read-mode viewport controls with Lucide icons from one icon set', () => {
    installResizeObserverStub()

    const { container, getByLabelText, queryByLabelText } = render(
      <AgentVisualization detail={detail()} />,
    )
    const controlButtons = Array.from(
      container.querySelectorAll<HTMLButtonElement>('.react-flow__controls-button'),
    )

    expect(controlButtons).toHaveLength(3)
    expect(controlButtons.every((button) => button.querySelector('svg.lucide'))).toBe(true)
    expect(container.querySelector('.react-flow__controls-zoomin svg.lucide-zoom-in')).not.toBeNull()
    expect(container.querySelector('.react-flow__controls-zoomout svg.lucide-zoom-out')).not.toBeNull()
    expect(container.querySelector('.react-flow__controls-fitview svg.lucide-maximize')).not.toBeNull()
    expect(queryByLabelText('Lock canvas')).toBeNull()
    expect(queryByLabelText(/snap to grid/i)).toBeNull()
    expect(queryByLabelText('Reset layout')).toBeNull()
    expect(container.querySelector('.react-flow__node.draggable')).toBeNull()
    expect(AGENT_VISUALIZATION_CSS).toMatch(
      /\.agent-visualization \.react-flow__controls-button svg\s*{[^}]*fill:\s*none;/m,
    )

    fireEvent.click(getByLabelText('Zoom in'))
    fireEvent.click(getByLabelText('Zoom out'))
    fireEvent.click(getByLabelText('Fit view'))

    expect(zoomInSpy).toHaveBeenCalledWith({ duration: 180 })
    expect(zoomOutSpy).toHaveBeenCalledWith({ duration: 180 })
    expect(fitViewSpy).toHaveBeenCalledWith({
      padding: 0.16,
      includeHiddenNodes: false,
      duration: 420,
    })
  })

  it('keeps layout controls available while authoring', () => {
    installResizeObserverStub()

    const { container, getByLabelText } = render(
      <AgentVisualization
        detail={null}
        editing
        mode="edit"
        initialDetail={detail()}
      />,
    )
    const controlButtons = Array.from(
      container.querySelectorAll<HTMLButtonElement>('.react-flow__controls-button'),
    )

    expect(controlButtons).toHaveLength(6)
    expect(getByLabelText('Lock canvas').querySelector('svg.lucide-lock-open')).not.toBeNull()
    expect(getByLabelText(/snap to grid/i).querySelector('svg.lucide-magnet')).not.toBeNull()
    expect(getByLabelText('Reset layout').querySelector('svg.lucide-rotate-ccw')).not.toBeNull()
  })

  it('refits the view canvas when the available width changes', async () => {
    installResizeObserverStub()
    mockElementBounds(1100, 760)
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    render(<AgentVisualization detail={detail()} />)

    await waitFor(() => expect(fitViewSpy).toHaveBeenCalledTimes(1))
    setViewportSpy.mockClear()
    fitViewSpy.mockClear()

    await act(async () => {
      triggerResizeObserver(720, 760)
    })

    await waitFor(() => expect(fitViewSpy).toHaveBeenCalledTimes(1))
    expect(fitViewSpy).toHaveBeenCalledWith({
      padding: 0.16,
      includeHiddenNodes: false,
      duration: 420,
    })
    expect(setViewportSpy).not.toHaveBeenCalled()
  })

  it('continues refitting the view canvas after repeated sidebar width changes', async () => {
    installResizeObserverStub()
    mockElementBounds(1100, 760)
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    render(<AgentVisualization detail={detail()} />)

    await waitFor(() => expect(fitViewSpy).toHaveBeenCalledTimes(1))
    fitViewSpy.mockClear()

    for (const width of [980, 860, 1240, 1100]) {
      await act(async () => {
        triggerResizeObserver(width, 760)
      })
      await waitFor(() =>
        expect(fitViewSpy, `width ${width}`).toHaveBeenCalledTimes(1),
      )
      expect(fitViewSpy).toHaveBeenCalledWith({
        padding: 0.16,
        includeHiddenNodes: false,
        duration: 420,
      })
      fitViewSpy.mockClear()
    }
  })

  it('fits the viewport once while editing a new graph', async () => {
    installResizeObserverStub()
    mockElementBounds(900, 700)
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    const { rerender } = render(
      <AgentVisualization detail={null} editing mode="create" />,
    )

    await waitFor(() => expect(setViewportSpy).toHaveBeenCalledTimes(1))
    expect(fitViewSpy).not.toHaveBeenCalled()
    const [initialViewport, initialOptions] = setViewportSpy.mock.calls[0]
    expect(initialOptions).toEqual({ duration: 0 })
    expect(initialViewport.zoom).toBeGreaterThan(0)
    expect(initialViewport.zoom).toBeLessThanOrEqual(1)
    expect(initialViewport.x).toBeGreaterThan(0)
    expect(initialViewport.y).toBeGreaterThan(0)
    fitViewSpy.mockClear()
    setViewportSpy.mockClear()

    rerender(
      <AgentVisualization
        detail={null}
        editing
        mode="edit"
        initialDetail={detail()}
      />,
    )

    expect(setViewportSpy).not.toHaveBeenCalled()
    expect(fitViewSpy).not.toHaveBeenCalled()
  })

  it('refits the new-agent canvas when the available width changes', async () => {
    installResizeObserverStub()
    mockElementBounds(1100, 760)
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    render(<AgentVisualization detail={null} editing mode="create" />)

    await waitFor(() => expect(setViewportSpy).toHaveBeenCalledTimes(1))
    const initialViewport = setViewportSpy.mock.calls[0]![0]
    setViewportSpy.mockClear()

    await act(async () => {
      triggerResizeObserver(720, 760)
    })

    await waitFor(() => expect(setViewportSpy).toHaveBeenCalledTimes(1))
    const [resizedViewport, resizedOptions] = setViewportSpy.mock.calls[0]
    expect(resizedOptions).toEqual({ duration: 180 })
    expect(resizedViewport.zoom).toBeGreaterThan(0)
    expect(resizedViewport.zoom).toBeLessThanOrEqual(1)
    expect(resizedViewport.x).toBeGreaterThan(0)
    expect(resizedViewport.y).toBeGreaterThan(0)
    expect(resizedViewport.x).not.toBe(initialViewport.x)
  })

  it('continues refitting the new-agent canvas after repeated sidebar width changes', async () => {
    installResizeObserverStub()
    mockElementBounds(1100, 760)
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    render(<AgentVisualization detail={null} editing mode="create" />)

    await waitFor(() => expect(setViewportSpy).toHaveBeenCalledTimes(1))
    setViewportSpy.mockClear()

    for (const width of [1000, 900, 800, 1300]) {
      await act(async () => {
        triggerResizeObserver(width, 760)
      })
      await waitFor(() =>
        expect(setViewportSpy, `width ${width}`).toHaveBeenCalledTimes(1),
      )
      const [viewport, options] = setViewportSpy.mock.calls[0]
      expect(options).toEqual({ duration: 180 })
      expect(viewport.zoom).toBeGreaterThan(0)
      expect(viewport.zoom).toBeLessThanOrEqual(1)
      expect(viewport.x).toBeGreaterThan(0)
      expect(viewport.y).toBeGreaterThan(0)
      setViewportSpy.mockClear()
    }
  })

  it('refits the new-agent canvas when a hidden workflow pane becomes active', async () => {
    installResizeObserverStub()
    mockElementBounds(720, 760)
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    const { rerender } = render(
      <AgentVisualization active={false} detail={null} editing mode="create" />,
    )

    expect(setViewportSpy).not.toHaveBeenCalled()
    expect(fitViewSpy).not.toHaveBeenCalled()

    rerender(<AgentVisualization active detail={null} editing mode="create" />)

    await waitFor(() => expect(setViewportSpy).toHaveBeenCalledTimes(1))
    expect(fitViewSpy).not.toHaveBeenCalled()
    const [activeViewport, activeOptions] = setViewportSpy.mock.calls[0]
    expect(activeOptions).toEqual({ duration: 0 })
    expect(activeViewport.zoom).toBeGreaterThan(0)
    expect(activeViewport.zoom).toBeLessThanOrEqual(1)
    expect(activeViewport.x).toBeGreaterThan(0)
    expect(activeViewport.y).toBeGreaterThan(0)
  })

  it('refits the new-agent canvas after React Activity hides and shows the workflow pane', async () => {
    installResizeObserverStub()
    mockElementBounds(720, 760)
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      callback(0)
      return 1
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})

    function ActivityWrappedCanvas({ active }: { active: boolean }) {
      return (
        <Activity mode={active ? 'visible' : 'hidden'} name="workflow-pane">
          <AgentVisualization active={active} detail={null} editing mode="create" />
        </Activity>
      )
    }

    const { rerender } = render(<ActivityWrappedCanvas active />)

    await waitFor(() => expect(setViewportSpy).toHaveBeenCalledTimes(1))
    setViewportSpy.mockClear()

    rerender(<ActivityWrappedCanvas active={false} />)
    expect(setViewportSpy).not.toHaveBeenCalled()

    rerender(<ActivityWrappedCanvas active />)

    await waitFor(() => expect(setViewportSpy).toHaveBeenCalledTimes(1))
    expect(fitViewSpy).not.toHaveBeenCalled()
    const [visibleAgainViewport, visibleAgainOptions] = setViewportSpy.mock.calls[0]
    expect(visibleAgainOptions).toEqual({ duration: 0 })
    expect(visibleAgainViewport.zoom).toBeGreaterThan(0)
    expect(visibleAgainViewport.zoom).toBeLessThanOrEqual(1)
    expect(visibleAgainViewport.x).toBeGreaterThan(0)
    expect(visibleAgainViewport.y).toBeGreaterThan(0)
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
    const readNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool:Read"]',
    )
    const grepNode = container.querySelector<HTMLElement>(
      '.react-flow__node[data-id="tool:Grep"]',
    )

    expect(frameNode).not.toBeNull()
    expect(readNode).not.toBeNull()
    expect(grepNode).not.toBeNull()
    expect(container.querySelector('.agent-tool-group-frame__drag-surface')).toBeNull()
    expect(frameNode!.style.pointerEvents).toBe('none')
    expect(readNode!.style.pointerEvents).toBe('all')

    fireEvent.pointerMove(readNode!, { buttons: 0, clientX: 10, clientY: 10 })

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

  it('removes a whole tool category from the authoring frame button', async () => {
    installResizeObserverStub()

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
    multiToolDetail.output.sections = [
      {
        id: 'files_changed',
        label: 'Files Changed',
        description: 'Per-file summary.',
        emphasis: 'core',
        producedByTools: ['Read', 'Grep'],
      },
    ]
    multiToolDetail.dbTouchpoints.reads = [
      {
        table: 'agent_runs',
        kind: 'read',
        purpose: 'reads run state',
        triggers: [{ kind: 'tool', name: 'Read' }],
        columns: [],
      },
    ]

    const { container, getByLabelText } = render(
      <AgentVisualization
        detail={null}
        editing
        mode="edit"
        initialDetail={multiToolDetail}
      />,
    )

    const removeCategory = getByLabelText('Remove Core tool category')
    fireEvent.pointerDown(removeCategory)
    fireEvent.click(removeCategory)

    await waitFor(() => {
      expect(container.querySelector('.react-flow__node[data-id="tool:Read"]')).toBeNull()
      expect(container.querySelector('.react-flow__node[data-id="tool:Grep"]')).toBeNull()
      expect(
        container.querySelector('.react-flow__node[data-id="tool-group-frame:core"]'),
      ).toBeNull()
    })
    expect(container.textContent).toContain('0tools')
  })

  it('locks canvas movement while compact cards stay non-expandable', () => {
    installResizeObserverStub()

    const { container, getByLabelText } = render(
      <AgentVisualization
        detail={null}
        editing
        mode="edit"
        initialDetail={detail()}
      />,
    )
    const canvas = container.querySelector<HTMLElement>('.agent-visualization')
    const lockButton = getByLabelText('Lock canvas') as HTMLButtonElement
    const snapButton = getByLabelText(/snap to grid/i) as HTMLButtonElement
    const resetButton = getByLabelText('Reset layout') as HTMLButtonElement
    const toolNode = container.querySelector<HTMLElement>('.react-flow__node[data-id="tool:Read"]')
    const toolCard = toolNode?.querySelector<HTMLElement>('.agent-card')

    expect(canvas).not.toBeNull()
    expect(toolNode).not.toBeNull()
    expect(toolCard).not.toBeNull()
    expect(toolNode!.querySelector('button')).toBeNull()

    fireEvent.click(lockButton)

    expect(canvas!.classList.contains('is-locked')).toBe(true)
    expect(getByLabelText('Unlock canvas')).toBe(lockButton)
    expect(lockButton.getAttribute('aria-pressed')).toBe('true')
    expect(snapButton.disabled).toBe(true)
    expect(resetButton.disabled).toBe(true)

    fireEvent.click(toolNode!)
    expect(toolCard!.classList.contains('is-card-expanded')).toBe(false)

    fireEvent.click(lockButton)

    expect(canvas!.classList.contains('is-locked')).toBe(false)
    expect(getByLabelText('Lock canvas')).toBe(lockButton)
    expect(snapButton.disabled).toBe(false)
    expect(resetButton.disabled).toBe(false)

    fireEvent.click(toolNode!)
    expect(toolCard!.classList.contains('is-card-expanded')).toBe(false)
  })

  it('keeps tool cards compact when clicked inside a category frame', () => {
    installResizeObserverStub()

    const { container } = render(<AgentVisualization detail={detail()} />)
    const toolNode = container.querySelector<HTMLElement>('.react-flow__node[data-id="tool:Read"]')
    const toolCard = toolNode?.querySelector<HTMLElement>('.agent-card')

    expect(toolNode).not.toBeNull()
    expect(toolCard).not.toBeNull()
    expect(toolNode!.style.pointerEvents).toBe('all')
    expect(toolNode!.querySelector('button')).toBeNull()
    expect(toolCard!.classList.contains('is-card-expanded')).toBe(false)

    fireEvent.pointerDown(toolNode!, { button: 0 })
    fireEvent.click(toolNode!)

    expect(toolCard!.classList.contains('is-card-expanded')).toBe(false)
  })
})
