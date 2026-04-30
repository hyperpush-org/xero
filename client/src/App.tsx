import { useCallback, useEffect, useRef, useState } from 'react'
import { AgentRuntime } from '@/components/xero/agent-runtime'
import { SetupEmptyState } from '@/components/xero/agent-runtime/setup-empty-state'
import { AgentSessionsSidebar } from '@/components/xero/agent-sessions-sidebar'
import { ArchivedSessionsDialog } from '@/components/xero/archived-sessions-dialog'
import { type View } from '@/components/xero/data'
import { LoadingScreen } from '@/components/xero/loading-screen'
import { ExecutionView } from '@/components/xero/execution-view'
import { NoProjectEmptyState } from '@/components/xero/no-project-empty-state'
import { OnboardingFlow } from '@/components/xero/onboarding/onboarding-flow'
import { ProjectLoadErrorState } from '@/components/xero/project-load-error-state'
import { PhaseView } from '@/components/xero/phase-view'
import { ProjectRail } from '@/components/xero/project-rail'
import { XeroShell, type PlatformVariant } from '@/components/xero/shell'
import type { StatusFooterProps } from '@/components/xero/status-footer'
import { GamesSidebar } from '@/components/xero/games-sidebar'
import { BrowserSidebar } from '@/components/xero/browser-sidebar'
import { IosEmulatorSidebar } from '@/components/xero/ios-emulator-sidebar'
import { AndroidEmulatorSidebar } from '@/components/xero/android-emulator-sidebar'
import { SolanaWorkbenchSidebar } from '@/components/xero/solana-workbench-sidebar'
import { SettingsDialog, type SettingsSection } from '@/components/xero/settings-dialog'
import { UsageStatsSidebar } from '@/components/xero/usage-stats-sidebar'
import { VcsSidebar } from '@/components/xero/vcs-sidebar'
import { XeroDesktopAdapter as DefaultXeroDesktopAdapter, type XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import { mapAgentSession } from '@/src/lib/xero-model/runtime'
import type {
  SessionTranscriptSearchResultSnippetDto,
} from '@/src/lib/xero-model/session-context'
import { type RepositoryDiffScope } from '@/src/lib/xero-model/project'
import { useXeroDesktopState } from '@/src/features/xero/use-xero-desktop-state'
import { useGitHubAuth } from '@/src/lib/github-auth'
import { cn } from '@/lib/utils'

export interface XeroAppProps {
  adapter?: XeroDesktopAdapter
}

export function XeroApp({ adapter }: XeroAppProps) {
  const resolvedAdapter = adapter ?? DefaultXeroDesktopAdapter
  const [activeView, setActiveViewRaw] = useState<View>('phases')

  // Tab switches simultaneously trigger the cross-fade of view panes AND the
  // auto-collapse of the project rail / sessions sidebar (both via useEffect
  // below). Animating the sidebar widths at the same time as heavy view
  // contents (CodeMirror, agent UI, phase view) re-layout produces visible
  // jitter on slower hosts, so we mark the document as `data-layout-shifting`
  // for one frame around the change. CSS in globals.css disables the
  // `.sidebar-motion-island` width transitions while the attribute is set —
  // sidebars snap to their new widths instantly, leaving only the cheap
  // GPU-driven pane cross-fade animating on the main thread. User-initiated
  // toggles (e.g. clicking the rail collapse button) still animate normally.
  const layoutShiftFrameRef = useRef<number | null>(null)
  const setActiveView = useCallback((view: View) => {
    setActiveViewRaw((current) => {
      if (current === view) return current
      if (typeof window !== 'undefined') {
        document.documentElement.dataset.layoutShifting = 'true'
        if (layoutShiftFrameRef.current !== null) {
          window.cancelAnimationFrame(layoutShiftFrameRef.current)
        }
        layoutShiftFrameRef.current = window.requestAnimationFrame(() => {
          layoutShiftFrameRef.current = window.requestAnimationFrame(() => {
            delete document.documentElement.dataset.layoutShifting
            layoutShiftFrameRef.current = null
          })
        })
      }
      return view
    })
  }, [])
  useEffect(() => {
    return () => {
      if (typeof window !== 'undefined' && layoutShiftFrameRef.current !== null) {
        window.cancelAnimationFrame(layoutShiftFrameRef.current)
      }
    }
  }, [])
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
    providerCredentials,
    providerCredentialsLoadStatus,
    providerCredentialsLoadError,
    providerCredentialsSaveStatus,
    providerCredentialsSaveError,
    doctorReport,
    doctorReportStatus,
    doctorReportError,
    mcpRegistry,
    mcpImportDiagnostics,
    mcpRegistryLoadStatus,
    mcpRegistryLoadError,
    mcpRegistryMutationStatus,
    pendingMcpServerId,
    mcpRegistryMutationError,
    skillRegistry,
    skillRegistryLoadStatus,
    skillRegistryLoadError,
    skillRegistryMutationStatus,
    pendingSkillSourceId,
    skillRegistryMutationError,
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
    moveProjectEntry,
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
    runDoctorReport,
    refreshProviderCredentials,
    upsertProviderCredential,
    deleteProviderCredential,
    startOAuthLogin,
    refreshMcpRegistry,
    upsertMcpServer,
    removeMcpServer,
    importMcpServers,
    refreshMcpServerStatuses,
    refreshSkillRegistry,
    reloadSkillRegistry,
    setSkillEnabled,
    removeSkill,
    upsertSkillLocalRoot,
    removeSkillLocalRoot,
    updateProjectSkillSource,
    updateGithubSkillSource,
    upsertPluginRoot,
    removePluginRoot,
    setPluginEnabled,
    removePlugin,
    refreshNotificationRoutes,
    upsertNotificationRoute,
    createAgentSession,
    selectAgentSession,
    archiveAgentSession,
    restoreAgentSession,
    deleteAgentSession,
    renameAgentSession,
    activeUsageSummary,
    refreshUsageSummary,
  } = useXeroDesktopState({ adapter })

  const {
    session: githubSession,
    status: githubAuthStatus,
    error: githubAuthError,
    login: loginWithGithub,
    logout: logoutGithub,
  } = useGitHubAuth()

  const [settingsOpen, setSettingsOpen] = useState(false)
  const [settingsInitialSection, setSettingsInitialSection] = useState<SettingsSection>('providers')
  const [pendingAgentSessionId, setPendingAgentSessionId] = useState<string | null>(null)
  const [isCreatingAgentSession, setIsCreatingAgentSession] = useState(false)
  const [archivedSessionsOpen, setArchivedSessionsOpen] = useState(false)
  const [gamesOpen, setGamesOpen] = useState(false)
  const [browserOpen, setBrowserOpen] = useState(false)
  const [iosOpen, setIosOpen] = useState(false)
  const [androidOpen, setAndroidOpen] = useState(false)
  const [solanaOpen, setSolanaOpen] = useState(false)
  const [vcsOpen, setVcsOpen] = useState(false)
  const [usageOpen, setUsageOpen] = useState(false)

  const openSettings = (section: SettingsSection = 'providers') => {
    setSettingsInitialSection(section)
    setSettingsOpen(true)
  }

  const toggleGames = () => {
    setGamesOpen((current) => {
      const next = !current
      if (next) {
        setBrowserOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
        setVcsOpen(false)
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
        setVcsOpen(false)
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
        setVcsOpen(false)
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
        setVcsOpen(false)
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
        setVcsOpen(false)
      }
      return next
    })
  }

  const toggleVcs = () => {
    setVcsOpen((current) => {
      const next = !current
      if (next) {
        setGamesOpen(false)
        setBrowserOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
      }
      return next
    })
  }
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [explorerCollapsed, setExplorerCollapsed] = useState<boolean>(() => {
    if (typeof window === 'undefined') return false
    try {
      return window.localStorage.getItem('xero.explorer.collapsed') === '1'
    } catch {
      return false
    }
  })

  useEffect(() => {
    if (typeof window === 'undefined') return
    try {
      window.localStorage.setItem(
        'xero.explorer.collapsed',
        explorerCollapsed ? '1' : '0',
      )
    } catch {
      /* storage unavailable — revert silently */
    }
  }, [explorerCollapsed])

  const [platformOverride, setPlatformOverride] = useState<PlatformVariant | null>(null)
  const [onboardingDismissed, setOnboardingDismissed] = useState(false)
  const [onboardingOpen, setOnboardingOpen] = useState(false)
  const shouldRestoreSidebarFromAutoCollapseRef = useRef(false)
  const shouldRestoreExplorerFromAutoCollapseRef = useRef(false)
  const previousViewRef = useRef<View>(activeView)
  const previousBrowserOpenRef = useRef<boolean>(browserOpen)

  useEffect(() => {
    const wasBrowserOpen = previousBrowserOpenRef.current

    if (activeView === 'agent' && browserOpen && !wasBrowserOpen) {
      shouldRestoreExplorerFromAutoCollapseRef.current = !explorerCollapsed
      if (!explorerCollapsed) {
        setExplorerCollapsed(true)
      }
    } else if (
      !browserOpen &&
      wasBrowserOpen &&
      shouldRestoreExplorerFromAutoCollapseRef.current
    ) {
      shouldRestoreExplorerFromAutoCollapseRef.current = false
      if (explorerCollapsed) {
        setExplorerCollapsed(false)
      }
    }

    previousBrowserOpenRef.current = browserOpen
  }, [activeView, browserOpen, explorerCollapsed])

  const footerRepositoryStatus = repositoryStatus ?? activeProject?.repositoryStatus ?? null
  const footerLastCommit = footerRepositoryStatus?.lastCommit ?? null
  const statusFooter: StatusFooterProps = {
    git: activeProject
      ? {
          branch:
            footerRepositoryStatus?.branchLabel ??
            activeProject.repository?.branchLabel ??
            activeProject.branchLabel,
          upstream: footerRepositoryStatus?.upstream ?? null,
          hasChanges: footerRepositoryStatus?.hasChanges ?? false,
          changedFiles: footerRepositoryStatus?.statusCount ?? 0,
          lastCommit: footerLastCommit
            ? {
                sha: footerLastCommit.sha,
                message: footerLastCommit.summary,
                committedAt: footerLastCommit.committedAt,
              }
            : null,
        }
      : null,
    spend: activeUsageSummary
      ? {
          totalTokens: activeUsageSummary.totals.totalTokens,
          totalCostMicros: activeUsageSummary.totals.estimatedCostMicros,
        }
      : null,
    spendActive: usageOpen,
    onSpendClick: activeProjectId
      ? () => setUsageOpen((current) => !current)
      : undefined,
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

  const handleSelectAgentSession = (agentSessionId: string) => {
    if (!activeProject) return
    if (agentSessionId === activeProject.selectedAgentSessionId) return
    setPendingAgentSessionId(agentSessionId)
    void selectAgentSession(agentSessionId).finally(() => {
      setPendingAgentSessionId(null)
    })
  }

  const handleCreateAgentSession = () => {
    if (!activeProject) return
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

  const handleRenameAgentSession = async (agentSessionId: string, title: string) => {
    await renameAgentSession(agentSessionId, title)
  }

  const handleOpenSearchResult = (result: SessionTranscriptSearchResultSnippetDto) => {
    if (!activeProject) return
    setActiveView('agent')
    if (!result.archived && result.agentSessionId !== activeProject.selectedAgentSessionId) {
      handleSelectAgentSession(result.agentSessionId)
    }
  }

  const renderBody = () => {
    if (isLoading && !activeProject) {
      return <LoadingScreen />
    }

    if (!activeProject && errorMessage) {
      return <ProjectLoadErrorState message={errorMessage} onRetry={() => void retry()} />
    }

    if (!activeProject) {
      if (activeView === 'agent') {
        const hasReadyProvider = (providerCredentials?.credentials.length ?? 0) > 0
        return (
          <div className="flex flex-1 items-center justify-center overflow-y-auto scrollbar-thin px-6 py-5">
            <SetupEmptyState
              kind={hasReadyProvider ? 'no-project' : 'no-provider'}
              onOpenSettings={() => openSettings('providers')}
              onImportProject={() => void importProject()}
              isImportingProject={isImporting}
              isDesktopRuntime={isDesktopRuntime}
            />
          </div>
        )
      }

      return (
        <NoProjectEmptyState
          isDesktopRuntime={isDesktopRuntime}
          isImporting={isImporting}
          onImport={() => void importProject()}
        />
      )
    }

    const shouldRenderExecutionPanel = Boolean(executionView && activeProjectId)

    const isExecutionVisible = activeView === 'execution'
    const getViewPaneClassName = (visible: boolean) =>
      cn(
        'view-pane absolute inset-0 flex min-h-0 min-w-0 transform-gpu overflow-hidden transition-[opacity,transform] motion-standard',
        visible
          ? 'z-10 translate-x-0 opacity-100'
          : 'pointer-events-none z-0 translate-x-2 opacity-0',
      )

    return (
      <>
        <AgentSessionsSidebar
          projectId={activeProject.id}
          sessions={activeProject.agentSessions}
          selectedSessionId={activeProject.selectedAgentSessionId}
          onSelectSession={handleSelectAgentSession}
          onCreateSession={handleCreateAgentSession}
          onArchiveSession={handleArchiveAgentSession}
          onOpenArchivedSessions={() => setArchivedSessionsOpen(true)}
          onRenameSession={handleRenameAgentSession}
          onSearchSessions={
            resolvedAdapter.searchSessionTranscripts
              ? async (query) => {
                  const response = await resolvedAdapter.searchSessionTranscripts?.({
                    projectId: activeProject.id,
                    query,
                    includeArchived: true,
                    limit: 12,
                  })
                  return response?.results ?? []
                }
              : undefined
          }
          onOpenSearchResult={handleOpenSearchResult}
          pendingSessionId={pendingAgentSessionId}
          isCreating={isCreatingAgentSession}
          collapsed={activeView !== 'agent' || explorerCollapsed}
          onCollapse={() => setExplorerCollapsed(true)}
        />
        <ArchivedSessionsDialog
          open={archivedSessionsOpen}
          onOpenChange={setArchivedSessionsOpen}
          projectId={activeProject.id}
          projectLabel={activeProject.name}
          onLoad={async (projectId) => {
            const response = await resolvedAdapter.listAgentSessions({
              projectId,
              includeArchived: true,
            })
            return response.sessions
              .filter((session) => session.status === 'archived')
              .map(mapAgentSession)
          }}
          onRestore={async (agentSessionId) => {
            await restoreAgentSession(agentSessionId)
            await selectAgentSession(agentSessionId)
          }}
          onDelete={async (agentSessionId) => {
            await deleteAgentSession(agentSessionId)
          }}
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
                onOpenSettings={() => openSettings('providers')}
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
                desktopAdapter={resolvedAdapter}
                accountAvatarUrl={githubSession?.user.avatarUrl ?? null}
                accountLogin={githubSession?.user.login ?? null}
                onLogout={() => logoutRuntimeSession()}
                onOpenSettings={() => openSettings('providers')}
                onOpenDiagnostics={() => openSettings('diagnostics')}
                onResolveOperatorAction={(actionId, decision, options) =>
                  resolveOperatorAction(actionId, decision, { userAnswer: options?.userAnswer ?? null })
                }
                onResumeOperatorRun={(actionId, options) =>
                  resumeOperatorRun(actionId, { userAnswer: options?.userAnswer ?? null })
                }
                onRefreshNotificationRoutes={(options) => refreshNotificationRoutes(options)}
                onRetryStream={() => retry()}
                onStartLogin={(options) => startOpenAiLogin(options)}
                onStartAutonomousRun={() => startAutonomousRun()}
                onInspectAutonomousRun={() => inspectAutonomousRun()}
                onCancelAutonomousRun={(runId) => cancelAutonomousRun(runId)}
                onStartRuntimeRun={(options) => startRuntimeRun(options)}
                onUpdateRuntimeRunControls={(request) => updateRuntimeRunControls(request)}
                onStartRuntimeSession={(options) => startRuntimeSession(options)}
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
                moveProjectEntry={moveProjectEntry}
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
  const shouldAutoOpenOnboarding = !onboardingDismissed && !isLoading && projects.length === 0
  const showOnboarding = (onboardingOpen || shouldAutoOpenOnboarding) && !onboardingDismissed && !isLoading

  if (showOnboarding) {
    return (
      <XeroShell
        activeView={activeView}
        onViewChange={setActiveView}
        projectName={activeProject?.name}
        onOpenSettings={() => openSettings('providers')}
        onOpenAccount={() => openSettings('account')}
        onAccountLogin={() => {
          void loginWithGithub()
        }}
        accountAuthenticating={githubAuthStatus === 'authenticating'}
        accountAvatarUrl={githubSession?.user.avatarUrl ?? null}
        accountLogin={githubSession?.user.login ?? null}
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
        onToggleVcs={toggleVcs}
        vcsOpen={vcsOpen}
        vcsChangeCount={repositoryStatus?.statusCount ?? 0}
        vcsAdditions={repositoryStatus?.additions ?? 0}
        vcsDeletions={repositoryStatus?.deletions ?? 0}
        sidebarCollapsed={sidebarCollapsed}
        onToggleSidebar={() => setSidebarCollapsed((current) => !current)}
        platformOverride={platformOverride}
        footer={statusFooter}
        chromeOnly
        hideFooter
      >
        <OnboardingFlow
          providerCredentials={providerCredentials}
          providerCredentialsLoadStatus={providerCredentialsLoadStatus}
          providerCredentialsLoadError={providerCredentialsLoadError}
          providerCredentialsSaveStatus={providerCredentialsSaveStatus}
          providerCredentialsSaveError={providerCredentialsSaveError}
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
          onRefreshProviderCredentials={(options) => refreshProviderCredentials(options)}
          onUpsertProviderCredential={(request) => upsertProviderCredential(request)}
          onDeleteProviderCredential={(providerId) => deleteProviderCredential(providerId)}
          onStartOAuthLogin={(request) => startOAuthLogin(request)}
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
      </XeroShell>
    )
  }

  return (
    <XeroShell
      activeView={activeView}
      onViewChange={setActiveView}
      projectName={activeProject?.name}
      onOpenSettings={() => openSettings('providers')}
      onOpenAccount={() => openSettings('account')}
      onAccountLogin={() => {
        void loginWithGithub()
      }}
      accountAuthenticating={githubAuthStatus === 'authenticating'}
      accountAvatarUrl={githubSession?.user.avatarUrl ?? null}
      accountLogin={githubSession?.user.login ?? null}
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
      onToggleVcs={toggleVcs}
      vcsOpen={vcsOpen}
      vcsChangeCount={repositoryStatus?.statusCount ?? 0}
      vcsAdditions={repositoryStatus?.additions ?? 0}
      vcsDeletions={repositoryStatus?.deletions ?? 0}
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
        explorerCollapsed={activeView === 'agent' && explorerCollapsed && Boolean(activeProject)}
        onExpandExplorer={() => setExplorerCollapsed(false)}
        sessions={activeProject?.agentSessions}
        selectedSessionId={activeProject?.selectedAgentSessionId ?? null}
        pendingSessionId={pendingAgentSessionId}
        isCreatingSession={isCreatingAgentSession}
        onSelectSession={handleSelectAgentSession}
        onCreateSession={handleCreateAgentSession}
        onArchiveSession={handleArchiveAgentSession}
        onOpenArchivedSessions={() => setArchivedSessionsOpen(true)}
      />
      {renderBody()}
      <GamesSidebar open={gamesOpen} />
      <BrowserSidebar open={browserOpen} />
      <UsageStatsSidebar
        open={usageOpen}
        projectId={activeProjectId}
        projectName={activeProject?.name ?? null}
        summary={activeUsageSummary}
        onClose={() => setUsageOpen(false)}
        onRefresh={refreshUsageSummary}
      />
      <IosEmulatorSidebar open={iosOpen} />
      <AndroidEmulatorSidebar open={androidOpen} />
      <SolanaWorkbenchSidebar open={solanaOpen} />
      <VcsSidebar
        open={vcsOpen}
        projectId={activeProjectId}
        status={repositoryStatus}
        branchLabel={repositoryStatus?.branchLabel ?? activeProject?.branchLabel ?? null}
        onClose={() => setVcsOpen(false)}
        onRefreshStatus={() => {
          if (activeProjectId) {
            return retry()
          }
          return undefined
        }}
        onLoadDiff={(projectId, scope: RepositoryDiffScope) =>
          resolvedAdapter.getRepositoryDiff(projectId, scope)
        }
        onStage={(projectId, paths) => resolvedAdapter.gitStagePaths(projectId, paths)}
        onUnstage={(projectId, paths) => resolvedAdapter.gitUnstagePaths(projectId, paths)}
        onDiscard={(projectId, paths) => resolvedAdapter.gitDiscardChanges(projectId, paths)}
        onCommit={(projectId, message) => resolvedAdapter.gitCommit(projectId, message)}
        onFetch={(projectId) => resolvedAdapter.gitFetch(projectId)}
        onPull={(projectId) => resolvedAdapter.gitPull(projectId)}
        onPush={(projectId) => resolvedAdapter.gitPush(projectId)}
      />
      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        initialSection={settingsInitialSection}
        agent={agentView}
        providerCredentials={providerCredentials}
        providerCredentialsLoadStatus={providerCredentialsLoadStatus}
        providerCredentialsLoadError={providerCredentialsLoadError}
        providerCredentialsSaveStatus={providerCredentialsSaveStatus}
        providerCredentialsSaveError={providerCredentialsSaveError}
        onRefreshProviderCredentials={(options) => refreshProviderCredentials(options)}
        onUpsertProviderCredential={(request) => upsertProviderCredential(request)}
        onDeleteProviderCredential={(providerId) => deleteProviderCredential(providerId)}
        onStartOAuthLogin={(request) => startOAuthLogin(request)}
        doctorReport={doctorReport}
        doctorReportStatus={doctorReportStatus}
        doctorReportError={doctorReportError}
        onRunDoctorReport={(request) => runDoctorReport(request)}
        dictationAdapter={resolvedAdapter}
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
        skillRegistry={skillRegistry}
        skillRegistryLoadStatus={skillRegistryLoadStatus}
        skillRegistryLoadError={skillRegistryLoadError}
        skillRegistryMutationStatus={skillRegistryMutationStatus}
        pendingSkillSourceId={pendingSkillSourceId}
        skillRegistryMutationError={skillRegistryMutationError}
        onRefreshSkillRegistry={(options) => refreshSkillRegistry(options)}
        onReloadSkillRegistry={(options) => reloadSkillRegistry(options)}
        onSetSkillEnabled={(request) => setSkillEnabled(request)}
        onRemoveSkill={(request) => removeSkill(request)}
        onUpsertSkillLocalRoot={(request) => upsertSkillLocalRoot(request)}
        onRemoveSkillLocalRoot={(request) => removeSkillLocalRoot(request)}
        onUpdateProjectSkillSource={(request) => updateProjectSkillSource(request)}
        onUpdateGithubSkillSource={(request) => updateGithubSkillSource(request)}
        onUpsertPluginRoot={(request) => upsertPluginRoot(request)}
        onRemovePluginRoot={(request) => removePluginRoot(request)}
        onSetPluginEnabled={(request) => setPluginEnabled(request)}
        onRemovePlugin={(request) => removePlugin(request)}
        platformOverride={platformOverride}
        onPlatformOverrideChange={setPlatformOverride}
        onStartOnboarding={() => {
          setSettingsOpen(false)
          setOnboardingDismissed(false)
          setOnboardingOpen(true)
        }}
        githubSession={githubSession}
        githubAuthStatus={githubAuthStatus}
        githubAuthError={githubAuthError}
        onGithubLogin={() => void loginWithGithub()}
        onGithubLogout={() => void logoutGithub()}
      />
    </XeroShell>
  )
}

export default function App() {
  return <XeroApp />
}
