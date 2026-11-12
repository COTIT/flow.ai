"use client"

import type { ReactNode } from "react"
import { useTranslations } from "next-intl"
import { ChevronDown, MoreHorizontal } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@genesy/ui/components/dropdown-menu"
import { flowStore, useFlow } from "@/lib/flow-store"

export type Column<R> = {
  key: string
  label: string
  /** Right-aligned with tabular figures. */
  numeric?: boolean
  /** The key number of the row (bold). */
  strong?: boolean
  value: (row: R) => ReactNode
}

export type RowAction = {
  key: string
  /** Its name in the row menu, where it stands alone ("Snooze 7 days"). */
  label: string
  /** Its name inside its group's menu ("For 7 days"). */
  short?: string
  onSelect: () => void
}

export type RowActions = {
  /** Shown as a button (Acknowledge). */
  inline?: RowAction
  /** Shown behind one menu button (Snooze ▾). */
  group?: { label: string; items: RowAction[] }
  /** Replaces the inline action once done (Acknowledged). */
  status?: string
  disabled?: boolean
}

/*
 * Lists board: one markup, three forms by the width of the list's container (the caller marks its
 * card `@container`, so an open conversation rail and a half-width bento tile both count):
 *   ≥ 940 px  a table, up to two actions inline
 *   520–939   the same columns, every action in one ⋯ row menu
 *   < 520     one labelled card per row, full-width actions
 * Native table elements keep explicit roles. In the card form the header row is not drawn at
 * all (never a stray wide row); each value carries its own visible label instead.
 */
const INLINE = "hidden @max-[519px]:flex @min-[940px]:flex"
const MENU = "hidden @min-[520px]:@max-[939px]:flex"

