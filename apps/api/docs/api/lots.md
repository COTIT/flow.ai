# Lots

Batches of a stock-tracked product, with an append-only trail. Code: `src/modules/lots/`.

Paths sit under `/v1/workspace/{workspace}`. Members read; writes need `product.update` (checked before the body, so `403` comes before `422`). Outsiders, other workspaces, malformed and unknown ids get `404`.

## Lot

| Field | Type | Notes |
|---|---|---|
| `id`, `product_id` | uuid | |
| `code` | string | 1–64 of `A-Z a-z 0-9 . _ / -`; unique per workspace, any case. Printed on the label. |
| `public_code` | string | 10 Crockford base32 chars (`0-9`, `A-Z` without `I L O U`), made by the server, unique across all workspaces, never changes. The QR link is `/b/{public_code}`. |
| `status` | string | `released` (default, sellable), `quarantine` or `recalled` |
| `produced_on`, `best_before`, `expires_on` | date or null | `YYYY-MM-DD`; `best_before` and `expires_on` not before `produced_on` |
| `origin` | string or null | up to 255 chars |
| `parent_lot` | `{id, code}` or null | the lot of this workspace it was packed from (read only) |
| `pack` | `{raw_kg, units}` or null | decimal strings: kg of the parent used and units made (read only) |
| `received_from` | `{transfer_id, from_lot_code, from_lot_public_code}` or null | the supply transfer it was received from, with the sender's lot codes as `/supply-transfers` shows them; never the sender's ids (read only) |
| `created_at`, `updated_at` | date-time | |

## Routes

| Method | Path | Body / query | Success |
|---|---|---|---|
| GET | `/lots` | `product_id`, `status`, `q` (code prefix, any case, `%` and `_` literal), `page`, `page_size` | `200 {data, meta}`, newest first |
| POST | `/lots` | `product_id`, `code` (required), `status`, dates, `origin` | `201 {data}` |
| GET | `/lots/{lot}` | | `200 {data}` |
| PATCH | `/lots/{lot}` | any of `code`, `status`, dates, `origin`; absent keeps, `null` clears a nullable field | `200 {data}` |
| GET | `/lots/{lot}/events` | `page`, `page_size` | `200 {data, meta}`, newest first |
| GET | `/lots/{lot}/farmer-credits` | `page`, `page_size` | `200 {data, meta, farmer_groups}`, newest first; see below |

**422 cases.** Missing `product_id` or `code`; bad code characters or length; unknown status; a date that isn't `YYYY-MM-DD`; a later date before `produced_on`; `origin` over 255. `product_id`: "The selected product id is invalid." when the product is malformed, unknown, deleted, of another workspace, or does not track stock. `code`: "The code has already been taken." (any case, this workspace only).

## Farmer credits (selling side)

`GET /lots/{lot}/farmer-credits` lists the sales and returns of this workspace's lot that credited or debited farmers (`farm_sale_attributions` with `sold_workspace_id` = this workspace): `{order_id, order_number, till_ref, kind, jars, at}`. `till_ref` is the till's `client_ref` (null for web and office orders); `kind` is `sale` or `return`; `jars` is the units sold, or for a return the change in units returned (positive = jars came back). `farmer_groups` is how many groups the lot's public page shows under the link's frozen allow-list; null when it shows none (no supply lineage, or `village` not shared).

