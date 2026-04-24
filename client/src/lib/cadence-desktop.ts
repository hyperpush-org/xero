import { Channel, invoke, isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { ZodError, z } from 'zod'
import { autonomousRunStateSchema, type AutonomousRunStateDto } from '@/src/lib/cadence-model/autonomous'
import {
  listNotificationDispatchesRequestSchema,
  listNotificationDispatchesResponseSchema,
  listNotificationRoutesRequestSchema,
  listNotificationRoutesResponseSchema,
  recordNotificationDispatchOutcomeRequestSchema,
  recordNotificationDispatchOutcomeResponseSchema,
  submitNotificationReplyRequestSchema,
  submitNotificationReplyResponseSchema,
  syncNotificationAdaptersRequestSchema,
  syncNotificationAdaptersResponseSchema,
  upsertNotificationRouteCredentialsRequestSchema,
  upsertNotificationRouteCredentialsResponseSchema,
  upsertNotificationRouteRequestSchema,
  upsertNotificationRouteResponseSchema,
  type ListNotificationDispatchesResponseDto,
  type ListNotificationRoutesResponseDto,
  type RecordNotificationDispatchOutcomeRequestDto,
  type RecordNotificationDispatchOutcomeResponseDto,
  type SubmitNotificationReplyRequestDto,
  type SubmitNotificationReplyResponseDto,
  type SyncNotificationAdaptersResponseDto,
  type UpsertNotificationRouteCredentialsRequestDto,
  type UpsertNotificationRouteCredentialsResponseDto,
  type UpsertNotificationRouteRequestDto,
  type UpsertNotificationRouteResponseDto,
} from '@/src/lib/cadence-model/notifications'
import {
  importMcpServersRequestSchema,
  importMcpServersResponseSchema,
  mcpRegistrySchema,
  refreshMcpServerStatusesRequestSchema,
  removeMcpServerRequestSchema,
  upsertMcpServerRequestSchema,
  type ImportMcpServersResponseDto,
  type McpRegistryDto,
  type UpsertMcpServerRequestDto,
} from '@/src/lib/cadence-model/mcp'
import {
  resolveOperatorActionRequestSchema,
  resolveOperatorActionResponseSchema,
  resumeOperatorRunRequestSchema,
  resumeOperatorRunResponseSchema,
  type ResolveOperatorActionResponseDto,
  type ResumeOperatorRunResponseDto,
} from '@/src/lib/cadence-model/operator-actions'
import {
  createProjectEntryRequestSchema,
  createProjectEntryResponseSchema,
  deleteProjectEntryResponseSchema,
  importRepositoryResponseSchema,
  listProjectFilesResponseSchema,
  listProjectsResponseSchema,
  projectFileRequestSchema,
  projectUpdatedPayloadSchema,
  readProjectFileResponseSchema,
  renameProjectEntryRequestSchema,
  renameProjectEntryResponseSchema,
  replaceInProjectRequestSchema,
  replaceInProjectResponseSchema,
  repositoryDiffResponseSchema,
  repositoryStatusChangedPayloadSchema,
  repositoryStatusResponseSchema,
  searchProjectRequestSchema,
  searchProjectResponseSchema,
  writeProjectFileRequestSchema,
  writeProjectFileResponseSchema,
  type CreateProjectEntryRequestDto,
  type CreateProjectEntryResponseDto,
  type DeleteProjectEntryResponseDto,
  type ImportRepositoryResponseDto,
  type ListProjectFilesResponseDto,
  type ListProjectsResponseDto,
  type ProjectFileRequestDto,
  type ProjectUpdatedPayloadDto,
  type ReadProjectFileResponseDto,
  type RenameProjectEntryRequestDto,
  type RenameProjectEntryResponseDto,
  type ReplaceInProjectRequestDto,
  type ReplaceInProjectResponseDto,
  type RepositoryDiffResponseDto,
  type RepositoryDiffScope,
  type RepositoryStatusChangedPayloadDto,
  type RepositoryStatusResponseDto,
  type SearchProjectRequestDto,
  type SearchProjectResponseDto,
  type WriteProjectFileRequestDto,
  type WriteProjectFileResponseDto,
} from '@/src/lib/cadence-model/project'
import {
  runtimeRunSchema,
  runtimeRunUpdatedPayloadSchema,
  runtimeSessionSchema,
  runtimeSettingsSchema,
  runtimeUpdatedPayloadSchema,
  startRuntimeRunRequestSchema,
  updateRuntimeRunControlsRequestSchema,
  upsertRuntimeSettingsRequestSchema,
  type RuntimeRunControlInputDto,
  type RuntimeRunDto,
  type RuntimeRunUpdatedPayloadDto,
  type RuntimeSessionDto,
  type RuntimeSettingsDto,
  type RuntimeUpdatedPayloadDto,
  type StartRuntimeRunRequestDto,
  type UpdateRuntimeRunControlsRequestDto,
  type UpsertRuntimeSettingsRequestDto,
} from '@/src/lib/cadence-model/runtime'
import {
  providerProfilesSchema,
  setActiveProviderProfileRequestSchema,
  upsertProviderProfileRequestSchema,
  type ProviderProfilesDto,
  type UpsertProviderProfileRequestDto,
} from '@/src/lib/cadence-model/provider-profiles'
import {
  createProviderModelCatalogRequest,
  providerModelCatalogSchema,
  type ProviderModelCatalogDto,
} from '@/src/lib/cadence-model/provider-models'
import {
  runtimeStreamItemSchema,
  subscribeRuntimeStreamRequestSchema,
  subscribeRuntimeStreamResponseSchema,
  type RuntimeStreamEventDto,
  type RuntimeStreamItemKindDto,
  type SubscribeRuntimeStreamResponseDto,
} from '@/src/lib/cadence-model/runtime-stream'
import {
  applyWorkflowTransitionRequestSchema,
  applyWorkflowTransitionResponseSchema,
  upsertWorkflowGraphRequestSchema,
  upsertWorkflowGraphResponseSchema,
  type ApplyWorkflowTransitionRequestDto,
  type ApplyWorkflowTransitionResponseDto,
  type UpsertWorkflowGraphRequestDto,
  type UpsertWorkflowGraphResponseDto,
} from '@/src/lib/cadence-model/workflow'
import { projectSnapshotResponseSchema, type ProjectSnapshotResponseDto } from '@/src/lib/cadence-model'

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
  searchProject: 'search_project',
  replaceInProject: 'replace_in_project',
  getAutonomousRun: 'get_autonomous_run',
  getRuntimeRun: 'get_runtime_run',
  getRuntimeSession: 'get_runtime_session',
  getRuntimeSettings: 'get_runtime_settings',
  listMcpServers: 'list_mcp_servers',
  upsertMcpServer: 'upsert_mcp_server',
  removeMcpServer: 'remove_mcp_server',
  importMcpServers: 'import_mcp_servers',
  refreshMcpServerStatuses: 'refresh_mcp_server_statuses',
  getProviderModelCatalog: 'get_provider_model_catalog',
  listProviderProfiles: 'list_provider_profiles',
  upsertProviderProfile: 'upsert_provider_profile',
  setActiveProviderProfile: 'set_active_provider_profile',
  startOpenAiLogin: 'start_openai_login',
  submitOpenAiCallback: 'submit_openai_callback',
  startAutonomousRun: 'start_autonomous_run',
  startRuntimeRun: 'start_runtime_run',
  updateRuntimeRunControls: 'update_runtime_run_controls',
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
  browserShow: 'browser_show',
  browserResize: 'browser_resize',
  browserHide: 'browser_hide',
  browserEval: 'browser_eval',
  browserCurrentUrl: 'browser_current_url',
  browserScreenshot: 'browser_screenshot',
  browserNavigate: 'browser_navigate',
  browserBack: 'browser_back',
  browserForward: 'browser_forward',
  browserReload: 'browser_reload',
  browserStop: 'browser_stop',
  browserClick: 'browser_click',
  browserType: 'browser_type',
  browserScroll: 'browser_scroll',
  browserPressKey: 'browser_press_key',
  browserReadText: 'browser_read_text',
  browserQuery: 'browser_query',
  browserWaitForSelector: 'browser_wait_for_selector',
  browserWaitForLoad: 'browser_wait_for_load',
  browserHistoryState: 'browser_history_state',
  browserCookiesGet: 'browser_cookies_get',
  browserCookiesSet: 'browser_cookies_set',
  browserStorageRead: 'browser_storage_read',
  browserStorageWrite: 'browser_storage_write',
  browserStorageClear: 'browser_storage_clear',
  browserTabList: 'browser_tab_list',
  browserTabFocus: 'browser_tab_focus',
  browserTabClose: 'browser_tab_close',
} as const

