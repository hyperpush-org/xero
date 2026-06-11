'use client'

import { Download, Minus, Plus, X } from 'lucide-react'
import { type MouseEvent, type ReactNode, useCallback, useState } from 'react'

import { cn } from '../lib/utils'
import { BaseDialog } from './base-dialog'
import {
  DialogClose,
  DialogDescription,
  DialogTitle,
} from './ui/dialog'

const IMAGE_LIGHTBOX_DEFAULT_SCALE = 0.72
const IMAGE_LIGHTBOX_MIN_SCALE = 0.42
const IMAGE_LIGHTBOX_MAX_SCALE = 1
const IMAGE_LIGHTBOX_SCALE_STEP = 0.14
const DIRECT_DOWNLOAD_PROTOCOL_PATTERN = /^(?:https?:|blob:|data:)/i

function shouldIsolateImageNavigation(src: string): boolean {
  return !DIRECT_DOWNLOAD_PROTOCOL_PATTERN.test(src)
}

export interface ImageLightboxProps {
  alt: string
  dimensions?: string | null
  downloadName?: string
  mediaType?: string | null
  onOpenChange: (open: boolean) => void
  open: boolean
  src: string
  title: string
  trigger: ReactNode
}

export function ImageLightbox({
  alt,
  dimensions = null,
  downloadName,
  mediaType = null,
  onOpenChange,
  open,
  src,
  title,
  trigger,
}: ImageLightboxProps) {
  const [previewScale, setPreviewScale] = useState(IMAGE_LIGHTBOX_DEFAULT_SCALE)

  const handlePreviewOpenChange = useCallback(
    (nextOpen: boolean) => {
      onOpenChange(nextOpen)
      if (nextOpen) setPreviewScale(IMAGE_LIGHTBOX_DEFAULT_SCALE)
    },
    [onOpenChange],
  )

  const decreasePreviewScale = useCallback(() => {
    setPreviewScale((current) =>
      Math.max(
        IMAGE_LIGHTBOX_MIN_SCALE,
        Number((current - IMAGE_LIGHTBOX_SCALE_STEP).toFixed(2)),
      ),
    )
  }, [])

  const increasePreviewScale = useCallback(() => {
    setPreviewScale((current) =>
      Math.min(
        IMAGE_LIGHTBOX_MAX_SCALE,
        Number((current + IMAGE_LIGHTBOX_SCALE_STEP).toFixed(2)),
      ),
    )
  }, [])

  const handleDownloadClick = useCallback(
    (event: MouseEvent<HTMLAnchorElement>) => {
      event.stopPropagation()
      if (!shouldIsolateImageNavigation(src)) return

      // Custom app asset schemes can replace the Tauri webview if followed normally.
      event.preventDefault()
      window.open(src, '_blank', 'noopener,noreferrer')
    },
    [src],
  )

  const previewScalePercent = Math.round(previewScale * 100)
  const previewStyle = {
    maxWidth: `min(${Math.round(94 * previewScale)}vw, ${Math.round(
      1680 * previewScale,
    )}px)`,
    maxHeight: `${Math.round(86 * previewScale)}vh`,
  }

  return (
    <BaseDialog
      open={open}
      onOpenChange={handlePreviewOpenChange}
      variant="custom"
      title={title}
      overlayClassName="bg-black/80 backdrop-blur-[4px]"
      showCloseButton={false}
      contentClassName={cn(
        'left-0 top-0 h-screen w-screen max-w-none translate-x-0 translate-y-0 gap-0 rounded-none border-0 bg-transparent p-0 shadow-none',
        'sm:max-w-none',
      )}
      header={
        <>
          <DialogTitle className="sr-only">{title}</DialogTitle>
          <DialogDescription className="sr-only">
            {dimensions ?? mediaType ?? 'Image preview'}
          </DialogDescription>
        </>
      }
      trigger={trigger}
    >
      <div className="relative flex h-full w-full items-center justify-center px-5 pb-24 pt-20 sm:px-10 sm:pb-28 sm:pt-24">
        <img
          src={src}
          alt={alt}
          style={previewStyle}
          className={cn(
            'block rounded-[6px] object-contain',
            'shadow-[0_34px_130px_rgba(0,0,0,0.78)] transition-[max-width,max-height] duration-150 ease-out',
          )}
          draggable={false}
        />
        <div className="fixed right-4 top-4 z-10 flex items-center gap-3 sm:right-7 sm:top-7">
          <a
            href={src}
            download={downloadName || title}
            target="_blank"
            rel="noreferrer noopener"
            onClick={handleDownloadClick}
            className={cn(
              'inline-flex h-12 w-12 items-center justify-center rounded-full',
              'bg-white/10 text-white shadow-[0_18px_48px_rgba(0,0,0,0.45)] ring-1 ring-white/10 backdrop-blur-md',
              'transition hover:bg-white/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/80',
            )}
            aria-label={`Download ${title}`}
          >
            <Download className="h-5 w-5" aria-hidden="true" />
          </a>
          <DialogClose asChild>
            <button
              type="button"
              className={cn(
                'inline-flex h-12 w-12 items-center justify-center rounded-full',
                'bg-white/10 text-white shadow-[0_18px_48px_rgba(0,0,0,0.45)] ring-1 ring-white/10 backdrop-blur-md',
                'transition hover:bg-white/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/80',
              )}
              aria-label="Close image preview"
            >
              <X className="h-5 w-5" aria-hidden="true" />
            </button>
          </DialogClose>
        </div>
        <div
          className={cn(
            'fixed bottom-8 left-1/2 z-10 flex -translate-x-1/2 items-center gap-1.5 rounded-full p-1.5',
            'bg-white/10 text-white shadow-[0_22px_60px_rgba(0,0,0,0.52)] ring-1 ring-white/10 backdrop-blur-md',
          )}
        >
          <button
            type="button"
            className={cn(
              'inline-flex h-10 w-10 items-center justify-center rounded-full transition',
              'hover:bg-white/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/80',
              'disabled:pointer-events-none disabled:opacity-40',
            )}
            onClick={decreasePreviewScale}
            disabled={previewScale <= IMAGE_LIGHTBOX_MIN_SCALE}
            aria-label="Zoom out"
          >
            <Minus className="h-4 w-4" aria-hidden="true" />
          </button>
          <span className="min-w-14 text-center text-sm font-semibold tabular-nums">
            {previewScalePercent}%
          </span>
          <button
            type="button"
            className={cn(
              'inline-flex h-10 w-10 items-center justify-center rounded-full transition',
              'hover:bg-white/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/80',
              'disabled:pointer-events-none disabled:opacity-40',
            )}
            onClick={increasePreviewScale}
            disabled={previewScale >= IMAGE_LIGHTBOX_MAX_SCALE}
            aria-label="Zoom in"
          >
            <Plus className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
      </div>
    </BaseDialog>
  )
}
