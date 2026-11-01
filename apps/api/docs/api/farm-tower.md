# Farm Control Tower: production and data quality

Per-village reads for staff (RUAHA `v_village_production`, `v_village_data_quality`). Code: `src/modules/farm_tower/`. Farmers get `403`; staff of another village, unknown ids and other workspaces `404`. The same check (`farm_tower::service::staff_village`) guards the other per-village reads: [energy and PUE pipeline](farm-pue.md), [supply and demand matches](farm-market.md).

## `GET /v1/workspace/{workspace}/farm/villages/{village}/production`

Live crop cycles by crop and harvest month:

```json
{ "data": [ { "crop_id": "…", "window_month": "2026-09-01", "cycle_count": 1, "plot_count": 1,
  "cycle_area_ha": "1.6000", "tree_count": null, "unit_count": null,
  "expected_kg": "3200.00", "actual_kg": null, "verified_cycles": 0 } ] }
```

`window_month` is the first day of the harvest month, `null` for cycles without a harvest window. `cycle_area_ha` is the **sum of cycle areas, not land area**: intercropped cycles share a plot, so it can exceed the village's hectares. `expected_kg` and `actual_kg` sum the current figures of each kind.

## `GET /v1/workspace/{workspace}/farm/villages/{village}/data-quality`

`{village_id, persons, persons_verified, farms, farms_with_gps, cycles, cycles_with_estimate}`, counting live rows only. `cycles_with_estimate` counts cycles with a current expected harvest figure.

## `GET /v1/workspace/{workspace}/farm/verify-queue/summary`

Any farm role. The records waiting for a second pair of eyes (`verification` `unverified` or `pending`), counted exactly as the caller's verify lists hold them (`GET …/persons?verification=unverified,pending` and the same for households, farms, plots, crop cycles and harvest figures): ops and admin the whole workspace, field officers their villages, a farmer their own records. Every outstanding harvest figure counts, superseded ones included, as the list does.

```json
{ "data": { "persons": 4, "households": 4, "farms": 3, "plots": 4, "crop_cycles": 4, "harvest_reports": 2, "total": 21 } }
```
