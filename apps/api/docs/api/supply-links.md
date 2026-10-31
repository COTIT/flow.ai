# Supply links

Supply links are how a farm workspace (R, the RUAHA project) and a shop workspace (A, AFM) consent to supply lots crossing between them. R grants with an allow-list of the farmer fields A may show on its public lot pages; A accepts; either side revokes. A link never opens R's records (farmers, kg shares, prices, passbooks) to A. Moving stock over a live link is a later step. Code: `src/modules/supply_links/`, migration `supply_links`.

All routes are for workspace admins (the owner or `workspace_admin`). Other members get `403`, checked before the body is read; non-members get `404`.

## `POST /v1/workspace/{workspace}/supply-links`

```json
{ "to_workspace_id": "…", "fields": ["village", "farmer_count", "harvest_window"] }
```

- `to_workspace_id`: another live workspace (`422` for itself, unknown or malformed).
- `fields`: at most 10, from `farmer_count`, `harvest_window`, `village`. An empty list is fine (the lot crosses, nothing about its farmers is shown). `farmer_names` and `farmer_photos` are refused with `{field} can't be shared until the data-protection gate opens.` until the user confirms Tanzania PDPC registration and the cross-border transfer permit.
- At most one pending or live link per pair (`409`).

`201` with the link, `status: "pending"`.

## `GET …/supply-links?page&page_size`

Links this workspace granted (`direction: "outgoing"`) or received (`"incoming"`), newest first. A workspace that is not a side of a link never sees it.

Link: `{id, direction, partner: {id, name}, fields, status, granted_at, accepted_at, revoked_at}`. `status` is `pending`, `live` or `revoked`.

## `POST …/supply-links/{link}/accept`

Only the receiving workspace accepts (the granting side gets `403`). Pending becomes live; a live or revoked link is `409`. A workspace that is not a side gets `404`.

## `DELETE …/supply-links/{link}`

Either side revokes, `204`, again `204`. A revoked link stays revoked: grant a new one. Stock already received over the link keeps crediting farmers when it sells (a later step).
