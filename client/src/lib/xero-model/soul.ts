import { z } from 'zod'
import { optionalIsoTimestampSchema } from '@xero/ui/model/shared'

export const soulIdSchema = z.enum(['steward', 'pair', 'builder', 'sentinel'])

export const soulPresetSchema = z
  .object({
    id: soulIdSchema,
    name: z.string().trim().min(1),
    summary: z.string().trim().min(1),
    prompt: z.string().trim().min(1),
  })
  .strict()

export const soulSettingsSchema = z
  .object({
    selectedSoulId: soulIdSchema,
    selectedSoul: soulPresetSchema,
    presets: z.array(soulPresetSchema).min(1),
    updatedAt: optionalIsoTimestampSchema,
  })
  .strict()

export const upsertSoulSettingsRequestSchema = z
  .object({
    selectedSoulId: soulIdSchema,
  })
  .strict()

export type SoulIdDto = z.infer<typeof soulIdSchema>
export type SoulPresetDto = z.infer<typeof soulPresetSchema>
export type SoulSettingsDto = z.infer<typeof soulSettingsSchema>
export type UpsertSoulSettingsRequestDto = z.infer<typeof upsertSoulSettingsRequestSchema>
