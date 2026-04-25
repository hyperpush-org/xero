"use client"

import { useEffect, useState } from "react"
import type {
  AgentPaneView,
  McpRegistryLoadStatus,
  McpRegistryMutationStatus,
  OperatorActionErrorView,
  ProviderModelCatalogLoadStatus,
  ProviderProfilesLoadStatus,
  ProviderProfilesSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import type {
  ImportMcpServersResponseDto,
  McpImportDiagnosticDto,
  McpRegistryDto,
  ProviderModelCatalogDto,
  ProviderProfilesDto,
  RuntimeSessionView,
  UpsertMcpServerRequestDto,
  UpsertNotificationRouteRequestDto,
  UpsertProviderProfileRequestDto,
} from "@/src/lib/cadence-model"
import type { PlatformVariant } from "@/components/cadence/shell"
import { Bell, Code2, Globe, KeyRound, Palette, PlugZap, Settings as SettingsIcon } from "lucide-react"
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
import { McpSection } from "@/components/cadence/settings-dialog/mcp-section"
import { NotificationsSection } from "@/components/cadence/settings-dialog/notifications-section"
import { ProvidersSection } from "@/components/cadence/settings-dialog/providers-section"
import { ThemesSection } from "@/components/cadence/settings-dialog/themes-section"

type SettingsSection = "providers" | "notifications" | "mcp" | "browser" | "themes" | "development"

interface NavItem {
  id: SettingsSection
  label: string
  icon: React.ElementType
  hint: string
}

interface NavGroup {
  id: string
  label: string
  items: NavItem[]
}

const WORKSPACE_GROUP: NavGroup = {
  id: "workspace",
  label: "Workspace",
  items: [
    { id: "providers", label: "Providers", icon: KeyRound, hint: "API keys & models" },
    { id: "notifications", label: "Notifications", icon: Bell, hint: "Telegram & Discord" },
    { id: "mcp", label: "MCP", icon: PlugZap, hint: "Model Context servers" },
    { id: "browser", label: "Browser", icon: Globe, hint: "Cookie import" },
  ],
}

const APPEARANCE_GROUP: NavGroup = {
  id: "appearance",
  label: "Appearance",
  items: [{ id: "themes", label: "Themes", icon: Palette, hint: "Color palettes" }],
}

const DEVELOPER_GROUP: NavGroup = {
  id: "developer",
  label: "Developer",
  items: [{ id: "development", label: "Development", icon: Code2, hint: "Preview tools" }],
}

const NAV_GROUPS: NavGroup[] = import.meta.env.DEV
  ? [WORKSPACE_GROUP, APPEARANCE_GROUP, DEVELOPER_GROUP]
  : [WORKSPACE_GROUP, APPEARANCE_GROUP]

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
  mcpRegistry?: McpRegistryDto | null
  mcpImportDiagnostics?: McpImportDiagnosticDto[]
  mcpRegistryLoadStatus?: McpRegistryLoadStatus
  mcpRegistryLoadError?: OperatorActionErrorView | null
  mcpRegistryMutationStatus?: McpRegistryMutationStatus
  pendingMcpServerId?: string | null
  mcpRegistryMutationError?: OperatorActionErrorView | null
  onRefreshMcpRegistry?: (options?: { force?: boolean }) => Promise<McpRegistryDto>
  onUpsertMcpServer?: (request: UpsertMcpServerRequestDto) => Promise<McpRegistryDto>
  onRemoveMcpServer?: (serverId: string) => Promise<McpRegistryDto>
  onImportMcpServers?: (path: string) => Promise<ImportMcpServersResponseDto>
  onRefreshMcpServerStatuses?: (options?: { serverIds?: string[] }) => Promise<McpRegistryDto>
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
  mcpRegistry = null,
  mcpImportDiagnostics = [],
  mcpRegistryLoadStatus = "idle",
  mcpRegistryLoadError = null,
  mcpRegistryMutationStatus = "idle",
  pendingMcpServerId = null,
  mcpRegistryMutationError = null,
  onRefreshMcpRegistry,
  onUpsertMcpServer,
  onRemoveMcpServer,
  onImportMcpServers,
  onRefreshMcpServerStatuses,
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

  useEffect(() => {
    if (!open || !onRefreshMcpRegistry) {
      return
    }

    void onRefreshMcpRegistry({ force: true }).catch(() => undefined)
  }, [onRefreshMcpRegistry, open])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex h-[min(680px,90vh)] w-[min(960px,94vw)] max-w-none flex-col gap-0 overflow-hidden border-border/70 p-0 shadow-2xl sm:max-w-none"
        showCloseButton
      >
        <DialogHeader className="shrink-0 border-b border-border/70 bg-sidebar/40 px-5 py-3.5">
          <div className="flex items-center gap-2.5">
            <SettingsIcon className="h-3.5 w-3.5 text-muted-foreground" />
            <DialogTitle className="text-[13px] font-semibold tracking-tight">Settings</DialogTitle>
          </div>
          <DialogDescription className="sr-only">
            Configure providers, notification routes, and development options.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-h-0 flex-1">
          <nav className="flex w-52 shrink-0 flex-col border-r border-border/70 bg-sidebar/60 py-2">
            {NAV_GROUPS.map((group, groupIndex) => (
              <div
                key={group.id}
                className={cn("flex flex-col", groupIndex > 0 ? "mt-3 border-t border-border/50 pt-3" : "")}
              >
                <span className="px-4 pb-1.5 text-[10.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/70">
                  {group.label}
                </span>
                <div className="flex flex-col px-1.5">
                  {group.items.map(({ id, label, icon: Icon, hint }) => {
                    const active = section === id
                    return (
                      <button
                        key={id}
                        type="button"
                        aria-label={label}
                        aria-current={active ? "page" : undefined}
                        onClick={() => setSection(id)}
                        className={cn(
                          "group relative flex items-center gap-2.5 rounded-md px-2.5 py-2 text-left transition-colors duration-150",
                          active
                            ? "bg-primary/[0.08] text-foreground"
                            : "text-muted-foreground hover:bg-secondary/40 hover:text-foreground",
                        )}
                      >
                        {active ? (
                          <span
                            aria-hidden
                            className="absolute left-0 top-1.5 bottom-1.5 w-[2px] rounded-r-sm bg-primary"
                          />
                        ) : null}
                        <span
                          className={cn(
                            "flex h-7 w-7 shrink-0 items-center justify-center rounded-md border transition-colors",
                            active
                              ? "border-primary/40 bg-primary/[0.12] text-primary"
                              : "border-border/60 bg-secondary/40 text-muted-foreground group-hover:border-border group-hover:text-foreground",
                          )}
                        >
                          <Icon className="h-3.5 w-3.5" />
                        </span>
                        <div className="min-w-0 flex-1">
                          <p className="text-[13px] font-medium leading-tight">{label}</p>
                          <p className="mt-0.5 truncate text-[11px] leading-tight text-muted-foreground/80">
                            {hint}
                          </p>
                        </div>
                      </button>
                    )
                  })}
                </div>
              </div>
            ))}
          </nav>

          <div className="flex flex-1 flex-col overflow-y-auto scrollbar-thin">
            <div
              key={section}
              className="flex flex-1 flex-col px-7 py-6 animate-in fade-in-0 slide-in-from-right-2 motion-enter"
            >
              {section === "providers" ? (
                <ProvidersSection
                  active={open && section === "providers"}
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
              ) : section === "mcp" ? (
                <McpSection
                  mcpRegistry={mcpRegistry}
                  mcpImportDiagnostics={mcpImportDiagnostics}
                  mcpRegistryLoadStatus={mcpRegistryLoadStatus}
                  mcpRegistryLoadError={mcpRegistryLoadError}
                  mcpRegistryMutationStatus={mcpRegistryMutationStatus}
                  pendingMcpServerId={pendingMcpServerId}
                  mcpRegistryMutationError={mcpRegistryMutationError}
                  onRefreshMcpRegistry={onRefreshMcpRegistry}
                  onUpsertMcpServer={onUpsertMcpServer}
                  onRemoveMcpServer={onRemoveMcpServer}
                  onImportMcpServers={onImportMcpServers}
                  onRefreshMcpServerStatuses={onRefreshMcpServerStatuses}
                />
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
      <div className="max-w-md rounded-xl border border-dashed border-border/70 bg-card/50 px-7 py-8">
        <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-md border border-border/70 bg-secondary/60">
          <Bell className="h-[18px] w-[18px] text-muted-foreground" />
        </div>
        <p className="mt-4 text-[14px] font-medium text-foreground">{title}</p>
        <p className="mt-2 text-[13px] leading-5 text-muted-foreground">{body}</p>
      </div>
    </div>
  )
}
