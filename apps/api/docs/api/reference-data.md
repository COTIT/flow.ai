# Reference data API

Workspace picklists ported from vimdesk-api `src/Domain/WorkspaceSetting/*`:

- currencies
- VAT groups
- hourly rates
- sales groups
- shipment methods
- custom types
- custom statuses

The `/searches` dropdown endpoint is covered at the end.

Shared rules (guard order, 404s, patch updates, list `meta`, money printing) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}`, and update is `POST /{id}`. Every write answers like this:

- create: `201 {"data": {"<x>_id": …}}`
- update and delete: `{"message": "ok"}`

Delete is soft and has no "in use" guard (Laravel parity).

## Currencies `/currencies`

| Route | Permission |
|---|---|
| `GET /`, `GET /{id}` | any active member (vimdesk-web and the POS load the list at boot to format money) |
| `POST /`, `POST /{id}`, `DELETE /{id}` | `currency.create` / `currency.update` / `currency.delete` |

```json
{ "id": "<uuid>", "name": "Euro", "asset_type": "eur", "symbol_prefix": "€", "symbol_suffix": "EUR",
  "decimals": 2, "default_currency": true, "active": true, "meta": null,
  "created_at": "…", "last_updated_at": "…" }
```

- **Create:**
  - `name` required, ≤255
  - `asset_type` required ISO 4217, any case, stored as sent ("Invalid asset type.")
  - `symbol_prefix` / `symbol_suffix` ≤10 (blank = null)
  - `decimals` integer 0..8
  - `default_currency`, `active` required booleans
  - `meta` object
- **Update:** the same fields as a patch. `default_currency: null` means false.
- **One live default per workspace.** Setting a default clears the old one in the same transaction, and a partial unique index catches a race (422 `default_currency`). Deleting the default is allowed and leaves none (Laravel parity).
- **List:** `q` (name), `page`, `page_size`.

## VAT groups `/settings/vat-groups`

Permissions `vatgroup.{list,get,create,update,delete}`. These are new catalog names: Laravel checked them but never catalogued them.

```json
{ "id": "<uuid>", "name": "High Rate", "vat_amount": "21", "default_group": true, "country": "NL",
  "created_at": "…", "last_updated_at": "…", "meta": null }
```

- `vat_amount` is `numeric(24,8)`, required, 0..100, at most 8 decimals. It prints as MoneyHelper 2 dp with a Falsy zero: `21` → `"21"`, `5.5` → `"5.5"`, `0` → `"0.00"`.
- `country` is optional ISO alpha-2, any case, stored uppercase ("country is not valid").
- `default_group` is an optional boolean; null means false.
- Update requires `name` and `vat_amount` (Laravel's rules), and patches the rest.
- **List:** `country` (422 when invalid), `q` (name), `page`, `page_size`. `country` and `q` combine; Laravel ignored `country` when `q` was set.
- **Not ported:** `POST /import` (EU rates from rinvex).

## Hourly rates `/hourly-rates`

Permissions `hourlyrate.{list,get,create,update,delete}`.

```json
{ "id": "<uuid>", "title": "rate 1", "description": "hourly rate", "consumer_price": "1000",
  "business_price": "800", "ledger_number": "test", "purchase_price": "40.12",
  "created_at": "…", "last_updated_at": "…", "meta": null,
  "relationships": { "vat_group": { …VAT group resource… } } }
```

- Prices are `numeric(24,8)`, ≥0. They print as MoneyHelper 2 dp with a Column zero: a zero price is `"0"`, a null `purchase_price` is `"0.00"`.
- **Create:**
  - `title`, `description` required, ≤255
  - `consumer_price`, `business_price` required
  - `purchase_price` optional
  - `ledger_number` ≤55
  - `vat_group_id` required: a live VAT group of this workspace ("The vat group id is an invalid id for the vat_group domain."). A composite FK enforces the same in the database.
- **Update:** both prices required (Laravel), the rest patches; `vat_group_id: null` clears it.
- `relationships.vat_group` is null when unset or soft-deleted. Lists join it in one query.
- **List:** `q` (title), `page`, `page_size`.
- **Not ported:** `GET|POST /hourly-rates/users/{user}` (rate links, waits for hour registrations).

## Sales groups `/settings/sales-groups`

Permissions `salesgroup.{list,get,create,update,delete}` (new catalog names).

```json
{ "id": "<uuid>", "name": "Retail", "purchase_code": "AB-333", "revenue_code": "BA-443",
  "created_at": "…", "last_updated_at": "…", "meta": null }
```

- `name` is required and ≤150. It is unique per workspace, ignoring case; a deleted group frees its name ("The name has already been taken."). Laravel's rule was global across workspaces, which told one tenant another tenant's names.
- Codes are optional, ≤150.
- Update requires `name` and patches the codes.
- **List:** `q`, `page`, `page_size`.

## Shipment methods `/shipment-methods`

Permissions `shipmentmethod.{list,get,create,update,delete}` (new catalog names).

```json
{ "id": "<uuid>", "name": "DHL 1-2KG", "carrier": "DHL", "shipping_costs": "780.00000000",
  "min_weight": 1.0, "max_weight": 2.5, "provider": null, "price_impact_business": "10.00000000",
  "price_impact_consumer": "-5.00000000", "provider_id": null,
  "countries": [{ "name": "Netherlands", "country": "NL", "price": "5.95", "lead_time_hours": 24 }],
  "can_be_selected": true, "created_at": "…", "last_updated_at": "…", "meta": null }
