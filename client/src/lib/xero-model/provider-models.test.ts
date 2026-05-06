import { describe, expect, it } from 'vitest'
import {
  createProviderModelCatalogRequest,
  createUnavailableProviderModelCatalog,
  estimateProviderModelCatalogBytes,
  getProviderModelCatalogFreshnessLabel,
  getProviderModelById,
  getProviderModelCatalogConfiguredModel,
  getProviderModelThinkingEffortLabel,
  hasProviderModelCatalogSnapshot,
  providerCapabilityCatalogSchema,
  providerPreflightSnapshotSchema,
  providerModelCatalogSchema,
} from './provider-models'

function makeOpenRouterCapabilities() {
  return {
    contractVersion: 1,
    providerId: 'openrouter',
    providerLabel: 'OpenRouter',
    defaultModelId: 'openai/o4-mini',
    runtimeFamily: 'openrouter',
    runtimeKind: 'openrouter',
    authMethod: 'api_key',
    credentialProof: 'stored_secret',
    transportMode: 'hosted_api',
    endpointShape: 'openai_chat_completions',
    catalogKind: 'model_provider',
    modelListStrategy: 'live_provider_catalog',
    externalAgentAdapter: false,
    cache: {
      source: 'live',
      fetchedAt: '2026-04-21T12:00:00Z',
      lastSuccessAt: '2026-04-21T12:00:00Z',
      ageSeconds: 120,
      ttlSeconds: 86_400,
      stale: false,
    },
    requestPreview: {
      route: 'POST /chat/completions',
      modelId: 'openai/o4-mini',
      enabledFeatures: ['streaming', 'tool_calls', 'reasoning'],
      toolSchemaNames: ['xero_echo_probe'],
      headers: ['Authorization/x-api-key: [redacted]'],
      metadata: ['transportMode=hosted_api'],
    },
    capabilities: {
      streaming: {
        status: 'supported',
        source: 'static',
        detail: 'Streaming provider requests are supported.',
      },
      toolCalls: {
        status: 'supported',
        source: 'static',
        strictnessBehavior: 'openai_function_schema',
        schemaDialect: 'json_schema_object',
        parallelCallBehavior: 'provider_decides',
        knownIncompatibilities: [],
      },
      reasoning: {
        status: 'supported',
        source: 'live',
        effortLevels: ['minimal', 'low', 'medium', 'high', 'x_high'],
        defaultEffort: 'medium',
        summarySupport: 'provider_default',
        clamping: 'unsupported_effort_dropped_before_request',
        unsupportedModelFallback: 'disable_reasoning_control',
      },
      attachments: {
        status: 'unavailable',
        source: 'static',
        imageInput: 'not_wired_in_owned_adapter',
        documentInput: 'not_wired_in_owned_adapter',
        supportedTypes: [],
        limits: ['This owned adapter currently sends text and tool calls only.'],
      },
      contextLimits: {
        status: 'supported',
        source: 'live_catalog',
        confidence: 'high',
        contextWindowTokens: 128_000,
        maxOutputTokens: 16_384,
      },
      costHints: {
        status: 'supported',
        source: 'live',
        detail: 'OpenRouter exposes price metadata as a hint.',
      },
    },
    knownLimitations: ['Image and document attachments are not sent by this owned adapter.'],
    remediations: [],
  }
}

function makeOpenRouterCatalog() {
  return {
    profileId: 'openrouter-work',
    providerId: 'openrouter' as const,
    configuredModelId: 'openai/o4-mini',
    source: 'live' as const,
    fetchedAt: '2026-04-21T12:00:00Z',
    lastSuccessAt: '2026-04-21T12:00:00Z',
    lastRefreshError: null,
    capabilities: makeOpenRouterCapabilities(),
    cacheAgeSeconds: 120,
    cacheTtlSeconds: 86_400,
    models: [
      {
        modelId: 'openai/o4-mini',
        displayName: 'OpenAI o4-mini',
        thinking: {
          supported: true,
          effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'] as const,
          defaultEffort: 'medium' as const,
        },
        contextWindowTokens: 128_000,
        maxOutputTokens: 16_384,
        contextLimitSource: 'live_catalog',
        contextLimitConfidence: 'high',
        contextLimitFetchedAt: '2026-04-21T12:00:00Z',
        capabilities: makeOpenRouterCapabilities(),
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
    expect(getProviderModelCatalogFreshnessLabel(catalog)).toContain('2m / 24h TTL')
    expect(catalog.capabilities?.requestPreview.headers[0]).toContain('[redacted]')
    expect(catalog.models[0]?.capabilities?.capabilities.contextLimits.contextWindowTokens).toBe(128_000)
  })

  it('parses the shared provider capability catalog contract', () => {
    const capabilities = providerCapabilityCatalogSchema.parse(makeOpenRouterCapabilities())

    expect(capabilities.catalogKind).toBe('model_provider')
    expect(capabilities.capabilities.toolCalls.status).toBe('supported')
    expect(capabilities.requestPreview.toolSchemaNames).toEqual(['xero_echo_probe'])
  })

  it('parses provider preflight snapshots without treating static metadata as a live probe', () => {
    const snapshot = providerPreflightSnapshotSchema.parse({
      contractVersion: 1,
      profileId: 'openrouter-default',
      providerId: 'openrouter',
      modelId: 'openai/o4-mini',
      source: 'static_manual',
      checkedAt: '2026-05-04T12:00:00Z',
      ageSeconds: 0,
      ttlSeconds: 21_600,
      stale: false,
      requiredFeatures: {
        streaming: true,
        toolCalls: true,
        reasoningControls: false,
        attachments: false,
      },
      capabilities: makeOpenRouterCapabilities(),
      checks: [
        {
          checkId: 'provider-preflight:v1:openrouter-default:openrouter:openai-o4-mini:tool',
          status: 'warning',
          code: 'provider_preflight_tool_schema',
          message: 'Minimal tool-call schema is known from capability metadata but was not proven by a live preflight probe.',
          source: 'static_manual',
          retryable: false,
        },
      ],
      status: 'warning',
    })

    expect(snapshot.checks[0]?.status).toBe('warning')
    expect(snapshot.source).toBe('static_manual')
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

  it('estimates provider-model catalog retained bytes for cache budgeting', () => {
    const catalog = providerModelCatalogSchema.parse(makeOpenRouterCatalog())

    expect(estimateProviderModelCatalogBytes(catalog)).toBeGreaterThan(
      catalog.models.reduce((sum, model) => sum + model.modelId.length, 0),
    )
    expect(estimateProviderModelCatalogBytes(null)).toBe(0)
  })
})
