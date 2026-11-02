"use client"

import * as React from "react"
import { Delete } from "lucide-react"
import { useTranslations } from "next-intl"

/** A digit, the bottom-left key ("clear" or "00"), or "back" (delete one). */
export type PadKey = string

/**
 * The till's one keypad (PIN, cash, card slip, drawer count): 1–9, a function key, 0, delete.
 * With `keyboard`, a hardware keyboard types too (digits, Backspace, Enter) unless a text field
 * has the focus.
 */
export function Keypad({
  fn,
  onKey,
  onEnter,
  keyboard = false,
  disabled = false,
}: {
  fn: "clear" | "00"
  onKey: (key: PadKey) => void
  onEnter?: () => void
  keyboard?: boolean
  disabled?: boolean
}) {
  const t = useTranslations("keypad")
  const down = React.useEffectEvent((e: KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return
    if ((e.target as HTMLElement).closest?.("input, textarea, select")) return
    if (/^\d$/.test(e.key)) onKey(e.key)
    else if (e.key === "Backspace") onKey("back")
    else if (e.key === "Enter" && onEnter) {
      e.preventDefault()
      onEnter()
    }
  })

  React.useEffect(() => {
    if (!keyboard || disabled) return
    window.addEventListener("keydown", down)
    return () => window.removeEventListener("keydown", down)
  }, [keyboard, disabled])

  const key = (k: PadKey, label: React.ReactNode, extra?: string) => (
    <button
      key={k}
      type="button"
      className={extra ? `key ${extra}` : "key"}
      disabled={disabled}
      aria-label={k === "back" ? t("delete") : undefined}
      onClick={() => onKey(k)}
    >
      {label}
    </button>
  )
  return (
    <div className="pad">
      {["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((d) => key(d, d))}
      {key(fn, fn === "clear" ? t("clear") : "00", "fn")}
      {key("0", "0")}
      {key("back", <Delete aria-hidden className="size-[22px]" />, "fn")}
    </div>
  )
}
