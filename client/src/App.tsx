import { useEffect, useRef, useState } from 'react'
import { AgentRuntime } from '@/components/cadence/agent-runtime'
import { type View } from '@/components/cadence/data'
import { EmptyPanel } from '@/components/cadence/empty-panel'
import { ExecutionView } from '@/components/cadence/execution-view'
import { PhaseView } from '@/components/cadence/phase-view'
import { ProjectRail } from '@/components/cadence/project-rail'
import { CadenceShell, type PlatformVariant } from '@/components/cadence/shell'
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
    refreshSource,
    runtimeSettings,
    runtimeSettingsLoadStatus,
    runtimeSettingsLoadError,
    runtimeSettingsSaveStatus,
    runtimeSettingsSaveError,
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
    startRuntimeSession,
    stopRuntimeRun,
    logoutRuntimeSession,
    resolveOperatorAction,
    resumeOperatorRun,
    refreshRuntimeSettings,
    upsertRuntimeSettings,
    refreshNotificationRoutes,
    upsertNotificationRoute,
  } = useCadenceDesktopState({ adapter })

  const [settingsOpen, setSettingsOpen] = useState(false)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [platformOverride, setPlatformOverride] = useState<PlatformVariant | null>(null)
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
      return (
        <EmptyPanel
          eyebrow="Load Error"
          title="Desktop state could not be loaded"
          body={errorMessage}
          action={
            <button
              className="rounded-md border border-border px-3 py-2 text-sm text-foreground transition-colors hover:bg-secondary/60"
              onClick={() => void retry()}
              type="button"
            >
              Retry
            </button>
          }
        />
      )
    }

    if (!activeProject) {
      return (
        <EmptyPanel
          eyebrow={isDesktopRuntime ? 'Desktop Shell Ready' : 'Desktop Runtime Required'}
          title="No projects imported"
          body={
            isDesktopRuntime
              ? 'The Vite/Tauri shell is running, but there is no backend project state loaded yet.'
              : 'Project import is only available inside the Tauri desktop runtime. Open the desktop shell to load backend state.'
          }
          action={
            isDesktopRuntime ? (
              <button
                className="rounded-md border border-border px-3 py-2 text-sm text-foreground transition-colors hover:bg-secondary/60 disabled:opacity-50"
                disabled={isImporting}
                onClick={() => void importProject()}
                type="button"
              >
                {isImporting ? 'Importing…' : 'Import repository'}
              </button>
            ) : null
          }
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
          onStartRuntimeRun={() => startRuntimeRun()}
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

  return (
    <CadenceShell
      activeView={activeView}
      onViewChange={setActiveView}
      projectName={activeProject?.name}
      onOpenSettings={() => setSettingsOpen(true)}
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
      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        agent={agentView}
        runtimeSettings={runtimeSettings}
        runtimeSettingsLoadStatus={runtimeSettingsLoadStatus}
        runtimeSettingsLoadError={runtimeSettingsLoadError}
        runtimeSettingsSaveStatus={runtimeSettingsSaveStatus}
        runtimeSettingsSaveError={runtimeSettingsSaveError}
        onRefreshRuntimeSettings={(options) => refreshRuntimeSettings(options)}
        onUpsertRuntimeSettings={(request) => upsertRuntimeSettings(request)}
        onStartLogin={() => startOpenAiLogin()}
        onLogout={() => logoutRuntimeSession()}
        onUpsertNotificationRoute={(request) => upsertNotificationRoute({ ...request, updatedAt: new Date().toISOString() })}
        platformOverride={platformOverride}
        onPlatformOverrideChange={setPlatformOverride}
      />
    </CadenceShell>
  )
}

export default function App() {
  return <CadenceApp />
}
