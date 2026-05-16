import { z } from 'zod'

import {
  agentDefinitionBaseCapabilityProfileSchema,
  agentDefinitionLifecycleStateSchema,
  agentDefinitionScopeSchema,
  canonicalCustomAgentDefinitionBaseSchema,
  canonicalCustomAgentDefinitionSchema,
  customAgentAttachedSkillSchema,
  customAgentHandoffPolicySchema,
  customAgentMemoryPolicySchema,
  customAgentProjectDataPolicySchema,
  customAgentRetrievalPolicySchema,
  customAgentSubagentRoleSchema,
  customAgentWorkflowStructureSchema,
  validateCanonicalCustomAgentDefinition,
} from './agent-definition'
import { isoTimestampSchema } from '@xero/ui/model/shared'
import { runtimeAgentIdSchema, runtimeRunApprovalModeSchema } from '@xero/ui/model/runtime'
import { skillSourceKindSchema, skillSourceScopeSchema, skillSourceStateSchema, skillTrustStateSchema } from './skills'

const addDuplicateStringIssues = (
  ctx: z.RefinementCtx,
  path: (string | number)[],
  values: string[],
  message: string,
) => {
  const seen = new Set<string>()
  values.forEach((value, index) => {
    if (seen.has(value)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: [...path, index],
        message,
      })
    }
    seen.add(value)
  })
}

export const runtimeAgentPromptPolicySchema = z.enum([
  'ask',
  'plan',
  'engineer',
  'debug',
  'crawl',
  'agent_create',
])
export type RuntimeAgentPromptPolicyDto = z.infer<typeof runtimeAgentPromptPolicySchema>

export const runtimeAgentToolPolicySchema = z.enum([
  'observe_only',
  'planning',
  'repository_recon',
  'engineering',
  'agent_builder',
])
export type RuntimeAgentToolPolicyDto = z.infer<typeof runtimeAgentToolPolicySchema>

export const runtimeAgentOutputContractSchema = z.enum([
  'answer',
  'plan_pack',
  'crawl_report',
  'engineering_summary',
  'debug_summary',
  'agent_definition_draft',
])
export type RuntimeAgentOutputContractDto = z.infer<typeof runtimeAgentOutputContractSchema>

export const agentToolEffectClassSchema = z.enum([
  'observe',
  'runtime_state',
  'write',
  'destructive_write',
  'command',
  'process_control',
  'browser_control',
  'device_control',
  'external_service',
  'skill_runtime',
  'agent_delegation',
  'unknown',
])
export type AgentToolEffectClassDto = z.infer<typeof agentToolEffectClassSchema>

export const agentPromptRoleSchema = z.enum(['system', 'developer', 'task'])
export type AgentPromptRoleDto = z.infer<typeof agentPromptRoleSchema>

export const agentRefSchema = z.discriminatedUnion('kind', [
  z
    .object({
      kind: z.literal('built_in'),
      runtimeAgentId: runtimeAgentIdSchema,
      version: z.number().int().positive(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('custom'),
      definitionId: z.string().trim().min(1),
      version: z.number().int().positive(),
    })
    .strict(),
])
export type AgentRefDto = z.infer<typeof agentRefSchema>

export const workflowAgentSummarySchema = z
  .object({
    ref: agentRefSchema,
    displayName: z.string().trim().min(1),
    shortLabel: z.string().trim().min(1),
    description: z.string(),
    scope: agentDefinitionScopeSchema,
    lifecycleState: agentDefinitionLifecycleStateSchema,
    baseCapabilityProfile: agentDefinitionBaseCapabilityProfileSchema,
    lastUsedAt: isoTimestampSchema.nullable().optional(),
    useCount: z.number().int().nonnegative(),
  })
  .strict()
export type WorkflowAgentSummaryDto = z.infer<typeof workflowAgentSummarySchema>

export const agentHeaderSchema = z
  .object({
    displayName: z.string().trim().min(1),
    shortLabel: z.string().trim().min(1),
    description: z.string(),
    taskPurpose: z.string(),
    scope: agentDefinitionScopeSchema,
    lifecycleState: agentDefinitionLifecycleStateSchema,
    baseCapabilityProfile: agentDefinitionBaseCapabilityProfileSchema,
    defaultApprovalMode: runtimeRunApprovalModeSchema,
    allowedApprovalModes: z.array(runtimeRunApprovalModeSchema),
    allowPlanGate: z.boolean(),
    allowVerificationGate: z.boolean(),
    allowAutoCompact: z.boolean(),
  })
  .strict()
export type AgentHeaderDto = z.infer<typeof agentHeaderSchema>

export const agentPromptSchema = z
  .object({
    id: z.string().trim().min(1),
    label: z.string().trim().min(1),
    role: agentPromptRoleSchema,
    policy: runtimeAgentPromptPolicySchema.nullable().optional(),
    source: z.string(),
    body: z.string(),
  })
  .strict()
export type AgentPromptDto = z.infer<typeof agentPromptSchema>

export const agentToolSummarySchema = z
  .object({
    name: z.string().trim().min(1),
    group: z.string(),
    description: z.string(),
    effectClass: agentToolEffectClassSchema,
    riskClass: z.string(),
    tags: z.array(z.string()),
    schemaFields: z.array(z.string()),
    examples: z.array(z.string()),
  })
  .strict()
export type AgentToolSummaryDto = z.infer<typeof agentToolSummarySchema>

export const agentAuthoringAvailabilityStatusSchema = z.enum([
  'available',
  'requires_profile_change',
  'unavailable',
])
export type AgentAuthoringAvailabilityStatusDto = z.infer<
  typeof agentAuthoringAvailabilityStatusSchema
>

export const agentAuthoringProfileAvailabilitySchema = z
  .object({
    subjectKind: z.string().trim().min(1),
    subjectId: z.string().trim().min(1),
    baseCapabilityProfile: agentDefinitionBaseCapabilityProfileSchema,
    status: agentAuthoringAvailabilityStatusSchema,
    reason: z.string().trim().min(1),
    requiredProfile: agentDefinitionBaseCapabilityProfileSchema.nullable().optional(),
  })
  .strict()
  .superRefine((availability, ctx) => {
    if (
      availability.status === 'requires_profile_change' &&
      availability.requiredProfile == null
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['requiredProfile'],
        message: 'Profile-gated authoring availability must name the required profile.',
      })
    }
    if (availability.status !== 'requires_profile_change' && availability.requiredProfile != null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['requiredProfile'],
        message: 'Only profile-gated authoring availability may include a required profile.',
      })
    }
  })
export type AgentAuthoringProfileAvailabilityDto = z.infer<
  typeof agentAuthoringProfileAvailabilitySchema
>

