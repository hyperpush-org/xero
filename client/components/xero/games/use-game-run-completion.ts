import { useEffect, useRef } from "react"

export interface GameRunCompletion {
  score: number
  timePlayedMs: number
}

export function useGameRunCompletion({
  status,
  score,
  onRunComplete,
}: {
  status: string
  score: number
  onRunComplete?: (run: GameRunCompletion) => void
}) {
  const previousStatusRef = useRef(status)
  const playingSinceRef = useRef<number | null>(null)
  const playedMsRef = useRef(0)
  const reportedRef = useRef(false)

  useEffect(() => {
    const previousStatus = previousStatusRef.current
    const now = performance.now()

    if (previousStatus === "playing" && status !== "playing" && playingSinceRef.current !== null) {
      playedMsRef.current += now - playingSinceRef.current
      playingSinceRef.current = null
    }

    if (status === "idle") {
      playedMsRef.current = 0
      playingSinceRef.current = null
      reportedRef.current = false
    }

    if (status === "playing" && playingSinceRef.current === null) {
      playingSinceRef.current = now
    }

    if (status === "over" && !reportedRef.current) {
      if (playingSinceRef.current !== null) {
        playedMsRef.current += now - playingSinceRef.current
        playingSinceRef.current = null
      }
      reportedRef.current = true
      onRunComplete?.({
        score,
        timePlayedMs: Math.max(0, Math.round(playedMsRef.current)),
      })
    }

    previousStatusRef.current = status
  }, [onRunComplete, score, status])
}
