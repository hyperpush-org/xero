import { useCallback, useEffect, useState } from "react"
import {
  AlertTriangle,
  Monitor,
  MousePointer2,
  RefreshCw,
  ShieldCheck,
  Square,
} from "lucide-react"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import type { XeroDesktopAdapter } from "@/src/lib/xero-desktop"
import type {
  DesktopControlStatusDto,
  UpsertDesktopControlSettingsRequestDto,
} from "@/src/lib/xero-model/desktop-control"
import { SectionHeader } from "./section-header"

const OWNER_ADMIN_DURATION_MINUTES = 30

export type DesktopControlSettingsAdapter = Pick<
  XeroDesktopAdapter,
  | "isDesktopRuntime"
  | "desktopControlStatus"
  | "desktopControlUpdateSettings"
  | "desktopControlStop"
  | "desktopControlOpenPermissionSettings"
>

type DesktopPermissionAction = NonNullable<DesktopControlStatusDto["permissions"][number]["action"]>

interface DesktopControlSectionProps {
  adapter?: DesktopControlSettingsAdapter | null
}

export function DesktopControlSection({ adapter }: DesktopControlSectionProps) {
  const canUseAdapter = Boolean(
    adapter?.isDesktopRuntime?.() &&
      adapter.desktopControlStatus &&
      adapter.desktopControlUpdateSettings &&
      adapter.desktopControlStop,
  )
  const [status, setStatus] = useState<DesktopControlStatusDto | null>(null)
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState<keyof UpsertDesktopControlSettingsRequestDto | null>(null)
  const [stopping, setStopping] = useState(false)
  const [openingPermission, setOpeningPermission] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(
    async ({ refreshPermissionStatus = false }: { refreshPermissionStatus?: boolean } = {}) => {
      if (!canUseAdapter || !adapter?.desktopControlStatus) {
        setStatus(null)
        setLoading(false)
        setError(null)
        return
      }
      setLoading(true)
      setError(null)
      try {
        setStatus(await adapter.desktopControlStatus({ refreshPermissionStatus }))
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : "Desktop-control status failed.")
      } finally {
        setLoading(false)
      }
    },
    [adapter, canUseAdapter],
  )

  useEffect(() => {
    void refresh({ refreshPermissionStatus: true })
  }, [refresh])

  const saveSettings = async (
    request: UpsertDesktopControlSettingsRequestDto,
    savingKey: keyof UpsertDesktopControlSettingsRequestDto,
  ) => {
    if (!adapter?.desktopControlUpdateSettings || !status) return
    const previous = status
    setSaving(savingKey)
    setError(null)
    setStatus({ ...status, settings: { ...status.settings, ...request } })
    try {
      setStatus(await adapter.desktopControlUpdateSettings(request))
    } catch (caught) {
      setStatus(previous)
      setError(caught instanceof Error ? caught.message : "Desktop-control settings failed.")
    } finally {
      setSaving(null)
    }
  }

  const currentSettingsRequest = (
    patch: Partial<UpsertDesktopControlSettingsRequestDto>,
  ): UpsertDesktopControlSettingsRequestDto => ({
    cloudStreamingEnabled: status?.settings.cloudStreamingEnabled ?? false,
    manualCloudControlEnabled: status?.settings.manualCloudControlEnabled ?? false,
    policyProfile: status?.settings.policyProfile ?? "default_safe",
    ...patch,
  })

  const updateSetting = <K extends keyof UpsertDesktopControlSettingsRequestDto>(
    key: K,
    value: UpsertDesktopControlSettingsRequestDto[K],
  ) => {
    void saveSettings(
      currentSettingsRequest({ [key]: value } as Partial<UpsertDesktopControlSettingsRequestDto>),
      key,
    )
  }

  const stopControl = async () => {
    if (!adapter?.desktopControlStop || !status) return
    setStopping(true)
    setError(null)
    try {
      setStatus(await adapter.desktopControlStop())
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Desktop-control stop failed.")
    } finally {
      setStopping(false)
    }
  }

  const openPermissionSettings = async (action: DesktopPermissionAction) => {
    if (!adapter?.desktopControlOpenPermissionSettings) return
    const actionId = permissionActionId(action)
    setOpeningPermission(actionId)
    setError(null)
    try {
      await adapter.desktopControlOpenPermissionSettings({
        kind: action.kind,
        target: action.target,
      })
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Desktop permission settings failed.")
    } finally {
      setOpeningPermission(null)
    }
  }

  const selectedStatus = canUseAdapter ? status : fallbackStatus()
  const isStatusLoading = canUseAdapter && status === null && error === null
  const activeLock = selectedStatus?.controllerLock ?? null
  const stream = selectedStatus?.stream ?? null
  const ownerAdminActive = selectedStatus ? isOwnerAdminActive(selectedStatus) : false

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Desktop Control"
        description="Local permissions and safety switches for Computer Use desktop viewing and input."
      />

      {!canUseAdapter ? (
        <Alert className="rounded-md px-3.5 py-2.5 text-[13px]">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle className="text-[13px] font-semibold">Desktop runtime unavailable</AlertTitle>
          <AlertDescription className="text-[12.5px] leading-[1.5]">
            Desktop control can only be configured from the Xero desktop app.
          </AlertDescription>
        </Alert>
      ) : null}

      {error ? (
        <Alert variant="destructive" className="rounded-md px-3.5 py-2.5 text-[13px]">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle className="text-[13px] font-semibold">Desktop control failed</AlertTitle>
          <AlertDescription className="text-[12.5px] leading-[1.5]">{error}</AlertDescription>
        </Alert>
      ) : null}

      {isStatusLoading ? (
        <DesktopControlLoadingState />
      ) : selectedStatus ? (
        <>
          <section className="flex flex-col gap-3">
            <div className="flex items-center justify-between gap-3">
              <h4 className="text-[12.5px] font-semibold text-foreground">Status</h4>
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="h-8 gap-1.5 text-[12px]"
                disabled={!canUseAdapter || loading || (status === null && error === null)}
                onClick={() => void refresh({ refreshPermissionStatus: true })}
              >
                <RefreshCw className={loading ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"} />
                Refresh
              </Button>
            </div>

            <div className="grid gap-2 sm:grid-cols-3">
              <StatusTile icon={ShieldCheck} label="Broker" value={selectedStatus.sidecar.health} />
              <StatusTile
                icon={Monitor}
                label="Stream"
                value={
                  stream
                    ? `${stream.status.replace(/_/g, " ")} · ${stream.transport.replace(/_/g, " ")}`
                    : "unknown"
                }
              />
              <StatusTile
                icon={MousePointer2}
                label="Controller"
                value={activeLock ? activeLock.actor.replace(/_/g, " ") : "idle"}
              />
            </div>
          </section>

          <section className="flex flex-col gap-3">
            <h4 className="text-[12.5px] font-semibold text-foreground">Policy profile</h4>
            <div className="overflow-hidden rounded-md border border-border/60">
              <div className="flex flex-col gap-3 px-4 py-3">
                <div className="grid gap-2 sm:grid-cols-3">
                  <ProfileButton
                    label="Default"
                    active={selectedStatus.settings.policyProfile === "default_safe"}
                    disabled={!canUseAdapter || !status || saving !== null}
                    onClick={() => updateSetting("policyProfile", "default_safe")}
                  />
                  <ProfileButton
                    label="Developer"
                    active={selectedStatus.settings.policyProfile === "developer_workstation"}
                    disabled={!canUseAdapter || !status || saving !== null}
                    onClick={() => updateSetting("policyProfile", "developer_workstation")}
                  />
                  <ProfileButton
                    label="Owner Admin"
                    active={ownerAdminActive}
                    disabled={!canUseAdapter || !status || saving !== null}
                    onClick={() =>
                      void saveSettings(
                        currentSettingsRequest({
                          policyProfile: ownerAdminActive ? "default_safe" : "owner_admin",
                          ownerAdminDurationMinutes: ownerAdminActive
                            ? undefined
                            : OWNER_ADMIN_DURATION_MINUTES,
                        }),
                        "policyProfile",
                      )
                    }
                  />
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant={ownerAdminActive ? "destructive" : "outline"}>
                    {formatPolicyProfile(selectedStatus.settings.policyProfile)}
                  </Badge>
                  {ownerAdminActive && selectedStatus.settings.ownerAdminExpiresAt ? (
                    <span className="text-[11.5px] leading-[1.45] text-muted-foreground">
                      Expires {formatDateTime(selectedStatus.settings.ownerAdminExpiresAt)}
                    </span>
                  ) : null}
                </div>
              </div>
            </div>
          </section>

          <section className="flex flex-col gap-3">
            <h4 className="text-[12.5px] font-semibold text-foreground">Cloud access</h4>
            <div className="overflow-hidden rounded-md border border-border/60">
              <SettingRow
                icon={Monitor}
                title="Allow cloud viewing"
                body="Let paired cloud sessions request a live desktop stream. When WebRTC is unavailable, Xero uses the documented screenshot fallback."
                checked={selectedStatus.settings.cloudStreamingEnabled}
                disabled={!canUseAdapter || !status || saving !== null}
                busy={saving === "cloudStreamingEnabled"}
                onChange={(value) => updateSetting("cloudStreamingEnabled", value)}
              />
              <SettingRow
                icon={MousePointer2}
                title="Allow cloud manual control"
                body="Let a paired cloud session request the exclusive desktop controller lock and send brokered mouse or keyboard input."
                checked={selectedStatus.settings.manualCloudControlEnabled}
                disabled={!canUseAdapter || !status || saving !== null}
                busy={saving === "manualCloudControlEnabled"}
                onChange={(value) => updateSetting("manualCloudControlEnabled", value)}
              />
            </div>
          </section>

          <section className="flex flex-col gap-3">
            <h4 className="text-[12.5px] font-semibold text-foreground">Permissions</h4>
            <ul className="overflow-hidden rounded-md border border-border/60">
              {selectedStatus.permissions.map((permission) => (
                <li
                  key={permission.name}
                  className="flex flex-col gap-2 border-b border-border/50 px-4 py-3 last:border-b-0 sm:flex-row sm:items-start sm:justify-between"
                >
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <p className="text-[12.5px] font-medium text-foreground">
                        {permission.name}
                      </p>
                      <Badge variant={permissionBadgeVariant(permission.status)}>
                        {permission.status.replace(/_/g, " ")}
                      </Badge>
                    </div>
                    <p className="mt-0.5 text-[12px] leading-[1.55] text-muted-foreground">
                      {permission.remediation}
                    </p>
                    {permission.requiredFor.length > 0 ? (
                      <p className="mt-1 text-[11.5px] leading-[1.45] text-muted-foreground">
                        Required for{" "}
                        {permission.requiredFor.map(formatPermissionPurpose).join(", ")}.
                      </p>
                    ) : null}
                    {permission.action ? (
                      <p className="mt-1 text-[11.5px] leading-[1.45] text-muted-foreground">
                        {permission.action.postActionHint}
                      </p>
                    ) : null}
                  </div>
                  {permission.action ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      className="h-8 shrink-0 gap-1.5 text-[12px]"
                      disabled={
                        !canUseAdapter ||
                        !status ||
                        !adapter?.desktopControlOpenPermissionSettings ||
                        openingPermission !== null
                      }
                      onClick={() => void openPermissionSettings(permission.action!)}
                      aria-label={permission.action.label}
                    >
                      {openingPermission === permissionActionId(permission.action) ? (
                        <RefreshCw className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <ShieldCheck className="h-3.5 w-3.5" />
                      )}
                      {permission.action.label}
                    </Button>
                  ) : null}
                </li>
              ))}
            </ul>
          </section>

          <section className="flex flex-col gap-3">
            <h4 className="text-[12.5px] font-semibold text-foreground">Emergency stop</h4>
            <div className="flex flex-col gap-3 rounded-md border border-border/60 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
              <p className="text-[12px] leading-[1.55] text-muted-foreground">
                Release the desktop controller lock, stop the active desktop stream, and revoke Owner Admin mode.
              </p>
              <Button
                type="button"
                size="sm"
                variant="destructive"
                className="h-8 shrink-0 gap-1.5 text-[12px]"
                disabled={!canUseAdapter || !status || stopping}
                onClick={() => void stopControl()}
              >
                <Square className="h-3.5 w-3.5" />
                Stop
              </Button>
            </div>
          </section>
        </>
      ) : (
        <DesktopControlUnavailableState
          loading={loading}
          onRefresh={() => void refresh({ refreshPermissionStatus: true })}
        />
      )}
    </div>
  )
}

