import { describe, expect, it } from 'vitest'
import type {
  NotificationDispatchView,
  NotificationRouteDto,
  ProjectDetailView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamView,
  SyncNotificationAdaptersResponseDto,
} from '@/src/lib/xero-model'
import {
  composeAgentTrustSnapshot,
  getBlockedNotificationSyncPollKey,
  getBlockedNotificationSyncPollTarget,
  mapNotificationChannelHealth,
  mapNotificationRouteViews,
} from './notification-health'
function makeRoute(overrides: Partial<NotificationRouteDto> = {}): NotificationRouteDto {
  const routeKind = overrides.routeKind ?? 'telegram'

  return {
    projectId: 'project-1',
    routeId: routeKind === 'discord' ? 'discord-fallback' : 'telegram-primary',
    routeKind,
    routeTarget: routeKind === 'discord' ? '1234567890' : '@ops-room',
    enabled: true,
    metadataJson: null,
    credentialReadiness:
      routeKind === 'discord'
        ? {
            hasBotToken: false,
            hasChatId: false,
            hasWebhookUrl: true,
            ready: false,
            status: 'missing',
            diagnostic: {
              code: 'notification_adapter_credentials_missing',
              message: 'Xero is missing app-local Discord botToken credentials.',
              retryable: false,
            },
          }
        : {
            hasBotToken: true,
            hasChatId: true,
            hasWebhookUrl: false,
            ready: true,
            status: 'ready',
            diagnostic: null,
          },
    createdAt: '2026-04-16T12:59:00Z',
    updatedAt: '2026-04-16T12:59:00Z',
    ...overrides,
  }
}

function makeDispatch(overrides: Partial<NotificationDispatchView> = {}): NotificationDispatchView {
  return {
    routeId: 'telegram-primary',
    isPending: false,
    isSent: false,
    isFailed: false,
    isClaimed: false,
    updatedAt: '2026-04-16T13:00:00Z',
    lastAttemptAt: null,
    createdAt: '2026-04-16T13:00:00Z',
    lastErrorCode: null,
    lastErrorMessage: null,
    ...overrides,
  } as NotificationDispatchView
}

function makeRuntimeSession(overrides: Partial<RuntimeSessionView> = {}): RuntimeSessionView {
  return {
    providerId: 'openai_codex',
    runtimeKind: 'openai_codex',
    phase: 'authenticated',
    sessionId: 'session-1',
    sessionLabel: 'session-1',
    accountLabel: 'acct-1',
    isAuthenticated: true,
    isLoginInProgress: false,
    lastError: null,
    ...overrides,
  } as RuntimeSessionView
}

function makeRuntimeRun(overrides: Partial<RuntimeRunView> = {}): RuntimeRunView {
  return {
    isFailed: false,
    isStale: false,
    isActive: true,
    isTerminal: false,
    hasCheckpoints: true,
    lastError: null,
    ...overrides,
  } as RuntimeRunView
}

function makeRuntimeStream(overrides: Partial<RuntimeStreamView> = {}): RuntimeStreamView {
  return {
    status: 'live',
    lastIssue: null,
    actionRequired: [],
    completion: null,
    failure: null,
    items: [],
    ...overrides,
  } as RuntimeStreamView
}

function makeSyncSummary(overrides: Partial<SyncNotificationAdaptersResponseDto> = {}): SyncNotificationAdaptersResponseDto {
  return {
    projectId: 'project-1',
    dispatch: {
      projectId: 'project-1',
      pendingCount: 0,
      attemptedCount: 1,
      sentCount: 1,
      failedCount: 0,
      attemptLimit: 64,
      attemptsTruncated: false,
      attempts: [],
      errorCodeCounts: [],
    },
    replies: {
      projectId: 'project-1',
      routeCount: 2,
      polledRouteCount: 2,
      messageCount: 1,
      acceptedCount: 1,
      rejectedCount: 0,
      attemptLimit: 256,
      attemptsTruncated: false,
      attempts: [],
      errorCodeCounts: [],
    },
    syncedAt: '2026-04-17T03:30:00Z',
    ...overrides,
  } as SyncNotificationAdaptersResponseDto
}

