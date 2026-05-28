import { describe, expect, it } from 'vitest'

import {
  dictationEventSchema,
  dictationStartRequestSchema,
  dictationStartResponseSchema,
  dictationStatusSchema,
} from './dictation'

describe('dictation contract', () => {
  it('normalizes status optional text without changing capability truth', () => {
    const status = dictationStatusSchema.parse({
      platform: 'macos',
      defaultLocale: ' en_US ',
      modern: {
        available: true,
        compiled: true,
        runtimeSupported: true,
        reason: '',
      },
      legacy: {
        available: false,
        compiled: true,
        runtimeSupported: true,
        reason: ' legacy_recognizer_unavailable ',
      },
      microphonePermission: 'not_determined',
      speechPermission: 'authorized',
      activeSession: undefined,
    })

    expect(status.defaultLocale).toBe('en_US')
    expect(status.modern.reason).toBeNull()
    expect(status.legacy.reason).toBe('legacy_recognizer_unavailable')
    expect(status.windowsSdk).toEqual({
      available: false,
      compiled: false,
      runtimeSupported: false,
      reason: null,
    })
    expect(status.activeSession).toBeNull()

    expect(
      dictationStatusSchema.parse({
        platform: 'linux',
        modern: {
          available: false,
          compiled: false,
          runtimeSupported: false,
          reason: 'native_engine_unavailable',
        },
        legacy: {
          available: false,
          compiled: false,
          runtimeSupported: false,
          reason: 'native_engine_unavailable',
        },
        microphonePermission: 'unsupported',
        speechPermission: 'unsupported',
      }).platform,
    ).toBe('linux')

    expect(
      dictationStatusSchema.parse({
        platform: 'windows',
        defaultLocale: 'en-US',
        modern: {
          available: false,
          compiled: false,
          runtimeSupported: false,
          reason: 'macos_modern_unavailable_on_windows',
        },
        legacy: {
          available: false,
          compiled: false,
          runtimeSupported: false,
          reason: 'macos_legacy_unavailable_on_windows',
        },
        windowsSdk: {
          available: true,
          compiled: true,
          runtimeSupported: true,
          reason: '',
        },
        microphonePermission: 'unknown',
        speechPermission: 'unknown',
      }).windowsSdk.reason,
    ).toBeNull()
  })

  it('normalizes start defaults and validates dictation event shapes', () => {
    expect(
      dictationStartRequestSchema.parse({
        locale: ' ',
        contextualPhrases: ['Xero', 'Tauri'],
      }),
    ).toEqual({
      locale: null,
      enginePreference: 'automatic',
      privacyMode: 'on_device_preferred',
      contextualPhrases: ['Xero', 'Tauri'],
    })

    expect(
      dictationStartResponseSchema.parse({
        sessionId: 'session-1',
        engine: 'windows_sdk',
        locale: 'en_US',
      }),
    ).toEqual({
      sessionId: 'session-1',
      engine: 'windows_sdk',
      locale: 'en_US',
    })

    expect(
      dictationEventSchema.parse({
        kind: 'audio_level',
        sessionId: 'session-1',
        level: 0.42,
        sequence: 3,
      }),
    ).toMatchObject({
      kind: 'audio_level',
      sessionId: 'session-1',
      level: 0.42,
      sequence: 3,
    })

    expect(
      dictationEventSchema.parse({
        kind: 'partial',
        sessionId: 'session-1',
        text: 'hello',
        sequence: 2,
      }),
    ).toMatchObject({
      kind: 'partial',
      sessionId: 'session-1',
      text: 'hello',
      sequence: 2,
    })
  })
})
