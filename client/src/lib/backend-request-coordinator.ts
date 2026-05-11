export class StaleBackendRequestError extends Error {
  scope: string

  constructor(scope: string, options: { cause?: unknown } = {}) {
    super(`Stale backend request ignored for ${scope}.`)
    this.name = 'StaleBackendRequestError'
    this.scope = scope
    if (options.cause !== undefined) {
      ;(this as Error & { cause?: unknown }).cause = options.cause
    }
  }
}

export interface BackendRequestCoordinator {
  cancelScope(scope: string): void
  runDeduped<T>(requestKey: string, work: () => Promise<T>): Promise<T>
  runLatest<T>(scope: string, requestKey: string, work: () => Promise<T>): Promise<T>
}

export function isStaleBackendRequestError(error: unknown): error is StaleBackendRequestError {
  return error instanceof StaleBackendRequestError
}

export function createBackendRequestCoordinator(): BackendRequestCoordinator {
  const inFlight = new Map<string, Promise<unknown>>()
  const latestByScope = new Map<string, number>()

  function nextSequence(scope: string): number {
    const next = (latestByScope.get(scope) ?? 0) + 1
    latestByScope.set(scope, next)
    return next
  }

  function isLatest(scope: string, sequence: number): boolean {
    return latestByScope.get(scope) === sequence
  }

  function runDeduped<T>(requestKey: string, work: () => Promise<T>): Promise<T> {
    const existing = inFlight.get(requestKey)
    if (existing) {
      return existing as Promise<T>
    }

    const promise = Promise.resolve()
      .then(work)
      .finally(() => {
        if (inFlight.get(requestKey) === promise) {
          inFlight.delete(requestKey)
        }
      })

    inFlight.set(requestKey, promise)
    return promise
  }

  return {
    cancelScope(scope) {
      nextSequence(scope)
    },

    runDeduped,

    async runLatest<T>(
      scope: string,
      requestKey: string,
      work: () => Promise<T>,
    ): Promise<T> {
      const sequence = nextSequence(scope)
      try {
        const response = await runDeduped<T>(requestKey, work)
        if (!isLatest(scope, sequence)) {
          throw new StaleBackendRequestError(scope)
        }
        return response
      } catch (error) {
        if (!isLatest(scope, sequence)) {
          throw new StaleBackendRequestError(scope, { cause: error })
        }
        throw error
      }
    },
  }
}

export function backendRequestKey(command: string, args?: Record<string, unknown>): string {
  const request = commandRequest(args)

  switch (command) {
    case 'get_repository_status':
      return repositoryStatusRequestKey(readString(request, 'projectId'))
    case 'get_repository_diff':
      return repositoryDiffRequestKey(readString(request, 'projectId'), readString(request, 'scope'))
    case 'list_project_files':
      return listProjectFilesRequestKey(readString(request, 'projectId'), readString(request, 'path', '/'))
    case 'read_project_file':
      return readProjectFileRequestKey(readString(request, 'projectId'), readString(request, 'path'))
    case 'search_project':
      return searchProjectRequestKey(request)
    case 'workspace_status':
      return workspaceStatusRequestKey(readString(request, 'projectId'))
    case 'workspace_query':
      return workspaceQueryRequestKey(request)
    case 'workspace_explain':
      return workspaceExplainRequestKey(request)
    case 'get_provider_model_catalog':
      return providerModelCatalogRequestKey(
        readString(request, 'profileId'),
        readBoolean(request, 'forceRefresh'),
      )
    case 'list_project_context_records':
      return listProjectContextRecordsRequestKey(readString(request, 'projectId'))
    default:
      throw new Error(`No explicit backend request key builder for ${command}.`)
  }
}

export function repositoryStatusRequestKey(projectId: string): string {
  return joinRequestKey('get_repository_status', projectId)
}

export function repositoryDiffRequestKey(projectId: string, scope: string): string {
  return joinRequestKey('get_repository_diff', projectId, scope)
}

export function listProjectFilesRequestKey(projectId: string, path = '/'): string {
  return joinRequestKey('list_project_files', projectId, path || '/')
}

export function readProjectFileRequestKey(projectId: string, path: string): string {
  return joinRequestKey('read_project_file', projectId, path)
}

export function searchProjectRequestKey(request: object): string {
  const record = request as Record<string, unknown>
  return joinRequestKey(
    'search_project',
    readString(record, 'projectId'),
    readString(record, 'query'),
    readString(record, 'cursor'),
    readBoolean(record, 'caseSensitive'),
    readBoolean(record, 'wholeWord'),
    readBoolean(record, 'regex'),
    readNumber(record, 'maxResults'),
    readNumber(record, 'maxFiles'),
    listRequestKey(readStringArray(record, 'includeGlobs')),
    listRequestKey(readStringArray(record, 'excludeGlobs')),
  )
}

export function workspaceStatusRequestKey(projectId: string): string {
  return joinRequestKey('workspace_status', projectId)
}

export function workspaceQueryRequestKey(request: object): string {
  const record = request as Record<string, unknown>
  return joinRequestKey(
    'workspace_query',
    readString(record, 'projectId'),
    readString(record, 'query'),
    readString(record, 'mode', 'auto'),
    readNumber(record, 'limit'),
    listRequestKey(readStringArray(record, 'paths')),
  )
}

export function workspaceExplainRequestKey(request: object): string {
  const record = request as Record<string, unknown>
  return joinRequestKey(
    'workspace_explain',
    readString(record, 'projectId'),
    readString(record, 'query'),
    readString(record, 'path'),
  )
}

export function providerModelCatalogRequestKey(profileId: string, forceRefresh = false): string {
  return joinRequestKey('get_provider_model_catalog', profileId, forceRefresh)
}

export function listProjectContextRecordsRequestKey(projectId: string): string {
  return joinRequestKey('list_project_context_records', projectId)
}

function commandRequest(args?: Record<string, unknown>): Record<string, unknown> {
  const request = args?.request
  return isRecord(request) ? request : {}
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function readString(record: Record<string, unknown>, key: string, fallback = ''): string {
  const value = record[key]
  return typeof value === 'string' ? value : fallback
}

function readBoolean(record: Record<string, unknown>, key: string, fallback = false): boolean {
  const value = record[key]
  return typeof value === 'boolean' ? value : fallback
}

function readNumber(record: Record<string, unknown>, key: string): number | null {
  const value = record[key]
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function readStringArray(record: Record<string, unknown>, key: string): string[] {
  const value = record[key]
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : []
}

function joinRequestKey(...parts: Array<boolean | number | string | null | undefined>): string {
  return parts.map(encodeRequestKeyPart).join('')
}

function listRequestKey(values: string[]): string {
  return joinRequestKey(values.length, ...values)
}

function encodeRequestKeyPart(value: boolean | number | string | null | undefined): string {
  const text = value === null || value === undefined ? '' : String(value)
  return `${text.length}:${text};`
}
