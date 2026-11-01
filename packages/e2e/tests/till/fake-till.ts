import type { Page } from "@playwright/test"

import { test as base, expect } from "../../src/test"
import { FakeApi, paged } from "../office/fake-api"

export const WS = "w1"
export const DEVICE_KEY = "gsk_e2e_device_not_a_secret"
export const TOKEN = "e2e-till-token"

export const register = {
  id: "reg-1",
  name: "Iringa till 1",
  warehouse_id: "wh-shop",
  currency_id: "cur-eur",
}
export const amina = { id: "tu-1", name: "Amina" }

export const session = {
  token: TOKEN,
  expires_at: "2099-01-01T00:00:00Z",
  register,
  user: amina,
}

/** Starts the page paired (and logged in unless `loggedIn: false`), via the till's localStorage entry. */
export async function seedTill(page: Page, { loggedIn = true } = {}) {
  const value = JSON.stringify({
    state: {
      device: { workspaceId: WS, deviceKey: DEVICE_KEY },
      session: loggedIn ? session : null,
    },
    version: 0,
  })
  await page.addInitScript((v) => localStorage.setItem("genesy.till", v), value)
}

/** A FakeApi with the device-key login routes; specs add the rest. */
export async function fakeTill(page: Page) {
  const api = new FakeApi()
    .on("GET", "/workspace/:ws/till/login/users", () => ({
      data: [amina, { id: "tu-2", name: "Baraka" }],
    }))
    .on("POST", "/workspace/:ws/till/login", () => ({ data: session }))
    .on("POST", "/workspace/:ws/till/logout", () => ({ status: 204 }))
  await page.route("**/v1/**", (route) => api.handle(route))
  return api
}

/** Guarded `test` with a faked till API on every page; the device is not seeded. */
export const test = base.extend<{ api: FakeApi }>({
  api: async ({ page }, use) => {
    await use(await fakeTill(page))
  },
})

export { expect, paged }
