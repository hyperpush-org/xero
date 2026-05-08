import { describe, expect, it } from 'vitest'
import {
  bindingFromEvent,
  bindingsEqual,
  defaultBindings,
  eventMatchesBinding,
  formatBinding,
  isBindingEmpty,
  normalizeKey,
  type ShortcutBinding,
} from './shortcuts-definitions'

function makeEvent(init: Partial<KeyboardEventInit> & { key: string }): KeyboardEvent {
  return new KeyboardEvent('keydown', {
    key: init.key,
    metaKey: Boolean(init.metaKey),
    ctrlKey: Boolean(init.ctrlKey),
    shiftKey: Boolean(init.shiftKey),
    altKey: Boolean(init.altKey),
  })
}

describe('shortcuts-definitions', () => {
  it('defaults Cmd+1, Cmd+2, Cmd+3 to the three views', () => {
    const defaults = defaultBindings()
    expect(defaults['view.phases']).toEqual({ mod: true, shift: false, alt: false, key: '1' })
    expect(defaults['view.agent']).toEqual({ mod: true, shift: false, alt: false, key: '2' })
    expect(defaults['view.execution']).toEqual({ mod: true, shift: false, alt: false, key: '3' })
  })

  it('captures bindings from keyboard events with cross-platform mod', () => {
    const macEvent = makeEvent({ key: '1', metaKey: true })
    expect(bindingFromEvent(macEvent)).toEqual({
      mod: true,
      shift: false,
      alt: false,
      key: '1',
    })

    const winEvent = makeEvent({ key: '2', ctrlKey: true, shiftKey: true })
    expect(bindingFromEvent(winEvent)).toEqual({
      mod: true,
      shift: true,
      alt: false,
      key: '2',
    })
  })

  it('returns null for pure modifier keystrokes', () => {
    expect(bindingFromEvent(makeEvent({ key: 'Meta', metaKey: true }))).toBeNull()
    expect(bindingFromEvent(makeEvent({ key: 'Shift', shiftKey: true }))).toBeNull()
  })

  it('matches Cmd+1 on macOS but not Ctrl+1', () => {
    const binding: ShortcutBinding = { mod: true, shift: false, alt: false, key: '1' }
    expect(eventMatchesBinding(makeEvent({ key: '1', metaKey: true }), binding, 'macos')).toBe(true)
    expect(eventMatchesBinding(makeEvent({ key: '1', ctrlKey: true }), binding, 'macos')).toBe(false)
  })

  it('matches Ctrl+1 on Windows/Linux but not Cmd+1', () => {
    const binding: ShortcutBinding = { mod: true, shift: false, alt: false, key: '1' }
    expect(eventMatchesBinding(makeEvent({ key: '1', ctrlKey: true }), binding, 'other')).toBe(true)
    expect(eventMatchesBinding(makeEvent({ key: '1', metaKey: true }), binding, 'other')).toBe(false)
  })

  it('rejects events with extra modifiers the binding did not request', () => {
    const binding: ShortcutBinding = { mod: true, shift: false, alt: false, key: '1' }
    expect(
      eventMatchesBinding(makeEvent({ key: '1', metaKey: true, shiftKey: true }), binding, 'macos'),
    ).toBe(false)
    expect(
      eventMatchesBinding(makeEvent({ key: '1', metaKey: true, altKey: true }), binding, 'macos'),
    ).toBe(false)
  })

  it('treats letter case as insignificant', () => {
    const binding: ShortcutBinding = { mod: true, shift: false, alt: false, key: 'k' }
    expect(eventMatchesBinding(makeEvent({ key: 'K', metaKey: true }), binding, 'macos')).toBe(true)
    expect(normalizeKey('K')).toBe('k')
    expect(normalizeKey('ArrowUp')).toBe('ArrowUp')
  })

  it('formats bindings using platform conventions', () => {
    const binding: ShortcutBinding = { mod: true, shift: true, alt: false, key: '1' }
    expect(formatBinding(binding, 'macos')).toBe('⇧⌘1')
    expect(formatBinding(binding, 'other')).toBe('Ctrl+Shift+1')

    const empty: ShortcutBinding = { mod: false, shift: false, alt: false, key: '' }
    expect(formatBinding(empty, 'macos')).toBe('Unbound')
    expect(isBindingEmpty(empty)).toBe(true)
  })

  it('compares bindings independent of letter case', () => {
    expect(
      bindingsEqual(
        { mod: true, shift: false, alt: false, key: 'K' },
        { mod: true, shift: false, alt: false, key: 'k' },
      ),
    ).toBe(true)
  })
})
