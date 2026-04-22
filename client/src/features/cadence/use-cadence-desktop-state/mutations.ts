import type {
  CadenceDesktopMutationActions,
  UseCadenceDesktopMutationsArgs,
} from './mutation-support'
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
    stopRuntimeRun,
  } = useRunControlMutations(args)
  const {
    refreshProviderProfiles,
    upsertProviderProfile,
    setActiveProviderProfile,
    refreshRuntimeSettings,
    upsertRuntimeSettings,
    refreshNotificationRoutes,
    upsertNotificationRoute,
  } = useRuntimeSettingsNotificationMutations(args)

  return {
    importProject,
    removeProject,
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
    refreshProviderProfiles,
    upsertProviderProfile,
    setActiveProviderProfile,
    refreshRuntimeSettings,
    upsertRuntimeSettings,
    refreshNotificationRoutes,
    upsertNotificationRoute,
  }
}
