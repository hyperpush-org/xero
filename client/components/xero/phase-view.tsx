'use client'

import { memo, useCallback, useState, type ReactNode } from 'react'
import {
  AlertCircle,
  Bot,
  GitPullRequestArrow,
  Loader2,
  Play,
  Save,
  Square,
  Workflow as WorkflowIcon,
  X,
} from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { WorkflowCanvasEmptyState } from '@/components/xero/workflow-canvas-empty-state'
import {
  AgentVisualization,
  type AgentVisualizationEditingStatus,
} from '@/components/xero/workflow-canvas/agent-visualization'
import {
  WorkflowDefinitionCanvas,
  type WorkflowDefinitionCanvasStatus,
} from '@/components/xero/workflow-canvas/workflow-definition-canvas'
import type { CanvasMode } from '@/components/xero/workflow-canvas/canvas-mode-context'
import { cn } from '@/lib/utils'
import type { WorkflowPaneView } from '@/src/features/xero/use-xero-desktop-state'
import type {
  AgentDetailStatus,
  AgentListStatus,
} from '@/src/features/xero/use-workflow-agent-inspector'
import type {
  AgentDefinitionPreviewResponseDto,
  AgentDefinitionWriteResponseDto,
} from '@/src/lib/xero-model/agent-definition'
import type { WorkflowDefinitionDto } from '@/src/lib/xero-model/workflow-definition'
import type {
  WorkflowRunBlockerResponseDto,
  WorkflowRunBundleResponseDto,
  WorkflowRunDto,
} from '@/src/lib/xero-model/workflow-run'
import type { WorkflowTemplateIdDto } from '@/src/lib/xero-model/workflow-templates'
import type {
  AgentAuthoringCatalogDto,
  AgentAuthoringAttachableSkillDto,
  AgentAuthoringSkillSearchResultDto,
  AgentRefDto,
  AgentToolPackCatalogDto,
  SearchAgentAuthoringSkillsResponseDto,
  WorkflowAgentDetailDto,
  WorkflowAgentSummaryDto,
} from '@/src/lib/xero-model/workflow-agents'

interface PhaseViewProps {
  active?: boolean
  projectId?: string | null
  workflow?: WorkflowPaneView
  onStartRun?: () => Promise<unknown>
  onOpenSettings?: () => void
  canStartRun?: boolean
  isStartingRun?: boolean
  onToggleWorkflows?: () => void
  workflowsOpen?: boolean
  onCreateAgent?: () => void
  onCreateAgentFromTemplate?: (ref: AgentRefDto) => void
  onEditAgentFromWorkflow?: (ref: AgentRefDto) => void
  selectedWorkflowDefinition?: WorkflowDefinitionDto | null
  selectedWorkflowRun?: WorkflowRunDto | null
  selectedWorkflowIsDraft?: boolean
  selectedWorkflowIsTemplatePreview?: boolean
  workflowActionRunning?: boolean
  onCreateWorkflow?: () => void
  onCreateWorkflowWithAgentCreate?: () => void
  onCreateWorkflowFromTemplate?: (templateId: WorkflowTemplateIdDto) => void
  onSaveWorkflowDefinition?: (definition: WorkflowDefinitionDto) => Promise<WorkflowDefinitionDto | void>
  onCancelWorkflowEditing?: () => void
  onClearWorkflowSelection?: () => void
  onStartWorkflowDefinitionRun?: (workflowId: string, initialInput: unknown) => Promise<WorkflowRunDto | void>
  onCancelWorkflowRun?: (runId: string) => Promise<WorkflowRunDto | void>
  onRetryWorkflowNodeRun?: (runId: string, nodeRunId: string) => Promise<WorkflowRunDto | void>
  onSkipWorkflowBranch?: (
    runId: string,
    nodeRunId: string,
    reason?: string,
  ) => Promise<WorkflowRunDto | void>
  onResumeWorkflowCheckpoint?: (
    runId: string,
    nodeRunId: string,
    decision: string,
    payload: unknown,
  ) => Promise<WorkflowRunDto | void>
  onExplainWorkflowRunBlocker?: (runId: string) => Promise<WorkflowRunBlockerResponseDto | void>
  onExportWorkflowRunBundle?: (runId: string) => Promise<WorkflowRunBundleResponseDto | void>
  onResumeWorkflowNextIncompletePhase?: (runId: string) => Promise<WorkflowRunDto | void>
  templates?: WorkflowAgentSummaryDto[]
  templatesLoading?: boolean
  templatesError?: Error | null
  agentDetail?: WorkflowAgentDetailDto | null
  agentDetailStatus?: AgentDetailStatus | AgentListStatus
  agentDetailError?: Error | null
  onClearAgentSelection?: () => void
  onReloadAgentDetail?: () => Promise<void>
  authoringSession?: {
    mode: CanvasMode
    initialDetail: WorkflowAgentDetailDto | null
  } | null
  authoringCatalog?: AgentAuthoringCatalogDto | null
  toolPackCatalog?: AgentToolPackCatalogDto | null
  onSearchAttachableSkills?: (params: {
    query: string
    offset: number
    limit: number
  }) => Promise<SearchAgentAuthoringSkillsResponseDto>
  onResolveAttachableSkill?: (
    skill: AgentAuthoringSkillSearchResultDto,
  ) => Promise<AgentAuthoringAttachableSkillDto>
  onAuthoringSubmit?: (params: {
    snapshot: Record<string, unknown>
    mode: CanvasMode
    definitionId?: string
  }) => Promise<AgentDefinitionWriteResponseDto>
  onAuthoringSaved?: (response: AgentDefinitionWriteResponseDto) => void
  onAuthoringCancel?: () => void
  onReadProjectUiState?: (key: string) => Promise<unknown | null>
  onWriteProjectUiState?: (key: string, value: unknown | null) => Promise<void>
  onSelectedNodeChange?: (hasSelection: boolean) => void
  onPreviewEffectiveRuntime?: (params: {
    snapshot: Record<string, unknown>
    definitionId: string | null
  }) => Promise<AgentDefinitionPreviewResponseDto>
}

