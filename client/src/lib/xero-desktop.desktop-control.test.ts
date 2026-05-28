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
        webrtcStream: true,
        screenshotFallbackStream: true,
        nativeVideoTrack: true,
        preferredCodec: 'video/H264',
        captureBackends: ['screencapturekit'],
        encoderBackends: ['videotoolbox'],
        hardwareEncoding: true,
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
        updatedAt: null,
      },
      auditLogPath: '/tmp/xero/desktop-control/audit.jsonl',
      updatedAt: '2026-05-26T12:00:00Z',
    })

    await expect(XeroDesktopAdapter.desktopControlStatus?.()).resolves.toMatchObject({
      capabilities: {
        nativeVideoTrack: true,
        preferredCodec: 'video/H264',
        captureBackends: ['screencapturekit'],
        encoderBackends: ['videotoolbox'],
        hardwareEncoding: true,
      },
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
    expect(mocks.invoke).toHaveBeenCalledWith('desktop_control_status', {
      request: { refreshPermissionStatus: false },
    })

    mocks.invoke.mockResolvedValueOnce(makeDesktopControlStatus())

    await expect(
      XeroDesktopAdapter.desktopControlStatus?.({ refreshPermissionStatus: true }),
    ).resolves.toMatchObject({
      schema: 'xero.desktop_control_status.v1',
    })
    expect(mocks.invoke).toHaveBeenLastCalledWith('desktop_control_status', {
      request: { refreshPermissionStatus: true },
    })
  })

  it('parses active WebRTC desktop streams and stopped stream responses with metrics', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    mocks.invoke.mockResolvedValueOnce(
      makeDesktopControlStatus({
        stream: makeWebRtcStreamStatus({ status: 'live', message: 'Native stream is live.' }),
      }),
    )

    await expect(XeroDesktopAdapter.desktopControlStatus?.()).resolves.toMatchObject({
      stream: {
        displayId: 'display-1',
        status: 'live',
        transport: 'web_rtc',
        metrics: {
          captureBackend: 'screencapturekit',
          encoderBackend: 'videotoolbox',
          keyframes: 2,
        },
      },
    })
    expect(mocks.invoke).toHaveBeenCalledWith('desktop_control_status', {
      request: { refreshPermissionStatus: false },
    })

    mocks.invoke.mockResolvedValueOnce(
      makeDesktopControlStatus({
        stream: makeWebRtcStreamStatus({
          status: 'stopped',
          message: 'Desktop stream stopped.',
        }),
      }),
    )

    await expect(XeroDesktopAdapter.desktopControlStop?.()).resolves.toMatchObject({
      stream: {
        status: 'stopped',
        transport: 'web_rtc',
        metrics: {
          captureDroppedFrames: 0,
          keyframes: 2,
        },
      },
    })
    expect(mocks.invoke).toHaveBeenLastCalledWith('desktop_control_stop', undefined)
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

function makeDesktopControlStatus(overrides: Record<string, unknown> = {}) {
  return {
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
      webrtcStream: true,
      screenshotFallbackStream: true,
      nativeVideoTrack: true,
      preferredCodec: 'video/H264',
      captureBackends: ['screencapturekit'],
      encoderBackends: ['videotoolbox'],
      hardwareEncoding: true,
      manualCloudControl: true,
    },
    permissions: [],
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
      updatedAt: null,
    },
    auditLogPath: '/tmp/xero/desktop-control/audit.jsonl',
    updatedAt: '2026-05-26T12:00:00Z',
    ...overrides,
  }
}

function makeWebRtcStreamStatus(overrides: Record<string, unknown> = {}) {
  return {
    streamId: 'stream-1',
    displayId: 'display-1',
    status: 'live',
    transport: 'web_rtc',
    signalingChannel: 'computer_use_stream',
    quality: 'balanced',
    maxWidth: 1280,
    maxFrameRate: 30,
    includeCursor: true,
    metrics: {
      captureBackend: 'screencapturekit',
      encoderBackend: 'videotoolbox',
      encoderHardware: true,
      preferredCodec: 'video/H264',
      captureFrameRate: 30,
      captureDroppedFrames: 0,
      encodeFrameRate: 30,
      encodeLatencyMs: 4,
      outboundBitrateBps: 2_500_000,
      availableOutgoingBitrateBps: 5_000_000,
      packetsSent: 120,
      bytesSent: 512_000,
      packetLoss: 0,
      roundTripTimeMs: 12,
      retransmits: 0,
      keyframes: 2,
    },
    message: 'Native stream is live.',
    ...overrides,
  }
}
