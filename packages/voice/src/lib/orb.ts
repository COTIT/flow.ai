// The Genesy orb's motion, typed from the approved design's orb-core (Claude Design "The Genesy orb").
// Every visual setting is a number on a critically damped spring, so a state change morphs in
// ~0.6 s instead of switching; the voice level has its own faster spring (~0.1 s attack).

export const ORB_STATES = [
  "idle",
  "connecting",
  "listening",
  "thinking",
  "speaking",
  "waiting",
  "executing",
  "recovering",
  "error",
  "ended",
] as const
export type OrbState = (typeof ORB_STATES)[number]
export type OrbTheme = "dark" | "light"
type Rgb = [number, number, number]

// Dark pages: luminous pale-glass body, soft gold glow. Light pages: pearl body, deeper gold/teal.
const PAL = {
  dark: {
    G: "#efc977",
    P: "#f9dd8c",
    T: "#86e0e4",
    R: "#fda29b",
    W: "#fff6e6",
    body: ["#d9d1c0", "#8f887b", "#f3ecdc"],
    bodyO: [0.34, 0.2, 0.16],
    glow: "#efc977",
    glowO: 0.22,
    blend: "screen",
    edge: "rgb(255,250,240,0.32)",
    hl: 0.28,
    coreBoost: 1.35,
  },
  light: {
    G: "#b8862b",
    P: "#d9a441",
    T: "#23898f",
    R: "#c0392b",
    W: "#a26f1e",
    body: ["#ffffff", "#f5f0e6", "#e3d8c4"],
    bodyO: [1, 1, 1],
    glow: "#e2b04f",
    glowO: 0.3,
    blend: "normal",
    edge: "rgb(28,27,24,0.16)",
    hl: 0.7,
    coreBoost: 0.9,
  },
} as const

const BASE = {
  amp: 6,
  ampL: 0,
  speed: 0.45,
  braid: 0,
  teal: 0,
  red: 0,
  core: 0.24,
  coreR: 30,
  coreL: 0,
  rim: 0.28,
  rimPulse: 0,
  arc: 0,
  arcSpeed: 0,
  ripple: 0,
  flicker: 0,
  dash: 0,
  fade: 1,
  spread: 0.6,
  sparks: 0,
  breathe: 1,
  swell: 0,
}
type Params = typeof BASE

const PRESETS: Record<OrbState, Partial<Params>> = {
  idle: {},
  connecting: {
    amp: 3,
    speed: 1.3,
    teal: 0.15,
    core: 0.34,
    coreR: 22,
    rim: 0.18,
    arc: 1,
    arcSpeed: 240,
    spread: 0.25,
    breathe: 0,
  },
  listening: {
    amp: 5,
    ampL: 46,
    speed: 1.1,
    teal: 1,
    core: 0.22,
    coreR: 24,
    coreL: 34,
    rim: 0.32,
    spread: 0.95,
    breathe: 0,
    swell: 0.025,
  },
  thinking: {
    amp: 15,
    speed: 1.9,
    braid: 1,
    teal: 0.35,
    core: 0.3,
    coreR: 30,
    rim: 0.2,
    spread: 1,
    sparks: 1,
    breathe: 0,
  },
  speaking: {
    amp: 8,
    ampL: 76,
    speed: 1.6,
    core: 0.34,
    coreR: 30,
    coreL: 50,
    rim: 0.36,
    ripple: 1,
    spread: 1,
    breathe: 0,
    swell: 0.06,
  },
  waiting: {
    amp: 2.5,
    speed: 0.4,
    core: 0.3,
    coreR: 26,
    rim: 0.55,
    rimPulse: 1,
    spread: 0.15,
    breathe: 0,
  },
  executing: {
    amp: 4,
    speed: 3,
    core: 0.32,
    coreR: 28,
    rim: 0.15,
    arc: 1,
    arcSpeed: 320,
    spread: 0.4,
    breathe: 0,
  },
  recovering: {
    amp: 8,
    speed: 1,
    core: 0.18,
    coreR: 20,
    rim: 0.15,
    arc: 0.7,
    arcSpeed: 90,
    flicker: 1,
    dash: 1,
    spread: 0.6,
    breathe: 0,
  },
  error: {
    amp: 0,
    speed: 0,
    red: 1,
    core: 0.12,
    coreR: 18,
    rim: 0.65,
    spread: 0,
    breathe: 0,
  },
  ended: {
    amp: 1,
    speed: 0.2,
    core: 0.22,
    coreR: 10,
    rim: 0.1,
    fade: 0.5,
    spread: 0.2,
    breathe: 0,
  },
}

