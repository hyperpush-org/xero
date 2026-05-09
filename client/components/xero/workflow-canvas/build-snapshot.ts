import type { Edge } from '@xyflow/react'

import {
  AGENT_DEFINITION_SCHEMA,
  AGENT_DEFINITION_SCHEMA_VERSION,
} from '@/src/lib/xero-model/agent-definition'

import type {
  AgentGraphNode,
  AgentHeaderAdvancedFields,
  AgentHeaderFlowNode,
  AgentInferredAdvanced,
  ConsumedArtifactFlowNode,
  DbTableFlowNode,
  OutputFlowNode,
  OutputSectionFlowNode,
  PromptFlowNode,
  ToolFlowNode,
} from './build-agent-graph'

const MIN_EXAMPLES = 3
const MIN_ESCALATIONS = 3

export interface BuildSnapshotResult {
  snapshot: Record<string, unknown>
  definitionId: string
}

function ensureId(seed: string): string {
  const slug = seed
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 80)
  return slug.length > 0 ? slug : 'custom_agent'
}

function trimAll(values: readonly string[]): string[] {
  return values.map((value) => value.trim()).filter((value) => value.length > 0)
}

function padExamples(values: readonly string[], displayName: string): string[] {
  const trimmed = trimAll(values)
  const subject = displayName.trim() || 'this agent'
  const fallbacks = [
    `Walk me through how ${subject} would tackle a typical assignment.`,
    `Give me a concrete example of an interaction ${subject} should handle well.`,
    `Outline a scenario where ${subject} stays in scope and produces a useful result.`,
  ]
  let i = 0
  while (trimmed.length < MIN_EXAMPLES && i < fallbacks.length) {
    trimmed.push(fallbacks[i++])
  }
  return trimmed
}

function padEscalations(values: readonly string[], displayName: string): string[] {
  const trimmed = trimAll(values)
  const subject = displayName.trim() || 'this agent'
  const fallbacks = [
    `${subject} is asked to perform an action outside of its capability profile.`,
    `${subject} is asked to handle sensitive credentials or secret values.`,
    `${subject} is asked to bypass user approvals or operate without explicit consent.`,
  ]
  let i = 0
  while (trimmed.length < MIN_ESCALATIONS && i < fallbacks.length) {
    trimmed.push(fallbacks[i++])
  }
  return trimmed
}

function isHeader(node: AgentGraphNode): node is AgentHeaderFlowNode {
  return node.type === 'agent-header'
}

function isPrompt(node: AgentGraphNode): node is PromptFlowNode {
  return node.type === 'prompt'
}

function isTool(node: AgentGraphNode): node is ToolFlowNode {
  return node.type === 'tool'
}

function isOutput(node: AgentGraphNode): node is OutputFlowNode {
  return node.type === 'agent-output'
}

function isOutputSection(node: AgentGraphNode): node is OutputSectionFlowNode {
  return node.type === 'output-section'
}

function isDbTable(node: AgentGraphNode): node is DbTableFlowNode {
  return node.type === 'db-table'
}

function isConsumed(node: AgentGraphNode): node is ConsumedArtifactFlowNode {
  return node.type === 'consumed-artifact'
}

export interface BuildSnapshotOptions {
  // The original definition id when editing/duplicating an existing agent.
  // Null/undefined for fresh creates — a slug is derived from displayName.
  initialDefinitionId?: string | null
  // Optional fields the canvas surfaces only via the agent-header form. The
  // viewing canvas does not render these so they're passed in here.
  examplePrompts?: readonly string[]
  refusalEscalationCases?: readonly string[]
  workflowContract?: string
  finalResponseContract?: string
  scope?: 'project_custom' | 'global_custom'
}

/**
 * Serialize an authoring graph (using the unified AgentGraphNode shape) into
 * the snapshot payload accepted by the Tauri save_agent_definition /
 * update_agent_definition commands. Reads DTOs straight out of node.data.*.
 */
