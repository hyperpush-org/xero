import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { THEMES, type ThemeDefinition } from '@xero/ui/theme'
import { XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import { ThemeProvider, useTheme } from './theme-provider'

const tauriInvokeMock = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/core', () => ({
  invoke: tauriInvokeMock,
  isTauri: () => true,
}))

const originalAdapter = {
  isDesktopRuntime: XeroDesktopAdapter.isDesktopRuntime,
  publishThemeToCloud: XeroDesktopAdapter.publishThemeToCloud,
  readAppUiState: XeroDesktopAdapter.readAppUiState,
  writeAppUiState: XeroDesktopAdapter.writeAppUiState,
}

afterEach(() => {
  tauriInvokeMock.mockClear()
  window.localStorage.clear()
  document.documentElement.removeAttribute('class')
  document.documentElement.removeAttribute('style')
  document.documentElement.removeAttribute('data-theme')
  XeroDesktopAdapter.isDesktopRuntime = originalAdapter.isDesktopRuntime
  XeroDesktopAdapter.publishThemeToCloud = originalAdapter.publishThemeToCloud
  XeroDesktopAdapter.readAppUiState = originalAdapter.readAppUiState
  XeroDesktopAdapter.writeAppUiState = originalAdapter.writeAppUiState
})

function ThemeSwitchButton() {
  const { setThemeId } = useTheme()
  return <button onClick={() => setThemeId('midnight')}>Use midnight</button>
}

describe('ThemeProvider cloud sync', () => {
  it('publishes only the theme id for built-in themes', async () => {
    const publishThemeToCloud = vi.fn(async () => undefined)
    XeroDesktopAdapter.isDesktopRuntime = () => true
    XeroDesktopAdapter.publishThemeToCloud = publishThemeToCloud
    XeroDesktopAdapter.writeAppUiState = vi.fn(async ({ key, value }) => ({
      schema: 'xero.app_ui_state.v1' as const,
      key,
      value,
      storageScope: 'os_app_data' as const,
      uiDeferred: false,
    }))

    render(
      <ThemeProvider initialThemeId="midnight">
        <div />
      </ThemeProvider>,
    )

    await waitFor(() => {
      expect(publishThemeToCloud).toHaveBeenCalledWith({ themeId: 'midnight' })
    })
  })

  it('publishes custom theme tokens only when the active theme is custom', async () => {
    const publishThemeToCloud = vi.fn(async () => undefined)
    const customTheme: ThemeDefinition = {
      ...THEMES[0],
      id: 'custom-ember',
      name: 'Ember',
      colors: {
        ...THEMES[0].colors,
        background: '#fff1e8',
        primary: '#b7431d',
      },
    }
    XeroDesktopAdapter.isDesktopRuntime = () => true
    XeroDesktopAdapter.publishThemeToCloud = publishThemeToCloud
    XeroDesktopAdapter.readAppUiState = vi.fn(async ({ key }) => ({
      schema: 'xero.app_ui_state.v1' as const,
      key,
      value: key === 'theme.active.v1' ? customTheme.id : [customTheme],
      storageScope: 'os_app_data' as const,
      uiDeferred: false,
    }))
    XeroDesktopAdapter.writeAppUiState = vi.fn(async ({ key, value }) => ({
      schema: 'xero.app_ui_state.v1' as const,
      key,
      value,
      storageScope: 'os_app_data' as const,
      uiDeferred: false,
    }))

    render(
      <ThemeProvider>
        <div />
      </ThemeProvider>,
    )

    await waitFor(() => {
      expect(publishThemeToCloud).toHaveBeenCalledWith({
        themeId: 'custom-ember',
        customTheme,
      })
    })
  })

  it('does not invoke runtime OS app icon updates on initial paint or theme changes', async () => {
    const publishThemeToCloud = vi.fn(async () => undefined)
    XeroDesktopAdapter.isDesktopRuntime = () => true
    XeroDesktopAdapter.publishThemeToCloud = publishThemeToCloud
    XeroDesktopAdapter.writeAppUiState = vi.fn(async ({ key, value }) => ({
      schema: 'xero.app_ui_state.v1' as const,
      key,
      value,
      storageScope: 'os_app_data' as const,
      uiDeferred: false,
    }))

    render(
      <ThemeProvider initialThemeId="dusk">
        <ThemeSwitchButton />
      </ThemeProvider>,
    )

    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe('dusk')
    })
    expect(tauriInvokeMock).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Use midnight' }))

    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe('midnight')
    })
    expect(tauriInvokeMock).not.toHaveBeenCalled()
  })
})
