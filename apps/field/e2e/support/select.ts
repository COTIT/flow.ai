import type { Page } from '@playwright/test'

/** Open a Base UI/shadcn Select and choose by its stable value or visible label. */
export async function chooseSelect(page: Page, testId: string, valueOrLabel: string) {
  await page.getByTestId(testId).click()
  // Wait for the list to open: counting options before it renders finds none
  // and would fall through to matching a value as if it were a label.
  await page.getByRole('option').first().waitFor()
  const byValue = page.locator(`[role="option"][data-value="${valueOrLabel}"]`)
  if (await byValue.count()) {
    await byValue.first().click()
    return
  }
  await page.getByRole('option').filter({ hasText: valueOrLabel }).first().click()
}
