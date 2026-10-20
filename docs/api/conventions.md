# CRUD and money conventions

Every workspace CRUD module follows these rules. Batch 4 (reference data) set them; CRM, catalog and the order/invoice batches reuse them. Code: `src/http/{workspace,list,json}.rs`, `src/money.rs`. Worked examples: `src/modules/{currencies,vat_groups,hourly_rates}`.

## Routes and errors

- Paths match vimdesk-api: `/v1/workspace/{workspace}/<resource>` plus `/{id}`. Update is `POST /{id}` (no PUT/PATCH).
- Responses:
  - create: `201 {"data": {"<x>_id": "<uuid>"}}`
  - get: `{"data": {…}}`
  - list: `{"data": […], "meta": {current_page, from, last_page, per_page, to, total}}`
  - update and delete: exactly `{"message": "ok"}`. vimdesk-web closes its modals only on that body; a `data` key breaks it.
- Guard order: 401 (no token), then 404 (outsider, or an unknown, malformed or deleted workspace), then 403 (inactive member, or missing permission via `Can<P>`), then 422. The permission check runs before body parsing, as Laravel's FormRequest `authorize()` did.
- A malformed, unknown, soft-deleted or other-workspace `{id}` is 404 with `The requested resource could not be found.` Laravel answered 422 or 500 for some of these.
- Permission names come from the catalog (`src/modules/permissions/catalog.rs`). Where Laravel checked `x.view` and the catalog only grants `x.get`, genesy checks `x.get`.

## Tables

- Columns: `id uuid DEFAULT uuidv7()`, `workspace_id uuid NOT NULL REFERENCES workspaces ON DELETE CASCADE`, the module's own columns, `meta jsonb` (object or null), `created_at`, `updated_at`, `deleted_at`.
- `CONSTRAINT <table>_workspace_id_id_key UNIQUE (workspace_id, id)`. List queries use it, and it is the target of **composite tenant FKs**: `FOREIGN KEY (workspace_id, vat_group_id) REFERENCES vat_groups (workspace_id, id)`. The database then refuses a reference to another workspace's row, even if an app check is missed.
- Delete is soft (`deleted_at`). Every read, update and delete filters `workspace_id` and `deleted_at IS NULL`.
- Uniqueness lives in partial unique indexes on live rows, case-insensitive where Laravel's MySQL collation was: `(workspace_id, domain, lower(name)) WHERE deleted_at IS NULL`. The service pre-checks for the friendly 422; the index catches races, and its violation maps to the same 422.

## Request bodies

- `ValidatedJson<T>` + `impl Validate`. Errors are `{"message", "errors": {field: [text]}}`. Dot keys for nested fields: `properties.discount_percentage`, `countries.0.price`.
- Wording comes from vimdesk-api `resources/lang/en/validation.php`:
  - "The X field is required."
  - "The X must be a number."
  - "The X must be at least N."
  - "The X must not be greater than N."
  - "The X must be an integer."
  - "The X field must be true or false."
  - Text length keeps the existing genesy wording "The X field must not be greater than N characters."
- Helpers on `Rules`:
  - `text` (required trimmed text), `opt_text` (blank = null)
  - `patch_text`, `patch_opt_text`
  - `required_value`, `decimal`, `boolean`, `integer`, `meta`, `country`
  - free fns: `filled` (null and blank = missing), `is_country`
