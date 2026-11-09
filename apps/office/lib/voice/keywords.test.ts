import { describe, expect, it } from "vitest"

import { discoverTools, operationManifest } from "./catalog"
import keywords from "./keywords.json"

// RUAHA co-op admin: workspace admin plus live farm admin role.
const admin = ["workspace_admin", "farm:admin", "farm:ops"]
const top = (query: string, n = 3) =>
  discoverTools(query, admin)
    .slice(0, n)
    .map((tool) => tool.id)

describe("voice trigger words", () => {
  it("names only real catalog operations, each with en/nl/sw word lists", () => {
    const ids = new Set(operationManifest.map((op) => op.id))
    for (const [id, words] of Object.entries(keywords)) {
      expect(ids.has(id), id).toBe(true)
      expect(Object.keys(words).sort(), id).toEqual(["en", "nl", "sw"])
      for (const list of Object.values(words))
        for (const word of list)
          expect(word, id).toMatch(/^[\p{Ll}\p{N}' -]+$/u)
    }
  })

  it.each([
    // en: queries the probe agent actually sent when it said "can't"
    [
      "record or create delivery received 40 kg coffee cherry from farmer in Kitowo; coffee deliveries intake farmer produce",
      "farm_supply_create",
    ],
    ["farmer delivered 200 kg coffee today", "farm_supply_create"],
    [
      "farmers outstanding balance unpaid amount owed to farmers",
      "farm_passbook_person",
    ],
    ["farmer passbook statement for October", "farm_passbook_person"],
    [
      "inventory stock transfer between branches; transfer quantity of product to another branch",
      "supply_chain_dispatch",
    ],
    ["send coffee to the other branch", "supply_chain_dispatch"],
    ["set up a supply link with a new farm co-op", "supply_links_grant"],
    ["pay farmers what we owe", "farm_payouts_create"],
    [
      "correct the delivery weight of a farmer harvest",
      "farm_records_harvest_reports_revise",
    ],
    ["how much do we owe farmers in total", "farm_passbook_balances"],
    ["did the farmer agree to share data", "farm_passbook_consent"],
    [
      "farmers who haven't delivered in a month",
      "farm_supply_quiet_farmers",
    ],
    // sw
    ["mkulima ameleta kilo 200 za kahawa leo", "farm_supply_create"],
    ["jumla ya deni la wakulima", "farm_passbook_balances"],
    ["ridhaa ya mkulima kushiriki taarifa", "farm_passbook_consent"],
    ["wakulima hawajaleta tangu mwezi", "farm_supply_quiet_farmers"],
    ["deni la mkulima daftari", "farm_passbook_person"],
    ["peleka kahawa tawi lingine", "supply_chain_dispatch"],
    ["unganisha na ushirika mwingine", "supply_links_grant"],
    ["lipa wakulima malipo", "farm_payouts_create"],
    // nl
    ["boer heeft 200 kg koffie geleverd", "farm_supply_create"],
    ["hoeveel zijn we de boer schuldig", "farm_passbook_person"],
    ["koffie naar het andere filiaal sturen", "supply_chain_dispatch"],
  ])("finds the right operation for %j", (query, id) => {
    expect(top(query)).toContain(id)
  })
})
