import { Activity, ArrowUp, Brain, CheckIcon, ChevronDownIcon, LoaderCircle, Mic, ShieldCheck, Sparkles } from 'lucide-react'
import { Fragment, useMemo, useState, type KeyboardEvent, type RefObject } from 'react'

import type {
  OperatorActionErrorView,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
} from '@/src/features/xero/use-xero-desktop-state/types'
import type {
  ProviderModelThinkingEffortDto,
  RuntimeRunApprovalModeDto,
} from '@/src/lib/xero-model'
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
  SelectTrigger,
  SelectValue,
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

interface ComposerDockProps {
  placeholder: string
  draftPrompt: string
  promptInputRef: RefObject<HTMLTextAreaElement | null>
  promptInputLabel: string
  sendButtonLabel: string
  isPromptDisabled: boolean
  isSendDisabled: boolean
  composerModelId: string | null
  composerModelGroups: ComposerModelGroup[]
  composerThinkingLevel: ProviderModelThinkingEffortDto | null
  composerThinkingOptions: ComposerThinkingOption[]
  composerThinkingPlaceholder: string
  composerApprovalMode: RuntimeRunApprovalModeDto
  composerApprovalOptions: ComposerApprovalOption[]
  autoCompactEnabled: boolean
  controlsDisabled: boolean
  runtimeSessionBindInFlight: boolean
  runtimeRunActionStatus: RuntimeRunActionStatus
  pendingRuntimeRunAction: RuntimeRunActionKind | null
  runtimeRunActionError: OperatorActionErrorView | null
  runtimeRunActionErrorTitle: string
  dictation: ComposerDictationControl
  onOpenDiagnostics?: () => void
  onDraftPromptChange: (value: string) => void
  onSubmitDraftPrompt: () => void
  onAutoCompactEnabledChange: (value: boolean) => void
  onComposerModelChange: (value: string) => void
  onComposerThinkingLevelChange: (value: ProviderModelThinkingEffortDto) => void
  onComposerApprovalModeChange: (value: RuntimeRunApprovalModeDto) => void
}

const composerInlineSelectTriggerClassName =
  'h-7 max-w-full gap-1 rounded-md border-0 bg-transparent px-2 text-[12px] font-medium text-muted-foreground/90 shadow-none transition-colors hover:bg-muted/60 hover:text-foreground focus-visible:border-transparent focus-visible:ring-0 data-[state=open]:bg-muted/60 data-[state=open]:text-foreground dark:bg-transparent dark:hover:bg-muted/60 [&_svg]:size-3 [&_svg]:text-muted-foreground/70'

const composerInlineSelectContentClassName =
  'max-h-72 border-border/70 bg-card/95 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90'

export function ComposerDock({
  placeholder,
  draftPrompt,
  promptInputRef,
  promptInputLabel,
  sendButtonLabel,
  isPromptDisabled,
  isSendDisabled,
  composerModelId,
  composerModelGroups,
  composerThinkingLevel,
  composerThinkingOptions,
  composerThinkingPlaceholder,
  composerApprovalMode,
  composerApprovalOptions,
  autoCompactEnabled,
  controlsDisabled,
  runtimeSessionBindInFlight,
  runtimeRunActionStatus,
  pendingRuntimeRunAction,
  runtimeRunActionError,
  runtimeRunActionErrorTitle,
  dictation,
  onOpenDiagnostics,
  onDraftPromptChange,
  onSubmitDraftPrompt,
  onAutoCompactEnabledChange,
  onComposerModelChange,
  onComposerThinkingLevelChange,
  onComposerApprovalModeChange,
}: ComposerDockProps) {
  const hasComposerModelOptions = composerModelGroups.length > 0
  const hasThinkingOptions = composerThinkingOptions.length > 0
  const isUpdatingControls = runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'update_controls'
  const isStartingRun =
    runtimeSessionBindInFlight || (runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'start')

  function handlePromptKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key !== 'Enter' || event.shiftKey) {
      return
    }

    event.preventDefault()

    if (!isSendDisabled) {
      onSubmitDraftPrompt()
    }
  }

  return (
    <div className="relative shrink-0 px-4 pb-6 pt-10">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 -top-14 h-24 bg-gradient-to-b from-background/0 via-background/86 to-background"
      />
      <div className="relative mx-auto flex w-full max-w-[880px] items-end justify-center gap-3">
        <div className="w-full max-w-[720px]">
          <div className="group/composer relative overflow-hidden rounded-2xl border border-border/60 bg-card/90 shadow-[0_20px_60px_-20px_rgba(0,0,0,0.6),0_2px_8px_-2px_rgba(0,0,0,0.3)] ring-1 ring-inset ring-white/[0.02] backdrop-blur transition-colors supports-[backdrop-filter]:bg-card/75 hover:border-border focus-within:border-primary/40 focus-within:ring-primary/20">
            <Textarea
              aria-label={promptInputLabel}
              className="max-h-56 min-h-[92px] resize-none border-0 bg-transparent px-4 pb-3 pt-3.5 text-[13px] leading-relaxed text-foreground placeholder:text-muted-foreground/50 shadow-none outline-none focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-100"
              disabled={isPromptDisabled}
              onChange={(event) => onDraftPromptChange(event.target.value)}
              onKeyDown={handlePromptKeyDown}
              placeholder={placeholder}
              ref={promptInputRef}
              rows={3}
              value={draftPrompt}
            />
            <div className="border-t border-border/40 bg-background/20 px-2 py-1.5">
              <div className="flex items-center justify-between gap-2">
                <div className="flex min-w-0 items-center gap-0.5 overflow-x-auto pb-0.5">
                  <ModelSelectorCombobox
                    disabled={!hasComposerModelOptions || controlsDisabled}
                    groups={composerModelGroups}
                    value={composerModelId}
                    onChange={onComposerModelChange}
                  />
                  <Select
                    disabled={!hasThinkingOptions || controlsDisabled}
                    value={composerThinkingLevel ?? ''}
                    onValueChange={(value) => onComposerThinkingLevelChange(value as ProviderModelThinkingEffortDto)}
                  >
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <SelectTrigger aria-label="Thinking level selector" className={composerInlineSelectTriggerClassName} size="sm">
                          <Brain aria-hidden="true" />
                          <SelectValue placeholder={composerThinkingPlaceholder} />
                        </SelectTrigger>
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
                  <Select disabled={controlsDisabled} value={composerApprovalMode} onValueChange={(value) => onComposerApprovalModeChange(value as RuntimeRunApprovalModeDto)}>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <SelectTrigger aria-label="Approval mode selector" className={composerInlineSelectTriggerClassName} size="sm">
                          <ShieldCheck aria-hidden="true" />
                          <SelectValue placeholder="Approval unavailable" />
                        </SelectTrigger>
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
                </div>
                <div className="flex items-center gap-1">
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
                  {dictation.isVisible ? (
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
                  ) : null}
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
                </div>
              </div>
              {runtimeRunActionError ? (
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
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </div>
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
        <button
          type="button"
          role="combobox"
          aria-label="Model selector"
          aria-expanded={open}
          aria-haspopup="listbox"
          data-state={open ? 'open' : 'closed'}
          data-placeholder={selectedLabel ? undefined : ''}
          disabled={disabled}
          className={cn(
            composerInlineSelectTriggerClassName,
            'flex w-fit min-w-0 items-center justify-between whitespace-nowrap disabled:cursor-not-allowed disabled:opacity-50',
          )}
        >
          <span className="line-clamp-1 truncate">{selectedLabel ?? 'Model not configured'}</span>
          <ChevronDownIcon aria-hidden="true" className="opacity-50" />
        </button>
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
