# Till

Point of sale for shops. A **register** is one till device with its own cash drawer. It sells from a store warehouse in one currency. **Till users** are shop staff who log in with a PIN. They don't need an email account.

Three kinds of caller:

| Caller | Credential | Routes |
|---|---|---|
| Office | member bearer + `cashregister.*` | `/till/registers`, `/till/users` |
| Device | the register's device key (`gsk_…`, scope `till:login`) | `/till/login/users`, `/till/login` |
| Till | till session token (64 hex) | `/till/logout`, `/till/me` |

Every path is under `/v1/workspace/{workspace}`. A key or session of another workspace gets 404.

## Registers

### `GET /till/registers` (`cashregister.list`)

`?page=&page_size=` (default 50, max 100). Newest first.

```json
{ "data": [{ "id": "…", "name": "Till 1", "warehouse_id": "…", "currency_id": "…",
             "key_prefix": "gsk_3f9a", "created_at": "…" }], "meta": { … } }
```

`key_prefix` is null once the key is revoked.

### `POST /till/registers` (`cashregister.create`)

| Field | Rule |
|---|---|
| `name` | required, 1–100 |
| `warehouse_id` | required, a live **store** warehouse (`is_store = true`) of the workspace |
| `currency_id` | optional, a currency of the workspace; default the main currency |

201 `{data: {…register, key}}`. This is the only time `key` is shown. The device key is an app key of kind `till` with scopes `till:login` and `catalog:read`, so the device can also read `/v1/storefront/products` for its offline cache. It is listed under `/app-keys`, and revoking it there disables the register. The `/app-keys` route itself never issues `till:login`. Known side effect, accepted: a member with `cashregister.create` or `cashregister.update` gets a key that can read the catalog (products, prices, stock) through `/storefront`, even without product rights.

422 messages: `The selected warehouse id must be a store warehouse.`, `The selected currency id is invalid.`

### `POST /till/registers/{register}/key` (`cashregister.update`)

Revokes the current device key and issues a new one: 200 `{data: {key}}`. Sessions opened with the old key end on their next request. Also clears the register's PIN lock.

## Till users

### `GET /till/users` (`cashregister.list`)

All till users by name: `{data: [{id, name, user_id, active, locked, created_at}]}`. `locked` is true while the user is locked after wrong PINs (see login).

### `POST /till/users` (`cashregister.create`)

| Field | Rule |
|---|---|
| `name` | required, 1–100 |
| `pin` | required, 4–8 digits (`The pin must be 4 to 8 digits.`) |
| `user_id` | optional, a member of the workspace (`The selected user id is invalid.`) |

201 `{data: {…}}`. The PIN is stored as an argon2id hash and is never returned.

### `PATCH /till/users/{user}` (`cashregister.update`)

`name`, `pin`, `active`. Leave a field out to keep it. A new PIN clears the lock. A new PIN or `active: false` ends the user's live sessions (they stay ended if the user is reactivated); `active: false` also blocks login. Till users are never deleted, because sales point at them.

## Login (device key)

### `GET /till/login/users`

Active till users for the login picker: `{data: [{id, name}]}`.

### `POST /till/login`

`{till_user_id, pin}` → 200:

```json
{ "data": { "token": "64 hex", "expires_at": "…",
            "register": { "id": "…", "name": "Till 1", "warehouse_id": "…", "currency_id": "…" },
            "user": { "id": "…", "name": "Mary" } } }
```

- The token lasts 8 hours (one shift). There is no refresh: after 8 hours the user enters the PIN again.
- A wrong PIN, an unknown user and an inactive user all get the same 401 `{"message": "Invalid credentials"}` after the same argon2 work.
- Every 5th wrong PIN since the user's last right one locks that user: 15 minutes the first time, then 30, 60, … up to 24 hours. A right PIN starts the count over.
- 20 wrong PINs within an hour on one register lock that register's login for 15 minutes. A right PIN does not reset this count, so a valid user can't hide someone else's guessing.
- Other registers and users are unaffected. While locked: 429, even with the right PIN, also for a right PIN already in flight when the lock landed.
- A login ends the same user's older sessions on that register.
- Known side effect, accepted: a locked user gets 429 at once, while an unknown id gets 401 after the argon2 work, so a device key holder can tell a real locked user id apart. The login picker lists active users anyway.
- 401: missing, revoked or rotated key, or a key bound to no register. 403: a key without `till:login`.

