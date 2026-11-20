"use client"

import { useSyncExternalStore, type ReactNode } from "react"

import { island, useMotionAllowed } from "@verzo/ui/components/island"

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

/** GlareHover's own root look, so the photo sits in the same box it did inside it. */
const FRAME =
  "relative isolate inline-grid place-items-center overflow-hidden border border-solid border-transparent align-top"

/**
 * A soft gold glare across a product photo under a mouse. The photo's wrapper is the same
 * element from the server render on (`display: contents` until the glare is allowed), and the
 * glare loads as an empty layer over it, so the photo is never torn down and rebuilt.
 */
export function Glare({
  children,
  radius,
}: {
  children: ReactNode
  /** The photo's corner radius, so the glare stays inside it. */
  radius: string
}) {
  const fine = useFinePointer()
  const motion = useMotionAllowed()
  const on = fine && motion
  return (
    <div
      className={on ? FRAME : undefined}
      style={
        on
          ? { width: "100%", height: "100%", borderRadius: radius }
          : { display: "contents" }
      }
    >
      {children}
      {on ? (
        <GlareHover
          plain={null}
          glareColor="#f9dd8c"
          glareOpacity={0.3}
          background="transparent"
          borderColor="transparent"
          width="100%"
          height="100%"
          borderRadius={radius}
          // Over the photo, not around it; no isolation, so the glare still blends with the photo.
          style={{
            position: "absolute",
            inset: 0,
            borderWidth: 0,
            isolation: "auto",
          }}
        />
      ) : null}
    </div>
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
