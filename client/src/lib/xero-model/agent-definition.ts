import { z } from 'zod'

import { isoTimestampSchema } from './shared'

export const agentDefinitionScopeSchema = z.enum([
  'built_in',
  'global_custom',
  'project_custom',
])
export type AgentDefinitionScopeDto = z.infer<typeof agentDefinitionScopeSchema>

export const agentDefinitionLifecycleStateSchema = z.enum(['draft', 'active', 'archived'])
export type AgentDefinitionLifecycleStateDto = z.infer<
  typeof agentDefinitionLifecycleStateSchema
>

export const agentDefinitionBaseCapabilityProfileSchema = z.enum([
  'observe_only',
  'engineering',
  'debugging',
  'agent_builder',
])
export type AgentDefinitionBaseCapabilityProfileDto = z.infer<
  typeof agentDefinitionBaseCapabilityProfileSchema
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
    case 'active':
      return 'Active'
    case 'archived':
      return 'Archived'
  }
}

export function getAgentDefinitionBaseCapabilityLabel(
  profile: AgentDefinitionBaseCapabilityProfileDto,
): string {
  switch (profile) {
    case 'observe_only':
      return 'Observe-only'
    case 'engineering':
      return 'Engineering'
    case 'debugging':
      return 'Debugging'
    case 'agent_builder':
      return 'Agent Builder'
  }
}
