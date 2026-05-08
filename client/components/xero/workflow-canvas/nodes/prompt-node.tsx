'use client'

import { memo, useEffect, useState } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { ChevronDown, ChevronRight, FileText } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'

import type { PromptFlowNode } from '../build-agent-graph'
import { useAgentCanvasExpansion } from '../expansion-context'

const ROLE_LABEL: Record<'system' | 'developer' | 'task', string> = {
  system: 'System',
  developer: 'Developer',
  task: 'Task',
}

const ROLE_DOT: Record<'system' | 'developer' | 'task', string> = {
  system: 'bg-amber-500',
  developer: 'bg-violet-400',
  task: 'bg-emerald-500',
}

export const PromptNode = memo(function PromptNode({ id, data }: NodeProps<PromptFlowNode>) {
  const { prompt } = data
  const [expanded, setExpanded] = useState(false)
  const tokenEstimate = Math.ceil(prompt.body.length / 4)
  const { setExpanded: reportExpanded } = useAgentCanvasExpansion()

  useEffect(() => {
    reportExpanded(id, expanded)
    return () => {
      reportExpanded(id, false)
    }
  }, [id, expanded, reportExpanded])

  return (
    <>
      <Handle type="target" position={Position.Bottom} className="!bg-amber-500 !w-2 !h-2" />
      <div
        className={cn(
          'agent-card overflow-hidden rounded-md border bg-card text-card-foreground shadow-sm',
          'border-amber-500/30 dark:border-amber-400/30',
          expanded && 'is-card-expanded',
        )}
        style={{ width: 300 }}
      >
        <div className={cn('agent-card-body-wrapper', expanded && 'is-open')}>
          <div className="agent-card-body">
            <div className="px-2.5 pt-2 pb-2 space-y-1.5 border-b border-border/60">
              <div className="flex items-center gap-2 text-[9px] text-muted-foreground uppercase tracking-wide">
                <span>{prompt.source}</span>
                {prompt.policy ? (
                  <Badge variant="outline" className="text-[9px] px-1 py-0">
                    policy: {prompt.policy}
                  </Badge>
                ) : null}
              </div>
              <pre className="whitespace-pre-wrap break-words text-[10px] leading-snug max-h-72 overflow-auto bg-muted/40 rounded p-1.5">
                {prompt.body}
              </pre>
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="agent-card-base flex w-full items-center gap-2 px-2.5 py-2 text-left hover:bg-muted/40"
        >
          <span
            aria-hidden="true"
            className={cn('h-2 w-2 shrink-0 rounded-full', ROLE_DOT[prompt.role])}
          />
          <FileText className="h-3 w-3 shrink-0 text-amber-500" />
          <span className="text-[11.5px] truncate flex-1 font-medium">{prompt.label}</span>
          <Badge variant="outline" className="text-[9px] px-1 py-0">
            {ROLE_LABEL[prompt.role]}
          </Badge>
          <span className="text-[9px] text-muted-foreground tabular-nums">~{tokenEstimate}t</span>
          {expanded ? (
            <ChevronDown className="h-3 w-3 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-3 w-3 text-muted-foreground" />
          )}
        </button>
      </div>
    </>
  )
})
