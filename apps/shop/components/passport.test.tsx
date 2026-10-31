import { renderToStaticMarkup } from "react-dom/server"
import { createTranslator } from "next-intl"
import { describe, expect, it } from "vitest"

import type { PublicLot } from "@genesy/api-client/model"

import en from "../messages/en.json"
import { Passport } from "./passport"

const t = createTranslator({ locale: "en", messages: en, namespace: "batch" })
const link = (name: string) => ({
  url: `https://files.example.com/${name}`,
  expires_at: "2026-10-31T10:10:00Z",
})

const lot = (over: Partial<PublicLot> = {}): PublicLot => ({
  code: "L-14",
  public_code: "7K3M9QX2WD",
  status: "released",
  produced_on: "2026-07-14",
  best_before: "2028-07-14",
  expires_on: null,
  origin: "Ilundo",
  product: { name: "Ilundo honey", brand: "Ilundo", short_description: null },
  farmers: [],
  passport: {
    title: "Wild forest honey",
    intro: "One harvest of log hives.",
    banner: link("banner"),
    origin: { country: "TZ", region: "Iringa", place: "Ilundo" },
    map: { lat: -7.77, lng: 35.69, label: "Ilundo forest" },
    materials: [{ name: "Raw honey", origin: "Tanzania", share_bp: 10000 }],
    certificates: [
      {
        name: "Organic",
        issuer: "Ecocert",
        number: "X1",
        valid_until: "2027-12-31",
        file: link("cert"),
      },
    ],
    story: [
      { kind: "text", title: "The hives", body: "High in the trees." },
      { kind: "image", image: link("story"), caption: "Hives" },
      { kind: "facts", items: [{ label: "Hives", value: "312" }] },
      { kind: "quote", text: "The bees know the forest.", by: "A beekeeper" },
    ],
    version: 3,
    published_at: "2026-08-01T09:00:00Z",
  },
  ...over,
})

const render = (l: PublicLot) =>
  renderToStaticMarkup(<Passport lot={l} locale="en" t={t} />)

describe("Passport", () => {
  it("shows title, intro, origin, materials, certificates and every story block", () => {
    const html = render(lot())
    for (const text of [
      "Wild forest honey",
      "One harvest of log hives.",
      "Tanzania",
      "Raw honey",
      "100%",
      "Organic",
      "Issued by Ecocert",
      "The hives",
      "High in the trees.",
      "https://files.example.com/story",
      "312",
      "The bees know the forest.",
      "July 14, 2026",
    ])
      expect(html).toContain(text)
  })

  it("hides Meet the farmers while the list is empty", () => {
    expect(render(lot())).not.toContain("Meet the farmers")
  })

  it("shows each consented farmer", () => {
    const html = render(
      lot({
        farmers: [
          {
            first_name: "Asha",
            village: "Ilundo",
            photo: link("asha"),
            harvest_dates: ["2026-07-10"],
          },
        ],
      })
    )
    expect(html).toContain("Meet the farmers")
    expect(html).toContain("Asha")
    expect(html).toContain("July 10, 2026")
  })

  it("announces a recall to screen readers, at the top", () => {
    const html = render(lot({ status: "recalled" }))
    const alert = html.indexOf('role="alert"')
    expect(alert).toBeGreaterThan(-1)
    expect(html.indexOf("has been recalled")).toBeGreaterThan(alert)
    expect(alert).toBeLessThan(html.indexOf("Wild forest honey"))
    expect(render(lot())).not.toContain('role="alert"')
  })

  it("shows the map as an area with an outbound link, never a pin", () => {
    const html = render(lot())
    expect(html).toContain("openstreetmap.org/#map=11/-7.77/35.69")
    expect(html).not.toContain("mlat")
    expect(html).toContain('target="_blank"')
    expect(html).toContain('rel="noopener noreferrer"')
  })

  it("leaves out sections the passport doesn't fill", () => {
    const empty = lot()
    empty.passport = {
      ...empty.passport,
      banner: null,
      intro: null,
      map: null,
      origin: null,
      materials: [],
      certificates: [],
      story: [],
    }
    const html = render(empty)
    expect(html).not.toContain("What&#x27;s inside")
    expect(html).not.toContain("Certificates")
    expect(html).not.toContain("The story")
    expect(html).not.toContain("openstreetmap")
    expect(html).not.toContain("<img")
  })
})
