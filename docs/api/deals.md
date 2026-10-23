# Deals API

Cards on a sales board, ported from vimdesk-api `src/Domain/CRM/Deal`. vimdesk-web calls these routes from `services/sales/deal.service.js` and `deal-pipeline.service.js`, used by:

- the deal board `/sales/deals` (kanban and list mode)
- deal detail `/sales/deals/{id}`
- the Deals tab of a relation

Boards and their columns are in `deal-pipelines.md`. Shared rules (guard order, 404s, list `meta`) are in `conventions.md`. Writes answer:

- create: `201 {"data": {"deal_id": "<uuid>"}}` (Laravel answered 200 with the same body)
- update, drag, add-relation, delete: `{"message": "ok"}`

## Routes

Under `/v1/workspace/{workspace}`:

| Route | Permission | Does |
|---|---|---|
| `GET /deals` | `deal.list` | newest first, `page` / `page_size` (50, max 100); filters below |
| `GET /deals/{deal}` | `deal.get` | one deal (`custom_fields=1` is ignored: custom fields aren't ported) |
| `POST /deals` | `deal.create` | new deal at the end of its column |
| `POST /deals/{deal}` | `deal.update` | patch; a new column or board appends the deal there |
| `POST /deals/{deal}/update-status` | `deal.update` | kanban drag |
| `POST /deals/{deal}/add-relation` | `deal.update` | attach a relation to a deal without one |
| `DELETE /deals/{deal}` | `deal.delete` | soft delete |
| `GET /deal-pipelines/{pipeline}/status/{status}/deals` | `deal.list` | one board column, in the board's order |

**Permissions:** Laravel checked none of these, because `DealPolicy` was never registered. Genesy checks the catalog names (user decision 2026-10-23); the owner and `workspace_admin` pass. The board also needs `dealpipeline.list` / `dealpipeline.get` (see `deal-pipelines.md`).

Not ported:

- archive and un-archive: 403 for everyone in Laravel, no vimdesk-web caller
- the V2 list (returned one raw model)
- the deal exports (`deals/export*`, user decision 2026-10-23)
- the customer-specific deal import

## Resources

**Deal (`DealResource`, list and detail):**

```json
{ "id": "<uuid>", "title": "Big order", "description": "…", "date": "2026-10-23",
  "deal_value": "1250.50000000", "deal_priority": "high", "expected_close_date": "2026-10-31",
  "position": 65536, "archived": false, "created_at": "…", "last_updated_at": "…", "meta": null,
  "relationships": {
    "sales_agent": { "id", "name", "initials", "email", "avatar_path", "color" } | null,
    "relation": <RelationIndexResource> | null,
    "currency": <CurrencyResource>,
    "deal_pipeline": <DealPipelineResource with statuses and summary>,
    "deal_pipeline_status": <DealPipelineStatusResource>,
    "custom_sections": [] } }
```

- `date` is the deal's date, else the day it was created (Laravel `date ?: created_at`).
- `deal_value` is the stored `numeric(24,8)` as text, as Laravel printed its uncast column.
- `position` is the deal's rank in its column (see Drags), not an index.
- `archived` is always `false`.
- `relation` uses the relation list shape (`RelationIndexResource`, with `contacts` and `relation_type`), the one invoices embed. Laravel used `RelationShortResource`; vimdesk-web reads only keys both shapes have: `id`, `type`, `company_name`, `alias`, `relation_nr`, `email`, `phone_number`, `created_at`, `relationships.contacts[].full_name`. A deleted relation prints `null`.
- `sales_agent` is `null` when unset or no longer a member. `currency` is `null` when the deal's currency was deleted (nothing stops that, as in Laravel).
- `deal_pipeline` carries the board's `summary`, as Laravel's did, so a full read also runs the board's won/lost aggregate once per board on the page. It grows with the won and lost columns; vimdesk-web doesn't read it from a deal.

**Board column row (`DealIndexResource`):** the same keys, with only `sales_agent`, `relation` and `currency` under `relationships`.

A page loads its embeds in a fixed number of queries, whatever its size:

- relations and their contacts;
- agents and currencies;
- for the full resource, boards with their columns and summaries.

## Lists

**`GET /deals`** query params:

| Param | Does |
|---|---|
| `q` | ILIKE on the title (Laravel used Meilisearch); blank or `*` = none, over 100 characters = 422 |
| `relation_id` | the relation's deals; must be a uuid (`The relation id must be a valid UUID.`); an unknown one gives an empty list (Laravel returned every deal) |
| `pipeline_id`, `status` | the list view's board and column filters (Laravel ignored them); each must be a uuid (`The pipeline id must be a valid UUID.`, `The status must be a valid UUID.`) |
| `page`, `page_size` | Laravel paging |

**Board column:** `GET /deal-pipelines/{pipeline}/status/{status}/deals` answers `{data, meta}` in pages of 999 (Laravel `paginate(999)`; `page` moves).

- The column must be a live column of that board, else 404. Laravel read any column id.
- The order follows the board's `sorting`:
  - `pos`: rank, then id
  - `a-z`: deal date ascending, deals without a date first (MySQL's NULL order), then id
  - `z-a`: deal date descending, deals without a date last

## Writing

| Field | Create | Update (patch) |
|---|---|---|
| `title` | required text ≤ 255 | text ≤ 255; null is `required` |
| `description` | required text ≤ 50000 | text ≤ 50000; null clears |
| `deal_pipeline_id` | required: a live board (`The deal pipeline id does not exist`) | uuid; with a changed column, moves the deal |
| `deal_pipeline_status_id` | required: a live column of that board (`This status is not one of the deal pipeline statuses`) | uuid; a changed one appends the deal to that column |
| `sales_agent_id` | required uuid of a member (`This sales agent id is not a valid user_id`) | null clears, absent keeps it |
| `relation_id` | a live relation (`This relation does not exist`) | not updatable here (use add-relation) |
| `currency_id` | a live currency (`This currency id does not exist`); absent: the workspace's default, else `No default currency has been found. Please create one` | not updatable |
| `deal_value` | number (any sign, ≤ 8 decimals); absent = 0 | number; null is `required` |
| `deal_priority` | required `high`, `low` or `medium`, any case, stored lowercase (`The selected deal priority: x is invalid.`) | same; null is `required` |
| `date` | `Y-m-d` (`The date does not match the format Y-m-d.`) | same; null clears |
| `expected_close_date` | required `Y-m-d`, today or later (UTC) (`The expected close date must be a date after yesterday.`) | a date; a new one must be today or later, the stored one re-sent may be past |
| `meta` | object or null | object or null |
| `custom_sections` | null, or items without `section_id` (custom fields aren't ported) | same |

- The edit modal sends the user's own id as `id`, plus the unchanged board and column. `id` is ignored, and an unchanged column moves nothing.
- `created_by` is the caller.

**Add relation:** `{relation_id}`, a required uuid of a live relation of the workspace.

- A deal that already has one: 400 `{"message": "This deal already has a relation attached."}`. Laravel printed its missing translation key.
- An unknown deal: 404, after the body checks.

## Drags (`POST /deals/{deal}/update-status`)

Body `{deal_pipeline_status_id, deal_pipeline_id, position}`, all required:

- `deal_pipeline_id`: a live board.
- `deal_pipeline_status_id`: a live column of it. A column deleted a moment ago is refused: the composite FK alone only proves the row existed.
- `position`: an integer 0..10000, the 0-based index the card was dropped at in the target column (vimdesk-web `e.newIndex`), not counting the dragged deal. An index past the end appends. The cap keeps a drag from walking a whole column under its lock; a board page holds 999.

How a drag is stored:

- `position` is a gap rank. New deals append at the column's highest rank + 65536.
- A drop between two neighbours takes the midpoint, so a drag writes one row.
- Only when no integer is left between two neighbours is the column respaced to 65536 apart, once, in the same transaction. That allows 16 drops at one spot before a respace.
- Laravel shifted every card after the drop point in one query, with no lock, and read the old position outside its transaction.

**Concurrency.** Every rank writer (drag, create, a column change on update) locks in one order:

1. the board rows `FOR SHARE`, in id order (both boards for a cross-board move);
2. the source and the target column rows `FOR NO KEY UPDATE`, in id order;
3. the deal.

This gives:

- Drags into or out of one column take turns, so ranks stay distinct and no card is lost.
- Drags between unrelated columns run side by side.
- No deal leaves a column while that column is being respaced, so a respace never rewrites a deal that has just moved, and opposite drags don't deadlock.
- Column edits on the board (`deal-pipelines.md`) wait for running drags and hold new ones off.
- Creates and column changes append by reading the column's last rank only, so they never respace a column.
- A deal moved between the unlocked read and the locks is read again (up to 3 times, then 503). A drag that Postgres still picks as a deadlock victim runs once more.
- A lock wait over 5 s answers 503 with `Retry-After`.

Measured (`tests/load_deals.rs`, a dev Mac with load average around 40 from other builds):

- L-B1: a full 999-deal column reads with p95 31 ms.
- L-B2: 50 concurrent movers over 3 columns of 1,000 deals, 10,000 drags:
  - 0 errors, every deal once, ranks distinct;
  - 553 drags/s;
  - one mover alone: p95 5 ms;
  - contended: p95 156 ms. That is the queue on the column locks: with 3 columns, most drags share a column with about a third of the movers.

## Delete

A soft delete; the deal leaves its column and the board's summary.

## Laravel differences

1. Every route checks its catalog permission.
2. A deal's column always belongs to its board (DB-enforced), and a column change moves the deal there. Laravel's drag checked the column against the body's board but never moved the deal's board.
3. Ranks instead of shifted positions; `position` in the resource is the rank.
4. Update is a patch: an absent `sales_agent_id` keeps it (Laravel cleared it), and absent board or column ids keep them (Laravel wrote NULL and answered 500).
5. A re-sent unchanged past `expected_close_date` saves; Laravel refused every edit of a deal whose close date had passed.
6. `GET /deals` honours `pipeline_id` and `status`, ANDs `q` with the other filters, and gives an unknown `relation_id` an empty list.
7. The relation embed uses the relation list shape (see Resources).
8. Create answers 201.
9. The board column read is 404 for a column of another board.
10. `status_changed_at`, `archived`, `import_source` and `import_id` aren't stored; nothing in vimdesk-web reads them.
