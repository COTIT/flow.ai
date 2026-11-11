import { test as guestBase } from "../../src/test"
import {
  expect,
  expectFits,
  fakeApi,
  paged,
  test,
  type FakeApi,
} from "./fake-api"

const guestTest = guestBase.extend<{ api: FakeApi }>({
  api: async ({ page }, use) => {
    await use(await fakeApi(page))
  },
})

const payment = {
  payment_id: "payment-1",
  status: "pending",
  amount: 1250,
  currency: "EUR",
  decimals: 2,
  checkout_url: null as string | null,
  attention: false,
  invoice_id: "inv-1",
}
const deploymentMeta = {
  globadyme_environment: "sandbox",
  production_origin_verified: false,
}

test("payment reconciliation refreshes without creating a payment", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/payments", () =>
    paged([{ ...payment, kind: "collection", environment: "sandbox" }])
  )
  api.on("POST", "/workspace/:ws/payments/:id/refresh", () => ({
    data: { ...payment, status: "paid" },
  }))
  await page.goto("/payments")
  await expect(
    page.getByRole("heading", { name: "Payments", exact: true })
  ).toBeVisible()
  await expect(page.getByText("€12.50", { exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Refresh status" }).click()
  await expect
    .poll(
      () => api.callsTo("POST", "/workspace/:ws/payments/:id/refresh").length
    )
    .toBe(1)
  expect(api.callsTo("POST", "/workspace/:ws/payments")).toHaveLength(0)
  await expectFits(page)
})

test("payment details preserve keyboard focus without writing to the API", async ({
  page,
  api,
}) => {
  const unresolved = { ...payment, status: "unknown", kind: "refund" }
  api.on("GET", "/workspace/:ws/payments", () => paged([unresolved]))
  api.on("GET", "/workspace/:ws/payments/:id", () => ({
    data: { ...unresolved, transactions: [], chargebacks: [] },
  }))
  await page.goto("/payments")
  const trigger = page.getByRole("button", {
    name: "Payment details",
    exact: true,
  })
  await trigger.focus()
  await expect(trigger).toBeFocused()
  await page.keyboard.press("Enter")
  const dialog = page.getByRole("dialog", { name: "Payment details" })
  await expect(dialog).toBeVisible()
  await expect(
    dialog.getByText(payment.payment_id, { exact: true })
  ).toBeVisible()
  for (const key of ["Tab", "Tab", "Shift+Tab", "Tab", "Tab"]) {
    await page.keyboard.press(key)
    await expect
      .poll(() =>
        dialog.evaluate((element) => element.contains(document.activeElement))
      )
      .toBe(true)
  }
  await page.keyboard.press("Escape")
  await expect(dialog).not.toBeVisible()
  await expect(trigger).toBeFocused()
  expect(api.calls.filter((call) => call.method !== "GET")).toHaveLength(0)
  await expectFits(page)
})

guestTest(
  "public invoice preserves bank details and blocks unknown payment retries",
  async ({ page, api }) => {
    api.on("GET", "/invoices/fetch-from-hashed/:token", () => ({
      data: {
        invoice_nr: "2026-0007",
        online_payment_enabled: true,
        is_credit: false,
        status: "pending",
        reference: "PO 12",
        due_date: null,
        balance: { total: 12.5, paid: 0, open: 12.5 },
        items: [
          { id: "line-1", title: "Honey", quantity: "1", total_incl: "12.50" },
        ],
        company_details: {
          name: "Ilundo Honey",
          account: {
            name: "Ilundo Honey",
            account_number: "NL91ABNA0417164300",
            bic_swift: "ABNANL2A",
          },
        },
        relationships: {
          currency: { decimals: 2, symbol_prefix: "€", symbol_suffix: null },
        },
      },
    }))
    api.on("GET", "/invoices/fetch-from-hashed/:token/payment", () => ({
      data: { ...payment, status: "unknown", attention: true },
    }))
    await page.goto("/clients/pay-for-invoice/token-1")
    await expect(page.getByText("NL91ABNA0417164300")).toBeVisible()
    await expect(page.getByText("Payment pending review")).toBeVisible()
    await expect(page).toHaveURL(/\/clients\/pay-for-invoice\/token-1$/)
    for (const call of api.calls)
      expect(call.headers.authorization, call.path).toBeUndefined()
    await expect(page.getByRole("button", { name: "Pay online" })).toHaveCount(
      0
    )
    expect(
      api.callsTo("POST", "/invoices/fetch-from-hashed/:token/payment")
    ).toHaveLength(0)
    await expectFits(page)
  }
)

test("payment settings retain masked credentials and unrelated integrations", async ({
  page,
  api,
}) => {
  const integrations = {
    mail: { enabled: true },
    payment: {
      active_provider: "globadyme",
      enabled: true,
      environment: "production",
      payment_providers: {
        mollie: { api_key: "********" },
        globadyme: {
          sandbox: {
            merchant_id: "********",
            username: "********",
            password: "********",
            callback_secret: "********",
            previous_callback_secret: "********",
          },
          production: {
            merchant_id: "production-history",
            username: "********",
            password: "********",
            callback_secret: "********",
            previous_callback_secret: "********",
          },
        },
      },
    },
  }
  api.on("GET", "/workspace/:ws/settings", () => ({
    data: integrations,
    meta: deploymentMeta,
  }))
  api.on("POST", "/workspace/:ws/settings", () => ({ message: "ok" }))
  await page.goto("/settings/payments")
  await expect(
    page.getByRole("heading", { name: "Payment settings", exact: true })
  ).toBeVisible()
  const sandbox = page.getByRole("region", {
    name: "Globadyme · Sandbox / Staging",
  })
  await expect(page.getByLabel("Environment", { exact: true })).toHaveValue(
    "Sandbox / Staging"
  )
  await expect(page.getByLabel("Environment", { exact: true })).toHaveAttribute(
    "readonly",
    ""
  )
  await expect(
    page.getByRole("combobox", { name: "Environment", exact: true })
  ).toHaveCount(0)
  await expect(page.locator("fieldset")).toHaveCount(1)
  await expect(
    page.getByRole("region", { name: "Globadyme · Production", exact: true })
  ).toHaveCount(0)
  await expect(page.locator('input[name^="production."]')).toHaveCount(0)
  await expect(
    page.getByRole("button", {
      name: "Retire previous callback secret",
      exact: true,
    })
  ).toHaveCount(1)
  await expect(sandbox.getByLabel("Previous callback secret")).toHaveAttribute(
    "readonly",
    ""
  )
  await sandbox
    .getByLabel("Password", { exact: true })
    .fill(" replacement-secret ")
  await page.getByRole("button", { name: "Save", exact: true }).click()
  await expect(page.getByText("Payment settings saved.")).toBeVisible()
  const call = api.callsTo("POST", "/workspace/:ws/settings")[0]!
  expect(call.body).toMatchObject({
    integrations: {
      mail: { enabled: true },
      payment: {
        environment: "sandbox",
        payment_providers: {
          mollie: { api_key: "********" },
          globadyme: {
            sandbox: {
              merchant_id: "********",
              username: "********",
              password: " replacement-secret ",
              callback_secret: "********",
              previous_callback_secret: "********",
            },
            production:
              integrations.payment.payment_providers.globadyme.production,
          },
        },
      },
    },
  })
  expect(call.body).not.toHaveProperty("meta")
  await expect(
    page.getByText(
      "Production unavailable until the official provider origin is approved.",
      { exact: true }
    )
  ).toHaveCount(0)
  await expectFits(page)
})

guestTest(
  "public invoice starts hosted checkout using payer details and server amount only",
  async ({ page, api }) => {
    const path = "/invoices/fetch-from-hashed/:token"
    const state = { payment: null as null | typeof payment, enabled: true }
    api.on("GET", path, () => ({
      data: {
        invoice_nr: "2026-0008",
        online_payment_enabled: state.enabled,
        is_credit: false,
        reference: null,
        due_date: null,
        balance: { total: 12.5, paid: 0, open: 12.5 },
        items: [],
        company_details: { company_name: "Ilundo Honey", account: {} },
        relationships: {
          currency: { decimals: 2, symbol_prefix: "€", symbol_suffix: null },
        },
      },
    }))
    api.on("GET", `${path}/payment`, () => ({ data: state.payment }))
    api.on("POST", `${path}/payment`, () => {
      state.payment = {
        ...payment,
        status: "open",
        checkout_url: "https://pay.example/checkout",
      }
      return { data: state.payment }
    })
    await page.route(
      "**/v1/invoices/fetch-from-hashed/token-online/payment/qr",
      (route) =>
        route.fulfill({
          contentType: "image/png",
          body: Buffer.from(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=",
            "base64"
          ),
        })
    )
    await page.goto("/clients/pay-for-invoice/token-online")
    await page.getByLabel("Payer name").fill("Ada Miller")
    await page.getByLabel("Country code (2 letters)").fill("nl")
    await page.getByRole("button", { name: "Pay online", exact: true }).click()
    await expect(
      page.getByRole("link", { name: "Continue to payment" })
    ).toHaveAttribute("href", "https://pay.example/checkout")
    await expect(
      page.getByRole("img", { name: "Scan this QR code to continue payment" })
    ).toBeVisible()
    state.enabled = false
    await page.reload()
    await expect(
      page.getByRole("link", { name: "Continue to payment" })
    ).toHaveAttribute("href", "https://pay.example/checkout")
    await expect(
      page.getByText("Online payment is unavailable", { exact: true })
    ).toHaveCount(0)
    expect(api.callsTo("POST", `${path}/payment`)).toHaveLength(1)
    expect(api.callsTo("POST", `${path}/payment`)[0]!.body).toEqual({
      country: "NL",
      payer_name: "Ada Miller",
    })
    await expect(page).toHaveURL(/\/clients\/pay-for-invoice\/token-online$/)
    for (const call of api.calls)
      expect(call.headers.authorization, call.path).toBeUndefined()
    await expectFits(page)
  }
)

guestTest(
  "public invoice uses a fresh collection for balance remaining after a paid attempt",
  async ({ page, api }) => {
    const path = "/invoices/fetch-from-hashed/:token"
    const oldUrl = "https://pay.example/used-checkout"
    const newUrl = "https://pay.example/remaining-checkout"
    const state = {
      payment: {
        ...payment,
        payment_id: "used-payment",
        status: "paid",
        amount: 1000,
        checkout_url: oldUrl,
      },
    }
    api.on("GET", path, () => ({
      data: {
        invoice_nr: "2026-0009",
        online_payment_enabled: true,
        is_credit: false,
        status: "partially_paid",
        reference: null,
        due_date: null,
        balance: { total: 20, paid: 10, open: 10 },
        items: [],
        company_details: { company_name: "Ilundo Honey", account: {} },
        relationships: {
          currency: { decimals: 2, symbol_prefix: "€", symbol_suffix: null },
        },
      },
    }))
    api.on("GET", `${path}/payment`, () => ({ data: state.payment }))
    api.on("POST", `${path}/payment`, () => {
      state.payment = {
        ...payment,
        payment_id: "remaining-payment",
        status: "open",
        amount: 1000,
        checkout_url: newUrl,
      }
      return { data: state.payment }
    })
    await page.route(
      "**/v1/invoices/fetch-from-hashed/token-remaining/payment/qr",
      (route) =>
        route.fulfill({
          contentType: "image/png",
          body: Buffer.from(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=",
            "base64"
          ),
        })
    )
    await page.goto("/clients/pay-for-invoice/token-remaining")
    await expect(
      page.getByRole("status").getByText("Paid", { exact: true })
    ).toBeVisible()
    await expect(
      page.getByText("Open", { exact: true }).locator("..")
    ).toHaveText(/Open.*€10\.00/)
    await expect(
      page.getByRole("link", { name: "Continue to payment" })
    ).toHaveCount(0)
    await expect(
      page.getByRole("img", { name: "Scan this QR code to continue payment" })
    ).toHaveCount(0)
    expect(api.callsTo("POST", `${path}/payment`)).toHaveLength(0)
    await page.getByLabel("Payer name").fill("Ada Miller")
    await page.getByLabel("Country code (2 letters)").fill("nl")
    await page.getByRole("button", { name: "Pay online", exact: true }).click()
    await expect(
      page.getByRole("link", { name: "Continue to payment" })
    ).toHaveAttribute("href", newUrl)
    await expect(page.locator(`a[href="${oldUrl}"]`)).toHaveCount(0)
    await expect(
      page.getByRole("img", { name: "Scan this QR code to continue payment" })
    ).toBeVisible()
    expect(api.callsTo("POST", `${path}/payment`)).toHaveLength(1)
    expect(api.callsTo("POST", `${path}/payment`)[0]!.body).toEqual({
      country: "NL",
      payer_name: "Ada Miller",
    })
    await expectFits(page)
  }
)

test("payment detail and refresh require view permission for that payment's owner", async ({
  page,
  api,
}) => {
  const invoicePayment = {
    ...payment,
    payment_id: "invoice-payment",
    invoice_id: "inv-1",
    order_id: null,
  }
  const orderPayment = {
    ...payment,
    payment_id: "order-payment",
    invoice_id: null,
    order_id: "order-1",
  }
  api.on("GET", "/me", () => ({
    data: {
      user: {
        id: "member",
        name: "Member",
        initials: "ME",
        permissions: [
          {
            workspace: "w1",
            permissions: ["invoice.list", "order.list", "order.view"],
          },
        ],
      },
    },
  }))
  api.on("GET", "/workspace/:ws/payments", () =>
    paged([invoicePayment, orderPayment])
  )
  api.on("GET", "/workspace/:ws/payments/:id", () => ({
    data: { ...orderPayment, transactions: [], chargebacks: [] },
  }))
  await page.goto("/payments")
  const invoiceCard = page
    .getByRole("region")
    .filter({ hasText: "invoice-payment" })
  await expect(
    invoiceCard.getByRole("button", { name: "Payment details" })
  ).toBeDisabled()
  await expect(
    invoiceCard.getByRole("button", { name: "Refresh status" })
  ).toBeDisabled()
  const orderCard = page
    .getByRole("region")
    .filter({ hasText: "order-payment" })
  await orderCard.getByRole("button", { name: "Payment details" }).click()
  await expect(
    page.getByRole("dialog", { name: "Payment details" })
  ).toBeVisible()
  await expect
    .poll(() => api.callsTo("GET", "/workspace/:ws/payments/:id").length)
    .toBe(1)
  const calls = api.callsTo("GET", "/workspace/:ws/payments/:id")
  expect(calls[0]!.path).toContain("/payments/order-payment")
})

test("legacy provider selection follows the deployment default", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/settings", () => ({
    meta: deploymentMeta,
    data: {
      payment: {
        active_provider: "stripe",
        environment: "sandbox",
        enabled: true,
      },
    },
  }))
  api.on("POST", "/workspace/:ws/settings", () => ({ message: "ok" }))
  await page.goto("/settings/payments")
  await expect(
    page.getByRole("combobox", { name: "Payment provider", exact: true })
  ).toHaveValue("")
  await expect(
    page
      .getByRole("combobox", { name: "Payment provider", exact: true })
      .getByRole("option")
  ).toHaveCount(2)
  await page.getByRole("button", { name: "Save", exact: true }).click()
  await expect(page.getByText("Payment settings saved.")).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/settings")[0]!.body).toMatchObject(
    { integrations: { payment: { active_provider: null } } }
  )
})

