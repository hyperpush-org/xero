import { MarkerType, type Edge, type Node } from '@xyflow/react'

import type {
  AgentConsumedArtifactDto,
  AgentDbTouchpointDetailDto,
  AgentDbTouchpointKindDto,
  AgentHeaderDto,
  AgentOutputContractDto,
  AgentOutputSectionDto,
  AgentPromptDto,
  AgentAttachedSkillDto,
  AgentToolSummaryDto,
  AgentTriggerLifecycleEventDto,
  AgentTriggerRefDto,
  WorkflowAgentGraphEdgeDto,
  WorkflowAgentGraphNodeDto,
  WorkflowAgentGraphProjectionDto,
  WorkflowAgentDetailDto,
} from '@/src/lib/xero-model/workflow-agents'
import type {
  CustomAgentWorkflowBranchConditionDto,
  CustomAgentWorkflowGateDto,
  CustomAgentWorkflowPhaseDto,
} from '@/src/lib/xero-model/agent-definition'

export type AgentGraphNodeKind =
  | 'agent-header'
  | 'prompt'
  | 'skills'
  | 'tool'
  | 'db-table'
  | 'agent-output'
  | 'output-section'
  | 'consumed-artifact'
  | 'stage'
  | 'stage-group-frame'

export interface AgentHeaderSummaryCounts {
  prompts: number
  tools: number
  dbTables: number
  outputSections: number
  consumes: number
  attachedSkills: number
}

// Advanced authoring fields exposed on the header node's expanded body. These
// are required by the agent-definition validator but don't have natural homes
// elsewhere on the canvas, so we surface them on the header.
export interface AgentHeaderAdvancedFields {
  workflowContract: string
  finalResponseContract: string
  examplePrompts: string[]
  refusalEscalationCases: string[]
  // Object-form toolPolicy. The runtime accepts either a string preset OR a
  // structured object — we always emit an object so downstream picked-tool
  // edits flow through cleanly.
  allowedEffectClasses: string[]
  deniedTools: string[]
  allowedToolPacks: string[]
  deniedToolPacks: string[]
  allowedToolGroups: string[]
  deniedToolGroups: string[]
  allowedMcpServers: string[]
  deniedMcpServers: string[]
  allowedDynamicTools: string[]
  deniedDynamicTools: string[]
  // Subagent role allow / deny. Edited from the granular policy editor in
  // the header properties panel; the snapshot emitter only ships them when
  // `subagentAllowed` is true (the validator requires allowedSubagentRoles
  // to be non-empty in that case).
  allowedSubagentRoles: string[]
  deniedSubagentRoles: string[]
  externalServiceAllowed: boolean
  browserControlAllowed: boolean
  skillRuntimeAllowed: boolean
  subagentAllowed: boolean
  commandAllowed: boolean
  destructiveWriteAllowed: boolean
}

export interface AgentHeaderNodeData extends Record<string, unknown> {
  header: AgentHeaderDto
  summary: AgentHeaderSummaryCounts
  advanced: AgentHeaderAdvancedFields
}

export interface PromptNodeData extends Record<string, unknown> {
  prompt: AgentPromptDto
}

export interface ToolNodeData extends Record<string, unknown> {
  tool: AgentToolSummaryDto
  directConnectionHandles: {
    source: boolean
    target: boolean
  }
}

export type DbTableTouchpointKind = AgentDbTouchpointKindDto

export interface DbTableNodeData extends Record<string, unknown> {
  table: string
  touchpoint: DbTableTouchpointKind
  purpose: string
  triggers: AgentTriggerRefDto[]
  columns: string[]
}

export interface OutputNodeData extends Record<string, unknown> {
  output: AgentOutputContractDto
}

export interface OutputSectionNodeData extends Record<string, unknown> {
  section: AgentOutputSectionDto
}

export interface ConsumedArtifactNodeData extends Record<string, unknown> {
  artifact: AgentConsumedArtifactDto
}

export interface SkillNodeData extends Record<string, unknown> {
  skill: AgentAttachedSkillDto
}

// Stage node — a single node in the authored in-agent state machine.
// Each stage declares an id, title, the tool names admitted while it is
// active, and required-check gates the runtime must observe before advancing.
// Branches are modeled as `phase-branch` edges between two stage nodes,
// not on the node data, so the canvas can show the same wiring the runtime
// evaluates without duplicating state.
export interface StageNodeData extends Record<string, unknown> {
  phase: CustomAgentWorkflowPhaseDto
  isStart: boolean
}

export interface LaneLabelNodeData extends Record<string, unknown> {
  label: string
  count: number
}

export interface ToolGroupFrameNodeData extends Record<string, unknown> {
  label: string
  count: number
  order: number
  sourceGroups: string[]
}

export interface StageGroupFrameNodeData extends Record<string, unknown> {
  count: number
}

export type AgentHeaderFlowNode = Node<AgentHeaderNodeData, 'agent-header'>
export type PromptFlowNode = Node<PromptNodeData, 'prompt'>
export type SkillFlowNode = Node<SkillNodeData, 'skills'>
export type ToolFlowNode = Node<ToolNodeData, 'tool'>
export type DbTableFlowNode = Node<DbTableNodeData, 'db-table'>
export type OutputFlowNode = Node<OutputNodeData, 'agent-output'>
export type OutputSectionFlowNode = Node<OutputSectionNodeData, 'output-section'>
export type ConsumedArtifactFlowNode = Node<ConsumedArtifactNodeData, 'consumed-artifact'>
export type StageFlowNode = Node<StageNodeData, 'stage'>
export type LaneLabelFlowNode = Node<LaneLabelNodeData, 'lane-label'>
export type ToolGroupFrameFlowNode = Node<ToolGroupFrameNodeData, 'tool-group-frame'>
export type StageGroupFrameFlowNode = Node<StageGroupFrameNodeData, 'stage-group-frame'>

// Union of node data shapes that authoring may mutate. Layout chrome
// (lane-label, tool-group-frame, stage-group-frame) is intentionally excluded
// — those nodes are computed from the user-facing nodes by the layout pass.
export type AgentGraphNodeData =
  | AgentHeaderNodeData
  | PromptNodeData
  | SkillNodeData
  | ToolNodeData
  | DbTableNodeData
  | OutputNodeData
  | OutputSectionNodeData
  | ConsumedArtifactNodeData
  | StageNodeData

export type AgentGraphNode =
  | AgentHeaderFlowNode
  | PromptFlowNode
  | SkillFlowNode
  | ToolFlowNode
  | DbTableFlowNode
  | OutputFlowNode
  | OutputSectionFlowNode
  | ConsumedArtifactFlowNode
  | StageFlowNode
  | LaneLabelFlowNode
  | ToolGroupFrameFlowNode
  | StageGroupFrameFlowNode

export type AgentGraphEdge = Edge

export interface AgentGraph {
  nodes: AgentGraphNode[]
  edges: AgentGraphEdge[]
}

function markerEndFromProjection(
  marker: WorkflowAgentGraphEdgeDto['marker'] | null | undefined,
): AgentGraphEdge['markerEnd'] | undefined {
  if (marker === 'arrow_closed') return ARROW_MARKER
  if (marker === 'arrow') return TRIGGER_ARROW_MARKER
  return undefined
}

function nodeFromProjection(node: WorkflowAgentGraphNodeDto): AgentGraphNode {
  const projected = {
    id: node.id,
    type: node.type,
    position: node.position,
    data: node.data,
    parentId: node.parentId,
    extent: node.extent,
    draggable: node.draggable,
    selectable: node.selectable,
    dragHandle: node.dragHandle,
    style: node.style,
    width: node.width,
    height: node.height,
  }
  return Object.fromEntries(
    Object.entries(projected).filter(([, value]) => value !== undefined),
  ) as unknown as AgentGraphNode
}

