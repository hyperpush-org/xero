"use client"

import {
  ArrowLeft,
  ChevronsLeftRight,
  Home,
  Power,
  Square,
  Volume2,
  VolumeX,
} from "lucide-react"
import { cn } from "@/lib/utils"
import type { EmulatorPlatform } from "@/src/features/emulator/use-emulator-session"

interface EmulatorHardwareStripProps {
  platform: EmulatorPlatform
  disabled: boolean
  onPressKey: (key: string) => void
}

interface ButtonSpec {
  key: string
  label: string
  icon: React.ComponentType<{ className?: string }>
}

const ANDROID_BUTTONS: ButtonSpec[] = [
  { key: "back", label: "Back", icon: ArrowLeft },
  { key: "home", label: "Home", icon: Home },
  { key: "recents", label: "Recents", icon: Square },
  { key: "vol_up", label: "Volume up", icon: Volume2 },
  { key: "vol_down", label: "Volume down", icon: VolumeX },
  { key: "power", label: "Power / lock", icon: Power },
]

const IOS_BUTTONS: ButtonSpec[] = [
  { key: "home", label: "Home", icon: Home },
  { key: "vol_up", label: "Volume up", icon: Volume2 },
  { key: "vol_down", label: "Volume down", icon: VolumeX },
  { key: "lock", label: "Side button / lock", icon: Power },
]

export function EmulatorHardwareStrip({
  platform,
  disabled,
  onPressKey,
}: EmulatorHardwareStripProps) {
  const buttons = platform === "android" ? ANDROID_BUTTONS : IOS_BUTTONS

  return (
    <div
      aria-label="Hardware controls"
      className="flex h-8 shrink-0 items-center gap-1 border-t border-border/70 bg-sidebar/40 px-2"
      role="toolbar"
    >
      {buttons.map(({ key, label, icon: Icon }) => (
        <button
          aria-label={label}
          className={cn(
            "flex h-6 w-8 items-center justify-center rounded-md border border-border/60 bg-background/40 text-muted-foreground transition-colors",
            "hover:border-primary/50 hover:text-primary disabled:cursor-not-allowed disabled:opacity-40",
          )}
          disabled={disabled}
          key={key}
          onClick={() => onPressKey(key)}
          title={label}
          type="button"
        >
          <Icon className="h-3 w-3" />
        </button>
      ))}
      <div aria-hidden="true" className="ml-auto flex items-center text-muted-foreground/60">
        <ChevronsLeftRight className="h-3 w-3" />
      </div>
    </div>
  )
}
