import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(),
}))

import { ProvidersStep } from '@/components/cadence/onboarding/steps/providers-step'
import type {
  ProviderModelCatalogDto,
  ProviderProfileDto,
  ProviderProfilesDto,
  RuntimeSessionView,
  UpsertProviderProfileRequestDto,
} from '@/src/lib/cadence-model'

function makeOpenAiProfile(overrides: Partial<ProviderProfileDto> = {}): ProviderProfileDto {
  return {
    profileId: 'openai_codex-default',
    providerId: 'openai_codex',
    label: 'OpenAI Codex',
    modelId: 'openai_codex',
    active: false,
    readiness: {
      ready: false,
      status: 'missing',
      credentialUpdatedAt: null,
    },
    migratedFromLegacy: false,
    migratedAt: null,
    ...overrides,
  }
}

function makeOpenRouterProfile(overrides: Partial<ProviderProfileDto> = {}): ProviderProfileDto {
  const ready = overrides.readiness?.ready ?? true

  return {
    profileId: 'openrouter-default',
    providerId: 'openrouter',
    label: 'OpenRouter',
    modelId: 'openai/gpt-4.1-mini',
    active: true,
    readiness: ready
      ? {
          ready: true,
          status: 'ready',
          credentialUpdatedAt: '2026-04-20T00:00:00Z',
        }
      : {
          ready: false,
          status: 'missing',
          credentialUpdatedAt: null,
        },
    migratedFromLegacy: true,
    migratedAt: '2026-04-20T00:00:00Z',
    ...overrides,
  }
}

function makeAnthropicProfile(overrides: Partial<ProviderProfileDto> = {}): ProviderProfileDto {
  const ready = overrides.readiness?.ready ?? false

  return {
    profileId: 'anthropic-default',
    providerId: 'anthropic',
    label: 'Anthropic',
    modelId: 'claude-3-7-sonnet-latest',
    active: false,
    readiness: ready
      ? {
          ready: true,
          status: 'ready',
          credentialUpdatedAt: '2026-04-20T00:00:00Z',
        }
      : {
          ready: false,
          status: 'missing',
          credentialUpdatedAt: null,
        },
    migratedFromLegacy: false,
    migratedAt: null,
    ...overrides,
  }
}

function makeProviderProfiles(overrides: Partial<ProviderProfilesDto> = {}): ProviderProfilesDto {
  return {
    activeProfileId: overrides.activeProfileId ?? 'openrouter-default',
    profiles:
      overrides.profiles ?? [makeOpenAiProfile({ active: false }), makeOpenRouterProfile({ active: true })],
    migration: overrides.migration ?? null,
  }
}

