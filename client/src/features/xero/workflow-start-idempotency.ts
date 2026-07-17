import { runWithDurableIdempotencyLock } from './durable-idempotency-lock'

export interface WorkflowStartIdentity {
  projectId: string
  workflowId: string
  initialInput: unknown
}

type WorkflowStartOperation<T> = (idempotencyKey: string) => Promise<T>

export interface WorkflowStartIdempotencyStorage {
  readonly length?: number
  key?(index: number): string | null
  getItem(key: string): string | null
  setItem(key: string, value: string): void
  removeItem(key: string): void
}

const WORKFLOW_START_PENDING_STORAGE_KEY_PREFIX = 'xero.workflow-start.pending.v2.'
const WORKFLOW_START_PENDING_LOCK = 'xero.workflow-start.pending.v2.lock'
const MAX_PERSISTED_PENDING_STARTS = 256

interface PersistedWorkflowStart {
  version: 2
  fingerprint: string
  idempotencyKey: string
}

function defaultWorkflowStartStorage(): WorkflowStartIdempotencyStorage | null {
  try {
    return globalThis.localStorage ?? null
  } catch {
    return null
  }
}

function canonicalizeJsonValue(value: unknown, ancestors: Set<object>): string {
  if (value === null) return 'null'

  switch (typeof value) {
    case 'string':
    case 'boolean':
      return JSON.stringify(value)
    case 'number':
      return JSON.stringify(value) ?? 'null'
    case 'undefined':
      return 'undefined'
    case 'object': {
      if (ancestors.has(value)) {
        throw new TypeError('Workflow initial input must not contain circular references.')
      }

      ancestors.add(value)
      try {
        if (Array.isArray(value)) {
          return `[${value
            .map((entry) =>
              entry === undefined || typeof entry === 'function' || typeof entry === 'symbol'
                ? 'null'
                : canonicalizeJsonValue(entry, ancestors),
            )
            .join(',')}]`
        }

        const valueWithOptionalJson = value as Record<string, unknown> & {
          toJSON?: () => unknown
        }
        if (typeof valueWithOptionalJson.toJSON === 'function') {
          return canonicalizeJsonValue(valueWithOptionalJson.toJSON(), ancestors)
        }

        const entries = Object.keys(valueWithOptionalJson)
          .filter((key) => {
            const entry = valueWithOptionalJson[key]
            return entry !== undefined && typeof entry !== 'function' && typeof entry !== 'symbol'
          })
          .sort()
          .map(
            (key) =>
              `${JSON.stringify(key)}:${canonicalizeJsonValue(valueWithOptionalJson[key], ancestors)}`,
          )
        return `{${entries.join(',')}}`
      } finally {
        ancestors.delete(value)
      }
    }
    case 'bigint':
      throw new TypeError('Workflow initial input must be JSON-serializable.')
    case 'function':
    case 'symbol':
      return 'undefined'
  }

  throw new TypeError('Workflow initial input must be JSON-serializable.')
}

export function workflowStartFingerprint(identity: WorkflowStartIdentity): string {
  return JSON.stringify([
    identity.projectId,
    identity.workflowId,
    canonicalizeJsonValue(identity.initialInput, new Set()),
  ])
}

function isDefinitiveWorkflowStartRejection(error: unknown): boolean {
  if (!error || typeof error !== 'object') return false

  const candidate = error as { errorClass?: unknown; retryable?: unknown }
  return (
    candidate.retryable === false &&
    (candidate.errorClass === 'user_fixable' || candidate.errorClass === 'policy_denied')
  )
}

/**
 * Holds each Workflow start key whose outcome is not yet known. Identical
 * retries reuse their key even when ambiguous starts for other Workflows are
 * interleaved, so a lost IPC response cannot create a duplicate run.
 */
export class WorkflowStartIdempotencyCoordinator {
  constructor(
    private readonly createIdempotencyKey: () => string,
    private readonly storage: WorkflowStartIdempotencyStorage | null = defaultWorkflowStartStorage(),
  ) {}

  async run<T>(identity: WorkflowStartIdentity, operation: WorkflowStartOperation<T>): Promise<T> {
    const fingerprint = workflowStartFingerprint(identity)
    const idempotencyKey = await runWithDurableIdempotencyLock(
      WORKFLOW_START_PENDING_LOCK,
      () => {
        try {
          this.validatePersistedStore()
          return this.readPending(fingerprint) ?? this.createPending(fingerprint)
        } catch (error) {
          throw asWorkflowDurabilityError(error)
        }
      },
    )

    try {
      const result = await operation(idempotencyKey)
      await this.clearIfCurrent(fingerprint, idempotencyKey)
      return result
    } catch (error) {
      if (isDefinitiveWorkflowStartRejection(error)) {
        await this.clearIfCurrent(fingerprint, idempotencyKey)
      }
      throw error
    }
  }

