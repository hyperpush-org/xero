import { Channel, invoke, isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { ZodError, z } from 'zod'
import {
  applyWorkflowTransitionRequestSchema,
  applyWorkflowTransitionResponseSchema,
  autonomousRunStateSchema,
  createProjectEntryRequestSchema,
  createProjectEntryResponseSchema,
  deleteProjectEntryResponseSchema,
  importRepositoryResponseSchema,
  listNotificationDispatchesRequestSchema,
  listNotificationDispatchesResponseSchema,
  listNotificationRoutesRequestSchema,
  listNotificationRoutesResponseSchema,
  listProjectFilesResponseSchema,
  listProjectsResponseSchema,
  projectFileRequestSchema,
  projectSnapshotResponseSchema,
  projectUpdatedPayloadSchema,
  readProjectFileResponseSchema,
  recordNotificationDispatchOutcomeRequestSchema,
  recordNotificationDispatchOutcomeResponseSchema,
  renameProjectEntryRequestSchema,
  renameProjectEntryResponseSchema,
  repositoryDiffResponseSchema,
  repositoryStatusChangedPayloadSchema,
  repositoryStatusResponseSchema,
  resolveOperatorActionRequestSchema,
  resolveOperatorActionResponseSchema,
  resumeOperatorRunRequestSchema,
  resumeOperatorRunResponseSchema,
  submitNotificationReplyRequestSchema,
  submitNotificationReplyResponseSchema,
  syncNotificationAdaptersRequestSchema,
  syncNotificationAdaptersResponseSchema,
  runtimeRunSchema,
  runtimeRunUpdatedPayloadSchema,
  runtimeSessionSchema,
  runtimeSettingsSchema,
  runtimeStreamItemSchema,
  runtimeUpdatedPayloadSchema,
  subscribeRuntimeStreamRequestSchema,
  subscribeRuntimeStreamResponseSchema,
  upsertNotificationRouteRequestSchema,
  upsertNotificationRouteResponseSchema,
  upsertNotificationRouteCredentialsRequestSchema,
  upsertNotificationRouteCredentialsResponseSchema,
  upsertRuntimeSettingsRequestSchema,
  upsertWorkflowGraphRequestSchema,
  upsertWorkflowGraphResponseSchema,
  writeProjectFileRequestSchema,
  writeProjectFileResponseSchema,
  type ApplyWorkflowTransitionRequestDto,
  type ApplyWorkflowTransitionResponseDto,
  type AutonomousRunStateDto,
  type CreateProjectEntryRequestDto,
  type CreateProjectEntryResponseDto,
  type DeleteProjectEntryResponseDto,
  type ImportRepositoryResponseDto,
  type ListNotificationDispatchesResponseDto,
  type ListNotificationRoutesResponseDto,
  type ListProjectFilesResponseDto,
  type ListProjectsResponseDto,
  type ProjectFileRequestDto,
  type ProjectSnapshotResponseDto,
  type ProjectUpdatedPayloadDto,
  type ReadProjectFileResponseDto,
  type RecordNotificationDispatchOutcomeRequestDto,
  type RecordNotificationDispatchOutcomeResponseDto,
  type RenameProjectEntryRequestDto,
  type RenameProjectEntryResponseDto,
  type RepositoryDiffResponseDto,
  type RepositoryDiffScope,
  type RepositoryStatusChangedPayloadDto,
  type RepositoryStatusResponseDto,
  type ResolveOperatorActionResponseDto,
  type ResumeOperatorRunResponseDto,
  type RuntimeRunDto,
  type RuntimeRunUpdatedPayloadDto,
  type RuntimeSessionDto,
  type RuntimeSettingsDto,
  type RuntimeStreamEventDto,
  type RuntimeStreamItemKindDto,
  type RuntimeUpdatedPayloadDto,
  type SubmitNotificationReplyRequestDto,
  type SubmitNotificationReplyResponseDto,
  type SyncNotificationAdaptersResponseDto,
  type SubscribeRuntimeStreamResponseDto,
  type UpsertNotificationRouteRequestDto,
  type UpsertNotificationRouteResponseDto,
  type UpsertNotificationRouteCredentialsRequestDto,
  type UpsertNotificationRouteCredentialsResponseDto,
  type UpsertRuntimeSettingsRequestDto,
  type UpsertWorkflowGraphRequestDto,
  type UpsertWorkflowGraphResponseDto,
  type WriteProjectFileRequestDto,
  type WriteProjectFileResponseDto,
} from '@/src/lib/cadence-model'

const COMMANDS = {
  importRepository: 'import_repository',
  listProjects: 'list_projects',
  removeProject: 'remove_project',
  getProjectSnapshot: 'get_project_snapshot',
  getRepositoryStatus: 'get_repository_status',
  getRepositoryDiff: 'get_repository_diff',
  listProjectFiles: 'list_project_files',
  readProjectFile: 'read_project_file',
  writeProjectFile: 'write_project_file',
  createProjectEntry: 'create_project_entry',
  renameProjectEntry: 'rename_project_entry',
  deleteProjectEntry: 'delete_project_entry',
  getAutonomousRun: 'get_autonomous_run',
  getRuntimeRun: 'get_runtime_run',
  getRuntimeSession: 'get_runtime_session',
  getRuntimeSettings: 'get_runtime_settings',
  startOpenAiLogin: 'start_openai_login',
  submitOpenAiCallback: 'submit_openai_callback',
  startAutonomousRun: 'start_autonomous_run',
  startRuntimeRun: 'start_runtime_run',
  startRuntimeSession: 'start_runtime_session',
  cancelAutonomousRun: 'cancel_autonomous_run',
  stopRuntimeRun: 'stop_runtime_run',
  logoutRuntimeSession: 'logout_runtime_session',
  upsertRuntimeSettings: 'upsert_runtime_settings',
  resolveOperatorAction: 'resolve_operator_action',
  resumeOperatorRun: 'resume_operator_run',
  listNotificationRoutes: 'list_notification_routes',
  listNotificationDispatches: 'list_notification_dispatches',
  upsertNotificationRoute: 'upsert_notification_route',
  upsertNotificationRouteCredentials: 'upsert_notification_route_credentials',
  recordNotificationDispatchOutcome: 'record_notification_dispatch_outcome',
  submitNotificationReply: 'submit_notification_reply',
  syncNotificationAdapters: 'sync_notification_adapters',
  subscribeRuntimeStream: 'subscribe_runtime_stream',
  upsertWorkflowGraph: 'upsert_workflow_graph',
  applyWorkflowTransition: 'apply_workflow_transition',
} as const

const EVENTS = {
  projectUpdated: 'project:updated',
  repositoryStatusChanged: 'repository:status_changed',
  runtimeUpdated: 'runtime:updated',
  runtimeRunUpdated: 'runtime_run:updated',
} as const

const commandErrorSchema = z.object({
  code: z.string(),
  class: z.enum(['user_fixable', 'retryable', 'system_fault', 'policy_denied']),
  message: z.string(),
  retryable: z.boolean(),
})

export class CadenceDesktopError extends Error {
  code: string
  errorClass: z.infer<typeof commandErrorSchema>['class'] | 'adapter_contract_mismatch' | 'desktop_runtime_unavailable'
  retryable: boolean

  constructor(options: {
    message: string
    code?: string
    errorClass?: CadenceDesktopError['errorClass']
    retryable?: boolean
    cause?: unknown
  }) {
    super(options.message)
    this.name = 'CadenceDesktopError'
    this.code = options.code ?? 'desktop_error'
    this.errorClass = options.errorClass ?? 'system_fault'
    this.retryable = options.retryable ?? false
    if (options.cause !== undefined) {
      ;(this as Error & { cause?: unknown }).cause = options.cause
    }
  }
}

export interface CadenceRuntimeStreamSubscription {
  response: SubscribeRuntimeStreamResponseDto
  unsubscribe: () => void
}

export interface CadenceDesktopAdapter {
  isDesktopRuntime(): boolean
  pickRepositoryFolder(): Promise<string | null>
  importRepository(path: string): Promise<ImportRepositoryResponseDto>
  listProjects(): Promise<ListProjectsResponseDto>
  removeProject(projectId: string): Promise<ListProjectsResponseDto>
  getProjectSnapshot(projectId: string): Promise<ProjectSnapshotResponseDto>
  getRepositoryStatus(projectId: string): Promise<RepositoryStatusResponseDto>
  getRepositoryDiff(projectId: string, scope: RepositoryDiffScope): Promise<RepositoryDiffResponseDto>
  listProjectFiles(projectId: string): Promise<ListProjectFilesResponseDto>
  readProjectFile(projectId: string, path: string): Promise<ReadProjectFileResponseDto>
  writeProjectFile(projectId: string, path: string, content: string): Promise<WriteProjectFileResponseDto>
  createProjectEntry(request: CreateProjectEntryRequestDto): Promise<CreateProjectEntryResponseDto>
  renameProjectEntry(request: RenameProjectEntryRequestDto): Promise<RenameProjectEntryResponseDto>
  deleteProjectEntry(projectId: string, path: string): Promise<DeleteProjectEntryResponseDto>
  getAutonomousRun(projectId: string): Promise<AutonomousRunStateDto>
  getRuntimeRun(projectId: string): Promise<RuntimeRunDto | null>
  getRuntimeSession(projectId: string): Promise<RuntimeSessionDto>
  getRuntimeSettings(): Promise<RuntimeSettingsDto>
  startOpenAiLogin(projectId: string, options?: { originator?: string | null }): Promise<RuntimeSessionDto>
  submitOpenAiCallback(
    projectId: string,
    flowId: string,
    options?: { manualInput?: string | null },
  ): Promise<RuntimeSessionDto>
  startAutonomousRun(projectId: string): Promise<AutonomousRunStateDto>
  startRuntimeRun(projectId: string): Promise<RuntimeRunDto>
  startRuntimeSession(projectId: string): Promise<RuntimeSessionDto>
  cancelAutonomousRun(projectId: string, runId: string): Promise<AutonomousRunStateDto>
  stopRuntimeRun(projectId: string, runId: string): Promise<RuntimeRunDto | null>
  logoutRuntimeSession(projectId: string): Promise<RuntimeSessionDto>
  upsertRuntimeSettings(request: UpsertRuntimeSettingsRequestDto): Promise<RuntimeSettingsDto>
  resolveOperatorAction(
    projectId: string,
    actionId: string,
    decision: 'approve' | 'reject',
    options?: { userAnswer?: string | null },
  ): Promise<ResolveOperatorActionResponseDto>
  resumeOperatorRun(
    projectId: string,
    actionId: string,
    options?: { userAnswer?: string | null },
  ): Promise<ResumeOperatorRunResponseDto>
  listNotificationRoutes(projectId: string): Promise<ListNotificationRoutesResponseDto>
  listNotificationDispatches(
    projectId: string,
    options?: { actionId?: string | null },
  ): Promise<ListNotificationDispatchesResponseDto>
  upsertNotificationRoute(
    request: UpsertNotificationRouteRequestDto,
  ): Promise<UpsertNotificationRouteResponseDto>
  upsertNotificationRouteCredentials(
    request: UpsertNotificationRouteCredentialsRequestDto,
  ): Promise<UpsertNotificationRouteCredentialsResponseDto>
  recordNotificationDispatchOutcome(
    request: RecordNotificationDispatchOutcomeRequestDto,
  ): Promise<RecordNotificationDispatchOutcomeResponseDto>
  submitNotificationReply(request: SubmitNotificationReplyRequestDto): Promise<SubmitNotificationReplyResponseDto>
  syncNotificationAdapters(projectId: string): Promise<SyncNotificationAdaptersResponseDto>
  upsertWorkflowGraph(request: UpsertWorkflowGraphRequestDto): Promise<UpsertWorkflowGraphResponseDto>
  applyWorkflowTransition(request: ApplyWorkflowTransitionRequestDto): Promise<ApplyWorkflowTransitionResponseDto>
  subscribeRuntimeStream(
    projectId: string,
    itemKinds: RuntimeStreamItemKindDto[],
    handler: (payload: RuntimeStreamEventDto) => void,
    onError?: (error: CadenceDesktopError) => void,
  ): Promise<CadenceRuntimeStreamSubscription>
  onProjectUpdated(
    handler: (payload: ProjectUpdatedPayloadDto) => void,
    onError?: (error: CadenceDesktopError) => void,
  ): Promise<UnlistenFn>
  onRepositoryStatusChanged(
    handler: (payload: RepositoryStatusChangedPayloadDto) => void,
    onError?: (error: CadenceDesktopError) => void,
  ): Promise<UnlistenFn>
  onRuntimeUpdated(
    handler: (payload: RuntimeUpdatedPayloadDto) => void,
    onError?: (error: CadenceDesktopError) => void,
  ): Promise<UnlistenFn>
  onRuntimeRunUpdated(
    handler: (payload: RuntimeRunUpdatedPayloadDto) => void,
    onError?: (error: CadenceDesktopError) => void,
  ): Promise<UnlistenFn>
}

function ensureDesktopRuntime(context: string): void {
  if (!isTauri()) {
    throw new CadenceDesktopError({
      code: 'desktop_runtime_unavailable',
      errorClass: 'desktop_runtime_unavailable',
      message: `${context} requires the Tauri desktop runtime.`,
    })
  }
}

function normalizeError(error: unknown, context: string): CadenceDesktopError {
  const commandError = commandErrorSchema.safeParse(error)
  if (commandError.success) {
    return new CadenceDesktopError({
      code: commandError.data.code,
      errorClass: commandError.data.class,
      message: commandError.data.message,
      retryable: commandError.data.retryable,
      cause: error,
    })
  }

  if (error instanceof ZodError) {
    return new CadenceDesktopError({
      code: 'adapter_contract_mismatch',
      errorClass: 'adapter_contract_mismatch',
      message: `${context} returned an unexpected payload shape.`,
      cause: error,
    })
  }

  if (error instanceof CadenceDesktopError) {
    return error
  }

  if (error instanceof Error) {
    return new CadenceDesktopError({
      message: error.message,
      cause: error,
    })
  }

  return new CadenceDesktopError({
    message: `${context} failed for an unknown reason.`,
    cause: error,
  })
}

async function invokeTyped<TResponse>(
  command: string,
  schema: z.ZodType<TResponse>,
  args?: Record<string, unknown>,
): Promise<TResponse> {
  ensureDesktopRuntime(`Command ${command}`)

  try {
    const response = await invoke(command, args)
    return schema.parse(response)
  } catch (error) {
    throw normalizeError(error, `Command ${command}`)
  }
}

async function listenTyped<TPayload>(
  eventName: string,
  schema: z.ZodType<TPayload>,
  handler: (payload: TPayload) => void,
  onError?: (error: CadenceDesktopError) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    return () => undefined
  }

  return listen(eventName, (event) => {
    try {
      handler(schema.parse(event.payload))
    } catch (error) {
      onError?.(normalizeError(error, `Event ${eventName}`))
    }
  })
}

