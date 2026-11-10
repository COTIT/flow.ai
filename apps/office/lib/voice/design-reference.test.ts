import { describe, expect, it } from "vitest"

import catalog from "./catalog.generated.json"

// Phase 4A: a shared screenshot/PDF may only inform a typed design edit. No design operation may
// accept a file, a URL or encoded bytes, so a reference can never become storefront content.

type Op = { id: string; requestMedia?: object | null }

/** Paths in a request schema that could carry a file, a link or encoded bytes. */
function mediaPaths(schema: unknown, path = "$"): string[] {
  if (Array.isArray(schema))
    return schema.flatMap((item, i) => mediaPaths(item, `${path}[${i}]`))
  if (!schema || typeof schema !== "object") return []
  const node = schema as Record<string, unknown>
  const own =
    ["binary", "byte", "uri", "url", "uri-reference"].includes(
      String(node.format)
    ) ||
    "contentEncoding" in node ||
    "contentMediaType" in node
  return [
    ...(own ? [path] : []),
    ...Object.entries(node).flatMap(([key, value]) =>
      mediaPaths(value, `${path}.${key}`)
    ),
  ]
}

const design = (catalog as Op[]).filter((op) =>
  op.id.startsWith("storefront_design_")
)

describe("design references cannot become storefront content", () => {
  it("finds a file or link field when one exists", () => {
    const upload = {
      type: "object",
      properties: {
        photo: { type: "string", format: "binary" },
        site: { type: "string", format: "uri" },
        data: { type: "string", contentEncoding: "base64" },
      },
    }
    expect(mediaPaths(upload)).toEqual([
      "$.properties.photo",
      "$.properties.site",
      "$.properties.data",
    ])
    const files = (catalog as Op[]).find((op) => op.id === "files_create")
    expect(mediaPaths(files?.requestMedia)).not.toEqual([])
  })

  it("covers every storefront design operation", () => {
    expect(design.map((op) => op.id).sort()).toEqual([
      "storefront_design_capabilities",
      "storefront_design_edit",
      "storefront_design_edit_preview",
      "storefront_design_get",
      "storefront_design_preview",
      "storefront_design_publish",
      "storefront_design_published",
      "storefront_design_restore",
      "storefront_design_revisions",
      "storefront_design_save",
    ])
  })

  it.each(design.map((op) => [op.id, op] as const))(
    "%s takes JSON only, with no file, link or encoded field",
    (_id, op) => {
      const media = op.requestMedia ?? {}
      expect(
        Object.keys(media).filter((type) => type !== "application/json")
      ).toEqual([])
      expect(mediaPaths(media)).toEqual([])
    }
  )
})
