import {
  Download,
  FileText,
  Maximize2,
  Minus,
  Plus,
  X,
} from 'lucide-react'
import { useCallback, useState } from 'react'

import { cn } from '../../lib/utils'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
} from '../ui/dialog'
import type { ConversationMessageAttachment } from './conversation-section'

export type ImageAttachmentPreviewVariant = 'tool' | 'response'

const IMAGE_LIGHTBOX_DEFAULT_SCALE = 0.72
const IMAGE_LIGHTBOX_MIN_SCALE = 0.42
const IMAGE_LIGHTBOX_MAX_SCALE = 1
const IMAGE_LIGHTBOX_SCALE_STEP = 0.14

export function ToolMediaAttachments({
  attachments,
  variant = 'tool',
}: {
  attachments: ConversationMessageAttachment[]
  variant?: ImageAttachmentPreviewVariant
}) {
  if (attachments.length === 0) return null
  const previewWidth =
    variant === 'response' ? 'max-w-[190px]' : 'max-w-[260px]'
  return (
    <div className="flex max-w-full flex-wrap gap-2">
      {attachments.map((attachment) => (
        <ImageAttachmentPreview
          key={attachment.id}
          attachment={attachment}
          className={previewWidth}
          variant={variant}
        />
      ))}
    </div>
  )
}

export function attachmentPreviewSrc(
  attachment: ConversationMessageAttachment,
): string | undefined {
  if (attachment.previewSrc) return attachment.previewSrc
  if (attachment.renderUrl) return attachment.renderUrl
  if (attachment.source?.kind === 'data_url') return attachment.source.dataUrl
  return undefined
}

export function attachmentDisplayName(
  attachment: ConversationMessageAttachment,
): string {
  return attachment.title?.trim() || attachment.originalName
}

export function AttachmentPreviewChip({
  attachment,
}: {
  attachment: ConversationMessageAttachment
}) {
  if (attachment.kind === 'image' && attachmentPreviewSrc(attachment)) {
    return (
      <ImageAttachmentPreview
        attachment={attachment}
        className="max-w-[260px]"
      />
    )
  }
  return (
    <div
      className="flex max-w-[260px] items-center gap-2 rounded-md border border-border/50 bg-muted/30 px-2 py-1 text-[11px] text-foreground"
      title={attachment.originalName}
    >
      <FileText
        className="h-3.5 w-3.5 shrink-0 text-muted-foreground"
        aria-hidden="true"
      />
      <span className="line-clamp-1 truncate">{attachment.originalName}</span>
    </div>
  )
}

export function ImageAttachmentPreview({
  attachment,
  className,
  variant = 'tool',
}: {
  attachment: ConversationMessageAttachment
  className?: string
  variant?: ImageAttachmentPreviewVariant
}) {
  const [isPreviewOpen, setIsPreviewOpen] = useState(false)
  const [previewScale, setPreviewScale] = useState(IMAGE_LIGHTBOX_DEFAULT_SCALE)
  const src = attachmentPreviewSrc(attachment)
  const title = attachmentDisplayName(attachment)
  const alt = attachment.alt?.trim() || title
  const dimensions =
    attachment.width && attachment.height
      ? `${attachment.width} x ${attachment.height}`
      : null
  const thumbnailClass =
    variant === 'response'
      ? 'max-h-32 w-auto max-w-full object-contain'
      : 'max-h-44 w-auto max-w-full object-contain'

  const handlePreviewOpenChange = useCallback((open: boolean) => {
    setIsPreviewOpen(open)
    if (open) setPreviewScale(IMAGE_LIGHTBOX_DEFAULT_SCALE)
  }, [])

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

  if (attachment.kind !== 'image' || !src) {
    return <AttachmentPreviewChip attachment={attachment} />
  }

  const previewScalePercent = Math.round(previewScale * 100)
  const previewStyle = {
    maxWidth: `min(${Math.round(94 * previewScale)}vw, ${Math.round(
      1680 * previewScale,
    )}px)`,
    maxHeight: `${Math.round(86 * previewScale)}vh`,
  }

  return (
    <Dialog open={isPreviewOpen} onOpenChange={handlePreviewOpenChange}>
      <DialogTrigger asChild>
        <button
          type="button"
          className={cn(
            'group/image relative overflow-hidden rounded-md border border-border/50 bg-background text-left shadow-sm',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
            className,
          )}
          title={title}
          aria-label={`Open image preview for ${title}`}
        >
          <img
            src={src}
            alt={alt}
            className={cn('block', thumbnailClass)}
            draggable={false}
          />
          <span
            aria-hidden="true"
            className="absolute right-1.5 top-1.5 inline-flex h-6 w-6 items-center justify-center rounded-md bg-background/80 text-muted-foreground opacity-0 shadow-sm ring-1 ring-border/50 backdrop-blur transition-opacity group-hover/image:opacity-100"
          >
            <Maximize2 className="h-3.5 w-3.5" />
          </span>
        </button>
      </DialogTrigger>
      <DialogContent
        overlayClassName="bg-black/80 backdrop-blur-[4px]"
        showCloseButton={false}
        className={cn(
          'left-0 top-0 h-screen w-screen max-w-none translate-x-0 translate-y-0 gap-0 rounded-none border-0 bg-transparent p-0 shadow-none',
          'sm:max-w-none',
        )}
      >
        <DialogTitle className="sr-only">{title}</DialogTitle>
        <DialogDescription className="sr-only">
          {dimensions ?? attachment.mediaType}
        </DialogDescription>
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
              download={attachment.originalName || title}
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
      </DialogContent>
    </Dialog>
  )
}
