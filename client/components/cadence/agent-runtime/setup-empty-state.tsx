import { Bot } from 'lucide-react'

import { CenteredEmptyState } from '@/components/cadence/centered-empty-state'
import { Button } from '@/components/ui/button'

interface SetupEmptyStateProps {
  onOpenSettings?: () => void
}

export function SetupEmptyState({ onOpenSettings }: SetupEmptyStateProps) {
  return (
    <CenteredEmptyState
      description="Open Settings to choose a provider and model before using the agent tab for this imported project."
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
