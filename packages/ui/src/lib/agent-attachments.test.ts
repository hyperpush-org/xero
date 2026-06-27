import { describe, expect, it } from 'vitest'
import { checkAttachmentModelCompatibility } from './agent-attachments'

describe('agent attachment compatibility', () => {
  it('allows image attachments for Grok Build through the xAI model fallback', () => {
    expect(
      checkAttachmentModelCompatibility(
        { kind: 'image', mediaType: 'image/png' },
        {
          providerId: 'xai',
          modelId: 'grok-build-0.1',
          modelLabel: 'Grok Build 0.1',
        },
      ),
    ).toEqual({ supported: true })
  })

  it('does not allow document attachments for Grok Build without a file modality', () => {
    expect(
      checkAttachmentModelCompatibility(
        { kind: 'document', mediaType: 'application/pdf' },
        {
          providerId: 'xai',
          modelId: 'grok-build-0.1',
          modelLabel: 'Grok Build 0.1',
        },
      ),
    ).toEqual({
      supported: false,
      requiredModality: 'file',
      message: 'Grok Build 0.1 does not support file attachments.',
    })
  })
})