## Session (till token)

`Authorization: Bearer <token>`. A session stops working when it is ended or expired, when the register's key is rotated or revoked, when the till user is set inactive, or when the workspace is deleted. Then: 401.

- `GET /till/me` → `{data: {register, user, expires_at}}`
- `POST /till/logout` → 204

## Catalog (till token)

### `GET /till/catalog?page&page_size&updated_since&code`

What the session's register sells, by id, paged (`page_size` default 50, max 100). Workspace, currency and warehouse come from the register. Code: `src/modules/till_catalog/`.

- Products in another currency are left out (a sale refuses them). `store_only` products show (the web shop hides them).
- Row: `id`, `name`, `type`, `ean13`, `barcode`, `store_only`, `vat_rate` (decimal string from the VAT group, `0` without one), `unit_price` (minor units excl VAT; what `POST /till/sales` books, so no mismatch flag), `has_stock`, `stock` (level − reserved in the register's warehouse, released unexpired lots and no-lot rows; may be negative; null when not tracked), `sellable`, `variants[]` (`id`, `sku`, `ean13`, `barcode`, `is_default`, `unit_price`, `options`), `updated_at`.
- `code`: exact product or variant EAN-13 or barcode, any case (≤ 100 chars).
- Without `updated_since`: sellable products only. With it (RFC 3339): every product whose row, a variant, its VAT group, or a stock row or lot in the register's warehouse changed after it, sellable or not; `sellable: false` tells the till to drop it.
- Reply: `{data, meta, next_since}`. `next_since` is a minute before the read: send it as the next `updated_since` and upsert by id (repeats are expected).
- shortcut: a lot passing its expiry date changes no row, so the delta doesn't carry it; the next full read does.
- `401` without a live till session (a member token or app key included), `404` on another workspace's path, `422` for a bad `updated_since` or a long `code`.

## Cash

All amounts are integer minor units of the register's currency (EUR: cents). A **count** is a list of at most 50 lines `{denomination, count}`: `denomination` 1–1 000 000 000 minor units per note or coin, `count` 0–1 000 000 pieces, each denomination once. Errors are keyed `count.N.denomination` / `count.N.count`.

Every movement is a row in `cash_ledger`, which refuses updates, deletes and truncates. A shift's expected cash is its opening count plus sales (step 4) plus or minus transfers. A vault's balance is the sum of its transfers.

### Drawer (till token)

- `GET /till/shift` → `{data: shift}` or `{data: null}` when the register has no open shift.
- `POST /till/shift/open` `{count}` → 201. The counted float becomes `opening` and `expected`. 409 `A shift is already open on this register.`
- `POST /till/shift/close` `{count}` → 200 with `expected`, `counted`, `difference` (counted − expected; negative = short), `closed_by`, `closed_at`. 409 `No shift is open on this register.`
- `POST /till/transfers` `{vault_id, direction, count}` → 201 `{data: {transfer_id, vault_id, direction, amount}}`.
  - `direction`: `to_vault` (a drop) or `from_vault` (more float). Needs an open shift (409).
  - The vault must be of this workspace and in the register's currency (422 on `vault_id`).
  - The source must hold the amount (422 on `count`, `The drawer holds only N (minor units).`). A count totalling 0 is refused.
  - Both sides are booked in one transaction under the register's and the vault's row locks, so parallel drops never overdraw.

Each register has its own drawer: one open shift per register, and a session only ever sees its own register's shift. Any till user logged in on the register may close its shift, not only the one who opened it; `opened_by` and `closed_by` show who did each.

Shift:

```json
{ "id": "…", "register_id": "…", "opened_by": "…", "opened_at": "…", "opening": 4515,
  "expected": 2515, "closed_by": null, "closed_at": null, "counted": null, "difference": null }
```

### Office (member bearer)

- `GET /till/vaults` (`cashregister.list`) → `{data: [{id, name, currency_id, balance, created_at}]}`
- `POST /till/vaults` (`cashregister.create`) `{name, currency_id?}` → 201. The currency defaults to the main one.
- `GET /till/shifts` (`cashregister.list`) `?page=&page_size=` → shifts of every register, newest first.

Vaults fill only from drawer drops for now. Bank deposits and withdrawals come later.

## Sales (till token; office list)

A till sale is an **order** with `source: "till"`, created `completed`, in the register's currency, from the register's store warehouse. The office sees it in `/orders` like any other order (returns work as for any completed order).

### `POST /till/sales`

```json
{ "client_ref": "0190f3c2-offline-17",
  "sold_at": "2026-11-01T09:12:00Z",
  "lines": [ { "product_id": "…", "quantity": "2", "unit_price": 1250 },
             { "product_id": "…", "product_variant_id": "…", "quantity": "1", "unit_price": 300 } ],
  "payment": { "method": "cash", "tendered": 5000 } }
```

| Field | Rule |
|---|---|
| `client_ref` | required, 1–64. The till's own id of the sale; unique per register. Reuse it on every retry. |
| `sold_at` | optional RFC 3339, default now. Within the last 7 days (5 minutes of clock skew allowed). A replay may spell the same instant differently (`Z` or `+00:00`). |
| `lines` | 1–50; `product_id`, optional `product_variant_id`, `quantity` (> 0, 2 decimals), **`unit_price` required**: what the till charged, minor units excl VAT (the storefront's `price_net`). `discount_percentage` and `title` are refused. |
| `payment` | `{method: "cash", tendered}` (minor units) or `{method: "card", reference?}` (≤ 64). |

What happens, in one transaction:

1. **Replay check.** The register's `client_ref` is locked. If a sale exists under it:
   - with the same body → **200** with the stored sale, nothing written;
   - with another body → **409** `This client_ref was used for another sale on this register.`
   Parallel copies of one sale wait on that lock: exactly one writes, the others answer 200.
2. **Prices.** The lines are priced through the pricing engine at the till's `unit_price` (VAT from the catalog's rate). Those amounts become the order lines and `total`. The same lines at today's catalog prices give `server_total`. `price_mismatch = total − server_total` and `mismatched_lines` (lines whose unit price differs from the catalog's, counted even when they offset each other) are stored, never refused: an offline sale rung up before a price change lands as charged and is flagged. A total above 1 000 000 000 000 minor units is refused (422 on `lines`).
3. **Payment.**
   - Cash: `due` = `total` rounded half up to the currency's step in settings `cash_register.cash_rounding` (EUR `5` → 0.05; no entry → no rounding). `rounding = due − total`. `tendered < due` → 422 on `payment.tendered`. Needs an open shift on the register (409). `change = tendered − due`. The drawer gets one `sale` ledger row of `due`.
   - Card: an external terminal recorded by hand. `due = total`, no rounding, no change, no drawer row. Real terminals (PAY.nl) are deferred, see `docs/deferred-providers.md`.
