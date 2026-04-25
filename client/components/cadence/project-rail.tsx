import { useState } from 'react'
import { Loader2, MoreHorizontal, Plus, RefreshCw, Trash2 } from 'lucide-react'

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
  collapsed?: boolean
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
  collapsed = false,
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
  const isBusy = isLoading || isImporting || isRemovingProject

  return (
    <aside
      className={cn(
        'motion-layout-island flex shrink-0 flex-col overflow-hidden border-r border-border/80 bg-sidebar transition-[width] motion-panel',
        collapsed ? 'w-11' : 'w-56',
      )}
      data-collapsed={collapsed ? 'true' : 'false'}
    >
      <div
        className={cn(
          'flex h-10 items-center border-b border-border/70 transition-[padding] motion-panel',
          collapsed ? 'justify-center px-1' : 'justify-between px-3',
        )}
      >
        <div
          className={cn(
            'flex items-center gap-1.5 overflow-hidden transition-[max-width,opacity] motion-standard',
            collapsed ? 'max-w-0 opacity-0' : 'max-w-[10rem] opacity-100',
          )}
        >
          <span className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            Projects
          </span>
          {projects.length > 0 ? (
            <span className="rounded-full bg-muted/80 px-1.5 py-[1px] font-mono text-[10px] leading-none tabular-nums text-muted-foreground">
              {projects.length}
            </span>
          ) : null}
        </div>
        <button
          aria-label="Import repository"
          className={cn(
            'flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors',
            'hover:bg-primary/10 hover:text-primary disabled:cursor-not-allowed disabled:opacity-50',
          )}
          disabled={isImporting || isRemovingProject}
          onClick={onImportProject}
          type="button"
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
      </div>

      {errorMessage ? (
        <div
          className={cn(
            'border-b border-border/70 bg-destructive/5 text-[11px] leading-snug text-destructive transition-[padding,opacity,max-height] motion-standard',
            collapsed ? 'max-h-0 px-0 py-0 opacity-0' : 'max-h-20 px-3 py-2 opacity-100',
          )}
        >
          {errorMessage}
        </div>
      ) : null}

      <div
        className={cn(
          'flex-1 overflow-y-auto scrollbar-thin',
          collapsed ? 'py-2' : '',
        )}
      >
        {projects.length === 0 ? (
          <div
            aria-hidden={collapsed ? true : undefined}
            className={cn(
              'px-3 py-5 text-center text-[11px] leading-relaxed text-muted-foreground/80 transition-[max-height,opacity] motion-standard',
              collapsed ? 'max-h-0 opacity-0' : 'max-h-24 opacity-100',
            )}
          >
            No projects imported yet.
          </div>
        ) : (
          <ul className={cn('flex flex-col', collapsed ? 'gap-1.5 px-1.5' : '')}>
            {projects.map((project) => (
              <li key={project.id}>
                <ProjectRailItem
                  collapsed={collapsed}
                  project={project}
                  isActive={project.id === activeProjectId}
                  isRemovalPending={project.id === pendingProjectRemovalId}
                  isRemovalLocked={isRemovingProject}
                  onRemoveProject={onRemoveProject}
                  onSelectProject={onSelectProject}
                />
              </li>
            ))}
          </ul>
        )}
      </div>

      {isBusy && (
        <div
          className={cn(
            'flex items-center border-t border-border/70 bg-sidebar text-[11px] text-muted-foreground transition-[padding,gap] motion-panel',
            collapsed ? 'justify-center gap-0 px-1.5 py-2.5' : 'gap-2 px-3 py-2.5',
          )}
        >
          <RefreshCw className="h-3 w-3 animate-spin text-primary/80" />
          <span
            className={cn(
              'overflow-hidden whitespace-nowrap transition-[max-width,opacity] motion-standard',
              collapsed ? 'max-w-0 opacity-0' : 'max-w-24 opacity-100',
            )}
          >
            {isImporting ? 'Importing…' : isRemovingProject ? 'Removing…' : 'Refreshing…'}
          </span>
        </div>
      )}
    </aside>
  )
}

