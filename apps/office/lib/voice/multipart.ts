import "server-only"
import { createHash, randomUUID } from "node:crypto"
import { getOperation, prepareAction, type PreparedAction } from "./catalog"

const MiB = 1024 * 1024
const BODY_LIMIT = 10 * MiB + 64 * 1024
const caps: Record<string, number> = {
  files_create: 10 * MiB,
  farm_documents_create: 10 * MiB,
  farm_bug_reports_create: 5 * MiB,
}
const zipTypes = new Set([
  "application/zip",
  "application/x-zip-compressed",
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  "application/vnd.openxmlformats-officedocument.presentationml.presentation",
  "application/vnd.ms-word.document.macroenabled.12",
  "application/vnd.ms-excel.sheet.macroenabled.12",
  "application/vnd.ms-powerpoint.presentation.macroenabled.12",
])
const oleTypes = new Set([
  "application/msword",
  "application/vnd.ms-excel",
  "application/vnd.ms-powerpoint",
])
const digest = (bytes: Uint8Array) =>
  createHash("sha256").update(bytes).digest("hex")

export type MultipartReview = {
  boundary: string
  bodySha256: string
  files: {
    field: string
    handle: string
    name: string
    contentType: string
    size: number
    sha256: string
  }[]
}

function kind(
  bytes: Uint8Array,
  name: string
): { contentType: string; extension: string } {
  const b = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  if (b.subarray(0, 3).equals(Buffer.from([0xff, 0xd8, 0xff])))
    return { contentType: "image/jpeg", extension: "jpg" }
  if (
    b
      .subarray(0, 8)
      .equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))
  )
    return { contentType: "image/png", extension: "png" }
  if (
    b.subarray(0, 4).toString() === "RIFF" &&
    b.subarray(8, 12).toString() === "WEBP"
  )
    return { contentType: "image/webp", extension: "webp" }
  if (b.subarray(0, 5).toString() === "%PDF-")
    return { contentType: "application/pdf", extension: "pdf" }
  if (["GIF87a", "GIF89a"].includes(b.subarray(0, 6).toString()))
    return { contentType: "image/gif", extension: "gif" }
  if (
    b.subarray(4, 8).toString() === "ftyp" &&
    ["heic", "heix", "hevc", "hevx", "mif1", "msf1"].includes(
      b.subarray(8, 12).toString()
    )
  )
    return { contentType: "image/heic", extension: "heic" }
  if (
    b.subarray(0, 4).equals(Buffer.from([0x50, 0x4b, 3, 4])) ||
    b.subarray(0, 4).equals(Buffer.from([0x50, 0x4b, 5, 6]))
  )
    return { contentType: "application/zip", extension: "zip" }
  if (
    b
      .subarray(0, 8)
      .equals(Buffer.from([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]))
  )
    return { contentType: "application/x-ole-storage", extension: "bin" }
  let text: string
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes)
  } catch {
    throw new Error("This file type is not supported.")
  }
  if (
    !text ||
    text.includes("\0") ||
    /^\s*(?:<|#!)/.test(text.replace(/^\uFEFF/, "")) ||
    /<(?:script|svg|html|!doctype|\?xml|iframe|body|img|object|embed|meta|link|style|base|form)/i.test(
      text
    )
  )
    throw new Error("This file type is not supported.")
  return name.toLowerCase().endsWith(".csv")
    ? { contentType: "text/csv", extension: "csv" }
    : { contentType: "text/plain", extension: "txt" }
}

