import { render, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { listenMock } = vi.hoisted(() => ({
  listenMock: vi.fn(),
}))

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    listen: listenMock,
  }),
}))

vi.mock('@tauri-apps/api/event', () => ({
  TauriEvent: {
    DRAG_ENTER: 'tauri://drag-enter',
    DRAG_OVER: 'tauri://drag-over',
    DRAG_DROP: 'tauri://drag-drop',
    DRAG_LEAVE: 'tauri://drag-leave',
  },
}))

import { AgentPaneDropOverlay } from './agent-pane-drop-overlay'

describe('AgentPaneDropOverlay', () => {
  beforeEach(() => {
    listenMock.mockReset()
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    })
  })

  afterEach(() => {
    Reflect.deleteProperty(window, '__TAURI_INTERNALS__')
  })

  it('cleans up native path drop listeners without leaking Tauri unlisten rejections', async () => {
    const unlisteners = Array.from({ length: 4 }, () =>
      vi.fn(() =>
        Promise.reject(
          new TypeError("undefined is not an object (evaluating 'listeners[eventId].handlerId')"),
        ),
      ),
    )
    listenMock.mockImplementation(async () => {
      const unlisten = unlisteners[listenMock.mock.calls.length - 1]
      if (!unlisten) {
        throw new Error('Unexpected native drop listener registration.')
      }
      return unlisten
    })

    const { unmount } = render(
      <AgentPaneDropOverlay
        enabled
        onFilesDropped={vi.fn()}
        onPathsDropped={vi.fn()}
      >
        <div>Pane</div>
      </AgentPaneDropOverlay>,
    )

    await waitFor(() => expect(listenMock).toHaveBeenCalledTimes(4))

    unmount()
    await Promise.resolve()

    expect(unlisteners.map((unlisten) => unlisten.mock.calls.length)).toEqual([1, 1, 1, 1])
  })
})
