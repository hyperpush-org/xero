"use client"

import { Play } from "lucide-react"
import { BaseDialog } from "@xero/ui/components/base-dialog"

import {
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  StartTargetsEditor,
  type StartTargetsSuggestRequest,
  type SuggestedTarget,
} from "@/components/xero/start-targets-editor"
import type { StartTargetDto, StartTargetInputDto } from "@/src/lib/xero-desktop"

export type StartTargetsDialogSuggestRequest = StartTargetsSuggestRequest

interface StartTargetsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  projectName: string
  initialTargets: StartTargetDto[]
  onSubmit: (targets: StartTargetInputDto[]) => Promise<void>
  resolveSuggestRequest?: () => StartTargetsSuggestRequest | null
  onSuggest?: (
    request: StartTargetsSuggestRequest,
  ) => Promise<{ targets: SuggestedTarget[] }>
}

export function StartTargetsDialog({
  open,
  onOpenChange,
  projectName,
  initialTargets,
  onSubmit,
  resolveSuggestRequest,
  onSuggest,
}: StartTargetsDialogProps) {
  return (
    <BaseDialog
      open={open}
      onOpenChange={onOpenChange}
      variant="editor"
      title="Project start targets"
      contentClassName="grid max-h-[min(760px,calc(100vh-4rem))] grid-rows-[auto_minmax(0,1fr)] gap-0 overflow-hidden p-0 sm:max-w-[560px]"
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
                <Play className="h-4 w-4 fill-current" />
              </span>
              <DialogTitle className="text-[15px]">Project start targets</DialogTitle>
            </div>
            <DialogDescription className="text-[12.5px] leading-relaxed">
              Define the shell commands Xero runs from the titlebar Play button
              for <span className="font-mono text-foreground/80">{projectName}</span>.
              Use one target for a simple project, or add more for monorepos
              and multi-process apps.
            </DialogDescription>
          </DialogHeader>
        </div>
      }
    >

        <div className="relative min-h-0">
          <StartTargetsEditor
            className="h-full"
            fixedFooter
            initialTargets={initialTargets}
            onSave={async (targets) => {
              await onSubmit(targets)
            }}
            onSaved={() => onOpenChange(false)}
            resolveSuggestRequest={resolveSuggestRequest}
            onSuggest={onSuggest}
            showModelSelector={false}
          />
        </div>
    </BaseDialog>
  )
}