function edgeFromProjection(edge: WorkflowAgentGraphEdgeDto): AgentGraphEdge {
  const projected = {
    id: edge.id,
    source: edge.source,
    target: edge.target,
    type: edge.type,
    sourceHandle: edge.sourceHandle,
    targetHandle: edge.targetHandle,
    data: edge.data,
    className: edge.className,
    markerEnd: markerEndFromProjection(edge.marker),
  }
  return Object.fromEntries(
    Object.entries(projected).filter(([, value]) => value !== undefined),
  ) as unknown as AgentGraphEdge
}

export function agentGraphFromProjection(
  projection: WorkflowAgentGraphProjectionDto,
): AgentGraph {
  return {
    nodes: projection.nodes.map(nodeFromProjection),
    edges: projection.edges.map(edgeFromProjection),
  }
}

const HEADER_NODE_ID = 'agent-header'
const OUTPUT_NODE_ID = 'agent-output'

const HEADER_SOURCE_HANDLE = {
  prompt: 'prompts',
  skills: 'skills',
  tool: 'tools',
  db: 'db',
  output: 'output',
  consumed: 'consumed',
  workflow: 'workflow',
} as const

export const AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS = {
  tool: 0.32,
  db: 0.68,
} as const

export const AGENT_GRAPH_HEADER_LEFT_HANDLE_RATIOS = {
  consumed: AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS.tool,
  skills: 0.68,
} as const

// Bottom edge of the header carries two source handles (output, workflow).
// React Flow defaults both to 50% along the edge, so we stagger them by
// horizontal position so each remains grabbable independently.
export const AGENT_GRAPH_HEADER_BOTTOM_HANDLE_RATIOS = {
  output: 0.35,
  workflow: 0.7,
} as const

export const AGENT_GRAPH_TRIGGER_HANDLES = {
  source: 'trigger-source',
  target: 'trigger-target',
} as const

export function promptNodeId(prompt: AgentPromptDto, index: number): string {
  return `prompt:${index}:${prompt.id}`
}

export function toolNodeId(tool: AgentToolSummaryDto): string {
  return `tool:${tool.name}`
}

export function skillNodeId(skill: Pick<AgentAttachedSkillDto, 'id'>): string {
  return `skills:${skill.id}`
}

export function toolGroupFrameNodeId(groupKey: string): string {
  return `tool-group-frame:${groupKey}`
}

// DOM-level id prefix for stage nodes is intentionally kept as
// `workflow-phase:` to preserve compatibility with any in-flight or persisted
// canvas state authored before the rename. The decoder below detects the
// same prefix.
export function stageNodeId(phaseId: string): string {
  return `workflow-phase:${phaseId}`
}

// There is exactly one stage frame per agent (stages always live as a single
// column), so a constant id is enough; no per-bucket variant needed like
// tool-group-frame:CORE / tool-group-frame:SOLANA.
export const STAGE_GROUP_FRAME_NODE_ID = 'stage-group-frame:stages'

export function stageBranchEdgeId(
  sourcePhaseId: string,
  targetPhaseId: string,
  branchIndex: number,
): string {
  return `e:phase-branch:${sourcePhaseId}->${targetPhaseId}:${branchIndex}`
}

// Stable string key for a stage-branch edge's data category. Used by the
// snapshot builder to thread the original `branches[branchIndex].condition`
// object stored on a phase-branch edge back to the right authored entry.
export const STAGE_BRANCH_DATA_CATEGORY = 'phase-branch'

// Identifiers that don't title-case cleanly (acronyms, brand names) get a
// hand-written display label. Looked up before the generic split-and-capitalize
// path so e.g. `mcp_invoke` doesn't render as "Mcp Invoke".
const HUMANIZE_OVERRIDES: Record<string, string> = {
  // tool groups / risk classes
  macos: 'macOS',
  macos_automation: 'macOS Automation',
  mcp: 'MCP',
  mcp_invoke: 'MCP',
  external_chain_observe: 'Chain Observe',
  external_chain_simulation: 'Chain Simulation',
  external_chain_control: 'Chain Control',
  external_capability_observe: 'External Capability',
  system_diagnostics: 'System Diagnostics',
  system_diagnostics_observe: 'System Diagnostics',
  project_context_write: 'Context',
  agent_definition_state: 'Agent Definition',
  coordination_state: 'Coordination',
  process_manager: 'Process Manager',
  registry_control: 'Registry',
  // output contracts (defined in workflow-agents.ts)
  plan_pack: 'Plan Pack',
  crawl_report: 'Crawl Report',
  engineering_summary: 'Engineering Summary',
  debug_summary: 'Debug Summary',
  agent_definition_draft: 'Agent Definition Draft',
  harness_test_report: 'Harness Test Report',
}

const HUMANIZE_WORD_OVERRIDES: Record<string, string> = {
  ai: 'AI',
  alt: 'ALT',
  api: 'API',
  cli: 'CLI',
  cpu: 'CPU',
  db: 'DB',
  http: 'HTTP',
  https: 'HTTPS',
  idl: 'IDL',
  ios: 'iOS',
  json: 'JSON',
  lsp: 'LSP',
  macos: 'macOS',
  mcp: 'MCP',
  os: 'OS',
  pda: 'PDA',
  rpc: 'RPC',
  sdk: 'SDK',
  sha: 'SHA',
  tx: 'TX',
  ui: 'UI',
  url: 'URL',
  vcs: 'VCS',
}

/**
 * Convert a snake_case / kebab-case / camelCase identifier into a Title Case
 * display string. Used wherever a raw identifier (table name, contract id,
 * tool name, section id, source agent id, etc.) would otherwise reach the
 * user. Raw identifiers stay in the DTOs for traceability without rendering
 * browser-native hover tooltips on the canvas.
 */
export function humanizeIdentifier(value: string): string {
  if (!value) return ''
  const override = HUMANIZE_OVERRIDES[value]
  if (override) return override
  return value
    .split(/[._\-\s]+|(?=[A-Z])/)
    .filter(Boolean)
    .map((word) => {
      const lower = word.toLowerCase()
      return HUMANIZE_WORD_OVERRIDES[lower] ?? word.charAt(0).toUpperCase() + lower.slice(1)
    })
    .join(' ')
}

export function humanizeToolGroupKey(key: string): string {
  if (!key) return 'Other'
  return humanizeIdentifier(key)
}

export interface ToolCategoryPresentation {
  key: string
  label: string
  order: number
}

const DEFAULT_TOOL_CATEGORY_ORDER = 10_000

const TOOL_CATEGORY_OVERRIDES: Record<string, ToolCategoryPresentation> = {
  core: { key: 'core', label: 'Core', order: 10 },
  project_context_write: {
    key: 'project_context',
    label: 'Project Context',
    order: 20,
  },
  intelligence: {
    key: 'code_intelligence',
    label: 'Code Intelligence',
    order: 30,
  },
  mutation: { key: 'file_changes', label: 'File Changes', order: 40 },
  command_readonly: { key: 'commands', label: 'Commands', order: 50 },
  command_mutating: { key: 'commands', label: 'Commands', order: 50 },
  command_session: { key: 'commands', label: 'Commands', order: 50 },
  command: { key: 'commands', label: 'Commands', order: 50 },
  process_manager: { key: 'processes', label: 'Processes', order: 60 },
  system_diagnostics: {
    key: 'system_diagnostics',
    label: 'System Diagnostics',
    order: 70,
  },
  system_diagnostics_observe: {
    key: 'system_diagnostics',
    label: 'System Diagnostics',
    order: 70,
  },
  system_diagnostics_privileged: {
    key: 'system_diagnostics',
    label: 'System Diagnostics',
    order: 70,
  },
  macos: { key: 'os_automation', label: 'OS Automation', order: 80 },
  web_search_only: { key: 'web', label: 'Web', order: 90 },
  web_fetch: { key: 'web', label: 'Web', order: 90 },
  web: { key: 'web', label: 'Web', order: 90 },
  browser_observe: { key: 'browser', label: 'Browser', order: 100 },
  browser_control: { key: 'browser', label: 'Browser', order: 100 },
  browser: { key: 'browser', label: 'Browser', order: 100 },
  mcp_list: { key: 'mcp', label: 'MCP', order: 110 },
  mcp_invoke: { key: 'mcp', label: 'MCP', order: 110 },
  mcp: { key: 'mcp', label: 'MCP', order: 110 },
  skills: { key: 'skills', label: 'Skills', order: 120 },
  agent_ops: { key: 'agent_ops', label: 'Agent Operations', order: 130 },
  agent_builder: { key: 'agent_builder', label: 'Agent Builder', order: 140 },
  notebook: { key: 'notebooks', label: 'Notebooks', order: 150 },
  powershell: { key: 'powershell', label: 'PowerShell', order: 160 },
  environment: { key: 'environment', label: 'Environment', order: 170 },
  emulator: { key: 'emulator', label: 'Emulator', order: 180 },
  harness_runner: { key: 'test_harness', label: 'Test Harness', order: 190 },
  solana: { key: 'solana', label: 'Solana', order: 200 },
}