async function createRuntimeStreamSubscription(
  projectId: string,
  itemKinds: RuntimeStreamItemKindDto[],
  handler: (payload: RuntimeStreamEventDto) => void,
  onError?: (error: CadenceDesktopError) => void,
): Promise<CadenceRuntimeStreamSubscription> {
  ensureDesktopRuntime(`Command ${COMMANDS.subscribeRuntimeStream}`)

  let disposed = false
  let response: SubscribeRuntimeStreamResponseDto | null = null
  let lastDeliveredSequence: number | null = null
  const pendingPayloads: unknown[] = []
  const channel = new Channel<unknown>()

  const unsubscribe = () => {
    disposed = true
    lastDeliveredSequence = null
    pendingPayloads.length = 0
    channel.onmessage = () => undefined
  }

  const deliver = (payload: unknown, activeResponse: SubscribeRuntimeStreamResponseDto) => {
    if (disposed) {
      return
    }

    try {
      const item = runtimeStreamItemSchema.parse(payload)
      if (item.runId !== activeResponse.runId) {
        throw new CadenceDesktopError({
          code: 'adapter_contract_mismatch',
          errorClass: 'adapter_contract_mismatch',
          message: `Command ${COMMANDS.subscribeRuntimeStream} channel returned a stream item for run ${item.runId} while ${activeResponse.runId} is subscribed.`,
        })
      }

      if (lastDeliveredSequence !== null) {
        if (item.sequence < lastDeliveredSequence) {
          throw new CadenceDesktopError({
            code: 'adapter_contract_mismatch',
            errorClass: 'adapter_contract_mismatch',
            message: `Command ${COMMANDS.subscribeRuntimeStream} channel returned non-monotonic sequence ${item.sequence} after ${lastDeliveredSequence} for run ${item.runId}.`,
          })
        }

        if (item.sequence === lastDeliveredSequence) {
          return
        }
      }

      lastDeliveredSequence = item.sequence

      handler({
        projectId: activeResponse.projectId,
        runtimeKind: activeResponse.runtimeKind,
        runId: activeResponse.runId,
        sessionId: activeResponse.sessionId,
        flowId: activeResponse.flowId ?? null,
        subscribedItemKinds: activeResponse.subscribedItemKinds,
        item,
      })
    } catch (error) {
      onError?.(normalizeError(error, `Command ${COMMANDS.subscribeRuntimeStream} channel`))
    }
  }

  channel.onmessage = (payload) => {
    if (disposed) {
      return
    }

    if (!response) {
      pendingPayloads.push(payload)
      return
    }

    deliver(payload, response)
  }

  try {
    const request = subscribeRuntimeStreamRequestSchema.parse({ projectId, itemKinds })
    response = await invokeTyped(COMMANDS.subscribeRuntimeStream, subscribeRuntimeStreamResponseSchema, {
      request: {
        projectId: request.projectId,
        itemKinds: request.itemKinds,
        channel,
      },
    })

    for (const pendingPayload of pendingPayloads.splice(0, pendingPayloads.length)) {
      deliver(pendingPayload, response)
    }

    return {
      response,
      unsubscribe,
    }
  } catch (error) {
    unsubscribe()
    throw normalizeError(error, `Command ${COMMANDS.subscribeRuntimeStream}`)
  }
}