function makeProviderModelCatalog(
  profileId: string,
  overrides: Partial<ProviderModelCatalogDto> = {},
): ProviderModelCatalogDto {
  const providerId =
    overrides.providerId ??
    (profileId.startsWith('openrouter')
      ? 'openrouter'
      : profileId.startsWith('anthropic')
        ? 'anthropic'
        : 'openai_codex')
  const configuredModelId =
    overrides.configuredModelId ??
    (providerId === 'openrouter'
      ? 'openai/gpt-4.1-mini'
      : providerId === 'anthropic'
        ? 'claude-3-7-sonnet-latest'
        : 'openai_codex')

  return {
    profileId,
    providerId,
    configuredModelId,
    source: overrides.source ?? 'live',
    fetchedAt: overrides.fetchedAt ?? '2026-04-21T12:00:00Z',
    lastSuccessAt: overrides.lastSuccessAt ?? '2026-04-21T12:00:00Z',
    lastRefreshError: overrides.lastRefreshError ?? null,
    models:
      overrides.models ??
      (providerId === 'openrouter'
        ? [
            {
              modelId: 'openai/gpt-4.1-mini',
              displayName: 'OpenAI GPT-4.1 Mini',
              thinking: {
                supported: true,
                effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
                defaultEffort: 'medium',
              },
            },
            {
              modelId: 'openrouter/anthropic/claude-3.5-sonnet',
              displayName: 'Claude 3.5 Sonnet',
              thinking: {
                supported: true,
                effortOptions: ['low', 'medium', 'high'],
                defaultEffort: 'medium',
              },
            },
          ]
        : providerId === 'anthropic'
          ? [
              {
                modelId: 'claude-3-7-sonnet-latest',
                displayName: 'Claude 3.7 Sonnet',
                thinking: {
                  supported: true,
                  effortOptions: ['low', 'medium', 'high', 'x_high'],
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
            ]
          : [
              {
                modelId: 'openai_codex',
                displayName: 'OpenAI Codex',
                thinking: {
                  supported: true,
                  effortOptions: ['low', 'medium', 'high'],
                  defaultEffort: 'medium',
                },
              },
            ]),
  }
}

function makeRuntimeSession(overrides: Partial<RuntimeSessionView> = {}): RuntimeSessionView {
  return {
    projectId: 'project-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: null,
    sessionId: null,
    accountId: null,
    phase: 'idle',
    phaseLabel: 'Idle',
    runtimeLabel: 'Openai Codex · Signed out',
    accountLabel: 'No account',
    sessionLabel: 'No session',
    callbackBound: null,
    authorizationUrl: null,
    redirectUri: null,
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-20T00:00:00Z',
    isAuthenticated: false,
    isLoginInProgress: false,
    needsManualInput: false,
    isSignedOut: true,
    isFailed: false,
    ...overrides,
  }
}

function makeProvidersStepProps(overrides: Partial<Parameters<typeof ProvidersStep>[0]> = {}) {
  return {
    providerProfiles: makeProviderProfiles(),
    providerProfilesLoadStatus: 'ready' as const,
    providerProfilesLoadError: null,
    providerProfilesSaveStatus: 'idle' as const,
    providerProfilesSaveError: null,
    providerModelCatalogs: {
      'openai_codex-default': makeProviderModelCatalog('openai_codex-default'),
      'openrouter-default': makeProviderModelCatalog('openrouter-default'),
    },
    providerModelCatalogLoadStatuses: {
      'openai_codex-default': 'ready' as const,
      'openrouter-default': 'ready' as const,
    },
    providerModelCatalogLoadErrors: {
      'openai_codex-default': null,
      'openrouter-default': null,
    },
    onRefreshProviderProfiles: vi.fn(async () => makeProviderProfiles()),
    onRefreshProviderModelCatalog: vi.fn(async (profileId: string) => makeProviderModelCatalog(profileId)),
    onUpsertProviderProfile: vi.fn(async (_request: UpsertProviderProfileRequestDto) => makeProviderProfiles()),
    onSetActiveProviderProfile: vi.fn(async (_profileId: string) => makeProviderProfiles()),
    ...overrides,
  }
}

function getProviderCard(label: string): HTMLElement {
  const card = screen
    .getAllByText(label)
    .map((node) => node.closest('.rounded-lg'))
    .find((value): value is HTMLElement => value instanceof HTMLElement)

  if (!card) {
    throw new Error(`Could not find provider card for ${label}`)
  }

  return card
}

describe('ProvidersStep', () => {
  it('renders migrated active profiles, keeps saved keys blank, and validates label/model edits', async () => {
    const onUpsertProviderProfile = vi.fn(async (_request: UpsertProviderProfileRequestDto) => makeProviderProfiles())

    render(
      <ProvidersStep
        {...makeProvidersStepProps({
          onUpsertProviderProfile,
        })}
      />,
    )

    expect(screen.getByText('Active')).toBeVisible()
    expect(screen.getByText('Ready')).toBeVisible()
    expect(screen.getAllByText('Unavailable')).toHaveLength(1)

    fireEvent.click(within(getProviderCard('OpenRouter')).getByRole('button', { name: 'Edit' }))

    const labelInput = screen.getByLabelText('Profile label') as HTMLInputElement
    const modelSelector = screen.getByLabelText('Model')
    const keyInput = screen.getByLabelText('API Key') as HTMLInputElement

    expect(labelInput).toHaveValue('OpenRouter')
    expect(modelSelector).toHaveTextContent('OpenAI GPT-4.1 Mini · openai/gpt-4.1-mini')
    expect(keyInput).toHaveValue('')

    fireEvent.change(labelInput, { target: { value: '   ' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByText('Profile label is required.')).toBeVisible()

    fireEvent.change(labelInput, { target: { value: 'Team OpenRouter' } })
    fireEvent.keyDown(modelSelector, { key: 'ArrowDown' })
    fireEvent.click(await screen.findByRole('option', { name: 'Claude 3.5 Sonnet · openrouter/anthropic/claude-3.5-sonnet' }))
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onUpsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'openrouter-default',
        providerId: 'openrouter',
        label: 'Team OpenRouter',
        modelId: 'openrouter/anthropic/claude-3.5-sonnet',
        activate: true,
      }),
    )
  })

  it('creates and clears Anthropic profiles from the onboarding provider step without special-case UI', async () => {
    const secret = 'sk-ant-test-secret'

    let providerProfiles = makeProviderProfiles({
      activeProfileId: 'openrouter-default',
      profiles: [makeOpenAiProfile({ active: false }), makeOpenRouterProfile({ active: true })],
    })

    const onUpsertProviderProfile = vi.fn(async (request: UpsertProviderProfileRequestDto) => {
      const anthropicReady = typeof request.anthropicApiKey === 'string' && request.anthropicApiKey.trim().length > 0
      providerProfiles = makeProviderProfiles({
        activeProfileId: request.activate ? 'anthropic-default' : providerProfiles.activeProfileId,
        profiles: [
          makeOpenAiProfile({ active: false }),
          makeOpenRouterProfile({ active: !request.activate }),
          makeAnthropicProfile({
            active: Boolean(request.activate),
            label: request.label,
            modelId: request.modelId,
            readiness: anthropicReady
              ? {
                  ready: true,
                  status: 'ready',
                  credentialUpdatedAt: '2026-04-20T12:00:00Z',
                }
              : {
                  ready: false,
                  status: 'missing',
                  credentialUpdatedAt: null,
                },
          }),
        ],
      })

      return providerProfiles
    })

    const { rerender } = render(
      <ProvidersStep
        {...makeProvidersStepProps({
          providerProfiles,
          onRefreshProviderProfiles: vi.fn(async () => providerProfiles),
          onUpsertProviderProfile,
        })}
      />,
    )

    fireEvent.click(within(getProviderCard('Anthropic')).getByRole('button', { name: 'Use this' }))

    await waitFor(() =>
      expect(onUpsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'anthropic-default',
        providerId: 'anthropic',
        label: 'Anthropic',
        modelId: 'claude-3-7-sonnet-latest',
        activate: true,
      }),
    )

    rerender(
      <ProvidersStep
        {...makeProvidersStepProps({
          providerProfiles,
          onRefreshProviderProfiles: vi.fn(async () => providerProfiles),
          onUpsertProviderProfile,
        })}
      />,
    )

    expect(within(getProviderCard('Anthropic')).getByText('Active')).toBeVisible()
    expect(within(getProviderCard('Anthropic')).getByText('Needs key')).toBeVisible()

    fireEvent.click(within(getProviderCard('Anthropic')).getByRole('button', { name: 'Set up' }))
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByText('Anthropic requires an API key.')).toBeVisible()

    fireEvent.change(screen.getByLabelText('API Key'), { target: { value: secret } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onUpsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'anthropic-default',
        providerId: 'anthropic',
        label: 'Anthropic',
        modelId: 'claude-3-7-sonnet-latest',
        anthropicApiKey: secret,
        activate: true,
      }),
    )

    rerender(
      <ProvidersStep
        {...makeProvidersStepProps({
          providerProfiles,
          onRefreshProviderProfiles: vi.fn(async () => providerProfiles),
          onUpsertProviderProfile,
        })}
      />,
    )

    fireEvent.click(within(getProviderCard('Anthropic')).getByRole('button', { name: 'Edit' }))
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }))
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onUpsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'anthropic-default',
        providerId: 'anthropic',
        label: 'Anthropic',
        modelId: 'claude-3-7-sonnet-latest',
        anthropicApiKey: '',
        activate: true,
      }),
    )
  })

  it('switches active profile truth without leaving stale active badges behind', async () => {
    let providerProfiles = makeProviderProfiles({
      activeProfileId: 'openai_codex-default',
      profiles: [makeOpenAiProfile({ active: true }), makeOpenRouterProfile({ active: false, migratedFromLegacy: false, migratedAt: null })],
    })

    const onSetActiveProviderProfile = vi.fn(async (_profileId: string) => {
      providerProfiles = makeProviderProfiles({
        activeProfileId: 'openrouter-default',
        profiles: [makeOpenAiProfile({ active: false }), makeOpenRouterProfile({ active: true, migratedFromLegacy: false, migratedAt: null })],
      })
      return providerProfiles
    })

    const { rerender } = render(
      <ProvidersStep
        {...makeProvidersStepProps({
          providerProfiles,
          onRefreshProviderProfiles: vi.fn(async () => providerProfiles),
          onUpsertProviderProfile: vi.fn(async (_request: UpsertProviderProfileRequestDto) => providerProfiles),
          onSetActiveProviderProfile,
        })}
      />,
    )

    fireEvent.click(within(getProviderCard('OpenRouter')).getByRole('button', { name: 'Use this' }))
    await waitFor(() => expect(onSetActiveProviderProfile).toHaveBeenCalledWith('openrouter-default'))

    rerender(
      <ProvidersStep
        {...makeProvidersStepProps({
          providerProfiles,
          onRefreshProviderProfiles: vi.fn(async () => providerProfiles),
          onUpsertProviderProfile: vi.fn(async (_request: UpsertProviderProfileRequestDto) => providerProfiles),
          onSetActiveProviderProfile,
        })}
      />,
    )

    expect(screen.getAllByText('Active')).toHaveLength(1)
  })

  it('scopes OpenAI auth copy to the selected profile and uses onboarding project guidance when no project is selected', () => {
    render(
      <ProvidersStep
        {...makeProvidersStepProps({
          providerProfiles: makeProviderProfiles({
            activeProfileId: 'zz-openai-alt',
            profiles: [
              makeOpenAiProfile({ active: false }),
              makeOpenAiProfile({
                profileId: 'zz-openai-alt',
                label: 'OpenAI Alt',
                active: true,
              }),
              makeOpenRouterProfile({ active: false, migratedFromLegacy: false, migratedAt: null }),
            ],
          }),
          runtimeSession: makeRuntimeSession(),
          hasSelectedProject: false,
          onStartLogin: vi.fn(async () => makeRuntimeSession()),
          onLogout: vi.fn(async () => makeRuntimeSession()),
        })}
      />,
    )

    expect(screen.getByText('Choose a project next')).toBeVisible()
    expect(screen.queryByRole('button', { name: 'Sign in' })).not.toBeInTheDocument()
  })

  it('shows the shared selected-profile mismatch recovery copy without forking onboarding provider logic', () => {
    render(
      <ProvidersStep
        {...makeProvidersStepProps({
          providerProfiles: makeProviderProfiles({
            activeProfileId: 'openrouter-work',
            profiles: [
              makeOpenAiProfile({ active: false }),
              makeOpenRouterProfile({
                profileId: 'openrouter-work',
                label: 'OpenRouter Work',
                active: true,
                migratedFromLegacy: false,
                migratedAt: null,
              }),
            ],
          }),
          runtimeSession: makeRuntimeSession({
            providerId: 'openai_codex',
            runtimeKind: 'openai_codex',
            phase: 'authenticated',
            phaseLabel: 'Authenticated',
            runtimeLabel: 'OpenAI Codex · Signed in',
            accountLabel: 'operator',
            sessionLabel: 'session-1',
            sessionId: 'session-1',
            accountId: 'acct-1',
            isAuthenticated: true,
            isSignedOut: false,
          }),
          hasSelectedProject: true,
          onStartLogin: vi.fn(async () => makeRuntimeSession()),
          onLogout: vi.fn(async () => makeRuntimeSession()),
        })}
      />,
    )

    expect(
      screen.getByText(
        'Settings now select provider profile OpenRouter Work (openrouter-work), but the persisted runtime session still reflects OpenAI Codex.',
      ),
    ).toBeVisible()
    expect(
      screen.getByText('Rebind the selected profile so durable runtime truth matches Settings.'),
    ).toBeVisible()
    expect(screen.getByText('OpenRouter Work')).toBeVisible()
    expect(screen.getAllByText('Active').length).toBeGreaterThan(0)
  })

  it('shows typed save errors while keeping the last truthful provider snapshot visible', () => {
    render(
      <ProvidersStep
        {...makeProvidersStepProps({
          providerProfilesSaveError: {
            code: 'provider_profiles_write_failed',
            message: 'Cadence could not save the selected provider profile.',
            retryable: true,
          },
        })}
      />,
    )

    expect(screen.getByText('Cadence could not save the selected provider profile.')).toBeVisible()
    expect(screen.getByText('OpenRouter')).toBeVisible()
    expect(screen.getByText('OpenAI Codex')).toBeVisible()
    expect(screen.getByText('Ready')).toBeVisible()
  })
})