export function toolCategoryPresentationForGroup(group: string): ToolCategoryPresentation {
  const trimmed = group.trim()
  if (!trimmed) {
    return { key: 'other', label: 'Other', order: DEFAULT_TOOL_CATEGORY_ORDER }
  }
  const override = TOOL_CATEGORY_OVERRIDES[trimmed]
  if (override) return override
  return {
    key: trimmed,
    label: humanizeToolGroupKey(trimmed),
    order: DEFAULT_TOOL_CATEGORY_ORDER,
  }
}

interface ToolGroupBucket {
  key: string
  label: string
  order: number
  sourceGroups: string[]
  tools: AgentToolSummaryDto[]
}

/**
 * Partition tool DTOs by their `group` field. Within each bucket, tools keep
 * the input order (already barycenter-sorted upstream). Buckets use a visual
 * taxonomy rather than raw runtime access groups, so split capabilities like
 * `mcp_list` + `mcp_invoke` render as one user-facing category.
 */
function partitionToolDtosByGroup(tools: AgentToolSummaryDto[]): ToolGroupBucket[] {
  const buckets = new Map<
    string,
    {
      label: string
      order: number
      sourceGroups: Set<string>
      tools: AgentToolSummaryDto[]
    }
  >()
  for (const tool of tools) {
    const rawGroup = tool.group?.trim() || 'other'
    const presentation = toolCategoryPresentationForGroup(rawGroup)
    const bucket = buckets.get(presentation.key) ?? {
      label: presentation.label,
      order: presentation.order,
      sourceGroups: new Set<string>(),
      tools: [],
    }
    bucket.sourceGroups.add(rawGroup)
    bucket.tools.push(tool)
    buckets.set(presentation.key, bucket)
  }
  return Array.from(buckets.entries())
    .map(([key, bucket]) => ({
      key,
      label: bucket.label,
      order: bucket.order,
      sourceGroups: Array.from(bucket.sourceGroups).sort((a, b) =>
        humanizeToolGroupKey(a).localeCompare(humanizeToolGroupKey(b)) ||
        a.localeCompare(b),
      ),
      tools: bucket.tools,
    }))
    .sort((a, b) => {
      const orderDelta = a.order - b.order
      if (orderDelta !== 0) return orderDelta
      return a.label.localeCompare(b.label)
    })
}

function dbNodeId(table: string, kind: DbTableTouchpointKind): string {
  return `db:${kind}:${table}`
}

function outputSectionNodeId(id: string): string {
  return `output-section:${id}`
}

function consumedArtifactNodeId(id: string): string {
  return `consumed:${id}`
}

const LIFECYCLE_EVENT_LABELS: Record<AgentTriggerLifecycleEventDto, string> = {
  state_transition: 'state transition',
  plan_update: 'plan update',
  message_persisted: 'message persisted',
  tool_call: 'tool call',
  file_edit: 'file edit',
  run_start: 'run start',
  run_complete: 'run complete',
  approval_decision: 'approval decision',
  verification_gate: 'verification gate',
  definition_persisted: 'definition persisted',
}

export function lifecycleEventLabel(event: AgentTriggerLifecycleEventDto): string {
  return LIFECYCLE_EVENT_LABELS[event] ?? event
}

const TOUCHPOINT_KIND_LABEL: Record<DbTableTouchpointKind, string> = {
  read: 'reads',
  write: 'writes',
  encouraged: 'encouraged',
}

const ARROW_MARKER = {
  type: MarkerType.ArrowClosed,
  width: 14,
  height: 14,
} as const

const TRIGGER_ARROW_MARKER = {
  type: MarkerType.Arrow,
  width: 16,
  height: 16,
} as const

const CONSUME_ARROW_MARKER = {
  type: MarkerType.Arrow,
  width: 16,
  height: 16,
} as const

interface OrderedTouchpoint {
  detail: AgentDbTouchpointDetailDto
  kind: DbTableTouchpointKind
}

/**
 * Bucket touchpoints by priority (writes → reads → encouraged) without
 * de-duplicating by table name. A table the agent both reads and writes
 * renders as two distinct cards — one per (table, kind) pair — so the
 * canvas reports every relationship the DTO declares instead of silently
 * dropping the read when a write is also present. Within each kind, dupes
 * by table are still collapsed (the DTO shouldn't list the same table
 * twice in `reads` for example, but be defensive).
 */
function dbTouchpointsByPriority(
  reads: AgentDbTouchpointDetailDto[],
  writes: AgentDbTouchpointDetailDto[],
  encouraged: AgentDbTouchpointDetailDto[],
): OrderedTouchpoint[] {
  const ordered: OrderedTouchpoint[] = []
  const seenPerKind = new Map<DbTableTouchpointKind, Set<string>>()

  const push = (detail: AgentDbTouchpointDetailDto, kind: DbTableTouchpointKind) => {
    const seen = seenPerKind.get(kind) ?? new Set<string>()
    if (seen.has(detail.table)) return
    seen.add(detail.table)
    seenPerKind.set(kind, seen)
    ordered.push({ detail, kind })
  }

  for (const detail of writes) push(detail, 'write')
  for (const detail of reads) push(detail, 'read')
  for (const detail of encouraged) push(detail, 'encouraged')
  return ordered
}

/**
 * Barycenter heuristic — order DB rows so each table sits as close as possible
 * to the average vertical position of the tool / section nodes that trigger
 * it. This is the standard Sugiyama-style cross-minimisation step: when each
 * cross-edge is short and roughly horizontal, edge crossings drop sharply.
 *
 * Tools live in the upper-right lane (column-wrapped); sections live in the
 * lower-centre grid. We translate both into a single comparable axis by
 * deriving each tool's row index from its column-wrapped position and each
 * section's row index from its grid row, then average across the touchpoint's
 * triggers. Touchpoints with no resolvable trigger keep stable order at the
 * tail of their bucket.
 */
function sortDbsByBarycenter(
  ordered: OrderedTouchpoint[],
  triggerSourceY: (trigger: AgentTriggerRefDto) => number | null,
): OrderedTouchpoint[] {
  const kindOrder: Record<DbTableTouchpointKind, number> = {
    write: 0,
    read: 1,
    encouraged: 2,
  }
  // Stable Schwartzian transform so ties keep insertion order from `ordered`.
  const decorated = ordered.map((entry, index) => {
    let sum = 0
    let count = 0
    for (const trigger of entry.detail.triggers) {
      const y = triggerSourceY(trigger)
      if (y === null) continue
      sum += y
      count++
    }
    const barycenter = count === 0 ? Number.POSITIVE_INFINITY : sum / count
    return { entry, index, barycenter }
  })
  decorated.sort((a, b) => {
    const kindDelta = kindOrder[a.entry.kind] - kindOrder[b.entry.kind]
    if (kindDelta !== 0) return kindDelta
    if (a.barycenter !== b.barycenter) return a.barycenter - b.barycenter
    // Tie-break alphabetically so visual ordering is deterministic when the
    // barycenter signal is missing (lifecycle-only triggers).
    const tableDelta = a.entry.detail.table.localeCompare(b.entry.detail.table)
    if (tableDelta !== 0) return tableDelta
    return a.index - b.index
  })
  return decorated.map((d) => d.entry)
}

