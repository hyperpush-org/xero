import type { Edge, Node } from '@xyflow/react'

import type { AgentGraphNodeKind } from './build-agent-graph'
import {
  AGENT_GRAPH_HEADER_NODE_ID,
  AGENT_GRAPH_OUTPUT_NODE_ID,
  type AgentHeaderFlowNode,
  type ConsumedArtifactFlowNode,
  type DbTableFlowNode,
  type PromptFlowNode,
  type ToolFlowNode,
} from './build-agent-graph'

export const INVALID_EDGE_CLASSNAME = 'agent-edge-invalid'

export interface EdgeValidationDiagnostic {
  edgeId: string
  message: string
}

// Allowed (source kind → target kind) pairings. Anything not listed here is
// flagged as semantically invalid in edit mode and rendered red. Mirrors the
// edges that {@link buildAgentGraph} emits when wiring a viewing graph from a
// WorkflowAgentDetailDto.
const ALLOWED_PAIRS: ReadonlyArray<readonly [AgentGraphNodeKind, AgentGraphNodeKind]> = [
  // Header drives the major lanes
  ['agent-header', 'prompt'],
  ['agent-header', 'tool'],
  ['agent-header', 'db-table'],
  ['agent-header', 'agent-output'],
  // Tools produce content for output sections; sections live under the output
  ['tool', 'output-section'],
  ['agent-output', 'output-section'],
  // DB touchpoints can be triggered by tools, sections, or upstream artifacts
  ['tool', 'db-table'],
  ['output-section', 'db-table'],
  ['consumed-artifact', 'db-table'],
  // Consumed artifacts feed into the agent header
  ['consumed-artifact', 'agent-header'],
]

// Layout-chrome node kinds. buildAgentGraph emits these (lane labels and
// per-tool-group frames with header-→-frame edges) purely for visual
// scaffolding; they don't represent user-mutable relationships and shouldn't
// be subjected to the structural pair check above.
const LAYOUT_CHROME_KINDS = new Set<string>(['lane-label', 'tool-group-frame'])

const ALLOWED_LOOKUP = new Set(
  ALLOWED_PAIRS.map(([source, target]) => `${source}->${target}`),
)

export function isEdgeAllowed(
  sourceKind: AgentGraphNodeKind | undefined,
  targetKind: AgentGraphNodeKind | undefined,
): boolean {
  if (!sourceKind || !targetKind) return false
  return ALLOWED_LOOKUP.has(`${sourceKind}->${targetKind}`)
}

export function describeInvalidEdge(
  sourceKind: AgentGraphNodeKind | undefined,
  targetKind: AgentGraphNodeKind | undefined,
): string {
  if (!sourceKind || !targetKind) {
    return 'Edge endpoints could not be resolved.'
  }
  return `Edges from ${humanKind(sourceKind)} to ${humanKind(targetKind)} aren't supported.`
}

export interface ValidatedEdges {
  invalidEdgeIds: Set<string>
  diagnostics: EdgeValidationDiagnostic[]
}

export function validateEdges(
  nodes: readonly Node[],
  edges: readonly Edge[],
): ValidatedEdges {
  const kindById = new Map<string, AgentGraphNodeKind | undefined>()
  for (const node of nodes) {
    kindById.set(node.id, node.type as AgentGraphNodeKind | undefined)
  }
  const invalidEdgeIds = new Set<string>()
  const diagnostics: EdgeValidationDiagnostic[] = []
  for (const edge of edges) {
    const sourceKind = kindById.get(edge.source)
    const targetKind = kindById.get(edge.target)
    // Skip edges that touch synthetic layout chrome (lane labels,
    // tool-group-frames). They're emitted by buildAgentGraph for visual
    // scaffolding only and aren't user-mutable.
    if (
      (sourceKind && LAYOUT_CHROME_KINDS.has(sourceKind)) ||
      (targetKind && LAYOUT_CHROME_KINDS.has(targetKind))
    ) {
      continue
    }
    if (isEdgeAllowed(sourceKind, targetKind)) continue
    invalidEdgeIds.add(edge.id)
    diagnostics.push({
      edgeId: edge.id,
      message: describeInvalidEdge(sourceKind, targetKind),
    })
  }
  return { invalidEdgeIds, diagnostics }
}

export function applyEdgeValidationClasses<T extends Edge>(
  edges: readonly T[],
  invalidEdgeIds: ReadonlySet<string>,
): T[] {
  return edges.map((edge) => {
    const isInvalid = invalidEdgeIds.has(edge.id)
    const tokens = edge.className?.split(/\s+/).filter(Boolean) ?? []
    const hasToken = tokens.includes(INVALID_EDGE_CLASSNAME)
    if (isInvalid === hasToken) return edge
    const next = isInvalid
      ? [...tokens, INVALID_EDGE_CLASSNAME]
      : tokens.filter((token) => token !== INVALID_EDGE_CLASSNAME)
    return { ...edge, className: next.length > 0 ? next.join(' ') : undefined }
  })
}

const KIND_LABELS: Record<AgentGraphNodeKind, string> = {
  'agent-header': 'agent header',
  prompt: 'prompt',
  tool: 'tool',
  'db-table': 'DB table',
  'agent-output': 'output',
  'output-section': 'output section',
  'consumed-artifact': 'consumed artifact',
}

