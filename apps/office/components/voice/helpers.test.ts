import { afterEach, expect, it, vi } from "vitest"
import { progressOf, whenSlow } from "./helpers"

afterEach(() => vi.useRealTimers())

it("advances to the still-working stage only after 800 ms in flight", async () => {
  vi.useFakeTimers()
  const slow = vi.fn()
  let finish = () => {}
  const work = new Promise<void>((resolve) => (finish = resolve))
  const done = whenSlow(work, slow)
  await vi.advanceTimersByTimeAsync(799)
  expect(slow).not.toHaveBeenCalled()
  await vi.advanceTimersByTimeAsync(1)
  expect(slow).toHaveBeenCalledOnce()
  finish()
  await done
})

it("skips the still-working stage when the request finishes quickly", async () => {
  vi.useFakeTimers()
  const slow = vi.fn()
  await whenSlow(Promise.resolve("fast"), slow)
  await vi.advanceTimersByTimeAsync(2000)
  expect(slow).not.toHaveBeenCalled()
})

it("takes status, steps and narration from a server result", () => {
  expect(
    progressOf("checking", {
      status: "requires_confirmation",
      steps: ["Checking: Create order", "Preparing your review"],
      narration: "Please read and approve the card.",
    })
  ).toEqual({
    status: "requires_confirmation",
    steps: ["Checking: Create order", "Preparing your review"],
    narration: "Please read and approve the card.",
  })
  expect(progressOf("running")).toEqual({
    status: "running",
    steps: [],
    narration: "",
  })
})
