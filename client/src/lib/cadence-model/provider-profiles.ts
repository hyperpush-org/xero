import { z } from 'zod'
import { isoTimestampSchema, optionalIsoTimestampSchema } from './shared'
import { runtimeProviderIdSchema, runtimeSettingsSchema, type RuntimeSettingsDto } from './runtime'

const providerProfileRuntimeKindSchema = z.enum([
  'openai_codex',
  'openrouter',
  'anthropic',
  'openai_compatible',
  'gemini',
])

const providerProfilePresetIdSchema = z.enum([
  'openrouter',
  'anthropic',
  'github_models',
  'openai_api',
  'azure_openai',
  'gemini_ai_studio',
])

const optionalUrlSchema = z.string().url().nullable().optional()

function expectedRuntimeKindForProvider(providerId: z.infer<typeof runtimeProviderIdSchema>): z.infer<typeof providerProfileRuntimeKindSchema> {
  switch (providerId) {
    case 'openai_codex':
      return 'openai_codex'
    case 'openrouter':
      return 'openrouter'
    case 'anthropic':
      return 'anthropic'
    case 'github_models':
    case 'openai_api':
    case 'azure_openai':
      return 'openai_compatible'
    case 'gemini_ai_studio':
      return 'gemini'
  }
}

function validateRuntimeProviderModel(
  payload: { providerId: z.infer<typeof runtimeProviderIdSchema>; modelId: string },
  ctx: z.RefinementCtx,
): void {
  if (payload.providerId === 'openai_codex' && payload.modelId !== 'openai_codex') {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ['modelId'],
      message: 'Cadence only supports modelId `openai_codex` for provider `openai_codex`.',
    })
  }
}

function validateCloudProfileContract(
  payload: {
    providerId: z.infer<typeof runtimeProviderIdSchema>
    runtimeKind: z.infer<typeof providerProfileRuntimeKindSchema>
    presetId?: z.infer<typeof providerProfilePresetIdSchema> | null
    baseUrl?: string | null
    apiVersion?: string | null
  },
  ctx: z.RefinementCtx,
): void {
  const expectedRuntimeKind = expectedRuntimeKindForProvider(payload.providerId)
  if (payload.runtimeKind !== expectedRuntimeKind) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ['runtimeKind'],
      message: `Cadence requires runtimeKind \`${expectedRuntimeKind}\` for provider \`${payload.providerId}\`.`,
    })
  }

  const hasPresetId = typeof payload.presetId === 'string' && payload.presetId.trim().length > 0
  const hasBaseUrl = typeof payload.baseUrl === 'string' && payload.baseUrl.trim().length > 0
  const hasApiVersion = typeof payload.apiVersion === 'string' && payload.apiVersion.trim().length > 0

  switch (payload.providerId) {
    case 'openai_codex':
      if (hasPresetId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['presetId'],
          message: 'Cadence OpenAI Codex profiles do not accept `presetId` metadata.',
        })
      }
      if (hasBaseUrl) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['baseUrl'],
          message: 'Cadence OpenAI Codex profiles do not accept `baseUrl` metadata.',
        })
      }
      if (hasApiVersion) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['apiVersion'],
          message: 'Cadence OpenAI Codex profiles do not accept `apiVersion` metadata.',
        })
      }
      return

    case 'openrouter':
    case 'anthropic':
    case 'github_models':
    case 'gemini_ai_studio': {
      if (payload.presetId !== payload.providerId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['presetId'],
          message: `Cadence requires presetId \`${payload.providerId}\` for provider \`${payload.providerId}\`.`,
        })
      }
      if (hasBaseUrl) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['baseUrl'],
          message: `Cadence does not accept custom baseUrl overrides for provider \`${payload.providerId}\`.`,
        })
      }
      if (hasApiVersion) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['apiVersion'],
          message: `Cadence does not accept apiVersion metadata for provider \`${payload.providerId}\`.`,
        })
      }
      return
    }

    case 'openai_api':
      if (!hasBaseUrl && payload.presetId !== 'openai_api') {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['presetId'],
          message:
            'Cadence requires presetId `openai_api` for the default OpenAI API endpoint, or a custom baseUrl for a custom OpenAI-compatible endpoint.',
        })
      }
      if (hasBaseUrl && hasPresetId && payload.presetId !== 'openai_api') {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['presetId'],
          message:
            'Cadence only accepts presetId `openai_api` when saving a custom OpenAI-compatible baseUrl for provider `openai_api`.',
        })
      }
      if (!hasBaseUrl && hasApiVersion) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['apiVersion'],
          message: 'Cadence only accepts apiVersion metadata for custom OpenAI-compatible endpoints.',
        })
      }
      return

    case 'azure_openai':
      if (payload.presetId !== 'azure_openai') {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['presetId'],
          message: 'Cadence requires presetId `azure_openai` for Azure OpenAI profiles.',
        })
      }
      if (!hasBaseUrl) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['baseUrl'],
          message: 'Cadence requires baseUrl metadata for Azure OpenAI profiles.',
        })
      }
      if (!hasApiVersion) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['apiVersion'],
          message: 'Cadence requires apiVersion metadata for Azure OpenAI profiles.',
        })
      }
      return
  }
}

