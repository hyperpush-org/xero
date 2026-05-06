import { createContext, useCallback, useContext, useMemo, useState } from 'react'
import { CheckCircle2, CircleHelp, ListChecks, Loader2, MessageSquare, X } from 'lucide-react'

import type {
  RuntimeActionAnswerShapeDto,
  RuntimeActionRequiredOptionDto,
} from '@/src/lib/xero-model'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/lib/utils'

export type ActionPromptDecision = 'approve' | 'reject' | 'resume'

export interface ActionPromptDispatchValue {
  pendingActionId: string | null
  pendingDecision: ActionPromptDecision | null
  isResolving: boolean
  resolveActionPrompt: (
    actionId: string,
    decision: ActionPromptDecision,
    options?: { userAnswer?: string | null },
  ) => Promise<unknown> | void
}

const ActionPromptDispatchContext = createContext<ActionPromptDispatchValue | null>(null)

export function ActionPromptDispatchProvider({
  value,
  children,
}: {
  value: ActionPromptDispatchValue
  children: React.ReactNode
}) {
  return (
    <ActionPromptDispatchContext.Provider value={value}>
      {children}
    </ActionPromptDispatchContext.Provider>
  )
}

function useActionPromptDispatch(): ActionPromptDispatchValue | null {
  return useContext(ActionPromptDispatchContext)
}

interface ActionPromptCardProps {
  actionId: string
  actionType: string
  title: string
  detail: string
  shape: RuntimeActionAnswerShapeDto
  options: RuntimeActionRequiredOptionDto[] | null
  allowMultiple: boolean
  resolved?: boolean
}

export function ActionPromptCard({
  actionId,
  actionType: _actionType,
  title,
  detail,
  shape,
  options,
  allowMultiple,
  resolved = false,
}: ActionPromptCardProps) {
  const dispatch = useActionPromptDispatch()
  const isPendingForThis =
    dispatch?.pendingActionId === actionId && dispatch?.isResolving === true
  const isLockedOut = resolved || isPendingForThis

  const Icon = useMemo(() => {
    if (shape === 'single_choice') return CircleHelp
    if (shape === 'multi_choice') return ListChecks
    return MessageSquare
  }, [shape])

  return (
    <div
      className={cn(
        'group/action-prompt rounded-md border border-border/50 bg-card/40 px-3 py-2.5',
        'flex flex-col gap-2.5',
      )}
      data-action-id={actionId}
    >
      <div className="flex items-start gap-2">
        <Icon
          aria-hidden="true"
          className={cn(
            'mt-0.5 h-3.5 w-3.5 shrink-0',
            resolved ? 'text-muted-foreground/60' : 'text-primary/80',
          )}
        />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="truncate text-[12.5px] font-medium text-foreground">{title}</span>
          {detail.length > 0 ? (
            <span className="text-[12px] text-muted-foreground">{detail}</span>
          ) : null}
        </div>
        {resolved ? (
          <span className="inline-flex items-center gap-1 rounded-sm bg-muted/40 px-1.5 py-0.5 text-[10.5px] font-medium text-muted-foreground">
            <CheckCircle2 className="h-3 w-3" /> Resolved
          </span>
        ) : isPendingForThis ? (
          <span className="inline-flex items-center gap-1 rounded-sm bg-primary/10 px-1.5 py-0.5 text-[10.5px] font-medium text-primary">
            <Loader2 className="h-3 w-3 animate-spin" /> Sending…
          </span>
        ) : null}
      </div>

      {shape === 'single_choice' && options ? (
        <SingleChoiceBody
          actionId={actionId}
          options={options}
          disabled={isLockedOut || !dispatch}
          onPick={(optionId) =>
            dispatch?.resolveActionPrompt(actionId, 'approve', { userAnswer: optionId })
          }
        />
      ) : null}

      {shape === 'multi_choice' && options ? (
        <MultiChoiceBody
          actionId={actionId}
          options={options}
          allowMultiple={allowMultiple}
          disabled={isLockedOut || !dispatch}
          onSubmit={(optionIds) =>
            dispatch?.resolveActionPrompt(actionId, 'approve', {
              userAnswer: JSON.stringify(optionIds),
            })
          }
        />
      ) : null}

      {(shape === 'plain_text' || shape === 'terminal_input') && (
        <FreeformBody
          actionId={actionId}
          shape={shape}
          disabled={isLockedOut || !dispatch}
          onApprove={(value) =>
            dispatch?.resolveActionPrompt(actionId, 'approve', { userAnswer: value })
          }
          onReject={() => dispatch?.resolveActionPrompt(actionId, 'reject')}
        />
      )}
    </div>
  )
}

