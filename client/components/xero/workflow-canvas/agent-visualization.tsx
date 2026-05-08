'use client'

import {
  startTransition,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import {
  applyNodeChanges,
  ControlButton,
  Controls,
  Position,
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useOnViewportChange,
  useReactFlow,
  useUpdateNodeInternals,
  type Edge,
  type EdgeTypes,
  type Node,
  type NodeChange,
  type OnNodeDrag,
  type NodeTypes,
  type Viewport,
  type XYPosition,
} from '@xyflow/react'
import { LayoutGrid, Magnet } from 'lucide-react'

import '@xyflow/react/dist/style.css'

import {
  agentRefKey,
  type AgentTriggerRefDto,
  type WorkflowAgentDetailDto,
} from '@/src/lib/xero-model/workflow-agents'

import {
  AGENT_GRAPH_HEADER_HANDLES,
  buildAgentGraph,
  humanizeIdentifier,
  lifecycleEventLabel,
  type AgentGraphEdge,
  type AgentGraphNode,
} from './build-agent-graph'
import {
  AgentCanvasExpansionContext,
  type AgentCanvasExpansionContextValue,
} from './expansion-context'
import { TriggerEdge } from './edges/trigger-edge'
import { layoutAgentGraphByCategory, type NodeSize } from './layout'
import { AgentHeaderNode } from './nodes/agent-header-node'
import { ConsumedArtifactNode } from './nodes/consumed-artifact-node'
import { DbTableNode } from './nodes/db-table-node'
import { LaneLabelNode } from './nodes/lane-label-node'
import { OutputNode } from './nodes/output-node'
import { OutputSectionNode } from './nodes/output-section-node'
import { PromptNode } from './nodes/prompt-node'
import { ToolGroupFrameNode } from './nodes/tool-group-frame-node'
import { ToolNode } from './nodes/tool-node'

import './agent-visualization.css'

const NODE_TYPES = {
  'agent-header': AgentHeaderNode,
  prompt: PromptNode,
  tool: ToolNode,
  'db-table': DbTableNode,
  'agent-output': OutputNode,
  'output-section': OutputSectionNode,
  'consumed-artifact': ConsumedArtifactNode,
  'lane-label': LaneLabelNode,
  'tool-group-frame': ToolGroupFrameNode,
} as unknown as NodeTypes

// Custom edge types. Trigger edges render their labels via the
// `EdgeLabelRenderer` portal so the labels sit above the node layer instead
// of inside the edges SVG, where they'd dim/clip behind any card the edge
// happens to cross.
const EDGE_TYPES = {
  trigger: TriggerEdge,
} as unknown as EdgeTypes

const REACT_FLOW_PRO_OPTIONS = { hideAttribution: true } as const
const FIT_VIEW_OPTIONS = { padding: 0.16, includeHiddenNodes: false } as const
const FIT_VIEW_TRANSITION_MS = 420
const AGENT_EXIT_TRANSITION_MS = 220
const EMPTY_CANVAS_DEFAULT_VIEWPORT = { x: 0, y: 0, zoom: 0.72 } as const
const DEFAULT_EDGE_OPTIONS = {
  type: 'smoothstep',
  animated: false,
  interactionWidth: 0,
} as const

const NODE_SIZE_BY_TYPE: Record<string, NodeSize> = {
  'agent-header': { width: 300, height: 210 },
  prompt: { width: 300, height: 96 },
  // Tool / output-section heights are intentionally close to the rendered
  // collapsed-card height so layout doesn't pad the column with empty
  // vertical slack between rows. Expansion delta is added separately via
  // EXPANDED_BODY_EXTRA when the user opens a card.
  tool: { width: 240, height: 36 },
  'db-table': { width: 260, height: 116 },
  'agent-output': { width: 300, height: 110 },
  'output-section': { width: 200, height: 32 },
  'consumed-artifact': { width: 260, height: 104 },
}

// Fallback extra height for an expanded card body, used only when React Flow
// has not yet reported a measurement for the node (e.g. the very first frame
// after expanding, or in test environments that stub ResizeObserver). Once a
// measurement arrives, the measured height drives the layout instead of these
// estimates so neighbours never push further than the actual rendered body.
const EXPANDED_BODY_EXTRA: Partial<Record<string, number>> = {
  'agent-header': 80,
  prompt: 200,
  tool: 90,
  'output-section': 80,
  'consumed-artifact': 130,
}

const POSITIONS_STORAGE_PREFIX = 'xero.workflows.canvas-positions:'
const SNAP_TO_GRID_STORAGE_KEY = 'xero.workflows.canvas-snap-to-grid'
const INTERACTION_SETTLE_MS = 110
const EXPANSION_MEASUREMENT_SETTLE_MS = 280
const DOT_GRID_GAP = 32
// Snap step is locked to half the visual dot-grid spacing so every snap stop
// is either directly on a dot or exactly between two adjacent dots. Keeps the
// snapping visually tied to the canvas pattern instead of feeling arbitrary.
const SNAP_GRID_SIZE = DOT_GRID_GAP / 2
const SNAP_GRID: [number, number] = [SNAP_GRID_SIZE, SNAP_GRID_SIZE]
const DOT_COORD_PRECISION = 10
const DOT_ZOOM_PRECISION = 1000
// React Flow's visibility culling recomputes visible node/edge id arrays on
// every viewport transform. Agent canvases are dense enough that a stable DOM
// moved by the viewport transform is smoother than per-frame culling work.
const ONLY_RENDER_VISIBLE_ELEMENTS = false
const FIT_VIEW_ON_INIT = import.meta.env.MODE !== 'test'
const HANDLE_SIZE = 8
const FOCUS_BULK_DOM_LOOKUP_THRESHOLD = 24
const TRIGGER_EDGE_ID_PREFIX = 'e:trigger:'
const EXPANDED_NODE_CLASS = 'agent-node-expanded'
const DB_COLLAPSED_TRIGGER_LIMIT = 3
const DB_CARD_INNER_WIDTH = 240
const DB_CHIP_GAP = 4
const DB_HEADER_HEIGHT = 32
const DB_PURPOSE_CHARS_PER_LINE = 38
const DB_PURPOSE_LINE_HEIGHT = 14
const DB_CHIP_ROW_HEIGHT = 24
const DB_CHIP_BOTTOM_PADDING = 6
const DB_EXPAND_ROW_HEIGHT = 29
const DB_COLUMNS_LABEL_HEIGHT = 16
const DB_COLUMNS_CHARS_PER_LINE = 36
const DB_COLUMNS_LINE_HEIGHT = 14
const DB_COLUMNS_VERTICAL_PADDING = 10
const DB_CARD_BORDER_HEIGHT = 2
const EMPTY_AGENT_GRAPH: {
  nodes: AgentGraphNode[]
  edges: AgentGraphEdge[]
} = { nodes: [], edges: [] }

type StoredPositions = Record<string, { x: number; y: number }>

type LaneDragCategory =
  | 'prompt'
  | 'tool'
  | 'db-table'
  | 'agent-output'
  | 'output-section'
  | 'consumed-artifact'

const LANE_NODE_PREFIX = 'lane:'

interface AgentVisualizationProps {
  detail?: WorkflowAgentDetailDto | null
  emptyState?: ReactNode
  emptyStateVisible?: boolean
}

interface FocusIndex {
  edgeIdsByNodeId: Map<string, Set<string>>
  edgeById: Map<string, Edge>
  nodeTypeById: Map<string, string | undefined>
  // Parent (frame) for each node, populated for tools that live inside a
  // tool-group-frame. Hover focus walks this chain so hovering a tool also
  // lights up its parent frame and the header → frame edge.
  parentIdByNodeId: Map<string, string>
  // Inverse of the parent map. Hovering a frame uses this to pull all child
  // tools into the focused set, so the entire category visually pops.
  childIdsByParent: Map<string, Set<string>>
}

interface FocusTarget {
  nodeIds: string[]
  edgeIds: string[]
}

interface AppliedFocus {
  nodeElements: HTMLElement[]
  edgeElements: SVGElement[]
  edgeLabelElements: HTMLElement[]
}

interface DotViewportElement extends HTMLElement {
  __agentDotTransform?: string
  __agentDotZoomKey?: string
}

interface DbTableCardHeightInput {
  purpose?: string
  triggers?: readonly AgentTriggerRefDto[]
  columns?: readonly string[]
  expanded?: boolean
}

interface DbTriggerChipEstimate {
  label: string
  hasIcon: boolean
}

function dbTriggerChipLabelText(trigger: AgentTriggerRefDto): DbTriggerChipEstimate | null {
  switch (trigger.kind) {
    case 'tool':
      return { label: `Tool: ${humanizeIdentifier(trigger.name)}`, hasIcon: false }
    case 'output_section':
      return { label: `Section: ${humanizeIdentifier(trigger.id)}`, hasIcon: false }
    case 'lifecycle':
      return { label: `on ${lifecycleEventLabel(trigger.event)}`, hasIcon: true }
    case 'upstream_artifact':
      return { label: `from ${humanizeIdentifier(trigger.id)}`, hasIcon: true }
  }
}

function estimateTextLineCount(text: string | undefined, charsPerLine: number): number {
  const length = text?.trim().length ?? 0
  if (length === 0) return 0
  return Math.max(1, Math.ceil(length / charsPerLine))
}

function estimateDbTriggerChipWidth(chip: DbTriggerChipEstimate): number {
  const textWidth = chip.label.length * 5.8
  const iconWidth = chip.hasIcon ? 14 : 0
  return Math.min(DB_CARD_INNER_WIDTH, Math.ceil(textWidth + iconWidth + 14))
}

function estimateHiddenDbTriggerChipWidth(hiddenCount: number): number {
  return Math.min(DB_CARD_INNER_WIDTH, 18 + `${hiddenCount}`.length * 6 + 26)
}

function estimateWrappedRows(widths: readonly number[], rowWidth: number): number {
  if (widths.length === 0) return 0

  let rows = 1
  let used = 0
  for (const width of widths) {
    const clamped = Math.min(width, rowWidth)
    const next = used === 0 ? clamped : used + DB_CHIP_GAP + clamped
    if (used > 0 && next > rowWidth) {
      rows++
      used = clamped
    } else {
      used = next
    }
  }
  return rows
}

export function estimateDbTableCardHeight({
  purpose,
  triggers = [],
  columns = [],
  expanded = false,
}: DbTableCardHeightInput): number {
  const canExpand = columns.length > 0
  const isExpanded = canExpand && expanded
  const chips = triggers
    .map(dbTriggerChipLabelText)
    .filter((chip): chip is DbTriggerChipEstimate => chip !== null)
  const visibleChips =
    canExpand && !isExpanded ? chips.slice(0, DB_COLLAPSED_TRIGGER_LIMIT) : chips
  const hiddenCount = canExpand ? Math.max(0, chips.length - visibleChips.length) : 0
  const chipWidths = visibleChips.map(estimateDbTriggerChipWidth)
  if (hiddenCount > 0) {
    chipWidths.push(estimateHiddenDbTriggerChipWidth(hiddenCount))
  }

  const purposeLines = estimateTextLineCount(purpose, DB_PURPOSE_CHARS_PER_LINE)
  const chipRows = estimateWrappedRows(chipWidths, DB_CARD_INNER_WIDTH)
  let height = DB_CARD_BORDER_HEIGHT + DB_HEADER_HEIGHT

  if (purposeLines > 0) {
    height += purposeLines * DB_PURPOSE_LINE_HEIGHT + DB_CHIP_BOTTOM_PADDING
  }
  if (chipRows > 0) {
    height +=
      chipRows * DB_CHIP_ROW_HEIGHT +
      Math.max(0, chipRows - 1) * DB_CHIP_GAP +
      DB_CHIP_BOTTOM_PADDING
  }
  if (isExpanded) {
    const columnLines = Math.max(
      1,
      estimateTextLineCount(columns.join(', '), DB_COLUMNS_CHARS_PER_LINE),
    )
    height +=
      DB_COLUMNS_VERTICAL_PADDING +
      DB_COLUMNS_LABEL_HEIGHT +
      columnLines * DB_COLUMNS_LINE_HEIGHT
  }
  if (canExpand) {
    height += DB_EXPAND_ROW_HEIGHT
  }

  return Math.max(NODE_SIZE_BY_TYPE['db-table'].height, Math.ceil(height))
}

function shouldUseMeasuredHeightForLayout(
  node: AgentGraphNode,
  expanded: boolean,
): boolean {
  switch (node.type) {
    case 'agent-header':
    case 'prompt':
    case 'tool':
    case 'output-section':
    case 'consumed-artifact':
      return expanded
    // DB cards have data-dependent collapsed heights, and the output card has
    // free-form descriptive text. Let their measured heights drive layout.
    case 'db-table':
    case 'agent-output':
      return true
    default:
      return true
  }
}

function buildSizeMap(
  nodes: AgentGraphNode[],
  expandedIds: ReadonlySet<string>,
  measuredSizes: ReadonlyMap<string, NodeSize>,
): Map<string, NodeSize> {
  const map = new Map<string, NodeSize>()
  for (const node of nodes) {
    if (!node.type) continue
    const declaredBase = NODE_SIZE_BY_TYPE[node.type] ?? { width: 280, height: 120 }
    const expanded = expandedIds.has(node.id)
    const base =
      node.type === 'db-table'
        ? {
            width: declaredBase.width,
            height: estimateDbTableCardHeight({ ...node.data, expanded }),
          }
        : declaredBase
    // Prefer the actual rendered height — that way neighbours displace by
    // exactly how much the card grew rather than by a conservative estimate.
    // Width is always controlled by an inline style on the card so we don't
    // honour a measured width here (it would only echo the design value).
    const measured = measuredSizes.get(node.id)
    if (measured && shouldUseMeasuredHeightForLayout(node, expanded)) {
      map.set(node.id, { width: base.width, height: measured.height })
      continue
    }
    const extra =
      node.type === 'db-table' ? 0 : expanded ? EXPANDED_BODY_EXTRA[node.type] ?? 0 : 0
    map.set(node.id, { width: base.width, height: base.height + extra })
  }
  return map
}

function storageKeyFor(detail: WorkflowAgentDetailDto): string {
  return `${POSITIONS_STORAGE_PREFIX}${agentRefKey(detail.ref)}`
}

function readStoredPositions(key: string): StoredPositions {
  if (typeof window === 'undefined') return {}
  try {
    const raw = window.localStorage.getItem(key)
    if (!raw) return {}
    const parsed = JSON.parse(raw)
    if (!parsed || typeof parsed !== 'object') return {}
    const out: StoredPositions = {}
    for (const [id, value] of Object.entries(parsed as Record<string, unknown>)) {
      if (
        value &&
        typeof value === 'object' &&
        typeof (value as { x?: unknown }).x === 'number' &&
        typeof (value as { y?: unknown }).y === 'number'
      ) {
        out[id] = {
          x: (value as { x: number }).x,
          y: (value as { y: number }).y,
        }
      }
    }
    return out
  } catch {
    return {}
  }
}

function writeStoredPositions(key: string, positions: StoredPositions): void {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(key, JSON.stringify(positions))
  } catch {
    // Best-effort; storage may be disabled or full.
  }
}

