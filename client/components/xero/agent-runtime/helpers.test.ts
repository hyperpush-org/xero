import { describe, expect, it } from 'vitest'

import { getComposerPlaceholder } from '@/components/xero/agent-runtime/composer-helpers'
import {
  getStreamStatusMeta,
  getToolCardTitle,
  getToolSummaryContext,
} from '@/components/xero/agent-runtime/runtime-stream-helpers'
import { displayValue, formatSequence } from '@/components/xero/agent-runtime/shared-helpers'
import type { AgentPaneView } from '@/src/features/xero/use-xero-desktop-state'
import type { RuntimeSessionView, RuntimeStreamToolItemView } from '@/src/lib/xero-model'

function makeAgent(overrides: Partial<AgentPaneView> = {}): AgentPaneView {
  return {
    project: {
      id: 'project-1',
      name: 'Xero',
      description: 'Desktop shell',
      milestone: 'M001',
      totalPhases: 0,
      completedPhases: 0,
      activePhase: 0,
      phases: [],
      branch: 'main',
      runtime: 'Runtime unavailable',
      branchLabel: 'main',
      runtimeLabel: 'Runtime unavailable',
      phaseProgressPercent: 0,
      repository: null,
      repositoryStatus: null,
      approvalRequests: [],
      pendingApprovalCount: 0,
      latestDecisionOutcome: null,
      verificationRecords: [],
      resumeHistory: [],
      agentSessions: [],
      selectedAgentSession: null,
      selectedAgentSessionId: 'agent-session-main',
      notificationBroker: {
        dispatches: [],
        actions: [],
        routes: [],
        byActionId: {},
        byRouteId: {},
        dispatchCount: 0,
        routeCount: 0,
        pendingCount: 0,
        sentCount: 0,
        failedCount: 0,
        claimedCount: 0,
        latestUpdatedAt: null,
        isTruncated: false,
        totalBeforeTruncation: 0,
      },
      runtimeSession: null,
      runtimeRun: null,
      autonomousRun: null,
    },
    activePhase: null,
    branchLabel: 'main',
    headShaLabel: 'No HEAD',
    runtimeLabel: 'Runtime unavailable',
    repositoryLabel: 'Xero',
    repositoryPath: '/tmp/Xero',
    notificationBroker: {
      dispatches: [],
      actions: [],
      routes: [],
      byActionId: {},
      byRouteId: {},
      dispatchCount: 0,
      routeCount: 0,
      pendingCount: 0,
      sentCount: 0,
      failedCount: 0,
      claimedCount: 0,
      latestUpdatedAt: null,
      isTruncated: false,
      totalBeforeTruncation: 0,
    },
    controlTruthSource: 'fallback',
    selectedRuntimeAgentId: 'ask',
    selectedRuntimeAgentLabel: 'Ask',
    selectedModelId: 'openai_codex',
    selectedThinkingEffort: null,
    selectedApprovalMode: 'suggest',
    selectedPrompt: {
      text: null,
      queuedAt: null,
      hasQueuedPrompt: false,
    },
    runtimeRunActiveControls: null,
    runtimeRunPendingControls: null,
    providerModelCatalog: {
      profileId: null,
      profileLabel: null,
      providerId: 'openai_codex',
      providerLabel: 'OpenAI Codex',
      source: null,
      loadStatus: 'idle',
      state: 'unavailable',
      stateLabel: 'Catalog unavailable',
      detail: 'Xero does not have a discovered model catalog for OpenAI Codex yet, so only configured model truth remains visible.',
      fetchedAt: null,
      lastSuccessAt: null,
      lastRefreshError: null,
      models: [],
    },
    selectedModelOption: null,
    selectedModelThinkingEffortOptions: [],
    selectedModelDefaultThinkingEffort: null,
    notificationRoutes: [],
    notificationChannelHealth: [],
    notificationRouteLoadStatus: 'idle',
    notificationRouteIsRefreshing: false,
    notificationRouteError: null,
    notificationSyncSummary: null,
    notificationSyncError: null,
    notificationSyncPollingActive: false,
    notificationSyncPollingActionId: null,
    notificationSyncPollingBoundaryId: null,
    notificationRouteMutationStatus: 'idle',
    pendingNotificationRouteId: null,
    notificationRouteMutationError: null,
    approvalRequests: [],
    pendingApprovalCount: 0,
    latestDecisionOutcome: null,
    resumeHistory: [],
    operatorActionStatus: 'idle',
    pendingOperatorActionId: null,
    operatorActionError: null,
    autonomousRunActionStatus: 'idle',
    pendingAutonomousRunAction: null,
    autonomousRunActionError: null,
    runtimeRunActionStatus: 'idle',
    pendingRuntimeRunAction: null,
    runtimeRunActionError: null,
    sessionUnavailableReason: 'Current session status for this project.',
    runtimeRunUnavailableReason:
      'Xero recovered a Xero-owned agent run before the live runtime feed resumed.',
    messagesUnavailableReason: 'Xero authenticated this project, but the live runtime stream has not started yet.',
    ...overrides,
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
    phaseLabel: 'Signed out',
    runtimeLabel: 'Runtime unavailable',
    accountLabel: 'Not signed in',
    sessionLabel: 'No session',
    callbackBound: null,
    authorizationUrl: null,
    redirectUri: null,
    lastErrorCode: 'auth_session_not_found',
    lastError: {
      code: 'auth_session_not_found',
      message: 'Sign in with OpenAI to create a runtime session for this project.',
      retryable: false,
    },
    updatedAt: '2026-04-13T20:00:49Z',
    isAuthenticated: false,
    isLoginInProgress: false,
    needsManualInput: false,
    isSignedOut: true,
    isFailed: false,
    ...overrides,
  }
}

