import {
  apiAs,
  cred,
  expect,
  pairTill,
  stackAllow,
  test,
  unique,
  urls,
} from "../src/stack"
import { tillApi } from "./till"

type Register = { id: string; warehouse_id: string; currency_id: string }

test.describe("pairing code used twice", () => {
  test.use({
    allow: {
      ...stackAllow,
      responses: [
        ...stackAllow.responses,
        { url: `${urls.till}/v1/till/pair`, status: 410 },
      ],
    },
  })

  test("a code pairs one till; a second till with it is told to ask for a new one", async ({
    page,
    browser,
    request,
  }) => {
    // A register of the test's own, beside the seeded counter (data rule 1).
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    const [seeded] = await owner.get<Register[]>(owner.ws("/till/registers"))
    const register = await owner.post<Register>(owner.ws("/till/registers"), {
      name: `e2e pair ${unique()}`,
      warehouse_id: seeded!.warehouse_id,
      currency_id: seeded!.currency_id,
    })
    const pairing = await owner.post<{ code: string; workspace: string }>(
      owner.ws(`/till/registers/${register.id}/pairing`)
    )
    const pair = async (p: typeof page) => {
      await p.goto(`${urls.till}/setup`)
      await p.getByLabel("Account").fill(pairing.workspace)
      await p.getByLabel("Pairing code").fill(pairing.code)
      await p.getByRole("button", { name: "Pair this till" }).click()
    }
    await pair(page)
    await expect(page).toHaveURL(`${urls.till}/login`)

    // A refused pairing counts toward the AFM name's 10 tries per address (shared with every
    // pairTill); the next successful pairing clears them.
    const other = await (await browser.newContext()).newPage()
    await pair(other)
    await expect(
      other.getByRole("status").filter({
        hasText:
          "This code has expired or was already used. Ask the office for a new one.",
      })
    ).toBeVisible()
    await expect(other).toHaveURL(`${urls.till}/setup`)
    await other.context().close()
  })
})

test.describe("wrong PINs", () => {
  test.use({
    allow: {
      ...stackAllow,
      responses: [
        ...stackAllow.responses,
        { url: tillApi("/login"), status: 401 },
        { url: tillApi("/login"), status: 429 },
      ],
    },
  })

  test("each wrong PIN says so; the fifth locks the cashier, even against the right PIN", async ({
    page,
    request,
  }) => {
    const { owner } = await pairTill(page, request)
    // A cashier of the test's own, so the lock never stops another test's seeded cashier.
    const name = `e2e cashier ${unique()}`
    await owner.post(owner.ws("/till/users"), { name, pin: "4821" })
    await page.getByRole("button", { name: "Lock", exact: true }).click()
    await expect(page).toHaveURL(`${urls.till}/login`)
    await page.getByRole("button", { name }).click()

    const enter = async (pin: string) => {
      for (const d of pin)
        await page.getByRole("button", { name: d, exact: true }).click()
      // The same notice stays up between tries: wait for each answer before reading it.
      const answer = page.waitForResponse(tillApi("/login"))
      await page.getByRole("button", { name: "Log in" }).click()
      return (await answer).status()
    }
    for (let i = 0; i < 5; i++) {
      expect(await enter("1111")).toBe(401)
      await expect(
        page.getByRole("alert").filter({ hasText: "Wrong PIN." })
      ).toBeVisible()
    }
    expect(await enter("4821")).toBe(429)
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "Too many wrong PINs. Try again later." })
    ).toBeVisible()
    await expect(page).toHaveURL(`${urls.till}/login`)
  })
})

test.describe("device key replaced in the office", () => {
  test.use({
    allow: {
      ...stackAllow,
      responses: [
        ...stackAllow.responses,
        { url: tillApi("/logout"), status: 401 },
        { url: tillApi("/login/users"), status: 401 },
      ],
    },
  })

  test("the till says it was disconnected and pairs again with a new code", async ({
    page,
    request,
  }) => {
    const { owner, register } = await pairTill(page, request)
    await owner.post(owner.ws(`/till/registers/${register.id}/key`))
    await page.getByRole("button", { name: "Lock", exact: true }).click()
    await expect(
      page.getByRole("alert").filter({
        hasText:
          "This till was disconnected or paired elsewhere. Pair it again with a new code from the office.",
      })
    ).toBeVisible()
    await page.getByRole("button", { name: "Pair again" }).click()
    await expect(page).toHaveURL(`${urls.till}/setup`)

    const pairing = await owner.post<{ code: string; workspace: string }>(
      owner.ws(`/till/registers/${register.id}/pairing`)
    )
    await page.getByLabel("Account").fill(pairing.workspace)
    await page.getByLabel("Pairing code").fill(pairing.code)
    await page.getByRole("button", { name: "Pair this till" }).click()
    await expect(page).toHaveURL(`${urls.till}/login`)
    await expect(
      page.getByRole("button", {
        name: cred("till (AFM)", "cashier PIN").login,
      })
    ).toBeVisible()
  })
})
