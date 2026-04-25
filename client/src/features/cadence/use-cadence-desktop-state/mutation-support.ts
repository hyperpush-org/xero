import type { Dispatch, MutableRefObject, SetStateAction } from 'react'

import {
  CadenceDesktopError,
  type CadenceDesktopAdapter,
} from '@/src/lib/cadence-desktop'
import { mapAutonomousRunInspection } from '@/src/lib/cadence-model/autonomous'
import {
  type McpImportDiagnosticDto,
  type McpRegistryDto,
} from '@/src/lib/cadence-model/mcp'
import { type SkillRegistryDto } from '@/src/lib/cadence-model/skills'
import { type NotificationRouteDto } from '@/src/lib/cadence-model/notifications'
import { type ProjectListItem } from '@/src/lib/cadence-model/project'
import {
  type ProviderProfilesDto,
} from '@/src/lib/cadence-model/provider-profiles'
import {
  type RuntimeRunView,
  type RuntimeSessionView,
  type RuntimeSettingsDto,
} from '@/src/lib/cadence-model/runtime'
import { type ProjectDetailView } from '@/src/lib/cadence-model'

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
  ProviderProfilesLoadStatus,
  ProviderProfilesSaveStatus,
  RefreshSource,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
  RuntimeSettingsLoadStatus,
  RuntimeSettingsSaveStatus,
  McpRegistryLoadStatus,
  McpRegistryMutationStatus,
  SkillRegistryLoadStatus,
  SkillRegistryMutationStatus,
  UseCadenceDesktopStateResult,
} from './types'

type SetState<T> = Dispatch<SetStateAction<T>>
export type NotificationRouteRecords = Record<string, NotificationRouteDto[]>
export type NotificationRouteStatusRecords = Record<string, NotificationRoutesLoadStatus>
export type NotificationRouteErrorRecords = Record<string, OperatorActionErrorView | null>
export type AutonomousInspection = ReturnType<typeof mapAutonomousRunInspection>

export type CadenceDesktopMutationActions = Pick<
  UseCadenceDesktopStateResult,
  | 'importProject'
  | 'removeProject'
  | 'listProjectFiles'
  | 'readProjectFile'
  | 'writeProjectFile'
  | 'createProjectEntry'
  | 'renameProjectEntry'
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
  | 'refreshProviderProfiles'
  | 'upsertProviderProfile'
  | 'setActiveProviderProfile'
  | 'refreshRuntimeSettings'
  | 'upsertRuntimeSettings'
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
  | 'refreshNotificationRoutes'
  | 'upsertNotificationRoute'
  | 'createAgentSession'
  | 'selectAgentSession'
  | 'archiveAgentSession'
  | 'renameAgentSession'
>

export interface UseCadenceDesktopMutationsRefs {
  activeProjectIdRef: MutableRefObject<string | null>
  activeProjectRef: MutableRefObject<ProjectDetailView | null>
  runtimeRunsRef: MutableRefObject<Record<string, RuntimeRunView>>
  providerProfilesRef: MutableRefObject<ProviderProfilesDto | null>
  providerProfilesLoadInFlightRef: MutableRefObject<Promise<ProviderProfilesDto> | null>
  runtimeSettingsRef: MutableRefObject<RuntimeSettingsDto | null>
  runtimeSettingsLoadInFlightRef: MutableRefObject<Promise<RuntimeSettingsDto> | null>
  mcpRegistryRef: MutableRefObject<McpRegistryDto | null>
  mcpRegistryLoadInFlightRef: MutableRefObject<Promise<McpRegistryDto> | null>
  skillRegistryRef: MutableRefObject<SkillRegistryDto | null>
  skillRegistryLoadInFlightRef: MutableRefObject<Promise<SkillRegistryDto> | null>
}

export interface UseCadenceDesktopMutationsSetters {
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
  setProviderProfiles: SetState<ProviderProfilesDto | null>
  setProviderProfilesLoadStatus: SetState<ProviderProfilesLoadStatus>
  setProviderProfilesLoadError: SetState<OperatorActionErrorView | null>
  setProviderProfilesSaveStatus: SetState<ProviderProfilesSaveStatus>
  setProviderProfilesSaveError: SetState<OperatorActionErrorView | null>
  setRuntimeSettings: SetState<RuntimeSettingsDto | null>
  setRuntimeSettingsLoadStatus: SetState<RuntimeSettingsLoadStatus>
  setRuntimeSettingsLoadError: SetState<OperatorActionErrorView | null>
  setRuntimeSettingsSaveStatus: SetState<RuntimeSettingsSaveStatus>
  setRuntimeSettingsSaveError: SetState<OperatorActionErrorView | null>
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

export interface UseCadenceDesktopMutationsOperations {
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

export interface UseCadenceDesktopMutationsArgs {
  adapter: CadenceDesktopAdapter
  refs: UseCadenceDesktopMutationsRefs
  setters: UseCadenceDesktopMutationsSetters
  operations: UseCadenceDesktopMutationsOperations
  providerProfilesLoadStatus: ProviderProfilesLoadStatus
  runtimeSettingsLoadStatus: RuntimeSettingsLoadStatus
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
  if (error instanceof CadenceDesktopError) {
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
