import { renderToStaticMarkup } from "react-dom/server"
import { beforeEach, describe, expect, it, vi } from "vitest"

import messages from "../messages/en.json"

const actionState = vi.hoisted(() => ({
  problem: undefined as string | undefined,
}))
vi.mock("react", async (real) => ({
  ...(await real<object>()),
  useActionState: () => [{ problem: actionState.problem }, () => {}, false],
}))
vi.mock("@/app/actions", () => ({ payOrder: vi.fn() }))
vi.mock("next-intl", () => ({
  useTranslations: () => (key: string) =>
    key
      .split(".")
      .reduce<unknown>(
        (value, field) => (value as Record<string, unknown>)[field],
        messages.order
      ),
}))

import { PayButton } from "./pay-button"

beforeEach(() => {
  actionState.problem = undefined
})

describe("payment setup guard", () => {
  it.each(["initial", "action"])(
    "disables Pay for %s setup unavailability",
    (source) => {
      if (source === "action") actionState.problem = "setup_required"
      const html = renderToStaticMarkup(
        <PayButton
          orderId="order-1"
          label="Pay $10"
          setupRequired={source === "initial"}
        />
      )
      expect(html).toContain("Online payment is currently unavailable")
      expect(html).toMatch(/<button[^>]*disabled=""/)
      expect(html).not.toContain("Try again")
    }
  )

  it.each(["initial", "action"])(
    "keeps %s review ahead of setup warnings",
    (source) => {
      actionState.problem =
        source === "action" ? "pending_review" : "unavailable"
      const html = renderToStaticMarkup(
        <PayButton
          orderId="order-1"
          label="Pay $10"
          setupRequired
          pendingReview={source === "initial"}
        />
      )
      expect(html).toContain("Payment pending review")
      expect(html).not.toContain("Online payment is currently unavailable")
      expect(html).not.toContain("<form")
      expect(html).not.toContain("<button")
    }
  )
})
