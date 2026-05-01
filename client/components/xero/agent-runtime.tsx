"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ChevronRight, Loader2, Plus } from 'lucide-react'

import { cn } from '@/lib/utils'
import type {
  AgentPaneView,
  AgentProviderModelView,
} from '@/src/features/xero/use-xero-desktop-state'
import type { XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import type {
  AgentSessionView,
  RuntimeRunView,
  RuntimeAutoCompactPreferenceDto,
  ProviderAuthSessionView,
  RuntimeSessionView,
  RuntimeStreamActionRequiredItemView,
  RuntimeStreamFailureItemView,
  RuntimeStreamToolItemView,
  RuntimeStreamViewItem,
  UpsertNotificationRouteRequestDto,
} from '@/src/lib/xero-model'
import type {
  DeleteSessionMemoryRequestDto,
  ExtractSessionMemoryCandidatesRequestDto,
  ExtractSessionMemoryCandidatesResponseDto,
  ListSessionMemoriesRequestDto,
  ListSessionMemoriesResponseDto,
  SessionMemoryRecordDto,
  SessionContextSnapshotDto,
  UpdateSessionMemoryRequestDto,
} from '@/src/lib/xero-model/session-context'
import {
  getRuntimeAgentLabel,
  getRuntimeRunThinkingEffortLabel,
  type RuntimeRunControlInputDto,
} from '@/src/lib/xero-model'

import {
  createEmptyCheckpointControlLoop,
  getCheckpointControlLoopCoverageAlertMeta,
  getCheckpointControlLoopRecoveryAlertMeta,
} from './agent-runtime/checkpoint-control-loop-helpers'
import { CheckpointControlLoopSection } from './agent-runtime/checkpoint-control-loop-section'
import {
  AgentContextMeter,
  type AgentContextMeterStatus,
} from './agent-runtime/agent-context-meter'
import {
  getComposerApprovalOptions,
  getComposerModelGroups,
  getComposerModelOption,
  getComposerPlaceholder,
  getComposerThinkingOptions,
} from './agent-runtime/composer-helpers'
import { ComposerDock } from './agent-runtime/composer-dock'
import { ConversationSection, type ConversationTurn } from './agent-runtime/conversation-section'
import { EmptySessionState } from './agent-runtime/empty-session-state'
import { MemoryReviewSection } from './agent-runtime/memory-review-section'
import {
  getStreamRunId,
  getToolSummaryContext,
  hasUsableRuntimeRunId,
} from './agent-runtime/runtime-stream-helpers'
import { SetupEmptyState } from './agent-runtime/setup-empty-state'
import { useAgentRuntimeController } from './agent-runtime/use-agent-runtime-controller'
import type { SpeechDictationAdapter } from './agent-runtime/use-speech-dictation'

type AgentRuntimeDesktopAdapter = SpeechDictationAdapter & Partial<Pick<XeroDesktopAdapter, 'getSessionContextSnapshot'>>

interface AgentRuntimeProps {
  agent: AgentPaneView
  onOpenSettings?: () => void
  onOpenDiagnostics?: () => void
  onStartLogin?: (options?: { originator?: string | null }) => Promise<ProviderAuthSessionView | null>
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
    autoCompact?: RuntimeAutoCompactPreferenceDto | null
  }) => Promise<RuntimeRunView | null>
  onComposerControlsChange?: (controls: RuntimeRunControlInputDto | null) => void
  onStartRuntimeSession?: (options?: { providerProfileId?: string | null }) => Promise<RuntimeSessionView | null>
  onStopRuntimeRun?: (runId: string) => Promise<RuntimeRunView | null>
  onSubmitManualCallback?: (flowId: string, manualInput: string) => Promise<ProviderAuthSessionView | null>
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
  onListSessionMemories?: (
    request: ListSessionMemoriesRequestDto,
  ) => Promise<ListSessionMemoriesResponseDto>
  onExtractSessionMemoryCandidates?: (
    request: ExtractSessionMemoryCandidatesRequestDto,
  ) => Promise<ExtractSessionMemoryCandidatesResponseDto>
  onUpdateSessionMemory?: (request: UpdateSessionMemoryRequestDto) => Promise<SessionMemoryRecordDto>
  onDeleteSessionMemory?: (request: DeleteSessionMemoryRequestDto) => Promise<void>
  onContextRefresh?: () => Promise<void>
  desktopAdapter?: AgentRuntimeDesktopAdapter
  /** GitHub avatar URL for the signed-in account, when available. */
  accountAvatarUrl?: string | null
  /** GitHub login for the signed-in account. */
  accountLogin?: string | null
  onCreateSession?: () => void
  isCreatingSession?: boolean
}

