import { describe, expect, it } from 'vitest'

import {
  MAX_ATTACHMENT_BYTES,
  classifyAttachment,
  classificationRejectionMessage,
  formatBytes,
} from './agent-attachments'

const file = (overrides: Partial<{ name: string; type: string; size: number }>) => ({
  name: 'sample.bin',
  type: 'application/octet-stream',
  size: 1_000,
  ...overrides,
})

describe('classifyAttachment', () => {
  it('classifies common image MIME types as image', () => {
    for (const mime of ['image/png', 'image/jpeg', 'image/gif', 'image/webp']) {
      const result = classifyAttachment(file({ type: mime, name: `pic.${mime.split('/')[1]}` }))
      expect(result).toEqual({ kind: 'image', mediaType: mime })
    }
  })

  it('classifies application/pdf as document', () => {
    const result = classifyAttachment(file({ type: 'application/pdf', name: 'a.pdf' }))
    expect(result).toEqual({ kind: 'document', mediaType: 'application/pdf' })
  })

  it('classifies text/* as text', () => {
    const result = classifyAttachment(file({ type: 'text/markdown', name: 'notes.md' }))
    expect(result).toEqual({ kind: 'text', mediaType: 'text/markdown' })
  })

  it('classifies code via extension when MIME is octet-stream', () => {
    const result = classifyAttachment(file({ type: '', name: 'route.ts' }))
    expect(result).toEqual({ kind: 'text', mediaType: 'application/x-typescript' })
  })

  it('rejects unsupported MIME types', () => {
    const result = classifyAttachment(
      file({ type: 'application/x-msdownload', name: 'thing.exe' }),
    )
    expect(result).toEqual({ kind: null, reason: 'unsupported' })
  })

  it('rejects empty files', () => {
    const result = classifyAttachment(file({ type: 'image/png', name: 'empty.png', size: 0 }))
    expect(result).toEqual({ kind: null, reason: 'empty' })
  })

  it('rejects files over the per-attachment cap', () => {
    const result = classifyAttachment(
      file({
        type: 'image/png',
        name: 'huge.png',
        size: MAX_ATTACHMENT_BYTES + 1,
      }),
    )
    expect(result).toEqual({ kind: null, reason: 'too_large' })
  })

  it('handles Dockerfile (no extension)', () => {
    const result = classifyAttachment(file({ type: '', name: 'Dockerfile' }))
    expect(result).toEqual({ kind: 'text', mediaType: 'text/plain' })
  })

  it('falls back to extension when MIME is missing', () => {
    const result = classifyAttachment(file({ type: '', name: 'doc.pdf' }))
    expect(result).toEqual({ kind: 'document', mediaType: 'application/pdf' })
  })
})

describe('classificationRejectionMessage', () => {
  it('produces a sensible message per reason', () => {
    expect(
      classificationRejectionMessage({ name: 'a.exe', size: 10 }, { kind: null, reason: 'unsupported' }),
    ).toMatch(/can't be sent/)
    expect(
      classificationRejectionMessage({ name: 'big.png', size: 30_000_000 }, { kind: null, reason: 'too_large' }),
    ).toMatch(/larger than/)
    expect(
      classificationRejectionMessage({ name: 'e.txt', size: 0 }, { kind: null, reason: 'empty' }),
    ).toMatch(/empty/)
  })
})

describe('formatBytes', () => {
  it('formats bytes/KB/MB/GB at sensible breakpoints', () => {
    expect(formatBytes(900)).toBe('900 B')
    expect(formatBytes(1024)).toBe('1.0 KB')
    expect(formatBytes(1024 * 1024)).toBe('1.0 MB')
    expect(formatBytes(1024 * 1024 * 1024)).toBe('1.0 GB')
  })
})
