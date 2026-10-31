import { Select as SelectPrimitive } from '@base-ui/react/select'
import { Check, ChevronDown } from 'lucide-react'
import type { ComponentPropsWithoutRef, ReactNode } from 'react'

import { cn } from '@/lib/utils'

export const Select = SelectPrimitive.Root

export function SelectTrigger({
  className,
  children,
  ...props
}: ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger>) {
  return (
    <SelectPrimitive.Trigger
      className={cn(
        'inline-flex min-h-11 w-full items-center justify-between gap-2 rounded-[var(--radius-control)] border-[1.5px] border-rule-2 bg-paper px-3 text-left text-sm text-ink outline-none focus-visible:ring-2 focus-visible:ring-primary disabled:pointer-events-none disabled:opacity-60',
        className,
      )}
      {...props}
    >
      {children}
      <ChevronDown aria-hidden size={16} strokeWidth={2} className="shrink-0 text-ink-2" />
    </SelectPrimitive.Trigger>
  )
}

export const SelectValue = SelectPrimitive.Value

export function SelectContent({
  className,
  children,
  ...props
}: ComponentPropsWithoutRef<typeof SelectPrimitive.Popup> & { children?: ReactNode }) {
  return (
    <SelectPrimitive.Portal>
      <SelectPrimitive.Positioner sideOffset={4} className="z-40">
        <SelectPrimitive.Popup
          className={cn(
            'max-h-72 min-w-[var(--anchor-width)] overflow-auto rounded-[var(--radius-control)] border border-rule-2 bg-paper p-1 text-sm text-ink',
            className,
          )}
          {...props}
        >
          <SelectPrimitive.List>{children}</SelectPrimitive.List>
        </SelectPrimitive.Popup>
      </SelectPrimitive.Positioner>
    </SelectPrimitive.Portal>
  )
}

export function SelectItem({
  className,
  children,
  ...props
}: ComponentPropsWithoutRef<typeof SelectPrimitive.Item> & { children?: ReactNode }) {
  return (
    <SelectPrimitive.Item
      className={cn(
        'relative flex min-h-10 cursor-default items-center rounded-[6px] px-8 py-2 outline-none data-[highlighted]:bg-primary-tint data-[selected]:font-semibold',
        className,
      )}
      data-value={props.value === undefined ? undefined : String(props.value)}
      {...props}
    >
      <span className="absolute left-2 inline-flex w-4 items-center justify-center">
        <SelectPrimitive.ItemIndicator>
          <Check aria-hidden size={14} strokeWidth={2.5} />
        </SelectPrimitive.ItemIndicator>
      </span>
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
    </SelectPrimitive.Item>
  )
}
