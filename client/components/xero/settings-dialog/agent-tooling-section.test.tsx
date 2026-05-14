import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { invokeMock, isTauriMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  isTauriMock: vi.fn(() => true),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
  isTauri: isTauriMock,
}))

import {
  AgentToolingSection,
  type AgentToolingSettingsAdapter,
} from '@/components/xero/settings-dialog/agent-tooling-section'
import type {
  AgentToolingSettingsDto,
  ProviderCredentialDto,
  ProviderCredentialsSnapshotDto,
  ProviderModelCatalogDto,
  ProviderModelDto,
  RuntimeProviderIdDto,
} from '@/src/lib/xero-model'

function makeSettings(overrides: Partial<AgentToolingSettingsDto> = {}): AgentToolingSettingsDto {
  return {
    globalDefault: 'balanced',
    modelOverrides: [],
    updatedAt: '2026-05-10T12:00:00Z',
    ...overrides,
  }
}

function makeCredential(
  providerId: RuntimeProviderIdDto,
  overrides: Partial<ProviderCredentialDto> = {},
): ProviderCredentialDto {
  return {
    providerId,
    kind: 'api_key',
    hasApiKey: true,
    hasOauthAccessToken: false,
    readinessProof: 'stored_secret',
    updatedAt: '2026-05-10T10:00:00Z',
    ...overrides,
  }
}

function makeModel(modelId: string, displayName?: string): ProviderModelDto {
  return {
    modelId,
    displayName: displayName ?? modelId,
    thinking: { supported: false, effortOptions: [] },
  } as ProviderModelDto
}

function makeCatalog(
  providerId: RuntimeProviderIdDto,
  profileId: string,
  models: ProviderModelDto[],
): ProviderModelCatalogDto {
  return {
    contractVersion: 1,
    profileId,
    providerId,
    configuredModelId: models[0]?.modelId ?? 'configured-model',
    source: 'manual',
    models,
    contractDiagnostics: [],
  } as ProviderModelCatalogDto
}

interface AdapterOverrides {
  settings?: AgentToolingSettingsDto
  updateError?: Error
  credentials?: ProviderCredentialsSnapshotDto
  catalogs?: Record<string, ProviderModelCatalogDto>
}

const DEFAULT_CREDENTIALS: ProviderCredentialsSnapshotDto = {
  credentials: [makeCredential('anthropic'), makeCredential('openrouter')],
}

const DEFAULT_CATALOGS: Record<string, ProviderModelCatalogDto> = {
  'anthropic-default': makeCatalog('anthropic', 'anthropic-default', [
    makeModel('claude-opus-4-7', 'Claude Opus 4.7'),
    makeModel('claude-sonnet-4-6', 'Claude Sonnet 4.6'),
  ]),
  'openrouter-default': makeCatalog('openrouter', 'openrouter-default', [
    makeModel('kimi-k2', 'Kimi K2'),
  ]),
}

function makeAdapter(
  overrides: AdapterOverrides = {},
): AgentToolingSettingsAdapter & {
  agentToolingSettings: ReturnType<typeof vi.fn>
  agentToolingUpdateSettings: ReturnType<typeof vi.fn>
  listProviderCredentials: ReturnType<typeof vi.fn>
  getProviderModelCatalog: ReturnType<typeof vi.fn>
} {
  let current: AgentToolingSettingsDto = overrides.settings ?? makeSettings()
  const credentials = overrides.credentials ?? DEFAULT_CREDENTIALS
  const catalogs = overrides.catalogs ?? DEFAULT_CATALOGS
  return {
    isDesktopRuntime: vi.fn(() => true),
    agentToolingSettings: vi.fn(async () => current),
    agentToolingUpdateSettings: vi.fn(async (request) => {
      if (overrides.updateError) {
        throw overrides.updateError
      }

      const next: AgentToolingSettingsDto = {
        globalDefault: request.globalDefault ?? current.globalDefault,
        modelOverrides: applyOverrides(current.modelOverrides, request.modelOverrides ?? []),
        updatedAt: '2026-05-10T13:00:00Z',
      }
      current = next
      return next
    }),
    listProviderCredentials: vi.fn(async () => credentials),
    getProviderModelCatalog: vi.fn(async (profileId: string) => {
      const catalog = catalogs[profileId]
      if (!catalog) throw new Error(`Missing catalog ${profileId}`)
      return catalog
    }),
  }
}

