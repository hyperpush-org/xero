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
import { ProjectAddDialog } from '@/components/xero/project-add-dialog'
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
import { VcsSidebar, type VcsCommitMessageModel } from '@/components/xero/vcs-sidebar'
import { WorkflowsSidebar } from '@/components/xero/workflows-sidebar'
import { XeroDesktopAdapter as DefaultXeroDesktopAdapter, type XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import { mapAgentSession, type RuntimeRunControlInputDto } from '@/src/lib/xero-model/runtime'
import type { AgentDefinitionSummaryDto } from '@/src/lib/xero-model/agent-definition'
import type {
  SessionTranscriptSearchResultSnippetDto,
} from '@/src/lib/xero-model/session-context'
import { type RepositoryDiffScope } from '@/src/lib/xero-model/project'
import { summarizeProjectUsageSpend } from '@/src/lib/xero-model/usage'
import type {
  EnvironmentDiscoveryStatusDto,
  EnvironmentProfileSummaryDto,
} from '@/src/lib/xero-model/environment'
import { useXeroDesktopState, type AgentPaneView } from '@/src/features/xero/use-xero-desktop-state'
import { useGitHubAuth } from '@/src/lib/github-auth'
import { getCloudProviderDefaultProfileId } from '@/src/lib/xero-model/provider-presets'
import { cn } from '@/lib/utils'

export interface XeroAppProps {
  adapter?: XeroDesktopAdapter
}

function getVcsCommitMessageModel(
  agent: AgentPaneView | null,
  composerControls: RuntimeRunControlInputDto | null,
): VcsCommitMessageModel | null {
  const modelId = composerControls?.modelId?.trim() || agent?.selectedModelId?.trim() || null
  if (!agent || !modelId) {
    return null
  }

  const providerId = agent.selectedModel?.providerId ?? agent.selectedProviderId ?? null
  const selectedModelOption =
    agent.providerModelCatalog.models.find(
      (model) =>
        model.modelId === modelId &&
        (!composerControls?.providerProfileId || model.profileId === composerControls.providerProfileId),
    ) ??
    agent.providerModelCatalog.models.find(
      (model) => model.modelId === modelId || model.selectionKey === `${providerId}:${modelId}`,
    ) ?? agent.selectedModelOption
  const providerProfileId =
    composerControls?.providerProfileId ??
    agent.runtimeRunActiveControls?.providerProfileId ??
    agent.runtimeRunPendingControls?.providerProfileId ??
    selectedModelOption?.profileId ??
    getCloudProviderDefaultProfileId(providerId) ??
    null

  return {
    providerProfileId,
    modelId,
    thinkingEffort:
      composerControls?.thinkingEffort ??
      agent.selectedThinkingEffort ??
      agent.selectedModelDefaultThinkingEffort ??
      null,
    label: selectedModelOption?.label ?? modelId,
  }
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
    pendingProjectSelectionId,
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
    createProject,
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
  const [agentComposerControls, setAgentComposerControls] =
    useState<RuntimeRunControlInputDto | null>(null)
  const [isCreatingAgentSession, setIsCreatingAgentSession] = useState(false)
  const [archivedSessionsOpen, setArchivedSessionsOpen] = useState(false)
  const [projectAddOpen, setProjectAddOpen] = useState(false)
  const [gamesOpen, setGamesOpen] = useState(false)
  const [browserOpen, setBrowserOpen] = useState(false)
  const [iosOpen, setIosOpen] = useState(false)
  const [androidOpen, setAndroidOpen] = useState(false)
  const [solanaOpen, setSolanaOpen] = useState(false)
  const [vcsOpen, setVcsOpen] = useState(false)
  const [workflowsOpen, setWorkflowsOpen] = useState(false)
  const [usageOpen, setUsageOpen] = useState(false)
  const [environmentDiscoveryStatus, setEnvironmentDiscoveryStatus] =
    useState<EnvironmentDiscoveryStatusDto | null>(null)
  const [environmentProfileSummary, setEnvironmentProfileSummary] =
    useState<EnvironmentProfileSummaryDto>(null)
  const environmentDiscoveryCheckedRef = useRef(false)
  const [customAgentDefinitions, setCustomAgentDefinitions] = useState<
    readonly AgentDefinitionSummaryDto[]
  >([])
  const [customAgentDefinitionsRevision, setCustomAgentDefinitionsRevision] = useState(0)
  const refreshCustomAgentDefinitions = useCallback(() => {
    setCustomAgentDefinitionsRevision((current) => current + 1)
  }, [])

  useEffect(() => {
    setAgentComposerControls(null)
  }, [activeProjectId])

  useEffect(() => {
    if (!activeProjectId) {
      setCustomAgentDefinitions([])
      return
    }

    let cancelled = false
    void resolvedAdapter
      .listAgentDefinitions({ projectId: activeProjectId, includeArchived: false })
      .then((response) => {
        if (cancelled) return
        const customs = response.definitions.filter((definition) => !definition.isBuiltIn)
        setCustomAgentDefinitions(customs)
      })
      .catch(() => {
        if (cancelled) return
        setCustomAgentDefinitions([])
      })

    return () => {
      cancelled = true
    }
  }, [activeProjectId, customAgentDefinitionsRevision, resolvedAdapter])

  const openSettings = (section: SettingsSection = 'providers') => {
    setSettingsInitialSection(section)
    setSettingsOpen(true)
  }

  const refreshEnvironmentDiscovery = useCallback(
    async (options: { force?: boolean } = {}) => {
      if (!resolvedAdapter.getEnvironmentDiscoveryStatus) {
        return null
      }

      let status =
        options.force && resolvedAdapter.refreshEnvironmentDiscovery
          ? await resolvedAdapter.refreshEnvironmentDiscovery()
          : options.force && resolvedAdapter.startEnvironmentDiscovery
            ? await resolvedAdapter.startEnvironmentDiscovery()
            : await resolvedAdapter.getEnvironmentDiscoveryStatus()

      if (
        !options.force &&
        status.shouldStart &&
        resolvedAdapter.startEnvironmentDiscovery
      ) {
        status = await resolvedAdapter.startEnvironmentDiscovery()
      }

      setEnvironmentDiscoveryStatus(status)
      if (resolvedAdapter.getEnvironmentProfileSummary) {
        const summary = await resolvedAdapter.getEnvironmentProfileSummary()
        setEnvironmentProfileSummary(summary)
      }
      return status
    },
    [resolvedAdapter],
  )

  const resolveEnvironmentPermissions = useCallback(
    async (
      decisions: Array<{
        id: string
        status: 'granted' | 'denied' | 'skipped'
      }>,
    ) => {
      if (!resolvedAdapter.resolveEnvironmentPermissionRequests) {
        return null
      }
      const status = await resolvedAdapter.resolveEnvironmentPermissionRequests({ decisions })
      setEnvironmentDiscoveryStatus(status)
      if (resolvedAdapter.getEnvironmentProfileSummary) {
        const summary = await resolvedAdapter.getEnvironmentProfileSummary()
        setEnvironmentProfileSummary(summary)
      }
      return status
    },
    [resolvedAdapter],
  )

  const toggleGames = () => {
    setGamesOpen((current) => {
      const next = !current
      if (next) {
        setBrowserOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
        setVcsOpen(false)
        setWorkflowsOpen(false)
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
        setWorkflowsOpen(false)
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
        setWorkflowsOpen(false)
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
        setWorkflowsOpen(false)
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
        setWorkflowsOpen(false)
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
        setWorkflowsOpen(false)
      }
      return next
    })
  }

  const toggleWorkflows = () => {
    setWorkflowsOpen((current) => {
      const next = !current
      if (next) {
        setGamesOpen(false)
        setBrowserOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
        setVcsOpen(false)
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
  const shouldRestoreSidebarFromWorkflowsRef = useRef(false)
  const previousViewRef = useRef<View>(activeView)
  const previousBrowserOpenRef = useRef<boolean>(browserOpen)
  const previousWorkflowsOpenRef = useRef<boolean>(workflowsOpen)

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
  const footerSpend = summarizeProjectUsageSpend(activeUsageSummary)
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
    spend: footerSpend
      ? {
          totalTokens: footerSpend.totalTokens,
          totalCostMicros: footerSpend.totalCostMicros,
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
    const wasOpen = previousWorkflowsOpenRef.current

    if (workflowsOpen && !wasOpen) {
      shouldRestoreSidebarFromWorkflowsRef.current = !sidebarCollapsed
      if (!sidebarCollapsed) {
        setSidebarCollapsed(true)
      }
    } else if (
      !workflowsOpen &&
      wasOpen &&
      shouldRestoreSidebarFromWorkflowsRef.current
    ) {
      shouldRestoreSidebarFromWorkflowsRef.current = false
      if (sidebarCollapsed) {
        setSidebarCollapsed(false)
      }
    }

    previousWorkflowsOpenRef.current = workflowsOpen
  }, [workflowsOpen, sidebarCollapsed])

  useEffect(() => {
    if (!onboardingDismissed && !isLoading && projects.length === 0) {
      setOnboardingOpen(true)
    }
  }, [isLoading, onboardingDismissed, projects.length])

  const selectedAgentSessionId = activeProject?.selectedAgentSessionId ?? null
  const handleSelectAgentSession = useCallback(
    (agentSessionId: string) => {
      if (!activeProjectId) return
      if (agentSessionId === selectedAgentSessionId) return
      void selectAgentSession(agentSessionId)
    },
    [activeProjectId, selectAgentSession, selectedAgentSessionId],
  )

  const handleCreateAgentSession = useCallback(() => {
    if (!activeProjectId) return
    setIsCreatingAgentSession(true)
    void createAgentSession().finally(() => {
      setIsCreatingAgentSession(false)
    })
  }, [activeProjectId, createAgentSession])

  const handleArchiveAgentSession = useCallback((agentSessionId: string) => {
    setPendingAgentSessionId(agentSessionId)
    void archiveAgentSession(agentSessionId).finally(() => {
      setPendingAgentSessionId(null)
    })
  }, [archiveAgentSession])

  const handleRenameAgentSession = useCallback(async (agentSessionId: string, title: string) => {
    await renameAgentSession(agentSessionId, title)
  }, [renameAgentSession])

  const handleOpenSearchResult = (result: SessionTranscriptSearchResultSnippetDto) => {
    if (!activeProject) return
    setActiveView('agent')
    if (!result.archived && result.agentSessionId !== activeProject.selectedAgentSessionId) {
      handleSelectAgentSession(result.agentSessionId)
    }
  }

  const handleSelectProject = useCallback(
    (projectId: string) => {
      void selectProject(projectId)
    },
    [selectProject],
  )

  const handleRemoveProject = useCallback(
    (projectId: string) => {
      void removeProject(projectId)
    },
    [removeProject],
  )

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
                onToggleWorkflows={toggleWorkflows}
                workflowsOpen={workflowsOpen}
                onCreateWorkflow={() => {
                  if (!workflowsOpen) toggleWorkflows()
                }}
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
                customAgentDefinitions={customAgentDefinitions}
                onOpenAgentManagement={() => openSettings('agents')}
                onCreateSession={handleCreateAgentSession}
                isCreatingSession={isCreatingAgentSession}
                onLogout={() => logoutRuntimeSession()}
                onOpenSettings={() => openSettings('providers')}
                onOpenDiagnostics={() => openSettings('diagnostics')}
                onResolveOperatorAction={async (actionId, decision, options) => {
                  const result = await resolveOperatorAction(actionId, decision, {
                    userAnswer: options?.userAnswer ?? null,
                  })
                  if (decision === 'approve') {
                    refreshCustomAgentDefinitions()
                  }
                  return result
                }}
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
                onComposerControlsChange={setAgentComposerControls}
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
                active={isExecutionVisible}
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

  useEffect(() => {
    if (environmentDiscoveryCheckedRef.current) {
      return
    }
    if (!resolvedAdapter.getEnvironmentDiscoveryStatus) {
      environmentDiscoveryCheckedRef.current = true
      return
    }

    let cancelled = false
    environmentDiscoveryCheckedRef.current = true

    const startEnvironmentDiscovery = async () => {
      try {
        const status = await refreshEnvironmentDiscovery()
        if (cancelled || !status) return
      } catch {
        // Startup remains non-blocking; diagnostics can surface discovery failures later.
      }
    }

    void startEnvironmentDiscovery()

    return () => {
      cancelled = true
    }
  }, [refreshEnvironmentDiscovery, resolvedAdapter.getEnvironmentDiscoveryStatus])

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
        onToggleWorkflows={toggleWorkflows}
        workflowsOpen={workflowsOpen}
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
          environmentPermissionRequests={environmentDiscoveryStatus?.permissionRequests ?? []}
          onResolveEnvironmentPermissions={resolveEnvironmentPermissions}
          onImportProject={async () => {
            await importProject()
          }}
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
      onToggleWorkflows={toggleWorkflows}
      workflowsOpen={workflowsOpen}
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
        onImportProject={() => setProjectAddOpen(true)}
        onRemoveProject={handleRemoveProject}
        onSelectProject={handleSelectProject}
        pendingProjectSelectionId={pendingProjectSelectionId}
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
      <GamesSidebar accountLogin={githubSession?.user.login ?? null} open={gamesOpen} />
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
      <WorkflowsSidebar open={workflowsOpen} />
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
        commitMessageModel={getVcsCommitMessageModel(agentView, agentComposerControls)}
        onGenerateCommitMessage={(projectId, model) =>
          resolvedAdapter.gitGenerateCommitMessage({
            projectId,
            providerProfileId: model.providerProfileId,
            modelId: model.modelId,
            thinkingEffort: model.thinkingEffort,
          })
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
        environmentDiscoveryStatus={environmentDiscoveryStatus}
        environmentProfileSummary={environmentProfileSummary}
        onRefreshEnvironmentDiscovery={(options) => refreshEnvironmentDiscovery(options)}
        onRunDoctorReport={(request) => runDoctorReport(request)}
        dictationAdapter={resolvedAdapter}
        soulAdapter={resolvedAdapter}
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
        onListAgentDefinitions={(request) => resolvedAdapter.listAgentDefinitions(request)}
        onArchiveAgentDefinition={(request) => resolvedAdapter.archiveAgentDefinition(request)}
        onGetAgentDefinitionVersion={(request) => resolvedAdapter.getAgentDefinitionVersion(request)}
        onAgentRegistryChanged={refreshCustomAgentDefinitions}
      />
      <ProjectAddDialog
        open={projectAddOpen}
        onOpenChange={setProjectAddOpen}
        isImporting={isImporting}
        onSelectExisting={() => importProject()}
        onPickParentFolder={() => resolvedAdapter.pickParentFolder()}
        onCreate={(parentPath, name) => createProject(parentPath, name)}
      />
    </XeroShell>
  )
}

export default function App() {
  return <XeroApp />
}
