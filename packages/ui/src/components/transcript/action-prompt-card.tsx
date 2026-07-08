import { createContext, useCallback, useContext, useMemo, useRef, useState } from 'react'
import {
  CheckCircle2,
  CircleHelp,
  Eye,
  EyeOff,
  KeyRound,
  ListChecks,
  Loader2,
  MessageSquare,
  X,
} from 'lucide-react'

import type {
  RuntimeActionAnswerShapeDto,
  RuntimeActionRequiredOptionDto,
  RuntimeSensitiveInputFieldDto,
} from '../../model'
import { Button } from '../ui/button'
import { Checkbox } from '../ui/checkbox'
import { Input } from '../ui/input'
import { RadioGroup, RadioGroupItem } from '../ui/radio-group'
import { Textarea } from '../ui/textarea'
import { cn } from '../../lib/utils'

export type ActionPromptDecision = 'approve' | 'reject' | 'resume'

export interface ActionPromptDispatchValue {
  pendingActionId: string | null
  pendingDecision: ActionPromptDecision | null
  isResolving: boolean
  actionError?: {
    actionId: string
    message: string
  } | null
  resolveActionPrompt: (
    actionId: string,
    decision: ActionPromptDecision,
    options?: { userAnswer?: string | null; runId?: string | null; actionType?: string | null },
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

type GuardedAction = () => Promise<unknown> | void

function useGuardedPromptAction(disabled: boolean) {
  const submittingRef = useRef(false)
  const [isSubmitting, setIsSubmitting] = useState(false)

  const run = useCallback(
    (action: GuardedAction) => {
      if (disabled || submittingRef.current) {
        return
      }

      submittingRef.current = true
      setIsSubmitting(true)

      try {
        const result = action()
        const reset = () => {
          submittingRef.current = false
          setIsSubmitting(false)
        }
        void Promise.resolve(result).then(reset, reset)
      } catch (error) {
        submittingRef.current = false
        setIsSubmitting(false)
        throw error
      }
    },
    [disabled],
  )

  return {
    isSubmitting,
    run,
  }
}

interface ActionPromptCardProps {
  actionId: string
  runId?: string | null
  actionType: string
  title: string
  detail: string
  shape: RuntimeActionAnswerShapeDto
  options: RuntimeActionRequiredOptionDto[] | null
  allowMultiple: boolean
  sensitiveFields?: RuntimeSensitiveInputFieldDto[] | null
  intendedUse?: string | null
  resolved?: boolean
}

export function ActionPromptCard({
  actionId,
  runId = null,
  actionType: _actionType,
  title,
  detail,
  shape,
  options,
  allowMultiple,
  sensitiveFields,
  intendedUse,
  resolved = false,
}: ActionPromptCardProps) {
  const dispatch = useActionPromptDispatch()
  const isPendingForThis =
    dispatch?.pendingActionId === actionId && dispatch?.isResolving === true
  const actionError =
    dispatch?.actionError?.actionId === actionId ? dispatch.actionError.message : null
  const isLockedOut = resolved || isPendingForThis

  const Icon = useMemo(() => {
    if (shape === 'sensitive_fields') return KeyRound
    if (shape === 'single_choice') return CircleHelp
    if (shape === 'multi_choice') return ListChecks
    return MessageSquare
  }, [shape])

  const kindLabel = useMemo(() => promptKindLabel(shape, allowMultiple), [shape, allowMultiple])

  return (
    <div
      className={cn(
        'group/action-prompt flex flex-col gap-3.5 rounded-xl border border-border/40 bg-card/30 px-4 py-3.5 transition-colors',
        resolved ? 'opacity-90' : null,
      )}
      data-action-id={actionId}
    >
      <div className="flex items-start gap-3">
        <span
          aria-hidden="true"
          className={cn(
            'mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-lg',
            resolved ? 'bg-muted/50 text-muted-foreground/70' : 'bg-primary/10 text-primary',
          )}
        >
          <Icon className="h-3.5 w-3.5" />
        </span>
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <div className="flex items-start gap-2">
            <span className="min-w-0 flex-1 text-[13px] font-medium leading-snug text-foreground">
              {title}
            </span>
            {resolved ? (
              <span className="agent-prompt-status-pop inline-flex shrink-0 items-center gap-1 rounded-md bg-muted/40 px-2 py-0.5 text-[10.5px] font-medium text-muted-foreground">
                <CheckCircle2 className="h-3 w-3" /> Resolved
              </span>
            ) : isPendingForThis ? (
              <span className="agent-prompt-status-pop inline-flex shrink-0 items-center gap-1 rounded-md bg-primary/10 px-2 py-0.5 text-[10.5px] font-medium text-primary">
                <Loader2 className="h-3 w-3 animate-spin" /> Sending…
              </span>
            ) : (
              <span className="shrink-0 rounded-md bg-muted/40 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/80">
                {kindLabel}
              </span>
            )}
          </div>
          {detail.length > 0 ? (
            <span className="text-[12px] leading-relaxed text-muted-foreground">{detail}</span>
          ) : null}
          {actionError ? (
            <span className="text-[12px] font-medium text-destructive">{actionError}</span>
          ) : null}
        </div>
      </div>

      {shape === 'single_choice' && options ? (
        <SingleChoiceBody
          actionId={actionId}
          options={options}
          disabled={isLockedOut || !dispatch}
          onSubmit={(answer) =>
            dispatch?.resolveActionPrompt(actionId, 'approve', {
              userAnswer: answer,
              runId,
              actionType: _actionType,
            })
          }
        />
      ) : null}

      {shape === 'multi_choice' && options ? (
        <MultiChoiceBody
          actionId={actionId}
          options={options}
          allowMultiple={allowMultiple}
          disabled={isLockedOut || !dispatch}
          onSubmit={(answer) =>
            dispatch?.resolveActionPrompt(actionId, 'approve', {
              userAnswer: answer,
              runId,
              actionType: _actionType,
            })
          }
        />
      ) : null}

      {shape === 'sensitive_fields' && sensitiveFields ? (
        <SensitiveFieldsBody
          actionId={actionId}
          fields={sensitiveFields}
          intendedUse={intendedUse ?? null}
          disabled={isLockedOut || !dispatch}
          onApprove={(values) =>
            dispatch?.resolveActionPrompt(actionId, 'approve', {
              userAnswer: JSON.stringify(values),
              runId,
              actionType: _actionType,
            })
          }
          onReject={() =>
            dispatch?.resolveActionPrompt(actionId, 'reject', {
              runId,
              actionType: _actionType,
            })
          }
        />
      ) : null}

      {(
        shape === 'plain_text' ||
        shape === 'terminal_input' ||
        shape === 'short_text' ||
        shape === 'long_text' ||
        shape === 'number' ||
        shape === 'date'
      ) && (
        <FreeformBody
          actionId={actionId}
          shape={shape}
          disabled={isLockedOut || !dispatch}
          onApprove={(value) =>
            dispatch?.resolveActionPrompt(actionId, 'approve', {
              userAnswer: value,
              runId,
              actionType: _actionType,
            })
          }
          onReject={() =>
            dispatch?.resolveActionPrompt(actionId, 'reject', {
              runId,
              actionType: _actionType,
            })
          }
        />
      )}
    </div>
  )
}

function promptKindLabel(shape: RuntimeActionAnswerShapeDto, allowMultiple: boolean): string {
  switch (shape) {
    case 'single_choice':
      return 'Choose one'
    case 'multi_choice':
      return allowMultiple ? 'Choose any' : 'Choose one'
    case 'sensitive_fields':
      return 'Sensitive'
    case 'terminal_input':
      return 'Terminal'
    case 'number':
      return 'Number'
    case 'date':
      return 'Date'
    case 'short_text':
    case 'long_text':
    case 'plain_text':
      return 'Your input'
  }
}

// Internal-only sentinel id for the user-provided "Something else" choice. It is
// never sent to the runtime — when chosen, the typed text is submitted instead.
const CUSTOM_CHOICE_ID = '__xero_custom_choice__'

const CUSTOM_CHOICE_OPTION: RuntimeActionRequiredOptionDto = {
  id: CUSTOM_CHOICE_ID,
  label: 'Something else',
  description: 'Provide your own answer instead of the options above.',
}

function appendChoiceNote(answer: string, note: string): string {
  const body = answer.trim()
  const trimmedNote = note.trim()
  if (trimmedNote.length === 0) {
    return body
  }
  if (body.length === 0) {
    return `Note: ${trimmedNote}`
  }
  return `${body}\n\nNote: ${trimmedNote}`
}

function PromptNoteField({
  value,
  onChange,
  disabled,
}: {
  value: string
  onChange: (value: string) => void
  disabled: boolean
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[11px] font-medium text-muted-foreground">Add a note (optional)</span>
      <Textarea
        aria-label="Add a note"
        rows={2}
        className="min-h-[56px] resize-none border-border/40 bg-muted/20 text-[12px]"
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        placeholder="Add context, constraints, or reasoning for your choice."
        value={value}
      />
    </div>
  )
}

function CustomChoiceField({
  value,
  onChange,
  disabled,
}: {
  value: string
  onChange: (value: string) => void
  disabled: boolean
}) {
  return (
    <Input
      aria-label="Your own answer"
      autoComplete="off"
      autoFocus
      className="h-9 border-border/40 bg-muted/20 text-[12px]"
      disabled={disabled}
      onChange={(event) => onChange(event.target.value)}
      placeholder="Type your own answer"
      value={value}
    />
  )
}

interface ChoiceRowProps {
  actionId: string
  selected: boolean
  disabled: boolean
  option: RuntimeActionRequiredOptionDto
  onToggle: () => void
}

function choiceControlId(actionId: string, optionId: string): string {
  return `${actionId}:${optionId}`.replace(/[^a-zA-Z0-9_-]+/g, '-')
}

function ChoiceRadioRow({
  actionId,
  selected,
  disabled,
  option,
  onToggle,
}: ChoiceRowProps) {
  const controlId = choiceControlId(actionId, option.id)
  const labelId = `${controlId}-label`
  const descriptionId = option.description ? `${controlId}-description` : undefined
  return (
    <div
      aria-disabled={disabled}
      onClick={() => {
        if (!disabled) {
          onToggle()
        }
      }}
      className={cn(
        'flex w-full items-start gap-3 rounded-lg px-3 py-2.5 text-left text-[12px] transition-colors',
        'focus-within:outline-none focus-within:ring-2 focus-within:ring-primary/30',
        selected
          ? 'bg-primary/10 ring-1 ring-inset ring-primary/25'
          : 'bg-muted/20 hover:bg-muted/40',
        disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer',
      )}
    >
      <RadioGroupItem
        id={controlId}
        value={option.id}
        disabled={disabled}
        aria-labelledby={labelId}
        aria-describedby={descriptionId}
        onClick={(event) => event.stopPropagation()}
        className="mt-0.5"
      />
      <span className="flex min-w-0 flex-col gap-1">
        <span id={labelId} className="font-medium leading-snug text-foreground">{option.label}</span>
        {option.description ? (
          <span id={descriptionId} className="text-[11px] leading-relaxed text-muted-foreground">
            {option.description}
          </span>
        ) : null}
      </span>
    </div>
  )
}

function ChoiceCheckboxRow({ actionId, selected, disabled, option, onToggle }: ChoiceRowProps) {
  const controlId = choiceControlId(actionId, option.id)
  const labelId = `${controlId}-label`
  const descriptionId = option.description ? `${controlId}-description` : undefined
  return (
    <div
      aria-disabled={disabled}
      onClick={() => {
        if (!disabled) {
          onToggle()
        }
      }}
      className={cn(
        'flex w-full items-start gap-3 rounded-lg px-3 py-2.5 text-left text-[12px] transition-colors',
        'focus-within:outline-none focus-within:ring-2 focus-within:ring-primary/30',
        selected
          ? 'bg-primary/10 ring-1 ring-inset ring-primary/25'
          : 'bg-muted/20 hover:bg-muted/40',
        disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer',
      )}
    >
      <Checkbox
        id={controlId}
        checked={selected}
        disabled={disabled}
        aria-labelledby={labelId}
        aria-describedby={descriptionId}
        onClick={(event) => event.stopPropagation()}
        onCheckedChange={() => onToggle()}
        className="mt-0.5"
      />
      <span className="flex min-w-0 flex-col gap-1">
        <span id={labelId} className="font-medium leading-snug text-foreground">{option.label}</span>
        {option.description ? (
          <span id={descriptionId} className="text-[11px] leading-relaxed text-muted-foreground">
            {option.description}
          </span>
        ) : null}
      </span>
    </div>
  )
}

function SingleChoiceBody({
  actionId,
  options,
  disabled,
  onSubmit,
}: {
  actionId: string
  options: RuntimeActionRequiredOptionDto[]
  disabled: boolean
  onSubmit: (answer: string) => Promise<unknown> | void
}) {
  const [selected, setSelected] = useState('')
  const [customValue, setCustomValue] = useState('')
  const [note, setNote] = useState('')
  const submitGuard = useGuardedPromptAction(disabled)
  const fieldsDisabled = disabled || submitGuard.isSubmitting
  const isCustom = selected === CUSTOM_CHOICE_ID
  const customTrimmed = customValue.trim()
  const hasSelection = selected.length > 0 && (!isCustom || customTrimmed.length > 0)
  const canSubmit = hasSelection && !disabled && !submitGuard.isSubmitting
  const scrollable = options.length > 6

  const handleSubmit = () => {
    const selectionAnswer = isCustom
      ? customTrimmed
      : options.find((option) => option.id === selected)?.id ?? selected
    return onSubmit(appendChoiceNote(selectionAnswer, note))
  }

  return (
    <div className="flex flex-col gap-3">
      <RadioGroup
        value={selected}
        onValueChange={setSelected}
        disabled={disabled}
        role="radiogroup"
        aria-label="Choose one option"
        className={cn(
          'flex flex-col gap-2',
          scrollable ? 'max-h-72 overflow-y-auto pr-1' : null,
        )}
      >
        {options.map((option) => (
          <ChoiceRadioRow
            key={`${actionId}:${option.id}`}
            actionId={actionId}
            option={option}
            selected={selected === option.id}
            disabled={disabled}
            onToggle={() => setSelected(option.id)}
          />
        ))}
        <ChoiceRadioRow
          key={`${actionId}:${CUSTOM_CHOICE_ID}`}
          actionId={actionId}
          option={CUSTOM_CHOICE_OPTION}
          selected={isCustom}
          disabled={disabled}
          onToggle={() => setSelected(CUSTOM_CHOICE_ID)}
        />
      </RadioGroup>
      {isCustom ? (
        <CustomChoiceField value={customValue} onChange={setCustomValue} disabled={fieldsDisabled} />
      ) : null}
      <PromptNoteField value={note} onChange={setNote} disabled={fieldsDisabled} />
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] text-muted-foreground">
          {hasSelection ? '1 selected' : isCustom ? 'Add your answer' : 'Choose one option'}
        </span>
        <Button
          type="button"
          size="sm"
          disabled={!canSubmit}
          onClick={() => submitGuard.run(handleSubmit)}
          className="h-8 px-4 text-[12px]"
        >
          Submit
        </Button>
      </div>
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
  onSubmit: (answer: string) => Promise<unknown> | void
}) {
  const [selected, setSelected] = useState<readonly string[]>([])
  const [customValue, setCustomValue] = useState('')
  const [note, setNote] = useState('')
  const submitGuard = useGuardedPromptAction(disabled)
  const fieldsDisabled = disabled || submitGuard.isSubmitting
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
  const isCustom = selected.includes(CUSTOM_CHOICE_ID)
  const customTrimmed = customValue.trim()
  const customSatisfied = !isCustom || customTrimmed.length > 0
  const canSubmit =
    selected.length > 0 && customSatisfied && !disabled && !submitGuard.isSubmitting
  const scrollable = options.length > 6

  const handleSubmit = () => {
    const selections = selected.map((id) =>
      id === CUSTOM_CHOICE_ID
        ? customTrimmed
        : options.find((option) => option.id === id)?.id ?? id,
    )
    return onSubmit(appendChoiceNote(JSON.stringify(selections), note))
  }

  return (
    <div className="flex flex-col gap-3">
      <div
        role="group"
        aria-label="Pick options"
        className={cn(
          'flex flex-col gap-2',
          scrollable ? 'max-h-72 overflow-y-auto pr-1' : null,
        )}
      >
        {options.map((option) => (
          <ChoiceCheckboxRow
            key={`${actionId}:${option.id}`}
            actionId={actionId}
            option={option}
            selected={selected.includes(option.id)}
            disabled={disabled}
            onToggle={() => toggle(option.id)}
          />
        ))}
        <ChoiceCheckboxRow
          key={`${actionId}:${CUSTOM_CHOICE_ID}`}
          actionId={actionId}
          option={CUSTOM_CHOICE_OPTION}
          selected={isCustom}
          disabled={disabled}
          onToggle={() => toggle(CUSTOM_CHOICE_ID)}
        />
      </div>
      {isCustom ? (
        <CustomChoiceField value={customValue} onChange={setCustomValue} disabled={fieldsDisabled} />
      ) : null}
      <PromptNoteField value={note} onChange={setNote} disabled={fieldsDisabled} />
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] text-muted-foreground">
          {selected.length > 0
            ? `${selected.length} selected`
            : allowMultiple
              ? 'Select one or more'
              : 'Select one'}
        </span>
        <Button
          type="button"
          size="sm"
          disabled={!canSubmit}
          onClick={() => submitGuard.run(handleSubmit)}
          className="h-8 px-4 text-[12px]"
        >
          Submit
        </Button>
      </div>
    </div>
  )
}