/**
 * Approximate the row index of a tool inside the tool lane. The lane wraps
 * into multiple columns at MAX_TOOLS_PER_COLUMN, so a tool's *row* matters
 * more than its raw alphabetical position when minimising crossings against
 * the DB column on its right. Mirrors the wrap math in `layout.ts` so the
 * graph builder and layout engine agree on the implied geometry.
 */
const MAX_TOOLS_PER_COLUMN = 6

function toolLaneRow(toolIndex: number, totalTools: number): number {
  if (totalTools <= 0) return 0
  const colCount = Math.max(1, Math.ceil(totalTools / MAX_TOOLS_PER_COLUMN))
  const rowsPerCol = Math.ceil(totalTools / colCount)
  return toolIndex % rowsPerCol
}

export function buildAgentGraph(detail: WorkflowAgentDetailDto): AgentGraph {
  const nodes: AgentGraphNode[] = []
  const edges: AgentGraphEdge[] = []

  // 1. Header. Summary counts mirror the on-canvas card counts so the chip
  // numbers always match what the user can see — including the case where a
  // table is both read and written and renders as two distinct DB cards.
  const dbTouchpointCount =
    detail.dbTouchpoints.reads.length +
    detail.dbTouchpoints.writes.length +
    detail.dbTouchpoints.encouraged.length
  nodes.push({
    id: HEADER_NODE_ID,
    type: 'agent-header',
    position: { x: 0, y: 0 },
    data: {
      header: detail.header,
      summary: {
        prompts: detail.prompts.length,
        tools: detail.tools.length,
        dbTables: dbTouchpointCount,
        outputSections: detail.output.sections.length,
        consumes: detail.consumes.length,
        attachedSkills: detail.attachedSkills.length,
      },
      advanced: deriveAdvancedFromDetail(detail),
    },
  })

  // 2. Prompts
  detail.prompts.forEach((prompt, index) => {
    const id = promptNodeId(prompt, index)
    nodes.push({
      id,
      type: 'prompt',
      position: { x: 0, y: 0 },
      data: { prompt },
    })
    edges.push({
      id: `e:header->${id}`,
      source: HEADER_NODE_ID,
      sourceHandle: HEADER_SOURCE_HANDLE.prompt,
      target: id,
      type: 'smoothstep',
      data: { category: 'prompt' },
      className: 'agent-edge agent-edge-prompt',
      markerEnd: ARROW_MARKER,
    })
  })

  // 3. Attached skills. These are always-injected context, not callable tool
  // capability, so they get their own lane and edge family.
  for (const skill of detail.attachedSkills) {
    const id = skillNodeId(skill)
    nodes.push({
      id,
      type: 'skills',
      position: { x: 0, y: 0 },
      data: { skill },
    })
    edges.push({
      id: `e:header->${id}`,
      source: HEADER_NODE_ID,
      sourceHandle: HEADER_SOURCE_HANDLE.skills,
      target: id,
      type: 'smoothstep',
      data: { category: 'skills' },
      className: 'agent-edge agent-edge-skill',
      markerEnd: ARROW_MARKER,
    })
  }

  // 4. Tools and 5. DBs require coordinated ordering: the barycenter heuristic
  // sorts each lane to minimise crossings with the other. We do a two-pass
  // refinement: alphabetical → reorder DBs by tool-row barycenter → reorder
  // tools by DB-row barycenter → reorder DBs once more. Two passes is enough
  // for the small graphs the inspector renders and converges to a stable
  // ordering well below the asymptotic Sugiyama bound.

  // Pass 0: deterministic alphabetical baseline so the first barycenter
  // calculation has a well-defined coordinate system.
  let sortedTools = [...detail.tools].sort((a, b) => a.name.localeCompare(b.name))
  let toolRowByName = new Map<string, number>()
  const refreshToolRows = () => {
    toolRowByName = new Map<string, number>()
    sortedTools.forEach((tool, index) => {
      toolRowByName.set(tool.name, toolLaneRow(index, sortedTools.length))
    })
  }
  refreshToolRows()

  const sectionRowByName = new Map<string, number>()
  detail.output.sections.forEach((section, index) => {
    // Sections now render as a single vertical column, so each section's row
    // is just its index. Sections live in a separate vertical band below the
    // DB column, so we scale them up by a large constant to keep section-fed
    // DBs at the bottom of their bucket.
    sectionRowByName.set(section.id, 1000 + index)
  })

  const dbBucketEntries = dbTouchpointsByPriority(
    detail.dbTouchpoints.reads,
    detail.dbTouchpoints.writes,
    detail.dbTouchpoints.encouraged,
  )

  const triggerSourceY = (trigger: AgentTriggerRefDto): number | null => {
    if (trigger.kind === 'tool') {
      const row = toolRowByName.get(trigger.name)
      return row === undefined ? null : row
    }
    if (trigger.kind === 'output_section') {
      const row = sectionRowByName.get(trigger.id)
      return row === undefined ? null : row
    }
    // Lifecycle and upstream-artifact triggers don't map to a positioned
    // source node — exclude from the barycenter so they don't bias placement.
    return null
  }

  // Pass 1: sort DBs by current tool/section positions.
  let dbEntries = sortDbsByBarycenter(dbBucketEntries, triggerSourceY)

  // Pass 2: reorder tools so each tool sits near the average row of the DBs
  // it writes / reads. Preserves alphabetical tie-breaking for tools without
  // any DB triggers.
  const dbRowByTable = new Map<string, number>()
  dbEntries.forEach((entry, index) => {
    dbRowByTable.set(entry.detail.table, index)
  })
  const toolBarycenter = (toolName: string): number => {
    let sum = 0
    let count = 0
    for (const entry of dbEntries) {
      for (const trigger of entry.detail.triggers) {
        if (trigger.kind === 'tool' && trigger.name === toolName) {
          const row = dbRowByTable.get(entry.detail.table)
          if (row === undefined) continue
          sum += row
          count++
        }
      }
    }
    return count === 0 ? Number.POSITIVE_INFINITY : sum / count
  }
  sortedTools = sortedTools
    .map((tool, index) => ({ tool, index, barycenter: toolBarycenter(tool.name) }))
    .sort((a, b) => {
      if (a.barycenter !== b.barycenter) return a.barycenter - b.barycenter
      // Stable tie-break: alphabetical for the unconstrained tail.
      const nameDelta = a.tool.name.localeCompare(b.tool.name)
      if (nameDelta !== 0) return nameDelta
      return a.index - b.index
    })
    .map((d) => d.tool)
  refreshToolRows()

  // Pass 3: re-sort DBs against the refreshed tool ordering.
  dbEntries = sortDbsByBarycenter(dbBucketEntries, triggerSourceY)

  // Now emit nodes and edges in the final order.
  // Tools are partitioned into visual category frames. Raw runtime groups can
  // merge here when they are capability splits of the same user-facing family.
  // Each frame is a draggable parent node; its tools render as children with
  // positions relative to the frame, so dragging a frame moves the whole
  // category. The agent header connects to each frame (instead of to every
  // tool), which keeps the edge bundle proportional to the number of
  // categories rather than the raw tool count.
  const toolIdsByName = new Map<string, string>()
  const toolGroupBuckets = partitionToolDtosByGroup(sortedTools)

  for (const bucket of toolGroupBuckets) {
    const frameId = toolGroupFrameNodeId(bucket.key)
    nodes.push({
      id: frameId,
      type: 'tool-group-frame',
      position: { x: 0, y: 0 },
      data: {
        label: bucket.label,
        count: bucket.tools.length,
        order: bucket.order,
        sourceGroups: bucket.sourceGroups,
      },
      dragHandle: '.agent-tool-group-frame__drag-handle',
      // React Flow makes draggable parent nodes pointer-active across their
      // full bounds. The frame is visual chrome; only its label should catch
      // events so tool buttons inside the frame remain clickable.
      style: { pointerEvents: 'none' },
    })
    edges.push({
      id: `e:header->${frameId}`,
      source: HEADER_NODE_ID,
      sourceHandle: HEADER_SOURCE_HANDLE.tool,
      target: frameId,
      type: 'smoothstep',
      data: { category: 'tool' },
      className: 'agent-edge agent-edge-tool',
      markerEnd: ARROW_MARKER,
    })
    for (const tool of bucket.tools) {
      const id = toolNodeId(tool)
      toolIdsByName.set(tool.name, id)
      nodes.push({
        id,
        type: 'tool',
        position: { x: 0, y: 0 },
        parentId: frameId,
        // Layout writes child positions relative to their parent frame; React
        // Flow requires `extent: 'parent'` to actually anchor the relative
        // coordinate system to the parent's bounds.
        extent: 'parent',
        // Individual tools no longer drag — the user moves a whole category
        // by grabbing the frame, which pulls every tool inside with it.
        draggable: false,
        // React Flow sets pointer-events: none on non-draggable/non-selectable
        // nodes. Tools still own interactive expand buttons, so opt them back
        // into hit testing without re-enabling node dragging.
        style: { pointerEvents: 'all' },
        data: { tool, directConnectionHandles: { source: false, target: false } },
      })
    }
  }

  // dbEntryById lets the trigger-edge loop find each entry by its node id
  // without re-deriving from (table, kind). Multiple entries per table are
  // expected — a table that's both read and written produces two entries —
  // so we key by id rather than table name.
  const dbEntryById = new Map<string, OrderedTouchpoint>()
  for (const entry of dbEntries) {
    const id = dbNodeId(entry.detail.table, entry.kind)
    dbEntryById.set(id, entry)
    nodes.push({
      id,
      type: 'db-table',
      position: { x: 0, y: 0 },
      data: {
        table: entry.detail.table,
        touchpoint: entry.kind,
        purpose: entry.detail.purpose,
        triggers: entry.detail.triggers,
        columns: entry.detail.columns,
      },
    })
    edges.push({
      id: `e:header->${id}`,
      source: HEADER_NODE_ID,
      sourceHandle: HEADER_SOURCE_HANDLE.db,
      target: id,
      type: 'smoothstep',
      data: { category: 'db-table' },
      className: 'agent-edge agent-edge-db',
      markerEnd: ARROW_MARKER,
    })
  }

  // 6. Output contract (parent) + sections (children).
  nodes.push({
    id: OUTPUT_NODE_ID,
    type: 'agent-output',
    position: { x: 0, y: 0 },
    data: { output: detail.output },
  })
  edges.push({
    id: `e:header->${OUTPUT_NODE_ID}`,
    source: HEADER_NODE_ID,
    sourceHandle: HEADER_SOURCE_HANDLE.output,
    target: OUTPUT_NODE_ID,
    type: 'smoothstep',
    data: { category: 'agent-output' },
    className: 'agent-edge agent-edge-output',
    markerEnd: ARROW_MARKER,
  })

  const sectionIdToNode = new Map<string, string>()
  for (const section of detail.output.sections) {
    const id = outputSectionNodeId(section.id)
    sectionIdToNode.set(section.id, id)
    nodes.push({
      id,
      type: 'output-section',
      position: { x: 0, y: 0 },
      data: { section },
    })
    edges.push({
      id: `e:${OUTPUT_NODE_ID}->${id}`,
      source: OUTPUT_NODE_ID,
      target: id,
      type: 'smoothstep',
      data: { category: 'output-section' },
      className: 'agent-edge agent-edge-output-section',
      markerEnd: ARROW_MARKER,
    })
    // Functional cross-edge: tool → output-section, when authored. The label
    // makes the relationship readable at the edge itself rather than forcing
    // the user to expand the section card to see the "produced by" chip.
    for (const toolName of section.producedByTools) {
      const toolId = toolIdsByName.get(toolName)
      if (!toolId) continue
      edges.push({
        id: `e:trigger:${toolId}->${id}`,
        source: toolId,
        sourceHandle: AGENT_GRAPH_TRIGGER_HANDLES.source,
        target: id,
        targetHandle: AGENT_GRAPH_TRIGGER_HANDLES.target,
        // Custom edge type — renders the label via EdgeLabelRenderer portal
        // so it sits above every node card the edge happens to cross.
        type: 'trigger',
        data: { category: 'trigger', triggerLabel: 'produces' },
        className: 'agent-edge agent-edge-trigger',
        markerEnd: TRIGGER_ARROW_MARKER,
      })
    }
  }

  // 7. Consumed artifacts (left of header).
  for (const artifact of detail.consumes) {
    const id = consumedArtifactNodeId(artifact.id)
    nodes.push({
      id,
      type: 'consumed-artifact',
      position: { x: 0, y: 0 },
      data: { artifact },
    })
    edges.push({
      id: `e:${id}->${HEADER_NODE_ID}`,
      source: id,
      target: HEADER_NODE_ID,
      targetHandle: HEADER_SOURCE_HANDLE.consumed,
      type: 'smoothstep',
      data: { category: 'consumed' },
      className: 'agent-edge agent-edge-consume',
      markerEnd: CONSUME_ARROW_MARKER,
    })
  }

  // 8. Cross-edges driven by db touchpoint triggers. Tool triggers connect
  // their tool node to the db node; output-section triggers connect their
  // section node to the db node; upstream-artifact triggers connect the
  // existing consumed-artifact node to the db node. Lifecycle triggers do
  // *not* emit edges — a single lifecycle event typically fires many DB
  // writes, so a synthetic source node would have to fan many long curves
  // across the canvas and obscure the rest of the graph. Lifecycle events
  // are surfaced on the DB card body itself instead, where the user reads
  // them in place without chasing edges.
  const consumedArtifactExists = new Set<string>(
    detail.consumes.map((artifact) => consumedArtifactNodeId(artifact.id)),
  )

  for (const [dbId, entry] of dbEntryById) {
    const seenEdge = new Set<string>()
    const touchpointLabel = TOUCHPOINT_KIND_LABEL[entry.kind]

    for (const trigger of entry.detail.triggers) {
      let sourceId: string | undefined
      let label: string = touchpointLabel

      if (trigger.kind === 'tool') {
        sourceId = toolIdsByName.get(trigger.name)
      } else if (trigger.kind === 'output_section') {
        sourceId = sectionIdToNode.get(trigger.id)
      } else if (trigger.kind === 'upstream_artifact') {
        const artifactId = consumedArtifactNodeId(trigger.id)
        if (consumedArtifactExists.has(artifactId)) {
          sourceId = artifactId
        }
        label = touchpointLabel
      }
      // Lifecycle triggers intentionally fall through: rendered as in-card
      // chips by db-table-node.tsx, no edge emitted here.
      if (!sourceId) continue

      const edgeId = `e:trigger:${sourceId}->${dbId}`
      if (seenEdge.has(edgeId)) continue
      seenEdge.add(edgeId)
      edges.push({
        id: edgeId,
        source: sourceId,
        sourceHandle: AGENT_GRAPH_TRIGGER_HANDLES.source,
        target: dbId,
        targetHandle: AGENT_GRAPH_TRIGGER_HANDLES.target,
        // Custom edge type — see TriggerEdge for label-portal handling.
        type: 'trigger',
        data: { category: 'trigger', triggerLabel: label, touchpoint: entry.kind },
        className: 'agent-edge agent-edge-trigger',
        markerEnd: TRIGGER_ARROW_MARKER,
      })
    }
  }

  // 9. Stages (optional). When the authored definition declares an in-agent
  // state machine, lay each stage out as its own node and emit one
  // phase-branch edge per authored branch. The runtime gates tool exposure
  // per-stage via the same data, so the canvas mirrors what executes.
  // Emit stage edges before computing tool direct-connection handles so
  // stage→tool edges count toward the tool's target handle visibility.
  emitStageNodesAndEdges(nodes, edges, detail.workflowStructure ?? null, toolIdsByName)

  const toolNodeIds = new Set(toolIdsByName.values())
  const directConnectionHandlesByToolId = new Map<
    string,
    ToolNodeData['directConnectionHandles']
  >()
  const noteToolConnectionHandle = (
    toolId: string,
    side: keyof ToolNodeData['directConnectionHandles'],
  ) => {
    const handles =
      directConnectionHandlesByToolId.get(toolId) ?? { source: false, target: false }
    handles[side] = true
    directConnectionHandlesByToolId.set(toolId, handles)
  }

  for (const edge of edges) {
    const category = (edge.data as { category?: string } | undefined)?.category
    if (category !== 'trigger' && category !== 'stage-tool') continue
    if (toolNodeIds.has(edge.source)) {
      noteToolConnectionHandle(edge.source, 'source')
    }
    if (toolNodeIds.has(edge.target)) {
      noteToolConnectionHandle(edge.target, 'target')
    }
  }

  for (const node of nodes) {
    if (node.type !== 'tool') continue
    node.data = {
      ...node.data,
      directConnectionHandles:
        directConnectionHandlesByToolId.get(node.id) ?? { source: false, target: false },
    }
  }

  return { nodes, edges }
}