  private createPending(fingerprint: string): string {
    if (!this.storage) {
      throw new Error(
        'Xero cannot safely start a Workflow because durable request storage is unavailable.',
      )
    }
    if (this.persistedPendingCount() >= MAX_PERSISTED_PENDING_STARTS) {
      throw new Error(
        'Xero has too many unresolved Workflow start requests. Retry an earlier request before starting another Workflow.',
      )
    }
    const idempotencyKey = this.createIdempotencyKey()
    this.storage.setItem(
      this.pendingStorageKey(fingerprint),
      JSON.stringify({
        version: 2,
        fingerprint,
        idempotencyKey,
      } satisfies PersistedWorkflowStart),
    )
    const persisted = this.readPending(fingerprint)
    if (persisted === null) {
      throw new Error('Xero could not verify the persisted Workflow request identity.')
    }
    return persisted
  }

  private clearIfCurrent(fingerprint: string, idempotencyKey: string): Promise<void> {
    return runWithDurableIdempotencyLock(WORKFLOW_START_PENDING_LOCK, () => {
      if (!this.storage) {
        throw new Error('Xero cannot clear the pending Workflow request identity.')
      }
      if (this.readPending(fingerprint) === idempotencyKey) {
        const storageKey = this.pendingStorageKey(fingerprint)
        this.storage.removeItem(storageKey)
        if (this.storage.getItem(storageKey) !== null) {
          throw new Error('Xero could not verify that the Workflow request identity was cleared.')
        }
      }
    })
  }

  private readPending(fingerprint: string): string | null {
    if (!this.storage) return null
    const raw = this.storage.getItem(this.pendingStorageKey(fingerprint))
    if (raw === null) return null
    return this.decodePending(raw, fingerprint).idempotencyKey
  }

  private validatePersistedStore(): void {
    if (!this.storage || typeof this.storage.length !== 'number' || !this.storage.key) return
    const pendingKeys: string[] = []
    for (let index = 0; index < this.storage.length; index += 1) {
      const key = this.storage.key(index)
      if (key?.startsWith(WORKFLOW_START_PENDING_STORAGE_KEY_PREFIX)) pendingKeys.push(key)
    }
    if (pendingKeys.length > MAX_PERSISTED_PENDING_STARTS) {
      throw new Error('The pending Workflow request store exceeds the supported size limit.')
    }
    for (const key of pendingKeys) {
      const raw = this.storage.getItem(key)
      if (raw === null) continue
      const fingerprint = key.slice(WORKFLOW_START_PENDING_STORAGE_KEY_PREFIX.length)
      this.decodePending(raw, fingerprint)
    }
  }

  private persistedPendingCount(): number {
    if (!this.storage || typeof this.storage.length !== 'number' || !this.storage.key) return 0
    let count = 0
    for (let index = 0; index < this.storage.length; index += 1) {
      if (this.storage.key(index)?.startsWith(WORKFLOW_START_PENDING_STORAGE_KEY_PREFIX)) count += 1
    }
    return count
  }

  private decodePending(raw: string, fingerprint: string): PersistedWorkflowStart {
    const decoded = JSON.parse(raw) as Partial<PersistedWorkflowStart>
    if (
      decoded.version !== 2 ||
      decoded.fingerprint !== fingerprint ||
      typeof decoded.idempotencyKey !== 'string' ||
      decoded.idempotencyKey.trim() !== decoded.idempotencyKey ||
      decoded.idempotencyKey.length === 0 ||
      decoded.idempotencyKey.length > 200
    ) {
      throw new Error('The pending Workflow request store contains an invalid entry.')
    }
    return decoded as PersistedWorkflowStart
  }

  private pendingStorageKey(fingerprint: string): string {
    return `${WORKFLOW_START_PENDING_STORAGE_KEY_PREFIX}${fingerprint}`
  }
}

function asWorkflowDurabilityError(error: unknown): Error {
  const detail = error instanceof Error ? error.message : String(error)
  return new Error(
    `Xero cannot safely start a Workflow because its durable request store is unreadable: ${detail}`,
  )
}
