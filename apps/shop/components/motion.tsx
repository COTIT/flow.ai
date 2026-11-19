"use client"

import { useSyncExternalStore, type ReactNode } from "react"

import { island } from "@verzo/ui/components/island"

import FadeTile from "./fade-tile"

// Animated pieces (React Bits copies), each in its own chunk; nothing on the
// server or under reduced motion.
const GlareHover = island(() => import("./rb/GlareHover"))

const FINE = "(pointer: fine)"

/** A mouse or trackpad: glare follows a pointer, so touch screens never load it. */
function useFinePointer() {
  return useSyncExternalStore(
    (onChange) => {
      const query = matchMedia(FINE)
      query.addEventListener("change", onChange)
      return () => query.removeEventListener("change", onChange)
    },
    () => matchMedia(FINE).matches,
    () => false
  )
}

/** A soft gold glare across a product photo under a mouse. */
export function Glare({
  children,
  radius,
}: {
  children: ReactNode
  /** The photo's corner radius, so the glare stays inside it. */
  radius: string
}) {
  const fine = useFinePointer()
  if (!fine) return children
  return (
    <GlareHover
      plain={children}
      glareColor="#f9dd8c"
      glareOpacity={0.3}
      background="transparent"
      borderColor="transparent"
      width="100%"
      height="100%"
      borderRadius={radius}
    >
      {children}
    </GlareHover>
  )
}

/** A passport tile that fades in as it scrolls into view; drawn as is everywhere else. */
export function FadeIn({
  as = "section",
  children,
  duration,
  ...props
}: {
  as?: "section" | "div"
  className?: string
  "aria-labelledby"?: string
  children: ReactNode
  duration?: number
}) {
  return (
    <FadeTile as={as} duration={duration} {...props}>
      {children}
    </FadeTile>
  )
}
