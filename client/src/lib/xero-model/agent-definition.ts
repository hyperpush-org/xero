import { z } from 'zod'

import { capabilityPermissionExplanationSchema } from './agent-reports'
import { isoTimestampSchema } from './shared'
import {
  skillSourceKindSchema,
  skillSourceScopeSchema,
  skillSourceStateSchema,
  skillTrustStateSchema,
} from './skills'

export const AGENT_DEFINITION_SCHEMA = 'xero.agent_definition.v1'
export const AGENT_DEFINITION_SCHEMA_VERSION = 2
const jsonObjectSchema = z.record(z.string(), z.unknown())

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

export const agentDefinitionScopeSchema = z.enum([
  'built_in',
  'global_custom',
  'project_custom',
])
export type AgentDefinitionScopeDto = z.infer<typeof agentDefinitionScopeSchema>

export const agentDefinitionLifecycleStateSchema = z.enum([
  'draft',
  'valid',
  'active',
  'archived',
  'blocked',
])
export type AgentDefinitionLifecycleStateDto = z.infer<
  typeof agentDefinitionLifecycleStateSchema
>

export const agentDefinitionBaseCapabilityProfileSchema = z.enum([
  'observe_only',
  'planning',
  'repository_recon',
  'engineering',
  'debugging',
  'agent_builder',
  'harness_test',
])
export type AgentDefinitionBaseCapabilityProfileDto = z.infer<
  typeof agentDefinitionBaseCapabilityProfileSchema
>

export const customAgentPromptRoleSchema = z.enum(['system', 'developer', 'task'])
export const customAgentToolEffectClassSchema = z.enum([
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
export const customAgentOutputContractSchema = z.enum([
  'answer',
  'plan_pack',
  'crawl_report',
  'engineering_summary',
  'debug_summary',
  'agent_definition_draft',
  'harness_test_report',
])
export const customAgentApprovalModeSchema = z.enum(['suggest', 'auto_edit', 'yolo'])
export const customAgentSubagentRoleSchema = z.enum([
  'engineer',
  'debugger',
  'planner',
  'researcher',
  'reviewer',
  'agent_builder',
  'browser',
  'emulator',
  'solana',
  'database',
])

const nonEmptyTextSchema = z.string().trim().min(1)
const trimmedTextArraySchema = z.array(nonEmptyTextSchema)

export const customAgentPromptSchema = z
  .object({
    id: nonEmptyTextSchema,
    label: nonEmptyTextSchema,
    role: customAgentPromptRoleSchema,
    source: z.string(),
    body: z.string(),
  })
  .strict()
export type CustomAgentPromptDto = z.infer<typeof customAgentPromptSchema>

export const customAgentToolSummarySchema = z
  .object({
    name: nonEmptyTextSchema,
    group: z.string(),
    description: z.string(),
    effectClass: customAgentToolEffectClassSchema,
    riskClass: z.string(),
    tags: z.array(z.string()),
    schemaFields: z.array(z.string()),
    examples: z.array(z.string()),
  })
  .strict()
export type CustomAgentToolSummaryDto = z.infer<typeof customAgentToolSummarySchema>

export const customAgentToolPolicySchema = z
  .object({
    allowedTools: trimmedTextArraySchema.optional(),
    deniedTools: trimmedTextArraySchema.optional(),
    allowedToolPacks: trimmedTextArraySchema.optional(),
    deniedToolPacks: trimmedTextArraySchema.optional(),
    allowedToolGroups: trimmedTextArraySchema.optional(),
    deniedToolGroups: trimmedTextArraySchema.optional(),
    allowedEffectClasses: z.array(customAgentToolEffectClassSchema).optional(),
    externalServiceAllowed: z.boolean().optional(),
    browserControlAllowed: z.boolean().optional(),
    skillRuntimeAllowed: z.boolean().optional(),
    subagentAllowed: z.boolean().optional(),
    allowedSubagentRoles: z.array(customAgentSubagentRoleSchema).optional(),
    deniedSubagentRoles: z.array(customAgentSubagentRoleSchema).optional(),
    commandAllowed: z.boolean().optional(),
    destructiveWriteAllowed: z.boolean().optional(),
  })
  .strict()
  .superRefine((policy, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['allowedTools'],
      policy.allowedTools ?? [],
      'Custom agent allowed tools must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedTools'],
      policy.deniedTools ?? [],
      'Custom agent denied tools must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedToolPacks'],
      policy.allowedToolPacks ?? [],
      'Custom agent allowed tool packs must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedToolPacks'],
      policy.deniedToolPacks ?? [],
      'Custom agent denied tool packs must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedToolGroups'],
      policy.allowedToolGroups ?? [],
      'Custom agent allowed tool groups must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedToolGroups'],
      policy.deniedToolGroups ?? [],
      'Custom agent denied tool groups must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedEffectClasses'],
      policy.allowedEffectClasses ?? [],
      'Custom agent allowed effect classes must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['allowedSubagentRoles'],
      policy.allowedSubagentRoles ?? [],
      'Custom agent allowed subagent roles must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['deniedSubagentRoles'],
      policy.deniedSubagentRoles ?? [],
      'Custom agent denied subagent roles must be unique.',
    )
    if (policy.subagentAllowed === true && (policy.allowedSubagentRoles?.length ?? 0) === 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['allowedSubagentRoles'],
        message: 'Custom agents that enable subagent delegation must declare allowedSubagentRoles.',
      })
    }
    const deniedRoles = new Set(policy.deniedSubagentRoles ?? [])
    for (const [index, role] of (policy.allowedSubagentRoles ?? []).entries()) {
      if (deniedRoles.has(role)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['allowedSubagentRoles', index],
          message: 'Custom agent allowed subagent roles cannot also be denied.',
        })
      }
    }
  })
export type CustomAgentToolPolicyDto = z.infer<typeof customAgentToolPolicySchema>