export function buildSnapshotFromGraph(
  nodes: readonly AgentGraphNode[],
  edges: readonly Edge[],
  options: BuildSnapshotOptions = {},
): BuildSnapshotResult {
  const header = nodes.find(isHeader)
  if (!header) {
    throw new Error('Authoring graph is missing the agent-header node.')
  }
  const output = nodes.find(isOutput)

  const definitionId =
    options.initialDefinitionId ?? ensureId(header.data.header.displayName)

  const prompts = nodes.filter(isPrompt).map((node, index) => {
    const dto = node.data.prompt
    return {
      id: dto.id.trim() || `prompt-${index + 1}`,
      label: dto.label.trim() || `Prompt ${index + 1}`,
      role: dto.role,
      source: dto.source || 'custom',
      body: dto.body,
    }
  })

  const tools = nodes.filter(isTool).map((node) => {
    const dto = node.data.tool
    return {
      name: dto.name.trim(),
      group: dto.group.trim() || 'core',
      description: dto.description,
      effectClass: dto.effectClass,
      riskClass: dto.riskClass || 'standard',
      tags: [...dto.tags],
      schemaFields: [...dto.schemaFields],
      examples: [...dto.examples],
    }
  })

  // Reconstruct producedByTools edges from the edge list. Manual smoothstep
  // edges drawn between tool and output-section nodes feed the section's
  // producedByTools array, mirroring the relationship the viewing canvas
  // derives the other direction.
  const sections = nodes.filter(isOutputSection)
  const sectionEdgeBySection = new Map<string, string[]>()
  const toolNodeIdToName = new Map<string, string>()
  for (const node of nodes) {
    if (isTool(node)) {
      toolNodeIdToName.set(node.id, node.data.tool.name.trim())
    }
  }
  for (const edge of edges) {
    const toolName = toolNodeIdToName.get(edge.source)
    if (!toolName) continue
    const sectionNode = sections.find((node) => node.id === edge.target)
    if (!sectionNode) continue
    const list = sectionEdgeBySection.get(sectionNode.id) ?? []
    if (!list.includes(toolName)) list.push(toolName)
    sectionEdgeBySection.set(sectionNode.id, list)
  }

  const outputSections = sections.map((node, index) => {
    const dto = node.data.section
    return {
      id: dto.id.trim() || `section-${index + 1}`,
      label: dto.label.trim() || `Section ${index + 1}`,
      description: dto.description,
      emphasis: dto.emphasis,
      producedByTools: sectionEdgeBySection.get(node.id) ?? [...dto.producedByTools],
    }
  })

  const dbReads: ReturnType<typeof toDbTouchpoint>[] = []
  const dbWrites: ReturnType<typeof toDbTouchpoint>[] = []
  const dbEncouraged: ReturnType<typeof toDbTouchpoint>[] = []
  for (const node of nodes.filter(isDbTable)) {
    const touchpoint = toDbTouchpoint(node)
    if (node.data.touchpoint === 'read') dbReads.push(touchpoint)
    else if (node.data.touchpoint === 'write') dbWrites.push(touchpoint)
    else dbEncouraged.push(touchpoint)
  }

  const consumes = nodes.filter(isConsumed).map((node, index) => {
    const dto = node.data.artifact
    return {
      id: dto.id.trim() || `artifact-${index + 1}`,
      label: dto.label.trim() || `Artifact ${index + 1}`,
      description: dto.description,
      sourceAgent: dto.sourceAgent || 'ask',
      contract: dto.contract || 'answer',
      sections: [...dto.sections],
      required: dto.required,
    }
  })

  const headerDto = header.data.header
  const advanced = header.data.advanced
  const description = headerDto.description.trim()
  // Workflow / final-response contracts have three sources, in priority order:
  // 1. Explicit caller override (rare — only the agent_create AI flow uses it).
  // 2. The advanced-panel value the user typed on the header.
  // 3. Fallback derived from the existing header.taskPurpose / output text.
  const workflowContract = (
    options.workflowContract ??
    advanced.workflowContract ??
    headerDto.taskPurpose
  ).trim()
  const finalResponseContract = (
    options.finalResponseContract ??
    advanced.finalResponseContract ??
    output?.data.output.description ??
    ''
  ).trim()
  const scope = options.scope ?? (
    headerDto.scope === 'global_custom' ? 'global_custom' : 'project_custom'
  )

  const baseCapabilityProfile =
    headerDto.baseCapabilityProfile === 'harness_test'
      ? 'observe_only'
      : headerDto.baseCapabilityProfile

  // allowedApprovalModes must always include 'suggest' (validator requirement).
  // Modes higher than 'suggest' are gated by the base profile elsewhere.
  const allowedApprovalModes = Array.from(
    new Set([
      'suggest',
      ...(headerDto.allowedApprovalModes ?? []).filter((mode) => mode === 'suggest' || mode === 'auto_edit' || mode === 'yolo'),
    ]),
  )

  // toolPolicy is required by the backend validator. We always emit the object
  // form, so granular tool/group/effect-flag changes flow through. allowedTools
  // is derived from the picked tool nodes (their names live in the registry).
  const allowedTools = tools.map((tool) => tool.name).filter((name) => name.length > 0)
  // Union the user's manual advanced selections with the canvas-implied ones
  // so an agent always declares the groups / capability flags it actually
  // needs to run. Inference comes from the snapshot's own tools and DB
  // writes — the same data the runtime will see — so there is no drift
  // between "what the canvas shows" and "what the saved policy declares".
  const inferred = inferFromSnapshotPieces(tools, dbWrites)
  const toolPolicy = buildToolPolicy(advanced, allowedTools, inferred)

  // Examples / escalations: prefer caller-supplied; otherwise use the user's
  // advanced-panel entries; if either is short, pad with auto-generated copies
  // so the validator's ≥3 rule is satisfied without surprise UX.
  const examplePrompts = padExamples(
    options.examplePrompts ?? trimAll(advanced.examplePrompts ?? []),
    headerDto.displayName,
  )
  const refusalEscalationCases = padEscalations(
    options.refusalEscalationCases ?? trimAll(advanced.refusalEscalationCases ?? []),
    headerDto.displayName,
  )

  const snapshot: Record<string, unknown> = {
    schema: AGENT_DEFINITION_SCHEMA,
    schemaVersion: AGENT_DEFINITION_SCHEMA_VERSION,
    id: definitionId,
    displayName: headerDto.displayName.trim() || 'Untitled agent',
    shortLabel: headerDto.shortLabel.trim() || 'Untitled',
    description,
    taskPurpose: workflowContract || description,
    scope,
    lifecycleState: 'active',
    baseCapabilityProfile,
    defaultApprovalMode: headerDto.defaultApprovalMode,
    allowedApprovalModes,
    toolPolicy,
    workflowContract,
    finalResponseContract,
    examplePrompts,
    refusalEscalationCases,
    output: output
      ? {
          contract: output.data.output.contract,
          label: output.data.output.label,
          description: output.data.output.description,
          sections: outputSections,
        }
      : {
          contract: 'answer',
          label: 'Final response',
          description: 'Describe what a successful response includes.',
          sections: outputSections,
        },
    prompts,
    tools,
    dbTouchpoints: {
      reads: dbReads,
      writes: dbWrites,
      encouraged: dbEncouraged,
    },
    consumes,
  }

  return { snapshot, definitionId }
}

