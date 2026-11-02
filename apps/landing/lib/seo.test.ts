import { describe, expect, it } from "vitest"

import { pageMetadata, robotsRules, SITE } from "./seo"

describe("robotsRules", () => {
  it("lets search engines in and names the sitemap", () => {
    expect(robotsRules(false)).toEqual({
      rules: { userAgent: "*", allow: "/" },
      sitemap: `${SITE}/sitemap.xml`,
    })
  })

  it("keeps a staging copy out of search engines", () => {
    expect(robotsRules(true)).toEqual({
      rules: { userAgent: "*", disallow: "/" },
    })
  })
})

describe("pageMetadata", () => {
  it("gives the canonical address and every language", () => {
    const m = pageMetadata("nl", { title: "T", description: "D" }, false)
    expect(m.alternates).toEqual({
      canonical: `${SITE}/nl`,
      languages: {
        en: SITE,
        nl: `${SITE}/nl`,
        sw: `${SITE}/sw`,
        "x-default": SITE,
      },
    })
    expect(m.robots).toBeUndefined()
    expect(m.openGraph).toMatchObject({
      url: `${SITE}/nl`,
      locale: "nl",
      siteName: "Genesy",
    })
  })

  it("adds noindex on staging", () => {
    expect(
      pageMetadata("en", { title: "T", description: "D" }, true).robots
    ).toEqual({
      index: false,
      follow: false,
    })
  })
})
