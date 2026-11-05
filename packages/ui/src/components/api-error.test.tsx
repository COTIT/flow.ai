import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { ApiError } from "./api-error"

const failed = (message?: string) => ({ response: { data: { message } } })

describe("ApiError", () => {
  it("shows a known code as the caller's sentence", () => {
    const out = renderToStaticMarkup(
      <ApiError
        error={failed("lot_recalled")}
        fallback="Something went wrong"
        codes={{ lot_recalled: "This batch is recalled." }}
      />
    )
    expect(out).toContain("This batch is recalled.")
    expect(out).not.toContain("lot_recalled")
  })

  it("keeps the API's words for an unknown code, and the fallback for none", () => {
    const words = renderToStaticMarkup(
      <ApiError
        error={failed("Not enough stock.")}
        fallback="Oops"
        codes={{}}
      />
    )
    expect(words).toContain("Not enough stock.")
    expect(
      renderToStaticMarkup(<ApiError error={failed()} fallback="Oops" />)
    ).toContain("Oops")
  })
})