const STRANDS = [
  { c: "G", k: 1.2, w: 3.2, o: 0.95, ph: 0 },
  { c: "P", k: 1.7, w: 2, o: 0.75, ph: 1.9 },
  { c: "T", k: 0.9, w: 1.6, o: 0.6, ph: 3.8 },
  { c: "G", k: 2.3, w: 1.1, o: 0.45, ph: 5.1 },
  { c: "W", k: 1.45, w: 0.8, o: 0.5, ph: 2.6 },
] as const

export type OrbSim = {
  t: number
  ph: number
  arc: number
  lvl: number
  lv: number
  lastRip: number
  ripples: { age: number; k: number }[]
  x: Params
  v: Params
}

const keys = Object.keys(BASE) as (keyof Params)[]
const target = (state: OrbState): Params => ({ ...BASE, ...PRESETS[state] })

export function createOrb(state: OrbState): OrbSim {
  const zero = Object.fromEntries(keys.map((k) => [k, 0])) as Params
  return {
    t: 0,
    ph: 0,
    arc: 0,
    lvl: 0,
    lv: 0,
    lastRip: -1,
    ripples: [],
    x: target(state),
    v: zero,
  }
}

/** Critically damped spring, one semi-implicit Euler sub-step. */
const spring = (x: number, v: number, goal: number, w: number, h: number) => {
  const nv = v + (w * w * (goal - x) - 2 * w * v) * h
  return [x + nv * h, nv] as const
}

/** Advances the orb by `dt` seconds in ≤8 ms sub-steps, so any frame rate gives the same motion. */
export function stepOrb(
  sim: OrbSim,
  state: OrbState,
  level: number,
  dt: number
): OrbSim {
  const goal = target(state)
  const n = Math.max(1, Math.ceil(dt / 0.008))
  const h = dt / n
  for (let i = 0; i < n; i++) {
    for (const k of keys)
      [sim.x[k], sim.v[k]] = spring(sim.x[k], sim.v[k], goal[k], 6.5, h)
    const [lvl, lv] = spring(sim.lvl, sim.lv, level, 20, h)
    sim.lvl = Math.max(0, lvl)
    sim.lv = lv
    sim.ph += sim.x.speed * h
    sim.arc += sim.x.arcSpeed * h
    sim.t += h
  }
  if (sim.x.ripple > 0.5 && sim.lvl > 0.42 && sim.t - sim.lastRip > 0.3) {
    sim.ripples.push({ age: 0, k: sim.lvl })
    sim.lastRip = sim.t
  }
  sim.ripples = sim.ripples.filter((r) => (r.age += dt) <= 1.1)
  return sim
}

/** Still frame (reduced motion, first paint): springs settled on the state, no ripples. */
export function settleOrb(sim: OrbSim, state: OrbState, level: number): OrbSim {
  for (let i = 0; i < 90; i++) stepOrb(sim, state, level, 1 / 30)
  sim.ripples = []
  return sim
}

const rgb = (c: string): Rgb =>
  [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16)) as Rgb
const mix = (a: string | Rgb, b: string | Rgb, f: number): Rgb => {
  const A = typeof a === "string" ? rgb(a) : a
  const B = typeof b === "string" ? rgb(b) : b
  const k = Math.max(0, Math.min(1, f))
  return [0, 1, 2].map((i) => A[i]! + (B[i]! - A[i]!) * k) as Rgb
}
const css = (c: Rgb) => `rgb(${c.map(Math.round).join(",")})`
const path = (pts: [number, number][]) => {
  let d = `M${pts[0]![0]},${pts[0]![1].toFixed(1)}`
  for (let i = 1; i < pts.length - 1; i++) {
    const [x, y] = pts[i]!
    const [nx, ny] = pts[i + 1]!
    d += `Q${x},${y.toFixed(1)} ${(x + nx) / 2},${((y + ny) / 2).toFixed(1)}`
  }
  const e = pts[pts.length - 1]!
  return `${d}L${e[0]},${e[1].toFixed(1)}`
}