const EMPTY_ACTION_REQUIRED_ITEMS: NonNullable<AgentPaneView['actionRequiredItems']> = []
const MAX_VISIBLE_RUNTIME_FEED_ITEMS = 24

function appendTranscriptDelta(current: string, delta: string): string {
  if (!current) {
    return delta
  }

  if (!delta) {
    return current
  }

  if (/\s$/.test(current) || /^\s/.test(delta) || /^[.,!?;:%)\]}]/.test(delta)) {
    return `${current}${delta}`
  }

  return `${current} ${delta}`
}

function shouldShowActionItem(item: RuntimeStreamViewItem): item is RuntimeStreamToolItemView | RuntimeStreamActionRequiredItemView | RuntimeStreamFailureItemView {
  return item.kind === 'tool' || item.kind === 'action_required' || item.kind === 'failure'
}

function actionTurnFromItem(item: RuntimeStreamToolItemView | RuntimeStreamActionRequiredItemView): ConversationTurn {
  if (item.kind === 'action_required') {
    return {
      id: item.id,
      kind: 'action',
      sequence: item.sequence,
      title: item.title,
      detail: item.detail,
      state: null,
    }
  }

  const summary = getToolSummaryContext(item)
  return {
    id: item.id,
    kind: 'action',
    sequence: item.sequence,
    title: item.toolName,
    detail: item.detail ?? summary ?? 'Tool activity recorded.',
    state: item.toolState,
  }
}

function buildConversationTurns(runtimeStreamItems: RuntimeStreamViewItem[]): ConversationTurn[] {
  const turns: ConversationTurn[] = []

  for (const item of runtimeStreamItems) {
    if (item.kind === 'transcript') {
      if (item.role !== 'user' && item.role !== 'assistant') {
        continue
      }

      const previous = turns.at(-1)
      if (previous?.kind === 'message' && previous.role === item.role) {
        previous.text = appendTranscriptDelta(previous.text, item.text)
        previous.sequence = item.sequence
        continue
      }

      turns.push({
        id: item.id,
        kind: 'message',
        role: item.role,
        sequence: item.sequence,
        text: item.text,
      })
      continue
    }

    if (!shouldShowActionItem(item)) {
      continue
    }

    if (item.kind === 'failure') {
      turns.push({
        id: item.id,
        kind: 'failure',
        sequence: item.sequence,
        code: item.code,
        message: item.message,
      })
      continue
    }

    turns.push(actionTurnFromItem(item))
  }

  return turns.slice(-MAX_VISIBLE_RUNTIME_FEED_ITEMS)
}

function toContextMeterError(error: unknown): {
  code: string
  message: string
  retryable: boolean
} {
  const candidate = error as { code?: unknown; retryable?: unknown; message?: unknown } | null
  return {
    code: typeof candidate?.code === 'string' && candidate.code.trim().length > 0
      ? candidate.code
      : 'agent_context_meter_failed',
    message: typeof candidate?.message === 'string' && candidate.message.trim().length > 0
      ? candidate.message
      : error instanceof Error && error.message.trim().length > 0
        ? error.message
        : 'Xero could not refresh the context meter.',
    retryable: typeof candidate?.retryable === 'boolean' ? candidate.retryable : true,
  }
}

function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value)

  useEffect(() => {
    const timeout = window.setTimeout(() => setDebounced(value), delayMs)
    return () => window.clearTimeout(timeout)
  }, [delayMs, value])

  return debounced
}

