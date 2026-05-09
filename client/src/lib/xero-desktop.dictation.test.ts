import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
  listen: vi.fn(),
  channels: [] as Array<{ onmessage?: (message: unknown) => void }>,
}))

function flushRuntimeStreamDelivery(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0))
}

vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {
    onmessage?: (message: unknown) => void

    constructor() {
      mocks.channels.push(this)
    }
  },
  invoke: mocks.invoke,
  isTauri: mocks.isTauri,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: mocks.listen,
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

describe('XeroDesktopAdapter dictation', () => {
  beforeEach(() => {
    mocks.invoke.mockReset()
    mocks.isTauri.mockReturnValue(true)
    mocks.listen.mockReset()
    mocks.channels.length = 0
  })

  it('normalizes mocked status responses through the adapter contract', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    mocks.invoke.mockResolvedValueOnce({
      platform: 'macos',
      defaultLocale: ' en_US ',
      modern: {
        available: false,
        compiled: false,
        runtimeSupported: false,
        reason: ' modern_sdk_unavailable ',
      },
      legacy: {
        available: true,
        compiled: true,
        runtimeSupported: true,
        reason: '',
      },
      microphonePermission: 'not_determined',
      speechPermission: 'authorized',
      activeSession: null,
    })

    await expect(XeroDesktopAdapter.speechDictationStatus?.()).resolves.toEqual({
      platform: 'macos',
      osVersion: null,
      defaultLocale: 'en_US',
      supportedLocales: [],
      modern: {
        available: false,
        compiled: false,
        runtimeSupported: false,
        reason: 'modern_sdk_unavailable',
      },
      legacy: {
        available: true,
        compiled: true,
        runtimeSupported: true,
        reason: null,
      },
      modernAssets: {
        status: 'unknown',
        locale: null,
        reason: null,
      },
      microphonePermission: 'not_determined',
      speechPermission: 'authorized',
      activeSession: null,
    })
    expect(mocks.invoke).toHaveBeenCalledWith('speech_dictation_status', undefined)
  })

  it('loads and updates dictation settings through the adapter contract', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    mocks.invoke.mockResolvedValueOnce({
      enginePreference: 'automatic',
      privacyMode: 'on_device_preferred',
      locale: ' en_US ',
      updatedAt: '2026-04-26T12:00:00Z',
    })

    await expect(XeroDesktopAdapter.speechDictationSettings?.()).resolves.toEqual({
      enginePreference: 'automatic',
      privacyMode: 'on_device_preferred',
      locale: 'en_US',
      updatedAt: '2026-04-26T12:00:00Z',
    })
    expect(mocks.invoke).toHaveBeenCalledWith('speech_dictation_settings', undefined)

    mocks.invoke.mockResolvedValueOnce({
      enginePreference: 'legacy',
      privacyMode: 'allow_network',
      locale: null,
      updatedAt: '2026-04-26T12:01:00Z',
    })

    await expect(
      XeroDesktopAdapter.speechDictationUpdateSettings?.({
        enginePreference: 'legacy',
        privacyMode: 'allow_network',
        locale: null,
      }),
    ).resolves.toMatchObject({
      enginePreference: 'legacy',
      privacyMode: 'allow_network',
      locale: null,
    })
    expect(mocks.invoke).toHaveBeenLastCalledWith('speech_dictation_update_settings', {
      request: {
        enginePreference: 'legacy',
        privacyMode: 'allow_network',
        locale: null,
      },
    })
  })

  it('starts dictation with a typed channel and normalizes streamed events', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')
    const handler = vi.fn()

    mocks.invoke.mockImplementationOnce(async (_command, args) => {
      args.request.channel.onmessage({
        kind: 'started',
        sessionId: 'session-1',
        engine: 'legacy',
        locale: 'en_US',
      })

      return {
        sessionId: 'session-1',
        engine: 'legacy',
        locale: 'en_US',
      }
    })

    const session = await XeroDesktopAdapter.speechDictationStart?.(
      {
        locale: ' ',
        contextualPhrases: ['Xero'],
      },
      handler,
    )

    expect(session?.response).toEqual({
      sessionId: 'session-1',
      engine: 'legacy',
      locale: 'en_US',
    })
    expect(handler).toHaveBeenCalledWith({
      kind: 'started',
      sessionId: 'session-1',
      engine: 'legacy',
      locale: 'en_US',
    })
    expect(mocks.invoke).toHaveBeenCalledWith('speech_dictation_start', {
      request: {
        locale: null,
        enginePreference: 'automatic',
        privacyMode: 'on_device_preferred',
        contextualPhrases: ['Xero'],
        channel: mocks.channels[0],
      },
    })
  })

  it('stops and cancels dictation through idempotent commands', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    mocks.invoke.mockResolvedValue(undefined)

    await XeroDesktopAdapter.speechDictationStop?.()
    await XeroDesktopAdapter.speechDictationCancel?.()

    expect(mocks.invoke).toHaveBeenNthCalledWith(1, 'speech_dictation_stop', undefined)
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, 'speech_dictation_cancel', undefined)
  })
})

