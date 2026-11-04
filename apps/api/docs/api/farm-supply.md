# Supply batches

ops buy verified harvests at an agreed price per kg. The batch becomes a raw lot (a `lots` row) and records each farmer's kg share of it. When jars from that lot sell, each farmer is credited `price_per_kg × their share of the kg sold` (later step). Code: `src/modules/farm_supply/`, migration `farm_supply`.

Money is integer minor units of a workspace currency: TZS 8,000.00 per kg at 2 decimals is `price_per_kg: 800000`. Kilograms are decimal text with 2 decimals.

## `POST /v1/workspace/{workspace}/farm/supply-batches`

ops or admin only (`403` otherwise).

```json
{ "client_ref": "…", "village_id": "…", "crop_id": "…", "code": "ILU-H-2026-01",
  "raw_product_id": "…", "price_per_kg": 800000, "currency_id": "…",
  "lines": [ { "harvest_report_id": "…", "kg": "250.00" } ] }
```

| Field | Rule |
|---|---|
| `client_ref` | required uuid, the retry key |
| `village_id`, `crop_id` | required, a village and a crop of this workspace |
| `code` | required, the raw lot's code: letters, digits, `. _ / -`, 1–64, unique per workspace in any case |
| `raw_product_id` | required, a live stock-tracked product of this workspace |
| `price_per_kg` | required integer ≥ 1, minor units per kg; frozen with the batch. `price_per_kg × total kg` must fit a 64-bit integer (credits are bigint) |
| `currency_id` | required, an active currency of this workspace |
| `lines` | 1–500; each harvest once; `kg` > 0, at most 2 decimals; total ≤ 9,999,999,999.99. Error keys use the index as sent |

Each line's harvest report must be the **current, verified, actual** figure of a crop cycle of that village and crop, and not deleted. Anything else is `422` on `lines.{n}.harvest_report_id`. This includes another workspace's reports, expected figures, figures replaced by a newer one, and unverified or disputed ones.

- **Who is credited:** the primary manager of the harvest's farm, fixed on the line when the batch is made. A farm without a primary manager is a `422` on that line.
- **Four eyes:** whoever verified one of the harvests can't buy it (`403`).
- **No double commit:** for each harvest, the kg in all batches together may not pass its `quantity_kg`. Otherwise `422` on `lines.{n}.kg`, worded like RUAHA's guard: `over-commitment: 100.00 kg available, 70.00 kg already committed, 30.01 kg requested`. The harvest rows are locked (in id order) before the sum, so parallel batches can't overcommit. The sum is the SQL function `farm_harvest_committed_kg(harvest_report_id)`, shared with the market module's opportunity supply. It counts every figure of the same cycle and kind, so a newer figure never frees kg already bought on an older one.
- **One transaction:** the raw lot (status `released`, origin = village name, a `created` event with `meta.supply_batch_client_ref`, `lot.created`), the batch and its lines. Stock in arrives with lot stock (a later step).
- **Retry:** the same `client_ref` with the same body (kg spelling and line order don't matter) returns the stored batch with `200`. Another body, or another member under the same key, gets `409`. Parallel copies write one batch.
- **Frozen:** there is no edit or delete route, and a database trigger refuses `UPDATE` and `DELETE` on batches and lines.

`201 {"data": Batch}`.

## `GET …/farm/supply-batches/options?village_id&crop_id`

ops or admin only (`403` otherwise). What a new batch of one village and crop can be made of, so a form offers only what create accepts.

- `village_id`, `crop_id`: required uuids of this workspace. Missing, malformed or unknown is `422` on the field (`The selected village id is invalid.`).
- `harvests`: the current, verified, actual harvests of that village and crop that have a primary manager, have kg left (`farm_harvest_committed_kg` below `quantity_kg`), and were not verified by the caller (four eyes). Each `{harvest_report_id, farmer_name, plot_label, quantity_kg, committed_kg, available_kg}`, by farmer name. At most 500.
- `raw_products`: the workspace's live stock-tracked products (`physical` or `physical_second_chance` with `has_stock`), `{id, name, unit, suggested_for_crop}`. Suggestions first, then by name. At most 500. `suggested_for_crop` is a hint: an earlier batch of this crop used the product, or its name contains the crop's English or Swahili name (any case). Create accepts any stock-tracked product.
- `can_create_product`: the caller may add a raw product with the route below (ops and admin, so `true` for everyone this route answers).

`200 {"data": {"harvests": [...], "raw_products": [...], "can_create_product": false}}`.

## `GET …/farm/dispatch/options`

ops or admin (others `403`; a non-member `404`). What the Field dispatch form may send, without the office's link and warehouse rights:

```json
{ "data": {
  "links": [{ "id": "…", "partner": { "id": "…", "name": "AFM" } }],
  "lots": [{ "id": "…", "code": "ILU-RAW-1" }],
  "warehouses": [{ "id": "…", "name": "Iringa store" }]
} }
```

- `links`: live supply links this workspace granted (accepted, not revoked), by partner name.
- `lots`: the raw lots of its supply batches, newest first.
- `warehouses`: live warehouses holding free stock (level − reserved > 0) of those lots, by name.
- The dispatch itself (`POST /lots/{lot}/transfers`) is already open to farm ops and admin.

## `POST …/farm/supply-batches/raw-product`

ops or admin only (`403` otherwise). No `product.create` needed: this makes only raw materials. Owners and admins still manage every product in Office.

`{"name": "Raw coffee"}`. `name` is required, trimmed, at most 255 characters (`422` on `name`).

It goes through the products service like `POST /products` (numbering, default variant, price history) with fixed fields: `type` physical, `has_stock` true, `unit` kg, net `price` 0, `status` disabled, and the workspace's default VAT group and currency. `disabled` keeps it out of every sale channel: the storefront, the till catalogue and web checkout sell only `active` products. Batches, lots, stock and transfers don't look at status, so it works as a raw lot's product. To sell it after all, an owner sets it active in Office.

`201 {"data": {"id", "name", "unit": "kg"}}`. The id is a valid `raw_product_id` at once.

## `GET …/farm/supply-batches?page&page_size`, `GET …/farm/supply-batches/{batch}`

Staff only (farmers `403`). Field officers see their villages' batches; ops and admin see all. Lists are newest first. Another village's or workspace's batch is `404`.

Batch: `{id, client_ref, village_id, crop_id, price_per_kg, currency_id, kg_total, lot: {id, code, public_code}, lines: [{harvest_report_id, person_id, kg}] (by harvest id), created_by, created_at}`.
