import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { XeroDesktopAdapter, type BridgeThemeSyncRequestDto } from '@/src/lib/xero-desktop'
import {
  CUSTOM_THEMES_STORAGE_KEY,
  DEFAULT_THEME_ID,
  THEMES,
  THEME_STORAGE_KEY,
  applyThemeToDocument,
  getThemeById,
  isCustomThemeId,
  isThemeDefinition,
  type ThemeDefinition,
} from '@xero/ui/theme'
import { syncThemeDockIcon } from './dock-icon'

interface ThemeContextValue {
  themes: ThemeDefinition[]
  customThemes: ThemeDefinition[]
  theme: ThemeDefinition
  themeId: string
  setThemeId: (id: string) => void
  saveCustomTheme: (theme: ThemeDefinition) => void
  deleteCustomTheme: (id: string) => void
}

const ThemeContext = createContext<ThemeContextValue | null>(null)
const THEME_APP_STATE_KEY = 'theme.active.v1'
const CUSTOM_THEMES_APP_STATE_KEY = 'theme.custom.v1'

/**
 * Read the stored theme id synchronously. Safe in the browser only — callers
 * must guard for SSR environments, but Xero is desktop-only so we assume a
 * DOM is available after mount.
 */
function readStoredThemeId(): string {
  if (typeof window === 'undefined') return DEFAULT_THEME_ID
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY)
    if (!stored) return DEFAULT_THEME_ID
    return stored
  } catch {
    return DEFAULT_THEME_ID
  }
}

/**
 * Load user-authored themes from localStorage. Tolerant of partially-shaped
 * payloads — anything that doesn't look like a complete theme is dropped
 * silently rather than crashing the app on first paint.
 */
