import { runWithDurableIdempotencyLock } from './durable-idempotency-lock'

export interface ContinuationIdentity {
  channel: 'send' | 'resume' | 'runtime-control' | 'agent-start'
  targetId: string
  payload: unknown
}

export interface IdempotencyStorage {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
  removeItem(key: string): void
}

export interface ContinuationIdempotencyOptions {
  storage: IdempotencyStorage
  storageKey?: string
}

const MAX_PERSISTED_PENDING_IDENTITIES = 256

export function browserIdempotencyStorage(): IdempotencyStorage {
  try {
    const storage = globalThis.localStorage
    if (!storage) throw new Error('localStorage is unavailable')
    return storage
  } catch (error) {
    const unavailable = new Error(
      `Xero cannot safely dispatch restart-sensitive operations because durable request storage is unavailable${
        error instanceof Error && error.message ? `: ${error.message}` : '.'
      }`,
    )
    const fail = (): never => {
      throw unavailable
    }
    return {
      getItem: fail,
      setItem: fail,
      removeItem: fail,
    }
  }
}

function canonicalize(value: unknown, ancestors: Set<object>): string {
  if (value === null) return 'null'
  if (typeof value === 'string' || typeof value === 'boolean' || typeof value === 'number') {
    return JSON.stringify(value) ?? 'null'
  }
  if (value === undefined) return 'undefined'
  if (typeof value === 'bigint') throw new TypeError('Continuation payload must be JSON-serializable.')
  if (typeof value === 'function' || typeof value === 'symbol') return 'undefined'
  if (ancestors.has(value)) throw new TypeError('Continuation payload must not be circular.')
  ancestors.add(value)
  try {
    if (Array.isArray(value)) {
      return `[${value
        .map((entry) =>
          entry === undefined || typeof entry === 'function' || typeof entry === 'symbol'
            ? 'null'
            : canonicalize(entry, ancestors),
        )
        .join(',')}]`
    }
    const object = value as Record<string, unknown>
    return `{${Object.keys(object)
      .filter((key) => object[key] !== undefined)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalize(object[key], ancestors)}`)
      .join(',')}}`
  } finally {
    ancestors.delete(value)
  }
}

function fingerprint(identity: ContinuationIdentity): string {
  return JSON.stringify([
    identity.channel,
    identity.targetId,
    canonicalize(identity.payload, new Set()),
  ])
}

function isDefinitiveRejection(error: unknown): boolean {
  if (!error || typeof error !== 'object') return false
  const candidate = error as { errorClass?: unknown; retryable?: unknown }
  return (
    candidate.retryable === false &&
    (candidate.errorClass === 'user_fixable' || candidate.errorClass === 'policy_denied')
  )
}

/** Keeps an ambiguous continuation identity until success or a definitive rejection. */
export class ContinuationIdempotencyCoordinator {
  private readonly pendingByFingerprint = new Map<string, string>()
  private readonly storage: IdempotencyStorage
  private readonly storageKey: string

  constructor(
    private readonly createRequestId: () => string,
    options: ContinuationIdempotencyOptions,
  ) {
    this.storage = options.storage
    this.storageKey = options.storageKey ?? 'xero.pending-continuation-identities.v1'
  }

  async run<T>(
    identity: ContinuationIdentity,
    operation: (requestId: string) => Promise<T>,
  ): Promise<T> {
    const nextFingerprint = fingerprint(identity)
    const requestId = await runWithDurableIdempotencyLock(this.storageKey, () => {
      this.reloadDurablePending()
      return this.pendingByFingerprint.get(nextFingerprint) ?? this.createPending(nextFingerprint)
    })
    try {
      const result = await operation(requestId)
      await runWithDurableIdempotencyLock(this.storageKey, () =>
        this.clear(nextFingerprint, requestId),
      )
      return result
    } catch (error) {
      if (isDefinitiveRejection(error)) {
        await runWithDurableIdempotencyLock(this.storageKey, () =>
          this.clear(nextFingerprint, requestId),
        )
      }
      throw error
    }
  }

  private createPending(nextFingerprint: string): string {
    if (this.pendingByFingerprint.size >= MAX_PERSISTED_PENDING_IDENTITIES) {
      throw new Error(
        'Xero has too many unresolved operations to safely dispatch another request. Retry an earlier operation before starting another one.',
      )
    }
    const requestId = this.createRequestId()
    this.pendingByFingerprint.set(nextFingerprint, requestId)
    try {
      this.persistPending()
    } catch (error) {
      this.pendingByFingerprint.delete(nextFingerprint)
      throw error
    }
    return requestId
  }

  private clear(nextFingerprint: string, requestId: string) {
    this.reloadDurablePending()
    if (this.pendingByFingerprint.get(nextFingerprint) === requestId) {
      this.pendingByFingerprint.delete(nextFingerprint)
      try {
        this.persistPending()
      } catch (error) {
        // The response is not safely acknowledged until its durable identity is
        // cleared. Restore the in-memory copy so a retry remains idempotent.
        this.pendingByFingerprint.set(nextFingerprint, requestId)
        throw error
      }
    }
  }

  private reloadDurablePending(): void {
    const raw = this.storage.getItem(this.storageKey)
    this.pendingByFingerprint.clear()
    if (!raw) return
    let parsed: unknown
    try {
      parsed = JSON.parse(raw)
    } catch {
      throw new Error('Xero could not decode durable pending operation identities.')
    }
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      throw new Error('Xero found invalid durable pending operation identities.')
    }
    for (const [key, value] of Object.entries(parsed)) {
      if (
        key.length === 0 ||
        typeof value !== 'string' ||
        value.trim() !== value ||
        value.length === 0 ||
        value.length > 200
      ) {
        throw new Error('Xero found an invalid durable pending operation identity.')
      }
      this.pendingByFingerprint.set(key, value)
      if (this.pendingByFingerprint.size > MAX_PERSISTED_PENDING_IDENTITIES) {
        throw new Error('Xero found too many unresolved durable operation identities.')
      }
    }
  }

  private persistPending(): void {
    if (this.pendingByFingerprint.size === 0) {
      this.storage.removeItem(this.storageKey)
      return
    }
    this.storage.setItem(
      this.storageKey,
      JSON.stringify(Object.fromEntries(this.pendingByFingerprint.entries())),
    )
  }
}
