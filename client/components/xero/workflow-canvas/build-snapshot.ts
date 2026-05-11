import type { Edge } from '@xyflow/react'

import {
  AGENT_DEFINITION_SCHEMA,
  AGENT_DEFINITION_SCHEMA_VERSION,
  type CustomAgentAttachedSkillDto,
  type CustomAgentWorkflowBranchConditionDto,
  type CustomAgentWorkflowBranchDto,
  type CustomAgentWorkflowGateDto,
  type CustomAgentWorkflowPhaseDto,
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
  SkillFlowNode,
  StageFlowNode,
  ToolFlowNode,
} from './build-agent-graph'
import { STAGE_BRANCH_DATA_CATEGORY } from './build-agent-graph'

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

function isSkill(node: AgentGraphNode): node is SkillFlowNode {
  return node.type === 'skills'
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

function isStage(node: AgentGraphNode): node is StageFlowNode {
  return node.type === 'stage'
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
  attachedSkills?: readonly CustomAgentAttachedSkillDto[]
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

  // workflowStructure is optional. Only emit it when the canvas has at least
  // one phase node, so definitions without an authored state machine keep the
  // same shape they had before phases existed.
  const workflowStructure = buildWorkflowStructure(nodes, edges)

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

  const baseCapabilityProfile = headerDto.baseCapabilityProfile

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
  const inferred = inferFromSnapshotPieces(tools)
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
  const skillNodeAttachments = nodes.filter(isSkill).map((node) => node.data.skill)
  const attachedSkillSource =
    skillNodeAttachments.length > 0 ? skillNodeAttachments : options.attachedSkills ?? []
  const attachedSkills = attachedSkillSource.map((skill) => ({
    id: skill.id,
    sourceId: skill.sourceId,
    skillId: skill.skillId,
    name: skill.name,
    description: skill.description,
    sourceKind: skill.sourceKind,
    scope: skill.scope,
    versionHash: skill.versionHash,
    includeSupportingAssets: skill.includeSupportingAssets,
    required: skill.required,
  }))

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
    attachedSkills,
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

  if (workflowStructure) {
    snapshot.workflowStructure = workflowStructure
  }

  return { snapshot, definitionId }
}

interface PhaseBranchEdgeData {
  category?: string
  sourcePhaseId?: string
  targetPhaseId?: string
  branchIndex?: number
  condition?: CustomAgentWorkflowBranchConditionDto
  label?: string
  // Set on edges synthesized by buildAgentGraph to visualize the runtime's
  // sequential fall-through advance. These mirror what would happen anyway
  // and must not be persisted as authored branches.
  implicit?: boolean
}

/**
 * Collect stage nodes + phase-branch edges into the workflowStructure object
 * the backend validator accepts. Branches authored directly on phase data
 * are preserved; edges drawn between stage nodes on the canvas overlay
 * those, so dragging a connection produces the right branch entry. Returns
 * undefined when no stages exist so the snapshot stays unchanged for
 * definitions without an authored state machine.
 */
function buildWorkflowStructure(
  nodes: readonly AgentGraphNode[],
  edges: readonly Edge[],
): { startPhaseId?: string; phases: CustomAgentWorkflowPhaseDto[] } | undefined {
  const phaseNodes = nodes.filter(isStage)
  if (phaseNodes.length === 0) return undefined

  const phaseIdSet = new Set<string>()
  for (const node of phaseNodes) {
    const id = node.data.phase.id.trim()
    if (id) phaseIdSet.add(id)
  }

  // Group canvas-authored phase-branch edges by their source phase. Each
  // edge becomes one branch entry on its source phase. Stable order: the
  // edges array is iterated in graph order, ties broken by branchIndex when
  // the original detail's branches survived through the round-trip.
  const canvasBranchesBySource = new Map<string, CustomAgentWorkflowBranchDto[]>()
  for (const edge of edges) {
    const data = edge.data as PhaseBranchEdgeData | undefined
    if (data?.category !== STAGE_BRANCH_DATA_CATEGORY) continue
    // Skip edges synthesized to visualize the runtime's sequential
    // fall-through. They are derived, not authored, so persisting them as
    // explicit branches would change semantics on the next load.
    if (data.implicit === true) continue
    const sourcePhaseId = data.sourcePhaseId?.trim() ?? phaseIdFromNodeId(edge.source)
    const targetPhaseId = data.targetPhaseId?.trim() ?? phaseIdFromNodeId(edge.target)
    if (!sourcePhaseId || !targetPhaseId) continue
    if (!phaseIdSet.has(sourcePhaseId) || !phaseIdSet.has(targetPhaseId)) continue
    const list = canvasBranchesBySource.get(sourcePhaseId) ?? []
    const branch: CustomAgentWorkflowBranchDto = {
      targetPhaseId,
      condition: data.condition ? cloneCondition(data.condition) : { kind: 'always' },
    }
    if (data.label !== undefined) branch.label = data.label
    list.push(branch)
    canvasBranchesBySource.set(sourcePhaseId, list)
  }

  const phases: CustomAgentWorkflowPhaseDto[] = phaseNodes.map((node, index) => {
    const dto = node.data.phase
    const id = dto.id.trim() || `phase-${index + 1}`
    const title = dto.title.trim() || `Phase ${index + 1}`
    const allowedTools = dto.allowedTools && dto.allowedTools.length > 0
      ? Array.from(new Set(dto.allowedTools.map((tool) => tool.trim()).filter(Boolean)))
      : undefined
    const requiredChecks = dto.requiredChecks
      ?.map(cloneCheck)
      .filter((check): check is CustomAgentWorkflowGateDto => check !== null)
    const canvasBranches = canvasBranchesBySource.get(id)
    // Canvas-drawn edges are the source of truth — they represent the user's
    // current authoring intent. Fall back to the phase's authored branches
    // when no edges target this phase id, so an existing workflow that's
    // viewed without being edited round-trips cleanly.
    const branches = canvasBranches && canvasBranches.length > 0
      ? canvasBranches
      : dto.branches?.map(cloneAuthoredBranch)
    const phase: CustomAgentWorkflowPhaseDto = { id, title }
    if (dto.description !== undefined) phase.description = dto.description
    if (allowedTools !== undefined) phase.allowedTools = allowedTools
    if (requiredChecks && requiredChecks.length > 0) phase.requiredChecks = requiredChecks
    if (dto.retryLimit !== undefined) phase.retryLimit = dto.retryLimit
    if (branches && branches.length > 0) phase.branches = branches
    return phase
  })

  const explicitStartPhaseId = phaseNodes.find((node) => node.data.isStart)?.data.phase.id.trim()
  const startPhaseId =
    explicitStartPhaseId && phaseIdSet.has(explicitStartPhaseId)
      ? explicitStartPhaseId
      : undefined

  const structure: { startPhaseId?: string; phases: CustomAgentWorkflowPhaseDto[] } = {
    phases,
  }
  if (startPhaseId) {
    structure.startPhaseId = startPhaseId
  }
  return structure
}

function phaseIdFromNodeId(nodeId: string): string | undefined {
  if (!nodeId.startsWith('workflow-phase:')) return undefined
  const value = nodeId.slice('workflow-phase:'.length).trim()
  return value.length > 0 ? value : undefined
}

function cloneCheck(check: CustomAgentWorkflowGateDto): CustomAgentWorkflowGateDto | null {
  if (check.kind === 'todo_completed') {
    if (!check.todoId?.trim()) return null
    const result: CustomAgentWorkflowGateDto = {
      kind: 'todo_completed',
      todoId: check.todoId,
    }
    if (check.description !== undefined) result.description = check.description
    return result
  }
  if (!check.toolName?.trim()) return null
  const result: CustomAgentWorkflowGateDto = {
    kind: 'tool_succeeded',
    toolName: check.toolName,
  }
  if (check.minCount !== undefined) result.minCount = check.minCount
  if (check.description !== undefined) result.description = check.description
  return result
}

function cloneAuthoredBranch(branch: CustomAgentWorkflowBranchDto): CustomAgentWorkflowBranchDto {
  const result: CustomAgentWorkflowBranchDto = {
    targetPhaseId: branch.targetPhaseId,
    condition: cloneCondition(branch.condition),
  }
  if (branch.label !== undefined) result.label = branch.label
  return result
}

function cloneCondition(
  condition: CustomAgentWorkflowBranchConditionDto,
): CustomAgentWorkflowBranchConditionDto {
  switch (condition.kind) {
    case 'always':
      return { kind: 'always' }
    case 'todo_completed':
      return { kind: 'todo_completed', todoId: condition.todoId }
    case 'tool_succeeded': {
      const result: CustomAgentWorkflowBranchConditionDto = {
        kind: 'tool_succeeded',
        toolName: condition.toolName,
      }
      if (condition.minCount !== undefined) result.minCount = condition.minCount
      return result
    }
  }
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
    allowedMcpServers: [...advanced.allowedMcpServers],
    deniedMcpServers: [...advanced.deniedMcpServers],
    allowedDynamicTools: [...advanced.allowedDynamicTools],
    deniedDynamicTools: [...advanced.deniedDynamicTools],
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
    // The validator requires allowedSubagentRoles to be non-empty when
    // subagent delegation is on. Honour the user's picks from the granular
    // policy editor; fall back to ['engineer'] only if nothing was picked
    // (preserves the prior implicit default so legacy snapshots round-trip).
    policy.allowedSubagentRoles =
      advanced.allowedSubagentRoles.length > 0
        ? [...advanced.allowedSubagentRoles]
        : ['engineer']
    if (advanced.deniedSubagentRoles.length > 0) {
      policy.deniedSubagentRoles = [...advanced.deniedSubagentRoles]
    }
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
// just to walk its tools.
function inferFromSnapshotPieces(
  tools: readonly { name: string; group: string; effectClass: string }[],
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
  // Note: do NOT infer `destructiveWriteAllowed` from db writes. The runtime
  // classifies DB writes as runtime_state — `destructive_write` is reserved
  // for repository-file destructive ops (Delete). Inferring it from dbWrites
  // made the planning profile reject duplicates of built-ins like Plan that
  // legitimately persist agent_runs / agent_messages.
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
