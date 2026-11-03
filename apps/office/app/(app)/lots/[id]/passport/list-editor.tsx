"use client"

import * as React from "react"
import { ChevronDownIcon, ChevronUpIcon, Trash2Icon } from "lucide-react"
import { useTranslations } from "next-intl"

import { Badge } from "@genesy/ui/components/badge"
import { Button } from "@genesy/ui/components/button"
import { SubTile, Tile, type Tone } from "@/components/bento"

/**
 * One passport list as a card (OfficePassport "Story blocks"): each row is a tile with a kind
 * badge, a one-line summary, move up/down and remove, then its fields; `render` draws the fields.
 */
export function ListEditor<T>({
  id,
  title,
  label,
  items,
  onChange,
  kind,
  summary,
  render,
  add,
}: {
  id: string
  title: string
  /** Rows are groups named "{label} {n}". */
  label: string
  items: T[]
  onChange: (items: T[]) => void
  kind: (item: T, index: number) => { label: string; tone: Tone }
  summary: (item: T) => string
  render: (item: T, set: (item: T) => void, index: number) => React.ReactNode
  add: React.ReactNode
}) {
  const t = useTranslations("passport")
  // Stable row keys, so inputs keep their own state when rows move.
  const [ids, setIds] = React.useState<number[]>([])
  if (ids.length !== items.length) {
    // The parent added rows: keep the keys we have, mint new ones past the highest.
    let nextId = ids.reduce((max, id) => Math.max(max, id + 1), 0)
    const fitted = ids.slice(0, items.length)
    while (fitted.length < items.length) fitted.push(nextId++)
    setIds(fitted)
  }
  const move = (from: number, to: number) => {
    const next = [...items]
    next.splice(to, 0, ...next.splice(from, 1))
    const keys = [...ids]
    keys.splice(to, 0, ...keys.splice(from, 1))
    setIds(keys)
    onChange(next)
  }
  const remove = (i: number) => {
    setIds(ids.filter((_, j) => j !== i))
    onChange(items.filter((_, j) => j !== i))
  }
  return (
    <Tile
      id={id}
      title={title}
      span=""
      action={<div className="flex flex-wrap gap-2">{add}</div>}
    >
      {items.map((item, i) => {
        const k = kind(item, i)
        return (
          <SubTile key={ids[i]}>
            <div role="group" aria-label={`${label} ${i + 1}`} className="flex flex-col gap-3">
              <div className="flex items-center gap-3">
                <Badge variant={k.tone} data-dot="false">
                  {k.label}
                </Badge>
                <p className="min-w-0 grow truncate text-sm">{summary(item)}</p>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t("moveUp")}
                  disabled={i === 0}
                  onClick={() => move(i, i - 1)}
                >
                  <ChevronUpIcon />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t("moveDown")}
                  disabled={i === items.length - 1}
                  onClick={() => move(i, i + 1)}
                >
                  <ChevronDownIcon />
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
              {render(
                item,
                (v) => onChange(items.map((x, j) => (j === i ? v : x))),
                i
              )}
            </div>
          </SubTile>
        )
      })}
    </Tile>
  )
}
