"use client"

import { useMemo } from 'react'

import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  RuntimeRunView,
  RuntimeSessionView,
  UpsertNotificationRouteRequestDto,
} from '@/src/lib/cadence-model'
import {
  getRuntimeRunApprovalModeLabel,
  getRuntimeRunThinkingEffortLabel,
  getRuntimeStreamStatusLabel,
  type RuntimeRunControlInputDto,
} from '@/src/lib/cadence-model'

import { AgentFeedSection } from './agent-runtime/agent-feed-section'
import { CheckpointControlLoopSection } from './agent-runtime/checkpoint-control-loop-section'
import {
  createEmptyCheckpointControlLoop,
  getCheckpointControlLoopCoverageAlertMeta,
  getCheckpointControlLoopRecoveryAlertMeta,
} from './agent-runtime/checkpoint-control-loop-helpers'
import {
  getComposerApprovalOptions,
  getComposerCatalogStatusCopy,
  getComposerControlStatusCopy,
  getComposerModelGroups,
  getComposerModelOption,
  getComposerPlaceholder,
  getComposerPromptStatusCopy,
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
  onStartRuntimeRun?: (options?: {
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
  }) => Promise<RuntimeRunView | null>
  onUpdateRuntimeRunControls?: (request?: {
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
  }) => Promise<RuntimeRunView | null>
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
  onUpdateRuntimeRunControls,
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
    hasRepositoryBinding && typeof onStartRuntimeRun === 'function' && runtimeSession?.isAuthenticated,
  )
  const canStopRuntimeRun = Boolean(
    hasRepositoryBinding && renderableRuntimeRun && !renderableRuntimeRun.isTerminal && typeof onStopRuntimeRun === 'function',
  )

  const controller = useAgentRuntimeController({
    projectId: agent.project.id,
    selectedModelId,
    selectedThinkingEffort: agent.selectedThinkingEffort,
    selectedApprovalMode: agent.selectedApprovalMode,
    selectedPrompt: agent.selectedPrompt,
    availableModels,
    approvalRequests: agent.approvalRequests,
    operatorActionStatus: agent.operatorActionStatus,
    pendingOperatorActionId: agent.pendingOperatorActionId,
    pendingRuntimeRunAction: agent.pendingRuntimeRunAction,
    renderableRuntimeRun,
    runtimeRunPendingControls: agent.runtimeRunPendingControls,
    runtimeStream,
    runtimeStreamItems,
    runtimeRunActionStatus: agent.runtimeRunActionStatus,
    runtimeRunActionError: agent.runtimeRunActionError,
    canStartRuntimeRun,
    canStopRuntimeRun,
    onStartRuntimeRun,
    onUpdateRuntimeRunControls,
    onStopRuntimeRun,
    onResolveOperatorAction,
    onResumeOperatorRun,
  })

  const selectedComposerModel = useMemo(
    () => getComposerModelOption(availableModels, selectedModelId),
    [availableModels, selectedModelId],
  )
  const composerModelGroups = useMemo(
    () => getComposerModelGroups(availableModels, selectedModelId),
    [availableModels, selectedModelId],
  )
  const composerThinkingOptions = useMemo(
    () => getComposerThinkingOptions(selectedComposerModel),
    [selectedComposerModel],
  )
  const composerApprovalOptions = useMemo(() => getComposerApprovalOptions(), [])
  const composerCatalogStatusCopy = useMemo(
    () => getComposerCatalogStatusCopy(agent.providerModelCatalog, selectedComposerModel),
    [agent.providerModelCatalog, selectedComposerModel],
  )
  const composerThinkingPlaceholder = agent.selectedThinkingEffort
    ? getRuntimeRunThinkingEffortLabel(agent.selectedThinkingEffort)
    : selectedModelId
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
  const composerPromptStatus = useMemo(
    () =>
      getComposerPromptStatusCopy({
        selectedPrompt: agent.selectedPrompt,
        runtimeRun: renderableRuntimeRun,
        canStartRuntimeRun,
        runtimeRunActionStatus: agent.runtimeRunActionStatus,
        pendingRuntimeRunAction: agent.pendingRuntimeRunAction,
        runtimeRunActionError: controller.runtimeRunActionError,
      }),
    [
      agent.pendingRuntimeRunAction,
      agent.runtimeRunActionStatus,
      agent.selectedPrompt,
      canStartRuntimeRun,
      controller.runtimeRunActionError,
      renderableRuntimeRun,
    ],
  )
  const composerModelStatus = useMemo(
    () =>
      getComposerControlStatusCopy({
        label: 'Model',
        selectedLabel: displayValue(selectedComposerModel?.label, 'Model unavailable'),
        truthSource: agent.controlTruthSource,
        activeLabel: agent.runtimeRunActiveControls ? getComposerModelOption(availableModels, agent.runtimeRunActiveControls.modelId)?.label ?? agent.runtimeRunActiveControls.modelId : null,
        activeRevision: agent.runtimeRunActiveControls?.revision ?? null,
        activeAt: agent.runtimeRunActiveControls?.appliedAt ?? null,
        pendingLabel: agent.runtimeRunPendingControls ? getComposerModelOption(availableModels, agent.runtimeRunPendingControls.modelId)?.label ?? agent.runtimeRunPendingControls.modelId : null,
        pendingRevision: agent.runtimeRunPendingControls?.revision ?? null,
        pendingAt: agent.runtimeRunPendingControls?.queuedAt ?? null,
      }),
    [
      agent.controlTruthSource,
      agent.runtimeRunActiveControls,
      agent.runtimeRunPendingControls,
      availableModels,
      selectedComposerModel,
    ],
  )
  const composerThinkingStatus = useMemo(
    () =>
      getComposerControlStatusCopy({
        label: 'Thinking',
        selectedLabel: getRuntimeRunThinkingEffortLabel(agent.selectedThinkingEffort),
        truthSource: agent.controlTruthSource,
        activeLabel: agent.runtimeRunActiveControls?.thinkingEffortLabel ?? null,
        activeRevision: agent.runtimeRunActiveControls?.revision ?? null,
        activeAt: agent.runtimeRunActiveControls?.appliedAt ?? null,
        pendingLabel: agent.runtimeRunPendingControls?.thinkingEffortLabel ?? null,
        pendingRevision: agent.runtimeRunPendingControls?.revision ?? null,
        pendingAt: agent.runtimeRunPendingControls?.queuedAt ?? null,
      }),
    [agent.controlTruthSource, agent.runtimeRunActiveControls, agent.runtimeRunPendingControls, agent.selectedThinkingEffort],
  )
  const composerApprovalStatus = useMemo(
    () =>
      getComposerControlStatusCopy({
        label: 'Approval',
        selectedLabel: getRuntimeRunApprovalModeLabel(agent.selectedApprovalMode),
        truthSource: agent.controlTruthSource,
        activeLabel: agent.runtimeRunActiveControls?.approvalModeLabel ?? null,
        activeRevision: agent.runtimeRunActiveControls?.revision ?? null,
        activeAt: agent.runtimeRunActiveControls?.appliedAt ?? null,
        pendingLabel: agent.runtimeRunPendingControls?.approvalModeLabel ?? null,
        pendingRevision: agent.runtimeRunPendingControls?.revision ?? null,
        pendingAt: agent.runtimeRunPendingControls?.queuedAt ?? null,
      }),
    [agent.controlTruthSource, agent.runtimeRunActiveControls, agent.runtimeRunPendingControls, agent.selectedApprovalMode],
  )
  const promptInputLabel = controller.promptInputAvailable ? 'Agent input' : 'Agent input unavailable'
  const sendButtonLabel = controller.promptInputAvailable ? 'Send message' : 'Send message unavailable'

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
          catalogStatusDetail={composerCatalogStatusCopy.catalogDetail}
          catalogStatusLabel={composerCatalogStatusCopy.catalogLabel}
          composerApprovalMode={agent.selectedApprovalMode}
          composerApprovalOptions={composerApprovalOptions}
          composerApprovalStatus={composerApprovalStatus}
          composerModelGroups={composerModelGroups}
          composerModelId={selectedModelId}
          composerModelStatus={composerModelStatus}
          composerThinkingLevel={agent.selectedThinkingEffort}
          composerThinkingOptions={composerThinkingOptions}
          composerThinkingPlaceholder={composerThinkingPlaceholder}
          composerThinkingStatus={composerThinkingStatus}
          controlsDisabled={controller.areControlsDisabled}
          draftPrompt={controller.draftPrompt}
          isPromptDisabled={controller.isPromptDisabled}
          isSendDisabled={!controller.canSubmitPrompt}
          onComposerApprovalModeChange={controller.handleComposerApprovalModeChange}
          onComposerModelChange={controller.handleComposerModelChange}
          onComposerThinkingLevelChange={controller.handleComposerThinkingLevelChange}
          onDraftPromptChange={controller.handleDraftPromptChange}
          onStartRuntimeRun={() => void controller.handleStartRuntimeRun()}
          onSubmitDraftPrompt={() => void controller.handleSubmitDraftPrompt()}
          pendingRuntimeRunAction={agent.pendingRuntimeRunAction ?? null}
          placeholder={composerPlaceholder}
          promptInputLabel={promptInputLabel}
          promptStatus={composerPromptStatus}
          runtimeRunActionError={controller.runtimeRunActionError}
          runtimeRunActionErrorTitle={controller.runtimeRunActionErrorTitle}
          runtimeRunActionStatus={agent.runtimeRunActionStatus}
          sendButtonLabel={sendButtonLabel}
          showStartRunButton={canStartRuntimeRun && !renderableRuntimeRun}
          thinkingStatusDetail={composerCatalogStatusCopy.thinkingDetail}
        />
      </div>
    </div>
  )
}
