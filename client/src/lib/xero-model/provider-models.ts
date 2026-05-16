import { z } from 'zod'
import { estimateUtf16Bytes } from '@/lib/byte-budget-cache'
import { optionalIsoTimestampSchema } from '@xero/ui/model/shared'
import { runtimeProviderIdSchema } from '@xero/ui/model/runtime'
import {
  sessionContextLimitConfidenceSchema,
  sessionContextLimitSourceSchema,
} from './session-context'

export const providerModelCatalogSourceSchema = z.enum(['live', 'cache', 'manual', 'unavailable'])
export const providerModelThinkingEffortSchema = z.enum(['minimal', 'low', 'medium', 'high', 'x_high'])
export const providerCapabilityStatusSchema = z.enum([
  'supported',
  'probed',
  'unknown',
  'unavailable',
  'not_applicable',
])

export const providerModelCatalogDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    retryable: z.boolean(),
  })
  .strict()

export const providerModelCatalogContractDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    severity: z.enum(['info', 'warning', 'error']),
    path: z.array(z.string()),
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

export const providerFeatureCapabilitySchema = z
  .object({
    status: providerCapabilityStatusSchema,
    source: z.string().trim().min(1),
    detail: z.string().trim().min(1),
  })
  .strict()

export const providerToolCallCapabilitySchema = z
  .object({
    status: providerCapabilityStatusSchema,
    source: z.string().trim().min(1),
    strictnessBehavior: z.string().trim().min(1),
    schemaDialect: z.string().trim().min(1),
    parallelCallBehavior: z.string().trim().min(1),
    knownIncompatibilities: z.array(z.string().trim().min(1)),
  })
  .strict()

export const providerReasoningCapabilitySchema = z
  .object({
    status: providerCapabilityStatusSchema,
    source: z.string().trim().min(1),
    effortLevels: z.array(providerModelThinkingEffortSchema),
    defaultEffort: providerModelThinkingEffortSchema.nullable().optional(),
    summarySupport: z.string().trim().min(1),
    clamping: z.string().trim().min(1),
    unsupportedModelFallback: z.string().trim().min(1),
  })
  .strict()

export const providerAttachmentCapabilitySchema = z
  .object({
    status: providerCapabilityStatusSchema,
    source: z.string().trim().min(1),
    imageInput: z.string().trim().min(1),
    documentInput: z.string().trim().min(1),
    supportedTypes: z.array(z.string().trim().min(1)),
    limits: z.array(z.string().trim().min(1)),
  })
  .strict()

export const providerContextLimitCapabilitySchema = z
  .object({
    status: providerCapabilityStatusSchema,
    source: z.string().trim().min(1),
    confidence: z.string().trim().min(1),
    contextWindowTokens: z.number().int().positive().nullable().optional(),
    maxOutputTokens: z.number().int().positive().nullable().optional(),
  })
  .strict()

export const providerCapabilityFeatureSetSchema = z
  .object({
    streaming: providerFeatureCapabilitySchema,
    toolCalls: providerToolCallCapabilitySchema,
    reasoning: providerReasoningCapabilitySchema,
    attachments: providerAttachmentCapabilitySchema,
    contextLimits: providerContextLimitCapabilitySchema,
    costHints: providerFeatureCapabilitySchema,
  })
  .strict()

export const providerCatalogCacheMetadataSchema = z
  .object({
    source: z.string().trim().min(1),
    fetchedAt: optionalIsoTimestampSchema,
    lastSuccessAt: optionalIsoTimestampSchema,
    ageSeconds: z.number().int().nullable().optional(),
    ttlSeconds: z.number().int().positive(),
    stale: z.boolean(),
  })
  .strict()

export const providerRedactedRequestPreviewSchema = z
  .object({
    route: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    enabledFeatures: z.array(z.string().trim().min(1)),
    toolSchemaNames: z.array(z.string().trim().min(1)),
    headers: z.array(z.string().trim().min(1)),
    metadata: z.array(z.string().trim().min(1)),
  })
  .strict()

export const providerCapabilityCatalogSchema = z
  .object({
    contractVersion: z.literal(1),
    providerId: z.string().trim().min(1),
    providerLabel: z.string().trim().min(1),
    defaultModelId: z.string().trim().min(1),
    runtimeFamily: z.string().trim().min(1),
    runtimeKind: z.string().trim().min(1),
    authMethod: z.string().trim().min(1),
    credentialProof: z.string().trim().min(1).nullable().optional(),
    transportMode: z.string().trim().min(1),
    endpointShape: z.string().trim().min(1),
    catalogKind: z.string().trim().min(1),
    modelListStrategy: z.string().trim().min(1),
    externalAgentAdapter: z.boolean(),
    cache: providerCatalogCacheMetadataSchema,
    requestPreview: providerRedactedRequestPreviewSchema,
    capabilities: providerCapabilityFeatureSetSchema,
    knownLimitations: z.array(z.string().trim().min(1)),
    remediations: z.array(z.string().trim().min(1)),
  })
  .strict()

