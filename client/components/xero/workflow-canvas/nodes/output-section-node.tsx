'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { Layers } from 'lucide-react'

import { cn } from '@/lib/utils'

import type { OutputSectionFlowNode } from '../build-agent-graph'
import { AGENT_GRAPH_TRIGGER_HANDLES } from '../build-agent-graph'

const TRIGGER_HANDLE_CLASS = '!bg-fuchsia-500'

const EMPHASIS_DOT: Record<string, string> = {
  core: 'bg-foreground',
  standard: 'bg-foreground/50',
  optional: 'bg-foreground/25',
}

const EMPHASIS_LABEL: Record<string, string> = {
  core: 'core',
  standard: 'std',
  optional: 'opt',
}

export const OutputSectionNode = memo(function OutputSectionNode({ data }: NodeProps<OutputSectionFlowNode>) {
  const { section } = data

  return (
    <>
      <Handle type="target" position={Position.Top} className="!bg-foreground" />
      <Handle
        id={AGENT_GRAPH_TRIGGER_HANDLES.target}
        type="target"
        position={Position.Left}
        className={TRIGGER_HANDLE_CLASS}
      />
      <Handle
        id={AGENT_GRAPH_TRIGGER_HANDLES.source}
        type="source"
        position={Position.Right}
        className={TRIGGER_HANDLE_CLASS}
      />
      <div
        className="agent-card overflow-hidden text-card-foreground"
        style={{ width: 200 }}
      >
        <div className="agent-card-tone-strip" data-tone="foreground" />
        <div className="flex w-full items-center gap-2 px-3 py-2 text-left">
          <span
            aria-hidden="true"
            className={cn(
              'h-2 w-2 shrink-0 rounded-full',
              EMPHASIS_DOT[section.emphasis] ?? 'bg-foreground/30',
            )}
          />
          <Layers className="h-3 w-3 shrink-0 text-muted-foreground" />
          <span className="text-[12px] font-medium truncate flex-1">
            {section.label}
          </span>
          <span className="agent-node-secondary text-[10px] uppercase tracking-wide text-muted-foreground/70">
            {EMPHASIS_LABEL[section.emphasis] ?? section.emphasis}
          </span>
        </div>
      </div>
    </>
  )
})
