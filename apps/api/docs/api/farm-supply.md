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

## `GET …/farm/supply-batches?page&page_size`, `GET …/farm/supply-batches/{batch}`

Staff only (farmers `403`). Field officers see their villages' batches; ops and admin see all. Lists are newest first. Another village's or workspace's batch is `404`.

Batch: `{id, client_ref, village_id, crop_id, price_per_kg, currency_id, kg_total, lot: {id, code, public_code}, lines: [{harvest_report_id, person_id, kg}] (by harvest id), created_by, created_at}`.
