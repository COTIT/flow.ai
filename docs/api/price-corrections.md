# Price corrections API

Price bands that round a product's price, for example "everything between €5 and €100 ends on .99 incl VAT". Ported from vimdesk-api `src/Domain/ERP/PriceCorrection` and the part of `Product/V1/Models/ProductPriceTrait::buildPrice` / `Actions/GetRoundedPrice` that applies them.

Shared rules (guard order, 404s, patch updates, list `meta`) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}/products/price-corrections`. Writes answer:

- create: `201 {"data": {"price_correction_id": "<uuid>"}}`
- update and delete: `{"message": "ok"}`

## Permissions

`pricecorrection.list` / `pricecorrection.view` / `pricecorrection.create` / `pricecorrection.update` / `pricecorrection.delete`.

## Resource

```json
{ "id": "<uuid>", "value_from": "5", "value_to": "100", "round_on": "1", "remove_amount": "0.01",
  "meta": null, "created_at": "…", "last_updated_at": "…" }
```

Amounts print like Laravel's `MoneyHelper::formatDecimals` on uncast columns: 2 dp, trailing zeros trimmed, zero `"0"`, null `"0.00"`.

## Routes

- **`GET /`**: paginated (`page`, `page_size`), oldest first.
- **`GET /{id}`**
- **`POST /`**:

  | Field | Rule |
  |---|---|
  | `value_from`, `value_to` | required number (net price, excl VAT) |
  | `round_on` | number or null: the incl-VAT step to round to; null or 0 = no rounding |
  | `remove_amount` | number or null: incl-VAT amount taken off after rounding |
  | `meta` | object or null |

  Numbers: JSON numbers or numeric strings, at most 8 decimals, within `numeric(24,8)` (`The <field> must be a number.`, `… must have 0-8 decimal places.`).

  A workspace holds at most 1000 live bands: `422 {"errors": {"price_corrections": ["The workspace can't hold more than 1000 price corrections."]}}`. Every product render loads all of them.
- **`POST /{id}`**: patch. `round_on`, `remove_amount` and `meta` sent as null are cleared. `value_from` or `value_to` sent as null is a 422 `The value from field is required.` (Laravel answered 500).
- **`DELETE /{id}`**: soft delete.

## How a band changes a price

For a net price `p` and the product's VAT `v` (0 without a VAT group), in exact decimals (`products::pricing::{band_for, correct, prices}`):

1. **Pick:** the live band with `value_from <= p <= value_to` (both ends included) and the lowest `value_from`, then the oldest. None: the price stays `p`.
2. **Skip** when the product has `exclude_price_corrections`.
3. **Round:** with `round_on = r` (not null, not 0), `incl = p × (100 + v) / 100` goes to the nearest multiple of `r`. An exact tie goes down. Then it is divided back to excl.
4. **Remove:** minus `remove_amount × 100 / (100 + v)`.

Example: `p = 10`, 21 % VAT, band `0..100`, `round_on 1`, `remove_amount 0.01`: 12.10 incl rounds to 12, minus 0.01 is 11.99 incl, which is 9.9091 excl.

`tests/fixtures/price_correction_parity.json` holds 14 cases from PHP 8.4 running vimdesk-api's real `GetRoundedPrice` (`price_correction_oracle.php`). All match.

## Product fields (`products.md`)

| Field | Base | Corrected |
|---|---|---|
| `price`, `original_price` | net price | yes |
| `business_price` | net price, or `gross_price` when the product is priced below gross and setting `product.disable_double_discount_for_business` is on | yes |
| `price_without_correction` | the business base | no |

Each has an `_incl` twin (× (100 + v) / 100, 2 dp). `original_price` equals `price` until special offers (F3b).

## Laravel differences

| Genesy | Laravel |
|---|---|
| Bands are read on every render; an edit shows at once | `Cache::rememberForever` per workspace, never cleared on create, update or delete: edits showed only after a cache flush |
| At most 1000 live bands per workspace | No cap |
| A band end sent as null on update is a 422 | 500 (NOT NULL column) |
| Exact decimals | Floats; all 14 oracle cases match, but a tie that float error moves off the exact midpoint can round the other way |
