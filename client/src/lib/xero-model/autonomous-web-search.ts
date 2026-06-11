import { z } from 'zod'

export const autonomousWebSearchModeSchema = z.enum([
  'auto',
  'provider_managed_only',
  'configured_provider_only',
  'disabled',
])
export type AutonomousWebSearchModeDto = z.infer<typeof autonomousWebSearchModeSchema>

export const autonomousWebSearchProviderKindSchema = z.enum([
  'custom_endpoint',
  'brave_search',
  'tavily_search',
  'exa_search',
  'firecrawl_search',
  'you_search',
  'linkup_search',
  'kagi_search',
  'searxng_json',
  'serpapi_google',
  'searchapi_google',
  'google_cse',
])
export type AutonomousWebSearchProviderKindDto = z.infer<typeof autonomousWebSearchProviderKindSchema>

export const autonomousWebSearchProviderCheckSchema = z
  .object({
    status: z.string(),
    code: z.string(),
    message: z.string(),
    latencyMs: z.number().int().nonnegative(),
    sampleResultCount: z.number().int().nonnegative(),
    checkedAt: z.string(),
  })
  .strict()
export type AutonomousWebSearchProviderCheckDto = z.infer<
  typeof autonomousWebSearchProviderCheckSchema
>

export const autonomousWebSearchProviderReadinessSchema = z
  .object({
    ready: z.boolean(),
    status: z.string(),
    message: z.string(),
  })
  .strict()
export type AutonomousWebSearchProviderReadinessDto = z.infer<
  typeof autonomousWebSearchProviderReadinessSchema
>

export const autonomousWebSearchProviderProfileSchema = z
  .object({
    profileId: z.string(),
    kind: autonomousWebSearchProviderKindSchema,
    displayName: z.string(),
    enabled: z.boolean(),
    endpoint: z.string().nullable().optional(),
    baseUrl: z.string().nullable().optional(),
    googleCseCx: z.string().nullable().optional(),
    resultLimit: z.number().int().positive().nullable().optional(),
    timeoutMs: z.number().int().positive().nullable().optional(),
    region: z.string().nullable().optional(),
    language: z.string().nullable().optional(),
    freshness: z.string().nullable().optional(),
    safeSearch: z.boolean().nullable().optional(),
    hasApiKey: z.boolean(),
    apiKeyUpdatedAt: z.string().nullable().optional(),
    readiness: autonomousWebSearchProviderReadinessSchema,
    lastCheck: autonomousWebSearchProviderCheckSchema.nullable().optional(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })
  .strict()
export type AutonomousWebSearchProviderProfileDto = z.infer<
  typeof autonomousWebSearchProviderProfileSchema
>

export const autonomousWebSearchProviderKindMetadataSchema = z
  .object({
    kind: autonomousWebSearchProviderKindSchema,
    label: z.string(),
    requiresApiKey: z.boolean(),
    supportsLocale: z.boolean(),
    supportsFreshness: z.boolean(),
    supportsSafeSearch: z.boolean(),
    selfHosted: z.boolean(),
    requiresEndpoint: z.boolean(),
    requiresGoogleCseCx: z.boolean(),
  })
  .strict()
export type AutonomousWebSearchProviderKindMetadataDto = z.infer<
  typeof autonomousWebSearchProviderKindMetadataSchema
>

export const autonomousWebProviderManagedStatusSchema = z
  .object({
    modeAvailable: z.boolean(),
    status: z.string(),
    message: z.string(),
    supportedSources: z.array(z.string()),
  })
  .strict()
export type AutonomousWebProviderManagedStatusDto = z.infer<
  typeof autonomousWebProviderManagedStatusSchema
>

export const autonomousWebSearchSettingsSchema = z
  .object({
    mode: autonomousWebSearchModeSchema,
    activeProviderId: z.string().nullable().optional(),
    providers: z.array(autonomousWebSearchProviderProfileSchema),
    providerKinds: z.array(autonomousWebSearchProviderKindMetadataSchema),
    providerManaged: autonomousWebProviderManagedStatusSchema,
    updatedAt: z.string().nullable().optional(),
  })
  .strict()
export type AutonomousWebSearchSettingsDto = z.infer<typeof autonomousWebSearchSettingsSchema>

export const upsertAutonomousWebSearchSettingsRequestSchema = z
  .object({
    mode: autonomousWebSearchModeSchema,
  })
  .strict()
export type UpsertAutonomousWebSearchSettingsRequestDto = z.infer<
  typeof upsertAutonomousWebSearchSettingsRequestSchema
>

export const upsertAutonomousWebSearchProviderRequestSchema = z
  .object({
    profileId: z.string().nullable().optional(),
    kind: autonomousWebSearchProviderKindSchema,
    displayName: z.string().nullable().optional(),
    enabled: z.boolean().nullable().optional(),
    endpoint: z.string().nullable().optional(),
    baseUrl: z.string().nullable().optional(),
    apiKey: z.string().nullable().optional(),
    clearApiKey: z.boolean().nullable().optional(),
    googleCseCx: z.string().nullable().optional(),
    resultLimit: z.number().int().positive().nullable().optional(),
    timeoutMs: z.number().int().positive().nullable().optional(),
    region: z.string().nullable().optional(),
    language: z.string().nullable().optional(),
    freshness: z.string().nullable().optional(),
    safeSearch: z.boolean().nullable().optional(),
  })
  .strict()
export type UpsertAutonomousWebSearchProviderRequestDto = z.infer<
  typeof upsertAutonomousWebSearchProviderRequestSchema
>

export const deleteAutonomousWebSearchProviderRequestSchema = z
  .object({
    providerId: z.string().trim().min(1),
  })
  .strict()
export type DeleteAutonomousWebSearchProviderRequestDto = z.infer<
  typeof deleteAutonomousWebSearchProviderRequestSchema
>

export const setActiveAutonomousWebSearchProviderRequestSchema = z
  .object({
    providerId: z.string().trim().min(1),
  })
  .strict()
export type SetActiveAutonomousWebSearchProviderRequestDto = z.infer<
  typeof setActiveAutonomousWebSearchProviderRequestSchema
>

export const checkAutonomousWebSearchProviderRequestSchema = z
  .object({
    providerId: z.string().trim().min(1),
    query: z.string().nullable().optional(),
  })
  .strict()
export type CheckAutonomousWebSearchProviderRequestDto = z.infer<
  typeof checkAutonomousWebSearchProviderRequestSchema
>
