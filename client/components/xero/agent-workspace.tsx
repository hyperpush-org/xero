"use client"

import { lazy, memo, Suspense, useCallback, useMemo, useState, type ReactNode } from 'react'

import { PaneGrid, type PaneGridSlot } from '@/components/xero/agent-runtime/pane-grid'
import type { AgentPaneCloseState, AgentRuntimeProps } from '@/components/xero/agent-runtime'
import { LoadingScreen } from '@/components/xero/loading-screen'
import {
  selectRuntimeStreamForProject,
  useXeroHighChurnStoreValue,
  type AgentPaneView,
  type AgentWorkspaceLayoutState,
  type AgentWorkspacePaneView,
  type XeroHighChurnStore,
} from '@/src/features/xero/use-xero-desktop-state'
import { getAgentMessagesUnavailableCredentialReason } from '@/src/features/xero/use-xero-desktop-state/runtime-provider'
import { getRuntimeStreamStatusLabel } from '@/src/lib/xero-model/runtime-stream'

const LazyAgentRuntime = lazy(() =>
  import('@/components/xero/agent-runtime').then((module) => ({ default: module.AgentRuntime })),
)

const AGENT_WORKSPACE_MAX_PANES = 6
const COMPACT_DENSITY_PANE_THRESHOLD = 3
const COMPACT_FIRST_RUN_STORAGE_KEY = 'xero.agentWorkspace.compactFirstRunSeen'

function readCompactFirstRunSeen(): boolean {
  if (typeof window === 'undefined') return true
  try {
    return window.localStorage?.getItem?.(COMPACT_FIRST_RUN_STORAGE_KEY) === '1'
  } catch {
    return true
  }
}

function writeCompactFirstRunSeen(): void {
  if (typeof window === 'undefined') return
  try {
    window.localStorage?.setItem?.(COMPACT_FIRST_RUN_STORAGE_KEY, '1')
  } catch {
    /* storage unavailable — show again next time */
  }
}

function getPaneAriaLabel(pane: AgentWorkspacePaneView, paneNumber: number): string {
  const sessionTitle = pane.agent.project.selectedAgentSession?.title?.trim()
  if (sessionTitle) {
    return `Agent pane ${paneNumber} - Session "${sessionTitle}"`
  }

  return `Agent pane ${paneNumber} - Empty session`
}

function useAgentViewWithLiveRuntimeStream(
  agent: AgentPaneView | null,
  highChurnStore: XeroHighChurnStore,
): AgentPaneView | null {
  const projectId = agent?.project.id ?? null
  const agentSessionId = agent?.project.selectedAgentSessionId ?? null
  const streamSelector = useMemo(
    () => selectRuntimeStreamForProject(projectId, agentSessionId),
    [agentSessionId, projectId],
  )
  const runtimeStream = useXeroHighChurnStoreValue(highChurnStore, streamSelector)

  return useMemo(() => {
    if (!agent) {
      return null
    }

    const streamStatus = runtimeStream?.status ?? 'idle'
    return {
      ...agent,
      runtimeStream,
      runtimeStreamStatus: streamStatus,
      runtimeStreamStatusLabel: getRuntimeStreamStatusLabel(streamStatus),
      runtimeStreamError: runtimeStream?.lastIssue ?? null,
      runtimeStreamItems: runtimeStream?.items ?? [],
      skillItems: runtimeStream?.skillItems ?? [],
      activityItems: runtimeStream?.activityItems ?? [],
      actionRequiredItems: runtimeStream?.actionRequired ?? [],
      messagesUnavailableReason: getAgentMessagesUnavailableCredentialReason(
        agent.runtimeSession ?? null,
        runtimeStream,
        agent.runtimeRun ?? null,
        agent.agentRuntimeBlocked ?? false,
      ),
    }
  }, [agent, runtimeStream])
}

