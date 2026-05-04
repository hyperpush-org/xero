import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from 'react'

import type {
  XeroDesktopAdapter,
  XeroDictationSession,
} from '@/src/lib/xero-desktop'
import type {
  DictationEventDto,
  DictationStatusDto,
} from '@/src/lib/xero-model/dictation'
import type { OperatorActionErrorView } from '@/src/features/xero/use-xero-desktop-state/types'

export type SpeechDictationPhase = 'idle' | 'requesting' | 'listening' | 'stopping'

export type SpeechDictationAdapter = Pick<
  XeroDesktopAdapter,
  | 'isDesktopRuntime'
  | 'speechDictationStatus'
  | 'speechDictationSettings'
  | 'speechDictationStart'
  | 'speechDictationStop'
  | 'speechDictationCancel'
>

interface UseSpeechDictationOptions {
  adapter?: SpeechDictationAdapter
  enabled?: boolean
  scopeKey: string
  draftPrompt: string
  setDraftPrompt: Dispatch<SetStateAction<string>>
  promptInputDisabled: boolean
  promptInputRef: RefObject<HTMLTextAreaElement | null>
}

interface SpeechDictationController {
  isVisible: boolean
  phase: SpeechDictationPhase
  isListening: boolean
  isToggleDisabled: boolean
  ariaLabel: string
  tooltip: string
  error: OperatorActionErrorView | null
  toggle: () => Promise<void>
  stopBeforeSubmit: () => Promise<boolean>
}

const DEFAULT_CONTEXTUAL_PHRASES = [
  'Xero',
  'Tauri',
  'ShadCN',
  'OpenAI',
  'OpenRouter',
  'Claude',
  'Gemini',
]

function appendDictationSegment(baseDraft: string, dictatedText: string): string {
  const segment = dictatedText.trimStart()
  if (segment.length === 0) {
    return baseDraft
  }

  if (baseDraft.length === 0) {
    return segment
  }

  const startsWithPunctuation = /^[,.;:!?)]/.test(segment)
  const needsSpace = !/\s$/.test(baseDraft) && !startsWithPunctuation
  return `${baseDraft}${needsSpace ? ' ' : ''}${segment}`
}

function getUnknownErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message
  }

  if (typeof error === 'string' && error.trim().length > 0) {
    return error
  }

  return fallback
}

function toDictationError(error: unknown, fallback: string): OperatorActionErrorView {
  const maybeDesktopError = error as {
    code?: unknown
    retryable?: unknown
  } | null
  const code =
    typeof maybeDesktopError?.code === 'string' && maybeDesktopError.code.trim().length > 0
      ? maybeDesktopError.code
      : 'dictation_failed'

  return {
    code,
    message: getRecoveryMessage(code, getUnknownErrorMessage(error, fallback)),
    retryable: typeof maybeDesktopError?.retryable === 'boolean' ? maybeDesktopError.retryable : false,
  }
}

function getRecoveryMessage(code: string, message: string): string {
  const recovery = (() => {
    switch (code) {
      case 'dictation_microphone_permission_denied':
        return 'Open System Settings > Privacy & Security > Microphone and allow Xero.'
      case 'dictation_speech_permission_denied':
        return 'Open System Settings > Privacy & Security > Speech Recognition and allow Xero.'
      case 'dictation_modern_locale_unsupported':
      case 'dictation_legacy_locale_unsupported':
        return 'Choose a supported locale in Dictation settings.'
      case 'dictation_legacy_network_recognition_required':
      case 'dictation_legacy_on_device_unavailable':
        return 'Allow Apple server recognition in Dictation settings or choose a locale with on-device recognition.'
      default:
        return null
    }
  })()

  if (!recovery || message.includes(recovery)) {
    return message
  }

  return `${message} ${recovery}`
}

function isDictationAvailable(status: DictationStatusDto | null): boolean {
  return Boolean(
    status?.platform === 'macos' &&
      (status.modern.available || status.legacy.available),
  )
}

