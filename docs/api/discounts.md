# Discounts API

Discount groups and their discounts: percentages per supplier discount code. Ported from vimdesk-api `src/Domain/ERP/{DiscountGroup,Discount}` and the two places that apply them, `Product::buildPurchasePrice` and `Product/V1/Actions/GetProductPrice`.

Shared rules (guard order, 404s, patch updates, list `meta`) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}/products`. Writes answer:

- create: `201 {"data": {"<x>_id": "<uuid>"}}`
- update and delete: `{"message": "ok"}`

## Model

- A **discount group** belongs to one supplier (`supplier_relation_id`).
  - `internal: true`: what the supplier gives us. It lowers the product's purchase price.
  - `internal: false`: what we give a customer. It is attached to customer relations and lowers their sale lines.
- A **discount** sits in one group and gives `percentage` (0..100) to every product of that supplier whose `supplier_discount_code` equals its `discount_product_code`, ignoring case.
- A discount applies when it is `active`, its group is `active`, neither is deleted, and today (UTC, as Laravel) lies in `valid_from..valid_to` (both days included, a null end is open).
- When several discounts apply to one product, the **highest percentage** wins.

## Permissions

| Routes | Permission |
|---|---|
| group list / get / relations | `discountgroup.list` / `discountgroup.view` / `discountgroup.view` |
| group create / update / delete | `discountgroup.create` / `discountgroup.update` / `discountgroup.delete` |
| attach / detach relations | `discountgroup.attach` / `discountgroup.detach` |
| discount list / get / create / update / delete | `discount.list` / `discount.view` / `discount.create` / `discount.update` / `discount.delete` |

## Discount groups `/discount-groups`

```json
{ "id": "<uuid>", "name": "Bosch 2026", "active": true, "internal": false, "meta": null,
  "created_at": "…", "last_updated_at": "…",
  "relationships": { "supplier_relation": <RelationIndexResource> | null } }
```

**`GET /discount-groups`:**
- Paginated (`page`, `page_size`), oldest first.
- Filters: `q` (name contains), `active`, `internal` (`1`/`0`/`true`/`false`).
- `limit=1..100` answers a plain `{"data": [...]}` of at most that many rows, without `meta` (Laravel ListDiscountGroupRequest). `limit=0` or blank pages as usual. Over 100: 422 `The limit must not be greater than 100.`; below 0: `The limit must be at least 0.`

**`POST /discount-groups`:**

| Field | Rule |
|---|---|
| `name` | required string ≤ 150 |
| `active` | required boolean |
| `internal` | boolean, default false |
| `supplier_relation_id` | required uuid of a live relation of this workspace, else `This relation does not exist` |
| `meta` | object or null |

**`POST /discount-groups/{id}`:** `name`, `active`, `meta`. `internal` and the supplier never change (Laravel UpdateDiscountGroupRequest had no rules for them).

**`DELETE /discount-groups/{id}`:** soft-deletes the group and its discounts and drops its relation links, in one transaction. A discount create or an attach running at the same moment holds the group lock: it either lands before the delete (and is retired with the group) or fails afterwards with the group's 422 / 404.

**`GET /discount-groups/{id}/relations`:** paginated RelationIndexResource rows of the attached live relations.

**`POST /discount-groups/{id}/attach-to-relations`** and **`/detach-from-relations`:**
- Body `{"relation_ids": ["<uuid>", …]}`, required, a non-empty array.
- Every id must be a live relation of this workspace, else 422 `relation ids must be an array of valid relation_ids` (Laravel's text, without "The").
- Attach keeps existing links and ignores repeats. Detach removes the listed links.

## Discounts `/discounts`

```json
{ "id": "<uuid>", "name": "Drills", "active": true, "discount_product_code": "DR",
  "percentage": "12.5", "valid_from": "2026-11-01T00:00:00.000000Z", "valid_to": null,
  "monthly": null, "yearly": null,
  "seasonal": { "percentage": "5", "valid_from": "2026-12-01", "valid_to": "2026-12-31",
                "description": null },
  "meta": null, "created_at": "…", "last_updated_at": "…" }
```

- `percentage` prints with `format_decimals` (2 dp, trailing zeros trimmed, `"0"` when zero).
- `valid_from` / `valid_to` print as Laravel's `datetime` cast did: the date at midnight UTC.

**`GET /discounts`:** paginated, oldest first. `?discount_group_id=` keeps one live group's discounts; an unknown group is 404.

**`POST /discounts`:**

| Field | Rule |
|---|---|
| `discount_group_id` | required uuid of a live group of this workspace, else `This discount group id is not a valid Discount Group` |
| `name` | required string ≤ 150 |
| `active` | required boolean |
| `discount_product_code` | required string ≤ 255 |
| `percentage` | required number 0..100 (Laravel: any number) |
| `valid_from` | `Y-m-d` or null, today or later: `The valid from must be a date after yesterday.` |
| `valid_to` | `Y-m-d` or null, after `valid_from`: `The valid to must be a date after valid from.` |
| `monthly`, `yearly` | null or `{percentage 0..100, description ≤ 10000}`; their `valid_from`/`valid_to` are always stored null |
| `seasonal` | the same plus `valid_from`/`valid_to` (`Y-m-d`), `valid_to` after `valid_from` |
| `meta` | object or null |

A bad date reads `The <field> does not match the format Y-m-d.` Years outside 1..9999 count as bad dates.

**`POST /discounts/{id}`:** every field optional; the group never changes. A key sent as null clears it. The window check runs on the merged row, locked from read to write, so moving `valid_from` past the stored `valid_to` is refused even when another edit changed it a moment before. The "after yesterday" rule applies on create only, as in Laravel.

**`DELETE /discounts/{id}`:** soft delete.

`monthly`, `yearly` and `seasonal` are stored and returned only. Nothing applies them, in Laravel or here.

## Where discounts apply

One query per request each (`discounts::repo`):

| Reader | Groups used | Effect |
|---|---|---|
| Product resource (`products.md`) | internal groups of the product's supplier | `purchase_discount_percentage`, `supplier_purchase_price_from_discount = gross_price × (1 − d/100)`, and `purchase_price` (Laravel `buildPurchasePrice`: wholesale when its markup beats the discount, else the discounted price, else the supplier's wholesale price) |
| Relation detail (`relations.md`) | groups the relation supplies, groups attached to it | `relationships.supplier_discount_groups` / `relation_discount_groups`: `{id, name, status: null, meta}` |
| `calculate-items` with `relation_id` (`pricing.md`) | non-internal groups attached to the relation | product lines without `productDiscount` get the relation's discount |

## Laravel differences

Prices can differ from Laravel for the same data. Cutover QA compares a few customers' invoice previews on both sides.

| Genesy | Laravel |
|---|---|
| Highest applicable percentage, on both paths | Relation path: the first attached group of the supplier, then its first matching discount (collection order) |
| Group `active` is respected on both paths | Relation path ignored group `active` |
| Relation path uses non-internal groups only | Used every group attached to the relation |
| Window includes both days; each null end is open | `valid_from <= now <= valid_to` with timestamps, so the `valid_to` day itself was excluded after midnight; a set `valid_from` with a null `valid_to` crashed the relation path, and the purchase path required both ends null or both set |
| `percentage` 0..100 (DB check) | Any number |
| An attached relation is linked once (primary key) | The pivot had no unique key |
| Codes match ignoring case on both paths | The purchase path (a MySQL join, `utf8mb4_unicode_ci`) ignored case; the relation path (a PHP collection filter) compared exactly |

Special offers (F3b) can still zero a relation discount; until F3b lands, a percentage offer above the discount does not cancel it.