export const customAgentTriggerRefSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('tool'), name: nonEmptyTextSchema }).strict(),
  z.object({ kind: z.literal('output_section'), id: nonEmptyTextSchema }).strict(),
  z
    .object({
      kind: z.literal('lifecycle'),
      event: z.enum([
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
      ]),
    })
    .strict(),
  z.object({ kind: z.literal('upstream_artifact'), id: nonEmptyTextSchema }).strict(),
])

export const customAgentDbTouchpointSchema = z
  .object({
    table: nonEmptyTextSchema,
    kind: z.enum(['read', 'write', 'encouraged']),
    purpose: nonEmptyTextSchema,
    triggers: z.array(customAgentTriggerRefSchema),
    columns: trimmedTextArraySchema,
  })
  .strict()
  .superRefine((touchpoint, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['columns'],
      touchpoint.columns,
      'Custom agent database touchpoint columns must be unique.',
    )
  })

export const customAgentDbTouchpointsSchema = z
  .object({
    reads: z.array(customAgentDbTouchpointSchema),
    writes: z.array(customAgentDbTouchpointSchema),
    encouraged: z.array(customAgentDbTouchpointSchema),
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
            message: `Custom agent database touchpoint ${section} entries must use kind ${expectedKind}.`,
          })
        }
      })
    })
  })

export const customAgentOutputSectionSchema = z
  .object({
    id: nonEmptyTextSchema,
    label: nonEmptyTextSchema,
    description: nonEmptyTextSchema,
    emphasis: z.enum(['core', 'standard', 'optional']),
    producedByTools: trimmedTextArraySchema,
  })
  .strict()
  .superRefine((section, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['producedByTools'],
      section.producedByTools,
      'Custom agent output produced-by tool ids must be unique.',
    )
  })

export const customAgentOutputSchema = z
  .object({
    contract: customAgentOutputContractSchema,
    label: nonEmptyTextSchema,
    description: nonEmptyTextSchema,
    sections: z.array(customAgentOutputSectionSchema),
  })
  .strict()
  .superRefine((output, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['sections'],
      output.sections.map((section) => section.id),
      'Custom agent output section ids must be unique.',
    )
  })

export const customAgentConsumedArtifactSchema = z
  .object({
    id: nonEmptyTextSchema,
    label: nonEmptyTextSchema,
    description: nonEmptyTextSchema,
    sourceAgent: z.enum(['ask', 'plan', 'engineer', 'debug', 'crawl', 'agent_create', 'test']),
    contract: customAgentOutputContractSchema,
    sections: trimmedTextArraySchema,
    required: z.boolean(),
  })
  .strict()
  .superRefine((artifact, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['sections'],
      artifact.sections,
      'Custom agent consumed artifact section ids must be unique.',
    )
  })

export const customAgentProjectDataPolicySchema = z
  .object({
    recordKinds: trimmedTextArraySchema,
    structuredSchemas: trimmedTextArraySchema.optional(),
    unstructuredScopes: trimmedTextArraySchema.optional(),
    memoryCandidateKinds: trimmedTextArraySchema.optional(),
  })
  .strict()
  .superRefine((policy, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['recordKinds'],
      policy.recordKinds,
      'Custom agent project-data record kinds must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['structuredSchemas'],
      policy.structuredSchemas ?? [],
      'Custom agent project-data structured schemas must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['unstructuredScopes'],
      policy.unstructuredScopes ?? [],
      'Custom agent project-data unstructured scopes must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['memoryCandidateKinds'],
      policy.memoryCandidateKinds ?? [],
      'Custom agent project-data memory candidate kinds must be unique.',
    )
  })

export const customAgentMemoryPolicySchema = z
  .object({
    memoryKinds: trimmedTextArraySchema,
    reviewRequired: z.boolean().optional(),
  })
  .strict()
  .superRefine((policy, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['memoryKinds'],
      policy.memoryKinds,
      'Custom agent memory kinds must be unique.',
    )
  })

export const customAgentRetrievalPolicySchema = z
  .object({
    enabled: z.boolean(),
    recordKinds: trimmedTextArraySchema.optional(),
    memoryKinds: trimmedTextArraySchema.optional(),
    limit: z.number().int().positive().optional(),
  })
  .strict()
  .superRefine((policy, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['recordKinds'],
      policy.recordKinds ?? [],
      'Custom agent retrieval record kinds must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['memoryKinds'],
      policy.memoryKinds ?? [],
      'Custom agent retrieval memory kinds must be unique.',
    )
  })

export const customAgentHandoffPolicySchema = z
  .object({
    enabled: z.boolean(),
    preserveDefinitionVersion: z.boolean().optional(),
  })
  .strict()

export const customAgentWorkflowGateSchema = z.discriminatedUnion('kind', [
  z
    .object({
      kind: z.literal('todo_completed'),
      todoId: nonEmptyTextSchema,
      description: z.string().optional(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('tool_succeeded'),
      toolName: nonEmptyTextSchema,
      minCount: z.number().int().positive().optional(),
      description: z.string().optional(),
    })
    .strict(),
])
export type CustomAgentWorkflowGateDto = z.infer<typeof customAgentWorkflowGateSchema>

export const customAgentWorkflowBranchConditionSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('always') }).strict(),
  z
    .object({
      kind: z.literal('todo_completed'),
      todoId: nonEmptyTextSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('tool_succeeded'),
      toolName: nonEmptyTextSchema,
      minCount: z.number().int().positive().optional(),
    })
    .strict(),
])
export type CustomAgentWorkflowBranchConditionDto = z.infer<
  typeof customAgentWorkflowBranchConditionSchema
>

export const customAgentWorkflowBranchSchema = z
  .object({
    targetPhaseId: nonEmptyTextSchema,
    condition: customAgentWorkflowBranchConditionSchema,
    label: z.string().optional(),
  })
  .strict()
export type CustomAgentWorkflowBranchDto = z.infer<typeof customAgentWorkflowBranchSchema>

