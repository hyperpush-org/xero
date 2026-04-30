"use client"

import { EmulatorSidebar } from "./emulator-sidebar"

interface IosEmulatorSidebarProps {
  open: boolean
}

export function IosEmulatorSidebar({ open }: IosEmulatorSidebarProps) {
  return <EmulatorSidebar open={open} platform="ios" />
}
