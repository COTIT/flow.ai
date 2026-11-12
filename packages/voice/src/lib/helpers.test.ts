import { afterEach, describe, expect, it, vi } from "vitest"
import { formatPrice, progressOf, SAFE_HREF, splitLinks, spokenText, whenSlow } from "./helpers"

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

it.each([
  ["[happy] I am still here.", "I am still here."],
  ["  [warmly] [excited]  Hallo, ik ben er nog.", "Hallo, ik ben er nog."],
  ["Sawa. [laughs] Nimeelewa.", "Sawa. Nimeelewa."],
  ["Order [ORD-12] has 2 lines [1].", "Order [ORD-12] has 2 lines [1]."],
])("hides voice audio tags from displayed agent text: %s", (raw, shown) => {
  expect(spokenText(raw)).toBe(shown)
})

it("only links Office pages and https by default", () => {
  expect(SAFE_HREF.test("/orders/1")).toBe(true)
  expect(splitLinks("[a](/assistant/x)")).toEqual([{ text: "[a](/assistant/x)" }])
})

it("links what the host's whitelist allows", () => {
  expect(splitLinks("see [a](/assistant/x)", /^\/assistant\//)).toEqual([
    { text: "see " },
    { text: "a", href: "/assistant/x" },
  ])
})

describe("formatPrice", () => {
  it("formats an API major-unit decimal string with its currency, rounding for display only", () => {
    expect(formatPrice("11.6600", 2, "EUR", "en")).toBe("€11.66")
    expect(formatPrice("1.2350", 2, "USD", "en")).toBe("$1.24")
    expect(formatPrice("1500.0000", 0, "TZS", "en")).toMatch(/^TZS\s1,500$/)
  })
})