function useAgentContextMeterSnapshot(options: {
  adapter?: AgentRuntimeDesktopAdapter
  projectId: string
  agentSessionId: string | null
  runId: string | null
  providerId: string | null
  modelId: string | null
  pendingPrompt: string
  lifecycleKey: string
}): {
  status: AgentContextMeterStatus
  snapshot: SessionContextSnapshotDto | null
  error: ReturnType<typeof toContextMeterError> | null
  refresh: () => void
} {
  const debouncedPendingPrompt = useDebouncedValue(options.pendingPrompt, 350)
  const [status, setStatus] = useState<AgentContextMeterStatus>('idle')
  const [snapshot, setSnapshot] = useState<SessionContextSnapshotDto | null>(null)
  const [error, setError] = useState<ReturnType<typeof toContextMeterError> | null>(null)
  const requestIdRef = useRef(0)
  const snapshotRef = useRef<SessionContextSnapshotDto | null>(null)

  useEffect(() => {
    snapshotRef.current = snapshot
  }, [snapshot])

  const refresh = useCallback(() => {
    if (!options.adapter?.getSessionContextSnapshot || !options.agentSessionId) {
      requestIdRef.current += 1
      setStatus('idle')
      setSnapshot(null)
      setError(null)
      return
    }

    const requestId = requestIdRef.current + 1
    requestIdRef.current = requestId
    setStatus((current) => (snapshotRef.current || current === 'ready' ? 'stale' : 'loading'))
    setError(null)

    void options.adapter
      .getSessionContextSnapshot({
        projectId: options.projectId,
        agentSessionId: options.agentSessionId,
        runId: options.runId,
        providerId: options.providerId,
        modelId: options.modelId,
        pendingPrompt: debouncedPendingPrompt,
      })
      .then((nextSnapshot) => {
        if (requestIdRef.current !== requestId) return
        setSnapshot(nextSnapshot)
        setStatus('ready')
        setError(null)
      })
      .catch((nextError) => {
        if (requestIdRef.current !== requestId) return
        setError(toContextMeterError(nextError))
        setStatus('error')
      })
  }, [
    debouncedPendingPrompt,
    options.adapter,
    options.agentSessionId,
    options.modelId,
    options.projectId,
    options.providerId,
    options.runId,
  ])

  useEffect(() => {
    refresh()
  }, [refresh, options.lifecycleKey])

  return { status, snapshot, error, refresh }
}

