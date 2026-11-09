import { expectNoRawValues } from "../../src/test"
import { expect, expectFits, me, paged, test, type FakeApi } from "./fake-api"

const tzs = (minor: number) => ({
  minor,
  currency: "TZS",
  decimals: 2,
  currency_id: "cur-tzs",
})

const pending = {
  id: "cor-1",
  batch_id: "b-1",
  village_id: "v-1",
  status: "pending",
  reason: "Scale read 20 kg high",
  requested_by: "u-ops",
  requested_at: "2026-11-09T08:00:00Z",
  decided_by: null,
  decided_at: null,
  net_kg: "-20.00",
  lines: [
    {
      share_id: "s-1",
      person_id: "p-gift",
      farmer_name: "Gift Mushi",
      kg_delta: "-20.00",
      kg_before: "60.00",
      kg_after: "40.00",
    },
  ],
  farmers: [
    {
      person_id: "p-gift",
      name: "Gift Mushi",
      change: tzs(-1_600_000),
      balance_after: tzs(-400_000),
    },
    {
      person_id: "p-juma",
      name: "Juma Ally",
      change: tzs(1_600_000),
      balance_after: tzs(2_100_000),
    },
  ],
}

const batch = {
  id: "b-1",
  client_ref: "c-1",
  village_id: "v-1",
  crop_id: "crop-1",
  price_per_kg: 800_000,
  currency_id: "cur-tzs",
  kg_total: "100.00",
  lot: { id: "lot-1", code: "ILU-C-2026-07", public_code: "7K3M9Q2XRT" },
  lines: [
    {
      share_id: "s-1",
      harvest_report_id: "h-1",
      person_id: "p-gift",
      farmer_name: "Gift Mushi",
      kg: "60.00",
    },
    {
      share_id: "s-2",
      harvest_report_id: "h-2",
      person_id: "p-juma",
      farmer_name: "Juma Ally",
      kg: "40.00",
    },
  ],
  created_by: "u-ops",
  created_at: "2026-11-01T08:00:00Z",
}

function correctionsApi(api: FakeApi, rows: unknown[] = [pending]) {
  api
    .on("GET", "/workspace/:ws/farm/supply-corrections", () => paged(rows))
    .on("GET", "/workspace/:ws/farm/supply-batches", () => paged([batch]))
}

test("the second person reviews kg, stock and money in words, then approves", async ({
  page,
  api,
}) => {
  correctionsApi(api)
  let approved = ""
  api.on(
    "POST",
    "/workspace/:ws/farm/supply-corrections/:id/approve",
    ({ params }) => {
      approved = String(params[1])
      return { data: { ...pending, status: "approved" } }
    }
  )
  await page.goto("/supply/corrections")
  await expect(page.getByText("Gift Mushi: 60.00 → 40.00 kg")).toBeVisible()
  await expect(page.getByText("Waiting for approval")).toBeVisible()
  await page.getByRole("button", { name: "Review" }).click()
  const dialog = page.getByRole("dialog")
  await expect(
    dialog.getByText("Raw lot stock changes by -20.00 kg")
  ).toBeVisible()
  await expect(
    dialog.getByText(/Gift Mushi will owe .*4,000\.00 back/)
  ).toBeVisible()
  await expect(dialog.getByText(/Juma Ally: .*16,000\.00/)).toBeVisible()
  await expectNoRawValues(page, { allow: [] })
  await expectFits(page)
  await dialog.getByRole("button", { name: "Approve" }).click()
  await expect(dialog).toBeHidden()
  expect(approved).toBe("cor-1")
})

test("the requester waits for someone else and sees no decision buttons", async ({
  page,
  api,
}) => {
  correctionsApi(api, [{ ...pending, requested_by: me.id }])
  await page.goto("/supply/corrections")
  await expect(page.getByText("Another ops or admin decides")).toBeVisible()
  await expect(page.getByRole("button", { name: "Review" })).toHaveCount(0)
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        {
          url: "/v1/workspace/w1/farm/supply-batches/b-1/corrections",
          status: 422,
        },
      ],
    },
  })
  test("filing a wrong-farmer correction sends two lines; the API's 422 shows at the field", async ({
    page,
    api,
  }) => {
    correctionsApi(api, [])
    const sent: unknown[] = []
    api.on(
      "POST",
      "/workspace/:ws/farm/supply-batches/:batch/corrections",
      ({ body }) => {
        sent.push(body)
        if (sent.length === 1)
          return {
            status: 422,
            body: {
              message: "The raw lot is sold out.",
              errors: {
                batch: [
                  "The raw lot is sold out: its deliveries can no longer be corrected.",
                ],
              },
            },
          }
        return { status: 201, body: { data: pending } }
      }
    )
    await page.goto("/supply/corrections")
    await page.getByRole("button", { name: "Correct a delivery" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Batch").selectOption({ label: "ILU-C-2026-07" })
    await dialog
      .getByLabel("Delivery")
      .selectOption({ label: "Gift Mushi · 60.00 kg" })
    await dialog.getByLabel("Right kg for this farmer").fill("40")
    await dialog
      .getByLabel("Kg belongs to another farmer")
      .selectOption({ label: "Juma Ally" })
    await dialog.getByLabel("Kg for the other farmer").fill("20")
    await dialog.getByLabel("Reason").fill("Juma's bag was weighed under Gift")
    await dialog.getByRole("button", { name: "Send for approval" }).click()
    await expect(dialog.getByText(/sold out/).first()).toBeVisible()
    await dialog.getByRole("button", { name: "Send for approval" }).click()
    await expect(dialog).toBeHidden()
    expect(sent[1]).toMatchObject({
      reason: "Juma's bag was weighed under Gift",
      lines: [
        { share_id: "s-1", kg: "40" },
        { share_id: "s-1", person_id: "p-juma", kg: "20" },
      ],
    })
  })
})
