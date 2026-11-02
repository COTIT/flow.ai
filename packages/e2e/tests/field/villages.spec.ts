import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role } from "./api"
import { factorRefused, ilundo, ilundoCapacity, mlowa, villageList } from "./villages.api"

const VILLAGES = "/workspace/w1/farm/villages"
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/

type Sent = { key: string; body: unknown }

/** Signed in as ops. Writes are recorded; `effects` update the reads before a write is answered. */
async function open(
  page: Page,
  { extra = {}, effects = {} }: {
    extra?: Record<string, MockReply>
    effects?: Record<string, (routes: Record<string, MockReply>) => void>
  } = {},
) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  const routes: Record<string, MockReply> = {
    "GET /farm/me": me({ name: "Omari Ops", memberships: [role("ops", null)] }),
    [`GET ${VILLAGES}`]: villageList(ilundo),
    ...extra,
  }
  await mockApi(page, routes)
  const sent: Sent[] = []
  await page.route("**/v1/**", async (route) => {
    const request = route.request()
    const key = `${request.method()} ${new URL(request.url()).pathname.replace(/^\/v1/, "")}`
    if (request.method() !== "GET") sent.push({ key, body: request.postDataJSON() })
    effects[key]?.(routes)
    await route.fallback()
  })
  await page.goto("/ops/villages")
  return sent
}

test("ops creates a village with its first planned capacity", async ({ page }) => {
  const sent = await open(page, {
    extra: { [`POST ${VILLAGES}`]: { status: 201, body: { data: mlowa } } },
    effects: { [`POST ${VILLAGES}`]: (routes) => { routes[`GET ${VILLAGES}`] = villageList(ilundo, mlowa) } },
  })

  const rows = page.getByTestId("village-row")
  await expect(rows).toHaveCount(1)
  await expect(rows.first()).toContainText("500.000 kW")
  await expect(rows.first().getByTestId("village-basis")).toHaveText("Planned")

  await page.getByTestId("village-create-open").click()
  await page.getByTestId("village-name").fill("Mlowa")
  await page.getByTestId("village-code").fill("MLW")
  await page.getByTestId("village-latitude").fill("-7.91")
  await page.getByTestId("village-longitude").fill("35.62")
  await page.getByTestId("village-capacity").fill("250")
  await page.getByTestId("village-simultaneity").fill("0.65")
  await page.getByTestId("village-create-submit").click()

  await expect(rows).toHaveCount(2)
  // The shape the API wants: decimal text, never floats; blanks go as null for the API's defaults.
  expect(sent.find((s) => s.key === `POST ${VILLAGES}`)!.body).toEqual({
    id: expect.stringMatching(UUID),
    name: "Mlowa",
    code: "MLW",
    latitude: "-7.91",
    longitude: "35.62",
    capacity: { capacity_kw: "250", basis: "planned", simultaneity_factor: "0.65", source_note: null, effective_from: null },
  })
  await expectNoHorizontalScroll(page)
})

test("ops sets a new capacity for a village", async ({ page }) => {
  const next = { ...ilundoCapacity, capacity_kw: "600.00", basis: "nameplate", effective_from: "2026-11-01" }
  const sent = await open(page, {
    extra: { [`POST ${VILLAGES}/v1/capacity`]: { status: 201, body: { data: next } } },
    effects: {
      [`POST ${VILLAGES}/v1/capacity`]: (routes) => {
        routes[`GET ${VILLAGES}`] = villageList({ ...ilundo, capacity: next })
      },
    },
  })

  await page.getByTestId("village-set-capacity").click()
  await expect(page.getByTestId("village-create-panel")).toContainText("Ilundo")
  await page.getByTestId("village-capacity").fill("600")
  await page.getByTestId("village-basis-select").click()
  await page.getByRole("option", { name: "Nameplate" }).click()
  await page.getByTestId("village-effective-from").fill("2026-11-01")
  await page.getByTestId("village-create-submit").click()

  await expect(page.getByTestId("village-create-panel")).toHaveCount(0)
  await expect(page.getByTestId("village-row")).toContainText(["600.000 kW"])
  expect(sent.find((s) => s.key === `POST ${VILLAGES}/v1/capacity`)!.body).toEqual({
    id: expect.stringMatching(UUID),
    capacity_kw: "600",
    basis: "nameplate",
    simultaneity_factor: "1.000",
    source_note: null,
    effective_from: "2026-11-01",
  })
  await expectNoHorizontalScroll(page)
})

test.describe("a capacity save that fails", () => {
  test.use({ allow: { responses: [{ url: `/v1${VILLAGES}/v1/capacity`, status: 503 }] } })

  test("is retried with the same client id, so the API replays it", async ({ page }) => {
    const next = { ...ilundoCapacity, capacity_kw: "600.00" }
    let calls = 0
    const sent = await open(page, {
      effects: {
        [`POST ${VILLAGES}/v1/capacity`]: (routes) => {
          calls++
          routes[`POST ${VILLAGES}/v1/capacity`] =
            calls === 1 ? { status: 503, body: { message: "Service unavailable." } } : { status: 201, body: { data: next } }
          if (calls > 1) routes[`GET ${VILLAGES}`] = villageList({ ...ilundo, capacity: next })
        },
      },
    })

    await page.getByTestId("village-set-capacity").click()
    await page.getByTestId("village-capacity").fill("600")
    await page.getByTestId("village-create-submit").click()
    await expect(page.getByTestId("village-create-error")).toBeVisible()
    await page.getByTestId("village-create-submit").click()

    await expect(page.getByTestId("village-create-panel")).toHaveCount(0)
    await expect(page.getByTestId("village-capacity-saved").getByRole("status")).toContainText("Capacity saved")
    const ids = sent.filter((s) => s.key === `POST ${VILLAGES}/v1/capacity`).map((s) => (s.body as { id: string }).id)
    expect(ids).toEqual([expect.stringMatching(UUID), ids[0]])
    await expectNoHorizontalScroll(page)
  })
})

test.describe("a refused village", () => {
  test.use({ allow: { responses: [{ url: `/v1${VILLAGES}`, status: 422 }] } })

  test("shows the API's message as written, at its field", async ({ page }) => {
    await open(page, { extra: { [`POST ${VILLAGES}`]: factorRefused } })

    await page.getByTestId("village-create-open").click()
    await page.getByTestId("village-name").fill("Mlowa")
    await page.getByTestId("village-code").fill("MLW")
    await page.getByTestId("village-capacity").fill("250")
    await page.getByTestId("village-simultaneity").fill("0")
    await page.getByTestId("village-create-submit").click()

    await expect(page.getByTestId("village-simultaneity-error")).toContainText(
      "simultaneity factor must be more than 0 and at most 1",
    )
    await expect(page.getByTestId("village-create-error")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})