export const providerProfileReadinessStatusSchema = z.enum(['ready', 'missing', 'malformed'])

export const providerProfileReadinessSchema = z
  .object({
    ready: z.boolean(),
    status: providerProfileReadinessStatusSchema,
    credentialUpdatedAt: optionalIsoTimestampSchema,
  })
  .strict()
  .superRefine((readiness, ctx) => {
    if (readiness.status === 'ready') {
      if (!readiness.ready) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['ready'],
          message: 'Provider-profile readiness rows with `status=ready` must set `ready=true`.',
        })
      }

      if (!readiness.credentialUpdatedAt) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['credentialUpdatedAt'],
          message: 'Ready provider-profile rows must include `credentialUpdatedAt`.',
        })
      }

      return
    }

    if (readiness.ready) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['ready'],
        message: 'Non-ready provider-profile rows must set `ready=false`.',
      })
    }

    if (readiness.status === 'missing' && readiness.credentialUpdatedAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['credentialUpdatedAt'],
        message: 'Missing provider-profile readiness rows must not include `credentialUpdatedAt`.',
      })
    }

    if (readiness.status === 'malformed' && !readiness.credentialUpdatedAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['credentialUpdatedAt'],
        message: 'Malformed provider-profile readiness rows must include `credentialUpdatedAt`.',
      })
    }
  })

export const providerProfileSchema = z
  .object({
    profileId: z.string().trim().min(1),
    providerId: runtimeProviderIdSchema,
    runtimeKind: providerProfileRuntimeKindSchema,
    label: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    presetId: providerProfilePresetIdSchema.nullable().optional(),
    baseUrl: optionalUrlSchema,
    apiVersion: z.string().trim().min(1).nullable().optional(),
    active: z.boolean(),
    readiness: providerProfileReadinessSchema,
    migratedFromLegacy: z.boolean(),
    migratedAt: optionalIsoTimestampSchema,
  })
  .strict()
  .superRefine((profile, ctx) => {
    validateRuntimeProviderModel(
      {
        providerId: profile.providerId,
        modelId: profile.modelId,
      },
      ctx,
    )

    validateCloudProfileContract(
      {
        providerId: profile.providerId,
        runtimeKind: profile.runtimeKind,
        presetId: profile.presetId,
        baseUrl: profile.baseUrl,
        apiVersion: profile.apiVersion,
      },
      ctx,
    )

    if (profile.migratedFromLegacy && !profile.migratedAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['migratedAt'],
        message: 'Legacy-migrated provider profiles must include `migratedAt`.',
      })
    }

    if (!profile.migratedFromLegacy && profile.migratedAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['migratedAt'],
        message: 'Non-migrated provider profiles must not include `migratedAt`.',
      })
    }
  })

export const providerProfilesMigrationSchema = z
  .object({
    source: z.string().trim().min(1),
    migratedAt: isoTimestampSchema,
    runtimeSettingsUpdatedAt: optionalIsoTimestampSchema,
    openrouterCredentialsUpdatedAt: optionalIsoTimestampSchema,
    openaiAuthUpdatedAt: optionalIsoTimestampSchema,
    openrouterModelInferred: z.boolean().nullable().optional(),
  })
  .strict()