export const customAgentWorkflowPhaseSchema = z
  .object({
    id: nonEmptyTextSchema,
    title: nonEmptyTextSchema,
    description: z.string().optional(),
    allowedTools: trimmedTextArraySchema.optional(),
    requiredChecks: z.array(customAgentWorkflowGateSchema).optional(),
    retryLimit: z.number().int().nonnegative().optional(),
    branches: z.array(customAgentWorkflowBranchSchema).optional(),
  })
  .strict()
  .superRefine((phase, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['allowedTools'],
      phase.allowedTools ?? [],
      'Custom agent workflow phase allowed tools must be unique.',
    )
    const requiredCheckKeys =
      phase.requiredChecks?.map((check) => {
        switch (check.kind) {
          case 'todo_completed':
            return `todo_completed:${check.todoId}`
          case 'tool_succeeded':
            return `tool_succeeded:${check.toolName}:${check.minCount ?? 1}`
        }
      }) ?? []
    addDuplicateStringIssues(
      ctx,
      ['requiredChecks'],
      requiredCheckKeys,
      'Custom agent workflow phase required checks must be unique.',
    )
    const branchKeys =
      phase.branches?.map((branch) =>
        [branch.targetPhaseId, branch.condition.kind, JSON.stringify(branch.condition)].join(':'),
      ) ?? []
    addDuplicateStringIssues(
      ctx,
      ['branches'],
      branchKeys,
      'Custom agent workflow phase branches must be unique.',
    )
  })
export type CustomAgentWorkflowPhaseDto = z.infer<typeof customAgentWorkflowPhaseSchema>

export const customAgentWorkflowStructureSchema = z
  .object({
    startPhaseId: nonEmptyTextSchema.optional(),
    phases: z.array(customAgentWorkflowPhaseSchema).min(1),
  })
  .strict()
  .superRefine((workflow, ctx) => {
    const phaseIds = workflow.phases.map((phase) => phase.id)
    addDuplicateStringIssues(
      ctx,
      ['phases'],
      phaseIds,
      'Custom agent workflow phase ids must be unique.',
    )
    const phaseIdSet = new Set(phaseIds)
    if (workflow.startPhaseId && !phaseIdSet.has(workflow.startPhaseId)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['startPhaseId'],
        message: 'Custom agent workflow start phase must reference a declared phase.',
      })
    }
    workflow.phases.forEach((phase, phaseIndex) => {
      phase.branches?.forEach((branch, branchIndex) => {
        if (!phaseIdSet.has(branch.targetPhaseId)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['phases', phaseIndex, 'branches', branchIndex, 'targetPhaseId'],
            message: 'Custom agent workflow branch target must reference a declared phase.',
          })
        }
      })
    })
  })
export type CustomAgentWorkflowStructureDto = z.infer<
  typeof customAgentWorkflowStructureSchema
>

export const customAgentAttachedSkillSchema = z
  .object({
    id: nonEmptyTextSchema,
    sourceId: nonEmptyTextSchema,
    skillId: nonEmptyTextSchema,
    name: nonEmptyTextSchema,
    description: z.string(),
    sourceKind: skillSourceKindSchema,
    scope: skillSourceScopeSchema,
    versionHash: nonEmptyTextSchema,
    includeSupportingAssets: z.boolean(),
    required: z.literal(true),
  })
  .strict()
export type CustomAgentAttachedSkillDto = z.infer<
  typeof customAgentAttachedSkillSchema
>

export const canonicalCustomAgentDefinitionBaseSchema = z
  .object({
    schema: z.literal(AGENT_DEFINITION_SCHEMA),
    schemaVersion: z.literal(AGENT_DEFINITION_SCHEMA_VERSION),
    id: nonEmptyTextSchema,
    version: z.number().int().positive().optional(),
    displayName: nonEmptyTextSchema,
    shortLabel: nonEmptyTextSchema,
    description: z.string(),
    taskPurpose: z.string(),
    scope: z.enum(['global_custom', 'project_custom']),
    lifecycleState: agentDefinitionLifecycleStateSchema,
    baseCapabilityProfile: agentDefinitionBaseCapabilityProfileSchema.exclude(['harness_test']),
    defaultApprovalMode: customAgentApprovalModeSchema,
    allowedApprovalModes: z.array(customAgentApprovalModeSchema).min(1),
    toolPolicy: z.union([
      z.enum(['observe_only', 'planning', 'repository_recon', 'engineering', 'agent_builder']),
      customAgentToolPolicySchema,
    ]),
    workflowContract: z.string(),
    workflowStructure: customAgentWorkflowStructureSchema.optional(),
    finalResponseContract: z.string(),
    examplePrompts: z.array(z.string()).min(3),
    refusalEscalationCases: z.array(z.string()).min(3),
    attachedSkills: z.array(customAgentAttachedSkillSchema),
    prompts: z.array(customAgentPromptSchema),
    tools: z.array(customAgentToolSummarySchema),
    output: customAgentOutputSchema,
    dbTouchpoints: customAgentDbTouchpointsSchema,
    consumes: z.array(customAgentConsumedArtifactSchema),
    promptFragments: z.unknown().optional(),
    projectDataPolicy: customAgentProjectDataPolicySchema.optional(),
    memoryCandidatePolicy: customAgentMemoryPolicySchema.optional(),
    retrievalDefaults: customAgentRetrievalPolicySchema.optional(),
    handoffPolicy: customAgentHandoffPolicySchema.optional(),
    defaultModel: z.unknown().optional(),
    capabilities: z.unknown().optional(),
    safetyLimits: z.unknown().optional(),
  })
  .strict()