const AUTHORING_TITLE_BY_MODE: Record<CanvasMode, string> = {
  create: 'New agent',
  edit: 'Editing agent',
  duplicate: 'Duplicating agent',
}

const AUTHORING_SAVE_LABEL_BY_MODE: Record<CanvasMode, string> = {
  create: 'Save agent',
  edit: 'Save changes',
  duplicate: 'Save copy',
}

export const PhaseView = memo(function PhaseView(props: PhaseViewProps) {
  const {
    onToggleWorkflows,
    active = true,
    workflowsOpen = false,
    onCreateAgent,
    onCreateAgentFromTemplate,
    onEditAgentFromWorkflow,
    selectedWorkflowDefinition = null,
    selectedWorkflowRun = null,
    selectedWorkflowIsDraft = false,
    selectedWorkflowIsTemplatePreview = false,
    workflowActionRunning = false,
    onCreateWorkflow,
    onCreateWorkflowWithAgentCreate,
    onCreateWorkflowFromTemplate,
    onSaveWorkflowDefinition,
    onCancelWorkflowEditing,
    onClearWorkflowSelection,
    onStartWorkflowDefinitionRun,
    onCancelWorkflowRun,
    onRetryWorkflowNodeRun,
    onSkipWorkflowBranch,
    onResumeWorkflowCheckpoint,
    onExplainWorkflowRunBlocker,
    onExportWorkflowRunBundle,
    onResumeWorkflowNextIncompletePhase,
    templates = [],
    templatesLoading = false,
    templatesError = null,
    agentDetail = null,
    agentDetailStatus = 'idle',
    agentDetailError = null,
    onClearAgentSelection,
    onReloadAgentDetail,
    authoringSession = null,
    authoringCatalog = null,
    toolPackCatalog = null,
    onSearchAttachableSkills,
    onResolveAttachableSkill,
    onAuthoringSubmit,
    onAuthoringSaved,
    onAuthoringCancel,
    onReadProjectUiState,
    onWriteProjectUiState,
    onSelectedNodeChange,
    onPreviewEffectiveRuntime,
    projectId = null,
  } = props

  const isAuthoring = Boolean(authoringSession && onAuthoringSubmit && onAuthoringSaved && onAuthoringCancel)
  const [editingStatus, setEditingStatus] =
    useState<AgentVisualizationEditingStatus | null>(null)
  const [workflowCanvasStatus, setWorkflowCanvasStatus] =
    useState<WorkflowDefinitionCanvasStatus | null>(null)
  const handleEditingStatusChange = useCallback(
    (status: AgentVisualizationEditingStatus | null) => {
      setEditingStatus(status)
    },
    [],
  )
  const handleWorkflowCanvasStatusChange = useCallback(
    (status: WorkflowDefinitionCanvasStatus | null) => {
      setWorkflowCanvasStatus(status)
    },
    [],
  )
  const showAgentVisualization =
    !isAuthoring && agentDetailStatus === 'ready' && agentDetail !== null
  const showWorkflowDefinition = !isAuthoring && !showAgentVisualization && selectedWorkflowDefinition !== null
  const selectedAgent = showAgentVisualization ? agentDetail : null
  const selectedAgentHeader = selectedAgent?.header ?? null
  const selectedAgentIsSystem = selectedAgent?.ref.kind === 'built_in'
  const authoringMode = authoringSession?.mode ?? null
  const authoringInitialHeader = authoringSession?.initialDetail?.header ?? null
  const authoringDisplayName =
    authoringMode === 'create' || !authoringInitialHeader
      ? AUTHORING_TITLE_BY_MODE[authoringMode ?? 'create']
      : authoringInitialHeader.displayName
  const authoringShortLabel =
    authoringMode && authoringMode !== 'create' && authoringInitialHeader
      ? authoringInitialHeader.shortLabel
      : null
  const showTopLeftHeader = isAuthoring || showAgentVisualization || showWorkflowDefinition
  const workflowHeaderDefinition =
    workflowCanvasStatus?.definition ?? selectedWorkflowDefinition
  const canvasDiagnosticsStatus = isAuthoring
    ? editingStatus
    : showWorkflowDefinition
      ? workflowCanvasStatus
      : null
  const emptyCanvasState = (
    <WorkflowCanvasEmptyState
      onCreateAgent={onCreateAgent}
      onCreateAgentFromTemplate={onCreateAgentFromTemplate}
      onCreateWorkflow={onCreateWorkflow}
      onCreateWorkflowWithAgentCreate={onCreateWorkflowWithAgentCreate}
      onCreateWorkflowFromTemplate={onCreateWorkflowFromTemplate}
      templates={templates}
      templatesLoading={templatesLoading}
      templatesError={templatesError}
      onBrowseWorkflows={
        onToggleWorkflows && !workflowsOpen ? onToggleWorkflows : undefined
      }
    />
  )
  const agentErrorState =
    agentDetailStatus === 'error' ? (
      <AgentDetailErrorState
        error={agentDetailError}
        onClearAgentSelection={onClearAgentSelection}
        onReloadAgentDetail={onReloadAgentDetail}
      />
    ) : null

  return (
    <div
      aria-label="Workflow canvas"
      className={cn(
        'relative flex h-full w-full select-none flex-col overflow-hidden bg-background',
      )}
      role="presentation"
    >
      {showTopLeftHeader ? (
        <div
          aria-label={showWorkflowDefinition ? 'Selected workflow' : 'Selected agent'}
          className={cn(
            'absolute left-2.5 top-2.5 z-10 flex h-[30px] max-w-[max(0px,min(34rem,calc(100%_-_19rem)))] items-center gap-2 rounded-md px-2',
            showWorkflowDefinition ? 'pointer-events-auto' : 'pointer-events-none',
          )}
          onPointerDown={(event) => {
            if (showWorkflowDefinition) event.stopPropagation()
          }}
        >
          {showWorkflowDefinition && workflowHeaderDefinition ? (
            <WorkflowTopLeftHeader
              status={workflowCanvasStatus}
              definition={workflowHeaderDefinition}
              templatePreview={selectedWorkflowIsTemplatePreview}
            />
          ) : (
            <>
              <Bot
                aria-label="Agent"
                role="img"
                className="size-3.5 shrink-0 text-foreground/65"
              />
              <span className="min-w-0 truncate text-[12.5px] font-semibold text-foreground/80">
                {isAuthoring ? authoringDisplayName : selectedAgentHeader?.displayName}
              </span>
              {!isAuthoring && selectedAgentIsSystem ? (
                <Badge
                  variant="secondary"
                  className="h-[18px] rounded px-1.5 py-0 text-[10px] font-semibold text-muted-foreground"
                >
                  system
                </Badge>
              ) : null}
              {isAuthoring && authoringMode ? (
                <Badge
                  variant="outline"
                  className="h-[18px] rounded px-1.5 py-0 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground"
                >
                  {authoringMode === 'edit'
                    ? 'editing'
                    : authoringMode === 'duplicate'
                      ? 'duplicating'
                      : 'new'}
                </Badge>
              ) : null}
              {!isAuthoring &&
              selectedAgentHeader?.shortLabel &&
              selectedAgentHeader.shortLabel !== selectedAgentHeader.displayName ? (
                <span className="min-w-0 truncate text-[11px] font-medium text-muted-foreground/70">
                  {selectedAgentHeader.shortLabel}
                </span>
              ) : null}
              {isAuthoring && authoringShortLabel ? (
                <span className="min-w-0 truncate text-[11px] font-medium text-muted-foreground/70">
                  {authoringShortLabel}
                </span>
              ) : null}
            </>
          )}
        </div>
      ) : null}

      {isAuthoring && authoringSession && onAuthoringSubmit && onAuthoringSaved && onAuthoringCancel ? (
        <AgentVisualization
          active={active}
          projectId={projectId}
          editing
          mode={authoringSession.mode}
          initialDetail={authoringSession.initialDetail}
          authoringCatalog={authoringCatalog}
          toolPackCatalog={toolPackCatalog}
          onSearchAttachableSkills={onSearchAttachableSkills}
          onResolveAttachableSkill={onResolveAttachableSkill}
          onSubmit={onAuthoringSubmit}
          onSaved={onAuthoringSaved}
          onCancel={onAuthoringCancel}
          onEditingStatusChange={handleEditingStatusChange}
          onReadProjectUiState={onReadProjectUiState}
          onWriteProjectUiState={onWriteProjectUiState}
          onSelectedNodeChange={onSelectedNodeChange}
          onPreviewEffectiveRuntime={onPreviewEffectiveRuntime}
        />
      ) : showWorkflowDefinition && selectedWorkflowDefinition ? (
        <WorkflowDefinitionCanvas
          active={active}
          definition={selectedWorkflowDefinition}
          run={selectedWorkflowRun}
          agents={templates}
          initialMode={selectedWorkflowIsDraft ? 'edit' : 'view'}
          isCreating={selectedWorkflowIsDraft}
          saving={workflowActionRunning}
          runningAction={workflowActionRunning}
          onSaveDefinition={selectedWorkflowIsTemplatePreview ? undefined : onSaveWorkflowDefinition}
          onCancelEditing={onCancelWorkflowEditing}
          onCanvasStatusChange={handleWorkflowCanvasStatusChange}
          onStartRun={selectedWorkflowIsTemplatePreview ? undefined : onStartWorkflowDefinitionRun}
          onCancelRun={onCancelWorkflowRun}
          onRetryNodeRun={onRetryWorkflowNodeRun}
          onSkipBranch={onSkipWorkflowBranch}
          onResumeCheckpoint={onResumeWorkflowCheckpoint}
          onExplainRunBlocker={onExplainWorkflowRunBlocker}
          onExportRunBundle={onExportWorkflowRunBundle}
          onResumeNextIncompletePhase={onResumeWorkflowNextIncompletePhase}
          onCreateAgent={onCreateAgent}
          onEditAgent={onEditAgentFromWorkflow}
        />
      ) : (
        <AgentVisualization
          active={active}
          projectId={projectId}
          detail={selectedAgent}
          emptyState={agentErrorState ?? emptyCanvasState}
          emptyStateVisible={!showAgentVisualization && agentDetailStatus !== 'loading'}
          onReadProjectUiState={onReadProjectUiState}
          onWriteProjectUiState={onWriteProjectUiState}
          onSelectedNodeChange={onSelectedNodeChange}
          onPreviewEffectiveRuntime={
            selectedAgent && selectedAgent.ref.kind === 'custom'
              ? onPreviewEffectiveRuntime
              : undefined
          }
        />
      )}

      {showTopLeftHeader ? (
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-x-0 top-0 z-[5] h-20 bg-gradient-to-b from-background to-transparent"
        />
      ) : null}

      {/* Centered authoring diagnostics indicator. Sits inside the same chrome
          strip as the title and the right-aligned buttons so the error UI
          travels with the canvas header instead of taking up canvas real
          estate. Hidden when there's nothing to show. Clicking expands a
          tight popover listing every issue for the user to address. */}
      {canvasDiagnosticsStatus &&
      (canvasDiagnosticsStatus.diagnosticCount > 0 || canvasDiagnosticsStatus.errorMessage) ? (
        <div
          className="pointer-events-auto absolute left-1/2 top-2.5 z-10 -translate-x-1/2"
          onPointerDown={(event) => event.stopPropagation()}
        >
          <AuthoringDiagnosticsBadge
            diagnosticCount={canvasDiagnosticsStatus.diagnosticCount}
            errorMessage={canvasDiagnosticsStatus.errorMessage}
            diagnostics={canvasDiagnosticsStatus.diagnostics}
          />
        </div>
      ) : null}

      {showAgentVisualization || isAuthoring || showWorkflowDefinition ? (
        <div
          className="absolute right-2.5 top-2.5 z-10 flex items-center gap-1.5"
          onPointerDown={(event) => event.stopPropagation()}
        >
          {showWorkflowDefinition && workflowCanvasStatus?.editing ? (
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              onClick={workflowCanvasStatus.save}
              disabled={workflowCanvasStatus.saveDisabled}
              title={selectedWorkflowIsDraft ? 'Save workflow' : 'Save workflow changes'}
              aria-label={selectedWorkflowIsDraft ? 'Save workflow' : 'Save workflow changes'}
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                workflowCanvasStatus.saveDisabled
                  ? 'text-foreground/35 hover:bg-transparent hover:text-foreground/35'
                  : 'text-foreground/70 hover:bg-transparent hover:text-primary',
              )}
            >
              {workflowCanvasStatus.saving ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Save className="size-3.5" />
              )}
            </Button>
          ) : null}
          {showWorkflowDefinition && workflowCanvasStatus?.editing ? (
            <Button
              type="button"
              aria-label={
                selectedWorkflowIsDraft ? 'Cancel workflow creation' : 'Cancel workflow editing'
              }
              onClick={workflowCanvasStatus.cancel}
              size="icon-sm"
              variant="ghost"
              disabled={workflowCanvasStatus.saving}
              title="Cancel"
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                'text-foreground/70 hover:bg-transparent hover:text-foreground',
              )}
            >
              <X className="size-3.5" />
            </Button>
          ) : null}
          {showWorkflowDefinition &&
          workflowCanvasStatus &&
          !workflowCanvasStatus.editing &&
          !selectedWorkflowIsTemplatePreview &&
          onSaveWorkflowDefinition ? (
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              aria-label="Edit workflow"
              title="Edit"
              onClick={workflowCanvasStatus.edit}
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                'text-foreground/70 hover:bg-transparent hover:text-foreground',
              )}
            >
              <GitPullRequestArrow className="size-3.5" />
            </Button>
          ) : null}
          {showWorkflowDefinition &&
          workflowCanvasStatus &&
          !workflowCanvasStatus.editing &&
          !selectedWorkflowIsTemplatePreview &&
          onStartWorkflowDefinitionRun ? (
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              aria-label="Start workflow"
              title="Start"
              disabled={workflowCanvasStatus.runningAction}
              onClick={workflowCanvasStatus.start}
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                'text-foreground/70 hover:bg-transparent hover:text-primary',
              )}
            >
              {workflowCanvasStatus.runningAction ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Play className="size-3.5" />
              )}
            </Button>
          ) : null}
          {showWorkflowDefinition &&
          workflowCanvasStatus?.cancelRun &&
          !workflowCanvasStatus.editing ? (
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              aria-label="Cancel workflow run"
              title="Cancel run"
              disabled={workflowCanvasStatus.runningAction}
              onClick={workflowCanvasStatus.cancelRun}
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                'text-foreground/70 hover:bg-transparent hover:text-destructive',
              )}
            >
              <Square className="size-3.5" />
            </Button>
          ) : null}
          {showWorkflowDefinition && !workflowCanvasStatus?.editing && onClearWorkflowSelection ? (
            <Button
              type="button"
              aria-label="Close workflow"
              onClick={onClearWorkflowSelection}
              size="icon-sm"
              variant="ghost"
              title="Close"
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                'text-foreground/70 hover:bg-transparent hover:text-foreground',
              )}
            >
              <X className="size-3.5" />
            </Button>
          ) : null}
          {isAuthoring && editingStatus ? (
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              onClick={editingStatus.save}
              disabled={editingStatus.saveDisabled}
              title={AUTHORING_SAVE_LABEL_BY_MODE[authoringMode ?? 'create']}
              aria-label={AUTHORING_SAVE_LABEL_BY_MODE[authoringMode ?? 'create']}
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                editingStatus.saveDisabled
                  ? 'text-foreground/35 hover:bg-transparent hover:text-foreground/35'
                  : 'text-foreground/70 hover:bg-transparent hover:text-primary',
              )}
            >
              {editingStatus.saving ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Save className="size-3.5" />
              )}
            </Button>
          ) : null}
          {isAuthoring && onAuthoringCancel ? (
            <Button
              type="button"
              aria-label="Cancel authoring"
              onClick={onAuthoringCancel}
              size="icon-sm"
              variant="ghost"
              disabled={editingStatus?.saving}
              title="Cancel"
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                'text-foreground/70 hover:bg-transparent hover:text-foreground',
              )}
            >
              <X className="size-3.5" />
            </Button>
          ) : null}
          {!isAuthoring && showAgentVisualization && onClearAgentSelection ? (
            <Button
              type="button"
              aria-label="Close agent inspector"
              onClick={onClearAgentSelection}
              size="icon-sm"
              variant="ghost"
              title="Close"
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                'text-foreground/70 hover:bg-transparent hover:text-foreground',
              )}
            >
              <X className="size-3.5" />
            </Button>
          ) : null}
          {onToggleWorkflows ? (
            <Button
              type="button"
              aria-label={workflowsOpen ? 'Close workflows' : 'Open workflows'}
              aria-pressed={workflowsOpen}
              onClick={onToggleWorkflows}
              title="Workflows"
              size="icon-sm"
              variant="ghost"
              className={cn(
                'size-[30px] cursor-pointer rounded-md bg-transparent',
                workflowsOpen
                  ? 'text-primary hover:bg-transparent hover:text-primary'
                  : 'text-foreground/70 hover:bg-transparent hover:text-foreground',
              )}
            >
              <WorkflowIcon className="size-3.5" />
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  )
})