function emitStageNodesAndEdges(
  nodes: AgentGraphNode[],
  edges: AgentGraphEdge[],
  workflow: CustomAgentWorkflowStructureLike | null,
  toolIdsByName: Map<string, string>,
): void {
  if (!workflow) return
  const phases = workflow.phases ?? []
  if (phases.length === 0) return
  const startPhaseId = workflow.startPhaseId ?? phases[0]?.id ?? null
  const phaseIdSet = new Set(phases.map((phase) => phase.id))

  // Wrap every stage in a dashed group frame, just like tools live inside a
  // tool-group-frame. The agent header connects to the frame rather than to
  // an individual stage so the workflow entry reads as a single bundle.
  nodes.push({
    id: STAGE_GROUP_FRAME_NODE_ID,
    type: 'stage-group-frame',
    position: { x: 0, y: 0 },
    data: { count: phases.length },
    // Same pointer-events strategy as tool-group-frame: only the drag
    // handle area catches events so the stage cards inside stay clickable.
    style: { pointerEvents: 'none' },
  })

  for (const phase of phases) {
    const id = stageNodeId(phase.id)
    nodes.push({
      id,
      type: 'stage',
      position: { x: 0, y: 0 },
      parentId: STAGE_GROUP_FRAME_NODE_ID,
      // Layout writes child positions relative to their parent frame; React
      // Flow requires `extent: 'parent'` to anchor the relative coordinates
      // to the frame's bounds.
      extent: 'parent',
      // Stages no longer drag individually — the user moves the whole
      // column by grabbing the frame, which carries every stage with it.
      draggable: false,
      style: { pointerEvents: 'all' },
      data: {
        phase: clonePhase(phase),
        isStart: phase.id === startPhaseId,
      },
    })
  }

  // Stage → tool edges. Each stage admits a specific tool set at runtime;
  // surface that policy as edges on the canvas instead of hiding it in the
  // stage card's badge list.
  for (const phase of phases) {
    const allowed = phase.allowedTools ?? []
    if (allowed.length === 0) continue
    const sourceNodeId = stageNodeId(phase.id)
    for (const toolName of allowed) {
      const toolNodeIdValue = toolIdsByName.get(toolName)
      if (!toolNodeIdValue) continue
      edges.push({
        id: `e:stage-tool:${phase.id}->${toolName}`,
        source: sourceNodeId,
        sourceHandle: 'out',
        target: toolNodeIdValue,
        targetHandle: AGENT_GRAPH_TRIGGER_HANDLES.target,
        type: 'default',
        data: {
          category: 'stage-tool',
          sourcePhaseId: phase.id,
          toolName,
        },
        className: 'agent-edge agent-edge-stage-tool',
        markerEnd: ARROW_MARKER,
      })
    }
  }

  phases.forEach((phase, phaseIndex) => {
    const sourceNodeId = stageNodeId(phase.id)
    const branches = phase.branches ?? []
    let emittedAny = false
    branches.forEach((branch, branchIndex) => {
      if (!phaseIdSet.has(branch.targetPhaseId)) return
      const targetNodeId = stageNodeId(branch.targetPhaseId)
      edges.push({
        id: stageBranchEdgeId(phase.id, branch.targetPhaseId, branchIndex),
        source: sourceNodeId,
        sourceHandle: 'out',
        target: targetNodeId,
        targetHandle: 'in',
        type: 'phase-branch',
        data: {
          category: STAGE_BRANCH_DATA_CATEGORY,
          sourcePhaseId: phase.id,
          targetPhaseId: branch.targetPhaseId,
          branchIndex,
          condition: branch.condition,
          label: branch.label,
        },
        className: 'agent-edge agent-edge-phase-branch',
        markerEnd: ARROW_MARKER,
      })
      emittedAny = true
    })
    // Mirror runtime fall-through: when a phase has no declared branches,
    // advance_state moves to the next sequential phase once its
    // requiredChecks pass. Show the same edge on the canvas.
    if (!emittedAny) {
      const next = phases[phaseIndex + 1]
      if (next) {
        edges.push({
          id: stageBranchEdgeId(phase.id, next.id, 0),
          source: sourceNodeId,
          sourceHandle: 'out',
          target: stageNodeId(next.id),
          targetHandle: 'in',
          type: 'phase-branch',
          data: {
            category: STAGE_BRANCH_DATA_CATEGORY,
            sourcePhaseId: phase.id,
            targetPhaseId: next.id,
            branchIndex: 0,
            implicit: true,
            condition: { kind: 'always' },
          },
          className: 'agent-edge agent-edge-phase-branch',
          markerEnd: ARROW_MARKER,
        })
      }
    }
  })

  if (startPhaseId && phaseIdSet.has(startPhaseId)) {
    // Single edge from the agent header to the STAGES frame, not to a
    // specific stage card. The runtime still routes the actual entry
    // through startPhaseId — the canvas just bundles every stage under one
    // frame and connects to that, matching how tools work.
    edges.push({
      id: `e:${HEADER_NODE_ID}->${STAGE_GROUP_FRAME_NODE_ID}`,
      source: HEADER_NODE_ID,
      sourceHandle: HEADER_SOURCE_HANDLE.workflow,
      target: STAGE_GROUP_FRAME_NODE_ID,
      targetHandle: 'workflow',
      type: 'smoothstep',
      data: {
        category: 'workflow-entry',
        targetPhaseId: startPhaseId,
        targetFrame: STAGE_GROUP_FRAME_NODE_ID,
      },
      className: 'agent-edge agent-edge-workflow',
      markerEnd: ARROW_MARKER,
    })
  }
}