function filename(blob: Blob): string {
  const name =
    "name" in blob && typeof blob.name === "string" ? blob.name : "upload"
  if (
    !name ||
    Array.from(name).length > 255 ||
    Array.from(name).some(
      (char) =>
        char.charCodeAt(0) < 32 ||
        (char.charCodeAt(0) >= 127 && char.charCodeAt(0) <= 159)
    ) ||
    /[\u200b-\u200f\u2028-\u202e\u2060-\u2069\ufeff/\\"]/.test(name) ||
    [".", ".."].includes(name)
  )
    throw new Error("The file name is not safe for upload.")
  return name
}

/** Original bytes stay in memory; review stores hashes and immutable multipart boundary only. */
export async function serializeMultipart(
  action: PreparedAction,
  files: Map<string, Blob>,
  boundary = `genesy-voice-${randomUUID()}`
): Promise<{
  bytes: Uint8Array<ArrayBuffer>
  contentType: string
  reviewMetadata: MultipartReview
}> {
  const maximum = caps[action.operationId]
  const tool = getOperation(action.operationId)
  if (
    !maximum ||
    !tool ||
    tool.disposition !== "execute" ||
    !tool.requestMedia["multipart/form-data"] ||
    action.method !== tool.method
  )
    throw new Error("This operation does not support business file uploads.")
  if (!/^[a-zA-Z0-9_-]{16,70}$/.test(boundary))
    throw new Error("Invalid multipart review boundary.")
  const names = [...tool.path.matchAll(/\{([^}]+)\}/g)].map(
    (match) => match[1]!
  )
  const template = tool.path
    .replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
    .replace(/\\\{[^}]+\\\}/g, "([^/]+)")
  const match = new RegExp(`^${template}$`).exec(action.path)
  if (!match) throw new Error("The upload path does not match its operation.")
  const parameters = Object.fromEntries(
    names.map((name, i) => [name, match[i + 1]!])
  )
  const { workspace, ...path } = parameters
  if (!workspace) throw new Error("The upload workspace is missing.")
  const checked = prepareAction(
    action.operationId,
    { path, body: action.body, attachments: action.attachments },
    workspace
  )
  const handles = new Set(Object.values(checked.attachments ?? {}).flat())
  if (
    !handles.size ||
    files.size !== handles.size ||
    [...files.keys()].some((handle) => !handles.has(handle))
  )
    throw new Error("Supply exactly the session files bound to this action.")
  const chunks: Uint8Array[] = []
  let size = 0
  const append = (value: string | Uint8Array) => {
    const bytes = typeof value === "string" ? Buffer.from(value, "utf8") : value
    size += bytes.byteLength
    if (size > BODY_LIMIT)
      throw new Error("The multipart upload exceeds 10 MiB plus form overhead.")
    chunks.push(bytes)
  }
  for (const [field, value] of Object.entries(
    checked.body as Record<string, unknown>
  ).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) {
    if (value === null || value === undefined) continue
    if (!["string", "number", "boolean"].includes(typeof value))
      throw new Error(`Multipart text field ${field} must be scalar.`)
    const text = String(value)
    if (
      text.includes(boundary) ||
      Buffer.byteLength(text) >
        (action.operationId === "files_create" ? 2048 : 64 * 1024)
    )
      throw new Error(
        "A multipart text field is too large or conflicts with its boundary."
      )
    append(
      `--${boundary}\r\nContent-Disposition: form-data; name="${field}"\r\n\r\n${text}\r\n`
    )
  }
  const reviewMetadata: MultipartReview = {
    boundary,
    bodySha256: "",
    files: [],
  }
  for (const [field, ids] of Object.entries(checked.attachments ?? {}).sort(
    ([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)
  )) {
    const handle = ids[0]!
    const blob = files.get(handle)
    if (!(blob instanceof Blob) || blob.size === 0)
      throw new Error(`The original file for ${field} is missing or empty.`)
    if (blob.size > maximum)
      throw new Error(`Each file must be at most ${maximum / MiB} MiB.`)
    const name = filename(blob)
    const bytes = new Uint8Array(await blob.arrayBuffer())
    const detected = kind(bytes, name)
    const claim = blob.type.split(";")[0]!.trim().toLowerCase()
    const compatible =
      !claim ||
      claim === "application/octet-stream" ||
      claim === detected.contentType ||
      (detected.contentType === "application/zip" && zipTypes.has(claim)) ||
      (detected.contentType === "application/x-ole-storage" &&
        oleTypes.has(claim))
    if (!compatible)
      throw new Error("The file's MIME type does not match its bytes.")
    const farmType = [
      "image/jpeg",
      "image/png",
      "image/webp",
      "image/heic",
      "application/pdf",
    ].includes(detected.contentType)
    if (
      action.operationId !== "files_create" &&
      (!farmType ||
        (action.operationId === "farm_bug_reports_create" &&
          detected.contentType === "application/pdf"))
    )
      throw new Error(
        "This farm upload requires a supported photo or document."
      )
    if (
      action.operationId === "files_create" &&
      detected.contentType === "image/heic"
    )
      throw new Error("HEIC is not supported by workspace files.")
    if (
      name.includes(boundary) ||
      Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength).includes(
        boundary
      )
    )
      throw new Error(
        "The file conflicts with the approved multipart boundary."
      )
    const contentType =
      detected.contentType === "application/x-ole-storage"
        ? "application/octet-stream"
        : detected.contentType
    const safeName = name === "upload" ? `upload.${detected.extension}` : name
    append(
      `--${boundary}\r\nContent-Disposition: form-data; name="${field}"; filename="${safeName}"\r\nContent-Type: ${contentType}\r\n\r\n`
    )
    append(bytes)
    append("\r\n")
    reviewMetadata.files.push({
      field,
      handle,
      name: safeName,
      contentType,
      size: bytes.byteLength,
      sha256: digest(bytes),
    })
  }
  append(`--${boundary}--\r\n`)
  const bytes = new Uint8Array(size)
  let offset = 0
  for (const chunk of chunks) {
    bytes.set(chunk, offset)
    offset += chunk.byteLength
  }
  reviewMetadata.bodySha256 = digest(bytes)
  return {
    bytes,
    contentType: `multipart/form-data; boundary=${boundary}`,
    reviewMetadata,
  }
}
