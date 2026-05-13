'use client'

import { useEffect, useState } from 'react'

interface EditorLiveRegionProps {
  /** Announcement for non-urgent status changes (saves, diagnostics counts, format results). */
  status?: string | null
  /** Announcement for urgent prompts (save conflicts, save failures). */
  alert?: string | null
}

// Some screen readers de-duplicate identical successive announcement text.
// Re-announce the same message by appending an invisible suffix when the
// upstream signal has changed.
function useReannounceableMessage(message: string | null | undefined): string {
  const [emitted, setEmitted] = useState('')

  useEffect(() => {
    if (!message) {
      setEmitted('')
      return
    }
    setEmitted((current) =>
      current === message ? `${message} ​` : message,
    )
  }, [message])

  return emitted
}

export function EditorLiveRegion({ status, alert }: EditorLiveRegionProps) {
  const statusMessage = useReannounceableMessage(status)
  const alertMessage = useReannounceableMessage(alert)

  return (
    <>
      <div
        aria-live="polite"
        aria-atomic="true"
        className="sr-only"
        data-testid="editor-live-region-status"
      >
        {statusMessage}
      </div>
      <div
        aria-live="assertive"
        aria-atomic="true"
        className="sr-only"
        data-testid="editor-live-region-alert"
      >
        {alertMessage}
      </div>
    </>
  )
}
