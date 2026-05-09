'use client'

import { memo, useEffect, useState } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { ChevronDown, ChevronRight, FileText } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'

import type { PromptFlowNode } from '../build-agent-graph'
import { humanizeIdentifier } from '../build-agent-graph'
import { useAgentCanvasExpansion } from '../expansion-context'

const ROLE_LABEL: Record<'system' | 'developer' | 'task', string> = {
  system: 'System',
  developer: 'Developer',
  task: 'Task',
}

export const PromptNode = memo(function PromptNode({ id, data }: NodeProps<PromptFlowNode>) {
  const { prompt } = data
  const [expanded, setExpanded] = useState(false)
  const tokenEstimate = Math.ceil(prompt.body.length / 4)
  const { locked, setExpanded: reportExpanded } = useAgentCanvasExpansion()

  useEffect(() => {
    reportExpanded(id, expanded)
    return () => {
      reportExpanded(id, false)
    }
  }, [id, expanded, reportExpanded])

  return (
    <>
      <Handle type="target" position={Position.Bottom} className="!bg-amber-500" />
      <div
        className={cn(
          'agent-card overflow-hidden text-card-foreground',
          expanded && 'is-card-expanded',
        )}
        style={{ width: 300 }}
      >
        <div className="agent-card-tone-strip" data-tone="amber" />
        <div className={cn('agent-card-body-wrapper', expanded && 'is-open')}>
          <div className="agent-card-body">
            <div className="px-3 pt-2 pb-2 space-y-1.5 border-b border-border/40 bg-muted/10">
              <div className="flex items-center gap-2 text-[10px] text-muted-foreground/80 uppercase tracking-wider font-medium">
                <span>{humanizeIdentifier(prompt.source)}</span>
                {prompt.policy ? (
                  <Badge
                    variant="outline"
                    className="text-[10px] px-1.5 py-0 font-medium"
                  >
                    policy: {humanizeIdentifier(prompt.policy)}
                  </Badge>
                ) : null}
              </div>
              <pre className="whitespace-pre-wrap break-words text-[11px] leading-relaxed max-h-72 overflow-auto bg-muted/30 border border-border/40 rounded-md p-2 font-mono text-foreground/90">
                {prompt.body}
              </pre>
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={() => {
            if (locked) return
            setExpanded((v) => !v)
          }}
          disabled={locked}
          className="nodrag nopan agent-card-base flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-muted/40 transition-colors"
          aria-expanded={expanded}
        >
          <FileText className="h-3 w-3 shrink-0 text-amber-500/80" />
          <span className="text-[12px] truncate flex-1 font-medium text-foreground/95">{prompt.label}</span>
          <Badge variant="outline" className="text-[10px] px-1.5 py-0 font-medium">
            {ROLE_LABEL[prompt.role]}
          </Badge>
          <span className="agent-node-secondary text-[10px] text-muted-foreground/80 tabular-nums">~{tokenEstimate}t</span>
          {expanded ? (
            <ChevronDown className="agent-node-chevron h-3 w-3 text-muted-foreground/70" />
          ) : (
            <ChevronRight className="agent-node-chevron h-3 w-3 text-muted-foreground/70" />
          )}
        </button>
      </div>
    </>
  )
})