interface ProjectRailItemProps {
  project: ProjectListItem
  collapsed: boolean
  isActive: boolean
  isRemovalPending: boolean
  isRemovalLocked: boolean
  onSelectProject: (projectId: string) => void
  onRemoveProject: (projectId: string) => void
}

function ProjectRailItem({
  project,
  collapsed,
  isActive,
  isRemovalPending,
  isRemovalLocked,
  onSelectProject,
  onRemoveProject,
}: ProjectRailItemProps) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const projectInitial = Array.from(project.name.trim())[0]?.toUpperCase() ?? '?'

  return (
    <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
      <div className="group relative">
        <button
          className={cn(
            'relative w-full transition-colors duration-150',
            collapsed
              ? cn(
                  'flex items-center justify-center rounded-md p-1',
                  isActive ? 'bg-primary/10' : 'hover:bg-secondary/60',
                )
              : cn(
                  'flex items-center gap-2.5 px-3 py-3 text-left',
                  isActive ? 'bg-primary/[0.08]' : 'hover:bg-secondary/50',
                ),
          )}
          onClick={() => onSelectProject(project.id)}
          title={collapsed ? project.name : undefined}
          type="button"
        >
          <div
            className={cn(
              'flex h-7 w-7 shrink-0 items-center justify-center rounded-md border text-[12px] font-semibold leading-none transition-colors duration-150',
              isActive
                ? 'border-primary/45 bg-primary/15 text-primary'
                : 'border-border/70 bg-secondary/70 text-muted-foreground group-hover:border-border group-hover:bg-secondary group-hover:text-foreground',
            )}
          >
            <span aria-hidden="true">{projectInitial}</span>
            {collapsed ? <span className="sr-only">{project.name}</span> : null}
          </div>

          <div
            aria-hidden={collapsed ? true : undefined}
            className={cn(
              'min-w-0 flex-1 overflow-hidden transition-[max-width,opacity] motion-standard',
              collapsed ? 'max-w-0 opacity-0' : 'max-w-[10.5rem] opacity-100',
            )}
          >
            <div className="flex items-center pr-6">
              <span
                className={cn(
                  'truncate text-[12.5px] font-medium leading-tight',
                  isActive ? 'text-foreground' : 'text-foreground/85 group-hover:text-foreground',
                )}
              >
                {project.name}
              </span>
            </div>
            <div className="mt-1.5 flex items-center gap-1.5">
              <div className="h-[3px] flex-1 overflow-hidden rounded-full bg-border/70">
                <div
                  className={cn(
                    'h-full rounded-full motion-progress',
                    isActive ? 'bg-primary' : 'bg-primary/55',
                  )}
                  style={{
                    transform: `scaleX(${Math.max(0, Math.min(100, project.phaseProgressPercent)) / 100})`,
                  }}
                />
              </div>
              <span
                className={cn(
                  'font-mono text-[10px] leading-none tabular-nums',
                  isActive ? 'text-foreground/80' : 'text-muted-foreground',
                )}
              >
                {project.phaseProgressPercent}%
              </span>
            </div>
          </div>
        </button>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              aria-hidden={collapsed ? true : undefined}
              aria-label={`Project actions for ${project.name}`}
              className={cn(
                'absolute right-1 top-1 z-10 flex h-5 w-5 items-center justify-center rounded-md text-muted-foreground transition-[opacity,color,background-color] motion-fast',
                'hover:bg-secondary hover:text-foreground disabled:opacity-50',
                collapsed
                  ? 'pointer-events-none opacity-0'
                  : isActive || isRemovalPending
                    ? 'opacity-100'
                    : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100',
              )}
              disabled={collapsed || isRemovalLocked}
              tabIndex={collapsed ? -1 : undefined}
              type="button"
            >
              {isRemovalPending ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <MoreHorizontal className="h-3.5 w-3.5" />
              )}
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