describe('XeroDesktopAdapter code history', () => {
  beforeEach(() => {
    mocks.invoke.mockReset()
    mocks.isTauri.mockReturnValue(true)
    mocks.listen.mockReset()
    mocks.channels.length = 0
  })

  it('invokes selective undo and return session to here through the new frontend contract names', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')

    mocks.invoke.mockResolvedValueOnce({
      operation: makeCodeHistoryOperation({
        operationId: 'code-undo-1',
        mode: 'selective_undo',
      }),
    })

    await expect(
      XeroDesktopAdapter.applySelectiveUndo({
        projectId: 'project-1',
        operationId: 'code-undo-1',
        target: {
          targetKind: 'change_group',
          targetId: 'code-change-1',
          changeGroupId: 'code-change-1',
          hunkIds: [],
        },
        expectedWorkspaceEpoch: 7,
      }),
    ).resolves.toMatchObject({
      operation: {
        mode: 'selective_undo',
        status: 'completed',
      },
    })

    expect(mocks.invoke).toHaveBeenLastCalledWith('apply_selective_undo', {
      request: {
        projectId: 'project-1',
        operationId: 'code-undo-1',
        target: {
          targetKind: 'change_group',
          targetId: 'code-change-1',
          changeGroupId: 'code-change-1',
          hunkIds: [],
        },
        expectedWorkspaceEpoch: 7,
      },
    })

    mocks.invoke.mockResolvedValueOnce({
      operation: makeCodeHistoryOperation({
        operationId: 'return-session-1',
        mode: 'session_rollback',
        target: {
          targetKind: 'run_boundary',
          targetId: 'run-1:boundary-1',
        },
      }),
    })

    await expect(
      XeroDesktopAdapter.returnSessionToHere({
        projectId: 'project-1',
        operationId: 'return-session-1',
        target: {
          targetKind: 'run_boundary',
          targetId: 'run-1:boundary-1',
          agentSessionId: 'agent-session-1',
          boundaryId: 'boundary-1',
          runId: 'run-1',
          changeGroupId: 'code-change-1',
        },
        expectedWorkspaceEpoch: 8,
      }),
    ).resolves.toMatchObject({
      operation: {
        mode: 'session_rollback',
      },
    })

    expect(mocks.invoke).toHaveBeenLastCalledWith('apply_session_rollback', {
      request: {
        projectId: 'project-1',
        operationId: 'return-session-1',
        target: {
          targetKind: 'run_boundary',
          targetId: 'run-1:boundary-1',
          agentSessionId: 'agent-session-1',
          boundaryId: 'boundary-1',
          runId: 'run-1',
          changeGroupId: 'code-change-1',
        },
        expectedWorkspaceEpoch: 8,
      },
    })
    expect('applyCodeRollback' in XeroDesktopAdapter).toBe(false)
  })
})

