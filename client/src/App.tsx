import { useEffect, useRef, useState } from 'react'
import { AgentRuntime } from '@/components/cadence/agent-runtime'
import { type View } from '@/components/cadence/data'
import { EmptyPanel } from '@/components/cadence/empty-panel'
import { ExecutionView } from '@/components/cadence/execution-view'
import { NoProjectEmptyState } from '@/components/cadence/no-project-empty-state'
import { OnboardingFlow } from '@/components/cadence/onboarding/onboarding-flow'
import { ProjectLoadErrorState } from '@/components/cadence/project-load-error-state'
import { PhaseView } from '@/components/cadence/phase-view'
import { ProjectRail } from '@/components/cadence/project-rail'
import { CadenceShell, type PlatformVariant } from '@/components/cadence/shell'
import { GamesSidebar } from '@/components/cadence/games-sidebar'
import { SettingsDialog } from '@/components/cadence/settings-dialog'
import { type CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
import { useCadenceDesktopState } from '@/src/features/cadence/use-cadence-desktop-state'

export interface CadenceAppProps {
  adapter?: CadenceDesktopAdapter
}

export function CadenceApp({ adapter }: CadenceAppProps) {
  const [activeView, setActiveView] = useState<View>('phases')
  const {
    projects,
    activeProject,
    activeProjectId,
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
    refreshNotificationRoutes,
    upsertNotificationRoute,
  } = useCadenceDesktopState({ adapter })

  const [settingsOpen, setSettingsOpen] = useState(false)
  const [gamesOpen, setGamesOpen] = useState(false)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [platformOverride, setPlatformOverride] = useState<PlatformVariant | null>(null)
  const [onboardingDismissed, setOnboardingDismissed] = useState(false)
  const [onboardingOpen, setOnboardingOpen] = useState(false)
  const shouldRestoreSidebarFromEditorRef = useRef(false)
  const previousViewRef = useRef<View>(activeView)

  useEffect(() => {
    const previousView = previousViewRef.current

    if (activeView === 'execution' && previousView !== 'execution') {
      shouldRestoreSidebarFromEditorRef.current = !sidebarCollapsed
      if (!sidebarCollapsed) {
        setSidebarCollapsed(true)
      }
    }

    if (activeView !== 'execution' && previousView === 'execution' && shouldRestoreSidebarFromEditorRef.current) {
      shouldRestoreSidebarFromEditorRef.current = false
      if (sidebarCollapsed) {
        setSidebarCollapsed(false)
      }
    }

    if (activeView !== 'execution' && previousView !== 'execution') {
      shouldRestoreSidebarFromEditorRef.current = false
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

    if (activeView === 'agent' && agentView) {
      return (
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
      )
    }

    if (activeView === 'execution' && executionView) {
      return (
        <ExecutionView
          execution={executionView}
          listProjectFiles={listProjectFiles}
          readProjectFile={readProjectFile}
          writeProjectFile={writeProjectFile}
          createProjectEntry={createProjectEntry}
          renameProjectEntry={renameProjectEntry}
          deleteProjectEntry={deleteProjectEntry}
        />
      )
    }

    if (workflowView) {
      return (
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
      )
    }

    return null
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
        onToggleGames={() => setGamesOpen((current) => !current)}
        gamesOpen={gamesOpen}
        sidebarCollapsed={sidebarCollapsed}
        onToggleSidebar={() => setSidebarCollapsed((current) => !current)}
        platformOverride={platformOverride}
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
      onToggleGames={() => setGamesOpen((current) => !current)}
      gamesOpen={gamesOpen}
      sidebarCollapsed={sidebarCollapsed}
      onToggleSidebar={() => setSidebarCollapsed((current) => !current)}
      platformOverride={platformOverride}
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
