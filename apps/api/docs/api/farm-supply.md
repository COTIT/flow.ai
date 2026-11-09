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
| `warehouse_id` | optional, a live warehouse of this workspace (`422` otherwise): where the bought kg go in. Default: the workspace's default warehouse |
| `lines` | 1–500; each harvest once; `kg` > 0, at most 2 decimals; total ≤ 9,999,999,999.99. Error keys use the index as sent |

Each line's harvest report must be the **current, verified, actual** figure of a crop cycle of that village and crop, and not deleted. Anything else is `422` on `lines.{n}.harvest_report_id`. This includes another workspace's reports, expected figures, figures replaced by a newer one, and unverified or disputed ones.

- **Who is credited:** the primary manager of the harvest's farm, fixed on the line when the batch is made. A farm without a primary manager is a `422` on that line.
- **Four eyes:** whoever verified one of the harvests can't buy it (`403`).
- **No double commit:** for each harvest, the kg in all batches together may not pass its `quantity_kg`. Otherwise `422` on `lines.{n}.kg`, worded like RUAHA's guard: `over-commitment: 100.00 kg available, 70.00 kg already committed, 30.01 kg requested`. The harvest rows are locked (in id order) before the sum, so parallel batches can't overcommit. The sum is the SQL function `farm_harvest_committed_kg(harvest_report_id)`, shared with the market module's opportunity supply. It counts every figure of the same cycle and kind, so a newer figure never frees kg already bought on an older one.
- **One transaction:** the raw lot (status `released`, origin = village name, a `created` event with `meta.supply_batch_client_ref`, `lot.created`), the batch and its lines.
- **Buying is the receipt:** in that transaction the bought kg (the lines' total) go into the raw lot at `warehouse_id`, else the default warehouse. That is a ledger `increase` with reason `bought` and related `supply_batch`. The lot can then be packed or dispatched with no further step. With no warehouse sent and no default one, the lot is made empty, and stock goes in later with `POST /lots/{lot}/stock-mutations`.
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
  "links": [{ "id": "…", "partner": { "id": "…", "name": "afm-demo", "display_name": "AFM Shop" } }],
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

Batch: `{id, client_ref, village_id, crop_id, price_per_kg, currency_id, kg_total, lot: {id, code, public_code}, lines: [{share_id, harvest_report_id, person_id, kg}] (by harvest id; `kg` is the frozen bought figure, corrections show under `/farm/supply-corrections`), created_by, created_at}`.

## `GET …/farm/quiet-farmers?since&village_id&page&page_size`

Farmers who have not delivered since a day. Staff only (farmers `403`): field officers see the live persons registered in their villages, ops and admin see everyone. `since` (`YYYY-MM-DD`) is a Tanzania day; without it, 30 days before today. A person is listed when their last bought kg (a supply batch line, `farm_supply_shares`) was created before that day, or when they never delivered. Harvest figures don't count as deliveries. `village_id` narrows to one village.

Row: `{person_id, name, village_id, village, last_delivery_at}`, where `last_delivery_at` is null for never. Persons who never delivered come first, then the oldest delivery, then by name. Paged `{data, meta}`. `422` on a malformed `since` (`The since must be a date (YYYY-MM-DD).`) or a malformed `village_id`.

## Corrections

A bought delivery's kg are never edited. A correction is a request, a decision by a second person, and on approval new rows only. Kg only; a price is never corrected.

### `POST …/farm/supply-batches/{batch}/corrections`

ops or admin (officers and farmers `403`). Body `{client_ref, reason (1-500), lines: [{share_id, person_id?, kg}]}` with 1-50 lines. `kg` is the kg the farmer should have on that share line (0 or more, 2 decimals); `person_id` defaults to the share's farmer. A wrong farmer is two lines on one share: the share's farmer to less, the right farmer (a person of the batch's village) to more. The API stores the change against the current effective kg.

`201` pending; the same `client_ref` and body replay `200`; another body or member under it `409`. `422`:
- a share of another batch, a farmer outside the village, a line listed twice, a kg equal to the current one;
- `batch`: the raw lot is sold out (no stock left in any warehouse);
- `lines`: a cut (net kg below 0) when the raw lot or a lot packed from it was dispatched over a supply link, or a cut larger than the raw lot's free stock in its main warehouse;
- `lines`: an increase past the harvest's figure (`over-commitment: …`, the batch create wording).

The "season closed" window of D2-b is not enforced yet: it waits for a season-close feature.

### `GET …/farm/supply-corrections?status&batch_id&page&page_size`, `GET …/farm/supply-corrections/{correction}`

Staff of the batch's village (ops and admin: all). `status` is `pending`, `approved` or `rejected`. Correction: `{id, batch_id, village_id, status, reason, requested_by, requested_at, decided_by, decided_at, net_kg, lines: [{share_id, person_id, farmer_name, kg_delta, kg_before, kg_after}], farmers: [{person_id, name, change, balance_after}]}`. While pending, `kg_before`/`kg_after` and `farmers` preview the approval (money as `{minor, currency, decimals, currency_id}`; a negative `balance_after` means the farmer will owe the co-op). Once approved, `farmers` lists the posted passbook amounts and the kg fields are null. Rejected: no farmers.

### `POST …/farm/supply-corrections/{correction}/approve`, `…/reject`

ops or admin other than the requester (`403`; the schema also refuses it: `CHECK (decided_by <> requested_by)`). Decided once (`409`). Reject records the decision only.

Approve, one transaction, with every window checked again under the batch lock:
1. The decision row; effective kg per share and farmer must stay ≥ 0 (`422` when an earlier correction changed it).
2. Net kg ≠ 0: one stock movement on the raw lot (`related_domain` `farm_supply_correction`, `is_correction`), floor = free stock.
3. Every sale and return attribution of the batch is re-split with `credit::split` over the corrected kg. The difference with what each farmer holds for that attribution (passbook lines plus earlier splits) is stored per attribution in `farm_supply_correction_splits`; each attribution's rows sum to 0, so the batch's credited total never changes. More than 5,000 attributions: `422`.
4. One `supply_correction` passbook line per farmer whose money moved (their sum), with `correction_id`; the passbook read shows the correction's `reason`.
5. `supply.corrected` (thin: `correction_id, batch_id, farmers, net_kg`).

A farmer already paid out may go negative: payouts above the balance are already refused, so nothing more is paid until later credits cover it. Later sales split by the corrected kg; return debits count the splits. Committed harvest kg and the lot passport's farmer groups read effective kg (view `farm_supply_share_kg`).
