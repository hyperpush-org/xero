'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import {
  Bot,
  Database,
  FileText,
  GitMerge,
  Layers,
  Lock,
  Wrench,
} from 'lucide-react'

import { cn } from '@/lib/utils'
import {
  getAgentDefinitionBaseCapabilityLabel,
  getAgentDefinitionScopeLabel,
} from '@/src/lib/xero-model/agent-definition'
import { getRuntimeRunApprovalModeLabel } from '@/src/lib/xero-model/runtime'

import {
  AGENT_GRAPH_HEADER_HANDLES,
  AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS,
  type AgentHeaderFlowNode,
} from '../build-agent-graph'

export const AgentHeaderNode = memo(function AgentHeaderNode({ data }: NodeProps<AgentHeaderFlowNode>) {
  const { header, summary } = data

  return (
    <>
      <Handle
        id={AGENT_GRAPH_HEADER_HANDLES.prompt}
        type="source"
        position={Position.Top}
        className="!bg-amber-500"
      />
      {/* Two source handles share the right edge (tool, db). React Flow
          centers them at 50% by default, so they overlap visually; stagger
          them vertically so both stay grabbable in editing mode. */}
      <Handle
        id={AGENT_GRAPH_HEADER_HANDLES.tool}
        type="source"
        position={Position.Right}
        style={{ top: `${AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS.tool * 100}%` }}
        className="!bg-sky-500"
      />
      <Handle
        id={AGENT_GRAPH_HEADER_HANDLES.db}
        type="source"
        position={Position.Right}
        style={{ top: `${AGENT_GRAPH_HEADER_RIGHT_HANDLE_RATIOS.db * 100}%` }}
        className="!bg-emerald-500"
      />
      <Handle
        id={AGENT_GRAPH_HEADER_HANDLES.output}
        type="source"
        position={Position.Bottom}
        className="!bg-foreground"
      />
      <Handle
        id={AGENT_GRAPH_HEADER_HANDLES.consumed}
        type="target"
        position={Position.Left}
        className="!bg-teal-500"
      />
      <div
        className="agent-graph-lane-label agent-graph-lane-label--agent agent-card-header__label"
        style={{ width: 320 }}
        aria-hidden="true"
      >
        <span className="agent-graph-lane-label__bar" />
        <span className="agent-graph-lane-label__text">Agent</span>
      </div>
      <div
        className="agent-card agent-card-header overflow-hidden text-card-foreground"
        style={{ width: 320 }}
      >
        <div className="px-3.5 pt-3 pb-3 border-b border-border/60">
          <div className="flex items-start gap-2.5">
            <span className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-primary/15 ring-1 ring-primary/30 mt-px">
              <Bot className="h-3.5 w-3.5 text-primary" />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-1.5">
                <span className="font-semibold text-[14px] tracking-tight truncate text-foreground">
                  {header.displayName}
                </span>
                {header.scope === 'built_in' ? (
                  <Lock
                    aria-label="System agent"
                    className="h-3 w-3 shrink-0 text-primary/70"
                  />
                ) : null}
                {header.shortLabel && header.shortLabel !== header.displayName ? (
                  <span className="ml-auto text-muted-foreground/70 text-[10px] font-mono truncate">
                    {header.shortLabel}
                  </span>
                ) : null}
              </div>
              <div className="mt-0.5 flex items-center gap-1 text-[10px] text-muted-foreground/85">
                <span>{getAgentDefinitionScopeLabel(header.scope)}</span>
                <MetaSep />
                <span>{getAgentDefinitionBaseCapabilityLabel(header.baseCapabilityProfile)}</span>
                <MetaSep />
                <span className="capitalize">{header.lifecycleState}</span>
              </div>
            </div>
          </div>
          <p className="agent-node-detail mt-2.5 text-[11px] text-muted-foreground/90 leading-relaxed">
            {header.description}
          </p>
        </div>
        <div className="agent-node-chip-row px-3.5 py-2.5 grid grid-cols-2 gap-x-4 gap-y-1.5 text-[10px] text-muted-foreground border-b border-border/60 bg-foreground/[0.04]">
          <SummaryChip icon={FileText} count={summary.prompts} label="prompts" />
          <SummaryChip icon={Wrench} count={summary.tools} label="tools" />
          <SummaryChip icon={Database} count={summary.dbTables} label="touchpoints" />
          <SummaryChip icon={Layers} count={summary.outputSections} label="sections" />
          {summary.consumes > 0 ? (
            <SummaryChip icon={GitMerge} count={summary.consumes} label="consumes" />
          ) : null}
        </div>
        <div className="agent-node-chip-row px-3.5 py-2.5 flex items-center flex-wrap gap-x-4 gap-y-1.5 text-[10px]">
          <GateMarker
            on
            label="Approval"
            value={getRuntimeRunApprovalModeLabel(header.defaultApprovalMode)}
          />
          <GateMarker on={header.allowPlanGate} label="Plan" />
          <GateMarker on={header.allowVerificationGate} label="Verify" />
          {header.allowAutoCompact ? <GateMarker on label="Auto-compact" /> : null}
        </div>
      </div>
    </>
  )
})

interface SummaryChipProps {
  icon: typeof FileText
  count: number
  label: string
}

function SummaryChip({ icon: Icon, count, label }: SummaryChipProps) {
  return (
    <span className="inline-flex items-center gap-1 tabular-nums">
      <Icon className="h-3 w-3 shrink-0 text-muted-foreground" />
      <span className="text-foreground font-medium">{count}</span>
      <span className="text-muted-foreground/75">{label}</span>
    </span>
  )
}

function MetaSep() {
  return (
    <span aria-hidden="true" className="text-muted-foreground/40">
      ·
    </span>
  )
}

interface GateMarkerProps {
  on: boolean
  label: string
  value?: string
}

// Flat dot+label gate display. Active gates use foreground text + emerald
// dot; inactive ones fade to muted text + grey dot. No filled pill — the
// header surface already separates this row from the canvas background, so
// a colored capsule per gate is redundant noise.
function GateMarker({ on, label, value }: GateMarkerProps) {
  return (
    <span className="inline-flex items-center gap-1 leading-none">
      <span
        aria-hidden="true"
        className={cn(
          'h-1.5 w-1.5 rounded-full',
          on ? 'bg-emerald-500' : 'bg-muted-foreground/30',
        )}
      />
      <span className={cn('font-medium', on ? 'text-foreground/90' : 'text-muted-foreground/60')}>
        {label}
      </span>
      {value ? (
        <span className="text-muted-foreground/80">: {value}</span>
      ) : null}
    </span>
  )
}
