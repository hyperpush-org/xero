import { z } from 'zod'
import { optionalIsoTimestampSchema } from '@xero/ui/model/shared'

export const browserControlPreferenceSchema = z.enum(['default', 'in_app_browser', 'native_browser'])

export const browserControlSettingsSchema = z
  .object({
    preference: browserControlPreferenceSchema,
    updatedAt: optionalIsoTimestampSchema,
  })
  .strict()

export const upsertBrowserControlSettingsRequestSchema = z
  .object({
    preference: browserControlPreferenceSchema,
  })
  .strict()

export type BrowserControlPreferenceDto = z.infer<typeof browserControlPreferenceSchema>
export type BrowserControlSettingsDto = z.infer<typeof browserControlSettingsSchema>
export type UpsertBrowserControlSettingsRequestDto = z.infer<
  typeof upsertBrowserControlSettingsRequestSchema
>
