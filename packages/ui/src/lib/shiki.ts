/** Lazy singleton shiki highlighter for code and diff syntax highlighting. */

import type { Highlighter, ThemedToken } from 'shiki'
import { estimateUtf16Bytes } from './byte-budget-cache'
export { getLangFromPath } from './language-detection'

let highlighterPromise: Promise<Highlighter> | null = null
const loadedLangs = new Set<string>()
const loadedThemes = new Set<string>()
const DEFAULT_SHIKI_THEME = 'github-dark'
const DEFAULT_TOKEN_CACHE_MAX_ENTRIES = 320
const DEFAULT_TOKEN_CACHE_MAX_BYTES = 2 * 1024 * 1024
const DEFAULT_TOKENIZE_MAX_BYTES = 160 * 1024

export const DEFAULT_SHIKI_TOKENIZE_MAX_BYTES = DEFAULT_TOKENIZE_MAX_BYTES

export type TokenizedLine = ThemedToken[]

interface ShikiTokenCacheEntry {
  byteSize: number
  lang: string
  theme: string
  tokens: TokenizedLine[]
}

export interface ShikiTokenCacheConfig {
  maxBytes: number
  maxEntries: number
}

export interface ShikiTokenCacheStats {
  byteSize: number
  entries: number
  evictions: number
  hits: number
  misses: number
  skippedByBudget: number
  themeInvalidations: number
}

export interface ShikiTokenCache {
  clear: () => void
  get: (code: string, lang: string, theme: string) => TokenizedLine[] | null
  getStats: () => ShikiTokenCacheStats
  invalidateTheme: (theme: string) => number
  noteSkippedByBudget: () => void
  set: (code: string, lang: string, theme: string, tokens: TokenizedLine[]) => void
}

export interface TokenizeCodeOptions {
  /** Maximum UTF-16 byte budget for a single tokenization request. */
  maxBytes?: number
}

export function hashCodeContent(input: string): string {
  let hash = 0x811c9dc5
  for (let index = 0; index < input.length; index += 1) {
    hash ^= input.charCodeAt(index)
    hash = Math.imul(hash, 0x01000193)
  }
  return (hash >>> 0).toString(36)
}

export function estimateCodeBytes(input: string): number {
  return estimateUtf16Bytes(input)
}

export function createShikiTokenCacheKey(code: string, lang: string, theme: string): string {
  return [lang, theme, code.length, hashCodeContent(code)].join('\u0000')
}

function estimateTokenizedLineBytes(tokens: TokenizedLine[]): number {
  let bytes = 0
  for (const line of tokens) {
    bytes += 16
    for (const token of line) {
      bytes += estimateCodeBytes(token.content)
      bytes += token.color ? token.color.length * 2 : 0
      bytes += 16
    }
  }
  return bytes
}

export function shouldSkipTokenization(code: string, maxBytes = DEFAULT_TOKENIZE_MAX_BYTES): boolean {
  return estimateCodeBytes(code) > maxBytes
}

export function createShikiTokenCache(
  config: Partial<ShikiTokenCacheConfig> = {},
): ShikiTokenCache {
  const resolved: ShikiTokenCacheConfig = {
    maxBytes: config.maxBytes ?? DEFAULT_TOKEN_CACHE_MAX_BYTES,
    maxEntries: config.maxEntries ?? DEFAULT_TOKEN_CACHE_MAX_ENTRIES,
  }
  const entries = new Map<string, ShikiTokenCacheEntry>()
  const stats = {
    evictions: 0,
    hits: 0,
    misses: 0,
    skippedByBudget: 0,
    themeInvalidations: 0,
  }
  let byteSize = 0

  const deleteEntry = (key: string): boolean => {
    const entry = entries.get(key)
    if (!entry) return false
    byteSize = Math.max(0, byteSize - entry.byteSize)
    return entries.delete(key)
  }

  const trim = () => {
    while (entries.size > resolved.maxEntries || byteSize > resolved.maxBytes) {
      const oldestKey = entries.keys().next().value
      if (oldestKey === undefined) return
      if (deleteEntry(oldestKey)) {
        stats.evictions += 1
      }
    }
  }

  return {
    clear() {
      entries.clear()
      byteSize = 0
      stats.evictions = 0
      stats.hits = 0
      stats.misses = 0
      stats.skippedByBudget = 0
      stats.themeInvalidations = 0
    },
    get(code, lang, theme) {
      const key = createShikiTokenCacheKey(code, lang, theme)
      const entry = entries.get(key)
      if (!entry) {
        stats.misses += 1
        return null
      }

      entries.delete(key)
      entries.set(key, entry)
      stats.hits += 1
      return entry.tokens
    },
    getStats() {
      return {
        byteSize,
        entries: entries.size,
        evictions: stats.evictions,
        hits: stats.hits,
        misses: stats.misses,
        skippedByBudget: stats.skippedByBudget,
        themeInvalidations: stats.themeInvalidations,
      }
    },
    invalidateTheme(theme) {
      let removed = 0
      for (const [key, entry] of entries) {
        if (entry.theme !== theme) continue
        if (deleteEntry(key)) {
          removed += 1
        }
      }
      stats.themeInvalidations += removed
      return removed
    },
    noteSkippedByBudget() {
      stats.skippedByBudget += 1
    },
    set(code, lang, theme, tokens) {
      const key = createShikiTokenCacheKey(code, lang, theme)
      if (entries.has(key)) {
        deleteEntry(key)
      }

      const tokenBytes = estimateTokenizedLineBytes(tokens)
      const entryBytes = estimateCodeBytes(code) + tokenBytes + lang.length * 2 + theme.length * 2
      if (entryBytes > resolved.maxBytes) {
        stats.skippedByBudget += 1
        return
      }

      entries.set(key, {
        byteSize: entryBytes,
        lang,
        theme,
        tokens,
      })
      byteSize += entryBytes
      trim()
    },
  }
}

