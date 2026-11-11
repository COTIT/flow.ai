# Storefront home designs

Phase one configures only the Shop home page. Existing catalog, cart, checkout, stock and lot-passport APIs remain authoritative. Designs contain presentation and catalog bindings only.

All staff routes below require a live bearer login and workspace owner or `workspace_admin`. An ordinary member gets 403, another workspace's user or revision gets 404. Public reads use the existing app key with `catalog:read`. Drafts never pass through the public route. Responses use `Cache-Control: no-store`; existing Shop catalog caching is unchanged.

## Staff routes

Prefix: `/v1/workspace/{workspace}/storefront/pages/home`.

| Method/path | Request | Success |
| --- | --- | --- |
| `GET` | none | 200 page state |
| `POST /revisions` | `{expected_version, document}` | 201 new draft and page state |
| `GET /revisions` | `page`, `page_size` (default/max 50) | 200 paginated revision metadata |
| `GET /preview` | required `revision_id` UUID | 200 validated document and bounded public catalog collections |
| `POST /restore` | `{expected_version, revision_id}` | 201 historical document copied into a new draft |
| `POST /publish` | `{expected_version, revision_id}` | 200 current draft published and page state |
| `POST /edits` | `{expected_version, base_revision_id?, changes}` | 201 targeted changes saved as a new draft and page state |
| `POST /edits/preview` | same body, no `Idempotency-Key` | 200 dry run: changed document, collections and change rows; stores nothing |

`GET /v1/workspace/{workspace}/storefront/design-capabilities` returns `{data:{schema_version,document_schema,constraints,components}}`. The document JSON schema is generated from Rust DTOs. Component property-schema references point into that schema's `$defs`. Registry entries describe supported props, defaults, children, responsive behavior, data/runtime requirements, accessibility, examples and deprecation. Unique section IDs and canonical serialized byte size also receive Rust validation.

Page state is `{data:{version,draft_revision_id,published_revision_id,draft}}`. An untouched workspace has version 0 and null pointers/draft. A draft is `{revision_id,document}`. Save, restore and publish return this same state shape. History rows contain `revision_id`, `parent_revision_id` (the draft it replaced, null for the first), `created_at`, `created_by`, `restored_from_revision_id` and `was_published`. Rows are newest first, so row 0 is the current draft.

Preview returns `{data:{version,revision_id,document,collections:[{section_id,products:[...]}]}}`. Products use the existing storefront DTO and rules. Preview revision IDs remain scoped to the requested workspace and home page; they are never shareable preview credentials.

## Writes, retries and rollback

Every POST requires `Idempotency-Key`. Retry with the same key and identical body: the existing actor-bound idempotency store replays the original answer for one day. Reusing the key for another actor or payload is 422. The SPA's existing idempotency mechanism supplies this header.

`expected_version` must match current page state. The first save expects 0. Each successful save, restore or publish increments the version. A stale write is 409 and changes nothing. Concurrent writes serialize on the page row. Revision insertion, pointer changes, audit events and idempotency answers commit together.

Publishing requires the selected revision to be the current draft, revalidates the entire document and its category bindings, and atomically advances the published pointer. Publishing an already published revision again with a fresh key is 409. Earlier revisions remain available. To roll back, restore a historical revision into a new draft, review its preview, then explicitly publish the new draft.

Revision content and audit events are immutable in PostgreSQL. Actor and login UUID snapshots survive user/token cleanup; they contain no bearer secrets. Deleting an existing workspace still cascades its design data. There is no design-history delete endpoint.

## Targeted edits

`POST /edits` changes the current draft without sending a whole document. `base_revision_id` must be the current draft (`draft_revision_id`); leave it out only while the page has no draft, and the edit then starts from the built-in starter (the registry examples: hero `hero-main`, grid `featured-products`). A stale `expected_version`, or a base that is not the current draft, is 409 and changes nothing. The changed document passes the same checks as a save (fields, counts, size, live same-workspace categories) and is stored as a new immutable revision whose `parent_revision_id` is the base; the audit event is `saved`. `Idempotency-Key` is required and replays like the other writes.

