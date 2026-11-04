import assert from "node:assert/strict"
import { test } from "node:test"

import { parseSeed } from "./seed.mjs"

test("reads the login table and the notes under it", () => {
  const out = [
    "app              | role           | login                            | secret                                   | path",
    "office (AFM)     | owner          | owner@afm.test                   | pw-1                                     | /login",
    "till (AFM)       | cashier PIN    | Amina                            | 1234                                     | /till/login",
    "storefront app key (shop container): gsk_abc",
    "passport (QR target): /b/7K3M9QX2WD",
    "seeded in 3.2 s",
  ].join("\n")
  const seed = parseSeed(out)
  assert.deepEqual(seed.credentials, [
    { app: "office (AFM)", role: "owner", login: "owner@afm.test", secret: "pw-1", path: "/login" },
    { app: "till (AFM)", role: "cashier PIN", login: "Amina", secret: "1234", path: "/till/login" },
  ])
  assert.equal(seed.notes["storefront app key (shop container)"], "gsk_abc")
  assert.equal(seed.notes["passport (QR target)"], "/b/7K3M9QX2WD")
})

test("refuses output without logins (already seeded, or a failed seed)", () => {
  assert.throws(() => parseSeed("staging demo already seeded: nothing to do"), /no logins/)
})
