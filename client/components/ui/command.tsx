'use client'

import * as React from 'react'
import { Command as CommandPrimitive } from 'cmdk'
import { SearchIcon } from 'lucide-react'

import { cn } from '@/lib/utils'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

function Command({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive>) {
  return (
    <CommandPrimitive
      data-slot="command"
      className={cn(
        'bg-popover text-popover-foreground flex h-full w-full flex-col overflow-hidden rounded-md',
        className,
      )}
      {...props}
    />
  )
}

function CommandDialog({
  title = 'Command Palette',
  description = 'Search for a command to run...',
  children,
  className,
  footer,
  showCloseButton = false,
  ...props
}: React.ComponentProps<typeof Dialog> & {
  title?: string
  description?: string
  className?: string
  footer?: React.ReactNode
  showCloseButton?: boolean
}) {
  return (
    <Dialog {...props}>
      <DialogHeader className="sr-only">
        <DialogTitle>{title}</DialogTitle>
        <DialogDescription>{description}</DialogDescription>
      </DialogHeader>
      <DialogContent
        className={cn(
          'top-[12%] translate-y-0 gap-0 overflow-hidden rounded-xl border-border/70 bg-popover/95 p-0 shadow-2xl shadow-black/40 backdrop-blur-xl sm:max-w-2xl',
          className,
        )}
        showCloseButton={showCloseButton}
      >
        <Command
          className={cn(
            'bg-transparent',
            '**:data-[slot=command-input-wrapper]:h-14 **:data-[slot=command-input-wrapper]:gap-3 **:data-[slot=command-input-wrapper]:px-4 **:data-[slot=command-input-wrapper]:border-border/70',
            '[&_[cmdk-input]]:h-14 [&_[cmdk-input]]:text-[15px]',
            '[&_[cmdk-list]]:max-h-[min(440px,60vh)] [&_[cmdk-list]]:p-1.5',
            '[&_[cmdk-group]]:px-1 [&_[cmdk-group]]:pb-1 [&_[cmdk-group]:not([hidden])_~[cmdk-group]]:pt-0',
            '[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:pt-3 [&_[cmdk-group-heading]]:pb-1.5 [&_[cmdk-group-heading]]:text-[10.5px] [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:tracking-[0.09em] [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:text-muted-foreground/55',
            '[&_[cmdk-item]]:min-h-10 [&_[cmdk-item]]:gap-3 [&_[cmdk-item]]:rounded-lg [&_[cmdk-item]]:px-2.5 [&_[cmdk-item]]:py-2 [&_[cmdk-item][data-selected=true]]:bg-muted',
            '[&_[cmdk-item]_svg]:size-[18px]',
          )}
        >
          {children}
          {footer}
        </Command>
      </DialogContent>
    </Dialog>
  )
}

function CommandInput({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Input>) {
  return (
    <div
      data-slot="command-input-wrapper"
      className="flex h-9 items-center gap-2 border-b px-3"
    >
      <SearchIcon className="size-4 shrink-0 opacity-50" />
      <CommandPrimitive.Input
        data-slot="command-input"
        className={cn(
          'placeholder:text-muted-foreground flex h-10 w-full rounded-md bg-transparent py-3 text-sm outline-hidden disabled:cursor-not-allowed disabled:opacity-50',
          className,
        )}
        {...props}
      />
    </div>
  )
}

function CommandList({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.List>) {
  return (
    <CommandPrimitive.List
      data-slot="command-list"
      className={cn(
        'max-h-[300px] scroll-py-1 overflow-x-hidden overflow-y-auto',
        className,
      )}
      {...props}
    />
  )
}

function CommandEmpty({
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Empty>) {
  return (
    <CommandPrimitive.Empty
      data-slot="command-empty"
      className="py-10 text-center text-sm text-muted-foreground"
      {...props}
    />
  )
}

function CommandGroup({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Group>) {
  return (
    <CommandPrimitive.Group
      data-slot="command-group"
      className={cn(
        'text-foreground [&_[cmdk-group-heading]]:text-muted-foreground overflow-hidden p-1 [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium',
        className,
      )}
      {...props}
    />
  )
}

function CommandSeparator({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Separator>) {
  return (
    <CommandPrimitive.Separator
      data-slot="command-separator"
      className={cn('bg-border/70 -mx-1 my-1 h-px', className)}
      {...props}
    />
  )
}

function CommandItem({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Item>) {
  return (
    <CommandPrimitive.Item
      data-slot="command-item"
      className={cn(
        "data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground [&_svg:not([class*='text-'])]:text-muted-foreground relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
        className,
      )}
      {...props}
    />
  )
}

const SHORTCUT_MODIFIER_KEYS = new Set([
  '⌘',
  '⌃',
  '⌥',
  '⇧',
  '↵',
  '⏎',
  '⌫',
  '⎋',
  '→',
  '←',
  '↑',
  '↓',
])

function parseShortcut(value: string): string[] {
  const keys: string[] = []
  let buffer = ''
  for (const char of value) {
    if (SHORTCUT_MODIFIER_KEYS.has(char)) {
      if (buffer) {
        keys.push(buffer)
        buffer = ''
      }
      keys.push(char)
    } else {
      buffer += char
    }
  }
  if (buffer) keys.push(buffer)
  return keys
}

function Kbd({ className, ...props }: React.ComponentProps<'kbd'>) {
  return (
    <kbd
      data-slot="kbd"
      className={cn(
        'border-border/70 bg-muted/60 text-muted-foreground inline-flex h-5 min-w-5 items-center justify-center rounded border px-1 font-sans text-[11px] leading-none font-medium',
        className,
      )}
      {...props}
    />
  )
}

function CommandShortcut({
  className,
  children,
  ...props
}: React.ComponentProps<'span'>) {
  const keys = typeof children === 'string' ? parseShortcut(children) : null
  return (
    <span
      data-slot="command-shortcut"
      className={cn('ml-auto flex items-center gap-1', className)}
      {...props}
    >
      {keys
        ? keys.map((key, index) => <Kbd key={`${key}-${index}`}>{key}</Kbd>)
        : children}
    </span>
  )
}

function CommandMeta({ className, ...props }: React.ComponentProps<'span'>) {
  return (
    <span
      data-slot="command-meta"
      className={cn(
        'text-muted-foreground/70 ml-auto text-xs tabular-nums',
        className,
      )}
      {...props}
    />
  )
}

function CommandFooter({
  className,
  children,
  primaryLabel = 'Open',
  ...props
}: React.ComponentProps<'div'> & { primaryLabel?: string }) {
  return (
    <div
      data-slot="command-footer"
      className={cn(
        'border-border/70 bg-muted/20 text-muted-foreground/80 flex items-center gap-4 border-t px-4 py-2.5 text-[11px]',
        className,
      )}
      {...props}
    >
      {children ?? (
        <>
          <span className="flex items-center gap-1.5">
            <Kbd>↵</Kbd>
            {primaryLabel}
          </span>
          <span className="flex items-center gap-1.5">
            <Kbd>↑</Kbd>
            <Kbd>↓</Kbd>
            Navigate
          </span>
          <span className="flex items-center gap-1.5">
            <Kbd>esc</Kbd>
            Close
          </span>
        </>
      )}
    </div>
  )
}

export {
  Command,
  CommandDialog,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandShortcut,
  CommandMeta,
  CommandFooter,
  CommandSeparator,
  Kbd,
}