describe('notification-health helpers', () => {
  it('prefers the active runtime action-required boundary when choosing a blocked sync poll target', () => {
    const actionId = 'flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required'
    const project = {
      id: 'project-1',
      approvalRequests: [
        {
          actionId,
          actionType: 'terminal_input_required',
          isPending: true,
        },
      ],
    } as ProjectDetailView

    const target = getBlockedNotificationSyncPollTarget({
      project,
      runtimeStream: makeRuntimeStream({
        actionRequired: [
          {
            id: 'action-required-1',
            kind: 'action_required',
            runId: 'run-1',
            sequence: 1,
            actionId,
            boundaryId: 'boundary-1',
            actionType: 'terminal_input_required',
            title: 'Terminal input required',
            detail: 'Xero is waiting for terminal input.',
            createdAt: '2026-04-16T13:00:01Z',
          },
        ],
      }),
    })

    expect(target).toEqual({
      projectId: 'project-1',
      actionId,
      boundaryId: 'boundary-1',
    })
    expect(getBlockedNotificationSyncPollKey(target)).toBe(
      'project-1:flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required:boundary-1',
    )
  })

  it('maps route and channel health from dispatch activity', () => {
    const routeViews = mapNotificationRouteViews(
      'project-1',
      [
        makeRoute({ routeKind: 'telegram' }),
        makeRoute({ routeKind: 'discord' }),
      ],
      [
        makeDispatch({
          routeId: 'telegram-primary',
          isFailed: true,
          lastErrorCode: 'notification_adapter_transport_failed',
          lastErrorMessage: 'Telegram returned 502.',
          updatedAt: '2026-04-16T13:01:00Z',
        }),
        makeDispatch({
          routeId: 'discord-fallback',
          isClaimed: true,
          updatedAt: '2026-04-16T13:02:00Z',
        }),
      ],
    )
    const channelHealth = mapNotificationChannelHealth(routeViews)

    const telegramRoute = routeViews.find((route) => route.routeId === 'telegram-primary')
    const telegramChannel = channelHealth.find((channel) => channel.routeKind === 'telegram')
    const discordChannel = channelHealth.find((channel) => channel.routeKind === 'discord')

    expect(telegramRoute).toMatchObject({
      failedCount: 1,
      health: 'degraded',
      latestFailureCode: 'notification_adapter_transport_failed',
    })
    expect(telegramChannel).toMatchObject({
      routeCount: 1,
      failedCount: 1,
      health: 'degraded',
    })
    expect(discordChannel).toMatchObject({
      routeCount: 1,
      claimedCount: 1,
      health: 'healthy',
    })
  })

  it('composes trust snapshot counts from route readiness and sync state', () => {
    const trustSnapshot = composeAgentTrustSnapshot({
      runtimeSession: makeRuntimeSession(),
      runtimeRun: makeRuntimeRun(),
      runtimeStream: makeRuntimeStream(),
      approvalRequests: [],
      routeViews: mapNotificationRouteViews(
        'project-1',
        [makeRoute({ routeKind: 'telegram' }), makeRoute({ routeKind: 'discord' })],
        [],
      ),
      notificationRouteError: null,
      notificationSyncSummary: makeSyncSummary({
        replies: {
          projectId: 'project-1',
          routeCount: 2,
          polledRouteCount: 2,
          messageCount: 1,
          acceptedCount: 0,
          rejectedCount: 1,
          attemptLimit: 256,
          attemptsTruncated: false,
          attempts: [],
          errorCodeCounts: [],
        },
      }),
      notificationSyncError: null,
    })

    expect(trustSnapshot.state).toBe('degraded')
    expect(trustSnapshot.credentialsState).toBe('degraded')
    expect(trustSnapshot.readyCredentialRouteCount).toBe(1)
    expect(trustSnapshot.missingCredentialRouteCount).toBe(1)
    expect(trustSnapshot.syncReplyRejectedCount).toBe(1)
  })

  it('throws when route readiness metadata is malformed', () => {
    expect(() =>
      composeAgentTrustSnapshot({
        runtimeSession: makeRuntimeSession(),
        runtimeRun: makeRuntimeRun(),
        runtimeStream: makeRuntimeStream(),
        approvalRequests: [],
        routeViews: mapNotificationRouteViews(
          'project-1',
          [
            makeRoute({
              routeKind: 'telegram',
              credentialReadiness: {
                hasBotToken: true,
                hasChatId: true,
                hasWebhookUrl: false,
                ready: false,
                status: 'ready',
                diagnostic: null,
              } as never,
            }),
          ],
          [],
        ),
        notificationRouteError: null,
        notificationSyncSummary: makeSyncSummary(),
        notificationSyncError: null,
      }),
    ).toThrow(/readiness metadata was malformed/)
  })
})
