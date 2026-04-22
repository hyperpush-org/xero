import { ArrowUp, LoaderCircle, Play } from 'lucide-react'

import type { RuntimeRunActionStatus } from '@/src/features/cadence/use-cadence-desktop-state'
import type { ProviderModelThinkingEffortDto } from '@/src/lib/cadence-model'
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
import { Textarea } from '@/components/ui/textarea'

import type { ComposerModelGroup, ComposerThinkingOption } from './composer-helpers'

interface ComposerDockProps {
  placeholder: string
  composerModelId: string | null
  composerModelGroups: ComposerModelGroup[]
  composerThinkingLevel: ProviderModelThinkingEffortDto | null
  composerThinkingOptions: ComposerThinkingOption[]
  composerThinkingPlaceholder: string
  catalogStatusLabel: string
  catalogStatusDetail: string
  thinkingStatusDetail: string
  onComposerModelChange: (value: string) => void
  onComposerThinkingLevelChange: (value: ProviderModelThinkingEffortDto) => void
  showStartRunButton: boolean
  runtimeRunActionStatus: RuntimeRunActionStatus
  onStartRuntimeRun?: () => void
}

const composerInlineSelectTriggerClassName =
  'h-7 max-w-full gap-1 rounded-md border-0 bg-transparent px-2 text-[12px] font-medium text-muted-foreground/90 shadow-none transition-colors hover:bg-muted/60 hover:text-foreground focus-visible:border-transparent focus-visible:ring-0 data-[state=open]:bg-muted/60 data-[state=open]:text-foreground dark:bg-transparent dark:hover:bg-muted/60 [&_svg]:size-3 [&_svg]:text-muted-foreground/70'

const composerInlineSelectContentClassName =
  'max-h-72 border-border/70 bg-card/95 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90'

export function ComposerDock({
  placeholder,
  composerModelId,
  composerModelGroups,
  composerThinkingLevel,
  composerThinkingOptions,
  composerThinkingPlaceholder,
  catalogStatusLabel,
  catalogStatusDetail,
  thinkingStatusDetail,
  onComposerModelChange,
  onComposerThinkingLevelChange,
  showStartRunButton,
  runtimeRunActionStatus,
  onStartRuntimeRun,
}: ComposerDockProps) {
  const hasComposerModelOptions = composerModelGroups.length > 0
  const hasThinkingOptions = composerThinkingOptions.length > 0

  return (
    <div className="relative shrink-0 px-4 pb-6 pt-10">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 -top-14 h-24 bg-gradient-to-b from-background/0 via-background/86 to-background"
      />
      <div className="relative mx-auto flex w-full max-w-[880px] items-end justify-center gap-3">
        <div className="w-full max-w-[680px]">
          <div className="group/composer relative overflow-hidden rounded-2xl border border-border/60 bg-card/90 shadow-[0_20px_60px_-20px_rgba(0,0,0,0.6),0_2px_8px_-2px_rgba(0,0,0,0.3)] ring-1 ring-inset ring-white/[0.02] backdrop-blur transition-colors supports-[backdrop-filter]:bg-card/75 hover:border-border focus-within:border-primary/40 focus-within:ring-primary/20">
            <Textarea
              aria-label="Agent input unavailable"
              className="max-h-56 min-h-[92px] resize-none border-0 bg-transparent px-4 pb-3 pt-3.5 text-[13px] leading-relaxed text-foreground placeholder:text-muted-foreground/50 shadow-none outline-none focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-100"
              disabled
              placeholder={placeholder}
              rows={3}
              value=""
            />
            <div className="border-t border-border/40 bg-background/20 px-2 py-1.5">
              <div className="flex items-center justify-between gap-2">
                <div className="flex min-w-0 items-center gap-0.5">
                  <Select disabled={!hasComposerModelOptions} value={composerModelId ?? ''} onValueChange={onComposerModelChange}>
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
                    disabled={!hasThinkingOptions}
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
                  <button
                    aria-label="Send message unavailable"
                    className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-muted/60 text-muted-foreground/60 transition-colors disabled:cursor-not-allowed"
                    disabled
                    type="button"
                  >
                    <ArrowUp className="h-3.5 w-3.5" strokeWidth={2.5} />
                  </button>
                </div>
              </div>
              <div className="px-2 pb-1 pt-1 text-[10px] leading-relaxed text-muted-foreground">
                <p>
                  <span className="font-medium text-foreground/80">{catalogStatusLabel}</span>
                  {' · '}
                  {catalogStatusDetail}
                </p>
                <p className="mt-0.5">{thinkingStatusDetail}</p>
              </div>
            </div>
          </div>
        </div>
        {showStartRunButton ? (
          <button
            className="shrink-0 flex items-center gap-1.5 rounded-lg border border-border bg-card/80 px-3 py-2 text-[12px] font-medium text-foreground transition-colors hover:border-border/80 hover:bg-card disabled:opacity-50"
            disabled={runtimeRunActionStatus === 'running'}
            onClick={onStartRuntimeRun}
            type="button"
          >
            {runtimeRunActionStatus === 'running' ? (
              <LoaderCircle className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
            ) : (
              <Play className="h-3.5 w-3.5" />
            )}
            Start run
          </button>
        ) : null}
      </div>
    </div>
  )
}
