'use client'

import { memo, useEffect, useState } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { ChevronDown, ChevronRight, Wrench } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'

import type { ToolFlowNode } from '../build-agent-graph'
import { useAgentCanvasExpansion } from '../expansion-context'

const EFFECT_LABEL: Record<string, string> = {
  observe: 'observe',
  runtime_state: 'runtime',
  write: 'write',
  destructive_write: 'destructive',
  command: 'command',
  process_control: 'process',
  browser_control: 'browser',
  device_control: 'device',
  external_service: 'external',
  skill_runtime: 'skill',
  agent_delegation: 'delegate',
  unknown: 'unknown',
}

const EFFECT_DOT: Record<string, string> = {
  observe: 'bg-emerald-500/70',
  runtime_state: 'bg-amber-500/70',
  write: 'bg-rose-500/70',
  destructive_write: 'bg-rose-600',
  command: 'bg-fuchsia-500/70',
  process_control: 'bg-orange-500/70',
  browser_control: 'bg-indigo-500/70',
  device_control: 'bg-indigo-500/70',
  external_service: 'bg-cyan-500/70',
  skill_runtime: 'bg-violet-500/70',
  agent_delegation: 'bg-primary/80',
  unknown: 'bg-muted-foreground/50',
}

export const ToolNode = memo(function ToolNode({ id, data }: NodeProps<ToolFlowNode>) {
  const { tool } = data
  const [expanded, setExpanded] = useState(false)
  const { setExpanded: reportExpanded } = useAgentCanvasExpansion()

  useEffect(() => {
    reportExpanded(id, expanded)
    return () => {
      reportExpanded(id, false)
    }
  }, [id, expanded, reportExpanded])

  return (
    <>
      <Handle type="target" position={Position.Left} className="!bg-sky-500 !w-2 !h-2" />
      <Handle type="source" position={Position.Right} className="!bg-sky-500 !w-2 !h-2" />
      <div
        className={cn(
          'agent-card overflow-hidden rounded-md border bg-card text-card-foreground shadow-sm',
          'border-sky-500/30 dark:border-sky-400/30',
          expanded && 'is-card-expanded',
        )}
        style={{ width: 240 }}
      >
        <div className={cn('agent-card-body-wrapper', expanded && 'is-open')}>
          <div className="agent-card-body">
            <div className="px-2.5 pt-2 pb-2 space-y-1.5 border-b border-border/60">
              <p className="text-[10.5px] text-muted-foreground leading-snug">
                {tool.description}
              </p>
              <div className="flex flex-wrap items-center gap-1">
                <span className="text-[9px] uppercase tracking-wide text-muted-foreground/70">
                  group
                </span>
                <span className="text-[10px] font-mono">{tool.group}</span>
                <span className="text-[9px] uppercase tracking-wide text-muted-foreground/70 ml-1">
                  effect
                </span>
                <span className="inline-flex items-center gap-1">
                  <span
                    aria-hidden="true"
                    className={cn(
                      'h-1.5 w-1.5 rounded-full',
                      EFFECT_DOT[tool.effectClass] ?? 'bg-muted-foreground/50',
                    )}
                  />
                  <span className="text-[10px] font-mono">
                    {EFFECT_LABEL[tool.effectClass] ?? tool.effectClass}
                  </span>
                </span>
                {tool.riskClass ? (
                  <Badge variant="outline" className="text-[9px] px-1 py-0 ml-1">
                    risk: {tool.riskClass}
                  </Badge>
                ) : null}
              </div>
              {tool.tags.length ? (
                <div className="flex flex-wrap gap-1">
                  {tool.tags.slice(0, 6).map((tag: string) => (
                    <span
                      key={tag}
                      className="text-[9px] uppercase tracking-wide text-muted-foreground bg-muted/40 px-1 py-0.5 rounded"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              ) : null}
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="agent-card-base flex w-full items-center gap-2 px-2.5 py-2 text-left hover:bg-muted/40"
          title={`${tool.name} — ${EFFECT_LABEL[tool.effectClass] ?? tool.effectClass}`}
        >
          <span
            aria-hidden="true"
            className={cn('h-2 w-2 shrink-0 rounded-full', EFFECT_DOT[tool.effectClass] ?? 'bg-muted-foreground/50')}
          />
          <Wrench className="h-3 w-3 shrink-0 text-sky-500" />
          <span className="font-mono text-[11.5px] truncate flex-1">{tool.name}</span>
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
