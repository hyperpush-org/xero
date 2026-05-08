'use client'

import { memo, useLayoutEffect, useState } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { ChevronDown, ChevronRight, Wrench } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'

import type { ToolFlowNode } from '../build-agent-graph'
import { humanizeIdentifier, toolCategoryPresentationForGroup } from '../build-agent-graph'
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
  const { tool, directConnectionHandles } = data
  const [expanded, setExpanded] = useState(false)
  const { locked, setExpanded: reportExpanded } = useAgentCanvasExpansion()
  const displayName = humanizeIdentifier(tool.name)
  const displayCategory = toolCategoryPresentationForGroup(tool.group).label

  // Report before paint so React Flow doesn't commit an intermediate measured
  // height while the tool body is beginning its collapse transition.
  useLayoutEffect(() => {
    reportExpanded(id, expanded)
    return () => {
      reportExpanded(id, false)
    }
  }, [id, expanded, reportExpanded])

  return (
    <>
      {directConnectionHandles.target ? (
        <Handle type="target" position={Position.Left} className="!bg-sky-500 !w-2 !h-2" />
      ) : null}
      {directConnectionHandles.source ? (
        <Handle type="source" position={Position.Right} className="!bg-sky-500 !w-2 !h-2" />
      ) : null}
      <div
        className={cn(
          'agent-card overflow-hidden text-card-foreground',
          expanded && 'is-card-expanded',
        )}
        style={{
          width: 240,
          borderColor: 'color-mix(in oklab, var(--color-sky-500, #0ea5e9) 28%, var(--agent-card-border))',
        }}
      >
        <button
          type="button"
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            event.stopPropagation()
            if (locked) return
            setExpanded((v) => !v)
          }}
          disabled={locked}
          className="nodrag nopan agent-card-base flex w-full items-center gap-2 px-2.5 py-2 text-left hover:bg-muted/40 transition-colors"
          aria-expanded={expanded}
        >
          <span
            aria-hidden="true"
            className={cn(
              'h-2 w-2 shrink-0 rounded-full ring-1 ring-background',
              EFFECT_DOT[tool.effectClass] ?? 'bg-muted-foreground/50',
            )}
          />
          <Wrench className="h-3 w-3 shrink-0 text-sky-500/80" />
          <span className="text-[11.5px] truncate flex-1 text-foreground/95">{displayName}</span>
          {expanded ? (
            <ChevronDown className="agent-node-chevron h-3 w-3 text-muted-foreground/70" />
          ) : (
            <ChevronRight className="agent-node-chevron h-3 w-3 text-muted-foreground/70" />
          )}
        </button>
        <div className={cn('agent-card-body-wrapper', expanded && 'is-open')}>
          <div className="agent-card-body">
            <div className="px-2.5 pt-2 pb-2 space-y-1.5 border-t border-border/40 bg-muted/10">
              <p className="agent-node-detail text-[10.5px] text-muted-foreground leading-relaxed">
                {tool.description}
              </p>
              <div className="agent-node-chip-row flex flex-wrap items-center gap-x-2 gap-y-1">
                <span className="inline-flex items-center gap-1">
                  <span className="text-[9px] uppercase tracking-wider text-muted-foreground/60 font-medium">
                    category
                  </span>
                  <span className="text-[10px] text-foreground/80">{displayCategory}</span>
                </span>
                <span className="inline-flex items-center gap-1">
                  <span className="text-[9px] uppercase tracking-wider text-muted-foreground/60 font-medium">
                    effect
                  </span>
                  <span className="inline-flex items-center gap-1">
                    <span
                      aria-hidden="true"
                      className={cn(
                        'h-1.5 w-1.5 rounded-full ring-1 ring-background',
                        EFFECT_DOT[tool.effectClass] ?? 'bg-muted-foreground/50',
                      )}
                    />
                    <span className="text-[10px] font-mono text-foreground/80">
                      {EFFECT_LABEL[tool.effectClass] ?? tool.effectClass}
                    </span>
                  </span>
                </span>
                {tool.riskClass ? (
                  <Badge variant="outline" className="text-[9px] px-1.5 py-0 font-medium">
                    risk: {tool.riskClass}
                  </Badge>
                ) : null}
              </div>
              {tool.tags.length ? (
                <div className="agent-node-chip-row flex flex-wrap gap-1">
                  {tool.tags.slice(0, 6).map((tag: string) => (
                    <span
                      key={tag}
                      className="text-[9px] uppercase tracking-wider text-muted-foreground/80 bg-muted/40 border border-border/40 px-1.5 py-0.5 rounded font-medium"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </>
  )
})
