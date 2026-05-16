export interface ByteBudgetCacheConfig {
  maxBytes: number
  maxEntries: number
}

export interface ByteBudgetCacheStats {
  byteSize: number
  entries: number
  evictions: number
  hits: number
  misses: number
  skippedByBudget: number
}

interface ByteBudgetCacheEntry<T> {
  byteSize: number
  value: T
}

export interface ByteBudgetCache<K, V> {
  clear: () => void
  delete: (key: K) => boolean
  get: (key: K) => V | null
  getStats: () => ByteBudgetCacheStats
  has: (key: K) => boolean
  set: (key: K, value: V, byteSize: number) => boolean
}

export function estimateUtf16Bytes(value: string): number {
  return value.length * 2
}

export function estimateJsonBytes(value: unknown): number {
  try {
    return estimateUtf16Bytes(JSON.stringify(value))
  } catch {
    return 0
  }
}

export function createByteBudgetCache<K, V>({
  maxBytes,
  maxEntries,
}: ByteBudgetCacheConfig): ByteBudgetCache<K, V> {
  const entries = new Map<K, ByteBudgetCacheEntry<V>>()
  const stats = {
    evictions: 0,
    hits: 0,
    misses: 0,
    skippedByBudget: 0,
  }
  let byteSize = 0

  const deleteEntry = (key: K): boolean => {
    const entry = entries.get(key)
    if (!entry) return false
    byteSize = Math.max(0, byteSize - entry.byteSize)
    return entries.delete(key)
  }

  const trim = () => {
    while (entries.size > maxEntries || byteSize > maxBytes) {
      const oldestKey = entries.keys().next().value
      if (oldestKey === undefined) return
      if (deleteEntry(oldestKey)) {
        stats.evictions += 1
      }
    }
  }

  return {
    clear() {
      entries.clear()
      byteSize = 0
      stats.evictions = 0
      stats.hits = 0
      stats.misses = 0
      stats.skippedByBudget = 0
    },
    delete: deleteEntry,
    get(key) {
      const entry = entries.get(key)
      if (!entry) {
        stats.misses += 1
        return null
      }

      entries.delete(key)
      entries.set(key, entry)
      stats.hits += 1
      return entry.value
    },
    getStats() {
      return {
        byteSize,
        entries: entries.size,
        evictions: stats.evictions,
        hits: stats.hits,
        misses: stats.misses,
        skippedByBudget: stats.skippedByBudget,
      }
    },
    has(key) {
      return entries.has(key)
    },
    set(key, value, entryBytes) {
      if (entries.has(key)) {
        deleteEntry(key)
      }

      const normalizedBytes = Math.max(0, Math.ceil(entryBytes))
      if (normalizedBytes > maxBytes) {
        stats.skippedByBudget += 1
        return false
      }

      entries.set(key, {
        byteSize: normalizedBytes,
        value,
      })
      byteSize += normalizedBytes
      trim()
      return true
    },
  }
}