const EVENTS = {
  projectUpdated: 'project:updated',
  repositoryStatusChanged: 'repository:status_changed',
  runtimeUpdated: 'runtime:updated',
  runtimeRunUpdated: 'runtime_run:updated',
  browserUrlChanged: 'browser:url_changed',
  browserLoadState: 'browser:load_state',
  browserConsole: 'browser:console',
  browserTabUpdated: 'browser:tab_updated',
} as const

const commandErrorSchema = z.object({
  code: z.string(),
  class: z.enum(['user_fixable', 'retryable', 'system_fault', 'policy_denied']),
  message: z.string(),
  retryable: z.boolean(),
})

const browserEvalResponseSchema = z.unknown()
const browserCurrentUrlResponseSchema = z.string().nullable()
const browserScreenshotResponseSchema = z.string()
const browserVoidSchema = z.null().optional().transform(() => undefined)
const browserJsonSchema = z.unknown()

export const browserTabMetadataSchema = z
  .object({
    id: z.string(),
    label: z.string(),
    title: z.string().nullable(),
    url: z.string().nullable(),
    loading: z.boolean(),
    canGoBack: z.boolean(),
    canGoForward: z.boolean(),
    active: z.boolean(),
  })
  .strict()
export type BrowserTabMetadataDto = z.infer<typeof browserTabMetadataSchema>

