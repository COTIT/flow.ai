import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import type { ResultView } from "../../lib/views"
import { VoiceContext } from "../provider"
import { stubVoice } from "../stub-voice"
import { VoiceResult } from "./index"

vi.mock("next-intl", () => ({
  useTranslations: () => Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))

const orders: ResultView[] = [
  {
    kind: "records",
    resource: "orders",
    total: 5,
    href: "/orders?status=open",
    rows: [
      { id: "1", label: "Order 1", detail: null, status: "open", money: null, stock: null, href: "/orders/1" },
    ],
  } as ResultView,
]

const html = (node: React.ReactNode, resolveHref?: (href: string) => string) =>
  renderToStaticMarkup(
    <VoiceContext.Provider value={stubVoice(resolveHref ? { resolveHref } : {})}>{node}</VoiceContext.Provider>
  )

describe("VoiceResult links", () => {
  it("keeps server hrefs as they are by default", () => {
    const out = html(<VoiceResult views={orders} />)
    expect(out).toContain('href="/orders/1"')
    expect(out).toContain('href="/orders?status=open"')
  })

  it("passes every server href through the host's resolveHref", () => {
    const out = html(<VoiceResult views={orders} />, (href) => `https://office.test${href}`)
    expect(out).toContain('href="https://office.test/orders/1"')
    expect(out).toContain('href="https://office.test/orders?status=open"')
  })

  it("draws no link where the host refuses the href", () => {
    const out = html(<VoiceResult views={orders} />, () => "")
    expect(out).not.toContain("<a")
    expect(out).toContain("Order 1")
  })
})

describe("VoiceResult status labels", () => {
  it("falls back to the humanized status without a host label", () => {
    expect(html(<VoiceResult views={orders} />)).toContain("Open")
  })

  it("uses the host's statusLabel when given", () => {
    const out = html(
      <VoiceResult views={orders} statusLabel={(domain, status) => `${domain}:${status}`} />
    )
    expect(out).toContain("orders:open")
  })
})
