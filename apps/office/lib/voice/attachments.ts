import "server-only"
import { VoiceError } from "./server"

export const ATTACHMENT_LIMIT = 10 * 1024 * 1024

/** Identify accepted input only; native provider handles decoding and PDF interpretation. */
export function attachmentType(bytes: Uint8Array, declared: string): string {
  const ascii = new TextDecoder("ascii").decode(bytes.subarray(0, 12))
  let type = ""
  if (bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff)
    type = "image/jpeg"
  else if ([137, 80, 78, 71, 13, 10, 26, 10].every((v, i) => bytes[i] === v))
    type = "image/png"
  else if (ascii.startsWith("RIFF") && ascii.slice(8, 12) === "WEBP")
    type = "image/webp"
  else if (ascii.startsWith("%PDF-")) type = "application/pdf"
  if (
    !type ||
    (declared && declared !== "application/octet-stream" && declared !== type)
  )
    throw new VoiceError(
      "Use a JPEG, PNG, WebP image or PDF with a matching file type.",
      415
    )
  return type
}
