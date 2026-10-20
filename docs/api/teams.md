# Teams API

Base path `/v1/workspace/{workspace}/users/teams`. Every route runs the workspace check from `permissions.md` (401 / 404 outsider / 403 inactive) and then needs its own permission. Owner and `workspace_admin` pass every check.

A team carries permission grants. A member's permissions in a workspace are the union of their live teams' grants. The permission check (403) runs before body validation (422), in Laravel's order.

**Admin teams.** A team that holds `workspace_admin` changes only through an admin, meaning the owner or a holder of `workspace_admin`. For anyone else, these all return `403 This action is unauthorized.`:

- adding users to it or removing them
- deleting it
- removing its `workspace_admin` grant
- inviting someone into it (`invites.md`)

This closes the Laravel path where `team.invite` or `invite.create` alone reached admin rights.

Unknown, deleted, malformed or other-workspace `{team}` ids are all `404 {"message": "The requested resource could not be found."}`.

## Team resource

```json
{ "id": "<uuid>", "name": "Marketing", "description": "…",
  "permissions": ["relation.create", "relation.get"],
  "created_at": "…", "last_updated_at": "…", "meta": { "any": "json" },
  "relationships": { "users": [
    { "id": "<user uuid>", "name": "Ada Lovelace", "initials": "AL", "email": "…",
      "avatar_path": "https://eu.ui-avatars.com/api/?name=Ada+Lovelace", "color": null } ] } }
```

`permissions` is sorted. `/v1/me` lists teams in the same shape with `relationships: []`.

## Routes

| Route | Permission | Body / query | Success |
|---|---|---|---|
| `POST /` | `team.create` | `name` required ≤55, `description` required ≤225, `permissions` optional array, `meta` optional JSON | `201 {"data": {"team_id": "<uuid>"}}` |
| `GET /` | `team.list` | `q` ≤100 (name or description, case-insensitive, `%` `_` literal), `sort` `name` / `-name` / `-created_at` (default oldest first), `page`, `page_size` (default 50, max 100) | `{"data": [team…], "meta": {current_page, from, last_page, per_page, to, total}}` |
| `GET /{team}` | `team.get` | | `{"data": team}` |
| `POST /{team}` | `team.update` | `name`, `description`, `meta`; an absent key keeps the value, `null` meta clears it | `{"message": "ok"}` |
| `DELETE /{team}` | `team.delete` | | `{"message": "ok"}` |
| `POST /{team}/add-users` | `team.invite` | `user_ids`: workspace members (active or not) | `{"message": "ok"}` |
| `POST /{team}/remove-users` | `team.invite` | `user_ids`: users already in the team | `{"message": "ok"}` |
| `POST /{team}/permissions` | `team.permissions` | `permissions` array, a delta (below) | `{"message": "ok"}` |

Validation messages follow Laravel:

- `The name field is required.` and `The name field must not be greater than 55 characters.` (the same for `description` with 225)
- `The permissions field must be an array.`
- `user ids must be an array of valid user_ids`: not an array, a bad uuid, or someone outside the workspace
- `The user ids field is required.`: missing or empty
- `This user ids does not exist in the team`: on remove

Unknown query params (`tab`, `team_id`, …) are ignored, and so is an unknown `sort` value.

## Grants

- **On create**, `permissions` keeps exact catalog names (lowercased) and drops the rest. `workspace_admin` is kept only when the owner sends it.
- **The permissions route takes a delta**, as the vimdesk-web editor sends it:
  - `"relation.get"` adds the grant.
  - `"-relation.create"` removes it.
  - When both appear in one call, the removal wins (Laravel adds first, then removes).
  - Non-strings, `""`, `"-"`, group-only names and non-catalog names are ignored.
  - `workspace_admin` is added only for the owner. Anyone else's attempt is dropped without an error.
  - Removing `workspace_admin` needs an admin (403 otherwise). Any other grant can be removed by anyone with `team.permissions`.
- **No "only grant what you hold" rule.** A `team.permissions` holder can add any catalog name to a team, their own included. `team.create` can likewise create a team with any catalog name. This is Laravel parity and a known risk in the risk register.

Deleting a team is a soft delete. Its grants and members stay in the database but stop counting.

## Laravel differences

- `GET /{team}` checks `team.get` and `DELETE` checks `team.delete`. Laravel checked `team.view` (not in the catalog, so only owner/admin passed) and `invite.delete`.
- `sort` accepts only the three values above. Laravel sorted on any column, and an unknown column gave a 500. The same goes for `filter[...]`, which isn't supported at all.
- An explicit `null` (or blank) `name`/`description` on update is a 422; Laravel hit the NOT NULL column and returned 500.
- Adding users to an unknown team is a 404; Laravel returned 500.
- No spatie role sync, Meilisearch indexing or activity log.
