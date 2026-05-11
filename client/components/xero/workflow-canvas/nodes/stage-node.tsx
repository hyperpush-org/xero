'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { CheckSquare, Flag, GitBranch, ListChecks } from 'lucide-react'

import { Badge } from '@/components/ui/badge'

import type { StageFlowNode } from '../build-agent-graph'
import { humanizeIdentifier } from '../build-agent-graph'

const STAGE_HANDLE_CLASS = '!bg-amber-500'

function gateLabel(check: StageFlowNode['data']['phase']['requiredChecks'] extends
  | readonly (infer G)[]
  | undefined
  ? G
  : never): string {
  if (check.kind === 'todo_completed') {
    return `todo: ${check.todoId}`
  }
  const count = check.minCount && check.minCount > 1 ? ` × ${check.minCount}` : ''
  return `tool: ${check.toolName}${count}`
}

export const StageNode = memo(function StageNode({
  data,
}: NodeProps<StageFlowNode>) {
  const { phase, isStart } = data
  const requiredChecks = phase.requiredChecks ?? []

  return (
    <>
      <Handle type="target" position={Position.Left} className={STAGE_HANDLE_CLASS} />
      <Handle type="source" position={Position.Right} className={STAGE_HANDLE_CLASS} />
      <div
        className="agent-card overflow-hidden text-card-foreground"
        style={{ width: 260 }}
        data-testid="stage-node"
        data-phase-id={phase.id}
      >
        <div className="agent-card-tone-strip" data-tone="amber" />
        <div className="space-y-2 px-3 py-2.5">
          <div className="flex items-start gap-2">
            <span className="mt-px inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-amber-500/12 ring-1 ring-amber-500/30">
              <GitBranch className="h-3.5 w-3.5 text-amber-500" aria-hidden="true" />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-1.5">
                <span className="truncate text-[12.5px] font-semibold text-foreground/95">
                  {phase.title || humanizeIdentifier(phase.id)}
                </span>
                {isStart ? (
                  <Badge
                    variant="outline"
                    className="h-5 px-1.5 text-[9.5px] font-medium border-amber-500/40 bg-amber-500/12 text-amber-700 dark:text-amber-300"
                  >
                    <Flag className="mr-0.5 h-2.5 w-2.5" aria-hidden="true" /> start
                  </Badge>
                ) : null}
              </div>
              <p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground/80">
                {phase.id}
              </p>
            </div>
          </div>

          {phase.description ? (
            <p className="agent-node-detail line-clamp-2 text-[10.5px] leading-relaxed text-muted-foreground">
              {phase.description}
            </p>
          ) : null}

          {requiredChecks.length > 0 ? (
            <div className="agent-node-chip-row flex flex-wrap items-center gap-1">
              <ListChecks
                className="h-3 w-3 shrink-0 text-muted-foreground/70"
                aria-hidden="true"
              />
              {requiredChecks.map((check, index) => (
                <Badge
                  key={`${check.kind}:${index}`}
                  variant="outline"
                  className="h-5 px-1.5 text-[9.5px] font-medium border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
                >
                  <CheckSquare className="mr-0.5 h-2.5 w-2.5" aria-hidden="true" />
                  {gateLabel(check)}
                </Badge>
              ))}
            </div>
          ) : null}

          {phase.retryLimit !== undefined ? (
            <div className="text-[10px] text-muted-foreground/80">
              retry limit: {phase.retryLimit}
            </div>
          ) : null}
        </div>
      </div>
    </>
  )
})
