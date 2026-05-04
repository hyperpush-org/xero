import type {
  XeroDesktopMutationActions,
  UseXeroDesktopMutationsArgs,
} from './mutation-support'
import { useAgentSessionMutations } from './agent-session-mutations'
import { useOperatorAuthMutations } from './operator-auth-mutations'
import { useProjectEntryMutations } from './project-entry-mutations'
import { useProviderCredentialsMutations } from './provider-credentials-mutations'
import { useRunControlMutations } from './run-control-mutations'
import { useRuntimeSettingsNotificationMutations } from './runtime-settings-notification-mutations'

export type { XeroDesktopMutationActions } from './mutation-support'

export function useXeroDesktopMutations(
  args: UseXeroDesktopMutationsArgs,
): XeroDesktopMutationActions {
  const {
    importProject,
    createProject,
    removeProject,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    revokeProjectAssetTokens,
    openProjectFileExternal,
    createProjectEntry,
    renameProjectEntry,
    moveProjectEntry,
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
  } = useRuntimeSettingsNotificationMutations(args)
  const {
    refreshProviderCredentials,
    upsertProviderCredential,
    deleteProviderCredential,
    startOAuthLogin,
    completeOAuthCallback,
  } = useProviderCredentialsMutations(args)
  const {
    createAgentSession,
    selectAgentSession,
    archiveAgentSession,
    restoreAgentSession,
    deleteAgentSession,
    renameAgentSession,
  } = useAgentSessionMutations(args)

  return {
    importProject,
    createProject,
    removeProject,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    revokeProjectAssetTokens,
    openProjectFileExternal,
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
    refreshProviderCredentials,
    upsertProviderCredential,
    deleteProviderCredential,
    startOAuthLogin,
    completeOAuthCallback,
    createAgentSession,
    selectAgentSession,
    archiveAgentSession,
    restoreAgentSession,
    deleteAgentSession,
    renameAgentSession,
  }
}
