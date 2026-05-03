"use client"

import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { cn } from '@/lib/utils'

export interface AgentPaneDropOverlayProps {
  enabled: boolean
  onFilesDropped: (files: File[]) => void
  children: ReactNode
  /**
   * Optional label shown inside the overlay while files are being dragged
   * over the pane. Defaults to "Drop files to attach".
   */
  label?: string
  className?: string
}

function isExternalFileDrag(event: React.DragEvent<HTMLDivElement>): boolean {
  const types = event.dataTransfer?.types
  if (!types) return false
  for (let index = 0; index < types.length; index += 1) {
    if (types[index] === 'Files') {
      return true
    }
  }
  return false
}

export function AgentPaneDropOverlay({
  enabled,
  onFilesDropped,
  children,
  label = 'Drop files to attach',
  className,
}: AgentPaneDropOverlayProps) {
  const [isOver, setIsOver] = useState(false)
  const enterCountRef = useRef(0)

  const handleDragEnter = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (!enabled) return
      if (!isExternalFileDrag(event)) return
      event.preventDefault()
      enterCountRef.current += 1
      if (enterCountRef.current === 1) {
        setIsOver(true)
      }
    },
    [enabled],
  )

  const handleDragOver = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (!enabled) return
      if (!isExternalFileDrag(event)) return
      event.preventDefault()
      if (event.dataTransfer) {
        event.dataTransfer.dropEffect = 'copy'
      }
    },
    [enabled],
  )

  const handleDragLeave = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (!enabled) return
      if (!isExternalFileDrag(event)) return
      enterCountRef.current = Math.max(0, enterCountRef.current - 1)
      if (enterCountRef.current === 0) {
        setIsOver(false)
      }
    },
    [enabled],
  )

  const handleDrop = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (!enabled) return
      if (!isExternalFileDrag(event)) return
      event.preventDefault()
      enterCountRef.current = 0
      setIsOver(false)
      const files = Array.from(event.dataTransfer?.files ?? [])
      if (files.length > 0) {
        onFilesDropped(files)
      }
    },
    [enabled, onFilesDropped],
  )

  useEffect(() => {
    if (!enabled) {
      enterCountRef.current = 0
      setIsOver(false)
    }
  }, [enabled])

  return (
    <div
      className={cn('relative flex min-h-0 min-w-0 flex-1 flex-col', className)}
      onDragEnter={handleDragEnter}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {children}
      {isOver ? (
        <div
          aria-hidden
          data-testid="agent-pane-drop-overlay"
          className="pointer-events-none absolute inset-0 z-40 flex items-center justify-center rounded-lg border-2 border-dashed border-primary/60 bg-primary/10 backdrop-blur-[1px]"
        >
          <span className="rounded-md bg-background/85 px-4 py-2 text-sm font-medium text-foreground shadow-sm">
            {label}
          </span>
        </div>
      ) : null}
    </div>
  )
}