function makeToolItem(overrides: Partial<RuntimeStreamToolItemView> = {}): RuntimeStreamToolItemView {
  return {
    id: 'tool:run-1:1',
    kind: 'tool',
    runId: 'run-1',
    sequence: 1,
    createdAt: '2026-04-24T17:30:00Z',
    toolCallId: 'tool-call-1',
    toolName: 'read',
    toolState: 'succeeded',
    detail: null,
    toolSummary: null,
    ...overrides,
  }
}

describe('agent-runtime helpers', () => {
  it('keeps blank labels and missing sequences on the existing fallback copy', () => {
    expect(displayValue('   ', 'Unavailable')).toBe('Unavailable')
    expect(formatSequence(null)).toBe('Not observed')
  })

  it('formats browser/computer-use tool summaries with safe fallback labels for optional metadata', () => {
    const browserItem = makeToolItem({
      toolCallId: 'browser-click-1',
      toolName: 'browser.click',
      toolState: 'succeeded',
      detail: 'Clicked submit in browser context.',
      toolSummary: {
        kind: 'browser_computer_use',
        surface: 'browser',
        action: 'click',
        status: 'succeeded',
        target: 'button[type=submit]',
        outcome: 'Clicked submit and advanced to confirmation.',
      },
    })

    const computerItem: RuntimeStreamToolItemView = {
      ...browserItem,
      id: 'tool:run-1:2',
      sequence: 2,
      toolCallId: 'computer-key-1',
      toolName: 'computer_use.key_press',
      toolState: 'failed',
      toolSummary: {
        kind: 'browser_computer_use',
        surface: 'computer_use',
        action: 'press_key',
        status: 'blocked',
        target: null,
        outcome: null,
      },
    }

    expect(getToolSummaryContext(browserItem)).toBe(
      'Browser action click · status Succeeded · target button[type=submit] · outcome Clicked submit and advanced to confirmation.',
    )
    expect(getToolSummaryContext(computerItem)).toBe(
      'Computer use action press_key · status Blocked · target Target unavailable · outcome Outcome unavailable',
    )
    expect(getToolSummaryContext({ ...browserItem, toolSummary: null })).toBeNull()
  })

  it('formats command, file, git, and web summaries for tool detail drawers', () => {
    expect(
      getToolSummaryContext(
        makeToolItem({
          toolName: 'command',
          toolSummary: {
            kind: 'command',
            exitCode: 0,
            timedOut: false,
            stdoutTruncated: true,
            stderrTruncated: false,
            stdoutRedacted: false,
            stderrRedacted: true,
          },
        }),
      ),
    ).toBe('Command · exit 0 · stdout truncated · stderr redacted')

    expect(
      getToolSummaryContext(
        makeToolItem({
          toolName: 'find',
          toolSummary: {
            kind: 'file',
            path: null,
            scope: 'client/src-tauri',
            lineCount: null,
            matchCount: 2,
            truncated: true,
          },
        }),
      ),
    ).toBe('File result · scope client/src-tauri · 2 matches · truncated')

    expect(
      getToolSummaryContext(
        makeToolItem({
          toolName: 'git_diff',
          toolSummary: {
            kind: 'git',
            scope: 'worktree',
            changedFiles: 3,
            truncated: true,
            baseRevision: 'HEAD~1',
          },
        }),
      ),
    ).toBe('Git · worktree · 3 changed files · base HEAD~1 · truncated')

    expect(
      getToolSummaryContext(
        makeToolItem({
          toolName: 'web_fetch',
          toolSummary: {
            kind: 'web',
            target: 'https://example.com',
            resultCount: null,
            finalUrl: 'https://www.example.com/',
            contentKind: 'html',
            contentType: 'text/html',
            truncated: false,
          },
        }),
      ),
    ).toBe('Web · https://example.com · final https://www.example.com/ · HTML · text/html')
  })

  it('builds compact action-target titles for common tool cards', () => {
    expect(
      getToolCardTitle(
        makeToolItem({
          toolName: 'read',
          detail: 'path: client/components/xero/agent-runtime.tsx, startLine: 12, lineCount: 40',
        }),
      ),
    ).toBe('read agent-runtime.tsx')

    expect(
      getToolCardTitle(
        makeToolItem({
          toolName: 'find',
          detail: 'pattern: appendTranscriptDelta, path: client/components/xero',
        }),
      ),
    ).toBe('find appendTranscriptDelta')

    expect(
      getToolCardTitle(
        makeToolItem({
          toolName: 'list',
          detail: 'path: client/components/xero, maxDepth: 2',
        }),
      ),
    ).toBe('list client/components/xero')
  })

  it('uses generic blocked copy when no credentials are configured for the chosen provider', () => {
    expect(
      getComposerPlaceholder(null, 'idle', null, undefined, {
        selectedProviderId: 'openrouter',
        agentRuntimeBlocked: true,
      }),
    ).toBe('Connect a provider in Settings to start chatting.')
  })

  it('falls through to a per-provider start prompt when not blocked and signed out', () => {
    expect(
      getComposerPlaceholder(null, 'idle', null, undefined, {
        selectedProviderId: 'openrouter',
        agentRuntimeBlocked: false,
      }),
    ).toBe('Ask anything to get started with OpenRouter.')

    expect(
      getComposerPlaceholder(null, 'idle', null, undefined, {
        selectedProviderId: 'github_models',
        agentRuntimeBlocked: false,
      }),
    ).toBe('Ask anything to get started with GitHub Models.')
  })

  it('keeps the stream meta copy stable', () => {
    const meta = getStreamStatusMeta(
      makeAgent({
        runtimeRun: { runId: 'run-unavailable' } as never,
      }),
      makeRuntimeSession({
        isAuthenticated: true,
        isSignedOut: false,
        lastError: null,
        lastErrorCode: null,
      }),
    )

    expect(meta.title).toBe('No agent run attached yet')
  })
})
