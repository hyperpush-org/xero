import type { Dispatch, MutableRefObject, SetStateAction } from 'react'

import {
  XeroDesktopError,
  type XeroDesktopAdapter,
} from '@/src/lib/xero-desktop'
import { mapAutonomousRunInspection } from '@/src/lib/xero-model/autonomous'
import {
  type McpImportDiagnosticDto,
  type McpRegistryDto,
} from '@/src/lib/xero-model/mcp'
import { type SkillRegistryDto } from '@/src/lib/xero-model/skills'
import { type NotificationRouteDto } from '@/src/lib/xero-model/notifications'
import { type ProjectListItem } from '@/src/lib/xero-model/project'
import {
  type ProviderCredentialsSnapshotDto,
} from '@/src/lib/xero-model/provider-credentials'
import {
  type RuntimeRunView,
  type RuntimeSessionView,
} from '@/src/lib/xero-model/runtime'
import { type ProjectDetailView } from '@/src/lib/xero-model'

import type { ProjectLoadSource } from './project-loaders'
import type {
  AutonomousRunActionKind,
  AutonomousRunActionStatus,
  NotificationRouteMutationStatus,
  NotificationRoutesLoadResult,
  NotificationRoutesLoadStatus,
  OperatorActionErrorView,
  OperatorActionStatus,
  ProjectRemovalStatus,
  ProviderCredentialsLoadStatus,
  ProviderCredentialsSaveStatus,
  RefreshSource,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
  McpRegistryLoadStatus,
  McpRegistryMutationStatus,
  SkillRegistryLoadStatus,
  SkillRegistryMutationStatus,
  UseXeroDesktopStateResult,
} from './types'

type SetState<T> = Dispatch<SetStateAction<T>>
export type NotificationRouteRecords = Record<string, NotificationRouteDto[]>
export type NotificationRouteStatusRecords = Record<string, NotificationRoutesLoadStatus>
export type NotificationRouteErrorRecords = Record<string, OperatorActionErrorView | null>
export type AutonomousInspection = ReturnType<typeof mapAutonomousRunInspection>

export type XeroDesktopMutationActions = Pick<
  UseXeroDesktopStateResult,
  | 'importProject'
  | 'removeProject'
  | 'listProjectFiles'
  | 'readProjectFile'
  | 'writeProjectFile'
  | 'createProjectEntry'
  | 'renameProjectEntry'
  | 'moveProjectEntry'
  | 'deleteProjectEntry'
  | 'searchProject'
  | 'replaceInProject'
  | 'startOpenAiLogin'
  | 'submitOpenAiCallback'
  | 'startAutonomousRun'
  | 'inspectAutonomousRun'
  | 'cancelAutonomousRun'
  | 'startRuntimeRun'
  | 'updateRuntimeRunControls'
  | 'startRuntimeSession'
  | 'stopRuntimeRun'
  | 'logoutRuntimeSession'
  | 'resolveOperatorAction'
  | 'resumeOperatorRun'
  | 'refreshProviderCredentials'
  | 'upsertProviderCredential'
  | 'deleteProviderCredential'
  | 'startOAuthLogin'
  | 'completeOAuthCallback'
  | 'refreshMcpRegistry'
  | 'upsertMcpServer'
  | 'removeMcpServer'
  | 'importMcpServers'
  | 'refreshMcpServerStatuses'
  | 'refreshSkillRegistry'
  | 'reloadSkillRegistry'
  | 'setSkillEnabled'
  | 'removeSkill'
  | 'upsertSkillLocalRoot'
  | 'removeSkillLocalRoot'
  | 'updateProjectSkillSource'
  | 'updateGithubSkillSource'
  | 'upsertPluginRoot'
  | 'removePluginRoot'
  | 'setPluginEnabled'
  | 'removePlugin'
  | 'refreshNotificationRoutes'
  | 'upsertNotificationRoute'
  | 'createAgentSession'
  | 'selectAgentSession'
  | 'archiveAgentSession'
  | 'restoreAgentSession'
  | 'deleteAgentSession'
  | 'renameAgentSession'
>

export interface UseXeroDesktopMutationsRefs {
  activeProjectIdRef: MutableRefObject<string | null>
  activeProjectRef: MutableRefObject<ProjectDetailView | null>
  runtimeRunsRef: MutableRefObject<Record<string, RuntimeRunView>>
  providerCredentialsRef: MutableRefObject<ProviderCredentialsSnapshotDto | null>
  providerCredentialsLoadInFlightRef: MutableRefObject<Promise<ProviderCredentialsSnapshotDto> | null>
  mcpRegistryRef: MutableRefObject<McpRegistryDto | null>
  mcpRegistryLoadInFlightRef: MutableRefObject<Promise<McpRegistryDto> | null>
  skillRegistryRef: MutableRefObject<SkillRegistryDto | null>
  skillRegistryLoadInFlightRef: MutableRefObject<Promise<SkillRegistryDto> | null>
}

