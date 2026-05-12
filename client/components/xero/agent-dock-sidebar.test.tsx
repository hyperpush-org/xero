import type { ComponentProps } from 'react'
import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { AgentDockSidebar } from './agent-dock-sidebar'
import { createXeroHighChurnStore } from '@/src/features/xero/use-xero-desktop-state/high-churn-store'
import type { AgentSessionView } from '@/src/lib/xero-model'

interface CapturedRuntimeProps {
  inSidebar?: boolean
  sidebarSessions?: readonly AgentSessionView[]
  onCloseSidebar?: () => void
  onSelectSidebarSession?: (id: string) => void
  onCreateSession?: () => void
  isCreatingSession?: boolean
  agentCreateCanvasIncluded?: boolean
  onStartWorkflowAgentCreate?: () => void
}

vi.mock('@/components/xero/agent-runtime/live-agent-runtime', () => ({
  LiveAgentRuntimeView: ({
    agent,
    inSidebar,
    sidebarSessions,
    onCloseSidebar,
    onSelectSidebarSession,
    onCreateSession,
    isCreatingSession,
    agentCreateCanvasIncluded,
    onStartWorkflowAgentCreate,
  }: CapturedRuntimeProps & { agent: unknown }) => {
    if (!agent) return null
    return (
      <div data-testid="live-agent-runtime">
        <div data-testid="live-agent-runtime-in-sidebar">{inSidebar ? 'true' : 'false'}</div>
        <div data-testid="live-agent-runtime-session-count">{sidebarSessions?.length ?? 0}</div>
        <div data-testid="live-agent-runtime-canvas-included">
          {agentCreateCanvasIncluded ? 'true' : 'false'}
        </div>
        <button type="button" onClick={() => onStartWorkflowAgentCreate?.()}>
          mock-start-workflow-agent-create
        </button>
        <button type="button" onClick={() => onCloseSidebar?.()}>
          mock-close
        </button>
        <button
          type="button"
          disabled={isCreatingSession}
          onClick={() => onCreateSession?.()}
        >
          mock-create-session
        </button>
        {sidebarSessions?.map((session) => (
          <button
            key={session.agentSessionId}
            type="button"
            onClick={() => onSelectSidebarSession?.(session.agentSessionId)}
          >
            mock-select-{session.agentSessionId}
          </button>
        ))}
      </div>
    )
  },
}))

const sessions: AgentSessionView[] = [
  {
    projectId: 'project-1',
    agentSessionId: 'session-a',
    title: 'First session',
    summary: '',
    status: 'active',
    statusLabel: 'Active',
    selected: true,
    createdAt: '2026-04-15T20:00:00Z',
    updatedAt: '2026-04-15T20:00:00Z',
    archivedAt: null,
    lastRunId: null,
    lastRuntimeKind: null,
    lastProviderId: null,
    lineage: null,
    isActive: true,
    isArchived: false,
  },
  {
    projectId: 'project-1',
    agentSessionId: 'session-b',
    title: 'Second session',
    summary: '',
    status: 'active',
    statusLabel: 'Active',
    selected: false,
    createdAt: '2026-04-16T20:00:00Z',
    updatedAt: '2026-04-16T20:00:00Z',
    archivedAt: null,
    lastRunId: null,
    lastRuntimeKind: null,
    lastProviderId: null,
    lineage: null,
    isActive: true,
    isArchived: false,
  },
]

const dummyAgent = {
  project: {
    id: 'project-1',
    selectedAgentSessionId: 'session-a',
    selectedAgentSession: sessions[0],
    agentSessions: sessions,
  },
} as unknown as ComponentProps<typeof AgentDockSidebar>['agent']

function renderDock(
  overrides: Partial<ComponentProps<typeof AgentDockSidebar>> = {},
) {
  const highChurnStore = createXeroHighChurnStore()
  return render(
    <AgentDockSidebar
      open
      agent={dummyAgent}
      highChurnStore={highChurnStore}
      sessions={sessions}
      selectedSessionId="session-a"
      onClose={vi.fn()}
      onSelectSession={vi.fn()}
      onCreateSession={vi.fn()}
      {...overrides}
    />,
  )
}

describe('AgentDockSidebar', () => {
  afterEach(() => {
    window.localStorage.clear()
  })

  it('renders the live agent runtime in sidebar mode when open with an agent', () => {
    renderDock()
    expect(screen.getByTestId('live-agent-runtime')).toBeInTheDocument()
    expect(screen.getByTestId('live-agent-runtime-in-sidebar')).toHaveTextContent('true')
    expect(screen.getByTestId('live-agent-runtime-session-count')).toHaveTextContent('2')
  })

  it('shows the empty state when no agent is available', () => {
    renderDock({ agent: null })
    expect(screen.queryByTestId('live-agent-runtime')).not.toBeInTheDocument()
    expect(screen.getByText('No active session')).toBeVisible()
    expect(screen.getByRole('button', { name: /New session/ })).toBeVisible()
  })

  it('forwards onCreateSession into the agent runtime header', () => {
    const onCreateSession = vi.fn()
    renderDock({ onCreateSession })

    fireEvent.click(screen.getByRole('button', { name: 'mock-create-session' }))

    expect(onCreateSession).toHaveBeenCalledTimes(1)
  })

  it('forwards the Agent Create canvas context into the agent runtime', () => {
    renderDock({ agentCreateCanvasIncluded: true })

    expect(screen.getByTestId('live-agent-runtime-canvas-included')).toHaveTextContent('true')
  })

  it('forwards the workflow canvas Agent Create action into the agent runtime', () => {
    const onStartWorkflowAgentCreate = vi.fn()
    renderDock({ onStartWorkflowAgentCreate })

    fireEvent.click(screen.getByRole('button', { name: 'mock-start-workflow-agent-create' }))

    expect(onStartWorkflowAgentCreate).toHaveBeenCalledTimes(1)
  })

  it('forwards onSelectSession into the agent runtime header', () => {
    const onSelectSession = vi.fn()
    renderDock({ onSelectSession })

    fireEvent.click(screen.getByRole('button', { name: 'mock-select-session-b' }))

    expect(onSelectSession).toHaveBeenCalledWith('session-b')
  })

  it('forwards onClose into the agent runtime header', () => {
    const onClose = vi.fn()
    renderDock({ onClose })

    fireEvent.click(screen.getByRole('button', { name: 'mock-close' }))

    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('hides the sidebar (width 0, aria-hidden) when closed', () => {
    renderDock({ open: false })

    const aside = screen.getByLabelText('Agent dock')
    expect(aside.getAttribute('aria-hidden')).toBe('true')
    expect((aside as HTMLElement).style.width).toBe('0px')
    expect((aside as HTMLElement).style.transition).toContain('width 160ms')
    expect(screen.queryByTestId('live-agent-runtime')).not.toBeInTheDocument()
    expect(screen.queryByText('No active session')).not.toBeInTheDocument()
  })
})