function permissionBadgeVariant(status: DesktopControlStatusDto["permissions"][number]["status"]) {
  if (status === "granted") return "secondary"
  if (status === "denied") return "destructive"
  return "outline"
}

function formatPermissionPurpose(value: string): string {
  return value.replace(/_/g, " ")
}

function formatPolicyProfile(value: DesktopControlStatusDto["settings"]["policyProfile"]): string {
  return value.replace(/_/g, " ")
}

function isOwnerAdminActive(status: DesktopControlStatusDto): boolean {
  if (status.settings.policyProfile !== "owner_admin" || !status.settings.ownerAdminExpiresAt) {
    return false
  }
  const expiresAt = Date.parse(status.settings.ownerAdminExpiresAt)
  return Number.isFinite(expiresAt) && expiresAt > Date.now()
}

function formatDateTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date)
}

function permissionActionId(action: DesktopPermissionAction): string {
  return `${action.kind}:${action.target}`
}

function DesktopControlLoadingState() {
  return (
    <div
      aria-busy="true"
      aria-label="Loading desktop-control status"
      className="flex flex-col gap-7"
      role="status"
    >
      <section className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-3">
          <h4 className="text-[12.5px] font-semibold text-foreground">Status</h4>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-8 gap-1.5 text-[12px]"
            disabled
          >
            <RefreshCw className="h-3.5 w-3.5 animate-spin" />
            Refresh
          </Button>
        </div>
        <div className="grid gap-2 sm:grid-cols-3">
          <StatusTileSkeleton label="Broker" />
          <StatusTileSkeleton label="Stream" />
          <StatusTileSkeleton label="Controller" />
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Policy profile</h4>
        <div className="overflow-hidden rounded-md border border-border/60 px-4 py-3">
          <div className="grid gap-2 sm:grid-cols-3">
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
          </div>
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Cloud access</h4>
        <div className="overflow-hidden rounded-md border border-border/60">
          <SettingRowSkeleton />
          <SettingRowSkeleton />
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Permissions</h4>
        <div className="overflow-hidden rounded-md border border-border/60">
          <PermissionRowSkeleton />
          <PermissionRowSkeleton />
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Emergency stop</h4>
        <div className="flex flex-col gap-3 rounded-md border border-border/60 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <Skeleton className="h-4 w-full max-w-[420px]" />
          <Skeleton className="h-8 w-20 shrink-0" />
        </div>
      </section>
    </div>
  )
}

