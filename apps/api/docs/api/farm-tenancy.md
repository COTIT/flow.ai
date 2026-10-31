# Farm tenancy: villages and farm roles

The RUAHA farm domain lives inside a workspace. A RUAHA project is one workspace; its villages are scopes inside it. Code: `src/modules/farm_villages/`, `src/modules/farm_memberships/`, guard `src/http/farm.rs`, migration `farm_tenancy`.

## Who gets in: the farm guard

Every `/v1/workspace/{workspace}/farm/...` route runs `FarmAccess`:

- A farm role is `farmer`, `field_officer`, `ops` or `admin`. Farmers and field officers hold one village each. ops and admin cover every village of the workspace.
- Staff roles (`field_officer`, `ops`, `admin`) count only while the user is an active workspace member. Deactivate the member and their farm role stops working on the next request.
- Farmers are **not** workspace members. They pass the farm guard on their farmer role alone, and every other workspace route answers them `404`.
- An active member with no farm role passes the guard but holds no farm rights: they see villages and their own (empty) memberships.
- No live role and no membership, an unknown or deleted workspace, or a malformed id: `404`. No token: `401`.

Mixed roles are allowed. Someone can be a farmer in one village and an officer in another; staff rights apply only where they are staff.

## Audit trail

Every insert, update and delete on a farm table writes a row to `farm_audit_log` through a trigger: table, record id, action, before/after JSON, time, and the actor. The service stamps the actor with `set_config('genesy.actor', <user>, true)` at the start of each write transaction (`FarmAccess::begin`). The log cannot be updated or deleted.

## Villages

### `GET /v1/workspace/{workspace}/farm/villages?page&page_size`

Any farm member. `200 {"data": [...], "meta": {...}}`, ordered by code. Each village: `id, code, name, latitude, longitude, primary_language, created_at, updated_at`. Coordinates are decimal text with 6 decimals, or `null`.

### `POST /v1/workspace/{workspace}/farm/villages`

ops or admin; others `403`.

```json
{ "id": "019a…", "code": "ILUNDO", "name": "Ilundo", "latitude": "-7.7", "longitude": "35.7", "primary_language": "sw" }
```

| Field | Rule |
|---|---|
| `id` | optional uuid. Send one made on the device so a retry is safe |
| `code` | required, 1–32 chars, unique in the workspace |
| `name` | required, 1–120 chars |
| `latitude` | optional decimal, −90 to 90 |
| `longitude` | optional decimal, −180 to 180 |
| `primary_language` | `sw` (default) or `en` |

`201 {"data": village}`. If a village with this `id` already exists in the workspace, the reply is `200` with that village and nothing is written. The same `id` used in another workspace: `409`. `422` on invalid input or a code already taken.

### `PATCH /v1/workspace/{workspace}/farm/villages/{village}`

ops or admin. Only the fields sent change; `null` clears a coordinate. `200 {"data": village}`, `404` for another workspace's village, `422` as above.

Villages are never deleted.

## Farm roles

### `GET /v1/workspace/{workspace}/farm/memberships?page&page_size`

ops, admin and workspace admins see every role in the workspace; anyone else sees their own. Newest first, revoked rows included. Each row: `id, user {id, name}, role, village_id, created_at, revoked_at`.

### `POST /v1/workspace/{workspace}/farm/memberships`

Grants a staff role to an active workspace member.

```json
{ "user_id": "…", "role": "field_officer", "village_id": "…" }
```

| Field | Rule |
|---|---|
| `user_id` | required uuid of an active member of this workspace |
| `role` | `field_officer`, `ops` or `admin`. `farmer` is refused: `role must be admin, ops or field_officer (farmers get logins from an officer)` |
| `village_id` | required for a field officer (`a field officer needs a village`). Refused for ops and admin (`ops covers the whole workspace: leave village out`). Must be a village of this workspace |

Who may grant:

- A workspace admin (owner or `workspace_admin`): any role to anyone, themselves included. This is how a new workspace gets its first farm admin.
- Farm admin: any role to anyone but themselves.
- ops: field officers only, never themselves.
- Everyone else: `403`.

Replies: `201` for a new role. `200` with the existing row when the role is already held (no second row), or when it was revoked and is now restored. `422` on invalid input, an unknown village, or a user who isn't an active member.

### `DELETE /v1/workspace/{workspace}/farm/memberships/{membership}`

`204`. Sets `revoked_at`; revoking again is also `204`. Same authority as granting, judged on the row's role: ops can revoke farmers and field officers, not ops or admins. Nobody revokes their own row except a workspace admin. Unknown id: `404`.

There is no route that changes a role. Revoke and grant again instead.
