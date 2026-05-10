'use client'

import {
  startTransition,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import {
  addEdge,
  applyEdgeChanges,
  applyNodeChanges,
  Background,
  ControlButton,
  Controls,
  Position,
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useOnViewportChange,
  useReactFlow,
  useUpdateNodeInternals,
  type Connection,
  type Edge,
  type EdgeChange,
  type EdgeTypes,
  type Node,
  type NodeChange,
  type OnNodeDrag,
  type NodeTypes,
  type Viewport,
  type XYPosition,
} from '@xyflow/react'
import {
  Lock,
  Magnet,
  Maximize,
  RotateCcw,
  Unlock,
  ZoomIn,
  ZoomOut,
} from 'lucide-react'

import '@xyflow/react/dist/style.css'

import type {
  AgentDefinitionValidationDiagnosticDto,
  AgentDefinitionWriteResponseDto,
} from '@/src/lib/xero-model/agent-definition'
import {
  agentRefKey,
  type AgentAuthoringCatalogDto,
  type AgentAuthoringAttachableSkillDto,
  type AgentAuthoringSkillSearchResultDto,
  type SearchAgentAuthoringSkillsResponseDto,
  type AgentTriggerRefDto,
  type WorkflowAgentDetailDto,
} from '@/src/lib/xero-model/workflow-agents'
import { XeroDesktopAdapter } from '@/src/lib/xero-desktop'

import {
  AGENT_GRAPH_HEADER_HANDLES,
  AGENT_GRAPH_HEADER_LEFT_HANDLE_RATIOS,
  AGENT_GRAPH_HEADER_NODE_ID,
  AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS,
  AGENT_GRAPH_OUTPUT_NODE_ID,
  AGENT_GRAPH_TRIGGER_HANDLES,
  agentGraphFromProjection,
  buildAgentGraph,
  buildAgentGraphForEditing,
  decodeAgentGraphNodeId,
  deriveAdvancedFromDetail,
  emptyInferredAdvanced,
  getHandleDragRole,
  humanizeIdentifier,
  inferAdvancedFromConnections,
  lifecycleEventLabel,
  promptNodeId,
  skillNodeId,
  toolNodeId,
  type AgentGraph,
  type AgentGraphEdge,
  type AgentGraphNode,
  type AgentGraphNodeKind,
  type AgentHeaderAdvancedFields,
} from './build-agent-graph'
import { buildSnapshotFromGraph } from './build-snapshot'
import {
  CanvasModeProvider,
  type CanvasMode,
  type CanvasModeContextValue,
} from './canvas-mode-context'
import type { CanvasPaletteKind } from './canvas-palette'
import { DropPicker } from './drop-picker'
import {
  applyEdgeValidationClasses,
  isEdgeAllowed,
  validateEdges,
  validateStructure,
} from './edge-validation'
import {
  AgentCanvasExpansionContext,
  type AgentCanvasExpansionContextValue,
} from './expansion-context'
import { TriggerEdge } from './edges/trigger-edge'
import { layoutAgentGraphByCategory, type NodeSize } from './layout'
import { NodeDetailsPanel } from './node-details-panel'
import { NodePropertiesPanel } from './node-properties-panel'
import { AgentHeaderNode } from './nodes/agent-header-node'
import { ConsumedArtifactNode } from './nodes/consumed-artifact-node'
import { DbTableNode } from './nodes/db-table-node'
import { LaneLabelNode } from './nodes/lane-label-node'
import { OutputNode } from './nodes/output-node'
import { OutputSectionNode } from './nodes/output-section-node'
import { PromptNode } from './nodes/prompt-node'
import { SkillNode } from './nodes/skill-node'
import { ToolGroupFrameNode } from './nodes/tool-group-frame-node'
import { ToolNode } from './nodes/tool-node'

import './agent-visualization.css'

const NODE_TYPES = {
  'agent-header': AgentHeaderNode,
  prompt: PromptNode,
  skills: SkillNode,
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
const CONTROL_ZOOM_TRANSITION_MS = 180
const CANVAS_CONTROL_ICON_CLASS = 'h-[18px] w-[18px]'
const PROPERTIES_PANEL_FOCUS_FOOTPRINT_PX = 320
// Initial authoring can mount while the agent dock is opening; allow a couple
// of measured-size corrections without turning every later resize into a refit.
const CREATE_MODE_INITIAL_FIT_MAX_PASSES = 3
const CREATE_MODE_INITIAL_FIT_TRANSITION_MS = 0
const CREATE_MODE_REFIT_TRANSITION_MS = 180
const CREATE_MODE_INITIAL_FIT_REVEAL_DELAY_MS = 80
const CREATE_MODE_FIT_MAX_ZOOM = 1
const CREATE_MODE_FIT_MIN_ZOOM = 0.38
const CREATE_MODE_FIT_MIN_PADDING = 72
const CREATE_MODE_FIT_MAX_PADDING = 144
const CREATE_MODE_FIT_PADDING_RATIO = 0.14
const AGENT_EXIT_TRANSITION_MS = 220
const EMPTY_CANVAS_DEFAULT_VIEWPORT = { x: 0, y: 0, zoom: 0.72 } as const
const DEFAULT_EDGE_OPTIONS = {
  type: 'smoothstep',
  animated: false,
  interactionWidth: 0,
} as const

const NODE_SIZE_BY_TYPE: Record<string, NodeSize> = {
  'agent-header': { width: 320, height: 210 },
  prompt: { width: 300, height: 48 },
  skills: { width: 260, height: 112 },
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
  prompt: 200,
  tool: 90,
  'output-section': 80,
  'consumed-artifact': 130,
}

const POSITIONS_STORAGE_PREFIX = 'xero.workflows.canvas-positions:'
const SNAP_TO_GRID_STORAGE_KEY = 'xero.workflows.canvas-snap-to-grid'
const SNAP_TO_GRID_APP_STATE_KEY = 'workflow.canvas.snapToGrid.v1'
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
const EDITING_DOM_MEASURED_HANDLE_NODE_TYPES = new Set<string>(['db-table'])

type StoredPositions = Record<string, { x: number; y: number }>

type LaneDragCategory =
  | 'prompt'
  | 'skills'
  | 'tool'
  | 'db-table'
  | 'agent-output'
  | 'output-section'
  | 'consumed-artifact'

const LANE_NODE_PREFIX = 'lane:'

/**
 * Editing-state surface published by the canvas to its embedding chrome
 * (phase-view). Lets the existing top-right button cluster render Save /
 * Cancel buttons alongside its other chrome instead of having the canvas
 * paint a separate toolbar over its own surface.
 */
export interface AgentVisualizationEditingStatus {
  saving: boolean
  saveDisabled: boolean
  hasInvalidEdges: boolean
  errorMessage: string | null
  diagnosticCount: number
  diagnostics: ReadonlyArray<AgentDefinitionValidationDiagnosticDto>
  save: () => void
}

interface AgentVisualizationProps {
  active?: boolean
  projectId?: string | null
  detail?: WorkflowAgentDetailDto | null
  emptyState?: ReactNode
  emptyStateVisible?: boolean
  // Editing extensions. When `editing` is true, `mode`, `onSubmit`, `onSaved`,
  // and `onCancel` must also be provided. The canvas switches to a mutable
  // graph seeded from `initialDetail` (or a blank graph for `mode === 'create'`)
  // and renders the palette overlay. The Save/Cancel buttons are rendered by
  // the embedding chrome (phase-view) using `onEditingStatusChange` so they
  // sit in the same top-right cluster as the rest of the agent chrome.
  editing?: boolean
  mode?: CanvasMode
  initialDetail?: WorkflowAgentDetailDto | null
  // Catalog of pickable tools / DB tables / upstream artifacts used by the
  // editing palette. When null the canvas falls back to empty pickers so the
  // UI degrades gracefully if the catalog hasn't loaded yet.
  authoringCatalog?: AgentAuthoringCatalogDto | null
  onSearchAttachableSkills?: (params: {
    query: string
    offset: number
    limit: number
  }) => Promise<SearchAgentAuthoringSkillsResponseDto>
  onResolveAttachableSkill?: (
    skill: AgentAuthoringSkillSearchResultDto,
  ) => Promise<AgentAuthoringAttachableSkillDto>
  onSubmit?: (params: {
    snapshot: Record<string, unknown>
    mode: CanvasMode
    definitionId?: string
  }) => Promise<AgentDefinitionWriteResponseDto>
  onSaved?: (response: AgentDefinitionWriteResponseDto) => void
  onCancel?: () => void
  onEditingStatusChange?: (status: AgentVisualizationEditingStatus | null) => void
  onReadProjectUiState?: (key: string) => Promise<unknown | null>
  onWriteProjectUiState?: (key: string, value: unknown | null) => Promise<void>
  // Fired when the inline properties / details panel becomes visible or hidden
  // so the surrounding chrome can react (e.g. App.tsx auto-collapses any open
  // sidebar so the panel has the canvas to itself, then reopens it on close).
  onSelectedNodeChange?: (hasSelection: boolean) => void
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
    case 'tool':
    case 'output-section':
    case 'consumed-artifact':
      return expanded
    // Prompt cards live directly above the header. Let their collapsed
    // measurement drive layout so the visible prompt/header gap matches the
    // output/header gap instead of reserving stale body height above the card.
    case 'prompt':
      return true
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
    return parseStoredPositions(JSON.parse(raw))
  } catch {
    return {}
  }
}

function parseStoredPositions(value: unknown): StoredPositions {
  if (!value || typeof value !== 'object') return {}
  const out: StoredPositions = {}
  for (const [id, entry] of Object.entries(value as Record<string, unknown>)) {
    if (
      entry &&
      typeof entry === 'object' &&
      typeof (entry as { x?: unknown }).x === 'number' &&
      typeof (entry as { y?: unknown }).y === 'number'
    ) {
      out[id] = {
        x: (entry as { x: number }).x,
        y: (entry as { y: number }).y,
      }
    }
  }
  return out
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

type NodeViewportBounds = {
  x: number
  y: number
  width: number
  height: number
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

function nodePositionForViewportBounds(node: Node): XYPosition {
  const internals = node as Node & {
    internals?: { positionAbsolute?: XYPosition }
    positionAbsolute?: XYPosition
  }
  return internals.positionAbsolute ?? internals.internals?.positionAbsolute ?? node.position
}

function nodeAbsolutePositionFromList(
  node: Node,
  nodesById: ReadonlyMap<string, Node>,
  visited = new Set<string>(),
): XYPosition {
  const internals = node as Node & {
    internals?: { positionAbsolute?: XYPosition }
    positionAbsolute?: XYPosition
  }
  const absolute = internals.positionAbsolute ?? internals.internals?.positionAbsolute
  if (absolute) return absolute

  const parentId = node.parentId
  if (!parentId || visited.has(node.id)) return node.position

  const parent = nodesById.get(parentId)
  if (!parent) return node.position

  visited.add(node.id)
  const parentPosition = nodeAbsolutePositionFromList(parent, nodesById, visited)
  return {
    x: parentPosition.x + node.position.x,
    y: parentPosition.y + node.position.y,
  }
}

function nodeSizeForViewportBounds(node: Node): NodeSize {
  const fallback =
    node.type === 'lane-label'
      ? { width: 300, height: 26 }
      : NODE_SIZE_BY_TYPE[node.type ?? ''] ?? { width: 240, height: 80 }
  return normalizeMeasuredSize({
    width: node.measured?.width ?? node.width ?? node.initialWidth ?? fallback.width,
    height: node.measured?.height ?? node.height ?? node.initialHeight ?? fallback.height,
  })
}

function visibleNodeBounds(nodes: readonly Node[]): NodeViewportBounds | null {
  let minX = Number.POSITIVE_INFINITY
  let minY = Number.POSITIVE_INFINITY
  let maxX = Number.NEGATIVE_INFINITY
  let maxY = Number.NEGATIVE_INFINITY

  for (const node of nodes) {
    if (!node.type || node.hidden) continue
    const position = nodePositionForViewportBounds(node)
    const size = nodeSizeForViewportBounds(node)
    minX = Math.min(minX, position.x)
    minY = Math.min(minY, position.y)
    maxX = Math.max(maxX, position.x + size.width)
    maxY = Math.max(maxY, position.y + size.height)
  }

  if (
    !Number.isFinite(minX) ||
    !Number.isFinite(minY) ||
    !Number.isFinite(maxX) ||
    !Number.isFinite(maxY)
  ) {
    return null
  }

  return {
    x: minX,
    y: minY,
    width: Math.max(1, maxX - minX),
    height: Math.max(1, maxY - minY),
  }
}

export function selectedNodeFocusCenter(
  node: Node,
  nodes: readonly Node[],
  panelFootprintPx: number,
  focusZoom: number,
): XYPosition {
  const nodesById = new Map(nodes.map((entry) => [entry.id, entry]))
  const position = nodeAbsolutePositionFromList(node, nodesById)
  const size = nodeSizeForViewportBounds(node)
  const screenOffset = panelFootprintPx / 2 / focusZoom
  return {
    x: position.x + size.width / 2 - screenOffset,
    y: position.y + size.height / 2,
  }
}

function viewportBoundsKey(bounds: NodeViewportBounds | null): string {
  if (!bounds) return ''
  return [
    Math.round(bounds.x),
    Math.round(bounds.y),
    Math.round(bounds.width),
    Math.round(bounds.height),
  ].join(':')
}

function createModeInitialViewport(
  nodeBounds: NodeViewportBounds,
  canvasBounds: { width: number; height: number },
): Viewport {
  const paddingX = clamp(
    canvasBounds.width * CREATE_MODE_FIT_PADDING_RATIO,
    CREATE_MODE_FIT_MIN_PADDING,
    CREATE_MODE_FIT_MAX_PADDING,
  )
  const paddingY = clamp(
    canvasBounds.height * CREATE_MODE_FIT_PADDING_RATIO,
    CREATE_MODE_FIT_MIN_PADDING,
    CREATE_MODE_FIT_MAX_PADDING,
  )
  const availableWidth = Math.max(1, canvasBounds.width - paddingX * 2)
  const availableHeight = Math.max(1, canvasBounds.height - paddingY * 2)
  const zoom = clamp(
    Math.min(
      availableWidth / Math.max(1, nodeBounds.width),
      availableHeight / Math.max(1, nodeBounds.height),
    ),
    CREATE_MODE_FIT_MIN_ZOOM,
    CREATE_MODE_FIT_MAX_ZOOM,
  )

  return {
    x: (canvasBounds.width - nodeBounds.width * zoom) / 2 - nodeBounds.x * zoom,
    y: (canvasBounds.height - nodeBounds.height * zoom) / 2 - nodeBounds.y * zoom,
    zoom,
  }
}

function isPersistableNode(node: AgentGraphNode | Node): boolean {
  return Boolean(node.type)
}

function laneCategoryFromNodeId(nodeId: string): LaneDragCategory | null {
  if (!nodeId.startsWith(LANE_NODE_PREFIX)) return null
  const category = nodeId.slice(LANE_NODE_PREFIX.length)
  switch (category) {
    case 'prompt':
    case 'skills':
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
    case 'skills':
      return node.type === 'skills'
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

function handleTopFromRatio(nodeHeight: number, ratio: number): number {
  return nodeHeight * ratio - HANDLE_SIZE / 2
}

function knownHandlesForNode(node: AgentGraphNode, width: number, height: number) {
  switch (node.type) {
    case 'agent-header':
      return [
        nodeHandle('source', Position.Top, width, height, AGENT_GRAPH_HEADER_HANDLES.prompt),
        nodeHandle(
          'source',
          Position.Left,
          width,
          height,
          AGENT_GRAPH_HEADER_HANDLES.skills,
          { y: handleTopFromRatio(height, AGENT_GRAPH_HEADER_LEFT_HANDLE_RATIOS.skills) },
        ),
        nodeHandle(
          'source',
          Position.Right,
          width,
          height,
          AGENT_GRAPH_HEADER_HANDLES.tool,
          { y: handleTopFromRatio(height, AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS.tool) },
        ),
        nodeHandle(
          'source',
          Position.Right,
          width,
          height,
          AGENT_GRAPH_HEADER_HANDLES.db,
          { y: handleTopFromRatio(height, AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS.db) },
        ),
        nodeHandle('source', Position.Bottom, width, height, AGENT_GRAPH_HEADER_HANDLES.output),
        nodeHandle(
          'target',
          Position.Left,
          width,
          height,
          AGENT_GRAPH_HEADER_HANDLES.consumed,
          { y: handleTopFromRatio(height, AGENT_GRAPH_HEADER_LEFT_HANDLE_RATIOS.consumed) },
        ),
      ]
    case 'prompt':
      return [nodeHandle('target', Position.Bottom, width, height)]
    case 'skills':
      return [nodeHandle('target', Position.Bottom, width, height)]
    case 'tool': {
      const handles: KnownNodeHandle[] = []
      if (node.data.directConnectionHandles.target) {
        handles.push(
          nodeHandle('target', Position.Left, width, height, AGENT_GRAPH_TRIGGER_HANDLES.target),
        )
      }
      if (node.data.directConnectionHandles.source) {
        handles.push(
          nodeHandle('source', Position.Right, width, height, AGENT_GRAPH_TRIGGER_HANDLES.source),
        )
      }
      return handles
    }
    case 'tool-group-frame':
      return [nodeHandle('target', Position.Left, width, height)]
    case 'db-table':
      return [
        nodeHandle('target', Position.Left, width, height),
        nodeHandle('target', Position.Left, width, height, AGENT_GRAPH_TRIGGER_HANDLES.target, {
          y: handleTopFromRatio(height, 0.72),
        }),
      ]
    case 'agent-output':
      return [
        nodeHandle('target', Position.Top, width, height),
        nodeHandle('source', Position.Bottom, width, height),
      ]
    case 'output-section':
      return [
        nodeHandle('target', Position.Top, width, height),
        nodeHandle('target', Position.Left, width, height, AGENT_GRAPH_TRIGGER_HANDLES.target),
        nodeHandle('source', Position.Right, width, height, AGENT_GRAPH_TRIGGER_HANDLES.source),
      ]
    case 'consumed-artifact':
      return [
        nodeHandle('source', Position.Right, width, height),
        nodeHandle('source', Position.Right, width, height, AGENT_GRAPH_TRIGGER_HANDLES.source, {
          y: handleTopFromRatio(height, 0.72),
        }),
      ]
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
  options: {
    domMeasuredHandleNodeTypes?: ReadonlySet<string>
  } = {},
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
    const handles = options.domMeasuredHandleNodeTypes?.has(node.type ?? '')
      ? undefined
      : knownHandlesForNode(node, width, height)
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
  active = true,
  projectId = null,
  detail = null,
  emptyState,
  emptyStateVisible = detail === null,
  editing = false,
  mode,
  initialDetail = null,
  authoringCatalog = null,
  onSearchAttachableSkills,
  onResolveAttachableSkill,
  onSubmit,
  onSaved,
  onCancel,
  onEditingStatusChange,
  onReadProjectUiState,
  onWriteProjectUiState,
  onSelectedNodeChange,
}: AgentVisualizationProps) {
  // ============================================================
  // Editing-mode state. Always declared (rules of hooks) but only
  // wired into the canvas when `editing` is true. View mode uses the
  // detail-derived pipeline below; edit mode bypasses that pipeline
  // and drives nodes/edges directly from local state — but renders
  // through the same dots, controls, and JSX shell so both modes
  // are visually identical.
  // ============================================================
  const editingInitial = useMemo(() => {
    if (!editing) return null
    const result = buildAgentGraphForEditing(mode ?? 'create', initialDetail ?? null)
    return { detail: result.detail }
  }, [editing, mode, initialDetail])
  const editingSeedBaselineTargetRef = useRef<WorkflowAgentDetailDto | null>(
    editingInitial?.detail ?? null,
  )

  // Editing source of truth: a real WorkflowAgentDetailDto we mutate as the
  // user types/drags. Derived nodes/edges run through the same buildAgentGraph
  // + layoutAgentGraphByCategory pipeline as view mode, so adding a tool
  // lands it inside the proper TOOLS lane / category frame instead of at an
  // arbitrary drop point.
  const [editingDetail, setEditingDetail] = useState<WorkflowAgentDetailDto | null>(
    () => editingInitial?.detail ?? null,
  )
  const [editingAdvanced, setEditingAdvanced] = useState<AgentHeaderAdvancedFields>(
    () =>
      editingInitial
        ? deriveAdvancedFromDetail(editingInitial.detail)
        : ({
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
            externalServiceAllowed: false,
            browserControlAllowed: false,
            skillRuntimeAllowed: false,
            subagentAllowed: false,
            commandAllowed: false,
            destructiveWriteAllowed: false,
          } satisfies AgentHeaderAdvancedFields),
  )
  const [editingSaving, setEditingSaving] = useState(false)
  const [editingServerDiagnostics, setEditingServerDiagnostics] = useState<
    readonly AgentDefinitionValidationDiagnosticDto[]
  >([])
  const [editingErrorMessage, setEditingErrorMessage] = useState<string | null>(null)
  // User-driven position overrides for nodes the layout placed automatically.
  // We persist these in memory only — the agent definition itself doesn't
  // care about layout positions, and there's no stable storage key for an
  // unsaved create. Declared here so the layout pipeline (computedNodes)
  // can reference them, even though most write paths live further down in
  // the editing-handlers section.
  const [editingPositionOverrides, setEditingPositionOverrides] = useState<
    Record<string, { x: number; y: number }>
  >({})

  useEffect(() => {
    if (!editingInitial) return
    editingSeedBaselineTargetRef.current = editingInitial.detail
    setEditingDetail(editingInitial.detail)
    setEditingAdvanced(deriveAdvancedFromDetail(editingInitial.detail))
    setEditingPositionOverrides({})
    setEditingServerDiagnostics([])
    setEditingErrorMessage(null)
  }, [editingInitial])

  const [renderedDetailFromView, setRenderedDetailFromView] =
    useState<WorkflowAgentDetailDto | null>(() => detail)
  // In editing mode the renderedDetail is the live editingDetail; the rest
  // of the pipeline (baseGraph → layout → computedNodes) runs on it
  // unchanged, so editing inherits the same lane / category-frame layout
  // view mode produces.
  const renderedDetail = editing ? editingDetail : renderedDetailFromView
  const setRenderedDetail = setRenderedDetailFromView
  const exitTimerRef = useRef<number | null>(null)
  const emptyStateEntryFrameRef = useRef<number | null>(null)
  const [emptyStateEntryKey, setEmptyStateEntryKey] = useState(0)
  const [emptyStateEntered, setEmptyStateEntered] = useState(false)
  const hasIncomingDetail = detail !== null
  const hasDetail = renderedDetail !== null
  const isAgentExiting = !hasIncomingDetail && renderedDetail !== null && !editing
  const emptyStateIsVisible = emptyStateVisible && emptyStateEntered
  const reactFlow = useReactFlow<Node, Edge>()
  const storageKey = useMemo(
    () => (!editing && renderedDetail ? storageKeyFor(renderedDetail) : ''),
    [editing, renderedDetail],
  )
  const projectUiStateKey = useMemo(
    () => (!editing && renderedDetail ? `workflow.canvas.positions:${agentRefKey(renderedDetail.ref)}` : ''),
    [editing, renderedDetail],
  )
  const hasProjectUiStateStorage = Boolean(
    projectId && projectUiStateKey && onReadProjectUiState && onWriteProjectUiState,
  )

  const baseGraph = useMemo(() => {
    if (!renderedDetail) return EMPTY_AGENT_GRAPH
    const graph =
      !editing && renderedDetail.graphProjection
        ? agentGraphFromProjection(renderedDetail.graphProjection)
        : buildAgentGraph(renderedDetail)
    if (!editing) return graph
    // Editing mode merges the current advanced-panel state into the header
    // node's data so the agent-header card renders with the user's typed
    // workflow contract / examples / capability flags. Cast through unknown
    // because the node union is too narrow for a structural spread.
    return {
      nodes: graph.nodes.map((node) => {
        if (node.type !== 'agent-header') return node
        return {
          ...node,
          data: {
            ...node.data,
            advanced: editingAdvanced,
          },
        } as unknown as AgentGraphNode
      }),
      edges: graph.edges,
    }
  }, [editing, editingAdvanced, renderedDetail])
  const canvasRef = useRef<HTMLDivElement | null>(null)
  const [canvasBounds, setCanvasBounds] = useState<{ width: number; height: number } | null>(null)
  const canvasBoundsKey = canvasBounds ? `${canvasBounds.width}x${canvasBounds.height}` : ''
  const updateNodeInternals = useUpdateNodeInternals()
  // Bumped each time the user invokes "Reset layout" so the memoized layout
  // computation re-runs even when storage was already empty.
  const [resetNonce, setResetNonce] = useState(0)
  const [snapToGrid, setSnapToGrid] = useState<boolean>(() => readSnapToGridPreference())
  const [snapPreferenceHydrated, setSnapPreferenceHydrated] = useState(false)
  const [canvasLocked, setCanvasLocked] = useState(false)
  const canvasInteractionsLocked = editing && canvasLocked

  useEffect(() => {
    const readAppUiState = XeroDesktopAdapter.readAppUiState
    if (typeof readAppUiState !== 'function') {
      setSnapPreferenceHydrated(true)
      return
    }

    let cancelled = false
    void readAppUiState({ key: SNAP_TO_GRID_APP_STATE_KEY })
      .then((response) => {
        if (cancelled) return
        if (typeof response.value === 'boolean') {
          setSnapToGrid(response.value)
        }
      })
      .catch(() => undefined)
      .finally(() => {
        if (cancelled) return
        setSnapPreferenceHydrated(true)
      })

    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    writeSnapToGridPreference(snapToGrid)
    if (snapPreferenceHydrated) {
      void XeroDesktopAdapter.writeAppUiState?.({
        key: SNAP_TO_GRID_APP_STATE_KEY,
        value: snapToGrid,
      }).catch(() => undefined)
    }
  }, [snapPreferenceHydrated, snapToGrid])
  const handleToggleSnapToGrid = useCallback(() => {
    setSnapToGrid((prev) => !prev)
  }, [])
  const handleToggleCanvasLock = useCallback(() => {
    setCanvasLocked((prev) => !prev)
  }, [])
  const handleZoomIn = useCallback(() => {
    void reactFlow.zoomIn({ duration: CONTROL_ZOOM_TRANSITION_MS })
  }, [reactFlow])
  const handleZoomOut = useCallback(() => {
    void reactFlow.zoomOut({ duration: CONTROL_ZOOM_TRANSITION_MS })
  }, [reactFlow])
  const handleFitView = useCallback(() => {
    void reactFlow.fitView({
      ...FIT_VIEW_OPTIONS,
      duration: FIT_VIEW_TRANSITION_MS,
    })
  }, [reactFlow])
  useLayoutEffect(() => {
    const root = canvasRef.current
    if (!root) return

    let resizeFrame: number | null = null
    let pendingSize: { width: number; height: number } | null = null

    const commitSize = (width: number, height: number) => {
      const next = {
        width: Math.round(width),
        height: Math.round(height),
      }
      if (next.width <= 1 || next.height <= 1) return
      setCanvasBounds((current) =>
        current?.width === next.width && current.height === next.height ? current : next,
      )
    }

    const scheduleSize = (width: number, height: number) => {
      pendingSize = { width, height }
      if (resizeFrame !== null) return
      // Keep this resilient to tests that mock requestAnimationFrame
      // synchronously: mark as pending before scheduling, then only store the
      // returned frame id if the callback did not already run.
      resizeFrame = -1
      const nextFrame = window.requestAnimationFrame(() => {
        resizeFrame = null
        if (!pendingSize) return
        const { width: pendingWidth, height: pendingHeight } = pendingSize
        pendingSize = null
        commitSize(pendingWidth, pendingHeight)
      })
      if (resizeFrame !== null) resizeFrame = nextFrame
    }

    const initialBounds = root.getBoundingClientRect()
    commitSize(initialBounds.width, initialBounds.height)

    if (typeof ResizeObserver === 'undefined') {
      const handleWindowResize = () => {
        const bounds = root.getBoundingClientRect()
        scheduleSize(bounds.width, bounds.height)
      }
      window.addEventListener('resize', handleWindowResize)
      return () => {
        window.removeEventListener('resize', handleWindowResize)
        if (resizeFrame !== null) window.cancelAnimationFrame(resizeFrame)
      }
    }

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0]
      if (!entry) return
      scheduleSize(entry.contentRect.width, entry.contentRect.height)
    })
    observer.observe(root)

    return () => {
      observer.disconnect()
      if (resizeFrame !== null) window.cancelAnimationFrame(resizeFrame)
    }
  }, [active])
  const storedPositionsRef = useRef<{ key: string; positions: StoredPositions } | null>(
    null,
  )
  const [storedPositionsNonce, setStoredPositionsNonce] = useState(0)

  useEffect(() => {
    if (!hasProjectUiStateStorage || !projectUiStateKey || !onReadProjectUiState) {
      return
    }

    let cancelled = false
    onReadProjectUiState(projectUiStateKey)
      .then((value) => {
        if (cancelled) return
        storedPositionsRef.current = {
          key: projectUiStateKey,
          positions: parseStoredPositions(value),
        }
        setStoredPositionsNonce((nonce) => nonce + 1)
      })
      .catch(() => {
        if (cancelled) return
        storedPositionsRef.current = { key: projectUiStateKey, positions: {} }
        setStoredPositionsNonce((nonce) => nonce + 1)
      })

    return () => {
      cancelled = true
    }
  }, [hasProjectUiStateStorage, onReadProjectUiState, projectUiStateKey])

  const getStoredPositions = useCallback(() => {
    if (hasProjectUiStateStorage) {
      const cached = storedPositionsRef.current
      return cached?.key === projectUiStateKey ? cached.positions : {}
    }
    const cached = storedPositionsRef.current
    if (cached?.key === storageKey) return cached.positions
    const positions = readStoredPositions(storageKey)
    storedPositionsRef.current = { key: storageKey, positions }
    return positions
  }, [hasProjectUiStateStorage, projectUiStateKey, storageKey])

  const commitStoredPositions = useCallback(
    (positions: StoredPositions) => {
      if (hasProjectUiStateStorage) {
        storedPositionsRef.current = { key: projectUiStateKey, positions }
        void onWriteProjectUiState?.(projectUiStateKey, positions).catch(() => {})
      } else {
        storedPositionsRef.current = { key: storageKey, positions }
        writeStoredPositions(storageKey, positions)
      }
      setStoredPositionsNonce((nonce) => nonce + 1)
    },
    [hasProjectUiStateStorage, onWriteProjectUiState, projectUiStateKey, storageKey],
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
      locked: canvasInteractionsLocked,
      setExpanded: setNodeExpanded,
    }),
    [canvasInteractionsLocked, setNodeExpanded],
  )

  const layoutResult = useMemo(() => {
    // resetNonce participates so "Reset layout" forces a recompute after the
    // persisted position overrides are wiped.
    void resetNonce
    const sizes = buildSizeMap(baseGraph.nodes, expandedIds, measuredSizes)
    const placed = layoutAgentGraphByCategory(baseGraph.nodes, sizes, {
      stableHeaderHeight: NODE_SIZE_BY_TYPE['agent-header'].height,
    })
    return { placed, sizes }
  }, [baseGraph, expandedIds, measuredSizes, resetNonce])

  const computedNodes = useMemo(() => {
    const { placed, sizes } = layoutResult
    // View mode reads persisted user-drag positions from project UI state when
    // available; editing mode keeps overrides in memory only (no stable storage
    // key for an unsaved create) so we plug in the in-memory map here instead.
    const stored = editing ? editingPositionOverrides : getStoredPositions()
    const positioned = applyStoredPositions(placed, stored)
    const classed = applyExpandedNodeClass(positioned, expandedIds)
    return applyKnownNodeDimensions(
      classed,
      sizes,
      editing
        ? { domMeasuredHandleNodeTypes: EDITING_DOM_MEASURED_HANDLE_NODE_TYPES }
        : undefined,
    ) as Node[]
  }, [
    editing,
    editingPositionOverrides,
    expandedIds,
    getStoredPositions,
    layoutResult,
    storedPositionsNonce,
  ])

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
      if (canvasInteractionsLocked) {
        clearFocus()
        return
      }
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
      canvasInteractionsLocked,
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

  useEffect(() => {
    if (!canvasInteractionsLocked) return
    clearFocus()
    canvasInteractingRef.current = false
    canvasRef.current?.classList.remove('is-dragging')
  }, [canvasInteractionsLocked, clearFocus])

  // Read mode is view-only: preserve selection and dimension measurements,
  // but ignore position changes so layout cannot be mutated from this path.
  const handleNodesChange = useCallback(
    (changes: NodeChange<Node>[]) => {
      const flowChanges: NodeChange<Node>[] = []

      for (const change of changes) {
        if (change.type === 'dimensions') {
          if (change.dimensions) scheduleMeasuredSize(change.id, change.dimensions)
          continue
        }

        if (canvasInteractionsLocked) continue

        if (change.type === 'position') {
          continue
        }

        flowChanges.push(change)
      }

      if (flowChanges.length > 0) {
        setNodes((current) => {
          const next = applyNodeChanges(flowChanges, current)
          nodesRef.current = next
          return next
        })
      }
    },
    [canvasInteractionsLocked, scheduleMeasuredSize, setNodes],
  )

  // ============================================================
  // Editing-mode handlers. The data source of truth is `editingDetail`;
  // every mutation rewrites that detail, which then flows through the same
  // buildAgentGraph + layoutAgentGraphByCategory pipeline used by view mode.
  // The structural layout of added nodes (tool category frames, lane labels,
  // proper positioning) therefore matches view mode automatically.
  // ============================================================
  const reactFlowForDrop = useReactFlow<Node, Edge>()

  const handleEditingNodesChange = useCallback((changes: NodeChange<Node>[]) => {
    // Mirror view-mode behavior: separate normal changes (which apply to the
    // dragged node directly) from lane-label changes (which propagate the
    // delta to every child in the lane via applyLaneDragPositionChanges).
    const flowChanges: NodeChange<Node>[] = []
    const laneDragChanges: NodeChange<Node>[] = []
    for (const change of changes) {
      if (change.type === 'dimensions') {
        if (change.dimensions) scheduleMeasuredSize(change.id, change.dimensions)
        continue
      }
      if (change.type === 'remove') continue // routed via detail mutation below
      if (change.type === 'position' && isLaneLabelNodeId(change.id)) {
        laneDragChanges.push(change)
        continue
      }
      flowChanges.push(change)
    }

    if (flowChanges.length > 0 || laneDragChanges.length > 0) {
      // Compute the post-change node array synchronously OUTSIDE the
      // setNodes updater so we can read member positions for the override
      // capture without violating "updaters must be pure". nodesRef is
      // populated from a post-render effect, so it's the freshest snapshot
      // we have between renders.
      const current = nodesRef.current
      const withFlowChanges =
        flowChanges.length > 0 ? applyNodeChanges(flowChanges, current) : current
      const next =
        laneDragChanges.length > 0
          ? applyLaneDragPositionChanges(withFlowChanges, laneDragChanges)
          : withFlowChanges

      // For lane drags, capture every member's new position so the override
      // map matches what setNodes is about to commit. The change events
      // only carry the LABEL'S position — member positions live exclusively
      // inside the applyLaneDragPositionChanges result above. Without this,
      // applyStoredPositions on the next layout pass replays the lane label
      // at its drop point but resets every member back to the layout
      // baseline, which is what made edges anchor to the pre-drag location.
      const overrideUpdates: Record<string, { x: number; y: number }> = {}
      for (const change of laneDragChanges) {
        if (change.type !== 'position' || !change.position) continue
        const memberIds = getLaneDragMemberIds(next, change.id)
        for (const member of next) {
          if (!memberIds.has(member.id)) continue
          overrideUpdates[member.id] = {
            x: member.position.x,
            y: member.position.y,
          }
        }
      }
      // Single-node drag-stops: persist the final position so subsequent
      // layout reflows respect the user's manual placement. We persist on
      // drag-stop (not every tick) to avoid recomputing the layout pipeline
      // on every mouse move.
      for (const change of changes) {
        if (
          change.type === 'position' &&
          change.dragging === false &&
          change.position &&
          !isLaneLabelNodeId(change.id)
        ) {
          overrideUpdates[change.id] = {
            x: change.position.x,
            y: change.position.y,
          }
        }
      }

      // Apply both state updates. React batches them inside this event
      // handler, so they commit together on the next render.
      nodesRef.current = next
      setNodes(next)
      if (Object.keys(overrideUpdates).length > 0) {
        setEditingPositionOverrides((currentOverrides) => ({
          ...currentOverrides,
          ...overrideUpdates,
        }))
      }
    }

    // Removals (backspace/delete on selected nodes) route through the
    // detail-mutating path so editingDetail stays the source of truth.
    for (const change of changes) {
      if (change.type === 'remove' && !PROTECTED_NODE_IDS.has(change.id)) {
        removeEditingNodeImpl(change.id)
      }
    }
  }, [])

  const handleEditingEdgesChange = useCallback((_changes: EdgeChange[]) => {
    // Edges are derived from editingDetail (producedByTools, db.triggers,
    // etc.). Edge removals on the canvas would need to translate back into
    // detail mutations; for the v1 of this refactor we treat edges as
    // read-only-derived. User can remove the underlying node via the trash
    // button on the node body to clear its associated edges.
  }, [])

  // Translate a user-drawn connection into the corresponding detail
  // relationship. Examples:
  //   tool → output-section : push toolName to section.producedByTools
  //   tool → db-table       : push tool trigger to db.triggers
  //   section → db-table    : push output_section trigger to db.triggers
  //   consumed → db-table   : push upstream_artifact trigger to db.triggers
  // header → {prompt|tool|db|output} and consumed → header are already
  // implied by the entity's existence and don't need explicit mutation.
  const handleEditingConnect = useCallback((connection: Connection) => {
    if (!connection.source || !connection.target) return
    const source = decodeAgentGraphNodeId(connection.source)
    const target = decodeAgentGraphNodeId(connection.target)
    setEditingDetail((current) => {
      if (!current) return current
      // tool → output-section
      if (source.kind === 'tool' && target.kind === 'output-section') {
        const sections = current.output.sections.map((section) =>
          section.id === target.sectionId
            ? section.producedByTools.includes(source.toolName)
              ? section
              : { ...section, producedByTools: [...section.producedByTools, source.toolName] }
            : section,
        )
        return { ...current, output: { ...current.output, sections } }
      }
      // tool → db-table | section → db-table | consumed → db-table
      if (target.kind === 'db' && (source.kind === 'tool' || source.kind === 'output-section' || source.kind === 'consumed-artifact')) {
        const trigger: AgentTriggerRefDto =
          source.kind === 'tool'
            ? { kind: 'tool', name: source.toolName }
            : source.kind === 'output-section'
              ? { kind: 'output_section', id: source.sectionId }
              : { kind: 'upstream_artifact', id: source.artifactId }
        const dedupKey = (t: AgentTriggerRefDto): string =>
          t.kind === 'tool'
            ? `tool:${t.name}`
            : t.kind === 'output_section'
              ? `section:${t.id}`
              : t.kind === 'upstream_artifact'
                ? `artifact:${t.id}`
                : `lifecycle:${t.event}`
        const triggerKey = dedupKey(trigger)
        const updateBucket = (bucket: typeof current.dbTouchpoints.reads) =>
          bucket.map((entry) =>
            entry.table === target.table && (target.touchpoint === 'read' ? entry.kind === 'read' : target.touchpoint === 'write' ? entry.kind === 'write' : entry.kind === 'encouraged')
              ? entry.triggers.some((t) => dedupKey(t) === triggerKey)
                ? entry
                : { ...entry, triggers: [...entry.triggers, trigger] }
              : entry,
          )
        return {
          ...current,
          dbTouchpoints: {
            reads: target.touchpoint === 'read' ? updateBucket(current.dbTouchpoints.reads) : current.dbTouchpoints.reads,
            writes: target.touchpoint === 'write' ? updateBucket(current.dbTouchpoints.writes) : current.dbTouchpoints.writes,
            encouraged: target.touchpoint === 'encouraged' ? updateBucket(current.dbTouchpoints.encouraged) : current.dbTouchpoints.encouraged,
          },
        }
      }
      return current
    })
  }, [])

  // Map editing-context updateNodeData calls back to editingDetail mutations.
  // Each node id encodes which entity it represents (see decodeAgentGraphNodeId
  // in build-agent-graph). The header path also handles the editingAdvanced
  // store, since that lives outside the WorkflowAgentDetailDto shape.
  const editingAdvancedRef = useRef(editingAdvanced)
  useEffect(() => {
    editingAdvancedRef.current = editingAdvanced
  }, [editingAdvanced])

  const updateEditingNodeData = useCallback<CanvasModeContextValue['updateNodeData']>(
    (nodeId, updater) => {
      const decoded = decodeAgentGraphNodeId(nodeId)
      setEditingDetail((current) => {
        if (!current) return current
        switch (decoded.kind) {
          case 'header': {
            const prev = {
              header: current.header,
              summary: {
                prompts: current.prompts.length,
                tools: current.tools.length,
                dbTables:
                  current.dbTouchpoints.reads.length +
                  current.dbTouchpoints.writes.length +
                  current.dbTouchpoints.encouraged.length,
                outputSections: current.output.sections.length,
                consumes: current.consumes.length,
                attachedSkills: current.attachedSkills.length,
              },
              advanced: editingAdvancedRef.current,
            }
            const next = updater(prev as never) as typeof prev
            // Push advanced back to its own state if it changed.
            if (next.advanced !== prev.advanced) {
              setEditingAdvanced(next.advanced)
            }
            if (next.header === prev.header) return current
            return { ...current, header: next.header }
          }
          case 'output': {
            const prev = { output: current.output }
            const next = updater(prev as never) as typeof prev
            if (next.output === prev.output) return current
            return { ...current, output: next.output }
          }
          case 'prompt': {
            const prompts = current.prompts.map((p, i) => {
              if (promptNodeId(p, i) !== nodeId) return p
              const prev = { prompt: p }
              const next = updater(prev as never) as typeof prev
              return next.prompt
            })
            return { ...current, prompts }
          }
          case 'skills': {
            const attachedSkills = current.attachedSkills.map((skill) => {
              if (skillNodeId(skill) !== nodeId) return skill
              const prev = { skill }
              const next = updater(prev as never) as typeof prev
              return next.skill
            })
            return { ...current, attachedSkills }
          }
          case 'tool': {
            const tools = current.tools.map((tool) => {
              if (toolNodeId(tool) !== nodeId) return tool
              const prev = {
                tool,
                directConnectionHandles: { source: false, target: false },
              }
              const next = updater(prev as never) as typeof prev
              return next.tool
            })
            return { ...current, tools }
          }
          case 'db': {
            const updateBucket = (bucket: typeof current.dbTouchpoints.reads) =>
              bucket.map((entry) => {
                if (entry.table !== decoded.table) return entry
                const prev = {
                  table: entry.table,
                  touchpoint: entry.kind,
                  purpose: entry.purpose,
                  triggers: entry.triggers,
                  columns: entry.columns,
                }
                const next = updater(prev as never) as typeof prev
                return {
                  table: next.table,
                  kind: next.touchpoint,
                  purpose: next.purpose,
                  triggers: next.triggers,
                  columns: next.columns,
                }
              })
            return {
              ...current,
              dbTouchpoints: {
                reads:
                  decoded.touchpoint === 'read'
                    ? updateBucket(current.dbTouchpoints.reads)
                    : current.dbTouchpoints.reads,
                writes:
                  decoded.touchpoint === 'write'
                    ? updateBucket(current.dbTouchpoints.writes)
                    : current.dbTouchpoints.writes,
                encouraged:
                  decoded.touchpoint === 'encouraged'
                    ? updateBucket(current.dbTouchpoints.encouraged)
                    : current.dbTouchpoints.encouraged,
              },
            }
          }
          case 'output-section': {
            const sections = current.output.sections.map((section) => {
              if (section.id !== decoded.sectionId) return section
              const prev = { section }
              const next = updater(prev as never) as typeof prev
              return next.section
            })
            return { ...current, output: { ...current.output, sections } }
          }
          case 'consumed-artifact': {
            const consumes = current.consumes.map((artifact) => {
              if (artifact.id !== decoded.artifactId) return artifact
              const prev = { artifact }
              const next = updater(prev as never) as typeof prev
              return next.artifact
            })
            return { ...current, consumes }
          }
          default:
            return current
        }
      })
    },
    [],
  )

  // Defined as a closure so handleEditingNodesChange can call it without a
  // forward reference. Removes a node from the corresponding detail array.
  const removeEditingNodeImpl = useCallback((nodeId: string) => {
    if (PROTECTED_NODE_IDS.has(nodeId)) return
    const decoded = decodeAgentGraphNodeId(nodeId)
    setEditingDetail((current) => {
      if (!current) return current
      switch (decoded.kind) {
        case 'prompt':
          return {
            ...current,
            prompts: current.prompts.filter(
              (p, i) => promptNodeId(p, i) !== nodeId,
            ),
          }
        case 'skills':
          return {
            ...current,
            attachedSkills: current.attachedSkills.filter(
              (skill) => skillNodeId(skill) !== nodeId,
            ),
          }
        case 'tool': {
          const removedName = decoded.toolName
          // Drop matching tool, plus any references in section.producedByTools
          // and db.triggers so the detail stays consistent.
          const tools = current.tools.filter((tool) => toolNodeId(tool) !== nodeId)
          const sections = current.output.sections.map((section) => ({
            ...section,
            producedByTools: section.producedByTools.filter((name) => name !== removedName),
          }))
          const stripTool = (entry: typeof current.dbTouchpoints.reads[number]) => ({
            ...entry,
            triggers: entry.triggers.filter(
              (t) => !(t.kind === 'tool' && t.name === removedName),
            ),
          })
          return {
            ...current,
            tools,
            output: { ...current.output, sections },
            dbTouchpoints: {
              reads: current.dbTouchpoints.reads.map(stripTool),
              writes: current.dbTouchpoints.writes.map(stripTool),
              encouraged: current.dbTouchpoints.encouraged.map(stripTool),
            },
          }
        }
        case 'db': {
          const filterBucket = (bucket: typeof current.dbTouchpoints.reads) =>
            bucket.filter((entry) => entry.table !== decoded.table)
          return {
            ...current,
            dbTouchpoints: {
              reads:
                decoded.touchpoint === 'read'
                  ? filterBucket(current.dbTouchpoints.reads)
                  : current.dbTouchpoints.reads,
              writes:
                decoded.touchpoint === 'write'
                  ? filterBucket(current.dbTouchpoints.writes)
                  : current.dbTouchpoints.writes,
              encouraged:
                decoded.touchpoint === 'encouraged'
                  ? filterBucket(current.dbTouchpoints.encouraged)
                  : current.dbTouchpoints.encouraged,
            },
          }
        }
        case 'output-section': {
          const removedId = decoded.sectionId
          // Remove section + any db.triggers referencing it.
          const sections = current.output.sections.filter((s) => s.id !== removedId)
          const stripSection = (entry: typeof current.dbTouchpoints.reads[number]) => ({
            ...entry,
            triggers: entry.triggers.filter(
              (t) => !(t.kind === 'output_section' && t.id === removedId),
            ),
          })
          return {
            ...current,
            output: { ...current.output, sections },
            dbTouchpoints: {
              reads: current.dbTouchpoints.reads.map(stripSection),
              writes: current.dbTouchpoints.writes.map(stripSection),
              encouraged: current.dbTouchpoints.encouraged.map(stripSection),
            },
          }
        }
        case 'consumed-artifact': {
          const removedId = decoded.artifactId
          const consumes = current.consumes.filter((a) => a.id !== removedId)
          const stripArtifact = (entry: typeof current.dbTouchpoints.reads[number]) => ({
            ...entry,
            triggers: entry.triggers.filter(
              (t) => !(t.kind === 'upstream_artifact' && t.id === removedId),
            ),
          })
          return {
            ...current,
            consumes,
            dbTouchpoints: {
              reads: current.dbTouchpoints.reads.map(stripArtifact),
              writes: current.dbTouchpoints.writes.map(stripArtifact),
              encouraged: current.dbTouchpoints.encouraged.map(stripArtifact),
            },
          }
        }
        default:
          return current
      }
    })
  }, [])

  const removeEditingNode = removeEditingNodeImpl

  const removeEditingToolGroup = useCallback((sourceGroups: readonly string[]) => {
    const groupSet = new Set(
      sourceGroups.map((group) => group.trim() || 'other'),
    )
    if (groupSet.size === 0) return

    setEditingDetail((current) => {
      if (!current) return current
      const removedNames = new Set(
        current.tools
          .filter((tool) => groupSet.has(tool.group?.trim() || 'other'))
          .map((tool) => tool.name),
      )
      if (removedNames.size === 0) return current

      const tools = current.tools.filter((tool) => !removedNames.has(tool.name))
      const sections = current.output.sections.map((section) => ({
        ...section,
        producedByTools: section.producedByTools.filter(
          (name) => !removedNames.has(name),
        ),
      }))
      const stripRemovedTools = (entry: typeof current.dbTouchpoints.reads[number]) => ({
        ...entry,
        triggers: entry.triggers.filter(
          (trigger) => !(trigger.kind === 'tool' && removedNames.has(trigger.name)),
        ),
      })

      return {
        ...current,
        tools,
        output: { ...current.output, sections },
        dbTouchpoints: {
          reads: current.dbTouchpoints.reads.map(stripRemovedTools),
          writes: current.dbTouchpoints.writes.map(stripRemovedTools),
          encouraged: current.dbTouchpoints.encouraged.map(stripRemovedTools),
        },
      }
    })
  }, [])

  // Drag-from-handle state. We watch onConnectStart to remember which
  // handle the user is pulling from, and onConnectEnd to detect when they
  // released the drag onto empty canvas (rather than another node). When
  // they release on empty canvas we either create the relevant node
  // immediately (single-option drags) or open a small inline picker at the
  // drop point.
  const connectAttemptRef = useRef<{
    sourceId: string
    sourceHandle: string | null
    handleType: 'source' | 'target'
  } | null>(null)

  type DropPickerKind = 'skill' | 'tool-category' | 'db-table' | 'consumed-artifact'
  interface DropPickerState {
    kind: DropPickerKind
    // Screen-space anchor for the popup itself.
    screenX: number
    screenY: number
    // Flow-space coordinates where the new node will be placed.
    flowX: number
    flowY: number
    sourceId: string
    sourceHandle: string | null
    handleType: 'source' | 'target'
  }
  const [dropPicker, setDropPicker] = useState<DropPickerState | null>(null)
  const closeDropPicker = useCallback(() => setDropPicker(null), [])

  // Drag-add functions push entities into editingDetail. The layout pipeline
  // re-runs after every change and slots the new entity into its proper lane
  // (TOOLS frame, DATABASE column, etc.) — exactly where view mode would
  // render it. The flowPos that comes from the drag-end is therefore mostly
  // informational at this point: the layout authoritatively decides
  // placement; the user can drag afterward to refine, and the drag is
  // captured into editingPositionOverrides.

  // Generate a unique entity id by suffixing a small counter when a base
  // id collides with one already in `existing`. Used for prompt / artifact /
  // section ids that must be unique within their lists.
  const uniqueEntityId = (base: string, existing: readonly { id: string }[]): string => {
    let id = base
    let n = 2
    while (existing.some((e) => e.id === id)) {
      id = `${base}_${n++}`
    }
    return id
  }

  const addPromptFromDrag = useCallback(() => {
    setEditingDetail((current) => {
      if (!current) return current
      const id = uniqueEntityId('custom_prompt', current.prompts)
      const index = current.prompts.length
      return {
        ...current,
        prompts: [
          ...current.prompts,
          {
            id,
            label: `Prompt ${index + 1}`,
            role: 'system',
            source: 'custom',
            policy: null,
            body:
              'Replace this with the prompt body. Describe what the agent should do at this stage.',
          },
        ],
      }
    })
  }, [])

  const addOutputSectionFromDrag = useCallback(() => {
    setEditingDetail((current) => {
      if (!current) return current
      const id = uniqueEntityId('custom_section', current.output.sections)
      const index = current.output.sections.length
      return {
        ...current,
        output: {
          ...current.output,
          sections: [
            ...current.output.sections,
            {
              id,
              label: `Section ${index + 1}`,
              description: '',
              emphasis: 'standard',
              producedByTools: [],
            },
          ],
        },
      }
    })
  }, [])

  // Tool category drags add every tool in the chosen category. The layout's
  // tool-group-frame logic then groups them visually, matching view mode.
  const addToolCategoryFromDrag = useCallback(
    (categoryId: string) => {
      if (!authoringCatalog) return
      const category = authoringCatalog.toolCategories.find(
        (entry) => entry.id === categoryId,
      )
      if (!category || category.tools.length === 0) return
      setEditingDetail((current) => {
        if (!current) return current
        const existingNames = new Set(current.tools.map((t) => t.name))
        const additions = category.tools
          .filter((tool) => !existingNames.has(tool.name))
          .map((tool) => ({
            name: tool.name,
            group: tool.group,
            description: tool.description,
            effectClass: tool.effectClass,
            riskClass: tool.riskClass,
            tags: [...tool.tags],
            schemaFields: [...tool.schemaFields],
            examples: [...tool.examples],
          }))
        if (additions.length === 0) return current
        return { ...current, tools: [...current.tools, ...additions] }
      })
      closeDropPicker()
    },
    [authoringCatalog, closeDropPicker],
  )

  const addSkillFromDrag = useCallback(
    (entry: AgentAuthoringAttachableSkillDto) => {
      setEditingDetail((current) => {
        if (!current) return current
        if (current.attachedSkills.some((skill) => skill.sourceId === entry.sourceId)) {
          return current
        }
        const id = uniqueEntityId(entry.attachment.id, current.attachedSkills)
        return {
          ...current,
          attachedSkills: [
            ...current.attachedSkills,
            {
              ...entry.attachment,
              id,
              sourceState: entry.sourceState,
              trustState: entry.trustState,
              availabilityStatus: entry.availabilityStatus,
              availabilityReason: 'Skill source is available in the authoring catalog.',
              repairHint: null,
            },
          ],
        }
      })
      closeDropPicker()
    },
    [closeDropPicker],
  )

  const addDbTableFromDrag = useCallback(
    (tableName: string) => {
      if (!authoringCatalog) return
      const entry = authoringCatalog.dbTables.find((db) => db.table === tableName)
      if (!entry) return
      setEditingDetail((current) => {
        if (!current) return current
        // Prevent duplicate (table+kind) pairs — buildAgentGraph keys nodes
        // on `db:${kind}:${table}` so dupes would collapse into one node.
        const exists = current.dbTouchpoints.reads.some((db) => db.table === entry.table)
        if (exists) return current
        return {
          ...current,
          dbTouchpoints: {
            ...current.dbTouchpoints,
            reads: [
              ...current.dbTouchpoints.reads,
              {
                table: entry.table,
                kind: 'read',
                purpose: entry.purpose,
                triggers: [],
                columns: [...entry.columns],
              },
            ],
          },
        }
      })
      closeDropPicker()
    },
    [authoringCatalog, closeDropPicker],
  )

  const addConsumedArtifactFromDrag = useCallback(
    (key: string) => {
      if (!authoringCatalog) return
      const entry = authoringCatalog.upstreamArtifacts.find(
        (artifact) => `${artifact.sourceAgent}::${artifact.contract}` === key,
      )
      if (!entry) return
      setEditingDetail((current) => {
        if (!current) return current
        const baseId = `${entry.sourceAgent}_${entry.contract}`
        const id = uniqueEntityId(baseId, current.consumes)
        return {
          ...current,
          consumes: [
            ...current.consumes,
            {
              id,
              label: entry.label,
              description: entry.description,
              sourceAgent: entry.sourceAgent,
              contract: entry.contract,
              sections: entry.sections.map((section) => section.id),
              required: false,
            },
          ],
        }
      })
      closeDropPicker()
    },
    [authoringCatalog, closeDropPicker],
  )

  const onEditingConnectStart = useCallback<
    NonNullable<React.ComponentProps<typeof ReactFlow>['onConnectStart']>
  >((_event, params) => {
    const root = canvasRef.current
    if (!params.nodeId) {
      connectAttemptRef.current = null
      root?.removeAttribute('data-drag-role')
      root?.removeAttribute('data-drag-source-kind')
      return
    }
    const handleType = (params.handleType as 'source' | 'target') ?? 'source'
    connectAttemptRef.current = {
      sourceId: params.nodeId,
      sourceHandle: params.handleId ?? null,
      handleType,
    }
    // Mark the canvas root so CSS can branch on the active drag's role and
    // source kind. This drives the truthful highlighting: picker drags
    // (header.tool, header.skills, etc.) suppress all target highlights;
    // connection drags fall back to React Flow's `valid` class on
    // legitimate target handles.
    const sourceNode = reactFlowForDrop.getNode(params.nodeId)
    const role = getHandleDragRole(sourceNode?.type, params.handleId, handleType)
    root?.setAttribute('data-drag-role', role)
    if (sourceNode?.type) {
      root?.setAttribute('data-drag-source-kind', sourceNode.type)
    } else {
      root?.removeAttribute('data-drag-source-kind')
    }
  }, [reactFlowForDrop])

  const onEditingConnectEnd = useCallback<
    NonNullable<React.ComponentProps<typeof ReactFlow>['onConnectEnd']>
  >(
    (event) => {
      const attempt = connectAttemptRef.current
      connectAttemptRef.current = null
      const root = canvasRef.current
      root?.removeAttribute('data-drag-role')
      root?.removeAttribute('data-drag-source-kind')
      if (!attempt) return
      // React Flow calls onConnect when the drop is on a valid handle, and
      // calls onConnectEnd in BOTH cases (handle drop + empty drop). We only
      // want to react to drops on empty canvas — detect by walking up from
      // event.target to see if we hit the React Flow pane.
      const target = (event.target as Element | null) ?? null
      const onPane = Boolean(
        target?.closest?.('.react-flow__pane') ||
          (target?.classList && target.classList.contains('react-flow__pane')),
      )
      if (!onPane) return
      const clientX =
        'clientX' in event ? (event as MouseEvent).clientX : 0
      const clientY =
        'clientY' in event ? (event as MouseEvent).clientY : 0
      const flowPos = reactFlowForDrop.screenToFlowPosition({
        x: clientX,
        y: clientY,
      })

      // Decide what kind of node to create. Single-option drags render
      // immediately; multi-option drags open the inline picker.
      if (attempt.sourceId === AGENT_GRAPH_HEADER_NODE_ID) {
        if (attempt.sourceHandle === AGENT_GRAPH_HEADER_HANDLES.prompt) {
          addPromptFromDrag()
          return
        }
        if (attempt.sourceHandle === AGENT_GRAPH_HEADER_HANDLES.skills) {
          setDropPicker({
            kind: 'skill',
            screenX: clientX,
            screenY: clientY,
            flowX: flowPos.x,
            flowY: flowPos.y,
            sourceId: attempt.sourceId,
            sourceHandle: attempt.sourceHandle,
            handleType: attempt.handleType,
          })
          return
        }
        if (attempt.sourceHandle === AGENT_GRAPH_HEADER_HANDLES.tool) {
          setDropPicker({
            kind: 'tool-category',
            screenX: clientX,
            screenY: clientY,
            flowX: flowPos.x,
            flowY: flowPos.y,
            sourceId: attempt.sourceId,
            sourceHandle: attempt.sourceHandle,
            handleType: attempt.handleType,
          })
          return
        }
        if (attempt.sourceHandle === AGENT_GRAPH_HEADER_HANDLES.db) {
          setDropPicker({
            kind: 'db-table',
            screenX: clientX,
            screenY: clientY,
            flowX: flowPos.x,
            flowY: flowPos.y,
            sourceId: attempt.sourceId,
            sourceHandle: attempt.sourceHandle,
            handleType: attempt.handleType,
          })
          return
        }
        if (attempt.sourceHandle === AGENT_GRAPH_HEADER_HANDLES.consumed) {
          setDropPicker({
            kind: 'consumed-artifact',
            screenX: clientX,
            screenY: clientY,
            flowX: flowPos.x,
            flowY: flowPos.y,
            sourceId: attempt.sourceId,
            sourceHandle: attempt.sourceHandle,
            handleType: attempt.handleType,
          })
          return
        }
        // header.output drag has no node to add — output already exists.
        return
      }
      // Output node bottom source drag → add an output-section.
      if (attempt.sourceId === AGENT_GRAPH_OUTPUT_NODE_ID && attempt.handleType === 'source') {
        addOutputSectionFromDrag()
        return
      }
    },
    [addOutputSectionFromDrag, addPromptFromDrag, reactFlowForDrop],
  )

  // Gate which target handles light up during a drag. React Flow adds the
  // `valid` class only to handles where this returns true — so picker-role
  // sources (header.tools, header.skills, etc.) keep every other handle
  // dark, and connection-role sources only light up handles that would form
  // a legitimate edge per ALLOWED_PAIRS.
  const isValidEditingConnection = useCallback<
    NonNullable<React.ComponentProps<typeof ReactFlow>['isValidConnection']>
  >(
    (conn) => {
      if (!conn.source || !conn.target) return false
      const sourceNode = reactFlowForDrop.getNode(conn.source)
      const targetNode = reactFlowForDrop.getNode(conn.target)
      if (!sourceNode || !targetNode) return false
      const sourceRole = getHandleDragRole(sourceNode.type, conn.sourceHandle, 'source')
      if (sourceRole === 'picker') return false
      const targetRole = getHandleDragRole(targetNode.type, conn.targetHandle, 'target')
      if (targetRole === 'picker') return false
      return isEdgeAllowed(
        sourceNode.type as AgentGraphNodeKind,
        targetNode.type as AgentGraphNodeKind,
      )
    },
    [reactFlowForDrop],
  )

  // Validation runs on the computed (layout-derived) nodes/edges in editing
  // mode — those are the source of structural truth, not the React Flow
  // working state which can have transient drag updates.
  const editingValidation = useMemo(
    () =>
      editing
        ? validateEdges(computedNodes, computedEdges)
        : { invalidEdgeIds: new Set<string>(), diagnostics: [] },
    [editing, computedNodes, computedEdges],
  )
  const editingStructuralDiagnostics = useMemo(
    () => (editing ? validateStructure({ nodes: computedNodes }) : []),
    [editing, computedNodes],
  )
  const decoratedEditingEdges = useMemo(
    () =>
      editing
        ? applyEdgeValidationClasses(computedEdges, editingValidation.invalidEdgeIds)
        : computedEdges,
    [editing, computedEdges, editingValidation.invalidEdgeIds],
  )

  const editingCombinedDiagnostics = useMemo<
    readonly AgentDefinitionValidationDiagnosticDto[]
  >(
    () => [
      ...editingValidation.diagnostics.map((diagnostic) => ({
        code: 'invalid_edge',
        message: diagnostic.message,
        path: diagnostic.edgeId,
      })),
      ...editingStructuralDiagnostics.map((diagnostic) => ({
        code: diagnostic.code,
        message: diagnostic.message,
        path: diagnostic.path,
      })),
      ...editingServerDiagnostics,
    ],
    [
      editingValidation.diagnostics,
      editingStructuralDiagnostics,
      editingServerDiagnostics,
    ],
  )

  const handleEditingSave = useCallback(async () => {
    if (!onSubmit || editingSaving) return
    if (editingValidation.invalidEdgeIds.size > 0) return
    if (editingStructuralDiagnostics.length > 0) return
    setEditingSaving(true)
    setEditingErrorMessage(null)
    setEditingServerDiagnostics([])
    try {
      const initialDefinitionId =
        mode === 'edit' && editingInitial?.detail.ref.kind === 'custom'
          ? editingInitial.detail.ref.definitionId
          : null
      const { snapshot, definitionId } = buildSnapshotFromGraph(
        computedNodes as unknown as AgentGraphNode[],
        computedEdges,
        {
          initialDefinitionId,
          attachedSkills: editingDetail?.attachedSkills ?? [],
        },
      )
      const response = await onSubmit({
        snapshot,
        mode: mode ?? 'create',
        definitionId,
      })
      if (!response.applied) {
        setEditingServerDiagnostics(response.validation.diagnostics)
        setEditingErrorMessage(response.message || 'Agent definition failed validation.')
        return
      }
      onSaved?.(response)
    } catch (error) {
      setEditingErrorMessage(error instanceof Error ? error.message : String(error))
    } finally {
      setEditingSaving(false)
    }
  }, [
    computedEdges,
    computedNodes,
    editingInitial,
    editingDetail,
    editingSaving,
    editingStructuralDiagnostics.length,
    editingValidation.invalidEdgeIds,
    mode,
    onSaved,
    onSubmit,
  ])

  const editingInferredAdvanced = useMemo(() => {
    if (!editing || !editingDetail) return emptyInferredAdvanced()
    return inferAdvancedFromConnections(editingDetail)
  }, [editing, editingDetail])

  const canvasModeContextValue = useMemo<CanvasModeContextValue>(
    () =>
      editing
        ? {
            editing: true,
            mode: mode ?? 'create',
            updateNodeData: updateEditingNodeData,
            removeNode: removeEditingNode,
            removeToolGroup: removeEditingToolGroup,
            authoringCatalog: authoringCatalog ?? null,
            inferredAdvanced: editingInferredAdvanced,
          }
        : {
            editing: false,
            mode: null,
            updateNodeData: () => {},
            removeNode: () => {},
            removeToolGroup: () => {},
            authoringCatalog: null,
            inferredAdvanced: emptyInferredAdvanced(),
          },
    [
      authoringCatalog,
      editing,
      editingInferredAdvanced,
      mode,
      removeEditingNode,
      removeEditingToolGroup,
      updateEditingNodeData,
    ],
  )

  // Publish editing status up to the embedding chrome so phase-view can
  // render Save / Cancel buttons in its existing top-right cluster instead of
  // having the canvas paint its own toolbar.
  useEffect(() => {
    if (!editing) {
      onEditingStatusChange?.(null)
      return
    }
    onEditingStatusChange?.({
      saving: editingSaving,
      saveDisabled:
        editingSaving ||
        editingValidation.invalidEdgeIds.size > 0 ||
        editingStructuralDiagnostics.length > 0,
      hasInvalidEdges: editingValidation.invalidEdgeIds.size > 0,
      errorMessage: editingErrorMessage,
      diagnosticCount: editingCombinedDiagnostics.length,
      diagnostics: editingCombinedDiagnostics,
      save: handleEditingSave,
    })
    return () => {
      onEditingStatusChange?.(null)
    }
  }, [
    editing,
    editingSaving,
    editingStructuralDiagnostics.length,
    editingValidation.invalidEdgeIds.size,
    editingErrorMessage,
    editingCombinedDiagnostics,
    handleEditingSave,
    onEditingStatusChange,
  ])

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

  const viewFitKey =
    hasIncomingDetail && storageKey.length > 0 && canvasBoundsKey.length > 0
      ? `${storageKey}:${canvasBoundsKey}`
      : ''
  const lastViewFitKeyRef = useRef<string | null>(null)
  useEffect(() => {
    if (editing) return
    if (!active) {
      lastViewFitKeyRef.current = null
      return
    }

    if (!hasIncomingDetail) {
      lastViewFitKeyRef.current = null
      void reactFlow.setViewport(EMPTY_CANVAS_DEFAULT_VIEWPORT, {
        duration: FIT_VIEW_TRANSITION_MS,
      })
      return
    }

    if (
      computedNodes.length === 0 ||
      viewFitKey.length === 0 ||
      lastViewFitKeyRef.current === viewFitKey
    ) {
      return
    }

    lastViewFitKeyRef.current = viewFitKey
    const frame = window.requestAnimationFrame(() => {
      void reactFlow.fitView({
        ...FIT_VIEW_OPTIONS,
        duration: FIT_VIEW_TRANSITION_MS,
      })
    })
    return () => {
      window.cancelAnimationFrame(frame)
    }
  }, [active, computedNodes.length, editing, hasIncomingDetail, reactFlow, viewFitKey])

  const createModeFitNodeBounds = useMemo(
    () => (editing && (mode ?? 'create') === 'create' ? visibleNodeBounds(nodes) : null),
    [editing, mode, nodes],
  )
  const createModeFitKey = createModeFitNodeBounds
    ? `${canvasBoundsKey}:${viewportBoundsKey(createModeFitNodeBounds)}`
    : ''
  const createModeHasSelection = editing && nodes.some((node) => node.selected)
  const initialCreateFitKeyRef = useRef<string | null>(null)
  const initialCreateFitCanvasBoundsKeyRef = useRef<string | null>(null)
  const initialCreateFitPassCountRef = useRef(0)
  const createModeWasActiveRef = useRef(false)
  const createModeViewportFittedRef = useRef(false)
  const createModeInitialFitReadyRef = useRef(false)
  const createModeInitialFitRevealTimerRef = useRef<number | null>(null)
  const [createModeInitialFitReady, setCreateModeInitialFitReady] = useState(false)
  const isCreateModeAuthoring = editing && (mode ?? 'create') === 'create'
  useEffect(() => {
    if (!active || !isCreateModeAuthoring) {
      createModeWasActiveRef.current = false
      createModeViewportFittedRef.current = false
      createModeInitialFitReadyRef.current = false
      setCreateModeInitialFitReady(false)
      if (createModeInitialFitRevealTimerRef.current !== null) {
        window.clearTimeout(createModeInitialFitRevealTimerRef.current)
        createModeInitialFitRevealTimerRef.current = null
      }
      return
    }

    if (!createModeWasActiveRef.current) {
      initialCreateFitKeyRef.current = null
      initialCreateFitCanvasBoundsKeyRef.current = null
      initialCreateFitPassCountRef.current = 0
      createModeViewportFittedRef.current = false
      createModeInitialFitReadyRef.current = false
      setCreateModeInitialFitReady(false)
    }
    createModeWasActiveRef.current = true

    // React Activity hides inactive panes by cleaning up effects without
    // running an `active={false}` effect pass. Mark the canvas inactive during
    // cleanup so returning to Workflow always gets a fresh create-mode fit.
    return () => {
      createModeWasActiveRef.current = false
    }
  }, [active, isCreateModeAuthoring])

  useEffect(
    () => () => {
      if (createModeInitialFitRevealTimerRef.current !== null) {
        window.clearTimeout(createModeInitialFitRevealTimerRef.current)
        createModeInitialFitRevealTimerRef.current = null
      }
    },
    [],
  )

  useEffect(() => {
    if (!active || !editing || (mode ?? 'create') !== 'create') {
      initialCreateFitKeyRef.current = null
      initialCreateFitCanvasBoundsKeyRef.current = null
      initialCreateFitPassCountRef.current = 0
      return
    }
    if (createModeHasSelection) {
      return
    }
    const canvasBoundsChanged =
      canvasBoundsKey.length > 0 &&
      initialCreateFitCanvasBoundsKeyRef.current !== canvasBoundsKey
    if (
      !hasDetail ||
      nodes.length === 0 ||
      !canvasBounds ||
      !createModeFitNodeBounds ||
      !createModeFitKey ||
      (initialCreateFitKeyRef.current === createModeFitKey && !canvasBoundsChanged) ||
      (
        !canvasBoundsChanged &&
        initialCreateFitPassCountRef.current >= CREATE_MODE_INITIAL_FIT_MAX_PASSES
      )
    ) {
      return
    }

    if (canvasBoundsChanged) {
      initialCreateFitCanvasBoundsKeyRef.current = canvasBoundsKey
      initialCreateFitPassCountRef.current = 0
    }
    initialCreateFitKeyRef.current = createModeFitKey
    initialCreateFitPassCountRef.current += 1
    let secondFrame: number | null = null
    const firstFrame = window.requestAnimationFrame(() => {
      secondFrame = window.requestAnimationFrame(() => {
        const duration = createModeViewportFittedRef.current
          ? CREATE_MODE_REFIT_TRANSITION_MS
          : CREATE_MODE_INITIAL_FIT_TRANSITION_MS
        void reactFlow.setViewport(
          createModeInitialViewport(createModeFitNodeBounds, canvasBounds),
          { duration },
        )
        createModeViewportFittedRef.current = true
        if (!createModeInitialFitReadyRef.current) {
          if (createModeInitialFitRevealTimerRef.current !== null) {
            window.clearTimeout(createModeInitialFitRevealTimerRef.current)
          }
          createModeInitialFitRevealTimerRef.current = window.setTimeout(() => {
            createModeInitialFitRevealTimerRef.current = null
            createModeInitialFitReadyRef.current = true
            setCreateModeInitialFitReady(true)
          }, CREATE_MODE_INITIAL_FIT_REVEAL_DELAY_MS)
        }
      })
    })
    return () => {
      window.cancelAnimationFrame(firstFrame)
      if (secondFrame !== null) window.cancelAnimationFrame(secondFrame)
      if (!createModeInitialFitReadyRef.current && createModeInitialFitRevealTimerRef.current !== null) {
        window.clearTimeout(createModeInitialFitRevealTimerRef.current)
        createModeInitialFitRevealTimerRef.current = null
      }
    }
  }, [
    active,
    canvasBounds,
    canvasBoundsKey,
    createModeFitKey,
    createModeFitNodeBounds,
    createModeHasSelection,
    editing,
    hasDetail,
    mode,
    nodes.length,
    reactFlow,
  ])

  const editingAutoFitNodeCountRef = useRef<number | null>(null)
  useEffect(() => {
    if (!active) return
    if (!editing || !hasDetail || nodes.length === 0) {
      editingAutoFitNodeCountRef.current = null
      editingSeedBaselineTargetRef.current = null
      return
    }

    const seedBaselineTarget = editingSeedBaselineTargetRef.current
    if (seedBaselineTarget) {
      if (renderedDetail !== seedBaselineTarget) return
      if (nodes.length !== computedNodes.length) return
      editingSeedBaselineTargetRef.current = null
      editingAutoFitNodeCountRef.current = nodes.length
      return
    }

    const previousCount = editingAutoFitNodeCountRef.current
    editingAutoFitNodeCountRef.current = nodes.length
    if (previousCount === null || nodes.length <= previousCount) return

    const frame = window.requestAnimationFrame(() => {
      void reactFlow.fitView({
        ...FIT_VIEW_OPTIONS,
        duration: FIT_VIEW_TRANSITION_MS,
      })
    })
    return () => {
      window.cancelAnimationFrame(frame)
    }
  }, [active, computedNodes.length, editing, hasDetail, nodes.length, reactFlow, renderedDetail])

  const handleResetLayout = useCallback(() => {
    if (hasProjectUiStateStorage) {
      void onWriteProjectUiState?.(projectUiStateKey, null).catch(() => {})
    } else if (typeof window !== 'undefined') {
      try {
        window.localStorage.removeItem(storageKey)
      } catch {
        // Ignore — storage may be disabled.
      }
    }
    storedPositionsRef.current = {
      key: hasProjectUiStateStorage ? projectUiStateKey : storageKey,
      positions: {},
    }
    setStoredPositionsNonce((nonce) => nonce + 1)
    // Editing mode keeps drag overrides in memory instead of durable storage,
    // so a reset has to clear that map too — otherwise the layout pipeline
    // keeps replaying the user's manual moves. We also force a fitView
    // because the per-detail fit-view effect only runs on storageKey
    // transitions, which don't apply in the unsaved authoring flow.
    if (editing) {
      setEditingPositionOverrides({})
      window.requestAnimationFrame(() => {
        void reactFlow.fitView({
          ...FIT_VIEW_OPTIONS,
          duration: FIT_VIEW_TRANSITION_MS,
        })
      })
    }
    canvasInteractingRef.current = false
    canvasRef.current?.classList.remove('is-dragging')
    setResetNonce((n) => n + 1)
  }, [
    editing,
    hasProjectUiStateStorage,
    onWriteProjectUiState,
    projectUiStateKey,
    reactFlow,
    storageKey,
  ])

  // Resolve final ReactFlow inputs based on mode. Edit mode uses mutable
  // editing state; view mode uses the layout-derived pipeline above. Both
  // paths feed the same dots, controls, and JSX shell below so the canvas
  // looks identical in either mode.
  // Both modes share the layout-computed nodes (`nodes` from useNodesState
  // syncs with computedNodes via the effect above). Editing mode just
  // overrides edge styling to flag invalid pairings, and routes node-change
  // events through a different handler so position drags/removals can
  // mutate editingDetail / editingPositionOverrides.
  const readOnlyNodes = useMemo(
    () =>
      nodes.map((node) =>
        node.draggable === true ? { ...node, draggable: false } : node,
      ),
    [nodes],
  )
  const finalNodes = editing ? nodes : readOnlyNodes
  const finalEdges = editing ? decoratedEditingEdges : computedEdges
  const finalOnNodesChange = editing ? handleEditingNodesChange : handleNodesChange
  const finalNodesDraggable = editing && !canvasInteractionsLocked
  const showLayoutControls = editing
  const showCanvasControls = hasDetail || editing
  const showEmptyState = !editing && Boolean(emptyState)
  // The properties / details panel is driven by React Flow's built-in selection.
  // Only the first selected node is shown; layout chrome (lane labels, tool
  // group frames) is ignored since it has no inspectable data.
  const selectedAuthoringNode = useMemo(() => {
    for (const node of finalNodes) {
      if (!node.selected) continue
      if (node.type === 'lane-label' || node.type === 'tool-group-frame') continue
      return node as AgentGraphNode
    }
    return null
  }, [finalNodes])
  const clearAuthoringSelection = useCallback(() => {
    setNodes((current) =>
      current.some((node) => node.selected)
        ? current.map((node) => (node.selected ? { ...node, selected: false } : node))
        : current,
    )
  }, [setNodes])
  // Pan/zoom to the selected node so the panel-driven editor has a clear visual
  // anchor. Keyed on the node id so panning only fires when selection changes,
  // not on every position drag. The horizontal offset compensates for the
  // properties panel's footprint on the left so the node centers in the
  // un-occluded portion of the canvas.
  //
  // Selecting a node can also trigger the host chrome to auto-collapse a
  // sidebar (so the properties panel has a clean stage). That collapse fires
  // a width transition on the canvas container — if we issue setCenter while
  // the container is still resizing, the moving viewport cancels our pan
  // mid-flight and lands the node off-target. To avoid that we wait until
  // the container has been size-stable for a beat before kicking off the
  // animation. When no resize is in progress we fire on the next frame.
  const selectedAuthoringNodeId = selectedAuthoringNode?.id ?? null
  useEffect(() => {
    if (!active) return
    if (!selectedAuthoringNodeId) return
    if (typeof reactFlow.setCenter !== 'function') return
    const root = canvasRef.current
    if (!root) return

    const fire = () => {
      const target = nodesRef.current.find((node) => node.id === selectedAuthoringNodeId)
      if (!target) return
      const focusZoom = 1.05
      const center = selectedNodeFocusCenter(
        target,
        nodesRef.current,
        PROPERTIES_PANEL_FOCUS_FOOTPRINT_PX,
        focusZoom,
      )
      void reactFlow.setCenter(center.x, center.y, { duration: 400, zoom: focusZoom })
    }

    let cancelled = false
    let stableTimer: number | null = null
    let lastWidth = root.clientWidth
    const cleanup = () => {
      cancelled = true
      observer.disconnect()
      if (stableTimer !== null) window.clearTimeout(stableTimer)
      window.clearTimeout(initialTimer)
    }
    const fireOnce = () => {
      if (cancelled) return
      cleanup()
      fire()
    }
    const observer = new ResizeObserver(() => {
      if (cancelled) return
      if (root.clientWidth === lastWidth) return
      lastWidth = root.clientWidth
      if (stableTimer !== null) window.clearTimeout(stableTimer)
      // Wait ~120ms past the last width change so the sidebar's transition
      // (or any other layout shift) is fully settled before we pan.
      stableTimer = window.setTimeout(fireOnce, 120)
    })
    observer.observe(root)
    // If no resize lands within 50ms the container is already stable — fire
    // immediately so the no-sidebar case stays snappy.
    const initialTimer = window.setTimeout(() => {
      if (cancelled) return
      if (stableTimer !== null) return
      fireOnce()
    }, 50)

    return () => {
      cancelled = true
      observer.disconnect()
      if (stableTimer !== null) window.clearTimeout(stableTimer)
      window.clearTimeout(initialTimer)
    }
  }, [active, reactFlow, selectedAuthoringNodeId])

  // Notify chrome whenever a node selection appears/disappears so it can
  // collapse competing sidebars while the inline panel is up. We pass the
  // boolean (not the id) because the host only cares about presence.
  useEffect(() => {
    if (!onSelectedNodeChange) return
    onSelectedNodeChange(Boolean(selectedAuthoringNodeId))
  }, [onSelectedNodeChange, selectedAuthoringNodeId])

  return (
    <CanvasModeProvider value={canvasModeContextValue}>
      <AgentCanvasExpansionContext.Provider value={expansionValue}>
        <div
          ref={canvasRef}
          className={`agent-visualization${isAgentExiting ? ' is-agent-exiting' : ''}${canvasInteractionsLocked ? ' is-locked' : ''}${editing ? ' is-editing' : ''}${isCreateModeAuthoring && !createModeInitialFitReady ? ' is-initial-fit-pending' : ''}${selectedAuthoringNodeId ? ' is-node-focused' : ''} h-full w-full`}
          aria-label={editing ? 'Agent authoring canvas' : undefined}
        >
          <ReactFlow
            nodes={finalNodes}
            edges={finalEdges}
            onNodesChange={finalOnNodesChange}
            onEdgesChange={editing ? handleEditingEdgesChange : undefined}
            onConnect={editing ? handleEditingConnect : undefined}
            onConnectStart={editing ? onEditingConnectStart : undefined}
            onConnectEnd={editing ? onEditingConnectEnd : undefined}
            isValidConnection={editing ? isValidEditingConnection : undefined}
            nodeTypes={NODE_TYPES}
            edgeTypes={EDGE_TYPES}
            nodesDraggable={finalNodesDraggable}
            nodesConnectable={editing && !canvasInteractionsLocked}
            nodesFocusable
            edgesFocusable={editing}
            edgesReconnectable={editing && !canvasInteractionsLocked}
            elementsSelectable
            selectNodesOnDrag={false}
            elevateNodesOnSelect={false}
            elevateEdgesOnSelect={false}
            onlyRenderVisibleElements={ONLY_RENDER_VISIBLE_ELEMENTS}
            onMoveStart={editing ? undefined : handleMoveStart}
            onMoveEnd={editing ? undefined : handleMoveEnd}
            onNodeDragStart={editing ? undefined : handleNodeDragStart}
            onNodeDragStop={editing ? undefined : handleNodeDragStop}
            defaultViewport={EMPTY_CANVAS_DEFAULT_VIEWPORT}
            fitView={!editing && hasDetail && FIT_VIEW_ON_INIT}
            fitViewOptions={FIT_VIEW_OPTIONS}
            minZoom={0.2}
            maxZoom={2}
            snapToGrid={snapToGrid}
            snapGrid={SNAP_GRID}
            width={canvasBounds?.width}
            height={canvasBounds?.height}
            proOptions={REACT_FLOW_PRO_OPTIONS}
            defaultEdgeOptions={DEFAULT_EDGE_OPTIONS}
            deleteKeyCode={editing ? ['Delete', 'Backspace'] : null}
          >
            <WorkflowCanvasDots />
            {showEmptyState ? (
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
            {showCanvasControls ? (
              <Controls
                position="bottom-right"
                showZoom={false}
                showFitView={false}
                showInteractive={false}
                className="!bg-card !border !border-border !rounded-md !shadow-sm"
              >
                <ControlButton
                  className="react-flow__controls-zoomin"
                  onClick={handleZoomIn}
                  aria-label="Zoom in"
                >
                  <ZoomIn className={CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
                </ControlButton>
                <ControlButton
                  className="react-flow__controls-zoomout"
                  onClick={handleZoomOut}
                  aria-label="Zoom out"
                >
                  <ZoomOut className={CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
                </ControlButton>
                <ControlButton
                  className="react-flow__controls-fitview"
                  onClick={handleFitView}
                  aria-label="Fit view"
                >
                  <Maximize className={CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
                </ControlButton>
                {showLayoutControls ? (
                  <>
                    <ControlButton
                      onClick={handleToggleCanvasLock}
                      aria-label={canvasLocked ? 'Unlock canvas' : 'Lock canvas'}
                      aria-pressed={canvasLocked}
                      style={canvasLocked ? { color: 'var(--primary)' } : undefined}
                    >
                      {canvasLocked ? (
                        <Lock className={CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
                      ) : (
                        <Unlock className={CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
                      )}
                    </ControlButton>
                    <ControlButton
                      onClick={handleToggleSnapToGrid}
                      aria-label={snapToGrid ? 'Disable snap to grid' : 'Enable snap to grid'}
                      aria-pressed={snapToGrid}
                      disabled={canvasInteractionsLocked}
                      style={
                        snapToGrid && !canvasInteractionsLocked
                          ? { color: 'var(--primary)' }
                          : undefined
                      }
                    >
                      <Magnet className={CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
                    </ControlButton>
                    <ControlButton
                      onClick={handleResetLayout}
                      aria-label="Reset layout"
                      disabled={canvasInteractionsLocked}
                    >
                      <RotateCcw className={CANVAS_CONTROL_ICON_CLASS} aria-hidden="true" />
                    </ControlButton>
                  </>
                ) : null}
              </Controls>
            ) : null}
          </ReactFlow>
          {editing && dropPicker ? (
            <DropPicker
              kind={dropPicker.kind}
              screenX={dropPicker.screenX}
              screenY={dropPicker.screenY}
              catalog={authoringCatalog}
              onSelectSkill={addSkillFromDrag}
              onSearchSkills={onSearchAttachableSkills}
              onResolveSkill={onResolveAttachableSkill}
              onSelectToolCategory={addToolCategoryFromDrag}
              onSelectDbTable={addDbTableFromDrag}
              onSelectConsumedArtifact={addConsumedArtifactFromDrag}
              onClose={closeDropPicker}
            />
          ) : null}
          {editing ? (
            <NodePropertiesPanel
              selectedNode={selectedAuthoringNode}
              onClose={clearAuthoringSelection}
            />
          ) : (
            <NodeDetailsPanel
              selectedNode={selectedAuthoringNode}
              onClose={clearAuthoringSelection}
            />
          )}
        </div>
      </AgentCanvasExpansionContext.Provider>
    </CanvasModeProvider>
  )
}


export function AgentVisualization(props: AgentVisualizationProps) {
  return (
    <ReactFlowProvider>
      <AgentVisualizationInner {...props} />
    </ReactFlowProvider>
  )
}

// =====================================================================
// Editing helpers — used by AgentVisualizationInner when `editing` is true.
// The same inner component renders both modes so dots, controls, focus,
// and chrome are guaranteed identical between view and edit.
// =====================================================================

const PROTECTED_NODE_IDS = new Set([AGENT_GRAPH_HEADER_NODE_ID, AGENT_GRAPH_OUTPUT_NODE_ID])

const EDITING_DEFAULT_EDGE_OPTIONS = {
  type: 'smoothstep',
  animated: false,
} as const

interface EditingPositionCounters {
  prompt: number
  skills: number
  tool: number
  'db-table': number
  'output-section': number
  'consumed-artifact': number
}

const EDITING_BLANK_COUNTERS: EditingPositionCounters = {
  prompt: 0,
  skills: 0,
  tool: 0,
  'db-table': 0,
  'output-section': 0,
  'consumed-artifact': 0,
}

/**
 * Approximate the lane positions used by the viewing canvas's category layout
 * — but without running the real layout pass. Newly-added nodes (whether by
 * palette click or by drop) that don't get an explicit cursor position fall
 * here and snap into the right band; the user can drag them anywhere
 * afterwards. Position values pick predictable column x's plus a vertical
 * offset based on how many nodes of the same kind already exist.
 */
function defaultLanePosition(
  kind: AgentGraphNodeKind,
  index: number,
): { x: number; y: number } {
  switch (kind) {
    case 'agent-header':
      return { x: 0, y: 0 }
    case 'agent-output':
      return { x: 0, y: 480 }
    case 'prompt':
      return { x: -380, y: -260 + index * 130 }
    case 'skills':
      return { x: -700, y: -180 + index * 120 }
    case 'tool':
      return { x: 380, y: -80 + index * 70 }
    case 'db-table':
      return { x: -380, y: 220 + index * 150 }
    case 'output-section':
      return { x: 380, y: 480 + index * 80 }
    case 'consumed-artifact':
      return { x: -780, y: index * 130 }
  }
}

/**
 * Collapse the viewing graph's tool-group-frame chrome so the editing canvas
 * works with a flat node list. Frames are a viewing-time visual grouping;
 * authoring tools as standalone nodes (with header → tool edges) keeps the
 * data model tractable for the snapshot serializer.
 */
function flattenForEditing(graph: AgentGraph): AgentGraph {
  const frameIds = new Set(
    graph.nodes.filter((node) => node.type === 'tool-group-frame').map((node) => node.id),
  )
  const toolIds = new Set(
    graph.nodes.filter((node) => node.type === 'tool').map((node) => node.id),
  )

  const counters: EditingPositionCounters = { ...EDITING_BLANK_COUNTERS }
  const nodes: AgentGraphNode[] = []
  for (const node of graph.nodes) {
    if (node.type === 'tool-group-frame') continue
    if (node.type === 'lane-label') continue
    let next: AgentGraphNode = node
    if (node.type === 'tool') {
      const stripped = { ...node }
      delete (stripped as { parentId?: string }).parentId
      delete (stripped as { extent?: 'parent' | unknown }).extent
      delete (stripped as { draggable?: boolean }).draggable
      delete (stripped as { style?: unknown }).style
      next = { ...stripped, draggable: true } as AgentGraphNode
    }
    const kind = next.type as AgentGraphNodeKind | undefined
    let position = next.position
    if (
      (position?.x === 0 && position?.y === 0 && kind && kind !== 'agent-header') ||
      !position
    ) {
      const slot =
        kind === 'agent-output'
          ? 0
          : kind && kind in counters
            ? counters[kind as keyof EditingPositionCounters]++
            : 0
      position = defaultLanePosition(kind ?? 'prompt', slot)
    }
    nodes.push({ ...next, position } as AgentGraphNode)
  }

  const edges: Edge[] = []
  for (const edge of graph.edges) {
    if (frameIds.has(edge.source) || frameIds.has(edge.target)) continue
    edges.push(edge)
  }
  // Re-link header → tool directly so tools that lost their frame anchor still
  // show as part of the agent in the editing graph.
  const seenHeaderToolEdges = new Set(
    edges
      .filter((edge) => edge.source === AGENT_GRAPH_HEADER_NODE_ID && toolIds.has(edge.target))
      .map((edge) => edge.target),
  )
  for (const toolId of toolIds) {
    if (seenHeaderToolEdges.has(toolId)) continue
    edges.push({
      id: `e:header->${toolId}`,
      source: AGENT_GRAPH_HEADER_NODE_ID,
      target: toolId,
      type: 'smoothstep',
      data: { category: 'tool' },
      className: 'agent-edge agent-edge-tool',
    })
  }

  return { nodes, edges }
}

function nextEditingId(kind: AgentGraphNodeKind, counter: number): string {
  return `${kind}:new:${counter}`
}

function makeEditingNode(
  kind: CanvasPaletteKind,
  counter: number,
  position: { x: number; y: number },
): AgentGraphNode | null {
  const id = nextEditingId(kind, counter)
  const base = { id, position }
  switch (kind) {
    case 'prompt':
      return {
        ...base,
        type: 'prompt',
        data: {
          prompt: {
            id: `prompt_${counter}`,
            label: `Prompt ${counter}`,
            role: 'system',
            source: 'custom',
            body: '',
          },
        },
      }
    case 'tool':
      return {
        ...base,
        type: 'tool',
        data: {
          tool: {
            name: `tool_${counter}`,
            group: 'core',
            description: '',
            effectClass: 'observe',
            riskClass: 'standard',
            tags: [],
            schemaFields: [],
            examples: [],
          },
          directConnectionHandles: { source: false, target: false },
        },
      }
    case 'db-table':
      return {
        ...base,
        type: 'db-table',
        data: {
          table: `table_${counter}`,
          touchpoint: 'read',
          purpose: '',
          triggers: [],
          columns: [],
        },
      }
    case 'output-section':
      return {
        ...base,
        type: 'output-section',
        data: {
          section: {
            id: `section_${counter}`,
            label: `Section ${counter}`,
            description: '',
            emphasis: 'standard',
            producedByTools: [],
          },
        },
      }
    case 'consumed-artifact':
      return {
        ...base,
        type: 'consumed-artifact',
        data: {
          artifact: {
            id: `artifact_${counter}`,
            label: `Artifact ${counter}`,
            description: '',
            sourceAgent: 'ask',
            contract: 'answer',
            sections: [],
            required: false,
          },
        },
      }
  }
}


function EditingDiagnosticsPanel({
  diagnostics,
}: {
  diagnostics: readonly AgentDefinitionValidationDiagnosticDto[]
}) {
  return (
    <div
      className="pointer-events-auto absolute bottom-3 left-2.5 right-2.5 z-10 max-h-48 overflow-y-auto rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-[12px] shadow-md backdrop-blur"
      onPointerDown={(event) => event.stopPropagation()}
    >
      <p className="font-semibold text-destructive">Validation issues</p>
      <ul className="mt-1.5 flex flex-col gap-1">
        {diagnostics.map((diagnostic, index) => (
          <li key={`${diagnostic.code}-${index}`} className="text-foreground/80">
            <span className="font-mono text-[11px] text-muted-foreground">{diagnostic.path}</span>{' '}
            — {diagnostic.message}
          </li>
        ))}
      </ul>
    </div>
  )
}

// Re-exports so tests/consumers can compose without re-importing the graph internals.
export type { AgentGraphEdge, AgentGraphNode }
