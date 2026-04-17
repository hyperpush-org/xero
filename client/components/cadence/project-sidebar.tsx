"use client"

import { Project, WorkflowPhase } from "@/app/page"
import { cn } from "@/lib/utils"
import { 
  FolderOpen, 
  Plus, 
  Search,
  Circle,
  CheckCircle2,
  Clock
} from "lucide-react"

interface ProjectSidebarProps {
  projects: Project[]
  activeProject: Project
  onSelectProject: (project: Project) => void
}

const phaseColors: Record<WorkflowPhase, string> = {
  setup: "text-muted-foreground",
  requirements: "text-chart-4",
  planning: "text-accent",
  execution: "text-primary",
  verification: "text-chart-2",
  shipping: "text-success"
}

const phaseLabels: Record<WorkflowPhase, string> = {
  setup: "Setup",
  requirements: "Requirements",
  planning: "Planning",
  execution: "Execution",
  verification: "Verification",
  shipping: "Shipping"
}

export function ProjectSidebar({ projects, activeProject, onSelectProject }: ProjectSidebarProps) {
  return (
    <div className="w-64 bg-sidebar border-r border-sidebar-border flex flex-col shrink-0">
      {/* Search */}
      <div className="p-3 border-b border-sidebar-border">
        <div className="flex items-center gap-2 px-3 py-2 bg-sidebar-accent rounded-md text-sm">
          <Search className="w-4 h-4 text-muted-foreground" />
          <input 
            type="text"
            placeholder="Search projects..."
            className="bg-transparent border-none outline-none flex-1 text-foreground placeholder:text-muted-foreground text-sm"
          />
          <kbd className="text-[10px] font-mono text-muted-foreground px-1.5 py-0.5 rounded bg-muted">/</kbd>
        </div>
      </div>
      
      {/* Project list */}
      <div className="flex-1 overflow-y-auto p-2">
        <div className="flex items-center justify-between px-2 py-1.5 mb-1">
          <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Projects</span>
          <button className="p-1 rounded hover:bg-sidebar-accent text-muted-foreground hover:text-foreground transition-colors">
            <Plus className="w-3.5 h-3.5" />
          </button>
        </div>
        
        <div className="space-y-0.5">
          {projects.map((project) => (
            <ProjectItem
              key={project.id}
              project={project}
              active={project.id === activeProject.id}
              onClick={() => onSelectProject(project)}
            />
          ))}
        </div>
      </div>
      
      {/* Active session info */}
      <div className="p-3 border-t border-sidebar-border bg-sidebar">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <div className="w-2 h-2 rounded-full bg-success animate-pulse" />
          <span>Agent active</span>
          <span className="ml-auto font-mono">23 tasks</span>
        </div>
      </div>
    </div>
  )
}

interface ProjectItemProps {
  project: Project
  active: boolean
  onClick: () => void
}

function ProjectItem({ project, active, onClick }: ProjectItemProps) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "w-full text-left px-2 py-2 rounded-md transition-colors group",
        active 
          ? "bg-sidebar-accent" 
          : "hover:bg-sidebar-accent/50"
      )}
    >
      <div className="flex items-start gap-2">
        <FolderOpen className={cn(
          "w-4 h-4 mt-0.5 shrink-0",
          active ? "text-primary" : "text-muted-foreground group-hover:text-foreground"
        )} />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className={cn(
              "text-sm font-medium truncate",
              active ? "text-foreground" : "text-sidebar-foreground"
            )}>
              {project.name}
            </span>
          </div>
          <div className="flex items-center gap-2 mt-0.5">
            <span className={cn("text-xs", phaseColors[project.currentPhase])}>
              {phaseLabels[project.currentPhase]}
            </span>
            <span className="text-[10px] text-muted-foreground">{project.progress}%</span>
          </div>
        </div>
        <PhaseIndicator phase={project.currentPhase} progress={project.progress} />
      </div>
    </button>
  )
}

function PhaseIndicator({ phase, progress }: { phase: WorkflowPhase, progress: number }) {
  if (phase === "shipping" && progress === 100) {
    return <CheckCircle2 className="w-4 h-4 text-success shrink-0" />
  }
  
  if (progress > 0) {
    return (
      <div className="relative w-4 h-4 shrink-0">
        <svg className="w-4 h-4 -rotate-90">
          <circle
            cx="8"
            cy="8"
            r="6"
            stroke="currentColor"
            strokeWidth="2"
            fill="none"
            className="text-muted"
          />
          <circle
            cx="8"
            cy="8"
            r="6"
            stroke="currentColor"
            strokeWidth="2"
            fill="none"
            strokeDasharray={`${(progress / 100) * 37.7} 37.7`}
            className={phaseColors[phase]}
          />
        </svg>
      </div>
    )
  }
  
  return <Circle className="w-4 h-4 text-muted-foreground shrink-0" />
}