test("open collection resolution offers only provider nonacceptance", async ({
  page,
  api,
}) => {
  const open = {
    ...payment,
    status: "open",
    kind: "payment",
    invoice_id: "inv-1",
  }
  api.on("GET", "/workspace/:ws/payments", () => paged([open]))
  api.on("GET", "/workspace/:ws/payments/:id", () => ({
    data: { ...open, transactions: [], chargebacks: [] },
  }))
  api.on("POST", "/workspace/:ws/payments/:id/resolve", () => ({
    data: { ...open, status: "failed" },
  }))
  await page.goto("/payments")
  await page
    .getByRole("button", { name: "Payment details", exact: true })
    .click()
  const dialog = page.getByRole("dialog", { name: "Payment details" })
  await dialog.getByRole("button", { name: "Record resolution" }).click()
  await expect(
    dialog.getByLabel("Verified outcome").getByRole("option")
  ).toHaveCount(1)
  await expect(dialog.getByLabel("Verified outcome")).toHaveValue(
    "not_accepted"
  )
  await dialog
    .getByLabel("Reason", { exact: true })
    .fill("Provider confirmed no charge")
  await dialog
    .getByLabel("Evidence / provider reference")
    .fill("Provider case 123")
  await dialog.getByRole("checkbox").check()
  await dialog.getByRole("button", { name: "Record resolution" }).click()
  await expect
    .poll(
      () => api.callsTo("POST", "/workspace/:ws/payments/:id/resolve").length
    )
    .toBe(1)
  expect(
    api.callsTo("POST", "/workspace/:ws/payments/:id/resolve")[0]!.body
  ).toEqual({
    outcome: "not_accepted",
    reason: "Provider confirmed no charge",
    evidence: "Provider case 123",
  })
})

