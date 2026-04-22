import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { isTauriMock, tauriWindowMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(() => false),
  tauriWindowMock: {
    close: vi.fn(),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    startDragging: vi.fn(),
  },
}))

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: isTauriMock,
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => tauriWindowMock,
}))

import { CadenceShell } from './shell'

describe('CadenceShell', () => {
  beforeEach(() => {
    isTauriMock.mockReturnValue(false)
    tauriWindowMock.close.mockReset()
    tauriWindowMock.minimize.mockReset()
    tauriWindowMock.toggleMaximize.mockReset()
    tauriWindowMock.startDragging.mockReset()
  })

  it.each(['macos', 'windows'] as const)('renders the sidebar toggle in the %s titlebar', (platform) => {
    const onToggleSidebar = vi.fn()

    render(
      <CadenceShell
        activeView="phases"
        onToggleSidebar={onToggleSidebar}
        onViewChange={() => undefined}
        platformOverride={platform}
      >
        <div>Body</div>
      </CadenceShell>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Collapse project sidebar' }))

    expect(onToggleSidebar).toHaveBeenCalledTimes(1)
    expect(screen.getByRole('navigation')).toBeVisible()
  })

  it.each(['macos', 'windows'] as const)('toggles the arcade from the %s titlebar', (platform) => {
    const onToggleGames = vi.fn()

    const { rerender } = render(
      <CadenceShell
        activeView="phases"
        onToggleGames={onToggleGames}
        onViewChange={() => undefined}
        platformOverride={platform}
      >
        <div>Body</div>
      </CadenceShell>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Open arcade' }))
    expect(onToggleGames).toHaveBeenCalledTimes(1)

    rerender(
      <CadenceShell
        activeView="phases"
        gamesOpen
        onToggleGames={onToggleGames}
        onViewChange={() => undefined}
        platformOverride={platform}
      >
        <div>Body</div>
      </CadenceShell>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Close arcade' }))
    expect(onToggleGames).toHaveBeenCalledTimes(2)
  })

  it.each(['macos', 'windows'] as const)('keeps titlebar controls out of the drag strip in %s', (platform) => {
    isTauriMock.mockReturnValue(true)

    const { container } = render(
      <CadenceShell activeView="phases" onOpenSettings={() => undefined} onViewChange={() => undefined} platformOverride={platform}>
        <div>Body</div>
      </CadenceShell>,
    )

    const header = container.querySelector('header')
    expect(header).not.toHaveAttribute('data-tauri-drag-region')

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Settings' }), { button: 0, detail: 2 })

    expect(tauriWindowMock.toggleMaximize).not.toHaveBeenCalled()
    expect(tauriWindowMock.startDragging).not.toHaveBeenCalled()
  })

  it.each(['macos', 'windows'] as const)('preserves drag strip gestures in %s', async (platform) => {
    isTauriMock.mockReturnValue(true)

    const { container } = render(
      <CadenceShell activeView="phases" onViewChange={() => undefined} platformOverride={platform}>
        <div>Body</div>
      </CadenceShell>,
    )

    const dragRegion = container.querySelector('[data-tauri-drag-region]')
    expect(dragRegion).toBeInstanceOf(HTMLElement)

    fireEvent.mouseDown(dragRegion as HTMLElement, { button: 0, detail: 1 })
    await waitFor(() => expect(tauriWindowMock.startDragging).toHaveBeenCalledTimes(1))

    fireEvent.mouseDown(dragRegion as HTMLElement, { button: 0, detail: 2 })
    await waitFor(() => expect(tauriWindowMock.toggleMaximize).toHaveBeenCalledTimes(1))
  })
})
