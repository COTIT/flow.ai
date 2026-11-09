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

`GET /v1/workspace/{workspace}/storefront/design-capabilities` returns `{data:{schema_version,document_schema,constraints,components}}`. The document JSON schema is generated from Rust DTOs. Component property-schema references point into that schema's `$defs`. Registry entries describe supported props, defaults, children, responsive behavior, data/runtime requirements, accessibility, examples and deprecation. Unique section IDs and canonical serialized byte size also receive Rust validation.

Page state is `{data:{version,draft_revision_id,published_revision_id,draft}}`. An untouched workspace has version 0 and null pointers/draft. A draft is `{revision_id,document}`. Save, restore and publish return this same state shape. History rows contain `revision_id`, `created_at`, `created_by`, `restored_from_revision_id` and `was_published`.

Preview returns `{data:{version,revision_id,document,collections:[{section_id,products:[...]}]}}`. Products use the existing storefront DTO and rules. Preview revision IDs remain scoped to the requested workspace and home page; they are never shareable preview credentials.

## Writes, retries and rollback

Every POST requires `Idempotency-Key`. Retry with the same key and identical body: the existing actor-bound idempotency store replays the original answer for one day. Reusing the key for another actor or payload is 422. The SPA's existing idempotency mechanism supplies this header.

`expected_version` must match current page state. The first save expects 0. Each successful save, restore or publish increments the version. A stale write is 409 and changes nothing. Concurrent writes serialize on the page row. Revision insertion, pointer changes, audit events and idempotency answers commit together.

Publishing requires the selected revision to be the current draft, revalidates the entire document and its category bindings, and atomically advances the published pointer. Publishing an already published revision again with a fresh key is 409. Earlier revisions remain available. To roll back, restore a historical revision into a new draft, review its preview, then explicitly publish the new draft.

Revision content and audit events are immutable in PostgreSQL. Actor and login UUID snapshots survive user/token cleanup; they contain no bearer secrets. Deleting an existing workspace still cascades its design data. There is no design-history delete endpoint.

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

All phase-one design routes are excluded from assistant discovery and Rust action proposals. Publishing requires the explicit authenticated Office workflow. Voice design tools, cross-subdomain preview tokens, Flow authentication, URL analysis and automated component development are deferred.
