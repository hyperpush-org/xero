import { createContext, useCallback, useContext, useMemo } from 'react'
import { ArrowRightCircle, Bug, Compass, ListChecks, Wrench } from 'lucide-react'

import type { RuntimeAgentIdDto } from '@/src/lib/xero-model'
import { getRuntimeAgentLabel } from '@/src/lib/xero-model'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

export type RoutingSuggestionDecision =
  | { kind: 'accept'; targetAgentId: RuntimeAgentIdDto }
  | { kind: 'decline' }

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
  targetAgentId: RuntimeAgentIdDto
  reason: string
  summary: string
  isResolved: boolean
  acceptedTarget: RuntimeAgentIdDto | null
}

export function RoutingSuggestionCard({
  turnId,
  targetAgentId,
  reason,
  summary,
  isResolved,
  acceptedTarget,
}: RoutingSuggestionCardProps) {
  const dispatch = useRoutingSuggestionDispatch()
  const TargetIcon = useMemo(() => iconForTarget(targetAgentId), [targetAgentId])
  const targetLabel = getRuntimeAgentLabel(targetAgentId)

  const handleAccept = useCallback(() => {
    if (isResolved || !dispatch) return
    dispatch.resolveRoutingSuggestion(turnId, { kind: 'accept', targetAgentId })
  }, [dispatch, isResolved, targetAgentId, turnId])

  const handleDecline = useCallback(() => {
    if (isResolved || !dispatch) return
    dispatch.resolveRoutingSuggestion(turnId, { kind: 'decline' })
  }, [dispatch, isResolved, turnId])

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
            This task may be better suited for the {targetLabel} agent
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
                Switched to {getRuntimeAgentLabel(acceptedTarget)} for your next message.
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
            Switch to {targetLabel}
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