guestTest(
  "public invoice keeps bank details without a new online payment when integration is disabled",
  async ({ page, api }) => {
    api.on("GET", "/invoices/fetch-from-hashed/:token", () => ({
      data: {
        invoice_nr: "2026-0010",
        online_payment_enabled: false,
        is_credit: false,
        reference: null,
        due_date: null,
        balance: { total: 12.5, paid: 0, open: 12.5 },
        items: [],
        company_details: {
          company_name: "Ilundo Honey",
          account: {
            account_name: "Ilundo Honey",
            account_number: "NL91ABNA0417164300",
            bic_swift: "ABNANL2A",
          },
        },
        relationships: {
          currency: { decimals: 2, symbol_prefix: "€", symbol_suffix: null },
        },
      },
    }))
    api.on("GET", "/invoices/fetch-from-hashed/:token/payment", () => ({
      data: null,
    }))
    await page.goto("/clients/pay-for-invoice/token-disabled")
    await expect(page.getByText("NL91ABNA0417164300")).toBeVisible()
    await expect(
      page.getByRole("region", { name: "Online payment" }).getByRole("status")
    ).toContainText("Online payment is unavailable")
    await expect(
      page.getByText(
        "Please use the bank details below or contact the supplier."
      )
    ).toBeVisible()
    await expect(page.getByLabel("Payer name")).toHaveCount(0)
    await expect(
      page.getByRole("button", { name: "Pay online", exact: true })
    ).toHaveCount(0)
    await expect(page).toHaveURL(/\/clients\/pay-for-invoice\/token-disabled$/)
    for (const call of api.calls)
      expect(call.headers.authorization, call.path).toBeUndefined()
    expect(
      api.callsTo("POST", "/invoices/fetch-from-hashed/:token/payment")
    ).toHaveLength(0)
    await expectFits(page)
  }
)

