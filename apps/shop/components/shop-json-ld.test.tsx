import { renderToStaticMarkup } from "react-dom/server"
import { expect, it, vi } from "vitest"

vi.mock("next/headers", () => ({
  headers: async () => new Headers({ "x-nonce": "test-nonce" }),
}))

import { ShopJsonLd } from "./shop-json-ld"

it("uses the request nonce and escapes catalog text before rendering", async () => {
  const data = { name: "</script><script>alert(1)</script>" }
  const html = renderToStaticMarkup(await ShopJsonLd({ data }))
  expect(html).toContain('nonce="test-nonce"')
  expect(html).not.toContain("<script>alert")
  const content = html.slice(
    html.indexOf(">") + 1,
    html.lastIndexOf("</script>")
  )
  expect(JSON.parse(content)).toEqual(data)
})
