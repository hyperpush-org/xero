import type { View } from '@/components/xero/data'

export const SHORTCUTS_STORAGE_KEY = 'xero.shortcuts.bindings.v1'

export type ShortcutId = 'view.phases' | 'view.agent' | 'view.execution'

export interface ShortcutBinding {
  /** "Mod" — Cmd on macOS, Ctrl on Windows/Linux. */
  mod: boolean
  shift: boolean
  alt: boolean
  /**
   * The KeyboardEvent.key value, normalized to lowercase for letters and the
   * literal digit / arrow / function-key name otherwise (e.g. "1", "k",
   * "ArrowUp", "F2"). Empty string means "unbound".
   */
  key: string
}

export interface ShortcutDefinition {
  id: ShortcutId
  label: string
  description: string
  category: ShortcutCategory
  defaultBinding: ShortcutBinding
  /** Optional view this shortcut switches to — used by the global listener. */
  view?: View
}

export type ShortcutCategory = 'Views'

export const SHORTCUT_CATEGORIES: ShortcutCategory[] = ['Views']

export const SHORTCUT_DEFINITIONS: ShortcutDefinition[] = [
  {
    id: 'view.phases',
    label: 'Switch to Workflow',
    description: 'Show the workflow / phases view.',
    category: 'Views',
    view: 'phases',
    defaultBinding: { mod: true, shift: false, alt: false, key: '1' },
  },
  {
    id: 'view.agent',
    label: 'Switch to Agent',
    description: 'Show the agent runtime view.',
    category: 'Views',
    view: 'agent',
    defaultBinding: { mod: true, shift: false, alt: false, key: '2' },
  },
  {
    id: 'view.execution',
    label: 'Switch to Editor',
    description: 'Show the editor / execution view.',
    category: 'Views',
    view: 'execution',
    defaultBinding: { mod: true, shift: false, alt: false, key: '3' },
  },
]

export function getShortcutDefinition(id: ShortcutId): ShortcutDefinition {
  const def = SHORTCUT_DEFINITIONS.find((entry) => entry.id === id)
  if (!def) {
    throw new Error(`Unknown shortcut id: ${id}`)
  }
  return def
}

export function isShortcutBinding(value: unknown): value is ShortcutBinding {
  if (!value || typeof value !== 'object') return false
  const v = value as Partial<ShortcutBinding>
  return (
    typeof v.mod === 'boolean' &&
    typeof v.shift === 'boolean' &&
    typeof v.alt === 'boolean' &&
    typeof v.key === 'string'
  )
}

export function bindingsEqual(a: ShortcutBinding, b: ShortcutBinding): boolean {
  return (
    a.mod === b.mod &&
    a.shift === b.shift &&
    a.alt === b.alt &&
    a.key.toLowerCase() === b.key.toLowerCase()
  )
}

export function isBindingEmpty(binding: ShortcutBinding): boolean {
  return binding.key.trim() === ''
}

/**
 * Normalize a `KeyboardEvent.key` value into the canonical form we store and
 * compare against. Letters become lowercase; everything else is preserved
 * verbatim so things like "ArrowUp", "F2", or "/" round-trip cleanly.
 */
export function normalizeKey(key: string): string {
  if (key.length === 1) {
    return key.toLowerCase()
  }
  return key
}

/**
 * Pretty-print a shortcut for the UI. macOS uses the symbol glyphs that the
 * platform expects; everywhere else uses spelled-out modifiers.
 */
export function formatBinding(
  binding: ShortcutBinding,
  platform: 'macos' | 'other',
): string {
  if (isBindingEmpty(binding)) return 'Unbound'

  const parts: string[] = []
  if (platform === 'macos') {
    if (binding.alt) parts.push('⌥')
    if (binding.shift) parts.push('⇧')
    if (binding.mod) parts.push('⌘')
  } else {
    if (binding.mod) parts.push('Ctrl')
    if (binding.shift) parts.push('Shift')
    if (binding.alt) parts.push('Alt')
  }

  parts.push(formatKeyLabel(binding.key))
  return platform === 'macos' ? parts.join('') : parts.join('+')
}

function formatKeyLabel(key: string): string {
  if (key.length === 1) return key.toUpperCase()
  switch (key) {
    case 'ArrowUp':
      return '↑'
    case 'ArrowDown':
      return '↓'
    case 'ArrowLeft':
      return '←'
    case 'ArrowRight':
      return '→'
    case ' ':
    case 'Space':
      return 'Space'
    default:
      return key
  }
}

/**
 * Capture a binding from a `KeyboardEvent`. Returns `null` if the event is
 * just a modifier press (no actual key yet).
 */
export function bindingFromEvent(event: KeyboardEvent): ShortcutBinding | null {
  const rawKey = event.key
  if (rawKey === 'Meta' || rawKey === 'Control' || rawKey === 'Shift' || rawKey === 'Alt') {
    return null
  }
  return {
    mod: event.metaKey || event.ctrlKey,
    shift: event.shiftKey,
    alt: event.altKey,
    key: normalizeKey(rawKey),
  }
}

/**
 * Test whether a `KeyboardEvent` matches a stored binding. Implements the
 * cross-platform "mod" semantics (Cmd on macOS, Ctrl elsewhere) and rejects
 * events with extra modifiers the binding didn't request.
 */
export function eventMatchesBinding(
  event: KeyboardEvent,
  binding: ShortcutBinding,
  platform: 'macos' | 'other',
): boolean {
  if (isBindingEmpty(binding)) return false

  const eventKey = normalizeKey(event.key)
  if (eventKey !== binding.key.toLowerCase()) return false

  const modPressed = platform === 'macos' ? event.metaKey : event.ctrlKey
  const otherModPressed = platform === 'macos' ? event.ctrlKey : event.metaKey
  if (binding.mod !== modPressed) return false
  if (otherModPressed) return false
  if (binding.shift !== event.shiftKey) return false
  if (binding.alt !== event.altKey) return false

  return true
}

export type ShortcutBindings = Record<ShortcutId, ShortcutBinding>

export function defaultBindings(): ShortcutBindings {
  const next = {} as ShortcutBindings
  for (const def of SHORTCUT_DEFINITIONS) {
    next[def.id] = { ...def.defaultBinding }
  }
  return next
}
