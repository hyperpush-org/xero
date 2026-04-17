"use client"

import { isTauri } from "@tauri-apps/api/core"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { ChevronDown, Settings } from "lucide-react"
import type { View } from "./data"

interface CadenceShellProps {
  activeView: View
  onViewChange: (view: View) => void
  children: React.ReactNode
  projectName?: string
  onOpenSettings?: () => void
}

type WindowAction = "close" | "minimize" | "toggle-maximize"

const NAV_ITEMS: { id: View; label: string }[] = [
  { id: "phases", label: "Workflow" },
  { id: "agent", label: "Agent" },
  { id: "execution", label: "Execution" },
]

export function CadenceShell({ activeView, onViewChange, children, projectName, onOpenSettings }: CadenceShellProps) {
  const desktopRuntime = isTauri()

  const handleWindowAction = async (action: WindowAction) => {
    if (!desktopRuntime) {
      return
    }

    const appWindow = getCurrentWindow()

    if (action === "close") {
      await appWindow.close()
      return
    }

    if (action === "minimize") {
      await appWindow.minimize()
      return
    }

    await appWindow.toggleMaximize()
  }

  const handleTitlebarPointerDown = async (event: React.MouseEvent<HTMLElement>) => {
    if (!desktopRuntime || event.button !== 0) {
      return
    }

    const target = event.target instanceof HTMLElement ? event.target : null

    if (target?.closest('[data-titlebar-no-drag="true"]')) {
      return
    }

    const appWindow = getCurrentWindow()

    if (event.detail === 2) {
      await appWindow.toggleMaximize()
      return
    }

    await appWindow.startDragging()
  }

  return (
    <div className="cadence-window-shell flex h-screen flex-col overflow-hidden bg-background text-foreground select-none">
      {/* Title bar — drag on the shell surface, interactive children opt out */}
      <header
        className="titlebar-drag-region flex h-11 items-center border-b border-border bg-sidebar px-3 shrink-0"
        data-tauri-drag-region
        onMouseDown={(event) => void handleTitlebarPointerDown(event)}
      >
        {/* Window controls */}
        <div className="titlebar-no-drag mr-5 flex items-center gap-2" data-titlebar-no-drag="true">
          <button
            aria-label="Close window"
            className="h-3 w-3 rounded-full bg-[#ec6a5e] transition-opacity hover:opacity-85 disabled:opacity-45"
            disabled={!desktopRuntime}
            onClick={() => void handleWindowAction("close")}
            type="button"
          />
          <button
            aria-label="Minimize window"
            className="h-3 w-3 rounded-full bg-[#f5bf4f] transition-opacity hover:opacity-85 disabled:opacity-45"
            disabled={!desktopRuntime}
            onClick={() => void handleWindowAction("minimize")}
            type="button"
          />
          <button
            aria-label="Toggle maximize window"
            className="h-3 w-3 rounded-full bg-[#61c554] transition-opacity hover:opacity-85 disabled:opacity-45"
            disabled={!desktopRuntime}
            onClick={() => void handleWindowAction("toggle-maximize")}
            type="button"
          />
        </div>

        {/* Logo */}
        <div className="flex items-center gap-2">
          <svg className="text-primary" fill="none" height="16" viewBox="0 0 24 24" width="16">
            <path d="M4 4h6v6H4V4Z" fill="currentColor" />
            <path d="M14 4h6v6h-6V4Z" fill="currentColor" fillOpacity="0.25" />
            <path d="M4 14h6v6H4v-6Z" fill="currentColor" fillOpacity="0.25" />
            <path d="M14 14h6v6h-6v-6Z" fill="currentColor" />
          </svg>
          <span className="text-[13px] font-semibold tracking-[-0.01em] text-foreground/90">Cadence</span>
        </div>

        {/* Divider */}
        <div className="mx-4 h-4 w-px bg-border" />

        {/* Navigation */}
        <nav className="titlebar-no-drag flex items-center gap-1" data-titlebar-no-drag="true">
          {NAV_ITEMS.map(({ id, label }) => (
            <button
              key={id}
              className={`
                rounded-md px-3 py-1.5 text-[13px] font-medium transition-colors
                ${
                  activeView === id
                    ? "bg-secondary text-foreground"
                    : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
                }
              `}
              onClick={() => onViewChange(id)}
              type="button"
            >
              {label}
            </button>
          ))}
        </nav>

        {/* Right side */}
        <div className="titlebar-no-drag ml-auto flex items-center gap-2" data-titlebar-no-drag="true">
          {projectName && (
            <button
              className="flex items-center gap-1.5 rounded-md px-2 py-1 text-[12px] text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
              type="button"
            >
              <span className="font-mono">{projectName}</span>
              <ChevronDown className="h-3 w-3" />
            </button>
          )}
          <button
            className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
            onClick={onOpenSettings}
            type="button"
          >
            <Settings className="h-4 w-4" />
          </button>
        </div>
      </header>

      {/* Main content */}
      <main className="flex min-h-0 flex-1">{children}</main>
    </div>
  )
}
