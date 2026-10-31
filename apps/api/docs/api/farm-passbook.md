# Farmer passbook and passport consent

Each registered farmer has a passbook: money lines in minor units of a workspace currency. Credits come from sales of their honey (a later step); debits come from payouts and reversals. Lines are never changed or removed. A mistake gets a reversing line, and a database trigger refuses `UPDATE`, `DELETE` and `TRUNCATE`. Code: `src/modules/farm_passbook/`, migration `farm_passbook`.

Money is `{minor, currency, decimals, currency_id}`, the same as vouchers.

## `GET /v1/workspace/{workspace}/farm/passbook/mine?page&page_size`

The caller's own lines, newest first. `404` when the account is not a registered person of the workspace.

## `GET …/farm/persons/{person}/passbook?page&page_size`

The person themself, or staff of the person's village (ops and admin: any village). Staff of another village or workspace get `404`; another farmer gets `403`.

Line: `{id, kind, amount, balance_after, attribution_id, payout_id, created_at}`.
- `kind`: `sale_credit`, `sale_reversal`, `payout` or `payout_void`.
- `amount`: signed, credits positive, debits negative; never 0.
- `balance_after`: the balance in that currency right after this line, summed over all lines before the page is cut. The first line of page 1 is the current balance.

## Credits at sale and debits on return (`farm_supply::sale`)

- **When:** jars leave the shelf for a sale: an office or web order is completed, or the till rings up a sale (an offline replay credits nothing more). Every supply lot among them is traced back to its batch through packing and transfers (`supply-chain.md`).
- **One transaction with the sale, in the farm workspace:**
  - a `sale` row in `farm_sale_attributions` (order, lot, jars, kg per jar, price, total)
  - a `sale_credit` passbook line per farmer
  - `sale.attributed`

  A lot is credited once per order. The sale amount (in the shop's currency) is not used: farmers are owed the agreed price per kg, in the batch currency, so no exchange rate is needed.
- **Returns:** after the order's returned quantities change, the returned units of each product fill the takes of the order in take order, each up to what it took. A restocked return puts the units back into those same lot rows (or the no-lot row for units taken from it), so a resold jar credits its farmers again. A recalled, quarantined or expired lot takes its units back and picking keeps them off sale. For each credited lot, the target debit is the same credit function applied to its returned units, split by kg. Only the change since the last return is written: a `return` row and `sale_reversal` lines.
  - A full return brings every farmer back to exactly 0.
  - A partial return may move one minor unit between farmers (the largest-remainder rule re-splits a new total); the next return evens it out. Lines are signed, so the money stays exact.
- **Append-only:** attributions and passbook lines are never changed; a trigger refuses `UPDATE`, `DELETE` and `TRUNCATE`.

## Credit math (`farm_supply::credit`)

- **Credit:** a sale of `jars` from a lot packed `units` jars from `raw_kg` kg owes `price_per_kg × jars × raw_kg ÷ units`. That total is rounded once, half up, to a minor unit (a raw lot sold by the kg is 1:1).
- **Split:** the total is split over the batch's farmers by their kg with the largest-remainder rule. Each farmer gets `floor(total × kg ÷ all kg)`; the leftover units go one each to the largest remainders, ties to the smaller person id. The credits always sum to the total exactly. Integers only.

## Passport consent

`POST …/farm/persons/{person}/passport-consent` records consent, `DELETE` withdraws it. Both return `200 {"data": {person_id, consent, consent_at, consent_by}}`. Who may call them is the same as for the passbook. The farm audit trail logs each change.

Consent is off by default. It records whether the public lot page may ever name or picture the person. Until the user confirms Tanzania PDPC registration and the cross-border transfer permit, the public page has no name or photo field at all, consent or not.

## Public lot page `farmers`

For a lot made by a supply batch, `farmers` lists one group per village: `{village, farmers, harvest_start, harvest_end}`.
- `farmers` is the number of farmers, or `null` below 3 so that a small group can't be told apart.
- The dates are the earliest start and latest end of their crop cycles' harvest windows.
- No person ids, names, phones, places, kg or prices appear. Any other lot has `farmers: []`.