export function AgentRuntime({
  agent,
  onOpenSettings,
  onOpenDiagnostics,
  onStartRuntimeRun,
  onUpdateRuntimeRunControls,
  onComposerControlsChange,
  onStopRuntimeRun,
  onStartRuntimeSession,
  onResolveOperatorAction,
  onResumeOperatorRun,
  onListSessionMemories,
  onExtractSessionMemoryCandidates,
  onUpdateSessionMemory,
  onDeleteSessionMemory,
  onContextRefresh,
  desktopAdapter,
  accountAvatarUrl = null,
  accountLogin = null,
  onCreateSession,
  isCreatingSession = false,
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
  const actionRequiredItems = agent.actionRequiredItems ?? runtimeStream?.actionRequired ?? EMPTY_ACTION_REQUIRED_ITEMS
  const transcriptItems = runtimeStream?.transcriptItems ?? []
  const toolCalls = runtimeStream?.toolCalls ?? []
  const streamIssue = agent.runtimeStreamError ?? runtimeStream?.lastIssue ?? null
  const visibleTurns = useMemo(() => buildConversationTurns(runtimeStreamItems), [runtimeStreamItems])
  const selectedAgentSession = (agent.project.selectedAgentSession ?? null) as AgentSessionView | null
  const selectedAgentSessionId =
    selectedAgentSession?.agentSessionId ?? agent.project.selectedAgentSessionId ?? null

  const selectedProviderId =
    agent.selectedModel?.providerId ?? agent.selectedProviderId ?? runtimeSession?.providerId ?? 'openai_codex'
  const selectedModelId =
    agent.selectedModel?.modelId?.trim() || agent.selectedModelId?.trim() || null
  const composerModelOptionsView = useMemo<AgentProviderModelView[]>(
    () =>
      (agent.composerModelOptions ?? []).map((option) => ({
        selectionKey: option.selectionKey,
        profileId: option.profileId,
        profileLabel: null,
        providerId: option.providerId,
        providerLabel: option.providerLabel,
        modelId: option.modelId,
        label: option.modelId,
        displayName: option.displayName,
        groupId: option.providerId,
        groupLabel: option.providerLabel,
        availability: 'available',
        availabilityLabel: 'Available',
        thinkingSupported: option.thinking.supported,
        thinkingEffortOptions: option.thinkingEffortOptions,
        defaultThinkingEffort: option.defaultThinkingEffort,
      })),
    [agent.composerModelOptions],
  )
  const availableModels = composerModelOptionsView
  const hasRepositoryBinding = Boolean(agent.repositoryPath?.trim())
  const agentRuntimeBlocked = agent.agentRuntimeBlocked ?? false
  const selectedProviderReadyForSession = !agentRuntimeBlocked
  const canMutateRuntimeRun = !agentRuntimeBlocked
  const canStartRuntimeSession = Boolean(
    canMutateRuntimeRun &&
      hasRepositoryBinding &&
      typeof onStartRuntimeSession === 'function' &&
      selectedProviderReadyForSession &&
      (!runtimeSession?.isAuthenticated || runtimeSession.providerId !== selectedProviderId),
  )
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
    selectedModelSelectionKey: agent.selectedModelSelectionKey ?? agent.selectedModelOption?.selectionKey ?? selectedModelId,
    selectedRuntimeAgentId: agent.selectedRuntimeAgentId,
    selectedThinkingEffort: agent.selectedThinkingEffort,
    selectedApprovalMode: agent.selectedApprovalMode,
    selectedPrompt: agent.selectedPrompt,
    availableModels,
    approvalRequests: agent.approvalRequests,
    operatorActionStatus: agent.operatorActionStatus,
    pendingOperatorActionId: agent.pendingOperatorActionId,
    renderableRuntimeRun,
    runtimeStream,
    runtimeStreamItems,
    runtimeRunActionStatus: agent.runtimeRunActionStatus,
    runtimeRunActionError: agent.runtimeRunActionError,
    canStartRuntimeRun,
    canStartRuntimeSession,
    canStopRuntimeRun,
    actionRequiredItems,
    dictationAdapter: desktopAdapter,
    dictationScopeKey: `${agent.project.id}:${agent.project.selectedAgentSessionId ?? 'none'}`,
    onStartRuntimeRun,
    onStartRuntimeSession,
    onUpdateRuntimeRunControls: canMutateRuntimeRun ? onUpdateRuntimeRunControls : undefined,
    onComposerControlsChange,
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
  const composerApprovalOptions = useMemo(() => getComposerApprovalOptions(), [])
  const streamRunId = getStreamRunId(runtimeStream, renderableRuntimeRun)
  const contextMeterState = useAgentContextMeterSnapshot({
    adapter: desktopAdapter,
    projectId: agent.project.id,
    agentSessionId: selectedAgentSessionId,
    runId: renderableRuntimeRun?.runId ?? streamRunId ?? null,
    providerId: selectedComposerModel?.providerId ?? selectedProviderId,
    modelId: selectedComposerModel?.modelId ?? selectedModelId,
    pendingPrompt: controller.draftPrompt,
    lifecycleKey: [
      renderableRuntimeRun?.runId ?? 'no-run',
      renderableRuntimeRun?.updatedAt ?? 'no-run-update',
      runtimeStream?.status ?? 'idle',
      runtimeStream?.lastItemAt ?? 'no-stream-update',
      agent.pendingRuntimeRunAction ?? 'no-action',
    ].join(':'),
  })
  const contextMeter =
    contextMeterState.status === 'idle' ? null : (
      <AgentContextMeter
        status={contextMeterState.status}
        snapshot={contextMeterState.snapshot}
        error={contextMeterState.error}
        onRefresh={contextMeterState.refresh}
      />
    )
  const composerThinkingPlaceholder = controller.composerThinkingEffort
    ? getRuntimeRunThinkingEffortLabel(controller.composerThinkingEffort)
    : controller.composerModelId
      ? 'Thinking unavailable'
      : 'Choose model'
  const checkpointControlLoop = agent.checkpointControlLoop ?? createEmptyCheckpointControlLoop()
  const checkpointControlLoopRecoveryAlert = getCheckpointControlLoopRecoveryAlertMeta({
    controlLoop: checkpointControlLoop,
    trustSnapshot: {
      syncState: agent.trustSnapshot?.syncState ?? 'unavailable',
      syncReason:
        agent.trustSnapshot?.syncReason ??
        'Xero has not projected notification sync trust for this project yet.',
    },
    autonomousRunErrorMessage: agent.autonomousRunErrorMessage,
    notificationSyncPollingActive: agent.notificationSyncPollingActive ?? false,
    notificationSyncPollingActionId: agent.notificationSyncPollingActionId ?? null,
    notificationSyncPollingBoundaryId: agent.notificationSyncPollingBoundaryId ?? null,
  })
  const checkpointControlLoopCoverageAlert = getCheckpointControlLoopCoverageAlertMeta(checkpointControlLoop)
  const showCheckpointControlLoopSection =
    checkpointControlLoop.items.length > 0 ||
    Boolean(checkpointControlLoopRecoveryAlert) ||
    Boolean(checkpointControlLoopCoverageAlert)
  const baseComposerPlaceholder = getComposerPlaceholder(
    runtimeSession,
    streamStatus,
    renderableRuntimeRun,
    streamRunId,
    {
      selectedProviderId,
      agentRuntimeBlocked,
    },
  )
  const composerPlaceholder =
    controller.composerRuntimeAgentId === 'ask' &&
    !agentRuntimeBlocked &&
    runtimeSession?.isAuthenticated &&
    !renderableRuntimeRun?.isTerminal
      ? 'Ask about this project...'
      : baseComposerPlaceholder
  const showAgentSetupEmptyState = Boolean(
    agentRuntimeBlocked &&
      (!runtimeSession || runtimeSession.isSignedOut || runtimeSession.phase === 'idle'),
  )
  const hasSessionActivity = Boolean(
    hasIncompleteRuntimeRunPayload ||
      renderableRuntimeRun ||
      controller.recentRunReplacement ||
      streamIssue ||
      transcriptItems.length > 0 ||
      activityItems.length > 0 ||
      toolCalls.length > 0 ||
      skillItems.length > 0 ||
      actionRequiredItems.length > 0 ||
      runtimeStream?.completion ||
      runtimeStream?.failure,
  )
  const promptInputLabel = controller.promptInputAvailable ? 'Agent input' : 'Agent input unavailable'
  const sendButtonLabel = controller.promptInputAvailable ? 'Send message' : 'Send message unavailable'
  const isProviderLoggedIn = Boolean(
    selectedProviderReadyForSession ||
      runtimeSession?.isAuthenticated,
  )
  const showEmptySessionState = Boolean(
    !showAgentSetupEmptyState && !agentRuntimeBlocked && isProviderLoggedIn && !hasSessionActivity,
  )
  const projectLabel =
    agent.project.repository?.displayName ?? agent.project.name ?? 'this project'
  const sessionLabel = agent.project.selectedAgentSession?.title?.trim() || 'New Chat'
  const refreshContextMeter = contextMeterState.refresh
  const handleContextRefresh = useCallback(async () => {
    await onContextRefresh?.()
    refreshContextMeter()
  }, [onContextRefresh, refreshContextMeter])

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <div className="relative flex min-w-0 flex-1 flex-col">
        <div className="pointer-events-none absolute inset-x-0 top-0 z-20 flex items-center justify-between gap-2 px-4 pt-2.5 pb-2.5">
          <div className="pointer-events-auto flex min-w-0 items-center gap-2 text-[13px] text-muted-foreground">
            <span className="truncate font-semibold text-foreground">{projectLabel}</span>
            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground/70" />
            <span className="truncate font-medium">{sessionLabel}</span>
          </div>
          {onCreateSession ? (
            <button
              type="button"
              aria-label="New session"
              onClick={onCreateSession}
              disabled={isCreatingSession}
              className={cn(
                'pointer-events-auto inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-[13px] font-semibold text-muted-foreground transition-colors',
                'hover:bg-primary/10 hover:text-primary',
                'disabled:cursor-not-allowed disabled:opacity-50',
              )}
            >
              {isCreatingSession ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Plus className="h-4 w-4" />
              )}
              <span>New Session</span>
            </button>
          ) : null}
        </div>
        <div
          className={
            showAgentSetupEmptyState || showEmptySessionState
              ? 'flex flex-1 items-center justify-center overflow-y-auto scrollbar-thin px-6 py-5'
              : 'flex-1 overflow-y-auto scrollbar-thin px-4 pt-14 pb-4'
          }
        >
          {showAgentSetupEmptyState ? (
            <SetupEmptyState onOpenSettings={onOpenSettings} />
          ) : showEmptySessionState ? (
            <EmptySessionState
              projectLabel={projectLabel}
              onSelectSuggestion={(prompt) => {
                controller.handleDraftPromptChange(prompt)
                controller.promptInputRef.current?.focus()
              }}
            />
          ) : (
            <div className="mx-auto flex max-w-4xl flex-col gap-4">
              <ConversationSection
                runtimeRun={renderableRuntimeRun}
                visibleTurns={visibleTurns}
                streamIssue={streamIssue}
                streamFailure={runtimeStream?.failure ?? null}
                streamCompletion={runtimeStream?.completion ?? null}
                accountAvatarUrl={accountAvatarUrl}
                accountLogin={accountLogin}
              />
              {showCheckpointControlLoopSection ? (
                <CheckpointControlLoopSection
                  checkpointControlLoop={checkpointControlLoop}
                  pendingApprovalCount={agent.pendingApprovalCount ?? 0}
                  operatorActionError={agent.operatorActionError ?? null}
                  operatorActionStatus={agent.operatorActionStatus}
                  pendingOperatorActionId={agent.pendingOperatorActionId ?? null}
                  pendingOperatorIntent={controller.pendingOperatorIntent}
                  operatorAnswers={controller.operatorAnswers}
                  checkpointControlLoopRecoveryAlert={checkpointControlLoopRecoveryAlert}
                  checkpointControlLoopCoverageAlert={checkpointControlLoopCoverageAlert}
                  onOperatorAnswerChange={controller.handleOperatorAnswerChange}
                  onResolveOperatorAction={controller.handleResolveOperatorAction}
                  onResumeOperatorRun={controller.handleResumeOperatorRun}
                  onResumeLiveActionRequired={controller.handleResumeLiveActionRequired}
                />
              ) : null}
              <MemoryReviewSection
                projectId={agent.project.id}
                selectedSession={selectedAgentSession}
                runId={renderableRuntimeRun?.runId ?? null}
                onListSessionMemories={onListSessionMemories}
                onExtractSessionMemoryCandidates={onExtractSessionMemoryCandidates}
                onUpdateSessionMemory={onUpdateSessionMemory}
                onDeleteSessionMemory={onDeleteSessionMemory}
                onContextRefresh={handleContextRefresh}
              />
            </div>
          )}
        </div>

        <ComposerDock
          composerRuntimeAgentId={controller.composerRuntimeAgentId}
          composerRuntimeAgentLabel={getRuntimeAgentLabel(controller.composerRuntimeAgentId)}
          composerApprovalMode={controller.composerApprovalMode}
          composerApprovalOptions={composerApprovalOptions}
          autoCompactEnabled={controller.autoCompactEnabled}
          composerModelGroups={composerModelGroups}
          composerModelId={controller.composerModelId}
          composerThinkingLevel={controller.composerThinkingEffort}
          composerThinkingOptions={composerThinkingOptions}
          composerThinkingPlaceholder={composerThinkingPlaceholder}
          controlsDisabled={controller.areControlsDisabled}
          runtimeAgentSwitchDisabled={controller.isRuntimeAgentSwitchDisabled}
          dictation={controller.dictation}
          contextMeter={contextMeter}
          draftPrompt={controller.draftPrompt}
          isPromptDisabled={controller.isPromptDisabled}
          isSendDisabled={!controller.canSubmitPrompt}
          onComposerApprovalModeChange={controller.handleComposerApprovalModeChange}
          onComposerRuntimeAgentChange={controller.handleComposerRuntimeAgentChange}
          onAutoCompactEnabledChange={controller.handleAutoCompactEnabledChange}
          onComposerModelChange={controller.handleComposerModelChange}
          onComposerThinkingLevelChange={controller.handleComposerThinkingLevelChange}
          onDraftPromptChange={controller.handleDraftPromptChange}
          onSubmitDraftPrompt={() => void controller.handleSubmitDraftPrompt()}
          pendingRuntimeRunAction={agent.pendingRuntimeRunAction ?? null}
          placeholder={composerPlaceholder}
          promptInputRef={controller.promptInputRef}
          promptInputLabel={promptInputLabel}
          runtimeSessionBindInFlight={controller.runtimeSessionBindInFlight}
          runtimeRunActionError={controller.runtimeRunActionError}
          runtimeRunActionErrorTitle={controller.runtimeRunActionErrorTitle}
          runtimeRunActionStatus={agent.runtimeRunActionStatus}
          sendButtonLabel={sendButtonLabel}
          onOpenDiagnostics={onOpenDiagnostics}
        />
      </div>
    </div>
  )
}