type PaneAwareRuntimeHandlers = {
  onStartAutonomousRun?: (paneId: string) => Promise<unknown>
  onInspectAutonomousRun?: (paneId: string) => Promise<unknown>
  onCancelAutonomousRun?: (paneId: string, runId: string) => Promise<unknown>
  onStartLogin?: (
    paneId: string,
    options?: Parameters<NonNullable<AgentRuntimeProps['onStartLogin']>>[0],
  ) => ReturnType<NonNullable<AgentRuntimeProps['onStartLogin']>>
  onStartRuntimeRun?: (
    paneId: string,
    options?: Parameters<NonNullable<AgentRuntimeProps['onStartRuntimeRun']>>[0],
  ) => ReturnType<NonNullable<AgentRuntimeProps['onStartRuntimeRun']>>
  onUpdateRuntimeRunControls?: (
    paneId: string,
    request?: Parameters<NonNullable<AgentRuntimeProps['onUpdateRuntimeRunControls']>>[0],
  ) => ReturnType<NonNullable<AgentRuntimeProps['onUpdateRuntimeRunControls']>>
  onComposerControlsChange?: (
    paneId: string,
    controls: Parameters<NonNullable<AgentRuntimeProps['onComposerControlsChange']>>[0],
  ) => void
  onStartRuntimeSession?: (
    paneId: string,
    options?: Parameters<NonNullable<AgentRuntimeProps['onStartRuntimeSession']>>[0],
  ) => ReturnType<NonNullable<AgentRuntimeProps['onStartRuntimeSession']>>
  onStopRuntimeRun?: (
    paneId: string,
    runId: string,
  ) => ReturnType<NonNullable<AgentRuntimeProps['onStopRuntimeRun']>>
  onSubmitManualCallback?: (
    paneId: string,
    flowId: string,
    manualInput: string,
  ) => ReturnType<NonNullable<AgentRuntimeProps['onSubmitManualCallback']>>
  onLogout?: (paneId: string) => ReturnType<NonNullable<AgentRuntimeProps['onLogout']>>
  onResolveOperatorAction?: (
    paneId: string,
    actionId: string,
    decision: 'approve' | 'reject',
    options?: Parameters<NonNullable<AgentRuntimeProps['onResolveOperatorAction']>>[2],
  ) => ReturnType<NonNullable<AgentRuntimeProps['onResolveOperatorAction']>>
  onResumeOperatorRun?: (
    paneId: string,
    actionId: string,
    options?: Parameters<NonNullable<AgentRuntimeProps['onResumeOperatorRun']>>[1],
  ) => ReturnType<NonNullable<AgentRuntimeProps['onResumeOperatorRun']>>
  onRefreshNotificationRoutes?: (
    paneId: string,
    options?: Parameters<NonNullable<AgentRuntimeProps['onRefreshNotificationRoutes']>>[0],
  ) => ReturnType<NonNullable<AgentRuntimeProps['onRefreshNotificationRoutes']>>
  onUpsertNotificationRoute?: (
    paneId: string,
    request: Parameters<NonNullable<AgentRuntimeProps['onUpsertNotificationRoute']>>[0],
  ) => ReturnType<NonNullable<AgentRuntimeProps['onUpsertNotificationRoute']>>
}

export interface AgentWorkspaceProps
  extends Omit<
      AgentRuntimeProps,
      | 'agent'
      | 'density'
      | 'paneId'
      | 'paneNumber'
      | 'paneCount'
      | 'onSpawnPane'
      | 'spawnPaneDisabled'
      | 'onClosePane'
      | 'onFocusPane'
      | 'isPaneFocused'
      | 'showCompactFirstRunTooltip'
      | 'onAckCompactFirstRunTooltip'
      | 'onPaneCloseStateChange'
      | 'onStartAutonomousRun'
      | 'onInspectAutonomousRun'
      | 'onCancelAutonomousRun'
      | 'onStartLogin'
      | 'onStartRuntimeRun'
      | 'onUpdateRuntimeRunControls'
      | 'onComposerControlsChange'
      | 'onStartRuntimeSession'
      | 'onStopRuntimeRun'
      | 'onSubmitManualCallback'
      | 'onLogout'
      | 'onResolveOperatorAction'
      | 'onResumeOperatorRun'
      | 'onRefreshNotificationRoutes'
      | 'onUpsertNotificationRoute'
    >,
    PaneAwareRuntimeHandlers {
  layout: AgentWorkspaceLayoutState | null
  panes: AgentWorkspacePaneView[]
  highChurnStore: XeroHighChurnStore
  fallback?: ReactNode
  onSpawnPane?: () => void
  onClosePane?: (paneId: string) => void
  onFocusPane?: (paneId: string) => void
  onSplitterRatiosChange?: (arrangementKey: string, ratios: number[]) => void
  onPaneCloseStateChange?: (paneId: string, state: AgentPaneCloseState) => void
}