const tokenCache = createShikiTokenCache()
const pendingTokenizations = new Map<string, Promise<TokenizedLine[] | null>>()

function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = import('shiki').then(({ createHighlighter }) =>
      createHighlighter({
        themes: [DEFAULT_SHIKI_THEME] as never,
        langs: [],
      }),
    )
    loadedThemes.add(DEFAULT_SHIKI_THEME)
  }
  return highlighterPromise
}

async function ensureTheme(hl: Highlighter, theme: string): Promise<void> {
  if (loadedThemes.has(theme)) {
    return
  }

  await hl.loadTheme(theme as never)
  loadedThemes.add(theme)
}

async function ensureLanguage(hl: Highlighter, lang: string): Promise<void> {
  if (loadedLangs.has(lang)) {
    return
  }

  await hl.loadLanguage(lang as never)
  loadedLangs.add(lang)
}

/**
 * Tokenize a block of code into per-line token arrays.
 *
 * @param code   Source text to tokenize.
 * @param lang   Shiki language id.
 * @param theme  Shiki theme id. Themes load on demand with the tokenization path.
 * @returns Per-line token arrays, or `null` if tokenization fails.
 */
export async function tokenizeCode(
  code: string,
  lang: string,
  theme: string = DEFAULT_SHIKI_THEME,
  options: TokenizeCodeOptions = {},
): Promise<TokenizedLine[] | null> {
  const maxBytes = options.maxBytes ?? DEFAULT_TOKENIZE_MAX_BYTES
  if (shouldSkipTokenization(code, maxBytes)) {
    tokenCache.noteSkippedByBudget()
    return null
  }

  const cached = tokenCache.get(code, lang, theme)
  if (cached) {
    return cached
  }

  const cacheKey = createShikiTokenCacheKey(code, lang, theme)
  const pending = pendingTokenizations.get(cacheKey)
  if (pending) {
    return pending
  }

  const tokenization = tokenizeCodeUncached(code, lang, theme)
    .then((tokens) => {
      if (tokens) {
        tokenCache.set(code, lang, theme, tokens)
      }
      return tokens
    })
    .finally(() => {
      pendingTokenizations.delete(cacheKey)
    })

  pendingTokenizations.set(cacheKey, tokenization)
  return tokenization
}

async function tokenizeCodeUncached(
  code: string,
  lang: string,
  theme: string = DEFAULT_SHIKI_THEME,
): Promise<TokenizedLine[] | null> {
  try {
    const hl = await getHighlighter()
    await ensureLanguage(hl, lang)
    await ensureTheme(hl, theme)

    const { tokens } = hl.codeToTokens(code, {
      lang: lang as never,
      theme,
    })
    return tokens
  } catch {
    return null
  }
}

export function getShikiTokenCacheStats(): ShikiTokenCacheStats & { pending: number } {
  return {
    ...tokenCache.getStats(),
    pending: pendingTokenizations.size,
  }
}

export function invalidateShikiTokenCacheTheme(theme: string): number {
  return tokenCache.invalidateTheme(theme)
}

export function resetShikiTokenCacheForTests(): void {
  tokenCache.clear()
  pendingTokenizations.clear()
}
