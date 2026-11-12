import type { Locator, Page } from "@playwright/test"

import { expect } from "../../src/test"
import { sc } from "./scenes-api"

/**
 * Runs a row action the way the person sees it at this width (Lists board): inline Acknowledge and
 * the Snooze ▾ menu (table ≥ 980 px, cards < 600 px), or the ⋯ row menu (600–979 px).
 * `press` clicks the control that writes (e.g. twice in one task, P2-3a); opening a menu stays one click.
 */
export async function rowAction(
  page: Page,
  row: Locator,
  action: "acknowledge" | "tomorrow" | "week",
  press: (control: Locator) => Promise<unknown> = (control) => control.click()
) {
  // Decide only once the row's own controls are drawn.
  await expect(row.getByRole("button").first()).toBeVisible()
  const more = row.getByRole("button", { name: /^Actions for / })
  const ack = row.getByRole("button", {
    name: sc.noticed.acknowledge,
    exact: true,
  })
  // Wait for the row to render in this width's form before picking a path.
  await expect(more.or(ack).filter({ visible: true })).toHaveCount(1)
  if (await more.isVisible()) {
    await more.click()
    const label = {
      acknowledge: sc.noticed.acknowledge,
      tomorrow: sc.noticed.snoozeTomorrow,
      week: sc.noticed.snoozeWeek,
    }[action]
    await press(page.getByRole("menuitem", { name: label, exact: true }))
    return expect(page.getByRole("menu")).toHaveCount(0)
  }
  if (action === "acknowledge") return press(ack)
  await row
    .getByRole("button", { name: new RegExp(`^${sc.noticed.snooze} `) })
    .click()
  await press(
    page.getByRole("menuitem", {
      name:
        action === "tomorrow" ? sc.noticed.untilTomorrow : sc.noticed.forWeek,
      exact: true,
    })
  )
  await expect(page.getByRole("menu")).toHaveCount(0)
}
