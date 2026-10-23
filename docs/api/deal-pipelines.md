# Deal pipelines API

Sales boards and their status columns. Ported from vimdesk-api `src/Domain/CRM/DealPipeline`. vimdesk-web calls these routes from `services/sales/deal-pipeline.service.js` and `deal-pipeline-status.service.js`, used by the deal board (`/sales/deals`) and the pipeline settings (`/settings/pipelines`).

A column is a `deal_pipeline` custom status (`/custom-status?domain=deal_pipeline`; signup seeds Won, Lost, In Progress, To Do and Completed) placed on a board at a position. Each column also carries lane rules and won/lost flags.

Shared rules (guard order, 404s, list `meta`) are in `conventions.md`. Writes answer:

- pipeline create: `201 {"data": {"deal_pipeline_id": "<uuid>"}}`
- status create: `201 {"data": {"deal_pipeline_status_id": "<uuid>"}}`
- updates and delete: `{"message": "ok"}`

## Routes

Under `/v1/workspace/{workspace}/deal-pipelines`:

| Route | Permission | Does |
|---|---|---|
| `GET /` | `dealpipeline.list` | boards, oldest first, `page` / `page_size` (50, max 100), each with its columns |
| `GET /{pipeline}` | `dealpipeline.get` | one board with its columns |
| `POST /` | `dealpipeline.create` | new board |
| `POST /{pipeline}` | `dealpipeline.update` | patch a board |
| `POST /{pipeline}/status` | `dealpipelinestatus.create` | new column |
| `POST /{pipeline}/status/{status}` | `dealpipelinestatus.update` | patch a column (edit modal, reorder) |
| `DELETE /{pipeline}/status/{status}` | `dealpipelinestatus.delete` | soft-delete a column |

A status route names a column of the board in its path. If the column belongs to another board, the answer is 404.

**Permissions:** Laravel checked only `dealpipeline.create`, `dealpipeline.list` and `dealpipelinestatus.create`. Every other route was open to any member. Genesy checks the catalog name on every route (user decision 2026-10-23). The owner and `workspace_admin` pass every check. A team that uses the deal board needs `dealpipeline.list` and `dealpipeline.get` too: the board and the deal edit modal load the pipelines.

Not ported, because no vimdesk-web screen reaches them:

- `DELETE /{pipeline}` (the settings list calls a store method that doesn't exist)
- `GET /{pipeline}/status/{status}` (only the never-rendered `StatusSelect` used it)
- `POST /{pipeline}/duplicate` (no Laravel route either)

`GET /{pipeline}/status/{status}/deals` (one board column of deals) comes with the deals module (B8-2).

## Resources

**Pipeline:**

```json
{ "id": "<uuid>", "name": "Sales", "type": "sales", "sorting": "pos",
  "created_at": "…", "last_updated_at": "…", "meta": null,
  "summary": { "won":  { "total_amount": 0, "total_values": [] },
               "lost": { "total_amount": 0, "total_values": [] } },
  "relationships": { "statuses": [<status>, …] } }
```

- `sorting` sets the order of deals in a column. `a-z` and `z-a` sort by deal date, and `pos` sorts by deal position (Laravel `SortingEnum`).
- `summary` counts won and lost deals, with sums per currency. It stays at zero until the deals module lands. The empty `total_values` prints `[]`, as PHP's empty array did.
- `relationships.statuses` holds the live columns in position order. A full page of boards loads all its columns in one query.

**Status (column):**

```json
{ "id": "<uuid>", "position": 0,
  "rules": { "max_items_per_lane": 0, "max_days_in_lane": 0, "auto_archive_in_days": 0 },
  "properties": { "lane_color": "#000000", "deal_probability": "0", "is_won": false, "is_lost": false },
  "created_at": "…", "last_updated_at": "…", "meta": null,
  "relationships": { "custom_status": <CustomStatusResource> | null } }
```

- `custom_status` is null once that custom status is deleted. vimdesk-web shows `N/A` then.
- `rules` are stored and shown only, as in Laravel. Nothing enforces lane limits or auto-archive.

## Writing a pipeline

| Field | Rule |
|---|---|
| `name` | required on create, text ≤ 255; on update absent keeps it, null is `The name field is required.` |
| `type` | optional text ≤ 255 (vimdesk-web sends `sales`, `recruitment` or `custom`); null clears it |
| `sorting` | `a-z`, `pos` or `z-a` (`The selected sorting is invalid.`); missing or null on create gives `a-z`, on update keeps the stored one |
| `meta` | object or null |

The settings page saves `{name}` alone before a reorder. The update is a patch, so `type` and `sorting` stay.

## Writing a status

| Field | Create | Update |
|---|---|---|
| `deal_pipeline_id` | required uuid | required uuid |
| `custom_status_id` | required: a live `deal_pipeline` custom status of the workspace (archived ones count), else `The custom status id does not exist`; already a column of this board: `This custom status id already exist in the dealPipeline` | ignored (the column's custom status never changes) |
| `position` | required integer ≥ 0 | optional; null or blank keeps it |
| `rules` | required object | optional; replaces all three when sent |
| `properties` | required object | optional; replaces all four when sent |
| `meta` | object or null | object or null |

- `deal_pipeline_id` is checked as Laravel did, but the path picks the board. Laravel created the column on the body's board.
- `rules` needs `max_items_per_lane`, `max_days_in_lane` and `auto_archive_in_days`, each an integer (a digit string counts). Any failure is `The rules must be an available rules.`. Laravel's value-object validator fell back to that text for every error.
- `properties` needs `lane_color` and `deal_probability` (non-blank strings ≤ 255; vimdesk-web sends the probability as text) and `is_won` / `is_lost` (Laravel booleans). Any failure is `The properties must be an available properties.`. Two more texts:
  - `Properties field is not valid, can not have is_won and is_lost field both checked at the same time`
  - `You already have a Deal Pipeline status with the same properties values`: another live column of the board is already won (or lost).
- The edit modal posts the whole board column back (`id`, `title`, `tasks`, `loading`, `custom_status_id`…). Unknown keys are ignored.

**Positions.** Positions are 0-based and distinct per board. Every column write locks the board row first, so parallel writes on one board run one after another.

- When the target position is taken, that column and every column after it move one to the right. This is Laravel's `checkPosition` cascade, done as one statement.
- A free position moves nobody, so gaps stay.
- The settings page reorders with one request per column, `{deal_pipeline_id, deal_pipeline_status_id, position: i}` in the new order. That gives exactly the new order.

**Uniqueness.** The rules above are also database indexes on live columns: one per (board, custom status), at most one won and one lost column per board. A race the lock misses still answers the same 422.

**Delete.** A soft delete frees the custom status for a new column on that board. Moving the column's deals to a fallback column comes with the deals module.

## Laravel differences

1. Every route checks its catalog permission (see Permissions).
2. Saving the won (or lost) column with its own flag still set works. Laravel counted the column being edited, so it refused.
3. `type` is stored on create. Laravel's store rules had no `type` and dropped it.
4. A status write on another board's column is 404. Laravel's update ignored the path's board.
5. Unknown or malformed `custom_status_id` / `deal_pipeline_id` in a body is a 422 field error. Laravel answered 404 or 500 before validating.
6. A status update without `position` keeps it. Laravel answered 500.
7. `position` must be ≥ 0. Laravel took any integer.
8. `meta` must be an object. Laravel's `MetaRule` checked nothing.