/** Everything the SVG orb needs for one frame, in a 200 × 200 view box. */
export function orbFrame(sim: OrbSim, theme: OrbTheme) {
  const pal = PAL[theme]
  const { x, lvl: l, t, ph } = sim
  const lead = mix(mix(pal.G, pal.T, x.teal), pal.R, x.red)
  const strands = STRANDS.map((st, i) => {
    const fl = 1 - x.flicker * 0.45 * (1 + Math.sin(t * 9 + i * 1.3))
    const A =
      (x.amp + x.ampL * l) * (1 - i * 0.13) * (i === 0 ? 1 : x.spread) * fl
    const phi = st.ph * (1 - x.braid) + i * 2.513 * x.braid
    const pts: [number, number][] = []
    for (let px = 4; px <= 196; px += 6) {
      const w = Math.pow(Math.sin((Math.PI * (px - 4)) / 192), 1.35)
      const y =
        100 +
        w *
          (A * Math.sin((st.k * px) / 30 + ph * (1 + i * 0.12) + phi) +
            0.24 * A * Math.sin((st.k * px) / 12 - ph * 1.7 + phi * 2))
      pts.push([px, y])
    }
    return {
      d: path(pts),
      c: css(
        mix(
          mix(pal[st.c], pal.T, x.teal * (i === 2 ? 0.4 : 0.85)),
          pal.R,
          x.red
        )
      ),
      w: st.w * (1 + x.swell * 6 * l),
      o:
        st.o *
        (0.55 + 0.45 * x.fade) *
        (i > 0 ? Math.min(1, 0.25 + x.spread) : 1),
      dash: x.dash > 0.5 ? (i % 2 ? "3 9" : "6 7") : "none",
    }
  })
  return {
    strands,
    coreR: x.coreR + x.coreL * l + 2.5 * x.breathe * Math.sin(t * 1.1),
    coreO: Math.min(
      1,
      (x.core + 0.5 * l * (x.coreL > 1 ? 1 : 0)) * pal.coreBoost
    ),
    coreC: css(lead),
    rimO: Math.max(
      0,
      x.rim * (1 - x.rimPulse * 0.35 * (1 + Math.sin(t * 3.1))) +
        0.4 * l * x.ripple
    ),
    rimC: css(lead),
    rimRot: -30 + 12 * Math.sin(t * 0.4),
    arcO: Math.max(0, x.arc),
    arcRot: sim.arc % 360,
    arcDash: x.dash > 0.5 ? "24 40" : "80 542",
    ripples: sim.ripples.map((r) => {
      const f = r.age / 1.1
      return { r: 97 + f * 26, o: (1 - f) * (1 - f) * r.k * 0.75 }
    }),
    sparks:
      x.sparks > 0.05
        ? [0, 1, 2].map((i) => {
            const a = t * 1.5 + i * 2.094
            return {
              x: 100 + 64 * Math.cos(a),
              y: 100 + 20 * Math.sin(a),
              o: x.sparks * (0.55 + 0.45 * Math.sin(a)),
            }
          })
        : [],
    body0: pal.body[0],
    body1: pal.body[1],
    body2: pal.body[2],
    bodyO: pal.bodyO,
    glowC: pal.glow,
    glowO: pal.glowO * (0.6 + 0.4 * x.fade) * (1 - x.red * 0.6) + 0.25 * l,
    blend: pal.blend,
    edge: pal.edge,
    hl: pal.hl,
    arcC: pal.P,
    rippleC: pal.G,
    scale: 1 + x.swell * l + 0.012 * x.breathe * Math.sin(t * 1.1),
    fade: 0.45 + 0.55 * x.fade,
    blur: 2.2 + 3.2 * l * (x.ampL > 1 ? 1 : 0),
  }
}
export type OrbFrame = ReturnType<typeof orbFrame>

/** The voice-session flags the orb reads (a structural subset of the provider's value). */
export type OrbVoice = {
  connected: boolean
  starting: boolean
  upgrading: boolean
  busy: boolean
  thinking: boolean
  requesting: boolean
  textOnly: boolean
  muted: boolean
  capturePaused: boolean
  speaking: boolean
  error: string
  unfinished: "stalled" | "dropped" | null
  review: unknown
  messages: readonly unknown[]
}

export function orbState(v: OrbVoice): OrbState {
  if (v.error) return "error"
  if (v.unfinished) return "recovering"
  if (v.starting || v.upgrading) return "connecting"
  if (!v.connected) return v.messages.length ? "ended" : "idle"
  if (v.busy) return "executing"
  if (v.review) return "waiting"
  if (v.thinking || v.requesting) return "thinking"
  if (v.speaking) return "speaking"
  if (!v.textOnly && !v.muted && !v.capturePaused) return "listening"
  return "idle"
}

export const orbFps = (size: number, coarse: boolean) =>
  coarse || size < 100 ? 30 : 60