// Loose structural mirror of CustomAgentWorkflowStructureDto so callers can
// hand either the schema-parsed object or a hand-built mock to the builder.
// The schema enforces the precise field shapes; here we only need read access.
interface CustomAgentWorkflowStructureLike {
  startPhaseId?: string | null
  phases: CustomAgentWorkflowPhaseDto[]
}

function clonePhase(phase: CustomAgentWorkflowPhaseDto): CustomAgentWorkflowPhaseDto {
  return {
    id: phase.id,
    title: phase.title,
    description: phase.description,
    allowedTools: phase.allowedTools ? [...phase.allowedTools] : undefined,
    requiredChecks: phase.requiredChecks?.map(cloneRequiredCheck),
    retryLimit: phase.retryLimit,
    branches: phase.branches?.map((branch) => ({
      targetPhaseId: branch.targetPhaseId,
      condition: cloneCondition(branch.condition),
      label: branch.label,
    })),
  }
}

function cloneRequiredCheck(
  check: CustomAgentWorkflowGateDto,
): CustomAgentWorkflowGateDto {
  if (check.kind === 'todo_completed') {
    return { kind: 'todo_completed', todoId: check.todoId, description: check.description }
  }
  return {
    kind: 'tool_succeeded',
    toolName: check.toolName,
    minCount: check.minCount,
    description: check.description,
  }
}

