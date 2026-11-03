import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { useIsMobile } from "./use-mobile"

function Probe() {
  return <i>{String(useIsMobile())}</i>
}

describe("useIsMobile", () => {
  it("is false on the server, before the screen is known", () => {
    expect(renderToStaticMarkup(<Probe />)).toBe("<i>false</i>")
  })
})