export const agentToolPolicyDetailsSchema = z
  .object({
    allowedTools: z.array(z.string().trim().min(1)),
    deniedTools: z.array(z.string().trim().min(1)),
    allowedToolPacks: z.array(z.string().trim().min(1)),
    deniedToolPacks: z.array(z.string().trim().min(1)),
    allowedToolGroups: z.array(z.string().trim().min(1)),
    deniedToolGroups: z.array(z.string().trim().min(1)),
    allowedMcpServers: z.array(z.string().trim().min(1)).default([]),
    deniedMcpServers: z.array(z.string().trim().min(1)).default([]),
    allowedDynamicTools: z.array(z.string().trim().min(1)).default([]),
    deniedDynamicTools: z.array(z.string().trim().min(1)).default([]),
    allowedEffectClasses: z.array(agentToolEffectClassSchema),
    externalServiceAllowed: z.boolean(),
    browserControlAllowed: z.boolean(),
    skillRuntimeAllowed: z.boolean(),
    subagentAllowed: z.boolean(),
    allowedSubagentRoles: z.array(customAgentSubagentRoleSchema),
    deniedSubagentRoles: z.array(customAgentSubagentRoleSchema),
    commandAllowed: z.boolean(),
    destructiveWriteAllowed: z.boolean(),
  })
  .strict()
  .superRefine((details, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['allowedTools'],
      details.allowedTools,
      'Tool-policy allowed tools must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedTools'],
      details.deniedTools,
      'Tool-policy denied tools must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedToolPacks'],
      details.allowedToolPacks,
      'Tool-policy allowed tool packs must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedToolPacks'],
      details.deniedToolPacks,
      'Tool-policy denied tool packs must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedToolGroups'],
      details.allowedToolGroups,
      'Tool-policy allowed tool groups must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedMcpServers'],
      details.allowedMcpServers,
      'Tool-policy allowed MCP servers must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedMcpServers'],
      details.deniedMcpServers,
      'Tool-policy denied MCP servers must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedDynamicTools'],
      details.allowedDynamicTools,
      'Tool-policy allowed dynamic tools must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedDynamicTools'],
      details.deniedDynamicTools,
      'Tool-policy denied dynamic tools must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedToolGroups'],
      details.deniedToolGroups,
      'Tool-policy denied tool groups must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedEffectClasses'],
      details.allowedEffectClasses,
      'Tool-policy allowed effect classes must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedSubagentRoles'],
      details.allowedSubagentRoles,
      'Tool-policy allowed subagent roles must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedSubagentRoles'],
      details.deniedSubagentRoles,
      'Tool-policy denied subagent roles must be unique.',
    )
    if (details.subagentAllowed && details.allowedSubagentRoles.length === 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['allowedSubagentRoles'],
        message: 'Tool policies that enable subagents must declare allowed subagent roles.',
      })
    }
    const deniedRoles = new Set(details.deniedSubagentRoles)
    for (const [index, role] of details.allowedSubagentRoles.entries()) {
      if (deniedRoles.has(role)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['allowedSubagentRoles', index],
          message: 'Tool-policy allowed subagent roles cannot also be denied.',
        })
      }
    }
  })
export type AgentToolPolicyDetailsDto = z.infer<typeof agentToolPolicyDetailsSchema>

export const agentDbTouchpointKindSchema = z.enum(['read', 'write', 'encouraged'])
export type AgentDbTouchpointKindDto = z.infer<typeof agentDbTouchpointKindSchema>

export const agentTriggerLifecycleEventSchema = z.enum([
  'state_transition',
  'plan_update',
  'message_persisted',
  'tool_call',
  'file_edit',
  'run_start',
  'run_complete',
  'approval_decision',
  'verification_gate',
  'definition_persisted',
])
export type AgentTriggerLifecycleEventDto = z.infer<typeof agentTriggerLifecycleEventSchema>

export const agentTriggerRefSchema = z.discriminatedUnion('kind', [
  z
    .object({
      kind: z.literal('tool'),
      name: z.string().trim().min(1),
    })
    .strict(),
  z
    .object({
      kind: z.literal('output_section'),
      id: z.string().trim().min(1),
    })
    .strict(),
  z
    .object({
      kind: z.literal('lifecycle'),
      event: agentTriggerLifecycleEventSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('upstream_artifact'),
      id: z.string().trim().min(1),
    })
    .strict(),
])
export type AgentTriggerRefDto = z.infer<typeof agentTriggerRefSchema>

export const agentDbTouchpointDetailSchema = z
  .object({
    table: z.string().trim().min(1),
    kind: agentDbTouchpointKindSchema,
    purpose: z.string().trim().min(1),
    triggers: z.array(agentTriggerRefSchema),
    columns: z.array(z.string().trim().min(1)),
  })
  .strict()
  .superRefine((touchpoint, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['columns'],
      touchpoint.columns,
      'Database touchpoint columns must be unique.',
    )
  })
export type AgentDbTouchpointDetailDto = z.infer<typeof agentDbTouchpointDetailSchema>

export const agentDbTouchpointsSchema = z
  .object({
    reads: z.array(agentDbTouchpointDetailSchema),
    writes: z.array(agentDbTouchpointDetailSchema),
    encouraged: z.array(agentDbTouchpointDetailSchema),
  })
  .strict()
  .superRefine((touchpoints, ctx) => {
    const sections = [
      ['reads', touchpoints.reads, 'read'],
      ['writes', touchpoints.writes, 'write'],
      ['encouraged', touchpoints.encouraged, 'encouraged'],
    ] as const
    sections.forEach(([section, entries, expectedKind]) => {
      entries.forEach((entry, index) => {
        if (entry.kind !== expectedKind) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: [section, index, 'kind'],
            message: `Database touchpoint ${section} entries must use kind ${expectedKind}.`,
          })
        }
      })
    })
  })
export type AgentDbTouchpointsDto = z.infer<typeof agentDbTouchpointsSchema>

export const agentOutputSectionEmphasisSchema = z.enum(['core', 'standard', 'optional'])
export type AgentOutputSectionEmphasisDto = z.infer<typeof agentOutputSectionEmphasisSchema>

export const agentOutputSectionSchema = z
  .object({
    id: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string().trim().min(1),
    emphasis: agentOutputSectionEmphasisSchema,
    producedByTools: z.array(z.string().trim().min(1)),
  })
  .strict()
  .superRefine((section, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['producedByTools'],
      section.producedByTools,
      'Output section produced-by tool ids must be unique.',
    )
  })
export type AgentOutputSectionDto = z.infer<typeof agentOutputSectionSchema>

export const agentOutputContractSchema = z
  .object({
    contract: runtimeAgentOutputContractSchema,
    label: z.string().trim().min(1),
    description: z.string().trim().min(1),
    sections: z.array(agentOutputSectionSchema),
  })
  .strict()
  .superRefine((output, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['sections'],
      output.sections.map((section) => section.id),
      'Output contract section ids must be unique.',
    )
  })
export type AgentOutputContractDto = z.infer<typeof agentOutputContractSchema>

export const agentConsumedArtifactSchema = z
  .object({
    id: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string().trim().min(1),
    sourceAgent: runtimeAgentIdSchema,
    contract: runtimeAgentOutputContractSchema,
    sections: z.array(z.string().trim().min(1)),
    required: z.boolean(),
  })
  .strict()
  .superRefine((artifact, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['sections'],
      artifact.sections,
      'Consumed artifact section ids must be unique.',
    )
  })
