import { describe, expect, it } from "vitest"
import {
  hostedPaymentUrl,
  providerStatusKey,
  paymentTone,
  providerMoney,
  paymentTimestamp,
  decimalAmount,
  paymentMoney,
  paymentStatusKey,
  paymentUncertain,
} from "./payments"

describe("payment presentation", () => {
  it("allows HTTPS hosted checkout only", () => {
    expect(hostedPaymentUrl("https://pay.example/checkout")).toBeTruthy()
    for (const url of [
      "javascript:alert(1)",
      "//pay.example",
      "http://pay.example",
      "garbage",
      null,
    ])
      expect(hostedPaymentUrl(url)).toBeNull()
  })
  it("keeps unknown outcomes under review", () => {
    expect(paymentUncertain({ status: "unknown", attention: false })).toBe(true)
    expect(paymentUncertain({ status: "pending", attention: true })).toBe(true)
    expect(
      paymentUncertain({ status: "provider_new_state", attention: false })
    ).toBe(true)
    expect(paymentStatusKey("provider_new_state")).toBe("review")
    expect(paymentStatusKey("paid")).toBe("paid")
    expect(paymentStatusKey("creating")).toBe("created")
    expect(paymentStatusKey("canceled")).toBe("cancelled")
  })
  it("formats decimal text without passing through floating point", () => {
    expect(decimalAmount("90071992547409.91", 2, "en")).toBe(
      "90,071,992,547,409.91"
    )
    expect(decimalAmount("12.50000000", 2, "nl")).toBe("12,50")
  })
  it("uses exact minor-unit text beyond the JavaScript integer limit", () => {
    expect(
      paymentMoney(
        {
          amount: 9007199254740992,
          amount_minor: "9007199254740993",
          decimals: 2,
          currency: "EUR",
        },
        "en"
      )
    ).toBe("€90,071,992,547,409.93")
    expect(
      paymentMoney({ amount: 1250, decimals: 2, currency: "EUR" }, "en")
    ).toBe("€12.50")
  })
})

describe("provider evidence presentation", () => {
  it("keeps pending, confirmed and unknown outcomes distinct", () => {
    expect(providerStatusKey("PROCESSED")).toBe("processed")
    expect(providerStatusKey("PENDING")).toBe("pending")
    expect(providerStatusKey("AUTHORIZED")).toBe("authorized")
    expect(providerStatusKey("future_status")).toBe("review")
    expect(providerStatusKey(null)).toBe("review")
    expect(paymentTone("processed")).toBe("teal")
    expect(paymentTone("pending")).toBe("gold")
    expect(paymentTone("authorized")).toBe("gold")
    expect(paymentTone("review")).toBe("gold")
    expect(paymentTone("chargebacked")).toBe("red")
    expect(paymentTone("failed")).toBe("red")
  })
  it("formats provider amounts in major units with exact currency and precision", () => {
    expect(providerMoney("16.04000000", "USD", 2, "en")).toBe("$16.04")
    expect(providerMoney("90071992547409.91", "EUR", 2, "en")).toBe(
      "€90,071,992,547,409.91"
    )
    expect(providerMoney("1.234", "EUR", 2, "en")).toBe("€1.234")
    expect(providerMoney(null, "USD", 2, "en")).toBeNull()
    expect(providerMoney("bad", "USD", 2, "en")).toBeNull()
    expect(providerMoney("16.04", null, 2, "en")).toBeNull()
  })
  it("does not invent a timezone for provider wall-clock timestamps", () => {
    const naive = paymentTimestamp("2026-11-18T07:52:57.163795", "en")!
    expect(naive.providerTime).toBe(true)
    expect(naive.text).toContain("7:52")
    expect(naive.text).not.toContain("UTC")
    expect(paymentTimestamp("2026-11-18T07:52:57Z", "en")?.providerTime).toBe(
      false
    )
    expect(
      paymentTimestamp("2026-11-18T07:52:57+02:00", "en")?.providerTime
    ).toBe(false)
    expect(paymentTimestamp("2026-11-18", "en")?.text).toBe("Nov 18, 2026")
    expect(paymentTimestamp("not-a-date", "en")).toBeNull()
    expect(paymentTimestamp(null, "en")).toBeNull()
  })
})
