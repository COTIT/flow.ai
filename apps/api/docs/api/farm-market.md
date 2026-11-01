# Farm market: buyers, demand, opportunities, supply

Supply → aggregation → buyer demand → opportunity (RUAHA `buyer`, `buyer_demand`, `opportunity`, `opportunity_supply`, `v_harvest_available`, `v_village_supply`, `v_demand_match`). An opportunity records that a village **can** supply a demand: never a sale, delivery, payment or contract. Code: `src/modules/farm_market/`, migrations `farm_market`, `farm_opportunity_supply`.

## Who sees what

- Buyers and demand are an ops surface: ops and admin read and write; officers and farmers get `403`.
- Opportunities: staff of the opportunity's village read them (ops and admin: all); a farmer sees an opportunity only when their own harvest is in it. Only ops and admin write: others who can see it get `403 {"message": "only project ops or admin may write opportunities"}`, everyone else `404`.
- Supply lines: staff of the village see every line and who attached it; a farmer sees only lines from their own farms, without the attacher.
- Harvest availability: staff of the village, and a farmer for their own cycles. Village supply: staff of the village (farmers `403`). Demand matches: ops and admin.

Every create takes an optional client `id`: same body `200`, a different body or an id used elsewhere `409 {"message": "This id is already in use."}`.

## Buyers

`GET /v1/workspace/{workspace}/farm/buyers?active&page&page_size` and `POST …/buyers {id?, name, channel?, contact_note?}`. `name` 1–120 chars, unique in the workspace. `channel` is `direct` (default), `afm` or `other`; `afm` is a label only, no integration.

## Demand

`GET …/buyer-demands?status&page&page_size` (by window start; `?status=open` and `meta.total` count open demand), `GET …/buyer-demands/{demand}`, `POST …/buyer-demands`:

```json
{ "id": "…?", "buyer_id": "…", "crop_id": "…", "quantity_kg": "5000", "window_start": "2026-09-01",
  "window_end": "2026-10-31", "quality_note": "…?", "delivery_point": "…?",
  "indicative_price_per_kg_minor": 80000, "currency_id": "…", "confidence": "medium" }
```

Born `open` and `unverified`, captured by the caller; status, verification and capture are never read from the body. `window_end` must not be before `window_start`; a price needs its `currency_id`. A demand has no `source` column on purpose: a buyer's stated requirement is a counterparty input, not a fact about the productive economy. The resource embeds `buyer {id, name}`, `crop {id, code, name_en, name_sw}`, `indicative_price_per_kg` as Money (or `null`) and `captured_by {id, name}`.

## Opportunities

`POST …/buyer-demands/{demand}/opportunities {id?, village_id, note?, crop_id?, status?}` (ops, admin). Born `proposed` with the demand's crop and `offered_quantity_kg` 0. A `status` other than `proposed` is `422 {"message": "an opportunity must start as proposed"}`; a `crop_id` other than the demand's is `422 {"message": "opportunity must match the demand crop and project"}`. One opportunity per demand and village (`422` on `village_id`).

`GET …/opportunities?village_id&buyer_demand_id&status`, `GET …/opportunities/{opportunity}`: `{id, demand {id, buyer, quantity_kg, window_start, window_end}, village {id, name}, crop, offered_quantity_kg, status, note, captured_at, captured_by, supply: [line], my_contribution_kg}`. `my_contribution_kg` is the kg from the caller's own farms (`null` for an account with no person record).

`POST …/opportunities/{opportunity}/status {status}`:

| from | to |
|---|---|
| proposed | shared, declined, lapsed |
| shared | accepted, declined, lapsed |
| accepted | declined, lapsed |

Declined and lapsed are terminal. Anything else is `422 {"message": "illegal opportunity transition {from} -> {to}"}`. `accepted` means both sides agreed to talk further: nothing has moved.

## Supply lines

`POST …/opportunities/{opportunity}/supply {id?, harvest_report_id, contributed_kg}` (ops, admin; others who see it `403 {"message": "only project ops or admin may attach supply"}`). Returns the opportunity with its lines; `offered_quantity_kg` is re-summed by the server. A line is `{id, harvest_report_id, crop_cycle_id, plot_id, plot_label, crop_id, farmer {person_id, name}, contributed_kg, captured_by, captured_at}`; `farmer` is the farm's primary manager, `captured_by` is the caller whatever the client sends. Lines are never edited or detached.

Refusals (`422 {"message": …}`, RUAHA's words):

- `supply must reference a current, undeleted expected harvest report`
- `supply requires a live opportunity` (live = proposed, shared, accepted)
- `supply must match the opportunity village, crop and harvest cycle`
- `over-commitment: {available} kg available, {committed} kg already committed, {requested} kg requested` (2 decimals each, the same text supply batches use)

### Committed kg

`farm_harvest_committed_kg(report)` is shared with supply batches. It counts every figure of the report's crop cycle and kind (the series), so a newer figure never frees kg promised on an older one. **Opportunity kg are counted against the expected harvest and supply-batch kg against the actual harvest; the two do not block each other.** Only live opportunities hold kg: declining or lapsing one frees them.

Lock order, the same in both guards: the harvest report rows `FOR UPDATE` (in id order), then the opportunity, then `farm_harvest_committed_kg` in its own statement, so it sees what earlier lock holders committed. Two attaches on one harvest never both pass.

## Availability and the Tower

`GET …/harvest-available?village_id&crop_id&page&page_size`: current expected figures, by harvest start: `{harvest_report_id, crop_cycle_id, village_id, crop_id, plot_id, plot_label, farmer, harvest_start, harvest_end, quantity_kg, confidence, verification, committed_kg, available_kg}`.

`GET …/villages/{village}/supply`: by crop and harvest month: `{crop_id, window_month, window_start, window_end, expected_kg, committed_kg, available_kg, cycle_count, verified_cycles}`.

`GET …/buyer-demands/{demand}/matches` and `GET …/villages/{village}/demand-matches`: one row per open demand and village whose current expected harvests of the demand's crop overlap the demand's window, each harvest counted once however many months the window spans: `{buyer_demand_id, buyer {id, name}, buyer_name, crop_id, demand_kg, window_start, window_end, village {id, name}, available_kg, committed_kg, coverable_kg, coverage_pct, opportunity {id, status} | null}`. `coverable_kg` = least(demand, available); `coverage_pct` has 1 decimal. A comparison, not an allocation: nothing is ranked or assigned.