export function RecordTable<R>({
  label,
  rows,
  rowKey,
  name,
  nameLabel,
  columns,
  actions,
  select,
  itemAttr = "data-item",
}: {
  label: string
  rows: R[]
  rowKey: (row: R) => string
  name: (row: R) => string
  nameLabel: string
  columns: Column<R>[]
  actions?: (row: R) => RowActions | null
  /** Rows the person can select for the conversation ("this one"). */
  select?: string
  itemAttr?: "data-item" | "data-row"
}) {
  const t = useTranslations("scenes")
  const withActions = !!actions && rows.some((r) => actions(r))
  if (!rows.length) return null
  return (
    <table
      role="table"
      aria-label={label}
      className="w-full border-collapse text-sm"
    >
      <thead role="rowgroup" className="@max-[519px]:hidden">
        <tr role="row" className="border-b text-xs text-muted-foreground">
          <th
            role="columnheader"
            scope="col"
            className="px-3 pb-2 text-left font-medium"
          >
            {nameLabel}
          </th>
          {columns.map((c) => (
            <th
              key={c.key}
              role="columnheader"
              scope="col"
              className={`w-24 px-3 pb-2 font-medium @min-[940px]:w-30 ${c.numeric ? "text-right" : "text-left"}`}
            >
              {c.label}
            </th>
          ))}
          {withActions ? (
            <th
              role="columnheader"
              scope="col"
              className="w-px px-3 pb-2 text-right font-medium whitespace-nowrap"
            >
              <span className="@min-[520px]:@max-[939px]:sr-only">
                {t("table.actions")}
              </span>
            </th>
          ) : null}
        </tr>
      </thead>
      <tbody
        role="rowgroup"
        className="@max-[519px]:flex @max-[519px]:flex-col @max-[519px]:gap-2"
      >
        {rows.map((row) => {
          const id = rowKey(row)
          const act = actions?.(row) ?? null
          return (
            <tr
              key={id}
              role="row"
              {...{ [itemAttr]: id }}
              className="border-b last:border-b-0 @max-[519px]:grid @max-[519px]:grid-cols-3 @max-[519px]:gap-x-2 @max-[519px]:gap-y-1 @max-[519px]:rounded-2xl @max-[519px]:border-b-0 @max-[519px]:bg-card @max-[519px]:p-3 @max-[519px]:shadow-sm @max-[519px]:ring-1 @max-[519px]:ring-foreground/5"
            >
              <th
                role="rowheader"
                scope="row"
                className="px-3 py-1 text-left align-middle font-medium @max-[519px]:col-span-3 @max-[519px]:p-0"
              >
                {select ? (
                  <SelectName type={select} id={id} name={name(row)} />
                ) : (
                  <span className="block py-2.5 text-[15px] break-words">
                    {name(row)}
                  </span>
                )}
                {act?.status ? (
                  <span
                    data-acknowledged=""
                    className="hidden pb-1 text-xs font-normal text-muted-foreground @min-[520px]:@max-[939px]:block"
                  >
                    {act.status}
                  </span>
                ) : null}
              </th>
              {columns.map((c) => (
                <td
                  key={c.key}
                  role="cell"
                  className={`px-3 py-1 align-middle text-[15px] tabular-nums @max-[519px]:p-0 @max-[519px]:text-left ${c.numeric ? "text-right" : ""} ${c.strong ? "font-semibold" : ""} ${c.key === "minimum" ? "text-muted-foreground" : ""}`}
                >
                  <span className="hidden text-xs font-normal text-muted-foreground @max-[519px]:block">
                    {c.label}
                  </span>
                  {c.value(row)}
                </td>
              ))}
              {withActions ? (
                <td
                  role="cell"
                  className="px-3 py-1 align-middle whitespace-nowrap @max-[519px]:col-span-3 @max-[519px]:p-0 @max-[519px]:pt-1 @max-[519px]:whitespace-normal"
                >
                  {act ? <Actions name={name(row)} act={act} /> : null}
                </td>
              ) : null}
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}

/** The row's name; pressing it selects the record for the agent's context (ids only). */
function SelectName({
  type,
  id,
  name,
}: {
  type: string
  id: string
  name: string
}) {
  const t = useTranslations("scenes")
  const on = useFlow(
    (s) => s.selected?.type === type && s.selected.ids.includes(id)
  )
  return (
    <button
      type="button"
      aria-pressed={on}
      data-record={id}
      onClick={() =>
        flowStore.getState().setSelected(on ? null : { type, ids: [id] })
      }
      className={`-mx-2 min-h-11 rounded-xl px-2 text-left text-[15px] break-words hover:bg-muted focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none ${on ? "bg-muted ring-1 ring-(--gold)" : ""}`}
    >
      {name}
      {on ? <span className="sr-only">{t("selected")}</span> : null}
    </button>
  )
}

function Actions({ name, act }: { name: string; act: RowActions }) {
  const t = useTranslations("scenes")
  const all = [
    ...(act.inline && !act.status ? [act.inline] : []),
    ...(act.group?.items ?? []),
  ]
  return (
    <>
      <div
        className={`${INLINE} items-center justify-end gap-1 @max-[519px]:gap-2`}
      >
        {act.status ? (
          <span
            data-acknowledged=""
            className="px-3 text-[13px] text-muted-foreground @max-[519px]:flex-1 @max-[519px]:px-0"
          >
            {act.status}
          </span>
        ) : act.inline ? (
          <Button
            variant="ghost"
            className="min-h-11 rounded-full @max-[519px]:flex-1 @max-[519px]:border @max-[519px]:bg-background"
            disabled={act.disabled}
            onClick={act.inline.onSelect}
          >
            {act.inline.label}
          </Button>
        ) : null}
        {act.group ? (
          <DropdownMenu>
            <DropdownMenuTrigger
              disabled={act.disabled}
              render={
                <Button
                  variant="outline"
                  className="min-h-11 rounded-full @max-[519px]:flex-1"
                  aria-label={`${act.group.label} ${name}`}
                />
              }
            >
              {act.group.label}
              <ChevronDown aria-hidden />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-auto min-w-48">
              {act.group.items.map((a) => (
                <DropdownMenuItem
                  key={a.key}
                  className="min-h-11"
                  onClick={a.onSelect}
                >
                  {a.short ?? a.label}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
      </div>
      {all.length ? (
        <div className={`${MENU} justify-end`}>
          <DropdownMenu>
            <DropdownMenuTrigger
              disabled={act.disabled}
              render={
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-11 rounded-full"
                  aria-label={t("table.rowActions", { name })}
                />
              }
            >
              <MoreHorizontal aria-hidden />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-auto min-w-56">
              {all.map((a) => (
                <DropdownMenuItem
                  key={a.key}
                  className="min-h-11"
                  onClick={a.onSelect}
                >
                  {a.label}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ) : null}
    </>
  )
}
