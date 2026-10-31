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
| `created_at`, `updated_at` | date-time | |

## Routes

| Method | Path | Body / query | Success |
|---|---|---|---|
| GET | `/lots` | `product_id`, `status`, `q` (code prefix, any case, `%` and `_` literal), `page`, `page_size` | `200 {data, meta}`, newest first |
| POST | `/lots` | `product_id`, `code` (required), `status`, dates, `origin` | `201 {data}` |
| GET | `/lots/{lot}` | | `200 {data}` |
| PATCH | `/lots/{lot}` | any of `code`, `status`, dates, `origin`; absent keeps, `null` clears a nullable field | `200 {data}` |
| GET | `/lots/{lot}/events` | `page`, `page_size` | `200 {data, meta}`, newest first |

**422 cases.** Missing `product_id` or `code`; bad code characters or length; unknown status; a date that isn't `YYYY-MM-DD`; a later date before `produced_on`; `origin` over 255. `product_id`: "The selected product id is invalid." when the product is malformed, unknown, deleted, of another workspace, or does not track stock. `code`: "The code has already been taken." (any case, this workspace only).

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
