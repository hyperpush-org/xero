import { z } from 'zod'
import { optionalIsoTimestampSchema } from '@xero/ui/model/shared'

export const environmentProfileStatusSchema = z.enum([
  'pending',
  'probing',
  'ready',
  'partial',
  'failed',
])

export const environmentPermissionKindSchema = z.enum([
  'os_permission',
  'protected_path',
  'network_access',
  'installation_action',
])

export const environmentPermissionStatusSchema = z.enum([
  'pending',
  'granted',
  'denied',
  'skipped',
])

export const environmentDiagnosticSeveritySchema = z.enum(['info', 'warning', 'error'])
export const environmentToolCategorySchema = z.enum([
  'base_developer_tool',
  'package_manager',
  'platform_package_manager',
  'language_runtime',
  'container_orchestration',
  'mobile_tooling',
  'cloud_deployment',
  'database_cli',
  'solana_tooling',
  'agent_ai_cli',
  'editor',
  'build_tool',
  'linter',
  'version_manager',
  'iac_tool',
  'shell_utility',
])
export const environmentToolProbeStatusSchema = z.enum([
  'ok',
  'missing',
  'timeout',
  'failed',
  'skipped',
  'not_run',
])
export const environmentCapabilityStateSchema = z.enum([
  'ready',
  'partial',
  'missing',
  'blocked',
  'unknown',
])

export const environmentPermissionRequestSchema = z
  .object({
    id: z.string().trim().min(1),
    kind: environmentPermissionKindSchema,
    status: environmentPermissionStatusSchema,
    title: z.string().trim().min(1),
    reason: z.string().trim().min(1),
    optional: z.boolean(),
  })
  .strict()

export const environmentDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    severity: environmentDiagnosticSeveritySchema,
    message: z.string().trim().min(1),
    retryable: z.boolean(),
    toolId: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export const environmentDiscoveryStatusSchema = z
  .object({
    hasProfile: z.boolean(),
    status: environmentProfileStatusSchema,
    stale: z.boolean(),
    shouldStart: z.boolean(),
    refreshedAt: optionalIsoTimestampSchema,
    probeStartedAt: optionalIsoTimestampSchema,
    probeCompletedAt: optionalIsoTimestampSchema,
    permissionRequests: z.array(environmentPermissionRequestSchema),
    diagnostics: z.array(environmentDiagnosticSchema),
  })
  .strict()

export const environmentPermissionDecisionStatusSchema = z.enum(['granted', 'denied', 'skipped'])

export const environmentPermissionDecisionSchema = z
  .object({
    id: z.string().trim().min(1),
    status: environmentPermissionDecisionStatusSchema,
  })
  .strict()

export const resolveEnvironmentPermissionRequestsSchema = z
  .object({
    decisions: z.array(environmentPermissionDecisionSchema).min(1),
  })
  .strict()

export const environmentToolSummarySchema = z
  .object({
    id: z.string().trim().min(1),
    category: environmentToolCategorySchema,
    custom: z.boolean().default(false),
    present: z.boolean(),
    version: z.string().trim().min(1).nullable().optional(),
    displayPath: z.string().trim().min(1).nullable().optional(),
    probeStatus: environmentToolProbeStatusSchema,
  })
  .strict()

export const environmentCapabilitySchema = z
  .object({
    id: z.string().trim().min(1),
    state: environmentCapabilityStateSchema,
    evidence: z.array(z.string().trim().min(1)).default([]),
    message: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export const environmentPlatformSchema = z
  .object({
    osKind: z.string().trim().min(1),
    osVersion: z.string().trim().min(1).nullable().optional(),
    arch: z.string().trim().min(1),
    defaultShell: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export const environmentProfileSummaryValueSchema = z
  .object({
    schemaVersion: z.literal(1),
    status: environmentProfileStatusSchema,
    platform: environmentPlatformSchema,
    refreshedAt: optionalIsoTimestampSchema,
    tools: z.array(environmentToolSummarySchema).default([]),
    capabilities: z.array(environmentCapabilitySchema).default([]),
    permissionRequests: z.array(environmentPermissionRequestSchema).default([]),
    diagnostics: z.array(environmentDiagnosticSchema).default([]),
  })
  .strict()

export const environmentProfileSummarySchema = environmentProfileSummaryValueSchema.nullable()

const userToolCommandSchema = z
  .string()
  .trim()
  .min(1)
  .max(256)
  .refine((value) => !/[;&|`$<>*?!\r\n\0]/.test(value), {
    message: 'Command must not contain shell metacharacters.',
  })

export const verifyUserToolRequestSchema = z
  .object({
    id: z.string().trim().min(1).max(32).regex(/^[a-z0-9][a-z0-9_-]*$/),
    category: environmentToolCategorySchema,
    command: userToolCommandSchema,
    args: z.array(z.string().trim().min(1).max(128)).max(8).default([]),
  })
  .strict()

export const verifyUserToolResponseSchema = z
  .object({
    record: environmentToolSummarySchema,
    diagnostics: z.array(environmentDiagnosticSchema).default([]),
  })
  .strict()

export const saveUserToolRequestSchema = verifyUserToolRequestSchema

export const environmentProbeReportSchema = z
  .object({
    status: environmentProfileStatusSchema,
    summary: environmentProfileSummaryValueSchema,
    startedAt: optionalIsoTimestampSchema,
    completedAt: optionalIsoTimestampSchema,
  })
  .passthrough()

export type EnvironmentProfileStatusDto = z.infer<typeof environmentProfileStatusSchema>
export type EnvironmentPermissionKindDto = z.infer<typeof environmentPermissionKindSchema>
export type EnvironmentPermissionStatusDto = z.infer<typeof environmentPermissionStatusSchema>
export type EnvironmentPermissionDecisionStatusDto = z.infer<typeof environmentPermissionDecisionStatusSchema>
export type EnvironmentDiagnosticSeverityDto = z.infer<typeof environmentDiagnosticSeveritySchema>
export type EnvironmentToolCategoryDto = z.infer<typeof environmentToolCategorySchema>
export type EnvironmentToolProbeStatusDto = z.infer<typeof environmentToolProbeStatusSchema>
export type EnvironmentCapabilityStateDto = z.infer<typeof environmentCapabilityStateSchema>
export type EnvironmentPermissionRequestDto = z.infer<typeof environmentPermissionRequestSchema>
export type EnvironmentPermissionDecisionDto = z.infer<typeof environmentPermissionDecisionSchema>
export type ResolveEnvironmentPermissionRequestsDto = z.infer<typeof resolveEnvironmentPermissionRequestsSchema>
export type EnvironmentDiagnosticDto = z.infer<typeof environmentDiagnosticSchema>
export type EnvironmentDiscoveryStatusDto = z.infer<typeof environmentDiscoveryStatusSchema>
export type EnvironmentToolSummaryDto = z.infer<typeof environmentToolSummarySchema>
export type EnvironmentCapabilityDto = z.infer<typeof environmentCapabilitySchema>
export type EnvironmentPlatformDto = z.infer<typeof environmentPlatformSchema>
export type EnvironmentProfileSummaryValueDto = z.infer<typeof environmentProfileSummaryValueSchema>
export type EnvironmentProfileSummaryDto = z.infer<typeof environmentProfileSummarySchema>
export type VerifyUserToolRequestDto = z.infer<typeof verifyUserToolRequestSchema>
export type VerifyUserToolResponseDto = z.infer<typeof verifyUserToolResponseSchema>
export type SaveUserToolRequestDto = z.infer<typeof saveUserToolRequestSchema>
export type EnvironmentProbeReportDto = z.infer<typeof environmentProbeReportSchema>