export function useSpeechDictation({
  adapter,
  enabled = true,
  scopeKey,
  draftPrompt,
  setDraftPrompt,
  promptInputDisabled,
  promptInputRef,
}: UseSpeechDictationOptions): SpeechDictationController {
  const [status, setStatus] = useState<DictationStatusDto | null>(null)
  const [phase, setPhase] = useState<SpeechDictationPhase>('idle')
  const [error, setError] = useState<OperatorActionErrorView | null>(null)

  const adapterRef = useRef(adapter)
  const draftPromptRef = useRef(draftPrompt)
  const phaseRef = useRef<SpeechDictationPhase>('idle')
  const sessionRef = useRef<XeroDictationSession | null>(null)
  const sessionIdRef = useRef<string | null>(null)
  const dictationBaseRef = useRef('')
  const renderedDraftRef = useRef('')
  const statusRequestRef = useRef(0)

  useEffect(() => {
    adapterRef.current = adapter
  }, [adapter])

  useEffect(() => {
    draftPromptRef.current = draftPrompt
  }, [draftPrompt])

  const updatePhase = useCallback((nextPhase: SpeechDictationPhase) => {
    phaseRef.current = nextPhase
    setPhase(nextPhase)
  }, [])

  const focusPromptInput = useCallback(() => {
    window.requestAnimationFrame(() => {
      promptInputRef.current?.focus()
    })
  }, [promptInputRef])

  const releaseSession = useCallback(() => {
    sessionRef.current?.unsubscribe()
    sessionRef.current = null
    sessionIdRef.current = null
    dictationBaseRef.current = draftPromptRef.current
    renderedDraftRef.current = draftPromptRef.current
  }, [])

  const applyDictatedSegment = useCallback(
    (text: string, commit: boolean) => {
      setDraftPrompt((currentDraft) => {
        const expectedDraft = renderedDraftRef.current
        const nextBase = currentDraft === expectedDraft ? dictationBaseRef.current : currentDraft
        const nextDraft = appendDictationSegment(nextBase, text)

        draftPromptRef.current = nextDraft
        renderedDraftRef.current = nextDraft
        dictationBaseRef.current = commit ? nextDraft : nextBase

        return nextDraft
      })
    },
    [setDraftPrompt],
  )

  const handleDictationEvent = useCallback(
    (event: DictationEventDto) => {
      if (event.kind === 'permission') {
        const deniedMicrophone = event.microphone === 'denied' || event.microphone === 'restricted'
        const deniedSpeech = event.speech === 'denied' || event.speech === 'restricted'
        if (deniedMicrophone || deniedSpeech) {
          setError({
            code: 'dictation_permission_denied',
            message: deniedMicrophone
              ? 'Allow microphone access in System Settings to use dictation.'
              : 'Allow speech recognition in System Settings to use dictation.',
            retryable: true,
          })
        }
        return
      }

      if (event.kind === 'asset_installing') {
        updatePhase('requesting')
        return
      }

      if (sessionIdRef.current && event.sessionId !== sessionIdRef.current) {
        return
      }

      if (event.kind === 'started') {
        sessionIdRef.current = event.sessionId
        setError(null)
        updatePhase('listening')
        focusPromptInput()
        return
      }

      if (event.kind === 'partial') {
        updatePhase('listening')
        applyDictatedSegment(event.text, false)
        return
      }

      if (event.kind === 'final') {
        updatePhase('listening')
        applyDictatedSegment(event.text, true)
        return
      }

      if (event.kind === 'stopped') {
        releaseSession()
        updatePhase('idle')
        return
      }

      setError({
        code: event.code,
        message: event.message,
        retryable: event.retryable,
      })
      releaseSession()
      updatePhase('idle')
    },
    [applyDictatedSegment, focusPromptInput, releaseSession, updatePhase],
  )

  const handleChannelError = useCallback(
    (channelError: unknown) => {
      setError(toDictationError(channelError, 'Xero could not read the native dictation stream.'))
      releaseSession()
      updatePhase('idle')
    },
    [releaseSession, updatePhase],
  )

  useEffect(() => {
    const requestId = statusRequestRef.current + 1
    statusRequestRef.current = requestId
    setStatus(null)

    if (
      !enabled ||
      !adapter ||
      !adapter.isDesktopRuntime() ||
      !adapter.speechDictationStatus ||
      !adapter.speechDictationStart
    ) {
      return
    }

    let cancelled = false

    adapter
      .speechDictationStatus()
      .then((nextStatus) => {
        if (!cancelled && statusRequestRef.current === requestId) {
          setStatus(nextStatus)
        }
      })
      .catch(() => {
        if (!cancelled && statusRequestRef.current === requestId) {
          setStatus(null)
        }
      })

    return () => {
      cancelled = true
    }
  }, [adapter, enabled])

  const start = useCallback(async () => {
    const currentAdapter = adapterRef.current
    if (
      !enabled ||
      promptInputDisabled ||
      phaseRef.current !== 'idle' ||
      !currentAdapter?.speechDictationStart ||
      !isDictationAvailable(status)
    ) {
      return
    }

    const baseDraft = draftPromptRef.current
    dictationBaseRef.current = baseDraft
    renderedDraftRef.current = baseDraft
    setError(null)
    updatePhase('requesting')

    try {
      const settings = await currentAdapter.speechDictationSettings?.().catch(() => null)
      const session = await currentAdapter.speechDictationStart(
        {
          enginePreference: settings?.enginePreference,
          privacyMode: settings?.privacyMode,
          locale: settings?.locale,
          contextualPhrases: DEFAULT_CONTEXTUAL_PHRASES,
        },
        handleDictationEvent,
        handleChannelError,
      )

      if (phaseRef.current === 'idle') {
        try {
          await session.cancel()
        } catch {
          // The UI has already moved on; this best-effort cleanup avoids leaving native capture open.
        }
        session.unsubscribe()
        return
      }

      sessionRef.current = session
      sessionIdRef.current = session.response.sessionId
      updatePhase('listening')
      focusPromptInput()
    } catch (startError) {
      releaseSession()
      setError(toDictationError(startError, 'Xero could not start native dictation.'))
      updatePhase('idle')
    }
  }, [
    focusPromptInput,
    handleChannelError,
    handleDictationEvent,
    enabled,
    promptInputDisabled,
    releaseSession,
    status,
    updatePhase,
  ])

  const stop = useCallback(async (): Promise<boolean> => {
    const session = sessionRef.current
    if (!session) {
      releaseSession()
      updatePhase('idle')
      return true
    }

    updatePhase('stopping')
    try {
      await session.stop()
      releaseSession()
      updatePhase('idle')
      return true
    } catch (stopError) {
      setError(toDictationError(stopError, 'Xero could not stop native dictation.'))
      releaseSession()
      updatePhase('idle')
      return false
    }
  }, [releaseSession, updatePhase])

  const cancel = useCallback(async () => {
    const session = sessionRef.current
    if (!session) {
      releaseSession()
      updatePhase('idle')
      return
    }

    updatePhase('stopping')
    try {
      await session.cancel()
    } catch (cancelError) {
      setError(toDictationError(cancelError, 'Xero could not cancel native dictation.'))
    } finally {
      releaseSession()
      updatePhase('idle')
    }
  }, [releaseSession, updatePhase])

  useEffect(() => {
    return () => {
      void cancel()
    }
  }, [cancel, scopeKey])

  const toggle = useCallback(async () => {
    if (phaseRef.current === 'listening') {
      await stop()
      return
    }

    await start()
  }, [start, stop])

  const isVisible = Boolean(enabled && adapter?.speechDictationStart && isDictationAvailable(status))
  const isListening = phase === 'listening'
  const isBusy = phase === 'requesting' || phase === 'stopping'
  const isToggleDisabled = promptInputDisabled || isBusy
  const ariaLabel = isListening ? 'Stop dictation' : 'Start dictation'
  const tooltip =
    phase === 'requesting'
      ? 'Requesting dictation permission'
      : phase === 'stopping'
        ? 'Stopping dictation'
        : ariaLabel

  return {
    isVisible,
    phase,
    isListening,
    isToggleDisabled,
    ariaLabel,
    tooltip,
    error,
    toggle,
    stopBeforeSubmit: stop,
  }
}
