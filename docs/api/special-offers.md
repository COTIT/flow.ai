# Special offers API

Promotional prices per product, plus the advanced rule fields vimdesk-web's discounts page edits. Ported from vimdesk-api `src/Domain/ERP/SpecialOffer` and the parts of `Product/V1/Models/ProductPriceTrait` (`calculateSpecialPrice`), `Product::activeSpecialOffer` and `Actions/GetProductPrice` that apply them.

Shared rules (guard order, 404s, list `meta`) are in `conventions.md`. Writes answer:

- create: `201 {"data": {"special_offer_id": "<uuid>"}}`
- update and delete: `{"message": "ok"}`

## Routes

Two families with the same five routes, under `/v1/workspace/{workspace}`:

| Family | Base | Sees |
|---|---|---|
| Product | `/erp/products/{product}/special-offers` | that product's offers; the product must be live (else 404); create sets `product_id` to it |
| Global | `/erp/special-offers` | every offer of the workspace |

- `GET /` paginated, oldest first. `?discount_rule_type=` filters: a rule type, or `legacy` for offers without one.
- `POST /` create, `GET /{id}`, `POST /{id}` update (patch), `DELETE /{id}` (soft).

**Permissions:** `specialoffer.list` / `view` / `create` / `update` / `delete`. Laravel checked these names but had no catalog file for them, so only the owner or a `workspace_admin` passed; genesy adds the group to the catalog (vimdesk-web already gates its discounts page on `specialoffer.list`).

## Resource

```json
{ "id": "<uuid>", "from_date": "2026-11-01", "to_date": "2026-11-30", "always_active": 0,
  "value": "12.5", "type": "fixed", "meta": null, "created_at": "…", "last_updated_at": "…",
  "discount_rule_type": "legacy", "condition_quantity": null, "discount_quantity": null,
  "condition_brand": null, "condition_supplier_relation_id": null, "condition_category_id": null,
  "target_brand": null, "target_supplier_relation_id": null, "target_category_id": null,
  "target_product_id": null, "product_id": "<uuid>", "discount_value": "12.5",
  "discount_type": "fixed", "priority": 0, "apply_to_cheapest": false, "relationships": [] }
```

- `always_active` prints 0/1 (Laravel left the column uncast).
- `value` and `discount_value` print with `formatDecimals` at 4 places; `discount_value` falls back to `value`, `discount_type` to `type`, `discount_rule_type` to `"legacy"`.
- Every id is a uuid. `relationships` is `[]`: Laravel's were never loaded.

## Writing

The body keys: `product_id` (global route), `from_date`, `to_date`, `always_active`, `value`, `type`, `discount_rule_type`, `condition_quantity`, `discount_quantity`, `condition_brand`, `condition_supplier_relation_id`, `condition_category_id`, `target_brand`, `target_supplier_relation_id`, `target_category_id`, `target_product_id`, `discount_value`, `discount_type`, `priority`, `apply_to_cheapest`, `meta`.

