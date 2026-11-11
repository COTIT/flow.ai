import { describe, expect, it } from "vitest"

import { prepareAction } from "./catalog"
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
      "storefront_design_preview_link_create",
      "storefront_design_preview_link_revoke",
      "storefront_design_preview_links",
      "storefront_design_preview_resolve",
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

describe("a shared reference cannot ride along on a design write", () => {
  const workspace = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
  const revision = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5c"
  const writes = [
    [
      "storefront_design_edit",
      {
        expected_version: 1,
        changes: [{ op: "set_theme", theme: "neutral" }],
      },
    ],
    [
      "storefront_design_publish",
      { expected_version: 1, revision_id: revision },
    ],
    [
      "storefront_design_restore",
      { expected_version: 1, revision_id: revision },
    ],
  ] as const

  it.each(writes)("%s is prepared without attachments", (id, body) => {
    expect(prepareAction(id, { body }, workspace)).toMatchObject({
      operationId: id,
      write: true,
    })
  })

  it.each(writes)("%s refuses a session attachment handle", (id, body) => {
    const args = {
      body,
      attachments: { reference: ["9f1c2b3a-0d4e-4f5a-8b6c-7d8e9f0a1b2c"] },
    }
    expect(() => prepareAction(id, args, workspace)).toThrow(
      "This operation does not accept these attachments."
    )
  })
})