function DesktopControlUnavailableState({
  loading,
  onRefresh,
}: {
  loading: boolean
  onRefresh: () => void
}) {
  return (
    <>
      <section className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-3">
          <h4 className="text-[12.5px] font-semibold text-foreground">Status</h4>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-8 gap-1.5 text-[12px]"
            disabled={loading}
            onClick={onRefresh}
          >
            <RefreshCw className={loading ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"} />
            Refresh
          </Button>
        </div>
        <UnavailablePanel message="Desktop-control status is unavailable. Refresh to try again." />
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Policy profile</h4>
        <UnavailablePanel message="Policy settings are unavailable until status loads." />
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Cloud access</h4>
        <UnavailablePanel message="Cloud access settings are unavailable until status loads." />
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Permissions</h4>
        <UnavailablePanel message="Permission status is unavailable until status loads." />
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Emergency stop</h4>
        <div className="flex flex-col gap-3 rounded-md border border-border/60 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <p className="text-[12px] leading-[1.55] text-muted-foreground">
            Release the desktop controller lock, stop the active desktop stream, and revoke Owner Admin mode.
          </p>
          <Button
            type="button"
            size="sm"
            variant="destructive"
            className="h-8 shrink-0 gap-1.5 text-[12px]"
            disabled
          >
            <Square className="h-3.5 w-3.5" />
            Stop
          </Button>
        </div>
      </section>
    </>
  )
}

function UnavailablePanel({ message }: { message: string }) {
  return (
    <div className="rounded-md border border-border/60 px-4 py-3 text-[12px] leading-[1.55] text-muted-foreground">
      {message}
    </div>
  )
}

function StatusTileSkeleton({ label }: { label: string }) {
  return (
    <div className="min-w-0 rounded-md border border-border/60 px-3 py-2">
      <div className="flex items-center gap-1.5 text-[11.5px] font-medium text-muted-foreground">
        <Skeleton className="h-3.5 w-3.5" />
        {label}
      </div>
      <Skeleton className="mt-2 h-3.5 w-24" />
    </div>
  )
}

function ProfileButton({
  label,
  active,
  disabled,
  onClick,
}: {
  label: string
  active: boolean
  disabled: boolean
  onClick: () => void
}) {
  return (
    <Button
      type="button"
      size="sm"
      variant={active ? "secondary" : "outline"}
      className="h-8 justify-center text-[12px]"
      disabled={disabled}
      onClick={onClick}
      aria-pressed={active}
    >
      {label}
    </Button>
  )
}

function SettingRowSkeleton() {
  return (
    <div className="flex items-start gap-3 border-b border-border/50 px-4 py-3 last:border-b-0">
      <Skeleton className="mt-0.5 size-7 shrink-0" />
      <div className="min-w-0 flex-1 space-y-2">
        <Skeleton className="h-3.5 w-40" />
        <Skeleton className="h-3 w-full" />
        <Skeleton className="h-3 w-2/3" />
      </div>
      <Skeleton className="h-5 w-9 shrink-0 rounded-full" />
    </div>
  )
}

function PermissionRowSkeleton() {
  return (
    <div className="flex flex-col gap-2 border-b border-border/50 px-4 py-3 last:border-b-0 sm:flex-row sm:items-start sm:justify-between">
      <div className="min-w-0 flex-1 space-y-2">
        <div className="flex items-center gap-2">
          <Skeleton className="h-3.5 w-36" />
          <Skeleton className="h-5 w-16 rounded-full" />
        </div>
        <Skeleton className="h-3 w-full" />
        <Skeleton className="h-3 w-1/2" />
      </div>
      <Skeleton className="h-8 w-32 shrink-0" />
    </div>
  )
}

function SettingRow({
  icon: Icon,
  title,
  body,
  checked,
  disabled,
  busy,
  onChange,
}: {
  icon: React.ElementType
  title: string
  body: string
  checked: boolean
  disabled: boolean
  busy: boolean
  onChange: (value: boolean) => void
}) {
  return (
    <div className="flex items-start gap-3 border-b border-border/50 px-4 py-3 last:border-b-0">
      <div className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 text-muted-foreground">
        <Icon className="h-3.5 w-3.5" />
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] font-medium text-foreground">{title}</p>
        <p className="mt-0.5 text-[12px] leading-[1.55] text-muted-foreground">{body}</p>
      </div>
      <Switch
        checked={checked}
        disabled={disabled || busy}
        onCheckedChange={onChange}
        aria-label={title}
      />
    </div>
  )
}