interface PaneRuntimeProps extends Omit<AgentRuntimeProps, 'agent'> {
  pane: AgentWorkspacePaneView
  highChurnStore: XeroHighChurnStore
}

const LiveAgentRuntime = memo(function LiveAgentRuntime({
  pane,
  highChurnStore,
  ...props
}: PaneRuntimeProps) {
  const liveAgent = useAgentViewWithLiveRuntimeStream(pane.agent, highChurnStore)
  if (!liveAgent) {
    return null
  }

  return (
    <Suspense fallback={<LoadingScreen />}>
      <LazyAgentRuntime {...props} agent={liveAgent} />
    </Suspense>
  )
})

export function AgentWorkspace({
  layout,
  panes,
  highChurnStore,
  fallback = null,
  onSpawnPane,
  onClosePane,
  onFocusPane,
  onSplitterRatiosChange,
  onPaneCloseStateChange,
  onStartAutonomousRun,
  onInspectAutonomousRun,
  onCancelAutonomousRun,
  onStartLogin,
  onStartRuntimeRun,
  onUpdateRuntimeRunControls,
  onComposerControlsChange,
  onStartRuntimeSession,
  onStopRuntimeRun,
  onSubmitManualCallback,
  onLogout,
  onResolveOperatorAction,
  onResumeOperatorRun,
  onRefreshNotificationRoutes,
  onUpsertNotificationRoute,
  ...runtimeProps
}: AgentWorkspaceProps) {
  const paneCount = panes.length
  const focusedPaneId = layout?.focusedPaneId ?? panes[0]?.paneId ?? null
  const density: 'comfortable' | 'compact' =
    paneCount >= COMPACT_DENSITY_PANE_THRESHOLD ? 'compact' : 'comfortable'
  const spawnPaneDisabled = paneCount >= AGENT_WORKSPACE_MAX_PANES

  const [compactFirstRunSeen, setCompactFirstRunSeen] = useState<boolean>(() =>
    readCompactFirstRunSeen(),
  )
  const ackCompactFirstRunTooltip = useCallback(() => {
    setCompactFirstRunSeen((prev) => {
      if (prev) return prev
      writeCompactFirstRunSeen()
      return true
    })
  }, [])
  const showCompactFirstRunTooltip =
    density === 'compact' && !compactFirstRunSeen

  const slots = useMemo<PaneGridSlot[]>(
    () =>
      panes.map((pane, index) => ({
        paneId: pane.paneId,
        isFocused: pane.paneId === focusedPaneId,
        ariaLabel: getPaneAriaLabel(pane, index + 1),
      })),
    [panes, focusedPaneId],
  )

  if (paneCount === 0) {
    return <>{fallback}</>
  }

  const renderPane = (slot: PaneGridSlot, index: number) => {
    const pane = panes[index]
    if (!pane || pane.paneId !== slot.paneId) {
      return null
    }
    return (
      <PaneRuntime
        pane={pane}
        index={index}
        paneCount={paneCount}
        density={density}
        isFocused={slot.isFocused}
        spawnPaneDisabled={spawnPaneDisabled}
        showCompactFirstRunTooltip={showCompactFirstRunTooltip && slot.isFocused}
        onAckCompactFirstRunTooltip={ackCompactFirstRunTooltip}
        onSpawnPane={onSpawnPane}
        onClosePane={onClosePane}
        onFocusPane={onFocusPane}
        onPaneCloseStateChange={onPaneCloseStateChange}
        runtimeProps={runtimeProps}
        highChurnStore={highChurnStore}
        onStartAutonomousRun={onStartAutonomousRun}
        onInspectAutonomousRun={onInspectAutonomousRun}
        onCancelAutonomousRun={onCancelAutonomousRun}
        onStartLogin={onStartLogin}
        onStartRuntimeRun={onStartRuntimeRun}
        onUpdateRuntimeRunControls={onUpdateRuntimeRunControls}
        onComposerControlsChange={onComposerControlsChange}
        onStartRuntimeSession={onStartRuntimeSession}
        onStopRuntimeRun={onStopRuntimeRun}
        onSubmitManualCallback={onSubmitManualCallback}
        onLogout={onLogout}
        onResolveOperatorAction={onResolveOperatorAction}
        onResumeOperatorRun={onResumeOperatorRun}
        onRefreshNotificationRoutes={onRefreshNotificationRoutes}
        onUpsertNotificationRoute={onUpsertNotificationRoute}
      />
    )
  }

  return (
    <PaneGrid
      slots={slots}
      splitterRatios={layout?.splitterRatios}
      onSplitterRatiosChange={onSplitterRatiosChange}
      onFocusPane={onFocusPane}
      renderPane={renderPane}
    />
  )
}

