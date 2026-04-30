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
    expect(status.activeSession).toBeNull()
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
        engine: 'legacy',
        locale: 'en_US',
      }),
    ).toEqual({
      sessionId: 'session-1',
      engine: 'legacy',
      locale: 'en_US',
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
