import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { DesktopControlSection } from '@/components/xero/settings-dialog/desktop-control-section'
import type {
  DesktopControlStatusDto,
  UpsertDesktopControlSettingsRequestDto,
} from '@/src/lib/xero-model/desktop-control'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('DesktopControlSection', () => {
  it('shows brokered macOS permission actions and retry guidance', async () => {
    const adapter = makeAdapter({
      status: makeStatus({
        permissions: [
          {
            name: 'Screen Recording',
            status: 'denied',
            requiredFor: ['screenshot', 'stream'],
            remediation: 'Grant screen capture permission in System Settings.',
            action: {
              kind: 'open_macos_privacy_pane',
              target: 'Privacy_ScreenCapture',
              label: 'Open Screen Recording',
              postActionHint:
                'After changing Screen Recording, macOS may ask you to quit and reopen Xero. Return here and refresh status after Xero is running again.',
            },
          },
        ],
      }),
    })

    render(<DesktopControlSection adapter={adapter} />)

    expect(await screen.findByText('Screen Recording')).toBeVisible()
    expect(adapter.desktopControlStatus).toHaveBeenCalledWith({ refreshPermissionStatus: true })
    expect(adapter.desktopControlStatus).toHaveBeenCalledTimes(1)
    expect(screen.getByText('Required for screenshot, stream.')).toBeVisible()
    expect(screen.getByText(/quit and reopen Xero/)).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }))

    await waitFor(() =>
      expect(adapter.desktopControlStatus).toHaveBeenLastCalledWith({
        refreshPermissionStatus: true,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Open Screen Recording' }))

    await waitFor(() =>
      expect(adapter.desktopControlOpenPermissionSettings).toHaveBeenCalledWith({
        kind: 'open_macos_privacy_pane',
        target: 'Privacy_ScreenCapture',
      }),
    )
  })

  it('does not invent macOS settings actions for non-actionable permission rows', async () => {
    const adapter = makeAdapter({
      status: makeStatus({
        platform: 'linux',
        permissions: [
          {
            name: 'Remote Desktop Portal',
            status: 'unknown',
            requiredFor: ['wayland_capture', 'wayland_input'],
            remediation: 'Approve the portal prompt in the local desktop session.',
            action: null,
          },
        ],
      }),
    })

    render(<DesktopControlSection adapter={adapter} />)

    expect(await screen.findByText('Remote Desktop Portal')).toBeVisible()
    expect(screen.getByText('Required for wayland capture, wayland input.')).toBeVisible()
    expect(screen.queryByRole('button', { name: /Open/ })).not.toBeInTheDocument()
  })
})

function makeAdapter({ status }: { status: DesktopControlStatusDto }) {
  return {
    isDesktopRuntime: vi.fn(() => true),
    desktopControlStatus: vi.fn(async () => status),
    desktopControlUpdateSettings: vi.fn(
      async (request: UpsertDesktopControlSettingsRequestDto) => ({
        ...status,
        settings: {
          ...status.settings,
          ...request,
          updatedAt: '2026-05-26T12:01:00Z',
        },
      }),
    ),
    desktopControlStop: vi.fn(async () => status),
    desktopControlOpenPermissionSettings: vi.fn(async () => undefined),
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
      updatedAt: '2026-05-26T12:00:00Z',
    },
    auditLogPath: '/tmp/xero/desktop-control/audit.jsonl',
    updatedAt: '2026-05-26T12:00:00Z',
    ...overrides,
  }
}