export type AgentConsumedArtifactDto = z.infer<typeof agentConsumedArtifactSchema>

export const agentAttachedSkillAvailabilityStatusSchema = z.enum([
  'available',
  'unavailable',
  'stale',
  'blocked',
  'missing',
])
export type AgentAttachedSkillAvailabilityStatusDto = z.infer<
  typeof agentAttachedSkillAvailabilityStatusSchema
>

export const agentAttachedSkillSchema = customAgentAttachedSkillSchema
  .extend({
    sourceState: skillSourceStateSchema.nullable().optional(),
    trustState: skillTrustStateSchema.nullable().optional(),
    availabilityStatus: agentAttachedSkillAvailabilityStatusSchema,
    availabilityReason: z.string().trim().min(1),
    repairHint: z
      .enum(['enable_source', 'approve_source', 'refresh_pin', 'remove_attachment'])
      .nullable()
      .optional(),
  })
  .strict()
  .superRefine((skill, ctx) => {
    if (skill.availabilityStatus === 'available' && skill.repairHint != null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['repairHint'],
        message: 'Available attached skills must not include a repair hint.',
      })
    }
    if (skill.availabilityStatus !== 'missing' && skill.sourceState == null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['sourceState'],
        message: 'Attached skill availability must include source state unless the source is missing.',
      })
    }
    if (skill.availabilityStatus !== 'missing' && skill.trustState == null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['trustState'],
        message: 'Attached skill availability must include trust state unless the source is missing.',
      })
    }
  })
export type AgentAttachedSkillDto = z.infer<typeof agentAttachedSkillSchema>

export const agentAuthoringGraphSourceSchema = z
  .object({
    kind: z.literal('agent_definition_version'),
    definitionId: z.string().trim().min(1),
    version: z.number().int().positive(),
    scope: agentDefinitionScopeSchema,
    lifecycleState: agentDefinitionLifecycleStateSchema,
    baseCapabilityProfile: agentDefinitionBaseCapabilityProfileSchema,
    createdAt: isoTimestampSchema,
    generatedBy: z.enum(['saved_definition', 'template', 'agent_builder']),
    uiDeferred: z.literal(true),
  })
  .strict()
export type AgentAuthoringGraphSourceDto = z.infer<typeof agentAuthoringGraphSourceSchema>

export const agentAuthoringCanonicalGraphSchema = canonicalCustomAgentDefinitionBaseSchema
  .extend({
    version: z.number().int().positive().nullable().optional(),
    workflowStructure: customAgentWorkflowStructureSchema.nullable().optional(),
    projectDataPolicy: customAgentProjectDataPolicySchema.nullable().optional(),
    memoryCandidatePolicy: customAgentMemoryPolicySchema.nullable().optional(),
    retrievalDefaults: customAgentRetrievalPolicySchema.nullable().optional(),
    handoffPolicy: customAgentHandoffPolicySchema.nullable().optional(),
  })
  .strict()
  .superRefine((definition, ctx) => {
    validateCanonicalCustomAgentDefinition(
      {
        ...definition,
        version: definition.version ?? undefined,
      } as z.infer<typeof canonicalCustomAgentDefinitionBaseSchema>,
      ctx,
    )
  })
export type AgentAuthoringCanonicalGraphDto = z.infer<typeof agentAuthoringCanonicalGraphSchema>

export const agentAuthoringEditableFieldSchema = z.enum([
  'prompts',
  'attachedSkills',
  'tools',
  'toolPolicy',
  'output',
  'dbTouchpoints',
  'consumes',
  'workflowStructure',
  'projectDataPolicy',
  'memoryCandidatePolicy',
  'retrievalDefaults',
  'handoffPolicy',
])
export type AgentAuthoringEditableFieldDto = z.infer<typeof agentAuthoringEditableFieldSchema>

export const agentAuthoringGraphSchema = z
  .object({
    schema: z.literal('xero.agent_authoring_graph.v1'),
    source: agentAuthoringGraphSourceSchema,
    editableFields: z.array(agentAuthoringEditableFieldSchema),
    canonicalGraph: agentAuthoringCanonicalGraphSchema,
  })
  .strict()
  .superRefine((graph, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['editableFields'],
      graph.editableFields,
      'Authoring graph editable fields must be unique.',
    )
    if (graph.source.definitionId !== graph.canonicalGraph.id) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['canonicalGraph', 'id'],
        message: 'Authoring graph source definitionId must match canonical graph id.',
      })
    }
    if (
      graph.canonicalGraph.version !== null &&
      graph.canonicalGraph.version !== undefined &&
      graph.source.version !== graph.canonicalGraph.version
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['canonicalGraph', 'version'],
        message: 'Authoring graph source version must match canonical graph version.',
      })
    }
    if (graph.source.scope !== graph.canonicalGraph.scope) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['canonicalGraph', 'scope'],
        message: 'Authoring graph source scope must match canonical graph scope.',
      })
    }
    if (graph.source.lifecycleState !== graph.canonicalGraph.lifecycleState) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['canonicalGraph', 'lifecycleState'],
        message: 'Authoring graph source lifecycle state must match canonical graph lifecycle state.',
      })
    }
    if (graph.source.baseCapabilityProfile !== graph.canonicalGraph.baseCapabilityProfile) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['canonicalGraph', 'baseCapabilityProfile'],
        message:
          'Authoring graph source base capability profile must match canonical graph base capability profile.',
      })
    }
  })
export type AgentAuthoringGraphDto = z.infer<typeof agentAuthoringGraphSchema>

export const workflowAgentGraphPositionSchema = z
  .object({
    x: z.number(),
    y: z.number(),
  })
  .strict()
export type WorkflowAgentGraphPositionDto = z.infer<typeof workflowAgentGraphPositionSchema>

export const workflowAgentGraphMarkerSchema = z.enum(['arrow', 'arrow_closed'])
export type WorkflowAgentGraphMarkerDto = z.infer<typeof workflowAgentGraphMarkerSchema>

export const workflowAgentGraphNodeSchema = z
  .object({
    id: z.string().trim().min(1),
    type: z.string().trim().min(1),
    position: workflowAgentGraphPositionSchema,
    data: z.record(z.string(), z.unknown()),
    parentId: z.string().trim().min(1).optional(),
    extent: z.string().trim().min(1).optional(),
    draggable: z.boolean().optional(),
    selectable: z.boolean().optional(),
    dragHandle: z.string().trim().min(1).optional(),
    style: z.record(z.string(), z.unknown()).optional(),
    width: z.number().positive().optional(),
    height: z.number().positive().optional(),
  })
  .strict()
export type WorkflowAgentGraphNodeDto = z.infer<typeof workflowAgentGraphNodeSchema>

export const workflowAgentGraphEdgeSchema = z
  .object({
    id: z.string().trim().min(1),
    source: z.string().trim().min(1),
    target: z.string().trim().min(1),
    type: z.string().trim().min(1),
    sourceHandle: z.string().trim().min(1).optional(),
    targetHandle: z.string().trim().min(1).optional(),
    data: z.record(z.string(), z.unknown()),
    className: z.string().trim().min(1),
    marker: workflowAgentGraphMarkerSchema.optional(),
  })
  .strict()
