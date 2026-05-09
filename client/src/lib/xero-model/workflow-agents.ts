import { z } from 'zod'

import {
  agentDefinitionBaseCapabilityProfileSchema,
  agentDefinitionLifecycleStateSchema,
  agentDefinitionScopeSchema,
} from './agent-definition'
import { isoTimestampSchema } from './shared'
import { runtimeAgentIdSchema, runtimeRunApprovalModeSchema } from './runtime'

export const runtimeAgentPromptPolicySchema = z.enum([
  'ask',
  'plan',
  'engineer',
  'debug',
  'crawl',
  'agent_create',
  'harness_test',
])
export type RuntimeAgentPromptPolicyDto = z.infer<typeof runtimeAgentPromptPolicySchema>

export const runtimeAgentToolPolicySchema = z.enum([
  'observe_only',
  'planning',
  'repository_recon',
  'engineering',
  'agent_builder',
  'harness_test',
])
export type RuntimeAgentToolPolicyDto = z.infer<typeof runtimeAgentToolPolicySchema>

export const runtimeAgentOutputContractSchema = z.enum([
  'answer',
  'plan_pack',
  'crawl_report',
  'engineering_summary',
  'debug_summary',
  'agent_definition_draft',
  'harness_test_report',
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
    purpose: z.string(),
    triggers: z.array(agentTriggerRefSchema),
    columns: z.array(z.string()),
  })
  .strict()
export type AgentDbTouchpointDetailDto = z.infer<typeof agentDbTouchpointDetailSchema>

export const agentDbTouchpointsSchema = z
  .object({
    reads: z.array(agentDbTouchpointDetailSchema),
    writes: z.array(agentDbTouchpointDetailSchema),
    encouraged: z.array(agentDbTouchpointDetailSchema),
  })
  .strict()
export type AgentDbTouchpointsDto = z.infer<typeof agentDbTouchpointsSchema>

export const agentOutputSectionEmphasisSchema = z.enum(['core', 'standard', 'optional'])
export type AgentOutputSectionEmphasisDto = z.infer<typeof agentOutputSectionEmphasisSchema>

export const agentOutputSectionSchema = z
  .object({
    id: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string(),
    emphasis: agentOutputSectionEmphasisSchema,
    producedByTools: z.array(z.string()),
  })
  .strict()
export type AgentOutputSectionDto = z.infer<typeof agentOutputSectionSchema>

export const agentOutputContractSchema = z
  .object({
    contract: runtimeAgentOutputContractSchema,
    label: z.string(),
    description: z.string(),
    sections: z.array(agentOutputSectionSchema),
  })
  .strict()
export type AgentOutputContractDto = z.infer<typeof agentOutputContractSchema>

export const agentConsumedArtifactSchema = z
  .object({
    id: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: z.string(),
    sourceAgent: runtimeAgentIdSchema,
    contract: runtimeAgentOutputContractSchema,
    sections: z.array(z.string()),
    required: z.boolean(),
  })
  .strict()
export type AgentConsumedArtifactDto = z.infer<typeof agentConsumedArtifactSchema>

export const workflowAgentDetailSchema = z
  .object({
    ref: agentRefSchema,
    header: agentHeaderSchema,
    promptPolicy: runtimeAgentPromptPolicySchema.nullable().optional(),
    toolPolicy: runtimeAgentToolPolicySchema.nullable().optional(),
    prompts: z.array(agentPromptSchema),
    tools: z.array(agentToolSummarySchema),
    dbTouchpoints: agentDbTouchpointsSchema,
    output: agentOutputContractSchema,
    consumes: z.array(agentConsumedArtifactSchema),
  })
  .strict()
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

export const agentAuthoringCatalogSchema = z
  .object({
    tools: z.array(agentToolSummarySchema),
    toolCategories: z.array(agentAuthoringToolCategorySchema),
    dbTables: z.array(agentAuthoringDbTableSchema),
    upstreamArtifacts: z.array(agentAuthoringUpstreamArtifactSchema),
  })
  .strict()
export type AgentAuthoringCatalogDto = z.infer<typeof agentAuthoringCatalogSchema>

export const getAgentAuthoringCatalogRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
  })
  .strict()
export type GetAgentAuthoringCatalogRequestDto = z.infer<
  typeof getAgentAuthoringCatalogRequestSchema
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
