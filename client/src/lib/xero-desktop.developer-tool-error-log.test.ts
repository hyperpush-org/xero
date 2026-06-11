import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
  isTauri: mocks.isTauri,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

describe('XeroDesktopAdapter developer tool error log', () => {
  beforeEach(() => {
    mocks.invoke.mockReset()
    mocks.isTauri.mockReturnValue(true)
  })

  it('lists and clears through validated IPC contracts', async () => {
    const { XeroDesktopAdapter } = await import('./xero-desktop')
    const entry = {
      id: 'tool-error-1',
      occurredAt: '2026-06-02T14:05:00Z',
      source: 'tool_registry_v2_dispatch',
      projectId: 'project-1',
      agentSessionId: 'session-1',
      runId: 'run-1',
      turnIndex: 4,
      toolCallId: 'call-1',
      toolName: 'write',
      inputSha256: 'a'.repeat(64),
      inputJson: { path: 'src/main.rs' },
      inputRedacted: false,
      errorCode: 'write_failed',
      errorClass: 'retryable',
      errorCategory: 'retryable_provider_tool_failure',
      errorMessage: 'Write failed.',
      modelMessage: null,
      retryable: true,
      dispatchJson: { groupMode: 'sequential_mutating' },
      contextJson: { launchMode: 'local-source' },
      messagePreview: 'Write failed.',
    }

    mocks.invoke.mockResolvedValueOnce({
      databasePath: '/tmp/xero/development/tool-call-errors.sqlite',
      entries: [entry],
      projectIds: ['project-1'],
      totalCount: 1,
      limit: 100,
      offset: 0,
    })

    await expect(
      XeroDesktopAdapter.developerToolErrorLogList?.({ toolName: 'write' }),
    ).resolves.toEqual({
      databasePath: '/tmp/xero/development/tool-call-errors.sqlite',
      entries: [entry],
      projectIds: ['project-1'],
      totalCount: 1,
      limit: 100,
      offset: 0,
    })
    expect(mocks.invoke).toHaveBeenCalledWith('developer_tool_error_log_list', {
      request: { toolName: 'write' },
    })

    mocks.invoke.mockResolvedValueOnce({
      databasePath: '/tmp/xero/development/tool-call-errors.sqlite',
      clearedCount: 1,
    })

    await expect(XeroDesktopAdapter.developerToolErrorLogClear?.()).resolves.toEqual({
      databasePath: '/tmp/xero/development/tool-call-errors.sqlite',
      clearedCount: 1,
    })
    expect(mocks.invoke).toHaveBeenLastCalledWith(
      'developer_tool_error_log_clear',
      undefined,
    )
  })
})
