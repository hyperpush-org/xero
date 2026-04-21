"use client"

import { useEffect, useState } from "react"
import type {
  AgentPaneView,
  OperatorActionErrorView,
  RuntimeSettingsLoadStatus,
  RuntimeSettingsSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import type {
  RuntimeSessionView,
  RuntimeSettingsDto,
  UpsertNotificationRouteRequestDto,
  UpsertRuntimeSettingsRequestDto,
} from "@/src/lib/cadence-model"
import type { PlatformVariant } from "@/components/cadence/shell"
import { Bell, Code2, KeyRound } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { cn } from "@/lib/utils"
import { DevelopmentSection } from "@/components/cadence/settings-dialog/development-section"
import { NotificationsSection } from "@/components/cadence/settings-dialog/notifications-section"
import { ProvidersSection } from "@/components/cadence/settings-dialog/providers-section"

type SettingsSection = "providers" | "notifications" | "development"

const NAV_BASE: Array<{ id: SettingsSection; label: string; icon: React.ElementType }> = [
  { id: "providers", label: "Providers", icon: KeyRound },
  { id: "notifications", label: "Notifications", icon: Bell },
]

const NAV: Array<{ id: SettingsSection; label: string; icon: React.ElementType }> = import.meta.env.DEV
  ? [...NAV_BASE, { id: "development" as SettingsSection, label: "Development", icon: Code2 }]
  : NAV_BASE

export interface SettingsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  agent: AgentPaneView | null
  runtimeSettings: RuntimeSettingsDto | null
  runtimeSettingsLoadStatus: RuntimeSettingsLoadStatus
  runtimeSettingsLoadError: OperatorActionErrorView | null
  runtimeSettingsSaveStatus: RuntimeSettingsSaveStatus
  runtimeSettingsSaveError: OperatorActionErrorView | null
  onRefreshRuntimeSettings?: (options?: { force?: boolean }) => Promise<RuntimeSettingsDto>
  onUpsertRuntimeSettings?: (request: UpsertRuntimeSettingsRequestDto) => Promise<RuntimeSettingsDto>
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  onUpsertNotificationRoute?: (req: Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt">) => Promise<unknown>
  platformOverride?: PlatformVariant | null
  onPlatformOverrideChange?: (value: PlatformVariant | null) => void
}

export function SettingsDialog({
  open,
  onOpenChange,
  agent,
  runtimeSettings,
  runtimeSettingsLoadStatus,
  runtimeSettingsLoadError,
  runtimeSettingsSaveStatus,
  runtimeSettingsSaveError,
  onRefreshRuntimeSettings,
  onUpsertRuntimeSettings,
  onStartLogin,
  onLogout,
  onUpsertNotificationRoute,
  platformOverride,
  onPlatformOverrideChange,
}: SettingsDialogProps) {
  const [section, setSection] = useState<SettingsSection>("providers")

  useEffect(() => {
    if (open) setSection("providers")
  }, [open])

  useEffect(() => {
    if (!open || !onRefreshRuntimeSettings) {
      return
    }

    void onRefreshRuntimeSettings({ force: true }).catch(() => undefined)
  }, [open, onRefreshRuntimeSettings])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex h-[min(560px,85vh)] w-[min(780px,92vw)] max-w-none flex-col gap-0 overflow-hidden p-0 sm:max-w-none"
        showCloseButton
      >
        <DialogHeader className="shrink-0 border-b border-border px-5 py-3">
          <DialogTitle className="text-sm">Settings</DialogTitle>
          <DialogDescription className="sr-only">
            Configure app-global providers, selected-project notification routes, and development options.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-h-0 flex-1">
          <nav className="flex w-44 shrink-0 flex-col gap-0.5 border-r border-border bg-sidebar/50 px-2 py-3">
            {NAV.map(({ id, label, icon: Icon }) => (
              <button
                key={id}
                type="button"
                onClick={() => setSection(id)}
                className={cn(
                  "flex items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-[12px] font-medium transition-colors",
                  section === id
                    ? "bg-secondary text-foreground"
                    : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
                )}
              >
                <Icon className="h-4 w-4 shrink-0" />
                {label}
              </button>
            ))}
          </nav>

          <div className="flex flex-1 flex-col overflow-y-auto px-6 py-5">
            {section === "providers" ? (
              <ProvidersSection
                agent={agent}
                runtimeSettings={runtimeSettings}
                runtimeSettingsLoadStatus={runtimeSettingsLoadStatus}
                runtimeSettingsLoadError={runtimeSettingsLoadError}
                runtimeSettingsSaveStatus={runtimeSettingsSaveStatus}
                runtimeSettingsSaveError={runtimeSettingsSaveError}
                onRefreshRuntimeSettings={onRefreshRuntimeSettings}
                onUpsertRuntimeSettings={onUpsertRuntimeSettings}
                onStartLogin={onStartLogin}
                onLogout={onLogout}
              />
            ) : section === "notifications" ? (
              agent ? (
                <NotificationsSection
                  agent={agent}
                  onUpsertNotificationRoute={onUpsertNotificationRoute}
                />
              ) : (
                <ProjectBoundEmptyState
                  title="Notifications require a selected project"
                  body="Provider settings are app-global, but notification routes stay project-bound so Cadence never writes cross-project delivery state into the wrong repository view."
                />
              )
            ) : section === "development" ? (
              <DevelopmentSection
                platformOverride={platformOverride}
                onPlatformOverrideChange={onPlatformOverrideChange}
              />
            ) : null}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function ProjectBoundEmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-1 items-center justify-center py-16 text-center">
      <div className="max-w-md rounded-xl border border-border bg-card px-6 py-8 shadow-sm">
        <p className="text-sm font-medium text-foreground">{title}</p>
        <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{body}</p>
      </div>
    </div>
  )
}