export interface UseXeroDesktopMutationsSetters {
  setProjects: SetState<ProjectListItem[]>
  setIsImporting: SetState<boolean>
  setProjectRemovalStatus: SetState<ProjectRemovalStatus>
  setPendingProjectRemovalId: SetState<string | null>
  setRefreshSource: SetState<RefreshSource>
  setErrorMessage: SetState<string | null>
  setOperatorActionStatus: SetState<OperatorActionStatus>
  setPendingOperatorActionId: SetState<string | null>
  setOperatorActionError: SetState<OperatorActionErrorView | null>
  setAutonomousRunActionStatus: SetState<AutonomousRunActionStatus>
  setPendingAutonomousRunAction: SetState<AutonomousRunActionKind | null>
  setAutonomousRunActionError: SetState<OperatorActionErrorView | null>
  setRuntimeRunActionStatus: SetState<RuntimeRunActionStatus>
  setPendingRuntimeRunAction: SetState<RuntimeRunActionKind | null>
  setRuntimeRunActionError: SetState<OperatorActionErrorView | null>
  setNotificationRoutes: SetState<NotificationRouteRecords>
  setNotificationRouteLoadStatuses: SetState<NotificationRouteStatusRecords>
  setNotificationRouteLoadErrors: SetState<NotificationRouteErrorRecords>
  setNotificationRouteMutationStatus: SetState<NotificationRouteMutationStatus>
  setPendingNotificationRouteId: SetState<string | null>
  setNotificationRouteMutationError: SetState<OperatorActionErrorView | null>
  setProviderCredentials: SetState<ProviderCredentialsSnapshotDto | null>
  setProviderCredentialsLoadStatus: SetState<ProviderCredentialsLoadStatus>
  setProviderCredentialsLoadError: SetState<OperatorActionErrorView | null>
  setProviderCredentialsSaveStatus: SetState<ProviderCredentialsSaveStatus>
  setProviderCredentialsSaveError: SetState<OperatorActionErrorView | null>
  setMcpRegistry: SetState<McpRegistryDto | null>
  setMcpImportDiagnostics: SetState<McpImportDiagnosticDto[]>
  setMcpRegistryLoadStatus: SetState<McpRegistryLoadStatus>
  setMcpRegistryLoadError: SetState<OperatorActionErrorView | null>
  setMcpRegistryMutationStatus: SetState<McpRegistryMutationStatus>
  setPendingMcpServerId: SetState<string | null>
  setMcpRegistryMutationError: SetState<OperatorActionErrorView | null>
  setSkillRegistry: SetState<SkillRegistryDto | null>
  setSkillRegistryLoadStatus: SetState<SkillRegistryLoadStatus>
  setSkillRegistryLoadError: SetState<OperatorActionErrorView | null>
  setSkillRegistryMutationStatus: SetState<SkillRegistryMutationStatus>
  setPendingSkillSourceId: SetState<string | null>
  setSkillRegistryMutationError: SetState<OperatorActionErrorView | null>
}

export interface UseXeroDesktopMutationsOperations {
  bootstrap: (source?: 'startup' | 'remove') => Promise<void>
  loadProject: (projectId: string, source: ProjectLoadSource) => Promise<ProjectDetailView | null>
  loadNotificationRoutes: (
    projectId: string,
    options?: { force?: boolean },
  ) => Promise<NotificationRoutesLoadResult>
  syncRuntimeSession: (projectId: string) => Promise<RuntimeSessionView>
  syncRuntimeRun: (projectId: string) => Promise<RuntimeRunView | null>
  syncAutonomousRun: (projectId: string) => Promise<ProjectDetailView['autonomousRun'] | null>
  applyRuntimeSessionUpdate: (
    runtimeSession: RuntimeSessionView,
    options?: { clearGlobalError?: boolean },
  ) => RuntimeSessionView
  applyRuntimeRunUpdate: (
    projectId: string,
    runtimeRun: RuntimeRunView | null,
    options?: { clearGlobalError?: boolean; loadError?: string | null },
  ) => RuntimeRunView | null
  applyAutonomousRunStateUpdate: (
    projectId: string,
    inspection: AutonomousInspection,
    options?: { clearGlobalError?: boolean; loadError?: string | null },
  ) => ProjectDetailView['autonomousRun']
}

export interface UseXeroDesktopMutationsArgs {
  adapter: XeroDesktopAdapter
  refs: UseXeroDesktopMutationsRefs
  setters: UseXeroDesktopMutationsSetters
  operations: UseXeroDesktopMutationsOperations
  providerCredentialsLoadStatus: ProviderCredentialsLoadStatus
  mcpRegistryLoadStatus: McpRegistryLoadStatus
  skillRegistryLoadStatus: SkillRegistryLoadStatus
}

export function getActiveProjectId(
  activeProjectIdRef: MutableRefObject<string | null>,
  errorMessage: string,
): string {
  const projectId = activeProjectIdRef.current
  if (!projectId) {
    throw new Error(errorMessage)
  }

  return projectId
}

export function getOperatorActionError(error: unknown, fallback: string): OperatorActionErrorView {
  if (error instanceof XeroDesktopError) {
    return {
      code: error.code,
      message: error.message,
      retryable: error.retryable,
    }
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return {
      code: 'operator_action_failed',
      message: error.message,
      retryable: false,
    }
  }

  return {
    code: 'operator_action_failed',
    message: fallback,
    retryable: false,
  }
}
