import { useEffect, useRef } from 'react'
import { detectPlatform } from '@/components/xero/shell'
import { eventMatchesBinding, type ShortcutId } from './shortcuts-definitions'
import { useShortcuts } from './shortcuts-provider'

export type ShortcutHandler = (id: ShortcutId, event: KeyboardEvent) => void

/**
 * Subscribe to global keydown events and fire `handler` when the event
 * matches any configured shortcut. The handler receives the matched shortcut
 * id so the caller can dispatch behavior. Bindings update reactively, but the
 * `handler` is wrapped in a ref so callers don't need to memoize it.
 */
export function useShortcutListener(handler: ShortcutHandler): void {
  const { bindings } = useShortcuts()
  const handlerRef = useRef(handler)

  useEffect(() => {
    handlerRef.current = handler
  }, [handler])

  useEffect(() => {
    if (typeof window === 'undefined') return
    const platform = detectPlatform() === 'macos' ? 'macos' : 'other'

    const onKeyDown = (event: KeyboardEvent) => {
      for (const [id, binding] of Object.entries(bindings)) {
        if (!eventMatchesBinding(event, binding, platform)) continue

        // If the binding has no modifier and the user is typing into an
        // editable surface, defer to normal text input. Modifier-driven
        // shortcuts always win — that matches platform conventions and
        // avoids surprising the user with unreachable bindings.
        const hasModifier = binding.mod || binding.alt
        if (!hasModifier) {
          const target = event.target as HTMLElement | null
          const tag = target?.tagName ?? ''
          if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || target?.isContentEditable) {
            continue
          }
        }

        event.preventDefault()
        handlerRef.current(id as ShortcutId, event)
        return
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [bindings])
}
