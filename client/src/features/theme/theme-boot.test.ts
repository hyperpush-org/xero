import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it, afterEach } from 'vitest'
import { THEMES } from './theme-definitions'

const INDEX_HTML = readFileSync(resolve(process.cwd(), 'index.html'), 'utf8')
const BOOT_SCRIPT = INDEX_HTML.match(/<script>\s*([\s\S]*?)\s*<\/script>/)?.[1]

function resetDocumentTheme() {
  const root = document.documentElement
  root.removeAttribute('class')
  root.removeAttribute('style')
  root.removeAttribute('data-theme')
}

function runBootScript() {
  if (!BOOT_SCRIPT) {
    throw new Error('Expected index.html to include the boot theme script.')
  }
  new Function(BOOT_SCRIPT)()
}

afterEach(() => {
  window.localStorage.clear()
  resetDocumentTheme()
})

describe('index.html boot theme script', () => {
  it('seeds the stored built-in palette before the loading splash paints', () => {
    window.localStorage.setItem('xero:theme', 'midnight')

    runBootScript()

    const root = document.documentElement
    expect(root.dataset.theme).toBe('midnight')
    expect(root.classList.contains('theme-midnight')).toBe(true)
    expect(root.classList.contains('dark')).toBe(true)
    expect(root.style.getPropertyValue('--background')).toBe('#1e1e1e')
    expect(root.style.getPropertyValue('--foreground')).toBe('#d4d4d4')
    expect(root.style.getPropertyValue('--primary')).toBe('#4ea1ff')
    expect(root.style.getPropertyValue('--shell-background')).toBe('#1e1e1e')
  })

  it('keeps a stored built-in theme when custom theme storage is unreadable', () => {
    window.localStorage.setItem('xero:theme', 'midnight')
    window.localStorage.setItem('xero:custom-themes', '{')

    runBootScript()

    const root = document.documentElement
    expect(root.dataset.theme).toBe('midnight')
    expect(root.style.getPropertyValue('--background')).toBe('#1e1e1e')
    expect(root.style.getPropertyValue('--primary')).toBe('#4ea1ff')
  })

  it('keeps every built-in loading palette in sync with the theme registry', () => {
    for (const theme of THEMES) {
      resetDocumentTheme()
      window.localStorage.clear()
      window.localStorage.setItem('xero:theme', theme.id)

      runBootScript()

      const root = document.documentElement
      expect(root.dataset.theme, theme.id).toBe(theme.id)
      expect(root.classList.contains(`theme-${theme.id}`), theme.id).toBe(true)
      expect(root.classList.contains(theme.appearance), theme.id).toBe(true)
      expect(root.style.getPropertyValue('--background'), theme.id).toBe(theme.colors.background)
      expect(root.style.getPropertyValue('--foreground'), theme.id).toBe(theme.colors.foreground)
      expect(root.style.getPropertyValue('--primary'), theme.id).toBe(theme.colors.primary)
      expect(root.style.getPropertyValue('--shell-background'), theme.id).toBe(
        theme.colors.shellBackground,
      )
    }
  })

  it('seeds custom theme colors for the boot and React loading screens', () => {
    window.localStorage.setItem('xero:theme', 'custom-ember')
    window.localStorage.setItem(
      'xero:custom-themes',
      JSON.stringify([
        {
          id: 'custom-ember',
          appearance: 'light',
          colors: {
            background: '#f8efe7',
            foreground: '#2a1710',
            primary: '#b7431d',
            shellBackground: '#fff8f2',
          },
        },
      ]),
    )

    runBootScript()

    const root = document.documentElement
    expect(root.dataset.theme).toBe('custom-ember')
    expect(root.classList.contains('theme-custom-ember')).toBe(true)
    expect(root.classList.contains('light')).toBe(true)
    expect(root.style.colorScheme).toBe('light')
    expect(root.style.getPropertyValue('--background')).toBe('#f8efe7')
    expect(root.style.getPropertyValue('--foreground')).toBe('#2a1710')
    expect(root.style.getPropertyValue('--primary')).toBe('#b7431d')
    expect(root.style.getPropertyValue('--shell-background')).toBe('#fff8f2')
  })

  it('keeps the pre-React loading mark tied to CSS theme variables', () => {
    expect(INDEX_HTML).toContain('class="boot-loading-logo"')
    expect(INDEX_HTML).toContain('fill="var(--primary, #d4a574)"')
    expect(INDEX_HTML).toContain('fill="var(--foreground, #f8f9fa)"')
    expect(INDEX_HTML).not.toContain('src="/icon-logo.svg" alt=""')
  })
})
