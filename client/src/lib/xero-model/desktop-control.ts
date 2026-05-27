import { z } from 'zod'

const desktopRedactionModeSchema = z.enum(['off', 'balanced', 'auto', 'strict'])
const desktopControlPermissionActionKindSchema = z.enum(['open_macos_privacy_pane'])

const desktopRegionSchema = z
  .object({
    x: z.number().int().min(0),
    y: z.number().int().min(0),
    width: z.number().int().positive(),
    height: z.number().int().positive(),
  })
  .strict()

export const desktopControlSettingsSchema = z
  .object({
    cloudStreamingEnabled: z.boolean(),
    manualCloudControlEnabled: z.boolean(),
    redactionMode: desktopRedactionModeSchema,
    privateRegions: z.array(desktopRegionSchema),
    updatedAt: z.string().nullable(),
  })
  .strict()

export const upsertDesktopControlSettingsRequestSchema = z
  .object({
    cloudStreamingEnabled: z.boolean(),
    manualCloudControlEnabled: z.boolean(),
    redactionMode: desktopRedactionModeSchema,
    privateRegions: z.array(desktopRegionSchema),
  })
  .strict()

const desktopPermissionSchema = z
  .object({
    name: z.string(),
    status: z.enum(['granted', 'denied', 'unknown', 'unsupported']),
    requiredFor: z.array(z.string()),
    remediation: z.string(),
    action: z
      .object({
        kind: desktopControlPermissionActionKindSchema,
        target: z.string().min(1),
        label: z.string().min(1),
        postActionHint: z.string().min(1),
      })
      .strict()
      .nullable()
      .optional(),
  })
  .strict()

export const desktopControlOpenPermissionSettingsRequestSchema = z
  .object({
    kind: desktopControlPermissionActionKindSchema,
    target: z.string().min(1),
  })
  .strict()

const desktopCapabilitiesSchema = z
  .object({
    platform: z.string(),
    schemaVersion: z.number(),
    displayList: z.boolean(),
    screenshot: z.boolean(),
    windowList: z.boolean(),
    appList: z.boolean(),
    foregroundState: z.boolean(),
    cursorState: z.boolean(),
    accessibilitySnapshot: z.boolean(),
    ocrSnapshot: z.boolean(),
    mouseInput: z.boolean(),
    keyboardInput: z.boolean(),
    clipboard: z.boolean(),
    accessibilityActions: z.boolean(),
    menuSelect: z.boolean(),
    webrtcStream: z.boolean(),
    screenshotFallbackStream: z.boolean(),
    manualCloudControl: z.boolean(),
  })
  .strict()

const desktopSidecarSchema = z
  .object({
    schemaVersion: z.number(),
    platform: z.string(),
    transport: z.string(),
    authenticated: z.boolean(),
    health: z.string(),
    message: z.string(),
  })
  .strict()

const desktopControllerLockSchema = z
  .object({
    actor: z.enum(['agent', 'local_user', 'cloud_manual_control']),
    leaseId: z.string().nullable().optional(),
    sessionId: z.string(),
    runId: z.string().nullable().optional(),
    acquiredAt: z.string(),
    expiresAt: z.string(),
    lastInputAt: z.string(),
    releaseReason: z.string().nullable().optional(),
  })
  .strict()

const desktopStreamSchema = z
  .object({
    streamId: z.string().nullable().optional(),
    status: z.enum(['idle', 'starting', 'live', 'degraded', 'paused', 'stopped', 'failed']),
    transport: z.enum(['web_rtc', 'screenshot_fallback', 'unavailable']),
    signalingChannel: z.string().nullable().optional(),
    quality: z.enum(['low', 'balanced', 'high']),
    maxWidth: z.number(),
    maxFrameRate: z.number(),
    includeCursor: z.boolean(),
    message: z.string(),
  })
  .strict()

export const desktopControlStatusSchema = z
  .object({
    schema: z.literal('xero.desktop_control_status.v1'),
    platform: z.string(),
    sidecar: desktopSidecarSchema,
    capabilities: desktopCapabilitiesSchema,
    permissions: z.array(desktopPermissionSchema),
    controllerLock: desktopControllerLockSchema.nullable().optional(),
    stream: desktopStreamSchema,
    settings: desktopControlSettingsSchema,
    auditLogPath: z.string(),
    updatedAt: z.string(),
  })
  .strict()

export type DesktopControlSettingsDto = z.infer<typeof desktopControlSettingsSchema>
export type UpsertDesktopControlSettingsRequestDto = z.infer<
  typeof upsertDesktopControlSettingsRequestSchema
>
export type DesktopControlOpenPermissionSettingsRequestDto = z.infer<
  typeof desktopControlOpenPermissionSettingsRequestSchema
>
export type DesktopControlStatusDto = z.infer<typeof desktopControlStatusSchema>