export const validateCanonicalCustomAgentDefinition = (
  definition: z.infer<typeof canonicalCustomAgentDefinitionBaseSchema>,
  ctx: z.RefinementCtx,
) => {
  addDuplicateStringIssues(
    ctx,
    ['allowedApprovalModes'],
    definition.allowedApprovalModes,
    'Custom agent allowed approval modes must be unique.',
  )
  if (!definition.allowedApprovalModes.includes(definition.defaultApprovalMode)) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ['defaultApprovalMode'],
      message: 'Custom agent default approval mode must be allowed.',
    })
  }
  addDuplicateStringIssues(
    ctx,
    ['prompts'],
    definition.prompts.map((prompt) => prompt.id),
    'Custom agent prompt ids must be unique.',
  )
  addDuplicateStringIssues(
    ctx,
    ['tools'],
    definition.tools.map((tool) => tool.name),
    'Custom agent tool names must be unique.',
  )
  addDuplicateStringIssues(
    ctx,
    ['consumes'],
    definition.consumes.map((artifact) => artifact.id),
    'Custom agent consumed artifact ids must be unique.',
  )
  addDuplicateStringIssues(
    ctx,
    ['attachedSkills'],
    definition.attachedSkills.map((skill) => skill.id),
    'Custom agent attached skill ids must be unique.',
  )
  addDuplicateStringIssues(
    ctx,
    ['attachedSkills'],
    definition.attachedSkills.map((skill) => skill.sourceId),
    'Custom agent attached skill source ids must be unique.',
  )
}

export const canonicalCustomAgentDefinitionSchema =
  canonicalCustomAgentDefinitionBaseSchema.superRefine(validateCanonicalCustomAgentDefinition)
export type CanonicalCustomAgentDefinitionDto = z.infer<
  typeof canonicalCustomAgentDefinitionSchema
>

export const agentDefinitionSummarySchema = z
  .object({
    definitionId: z.string().trim().min(1),
    currentVersion: z.number().int().positive(),
    displayName: z.string().trim().min(1),
    shortLabel: z.string().trim().min(1),
    description: z.string(),
    scope: agentDefinitionScopeSchema,
    lifecycleState: agentDefinitionLifecycleStateSchema,
    baseCapabilityProfile: agentDefinitionBaseCapabilityProfileSchema,
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
    isBuiltIn: z.boolean(),
  })
  .strict()
export type AgentDefinitionSummaryDto = z.infer<typeof agentDefinitionSummarySchema>

export const agentDefinitionVersionSummarySchema = z
  .object({
    definitionId: z.string().trim().min(1),
    version: z.number().int().positive(),
    createdAt: isoTimestampSchema,
    validationStatus: z.string().nullable().optional(),
    validationDiagnosticCount: z.number().int().nonnegative(),
    snapshot: z.unknown(),
    validationReport: z.unknown().nullable().optional(),
  })
  .strict()
export type AgentDefinitionVersionSummaryDto = z.infer<
  typeof agentDefinitionVersionSummarySchema
>

export const listAgentDefinitionsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    includeArchived: z.boolean().default(false),
  })
  .strict()
export type ListAgentDefinitionsRequestDto = z.infer<typeof listAgentDefinitionsRequestSchema>

export const listAgentDefinitionsResponseSchema = z
  .object({
    definitions: z.array(agentDefinitionSummarySchema),
  })
  .strict()
export type ListAgentDefinitionsResponseDto = z.infer<
  typeof listAgentDefinitionsResponseSchema
>

export const archiveAgentDefinitionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    definitionId: z.string().trim().min(1),
  })
  .strict()
export type ArchiveAgentDefinitionRequestDto = z.infer<
  typeof archiveAgentDefinitionRequestSchema
>

export const getAgentDefinitionVersionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    definitionId: z.string().trim().min(1),
    version: z.number().int().positive(),
  })
  .strict()
export type GetAgentDefinitionVersionRequestDto = z.infer<
  typeof getAgentDefinitionVersionRequestSchema
>

export const getAgentDefinitionVersionDiffRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    definitionId: z.string().trim().min(1),
    fromVersion: z.number().int().positive(),
    toVersion: z.number().int().positive(),
  })
  .strict()
  .superRefine((request, ctx) => {
    if (request.fromVersion === request.toVersion) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['toVersion'],
        message: 'Definition version diff requests must compare distinct versions.',
      })
    }
  })
export type GetAgentDefinitionVersionDiffRequestDto = z.infer<
  typeof getAgentDefinitionVersionDiffRequestSchema
>

export const agentDefinitionVersionDiffSectionSchema = z
  .object({
    section: z.string().trim().min(1),
    fields: z.array(z.string().trim().min(1)),
    changed: z.boolean(),
    before: jsonObjectSchema,
    after: jsonObjectSchema,
  })
  .strict()
  .superRefine((section, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['fields'],
      section.fields,
      'Definition diff section fields must be unique.',
    )
  })

export const agentDefinitionVersionDiffSchema = z
  .object({
    schema: z.literal('xero.agent_definition_version_diff.v1'),
    definitionId: z.string().trim().min(1),
    fromVersion: z.number().int().positive(),
    toVersion: z.number().int().positive(),
    fromCreatedAt: isoTimestampSchema,
    toCreatedAt: isoTimestampSchema,
    changed: z.boolean(),
    changedSections: z.array(z.string().trim().min(1)),
    sections: z.array(agentDefinitionVersionDiffSectionSchema),
  })
  .strict()
  .superRefine((diff, ctx) => {
    if (diff.fromVersion === diff.toVersion) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['toVersion'],
        message: 'Definition version diffs must compare distinct versions.',
      })
    }
    addDuplicateStringIssues(
      ctx,
      ['changedSections'],
      diff.changedSections,
      'Definition diff changedSections must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['sections'],
      diff.sections.map((section) => section.section),
      'Definition diff sections must be unique.',
    )
    const changedSections = diff.sections
      .filter((section) => section.changed)
      .map((section) => section.section)
    const declared = [...diff.changedSections].sort()
    const computed = [...changedSections].sort()
    if (diff.changed !== (changedSections.length > 0)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['changed'],
        message: 'Definition diff changed flag must match changed sections.',
      })
    }
    if (
      declared.length !== computed.length ||
      declared.some((section, index) => section !== computed[index])
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['changedSections'],
        message: 'Definition diff changedSections must match changed section entries.',
      })
    }
  })
export type AgentDefinitionVersionDiffDto = z.infer<
  typeof agentDefinitionVersionDiffSchema
