import { describe, expect, it } from "vitest"
import { batchAttachmentReferences, voiceRequestBody } from "./multipart"

describe("original files for approved business actions", () => {
  it("collects only exact batch attachment roles and sends a shared original once", () => {
    const calls = [
      { arguments: { attachments: { upload: ["app-file"] } } },
      {
        arguments: {
          attachments: { upload: ["app-file"], evidence: ["other"] },
        },
      },
      { arguments: { body: { upload: "never-a-file-handle" } } },
    ]
    const references = batchAttachmentReferences(calls)
    const request = voiceRequestBody(
      { calls },
      references,
      new Map([
        ["app-file", new Blob(["one"])],
        ["other", new Blob(["two"])],
      ])
    )
    expect(Array.from((request.body as FormData).keys())).toEqual([
      "payload",
      "app-file",
      "other",
    ])
    expect(
      batchAttachmentReferences([{ arguments: { body: {} } }])
    ).toBeUndefined()
    expect(() =>
      batchAttachmentReferences([
        { arguments: { attachments: { upload: "app-file" } } },
      ])
    ).toThrow("invalid_references")
  })
  it("sends JSON for ordinary actions and original bytes only for named handles", async () => {
    const payload = {
      sessionId: "session-one",
      arguments: { attachments: { file: ["app-file"] } },
    }
    const originals = new Map<string, Blob>([
      [
        "app-file",
        new File(["original bytes"], "receipt.txt", { type: "text/plain" }),
      ],
    ])
    const request = voiceRequestBody(
      payload,
      payload.arguments.attachments,
      originals
    )
    expect(request.body).toBeInstanceOf(FormData)
    const body = request.body as FormData
    expect(JSON.parse(String(body.get("payload")))).toEqual(payload)
    expect(await (body.get("app-file") as File).text()).toBe("original bytes")
    expect((body.get("app-file") as File).name).toBe("receipt.txt")
    expect(body.get("provider-file")).toBeNull()
    expect(
      voiceRequestBody({ decision: "cancel" }, undefined, originals).body
    ).toBe('{"decision":"cancel"}')
  })
  it("refuses missing original data and malformed attachment roles before any request", () => {
    expect(() =>
      voiceRequestBody({}, { file: ["provider-file"] }, new Map())
    ).toThrow("missing_reference")
    expect(() => voiceRequestBody({}, { file: "app-file" }, new Map())).toThrow(
      "invalid_references"
    )
  })
})
