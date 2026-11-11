import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import { AttachmentList } from "./attachments"
import { VoiceContext, type Attachment } from "./provider"
import { stubVoice } from "./stub-voice"

vi.mock("next-intl", () => ({
  useTranslations: () =>
    Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))

const file = (over: Partial<Attachment>): Attachment => ({
  key: "k",
  ordinal: 1,
  name: "price-list.csv",
  file: new File(["a,b"], "price-list.csv", { type: "text/csv" }),
  state: "selected",
  ...over,
})

const html = (attachments: Attachment[]) =>
  renderToStaticMarkup(
    <VoiceContext.Provider value={stubVoice({ attachments })}>
      <AttachmentList onPreview={() => undefined} />
    </VoiceContext.Provider>
  )

describe("AttachmentList", () => {
  it("renders nothing without files", () => {
    expect(html([])).toBe("")
  })

  it("lists each file with its number, state and a remove button", () => {
    const out = html([file({ state: "uploaded" })])
    expect(out).toContain("1. price-list.csv")
    expect(out).toContain("fileStates.uploaded")
    expect(out).toContain("removeReference")
  })

  it("says why a file failed and offers a retry", () => {
    const out = html([file({ state: "failed", error: "Too large" })])
    expect(out).toContain('role="alert"')
    expect(out).toContain("Too large")
    expect(out).toContain("retryReference")
  })

  it("cannot remove a file the agent already received", () => {
    expect(html([file({ state: "submitted" })])).not.toContain(
      "removeReference"
    )
  })
})
