import { Bot, FolderPlus, Loader2 } from 'lucide-react'

import { CenteredEmptyState } from '@/components/xero/centered-empty-state'
import { Button } from '@/components/ui/button'

export type SetupEmptyStateKind = 'no-provider' | 'no-project'

interface SetupEmptyStateProps {
  kind?: SetupEmptyStateKind
  onOpenSettings?: () => void
  onImportProject?: () => void
  isImportingProject?: boolean
  isDesktopRuntime?: boolean
}

export function SetupEmptyState({
  kind = 'no-provider',
  onOpenSettings,
  onImportProject,
  isImportingProject = false,
  isDesktopRuntime = true,
}: SetupEmptyStateProps) {
  if (kind === 'no-project') {
    return (
      <CenteredEmptyState
        description={
          isDesktopRuntime
            ? 'Import a local Git repository to start a session and let the agent work alongside you.'
            : 'Project import is only available inside the Xero desktop runtime.'
        }
        icon={FolderPlus}
        title="Add a project to begin"
        action={
          onImportProject && isDesktopRuntime ? (
            <div className="flex flex-wrap items-center justify-center gap-2">
              <Button disabled={isImportingProject} onClick={onImportProject} type="button">
                {isImportingProject ? (
                  <>
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    Importing…
                  </>
                ) : (
                  <>
                    <FolderPlus className="h-3.5 w-3.5" />
                    Import repository
                  </>
                )}
              </Button>
            </div>
          ) : undefined
        }
      />
    )
  }

  return (
    <CenteredEmptyState
      description="Connect a provider in Settings to start chatting with the agent."
      icon={Bot}
      title="Configure agent runtime"
      action={
        onOpenSettings ? (
          <div className="flex flex-wrap items-center justify-center gap-2">
            <Button onClick={onOpenSettings} type="button">
              Configure
            </Button>
          </div>
        ) : undefined
      }
    />
  )
}
