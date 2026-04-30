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

export type EnvironmentProfileStatusDto = z.infer<typeof environmentProfileStatusSchema>
export type EnvironmentPermissionKindDto = z.infer<typeof environmentPermissionKindSchema>
export type EnvironmentPermissionStatusDto = z.infer<typeof environmentPermissionStatusSchema>
export type EnvironmentDiagnosticSeverityDto = z.infer<typeof environmentDiagnosticSeveritySchema>
export type EnvironmentPermissionRequestDto = z.infer<typeof environmentPermissionRequestSchema>
export type EnvironmentDiagnosticDto = z.infer<typeof environmentDiagnosticSchema>
export type EnvironmentDiscoveryStatusDto = z.infer<typeof environmentDiscoveryStatusSchema>
