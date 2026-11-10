import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import { VoiceComposer } from "./composer"
import {
  VoiceContext,
  type Attachment,
  type VoiceContextValue,
} from "./provider"
import { stubVoice } from "./stub-voice"

vi.mock("next-intl", () => ({
  useTranslations: () =>
    Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))

const html = (voice: Partial<VoiceContextValue>) =>
  renderToStaticMarkup(
    <VoiceContext.Provider value={stubVoice(voice)}>
      <VoiceComposer onPreview={() => undefined} />
    </VoiceContext.Provider>
  )

const uploaded: Attachment = {
  key: "k",
  ordinal: 1,
  name: "photo.png",
  file: new File(["x"], "photo.png", { type: "image/png" }),
  state: "uploaded",
}

describe("VoiceComposer", () => {
  it("offers the message box, file picker, camera and file hint in a text session", () => {
    const out = html({ textOnly: true })
    expect(out).toContain('class="voice-composer')
    expect(out).toContain("<textarea")
    expect(out).toContain('aria-label="chooseFiles"')
    expect(out).toContain("takePhoto")
    expect(out).toContain('id="voice-file-hint"')
  })

  it("in a voice session sends ready files with a button instead of the box", () => {
    const out = html({ textOnly: false, attachments: [uploaded] })
    expect(out).not.toContain("<textarea")
    expect(out).toContain(">send<")
  })

  it("says when the microphone is paused", () => {
    expect(html({ textOnly: false, capturePaused: true })).toContain(
      "capturePaused"
    )
  })
})
