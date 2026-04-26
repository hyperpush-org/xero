import { Activity, ArrowUp, LoaderCircle } from 'lucide-react'
import type { KeyboardEvent } from 'react'

import type {
  OperatorActionErrorView,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
} from '@/src/features/cadence/use-cadence-desktop-state/types'
import type {
  ProviderModelThinkingEffortDto,
  RuntimeRunApprovalModeDto,
} from '@/src/lib/cadence-model'
import { Button } from '@/components/ui/button'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'

import type {
  ComposerApprovalOption,
  ComposerModelGroup,
  ComposerThinkingOption,
} from './composer-helpers'

interface ComposerDockProps {
  placeholder: string
  draftPrompt: string
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
              rows={3}
              value={draftPrompt}
            />
            <div className="border-t border-border/40 bg-background/20 px-2 py-1.5">
              <div className="flex items-center justify-between gap-2">
                <div className="flex min-w-0 items-center gap-0.5 overflow-x-auto pb-0.5">
                  <Select disabled={!hasComposerModelOptions || controlsDisabled} value={composerModelId ?? ''} onValueChange={onComposerModelChange}>
                    <SelectTrigger aria-label="Model selector" className={composerInlineSelectTriggerClassName} size="sm">
                      <SelectValue placeholder="Model not configured" />
                    </SelectTrigger>
                    <SelectContent className={composerInlineSelectContentClassName}>
                      {composerModelGroups.map((group, index) => (
                        <div key={group.id}>
                          {index > 0 ? <SelectSeparator /> : null}
                          <SelectGroup>
                            <SelectLabel>{group.label}</SelectLabel>
                            {group.items.map((model) => (
                              <SelectItem key={model.value} value={model.value}>
                                {model.label}
                              </SelectItem>
                            ))}
                          </SelectGroup>
                        </div>
                      ))}
                    </SelectContent>
                  </Select>
                  <span aria-hidden="true" className="h-3.5 w-px bg-border/60" />
                  <Select
                    disabled={!hasThinkingOptions || controlsDisabled}
                    value={composerThinkingLevel ?? ''}
                    onValueChange={(value) => onComposerThinkingLevelChange(value as ProviderModelThinkingEffortDto)}
                  >
                    <SelectTrigger aria-label="Thinking level selector" className={composerInlineSelectTriggerClassName} size="sm">
                      <SelectValue placeholder={composerThinkingPlaceholder} />
                    </SelectTrigger>
                    <SelectContent className={composerInlineSelectContentClassName}>
                      {composerThinkingOptions.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {option.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <span aria-hidden="true" className="h-3.5 w-px bg-border/60" />
                  <Select disabled={controlsDisabled} value={composerApprovalMode} onValueChange={(value) => onComposerApprovalModeChange(value as RuntimeRunApprovalModeDto)}>
                    <SelectTrigger aria-label="Approval mode selector" className={composerInlineSelectTriggerClassName} size="sm">
                      <SelectValue placeholder="Approval unavailable" />
                    </SelectTrigger>
                    <SelectContent className={composerInlineSelectContentClassName}>
                      {composerApprovalOptions.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {option.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <span aria-hidden="true" className="h-3.5 w-px bg-border/60" />
                  <label className="flex h-7 shrink-0 items-center gap-2 rounded-md px-2 text-[12px] font-medium text-muted-foreground/90 transition-colors hover:bg-muted/60 hover:text-foreground">
                    <Switch
                      aria-label="Auto-compact before sending"
                      checked={autoCompactEnabled}
                      disabled={runtimeRunActionStatus === 'running'}
                      onCheckedChange={onAutoCompactEnabledChange}
                    />
                    <span>Auto-compact</span>
                  </label>
                </div>
                <div className="flex items-center gap-2">
                  <span
                    aria-hidden="true"
                    className="hidden items-center gap-1 text-[11px] font-medium text-muted-foreground/50 sm:inline-flex"
                  >
                    <kbd className="rounded border border-border/60 bg-muted/40 px-1.5 py-0.5 font-sans text-[10px] leading-none text-muted-foreground/70">
                      ⏎
                    </kbd>
                    <span>to send</span>
                  </span>
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
