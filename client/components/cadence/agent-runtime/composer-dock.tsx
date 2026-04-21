import { LoaderCircle, Play, Send } from 'lucide-react'

import type { RuntimeRunActionStatus } from '@/src/features/cadence/use-cadence-desktop-state'
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

interface ComposerModelOption {
  value: string
  label: string
}

interface ComposerModelGroup {
  id: string
  label: string
  items: ComposerModelOption[]
}

type ComposerThinkingLevel = 'low' | 'medium' | 'high'

interface ComposerDockProps {
  placeholder: string
  composerModelId: string
  composerModelGroups: ComposerModelGroup[]
  composerThinkingLevel: ComposerThinkingLevel
  onComposerModelChange: (value: string) => void
  onComposerThinkingLevelChange: (value: ComposerThinkingLevel) => void
  showStartRunButton: boolean
  runtimeRunActionStatus: RuntimeRunActionStatus
  onStartRuntimeRun?: () => void
}

const composerInlineSelectTriggerClassName =
  'h-8 max-w-full gap-1.5 border-0 bg-transparent px-1 text-[13px] font-normal text-muted-foreground shadow-none hover:bg-transparent focus-visible:border-transparent focus-visible:ring-0 dark:bg-transparent dark:hover:bg-transparent [&_svg]:text-muted-foreground/80'

const composerInlineSelectContentClassName =
  'max-h-72 border-border/70 bg-card/95 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90'

const SAMPLE_COMPOSER_THINKING_LEVELS: Array<{ value: ComposerThinkingLevel; label: string }> = [
  { value: 'low', label: 'Thinking · low' },
  { value: 'medium', label: 'Thinking · medium' },
  { value: 'high', label: 'Thinking · high' },
]

export function ComposerDock({
  placeholder,
  composerModelId,
  composerModelGroups,
  composerThinkingLevel,
  onComposerModelChange,
  onComposerThinkingLevelChange,
  showStartRunButton,
  runtimeRunActionStatus,
  onStartRuntimeRun,
}: ComposerDockProps) {
  return (
    <div className="relative shrink-0 px-4 pb-7 pt-10">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 -top-14 h-24 bg-gradient-to-b from-background/0 via-background/86 to-background"
      />
      <div className="relative mx-auto flex w-full max-w-[880px] items-end justify-center gap-3">
        <div className="w-full max-w-[620px]">
          <div className="relative overflow-hidden rounded-xl border border-border/70 bg-card/95 shadow-[0_18px_50px_rgba(0,0,0,0.2)] backdrop-blur supports-[backdrop-filter]:bg-card/80">
            <Textarea
              aria-label="Agent input unavailable"
              className="max-h-56 min-h-[120px] resize-none border-0 bg-transparent px-4 pb-12 pt-4 text-[13px] leading-relaxed text-foreground placeholder:text-muted-foreground/55 shadow-none outline-none focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-100"
              disabled
              placeholder={placeholder}
              rows={4}
              value=""
            />
            <div className="absolute bottom-2 left-3 right-14 flex max-w-[calc(100%-5rem)] flex-wrap items-center gap-3">
              <Select value={composerModelId} onValueChange={onComposerModelChange}>
                <SelectTrigger aria-label="Model selector" className={composerInlineSelectTriggerClassName} size="sm">
                  <SelectValue />
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
              <Select value={composerThinkingLevel} onValueChange={(value) => onComposerThinkingLevelChange(value as ComposerThinkingLevel)}>
                <SelectTrigger aria-label="Thinking level selector" className={composerInlineSelectTriggerClassName} size="sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent className={composerInlineSelectContentClassName}>
                  {SAMPLE_COMPOSER_THINKING_LEVELS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <button
              aria-label="Send message unavailable"
              className="absolute bottom-3 right-3 inline-flex h-8 w-8 items-center justify-center rounded-lg bg-foreground/90 text-background opacity-40 shadow-sm"
              disabled
              type="button"
            >
              <Send className="h-3.5 w-3.5" />
            </button>
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
