import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
  channels: [] as Array<{ onmessage?: (message: unknown) => void }>,
}))

vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {
    onmessage?: (message: unknown) => void

    constructor() {
      mocks.channels.push(this)
    }
  },
  invoke: mocks.invoke,
  isTauri: mocks.isTauri,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

describe('CadenceDesktopAdapter dictation', () => {
  beforeEach(() => {
    mocks.invoke.mockReset()
    mocks.isTauri.mockReturnValue(true)
    mocks.channels.length = 0
  })

  it('normalizes mocked status responses through the adapter contract', async () => {
    const { CadenceDesktopAdapter } = await import('./cadence-desktop')

    mocks.invoke.mockResolvedValueOnce({
      platform: 'macos',
      defaultLocale: ' en_US ',
      modern: {
        available: false,
        compiled: false,
        runtimeSupported: false,
        reason: ' modern_sdk_unavailable ',
      },
      legacy: {
        available: true,
        compiled: true,
        runtimeSupported: true,
        reason: '',
      },
      microphonePermission: 'not_determined',
      speechPermission: 'authorized',
      activeSession: null,
    })

    await expect(CadenceDesktopAdapter.speechDictationStatus?.()).resolves.toEqual({
      platform: 'macos',
      defaultLocale: 'en_US',
      modern: {
        available: false,
        compiled: false,
        runtimeSupported: false,
        reason: 'modern_sdk_unavailable',
      },
      legacy: {
        available: true,
        compiled: true,
        runtimeSupported: true,
        reason: null,
      },
      microphonePermission: 'not_determined',
      speechPermission: 'authorized',
      activeSession: null,
    })
    expect(mocks.invoke).toHaveBeenCalledWith('speech_dictation_status', undefined)
  })

  it('starts dictation with a typed channel and normalizes streamed events', async () => {
    const { CadenceDesktopAdapter } = await import('./cadence-desktop')
    const handler = vi.fn()

    mocks.invoke.mockImplementationOnce(async (_command, args) => {
      args.request.channel.onmessage({
        kind: 'started',
        sessionId: 'session-1',
        engine: 'legacy',
        locale: 'en_US',
      })

      return {
        sessionId: 'session-1',
        engine: 'legacy',
        locale: 'en_US',
      }
    })

    const session = await CadenceDesktopAdapter.speechDictationStart?.(
      {
        locale: ' ',
        contextualPhrases: ['Cadence'],
      },
      handler,
    )

    expect(session?.response).toEqual({
      sessionId: 'session-1',
      engine: 'legacy',
      locale: 'en_US',
    })
    expect(handler).toHaveBeenCalledWith({
      kind: 'started',
      sessionId: 'session-1',
      engine: 'legacy',
      locale: 'en_US',
    })
    expect(mocks.invoke).toHaveBeenCalledWith('speech_dictation_start', {
      request: {
        locale: null,
        enginePreference: 'automatic',
        privacyMode: 'on_device_preferred',
        contextualPhrases: ['Cadence'],
        channel: mocks.channels[0],
      },
    })
  })

  it('stops and cancels dictation through idempotent commands', async () => {
    const { CadenceDesktopAdapter } = await import('./cadence-desktop')

    mocks.invoke.mockResolvedValue(undefined)

    await CadenceDesktopAdapter.speechDictationStop?.()
    await CadenceDesktopAdapter.speechDictationCancel?.()

    expect(mocks.invoke).toHaveBeenNthCalledWith(1, 'speech_dictation_stop', undefined)
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, 'speech_dictation_cancel', undefined)
  })
})