function SensitiveFieldsBody({
  actionId,
  fields,
  intendedUse,
  disabled,
  onApprove,
  onReject,
}: {
  actionId: string
  fields: RuntimeSensitiveInputFieldDto[]
  intendedUse: string | null
  disabled: boolean
  onApprove: (values: Record<string, string>) => Promise<unknown> | void
  onReject: () => Promise<unknown> | void
}) {
  const [values, setValues] = useState<Record<string, string>>({})
  const [revealed, setRevealed] = useState<Record<string, boolean>>({})
  const requiredMissing = fields.some((field) => field.required && !values[field.key]?.trim())
  const submitGuard = useGuardedPromptAction(disabled)
  const isDisabled = disabled || submitGuard.isSubmitting

  return (
    <div className="flex flex-col gap-2.5">
      {intendedUse ? (
        <div className="rounded-lg bg-muted/25 px-3 py-2 text-[11.5px] leading-relaxed text-muted-foreground">
          {intendedUse}
        </div>
      ) : null}
      <div className="flex flex-col gap-2.5">
        {fields.map((field) => {
          const inputId = `${actionId}:${field.key}`
          const isRevealed = revealed[field.key] === true
          const value = values[field.key] ?? ''
          return (
            <div key={field.key} className="flex flex-col gap-1">
              <div className="flex items-center justify-between gap-2">
                <label htmlFor={inputId} className="truncate text-[12px] font-medium text-foreground">
                  {field.label}
                </label>
                <span className="shrink-0 text-[10.5px] text-muted-foreground">
                  {field.required ? 'Required' : 'Optional'}
                </span>
              </div>
              {field.description ? (
                <span className="text-[11px] text-muted-foreground">{field.description}</span>
              ) : null}
              <div className="flex items-center gap-1.5">
                <Input
                  id={inputId}
                  aria-label={field.label}
                  autoComplete="off"
                  className="h-9 border-border/40 bg-muted/20 text-[12px]"
                  disabled={isDisabled}
                  onChange={(event) =>
                    setValues((current) => ({
                      ...current,
                      [field.key]: event.target.value,
                    }))
                  }
                  placeholder={field.validationHint ?? 'Enter sensitive value'}
                  type={isRevealed ? 'text' : 'password'}
                  value={value}
                />
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  disabled={isDisabled || value.length === 0}
                  onClick={() =>
                    setRevealed((current) => ({
                      ...current,
                      [field.key]: !isRevealed,
                    }))
                  }
                  className="h-9 w-9 shrink-0"
                  aria-label={isRevealed ? `Hide ${field.label}` : `Reveal ${field.label}`}
                >
                  {isRevealed ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                </Button>
              </div>
            </div>
          )
        })}
      </div>
      <div className="flex justify-end gap-2">
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={isDisabled}
          onClick={() => submitGuard.run(() => onReject())}
          className="h-8 px-3 text-[12px] text-muted-foreground hover:text-destructive"
        >
          <X className="mr-1 h-3 w-3" />
          Deny
        </Button>
        <Button
          type="button"
          size="sm"
          disabled={isDisabled || requiredMissing}
          onClick={() => submitGuard.run(() => {
            const submitted = Object.fromEntries(
              Object.entries(values)
                .map(([key, value]) => [key, value.trim()] as const)
                .filter(([, value]) => value.length > 0),
            )
            return onApprove(submitted)
          })}
          className="h-8 px-4 text-[12px]"
          data-action-id={actionId}
        >
          Approve
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
  shape: RuntimeActionAnswerShapeDto
  disabled: boolean
  onApprove: (value: string) => Promise<unknown> | void
  onReject: () => Promise<unknown> | void
}) {
  const [value, setValue] = useState('')
  const trimmed = value.trim()
  const placeholder = getFreeformPlaceholder(shape)
  const isTextArea = shape === 'plain_text' || shape === 'terminal_input' || shape === 'long_text'
  const requiresValue = shape !== 'plain_text'
  const submitGuard = useGuardedPromptAction(disabled)
  const isDisabled = disabled || submitGuard.isSubmitting

  return (
    <div className="flex flex-col gap-2.5">
      {isTextArea ? (
        <Textarea
          aria-label="Operator response"
          rows={2}
          className="min-h-[72px] resize-none border-border/40 bg-muted/20 text-[12px]"
          disabled={isDisabled}
          onChange={(event) => setValue(event.target.value)}
          placeholder={placeholder}
          value={value}
        />
      ) : (
        <Input
          aria-label="Operator response"
          className="h-9 border-border/40 bg-muted/20 text-[12px]"
          disabled={isDisabled}
          inputMode={shape === 'number' ? 'decimal' : undefined}
          onChange={(event) => setValue(event.target.value)}
          placeholder={placeholder}
          type={shape === 'number' ? 'number' : shape === 'date' ? 'date' : 'text'}
          value={value}
        />
      )}
      <div className="flex justify-end gap-2">
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={isDisabled}
          onClick={() => submitGuard.run(() => onReject())}
          className="h-8 px-3 text-[12px] text-muted-foreground hover:text-destructive"
        >
          <X className="mr-1 h-3 w-3" />
          Reject
        </Button>
        <Button
          type="button"
          size="sm"
          disabled={isDisabled || (requiresValue && trimmed.length === 0)}
          onClick={() => submitGuard.run(() => onApprove(trimmed.length > 0 ? trimmed : ''))}
          className="h-8 px-4 text-[12px]"
          data-action-id={actionId}
        >
          Approve
        </Button>
      </div>
    </div>
  )
}

function getFreeformPlaceholder(shape: RuntimeActionAnswerShapeDto): string {
  switch (shape) {
    case 'terminal_input':
      return 'Type the exact terminal input to submit on resume.'
    case 'short_text':
      return 'Enter a short answer.'
    case 'long_text':
      return 'Enter the details.'
    case 'number':
      return 'Enter a number.'
    case 'date':
      return 'Choose a date.'
    case 'sensitive_fields':
      return 'Enter sensitive values.'
    case 'plain_text':
    case 'single_choice':
    case 'multi_choice':
      return 'Optional plain-text rationale for this decision.'
  }
}
