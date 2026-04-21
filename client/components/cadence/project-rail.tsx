import { useState } from 'react'
import { Folder, Loader2, MoreHorizontal, Plus, RefreshCw, Trash2 } from 'lucide-react'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { buttonVariants } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import type { ProjectListItem } from '@/src/lib/cadence-model'

interface ProjectRailProps {
  projects: ProjectListItem[]
  activeProjectId: string | null
  isLoading: boolean
  isImporting: boolean
  projectRemovalStatus: 'idle' | 'running'
  pendingProjectRemovalId: string | null
  errorMessage: string | null
  onSelectProject: (projectId: string) => void
  onImportProject: () => void
  onRemoveProject: (projectId: string) => void
}

export function ProjectRail({
  projects,
  activeProjectId,
  isLoading,
  isImporting,
  projectRemovalStatus,
  pendingProjectRemovalId,
  errorMessage,
  onSelectProject,
  onImportProject,
  onRemoveProject,
}: ProjectRailProps) {
  const isRemovingProject = projectRemovalStatus === 'running'

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-border bg-sidebar">
      <div className="flex h-10 items-center justify-between border-b border-border px-3">
        <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">Projects</span>
        <button
          aria-label="Import repository"
          className="rounded p-1 text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground disabled:opacity-50"
          disabled={isImporting || isRemovingProject}
          onClick={onImportProject}
          type="button"
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
      </div>

      {errorMessage ? (
        <div className="border-b border-border px-3 py-2 text-[11px] text-destructive">{errorMessage}</div>
      ) : null}

      <div className="flex-1 overflow-y-auto scrollbar-thin pb-1">
        {projects.length === 0 ? (
          <div className="px-3 py-4 text-[12px] text-muted-foreground">No projects imported yet.</div>
        ) : (
          projects.map((project) => (
            <ProjectRailItem
              key={project.id}
              project={project}
              isActive={project.id === activeProjectId}
              isRemovalPending={project.id === pendingProjectRemovalId}
              isRemovalLocked={isRemovingProject}
              onRemoveProject={onRemoveProject}
              onSelectProject={onSelectProject}
            />
          ))
        )}
      </div>

      {(isLoading || isImporting || isRemovingProject) && (
        <div className="flex items-center gap-2 border-t border-border px-3 py-2.5 text-[11px] text-muted-foreground">
          <RefreshCw className="h-3 w-3 animate-spin" />
          <span>{isImporting ? 'Importing…' : isRemovingProject ? 'Removing…' : 'Refreshing…'}</span>
        </div>
      )}
    </aside>
  )
}

interface ProjectRailItemProps {
  project: ProjectListItem
  isActive: boolean
  isRemovalPending: boolean
  isRemovalLocked: boolean
  onSelectProject: (projectId: string) => void
  onRemoveProject: (projectId: string) => void
}

function ProjectRailItem({
  project,
  isActive,
  isRemovalPending,
  isRemovalLocked,
  onSelectProject,
  onRemoveProject,
}: ProjectRailItemProps) {
  const [confirmOpen, setConfirmOpen] = useState(false)

  return (
    <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
      <div className="group relative">
        <button
          className={`w-full px-3 py-2.5 text-left transition-colors ${isActive ? 'bg-secondary' : 'hover:bg-secondary/40'}`}
          onClick={() => onSelectProject(project.id)}
          type="button"
        >
          <div className="mb-1 flex items-center gap-2">
            <Folder className={`h-3.5 w-3.5 shrink-0 ${isActive ? 'text-primary' : 'text-muted-foreground'}`} />
            <span className={`truncate text-[13px] font-medium ${isActive ? 'text-foreground' : 'text-foreground/80'}`}>
              {project.name}
            </span>
          </div>

          <p className="mb-2 ml-[22px] truncate text-[11px] text-muted-foreground">
            {project.milestone || 'No milestone'}
          </p>

          <div className="ml-[22px] flex items-center gap-2">
            <div className="h-1 flex-1 overflow-hidden rounded-full bg-border">
              <div
                className="h-full rounded-full bg-primary/60 transition-all"
                style={{ width: `${project.phaseProgressPercent}%` }}
              />
            </div>
            <span className="w-8 text-[10px] font-mono tabular-nums text-muted-foreground">
              {project.phaseProgressPercent}%
            </span>
          </div>
        </button>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              aria-label={`Project actions for ${project.name}`}
              className={`absolute top-2 right-2 z-10 rounded p-1 text-muted-foreground transition-colors hover:bg-secondary/80 hover:text-foreground disabled:opacity-50 ${
                isActive || isRemovalPending ? 'opacity-100' : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100'
              }`}
              disabled={isRemovalLocked}
              type="button"
            >
              {isRemovalPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <MoreHorizontal className="h-3.5 w-3.5" />}
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem
              onSelect={(event) => {
                event.preventDefault()
                setConfirmOpen(true)
              }}
              variant="destructive"
            >
              <Trash2 className="h-4 w-4" />
              Remove
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove {project.name} from the sidebar?</AlertDialogTitle>
          <AlertDialogDescription>
            Cadence will only forget this project in the desktop registry. The repo, the local{' '}
            <code className="mx-1 rounded bg-muted px-1 py-0.5 text-xs text-foreground">.cadence</code>{' '}
            database, and the rest of the project state stay untouched. You can import the same folder again any time.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isRemovalPending}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            className={buttonVariants({ variant: 'destructive' })}
            disabled={isRemovalPending}
            onClick={() => onRemoveProject(project.id)}
          >
            {isRemovalPending ? 'Removing…' : 'Remove'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
