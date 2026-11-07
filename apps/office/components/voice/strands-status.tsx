"use client"

import { useEffect, useRef, useState } from "react"
import { Mic } from "lucide-react"
import { useMotionAllowed } from "@genesy/ui/components/island"
import { useVoice } from "./provider"
import { voiceMotionActive } from "./motion"

/** Lightweight brand rings keep request feedback responsive without a WebGL context. */
export function StrandsStatus() {
  const {
    connected,
    textOnly,
    muted,
    capturePaused,
    speaking,
    inputLevel,
    thinking,
    requesting,
    busy,
  } = useVoice()
  const element = useRef<HTMLDivElement>(null)
  const [visible, setVisible] = useState(false)
  const [moving, setMoving] = useState(false)
  const lastInput = useRef<number | null>(null)
  const motionAllowed = useMotionAllowed()
  useEffect(() => {
    const target = element.current
    if (!target) return
    let intersecting = typeof IntersectionObserver === "undefined"
    const update = () =>
      setVisible(intersecting && document.visibilityState === "visible")
    const observer =
      typeof IntersectionObserver === "undefined"
        ? null
        : new IntersectionObserver(([entry]) => {
            intersecting = Boolean(entry?.isIntersecting)
            update()
          })
    observer?.observe(target)
    update()
    document.addEventListener("visibilitychange", update)
    return () => {
      observer?.disconnect()
      document.removeEventListener("visibilitychange", update)
    }
  }, [])
  const canListen =
    visible &&
    motionAllowed &&
    connected &&
    !textOnly &&
    !muted &&
    !capturePaused
  useEffect(() => {
    if (!canListen) return
    const sample = () => {
      const now = Date.now()
      if (inputLevel() > 0.015) lastInput.current = now
      setMoving(
        voiceMotionActive(
          {
            visible,
            connected,
            textOnly,
            muted,
            speaking,
            lastInputAt: lastInput.current,
          },
          now
        )
      )
    }
    sample()
    const timer = setInterval(sample, 200)
    return () => clearInterval(timer)
  }, [canListen, visible, connected, textOnly, muted, speaking, inputLevel])
  return (
    <div
      ref={element}
      data-voice-orb=""
      aria-hidden="true"
      data-animate={visible && motionAllowed && !muted && !capturePaused}
      data-active={
        thinking || requesting || busy || speaking || (canListen && moving)
      }
      className="voice-orb relative mx-auto grid size-40 shrink-0 place-items-center rounded-full sm:size-56"
    >
      <span />
      <span />
      <span />
      <Mic className="relative size-7 text-foreground/80" />
    </div>
  )
}
