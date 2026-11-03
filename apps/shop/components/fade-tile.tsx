"use client"

import { useLayoutEffect, useRef, type ReactNode } from "react"
import { animate, inView } from "motion"

/**
 * A tile that fades in when scrolled to. Tiles already on screen when it mounts are left
 * alone, so nothing the reader sees blinks and the largest paint never waits. Opacity only:
 * no layout shift. Loaded through `island` (see motion.tsx), never on the server.
 */
export default function FadeTile({
  as: Tag,
  children,
  ...props
}: {
  as: "section" | "div"
  className?: string
  "aria-labelledby"?: string
  children: ReactNode
}) {
  // Div-typed so JSX accepts it on either tag; only HTMLElement members are used.
  const ref = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const el = ref.current
    if (!el || el.getBoundingClientRect().top < window.innerHeight) return
    el.style.opacity = "0"
    const stop = inView(
      el,
      () => {
        animate(el, { opacity: 1 }, { duration: 0.5, ease: "easeOut" })
      },
      { amount: 0.15 }
    )
    return () => {
      stop()
      el.style.opacity = ""
    }
  }, [])
  return (
    <Tag ref={ref} {...props}>
      {children}
    </Tag>
  )
}
