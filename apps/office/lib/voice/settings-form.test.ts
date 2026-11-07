import { expect, it } from "vitest"
import { settingsFormValues } from "./settings-form"

it("prefills omitted native form values from masked settings without touching other sections", () => {
  expect(
    settingsFormValues(
      {
        smtp: {
          host: "mail.example.test",
          from_name: "Old",
          password: "********",
        },
        integrations: { payment: { enabled: true } },
      },
      { smtp: { from_name: "New" } }
    )
  ).toEqual({
    smtp: { host: "mail.example.test", from_name: "New", password: "********" },
  })
})

it("keeps explicit clears and credential replacements while preserving omitted nested providers", () => {
  const current = {
    integrations: {
      payment: {
        mollie: { api_key: "********" },
        pay: { api_token: "********" },
      },
    },
    smtp: { password: "********", host: "old" },
  }
  expect(
    settingsFormValues(current, {
      integrations: { payment: { mollie: { api_key: null } } },
      smtp: { password: "replacement", host: "new" },
    })
  ).toEqual({
    integrations: {
      payment: { mollie: { api_key: null }, pay: { api_token: "********" } },
    },
    smtp: { password: "replacement", host: "new" },
  })
  expect(settingsFormValues(current, { integrations: null })).toEqual({
    integrations: null,
  })
})
