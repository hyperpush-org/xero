"use client"

import { ReactNode } from "react"
import { 
  Workflow, 
  Bot, 
  Play, 
  Settings, 
  GitBranch,
  Keyboard
} from "lucide-react"
import { cn } from "@/lib/utils"

interface AppShellProps {
  children: ReactNode
  activeView: "workflow" | "agent" | "execution"
  onViewChange: (view: "workflow" | "agent" | "execution") => void
}

export function AppShell({ children, activeView, onViewChange }: AppShellProps) {
  return (
    <div className="h-screen flex flex-col bg-background overflow-hidden">
      {/* Title bar - mimics native window chrome */}
      <div className="h-10 bg-sidebar border-b border-border flex items-center justify-between px-4 shrink-0">
        <div className="flex items-center gap-3">
          {/* Window controls (decorative) */}
          <div className="flex items-center gap-1.5">
            <div className="w-3 h-3 rounded-full bg-destructive/60" />
            <div className="w-3 h-3 rounded-full bg-accent/60" />
            <div className="w-3 h-3 rounded-full bg-success/60" />
          </div>
          
          {/* App branding */}
          <div className="flex items-center gap-2 ml-3">
            <div className="w-5 h-5 rounded bg-primary flex items-center justify-center">
              <span className="text-xs font-bold text-primary-foreground">C</span>
            </div>
            <span className="text-sm font-semibold text-foreground">Xero</span>
          </div>
        </div>
        
        {/* Center navigation */}
        <div className="flex items-center gap-1 bg-muted rounded-md p-0.5">
          <NavButton
            icon={Workflow}
            label="Workflow"
            active={activeView === "workflow"}
            onClick={() => onViewChange("workflow")}
            shortcut="1"
          />
          <NavButton
            icon={Bot}
            label="Agent"
            active={activeView === "agent"}
            onClick={() => onViewChange("agent")}
            shortcut="2"
          />
          <NavButton
            icon={Play}
            label="Editor"
            active={activeView === "execution"}
            onClick={() => onViewChange("execution")}
            shortcut="3"
          />
        </div>
        
        {/* Right side controls */}
        <div className="flex items-center gap-2">
          <button className="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">
            <GitBranch className="w-4 h-4" />
          </button>
          <button className="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">
            <Keyboard className="w-4 h-4" />
          </button>
          <button className="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">
            <Settings className="w-4 h-4" />
          </button>
        </div>
      </div>
      
      {/* Main content */}
      <div className="flex-1 overflow-hidden">
        {children}
      </div>
    </div>
  )
}

interface NavButtonProps {
  icon: React.ElementType
  label: string
  active: boolean
  onClick: () => void
  shortcut: string
}

function NavButton({ icon: Icon, label, active, onClick, shortcut }: NavButtonProps) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex items-center gap-2 px-3 py-1.5 rounded text-sm transition-colors",
        active 
          ? "bg-secondary text-foreground" 
          : "text-muted-foreground hover:text-foreground"
      )}
    >
      <Icon className="w-4 h-4" />
      <span>{label}</span>
      <kbd className="hidden sm:inline-flex h-5 items-center justify-center rounded border border-border bg-muted px-1.5 text-[10px] font-mono text-muted-foreground">
        {shortcut}
      </kbd>
    </button>
  )
}
