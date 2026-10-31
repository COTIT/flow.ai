# Farm records: registration, reads, corrections, verification, harvests

The RUAHA identity and production records. Code: `src/modules/farm_records/`, `src/modules/farm_crops/`, migration `farm_records`. Tenancy and the farm guard are in `farm-tenancy.md`.

## Records and provenance

Persons, households (with members), farms (with managers), plots, crop cycles and harvest reports. Each row belongs to one village. Every child row carries its parent's workspace and village in its foreign key, so a farm can't sit in another village's household, a manager can't come from another village, and so on.

Every record carries a `provenance` block:

| Field | Meaning |
|---|---|
| `source` | `farmer_reported`, `field_verified`, `transaction_derived`, `sensor_derived` or `model_estimated` |
| `captured_at`, `captured_by {id, name}` | who wrote the current values, and when |
| `verification` | `unverified`, `pending`, `verified` or `disputed` |
| `verified_by {id, name}`, `verified_at` | set by the verify route only |
| `confidence` | `low`, `medium`, `high` or null |
| `evidence_ref` | free text or null |

Records are never deleted through the API. A soft-deleted row (`deleted_at`) disappears from every read. Every change lands in the farm audit trail.

## Who reads what

- Staff (field officer, ops, admin) read every record in their villages. ops and admin cover all villages.
- A farmer reads what hangs off their own person: themselves and their household co-members, their household, the farms they manage or their household owns, those farms' plots, crop cycles and harvest reports.
- Mixed roles: a farmer in one village who is an officer in another gets staff reads only in the second.
- Anything else is `404`, including another workspace.
- **Phone and GPS** (`phone`, farm and plot `latitude`/`longitude`) are filled only for staff of the record's village. Everyone else gets `null`.

Lists take `?village_id=&page=&page_size=` and come oldest first: `GET /v1/workspace/{workspace}/farm/{persons|households|farms|plots|crop-cycles}`. Each has `GET …/{id}`. Households embed `members [{person_id, is_head}]`; farms embed `managers [{person_id, is_primary}]`.

## Registration: `POST /v1/workspace/{workspace}/farm/registrations`

Field staff of the village. One transaction: all of it is saved or none of it.

```json
{
  "client_ref": "uuid made once when the form opens",
  "village_id": "…",
  "person":    { "given_name": "Neema", "family_name": "Mwakalinga", "phone": "0712 345 678", "confidence": "high" },
  "household": { "label": "Mwakalinga household" },
  "farm":      { "label": "Riverside farm", "latitude": "-7.71", "longitude": "35.71" },
  "plot":      { "label": "Riverside", "area_ha": "1.8" },
  "cycles": [
    { "crop_id": "…", "area_ha": "1.6", "harvest_start": "2026-09-01", "harvest_end": "2026-09-30",
      "harvest": { "quantity_kg": "4100", "confidence": "medium", "reported_for": "2026-09-15" } }
  ]
}
```

- `household` is either new (`label`, default "<family name> household") or `{ "existing_household_id": … }`, which must be a live household of the same village. `is_head` defaults to true for a new household and false when joining. Joining a household that already has a head with `is_head: true` gets `this household already has a head`.
- `farm` is either new (label plus GPS, required: `the farm location is required: capture the GPS position before saving`) or `{ "existing_farm_id": … }` in the same village. The person becomes a manager; primary if the farm has none yet.
- `cycles`: at most 20, one per crop (`each crop may be registered once per plot`). The crop must be active in this workspace (`unknown crop`), and the cycle must carry its crop's measure, e.g. `this crop is measured by area: area_ha is required`.
- `person.phone` is required (`a phone number is required so the farmer can sign in`) and must be a Tanzanian mobile. Any spelling (`+255 712 345 678`, `255712345678`, `0712-345-678`) is stored as `+255712345678`. Anything else: `phone number must be a Tanzanian mobile number, for example +255 712 345 678`.
- Provenance: everything is `field_verified` with the officer as `captured_by`, except the expected harvest, which is `farmer_reported` because the farmer gave the number. Nothing is verified.

Replies:

- `201 {"data": {person_id, household_id, farm_id, plot_id, village_id, crop_cycle_ids, harvest_report_ids, replayed: false}}`.
- The same `client_ref` again from the same user: `200`, the first result with `replayed: true`, and nothing written. From another user: `409`.
- `403` for a non-staff caller or a village outside the caller's villages. `422` for invalid input; the field name is the error key, e.g. `person.phone`, `household.existing_household_id`, `cycles`.

## Corrections: `PUT /v1/workspace/{workspace}/farm/{kind}/{id}`

Field staff of the record's village; a farmer gets `403`, another village `404`. Send every editable field of the section; any other field (`verification`, `source`, `user_id`, …) is a `422`.

| Kind | Editable fields |
|---|---|
| persons | `given_name`, `family_name` (required), `phone` (normalised, may be null) |
| households | `label` |
| farms | `label`, `latitude`, `longitude` |
| plots | `label`, `area_ha`, `latitude`, `longitude` |
| crop-cycles | `crop_id`, `status` (required), `season_label`, `area_ha` / `tree_count` / `unit_count` (exactly the crop's measure), `planted_on`, `harvest_start`, `harvest_end` (end on or after start) |

A correction sets `source = field_verified`, `captured_by` = the corrector, `captured_at` = now, and resets verification to `unverified`. A second officer has to verify again. Replies `200` with the record.

## Verification: `POST /v1/workspace/{workspace}/farm/{kind}/{id}/verification`

For persons, households, farms, plots, crop-cycles and `harvest-reports`. Field staff of the record's village.

- Sets `verified`, `verified_by` = caller, `verified_at` = now; `200` with the record.
- One-way. A record that is already verified, or disputed, gets `422 record is not awaiting verification`.
- **Four eyes on households:** the officer who registered a household can't verify it (`422 a household must be verified by someone other than the officer who registered it`). They can still verify the person.
- Unknown, deleted or another village's record: `404`. Farmer: `403`.

## Harvest figures

A harvest figure is a series. Each crop cycle has one current `expected` and one current `actual` figure at most; older figures stay on record.

- `GET /v1/workspace/{workspace}/farm/crop-cycles/{cycle}/harvest-reports?current=true`: newest first; `current=true` keeps only the current figures. `404` when the cycle isn't visible.
- `POST` the same path, field staff only: `{ "kind": "expected|actual", "quantity_kg": "4100", "source": "farmer_reported|field_verified", "confidence": "medium", "reported_for": "2026-09-15" }`. The cycle is locked; the current figure of that kind becomes `is_current: false`, and the new one is current. `201` with the new figure.

## Crops: `/v1/workspace/{workspace}/farm/crops`

Reference data per workspace: `code` (unique), `name_en`, `name_sw`, `measured_by` (`area`, `tree_count`, `unit_count`), `is_active`. Every farm member reads. ops and admin create (`POST`, `201`) and change (`PATCH /crops/{crop}`). There is no delete; retire a crop with `is_active: false`.
