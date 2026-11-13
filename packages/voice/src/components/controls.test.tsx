import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import { VoiceControls } from "./controls"
import { VoiceContext } from "./provider"
import { stubVoice } from "./stub-voice"

vi.mock("next-intl", () => ({
  useTranslations: () => Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))

const html = (node: React.ReactNode, historyEnabled = false) =>
  renderToStaticMarkup(
    <VoiceContext.Provider value={stubVoice({ historyEnabled })}>
      {node}
    </VoiceContext.Provider>
  )

describe("VoiceControls open link", () => {
  it("opens the Office assistant page by default", () => {
    expect(html(<VoiceControls compact />)).toContain('href="/voice"')
  })

  it("names the opt-in transport action End call, distinct from ending history", () => {
    expect(html(<VoiceControls />, true)).toContain('aria-label="endCall"')
    expect(html(<VoiceControls />)).toContain('aria-label="end"')
  })

  it("opens the host's assistant page when given", () => {
    const out = html(<VoiceControls compact openHref="/assistant" />)
    expect(out).toContain('href="/assistant"')
    expect(out).not.toContain('href="/voice"')
  })
})