guestTest.describe("stale invoice setup rejection", () => {
  guestTest.use({
    allow: {
      responses: [
        {
          url: "/v1/invoices/fetch-from-hashed/token-stale-setup/payment",
          status: 503,
        },
      ],
    },
  })
  guestTest(
    "public invoice shows setup unavailable instead of uncertain payment after a definitive rejection",
    async ({ page, api }) => {
      const path = "/invoices/fetch-from-hashed/:token"
      api.on("GET", path, () => ({
        data: {
          invoice_nr: "2026-0011",
          online_payment_enabled: true,
          is_credit: false,
          reference: null,
          due_date: null,
          balance: { total: 12.5, paid: 0, open: 12.5 },
          items: [],
          company_details: {
            company_name: "Ilundo Honey",
            account: { account_number: "NL91ABNA0417164300" },
          },
          relationships: {
            currency: { decimals: 2, symbol_prefix: "€", symbol_suffix: null },
          },
        },
      }))
      api.on("GET", `${path}/payment`, () => ({ data: null }))
      api.on("POST", `${path}/payment`, () => ({
        status: 503,
        body: {
          message:
            "Online payments are unavailable until the shop finishes payment setup.",
          code: "payment_setup_required",
        },
      }))
      await page.clock.install()
      await page.goto("/clients/pay-for-invoice/token-stale-setup")
      await page.getByLabel("Payer name").fill("Ada Miller")
      await page.getByLabel("Country code (2 letters)").fill("nl")
      await page
        .getByRole("button", { name: "Pay online", exact: true })
        .click()
      await expect(
        page.getByText("Online payment is unavailable", { exact: true })
      ).toBeVisible()
      await expect(
        page.getByText("Payment pending review", { exact: true })
      ).toHaveCount(0)
      await expect(page.getByText("NL91ABNA0417164300")).toBeVisible()
      await expect
        .poll(() => api.callsTo("GET", path).length)
        .toBeGreaterThanOrEqual(2)
      await expect
        .poll(() => api.callsTo("GET", `${path}/payment`).length)
        .toBeGreaterThanOrEqual(2)
      await page.clock.fastForward(20000)
      await expect(page.getByLabel("Payer name")).toHaveCount(0)
      await expect(
        page.getByRole("button", { name: "Pay online", exact: true })
      ).toHaveCount(0)
      await expect(
        page.getByRole("link", { name: "Continue to payment" })
      ).toHaveCount(0)
      expect(api.callsTo("POST", `${path}/payment`)).toHaveLength(1)
      await expectFits(page)
    }
  )
})

