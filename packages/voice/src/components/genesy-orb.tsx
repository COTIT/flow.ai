"use client"

import { useEffect, useId, useRef, useState, useSyncExternalStore } from "react"
import { island, useMotionAllowed } from "@genesy/ui/components/island"
import {
  createOrb,
  orbFps,
  orbFrame,
  orbMode,
  settleOrb,
  stepOrb,
  strandsProps,
  type OrbFrame,
  type OrbSim,
  type OrbState,
  type OrbTheme,
} from "../lib/orb"

// The 5-strand orb is SVG; the big WebGL orb keeps vendored Strands' 3 light sheets (its shader stays unchanged).
const Strands = island(() => import("./strands"))

// Apps set `.dark` on <html> (next-themes). The server snapshot is null, so nothing themed is drawn
// before hydration and the wrong palette never flashes.
const themeStore = {
  subscribe(onChange: () => void) {
    const observer = new MutationObserver(onChange)
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    })
    return () => observer.disconnect()
  },
  read: (): OrbTheme =>
    document.documentElement.classList.contains("dark") ? "dark" : "light",
  server: () => null,
}

// Read once per page load; the server has neither, so it draws the SVG orb.
let envCache: { webgl2: boolean; coarse: boolean } | undefined
const envStore = {
  subscribe: () => () => undefined,
  read: () =>
    (envCache ??= {
      // shortcut: checks the API exists, not that a context can be made; Strands then draws nothing.
      webgl2: typeof WebGL2RenderingContext !== "undefined",
      coarse: matchMedia("(pointer: coarse)").matches,
    }),
  server: () => SERVER_ENV,
}
const SERVER_ENV = { webgl2: false, coarse: false }

const stillLevel = (state: OrbState) =>
  state === "speaking" ? 0.55 : state === "listening" ? 0.4 : 0
const n = (v: number) => +v.toFixed(3)

export type GenesyOrbProps = {
  state: OrbState
  /** Voice level 0–1, read by the orb on each of its own frames (no re-render of the parent). */
  level?: () => number
  size?: number
  /** Defaults to the app theme. */
  theme?: OrbTheme
  /** Forces the still orb; prefers-reduced-motion does the same without it. */
  reducedMotion?: boolean
  /** Translated state text, always written as live text (the visuals are decorative). */
  label: string
  className?: string
}

/** The Genesy voice orb: one spring-driven light that morphs between ten states. */
export function GenesyOrb({
  state,
  level,
  size = 256,
  theme,
  reducedMotion = false,
  label,
  className = "",
}: GenesyOrbProps) {
  const appTheme = useSyncExternalStore(
    themeStore.subscribe,
    themeStore.read,
    themeStore.server
  )
  const look = theme ?? appTheme
  const motion = useMotionAllowed()
  const still = reducedMotion || !motion
  const element = useRef<HTMLDivElement>(null)
  const live = useRef({ state, level })
  useEffect(() => {
    live.current = { state, level }
  })
  const [moving, setMoving] = useState<OrbSim | null>(null)
  const env = useSyncExternalStore(
    envStore.subscribe,
    envStore.read,
    envStore.server
  )
  const id = useId().replace(/[^a-zA-Z0-9_-]/g, "")

  useEffect(() => {
    if (still) return
    const target = element.current!
    const sim = createOrb(live.current.state)
    let frame = 0
    let last = 0
    let onScreen = typeof IntersectionObserver === "undefined"
    const gap = 1000 / orbFps(size, env.coarse) - 2
    const loop = (now: number) => {
      frame = requestAnimationFrame(loop)
      if (last && now - last < gap) return
      const dt = last ? Math.min(0.05, (now - last) / 1000) : 1 / 60
      last = now
      stepOrb(sim, live.current.state, live.current.level?.() ?? 0, dt)
      setMoving({ ...sim, x: { ...sim.x } })
    }
    // Off screen or in a hidden tab nothing is stepped or drawn.
    const update = () => {
      cancelAnimationFrame(frame)
      frame = 0
      last = 0
      if (onScreen && document.visibilityState === "visible")
        frame = requestAnimationFrame(loop)
    }
    const observer =
      typeof IntersectionObserver === "undefined"
        ? null
        : new IntersectionObserver(([entry]) => {
            onScreen = Boolean(entry?.isIntersecting)
            update()
          })
    observer?.observe(target)
    update()
    document.addEventListener("visibilitychange", update)
    return () => {
      cancelAnimationFrame(frame)
      observer?.disconnect()
      document.removeEventListener("visibilitychange", update)
      setMoving(null)
    }
  }, [still, size, env.coarse])

  const shown =
    (!still && moving) || settleOrb(createOrb(state), state, stillLevel(state))
  const mode = orbMode({ size, webgl2: env.webgl2, reduced: still })
  const f = look ? orbFrame(shown, look) : null
  const soft = size < 100 || env.coarse ? 0.55 : 1

  return (
    <div
      ref={element}
      data-genesy-orb=""
      data-state={state}
      data-still={still}
      data-reduced={reducedMotion}
      className={`genesy-orb relative shrink-0 ${className}`.trim()}
      style={{ width: size, height: size }}
    >
      {f && look ? (
        <div aria-hidden="true" className="absolute inset-0">
          {mode === "webgl" ? (
            <>
              <div
                className="absolute inset-0"
                style={{ transform: `scale(${n(f.scale)})` }}
              >
                <Strands
                  {...strandsProps(shown, look)}
                  count={3}
                  glass
                  glassSize={0.952} // matches the SVG orb's radius (96 of 240)
                  saturation={1.3}
                  intro={false}
                  interactive={false}
                  clickPulse={false}
                  maxDevicePixelRatio={1.5}
                  className="absolute inset-0"
                  plain={<OrbSvg f={f} id={id} soft={soft} />}
                />
              </div>
              <OrbSvg f={f} id={id + "o"} soft={soft} overlay />
            </>
          ) : (
            <OrbSvg f={f} id={id} soft={soft} />
          )}
        </div>
      ) : null}
      <span role="status" aria-live="polite" className="genesy-orb-label">
        {label}
      </span>
    </div>
  )
}

