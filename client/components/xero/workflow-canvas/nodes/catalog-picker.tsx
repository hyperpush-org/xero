'use client'

import { useState, type ReactNode } from 'react'
import { Check, ChevronsUpDown } from 'lucide-react'

import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'

export interface CatalogPickerOption<TValue extends string = string> {
  value: TValue
  label: string
  description?: string
  meta?: ReactNode
  group?: string
  // Free-text search bag — extra strings (tags, ids, aliases) the user might
  // type to find this entry. cmdk searches against label by default; this
  // augments it.
  keywords?: readonly string[]
}

interface CatalogPickerProps<TValue extends string = string> {
  options: readonly CatalogPickerOption<TValue>[]
  value: TValue | null
  onChange: (value: TValue) => void
  placeholder?: string
  searchPlaceholder?: string
  emptyMessage?: string
  loading?: boolean
  loadingMessage?: string
  triggerClassName?: string
  disabled?: boolean
  // Optional renderer for the trigger label area when a value is selected.
  // Defaults to showing the option's label. Useful when callers want richer
  // chrome (e.g. an icon + label + small meta).
  renderSelected?: (option: CatalogPickerOption<TValue>) => ReactNode
}

export function CatalogPicker<TValue extends string = string>({
  options,
  value,
  onChange,
  placeholder = 'Choose…',
  searchPlaceholder = 'Search…',
  emptyMessage = 'No matches.',
  loading = false,
  loadingMessage = 'Loading catalog…',
  triggerClassName,
  disabled = false,
  renderSelected,
}: CatalogPickerProps<TValue>) {
  const [open, setOpen] = useState(false)
  const selected = value ? options.find((option) => option.value === value) ?? null : null

  // Group options for visual scanning when the catalog provides a `group`
  // hint (tools have groups like "core", "harness_runner"; tables don't).
  const grouped = new Map<string, CatalogPickerOption<TValue>[]>()
  for (const option of options) {
    const key = option.group ?? ''
    const list = grouped.get(key) ?? []
    list.push(option)
    grouped.set(key, list)
  }
  const groupKeys = Array.from(grouped.keys()).sort((a, b) => a.localeCompare(b))

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        type="button"
        disabled={disabled || loading}
        onPointerDown={(event) => event.stopPropagation()}
        className={cn(
          'nodrag nopan flex w-full items-center gap-2 rounded-md border border-border/70 bg-background/80 px-2 py-1.5 text-left text-[10.5px] hover:bg-muted/40 disabled:opacity-50',
          triggerClassName,
        )}
      >
        <span className="flex-1 min-w-0 truncate">
          {selected
            ? renderSelected
              ? renderSelected(selected)
              : selected.label
            : loading
              ? loadingMessage
              : placeholder}
        </span>
        <ChevronsUpDown className="h-3 w-3 shrink-0 text-muted-foreground/60" aria-hidden="true" />
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-[304px] p-0"
        onPointerDown={(event) => event.stopPropagation()}
      >
        <Command>
          <CommandInput placeholder={searchPlaceholder} className="h-9" />
          <CommandList className="max-h-[280px]">
            <CommandEmpty>{loading ? loadingMessage : emptyMessage}</CommandEmpty>
            {groupKeys.map((key) => (
              <CommandGroup key={key || 'default'} heading={key || undefined}>
                {(grouped.get(key) ?? []).map((option) => {
                  const isSelected = option.value === value
                  // cmdk fuzzy-matches against this string; we concatenate label
                  // + description + keywords so users can search by any.
                  const valueText = [
                    option.label,
                    option.description ?? '',
                    ...(option.keywords ?? []),
                  ]
                    .join(' ')
                    .toLowerCase()
                  return (
                    <CommandItem
                      key={option.value}
                      value={`${option.value} ${valueText}`}
                      onSelect={() => {
                        onChange(option.value)
                        setOpen(false)
                      }}
                      className="flex flex-col items-start gap-0.5 px-2 py-1.5"
                    >
                      <div className="flex w-full items-center gap-2">
                        <Check
                          className={cn(
                            'h-3 w-3 shrink-0',
                            isSelected ? 'text-primary' : 'text-transparent',
                          )}
                          aria-hidden="true"
                        />
                        <span className="truncate text-[11px] font-medium">{option.label}</span>
                        {option.meta ? (
                          <span className="ml-auto text-[9.5px] text-muted-foreground">
                            {option.meta}
                          </span>
                        ) : null}
                      </div>
                      {option.description ? (
                        <span className="ml-5 text-[9.5px] text-muted-foreground/85 leading-snug line-clamp-2">
                          {option.description}
                        </span>
                      ) : null}
                    </CommandItem>
                  )
                })}
              </CommandGroup>
            ))}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}
