"use client"

import { Project, WorkflowPhase } from "@/app/page"
import { cn } from "@/lib/utils"
import { 
  Settings2, 
  FileText, 
  Map, 
  Play,
  CheckSquare,
  Rocket,
  ChevronRight,
  Clock,
  CheckCircle2,
  AlertCircle,
  ArrowRight
} from "lucide-react"

interface WorkflowPanelProps {
  project: Project
}

interface PhaseConfig {
  id: WorkflowPhase
  label: string
  icon: React.ElementType
  description: string
  tasks: { name: string; status: "complete" | "current" | "pending" }[]
}

const phases: PhaseConfig[] = [
  {
    id: "setup",
    label: "Project Setup",
    icon: Settings2,
    description: "Initialize repository, configure environment, establish project structure",
    tasks: [
      { name: "Repository initialized", status: "complete" },
      { name: "Dependencies configured", status: "complete" },
      { name: "CI/CD pipeline setup", status: "complete" }
    ]
  },
  {
    id: "requirements",
    label: "Requirements",
    icon: FileText,
    description: "Define functional requirements, acceptance criteria, and constraints",
    tasks: [
      { name: "User stories documented", status: "complete" },
      { name: "API contracts defined", status: "complete" },
      { name: "Acceptance criteria reviewed", status: "complete" }
    ]
  },
  {
    id: "planning",
    label: "Planning",
    icon: Map,
    description: "Break down work into phases, estimate effort, identify dependencies",
    tasks: [
      { name: "Architecture designed", status: "complete" },
      { name: "Task breakdown complete", status: "complete" },
      { name: "Dependencies mapped", status: "complete" }
    ]
  },
  {
    id: "execution",
    label: "Execution",
    icon: Play,
    description: "Implement features phase by phase with agent assistance",
    tasks: [
      { name: "Phase 1: Core API", status: "complete" },
      { name: "Phase 2: Authentication", status: "current" },
      { name: "Phase 3: Rate limiting", status: "pending" },
      { name: "Phase 4: Caching layer", status: "pending" }
    ]
  },
  {
    id: "verification",
    label: "Verification",
    icon: CheckSquare,
    description: "Run tests, validate requirements, review code quality",
    tasks: [
      { name: "Unit tests passing", status: "pending" },
      { name: "Integration tests", status: "pending" },
      { name: "Code review complete", status: "pending" }
    ]
  },
  {
    id: "shipping",
    label: "Shipping",
    icon: Rocket,
    description: "Deploy to production, monitor, document",
    tasks: [
      { name: "Staging deployment", status: "pending" },
      { name: "Production release", status: "pending" },
      { name: "Documentation published", status: "pending" }
    ]
  }
]

const phaseOrder: WorkflowPhase[] = ["setup", "requirements", "planning", "execution", "verification", "shipping"]

export function WorkflowPanel({ project }: WorkflowPanelProps) {
  const currentPhaseIndex = phaseOrder.indexOf(project.currentPhase)
  
  return (
    <div className="flex-1 flex flex-col bg-background overflow-hidden">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-lg font-semibold text-foreground">{project.name}</h1>
            <p className="text-sm text-muted-foreground mt-0.5">{project.description}</p>
          </div>
          <div className="flex items-center gap-3">
            <div className="text-right">
              <div className="text-2xl font-bold text-foreground">{project.progress}%</div>
              <div className="text-xs text-muted-foreground">Overall progress</div>
            </div>
            <div className="w-24 h-2 bg-muted rounded-full overflow-hidden">
              <div 
                className="h-full bg-primary rounded-full transition-all duration-500"
                style={{ width: `${project.progress}%` }}
              />
            </div>
          </div>
        </div>
      </div>
      
      {/* Phase timeline */}
      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-4xl mx-auto">
          {/* Timeline connector */}
          <div className="relative">
            {phases.map((phase, index) => {
              const phaseIndex = phaseOrder.indexOf(phase.id)
              const isComplete = phaseIndex < currentPhaseIndex
              const isCurrent = phase.id === project.currentPhase
              const isPending = phaseIndex > currentPhaseIndex
              
              return (
                <div key={phase.id} className="relative">
                  {/* Connector line */}
                  {index < phases.length - 1 && (
                    <div className={cn(
                      "absolute left-5 top-12 w-0.5 h-[calc(100%-24px)]",
                      isComplete ? "bg-primary" : "bg-border"
                    )} />
                  )}
                  
                  <PhaseCard
                    phase={phase}
                    status={isComplete ? "complete" : isCurrent ? "current" : "pending"}
                  />
                </div>
              )
            })}
          </div>
        </div>
      </div>
    </div>
  )
}

interface PhaseCardProps {
  phase: PhaseConfig
  status: "complete" | "current" | "pending"
}

function PhaseCard({ phase, status }: PhaseCardProps) {
  const Icon = phase.icon
  
  return (
    <div className={cn(
      "relative pl-14 pb-8 group",
      status === "pending" && "opacity-50"
    )}>
      {/* Phase indicator */}
      <div className={cn(
        "absolute left-0 w-10 h-10 rounded-full flex items-center justify-center border-2 transition-colors",
        status === "complete" && "bg-primary border-primary",
        status === "current" && "bg-background border-primary",
        status === "pending" && "bg-muted border-border"
      )}>
        {status === "complete" ? (
          <CheckCircle2 className="w-5 h-5 text-primary-foreground" />
        ) : (
          <Icon className={cn(
            "w-5 h-5",
            status === "current" ? "text-primary" : "text-muted-foreground"
          )} />
        )}
      </div>
      
      {/* Phase content */}
      <div className={cn(
        "bg-card border rounded-lg p-4 transition-colors",
        status === "current" && "border-primary/50 ring-1 ring-primary/20"
      )}>
        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-2">
              <h3 className="font-medium text-card-foreground">{phase.label}</h3>
              {status === "current" && (
                <span className="px-2 py-0.5 rounded-full bg-primary/20 text-primary text-xs font-medium">
                  In Progress
                </span>
              )}
              {status === "complete" && (
                <span className="px-2 py-0.5 rounded-full bg-success/20 text-success text-xs font-medium">
                  Complete
                </span>
              )}
            </div>
            <p className="text-sm text-muted-foreground mt-1">{phase.description}</p>
          </div>
          
          {status === "current" && (
            <button className="flex items-center gap-1 px-3 py-1.5 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90 transition-colors">
              <span>Continue</span>
              <ArrowRight className="w-4 h-4" />
            </button>
          )}
        </div>
        
        {/* Tasks */}
        <div className="mt-4 space-y-2">
          {phase.tasks.map((task, idx) => (
            <div key={idx} className="flex items-center gap-2 text-sm">
              {task.status === "complete" && (
                <CheckCircle2 className="w-4 h-4 text-success shrink-0" />
              )}
              {task.status === "current" && (
                <div className="w-4 h-4 rounded-full border-2 border-primary shrink-0 flex items-center justify-center">
                  <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" />
                </div>
              )}
              {task.status === "pending" && (
                <div className="w-4 h-4 rounded-full border border-muted-foreground/30 shrink-0" />
              )}
              <span className={cn(
                task.status === "complete" && "text-muted-foreground",
                task.status === "current" && "text-foreground font-medium",
                task.status === "pending" && "text-muted-foreground"
              )}>
                {task.name}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