function SingleChoiceBody({
  actionId,
  options,
  disabled,
  onPick,
}: {
  actionId: string
  options: RuntimeActionRequiredOptionDto[]
  disabled: boolean
  onPick: (optionId: string) => void
}) {
  return (
    <div role="radiogroup" aria-label="Choose one option" className="flex flex-col gap-1.5">
      {options.map((option) => (
        <button
          key={`${actionId}:${option.id}`}
          type="button"
          role="radio"
          aria-checked="false"
          disabled={disabled}
          onClick={() => onPick(option.id)}
          className={cn(
            'group/choice flex items-start gap-2 rounded-md border border-border/40 bg-background/40 px-2.5 py-1.5 text-left text-[12px] transition-colors',
            'hover:border-primary/40 hover:bg-primary/5',
            'focus-visible:border-primary/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30',
            'disabled:cursor-not-allowed disabled:opacity-60',
          )}
        >
          <span className="mt-0.5 h-3 w-3 shrink-0 rounded-full border border-border/60 group-hover/choice:border-primary/60" />
          <span className="flex min-w-0 flex-col gap-0.5">
            <span className="truncate font-medium text-foreground">{option.label}</span>
            {option.description ? (
              <span className="text-[11px] text-muted-foreground">{option.description}</span>
            ) : null}
          </span>
        </button>
      ))}
    </div>
  )
}

function MultiChoiceBody({
  actionId,
  options,
  allowMultiple,
  disabled,
  onSubmit,
}: {
  actionId: string
  options: RuntimeActionRequiredOptionDto[]
  allowMultiple: boolean
  disabled: boolean
  onSubmit: (optionIds: string[]) => void
}) {
  const [selected, setSelected] = useState<readonly string[]>([])
  const toggle = useCallback(
    (optionId: string) => {
      setSelected((current) => {
        if (current.includes(optionId)) {
          return current.filter((id) => id !== optionId)
        }
        if (!allowMultiple) {
          return [optionId]
        }
        return [...current, optionId]
      })
    },
    [allowMultiple],
  )
  const canSubmit = selected.length > 0 && !disabled

  return (
    <div className="flex flex-col gap-1.5">
      <div role="group" aria-label="Pick options" className="flex flex-col gap-1.5">
        {options.map((option) => {
          const checked = selected.includes(option.id)
          return (
            <label
              key={`${actionId}:${option.id}`}
              className={cn(
                'flex cursor-pointer items-start gap-2 rounded-md border border-border/40 bg-background/40 px-2.5 py-1.5 text-[12px] transition-colors',
                'hover:border-primary/40 hover:bg-primary/5',
                disabled ? 'cursor-not-allowed opacity-60' : null,
                checked ? 'border-primary/50 bg-primary/5' : null,
              )}
            >
              <input
                type="checkbox"
                className="mt-0.5 h-3 w-3 shrink-0 accent-primary"
                checked={checked}
                disabled={disabled}
                onChange={() => toggle(option.id)}
              />
              <span className="flex min-w-0 flex-col gap-0.5">
                <span className="truncate font-medium text-foreground">{option.label}</span>
                {option.description ? (
                  <span className="text-[11px] text-muted-foreground">{option.description}</span>
                ) : null}
              </span>
            </label>
          )
        })}
      </div>
      <div className="flex justify-end">
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={!canSubmit}
          onClick={() => onSubmit([...selected])}
          className="h-7 px-2.5 text-[12px]"
        >
          Submit
        </Button>
      </div>
    </div>
  )
}

function FreeformBody({
  actionId,
  shape,
  disabled,
  onApprove,
  onReject,
}: {
  actionId: string
  shape: 'plain_text' | 'terminal_input'
  disabled: boolean
  onApprove: (value: string) => void
  onReject: () => void
}) {
  const [value, setValue] = useState('')
  const trimmed = value.trim()
  const placeholder =
    shape === 'terminal_input'
      ? 'Type the exact terminal input to submit on resume.'
      : 'Optional plain-text rationale for this decision.'

  return (
    <div className="flex flex-col gap-1.5">
      <Textarea
        aria-label="Operator response"
        rows={2}
        className="min-h-[64px] resize-none border-border/50 bg-background/40 text-[12px]"
        disabled={disabled}
        onChange={(event) => setValue(event.target.value)}
        placeholder={placeholder}
        value={value}
      />
      <div className="flex justify-end gap-1.5">
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={disabled}
          onClick={() => onReject()}
          className="h-7 px-2 text-[12px] text-muted-foreground hover:text-destructive"
        >
          <X className="mr-1 h-3 w-3" />
          Reject
        </Button>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={disabled || (shape === 'terminal_input' && trimmed.length === 0)}
          onClick={() => onApprove(trimmed.length > 0 ? trimmed : '')}
          className="h-7 px-2.5 text-[12px]"
          data-action-id={actionId}
        >
          Approve
        </Button>
      </div>
    </div>
  )
}
