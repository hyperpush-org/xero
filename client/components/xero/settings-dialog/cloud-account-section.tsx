import { invoke, isTauri } from "@tauri-apps/api/core"
import { Cloud, Globe, Laptop, Loader2, Trash2 } from "lucide-react"
import { useCallback, useEffect, useState } from "react"

import { Button } from "@xero/ui/components/ui/button"
import { SectionHeader } from "./section-header"
import { SubHeading } from "./_shared"

export interface AccountDevice {
  id: string
  kind: "desktop" | "web"
  name: string | null
  lastSeen: string | null
  revokedAt: string | null
  userAgent: string | null
}

interface BridgeAccountInfo {
  githubLogin?: string | null
  avatarUrl?: string | null
}

interface BridgeStatusResponse {
  signedIn: boolean
  account?: BridgeAccountInfo | null
  devices?: AccountDevice[]
  devicesError?: string | null
}

export function CloudAccountSection() {
  const [signedIn, setSignedIn] = useState(false)
  const [account, setAccount] = useState<BridgeAccountInfo | null>(null)
  const [devices, setDevices] = useState<AccountDevice[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [revoking, setRevoking] = useState<string | null>(null)
  const [unlinkConfirmationDeviceId, setUnlinkConfirmationDeviceId] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    if (!isTauri()) {
      setLoading(false)
      return
    }
    setLoading(true)
    setError(null)
    try {
      const response = await invoke<BridgeStatusResponse>("bridge_status")
      setSignedIn(Boolean(response.signedIn))
      setAccount(response.account ?? null)
      setDevices(
        (response.devices ?? []).filter(
          (device) => device.kind === "web" && !device.revokedAt,
        ),
      )
      setError(response.devicesError?.trim() || null)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const handleRevoke = async (deviceId: string) => {
    setRevoking(deviceId)
    setUnlinkConfirmationDeviceId(null)
    setError(null)
    try {
      await invoke("bridge_revoke_device", { request: { deviceId } })
      await refresh()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setRevoking(null)
    }
  }

  const clearUnlinkConfirmation = useCallback(() => {
    setUnlinkConfirmationDeviceId(null)
  }, [])

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Cloud account"
        description="Manage cloud-app browser connections linked to your GitHub account. Linked browsers can drive non-archived sessions from signed-in desktops."
      />

      <div className="flex flex-col gap-3">
        <SubHeading>Linked devices</SubHeading>
        {loading ? (
          <p className="flex items-center gap-2 text-[12px] text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" /> Loading devices…
          </p>
        ) : !signedIn ? (
          <p className="text-[12px] text-muted-foreground">
            Sign in with GitHub from the Account section to see linked devices.
          </p>
        ) : devices.length === 0 ? (
          <p className="text-[12px] text-muted-foreground">
            No active cloud-app browser connections.
          </p>
        ) : (
          <ul className="flex flex-col divide-y divide-border/60 rounded-md border border-border/60 bg-secondary/10">
            {devices.map((device) => {
              const deviceLabel = device.name ?? (device.kind === "desktop" ? "Desktop" : "Browser")
              const isRevoking = revoking === device.id
              const isUnlinkConfirming = unlinkConfirmationDeviceId === device.id && !isRevoking
              const unlinkActionLabel = isUnlinkConfirming
                ? `Confirm unlink ${deviceLabel}`
                : `Unlink ${deviceLabel}`
              const unlinkActionTitle = isUnlinkConfirming
                ? `Press again to unlink ${deviceLabel}`
                : `Unlink ${deviceLabel}`

              return (
                <li
                  key={device.id}
                  className="flex items-center justify-between gap-3 px-3.5 py-3"
                >
                  <div className="flex items-center gap-3 min-w-0">
                    {device.kind === "desktop" ? (
                      <Laptop className="h-4 w-4 shrink-0 text-muted-foreground" />
                    ) : (
                      <Globe className="h-4 w-4 shrink-0 text-muted-foreground" />
                    )}
                    <div className="min-w-0">
                      <p className="truncate text-[12.5px] font-medium text-foreground">
                        {deviceLabel}
                      </p>
                      <p className="truncate text-[11.5px] text-muted-foreground">
                        {device.kind === "desktop" ? "Desktop" : "Browser"}
                        {device.lastSeen ? ` · last seen ${formatRelative(device.lastSeen)}` : ""}
                      </p>
                    </div>
                  </div>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className={[
                      "h-7 p-0 text-muted-foreground hover:bg-destructive/10 hover:text-destructive",
                      isUnlinkConfirming
                        ? "w-auto min-w-[60px] px-2 text-[11.5px] font-semibold text-destructive hover:bg-destructive/15"
                        : "w-7",
                    ].join(" ")}
                    onClick={() => {
                      if (isUnlinkConfirming) {
                        void handleRevoke(device.id)
                        return
                      }
                      setUnlinkConfirmationDeviceId(device.id)
                    }}
                    onBlur={(event) => {
                      const nextFocused = event.relatedTarget
                      if (nextFocused instanceof Node && event.currentTarget.contains(nextFocused)) {
                        return
                      }
                      clearUnlinkConfirmation()
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") {
                        event.stopPropagation()
                        clearUnlinkConfirmation()
                      }
                    }}
                    onPointerLeave={clearUnlinkConfirmation}
                    disabled={isRevoking}
                    aria-label={unlinkActionLabel}
                    title={unlinkActionTitle}
                  >
                    {isRevoking ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : isUnlinkConfirming ? (
                      <span>Unlink</span>
                    ) : (
                      <Trash2 className="h-3.5 w-3.5" />
                    )}
                  </Button>
                </li>
              )
            })}
          </ul>
        )}
        {account?.githubLogin ? (
          <p className="text-[11.5px] text-muted-foreground">
            Showing devices for <span className="text-foreground">@{account.githubLogin}</span>.
          </p>
        ) : null}
        {error ? (
          <p className="flex items-center gap-2 text-[12px] text-destructive" role="alert">
            <Cloud className="h-3.5 w-3.5" /> {error}
          </p>
        ) : null}
      </div>
    </div>
  )
}

function formatRelative(timestamp: string): string {
  const date = Date.parse(timestamp)
  if (Number.isNaN(date)) return timestamp
  const seconds = Math.max(0, Math.round((Date.now() - date) / 1000))
  if (seconds < 60) return "just now"
  if (seconds < 3600) return `${Math.round(seconds / 60)}m ago`
  if (seconds < 86_400) return `${Math.round(seconds / 3600)}h ago`
  return `${Math.round(seconds / 86_400)}d ago`
}
