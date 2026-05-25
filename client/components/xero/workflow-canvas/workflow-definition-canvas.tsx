'use client'

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react'
import {
  applyNodeChanges,
  ConnectionMode,
  Handle,
  MarkerType,
  Position,
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useReactFlow,
  useUpdateNodeInternals,
  type Connection,
  type Edge,
  type EdgeTypes,
  type Node,
  type NodeChange,
  type NodeProps,
} from '@xyflow/react'
import {
  Bot,
  CheckCircle2,
  ClipboardCheck,
  Database,
  AlertCircle,
  FileText,
  Flag,
  GitBranch,
  GitMerge,
  ListChecks,
  Loader2,
  Lock,
  PauseCircle,
  Play,
  Plus,
  RotateCcw,
  Route,
  ShieldCheck,
  SkipForward,
  SquareTerminal,
  Trash2,
  Workflow,
  X,
  type LucideIcon,
} from 'lucide-react'

import '@xyflow/react/dist/style.css'
import './agent-visualization.css'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/lib/utils'
import {
  validateWorkflowDefinition,
  type WorkflowConditionDto,
  type WorkflowDefinitionDto,
  type WorkflowEdgeDto,
  type WorkflowEdgeTypeDto,
  type WorkflowInputBindingDto,
  type WorkflowNodeDto,
  type WorkflowNodeRunStatusDto,
  type WorkflowOutputExtractionDto,
  type WorkflowRunStatusDto,
  type WorkflowStateQueryFilterDto,
  type WorkflowTerminalStatusDto,
} from '@/src/lib/xero-model/workflow-definition'
import {
  WORKFLOW_TEMPLATE_DEFAULT_NODE_POSITIONS,
  type WorkflowTemplateNodePositionsDto,
} from '@/src/lib/xero-model/workflow-templates'
import type {
  WorkflowArtifactRecordDto,
  WorkflowEventDto,
  WorkflowRunBlockerResponseDto,
  WorkflowRunBundleResponseDto,
  WorkflowRunDto,
  WorkflowRunEdgeDecisionDto,
  WorkflowRunNodeDto,
} from '@/src/lib/xero-model/workflow-run'
import {
  agentRefKey,
  type AgentRefDto,
  type WorkflowAgentSummaryDto,
} from '@/src/lib/xero-model/workflow-agents'
import {
  AgentCanvasControls,
  AgentCanvasDots,
  AGENT_CANVAS_EMPTY_VIEWPORT,
  AGENT_CANVAS_SNAP_GRID,
  type AgentCanvasControlItem,
} from './canvas-shell'
import { PhaseBranchEdge } from './edges/phase-branch-edge'
import { CanvasNodeCard } from './nodes/canvas-node-card'
import {
  WORKFLOW_EDGE_HELP,
  WORKFLOW_HANDOFF_PRESETS,
  WORKFLOW_NODE_HELP,
  handoffLabel,
  workflowConditionPlainSummary,
  workflowEdgePlainSummary,
  workflowInputBindingPlainSummary,
  workflowNodePlainSummary,
  workflowStateFilterPlainSummary,
  workflowStateQueryPlainSummary,
  workflowStateWritePlainSummary,
} from './workflow-authoring-help'

type WorkflowCanvasMode = 'view' | 'edit'
type Selection =
  | { kind: 'node'; id: string }
  | { kind: 'edge'; id: string }
  | null

const WORKFLOW_PANEL_TEXT_WRAP_CLASS = 'break-words [overflow-wrap:anywhere]'

export interface WorkflowDefinitionCanvasStatus {
  editing: boolean
  saving: boolean
  runningAction: boolean
  saveDisabled: boolean
  diagnosticCount: number
  diagnostics: ReadonlyArray<{ message: string; path: string }>
  errorMessage: string | null
  definition: WorkflowDefinitionDto
  run: WorkflowRunDto | null
  updateName: (value: string) => void
  updateDescription: (value: string) => void
  edit: () => void
  save: () => void
  cancel: () => void
  start: () => void
  cancelRun: (() => void) | null
  retryNodeRun: ((nodeRunId: string) => void) | null
  skipBranch: ((nodeRunId: string) => void) | null
}

interface WorkflowDefinitionCanvasProps {
  active?: boolean
  definition: WorkflowDefinitionDto
  run?: WorkflowRunDto | null
  agents?: readonly WorkflowAgentSummaryDto[]
  initialMode?: WorkflowCanvasMode
  isCreating?: boolean
  saving?: boolean
  runningAction?: boolean
  onSaveDefinition?: (definition: WorkflowDefinitionDto) => Promise<WorkflowDefinitionDto | void>
  onCancelEditing?: () => void
  onCanvasStatusChange?: (status: WorkflowDefinitionCanvasStatus | null) => void
  onStartRun?: (workflowId: string, initialInput: unknown) => Promise<WorkflowRunDto | void>
  onCancelRun?: (runId: string) => Promise<WorkflowRunDto | void>
  onRetryNodeRun?: (runId: string, nodeRunId: string) => Promise<WorkflowRunDto | void>
  onSkipBranch?: (
    runId: string,
    nodeRunId: string,
    reason?: string,
  ) => Promise<WorkflowRunDto | void>
  onResumeCheckpoint?: (
    runId: string,
    nodeRunId: string,
    decision: string,
    payload: unknown,
  ) => Promise<WorkflowRunDto | void>
  onExplainRunBlocker?: (runId: string) => Promise<WorkflowRunBlockerResponseDto | void>
  onExportRunBundle?: (runId: string) => Promise<WorkflowRunBundleResponseDto | void>
  onResumeNextIncompletePhase?: (runId: string) => Promise<WorkflowRunDto | void>
  onCreateAgent?: () => void
  onEditAgent?: (ref: AgentRefDto) => void
}

interface WorkflowNodeData extends Record<string, unknown> {
  node: WorkflowNodeDto
  runNode: WorkflowRunNodeDto | null
  artifact: WorkflowArtifactRecordDto | null
  agentLabel: string | null
  isStart: boolean
  incomingCount: number
  outgoingCount: number
  handles: WorkflowNodeHandle[]
}

type WorkflowReactNode = Node<WorkflowNodeData, 'workflowNode'>
type WorkflowReactEdge = Edge<{
  workflowEdge: WorkflowEdgeDto
  label: string
  targetClearance: number
  edgeColor: string
  labelBackground: string
  labelBorderColor: string
  labelTextColor: string
  focused?: boolean
  visualOnly?: boolean
}>
type WorkflowEdgeTone = {
  color: string
  labelBackground: string
  labelBorderColor: string
  labelTextColor: string
}
type WorkflowRunInputField = {
  key: string
  label: string
  required: boolean
}
type WorkflowRunPreview = {
  stateReads: string[]
  stateWrites: string[]
  loops: string[]
  commands: string[]
  checkpoints: string[]
  terminals: string[]
}
type WorkflowRecoveryResult = {
  kind: 'blocker' | 'bundle' | 'resume'
  title: string
  summary: string
  detail: string
}
type WorkflowLayoutLane = 'upper' | 'main' | 'recovery' | 'terminal'
type WorkflowLayoutEdge = {
  id: string
  fromNodeId: string
  toNodeId: string
  edge: WorkflowEdgeDto
  kind: 'direct' | 'exhausted'
}
type WorkflowHandleSide = 'top' | 'right' | 'bottom' | 'left'
type WorkflowNodePosition = { x: number; y: number }
type WorkflowNodeHandle = {
  id: string
  side: WorkflowHandleSide
  edgeType: WorkflowEdgeTypeDto
  className: string
  style?: CSSProperties
}

const NODE_TYPES = {
  workflowNode: WorkflowNodeCard,
}
const EDGE_TYPES = {
  'workflow-branch': PhaseBranchEdge,
} as unknown as EdgeTypes

const FIT_VIEW_OPTIONS = { padding: 0.14, includeHiddenNodes: false, maxZoom: 1 } as const
const WORKFLOW_LAYOUT_CARD_WIDTH = 240
const WORKFLOW_LAYOUT_CARD_HEIGHT = 112
const WORKFLOW_LAYOUT_COLUMN_GAP = 400
const WORKFLOW_LAYOUT_ORIGIN_X = 80
const WORKFLOW_LAYOUT_LANE_GAP = 184
const WORKFLOW_LAYOUT_LANE_Y: Record<WorkflowLayoutLane, number> = {
  upper: 56,
  main: 284,
  recovery: 512,
  terminal: 284,
}
const WORKFLOW_HANDLE_SIDES: readonly WorkflowHandleSide[] = ['top', 'right', 'bottom', 'left']
const WORKFLOW_HANDLE_POSITION: Record<WorkflowHandleSide, Position> = {
  top: Position.Top,
  right: Position.Right,
  bottom: Position.Bottom,
  left: Position.Left,
}
const WORKFLOW_EDGE_TYPE_ORDER: readonly WorkflowEdgeTypeDto[] = [
  'success',
  'conditional',
  'loop',
  'recovery',
  'failure',
  'manual_override',
]
const WORKFLOW_EDGE_MARKER = {
  type: MarkerType.ArrowClosed,
  width: 18,
  height: 18,
  markerUnits: 'strokeWidth',
  strokeWidth: 1.8,
} as const
const EMPTY_WORKFLOW_AGENTS: readonly WorkflowAgentSummaryDto[] = []
const WORKFLOW_HANDLE_VISUAL_SIZE = 18
const WORKFLOW_ARROW_TARGET_GAP = 4
const WORKFLOW_MARKER_VIEWBOX_SIZE = 20
const WORKFLOW_EDGE_TARGET_CLEARANCE = workflowArrowTargetClearance(WORKFLOW_EDGE_MARKER)
const NODE_KIND_LABEL: Record<WorkflowNodeDto['type'], string> = {
  agent: 'Agent',
  router: 'Router',
  gate: 'Gate',
  human_checkpoint: 'Checkpoint',
  merge: 'Merge',
  terminal: 'Terminal',
  state_read: 'State read',
  state_write: 'State write',
  state_patch: 'State patch',
  state_query: 'State query',
  state_checkpoint: 'State checkpoint',
  collection_loop: 'Collection loop',
  subgraph: 'Subgraph',
  command: 'Command',
}
const EDGE_TYPE_LABEL: Record<WorkflowEdgeTypeDto, string> = {
  success: 'Success',
  failure: 'Failure',
  conditional: 'If',
  loop: 'Loop',
  recovery: 'Recovery',
  manual_override: 'Manual',
}
const NODE_STATUS_TONE: Record<WorkflowNodeRunStatusDto, string> = {
  pending: 'border-muted-foreground/20 bg-muted/45 text-muted-foreground',
  eligible: 'border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300',
  starting: 'border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300',
  running: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
  waiting_on_gate: 'border-amber-500/35 bg-amber-500/10 text-amber-700 dark:text-amber-300',
  succeeded: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
  failed: 'border-destructive/35 bg-destructive/10 text-destructive',
  stalled: 'border-orange-500/35 bg-orange-500/10 text-orange-700 dark:text-orange-300',
  skipped: 'border-muted-foreground/20 bg-muted/45 text-muted-foreground',
  cancelled: 'border-muted-foreground/20 bg-muted/45 text-muted-foreground',
}
const WORKFLOW_CONTROL_ENTRIES: {
  type: WorkflowNodeDto['type']
  label: string
  icon: LucideIcon
}[] = [
  { type: 'agent', label: 'Add agent', icon: Bot },
  { type: 'router', label: 'Add router', icon: Route },
  { type: 'gate', label: 'Add gate', icon: ShieldCheck },
  { type: 'human_checkpoint', label: 'Add checkpoint', icon: PauseCircle },
  { type: 'merge', label: 'Add merge', icon: GitMerge },
  { type: 'state_query', label: 'Add state query', icon: Database },
  { type: 'state_write', label: 'Add state write', icon: ClipboardCheck },
  { type: 'collection_loop', label: 'Add collection loop', icon: ListChecks },
  { type: 'subgraph', label: 'Add subgraph', icon: Workflow },
  { type: 'command', label: 'Add command', icon: SquareTerminal },
  { type: 'terminal', label: 'Add terminal', icon: CheckCircle2 },
]
const DELIVERY_ENTITY_TYPES = [
  'delivery_project',
  'milestone',
  'requirement',
  'delivery_phase',
  'phase_context',
  'phase_plan',
  'phase_summary',
  'verification_evidence',
  'deferred_item',
  'milestone_archive',
] as const
const STATE_WRITE_ACTIONS = ['create', 'upsert', 'update', 'patch', 'mark_complete', 'archive'] as const

export function WorkflowDefinitionCanvas(props: WorkflowDefinitionCanvasProps) {
  return (
    <ReactFlowProvider>
      <WorkflowDefinitionCanvasInner {...props} />
    </ReactFlowProvider>
  )
}

