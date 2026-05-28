import { useCallback, useEffect, useMemo, useState } from 'react'
import { Monitor, MousePointer2, Square } from 'lucide-react'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import type { DesktopControlStatusDto } from '@/src/lib/xero-model/desktop-control'

export type DesktopControlBannerAdapter = Pick<
  XeroDesktopAdapter,
  'isDesktopRuntime' | 'desktopControlStatus' | 'desktopControlStop'
>

interface DesktopControlBannerProps {
  adapter?: DesktopControlBannerAdapter | null
  onOpenSettings?: () => void
}

const ACTIVE_STREAM_STATES = new Set(['starting', 'live', 'degraded', 'paused'])

export function DesktopControlBanner({ adapter, onOpenSettings }: DesktopControlBannerProps) {
  const canUseAdapter = Boolean(
    adapter?.isDesktopRuntime?.() && adapter.desktopControlStatus && adapter.desktopControlStop,
  )
  const [status, setStatus] = useState<DesktopControlStatusDto | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [stopping, setStopping] = useState(false)

  const activeState = useMemo(() => getActiveDesktopControlState(status), [status])

  useEffect(() => {
    if (!canUseAdapter || !adapter?.desktopControlStatus) {
      setStatus(null)
      setError(null)
      return
    }

    let disposed = false
    let timer: number | null = null

    const refresh = async () => {
      try {
        const nextStatus = await adapter.desktopControlStatus?.()
        if (disposed || !nextStatus) return
        setStatus(nextStatus)
        setError(null)
      } catch (caught) {
        if (disposed) return
        setError(caught instanceof Error ? caught.message : 'Desktop-control status failed.')
      } finally {
        if (disposed) return
        timer = window.setTimeout(refresh, 2500)
      }
    }

    void refresh()

    return () => {
      disposed = true
      if (timer) {
        window.clearTimeout(timer)
      }
    }
  }, [adapter, canUseAdapter])

  const stopControl = useCallback(async () => {
    if (!adapter?.desktopControlStop) return
    setStopping(true)
    setError(null)
    try {
      setStatus(await adapter.desktopControlStop())
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Desktop-control stop failed.')
    } finally {
      setStopping(false)
    }
  }, [adapter])

  if (!canUseAdapter || !activeState) {
    return null
  }

  const Icon = activeState.kind === 'manual_control' ? MousePointer2 : Monitor

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-4 z-[80] flex justify-center px-4">
      <Alert
        className={cn(
          'pointer-events-auto flex w-full max-w-[760px] items-center gap-3 rounded-md border px-3.5 py-3 shadow-xl shadow-background/40',
          activeState.kind === 'manual_control'
            ? 'border-destructive/45 bg-destructive/10 text-foreground'
            : 'border-border bg-popover text-popover-foreground',
        )}
      >
        <Icon className="h-4 w-4 shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <AlertTitle className="text-[13px] font-semibold">{activeState.title}</AlertTitle>
            <Badge variant={activeState.kind === 'manual_control' ? 'destructive' : 'secondary'}>
              {activeState.badge}
            </Badge>
          </div>
          {error ? (
            <AlertDescription className="mt-0.5 truncate text-[12px] leading-[1.45] text-muted-foreground">
              {error}
            </AlertDescription>
          ) : null}
        </div>
        {onOpenSettings ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="hidden h-8 shrink-0 text-[12px] sm:inline-flex"
            onClick={onOpenSettings}
          >
            Settings
          </Button>
        ) : null}
        <Button
          type="button"
          size="sm"
          variant="destructive"
          className="h-8 shrink-0 gap-1.5 text-[12px]"
          disabled={stopping}
          onClick={() => void stopControl()}
        >
          <Square className="h-3.5 w-3.5" />
          Stop
        </Button>
      </Alert>
    </div>
  )
}

function getActiveDesktopControlState(status: DesktopControlStatusDto | null):
  | {
      kind: 'manual_control' | 'agent_control' | 'stream'
      title: string
      badge: string
    }
  | null {
  if (!status) {
    return null
  }

  if (status.controllerLock?.actor === 'cloud_manual_control') {
    return {
      kind: 'manual_control',
      title: 'Remote desktop control active',
      badge: 'manual control',
    }
  }

  if (status.controllerLock?.actor === 'agent') {
    return {
      kind: 'agent_control',
      title: 'Computer Use desktop control active',
      badge: 'agent',
    }
  }

  if (
    ACTIVE_STREAM_STATES.has(status.stream.status) &&
    status.stream.transport !== 'unavailable'
  ) {
    return {
      kind: 'stream',
      title: 'Desktop stream active',
      badge: status.stream.transport.replace(/_/g, ' '),
    }
  }

  return null
}
