"use client"

import { MinusIcon, PlusIcon } from "lucide-react"

/**
 * Mock `.stepper`: − count + in one field-like box. 44 px tall as drawn; on phones the buttons
 * get the full 44 px tap height (the drawn 42 px inner buttons are under the minimum).
 */
export function Stepper({
  value,
  onChange,
  disabled,
  min = 1,
  max = 1000,
  labels,
}: {
  value: number
  onChange: (value: number) => void
  disabled?: boolean
  min?: number
  max?: number
  labels: { less: string; more: string; value: string }
}) {
  const button =
    "inline-flex h-11 w-11 items-center justify-center disabled:opacity-50 md:h-[42px] [&_svg]:size-[18px]"
  return (
    <span className="inline-flex h-[46px] shrink-0 items-center rounded-[12px] border border-[#0a183a]/16 bg-white/78 md:h-11 dark:border-white/18 dark:bg-white/6">
      <button
        type="button"
        className={button}
        aria-label={labels.less}
        disabled={disabled || value <= min}
        onClick={() => onChange(value - 1)}
      >
        <MinusIcon aria-hidden />
      </button>
      <span
        aria-label={labels.value}
        className="min-w-8 text-center font-semibold tabular-nums"
      >
        {value}
      </span>
      <button
        type="button"
        className={button}
        aria-label={labels.more}
        disabled={disabled || value >= max}
        onClick={() => onChange(value + 1)}
      >
        <PlusIcon aria-hidden />
      </button>
    </span>
  )
}