4. **Stock** is taken at once from the register's warehouse (`stock::reserve::take`, `Floor::None`): a sale is never refused for stock, and a store may go below zero, which the office sees on the product's stock. The web shop's `available_stock` drops by the same amount.
5. **Event** `order.created` (status `completed`, source `till`) and `stock.changed` per product.

201 (or 200 on a replay):

```json
{ "data": { "order_id": "…", "number": "1042", "client_ref": "0190f3c2-offline-17",
            "register_id": "…", "till_user_id": "…", "method": "cash",
            "total": 3088, "server_total": 3088, "price_mismatch": 0, "mismatched_lines": 0,
            "due": 3090, "rounding": 2, "tendered": 5000, "change": 1910,
            "card_reference": null, "sold_at": "…" } }
```

The drawer's expected cash uses `due`, the amount actually taken, so it balances even when `price_mismatch` is not 0.

### `GET /till/sales/{client_ref}`

The sale of **this register** under that ref, else 404. Use it before replaying a queued sale whose answer got lost.

### `GET /till/sales` (`cashregister.list`)

`?mismatch=true` (only sales with `mismatched_lines > 0`), `?page=&page_size=`. Newest first.

Known limits, accepted:
- `order.created` carries no mismatch; the office finds price differences through `GET /till/sales?mismatch=true`.
- An offline cash sale goes into the shift that is open when it is replayed, whatever its `sold_at`: that drawer holds the cash. Replay the queue before closing the shift.
