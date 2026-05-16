import { ArrowUp, ChevronDown, Plus } from 'lucide-react'
import { useEffect, useRef, type KeyboardEvent } from 'react'

import { Button } from '../ui/button'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
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

const MAX_TEXTAREA_HEIGHT_PX = 240

export function WebComposer({
  draftPrompt,
  onDraftPromptChange,
  onSubmit,
  isSendDisabled = false,
  placeholder = 'Prompt here',
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

  // Auto-grow textarea up to MAX_TEXTAREA_HEIGHT_PX, then scroll inside.
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

  return (
    <div
      className={cn(
        'flex w-full flex-col gap-2 rounded-2xl border border-border bg-card/80 px-3 py-2 shadow-sm backdrop-blur',
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
        className="min-h-[28px] resize-none border-0 bg-transparent px-1 py-1 text-[14px] leading-relaxed shadow-none focus-visible:ring-0"
      />
      <div className="flex items-center justify-between gap-2">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-7 w-7 text-muted-foreground"
              disabled
              aria-label="Add attachment"
            >
              <Plus className="h-4 w-4" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Attachments coming soon</TooltipContent>
        </Tooltip>
        <div className="flex flex-1 items-center justify-end gap-1.5">
          <ComposerInlineSelect
            value={selectedAgentId}
            options={agentOptions}
            onChange={onAgentChange}
            ariaLabel="Agent"
          />
          <ComposerInlineSelect
            value={selectedModelId}
            options={modelOptions}
            onChange={onModelChange}
            ariaLabel="Model"
          />
          <Button
            type="button"
            size="icon"
            className="h-8 w-8 rounded-full"
            onClick={onSubmit}
            disabled={disabled}
            aria-label="Send message"
          >
            <ArrowUp className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  )
}

interface ComposerInlineSelectProps {
  value: string | null
  options: readonly WebComposerSelectOption[]
  onChange: (id: string) => void
  ariaLabel: string
}

function ComposerInlineSelect({ value, options, onChange, ariaLabel }: ComposerInlineSelectProps) {
  if (options.length === 0) return null
  return (
    <Select value={value ?? undefined} onValueChange={onChange}>
      <SelectTrigger
        size="sm"
        aria-label={ariaLabel}
        className="h-7 gap-1 border-0 bg-transparent px-2 text-[12px] text-muted-foreground shadow-none hover:text-foreground focus:ring-0 [&>svg:last-child]:hidden"
      >
        <SelectValue placeholder={ariaLabel} />
        <ChevronDown className="h-3 w-3" />
      </SelectTrigger>
      <SelectContent align="end">
        {options.map((option) => (
          <SelectItem key={option.id} value={option.id}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
