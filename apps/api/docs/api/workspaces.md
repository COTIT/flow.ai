# Workspace record API

Base path `/v1/workspace/{workspace}`. Both routes run the workspace check from `permissions.md`. The permission check comes before body validation.

## GET /v1/workspace/{workspace}

Needs `workspace.get`. Laravel checked `workspace.view`, which no catalog entry grants.

```json
{ "data": {
  "id": "<uuid>", "workspace_name": "acme", "display_name": "Acme BV", "workspace_type": "retail",
  "pending_invites_count": 0, "users_count": 1,
  "workspace_profile": {
    "company_name": null, "vat_number": null, "coc_number": null,
    "address": { "address": null, "house_number": null, "house_number_addition": null,
                 "address2": null, "zipcode": null, "city": null, "state": null, "country": null },
    "account": { "account_name": null, "account_number": null, "bic_swift": null, "credit_limit": null },
    "phone_number": null, "email": null, "website": null,
    "logo_file_id": null, "logo": null, "language": null },
  "created_at": "…", "last_updated_at": "…", "meta": null, "relationships": [] } }
```

- `pending_invites_count` counts pending invite rows, expired ones included (Laravel parity; the invite list shows them as pending too).
- `users_count` counts every membership, inactive ones included (Laravel parity).
- `workspace_profile` is always present, and `address` and `account` are always objects. `settings.store.js` replaces its whole form with this object and binds `address.city` and the like, so a null there would break the page.

## POST /v1/workspace/{workspace}/update

Needs `workspace.update`. Send any subset of the keys below:

- An absent key keeps its value.
- `null` or a blank string clears it.
- Unknown keys are dropped, including `workspace_type` and `workspace_name`; neither changes here.
- A sent `address` or `account` replaces the stored one as a whole (Laravel `array_merge`).

Success: `{"message": "ok"}`.

| Key | Rules |
|---|---|
| `company_name`, `vat_number`, `coc_number` | string ≤55 |
| `phone_number` | string ≤55, international: `+` and 8..=15 digits, separators ` -().` (`Phone number is not valid`) |
| `language` | `en` or `nl` (`The selected language is invalid.`) |
| `email` | valid email |
| `website` | `http(s)://` URL with a dotted host (`The website must be a valid URL.`) |
| `logo_file_id` | must be null: files aren't ported yet (`The selected logo file id is invalid.`) |
| `address` | null, or an object. Required: `address` (≤255), `house_number` (numeric), `zipcode` (≤20), `city`, `country`. Optional: `house_number_addition` (≤10), `address2`, `state`. Errors are keyed `address.<field>`. |
| `account` | null, or an object: `account_name` ≤255, `account_number` a valid IBAN (mod-97), `bic_swift` a valid BIC, `credit_limit` numeric ≥0. Errors are keyed `account.<field>`. |
| `meta` | null or a JSON object (`The meta must be an array.`) |

The merge is one statement (`profile = profile || patch`), so two saves at once can't drop each other's keys. Values are stored normalized: IBAN without spaces and uppercase, `credit_limit` as a JSON number, `house_number` trimmed. Numeric strings are capped at 20 chars. A body with no known keys writes nothing.

`GET /v1/me/workspaces` items carry the same `workspace_profile` once a profile has been saved, **without `account`**: every member reads that list, so bank details stay behind `workspace.get`. Until a profile is saved the key is omitted, as in Laravel.

## Laravel differences

- The phone check validates shape only; Laravel ran libphonenumber.
- The URL check accepts `http(s)` only, with no userinfo, quotes or angle brackets. Laravel also took other schemes.
- The IBAN check is ASCII mod-97 with no per-country length table (Laravel had one).
- `/me/workspaces` leaves out `account`. Laravel exposed it to every member.
- The address output drops Laravel's derived fields: `formatted`, `search_format`, `label`, `main_address`, `type`, `attn`, `vat_rates`.
- `logo` is always null until files are ported.
- Laravel cached the profile forever and could merge onto a stale copy. Here the merge runs in SQL on the current row.
