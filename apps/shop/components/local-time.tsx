"use client"

import { useSyncExternalStore } from "react"
import { useLocale } from "next-intl"

const noop = () => () => {}

/** A time in the visitor's own time zone; the server can't know it, so it renders after mount. */
export function LocalTime({ iso }: { iso: string }) {
  const locale = useLocale()
  const text = useSyncExternalStore(
    noop,
    () =>
      new Intl.DateTimeFormat(locale, { timeStyle: "short" }).format(
        new Date(iso)
      ),
    () => ""
  )
  return <time dateTime={iso}>{text}</time>
}
