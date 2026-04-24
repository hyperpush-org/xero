"use client"

import { useMemo } from 'react'

import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  RuntimeRunView,
  RuntimeSessionView,
  UpsertNotificationRouteRequestDto,
} from '@/src/lib/cadence-model'
import {
  getRuntimeRunThinkingEffortLabel,
  getRuntimeStreamStatusLabel,
  type RuntimeRunControlInputDto,
} from '@/src/lib/cadence-model'

import { AgentFeedSection } from './agent-runtime/agent-feed-section'
import {
  getComposerApprovalOptions,
  getComposerModelGroups,
  getComposerModelOption,
  getComposerPlaceholder,
  getComposerThinkingOptions,
  getSelectedProviderId,
} from './agent-runtime/composer-helpers'
import { ComposerDock } from './agent-runtime/composer-dock'
import {
  getStreamRunId,
  getStreamStatusMeta,
  hasUsableRuntimeRunId,
} from './agent-runtime/runtime-stream-helpers'
import { displayValue, formatSequence } from './agent-runtime/shared-helpers'
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
  const streamStatusLabel = displayValue(
    agent.runtimeStreamStatusLabel,
    getRuntimeStreamStatusLabel(streamStatus),
  )

  const selectedProviderId = getSelectedProviderId(agent, runtimeSession)
  const selectedModelId = agent.selectedModelId?.trim() || null
  const availableModels = agent.providerModelCatalog.models
  const openrouterApiKeyConfigured = agent.openrouterApiKeyConfigured ?? false
  const providerMismatch = agent.providerMismatch ?? false
  const hasRepositoryBinding = Boolean(agent.repositoryPath?.trim())
  const canMutateRuntimeRun = !providerMismatch
  const canStartRuntimeRun = Boolean(
    canMutateRuntimeRun &&
      hasRepositoryBinding &&
      typeof onStartRuntimeRun === 'function' &&
      runtimeSession?.isAuthenticated,
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
    onUpdateRuntimeRunControls: canMutateRuntimeRun ? onUpdateRuntimeRunControls : undefined,
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
  const showNoRunStreamBanner = Boolean(runtimeSession?.isAuthenticated && !renderableRuntimeRun)
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
      selectedProfileReadiness: agent.selectedProfileReadiness ?? null,
      openrouterApiKeyConfigured,
      providerMismatch,
    },
  )
  const showAgentSetupEmptyState = Boolean(
    !providerMismatch && (!runtimeSession || runtimeSession.isSignedOut || runtimeSession.phase === 'idle'),
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
            </div>
          )}
        </div>

        <ComposerDock
          composerApprovalMode={agent.selectedApprovalMode}
          composerApprovalOptions={composerApprovalOptions}
          composerModelGroups={composerModelGroups}
          composerModelId={selectedModelId}
          composerThinkingLevel={agent.selectedThinkingEffort}
          composerThinkingOptions={composerThinkingOptions}
          composerThinkingPlaceholder={composerThinkingPlaceholder}
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
          runtimeRunActionError={controller.runtimeRunActionError}
          runtimeRunActionErrorTitle={controller.runtimeRunActionErrorTitle}
          runtimeRunActionStatus={agent.runtimeRunActionStatus}
          sendButtonLabel={sendButtonLabel}
          showStartRunButton={canStartRuntimeRun && !renderableRuntimeRun}
        />
      </div>
    </div>
  )
}