const browserTabListSchema = z.array(browserTabMetadataSchema)

export const browserUrlChangedPayloadSchema = z
  .object({
    tabId: z.string(),
    url: z.string(),
    title: z.string().nullable(),
    canGoBack: z.boolean(),
    canGoForward: z.boolean(),
  })
  .strict()
export type BrowserUrlChangedPayload = z.infer<typeof browserUrlChangedPayloadSchema>

export const browserLoadStatePayloadSchema = z
  .object({
    tabId: z.string(),
    loading: z.boolean(),
    url: z.string().nullable(),
    error: z.string().nullable(),
  })
  .strict()
export type BrowserLoadStatePayload = z.infer<typeof browserLoadStatePayloadSchema>

export const browserConsolePayloadSchema = z
  .object({
    tabId: z.string(),
    level: z.string(),
    message: z.string(),
  })
  .strict()
export type BrowserConsolePayload = z.infer<typeof browserConsolePayloadSchema>

export const browserTabUpdatedPayloadSchema = z
  .object({
    tabs: browserTabListSchema,
  })
  .strict()
export type BrowserTabUpdatedPayload = z.infer<typeof browserTabUpdatedPayloadSchema>

const startOpenAiLoginRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    profileId: z.string().trim().min(1),
    originator: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

const submitOpenAiCallbackRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    profileId: z.string().trim().min(1),
    flowId: z.string().trim().min(1),
    manualInput: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export interface StartOpenAiLoginOptions {
  selectedProfileId: string
  originator?: string | null
}

export interface SubmitOpenAiCallbackOptions {
  selectedProfileId: string
  manualInput?: string | null
}