describe('XeroDesktopAdapter event listeners', () => {
  beforeEach(() => {
    mocks.invoke.mockReset()
    mocks.isTauri.mockReturnValue(true)
    mocks.listen.mockReset()
    mocks.channels.length = 0
  })

  it('returns idempotent unlisteners that absorb Tauri teardown rejections', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')
    const rawUnlisten = vi.fn(() =>
      Promise.reject(new TypeError("undefined is not an object (evaluating 'listeners[eventId].handlerId')")),
    )

    mocks.listen.mockResolvedValueOnce(rawUnlisten)

    const unlisten = await XeroDesktopAdapter.onRuntimeRunUpdated(vi.fn(), vi.fn())

    expect(unlisten()).toBeUndefined()
    expect(unlisten()).toBeUndefined()
    await Promise.resolve()

    expect(rawUnlisten).toHaveBeenCalledTimes(1)
  })
})

function makeCodeHistoryOperation(overrides: Record<string, unknown> = {}) {
  return {
    projectId: 'project-1',
    operationId: 'code-undo-1',
    mode: 'selective_undo',
    status: 'completed',
    target: {
      targetKind: 'change_group',
      targetId: 'code-change-1',
      hunkIds: [],
    },
    affectedPaths: ['src/app.ts'],
    conflicts: [],
    resultCommitId: 'code-commit-undo-1',
    resultChangeGroupId: 'code-change-undo-1',
    createdAt: '2026-05-06T12:00:00Z',
    updatedAt: '2026-05-06T12:00:01Z',
    ...overrides,
  }
}

function makeRuntimeStreamItem(sequence: number, text: string) {
  return {
    kind: 'transcript',
    runId: 'run-1',
    sequence,
    sessionId: 'session-1',
    flowId: null,
    text,
    transcriptRole: 'assistant',
    toolCallId: null,
    toolName: null,
    toolState: null,
    toolSummary: null,
    skillId: null,
    skillStage: null,
    skillResult: null,
    skillSource: null,
    skillCacheStatus: null,
    skillDiagnostic: null,
    actionId: null,
    boundaryId: null,
    actionType: null,
    title: null,
    detail: null,
    code: null,
    message: null,
    retryable: null,
    createdAt: '2026-04-30T22:41:55Z',
  }
}

describe('XeroDesktopAdapter runtime stream', () => {
  beforeEach(() => {
    mocks.invoke.mockReset()
    mocks.isTauri.mockReturnValue(true)
    mocks.listen.mockReset()
    mocks.channels.length = 0
  })

  it('drops stale replayed channel items without surfacing an adapter warning', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')
    const handler = vi.fn()
    const onError = vi.fn()

    mocks.invoke.mockImplementationOnce(async (_command, args) => {
      args.request.channel.onmessage(makeRuntimeStreamItem(3, 'newest replay item'))
      args.request.channel.onmessage(makeRuntimeStreamItem(2, 'older replay item'))
      args.request.channel.onmessage(makeRuntimeStreamItem(3, 'duplicate replay item'))

      return {
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        runtimeKind: 'openai_codex',
        runId: 'run-1',
        sessionId: 'session-1',
        flowId: null,
        subscribedItemKinds: ['transcript'],
      }
    })

    const subscription = await XeroDesktopAdapter.subscribeRuntimeStream(
      'project-1',
      'agent-session-main',
      ['transcript'],
      handler,
      onError,
    )
    await flushRuntimeStreamDelivery()

    mocks.channels[0].onmessage?.(makeRuntimeStreamItem(2, 'late stale replay item'))
    await flushRuntimeStreamDelivery()

    expect(handler).toHaveBeenCalledTimes(1)
    expect(handler).toHaveBeenCalledWith(
      expect.objectContaining({
        projectId: 'project-1',
        runId: 'run-1',
        item: expect.objectContaining({
          sequence: 3,
          text: 'newest replay item',
        }),
      }),
    )
    expect(onError).not.toHaveBeenCalled()

    subscription.unsubscribe()
  })
})
