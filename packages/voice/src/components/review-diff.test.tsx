import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import type { VoiceReview } from "../lib/client-contract"
import { ReviewDiff } from "./review-diff"

vi.mock("next-intl", () => ({
  useTranslations: () =>
    Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))

const review = (changes: unknown[]) =>
  ({
    ticket: "t",
    summary: "Edit storefront design",
    details: { current: { changes } },
    expiresAt: new Date(Date.now() + 60_000).toISOString(),
  }) as unknown as VoiceReview

const rows = (out: string) =>
  [...out.matchAll(/<dt[^>]*>(.*?)<\/dt>/g)].map((m) =>
    (m[1] ?? "").replace(/<[^>]+>/g, "")
  )

describe("ReviewDiff rows", () => {
  it("names a labelled row by its label and its field", () => {
    const out = renderToStaticMarkup(
      <ReviewDiff
        review={review([
          {
            field: "heading",
            label: "Our products",
            before: "Our products",
            after: "Fresh this week",
          },
        ])}
      >
        {null}
      </ReviewDiff>
    )
    expect(rows(out)).toEqual(["Our products · Heading"])
  })

  it("keeps an unlabelled row as its field name alone", () => {
    const out = renderToStaticMarkup(
      <ReviewDiff
        review={review([
          { field: "theme", before: "neutral", after: "forest" },
        ])}
      >
        {null}
      </ReviewDiff>
    )
    expect(rows(out)).toEqual(["Theme"])
  })

  it("shows every row when two sections change the same field", () => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => {})
    const out = renderToStaticMarkup(
      <ReviewDiff
        review={review([
          {
            field: "heading",
            label: "Welcome",
            before: "Hello",
            after: "Karibu",
          },
          {
            field: "heading",
            label: "Our products",
            before: "Our products",
            after: "Fresh this week",
          },
        ])}
      >
        {null}
      </ReviewDiff>
    )
    expect(rows(out)).toEqual(["Welcome · Heading", "Our products · Heading"])
    // React warns about duplicate keys; rows keyed by field alone collide here.
    expect(errors.mock.calls.flat().join(" ")).not.toMatch(/same key/)
    errors.mockRestore()
  })
})
