import { createHash } from "node:crypto"
import { describe, expect, it } from "vitest"
import { prepareAction } from "./catalog"
import { serializeMultipart } from "./multipart"

const id = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
const boundary = "genesy-voice-reviewed-boundary-1234"
const pdf = () =>
  new File(["%PDF-1.7\nreference"], "reference.pdf", {
    type: "application/pdf",
  })
const photo = () =>
  new File([new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 1, 2])], "photo.jpg", {
    type: "image/jpeg",
  })
const document = () =>
  prepareAction(
    "farm_documents_create",
    { path: { plot: id }, attachments: { file: [id] } },
    id
  )

describe("reviewed multipart business uploads", () => {
  it("serializes reproducibly and hashes exactly the approved raw body", async () => {
    // One prepared action: its minted retry id stays fixed across serializations.
    const action = document()
    const first = await serializeMultipart(
      action,
      new Map([[id, pdf()]]),
      boundary
    )
    const second = await serializeMultipart(
      action,
      new Map([[id, pdf()]]),
      boundary
    )
    expect(first.bytes).toEqual(second.bytes)
    expect(first.contentType).toBe(`multipart/form-data; boundary=${boundary}`)
    expect(first.reviewMetadata.bodySha256).toBe(
      createHash("sha256").update(first.bytes).digest("hex")
    )
    expect(first.reviewMetadata.files).toMatchObject([
      {
        field: "file",
        handle: id,
        name: "reference.pdf",
        contentType: "application/pdf",
        size: pdf().size,
      },
    ])
    const changed = await serializeMultipart(
      action,
      new Map([
        [
          id,
          new File(["%PDF-1.7\nchanged"], "reference.pdf", {
            type: "application/pdf",
          }),
        ],
      ]),
      boundary
    )
    expect(changed.reviewMetadata.bodySha256).not.toBe(
      first.reviewMetadata.bodySha256
    )
    const parsed = await new Response(first.bytes, {
      headers: { "Content-Type": first.contentType },
    }).formData()
    expect((parsed.get("file") as File).name).toBe("reference.pdf")
    expect(await (parsed.get("file") as File).text()).toBe(await pdf().text())
    expect(JSON.stringify(first.reviewMetadata)).not.toContain("%PDF")
  })

  it("sorts text/file fields and preserves typed values in canonical form", async () => {
    const action = prepareAction(
      "farm_bug_reports_create",
      {
        body: {
          route: "/farm",
          user_agent: "test",
          viewport: "375x800",
          description: "Problem",
          actions: "[]",
        },
        attachments: { screenshot: [id], photo: ["second"] },
      },
      id
    )
    const a = await serializeMultipart(
      action,
      new Map([
        ["second", photo()],
        [id, photo()],
      ]),
      boundary
    )
    const b = await serializeMultipart(
      {
        ...action,
        body: {
          id: (action.body as { id: string }).id,
          actions: "[]",
          description: "Problem",
          route: "/farm",
          viewport: "375x800",
          user_agent: "test",
        },
        attachments: { photo: ["second"], screenshot: [id] },
      },
      new Map([
        [id, photo()],
        ["second", photo()],
      ]),
      boundary
    )
    expect(a.bytes).toEqual(b.bytes)
    expect(a.reviewMetadata.files.map((file) => file.field)).toEqual([
      "photo",
      "screenshot",
    ])
  })

  it("requires exactly the bound session handles and known file fields", async () => {
    await expect(
      serializeMultipart(document(), new Map(), boundary)
    ).rejects.toThrow()
    await expect(
      serializeMultipart(
        document(),
        new Map([
          [id, pdf()],
          ["extra", pdf()],
        ]),
        boundary
      )
    ).rejects.toThrow()
    await expect(
      serializeMultipart(
        { ...document(), attachments: { wrong: [id] } },
        new Map([[id, pdf()]]),
        boundary
      )
    ).rejects.toThrow()
    await expect(
      serializeMultipart(
        { ...document(), body: { file: "bytes" } },
        new Map([[id, pdf()]]),
        boundary
      )
    ).rejects.toThrow()
    await expect(
      serializeMultipart(
        { ...document(), attachments: { file: ["https://elsewhere/file"] } },
        new Map([["https://elsewhere/file", pdf()]]),
        boundary
      )
    ).rejects.toThrow()
    await expect(
      serializeMultipart(
        { ...document(), operationId: "orders_create" },
        new Map([[id, pdf()]]),
        boundary
      )
    ).rejects.toThrow()
  })

  it("rejects unsafe filenames, MIME claims, formats and boundary collisions", async () => {
    await expect(
      serializeMultipart(
        document(),
        new Map([
          [
            id,
            new File(["%PDF-1.7\n"], "bad\r\nname.pdf", {
              type: "application/pdf",
            }),
          ],
        ]),
        boundary
      )
    ).rejects.toThrow()
    await expect(
      serializeMultipart(
        document(),
        new Map([
          [id, new File(["%PDF-1.7\n"], "scan.pdf", { type: "image/png" })],
        ]),
        boundary
      )
    ).rejects.toThrow()
    await expect(
      serializeMultipart(
        document(),
        new Map([
          [id, new File(["<svg/>"], "image.svg", { type: "image/svg+xml" })],
        ]),
        boundary
      )
    ).rejects.toThrow()
    await expect(
      serializeMultipart(document(), new Map([[id, pdf()]]), "short")
    ).rejects.toThrow()
    await expect(
      serializeMultipart(
        document(),
        new Map([
          [
            id,
            new File([`%PDF-1.7\r\n--${boundary}\r\n`], "scan.pdf", {
              type: "application/pdf",
            }),
          ],
        ]),
        boundary
      )
    ).rejects.toThrow()
  })

  it("enforces actual farm limits before buffering large files", async () => {
    const huge = new File([new Uint8Array(10 * 1024 * 1024 + 1)], "huge.pdf", {
      type: "application/pdf",
    })
    await expect(
      serializeMultipart(document(), new Map([[id, huge]]), boundary)
    ).rejects.toThrow(/10 MiB/)
    const bug = prepareAction(
      "farm_bug_reports_create",
      {
        body: {
          route: "/",
          user_agent: "test",
          viewport: "375",
          description: "x",
          actions: "[]",
        },
        attachments: { screenshot: [id] },
      },
      id
    )
    await expect(
      serializeMultipart(
        bug,
        new Map([
          [
            id,
            new File([new Uint8Array(5 * 1024 * 1024 + 1)], "huge.jpg", {
              type: "image/jpeg",
            }),
          ],
        ]),
        boundary
      )
    ).rejects.toThrow(/5 MiB/)
  })

  it("supports business file uploads without exposing base64 to model", async () => {
    const action = prepareAction(
      "files_create",
      {
        body: { public: false, document_name: "reference" },
        attachments: { upload: [id] },
      },
      id
    )
    const result = await serializeMultipart(
      action,
      new Map([[id, pdf()]]),
      boundary
    )
    const form = await new Response(result.bytes, {
      headers: { "Content-Type": result.contentType },
    }).formData()
    expect(form.get("public")).toBe("false")
    expect(form.get("document_name")).toBe("reference")
    expect((form.get("upload") as File).name).toBe("reference.pdf")
  })
})
