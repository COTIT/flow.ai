import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import type { TranscriptRow } from "../lib/helpers"
import { VoiceContext } from "./provider"
import { stubVoice } from "./stub-voice"
import { TranscriptRows } from "./transcript"

vi.mock("next-intl", () => ({
  useTranslations: () =>
    Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))

const html = (
  rows: TranscriptRow[],
  options: {
    resolveHref?: (href: string) => string
    renderResult?: (row: TranscriptRow) => React.ReactNode
  } = {}
) =>
  renderToStaticMarkup(
    <VoiceContext.Provider
      value={stubVoice(
        options.resolveHref ? { resolveHref: options.resolveHref } : {}
      )}
    >
      <TranscriptRows rows={rows} renderResult={options.renderResult} />
    </VoiceContext.Provider>
  )

describe("TranscriptRows", () => {
  it("draws user, agent and action lines with their role for screen readers", () => {
    const out = html([
      { id: "1", role: "user", text: "Show orders" },
      { id: "2", role: "agent", text: "Here they are" },
      { id: "3", role: "action", text: "Looked up orders" },
    ])
    expect(out).toContain('data-role="user"')
    expect(out).toContain('data-role="agent"')
    expect(out).toContain('data-role="action"')
    expect(out).toContain("roles.user")
    expect(out).toContain("Here they are")
  })

  it("opens a record link row in a new tab without opener or referrer", () => {
    const out = html([
      {
        id: "1",
        role: "link",
        text: "Order 7",
        href: "https://office.test/orders/7",
      },
    ])
    expect(out).toContain('href="https://office.test/orders/7"')
    expect(out).toContain('target="_blank"')
    expect(out).toContain('rel="noopener noreferrer"')
  })

  it("keeps agent-text links as Office had them by default", () => {
    const out = html([
      {
        id: "1",
        role: "agent",
        text: "See [the order](/orders/7) or [docs](https://docs.test)",
      },
    ])
    expect(out).toContain('href="/orders/7"')
    expect(out).toContain('href="https://docs.test"')
  })

  it("passes agent-text links through the host's resolveHref; a refused one stays text", () => {
    const out = html(
      [
        {
          id: "1",
          role: "agent",
          text: "See [the order](/orders/7) or [docs](https://evil.test)",
        },
      ],
      {
        resolveHref: (href) =>
          href.startsWith("/orders/") ? `https://office.test${href}` : "",
      }
    )
    expect(out).toContain('href="https://office.test/orders/7"')
    expect(out).not.toContain("evil.test")
    expect(out).toContain("docs")
  })

  it("draws a result row with the shared result card unless the host draws it", () => {
    const row: TranscriptRow = {
      id: "r",
      role: "result",
      text: "",
      views: [{ kind: "trail", events: [] }],
    }
    expect(html([row])).toContain('data-role="result"')
    expect(
      html([row], { renderResult: () => <button>View result</button> })
    ).toContain("View result")
  })
})