function cloneCondition(
  condition: CustomAgentWorkflowBranchConditionDto,
): CustomAgentWorkflowBranchConditionDto {
  switch (condition.kind) {
    case 'always':
      return { kind: 'always' }
    case 'todo_completed':
      return { kind: 'todo_completed', todoId: condition.todoId }
    case 'tool_succeeded':
      return {
        kind: 'tool_succeeded',
        toolName: condition.toolName,
        minCount: condition.minCount,
      }
  }
}

// Default advanced fields for a fresh detail. Mirrors the validator's minimum
// requirements: workflowContract / finalResponseContract non-empty, ≥3
// example prompts, ≥3 refusal escalation cases, allowedApprovalModes includes
// 'suggest'. Effect-class flags default to "off"; the user opts in.
function defaultAdvancedFor(detail: WorkflowAgentDetailDto): AgentHeaderAdvancedFields {
  const subject = detail.header.displayName.trim() || 'this agent'
  return {
    workflowContract: detail.header.taskPurpose,
    finalResponseContract: detail.output.description,
    examplePrompts: [
      `Walk me through how ${subject} would tackle a typical assignment.`,
      `Give me a concrete example of an interaction ${subject} should handle well.`,
      `Outline a scenario where ${subject} stays in scope and produces a useful result.`,
    ],
    refusalEscalationCases: [
      `${subject} is asked to perform an action outside of its capability profile.`,
      `${subject} is asked to handle sensitive credentials or secret values.`,
      `${subject} is asked to bypass user approvals or operate without explicit consent.`,
    ],
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
  }
}

// Read advanced fields off a detail's existing toolPolicy / contracts so
// editing an existing agent preserves whatever it was already configured with.
export function deriveAdvancedFromDetail(
  detail: WorkflowAgentDetailDto,
): AgentHeaderAdvancedFields {
  const base = defaultAdvancedFor(detail)
  const policy = detail.toolPolicyDetails
  if (policy) {
    base.allowedEffectClasses = [...policy.allowedEffectClasses]
    base.deniedTools = [...policy.deniedTools]
    base.allowedToolPacks = [...policy.allowedToolPacks]
    base.deniedToolPacks = [...policy.deniedToolPacks]
    base.allowedToolGroups = [...policy.allowedToolGroups]
    base.deniedToolGroups = [...policy.deniedToolGroups]
    base.allowedMcpServers = [...policy.allowedMcpServers]
    base.deniedMcpServers = [...policy.deniedMcpServers]
    base.allowedDynamicTools = [...policy.allowedDynamicTools]
    base.deniedDynamicTools = [...policy.deniedDynamicTools]
    base.allowedSubagentRoles = [...policy.allowedSubagentRoles]
    base.deniedSubagentRoles = [...policy.deniedSubagentRoles]
    base.externalServiceAllowed = policy.externalServiceAllowed
    base.browserControlAllowed = policy.browserControlAllowed
    base.skillRuntimeAllowed = policy.skillRuntimeAllowed
    base.subagentAllowed = policy.subagentAllowed
    base.commandAllowed = policy.commandAllowed
    base.destructiveWriteAllowed = policy.destructiveWriteAllowed
  }
  return base
}

export type AgentInferredCapabilityFlags = {
  externalServiceAllowed: boolean
  browserControlAllowed: boolean
  skillRuntimeAllowed: boolean
  subagentAllowed: boolean
  commandAllowed: boolean
  destructiveWriteAllowed: boolean
}

export interface AgentInferredAdvanced {
  effectClasses: string[]
  toolGroups: string[]
  flags: AgentInferredCapabilityFlags
  // Map raw inferred items back to the connected tool name(s) that produced
  // them, so the UI can explain why an item is auto-checked.
  toolGroupReasons: Record<string, string[]>
  flagReasons: Record<keyof AgentInferredCapabilityFlags, string[]>
}

const EMPTY_INFERRED_FLAGS: AgentInferredCapabilityFlags = {
  externalServiceAllowed: false,
  browserControlAllowed: false,
  skillRuntimeAllowed: false,
  subagentAllowed: false,
  commandAllowed: false,
  destructiveWriteAllowed: false,
}

/**
 * Derive the tool groups and capability flags implied by the canvas's current
 * connections. The properties panel uses this to auto-check (and lock)
 * settings the user has already committed to by adding tools / DB writes,
 * so the rule "what you've connected on the canvas matches what you're
 * allowed to use" stays an invariant rather than a hand-maintained list.
 *
 * Each tool's `group` becomes an allowed tool group entry, and each tool's
 * `effectClass` maps to one of the granular capability flags. Cross-cutting
 * effects (a `command` tool also implies the runtime can run shell commands;
 * an `agent_delegation` tool implies subagent dispatch) are expressed here
 * once instead of being re-derived at every read site.
 */
export function inferAdvancedFromConnections(
  detail: WorkflowAgentDetailDto,
): AgentInferredAdvanced {
  const groupReasons: Record<string, string[]> = {}
  const flagReasons: Record<keyof AgentInferredCapabilityFlags, string[]> = {
    externalServiceAllowed: [],
    browserControlAllowed: [],
    skillRuntimeAllowed: [],
    subagentAllowed: [],
    commandAllowed: [],
    destructiveWriteAllowed: [],
  }
  const flags: AgentInferredCapabilityFlags = { ...EMPTY_INFERRED_FLAGS }
  const effectClasses = new Set<string>()
  const noteFlag = (key: keyof AgentInferredCapabilityFlags, source: string) => {
    flags[key] = true
    if (!flagReasons[key].includes(source)) flagReasons[key].push(source)
  }
  for (const tool of detail.tools) {
    const group = tool.group?.trim()
    const display = tool.name
    if (tool.effectClass) {
      effectClasses.add(tool.effectClass)
    }
    if (group) {
      const list = groupReasons[group] ?? (groupReasons[group] = [])
      if (!list.includes(display)) list.push(display)
    }
    switch (tool.effectClass) {
      case 'external_service':
        noteFlag('externalServiceAllowed', display)
        break
      case 'browser_control':
        noteFlag('browserControlAllowed', display)
        break
      case 'skill_runtime':
        noteFlag('skillRuntimeAllowed', display)
        break
      case 'agent_delegation':
        noteFlag('subagentAllowed', display)
        break
      case 'command':
      case 'process_control':
        noteFlag('commandAllowed', display)
        break
      case 'destructive_write':
        noteFlag('destructiveWriteAllowed', display)
        break
      default:
        break
    }
  }
  // DB writes are always destructive at the table level — even if no tool's
  // effectClass is destructive_write, the agent declares it will mutate
  // tables, so the destructive-writes flag must be on for the runtime
  // permission gate to allow the write path.
  for (const entry of detail.dbTouchpoints.writes) {
    noteFlag('destructiveWriteAllowed', `db:${entry.table}`)
  }
  return {
    effectClasses: Array.from(effectClasses).sort((a, b) => a.localeCompare(b)),
    toolGroups: Object.keys(groupReasons).sort((a, b) => a.localeCompare(b)),
    flags,
    toolGroupReasons: groupReasons,
    flagReasons,
  }
}

export function emptyInferredAdvanced(): AgentInferredAdvanced {
  return {
    effectClasses: [],
    toolGroups: [],
    flags: { ...EMPTY_INFERRED_FLAGS },
    toolGroupReasons: {},
    flagReasons: {
      externalServiceAllowed: [],
      browserControlAllowed: [],
      skillRuntimeAllowed: [],
      subagentAllowed: [],
      commandAllowed: [],
      destructiveWriteAllowed: [],
    },
  }
}

export const AGENT_GRAPH_HEADER_NODE_ID = HEADER_NODE_ID
export const AGENT_GRAPH_OUTPUT_NODE_ID = OUTPUT_NODE_ID
export const AGENT_GRAPH_HEADER_HANDLES = HEADER_SOURCE_HANDLE
export { outputSectionNodeId, consumedArtifactNodeId, dbNodeId }