>

export const saveAgentDefinitionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    definition: canonicalCustomAgentDefinitionSchema,
    definitionId: z.string().trim().min(1).nullable().optional(),
  })
  .strict()
  .superRefine((request, ctx) => {
    if (request.definitionId != null && request.definitionId !== request.definition.id) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['definitionId'],
        message: 'Save definition request id must match the canonical definition id.',
      })
    }
  })
export type SaveAgentDefinitionRequestDto = z.infer<
  typeof saveAgentDefinitionRequestSchema
>

export const updateAgentDefinitionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    definitionId: z.string().trim().min(1),
    definition: canonicalCustomAgentDefinitionSchema,
  })
  .strict()
  .superRefine((request, ctx) => {
    if (request.definitionId !== request.definition.id) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['definitionId'],
        message: 'Update definition request id must match the canonical definition id.',
      })
    }
  })
export type UpdateAgentDefinitionRequestDto = z.infer<
  typeof updateAgentDefinitionRequestSchema
>

export const previewAgentDefinitionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    definitionId: z.string().trim().min(1).nullable().optional(),
    definition: canonicalCustomAgentDefinitionSchema,
  })
  .strict()
  .superRefine((request, ctx) => {
    if (request.definitionId != null && request.definitionId !== request.definition.id) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['definitionId'],
        message: 'Preview definition request id must match the canonical definition id.',
      })
    }
  })
export type PreviewAgentDefinitionRequestDto = z.infer<
  typeof previewAgentDefinitionRequestSchema
>

export const agentPreviewPromptFragmentSchema = z
  .object({
    id: z.string().trim().min(1),
    priority: z.number().int().nonnegative(),
    title: z.string().trim().min(1),
    provenance: z.string().trim().min(1),
    budgetPolicy: z.string().trim().min(1),
    inclusionReason: z.string().trim().min(1),
    content: z.string().trim().min(1),
    sha256: z.string().trim().regex(/^[a-f0-9]{64}$/),
    tokenEstimate: z.number().int().nonnegative(),
  })
  .strict()

export const agentPreviewPromptSchema = z
  .object({
    compiler: z.literal('PromptCompiler'),
    selectionMode: z.literal('capability_ceiling_without_task_prompt'),
    promptSha256: z.string().trim().regex(/^[a-f0-9]{64}$/),
    promptBudgetTokens: z.number().int().positive(),
    estimatedPromptTokens: z.number().int().nonnegative(),
    fragmentCount: z.number().int().nonnegative(),
    fragmentIds: z.array(z.string().trim().min(1)),
    fragments: z.array(agentPreviewPromptFragmentSchema),
  })
  .strict()
  .superRefine((prompt, ctx) => {
    const fragmentIds = prompt.fragments.map((fragment) => fragment.id)
    if (prompt.fragmentCount !== prompt.fragments.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['fragmentCount'],
        message: 'Preview prompt fragmentCount must match fragments.',
      })
    }
    if (
      prompt.fragmentIds.length !== fragmentIds.length ||
      prompt.fragmentIds.some((id, index) => id !== fragmentIds[index])
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['fragmentIds'],
        message: 'Preview prompt fragmentIds must match fragment order.',
      })
    }
  })

export const agentPreviewGraphDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    path: z.string().trim().min(1),
    message: z.string().trim().min(1),
    deniedTool: z.string().trim().min(1).nullable(),
    deniedEffectClass: z.string().trim().min(1).nullable(),
    baseCapabilityProfile: z.string().trim().min(1).nullable(),
    reason: z.string().trim().min(1).nullable(),
    repairHint: z.string().trim().min(1).nullable(),
  })
  .strict()

export const agentPreviewGraphValidationCategorySchema = z
  .object({
    category: z.string().trim().min(1),
    count: z.number().int().nonnegative(),
    diagnostics: z.array(agentPreviewGraphDiagnosticSchema),
  })
  .strict()
  .superRefine((category, ctx) => {
    if (category.count !== category.diagnostics.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['count'],
        message: 'Graph validation category count must match diagnostics.',
      })
    }
  })

export const agentPreviewGraphValidationSchema = z
  .object({
    schema: z.literal('xero.agent_graph_validation_summary.v1'),
    status: z.enum(['valid', 'invalid']),
    diagnosticCount: z.number().int().nonnegative(),
    categories: z.array(agentPreviewGraphValidationCategorySchema),
  })
  .strict()
  .superRefine((validation, ctx) => {
    const total = validation.categories.reduce((sum, category) => sum + category.count, 0)
    if (validation.diagnosticCount < total) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['diagnosticCount'],
        message: 'Graph validation diagnosticCount must cover category diagnostics.',
      })
    }
    if (validation.status === 'valid' && validation.diagnosticCount !== 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['diagnosticCount'],
        message: 'Valid graph validation summaries must not include diagnostics.',
      })
    }
    if (validation.status === 'invalid' && validation.diagnosticCount === 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['diagnosticCount'],
        message: 'Invalid graph validation summaries must include diagnostics.',
      })
    }
  })

export const agentPreviewGraphRepairHintSchema = z
  .object({
    kind: z.string().trim().min(1),
    capabilityId: z.string().trim().min(1),
    status: z.enum(['supported', 'partially_supported', 'unsupported']),
    reasonCodes: z.array(z.string().trim().min(1)).optional(),
    note: z.string().trim().min(1),
  })
  .strict()

