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
    }
  | {
      kind: 'decline'
      targetAgentId: RuntimeAgentIdDto
      targetAgentDefinitionId?: string | null
      targetAgentDefinitionVersion?: number | null
      targetLabel?: string | null
      reason?: string | null
      summary?: string | null
    }

export interface RoutingSuggestionDispatchValue {
  resolveRoutingSuggestion: (turnId: string, decision: RoutingSuggestionDecision) => void
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
}: RoutingSuggestionCardProps) {
  const dispatch = useRoutingSuggestionDispatch()
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

  const handleAccept = useCallback(() => {
    if (isResolved || !dispatch) return
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
    if (isResolved || !dispatch) return
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
        'rounded-md border border-l-2 border-l-primary/70 border-border/50 bg-card/40 px-3 py-2.5',
        'flex flex-col gap-2.5',
      )}
      data-routing-turn-id={turnId}
      data-resolved={isResolved ? 'true' : 'false'}
    >
      <div className="flex items-start gap-2">
        <Compass
          aria-hidden="true"
          className={cn(
            'mt-0.5 h-3.5 w-3.5 shrink-0',
            isResolved ? 'text-muted-foreground/60' : 'text-primary/80',
          )}
        />
        <div className="min-w-0 flex-1">
          <div className="text-[12px] font-medium text-foreground">
            This task may be better suited for {targetDescription}
          </div>
          {reason ? (
            <div className="mt-1 text-[12px] leading-snug text-muted-foreground">
              {reason}
            </div>
          ) : null}
          {summary ? (
            <div className="mt-1.5 rounded border border-border/40 bg-muted/30 px-2 py-1 text-[11.5px] leading-snug text-muted-foreground">
              <span className="font-medium text-foreground/80">Carry over: </span>
              {summary}
            </div>
          ) : null}
        </div>
      </div>

      {isResolved ? (
        <div className="flex items-center gap-1.5 text-[11.5px] text-muted-foreground">
          {acceptedTarget ? (
            <>
              <ArrowRightCircle className="h-3.5 w-3.5" aria-hidden />
              <span>
                Switched to {resolvedTargetLabel ?? getRuntimeAgentLabel(acceptedTarget)} and continued.
              </span>
            </>
          ) : (
            <span>Continued with Agent.</span>
          )}
        </div>
      ) : (
        <div className="flex items-center gap-2">
          <Button
            type="button"
            size="sm"
            onClick={handleAccept}
            className="h-7 gap-1.5 text-[12px]"
          >
            <TargetIcon className="h-3.5 w-3.5" aria-hidden />
            Switch to {displayTargetLabel}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={handleDecline}
            className="h-7 text-[12px]"
          >
            Continue with Agent
          </Button>
        </div>
      )}
    </div>
  )
}