export const providerPreflightStatusSchema = z.enum(['passed', 'warning', 'failed', 'skipped'])
export const providerPreflightSourceSchema = z.enum([
  'live_probe',
  'live_catalog',
  'cached_probe',
  'static_manual',
  'unavailable',
])

export const providerPreflightRequiredFeaturesSchema = z
  .object({
    streaming: z.boolean().default(false),
    toolCalls: z.boolean().default(false),
    reasoningControls: z.boolean().default(false),
    attachments: z.boolean().default(false),
  })
  .strict()

export const providerPreflightCheckSchema = z
  .object({
    checkId: z.string().trim().min(1),
    status: providerPreflightStatusSchema,
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    source: providerPreflightSourceSchema,
    retryable: z.boolean(),
  })
  .strict()

export const providerPreflightSnapshotSchema = z
  .object({
    contractVersion: z.literal(1),
    profileId: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    source: providerPreflightSourceSchema,
    checkedAt: z.string().trim().min(1),
    ageSeconds: z.number().int().nullable().optional(),
    ttlSeconds: z.number().int().positive(),
    stale: z.boolean(),
    requiredFeatures: providerPreflightRequiredFeaturesSchema,
    capabilities: providerCapabilityCatalogSchema,
    checks: z.array(providerPreflightCheckSchema),
    status: providerPreflightStatusSchema,
  })
  .strict()
  .superRefine((snapshot, ctx) => {
    for (const [index, check] of snapshot.checks.entries()) {
      if (check.source !== snapshot.source) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['checks', index, 'source'],
          message: 'Provider preflight checks must use the same source as the snapshot.',
        })
      }
    }
  })

export const preflightProviderProfileRequestSchema = z
  .object({
    profileId: z.string().trim().min(1),
    forceRefresh: z.boolean().default(false),
    modelId: z.string().trim().min(1).nullable().optional(),
    requiredFeatures: providerPreflightRequiredFeaturesSchema.default({
      streaming: true,
      toolCalls: true,
      reasoningControls: false,
      attachments: false,
    }),
  })
  .strict()

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
    capabilities: providerCapabilityCatalogSchema.nullable().optional(),
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
    contractVersion: z.literal(1).default(1),
    profileId: z.string().trim().min(1),
    providerId: runtimeProviderIdSchema,
    configuredModelId: z.string().trim().min(1),
    source: providerModelCatalogSourceSchema,
    fetchedAt: optionalIsoTimestampSchema,
    lastSuccessAt: optionalIsoTimestampSchema,
    lastRefreshError: providerModelCatalogDiagnosticSchema.nullable().optional(),
    capabilities: providerCapabilityCatalogSchema.nullable().optional(),
    cacheAgeSeconds: z.number().int().nullable().optional(),
    cacheTtlSeconds: z.number().int().positive().nullable().optional(),
    models: z.array(providerModelSchema),
    contractDiagnostics: z.array(providerModelCatalogContractDiagnosticSchema).default([]),
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
export type ProviderCapabilityStatusDto = z.infer<typeof providerCapabilityStatusSchema>
export type ProviderModelCatalogDiagnosticDto = z.infer<typeof providerModelCatalogDiagnosticSchema>
export type ProviderModelCatalogContractDiagnosticDto = z.infer<typeof providerModelCatalogContractDiagnosticSchema>
export type ProviderModelThinkingCapabilityDto = z.infer<typeof providerModelThinkingCapabilitySchema>
export type ProviderFeatureCapabilityDto = z.infer<typeof providerFeatureCapabilitySchema>
export type ProviderToolCallCapabilityDto = z.infer<typeof providerToolCallCapabilitySchema>
export type ProviderReasoningCapabilityDto = z.infer<typeof providerReasoningCapabilitySchema>
export type ProviderAttachmentCapabilityDto = z.infer<typeof providerAttachmentCapabilitySchema>
export type ProviderContextLimitCapabilityDto = z.infer<typeof providerContextLimitCapabilitySchema>
export type ProviderCapabilityFeatureSetDto = z.infer<typeof providerCapabilityFeatureSetSchema>
export type ProviderCatalogCacheMetadataDto = z.infer<typeof providerCatalogCacheMetadataSchema>
export type ProviderRedactedRequestPreviewDto = z.infer<typeof providerRedactedRequestPreviewSchema>
export type ProviderCapabilityCatalogDto = z.infer<typeof providerCapabilityCatalogSchema>
export type ProviderPreflightStatusDto = z.infer<typeof providerPreflightStatusSchema>
export type ProviderPreflightSourceDto = z.infer<typeof providerPreflightSourceSchema>
export type ProviderPreflightRequiredFeaturesDto = z.infer<typeof providerPreflightRequiredFeaturesSchema>
export type ProviderPreflightCheckDto = z.infer<typeof providerPreflightCheckSchema>
export type ProviderPreflightSnapshotDto = z.infer<typeof providerPreflightSnapshotSchema>
export type ProviderModelDto = z.infer<typeof providerModelSchema>
export type ProviderModelCatalogDto = z.infer<typeof providerModelCatalogSchema>
export type GetProviderModelCatalogRequestDto = z.infer<typeof getProviderModelCatalogRequestSchema>
export type PreflightProviderProfileRequestDto = z.infer<typeof preflightProviderProfileRequestSchema>

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

