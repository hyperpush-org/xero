/**
 * Singleton shiki highlighter for diff syntax highlighting.
 *
 * Uses the JavaScript regex engine (no WASM) and loads languages on demand.
 * Themes are bundled at build time for the two we need (dark/light).
 */

import { createHighlighter, type Highlighter, type ThemedToken } from 'shiki'

let highlighterPromise: Promise<Highlighter> | null = null
const loadedLangs = new Set<string>()

/** File extension → shiki language id */
const EXT_LANG_MAP: Record<string, string> = {
  ts: 'typescript',
  tsx: 'tsx',
  js: 'javascript',
  jsx: 'jsx',
  mts: 'typescript',
  cts: 'typescript',
  mjs: 'javascript',
  cjs: 'javascript',
  rs: 'rust',
  toml: 'toml',
  json: 'json',
  jsonc: 'jsonc',
  md: 'markdown',
  mdx: 'mdx',
  css: 'css',
  scss: 'scss',
  html: 'html',
  vue: 'vue',
  svelte: 'svelte',
  yaml: 'yaml',
  yml: 'yaml',
  sh: 'bash',
  bash: 'bash',
  zsh: 'bash',
  sql: 'sql',
  py: 'python',
  rb: 'ruby',
  go: 'go',
  swift: 'swift',
  kt: 'kotlin',
  java: 'java',
  c: 'c',
  h: 'c',
  cpp: 'cpp',
  hpp: 'cpp',
  lock: 'json',
}

function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: ['github-dark'],
      langs: [],
    })
  }
  return highlighterPromise
}

export function getLangFromPath(filePath: string): string | null {
  const dot = filePath.lastIndexOf('.')
  if (dot < 0) return null
  const ext = filePath.slice(dot + 1).toLowerCase()
  return EXT_LANG_MAP[ext] ?? null
}

export type TokenizedLine = ThemedToken[]

/**
 * Tokenize a block of code into per-line token arrays.
 * Returns null if the language is unsupported or loading fails.
 */
export async function tokenizeCode(
  code: string,
  lang: string,
): Promise<TokenizedLine[] | null> {
  try {
    const hl = await getHighlighter()

    if (!loadedLangs.has(lang)) {
      await hl.loadLanguage(lang as any)
      loadedLangs.add(lang)
    }

    const { tokens } = hl.codeToTokens(code, {
      lang: lang as any,
      theme: 'github-dark',
    })
    return tokens
  } catch {
    return null
  }
}
