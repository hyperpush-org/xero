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
  SHORTCUTS_STORAGE_KEY,
  SHORTCUT_DEFINITIONS,
  defaultBindings,
  getShortcutDefinition,
  isShortcutBinding,
  type ShortcutBinding,
  type ShortcutBindings,
  type ShortcutId,
} from './shortcuts-definitions'

interface ShortcutsContextValue {
  bindings: ShortcutBindings
  setBinding: (id: ShortcutId, binding: ShortcutBinding) => void
  resetBinding: (id: ShortcutId) => void
  resetAll: () => void
}

const ShortcutsContext = createContext<ShortcutsContextValue | null>(null)

function readStoredBindings(): ShortcutBindings {
  const fallback = defaultBindings()
  if (typeof window === 'undefined') return fallback
  try {
    const raw = window.localStorage.getItem(SHORTCUTS_STORAGE_KEY)
    if (!raw) return fallback
    const parsed = JSON.parse(raw) as unknown
    if (!parsed || typeof parsed !== 'object') return fallback
    const merged = { ...fallback }
    for (const def of SHORTCUT_DEFINITIONS) {
      const candidate = (parsed as Record<string, unknown>)[def.id]
      if (isShortcutBinding(candidate)) {
        merged[def.id] = candidate
      }
    }
    return merged
  } catch {
    return fallback
  }
}

function persistBindings(bindings: ShortcutBindings): void {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(SHORTCUTS_STORAGE_KEY, JSON.stringify(bindings))
  } catch {
    // Best-effort persistence — storage may be disabled in some sandboxes.
  }
}

export interface ShortcutsProviderProps {
  children: ReactNode
  /** Optional override for tests — bypasses localStorage. */
  initialBindings?: ShortcutBindings
}

export function ShortcutsProvider({ children, initialBindings }: ShortcutsProviderProps) {
  const [bindings, setBindings] = useState<ShortcutBindings>(
    () => initialBindings ?? readStoredBindings(),
  )

  useEffect(() => {
    if (initialBindings) return
    persistBindings(bindings)
  }, [bindings, initialBindings])

  const setBinding = useCallback((id: ShortcutId, binding: ShortcutBinding) => {
    setBindings((prev) => ({ ...prev, [id]: binding }))
  }, [])

  const resetBinding = useCallback((id: ShortcutId) => {
    setBindings((prev) => ({ ...prev, [id]: { ...getShortcutDefinition(id).defaultBinding } }))
  }, [])

  const resetAll = useCallback(() => {
    setBindings(defaultBindings())
  }, [])

  const value = useMemo<ShortcutsContextValue>(
    () => ({ bindings, setBinding, resetBinding, resetAll }),
    [bindings, setBinding, resetBinding, resetAll],
  )

  return <ShortcutsContext.Provider value={value}>{children}</ShortcutsContext.Provider>
}

const FALLBACK_CONTEXT: ShortcutsContextValue = {
  bindings: defaultBindings(),
  setBinding: () => {},
  resetBinding: () => {},
  resetAll: () => {},
}

export function useShortcuts(): ShortcutsContextValue {
  return useContext(ShortcutsContext) ?? FALLBACK_CONTEXT
}