function readSnapToGridPreference(): boolean {
  if (typeof window === 'undefined') return true
  try {
    const raw = window.localStorage.getItem(SNAP_TO_GRID_STORAGE_KEY)
    if (raw === null) return true
    return raw === 'true'
  } catch {
    return true
  }
}

function writeSnapToGridPreference(enabled: boolean): void {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(SNAP_TO_GRID_STORAGE_KEY, enabled ? 'true' : 'false')
  } catch {
    // Best-effort; storage may be disabled or full.
  }
}

function roundCanvasCoord(value: number): number {
  return Math.round(value)
}

function normalizeMeasuredSize(size: NodeSize): NodeSize {
  return {
    width: Math.max(1, Math.round(size.width)),
    height: Math.max(1, Math.round(size.height)),
  }
}

export function estimateExpandedCardHeight(
  cardHeight: number,
  bodyWrapperHeight: number,
  bodyScrollHeight: number,
  expanded: boolean,
): number {
  const stableChromeHeight = Math.max(0, cardHeight - bodyWrapperHeight)
  if (!expanded) return stableChromeHeight
  return stableChromeHeight + Math.max(0, bodyScrollHeight)
}

function sameMeasuredSize(left: NodeSize | undefined, right: NodeSize): boolean {
  return Boolean(left && left.width === right.width && left.height === right.height)
}

function isPersistableNode(node: AgentGraphNode | Node): boolean {
  return Boolean(node.type)
}

function laneCategoryFromNodeId(nodeId: string): LaneDragCategory | null {
  if (!nodeId.startsWith(LANE_NODE_PREFIX)) return null
  const category = nodeId.slice(LANE_NODE_PREFIX.length)
  switch (category) {
    case 'prompt':
    case 'tool':
    case 'db-table':
    case 'agent-output':
    case 'output-section':
    case 'consumed-artifact':
      return category
    default:
      return null
  }
}

