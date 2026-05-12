"use client"

import { Play } from "lucide-react"

import {
  StartTargetsEditor,
  type StartTargetsSuggestRequest,
  type SuggestedTarget,
} from "@/components/xero/start-targets-editor"
import type { StartTargetDto, StartTargetInputDto } from "@/src/lib/xero-desktop"

export type ProjectRunnerSuggestRequest = StartTargetsSuggestRequest

interface ProjectRunnerSectionProps {
  projectId: string | null
  projectLabel: string | null
  startTargets: StartTargetDto[]
  onSave?: (targets: StartTargetInputDto[]) => Promise<void>
  resolveSuggestRequest?: () => ProjectRunnerSuggestRequest | null
  onSuggest?: (
    request: ProjectRunnerSuggestRequest,
  ) => Promise<{ targets: SuggestedTarget[] }>
}

export function ProjectRunnerSection({
  projectId,
  projectLabel,
  startTargets,
  onSave,
  resolveSuggestRequest,
  onSuggest,
}: ProjectRunnerSectionProps) {
  if (!projectId) {
    return (
      <div className="p-6 text-[13px] text-muted-foreground">
        Open a project to configure its start targets.
      </div>
    )
  }

  const projectName = projectLabel ?? projectId

  return (
    <div className="flex h-full min-h-0 flex-col gap-6 overflow-auto p-6">
      <div className="space-y-1">
        <div className="flex items-center gap-2 text-[13px] font-semibold text-foreground">
          <Play className="h-4 w-4" />
          Project runner
        </div>
        <p className="text-[12.5px] text-muted-foreground">
          Configure the shell commands Xero runs from the titlebar Play button
          for <span className="font-mono">{projectName}</span>. One target = a
          single root command. Add more for monorepos or multi-process apps.
        </p>
      </div>

      <StartTargetsEditor
        key={projectId}
        initialTargets={startTargets}
        onSave={async (targets) => {
          if (!onSave) return
          await onSave(targets)
        }}
        resolveSuggestRequest={resolveSuggestRequest}
        onSuggest={onSuggest}
      />
    </div>
  )
}
