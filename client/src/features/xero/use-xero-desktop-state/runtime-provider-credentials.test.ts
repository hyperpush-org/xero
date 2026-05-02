import { describe, expect, it } from 'vitest'
import type {
  ProviderCredentialDto,
  ProviderCredentialsSnapshotDto,
  ProviderModelCatalogDto,
  RuntimeRunControlSelectionView,
  RuntimeRunView,
} from '@/src/lib/xero-model'
import {
  buildComposerModelOptions,
  buildComposerModelSelectionKey,
  isAgentRuntimeBlocked,
  parseComposerModelSelectionKey,
  resolveSelectedModel,
} from './runtime-provider'

function makeCredential(overrides: Partial<ProviderCredentialDto> = {}): ProviderCredentialDto {
  return {
    providerId: 'openrouter',
    kind: 'api_key',
    hasApiKey: true,
    oauthAccountId: null,
    oauthSessionId: null,
    hasOauthAccessToken: false,
    oauthExpiresAt: null,
    baseUrl: null,
    apiVersion: null,
    region: null,
    projectId: null,
    defaultModelId: null,
    readinessProof: 'stored_secret',
    updatedAt: '2026-04-15T20:00:00.000Z',
    ...overrides,
  }
}

function makeSnapshot(credentials: ProviderCredentialDto[]): ProviderCredentialsSnapshotDto {
  return { credentials }
}

function makeCatalog(
  providerId: ProviderModelCatalogDto['providerId'],
  models: { modelId: string; displayName: string; thinking?: boolean }[],
): ProviderModelCatalogDto {
  return {
    profileId: `${providerId}-default`,
    providerId,
    configuredModelId: models[0]?.modelId ?? '',
    source: 'cache',
    fetchedAt: '2026-04-15T20:00:00.000Z',
    lastSuccessAt: '2026-04-15T20:00:00.000Z',
    lastRefreshError: null,
    models: models.map((m) => ({
      modelId: m.modelId,
      displayName: m.displayName,
      thinking: {
        supported: m.thinking ?? false,
        effortOptions: m.thinking ? ['medium', 'high'] : [],
        defaultEffort: m.thinking ? 'medium' : null,
      },
    })),
  }
}

function makeSelectedRunControls(
  overrides: Partial<RuntimeRunControlSelectionView> = {},
): RuntimeRunControlSelectionView {
  return {
    providerProfileId: null,
    agentDefinitionId: null,
    agentDefinitionVersion: null,
    runtimeAgentId: 'ask',
    runtimeAgentLabel: 'Ask',
    modelId: 'gpt-5',
    thinkingEffort: null,
    thinkingEffortLabel: 'Thinking unavailable',
    approvalMode: 'suggest',
    approvalModeLabel: 'Suggest',
    planModeRequired: false,
    source: 'active',
    revision: 1,
    effectiveAt: '2026-04-15T20:00:00.000Z',
    queuedPrompt: null,
    queuedPromptAt: null,
    hasQueuedPrompt: false,
    ...overrides,
  }
}

function makeRuntimeRun(overrides: Partial<RuntimeRunView> = {}): RuntimeRunView {
  return {
    projectId: 'project-1',
    agentSessionId: 'agent-1',
    runId: 'run-1',
    runtimeKind: 'openrouter',
    providerId: 'openrouter',
    runtimeLabel: 'OpenRouter · Agent running',
    supervisorKind: 'owned_agent',
    supervisorLabel: 'Owned Agent',
    status: 'running',
    statusLabel: 'Agent running',
    transport: {
      kind: 'internal',
      endpoint: 'xero://owned-agent',
      liveness: 'reachable',
      livenessLabel: 'Runtime reachable',
    },
    controls: {
      active: {
        providerProfileId: null,
        agentDefinitionId: null,
        agentDefinitionVersion: null,
        runtimeAgentId: 'ask',
        runtimeAgentLabel: 'Ask',
        modelId: 'gpt-5',
        thinkingEffort: null,
        thinkingEffortLabel: 'Thinking unavailable',
        approvalMode: 'suggest',
        approvalModeLabel: 'Suggest',
        planModeRequired: false,
        revision: 1,
        appliedAt: '2026-04-15T20:00:00.000Z',
      },
      pending: null,
      selected: makeSelectedRunControls(),
      hasPendingControls: false,
    },
    startedAt: '2026-04-15T20:00:00.000Z',
    lastHeartbeatAt: null,
    lastCheckpointSequence: 0,
    lastCheckpointAt: null,
    stoppedAt: null,
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-15T20:00:00.000Z',
    checkpoints: [],
    latestCheckpoint: null,
    checkpointCount: 0,
    hasCheckpoints: false,
    isActive: true,
    isTerminal: false,
    isStale: false,
    isFailed: false,
    ...overrides,
  }
}

