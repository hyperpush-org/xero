export type LineEnding = 'lf' | 'crlf' | 'mixed'
export type IndentMode = 'spaces' | 'tabs' | 'mixed'

export interface DocumentSettings {
  eol: LineEnding
  indentMode: IndentMode
  indentSize: number
  hasFinalNewline: boolean
}

const INDENT_DETECTION_LINE_LIMIT = 400

export function detectLineEnding(rawText: string): LineEnding {
  let lf = 0
  let crlf = 0
  for (let index = 0; index < rawText.length; index += 1) {
    const char = rawText.charCodeAt(index)
    if (char === 10) {
      const previous = index > 0 ? rawText.charCodeAt(index - 1) : 0
      if (previous === 13) {
        crlf += 1
      } else {
        lf += 1
      }
    }
  }
  if (crlf > 0 && lf === 0) return 'crlf'
  if (lf > 0 && crlf === 0) return 'lf'
  if (crlf === 0 && lf === 0) return 'lf'
  return 'mixed'
}

export function normalizeToLf(rawText: string): string {
  if (!rawText.includes('\r')) return rawText
  return rawText.replace(/\r\n/g, '\n').replace(/\r/g, '\n')
}

export function applyLineEnding(text: string, eol: LineEnding): string {
  if (eol === 'crlf') {
    return text.replace(/\r\n/g, '\n').replace(/\n/g, '\r\n')
  }
  return text
}

export function detectHasFinalNewline(rawText: string): boolean {
  if (rawText.length === 0) return false
  return rawText.endsWith('\n') || rawText.endsWith('\r')
}

export function detectIndent(text: string): { mode: IndentMode; size: number } {
  const lines = text.split('\n')
  const lineCount = Math.min(lines.length, INDENT_DETECTION_LINE_LIMIT)
  let tabCount = 0
  let spaceCount = 0
  const spaceSizeFrequency = new Map<number, number>()
  let previousIndent = 0

  for (let index = 0; index < lineCount; index += 1) {
    const line = lines[index]
    if (line.length === 0) continue
    const first = line.charAt(0)
    if (first === '\t') {
      tabCount += 1
      previousIndent = 0
      continue
    }
    if (first !== ' ') {
      previousIndent = 0
      continue
    }
    let indent = 0
    while (indent < line.length && line.charAt(indent) === ' ') {
      indent += 1
    }
    if (indent === 0) {
      previousIndent = 0
      continue
    }
    spaceCount += 1
    if (previousIndent > 0 && indent > previousIndent) {
      const diff = indent - previousIndent
      if (diff >= 1 && diff <= 8) {
        spaceSizeFrequency.set(diff, (spaceSizeFrequency.get(diff) ?? 0) + 1)
      }
    }
    if (indent > 0 && previousIndent === 0) {
      // First-level indent: record its width too so files where everything sits at one
      // level still produce a reasonable answer.
      if (indent >= 1 && indent <= 8) {
        spaceSizeFrequency.set(indent, (spaceSizeFrequency.get(indent) ?? 0) + 1)
      }
    }
    previousIndent = indent
  }

  if (tabCount === 0 && spaceCount === 0) {
    return { mode: 'spaces', size: 2 }
  }
  if (tabCount > 0 && spaceCount === 0) {
    return { mode: 'tabs', size: 4 }
  }
  if (spaceCount > 0 && tabCount === 0) {
    return { mode: 'spaces', size: pickIndentSize(spaceSizeFrequency) }
  }
  return { mode: spaceCount >= tabCount ? 'spaces' : 'mixed', size: pickIndentSize(spaceSizeFrequency) }
}

function pickIndentSize(frequency: Map<number, number>): number {
  let best = 2
  let bestCount = 0
  for (const [size, count] of frequency.entries()) {
    if (count > bestCount) {
      best = size
      bestCount = count
    }
  }
  return best
}

export function detectDocumentSettings(rawText: string): DocumentSettings {
  const eol = detectLineEnding(rawText)
  const hasFinalNewline = detectHasFinalNewline(rawText)
  const { mode, size } = detectIndent(normalizeToLf(rawText))
  return {
    eol,
    indentMode: mode,
    indentSize: size,
    hasFinalNewline,
  }
}

export function serializeWithSettings(
  text: string,
  settings: DocumentSettings,
): string {
  let output = text
  if (settings.hasFinalNewline && !output.endsWith('\n')) {
    output = `${output}\n`
  } else if (!settings.hasFinalNewline && output.endsWith('\n')) {
    output = output.slice(0, -1)
  }
  return applyLineEnding(output, settings.eol)
}

export function describeLineEnding(eol: LineEnding): string {
  if (eol === 'crlf') return 'CRLF'
  if (eol === 'mixed') return 'Mixed'
  return 'LF'
}

export function describeIndentMode(settings: DocumentSettings): string {
  if (settings.indentMode === 'tabs') return `Tabs (${settings.indentSize})`
  if (settings.indentMode === 'spaces') return `Spaces (${settings.indentSize})`
  return `Mixed indent`
}
