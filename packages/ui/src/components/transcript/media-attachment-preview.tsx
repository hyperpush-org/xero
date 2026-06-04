import {
  FileText,
  Maximize2,
} from 'lucide-react'
import { useState } from 'react'

import { cn } from '../../lib/utils'
import { ImageLightbox } from '../image-lightbox'
import type { ConversationMessageAttachment } from './conversation-section'

export type ImageAttachmentPreviewVariant = 'tool' | 'response'

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

  if (attachment.kind !== 'image' || !src) {
    return <AttachmentPreviewChip attachment={attachment} />
  }

  return (
    <ImageLightbox
      open={isPreviewOpen}
      onOpenChange={setIsPreviewOpen}
      src={src}
      title={title}
      alt={alt}
      dimensions={dimensions}
      mediaType={attachment.mediaType}
      downloadName={attachment.originalName || title}
      trigger={
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
      }
    />
  )
}
