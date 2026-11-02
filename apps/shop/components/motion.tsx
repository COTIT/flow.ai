"use client"

import { useSyncExternalStore, type ReactNode } from "react"

import { island } from "@genesy/ui/components/island"

// React Bits pieces, each in its own chunk; nothing on the server or under reduced motion.
const BlurText = island(() => import("./rb/BlurText"))
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

/** The headline comes into focus word by word; screen readers get the plain sentence. */
export function Headline({ text }: { text: string }) {
  return (
    <BlurText
      plain={text}
      text={text}
      animateBy="words"
      direction="top"
      delay={90}
    />
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
