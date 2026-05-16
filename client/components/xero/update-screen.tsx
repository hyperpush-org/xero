import { RefreshCw } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { cn } from '@/lib/utils'
import { AppLogo } from '@xero/ui/components/app-logo'

interface UpdateScreenProps {
  status: 'checking' | 'downloading' | 'installing' | 'error'
  percent: number
  version: string | null
  error: string | null
  onRetry: () => void
}

function statusLabel(status: UpdateScreenProps['status'], version: string | null): string {
  if (status === 'checking') {
    return 'Checking for updates'
  }
  if (status === 'downloading') {
    return version ? `Updating to ${version}` : 'Downloading update'
  }
  if (status === 'installing') {
    return 'Installing update'
  }
  return 'Update failed'
}

export function UpdateScreen({
  status,
  percent,
  version,
  error,
  onRetry,
}: UpdateScreenProps) {
  const indeterminate = status === 'checking' || (status === 'downloading' && percent <= 0)

  return (
    <div
      role="status"
      aria-live="polite"
      aria-busy={status !== 'error'}
      aria-label="Updating Xero"
      className="flex h-screen w-screen flex-col items-center justify-center bg-background px-6"
    >
      <div className="relative flex h-20 w-20 items-center justify-center">
        <span
          aria-hidden
          className="absolute inset-0 rounded-full xero-loading-ring"
          style={{
            border: '1px solid color-mix(in oklab, var(--primary) 40%, transparent)',
          }}
        />
        <AppLogo className="h-7 w-7 xero-loading-breathe" />
      </div>

      <Progress
        aria-label={statusLabel(status, version)}
        aria-valuetext={indeterminate ? undefined : `${percent}%`}
        data-indeterminate={indeterminate ? 'true' : undefined}
        className={cn(
          'xero-update-progress mt-5 h-1 w-44 bg-primary/15',
          status === 'error' && 'bg-destructive/20',
        )}
        value={status === 'error' ? 100 : percent}
      />

      <p className="mt-3 min-h-5 text-center text-[12px] font-medium text-muted-foreground">
        {statusLabel(status, version)}
      </p>

      {status === 'error' ? (
        <div className="mt-3 flex max-w-sm flex-col items-center gap-3 text-center">
          <p className="text-[12px] leading-relaxed text-muted-foreground">
            {error ?? 'Xero could not install the required update.'}
          </p>
          <Button variant="outline" size="sm" onClick={onRetry}>
            <RefreshCw className="h-3.5 w-3.5" />
            Retry update
          </Button>
        </div>
      ) : null}
    </div>
  )
}
