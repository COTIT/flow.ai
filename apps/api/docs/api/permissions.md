# Permissions API

Base path `/v1`. How access works on every `/v1/workspace/{workspace}/...` route, plus the catalog the team permission editor reads.

## Who may do what

Every workspace route runs the `WorkspaceAccess` check first:

| Caller | Result |
|---|---|
| no or bad token | `401` |
| not a member, or workspace unknown, deleted or malformed | `404 {"message": "The requested resource could not be found."}`, so outsiders can't tell the workspace exists |
| member with an inactive membership | `403 {"message": "This action is unauthorized."}` |
| active member | passes; the route then checks its own permission name |

A member's permissions in a workspace are the union of the grants on their live (not deleted) teams in that workspace. A route's check passes when any of these holds:

- the caller created the workspace (`workspace_owner`, computed, never stored)
- one of their teams holds `workspace_admin`
- the name is frontend-only: `product.view_price`, `product.view_purchase_price`, `product.view_margin` (they only hide UI columns, Laravel `config/rbac.php`)
- the exact `group.action` name was granted

Otherwise the route returns `403 This action is unauthorized.`

In code, a route declares its permission as a type, checked by the `Can<P>` extractor before the body is parsed. So a member without the permission gets 403 even for a malformed body, as in Laravel's FormRequest order:

```rust
permissions!(TeamCreate => "team.create");
pub async fn create(access: Can<TeamCreate>, ValidatedJson(input): ValidatedJson<CreateTeamRequest>) -> …
```

Member-only routes take `WorkspaceAccess` instead. `access.is_owner()` and `access.is_admin()` (owner, or a holder of `workspace_admin`) serve the extra rules, e.g. the admin-team gate in `teams.md`.

Signup seeds an `Admin Team` with `workspace_admin` and puts the owner in it (Laravel `WorkspaceSetup`).

## GET /v1/workspace/{workspace}/permissions

Any active member. No permission name needed (Laravel had no policy here either).

```json
{ "data": {
  "association": ["create", "delete", "get", "list", "update"],
  "…": [],
  "workspace": ["get", "global-search", "list", "send-message", "update", "view-event"],
  "__classification": {
    "frontend_only": ["product.view_price", "product.view_purchase_price", "product.view_margin"],
    "api_enforced": ["association.create", "…all 242 catalog names…"]
  }
} }
```

- 47 groups and 242 names. 43 groups / 222 names are frozen from Laravel's `src/Domain/**/V1/Permissions/permissions.json` exactly as `PermissionHelper` serves them: groups sorted, actions sorted, `__classification` last. Source: `src/modules/permissions/catalog.rs`.
- `salesgroup`, `shipmentmethod` and `vatgroup` (`create, delete, get, list, update`) are added by genesy. Laravel checked these names but had no catalog file for them, so only the owner or a `workspace_admin` could pass.
- `specialoffer` (`create, delete, list, update, view`) is added the same way (F3b). vimdesk-web already gates its discounts page on `specialoffer.list`.
- `workspace_admin` isn't in the catalog. Only the owner may grant it.
- vimdesk-web drops `__classification` when it builds the editor.

## Laravel differences

- A grant must be an exact catalog `group.action`. Laravel matched only the last dot segment, so `relation.anything.list` slipped in while `manufacturing.release.compliance_override` (a real catalog entry) could never be granted.
- Catalog gaps are kept as Laravel served them:
  - `activity` is missing because its file is named `permission.json`
  - the nested TicketManagement and CustomField files aren't read
  - the frontend-only names aren't grantable, since no catalog group lists them
- Spatie roles aren't ported. Laravel kept one role per team as a copy of the team's permissions; `team_permissions` holds the same set directly.
