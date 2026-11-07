import { describe, expect, it } from "vitest"
import { invoiceDeliveryNotice } from "./invoice-delivery"

const receipt = (status: unknown) => ({
  message: "ok",
  delivery: {
    status,
    invoice_id: "inv-1",
    invoice_delivery_id: "delivery-1",
    send_log_id: null,
    pdf_file_id: null,
  },
})

describe("invoice delivery notices", () => {
  it.each(["queued", "accepted", "manual", "blocked", "failed", "unknown"])(
    "reports %s without claiming delivery",
    (status) => {
      expect(invoiceDeliveryNotice(receipt(status), "inv-1")).toEqual({
        key: `invoices.delivery.${status}`,
        variant: ["queued", "accepted", "manual"].includes(status)
          ? "info"
          : "warning",
      })
    }
  )

  it.each([
    undefined,
    null,
    { message: "ok" },
    receipt("sent"),
    receipt(null),
    { ...receipt("accepted"), message: "error" },
    { message: "ok", delivery: { status: "accepted" } },
  ])("treats missing or invalid evidence as uncertain", (body) => {
    expect(invoiceDeliveryNotice(body, "inv-1")).toEqual({
      key: "invoices.delivery.uncertain",
      variant: "warning",
    })
  })

  it("rejects another invoice's receipt", () => {
    expect(invoiceDeliveryNotice(receipt("accepted"), "inv-other").key).toBe(
      "invoices.delivery.uncertain"
    )
  })
})
