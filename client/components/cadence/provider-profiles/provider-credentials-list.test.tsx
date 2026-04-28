import { describe, expect, it, vi, beforeEach } from 'vitest'
import { fireEvent, render, screen, within, waitFor, act } from '@testing-library/react'
import { ProviderCredentialsList } from '@/components/cadence/provider-profiles/provider-credentials-list'
import type {
  ProviderCredentialDto,
  ProviderCredentialsSnapshotDto,
  RuntimeSessionView,
} from '@/src/lib/cadence-model'

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(async () => undefined),
}))

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

function makeRuntimeSession(overrides: Partial<RuntimeSessionView> = {}): RuntimeSessionView {
  return {
    projectId: 'project-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: null,
    sessionId: null,
    accountId: null,
    phase: 'idle',
    phaseLabel: 'Signed out',
    runtimeLabel: 'Runtime unavailable',
    accountLabel: 'Not signed in',
    sessionLabel: 'No session',
    callbackBound: null,
    authorizationUrl: null,
    redirectUri: null,
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-15T20:00:00.000Z',
    isAuthenticated: false,
    isLoginInProgress: false,
    needsManualInput: false,
    isSignedOut: true,
    isFailed: false,
    ...overrides,
  }
}

function getProviderCard(label: string): HTMLElement {
  const card = screen
    .getAllByText(label)
    .map((node) => node.closest('.rounded-lg'))
    .find((value): value is HTMLElement => value instanceof HTMLElement)
  if (!card) throw new Error(`Could not find card for ${label}`)
  return card
}

