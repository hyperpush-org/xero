"use client"

import { Check, FolderGit2, FolderOpen, Loader2 } from "lucide-react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { StepHeader } from "./providers-step"

interface ImportedProjectView {
  name: string
  path: string
}

interface ProjectStepProps {
  project: ImportedProjectView | null
  isImporting: boolean
  isProjectLoading: boolean
  errorMessage: string | null
  onImportProject: () => void
}

export function ProjectStep({
  project,
  isImporting,
  isProjectLoading,
  errorMessage,
  onImportProject,
}: ProjectStepProps) {
  const isBusy = isImporting || isProjectLoading

  // Only show the project card once the full load is complete.
  //
  // The import flow has two async phases: the initial snapshot load
  // (which sets `project` to a partial value mid-import) and the runtime
  // load (which finalises it). If we render the project card as soon as
  // `project` becomes non-null while `isBusy` is still true we get a
  // jarring mid-import layout switch: the "Importing repository…" button
  // unmounts, the project card mounts and animates in, then runtime data
  // arrives and updates the text again — all while the spinner is still
  // active. Holding the card until the busy phase ends gives a single,
  // clean transition from loading state to finished state.
  const showProjectCard = !isBusy && project !== null

  return (
    <div>
      <StepHeader
        title="Add a project"
        description="Projects stay separate from provider setup. Import a local repository when you're ready to work in it."
      />

      {errorMessage ? (
        <Alert variant="destructive" className="mt-5 py-3">
          <AlertDescription className="text-[12px]">{errorMessage}</AlertDescription>
        </Alert>
      ) : null}

      {showProjectCard ? (
        <div className="mt-7 animate-in fade-in-0 slide-in-from-bottom-1 motion-enter [animation-delay:60ms] [animation-fill-mode:both]">
          <div className="overflow-hidden rounded-lg border border-primary/40 bg-primary/[0.03]">
            <div className="flex items-center gap-3 px-4 py-3">
              <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md border border-primary/40 bg-primary/10 text-primary">
                <FolderGit2 className="h-4 w-4" />
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <p className="truncate text-[13px] font-medium text-foreground">{project!.name}</p>
                  <span className="inline-flex items-center gap-0.5 rounded-sm border border-emerald-500/30 bg-emerald-500/10 px-1 py-0 text-[9.5px] font-medium text-emerald-500 dark:text-emerald-400">
                    <Check className="h-2.5 w-2.5" strokeWidth={3} />
                    Imported
                  </span>
                </div>
                <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">{project!.path}</p>
              </div>
            </div>
          </div>
          <div className="mt-2 flex items-center justify-end">
            <Button
              variant="ghost"
              size="sm"
              onClick={onImportProject}
              className="h-7 gap-1.5 px-2 text-[11px] text-muted-foreground hover:text-foreground"
            >
              <FolderOpen className="h-3 w-3" />
              Pick a different folder
            </Button>
          </div>
        </div>
      ) : (
        <button
          type="button"
          onClick={onImportProject}
          disabled={isBusy}
          className={cn(
            "group/drop mt-7 flex w-full animate-in fade-in-0 slide-in-from-bottom-1 motion-enter items-center gap-3 rounded-lg border border-dashed bg-card/30 px-4 py-5 text-left transition-colors [animation-delay:60ms] [animation-fill-mode:both]",
            isBusy
              ? "border-primary/40 bg-card/50"
              : "border-border hover:border-primary/40 hover:bg-card/50",
          )}
        >
          <span
            className={cn(
              "flex h-10 w-10 shrink-0 items-center justify-center rounded-md border transition-colors",
              isBusy
                ? "border-primary/40 bg-primary/10 text-primary"
                : "border-border bg-secondary/60 text-muted-foreground group-hover/drop:text-foreground/80",
            )}
          >
            {isBusy ? <Loader2 className="h-4 w-4 animate-spin" /> : <FolderOpen className="h-4 w-4" />}
          </span>
          <div className="flex-1">
            <p className="text-[13px] font-medium text-foreground">
              {isBusy ? "Importing repository…" : "Choose a folder"}
            </p>
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              {isBusy
                ? "Loading the repository snapshot and workspace state."
                : "Select a local Git repository."}
            </p>
          </div>
        </button>
      )}
    </div>
  )
}
