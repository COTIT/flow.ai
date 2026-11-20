"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { ChevronDown, Loader2 } from "lucide-react"

import { Button } from "@verzo/ui/components/button"
import { Input } from "@verzo/ui/components/input"
import { cn } from "@verzo/ui/lib/utils"
import { selectClass } from "@/components/field"

export type Option = { id: string; label: string; hint?: string }

/** One page of options for a search: the API's own `q` and paging. */
export type UseOptions = (
  q: string,
  page: number
) => {
  options?: Option[]
  total?: number
  size: number
  isLoading: boolean
  isError: boolean
}

/** The label of the chosen id (a single GET, so it shows even beyond the first page). */
export type UseLabel = (id: string) => string | undefined

const PAGE_SIZE = 20

/** Waits until typing stops (250 ms) before the search goes to the API. */
function useDebounced(value: string) {
  const [debounced, setDebounced] = React.useState(value)
  React.useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), 250)
    return () => clearTimeout(timer)
  }, [value])
  return debounced
}

function OptionsPage({
  q,
  page,
  last,
  useOptions,
  value,
  onPick,
  onMore,
}: {
  q: string
  page: number
  last: boolean
  useOptions: UseOptions
  value: string
  onPick: (o: Option) => void
  onMore: () => void
}) {
  const t = useTranslations("picker")
  const r = useOptions(q, page)
  if (r.isLoading)
    return (
      <li className="flex min-h-11 items-center gap-2 px-3 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" aria-hidden />
        {t("loading")}
      </li>
    )
  if (r.isError)
    return (
      <li role="alert" className="px-3 py-2 text-sm text-destructive">
        {t("error")}
      </li>
    )
  const options = r.options ?? []
  if (page === 1 && options.length === 0)
    return (
      <li className="px-3 py-2 text-sm text-muted-foreground">{t("none")}</li>
    )
  return (
    <>
      {options.map((o) => (
        <li key={o.id} role="presentation">
          <button
            type="button"
            role="option"
            aria-selected={o.id === value}
            onClick={() => onPick(o)}
            className={cn(
              "flex min-h-11 w-full flex-col justify-center rounded-lg px-3 text-left text-sm hover:bg-muted focus-visible:bg-muted focus-visible:outline-none",
              o.id === value && "font-semibold"
            )}
          >
            <span>{o.label}</span>
            {o.hint ? (
              <span className="text-xs text-muted-foreground">{o.hint}</span>
            ) : null}
          </button>
        </li>
      ))}
      {last && (r.total ?? 0) > page * r.size ? (
        <li role="presentation" className="p-1">
          <Button
            type="button"
            variant="ghost"
            className="min-h-11 w-full"
            onClick={onMore}
          >
            {t("more", { shown: page * r.size, total: r.total ?? 0 })}
          </Button>
        </li>
      ) : null}
    </>
  )
}

/**
 * A picker over a long API list: type to search (the API's `q`), "More" loads the next page.
 * With `name` it also posts the chosen id in a form. Replaces first-100 `<select>`s.
 */
export function SearchPicker({
  id,
  name,
  value,
  onChange,
  useOptions,
  useLabel,
  placeholder,
  disabled,
  "aria-invalid": invalid,
  "aria-describedby": describedBy,
}: {
  id?: string
  name?: string
  value: string
  onChange: (id: string, option?: Option) => void
  useOptions: UseOptions
  useLabel: UseLabel
  placeholder?: string
  disabled?: boolean
  "aria-invalid"?: boolean
  "aria-describedby"?: string
}) {
  const t = useTranslations("picker")
  const [open, setOpen] = React.useState(false)
  const [text, setText] = React.useState("")
  // Pages loaded for one search; a new search starts again at page 1.
  const [paged, setPaged] = React.useState({ q: "", n: 1 })
  const [picked, setPicked] = React.useState<Option>()
  const q = useDebounced(text)
  const known = useLabel(value)
  const label = (picked?.id === value ? picked.label : undefined) ?? known
  const listId = React.useId()
  const box = React.useRef<HTMLDivElement>(null)

  const pages = paged.q === q ? paged.n : 1
  React.useEffect(() => {
    if (!open) return
    const close = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener("mousedown", close)
    return () => document.removeEventListener("mousedown", close)
  }, [open])

  const pick = (o: Option) => {
    setPicked(o)
    onChange(o.id, o)
    setOpen(false)
    setText("")
  }

  return (
    <div
      ref={box}
      className="relative"
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation()
          setOpen(false)
        }
      }}
    >
      {name ? <input type="hidden" name={name} value={value} /> : null}
      <button
        id={id}
        type="button"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-invalid={invalid}
        aria-describedby={describedBy}
        onClick={() => setOpen((o) => !o)}
        className={cn(
          selectClass,
          "flex w-full items-center justify-between gap-2 text-left disabled:opacity-50",
          !value && "text-muted-foreground"
        )}
      >
        <span className="truncate">
          {value ? (label ?? t("loading")) : (placeholder ?? t("choose"))}
        </span>
        <ChevronDown className="size-4 shrink-0 opacity-60" aria-hidden />
      </button>
      {open ? (
        <div className="absolute z-50 mt-1 w-full min-w-64 rounded-xl border bg-popover p-1 shadow-lg">
          <Input
            autoFocus
            type="search"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={t("search")}
            aria-label={t("search")}
            maxLength={100}
            className="mb-1"
          />
          <ul
            id={listId}
            role="listbox"
            aria-label={placeholder ?? t("choose")}
            className="max-h-64 overflow-y-auto"
          >
            {Array.from({ length: pages }, (_, i) => (
              <OptionsPage
                key={`${q}:${i}`}
                q={q}
                page={i + 1}
                last={i + 1 === pages}
                useOptions={useOptions}
                value={value}
                onPick={pick}
                onMore={() => setPaged({ q, n: pages + 1 })}
              />
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  )
}

export { PAGE_SIZE }
