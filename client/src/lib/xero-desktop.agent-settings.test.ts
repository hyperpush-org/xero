import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
  listen: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {},
  invoke: mocks.invoke,
  isTauri: mocks.isTauri,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: mocks.listen,
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

describe('XeroDesktopAdapter agent settings', () => {
  beforeEach(() => {
    mocks.invoke.mockReset()
    mocks.isTauri.mockReturnValue(true)
    mocks.listen.mockReset()
  })

  it('loads and updates Agent Tooling settings with normalized command payloads', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    mocks.invoke.mockResolvedValueOnce({
      globalDefault: 'balanced',
      modelOverrides: [],
      updatedAt: null,
    })
    await expect(XeroDesktopAdapter.agentToolingSettings?.()).resolves.toEqual({
      globalDefault: 'balanced',
      modelOverrides: [],
      updatedAt: null,
    })
    expect(mocks.invoke).toHaveBeenCalledWith('agent_tooling_settings', undefined)

    mocks.invoke.mockResolvedValueOnce({
      globalDefault: 'conservative',
      modelOverrides: [
        {
          providerId: 'provider-a',
          modelId: 'model-a',
          style: 'declarative_first',
          updatedAt: '2026-07-17T20:00:00Z',
        },
      ],
      updatedAt: '2026-07-17T20:00:01Z',
    })
    await expect(
      XeroDesktopAdapter.agentToolingUpdateSettings?.({
        globalDefault: 'conservative',
        modelOverrides: [
          {
            providerId: ' provider-a ',
            modelId: ' model-a ',
            style: 'declarative_first',
          },
        ],
      }),
    ).resolves.toMatchObject({
      globalDefault: 'conservative',
      modelOverrides: [{ providerId: 'provider-a', modelId: 'model-a' }],
    })
    expect(mocks.invoke).toHaveBeenLastCalledWith('agent_tooling_update_settings', {
      request: {
        globalDefault: 'conservative',
        modelOverrides: [
          {
            providerId: 'provider-a',
            modelId: 'model-a',
            style: 'declarative_first',
          },
        ],
      },
    })
  })

  it('sets and resets custom-agent default models through the typed command', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    mocks.invoke.mockResolvedValueOnce({
      defaultModel: {
        providerId: 'anthropic',
        providerProfileId: 'work',
        modelId: 'claude-sonnet-4-5',
        selectionKey: 'anthropic:claude-sonnet-4-5',
        thinkingEffort: 'high',
      },
    })
    await expect(
      XeroDesktopAdapter.setAgentDefaultModel({
        projectId: ' project-1 ',
        ref: {
          kind: 'custom',
          definitionId: ' reviewer ',
          version: 2,
        },
        defaultModel: {
          providerId: ' anthropic ',
          providerProfileId: ' work ',
          modelId: ' claude-sonnet-4-5 ',
          selectionKey: ' anthropic:claude-sonnet-4-5 ',
          thinkingEffort: 'high',
        },
      }),
    ).resolves.toMatchObject({
      defaultModel: {
        providerId: 'anthropic',
        modelId: 'claude-sonnet-4-5',
      },
    })
    expect(mocks.invoke).toHaveBeenLastCalledWith('set_agent_default_model', {
      request: {
        projectId: 'project-1',
        ref: {
          kind: 'custom',
          definitionId: 'reviewer',
          version: 2,
        },
        defaultModel: {
          providerId: 'anthropic',
          providerProfileId: 'work',
          modelId: 'claude-sonnet-4-5',
          selectionKey: 'anthropic:claude-sonnet-4-5',
          thinkingEffort: 'high',
        },
      },
    })

    mocks.invoke.mockResolvedValueOnce({ defaultModel: null })
    await expect(
      XeroDesktopAdapter.setAgentDefaultModel({
        projectId: 'project-1',
        ref: { kind: 'built_in', runtimeAgentId: 'engineer', version: 1 },
        defaultModel: null,
      }),
    ).resolves.toEqual({ defaultModel: null })
  })

  it('rejects malformed requests before invoke and malformed native responses after invoke', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    expect(() =>
      XeroDesktopAdapter.agentToolingUpdateSettings?.({
        modelOverrides: [
          {
            providerId: '',
            modelId: 'model-a',
            style: 'balanced',
          },
        ],
      }),
    ).toThrow()
    expect(mocks.invoke).not.toHaveBeenCalled()

    mocks.invoke.mockResolvedValueOnce({
      globalDefault: 'unsupported',
      modelOverrides: [],
    })
    await expect(XeroDesktopAdapter.agentToolingSettings?.()).rejects.toThrow()
    expect(mocks.invoke).toHaveBeenCalledTimes(1)
  })
})
