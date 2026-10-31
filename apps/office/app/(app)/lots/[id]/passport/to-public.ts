import type {
  FileResource,
  Passport,
  PublicBlock,
  PublicPassport,
} from "@genesy/api-client/model"

/**
 * The draft in the public page's shape, for the preview only. File ids become the
 * signed links `files_list` already gave us (null when the file isn't in that list).
 */
export function toPublic(
  draft: Passport,
  files: FileResource[],
  version: number,
  publishedAt: string
): PublicPassport {
  const link = (id: string | null | undefined) => {
    const file = id ? files.find((f) => f.id === id) : undefined
    // The list's link is fresh; its exact expiry doesn't matter for a preview.
    return file
      ? { url: file.file_path, expires_at: file.last_updated_at }
      : null
  }
  return {
    title: draft.title ?? null,
    intro: draft.intro ?? null,
    banner: link(draft.banner_file_id),
    origin: draft.origin ?? null,
    map: draft.map ?? null,
    materials: draft.materials ?? [],
    certificates: (draft.certificates ?? []).map((c) => ({
      name: c.name,
      issuer: c.issuer ?? null,
      number: c.number ?? null,
      valid_until: c.valid_until ?? null,
      file: link(c.file_id),
    })),
    story: (draft.story ?? []).map((b): PublicBlock =>
      b.kind === "image"
        ? { kind: "image", caption: b.caption ?? null, image: link(b.file_id) }
        : b.kind === "text"
          ? { kind: "text", title: b.title ?? null, body: b.body }
          : b.kind === "quote"
            ? { kind: "quote", text: b.text, by: b.by ?? null }
            : b
    ),
    version,
    published_at: publishedAt,
  }
}

/** "62.5" → 6250 basis points, by string, no floats; null when not a 0–100 percent. */
export function percentToBp(value: string): number | null {
  const m = /^(\d{1,3})(?:[.,](\d{1,2}))?$/.exec(value.trim())
  if (!m) return null
  const bp = Number(m[1]) * 100 + Number((m[2] ?? "").padEnd(2, "0"))
  return bp <= 10000 ? bp : null
}

/** 6250 → "62.5": the editor shows what was stored. */
export function bpToPercent(bp: number | null | undefined): string {
  if (bp == null) return ""
  const frac = String(bp % 100)
    .padStart(2, "0")
    .replace(/0+$/, "")
  return frac ? `${Math.floor(bp / 100)}.${frac}` : String(bp / 100)
}
