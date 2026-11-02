"use client"

import { createElement, useSyncExternalStore, type ReactNode } from "react"

import { island } from "@genesy/ui/components/island"

// Animated pieces (React Bits copies, our tile fade), each in its own chunk; nothing on the
// server or under reduced motion.
const BlurText = island(() => import("./rb/BlurText"))
const GlareHover = island(() => import("./rb/GlareHover"))
const FadeTile = island(() => import("./fade-tile"))

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

/** A passport tile that fades in as it scrolls into view; drawn as is everywhere else. */
export function FadeIn({
  as = "section",
  children,
  ...props
}: {
  as?: "section" | "div"
  className?: string
  "aria-labelledby"?: string
  children: ReactNode
}) {
  return (
    <FadeTile plain={createElement(as, props, children)} as={as} {...props}>
      {children}
    </FadeTile>
  )
}
