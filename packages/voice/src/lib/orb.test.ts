import { describe, expect, it } from "vitest"

import {
  ORB_STATES,
  createOrb,
  orbFps,
  orbFrame,
  orbState,
  settleOrb,
  stepOrb,
  type OrbState,
  type OrbVoice,
} from "./orb"

const run = (
  state: OrbState,
  from: OrbState,
  seconds: number,
  fps = 60,
  level = 0
) => {
  const sim = createOrb(from)
  for (let i = 0; i < Math.round(seconds * fps); i++)
    stepOrb(sim, state, level, 1 / fps)
  return sim
}

describe("orb springs", () => {
  it("morph a state change in about 0.6 s and settle by 1.2 s", () => {
    const start = createOrb("idle").x.amp
    const goal = createOrb("thinking").x.amp
    const part = (s: number) =>
      (run("thinking", "idle", s).x.amp - start) / (goal - start)
    expect(part(0.6)).toBeGreaterThan(0.85)
    expect(part(0.6)).toBeLessThan(1)
    expect(part(1.2)).toBeGreaterThan(0.99)
  })

  it("never overshoot the goal (critically damped)", () => {
    const sim = createOrb("idle")
    const goal = createOrb("thinking").x.amp
    let max = 0
    for (let i = 0; i < 180; i++)
      max = Math.max(max, stepOrb(sim, "thinking", 0, 1 / 60).x.amp)
    expect(max).toBeLessThanOrEqual(goal + 1e-3)
  })

  it("give the same motion at 60 and 30 fps", () => {
    const fast = run("speaking", "idle", 0.5, 60)
    const slow = run("speaking", "idle", 0.5, 30)
    for (const key of Object.keys(fast.x) as (keyof typeof fast.x)[])
      expect(Math.abs(fast.x[key] - slow.x[key])).toBeLessThan(
        0.01 * Math.max(1, Math.abs(fast.x[key]))
      )
    expect(fast.t).toBeCloseTo(slow.t, 6)
  })

  it("track the voice level faster than a state change", () => {
    const sim = run("speaking", "speaking", 0.15, 60, 1)
    expect(sim.lvl).toBeGreaterThan(0.75)
    const morph = run("thinking", "idle", 0.15)
    const start = createOrb("idle").x.amp
    expect(
      (morph.x.amp - start) / (createOrb("thinking").x.amp - start)
    ).toBeLessThan(0.5)
  })

  it("survive a long frame gap without blowing up", () => {
    const sim = createOrb("idle")
    stepOrb(sim, "executing", 1, 5)
    expect(Number.isFinite(sim.x.speed)).toBe(true)
    expect(sim.x.speed).toBeCloseTo(createOrb("executing").x.speed, 1)
  })
})

describe("orb presets", () => {
  it("cover the ten approved states, each a different shape", () => {
    expect(ORB_STATES).toEqual([
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
    ])
    const shapes = ORB_STATES.map((s) => JSON.stringify(createOrb(s).x))
    expect(new Set(shapes).size).toBe(ORB_STATES.length)
  })

  it("spawn ripples only while speaking on voice peaks, gone after 1.1 s", () => {
    expect(run("listening", "listening", 1, 60, 1).ripples).toHaveLength(0)
    expect(run("speaking", "speaking", 1, 60, 0.1).ripples).toHaveLength(0)
    const loud = run("speaking", "speaking", 1, 60, 1)
    expect(loud.ripples.length).toBeGreaterThan(0)
    for (let i = 0; i < 80; i++) stepOrb(loud, "idle", 0, 1 / 60)
    expect(loud.ripples).toHaveLength(0)
  })

  it("settle into a still frame with no ripples", () => {
    const sim = settleOrb(createOrb("idle"), "speaking", 0.55)
    expect(sim.ripples).toHaveLength(0)
    expect(sim.x.amp).toBeCloseTo(createOrb("speaking").x.amp, 1)
  })
})

describe("orbFrame (SVG)", () => {
  it("draws five strands, teal while listening, red on error", () => {
    const listening = orbFrame(
      settleOrb(createOrb("listening"), "listening", 0),
      "dark"
    )
    expect(listening.strands).toHaveLength(5)
    const teal = listening.strands[0]!.c
    expect(teal).not.toBe(orbFrame(createOrb("speaking"), "dark").strands[0]!.c)
    const error = orbFrame(createOrb("error"), "dark")
    expect(error.rimC).toBe("rgb(253,162,155)")
  })

  it("uses the pearl body and deeper gold in the light theme", () => {
    const dark = orbFrame(createOrb("idle"), "dark")
    const light = orbFrame(createOrb("idle"), "light")
    expect(light.body0).toBe("#ffffff")
    expect(light.strands[0]!.c).toBe("rgb(184,134,43)")
    expect(dark.strands[0]!.c).toBe("rgb(239,201,119)")
  })

  it("orbits sparks only while thinking", () => {
    expect(orbFrame(createOrb("thinking"), "dark").sparks).toHaveLength(3)
    expect(orbFrame(createOrb("idle"), "dark").sparks).toHaveLength(0)
  })
})

describe("orbState (voice session → orb)", () => {
  const voice: OrbVoice = {
    connected: true,
    starting: false,
    upgrading: false,
    busy: false,
    thinking: false,
    requesting: false,
    textOnly: false,
    muted: false,
    capturePaused: false,
    speaking: false,
    error: "",
    unfinished: null,
    review: null,
    messages: [],
  }
  it.each<[Partial<OrbVoice>, OrbState]>([
    [{ connected: false }, "idle"],
    [{ connected: false, messages: [{}] }, "ended"],
    [{ connected: false, starting: true }, "connecting"],
    [{ upgrading: true }, "connecting"],
    [{}, "listening"],
    [{ textOnly: true }, "idle"],
    [{ muted: true }, "idle"],
    [{ capturePaused: true }, "idle"],
    [{ speaking: true }, "speaking"],
    [{ thinking: true }, "thinking"],
    [{ requesting: true }, "thinking"],
    [{ review: {} }, "waiting"],
    [{ review: {}, busy: true }, "executing"],
    [{ busy: true }, "executing"],
    [{ unfinished: "dropped" }, "recovering"],
    [{ unfinished: "stalled", speaking: true }, "recovering"],
    [{ error: "Mic blocked", speaking: true }, "error"],
  ])("%j → %s", (patch, expected) => {
    expect(orbState({ ...voice, ...patch })).toBe(expected)
  })
})

describe("render choices", () => {
  it("runs 60 fps on desktop, 30 on touch and small orbs", () => {
    expect(orbFps(256, false)).toBe(60)
    expect(orbFps(256, true)).toBe(30)
    expect(orbFps(99, false)).toBe(30)
  })
})
