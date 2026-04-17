"use client"

import { useState } from "react"
import type { Project, Phase, PhaseStep } from "./data"
import {
  Check, Loader2, Circle, ChevronRight,
  PanelRightClose, PanelRight
} from "lucide-react"

interface WorkflowSidebarProps {
  project: Project
  collapsed: boolean
  onToggleCollapse: () => void
}

const STEPS: PhaseStep[] = ["discuss", "plan", "execute", "verify", "ship"]
const STEP_LABELS: Record<PhaseStep, string> = {
  discuss: "D",
  plan: "P",
  execute: "E",
  verify: "V",
  ship: "S",
}

function PhaseRow({ phase, isActive }: { phase: Phase; isActive: boolean }) {
  const isComplete = phase.status === "complete"
  const isRunning = phase.status === "active"
  const isPending = phase.status === "pending" || phase.status === "blocked"

  return (
    <div
      className={`
        group px-3 py-2 cursor-pointer transition-colors
        ${isActive ? "bg-[#232323]" : "hover:bg-[#1a1a1a]"}
        ${isPending ? "opacity-40" : ""}
      `}
    >
      {/* Phase header */}
      <div className="flex items-center gap-2.5 mb-1.5">
        {/* Status icon */}
        <div className="shrink-0 w-4 flex justify-center">
          {isComplete && <Check className="w-3.5 h-3.5 text-[#5a9a6a]" strokeWidth={2} />}
          {isRunning && <Loader2 className="w-3.5 h-3.5 text-[#888] animate-spin" />}
          {isPending && <Circle className="w-3 h-3 text-[#444]" strokeWidth={1.5} />}
        </div>

        {/* Phase info */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="text-[10px] font-mono text-[#666]">P{phase.id}</span>
            <span className={`text-[12px] truncate ${isActive ? "text-[#e0e0e0] font-medium" : "text-[#999]"}`}>
              {phase.name}
            </span>
          </div>
        </div>

        {/* Chevron for active */}
        {isActive && (
          <ChevronRight className="w-3 h-3 text-[#555] shrink-0" />
        )}
      </div>

      {/* Step indicators */}
      <div className="flex items-center gap-0.5 ml-6">
        {STEPS.map((step) => {
          const status = phase.stepStatuses[step]
          const isStepComplete = status === "complete"
          const isStepActive = status === "active"
          const isStepPending = status === "pending" || status === "skipped"

          return (
            <div
              key={step}
              className={`
                w-5 h-4 flex items-center justify-center text-[8px] font-medium rounded-sm
                ${isStepComplete ? "bg-[#2a3a2e] text-[#5a9a6a]" : ""}
                ${isStepActive ? "bg-[#333] text-[#ccc] ring-1 ring-[#555]" : ""}
                ${isStepPending ? "bg-[#1a1a1a] text-[#444]" : ""}
              `}
              title={step.charAt(0).toUpperCase() + step.slice(1)}
            >
              {STEP_LABELS[step]}
            </div>
          )
        })}
      </div>
    </div>
  )
}

export function WorkflowSidebar({ project, collapsed, onToggleCollapse }: WorkflowSidebarProps) {
  if (collapsed) {
    return (
      <aside className="w-10 shrink-0 border-l border-[#252525] bg-[#141414] flex flex-col">
        <button
          onClick={onToggleCollapse}
          className="p-2.5 text-[#666] hover:text-[#999] hover:bg-[#1a1a1a] transition-colors"
          title="Expand sidebar"
        >
          <PanelRight className="w-4 h-4" />
        </button>
        
        {/* Mini phase indicators */}
        <div className="flex-1 py-2 space-y-1">
          {project.phases.map((phase) => {
            const isActive = phase.status === "active"
            const isComplete = phase.status === "complete"
            const isPending = phase.status === "pending"

            return (
              <div
                key={phase.id}
                className="flex justify-center"
                title={`P${phase.id}: ${phase.name}`}
              >
                <div
                  className={`
                    w-1.5 h-1.5 rounded-full
                    ${isComplete ? "bg-[#5a9a6a]" : ""}
                    ${isActive ? "bg-[#888] ring-2 ring-[#888]/30" : ""}
                    ${isPending ? "bg-[#333]" : ""}
                  `}
                />
              </div>
            )
          })}
        </div>
      </aside>
    )
  }

  const overallProgress = Math.round((project.completedPhases / project.totalPhases) * 100)

  return (
    <aside className="w-56 shrink-0 border-l border-[#252525] bg-[#141414] flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2.5 border-b border-[#252525]">
        <span className="text-[10px] font-medium uppercase tracking-wider text-[#666]">
          Workflow
        </span>
        <button
          onClick={onToggleCollapse}
          className="p-1 text-[#555] hover:text-[#888] hover:bg-[#1a1a1a] rounded transition-colors"
          title="Collapse sidebar"
        >
          <PanelRightClose className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Milestone */}
      <div className="px-3 py-2.5 border-b border-[#252525]">
        <p className="text-[10px] text-[#555] mb-1">Milestone</p>
        <p className="text-[11px] text-[#bbb] font-medium truncate">{project.milestone}</p>
        
        {/* Progress bar */}
        <div className="flex items-center gap-2 mt-2">
          <div className="flex-1 h-1 rounded-full bg-[#252525] overflow-hidden">
            <div
              className="h-full rounded-full bg-[#5a9a6a] transition-all duration-500"
              style={{ width: `${overallProgress}%` }}
            />
          </div>
          <span className="text-[10px] font-mono text-[#666]">{overallProgress}%</span>
        </div>
      </div>

      {/* Phase list */}
      <div className="flex-1 overflow-y-auto scrollbar-thin py-1">
        {project.phases.map((phase) => (
          <PhaseRow
            key={phase.id}
            phase={phase}
            isActive={phase.status === "active"}
          />
        ))}
      </div>

      {/* Footer stats */}
      <div className="px-3 py-2.5 border-t border-[#252525] space-y-1.5">
        <div className="flex justify-between text-[10px]">
          <span className="text-[#555]">Phases</span>
          <span className="font-mono text-[#777]">
            {project.completedPhases}/{project.totalPhases}
          </span>
        </div>
        <div className="flex justify-between text-[10px]">
          <span className="text-[#555]">Branch</span>
          <span className="font-mono text-[#777] truncate max-w-[100px]">
            {project.branch.split("/").pop()}
          </span>
        </div>
        <div className="flex justify-between text-[10px]">
          <span className="text-[#555]">Runtime</span>
          <span className="font-mono text-[#777]">{project.runtime}</span>
        </div>
      </div>
    </aside>
  )
}
