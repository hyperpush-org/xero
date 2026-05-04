/** @vitest-environment jsdom */

import { describe, expect, it } from 'vitest'

import {
  AGENT_WORKSPACE_DROP_TARGET_ID,
  resolveAgentWorkspaceDragEnd,
} from './agent-workspace-dnd-provider'

describe('resolveAgentWorkspaceDragEnd', () => {
  const paneSlots = [{ id: 'pane-a' }, { id: 'pane-b' }, { id: 'pane-c' }]

  it('returns null when nothing is dropped on', () => {
    expect(
      resolveAgentWorkspaceDragEnd(
        { activeId: 'pane-a', overId: null, activeData: undefined },
        paneSlots,
      ),
    ).toBeNull()
  })

  it('returns null when a session is dropped outside a target and outside the workspace', () => {
    expect(
      resolveAgentWorkspaceDragEnd(
        {
          activeId: 'session-7',
          overId: null,
          activeData: { type: 'session', sessionId: 'sess-7', title: 'Working' },
          droppedOnWorkspace: false,
        },
        paneSlots,
      ),
    ).toBeNull()
  })

  it('returns reorder when dragging a pane onto another pane', () => {
    expect(
      resolveAgentWorkspaceDragEnd(
        { activeId: 'pane-a', overId: 'pane-c', activeData: undefined },
        paneSlots,
      ),
    ).toEqual({ kind: 'reorder', activeId: 'pane-a', overId: 'pane-c' })
  })

  it('returns null when reorder target equals source', () => {
    expect(
      resolveAgentWorkspaceDragEnd(
        { activeId: 'pane-a', overId: 'pane-a', activeData: undefined },
        paneSlots,
      ),
    ).toBeNull()
  })

  it('returns open-session with the dropped pane index when a session lands on a pane', () => {
    expect(
      resolveAgentWorkspaceDragEnd(
        {
          activeId: 'session-7',
          overId: 'pane-b',
          activeData: { type: 'session', sessionId: 'sess-7', title: 'Working' },
        },
        paneSlots,
      ),
    ).toEqual({ kind: 'open-session', sessionId: 'sess-7', atIndex: 1 })
  })

  it('returns open-session with no index when the drop target is not a known pane', () => {
    expect(
      resolveAgentWorkspaceDragEnd(
        {
          activeId: 'session-7',
          overId: 'pane-area',
          activeData: { type: 'session', sessionId: 'sess-7', title: 'Working' },
        },
        paneSlots,
      ),
    ).toEqual({ kind: 'open-session', sessionId: 'sess-7', atIndex: undefined })
  })

  it('returns open-session with no index when a session lands on the workspace target', () => {
    expect(
      resolveAgentWorkspaceDragEnd(
        {
          activeId: 'session-7',
          overId: AGENT_WORKSPACE_DROP_TARGET_ID,
          activeData: { type: 'session', sessionId: 'sess-7', title: 'Working' },
        },
        paneSlots,
      ),
    ).toEqual({ kind: 'open-session', sessionId: 'sess-7', atIndex: undefined })
  })

  it('returns open-session when dnd-kit reports no target but the pointer ended inside the workspace', () => {
    expect(
      resolveAgentWorkspaceDragEnd(
        {
          activeId: 'session-7',
          overId: null,
          activeData: { type: 'session', sessionId: 'sess-7', title: 'Working' },
          droppedOnWorkspace: true,
        },
        paneSlots,
      ),
    ).toEqual({ kind: 'open-session', sessionId: 'sess-7', atIndex: undefined })
  })

  it('recognizes sidebar session drags from the draggable id when drag data is unavailable', () => {
    expect(
      resolveAgentWorkspaceDragEnd(
        {
          activeId: 'session-sess-7',
          overId: null,
          activeData: undefined,
          droppedOnWorkspace: true,
        },
        paneSlots,
      ),
    ).toEqual({ kind: 'open-session', sessionId: 'sess-7', atIndex: undefined })
  })
})
