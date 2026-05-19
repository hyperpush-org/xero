import { z } from 'zod'
import { optionalIsoTimestampSchema } from '@xero/ui/model/shared'

export const adrenalineModeAssertionKindSchema = z.enum([
  'prevent_idle_system_sleep',
  'prevent_idle_display_sleep',
])

export const adrenalineModeActiveStatusSchema = z.enum(['active', 'inactive', 'unsupported'])

export const closedLidModeActiveStatusSchema = z.enum([
  'active',
  'inactive',
  'needs_attention',
  'unsupported',
])

export const adrenalineModeSettingsSchema = z
  .object({
    enabled: z.boolean(),
    assertionKind: adrenalineModeAssertionKindSchema,
    active: z.boolean(),
    activeStatus: adrenalineModeActiveStatusSchema,
    platformSupported: z.boolean(),
    updatedAt: optionalIsoTimestampSchema,
    diagnosticMessage: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export const upsertAdrenalineModeSettingsRequestSchema = z
  .object({
    enabled: z.boolean(),
    assertionKind: adrenalineModeAssertionKindSchema,
  })
  .strict()

export const closedLidModeSettingsSchema = z
  .object({
    enabled: z.boolean(),
    active: z.boolean(),
    activeStatus: closedLidModeActiveStatusSchema,
    platformSupported: z.boolean(),
    authorizationRequired: z.boolean(),
    currentDisablesleep: z.boolean().nullable().optional(),
    previousDisablesleep: z.boolean().nullable().optional(),
    updatedAt: optionalIsoTimestampSchema,
    diagnosticMessage: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export const upsertClosedLidModeSettingsRequestSchema = z
  .object({
    enabled: z.boolean(),
    acknowledgeGlobalPowerChange: z.boolean(),
  })
  .strict()

export type AdrenalineModeAssertionKindDto = z.infer<typeof adrenalineModeAssertionKindSchema>
export type AdrenalineModeActiveStatusDto = z.infer<typeof adrenalineModeActiveStatusSchema>
export type AdrenalineModeSettingsDto = z.infer<typeof adrenalineModeSettingsSchema>
export type UpsertAdrenalineModeSettingsRequestDto = z.infer<
  typeof upsertAdrenalineModeSettingsRequestSchema
>
export type ClosedLidModeActiveStatusDto = z.infer<typeof closedLidModeActiveStatusSchema>
export type ClosedLidModeSettingsDto = z.infer<typeof closedLidModeSettingsSchema>
export type UpsertClosedLidModeSettingsRequestDto = z.infer<
  typeof upsertClosedLidModeSettingsRequestSchema
>
