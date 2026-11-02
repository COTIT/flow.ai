import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { ApiError } from "./api-error"
import { EmptyState } from "./empty-state"
import { Notice } from "./notice"

const html = (node: React.ReactNode) => renderToStaticMarkup(node)

describe("Notice", () => {
  it.each([
    ["info", "status"],
    ["success", "status"],
    ["warning", "status"],
    ["error", "alert"],
  ] as const)("%s is a %s live region", (variant, role) => {
    const out = html(<Notice variant={variant} title="Heads up" />)
    expect(out).toMatch(new RegExp(`^<div[^>]*role="${role}"`))
    expect(out).toContain(`data-variant="${variant}"`)
  })

  it("never relies on colour alone: an icon (hidden from readers) plus the words", () => {
    const out = html(<Notice variant="warning" title="Sold out" />)
    expect(out).toMatch(/<svg[^>]*aria-hidden="true"/)
    expect(out).toContain("Sold out")
  })

  it("shows an optional body and action", () => {
    const out = html(
      <Notice
        variant="info"
        title="Waiting"
        action={<button>Check again</button>}
      >
        Reserved until 10:30.
      </Notice>
    )
    expect(out).toContain("Reserved until 10:30.")
    expect(out).toContain("<button>Check again</button>")
  })

  it("can sit inside a region the page keeps live itself", () => {
    expect(
      html(<Notice variant="success" title="Paid" live={false} />)
    ).not.toContain("role=")
  })

  it("defaults to info", () => {
    expect(html(<Notice title="Hi" />)).toContain('data-variant="info"')
  })
})

describe("EmptyState", () => {
  it("says what is empty, why, and what to do next", () => {
    const out = html(
      <EmptyState
        title="Your cart is empty"
        description="Add something from the shop."
        action={<a href="/">Browse the shop</a>}
      />
    )
    expect(out).toContain("Your cart is empty")
    expect(out).toContain("Add something from the shop.")
    expect(out).toContain('<a href="/">Browse the shop</a>')
    expect(out).not.toContain("role=")
  })
})

describe("ApiError", () => {
  const failed = (data: unknown) => ({ response: { data } })

  it("is an error Notice with the API's own words", () => {
    const out = html(
      <ApiError
        error={failed({ message: "Name is taken." })}
        fallback="Failed"
      />
    )
    expect(out).toContain('role="alert"')
    expect(out).toContain('data-variant="error"')
    expect(out).toContain("Name is taken.")
  })

  it("uses the fallback when the API sent no message, and renders nothing without an error", () => {
    expect(
      html(<ApiError error={new Error("x")} fallback="Something went wrong." />)
    ).toContain("Something went wrong.")
    expect(html(<ApiError error={null} fallback="x" />)).toBe("")
  })
})
