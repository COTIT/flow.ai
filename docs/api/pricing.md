# Pricing API (item totals preview)

The item editors in vimdesk-web (invoices, estimates, orders) and the POS post their lines here on every edit and show the totals that come back. Nothing is stored. Ported from vimdesk-api `src/Domain/Shared/Item` (`CalculateItemsController`, `CalculateItemMargeController`) and `src/Infrastructure/{Values/Item,Actions/BuildItemsSummary,Actions/BuildItemsMargeCalculationSummary}`.

Shared rules (guard order, 404s, 422 wording) are in `conventions.md`. Both routes sit under `/v1/workspace/{workspace}`.

## How the maths works

Think of a till receipt: every line is rounded to whole cents as it's printed, and the bottom line is the sum of what's printed.

For each line, rounded to the currency's decimals (2 for EUR), half away from zero:

```
gross    = round(quantity × price)
discount = round(gross × productDiscount / 100)
excl     = gross − discount
vat      = round(excl × vat / 100)
```

Totals are sums of the rounded lines, so `total = total_excl + total_vat` always holds.

**Laravel difference (user decision 2026-10-21):** Laravel summed unrounded floats and rounded the grand total on its own. For 3 × 0.07 at 21 % it answered `total: 0.25` while its lines add up to 0.21 + 0.03 = 0.24; for 3 × 9.99 at 21 % with 10 % off it answered 32.64 against 26.97 + 5.66 = 32.63. Genesy answers 0.24 and 32.63. Where Laravel's own figures agree, genesy matches them exactly (`pricing::engine::tests::matches_laravel_within_per_line_rounding` runs all 24 cases of `tests/fixtures/pricing_parity.json`, produced by PHP 8.4 running Laravel's real classes via `tests/fixtures/pricing_oracle.php`).

**Known limit of per-line VAT (open question for the user):** VAT is rounded on each line, the way Laravel's VAT breakdown already did. Lines whose VAT is under half a cent lose it: 1000 lines of €0.01 at 21 % show €0.00 VAT (Laravel showed €0.00 VAT too but slipped €2.10 into its total). Computing VAT once per rate on the summed lines would collect it. Real documents rarely have such lines; the choice is recorded in `docs/ai/risk-register.md`.

## Request (both routes)

```json
{
  "currency_id": "uuid | null",
  "relation_id": "uuid | null",
  "items": [
    { "id": 1, "title": "Cable", "description": "", "quantity": 3, "price": "9.99", "vat": 21,
      "productDiscount": 10, "purchase_price": 4.5, "product_id": "uuid | null",
      "multi_set_item_id": null }
  ]
}
```

| Field | Rule | Notes |
|---|---|---|
| `items` | required, array, 1..1000 lines | Laravel had no cap. |
| `items.N` | object | |
| `items.N.quantity`, `price` | number or numeric string, ±`9999999999999999.99999999`, ≤ 8 decimals | Blank, null or missing = 0. Prices are excl VAT. |
| `items.N.vat` | number ≥ 0 | VAT percent. Blank = 0. |
| `items.N.productDiscount` | 0..100 | Percent. |
| `items.N.purchase_price` | number | Margin only. |
| `items.N.title` | string ≤ 500 | Echoed. |
| `items.N.description` | string ≤ 5000 | Echoed; a product line gets the product's name. |
| `items.N.product_id` | uuid of a live product of this workspace | One query for all lines. Looked up only for callers with `product.view` (see Auth). |
| `items.N.multi_set_item_id` | any | PHP-truthy = child line of a product set; margin skips it (`{}` and `[]` are falsy, as PHP decodes them). |
| `items.N.client_default_discount` | number | Echoed back formatted (vimdesk-web writes it onto the row). |
| `items.N.id` | any scalar | Echoed; an array or object echoes as null. |
| `currency_id` | live currency of this workspace | Missing = the workspace's default currency, else its oldest. |
| `relation_id` | live relation of this workspace | Only validated for now (see below). |

Unknown keys (`disable_client_discounts`, `drag_key`, `relationships`, …) are ignored, because vimdesk-web posts whole item rows. The body limit on these two routes is 2 MiB (the app-wide limit is 64 KiB, which a 50-line invoice can exceed). The parser (`pricing::body`) skips unknown keys and nested values without keeping them and stops collecting lines past 1000, so a 2 MiB body costs about its own size in memory.

**Errors (422):**

