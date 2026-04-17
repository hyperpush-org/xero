import { useState } from 'react'
import { AgentRuntime } from '@/components/cadence/agent-runtime'
import { type View } from '@/components/cadence/data'
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

function EmptyPanel({
  eyebrow,
  title,
  body,
  action,
}: {
  eyebrow: string
  title: string
  body: string
  action?: React.ReactNode
}) {
  return (
    <div className="flex flex-1 items-center justify-center bg-background p-6">
      <div className="max-w-md rounded-xl border border-border bg-card px-6 py-8 text-center shadow-sm">
        <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">{eyebrow}</p>
        <h1 className="mt-3 text-2xl font-semibold text-foreground">{title}</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">{body}</p>
        {action ? <div className="mt-5">{action}</div> : null}
      </div>
    </div>
  )
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
    activeDiffScope,
    activeRepositoryDiff,
    isLoading,
    isProjectLoading,
    isImporting,
    errorMessage,
    refreshSource,
    isDesktopRuntime,
    selectProject,
    importProject,
    retry,
    showRepositoryDiff,
    retryActiveRepositoryDiff,
    startOpenAiLogin,
    submitOpenAiCallback,
    startRuntimeRun,
    startRuntimeSession,
    stopRuntimeRun,
    logoutRuntimeSession,
    resolveOperatorAction,
    resumeOperatorRun,
    refreshNotificationRoutes,
    upsertNotificationRoute,
  } = useCadenceDesktopState({ adapter })

  const [settingsOpen, setSettingsOpen] = useState(false)
  const [platformOverride, setPlatformOverride] = useState<PlatformVariant | null>(null)

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
          onResolveOperatorAction={(actionId, decision, options) =>
            resolveOperatorAction(actionId, decision, { userAnswer: options?.userAnswer ?? null })
          }
          onResumeOperatorRun={(actionId, options) =>
            resumeOperatorRun(actionId, { userAnswer: options?.userAnswer ?? null })
          }
          onRefreshNotificationRoutes={(options) => refreshNotificationRoutes(options)}
          onRetryStream={() => retry()}
          onStartLogin={() => startOpenAiLogin()}
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
          activeDiff={activeRepositoryDiff}
          activeDiffScope={activeDiffScope}
          execution={executionView}
          onRetryDiff={() => void retryActiveRepositoryDiff()}
          onSelectDiffScope={(scope) => void showRepositoryDiff(scope)}
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
          onStartRun={() => startRuntimeRun()}
        />
      )
    }

    return null
  }

  return (
    <CadenceShell activeView={activeView} onViewChange={setActiveView} projectName={activeProject?.name} onOpenSettings={() => setSettingsOpen(true)} platformOverride={platformOverride}>
      <ProjectRail
        activeProjectBranch={repositoryStatus?.branchLabel ?? activeProject?.branchLabel ?? null}
        activeProjectId={activeProjectId}
        errorMessage={errorMessage}
        isImporting={isImporting}
        isLoading={isLoading || isProjectLoading}
        onImportProject={() => void importProject()}
        onSelectProject={(projectId) => void selectProject(projectId)}
        projects={projects}
        refreshSource={refreshSource}
        repositoryStatus={repositoryStatus}
      />
      {renderBody()}
      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        agent={agentView}
        onStartLogin={() => startOpenAiLogin()}
        onLogout={() => logoutRuntimeSession()}
        onRefreshNotificationRoutes={(options) => refreshNotificationRoutes(options)}
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
