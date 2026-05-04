import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { isTauriMock, tauriWindowMock, invokeMock, listenMock, openUrlMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(() => false),
  tauriWindowMock: {
    close: vi.fn(),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    startDragging: vi.fn(),
  },
  invokeMock: vi.fn(async () => ({
    android: { present: false },
    ios: { present: false, supported: false },
  })),
  listenMock: vi.fn(async () => () => undefined),
  openUrlMock: vi.fn(async () => undefined),
}))

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: isTauriMock,
  invoke: invokeMock,
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => tauriWindowMock,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: listenMock,
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: openUrlMock,
}))

import { XeroShell } from './shell'

describe('XeroShell', () => {
  beforeEach(() => {
    isTauriMock.mockReturnValue(false)
    tauriWindowMock.close.mockReset()
    tauriWindowMock.minimize.mockReset()
    tauriWindowMock.toggleMaximize.mockReset()
    tauriWindowMock.startDragging.mockReset()
    invokeMock.mockReset()
    invokeMock.mockResolvedValue({
      android: { present: false },
      ios: { present: false, supported: false },
    })
    listenMock.mockReset()
    listenMock.mockResolvedValue(() => undefined)
    openUrlMock.mockReset()
    openUrlMock.mockResolvedValue(undefined)
  })

  it.each(['macos', 'windows'] as const)('renders the sidebar toggle in the %s titlebar', (platform) => {
    const onToggleSidebar = vi.fn()

    render(
      <XeroShell
        activeView="phases"
        onToggleSidebar={onToggleSidebar}
        onViewChange={() => undefined}
        platformOverride={platform}
      >
        <div>Body</div>
      </XeroShell>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Collapse project sidebar' }))

    expect(onToggleSidebar).toHaveBeenCalledTimes(1)
    expect(screen.getByRole('navigation')).toBeVisible()
  })

  it('places macOS tabs in the left titlebar slot and centers the logo', () => {
    render(
      <XeroShell
        activeView="phases"
        onViewChange={() => undefined}
        platformOverride="macos"
      >
        <div>Body</div>
      </XeroShell>,
    )

    const nav = screen.getByRole('navigation')
    const logo = screen.getByText('Xero')
    expect(nav.compareDocumentPosition(logo) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(logo.parentElement?.parentElement).toHaveClass('absolute', 'left-1/2')
  })

  it.each(['macos', 'windows'] as const)('toggles the arcade from the %s titlebar', (platform) => {
    const onToggleGames = vi.fn()

    const { rerender } = render(
      <XeroShell
        activeView="phases"
        onToggleGames={onToggleGames}
        onViewChange={() => undefined}
        platformOverride={platform}
      >
        <div>Body</div>
      </XeroShell>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Open arcade' }))
    expect(onToggleGames).toHaveBeenCalledTimes(1)

    rerender(
      <XeroShell
        activeView="phases"
        gamesOpen
        onToggleGames={onToggleGames}
        onViewChange={() => undefined}
        platformOverride={platform}
      >
        <div>Body</div>
      </XeroShell>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Close arcade' }))
    expect(onToggleGames).toHaveBeenCalledTimes(2)
  })

  it.each(['macos', 'windows'] as const)('toggles the Android emulator from the %s tools menu', async (platform) => {
    const onToggleAndroid = vi.fn()

    const { rerender } = render(
      <XeroShell
        activeView="phases"
        onToggleAndroid={onToggleAndroid}
        onViewChange={() => undefined}
        platformOverride={platform}
      >
        <div>Body</div>
      </XeroShell>,
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Tools' }), { button: 0, ctrlKey: false })
    fireEvent.click(screen.getByRole('menuitem', { name: 'Open Android emulator' }))
    await waitFor(() => expect(onToggleAndroid).toHaveBeenCalledTimes(1))

    rerender(
      <XeroShell
        activeView="phases"
        androidOpen
        onToggleAndroid={onToggleAndroid}
        onViewChange={() => undefined}
        platformOverride={platform}
      >
        <div>Body</div>
      </XeroShell>,
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Tools' }), { button: 0, ctrlKey: false })
    fireEvent.click(screen.getByRole('menuitem', { name: 'Close Android emulator' }))
    await waitFor(() => expect(onToggleAndroid).toHaveBeenCalledTimes(2))
  })

  it('flips the iOS menu item to an Install Xcode CTA when Xcode is missing', async () => {
    isTauriMock.mockReturnValue(true)
    invokeMock.mockResolvedValue({
      android: { present: true },
      ios: { present: false, supported: true },
    })
    const onToggleIos = vi.fn()

    render(
      <XeroShell
        activeView="phases"
        onToggleIos={onToggleIos}
        onViewChange={() => undefined}
        platformOverride="macos"
      >
        <div>Body</div>
      </XeroShell>,
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Tools' }), { button: 0, ctrlKey: false })
    const ctaItem = await screen.findByRole('menuitem', { name: 'Install Xcode' })
    fireEvent.click(ctaItem)
    await waitFor(() =>
      expect(openUrlMock).toHaveBeenCalledWith('https://apps.apple.com/app/xcode/id497799835'),
    )
    // Clicking the CTA never toggles the iOS sidebar — opening an
    // empty panel would just repeat the same "Install Xcode" message.
    expect(onToggleIos).not.toHaveBeenCalled()
    expect(screen.queryByRole('menuitem', { name: /Open iOS simulator/ })).toBeNull()
  })

  it('surfaces Android SDK setup context from the tools menu', async () => {
    isTauriMock.mockReturnValue(true)
    invokeMock.mockResolvedValue({
      android: { present: false },
      ios: { present: true, supported: true },
    })

    render(
      <XeroShell
        activeView="phases"
        onToggleAndroid={vi.fn()}
        onViewChange={() => undefined}
        platformOverride="macos"
      >
        <div>Body</div>
      </XeroShell>,
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Tools' }), { button: 0, ctrlKey: false })
    const item = await screen.findByRole('menuitem', { name: 'Open Android emulator' })
    await waitFor(() =>
      expect(item.getAttribute('title')).toMatch(/Android SDK not installed/),
    )
  })

  it('renders the iOS menu item only on macOS', async () => {
    const onToggleIos = vi.fn()

    const { rerender } = render(
      <XeroShell
        activeView="phases"
        onToggleIos={onToggleIos}
        onViewChange={() => undefined}
        platformOverride="macos"
      >
        <div>Body</div>
      </XeroShell>,
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Tools' }), { button: 0, ctrlKey: false })
    expect(screen.getByRole('menuitem', { name: 'Open iOS simulator' })).toBeVisible()
    fireEvent.click(screen.getByRole('menuitem', { name: 'Open iOS simulator' }))
    await waitFor(() => expect(onToggleIos).toHaveBeenCalledTimes(1))

    for (const platform of ['windows', 'linux'] as const) {
      rerender(
        <XeroShell
          activeView="phases"
          onToggleIos={onToggleIos}
          onViewChange={() => undefined}
          platformOverride={platform}
        >
          <div>Body</div>
        </XeroShell>,
      )
      expect(screen.queryByRole('menuitem', { name: /iOS simulator/ })).toBeNull()
    }
  })

  it.each(['macos', 'windows'] as const)('keeps titlebar controls out of the drag strip in %s', (platform) => {
    isTauriMock.mockReturnValue(true)
    invokeMock.mockReturnValue(new Promise(() => undefined))

    const { container } = render(
      <XeroShell activeView="phases" onOpenSettings={() => undefined} onViewChange={() => undefined} platformOverride={platform}>
        <div>Body</div>
      </XeroShell>,
    )

    const header = container.querySelector('header')
    expect(header).not.toHaveAttribute('data-tauri-drag-region')

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Settings' }), { button: 0, detail: 2 })

    expect(tauriWindowMock.toggleMaximize).not.toHaveBeenCalled()
    expect(tauriWindowMock.startDragging).not.toHaveBeenCalled()
  })

  it.each(['macos', 'windows'] as const)('preserves drag strip gestures in %s', async (platform) => {
    isTauriMock.mockReturnValue(true)
    invokeMock.mockReturnValue(new Promise(() => undefined))

    const { container } = render(
      <XeroShell activeView="phases" onViewChange={() => undefined} platformOverride={platform}>
        <div>Body</div>
      </XeroShell>,
    )

    const dragRegion = container.querySelector('[data-tauri-drag-region]')
    expect(dragRegion).toBeInstanceOf(HTMLElement)

    fireEvent.mouseDown(dragRegion as HTMLElement, { button: 0, detail: 1 })
    await waitFor(() => expect(tauriWindowMock.startDragging).toHaveBeenCalledTimes(1))

    fireEvent.mouseDown(dragRegion as HTMLElement, { button: 0, detail: 2 })
    await waitFor(() => expect(tauriWindowMock.toggleMaximize).toHaveBeenCalledTimes(1))
  })
})
