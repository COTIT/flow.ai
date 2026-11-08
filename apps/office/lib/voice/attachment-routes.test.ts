import { createHash } from "node:crypto"
import { beforeEach, expect, it, vi } from "vitest"
const mocks = vi.hoisted(() => ({
  ledger: vi.fn(),
  provider: vi.fn(),
}))
vi.mock("./server", async (original) => ({
  ...(await original<typeof import("./server")>()),
  authorizeVoice: async () => ({
    token: "t",
    workspaceId: "01950000-0000-7000-8000-000000000009",
    permissions: ["workspace_admin"],
    apiUrl: "https://api.test/v1",
  }),
}))
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  requireAssistantSession: async () => ({ id: "s", conversation_id: "conv" }),
  assistantRequest: mocks.ledger,
}))
vi.mock("./provider", () => ({ providerRequest: mocks.provider }))
vi.mock("./admission", () => ({ requireAssistantAdmission: async () => {} }))
import { POST } from "@/app/api/voice/attachments/route"

const upload = "01950000-0000-7000-8000-0000000000aa"
const send = (bytes: Uint8Array | string, type: string) => {
  const body =
    typeof bytes === "string" ? new TextEncoder().encode(bytes) : bytes
  return POST(
    new Request("http://office.test/api/voice/attachments", {
      method: "POST",
      headers: {
        "Content-Type": type,
        "Content-Length": String(body.length),
        "X-Workspace-Id": "w",
        "X-Voice-Session": "s",
      },
      body: body as Uint8Array<ArrayBuffer>,
    })
  )
}
beforeEach(() => {
  vi.clearAllMocks()
  mocks.ledger.mockImplementation(async (_c, path: string) =>
    path.endsWith("/uploads") ? { id: upload } : {}
  )
  mocks.provider.mockResolvedValue({ file_id: "file_1" })
})

it("keeps a price-list sheet in the session: no provider file, header names returned", async () => {
  const csv = "Product;Price\nRice;12,50\nBeans;3\n"
  const response = await send(csv, "application/vnd.ms-excel")
  expect(await response.json()).toEqual({
    id: upload,
    kind: "sheet",
    status: "uploaded",
    rows: 2,
    columns: ["Product", "Price"],
  })
  expect(mocks.provider).not.toHaveBeenCalled()
  expect(mocks.ledger).toHaveBeenCalledWith(
    expect.anything(),
    "/sessions/s/uploads",
    "POST",
    { media_type: "text/csv", size_bytes: csv.length }
  )
  expect(mocks.ledger).toHaveBeenCalledWith(
    expect.anything(),
    `/sessions/s/uploads/${upload}/complete`,
    "POST",
    {
      size_bytes: csv.length,
      media_type: "text/csv",
      sha256: createHash("sha256").update(csv).digest("hex"),
    },
    true
  )
})

it("refuses old .xls, unreadable and oversized sheets before reserving anything", async () => {
  const xls = new Uint8Array([0xd0, 0xcf, 0x11, 0xe0, 1, 2, 3])
  expect((await send(xls, "application/vnd.ms-excel")).status).toBe(415)
  expect((await send("Product,Price\n", "text/csv")).status).toBe(422)
  const big = await send("a".repeat(2 * 1024 * 1024 + 1), "text/csv")
  expect(big.status).toBe(413)
  expect(mocks.ledger).not.toHaveBeenCalled()
})

it("still sends photos to the conversation", async () => {
  const response = await send(
    new Uint8Array([0xff, 0xd8, 0xff, 0, 1]),
    "image/jpeg"
  )
  expect(await response.json()).toMatchObject({ fileId: "file_1" })
  expect(mocks.provider).toHaveBeenCalledOnce()
})