```

- Money columns print as 8-decimal strings, weights as numbers (Laravel's raw columns). `can_be_selected` is a JSON bool (Laravel sent 0/1).
- **Create:**
  - `name`, `carrier` required, ≤255
  - `shipping_costs`, `min_weight`, `max_weight` required, ≥0, with `max_weight > min_weight` ("The max weight must be greater than 1.")
  - `price_impact_business` / `price_impact_consumer` optional, -100..100, default 0
  - `countries` optional array of objects
  - `can_be_selected` optional

  Laravel dropped the last three on create.
- **Countries:** each element must be an object, and is rebuilt from four keys; anything else is dropped. `name` is text ≤255, `country` ISO alpha-2 (stored uppercase), `price` a number ≥0 (stored as sent, trimmed), and `lead_time_hours` a whole number ≥0. Errors use keys like `countries.0.price`. At most 300 elements.
- **Update:** the price impacts and `countries` are required (Laravel). An empty `countries` array is accepted; Laravel refused it. The rest patches. The weight rule is a DB CHECK on the merged row, so sending only `max_weight` is compared with the stored `min_weight`.
- `provider` / `provider_id` are set by fulfilment integrations, never by this API.
- **List:** `q`, `page`, `page_size`. `can_be_selected` filters only through `/searches`.

## Custom types `/custom-types`

Permissions `customtype.{list,view,create,update,delete}`. The catalog has `view`, not `get`.

```json
{ "id": "<uuid>", "name": "Supplier", "domain": "relation", "properties": { "is_supplier": true },
  "created_at": "…", "last_updated_at": "…", "meta": null }
```

- **Create:**
  - `name` required, ≤55. Unique per (workspace, domain), ignoring case, live rows only: "The name is not unique".
  - `domain` required, one of the 84 Laravel `Domains` values, any case, stored lowercase ("The selected domain: xyz is invalid.").
- **`properties` by domain** (Laravel ValidateCustomTypeProperties; errors under `properties.<key>`):

  | Domain | Stored | Errors |
  |---|---|---|
  | `relation` | `{is_supplier}`, default false | "is supplier must be true or false" |
  | `activity` | `{is_ticket_type, icon, color}`, defaults false; `icon`/`color` text ≤255 | "is ticket type must be true or false" |
  | `product_return_reason` | `{discount_percentage}` when properties are sent; a number stays a number, text is stored normalized (`" 12.50 "` → `"12.5"`) | "discount percentage is required / must be numeric / must be between 0 and 100" |
  | `hour_registration_type` | the sent booleans among `default_for_work_order`, `is_leave_hours_type`, `is_sick_hours_type` | "<key> must be true or false" |
  | any other | null | – |
- **Update:**
  - The stored domain is kept; a `domain` in the body is ignored.
  - `properties`, when sent, are rechecked against that domain and replace the old ones. Absent keeps them.
  - The name must stay unique; Laravel only checked on create.
  - An unknown id is 404; Laravel answered 422 `domain`.
- **List:**
  - `domain` (422 when unknown), `q`, `page`, `page_size`.
  - `default_for_work_order` and `is_leave_hours_type` apply only with `domain=hour_registration_type`; `is_supplier` only with `domain=relation`.
  - The flags take `1/true/0/false`.

## Custom statuses `/custom-status`

Permissions `customstatus.{list,get,create,update,delete}`. The path is singular, as in Laravel.

```json
{ "id": "<uuid>", "name": "Won", "domain": "deal_pipeline", "archived": false,
  "created_at": "…", "last_updated_at": "…", "meta": null }
```

- **Create:** `name` required, ≤55. `domain` uses the same list as custom types. The name is unique per (workspace, domain), archived rows included: "The name is not unique".
- **Update:** `name` and `meta` only. Saving the unchanged name works; Laravel answered 422 there. The domain and `archived` can't change.
- **List:**
  - `domain`, `q`.
  - `limit` 1..200 (default 50) sets the page size; `page_size` is ignored.
  - Archived rows are left out, but `GET /{id}` still returns them.
- **Not ported:** `POST /{id}/archive` and `/unarchive` (no client calls them).

## Search `/searches`

`GET /searches?domain=X&q=…&page=…&page_size=…`, permission `workspace.global-search`.

- **Domains:** `currency`, `vat_group`, `sales_group`, `hourly_rate`, `shipment_method` (plus `can_be_selected=1|0`), `custom_status`, `custom_type`. Each answers in that module's list shape.
- **Query:** `q` matches the name (title for hourly rates); blank or `*` matches all.
- **404:** any other domain, or no domain (Laravel's global search).
- **Permissions:** only `workspace.global-search` is checked, as in Laravel, not each module's `*.list`. A member who can search sees every listed domain's rows, hourly rate purchase prices and ledger codes included. Requiring the new `vatgroup.list` / `salesgroup.list` / `shipmentmethod.list` names would break every dropdown for members, since nobody holds them yet.

## Seeded at signup

These are created in the signup transaction (Laravel `WorkspaceSetup`):

- **Currencies:**
  - Euro / eur / € / EUR, the default
  - UK Sterling / gbp / £ / GBP
  - Dollar / usd / $ / USD

  All have 2 decimals and are active.
- **Custom statuses** (`deal_pipeline`): Won, Lost, In Progress, To Do, Completed.
- **Custom types:**
  - Supplier `{is_supplier: true}` and Client `{is_supplier: false}` (`relation`)
  - Note and Ticket `{is_ticket_type: true}` (`activity`)
  - Website and Social media (`lead_source`)
  - Work, Leave `{is_leave_hours_type: true}` and Sick leave `{is_leave_hours_type: true, is_sick_hours_type: true}` (`hour_registration_type`)

  Laravel wrote these hour types under `hour_registration`, which vimdesk-web never reads, dropped Supplier's flag, and left Sick leave without the flag its leave-request sick check reads.
