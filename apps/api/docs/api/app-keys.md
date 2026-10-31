# App keys

Keys the shop, till, field and partner apps call the API with. Each key belongs to one workspace and holds a fixed list of scopes. Code: `src/modules/app_keys/`, extractor `src/http/app_key.rs`.

## Using a key

Send `Authorization: Bearer gsk_<64 hex>`. The workspace comes from the key, never from the path.

- An unknown, revoked or malformed key, or the key of a deleted workspace, gets `401`.
- A key without the route's scope gets `403` (`This action is unauthorized.`).
- User tokens and app keys never pass for each other: a user token on `/storefront` is `401`, and so is an app key on a user route.
- Requests count against the private rate limit in a bucket per key, and take the workspace's in-flight slots like a member.
- `last_used_at` is stamped at most once a minute.

Scopes: `catalog:read` (the storefront). Kinds: `shop`, `till`, `field`, `partner`.

Storage: only `sha256(key)` is stored, looked up through a unique index. The plaintext is in the create response and nowhere else.

## Management (workspace owner or `workspace_admin`)

Other members get `403`, before the body is read; outsiders and other workspaces get `404`.

### `POST /v1/workspace/{workspace}/app-keys`

```json
{ "name": "Web shop", "kind": "shop", "scopes": ["catalog:read"] }
```

| Field | Rule |
|---|---|
| `name` | required, 1–100 chars |
| `kind` | required, one of the kinds |
| `scopes` | required, 1–20 known scopes (duplicates dropped) |

`201 {"data": {id, name, kind, scopes, prefix, created_at, key}}`. Store `key` now: it is never shown again. `422` on invalid input.

### `GET /v1/workspace/{workspace}/app-keys?page&page_size`

`200 {"data": [...], "meta": {...}}`, newest first. Each row: `id, name, kind, scopes, prefix` (first 8 characters, e.g. `gsk_3f9a`), `created_at, last_used_at, revoked_at`. Revoked keys stay listed.

### `DELETE /v1/workspace/{workspace}/app-keys/{key}`

`204`. Sets `revoked_at`; revoking again is also `204`. Unknown or malformed id: `404`.
