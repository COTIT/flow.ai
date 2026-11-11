import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import { VoiceControls } from "./controls"
import { VoiceContext } from "./provider"
import { stubVoice } from "./stub-voice"

vi.mock("next-intl", () => ({
  useTranslations: () => Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))

const html = (node: React.ReactNode) =>
  renderToStaticMarkup(<VoiceContext.Provider value={stubVoice()}>{node}</VoiceContext.Provider>)

describe("VoiceControls open link", () => {
  it("opens the Office assistant page by default", () => {
    expect(html(<VoiceControls compact />)).toContain('href="/voice"')
  })

  it("opens the host's assistant page when given", () => {
    const out = html(<VoiceControls compact openHref="/assistant" />)
    expect(out).toContain('href="/assistant"')
    expect(out).not.toContain('href="/voice"')
  })
})
