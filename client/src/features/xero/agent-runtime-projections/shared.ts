export function getTimestampMs(value: string | null | undefined): number {
  if (typeof value !== 'string' || value.trim().length === 0) {
    return 0
  }

  const parsed = Date.parse(value)
  return Number.isFinite(parsed) ? parsed : 0
}

export function normalizeText(value: string | null | undefined): string | null {
  if (typeof value !== 'string') {
    return null
  }

  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

export function sortByNewest<T>(
  items: readonly T[],
  getTimestamp: (item: T) => string | null | undefined,
): T[] {
  return [...items]
    .map((item, index) => ({ item, index }))
    .sort((left, right) => {
      const leftTime = getTimestampMs(getTimestamp(left.item))
      const rightTime = getTimestampMs(getTimestamp(right.item))
      if (leftTime === rightTime) {
        return left.index - right.index
      }

      return rightTime - leftTime
    })
    .map(({ item }) => item)
}
