"use client"

import { useMemo } from 'react'

import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  RuntimeRunView,
  RuntimeSessionView,
  UpsertNotificationRouteRequestDto,
} from '@/src/lib/cadence-model'
import { getRuntimeStreamStatusLabel } from '@/src/lib/cadence-model'

import { AgentFeedSection } from './agent-runtime/agent-feed-section'
import { CheckpointControlLoopSection } from './agent-runtime/checkpoint-control-loop-section'
import {
  createEmptyCheckpointControlLoop,
  getCheckpointControlLoopCoverageAlertMeta,
  getCheckpointControlLoopRecoveryAlertMeta,
} from './agent-runtime/checkpoint-control-loop-helpers'
import {
  getComposerCatalogStatusCopy,
  getComposerModelGroups,
  getComposerModelOption,
  getComposerPlaceholder,
  getComposerThinkingOptions,
  getSelectedProviderId,
} from './agent-runtime/composer-helpers'
import { ComposerDock } from './agent-runtime/composer-dock'
import { RecoveredRuntimeSection } from './agent-runtime/recovered-runtime-section'
import {
  getPrimaryRuntimeRunActionLabel,
  getRuntimeRunStatusText,
  getStreamRunId,
  getStreamStatusMeta,
  hasUsableRuntimeRunId,
} from './agent-runtime/runtime-stream-helpers'
import { displayValue, formatSequence, sortByNewest } from './agent-runtime/shared-helpers'
import { SetupEmptyState } from './agent-runtime/setup-empty-state'
import { useAgentRuntimeController } from './agent-runtime/use-agent-runtime-controller'

interface AgentRuntimeProps {
  agent: AgentPaneView
  onOpenSettings?: () => void
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onStartAutonomousRun?: () => Promise<unknown>
  onInspectAutonomousRun?: () => Promise<unknown>
  onCancelAutonomousRun?: (runId: string) => Promise<unknown>
  onStartRuntimeRun?: () => Promise<RuntimeRunView | null>
  onStartRuntimeSession?: () => Promise<RuntimeSessionView | null>
  onStopRuntimeRun?: (runId: string) => Promise<RuntimeRunView | null>
  onSubmitManualCallback?: (flowId: string, manualInput: string) => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  onRetryStream?: () => Promise<void>
  onResolveOperatorAction?: (
    actionId: string,
    decision: 'approve' | 'reject',
    options?: { userAnswer?: string | null },
  ) => Promise<unknown>
  onResumeOperatorRun?: (actionId: string, options?: { userAnswer?: string | null }) => Promise<unknown>
  onRefreshNotificationRoutes?: (options?: { force?: boolean }) => Promise<unknown>
  onUpsertNotificationRoute?: (
    request: Omit<UpsertNotificationRouteRequestDto, 'projectId'>,
  ) => Promise<unknown>
}

