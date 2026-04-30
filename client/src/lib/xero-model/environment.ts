import { z } from 'zod'
import { optionalIsoTimestampSchema } from './shared'

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

export const environmentToolSummarySchema = z
  .object({
    id: z.string().trim().min(1),
    category: environmentToolCategorySchema,
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

export const environmentProfileSummarySchema = z
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
  .nullable()

export type EnvironmentProfileStatusDto = z.infer<typeof environmentProfileStatusSchema>
export type EnvironmentPermissionKindDto = z.infer<typeof environmentPermissionKindSchema>
export type EnvironmentPermissionStatusDto = z.infer<typeof environmentPermissionStatusSchema>
export type EnvironmentDiagnosticSeverityDto = z.infer<typeof environmentDiagnosticSeveritySchema>
export type EnvironmentToolCategoryDto = z.infer<typeof environmentToolCategorySchema>
export type EnvironmentToolProbeStatusDto = z.infer<typeof environmentToolProbeStatusSchema>
export type EnvironmentCapabilityStateDto = z.infer<typeof environmentCapabilityStateSchema>
export type EnvironmentPermissionRequestDto = z.infer<typeof environmentPermissionRequestSchema>
export type EnvironmentDiagnosticDto = z.infer<typeof environmentDiagnosticSchema>
export type EnvironmentDiscoveryStatusDto = z.infer<typeof environmentDiscoveryStatusSchema>
export type EnvironmentToolSummaryDto = z.infer<typeof environmentToolSummarySchema>
export type EnvironmentCapabilityDto = z.infer<typeof environmentCapabilitySchema>
export type EnvironmentPlatformDto = z.infer<typeof environmentPlatformSchema>
export type EnvironmentProfileSummaryDto = z.infer<typeof environmentProfileSummarySchema>
