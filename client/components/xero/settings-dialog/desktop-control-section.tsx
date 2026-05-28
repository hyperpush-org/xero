import { useCallback, useEffect, useState } from "react"
import {
  AlertTriangle,
  Monitor,
  MousePointer2,
  Plus,
  RefreshCw,
  ShieldCheck,
  Square,
  Trash2,
} from "lucide-react"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import type { XeroDesktopAdapter } from "@/src/lib/xero-desktop"
import type {
  DesktopControlStatusDto,
  UpsertDesktopControlSettingsRequestDto,
} from "@/src/lib/xero-model/desktop-control"
import { SectionHeader } from "./section-header"

export type DesktopControlSettingsAdapter = Pick<
  XeroDesktopAdapter,
  | "isDesktopRuntime"
  | "desktopControlStatus"
  | "desktopControlUpdateSettings"
  | "desktopControlStop"
  | "desktopControlOpenPermissionSettings"
>

type DesktopPrivateRegion = DesktopControlStatusDto["settings"]["privateRegions"][number]
type DesktopRedactionMode = DesktopControlStatusDto["settings"]["redactionMode"]
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
    void refresh()
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
    redactionMode: status?.settings.redactionMode ?? "balanced",
    privateRegions: status?.settings.privateRegions ?? [],
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
    if (!adapter?.desktopControlStop) return
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

  const selectedStatus = status ?? fallbackStatus()
  const activeLock = selectedStatus.controllerLock
  const stream = selectedStatus.stream

  const updatePrivateRegion = (index: number, patch: Partial<DesktopPrivateRegion>) => {
    const regions = [...selectedStatus.settings.privateRegions]
    const current = regions[index]
    if (!current) return
    const next = { ...current, ...patch }
    if (!privateRegionIsValid(next)) return
    regions[index] = next
    updateSetting("privateRegions", regions)
  }

  const addPrivateRegion = () => {
    updateSetting("privateRegions", [
      ...selectedStatus.settings.privateRegions,
      { x: 0, y: 0, width: 320, height: 180 },
    ])
  }

  const removePrivateRegion = (index: number) => {
    updateSetting(
      "privateRegions",
      selectedStatus.settings.privateRegions.filter((_, candidate) => candidate !== index),
    )
  }

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

      <section className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-3">
          <h4 className="text-[12.5px] font-semibold text-foreground">Status</h4>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-8 gap-1.5 text-[12px]"
            disabled={!canUseAdapter || loading}
            onClick={() => void refresh({ refreshPermissionStatus: true })}
          >
            <RefreshCw className="h-3.5 w-3.5" />
            Refresh
          </Button>
        </div>

        <div className="grid gap-2 sm:grid-cols-3">
          <StatusTile icon={ShieldCheck} label="Broker" value={selectedStatus.sidecar.health} />
          <StatusTile
            icon={Monitor}
            label="Stream"
            value={`${stream.status.replace(/_/g, " ")} · ${stream.transport.replace(/_/g, " ")}`}
          />
          <StatusTile
            icon={MousePointer2}
            label="Controller"
            value={activeLock ? activeLock.actor.replace(/_/g, " ") : "idle"}
          />
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
            disabled={!canUseAdapter || saving !== null}
            busy={saving === "cloudStreamingEnabled"}
            onChange={(value) => updateSetting("cloudStreamingEnabled", value)}
          />
          <SettingRow
            icon={MousePointer2}
            title="Allow cloud manual control"
            body="Let a paired cloud session request the exclusive desktop controller lock and send brokered mouse or keyboard input."
            checked={selectedStatus.settings.manualCloudControlEnabled}
            disabled={!canUseAdapter || saving !== null}
            busy={saving === "manualCloudControlEnabled"}
            onChange={(value) => updateSetting("manualCloudControlEnabled", value)}
          />
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
          <div className="min-w-0">
            <h4 className="text-[12.5px] font-semibold text-foreground">Redaction</h4>
            <p className="mt-0.5 text-[12px] leading-[1.55] text-muted-foreground">
              User-marked regions are always blacked out before fallback frames leave the desktop.
            </p>
          </div>
          <label className="sr-only" htmlFor="desktop-redaction-mode">
            Redaction mode
          </label>
          <select
            id="desktop-redaction-mode"
            value={selectedStatus.settings.redactionMode}
            disabled={!canUseAdapter || saving !== null}
            onChange={(event) =>
              updateSetting("redactionMode", event.currentTarget.value as DesktopRedactionMode)
            }
            className="h-8 rounded-md border border-input bg-background px-2 text-[12px] text-foreground disabled:cursor-not-allowed disabled:opacity-50"
          >
            <option value="off">Manual regions only</option>
            <option value="balanced">Balanced</option>
            <option value="auto">Auto</option>
            <option value="strict">Strict</option>
          </select>
        </div>
        <div className="overflow-hidden rounded-md border border-border/60">
          {selectedStatus.settings.privateRegions.length === 0 ? (
            <p className="px-4 py-3 text-[12px] leading-[1.55] text-muted-foreground">
              No private regions configured.
            </p>
          ) : (
            selectedStatus.settings.privateRegions.map((region, index) => (
              <PrivateRegionRow
                key={`${region.x}:${region.y}:${region.width}:${region.height}:${index}`}
                index={index}
                region={region}
                disabled={!canUseAdapter || saving !== null}
                onChange={(patch) => updatePrivateRegion(index, patch)}
                onRemove={() => removePrivateRegion(index)}
              />
            ))
          )}
          <div className="border-t border-border/50 px-4 py-3">
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-8 gap-1.5 text-[12px]"
              disabled={
                !canUseAdapter ||
                saving !== null ||
                selectedStatus.settings.privateRegions.length >= 16
              }
              onClick={addPrivateRegion}
            >
              <Plus className="h-3.5 w-3.5" />
              Add Region
            </Button>
          </div>
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
                  <p className="text-[12.5px] font-medium text-foreground">{permission.name}</p>
                  <Badge variant={permissionBadgeVariant(permission.status)}>
                    {permission.status.replace(/_/g, " ")}
                  </Badge>
                </div>
                <p className="mt-0.5 text-[12px] leading-[1.55] text-muted-foreground">
                  {permission.remediation}
                </p>
                {permission.requiredFor.length > 0 ? (
                  <p className="mt-1 text-[11.5px] leading-[1.45] text-muted-foreground">
                    Required for {permission.requiredFor.map(formatPermissionPurpose).join(", ")}.
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
            Release the desktop controller lock and stop the active desktop stream.
          </p>
          <Button
            type="button"
            size="sm"
            variant="destructive"
            className="h-8 shrink-0 gap-1.5 text-[12px]"
            disabled={!canUseAdapter || stopping}
            onClick={() => void stopControl()}
          >
            <Square className="h-3.5 w-3.5" />
            Stop
          </Button>
        </div>
      </section>
    </div>
  )
}