`changes` holds 1 to 10 items, applied in order. Each has an `op`:

| `op` | Fields | Rule |
| --- | --- | --- |
| `set_theme` | `theme` | `forest` or `neutral` |
| `set_hero` | `section_id`, any of `heading`, `body`, `layout` | target must be a hero; at least one field |
| `clear_hero_body` | `section_id` | removes the hero body |
| `set_product_grid` | `section_id`, any of `heading`, `category_id`, `limit`, `columns` | target must be a product grid; at least one field |
| `show_all_products` | `section_id` | removes the category binding |
| `add_section` | `section` (a full hero or product grid section), optional `position` | `position` 1 is first; absent appends |
| `remove_section` | `section_id` | at least one section remains |
| `move_section` | `section_id`, `position` | 1-based position |

Unknown ops or fields are 422; so is a whole `document`. An edit that leaves the document as it was is 422 on `changes` ("This change leaves the design as it is."). Errors name the change (`changes.0.section_id`, `changes.1.position`) or the resulting document path (`document.sections.0.props.heading`).

`POST /edits/preview` takes the same body without a key and stores nothing: no page row, revision, event or idempotency answer. It reads page and revision from one read-only snapshot. It answers `{data:{version, base_revision_id, document, collections, changes}}`. `changes` are review rows `{field, label, before, after}`: `field` is `theme`, `heading`, `text` (the hero `body`), `layout`, `category`, `limit`, `columns`, `section` or `position`; `label` is the section heading before the change; `before` is null when there was no value; `after` is `—` when a value or section is removed. Categories are shown by name, and a `category` row also carries `category_id`, the id that will be stored (absent when the binding is removed). Columns read `mobile / tablet / desktop`. A new section lists its kind and then every field it will write. The same rows are the assistant review facts for `storefront_design_edit`.

## Preview links

A full-page preview link opens one unpublished draft on the Shop at `/preview/{token}` without a login. It expires, can be revoked, and stops working once its revision is published or replaced by a newer draft (each draft gets its own link). It never publishes or changes anything.

- `POST /workspace/{ws}/storefront/pages/home/preview-links` (`storefront_design_preview_link_create`, owner or `workspace_admin`, `Idempotency-Key` required). Body `{revision_id, hours?}`; `hours` is 1 to 168, default 48. 201 `{data:{link_id, revision_id, created_at, created_by, expires_at, revoked_at:null, status:"active", token}}`. `token` is 64 hex characters and appears in this first answer only: the database keeps its sha256, and a keyed replay answers the same link with `token: null` (issue another link if the first answer was lost). 404 unknown or foreign revision; 409 the revision is not the current unpublished draft, or it already has 5 active links; 422 `hours` out of range, bad body, missing key.
- `GET …/preview-links?revision_id=` (`storefront_design_preview_links`, admin): newest 50 links of that revision, never a token. `status` is `active`, `revoked`, `expired` or `ended` (revision published or no longer the current draft), checked in that order. Another workspace's revision lists nothing.
- `DELETE …/preview-links/{link}` (`storefront_design_preview_link_revoke`, admin): revokes at once; a second call answers the same row. 404 unknown or foreign link.
- `POST /v1/storefront/preview` (`storefront_design_preview_resolve`, app key with `catalog:read`, the Shop server only). Body `{token}`; the token never goes in an API URL. 200 `{data:{revision_id, expires_at, document, collections}}` with the same public product fields as `GET /storefront/products`; no page version, pointers, actors, link or workspace ids. One SQL statement checks hash, the key's workspace, revocation, expiry and that the revision is still the unpublished current draft. Every failure (unknown, malformed, tampered, expired, revoked, ended, another workspace's link) is the same 404 `Not found.`, so the answer never tells which. A key without `catalog:read` is 403. Own limit per app key: `PREVIEW_RATE_LIMIT_BURST` (default 30) at once, one more every `PREVIEW_RATE_LIMIT_REPLENISH_MS` (default 2000), then 429.