export function AgentRuntime({
  agent,
  onOpenSettings,
  onStartRuntimeRun,
  onStopRuntimeRun,
  onResolveOperatorAction,
  onResumeOperatorRun,
}: AgentRuntimeProps) {
  const runtimeSession = agent.runtimeSession ?? null
  const runtimeRun = agent.runtimeRun ?? null
  const renderableRuntimeRun = hasUsableRuntimeRunId(runtimeRun) ? runtimeRun : null
  const hasIncompleteRuntimeRunPayload = Boolean(runtimeRun && !renderableRuntimeRun)
  const runtimeStream = agent.runtimeStream ?? null
  const streamStatus = agent.runtimeStreamStatus ?? runtimeStream?.status ?? 'idle'
  const runtimeStreamItems = agent.runtimeStreamItems ?? runtimeStream?.items ?? []
  const activityItems = agent.activityItems ?? runtimeStream?.activityItems ?? []
  const skillItems = agent.skillItems ?? runtimeStream?.skillItems ?? []
  const transcriptItems = runtimeStream?.transcriptItems ?? []
  const toolCalls = runtimeStream?.toolCalls ?? []
  const streamIssue = agent.runtimeStreamError ?? runtimeStream?.lastIssue ?? null
  const checkpointControlLoop = agent.checkpointControlLoop ?? createEmptyCheckpointControlLoop()
  const streamStatusLabel = displayValue(
    agent.runtimeStreamStatusLabel,
    getRuntimeStreamStatusLabel(streamStatus),
  )
  const runtimeRunCheckpoints = useMemo(
    () => sortByNewest(renderableRuntimeRun?.checkpoints ?? [], (checkpoint) => checkpoint.createdAt).slice(0, 4),
    [renderableRuntimeRun],
  )

  const selectedProviderId = getSelectedProviderId(agent, runtimeSession)
  const selectedModelId = agent.selectedModelId?.trim() || null
  const availableModels = agent.providerModelCatalog.models
  const openrouterApiKeyConfigured = agent.openrouterApiKeyConfigured ?? false
  const providerMismatch = agent.providerMismatch ?? false
  const hasRepositoryBinding = Boolean(agent.repositoryPath?.trim())
  const canStartRuntimeRun = Boolean(
    hasRepositoryBinding && typeof onStartRuntimeRun === 'function' && (runtimeSession?.isAuthenticated || renderableRuntimeRun),
  )
  const canStopRuntimeRun = Boolean(
    hasRepositoryBinding && renderableRuntimeRun && !renderableRuntimeRun.isTerminal && typeof onStopRuntimeRun === 'function',
  )

  const controller = useAgentRuntimeController({
    projectId: agent.project.id,
    selectedModelId,
    availableModels,
    approvalRequests: agent.approvalRequests,
    operatorActionStatus: agent.operatorActionStatus,
    pendingOperatorActionId: agent.pendingOperatorActionId,
    renderableRuntimeRun,
    runtimeStream,
    runtimeStreamItems,
    runtimeRunActionError: agent.runtimeRunActionError,
    canStartRuntimeRun,
    canStopRuntimeRun,
    onStartRuntimeRun,
    onStopRuntimeRun,
    onResolveOperatorAction,
    onResumeOperatorRun,
  })

  const selectedComposerModel = useMemo(
    () => getComposerModelOption(availableModels, controller.composerModelId),
    [availableModels, controller.composerModelId],
  )
  const composerModelGroups = useMemo(
    () => getComposerModelGroups(availableModels, controller.composerModelId),
    [availableModels, controller.composerModelId],
  )
  const composerThinkingOptions = useMemo(
    () => getComposerThinkingOptions(selectedComposerModel),
    [selectedComposerModel],
  )
  const composerCatalogStatusCopy = useMemo(
    () => getComposerCatalogStatusCopy(agent.providerModelCatalog, selectedComposerModel),
    [agent.providerModelCatalog, selectedComposerModel],
  )
  const composerThinkingPlaceholder = controller.composerModelId
    ? 'Thinking unavailable'
    : 'Choose model'
  const streamStatusMeta = useMemo(() => getStreamStatusMeta(agent, runtimeSession), [agent, runtimeSession])
  const streamRunId = getStreamRunId(runtimeStream, renderableRuntimeRun)
  const streamSequenceLabel = formatSequence(runtimeStream?.lastSequence ?? null)
  const streamSessionLabel = displayValue(
    runtimeStream?.sessionId,
    runtimeSession?.sessionLabel ?? 'No session',
  )
  const checkpointTrustSnapshot =
    agent.trustSnapshot ?? {
      syncState: agent.notificationSyncError ? 'degraded' : 'unavailable',
      syncReason: agent.notificationSyncError
        ? agent.notificationSyncError.message
        : agent.notificationSyncSummary
          ? 'Cadence is keeping the last observed sync counts visible, but hook-owned trust projection is unavailable.'
          : 'No notification adapter sync summary is available yet.',
    }
  const checkpointControlLoopRecoveryAlert = getCheckpointControlLoopRecoveryAlertMeta({
    controlLoop: checkpointControlLoop,
    trustSnapshot: checkpointTrustSnapshot,
    autonomousRunErrorMessage: agent.autonomousRunErrorMessage ?? null,
    notificationSyncPollingActive: agent.notificationSyncPollingActive ?? false,
    notificationSyncPollingActionId: agent.notificationSyncPollingActionId ?? null,
    notificationSyncPollingBoundaryId: agent.notificationSyncPollingBoundaryId ?? null,
  })
  const checkpointControlLoopCoverageAlert = getCheckpointControlLoopCoverageAlertMeta(checkpointControlLoop)
  const runtimeRunStatusText = getRuntimeRunStatusText(renderableRuntimeRun)
  const primaryRuntimeRunActionLabel = getPrimaryRuntimeRunActionLabel(renderableRuntimeRun)
  const showNoRunStreamBanner = Boolean(runtimeSession?.isAuthenticated && !renderableRuntimeRun)
  const hasCheckpointControlLoopSurface = Boolean(checkpointControlLoop.totalCount > 0 || agent.operatorActionError)
  const hasAgentFeedSurface = Boolean(
    hasIncompleteRuntimeRunPayload ||
      renderableRuntimeRun ||
      runtimeSession?.isAuthenticated ||
      controller.recentRunReplacement ||
      streamIssue ||
      transcriptItems.length > 0 ||
      activityItems.length > 0 ||
      toolCalls.length > 0 ||
      skillItems.length > 0 ||
      (agent.actionRequiredItems ?? runtimeStream?.actionRequired ?? []).length > 0 ||
      runtimeStream?.completion ||
      runtimeStream?.failure,
  )
  const composerPlaceholder = getComposerPlaceholder(
    runtimeSession,
    streamStatus,
    renderableRuntimeRun,
    streamRunId,
    {
      selectedProviderId,
      openrouterApiKeyConfigured,
      providerMismatch,
    },
  )
  const showAgentSetupEmptyState = Boolean(
    !providerMismatch && (!runtimeSession || runtimeSession.isSignedOut || runtimeSession.phase === 'idle'),
  )

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <div
          className={
            showAgentSetupEmptyState
              ? 'flex flex-1 items-center justify-center overflow-y-auto scrollbar-thin px-6 py-5'
              : 'flex-1 overflow-y-auto scrollbar-thin px-4 py-4'
          }
        >
          {showAgentSetupEmptyState ? (
            <SetupEmptyState onOpenSettings={onOpenSettings} />
          ) : (
            <div className="mx-auto flex max-w-4xl flex-col gap-4">
              {hasIncompleteRuntimeRunPayload || renderableRuntimeRun ? (
                <RecoveredRuntimeSection
                  canStartRuntimeRun={canStartRuntimeRun}
                  canStopRuntimeRun={canStopRuntimeRun}
                  hasIncompleteRuntimeRunPayload={hasIncompleteRuntimeRunPayload}
                  onStartRuntimeRun={() => void controller.handleStartRuntimeRun()}
                  onStopRuntimeRun={() => void controller.handleStopRuntimeRun()}
                  pendingRuntimeRunAction={agent.pendingRuntimeRunAction ?? null}
                  primaryRuntimeRunActionLabel={primaryRuntimeRunActionLabel}
                  renderableRuntimeRun={renderableRuntimeRun}
                  runtimeRunActionError={controller.runtimeRunActionError}
                  runtimeRunActionErrorTitle={controller.runtimeRunActionErrorTitle}
                  runtimeRunActionStatus={agent.runtimeRunActionStatus ?? 'idle'}
                  runtimeRunCheckpoints={runtimeRunCheckpoints}
                  runtimeRunStatusText={runtimeRunStatusText}
                  runtimeRunUnavailableReason={agent.runtimeRunUnavailableReason}
                />
              ) : null}

              {hasAgentFeedSurface ? (
                <AgentFeedSection
                  activityItems={activityItems}
                  messagesUnavailableReason={agent.messagesUnavailableReason}
                  recentRunReplacement={controller.recentRunReplacement}
                  showNoRunStreamBanner={showNoRunStreamBanner}
                  skillItems={skillItems}
                  streamIssue={streamIssue}
                  streamRunId={streamRunId}
                  streamSequenceLabel={streamSequenceLabel}
                  streamSessionLabel={streamSessionLabel}
                  streamStatus={streamStatus}
                  streamStatusLabel={streamStatusLabel}
                  streamStatusMeta={streamStatusMeta}
                  toolCalls={toolCalls}
                  transcriptItems={transcriptItems}
                />
              ) : null}

              {hasCheckpointControlLoopSurface ? (
                <CheckpointControlLoopSection
                  checkpointControlLoop={checkpointControlLoop}
                  checkpointControlLoopCoverageAlert={checkpointControlLoopCoverageAlert}
                  checkpointControlLoopRecoveryAlert={checkpointControlLoopRecoveryAlert}
                  onOperatorAnswerChange={controller.handleOperatorAnswerChange}
                  onResolveOperatorAction={controller.handleResolveOperatorAction}
                  onResumeOperatorRun={controller.handleResumeOperatorRun}
                  operatorActionError={agent.operatorActionError}
                  operatorActionStatus={agent.operatorActionStatus}
                  operatorAnswers={controller.operatorAnswers}
                  pendingApprovalCount={agent.pendingApprovalCount}
                  pendingOperatorActionId={agent.pendingOperatorActionId}
                  pendingOperatorIntent={controller.pendingOperatorIntent}
                />
              ) : null}
            </div>
          )}
        </div>

        <ComposerDock
          composerModelGroups={composerModelGroups}
          composerModelId={controller.composerModelId}
          composerThinkingLevel={controller.composerThinkingLevel}
          composerThinkingOptions={composerThinkingOptions}
          composerThinkingPlaceholder={composerThinkingPlaceholder}
          catalogStatusLabel={composerCatalogStatusCopy.catalogLabel}
          catalogStatusDetail={composerCatalogStatusCopy.catalogDetail}
          thinkingStatusDetail={composerCatalogStatusCopy.thinkingDetail}
          onComposerModelChange={controller.setComposerModelId}
          onComposerThinkingLevelChange={controller.setComposerThinkingLevel}
          onStartRuntimeRun={() => void controller.handleStartRuntimeRun()}
          placeholder={composerPlaceholder}
          runtimeRunActionStatus={agent.runtimeRunActionStatus}
          showStartRunButton={canStartRuntimeRun && !renderableRuntimeRun}
        />
      </div>
    </div>
  )
}
