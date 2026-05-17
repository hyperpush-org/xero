import * as SelectPrimitive from '@radix-ui/react-select'
import { ArrowUp, ChevronDown, Cpu, MessageCircle, Mic } from 'lucide-react'
import { forwardRef, useEffect, useRef, type ComponentPropsWithoutRef, type KeyboardEvent, type ReactNode } from 'react'

import { Button } from '../ui/button'
import {
  Select,
  SelectContent,
  SelectItem,
} from '../ui/select'
import { Textarea } from '../ui/textarea'
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/tooltip'
import { cn } from '../../lib/utils'

export interface WebComposerSelectOption {
  id: string
  label: string
}

export interface WebComposerProps {
  draftPrompt: string
  onDraftPromptChange: (value: string) => void
  onSubmit: () => void
  isSendDisabled?: boolean
  placeholder?: string
  agentOptions: readonly WebComposerSelectOption[]
  selectedAgentId: string | null
  onAgentChange: (id: string) => void
  modelOptions: readonly WebComposerSelectOption[]
  selectedModelId: string | null
  onModelChange: (id: string) => void
  className?: string
}

const MAX_TEXTAREA_HEIGHT_PX = 200

const inlineTriggerClassName =
  'flex h-8 w-fit min-w-0 items-center gap-1.5 rounded-md border-0 bg-transparent px-2 text-[13px] font-medium text-muted-foreground/90 whitespace-nowrap shadow-none transition-colors outline-none hover:bg-muted/60 hover:text-foreground focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-50 data-[state=open]:bg-muted/60 data-[state=open]:text-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg]:text-muted-foreground/70'

const inlineSelectContentClassName =
  'max-h-72 min-w-40 border-border/70 bg-card/95 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90'

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
        className={cn(inlineTriggerClassName, className)}
        {...props}
      >
        {icon}
        <span className="line-clamp-1 truncate">{label}</span>
        <ChevronDown aria-hidden="true" className="size-3.5 opacity-60" />
      </button>
    )
  },
)

export function WebComposer({
  draftPrompt,
  onDraftPromptChange,
  onSubmit,
  isSendDisabled = false,
  placeholder = 'Ask anything…',
  agentOptions,
  selectedAgentId,
  onAgentChange,
  modelOptions,
  selectedModelId,
  onModelChange,
  className,
}: WebComposerProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const hasText = draftPrompt.trim().length > 0
  const disabled = isSendDisabled || !hasText

  useEffect(() => {
    const node = textareaRef.current
    if (!node) return
    node.style.height = 'auto'
    const nextHeight = Math.min(node.scrollHeight, MAX_TEXTAREA_HEIGHT_PX)
    node.style.height = `${nextHeight}px`
  }, [draftPrompt])

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      if (!disabled) onSubmit()
    }
  }

  const agentLabel =
    agentOptions.find((option) => option.id === selectedAgentId)?.label ?? 'Agent'
  const modelLabel =
    modelOptions.find((option) => option.id === selectedModelId)?.label ?? 'Model'

  return (
    <div
      className={cn(
        'flex w-full flex-col gap-1.5 rounded-2xl border border-border/60 bg-card/95 px-3 py-2.5 shadow-none supports-[backdrop-filter]:bg-card/80',
        className,
      )}
    >
      <Textarea
        ref={textareaRef}
        value={draftPrompt}
        onChange={(event) => onDraftPromptChange(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        rows={1}
        className="min-h-[32px] resize-none border-0 bg-transparent px-1.5 py-1 text-[15px] leading-relaxed shadow-none placeholder:text-muted-foreground/60 focus-visible:ring-0 dark:bg-transparent"
      />
      <div className="flex items-center gap-1">
        <ComposerInlineSelect
          icon={<MessageCircle aria-hidden="true" className="size-3.5" />}
          label={agentLabel}
          value={selectedAgentId}
          options={agentOptions}
          onChange={onAgentChange}
          ariaLabel="Agent"
        />
        <ComposerInlineSelect
          icon={<Cpu aria-hidden="true" className="size-3.5" />}
          label={modelLabel}
          value={selectedModelId}
          options={modelOptions}
          onChange={onModelChange}
          ariaLabel="Model"
        />
        <div className="ml-auto flex items-center gap-1">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-9 w-9 rounded-md text-muted-foreground/70 hover:text-foreground"
                disabled
                aria-label="Voice input"
              >
                <Mic className="h-4 w-4" strokeWidth={2.25} />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="top">Voice input coming soon</TooltipContent>
          </Tooltip>
          <Button
            type="button"
            size="icon"
            variant="secondary"
            className="h-9 w-9 rounded-md"
            onClick={onSubmit}
            disabled={disabled}
            aria-label="Send message"
          >
            <ArrowUp className="h-4 w-4" strokeWidth={2.25} />
          </Button>
        </div>
      </div>
    </div>
  )
}

interface ComposerInlineSelectProps {
  icon: ReactNode
  label: string
  value: string | null
  options: readonly WebComposerSelectOption[]
  onChange: (id: string) => void
  ariaLabel: string
}

function ComposerInlineSelect({
  icon,
  label,
  value,
  options,
  onChange,
  ariaLabel,
}: ComposerInlineSelectProps) {
  if (options.length === 0) return null
  return (
    <Select value={value ?? undefined} onValueChange={onChange}>
      <SelectPrimitive.Trigger asChild>
        <ComposerInlineTrigger aria-label={ariaLabel} icon={icon} label={label} />
      </SelectPrimitive.Trigger>
      <SelectContent align="start" className={inlineSelectContentClassName}>
        {options.map((option) => (
          <SelectItem key={option.id} value={option.id}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