function WorkflowDefinitionCanvasInner({
  active = true,
  definition,
  run = null,
  agents = EMPTY_WORKFLOW_AGENTS,
  initialMode = 'view',
  isCreating = false,
  saving = false,
  runningAction = false,
  onSaveDefinition,
  onCancelEditing,
  onCanvasStatusChange,
  onStartRun,
  onCancelRun,
  onRetryNodeRun,
  onSkipBranch,
  onResumeCheckpoint,
  onExplainRunBlocker,
  onExportRunBundle,
  onResumeNextIncompletePhase,
  onCreateAgent,
  onEditAgent,
}: WorkflowDefinitionCanvasProps) {
  const reactFlow = useReactFlow()
  const updateNodeInternals = useUpdateNodeInternals()
  const [mode, setMode] = useState<WorkflowCanvasMode>(initialMode)
  const [draft, setDraft] = useState<WorkflowDefinitionDto>(() => cloneDefinition(definition))
  const [selection, setSelection] = useState<Selection>(null)
  const [startDialogOpen, setStartDialogOpen] = useState(false)
  const [startInputs, setStartInputs] = useState<Record<string, string>>({})
  const [recoveryResult, setRecoveryResult] = useState<WorkflowRecoveryResult | null>(null)
  const [recoveryAction, setRecoveryAction] = useState<'blocker' | 'bundle' | 'resume' | null>(null)
  const [localError, setLocalError] = useState<string | null>(null)
  const [canvasLocked, setCanvasLocked] = useState(false)
  const [snapToGrid, setSnapToGrid] = useState(true)
  const [entering, setEntering] = useState(true)
  const [dragging, setDragging] = useState(false)
  const editFitFrameRef = useRef<number | null>(null)
  const nodeInternalsRefreshKeyRef = useRef('')
  const nodeInternalsSecondPassFrameRef = useRef<number | null>(null)
  const editable = mode === 'edit'
  const canvasInteractionsLocked = editable && canvasLocked

  useEffect(() => {
    setMode(initialMode)
    setDraft(autoLayoutWorkflowDefinition(cloneDefinition(definition)))
    setSelection(null)
    setLocalError(null)
  }, [definition.id, definition.version, definition.updatedAt, initialMode])

  useEffect(
    () => () => {
      if (editFitFrameRef.current !== null) {
        window.cancelAnimationFrame(editFitFrameRef.current)
      }
      if (nodeInternalsSecondPassFrameRef.current !== null) {
        window.cancelAnimationFrame(nodeInternalsSecondPassFrameRef.current)
      }
    },
    [],
  )

  useEffect(() => {
    setEntering(true)
    let secondFrame: number | null = null
    const firstFrame = window.requestAnimationFrame(() => {
      secondFrame = window.requestAnimationFrame(() => setEntering(false))
    })
    return () => {
      window.cancelAnimationFrame(firstFrame)
      if (secondFrame !== null) window.cancelAnimationFrame(secondFrame)
    }
  }, [definition.id, isCreating])

  const resetDefinition = useMemo(() => autoLayoutWorkflowDefinition(definition), [definition])
  const effectiveDefinition = editable ? draft : resetDefinition
  const validation = useMemo(
    () => validateWorkflowDefinition(effectiveDefinition),
    [effectiveDefinition],
  )
  const startInputFields = useMemo(() => workflowRunInputFields(definition), [definition])
  const startPreview = useMemo(() => buildWorkflowRunPreview(definition), [definition])
  const startInputValid = useMemo(
    () =>
      startInputFields.every((field) => {
        if (!field.required) return true
        return (startInputs[field.key] ?? '').trim().length > 0
      }),
    [startInputFields, startInputs],
  )
  const selectedNode = selection?.kind === 'node'
    ? effectiveDefinition.nodes.find((node) => node.id === selection.id) ?? null
    : null
  const selectedNodeId = selectedNode?.id ?? null
  const selectedEdge = selection?.kind === 'edge'
    ? effectiveDefinition.edges.find((edge) => edge.id === selection.id) ?? null
    : null
  const latestRunNodeByNodeId = useMemo(() => latestRunNodesByNodeId(run), [run])
  const latestArtifactByNodeId = useMemo(() => latestArtifactsByNodeId(run), [run])
  const latestArtifactByNodeRunId = useMemo(() => latestArtifactsByNodeRunId(run), [run])
  const selectedSubgraphChildRuns = useMemo(() => {
    if (!run || selectedNode?.type !== 'subgraph') return []
    const prefix = `${selectedNode.id}::`
    return run.nodes
      .filter((node) => node.nodeId.startsWith(prefix))
      .sort((left, right) => {
        const byNode = left.nodeId.localeCompare(right.nodeId)
        return byNode === 0 ? left.attemptNumber - right.attemptNumber : byNode
      })
  }, [run, selectedNode])
  const matchedEdgeIds = useMemo(
    () => new Set((run?.edgeDecisions ?? []).map((decision) => decision.edgeId)),
    [run?.edgeDecisions],
  )
  const agentOptions = useMemo(
    () => agents.map((agent) => ({ agent, key: agentRefKey(agent.ref) })),
    [agents],
  )
  const computedNodes = useMemo<WorkflowReactNode[]>(
    () => {
      const handlesByNodeId = buildWorkflowNodeHandles(effectiveDefinition, editable)
      return effectiveDefinition.nodes.map((node) => {
        const incomingCount = workflowIncomingEdgeCount(node.id, effectiveDefinition.edges)
        const outgoingCount = workflowOutgoingEdgeCount(node.id, effectiveDefinition.edges)
        return {
          id: node.id,
          type: 'workflowNode',
          position: node.position,
          data: {
            node,
            runNode: latestRunNodeByNodeId.get(node.id) ?? null,
            artifact: latestArtifactByNodeId.get(node.id) ?? null,
            agentLabel: node.type === 'agent' ? labelForAgentRef(node.agentRef, agents) : null,
            isStart: node.id === effectiveDefinition.startNodeId,
            incomingCount,
            outgoingCount,
            handles: handlesByNodeId.get(node.id) ?? [],
          },
          draggable: editable && !canvasInteractionsLocked,
          selectable: true,
          selected: selection?.kind === 'node' && selection.id === node.id,
        }
      })
    },
    [
      agents,
      canvasInteractionsLocked,
      editable,
      effectiveDefinition,
      latestArtifactByNodeId,
      latestRunNodeByNodeId,
      selection,
    ],
  )
  const [nodes, setNodes] = useNodesState<WorkflowReactNode>(computedNodes)
  const nodesRef = useRef<WorkflowReactNode[]>(computedNodes)
  useEffect(() => {
    nodesRef.current = nodes
  }, [nodes])

  useEffect(() => {
    const current = nodesRef.current
    const byId = new Map(current.map((node) => [node.id, node] as const))
    let changed = current.length !== computedNodes.length
    const nextNodes = computedNodes.map((next) => {
      const previous = byId.get(next.id)
      if (!previous) {
        changed = true
        return next
      }
      if (
        previous.position.x === next.position.x &&
        previous.position.y === next.position.y &&
        previous.type === next.type &&
        previous.draggable === next.draggable &&
        previous.selectable === next.selectable &&
        previous.selected === next.selected &&
        previous.data === next.data
      ) {
        return previous
      }
      changed = true
      return { ...previous, ...next, position: next.position }
    })
    if (!changed) return
    nodesRef.current = nextNodes
    setNodes(nextNodes)
  }, [computedNodes, setNodes])

  const renderedDefinition = useMemo(
    () => workflowDefinitionWithReactNodePositions(effectiveDefinition, nodes),
    [effectiveDefinition, nodes],
  )
  const renderedNodes = useMemo(
    () => workflowNodesWithRenderedHandles(nodes, renderedDefinition, editable),
    [editable, nodes, renderedDefinition],
  )
  const refreshWorkflowNodeInternals = useCallback(
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

  useEffect(() => {
    const refreshKey = workflowNodeInternalsRefreshKey(renderedNodes)
    if (refreshKey === nodeInternalsRefreshKeyRef.current) return
    nodeInternalsRefreshKeyRef.current = refreshKey
    refreshWorkflowNodeInternals(renderedNodes.map((node) => node.id))
  }, [refreshWorkflowNodeInternals, renderedNodes])

  const edges = useMemo<WorkflowReactEdge[]>(
    () => {
      const nodeById = new Map(renderedDefinition.nodes.map((node) => [node.id, node]))
      return renderedDefinition.edges.flatMap((edge) => {
        const reactEdges = [
          workflowReactEdgeFromDefinitionEdge(edge, nodeById, {
            editing: editable,
            selectedNodeId,
            matched: matchedEdgeIds.has(edge.id),
            running: run?.status === 'running',
          }),
        ]
        const exhaustionTargetId = workflowLoopExhaustionTarget(edge)
        if (exhaustionTargetId && nodeById.has(exhaustionTargetId)) {
          const visualEdge = workflowLoopExhaustionVisualEdge(edge, exhaustionTargetId)
          reactEdges.push(
            workflowReactEdgeFromDefinitionEdge(
              visualEdge,
              nodeById,
              {
                editing: editable,
                selectedNodeId,
                visualOnly: true,
                className: 'workflow-definition-edge--exhausted workflow-definition-edge--loop',
              },
            ),
          )
        }
        return reactEdges
      })
    },
    [editable, matchedEdgeIds, renderedDefinition.edges, renderedDefinition.nodes, run?.status, selectedNodeId],
  )

  const canSave = editable && validation.status === 'valid' && Boolean(onSaveDefinition)
  const waitingCheckpoint = useMemo(() => {
    if (!run || run.status !== 'paused') return null
    return run.nodes.find((node) => node.status === 'waiting_on_gate') ?? null
  }, [run])

  const updateDefinition = useCallback((updater: (current: WorkflowDefinitionDto) => WorkflowDefinitionDto) => {
    setDraft((current) => updater(cloneDefinition(current)))
  }, [])

  const updateSelectedNode = useCallback(
    (updater: (node: WorkflowNodeDto) => WorkflowNodeDto) => {
      if (!selectedNode) return
      updateDefinition((current) => ({
        ...current,
        nodes: current.nodes.map((node) => (node.id === selectedNode.id ? updater(node) : node)),
      }))
    },
    [selectedNode, updateDefinition],
  )

  const updateSelectedEdge = useCallback(
    (updater: (edge: WorkflowEdgeDto) => WorkflowEdgeDto) => {
      if (!selectedEdge) return
      updateDefinition((current) => ({
        ...current,
        edges: current.edges.map((edge) => (edge.id === selectedEdge.id ? updater(edge) : edge)),
      }))
    },
    [selectedEdge, updateDefinition],
  )

  const handleNodeDragStart = useCallback(() => {
    setDragging(true)
  }, [])

  const handleNodeDragStop = useCallback(
    (_event: unknown, node: WorkflowReactNode) => {
      setDragging(false)
      if (!editable || canvasInteractionsLocked) return
      updateDefinition((current) => ({
        ...current,
        nodes: current.nodes.map((entry) =>
          entry.id === node.id ? { ...entry, position: node.position } : entry,
        ),
      }))
    },
    [canvasInteractionsLocked, editable, updateDefinition],
  )

  const handleNodesChange = useCallback(
    (changes: NodeChange<WorkflowReactNode>[]) => {
      if (!editable || canvasInteractionsLocked) return
      const flowChanges: NodeChange<WorkflowReactNode>[] = []
      for (const change of changes) {
        if (change.type === 'remove') continue
        flowChanges.push(change)
      }
      if (flowChanges.length === 0) return
      setNodes((current) => {
        const next = applyNodeChanges(flowChanges, current)
        nodesRef.current = next
        return next
      })
    },
    [canvasInteractionsLocked, editable, setNodes],
  )

  const handleConnect = useCallback(
    (connection: Connection) => {
      if (!editable || canvasInteractionsLocked || !connection.source || !connection.target) return
      updateDefinition((current) => {
        if (!isValidWorkflowConnection(connection, current.nodes)) return current
        const edgeType = workflowEdgeTypeFromHandleId(connection.sourceHandle) ?? 'success'
        const id = uniqueId('edge', current.edges.map((edge) => edge.id))
        const baseEdge: WorkflowEdgeDto = {
          id,
          fromNodeId: connection.source ?? '',
          toNodeId: connection.target ?? '',
          type: edgeType,
          label: '',
          priority: workflowDefaultEdgePriority(edgeType),
          condition: { kind: 'always' },
          loopPolicy: null,
        }
        return {
          ...current,
          edges: [
            ...current.edges,
            edgeType === 'loop'
              ? { ...baseEdge, loopPolicy: defaultWorkflowLoopPolicy(baseEdge) }
              : baseEdge,
          ],
        }
      })
    },
    [canvasInteractionsLocked, editable, updateDefinition],
  )

  const isWorkflowConnectionValid = useCallback(
    (connection: Connection | WorkflowReactEdge) =>
      isValidWorkflowConnection(connection, effectiveDefinition.nodes),
    [effectiveDefinition.nodes],
  )

  const addNode = useCallback(
    (type: WorkflowNodeDto['type']) => {
      updateDefinition((current) => {
        const id = uniqueId(type.replace('_', '-'), current.nodes.map((node) => node.id))
        const offset = current.nodes.length * 36
        const node = createNode(type, id, agents[0]?.ref ?? {
          kind: 'built_in',
          runtimeAgentId: 'generalist',
          version: 1,
        }, { x: 140 + offset, y: 180 + offset })
        return {
          ...current,
          nodes: [...current.nodes, node],
          startNodeId: current.nodes.length === 0 ? id : current.startNodeId,
        }
      })
      setSelection(null)
    },
    [agents, updateDefinition],
  )

  const deleteSelected = useCallback(() => {
    if (!selection) return
    updateDefinition((current) => {
      if (selection.kind === 'edge') {
        return {
          ...current,
          edges: current.edges.filter((edge) => edge.id !== selection.id),
        }
      }
      const nextNodes = current.nodes.filter((node) => node.id !== selection.id)
      const nextEdges = current.edges.filter(
        (edge) => edge.fromNodeId !== selection.id && edge.toNodeId !== selection.id,
      )
      return {
        ...current,
        nodes: nextNodes,
        edges: nextEdges,
        startNodeId:
          current.startNodeId === selection.id ? nextNodes[0]?.id ?? '' : current.startNodeId,
      }
    })
    setSelection(null)
  }, [selection, updateDefinition])

  const handleSave = useCallback(async () => {
    if (!onSaveDefinition || validation.status !== 'valid') return
    setLocalError(null)
    try {
      const saved = await onSaveDefinition(draft)
      if (saved) setDraft(autoLayoutWorkflowDefinition(cloneDefinition(saved)))
      setMode('view')
    } catch (error) {
      setLocalError(error instanceof Error ? error.message : 'Xero could not save the Workflow.')
    }
  }, [draft, onSaveDefinition, validation.status])

  const handleStart = useCallback(async () => {
    if (!onStartRun) return
    setLocalError(null)
    try {
      await onStartRun(definition.id, buildInitialWorkflowInput(startInputFields, startInputs))
      setStartDialogOpen(false)
      setStartInputs({})
    } catch (error) {
      setLocalError(error instanceof Error ? error.message : 'Xero could not start the Workflow.')
    }
  }, [definition.id, onStartRun, startInputFields, startInputs])

  const handleCancelRun = useCallback(async () => {
    if (!run || !onCancelRun) return
    setLocalError(null)
    try {
      await onCancelRun(run.id)
    } catch (error) {
      setLocalError(error instanceof Error ? error.message : 'Xero could not cancel the Workflow run.')
    }
  }, [onCancelRun, run])

  const handleRetryNodeRun = useCallback(
    async (nodeRunId: string) => {
      if (!run || !onRetryNodeRun) return
      setLocalError(null)
      try {
        await onRetryNodeRun(run.id, nodeRunId)
      } catch (error) {
        setLocalError(error instanceof Error ? error.message : 'Xero could not retry the Workflow node.')
      }
    },
    [onRetryNodeRun, run],
  )

  const handleSkipBranch = useCallback(
    async (nodeRunId: string) => {
      if (!run || !onSkipBranch) return
      setLocalError(null)
      try {
        await onSkipBranch(run.id, nodeRunId, 'Skipped from the Workflow canvas.')
      } catch (error) {
        setLocalError(error instanceof Error ? error.message : 'Xero could not skip the Workflow branch.')
      }
    },
    [onSkipBranch, run],
  )

  const handleResumeCheckpoint = useCallback(
    async (decision: string) => {
      if (!run || !waitingCheckpoint || !onResumeCheckpoint) return
      setLocalError(null)
      try {
        await onResumeCheckpoint(run.id, waitingCheckpoint.id, decision, { decision })
      } catch (error) {
        setLocalError(error instanceof Error ? error.message : 'Xero could not resume the Workflow.')
      }
    },
    [onResumeCheckpoint, run, waitingCheckpoint],
  )

  const handleExplainRunBlocker = useCallback(async () => {
    if (!run || !onExplainRunBlocker) return
    setLocalError(null)
    setRecoveryAction('blocker')
    try {
      const response = await onExplainRunBlocker(run.id)
      if (response) {
        setRecoveryResult({
          kind: 'blocker',
          title: 'Current blocker',
          summary: response.summary,
          detail: JSON.stringify(response, null, 2),
        })
      }
    } catch (error) {
      setLocalError(error instanceof Error ? error.message : 'Xero could not explain the Workflow blocker.')
    } finally {
      setRecoveryAction(null)
    }
  }, [onExplainRunBlocker, run])

  const handleExportRunBundle = useCallback(async () => {
    if (!run || !onExportRunBundle) return
    setLocalError(null)
    setRecoveryAction('bundle')
    try {
      const response = await onExportRunBundle(run.id)
      if (response) {
        setRecoveryResult({
          kind: 'bundle',
          title: 'Support bundle',
          summary: 'Workflow run bundle ready for support inspection.',
          detail: JSON.stringify(workflowBundlePreview(response.bundle), null, 2),
        })
      }
    } catch (error) {
      setLocalError(error instanceof Error ? error.message : 'Xero could not export the Workflow run bundle.')
    } finally {
      setRecoveryAction(null)
    }
  }, [onExportRunBundle, run])

  const handleResumeNextIncompletePhase = useCallback(async () => {
    if (!run || !onResumeNextIncompletePhase) return
    setLocalError(null)
    setRecoveryAction('resume')
    try {
      const resumed = await onResumeNextIncompletePhase(run.id)
      if (resumed) {
        setRecoveryResult({
          kind: 'resume',
          title: 'Resume scheduled',
          summary: `Started Workflow run ${resumed.id}.`,
          detail: JSON.stringify({
            runId: resumed.id,
            status: resumed.status,
            workflowId: resumed.workflowId,
            initialInput: resumed.initialInput,
          }, null, 2),
        })
      }
    } catch (error) {
      setLocalError(error instanceof Error ? error.message : 'Xero could not resume the next delivery phase.')
    } finally {
      setRecoveryAction(null)
    }
  }, [onResumeNextIncompletePhase, run])

  const handleEdit = useCallback(() => {
    setDraft(autoLayoutWorkflowDefinition(cloneDefinition(definition)))
    setMode('edit')
  }, [definition])

  const handleCancelEditing = useCallback(() => {
    if (isCreating) {
      onCancelEditing?.()
      return
    }
    setDraft(autoLayoutWorkflowDefinition(cloneDefinition(definition)))
    setMode('view')
    setSelection(null)
  }, [definition, isCreating, onCancelEditing])

  const updateWorkflowName = useCallback(
    (name: string) => updateDefinition((current) => ({ ...current, name })),
    [updateDefinition],
  )

  const updateWorkflowDescription = useCallback(
    (description: string) => updateDefinition((current) => ({ ...current, description })),
    [updateDefinition],
  )

  const fitWorkflowView = useCallback(() => {
    void reactFlow.fitView({ ...FIT_VIEW_OPTIONS, duration: 420 })
  }, [reactFlow])

  useEffect(() => {
    if (!editable || effectiveDefinition.nodes.length === 0) return
    if (editFitFrameRef.current !== null) {
      window.cancelAnimationFrame(editFitFrameRef.current)
    }
    editFitFrameRef.current = window.requestAnimationFrame(() => {
      editFitFrameRef.current = null
      void reactFlow.fitView({ ...FIT_VIEW_OPTIONS, duration: 0 })
    })
    return () => {
      if (editFitFrameRef.current !== null) {
        window.cancelAnimationFrame(editFitFrameRef.current)
        editFitFrameRef.current = null
      }
    }
  }, [editable, effectiveDefinition.id, effectiveDefinition.nodes.length, effectiveDefinition.version, reactFlow])

  const handleResetLayout = useCallback(() => {
    updateDefinition(autoLayoutWorkflowDefinition)
    window.requestAnimationFrame(fitWorkflowView)
  }, [fitWorkflowView, updateDefinition])

  const workflowControlItems = useMemo<AgentCanvasControlItem[]>(
    () =>
      editable
        ? WORKFLOW_CONTROL_ENTRIES.map((entry) => {
            const Icon = entry.icon
            return {
              key: entry.type,
              label: entry.label,
              title: entry.label,
              disabled: canvasInteractionsLocked,
              onClick: () => addNode(entry.type),
              children: <Icon className="h-[18px] w-[18px]" aria-hidden="true" />,
            }
          })
        : [],
    [addNode, canvasInteractionsLocked, editable],
  )

  useEffect(() => {
    onCanvasStatusChange?.({
      editing: editable,
      saving,
      runningAction,
      saveDisabled: !canSave || saving,
      diagnosticCount: validation.diagnostics.length,
      diagnostics: validation.diagnostics,
      errorMessage: localError,
      definition: effectiveDefinition,
      run,
      updateName: updateWorkflowName,
      updateDescription: updateWorkflowDescription,
      edit: handleEdit,
      save: () => {
        void handleSave()
      },
      cancel: handleCancelEditing,
      start: () => setStartDialogOpen(true),
      cancelRun:
        run && isActiveRun(run.status) && onCancelRun
          ? () => {
              void handleCancelRun()
            }
          : null,
      retryNodeRun:
        run && onRetryNodeRun
          ? (nodeRunId: string) => {
              void handleRetryNodeRun(nodeRunId)
            }
          : null,
      skipBranch:
        run && onSkipBranch
          ? (nodeRunId: string) => {
              void handleSkipBranch(nodeRunId)
            }
          : null,
    })
    return () => onCanvasStatusChange?.(null)
  }, [
    canSave,
    editable,
    effectiveDefinition,
    handleCancelEditing,
    handleCancelRun,
    handleEdit,
    handleRetryNodeRun,
    handleSave,
    handleSkipBranch,
    localError,
    onCancelRun,
    onCanvasStatusChange,
    onRetryNodeRun,
    onSkipBranch,
    run,
    runningAction,
    saving,
    updateWorkflowDescription,
    updateWorkflowName,
    validation.diagnostics,
  ])

  return (
    <div
      className={cn(
        'agent-visualization relative h-full w-full overflow-hidden',
        'is-workflow-definition',
        canvasInteractionsLocked && 'is-locked',
        entering && 'is-workflow-entering',
        dragging && 'is-dragging',
        editable && 'is-editing',
        selection?.kind === 'node' && 'is-node-focused',
      )}
    >
      <ReactFlow
        nodes={renderedNodes}
        edges={edges}
        nodeTypes={NODE_TYPES}
        edgeTypes={EDGE_TYPES}
        fitView={!editable && effectiveDefinition.nodes.length > 0}
        fitViewOptions={FIT_VIEW_OPTIONS}
        defaultViewport={AGENT_CANVAS_EMPTY_VIEWPORT}
        minZoom={0.2}
        maxZoom={2}
        connectionMode={ConnectionMode.Loose}
        isValidConnection={isWorkflowConnectionValid}
        nodesDraggable={editable && !canvasInteractionsLocked}
        nodesConnectable={editable && !canvasInteractionsLocked}
        elementsSelectable
        snapToGrid={snapToGrid}
        snapGrid={AGENT_CANVAS_SNAP_GRID}
        onNodesChange={handleNodesChange}
        onConnect={handleConnect}
        onNodeDragStart={handleNodeDragStart}
        onEdgeClick={(_, edge) => {
          if (edge.data?.visualOnly) return
          setSelection({ kind: 'edge', id: edge.id })
        }}
        onNodeClick={(_, node) => setSelection({ kind: 'node', id: node.id })}
        onNodeDragStop={handleNodeDragStop}
        onPaneClick={() => setSelection(null)}
        proOptions={{ hideAttribution: true }}
      >
        <AgentCanvasDots />
        <AgentCanvasControls
          showLayoutControls={editable}
          layoutControlsDisabled={canvasInteractionsLocked}
          locked={canvasLocked}
          snapToGrid={snapToGrid}
          extraControls={workflowControlItems}
          onFitView={fitWorkflowView}
          onToggleLock={() => setCanvasLocked((current) => !current)}
          onToggleSnapToGrid={() => setSnapToGrid((current) => !current)}
          onResetLayout={handleResetLayout}
        />
      </ReactFlow>

      {editable && effectiveDefinition.nodes.length === 0 ? (
        <WorkflowDraftEmptyState
          onAddAgent={() => addNode('agent')}
          onCreateAgent={onCreateAgent}
        />
      ) : null}

      {editable && (selectedNode || selectedEdge) ? (
        <WorkflowPropertiesPanel
          agents={agentOptions}
          definition={effectiveDefinition}
          selectedNode={selectedNode}
          selectedEdge={selectedEdge}
          diagnostics={validation.diagnostics.filter((diagnostic) =>
            selection?.kind === 'node'
              ? diagnostic.path.includes(selection.id)
              : selection?.kind === 'edge'
                ? diagnostic.path.includes(selection.id)
                : false,
          )}
          onClose={() => setSelection(null)}
          onDelete={deleteSelected}
          onUpdateNode={updateSelectedNode}
          onUpdateEdge={updateSelectedEdge}
          onSetStartNode={(nodeId) =>
            updateDefinition((current) => ({ ...current, startNodeId: nodeId }))
          }
          onCreateAgent={onCreateAgent}
          onEditAgent={onEditAgent}
        />
      ) : !editable && (selectedNode || selectedEdge) ? (
        <WorkflowDetailsPanel
          definition={effectiveDefinition}
          node={selectedNode}
          edge={selectedEdge}
          runNode={selectedNode ? latestRunNodeByNodeId.get(selectedNode.id) ?? null : null}
          artifact={selectedNode ? latestArtifactByNodeId.get(selectedNode.id) ?? null : null}
          edgeDecision={selectedEdge ? latestEdgeDecision(run, selectedEdge.id) : null}
          events={run?.events ?? []}
          subgraphChildRuns={selectedSubgraphChildRuns}
          artifactByNodeRunId={latestArtifactByNodeRunId}
          agentLabel={selectedNode?.type === 'agent' ? labelForAgentRef(selectedNode.agentRef, agents) : null}
          running={runningAction}
          onRetryNodeRun={onRetryNodeRun ? handleRetryNodeRun : null}
          onSkipBranch={onSkipBranch ? handleSkipBranch : null}
          onClose={() => setSelection(null)}
        />
      ) : null}

      {!editable && run && (onExplainRunBlocker || onExportRunBundle || (onResumeNextIncompletePhase && workflowRunCanResumeNextPhase(run))) && workflowRunNeedsRecoverySurface(run) ? (
        <WorkflowRunRecoveryPanel
          run={run}
          result={recoveryResult}
          runningAction={runningAction || recoveryAction !== null}
          recoveryAction={recoveryAction}
          onExplainRunBlocker={onExplainRunBlocker ? handleExplainRunBlocker : null}
          onExportRunBundle={onExportRunBundle ? handleExportRunBundle : null}
          onResumeNextIncompletePhase={onResumeNextIncompletePhase && workflowRunCanResumeNextPhase(run) ? handleResumeNextIncompletePhase : null}
        />
      ) : null}

      {waitingCheckpoint && onResumeCheckpoint ? (
        <CheckpointResumeBar
          node={effectiveDefinition.nodes.find((node) => node.id === waitingCheckpoint.nodeId) ?? null}
          onResume={handleResumeCheckpoint}
          running={runningAction}
        />
      ) : null}

      <Dialog open={startDialogOpen} onOpenChange={setStartDialogOpen}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>Start {definition.name}</DialogTitle>
            <DialogDescription>
              Run inputs are captured before Xero creates the Workflow run.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            {startInputFields.length > 0 ? (
              <div className="space-y-3">
                {startInputFields.map((field) => (
                  <div key={field.key} className="space-y-2">
                    <Label htmlFor={`workflow-start-${field.key}`}>
                      {field.label}{field.required ? '' : ' (optional)'}
                    </Label>
                    <Textarea
                      id={`workflow-start-${field.key}`}
                      value={startInputs[field.key] ?? ''}
                      onChange={(event) =>
                        setStartInputs((current) => ({
                          ...current,
                          [field.key]: event.target.value,
                        }))
                      }
                      className="min-h-24"
                    />
                  </div>
                ))}
              </div>
            ) : null}
            <WorkflowStartPreview preview={startPreview} />
          </div>
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => setStartDialogOpen(false)}>
              Cancel
            </Button>
            <Button type="button" onClick={() => void handleStart()} disabled={runningAction || !startInputValid}>
              {runningAction ? <Loader2 className="size-4 animate-spin" /> : <Play className="size-4" />}
              Start
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function WorkflowNodeCard({ data, selected }: NodeProps<WorkflowReactNode>) {
  const node = data.node
  const Icon = iconForNodeType(node.type)
  const status = data.runNode?.status ?? null
  const isStart = data.isStart
  return (
    <>
      {data.handles.map((handle) => (
        <Handle
          key={handle.id}
          id={handle.id}
          type="source"
          position={WORKFLOW_HANDLE_POSITION[handle.side]}
          className={handle.className}
          style={handle.style}
          isConnectableStart
          isConnectableEnd
        />
      ))}
      <CanvasNodeCard
        title={node.title}
        subtitle={NODE_KIND_LABEL[node.type]}
        icon={Icon}
        tone={nodeTone(node.type)}
        iconClassName={nodeIconTone(node.type)}
        width={WORKFLOW_LAYOUT_CARD_WIDTH}
        selected={selected}
        detail={
          node.type === 'agent'
            ? data.agentLabel ?? 'Choose an agent'
            : node.type === 'terminal'
              ? humanize(node.terminalStatus)
              : node.description || `${data.incomingCount} in · ${data.outgoingCount} out`
        }
        badges={
          <>
            {isStart ? (
              <Badge
                variant="outline"
                className="h-5 px-1.5 text-[9.5px] font-medium border-amber-500/40 bg-amber-500/12 text-amber-700 dark:text-amber-300"
              >
                <Flag className="mr-0.5 h-2.5 w-2.5" aria-hidden="true" />
                start
              </Badge>
            ) : null}
            {status ? (
              <Badge
                variant="outline"
                className={cn('h-5 px-1.5 text-[9.5px] font-medium', NODE_STATUS_TONE[status])}
              >
                {humanize(status)}
              </Badge>
            ) : null}
          </>
        }
        chips={
          <>
            {node.type === 'agent' ? (
              <Badge
                variant="outline"
                className="h-5 px-1.5 text-[9.5px] font-medium border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300"
              >
                {node.outputContract.artifactType}
              </Badge>
            ) : null}
            {data.artifact ? (
              <Badge
                variant="outline"
                className="h-5 px-1.5 text-[9.5px] font-medium border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
              >
                artifact
              </Badge>
            ) : null}
          </>
        }
      />
    </>
  )
}

function WorkflowDraftEmptyState({
  onAddAgent,
  onCreateAgent,
}: {
  onAddAgent: () => void
  onCreateAgent?: () => void
}) {
  return (
    <div className="pointer-events-none absolute inset-0 z-[6] flex items-center justify-center px-6">
      <div
        aria-label="Blank workflow start"
        className="pointer-events-auto flex w-full max-w-sm flex-col items-center text-center"
        onPointerDown={(event) => event.stopPropagation()}
        role="region"
      >
        <div className="flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-card/80 shadow-sm">
          <Workflow className="h-6 w-6 text-foreground" aria-hidden="true" />
        </div>
        <h3 className="mt-5 text-[22px] font-semibold tracking-tight text-foreground">
          Start with an agent
        </h3>
        <p className="mt-2 max-w-sm text-[12.5px] leading-relaxed text-muted-foreground">
          The first step receives the goal and produces the first handoff. Add routing, checkpoints,
          and terminal outcomes after that path exists.
        </p>
        <Button
          type="button"
          className="mt-6 h-10 w-full justify-center text-[12.5px] font-semibold"
          onClick={onAddAgent}
        >
          <Bot className="h-4 w-4" />
          Add first agent step
        </Button>
        {onCreateAgent ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="mt-3 text-muted-foreground hover:text-foreground"
            onClick={onCreateAgent}
          >
            <Plus className="h-3.5 w-3.5" />
            Create agent
          </Button>
        ) : null}
      </div>
    </div>
  )
}

function WorkflowPropertiesPanel({
  agents,
  definition,
  selectedNode,
  selectedEdge,
  diagnostics,
  onClose,
  onDelete,
  onUpdateNode,
  onUpdateEdge,
  onSetStartNode,
  onCreateAgent,
  onEditAgent,
}: {
  agents: { agent: WorkflowAgentSummaryDto; key: string }[]
  definition: WorkflowDefinitionDto
  selectedNode: WorkflowNodeDto | null
  selectedEdge: WorkflowEdgeDto | null
  diagnostics: { message: string; path: string }[]
  onClose: () => void
  onDelete: () => void
  onUpdateNode: (updater: (node: WorkflowNodeDto) => WorkflowNodeDto) => void
  onUpdateEdge: (updater: (edge: WorkflowEdgeDto) => WorkflowEdgeDto) => void
  onSetStartNode: (nodeId: string) => void
  onCreateAgent?: () => void
  onEditAgent?: (ref: AgentRefDto) => void
}) {
  const title = selectedNode ? selectedNode.title : selectedEdge?.label || selectedEdge?.id || ''
  const selectedAgentLabel =
    selectedNode?.type === 'agent'
      ? labelForAgentRef(selectedNode.agentRef, agents.map((entry) => entry.agent))
      : null
  return (
    <div
      className="agent-properties-panel pointer-events-auto absolute bottom-4 left-5 z-30 flex max-h-[calc(100%-5rem)] w-[300px] flex-col overflow-hidden rounded-lg border border-border/60 bg-card/95 text-[11.5px] text-card-foreground shadow-[0_8px_28px_-12px_rgba(0,0,0,0.55)] backdrop-blur-md"
      onPointerDown={(event) => event.stopPropagation()}
      onWheel={(event) => event.stopPropagation()}
    >
      <header className="flex items-center gap-2 border-b border-border/50 px-3 py-1.5">
        <span className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded bg-primary/10 text-primary">
          {selectedNode ? <Workflow className="h-3 w-3" /> : <GitBranch className="h-3 w-3" />}
        </span>
        <p className="min-w-0 flex-1 truncate text-[12px] font-semibold leading-none text-foreground">
          {title}
        </p>
        <Button type="button" size="icon-sm" variant="ghost" onClick={onDelete} className="size-5 text-muted-foreground hover:text-destructive" aria-label="Delete selected workflow item">
          <Trash2 className="h-3 w-3" />
        </Button>
        <Button type="button" size="icon-sm" variant="ghost" onClick={onClose} className="size-5 text-muted-foreground hover:text-foreground" aria-label="Close properties">
          <X className="h-3 w-3" />
        </Button>
      </header>
      <div className="min-h-0 space-y-4 overflow-y-auto px-3 py-3">
        {diagnostics.length > 0 ? (
          <div className="space-y-1 rounded-md border border-destructive/25 bg-destructive/10 px-2 py-2 text-[11px] text-destructive">
            {diagnostics.slice(0, 3).map((diagnostic) => (
              <p key={`${diagnostic.path}:${diagnostic.message}`}>{diagnostic.message}</p>
            ))}
          </div>
        ) : null}
        <WorkflowGuidanceCard
          definition={definition}
          edge={selectedEdge}
          node={selectedNode}
          agentLabel={selectedAgentLabel}
        />
        {selectedNode ? (
          <NodeEditor
            agents={agents}
            definition={definition}
            node={selectedNode}
            onUpdate={onUpdateNode}
            onSetStartNode={onSetStartNode}
            onCreateAgent={onCreateAgent}
            onEditAgent={onEditAgent}
          />
        ) : selectedEdge ? (
          <EdgeEditor definition={definition} edge={selectedEdge} onUpdate={onUpdateEdge} />
        ) : null}
      </div>
    </div>
  )
}

function NodeEditor({
  agents,
  definition,
  node,
  onUpdate,
  onSetStartNode,
  onCreateAgent,
  onEditAgent,
}: {
  agents: { agent: WorkflowAgentSummaryDto; key: string }[]
  definition: WorkflowDefinitionDto
  node: WorkflowNodeDto
  onUpdate: (updater: (node: WorkflowNodeDto) => WorkflowNodeDto) => void
  onSetStartNode: (nodeId: string) => void
  onCreateAgent?: () => void
  onEditAgent?: (ref: AgentRefDto) => void
}) {
  return (
    <>
      <Field label="Title" hint="Name this step the way a teammate would describe it.">
        <Input
          value={node.title}
          onChange={(event) => onUpdate((current) => ({ ...current, title: event.target.value }))}
          className="h-8 text-[12px]"
        />
      </Field>
      <Field label="Description" hint="Optional note shown on the canvas and in run details.">
        <Textarea
          value={node.description}
          onChange={(event) => onUpdate((current) => ({ ...current, description: event.target.value }))}
          className="min-h-20 text-[12px]"
        />
      </Field>
      <Field label="Start" hint="The Workflow begins at exactly one start node.">
        <Button
          type="button"
          variant={definition.startNodeId === node.id ? 'secondary' : 'outline'}
          size="sm"
          className="h-7 text-[11px]"
          onClick={() => onSetStartNode(node.id)}
          disabled={definition.startNodeId === node.id}
        >
          <Lock className="size-3" />
          {definition.startNodeId === node.id ? 'Start node' : 'Make start node'}
        </Button>
      </Field>
      {node.type === 'agent' ? (
        <AgentNodeEditor
          agents={agents}
          node={node}
          onUpdate={onUpdate}
          onCreateAgent={onCreateAgent}
          onEditAgent={onEditAgent}
        />
      ) : null}
      {node.type === 'gate' ? (
        <Field label="Blocked behavior" hint="Choose what happens when a required check does not pass.">
          <Select
            value={node.onBlocked}
            onValueChange={(value) => onUpdate((current) => current.type === 'gate' ? { ...current, onBlocked: value as 'pause' | 'fail' } : current)}
          >
            <SelectTrigger className="h-8 text-[12px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="pause">Pause</SelectItem>
              <SelectItem value="fail">Fail</SelectItem>
            </SelectContent>
          </Select>
        </Field>
      ) : null}
      {node.type === 'state_read' || node.type === 'state_query' ? (
        <>
          <div
            className={cn(
              'rounded-md border border-border/45 bg-muted/25 px-2.5 py-2 text-[10.5px] leading-relaxed text-muted-foreground',
              WORKFLOW_PANEL_TEXT_WRAP_CLASS,
            )}
          >
            {workflowStateQueryPlainSummary(node.query)}
          </div>
          <Field label="Record type" hint="The durable project state this node reads.">
            <Select
              value={node.query.entityType}
              onValueChange={(value) =>
                onUpdate((current) =>
                  current.type === 'state_read' || current.type === 'state_query'
                    ? {
                        ...current,
                        query: {
                          ...current.query,
                          entityType: value as typeof current.query.entityType,
                        },
                      }
                    : current,
                )
              }
            >
              <SelectTrigger className="h-8 text-[12px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {DELIVERY_ENTITY_TYPES.map((entity) => (
                  <SelectItem key={entity} value={entity}>
                    {humanize(entity)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label="Filter field" hint="Common fields include status, priority, updatedAt, and sortOrder.">
            <Input
              value={node.query.filters[0]?.path ?? '$.status'}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'state_read' || current.type === 'state_query'
                    ? {
                        ...current,
                        query: {
                          ...current.query,
                          filters: [
                            {
                              path: event.target.value,
                              operator: current.query.filters[0]?.operator ?? 'neq',
                              value: current.query.filters[0]?.value ?? 'archived',
                              values: current.query.filters[0]?.values ?? [],
                            },
                          ],
                        },
                      }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Filter" hint="Use simple language first; the raw JSON path stays editable here.">
            <Select
              value={node.query.filters[0]?.operator ?? 'neq'}
              onValueChange={(value) =>
                onUpdate((current) =>
                  current.type === 'state_read' || current.type === 'state_query'
                    ? {
                        ...current,
                        query: {
                          ...current.query,
                          filters: [
                            {
                              path: current.query.filters[0]?.path ?? '$.status',
                              operator: value as typeof current.query.filters[number]['operator'],
                              value: current.query.filters[0]?.value ?? 'archived',
                              values: current.query.filters[0]?.values ?? [],
                            },
                          ],
                        },
                      }
                    : current,
                )
              }
            >
              <SelectTrigger className="h-8 text-[12px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {['eq', 'neq', 'in', 'not_in', 'exists', 'missing'].map((operator) => (
                  <SelectItem key={operator} value={operator}>
                    {operator}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label="Compare to">
            <Input
              value={stateFilterValueText(node.query.filters[0])}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'state_read' || current.type === 'state_query'
                    ? {
                        ...current,
                        query: {
                          ...current.query,
                          filters: [
                            stateFilterFromText(
                              current.query.filters[0],
                              event.target.value,
                            ),
                          ],
                        },
                      }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Sort by" hint="Leave blank for default ordering.">
            <Input
              value={node.query.orderBy ?? ''}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'state_read' || current.type === 'state_query'
                    ? {
                        ...current,
                        query: {
                          ...current.query,
                          orderBy: event.target.value.trim() ? event.target.value : null,
                        },
                      }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Handoff name" hint="This is the name later nodes use to consume the query result.">
            <Input
              value={node.outputArtifactType}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'state_read' || current.type === 'state_query'
                    ? { ...current, outputArtifactType: event.target.value }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
        </>
      ) : null}
      {node.type === 'state_write' || node.type === 'state_patch' ? (
        <>
          <div
            className={cn(
              'rounded-md border border-border/45 bg-muted/25 px-2.5 py-2 text-[10.5px] leading-relaxed text-muted-foreground',
              WORKFLOW_PANEL_TEXT_WRAP_CLASS,
            )}
          >
            {workflowStateWritePlainSummary(node.operation)}
          </div>
          <Field label="Record type">
            <Select
              value={node.operation.entityType}
              onValueChange={(value) =>
                onUpdate((current) =>
                  current.type === 'state_write' || current.type === 'state_patch'
                    ? {
                        ...current,
                        operation: {
                          ...current.operation,
                          entityType: value as typeof current.operation.entityType,
                        },
                      }
                    : current,
                )
              }
            >
              <SelectTrigger className="h-8 text-[12px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {DELIVERY_ENTITY_TYPES.map((entity) => (
                  <SelectItem key={entity} value={entity}>
                    {humanize(entity)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label="Action" hint="Use upsert for repeatable writes; use mark complete or archive for lifecycle changes.">
            <Select
              value={node.operation.action}
              onValueChange={(value) =>
                onUpdate((current) =>
                  current.type === 'state_write' || current.type === 'state_patch'
                    ? {
                        ...current,
                        operation: {
                          ...current.operation,
                          action: value as typeof current.operation.action,
                        },
                      }
                    : current,
                )
              }
            >
              <SelectTrigger className="h-8 text-[12px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {STATE_WRITE_ACTIONS.map((action) => (
                  <SelectItem key={action} value={action}>
                    {humanize(action)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label="Target id" hint="Leave blank when the payload creates or resolves the record id.">
            <Input
              value={node.operation.targetId ?? ''}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'state_write' || current.type === 'state_patch'
                    ? {
                        ...current,
                        operation: {
                          ...current.operation,
                          targetId: event.target.value.trim() ? event.target.value : null,
                        },
                      }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Payload JSON" hint="Advanced: values can include runtime bindings like input, artifact, or state placeholders.">
            <Textarea
              value={JSON.stringify(node.operation.payload, null, 2)}
              onChange={(event) => {
                try {
                  const parsed = JSON.parse(event.target.value)
                  if (!isRecord(parsed)) return
                  onUpdate((current) =>
                    current.type === 'state_write' || current.type === 'state_patch'
                      ? { ...current, operation: { ...current.operation, payload: parsed } }
                      : current,
                  )
                } catch {
                  // Keep the last valid payload while the user is mid-edit.
                }
              }}
              className="min-h-28 font-mono text-[11px]"
            />
          </Field>
        </>
      ) : null}
      {node.type === 'state_checkpoint' ? (
        <Field label="Blocked behavior" hint="Choose what happens when durable state is not ready.">
          <Select
            value={node.onBlocked}
            onValueChange={(value) => onUpdate((current) => current.type === 'state_checkpoint' ? { ...current, onBlocked: value as 'pause' | 'fail' } : current)}
          >
            <SelectTrigger className="h-8 text-[12px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="pause">Pause</SelectItem>
              <SelectItem value="fail">Fail</SelectItem>
            </SelectContent>
          </Select>
        </Field>
      ) : null}
      {node.type === 'collection_loop' ? (
        <>
          <div
            className={cn(
              'rounded-md border border-border/45 bg-muted/25 px-2.5 py-2 text-[10.5px] leading-relaxed text-muted-foreground',
              WORKFLOW_PANEL_TEXT_WRAP_CLASS,
            )}
          >
            {workflowStateQueryPlainSummary(node.collection)}
          </div>
          <Field label="Collection">
            <Select
              value={node.collection.entityType}
              onValueChange={(value) =>
                onUpdate((current) =>
                  current.type === 'collection_loop'
                    ? {
                        ...current,
                        collection: {
                          ...current.collection,
                          entityType: value as typeof current.collection.entityType,
                        },
                      }
                    : current,
                )
              }
            >
              <SelectTrigger className="h-8 text-[12px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {DELIVERY_ENTITY_TYPES.map((entity) => (
                  <SelectItem key={entity} value={entity}>
                    {humanize(entity)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label="Sort by" hint="The field used to pick the next item.">
            <Input
              value={node.sortKey ?? ''}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'collection_loop'
                    ? { ...current, sortKey: event.target.value.trim() ? event.target.value : null }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Max items" hint="Hard guard against runaway loops.">
            <Input
              type="number"
              min={1}
              value={node.maxItemCount}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'collection_loop'
                    ? { ...current, maxItemCount: Math.max(1, Number(event.target.value) || 1) }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Only input path" hint="Optional start input path for running one matching item.">
            <Input
              value={node.controls.onlyInputPath ?? ''}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'collection_loop'
                    ? {
                        ...current,
                        controls: {
                          ...current.controls,
                          onlyInputPath: event.target.value.trim() ? event.target.value : null,
                        },
                      }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <div className="grid grid-cols-2 gap-2">
            <Field label="From path">
              <Input
                value={node.controls.fromInputPath ?? ''}
                onChange={(event) =>
                  onUpdate((current) =>
                    current.type === 'collection_loop'
                      ? {
                          ...current,
                          controls: {
                            ...current.controls,
                            fromInputPath: event.target.value.trim() ? event.target.value : null,
                          },
                        }
                      : current,
                  )
                }
                className="h-8 text-[12px]"
              />
            </Field>
            <Field label="To path">
              <Input
                value={node.controls.toInputPath ?? ''}
                onChange={(event) =>
                  onUpdate((current) =>
                    current.type === 'collection_loop'
                      ? {
                          ...current,
                          controls: {
                            ...current.controls,
                            toInputPath: event.target.value.trim() ? event.target.value : null,
                          },
                        }
                      : current,
                  )
                }
                className="h-8 text-[12px]"
              />
            </Field>
          </div>
        </>
      ) : null}
      {node.type === 'subgraph' ? (
        <Field label="Subflow">
          <Select
            value={node.subgraphId}
            onValueChange={(value) =>
              onUpdate((current) => current.type === 'subgraph' ? { ...current, subgraphId: value } : current)
            }
          >
            <SelectTrigger className="h-8 text-[12px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {definition.subgraphs.length > 0 ? definition.subgraphs.map((subgraph) => (
                <SelectItem key={subgraph.id} value={subgraph.id}>
                  {subgraph.title}
                </SelectItem>
              )) : (
                <SelectItem value={node.subgraphId}>{node.subgraphId}</SelectItem>
              )}
            </SelectContent>
          </Select>
        </Field>
      ) : null}
      {node.type === 'command' ? (
        <>
          <Field label="Command" hint="Only commands on the allowlist can run.">
            <Input
              value={node.command}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'command'
                    ? { ...current, command: event.target.value, allowedCommands: [event.target.value].filter(Boolean) }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Arguments">
            <Input
              value={node.args.join(' ')}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'command'
                    ? { ...current, args: event.target.value.split(' ').map((entry) => entry.trim()).filter(Boolean) }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Timeout seconds" hint="Commands stop automatically after this limit.">
            <Input
              type="number"
              min={1}
              value={node.timeoutSeconds}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'command'
                    ? { ...current, timeoutSeconds: Math.max(1, Number(event.target.value) || 1) }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Allowlist" hint="Comma-separated executable names approved for this node.">
            <Input
              value={node.allowedCommands.join(', ')}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'command'
                    ? {
                        ...current,
                        allowedCommands: event.target.value
                          .split(',')
                          .map((entry) => entry.trim())
                          .filter(Boolean),
                      }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Working directory" hint="Leave blank for the project workspace.">
            <Input
              value={node.workingDirectory ?? ''}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'command'
                    ? {
                        ...current,
                        workingDirectory: event.target.value.trim() ? event.target.value : null,
                      }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Parser" hint="Choose whether output is plain text or structured JSON.">
            <Select
              value={node.parser.extraction}
              onValueChange={(value) =>
                onUpdate((current) =>
                  current.type === 'command'
                    ? {
                        ...current,
                        parser: {
                          ...current.parser,
                          extraction: value as WorkflowOutputExtractionDto,
                        },
                      }
                    : current,
                )
              }
            >
              <SelectTrigger className="h-8 text-[12px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="generic_text">Generic text</SelectItem>
                <SelectItem value="json_object">JSON object</SelectItem>
                <SelectItem value="json_array">JSON array</SelectItem>
              </SelectContent>
            </Select>
          </Field>
          <Field label="Preview field" hint="JSON path used for the run detail preview.">
            <Input
              value={node.parser.renderTextPath ?? ''}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'command'
                    ? {
                        ...current,
                        parser: {
                          ...current.parser,
                          renderTextPath: event.target.value.trim() ? event.target.value : null,
                        },
                        outputContract: {
                          ...current.outputContract,
                          renderTextPath: event.target.value.trim() ? event.target.value : null,
                        },
                      }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
        </>
      ) : null}
      {node.type === 'human_checkpoint' ? (
        <>
          <Field label="Checkpoint type">
            <Select
              value={node.checkpointType}
              onValueChange={(value) =>
                onUpdate((current) =>
                  current.type === 'human_checkpoint'
                    ? {
                        ...current,
                        checkpointType: value as typeof current.checkpointType,
                      }
                    : current,
                )
              }
            >
              <SelectTrigger className="h-8 text-[12px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="human_verify">Human verify</SelectItem>
                <SelectItem value="decision">Decision</SelectItem>
                <SelectItem value="human_action">Human action</SelectItem>
              </SelectContent>
            </Select>
          </Field>
          <Field label="Checkpoint prompt" hint="This prompt is shown when the Workflow pauses.">
            <Textarea
              value={node.prompt}
              onChange={(event) => onUpdate((current) => current.type === 'human_checkpoint' ? { ...current, prompt: event.target.value } : current)}
              className="min-h-20 text-[12px]"
            />
          </Field>
          <Field label="Decision buttons" hint="Comma-separated labels, such as continue, stop, or gaps_found.">
            <Input
              value={node.decisionOptions.join(', ')}
              onChange={(event) =>
                onUpdate((current) =>
                  current.type === 'human_checkpoint'
                    ? {
                        ...current,
                        decisionOptions: event.target.value
                          .split(',')
                          .map((entry) => entry.trim())
                          .filter(Boolean),
                      }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Resume payload schema" hint="Advanced: optional JSON Schema for extra data on resume.">
            <Textarea
              value={JSON.stringify(node.resumePayloadSchema ?? {}, null, 2)}
              onChange={(event) => {
                try {
                  const parsed = JSON.parse(event.target.value)
                  if (!isRecord(parsed)) return
                  onUpdate((current) =>
                    current.type === 'human_checkpoint'
                      ? {
                          ...current,
                          resumePayloadSchema: Object.keys(parsed).length > 0 ? parsed : null,
                        }
                      : current,
                  )
                } catch {
                  // Keep the last valid schema while the user is mid-edit.
                }
              }}
              className="min-h-24 font-mono text-[11px]"
            />
          </Field>
          <Field label="State updates JSON" hint="Advanced: optional state writes after a human decision.">
            <Textarea
              value={JSON.stringify(node.stateUpdates, null, 2)}
              onChange={(event) => {
                try {
                  const parsed = JSON.parse(event.target.value)
                  if (!Array.isArray(parsed)) return
                  onUpdate((current) =>
                    current.type === 'human_checkpoint'
                      ? { ...current, stateUpdates: parsed as typeof current.stateUpdates }
                      : current,
                  )
                } catch {
                  // Keep the last valid updates while the user is mid-edit.
                }
              }}
              className="min-h-24 font-mono text-[11px]"
            />
          </Field>
        </>
      ) : null}
      {node.type === 'terminal' ? (
        <Field label="Final status">
          <Select
            value={node.terminalStatus}
            onValueChange={(value) =>
              onUpdate((current) =>
                current.type === 'terminal'
                  ? { ...current, terminalStatus: value as WorkflowTerminalStatusDto }
                  : current,
              )
            }
          >
            <SelectTrigger className="h-8 text-[12px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="success">Success</SelectItem>
              <SelectItem value="failure">Failure</SelectItem>
              <SelectItem value="cancelled">Cancelled</SelectItem>
              <SelectItem value="needs_human">Needs human</SelectItem>
            </SelectContent>
          </Select>
        </Field>
      ) : null}
    </>
  )
}

function AgentNodeEditor({
  agents,
  node,
  onUpdate,
  onCreateAgent,
  onEditAgent,
}: {
  agents: { agent: WorkflowAgentSummaryDto; key: string }[]
  node: Extract<WorkflowNodeDto, { type: 'agent' }>
  onUpdate: (updater: (node: WorkflowNodeDto) => WorkflowNodeDto) => void
  onCreateAgent?: () => void
  onEditAgent?: (ref: AgentRefDto) => void
}) {
  const selectedKey = agentRefKey(node.agentRef)
  const selectedHandoffPreset = WORKFLOW_HANDOFF_PRESETS.some(
    (preset) => preset.artifactType === node.outputContract.artifactType,
  )
    ? node.outputContract.artifactType
    : 'custom'
  return (
    <>
      <Field label="Agent" hint="Pick the worker that should handle this step.">
        <Select
          value={selectedKey}
          onValueChange={(value) => {
            const match = agents.find((entry) => entry.key === value)
            if (!match) return
            onUpdate((current) => current.type === 'agent' ? { ...current, agentRef: match.agent.ref } : current)
          }}
        >
          <SelectTrigger className="h-8 text-[12px]">
            <SelectValue placeholder="Choose agent" />
          </SelectTrigger>
          <SelectContent>
            {agents.map(({ agent, key }) => (
              <SelectItem key={key} value={key}>
                {agent.displayName}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="mt-2 flex gap-2">
          {onCreateAgent ? (
            <Button type="button" variant="outline" size="sm" className="h-7 text-[11px]" onClick={onCreateAgent}>
              <Plus className="size-3" />
              New
            </Button>
          ) : null}
          {onEditAgent && node.agentRef.kind === 'custom' ? (
            <Button type="button" variant="ghost" size="sm" className="h-7 text-[11px]" onClick={() => onEditAgent(node.agentRef)}>
              Edit
            </Button>
          ) : null}
        </div>
      </Field>
      <Field label="Handoff preset" hint="Choose a common result shape when later routing needs structure.">
        <Select
          value={selectedHandoffPreset}
          onValueChange={(value) => {
            if (value === 'custom') return
            const preset = WORKFLOW_HANDOFF_PRESETS.find((candidate) => candidate.artifactType === value)
            onUpdate((current) =>
              current.type === 'agent'
                ? {
                    ...current,
                    outputContract: {
                      ...current.outputContract,
                      artifactType: value,
                      extraction: preset?.extraction ?? current.outputContract.extraction,
                    },
                  }
                : current,
            )
          }}
        >
          <SelectTrigger className="h-8 text-[12px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {WORKFLOW_HANDOFF_PRESETS.map((preset) => (
              <SelectItem key={preset.artifactType} value={preset.artifactType}>
                {preset.label}
              </SelectItem>
            ))}
            <SelectItem value="custom">Custom handoff</SelectItem>
          </SelectContent>
        </Select>
        <p className="mt-1 text-[10.5px] leading-relaxed text-muted-foreground/75">
          {WORKFLOW_HANDOFF_PRESETS.find((preset) => preset.artifactType === node.outputContract.artifactType)?.description
            ?? 'Use a custom handoff when this Workflow owns a specialized schema.'}
        </p>
      </Field>
      <Field label="Handoff name" hint="Later nodes refer to this as this node plus this handoff name.">
        <Input
          value={node.outputContract.artifactType}
          onChange={(event) =>
            onUpdate((current) =>
              current.type === 'agent'
                ? {
                    ...current,
                    outputContract: {
                      ...current.outputContract,
                      artifactType: event.target.value,
                    },
                  }
                : current,
            )
          }
          className="h-8 text-[12px]"
        />
      </Field>
      <Field label="Extraction" hint="Use plain text for flexible summaries; use JSON when routers need fields.">
        <Select
          value={node.outputContract.extraction}
          onValueChange={(value) =>
            onUpdate((current) =>
              current.type === 'agent'
                ? {
                    ...current,
                    outputContract: {
                      ...current.outputContract,
                      extraction: value as WorkflowOutputExtractionDto,
                    },
                  }
                : current,
            )
          }
        >
          <SelectTrigger className="h-8 text-[12px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="generic_text">Generic text</SelectItem>
            <SelectItem value="json_object">JSON object</SelectItem>
            <SelectItem value="json_array">JSON array</SelectItem>
          </SelectContent>
        </Select>
      </Field>
    </>
  )
}

function EdgeRecipePicker({
  definition,
  edge,
  onUpdate,
}: {
  definition: WorkflowDefinitionDto
  edge: WorkflowEdgeDto
  onUpdate: (updater: (edge: WorkflowEdgeDto) => WorkflowEdgeDto) => void
}) {
  const sourceNode = definition.nodes.find((node) => node.id === edge.fromNodeId) ?? null
  const producedArtifactRef = sourceNode ? producedArtifactRefForNode(sourceNode) : null
  const firstDecision =
    sourceNode?.type === 'human_checkpoint'
      ? sourceNode.decisionOptions[0] ?? 'continue'
      : 'continue'
  const recipes: Array<{
    key: string
    label: string
    description: string
    disabled?: boolean
    apply: () => void
  }> = [
    {
      key: 'success',
      label: 'After Success',
      description: 'Normal happy path.',
      apply: () =>
        onUpdate((current) => ({
          ...current,
          type: 'success',
          priority: 10,
          condition: { kind: 'always' },
          loopPolicy: null,
        })),
    },
    {
      key: 'failure',
      label: 'On Failure',
      description: 'Escalate, debug, or stop.',
      apply: () =>
        onUpdate((current) => ({
          ...current,
          type: 'failure',
          priority: 10,
          condition: { kind: 'always' },
          loopPolicy: null,
        })),
    },
    {
      key: 'artifact',
      label: 'If Handoff Exists',
      description: producedArtifactRef
        ? `Checks ${handoffLabel(producedArtifactRef.split('.')[1] ?? producedArtifactRef)}.`
        : 'Source node does not create a handoff.',
      disabled: !producedArtifactRef,
      apply: () => {
        if (!producedArtifactRef) return
        onUpdate((current) => ({
          ...current,
          type: 'conditional',
          priority: 20,
          condition: { kind: 'artifact_exists', artifactRef: producedArtifactRef },
          loopPolicy: null,
        }))
      },
    },
    {
      key: 'manual',
      label: 'Human Decision',
      description: 'Match a checkpoint button.',
      disabled: sourceNode?.type !== 'human_checkpoint',
      apply: () =>
        onUpdate((current) => ({
          ...current,
          type: 'manual_override',
          priority: 20,
          condition: {
            kind: 'human_decision_is',
            checkpointNodeId: current.fromNodeId,
            decision: firstDecision,
          },
          loopPolicy: null,
        })),
    },
    {
      key: 'loop',
      label: 'Loop Limit',
      description: 'Retry with a hard cap.',
      apply: () =>
        onUpdate((current) => ({
          ...current,
          type: 'loop',
          priority: 30,
          condition: current.condition,
          loopPolicy: current.loopPolicy ?? defaultWorkflowLoopPolicy(current),
        })),
    },
  ]

  return (
    <section className="space-y-2">
      <div className="space-y-0.5">
        <h3 className="text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
          Quick path recipes
        </h3>
        <p className="text-[10.5px] leading-relaxed text-muted-foreground/75">
          Pick a recipe to fill the connection type, condition, and retry policy.
        </p>
      </div>
      <div className="grid grid-cols-2 gap-1.5">
        {recipes.map((recipe) => (
          <Button
            key={recipe.key}
            type="button"
            variant="outline"
            size="sm"
            className="h-auto min-h-9 justify-start px-2 py-1.5 text-left text-[10.5px]"
            disabled={recipe.disabled}
            title={recipe.description}
            onClick={recipe.apply}
          >
            <span className="min-w-0">
              <span className="block truncate font-medium">{recipe.label}</span>
              <span className="block truncate text-[9.5px] font-normal text-muted-foreground">
                {recipe.description}
              </span>
            </span>
          </Button>
        ))}
      </div>
    </section>
  )
}

function EdgeEditor({
  definition,
  edge,
  onUpdate,
}: {
  definition: WorkflowDefinitionDto
  edge: WorkflowEdgeDto
  onUpdate: (updater: (edge: WorkflowEdgeDto) => WorkflowEdgeDto) => void
}) {
  return (
    <>
      <EdgeRecipePicker definition={definition} edge={edge} onUpdate={onUpdate} />
      <Field label="Path label" hint="Short text shown on the connection line.">
        <Input
          value={edge.label}
          onChange={(event) => onUpdate((current) => ({ ...current, label: event.target.value }))}
          className="h-8 text-[12px]"
        />
      </Field>
      <Field label="When this path runs" hint={WORKFLOW_EDGE_HELP[edge.type].oneLine}>
        <Select
          value={edge.type}
          onValueChange={(value) =>
            onUpdate((current) => ({
              ...current,
              type: value as WorkflowEdgeTypeDto,
              loopPolicy:
                value === 'loop'
                  ? current.loopPolicy ?? defaultWorkflowLoopPolicy(current)
                  : null,
            }))
          }
        >
          <SelectTrigger className="h-8 text-[12px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {Object.entries(EDGE_TYPE_LABEL).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <Field label="Priority" hint="Lower numbers win when multiple outgoing paths match.">
        <Input
          type="number"
          value={edge.priority}
          onChange={(event) => onUpdate((current) => ({ ...current, priority: Number(event.target.value) || 0 }))}
          className="h-8 text-[12px]"
        />
      </Field>
      <ConditionEditor edge={edge} onUpdate={onUpdate} />
      {edge.loopPolicy ? (
        <>
          <Field label="Loop key" hint="Shared counter name for this bounded loop.">
            <Input
              value={edge.loopPolicy.loopKey}
              onChange={(event) =>
                onUpdate((current) =>
                  current.loopPolicy
                    ? { ...current, loopPolicy: { ...current.loopPolicy, loopKey: event.target.value } }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Max attempts" hint="Hard stop before routing to the exhausted path.">
            <Input
              type="number"
              min={1}
              value={edge.loopPolicy.maxAttempts}
              onChange={(event) =>
                onUpdate((current) =>
                  current.loopPolicy
                    ? {
                        ...current,
                        loopPolicy: {
                          ...current.loopPolicy,
                          maxAttempts: Math.max(1, Number(event.target.value) || 1),
                        },
                      }
                    : current,
                )
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="When exhausted">
            <Select
              value={edge.loopPolicy.onExhausted}
              onValueChange={(value) =>
                onUpdate((current) =>
                  current.loopPolicy
                    ? { ...current, loopPolicy: { ...current.loopPolicy, onExhausted: value } }
                    : current,
                )
              }
            >
              <SelectTrigger className="h-8 text-[12px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {definition.nodes.map((node) => (
                  <SelectItem key={node.id} value={node.id}>
                    {node.title}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
        </>
      ) : null}
    </>
  )
}

function ConditionEditor({
  edge,
  onUpdate,
}: {
  edge: WorkflowEdgeDto
  onUpdate: (updater: (edge: WorkflowEdgeDto) => WorkflowEdgeDto) => void
}) {
  const condition = edge.condition
  const conditionKind = supportedConditionKind(condition)
  const setCondition = (next: WorkflowConditionDto) => {
    onUpdate((current) => ({ ...current, condition: next }))
  }
  return (
    <>
      <Field label="Condition" hint={workflowConditionPlainSummary(condition)}>
        <Select
          value={conditionKind}
          onValueChange={(value) => setCondition(defaultCondition(value))}
        >
          <SelectTrigger className="h-8 text-[12px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="always">Always</SelectItem>
            <SelectItem value="artifact_exists">Artifact exists</SelectItem>
            <SelectItem value="artifact_field_equals">Artifact field equals</SelectItem>
            <SelectItem value="artifact_field_number_compare">Number comparison</SelectItem>
            <SelectItem value="state_field_equals">State field equals</SelectItem>
            <SelectItem value="state_collection_count_compare">State count comparison</SelectItem>
            <SelectItem value="human_decision_is">Human decision</SelectItem>
          </SelectContent>
        </Select>
      </Field>
      {'stateRef' in condition ? (
        <Field label="State handoff">
          <Input
            value={condition.stateRef}
            onChange={(event) =>
              setCondition({ ...condition, stateRef: event.target.value } as WorkflowConditionDto)
            }
            className="h-8 text-[12px]"
          />
        </Field>
      ) : null}
      {'artifactRef' in condition ? (
        <Field label="Handoff">
          <Input
            value={condition.artifactRef}
            onChange={(event) =>
              setCondition({ ...condition, artifactRef: event.target.value } as WorkflowConditionDto)
            }
            className="h-8 text-[12px]"
          />
        </Field>
      ) : null}
      {'path' in condition ? (
        <Field label="Field path">
          <Input
            value={condition.path}
            onChange={(event) =>
              setCondition({ ...condition, path: event.target.value } as WorkflowConditionDto)
            }
            className="h-8 text-[12px]"
          />
        </Field>
      ) : null}
      {condition.kind === 'artifact_field_equals' ? (
        <Field label="Compare to">
          <Input
            value={String(condition.value ?? '')}
            onChange={(event) => setCondition({ ...condition, value: event.target.value })}
            className="h-8 text-[12px]"
          />
        </Field>
      ) : null}
      {condition.kind === 'state_field_equals' ? (
        <Field label="Compare to">
          <Input
            value={String(condition.value ?? '')}
            onChange={(event) => setCondition({ ...condition, value: event.target.value })}
            className="h-8 text-[12px]"
          />
        </Field>
      ) : null}
      {condition.kind === 'artifact_field_number_compare' || condition.kind === 'state_collection_count_compare' ? (
        <>
          <Field label="Operator">
            <Select
              value={condition.operator}
              onValueChange={(value) =>
                setCondition({
                  ...condition,
                  operator: value as typeof condition.operator,
                })
              }
            >
              <SelectTrigger className="h-8 text-[12px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {['eq', 'neq', 'gt', 'gte', 'lt', 'lte'].map((operator) => (
                  <SelectItem key={operator} value={operator}>
                    {operator}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label="Number">
            <Input
              type="number"
              value={condition.value}
              onChange={(event) =>
                setCondition({ ...condition, value: Number(event.target.value) || 0 })
              }
              className="h-8 text-[12px]"
            />
          </Field>
        </>
      ) : null}
      {condition.kind === 'human_decision_is' ? (
        <>
          <Field label="Checkpoint">
            <Input
              value={condition.checkpointNodeId}
              onChange={(event) =>
                setCondition({ ...condition, checkpointNodeId: event.target.value })
              }
              className="h-8 text-[12px]"
            />
          </Field>
          <Field label="Decision">
            <Input
              value={condition.decision}
              onChange={(event) => setCondition({ ...condition, decision: event.target.value })}
              className="h-8 text-[12px]"
            />
          </Field>
        </>
      ) : null}
    </>
  )
}

function WorkflowGuidanceCard({
  definition,
  node,
  edge,
  agentLabel,
}: {
  definition: WorkflowDefinitionDto
  node: WorkflowNodeDto | null
  edge: WorkflowEdgeDto | null
  agentLabel: string | null
}) {
  if (!node && !edge) return null
  const help = node ? WORKFLOW_NODE_HELP[node.type] : edge ? WORKFLOW_EDGE_HELP[edge.type] : null
  if (!help) return null
  const summary = node
    ? workflowNodePlainSummary(node, agentLabel)
    : edge
      ? workflowEdgePlainSummary(edge, definition)
      : ''
  const advanced = node ? WORKFLOW_NODE_HELP[node.type].advanced : edge ? ['Condition', 'priority', 'loop policy'] : []
  const setup = node ? WORKFLOW_NODE_HELP[node.type].setup : edge ? [WORKFLOW_EDGE_HELP[edge.type].useWhen] : []

  return (
    <section className="rounded-md border border-primary/20 bg-primary/[0.045] p-2.5">
      <div className="flex items-start gap-2">
        <span className="mt-0.5 inline-flex size-5 shrink-0 items-center justify-center rounded bg-primary/10 text-primary">
          <InfoIcon />
        </span>
        <div className="min-w-0 flex-1 space-y-2">
          <div className="space-y-0.5">
            <p className="text-[11.5px] font-semibold text-foreground">{help.label}</p>
            <p
              className={cn(
                'text-[11px] leading-relaxed text-muted-foreground',
                WORKFLOW_PANEL_TEXT_WRAP_CLASS,
              )}
            >
              {summary}
            </p>
          </div>
          <Tabs defaultValue="guide" className="gap-2">
            <TabsList className="h-7 rounded-md bg-background/45 p-0.5">
              <TabsTrigger value="guide" className="h-6 px-2 text-[10.5px]">
                Guide
              </TabsTrigger>
              <TabsTrigger value="terms" className="h-6 px-2 text-[10.5px]">
                Terms
              </TabsTrigger>
            </TabsList>
            <TabsContent value="guide" className="mt-0 space-y-1.5">
              <p
                className={cn(
                  'text-[10.5px] leading-relaxed text-muted-foreground',
                  WORKFLOW_PANEL_TEXT_WRAP_CLASS,
                )}
              >
                {help.useWhen}
              </p>
              {setup.length > 0 ? (
                <ul className="space-y-1">
                  {setup.map((item) => (
                    <li key={item} className="flex gap-1.5 text-[10.5px] leading-relaxed text-foreground/85">
                      <CheckCircle2 className="mt-0.5 size-3 shrink-0 text-primary" aria-hidden="true" />
                      <span className={WORKFLOW_PANEL_TEXT_WRAP_CLASS}>{item}</span>
                    </li>
                  ))}
                </ul>
              ) : null}
            </TabsContent>
            <TabsContent value="terms" className="mt-0">
              <p
                className={cn(
                  'text-[10.5px] leading-relaxed text-muted-foreground',
                  WORKFLOW_PANEL_TEXT_WRAP_CLASS,
                )}
              >
                Advanced fields: {advanced.join(', ') || 'none'}.
              </p>
            </TabsContent>
          </Tabs>
        </div>
      </div>
    </section>
  )
}

function InfoIcon() {
  return <AlertCircle className="size-3" aria-hidden="true" />
}

function WorkflowRunRecoveryPanel({
  run,
  result,
  runningAction,
  recoveryAction,
  onExplainRunBlocker,
  onExportRunBundle,
  onResumeNextIncompletePhase,
}: {
  run: WorkflowRunDto
  result: WorkflowRecoveryResult | null
  runningAction: boolean
  recoveryAction: 'blocker' | 'bundle' | 'resume' | null
  onExplainRunBlocker: (() => void) | null
  onExportRunBundle: (() => void) | null
  onResumeNextIncompletePhase: (() => void) | null
}) {
  const recoverableNodeCount = run.nodes.filter((node) => isRetryableRunNodeStatus(node.status)).length
  return (
    <section className="agent-properties-panel pointer-events-auto absolute right-5 top-14 z-30 flex w-[320px] flex-col gap-2 rounded-lg border border-border/60 bg-card/95 px-3 py-3 text-[12px] text-card-foreground shadow-[0_8px_28px_-12px_rgba(0,0,0,0.55)] backdrop-blur-md">
      <div className="flex items-center gap-2">
        <span className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded bg-amber-500/10 text-amber-600 dark:text-amber-300">
          <AlertCircle className="h-3 w-3" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate text-[12px] font-semibold leading-none text-foreground">
            Run recovery
          </p>
          <p className="mt-1 truncate text-[10.5px] text-muted-foreground">
            {humanize(run.status)}
            {recoverableNodeCount > 0 ? `, ${recoverableNodeCount} recoverable node${recoverableNodeCount === 1 ? '' : 's'}` : ''}
          </p>
        </div>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {onExplainRunBlocker ? (
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={runningAction}
            className="h-7 px-2 text-[11px]"
            onClick={onExplainRunBlocker}
          >
            {recoveryAction === 'blocker' ? <Loader2 className="size-3 animate-spin" /> : <AlertCircle className="size-3" />}
            Explain
          </Button>
        ) : null}
        {onExportRunBundle ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={runningAction}
            className="h-7 px-2 text-[11px]"
            onClick={onExportRunBundle}
          >
            {recoveryAction === 'bundle' ? <Loader2 className="size-3 animate-spin" /> : <FileText className="size-3" />}
            Bundle
          </Button>
        ) : null}
        {onResumeNextIncompletePhase ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={runningAction}
            className="h-7 px-2 text-[11px]"
            onClick={onResumeNextIncompletePhase}
          >
            {recoveryAction === 'resume' ? <Loader2 className="size-3 animate-spin" /> : <RotateCcw className="size-3" />}
            Resume phase
          </Button>
        ) : null}
      </div>
      {result ? (
        <div className="rounded-md border border-border/45 bg-background/45 px-2 py-2">
          <div className="mb-1 flex items-center justify-between gap-2">
            <p className="truncate text-[11px] font-medium text-foreground">{result.title}</p>
            <Badge variant="outline" className="shrink-0 rounded px-1.5 py-0 text-[9.5px]">
              {humanize(result.kind)}
            </Badge>
          </div>
          <p
            className={cn(
              'line-clamp-2 text-[10.5px] text-muted-foreground',
              WORKFLOW_PANEL_TEXT_WRAP_CLASS,
            )}
          >
            {result.summary}
          </p>
          <pre className="mt-2 max-h-44 overflow-auto rounded border border-border/35 bg-muted/25 p-2 text-[10px] leading-relaxed text-foreground/80">
            {result.detail}
          </pre>
        </div>
      ) : null}
    </section>
  )
}

function WorkflowDetailsPanel({
  definition,
  node,
  edge,
  runNode,
  artifact,
  edgeDecision,
  events,
  subgraphChildRuns,
  artifactByNodeRunId,
  agentLabel,
  running,
  onRetryNodeRun,
  onSkipBranch,
  onClose,
}: {
  definition: WorkflowDefinitionDto
  node: WorkflowNodeDto | null
  edge: WorkflowEdgeDto | null
  runNode: WorkflowRunNodeDto | null
  artifact: WorkflowArtifactRecordDto | null
  edgeDecision: WorkflowRunEdgeDecisionDto | null
  events: readonly WorkflowEventDto[]
  subgraphChildRuns: readonly WorkflowRunNodeDto[]
  artifactByNodeRunId: ReadonlyMap<string, WorkflowArtifactRecordDto>
  agentLabel: string | null
  running: boolean
  onRetryNodeRun: ((nodeRunId: string) => void) | null
  onSkipBranch: ((nodeRunId: string) => void) | null
  onClose: () => void
}) {
  const canRetry = runNode ? isRetryableRunNodeStatus(runNode.status) && onRetryNodeRun : false
  const canSkip = runNode ? isSkippableRunNodeStatus(runNode.status) && onSkipBranch : false
  const timeline = workflowTimelineEvents(events, runNode, edge).slice(0, 6)
  const nodeById = useMemo(
    () => new Map(definition.nodes.map((definitionNode) => [definitionNode.id, definitionNode])),
    [definition.nodes],
  )
  const incomingEdges = useMemo(
    () => node ? workflowSortedEdges(definition.edges.filter((candidate) => candidate.toNodeId === node.id)) : [],
    [definition.edges, node],
  )
  const outgoingEdges = useMemo(
    () => node ? workflowSortedEdges(definition.edges.filter((candidate) => candidate.fromNodeId === node.id)) : [],
    [definition.edges, node],
  )
  const fromNode = edge ? nodeById.get(edge.fromNodeId) ?? null : null
  const toNode = edge ? nodeById.get(edge.toNodeId) ?? null : null
  return (
    <div
      className="agent-properties-panel pointer-events-auto absolute bottom-4 left-5 z-30 flex max-h-[calc(100%-5rem)] w-[360px] max-w-[calc(100%-2.5rem)] flex-col overflow-hidden rounded-lg border border-border/60 bg-card/95 text-[12px] text-card-foreground shadow-[0_8px_28px_-12px_rgba(0,0,0,0.55)] backdrop-blur-md"
      onPointerDown={(event) => event.stopPropagation()}
      onWheel={(event) => event.stopPropagation()}
    >
      <header className="flex items-center gap-2 border-b border-border/50 px-3 py-1.5">
        <span className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded bg-primary/10 text-primary">
          {node ? <Workflow className="h-3 w-3" /> : <GitBranch className="h-3 w-3" />}
        </span>
        <p className="min-w-0 flex-1 truncate text-[12px] font-semibold leading-none text-foreground">
          {node?.title ?? edge?.label ?? edge?.id}
        </p>
        <Button type="button" size="icon-sm" variant="ghost" onClick={onClose} className="size-5 text-muted-foreground hover:text-foreground" aria-label="Close details">
          <X className="h-3 w-3" />
        </Button>
      </header>
      <div className="min-h-0 space-y-4 overflow-y-auto px-3 py-3">
        {node ? (
          <>
            <ReadOnlyRow label="Summary" value={workflowNodePlainSummary(node, agentLabel)} />
            <ReadOnlyRow label="Type" value={WORKFLOW_NODE_HELP[node.type].label} />
            <ReadOnlyRow label="Node ID" value={node.id} />
            {node.description.trim() ? <ReadOnlyRow label="Description" value={node.description} /> : null}
            {agentLabel ? <ReadOnlyRow label="Agent" value={agentLabel} /> : null}
            <WorkflowNodeReadOnlyDetails node={node} agentLabel={agentLabel} />
            <WorkflowConnectionSummary
              incomingEdges={incomingEdges}
              outgoingEdges={outgoingEdges}
              nodeById={nodeById}
            />
            {runNode ? <ReadOnlyRow label="Status" value={humanize(runNode.status)} /> : null}
            {runNode?.failureClass ? <ReadOnlyRow label="Failure" value={runNode.failureClass} /> : null}
            {runNode && (canRetry || canSkip) ? (
              <section className="space-y-1.5">
                <h3 className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                  Operations
                </h3>
                <div className="flex flex-wrap gap-1.5">
                  {canRetry ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      disabled={running}
                      className="h-7 px-2 text-[11px]"
                      onClick={() => onRetryNodeRun?.(runNode.id)}
                    >
                      {running ? <Loader2 className="size-3 animate-spin" /> : <RotateCcw className="size-3" />}
                      Retry
                    </Button>
                  ) : null}
                  {canSkip ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={running}
                      className="h-7 px-2 text-[11px]"
                      onClick={() => onSkipBranch?.(runNode.id)}
                    >
                      {running ? <Loader2 className="size-3 animate-spin" /> : <SkipForward className="size-3" />}
                      Skip
                    </Button>
                  ) : null}
                </div>
              </section>
            ) : null}
            {artifact ? (
              <section className="space-y-1.5">
                <h3 className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                  Artifact
                </h3>
                <div className="rounded-md border border-border/45 bg-background/45 px-2 py-2 text-[11.5px] leading-relaxed">
                  <p className="font-medium text-foreground">{artifact.artifactType} v{artifact.schemaVersion}</p>
                  <p
                    className={cn(
                      'mt-1 max-h-28 overflow-hidden text-muted-foreground [white-space:pre-wrap]',
                      WORKFLOW_PANEL_TEXT_WRAP_CLASS,
                    )}
                  >
                    {artifact.renderText ?? summarizeJson(artifact.payload)}
                  </p>
                </div>
              </section>
            ) : null}
            {node.type === 'subgraph' && subgraphChildRuns.length > 0 ? (
              <section className="space-y-1.5">
                <h3 className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                  Subgraph Runs
                </h3>
                <ol className="space-y-1.5">
                  {subgraphChildRuns.map((childRun) => {
                    const childArtifact = artifactByNodeRunId.get(childRun.id)
                    return (
                      <li
                        key={childRun.id}
                        className="rounded-md border border-border/45 bg-background/35 px-2 py-1.5"
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="min-w-0 truncate text-[11px] font-medium text-foreground/90">
                            {localSubgraphNodeId(node.id, childRun.nodeId)}
                          </span>
                          <Badge variant={runNodeStatusBadgeVariant(childRun.status)} className="shrink-0 rounded px-1.5 py-0 text-[9.5px]">
                            {humanize(childRun.status)}
                          </Badge>
                        </div>
                        {childRun.failureClass ? (
                          <p className="mt-1 truncate text-[10.5px] text-destructive">
                            {childRun.failureClass}
                          </p>
                        ) : null}
                        {childArtifact ? (
                          <p
                            className={cn(
                              'mt-1 line-clamp-2 text-[10.5px] text-muted-foreground',
                              WORKFLOW_PANEL_TEXT_WRAP_CLASS,
                            )}
                          >
                            {childArtifact.renderText ?? summarizeJson(childArtifact.payload)}
                          </p>
                        ) : null}
                      </li>
                    )
                  })}
                </ol>
              </section>
            ) : null}
            {timeline.length > 0 ? <WorkflowEventTimeline events={timeline} /> : null}
          </>
        ) : edge ? (
          <>
            <ReadOnlyRow label="Summary" value={workflowEdgePlainSummary(edge, definition)} />
            <ReadOnlyRow label="Type" value={WORKFLOW_EDGE_HELP[edge.type].label} />
            <ReadOnlyRow label="Edge ID" value={edge.id} />
            <ReadOnlyRow label="From" value={fromNode ? `${fromNode.title} (${edge.fromNodeId})` : edge.fromNodeId} />
            <ReadOnlyRow label="To" value={toNode ? `${toNode.title} (${edge.toNodeId})` : edge.toNodeId} />
            {edge.label ? <ReadOnlyRow label="Label" value={edge.label} /> : null}
            <ReadOnlyRow label="Priority" value={String(edge.priority)} />
            <ReadOnlyRow label="Condition" value={conditionSummary(edge.condition)} />
            {edge.loopPolicy ? <ReadOnlyRow label="Loop" value={loopPolicySummary(edge.loopPolicy)} /> : null}
            {edgeDecision ? (
              <section className="space-y-1.5">
                <h3 className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                  Decision evidence
                </h3>
                <pre className="max-h-40 overflow-auto rounded-md border border-border/45 bg-background/45 p-2 text-[10.5px] text-foreground/80">
                  {JSON.stringify(edgeDecision.evidence, null, 2)}
                </pre>
              </section>
            ) : null}
            {timeline.length > 0 ? <WorkflowEventTimeline events={timeline} /> : null}
          </>
        ) : null}
      </div>
    </div>
  )
}

function WorkflowNodeReadOnlyDetails({
  node,
  agentLabel,
}: {
  node: WorkflowNodeDto
  agentLabel: string | null
}) {
  if (node.type === 'agent') {
    return (
      <>
        {agentLabel ? null : <ReadOnlyRow label="Agent" value={labelForAgentRef(node.agentRef, [])} />}
        <ReadOnlyRow label="Handoff" value={outputContractSummary(node.outputContract)} />
        <ReadOnlyRow label="Inputs" value={inputBindingsSummary(node.inputBindings)} />
        {node.resourceScopes.length > 0 ? <ReadOnlyRow label="Resource scopes" value={node.resourceScopes.join(', ')} /> : null}
        {node.runOverrides ? <ReadOnlyRow label="Run overrides" value={runOverrideSummary(node.runOverrides)} /> : null}
      </>
    )
  }

  if (node.type === 'state_read' || node.type === 'state_query') {
    return (
      <>
        <ReadOnlyRow label="Entity" value={humanize(node.query.entityType)} />
        <ReadOnlyRow label="Filters" value={stateFiltersSummary(node.query.filters)} />
        <ReadOnlyRow label="Order" value={stateQueryOrderSummary(node.query)} />
        <ReadOnlyRow label="Handoff" value={handoffLabel(node.outputArtifactType)} />
      </>
    )
  }

  if (node.type === 'state_write' || node.type === 'state_patch') {
    return (
      <>
        <ReadOnlyRow label="State action" value={`${humanize(node.operation.action)} ${humanize(node.operation.entityType)}`} />
        <ReadOnlyRow label="Target" value={node.operation.targetId ?? 'Created from payload'} />
        {node.operation.idempotencyKey ? <ReadOnlyRow label="Idempotency key" value={node.operation.idempotencyKey} /> : null}
        <ReadOnlyRow label="Payload" value={jsonPreview(node.operation.payload)} />
        <ReadOnlyRow label="Handoff" value={handoffLabel(node.operation.outputArtifactType)} />
        {node.inputBindings.length > 0 ? <ReadOnlyRow label="Inputs" value={inputBindingsSummary(node.inputBindings)} /> : null}
      </>
    )
  }

  if (node.type === 'collection_loop') {
    return (
      <>
        <ReadOnlyRow label="Collection" value={`${humanize(node.collection.entityType)}, max ${node.maxItemCount}`} />
        <ReadOnlyRow label="Filters" value={stateFiltersSummary(node.collection.filters)} />
        <ReadOnlyRow label="Sort" value={node.sortKey ?? node.collection.orderBy ?? 'Insertion order'} />
        <ReadOnlyRow label="Window controls" value={loopControlsSummary(node.controls)} />
        <ReadOnlyRow label="Item handoff" value={`${handoffLabel(node.itemArtifactType)} as ${node.itemVariableName}`} />
        <ReadOnlyRow label="After item" value={node.afterItemRequery ? 'Requery collection before continuing' : 'Continue without requery'} />
        {node.maxRuntimeSeconds ? <ReadOnlyRow label="Max runtime" value={`${node.maxRuntimeSeconds}s`} /> : null}
      </>
    )
  }

  if (node.type === 'command') {
    return (
      <>
        <ReadOnlyRow label="Command" value={[node.command, ...node.args].join(' ')} />
        <ReadOnlyRow label="Allowed commands" value={node.allowedCommands.length > 0 ? node.allowedCommands.join(', ') : node.command} />
        <ReadOnlyRow label="Working directory" value={node.workingDirectory ?? 'Project workspace'} />
        <ReadOnlyRow label="Timeout" value={`${node.timeoutSeconds}s`} />
        <ReadOnlyRow label="Success exits" value={node.successExitCodes.join(', ')} />
        <ReadOnlyRow label="Handoff" value={outputContractSummary(node.outputContract)} />
      </>
    )
  }

  if (node.type === 'human_checkpoint') {
    return (
      <>
        <ReadOnlyRow label="Checkpoint" value={humanize(node.checkpointType)} />
        <ReadOnlyRow label="Prompt" value={node.prompt} />
        <ReadOnlyRow label="Decisions" value={node.decisionOptions.join(', ') || 'continue'} />
        {node.stateUpdates.length > 0 ? <ReadOnlyRow label="State updates" value={stateWriteOperationsSummary(node.stateUpdates)} /> : null}
      </>
    )
  }

  if (node.type === 'gate' || node.type === 'state_checkpoint') {
    return (
      <>
        <ReadOnlyRow label="Required checks" value={conditionsSummary(node.requiredChecks)} />
        <ReadOnlyRow label="Blocked behavior" value={humanize(node.onBlocked)} />
      </>
    )
  }

  if (node.type === 'merge') {
    return (
      <>
        <ReadOnlyRow label="Wait policy" value={humanize(node.waitPolicy)} />
        {node.quorum ? <ReadOnlyRow label="Quorum" value={String(node.quorum)} /> : null}
        <ReadOnlyRow label="Fail fast" value={node.failFast ? 'Yes' : 'No'} />
      </>
    )
  }

  if (node.type === 'terminal') {
    return <ReadOnlyRow label="Terminal status" value={humanize(node.terminalStatus)} />
  }

  if (node.type === 'subgraph') {
    return (
      <>
        <ReadOnlyRow label="Subgraph" value={node.subgraphId} />
        <ReadOnlyRow label="Inputs" value={inputBindingsSummary(node.inputBindings)} />
        <ReadOnlyRow label="Handoff" value={outputContractSummary(node.outputContract)} />
      </>
    )
  }

  return null
}

function WorkflowConnectionSummary({
  incomingEdges,
  outgoingEdges,
  nodeById,
}: {
  incomingEdges: readonly WorkflowEdgeDto[]
  outgoingEdges: readonly WorkflowEdgeDto[]
  nodeById: ReadonlyMap<string, WorkflowNodeDto>
}) {
  if (incomingEdges.length === 0 && outgoingEdges.length === 0) return null
  return (
    <section className="space-y-2">
      <h3 className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
        Connections
      </h3>
      <div className="grid gap-2">
        {incomingEdges.length > 0 ? (
          <WorkflowConnectionList
            label="Incoming"
            items={incomingEdges.map((connection) => incomingEdgeSummary(connection, nodeById))}
          />
        ) : null}
        {outgoingEdges.length > 0 ? (
          <WorkflowConnectionList
            label="Outgoing"
            items={outgoingEdges.map((connection) => outgoingEdgeSummary(connection, nodeById))}
          />
        ) : null}
      </div>
    </section>
  )
}

function WorkflowConnectionList({ label, items }: { label: string; items: readonly string[] }) {
  return (
    <div className="space-y-1">
      <p className="text-[10.5px] font-medium text-foreground/80">{label}</p>
      <ul className="space-y-1">
        {items.map((item) => (
          <li
            key={item}
            className={cn(
              'rounded-md border border-border/45 bg-background/35 px-2 py-1.5 text-[11px] leading-relaxed text-muted-foreground',
              WORKFLOW_PANEL_TEXT_WRAP_CLASS,
            )}
          >
            {item}
          </li>
        ))}
      </ul>
    </div>
  )
}

function WorkflowEventTimeline({ events }: { events: readonly WorkflowEventDto[] }) {
  return (
    <section className="space-y-1.5">
      <h3 className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
        Timeline
      </h3>
      <ol className="space-y-1.5">
        {events.map((event) => (
          <li
            key={event.id}
            className="rounded-md border border-border/45 bg-background/35 px-2 py-1.5"
          >
            <div className="flex items-center justify-between gap-2">
              <span className="min-w-0 truncate text-[11px] font-medium text-foreground/90">
                {humanize(event.eventType)}
              </span>
              <time className="shrink-0 text-[9.5px] text-muted-foreground">
                {compactTime(event.createdAt)}
              </time>
            </div>
            <p
              className={cn(
                'mt-0.5 line-clamp-2 text-[10.5px] text-muted-foreground',
                WORKFLOW_PANEL_TEXT_WRAP_CLASS,
              )}
            >
              {timelineEventSummary(event)}
            </p>
          </li>
        ))}
      </ol>
    </section>
  )
}

function WorkflowStartPreview({ preview }: { preview: WorkflowRunPreview }) {
  const rows = [
    { label: 'State reads', values: preview.stateReads },
    { label: 'State writes', values: preview.stateWrites },
    { label: 'Loops', values: preview.loops },
    { label: 'Commands', values: preview.commands },
    { label: 'Checkpoints', values: preview.checkpoints },
    { label: 'Terminals', values: preview.terminals },
  ].filter((row) => row.values.length > 0)
  if (rows.length === 0) return null
  return (
    <section className="space-y-2 rounded-lg border border-border/55 bg-muted/25 px-3 py-3">
      <h3 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
        Run Preview
      </h3>
      <div className="space-y-2">
        {rows.map((row) => (
          <div key={row.label} className="space-y-1">
            <p className="text-[11px] font-medium text-foreground/80">{row.label}</p>
            <div className="flex flex-wrap gap-1.5">
              {row.values.slice(0, 6).map((value) => (
                <Badge key={`${row.label}:${value}`} variant="outline" className="max-w-full truncate rounded px-1.5 py-0 text-[10px]">
                  {value}
                </Badge>
              ))}
              {row.values.length > 6 ? (
                <Badge variant="secondary" className="rounded px-1.5 py-0 text-[10px]">
                  +{row.values.length - 6}
                </Badge>
              ) : null}
            </div>
          </div>
        ))}
      </div>
    </section>
  )
}

function CheckpointResumeBar({
  node,
  running,
  onResume,
}: {
  node: WorkflowNodeDto | null
  running: boolean
  onResume: (decision: string) => void
}) {
  const options = node?.type === 'human_checkpoint' && node.decisionOptions.length > 0
    ? node.decisionOptions
    : ['continue']
  return (
    <div className="pointer-events-auto absolute bottom-5 right-5 z-30 flex max-w-md items-center gap-2 rounded-lg border border-amber-500/25 bg-card/95 px-3 py-2 text-[12px] shadow-lg backdrop-blur-md">
      <PauseCircle className="size-4 shrink-0 text-amber-500" />
      <p className="min-w-0 flex-1 truncate text-muted-foreground">
        {node?.type === 'human_checkpoint' ? node.prompt : 'Workflow is paused at a gate.'}
      </p>
      {options.map((option) => (
        <Button key={option} type="button" size="sm" className="h-7 text-[11px]" disabled={running} onClick={() => onResume(option)}>
          {running ? <Loader2 className="size-3 animate-spin" /> : null}
          {humanize(option)}
        </Button>
      ))}
    </div>
  )
}

function Field({
  label,
  hint,
  children,
}: {
  label: string
  hint?: ReactNode
  children: ReactNode
}) {
  return (
    <div className="space-y-1.5">
      <div className="space-y-0.5">
        <Label className="text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
          {label}
        </Label>
        {hint ? (
          <p
            className={cn(
              'text-[10.5px] leading-relaxed text-muted-foreground/75',
              WORKFLOW_PANEL_TEXT_WRAP_CLASS,
            )}
          >
            {hint}
          </p>
        ) : null}
      </div>
      {children}
    </div>
  )
}

function ReadOnlyRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="space-y-0.5">
      <dt className="text-[9.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
        {label}
      </dt>
      <dd
        className={cn(
          'whitespace-pre-wrap text-[12px] leading-relaxed text-foreground/90',
          WORKFLOW_PANEL_TEXT_WRAP_CLASS,
        )}
      >
        {value}
      </dd>
    </div>
  )
}

function workflowSortedEdges(edges: readonly WorkflowEdgeDto[]): WorkflowEdgeDto[] {
  return [...edges].sort((left, right) => {
    const byPriority = left.priority - right.priority
    return byPriority === 0 ? left.id.localeCompare(right.id) : byPriority
  })
}

function incomingEdgeSummary(edge: WorkflowEdgeDto, nodeById: ReadonlyMap<string, WorkflowNodeDto>): string {
  const source = nodeById.get(edge.fromNodeId)
  return `${source?.title ?? edge.fromNodeId} -> ${edge.label || EDGE_TYPE_LABEL[edge.type]}`
}

function outgoingEdgeSummary(edge: WorkflowEdgeDto, nodeById: ReadonlyMap<string, WorkflowNodeDto>): string {
  const target = nodeById.get(edge.toNodeId)
  return `${edge.label || EDGE_TYPE_LABEL[edge.type]} -> ${target?.title ?? edge.toNodeId}`
}

function outputContractSummary(contract: WorkflowOutputContractLike): string {
  const extraction = humanize(contract.extraction)
  const required = contract.required ? 'required' : 'optional'
  const renderText = contract.renderTextPath ? `, render ${contract.renderTextPath}` : ''
  return `${handoffLabel(contract.artifactType)} (${contract.artifactType}) v${contract.schemaVersion}, ${extraction}, ${required}${renderText}`
}

type WorkflowOutputContractLike = {
  artifactType: string
  schemaVersion: number
  extraction: WorkflowOutputExtractionDto
  required: boolean
  renderTextPath?: string | null
}

function inputBindingsSummary(bindings: readonly WorkflowInputBindingDto[]): string {
  if (bindings.length === 0) return 'None'
  return bindings.map(inputBindingSummary).join('\n')
}

function inputBindingSummary(binding: WorkflowInputBindingDto): string {
  const plain = workflowInputBindingPlainSummary(binding)
  const label = binding.promptLabel ?? humanize(binding.name)
  const required = binding.required ? 'required' : 'optional'
  if (binding.source === 'run_input') {
    const path = binding.path ? ` ${binding.path}` : ''
    return `${plain}\n${label} <- run input ${binding.name}${path} (${required})`
  }
  if (binding.source === 'artifact') {
    const path = binding.path ? ` ${binding.path}` : ''
    return `${plain}\n${label} <- artifact ${binding.artifactRef}${path} (${required})`
  }
  const path = binding.path ? ` ${binding.path}` : ''
  return `${plain}\n${label} <- state ${binding.stateRef}${path} (${required})`
}

function stateFiltersSummary(filters: readonly WorkflowStateQueryFilterDto[]): string {
  if (filters.length === 0) return 'None'
  return filters.map(stateFilterSummary).join('\n')
}

function stateFilterSummary(filter: WorkflowStateQueryFilterDto): string {
  const plain = workflowStateFilterPlainSummary(filter)
  if (filter.operator === 'exists' || filter.operator === 'missing') {
    return `${plain} (${filter.path} ${filter.operator})`
  }
  if (filter.operator === 'in' || filter.operator === 'not_in') {
    return `${plain} (${filter.path} ${filter.operator.replace('_', ' ')} ${formatJsonValues(filter.values ?? [])})`
  }
  return `${plain} (${filter.path} ${filter.operator} ${formatJsonValue(filter.value)})`
}

function stateQueryOrderSummary(query: {
  orderBy?: string | null
  limit?: number | null
  includeArchived: boolean
}): string {
  const parts = [
    query.orderBy ? `order ${query.orderBy}` : 'default order',
    query.limit ? `limit ${query.limit}` : 'no limit',
    query.includeArchived ? 'includes archived' : 'active only',
  ]
  return parts.join(', ')
}

function loopControlsSummary(controls: {
  onlyInputPath?: string | null
  fromInputPath?: string | null
  toInputPath?: string | null
}): string {
  const rows = [
    controls.onlyInputPath ? `only <- ${controls.onlyInputPath}` : null,
    controls.fromInputPath ? `from <- ${controls.fromInputPath}` : null,
    controls.toInputPath ? `to <- ${controls.toInputPath}` : null,
  ].filter(Boolean)
  return rows.length > 0 ? rows.join('\n') : 'None'
}

function stateWriteOperationsSummary(
  operations: readonly {
    entityType: string
    action: string
    targetId?: string | null
    idempotencyKey?: string | null
  }[],
): string {
  return operations.map((operation) => {
    const target = operation.targetId ? ` -> ${operation.targetId}` : ''
    const idempotency = operation.idempotencyKey ? ` (${operation.idempotencyKey})` : ''
    return `${humanize(operation.action)} ${humanize(operation.entityType)}${target}${idempotency}`
  }).join('\n')
}

function conditionsSummary(conditions: readonly WorkflowConditionDto[]): string {
  if (conditions.length === 0) return 'None'
  return conditions.map(conditionSummary).join('\n')
}

function conditionsSummaryInline(conditions: readonly WorkflowConditionDto[]): string {
  if (conditions.length === 0) return 'None'
  return conditions.map(conditionSummary).join('; ')
}

function runOverrideSummary(overrides: {
  providerProfileId?: string | null
  modelId?: string | null
  thinkingEffort?: string | null
  approvalMode?: string | null
  promptPreface?: string
  planModeRequired?: boolean
  autoCompactEnabled?: boolean
}): string {
  const parts = [
    overrides.providerProfileId ? `provider ${overrides.providerProfileId}` : null,
    overrides.modelId ? `model ${overrides.modelId}` : null,
    overrides.thinkingEffort ? `thinking ${overrides.thinkingEffort}` : null,
    overrides.approvalMode ? `approval ${humanize(overrides.approvalMode)}` : null,
    overrides.planModeRequired ? 'plan mode required' : null,
    overrides.autoCompactEnabled === false ? 'auto compact disabled' : null,
    overrides.promptPreface?.trim() ? 'prompt preface set' : null,
  ].filter(Boolean)
  return parts.length > 0 ? parts.join(', ') : 'Default runtime'
}

function loopPolicySummary(policy: NonNullable<WorkflowEdgeDto['loopPolicy']>): string {
  const parts = [
    `${policy.loopKey}, max ${policy.maxAttempts}`,
    `scope ${humanize(policy.attemptScope)}`,
    `carry ${humanize(policy.carryoverPolicy)}`,
    `reset ${humanize(policy.resetPolicy)}`,
    policy.stallDetector ? `stall ${humanize(policy.stallDetector)}` : null,
    `exhausted -> ${policy.onExhausted}`,
  ].filter(Boolean)
  return parts.join('\n')
}

function jsonPreview(value: unknown, maxLength = 480): string {
  const summary = summarizeJson(value)
  if (summary.length <= maxLength) return summary
  return `${summary.slice(0, maxLength - 1)}...`
}

function formatJsonValues(values: readonly unknown[]): string {
  if (values.length === 0) return '[]'
  return values.map(formatJsonValue).join(', ')
}

function formatJsonValue(value: unknown): string {
  if (typeof value === 'string') return value
  if (value === null) return 'null'
  if (value === undefined) return 'undefined'
  return summarizeJson(value)
}

function stateFilterValueText(filter: WorkflowStateQueryFilterDto | undefined): string {
  if (!filter) return 'archived'
  if (filter.operator === 'in' || filter.operator === 'not_in') {
    return (filter.values ?? []).map(String).join(', ')
  }
  if (filter.value === null || filter.value === undefined) return ''
  return String(filter.value)
}

function stateFilterFromText(
  current: WorkflowStateQueryFilterDto | undefined,
  text: string,
): WorkflowStateQueryFilterDto {
  const base: WorkflowStateQueryFilterDto = current ?? {
    path: '$.status',
    operator: 'neq',
    value: 'archived',
    values: [],
  }
  if (base.operator === 'in' || base.operator === 'not_in') {
    return {
      ...base,
      value: undefined,
      values: text
        .split(',')
        .map((entry) => entry.trim())
        .filter(Boolean),
    }
  }
  return {
    ...base,
    value: text,
    values: [],
  }
}

function workflowRunInputFields(definition: WorkflowDefinitionDto): WorkflowRunInputField[] {
  const fields = new Map<string, WorkflowRunInputField>()
  const visitBindings = (bindings: readonly WorkflowInputBindingDto[]) => {
    for (const binding of bindings) {
      if (!isRunInputBinding(binding)) continue
      const key = inputFieldKey(binding.name, binding.path ?? null)
      const previous = fields.get(key)
      fields.set(key, {
        key,
        label: binding.promptLabel ?? humanize(binding.name),
        required: Boolean(previous?.required || binding.required),
      })
    }
  }
  for (const node of definition.nodes) {
    if (
      node.type === 'agent' ||
      node.type === 'state_write' ||
      node.type === 'state_patch' ||
      node.type === 'subgraph'
    ) {
      visitBindings(node.inputBindings)
    }
  }
  for (const subgraph of definition.subgraphs) {
    visitBindings(subgraph.inputBindings)
    for (const node of subgraph.nodes) {
      if (
        node.type === 'agent' ||
        node.type === 'state_write' ||
        node.type === 'state_patch' ||
        node.type === 'subgraph'
      ) {
        visitBindings(node.inputBindings)
      }
    }
  }
  return [...fields.values()].sort((left, right) => Number(right.required) - Number(left.required) || left.label.localeCompare(right.label))
}

function isRunInputBinding(binding: unknown): binding is WorkflowInputBindingDto & { source: 'run_input' } {
  return isRecord(binding) && binding.source === 'run_input' && typeof binding.name === 'string'
}

function inputFieldKey(name: string, path: string | null): string {
  const field = path?.match(/^\$\.([A-Za-z0-9_-]+)$/)?.[1]
  return (field || name).replace(/[^A-Za-z0-9_-]+/g, '_')
}

function buildInitialWorkflowInput(
  fields: readonly WorkflowRunInputField[],
  values: Record<string, string>,
): Record<string, string> {
  const input: Record<string, string> = {}
  for (const field of fields) {
    const value = (values[field.key] ?? '').trim()
    if (value) input[field.key] = value
  }
  return input
}

function buildWorkflowRunPreview(definition: WorkflowDefinitionDto): WorkflowRunPreview {
  const preview: WorkflowRunPreview = {
    stateReads: [],
    stateWrites: [],
    loops: [],
    commands: [],
    checkpoints: [],
    terminals: [],
  }
  const visitNode = (node: WorkflowNodeDto) => {
    if (node.type === 'state_read' || node.type === 'state_query') {
      preview.stateReads.push(`${node.title}: ${humanize(node.query.entityType)}`)
    } else if (node.type === 'state_write' || node.type === 'state_patch') {
      preview.stateWrites.push(`${node.title}: ${humanize(node.operation.action)} ${humanize(node.operation.entityType)}`)
    } else if (node.type === 'collection_loop') {
      preview.loops.push(`${node.title}: ${humanize(node.collection.entityType)} max ${node.maxItemCount}`)
    } else if (node.type === 'command') {
      preview.commands.push([node.command, ...node.args].join(' '))
    } else if (node.type === 'human_checkpoint') {
      preview.checkpoints.push(`${node.title}: ${node.decisionOptions.join(', ') || humanize(node.checkpointType)}`)
    } else if (node.type === 'terminal') {
      preview.terminals.push(`${node.title}: ${humanize(node.terminalStatus)}`)
    }
  }
  definition.nodes.forEach(visitNode)
  definition.subgraphs.forEach((subgraph) => subgraph.nodes.forEach(visitNode))
  return preview
}

function producedArtifactRefForNode(node: WorkflowNodeDto): string | null {
  switch (node.type) {
    case 'agent':
    case 'command':
    case 'subgraph':
      return `${node.id}.${node.outputContract.artifactType}`
    case 'state_read':
    case 'state_query':
      return `${node.id}.${node.outputArtifactType}`
    case 'state_write':
    case 'state_patch':
      return `${node.id}.${node.operation.outputArtifactType}`
    case 'collection_loop':
      return `${node.id}.${node.itemArtifactType}`
    default:
      return null
  }
}

function defaultWorkflowLoopPolicy(edge: WorkflowEdgeDto): NonNullable<WorkflowEdgeDto['loopPolicy']> {
  return {
    loopKey: `${edge.fromNodeId}_loop`,
    maxAttempts: 2,
    attemptScope: 'run',
    carryoverPolicy: 'all',
    selectedArtifactRefs: [],
    resetPolicy: 'never',
    stallDetector: null,
    onExhausted: edge.toNodeId,
  }
}

function createNode(
  type: WorkflowNodeDto['type'],
  id: string,
  fallbackAgentRef: AgentRefDto,
  position: { x: number; y: number },
): WorkflowNodeDto {
  const base = {
    id,
    title: humanize(id),
    description: '',
    position,
  }
  if (type === 'agent') {
    return {
      ...base,
      type,
      agentRef: fallbackAgentRef,
      displayLabel: null,
      inputBindings: [],
      outputContract: {
        artifactType: 'text_output',
        schemaVersion: 1,
        extraction: 'generic_text',
        required: true,
      },
      runOverrides: null,
      resourceScopes: [],
      failurePolicy: {
        quotaFailureClasses: [],
        transientFailureClasses: [],
      },
    }
  }
  if (type === 'router') return { ...base, type }
  if (type === 'gate') return { ...base, type, requiredChecks: [], onBlocked: 'pause' }
  if (type === 'state_read' || type === 'state_query') {
    return {
      ...base,
      type,
      query: {
        entityType: 'delivery_phase',
        filters: [{ path: '$.status', operator: 'neq', value: 'archived', values: [] }],
        orderBy: '$.sortOrder',
        limit: null,
        includeArchived: false,
      },
      outputArtifactType: type === 'state_read' ? 'state_read_result' : 'state_query_result',
    }
  }
  if (type === 'state_write' || type === 'state_patch') {
    return {
      ...base,
      type,
      inputBindings: [],
      operation: {
        entityType: 'delivery_phase',
        action: type === 'state_patch' ? 'patch' : 'upsert',
        idempotencyKey: null,
        targetId: null,
        payload: {
          title: 'Delivery phase',
          status: 'incomplete',
        },
        outputArtifactType: 'state_write_result',
      },
    }
  }
  if (type === 'state_checkpoint') {
    return { ...base, type, requiredChecks: [], onBlocked: 'pause' }
  }
  if (type === 'collection_loop') {
    return {
      ...base,
      type,
      collection: {
        entityType: 'delivery_phase',
        filters: [{ path: '$.status', operator: 'not_in', values: ['complete', 'archived'] }],
        orderBy: '$.sortOrder',
        limit: null,
        includeArchived: false,
      },
      itemArtifactType: 'collection_item',
      itemVariableName: 'item',
      sortKey: '$.sortOrder',
      afterItemRequery: true,
      maxItemCount: 100,
      maxRuntimeSeconds: null,
      controls: {},
    }
  }
  if (type === 'subgraph') {
    return {
      ...base,
      type,
      subgraphId: 'subgraph',
      inputBindings: [],
      outputContract: {
        artifactType: 'subgraph_result',
        schemaVersion: 1,
        extraction: 'json_object',
        required: true,
        renderTextPath: '$.summary',
      },
    }
  }
  if (type === 'command') {
    return {
      ...base,
      type,
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
    }
  }
  if (type === 'human_checkpoint') {
    return {
      ...base,
      type,
      checkpointType: 'human_verify',
      prompt: 'Review the Workflow state and choose a decision.',
      decisionOptions: ['continue'],
      resumePayloadSchema: null,
      stateUpdates: [],
    }
  }
  if (type === 'merge') {
    return { ...base, type, waitPolicy: 'all', quorum: null, failFast: false }
  }
  return { ...base, type: 'terminal', terminalStatus: 'success' }
}

function autoLayoutWorkflowDefinition(definition: WorkflowDefinitionDto): WorkflowDefinitionDto {
  const defaultTemplatePositions = defaultWorkflowTemplatePositionsForDefinition(definition)
  if (defaultTemplatePositions) {
    return applyWorkflowNodePositions(definition, defaultTemplatePositions)
  }

  const byId = new Map(definition.nodes.map((node) => [node.id, node]))
  const originalIndex = new Map(definition.nodes.map((node, index) => [node.id, index]))
  const layoutEdges = workflowLayoutEdges(definition, byId)
  const incoming = new Map<string, WorkflowLayoutEdge[]>()
  const outgoing = new Map<string, WorkflowLayoutEdge[]>()

  for (const node of definition.nodes) {
    incoming.set(node.id, [])
    outgoing.set(node.id, [])
  }

  for (const edge of layoutEdges) {
    incoming.get(edge.toNodeId)?.push(edge)
    outgoing.get(edge.fromNodeId)?.push(edge)
  }

  const rankById = workflowLayoutRankById(definition, layoutEdges)
  const groups = new Map<number, WorkflowNodeDto[]>()
  for (const node of definition.nodes) {
    const rank = rankById.get(node.id) ?? 0
    groups.set(rank, [...(groups.get(rank) ?? []), node])
  }

  const positionById = new Map<string, { x: number; y: number }>()
  for (const [rank, rankNodes] of [...groups].sort(([left], [right]) => left - right)) {
    const x = WORKFLOW_LAYOUT_ORIGIN_X + rank * WORKFLOW_LAYOUT_COLUMN_GAP
    const usedY: number[] = []
    const nodes = [...rankNodes].sort((left, right) => {
      const leftY = workflowLayoutPreferredY(left, incoming.get(left.id) ?? [], outgoing.get(left.id) ?? [], byId)
      const rightY = workflowLayoutPreferredY(right, incoming.get(right.id) ?? [], outgoing.get(right.id) ?? [], byId)
      if (leftY !== rightY) return leftY - rightY
      const terminalDelta = terminalLayoutOrder(left) - terminalLayoutOrder(right)
      if (terminalDelta !== 0) return terminalDelta
      return (originalIndex.get(left.id) ?? 0) - (originalIndex.get(right.id) ?? 0)
    })
    for (const node of nodes) {
      const preferredY = workflowLayoutPreferredY(
        node,
        incoming.get(node.id) ?? [],
        outgoing.get(node.id) ?? [],
        byId,
      )
      const y = nearestOpenWorkflowLayoutY(preferredY, usedY)
      usedY.push(y)
      positionById.set(node.id, { x, y })
    }
  }

  return {
    ...definition,
    nodes: definition.nodes.map((node) => ({
      ...node,
      position: positionById.get(node.id) ?? node.position,
    })),
  }
}

function defaultWorkflowTemplatePositionsForDefinition(
  definition: WorkflowDefinitionDto,
): WorkflowTemplateNodePositionsDto | null {
  const nodeIds = new Set(definition.nodes.map((node) => node.id))
  for (const positions of Object.values(WORKFLOW_TEMPLATE_DEFAULT_NODE_POSITIONS)) {
    const positionNodeIds = Object.keys(positions)
    if (positionNodeIds.length !== definition.nodes.length) continue
    if (positionNodeIds.every((nodeId) => nodeIds.has(nodeId))) return positions
  }
  return null
}

function applyWorkflowNodePositions(
  definition: WorkflowDefinitionDto,
  positions: WorkflowTemplateNodePositionsDto,
): WorkflowDefinitionDto {
  return {
    ...definition,
    nodes: definition.nodes.map((node) => {
      const position = positions[node.id]
      if (!position) return node
      return {
        ...node,
        position: { x: position.x, y: position.y },
      }
    }),
  }
}

function workflowLayoutEdges(
  definition: WorkflowDefinitionDto,
  byId: ReadonlyMap<string, WorkflowNodeDto>,
): WorkflowLayoutEdge[] {
  const edges: WorkflowLayoutEdge[] = []
  for (const edge of definition.edges) {
    if (!byId.has(edge.fromNodeId)) continue
    if (byId.has(edge.toNodeId) && edge.type !== 'loop') {
      edges.push({
        id: edge.id,
        fromNodeId: edge.fromNodeId,
        toNodeId: edge.toNodeId,
        edge,
        kind: 'direct',
      })
    }
    const exhaustionTarget = workflowLoopExhaustionTarget(edge)
    if (exhaustionTarget && byId.has(exhaustionTarget)) {
      edges.push({
        id: `${edge.id}__exhausted`,
        fromNodeId: edge.fromNodeId,
        toNodeId: exhaustionTarget,
        edge,
        kind: 'exhausted',
      })
    }
  }
  return edges
}

function workflowLayoutRankById(
  definition: WorkflowDefinitionDto,
  edges: readonly WorkflowLayoutEdge[],
): Map<string, number> {
  const rankById = new Map<string, number>()
  for (const node of definition.nodes) {
    rankById.set(node.id, node.id === definition.startNodeId ? 0 : Number.NEGATIVE_INFINITY)
  }

  for (let pass = 0; pass < definition.nodes.length; pass += 1) {
    let changed = false
    for (const edge of edges) {
      const sourceRank = rankById.get(edge.fromNodeId) ?? Number.NEGATIVE_INFINITY
      if (!Number.isFinite(sourceRank)) continue
      const targetRank = Math.max(rankById.get(edge.toNodeId) ?? Number.NEGATIVE_INFINITY, sourceRank + 1)
      if (targetRank !== rankById.get(edge.toNodeId)) {
        rankById.set(edge.toNodeId, targetRank)
        changed = true
      }
    }
    if (!changed) break
  }

  const finiteRanks = [...rankById.values()].filter(Number.isFinite)
  let fallbackRank = finiteRanks.length > 0 ? Math.max(...finiteRanks) + 1 : 0
  for (const node of definition.nodes) {
    if (!Number.isFinite(rankById.get(node.id))) {
      rankById.set(node.id, fallbackRank)
      fallbackRank += 1
    }
  }
  return rankById
}

function workflowLayoutPreferredY(
  node: WorkflowNodeDto,
  incoming: readonly WorkflowLayoutEdge[],
  outgoing: readonly WorkflowLayoutEdge[],
  byId: ReadonlyMap<string, WorkflowNodeDto>,
): number {
  const lane = workflowLayoutLaneForNode(node, incoming, outgoing, byId)
  return WORKFLOW_LAYOUT_LANE_Y[lane]
}

function nearestOpenWorkflowLayoutY(preferredY: number, usedY: readonly number[]): number {
  if (usedY.every((y) => Math.abs(y - preferredY) >= WORKFLOW_LAYOUT_LANE_GAP)) {
    return preferredY
  }
  for (let step = 1; step <= usedY.length + 2; step += 1) {
    const down = preferredY + step * WORKFLOW_LAYOUT_LANE_GAP
    if (usedY.every((y) => Math.abs(y - down) >= WORKFLOW_LAYOUT_LANE_GAP)) {
      return down
    }
    const up = preferredY - step * WORKFLOW_LAYOUT_LANE_GAP
    if (up >= WORKFLOW_LAYOUT_LANE_Y.upper && usedY.every((y) => Math.abs(y - up) >= WORKFLOW_LAYOUT_LANE_GAP)) {
      return up
    }
  }
  return preferredY + (usedY.length + 1) * WORKFLOW_LAYOUT_LANE_GAP
}

function workflowLayoutLaneForNode(
  node: WorkflowNodeDto,
  incoming: readonly WorkflowLayoutEdge[],
  outgoing: readonly WorkflowLayoutEdge[],
  byId: ReadonlyMap<string, WorkflowNodeDto>,
): WorkflowLayoutLane {
  if (node.type === 'terminal') return terminalWorkflowLayoutLane(node, incoming, byId)
  const label = `${node.id} ${node.title}`.toLowerCase()
  if (node.type === 'human_checkpoint' || node.type === 'state_checkpoint' || node.type === 'gate') {
    return 'recovery'
  }
  if (incoming.some((entry) => entry.edge.type === 'failure' || entry.edge.type === 'recovery' || entry.kind === 'exhausted')) {
    return 'recovery'
  }
  if (outgoing.some((entry) => {
    const target = byId.get(entry.toNodeId)
    return target?.type === 'terminal' && target.terminalStatus === 'needs_human'
  })) {
    return 'recovery'
  }
  if (/\b(debug|gap|fix|human|audit|blocked|decision|recovery)\b/.test(label)) {
    return 'recovery'
  }
  if (/\b(intake|seed|process|execute|verify|check|complete|archive|close)\b/.test(label)) {
    return 'upper'
  }
  return 'main'
}

function terminalWorkflowLayoutLane(
  node: Extract<WorkflowNodeDto, { type: 'terminal' }>,
  incoming: readonly WorkflowLayoutEdge[],
  byId: ReadonlyMap<string, WorkflowNodeDto>,
): WorkflowLayoutLane {
  void incoming
  void byId
  if (node.terminalStatus === 'needs_human' || node.terminalStatus === 'failure') return 'recovery'
  if (node.id.toLowerCase().includes('partial') || node.title.toLowerCase().includes('partial')) return 'main'
  if (node.terminalStatus === 'success') return 'upper'
  return 'terminal'
}

function terminalLayoutOrder(node: WorkflowNodeDto): number {
  if (node.type !== 'terminal') return 0
  if (node.terminalStatus === 'success' && !node.id.toLowerCase().includes('partial')) return 0
  if (node.id.toLowerCase().includes('partial') || node.title.toLowerCase().includes('partial')) return 1
  if (node.terminalStatus === 'needs_human') return 2
  if (node.terminalStatus === 'failure') return 3
  return 4
}

function latestRunNodesByNodeId(run: WorkflowRunDto | null): Map<string, WorkflowRunNodeDto> {
  const map = new Map<string, WorkflowRunNodeDto>()
  for (const node of run?.nodes ?? []) {
    const previous = map.get(node.nodeId)
    if (!previous || previous.attemptNumber <= node.attemptNumber) {
      map.set(node.nodeId, node)
    }
  }
  return map
}

function latestArtifactsByNodeId(run: WorkflowRunDto | null): Map<string, WorkflowArtifactRecordDto> {
  const nodeIdByRunId = new Map((run?.nodes ?? []).map((node) => [node.id, node.nodeId]))
  const map = new Map<string, WorkflowArtifactRecordDto>()
  for (const artifact of run?.artifacts ?? []) {
    const nodeId = nodeIdByRunId.get(artifact.producerNodeRunId)
    if (!nodeId) continue
    map.set(nodeId, artifact)
  }
  return map
}

function latestArtifactsByNodeRunId(run: WorkflowRunDto | null): Map<string, WorkflowArtifactRecordDto> {
  const map = new Map<string, WorkflowArtifactRecordDto>()
  for (const artifact of run?.artifacts ?? []) {
    map.set(artifact.producerNodeRunId, artifact)
  }
  return map
}

function localSubgraphNodeId(parentNodeId: string, childNodeId: string): string {
  const prefix = `${parentNodeId}::`
  return childNodeId.startsWith(prefix) ? childNodeId.slice(prefix.length) : childNodeId
}

function runNodeStatusBadgeVariant(status: WorkflowNodeRunStatusDto): 'default' | 'secondary' | 'destructive' | 'outline' {
  if (status === 'succeeded') return 'default'
  if (status === 'failed' || status === 'stalled' || status === 'cancelled') return 'destructive'
  if (status === 'waiting_on_gate') return 'secondary'
  return 'outline'
}

function latestEdgeDecision(
  run: WorkflowRunDto | null,
  edgeId: string,
): WorkflowRunEdgeDecisionDto | null {
  const decisions = run?.edgeDecisions.filter((decision) => decision.edgeId === edgeId) ?? []
  return decisions.at(-1) ?? null
}

function labelForAgentRef(
  ref: AgentRefDto,
  agents: readonly WorkflowAgentSummaryDto[],
): string {
  return agents.find((agent) => agentRefKey(agent.ref) === agentRefKey(ref))?.displayName ??
    (ref.kind === 'built_in' ? humanize(ref.runtimeAgentId) : ref.definitionId)
}

function iconForNodeType(type: WorkflowNodeDto['type']) {
  switch (type) {
    case 'agent':
      return Bot
    case 'router':
      return Route
    case 'gate':
      return ShieldCheck
    case 'human_checkpoint':
      return PauseCircle
    case 'merge':
      return GitMerge
    case 'terminal':
      return CheckCircle2
    case 'state_read':
    case 'state_query':
      return Database
    case 'state_write':
    case 'state_patch':
    case 'state_checkpoint':
      return ClipboardCheck
    case 'collection_loop':
      return ListChecks
    case 'subgraph':
      return Workflow
    case 'command':
      return SquareTerminal
  }
}

function nodeTone(type: WorkflowNodeDto['type']): string {
  switch (type) {
    case 'agent':
      return 'amber'
    case 'router':
      return 'sky'
    case 'gate':
      return 'emerald'
    case 'human_checkpoint':
      return 'rose'
    case 'merge':
      return 'violet'
    case 'terminal':
      return 'foreground'
    case 'state_read':
    case 'state_query':
      return 'teal'
    case 'state_write':
    case 'state_patch':
    case 'state_checkpoint':
      return 'emerald'
    case 'collection_loop':
      return 'violet'
    case 'subgraph':
      return 'teal'
    case 'command':
      return 'amber'
  }
}

function nodeIconTone(type: WorkflowNodeDto['type']): string {
  switch (type) {
    case 'agent':
      return 'bg-amber-500/12 text-amber-500 ring-amber-500/30'
    case 'router':
      return 'bg-sky-500/12 text-sky-500 ring-sky-500/30'
    case 'gate':
      return 'bg-emerald-500/12 text-emerald-500 ring-emerald-500/30'
    case 'human_checkpoint':
      return 'bg-rose-500/12 text-rose-500 ring-rose-500/30'
    case 'merge':
      return 'bg-violet-500/12 text-violet-500 ring-violet-500/30'
    case 'terminal':
      return 'bg-foreground/10 text-muted-foreground ring-foreground/20'
    case 'state_read':
    case 'state_query':
      return 'bg-cyan-500/12 text-cyan-600 ring-cyan-500/30 dark:text-cyan-300'
    case 'state_write':
    case 'state_patch':
    case 'state_checkpoint':
      return 'bg-lime-500/12 text-lime-700 ring-lime-500/30 dark:text-lime-300'
    case 'collection_loop':
      return 'bg-indigo-500/12 text-indigo-500 ring-indigo-500/30'
    case 'subgraph':
      return 'bg-fuchsia-500/12 text-fuchsia-500 ring-fuchsia-500/30'
    case 'command':
      return 'bg-orange-500/12 text-orange-600 ring-orange-500/30 dark:text-orange-300'
  }
}

function handleClassForEdgeType(type: WorkflowEdgeTypeDto): string {
  switch (type) {
    case 'success':
      return '!bg-emerald-500'
    case 'failure':
      return '!bg-rose-500'
    case 'conditional':
      return '!bg-sky-500'
    case 'loop':
      return '!bg-indigo-500'
    case 'recovery':
      return '!bg-orange-500'
    case 'manual_override':
      return '!bg-violet-500'
  }
}

function workflowDefinitionWithReactNodePositions(
  definition: WorkflowDefinitionDto,
  nodes: readonly WorkflowReactNode[],
): WorkflowDefinitionDto {
  if (nodes.length === 0) return definition
  const positionById = new Map<string, WorkflowNodePosition>()
  for (const node of nodes) {
    positionById.set(node.id, node.position)
  }
  let changed = false
  const positionedNodes = definition.nodes.map((node) => {
    const position = positionById.get(node.id)
    if (!position || (position.x === node.position.x && position.y === node.position.y)) {
      return node
    }
    changed = true
    return { ...node, position }
  })
  return changed ? { ...definition, nodes: positionedNodes } : definition
}

function workflowNodesWithRenderedHandles(
  nodes: readonly WorkflowReactNode[],
  definition: WorkflowDefinitionDto,
  editable: boolean,
): WorkflowReactNode[] {
  const nodeById = new Map(definition.nodes.map((node) => [node.id, node]))
  const handlesByNodeId = buildWorkflowNodeHandles(definition, editable)
  let changed = false
  const renderedNodes = nodes.map((node) => {
    const definitionNode = nodeById.get(node.id) ?? node.data.node
    const handles = handlesByNodeId.get(node.id) ?? []
    if (node.data.node === definitionNode && workflowNodeHandlesEqual(node.data.handles, handles)) {
      return node
    }
    changed = true
    return {
      ...node,
      data: {
        ...node.data,
        node: definitionNode,
        handles,
      },
    }
  })
  return changed ? renderedNodes : nodes as WorkflowReactNode[]
}

function workflowNodeHandlesEqual(
  left: readonly WorkflowNodeHandle[],
  right: readonly WorkflowNodeHandle[],
): boolean {
  if (left.length !== right.length) return false
  for (let index = 0; index < left.length; index += 1) {
    const leftHandle = left[index]
    const rightHandle = right[index]
    if (
      leftHandle.id !== rightHandle.id ||
      leftHandle.side !== rightHandle.side ||
      leftHandle.edgeType !== rightHandle.edgeType ||
      leftHandle.className !== rightHandle.className ||
      !workflowHandleStylesEqual(leftHandle.style, rightHandle.style)
    ) {
      return false
    }
  }
  return true
}

function workflowHandleStylesEqual(
  left: CSSProperties | undefined,
  right: CSSProperties | undefined,
): boolean {
  if (!left || !right) return left === right
  return left.left === right.left && left.top === right.top
}

function workflowNodeInternalsRefreshKey(nodes: readonly WorkflowReactNode[]): string {
  const parts: string[] = []
  for (const node of nodes) {
    parts.push([
      node.id,
      node.type ?? '',
      node.width ?? '',
      node.height ?? '',
      node.initialWidth ?? '',
      node.initialHeight ?? '',
      node.measured?.width ?? '',
      node.measured?.height ?? '',
      node.data.handles
        .map((handle) => [
          handle.id,
          handle.side,
          handle.edgeType,
          handle.style?.left ?? '',
          handle.style?.top ?? '',
        ].join(':'))
        .join(','),
    ].join('|'))
  }
  return parts.join('::')
}

function buildWorkflowNodeHandles(
  definition: WorkflowDefinitionDto,
  editable: boolean,
): Map<string, WorkflowNodeHandle[]> {
  const nodeById = new Map(definition.nodes.map((node) => [node.id, node]))
  const buckets = new Map<string, Map<string, WorkflowNodeHandle>>()
  const addHandle = (
    nodeId: string,
    side: WorkflowHandleSide,
    edgeType: WorkflowEdgeTypeDto,
  ) => {
    const node = nodeById.get(nodeId)
    if (!node) return
    const nodeHandles = buckets.get(nodeId) ?? new Map<string, WorkflowNodeHandle>()
    buckets.set(nodeId, nodeHandles)
    const id = workflowHandleId(side, edgeType)
    if (nodeHandles.has(id)) return
    nodeHandles.set(id, {
      id,
      side,
      edgeType,
      className: handleClassForEdgeType(edgeType),
    })
  }

  for (const edge of workflowVisibleEdges(definition.edges)) {
    const sides = workflowEdgeHandleSides(edge, nodeById)
    addHandle(edge.fromNodeId, sides.source, edge.type)
    addHandle(edge.toNodeId, sides.target, edge.type)
  }

  if (editable) {
    for (const node of definition.nodes) {
      const outgoingTypes = workflowSupportedOutgoingEdgeTypes(node)
      for (const edgeType of outgoingTypes) {
        addHandle(node.id, 'right', edgeType)
      }
      for (const edgeType of workflowSupportedIncomingEdgeTypes(node)) {
        if (node.type === 'terminal' || !outgoingTypes.includes(edgeType)) {
          addHandle(node.id, 'left', edgeType)
        }
      }
    }
  }

  const result = new Map<string, WorkflowNodeHandle[]>()
  for (const [nodeId, nodeHandles] of buckets) {
    const handles = [...nodeHandles.values()].sort(workflowHandleSort)
    const sideCounts = new Map<WorkflowHandleSide, number>()
    for (const handle of handles) {
      sideCounts.set(handle.side, (sideCounts.get(handle.side) ?? 0) + 1)
    }
    const sideIndexes = new Map<WorkflowHandleSide, number>()
    result.set(
      nodeId,
      handles.map((handle) => {
        const index = sideIndexes.get(handle.side) ?? 0
        sideIndexes.set(handle.side, index + 1)
        return {
          ...handle,
          style: workflowHandleOffsetStyle(handle.side, index, sideCounts.get(handle.side) ?? 1),
        }
      }),
    )
  }
  return result
}

function workflowVisibleEdges(edges: readonly WorkflowEdgeDto[]): WorkflowEdgeDto[] {
  const visible: WorkflowEdgeDto[] = []
  for (const edge of edges) {
    visible.push(edge)
    const exhaustionTargetId = workflowLoopExhaustionTarget(edge)
    if (exhaustionTargetId) {
      visible.push(workflowLoopExhaustionVisualEdge(edge, exhaustionTargetId))
    }
  }
  return visible
}

function workflowHandleSort(left: WorkflowNodeHandle, right: WorkflowNodeHandle): number {
  const sideDelta = WORKFLOW_HANDLE_SIDES.indexOf(left.side) - WORKFLOW_HANDLE_SIDES.indexOf(right.side)
  if (sideDelta !== 0) return sideDelta
  return workflowEdgeTypeOrder(left.edgeType) - workflowEdgeTypeOrder(right.edgeType)
}

function workflowEdgeTypeOrder(type: WorkflowEdgeTypeDto): number {
  const index = WORKFLOW_EDGE_TYPE_ORDER.indexOf(type)
  return index === -1 ? WORKFLOW_EDGE_TYPE_ORDER.length : index
}

function workflowHandleOffsetStyle(
  side: WorkflowHandleSide,
  index: number,
  count: number,
): CSSProperties | undefined {
  if (count <= 1) return undefined
  const offset = `${((index + 1) / (count + 1)) * 100}%`
  if (side === 'top' || side === 'bottom') return { left: offset }
  return { top: offset }
}

function workflowSupportedOutgoingEdgeTypes(node: WorkflowNodeDto): readonly WorkflowEdgeTypeDto[] {
  switch (node.type) {
    case 'terminal':
      return []
    case 'router':
      return ['conditional', 'loop']
    case 'human_checkpoint':
      return ['manual_override']
    case 'gate':
    case 'state_checkpoint':
      return ['success', 'failure', 'recovery']
    case 'command':
      return ['success', 'failure', 'recovery']
    case 'collection_loop':
      return ['success', 'conditional', 'loop']
    case 'merge':
      return ['success', 'conditional', 'loop']
    case 'subgraph':
      return ['success', 'failure', 'recovery', 'loop']
    case 'agent':
      return ['success', 'failure', 'conditional', 'loop', 'recovery']
    case 'state_read':
    case 'state_query':
      return ['success', 'failure', 'conditional']
    case 'state_write':
    case 'state_patch':
      return ['success', 'failure', 'loop']
  }
}

function workflowSupportedIncomingEdgeTypes(node: WorkflowNodeDto): readonly WorkflowEdgeTypeDto[] {
  if (node.type === 'terminal') {
    if (node.terminalStatus === 'success') return ['success', 'conditional', 'manual_override']
    if (node.terminalStatus === 'failure') return ['failure', 'recovery', 'loop']
    return ['failure', 'recovery', 'conditional', 'loop', 'manual_override']
  }
  switch (node.type) {
    case 'router':
      return ['success', 'conditional', 'loop']
    case 'human_checkpoint':
      return ['failure', 'recovery', 'conditional', 'loop', 'manual_override']
    case 'gate':
    case 'state_checkpoint':
      return ['success', 'failure', 'recovery', 'conditional']
    case 'merge':
      return ['success', 'conditional', 'loop']
    case 'state_read':
    case 'state_query':
    case 'collection_loop':
      return ['success', 'conditional', 'loop']
    case 'state_write':
    case 'state_patch':
      return ['success', 'conditional', 'loop', 'manual_override']
    case 'command':
      return ['success', 'conditional']
    case 'agent':
    case 'subgraph':
      return ['success', 'failure', 'conditional', 'loop', 'recovery', 'manual_override']
  }
}

function workflowNodeSupportsOutgoingEdgeType(
  node: WorkflowNodeDto | undefined,
  edgeType: WorkflowEdgeTypeDto,
): boolean {
  return Boolean(node && workflowSupportedOutgoingEdgeTypes(node).includes(edgeType))
}

function workflowNodeSupportsIncomingEdgeType(
  node: WorkflowNodeDto | undefined,
  edgeType: WorkflowEdgeTypeDto,
): boolean {
  return Boolean(node && workflowSupportedIncomingEdgeTypes(node).includes(edgeType))
}

function isValidWorkflowConnection(
  connection: Connection | WorkflowReactEdge,
  nodes: readonly WorkflowNodeDto[],
): boolean {
  if (!connection.source || !connection.target || connection.source === connection.target) {
    return false
  }
  const sourceEdgeType = workflowEdgeTypeFromHandleId(connection.sourceHandle)
  const targetEdgeType = workflowEdgeTypeFromHandleId(connection.targetHandle)
  if (!sourceEdgeType || !targetEdgeType || sourceEdgeType !== targetEdgeType) return false
  const nodeById = new Map(nodes.map((node) => [node.id, node]))
  return (
    workflowNodeSupportsOutgoingEdgeType(nodeById.get(connection.source), sourceEdgeType) &&
    workflowNodeSupportsIncomingEdgeType(nodeById.get(connection.target), targetEdgeType)
  )
}

function workflowDefaultEdgePriority(edgeType: WorkflowEdgeTypeDto): number {
  switch (edgeType) {
    case 'success':
      return 10
    case 'failure':
    case 'recovery':
      return 20
    case 'conditional':
      return 30
    case 'loop':
      return 40
    case 'manual_override':
      return 50
  }
}

function workflowIncomingEdgeCount(nodeId: string, edges: readonly WorkflowEdgeDto[]): number {
  return edges.reduce((count, edge) => {
    const visibleTargetCount = edge.toNodeId === nodeId ? 1 : 0
    const exhaustedTargetCount = workflowLoopExhaustionTarget(edge) === nodeId ? 1 : 0
    return count + visibleTargetCount + exhaustedTargetCount
  }, 0)
}

function workflowOutgoingEdgeCount(nodeId: string, edges: readonly WorkflowEdgeDto[]): number {
  return edges.reduce((count, edge) => {
    if (edge.fromNodeId !== nodeId) return count
    return count + 1 + (workflowLoopExhaustionTarget(edge) ? 1 : 0)
  }, 0)
}

function workflowLoopExhaustionTarget(edge: WorkflowEdgeDto): string | null {
  const targetId = edge.loopPolicy?.onExhausted
  if (!targetId || targetId === edge.toNodeId) return null
  return targetId
}

function workflowLoopExhaustionVisualEdge(
  edge: WorkflowEdgeDto,
  targetId: string,
): WorkflowEdgeDto {
  return {
    ...edge,
    id: `${edge.id}__exhausted`,
    toNodeId: targetId,
    type: 'recovery',
    label: 'exhausted',
    condition: { kind: 'always' },
    loopPolicy: null,
  }
}

function workflowReactEdgeFromDefinitionEdge(
  edge: WorkflowEdgeDto,
  nodeById: ReadonlyMap<string, WorkflowNodeDto>,
  options: {
    editing?: boolean
    selectedNodeId?: string | null
    matched?: boolean
    running?: boolean
    visualOnly?: boolean
    className?: string
  } = {},
): WorkflowReactEdge {
  const handleSides = workflowEdgeHandleSides(edge, nodeById)
  const edgeTone = workflowEdgeTone(edge.type)
  const focused = options.selectedNodeId
    ? edge.fromNodeId === options.selectedNodeId || edge.toNodeId === options.selectedNodeId
    : false
  return {
    id: edge.id,
    source: edge.fromNodeId,
    sourceHandle: workflowHandleId(handleSides.source, edge.type),
    target: edge.toNodeId,
    targetHandle: workflowHandleId(handleSides.target, edge.type),
    type: 'workflow-branch',
    markerEnd: { ...WORKFLOW_EDGE_MARKER, color: edgeTone.color },
    data: {
      workflowEdge: edge,
      label: edge.label || EDGE_TYPE_LABEL[edge.type],
      targetClearance: options.editing === true ? WORKFLOW_EDGE_TARGET_CLEARANCE : 0,
      edgeColor: edgeTone.color,
      labelBackground: edgeTone.labelBackground,
      labelBorderColor: edgeTone.labelBorderColor,
      labelTextColor: edgeTone.labelTextColor,
      focused,
      visualOnly: options.visualOnly,
    },
    animated: !options.visualOnly && options.running === true && options.matched === true,
    selectable: !options.visualOnly,
    className: cn(
      'workflow-definition-edge',
      'agent-edge-phase-branch',
      !options.visualOnly && options.matched && 'workflow-definition-edge--matched',
      focused && 'workflow-definition-edge--related',
      edge.type === 'loop' && 'workflow-definition-edge--loop',
      edge.type === 'recovery' && 'workflow-definition-edge--recovery',
      options.className,
    ),
    style: {
      stroke: edgeTone.color,
      ...(!options.visualOnly && options.matched ? { strokeWidth: 2.2 } : null),
    },
  }
}

function workflowEdgeTone(type: WorkflowEdgeTypeDto): WorkflowEdgeTone {
  const base = workflowEdgeColorToken(type)
  return {
    color: `color-mix(in oklab, ${base} 82%, var(--foreground))`,
    labelBackground: `color-mix(in oklab, var(--background) 94%, ${base} 6%)`,
    labelBorderColor: `color-mix(in oklab, ${base} 42%, var(--background))`,
    labelTextColor: `color-mix(in oklab, ${base} 86%, var(--foreground))`,
  }
}

function workflowEdgeColorToken(type: WorkflowEdgeTypeDto): string {
  switch (type) {
    case 'success':
      return 'var(--color-emerald-500, #10b981)'
    case 'failure':
      return 'var(--color-rose-500, #f43f5e)'
    case 'conditional':
      return 'var(--color-sky-500, #0ea5e9)'
    case 'loop':
      return 'var(--color-indigo-500, #6366f1)'
    case 'recovery':
      return 'var(--color-orange-500, #f97316)'
    case 'manual_override':
      return 'var(--color-violet-500, #8b5cf6)'
  }
}

function workflowHandleId(side: WorkflowHandleSide, edgeType: WorkflowEdgeTypeDto): string {
  return `workflow-${side}-${edgeType}`
}

function workflowEdgeTypeFromHandleId(handleId: string | null | undefined): WorkflowEdgeTypeDto | null {
  if (!handleId) return null
  const prefix = 'workflow-'
  if (!handleId.startsWith(prefix)) return null
  const withoutPrefix = handleId.slice(prefix.length)
  const side = WORKFLOW_HANDLE_SIDES.find((candidate) => withoutPrefix.startsWith(`${candidate}-`))
  if (!side) return null
  const edgeType = withoutPrefix.slice(side.length + 1)
  return WORKFLOW_EDGE_TYPE_ORDER.includes(edgeType as WorkflowEdgeTypeDto)
    ? edgeType as WorkflowEdgeTypeDto
    : null
}

function workflowArrowTargetClearance(marker: {
  width?: number
  markerUnits?: string
  strokeWidth?: number
}): number {
  const markerWidth = marker.width ?? 12.5
  const markerStrokeWidth = marker.strokeWidth ?? 1
  const markerUnitScale = marker.markerUnits === 'strokeWidth' ? markerStrokeWidth : 1
  const markerCoordinateScale = (markerWidth * markerUnitScale) / WORKFLOW_MARKER_VIEWBOX_SIZE
  const arrowTipStrokeRadius = (markerStrokeWidth * markerCoordinateScale) / 2

  return Math.ceil(
    WORKFLOW_HANDLE_VISUAL_SIZE / 2
    + WORKFLOW_ARROW_TARGET_GAP
    + arrowTipStrokeRadius,
  )
}

function workflowEdgeHandleSides(
  edge: WorkflowEdgeDto,
  nodeById: ReadonlyMap<string, WorkflowNodeDto>,
): { source: WorkflowHandleSide; target: WorkflowHandleSide } {
  const source = nodeById.get(edge.fromNodeId)
  const target = nodeById.get(edge.toNodeId)
  if (!source || !target || source.id === target.id) {
    return { source: 'right', target: 'left' }
  }

  const sourceRect = workflowNodeRect(source)
  const targetRect = workflowNodeRect(target)
  const sourceCenter = rectCenter(sourceRect)
  const targetCenter = rectCenter(targetRect)
  const dx = targetCenter.x - sourceCenter.x
  const dy = targetCenter.y - sourceCenter.y

  if (dx === 0 && dy === 0) {
    return { source: 'right', target: 'left' }
  }

  return {
    source: workflowSideFacingVector(sourceRect, dx, dy),
    target: workflowSideFacingVector(targetRect, -dx, -dy),
  }
}

function workflowNodeRect(node: WorkflowNodeDto): {
  x: number
  y: number
  width: number
  height: number
} {
  return {
    x: node.position.x,
    y: node.position.y,
    width: WORKFLOW_LAYOUT_CARD_WIDTH,
    height: workflowEstimatedNodeHeight(node),
  }
}

function workflowEstimatedNodeHeight(node: WorkflowNodeDto): number {
  const detailRows = node.description.trim().length > 0 ? 2 : 1
  const chipRows = node.type === 'agent' || node.type === 'subgraph' || node.type === 'command' ? 1 : 0
  const terminalAdjustment = node.type === 'terminal' ? -14 : 0
  return WORKFLOW_LAYOUT_CARD_HEIGHT + detailRows * 8 + chipRows * 10 + terminalAdjustment
}

function rectCenter(rect: { x: number; y: number; width: number; height: number }): {
  x: number
  y: number
} {
  return {
    x: rect.x + rect.width / 2,
    y: rect.y + rect.height / 2,
  }
}

function workflowSideFacingVector(
  rect: { width: number; height: number },
  dx: number,
  dy: number,
): WorkflowHandleSide {
  const exitsThroughHorizontalSide = Math.abs(dx) * rect.height >= Math.abs(dy) * rect.width
  if (exitsThroughHorizontalSide) return dx >= 0 ? 'right' : 'left'
  return dy >= 0 ? 'bottom' : 'top'
}

function defaultCondition(kind: string): WorkflowConditionDto {
  if (kind === 'artifact_exists') return { kind, artifactRef: 'node.text_output' }
  if (kind === 'artifact_field_equals') {
    return { kind, artifactRef: 'node.text_output', path: '$.status', value: 'passed' }
  }
  if (kind === 'artifact_field_number_compare') {
    return {
      kind,
      artifactRef: 'node.text_output',
      path: '$.count',
      operator: 'gt',
      value: 0,
    }
  }
  if (kind === 'state_field_equals') {
    return { kind, stateRef: 'node.state_query_result', path: '$.status', value: 'active' }
  }
  if (kind === 'state_collection_count_compare') {
    return { kind, stateRef: 'node.state_query_result', operator: 'gt', value: 0 }
  }
  if (kind === 'human_decision_is') {
    return { kind, checkpointNodeId: 'human_checkpoint', decision: 'continue' }
  }
  return { kind: 'always' }
}

function supportedConditionKind(condition: WorkflowConditionDto): string {
  if (
    condition.kind === 'always' ||
    condition.kind === 'artifact_exists' ||
    condition.kind === 'artifact_field_equals' ||
    condition.kind === 'artifact_field_number_compare' ||
    condition.kind === 'state_field_equals' ||
    condition.kind === 'state_collection_count_compare' ||
    condition.kind === 'human_decision_is'
  ) {
    return condition.kind
  }
  return 'always'
}

function conditionSummary(condition: WorkflowConditionDto): string {
  switch (condition.kind) {
    case 'always':
      return 'Always'
    case 'artifact_exists':
      return `${condition.artifactRef} exists`
    case 'artifact_field_equals':
      return `${condition.artifactRef}${condition.path} = ${formatJsonValue(condition.value)}`
    case 'artifact_field_number_compare':
      return `${condition.artifactRef}${condition.path} ${condition.operator} ${condition.value}`
    case 'human_decision_is':
      return `${condition.checkpointNodeId} decision is ${condition.decision}`
    case 'state_field_equals':
      return `${condition.stateRef}${condition.path} = ${formatJsonValue(condition.value)}`
    case 'state_collection_count_compare':
      return `${condition.stateRef} count ${condition.operator} ${condition.value}`
    case 'all':
      return `All: ${conditionsSummaryInline(condition.conditions)}`
    case 'any':
      return `Any: ${conditionsSummaryInline(condition.conditions)}`
    case 'not':
      return `Not: ${conditionSummary(condition.condition)}`
    case 'node_status':
      return `${condition.nodeId} is ${humanize(condition.status)}`
    case 'artifact_field_in':
      return `${condition.artifactRef}${condition.path} in ${formatJsonValues(condition.values)}`
    case 'failure_class_is':
      return `Failure class is ${condition.failureClass}`
    case 'loop_attempt_lt':
      return `${condition.loopKey} attempts < ${condition.value}`
    case 'loop_attempt_gte':
      return `${condition.loopKey} attempts >= ${condition.value}`
  }
}

function uniqueId(prefix: string, existing: readonly string[]): string {
  const normalized = prefix.replace(/[^A-Za-z0-9_-]+/g, '_')
  let candidate = normalized
  let index = 2
  const seen = new Set(existing)
  while (seen.has(candidate)) {
    candidate = `${normalized}_${index}`
    index += 1
  }
  return candidate
}

function cloneDefinition(definition: WorkflowDefinitionDto): WorkflowDefinitionDto {
  return JSON.parse(JSON.stringify(definition)) as WorkflowDefinitionDto
}

function summarizeJson(value: unknown): string {
  if (typeof value === 'string') return value
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

function workflowBundlePreview(bundle: unknown): unknown {
  if (!isRecord(bundle)) return bundle
  const run = isRecord(bundle.run) ? bundle.run : null
  const deliveryState = isRecord(bundle.deliveryState) ? bundle.deliveryState : null
  return {
    schema: typeof bundle.schema === 'string' ? bundle.schema : 'xero.workflow_run_bundle.v1',
    projectId: bundle.projectId,
    runId: bundle.runId,
    blocker: isRecord(bundle.blocker)
      ? {
          status: bundle.blocker.status,
          summary: bundle.blocker.summary,
          nodeId: bundle.blocker.nodeId,
          failureClass: bundle.blocker.failureClass,
        }
      : bundle.blocker,
    run: run
      ? {
          status: run.status,
          terminalStatus: run.terminalStatus,
          nodeCount: arrayFieldLength(run, 'nodes'),
          eventCount: arrayFieldLength(run, 'events'),
          artifactCount: arrayFieldLength(run, 'artifacts'),
          edgeDecisionCount: arrayFieldLength(run, 'edgeDecisions'),
          gateDecisionCount: arrayFieldLength(run, 'gateDecisions'),
          loopAttemptCount: arrayFieldLength(run, 'loopAttempts'),
        }
      : null,
    deliveryState: deliveryState
      ? Object.fromEntries(
          Object.entries(deliveryState).map(([key, value]) => [
            key,
            Array.isArray(value) ? { count: value.length } : value,
          ]),
        )
      : null,
    previewNote: 'Large arrays are summarized in this panel; the exported bundle response keeps the full run payload.',
  }
}

function arrayFieldLength(record: Record<string, unknown>, field: string): number {
  const value = record[field]
  return Array.isArray(value) ? value.length : 0
}

function workflowTimelineEvents(
  events: readonly WorkflowEventDto[],
  runNode: WorkflowRunNodeDto | null,
  edge: WorkflowEdgeDto | null,
): WorkflowEventDto[] {
  const filtered = events.filter((event) => {
    if (runNode) return event.nodeRunId === runNode.id
    if (!edge) return true
    const payload = event.event
    return isRecord(payload) && payload.edgeId === edge.id
  })
  return [...filtered].sort((left, right) => right.createdAt.localeCompare(left.createdAt))
}

function timelineEventSummary(event: WorkflowEventDto): string {
  const payload = event.event
  if (!isRecord(payload)) return summarizeJson(payload)
  if (typeof payload.metric === 'string') return humanize(payload.metric)
  if (typeof payload.failureClass === 'string') return payload.failureClass
  if (typeof payload.reason === 'string') return payload.reason
  if (typeof payload.edgeId === 'string') {
    const matched = typeof payload.matched === 'boolean'
      ? payload.matched ? 'matched' : 'not matched'
      : 'evaluated'
    return `${payload.edgeId} ${matched}`
  }
  if (typeof payload.nodeId === 'string') return payload.nodeId
  return summarizeJson(payload)
}

function compactTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function humanize(value: string): string {
  return value
    .replace(/[_-]+/g, ' ')
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
}

function isActiveRun(status: WorkflowRunStatusDto): boolean {
  return status === 'queued' || status === 'running' || status === 'paused'
}

function isRetryableRunNodeStatus(status: WorkflowNodeRunStatusDto): boolean {
  return status === 'failed' || status === 'stalled' || status === 'skipped' || status === 'cancelled'
}

function isSkippableRunNodeStatus(status: WorkflowNodeRunStatusDto): boolean {
  return status === 'pending'
    || status === 'eligible'
    || status === 'starting'
    || status === 'running'
    || status === 'waiting_on_gate'
}

function workflowRunNeedsRecoverySurface(run: WorkflowRunDto): boolean {
  if (run.status === 'paused' || run.status === 'failed' || run.status === 'cancelled') {
    return true
  }
  return run.nodes.some((node) => isRetryableRunNodeStatus(node.status))
}

function workflowRunCanResumeNextPhase(run: WorkflowRunDto): boolean {
  return run.definitionSnapshot.nodes.some(
    (node) => node.type === 'collection_loop' && node.collection.entityType === 'delivery_phase',
  )
}