const maskedCredentials = {
  merchant_id: "********",
  username: "********",
  password: "********",
  callback_secret: "********",
}

for (const [field, label, environment, storedValue] of [
  ["merchant_id", "Merchant ID", "sandbox", null],
  ["username", "Username", "sandbox", null],
  ["password", "Password", "production", null],
  ["callback_secret", "Callback secret", "production", null],
  ["merchant_id", "Merchant ID", "sandbox", " \t "],
  ["username", "Username", "sandbox", " \t "],
] as const) {
  test(`saved ${environment} setup identifies missing ${label}${storedValue === null ? "" : " (whitespace only)"}`, async ({
    page,
    api,
  }) => {
    api.on("GET", "/workspace/:ws/settings", () => ({
      meta: { ...deploymentMeta, globadyme_environment: environment },
      data: {
        payment: {
          active_provider: "globadyme",
          enabled: true,
          environment: environment === "sandbox" ? "production" : "sandbox",
          payment_providers: {
            globadyme: {
              sandbox: maskedCredentials,
              production: maskedCredentials,
              [environment]: { ...maskedCredentials, [field]: storedValue },
            },
          },
        },
      },
    }))
    await page.goto("/settings/payments")
    const notice = page
      .getByRole("status")
      .filter({ hasText: "Saved payment setup is incomplete" })
    await expect(notice).toBeVisible()
    await expect(notice).toContainText(
      `Saved settings for this deployment: ${environment === "sandbox" ? "Sandbox / Staging" : "Production"}.`
    )
    await expect(notice).toContainText(
      "New online payments are unavailable until setup is complete."
    )
    await expect(notice.getByRole("listitem")).toHaveText([label])
    await page
      .getByRole("region", {
        name: `Globadyme · ${environment === "sandbox" ? "Sandbox / Staging" : "Production"}`,
      })
      .getByLabel(label, { exact: true })
      .fill("unsaved replacement")
    await expect(notice.getByRole("listitem")).toHaveText([label])
    expect(api.callsTo("POST", "/workspace/:ws/settings")).toHaveLength(0)
    await expectFits(page)
  })
}