export type WorkflowAgentGraphEdgeDto = z.infer<typeof workflowAgentGraphEdgeSchema>

export const workflowAgentGraphGroupSchema = z
  .object({
    key: z.string().trim().min(1),
    label: z.string().trim().min(1),
    kind: z.string().trim().min(1),
    order: z.number().int(),
    nodeIds: z.array(z.string().trim().min(1)),
    sourceGroups: z.array(z.string().trim().min(1)).default([]),
  })
  .strict()
export type WorkflowAgentGraphGroupDto = z.infer<typeof workflowAgentGraphGroupSchema>

export const workflowAgentGraphProjectionSchema = z
  .object({
    schema: z.literal('xero.workflow_agent_graph_projection.v1'),
    nodes: z.array(workflowAgentGraphNodeSchema),
    edges: z.array(workflowAgentGraphEdgeSchema),
    groups: z.array(workflowAgentGraphGroupSchema),
  })
  .strict()
export type WorkflowAgentGraphProjectionDto = z.infer<
  typeof workflowAgentGraphProjectionSchema
>

export const workflowAgentDetailSchema = z
  .object({
    ref: agentRefSchema,
    header: agentHeaderSchema,
    promptPolicy: runtimeAgentPromptPolicySchema.nullable().optional(),
    toolPolicy: runtimeAgentToolPolicySchema.nullable().optional(),
    toolPolicyDetails: agentToolPolicyDetailsSchema.nullable().optional(),
    prompts: z.array(agentPromptSchema),
    tools: z.array(agentToolSummarySchema),
    dbTouchpoints: agentDbTouchpointsSchema,
    output: agentOutputContractSchema,
    consumes: z.array(agentConsumedArtifactSchema),
    attachedSkills: z.array(agentAttachedSkillSchema),
    workflowStructure: customAgentWorkflowStructureSchema.nullable().optional(),
    authoringGraph: agentAuthoringGraphSchema.nullable().optional(),
    graphProjection: workflowAgentGraphProjectionSchema.nullable().optional(),
  })
  .strict()
  .superRefine((detail, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['attachedSkills'],
      detail.attachedSkills.map((skill) => skill.id),
      'Workflow agent attached skill ids must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['attachedSkills'],
      detail.attachedSkills.map((skill) => skill.sourceId),
      'Workflow agent attached skill source ids must be unique.',
    )
    const graph = detail.authoringGraph
    if (!graph) {
      return
    }
    if (detail.ref.kind !== 'custom') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['authoringGraph'],
        message: 'Authoring graph detail is only valid for custom agents.',
      })
      return
    }
    if (detail.ref.definitionId !== graph.source.definitionId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['authoringGraph', 'source', 'definitionId'],
        message: 'Authoring graph source definitionId must match detail ref.',
      })
    }
    if (detail.ref.version !== graph.source.version) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['authoringGraph', 'source', 'version'],
        message: 'Authoring graph source version must match detail ref.',
      })
    }
    if (detail.header.scope !== graph.source.scope) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['authoringGraph', 'source', 'scope'],
        message: 'Authoring graph source scope must match detail header.',
      })
    }
    if (detail.header.lifecycleState !== graph.source.lifecycleState) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['authoringGraph', 'source', 'lifecycleState'],
        message: 'Authoring graph source lifecycle state must match detail header.',
      })
    }
    if (detail.header.baseCapabilityProfile !== graph.source.baseCapabilityProfile) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['authoringGraph', 'source', 'baseCapabilityProfile'],
        message: 'Authoring graph source base capability profile must match detail header.',
      })
    }
  })
export type WorkflowAgentDetailDto = z.infer<typeof workflowAgentDetailSchema>

export const listWorkflowAgentsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    includeArchived: z.boolean().default(false),
  })
  .strict()
export type ListWorkflowAgentsRequestDto = z.infer<typeof listWorkflowAgentsRequestSchema>

export const listWorkflowAgentsResponseSchema = z
  .object({
    agents: z.array(workflowAgentSummarySchema),
  })
  .strict()
export type ListWorkflowAgentsResponseDto = z.infer<typeof listWorkflowAgentsResponseSchema>

export const getWorkflowAgentDetailRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    ref: agentRefSchema,
  })
  .strict()
export type GetWorkflowAgentDetailRequestDto = z.infer<typeof getWorkflowAgentDetailRequestSchema>

export const getWorkflowAgentGraphProjectionRequestSchema = getWorkflowAgentDetailRequestSchema
export type GetWorkflowAgentGraphProjectionRequestDto = z.infer<
  typeof getWorkflowAgentGraphProjectionRequestSchema
>

export const agentAuthoringDbTableSchema = z
  .object({
    table: z.string().trim().min(1),
    purpose: z.string(),
    columns: z.array(z.string()),
  })
  .strict()
export type AgentAuthoringDbTableDto = z.infer<typeof agentAuthoringDbTableSchema>

export const agentAuthoringUpstreamArtifactSchema = z
  .object({
    sourceAgent: runtimeAgentIdSchema,
    sourceAgentLabel: z.string(),
    contract: runtimeAgentOutputContractSchema,
    contractLabel: z.string(),
    label: z.string(),
    description: z.string(),
    sections: z.array(agentOutputSectionSchema),
  })
  .strict()
export type AgentAuthoringUpstreamArtifactDto = z.infer<typeof agentAuthoringUpstreamArtifactSchema>

export const agentAuthoringToolCategorySchema = z
  .object({
    id: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string(),
    tools: z.array(agentToolSummarySchema),
  })
  .strict()
export type AgentAuthoringToolCategoryDto = z.infer<typeof agentAuthoringToolCategorySchema>

export const agentAuthoringAttachableSkillSchema = z
  .object({
    attachmentId: z.string().trim().min(1),
    sourceId: z.string().trim().min(1),
    skillId: z.string().trim().min(1),
    name: z.string().trim().min(1),
    description: z.string(),
    sourceKind: skillSourceKindSchema,
    scope: skillSourceScopeSchema,
    versionHash: z.string().trim().min(1),
    sourceState: skillSourceStateSchema,
    trustState: skillTrustStateSchema,
    availabilityStatus: z.literal('available'),
    attachment: customAgentAttachedSkillSchema,
  })
  .strict()
  .superRefine((entry, ctx) => {
    if (entry.attachment.id !== entry.attachmentId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attachment', 'id'],
        message: 'Attachable skill template id must match attachmentId.',
      })
    }
    if (entry.attachment.sourceId !== entry.sourceId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attachment', 'sourceId'],
        message: 'Attachable skill template sourceId must match the catalog entry.',
      })
    }
    if (entry.attachment.skillId !== entry.skillId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attachment', 'skillId'],
        message: 'Attachable skill template skillId must match the catalog entry.',
      })
    }
    if (entry.attachment.sourceKind !== entry.sourceKind) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attachment', 'sourceKind'],
        message: 'Attachable skill template sourceKind must match the catalog entry.',
      })
    }
    if (entry.attachment.scope !== entry.scope) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attachment', 'scope'],
        message: 'Attachable skill template scope must match the catalog entry.',
      })
    }
    if (entry.attachment.versionHash !== entry.versionHash) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attachment', 'versionHash'],
        message: 'Attachable skill template versionHash must match the catalog entry.',
      })
    }
  })
