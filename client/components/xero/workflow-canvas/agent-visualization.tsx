'use client'

import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  ControlButton,
  Controls,
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useOnViewportChange,
  useReactFlow,
  type Edge,
  type Node,
  type NodeChange,
  type NodeTypes,
  type Viewport,
} from '@xyflow/react'
import { RotateCcw } from 'lucide-react'

import '@xyflow/react/dist/style.css'

import {
  agentRefKey,
  type WorkflowAgentDetailDto,
} from '@/src/lib/xero-model/workflow-agents'

import {
  buildAgentGraph,
  type AgentGraphEdge,
  type AgentGraphNode,
} from './build-agent-graph'
import {
  AgentCanvasExpansionContext,
  type AgentCanvasExpansionContextValue,
} from './expansion-context'
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

const NODE_SIZE_BY_TYPE: Record<string, NodeSize> = {
  'agent-header': { width: 300, height: 210 },
  prompt: { width: 300, height: 96 },
  // Tool / output-section heights are intentionally close to the rendered
  // collapsed-card height so layout doesn't pad the column with empty
  // vertical slack between rows. Expansion delta is added separately via
  // EXPANDED_BODY_EXTRA when the user opens a card.
  tool: { width: 240, height: 36 },
  'db-table': { width: 260, height: 104 },
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
  'db-table': 90,
  'output-section': 80,
  'consumed-artifact': 130,
}

const POSITIONS_STORAGE_PREFIX = 'xero.workflows.canvas-positions:'
const INTERACTION_SETTLE_MS = 110
const DOT_GRID_GAP = 32

type StoredPositions = Record<string, { x: number; y: number }>

interface AgentVisualizationProps {
  detail: WorkflowAgentDetailDto
}

interface FocusIndex {
  edgeIdsByNodeId: Map<string, Set<string>>
  edgeById: Map<string, Edge>
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
}

interface FocusElementIndex {
  nodeById: Map<string, HTMLElement>
  edgeById: Map<string, SVGElement>
}

function buildSizeMap(
  nodes: AgentGraphNode[],
  expandedIds: ReadonlySet<string>,
  measuredSizes: ReadonlyMap<string, NodeSize>,
): Map<string, NodeSize> {
  const map = new Map<string, NodeSize>()
  for (const node of nodes) {
    if (!node.type) continue
    const base = NODE_SIZE_BY_TYPE[node.type] ?? { width: 280, height: 120 }
    // Prefer the actual rendered height — that way neighbours displace by
    // exactly how much the card grew rather than by a conservative estimate.
    // Width is always controlled by an inline style on the card so we don't
    // honour a measured width here (it would only echo the design value).
    const measured = measuredSizes.get(node.id)
    if (measured) {
      map.set(node.id, { width: base.width, height: measured.height })
      continue
    }
    const extra = expandedIds.has(node.id) ? EXPANDED_BODY_EXTRA[node.type] ?? 0 : 0
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

function roundCanvasCoord(value: number): number {
  return Math.round(value)
}

function normalizeMeasuredSize(size: NodeSize): NodeSize {
  return {
    width: Math.max(1, Math.round(size.width)),
    height: Math.max(1, Math.round(size.height)),
  }
}

function sameMeasuredSize(left: NodeSize | undefined, right: NodeSize): boolean {
  return Boolean(left && left.width === right.width && left.height === right.height)
}

function isPersistableNode(node: AgentGraphNode | Node): boolean {
  return node.type !== 'lane-label'
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

function buildFocusIndex(
  nodes: readonly Node[],
  edges: readonly Edge[],
): FocusIndex {
  const edgeIdsByNodeId = new Map<string, Set<string>>()
  const edgeById = new Map<string, Edge>()
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
    const parent = (node as Node).parentId
    if (!parent) continue
    parentIdByNodeId.set(node.id, parent)
    const set = childIdsByParent.get(parent) ?? new Set<string>()
    set.add(node.id)
    childIdsByParent.set(parent, set)
  }

  return { edgeIdsByNodeId, edgeById, parentIdByNodeId, childIdsByParent }
}

function buildFocusTargets(
  nodes: readonly Node[],
  edges: readonly Edge[],
): Map<string, FocusTarget> {
  const index = buildFocusIndex(nodes, edges)
  const targets = new Map<string, FocusTarget>()

  for (const node of nodes) {
    if (node.type === 'lane-label') continue

    const focusedNodes = new Set<string>()
    const focusedEdges = new Set<string>()

    const visit = (id: string) => {
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
      const children = index.childIdsByParent.get(id)
      if (children) {
        for (const childId of children) focusedNodes.add(childId)
      }
    }

    visit(node.id)

    let cursor: string | undefined = index.parentIdByNodeId.get(node.id)
    const seenAncestors = new Set<string>()
    while (cursor && !seenAncestors.has(cursor)) {
      seenAncestors.add(cursor)
      visit(cursor)
      cursor = index.parentIdByNodeId.get(cursor)
    }

    targets.set(node.id, {
      nodeIds: Array.from(focusedNodes),
      edgeIds: Array.from(focusedEdges),
    })
  }

  return targets
}

function applyDotViewport(element: HTMLElement | null, viewport: Viewport): void {
  if (!element) return
  const zoom = Number.isFinite(viewport.zoom) && viewport.zoom > 0 ? viewport.zoom : 1
  const scaledGap = Math.max(1, DOT_GRID_GAP * zoom)
  element.style.setProperty('--agent-dot-scale', `${zoom}`)
  element.style.setProperty('--agent-dot-x', `${viewport.x % scaledGap}px`)
  element.style.setProperty('--agent-dot-y', `${viewport.y % scaledGap}px`)
  element.style.width = `calc(${100 / zoom}% + ${DOT_GRID_GAP * 2}px)`
  element.style.height = `calc(${100 / zoom}% + ${DOT_GRID_GAP * 2}px)`
}

function WorkflowCanvasDots() {
  const ref = useRef<HTMLDivElement | null>(null)
  const reactFlow = useReactFlow()
  const updateDots = useCallback((viewport: Viewport) => {
    applyDotViewport(ref.current, viewport)
  }, [])

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      updateDots(reactFlow.getViewport())
    })
    return () => window.cancelAnimationFrame(frame)
  }, [reactFlow, updateDots])

  useOnViewportChange({
    onStart: updateDots,
    onChange: updateDots,
    onEnd: updateDots,
  })

  return <div ref={ref} className="agent-visualization__dots" aria-hidden="true" />
}

