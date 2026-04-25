export type View = "phases" | "agent" | "execution"

export type PhaseStatus = "complete" | "active" | "pending" | "blocked"
export type PhaseStep = "discuss" | "plan" | "execute" | "verify" | "ship"

export interface Phase {
  id: number
  name: string
  description: string
  status: PhaseStatus
  currentStep: PhaseStep | null
  stepStatuses: Record<PhaseStep, "complete" | "active" | "pending" | "skipped">
  taskCount: number
  completedTasks: number
  waveCount?: number
  completedWaves?: number
  commits?: number
  summary?: string
}

export interface Project {
  id: string
  name: string
  description: string
  milestone: string
  totalPhases: number
  completedPhases: number
  activePhase: number
  phases: Phase[]
  branch: string
  runtime: string
}
