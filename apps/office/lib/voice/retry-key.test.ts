import { describe, expect, it } from "vitest"
import { prepareAction } from "./catalog"
import { restoreAction, type StoredAction } from "./actions"
import { createReview, verifyReview } from "./workflow"
import { serializeMultipart } from "./multipart"

const workspace = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/
const supply = {
  village_id: "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a01",
  crop_id: "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a02",
  code: "ILU-H-2026-01",
  raw_product_id: "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a03",
  currency_id: "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a04",
  price_per_kg: 800000,
  lines: [
    { harvest_report_id: "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a05", kg: "250.00" },
  ],
}
const prepare = (body: Record<string, unknown>) =>
  prepareAction("farm_supply_create", { body }, workspace)
const ref = (action: { body?: unknown }) =>
  (action.body as Record<string, unknown>).client_ref

describe("retry key for client-reference writes", () => {
  it("fills a missing client_ref with a fresh uuid", () => {
    expect(ref(prepare(supply))).toMatch(uuid)
  })

  it("keeps a client_ref the agent supplied", () => {
    const given = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4aff"
    expect(ref(prepare({ ...supply, client_ref: given }))).toBe(given)
  })

  it("gives two separate requests different keys", () => {
    expect(ref(prepare(supply))).not.toBe(ref(prepare(supply)))
  })

  it("keeps the same key through approval, dispatch and restore", () => {
    const action = prepare(supply)
    const context = {
      token: "staff",
      workspaceId: workspace,
      permissions: ["farm:ops"],
      apiUrl: "https://api.test/v1",
      sessionId: "s1",
    }
    const ticket = createReview(action, context, "secret", "s1", "a1")
    // Dispatch runs the action carried by the verified ticket.
    expect(ref(verifyReview(ticket, context, "secret").action)).toBe(
      ref(action)
    )
    const restored = restoreAction({
      operation_id: "farm_supply_create",
      proposal: { path: action.path, body: action.body },
    } as unknown as StoredAction)
    expect(ref(restored!)).toBe(ref(action))
  })

  it("mints an upload's key once, so its reviewed hash matches at dispatch", async () => {
    const plot = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a06"
    const action = prepareAction(
      "farm_documents_create",
      { path: { plot }, attachments: { file: [plot] } },
      workspace
    )
    expect((action.body as { id?: unknown }).id).toMatch(uuid)
    const pdf = () =>
      new File(["%PDF-1.7\nx"], "a.pdf", { type: "application/pdf" })
    const hash = async () =>
      (
        await serializeMultipart(
          action,
          new Map([[plot, pdf()]]),
          "genesy-voice-reviewed-boundary-1234"
        )
      ).reviewMetadata.bodySha256
    expect(await hash()).toBe(await hash())
  })
})