function isLaneLabelNodeId(nodeId: string): boolean {
  return laneCategoryFromNodeId(nodeId) !== null
}

function shouldRefreshNodeInternals(node: Node): boolean {
  return Boolean(node.type && node.type !== 'lane-label')
}

function nodeInternalsRefreshKey(nodes: readonly Node[]): string {
  const parts: string[] = []
  for (const node of nodes) {
    if (!shouldRefreshNodeInternals(node)) continue
    parts.push(
      [
        node.id,
        node.type ?? '',
        node.parentId ?? '',
        node.width ?? '',
        node.height ?? '',
        node.initialWidth ?? '',
        node.initialHeight ?? '',
        node.measured?.width ?? '',
        node.measured?.height ?? '',
      ].join(':'),
    )
  }
  return parts.join('|')
}

function refreshableNodeIds(nodes: readonly Node[]): string[] {
  const ids: string[] = []
  for (const node of nodes) {
    if (shouldRefreshNodeInternals(node)) ids.push(node.id)
  }
  return ids
}

function nodeMovesWithLane(node: Node, category: LaneDragCategory): boolean {
  switch (category) {
    case 'prompt':
      return node.type === 'prompt'
    case 'tool':
      // Tool cards are children of their group frames. Moving the frame moves
      // the cards visually while preserving each card's relative coordinates.
      return node.type === 'tool-group-frame'
    case 'db-table':
      return node.type === 'db-table'
    case 'agent-output':
      return node.type === 'agent-output'
    case 'output-section':
      return node.type === 'output-section'
    case 'consumed-artifact':
      return node.type === 'consumed-artifact'
  }
}

export function getLaneDragMemberIds(
  nodes: readonly Node[],
  laneNodeId: string,
): Set<string> {
  const category = laneCategoryFromNodeId(laneNodeId)
  if (!category) return new Set()

  const ids = new Set<string>([laneNodeId])
  for (const node of nodes) {
    if (nodeMovesWithLane(node, category)) ids.add(node.id)
  }
  return ids
}

function applyPositionDelta(position: XYPosition, dx: number, dy: number): XYPosition {
  return {
    x: position.x + dx,
    y: position.y + dy,
  }
}

export function applyLaneDragPositionChanges(
  current: Node[],
  changes: readonly NodeChange<Node>[],
): Node[] {
  let next = current

  for (const change of changes) {
    if (
      change.type !== 'position' ||
      !change.position ||
      !isLaneLabelNodeId(change.id)
    ) {
      continue
    }

    const laneNode = next.find((node) => node.id === change.id)
    if (!laneNode) continue

    const dx = change.position.x - laneNode.position.x
    const dy = change.position.y - laneNode.position.y
    const memberIds = getLaneDragMemberIds(next, change.id)
    if (memberIds.size === 0) continue

    next = next.map((node) => {
      if (!memberIds.has(node.id)) return node

      const position =
        node.id === change.id
          ? change.position!
          : applyPositionDelta(node.position, dx, dy)

      if (node.id === change.id && change.dragging !== undefined) {
        return { ...node, position, dragging: change.dragging }
      }
      return { ...node, position }
    })
  }

  return next
}

function applyStoredPositions(
  nodes: AgentGraphNode[],
  stored: StoredPositions,
): AgentGraphNode[] {
  if (Object.keys(stored).length === 0) return nodes
  return nodes.map((node) => {
    if (!isPersistableNode(node)) return node
    const saved = stored[node.id]
    if (!saved) return node
    return { ...node, position: { x: saved.x, y: saved.y } }
  })
}

function toggleClassToken(
  className: string | undefined,
  token: string,
  enabled: boolean,
): string | undefined {
  const tokens = className?.split(/\s+/).filter(Boolean) ?? []
  const hasToken = tokens.includes(token)
  if (enabled === hasToken) return className
  const nextTokens = enabled
    ? [...tokens, token]
    : tokens.filter((existing) => existing !== token)
  return nextTokens.length > 0 ? nextTokens.join(' ') : undefined
}

function applyExpandedNodeClass(
  nodes: AgentGraphNode[],
  expandedIds: ReadonlySet<string>,
): AgentGraphNode[] {
  return nodes.map((node) => {
    const className = toggleClassToken(
      node.className,
      EXPANDED_NODE_CLASS,
      expandedIds.has(node.id),
    )
    return className === node.className ? node : { ...node, className }
  })
}

function nodeHandle(
  type: 'source' | 'target',
  position: Position,
  nodeWidth: number,
  nodeHeight: number,
  id?: string,
  offset?: { x?: number; y?: number },
) {
  const half = HANDLE_SIZE / 2
  const x =
    offset?.x ??
    (position === Position.Left
      ? -half
      : position === Position.Right
        ? nodeWidth - half
        : nodeWidth / 2 - half)
  const y =
    offset?.y ??
    (position === Position.Top
      ? -half
      : position === Position.Bottom
        ? nodeHeight - half
        : nodeHeight / 2 - half)

  return {
    id,
    type,
    position,
    x,
    y,
    width: HANDLE_SIZE,
    height: HANDLE_SIZE,
  }
}

function knownHandlesForNode(node: AgentGraphNode, width: number, height: number) {
  switch (node.type) {
    case 'agent-header':
      return [
        nodeHandle('source', Position.Top, width, height, AGENT_GRAPH_HEADER_HANDLES.prompt),
        nodeHandle('source', Position.Right, width, height, AGENT_GRAPH_HEADER_HANDLES.tool),
        nodeHandle('source', Position.Right, width, height, AGENT_GRAPH_HEADER_HANDLES.db),
        nodeHandle('source', Position.Bottom, width, height, AGENT_GRAPH_HEADER_HANDLES.output),
        nodeHandle('target', Position.Left, width, height, AGENT_GRAPH_HEADER_HANDLES.consumed),
      ]
    case 'prompt':
      return [nodeHandle('target', Position.Bottom, width, height)]
    case 'tool': {
      const handles: KnownNodeHandle[] = []
      if (node.data.directConnectionHandles.target) {
        handles.push(nodeHandle('target', Position.Left, width, height))
      }
      if (node.data.directConnectionHandles.source) {
        handles.push(nodeHandle('source', Position.Right, width, height))
      }
      return handles
    }
    case 'tool-group-frame':
      return [nodeHandle('target', Position.Left, width, height)]
    case 'db-table':
      return [nodeHandle('target', Position.Left, width, height)]
    case 'agent-output':
      return [
        nodeHandle('target', Position.Top, width, height),
        nodeHandle('source', Position.Bottom, width, height),
      ]
    case 'output-section':
      return [
        nodeHandle('target', Position.Top, width, height),
        nodeHandle('source', Position.Right, width, height),
      ]
    case 'consumed-artifact':
      return [nodeHandle('source', Position.Right, width, height)]
    default:
      return undefined
  }
}

type KnownNodeHandle = ReturnType<typeof nodeHandle>

function sameKnownNodeHandles(
  left: readonly KnownNodeHandle[] | undefined,
  right: readonly KnownNodeHandle[] | undefined,
): boolean {
  if (left === right) return true
  if (!left || !right || left.length !== right.length) return false
  for (let i = 0; i < left.length; i++) {
    const a = left[i]
    const b = right[i]
    if (
      a.id !== b.id ||
      a.type !== b.type ||
      a.position !== b.position ||
      a.x !== b.x ||
      a.y !== b.y ||
      a.width !== b.width ||
      a.height !== b.height
    ) {
      return false
    }
  }
  return true
}

export function applyKnownNodeDimensions(
  nodes: AgentGraphNode[],
  sizes: ReadonlyMap<string, NodeSize>,
): AgentGraphNode[] {
  // Seed React Flow with deterministic dimensions/handles so initial edge
  // routing does not wait on ResizeObserver and tests do not depend on DOM
  // layout APIs that JSDOM lacks.
  return nodes.map((node) => {
    if (node.type === 'lane-label') return node
    const fallback = sizes.get(node.id)
    const width = typeof node.width === 'number' ? node.width : fallback?.width
    const height = typeof node.height === 'number' ? node.height : fallback?.height
    if (typeof width !== 'number' || typeof height !== 'number') return node
    const handles = knownHandlesForNode(node, width, height)
    const pinsWrapperSize = node.type === 'tool-group-frame'
    const nextWidth = pinsWrapperSize ? width : undefined
    const nextHeight = pinsWrapperSize ? height : undefined
    if (
      node.width === nextWidth &&
      node.height === nextHeight &&
      node.initialWidth === width &&
      node.initialHeight === height &&
      node.measured?.width === width &&
      node.measured?.height === height &&
      sameKnownNodeHandles(node.handles as KnownNodeHandle[] | undefined, handles)
    ) {
      return node
    }
    return {
      ...node,
      width: nextWidth,
      height: nextHeight,
      initialWidth: width,
      initialHeight: height,
      measured: { width, height },
      handles,
    }
  })
}

