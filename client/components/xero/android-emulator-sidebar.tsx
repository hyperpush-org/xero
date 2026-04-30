"use client"

import { EmulatorSidebar } from "./emulator-sidebar"

interface AndroidEmulatorSidebarProps {
  open: boolean
}

export function AndroidEmulatorSidebar({ open }: AndroidEmulatorSidebarProps) {
  return <EmulatorSidebar open={open} platform="android" />
}
