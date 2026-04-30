export type View = "phases" | "agent" | "execution"

export type PhaseStatus = "complete" | "active" | "pending" | "blocked"

export interface Phase {
  id: number
  name: string
  description: string
  status: PhaseStatus
  currentStep: string | null
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
