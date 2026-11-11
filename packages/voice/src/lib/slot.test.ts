import { describe, expect, it } from "vitest"
import { oneSlot } from "./slot"

const tick = () => new Promise((r) => setTimeout(r, 0))

describe("oneSlot: one request at a time, one waiting slot", () => {
  it("a call that arrives while one runs waits, then runs as soon as it settles", async () => {
    const slot = oneSlot()
    const done = slot.begin()
    let turn: string | undefined
    void slot.wait(() => true).then((t) => (turn = t))
    await tick()
    expect(turn).toBeUndefined()
    done()
    await tick()
    expect(turn).toBe("run")
  })

  it("the newest waiting call wins the slot; the older one is superseded at once", async () => {
    const slot = oneSlot()
    const done = slot.begin()
    const turns: string[] = []
    void slot.wait(() => true).then((t) => turns.push(`older:${t}`))
    void slot.wait(() => true).then((t) => turns.push(`newer:${t}`))
    await tick()
    expect(turns).toEqual(["older:superseded"])
    done()
    await tick()
    expect(turns).toEqual(["older:superseded", "newer:run"])
  })

  it("never runs a waiting call while one is still running (writes included)", async () => {
    const slot = oneSlot()
    const running: string[] = []
    const run = async (name: string, work: Promise<void>) => {
      const done = slot.begin()
      running.push(name)
      await work
      running.splice(running.indexOf(name), 1)
      done()
    }
    let finish = () => {}
    void run("write", new Promise<void>((r) => (finish = r)))
    let overlap = false
    void slot.wait(() => true).then(() => {
      overlap = running.length > 0
      void run("read", Promise.resolve())
    })
    await tick()
    finish()
    await tick()
    expect(overlap).toBe(false)
  })

  it("a waiting call is cancelled, never run, when the conversation moved on while it waited", async () => {
    const slot = oneSlot()
    const done = slot.begin()
    let generation = 1
    const mine = generation
    const turn = slot.wait(() => generation === mine)
    generation = 2
    done()
    await expect(turn).resolves.toBe("cancelled")
  })

  it("with nothing running a call goes straight on", async () => {
    await expect(oneSlot().wait(() => true)).resolves.toBe("run")
  })
})
