import { cn } from '@/lib/utils'
import { AppLogo } from '@xero/ui/components/app-logo'

interface LoadingScreenProps {
  className?: string
  state?: 'open' | 'closed'
}

export function LoadingScreen({ className, state = 'open' }: LoadingScreenProps) {
  return (
    <div
      role="status"
      aria-live="polite"
      aria-busy="true"
      aria-label="Loading"
      data-state={state}
      className={cn(
        'xero-loading-screen flex flex-1 items-center justify-center bg-background',
        className,
      )}
    >
      <div className="xero-loading-symbol relative flex h-20 w-20 items-center justify-center">
        <span
          aria-hidden
          className="absolute inset-0 rounded-full xero-loading-ring"
          style={{
            border: '1px solid color-mix(in oklab, var(--primary) 40%, transparent)',
          }}
        />
        <AppLogo className="h-7 w-7 xero-loading-breathe" />
      </div>
    </div>
  )
}
