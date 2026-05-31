import { type ReactNode } from 'react'
import { ArrowLeft, ChevronRight } from 'lucide-react'
import { BaseDialog } from '@xero/ui/components/base-dialog'

import { Button } from '@/components/ui/button'
import {
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { cn } from '@/lib/utils'

export type CreateEntityDialogView = 'choice' | 'templates'

export interface CreateEntityDialogChoice {
  icon: ReactNode
  title: string
  description: string
  onClick: () => void
  disabled?: boolean
}

interface CreateEntityDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  view: CreateEntityDialogView
  onSetView: (view: CreateEntityDialogView) => void
  title: string
  icon: ReactNode
  choiceDescription: string
  templatesDescription: string
  footerNote: string
  blankChoice: CreateEntityDialogChoice
  extraChoices?: CreateEntityDialogChoice[]
  templateChoice?: CreateEntityDialogChoice
  templatesContent: ReactNode
}

export function CreateEntityDialog({
  open,
  onOpenChange,
  view,
  onSetView,
  title,
  icon,
  choiceDescription,
  templatesDescription,
  footerNote,
  blankChoice,
  extraChoices = [],
  templateChoice,
  templatesContent,
}: CreateEntityDialogProps) {
  const isChoice = view === 'choice'
  return (
    <BaseDialog
      open={open}
      onOpenChange={onOpenChange}
      variant="custom"
      title={title}
      contentClassName="gap-0 overflow-hidden p-0 sm:max-w-[460px]"
      leading={
        <div
          aria-hidden
          className="pointer-events-none absolute inset-x-0 top-0 h-32 bg-gradient-to-b from-primary/[0.06] to-transparent"
        />
      }
      header={
        <div className="relative px-6 pb-2 pt-6">
          <DialogHeader className="space-y-2">
            <div className="flex items-center gap-2.5">
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-primary/30 bg-primary/10 text-primary">
                {icon}
              </span>
              <DialogTitle className="text-[15px]">{title}</DialogTitle>
            </div>
            <DialogDescription className="text-[12.5px] leading-relaxed">
              {isChoice ? choiceDescription : templatesDescription}
            </DialogDescription>
          </DialogHeader>
        </div>
      }
      footerClassName="border-t border-border/60 bg-secondary/20 px-6 py-3 sm:justify-between"
      footer={
        isChoice ? (
          <>
            <p className="hidden text-[11px] text-muted-foreground/70 sm:block">
              {footerNote}
            </p>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => onOpenChange(false)}
              className="text-muted-foreground hover:text-foreground"
            >
              Cancel
            </Button>
          </>
        ) : (
          <>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => onSetView('choice')}
              className="text-muted-foreground hover:text-foreground"
            >
              <ArrowLeft className="h-3.5 w-3.5" />
              Back
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => onOpenChange(false)}
              className="text-muted-foreground hover:text-foreground"
            >
              Cancel
            </Button>
          </>
        )
      }
    >
        <div className="relative mt-4 px-6 pb-5">
          {isChoice ? (
            <div className="flex flex-col gap-2">
              <ChoiceCard {...blankChoice} />
              {extraChoices.map((choice) => (
                <ChoiceCard key={choice.title} {...choice} />
              ))}
              {templateChoice ? <ChoiceCard {...templateChoice} /> : null}
            </div>
          ) : (
            templatesContent
          )}
        </div>
    </BaseDialog>
  )
}

interface ChoiceCardProps {
  icon: ReactNode
  title: string
  description: string
  onClick: () => void
  disabled?: boolean
}

export function ChoiceCard({ icon, title, description, onClick, disabled = false }: ChoiceCardProps) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className={cn(
        'group relative flex items-center gap-3 rounded-lg border border-border/60 bg-card/40 px-3.5 py-3 text-left transition-all',
        disabled
          ? 'cursor-not-allowed opacity-50'
          : 'hover:border-primary/40 hover:bg-primary/[0.04] focus-visible:border-primary/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30',
      )}
    >
      <span
        className={cn(
          'flex h-9 w-9 shrink-0 items-center justify-center rounded-md border transition-colors',
          'border-border/60 bg-secondary/60 text-muted-foreground',
          'group-hover:border-primary/40 group-hover:bg-primary/10 group-hover:text-primary',
        )}
      >
        {icon}
      </span>
      <div className="min-w-0 flex-1 space-y-0.5">
        <div className="text-[13px] font-medium text-foreground">{title}</div>
        <div className="text-[11.5px] leading-snug text-muted-foreground">{description}</div>
      </div>
      <ChevronRight
        className={cn(
          'h-4 w-4 shrink-0 text-muted-foreground/50 transition-all',
          !disabled && 'group-hover:translate-x-0.5 group-hover:text-primary',
        )}
      />
    </button>
  )
}
