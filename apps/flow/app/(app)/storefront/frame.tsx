"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { PROPOSAL, readyFromFrame, type Proposal } from "@/lib/storefront-proposal"

const devices = {
  mobile: { width: 375, height: 812 },
  tablet: { width: 834, height: 1112 },
  desktop: { width: 1280, height: 800 },
} as const
type Device = keyof typeof devices

/**
 * A saved revision, or a proposal (an edit's dry run), rendered by /storefront/preview in a frame
 * of a real device width, so the shared renderer's breakpoints apply; the frame is scaled down to
 * fit. The URL carries only the revision id (or `proposal=1`): the frame reads the login and
 * workspace from this origin's own session, and a proposal arrives by message from this page.
 */
export function StorefrontFrame(
  props: { revisionId: string; proposal?: never } | { proposal: Proposal; revisionId?: never }
) {
  const { revisionId, proposal } = props
  const t = useTranslations("flow.storefront")
  const [device, setDevice] = React.useState<Device>("mobile")
  const box = React.useRef<HTMLDivElement>(null)
  const frame = React.useRef<HTMLIFrameElement>(null)
  const [room, setRoom] = React.useState(0)
  React.useEffect(() => {
    const element = box.current!
    const observer = new ResizeObserver(() => setRoom(element.clientWidth))
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  // A proposal goes only to our own frame, only once it says it is ready, only to this origin.
  React.useEffect(() => {
    if (!proposal) return
    const onMessage = (event: MessageEvent) => {
      const target = frame.current?.contentWindow
      if (!readyFromFrame(event, target, location.origin)) return
      target!.postMessage(
        { type: PROPOSAL, document: proposal.document, collections: proposal.collections },
        location.origin
      )
    }
    window.addEventListener("message", onMessage)
    return () => window.removeEventListener("message", onMessage)
  }, [proposal])
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
            className="min-h-11"
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
              ref={frame}
              title={t(proposal ? "proposalFrameTitle" : "frameTitle")}
              src={
                proposal
                  ? "/storefront/preview?proposal=1"
                  : `/storefront/preview?revision=${encodeURIComponent(revisionId!)}`
              }
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
