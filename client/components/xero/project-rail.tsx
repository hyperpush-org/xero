import { memo, useCallback, useEffect, useState } from 'react'
import { Loader2, Plus, RefreshCw, Settings } from 'lucide-react'

import { cn } from '@/lib/utils'
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
import type { ProjectListItem } from '@/src/lib/xero-model'

interface ProjectRailProps {
  projects: ProjectListItem[]
  activeProjectId: string | null
  isLoading: boolean
  isImporting: boolean
  projectRemovalStatus: 'idle' | 'running'
  pendingProjectRemovalId: string | null
  pendingProjectSelectionId?: string | null
  errorMessage: string | null
  onSelectProject: (projectId: string) => void
  onImportProject: () => void
  onRemoveProject: (projectId: string) => void
  onOpenSettings?: () => void
  onSessionsHoverEnter?: () => void
  onSessionsHoverLeave?: () => void
}

export function ProjectRail({
  projects,
  activeProjectId,
  isLoading,
  isImporting,
  projectRemovalStatus,
  pendingProjectRemovalId,
  pendingProjectSelectionId = null,
  errorMessage,
  onSelectProject,
  onImportProject,
  onRemoveProject,
  onOpenSettings,
  onSessionsHoverEnter,
  onSessionsHoverLeave,
}: ProjectRailProps) {
  const isRemovingProject = projectRemovalStatus === 'running'
  const isBusy = isLoading || isImporting || isRemovingProject
  const [optimisticProjectId, setOptimisticProjectId] = useState<string | null>(null)
  const displayedActiveProjectId =
    optimisticProjectId ?? pendingProjectSelectionId ?? activeProjectId

  useEffect(() => {
    if (!optimisticProjectId) return
    if (
      activeProjectId === optimisticProjectId ||
      !projects.some((project) => project.id === optimisticProjectId)
    ) {
      setOptimisticProjectId(null)
    }
  }, [activeProjectId, optimisticProjectId, projects])

  const handleSelectProject = useCallback(
    (projectId: string) => {
      setOptimisticProjectId((current) => {
        const currentDisplayed = current ?? pendingProjectSelectionId ?? activeProjectId
        return currentDisplayed === projectId ? current : projectId
      })
      onSelectProject(projectId)
    },
    [activeProjectId, onSelectProject, pendingProjectSelectionId],
  )

  const handlePreviewProject = useCallback(
    (projectId: string) => {
      setOptimisticProjectId((current) => {
        const currentDisplayed = current ?? pendingProjectSelectionId ?? activeProjectId
        return currentDisplayed === projectId ? current : projectId
      })
    },
    [activeProjectId, pendingProjectSelectionId],
  )

  return (
    <aside
      aria-label="Projects"
      className="sidebar-layout-island relative flex w-12 shrink-0 flex-col overflow-hidden border-r border-border/70 bg-sidebar/95"
      data-collapsed="true"
      onPointerEnter={onSessionsHoverEnter}
      onPointerLeave={onSessionsHoverLeave}
    >
      {errorMessage ? (
        <div
          className="border-b border-destructive/30 bg-destructive/10 py-1 text-center text-[9px] font-semibold uppercase tracking-wider text-destructive"
          title={errorMessage}
        >
          !
        </div>
      ) : null}

      <div className="min-h-0 flex-1 overflow-y-auto scrollbar-thin">
        <ul className="flex flex-col items-center gap-1 px-2 py-2.5">
          {projects.map((project) => (
            <li key={project.id} className="w-full">
              <ProjectRailItem
                project={project}
                isActive={project.id === displayedActiveProjectId}
                isRemovalPending={project.id === pendingProjectRemovalId}
                isRemovalLocked={isRemovingProject}
                onPreviewProject={handlePreviewProject}
                onRemoveProject={onRemoveProject}
                onSelectProject={handleSelectProject}
              />
            </li>
          ))}
          <li className="mt-1 w-full">
            <button
              aria-label="Import repository"
              className={cn(
                'group/add relative mx-auto flex h-8 w-8 items-center justify-center rounded-lg border border-dashed border-border/70 text-muted-foreground/80 transition-all duration-150',
                'hover:border-primary/50 hover:bg-primary/10 hover:text-primary',
                'disabled:cursor-not-allowed disabled:opacity-40',
              )}
              disabled={isImporting || isRemovingProject}
              onClick={onImportProject}
              title="Import repository"
              type="button"
            >
              <Plus className="h-3.5 w-3.5 transition-transform duration-150 group-hover/add:scale-110" />
            </button>
          </li>
        </ul>
      </div>

      {isBusy ? (
        <div
          aria-label={
            isImporting ? 'Importing project' : isRemovingProject ? 'Removing project' : 'Refreshing'
          }
          className="flex items-center justify-center border-t border-border/60 py-2 text-muted-foreground/80"
          title={
            isImporting ? 'Importing…' : isRemovingProject ? 'Removing…' : 'Refreshing…'
          }
        >
          <RefreshCw className="h-3 w-3 animate-spin text-primary/80" />
        </div>
      ) : null}

      {onOpenSettings ? (
        <div className="flex items-center justify-center px-2 py-2">
          <button
            aria-label="Settings"
            className="flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground/80 transition-colors hover:bg-secondary/60 hover:text-foreground"
            onClick={onOpenSettings}
            title="Settings"
            type="button"
          >
            <Settings className="h-4 w-4" />
          </button>
        </div>
      ) : null}
    </aside>
  )
}

interface ProjectRailItemProps {
  project: ProjectListItem
  isActive: boolean
  isRemovalPending: boolean
  isRemovalLocked: boolean
  onSelectProject: (projectId: string) => void
  onPreviewProject: (projectId: string) => void
  onRemoveProject: (projectId: string) => void
}

const ProjectRailItem = memo(function ProjectRailItem({
  project,
  isActive,
  isRemovalPending,
  isRemovalLocked,
  onSelectProject,
  onPreviewProject,
  onRemoveProject,
}: ProjectRailItemProps) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const projectInitial = Array.from(project.name.trim())[0]?.toUpperCase() ?? '?'

  return (
    <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
      <div className="group relative mx-auto w-8">
        <button
          aria-label={`Open ${project.name}${isActive ? ' (active)' : ''}`}
          aria-current={isActive ? 'true' : undefined}
          className={cn(
            'relative flex h-8 w-8 items-center justify-center rounded-lg border text-[12px] font-medium leading-none transition-colors duration-150',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-foreground/30',
            isActive
              ? 'border-border/80 bg-secondary text-foreground'
              : 'border-transparent bg-secondary/30 text-foreground/55 hover:bg-secondary/60 hover:text-foreground',
          )}
          onClick={() => onSelectProject(project.id)}
          onContextMenu={(event) => {
            event.preventDefault()
            if (!isRemovalLocked) setConfirmOpen(true)
          }}
          onPointerDown={(event) => {
            if (event.button === 0) onPreviewProject(project.id)
          }}
          title={project.name}
          type="button"
        >
          {isRemovalPending ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <span aria-hidden="true">{projectInitial}</span>
          )}
          <span className="sr-only">{project.name}</span>
        </button>
      </div>

      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove {project.name} from the sidebar?</AlertDialogTitle>
          <AlertDialogDescription>
            Xero will only forget this project in the desktop registry. The repo and its app-data
            project state stay untouched. You can import the same folder again any time.
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
})
