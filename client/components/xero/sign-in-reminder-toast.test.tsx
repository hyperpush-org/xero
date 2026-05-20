import { act, cleanup, render } from '@testing-library/react'
import type { ReactElement } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { GitHubAuthStatus } from '@/src/lib/github-auth'

import { SignInReminderToast } from './sign-in-reminder-toast'

interface ToastArgs {
  title?: string
  description?: string
  action?: ReactElement<{ onClick: () => void }>
}

const { authState, loginMock, dismissMock, toastMock } = vi.hoisted(() => ({
  authState: { status: 'loading' as GitHubAuthStatus, session: null as unknown },
  loginMock: vi.fn(),
  dismissMock: vi.fn(),
  toastMock: vi.fn((_args: ToastArgs) => ({
    id: '1',
    dismiss: dismissMock,
    update: vi.fn(),
  })),
}))

vi.mock('@/src/lib/github-auth', () => ({
  useGitHubAuth: () => ({
    status: authState.status,
    session: authState.session,
    error: null,
    login: loginMock,
    logout: vi.fn(),
    refresh: vi.fn(),
  }),
}))

vi.mock('@xero/ui/components/ui/use-toast', () => ({ toast: toastMock }))

function setAuth(status: GitHubAuthStatus, session: unknown = null) {
  authState.status = status
  authState.session = session
}

function clickToastAction() {
  const action = toastMock.mock.calls.at(-1)?.[0]?.action
  if (!action) throw new Error('expected a toast action')
  act(() => action.props.onClick())
}

describe('SignInReminderToast', () => {
  beforeEach(() => {
    setAuth('loading')
    toastMock.mockClear()
    loginMock.mockClear()
    dismissMock.mockClear()
  })

  afterEach(() => {
    cleanup()
  })

  it('stays silent while the session is still resolving', () => {
    setAuth('loading')
    render(<SignInReminderToast />)
    expect(toastMock).not.toHaveBeenCalled()
  })

  it('nudges once when the load resolves to a signed-out state', () => {
    const { rerender } = render(<SignInReminderToast />)
    expect(toastMock).not.toHaveBeenCalled()

    setAuth('idle')
    rerender(<SignInReminderToast />)
    expect(toastMock).toHaveBeenCalledTimes(1)

    // A subsequent re-render must not re-fire the nudge.
    rerender(<SignInReminderToast />)
    expect(toastMock).toHaveBeenCalledTimes(1)
  })

  it('triggers GitHub login from the toast action', () => {
    const { rerender } = render(<SignInReminderToast />)
    setAuth('idle')
    rerender(<SignInReminderToast />)

    clickToastAction()
    expect(loginMock).toHaveBeenCalledTimes(1)
  })

  it('stays silent when the user is already signed in', () => {
    const { rerender } = render(<SignInReminderToast />)
    setAuth('ready', { user: { login: 'octocat' } })
    rerender(<SignInReminderToast />)
    expect(toastMock).not.toHaveBeenCalled()
  })

  it('does not nudge on a transient initial idle before any load', () => {
    setAuth('idle')
    render(<SignInReminderToast />)
    expect(toastMock).not.toHaveBeenCalled()
  })

  it('dismisses an open nudge once the user signs in', () => {
    const { rerender } = render(<SignInReminderToast />)
    setAuth('idle')
    rerender(<SignInReminderToast />)
    expect(toastMock).toHaveBeenCalledTimes(1)

    setAuth('ready', { user: { login: 'octocat' } })
    rerender(<SignInReminderToast />)
    expect(dismissMock).toHaveBeenCalledTimes(1)
  })
})
