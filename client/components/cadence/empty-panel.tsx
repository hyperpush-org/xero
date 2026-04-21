import { cn } from '@/lib/utils'

interface EmptyPanelProps {
  eyebrow: string
  title: string
  body: string
  action?: React.ReactNode
  className?: string
}

export function EmptyPanel({ eyebrow, title, body, action, className }: EmptyPanelProps) {
  return (
    <div className={cn('flex flex-1 items-center justify-center bg-background p-6', className)}>
      <div className="max-w-md rounded-xl border border-border bg-card px-6 py-8 text-center shadow-sm">
        <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">{eyebrow}</p>
        <h1 className="mt-3 text-2xl font-semibold text-foreground">{title}</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">{body}</p>
        {action ? <div className="mt-5">{action}</div> : null}
      </div>
    </div>
  )
}