function readStoredCustomThemes(): ThemeDefinition[] {
  if (typeof window === 'undefined') return []
  try {
    const raw = window.localStorage.getItem(CUSTOM_THEMES_STORAGE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return []
    return parsed.filter(isThemeDefinition)
  } catch {
    return []
  }
}

function appStateThemeId(value: unknown): string | null {
  return typeof value === 'string' && value.trim().length > 0 ? value : null
}

function appStateCustomThemes(value: unknown): ThemeDefinition[] {
  if (!Array.isArray(value)) return []
  return value.filter(isThemeDefinition)
}

function themeSyncRequest(theme: ThemeDefinition): BridgeThemeSyncRequestDto {
  const request: BridgeThemeSyncRequestDto = { themeId: theme.id }
  if (isCustomThemeId(theme.id)) {
    request.customTheme = theme
  }
  return request
}

export interface ThemeProviderProps {
  children: ReactNode
  /** Optional override for tests — bypasses localStorage. */
  initialThemeId?: string
}

export function ThemeProvider({ children, initialThemeId }: ThemeProviderProps) {
  const appStateHydratedRef = useRef(Boolean(initialThemeId))
  const [appStateHydrated, setAppStateHydrated] = useState(Boolean(initialThemeId))
  const [customThemes, setCustomThemes] = useState<ThemeDefinition[]>(() =>
    initialThemeId ? [] : readStoredCustomThemes(),
  )

  const allThemes = useMemo(() => [...THEMES, ...customThemes], [customThemes])

  const [themeId, setThemeIdState] = useState<string>(() => {
    if (initialThemeId) return getThemeById(initialThemeId).id
    const stored = readStoredThemeId()
    return getThemeById(stored, [...THEMES, ...readStoredCustomThemes()]).id
  })

  const theme = useMemo(() => getThemeById(themeId, allThemes), [themeId, allThemes])

  useEffect(() => {
    if (initialThemeId) return
    const readAppUiState = XeroDesktopAdapter.readAppUiState
    if (typeof readAppUiState !== 'function') {
      appStateHydratedRef.current = true
      setAppStateHydrated(true)
      return
    }

    let disposed = false
    void Promise.all([
      readAppUiState({ key: THEME_APP_STATE_KEY }),
      readAppUiState({ key: CUSTOM_THEMES_APP_STATE_KEY }),
    ])
      .then(([themeResponse, customThemeResponse]) => {
        if (disposed) return
        const nextCustomThemes = appStateCustomThemes(customThemeResponse.value)
        if (nextCustomThemes.length > 0 || customThemeResponse.value != null) {
          setCustomThemes(nextCustomThemes)
        }
        const nextThemeId = appStateThemeId(themeResponse.value)
        if (nextThemeId) {
          setThemeIdState(getThemeById(nextThemeId, [...THEMES, ...nextCustomThemes]).id)
        }
      })
      .catch(() => undefined)
      .finally(() => {
        if (disposed) return
        appStateHydratedRef.current = true
        setAppStateHydrated(true)
      })

    return () => {
      disposed = true
    }
  }, [initialThemeId])

  useLayoutEffect(() => {
    applyThemeToDocument(theme)
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, theme.id)
    } catch {
      // Storage may be disabled (private mode, Tauri sandbox quirks) — the
      // theme still applies for the session, we just can't persist it.
    }
    if (appStateHydrated) {
      void XeroDesktopAdapter.writeAppUiState?.({
        key: THEME_APP_STATE_KEY,
        value: theme.id,
      }).catch(() => undefined)
      if (XeroDesktopAdapter.isDesktopRuntime()) {
        void XeroDesktopAdapter.publishThemeToCloud?.(themeSyncRequest(theme)).catch(
          () => undefined,
        )
      }
    }
  }, [theme, appStateHydrated])

  useEffect(() => {
    void syncThemeDockIcon(theme)
  }, [theme])

  const persistCustomThemes = useCallback((next: ThemeDefinition[]) => {
    try {
      window.localStorage.setItem(CUSTOM_THEMES_STORAGE_KEY, JSON.stringify(next))
    } catch {
      // Same fallback story as the active theme id — best-effort persistence.
    }
    if (appStateHydratedRef.current) {
      void XeroDesktopAdapter.writeAppUiState?.({
        key: CUSTOM_THEMES_APP_STATE_KEY,
        value: next,
      }).catch(() => undefined)
    }
  }, [])

  const setThemeId = useCallback(
    (id: string) => {
      setThemeIdState(getThemeById(id, allThemes).id)
    },
    [allThemes],
  )

  const saveCustomTheme = useCallback(
    (next: ThemeDefinition) => {
      setCustomThemes((prev) => {
        const idx = prev.findIndex((t) => t.id === next.id)
        const updated = idx >= 0 ? prev.map((t, i) => (i === idx ? next : t)) : [...prev, next]
        persistCustomThemes(updated)
        return updated
      })
    },
    [persistCustomThemes],
  )

  const deleteCustomTheme = useCallback(
    (id: string) => {
      setCustomThemes((prev) => {
        const updated = prev.filter((t) => t.id !== id)
        persistCustomThemes(updated)
        return updated
      })
      setThemeIdState((current) => (current === id ? DEFAULT_THEME_ID : current))
    },
    [persistCustomThemes],
  )

  const value = useMemo<ThemeContextValue>(
    () => ({
      themes: allThemes,
      customThemes,
      theme,
      themeId: theme.id,
      setThemeId,
      saveCustomTheme,
      deleteCustomTheme,
    }),
    [allThemes, customThemes, theme, setThemeId, saveCustomTheme, deleteCustomTheme],
  )

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}

/**
 * Fallback context returned when no provider is mounted. Tests that render a
 * single component (e.g. `CodeEditor`) shouldn't be forced to wrap with a
 * provider — they just see the default theme and a no-op setter. The real
 * app always mounts `ThemeProvider` at the root so this branch is never hit
 * in production.
 */
const FALLBACK_CONTEXT: ThemeContextValue = {
  themes: THEMES,
  customThemes: [],
  theme: THEMES[0],
  themeId: THEMES[0].id,
  setThemeId: () => {},
  saveCustomTheme: () => {},
  deleteCustomTheme: () => {},
}

export function useTheme(): ThemeContextValue {
  return useContext(ThemeContext) ?? FALLBACK_CONTEXT
}