function findRenderedNodeElement(
  root: HTMLElement | null,
  nodeId: string,
): HTMLElement | null {
  if (!root) return null
  for (const node of root.querySelectorAll<HTMLElement>('.react-flow__node[data-id]')) {
    if (node.getAttribute('data-id') === nodeId) return node
  }
  return null
}

function estimateRenderedNodeSize(
  root: HTMLElement | null,
  nodeId: string,
  expanded: boolean,
): NodeSize | null {
  const nodeElement = findRenderedNodeElement(root, nodeId)
  const card = nodeElement?.querySelector<HTMLElement>('.agent-card')
  if (!card) return null

  const cardRect = card.getBoundingClientRect()
  if (cardRect.height <= 0) return null

  const bodyWrapper = card.querySelector<HTMLElement>('.agent-card-body-wrapper')
  const body = bodyWrapper?.querySelector<HTMLElement>('.agent-card-body')
  const wrapperHeight = bodyWrapper?.getBoundingClientRect().height ?? 0
  const bodyScrollHeight = body
    ? Math.max(body.scrollHeight, body.getBoundingClientRect().height)
    : 0

  return normalizeMeasuredSize({
    width: cardRect.width,
    height:
      bodyWrapper && body
        ? estimateExpandedCardHeight(
            cardRect.height,
            wrapperHeight,
            bodyScrollHeight,
            expanded,
          )
        : cardRect.height,
  })
}

function buildFocusIndex(
  nodes: readonly Node[],
  edges: readonly Edge[],
): FocusIndex {
  const edgeIdsByNodeId = new Map<string, Set<string>>()
  const edgeById = new Map<string, Edge>()
  const nodeTypeById = new Map<string, string | undefined>()
  const parentIdByNodeId = new Map<string, string>()
  const childIdsByParent = new Map<string, Set<string>>()
  const add = (nodeId: string, edgeId: string) => {
    const set = edgeIdsByNodeId.get(nodeId) ?? new Set<string>()
    set.add(edgeId)
    edgeIdsByNodeId.set(nodeId, set)
  }

  for (const edge of edges) {
    edgeById.set(edge.id, edge)
    add(edge.source, edge.id)
    add(edge.target, edge.id)
  }

  for (const node of nodes) {
    nodeTypeById.set(node.id, node.type)
    const parent = (node as Node).parentId
    if (!parent) continue
    parentIdByNodeId.set(node.id, parent)
    const set = childIdsByParent.get(parent) ?? new Set<string>()
    set.add(node.id)
    childIdsByParent.set(parent, set)
  }

  return { edgeIdsByNodeId, edgeById, nodeTypeById, parentIdByNodeId, childIdsByParent }
}

function buildFocusTarget(nodeId: string, index: FocusIndex): FocusTarget | null {
  const nodeType = index.nodeTypeById.get(nodeId)
  if (!nodeType || nodeType === 'lane-label') return null

  const focusedNodes = new Set<string>()
  const focusedEdges = new Set<string>()

  const visit = (id: string, options: { includeChildConnections?: boolean } = {}) => {
    if (focusedNodes.has(id)) return
    focusedNodes.add(id)
    const incident = index.edgeIdsByNodeId.get(id)
    if (incident) {
      for (const edgeId of incident) {
        const edge = index.edgeById.get(edgeId)
        if (!edge) continue
        focusedEdges.add(edge.id)
        focusedNodes.add(edge.source)
        focusedNodes.add(edge.target)
      }
    }
    const children = options.includeChildConnections ? index.childIdsByParent.get(id) : undefined
    if (children) {
      for (const childId of children) {
        visit(childId)
      }
    }
  }

  visit(nodeId, { includeChildConnections: nodeType === 'tool-group-frame' })

  let cursor: string | undefined = index.parentIdByNodeId.get(nodeId)
  const seenAncestors = new Set<string>()
  while (cursor && !seenAncestors.has(cursor)) {
    seenAncestors.add(cursor)
    visit(cursor)
    cursor = index.parentIdByNodeId.get(cursor)
  }

  return {
    nodeIds: Array.from(focusedNodes),
    edgeIds: Array.from(focusedEdges),
  }
}

function cssString(value: string): string {
  return `"${value
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\n/g, '\\a ')
    .replace(/\r/g, '\\d ')}"`
}

function getFocusedDomElements(root: HTMLElement, target: FocusTarget): AppliedFocus {
  const nodeElements: HTMLElement[] = []
  const edgeElements: SVGElement[] = []
  const edgeLabelElements: HTMLElement[] = []
  const targetSize = target.nodeIds.length + target.edgeIds.length

  if (targetSize > FOCUS_BULK_DOM_LOOKUP_THRESHOLD) {
    const nodeIds = new Set(target.nodeIds)
    const edgeIds = new Set(target.edgeIds)
    const hasTriggerEdge = target.edgeIds.some((id) =>
      id.startsWith(TRIGGER_EDGE_ID_PREFIX),
    )
    root.querySelectorAll<HTMLElement>('.react-flow__node[data-id]').forEach((nodeElement) => {
      const id = nodeElement.getAttribute('data-id')
      if (id && nodeIds.has(id)) nodeElements.push(nodeElement)
    })
    root.querySelectorAll<SVGElement>('.react-flow__edge[data-id]').forEach((edgeElement) => {
      const id = edgeElement.getAttribute('data-id')
      if (id && edgeIds.has(id)) edgeElements.push(edgeElement)
    })
    if (hasTriggerEdge) {
      root
        .querySelectorAll<HTMLElement>('.agent-edge-trigger-label[data-edge-id]')
        .forEach((labelElement) => {
          const id = labelElement.getAttribute('data-edge-id')
          if (id && edgeIds.has(id)) edgeLabelElements.push(labelElement)
        })
    }

    return { nodeElements, edgeElements, edgeLabelElements }
  }

  for (const focusedNodeId of target.nodeIds) {
    const nodeElement = root.querySelector<HTMLElement>(
      `.react-flow__node[data-id=${cssString(focusedNodeId)}]`,
    )
    if (nodeElement) nodeElements.push(nodeElement)
  }

  for (const focusedEdgeId of target.edgeIds) {
    const edgeElement = root.querySelector<SVGElement>(
      `.react-flow__edge[data-id=${cssString(focusedEdgeId)}]`,
    )
    if (edgeElement) edgeElements.push(edgeElement)
    if (!focusedEdgeId.startsWith(TRIGGER_EDGE_ID_PREFIX)) continue
    root
      .querySelectorAll<HTMLElement>(
        `.agent-edge-trigger-label[data-edge-id=${cssString(focusedEdgeId)}]`,
      )
      .forEach((labelElement) => edgeLabelElements.push(labelElement))
  }

  return { nodeElements, edgeElements, edgeLabelElements }
}

function applyDotViewport(element: HTMLElement | null, viewport: Viewport): void {
  if (!element) return
  const dotElement = element as DotViewportElement
  const zoom = Number.isFinite(viewport.zoom) && viewport.zoom > 0 ? viewport.zoom : 1
  const roundedZoom = Math.max(
    1 / DOT_ZOOM_PRECISION,
    Math.round(zoom * DOT_ZOOM_PRECISION) / DOT_ZOOM_PRECISION,
  )
  const scaledGap = Math.max(1, DOT_GRID_GAP * zoom)
  const dotX = `${Math.round((viewport.x % scaledGap) * DOT_COORD_PRECISION) / DOT_COORD_PRECISION}px`
  const dotY = `${Math.round((viewport.y % scaledGap) * DOT_COORD_PRECISION) / DOT_COORD_PRECISION}px`
  const transform = `translate3d(${dotX}, ${dotY}, 0) scale(${roundedZoom})`
  if (dotElement.__agentDotTransform !== transform) {
    dotElement.__agentDotTransform = transform
    dotElement.style.transform = transform
  }

  const zoomKey = String(Math.round(zoom * DOT_ZOOM_PRECISION))
  if (dotElement.__agentDotZoomKey !== zoomKey) {
    dotElement.__agentDotZoomKey = zoomKey
    const size = `calc(${100 / roundedZoom}% + ${DOT_GRID_GAP * 2}px)`
    dotElement.style.width = size
    dotElement.style.height = size
  }
}