| Case | Key | Message |
|---|---|---|
| no / empty / null `items` | `items` | The items field is required. |
| `items` not a list | `items` | The items must be an array. |
| more than 1000 lines | `items` | The items must not have more than 1000 items. |
| line not an object | `items.N` | The items.N must be an array. |
| bad number | `items.N.price` | The items.N.price must be a number. (also `must be at least 0.`, `must not be greater than 100.`, `must have 0-8 decimal places.`) |
| a line overflows the decimal range | `items.N` | The items.N is out of range. |
| unknown product | `items.N.product_id` | The selected items.N.product id is invalid. |
| unknown currency | `currency_id` | This currency id does not exist |
| no currency at all | `currency_id` | No default currency has been found. Please create one |
| unknown relation | `relation_id` | The relation id is an invalid id for the relation domain. |

Laravel reported a bad line differently: spatie Data's validator threw with item-level keys. vimdesk-web shows none of these errors; it only keeps the previous totals.

**Auth:** bearer token, then any active member of the workspace. 401 without a token, 404 for an outsider, 403 for an inactive member. No permission is needed for the maths: Laravel's `authorize()` returned true.

**Product details need `product.view`.** A product line's name and purchase price are catalog data that `GET /products/{id}` guards. Without `product.view`, `product_id` is echoed but not looked up: the description stays as sent and margin uses the sent purchase price. Laravel filled them for any caller.

## `POST /calculate-items`

```json
{ "data": {
  "sum_total": 29.97, "total_discount": 3, "sub_total": 29.97,
  "total_vats": { "21": { "percentage": 21, "total_amount": 5.66 } },
  "total_vat": 5.66, "total": 32.63, "total_excl": 26.97,
  "total_excl_materials": 26.97, "total_excl_hours": 0, "total_excl_hours_users": [],
  "discounts": [ { "id": 1, "title": "Cable", "quantity": 3, "price": 9.99, "vat": 21,
                   "description": "", "product_id": null, "total": 26.97, "subtotal": 29.97,
                   "productDiscount": "10", "purchase_price": null,
                   "client_default_discount": "0.00", "multi_set_item_id": null } ]
} }
```

- Amounts are JSON numbers, as Laravel's floats were (`29.97`, `100`, never `100.0`). `productDiscount` and `client_default_discount` are strings (Laravel `MoneyHelper::formatDecimals`).
- `sum_total` and `sub_total` are the same: Σ gross. `total_excl_materials` = `total_excl`. Hours are always 0 (hour lines exist only on stored work orders).
- `total_vats` is keyed by the exact rate (`"5.5"`) and is always an object. Laravel cast the key to an int, so 5.5 % and 5 % merged into one `"5"` row, and a single 0 % rate came back as a JSON list. vimdesk-web loops over either shape.
- `discounts` lists the lines with a discount > 0 and a positive gross, echoed with their computed `total` (excl) and `subtotal` (gross).

## `POST /calculate-marge`

```json
{ "data": {
  "total_purchase_price": 34.5, "marge_percentage": 41.52542372881356, "marge_total_amount": 24.5,
  "extra_data": {
    "items_percentage": 41.52542372881356, "items_purchase_price": 34.5, "items_amount": 24.5,
    "transfer_lists_percentage": 0, "transfer_lists_purchase_price": 0, "transfer_lists_amount": 0,
    "hours_purchase_price": 0, "hours_sale_price": 0,
    "components": {
      "items": { "cost_excl": 34.5, "revenue_excl": 59, "margin_amount": 24.5, "margin_percentage": 41.52542372881356 },
      "transfer_lists": { "cost_excl": 0, "revenue_excl": 0, "margin_amount": 0, "margin_percentage": 0 },
      "hours": { "cost_excl": 0, "revenue_excl": 0, "margin_amount": 0, "margin_percentage": 0 } },
    "totals": { "cost_excl": 34.5, "revenue_excl": 59, "margin_amount": 24.5, "margin_percentage": 41.52542372881356 }
} } }
```

- `cost_excl` = Σ round(purchase_price × quantity). A product line without a purchase price (or with 0) takes the product's (`products::pricing::purchase_price`).
- `revenue_excl` = Σ line `excl` (after discount, per-line rounded).
- `margin_percentage` = (revenue − cost) / revenue × 100, unrounded. With zero revenue Laravel divides by 1, so a free line that cost 5 shows −500; genesy keeps that.
- Set children (`multi_set_item_id` truthy) are skipped.
- Transfer lists and hours come from a stored work order; a calculate request never has one, so they are 0.

## Not ported yet

- **Relation discounts (F3):** with `relation_id` and a `product_id`, Laravel filled `productDiscount` (when not sent) and `client_default_discount` from the relation's discount groups and the product's special offer. Genesy validates the relation but applies no discount and echoes the sent `client_default_discount`. F3 must land before the cutover.