**Field formats** (422 texts as Laravel's):

| Field | Rule |
|---|---|
| `from_date`, `to_date` | `Y-m-d`, `Y-m-d H:i:s` or an RFC 3339 timestamp (kept as its UTC date) |
| `value`, `discount_value` | number ≥ 0 |
| `type` | `exact`, `fixed`, `percentage` (`The selected type: x is invalid.`) |
| `discount_rule_type` | `bogo`, `brand_discount`, `bundle_price`, `category_discount`, `cross_product_discount`, `global_discount`, `quantity_based`, `supplier_discount` |
| `discount_type` | `bundle_price`, `fixed`, `free`, `percentage` |
| `condition_quantity`, `discount_quantity` | integer ≥ 1 |
| `priority` | integer ≥ 0 |
| `condition_brand`, `target_brand` | string ≤ 255 |
| `product_id` | a live product (`The product id does not exist`) |
| supplier, category and target product ids | live rows of the workspace (`The selected … is invalid.`) |

**Offer rules**, checked on the merged offer (an update merges the sent keys over the stored row first):

- **Legacy offer** (no `discount_rule_type`): `product_id`, `type` and `value` required. When the value or type is written: a `fixed` value above the product's price is refused (`The selected value is higher then the current product price.`), a `percentage` must lie strictly between 0 and 100 (`Only values between 1 and 99 are available`).
- **Advanced fields without a rule type:** `The discount rule type field is required when discount type / … / apply to cheapest is present.`
- **Rule offer:**
  - a condition (`product_id`, `condition_brand`, `condition_supplier_relation_id` or `condition_category_id`) unless `global_discount`;
  - `value` or `discount_value`;
  - `cross_product_discount` needs a target;
  - `quantity_based`, `bogo`, `bundle_price` need `condition_quantity`, `bogo` also `discount_quantity`.
- **Dates:** unless `always_active`, both dates are required and `from_date` must be before `to_date`. An offer with a product may not overlap another of that product's offers, an always-active one included (`There is already an special offer active between the given dates.`). Writes for one product take a per-product lock, so two parallel offers can't both pass.
- **Defaults** (Laravel `validated()`): `value` = `discount_value` (or 0) when left out on create; a `discount_value` of 0..100 without a `discount_type` gets `percentage`; `type` = `discount_type` when left out (so `free` and `bundle_price` occur as types); `always_active` clears both dates.
- **Frozen base price:** a legacy `percentage` or `fixed` offer stores the product's net price when it is created and whenever `value`, `type`, `discount_value` or `discount_type` is edited. Its discount applies to that stored price.

## How an offer sets a product's price

Only an offer **with a `product_id`** reaches a price, as in Laravel: brand, supplier, category and global offers without a product are stored for the discounts page and never applied (Laravel's cart engine that would read them has no caller).

1. **Active offer:** the product's live offers that are `always_active` or have today (UTC) between `from_date` and `to_date`, both days included. The one with the highest `priority` wins, then the newest.
2. **Special price** from a base price `b` (`products::pricing::special`, Laravel `calculateSpecialPrice` without a cart):
   - **Legacy:** `percentage` = frozen base × (100 − value) / 100; `fixed` = frozen base − value; `exact` = value.
   - **Rules other than global, cross-product and BOGO:** `type` percentage → `b × (100 − value) / 100`; exact → value; else `discount_type` percentage → `b × (100 − discount_value) / 100`, fixed → `max(0, b − discount_value)`; else `b − value`.
   - **Global, cross-product and BOGO:** the offer's own discount amount from the stored net price (percentage, fixed, free = the whole price, bundle price = price − discount_value / condition_quantity) gives `max(0, b − amount)`; an amount of 0 falls back to `type`/`value` as above.
3. **Product fields** (`products.md`): `price` and `business_price` start from their base, take the special price, then the price correction (`price-corrections.md`). `price_without_correction` takes the special price without the correction. `original_price` is the net price, corrected, without the offer. With setting `product.disable_double_discount_for_business`, a product priced below gross starts its consumer price from gross while an offer is active.
4. **`calculate-items` with a relation** (`pricing.md`): when the product's active offer is a `percentage` above the customer's discount, the customer discount drops to 0 (Laravel `GetProductPrice`).

The product resource also prints `relationships.active_special_offer` and `relationships.special_offers` (every live offer of the product, best first).

## Laravel differences

| Genesy | Laravel |
|---|---|
| The active offer is the highest priority, then the newest (user decision) | `hasOne` without an order: whichever row came first |
| Global, cross-product and BOGO offers on a product apply their own discount | They ran the cart engine with an empty cart over every active offer of the workspace, so another matching offer with a bigger amount could win |
| Update is a patch: `always_active` and `product_id` keep their values when not sent | Every update without them set `always_active` to false and `product_id` to null, detaching the offer |
| "Either discount_value or value" and the cross-product target rule apply | Their closure rules never ran on an empty field |
| No `type` and no `discount_type` is a 422 | 500 (NOT NULL column) |
| A stored `free` / `bundle_price` type prints | The enum cast failed on read (500 on the offer and the product) |
| Rule values match ignoring case and are stored lowercase | Matched ignoring case, stored as sent (an uppercase value broke reads) |
| Target and category ids are uuids | Targets printed internal integer ids; categories were integers |
| An offer of another product on a product route is 404 | Update: 422 `This special offer does not belong to the provided product` |
| Delete reads the ids from the path | Read `product_id` and `special_offer_id` from the body |
| Overlap checks are serialised per product | No lock |
| `product_variant_id` is not ported | The column existed, but the API never wrote or printed it |
