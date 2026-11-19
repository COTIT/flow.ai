# Imports

Generic master-data imports (masterplan §14/§15). Profiles: `products`, `relations`, `bank_statement` (whole statement in one transaction, native permission `accounting.reconcile`; see `accounting.md` "Bank statements and reconciliation"). Opening profiles `opening_inventory`, `opening_receivables`, `opening_payables`, `opening_trial_balance` stage reviewed opening lines only (whole batch in one transaction, native permission `accounting.configure`, setup books only; see `accounting.md` "Opening conversion").

Flow: create batch → upload chunks → validate (complete revision) → review rows → apply (queued) → results.
All mutations need an `Idempotency-Key` header and the C/A envelope (`client_ref`, `expected_version`, `payload`); a repeated `client_ref` with the same body replays the first answer.

## Limits

| What | Limit |
|---|---|
| Rows per batch (`expected_rows`) | 1–500 |
| Rows per chunk | 1–20 |
| Chunk index | 0–24 |
| Canonical chunk encoding | ≤ 60 KiB (413 above), request ≤ 64 KiB |
| Cell / `external_key` / `filename` | ≤ 255 characters |
| Cell type | text only |

## Routes (`/v1/workspace/{workspace}/imports…`)

| Method + path | Operation | Permission | Body | Success |
|---|---|---|---|---|
| GET `` | `imports_list` | `import.list` | – | 200 `{data,meta}` |
| POST `` | `imports_create` | `import.create` | C `{profile, filename, file_hash, expected_rows, mapping}` | 201 `{data}` |
| GET `/{id}` | `imports_get` | `import.get` | – | 200 `{data}` |
| PUT `/{id}/chunks/{index}` | `imports_upload` | `import.upload` | A `{rows:[{row_number, external_key?, fields}]}` | 200 `{data}` |
| POST `/{id}/validate` | `imports_validate` | `import.validate` | A `{expected_chunks}` | 200 `{data}` |
| POST `/{id}/apply` | `imports_apply` | `import.apply` + native (`product.create`/`product.update`/`relation.create`) | A `{validation_fingerprint}` | 202 `{data}` |
| GET `/{id}/rows?status=` | `imports_rows` | `import.get` | – | 200 `{data,meta}` |

`mapping`: products `{operation: "create"|"update", match_key: "product_nr_formatted"|"barcode"|"ean13"|"reference"}`; relations `{}`.

Columns: products `name, price, barcode, ean13, reference, description, unit`; relations `type, company_name, alias, coc_number, vat_number, email, website, phone_number, default_language` plus address columns `address, address2, house_number, zipcode, city, state, country` (one address).

Batch: `id, profile, filename, file_hash, expected_rows, mapping, revision, expected_chunks, state (uploading|validated|applying|completed|failed), validation_fingerprint, issues, version, counts{staged,valid,invalid,applied,failed,received}, allowed_actions, created_by, created_at, updated_at`.

Row: `id, row_number, external_key, fields, status (staged|valid|invalid|applied|failed), issues[{code,message,fields?}], target_id, result_id, applied_at`.

## Rules

- Chunks: same rows for a stored index replay (200); other rows for it → 409. Changed content needs a new batch.
- Validate refuses (422) until every chunk 0..n-1 is in and the rows add up to `expected_rows`. Each row is decoded by the native product/relation rules. Products match exactly on the chosen key (no trimming, case folding or fuzzy match): create refuses an existing key; update needs exactly one match through `external_key`. Rows sharing a key are all invalid.
- Apply needs the current `version` and `validation_fingerprint` (409 `stale_document` otherwise) and at least one valid row (422). Invalid rows never apply.
- The apply job rechecks the applier's permissions (lost → batch `failed`, nothing written), then writes one row per transaction through the native service; a refused row becomes `failed` with a readable issue. Restart skips applied rows.

## Errors

401 not signed in · 403 missing permission · 404 other workspace or unknown batch · 409 stale / changed chunk / reused `client_ref` · 413 chunk too large · 422 invalid input, limits, incomplete revision, no valid rows.
