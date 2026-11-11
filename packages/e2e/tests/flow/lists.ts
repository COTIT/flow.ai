import type { Locator, Page } from "@playwright/test"

import { sc } from "./scenes-api"

/**
 * Runs a row action the way the person sees it at this width (Lists board): inline Acknowledge and
 * the Snooze ▾ menu (table ≥ 980 px, cards < 600 px), or the ⋯ row menu (600–979 px).
 */
export async function rowAction(
  page: Page,
  row: Locator,
  action: "acknowledge" | "tomorrow" | "week"
) {
  const more = row.getByRole("button", { name: /^Actions for / })
  if (await more.isVisible()) {
    await more.click()
    const label = {
      acknowledge: sc.noticed.acknowledge,
      tomorrow: sc.noticed.snoozeTomorrow,
      week: sc.noticed.snoozeWeek,
    }[action]
    return page.getByRole("menuitem", { name: label, exact: true }).click()
  }
  if (action === "acknowledge")
    return row
      .getByRole("button", { name: sc.noticed.acknowledge, exact: true })
      .click()
  await row.getByRole("button", { name: new RegExp(`^${sc.noticed.snooze} `) }).click()
  await page
    .getByRole("menuitem", {
      name: action === "tomorrow" ? sc.noticed.untilTomorrow : sc.noticed.forWeek,
      exact: true,
    })
    .click()
}