/** The approved SVG orb; `overlay` draws only the rings, arcs, ripples and sparks over WebGL. */
function OrbSvg({
  f,
  id,
  soft,
  overlay = false,
}: {
  f: OrbFrame
  id: string
  soft: number
  overlay?: boolean
}) {
  const sparks = f.sparks.map((p, i) => (
    <circle
      key={i}
      cx={n(p.x)}
      cy={n(p.y)}
      r={2.2}
      fill="#fff1d6"
      opacity={n(p.o)}
    />
  ))
  return (
    <svg
      viewBox="-20 -20 240 240"
      className="absolute inset-0 size-full overflow-visible"
      style={{ opacity: n(f.fade) }}
    >
      {overlay ? null : (
        <defs>
          <radialGradient id={id + "s"} cx="50%" cy="30%" r="74%">
            <stop offset="0%" stopColor={f.body0} stopOpacity={f.bodyO[0]} />
            <stop offset="60%" stopColor={f.body1} stopOpacity={f.bodyO[1]} />
            <stop offset="100%" stopColor={f.body2} stopOpacity={f.bodyO[2]} />
          </radialGradient>
          <radialGradient id={id + "o"} cx="50%" cy="50%" r="50%">
            <stop offset="76%" stopColor={f.glowC} stopOpacity={0.55} />
            <stop offset="100%" stopColor={f.glowC} stopOpacity={0} />
          </radialGradient>
          <radialGradient id={id + "c"} cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor={f.coreC} stopOpacity={0.9} />
            <stop offset="40%" stopColor={f.coreC} stopOpacity={0.32} />
            <stop offset="100%" stopColor={f.coreC} stopOpacity={0} />
          </radialGradient>
          <radialGradient id={id + "h"} cx="30%" cy="22%" r="40%">
            <stop offset="0%" stopColor="#fff" stopOpacity={f.hl} />
            <stop offset="100%" stopColor="#fff" stopOpacity={0} />
          </radialGradient>
          {f.strands.map((s, i) => (
            <linearGradient
              key={i}
              id={id + "l" + i}
              gradientUnits="userSpaceOnUse"
              x1="4"
              y1="0"
              x2="196"
              y2="0"
            >
              <stop offset="0" stopColor={s.c} stopOpacity={0} />
              <stop offset="0.2" stopColor={s.c} />
              <stop offset="0.8" stopColor={s.c} />
              <stop offset="1" stopColor={s.c} stopOpacity={0} />
            </linearGradient>
          ))}
          <filter
            id={id + "g"}
            filterUnits="userSpaceOnUse"
            x="-10"
            y="-10"
            width="220"
            height="220"
          >
            <feGaussianBlur stdDeviation={n(f.blur * soft)} result="b" />
            <feMerge>
              <feMergeNode in="b" />
              <feMergeNode in="b" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
          <clipPath id={id + "k"}>
            <circle cx="100" cy="100" r="95" />
          </clipPath>
        </defs>
      )}
      {f.ripples.map((r, i) => (
        <circle
          key={i}
          cx="100"
          cy="100"
          r={n(r.r)}
          fill="none"
          stroke={f.rippleC}
          strokeWidth={1.4}
          opacity={n(r.o)}
        />
      ))}
      <g
        transform={`translate(100 100) scale(${n(f.scale)}) translate(-100 -100)`}
      >
        {overlay ? (
          sparks
        ) : (
          <>
            <circle
              cx="100"
              cy="100"
              r="126"
              fill={`url(#${id}o)`}
              opacity={n(f.glowO)}
            />
            <circle cx="100" cy="100" r="96" fill={`url(#${id}s)`} />
            <g clipPath={`url(#${id}k)`}>
              <circle
                cx="100"
                cy="100"
                r={n(f.coreR)}
                fill={`url(#${id}c)`}
                opacity={n(f.coreO)}
              />
              <g filter={`url(#${id}g)`} style={{ mixBlendMode: f.blend }}>
                {f.strands.map((s, i) => (
                  <path
                    key={i}
                    d={s.d}
                    fill="none"
                    stroke={`url(#${id}l${i})`}
                    strokeWidth={n(s.w)}
                    strokeLinecap="round"
                    strokeDasharray={s.dash}
                    opacity={n(s.o)}
                  />
                ))}
              </g>
              {sparks}
              <circle cx="100" cy="100" r="96" fill={`url(#${id}h)`} />
            </g>
            <circle
              cx="100"
              cy="100"
              r="96"
              fill="none"
              stroke={f.edge}
              strokeWidth={1}
            />
          </>
        )}
        <circle
          cx="100"
          cy="100"
          r="95"
          fill="none"
          stroke={f.rimC}
          strokeWidth={2.6}
          strokeDasharray="150 447"
          strokeLinecap="round"
          opacity={n(f.rimO)}
          transform={`rotate(${n(f.rimRot)} 100 100)`}
        />
        <circle
          cx="100"
          cy="100"
          r="99.5"
          fill="none"
          stroke={f.arcC}
          strokeWidth={2.4}
          strokeDasharray={f.arcDash}
          strokeLinecap="round"
          opacity={n(f.arcO)}
          transform={`rotate(${n(f.arcRot)} 100 100)`}
        />
      </g>
    </svg>
  )
}
