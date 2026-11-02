import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import { island } from "./island"

describe("island", () => {
  it("renders the still version on the server and never fetches the animation", () => {
    const load = vi.fn(async () => ({
      default: ({ to }: { to: number }) => <b>animated {to}</b>,
    }))
    const CountUp = island(load)

    const out = renderToStaticMarkup(<CountUp to={12} plain={<span>12</span>} />)

    expect(out).toBe("<span>12</span>")
    expect(out).not.toContain("data-motion")
    expect(load).not.toHaveBeenCalled()
  })
})
