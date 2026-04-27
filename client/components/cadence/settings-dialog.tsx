"use client"

import { useEffect, useRef, useState } from "react"
import type {
  AgentPaneView,
  DoctorReportRunStatus,
  McpRegistryLoadStatus,
  McpRegistryMutationStatus,
  OperatorActionErrorView,
  ProviderModelCatalogLoadStatus,
  ProviderProfilesLoadStatus,
  ProviderProfilesSaveStatus,
  SkillRegistryLoadStatus,
  SkillRegistryMutationStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import type { DictationSettingsAdapter } from "@/components/cadence/settings-dialog/dictation-section"
import type {
  ImportMcpServersResponseDto,
  CadenceDoctorReportDto,
  McpImportDiagnosticDto,
  McpRegistryDto,
  ProviderModelCatalogDto,
  ProviderProfileDiagnosticsDto,
  ProviderProfilesDto,
  RuntimeSessionView,
  RunDoctorReportRequestDto,
  ListSkillRegistryRequestDto,
  RemovePluginRequestDto,
  RemovePluginRootRequestDto,
  RemoveSkillLocalRootRequestDto,
  RemoveSkillRequestDto,
  SetPluginEnabledRequestDto,
  SetSkillEnabledRequestDto,
  SkillRegistryDto,
  UpdateGithubSkillSourceRequestDto,
  UpdateProjectSkillSourceRequestDto,
  UpsertPluginRootRequestDto,
  UpsertSkillLocalRootRequestDto,
  UpsertMcpServerRequestDto,
  UpsertNotificationRouteRequestDto,
  UpsertProviderProfileRequestDto,
} from "@/src/lib/cadence-model"
import type { PlatformVariant } from "@/components/cadence/shell"
import type {
  GitHubAuthError,
  GitHubAuthStatus,
  GitHubSessionView,
} from "@/src/lib/github-auth"
import { Activity, Bell, Code2, Globe, KeyRound, Mic, Palette, Plug, PlugZap, UserRound, WandSparkles } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { cn } from "@/lib/utils"
import { AccountSection } from "@/components/cadence/settings-dialog/account-section"
import { BrowserSection } from "@/components/cadence/settings-dialog/browser-section"
import { DevelopmentSection } from "@/components/cadence/settings-dialog/development-section"
import { DictationSection } from "@/components/cadence/settings-dialog/dictation-section"
import { DiagnosticsSection } from "@/components/cadence/settings-dialog/diagnostics-section"
import { McpSection } from "@/components/cadence/settings-dialog/mcp-section"
import { NotificationsSection } from "@/components/cadence/settings-dialog/notifications-section"
import { ProvidersSection } from "@/components/cadence/settings-dialog/providers-section"
import { PluginsSection } from "@/components/cadence/settings-dialog/plugins-section"
import { SkillsSection } from "@/components/cadence/settings-dialog/skills-section"
import { ThemesSection } from "@/components/cadence/settings-dialog/themes-section"

export type SettingsSection =
  | "account"
  | "providers"
  | "diagnostics"
  | "dictation"
  | "notifications"
  | "mcp"
  | "skills"
  | "plugins"
  | "browser"
  | "themes"
  | "development"

interface NavItem {
  id: SettingsSection
  label: string
  icon: React.ElementType
}

interface NavGroup {
  id: string
  label: string
  items: NavItem[]
}

const ACCOUNT_GROUP: NavGroup = {
  id: "account",
  label: "Account",
  items: [{ id: "account", label: "Account", icon: UserRound }],
}

const WORKSPACE_GROUP: NavGroup = {
  id: "workspace",
  label: "Workspace",
  items: [
    { id: "providers", label: "Providers", icon: KeyRound },
    { id: "diagnostics", label: "Diagnostics", icon: Activity },
    { id: "dictation", label: "Dictation", icon: Mic },
    { id: "notifications", label: "Notifications", icon: Bell },
    { id: "mcp", label: "MCP", icon: PlugZap },
    { id: "skills", label: "Skills", icon: WandSparkles },
    { id: "plugins", label: "Plugins", icon: Plug },
    { id: "browser", label: "Browser", icon: Globe },
  ],
}

const APPEARANCE_GROUP: NavGroup = {
  id: "appearance",
  label: "Appearance",
  items: [{ id: "themes", label: "Themes", icon: Palette }],
}

const DEVELOPER_GROUP: NavGroup = {
  id: "developer",
  label: "Developer",
  items: [{ id: "development", label: "Development", icon: Code2 }],
}

const NAV_GROUPS: NavGroup[] = import.meta.env.DEV
  ? [ACCOUNT_GROUP, WORKSPACE_GROUP, APPEARANCE_GROUP, DEVELOPER_GROUP]
  : [ACCOUNT_GROUP, WORKSPACE_GROUP, APPEARANCE_GROUP]

export interface SettingsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  initialSection?: SettingsSection
  agent: AgentPaneView | null
  providerProfiles: ProviderProfilesDto | null
  providerProfilesLoadStatus: ProviderProfilesLoadStatus
  providerProfilesLoadError: OperatorActionErrorView | null
  providerProfilesSaveStatus: ProviderProfilesSaveStatus
  providerProfilesSaveError: OperatorActionErrorView | null
  providerModelCatalogs: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses: Record<string, ProviderModelCatalogLoadStatus>
  onRefreshProviderProfiles?: (options?: { force?: boolean }) => Promise<ProviderProfilesDto>
  onRefreshProviderModelCatalog?: (
    profileId: string,
    options?: { force?: boolean },
  ) => Promise<ProviderModelCatalogDto>
  onCheckProviderProfile?: (
    profileId: string,
    options?: { includeNetwork?: boolean },
  ) => Promise<ProviderProfileDiagnosticsDto>
  doctorReport?: CadenceDoctorReportDto | null
  doctorReportStatus?: DoctorReportRunStatus
  doctorReportError?: OperatorActionErrorView | null
  onRunDoctorReport?: (request?: Partial<RunDoctorReportRequestDto>) => Promise<CadenceDoctorReportDto>
  dictationAdapter?: DictationSettingsAdapter
  onUpsertProviderProfile?: (request: UpsertProviderProfileRequestDto) => Promise<ProviderProfilesDto>
  onStartLogin?: (options?: { profileId?: string | null }) => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  onLogoutProviderProfile?: (profileId: string) => Promise<ProviderProfilesDto>
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
  skillRegistry?: SkillRegistryDto | null
  skillRegistryLoadStatus?: SkillRegistryLoadStatus
  skillRegistryLoadError?: OperatorActionErrorView | null
  skillRegistryMutationStatus?: SkillRegistryMutationStatus
  pendingSkillSourceId?: string | null
  skillRegistryMutationError?: OperatorActionErrorView | null
  onRefreshSkillRegistry?: (options?: Partial<ListSkillRegistryRequestDto> & { force?: boolean }) => Promise<SkillRegistryDto>
  onReloadSkillRegistry?: (options?: Partial<ListSkillRegistryRequestDto>) => Promise<SkillRegistryDto>
  onSetSkillEnabled?: (request: SetSkillEnabledRequestDto) => Promise<SkillRegistryDto>
  onRemoveSkill?: (request: RemoveSkillRequestDto) => Promise<SkillRegistryDto>
  onUpsertSkillLocalRoot?: (request: UpsertSkillLocalRootRequestDto) => Promise<SkillRegistryDto>
  onRemoveSkillLocalRoot?: (request: RemoveSkillLocalRootRequestDto) => Promise<SkillRegistryDto>
  onUpdateProjectSkillSource?: (request: UpdateProjectSkillSourceRequestDto) => Promise<SkillRegistryDto>
  onUpdateGithubSkillSource?: (request: UpdateGithubSkillSourceRequestDto) => Promise<SkillRegistryDto>
  onUpsertPluginRoot?: (request: UpsertPluginRootRequestDto) => Promise<SkillRegistryDto>
  onRemovePluginRoot?: (request: RemovePluginRootRequestDto) => Promise<SkillRegistryDto>
  onSetPluginEnabled?: (request: SetPluginEnabledRequestDto) => Promise<SkillRegistryDto>
  onRemovePlugin?: (request: RemovePluginRequestDto) => Promise<SkillRegistryDto>
  platformOverride?: PlatformVariant | null
  onPlatformOverrideChange?: (value: PlatformVariant | null) => void
  onStartOnboarding?: () => void
  githubSession?: GitHubSessionView | null
  githubAuthStatus?: GitHubAuthStatus
  githubAuthError?: GitHubAuthError | null
  onGithubLogin?: () => void
  onGithubLogout?: () => void
}