export const agentPreviewGraphRepairHintsSchema = z
  .object({
    schema: z.literal('xero.agent_graph_repair_hints.v1'),
    supported: z.array(agentPreviewGraphRepairHintSchema),
    partiallySupported: z.array(agentPreviewGraphRepairHintSchema),
    unsupported: z.array(agentPreviewGraphRepairHintSchema),
  })
  .strict()
  .superRefine((hints, ctx) => {
    const buckets = [
      ['supported', hints.supported, 'supported'],
      ['partiallySupported', hints.partiallySupported, 'partially_supported'],
      ['unsupported', hints.unsupported, 'unsupported'],
    ] as const
    const seen = new Set<string>()
    buckets.forEach(([bucket, entries, expectedStatus]) => {
      entries.forEach((hint, index) => {
        if (hint.status !== expectedStatus) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: [bucket, index, 'status'],
            message: 'Graph repair hint status must match its response bucket.',
          })
        }
        const key = [hint.kind, hint.capabilityId].join(':')
        if (seen.has(key)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: [bucket, index, 'capabilityId'],
            message: 'Graph repair hints must be unique per capability.',
          })
        }
        seen.add(key)
      })
    })
  })

export const agentPreviewAttachedSkillDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    path: z.string().trim().min(1),
    message: z.string().trim().min(1),
    reason: z.string().trim().min(1).nullable(),
    repairHint: z.string().trim().min(1).nullable(),
  })
  .strict()

export const agentPreviewAttachedSkillInjectionEntrySchema = z
  .object({
    attachmentId: z.string(),
    sourceId: z.string(),
    skillId: z.string(),
    name: z.string(),
    sourceKind: z.union([skillSourceKindSchema, z.literal('')]),
    scope: z.union([skillSourceScopeSchema, z.literal('')]),
    required: z.boolean(),
    includeSupportingAssets: z.boolean(),
    pinnedVersionHash: z.string(),
    registryVersionHash: z.string().nullable(),
    sourceState: skillSourceStateSchema.nullable(),
    trustState: skillTrustStateSchema.nullable(),
    status: z.enum(['resolved', 'stale', 'unavailable', 'blocked']),
    willInject: z.boolean(),
    skillToolRequired: z.literal(false),
    reasonCodes: z.array(z.string().trim().min(1)),
    repairHints: z.array(z.string().trim().min(1)),
    explanation: z.string().trim().min(1),
    diagnostics: z.array(agentPreviewAttachedSkillDiagnosticSchema),
  })
  .strict()
  .superRefine((entry, ctx) => {
    if (entry.status === 'resolved' && !entry.willInject) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['willInject'],
        message: 'Resolved attached skill preview entries must inject.',
      })
    }
    if (entry.status !== 'resolved' && entry.willInject) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['willInject'],
        message: 'Blocked attached skill preview entries must not inject.',
      })
    }
  })

export const agentPreviewAttachedSkillInjectionSchema = z
  .object({
    schema: z.literal('xero.agent_attached_skill_injection_preview.v1'),
    schemaVersion: z.literal(1),
    selectionMode: z.literal('definition_attached_skills_without_skill_tool'),
    status: z.enum(['resolved', 'blocked']),
    skillToolRequired: z.literal(false),
    attachmentCount: z.number().int().nonnegative(),
    resolvedCount: z.number().int().nonnegative(),
    staleCount: z.number().int().nonnegative(),
    unavailableCount: z.number().int().nonnegative(),
    blockedCount: z.number().int().nonnegative(),
    entries: z.array(agentPreviewAttachedSkillInjectionEntrySchema),
  })
  .strict()
  .superRefine((preview, ctx) => {
    if (preview.attachmentCount !== preview.entries.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['attachmentCount'],
        message: 'Attached skill preview attachmentCount must match entries.',
      })
    }
    const counts = preview.entries.reduce(
      (sum, entry) => {
        sum[entry.status] += 1
        return sum
      },
      { resolved: 0, stale: 0, unavailable: 0, blocked: 0 },
    )
    if (preview.resolvedCount !== counts.resolved) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['resolvedCount'],
        message: 'Attached skill preview resolvedCount must match entries.',
      })
    }
    if (preview.staleCount !== counts.stale) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['staleCount'],
        message: 'Attached skill preview staleCount must match entries.',
      })
    }
    if (preview.unavailableCount !== counts.unavailable) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['unavailableCount'],
        message: 'Attached skill preview unavailableCount must match entries.',
      })
    }
    if (preview.blockedCount !== counts.blocked) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['blockedCount'],
        message: 'Attached skill preview blockedCount must match entries.',
      })
    }
    if (
      preview.status === 'resolved' &&
      (preview.staleCount > 0 || preview.unavailableCount > 0 || preview.blockedCount > 0)
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['status'],
        message: 'Attached skill preview cannot be resolved when an attachment will not inject.',
      })
    }
  })

export const agentPreviewToolAccessEntrySchema = z
  .object({
    toolName: z.string().trim().min(1),
    group: z.string().trim().min(1),
    description: z.string().trim().min(1),
    riskClass: z.string().trim().min(1),
    effectClass: z.string().trim().min(1),
    tags: z.array(z.string().trim().min(1)),
    schemaFields: z.array(z.string().trim().min(1)),
    runtimeProfileAllowed: z.boolean(),
    customPolicyAllowed: z.boolean(),
    hostAvailable: z.boolean(),
    effectiveAllowed: z.boolean(),
    deniedBy: z.array(z.string().trim().min(1)),
  })
  .strict()