test("masked saved credentials are present and previous callback secret is optional", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/settings", () => ({
    meta: deploymentMeta,
    data: {
      payment: {
        active_provider: "globadyme",
        enabled: true,
        environment: "sandbox",
        payment_providers: {
          globadyme: {
            sandbox: { ...maskedCredentials, previous_callback_secret: null },
            production: {},
          },
        },
      },
    },
  }))
  await page.goto("/settings/payments")
  const sandbox = page.getByRole("region", {
    name: "Globadyme · Sandbox / Staging",
  })
  await expect(sandbox).toBeVisible()
  await expect(sandbox.getByLabel("Previous callback secret")).toHaveValue("")
  await expect(
    page.getByText("Saved payment setup is incomplete", { exact: true })
  ).toHaveCount(0)
  await expect(
    page.getByText(
      "Production unavailable until the official provider origin is approved."
    )
  ).toHaveCount(0)
  await expectFits(page)
})

test("production metadata renders one readonly production realm despite stored sandbox selection", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/settings", () => ({
    data: {
      payment: {
        active_provider: "globadyme",
        environment: "sandbox",
        payment_providers: {
          globadyme: {
            sandbox: maskedCredentials,
            production: maskedCredentials,
          },
        },
      },
    },
    meta: { ...deploymentMeta, globadyme_environment: "production" },
  }))
  await page.goto("/settings/payments")
  await expect(page.getByLabel("Environment", { exact: true })).toHaveValue(
    "Production"
  )
  await expect(page.getByLabel("Environment", { exact: true })).toHaveAttribute(
    "readonly",
    ""
  )
  await expect(
    page.getByRole("combobox", { name: "Environment", exact: true })
  ).toHaveCount(0)
  await expect(
    page.getByRole("group", { name: "Globadyme · Production", exact: true })
  ).toBeVisible()
  await expect(page.locator("fieldset")).toHaveCount(1)
  await expect(page.locator('input[name^="sandbox."]')).toHaveCount(0)
  await expect(page.getByLabel("Merchant ID", { exact: true })).toHaveCount(1)
  const warning = page.getByText(
    "Production unavailable until the official provider origin is approved.",
    { exact: true }
  )
  await expect(warning).toBeVisible()
  await page
    .getByRole("combobox", { name: "Payment provider", exact: true })
    .selectOption("")
  await expect(warning).toHaveCount(0)
  expect(api.calls.filter((call) => call.method === "POST")).toHaveLength(0)
  await expectFits(page)
})

