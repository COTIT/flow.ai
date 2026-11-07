import { expect, it, vi } from "vitest"
import { takeFilePermit, voiceInput } from "./input"

it("limits transient binary requests and releases a permit exactly once", () => {
  const request = new Request("http://office.test/tools", {
    method: "POST",
    body: new FormData(),
  })
  const first = takeFilePermit(request)
  const second = takeFilePermit(request)
  expect(() => takeFilePermit(request)).toThrow(
    "Two file requests are already being processed."
  )
  first()
  first()
  const third = takeFilePermit(request, true)
  expect(() => takeFilePermit(request)).toThrow()
  second()
  third()
})

it("shares the binary cap across separate route module instances", async () => {
  const request = new Request("http://office.test/tools", {
    method: "POST",
    body: new FormData(),
  })
  const first = takeFilePermit(request)
  const second = takeFilePermit(request)
  try {
    vi.resetModules()
    const anotherRoute = await import("./input")
    expect(() => anotherRoute.takeFilePermit(request)).toThrow(
      "Two file requests are already being processed."
    )
  } finally {
    first()
    second()
  }
})

it("accepts a multipart payload with session attachment handles rather than provider IDs", async () => {
  const data = new FormData()
  data.append(
    "payload",
    JSON.stringify({
      sessionId: "s1",
      arguments: {
        attachments: { file: ["01950000-0000-7000-8000-000000000001"] },
      },
    })
  )
  data.append(
    "01950000-0000-7000-8000-000000000001",
    new Blob(["%PDF-1.7\n"], { type: "application/pdf" }),
    "reference.pdf"
  )
  const result = await voiceInput(
    new Request("http://office.test/tools", { method: "POST", body: data })
  )
  expect(result.files.size).toBe(1)
  expect(result.input.sessionId).toBe("s1")
})

it("rejects duplicate file parts and JSON authority hidden in file fields", async () => {
  const data = new FormData()
  data.append("payload", "{}")
  data.append("provider_file_1", new Blob(["x"]))
  await expect(
    voiceInput(
      new Request("http://office.test/tools", { method: "POST", body: data })
    )
  ).rejects.toThrow()
})