function WorkflowCanvasDots() {
  const ref = useRef<HTMLDivElement | null>(null)
  const reactFlow = useReactFlow()
  const pendingViewportRef = useRef<Viewport | null>(null)
  const pendingFrameRef = useRef<number | null>(null)

  const flushDots = useCallback(() => {
    pendingFrameRef.current = null
    const viewport = pendingViewportRef.current
    pendingViewportRef.current = null
    if (viewport) applyDotViewport(ref.current, viewport)
  }, [])

  const updateDots = useCallback((viewport: Viewport) => {
    pendingViewportRef.current = viewport
    if (pendingFrameRef.current !== null) return

    pendingFrameRef.current = -1
    const frame = window.requestAnimationFrame(flushDots)
    if (pendingFrameRef.current === -1) {
      pendingFrameRef.current = frame
    }
  }, [flushDots])

  useEffect(
    () => () => {
      if (pendingFrameRef.current !== null && pendingFrameRef.current !== -1) {
        window.cancelAnimationFrame(pendingFrameRef.current)
      }
      pendingFrameRef.current = null
      pendingViewportRef.current = null
    },
    [],
  )

  useEffect(() => {
    updateDots(reactFlow.getViewport())
  }, [reactFlow, updateDots])

  useOnViewportChange({
    onChange: updateDots,
  })

  return <div ref={ref} className="agent-visualization__dots" aria-hidden="true" />
}