export type AgentAuthoringAttachableSkillDto = z.infer<
  typeof agentAuthoringAttachableSkillSchema
>

export const agentAuthoringSkillSearchResultSchema = z
  .object({
    source: z.string().trim().min(1),
    skillId: z.string().trim().min(1),
    name: z.string().trim().min(1),
    description: z.string(),
    installs: z.number().int().nonnegative().nullable().optional(),
    isOfficial: z.boolean(),
  })
  .strict()
export type AgentAuthoringSkillSearchResultDto = z.infer<
  typeof agentAuthoringSkillSearchResultSchema
>

export const agentAuthoringPolicyControlKindSchema = z.enum([
  'context',
  'memory',
  'retrieval',
  'handoff',
])
export type AgentAuthoringPolicyControlKindDto = z.infer<
  typeof agentAuthoringPolicyControlKindSchema
>

export const agentAuthoringPolicyControlValueKindSchema = z.enum([
  'boolean',
  'positive_integer',
  'string_array',
  'object',
])
export type AgentAuthoringPolicyControlValueKindDto = z.infer<
  typeof agentAuthoringPolicyControlValueKindSchema
>

const agentAuthoringPolicyControlBaseSchema = z
  .object({
    id: z.string().trim().min(1),
    kind: agentAuthoringPolicyControlKindSchema,
    label: z.string().trim().min(1),
    description: z.string(),
    snapshotPath: z.string().trim().min(1),
    runtimeEffect: z.string(),
    reviewRequired: z.boolean(),
  })
  .strict()

export const agentAuthoringPolicyControlSchema = z.discriminatedUnion('valueKind', [
  agentAuthoringPolicyControlBaseSchema.extend({
    valueKind: z.literal('boolean'),
    defaultValue: z.boolean(),
  }),
  agentAuthoringPolicyControlBaseSchema.extend({
    valueKind: z.literal('positive_integer'),
    defaultValue: z.number().int().positive(),
  }),
  agentAuthoringPolicyControlBaseSchema.extend({
    valueKind: z.literal('string_array'),
    defaultValue: z.array(z.string().trim().min(1)),
  }),
  agentAuthoringPolicyControlBaseSchema.extend({
    valueKind: z.literal('object'),
    defaultValue: z.record(z.string(), z.unknown()),
  }),
])
export type AgentAuthoringPolicyControlDto = z.infer<
  typeof agentAuthoringPolicyControlSchema
>

export const agentAuthoringTemplateSchema = z
  .object({
    id: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string(),
    taskKind: z.string().trim().min(1),
    baseCapabilityProfile: agentDefinitionBaseCapabilityProfileSchema,
    definition: canonicalCustomAgentDefinitionSchema,
    examples: z.array(z.string().trim().min(1)),
  })
  .strict()
  .superRefine((template, ctx) => {
    if (template.baseCapabilityProfile !== template.definition.baseCapabilityProfile) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['definition', 'baseCapabilityProfile'],
        message:
          'Authoring template base capability profile must match its canonical definition.',
      })
    }
  })
export type AgentAuthoringTemplateDto = z.infer<typeof agentAuthoringTemplateSchema>

export const agentAuthoringCreationFlowEntryKindSchema = z.enum([
  'template',
  'describe_intent',
  'compose_templates',
])
export type AgentAuthoringCreationFlowEntryKindDto = z.infer<
  typeof agentAuthoringCreationFlowEntryKindSchema
>

export const agentAuthoringCreationFlowSchema = z
  .object({
    id: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string(),
    entryKind: agentAuthoringCreationFlowEntryKindSchema,
    taskKind: z.string().trim().min(1),
    templateIds: z.array(z.string().trim().min(1)),
    intentPrompt: z.string().trim().min(1),
    expectedOutputContract: runtimeAgentOutputContractSchema,
    baseCapabilityProfile: agentDefinitionBaseCapabilityProfileSchema,
  })
  .strict()
export type AgentAuthoringCreationFlowDto = z.infer<
  typeof agentAuthoringCreationFlowSchema
>

export const agentAuthoringConstraintExplanationSchema = z
  .object({
    id: z.string().trim().min(1),
    subjectKind: z.string().trim().min(1),
    subjectId: z.string().trim().min(1),
    baseCapabilityProfile: agentDefinitionBaseCapabilityProfileSchema,
    status: agentAuthoringAvailabilityStatusSchema,
    message: z.string().trim().min(1),
    resolution: z.string().trim().min(1),
    requiredProfile: agentDefinitionBaseCapabilityProfileSchema.nullable().optional(),
    source: z.literal('profileAvailability'),
  })
  .strict()
  .superRefine((explanation, ctx) => {
    if (explanation.status === 'available') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['status'],
        message: 'Constraint explanations are only emitted for unavailable catalog choices.',
      })
    }
    if (
      explanation.status === 'requires_profile_change' &&
      explanation.requiredProfile == null
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['requiredProfile'],
        message: 'Profile-gated constraint explanations must name the required profile.',
      })
    }
    if (explanation.status === 'unavailable' && explanation.requiredProfile != null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['requiredProfile'],
        message: 'Unavailable constraint explanations must not name a profile upgrade.',
      })
    }
  })
export type AgentAuthoringConstraintExplanationDto = z.infer<
  typeof agentAuthoringConstraintExplanationSchema
>

export const agentToolPackReviewRequirementSchema = z
  .object({
    requirementId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string().trim().min(1),
    required: z.boolean(),
  })
  .strict()
export type AgentToolPackReviewRequirementDto = z.infer<
  typeof agentToolPackReviewRequirementSchema
>

export const agentToolPackPrerequisiteSchema = z
  .object({
    prerequisiteId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    kind: z.string().trim().min(1),
    required: z.boolean(),
    remediation: z.string().trim().min(1),
  })
  .strict()
export type AgentToolPackPrerequisiteDto = z.infer<typeof agentToolPackPrerequisiteSchema>

export const agentToolPackCheckDescriptorSchema = z
  .object({
    checkId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string().trim().min(1),
    prerequisiteIds: z.array(z.string().trim().min(1)),
  })
  .strict()
export type AgentToolPackCheckDescriptorDto = z.infer<
  typeof agentToolPackCheckDescriptorSchema
>

export const agentToolPackScenarioDescriptorSchema = z
  .object({
    scenarioId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string().trim().min(1),
    toolNames: z.array(z.string().trim().min(1)),
    mutating: z.boolean(),
    requiresApproval: z.boolean(),
  })
  .strict()
export type AgentToolPackScenarioDescriptorDto = z.infer<
  typeof agentToolPackScenarioDescriptorSchema
>

export const agentToolPackUiAffordanceSchema = z
  .object({
    surface: z.string().trim().min(1),
    label: z.string().trim().min(1),
  })
  .strict()