Never shown (G10): farmer names or ids, per-farmer or total amounts, kg, price per kg, the batch, or any farm-side id. A lot of another workspace (the farm's included) is 404. Members read.

## Trail (`lot_events`)

Written in the same transaction as the change; never updated or deleted (a test greps the code for it).

| `kind` | When | Extra |
|---|---|---|
| `created` | POST | `to_status` |
| `updated` | PATCH changed code, dates or origin | `meta.changed`: the changed field names |
| `status_changed` | PATCH changed `status` | `from_status`, `to_status` |

A PATCH that changes nothing writes nothing. Each row has `user_id` (null once the user is deleted) and `created_at`.

## Events

`lot.created` (on POST, same transaction): `{lot_id, product_id, code, public_code}`.

## Stock per lot

Stock rows carry an optional `lot_id`. Null is the product's no-lot stock, which keeps the old one row per product, warehouse and bin. Each lot gets its own warehouse-level row next to it. Every ledger line (`stock_movements`) carries the row's `lot_id`, and so does `stock.changed`.

`POST /lots/{lot}/stock-mutations` (`product.update`) with body `{warehouse_id, quantity, reason?, is_correction?}`:

- `quantity`: non-zero, rounded to 2 decimals. Positive puts stock in, negative takes it out.
- Out may not take more than the row has free (level − reserved). Otherwise `422` on `quantity`: "Insufficient stock in this lot. Available: …, Reserved: …, Requested: …".
- An unknown or foreign warehouse gives `422` on `warehouse_id`.
- Reply: `200 {data: {warehouse_id, stock_lvl}}`.

`GET /lots/{lot}/stock` (any member, like reading the lot; another workspace's lot `404`): `200 {data: [{warehouse_id, level, reserved, available}]}`, one row per warehouse holding the lot (its rows there summed), by warehouse name, 2-decimal text. `available = level − reserved`. No stock: `[]`.

Once a lot holds any stock row, its `code` can't change (`422` on `code`); the code is printed on labels.

The product read (`stocks[]`), `GET /products/stocks` and `stock-with-bins` show lot rows as rows of their own, with `lot: {id, code, expires_on}`. No-lot rows have `lot: null`.

### Orders on lot stock

An order's hold (confirm) and shipment (complete) pick rows first-expired-first:

1. The product's no-lot stock goes first. It has no date, and selling it first clears untracked stock.
2. Then sellable lots by their earliest date (expiry or best-before, whichever is sooner). Undated lots go last.

Only `released` lots that are not past `expires_on` are sellable; quarantined, recalled and expired lots are never held. A hold may spread over several lots, and completing ships from exactly the rows held. Each ledger line names its lot.

Completing an order that holds a lot recalled since the hold gives `409 {"message": "lot_recalled"}`. A lot put in quarantine since gives `409 {"message": "lot_quarantined"}`, and a lot past its expiry date since gives `409 {"message": "lot_expired"}`. In both cases nothing is written and the hold stays, so the office can cancel the order or release the hold; nothing is swapped to another lot silently.

The storefront's `available_stock` counts only no-lot stock and sellable lot stock.

### Sales without a hold (till, invoices)

Till sales and sent invoices take stock straight away, through the stock seam's `take`. They use the same order: no-lot stock first, then sellable lots first-expired-first. Each ledger line names its lot.

- **Store warehouses** never refuse. What the rows can't cover lands on the no-lot row, which may go below zero, as before.
- **Other warehouses** refuse an invoice that asks for more than the no-lot row and the sellable lots have free (level − what orders hold): `422` "Insufficient stock for product … Available stock: … Required: …". Nothing is written. An invoice never takes stock a web order holds.
- **Credit notes** put stock back on the no-lot row, never into a lot.

## Passport

One per lot: a **draft** the office edits and a **published** copy the public page shows. Routes (same access rules as above):

| Method | Path | Success |
|---|---|---|
| GET | `/lots/{lot}/passport` | `200 {data: {draft, published, version, published_at}}` |
| PUT | `/lots/{lot}/passport` | body = the whole draft; `200`. The published copy is untouched. |
| POST | `/lots/{lot}/passport/publish` | the draft becomes the published copy, `version` + 1; `200` |
| DELETE | `/lots/{lot}/passport/publish` | the published copy is hidden (draft and version stay); a no-op when hidden; `200` |

**Shape** (every key optional; unknown keys at any level are a `422` on `body`, so no phone, exact location or other farmer data can be stored):

| Key | Rules |
|---|---|
| `title` | ≤ 120; required to publish |
| `intro` | ≤ 2000 |
| `banner_file_id` | an `image/*` file of this workspace |
| `origin` | `{country (ISO 3166 alpha-2, stored upper case), region ≤ 120, place ≤ 120}` |
| `map` | `{lat −90..90, lng −180..180, label ≤ 120}`; lat/lng are **rounded to 2 decimals (~1 km) when saved** |
| `materials` | ≤ 30 × `{name ≤ 120, origin ≤ 120, share_bp 0..10000}` (basis points) |
| `certificates` | ≤ 20 × `{name ≤ 120, issuer ≤ 120, number ≤ 64, valid_until (date), file_id}`; the file may be an image or a PDF |
| `story` | ≤ 30 blocks by `kind`: `text {title ≤ 120, body ≤ 5000}`, `image {file_id, caption ≤ 300}`, `facts {items: ≤ 20 × {label ≤ 60, value ≤ 120}}`, `quote {text ≤ 1000, by ≤ 120}` |

**Files.** Every `file_id` must be a stored file of this workspace of the right type, else `422` on its path (`banner_file_id`, `story.3.file_id`, `certificates.0.file_id`). Checked on save and again on publish (a file deleted in between fails the publish). Saving or publishing a passport that references any file also needs `file.get` (`403` otherwise), so `product.update` alone can't expose files its holder can't read. Files have no public flag; publishing is the choice to show them, and the public page only ever gets short-lived signed links.

**Publish** runs in one transaction: the lot row locked, the draft checked (title, files), the copy and version written, a `passport_published` trail row (`meta.version`) and the `passport.published` event (`{lot_id, product_id, public_code, version}`). Unpublish writes a `passport_unpublished` row and sends no event.

## Public page

What a scanned jar opens. The QR code holds the shop URL `/b/{public_code}`; the shop renders it from one of two mounts of the same reply:

| Mount | Auth | Limit | Scope |
|---|---|---|---|
| `GET /v1/public/lots/{code}` | none | the public per-IP limit (shared with login) | any workspace |
| `GET /v1/storefront/lots/{code}` | app key with `catalog:read` | per key | the key's workspace only |

The shop server uses the storefront mount: all its calls come from one IP and would exhaust the public bucket.

**Reply** (`200 {data}`), an allow-list. No internal ids, no workspace, no draft, no quantities:

```
{ code, public_code, status, produced_on, best_before, expires_on, origin,
  product:  { name, brand, short_description },
  passport: { title, intro, banner, origin, map, materials, certificates, story, version, published_at },
  farmers:  [] }
```

- `passport` is the **published** copy. Draft edits never show until they are published again.
- File ids become links: `banner`, each certificate's `file` and each image block's `image` are `{url, expires_at}`. The links are signed and last 10 minutes, so cache the page for less than that. A file deleted since publishing, or a server without file storage, gives `null` for that link.
- `status` is shown as it is. A recalled lot stays visible with `"recalled"`.
- `farmers` is always `[]` for now. The type allows only `village`, `first_name`, `photo`, `harvest_dates`, and is filled once consented farmer links exist. Phone, exact location and balances can never appear.

**404.** An unknown code, a lot that was never published or is unpublished, and (on the storefront mount) another workspace's lot all get the same `404` body and headers. Each costs the same single query; there is no early return.