function AgentVisualizationInner({
  detail = null,
  emptyState,
  emptyStateVisible = detail === null,
}: AgentVisualizationProps) {
  const [renderedDetail, setRenderedDetail] =
    useState<WorkflowAgentDetailDto | null>(() => detail)
  const exitTimerRef = useRef<number | null>(null)
  const emptyStateEntryFrameRef = useRef<number | null>(null)
  const [emptyStateEntryKey, setEmptyStateEntryKey] = useState(0)
  const [emptyStateEntered, setEmptyStateEntered] = useState(false)
  const hasIncomingDetail = detail !== null
  const hasDetail = renderedDetail !== null
  const isAgentExiting = !hasIncomingDetail && renderedDetail !== null
  const emptyStateIsVisible = emptyStateVisible && emptyStateEntered
  const reactFlow = useReactFlow<Node, Edge>()
  const storageKey = useMemo(
    () => (renderedDetail ? storageKeyFor(renderedDetail) : ''),
    [renderedDetail],
  )

  const baseGraph = useMemo(
    () => (renderedDetail ? buildAgentGraph(renderedDetail) : EMPTY_AGENT_GRAPH),
    [renderedDetail],
  )
  const canvasRef = useRef<HTMLDivElement | null>(null)
  const updateNodeInternals = useUpdateNodeInternals()
  // Bumped each time the user invokes "Reset layout" so the memoized layout
  // computation re-runs even when storage was already empty.
  const [resetNonce, setResetNonce] = useState(0)
  const [snapToGrid, setSnapToGrid] = useState<boolean>(() => readSnapToGridPreference())
  useEffect(() => {
    writeSnapToGridPreference(snapToGrid)
  }, [snapToGrid])
  const handleToggleSnapToGrid = useCallback(() => {
    setSnapToGrid((prev) => !prev)
  }, [])
  const storedPositionsRef = useRef<{ key: string; positions: StoredPositions } | null>(
    null,
  )
  const [storedPositionsNonce, setStoredPositionsNonce] = useState(0)

  const getStoredPositions = useCallback(() => {
    const cached = storedPositionsRef.current
    if (cached?.key === storageKey) return cached.positions
    const positions = readStoredPositions(storageKey)
    storedPositionsRef.current = { key: storageKey, positions }
    return positions
  }, [storageKey])

  const commitStoredPositions = useCallback(
    (positions: StoredPositions) => {
      storedPositionsRef.current = { key: storageKey, positions }
      writeStoredPositions(storageKey, positions)
      setStoredPositionsNonce((nonce) => nonce + 1)
    },
    [storageKey],
  )

  // Track which nodes are inline-expanded. Stored separately so changes here
  // re-run the layout and trigger neighbouring cards to shift.
  const [expandedIds, setExpandedIds] = useState<ReadonlySet<string>>(() => new Set())
  const expandedIdsRef = useRef<ReadonlySet<string>>(new Set())

  // Actual rendered size for each node, populated from React Flow's dimension
  // change events. Drives layout so expansion displaces neighbours by exactly
  // the amount the card grew (rather than by EXPANDED_BODY_EXTRA, which had to
  // pad generously to cover worst-case content and was the source of the
  // "expand overshoots its actual height" bug).
  const [measuredSizes, setMeasuredSizes] = useState<ReadonlyMap<string, NodeSize>>(
    () => new Map(),
  )
  const measuredSizesRef = useRef<ReadonlyMap<string, NodeSize>>(new Map())
  const pendingMeasuredSizesRef = useRef<Map<string, NodeSize>>(new Map())
  const measurementFrameRef = useRef<number | null>(null)
  const measurementSettleTimersRef = useRef<Map<string, number>>(new Map())

  const commitMeasuredSizes = useCallback(
    (
      updater: (
        previous: ReadonlyMap<string, NodeSize>,
      ) => ReadonlyMap<string, NodeSize>,
    ) => {
      setMeasuredSizes((previous) => {
        const next = updater(previous)
        measuredSizesRef.current = next
        return next
      })
    },
    [],
  )

  const flushPendingMeasuredSizes = useCallback(
    (allowedIds?: ReadonlySet<string>) => {
      const pending = pendingMeasuredSizesRef.current
      if (pending.size === 0) return

      const updates = new Map<string, NodeSize>()
      const settling = measurementSettleTimersRef.current
      for (const [id, size] of pending) {
        if (allowedIds) {
          if (!allowedIds.has(id)) continue
        } else if (settling.has(id)) {
          continue
        }
        updates.set(id, size)
      }
      if (updates.size === 0) return

      for (const id of updates.keys()) {
        pending.delete(id)
      }

      startTransition(() => {
        commitMeasuredSizes((previous) => {
          let next: Map<string, NodeSize> | null = null
          for (const [id, nextSize] of updates) {
            if (sameMeasuredSize(previous.get(id), nextSize)) continue
            if (!next) next = new Map(previous)
            next.set(id, nextSize)
          }
          return next ?? previous
        })
      })
    },
    [commitMeasuredSizes],
  )

  const markMeasuredSizeSettling = useCallback(
    (nodeId: string) => {
      const timers = measurementSettleTimersRef.current
      const existing = timers.get(nodeId)
      if (existing !== undefined) {
        window.clearTimeout(existing)
      }
      const timer = window.setTimeout(() => {
        timers.delete(nodeId)
        flushPendingMeasuredSizes(new Set([nodeId]))
      }, EXPANSION_MEASUREMENT_SETTLE_MS)
      timers.set(nodeId, timer)
    },
    [flushPendingMeasuredSizes],
  )

  const invalidateMeasuredSize = useCallback(
    (nodeId: string) => {
      pendingMeasuredSizesRef.current.delete(nodeId)
      commitMeasuredSizes((previous) => {
        if (!previous.has(nodeId)) return previous
        const next = new Map(previous)
        next.delete(nodeId)
        return next
      })
    },
    [commitMeasuredSizes],
  )

  const commitEstimatedMeasuredSize = useCallback(
    (nodeId: string, expanded: boolean): boolean => {
      const estimated = estimateRenderedNodeSize(canvasRef.current, nodeId, expanded)
      if (!estimated) return false

      pendingMeasuredSizesRef.current.delete(nodeId)
      commitMeasuredSizes((previous) => {
        if (sameMeasuredSize(previous.get(nodeId), estimated)) return previous
        const next = new Map(previous)
        next.set(nodeId, estimated)
        return next
      })
      return true
    },
    [commitMeasuredSizes],
  )

  const scheduleMeasuredSize = useCallback(
    (nodeId: string, size: NodeSize) => {
      const normalized = normalizeMeasuredSize(size)
      const pending = pendingMeasuredSizesRef.current
      const existing = pending.get(nodeId) ?? measuredSizesRef.current.get(nodeId)
      if (sameMeasuredSize(existing, normalized)) return

      pending.set(nodeId, normalized)
      if (measurementSettleTimersRef.current.has(nodeId)) return
      if (measurementFrameRef.current !== null) return

      measurementFrameRef.current = window.requestAnimationFrame(() => {
        measurementFrameRef.current = null
        flushPendingMeasuredSizes()
      })
    },
    [flushPendingMeasuredSizes],
  )

  const setNodeExpanded = useCallback(
    (nodeId: string, expanded: boolean) => {
      const current = expandedIdsRef.current
      const has = current.has(nodeId)
      if (expanded === has) return

      const next = new Set(current)
      if (expanded) next.add(nodeId)
      else next.delete(nodeId)

      expandedIdsRef.current = next
      markMeasuredSizeSettling(nodeId)
      if (!commitEstimatedMeasuredSize(nodeId, expanded)) {
        invalidateMeasuredSize(nodeId)
      }
      setExpandedIds(next)
    },
    [commitEstimatedMeasuredSize, invalidateMeasuredSize, markMeasuredSizeSettling],
  )

  const expansionValue = useMemo<AgentCanvasExpansionContextValue>(
    () => ({
      setExpanded: setNodeExpanded,
    }),
    [setNodeExpanded],
  )

  const layoutResult = useMemo(() => {
    // resetNonce participates so "Reset layout" forces a recompute after the
    // localStorage entry is wiped.
    void resetNonce
    const sizes = buildSizeMap(baseGraph.nodes, expandedIds, measuredSizes)
    const placed = layoutAgentGraphByCategory(baseGraph.nodes, sizes, {
      stableHeaderHeight: NODE_SIZE_BY_TYPE['agent-header'].height,
    })
    return { placed, sizes }
  }, [baseGraph, expandedIds, measuredSizes, resetNonce])

  const computedNodes = useMemo(() => {
    const { placed, sizes } = layoutResult
    const stored = getStoredPositions()
    const positioned = applyStoredPositions(placed, stored)
    const classed = applyExpandedNodeClass(positioned, expandedIds)
    return applyKnownNodeDimensions(classed, sizes) as Node[]
  }, [expandedIds, getStoredPositions, layoutResult, storedPositionsNonce])

  const graphEdges = useMemo(
    () =>
      (baseGraph.edges as Edge[]).map((edge) =>
        edge.interactionWidth === 0 ? edge : { ...edge, interactionWidth: 0 },
      ),
    [baseGraph],
  )
  const computedEdges = graphEdges
  const canvasInteractingRef = useRef(false)

  const [nodes, setNodes] = useNodesState<Node>(computedNodes)
  const nodesRef = useRef<Node[]>(computedNodes)
  const nodeInternalsRefreshKeyRef = useRef('')
  const nodeInternalsSecondPassFrameRef = useRef<number | null>(null)
  const refreshNodeInternals = useCallback(
    (ids: readonly string[]) => {
      if (ids.length === 0) return
      const nextIds = [...ids]
      updateNodeInternals(nextIds)
      if (nodeInternalsSecondPassFrameRef.current !== null) {
        window.cancelAnimationFrame(nodeInternalsSecondPassFrameRef.current)
      }
      nodeInternalsSecondPassFrameRef.current = window.requestAnimationFrame(() => {
        nodeInternalsSecondPassFrameRef.current = null
        updateNodeInternals(nextIds)
      })
    },
    [updateNodeInternals],
  )
  const focusIndex = useMemo(
    () => buildFocusIndex(computedNodes, graphEdges),
    [computedNodes, graphEdges],
  )
  const focusTargetCacheRef = useRef<{
    index: FocusIndex | null
    targets: Map<string, FocusTarget>
  }>({ index: null, targets: new Map() })
  const getFocusTarget = useCallback(
    (nodeId: string): FocusTarget | null => {
      const cache = focusTargetCacheRef.current
      if (cache.index !== focusIndex) {
        cache.index = focusIndex
        cache.targets = new Map()
      }
      const cached = cache.targets.get(nodeId)
      if (cached) return cached
      const target = buildFocusTarget(nodeId, focusIndex)
      if (target) cache.targets.set(nodeId, target)
      return target
    },
    [focusIndex],
  )
  const persistableNodeIds = useMemo(
    () =>
      new Set(
        computedNodes
          .filter((node) => isPersistableNode(node))
          .map((node) => node.id),
      ),
    [computedNodes],
  )

  // Keep React Flow state in sync with computed layout. Position-only updates
  // benefit from CSS transition on .react-flow__node so neighbours animate.
  useEffect(() => {
    nodesRef.current = nodes
  }, [nodes])

  useEffect(() => {
    const refreshKey = nodeInternalsRefreshKey(nodes)
    const refreshIds = refreshableNodeIds(nodes)
    if (refreshKey === nodeInternalsRefreshKeyRef.current) return
    nodeInternalsRefreshKeyRef.current = refreshKey
    refreshNodeInternals(refreshIds)
  }, [nodes, refreshNodeInternals])

  useEffect(
    () => () => {
      if (nodeInternalsSecondPassFrameRef.current !== null) {
        window.cancelAnimationFrame(nodeInternalsSecondPassFrameRef.current)
      }
    },
    [],
  )

  useEffect(() => {
    setNodes((current) => {
      // Preserve user drag positions where present, otherwise adopt the new
      // computed position so layout reflow visibly takes effect.
      const byId = new Map(current.map((n) => [n.id, n] as const))
      let changed = current.length !== computedNodes.length
      const nextNodes = computedNodes.map((next) => {
        const prev = byId.get(next.id)
        if (!prev) {
          changed = true
          return next
        }
        // If the node has been measured with the same data and only position changed,
        // we keep the rest of its React Flow state (selection, etc.) and update position.
        if (
          prev.position.x === next.position.x &&
          prev.position.y === next.position.y &&
          prev.type === next.type &&
          prev.parentId === next.parentId &&
          prev.width === next.width &&
          prev.height === next.height &&
          prev.draggable === next.draggable &&
          prev.selectable === next.selectable &&
          prev.data === next.data
        ) {
          return prev
        }
        changed = true
        return { ...prev, ...next, position: next.position }
      })
      const resolved = changed ? nextNodes : current
      nodesRef.current = resolved
      return resolved
    })
  }, [computedNodes, setNodes])

  // Hover focus is intentionally DOM-only. The previous React-state version
  // rebuilt every node and edge object on hover, forcing React Flow to
  // reconcile the whole graph and recalculate SVG paths during pointer moves.
  // Here React owns graph structure; hover only toggles classes on the
  // already-mounted elements touched by the focused cluster.
  const pendingFrameRef = useRef<number | null>(null)
  const lastPointerRef = useRef<{ x: number; y: number; nodeId: string | null } | null>(
    null,
  )
  const hoveredNodeIdRef = useRef<string | null>(null)
  const lastPointerElementRef = useRef<Element | null>(null)
  const lastPointerDirectNodeIdRef = useRef<string | null>(null)
  const nodeIdByElementRef = useRef<WeakMap<Element, string | null>>(new WeakMap())
  const appliedFocusRef = useRef<AppliedFocus | null>(null)
  const interactionSettleTimerRef = useRef<number | null>(null)

  const resolveNodeIdFromElement = useCallback((element: Element | null): string | null => {
    if (!element) return null
    const cache = nodeIdByElementRef.current
    if (cache.has(element)) return cache.get(element) ?? null

    let cursor: Element | null = element
    let frameId: string | null = null
    while (cursor) {
      if (cursor.classList.contains('react-flow__node')) {
        if (cursor.classList.contains('react-flow__node-lane-label')) {
          cache.set(element, null)
          return null
        }
        const id = cursor.getAttribute('data-id')
        if (id) {
          if (!cursor.classList.contains('react-flow__node-tool-group-frame')) {
            cache.set(element, id)
            return id
          }
          frameId ??= id
        }
      }
      cursor = cursor.parentElement
    }
    cache.set(element, frameId)
    return frameId
  }, [])

  const resolveNodeIdAtPoint = useCallback((x: number, y: number): string | null => {
    const pointElements =
      typeof document.elementsFromPoint === 'function'
        ? document.elementsFromPoint(x, y)
        : [document.elementFromPoint(x, y)].filter((el): el is Element => Boolean(el))
    let frameId: string | null = null

    for (const element of pointElements) {
      let cursor: Element | null = element
      while (cursor) {
        if (cursor.classList.contains('react-flow__node')) {
          if (cursor.classList.contains('react-flow__node-lane-label')) break
          const id = cursor.getAttribute('data-id')
          if (id) {
            if (!cursor.classList.contains('react-flow__node-tool-group-frame')) {
              return id
            }
            frameId ??= id
          }
          break
        }
        cursor = cursor.parentElement
      }
    }

    return frameId
  }, [])

  const removeAppliedFocus = useCallback(() => {
    const root = canvasRef.current
    const applied = appliedFocusRef.current
    if (!root) return
    root.classList.remove('is-focusing')
    if (!applied) return
    for (const nodeElement of applied.nodeElements) {
      nodeElement.classList.remove('is-focused')
    }
    for (const edgeElement of applied.edgeElements) {
      edgeElement.classList.remove('is-active')
    }
    for (const edgeLabelElement of applied.edgeLabelElements) {
      edgeLabelElement.classList.remove('is-active')
    }
    appliedFocusRef.current = null
  }, [])

  const clearFocus = useCallback(() => {
    const hasFocusWork =
      pendingFrameRef.current !== null ||
      lastPointerRef.current !== null ||
      hoveredNodeIdRef.current !== null ||
      appliedFocusRef.current !== null
    if (!hasFocusWork) return

    if (pendingFrameRef.current !== null) {
      window.cancelAnimationFrame(pendingFrameRef.current)
      pendingFrameRef.current = null
    }
    lastPointerRef.current = null
    lastPointerElementRef.current = null
    lastPointerDirectNodeIdRef.current = null
    hoveredNodeIdRef.current = null
    removeAppliedFocus()
  }, [removeAppliedFocus])

  const applyFocusForNode = useCallback(
    (nodeId: string | null, options: { force?: boolean } = {}) => {
      if (!options.force && hoveredNodeIdRef.current === nodeId) return
      hoveredNodeIdRef.current = nodeId
      removeAppliedFocus()

      const root = canvasRef.current
      if (!root || !nodeId) return
      const target = getFocusTarget(nodeId)
      if (!target) return

      root.classList.add('is-focusing')
      const applied = getFocusedDomElements(root, target)
      for (const nodeElement of applied.nodeElements) {
        nodeElement.classList.add('is-focused')
      }
      for (const edgeElement of applied.edgeElements) {
        edgeElement.classList.add('is-active')
      }
      for (const edgeLabelElement of applied.edgeLabelElements) {
        edgeLabelElement.classList.add('is-active')
      }
      appliedFocusRef.current = applied
    },
    [getFocusTarget, removeAppliedFocus],
  )

  const markCanvasInteracting = useCallback(() => {
    const root = canvasRef.current
    if (!root) return
    if (interactionSettleTimerRef.current !== null) {
      window.clearTimeout(interactionSettleTimerRef.current)
      interactionSettleTimerRef.current = null
    }
    if (!canvasInteractingRef.current) {
      canvasInteractingRef.current = true
    }
  }, [])

  const settleCanvasInteraction = useCallback(() => {
    if (interactionSettleTimerRef.current !== null) {
      window.clearTimeout(interactionSettleTimerRef.current)
    }
    interactionSettleTimerRef.current = window.setTimeout(() => {
      interactionSettleTimerRef.current = null
      canvasInteractingRef.current = false
    }, INTERACTION_SETTLE_MS)
  }, [])

  const handleMoveStart = useCallback(
    (event: MouseEvent | TouchEvent | WheelEvent | null) => {
      markCanvasInteracting()
      clearFocus()
      if (typeof WheelEvent !== 'undefined' && event instanceof WheelEvent) {
        return
      }
    },
    [clearFocus, markCanvasInteracting],
  )

  const handleMoveEnd = useCallback(() => {
    settleCanvasInteraction()
  }, [settleCanvasInteraction])

  const handleNodeDragStart = useCallback<OnNodeDrag<Node>>(() => {
    clearFocus()
    markCanvasInteracting()
    canvasRef.current?.classList.add('is-dragging')
  }, [clearFocus, markCanvasInteracting])

  const handleNodeDragStop = useCallback<OnNodeDrag<Node>>(
    (_event, node) => {
      canvasRef.current?.classList.remove('is-dragging')

      if (isLaneLabelNodeId(node.id)) {
        const currentNodes = nodesRef.current
        const memberIds = getLaneDragMemberIds(currentNodes, node.id)
        if (memberIds.size > 0) {
          const nextPositions = { ...getStoredPositions() }
          for (const currentNode of currentNodes) {
            if (!memberIds.has(currentNode.id) || !isPersistableNode(currentNode)) {
              continue
            }
            nextPositions[currentNode.id] = {
              x: roundCanvasCoord(currentNode.position.x),
              y: roundCanvasCoord(currentNode.position.y),
            }
          }
          commitStoredPositions(nextPositions)
        }
      }

      settleCanvasInteraction()
    },
    [commitStoredPositions, getStoredPositions, settleCanvasInteraction],
  )

  const handleWheelCapture = useCallback(() => {
    markCanvasInteracting()
    clearFocus()
    settleCanvasInteraction()
  }, [clearFocus, markCanvasInteracting, settleCanvasInteraction])

  const handlePointerMove = useCallback(
    (event: PointerEvent) => {
      if (event.buttons !== 0 || canvasInteractingRef.current) {
        clearFocus()
        return
      }
      const target = event.target instanceof Element ? event.target : null
      if (
        target &&
        target === lastPointerElementRef.current &&
        lastPointerDirectNodeIdRef.current !== null &&
        pendingFrameRef.current === null
      ) {
        return
      }
      lastPointerElementRef.current = target
      const targetNodeId = resolveNodeIdFromElement(target)
      lastPointerDirectNodeIdRef.current = targetNodeId
      if (targetNodeId && targetNodeId === hoveredNodeIdRef.current) return
      lastPointerRef.current = {
        x: event.clientX,
        y: event.clientY,
        nodeId: targetNodeId,
      }
      if (pendingFrameRef.current !== null) return
      pendingFrameRef.current = window.requestAnimationFrame(() => {
        pendingFrameRef.current = null
        const point = lastPointerRef.current
        if (!point) return
        applyFocusForNode(point.nodeId ?? resolveNodeIdAtPoint(point.x, point.y))
      })
    },
    [
      applyFocusForNode,
      clearFocus,
      resolveNodeIdAtPoint,
      resolveNodeIdFromElement,
    ],
  )

  useEffect(() => {
    const root = canvasRef.current
    if (!root) return

    const passiveOptions: AddEventListenerOptions = { passive: true }
    const passiveCaptureOptions: AddEventListenerOptions = { capture: true, passive: true }
    root.addEventListener('pointermove', handlePointerMove, passiveOptions)
    root.addEventListener('pointerleave', clearFocus, passiveOptions)
    root.addEventListener('wheel', handleWheelCapture, passiveCaptureOptions)

    return () => {
      root.removeEventListener('pointermove', handlePointerMove, passiveOptions)
      root.removeEventListener('pointerleave', clearFocus, passiveOptions)
      root.removeEventListener('wheel', handleWheelCapture, passiveCaptureOptions)
    }
  }, [clearFocus, handlePointerMove, handleWheelCapture])

  useEffect(
    () => () => {
      if (pendingFrameRef.current !== null) {
        window.cancelAnimationFrame(pendingFrameRef.current)
        pendingFrameRef.current = null
      }
      if (measurementFrameRef.current !== null) {
        window.cancelAnimationFrame(measurementFrameRef.current)
        measurementFrameRef.current = null
      }
      if (nodeInternalsSecondPassFrameRef.current !== null) {
        window.cancelAnimationFrame(nodeInternalsSecondPassFrameRef.current)
        nodeInternalsSecondPassFrameRef.current = null
      }
      for (const timer of measurementSettleTimersRef.current.values()) {
        window.clearTimeout(timer)
      }
      measurementSettleTimersRef.current.clear()
      if (interactionSettleTimerRef.current !== null) {
        window.clearTimeout(interactionSettleTimerRef.current)
        interactionSettleTimerRef.current = null
      }
      if (emptyStateEntryFrameRef.current !== null) {
        window.cancelAnimationFrame(emptyStateEntryFrameRef.current)
        emptyStateEntryFrameRef.current = null
      }
      removeAppliedFocus()
    },
    [removeAppliedFocus],
  )

  useEffect(() => {
    nodeIdByElementRef.current = new WeakMap()
    clearFocus()
  }, [clearFocus, computedEdges, computedNodes])

  // Persist positions when the user finishes a drag. Avoids hammering
  // localStorage on every intermediate position event during the gesture.
  // ALSO captures dimension measurements from React Flow so the layout can
  // displace neighbours based on actual rendered heights instead of fixed
  // worst-case estimates.
  const handleNodesChange = useCallback(
    (changes: NodeChange<Node>[]) => {
      const flowChanges: NodeChange<Node>[] = []
      const laneDragChanges: NodeChange<Node>[] = []
      let nextPositions: StoredPositions | null = null

      for (const change of changes) {
        if (change.type === 'dimensions') {
          if (change.dimensions) scheduleMeasuredSize(change.id, change.dimensions)
          continue
        }

        if (change.type === 'position') {
          if (isLaneLabelNodeId(change.id)) {
            laneDragChanges.push(change)
            continue
          }

          if (
            change.dragging === false &&
            change.position &&
            persistableNodeIds.has(change.id)
          ) {
            if (!nextPositions) nextPositions = { ...getStoredPositions() }
            nextPositions[change.id] = {
              x: roundCanvasCoord(change.position.x),
              y: roundCanvasCoord(change.position.y),
            }
          }
        }

        flowChanges.push(change)
      }

      if (flowChanges.length > 0 || laneDragChanges.length > 0) {
        setNodes((current) => {
          const withFlowChanges =
            flowChanges.length > 0 ? applyNodeChanges(flowChanges, current) : current
          const next =
            laneDragChanges.length > 0
              ? applyLaneDragPositionChanges(withFlowChanges, laneDragChanges)
              : withFlowChanges
          nodesRef.current = next
          return next
        })
      }

      // Persist only the nodes the user *actually* dragged in this gesture.
      // The previous version snapshotted every node's position on every
      // drag-end, which made `applyStoredPositions` override the layout for
      // every node — so once you nudged anything, expansion no longer
      // re-flowed any neighbours.
      if (nextPositions) commitStoredPositions(nextPositions)
    },
    [
      commitStoredPositions,
      getStoredPositions,
      persistableNodeIds,
      scheduleMeasuredSize,
      setNodes,
    ],
  )

  useEffect(() => {
    if (exitTimerRef.current !== null) {
      window.clearTimeout(exitTimerRef.current)
      exitTimerRef.current = null
    }

    if (detail) {
      setRenderedDetail(detail)
      return
    }

    if (!renderedDetail) return

    const timer = window.setTimeout(() => {
      if (exitTimerRef.current === timer) {
        exitTimerRef.current = null
      }
      setRenderedDetail(null)
    }, AGENT_EXIT_TRANSITION_MS)
    exitTimerRef.current = timer
    return () => {
      if (exitTimerRef.current === timer) {
        window.clearTimeout(timer)
        exitTimerRef.current = null
      }
    }
  }, [detail, renderedDetail])

  useEffect(() => {
    if (emptyStateEntryFrameRef.current !== null) {
      window.cancelAnimationFrame(emptyStateEntryFrameRef.current)
      emptyStateEntryFrameRef.current = null
    }

    if (!emptyStateVisible) {
      setEmptyStateEntered(false)
      return
    }

    setEmptyStateEntered(false)
    setEmptyStateEntryKey((key) => key + 1)
    emptyStateEntryFrameRef.current = window.requestAnimationFrame(() => {
      emptyStateEntryFrameRef.current = window.requestAnimationFrame(() => {
        emptyStateEntryFrameRef.current = null
        setEmptyStateEntered(true)
      })
    })

    return () => {
      if (emptyStateEntryFrameRef.current !== null) {
        window.cancelAnimationFrame(emptyStateEntryFrameRef.current)
        emptyStateEntryFrameRef.current = null
      }
    }
  }, [emptyStateVisible])

  // Reset expansion state when the rendered agent changes — a different agent has a
  // different node id space, and stale entries would leak into its sizing.
  const lastDetailRef = useRef(renderedDetail)
  useEffect(() => {
    if (lastDetailRef.current !== renderedDetail) {
      lastDetailRef.current = renderedDetail
      const emptyExpanded = new Set<string>()
      const emptySizes = new Map<string, NodeSize>()
      if (measurementFrameRef.current !== null) {
        window.cancelAnimationFrame(measurementFrameRef.current)
        measurementFrameRef.current = null
      }
      for (const timer of measurementSettleTimersRef.current.values()) {
        window.clearTimeout(timer)
      }
      measurementSettleTimersRef.current.clear()
      expandedIdsRef.current = emptyExpanded
      measuredSizesRef.current = emptySizes
      pendingMeasuredSizesRef.current = new Map()
      nodeInternalsRefreshKeyRef.current = ''
      setExpandedIds(emptyExpanded)
      setMeasuredSizes(emptySizes)
      canvasInteractingRef.current = false
      canvasRef.current?.classList.remove('is-dragging')
    }
  }, [renderedDetail])

  const lastFitStorageKeyRef = useRef<string | null>(hasDetail ? storageKey : null)
  useEffect(() => {
    if (!FIT_VIEW_ON_INIT) return

    if (!hasIncomingDetail) {
      lastFitStorageKeyRef.current = null
      void reactFlow.setViewport(EMPTY_CANVAS_DEFAULT_VIEWPORT, {
        duration: FIT_VIEW_TRANSITION_MS,
      })
      return
    }

    if (
      computedNodes.length === 0 ||
      storageKey.length === 0 ||
      lastFitStorageKeyRef.current === storageKey
    ) {
      return
    }

    lastFitStorageKeyRef.current = storageKey
    const frame = window.requestAnimationFrame(() => {
      void reactFlow.fitView({
        ...FIT_VIEW_OPTIONS,
        duration: FIT_VIEW_TRANSITION_MS,
      })
    })
    return () => {
      window.cancelAnimationFrame(frame)
    }
  }, [computedNodes.length, hasDetail, hasIncomingDetail, reactFlow, storageKey])

  const handleResetLayout = useCallback(() => {
    if (typeof window !== 'undefined') {
      try {
        window.localStorage.removeItem(storageKey)
      } catch {
        // Ignore — storage may be disabled.
      }
    }
    storedPositionsRef.current = { key: storageKey, positions: {} }
    setStoredPositionsNonce((nonce) => nonce + 1)
    canvasInteractingRef.current = false
    canvasRef.current?.classList.remove('is-dragging')
    setResetNonce((n) => n + 1)
  }, [storageKey])

  return (
    <AgentCanvasExpansionContext.Provider value={expansionValue}>
      <div
        ref={canvasRef}
        className={
          isAgentExiting
            ? 'agent-visualization is-agent-exiting h-full w-full'
            : 'agent-visualization h-full w-full'
        }
      >
        <ReactFlow
          nodes={nodes}
          edges={computedEdges}
          onNodesChange={handleNodesChange}
          nodeTypes={NODE_TYPES}
          edgeTypes={EDGE_TYPES}
          nodesDraggable
          nodesConnectable={false}
          nodesFocusable={false}
          edgesFocusable={false}
          edgesReconnectable={false}
          elementsSelectable={false}
          elevateNodesOnSelect={false}
          elevateEdgesOnSelect={false}
          onlyRenderVisibleElements={ONLY_RENDER_VISIBLE_ELEMENTS}
          onMoveStart={handleMoveStart}
          onMoveEnd={handleMoveEnd}
          onNodeDragStart={handleNodeDragStart}
          onNodeDragStop={handleNodeDragStop}
          defaultViewport={EMPTY_CANVAS_DEFAULT_VIEWPORT}
          fitView={hasDetail && FIT_VIEW_ON_INIT}
          fitViewOptions={FIT_VIEW_OPTIONS}
          minZoom={0.2}
          maxZoom={2}
          snapToGrid={snapToGrid}
          snapGrid={SNAP_GRID}
          proOptions={REACT_FLOW_PRO_OPTIONS}
          defaultEdgeOptions={DEFAULT_EDGE_OPTIONS}
        >
          <WorkflowCanvasDots />
          {emptyState ? (
            <div
              key={emptyStateEntryKey}
              aria-hidden={!emptyStateVisible}
              className={
                emptyStateIsVisible
                  ? 'agent-visualization__empty-state is-visible'
                  : 'agent-visualization__empty-state is-hidden'
              }
            >
              {emptyState}
            </div>
          ) : null}
          {hasDetail ? (
            <Controls
              position="bottom-right"
              showInteractive={false}
              className="!bg-card !border !border-border !rounded-md !shadow-sm"
            >
              <ControlButton
                onClick={handleToggleSnapToGrid}
                aria-label={snapToGrid ? 'Disable snap to grid' : 'Enable snap to grid'}
                aria-pressed={snapToGrid}
                style={snapToGrid ? { color: 'var(--primary)' } : undefined}
              >
                <Magnet />
              </ControlButton>
              <ControlButton
                onClick={handleResetLayout}
                aria-label="Reset layout"
              >
                <LayoutGrid />
              </ControlButton>
            </Controls>
          ) : null}
        </ReactFlow>
      </div>
    </AgentCanvasExpansionContext.Provider>
  )
}

export function AgentVisualization(props: AgentVisualizationProps) {
  return (
    <ReactFlowProvider>
      <AgentVisualizationInner {...props} />
    </ReactFlowProvider>
  )
}

// Re-exports so tests/consumers can compose without re-importing the graph internals.
export type { AgentGraphEdge, AgentGraphNode }