export const agentPreviewEffectiveToolAccessSchema = z
  .object({
    selectionMode: z.literal('capability_ceiling_without_task_prompt'),
    skillToolEnabled: z.boolean(),
    runtimeAgentId: z.string().trim().min(1),
    requestedTools: z.array(z.string().trim().min(1)),
    requestedEffectClasses: z.array(z.string().trim().min(1)),
    explicitlyDeniedTools: z.array(z.string().trim().min(1)),
    allowedToolCount: z.number().int().nonnegative(),
    deniedCapabilityCount: z.number().int().nonnegative(),
    allowedTools: z.array(agentPreviewToolAccessEntrySchema),
    deniedCapabilities: z.array(agentPreviewToolAccessEntrySchema),
  })
  .strict()
  .superRefine((access, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['requestedTools'],
      access.requestedTools,
      'Effective tool access requested tools must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['requestedEffectClasses'],
      access.requestedEffectClasses,
      'Effective tool access requested effect classes must be unique.',
    )
    addDuplicateStringIssues(
      ctx,
      ['explicitlyDeniedTools'],
      access.explicitlyDeniedTools,
      'Effective tool access explicitly denied tools must be unique.',
    )
    if (access.allowedToolCount !== access.allowedTools.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['allowedToolCount'],
        message: 'Effective tool access allowedToolCount must match allowedTools.',
      })
    }
    if (access.deniedCapabilityCount !== access.deniedCapabilities.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['deniedCapabilityCount'],
        message: 'Effective tool access deniedCapabilityCount must match deniedCapabilities.',
      })
    }
    const allowedToolNames = access.allowedTools.map((tool) => tool.toolName)
    addDuplicateStringIssues(
      ctx,
      ['allowedTools'],
      allowedToolNames,
      'Effective tool access allowed tool names must be unique.',
    )
    const deniedToolNames = access.deniedCapabilities.map((tool) => tool.toolName)
    addDuplicateStringIssues(
      ctx,
      ['deniedCapabilities'],
      deniedToolNames,
      'Effective tool access denied capability names must be unique.',
    )
    const deniedToolNameSet = new Set(deniedToolNames)
    access.allowedTools.forEach((tool, index) => {
      if (!tool.effectiveAllowed) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['allowedTools', index, 'effectiveAllowed'],
          message: 'Effective tool access allowed tools must be effectively allowed.',
        })
      }
      if (deniedToolNameSet.has(tool.toolName)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['allowedTools', index, 'toolName'],
          message: 'Effective tool access cannot list a tool as both allowed and denied.',
        })
      }
    })
    access.deniedCapabilities.forEach((tool, index) => {
      if (tool.effectiveAllowed) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['deniedCapabilities', index, 'effectiveAllowed'],
          message: 'Effective tool access denied capabilities must not be effectively allowed.',
        })
      }
    })
  })

export const agentPreviewPoliciesSchema = z
  .object({
    toolPolicy: z.union([jsonObjectSchema, z.string().trim().min(1)]).nullable(),
    outputContract: z.union([jsonObjectSchema, z.string().trim().min(1)]).nullable(),
    contextPolicy: jsonObjectSchema.nullable(),
    memoryPolicy: jsonObjectSchema.nullable(),
    retrievalPolicy: jsonObjectSchema.nullable(),
    handoffPolicy: jsonObjectSchema.nullable(),
    attachedSkills: z.array(customAgentAttachedSkillSchema),
    workflowContract: z.string().trim().min(1).nullable(),
    workflowStructure: jsonObjectSchema.nullable(),
    finalResponseContract: z.string().trim().min(1).nullable(),
  })
  .strict()

export const agentPreviewRiskyCapabilityPromptSchema = z
  .object({
    flag: z.string().trim().min(1),
    effectClass: z.string().trim().min(1),
    enabled: z.boolean(),
    requiresOperatorPrompt: z.literal(true),
    prompt: z.string().trim().min(1),
  })
  .strict()

export const agentPreviewRuntimeConsistencySchema = z
  .object({
    toolPolicySource: z.literal('AutonomousAgentToolPolicy::from_definition_snapshot'),
    toolRegistrySource: z.literal('ToolRegistry::builtin_with_options'),
    promptCompilerSource: z.literal('PromptCompiler::with_agent_definition_snapshot'),
    taskPromptNarrowing: z.literal('not_applied_in_preview'),
  })
  .strict()

export const agentEffectiveRuntimePreviewSchema = z
  .object({
    schema: z.literal('xero.agent_effective_runtime_preview.v1'),
    schemaVersion: z.literal(1),
    source: z
      .object({
        kind: z.literal('normalized_agent_definition_snapshot'),
        uiDeferred: z.literal(true),
        uiDeferralReason: z.string().trim().min(1),
      })
      .strict(),
    definition: z
      .object({
        definitionId: z.string().trim().min(1),
        version: z.number().int().positive(),
        displayName: z.string().trim().min(1),
        scope: z.string().trim().min(1),
        lifecycleState: z.string().trim().min(1),
        baseCapabilityProfile: z.string().trim().min(1),
        runtimeAgentId: z.string().trim().min(1),
      })
      .strict(),
    validation: z.unknown(),
    prompt: agentPreviewPromptSchema,
    graphValidation: agentPreviewGraphValidationSchema,
    graphRepairHints: agentPreviewGraphRepairHintsSchema,
    attachedSkillInjection: agentPreviewAttachedSkillInjectionSchema,
    effectiveToolAccess: agentPreviewEffectiveToolAccessSchema,
    capabilityPermissionExplanations: z.array(capabilityPermissionExplanationSchema),
    policies: agentPreviewPoliciesSchema,
    riskyCapabilityPrompts: z.array(agentPreviewRiskyCapabilityPromptSchema),
    runtimeConsistency: agentPreviewRuntimeConsistencySchema,
  })
  .strict()
export type AgentEffectiveRuntimePreviewDto = z.infer<
  typeof agentEffectiveRuntimePreviewSchema
>
export type AgentPreviewPromptFragmentDto = z.infer<typeof agentPreviewPromptFragmentSchema>
export type AgentPreviewPromptDto = z.infer<typeof agentPreviewPromptSchema>
export type AgentPreviewGraphDiagnosticDto = z.infer<typeof agentPreviewGraphDiagnosticSchema>
export type AgentPreviewGraphValidationCategoryDto = z.infer<
  typeof agentPreviewGraphValidationCategorySchema
>
export type AgentPreviewGraphValidationDto = z.infer<typeof agentPreviewGraphValidationSchema>
export type AgentPreviewGraphRepairHintDto = z.infer<typeof agentPreviewGraphRepairHintSchema>
export type AgentPreviewGraphRepairHintsDto = z.infer<typeof agentPreviewGraphRepairHintsSchema>
export type AgentPreviewAttachedSkillDiagnosticDto = z.infer<
  typeof agentPreviewAttachedSkillDiagnosticSchema
