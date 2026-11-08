# Workspace users API

Base path `/v1/workspace/{workspace}/users`. Both routes run the workspace check from `permissions.md` first.

## GET /v1/workspace/{workspace}/users

Needs `workspace.list` (Laravel parity). This is the settings users tab (`tabs/users.vue`).

| Query | Rules |
|---|---|
| `q` | ≤100 chars. Case-insensitive match on email, `first middle last` or `first last`. `%` and `_` are matched literally. |
| `team_id` | uuid; only members of that live team in this workspace. A bad uuid gives 422 `The team id must be a valid UUID.` |
| `has_pin_code` | `1`/`true` matches no one (PIN login isn't ported); `0`/`false`/blank = no filter; anything else 422 `The has pin code field must be true or false.` |
| `page`, `page_size` | default 50, max 100 |

Unknown params are ignored.

```json
{ "data": [ {
    "id": "<user uuid>", "name": "Ada Lovelace", "email": "ada@example.com",
    "user_type": null, "avatar_path": "https://eu.ui-avatars.com/api/?name=Ada+Lovelace",
    "initials": "AL", "color": null, "active": true,
    "permissions": [ { "workspace": "<this workspace>", "permissions": ["workspace_admin", "workspace_owner"] } ],
    "teams": [ { "id": "<uuid>", "name": "Admin Team", "description": "…",
                 "permissions": ["workspace_admin"], "meta": null } ],
    "relationships": [], "created_at": "…" } ],
  "meta": { "current_page": 1, "from": 1, "last_page": 1, "per_page": 50, "to": 1, "total": 1 } }
```

- Members are listed in membership order, inactive ones included; `active` is the membership flag.
- `teams` holds the user's live teams in this workspace. It sits at the top level because `users.vue` reads `teams.0.name`.

## GET /v1/workspace/{workspace}/users/{user}

Needs `user.get`. Used by the user detail page (`user-tabs/*.vue`).

The body is `{"data": UserResource}`, in the same shape as `GET /v1/me`'s `data.user` (see `me.md`), with these differences:

- `active` is the membership flag in this workspace.
- `permissions` has one entry, for this workspace.
- `relationships.teams` lists the user's live teams in this workspace.
- `relationships.pending_invites` is `[]`.

An unknown or malformed id, or a user who isn't a member here, gives 404.

## DELETE /v1/workspace/{workspace}/users/{user}

Offboarding: ends a member's access to this workspace and keeps their history. Owner or `workspace_admin` only. No body. Success is `204` with no content.

In one transaction:

- the membership turns inactive (`active: false`). The row stays, so deals and warehouses that name them as agent or manager keep the link, and orders, invoices and audit keep their author;
- they leave every team of this workspace;
- their farm roles in this workspace are revoked (`revoked_at` set);
- their assistant sessions here close and pending (`proposed` / `approved`) assistant actions and batches are cancelled;
- their logins are deleted only when no active workspace membership and no live farm role is left anywhere. Otherwise their login keeps working in their other workspaces; this workspace refuses them on the next request, because every request re-reads the membership.

Inviting them again (`POST /invites`, then accepting) makes the membership active again. Only the team in the new invite applies; old teams are not restored.

| Status | When |
|---|---|
| 204 | Access removed |
| 403 | Caller's membership is inactive, or caller is neither owner nor `workspace_admin` |
| 404 | Unknown or malformed workspace or user, user not an active member here, caller not a member |
| 409 | `You can't remove yourself from this workspace.` (checked first) or `The workspace owner can't be removed.` |

## Laravel differences

- `permissions` covers only this workspace. Laravel listed every workspace of that user, which leaked other tenants' ids to an admin here. The detail page also hides the user's pending invites.
- The detail page checks `user.get`. Laravel checked nothing, so any member could open any colleague's record.
- `q` is a SQL match, not Meilisearch. A `team_id` from another workspace returns no one, where Laravel returned its members.
- No `custom_sections` / `custom_section_forms` (custom fields aren't ported), and `employment_group` is null.