function WorkflowTopLeftHeader({
  status,
  definition,
  templatePreview,
}: {
  status: WorkflowDefinitionCanvasStatus | null
  definition: WorkflowDefinitionDto
  templatePreview: boolean
}) {
  const run = status?.run ?? null
  const editing = status?.editing ?? false
  return (
    <div className="pointer-events-auto flex min-w-0 items-center gap-2">
      <WorkflowIcon
        aria-label="Workflow"
        role="img"
        className="size-3.5 shrink-0 text-foreground/65"
      />
      {editing && status ? (
        <Input
          aria-label="Workflow name"
          value={definition.name}
          onChange={(event) => status.updateName(event.target.value)}
          className="h-7 w-[10rem] border-border/45 bg-card/80 text-[12.5px] font-semibold"
        />
      ) : (
        <span className="min-w-0 truncate text-[12.5px] font-semibold text-foreground/80">
          {definition.name}
        </span>
      )}
      <Badge
        variant="outline"
        className="h-[18px] shrink-0 rounded px-1.5 py-0 text-[10px] font-semibold text-muted-foreground"
      >
        v{definition.version}
      </Badge>
      {templatePreview ? (
        <Badge
          variant="secondary"
          className="h-[18px] shrink-0 rounded px-1.5 py-0 text-[10px] font-semibold text-muted-foreground"
        >
          template
        </Badge>
      ) : null}
      {run ? (
        <Badge
          variant="outline"
          className={cn(
            'h-[18px] shrink-0 rounded px-1.5 py-0 text-[10px] font-semibold',
            workflowRunTone(run.status),
          )}
        >
          {humanizeWorkflowStatus(run.status)}
        </Badge>
      ) : null}
      {editing && status ? (
        <Input
          aria-label="Workflow description"
          value={definition.description}
          onChange={(event) => status.updateDescription(event.target.value)}
          className="h-7 w-[14rem] min-w-0 border-border/45 bg-card/70 text-[11.5px]"
        />
      ) : definition.description ? (
        <span className="min-w-0 truncate text-[11px] font-medium text-muted-foreground/70">
          {definition.description}
        </span>
      ) : null}
    </div>
  )
}

