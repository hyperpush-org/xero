import { z } from 'zod'
import { optionalIsoTimestampSchema } from './shared'
import { runtimeProviderIdSchema } from './runtime'
import {
  sessionContextLimitConfidenceSchema,
  sessionContextLimitSourceSchema,
} from './session-context'

export const providerModelCatalogSourceSchema = z.enum(['live', 'cache', 'manual', 'unavailable'])
export const providerModelThinkingEffortSchema = z.enum(['minimal', 'low', 'medium', 'high', 'x_high'])

export const providerModelCatalogDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    retryable: z.boolean(),
  })
  .strict()

export const providerModelThinkingCapabilitySchema = z
  .object({
    supported: z.boolean(),
    effortOptions: z.array(providerModelThinkingEffortSchema),
    defaultEffort: providerModelThinkingEffortSchema.nullable().optional(),
  })
  .strict()
  .superRefine((capability, ctx) => {
    const uniqueEffortOptions = new Set(capability.effortOptions)
    if (uniqueEffortOptions.size !== capability.effortOptions.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['effortOptions'],
        message: 'Provider-model thinking effort options must be unique.',
      })
    }

    if (!capability.supported) {
      if (capability.effortOptions.length > 0) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['effortOptions'],
          message: 'Unsupported provider-model thinking capability must not expose effort options.',
        })
      }

      if (capability.defaultEffort) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['defaultEffort'],
          message: 'Unsupported provider-model thinking capability must not expose a default effort.',
        })
      }

      return
    }

    if (capability.effortOptions.length === 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['effortOptions'],
        message: 'Supported provider-model thinking capability must expose at least one effort option.',
      })
    }

    if (capability.defaultEffort && !uniqueEffortOptions.has(capability.defaultEffort)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['defaultEffort'],
        message: 'Provider-model thinking default effort must be included in `effortOptions`.',
      })
    }
  })

export const providerModelSchema = z
  .object({
    modelId: z.string().trim().min(1),
    displayName: z.string().trim().min(1),
    thinking: providerModelThinkingCapabilitySchema,
    contextWindowTokens: z.number().int().positive().nullable().optional(),
    maxOutputTokens: z.number().int().positive().nullable().optional(),
    contextLimitSource: sessionContextLimitSourceSchema.nullable().optional(),
    contextLimitConfidence: sessionContextLimitConfidenceSchema.nullable().optional(),
    contextLimitFetchedAt: optionalIsoTimestampSchema,
  })
  .strict()

function validateRuntimeProviderModel(
  payload: { providerId: z.infer<typeof runtimeProviderIdSchema>; modelId: string },
  ctx: z.RefinementCtx,
): void {
  if (payload.providerId === 'openai_codex' && payload.modelId.trim().length === 0) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ['modelId'],
      message: 'Xero requires a modelId for provider `openai_codex`.',
    })
  }
}

export const providerModelCatalogSchema = z
  .object({
    profileId: z.string().trim().min(1),
    providerId: runtimeProviderIdSchema,
    configuredModelId: z.string().trim().min(1),
    source: providerModelCatalogSourceSchema,
    fetchedAt: optionalIsoTimestampSchema,
    lastSuccessAt: optionalIsoTimestampSchema,
    lastRefreshError: providerModelCatalogDiagnosticSchema.nullable().optional(),
    models: z.array(providerModelSchema),
  })
  .strict()
  .superRefine((catalog, ctx) => {
    validateRuntimeProviderModel(
      {
        providerId: catalog.providerId,
        modelId: catalog.configuredModelId,
      },
      ctx,
    )

    const modelIds = new Set<string>()
    for (const [index, model] of catalog.models.entries()) {
      if (modelIds.has(model.modelId)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['models', index, 'modelId'],
          message: `Provider-model catalog rows must not duplicate model id \`${model.modelId}\`.`,
        })
      }
      modelIds.add(model.modelId)

      validateRuntimeProviderModel(
        {
          providerId: catalog.providerId,
          modelId: model.modelId,
        },
        {
          addIssue: (issue) => {
            ctx.addIssue({
              ...issue,
              path: ['models', index, ...(issue.path ?? [])],
            })
          },
          path: ctx.path,
        },
      )
    }

    if (catalog.source === 'unavailable') {
      if (catalog.fetchedAt) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['fetchedAt'],
          message: 'Unavailable provider-model catalogs must not expose `fetchedAt`.',
        })
      }

      if (catalog.lastSuccessAt) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['lastSuccessAt'],
          message: 'Unavailable provider-model catalogs must not expose `lastSuccessAt`.',
        })
      }

      if (catalog.models.length > 0) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['models'],
          message: 'Unavailable provider-model catalogs must not expose discovered models.',
        })
      }

      return
    }

    if (!catalog.fetchedAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['fetchedAt'],
        message: 'Live or cached provider-model catalogs must expose `fetchedAt`.',
      })
    }

    if (!catalog.lastSuccessAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['lastSuccessAt'],
        message: 'Live or cached provider-model catalogs must expose `lastSuccessAt`.',
      })
    }

    const parsedFetchedAt = Date.parse(catalog.fetchedAt ?? '')
    const parsedLastSuccessAt = Date.parse(catalog.lastSuccessAt ?? '')
    if (Number.isFinite(parsedFetchedAt) && Number.isFinite(parsedLastSuccessAt) && parsedLastSuccessAt > parsedFetchedAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['lastSuccessAt'],
        message: 'Provider-model `lastSuccessAt` must not be newer than `fetchedAt`.',
      })
    }
  })

