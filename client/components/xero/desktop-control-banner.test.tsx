import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { DesktopControlBanner } from '@/components/xero/desktop-control-banner'
import type { DesktopControlStatusDto } from '@/src/lib/xero-model/desktop-control'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('DesktopControlBanner', () => {
  it('stays hidden while desktop control is idle', async () => {
    const adapter = makeAdapter({ status: makeStatus() })

    render(<DesktopControlBanner adapter={adapter} />)

    await waitFor(() => expect(adapter.desktopControlStatus).toHaveBeenCalled())
    expect(screen.queryByText('Remote desktop control active')).not.toBeInTheDocument()
    expect(screen.queryByText('Desktop stream active')).not.toBeInTheDocument()
  })

  it('shows active cloud manual control and routes stop through the broker', async () => {
    const stopped = makeStatus()
    const adapter = makeAdapter({
      status: makeStatus({
        controllerLock: {
          actor: 'cloud_manual_control',
          sessionId: 'agent-session-global-computer-use',
          runId: 'run-123',
          acquiredAt: '2026-05-26T12:00:00Z',
          expiresAt: '2026-05-26T12:01:00Z',
          lastInputAt: '2026-05-26T12:00:30Z',
          releaseReason: null,
        },
      }),
      stopStatus: stopped,
    })

    render(<DesktopControlBanner adapter={adapter} />)

    await screen.findByText('Remote desktop control active')
    expect(screen.queryByText(/holds the desktop controller lock/i)).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Stop' }))

    await waitFor(() => expect(adapter.desktopControlStop).toHaveBeenCalledTimes(1))
    await waitFor(() =>
      expect(screen.queryByText('Remote desktop control active')).not.toBeInTheDocument(),
    )
  })

  it('shows active degraded streams and opens desktop-control settings', async () => {
    const onOpenSettings = vi.fn()
    const adapter = makeAdapter({
      status: makeStatus({
        stream: {
          streamId: 'stream-123',
          status: 'degraded',
          transport: 'screenshot_fallback',
          signalingChannel: 'computer_use_stream',
          quality: 'balanced',
          maxWidth: 1280,
          maxFrameRate: 2,
          includeCursor: true,
          message: 'Desktop stream is using fallback frames.',
        },
      }),
    })

    render(<DesktopControlBanner adapter={adapter} onOpenSettings={onOpenSettings} />)

    await screen.findByText('Desktop stream active')
    expect(screen.queryByText('Desktop stream is using fallback frames.')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Settings' }))

    expect(onOpenSettings).toHaveBeenCalledTimes(1)
  })

  it('stops active WebRTC streams with native stream metadata', async () => {
    const adapter = makeAdapter({
      status: makeStatus({
        stream: makeWebRtcStream({ status: 'live', message: 'Native stream is live.' }),
      }),
      stopStatus: makeStatus({
        stream: makeWebRtcStream({
          status: 'stopped',
          message: 'Desktop stream stopped.',
        }),
      }),
    })

    render(<DesktopControlBanner adapter={adapter} />)

    await screen.findByText('Desktop stream active')
    expect(screen.queryByText('Native stream is live.')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Stop' }))

    await waitFor(() => expect(adapter.desktopControlStop).toHaveBeenCalledTimes(1))
    await waitFor(() =>
      expect(screen.queryByText('Desktop stream active')).not.toBeInTheDocument(),
    )
  })
})

function makeAdapter({
  status,
  stopStatus,
}: {
  status: DesktopControlStatusDto
  stopStatus?: DesktopControlStatusDto
}) {
  return {
    isDesktopRuntime: vi.fn(() => true),
    desktopControlStatus: vi.fn(async () => status),
    desktopControlStop: vi.fn(async () => stopStatus ?? makeStatus()),
  }
}

function makeStatus(overrides: Partial<DesktopControlStatusDto> = {}): DesktopControlStatusDto {
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
      notificationObservation: true,
      foregroundState: true,
      cursorState: true,
      accessibilitySnapshot: true,
      ocrSnapshot: true,
      mouseInput: true,
      keyboardInput: true,
      clipboard: true,
      windowFocus: true,
      appControl: true,
      accessibilityActions: true,
      menuSelect: true,
      webrtcStream: false,
      screenshotFallbackStream: true,
      nativeVideoTrack: false,
      preferredCodec: null,
      captureBackends: [],
      encoderBackends: [],
      hardwareEncoding: false,
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
      cloudStreamingEnabled: true,
      manualCloudControlEnabled: true,
      policyProfile: 'default_safe',
      ownerAdminExpiresAt: null,
      updatedAt: '2026-05-26T12:00:00Z',
    },
    auditLogPath: '/tmp/xero/desktop-control/audit.jsonl',
    updatedAt: '2026-05-26T12:00:00Z',
    ...overrides,
  }
}

function makeWebRtcStream(overrides: Partial<DesktopControlStatusDto['stream']> = {}) {
  return {
    streamId: 'stream-123',
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
  } satisfies DesktopControlStatusDto['stream']
}
