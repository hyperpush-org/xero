'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { CheckCircle2, Sparkles, TriangleAlert } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import type { AgentAttachedSkillAvailabilityStatusDto } from '@/src/lib/xero-model/workflow-agents'

import type { SkillFlowNode } from '../build-agent-graph'
import { humanizeIdentifier } from '../build-agent-graph'

const STATUS_LABEL: Record<AgentAttachedSkillAvailabilityStatusDto, string> = {
  available: 'Pinned',
  unavailable: 'Unavailable',
  stale: 'Stale',
  blocked: 'Blocked',
  missing: 'Missing',
}

const STATUS_STYLE: Record<AgentAttachedSkillAvailabilityStatusDto, string> = {
  available: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
  unavailable: 'border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300',
  stale: 'border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300',
  blocked: 'border-destructive/35 bg-destructive/10 text-destructive',
  missing: 'border-destructive/35 bg-destructive/10 text-destructive',
}

function shortHash(hash: string): string {
  return hash.length > 14 ? `${hash.slice(0, 14)}...` : hash
}

export const SkillNode = memo(function SkillNode({ data }: NodeProps<SkillFlowNode>) {
  const { skill } = data
  const healthy = skill.availabilityStatus === 'available'

  return (
    <>
      <Handle type="target" position={Position.Bottom} className="!bg-rose-500" />
      <div className="agent-card overflow-hidden text-card-foreground" style={{ width: 260 }}>
        <div className="agent-card-tone-strip" data-tone="rose" />
        <div className="space-y-2 px-3 py-2.5">
          <div className="flex items-start gap-2">
            <span className="mt-px inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-rose-500/12 ring-1 ring-rose-500/30">
              <Sparkles className="h-3.5 w-3.5 text-rose-500" aria-hidden="true" />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-1.5">
                <span className="truncate text-[12.5px] font-semibold text-foreground/95">
                  {skill.name}
                </span>
                {healthy ? (
                  <CheckCircle2 className="h-3 w-3 shrink-0 text-emerald-500" aria-label="Skill pin is current" />
                ) : (
                  <TriangleAlert className="h-3 w-3 shrink-0 text-amber-500" aria-label="Skill needs attention" />
                )}
              </div>
              <p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground/80">
                {skill.skillId}
              </p>
            </div>
          </div>

          {skill.description ? (
            <p className="agent-node-detail line-clamp-2 text-[10.5px] leading-relaxed text-muted-foreground">
              {skill.description}
            </p>
          ) : null}

          <div className="agent-node-chip-row flex flex-wrap gap-1">
            <Badge variant="outline" className="h-5 px-1.5 text-[9.5px] font-medium">
              {humanizeIdentifier(skill.sourceKind)}
            </Badge>
            <Badge variant="outline" className="h-5 px-1.5 text-[9.5px] font-medium">
              {humanizeIdentifier(skill.scope)}
            </Badge>
            <Badge
              variant="outline"
              className={cn('h-5 px-1.5 text-[9.5px] font-medium', STATUS_STYLE[skill.availabilityStatus])}
            >
              {STATUS_LABEL[skill.availabilityStatus]}
            </Badge>
            {skill.required ? (
              <Badge variant="outline" className="h-5 px-1.5 text-[9.5px] font-medium">
                Required
              </Badge>
            ) : null}
          </div>

          <div className="flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground">
            <span className="shrink-0 uppercase tracking-wider text-muted-foreground/60">hash</span>
            <span className="truncate font-mono text-foreground/80">{shortHash(skill.versionHash)}</span>
            {skill.includeSupportingAssets ? (
              <span className="ml-auto shrink-0 text-muted-foreground/80">assets</span>
            ) : null}
          </div>
        </div>
      </div>
    </>
  )
})
