# Farm productive-use (PUE) requests and village energy

A farmer asks to rent or buy a machine from the [catalogue](farm-catalogue.md); ops decides (RUAHA `pue_request`, `energy_estimate`, `v_village_energy`, `v_village_pue_pipeline`). The MVP stops at the decision: delivery, terms and repayment are not modelled. Code: `src/modules/farm_pue/`, migration `farm_pue`.

## Who sees what

- A farmer sees their own requests. Staff see every request of their villages (ops and admin: all). Anything else is `404`.
- Village energy and pipeline are staff reads: farmers get `403`, staff of another village `404`.

## Requests

### `POST /v1/workspace/{workspace}/farm/pue-requests`

Body `{id?, equipment_id, person_id?, farm_id?, acquisition?, quantity?, hours_per_day?, days_per_week?, purpose?, status?}`.

- A farmer files for themselves; `person_id` is ignored. Staff name a person of their villages (`422` on `person_id` otherwise). The village is the person's. `farm_id`, if sent, must be a farm of that village (`422` on `farm_id`).
- `acquisition` is `buy` (default) or `rent`, and the catalogue must offer it: `422 {"message": "this equipment is not offered for rent"}` / `"this equipment is not offered for sale"`.
- `quantity` 1–10000 (default 1). `hours_per_day` 0–24 (2 decimals) and `days_per_week` 0–7 (1 decimal); absent = the catalogue's typical figure is used for the estimate.
- `status` is `submitted` (default) or `draft`; anything else is `422 {"message": "a request may only be created as draft or submitted"}`.
- Provenance is the server's: `source` farmer_reported, `verification` unverified, `captured_by` the caller. `submitted_at`, `decided_*` and the estimate are never read from the body.
- A client `id` makes retries safe: same body `200`, different body or an id used elsewhere `409 {"message": "This id is already in use."}`.

`201 {"data": request}`:

```json
{ "id": "…", "village_id": "…", "person": { "id": "…", "name": "Neema Mwakalinga" }, "farm_id": null,
  "equipment": { "id": "…", "code": "MILL", "name_en": "Maize mill", "name_sw": "Kinu" },
  "acquisition": "buy", "quantity": 1, "hours_per_day": null, "days_per_week": null, "purpose": "…",
  "status": "submitted", "submitted_at": "…", "decided_at": null, "decided_by": null, "decision_note": null,
  "estimate": { "rated_power_kw": "15.000", "quantity": 1, "hours_per_day": "8.00", "days_per_week": "6.0",
                "est_power_kw": "15.000", "est_kwh_per_day": "120.000", "est_kwh_per_week": "720.000",
                "method": "level1_rated_x_hours", "source": "model_estimated", "confidence": null, "computed_at": "…" },
  "provenance": { "source": "farmer_reported", "captured_by": { "id": "…", "name": "…" }, "verification": "unverified", … },
  "created_at": "…", "updated_at": "…" }
```

### The energy estimate

One per request, written only by the database (`farm_pue_recompute_estimate`): rated kW × quantity, × hours a day, × days a week. The inputs are copied when it is computed, so a later catalogue change does not move a stored estimate; changing a request's machine, quantity, hours or days recomputes it, a status change does not. A machine with no rated power has no estimate (`null`). No route writes an estimate.

### `GET /v1/workspace/{workspace}/farm/pue-requests?village_id&status&page&page_size`

Newest first. `status` is a comma-separated list (`submitted,under_review` counts open requests through `meta.total`).

### `GET /v1/workspace/{workspace}/farm/pue-requests/{request}`

### `POST /v1/workspace/{workspace}/farm/pue-requests/{request}/transitions`

Body `{action, note?}`. `200 {"data": request}`.

| action | moves | who |
|---|---|---|
| `submit` | draft → submitted (stamps `submitted_at`) | the applicant, village staff |
| `withdraw` | draft or submitted → withdrawn | the applicant, village staff |
| `start_review` | submitted → under_review | ops, admin |
| `approve` | under_review → approved (stamps `decided_at`, `decided_by`) | ops, admin |
| `reject` | under_review → rejected (stamps `decided_at`, `decided_by`) | ops, admin |

Any other move is `422 {"message": "illegal transition {from} -> {to}"}`; a legal review move by someone else is `403 {"message": "only ops or admin may review a request"}` (legality is checked first, as in RUAHA). `note` is kept as `decision_note`. There is no content edit route: a request keeps what it asked for.

## Village energy

### `GET /v1/workspace/{workspace}/farm/villages/{village}/energy`

```json
{ "village_id": "…", "capacity_kw": "500.00", "capacity_basis": "planned", "simultaneity_factor": "0.600",
  "prospective_kw_raw": "15.000", "approved_kw_raw": "18.000",
  "prospective_peak_kw": "9.000", "approved_peak_kw": "10.800", "headroom_kw": "489.200",
  "approved_kwh_per_week": "828.000", "approved_assets": 2 }
```

Prospective (submitted, under review) and approved load are never summed. Peak = raw × the current capacity row's simultaneity factor (village peak is not the sum of rated power); headroom = capacity − approved peak. Capacity is `planned` or `nameplate`, never measured. Every village has a capacity from creation, so it has figures from the start.

### `GET /v1/workspace/{workspace}/farm/villages/{village}/pue-pipeline`

One row per status and currency: `{status, request_count, indicative_value, indicative_rent_per_day}`. Buy requests add catalogue price × quantity to `indicative_value`; rent requests add rent per day × quantity to `indicative_rent_per_day` (a rent is not a purchase price). Money is `{minor, currency, decimals, currency_id}`, `null` when no request of that kind. INDICATIVE: not financed value, not a loan book.
