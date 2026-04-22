import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react'
import {
  DEFAULT_THEME_ID,
  THEMES,
  THEME_STORAGE_KEY,
  getThemeById,
  themeClassName,
  type ThemeDefinition,
} from './theme-definitions'

interface ThemeContextValue {
  themes: ThemeDefinition[]
  theme: ThemeDefinition
  themeId: string
  setThemeId: (id: string) => void
}

const ThemeContext = createContext<ThemeContextValue | null>(null)

/**
 * Read the stored theme id synchronously. Safe in the browser only — callers
 * must guard for SSR environments, but Cadence is desktop-only so we assume a
 * DOM is available after mount.
 */
function readStoredThemeId(): string {
  if (typeof window === 'undefined') return DEFAULT_THEME_ID
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY)
    if (!stored) return DEFAULT_THEME_ID
    return getThemeById(stored).id
  } catch {
    return DEFAULT_THEME_ID
  }
}

/**
 * Swap the `.theme-<id>` and `.dark` / `.light` classes on `<html>` so that
 * `globals.css` picks up the correct palette. Idempotent: only the current
 * theme class remains after each call.
 */
function applyThemeToDocument(theme: ThemeDefinition): void {
  if (typeof document === 'undefined') return
  const root = document.documentElement
  const knownClasses = THEMES.map((t) => themeClassName(t.id))
  for (const cls of knownClasses) {
    root.classList.remove(cls)
  }
  root.classList.add(themeClassName(theme.id))

  root.classList.remove('dark', 'light')
  root.classList.add(theme.appearance)
  root.style.colorScheme = theme.appearance
  root.dataset.theme = theme.id
}

export interface ThemeProviderProps {
  children: ReactNode
  /** Optional override for tests — bypasses localStorage. */
  initialThemeId?: string
}

export function ThemeProvider({ children, initialThemeId }: ThemeProviderProps) {
  const [themeId, setThemeIdState] = useState<string>(() => {
    if (initialThemeId) return getThemeById(initialThemeId).id
    return readStoredThemeId()
  })

  const theme = useMemo(() => getThemeById(themeId), [themeId])

  useEffect(() => {
    applyThemeToDocument(theme)
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, theme.id)
    } catch {
      // Storage may be disabled (private mode, Tauri sandbox quirks) — the
      // theme still applies for the session, we just can't persist it.
    }
  }, [theme])

  const setThemeId = useCallback((id: string) => {
    setThemeIdState(getThemeById(id).id)
  }, [])

  const value = useMemo<ThemeContextValue>(
    () => ({ themes: THEMES, theme, themeId: theme.id, setThemeId }),
    [theme, setThemeId],
  )

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext)
  if (!ctx) {
    throw new Error('useTheme must be used within a <ThemeProvider>')
  }
  return ctx
}
