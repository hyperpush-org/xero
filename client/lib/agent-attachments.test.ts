import { describe, expect, it } from 'vitest'

import {
  MAX_ATTACHMENT_BYTES,
  checkAttachmentModelCompatibility,
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

describe('checkAttachmentModelCompatibility', () => {
  it('allows text attachments without special model input modalities', () => {
    expect(checkAttachmentModelCompatibility({ kind: 'text', mediaType: 'text/plain' }, null)).toEqual({
      supported: true,
    })
  })

  it('allows images when the selected model reports image input', () => {
    expect(
      checkAttachmentModelCompatibility(
        { kind: 'image', mediaType: 'image/png' },
        { label: 'Vision model', inputModalities: ['text', 'image'] },
      ),
    ).toEqual({ supported: true })
  })

  it('rejects images when the selected model only reports text input', () => {
    const result = checkAttachmentModelCompatibility(
      { kind: 'image', mediaType: 'image/png' },
      { label: 'Text model', inputModalities: ['text'] },
    )

    expect(result.supported).toBe(false)
    if (!result.supported) {
      expect(result.message).toContain('Text model does not support image attachments')
    }
  })

  it('allows xAI Grok image attachments when stale catalog state omitted modalities', () => {
    expect(
      checkAttachmentModelCompatibility(
        { kind: 'image', mediaType: 'image/png' },
        {
          providerId: 'xai',
          modelId: 'grok-4.3',
          label: 'Grok 4.3',
          inputModalities: [],
        },
      ),
    ).toEqual({ supported: true })

    expect(
      checkAttachmentModelCompatibility(
        { kind: 'image', mediaType: 'image/png' },
        {
          providerId: 'xai',
          modelId: 'grok-4.3-latest',
          label: 'Grok 4.3 Latest',
          inputModalities: [],
        },
      ),
    ).toEqual({ supported: true })
  })

  it('allows OpenAI GPT attachments when stale catalog state omitted modalities', () => {
    const staleProfile = {
      providerId: 'openai_codex',
      modelId: 'gpt-5.5',
      label: 'GPT-5.5',
      inputModalities: [],
    }

    expect(
      checkAttachmentModelCompatibility(
        { kind: 'image', mediaType: 'image/png' },
        staleProfile,
      ),
    ).toEqual({ supported: true })

    expect(
      checkAttachmentModelCompatibility(
        { kind: 'document', mediaType: 'application/pdf' },
        staleProfile,
      ),
    ).toEqual({ supported: true })

    expect(
      checkAttachmentModelCompatibility(
        { kind: 'image', mediaType: 'image/png' },
        {
          providerId: 'openai_api',
          modelId: 'openai/gpt-5.4',
          label: 'GPT-5.4',
          inputModalities: [],
        },
      ),
    ).toEqual({ supported: true })
  })

  it('does not infer attachment support for GPT-specialized models without matching modalities', () => {
    const result = checkAttachmentModelCompatibility(
      { kind: 'image', mediaType: 'image/png' },
      {
        providerId: 'openai_api',
        modelId: 'gpt-audio',
        label: 'GPT Audio',
        inputModalities: [],
      },
    )

    expect(result.supported).toBe(false)
  })

  it('allows documents when supported types include the file media type', () => {
    expect(
      checkAttachmentModelCompatibility(
        { kind: 'document', mediaType: 'application/pdf' },
        { label: 'File model', supportedTypes: ['application/pdf'] },
      ),
    ).toEqual({ supported: true })
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
