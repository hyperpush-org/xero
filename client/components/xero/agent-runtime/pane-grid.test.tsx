/** @vitest-environment jsdom */

import { useEffect } from 'react'
import { DndContext } from '@dnd-kit/core'
import { render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  getPaneSortableDisabledState,
  PaneGrid,
  type PaneDragHandle,
  type PaneGridSlot,
} from './pane-grid'

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

  it('keeps a solo pane droppable while disabling pane dragging', () => {
    expect(getPaneSortableDisabledState(true)).toEqual({
      draggable: true,
      droppable: false,
    })
    expect(getPaneSortableDisabledState(false)).toBe(false)
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

  it('exposes a sortable drag handle to its rendered panes when more than one pane is open', async () => {
    const handles: Record<string, PaneDragHandle> = {}
    const renderPane = (slot: PaneGridSlot, _index: number, dragHandle: PaneDragHandle) => {
      handles[slot.paneId] = dragHandle
      return <div>{slot.paneId}</div>
    }

    render(
      <DndContext>
        <PaneGrid
          slots={[makeSlot('pane-1', true), makeSlot('pane-2')]}
          renderPane={renderPane}
        />
      </DndContext>,
    )

    await screen.findByText('pane-2')
    const receivedHandle = handles['pane-2']
    expect(receivedHandle).toBeDefined()
    expect(typeof receivedHandle.setActivatorNodeRef).toBe('function')
    expect(receivedHandle.attributes).toBeDefined()
    expect(receivedHandle.listeners).toBeDefined()

    const panes = Array.from(document.querySelectorAll('[data-pane-id]'))
    expect(panes.map((node) => node.getAttribute('data-pane-id'))).toEqual([
      'pane-1',
      'pane-2',
    ])
  })

  it('emits an empty drag handle when only one pane is mounted (sortable disabled)', async () => {
    const handles: PaneDragHandle[] = []
    const renderPane = (slot: PaneGridSlot, _index: number, dragHandle: PaneDragHandle) => {
      handles.push(dragHandle)
      return <div>{slot.paneId}</div>
    }

    render(
      <DndContext>
        <PaneGrid slots={[makeSlot('pane-1', true)]} renderPane={renderPane} />
      </DndContext>,
    )

    await screen.findByText('pane-1')
    expect(handles[0]).toEqual({})
  })
})
