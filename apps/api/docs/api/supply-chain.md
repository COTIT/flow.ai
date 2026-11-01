# Packing and supply transfers

These routes move supply lots on from the raw lot of a supply batch (`farm-supply.md`). Packing turns kg into a child lot of jars. A transfer sends lot stock over a live supply link (`supply-links.md`) into a new lot of the receiving workspace. Stock moves through the stock seam in the same transaction, never below what a lot holds free. Code: `src/modules/supply_chain/`, migration `supply_chain`.

The raw lot's kg are put into stock with the lot stock route (`POST /lots/{lot}/stock-mutations`, `lots.md`).

## `POST /v1/workspace/{workspace}/lots/{lot}/packings` (`product.update`)

```json
{ "product_id": "…", "code": "ILU-J-2026-01", "units": "10", "raw_kg": "7.50",
  "warehouse_id": "…", "best_before": "2027-09-30", "expires_on": null }
```

- The lot must come from a supply batch or a supply transfer. Otherwise `422 Only a lot from a supply batch or a supply transfer can be packed.`
- A packed lot, or one received already packed, gets `422 A packed lot can't be packed again.` (One packing per lineage, no blends.)
- `units` and `raw_kg` are more than 0 with at most 2 decimals. `product_id` must be a live stock-tracked product here, and `warehouse_id` a warehouse here.
- One transaction:
  - The child lot (`lot.created`): `parent_lot_id`, `pack_raw_kg`, `pack_units`. Kg per jar is `pack_raw_kg ÷ pack_units`, kept as the two exact numbers.
  - `raw_kg` out of the parent in the warehouse (`422` on `raw_kg` when it holds less free).
  - `units` into the child.
  - A `packed` event on the parent.
- Another workspace's lot gets `404`.

`201 {"data": {id, code, public_code, product_id, parent_lot_id, pack_raw_kg, pack_units, best_before, expires_on}}`.

## `POST …/lots/{lot}/transfers` (`product.update`)

`{link_id, warehouse_id, quantity}`:
- The link must be one this workspace granted (`422` on `link_id` otherwise). It must be live (`403 The supply link is not live.` when pending or revoked).
- The lot must come from a supply batch.
- One transaction: the quantity out of the lot in that warehouse (`422` when it holds less free), a `dispatched` transfer and a `dispatched` lot event.
- Another workspace's lot gets `404`.

## `GET …/supply-transfers?page&page_size` (members)

Transfers this workspace sent (`direction: "outgoing"`) or is sent (`"incoming"`), newest first.

Transfer: `{id, link_id, direction, partner: {id, name}, from_lot: {code, public_code}, quantity, status, dispatched_at, to_lot, received_quantity, received_at}`.
- `status` is `dispatched` or `received`.
- No farmers, kg shares or prices ever appear.
- A workspace that is not a side of a transfer never sees it.

## `POST …/supply-transfers/{transfer}/receive` (`product.update`)

`{product_id, code, warehouse_id, quantity}`:
- The receiving side only: the sender gets `403`; any other workspace gets `404`.
- The link must still be live (`403`).
- Once only (`409`).
- `quantity` is at most the dispatched one (`422`). A shortfall stays on record.
- One transaction: a new lot of this workspace (`lot.created`, origin left empty) holding the quantity in the warehouse. The link's allow-list is copied onto the transfer at this moment, so the received lot keeps it even after the link is revoked.

## Lineage and the public page

A lot's farmers are found by walking back from it, up to 4 steps, through packing parents and received transfers, to the raw lot of a supply batch. A lot that is not reached this way has `farmers: []`.

When the walk crosses a transfer, the allow-list frozen on that transfer when it was received cuts the public groups:
- no `village`: `farmers: []`
- no `farmer_count`: `farmers: null`
- no `harvest_window`: no dates

The same walk gives the kg sold per jar when farmers are credited at sale (a later step).
