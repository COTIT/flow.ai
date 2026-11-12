import { PORT_OFFSET } from "../../mock/fixtures.mjs"
import { expect, test } from "../../src/test"
import { flowApi, logIn, mockApi, readiness, savedToken } from "./api"

// Office runs on its own origin in this suite, as office.genesy.app does beside flow.genesy.app.
const office = `http://127.0.0.1:${3201 + PORT_OFFSET}`

test("a session saved by Office is invisible to Flow", async ({ page }) => {
  await page.goto(`${office}/login`)
  await page.evaluate(
    (v) => localStorage.setItem("genesy.session", v),
    JSON.stringify({
      state: { token: "office-token", workspaceIds: ["w1"], workspaceId: "w1" },
      version: 0,
    })
  )
  await page.goto("/")
  await expect(page).toHaveURL("/login")
  expect(await savedToken(page)).toBeNull()
})

test("a Flow login never reaches Office's storage", async ({ page }) => {
  await mockApi(page, flowApi())
  await readiness(page, 200, { enabled: true, ready: true })
  await page.goto("/login")
  await logIn(page)
  await expect(page.locator("[data-flow-header]").getByText("Ilundo Honey Co.")).toBeVisible()
  expect(await savedToken(page)).toBe("e2e-token")
  await page.goto(`${office}/login`)
  await expect(page.getByRole("heading", { name: "Log in" })).toBeVisible()
  expect(await savedToken(page)).toBeNull()
})
