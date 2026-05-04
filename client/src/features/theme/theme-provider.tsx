import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react'
import {
  CUSTOM_THEMES_STORAGE_KEY,
  DEFAULT_THEME_ID,
  THEMES,
  THEME_STORAGE_KEY,
  getThemeById,
  themeClassName,
  themeColorsToCSSVars,
  type ThemeDefinition,
} from './theme-definitions'

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
    return parsed.filter(isValidThemeDefinition)
  } catch {
    return []
  }
}

function isValidThemeDefinition(value: unknown): value is ThemeDefinition {
  if (!value || typeof value !== 'object') return false
  const t = value as Partial<ThemeDefinition>
  return (
    typeof t.id === 'string' &&
    typeof t.name === 'string' &&
    typeof t.description === 'string' &&
    (t.appearance === 'dark' || t.appearance === 'light') &&
    typeof t.shiki === 'string' &&
    typeof t.colors === 'object' &&
    t.colors !== null &&
    typeof t.editor === 'object' &&
    t.editor !== null
  )
}

/**
 * Swap the `.theme-<id>` and `.dark` / `.light` classes on `<html>`, then
 * push every palette color onto the document root as inline CSS custom
 * properties. The class still drives first paint via `globals.css` for
 * built-in themes, but the inline vars are what make user-authored themes
 * possible at runtime — they take precedence over the stylesheet rules.
 */
function applyThemeToDocument(theme: ThemeDefinition, allThemeIds: string[]): void {
  if (typeof document === 'undefined') return
  const root = document.documentElement
  for (const id of allThemeIds) {
    root.classList.remove(themeClassName(id))
  }
  root.classList.add(themeClassName(theme.id))

  root.classList.remove('dark', 'light')
  root.classList.add(theme.appearance)
  root.style.colorScheme = theme.appearance
  root.dataset.theme = theme.id

  for (const [cssVar, value] of themeColorsToCSSVars(theme.colors)) {
    root.style.setProperty(cssVar, value)
  }
}

export interface ThemeProviderProps {
  children: ReactNode
  /** Optional override for tests — bypasses localStorage. */
  initialThemeId?: string
}

export function ThemeProvider({ children, initialThemeId }: ThemeProviderProps) {
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

  useLayoutEffect(() => {
    applyThemeToDocument(
      theme,
      allThemes.map((t) => t.id),
    )
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, theme.id)
    } catch {
      // Storage may be disabled (private mode, Tauri sandbox quirks) — the
      // theme still applies for the session, we just can't persist it.
    }
  }, [theme, allThemes])

  const persistCustomThemes = useCallback((next: ThemeDefinition[]) => {
    try {
      window.localStorage.setItem(CUSTOM_THEMES_STORAGE_KEY, JSON.stringify(next))
    } catch {
      // Same fallback story as the active theme id — best-effort persistence.
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