function humanizeWorkflowStatus(value: string): string {
  return value
    .replace(/[_-]+/g, ' ')
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
}

function workflowRunTone(status: WorkflowRunDto['status']): string {
  switch (status) {
    case 'running':
    case 'completed':
      return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
    case 'paused':
      return 'border-amber-500/35 bg-amber-500/10 text-amber-700 dark:text-amber-300'
    case 'failed':
      return 'border-destructive/35 bg-destructive/10 text-destructive'
    case 'cancelled':
      return 'border-muted-foreground/25 bg-muted text-muted-foreground'
    case 'queued':
    default:
      return 'border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300'
  }
}

function AuthoringDiagnosticsBadge({
  diagnosticCount,
  errorMessage,
  diagnostics,
}: {
  diagnosticCount: number
  errorMessage: string | null
  diagnostics: ReadonlyArray<{ path: string; message: string; code?: string }>
}) {
  const tone = errorMessage
    ? 'border-destructive/30 bg-destructive/10 text-destructive'
    : 'border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300'
  const label = errorMessage
    ? 'Save failed'
    : `${diagnosticCount} ${diagnosticCount === 1 ? 'issue' : 'issues'}`
  const showPopover = errorMessage !== null || diagnostics.length > 0

  return (
    <Popover>
      <PopoverTrigger asChild disabled={!showPopover}>
        <button
          type="button"
          className={cn(
            'inline-flex h-[24px] items-center gap-1.5 rounded-md border px-2 text-[11px] font-medium transition-colors',
            tone,
            showPopover ? 'cursor-pointer hover:bg-opacity-20' : 'cursor-default',
          )}
        >
          <AlertCircle className="size-3" />
          {label}
        </button>
      </PopoverTrigger>
      {showPopover ? (
        <PopoverContent
          align="center"
          sideOffset={6}
          className="w-[360px] max-h-[340px] overflow-y-auto p-2"
        >
          {errorMessage ? (
            <p className="px-1 pb-2 text-[12px] font-medium text-destructive">
              {errorMessage}
            </p>
          ) : null}
          {diagnostics.length > 0 ? (
            <ul className="flex flex-col gap-1 text-[11.5px] text-foreground/80">
              {diagnostics.map((diagnostic, index) => (
                <li
                  key={`${diagnostic.code}-${index}`}
                  className="flex flex-col rounded px-1 py-0.5 hover:bg-muted/40"
                >
                  <span className="font-mono text-[10px] text-muted-foreground">
                    {diagnostic.path}
                  </span>
                  <span>{diagnostic.message}</span>
                </li>
              ))}
            </ul>
          ) : null}
        </PopoverContent>
      ) : null}
    </Popover>
  )
}

function PhaseCanvasFallback({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full w-full flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
      {children}
    </div>
  )
}

function AgentDetailErrorState({
  error,
  onClearAgentSelection,
  onReloadAgentDetail,
}: {
  error?: Error | null
  onClearAgentSelection?: () => void
  onReloadAgentDetail?: () => Promise<void>
}) {
  return (
    <PhaseCanvasFallback>
      <div
        className="pointer-events-auto flex flex-col items-center gap-3"
        onPointerDown={(event) => event.stopPropagation()}
      >
        <p className="text-sm font-medium text-destructive">
          Failed to load agent details.
        </p>
        {error ? (
          <p className="text-xs text-muted-foreground">{error.message}</p>
        ) : null}
        <div className="flex gap-2 pt-2">
          {onReloadAgentDetail ? (
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                void onReloadAgentDetail()
              }}
            >
              Retry
            </Button>
          ) : null}
          {onClearAgentSelection ? (
            <Button size="sm" variant="ghost" onClick={onClearAgentSelection}>
              Clear selection
            </Button>
          ) : null}
        </div>
      </div>
    </PhaseCanvasFallback>
  )
}
