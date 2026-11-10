import { designs, draft, expect, test } from "./storefront-api"

test.use({
  allow: {
    consolePrefix: ["Framing 'http://127.0.0.1"],
    failures: [
      "/storefront/preview?revision=01950000-0000-7000-8000-000000000002",
    ],
  },
  // The faked parent has no real address, so Chrome's local-network check would refuse the
  // frame before the CSP is consulted; off here so the CSP is what refuses it.
  launchOptions: { args: ["--disable-features=LocalNetworkAccessChecks"] },
})

test("another site cannot frame the preview", async ({
  page,
  api,
  baseURL,
}) => {
  designs(api)
  // Another origin: same machine, other host name.
  const other = baseURL!.replace("127.0.0.1", "localhost") + "/elsewhere"
  await page.route(other, (route) =>
    route.fulfill({
      contentType: "text/html",
      body: `<iframe title="x" src="${baseURL}/storefront/preview?revision=${draft}"></iframe>`,
    })
  )
  const refused = page.waitForEvent("console", (m) =>
    m.text().startsWith("Framing ")
  )
  await page.goto(other)
  expect((await refused).text()).toContain("frame-ancestors 'self'")
  for (const frame of page.frames().slice(1))
    expect(await frame.content().catch(() => "")).not.toContain(
      "From farm to your table"
    )
  expect(api.calls).toHaveLength(0)
})
