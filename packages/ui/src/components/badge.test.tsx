import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { Badge, badgeVariants } from "./badge"

describe("Badge", () => {
  it.each(["teal", "gold", "red", "slate"] as const)(
    "has the %s tone, marked for the glass look",
    (tone) => {
      const out = renderToStaticMarkup(<Badge variant={tone}>Released</Badge>)
      expect(out).toContain(`data-variant="${tone}"`)
      expect(out).toContain('data-slot="badge"')
      // Outside the glass look a tone still reads as a tint, not the navy default.
      expect(badgeVariants({ variant: tone })).not.toContain("bg-primary")
      expect(badgeVariants({ variant: tone })).toMatch(/\bbg-/)
    }
  )

  it("keeps the existing variants as they were", () => {
    expect(badgeVariants({ variant: "default" })).toContain(
      "bg-primary text-primary-foreground"
    )
    expect(badgeVariants({ variant: "destructive" })).toContain(
      "bg-destructive/10 text-destructive"
    )
  })
})
