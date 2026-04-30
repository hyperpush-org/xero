import { cn } from '@/lib/utils'

interface LoadingScreenProps {
  className?: string
}

export function LoadingScreen({ className }: LoadingScreenProps) {
  return (
    <div
      role="status"
      aria-live="polite"
      aria-busy="true"
      aria-label="Loading"
      className={cn(
        'flex flex-1 items-center justify-center bg-background',
        className,
      )}
    >
      <div className="relative flex h-20 w-20 items-center justify-center">
        <span
          aria-hidden
          className="absolute inset-0 rounded-full xero-loading-ring"
          style={{
            border: '1px solid color-mix(in oklab, var(--primary) 40%, transparent)',
          }}
        />
        <img
          src="/icon-logo.svg"
          alt=""
          draggable={false}
          className="h-7 w-7 xero-loading-breathe"
        />
      </div>
    </div>
  )
}
