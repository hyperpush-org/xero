import { z } from 'zod'
import { isoTimestampSchema, optionalIsoTimestampSchema } from './shared'
import { runtimeProviderIdSchema, runtimeSettingsSchema, type RuntimeSettingsDto } from './runtime'

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
    label: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
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
        message: 'Cadence could not resolve the active provider profile because `activeProfileId` did not match a stored profile.',
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
    label: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    openrouterApiKey: z.string().nullable().optional(),
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
    openrouterApiKeyConfigured: providerProfiles?.profiles.some(
      (profile) => profile.providerId === 'openrouter' && profile.readiness.ready,
    ) ?? false,
  })
}
