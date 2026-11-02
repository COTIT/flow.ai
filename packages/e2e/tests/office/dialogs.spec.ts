import { expect, paged, test } from "./fake-api"

// Dialogs (OfficeSettings): centred 540 px glass panels on a desktop, bottom sheets on a phone.
test("a dialog is a centred panel on a desktop and a bottom sheet on a phone", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/app-keys", () => paged([]))
  await page.goto("/settings/app-keys")
  await page.getByRole("button", { name: "New app key" }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog.getByLabel("Name")).toBeVisible()
  await page.evaluate(() => Promise.all(document.getAnimations().map((a) => a.finished)))
  const box = (await dialog.boundingBox())!
  const view = page.viewportSize()!
  if (view.width < 768) {
    expect(Math.round(box.width)).toBe(view.width)
    expect(Math.round(box.y + box.height)).toBe(view.height)
  } else {
    expect(Math.round(box.width)).toBe(540)
    expect(Math.abs(box.y + box.height / 2 - view.height / 2)).toBeLessThan(2)
  }
  await page.keyboard.press("Escape")
  await expect(dialog).toBeHidden()
})
