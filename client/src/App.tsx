import { useEffect, useRef, useState } from 'react'
import { AgentRuntime } from '@/components/cadence/agent-runtime'
import { AgentSessionsSidebar } from '@/components/cadence/agent-sessions-sidebar'
import { type View } from '@/components/cadence/data'
import { EmptyPanel } from '@/components/cadence/empty-panel'
import { ExecutionView } from '@/components/cadence/execution-view'
import { NoProjectEmptyState } from '@/components/cadence/no-project-empty-state'
import { OnboardingFlow } from '@/components/cadence/onboarding/onboarding-flow'
import { ProjectLoadErrorState } from '@/components/cadence/project-load-error-state'
import { PhaseView } from '@/components/cadence/phase-view'
import { ProjectRail } from '@/components/cadence/project-rail'
import { CadenceShell, type PlatformVariant } from '@/components/cadence/shell'
import type { FooterRuntimeState, StatusFooterProps } from '@/components/cadence/status-footer'
import { GamesSidebar } from '@/components/cadence/games-sidebar'
import { BrowserSidebar } from '@/components/cadence/browser-sidebar'
import { IosEmulatorSidebar } from '@/components/cadence/ios-emulator-sidebar'
import { AndroidEmulatorSidebar } from '@/components/cadence/android-emulator-sidebar'
import { SolanaWorkbenchSidebar } from '@/components/cadence/solana-workbench-sidebar'
import { SettingsDialog } from '@/components/cadence/settings-dialog'
import { type CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
import { useCadenceDesktopState } from '@/src/features/cadence/use-cadence-desktop-state'
import { cn } from '@/lib/utils'

export interface CadenceAppProps {
  adapter?: CadenceDesktopAdapter
}

function resolveFooterRuntimeState(status: {
  isActive?: boolean
  isStale?: boolean
} | null | undefined): FooterRuntimeState {
  if (status?.isActive) {
    return 'running'
  }

  if (status?.isStale) {
    return 'paused'
  }

  return 'idle'
}

export function CadenceApp({ adapter }: CadenceAppProps) {
  const [activeView, setActiveView] = useState<View>('phases')
  const {
    projects,
    activeProject,
    activeProjectId,
    repositoryStatus,
    workflowView,
    agentView,
    executionView,
    isLoading,
    isProjectLoading,
    isImporting,
    projectRemovalStatus,
    pendingProjectRemovalId,
    errorMessage,
    providerProfiles,
    providerProfilesLoadStatus,
    providerProfilesLoadError,
    providerProfilesSaveStatus,
    providerProfilesSaveError,
    providerModelCatalogs,
    providerModelCatalogLoadStatuses,
    providerModelCatalogLoadErrors,
    mcpRegistry,
    mcpImportDiagnostics,
    mcpRegistryLoadStatus,
    mcpRegistryLoadError,
    mcpRegistryMutationStatus,
    pendingMcpServerId,
    mcpRegistryMutationError,
    isDesktopRuntime,
    selectProject,
    importProject,
    removeProject,
    retry,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    deleteProjectEntry,
    searchProject,
    replaceInProject,
    startOpenAiLogin,
    submitOpenAiCallback,
    startAutonomousRun,
    inspectAutonomousRun,
    cancelAutonomousRun,
    startRuntimeRun,
    updateRuntimeRunControls,
    startRuntimeSession,
    stopRuntimeRun,
    logoutRuntimeSession,
    resolveOperatorAction,
    resumeOperatorRun,
    refreshProviderProfiles,
    refreshProviderModelCatalog,
    upsertProviderProfile,
    setActiveProviderProfile,
    refreshMcpRegistry,
    upsertMcpServer,
    removeMcpServer,
    importMcpServers,
    refreshMcpServerStatuses,
    refreshNotificationRoutes,
    upsertNotificationRoute,
    createAgentSession,
    selectAgentSession,
    archiveAgentSession,
  } = useCadenceDesktopState({ adapter })

  const [settingsOpen, setSettingsOpen] = useState(false)
  const [pendingAgentSessionId, setPendingAgentSessionId] = useState<string | null>(null)
  const [isCreatingAgentSession, setIsCreatingAgentSession] = useState(false)
  const [gamesOpen, setGamesOpen] = useState(false)
  const [browserOpen, setBrowserOpen] = useState(false)
  const [iosOpen, setIosOpen] = useState(false)
  const [androidOpen, setAndroidOpen] = useState(false)
  const [solanaOpen, setSolanaOpen] = useState(false)

  const toggleGames = () => {
    setGamesOpen((current) => {
      const next = !current
      if (next) {
        setBrowserOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
      }
      return next
    })
  }

  const toggleBrowser = () => {
    setBrowserOpen((current) => {
      const next = !current
      if (next) {
        setGamesOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
      }
      return next
    })
  }

  const toggleIos = () => {
    setIosOpen((current) => {
      const next = !current
      if (next) {
        setGamesOpen(false)
        setBrowserOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
      }
      return next
    })
  }

  const toggleAndroid = () => {
    setAndroidOpen((current) => {
      const next = !current
      if (next) {
        setGamesOpen(false)
        setBrowserOpen(false)
        setIosOpen(false)
        setSolanaOpen(false)
      }
      return next
    })
  }

  const toggleSolana = () => {
    setSolanaOpen((current) => {
      const next = !current
      if (next) {
        setGamesOpen(false)
        setBrowserOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
      }
      return next
    })
  }
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [platformOverride, setPlatformOverride] = useState<PlatformVariant | null>(null)
  const [onboardingDismissed, setOnboardingDismissed] = useState(false)
  const [onboardingOpen, setOnboardingOpen] = useState(false)
  const shouldRestoreSidebarFromAutoCollapseRef = useRef(false)
  const previousViewRef = useRef<View>(activeView)

  const statusFooter: StatusFooterProps = {
    git: activeProject
      ? {
          branch: repositoryStatus?.branchLabel ?? activeProject.repository?.branchLabel ?? activeProject.branchLabel,
          hasChanges: repositoryStatus?.hasChanges ?? activeProject.repositoryStatus?.hasChanges ?? false,
          changedFiles: repositoryStatus?.statusCount ?? activeProject.repositoryStatus?.statusCount ?? 0,
          lastCommit: (repositoryStatus?.lastCommit ?? activeProject.repositoryStatus?.lastCommit)
            ? {
                sha: (repositoryStatus?.lastCommit ?? activeProject.repositoryStatus?.lastCommit)?.sha,
                message: (repositoryStatus?.lastCommit ?? activeProject.repositoryStatus?.lastCommit)?.summary,
                committedAt: (repositoryStatus?.lastCommit ?? activeProject.repositoryStatus?.lastCommit)?.committedAt,
              }
            : null,
        }
      : null,
    runtime: agentView
      ? {
          provider: agentView.selectedProviderLabel ?? null,
          state: resolveFooterRuntimeState(agentView.runtimeRun),
        }
      : null,
  }

  useEffect(() => {
    const previousView = previousViewRef.current
    const autoCollapseViews: View[] = ['execution', 'agent']
    const isAutoCollapseView = autoCollapseViews.includes(activeView)
    const wasAutoCollapseView = autoCollapseViews.includes(previousView)

    if (isAutoCollapseView && !wasAutoCollapseView) {
      shouldRestoreSidebarFromAutoCollapseRef.current = !sidebarCollapsed
      if (!sidebarCollapsed) {
        setSidebarCollapsed(true)
      }
    }

    if (!isAutoCollapseView && wasAutoCollapseView && shouldRestoreSidebarFromAutoCollapseRef.current) {
      shouldRestoreSidebarFromAutoCollapseRef.current = false
      if (sidebarCollapsed) {
        setSidebarCollapsed(false)
      }
    }

    if (!isAutoCollapseView && !wasAutoCollapseView) {
      shouldRestoreSidebarFromAutoCollapseRef.current = false
    }

    previousViewRef.current = activeView
  }, [activeView, sidebarCollapsed])

  useEffect(() => {
    if (!onboardingDismissed && !isLoading && projects.length === 0) {
      setOnboardingOpen(true)
    }
  }, [isLoading, onboardingDismissed, projects.length])

  const renderBody = () => {
    if (isLoading && !activeProject) {
      return (
        <EmptyPanel
          eyebrow="Loading"
          title="Loading desktop project state"
          body="Cadence is reading the imported projects, snapshot, and repository status from the desktop backend."
        />
      )
    }

    if (!activeProject && errorMessage) {
      return <ProjectLoadErrorState message={errorMessage} onRetry={() => void retry()} />
    }

    if (!activeProject) {
      return (
        <NoProjectEmptyState
          isDesktopRuntime={isDesktopRuntime}
          isImporting={isImporting}
          onImport={() => void importProject()}
        />
      )
    }

    const shouldRenderExecutionPanel = Boolean(executionView && activeProjectId)

    const handleSelectAgentSession = (agentSessionId: string) => {
      if (agentSessionId === activeProject.selectedAgentSessionId) return
      setPendingAgentSessionId(agentSessionId)
      void selectAgentSession(agentSessionId).finally(() => {
        setPendingAgentSessionId(null)
      })
    }

    const handleCreateAgentSession = () => {
      setIsCreatingAgentSession(true)
      void createAgentSession().finally(() => {
        setIsCreatingAgentSession(false)
      })
    }

    const handleArchiveAgentSession = (agentSessionId: string) => {
      setPendingAgentSessionId(agentSessionId)
      void archiveAgentSession(agentSessionId).finally(() => {
        setPendingAgentSessionId(null)
      })
    }

    const isExecutionVisible = activeView === 'execution'
    const getViewPaneClassName = (visible: boolean) =>
      cn(
        'absolute inset-0 flex min-h-0 min-w-0 transform-gpu overflow-hidden transition-[opacity,transform] motion-standard',
        visible
          ? 'z-10 translate-x-0 opacity-100'
          : 'pointer-events-none z-0 translate-x-2 opacity-0',
      )

    return (
      <>
        <AgentSessionsSidebar
          projectLabel={activeProject.name}
          sessions={activeProject.agentSessions}
          selectedSessionId={activeProject.selectedAgentSessionId}
          onSelectSession={handleSelectAgentSession}
          onCreateSession={handleCreateAgentSession}
          onArchiveSession={handleArchiveAgentSession}
          pendingSessionId={pendingAgentSessionId}
          isCreating={isCreatingAgentSession}
          collapsed={activeView !== 'agent'}
        />
        <div className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden">
          {workflowView ? (
            <div
              aria-hidden={activeView !== 'phases'}
              className={getViewPaneClassName(activeView === 'phases')}
              inert={activeView !== 'phases' ? true : undefined}
            >
              <PhaseView
                workflow={workflowView}
                canStartRun={Boolean(
                  agentView?.runtimeRunActionStatus !== undefined &&
                    !agentView.runtimeRun &&
                    agentView.runtimeSession?.isAuthenticated,
                )}
                isStartingRun={agentView?.runtimeRunActionStatus === 'running'}
                onOpenSettings={() => setSettingsOpen(true)}
                onStartRun={() => startRuntimeRun()}
              />
            </div>
          ) : null}

          {agentView ? (
            <div
              aria-hidden={activeView !== 'agent'}
              className={getViewPaneClassName(activeView === 'agent')}
              inert={activeView !== 'agent' ? true : undefined}
            >
              <AgentRuntime
                agent={agentView}
                onLogout={() => logoutRuntimeSession()}
                onOpenSettings={() => setSettingsOpen(true)}
                onResolveOperatorAction={(actionId, decision, options) =>
                  resolveOperatorAction(actionId, decision, { userAnswer: options?.userAnswer ?? null })
                }
                onResumeOperatorRun={(actionId, options) =>
                  resumeOperatorRun(actionId, { userAnswer: options?.userAnswer ?? null })
                }
                onRefreshNotificationRoutes={(options) => refreshNotificationRoutes(options)}
                onRetryStream={() => retry()}
                onStartLogin={() => startOpenAiLogin()}
                onStartAutonomousRun={() => startAutonomousRun()}
                onInspectAutonomousRun={() => inspectAutonomousRun()}
                onCancelAutonomousRun={(runId) => cancelAutonomousRun(runId)}
                onStartRuntimeRun={(options) => startRuntimeRun(options)}
                onUpdateRuntimeRunControls={(request) => updateRuntimeRunControls(request)}
                onStartRuntimeSession={() => startRuntimeSession()}
                onStopRuntimeRun={(runId) => stopRuntimeRun(runId)}
                onSubmitManualCallback={(flowId, manualInput) =>
                  submitOpenAiCallback(flowId, { manualInput })
                }
                onUpsertNotificationRoute={(request) => upsertNotificationRoute(request)}
              />
            </div>
          ) : null}

          {shouldRenderExecutionPanel && executionView ? (
            <div
              aria-hidden={!isExecutionVisible}
              className={getViewPaneClassName(isExecutionVisible)}
              inert={!isExecutionVisible ? true : undefined}
            >
              <ExecutionView
                execution={executionView}
                listProjectFiles={listProjectFiles}
                readProjectFile={readProjectFile}
                writeProjectFile={writeProjectFile}
                createProjectEntry={createProjectEntry}
                renameProjectEntry={renameProjectEntry}
                deleteProjectEntry={deleteProjectEntry}
                searchProject={searchProject}
                replaceInProject={replaceInProject}
              />
            </div>
          ) : null}
        </div>
      </>
    )
  }

  const onboardingProject = activeProject
    ? {
        name: activeProject.name,
        path: activeProject.repository?.rootPath ?? activeProject.name,
      }
    : null
  const showOnboarding = onboardingOpen && !onboardingDismissed && !isLoading

  if (showOnboarding) {
    return (
      <CadenceShell
        activeView={activeView}
        onViewChange={setActiveView}
        projectName={activeProject?.name}
        onOpenSettings={() => setSettingsOpen(true)}
        onToggleGames={toggleGames}
        gamesOpen={gamesOpen}
        onToggleBrowser={toggleBrowser}
        browserOpen={browserOpen}
        onToggleIos={toggleIos}
        iosOpen={iosOpen}
        onToggleAndroid={toggleAndroid}
        androidOpen={androidOpen}
        onToggleSolana={toggleSolana}
        solanaOpen={solanaOpen}
        sidebarCollapsed={sidebarCollapsed}
        onToggleSidebar={() => setSidebarCollapsed((current) => !current)}
        platformOverride={platformOverride}
        footer={statusFooter}
        chromeOnly
      >
        <OnboardingFlow
          providerProfiles={providerProfiles}
          providerProfilesLoadStatus={providerProfilesLoadStatus}
          providerProfilesLoadError={providerProfilesLoadError}
          providerProfilesSaveStatus={providerProfilesSaveStatus}
          providerProfilesSaveError={providerProfilesSaveError}
          providerModelCatalogs={providerModelCatalogs}
          providerModelCatalogLoadStatuses={providerModelCatalogLoadStatuses}
          providerModelCatalogLoadErrors={providerModelCatalogLoadErrors}
          runtimeSession={agentView?.runtimeSession ?? null}
          project={onboardingProject}
          isImporting={isImporting}
          isProjectLoading={isProjectLoading}
          projectErrorMessage={errorMessage}
          notificationRoutes={agentView?.notificationRoutes ?? []}
          notificationRouteMutationStatus={agentView?.notificationRouteMutationStatus ?? 'idle'}
          pendingNotificationRouteId={agentView?.pendingNotificationRouteId ?? null}
          notificationRouteMutationError={agentView?.notificationRouteMutationError ?? null}
          onImportProject={() => importProject()}
          onRefreshProviderProfiles={(options) => refreshProviderProfiles(options)}
          onRefreshProviderModelCatalog={(profileId, options) =>
            refreshProviderModelCatalog(profileId, options)
          }
          onUpsertProviderProfile={(request) => upsertProviderProfile(request)}
          onSetActiveProviderProfile={(profileId) => setActiveProviderProfile(profileId)}
          onStartLogin={() => startOpenAiLogin()}
          onLogout={() => logoutRuntimeSession()}
          onUpsertNotificationRoute={(request) => upsertNotificationRoute(request)}
          onComplete={() => {
            setOnboardingDismissed(true)
            setOnboardingOpen(false)
          }}
          onDismiss={() => {
            setOnboardingDismissed(true)
            setOnboardingOpen(false)
          }}
        />
      </CadenceShell>
    )
  }

  return (
    <CadenceShell
      activeView={activeView}
      onViewChange={setActiveView}
      projectName={activeProject?.name}
      onOpenSettings={() => setSettingsOpen(true)}
      onToggleGames={toggleGames}
      gamesOpen={gamesOpen}
      onToggleBrowser={toggleBrowser}
      browserOpen={browserOpen}
      onToggleIos={toggleIos}
      iosOpen={iosOpen}
      onToggleAndroid={toggleAndroid}
      androidOpen={androidOpen}
      onToggleSolana={toggleSolana}
      solanaOpen={solanaOpen}
      sidebarCollapsed={sidebarCollapsed}
      onToggleSidebar={() => setSidebarCollapsed((current) => !current)}
      platformOverride={platformOverride}
      footer={statusFooter}
    >
      <ProjectRail
        activeProjectId={activeProjectId}
        collapsed={sidebarCollapsed}
        errorMessage={errorMessage}
        isImporting={isImporting}
        isLoading={isLoading || isProjectLoading}
        onImportProject={() => void importProject()}
        onRemoveProject={(projectId) => void removeProject(projectId)}
        onSelectProject={(projectId) => void selectProject(projectId)}
        pendingProjectRemovalId={pendingProjectRemovalId}
        projectRemovalStatus={projectRemovalStatus}
        projects={projects}
      />
      {renderBody()}
      <GamesSidebar open={gamesOpen} />
      <BrowserSidebar open={browserOpen} />
      <IosEmulatorSidebar open={iosOpen} />
      <AndroidEmulatorSidebar open={androidOpen} />
      <SolanaWorkbenchSidebar open={solanaOpen} />
      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        agent={agentView}
        providerProfiles={providerProfiles}
        providerProfilesLoadStatus={providerProfilesLoadStatus}
        providerProfilesLoadError={providerProfilesLoadError}
        providerProfilesSaveStatus={providerProfilesSaveStatus}
        providerProfilesSaveError={providerProfilesSaveError}
        providerModelCatalogs={providerModelCatalogs}
        providerModelCatalogLoadStatuses={providerModelCatalogLoadStatuses}
        providerModelCatalogLoadErrors={providerModelCatalogLoadErrors}
        onRefreshProviderProfiles={(options) => refreshProviderProfiles(options)}
        onRefreshProviderModelCatalog={(profileId, options) =>
          refreshProviderModelCatalog(profileId, options)
        }
        onUpsertProviderProfile={(request) => upsertProviderProfile(request)}
        onSetActiveProviderProfile={(profileId) => setActiveProviderProfile(profileId)}
        onStartLogin={() => startOpenAiLogin()}
        onLogout={() => logoutRuntimeSession()}
        onUpsertNotificationRoute={(request) =>
          upsertNotificationRoute({ ...request, updatedAt: new Date().toISOString() })
        }
        mcpRegistry={mcpRegistry}
        mcpImportDiagnostics={mcpImportDiagnostics}
        mcpRegistryLoadStatus={mcpRegistryLoadStatus}
        mcpRegistryLoadError={mcpRegistryLoadError}
        mcpRegistryMutationStatus={mcpRegistryMutationStatus}
        pendingMcpServerId={pendingMcpServerId}
        mcpRegistryMutationError={mcpRegistryMutationError}
        onRefreshMcpRegistry={(options) => refreshMcpRegistry(options)}
        onUpsertMcpServer={(request) => upsertMcpServer(request)}
        onRemoveMcpServer={(serverId) => removeMcpServer(serverId)}
        onImportMcpServers={(path) => importMcpServers(path)}
        onRefreshMcpServerStatuses={(options) => refreshMcpServerStatuses(options)}
        platformOverride={platformOverride}
        onPlatformOverrideChange={setPlatformOverride}
        onStartOnboarding={() => {
          setSettingsOpen(false)
          setOnboardingDismissed(false)
          setOnboardingOpen(true)
        }}
      />
    </CadenceShell>
  )
}

export default function App() {
  return <CadenceApp />
}
