# Tax profiles

Per workspace, the verified tax rules a sale or purchase line is taxed by (Flow ERP masterplan §13 TAX, §15 TAX). No rates are seeded: each profile is entered by the workspace with evidence and verified by a reviewer. Migration `tax_profiles`.

All paths sit under `/v1/workspace/{workspace}`. Lists return `{data, meta}`; detail and actions return `{data}`. Writes need an `Idempotency-Key` header and the C/A envelope from `accounting.md`. A `client_ref` replays its first answer for good; the same ref with another body is 409.

## Model

| Field | Rule |
|---|---|
| `country` | ISO 3166-1 alpha-2 |
| `jurisdiction_scope` | ISO 3166-2 subdivision of `country` (`TZ-02`), or null for the whole country |
| `effective_from`, `effective_to` | dates; `to` is exclusive and after `from`; null `to` = open |
| `transaction_types` | non-empty subset of `sale`, `purchase` |
| `product_categories` | category ids of this workspace; empty = every category |
| `components` | 1 to 10: `{code, name, kind, rate, recoverable}` |
| `exemptions` | up to 100: `{relation_id` or `product_category_id, reason, evidence_ref}` |
| `tax_point` | `invoice` or `supply` |
| `evidence_ref` | up to 255 chars; required at verify |

- `kind` must be `percentage`. Fixed amounts and compound tax (tax on tax) are refused (422).
- `rate` is a percent as an exact decimal string, 0 to 100, at most 8 decimals. 0 is a zero-rated component.
- `recoverable`: whether purchase tax can be reclaimed, all or nothing. Partial recoverability is not supported yet.
- `tax_point` `payment` (cash-basis timing) is refused (422).
- An exemption makes every component 0 for that relation or product category.

Revisions: `lineage_id` is revision 1's id. `revision` counts corrections. `corrects_id` names the revision this one corrects, and `superseded_by_id` names the verified correction that replaced this one.

`allowed_actions`: `update`, `verify` for a draft; `correct` for the current verified revision; none for a superseded one.

## Routes

| Method, path | operation_id | Permission |
|---|---|---|
| GET `/tax-profiles?country=&state=&page=&page_size=` | `tax_profiles_list` | `taxprofile.read` |
| POST `/tax-profiles` | `tax_profiles_create` | `taxprofile.configure` |
| GET `/tax-profiles/{id}` | `tax_profiles_get` | `taxprofile.read` |
| PATCH `/tax-profiles/{id}` | `tax_profiles_update` | `taxprofile.configure` |
| POST `/tax-profiles/{id}/verify/review` | `tax_profiles_verify_review` | `taxprofile.verify` |
| POST `/tax-profiles/{id}/verify` | `tax_profiles_verify` | `taxprofile.verify` |

- **Create**: `C{client_ref, payload}`. The payload is the model fields plus an optional `corrects_id`, which must be the current verified revision of a lineage. A correction becomes the next revision; a revision can be corrected only once (409 `tax_profile_correction_exists`). Returns 201 with the draft.
- **PATCH**: `A{client_ref, expected_version, payload}`. Drafts only. A field left out keeps its value, and null clears an optional one. `components` or `exemptions`, when sent, replace the whole list. Version + 1.
- **Review**: no body, no writes. Returns `{profile, review_fingerprint}`; the fingerprint hashes the id, the version and every rule field.
- **Verify**: `A{client_ref, expected_version, payload:{review_fingerprint, evidence_ref}}`. Recomputes the fingerprint under lock. Verifies run one at a time per workspace (advisory lock `erp.tax|<ws>`). They refuse a current verified profile of another lineage that has the same country and subdivision and shares a transaction type, a category (empty = all) and a day. A verified correction sets `superseded_by_id` on the revision it corrects.

A verified revision never changes. Its fields, components and exemptions are fixed by triggers; only the one-time `superseded_by_id` link may be set. To change the rules, create a correction.

## Errors

| Status | When |
|---|---|
| 403 | missing permission, inactive membership |
| 404 | unknown or other workspace's profile |
| 409 `stale_document` | `expected_version` or `review_fingerprint` out of date |
| 409 `tax_profile_verified` | PATCH, review or verify on a verified revision |
| 409 `tax_profile_correction_exists` | the revision already has a correction |
| 409 | `client_ref` reused for another request |
| 422 | invalid or unsupported input, unknown category / relation / `corrects_id`, missing `Idempotency-Key` |
| 422 `tax_profile_overlap` | verify would overlap a current verified profile |

## Line calculation (SQL)

`tax_line(ws, profile, on_date, txn, amount, includes_tax, category, relation)` returns one row per component: `line_no, code, rate, base, tax, recoverable, exempt`. Amounts are minor units (bigint).

- Exclusive (`includes_tax` false): `base = amount`, `tax = round(amount × rate / 100)` per component.
- Inclusive: `base = round(amount × 100 / (100 + Σ rate))`. Every component rounds on `base`, except the last non-zero one, which takes the exact remainder, so `base + Σ tax = amount`.
- Rounding is half away from zero via `money_round_div`, exact. Negative amounts (credits) mirror positive ones.
- Refusals raise SQLSTATE 22023 with message `tax_profile_not_verified` (also for another workspace's id), `tax_profile_not_effective`, `tax_profile_wrong_transaction` or `tax_profile_wrong_category`.

## Not wired yet (later step)

Pricing, orders, storefront, Till, invoices, purchasing and bills still use the legacy `vat_groups` rate. Planned single boundary:

- One server quote evaluates `tax_line` for all lines in one query.
- Each line keeps a snapshot: profile, revision, components, base, tax, exempt flag, recoverable flag and tax point.
- Legacy `vat_rate`, `line_vat` and `vat` become projections of that snapshot. A client's legacy rate never overrides a verified profile.
- Linked invoices copy the order's tax, credits reverse the original, and issued values are never recalculated.

**Open wiring decision:** today VAT is rounded once per rate on the document sum (`pricing/engine.rs`); `tax_line` rounds per line and component. Document VAT can then differ by up to half a minor unit per line. Decided when wiring lands.
