import { Activity, AlertTriangle, ArrowUp, Brain, Bug, CheckIcon, ChevronDownIcon, Cpu, FileText, LoaderCircle, MessageCircle, Mic, Paperclip, Settings, ShieldCheck, Sparkles, Users, Wrench, X } from 'lucide-react'
import * as SelectPrimitive from '@radix-ui/react-select'
import { forwardRef, Fragment, useMemo, useState, type ComponentPropsWithoutRef, type KeyboardEvent, type ReactNode, type RefObject } from 'react'

import type {
  OperatorActionErrorView,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
} from '@/src/features/xero/use-xero-desktop-state/types'
import {
  getRuntimeAgentDescriptor,
  RUNTIME_AGENT_DESCRIPTORS,
  type AgentDefinitionSummaryDto,
  type ProviderModelThinkingEffortDto,
  type RuntimeAgentIdDto,
  type RuntimeRunApprovalModeDto,
} from '@/src/lib/xero-model'

import {
  buildComposerAgentSelectionKey,
  runtimeAgentIdForCustomBaseCapability,
} from './composer-helpers'
import { Button } from '@/components/ui/button'
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from '@/components/ui/command'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import {
  Select,
  SelectContent,
  SelectItem,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'

import type {
  ComposerApprovalOption,
  ComposerModelGroup,
  ComposerThinkingOption,
} from './composer-helpers'
import type { SpeechDictationPhase } from './use-speech-dictation'

interface ComposerDictationControl {
  isVisible: boolean
  phase: SpeechDictationPhase
  isListening: boolean
  isToggleDisabled: boolean
  ariaLabel: string
  tooltip: string
  toggle: () => Promise<void>
}

export type ComposerAttachmentKind = 'image' | 'document' | 'text'

export interface ComposerPendingAttachment {
  id: string
  kind: ComposerAttachmentKind
  originalName: string
  mediaType: string
  sizeBytes: number
  /** When still uploading bytes to the backend, set to 'staging'. */
  status: 'staging' | 'ready' | 'error'
  /** Optional local preview URL for images (object URL while staging). */
  previewUrl?: string
  /** Populated once staging completes — the absolute path on disk. */
  absolutePath?: string
  errorMessage?: string
}

interface ComposerDockProps {
  density?: 'comfortable' | 'compact'
  placeholder: string
  draftPrompt: string
  promptInputRef: RefObject<HTMLTextAreaElement | null>
  promptInputLabel: string
  sendButtonLabel: string
  isPromptDisabled: boolean
  isSendDisabled: boolean
  composerRuntimeAgentId: RuntimeAgentIdDto
  composerRuntimeAgentLabel: string
  composerAgentDefinitionId?: string | null
  composerAgentSelectionKey?: string
  customAgentDefinitions?: readonly AgentDefinitionSummaryDto[]
  composerModelId: string | null
  composerModelGroups: ComposerModelGroup[]
  composerThinkingLevel: ProviderModelThinkingEffortDto | null
  composerThinkingOptions: ComposerThinkingOption[]
  composerThinkingPlaceholder: string
  composerApprovalMode: RuntimeRunApprovalModeDto
  composerApprovalOptions: ComposerApprovalOption[]
  autoCompactEnabled: boolean
  controlsDisabled: boolean
  runtimeAgentSwitchDisabled: boolean
  runtimeSessionBindInFlight: boolean
  runtimeRunActionStatus: RuntimeRunActionStatus
  pendingRuntimeRunAction: RuntimeRunActionKind | null
  runtimeRunActionError: OperatorActionErrorView | null
  runtimeRunActionErrorTitle: string
  dictation: ComposerDictationControl
  contextMeter?: ReactNode
  pendingAttachments?: ComposerPendingAttachment[]
  onRemoveAttachment?: (id: string) => void
  onOpenDiagnostics?: () => void
  onDraftPromptChange: (value: string) => void
  onSubmitDraftPrompt: () => void
  onAutoCompactEnabledChange: (value: boolean) => void
  onComposerRuntimeAgentChange: (value: RuntimeAgentIdDto) => void
  onComposerAgentSelectionChange?: (selectionKey: string) => void
  onComposerModelChange: (value: string) => void
  onComposerThinkingLevelChange: (value: ProviderModelThinkingEffortDto) => void
  onComposerApprovalModeChange: (value: RuntimeRunApprovalModeDto) => void
}

const composerInlineTriggerClassName =
  'flex h-7 w-fit min-w-0 items-center gap-1 rounded-md border-0 bg-transparent px-2 text-[12px] font-medium text-muted-foreground/90 whitespace-nowrap shadow-none transition-colors outline-none hover:bg-muted/60 hover:text-foreground focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-50 data-[state=open]:bg-muted/60 data-[state=open]:text-foreground dark:bg-transparent dark:hover:bg-muted/60 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg]:text-muted-foreground/70'

const composerInlineSelectContentClassName =
  'max-h-72 border-border/70 bg-card/95 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90'

interface ComposerInlineTriggerProps extends ComponentPropsWithoutRef<'button'> {
  icon: ReactNode
  label: ReactNode
}

const ComposerInlineTrigger = forwardRef<HTMLButtonElement, ComposerInlineTriggerProps>(
  function ComposerInlineTrigger({ icon, label, className, ...props }, ref) {
    return (
      <button
        ref={ref}
        type="button"
        className={cn(composerInlineTriggerClassName, className)}
        {...props}
      >
        {icon}
        <span className="line-clamp-1 truncate">{label}</span>
        <ChevronDownIcon aria-hidden="true" className="size-4 opacity-50" />
      </button>
    )
  },
)

export function ComposerDock({
  density = 'comfortable',
  placeholder,
  draftPrompt,
  promptInputRef,
  promptInputLabel,
  sendButtonLabel,
  isPromptDisabled,
  isSendDisabled,
  composerRuntimeAgentId,
  composerRuntimeAgentLabel,
  composerAgentDefinitionId = null,
  composerAgentSelectionKey,
  customAgentDefinitions = [],
  composerModelId,
  composerModelGroups,
  composerThinkingLevel,
  composerThinkingOptions,
  composerThinkingPlaceholder,
  composerApprovalMode,
  composerApprovalOptions,
  autoCompactEnabled,
  controlsDisabled,
  runtimeAgentSwitchDisabled,
  runtimeSessionBindInFlight,
  runtimeRunActionStatus,
  pendingRuntimeRunAction,
  runtimeRunActionError,
  runtimeRunActionErrorTitle,
  dictation,
  contextMeter,
  pendingAttachments,
  onRemoveAttachment,
  onOpenDiagnostics,
  onDraftPromptChange,
  onSubmitDraftPrompt,
  onAutoCompactEnabledChange,
  onComposerRuntimeAgentChange,
  onComposerAgentSelectionChange,
  onComposerModelChange,
  onComposerThinkingLevelChange,
  onComposerApprovalModeChange,
}: ComposerDockProps) {
  const attachmentsRow =
    pendingAttachments && pendingAttachments.length > 0 ? (
      <ComposerAttachmentChips
        attachments={pendingAttachments}
        onRemove={onRemoveAttachment}
      />
    ) : null
  const hasComposerModelOptions = composerModelGroups.length > 0
  const hasThinkingOptions = composerThinkingOptions.length > 0
  const composerRuntimeAgentDescriptor = getRuntimeAgentDescriptor(composerRuntimeAgentId)
  const showApprovalSelector = composerRuntimeAgentDescriptor.allowedApprovalModes.length > 1
  const isAgentSelectorDisabled = runtimeAgentSwitchDisabled || controlsDisabled
  const isUpdatingControls = runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'update_controls'
  const isStartingRun =
    runtimeSessionBindInFlight || (runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'start')
  const thinkingTriggerLabel =
    composerThinkingOptions.find((option) => option.value === composerThinkingLevel)?.label ?? composerThinkingPlaceholder
  const approvalTriggerLabel =
    composerApprovalOptions.find((option) => option.value === composerApprovalMode)?.label ?? 'Approval unavailable'
  const activeCustomAgent = useMemo(() => {
    if (!composerAgentDefinitionId) return null
    return customAgentDefinitions.find((agent) => agent.definitionId === composerAgentDefinitionId) ?? null
  }, [composerAgentDefinitionId, customAgentDefinitions])
  const isCustomAgent = Boolean(activeCustomAgent)
  const agentTriggerLabel = activeCustomAgent?.displayName ?? composerRuntimeAgentLabel
  const agentSelectorValue =
    composerAgentSelectionKey ??
    buildComposerAgentSelectionKey(composerRuntimeAgentId, composerAgentDefinitionId)
  const visibleCustomAgents = useMemo(
    () =>
      customAgentDefinitions.filter(
        (agent) => agent.lifecycleState === 'active' || agent.definitionId === composerAgentDefinitionId,
      ),
    [composerAgentDefinitionId, customAgentDefinitions],
  )
  const projectCustomAgents = useMemo(
    () => visibleCustomAgents.filter((agent) => agent.scope === 'project_custom'),
    [visibleCustomAgents],
  )
  const globalCustomAgents = useMemo(
    () => visibleCustomAgents.filter((agent) => agent.scope === 'global_custom'),
    [visibleCustomAgents],
  )
  const AgentTriggerIcon = isCustomAgent
    ? Users
    : composerRuntimeAgentId === 'ask'
      ? MessageCircle
      : composerRuntimeAgentId === 'debug'
        ? Bug
        : composerRuntimeAgentId === 'agent_create'
          ? Sparkles
          : Wrench

  function handlePromptKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key !== 'Enter' || event.shiftKey) {
      return
    }

    event.preventDefault()

    if (!isSendDisabled) {
      onSubmitDraftPrompt()
    }
  }

  const isCompact = density === 'compact'

  const agentSelector = (
    <Select
      disabled={isAgentSelectorDisabled}
      value={agentSelectorValue}
      onValueChange={(value) => {
        if (onComposerAgentSelectionChange) {
          onComposerAgentSelectionChange(value)
          return
        }
        if (value.startsWith('builtin:')) {
          const builtinId = value.slice('builtin:'.length) as RuntimeAgentIdDto
          if (RUNTIME_AGENT_DESCRIPTORS.some((agent) => agent.id === builtinId)) {
            onComposerRuntimeAgentChange(builtinId)
          }
        }
      }}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <SelectPrimitive.Trigger asChild>
            <ComposerInlineTrigger
              aria-label="Agent selector"
              icon={<AgentTriggerIcon aria-hidden="true" className="size-3" />}
              label={agentTriggerLabel}
            />
          </SelectPrimitive.Trigger>
        </TooltipTrigger>
        <TooltipContent side="top">
          {runtimeAgentSwitchDisabled
            ? 'Selected agent is fixed for the current run.'
            : isCustomAgent
              ? `${agentTriggerLabel} (${activeCustomAgent?.scope === 'project_custom' ? 'project' : 'global'} custom agent)`
              : `${agentTriggerLabel} agent`}
        </TooltipContent>
      </Tooltip>
      <SelectContent className={composerInlineSelectContentClassName}>
        {RUNTIME_AGENT_DESCRIPTORS.map((agent) => (
          <SelectItem
            key={agent.id}
            value={buildComposerAgentSelectionKey(agent.id, null)}
          >
            {agent.label}
          </SelectItem>
        ))}
        {projectCustomAgents.length > 0 ? (
          <>
            <SelectPrimitive.Separator className="my-1 h-px bg-border/60" />
            <SelectPrimitive.Label className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/70">
              Project agents
            </SelectPrimitive.Label>
            {projectCustomAgents.map((agent) => (
              <SelectItem
                key={`project-${agent.definitionId}`}
                value={buildComposerAgentSelectionKey(
                  runtimeAgentIdForCustomBaseCapability(agent.baseCapabilityProfile),
                  agent.definitionId,
                )}
              >
                <span className="flex items-center gap-1.5">
                  <Users aria-hidden="true" className="size-3 text-primary" />
                  {agent.displayName}
                  {agent.lifecycleState !== 'active' ? (
                    <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
                      · {agent.lifecycleState}
                    </span>
                  ) : null}
                </span>
              </SelectItem>
            ))}
          </>
        ) : null}
        {globalCustomAgents.length > 0 ? (
          <>
            <SelectPrimitive.Separator className="my-1 h-px bg-border/60" />
            <SelectPrimitive.Label className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/70">
              Global agents
            </SelectPrimitive.Label>
            {globalCustomAgents.map((agent) => (
              <SelectItem
                key={`global-${agent.definitionId}`}
                value={buildComposerAgentSelectionKey(
                  runtimeAgentIdForCustomBaseCapability(agent.baseCapabilityProfile),
                  agent.definitionId,
                )}
              >
                <span className="flex items-center gap-1.5">
                  <Users aria-hidden="true" className="size-3 text-muted-foreground" />
                  {agent.displayName}
                  {agent.lifecycleState !== 'active' ? (
                    <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
                      · {agent.lifecycleState}
                    </span>
                  ) : null}
                </span>
              </SelectItem>
            ))}
          </>
        ) : null}
      </SelectContent>
    </Select>
  )

  const modelSelector = (
    <ModelSelectorCombobox
      disabled={!hasComposerModelOptions || controlsDisabled}
      groups={composerModelGroups}
      value={composerModelId}
      onChange={onComposerModelChange}
    />
  )

  const thinkingSelector = (
    <Select
      disabled={!hasThinkingOptions || controlsDisabled}
      value={composerThinkingLevel ?? ''}
      onValueChange={(value) => onComposerThinkingLevelChange(value as ProviderModelThinkingEffortDto)}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <SelectPrimitive.Trigger asChild>
            <ComposerInlineTrigger
              aria-label="Thinking level selector"
              icon={<Brain aria-hidden="true" className="size-3" />}
              label={thinkingTriggerLabel}
            />
          </SelectPrimitive.Trigger>
        </TooltipTrigger>
        <TooltipContent side="top">Thinking effort</TooltipContent>
      </Tooltip>
      <SelectContent className={composerInlineSelectContentClassName}>
        {composerThinkingOptions.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )

  const approvalSelector = showApprovalSelector ? (
    <Select disabled={controlsDisabled} value={composerApprovalMode} onValueChange={(value) => onComposerApprovalModeChange(value as RuntimeRunApprovalModeDto)}>
      <Tooltip>
        <TooltipTrigger asChild>
          <SelectPrimitive.Trigger asChild>
            <ComposerInlineTrigger
              aria-label="Approval mode selector"
              icon={<ShieldCheck aria-hidden="true" className="size-3" />}
              label={approvalTriggerLabel}
            />
          </SelectPrimitive.Trigger>
        </TooltipTrigger>
        <TooltipContent side="top">Approval mode</TooltipContent>
      </Tooltip>
      <SelectContent className={composerInlineSelectContentClassName}>
        {composerApprovalOptions.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  ) : null

  const autoCompactToggle = (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          aria-label="Auto-compact before sending"
          aria-pressed={autoCompactEnabled}
          className={cn(
            'h-8 w-8 rounded-md px-0 text-muted-foreground/70 hover:text-foreground',
            autoCompactEnabled ? 'bg-primary/10 text-primary hover:bg-primary/15 hover:text-primary' : null,
          )}
          disabled={runtimeRunActionStatus === 'running'}
          onClick={() => onAutoCompactEnabledChange(!autoCompactEnabled)}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <Sparkles className="h-3.5 w-3.5" strokeWidth={2.5} />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top">
        Auto-compact before sending {autoCompactEnabled ? '· on' : '· off'}
      </TooltipContent>
    </Tooltip>
  )

  const dictationToggle = dictation.isVisible ? (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          aria-label={dictation.ariaLabel}
          aria-pressed={dictation.isListening}
          className={cn(
            'relative h-8 w-8 rounded-md px-0',
            dictation.isListening
              ? 'border-destructive/35 bg-destructive/10 text-destructive hover:bg-destructive/15'
              : null,
          )}
          disabled={dictation.isToggleDisabled}
          onClick={() => void dictation.toggle()}
          size="icon-sm"
          type="button"
          variant={dictation.isListening ? 'outline' : 'ghost'}
        >
          {dictation.phase === 'requesting' || dictation.phase === 'stopping' ? (
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" strokeWidth={2.5} />
          ) : (
            <Mic
              className={cn('h-3.5 w-3.5', dictation.isListening ? 'animate-pulse' : null)}
              strokeWidth={2.5}
            />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top">{dictation.tooltip}</TooltipContent>
    </Tooltip>
  ) : null

  const sendButton = (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          aria-label={sendButtonLabel}
          className="h-8 w-8 rounded-md px-0"
          disabled={isSendDisabled}
          onClick={onSubmitDraftPrompt}
          size="icon-sm"
          type="button"
          variant="secondary"
        >
          {isUpdatingControls || isStartingRun ? (
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" strokeWidth={2.5} />
          ) : (
            <ArrowUp className="h-3.5 w-3.5" strokeWidth={2.5} />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top" className="flex items-center gap-1.5">
        <span>{sendButtonLabel}</span>
        <kbd className="rounded border border-background/30 bg-background/10 px-1 py-0.5 font-sans text-[10px] leading-none">
          ⏎
        </kbd>
      </TooltipContent>
    </Tooltip>
  )

  const errorRow = runtimeRunActionError ? (
    <div
      className="border-t border-destructive/25 bg-destructive/5 px-3 py-2 text-[10px] leading-relaxed text-destructive/90"
      role="alert"
    >
      <div className="flex items-start justify-between gap-2">
        <p className="font-medium">{runtimeRunActionErrorTitle}</p>
        {onOpenDiagnostics ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-6 shrink-0 gap-1 px-1.5 text-[10.5px] text-destructive hover:bg-destructive/10 hover:text-destructive"
            onClick={onOpenDiagnostics}
          >
            <Activity className="h-3 w-3" />
            Diagnostics
          </Button>
        ) : null}
      </div>
      <p>{runtimeRunActionError.message}</p>
      {runtimeRunActionError.code ? <p className="font-mono text-[10px]">code: {runtimeRunActionError.code}</p> : null}
    </div>
  ) : null

  if (isCompact) {
    return (
      <div className="relative shrink-0">
        <div className="border-t border-border/60 bg-card/95 supports-[backdrop-filter]:bg-card/80">
          {attachmentsRow ? <div className="px-2 pt-1.5">{attachmentsRow}</div> : null}
          <div className="flex items-end gap-1.5 px-2 py-1.5">
            <div className="flex min-w-0 flex-1 items-end">
              <Textarea
                aria-label={promptInputLabel}
                className="min-h-[28px] max-h-[120px] flex-1 resize-none overflow-y-auto border-0 bg-transparent dark:bg-transparent px-2 py-1 text-[12.5px] leading-relaxed text-foreground placeholder:text-muted-foreground/50 shadow-none outline-none focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-100"
                disabled={isPromptDisabled}
                onChange={(event) => onDraftPromptChange(event.target.value)}
                onKeyDown={handlePromptKeyDown}
                placeholder={placeholder}
                ref={promptInputRef}
                rows={1}
                value={draftPrompt}
              />
            </div>
            <div className="flex shrink-0 items-center gap-0.5">
              {modelSelector}
              <CompactGearPopover>
                <div className="space-y-2 p-2">
                  <div className="flex flex-col gap-1">
                    <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                      Agent
                    </span>
                    <div className="flex flex-wrap items-center gap-1">{agentSelector}</div>
                  </div>
                  <div className="flex flex-col gap-1">
                    <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                      Thinking
                    </span>
                    <div className="flex flex-wrap items-center gap-1">{thinkingSelector}</div>
                  </div>
                  {approvalSelector ? (
                    <div className="flex flex-col gap-1">
                      <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                        Approval
                      </span>
                      <div className="flex flex-wrap items-center gap-1">{approvalSelector}</div>
                    </div>
                  ) : null}
                  <div className="flex flex-col gap-1">
                    <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                      Composer
                    </span>
                    <div className="flex flex-wrap items-center gap-1">
                      {autoCompactToggle}
                      {dictationToggle}
                    </div>
                  </div>
                  {contextMeter ? (
                    <div className="flex flex-col gap-1 border-t border-border/60 pt-2">
                      {contextMeter}
                    </div>
                  ) : null}
                </div>
              </CompactGearPopover>
              {sendButton}
            </div>
          </div>
          {errorRow}
        </div>
      </div>
    )
  }

  return (
    <div className="relative shrink-0 px-4 pb-3 pt-0">
      <div className="relative mx-auto flex w-full max-w-[720px] items-end justify-center gap-3">
        <div className="w-full max-w-[720px]">
          <div className="group/composer relative overflow-hidden rounded-2xl border border-border/60 bg-card/90 shadow-[0_8px_24px_-12px_rgba(15,23,42,0.12),0_1px_3px_-1px_rgba(15,23,42,0.06)] ring-1 ring-inset ring-foreground/[0.03] backdrop-blur transition-colors supports-[backdrop-filter]:bg-card/75 hover:border-border focus-within:border-primary/40 focus-within:ring-primary/20 dark:shadow-[0_20px_60px_-20px_rgba(0,0,0,0.6),0_2px_8px_-2px_rgba(0,0,0,0.3)]">
            {attachmentsRow ? <div className="px-3 pt-2">{attachmentsRow}</div> : null}
            <div className="pb-1.5 pt-2.5">
              <Textarea
                aria-label={promptInputLabel}
                className="min-h-[24px] max-h-[68px] resize-none overflow-y-auto border-0 bg-transparent dark:bg-transparent px-3 py-0 text-[13px] leading-relaxed text-foreground placeholder:text-muted-foreground/50 shadow-none outline-none focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-100 md:text-[13px]"
                disabled={isPromptDisabled}
                onChange={(event) => onDraftPromptChange(event.target.value)}
                onKeyDown={handlePromptKeyDown}
                placeholder={placeholder}
                ref={promptInputRef}
                rows={1}
                value={draftPrompt}
              />
            </div>
            <div className="border-t border-border/40 px-2 py-1.5">
              <div className="flex items-center justify-between gap-2">
                <div className="flex min-w-0 items-center gap-0.5 overflow-x-auto pb-0.5">
                  {agentSelector}
                  {modelSelector}
                  {thinkingSelector}
                  {approvalSelector}
                </div>
                <div className="flex items-center gap-1">
                  {contextMeter ? <div className="shrink-0">{contextMeter}</div> : null}
                  {autoCompactToggle}
                  {dictationToggle}
                  {sendButton}
                </div>
              </div>
              {errorRow}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

interface CompactGearPopoverProps {
  children: ReactNode
}

function CompactGearPopover({ children }: CompactGearPopoverProps) {
  const [open, setOpen] = useState(false)

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          aria-label="Composer settings"
          className="h-8 w-8 rounded-md px-0 text-muted-foreground/80 hover:text-foreground"
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <Settings className="h-3.5 w-3.5" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        side="top"
        className="w-72 max-w-[90vw] border-border/70 bg-card/95 p-0 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90"
      >
        <div className="flex items-center justify-between border-b border-border/40 px-2 py-1.5">
          <span className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            Composer settings
          </span>
          <Button
            aria-label="Close composer settings"
            className="h-6 w-6 rounded-md text-muted-foreground hover:text-foreground"
            onClick={() => setOpen(false)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <X className="h-3 w-3" />
          </Button>
        </div>
        {children}
      </PopoverContent>
    </Popover>
  )
}

interface ModelSelectorComboboxProps {
  disabled: boolean
  groups: ComposerModelGroup[]
  value: string | null
  onChange: (value: string) => void
}

function ModelSelectorCombobox({ disabled, groups, value, onChange }: ModelSelectorComboboxProps) {
  const [open, setOpen] = useState(false)
  const selectedLabel = useMemo(() => {
    for (const group of groups) {
      const match = group.items.find((item) => item.value === value)
      if (match) return match.label
    }
    return null
  }, [groups, value])

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <ComposerInlineTrigger
          role="combobox"
          aria-label="Model selector"
          aria-expanded={open}
          aria-haspopup="listbox"
          disabled={disabled}
          icon={<Cpu aria-hidden="true" className="size-3" />}
          label={selectedLabel ?? 'Model not configured'}
        />
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className={cn('w-72 p-0', composerInlineSelectContentClassName)}
      >
        <Command>
          <CommandInput placeholder="Search models..." />
          <CommandList>
            <CommandEmpty>No models found.</CommandEmpty>
            {groups.map((group, index) => (
              <Fragment key={group.id}>
                {index > 0 ? <CommandSeparator /> : null}
                <CommandGroup heading={group.label}>
                  {group.items.map((item) => (
                    <CommandItem
                      key={item.value}
                      value={`${group.label} ${item.label}`}
                      onSelect={() => {
                        onChange(item.value)
                        setOpen(false)
                      }}
                    >
                      <span className="line-clamp-1 truncate">{item.label}</span>
                      {value === item.value ? (
                        <CheckIcon aria-hidden="true" className="ml-auto size-3.5" />
                      ) : null}
                    </CommandItem>
                  ))}
                </CommandGroup>
              </Fragment>
            ))}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

interface ComposerAttachmentChipsProps {
  attachments: ComposerPendingAttachment[]
  onRemove?: (id: string) => void
}

function ComposerAttachmentChips({ attachments, onRemove }: ComposerAttachmentChipsProps) {
  return (
    <div className="flex flex-wrap items-center gap-1.5" role="list" aria-label="Pending attachments">
      {attachments.map((attachment) => (
        <ComposerAttachmentChip
          key={attachment.id}
          attachment={attachment}
          onRemove={onRemove}
        />
      ))}
    </div>
  )
}

interface ComposerAttachmentChipProps {
  attachment: ComposerPendingAttachment
  onRemove?: (id: string) => void
}

function ComposerAttachmentChip({ attachment, onRemove }: ComposerAttachmentChipProps) {
  const isImage = attachment.kind === 'image'
  const isStaging = attachment.status === 'staging'
  const isError = attachment.status === 'error'
  const previewUrl = attachment.previewUrl
  const truncatedName =
    attachment.originalName.length > 24
      ? `${attachment.originalName.slice(0, 21)}…`
      : attachment.originalName
  return (
    <div
      role="listitem"
      className={cn(
        'group relative flex max-w-[220px] items-center gap-2 rounded-md border border-border/60 bg-muted/40 py-1 pl-1 pr-1.5 text-[11px] text-foreground shadow-sm',
        isError ? 'border-destructive/60 bg-destructive/10' : null,
      )}
      data-attachment-id={attachment.id}
      data-attachment-status={attachment.status}
    >
      <div className="flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-sm bg-background">
        {isImage && previewUrl ? (
          <img
            src={previewUrl}
            alt=""
            className="h-full w-full object-cover"
            draggable={false}
          />
        ) : isError ? (
          <AlertTriangle className="h-4 w-4 text-destructive" aria-hidden="true" />
        ) : isImage ? (
          <Paperclip className="h-3.5 w-3.5 text-muted-foreground" aria-hidden="true" />
        ) : (
          <FileText className="h-3.5 w-3.5 text-muted-foreground" aria-hidden="true" />
        )}
        {isStaging ? (
          <span
            className="absolute inset-0 flex items-center justify-center bg-background/60"
            aria-hidden="true"
          >
            <LoaderCircle className="h-4 w-4 animate-spin text-muted-foreground" />
          </span>
        ) : null}
      </div>
      <div className="flex min-w-0 flex-col">
        <span className="line-clamp-1 truncate font-medium" title={attachment.originalName}>
          {truncatedName}
        </span>
        <span className="text-[10px] text-muted-foreground">
          {isError ? attachment.errorMessage ?? 'Upload failed' : formatChipBytes(attachment.sizeBytes)}
        </span>
      </div>
      {onRemove ? (
        <button
          type="button"
          aria-label={`Remove ${attachment.originalName}`}
          className="ml-1 inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          onClick={() => onRemove(attachment.id)}
        >
          <X className="h-3 w-3" aria-hidden="true" />
        </button>
      ) : null}
    </div>
  )
}

function formatChipBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
