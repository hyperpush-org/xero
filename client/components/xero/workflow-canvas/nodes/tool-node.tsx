'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { Wrench } from 'lucide-react'

import { cn } from '@/lib/utils'

import type { ToolFlowNode } from '../build-agent-graph'
import {
  AGENT_GRAPH_TRIGGER_HANDLES,
  humanizeIdentifier,
} from '../build-agent-graph'
import { useCanvasMode } from '../canvas-mode-context'

// Consolidated palette: emerald (read-only), sky (default action),
// amber (system action), rose (destructive). Keeps the dot informative at
// small sizes without turning the canvas into a rainbow.
const EFFECT_DOT: Record<string, string> = {
  observe: 'bg-emerald-500/80',
  runtime_state: 'bg-sky-500/80',
  write: 'bg-sky-500/80',
  external_service: 'bg-sky-500/80',
  skill_runtime: 'bg-sky-500/80',
  agent_delegation: 'bg-sky-500/80',
  command: 'bg-amber-500/80',
  process_control: 'bg-amber-500/80',
  browser_control: 'bg-amber-500/80',
  device_control: 'bg-amber-500/80',
  destructive_write: 'bg-rose-500',
  unknown: 'bg-muted-foreground/50',
}

const TOOL_TRIGGER_HANDLE_CLASS = '!bg-fuchsia-500'

export const ToolNode = memo(function ToolNode({ id, data }: NodeProps<ToolFlowNode>) {
  const { tool, directConnectionHandles } = data
  const { editing } = useCanvasMode()
  // Real tools (e.g. `tool_access`, `tool_search`) can start with `tool_`;
  // only the strict `tool_<digits>` pattern is a placeholder.
  const isPlaceholder = !tool.name || /^tool_\d+$/.test(tool.name)
  const displayName = tool.name && !isPlaceholder
    ? humanizeIdentifier(tool.name)
    : editing
      ? 'Choose a tool'
      : 'Untitled tool'

  // In edit mode the user always gets both handles so they can wire the tool
  // freely; validation flags any unsupported pairings.
  const showTargetHandle = editing || directConnectionHandles.target
  const showSourceHandle = editing || directConnectionHandles.source

  return (
    <>
      {showTargetHandle ? (
        <Handle
          id={AGENT_GRAPH_TRIGGER_HANDLES.target}
          type="target"
          position={Position.Left}
          className={TOOL_TRIGGER_HANDLE_CLASS}
        />
      ) : null}
      {showSourceHandle ? (
        <Handle
          id={AGENT_GRAPH_TRIGGER_HANDLES.source}
          type="source"
          position={Position.Right}
          className={TOOL_TRIGGER_HANDLE_CLASS}
        />
      ) : null}
      <div
        className="agent-card overflow-hidden text-card-foreground"
        style={{ width: 240 }}
      >
        <div className="agent-card-tone-strip" data-tone="sky" />
        <div className="agent-card-base flex w-full items-center gap-2 px-3 py-2 text-left">
          <span
            aria-hidden="true"
            className={cn(
              'h-2 w-2 shrink-0 rounded-full ring-1 ring-background',
              EFFECT_DOT[tool.effectClass] ?? 'bg-muted-foreground/50',
            )}
          />
          <Wrench className="h-3 w-3 shrink-0 text-sky-500/80" />
          <span className="text-[12px] truncate flex-1 text-foreground/95">{displayName}</span>
        </div>
      </div>
    </>
  )
})
