import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import type { VoiceReview } from "../lib/client-contract"
import { VoiceContext, type VoiceContextValue } from "./provider"
import { ReviewButtons, ReviewDetails } from "./review-details"
import { stubVoice } from "./stub-voice"

vi.mock("next-intl", () => ({
  useTranslations: () =>
    Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))

const html = (node: React.ReactNode, voice: Partial<VoiceContextValue> = {}) =>
  renderToStaticMarkup(
    <VoiceContext.Provider value={stubVoice(voice)}>
      {node}
    </VoiceContext.Provider>
  )

describe("ReviewDetails links", () => {
  it("keeps Office's record and https links by default", () => {
    expect(html(<ReviewDetails value="/orders/7" />)).toContain(
      'href="/orders/7"'
    )
    const out = html(<ReviewDetails value="https://docs.test/x" />)
    expect(out).toContain('href="https://docs.test/x"')
    expect(out).toContain('target="_blank"')
  })

  it("sends record links through the host's resolveHref, in a new tab", () => {
    const out = html(<ReviewDetails value="/orders/7" />, {
      resolveHref: (href) => `https://office.test${href}`,
    })
    expect(out).toContain('href="https://office.test/orders/7"')
    expect(out).toContain('target="_blank"')
    expect(out).toContain("openRecord")
  })

  it("shows a refused link as plain text", () => {
    const out = html(<ReviewDetails value="https://evil.test/x" />, {
      resolveHref: () => "",
    })
    expect(out).not.toContain("<a")
    expect(out).toContain("https://evil.test/x")
  })
})

const review = (kind?: "batch") =>
  ({
    ticket: "t",
    summary: "Create order",
    details: {},
    expiresAt: new Date(Date.now() + 60_000).toISOString(),
    ...(kind ? { kind } : {}),
  }) as unknown as VoiceReview

describe("ReviewButtons", () => {
  it("offers cancel and approve while idle", () => {
    const out = html(<ReviewButtons />, { review: review() })
    expect(out).toContain(">cancel<")
    expect(out).toContain(">approve<")
    expect(out).not.toContain(' disabled=""')
  })

  it("disables both while an approval runs, so nothing runs twice", () => {
    const out = html(<ReviewButtons />, { review: review(), busy: true })
    expect(out.match(/ disabled=""/g)).toHaveLength(2)
    expect(out).toContain(">working<")
  })

  it("lets a running batch be stopped", () => {
    const out = html(<ReviewButtons />, { review: review("batch"), busy: true })
    expect(out).toContain(">batchStop<")
    expect(out.match(/ disabled=""/g)).toHaveLength(1)
  })
})