Link rows are the audit record (who issued and revoked, with login ids); a trigger refuses every change except one revoke and every delete except the workspace cascade. Issue and revoke log the link and workspace ids at info level, never the token. Responses carry `Cache-Control: no-store`.

## Document v1

Use the shared fixture at `apps/api/tests/fixtures/storefront-design-v1.json` for an API-driven initial draft. Draft authoring UI and conversational edits belong to later phases.

- Root: `schema_version: 1`, `page: "home"`, `theme: "forest" | "neutral"`, ordered `sections`.
- Flat sections; unique lowercase slug IDs, 1–64 characters; at most one `hero` and four `product_grid` sections. The schema allows 1–12 sections, but the per-type limits cap a valid document at 5 (1 hero + 4 grids). Canonical compact document size is at most 32 KiB, checked in Rust; PostgreSQL keeps a 64 KiB backstop on the stored text form. Requests retain the API's 64 KiB body limit.
- Hero: `props.heading` required, 1–160 characters; optional `body`, at most 1,000; `layout: "centered" | "split"`, default `split`. Shop CTA always links to `/shop`. Heading and body are plain text that Shop renders escaped. By decision there is no content filter on this text.
- Product grid: required `heading`, 1–160 characters; `source: "catalog"`, default `catalog`; optional live same-workspace `category_id`; `limit: 1..12`, default 6; `columns` default `{mobile:1,tablet:2,desktop:3}`. Allowed columns: mobile 1–2, tablet 1–3, desktop 1–4. At most 48 products are materialized across four grids.
- Unknown fields, component types and schema versions are rejected. No children, arbitrary CSS/HTML/scripts, URLs, image inputs or API definitions. Optional null body/category bindings normalize to absence; other defaults are filled in stored documents.

Product prices, availability and images are read through the existing catalog service. They are never saved in design JSONB. Existing live product images supply all media in this slice. A deleted category produces an empty collection on an already published page; subsequent publish/restore requires a valid binding.

## Public retrieval and errors

`GET /v1/storefront/pages/home` returns `{data:null}` before publication, otherwise `{data:{revision_id,document,collections}}`. If the stored published document no longer passes current validation, the route returns `{data:null}` and logs a warning with workspace and revision IDs only; Shop then shows its existing home page. Staff preview of that revision still returns 422. It never accepts a revision selector or returns draft state, authors or login IDs. Shop opts into the renderer with its server-side rollout flag and uses design-only uncached retrieval; unsupported/unavailable designs fall back to the existing home page.

Common failures: 401 invalid login/key; 403 missing admin or catalog scope; 404 unknown/foreign revision or workspace; 409 stale version or invalid publication selection; 413 request body too large; 422 invalid document, query, binding or idempotency key. Validation uses the API's `{message,errors}` shape; existing idempotency errors can use `{message}`. Document errors are keyed by path, for example `document.sections.1.id` or `document.schema_version`. Internal database errors stay private.

Assistant (voice) access: `storefront_design_get`, `storefront_design_revisions` and `storefront_design_capabilities` are voice reads; `storefront_design_edit`, `storefront_design_restore` and `storefront_design_publish` are voice writes that run only through a reviewed, signed, single-use assistant action (publish is high risk). Their proposals capture admin-only review facts (field changes against the current draft, or against the published revision for publish) and refuse stale or impossible changes before a card exists. The whole-document save (`POST /revisions`), both previews (`GET /preview`, `POST /edits/preview`) and the public read stay outside voice, and so do the four preview-link operations (they mint and resolve bearer secrets): the voice policy marks them internal and the Rust resolver refuses them. URL analysis and automated component development are deferred.
