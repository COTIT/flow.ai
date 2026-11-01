# Farm resource catalogue: equipment

The machines a farmer may ask to rent or buy (RUAHA `equipment_category`, `equipment`). Config rows, so they carry no provenance block. Prices are **indicative**: not a quotation and not a financial record. Code: `src/modules/farm_catalogue/`, migration `farm_equipment`.

Every farm member reads the catalogue; ops and admin add to it. Rows are never deleted; `is_active = false` retires one.

## Creates are safe to retry

Every create takes an optional client-made `id`. Sending the same `id` with the same body again returns `200` and the stored row, and nothing is written. The same `id` with a different body, or an `id` used in another workspace, gets `409 {"message": "This id is already in use."}`.

## Categories

### `GET /v1/workspace/{workspace}/farm/equipment-categories?active&page&page_size`

`200 {"data": [...], "meta": {...}}`, ordered by English name. `active=true` lists active rows only, `active=false` retired ones only. Each: `id, code, name_en, name_sw, is_active, created_at, updated_at`.

### `POST /v1/workspace/{workspace}/farm/equipment-categories`

ops or admin; others `403`. Body `{id?, code, name_en, name_sw}`: code 1–32 chars and unique in the workspace (`422` on `code` otherwise), names 1–120 chars. `201 {"data": category}`.

## Equipment

### `GET /v1/workspace/{workspace}/farm/equipment?active&page&page_size`

`200`, ordered by English name, `active` as above. Each machine:

```json
{ "id": "…", "category": { "id": "…", "code": "MILLING", "name_en": "Milling", "name_sw": "Kusaga" },
  "code": "MILL-500", "name_en": "Maize mill", "name_sw": "Mashine ya kusaga",
  "rated_power_kw": "15.000", "typical_hours_per_day": "6.00", "typical_days_per_week": "5.0",
  "can_rent": true, "can_buy": true,
  "indicative_price": { "minor": 2200000000, "currency": "TZS", "decimals": 2, "currency_id": "…" },
  "indicative_rent_per_day": { "minor": 5000000, "currency": "TZS", "decimals": 2, "currency_id": "…" },
  "is_active": true, "created_at": "…", "updated_at": "…" }
```

Figures are decimal text (`null` when not set). A null `rated_power_kw` means requests for this machine get no energy estimate. A price is `null` when not set.

### `GET /v1/workspace/{workspace}/farm/equipment/{equipment}`

`200 {"data": machine}`; `404` for an unknown id or another workspace's machine.

### `POST /v1/workspace/{workspace}/farm/equipment`

ops or admin; others `403`.

| Field | Rule |
|---|---|
| `id` | optional uuid (see retries above) |
| `category_id` | required, a category of this workspace (`422` "no such category in this workspace") |
| `code` | required, 1–32 chars, unique in the workspace |
| `name_en`, `name_sw` | required, 1–120 chars |
| `rated_power_kw` | optional decimal ≥ 0, at most 3 decimals |
| `typical_hours_per_day` | optional decimal 0–24, at most 2 decimals |
| `typical_days_per_week` | optional decimal 0–7, at most 1 decimal |
| `can_rent` | boolean, default `false` |
| `can_buy` | boolean, default `true`. At least one of the two must be true: `422` on `can_rent` with `equipment must be offered to rent or to buy` |
| `indicative_price_minor` | optional integer ≥ 0, minor units of `currency_id` |
| `indicative_rent_per_day_minor` | optional integer ≥ 0, minor units of `currency_id` |
| `currency_id` | required, a currency of this workspace (`422` "no such currency in this workspace") |

`201 {"data": machine}`.
