# Me API

Base path `/v1`. Both routes need `Authorization: Bearer <token>`, and return `401` without one. Shapes follow Laravel `UserResource` and `WorkspaceResource`, limited to what vimdesk-web reads plus the fields its profile form sends back.

## GET /v1/me

```json
{ "data": { "user": {
  "id": "<uuid>", "user_id": "<uuid>",
  "name": "Ada Lovelace", "first_name": "Ada", "middle_name": null, "last_name": "Lovelace",
  "email": "ada@example.com", "initials": "AL",
  "has_pin_code": false, "color": null, "meta": null, "interface_language": null, "active": true,
  "avatar_path": "https://eu.ui-avatars.com/api/?name=Ada+Lovelace",
  "created_at": "…", "last_updated_at": "…",
  "permissions": [ { "workspace": "<workspace uuid>",
                     "permissions": ["workspace_admin", "workspace_owner"] } ],
  "relationships": {
    "pending_invites": [ {
      "id": "<invite uuid>", "status": "pending", "workspace_name": "acme",
      "invite_code": "<invite uuid>", "invited_on": "…",
      "invited_by": { "email": "owner@example.com", "name": "Owner Name" }
    } ],
    "teams": [ { "id": "<team uuid>", "name": "Admin Team",
                 "description": "This team consist of admins of the workspace",
                 "permissions": ["workspace_admin"], "created_at": "…", "last_updated_at": "…",
                 "meta": null, "relationships": [] } ],
    "employment_group": null
  }
} } }
```

- `permissions`: one entry per **active** membership in a live workspace. The list is the union of the user's live teams' grants in that workspace, sorted, with `workspace_owner` appended for the creator (Laravel appends it last too). A member in no team gets `[]`. `workspace` is the same uuid as `/me/workspaces[].id`; `auth.store.js` matches on it.
- `relationships.teams`: the user's live teams in workspaces where they're an active member, oldest first, in Laravel `TeamResource` shape. Users aren't loaded here, so `relationships` is `[]`. Signup seeds an `Admin Team` holding `workspace_admin` with the owner in it.
- `pending_invites`: pending, unexpired invites to the user's email (case-insensitive), newest first. **Empty until the email is verified**, because an unverified account may be squatting someone else's address. `invite_code` is the invite id, accepted by `POST /v1/invites`.
- Not ported yet, so these keep fixed values:
  - `user_id` is the uuid (Laravel sent the int PK; Security.vue skips uuid-looking values)
  - `has_pin_code: false`
  - `interface_language: null` (the frontend falls back to `nl`)
  - `employment_group: null`

## GET /v1/me/workspaces

```json
{ "data": [ { "id": "<uuid>", "workspace_name": "acme", "workspace_type": "retail",
              "created_at": "…", "last_updated_at": "…", "relationships": [] } ] }
```

Active memberships in live workspaces, oldest membership first, which is the same order as login's `workspace_ids`.

## Laravel differences

- Inactive memberships are left out of both `permissions` and `/me/workspaces`. Laravel included them in those two places, while login already excluded them.
- Expired invites aren't listed. Laravel listed them, and they failed only when answered.
- `workspace_profile` (shape in `workspaces.md`) is omitted until a profile is saved, as Laravel omitted it when null. POS receipt views read it without `?.`, so a workspace needs a saved profile before printing.
- `pending_invites_count` and `users_count` aren't sent, since the frontend never reads them.
