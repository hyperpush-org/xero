import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
}))

vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {},
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
})
