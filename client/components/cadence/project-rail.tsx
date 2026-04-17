import { Folder, Plus, RefreshCw } from 'lucide-react'
import type { ProjectListItem, RepositoryStatusView } from '@/src/lib/cadence-model'

interface ProjectRailProps {
  projects: ProjectListItem[]
  activeProjectId: string | null
  repositoryStatus: RepositoryStatusView | null
  activeProjectBranch: string | null
  isLoading: boolean
  isImporting: boolean
  errorMessage: string | null
  refreshSource: string | null
  onSelectProject: (projectId: string) => void
  onImportProject: () => void
}

export function ProjectRail({
  projects,
  activeProjectId,
  repositoryStatus,
  activeProjectBranch,
  isLoading,
  isImporting,
  errorMessage,
  refreshSource,
  onSelectProject,
  onImportProject,
}: ProjectRailProps) {
  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-border bg-sidebar">
      <div className="flex h-10 items-center justify-between border-b border-border px-3">
        <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">Projects</span>
        <button
          aria-label="Import repository"
          className="rounded p-1 text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground disabled:opacity-50"
          disabled={isImporting}
          onClick={onImportProject}
          type="button"
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
      </div>

      {errorMessage ? (
        <div className="border-b border-border px-3 py-2 text-[11px] text-destructive">{errorMessage}</div>
      ) : null}

      <div className="flex-1 overflow-y-auto scrollbar-thin py-1">
        {projects.length === 0 ? (
          <div className="px-3 py-4 text-[12px] text-muted-foreground">
            No projects imported yet.
          </div>
        ) : (
          projects.map((project) => {
            const isActive = project.id === activeProjectId

            return (
              <button
                key={project.id}
                className={`w-full px-3 py-2.5 text-left transition-colors ${isActive ? 'bg-secondary' : 'hover:bg-secondary/40'}`}
                onClick={() => onSelectProject(project.id)}
                type="button"
              >
                <div className="mb-1 flex items-center gap-2">
                  <Folder
                    className={`h-3.5 w-3.5 shrink-0 ${isActive ? 'text-primary' : 'text-muted-foreground'}`}
                  />
                  <span
                    className={`truncate text-[13px] font-medium ${isActive ? 'text-foreground' : 'text-foreground/80'}`}
                  >
                    {project.name}
                  </span>
                </div>

                <p className="mb-2 ml-[22px] truncate text-[11px] text-muted-foreground">{project.milestone || 'No milestone'}</p>

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
            )
          })
        )}
      </div>

      {(isLoading || isImporting) && (
        <div className="flex items-center gap-2 border-t border-border px-3 py-2.5 text-[11px] text-muted-foreground">
          <RefreshCw className="h-3 w-3 animate-spin" />
          <span>{isImporting ? 'Importing…' : 'Refreshing…'}</span>
        </div>
      )}
    </aside>
  )
}
