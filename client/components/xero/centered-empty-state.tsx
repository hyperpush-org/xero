import type { ElementType } from 'react'

import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from '@/components/ui/empty'

interface CenteredEmptyStateProps {
  icon: ElementType
  title: string
  description: string
  action?: React.ReactNode
}

export function CenteredEmptyState({ icon: Icon, title, description, action }: CenteredEmptyStateProps) {
  return (
    <div className="flex min-h-full w-full items-center justify-center">
      <Empty className="border-0">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <Icon className="size-5 text-muted-foreground" />
          </EmptyMedia>
          <EmptyTitle className="text-sm font-medium text-foreground">{title}</EmptyTitle>
          <EmptyDescription className="text-xs">{description}</EmptyDescription>
        </EmptyHeader>
        {action ? <EmptyContent>{action}</EmptyContent> : null}
      </Empty>
    </div>
  )
}
