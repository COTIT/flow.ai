"use client"

import { useLayoutEffect, useRef, type ReactNode } from "react"

import { useMotionAllowed } from "@genesy/ui/components/island"

/**
 * A tile that fades in when scrolled to. Tiles already on screen when it mounts are left
 * alone, so nothing the reader sees blinks and the largest paint never waits. Opacity only:
 * no layout shift. The same element from the server render on, so its contents (photos) are
 * never torn down and rebuilt; it only animates once the client allows motion (`data-motion`).
 */
export default function FadeTile({
  as: Tag,
  children,
  duration = 0.5,
  ...props
}: {
  as: "section" | "div"
  className?: string
  "aria-labelledby"?: string
  children: ReactNode
  duration?: number
}) {
  const motion = useMotionAllowed()
  // Div-typed so JSX accepts it on either tag; only HTMLElement members are used.
  const ref = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const el = ref.current
    if (
      !motion ||
      !el ||
      typeof IntersectionObserver === "undefined" ||
      el.getBoundingClientRect().top < window.innerHeight
    )
      return
    el.style.opacity = "0"
    const seen = new IntersectionObserver(
      ([entry]) => {
        if (!entry?.isIntersecting) return
        seen.disconnect()
        el.style.opacity = ""
        el.animate?.(
          { opacity: [0, 1] },
          { duration: duration * 1000, easing: "ease-out" }
        )
      },
      { threshold: 0.15 }
    )
    seen.observe(el)
    return () => {
      seen.disconnect()
      el.style.opacity = ""
    }
  }, [motion, duration])
  return (
    <Tag ref={ref} data-motion={motion ? "" : undefined} {...props}>
      {children}
    </Tag>
  )
}