function StatusTile({
  icon: Icon,
  label,
  value,
}: {
  icon: React.ElementType
  label: string
  value: string
}) {
  return (
    <div className="min-w-0 rounded-md border border-border/60 px-3 py-2">
      <div className="flex items-center gap-1.5 text-[11.5px] font-medium text-muted-foreground">
        <Icon className="h-3.5 w-3.5" />
        {label}
      </div>
      <p className="mt-1 truncate text-[12.5px] font-semibold text-foreground">{value}</p>
    </div>
  )
}

function fallbackStatus(): DesktopControlStatusDto {
  return {
    schema: "xero.desktop_control_status.v1",
    platform: "unknown",
    sidecar: {
      schemaVersion: 1,
      platform: "unknown",
      transport: "unavailable",
      authenticated: false,
      health: "unavailable",
      message: "Desktop runtime unavailable.",
    },
    capabilities: {
      platform: "unknown",
      schemaVersion: 1,
      displayList: false,
      screenshot: false,
      windowList: false,
      appList: false,
      notificationObservation: false,
      foregroundState: false,
      cursorState: false,
      accessibilitySnapshot: false,
      ocrSnapshot: false,
      mouseInput: false,
      keyboardInput: false,
      clipboard: false,
      windowFocus: false,
      appControl: false,
      accessibilityActions: false,
      menuSelect: false,
      webrtcStream: false,
      screenshotFallbackStream: false,
      nativeVideoTrack: false,
      preferredCodec: null,
      captureBackends: [],
      encoderBackends: [],
      hardwareEncoding: false,
      manualCloudControl: false,
    },
    permissions: [],
    controllerLock: null,
    stream: {
      streamId: null,
      status: "idle",
      transport: "unavailable",
      signalingChannel: null,
      quality: "balanced",
      maxWidth: 1280,
      maxFrameRate: 2,
      includeCursor: true,
      message: "Desktop stream is idle.",
    },
    settings: {
      cloudStreamingEnabled: false,
      manualCloudControlEnabled: false,
      policyProfile: "default_safe",
      ownerAdminExpiresAt: null,
      updatedAt: null,
    },
    auditLogPath: "",
    updatedAt: "",
  }
}
