import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const scrollAreaPath = resolve(
  dirname(fileURLToPath(import.meta.url)),
  'scroll-area.tsx',
)

describe('ScrollArea source', () => {
  it('keeps Radix scrollbar layers elevated', () => {
    const source = readFileSync(scrollAreaPath, 'utf8')

    expect(source).toContain(
      'relative z-[var(--scrollbar-z-index)] flex touch-none',
    )
    expect(source).toContain(
      'relative z-[var(--scrollbar-z-index)] flex-1 rounded-full bg-border',
    )
    expect(source).toContain('data-slot="scroll-area-corner"')
    expect(source).toContain('className="relative z-[var(--scrollbar-z-index)]"')
  })
})
