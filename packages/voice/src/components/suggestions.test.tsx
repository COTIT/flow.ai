import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import type { Available } from "../lib/capabilities"
import { useVoiceTips } from "./suggestions"

vi.mock("next-intl", () => ({
  useTranslations: () => Object.assign((key: string) => key, { has: () => false }),
}))
vi.mock("./provider", () => ({
  useVoice: () => ({ connected: false, starting: false, textOnly: false, busy: false, messages: [], outcome: null }),
}))

const cap = (id: string, right: string): Available => ({
  id,
  module: "orders",
  priority: 1,
  rights: { allOf: [right], anyOf: [] },
}) as Available

const capabilities = [cap("todays_orders", "order.list"), cap("weekly_sales", "analytics.read")]

function Chips({ rights }: { rights: { read: boolean; may: (right: string) => boolean } }) {
  const { chips } = useVoiceTips(capabilities, undefined, rights)
  return <>{chips.map((c) => c.id).join(",")}</>
}

describe("useVoiceTips rights", () => {
  it("offers nothing until the host's rights are read", () => {
    expect(renderToStaticMarkup(<Chips rights={{ read: false, may: () => true }} />)).toBe("")
  })

  it("offers only what the host's rights allow", () => {
    const out = renderToStaticMarkup(<Chips rights={{ read: true, may: (r) => r === "order.list" }} />)
    expect(out).toBe("todays_orders")
  })
})
