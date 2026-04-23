"use client"

import { useEffect, useState } from "react"
import type {
  AgentPaneView,
  OperatorActionErrorView,
  ProviderModelCatalogLoadStatus,
  ProviderProfilesLoadStatus,
  ProviderProfilesSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import type {
  ProviderModelCatalogDto,
  ProviderProfilesDto,
  RuntimeSessionView,
  UpsertNotificationRouteRequestDto,
  UpsertProviderProfileRequestDto,
} from "@/src/lib/cadence-model"
import type { PlatformVariant } from "@/components/cadence/shell"
import { Bell, Code2, Globe, KeyRound, Palette } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { cn } from "@/lib/utils"
import { BrowserSection } from "@/components/cadence/settings-dialog/browser-section"
import { DevelopmentSection } from "@/components/cadence/settings-dialog/development-section"
import { NotificationsSection } from "@/components/cadence/settings-dialog/notifications-section"
import { ProvidersSection } from "@/components/cadence/settings-dialog/providers-section"
import { ThemesSection } from "@/components/cadence/settings-dialog/themes-section"

type SettingsSection = "providers" | "notifications" | "browser" | "themes" | "development"

const NAV_BASE: Array<{ id: SettingsSection; label: string; icon: React.ElementType }> = [
  { id: "providers", label: "Providers", icon: KeyRound },
  { id: "notifications", label: "Notifications", icon: Bell },
  { id: "browser", label: "Browser", icon: Globe },
  { id: "themes", label: "Themes", icon: Palette },
]

const NAV: Array<{ id: SettingsSection; label: string; icon: React.ElementType }> = import.meta.env.DEV
  ? [...NAV_BASE, { id: "development" as SettingsSection, label: "Development", icon: Code2 }]
  : NAV_BASE

export interface SettingsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  agent: AgentPaneView | null
  providerProfiles: ProviderProfilesDto | null
  providerProfilesLoadStatus: ProviderProfilesLoadStatus
  providerProfilesLoadError: OperatorActionErrorView | null
  providerProfilesSaveStatus: ProviderProfilesSaveStatus
  providerProfilesSaveError: OperatorActionErrorView | null
  providerModelCatalogs: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors: Record<string, OperatorActionErrorView | null>
  onRefreshProviderProfiles?: (options?: { force?: boolean }) => Promise<ProviderProfilesDto>
  onRefreshProviderModelCatalog?: (
    profileId: string,
    options?: { force?: boolean },
  ) => Promise<ProviderModelCatalogDto>
  onUpsertProviderProfile?: (request: UpsertProviderProfileRequestDto) => Promise<ProviderProfilesDto>
  onSetActiveProviderProfile?: (profileId: string) => Promise<ProviderProfilesDto>
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  onUpsertNotificationRoute?: (req: Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt">) => Promise<unknown>
  platformOverride?: PlatformVariant | null
  onPlatformOverrideChange?: (value: PlatformVariant | null) => void
  onStartOnboarding?: () => void
}

export function SettingsDialog({
  open,
  onOpenChange,
  agent,
  providerProfiles,
  providerProfilesLoadStatus,
  providerProfilesLoadError,
  providerProfilesSaveStatus,
  providerProfilesSaveError,
  providerModelCatalogs,
  providerModelCatalogLoadStatuses,
  providerModelCatalogLoadErrors,
  onRefreshProviderProfiles,
  onRefreshProviderModelCatalog,
  onUpsertProviderProfile,
  onSetActiveProviderProfile,
  onStartLogin,
  onLogout,
  onUpsertNotificationRoute,
  platformOverride,
  onPlatformOverrideChange,
  onStartOnboarding,
}: SettingsDialogProps) {
  const [section, setSection] = useState<SettingsSection>("providers")

  useEffect(() => {
    if (open) setSection("providers")
  }, [open])

  useEffect(() => {
    if (!open || !onRefreshProviderProfiles) {
      return
    }

    void onRefreshProviderProfiles({ force: true }).catch(() => undefined)
  }, [open, onRefreshProviderProfiles])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex h-[min(640px,88vh)] w-[min(880px,94vw)] max-w-none flex-col gap-0 overflow-hidden border-border/70 p-0 sm:max-w-none"
        showCloseButton
      >
        <DialogHeader className="shrink-0 border-b border-border/70 px-6 py-4">
          <DialogTitle className="text-[14px] font-semibold tracking-tight">Settings</DialogTitle>
          <DialogDescription className="sr-only">
            Configure providers, notification routes, and development options.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-h-0 flex-1">
          <nav className="flex w-48 shrink-0 flex-col border-r border-border/70 bg-sidebar">
            <div className="px-3.5 pt-3.5 pb-2">
              <span className="text-[11.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
                Settings
              </span>
            </div>
            <div className="flex flex-col">
              {NAV.map(({ id, label, icon: Icon }) => {
                const active = section === id
                return (
                  <button
                    key={id}
                    type="button"
                    aria-label={label}
                    onClick={() => setSection(id)}
                    className={cn(
                      "group flex items-center gap-3 px-3.5 py-2.5 text-left transition-colors duration-150",
                      active
                        ? "bg-primary/[0.08] text-foreground"
                        : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
                    )}
                  >
                    <Icon
                      className={cn(
                        "h-4 w-4 shrink-0",
                        active ? "text-primary" : "text-muted-foreground group-hover:text-foreground",
                      )}
                    />
                    <span className="text-[13.5px] font-medium leading-tight">{label}</span>
                  </button>
                )
              })}
            </div>
          </nav>

          <div className="flex flex-1 flex-col overflow-y-auto scrollbar-thin">
            <div
              key={section}
              className="flex flex-1 flex-col px-7 py-6 animate-in fade-in-0 slide-in-from-right-2 duration-200 ease-out"
            >
              {section === "providers" ? (
                <ProvidersSection
                  agent={agent}
                  providerProfiles={providerProfiles}
                  providerProfilesLoadStatus={providerProfilesLoadStatus}
                  providerProfilesLoadError={providerProfilesLoadError}
                  providerProfilesSaveStatus={providerProfilesSaveStatus}
                  providerProfilesSaveError={providerProfilesSaveError}
                  providerModelCatalogs={providerModelCatalogs}
                  providerModelCatalogLoadStatuses={providerModelCatalogLoadStatuses}
                  providerModelCatalogLoadErrors={providerModelCatalogLoadErrors}
                  onRefreshProviderProfiles={onRefreshProviderProfiles}
                  onRefreshProviderModelCatalog={onRefreshProviderModelCatalog}
                  onUpsertProviderProfile={onUpsertProviderProfile}
                  onSetActiveProviderProfile={onSetActiveProviderProfile}
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
              ) : section === "browser" ? (
                <BrowserSection />
              ) : section === "themes" ? (
                <ThemesSection />
              ) : section === "development" ? (
                <DevelopmentSection
                  platformOverride={platformOverride}
                  onPlatformOverrideChange={onPlatformOverrideChange}
                  onStartOnboarding={onStartOnboarding}
                />
              ) : null}
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function ProjectBoundEmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-1 items-center justify-center py-14 text-center">
      <div className="max-w-md px-6">
        <p className="text-[14px] font-medium text-foreground">{title}</p>
        <p className="mt-2.5 text-[13px] leading-5 text-muted-foreground">{body}</p>
      </div>
    </div>
  )
}