describe('resolveSelectedModel', () => {
  it('falls back to provider:Runtime provider when no credentials exist', () => {
    const view = resolveSelectedModel(null, null)
    expect(view.providerId).toBeNull()
    expect(view.providerLabel).toBe('Runtime provider')
    expect(view.hasCredential).toBe(false)
    expect(view.modelId).toBeNull()
    expect(view.source).toBe('fallback')
  })

  it('uses runtime-run controls as the source when present', () => {
    const credentials = makeSnapshot([makeCredential({ providerId: 'openrouter' })])
    const view = resolveSelectedModel(
      credentials,
      makeSelectedRunControls({ modelId: 'openai/gpt-4.1-mini' }),
      { runtimeRun: makeRuntimeRun({ providerId: 'openrouter' }) },
    )
    expect(view.source).toBe('runtime_run')
    expect(view.providerId).toBe('openrouter')
    expect(view.modelId).toBe('openai/gpt-4.1-mini')
    expect(view.hasCredential).toBe(true)
    expect(view.credentialKind).toBe('api_key')
  })

  it('uses runtime-run truth even when the provider has no credential', () => {
    const credentials = makeSnapshot([])
    const view = resolveSelectedModel(
      credentials,
      makeSelectedRunControls({ modelId: 'claude-3-7-sonnet-latest' }),
      { runtimeRun: makeRuntimeRun({ providerId: 'anthropic' }) },
    )
    expect(view.source).toBe('runtime_run')
    expect(view.providerId).toBe('anthropic')
    expect(view.hasCredential).toBe(false)
  })

  it('falls back to credential.defaultModelId when no run controls', () => {
    const credentials = makeSnapshot([
      makeCredential({ providerId: 'openrouter', defaultModelId: null }),
      makeCredential({ providerId: 'anthropic', defaultModelId: 'claude-3-7-sonnet-latest' }),
    ])
    const view = resolveSelectedModel(credentials, null)
    expect(view.source).toBe('credential_default')
    expect(view.providerId).toBe('anthropic')
    expect(view.modelId).toBe('claude-3-7-sonnet-latest')
  })

  it('falls back to first credential when none has a default model', () => {
    const credentials = makeSnapshot([
      makeCredential({ providerId: 'openrouter' }),
      makeCredential({ providerId: 'anthropic' }),
    ])
    const view = resolveSelectedModel(credentials, null)
    expect(view.source).toBe('fallback')
    expect(view.providerId).toBe('openrouter')
    expect(view.modelId).toBeNull()
    expect(view.hasCredential).toBe(true)
  })
})