export const CadenceDesktopAdapter: CadenceDesktopAdapter = {
  isDesktopRuntime() {
    return isTauri()
  },

  async pickRepositoryFolder() {
    ensureDesktopRuntime('Repository import')

    try {
      const selected = await open({
        directory: true,
        multiple: false,
      })

      if (selected === null) {
        return null
      }

      const path = Array.isArray(selected) ? selected[0] : selected
      return typeof path === 'string' && path.trim().length > 0 ? path : null
    } catch (error) {
      throw normalizeError(error, 'Repository import')
    }
  },

  importRepository(path) {
    return invokeTyped(COMMANDS.importRepository, importRepositoryResponseSchema, {
      request: { path },
    })
  },

  listProjects() {
    return invokeTyped(COMMANDS.listProjects, listProjectsResponseSchema)
  },

  removeProject(projectId) {
    return invokeTyped(COMMANDS.removeProject, listProjectsResponseSchema, {
      request: { projectId },
    })
  },

  getProjectSnapshot(projectId) {
    return invokeTyped(COMMANDS.getProjectSnapshot, projectSnapshotResponseSchema, {
      request: { projectId },
    })
  },

  getRepositoryStatus(projectId) {
    return invokeTyped(COMMANDS.getRepositoryStatus, repositoryStatusResponseSchema, {
      request: { projectId },
    })
  },

  getRepositoryDiff(projectId, scope) {
    return invokeTyped(COMMANDS.getRepositoryDiff, repositoryDiffResponseSchema, {
      request: { projectId, scope },
    })
  },

  listProjectFiles(projectId) {
    return invokeTyped(COMMANDS.listProjectFiles, listProjectFilesResponseSchema, {
      request: { projectId },
    })
  },

  readProjectFile(projectId, path) {
    const request = projectFileRequestSchema.parse({ projectId, path })
    return invokeTyped(COMMANDS.readProjectFile, readProjectFileResponseSchema, {
      request,
    })
  },

  writeProjectFile(projectId, path, content) {
    const request = writeProjectFileRequestSchema.parse({ projectId, path, content })
    return invokeTyped(COMMANDS.writeProjectFile, writeProjectFileResponseSchema, {
      request,
    })
  },

  createProjectEntry(request) {
    const parsedRequest = createProjectEntryRequestSchema.parse(request)
    return invokeTyped(COMMANDS.createProjectEntry, createProjectEntryResponseSchema, {
      request: parsedRequest,
    })
  },

  renameProjectEntry(request) {
    const parsedRequest = renameProjectEntryRequestSchema.parse(request)
    return invokeTyped(COMMANDS.renameProjectEntry, renameProjectEntryResponseSchema, {
      request: parsedRequest,
    })
  },

  deleteProjectEntry(projectId, path) {
    const request = projectFileRequestSchema.parse({ projectId, path })
    return invokeTyped(COMMANDS.deleteProjectEntry, deleteProjectEntryResponseSchema, {
      request,
    })
  },

  getAutonomousRun(projectId) {
    return invokeTyped(COMMANDS.getAutonomousRun, autonomousRunStateSchema, {
      request: { projectId },
    })
  },

  getRuntimeRun(projectId) {
    return invokeTyped(COMMANDS.getRuntimeRun, runtimeRunSchema.nullable(), {
      request: { projectId },
    })
  },

  getRuntimeSession(projectId) {
    return invokeTyped(COMMANDS.getRuntimeSession, runtimeSessionSchema, {
      request: { projectId },
    })
  },

  getRuntimeSettings() {
    return invokeTyped(COMMANDS.getRuntimeSettings, runtimeSettingsSchema)
  },

  startOpenAiLogin(projectId, options) {
    return invokeTyped(COMMANDS.startOpenAiLogin, runtimeSessionSchema, {
      request: {
        projectId,
        originator: options?.originator ?? null,
      },
    })
  },

  submitOpenAiCallback(projectId, flowId, options) {
    return invokeTyped(COMMANDS.submitOpenAiCallback, runtimeSessionSchema, {
      request: {
        projectId,
        flowId,
        manualInput: options?.manualInput ?? null,
      },
    })
  },

  startAutonomousRun(projectId) {
    return invokeTyped(COMMANDS.startAutonomousRun, autonomousRunStateSchema, {
      request: { projectId },
    })
  },

  startRuntimeRun(projectId) {
    return invokeTyped(COMMANDS.startRuntimeRun, runtimeRunSchema, {
      request: { projectId },
    })
  },

  startRuntimeSession(projectId) {
    return invokeTyped(COMMANDS.startRuntimeSession, runtimeSessionSchema, {
      request: { projectId },
    })
  },

  cancelAutonomousRun(projectId, runId) {
    return invokeTyped(COMMANDS.cancelAutonomousRun, autonomousRunStateSchema, {
      request: { projectId, runId },
    })
  },

  stopRuntimeRun(projectId, runId) {
    return invokeTyped(COMMANDS.stopRuntimeRun, runtimeRunSchema.nullable(), {
      request: { projectId, runId },
    })
  },

  logoutRuntimeSession(projectId) {
    return invokeTyped(COMMANDS.logoutRuntimeSession, runtimeSessionSchema, {
      request: { projectId },
    })
  },

  upsertRuntimeSettings(request) {
    const parsedRequest = upsertRuntimeSettingsRequestSchema.parse(request)
    return invokeTyped(COMMANDS.upsertRuntimeSettings, runtimeSettingsSchema, {
      request: parsedRequest,
    })
  },

  resolveOperatorAction(projectId, actionId, decision, options) {
    const request = resolveOperatorActionRequestSchema.parse({
      projectId,
      actionId,
      decision,
      userAnswer: options?.userAnswer ?? null,
    })

    return invokeTyped(COMMANDS.resolveOperatorAction, resolveOperatorActionResponseSchema, {
      request,
    })
  },

  resumeOperatorRun(projectId, actionId, options) {
    const request = resumeOperatorRunRequestSchema.parse({
      projectId,
      actionId,
      userAnswer: options?.userAnswer ?? null,
    })

    return invokeTyped(COMMANDS.resumeOperatorRun, resumeOperatorRunResponseSchema, {
      request,
    })
  },

  listNotificationRoutes(projectId) {
    const request = listNotificationRoutesRequestSchema.parse({ projectId })
    return invokeTyped(COMMANDS.listNotificationRoutes, listNotificationRoutesResponseSchema, {
      request,
    })
  },

  listNotificationDispatches(projectId, options) {
    const request = listNotificationDispatchesRequestSchema.parse({
      projectId,
      actionId: options?.actionId ?? null,
    })

    return invokeTyped(COMMANDS.listNotificationDispatches, listNotificationDispatchesResponseSchema, {
      request,
    })
  },

  upsertNotificationRoute(request) {
    const parsedRequest = upsertNotificationRouteRequestSchema.parse(request)
    return invokeTyped(COMMANDS.upsertNotificationRoute, upsertNotificationRouteResponseSchema, {
      request: parsedRequest,
    })
  },

  upsertNotificationRouteCredentials(request) {
    const parsedRequest = upsertNotificationRouteCredentialsRequestSchema.parse(request)
    return invokeTyped(
      COMMANDS.upsertNotificationRouteCredentials,
      upsertNotificationRouteCredentialsResponseSchema,
      {
        request: parsedRequest,
      },
    )
  },

  recordNotificationDispatchOutcome(request) {
    const parsedRequest = recordNotificationDispatchOutcomeRequestSchema.parse(request)
    return invokeTyped(
      COMMANDS.recordNotificationDispatchOutcome,
      recordNotificationDispatchOutcomeResponseSchema,
      {
        request: parsedRequest,
      },
    )
  },

  submitNotificationReply(request) {
    const parsedRequest = submitNotificationReplyRequestSchema.parse(request)
    return invokeTyped(COMMANDS.submitNotificationReply, submitNotificationReplyResponseSchema, {
      request: parsedRequest,
    })
  },

  syncNotificationAdapters(projectId) {
    const request = syncNotificationAdaptersRequestSchema.parse({ projectId })
    return invokeTyped(COMMANDS.syncNotificationAdapters, syncNotificationAdaptersResponseSchema, {
      request,
    })
  },

  upsertWorkflowGraph(request) {
    const parsedRequest = upsertWorkflowGraphRequestSchema.parse(request)
    return invokeTyped(COMMANDS.upsertWorkflowGraph, upsertWorkflowGraphResponseSchema, {
      request: parsedRequest,
    })
  },

  applyWorkflowTransition(request) {
    const parsedRequest = applyWorkflowTransitionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.applyWorkflowTransition, applyWorkflowTransitionResponseSchema, {
      request: parsedRequest,
    })
  },

  subscribeRuntimeStream(projectId, itemKinds, handler, onError) {
    return createRuntimeStreamSubscription(projectId, itemKinds, handler, onError)
  },

  onProjectUpdated(handler, onError) {
    return listenTyped(EVENTS.projectUpdated, projectUpdatedPayloadSchema, handler, onError)
  },

  onRepositoryStatusChanged(handler, onError) {
    return listenTyped(EVENTS.repositoryStatusChanged, repositoryStatusChangedPayloadSchema, handler, onError)
  },

  onRuntimeUpdated(handler, onError) {
    return listenTyped(EVENTS.runtimeUpdated, runtimeUpdatedPayloadSchema, handler, onError)
  },

  onRuntimeRunUpdated(handler, onError) {
    return listenTyped(EVENTS.runtimeRunUpdated, runtimeRunUpdatedPayloadSchema, handler, onError)
  },
}

export function getDesktopErrorMessage(error: unknown): string {
  return normalizeError(error, 'Cadence desktop state').message
}
