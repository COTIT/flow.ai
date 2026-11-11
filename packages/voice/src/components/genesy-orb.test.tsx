import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import { GenesyOrb } from "./genesy-orb"
import { VoiceOrb } from "./voice-orb"
import { VoiceContext } from "./provider"
import { stubVoice } from "./stub-voice"

vi.mock("next-intl", () => ({
  useTranslations: () =>
    Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))

describe("GenesyOrb on the server", () => {
  it("draws nothing until the app theme is known, so no wrong palette flashes", () => {
    const html = renderToStaticMarkup(
      <GenesyOrb state="speaking" label="Speaking" size={120} />
    )
    expect(html).toContain('data-genesy-orb=""')
    expect(html).toContain('data-state="speaking"')
    expect(html).toContain("width:120px;height:120px")
    expect(html).not.toContain("<svg")
    expect(html).toMatch(/role="status"[^>]*>Speaking</)
  })

  it("with a given theme renders the still five-strand SVG orb, decorative", () => {
    const html = renderToStaticMarkup(
      <GenesyOrb state="thinking" label="Thinking" theme="light" />
    )
    expect(html).toContain('data-still="true"')
    expect(html.match(/<path /g)).toHaveLength(5)
    expect(html).toContain('aria-hidden="true"')
    expect(html).toContain("#ffffff") // pearl body
    expect(html).not.toContain("data-motion")
  })

  it("keeps the label as live text whatever the state", () => {
    const html = renderToStaticMarkup(
      <GenesyOrb state="error" label="Voice error" theme="dark" reducedMotion />
    )
    expect(html).toMatch(
      /role="status"[^>]*aria-live="polite"[^>]*>Voice error</
    )
  })
})

describe("VoiceOrb", () => {
  it("maps the voice session to the orb state and label", () => {
    const html = renderToStaticMarkup(
      <VoiceContext.Provider
        value={stubVoice({ textOnly: false, speaking: true })}
      >
        <VoiceOrb label={(state) => `orb.${state}`} />
      </VoiceContext.Provider>
    )
    expect(html).toContain('data-state="speaking"')
    expect(html).toContain(">orb.speaking<")
  })
})
