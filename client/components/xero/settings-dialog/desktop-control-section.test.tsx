import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
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
  it('shows loading skeletons instead of fallback status while initial desktop status is pending', async () => {
    const status = makeStatus({
      permissions: [
        {
          name: 'Screen Recording',
          status: 'granted',
          requiredFor: ['screenshot', 'stream'],
          remediation: 'Screen capture permission is granted.',
          action: null,
        },
      ],
    })
    const pendingStatus = createDeferred<DesktopControlStatusDto>()
    const adapter = makeAdapter({ status })
    adapter.desktopControlStatus.mockImplementationOnce(async () => pendingStatus.promise)

    render(<DesktopControlSection adapter={adapter} />)

    expect(
      screen.getByRole('status', { name: 'Loading desktop-control status' }),
    ).toBeVisible()
    expect(screen.getByRole('button', { name: 'Refresh' })).toBeDisabled()
    expect(screen.queryByText('unavailable')).not.toBeInTheDocument()
    expect(screen.queryByText('idle · unavailable')).not.toBeInTheDocument()
    expect(screen.queryByRole('switch', { name: 'Allow cloud viewing' })).not.toBeInTheDocument()
    expect(screen.queryByText('Screen Recording')).not.toBeInTheDocument()

    await act(async () => {
      pendingStatus.resolve(status)
      await pendingStatus.promise
    })

    expect(await screen.findByText('ready')).toBeVisible()
    expect(screen.getByRole('switch', { name: 'Allow cloud viewing' })).toBeChecked()
    expect(screen.getByText('Screen Recording')).toBeVisible()
  })

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

  it('enables owner-admin mode with a bounded local duration and revokes it on stop', async () => {
    const activeUntil = new Date(Date.now() + 15 * 60 * 1000).toISOString()
    const status = makeStatus()
    const adapter = makeAdapter({
      status,
      updateStatus: makeStatus({
        settings: {
          ...status.settings,
          policyProfile: 'owner_admin',
          ownerAdminExpiresAt: activeUntil,
          updatedAt: '2026-05-26T12:01:00Z',
        },
      }),
      stopStatus: makeStatus({
        settings: {
          ...status.settings,
          policyProfile: 'default_safe',
          ownerAdminExpiresAt: null,
          updatedAt: '2026-05-26T12:02:00Z',
        },
      }),
    })

    render(<DesktopControlSection adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Owner Admin' }))

    await waitFor(() =>
      expect(adapter.desktopControlUpdateSettings).toHaveBeenCalledWith({
        cloudStreamingEnabled: true,
        manualCloudControlEnabled: true,
        policyProfile: 'owner_admin',
        ownerAdminDurationMinutes: 30,
      }),
    )
    expect(await screen.findByText('owner admin')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Stop' }))

    await waitFor(() => expect(adapter.desktopControlStop).toHaveBeenCalled())
  })
})

function makeAdapter({
  status,
  updateStatus,
  stopStatus,
}: {
  status: DesktopControlStatusDto
  updateStatus?: DesktopControlStatusDto
  stopStatus?: DesktopControlStatusDto
}) {
  return {
    isDesktopRuntime: vi.fn(() => true),
    desktopControlStatus: vi.fn(async () => status),
    desktopControlUpdateSettings: vi.fn(
      async (request: UpsertDesktopControlSettingsRequestDto) =>
        updateStatus ?? {
        ...status,
        settings: {
          ...status.settings,
          ...request,
          ownerAdminExpiresAt:
            request.policyProfile === 'owner_admin' ? status.settings.ownerAdminExpiresAt : null,
          updatedAt: '2026-05-26T12:01:00Z',
        },
      },
    ),
    desktopControlStop: vi.fn(async () => stopStatus ?? status),
    desktopControlOpenPermissionSettings: vi.fn(async () => undefined),
  }
}

function createDeferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((promiseResolve) => {
    resolve = promiseResolve
  })

  return { promise, resolve }
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
