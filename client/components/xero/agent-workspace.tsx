"use client"

import { memo, useCallback, useMemo, useRef, type ReactNode } from 'react'
import { useDroppable } from '@dnd-kit/core'

import { PaneGrid, type PaneDragHandle, type PaneGridSlot } from '@/components/xero/agent-runtime/pane-grid'
import { AGENT_WORKSPACE_DROP_TARGET_ID } from '@/components/xero/agent-runtime/agent-workspace-dnd-provider'
import { isAgentPaneWorking } from '@/components/xero/agent-runtime/runtime-stream-helpers'
import type { AgentPaneCloseState, AgentRuntimeProps } from '@/components/xero/agent-runtime'
import { LiveAgentRuntimeView } from '@/components/xero/agent-runtime/live-agent-runtime'
import type {
  AgentWorkspaceLayoutState,
  AgentWorkspacePaneView,
  XeroHighChurnStore,
} from '@/src/features/xero/use-xero-desktop-state'

const AGENT_WORKSPACE_MAX_PANES = 6
const COMPACT_DENSITY_PANE_THRESHOLD = 3

function getPaneAriaLabel(pane: AgentWorkspacePaneView, paneNumber: number): string {
  const sessionTitle = pane.agent.project.selectedAgentSession?.title?.trim()
  if (sessionTitle) {
    return `Agent pane ${paneNumber} - Session "${sessionTitle}"`
  }

  return `Agent pane ${paneNumber} - Empty session`
}

function shallowEqualRecords<T extends object>(left: T, right: T): boolean {
  if (left === right) {
    return true
  }

  const leftKeys = Object.keys(left)
  const rightKeys = Object.keys(right)
  if (leftKeys.length !== rightKeys.length) {
    return false
  }

  return leftKeys.every((key) =>
    Object.is(left[key as keyof T], right[key as keyof T]),
  )
}

function useShallowStableRecord<T extends object>(value: T): T {
  const ref = useRef(value)
  if (!shallowEqualRecords(ref.current, value)) {
    ref.current = value
  }
  return ref.current
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
      | 'active'
      | 'density'
      | 'paneId'
      | 'paneNumber'
      | 'paneCount'
      | 'onSpawnPane'
      | 'spawnPaneDisabled'
      | 'onClosePane'
      | 'onFocusPane'
      | 'isPaneFocused'
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
  active?: boolean
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

export const AgentWorkspace = memo(function AgentWorkspace({
  layout,
  panes,
  active = true,
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
  const stableRuntimeProps = useShallowStableRecord(runtimeProps)
  const paneCount = panes.length
  const focusedPaneId = layout?.focusedPaneId ?? panes[0]?.paneId ?? null
  const density: 'comfortable' | 'compact' =
    paneCount >= COMPACT_DENSITY_PANE_THRESHOLD ? 'compact' : 'comfortable'
  const spawnPaneDisabled = paneCount >= AGENT_WORKSPACE_MAX_PANES
  const {
    isOver: isWorkspaceDropOver,
    setNodeRef: setWorkspaceDropNodeRef,
  } = useDroppable({
    id: AGENT_WORKSPACE_DROP_TARGET_ID,
  })

  const slots = useMemo<PaneGridSlot[]>(
    () =>
      panes.map((pane, index) => ({
        paneId: pane.paneId,
        isFocused: pane.paneId === focusedPaneId,
        ariaLabel: getPaneAriaLabel(pane, index + 1),
        isWorking: isAgentPaneWorking(pane.agent),
      })),
    [panes, focusedPaneId],
  )

  if (paneCount === 0) {
    return (
      <div
        ref={setWorkspaceDropNodeRef}
        className="flex h-full w-full min-h-0 min-w-0 flex-1"
        data-agent-workspace-drop-over={isWorkspaceDropOver ? 'true' : undefined}
        data-agent-workspace-drop-target
      >
        {fallback}
      </div>
    )
  }

  const renderPane = useCallback(
    (slot: PaneGridSlot, index: number, dragHandle: PaneDragHandle) => {
      const pane = panes[index]
      if (!pane || pane.paneId !== slot.paneId) {
        return null
      }
      return (
        <PaneRuntime
          pane={pane}
          active={active}
          index={index}
          paneCount={paneCount}
          density={density}
          isFocused={slot.isFocused}
          spawnPaneDisabled={spawnPaneDisabled}
          onSpawnPane={onSpawnPane}
          onClosePane={onClosePane}
          onFocusPane={onFocusPane}
          onPaneCloseStateChange={onPaneCloseStateChange}
          runtimeProps={stableRuntimeProps}
          dragHandle={dragHandle}
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
    },
    [
      active,
      density,
      highChurnStore,
      onCancelAutonomousRun,
      onClosePane,
      onComposerControlsChange,
      onFocusPane,
      onInspectAutonomousRun,
      onLogout,
      onPaneCloseStateChange,
      onRefreshNotificationRoutes,
      onResolveOperatorAction,
      onResumeOperatorRun,
      onSpawnPane,
      onStartAutonomousRun,
      onStartLogin,
      onStartRuntimeRun,
      onStartRuntimeSession,
      onStopRuntimeRun,
      onSubmitManualCallback,
      onUpdateRuntimeRunControls,
      onUpsertNotificationRoute,
      paneCount,
      panes,
      spawnPaneDisabled,
      stableRuntimeProps,
    ],
  )

  return (
    <div
      ref={setWorkspaceDropNodeRef}
      className="flex h-full w-full min-h-0 min-w-0 flex-1"
      data-agent-workspace-drop-over={isWorkspaceDropOver ? 'true' : undefined}
      data-agent-workspace-drop-target
    >
      <PaneGrid
        slots={slots}
        splitterRatios={layout?.splitterRatios}
        onSplitterRatiosChange={onSplitterRatiosChange}
        onFocusPane={onFocusPane}
        renderPane={renderPane}
      />
    </div>
  )
})

interface PaneRuntimeWrapperProps extends PaneAwareRuntimeHandlers {
  pane: AgentWorkspacePaneView
  active: boolean
  index: number
  paneCount: number
  density: 'comfortable' | 'compact'
  isFocused: boolean
  spawnPaneDisabled: boolean
  onSpawnPane?: () => void
  onClosePane?: (paneId: string) => void
  onFocusPane?: (paneId: string) => void
  onPaneCloseStateChange?: (paneId: string, state: AgentPaneCloseState) => void
  dragHandle?: PaneDragHandle
  runtimeProps: Omit<AgentRuntimeProps,
    | 'agent'
    | 'active'
    | 'density'
    | 'paneId'
    | 'paneNumber'
    | 'paneCount'
    | 'onSpawnPane'
    | 'spawnPaneDisabled'
    | 'onClosePane'
    | 'onFocusPane'
    | 'isPaneFocused'
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
  active,
  index,
  paneCount,
  density,
  isFocused,
  spawnPaneDisabled,
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
  dragHandle,
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
    <LiveAgentRuntimeView
      {...runtimeProps}
      {...paneBoundHandlers}
      active={active}
      agent={pane.agent}
      highChurnStore={highChurnStore}
      density={density}
      paneId={paneId}
      paneNumber={index + 1}
      paneCount={paneCount}
      isPaneFocused={isFocused}
      onSpawnPane={onSpawnPane}
      spawnPaneDisabled={spawnPaneDisabled}
      onClosePane={handleClose}
      onFocusPane={handleFocus}
      onPaneCloseStateChange={handlePaneCloseStateChange}
      dragHandle={dragHandle}
    />
  )
})