export type AgentToolPackUiAffordanceDto = z.infer<
  typeof agentToolPackUiAffordanceSchema
>

export const agentToolPackManifestSchema = z
  .object({
    contractVersion: z.literal(1),
    packId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    summary: z.string().trim().min(1),
    policyProfile: z.string().trim().min(1),
    toolGroups: z.array(z.string().trim().min(1)),
    tools: z.array(z.string().trim().min(1)),
    capabilities: z.array(z.string().trim().min(1)),
    allowedEffectClasses: z.array(z.string().trim().min(1)),
    deniedEffectClasses: z.array(z.string().trim().min(1)),
    reviewRequirements: z.array(agentToolPackReviewRequirementSchema),
    prerequisites: z.array(agentToolPackPrerequisiteSchema),
    healthChecks: z.array(agentToolPackCheckDescriptorSchema),
    scenarioChecks: z.array(agentToolPackScenarioDescriptorSchema),
    uiAffordances: z.array(agentToolPackUiAffordanceSchema),
    cliCommands: z.array(z.string().trim().min(1)),
    approvalBoundaries: z.array(z.string().trim().min(1)),
  })
  .strict()
  .superRefine((manifest, ctx) => {
    addDuplicateStringIssues(ctx, ['toolGroups'], manifest.toolGroups, 'Tool-pack groups must be unique.')
    addDuplicateStringIssues(ctx, ['tools'], manifest.tools, 'Tool-pack tools must be unique.')
    addDuplicateStringIssues(
      ctx,
      ['capabilities'],
      manifest.capabilities,
      'Tool-pack capabilities must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedEffectClasses'],
      manifest.allowedEffectClasses,
      'Tool-pack allowed effect classes must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedEffectClasses'],
      manifest.deniedEffectClasses,
      'Tool-pack denied effect classes must be unique.',
    )
    addDuplicateStringIssues(ctx, ['cliCommands'], manifest.cliCommands, 'Tool-pack CLI commands must be unique.')
    addDuplicateStringIssues(
      ctx,
      ['approvalBoundaries'],
      manifest.approvalBoundaries,
      'Tool-pack approval boundaries must be unique.',
    )

    const allowedEffectClasses = new Set(manifest.allowedEffectClasses)
    manifest.deniedEffectClasses.forEach((effectClass, index) => {
      if (allowedEffectClasses.has(effectClass)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['deniedEffectClasses', index],
          message: 'Tool-pack effect classes cannot be both allowed and denied.',
        })
      }
    })

    const reviewRequirementIds = manifest.reviewRequirements.map(
      (requirement) => requirement.requirementId,
    )
    addDuplicateStringIssues(
      ctx,
      ['reviewRequirements'],
      reviewRequirementIds,
      'Tool-pack review requirement ids must be unique.',
    )

    const prerequisiteIds = manifest.prerequisites.map(
      (prerequisite) => prerequisite.prerequisiteId,
    )
    addDuplicateStringIssues(
      ctx,
      ['prerequisites'],
      prerequisiteIds,
      'Tool-pack prerequisite ids must be unique.',
    )
    const prerequisiteIdSet = new Set(prerequisiteIds)

    const healthCheckIds = manifest.healthChecks.map((check) => check.checkId)
    addDuplicateStringIssues(
      ctx,
      ['healthChecks'],
      healthCheckIds,
      'Tool-pack health check ids must be unique.',
    )
    manifest.healthChecks.forEach((check, checkIndex) => {
      addDuplicateStringIssues(
        ctx,
        ['healthChecks', checkIndex, 'prerequisiteIds'],
        check.prerequisiteIds,
        'Tool-pack health check prerequisite ids must be unique.',
      )
      check.prerequisiteIds.forEach((prerequisiteId, prerequisiteIndex) => {
        if (!prerequisiteIdSet.has(prerequisiteId)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['healthChecks', checkIndex, 'prerequisiteIds', prerequisiteIndex],
            message: 'Tool-pack health checks must reference declared prerequisites.',
          })
        }
      })
    })

    const scenarioIds = manifest.scenarioChecks.map((check) => check.scenarioId)
    addDuplicateStringIssues(
      ctx,
      ['scenarioChecks'],
      scenarioIds,
      'Tool-pack scenario check ids must be unique.',
    )
    const toolSet = new Set(manifest.tools)
    manifest.scenarioChecks.forEach((scenario, scenarioIndex) => {
      addDuplicateStringIssues(
        ctx,
        ['scenarioChecks', scenarioIndex, 'toolNames'],
        scenario.toolNames,
        'Tool-pack scenario tool names must be unique.',
      )
      scenario.toolNames.forEach((toolName, toolIndex) => {
        if (!toolSet.has(toolName)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['scenarioChecks', scenarioIndex, 'toolNames', toolIndex],
            message: 'Tool-pack scenario checks must reference declared tools.',
          })
        }
      })
    })

    const uiAffordanceSurfaces = manifest.uiAffordances.map((affordance) => affordance.surface)
    addDuplicateStringIssues(
      ctx,
      ['uiAffordances'],
      uiAffordanceSurfaces,
      'Tool-pack UI affordance surfaces must be unique.',
    )
  })
export type AgentToolPackManifestDto = z.infer<typeof agentToolPackManifestSchema>

export const agentToolPackHealthStatusSchema = z.enum([
  'passed',
  'warning',
  'failed',
  'skipped',
])
export type AgentToolPackHealthStatusDto = z.infer<
  typeof agentToolPackHealthStatusSchema
>

export const agentToolPackHealthDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    remediation: z.string().trim().min(1),
  })
  .strict()
export type AgentToolPackHealthDiagnosticDto = z.infer<
  typeof agentToolPackHealthDiagnosticSchema
>

export const agentToolPackHealthCheckSchema = z
  .object({
    checkId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    status: agentToolPackHealthStatusSchema,
    summary: z.string().trim().min(1),
    diagnostic: agentToolPackHealthDiagnosticSchema.nullable().optional(),
  })
  .strict()
export type AgentToolPackHealthCheckDto = z.infer<typeof agentToolPackHealthCheckSchema>

export const agentToolPackScenarioCheckSchema = z
  .object({
    scenarioId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    status: agentToolPackHealthStatusSchema,
    summary: z.string().trim().min(1),
    toolNames: z.array(z.string().trim().min(1)),
    mutating: z.boolean(),
    requiresApproval: z.boolean(),
  })
  .strict()
export type AgentToolPackScenarioCheckDto = z.infer<
  typeof agentToolPackScenarioCheckSchema
>

export const agentToolPackHealthReportSchema = z
  .object({
    contractVersion: z.literal(1),
    packId: z.string().trim().min(1),
    label: z.string().trim().min(1),
    enabledByPolicy: z.boolean(),
    status: agentToolPackHealthStatusSchema,
    checkedAt: isoTimestampSchema,
    checks: z.array(agentToolPackHealthCheckSchema),
    scenarioChecks: z.array(agentToolPackScenarioCheckSchema),
    missingPrerequisites: z.array(z.string().trim().min(1)),
  })
  .strict()
