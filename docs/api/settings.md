# Settings API

Workspace settings: numbering series, document defaults, the POS till, the general locale and the integrations tree. Ported from vimdesk-api `src/Domain/WorkspaceSetting/Setting` (`GetSettingController`, `UpdateSettingController`, `StoreSettingRequest`, `Values/*Setting`, `Helpers/FetchSettings`).

Shared rules (guard order, 404s, 422 wording) are in `conventions.md`. Both routes sit under `/v1/workspace/{workspace}`.

Not here yet: Picqer warehouse import and shipping mappings, webhook errors, external stock sync, loyalty, Office 365 test calls, email logs. Those come with their sub-batches.

## Storage

One row per workspace in `settings`: `data jsonb`, one key per section, plus `updated_by` (the last writer, since numbering series are auditable config). Laravel had 15 longtext columns; the API is the same. The row appears on the first POST, and until then GET answers the defaults. A hand-edited section that isn't an object prints as its defaults.

## `GET /settings` (`setting.get`)

Laravel checked `setting.view`, which no team can hold; genesy checks the catalog's `setting.get` (see conventions).

**Without `type`:** every section, stored values over Laravel's defaults (`XSetting::empty()`):

| Section | Defaults |
|---|---|
| `smtp` | all null |
| `invoice` | all null, `customer_ref {ordered_by: true, po_number: false, reference: false}` |
| `estimate` | `EST-`, start 1, validity 14 / 30 days, digits 4 |
| `credit` | `CRE-`, start 1, payment term 14 / 30, digits 4 |
| `collective_invoice` | `COL-INV-`, same as credit |
| `relation`, `order`, `work_order` | `RE-` / `ORD-` / `WO-`, start 1, digits 4 (`work_order.fetch_users_from_team_id` null) |
| `purchase_order` | `PO-`, start 1, digits 5 |
| `product` | `PRO-`, start 1, digits 5, `user_search_history_count` 10, flags false |
| `cash_register` | numbering null, `receipt_line_length` 34, `allow_individual_lines` true, `cash_drawer_link_mode` `flexible`, `accepted_cash` the 13 denominations `"0.05"`…`"500.00"`, `logo` null |
| `incoming_invoice` | `required_approvals` 1, tolerances 0.0, scores 95 / 80 |
| `general` | `en`, `UTC`, `metric`, `YYYY-mm-dd`, `.`, `,`, `default` |
| `integrations` | the full provider tree (sendcloud, mollie, pay with `terminals: []`, jortt, prestashop, modjo, hostbill null, picqer, mail driver `imap`) |
| `meta` | as stored, else null |

- A stored null section, or a stored null child object inside `integrations`, prints as its default (Laravel `from([])`).
- `cash_register.logo` is always null until files are ported (F5). Laravel inlined the receipt logo file as base64.

**With `?type=`** (Laravel `FetchSettings`), a section name or a dotted path:

- `?type=cash_register` gives the section object.
- `?type=product.warning_below_preferred_marge` gives `[15]`. A scalar is wrapped in a list and null becomes `[]`, from Laravel's `(array)` cast; vimdesk-web reads `res[0]`.
- A falsy stored value (`null`, `0`, `""`, `false`, `[]`) falls back to the default: `estimate.digits` stored as 0 reads `[4]`. `cash_register.cash_drawer_link_mode` always reads `flexible` when unset.
- An unknown section or key reads `[]`, and so does a path with an empty segment (`smtp.`, `a..b`).
- A numeric segment indexes a list: `cash_register.shipping_options.0.name`.

## `POST /settings` (`setting.update`)

A partial object keyed by section: `{"invoice": {...}, "smtp": {...}}`. Answers `{"message": "ok"}`.

