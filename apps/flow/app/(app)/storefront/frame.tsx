"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"

const devices = {
  mobile: { width: 375, height: 812 },
  tablet: { width: 834, height: 1112 },
  desktop: { width: 1280, height: 800 },
} as const
type Device = keyof typeof devices

/**
 * The revision rendered by /storefront/preview in a frame of a real device width, so the shared
 * renderer's breakpoints apply; the frame is scaled down to fit. The URL carries only the
 * revision id: the frame reads the login and workspace from this origin's own session.
 */
export function StorefrontFrame({ revisionId }: { revisionId: string }) {
  const t = useTranslations("flow.storefront")
  const [device, setDevice] = React.useState<Device>("mobile")
  const box = React.useRef<HTMLDivElement>(null)
  const [room, setRoom] = React.useState(0)
  React.useEffect(() => {
    const element = box.current!
    const observer = new ResizeObserver(() => setRoom(element.clientWidth))
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  const { width, height } = devices[device]
  const scale = room ? Math.min(1, room / width) : 0

  return (
    <div className="flex min-w-0 flex-col gap-3">
      <div role="group" aria-label={t("size")} className="flex flex-wrap gap-2">
        {(Object.keys(devices) as Device[]).map((d) => (
          <Button
            key={d}
            variant={d === device ? "default" : "outline"}
            size="sm"
            aria-pressed={d === device}
            onClick={() => setDevice(d)}
          >
            {t(d)}
          </Button>
        ))}
      </div>
      <div ref={box} className="w-full min-w-0">
        <div
          className="relative mx-auto overflow-hidden rounded-xl border bg-background"
          style={{ width: width * scale, height: height * scale }}
        >
          {scale ? (
            <iframe
              title={t("frameTitle")}
              src={`/storefront/preview?revision=${encodeURIComponent(revisionId)}`}
              // For inertness, not isolation: the frame is Flow's own origin. Without
              // allow-forms, allow-popups and allow-top-navigation it cannot submit, open
              // windows or move this page.
              sandbox="allow-scripts allow-same-origin"
              referrerPolicy="no-referrer"
              className="absolute top-0 left-0 origin-top-left border-0"
              style={{ width, height, transform: `scale(${scale})` }}
            />
          ) : null}
        </div>
      </div>
    </div>
  )
}
