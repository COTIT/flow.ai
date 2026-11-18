# Purchase units

Per product, the units a supplier sells in, each with an exact ratio to the product's own unit (Flow ERP masterplan §5 UNITS). Purchasing (later) converts `input quantity × ratio` into base units. Migration `purchase_units`.

All paths sit under `/v1/workspace/{workspace}/products/{product}/purchase-units`. Another workspace's product, a deleted product or a unit under another product is 404.

## Categories

The unit's `measurement_category` must equal the category of the product's `unit`:

| Product unit | Category |
|---|---|
| kg, gram | mass |
| cm, m | length |
| m2 | area |
| liter, m3 | volume |
| pc, single | count |

A product without a unit, or in `box`, refuses purchase units: 422 code `purchase_unit_base_unsupported` (a box needs a content unit, which does not exist yet). No mass↔volume conversion, no global conversion table.

## Resource

```json
{ "id": "uuid", "product_id": "uuid", "code": "bag", "measurement_category": "mass",
  "base_unit": "kg", "base_units_per_input_unit": "25.5", "active": true, "version": 1,
  "created_at": "…", "updated_at": "…" }
```

`base_units_per_input_unit` is an exact decimal string, trailing zeros dropped.

## `GET` — `purchase_units_list` (`product.view`)

`{"data": [Resource]}`, ordered by code, at most 50 (a product holds no more).

## `POST` — `purchase_units_create` (`product.update`)

Header `Idempotency-Key` is required (missing: 422 `The Idempotency-Key header is required.`, nothing written).

```json
{ "client_ref": "uuid",
  "payload": { "code": "bag", "measurement_category": "mass", "base_units_per_input_unit": "25" } }
```

- `code`: 1–50 characters, unique per product ignoring case.
- `base_units_per_input_unit`: a JSON **string**, above 0, at most 8 decimals, at most 999999999999.99999999. A JSON number is refused.
- Unknown fields: 422.
- 201 `{"data": Resource}`.
- `client_ref` is the permanent identity of this create: the same `client_ref` with the same meaning (code case, ratio spelling ignored) answers 200 with the stored unit; other content is 409. The Idempotency-Key replay (one day, byte for byte) runs first.
- 422 field errors under `payload.<field>`; a category that doesn't match the product unit names the right one.

Code, category and ratio never change afterwards; a database trigger refuses an edit. While units exist, the product's `unit` is frozen (`product_master_frozen`).

## `PATCH /{unit}` — `purchase_units_update` (`product.update`)

Header `Idempotency-Key` is required.

```json
{ "client_ref": "uuid", "expected_version": 1, "payload": { "active": false } }
```

- Only `active` may be sent (anything else in `payload`: 422).
- `expected_version` other than the stored version: 409 `{message, code: "stale_document"}`.
- A change bumps `version` by one; sending the current value changes nothing and keeps the version.
- The latest change's `client_ref` replays it (200, no second bump); that `client_ref` with other content is 409.
- 200 `{"data": Resource}`.

## Errors

| Status | When |
|---|---|
| 403 | missing permission |
| 404 | unknown workspace, product or unit |
| 409 | `client_ref` reused with other content; stale `expected_version` (code `stale_document`) |
| 422 | invalid fields, missing Idempotency-Key, `purchase_unit_base_unsupported` |