describe('ProviderCredentialsList', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders a row per provider preset with the right action button when no credentials exist', () => {
    render(
      <ProviderCredentialsList
        providerCredentials={makeSnapshot([])}
        providerCredentialsLoadStatus="ready"
        providerCredentialsLoadError={null}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
      />,
    )

    expect(within(getProviderCard('OpenAI Codex')).getByRole('button', { name: /sign in/i })).toBeInTheDocument()
    expect(within(getProviderCard('OpenRouter')).getByRole('button', { name: /configure/i })).toBeInTheDocument()
    expect(within(getProviderCard('Anthropic')).getByRole('button', { name: /configure/i })).toBeInTheDocument()
    expect(within(getProviderCard('Ollama')).getByRole('button', { name: /configure/i })).toBeInTheDocument()
    expect(within(getProviderCard('Amazon Bedrock')).getByRole('button', { name: /configure/i })).toBeInTheDocument()
  })

  it('shows Ready badge for an api_key provider with stored credential', () => {
    const credentials = makeSnapshot([
      makeCredential({ providerId: 'openrouter', readinessProof: 'stored_secret' }),
    ])
    render(
      <ProviderCredentialsList
        providerCredentials={credentials}
        providerCredentialsLoadStatus="ready"
        providerCredentialsLoadError={null}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
      />,
    )

    expect(within(getProviderCard('OpenRouter')).getByText('Ready')).toBeInTheDocument()
  })

  it('shows Signed in badge and Sign out button for OAuth provider with active session', () => {
    const credentials = makeSnapshot([
      makeCredential({
        providerId: 'openai_codex',
        kind: 'oauth_session',
        hasOauthAccessToken: true,
        oauthAccountId: 'acct-1',
        oauthSessionId: 'sess-1',
        readinessProof: 'oauth_session',
      }),
    ])
    render(
      <ProviderCredentialsList
        providerCredentials={credentials}
        providerCredentialsLoadStatus="ready"
        providerCredentialsLoadError={null}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
      />,
    )

    expect(within(getProviderCard('OpenAI Codex')).getByText('Signed in')).toBeInTheDocument()
    expect(within(getProviderCard('OpenAI Codex')).getByRole('button', { name: /sign out/i })).toBeInTheDocument()
  })

  it('opens an inline editor for an api_key provider, validates the api key, and submits an upsert', async () => {
    const onUpsert = vi.fn(async () => makeSnapshot([]))
    render(
      <ProviderCredentialsList
        providerCredentials={makeSnapshot([])}
        providerCredentialsLoadStatus="ready"
        providerCredentialsLoadError={null}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
        onUpsertProviderCredential={onUpsert}
      />,
    )

    const card = getProviderCard('OpenRouter')
    await act(async () => {
      fireEvent.click(within(card).getByRole('button', { name: /configure/i }))
    })
    await act(async () => {
      fireEvent.click(within(card).getByRole('button', { name: /save/i }))
    })
    expect(within(card).getByText('API key is required.')).toBeInTheDocument()

    fireEvent.change(within(card).getByLabelText(/API key/i), {
      target: { value: 'sk-or-test' },
    })
    await act(async () => {
      fireEvent.click(within(card).getByRole('button', { name: /save/i }))
    })

    await waitFor(() => expect(onUpsert).toHaveBeenCalledTimes(1))
    expect(onUpsert).toHaveBeenCalledWith(
      expect.objectContaining({
        providerId: 'openrouter',
        kind: 'api_key',
        apiKey: 'sk-or-test',
      }),
    )
  })

  it('calls deleteProviderCredential when removing an existing api_key credential', async () => {
    const onDelete = vi.fn(async () => makeSnapshot([]))
    const credentials = makeSnapshot([
      makeCredential({ providerId: 'openrouter' }),
    ])
    render(
      <ProviderCredentialsList
        providerCredentials={credentials}
        providerCredentialsLoadStatus="ready"
        providerCredentialsLoadError={null}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
        onDeleteProviderCredential={onDelete}
      />,
    )

    const card = getProviderCard('OpenRouter')
    await act(async () => {
      fireEvent.click(within(card).getByRole('button', { name: /edit/i }))
    })
    await act(async () => {
      fireEvent.click(within(card).getByRole('button', { name: /remove/i }))
    })
    await waitFor(() => expect(onDelete).toHaveBeenCalledWith('openrouter'))
  })

  it('triggers startOAuthLogin when clicking Sign in on an OAuth provider', async () => {
    const onStart = vi.fn(async () => makeRuntimeSession())
    render(
      <ProviderCredentialsList
        providerCredentials={makeSnapshot([])}
        providerCredentialsLoadStatus="ready"
        providerCredentialsLoadError={null}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
        onStartOAuthLogin={onStart}
      />,
    )

    const card = getProviderCard('OpenAI Codex')
    await act(async () => {
      fireEvent.click(within(card).getByRole('button', { name: /sign in/i }))
    })
    await waitFor(() => expect(onStart).toHaveBeenCalledWith({ providerId: 'openai_codex' }))
  })

  it('calls deleteProviderCredential when signing out an OAuth provider', async () => {
    const onDelete = vi.fn(async () => makeSnapshot([]))
    const credentials = makeSnapshot([
      makeCredential({
        providerId: 'openai_codex',
        kind: 'oauth_session',
        hasOauthAccessToken: true,
        readinessProof: 'oauth_session',
      }),
    ])
    render(
      <ProviderCredentialsList
        providerCredentials={credentials}
        providerCredentialsLoadStatus="ready"
        providerCredentialsLoadError={null}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
        onDeleteProviderCredential={onDelete}
      />,
    )

    const card = getProviderCard('OpenAI Codex')
    await act(async () => {
      fireEvent.click(within(card).getByRole('button', { name: /sign out/i }))
    })
    await waitFor(() => expect(onDelete).toHaveBeenCalledWith('openai_codex'))
  })

  it('auto-refreshes when load status is idle', async () => {
    const onRefresh = vi.fn(async () => makeSnapshot([]))
    render(
      <ProviderCredentialsList
        providerCredentials={null}
        providerCredentialsLoadStatus="idle"
        providerCredentialsLoadError={null}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
        onRefreshProviderCredentials={onRefresh}
      />,
    )

    await waitFor(() => expect(onRefresh).toHaveBeenCalledTimes(1))
  })

  it('does not show "make active" controls anywhere', () => {
    render(
      <ProviderCredentialsList
        providerCredentials={makeSnapshot([
          makeCredential({ providerId: 'openrouter' }),
          makeCredential({ providerId: 'anthropic' }),
        ])}
        providerCredentialsLoadStatus="ready"
        providerCredentialsLoadError={null}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
      />,
    )

    expect(screen.queryByText(/make active/i)).not.toBeInTheDocument()
    expect(screen.queryByRole('radio')).not.toBeInTheDocument()
  })

  it('surfaces a load error in an alert', () => {
    render(
      <ProviderCredentialsList
        providerCredentials={null}
        providerCredentialsLoadStatus="error"
        providerCredentialsLoadError={{
          code: 'load_failed',
          message: 'Could not load provider credentials.',
          retryable: true,
        }}
        providerCredentialsSaveStatus="idle"
        providerCredentialsSaveError={null}
      />,
    )

    expect(screen.getByText('Could not load provider credentials.')).toBeInTheDocument()
  })
})