export type AgentToolPackHealthReportDto = z.infer<typeof agentToolPackHealthReportSchema>

export const agentToolPackCatalogSchema = z
  .object({
    schema: z.literal('xero.agent_tool_pack_catalog.v1'),
    projectId: z.string().trim().min(1),
    toolPacks: z.array(agentToolPackManifestSchema),
    availablePackIds: z.array(z.string().trim().min(1)),
    healthReports: z.array(agentToolPackHealthReportSchema),
    uiDeferred: z.literal(true),
  })
  .strict()
  .superRefine((catalog, ctx) => {
    const manifestIds = catalog.toolPacks.map((pack) => pack.packId)
    addDuplicateStringIssues(
      ctx,
      ['toolPacks'],
      manifestIds,
      'Tool-pack catalog pack ids must be unique.',
    )
    const manifestsById = new Map<string, AgentToolPackManifestDto>()
    catalog.toolPacks.forEach((pack) => {
      manifestsById.set(pack.packId, pack)
    })

    addDuplicateStringIssues(
      ctx,
      ['availablePackIds'],
      catalog.availablePackIds,
      'Tool-pack catalog available pack ids must be unique.',
    )
    catalog.availablePackIds.forEach((packId, index) => {
      if (!manifestsById.has(packId)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['availablePackIds', index],
          message: 'Tool-pack catalog available pack ids must reference tool-pack manifests.',
        })
      }
    })

    const reportIds = catalog.healthReports.map((report) => report.packId)
    addDuplicateStringIssues(
      ctx,
      ['healthReports'],
      reportIds,
      'Tool-pack catalog health reports must be unique per pack.',
    )
    catalog.healthReports.forEach((report, reportIndex) => {
      const manifest = manifestsById.get(report.packId)
      if (!manifest) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['healthReports', reportIndex, 'packId'],
          message: 'Tool-pack health reports must reference tool-pack manifests.',
        })
        return
      }
      if (report.label !== manifest.label) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['healthReports', reportIndex, 'label'],
          message: 'Tool-pack health report label must match the manifest label.',
        })
      }

      const manifestHealthCheckIds = new Set(manifest.healthChecks.map((check) => check.checkId))
      const reportCheckIds = report.checks.map((check) => check.checkId)
      addDuplicateStringIssues(
        ctx,
        ['healthReports', reportIndex, 'checks'],
        reportCheckIds,
        'Tool-pack health report checks must be unique.',
      )
      report.checks.forEach((check, checkIndex) => {
        if (!manifestHealthCheckIds.has(check.checkId)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['healthReports', reportIndex, 'checks', checkIndex, 'checkId'],
            message: 'Tool-pack health report checks must reference manifest health checks.',
          })
        }
      })

      const manifestScenarioIds = new Set(
        manifest.scenarioChecks.map((scenario) => scenario.scenarioId),
      )
      const manifestToolNames = new Set(manifest.tools)
      const reportScenarioIds = report.scenarioChecks.map((scenario) => scenario.scenarioId)
      addDuplicateStringIssues(
        ctx,
        ['healthReports', reportIndex, 'scenarioChecks'],
        reportScenarioIds,
        'Tool-pack health report scenario checks must be unique.',
      )
      report.scenarioChecks.forEach((scenario, scenarioIndex) => {
        if (!manifestScenarioIds.has(scenario.scenarioId)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['healthReports', reportIndex, 'scenarioChecks', scenarioIndex, 'scenarioId'],
            message: 'Tool-pack health report scenarios must reference manifest scenarios.',
          })
        }
        scenario.toolNames.forEach((toolName, toolIndex) => {
          if (!manifestToolNames.has(toolName)) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: [
                'healthReports',
                reportIndex,
                'scenarioChecks',
                scenarioIndex,
                'toolNames',
                toolIndex,
              ],
              message: 'Tool-pack health report scenarios must reference manifest tools.',
            })
          }
        })
      })

      const manifestPrerequisiteIds = new Set(
        manifest.prerequisites.map((prerequisite) => prerequisite.prerequisiteId),
      )
      report.missingPrerequisites.forEach((prerequisiteId, prerequisiteIndex) => {
        if (!manifestPrerequisiteIds.has(prerequisiteId)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['healthReports', reportIndex, 'missingPrerequisites', prerequisiteIndex],
            message:
              'Tool-pack health report missing prerequisites must reference manifest prerequisites.',
          })
        }
      })
    })
  })
export type AgentToolPackCatalogDto = z.infer<typeof agentToolPackCatalogSchema>

export const getAgentToolPackCatalogRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
  })
  .strict()
export type GetAgentToolPackCatalogRequestDto = z.infer<
  typeof getAgentToolPackCatalogRequestSchema
>

export const agentAuthoringCatalogDiagnosticSchema = z
  .object({
    severity: z.enum(['warning', 'error']),
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    path: z.array(z.string().trim().min(1)),
  })
  .strict()
export type AgentAuthoringCatalogDiagnosticDto = z.infer<
  typeof agentAuthoringCatalogDiagnosticSchema
>