describe('buildComposerModelOptions', () => {
  it('returns an empty list when no credentials exist', () => {
    expect(buildComposerModelOptions(null, {})).toEqual([])
    expect(buildComposerModelOptions(makeSnapshot([]), {})).toEqual([])
  })

  it('returns a flat list across credentialed providers, sorted by provider then displayName', () => {
    const credentials = makeSnapshot([
      makeCredential({ providerId: 'openrouter' }),
      makeCredential({ providerId: 'anthropic' }),
    ])
    const catalogs = {
      openrouter: makeCatalog('openrouter', [
        { modelId: 'openai/gpt-4.1-mini', displayName: 'GPT-4.1 mini' },
        { modelId: 'meta/llama-4', displayName: 'Llama 4', thinking: true },
      ]),
      anthropic: makeCatalog('anthropic', [
        { modelId: 'claude-3-7-sonnet-latest', displayName: 'Claude 3.7 Sonnet' },
      ]),
    }
    const options = buildComposerModelOptions(credentials, catalogs)
    expect(options.map((o) => o.selectionKey)).toEqual([
      'anthropic:claude-3-7-sonnet-latest',
      'openrouter:openai/gpt-4.1-mini',
      'openrouter:meta/llama-4',
    ])
    expect(options.map((o) => o.profileId)).toEqual([
      'anthropic-default',
      'openrouter-default',
      'openrouter-default',
    ])
    expect(options[2].thinkingEffortOptions).toEqual(['medium', 'high'])
    expect(options[2].defaultThinkingEffort).toBe('medium')
  })

  it('uses profile-keyed catalogs for credentialed providers', () => {
    const credentials = makeSnapshot([makeCredential({ providerId: 'openai_codex', kind: 'oauth_session' })])
    const catalogs = {
      'openai_codex-default': makeCatalog('openai_codex', [
        { modelId: 'gpt-5.4', displayName: 'GPT-5.4', thinking: true },
      ]),
    }
    const options = buildComposerModelOptions(credentials, catalogs)
    expect(options).toHaveLength(1)
    expect(options[0]).toMatchObject({
      selectionKey: 'openai_codex:gpt-5.4',
      profileId: 'openai_codex-default',
      providerId: 'openai_codex',
      modelId: 'gpt-5.4',
      thinkingEffortOptions: ['medium', 'high'],
      defaultThinkingEffort: 'medium',
    })
  })

  it('omits providers without a catalog entry', () => {
    const credentials = makeSnapshot([
      makeCredential({ providerId: 'openrouter' }),
      makeCredential({ providerId: 'anthropic' }),
    ])
    const catalogs = {
      openrouter: makeCatalog('openrouter', [
        { modelId: 'openai/gpt-4.1-mini', displayName: 'GPT-4.1 mini' },
      ]),
    }
    const options = buildComposerModelOptions(credentials, catalogs)
    expect(options).toHaveLength(1)
    expect(options[0].providerId).toBe('openrouter')
  })
})

describe('isAgentRuntimeBlocked', () => {
  it('blocks when no credentials are configured', () => {
    const blocked = isAgentRuntimeBlocked(makeSnapshot([]), {
      providerId: null,
      providerLabel: 'Runtime provider',
      modelId: null,
      hasCredential: false,
      credentialKind: null,
      source: 'fallback',
    })
    expect(blocked).toBe(true)
  })

  it('blocks when the chosen provider has no credential', () => {
    const credentials = makeSnapshot([makeCredential({ providerId: 'openrouter' })])
    const blocked = isAgentRuntimeBlocked(credentials, {
      providerId: 'anthropic',
      providerLabel: 'Anthropic',
      modelId: 'claude-3-7-sonnet-latest',
      hasCredential: false,
      credentialKind: null,
      source: 'runtime_run',
    })
    expect(blocked).toBe(true)
  })

  it('does not block when the chosen provider has a credential', () => {
    const credentials = makeSnapshot([makeCredential({ providerId: 'openrouter' })])
    const blocked = isAgentRuntimeBlocked(credentials, {
      providerId: 'openrouter',
      providerLabel: 'OpenRouter',
      modelId: 'openai/gpt-4.1-mini',
      hasCredential: true,
      credentialKind: 'api_key',
      source: 'credential_default',
    })
    expect(blocked).toBe(false)
  })
})

describe('composer model selection key', () => {
  it('round-trips through build/parse', () => {
    const key = buildComposerModelSelectionKey('openrouter', 'openai/gpt-4.1-mini')
    expect(key).toBe('openrouter:openai/gpt-4.1-mini')
    expect(parseComposerModelSelectionKey(key)).toEqual({
      providerId: 'openrouter',
      modelId: 'openai/gpt-4.1-mini',
    })
  })

  it('returns null for malformed keys', () => {
    expect(parseComposerModelSelectionKey('no-colon')).toBeNull()
    expect(parseComposerModelSelectionKey(':leading-empty')).toBeNull()
    expect(parseComposerModelSelectionKey('trailing-empty:')).toBeNull()
  })

  it('parses model ids that contain colons', () => {
    expect(parseComposerModelSelectionKey('openrouter:vendor:model:variant')).toEqual({
      providerId: 'openrouter',
      modelId: 'vendor:model:variant',
    })
  })
})
