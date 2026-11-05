# Warehouses API

Warehouses, their flat scan locations, and the section → row → shelf → bin hierarchy. Ported from vimdesk-api `src/Domain/ERP/{Warehouse,WarehouseLocation,WarehouseSection,WarehouseSectionRow,WarehouseSectionRowShelf,WarehouseSectionRowBin}`.

Shared rules (guard order, 404s, patch updates, list `meta`) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}`. Writes answer:

- create: `201 {"data": {"<x>_id": "<uuid>"}}`
- update and delete: `{"message": "ok"}`

Deletes are soft and don't cascade, as in Laravel. A child of a deleted parent still exists, but every route reaches it through its parents, so it answers 404.

A warehouse or bin holding stock is not deleted: while any live stock row there has a level or an order hold that is not zero (negative store stock counts), `DELETE` answers `409 {"message": "warehouse_holds_stock"}` / `bin_holds_stock`. Move or write the stock off first. Laravel deleted it and stranded the stock.

## Permissions

| Routes | Permission |
|---|---|
| warehouse list / get / create / update / delete | `warehouse.list` / `warehouse.get` / `warehouse.create` / `warehouse.update` / `warehouse.delete` |
| `POST /warehouses/{id}/default` | `warehouse.set_default` |
| every read below a warehouse (locations, sections, rows, shelves, bins, `/warehouses/{id}/bins`) | `warehouse.get` |
| every write below a warehouse | `warehouse.update` |

- **Below-warehouse access is wider than Laravel's** (user decision, 2026-10-21). Laravel checked `warehouselocation.*`, `section.*`, `warehousesectionrow*.*` and `storagelocation.*`. None of those are in the catalog, so only the owner or a `workspace_admin` could pass, and section create/update/list had no check at all. genesy maps them to the two warehouse names above.
- **Laravel's `warehouse.view` is checked as the catalog's `warehouse.get`.**

## Warehouses `/warehouses`

```json
{ "id": "<uuid>", "name": "North West Storage", "description": null, "slug": "north-west-storage",
  "alias": "NW",
  "location": { "address": "Damrak", "house_number": "1", "house_number_addition": null,
                "address2": null, "zipcode": "1012 LG", "city": "Amsterdam", "country": "NL",
                "state": null, "type": "other", "attn": null, "phone_number": null },
  "formatted_location": "Damrak 1\n1012 LG Amsterdam\nNetherlands",
  "default_warehouse": true, "stock_count_enabled": false, "is_store": null, "pickup_enabled": false,
  "auto_generate_transfer_list": false, "total_sum_enabled": false,
  "manager_id": "<user uuid>", "manager_assistant_id": null, "status": "active", "type": "storage",
  "dimensions": { "total_area": 120.5, "storage_height": 8.0 }, "loading_docks": 2, "sections": 3,
  "rows_per_sections": 4, "shelves_per_row": 5, "bins_per_shelf": 6, "utilization": 0,
  "management_contact": {}, "operating_hours": {}, "additional_settings": {}, "meta": null,
  "created_at": "…", "last_updated_at": "…",
  "relationships": { "custom_sections": [], "children": [<compact>],
                     "user": <user>, "manager": <user>, "manager_assistant": null } }