function PrivateRegionRow({
  index,
  region,
  disabled,
  onChange,
  onRemove,
}: {
  index: number
  region: DesktopPrivateRegion
  disabled: boolean
  onChange: (patch: Partial<DesktopPrivateRegion>) => void
  onRemove: () => void
}) {
  return (
    <div className="grid gap-2 border-b border-border/50 px-4 py-3 last:border-b-0 sm:grid-cols-[auto_repeat(4,minmax(0,1fr))_auto] sm:items-end">
      <div className="text-[12px] font-medium text-muted-foreground">#{index + 1}</div>
      <RegionNumberField
        label="X"
        value={region.x}
        disabled={disabled}
        onChange={(x) => onChange({ x })}
      />
      <RegionNumberField
        label="Y"
        value={region.y}
        disabled={disabled}
        onChange={(y) => onChange({ y })}
      />
      <RegionNumberField
        label="Width"
        value={region.width}
        disabled={disabled}
        min={1}
        onChange={(width) => onChange({ width })}
      />
      <RegionNumberField
        label="Height"
        value={region.height}
        disabled={disabled}
        min={1}
        onChange={(height) => onChange({ height })}
      />
      <Button
        type="button"
        size="icon"
        variant="ghost"
        className="h-8 w-8 text-muted-foreground"
        disabled={disabled}
        onClick={onRemove}
        aria-label={`Remove private region ${index + 1}`}
      >
        <Trash2 className="h-3.5 w-3.5" />
      </Button>
    </div>
  )
}

function RegionNumberField({
  label,
  value,
  disabled,
  min = 0,
  onChange,
}: {
  label: string
  value: number
  disabled: boolean
  min?: number
  onChange: (value: number) => void
}) {
  return (
    <label className="flex min-w-0 flex-col gap-1 text-[11px] font-medium text-muted-foreground">
      {label}
      <input
        type="number"
        min={min}
        step={1}
        value={value}
        disabled={disabled}
        onChange={(event) => {
          const next = Number(event.currentTarget.value)
          if (!Number.isInteger(next) || next < min) return
          onChange(next)
        }}
        className="h-8 min-w-0 rounded-md border border-input bg-background px-2 text-[12px] text-foreground disabled:cursor-not-allowed disabled:opacity-50"
      />
    </label>
  )
}

function privateRegionIsValid(region: DesktopPrivateRegion): boolean {
  return (
    Number.isInteger(region.x) &&
    Number.isInteger(region.y) &&
    Number.isInteger(region.width) &&
    Number.isInteger(region.height) &&
    region.x >= 0 &&
    region.y >= 0 &&
    region.width > 0 &&
    region.height > 0
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

function permissionActionId(action: DesktopPermissionAction): string {
  return `${action.kind}:${action.target}`
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
      foregroundState: false,
      cursorState: false,
      accessibilitySnapshot: false,
      ocrSnapshot: false,
      mouseInput: false,
      keyboardInput: false,
      clipboard: false,
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
      redactionMode: "balanced",
      privateRegions: [],
      updatedAt: null,
    },
    auditLogPath: "",
    updatedAt: "",
  }
}