export const providerProfilesSchema = z
  .object({
    activeProfileId: z.string().trim().min(1),
    profiles: z.array(providerProfileSchema),
    migration: providerProfilesMigrationSchema.nullable().optional(),
  })
  .strict()
  .superRefine((payload, ctx) => {
    const profileIds = new Set<string>()
    let activeFlagCount = 0

    for (const [index, profile] of payload.profiles.entries()) {
      if (profileIds.has(profile.profileId)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['profiles', index, 'profileId'],
          message: `Provider profile id \`${profile.profileId}\` must be unique.`,
        })
      }
      profileIds.add(profile.profileId)

      if (profile.active) {
        activeFlagCount += 1
      }
    }

    if (!profileIds.has(payload.activeProfileId)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['activeProfileId'],
        message:
          'Cadence could not resolve the active provider profile because `activeProfileId` did not match a stored profile.',
      })
    }

    if (activeFlagCount !== 1) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['profiles'],
        message: 'Provider-profile payloads must mark exactly one profile as active.',
      })
    }

    const activeProfile = payload.profiles.find((profile) => profile.active)
    if (activeProfile && activeProfile.profileId !== payload.activeProfileId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['activeProfileId'],
        message: 'Cadence received inconsistent active provider-profile metadata.',
      })
    }
  })

export const upsertProviderProfileRequestSchema = z
  .object({
    profileId: z.string().trim().min(1),
    providerId: runtimeProviderIdSchema,
    runtimeKind: providerProfileRuntimeKindSchema,
    label: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    presetId: providerProfilePresetIdSchema.nullable().optional(),
    baseUrl: optionalUrlSchema,
    apiVersion: z.string().trim().min(1).nullable().optional(),
    apiKey: z.string().nullable().optional(),
    activate: z.boolean().optional(),
  })
  .strict()
  .superRefine((payload, ctx) => {
    validateRuntimeProviderModel(
      {
        providerId: payload.providerId,
        modelId: payload.modelId,
      },
      ctx,
    )

    validateCloudProfileContract(
      {
        providerId: payload.providerId,
        runtimeKind: payload.runtimeKind,
        presetId: payload.presetId,
        baseUrl: payload.baseUrl,
        apiVersion: payload.apiVersion,
      },
      ctx,
    )

    if (payload.providerId === 'openai_codex' && typeof payload.apiKey === 'string' && payload.apiKey.trim().length > 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['apiKey'],
        message: 'Cadence OpenAI Codex profiles use OAuth and do not accept `apiKey` payloads.',
      })
    }
  })

export const setActiveProviderProfileRequestSchema = z
  .object({
    profileId: z.string().trim().min(1),
  })
  .strict()

export type ProviderProfileReadinessStatusDto = z.infer<typeof providerProfileReadinessStatusSchema>
export type ProviderProfileReadinessDto = z.infer<typeof providerProfileReadinessSchema>
export type ProviderProfileDto = z.infer<typeof providerProfileSchema>
export type ProviderProfilesMigrationDto = z.infer<typeof providerProfilesMigrationSchema>
export type ProviderProfilesDto = z.infer<typeof providerProfilesSchema>
export type UpsertProviderProfileRequestDto = z.infer<typeof upsertProviderProfileRequestSchema>
export type SetActiveProviderProfileRequestDto = z.infer<typeof setActiveProviderProfileRequestSchema>

export function getActiveProviderProfile(
  providerProfiles: ProviderProfilesDto | null | undefined,
): ProviderProfileDto | null {
  if (!providerProfiles || !Array.isArray(providerProfiles.profiles)) {
    return null
  }

  return (
    providerProfiles.profiles.find((profile) => profile.profileId === providerProfiles.activeProfileId) ?? null
  )
}

function hasAnyReadyProfile(
  providerProfiles: ProviderProfilesDto | null | undefined,
  providerId: RuntimeSettingsDto['providerId'],
): boolean {
  return providerProfiles?.profiles.some((profile) => profile.providerId === providerId && profile.readiness.ready) ?? false
}

export function projectRuntimeSettingsFromProviderProfiles(
  providerProfiles: ProviderProfilesDto | null | undefined,
): RuntimeSettingsDto | null {
  const activeProfile = getActiveProviderProfile(providerProfiles)
  if (!activeProfile) {
    return null
  }

  return runtimeSettingsSchema.parse({
    providerId: activeProfile.providerId,
    modelId: activeProfile.modelId,
    openrouterApiKeyConfigured: hasAnyReadyProfile(providerProfiles, 'openrouter'),
    anthropicApiKeyConfigured: hasAnyReadyProfile(providerProfiles, 'anthropic'),
  })
}