```

- **Resource details:**
  - `formatted_location` is left out when there is no location, as in Laravel.
  - `location` is Laravel `Address::toSingleArray`: the zipcode is uppercased, and `""` when missing.
  - `is_store` is null, true or false. The POS treats only `true` as a store.
  - `utilization`: occupied bins over the grid, % (see `stock.md`, "Utilization"); null without a grid.
  - `parent_id` and `user_id` are not printed; Laravel didn't print them either.
- **Embedded objects:**
  - `<compact>` is WarehouseCompactedResource: `{id, name, slug, alias, is_store, pickup_enabled, parent_name, status, type}`.
  - `<user>` is UserEmailResource: `{id, name, initials, email, avatar_path, color}`.
- **Create:**
  - `name` is required, ≤150 characters.
  - `slug` is Laravel `Str::slug(name)`, fixed at create. A slug already used by a live warehouse in the workspace gives 422 `slug`: "a Warehouse with this slug already exists". Laravel sent only a `message`; genesy also sends `errors.slug`, which vimdesk-web reads. A deleted warehouse frees its slug; Laravel answered 500 there.
  - `parent_id` must be a live warehouse of this workspace ("The parent id is an invalid id for the warehouse domain.").
  - `user_id`, `manager_id`, `manager_assistant_id` must be **members of this workspace** ("The selected manager id is invalid."). Laravel accepted any user, which would embed another tenant's name and email. Composite FKs into `workspace_members` enforce this in the DB, and a member who leaves is cleared from these fields.
  - `location` uses the Address rules: `address`, `house_number` (numeric), `country`, `zipcode` and `city` are required. Errors use bare keys (`city`), as in Laravel.
  - `status` is one of active / construction / inactive / maintenance. `type` is one of cross-dock / distribution / fulfillment / processing / storage / store.
  - `dimensions`: `total_area` and `storage_height` are numbers ≥ 0. `total_height` is validated, then dropped (Laravel's cast).
  - Integers: `loading_docks`, `sections`, `rows_per_sections`, `shelves_per_row`, `bins_per_shelf`.
  - `management_contact`, `operating_hours`, `additional_settings` and `meta` are objects. The nested rules for contact and hours run on create only.
  - Booleans follow Laravel's set; null means false (`is_store` keeps null).
  - `custom_sections` with a `section_id` gives 422: custom fields aren't ported.
- **Default warehouse:**
  - The first warehouse of a workspace becomes the default unless sent `default_warehouse: false`.
  - `default_warehouse: true` (on create or update) moves the default. `POST /{id}/default` sets it too.
  - **Changing which warehouse is the default needs `warehouse.set_default`**, on every route, so holders of `warehouse.create`/`warehouse.update` get 403 when trying. Laravel only checked it on `/default`. Echoing the stored value is allowed, since vimdesk-web sends `default_warehouse` on every save, and the automatic first default needs no permission.
  - There is one live default per workspace, enforced by a partial unique index plus a per-workspace lock. Deleting the default, or updating it with `default_warehouse: false`, leaves none (Laravel parity).
- **Update** is a patch. `name` is still required (Laravel's update rules), and `parent_id` is ignored. A rename keeps the slug.
- **List:**
  - Filters: `parent_id`, `roots_only` (default true; `0`/`false` lists children too), `pickup_enabled=1`.
  - `page` and `page_size`, oldest first. A malformed or unknown `parent_id` is 422.
  - Each row embeds its live children. Children and people load in one query each per page.

## Warehouse locations `/erp/warehouses/{warehouse}/locations`

Flat, free-text scan labels such as `A.01.02`. They are separate from the bin hierarchy. Inventory reconciliation scans these.

```json
{ "id": "<uuid>", "location": "A.01.02", "warehouse": <compact>, "meta": null,
  "created_at": "…", "last_updated_at": "…" }
```

- **Create / update:** `location` is required, ≤500 characters. `meta` is an object; on update it is a patch.
- **List:** `q` matches the label (ILIKE, ≤100 characters; Laravel used Meilisearch). `limit` is the page size: default 50, capped at 5000 for the reconciliation screen's one-call load. Use `page` for later pages.
- **`DELETE /locations?locations=<uuid>,<uuid>`** soft-deletes the listed ids of this warehouse. Unknown ids are skipped, as in Laravel.
- **A deleted location (one or in bulk) takes its storage locations with it**, in the same statement (Laravel model `deleted` hook; see `stock.md`).
  - An empty list gives 422 "The locations.0 field is required."
  - A malformed id gives "The locations.0 must be a valid UUID."
  - More than 100 ids gives 422; Laravel had no cap.
- A location of another warehouse, or of a deleted warehouse, is 404.

## Sections `/warehouses/{warehouse}/sections`

```json
{ "id": "<uuid>", "section_id": "<uuid>", "warehouse_id": "<uuid>", "section_name": "Hall",
  "section_code": "S1", "category": null, "description": null, "rows": 8, "shelves_per_row": 6,
  "bins_per_shelf": 4, "total_bins": null,
  "layout": { "rows": 8, "shelves_per_row": 6, "bins_per_shelf": 4, "total_rows": 8,
              "total_shelves": 48, "total_bins": 192, "total_capacity": 192,
              "location_format": "S1-GENERAL" },
  "zone": null, "temperature_control": "ambient", "restricted_access": false,
  "priority_level": "standard", "assigned_manager": null, "created_at": "…", "updated_at": "…",
  "relationships": { "warehouse": { "id": "<uuid>", "name": "Main" }, "assigned_manager": null } }