function buildToolPolicy(
  advanced: AgentHeaderAdvancedFields,
  allowedTools: readonly string[],
  inferred: AgentInferredAdvanced,
): Record<string, unknown> {
  // Object-form policy. We omit empty arrays/false flags only when noise would
  // be confusing — the backend tolerates either shape, but explicit defaults
  // make the saved JSON easier to read in version control.
  const allowedToolGroups = Array.from(
    new Set([...advanced.allowedToolGroups, ...inferred.toolGroups]),
  ).sort((a, b) => a.localeCompare(b))
  const allowedEffectClasses = Array.from(
    new Set([...advanced.allowedEffectClasses, ...inferred.effectClasses]),
  ).sort((a, b) => a.localeCompare(b))
  const policy: Record<string, unknown> = {
    allowedTools: [...allowedTools],
    deniedTools: [...advanced.deniedTools],
    allowedToolPacks: [...advanced.allowedToolPacks],
    deniedToolPacks: [...advanced.deniedToolPacks],
    allowedToolGroups,
    deniedToolGroups: [...advanced.deniedToolGroups],
    allowedEffectClasses,
  }
  if (advanced.externalServiceAllowed || inferred.flags.externalServiceAllowed) {
    policy.externalServiceAllowed = true
  }
  if (advanced.browserControlAllowed || inferred.flags.browserControlAllowed) {
    policy.browserControlAllowed = true
  }
  if (advanced.skillRuntimeAllowed || inferred.flags.skillRuntimeAllowed) {
    policy.skillRuntimeAllowed = true
  }
  if (advanced.subagentAllowed || inferred.flags.subagentAllowed) {
    policy.subagentAllowed = true
  }
  if (advanced.commandAllowed || inferred.flags.commandAllowed) {
    policy.commandAllowed = true
  }
  if (advanced.destructiveWriteAllowed || inferred.flags.destructiveWriteAllowed) {
    policy.destructiveWriteAllowed = true
  }
  return policy
}

