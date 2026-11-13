import { describe, expect, it } from "vitest"
import { oneSlot } from "./slot"

const tick = () => new Promise((r) => setTimeout(r, 0))

describe("oneSlot: serial FIFO", () => {
  it("retains all three concurrent requests in arrival order", async () => {
    const slot = oneSlot()
    const first = await slot.acquire(() => true)
    const ran: string[] = []
    void slot.acquire(() => true).then((release) => { ran.push("second"); release?.() })
    void slot.acquire(() => true).then((release) => { ran.push("third"); release?.() })
    await tick()
    expect(ran).toEqual([])
    first?.()
    await tick()
    expect(ran).toEqual(["second", "third"])
  })

  it("never overlaps work and releases each reservation only once", async () => {
    const slot = oneSlot()
    const first = await slot.acquire(() => true)
    const second = slot.acquire(() => true)
    first?.()
    const releaseSecond = await second
    let thirdRan = false
    void slot.acquire(() => true).then((release) => { thirdRan = true; release?.() })
    first?.()
    await tick()
    expect(thirdRan).toBe(false)
    releaseSecond?.()
    await tick()
    expect(thirdRan).toBe(true)
  })

  it("cancels old-request waiters, then dispatches the current request", async () => {
    const slot = oneSlot()
    let request = "old"
    const first = await slot.acquire(() => true)
    const old = slot.acquire(() => request === "old")
    const current = slot.acquire(() => request === "new")
    request = "new"
    first?.()
    await expect(old).resolves.toBeNull()
    const release = await current
    expect(release).toBeTypeOf("function")
    release?.()
  })
})
