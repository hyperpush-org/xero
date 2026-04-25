import type {
  CadenceDesktopMutationActions,
  UseCadenceDesktopMutationsArgs,
} from './mutation-support'
import { useAgentSessionMutations } from './agent-session-mutations'
import { useOperatorAuthMutations } from './operator-auth-mutations'
import { useProjectEntryMutations } from './project-entry-mutations'
import { useRunControlMutations } from './run-control-mutations'
import { useRuntimeSettingsNotificationMutations } from './runtime-settings-notification-mutations'

export type { CadenceDesktopMutationActions } from './mutation-support'

export function useCadenceDesktopMutations(
  args: UseCadenceDesktopMutationsArgs,
): CadenceDesktopMutationActions {
  const {
    importProject,
    removeProject,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    deleteProjectEntry,
    searchProject,
    replaceInProject,
  } = useProjectEntryMutations(args)
  const {
    startOpenAiLogin,
    submitOpenAiCallback,
    startRuntimeSession,
    logoutRuntimeSession,
    resolveOperatorAction,
    resumeOperatorRun,
  } = useOperatorAuthMutations(args)
  const {
    startAutonomousRun,
    inspectAutonomousRun,
    cancelAutonomousRun,
    startRuntimeRun,
    updateRuntimeRunControls,
    stopRuntimeRun,
  } = useRunControlMutations(args)
  const {
    refreshProviderProfiles,
    upsertProviderProfile,
    setActiveProviderProfile,
    refreshRuntimeSettings,
    upsertRuntimeSettings,
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
    refreshNotificationRoutes,
    upsertNotificationRoute,
  } = useRuntimeSettingsNotificationMutations(args)
  const {
    createAgentSession,
    selectAgentSession,
    archiveAgentSession,
    renameAgentSession,
  } = useAgentSessionMutations(args)

  return {
    importProject,
    removeProject,
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
    upsertProviderProfile,
    setActiveProviderProfile,
    refreshRuntimeSettings,
    upsertRuntimeSettings,
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
    refreshNotificationRoutes,
    upsertNotificationRoute,
    createAgentSession,
    selectAgentSession,
    archiveAgentSession,
    renameAgentSession,
  }
}