function applyOverrides(
  current: AgentToolingSettingsDto['modelOverrides'],
  patch: { providerId: string; modelId: string; style?: 'conservative' | 'balanced' | 'declarative_first' | null }[],
): AgentToolingSettingsDto['modelOverrides'] {
  const map = new Map(current.map((entry) => [`${entry.providerId}::${entry.modelId}`, entry]))
  for (const item of patch) {
    const key = `${item.providerId}::${item.modelId}`
    if (item.style == null) {
      map.delete(key)
    } else {
      map.set(key, {
        providerId: item.providerId,
        modelId: item.modelId,
        style: item.style,
        updatedAt: '2026-05-10T13:00:00Z',
      })
    }
  }
  return [...map.values()].sort((left, right) => {
    const providerCompare = left.providerId.localeCompare(right.providerId)
    if (providerCompare !== 0) return providerCompare
    return left.modelId.localeCompare(right.modelId)
  })
}

describe('AgentToolingSection', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    isTauriMock.mockReturnValue(true)
  })

  it('renders the desktop-required notice when no adapter is wired up', async () => {
    render(<AgentToolingSection />)
    expect(await screen.findByText('Desktop runtime required')).toBeVisible()
  })

  it('loads the saved global default from the backend on mount', async () => {
    const adapter = makeAdapter({ settings: makeSettings({ globalDefault: 'declarative_first' }) })

    render(<AgentToolingSection adapter={adapter} />)

    await waitFor(() => expect(adapter.agentToolingSettings).toHaveBeenCalled())
    const declarative = await screen.findByRole('radio', { name: 'Declarative-first' })
    expect(declarative).toBeChecked()
  })

  it('saves a new global default through the adapter when the user picks a different mode', async () => {
    const adapter = makeAdapter()

    render(<AgentToolingSection adapter={adapter} />)
    await screen.findByRole('radio', { name: 'Balanced' })

    fireEvent.click(screen.getByRole('radio', { name: 'Conservative' }))

    await waitFor(() =>
      expect(adapter.agentToolingUpdateSettings).toHaveBeenCalledWith({
        globalDefault: 'conservative',
        modelOverrides: [],
      }),
    )
    expect(screen.getByRole('radio', { name: 'Conservative' })).toBeChecked()
  })

  it('saves the tool call grouping display preference without touching model tooling settings', async () => {
    const adapter = makeAdapter()
    const onToolCallGroupingPreferenceChange = vi.fn(async () => undefined)

    render(
      <AgentToolingSection
        adapter={adapter}
        toolCallGroupingPreference="grouped"
        onToolCallGroupingPreferenceChange={onToolCallGroupingPreferenceChange}
      />,
    )

    const groupingSwitch = await screen.findByRole('switch', {
      name: 'Group completed tool calls',
    })
    expect(groupingSwitch).toBeChecked()

    fireEvent.click(groupingSwitch)

    await waitFor(() =>
      expect(onToolCallGroupingPreferenceChange).toHaveBeenCalledWith('separate'),
    )
    expect(adapter.agentToolingUpdateSettings).not.toHaveBeenCalled()
  })

  it('renders saved per-model overrides and updates an override style through the adapter', async () => {
    const adapter = makeAdapter({
      settings: makeSettings({
        modelOverrides: [
          {
            providerId: 'anthropic',
            modelId: 'claude-opus-4-7',
            style: 'declarative_first',
            updatedAt: '2026-05-10T11:00:00Z',
          },
        ],
      }),
    })

    render(<AgentToolingSection adapter={adapter} />)

    const overrideList = await screen.findByLabelText('Per-model overrides')
    expect(within(overrideList).getByText('Claude Opus 4.7')).toBeVisible()
    expect(within(overrideList).getByText('Anthropic')).toBeVisible()

    fireEvent.click(
      screen.getByRole('combobox', { name: 'Style for Anthropic Claude Opus 4.7' }),
    )
    fireEvent.click(await screen.findByRole('option', { name: 'Conservative' }))

    await waitFor(() =>
      expect(adapter.agentToolingUpdateSettings).toHaveBeenCalledWith({
        modelOverrides: [
          { providerId: 'anthropic', modelId: 'claude-opus-4-7', style: 'conservative' },
        ],
      }),
    )
  })

  it('removes a per-model override when the user clicks the trash button', async () => {
    const adapter = makeAdapter({
      settings: makeSettings({
        modelOverrides: [
          {
            providerId: 'openrouter',
            modelId: 'kimi-k2',
            style: 'balanced',
            updatedAt: '2026-05-10T11:00:00Z',
          },
        ],
      }),
    })

    render(<AgentToolingSection adapter={adapter} />)

    const removeButton = await screen.findByRole('button', {
      name: 'Remove override for OpenRouter Kimi K2',
    })
    fireEvent.click(removeButton)

    await waitFor(() =>
      expect(adapter.agentToolingUpdateSettings).toHaveBeenCalledWith({
        modelOverrides: [
          { providerId: 'openrouter', modelId: 'kimi-k2', style: null },
        ],
      }),
    )
    await waitFor(() =>
      expect(
        screen.queryByRole('button', { name: /Remove override for OpenRouter/i }),
      ).not.toBeInTheDocument(),
    )
  })

  it('adds a new override via the form, picking a configured model from the dropdown', async () => {
    const adapter = makeAdapter()

    render(<AgentToolingSection adapter={adapter} />)
    await waitFor(() => expect(adapter.listProviderCredentials).toHaveBeenCalled())
    await waitFor(() =>
      expect(adapter.getProviderModelCatalog).toHaveBeenCalledWith('anthropic-default', {
        forceRefresh: false,
      }),
    )

    fireEvent.click(await screen.findByRole('combobox', { name: 'Model' }))
    fireEvent.click(await screen.findByRole('option', { name: 'Claude Sonnet 4.6' }))
    fireEvent.click(screen.getByRole('button', { name: /Add override/i }))

    await waitFor(() =>
      expect(adapter.agentToolingUpdateSettings).toHaveBeenCalledWith({
        modelOverrides: [
          { providerId: 'anthropic', modelId: 'claude-sonnet-4-6', style: 'balanced' },
        ],
      }),
    )
  })

  it('falls back to a configure-providers hint when no provider credentials are set up', async () => {
    const adapter = makeAdapter({
      credentials: { credentials: [] },
      catalogs: {},
    })

    render(<AgentToolingSection adapter={adapter} />)

    expect(
      await screen.findByText(/Configure a provider in/i, { exact: false }),
    ).toBeVisible()
    expect(screen.queryByRole('button', { name: /Add override/i })).not.toBeInTheDocument()
  })

  it('surfaces a load error from the adapter without crashing the panel', async () => {
    const adapter: AgentToolingSettingsAdapter = {
      isDesktopRuntime: vi.fn(() => true),
      agentToolingSettings: vi.fn(async () => {
        throw new Error('connection refused')
      }),
      agentToolingUpdateSettings: vi.fn(),
    }

    render(<AgentToolingSection adapter={adapter} />)

    expect(await screen.findByText('connection refused')).toBeVisible()
    // Falls back to the balanced default so the panel stays interactive.
    expect(screen.getByRole('radio', { name: 'Balanced' })).toBeChecked()
  })

  it('shows a save error and reverts to the previous global default if the update fails', async () => {
    const adapter = makeAdapter({ updateError: new Error('write failed') })

    render(<AgentToolingSection adapter={adapter} />)
    await screen.findByRole('radio', { name: 'Balanced' })

    fireEvent.click(screen.getByRole('radio', { name: 'Declarative-first' }))

    expect(await screen.findByText('write failed')).toBeVisible()
    await waitFor(() => expect(screen.getByRole('radio', { name: 'Balanced' })).toBeChecked())
  })
})
