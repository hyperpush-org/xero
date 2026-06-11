import { createContext, useCallback, useContext, useMemo } from 'react'
import { ArrowRightCircle, Bug, Compass, ListChecks, Wrench } from 'lucide-react'

import type { RuntimeAgentIdDto } from '../../model'
import { getRuntimeAgentLabel } from '../../model'
import { Button } from '../ui/button'
import { cn } from '../../lib/utils'

export type RoutingSuggestionDecision =
  | {
      kind: 'accept'
      targetAgentId: RuntimeAgentIdDto
      targetAgentDefinitionId?: string | null
      targetAgentDefinitionVersion?: number | null
      targetLabel?: string | null
      reason?: string | null
      summary?: string | null
      resolutionMode?: 'manual' | 'automatic'
    }
  | {
      kind: 'decline'
      targetAgentId: RuntimeAgentIdDto
      targetAgentDefinitionId?: string | null
      targetAgentDefinitionVersion?: number | null
      targetLabel?: string | null
      reason?: string | null
      summary?: string | null
      resolutionMode?: 'manual' | 'automatic'
    }

export interface RoutingSuggestionDispatchValue {
  resolveRoutingSuggestion: (turnId: string, decision: RoutingSuggestionDecision) => void
  getRoutingSuggestionActionAvailability?: (turnId: string) => {
    disabled: boolean
    reason?: string | null
  }
}

const RoutingSuggestionDispatchContext = createContext<RoutingSuggestionDispatchValue | null>(null)

export function RoutingSuggestionDispatchProvider({
  value,
  children,
}: {
  value: RoutingSuggestionDispatchValue
  children: React.ReactNode
}) {
  return (
    <RoutingSuggestionDispatchContext.Provider value={value}>
      {children}
    </RoutingSuggestionDispatchContext.Provider>
  )
}

function useRoutingSuggestionDispatch(): RoutingSuggestionDispatchValue | null {
  return useContext(RoutingSuggestionDispatchContext)
}

function iconForTarget(targetAgentId: RuntimeAgentIdDto) {
  switch (targetAgentId) {
    case 'plan':
      return ListChecks
    case 'engineer':
      return Wrench
    case 'debug':
      return Bug
    default:
      return Compass
  }
}

export interface RoutingSuggestionCardProps {
  turnId: string
  targetKind: 'built_in' | 'custom'
  targetAgentId: RuntimeAgentIdDto
  targetAgentDefinitionId: string | null
  targetAgentDefinitionVersion: number | null
  targetLabel: string | null
  reason: string
  summary: string
  isResolved: boolean
  acceptedTarget: RuntimeAgentIdDto | null
  acceptedTargetAgentDefinitionId: string | null
  acceptedTargetLabel: string | null
  resolutionMode?: 'manual' | 'automatic' | null
  currentAgentLabel?: string | null
}

export function RoutingSuggestionCard({
  turnId,
  targetKind,
  targetAgentId,
  targetAgentDefinitionId,
  targetAgentDefinitionVersion,
  targetLabel,
  reason,
  summary,
  isResolved,
  acceptedTarget,
  acceptedTargetAgentDefinitionId,
  acceptedTargetLabel,
  resolutionMode = null,
  currentAgentLabel = null,
}: RoutingSuggestionCardProps) {
  const dispatch = useRoutingSuggestionDispatch()
  const actionAvailability =
    dispatch?.getRoutingSuggestionActionAvailability?.(turnId) ?? null
  const actionsDisabled = Boolean(!dispatch || actionAvailability?.disabled)
  const disabledReason = actionAvailability?.reason?.trim() || undefined
  const TargetIcon = useMemo(() => iconForTarget(targetAgentId), [targetAgentId])
  const displayTargetLabel =
    targetLabel?.trim() ||
    (targetKind === 'custom' ? 'a custom agent' : getRuntimeAgentLabel(targetAgentId))
  const targetDescription =
    targetKind === 'custom' ? displayTargetLabel : `the ${displayTargetLabel} agent`
  const resolvedTargetLabel =
    acceptedTargetLabel?.trim() ||
    (acceptedTargetAgentDefinitionId ? 'custom agent' : null) ||
    (acceptedTarget ? getRuntimeAgentLabel(acceptedTarget) : null)
  const displayCurrentAgentLabel = currentAgentLabel?.trim() || 'current agent'

  const handleAccept = useCallback(() => {
    if (isResolved || !dispatch || actionsDisabled) return
    dispatch.resolveRoutingSuggestion(turnId, {
      kind: 'accept',
      targetAgentId,
      targetAgentDefinitionId,
      targetAgentDefinitionVersion,
      targetLabel: displayTargetLabel,
      reason,
      summary,
    })
  }, [
    dispatch,
    actionsDisabled,
    displayTargetLabel,
    isResolved,
    reason,
    summary,
    targetAgentDefinitionId,
    targetAgentDefinitionVersion,
    targetAgentId,
    turnId,
  ])

  const handleDecline = useCallback(() => {
    if (isResolved || !dispatch || actionsDisabled) return
    dispatch.resolveRoutingSuggestion(turnId, {
      kind: 'decline',
      targetAgentId,
      targetAgentDefinitionId,
      targetAgentDefinitionVersion,
      targetLabel: displayTargetLabel,
      reason,
      summary,
    })
  }, [
    dispatch,
    actionsDisabled,
    displayTargetLabel,
    isResolved,
    reason,
    summary,
    targetAgentDefinitionId,
    targetAgentDefinitionVersion,
    targetAgentId,
    turnId,
  ])

  return (
    <div
      className={cn(
        'rounded-md border border-border/55 bg-card/45 px-2.5 py-2 shadow-sm shadow-black/5',
        'transition-colors hover:border-border/75 hover:bg-card/55',
      )}
      data-routing-turn-id={turnId}
      data-resolved={isResolved ? 'true' : 'false'}
    >
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex min-w-0 items-center gap-2.5">
          <span
            className={cn(
              'inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md border',
              isResolved
                ? 'border-border/60 bg-muted/35 text-muted-foreground'
                : 'border-primary/25 bg-primary/10 text-primary',
            )}
          >
            <Compass aria-hidden="true" className="h-3.5 w-3.5" />
          </span>
          <div className="min-w-0 text-[12.5px] font-medium leading-snug text-foreground">
            This task may be better suited for {targetDescription}
          </div>
        </div>

        {isResolved ? (
          <div className="flex shrink-0 items-center gap-1.5 rounded-md border border-border/45 bg-muted/25 px-2 py-1 text-[11.5px] text-muted-foreground">
            {acceptedTarget ? (
              <>
                <ArrowRightCircle className="h-3.5 w-3.5" aria-hidden />
                <span>
                  {resolutionMode === 'automatic' ? 'Auto-switched' : 'Switched'} to{' '}
                  {resolvedTargetLabel ?? getRuntimeAgentLabel(acceptedTarget)} and continued.
                </span>
              </>
            ) : (
              <span>Continued with {displayCurrentAgentLabel}.</span>
            )}
          </div>
        ) : (
          <div className="flex shrink-0 items-center gap-1.5 sm:justify-end">
            <Button
              type="button"
              size="sm"
              onClick={handleAccept}
              disabled={actionsDisabled}
              title={disabledReason}
              className="h-7 gap-1.5 px-2.5 text-[12px]"
            >
              <TargetIcon className="h-3.5 w-3.5" aria-hidden />
              Switch to {displayTargetLabel}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={handleDecline}
              disabled={actionsDisabled}
              title={disabledReason}
              className="h-7 px-2.5 text-[12px]"
            >
              Continue with {displayCurrentAgentLabel}
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}
