import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const stylesPath = resolve(dirname(fileURLToPath(import.meta.url)), 'styles.css')
const sharedStylesPath = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '../../packages/ui/src/styles.css',
)

describe('client stylesheet', () => {
  it('scans shared UI component classes for Tailwind utilities', () => {
    const styles = readFileSync(stylesPath, 'utf8')

    expect(styles).toContain('@source "../../packages/ui/src";')
  })

  it('defines a concrete exit animation for full-window loading screens', () => {
    const styles = readFileSync(sharedStylesPath, 'utf8')

    expect(styles).toContain(".xero-loading-screen[data-state='closed']")
    expect(styles).toContain('xero-loading-screen-exit 160ms')
    expect(styles).toContain('@keyframes xero-loading-symbol-exit')
  })

  it('keeps shared scrollbars above elevated app chrome', () => {
    const styles = readFileSync(sharedStylesPath, 'utf8')

    expect(styles).toContain('--scrollbar-overlay-gutter: 12px;')
    expect(styles).toContain('--scrollbar-z-index: 2147483647;')
    expect(styles).toMatch(/::-webkit-scrollbar\s*\{[^}]*z-index:\s*var\(--scrollbar-z-index\)/)
    expect(styles).toMatch(/::-webkit-scrollbar-thumb\s*\{[^}]*z-index:\s*var\(--scrollbar-z-index\)/)
    expect(styles).toMatch(/::-webkit-scrollbar-corner\s*\{[^}]*z-index:\s*var\(--scrollbar-z-index\)/)
  })

  it('animates newly loaded agent session surfaces', () => {
    const styles = readFileSync(sharedStylesPath, 'utf8')

    expect(styles).toContain('.agent-session-surface-enter')
    expect(styles).toContain('.agent-workspace-pane-enter')
    expect(styles).toContain('@keyframes xero-agent-session-surface-enter')
    expect(styles).toContain('translate3d(0, 6px, 0)')
  })
})
