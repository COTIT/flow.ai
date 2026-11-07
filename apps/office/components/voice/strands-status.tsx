"use client"

import { useEffect, useRef, useState } from "react"
import { useTheme } from "next-themes"
import { Mic } from "lucide-react"
import { island, useMotionAllowed } from "@genesy/ui/components/island"
import { useVoice } from "./provider"
import { voiceMotionActive } from "./motion"

const Strands = island(() => import("./strands"))

export function StrandsStatus() {
  const { connected, textOnly, muted, speaking, inputLevel } = useVoice()
  const { resolvedTheme } = useTheme()
  const element = useRef<HTMLDivElement>(null)
  const [visible, setVisible] = useState(false)
  const [moving, setMoving] = useState(false)
  const lastInput = useRef<number | null>(null)
  const motionAllowed = useMotionAllowed()
  useEffect(() => {
    const target = element.current
    if (!target || typeof IntersectionObserver === "undefined") return
    let intersecting = false
    const update = () =>
      setVisible(intersecting && document.visibilityState === "visible")
    const observer = new IntersectionObserver(([entry]) => {
      intersecting = Boolean(entry?.isIntersecting)
      update()
    })
    observer.observe(target)
    document.addEventListener("visibilitychange", update)
    return () => {
      observer.disconnect()
      document.removeEventListener("visibilitychange", update)
    }
  }, [])
  const canListen = visible && motionAllowed && connected && !textOnly && !muted
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
  const plain = (
    <div className="grid size-full place-items-center rounded-full border border-(--hair) bg-(--glass-subtle)">
      <Mic className="size-8 text-foreground" />
    </div>
  )
  const animate = canListen && moving
  return (
    <div
      ref={element}
      aria-hidden="true"
      className="relative mx-auto h-28 w-40 shrink-0 overflow-hidden rounded-2xl md:h-32 md:w-48"
    >
      {plain}
      {animate ? (
        <div className="pointer-events-none absolute inset-0">
          <Strands
            plain={<span />}
            maxDevicePixelRatio={1}
            interactive={false}
            clickPulse={false}
            intro={false}
            lightMode={resolvedTheme !== "dark"}
            colors={
              resolvedTheme === "dark"
                ? ["#efc977", "#6fd6dd", "#b4c3d3"]
                : ["#0a183a", "#d0a55f", "#01909b"]
            }
            count={3}
            speed={speaking ? 0.25 : 0.12}
            intensity={speaking ? 0.45 : 0.15}
            saturation={0.8}
            glow={1}
            opacity={0.7}
            glass={false}
          />
        </div>
      ) : null}
    </div>
  )
}
