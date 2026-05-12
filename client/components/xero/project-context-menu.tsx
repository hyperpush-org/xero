"use client"

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import { Pencil, Play, Square, TerminalSquare } from "lucide-react"

interface ProjectContextMenuProps {
  projectRunning: boolean
  startTargets: { id: string; name: string }[]
  onEditStartTargets: () => void
  onRunTarget: (targetId: string) => void
  onRunAllTargets: () => void
  onStop: () => void
  onOpenTerminal: () => void
  children: React.ReactNode
}

export function ProjectContextMenu({
  projectRunning,
  startTargets,
  onEditStartTargets,
  onRunTarget,
  onRunAllTargets,
  onStop,
  onOpenTerminal,
  children,
}: ProjectContextMenuProps) {
  const hasTargets = startTargets.length > 0
  const onlyOne = startTargets.length === 1

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-60">
        {projectRunning ? (
          <ContextMenuItem onClick={onStop} className="text-warning">
            <Square className="mr-2 h-4 w-4 fill-current" />
            Stop project
          </ContextMenuItem>
        ) : !hasTargets ? (
          <ContextMenuItem onClick={onEditStartTargets}>
            <Play className="mr-2 h-4 w-4 fill-current" />
            Configure start commands…
          </ContextMenuItem>
        ) : onlyOne ? (
          <ContextMenuItem onClick={() => onRunTarget(startTargets[0].id)}>
            <Play className="mr-2 h-4 w-4 fill-current" />
            Run {startTargets[0].name}
          </ContextMenuItem>
        ) : (
          <ContextMenuSub>
            <ContextMenuSubTrigger>
              <Play className="mr-2 h-4 w-4 fill-current" />
              Run target
            </ContextMenuSubTrigger>
            <ContextMenuSubContent className="w-48">
              {startTargets.map((target) => (
                <ContextMenuItem
                  key={target.id}
                  onClick={() => onRunTarget(target.id)}
                >
                  <span className="font-mono text-[12.5px]">{target.name}</span>
                </ContextMenuItem>
              ))}
              <ContextMenuSeparator />
              <ContextMenuItem onClick={onRunAllTargets}>Run all</ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
        )}
        <ContextMenuItem onClick={onOpenTerminal}>
          <TerminalSquare className="mr-2 h-4 w-4" />
          Open terminal sidebar
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={onEditStartTargets}>
          <Pencil className="mr-2 h-4 w-4" />
          Configure start commands…
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}