export const getProviderModelCatalogRequestSchema = z
  .object({
    profileId: z.string().trim().min(1),
    forceRefresh: z.boolean().optional(),
  })
  .strict()

export type ProviderModelCatalogSourceDto = z.infer<typeof providerModelCatalogSourceSchema>
export type ProviderModelThinkingEffortDto = z.infer<typeof providerModelThinkingEffortSchema>
export type ProviderModelCatalogDiagnosticDto = z.infer<typeof providerModelCatalogDiagnosticSchema>
export type ProviderModelThinkingCapabilityDto = z.infer<typeof providerModelThinkingCapabilitySchema>
export type ProviderModelDto = z.infer<typeof providerModelSchema>
export type ProviderModelCatalogDto = z.infer<typeof providerModelCatalogSchema>
export type GetProviderModelCatalogRequestDto = z.infer<typeof getProviderModelCatalogRequestSchema>

export function getProviderModelCatalogConfiguredModel(
  catalog: ProviderModelCatalogDto | null | undefined,
): ProviderModelDto | null {
  if (!catalog) {
    return null
  }

  return catalog.models.find((model) => model.modelId === catalog.configuredModelId) ?? null
}

export function getProviderModelById(
  catalog: ProviderModelCatalogDto | null | undefined,
  modelId: string | null | undefined,
): ProviderModelDto | null {
  const trimmedModelId = modelId?.trim() ?? ''
  if (!catalog || trimmedModelId.length === 0) {
    return null
  }

  return catalog.models.find((model) => model.modelId === trimmedModelId) ?? null
}

export function getProviderModelThinkingEffortLabel(
  effort: ProviderModelThinkingEffortDto | null | undefined,
): string {
  switch (effort) {
    case 'minimal':
      return 'Minimal'
    case 'low':
      return 'Low'
    case 'medium':
      return 'Medium'
    case 'high':
      return 'High'
    case 'x_high':
      return 'Very high'
    default:
      return 'Thinking'
  }
}

export function getProviderModelCatalogFetchedAt(
  catalog: ProviderModelCatalogDto | null | undefined,
): string | null {
  if (!catalog) {
    return null
  }

  return catalog.fetchedAt ?? catalog.lastSuccessAt ?? null
}

export function hasProviderModelCatalogSnapshot(catalog: ProviderModelCatalogDto | null | undefined): boolean {
  if (!catalog) {
    return false
  }

  return catalog.source !== 'unavailable' && catalog.models.length > 0
}

export function createUnavailableProviderModelCatalog(
  input: Pick<ProviderModelCatalogDto, 'profileId' | 'providerId' | 'configuredModelId'> & {
    lastRefreshError?: ProviderModelCatalogDiagnosticDto | null
  },
): ProviderModelCatalogDto {
  return providerModelCatalogSchema.parse({
    profileId: input.profileId,
    providerId: input.providerId,
    configuredModelId: input.configuredModelId,
    source: 'unavailable',
    fetchedAt: null,
    lastSuccessAt: null,
    lastRefreshError: input.lastRefreshError ?? null,
    models: [],
  })
}

export function createProviderModelCatalogRequest(
  profileId: string,
  options: { forceRefresh?: boolean } = {},
): GetProviderModelCatalogRequestDto {
  return getProviderModelCatalogRequestSchema.parse({
    profileId,
    forceRefresh: options.forceRefresh ?? false,
  })
}
