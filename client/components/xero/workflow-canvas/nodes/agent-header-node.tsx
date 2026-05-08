'use client'

import { memo, useEffect, useState } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import {
  Bot,
  ChevronDown,
  ChevronRight,
  Database,
  FileText,
  GitMerge,
  Layers,
  Lock,
  Wrench,
} from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import {
  getAgentDefinitionBaseCapabilityLabel,
  getAgentDefinitionScopeLabel,
} from '@/src/lib/xero-model/agent-definition'
import { getRuntimeRunApprovalModeLabel } from '@/src/lib/xero-model/runtime'

import { AGENT_GRAPH_HEADER_HANDLES, type AgentHeaderFlowNode } from '../build-agent-graph'
import { useAgentCanvasExpansion } from '../expansion-context'

export const AgentHeaderNode = memo(function AgentHeaderNode({ id, data }: NodeProps<AgentHeaderFlowNode>) {
  const { header, summary } = data
  const [showPurpose, setShowPurpose] = useState(false)
  const hasPurpose = Boolean(header.taskPurpose)
  const expanded = hasPurpose && showPurpose
  const { locked, setExpanded: reportExpanded } = useAgentCanvasExpansion()

  useEffect(() => {
    reportExpanded(id, expanded)
    return () => {
      reportExpanded(id, false)
    }
  }, [id, expanded, reportExpanded])

  return (
    <>
      <Handle
        id={AGENT_GRAPH_HEADER_HANDLES.prompt}
        type="source"
        position={Position.Top}
        className="!bg-amber-500 !w-2 !h-2"
      />
      <Handle
        id={AGENT_GRAPH_HEADER_HANDLES.tool}
        type="source"
        position={Position.Right}
        className="!bg-sky-500 !w-2 !h-2"
      />
      <Handle
        id={AGENT_GRAPH_HEADER_HANDLES.db}
        type="source"
        position={Position.Right}
        className="!bg-emerald-500 !w-2 !h-2"
      />
      <Handle
        id={AGENT_GRAPH_HEADER_HANDLES.output}
        type="source"
        position={Position.Bottom}
        className="!bg-foreground !w-2 !h-2"
      />
      <Handle
        id={AGENT_GRAPH_HEADER_HANDLES.consumed}
        type="target"
        position={Position.Left}
        className="!bg-teal-500 !w-2 !h-2"
      />
      <div
        className={cn(
          'agent-card agent-card-header overflow-hidden text-card-foreground',
          expanded && 'is-card-expanded',
        )}
        style={{ width: 300 }}
      >
        <div className="px-3 pt-2.5 pb-2 border-b border-border/40">
          <div className="flex items-center gap-2">
            <span className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-primary/15 ring-1 ring-primary/30">
              <Bot className="h-3.5 w-3.5 text-primary" />
            </span>
            <span className="font-semibold text-[13px] tracking-tight truncate">
              {header.displayName}
            </span>
            {header.shortLabel && header.shortLabel !== header.displayName ? (
              <span className="text-muted-foreground/80 text-[10px] font-mono truncate">
                {header.shortLabel}
              </span>
            ) : null}
            {header.scope === 'built_in' ? (
              <span
                className="ml-auto inline-flex shrink-0 items-center gap-1 rounded-full border border-primary/35 bg-primary/10 px-1.5 py-0.5 text-[8.5px] font-semibold uppercase tracking-wider text-primary"
              >
                <Lock aria-hidden="true" className="h-2.5 w-2.5" />
                <span>system</span>
              </span>
            ) : null}
          </div>
          <p className="agent-node-detail mt-1.5 text-[11px] text-muted-foreground leading-relaxed line-clamp-2">
            {header.description}
          </p>
          <div className="agent-node-chip-row mt-2 flex flex-wrap gap-1">
            <Badge variant="outline" className="text-[9px] px-1.5 py-0 font-medium">
              {getAgentDefinitionScopeLabel(header.scope)}
            </Badge>
            <Badge variant="secondary" className="text-[9px] px-1.5 py-0 font-medium">
              {getAgentDefinitionBaseCapabilityLabel(header.baseCapabilityProfile)}
            </Badge>
            <Badge variant="outline" className="text-[9px] px-1.5 py-0 capitalize font-medium">
              {header.lifecycleState}
            </Badge>
          </div>
        </div>
        <div className="agent-node-chip-row px-3 py-1.5 flex items-center gap-x-3 gap-y-1 flex-wrap text-[10px] text-muted-foreground border-b border-border/40 bg-muted/15">
          <SummaryChip icon={FileText} count={summary.prompts} label="prompts" tone="amber" />
          <SummaryChip icon={Wrench} count={summary.tools} label="tools" tone="sky" />
          <SummaryChip icon={Database} count={summary.dbTables} label="touchpoints" tone="emerald" />
          <SummaryChip icon={Layers} count={summary.outputSections} label="sections" tone="foreground" />
          {summary.consumes > 0 ? (
            <SummaryChip icon={GitMerge} count={summary.consumes} label="consumes" tone="teal" />
          ) : null}
        </div>
        <div className="agent-node-chip-row px-3 py-1.5 flex items-center gap-1.5 flex-wrap text-[10px]">
          <GatePill
            on={true}
            label="Approval"
            value={getRuntimeRunApprovalModeLabel(header.defaultApprovalMode)}
          />
          <GatePill on={header.allowPlanGate} label="Plan" />
          <GatePill on={header.allowVerificationGate} label="Verify" />
          {header.allowAutoCompact ? (
            <GatePill on={true} label="Auto-compact" subtle />
          ) : null}
        </div>
        {hasPurpose ? (
          <button
            type="button"
            onClick={() => {
              if (locked) return
              setShowPurpose((v) => !v)
            }}
            disabled={locked}
            className="flex w-full items-center gap-1.5 px-3 py-1.5 text-[10px] font-medium text-muted-foreground hover:bg-muted/40 hover:text-foreground transition-colors border-t border-border/40"
          >
            {expanded ? (
              <ChevronDown className="agent-node-chevron h-3 w-3" />
            ) : (
              <ChevronRight className="agent-node-chevron h-3 w-3" />
            )}
            {expanded ? 'Hide task purpose' : 'Task purpose'}
          </button>
        ) : null}
        {hasPurpose ? (
          <div className={cn('agent-card-body-wrapper', expanded && 'is-open')}>
            <div className="agent-card-body">
              <div className="px-3 pt-2 pb-2.5 text-[10.5px] text-muted-foreground leading-relaxed border-t border-border/40 bg-muted/10">
                {header.taskPurpose}
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </>
  )
})

interface SummaryChipProps {
  icon: typeof FileText
  count: number
  label: string
  tone: 'amber' | 'sky' | 'emerald' | 'foreground' | 'teal'
}

const SUMMARY_TONE: Record<SummaryChipProps['tone'], string> = {
  amber: 'text-amber-500',
  sky: 'text-sky-500',
  emerald: 'text-emerald-500',
  foreground: 'text-foreground/70',
  teal: 'text-teal-500',
}

function SummaryChip({ icon: Icon, count, label, tone }: SummaryChipProps) {
  return (
    <span className="inline-flex items-center gap-1 tabular-nums">
      <Icon className={cn('h-3 w-3 shrink-0', SUMMARY_TONE[tone])} />
      <span className="font-mono text-foreground font-medium">{count}</span>
      <span className="text-muted-foreground/75">{label}</span>
    </span>
  )
}

interface GatePillProps {
  on: boolean
  label: string
  value?: string
  subtle?: boolean
}

function GatePill({ on, label, value, subtle }: GatePillProps) {
  const tone = subtle
    ? 'border-border/50 bg-muted/30 text-muted-foreground'
    : on
      ? 'border-emerald-500/35 bg-emerald-500/12 text-emerald-700 dark:text-emerald-300'
      : 'border-border/50 bg-muted/20 text-muted-foreground/70'
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 leading-none',
        tone,
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          'h-1.5 w-1.5 rounded-full',
          subtle
            ? 'bg-muted-foreground/50'
            : on
              ? 'bg-emerald-500'
              : 'bg-muted-foreground/40',
        )}
      />
      <span className="font-medium">{label}</span>
      {value ? <span className="font-mono opacity-80">{value}</span> : null}
    </span>
  )
}
