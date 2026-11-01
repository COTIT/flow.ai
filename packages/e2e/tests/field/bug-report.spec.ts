import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { eligibilityBefore, farmerMe } from "./farmer.api"
import { categories, currencies, equipment, mill, milling } from "./equipment.api"

const BUGS = "/workspace/w1/farm/bug-reports"
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/
// A 1x1 PNG, picked as the photo so the report always carries an image.
const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=", "base64")

/** The multipart fields of a request, by name (file parts as their file name). */
function fields(body: Buffer, contentType: string): Record<string, string> {
  const boundary = /boundary=(.+)$/.exec(contentType)![1]!
  const out: Record<string, string> = {}
  for (const part of body.toString("latin1").split(`--${boundary}`)) {
    const name = /name="([^"]+)"(?:; filename="([^"]*)")?/.exec(part)
    if (!name) continue
    out[name[1]!] = name[2] ?? part.slice(part.indexOf("\r\n\r\n") + 4).replace(/\r\n$/, "")
  }
  return out
}

/** Signed in, on a screen; each bug-report POST is recorded as its multipart fields. */
async function open(page: Page, who: "farmer" | "ops", post: MockReply) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["farmer:u1", "ops:u1"])))
  const routes: Record<string, MockReply> = who === "farmer"
    ? {
        "GET /farm/me": farmerMe,
        "GET /workspace/w1/farm/surveys/eligibility": eligibilityBefore,
        "GET /workspace/w1/farm/equipment": equipment(mill),
      }
    : {
        "GET /farm/me": me({ name: "Omari Ops", memberships: [role("ops", null)] }),
        "GET /workspace/w1/farm/villages": villages,
        "GET /workspace/w1/currencies": currencies,
        "GET /workspace/w1/farm/equipment-categories": categories(milling),
        "GET /workspace/w1/farm/equipment": equipment(mill),
      }
  await mockApi(page, { ...routes, [`POST ${BUGS}`]: post })
  const sent: Record<string, string>[] = []
  await page.route(`**/v1${BUGS}`, async (route) => {
    const request = route.request()
    sent.push(fields(request.postDataBuffer()!, (await request.allHeaders())["content-type"] ?? ""))
    await route.fallback()
  })
  await page.goto(who === "farmer" ? "/farm/equipment?debug=1" : "/ops/catalogue")
  await page.getByTestId("bug-fab").click()
  await expect(page.getByTestId("bug-sheet")).toBeVisible()
  await page.getByTestId("bug-photo").setInputFiles({ name: "screen.png", mimeType: "image/png", buffer: PNG })
  await page.getByTestId("bug-description").fill("The price looks wrong")
  return sent
}

test("a farmer files a report: multipart, under the session, with a device-made id", async ({ page }) => {
  const sent = await open(page, "farmer", { status: 201, body: { data: { id: "81000000-0000-4000-8000-000000000001", ref: 12 } } })
  await page.getByTestId("bug-submit").click()

  await expect(page.getByTestId("bug-sent")).toContainText("Thank you. Your report was sent.")
  await expect(page.getByTestId("bug-sheet")).toHaveCount(0)
  expect(sent).toHaveLength(1)
  const form = sent[0]!
  expect(Object.keys(form).sort()).toEqual(
    expect.arrayContaining(["actions", "description", "id", "photo", "route", "user_agent", "viewport"]),
  )
  // The API files under the signed-in user: no reporter is ever named (an unknown field is a 422).
  expect(form).not.toHaveProperty("reporter_id")
  expect(form.id).toMatch(UUID)
  expect(form.description).toBe("The price looks wrong")
  // Path and query keys only, never a value.
  expect(form.route).toBe("/farm/equipment?debug")
  expect(form.viewport).toMatch(/^\d+x\d+$/)
  expect(form.photo).toBe("screen.png")
  const actions = JSON.parse(form.actions ?? "[]") as { kind: string; detail: string }[]
  expect(actions.length).toBeLessThanOrEqual(30)
  expect(actions.some((a) => a.kind === "api" && a.detail.startsWith("GET /v1/workspace/w1/farm/equipment"))).toBe(true)
  expect(form.actions ?? "").not.toContain("e2e-token")
  await expectNoHorizontalScroll(page)
})

test.describe("a refused report", () => {
  test.use({ allow: { responses: [{ url: `/v1${BUGS}`, status: 422 }] } })

  test("keeps the sheet open with the API's message as written; a retry sends the same id", async ({ page }) => {
    const sent = await open(page, "ops", {
      status: 422,
      body: { message: "The given data was invalid.", errors: { photo: ["The photo must be a JPEG, PNG, WebP or HEIC image."] } },
    })
    await page.getByTestId("bug-submit").click()

    await expect(page.getByTestId("bug-send-error")).toContainText("The photo must be a JPEG, PNG, WebP or HEIC image.")
    await expect(page.getByTestId("bug-sheet")).toBeVisible()
    await page.getByTestId("bug-submit").click()
    await expect.poll(() => sent.length).toBe(2)
    expect(sent[1]!.id).toBe(sent[0]!.id)
    await expectNoHorizontalScroll(page)
  })
})