// Drag-role classification for canvas handles. A 'picker' handle has no
// meaningful handle-to-handle drop — its only purpose is dropping on empty
// canvas to open a picker / auto-create an entity. A 'connection' handle is
// a normal wire endpoint, valid drop targets gated by edge-validation's
// ALLOWED_PAIRS. Anything not listed defaults to 'connection'.
//
// The map is keyed by `${nodeKind}::${handleId}::${handleType}` so the two
// no-id handles on agent-output (top target / bottom source) are unambiguous.
export type HandleDragRole = 'picker' | 'connection'

export const HANDLE_DRAG_ROLES: Readonly<Record<string, HandleDragRole>> = {
  // Agent header source handles open pickers / auto-create on pane drop.
  // header.output is also picker because the output node always exists; the
  // edge from header → agent-output is auto-implied, so dragging it onto any
  // handle would never produce a meaningful change.
  [`agent-header::${HEADER_SOURCE_HANDLE.prompt}::source`]: 'picker',
  [`agent-header::${HEADER_SOURCE_HANDLE.skills}::source`]: 'picker',
  [`agent-header::${HEADER_SOURCE_HANDLE.tool}::source`]: 'picker',
  [`agent-header::${HEADER_SOURCE_HANDLE.db}::source`]: 'picker',
  [`agent-header::${HEADER_SOURCE_HANDLE.output}::source`]: 'picker',
  // header.workflow drops onto empty canvas to auto-create a stage. Dragging
  // it onto a node has no meaningful target — every node kind that could
  // accept it (stage) is created by the drop itself.
  [`agent-header::${HEADER_SOURCE_HANDLE.workflow}::source`]: 'picker',
  // header.consumed is a target-type handle; pane drop opens the
  // consumed-artifact picker, no handle target carries semantics.
  [`agent-header::${HEADER_SOURCE_HANDLE.consumed}::target`]: 'picker',
  // agent-output bottom source — pane drop adds a new output-section. The
  // top target on agent-output is the regular receiver of header.output, so
  // it stays connection-role by virtue of not being listed.
  [`agent-output::::source`]: 'picker',
}

export function getHandleDragRole(
  nodeKind: string | undefined,
  handleId: string | null | undefined,
  handleType: 'source' | 'target',
): HandleDragRole {
  if (!nodeKind) return 'connection'
  const key = `${nodeKind}::${handleId ?? ''}::${handleType}`
  return HANDLE_DRAG_ROLES[key] ?? 'connection'
}

// Decode helpers — given a structural node id, return the part of the
// detail it refers to. Used by editing-mode mutators to translate
// updateNodeData / removeNode calls back to detail mutations.
export function decodeAgentGraphNodeId(
  id: string,
):
  | { kind: 'header' }
  | { kind: 'output' }
  | { kind: 'prompt'; index: number; promptId: string }
  | { kind: 'skills'; skillId: string }
  | { kind: 'tool'; toolName: string }
  | { kind: 'db'; touchpoint: 'read' | 'write' | 'encouraged'; table: string }
  | { kind: 'output-section'; sectionId: string }
  | { kind: 'consumed-artifact'; artifactId: string }
  | { kind: 'stage'; phaseId: string }
  | { kind: 'tool-group-frame'; groupKey: string }
  | { kind: 'lane-label' }
  | { kind: 'unknown' } {
  if (id === HEADER_NODE_ID) return { kind: 'header' }
  if (id === OUTPUT_NODE_ID) return { kind: 'output' }
  if (id.startsWith('prompt:')) {
    const [, indexRaw, ...rest] = id.split(':')
    return {
      kind: 'prompt',
      index: Number.parseInt(indexRaw, 10),
      promptId: rest.join(':'),
    }
  }
  if (id.startsWith('skills:')) {
    return { kind: 'skills', skillId: id.slice('skills:'.length) }
  }
  if (id.startsWith('tool:')) {
    return { kind: 'tool', toolName: id.slice('tool:'.length) }
  }
  if (id.startsWith('db:')) {
    const [, touchpoint, ...tableParts] = id.split(':')
    return {
      kind: 'db',
      touchpoint: touchpoint as 'read' | 'write' | 'encouraged',
      table: tableParts.join(':'),
    }
  }
  if (id.startsWith('output-section:')) {
    return { kind: 'output-section', sectionId: id.slice('output-section:'.length) }
  }
  if (id.startsWith('consumed:')) {
    return { kind: 'consumed-artifact', artifactId: id.slice('consumed:'.length) }
  }
  if (id.startsWith('workflow-phase:')) {
    return { kind: 'stage', phaseId: id.slice('workflow-phase:'.length) }
  }
  if (id.startsWith('tool-group-frame:')) {
    return { kind: 'tool-group-frame', groupKey: id.slice('tool-group-frame:'.length) }
  }
  if (id.startsWith('lane:')) {
    return { kind: 'lane-label' }
  }
  return { kind: 'unknown' }
}

export type EditingMode = 'create' | 'edit' | 'duplicate'

// Default seed prompt body. Non-empty so a freshly-created agent passes the
// "prompt body required" structural check immediately. The text reads as a
// placeholder so users know they're meant to replace it with the real system
// prompt.
const DEFAULT_PROMPT_BODY =
  'You are a helpful agent. Replace this with the system prompt that describes how the agent should behave, what it must do, and what it must avoid.'

function blankDetail(): WorkflowAgentDetailDto {
  return {
    ref: { kind: 'custom', definitionId: 'untitled-agent', version: 1 },
    header: {
      displayName: 'Untitled agent',
      shortLabel: 'Untitled',
      description: 'Custom agent built on the canvas.',
      taskPurpose:
        "Describe the agent's primary task, the steps it should take, and the boundaries it must respect.",
      scope: 'project_custom',
      lifecycleState: 'active',
      baseCapabilityProfile: 'observe_only',
      defaultApprovalMode: 'suggest',
      allowedApprovalModes: ['suggest'],
      allowPlanGate: true,
      allowVerificationGate: true,
      allowAutoCompact: true,
    },
    promptPolicy: null,
    toolPolicy: null,
    // Seed a single system prompt with non-empty body so the structural
    // validator doesn't fire the "no prompts" / "empty body" diagnostics on
    // first paint. The user is expected to replace the body — the placeholder
    // text reads like a TODO so it's obvious.
    prompts: [
      {
        id: 'system_prompt',
        label: 'System prompt',
        role: 'system',
        source: 'custom',
        policy: null,
        body: DEFAULT_PROMPT_BODY,
      },
    ],
    tools: [],
    dbTouchpoints: { reads: [], writes: [], encouraged: [] },
    output: {
      contract: 'answer',
      label: 'Final response',
      description:
        'Replace this with what a successful final response from the agent must include.',
      sections: [],
    },
    consumes: [],
    attachedSkills: [],
  }
}

/**
 * Build the initial graph for editing/authoring. For 'create' mode this
 * returns a minimal graph with just header + output. For 'edit' / 'duplicate'
 * this defers to {@link buildAgentGraph} so the editing canvas inherits the
 * exact same node structure used during viewing.
 *
 * The duplicate case adjusts the header DTO so the user sees a "(copy)"
 * label and a derived definition id is generated on save.
 */
export function buildAgentGraphForEditing(
  mode: EditingMode,
  detail: WorkflowAgentDetailDto | null,
): { graph: AgentGraph; detail: WorkflowAgentDetailDto } {
  if (mode === 'create' || !detail) {
    const blank = blankDetail()
    return { graph: buildAgentGraph(blank), detail: blank }
  }
  if (mode === 'duplicate') {
    const next: WorkflowAgentDetailDto = {
      ...detail,
      header: {
        ...detail.header,
        displayName: `${detail.header.displayName} (copy)`.slice(0, 80),
        shortLabel: `${detail.header.shortLabel} copy`.slice(0, 24),
        scope:
          detail.header.scope === 'global_custom' ? 'global_custom' : 'project_custom',
      },
    }
    return { graph: buildAgentGraph(next), detail: next }
  }
  return { graph: buildAgentGraph(detail), detail }
}
