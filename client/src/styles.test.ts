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
})