export function getProviderModelCatalogFreshnessLabel(
  catalog: ProviderModelCatalogDto | null | undefined,
): string {
  if (!catalog) {
    return 'Catalog unavailable'
  }

  const sourceLabel =
    catalog.source === 'live'
      ? 'Live'
      : catalog.source === 'cache'
        ? 'Cached'
        : catalog.source === 'manual'
          ? 'Manual'
          : 'Unavailable'
  const age = catalog.cacheAgeSeconds ?? catalog.capabilities?.cache.ageSeconds ?? null
  const ttl = catalog.cacheTtlSeconds ?? catalog.capabilities?.cache.ttlSeconds ?? null
  if (typeof age === 'number' && typeof ttl === 'number' && ttl > 0) {
    return `${sourceLabel} · ${formatProviderCatalogAge(age)} / ${formatProviderCatalogAge(ttl)} TTL`
  }
  if (typeof age === 'number') {
    return `${sourceLabel} · ${formatProviderCatalogAge(age)} old`
  }
  return sourceLabel
}

export function getProviderCapabilityStatusLabel(
  status: ProviderCapabilityStatusDto | null | undefined,
): string {
  switch (status) {
    case 'supported':
      return 'Supported'
    case 'probed':
      return 'Probed'
    case 'not_applicable':
      return 'N/A'
    case 'unavailable':
      return 'Unavailable'
    case 'unknown':
    default:
      return 'Unknown'
  }
}

function formatProviderCatalogAge(seconds: number): string {
  if (seconds < 60) {
    return `${Math.max(0, Math.round(seconds))}s`
  }
  const minutes = Math.round(seconds / 60)
  if (minutes < 60) {
    return `${minutes}m`
  }
  const hours = Math.round(minutes / 60)
  if (hours < 48) {
    return `${hours}h`
  }
  return `${Math.round(hours / 24)}d`
}

export function hasProviderModelCatalogSnapshot(catalog: ProviderModelCatalogDto | null | undefined): boolean {
  if (!catalog) {
    return false
  }

  return catalog.source !== 'unavailable' && catalog.models.length > 0
}

function estimateOptionalTextBytes(value: string | null | undefined): number {
  return value ? estimateUtf16Bytes(value) : 0
}

export function estimateProviderModelCatalogBytes(
  catalog: ProviderModelCatalogDto | null | undefined,
): number {
  if (!catalog) {
    return 0
  }

  let bytes = 128
  bytes += estimateUtf16Bytes(catalog.profileId)
  bytes += estimateUtf16Bytes(catalog.providerId)
  bytes += estimateUtf16Bytes(catalog.configuredModelId)
  bytes += estimateUtf16Bytes(catalog.source)
  bytes += estimateOptionalTextBytes(catalog.fetchedAt)
  bytes += estimateOptionalTextBytes(catalog.lastSuccessAt)
  bytes += estimateOptionalTextBytes(catalog.capabilities ? JSON.stringify(catalog.capabilities) : null)
  if (catalog.lastRefreshError) {
    bytes += estimateUtf16Bytes(catalog.lastRefreshError.code)
    bytes += estimateUtf16Bytes(catalog.lastRefreshError.message)
  }

  for (const model of catalog.models) {
    bytes += 96
    bytes += estimateUtf16Bytes(model.modelId)
    bytes += estimateUtf16Bytes(model.displayName)
    bytes += estimateOptionalTextBytes(model.contextLimitFetchedAt)
    bytes += estimateOptionalTextBytes(model.contextLimitSource)
    bytes += estimateOptionalTextBytes(model.contextLimitConfidence)
    bytes += estimateOptionalTextBytes(model.capabilities ? JSON.stringify(model.capabilities) : null)
    bytes += estimateOptionalTextBytes(model.thinking.defaultEffort)
    for (const effort of model.thinking.effortOptions) {
      bytes += estimateUtf16Bytes(effort)
    }
  }

  return bytes
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

export function createPreflightProviderProfileRequest(
  profileId: string,
  options: {
    forceRefresh?: boolean
    modelId?: string | null
    requiredFeatures?: Partial<ProviderPreflightRequiredFeaturesDto>
  } = {},
): PreflightProviderProfileRequestDto {
  return preflightProviderProfileRequestSchema.parse({
    profileId,
    forceRefresh: options.forceRefresh ?? false,
    modelId: options.modelId ?? null,
    requiredFeatures: {
      streaming: options.requiredFeatures?.streaming ?? true,
      toolCalls: options.requiredFeatures?.toolCalls ?? true,
      reasoningControls: options.requiredFeatures?.reasoningControls ?? false,
      attachments: options.requiredFeatures?.attachments ?? false,
    },
  })
}
