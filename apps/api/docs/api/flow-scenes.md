# Your Flow scenes

A member's own Genesy Flow layout per scene: position, hidden, and which components it shows. Display settings only. A component names a kind, a metric key and a period, never a value: the scene's data comes from the normal read routes at render time, so a data change never touches this config, and loading it needs no language model.

All routes need `Authorization: Bearer <token>` and an active membership of `{workspace}`. No extra permission. Each member reads and changes only their own rows (filtered by workspace and user): another member's settings are invisible and untouched, another workspace's paths are 404.

## Vocabulary (Rust is the only authority)

| Field | Values |
|---|---|
| scene key | built-in `pulse`, `stock`, `noticed`, `orders`, `sales`, `routines`, `studio`; custom `sales_*` or `stock_*` (1-26 of `a-z`, `0-9`, `_` after the prefix) |
| `type` | `metric_card`, `line_chart`, `bar_chart`, `comparison` (need `metric` and `period`); `order_list`, `stock_list`, `attention_panel`, `invoice_table`, `report_list` (no `metric`, `period` optional) |
| `metric` | `sales.gross`, `sales.net`, `orders.pending`, `orders.draft`, `stock.below_minimum`, `invoices.overdue`, `invoices.unpaid` |
| `period` | `today`, `yesterday`, `this_week`, `last_week`, `last_7_days`, `this_month` |
| component `id` | `^[a-z0-9-]{1,32}$`, unique in its scene |

At most 8 components and 8 KiB of serialized config per scene; at most 20 saved scenes per member per workspace (built-ins count once saved). Unknown fields anywhere are 422.

Permission per component: `sales.*`, `orders.*` and `order_list` need `order.list`; `invoices.*` and `invoice_table` need `invoice.list`; `stock.*` and `stock_list` need `stock.list`; `attention_panel` (its sections gate themselves) and `report_list` (own schedules) need none.

## Scene object

`{scene, position, hidden, saved, schema_version, config: {components: [{id, type, metric?, period?}]}, restricted: [component id]}`

- `saved` false = Rust default; true = the caller's row.
- `restricted` = ids of components whose permission the caller lacks. Flow shows "not available for your role"; values are never sent.
- `schema_version` is 1.

## GET /v1/workspace/{workspace}/flow/scenes

200 `{data: [scene]}`: every built-in (default unless saved) plus every saved custom scene, sorted by `position`, then key. One SQL statement for the saved rows.

## PUT /v1/workspace/{workspace}/flow/scenes/{scene}

Body `{position?, hidden?, config?}`, at least one. Omitted fields keep the saved value, else the default (a new custom scene lands at position 7). `config` replaces the whole component list. Same body twice = same state (no `Idempotency-Key`).

- 200 `{data: scene}`.
- 403 when `config` adds a component the caller can't read. "Adds" means no component with the same `id`, `type` and permission area (sales/orders, invoices, stock) is in the current config (saved, else default), so swapping `sales.gross` for `sales.net` on a kept card is not adding. A member who lost a right can still hide, move and reorder what they had.
- 422 `scene`: unknown key, or a 21st saved scene. `config`: missing on a new custom scene, or over 8 KiB. `config.components`: more than 8. `config.components.N.<field>`: bad id, repeated id, unknown type/metric/period, metric missing or not allowed for the type, period missing. `position` outside 0-19, `hidden` not a boolean, `body` empty or unknown field.

## DELETE /v1/workspace/{workspace}/flow/scenes/{scene}

204. A built-in goes back to its default, a custom scene disappears. Nothing saved is also 204. Unknown key 422.

## POST /v1/workspace/{workspace}/flow/scenes/reset

204. Drops every row the caller saved in this workspace. Other members' rows stay.

## Storage

Table `flow_scenes` (migration `flow_scenes`): unique `(workspace_id, user_id, scene)`, `position` 0-19, `schema_version` = 1, `config` jsonb with `pg_column_size(config) <= 8192`. Rows cascade on workspace or user delete; no prune. Member removal keeps them (display settings only, no data): a re-added member gets their layout back. A stored config the current vocabulary no longer accepts is skipped with a warning log (ids only): the scene reads as its default (a custom scene drops out) until the member saves it again.

## Voice

The four operations are `internal` in Office voice policy for now: voice can't call them. The Flow UI writes directly with the member's bearer. Turning on voice edits (through the normal review card) is a separate change.
