import { describe, expect, it } from 'vitest'
import { installNativeTitleSuppression } from './native-title-suppression'

async function flushMutationObserver() {
  await Promise.resolve()
}

describe('installNativeTitleSuppression', () => {
  it('removes native title attributes from the existing document tree', () => {
    document.body.innerHTML = '<button title="Create">Create</button>'

    const handle = installNativeTitleSuppression(document)

    expect(document.querySelector('[title]')).toBeNull()
    handle.dispose()
  })

  it('removes native title attributes added after installation', async () => {
    document.body.innerHTML = '<div id="root"></div>'

    const handle = installNativeTitleSuppression(document)
    const button = document.createElement('button')
    button.setAttribute('title', 'Refresh')
    document.getElementById('root')?.append(button)

    await flushMutationObserver()

    expect(button).not.toHaveAttribute('title')
    handle.dispose()
  })

  it('removes native title attributes that React-style updates add later', async () => {
    document.body.innerHTML = '<button>Run</button>'

    const handle = installNativeTitleSuppression(document)
    const button = document.querySelector('button')
    button?.setAttribute('title', 'Run project')

    await flushMutationObserver()

    expect(button).not.toHaveAttribute('title')
    handle.dispose()
  })

  it('promotes title text to aria-label for unlabeled icon-only controls', () => {
    document.body.innerHTML = '<button title="Open command palette"><svg aria-hidden="true"></svg></button>'

    const handle = installNativeTitleSuppression(document)
    const button = document.querySelector('button')

    expect(button).not.toHaveAttribute('title')
    expect(button).toHaveAttribute('aria-label', 'Open command palette')
    handle.dispose()
  })

  it('keeps existing accessible names instead of replacing them', () => {
    document.body.innerHTML = '<button aria-label="Open browser" title="Browser"><svg aria-hidden="true"></svg></button>'

    const handle = installNativeTitleSuppression(document)
    const button = document.querySelector('button')

    expect(button).not.toHaveAttribute('title')
    expect(button).toHaveAttribute('aria-label', 'Open browser')
    handle.dispose()
  })

  it('stops observing future title attributes after disposal', async () => {
    document.body.innerHTML = '<button>Settings</button>'

    const handle = installNativeTitleSuppression(document)
    handle.dispose()

    const button = document.querySelector('button')
    button?.setAttribute('title', 'Settings')

    await flushMutationObserver()

    expect(button).toHaveAttribute('title', 'Settings')
  })
})
