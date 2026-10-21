# Stock API

Stock levels per product, warehouse and bin, the movement ledger, and storage locations. Ported from vimdesk-api `src/Domain/ERP/Stock` and `src/Domain/ERP/StorageLocation`.

Shared rules (guard order, 404s, list `meta`, money printing) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}`.

## Model

- **One live row per slot:** product + warehouse + bin. A row without a bin is the warehouse-level row.
  - The unique index `stocks_slot_unique` enforces this. Laravel had none and picked the "first" row.
  - A bin row also stores the bin's shelf. A composite FK keeps the bin inside the row's warehouse and workspace.
- **Columns:**
  - `stock_lvl` and `reserved` keep Laravel's `decimal(19,2)`. Input is rounded half away from zero, as MySQL did.
  - `minimal_stock_level` and `maximum_stock_level` are integers. Numeric input is rounded, as MySQL did.
- **The ledger:** every change appends one `stock_movements` row in the same transaction. `movement_type` is `increase`, `decrease`, `set_level` or `set_requirements`.
  - The row carries the change, the level before and after, min/max before and after, the reason, `is_correction`, the user, an optional cause (`related_domain` + `related_id`) and `meta`.
  - A refused write leaves no ledger row.
- **Locking:** every write locks its row (`FOR NO KEY UPDATE`, 5 s lock timeout) before it reads the level.
  - Parallel writes on one slot queue up; none is lost.
  - The first write on an empty slot creates the row. Two creators meet on the unique index.
- **The shared writer `stock::repo::mutate(&mut PgConnection, …)`** is the only increase/decrease path; order flows (final batch) reuse it. Its `Floor` sets how far a decrease may go:
  - `None`: stores (Laravel `MutateStock` let them go negative)
  - `Zero`: other warehouses
  - `Available`: level − reserved stays ≥ 0. This is Laravel's `EnoughStockRule` on the HTTP route.
- **Product create seeds zero rows** (Laravel `EnsureProductWarehouseStockEntries`):
  - When the product has `has_stock` and the workspace has a main (default) warehouse, it gets a zero warehouse-level row there and in every store. No movements are written.
  - The main warehouse becomes the product's default only when none was chosen. Laravel overwrote a chosen one.

## Writes

Every write answers `{"message":"ok"}`.

| Route | Permission |
|---|---|
| `POST /products/{id}/stocks` | `stock.mutate` |
| `POST /products/{id}/set-stock-levels` | `stock.set_stock_levels` |
| `POST /products/{id}/set-stock-requirements` | `stock.set_stock_requirements` |
| `DELETE /products/{id}/stocks?warehouse_id=&bin_id=` | `stock.set_stock_levels` (as in Laravel) |
| `POST /products/{id}/move-stock-between-bins` | `stock.move` |

### `POST /products/{id}/stocks` (mutate)

- **Body:**
  - `warehouse_id` (required; a live warehouse of the workspace, else "This warehouse id does not exist")
  - `mutation_value` (required number, may be negative)
  - `reason` (≤255; required when `is_correction` is true)
  - `is_correction` (bool)
  - `meta` (object)
  - `bin_id` (a live bin of that warehouse). Laravel's request dropped `bin_id`, so its HTTP mutate always hit the "first" row.
- **Rules:**
  - The available stock (level − reserved) must cover a decrease, in every warehouse, stores included. Otherwise 422 on `mutation_value`: "The mutation value is higher then the available stock" (Laravel `EnoughStockRule`).
  - Laravel checked that before the write; here it is checked under the row lock, so two parallel decreases can't both pass.
  - An increase is always allowed. Laravel's rule also refused an increase that left a negative level below 0.
  - A product that doesn't track stock (not physical, or `has_stock` off), or a zero change, is a no-op.
  - `reserved` in the body is ignored: reservations come with orders. Laravel also required `stock.reserve` for every mutate; that check isn't copied.

### `POST /products/{id}/set-stock-levels`

- **Body:** `warehouses[]` (1–100 entries), each with:
  - `warehouse_id`
  - `stock_lvl` (≥ 0)
  - `reason` (required, ≤255)
  - optional `bin_id`
  - optional `minimal_stock_level` / `maximum_stock_level` (≥ 0; max ≥ min)
- **Errors** name the entry: `warehouses.0.stock_lvl` with "The warehouses.0.stock lvl must be at least 0.", or "This warehouses.0.warehouse id does not exist".
- **Behaviour:**
  - All entries run in one transaction. Each entry is its own slot: Laravel leaked one entry's `bin_id` into the next.
  - An absent min or max keeps its value. Laravel logged it as changed to "not set".
  - **Movement:** `set_level` when the level changed, `set_requirements` when only min/max changed, none when nothing changed.
  - **Reason when only min/max changed:** `Min: 0.00 → 2.00, Max: 0.00 → 8.00. <reason>`. Laravel glued in a double space; it is one space here.
- **Not copied:** Laravel's hard-coded user list for workspace 3.

### `POST /products/{id}/set-stock-requirements`

- **Body:**
  - `warehouses[]`, each with `warehouse_id`, `minimal_stock_level` and `maximum_stock_level` (all required; max ≥ min)
  - optional `default_warehouse_id`
- **Behaviour:**
  - Writes min/max of each warehouse-level row. Laravel wrote the warehouse's "first" row.
  - Writes a `set_requirements` movement when they changed.
  - A `default_warehouse_id` that differs becomes the product's default warehouse.

### `DELETE /products/{id}/stocks?warehouse_id=&bin_id=`

- Soft-deletes the slot's row, only at level 0.
- The ledger stays.
- **Errors** (400 `{message}`):
  - "Stock not found"
  - "Only stock at level 0 can be deleted"
  - Laravel printed untranslated keys here.

### `POST /products/{id}/move-stock-between-bins`

- **Body:**
  - `warehouse_id`
  - `bin_id_from` / `bin_id_to` (bins of that warehouse; null = the warehouse-level row)
  - `quantity` (≥ 0.01)
  - `reason` (required, ≤255)
- **Behaviour:**
  - A `decrease` on the source and an `increase` on the target, in one transaction.
  - Both rows are locked in one order (warehouse row first, then by bin id), so opposite moves queue instead of deadlocking.
- **Short source** (level − reserved < quantity): 400 `{message}` in Laravel's text, e.g. `Insufficient stock for product  - A in Main for move Available stock: 3. Required: 10`.

## Reads

| Route | Permission | Answer |
|---|---|---|
| `GET /products/{id}/stock-with-bins` | `stock.list` | `{data:[…]}` |
| `GET /products/{id}/configured-bins` | `stock.list` | `{data:[…]}` |
| `GET /products/{id}/stock-movements` | `stock.list` | `{data:[…], meta}` |
| `GET /products/stocks` | `stock.list` | `{data:[…], meta}` |
| `GET /products/stocks/negative` | `stock.list` | `{data:[…], meta}` |

- **Quantities** print as MySQL's `decimal(19,2)` strings (`"4.00"`, `"-3.00"`).
- **Lists without an order in Laravel** run in insertion order.
- **`/products/stocks`** sits before `/products/{product}`, as in Laravel.

### `stock-with-bins`

- Each row is `{id, stock_lvl, reserved, minimal_stock_level, maximum_stock_level, warehouse, shelf, bin}`:
  - `warehouse`: `{id, name}`
  - `shelf`: `{id, shelf_code, shelf_name, row: {id, row_code, row_name, section: {id, section_code, section_name}}}`
  - `bin`: `{id, bin_code, bin_name}`
- A relation that is gone or deleted prints null.

### `configured-bins`

- The live bins that hold a live stock row of the product, grouped by warehouse: `{warehouse_id, warehouse_uuid, bins: [{id, bin_code, bin_name, section, row, shelf}]}`.
- Laravel's `warehouse_id` was its internal int id; genesy has none, so both fields carry the uuid. The clients key on `warehouse_uuid || warehouse_id`.

### `stock-movements`

- **Query:**
  - `warehouse_id`, `shelf_id`, `bin_id`: each filters on its own. Laravel dropped `bin_id` without `shelf_id`, which the web filter sends.
  - `page`, `per_page` (default 15, max 100)
  - A malformed or unknown id matches nothing; Laravel answered 404 or 422.
- **Order:** newest first.
- **`meta`:** Laravel built a paginator but sent only `{data}`, so the web client's paging never worked. genesy sends `meta`.
- **Row** (Laravel `StockMovementResource`):
  - the quantities and min/max before and after
  - `reason`, `is_correction`, `related_domain`, `related_id` (uuid), `meta`
  - `created_at`, and `updated_at` (the same value: the ledger is append-only)
  - `location`: section.row.shelf, names before codes, null without a shelf
  - `relationships`:
    - `warehouse`, `shelf`
    - `bin`, which adds `name` and `code`
    - `user` (`{id, name, email}`)
    - `stock` (the stock row; ids are uuids)
    - `picqer_sync` (always null: Picqer isn't ported)
- No `product` key, as in Laravel.

### `GET /products/stocks` and `/products/stocks/negative`

- **Row** (Laravel `StockResource`):
  - `{id, stock_lvl, reserved, minimal_stock_level, maximum_stock_level, meta}`
  - `relationships.product`: the product in list shape
  - `relationships.warehouse`: the full WarehouseResource
  - Products and warehouses load in one batch per page.
- **`reserved`** is the product's reservations, which arrive with orders, so it is `0` for now.
- **`/negative`:**
  - Shows store rows below 0, lowest first.
  - `q` (escaped, case-insensitive) matches the product name, reference, vendor reference, number and EAN, and the warehouse name and alias.
- **`meta`** has `total` (Laravel's simple paginator didn't; the POS DataTable reads it).
- **Left out:** rows of deleted products or warehouses. Laravel answered 500 there.
- **Product embed:** Laravel's embedded product changed its stock fields depending on lazy loading. genesy prints the one product shape.

## Product resource stock fields

`GET /products`, `GET /products/{id}` (both take `?warehouse_id=`).

- **`stocks`:**
  - The live rows, in Laravel `StockWithoutRelationsResource` shape: `{id, stock_lvl, reserved, minimal_stock_level, maximum_stock_level, warehouse {id, name, alias, color: null, icon: null, is_store, pickup_enabled, location}, shelf, bin, updated_at, meta}`. There is no `product` key.
  - Then a zero `fallback-<warehouse>` row for the default warehouse and for the first store, when they have no row.
- **`total_available_stock`:** the sum of the levels. Whole numbers print without a fraction, as PHP did (`9`, `8.5`, `0`).
- **`total_reserved_stock`:** `0` until orders.
- **`pos_*`** (only for physical products with `has_stock`, else `0` / `not_applicable`). The pick is the highest pickup-enabled row:
  - in `?warehouse_id` when one was asked (none there gives `0` / `unavailable`);
  - otherwise in the default warehouse, then anywhere.
  - Ties keep the oldest row.

## Utilization

- **Warehouse** (GET/list): `round(min(100, occupied / grid × 100), 2)`, as a PHP float.
  - grid = `sections × rows_per_sections × shelves_per_row × bins_per_shelf`; null when the grid is 0.
  - occupied = distinct live bins of the warehouse holding stock with level − reserved > 0.
  - Laravel's `withCount` lost its `DISTINCT` and counted rows, so two products in one bin counted twice.
- **Row:** the same over `number_of_shelves × section.bins_per_shelf`; `0` without bins.
- **Elsewhere** (a warehouse embedded in a product or stock row), occupancy is counted as well. Laravel showed 0 there.

## Storage locations `/erp/storage-locations`

| Route | Permission | Answer |
|---|---|---|
| `POST /` | `warehouse.update` | `201 {data:{storage_location_id}}` |
| `DELETE /{id}` | `warehouse.update` | `{"message":"ok"}` |
| `GET /{parent_domain}/{parent_id}` | `warehouse.get` | `{data:[…]}` |

- **Permissions:** Laravel had no catalog entries here (owner/admin only), and its GET had no check at all. These names follow the user's decision for warehouse locations.
- **Body:**
  - `parent_domain`: one of product / order / transfer_list / work_order / warehouse, lowercased. Otherwise "The selected parent domain: X is invalid."
  - `parent_id`
  - `warehouse_location_id` or `bin_id` (one is required: "… field is required when … is not present.")
  - `description` ≤1500
  - `replace` (bool)
  - `meta` (object)
- **Parents:** only `product` is ported. Other domains, and an unknown product, location or bin, answer 404.
- **From a bin:**
  - Its live section, row and shelf codes plus the bin code, joined with dots (empty and `"0"` codes dropped), name a warehouse location of the bin's warehouse.
  - It is found or created once, under a lock, with meta `{generated_from: "bin_hierarchy", bin_uuid}`.
- **`replace: true`** soft-deletes the product's other storage locations first.
- **Row:** `{id, warehouse_location: {id, location, warehouse (compact), meta, created_at, last_updated_at}, description, meta}`. When the location is gone: `{"warehouse":{"name":"N/A"}}`.
- **Location delete:** deleting a warehouse location (one or in bulk) soft-deletes its storage locations in the same statement (Laravel's model `deleted` hook).

## Not ported

- **Zero callers, and guarded by `stock.view` (a permission in neither catalog, so owner/admin only in Laravel):** `GET /products/stocks/{stockId}`, `/stocks/ean/{ean}`, `/stocks/reference/{ref}`, `/stocks/id/{id}`.
- **Zero callers:** `POST /products/{id}/move-stocks`.
- **Not routed in Laravel:** `GetStockHistory`.
- **Picqer:** stock sync, the `picqer_sync` relation, and blocking local mutations.
- **Reservations:** `reserved` from orders and transfer lists (final batch).
- **Second-chance NLA:** a product going `nla` when out of stock.
- **Caches:** order-advice cache invalidation.