// Lightweight re-derivation of inferAdvancedFromConnections that operates on
// the already-mapped snapshot pieces. Avoids reconstructing a full detail DTO
// just to walk its tools and dbWrites.
function inferFromSnapshotPieces(
  tools: readonly { name: string; group: string; effectClass: string }[],
  dbWrites: readonly { table: string }[],
): AgentInferredAdvanced {
  const groupReasons: Record<string, string[]> = {}
  const effectClasses = new Set<string>()
  const flagReasons = {
    externalServiceAllowed: [] as string[],
    browserControlAllowed: [] as string[],
    skillRuntimeAllowed: [] as string[],
    subagentAllowed: [] as string[],
    commandAllowed: [] as string[],
    destructiveWriteAllowed: [] as string[],
  }
  const flags = {
    externalServiceAllowed: false,
    browserControlAllowed: false,
    skillRuntimeAllowed: false,
    subagentAllowed: false,
    commandAllowed: false,
    destructiveWriteAllowed: false,
  }
  const noteFlag = (key: keyof typeof flags, source: string) => {
    flags[key] = true
    if (!flagReasons[key].includes(source)) flagReasons[key].push(source)
  }
  for (const tool of tools) {
    if (tool.effectClass) {
      effectClasses.add(tool.effectClass)
    }
    if (tool.group) {
      const list = groupReasons[tool.group] ?? (groupReasons[tool.group] = [])
      if (!list.includes(tool.name)) list.push(tool.name)
    }
    switch (tool.effectClass) {
      case 'external_service':
        noteFlag('externalServiceAllowed', tool.name)
        break
      case 'browser_control':
        noteFlag('browserControlAllowed', tool.name)
        break
      case 'skill_runtime':
        noteFlag('skillRuntimeAllowed', tool.name)
        break
      case 'agent_delegation':
        noteFlag('subagentAllowed', tool.name)
        break
      case 'command':
      case 'process_control':
        noteFlag('commandAllowed', tool.name)
        break
      case 'destructive_write':
        noteFlag('destructiveWriteAllowed', tool.name)
        break
      default:
        break
    }
  }
  for (const entry of dbWrites) {
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

function toDbTouchpoint(node: DbTableFlowNode) {
  return {
    table: node.data.table.trim(),
    kind: node.data.touchpoint,
    purpose: node.data.purpose,
    triggers: [...node.data.triggers],
    columns: [...node.data.columns],
  }
}