- `boolean` accepts Laravel's set: `true`, `false`, `1`, `0`, `"1"`, `"0"`. Output is always a JSON bool (vimdesk-web compares `=== true`).
- Numbers arrive as JSON numbers or strings: edited inputs send `"21"`, `"12.5"` or `""`. `""` counts as missing.
- **Updates are patches.** Declare optional fields `#[serde(default, deserialize_with = "present")] Option<Option<T>>`:
  - absent = keep
  - `null` = clear on a nullable column, 422 "required" on a NOT NULL one (Laravel's answer there was a 500)
  - Fields that Laravel's update rules mark `required` stay required.
- The repo applies a patch in one `UPDATE … SET col = COALESCE($n, col)` / `CASE WHEN $sent THEN $value ELSE col END` statement, so two concurrent edits can't drop each other's fields. A cross-field rule that depends on the stored row is a DB `CHECK`, mapped to a 422 (`shipment_methods_weight_range`).
- Read-only keys the client echoes back (`id`, `created_at`, `workspace_id`) are ignored.

## Lists

- `Page::parse(page, page_size)`: default 50, max 100, junk falls back. A module with a different paging param builds `Page` itself (`custom-status` `limit`, 1..200, 422 outside).
- One query: `count(*) OVER ()` on the page, `ORDER BY id` (uuidv7 = insertion order, Laravel's de-facto order). A page past the end has no row to carry the total, so the service runs a separate `count` when the page is empty and `offset > 0`.
- `q`: `list::search` trims it; blank or `*` means no filter, more than 100 chars is 422. It returns an escaped `%q%` for `ILIKE` on the name column.
- Unknown query params are ignored: vimdesk-web spreads its route query into list calls.
- Query flags use Laravel `$request->boolean()` semantics (`list::flag`: `1/true/on/yes` = true).

## Search registry

`GET /searches?domain=X` (`src/modules/search/handlers.rs`, permission `workspace.global-search`) is what every vimdesk-web dropdown calls:

- Each ported domain is one match arm calling the module's `service::list` with `q`. It answers in that module's list shape.
- A domain that isn't ported, or no domain at all (Laravel's global search), is 404.
- A new module adds its arm. Laravel searched Meilisearch; genesy uses ILIKE in insertion order, which is fine for reference lists.

## Money and decimals

Think of it as a till that counts exact coins, not a calculator that shows 0.1 + 0.2 = 0.30000000000000004.

| What | Column | Rust | JSON |
|---|---|---|---|
| Rates, unit prices, percentages, quantities, weights | `numeric(24,8)` | `rust_decimal::Decimal` | string from `money::format_decimals`, or as the resource did in Laravel |
| Stored document totals (order/invoice batch) | `bigint` cents | `i64` | per that batch |

- `numeric(24,8)`: 16 integer digits, 8 decimals, fits `Decimal` (Laravel's `decimal(40,8)` doesn't). Input past that range is a 422.
- Input: `Rules::decimal(field, value, min, max)` → `Decimal`. At most 8 decimals; `money::MAX` is the column ceiling.
- Rounding: `money::round` = half away from zero, PHP `round()`. Never `f64` for money; a weight may be printed as a JSON number when Laravel printed a double.
- Output: `format_decimals(value, dp, rtrim, zero)` = vimdesk-api `MoneyHelper::formatDecimals`:
  - `None` gives `"0.00"`.
  - Otherwise: round to `dp`, print `dp` digits, trim trailing zeros when `rtrim`, then a trailing `.`.
  - An exact zero depends on how Laravel held the value: `Zero::Falsy` (a `float` cast, or computed) gives `"0.00"`; `Zero::Column` (an uncast decimal column string) gives `"0"`.
  - Pick `dp` and `zero` per field from the Laravel resource and model casts:

    | Field | dp | zero |
    |---|---|---|
    | VAT `vat_amount` | 2 | Falsy |
    | hourly prices | 2 | Column |
    | product price-like fields | 4 | Column |
    | `*_incl` | 2 | Falsy |
- Oracle: `tests/fixtures/format_decimals.json` holds 207 cases produced by PHP 8.4 running the real `MoneyHelper.php` (prod is PHP 8.4 too). `money::tests::matches_php_oracle` runs all of them.
- Raw columns that Laravel printed unformatted (`shipping_costs` → `"780.00000000"`) print `Decimal` rescaled to 8.

## Tests

- One file per module under `tests/`. `common::Api` holds the shared access checks every route needs:
  - `assert_member_forbidden`: 403 before 422 on all five routes
  - `assert_granted`: a member with the five permissions can do everything
  - `assert_foreign_404`: another tenant gets 404 and never sees the row in its list
  - `assert_bad_ids_404`: malformed, unknown and soft-deleted ids
- Signup seeds reference rows (currencies, statuses, custom types), so a test that counts rows takes a baseline first, or uses names and domains the seed doesn't use.
