import {
  FileText,
  Maximize2,
} from 'lucide-react'
import { useState } from 'react'

import { cn } from '../../lib/utils'
import { ImageLightbox } from '../image-lightbox'
import type { ConversationMessageAttachment } from './conversation-section'

export type ImageAttachmentPreviewVariant = 'tool' | 'response' | 'card'

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
  const title = attachmentDisplayName(attachment)
  if (attachment.kind === 'image' && attachmentPreviewSrc(attachment)) {
    return (
      <div
        className="flex max-w-[240px] items-center gap-2 rounded-lg border border-border/50 bg-muted/30 p-1.5 text-left text-foreground shadow-sm"
        title={title}
      >
        <ImageAttachmentPreview
          attachment={attachment}
          className="h-12 w-16 shrink-0"
          variant="card"
        />
        <div className="min-w-0 flex-1">
          <p className="m-0 truncate text-[12px] font-medium leading-tight">
            {title}
          </p>
          <p className="m-0 mt-0.5 truncate text-[10.5px] leading-tight text-muted-foreground">
            Image attachment
          </p>
        </div>
      </div>
    )
  }
  return (
    <div
      className="flex max-w-[240px] items-center gap-2 rounded-lg border border-border/50 bg-muted/30 p-1.5 text-left text-foreground shadow-sm"
      title={title}
    >
      <span className="flex h-12 w-12 shrink-0 items-center justify-center rounded-md bg-background text-muted-foreground ring-1 ring-border/40">
        <FileText
          className="h-4 w-4"
          aria-hidden="true"
        />
      </span>
      <div className="min-w-0 flex-1">
        <p className="m-0 truncate text-[12px] font-medium leading-tight">
          {title}
        </p>
        <p className="m-0 mt-0.5 truncate text-[10.5px] leading-tight text-muted-foreground">
          Attachment
        </p>
      </div>
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
    variant === 'card'
      ? 'h-full w-full object-cover'
      : variant === 'response'
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
