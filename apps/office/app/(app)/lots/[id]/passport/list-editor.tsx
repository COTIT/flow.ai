"use client"

import * as React from "react"
import { ArrowDownIcon, ArrowUpIcon, Trash2Icon } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"

/** Rows of one passport list with move up/down and remove; `render` draws one row. */
export function ListEditor<T>({
  label,
  items,
  onChange,
  render,
  add,
}: {
  label: string
  items: T[]
  onChange: (items: T[]) => void
  render: (item: T, set: (item: T) => void, index: number) => React.ReactNode
  add: React.ReactNode
}) {
  const t = useTranslations("passport")
  // Stable row keys, so inputs keep their own state when rows move.
  const ids = React.useRef<number[]>([])
  const nextId = React.useRef(0)
  while (ids.current.length < items.length) ids.current.push(nextId.current++)
  ids.current.length = items.length
  const move = (from: number, to: number) => {
    const next = [...items]
    next.splice(to, 0, ...next.splice(from, 1))
    ids.current.splice(to, 0, ...ids.current.splice(from, 1))
    onChange(next)
  }
  const remove = (i: number) => {
    ids.current.splice(i, 1)
    onChange(items.filter((_, j) => j !== i))
  }
  return (
    <fieldset className="flex flex-col gap-3 rounded-lg border border-border p-3">
      <legend className="px-1 text-sm font-medium">{label}</legend>
      {items.map((item, i) => (
        <div
          key={ids.current[i]}
          role="group"
          aria-label={`${label} ${i + 1}`}
          className="flex flex-col gap-2 border-b border-border pb-3 last:border-0"
        >
          {render(
            item,
            (v) => onChange(items.map((x, j) => (j === i ? v : x))),
            i
          )}
          <div className="flex gap-1">
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={t("moveUp")}
              disabled={i === 0}
              onClick={() => move(i, i - 1)}
            >
              <ArrowUpIcon />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={t("moveDown")}
              disabled={i === items.length - 1}
              onClick={() => move(i, i + 1)}
            >
              <ArrowDownIcon />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={t("remove")}
              onClick={() => remove(i)}
            >
              <Trash2Icon />
            </Button>
          </div>
        </div>
      ))}
      <div className="flex flex-wrap gap-2">{add}</div>
    </fieldset>
  )
}
