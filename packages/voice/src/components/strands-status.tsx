"use client"

import { useEffect, useRef, useState } from "react"
import { island, useMotionAllowed } from "@genesy/ui/components/island"
import { useVoice } from "./provider"
import { voiceMotionActive } from "../lib/motion"

// React Bits Strands (WebGL2) is fetched lazily; the brand rings below stay as the still version.
const Strands = island(() => import("./strands"))
const COLORS = ["#efc977", "#6fd6dd", "#f9dd8c"]
const rings = (
  <>
    <span />
    <span />
    <span />
  </>
)

/** Audio-reactive avatar: strands swell with the agent's voice, breathe while listening. */
export function StrandsStatus() {
  const {
    connected,
    textOnly,
    muted,
    capturePaused,
    speaking,
    inputLevel,
    outputLevel,
    thinking,
    requesting,
    busy,
  } = useVoice()
  const element = useRef<HTMLDivElement>(null)
  const [visible, setVisible] = useState(false)
  const [moving, setMoving] = useState(false)
  const [level, setLevel] = useState(0)
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
  const live = visible && motionAllowed && connected
  useEffect(() => {
    if (!live) return
    let frame = 0
    let last = 0
    const sample = (now: number) => {
      frame = requestAnimationFrame(sample)
      if (now - last < 33) return // ~30 fps is plenty for the strands
      last = now
      const input = canListen ? inputLevel() : 0
      if (input > 0.015) lastInput.current = Date.now()
      setLevel(speaking ? outputLevel() : input)
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
          Date.now()
        )
      )
    }
    frame = requestAnimationFrame(sample)
    return () => cancelAnimationFrame(frame)
  }, [
    live,
    canListen,
    visible,
    connected,
    textOnly,
    muted,
    speaking,
    inputLevel,
    outputLevel,
  ])
  const working = thinking || requesting || busy
  return (
    <div
      ref={element}
      data-voice-orb=""
      aria-hidden="true"
      data-animate={visible && motionAllowed && !muted && !capturePaused}
      data-active={working || speaking || (canListen && moving)}
      className="voice-orb relative mx-auto size-64 shrink-0 sm:size-80"
    >
      {/* Off screen or in a hidden tab the GPU rests: the still rings stand in. */}
      {visible ? (
        <Strands
          colors={COLORS}
          count={3}
          glass
          intensity={1.4}
          saturation={1.3}
          glow={1.2}
          thickness={1.1}
          speed={working ? 1.6 : 1}
          energy={working ? 0.85 : speaking ? 0.7 : 0.45}
          listening={canListen && !speaking}
          level={live && (speaking || canListen) ? level : undefined}
          intro={false}
          interactive={false}
          clickPulse={false}
          className="absolute inset-0"
          plain={rings}
        />
      ) : (
        rings
      )}
    </div>
  )
}