>
export type AgentPreviewAttachedSkillInjectionEntryDto = z.infer<
  typeof agentPreviewAttachedSkillInjectionEntrySchema
>
export type AgentPreviewAttachedSkillInjectionDto = z.infer<
  typeof agentPreviewAttachedSkillInjectionSchema
>
export type AgentPreviewToolAccessEntryDto = z.infer<typeof agentPreviewToolAccessEntrySchema>
export type AgentPreviewEffectiveToolAccessDto = z.infer<
  typeof agentPreviewEffectiveToolAccessSchema
>
export type AgentPreviewPoliciesDto = z.infer<typeof agentPreviewPoliciesSchema>
export type AgentPreviewRiskyCapabilityPromptDto = z.infer<
  typeof agentPreviewRiskyCapabilityPromptSchema
>
export type AgentPreviewRuntimeConsistencyDto = z.infer<
  typeof agentPreviewRuntimeConsistencySchema
>

export const agentDefinitionValidationStatusSchema = z.enum(['valid', 'invalid'])
export type AgentDefinitionValidationStatusDto = z.infer<
  typeof agentDefinitionValidationStatusSchema
>

export const agentDefinitionValidationDiagnosticSchema = z
  .object({
    code: z.string(),
    message: z.string(),
    path: z.string(),
    deniedTool: z.string().nullable().optional(),
    deniedEffectClass: z.string().nullable().optional(),
    baseCapabilityProfile: z.string().nullable().optional(),
    reason: z.string().nullable().optional(),
    repairHint: z.string().nullable().optional(),
  })
  .strict()
export type AgentDefinitionValidationDiagnosticDto = z.infer<
  typeof agentDefinitionValidationDiagnosticSchema
>

export const agentDefinitionValidationReportSchema = z
  .object({
    status: agentDefinitionValidationStatusSchema,
    diagnostics: z.array(agentDefinitionValidationDiagnosticSchema),
  })
  .strict()
  .superRefine((report, ctx) => {
    if (report.status === 'valid' && report.diagnostics.length > 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['diagnostics'],
        message: 'Valid agent definition validation reports must not include diagnostics.',
      })
    }
    if (report.status === 'invalid' && report.diagnostics.length === 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['diagnostics'],
        message: 'Invalid agent definition validation reports must include diagnostics.',
      })
    }
  })
export type AgentDefinitionValidationReportDto = z.infer<
  typeof agentDefinitionValidationReportSchema
>

export const agentDefinitionWriteResponseSchema = z
  .object({
    applied: z.boolean(),
    message: z.string(),
    summary: agentDefinitionSummarySchema.nullable().optional(),
    validation: agentDefinitionValidationReportSchema,
  })
  .strict()
export type AgentDefinitionWriteResponseDto = z.infer<
  typeof agentDefinitionWriteResponseSchema
>

export const agentDefinitionPreviewSummarySchema = z
  .object({
    definitionId: z.string().trim().min(1),
    version: z.number().int().positive(),
    displayName: z.string().trim().min(1),
    shortLabel: z.string().trim().min(1),
    description: z.string(),
    scope: z.string().trim().min(1),
    lifecycleState: z.string().trim().min(1),
    baseCapabilityProfile: z.string().trim().min(1),
    snapshot: z.unknown().nullable().optional(),
  })
  .strict()

export const agentDefinitionPreviewResponseSchema = z
  .object({
    schema: z.literal('xero.agent_definition_preview_command.v1'),
    projectId: z.string().trim().min(1),
    applied: z.literal(false),
    message: z.string().trim().min(1),
    definition: agentDefinitionPreviewSummarySchema.nullable().optional(),
    validation: agentDefinitionValidationReportSchema,
    effectiveRuntimePreview: agentEffectiveRuntimePreviewSchema,
    uiDeferred: z.literal(true),
  })
  .strict()
  .superRefine((response, ctx) => {
    const summary = response.definition
    if (!summary) {
      return
    }
    const previewDefinition = response.effectiveRuntimePreview.definition
    const matchingFields = [
      ['definitionId', summary.definitionId, previewDefinition.definitionId],
      ['version', summary.version, previewDefinition.version],
      ['displayName', summary.displayName, previewDefinition.displayName],
      ['scope', summary.scope, previewDefinition.scope],
      ['lifecycleState', summary.lifecycleState, previewDefinition.lifecycleState],
      ['baseCapabilityProfile', summary.baseCapabilityProfile, previewDefinition.baseCapabilityProfile],
    ] as const
    matchingFields.forEach(([field, summaryValue, previewValue]) => {
      if (summaryValue !== previewValue) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['effectiveRuntimePreview', 'definition', field],
          message: 'Preview response summary must match effective runtime preview definition.',
        })
      }
    })
  })
export type AgentDefinitionPreviewResponseDto = z.infer<
  typeof agentDefinitionPreviewResponseSchema
>

export function getAgentDefinitionScopeLabel(scope: AgentDefinitionScopeDto): string {
  switch (scope) {
    case 'built_in':
      return 'Built-in'
    case 'global_custom':
      return 'Global'
    case 'project_custom':
      return 'Project'
  }
}

export function getAgentDefinitionLifecycleLabel(
  state: AgentDefinitionLifecycleStateDto,
): string {
  switch (state) {
    case 'draft':
      return 'Draft'
    case 'valid':
      return 'Valid'
    case 'active':
      return 'Active'
    case 'archived':
      return 'Archived'
    case 'blocked':
      return 'Blocked'
  }
}

export function getAgentDefinitionBaseCapabilityLabel(
  profile: AgentDefinitionBaseCapabilityProfileDto,
): string {
  switch (profile) {
    case 'observe_only':
      return 'Observe-only'
    case 'planning':
      return 'Planning'
    case 'repository_recon':
      return 'Repository Recon'
    case 'engineering':
      return 'Engineering'
    case 'debugging':
      return 'Debugging'
    case 'agent_builder':
      return 'Agent Builder'
    case 'harness_test':
      return 'Harness Test'
  }
}
