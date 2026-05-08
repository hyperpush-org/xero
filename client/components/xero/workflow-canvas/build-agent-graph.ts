import type { Edge, Node } from '@xyflow/react'

import type {
  AgentConsumedArtifactDto,
  AgentDbTouchpointDetailDto,
  AgentDbTouchpointKindDto,
  AgentHeaderDto,
  AgentOutputContractDto,
  AgentOutputSectionDto,
  AgentPromptDto,
  AgentToolSummaryDto,
  AgentTriggerRefDto,
  WorkflowAgentDetailDto,
} from '@/src/lib/xero-model/workflow-agents'

export type AgentGraphNodeKind =
  | 'agent-header'
  | 'prompt'
  | 'tool'
  | 'db-table'
  | 'agent-output'
  | 'output-section'
  | 'consumed-artifact'

export interface AgentHeaderSummaryCounts {
  prompts: number
  tools: number
  dbTables: number
  outputSections: number
  consumes: number
}

export interface AgentHeaderNodeData extends Record<string, unknown> {
  header: AgentHeaderDto
  summary: AgentHeaderSummaryCounts
}

export interface PromptNodeData extends Record<string, unknown> {
  prompt: AgentPromptDto
}

export interface ToolNodeData extends Record<string, unknown> {
  tool: AgentToolSummaryDto
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

export interface LaneLabelNodeData extends Record<string, unknown> {
  label: string
  count: number
}

export interface ToolGroupFrameNodeData extends Record<string, unknown> {
  label: string
  count: number
}

export type AgentHeaderFlowNode = Node<AgentHeaderNodeData, 'agent-header'>
export type PromptFlowNode = Node<PromptNodeData, 'prompt'>
export type ToolFlowNode = Node<ToolNodeData, 'tool'>
export type DbTableFlowNode = Node<DbTableNodeData, 'db-table'>
export type OutputFlowNode = Node<OutputNodeData, 'agent-output'>
export type OutputSectionFlowNode = Node<OutputSectionNodeData, 'output-section'>
export type ConsumedArtifactFlowNode = Node<ConsumedArtifactNodeData, 'consumed-artifact'>
export type LaneLabelFlowNode = Node<LaneLabelNodeData, 'lane-label'>
export type ToolGroupFrameFlowNode = Node<ToolGroupFrameNodeData, 'tool-group-frame'>

export type AgentGraphNode =
  | AgentHeaderFlowNode
  | PromptFlowNode
  | ToolFlowNode
  | DbTableFlowNode
  | OutputFlowNode
  | OutputSectionFlowNode
  | ConsumedArtifactFlowNode
  | LaneLabelFlowNode
  | ToolGroupFrameFlowNode

export type AgentGraphEdge = Edge

export interface AgentGraph {
  nodes: AgentGraphNode[]
  edges: AgentGraphEdge[]
}

const HEADER_NODE_ID = 'agent-header'
const OUTPUT_NODE_ID = 'agent-output'

const HEADER_SOURCE_HANDLE = {
  prompt: 'prompts',
  tool: 'tools',
  db: 'db',
  output: 'output',
  consumed: 'consumed',
} as const

function promptNodeId(prompt: AgentPromptDto, index: number): string {
  return `prompt:${index}:${prompt.id}`
}

function toolNodeId(tool: AgentToolSummaryDto): string {
  return `tool:${tool.name}`
}

export function toolGroupFrameNodeId(groupKey: string): string {
  return `tool-group-frame:${groupKey}`
}

const TOOL_GROUP_LABEL_OVERRIDES: Record<string, string> = {
  mcp_invoke: 'MCP',
  external_chain_observe: 'Chain Observe',
  external_chain_simulation: 'Chain Simulation',
  external_chain_control: 'Chain Control',
  external_capability_observe: 'External Capability',
  system_diagnostics_observe: 'System Diagnostics',
  project_context_write: 'Project Context',
  agent_definition_state: 'Agent Definition',
  coordination_state: 'Coordination',
  process_manager: 'Process Manager',
  registry_control: 'Registry',
}

export function humanizeToolGroupKey(key: string): string {
  if (TOOL_GROUP_LABEL_OVERRIDES[key]) return TOOL_GROUP_LABEL_OVERRIDES[key]
  if (!key) return 'Other'
  return key
    .split(/[._-]+|(?=[A-Z])/)
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
    .join(' ')
}

interface ToolGroupBucket {
  key: string
  label: string
  tools: AgentToolSummaryDto[]
}

/**
 * Partition tool DTOs by their `group` field. Within each bucket, tools keep
 * the input order (already barycenter-sorted upstream); buckets themselves
 * are sorted by humanised label so on-screen ordering is predictable.
 */
function partitionToolDtosByGroup(tools: AgentToolSummaryDto[]): ToolGroupBucket[] {
  const buckets = new Map<string, AgentToolSummaryDto[]>()
  for (const tool of tools) {
    const key = tool.group?.trim() || 'other'
    const arr = buckets.get(key) ?? []
    arr.push(tool)
    buckets.set(key, arr)
  }
  return Array.from(buckets.entries())
    .map(([key, items]) => ({
      key,
      label: humanizeToolGroupKey(key),
      tools: items,
    }))
    .sort((a, b) => a.label.localeCompare(b.label))
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

interface OrderedTouchpoint {
  detail: AgentDbTouchpointDetailDto
  kind: DbTableTouchpointKind
}

/**
 * Bucket touchpoints by priority (writes → reads → encouraged) and de-duplicate
 * by table name across kinds, so a table that the agent both reads and writes
 * is rendered as a single write node — the higher-impact relationship wins.
 */
function dbTouchpointsByPriority(
  reads: AgentDbTouchpointDetailDto[],
  writes: AgentDbTouchpointDetailDto[],
  encouraged: AgentDbTouchpointDetailDto[],
): OrderedTouchpoint[] {
  const ordered: OrderedTouchpoint[] = []
  const seenTables = new Set<string>()

  const push = (detail: AgentDbTouchpointDetailDto, kind: DbTableTouchpointKind) => {
    if (seenTables.has(detail.table)) return
    seenTables.add(detail.table)
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

  // 1. Header
  const dbTableCount = new Set<string>([
    ...detail.dbTouchpoints.reads.map((d) => d.table),
    ...detail.dbTouchpoints.writes.map((d) => d.table),
    ...detail.dbTouchpoints.encouraged.map((d) => d.table),
  ]).size
  nodes.push({
    id: HEADER_NODE_ID,
    type: 'agent-header',
    position: { x: 0, y: 0 },
    data: {
      header: detail.header,
      summary: {
        prompts: detail.prompts.length,
        tools: detail.tools.length,
        dbTables: dbTableCount,
        outputSections: detail.output.sections.length,
        consumes: detail.consumes.length,
      },
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
    })
  })

  // 3. Tools and 4. DBs require coordinated ordering: the barycenter heuristic
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
  // Tools are partitioned into category frames (one frame per tool.group).
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
      data: { label: bucket.label, count: bucket.tools.length },
    })
    edges.push({
      id: `e:header->${frameId}`,
      source: HEADER_NODE_ID,
      sourceHandle: HEADER_SOURCE_HANDLE.tool,
      target: frameId,
      type: 'smoothstep',
      data: { category: 'tool' },
      className: 'agent-edge agent-edge-tool',
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
        data: { tool },
      })
    }
  }

  const dbNodeIdByTable = new Map<string, string>()
  for (const entry of dbEntries) {
    const id = dbNodeId(entry.detail.table, entry.kind)
    dbNodeIdByTable.set(entry.detail.table, id)
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
    })
  }

  // 5. Output contract (parent) + sections (children).
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
    })
    // Functional cross-edge: tool → output-section, when authored.
    for (const toolName of section.producedByTools) {
      const toolId = toolIdsByName.get(toolName)
      if (!toolId) continue
      edges.push({
        id: `e:trigger:${toolId}->${id}`,
        source: toolId,
        target: id,
        type: 'default',
        data: { category: 'trigger' },
        className: 'agent-edge agent-edge-trigger',
      })
    }
  }

  // 6. Consumed artifacts (left of header).
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
      type: 'smoothstep',
      data: { category: 'consumed' },
      className: 'agent-edge agent-edge-consume',
    })
  }

  // 7. Cross-edges driven by db touchpoint triggers. Tool triggers connect
  // their tool node to the db node; output-section triggers connect their
  // section node to the db node. Lifecycle triggers stay as chips on the card.
  for (const entry of dbEntries) {
    const dbId = dbNodeIdByTable.get(entry.detail.table)
    if (!dbId) continue
    const seenEdge = new Set<string>()
    for (const trigger of entry.detail.triggers) {
      let sourceId: string | undefined
      if (trigger.kind === 'tool') {
        sourceId = toolIdsByName.get(trigger.name)
      } else if (trigger.kind === 'output_section') {
        sourceId = sectionIdToNode.get(trigger.id)
      }
      if (!sourceId) continue
      const edgeId = `e:trigger:${sourceId}->${dbId}`
      if (seenEdge.has(edgeId)) continue
      seenEdge.add(edgeId)
      edges.push({
        id: edgeId,
        source: sourceId,
        target: dbId,
        type: 'default',
        data: { category: 'trigger' },
        className: 'agent-edge agent-edge-trigger',
      })
    }
  }

  return { nodes, edges }
}

export const AGENT_GRAPH_HEADER_NODE_ID = HEADER_NODE_ID
export const AGENT_GRAPH_OUTPUT_NODE_ID = OUTPUT_NODE_ID
export const AGENT_GRAPH_HEADER_HANDLES = HEADER_SOURCE_HANDLE
export { outputSectionNodeId, consumedArtifactNodeId, dbNodeId }
