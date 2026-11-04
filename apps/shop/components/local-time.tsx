"use client"

import { useSyncExternalStore } from "react"
import { useLocale } from "next-intl"

const noop = () => () => {}

/** A time in the visitor's own time zone; the server can't know it, so it renders after mount. */
export function LocalTime({ iso }: { iso: string }) {
  const locale = useLocale()
  const mounted = useSyncExternalStore(
    noop,
    () => true,
    () => false
  )
  // The server paints a hidden UTC time of the same length, so nothing moves when the real one lands.
  const text = new Intl.DateTimeFormat(locale, {
    timeStyle: "short",
    timeZone: mounted ? undefined : "UTC",
  }).format(new Date(iso))
  return (
    <time dateTime={iso} className={mounted ? undefined : "invisible"}>
      {text}
    </time>
  )
}
