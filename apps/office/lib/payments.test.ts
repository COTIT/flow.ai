import { describe, expect, it } from "vitest"
import {
  hostedPaymentUrl,
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
