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

201 `{data: {…register, key}}`. This is the only time `key` is shown. The device key is an app key of kind `till` with scopes `till:login` and `catalog:read`, so the device can also read `/v1/storefront/products` for its offline cache. It is listed under `/app-keys`, and revoking it there disables the register. The `/app-keys` route itself never issues `till:login`.

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
- 401: missing, revoked or rotated key, or a key bound to no register. 403: a key without `till:login`.

## Session (till token)

`Authorization: Bearer <token>`. A session stops working when it is ended or expired, when the register's key is rotated or revoked, when the till user is set inactive, or when the workspace is deleted. Then: 401.

- `GET /till/me` → `{data: {register, user, expires_at}}`
- `POST /till/logout` → 204
