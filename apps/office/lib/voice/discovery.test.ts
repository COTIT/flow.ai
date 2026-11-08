import { describe, expect, it } from "vitest"

import { discoverTools } from "./catalog"
import replay from "./discovery-replay.fixture.json"

const owner = ["workspace_owner"]
const top = (query: string) => discoverTools(query, owner).map((t) => t.id)

describe("voice tool discovery", () => {
  // Real agent queries from the 2026-11-08 voice probe; `want` = the op the scenario needed.
  it("finds the needed operation for real probe queries", () => {
    const found = replay.filter(({ query, want }) =>
      want.some((id) => top(query).includes(id))
    )
    // 155 of 225 before short words and punctuation stopped matching; raise, never lower.
    expect(found.length).toBeGreaterThanOrEqual(174)
  })

  it.each([
    [
      "vaults unusual or odd activity, anomalies, audit activity",
      "till_vaults_list",
    ],
    ["cancel an order", "orders_cancel"],
    [
      "transfer stock inventory product between warehouses or branches",
      "stock_move_between_bins",
    ],
    ["resend last invoice to customer Brightwater Co-op", "invoices_resend"],
  ])("ranks the right operation for %s", (query, id) => {
    expect(top(query)).toContain(id)
  })

  it.each([
    ["anything odd in the vaults", "till_vaults_list"],
    ["hoeveel contant geld zit in de kluis", "till_vaults_list"],
    ["kuna nini kwenye sefu", "till_vaults_list"],
    ["how much does the customer owe", "invoices_list"],
    ["hoeveel schuld heeft deze klant", "invoices_list"],
    ["mteja ana deni kiasi gani", "invoices_list"],
    ["repeat the customer's last order", "orders_create"],
    ["herhaal de laatste bestelling", "orders_create"],
    ["rudia oda ya mwisho", "orders_create"],
    ["hide product until next season", "products_update"],
    ["product verbergen tot volgend seizoen", "products_update"],
    ["ficha bidhaa hadi msimu ujao", "products_update"],
    ["duplicate product as a 1 kg version", "products_duplicate"],
    ["kopie van product maken", "products_duplicate"],
    ["nakili bidhaa", "products_duplicate"],
    ["mark picked orders delivered", "orders_complete"],
    ["refund the last till sale", "orders_returns"],
    ["kassabon retour", "orders_returns"],
    ["rudisha mauzo ya kaunta", "orders_returns"],
    ["annuleer de bestelling", "orders_cancel"],
    ["ghairi oda", "orders_cancel"],
  ])("finds %s in any app language", (query, id) => {
    expect(top(query)).toContain(id)
  })

  it("ignores filler words and punctuation", () => {
    expect(top("or an a to in by of, it")).toEqual([])
  })
})