function AgentVisualizationInner({ detail }: AgentVisualizationProps) {
  const storageKey = useMemo(() => storageKeyFor(detail), [detail])

  const baseGraph = useMemo(() => buildAgentGraph(detail), [detail])
  const canvasRef = useRef<HTMLDivElement | null>(null)
  const reactFlow = useReactFlow()
  // Bumped each time the user invokes "Reset layout" so the memoized layout
  // computation re-runs even when storage was already empty.
  const [resetNonce, setResetNonce] = useState(0)
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

  const scheduleMeasuredSize = useCallback(
    (nodeId: string, size: NodeSize) => {
      const normalized = normalizeMeasuredSize(size)
      const pending = pendingMeasuredSizesRef.current
      const existing = pending.get(nodeId) ?? measuredSizesRef.current.get(nodeId)
      if (sameMeasuredSize(existing, normalized)) return

      pending.set(nodeId, normalized)
      if (measurementFrameRef.current !== null) return

      measurementFrameRef.current = window.requestAnimationFrame(() => {
        measurementFrameRef.current = null
        const updates = pendingMeasuredSizesRef.current
        pendingMeasuredSizesRef.current = new Map()
        if (updates.size === 0) return

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
      })
    },
    [commitMeasuredSizes],
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
      invalidateMeasuredSize(nodeId)
      setExpandedIds(next)
    },
    [invalidateMeasuredSize],
  )

  const expansionValue = useMemo<AgentCanvasExpansionContextValue>(
    () => ({
      setExpanded: setNodeExpanded,
    }),
    [setNodeExpanded],
  )

  const computedNodes = useMemo(() => {
    // resetNonce participates so "Reset layout" forces a recompute after the
    // localStorage entry is wiped.
    void resetNonce
    const sizes = buildSizeMap(baseGraph.nodes, expandedIds, measuredSizes)
    const placed = layoutAgentGraphByCategory(baseGraph.nodes, sizes, {
      stableHeaderHeight: NODE_SIZE_BY_TYPE['agent-header'].height,
    })
    const stored = getStoredPositions()
    return applyStoredPositions(placed, stored) as Node[]
  }, [
    baseGraph,
    expandedIds,
    measuredSizes,
    getStoredPositions,
    resetNonce,
    storedPositionsNonce,
  ])

  const computedEdges = useMemo(() => baseGraph.edges as Edge[], [baseGraph])
  const canvasInteractingRef = useRef(false)

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>(computedNodes)
  const focusTargets = useMemo(
    () => buildFocusTargets(computedNodes, computedEdges),
    [computedNodes, computedEdges],
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
      return changed ? nextNodes : current
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
  const appliedFocusRef = useRef<AppliedFocus | null>(null)
  const focusElementIndexRef = useRef<FocusElementIndex | null>(null)
  const interactionSettleTimerRef = useRef<number | null>(null)

  const resolveNodeIdFromElement = useCallback((element: Element | null): string | null => {
    const node = element?.closest?.('.react-flow__node')
    if (!node) return null
    if (node.classList.contains('react-flow__node-lane-label')) return null
    if (node.classList.contains('react-flow__node-tool-group-frame')) return null
    return node.getAttribute('data-id')
  }, [])

  const resolveNodeIdAtPoint = useCallback((x: number, y: number): string | null => {
    let el: Element | null = document.elementFromPoint(x, y)
    while (el) {
      if (el.classList.contains('react-flow__node')) {
        if (el.classList.contains('react-flow__node-lane-label')) return null
        if (el.classList.contains('react-flow__node-tool-group-frame')) return null
        return el.getAttribute('data-id')
      }
      el = el.parentElement
    }
    return null
  }, [])

  const getFocusElementIndex = useCallback((): FocusElementIndex | null => {
    const cached = focusElementIndexRef.current
    if (cached) return cached

    const root = canvasRef.current
    if (!root) return null

    const nodeById = new Map<string, HTMLElement>()
    const edgeById = new Map<string, SVGElement>()
    root.querySelectorAll<HTMLElement>('.react-flow__node[data-id]').forEach((node) => {
      const id = node.getAttribute('data-id')
      if (id) nodeById.set(id, node)
    })
    root.querySelectorAll<SVGElement>('.react-flow__edge[data-id]').forEach((edge) => {
      const id = edge.getAttribute('data-id')
      if (id) edgeById.set(id, edge)
    })

    const index = { nodeById, edgeById }
    focusElementIndexRef.current = index
    return index
  }, [])

  const invalidateFocusElementIndex = useCallback(() => {
    focusElementIndexRef.current = null
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
      const elements = getFocusElementIndex()
      if (!elements) return
      const target = focusTargets.get(nodeId)
      if (!target) return

      root.classList.add('is-focusing')
      const nodeElements: HTMLElement[] = []
      const edgeElements: SVGElement[] = []
      for (const focusedNodeId of target.nodeIds) {
        const nodeElement = elements.nodeById.get(focusedNodeId)
        if (!nodeElement) continue
        nodeElement.classList.add('is-focused')
        nodeElements.push(nodeElement)
      }
      for (const focusedEdgeId of target.edgeIds) {
        const edgeElement = elements.edgeById.get(focusedEdgeId)
        if (!edgeElement) continue
        edgeElement.classList.add('is-active')
        edgeElements.push(edgeElement)
      }
      appliedFocusRef.current = { nodeElements, edgeElements }
    },
    [focusTargets, getFocusElementIndex, removeAppliedFocus],
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
      root.classList.add('is-interacting')
    }
  }, [])

  const settleCanvasInteraction = useCallback(() => {
    if (interactionSettleTimerRef.current !== null) {
      window.clearTimeout(interactionSettleTimerRef.current)
    }
    interactionSettleTimerRef.current = window.setTimeout(() => {
      interactionSettleTimerRef.current = null
      canvasInteractingRef.current = false
      canvasRef.current?.classList.remove('is-interacting')
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

  const handleNodeDragStart = useCallback(() => {
    clearFocus()
    markCanvasInteracting()
    canvasRef.current?.classList.add('is-dragging')
  }, [clearFocus, markCanvasInteracting])

  const handleNodeDragStop = useCallback(() => {
    canvasRef.current?.classList.remove('is-dragging')
    settleCanvasInteraction()
  }, [settleCanvasInteraction])

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
      lastPointerRef.current = {
        x: event.clientX,
        y: event.clientY,
        nodeId: resolveNodeIdFromElement(target),
      }
      if (pendingFrameRef.current !== null) return
      pendingFrameRef.current = window.requestAnimationFrame(() => {
        pendingFrameRef.current = null
        const point = lastPointerRef.current
        if (!point) return
        applyFocusForNode(point.nodeId ?? resolveNodeIdAtPoint(point.x, point.y))
      })
    },
    [applyFocusForNode, clearFocus, resolveNodeIdAtPoint, resolveNodeIdFromElement],
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
      if (interactionSettleTimerRef.current !== null) {
        window.clearTimeout(interactionSettleTimerRef.current)
        interactionSettleTimerRef.current = null
      }
      removeAppliedFocus()
    },
    [removeAppliedFocus],
  )

  useEffect(() => {
    invalidateFocusElementIndex()
    clearFocus()
  }, [clearFocus, computedEdges, computedNodes, invalidateFocusElementIndex])

  // Persist positions when the user finishes a drag. Avoids hammering
  // localStorage on every intermediate position event during the gesture.
  // ALSO captures dimension measurements from React Flow so the layout can
  // displace neighbours based on actual rendered heights instead of fixed
  // worst-case estimates.
  const handleNodesChange = useCallback(
    (changes: NodeChange<Node>[]) => {
      const flowChanges = changes.filter((change) => change.type !== 'dimensions')
      if (flowChanges.length > 0) {
        onNodesChange(flowChanges)
      }

      // ResizeObserver can emit several dimension changes during one expand
      // animation. Coalesce those into one layout update per frame and round to
      // whole pixels so tiny font-rasterization deltas do not jitter neighbours.
      for (const change of changes) {
        if (change.type !== 'dimensions' || !change.dimensions) continue
        scheduleMeasuredSize(change.id, change.dimensions)
      }

      // Persist only the nodes the user *actually* dragged in this gesture.
      // The previous version snapshotted every node's position on every
      // drag-end, which made `applyStoredPositions` override the layout for
      // every node — so once you nudged anything, expansion no longer
      // re-flowed any neighbours.
      let nextPositions: StoredPositions | null = null
      for (const change of changes) {
        if (
          change.type !== 'position' ||
          change.dragging !== false ||
          !change.position ||
          !persistableNodeIds.has(change.id)
        ) {
          continue
        }
        if (!nextPositions) nextPositions = { ...getStoredPositions() }
        nextPositions[change.id] = {
          x: roundCanvasCoord(change.position.x),
          y: roundCanvasCoord(change.position.y),
        }
      }
      if (nextPositions) commitStoredPositions(nextPositions)
    },
    [
      commitStoredPositions,
      getStoredPositions,
      onNodesChange,
      persistableNodeIds,
      scheduleMeasuredSize,
    ],
  )

  // Reset expansion state when the agent changes — a different agent has a
  // different node id space, and stale entries would leak into its sizing.
  const lastDetailRef = useRef(detail)
  useEffect(() => {
    if (lastDetailRef.current !== detail) {
      lastDetailRef.current = detail
      const emptyExpanded = new Set<string>()
      const emptySizes = new Map<string, NodeSize>()
      if (measurementFrameRef.current !== null) {
        window.cancelAnimationFrame(measurementFrameRef.current)
        measurementFrameRef.current = null
      }
      expandedIdsRef.current = emptyExpanded
      measuredSizesRef.current = emptySizes
      pendingMeasuredSizesRef.current = new Map()
      setExpandedIds(emptyExpanded)
      setMeasuredSizes(emptySizes)
      canvasInteractingRef.current = false
      canvasRef.current?.classList.remove('is-dragging')
      canvasRef.current?.classList.remove('is-interacting')
    }
  }, [detail])

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
    canvasRef.current?.classList.remove('is-interacting')
    setResetNonce((n) => n + 1)
    // Defer fitView until the layout update has committed, then animate the
    // viewport in the same window as the node/body transitions.
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        reactFlow.fitView({
          padding: 0.16,
          includeHiddenNodes: false,
          duration: 260,
        })
      })
    })
  }, [reactFlow, storageKey])

  return (
    <AgentCanvasExpansionContext.Provider value={expansionValue}>
      <div
        ref={canvasRef}
        className="agent-visualization h-full w-full"
      >
        <ReactFlow
          nodes={nodes}
          edges={computedEdges}
          onNodesChange={handleNodesChange}
          nodeTypes={NODE_TYPES}
          nodesDraggable
          nodesConnectable={false}
          nodesFocusable={false}
          edgesFocusable={false}
          edgesReconnectable={false}
          elementsSelectable={false}
          elevateNodesOnSelect={false}
          elevateEdgesOnSelect={false}
          onlyRenderVisibleElements
          onMoveStart={handleMoveStart}
          onMoveEnd={handleMoveEnd}
          onNodeDragStart={handleNodeDragStart}
          onNodeDragStop={handleNodeDragStop}
          fitView
          fitViewOptions={{ padding: 0.16, includeHiddenNodes: false }}
          minZoom={0.2}
          maxZoom={2}
          proOptions={{ hideAttribution: true }}
          defaultEdgeOptions={{
            type: 'smoothstep',
            animated: false,
          }}
        >
          <WorkflowCanvasDots />
          <Controls
            position="bottom-right"
            showInteractive={false}
            className="!bg-card !border !border-border !rounded-md !shadow-sm"
          >
            <ControlButton onClick={handleResetLayout} title="Reset layout">
              <RotateCcw />
            </ControlButton>
          </Controls>
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
