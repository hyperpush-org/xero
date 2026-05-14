"use client"

import { PlaySquare } from "lucide-react"

import {
  StartTargetsEditor,
  type StartTargetsSuggestRequest,
  type SuggestedTarget,
} from "@/components/xero/start-targets-editor"
import type { StartTargetDto, StartTargetInputDto } from "@/src/lib/xero-desktop"

import { SectionHeader } from "./section-header"
import { EmptyPanel } from "./_shared"

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
      <div className="flex flex-col gap-7">
        <SectionHeader
          title="Project Runner"
          description="Configure the shell commands Xero runs from the titlebar Play button. One target = a single root command. Add more for monorepos or multi-process apps."
        />
        <EmptyPanel
          icon={<PlaySquare className="h-5 w-5 text-muted-foreground/70" />}
          title="Select a project"
          body="Start targets are scoped to the active project."
        />
      </div>
    )
  }

  const projectName = projectLabel ?? projectId

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Project Runner"
        description={`Configure the shell commands Xero runs from the titlebar Play button for ${projectName}. One target = a single root command. Add more for monorepos or multi-process apps.`}
      />

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