export function SettingsDialog({
  open,
  onOpenChange,
  initialSection = "providers",
  agent,
  providerProfiles,
  providerProfilesLoadStatus,
  providerProfilesLoadError,
  providerProfilesSaveStatus,
  providerProfilesSaveError,
  providerModelCatalogs,
  providerModelCatalogLoadStatuses,
  onRefreshProviderProfiles,
  onRefreshProviderModelCatalog,
  onCheckProviderProfile,
  doctorReport = null,
  doctorReportStatus = "idle",
  doctorReportError = null,
  onRunDoctorReport,
  dictationAdapter,
  onUpsertProviderProfile,
  onStartLogin,
  onLogout,
  onLogoutProviderProfile,
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
  skillRegistry = null,
  skillRegistryLoadStatus = "idle",
  skillRegistryLoadError = null,
  skillRegistryMutationStatus = "idle",
  pendingSkillSourceId = null,
  skillRegistryMutationError = null,
  onRefreshSkillRegistry,
  onReloadSkillRegistry,
  onSetSkillEnabled,
  onRemoveSkill,
  onUpsertSkillLocalRoot,
  onRemoveSkillLocalRoot,
  onUpdateProjectSkillSource,
  onUpdateGithubSkillSource,
  onUpsertPluginRoot,
  onRemovePluginRoot,
  onSetPluginEnabled,
  onRemovePlugin,
  platformOverride,
  onPlatformOverrideChange,
  onStartOnboarding,
  githubSession = null,
  githubAuthStatus = "idle",
  githubAuthError = null,
  onGithubLogin,
  onGithubLogout,
}: SettingsDialogProps) {
  const [section, setSection] = useState<SettingsSection>("providers")
  const refreshOnOpenCallbacksRef = useRef({
    providerProfiles: onRefreshProviderProfiles,
    mcpRegistry: onRefreshMcpRegistry,
    skillRegistry: onRefreshSkillRegistry,
  })

  useEffect(() => {
    if (open) setSection(initialSection)
  }, [initialSection, open])

  useEffect(() => {
    refreshOnOpenCallbacksRef.current = {
      providerProfiles: onRefreshProviderProfiles,
      mcpRegistry: onRefreshMcpRegistry,
      skillRegistry: onRefreshSkillRegistry,
    }
  }, [onRefreshMcpRegistry, onRefreshProviderProfiles, onRefreshSkillRegistry])

  useEffect(() => {
    if (!open) {
      return
    }

    const { providerProfiles, mcpRegistry, skillRegistry } = refreshOnOpenCallbacksRef.current

    void providerProfiles?.({ force: true }).catch(() => undefined)
    void mcpRegistry?.({ force: true }).catch(() => undefined)
    void skillRegistry?.({ force: true }).catch(() => undefined)
  }, [open])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex h-[min(640px,88vh)] w-[min(880px,92vw)] max-w-none flex-col gap-0 overflow-hidden border-border/80 p-0 shadow-xl sm:max-w-none"
        showCloseButton
      >
        <DialogTitle className="sr-only">Settings</DialogTitle>
        <DialogDescription className="sr-only">
          Configure providers, skills, notification routes, and development options.
        </DialogDescription>

        <div className="flex min-h-0 flex-1">
          <nav className="flex w-44 shrink-0 flex-col gap-3 border-r border-border/70 bg-sidebar py-3">
            {NAV_GROUPS.map((group) => (
              <div key={group.id} className="flex flex-col">
                <span className="px-3 pb-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground/70">
                  {group.label}
                </span>
                <div className="flex flex-col">
                  {group.items.map(({ id, label, icon: Icon }) => {
                    const active = section === id
                    return (
                      <button
                        key={id}
                        type="button"
                        aria-label={label}
                        aria-current={active ? "page" : undefined}
                        onClick={() => setSection(id)}
                        className={cn(
                          "group relative flex items-center gap-2 px-3 py-2.5 text-left text-[12.5px] leading-tight transition-colors",
                          active
                            ? "text-foreground"
                            : "text-muted-foreground hover:text-foreground",
                        )}
                      >
                        {active ? (
                          <span
                            aria-hidden
                            className="absolute inset-y-1 left-0 w-[2px] rounded-r-sm bg-primary"
                          />
                        ) : null}
                        <Icon
                          className={cn(
                            "h-3.5 w-3.5 shrink-0",
                            active ? "text-primary" : "text-muted-foreground/80",
                          )}
                        />
                        <span className="truncate font-medium">{label}</span>
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
              className="flex flex-1 flex-col gap-5 py-5 pl-6 pr-14 animate-in fade-in-0 motion-enter"
            >
              {section === "account" ? (
                <AccountSection
                  session={githubSession ?? null}
                  status={githubAuthStatus ?? "idle"}
                  error={githubAuthError ?? null}
                  onLogin={() => onGithubLogin?.()}
                  onLogout={() => onGithubLogout?.()}
                />
              ) : section === "providers" ? (
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
                  onRefreshProviderProfiles={onRefreshProviderProfiles}
                  onRefreshProviderModelCatalog={onRefreshProviderModelCatalog}
                  onCheckProviderProfile={onCheckProviderProfile}
                  onUpsertProviderProfile={onUpsertProviderProfile}
                  onStartLogin={onStartLogin}
                  onLogout={onLogout}
                  onLogoutProviderProfile={onLogoutProviderProfile}
                />
              ) : section === "diagnostics" ? (
                <DiagnosticsSection
                  doctorReport={doctorReport}
                  doctorReportStatus={doctorReportStatus}
                  doctorReportError={doctorReportError}
                  onRunDoctorReport={onRunDoctorReport}
                />
              ) : section === "dictation" ? (
                <DictationSection adapter={dictationAdapter} />
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
              ) : section === "skills" ? (
                <SkillsSection
                  agent={agent}
                  skillRegistry={skillRegistry}
                  skillRegistryLoadStatus={skillRegistryLoadStatus}
                  skillRegistryLoadError={skillRegistryLoadError}
                  skillRegistryMutationStatus={skillRegistryMutationStatus}
                  pendingSkillSourceId={pendingSkillSourceId}
                  skillRegistryMutationError={skillRegistryMutationError}
                  onRefreshSkillRegistry={onRefreshSkillRegistry}
                  onReloadSkillRegistry={onReloadSkillRegistry}
                  onSetSkillEnabled={onSetSkillEnabled}
                  onRemoveSkill={onRemoveSkill}
                  onUpsertSkillLocalRoot={onUpsertSkillLocalRoot}
                  onRemoveSkillLocalRoot={onRemoveSkillLocalRoot}
                  onUpdateProjectSkillSource={onUpdateProjectSkillSource}
                  onUpdateGithubSkillSource={onUpdateGithubSkillSource}
                />
              ) : section === "plugins" ? (
                <PluginsSection
                  agent={agent}
                  skillRegistry={skillRegistry}
                  skillRegistryLoadStatus={skillRegistryLoadStatus}
                  skillRegistryLoadError={skillRegistryLoadError}
                  skillRegistryMutationStatus={skillRegistryMutationStatus}
                  pendingSkillSourceId={pendingSkillSourceId}
                  skillRegistryMutationError={skillRegistryMutationError}
                  onRefreshSkillRegistry={onRefreshSkillRegistry}
                  onReloadSkillRegistry={onReloadSkillRegistry}
                  onUpsertPluginRoot={onUpsertPluginRoot}
                  onRemovePluginRoot={onRemovePluginRoot}
                  onSetPluginEnabled={onSetPluginEnabled}
                  onRemovePlugin={onRemovePlugin}
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
      <div className="max-w-md">
        <Bell className="mx-auto h-4 w-4 text-muted-foreground/70" />
        <p className="mt-3 text-[13px] font-medium text-foreground">{title}</p>
        <p className="mt-1.5 text-[12px] leading-[1.55] text-muted-foreground">{body}</p>
      </div>
    </div>
  )
}
