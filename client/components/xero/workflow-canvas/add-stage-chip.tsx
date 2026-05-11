'use client'

import { useMemo, useState } from 'react'
import { GitBranch, Layers, Plus } from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'

import type { AgentToolSummaryDto } from '@/src/lib/xero-model/workflow-agents'
import {
  STAGE_PRESETS,
  applyStagePreset,
  type StagePreset,
} from '@/src/lib/xero-model/stage-presets'
import type { CustomAgentWorkflowPhaseDto } from '@/src/lib/xero-model/agent-definition'

interface AddStageChipProps {
  agentTools: ReadonlyArray<AgentToolSummaryDto>
  onAddBlankStage: () => void
  onApplyPreset: (resolved: {
    startPhaseId: string
    phases: CustomAgentWorkflowPhaseDto[]
  }) => void
}

// Empty-state affordance for stages. Only mounted when the agent currently
// has zero stages — once the user adds a stage (here or by dragging from the
// agent header's workflow handle), the chip unmounts and the STAGES lane
// takes over.
//
// Click the chip to open a small popover offering a blank stage or one of
// the canonical presets ("Research → Plan → Execute", "Read-only audit").
// Presets resolve their `allowedTools` against the agent's currently-wired
// tools so the result reflects what the agent actually has available.
export function AddStageChip({
  agentTools,
  onAddBlankStage,
  onApplyPreset,
}: AddStageChipProps) {
  const [open, setOpen] = useState(false)
  const presets = useMemo(() => STAGE_PRESETS, [])

  const handlePreset = (preset: StagePreset) => {
    const resolved = applyStagePreset(preset, agentTools)
    onApplyPreset(resolved)
    setOpen(false)
  }

  const handleBlank = () => {
    onAddBlankStage()
    setOpen(false)
  }

  return (
    <div className="agent-visualization__add-stage-chip pointer-events-auto absolute bottom-2.5 left-1/2 z-10 -translate-x-1/2">
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 gap-1.5 rounded-full border-amber-500/40 bg-card/90 px-2.5 text-[11px] font-medium text-foreground/85 shadow-[0_4px_14px_-6px_rgba(0,0,0,0.45)] backdrop-blur-md hover:border-amber-500/60 hover:text-foreground"
            title="Restrict which tools the agent can call at each step of a run."
            data-testid="add-stage-chip"
          >
            <Plus className="h-3 w-3" aria-hidden="true" />
            <span>Add stage</span>
          </Button>
        </PopoverTrigger>
        <PopoverContent align="center" sideOffset={8} className="w-72 p-2">
          <div className="px-2 py-1.5">
            <p className="text-[11px] font-semibold text-foreground">Add a stage</p>
            <p className="mt-0.5 text-[10.5px] leading-snug text-muted-foreground">
              Stages restrict which tools the agent can call at each step of a single
              run.
            </p>
          </div>
          <div className="mt-1 flex flex-col gap-0.5">
            <button
              type="button"
              onClick={handleBlank}
              className="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left text-[11px] hover:bg-accent"
              data-testid="add-stage-chip-blank"
            >
              <GitBranch
                className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-500"
                aria-hidden="true"
              />
              <span className="flex-1">
                <span className="font-medium text-foreground">Blank stage</span>
                <span className="block text-[10px] leading-snug text-muted-foreground">
                  Start with one empty stage. Configure tools and gates yourself.
                </span>
              </span>
            </button>
            {presets.map((preset) => (
              <button
                key={preset.id}
                type="button"
                onClick={() => handlePreset(preset)}
                className="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left text-[11px] hover:bg-accent"
                data-testid={`add-stage-chip-preset-${preset.id}`}
              >
                <Layers
                  className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-500"
                  aria-hidden="true"
                />
                <span className="flex-1">
                  <span className="font-medium text-foreground">{preset.title}</span>
                  <span className="block text-[10px] leading-snug text-muted-foreground">
                    {preset.description}
                  </span>
                </span>
              </button>
            ))}
          </div>
        </PopoverContent>
      </Popover>
    </div>
  )
}