export const agentAuthoringCatalogSchema = z
  .object({
    contractVersion: z.literal(1),
    tools: z.array(agentToolSummarySchema),
    toolCategories: z.array(agentAuthoringToolCategorySchema),
    dbTables: z.array(agentAuthoringDbTableSchema),
    upstreamArtifacts: z.array(agentAuthoringUpstreamArtifactSchema),
    attachableSkills: z.array(agentAuthoringAttachableSkillSchema),
    policyControls: z.array(agentAuthoringPolicyControlSchema).default([]),
    templates: z.array(agentAuthoringTemplateSchema).default([]),
    creationFlows: z.array(agentAuthoringCreationFlowSchema).default([]),
    profileAvailability: z.array(agentAuthoringProfileAvailabilitySchema).default([]),
    constraintExplanations: z
      .array(agentAuthoringConstraintExplanationSchema)
      .default([]),
    diagnostics: z.array(agentAuthoringCatalogDiagnosticSchema),
  })
  .strict()
  .superRefine((catalog, ctx) => {
    const toolNames = catalog.tools.map((tool) => tool.name)
    addDuplicateStringIssues(
      ctx,
      ['tools'],
      toolNames,
      'Authoring catalog tool names must be unique.',
    )
    const toolsByName = new Set(toolNames)

    const categoryIds = catalog.toolCategories.map((category) => category.id)
    addDuplicateStringIssues(
      ctx,
      ['toolCategories'],
      categoryIds,
      'Authoring catalog tool category ids must be unique.',
    )
    catalog.toolCategories.forEach((category, categoryIndex) => {
      const categoryToolNames = category.tools.map((tool) => tool.name)
      addDuplicateStringIssues(
        ctx,
        ['toolCategories', categoryIndex, 'tools'],
        categoryToolNames,
        'Authoring catalog category tool names must be unique.',
      )
      category.tools.forEach((tool, toolIndex) => {
        if (!toolsByName.has(tool.name)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['toolCategories', categoryIndex, 'tools', toolIndex, 'name'],
            message: 'Authoring catalog category tools must reference catalog tools.',
          })
        }
      })
    })

    addDuplicateStringIssues(
      ctx,
      ['dbTables'],
      catalog.dbTables.map((table) => table.table),
      'Authoring catalog database tables must be unique.',
    )

    addDuplicateStringIssues(
      ctx,
      ['upstreamArtifacts'],
      catalog.upstreamArtifacts.map((artifact) =>
        [artifact.sourceAgent, artifact.contract].join(':'),
      ),
      'Authoring catalog upstream artifacts must be unique per source and contract.',
    )

    addDuplicateStringIssues(
      ctx,
      ['attachableSkills'],
      catalog.attachableSkills.map((skill) => skill.sourceId),
      'Authoring catalog attachable skill source ids must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['attachableSkills'],
      catalog.attachableSkills.map((skill) => skill.attachmentId),
      'Authoring catalog attachable skill attachment ids must be unique.',
    )

    addDuplicateStringIssues(
      ctx,
      ['policyControls'],
      catalog.policyControls.map((control) => control.id),
      'Authoring catalog policy control ids must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['policyControls'],
      catalog.policyControls.map((control) => control.snapshotPath),
      'Authoring catalog policy control snapshot paths must be unique.',
    )

    const templateIds = new Set<string>()
    const templatesById = new Map<string, AgentAuthoringTemplateDto>()
    catalog.templates.forEach((template, index) => {
      if (templateIds.has(template.id)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['templates', index, 'id'],
          message: 'Authoring template ids must be unique.',
        })
      }
      templateIds.add(template.id)
      templatesById.set(template.id, template)
    })

    const creationFlowIds = new Set<string>()
    catalog.creationFlows.forEach((flow, flowIndex) => {
      if (creationFlowIds.has(flow.id)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['creationFlows', flowIndex, 'id'],
          message: 'Authoring creation flow ids must be unique.',
        })
      }
      creationFlowIds.add(flow.id)

      if (flow.templateIds.length === 0) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['creationFlows', flowIndex, 'templateIds'],
          message: 'Authoring creation flows must reference at least one template.',
        })
      }

      const referencedTemplates = flow.templateIds.flatMap((templateId, templateIndex) => {
        const template = templatesById.get(templateId)
        if (!template) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['creationFlows', flowIndex, 'templateIds', templateIndex],
            message: 'Authoring creation flow references an unknown template id.',
          })
          return []
        }
        return [template]
      })

      const hasCompatibleTemplate = referencedTemplates.some(
        (template) =>
          template.taskKind === flow.taskKind &&
          template.baseCapabilityProfile === flow.baseCapabilityProfile &&
          template.definition.output.contract === flow.expectedOutputContract,
      )
      if (referencedTemplates.length > 0 && !hasCompatibleTemplate) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['creationFlows', flowIndex, 'templateIds'],
          message:
            'Authoring creation flow must reference a template matching its task kind, base capability profile, and expected output contract.',
        })
      }
    })

    addDuplicateStringIssues(
      ctx,
      ['profileAvailability'],
      catalog.profileAvailability.map((availability) =>
        [
          availability.subjectKind,
          availability.subjectId,
          availability.baseCapabilityProfile,
        ].join(':'),
      ),
      'Authoring profile availability entries must be unique per subject and profile.',
    )
    const availabilityByKey = new Map<string, AgentAuthoringProfileAvailabilityDto>()
    catalog.profileAvailability.forEach((availability) => {
      availabilityByKey.set(
        [
          availability.subjectKind,
          availability.subjectId,
          availability.baseCapabilityProfile,
        ].join(':'),
        availability,
      )
    })

    addDuplicateStringIssues(
      ctx,
      ['constraintExplanations'],
      catalog.constraintExplanations.map((explanation) => explanation.id),
      'Authoring constraint explanation ids must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['constraintExplanations'],
      catalog.constraintExplanations.map((explanation) =>
        [
          explanation.subjectKind,
          explanation.subjectId,
          explanation.baseCapabilityProfile,
        ].join(':'),
      ),
      'Authoring constraint explanations must be unique per subject and profile.',
    )
    catalog.constraintExplanations.forEach((explanation, index) => {
      const key = [
        explanation.subjectKind,
        explanation.subjectId,
        explanation.baseCapabilityProfile,
      ].join(':')
      const availability = availabilityByKey.get(key)
      if (!availability) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['constraintExplanations', index],
          message: 'Authoring constraint explanation must reference profile availability.',
        })
        return
      }
      if (availability.status !== explanation.status) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['constraintExplanations', index, 'status'],
          message: 'Authoring constraint explanation status must match profile availability.',
        })
      }
      if ((availability.requiredProfile ?? null) !== (explanation.requiredProfile ?? null)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['constraintExplanations', index, 'requiredProfile'],
          message:
            'Authoring constraint explanation required profile must match profile availability.',
        })
      }
    })
  })
export type AgentAuthoringCatalogDto = z.infer<typeof agentAuthoringCatalogSchema>

export const getAgentAuthoringCatalogRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    skillQuery: z.string().trim().min(1).optional(),
  })
  .strict()
export type GetAgentAuthoringCatalogRequestDto = z.infer<
  typeof getAgentAuthoringCatalogRequestSchema
>

export const searchAgentAuthoringSkillsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    query: z.string().trim().optional(),
    offset: z.number().int().nonnegative().default(0),
    limit: z.number().int().min(1).max(50).default(10),
  })
  .strict()
export type SearchAgentAuthoringSkillsRequestDto = z.infer<
  typeof searchAgentAuthoringSkillsRequestSchema
>

export const searchAgentAuthoringSkillsResponseSchema = z
  .object({
    entries: z.array(agentAuthoringSkillSearchResultSchema),
    offset: z.number().int().nonnegative(),
    limit: z.number().int().min(1).max(50),
    nextOffset: z.number().int().nonnegative().nullable(),
    hasMore: z.boolean(),
  })
  .strict()
export type SearchAgentAuthoringSkillsResponseDto = z.infer<
  typeof searchAgentAuthoringSkillsResponseSchema
>

export const resolveAgentAuthoringSkillRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    source: z.string().trim().min(1),
    skillId: z.string().trim().min(1),
  })
  .strict()
export type ResolveAgentAuthoringSkillRequestDto = z.infer<
  typeof resolveAgentAuthoringSkillRequestSchema
>

export function agentRefKey(ref: AgentRefDto): string {
  return ref.kind === 'built_in'
    ? `built_in:${ref.runtimeAgentId}@${ref.version}`
    : `custom:${ref.definitionId}@${ref.version}`
}

export function agentRefsEqual(a: AgentRefDto, b: AgentRefDto): boolean {
  if (a.kind !== b.kind) return false
  if (a.kind === 'built_in' && b.kind === 'built_in') {
    return a.runtimeAgentId === b.runtimeAgentId && a.version === b.version
  }
  if (a.kind === 'custom' && b.kind === 'custom') {
    return a.definitionId === b.definitionId && a.version === b.version
  }
  return false
}
