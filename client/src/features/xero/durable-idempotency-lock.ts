type ExclusiveOperation<T> = () => T | Promise<T>

interface BrowserLockManager {
  request<T>(name: string, callback: () => T | Promise<T>): Promise<T>
}

const inProcessLockTails = new Map<string, Promise<void>>()

async function runWithInProcessLock<T>(
  name: string,
  operation: ExclusiveOperation<T>,
): Promise<T> {
  const previous = inProcessLockTails.get(name) ?? Promise.resolve()
  let release: () => void = () => {}
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  const tail = previous.catch(() => undefined).then(() => gate)
  inProcessLockTails.set(name, tail)

  await previous.catch(() => undefined)
  try {
    return await operation()
  } finally {
    release()
    if (inProcessLockTails.get(name) === tail) {
      await tail
      if (inProcessLockTails.get(name) === tail) inProcessLockTails.delete(name)
    }
  }
}

/**
 * Serializes durable idempotency-map mutations across WebViews. The Web Locks
 * API supplies the cross-context boundary in the desktop renderer; the module
 * queue preserves the same semantics in tests and single-context runtimes.
 */
export function runWithDurableIdempotencyLock<T>(
  name: string,
  operation: ExclusiveOperation<T>,
): Promise<T> {
  const lockManager =
    typeof navigator === 'undefined'
      ? undefined
      : (navigator as Navigator & { locks?: BrowserLockManager }).locks
  if (lockManager) return lockManager.request(name, operation)
  return runWithInProcessLock(name, operation)
}