interface PaneRuntimeWrapperProps extends PaneAwareRuntimeHandlers {
  pane: AgentWorkspacePaneView
  index: number
  paneCount: number
  density: 'comfortable' | 'compact'
  isFocused: boolean
  spawnPaneDisabled: boolean
  showCompactFirstRunTooltip: boolean
  onAckCompactFirstRunTooltip: () => void
  onSpawnPane?: () => void
  onClosePane?: (paneId: string) => void
  onFocusPane?: (paneId: string) => void
  onPaneCloseStateChange?: (paneId: string, state: AgentPaneCloseState) => void
  runtimeProps: Omit<AgentRuntimeProps,
    | 'agent'
    | 'density'
    | 'paneId'
    | 'paneNumber'
    | 'paneCount'
    | 'onSpawnPane'
    | 'spawnPaneDisabled'
    | 'onClosePane'
    | 'onFocusPane'
    | 'isPaneFocused'
    | 'showCompactFirstRunTooltip'
    | 'onAckCompactFirstRunTooltip'
    | 'onPaneCloseStateChange'
    | 'onStartAutonomousRun'
    | 'onInspectAutonomousRun'
    | 'onCancelAutonomousRun'
    | 'onStartLogin'
    | 'onStartRuntimeRun'
    | 'onUpdateRuntimeRunControls'
    | 'onComposerControlsChange'
    | 'onStartRuntimeSession'
    | 'onStopRuntimeRun'
    | 'onSubmitManualCallback'
    | 'onLogout'
    | 'onResolveOperatorAction'
    | 'onResumeOperatorRun'
    | 'onRefreshNotificationRoutes'
    | 'onUpsertNotificationRoute'
  >
  highChurnStore: XeroHighChurnStore
}

