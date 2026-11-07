import { expect, it } from "vitest"
import { attachmentType } from "./attachments"

it("recognizes allowed file bytes and rejects a forged declared MIME", () => {
  expect(
    attachmentType(new Uint8Array([0xff, 0xd8, 0xff, 0, 1]), "image/jpeg")
  ).toBe("image/jpeg")
  expect(
    attachmentType(new TextEncoder().encode("%PDF-1.7\n"), "application/pdf")
  ).toBe("application/pdf")
  expect(() =>
    attachmentType(new TextEncoder().encode("<svg>"), "image/png")
  ).toThrow()
  expect(() =>
    attachmentType(new TextEncoder().encode("%PDF-1.7\n"), "image/png")
  ).toThrow()
})

it("does not claim arbitrary RIFF data is WebP", () => {
  expect(() =>
    attachmentType(new TextEncoder().encode("RIFF0000WAVEdata"), "image/webp")
  ).toThrow()
  expect(
    attachmentType(new TextEncoder().encode("RIFF0000WEBPdata"), "image/webp")
  ).toBe("image/webp")
})
