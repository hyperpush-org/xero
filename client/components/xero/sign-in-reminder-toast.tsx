import { ToastAction } from '@xero/ui/components/ui/toast'
import { toast } from '@xero/ui/components/ui/use-toast'
import { useEffect, useRef } from 'react'

import { useGitHubAuth } from '@/src/lib/github-auth'

/**
 * One-time, per-launch nudge: when the desktop app opens with no GitHub
 * account linked, remind the user that signing in lets them drive these
 * same sessions from the cloud app on any device.
 *
 * The bridge identity starts out `idle` before the first status refresh
 * resolves, so we only treat `idle` as "signed out" once we've actually
 * observed a load — otherwise the nudge would flash on every launch before
 * the real session is known.
 */
export function SignInReminderToast() {
  const { status, session, login } = useGitHubAuth()
  const observedLoadRef = useRef(false)
  const shownRef = useRef(false)
  const toastRef = useRef<ReturnType<typeof toast> | null>(null)

  useEffect(() => {
    if (status === 'loading' || status === 'authenticating') {
      observedLoadRef.current = true
      return
    }
    if (status === 'ready' || session) {
      // Signed in — retract the nudge if it's still on screen.
      toastRef.current?.dismiss()
      toastRef.current = null
      return
    }
    // `error` means we couldn't determine the session; don't guess.
    if (status === 'error') return
    // status === 'idle' (definitively signed out)
    if (!observedLoadRef.current) return
    if (shownRef.current) return
    shownRef.current = true

    toastRef.current = toast({
      title: 'Sign in to continue from anywhere',
      description:
        'Sign in with GitHub to drive your sessions from the cloud app on any device.',
      // Hold until the user acts on or dismisses the nudge.
      duration: Number.POSITIVE_INFINITY,
      // Stack the action under the text (full-width description) instead of
      // reserving a wide right column, then tuck the button up into the
      // trailing whitespace of the last line.
      className: 'flex-col items-stretch gap-0',
      action: (
        <ToastAction
          altText="Sign in with GitHub"
          onClick={() => void login()}
          className="-mt-1 -mr-6 self-end"
        >
          Sign in
        </ToastAction>
      ),
    })
  }, [status, session, login])

  return null
}