function humanKind(kind: AgentGraphNodeKind): string {
  return KIND_LABELS[kind] ?? kind
}

// Structural diagnostics that aren't about edge pairings — empty prompt
// bodies, unpicked placeholder tools/tables/artifacts, missing displayName,
// etc. Surfaced through the same diagnostics panel and Save-disabled flow as
// the edge validator so the user gets a single coherent error list.
export interface StructuralDiagnostic {
  code: string
  message: string
  path: string
}

interface ValidateStructureInput {
  nodes: readonly Node[]
}

export function validateStructure({ nodes }: ValidateStructureInput): StructuralDiagnostic[] {
  const diagnostics: StructuralDiagnostic[] = []
  const header = nodes.find((node) => node.id === AGENT_GRAPH_HEADER_NODE_ID) as
    | AgentHeaderFlowNode
    | undefined
  if (header) {
    const { displayName, shortLabel, description } = header.data.header
    if (!displayName.trim()) {
      diagnostics.push({
        code: 'header_display_name_required',
        message: 'Agent display name is required.',
        path: 'header.displayName',
      })
    }
    if (!shortLabel.trim()) {
      diagnostics.push({
        code: 'header_short_label_required',
        message: 'Agent short label is required.',
        path: 'header.shortLabel',
      })
    }
    if (!description.trim()) {
      diagnostics.push({
        code: 'header_description_required',
        message: 'Agent description is required.',
        path: 'header.description',
      })
    }
    const advanced = header.data.advanced
    if (advanced) {
      if (!advanced.workflowContract.trim()) {
        diagnostics.push({
          code: 'workflow_contract_required',
          message: 'Workflow contract (Advanced) must be non-empty.',
          path: 'advanced.workflowContract',
        })
      }
      if (!advanced.finalResponseContract.trim()) {
        diagnostics.push({
          code: 'final_response_contract_required',
          message: 'Final response contract (Advanced) must be non-empty.',
          path: 'advanced.finalResponseContract',
        })
      }
    }
  } else {
    diagnostics.push({
      code: 'header_missing',
      message: 'Agent header node is missing.',
      path: 'header',
    })
  }

  // Output: a saved snapshot needs at least the description; the snapshot
  // builder falls back to a placeholder, but the user authoring the agent
  // should see this as a required step rather than getting an autopadded
  // value silently.
  const outputNode = nodes.find((node) => node.id === AGENT_GRAPH_OUTPUT_NODE_ID)
  if (!outputNode) {
    diagnostics.push({
      code: 'output_missing',
      message: 'Agent output node is missing.',
      path: 'output',
    })
  }

  // At least one prompt with a non-empty body is required for the agent to
  // run meaningfully. The validator doesn't enforce this, but the agent will
  // be useless without it.
  const prompts = nodes.filter(
    (node): node is PromptFlowNode => node.type === 'prompt',
  )
  if (prompts.length === 0) {
    diagnostics.push({
      code: 'prompts_required',
      message: 'Add at least one prompt — the agent has nothing to say without one.',
      path: 'prompts',
    })
  } else {
    prompts.forEach((node, index) => {
      if (!node.data.prompt.body.trim()) {
        diagnostics.push({
          code: 'prompt_body_required',
          message: `Prompt #${index + 1} has an empty body.`,
          path: `prompts[${index}].body`,
        })
      }
    })
  }

  // Tool / table / artifact nodes that are still unfilled placeholders (the
  // user dragged them onto the canvas but never picked from the catalog).
  // Use strict patterns — `^tool_\d+$` etc. — because real registry tools
  // can legitimately start with `tool_` (e.g. `tool_access`, `tool_search`).
  const TOOL_PLACEHOLDER = /^tool_\d+$/
  const TABLE_PLACEHOLDER = /^table_\d+$/
  const ARTIFACT_PLACEHOLDER = /^artifact_\d+$/
  const tools = nodes.filter((node): node is ToolFlowNode => node.type === 'tool')
  tools.forEach((node, index) => {
    const name = node.data.tool.name
    if (!name || TOOL_PLACEHOLDER.test(name)) {
      diagnostics.push({
        code: 'tool_unpicked',
        message: `Tool #${index + 1} hasn't been picked from the registry.`,
        path: `tools[${index}].name`,
      })
    }
  })
  const dbs = nodes.filter((node): node is DbTableFlowNode => node.type === 'db-table')
  dbs.forEach((node, index) => {
    if (!node.data.table || TABLE_PLACEHOLDER.test(node.data.table)) {
      diagnostics.push({
        code: 'db_table_unpicked',
        message: `DB table #${index + 1} hasn't been picked.`,
        path: `dbTouchpoints[${index}].table`,
      })
    }
  })
  const consumes = nodes.filter(
    (node): node is ConsumedArtifactFlowNode => node.type === 'consumed-artifact',
  )
  consumes.forEach((node, index) => {
    const id = node.data.artifact.id
    if (!id || ARTIFACT_PLACEHOLDER.test(id)) {
      diagnostics.push({
        code: 'consumed_artifact_unpicked',
        message: `Consumed artifact #${index + 1} hasn't been picked.`,
        path: `consumes[${index}].id`,
      })
    }
  })

  return diagnostics
}
