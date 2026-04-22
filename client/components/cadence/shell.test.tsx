import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => false,
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: vi.fn(),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    startDragging: vi.fn(),
  }),
}))

import { CadenceShell } from './shell'

describe('CadenceShell', () => {
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
})
