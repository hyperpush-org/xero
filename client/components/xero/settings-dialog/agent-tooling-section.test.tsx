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
  AgentToolExtensionCatalogDto,
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
    inputModalities: [],
    inputModalitiesSource: 'test_fixture_unreported',
    thinking: { supported: false, effortOptions: [], defaultEffort: null },
  }
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
  extensionCatalog?: AgentToolExtensionCatalogDto
}

const DEFAULT_EXTENSION_CATALOG: AgentToolExtensionCatalogDto = {
  schema: 'xero.agent_tool_extension_catalog.v1',
  appDataDirectory: '/Users/test/Library/Application Support/dev.sn0w.xero/tool-extensions',
  extensions: [
    {
      extensionId: 'demo.read',
      label: 'Demo reader',
      toolName: 'demo_read',
      enabled: false,
      eligible: true,
      installationHash: 'a'.repeat(64),
      permission: {
        permissionId: 'demo_read_permission',
        label: 'Read demo input',
        effectClass: 'observe',
        riskClass: 'low',
        auditLabel: 'demo_read',
        mutability: 'read_only',
        sandboxRequirement: 'read_only',
        approvalRequirement: 'policy',
        capabilityTags: ['demo', 'read'],
      },
      diagnostics: [],
    },
  ],
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
  pickToolExtensionFolder: ReturnType<typeof vi.fn>
  listAgentToolExtensions: ReturnType<typeof vi.fn>
  installAgentToolExtension: ReturnType<typeof vi.fn>
  setAgentToolExtensionEnabled: ReturnType<typeof vi.fn>
  removeAgentToolExtension: ReturnType<typeof vi.fn>
} {
  let current: AgentToolingSettingsDto = overrides.settings ?? makeSettings()
  const credentials = overrides.credentials ?? DEFAULT_CREDENTIALS
  const catalogs = overrides.catalogs ?? DEFAULT_CATALOGS
  let extensionCatalog = overrides.extensionCatalog ?? DEFAULT_EXTENSION_CATALOG
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
    pickToolExtensionFolder: vi.fn(async () => '/tmp/demo-extension'),
    listAgentToolExtensions: vi.fn(async () => extensionCatalog),
    installAgentToolExtension: vi.fn(async () => extensionCatalog),
    setAgentToolExtensionEnabled: vi.fn(async (request) => {
      extensionCatalog = {
        ...extensionCatalog,
        extensions: extensionCatalog.extensions.map((entry) =>
          entry.extensionId === request.extensionId
            ? { ...entry, enabled: request.enabled }
            : entry,
        ),
      }
      return extensionCatalog
    }),
    removeAgentToolExtension: vi.fn(async (request) => {
      extensionCatalog = {
        ...extensionCatalog,
        extensions: extensionCatalog.extensions.filter(
          (entry) => entry.extensionId !== request.extensionId,
        ),
      }
      return extensionCatalog
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

  it('saves the automatic agent routing preference without touching model tooling settings', async () => {
    const adapter = makeAdapter()
    const onAgentRoutingAutoSwitchChange = vi.fn(async () => undefined)

    render(
      <AgentToolingSection
        adapter={adapter}
        agentRoutingAutoSwitchEnabled={false}
        onAgentRoutingAutoSwitchChange={onAgentRoutingAutoSwitchChange}
      />,
    )

    const autoSwitch = await screen.findByRole('switch', {
      name: 'Auto-switch suggested agents',
    })
    expect(autoSwitch).not.toBeChecked()

    fireEvent.click(autoSwitch)

    await waitFor(() =>
      expect(onAgentRoutingAutoSwitchChange).toHaveBeenCalledWith(true),
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

  it('shows declared extension permissions and grants the exact permission when enabling', async () => {
    const adapter = makeAdapter()

    render(<AgentToolingSection adapter={adapter} />)

    expect(await screen.findByText('Read demo input')).toBeVisible()
    expect(screen.getByText('demo_read_permission')).toBeVisible()
    expect(screen.getByText(/read only sandbox/i)).toBeVisible()

    fireEvent.click(screen.getByRole('switch', { name: 'Enable Demo reader' }))

    await waitFor(() =>
      expect(adapter.setAgentToolExtensionEnabled).toHaveBeenCalledWith({
        extensionId: 'demo.read',
        enabled: true,
        permissionId: 'demo_read_permission',
      }),
    )
    expect(await screen.findByText('Enabled')).toBeVisible()
  })

  it('installs from an explicitly selected bundle and supports deterministic removal', async () => {
    const adapter = makeAdapter()

    render(<AgentToolingSection adapter={adapter} />)
    await screen.findByRole('button', { name: 'Install extension' })

    fireEvent.click(screen.getByRole('button', { name: 'Install extension' }))
    await waitFor(() =>
      expect(adapter.installAgentToolExtension).toHaveBeenCalledWith({
        sourceDirectory: '/tmp/demo-extension',
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Remove Demo reader' }))
    await waitFor(() =>
      expect(adapter.removeAgentToolExtension).toHaveBeenCalledWith({
        extensionId: 'demo.read',
      }),
    )
    expect(await screen.findByText('No tool extensions are installed.')).toBeVisible()
  })
})