- **Each section sent replaces that section whole**: fields left out go back to their defaults (Laravel cast the section through its Data class, without a deep merge). Sections not sent stay as they were.
- **Blank is null.** Strings are trimmed (not `smtp.password`) and `""` becomes null before validation, as Laravel's global TrimStrings and ConvertEmptyStringsToNull middleware did. A nullable field sent as `""` is cleared, not refused.
- **Only listed keys are stored.** Unknown sections and keys are dropped. `order` and `purchase_order` had no rules in Laravel and stay read-only.
- `null` for a section clears it back to its defaults.
- One statement writes: `INSERT … ON CONFLICT (workspace_id) DO UPDATE SET data = settings.data || EXCLUDED.data`. Two first saves at the same moment both land, neither is lost (`tests/settings.rs` `parallel_first_posts_lose_nothing`).

**Rules** (`src/modules/settings/schema.rs`, from Laravel `StoreSettingRequest`):

| Field | Rule |
|---|---|
| `*.prefix`, `*.suffix` | string ≤ 10 |
| `*.digits` | integer ≤ 10 |
| `*.start_number` | integer 0..999999999999 (Laravel: any integer; a huge start number would let the series overflow) |
| payment terms, validity periods | integer 0..3650 days |
| `incoming_invoice.required_approvals` | 0..100 |
| `invoice.default_footer_message` | ≤ 2500 (≤ 255 for credit, estimate, collective invoice) |
| `*.default_note` | ≤ 1000 |
| `estimate.default_due_date_in_days` | integer ≥ 0, checked but not stored (no field on Laravel's EstimateSetting) |
| `smtp.encryption` | `ssl` or `tls` ("Invalid smtp.encryption encryption type.") |
| `product.warning_below_preferred_marge` | 0..100 |
| `product.user_search_history_count` | 1..40 |
| `cash_register.receipt_line_length` | 20..80 |
| `cash_register.receipt_header`, `receipt_footer` | ≤ 1500 |
| `cash_register.cash_drawer_link_mode` | `strict` or `flexible` |
| `cash_register.accepted_cash` | at most 50 numbers, stored as sent |
| `cash_register.shipping_options` | at most 50; `type` (`delivery`, `pickup`) and `name` (≤ 40) required, `value` a warehouse |
| `incoming_invoice.ledgers` | at most 500; `no` is a required number |
| `integrations.payment.payment_providers.pay.terminals` | at most 100 of `{id, name, simulation}` (Laravel stored the array unvalidated; other keys are dropped) |
| `…picqer.base_url`, `…prestashop.shop_url`, `…hostbill.endpoint` | http(s) URL without credentials in it (Laravel: any string) |
| other lists (tag ids 500, countries 300, skip sources 100, senders 100) | capped; an oversized list is one error |
| uuid refs | must be a live row of this workspace (`default_relation_type_id`: a custom type of the `relation` domain): `product.second_chance_product_default_warehouse_id`, `cash_register.{default_mutate_from_warehouse_id, default_relation_id, default_relation_type_id, shipping_options.*.value}`, `work_order.fetch_users_from_team_id`. Laravel checked `exists` across all workspaces. |
| `integrations…error_email`, `mail.graph.mailbox`, sender emails | email |
| `integrations.syncProviders.picqer.order_export_start_date` | date |
| picqer tag id lists | integers |
| `integrations.mail.driver` | `imap` or `graph`; with `graph`, the four graph fields are required |
| `general.*` | strings ≤ 255 (Laravel had no rules; a non-string crashed its cast) |
| `meta` | object or null |

**`invoice.customer_ref` difference:** Laravel's rules had trailing spaces in the keys, so the flags were never validated. Genesy validates `ordered_by`, `po_number` and `reference` as booleans and stores them.

## Integration credentials

Credentials of the kept providers are stored encrypted, apart from the settings row:

| Key | Max chars |
|---|---|
| `smtp.password` (not trimmed) | 255 |
| `integrations.payment.payment_providers.mollie.api_key` | 255 (Laravel: no max) |
| `…pay.api_code`, `api_token`, `service_id` | 255 (Laravel: no max) |
| `integrations.mail.graph.client_secret` | 1000 |

**GET** (full or `?type=`) prints `"********"` for a stored credential and null for an unset one. It never prints the value. Laravel printed all of them in plaintext to anyone with settings access, except the Graph secret, which it printed as ciphertext.

**POST**, for each key in a section that is sent:

| Sent value | Effect |
|---|---|
| `"********"` | keeps the stored value (vimdesk-web posts back what GET printed); a **422** `Enter this credential again.` on that key when none is stored, or when the SMTP password is sent with another `host`, `port`, `username` or `encryption` than stored |
| any other string | replaces it |
| `null`, blank, or the key left out | clears it, as the rest of the section resets (Laravel replaced the whole section). The SMTP password isn't trimmed, so spaces are a password (as in Laravel) |

- A section that isn't sent keeps its credentials.
- The SMTP binding stops a member who can edit settings but doesn't know the password from pointing the stored one at their own server. Mollie, PAY.nl and Graph send theirs to fixed provider hosts.
- Each stored or cleared credential is logged (workspace, user, key; never the value).
- An SMTP password that is exactly `********` can't be stored, since that value means "keep".
- The settings write and its credential changes run in one transaction.
- Without a server key (`CREDENTIALS_KEY`), a new value is a **422** `Credential storage isn't configured.` on that key, and nothing of the save is written. Keeping and clearing still work.

**Dropped providers** (Sendcloud `public`/`secret`, `jortt.client_secret`, `modjo.api_key`, `picqer.api_key`): a non-empty value is a **422** `This integration isn't supported.` on that key. `null` and `""` are accepted. GET prints them as null.

**Storage:**
- Table `workspace_credentials`, one row per (workspace, key).
- Each value is sealed with XChaCha20-Poly1305 (random 24-byte nonce) under the server key, with the workspace id and the key's path as associated data. A value copied to another workspace or key doesn't decrypt.
- Each row also stores `key_id`, a hash prefix naming the server key that sealed it.
- The settings row never holds a credential: a final check (`schema::holds_no_secret`) refuses any write that would store one there.

**Server key:** `CREDENTIALS_KEY_FILE` (a Docker secret; `CREDENTIALS_KEY` also works, but an env var shows in `docker inspect`) holds 64 hex characters (32 random bytes, e.g. `openssl rand -hex 32`). It is optional: the API boots without it, logs a warning, and refuses new credentials. A malformed key, or one with fewer than 16 distinct bytes (a zero key, a pattern), stops the boot with an error that doesn't print it. Keep the key outside the database and its backups; losing it loses every stored credential, which then has to be entered again.

The Mollie, PAY.nl and mail steps (F5, F8, F16) read these values on the server; none is ever taken from the client. The Resend webhook secret is platform-wide, not per workspace, so F20 reads it from its own `*_FILE` secret.

## Numbering

`src/numbering.rs` prints numbers as Laravel `DomainNumber` did: prefix + number zero-padded to `digits` (1..10, default 1) + suffix. Each document stores the appendix `{prefix, digits, suffix}` it was numbered with, and displays from that snapshot.

Each numbered table takes its next number inside the transaction that writes the row (the functions take a `Transaction`, so they can't run outside one):

1. `lock_numbers`: `SET LOCAL lock_timeout = '5s'`, then `pg_advisory_xact_lock(hashtextextended('<table>.<column>|' || workspace, 0))`. One 64-bit key, so workspaces don't share a lock.
2. Read the series settings (after the lock, so the snapshot is the latest).
3. `next_number`: `max(number) + 1` over all rows (soft-deleted ones count), or the section's `start_number` when the workspace has none yet. This must stay a separate statement after the lock: under READ COMMITTED it then sees the previous holder's committed row.
4. A unique index backs it up.

A rollback releases the lock and leaves `max` unchanged, so the next writer gets the same number: no gaps. Relations and products follow `settings.relation` / `settings.product` from F2 on. A workspace that already has rows continues from its highest number; a `start_number` set later is ignored, as in Laravel.

Invariants for invoices (F6) are in `docs/ai/risk-register.md` ("document numbering").