```

- **Fields:**
  - `section_name` is required, ≤255.
  - `section_code` is required, ≤50, and unique per warehouse ignoring case ("The section code must be unique within this warehouse."). Laravel's DB counted deleted rows; genesy doesn't.
  - `category` and `zone` are ≤255.
  - `rows`, `shelves_per_row`, `bins_per_shelf`, `total_bins` are integers ≥ 0.
  - `temperature_control` is one of ambient / cold / freezer / heated, default ambient. `priority_level` is one of low / standard / high / critical, default standard. Both use Laravel's custom texts.
  - `restricted_access` is a boolean.
- **`assigned_manager`** is a **member uuid** ("The selected assigned manager is invalid."). Laravel took an integer user id, which genesy doesn't have.
- **Auto layout** (Laravel SectionCreated). When `rows`, `shelves_per_row` and `bins_per_shelf` are all > 0, create also generates the children:
  - Rows `{code}-ROW-A…` ("Row A"), 10 × 1.2 × 6 m, in the section's temperature.
  - Shelves `{row}-SHELF-{n}` ("Shelf n").
  - Bins `{code}-{letter}{shelf}{nn}` ("Bin A101").
  - It all happens in the same transaction, in three batched inserts.
  - Row letters continue `Z, AA, AB…`. Laravel's `chr(65+i)` broke past Z.
  - A layout over 10 000 bins is 422 on `rows`. A layout that would repeat a bin code (shelves ≥ 11 with bins ≥ 101, Laravel's scheme) is 422 on `bins_per_shelf`; Laravel answered 500.
  - **Workspace quota:** at most 200 000 live bins per workspace ("The workspace can't hold more than 200000 bins.", on `rows` for a layout and on `bin_code` for a single bin). Laravel had no limit. This bounds what a self-signup account can write.
- **Update** is a patch (absent keys keep). Laravel reset `temperature_control`/`priority_level` to defaults when they were omitted; vimdesk-web always sends them, so nothing it does changes. The layout is never regenerated. Huge counts are stored as sent; the `layout` totals saturate instead of overflowing.
- **List:** sorted by `section_name`; `per_page` default 15, max 100 (missing, junk or < 1 means 15). Filters: `category`, `zone`, `priority_level`, `restricted_access`, `search` (name, code or description, ≤255).

## Rows `/warehouses/{w}/sections/{s}/rows`

- **Create:**
  - `row_code` is required, ≤100, unique per section ignoring case ("The row code has already been taken."). Laravel's request checked uniqueness across all workspaces.
  - `row_name` is required, ≤255.
  - `length_meters`, `width_meters`, `height_meters` are required numbers ≥ 0.
  - Optional: `orientation`, `row_type`, `calculated_volume_cubic_meters`, `number_of_shelves`, `shelf_spacing_cm`, `position_x`/`position_y` (any sign), `access_side`, `aisle_width_cm`, `equipment_access`, `priority_level`, `temperature_zone`, `notes`, and the two booleans.
- **Response:** `201 {"data": {"row_id", "warehouse_section_row_id"}}`, the same uuid twice, as in Laravel.
- **Resource:** Laravel's grouped shape. It has `basic_information`, `physical_dimensions`, `shelf_configuration`, `location_access.starting_position{x,y}` and `additional_settings`. It also has:
  - `bins_per_shelf` (the section's) and `total_bins = number_of_shelves × section bins_per_shelf`
  - `utilization`: occupied bins of the row, % as a PHP float (`0`, `50`; see `stock.md`)
  - `relationships{warehouse{id,name}, assigned_manager: null}`
- **Update** is a patch. Null on a required column is 422 "required".

## Shelves `/warehouses/{w}/sections/{s}/rows/{r}/shelves`

- **Create:**
  - `shelf_code` is required, ≤100, unique per row ignoring case.
  - `shelf_level` is a required integer ≥ 1.
  - Optional: `shelf_name`, `side` (left / right), `bins_per_shelf`, `total_storage_bins`, `max_weight_kg` (integers ≥ 0), `notes`.
- **Response:** `201 {"data": {"shelf_id"}}`.
- **Resource:** `{id, shelf_id, workspace_id, warehouse_id, section_id, row_id, shelf_code, shelf_name, shelf_level, side, bins_per_shelf, total_storage_bins, max_weight_kg, notes, created_at, updated_at}`. The parent ids are **uuids**; Laravel printed internal integers.

## Bins `/warehouses/{w}/sections/{s}/rows/{r}/shelves/{sh}/bins`

- **Create:**
  - `bin_code` is required, ≤150, unique per **row** ignoring case (Laravel's DB key).
  - `bin_type` is one of standard / oversize / fragile / hazmat / cold_storage, default standard.
  - `access_type` is one of manual_access / conveyor / robotic_pick, default manual_access.
  - `priority_level` defaults to standard.
  - Optional: `bin_name`, `side`, `position`, `length_cm`/`width_cm`/`height_cm`/`max_weight_kg` (integers ≥ 0), `volume_m3` (a number ≥ 0), `load_rating`, the four booleans, `notes`.
- **Response:** `201 {"data": {"bin_id"}}`.
- **Resource:** Laravel's grouped shape with uuid parent ids. `GET /{bin}` adds `stocks: []` until stock is ported; lists leave the key out.

## `GET /warehouses/{warehouse}/bins`

Every live bin of the warehouse whose whole path is live: `{data: [{id, bin_code, bin_name, warehouse{id,name}, section{id,section_code,section_name}, row{id,row_code,row_name}, shelf{id,shelf_code,shelf_name}}]}`.

- Ordered by section, row, shelf, then bin code.
- Not paginated (Laravel parity), but capped at 10 000 rows. When the cap cuts the list, the body also has `"meta": {"truncated": true}`; otherwise there is no `meta`.
- One query.

## Search `GET /searches?domain=warehouse_location`

Used by vimdesk-web's `WarehouseLocationSelect`. It lists live locations of live warehouses in the workspace, in the location list shape.

- `q` matches the label.
- `warehouse_id` narrows the list to one warehouse; a malformed id matches nothing.
- `page` and `page_size` page the list. Laravel searched Meilisearch.

## Not ported

- `GET /warehouses/bins` (all warehouses) and `GET /warehouses/{id}/location-hierarchy`: no client calls them.
- `GET /warehouses/{id}/download-label-for-locations/{template}`: label printing over FTP.
- Meilisearch indexing, the activity log, the response cache and custom fields (`custom_sections` is always `[]`).
