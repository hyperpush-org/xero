import { Bot, Plus, Workflow as WorkflowIcon } from 'lucide-react'

import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

interface WorkflowCanvasEmptyStateProps {
  onCreateWorkflow?: () => void
  onCreateAgent?: () => void
  className?: string
}

export function WorkflowCanvasEmptyState({
  onCreateWorkflow,
  onCreateAgent,
  className,
}: WorkflowCanvasEmptyStateProps) {
  return (
    <div
      className={cn(
        'pointer-events-none absolute inset-0 z-[5] flex items-center justify-center px-6',
        className,
      )}
    >
      <div
        className="pointer-events-auto"
        onPointerDown={(event) => event.stopPropagation()}
        onWheel={(event) => event.stopPropagation()}
      >
        <Empty className="border-0">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <WorkflowIcon className="size-5 text-muted-foreground" />
            </EmptyMedia>
            <EmptyTitle className="text-sm font-medium text-foreground">
              Start with a workflow
            </EmptyTitle>
            <EmptyDescription className="text-xs">
              Compose agents into a workflow on the canvas, or define a new
              agent to use as a building block.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <div className="flex flex-wrap items-center justify-center gap-2">
              <Button
                type="button"
                size="sm"
                onClick={onCreateWorkflow}
                className="gap-1.5"
              >
                <Plus className="size-3.5" />
                Create workflow
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={onCreateAgent}
                className="gap-1.5"
              >
                <Bot className="size-3.5" />
                Create agent
              </Button>
            </div>
          </EmptyContent>
        </Empty>
      </div>
    </div>
  )
}
