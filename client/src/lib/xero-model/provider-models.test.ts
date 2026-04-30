import { describe, expect, it } from 'vitest'
import {
  createProviderModelCatalogRequest,
  createUnavailableProviderModelCatalog,
  getProviderModelById,
  getProviderModelCatalogConfiguredModel,
  getProviderModelThinkingEffortLabel,
  hasProviderModelCatalogSnapshot,
  providerModelCatalogSchema,
} from './provider-models'

function makeOpenRouterCatalog() {
  return {
    profileId: 'openrouter-work',
    providerId: 'openrouter' as const,
    configuredModelId: 'openai/o4-mini',
    source: 'live' as const,
    fetchedAt: '2026-04-21T12:00:00Z',
    lastSuccessAt: '2026-04-21T12:00:00Z',
    lastRefreshError: null,
    models: [
      {
        modelId: 'openai/o4-mini',
        displayName: 'OpenAI o4-mini',
        thinking: {
          supported: true,
          effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'] as const,
          defaultEffort: 'medium' as const,
        },
      },
      {
        modelId: 'anthropic/claude-3.7-sonnet',
        displayName: 'Claude 3.7 Sonnet',
        thinking: {
          supported: false,
          effortOptions: [],
          defaultEffort: null,
        },
      },
    ],
  }
}

describe('provider-models', () => {
  it('parses a strict provider-model catalog and exposes configured-model helpers', () => {
    const catalog = providerModelCatalogSchema.parse(makeOpenRouterCatalog())

    expect(getProviderModelCatalogConfiguredModel(catalog)?.modelId).toBe('openai/o4-mini')
    expect(getProviderModelById(catalog, 'anthropic/claude-3.7-sonnet')?.displayName).toBe('Claude 3.7 Sonnet')
    expect(hasProviderModelCatalogSnapshot(catalog)).toBe(true)
    expect(getProviderModelThinkingEffortLabel('x_high')).toBe('Very high')
  })

  it('rejects unknown providers and malformed thinking capability payloads', () => {
    const unknownProvider = providerModelCatalogSchema.safeParse({
      ...makeOpenRouterCatalog(),
      providerId: 'deepseek',
    })
    expect(unknownProvider.success).toBe(false)

    const invalidThinking = providerModelCatalogSchema.safeParse({
      ...makeOpenRouterCatalog(),
      models: [
        {
          modelId: 'openai/o4-mini',
          displayName: 'OpenAI o4-mini',
          thinking: {
            supported: true,
            effortOptions: ['low', 'high'],
            defaultEffort: 'medium',
          },
        },
      ],
    })
    expect(invalidThinking.success).toBe(false)

    const unsupportedWithOptions = providerModelCatalogSchema.safeParse({
      ...makeOpenRouterCatalog(),
      models: [
        {
          modelId: 'anthropic/claude-3.7-sonnet',
          displayName: 'Claude 3.7 Sonnet',
          thinking: {
            supported: false,
            effortOptions: ['low'],
            defaultEffort: null,
          },
        },
      ],
    })
    expect(unsupportedWithOptions.success).toBe(false)
  })

  it('rejects inconsistent timestamp or source combinations', () => {
    const unavailableWithSnapshot = providerModelCatalogSchema.safeParse({
      ...makeOpenRouterCatalog(),
      source: 'unavailable',
    })
    expect(unavailableWithSnapshot.success).toBe(false)

    const cacheWithoutTimestamps = providerModelCatalogSchema.safeParse({
      ...makeOpenRouterCatalog(),
      source: 'cache',
      fetchedAt: null,
      lastSuccessAt: null,
    })
    expect(cacheWithoutTimestamps.success).toBe(false)

    const lastSuccessAfterFetch = providerModelCatalogSchema.safeParse({
      ...makeOpenRouterCatalog(),
      fetchedAt: '2026-04-21T12:00:00Z',
      lastSuccessAt: '2026-04-21T12:01:00Z',
    })
    expect(lastSuccessAfterFetch.success).toBe(false)
  })

  it('accepts OpenAI Codex catalogs with real Codex OAuth model ids', () => {
    const valid = providerModelCatalogSchema.safeParse({
      profileId: 'openai_codex-default',
      providerId: 'openai_codex',
      configuredModelId: 'gpt-5.4',
      source: 'live',
      fetchedAt: '2026-04-21T12:00:00Z',
      lastSuccessAt: '2026-04-21T12:00:00Z',
      lastRefreshError: null,
      models: [
        {
          modelId: 'gpt-5.4',
          displayName: 'GPT-5.4',
          thinking: {
            supported: true,
            effortOptions: ['low', 'medium', 'high'],
            defaultEffort: 'medium',
          },
        },
      ],
    })
    expect(valid.success).toBe(true)
  })

  it('accepts Anthropic catalogs with truthful Claude thinking payloads and rejects malformed thinking rows', () => {
    const anthropicCatalog = providerModelCatalogSchema.parse({
      profileId: 'anthropic-work',
      providerId: 'anthropic',
      configuredModelId: 'claude-3-7-sonnet-latest',
      source: 'live',
      fetchedAt: '2026-04-21T12:00:00Z',
      lastSuccessAt: '2026-04-21T12:00:00Z',
      lastRefreshError: null,
      models: [
        {
          modelId: 'claude-3-7-sonnet-latest',
          displayName: 'Claude 3.7 Sonnet',
          thinking: {
            supported: true,
            effortOptions: ['low', 'medium', 'high'],
            defaultEffort: 'medium',
          },
        },
        {
          modelId: 'claude-3-5-haiku-latest',
          displayName: 'Claude 3.5 Haiku',
          thinking: {
            supported: false,
            effortOptions: [],
            defaultEffort: null,
          },
        },
      ],
    })

    expect(getProviderModelCatalogConfiguredModel(anthropicCatalog)?.modelId).toBe('claude-3-7-sonnet-latest')
    expect(getProviderModelById(anthropicCatalog, 'claude-3-5-haiku-latest')?.displayName).toBe('Claude 3.5 Haiku')

    const malformedAnthropic = providerModelCatalogSchema.safeParse({
      ...anthropicCatalog,
      models: [
        {
          modelId: 'claude-3-7-sonnet-latest',
          displayName: 'Claude 3.7 Sonnet',
          thinking: {
            supported: true,
            effortOptions: ['low', 'low'],
            defaultEffort: 'low',
          },
        },
      ],
    })
    expect(malformedAnthropic.success).toBe(false)
  })

  it('builds typed request and unavailable catalog helpers', () => {
    expect(createProviderModelCatalogRequest('openrouter-work')).toEqual({
      profileId: 'openrouter-work',
      forceRefresh: false,
    })

    const unavailable = createUnavailableProviderModelCatalog({
      profileId: 'openrouter-work',
      providerId: 'openrouter',
      configuredModelId: 'openai/o4-mini',
      lastRefreshError: {
        code: 'openrouter_provider_unavailable',
        message: 'Timed out while refreshing provider models.',
        retryable: true,
      },
    })

    expect(unavailable.source).toBe('unavailable')
    expect(unavailable.models).toEqual([])
    expect(hasProviderModelCatalogSnapshot(unavailable)).toBe(false)
  })
})