export interface StartRuntimeRunOptions {
  initialControls?: RuntimeRunControlInputDto | null
  initialPrompt?: string | null
}

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
  searchProject(request: SearchProjectRequestDto): Promise<SearchProjectResponseDto>
  replaceInProject(request: ReplaceInProjectRequestDto): Promise<ReplaceInProjectResponseDto>
  getAutonomousRun(projectId: string): Promise<AutonomousRunStateDto>
  getRuntimeRun(projectId: string): Promise<RuntimeRunDto | null>
  getRuntimeSession(projectId: string): Promise<RuntimeSessionDto>
  getRuntimeSettings(): Promise<RuntimeSettingsDto>
  listMcpServers(): Promise<McpRegistryDto>
  upsertMcpServer(request: UpsertMcpServerRequestDto): Promise<McpRegistryDto>
  removeMcpServer(serverId: string): Promise<McpRegistryDto>
  importMcpServers(path: string): Promise<ImportMcpServersResponseDto>
  refreshMcpServerStatuses(options?: { serverIds?: string[] }): Promise<McpRegistryDto>
  getProviderModelCatalog(
    profileId: string,
    options?: { forceRefresh?: boolean },
  ): Promise<ProviderModelCatalogDto>
  getProviderProfiles(): Promise<ProviderProfilesDto>
  startOpenAiLogin(projectId: string, options: StartOpenAiLoginOptions): Promise<RuntimeSessionDto>
  submitOpenAiCallback(
    projectId: string,
    flowId: string,
    options: SubmitOpenAiCallbackOptions,
  ): Promise<RuntimeSessionDto>
  startAutonomousRun(projectId: string): Promise<AutonomousRunStateDto>
  startRuntimeRun(projectId: string, options?: StartRuntimeRunOptions): Promise<RuntimeRunDto>
  updateRuntimeRunControls(request: UpdateRuntimeRunControlsRequestDto): Promise<RuntimeRunDto>
  startRuntimeSession(projectId: string): Promise<RuntimeSessionDto>
  cancelAutonomousRun(projectId: string, runId: string): Promise<AutonomousRunStateDto>
  stopRuntimeRun(projectId: string, runId: string): Promise<RuntimeRunDto | null>
  logoutRuntimeSession(projectId: string): Promise<RuntimeSessionDto>
  upsertRuntimeSettings(request: UpsertRuntimeSettingsRequestDto): Promise<RuntimeSettingsDto>
  upsertProviderProfile(request: UpsertProviderProfileRequestDto): Promise<ProviderProfilesDto>
  setActiveProviderProfile(profileId: string): Promise<ProviderProfilesDto>
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
  browserEval(js: string, options?: { timeoutMs?: number }): Promise<unknown>
  browserCurrentUrl(): Promise<string | null>
  browserScreenshot(): Promise<string>
  browserNavigate(url: string, options?: { tabId?: string }): Promise<void>
  browserBack(): Promise<unknown>
  browserForward(): Promise<unknown>
  browserReload(options?: { tabId?: string }): Promise<void>
  browserStop(): Promise<unknown>
  browserClick(selector: string, options?: { timeoutMs?: number }): Promise<unknown>
  browserType(
    selector: string,
    text: string,
    options?: { append?: boolean; timeoutMs?: number },
  ): Promise<unknown>
  browserScroll(options?: {
    selector?: string
    x?: number
    y?: number
    timeoutMs?: number
  }): Promise<unknown>
  browserPressKey(
    key: string,
    options?: { selector?: string; timeoutMs?: number },
  ): Promise<unknown>
  browserReadText(options?: { selector?: string; timeoutMs?: number }): Promise<unknown>
  browserQuery(
    selector: string,
    options?: { limit?: number; timeoutMs?: number },
  ): Promise<unknown>
  browserWaitForSelector(
    selector: string,
    options?: { timeoutMs?: number; visible?: boolean },
  ): Promise<unknown>
  browserWaitForLoad(options?: { timeoutMs?: number }): Promise<unknown>
  browserHistoryState(): Promise<unknown>
  browserCookiesGet(): Promise<unknown>
  browserCookiesSet(cookie: string): Promise<unknown>
  browserStorageRead(area: 'local' | 'session', key?: string): Promise<unknown>
  browserStorageWrite(area: 'local' | 'session', key: string, value: string | null): Promise<unknown>
  browserStorageClear(area: 'local' | 'session'): Promise<unknown>
  browserTabList(): Promise<BrowserTabMetadataDto[]>
  browserTabFocus(tabId: string): Promise<BrowserTabMetadataDto>
  browserTabClose(tabId: string): Promise<BrowserTabMetadataDto[]>
  onBrowserUrlChanged(
    handler: (payload: BrowserUrlChangedPayload) => void,
    onError?: (error: CadenceDesktopError) => void,
  ): Promise<UnlistenFn>
  onBrowserLoadState(
    handler: (payload: BrowserLoadStatePayload) => void,
    onError?: (error: CadenceDesktopError) => void,
  ): Promise<UnlistenFn>
  onBrowserConsole(
    handler: (payload: BrowserConsolePayload) => void,
    onError?: (error: CadenceDesktopError) => void,
  ): Promise<UnlistenFn>
  onBrowserTabUpdated(
    handler: (payload: BrowserTabUpdatedPayload) => void,
    onError?: (error: CadenceDesktopError) => void,
  ): Promise<UnlistenFn>
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

  searchProject(request) {
    const parsed = searchProjectRequestSchema.parse(request)
    return invokeTyped(COMMANDS.searchProject, searchProjectResponseSchema, {
      request: parsed,
    })
  },

  replaceInProject(request) {
    const parsed = replaceInProjectRequestSchema.parse(request)
    return invokeTyped(COMMANDS.replaceInProject, replaceInProjectResponseSchema, {
      request: parsed,
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

  listMcpServers() {
    return invokeTyped(COMMANDS.listMcpServers, mcpRegistrySchema)
  },

  upsertMcpServer(request) {
    const parsedRequest = upsertMcpServerRequestSchema.parse(request)
    return invokeTyped(COMMANDS.upsertMcpServer, mcpRegistrySchema, {
      request: parsedRequest,
    })
  },

  removeMcpServer(serverId) {
    const request = removeMcpServerRequestSchema.parse({ serverId })
    return invokeTyped(COMMANDS.removeMcpServer, mcpRegistrySchema, {
      request,
    })
  },

  importMcpServers(path) {
    const request = importMcpServersRequestSchema.parse({ path })
    return invokeTyped(COMMANDS.importMcpServers, importMcpServersResponseSchema, {
      request,
    })
  },

  refreshMcpServerStatuses(options) {
    const request = refreshMcpServerStatusesRequestSchema.parse({
      serverIds: options?.serverIds ?? [],
    })

    return invokeTyped(COMMANDS.refreshMcpServerStatuses, mcpRegistrySchema, {
      request,
    })
  },

  getProviderModelCatalog(profileId, options) {
    const request = createProviderModelCatalogRequest(profileId, {
      forceRefresh: options?.forceRefresh ?? false,
    })
    return invokeTyped(COMMANDS.getProviderModelCatalog, providerModelCatalogSchema, {
      request,
    })
  },

  getProviderProfiles() {
    return invokeTyped(COMMANDS.listProviderProfiles, providerProfilesSchema)
  },

  startOpenAiLogin(projectId, options) {
    const request = startOpenAiLoginRequestSchema.parse({
      projectId,
      profileId: options.selectedProfileId,
      originator: options.originator ?? null,
    })

    return invokeTyped(COMMANDS.startOpenAiLogin, runtimeSessionSchema, {
      request: {
        projectId: request.projectId,
        profileId: request.profileId,
        originator: request.originator ?? null,
      },
    })
  },

  submitOpenAiCallback(projectId, flowId, options) {
    const request = submitOpenAiCallbackRequestSchema.parse({
      projectId,
      profileId: options.selectedProfileId,
      flowId,
      manualInput: options.manualInput ?? null,
    })

    return invokeTyped(COMMANDS.submitOpenAiCallback, runtimeSessionSchema, {
      request: {
        projectId: request.projectId,
        profileId: request.profileId,
        flowId: request.flowId,
        manualInput: request.manualInput ?? null,
      },
    })
  },

  startAutonomousRun(projectId) {
    return invokeTyped(COMMANDS.startAutonomousRun, autonomousRunStateSchema, {
      request: { projectId },
    })
  },

  startRuntimeRun(projectId, options) {
    const request: StartRuntimeRunRequestDto = startRuntimeRunRequestSchema.parse({
      projectId,
      initialControls: options?.initialControls ?? null,
      initialPrompt: options?.initialPrompt ?? null,
    })

    return invokeTyped(COMMANDS.startRuntimeRun, runtimeRunSchema, {
      request,
    })
  },

  updateRuntimeRunControls(request) {
    const parsedRequest = updateRuntimeRunControlsRequestSchema.parse(request)
    return invokeTyped(COMMANDS.updateRuntimeRunControls, runtimeRunSchema, {
      request: parsedRequest,
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

  upsertProviderProfile(request) {
    const parsedRequest = upsertProviderProfileRequestSchema.parse(request)
    return invokeTyped(COMMANDS.upsertProviderProfile, providerProfilesSchema, {
      request: parsedRequest,
    })
  },

  setActiveProviderProfile(profileId) {
    const request = setActiveProviderProfileRequestSchema.parse({ profileId })
    return invokeTyped(COMMANDS.setActiveProviderProfile, providerProfilesSchema, {
      request,
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

  async browserEval(js, options) {
    if (typeof js !== 'string' || js.trim().length === 0) {
      throw new CadenceDesktopError({
        code: 'invalid_request',
        errorClass: 'user_fixable',
        message: 'browserEval requires a non-empty `js` string.',
      })
    }
    return invokeTyped(COMMANDS.browserEval, browserEvalResponseSchema, {
      js,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserCurrentUrl() {
    return invokeTyped(COMMANDS.browserCurrentUrl, browserCurrentUrlResponseSchema)
  },

  browserScreenshot() {
    return invokeTyped(COMMANDS.browserScreenshot, browserScreenshotResponseSchema)
  },

  async browserNavigate(url, options) {
    await invokeTyped(COMMANDS.browserNavigate, browserVoidSchema, {
      url,
      tab_id: options?.tabId ?? null,
    })
  },

  browserBack() {
    return invokeTyped(COMMANDS.browserBack, browserJsonSchema)
  },

  browserForward() {
    return invokeTyped(COMMANDS.browserForward, browserJsonSchema)
  },

  async browserReload(options) {
    await invokeTyped(COMMANDS.browserReload, browserVoidSchema, {
      tab_id: options?.tabId ?? null,
    })
  },

  browserStop() {
    return invokeTyped(COMMANDS.browserStop, browserJsonSchema)
  },

  browserClick(selector, options) {
    return invokeTyped(COMMANDS.browserClick, browserJsonSchema, {
      selector,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserType(selector, text, options) {
    return invokeTyped(COMMANDS.browserType, browserJsonSchema, {
      selector,
      text,
      append: options?.append ?? null,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserScroll(options) {
    return invokeTyped(COMMANDS.browserScroll, browserJsonSchema, {
      selector: options?.selector ?? null,
      x: options?.x ?? null,
      y: options?.y ?? null,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserPressKey(key, options) {
    return invokeTyped(COMMANDS.browserPressKey, browserJsonSchema, {
      key,
      selector: options?.selector ?? null,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserReadText(options) {
    return invokeTyped(COMMANDS.browserReadText, browserJsonSchema, {
      selector: options?.selector ?? null,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserQuery(selector, options) {
    return invokeTyped(COMMANDS.browserQuery, browserJsonSchema, {
      selector,
      limit: options?.limit ?? null,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserWaitForSelector(selector, options) {
    return invokeTyped(COMMANDS.browserWaitForSelector, browserJsonSchema, {
      selector,
      timeout_ms: options?.timeoutMs ?? null,
      visible: options?.visible ?? null,
    })
  },

  browserWaitForLoad(options) {
    return invokeTyped(COMMANDS.browserWaitForLoad, browserJsonSchema, {
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserHistoryState() {
    return invokeTyped(COMMANDS.browserHistoryState, browserJsonSchema)
  },

  browserCookiesGet() {
    return invokeTyped(COMMANDS.browserCookiesGet, browserJsonSchema)
  },

  browserCookiesSet(cookie) {
    return invokeTyped(COMMANDS.browserCookiesSet, browserJsonSchema, { cookie })
  },

  browserStorageRead(area, key) {
    return invokeTyped(COMMANDS.browserStorageRead, browserJsonSchema, {
      area,
      key: key ?? null,
    })
  },

  browserStorageWrite(area, key, value) {
    return invokeTyped(COMMANDS.browserStorageWrite, browserJsonSchema, {
      area,
      key,
      value,
    })
  },

  browserStorageClear(area) {
    return invokeTyped(COMMANDS.browserStorageClear, browserJsonSchema, { area })
  },

  browserTabList() {
    return invokeTyped(COMMANDS.browserTabList, browserTabListSchema)
  },

  browserTabFocus(tabId) {
    return invokeTyped(COMMANDS.browserTabFocus, browserTabMetadataSchema, { tab_id: tabId })
  },

  browserTabClose(tabId) {
    return invokeTyped(COMMANDS.browserTabClose, browserTabListSchema, { tab_id: tabId })
  },

  onBrowserUrlChanged(handler, onError) {
    return listenTyped(EVENTS.browserUrlChanged, browserUrlChangedPayloadSchema, handler, onError)
  },

  onBrowserLoadState(handler, onError) {
    return listenTyped(EVENTS.browserLoadState, browserLoadStatePayloadSchema, handler, onError)
  },

  onBrowserConsole(handler, onError) {
    return listenTyped(EVENTS.browserConsole, browserConsolePayloadSchema, handler, onError)
  },

  onBrowserTabUpdated(handler, onError) {
    return listenTyped(EVENTS.browserTabUpdated, browserTabUpdatedPayloadSchema, handler, onError)
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
