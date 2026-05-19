import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
  listen: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {},
  invoke: mocks.invoke,
  isTauri: mocks.isTauri,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: mocks.listen,
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

describe('XeroDesktopAdapter Adrenaline Mode', () => {
  beforeEach(() => {
    mocks.invoke.mockReset()
    mocks.isTauri.mockReturnValue(true)
    mocks.listen.mockReset()
  })

  it('loads and updates Adrenaline and Closed-Lid modes through the adapter contract', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    mocks.invoke.mockResolvedValueOnce({
      enabled: false,
      assertionKind: 'prevent_idle_system_sleep',
      active: false,
      activeStatus: 'inactive',
      platformSupported: true,
      updatedAt: null,
      diagnosticMessage: null,
    })

    await expect(XeroDesktopAdapter.adrenalineModeSettings?.()).resolves.toEqual({
      enabled: false,
      assertionKind: 'prevent_idle_system_sleep',
      active: false,
      activeStatus: 'inactive',
      platformSupported: true,
      updatedAt: null,
      diagnosticMessage: null,
    })
    expect(mocks.invoke).toHaveBeenCalledWith('adrenaline_mode_settings', undefined)

    mocks.invoke.mockResolvedValueOnce({
      enabled: true,
      assertionKind: 'prevent_idle_system_sleep',
      active: true,
      activeStatus: 'active',
      platformSupported: true,
      updatedAt: '2026-05-18T12:01:00Z',
      diagnosticMessage: null,
    })

    await expect(
      XeroDesktopAdapter.adrenalineModeUpdateSettings?.({
        enabled: true,
        assertionKind: 'prevent_idle_system_sleep',
      }),
    ).resolves.toMatchObject({
      enabled: true,
      active: true,
      activeStatus: 'active',
    })
    expect(mocks.invoke).toHaveBeenLastCalledWith('adrenaline_mode_update_settings', {
      request: {
        enabled: true,
        assertionKind: 'prevent_idle_system_sleep',
      },
    })

    mocks.invoke.mockResolvedValueOnce({
      enabled: false,
      active: false,
      activeStatus: 'inactive',
      platformSupported: true,
      authorizationRequired: true,
      currentDisablesleep: false,
      previousDisablesleep: null,
      updatedAt: null,
      diagnosticMessage: null,
    })

    await expect(XeroDesktopAdapter.closedLidModeSettings?.()).resolves.toMatchObject({
      enabled: false,
      active: false,
      activeStatus: 'inactive',
    })
    expect(mocks.invoke).toHaveBeenLastCalledWith('closed_lid_mode_settings', undefined)

    mocks.invoke.mockResolvedValueOnce({
      enabled: true,
      active: true,
      activeStatus: 'active',
      platformSupported: true,
      authorizationRequired: true,
      currentDisablesleep: true,
      previousDisablesleep: false,
      updatedAt: '2026-05-18T12:02:00Z',
      diagnosticMessage: null,
    })

    await expect(
      XeroDesktopAdapter.closedLidModeUpdateSettings?.({
        enabled: true,
        acknowledgeGlobalPowerChange: true,
      }),
    ).resolves.toMatchObject({
      enabled: true,
      active: true,
      activeStatus: 'active',
    })
    expect(mocks.invoke).toHaveBeenLastCalledWith('closed_lid_mode_update_settings', {
      request: {
        enabled: true,
        acknowledgeGlobalPowerChange: true,
      },
    })
  })
})
