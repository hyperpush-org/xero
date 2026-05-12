"use client"

import { EmulatorSidebar } from "./emulator-sidebar"

interface IosEmulatorSidebarProps {
  open: boolean
  openImmediately?: boolean
}

export function IosEmulatorSidebar({ open, openImmediately = false }: IosEmulatorSidebarProps) {
  return <EmulatorSidebar open={open} openImmediately={openImmediately} platform="ios" />
}
