/** @vitest-environment jsdom */

import { useEffect } from 'react'
import { render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { PaneGrid, type PaneGridSlot } from './pane-grid'

const resizeObserverMock = vi.hoisted(() => {
  return class ResizeObserverMock {
    observe() {}
    disconnect() {}
  }
})

function makeSlot(paneId: string, isFocused = false): PaneGridSlot {
  return {
    paneId,
    isFocused,
    ariaLabel: paneId,
  }
}

function MountProbe({
  paneId,
  onMount,
  onUnmount,
}: {
  paneId: string
  onMount: (paneId: string) => void
  onUnmount: (paneId: string) => void
}) {
  useEffect(() => {
    onMount(paneId)
    return () => onUnmount(paneId)
  }, [onMount, onUnmount, paneId])

  return <div>{paneId}</div>
}

describe('PaneGrid', () => {
  const originalResizeObserver = globalThis.ResizeObserver
  let rectSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    globalThis.ResizeObserver = resizeObserverMock as unknown as typeof ResizeObserver
    rectSpy = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      bottom: 800,
      height: 800,
      left: 0,
      right: 1200,
      top: 0,
      width: 1200,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    } as DOMRect)
  })

  afterEach(() => {
    globalThis.ResizeObserver = originalResizeObserver
    rectSpy.mockRestore()
  })

  it('keeps existing pane contents mounted while panes are added and removed', async () => {
    const onMount = vi.fn()
    const onUnmount = vi.fn()
    const renderPane = (slot: PaneGridSlot) => (
      <MountProbe paneId={slot.paneId} onMount={onMount} onUnmount={onUnmount} />
    )
    const { rerender } = render(
      <PaneGrid slots={[makeSlot('pane-1', true)]} renderPane={renderPane} />,
    )

    await screen.findByText('pane-1')
    expect(onMount).toHaveBeenCalledWith('pane-1')

    rerender(
      <PaneGrid
        slots={[makeSlot('pane-1'), makeSlot('pane-2', true)]}
        renderPane={renderPane}
      />,
    )

    await screen.findByText('pane-2')
    await waitFor(() => expect(onMount).toHaveBeenCalledWith('pane-2'))
    expect(onUnmount).not.toHaveBeenCalledWith('pane-1')

    rerender(
      <PaneGrid
        slots={[makeSlot('pane-1'), makeSlot('pane-2'), makeSlot('pane-3', true)]}
        renderPane={renderPane}
      />,
    )

    await screen.findByText('pane-3')
    await waitFor(() => expect(onMount).toHaveBeenCalledWith('pane-3'))
    expect(onUnmount).not.toHaveBeenCalledWith('pane-1')
    expect(onUnmount).not.toHaveBeenCalledWith('pane-2')

    rerender(
      <PaneGrid slots={[makeSlot('pane-1', true)]} renderPane={renderPane} />,
    )

    expect(screen.getByText('pane-1')).toBeInTheDocument()
    expect(onUnmount).not.toHaveBeenCalledWith('pane-1')
    await waitFor(() => expect(onUnmount).toHaveBeenCalledWith('pane-2'))
    expect(onUnmount).toHaveBeenCalledWith('pane-3')
  })
})
