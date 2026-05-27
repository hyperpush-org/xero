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

describe('XeroDesktopAdapter desktop control', () => {
  beforeEach(() => {
    mocks.invoke.mockReset()
    mocks.isTauri.mockReturnValue(true)
    mocks.listen.mockReset()
  })

  it('loads desktop-control status with permission remediation actions', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    mocks.invoke.mockResolvedValueOnce({
      schema: 'xero.desktop_control_status.v1',
      platform: 'macos',
      sidecar: {
        schemaVersion: 1,
        platform: 'macos',
        transport: 'stdio',
        authenticated: true,
        health: 'ready',
        message: 'Desktop sidecar is ready.',
      },
      capabilities: {
        platform: 'macos',
        schemaVersion: 1,
        displayList: true,
        screenshot: true,
        windowList: true,
        appList: true,
        foregroundState: true,
        cursorState: true,
        accessibilitySnapshot: true,
        ocrSnapshot: true,
        mouseInput: true,
        keyboardInput: true,
        clipboard: true,
        accessibilityActions: true,
        menuSelect: true,
        webrtcStream: false,
        screenshotFallbackStream: true,
        manualCloudControl: true,
      },
      permissions: [
        {
          name: 'Accessibility',
          status: 'denied',
          requiredFor: ['mouse', 'keyboard'],
          remediation: 'Grant Accessibility permission to Xero.',
          action: {
            kind: 'open_macos_privacy_pane',
            target: 'Privacy_Accessibility',
            label: 'Open Accessibility',
            postActionHint: 'Return here and refresh status.',
          },
        },
      ],
      controllerLock: null,
      stream: {
        streamId: null,
        status: 'idle',
        transport: 'unavailable',
        signalingChannel: null,
        quality: 'balanced',
        maxWidth: 1280,
        maxFrameRate: 2,
        includeCursor: true,
        message: 'Desktop stream is idle.',
      },
      settings: {
        cloudStreamingEnabled: false,
        manualCloudControlEnabled: false,
        redactionMode: 'balanced',
        privateRegions: [],
        updatedAt: null,
      },
      auditLogPath: '/tmp/xero/desktop-control/audit.jsonl',
      updatedAt: '2026-05-26T12:00:00Z',
    })

    await expect(XeroDesktopAdapter.desktopControlStatus?.()).resolves.toMatchObject({
      permissions: [
        {
          name: 'Accessibility',
          action: {
            kind: 'open_macos_privacy_pane',
            target: 'Privacy_Accessibility',
          },
        },
      ],
    })
    expect(mocks.invoke).toHaveBeenCalledWith('desktop_control_status', undefined)
  })

  it('routes permission settings through the vetted desktop command', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    mocks.invoke.mockResolvedValueOnce(undefined)

    await XeroDesktopAdapter.desktopControlOpenPermissionSettings?.({
      kind: 'open_macos_privacy_pane',
      target: 'Privacy_ScreenCapture',
    })

    expect(mocks.invoke).toHaveBeenCalledWith('desktop_control_open_permission_settings', {
      request: {
        kind: 'open_macos_privacy_pane',
        target: 'Privacy_ScreenCapture',
      },
    })
  })
})