for (const withMetadata of [false, true]) {
  test(`missing allowed server realm disables setup and saving (${withMetadata ? "null realm" : "no metadata"})`, async ({
    page,
    api,
  }) => {
    api.on("GET", "/workspace/:ws/settings", () => ({
      data: {
        payment: {
          active_provider: "globadyme",
          enabled: true,
          environment: "sandbox",
          payment_providers: {
            globadyme: {
              sandbox: {
                ...maskedCredentials,
                previous_callback_secret: "********",
              },
              production: maskedCredentials,
            },
          },
        },
      },
      ...(withMetadata
        ? { meta: { ...deploymentMeta, globadyme_environment: null } }
        : {}),
    }))
    await page.goto("/settings/payments")
    await expect(
      page.getByText("Payment setup is unavailable", { exact: true })
    ).toBeVisible()
    await expect(page.getByLabel("Environment", { exact: true })).toHaveValue(
      "Unavailable"
    )
    await expect(
      page.getByRole("button", { name: "Save", exact: true })
    ).toBeDisabled()
    await expect(
      page.getByRole("combobox", { name: "Environment", exact: true })
    ).toHaveCount(0)
    await expect(page.locator("fieldset")).toHaveCount(0)
    await expect(page.getByLabel("Merchant ID", { exact: true })).toHaveCount(0)
    await expect(
      page.getByRole("button", {
        name: "Retire previous callback secret",
        exact: true,
      })
    ).toHaveCount(0)
    expect(api.callsTo("POST", "/workspace/:ws/settings")).toHaveLength(0)
    await expectFits(page)
  })
}

test("legacy settings save without a server realm or Globadyme credentials", async ({
  page,
  api,
}) => {
  const integrations = {
    mail: { enabled: true },
    payment: {
      active_provider: "stripe",
      enabled: true,
      environment: "production",
      payment_providers: { mollie: { api_key: "********" } },
    },
  }
  api.on("GET", "/workspace/:ws/settings", () => ({
    data: integrations,
    meta: { ...deploymentMeta, globadyme_environment: null },
  }))
  api.on("POST", "/workspace/:ws/settings", () => ({ message: "ok" }))
  await page.goto("/settings/payments")
  await expect(
    page.getByRole("button", { name: "Save", exact: true })
  ).toBeEnabled()
  await expect(page.locator("fieldset")).toHaveCount(0)
  await expect(page.getByLabel("Merchant ID", { exact: true })).toHaveCount(0)
  await page
    .getByRole("checkbox", { name: "Enable online payments", exact: true })
    .uncheck()
  await page.getByRole("button", { name: "Save", exact: true }).click()
  await expect(page.getByText("Payment settings saved.")).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/settings")[0]!.body).toEqual({
    integrations: {
      ...integrations,
      payment: {
        ...integrations.payment,
        active_provider: null,
        enabled: false,
      },
    },
  })
})

test("blocked Globadyme setup can recover to legacy without changing stored slots", async ({
  page,
  api,
}) => {
  const integrations = {
    payment: {
      active_provider: "globadyme",
      enabled: true,
      environment: "sandbox",
      payment_providers: {
        globadyme: {
          sandbox: {
            ...maskedCredentials,
            previous_callback_secret: "********",
          },
          production: {
            ...maskedCredentials,
            previous_callback_secret: "********",
          },
        },
      },
    },
  }
  api.on("GET", "/workspace/:ws/settings", () => ({
    data: integrations,
    meta: { ...deploymentMeta, globadyme_environment: null },
  }))
  api.on("POST", "/workspace/:ws/settings", () => ({ message: "ok" }))
  await page.goto("/settings/payments")
  await expect(
    page.getByRole("button", { name: "Save", exact: true })
  ).toBeDisabled()
  await page
    .getByRole("combobox", { name: "Payment provider", exact: true })
    .selectOption("")
  await expect(
    page.getByRole("button", { name: "Save", exact: true })
  ).toBeEnabled()
  await expect(page.locator("fieldset")).toHaveCount(0)
  await page.getByRole("button", { name: "Save", exact: true }).click()
  await expect(page.getByText("Payment settings saved.")).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/settings")[0]!.body).toEqual({
    integrations: {
      ...integrations,
      payment: { ...integrations.payment, active_provider: null },
    },
  })
})