const PaneRuntime = memo(function PaneRuntime({
  pane,
  index,
  paneCount,
  density,
  isFocused,
  spawnPaneDisabled,
  showCompactFirstRunTooltip,
  onAckCompactFirstRunTooltip,
  onSpawnPane,
  onClosePane,
  onFocusPane,
  onPaneCloseStateChange,
  runtimeProps,
  highChurnStore,
  onStartAutonomousRun,
  onInspectAutonomousRun,
  onCancelAutonomousRun,
  onStartLogin,
  onStartRuntimeRun,
  onUpdateRuntimeRunControls,
  onComposerControlsChange,
  onStartRuntimeSession,
  onStopRuntimeRun,
  onSubmitManualCallback,
  onLogout,
  onResolveOperatorAction,
  onResumeOperatorRun,
  onRefreshNotificationRoutes,
  onUpsertNotificationRoute,
}: PaneRuntimeWrapperProps) {
  const paneId = pane.paneId
  const paneBoundHandlers = useMemo<
    Partial<
      Pick<
        AgentRuntimeProps,
        | 'onStartAutonomousRun'
        | 'onInspectAutonomousRun'
        | 'onCancelAutonomousRun'
        | 'onStartLogin'
        | 'onStartRuntimeRun'
        | 'onUpdateRuntimeRunControls'
        | 'onComposerControlsChange'
        | 'onStartRuntimeSession'
        | 'onStopRuntimeRun'
        | 'onSubmitManualCallback'
        | 'onLogout'
        | 'onResolveOperatorAction'
        | 'onResumeOperatorRun'
        | 'onRefreshNotificationRoutes'
        | 'onUpsertNotificationRoute'
      >
    >
  >(() => {
    return {
      onStartAutonomousRun: onStartAutonomousRun
        ? () => onStartAutonomousRun(paneId)
        : undefined,
      onInspectAutonomousRun: onInspectAutonomousRun
        ? () => onInspectAutonomousRun(paneId)
        : undefined,
      onCancelAutonomousRun: onCancelAutonomousRun
        ? (runId) => onCancelAutonomousRun(paneId, runId)
        : undefined,
      onStartLogin: onStartLogin
        ? (options) => onStartLogin(paneId, options)
        : undefined,
      onStartRuntimeRun: onStartRuntimeRun
        ? (options) => onStartRuntimeRun(paneId, options)
        : undefined,
      onUpdateRuntimeRunControls: onUpdateRuntimeRunControls
        ? (request) => onUpdateRuntimeRunControls(paneId, request)
        : undefined,
      onComposerControlsChange: onComposerControlsChange
        ? (controls) => onComposerControlsChange(paneId, controls)
        : undefined,
      onStartRuntimeSession: onStartRuntimeSession
        ? (options) => onStartRuntimeSession(paneId, options)
        : undefined,
      onStopRuntimeRun: onStopRuntimeRun
        ? (runId) => onStopRuntimeRun(paneId, runId)
        : undefined,
      onSubmitManualCallback: onSubmitManualCallback
        ? (flowId, manualInput) => onSubmitManualCallback(paneId, flowId, manualInput)
        : undefined,
      onLogout: onLogout ? () => onLogout(paneId) : undefined,
      onResolveOperatorAction: onResolveOperatorAction
        ? (actionId, decision, options) =>
            onResolveOperatorAction(paneId, actionId, decision, options)
        : undefined,
      onResumeOperatorRun: onResumeOperatorRun
        ? (actionId, options) => onResumeOperatorRun(paneId, actionId, options)
        : undefined,
      onRefreshNotificationRoutes: onRefreshNotificationRoutes
        ? (options) => onRefreshNotificationRoutes(paneId, options)
        : undefined,
      onUpsertNotificationRoute: onUpsertNotificationRoute
        ? (request) => onUpsertNotificationRoute(paneId, request)
        : undefined,
    }
  }, [
    onCancelAutonomousRun,
    onComposerControlsChange,
    onInspectAutonomousRun,
    onLogout,
    onRefreshNotificationRoutes,
    onResolveOperatorAction,
    onResumeOperatorRun,
    onStartAutonomousRun,
    onStartLogin,
    onStartRuntimeRun,
    onStartRuntimeSession,
    onStopRuntimeRun,
    onSubmitManualCallback,
    onUpdateRuntimeRunControls,
    onUpsertNotificationRoute,
    paneId,
  ])

  const handleClose = useMemo(
    () => (onClosePane ? () => onClosePane(paneId) : undefined),
    [onClosePane, paneId],
  )
  const handleFocus = useMemo(
    () => (onFocusPane ? () => onFocusPane(paneId) : undefined),
    [onFocusPane, paneId],
  )
  const handlePaneCloseStateChange = useMemo(
    () =>
      onPaneCloseStateChange
        ? (state: AgentPaneCloseState) => onPaneCloseStateChange(paneId, state)
        : undefined,
    [onPaneCloseStateChange, paneId],
  )

  return (
    <LiveAgentRuntime
      {...runtimeProps}
      {...paneBoundHandlers}
      pane={pane}
      highChurnStore={highChurnStore}
      density={density}
      paneId={paneId}
      paneNumber={index + 1}
      paneCount={paneCount}
      onSpawnPane={onSpawnPane}
      spawnPaneDisabled={spawnPaneDisabled}
      onClosePane={handleClose}
      onFocusPane={handleFocus}
      showCompactFirstRunTooltip={showCompactFirstRunTooltip}
      onAckCompactFirstRunTooltip={onAckCompactFirstRunTooltip}
      onPaneCloseStateChange={handlePaneCloseStateChange}
    />
  )
})
