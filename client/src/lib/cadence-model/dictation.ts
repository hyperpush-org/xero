import { z } from 'zod'

import { normalizeOptionalText } from './shared'

export const dictationPlatformSchema = z.enum(['macos', 'unsupported'])
export const dictationEngineSchema = z.enum(['modern', 'legacy'])
export const dictationEnginePreferenceSchema = z.enum(['automatic', 'modern', 'legacy'])
export const dictationPrivacyModeSchema = z.enum([
  'on_device_preferred',
  'on_device_required',
  'allow_network',
])
export const dictationPermissionStateSchema = z.enum([
  'authorized',
  'denied',
  'restricted',
  'not_determined',
  'unsupported',
  'unknown',
])
export const dictationStopReasonSchema = z.enum([
  'user',
  'cancelled',
  'error',
  'channel_closed',
  'app_closing',
])

const normalizedOptionalTextSchema = z
  .string()
  .nullable()
  .optional()
  .transform((value) => normalizeOptionalText(value))

export const dictationEngineStatusSchema = z
  .object({
    available: z.boolean(),
    compiled: z.boolean(),
    runtimeSupported: z.boolean(),
    reason: normalizedOptionalTextSchema,
  })
  .strict()

export const activeDictationSessionSchema = z
  .object({
    sessionId: z.string().trim().min(1),
    engine: dictationEngineSchema,
  })
  .strict()

export const dictationStatusSchema = z
  .object({
    platform: dictationPlatformSchema,
    defaultLocale: normalizedOptionalTextSchema,
    modern: dictationEngineStatusSchema,
    legacy: dictationEngineStatusSchema,
    microphonePermission: dictationPermissionStateSchema,
    speechPermission: dictationPermissionStateSchema,
    activeSession: activeDictationSessionSchema.nullable().optional().transform((value) => value ?? null),
  })
  .strict()

export const dictationStartRequestSchema = z
  .object({
    locale: normalizedOptionalTextSchema,
    enginePreference: dictationEnginePreferenceSchema.nullable().optional().transform((value) => value ?? 'automatic'),
    privacyMode: dictationPrivacyModeSchema
      .nullable()
      .optional()
      .transform((value) => value ?? 'on_device_preferred'),
    contextualPhrases: z.array(z.string().trim().min(1)).optional().default([]),
  })
  .strict()

export const dictationStartResponseSchema = z
  .object({
    sessionId: z.string().trim().min(1),
    engine: dictationEngineSchema,
    locale: z.string().trim().min(1),
  })
  .strict()

const dictationSequenceSchema = z.number().int().nonnegative()

export const dictationEventSchema = z.discriminatedUnion('kind', [
  z
    .object({
      kind: z.literal('permission'),
      microphone: dictationPermissionStateSchema,
      speech: dictationPermissionStateSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('started'),
      sessionId: z.string().trim().min(1),
      engine: dictationEngineSchema,
      locale: z.string().trim().min(1),
    })
    .strict(),
  z
    .object({
      kind: z.literal('asset_installing'),
      progress: z.number().min(0).max(1).nullable(),
    })
    .strict(),
  z
    .object({
      kind: z.literal('partial'),
      sessionId: z.string().trim().min(1),
      text: z.string(),
      sequence: dictationSequenceSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('final'),
      sessionId: z.string().trim().min(1),
      text: z.string(),
      sequence: dictationSequenceSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('stopped'),
      sessionId: z.string().trim().min(1),
      reason: dictationStopReasonSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('error'),
      sessionId: z.string().trim().min(1).nullable(),
      code: z.string().trim().min(1),
      message: z.string().trim().min(1),
      retryable: z.boolean(),
    })
    .strict(),
])

export type DictationPlatformDto = z.infer<typeof dictationPlatformSchema>
export type DictationEngineDto = z.infer<typeof dictationEngineSchema>
export type DictationEnginePreferenceDto = z.infer<typeof dictationEnginePreferenceSchema>
export type DictationPrivacyModeDto = z.infer<typeof dictationPrivacyModeSchema>
export type DictationPermissionStateDto = z.infer<typeof dictationPermissionStateSchema>
export type DictationStopReasonDto = z.infer<typeof dictationStopReasonSchema>
export type DictationEngineStatusDto = z.infer<typeof dictationEngineStatusSchema>
export type ActiveDictationSessionDto = z.infer<typeof activeDictationSessionSchema>
export type DictationStatusDto = z.infer<typeof dictationStatusSchema>
export type DictationStartRequestInputDto = z.input<typeof dictationStartRequestSchema>
export type DictationStartRequestDto = z.infer<typeof dictationStartRequestSchema>
export type DictationStartResponseDto = z.infer<typeof dictationStartResponseSchema>
export type DictationEventDto = z.infer<typeof dictationEventSchema>
