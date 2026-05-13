import { describe, expect, it } from 'vitest'
import {
  applyLineEnding,
  describeIndentMode,
  describeLineEnding,
  detectDocumentSettings,
  detectHasFinalNewline,
  detectIndent,
  detectLineEnding,
  normalizeToLf,
  serializeWithSettings,
} from './document-settings'

describe('detectLineEnding', () => {
  it('detects pure LF', () => {
    expect(detectLineEnding('line 1\nline 2\n')).toBe('lf')
  })

  it('detects pure CRLF', () => {
    expect(detectLineEnding('line 1\r\nline 2\r\n')).toBe('crlf')
  })

  it('detects mixed line endings', () => {
    expect(detectLineEnding('line 1\r\nline 2\nline 3\n')).toBe('mixed')
  })

  it('defaults to LF for empty or single-line content', () => {
    expect(detectLineEnding('')).toBe('lf')
    expect(detectLineEnding('no newline')).toBe('lf')
  })
})

describe('normalizeToLf', () => {
  it('normalizes CRLF and lone CR to LF', () => {
    expect(normalizeToLf('a\r\nb\rc')).toBe('a\nb\nc')
  })

  it('leaves LF-only content untouched', () => {
    const value = 'a\nb\nc'
    expect(normalizeToLf(value)).toBe(value)
  })
})

describe('applyLineEnding', () => {
  it('converts LF to CRLF when requested', () => {
    expect(applyLineEnding('a\nb\n', 'crlf')).toBe('a\r\nb\r\n')
  })

  it('keeps LF for lf/mixed', () => {
    expect(applyLineEnding('a\nb\n', 'lf')).toBe('a\nb\n')
    expect(applyLineEnding('a\nb\n', 'mixed')).toBe('a\nb\n')
  })

  it('does not double-encode existing CRLF when applying CRLF', () => {
    expect(applyLineEnding('a\r\nb\r\n', 'crlf')).toBe('a\r\nb\r\n')
  })
})

describe('detectHasFinalNewline', () => {
  it('returns true when content ends with newline', () => {
    expect(detectHasFinalNewline('hello\n')).toBe(true)
    expect(detectHasFinalNewline('hello\r')).toBe(true)
  })

  it('returns false otherwise', () => {
    expect(detectHasFinalNewline('hello')).toBe(false)
    expect(detectHasFinalNewline('')).toBe(false)
  })
})

describe('detectIndent', () => {
  it('detects two-space indentation', () => {
    const code = 'function foo() {\n  return 1\n  return 2\n}\n'
    expect(detectIndent(code)).toEqual({ mode: 'spaces', size: 2 })
  })

  it('detects four-space indentation', () => {
    const code = 'def foo():\n    return 1\n    if cond:\n        pass\n'
    expect(detectIndent(code)).toEqual({ mode: 'spaces', size: 4 })
  })

  it('detects tab indentation', () => {
    const code = 'func main() {\n\treturn 1\n\treturn 2\n}\n'
    expect(detectIndent(code)).toEqual({ mode: 'tabs', size: 4 })
  })

  it('falls back to spaces (2) for empty content', () => {
    expect(detectIndent('')).toEqual({ mode: 'spaces', size: 2 })
  })
})

describe('serializeWithSettings', () => {
  it('preserves CRLF when round-tripping', () => {
    const settings = detectDocumentSettings('a\r\nb\r\nc\r\n')
    expect(settings.eol).toBe('crlf')
    const result = serializeWithSettings('a\nb\nc\n', settings)
    expect(result).toBe('a\r\nb\r\nc\r\n')
  })

  it('adds final newline when source had one', () => {
    const settings = detectDocumentSettings('a\nb\nc\n')
    expect(settings.hasFinalNewline).toBe(true)
    expect(serializeWithSettings('a\nb\nc', settings)).toBe('a\nb\nc\n')
  })

  it('strips final newline when source had none', () => {
    const settings = detectDocumentSettings('a\nb\nc')
    expect(settings.hasFinalNewline).toBe(false)
    expect(serializeWithSettings('a\nb\nc\n', settings)).toBe('a\nb\nc')
  })
})

describe('describeIndentMode / describeLineEnding', () => {
  it('describes settings for the status bar', () => {
    expect(describeLineEnding('crlf')).toBe('CRLF')
    expect(describeLineEnding('lf')).toBe('LF')
    expect(describeLineEnding('mixed')).toBe('Mixed')
    expect(describeIndentMode({ eol: 'lf', indentMode: 'tabs', indentSize: 4, hasFinalNewline: true })).toBe(
      'Tabs (4)',
    )
    expect(describeIndentMode({ eol: 'lf', indentMode: 'spaces', indentSize: 2, hasFinalNewline: true })).toBe(
      'Spaces (2)',
    )
  })
})
