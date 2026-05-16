import { z } from 'zod'

export const developerToolPackSummarySchema = z
  .object({
    packId: z.string().trim().min(1),
    label: z.string(),
    policyProfile: z.string(),
  })
  .strict()
export type DeveloperToolPackSummaryDto = z.infer<typeof developerToolPackSummarySchema>

export const developerToolCatalogEntrySchema = z
  .object({
    toolName: z.string().trim().min(1),
    group: z.string(),
    description: z.string(),
    tags: z.array(z.string()),
    schemaFields: z.array(z.string()),
    examples: z.array(z.string()),
    riskClass: z.string(),
    effectClass: z.string(),
    runtimeAvailable: z.boolean(),
    allowedRuntimeAgents: z.array(z.string()),
    activationGroups: z.array(z.string()),
    toolPacks: z.array(developerToolPackSummarySchema),
    inputSchema: z.unknown().nullable().optional(),
    runtimeUnavailableReason: z.string().nullable().optional(),
  })
  .strict()
export type DeveloperToolCatalogEntryDto = z.infer<typeof developerToolCatalogEntrySchema>

export const developerToolCatalogResponseSchema = z
  .object({
    hostOs: z.string(),
    hostOsLabel: z.string(),
    skillToolEnabled: z.boolean(),
    entries: z.array(developerToolCatalogEntrySchema),
  })
  .strict()
export type DeveloperToolCatalogResponseDto = z.infer<typeof developerToolCatalogResponseSchema>

export const developerToolCatalogRequestSchema = z
  .object({
    skillToolEnabled: z.boolean().optional(),
  })
  .strict()
export type DeveloperToolCatalogRequestDto = z.infer<typeof developerToolCatalogRequestSchema>

export const developerToolHarnessCallSchema = z
  .object({
    toolName: z.string().trim().min(1),
    input: z.unknown(),
    toolCallId: z.string().min(1).optional(),
  })
  .strict()
export type DeveloperToolHarnessCallDto = z.infer<typeof developerToolHarnessCallSchema>

export const developerToolHarnessRunOptionsSchema = z
  .object({
    stopOnFailure: z.boolean().optional(),
    approveWrites: z.boolean().optional(),
    operatorApproveAll: z.boolean().optional(),
  })
  .strict()
export type DeveloperToolHarnessRunOptionsDto = z.infer<
  typeof developerToolHarnessRunOptionsSchema
>

export const developerToolSyntheticRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().min(1).optional(),
    calls: z.array(developerToolHarnessCallSchema).min(1),
    options: developerToolHarnessRunOptionsSchema.optional(),
  })
  .strict()
export type DeveloperToolSyntheticRunRequestDto = z.infer<
  typeof developerToolSyntheticRunRequestSchema
>

export const developerToolHarnessCallResultSchema = z
  .object({
    toolCallId: z.string(),
    toolName: z.string(),
    ok: z.boolean(),
    summary: z.string(),
    output: z.unknown(),
  })
  .strict()
export type DeveloperToolHarnessCallResultDto = z.infer<
  typeof developerToolHarnessCallResultSchema
>

export const developerToolSyntheticRunResponseSchema = z
  .object({
    runId: z.string(),
    agentSessionId: z.string(),
    stoppedEarly: z.boolean(),
    hadFailure: z.boolean(),
    results: z.array(developerToolHarnessCallResultSchema),
  })
  .strict()
export type DeveloperToolSyntheticRunResponseDto = z.infer<
  typeof developerToolSyntheticRunResponseSchema
>

export const developerToolDryRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    toolName: z.string().trim().min(1),
    input: z.unknown(),
    toolCallId: z.string().min(1).optional(),
    operatorApproved: z.boolean().optional(),
  })
  .strict()
export type DeveloperToolDryRunRequestDto = z.infer<typeof developerToolDryRunRequestSchema>

export const developerToolPolicyDecisionSchema = z
  .object({
    action: z.string(),
    code: z.string(),
    explanation: z.string(),
    riskClass: z.string(),
    projectTrust: z.string(),
    networkIntent: z.string(),
    credentialSensitivity: z.string(),
    priorObservationRequired: z.boolean(),
    osTarget: z.string().nullable().optional(),
  })
  .strict()
export type DeveloperToolPolicyDecisionDto = z.infer<typeof developerToolPolicyDecisionSchema>

export const developerToolDryRunResponseSchema = z
  .object({
    toolCallId: z.string(),
    toolName: z.string(),
    decoded: z.boolean(),
    policyDecision: developerToolPolicyDecisionSchema,
    sandboxDecision: z.unknown(),
    sandboxDenied: z.boolean(),
  })
  .strict()
export type DeveloperToolDryRunResponseDto = z.infer<typeof developerToolDryRunResponseSchema>

export const developerToolSequenceRecordSchema = z
  .object({
    id: z.string().min(1),
    name: z.string().min(1),
    calls: z.array(developerToolHarnessCallSchema),
    options: developerToolHarnessRunOptionsSchema.nullable().optional(),
    createdAt: z.string().min(1),
    updatedAt: z.string().min(1),
  })
  .strict()
export type DeveloperToolSequenceRecordDto = z.infer<typeof developerToolSequenceRecordSchema>

export const developerToolSequenceListResponseSchema = z
  .object({
    sequences: z.array(developerToolSequenceRecordSchema),
  })
  .strict()
export type DeveloperToolSequenceListResponseDto = z.infer<
  typeof developerToolSequenceListResponseSchema
>

export const developerToolSequenceUpsertRequestSchema = z
  .object({
    id: z.string().min(1).optional(),
    name: z.string().trim().min(1),
    calls: z.array(developerToolHarnessCallSchema).min(1),
    options: developerToolHarnessRunOptionsSchema.optional(),
  })
  .strict()
export type DeveloperToolSequenceUpsertRequestDto = z.infer<
  typeof developerToolSequenceUpsertRequestSchema
>

export const developerToolSequenceDeleteRequestSchema = z
  .object({
    id: z.string().min(1),
  })
  .strict()
export type DeveloperToolSequenceDeleteRequestDto = z.infer<
  typeof developerToolSequenceDeleteRequestSchema
>

export const developerToolModelRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    toolName: z.string().trim().min(1),
    prompt: z.string().trim().min(1),
    agentSessionId: z.string().min(1).optional(),
    runtimeAgentId: z.string().min(1).optional(),
  })
  .strict()
export type DeveloperToolModelRunRequestDto = z.infer<typeof developerToolModelRunRequestSchema>

export const developerToolModelRunResponseSchema = z
  .object({
    runId: z.string(),
    agentSessionId: z.string(),
    projectId: z.string(),
  })
  .strict()
export type DeveloperToolModelRunResponseDto = z.infer<typeof developerToolModelRunResponseSchema>

export const developerToolHarnessProjectSchema = z
  .object({
    projectId: z.string().trim().min(1),
    displayName: z.string(),
    rootPath: z.string(),
  })
  .strict()
export type DeveloperToolHarnessProjectDto = z.infer<
  typeof developerToolHarnessProjectSchema
>
