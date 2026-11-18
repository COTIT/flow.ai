# Integrations API

Connections to customer systems (Odoo, NetSuite), source→target pairs and their id mappings. This is the glue under workflow WF-01: an Odoo purchase request is approved in Flow, then created in NetSuite as a purchase order. The frozen contract is `docs/integrations/contracts.md` §2.1, §3.6, §3.7, §4 and §5.2.

Shared rules (guard order, 404s, 422 wording, list paging) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}/integrations`. A malformed, unknown or other-workspace id is 404 `The requested resource could not be found.`

Permissions: reads need `integration.view`, writes need `integration.manage`. The catalog also holds `integration.approve`, `integration.approve_high` and `integration.reconcile`, which the approvals and workflows routes use.

## Storage

| Table | Holds |
|---|---|
| `integration_connections` | provider, label, non-secret `config`, status, last probe, credential expiry |
| `integration_secrets` | one sealed secret per connection (`api_key` for Odoo, `private_key` for NetSuite) |
| `integration_pairs` | workflow `wf01_purchase`, source and target connection, status, config |
| `integration_mappings` | source id → target id per entity, unique per `(pair, entity, source_id)` |
| `integration_poll_cursors` | poll position per connection and stream |
| `external_links` | correlation id `vz-<32 hex>` → source record and target record |

Child tables point at their parent through `(workspace_id, id)` foreign keys, so the database refuses a reference to another workspace's connection or pair.

## Secrets

Secrets are write-only:
- They are sealed with the server key (`CREDENTIALS_KEY`, XChaCha20-Poly1305) and bound to the workspace and to the path `integration:<connection_id>:<name>`. The table holds nonce, ciphertext and tag only.
- GET prints `"********"` for a stored secret and `null` for none. No route ever returns the plaintext, and the plaintext never reaches a log.
- Sending `"********"` back keeps the stored secret. Sending `null` clears it.
- If `base_url` (Odoo) or `rest_domain` (NetSuite) changes, the request must also send a new secret. If it sends the mask instead, or sends no secret, the answer is 422 `Enter this credential again.` on `secrets.<name>`. This stops a member from pointing a kept key at their own server.
- With no `CREDENTIALS_KEY` configured, any secret is refused with 422 `Credential storage isn't configured.`

## Config per provider

Any other key is refused with 422 on `config.<key>`.

| Provider | `config` | Secret |
|---|---|---|
| `odoo` | `base_url` (required), `database` (text or null) | `api_key` |
| `netsuite` | `rest_domain`, `environment` (`production` or `sandbox`), `client_id`, `certificate_id`, `alg` (`ES256`), all required | `private_key` (PEM, up to 4000 chars) |

Address guard on save:
- `base_url` must be `https://<host>[:port]` with no user, path, query or fragment. A trailing `/` is dropped.
- `rest_domain` is a bare host name.
- The host must be a public IP address, or a dotted name that is not `localhost`, `*.localhost`, `*.local`, `*.internal`, `*.lan` or `*.home.arpa`.
- At send time the outbound client checks DNS again and refuses private and metadata addresses.

## Connection status

`draft → active | degraded | unauthorized | expired`, and `disconnected` from any status.

| Status | Reason | When |
|---|---|---|
| `expired` | `credential_expired` | `credential_expires_at` is in the past |
| `unauthorized` | `auth_failed` | a check failed with class `auth` |
| `degraded` | `permission_denied` | a check failed with class `permission` |
| `degraded` | `probe_failed:<capability_key>` | any other check failed (the first one is named) |
| `degraded` | `credential_expiring` | every check passed, and expiry is 14 days away or less |
| `active` | null | every check passed, and expiry is more than 14 days away or not set |

The rows are checked in that order; the first match wins.
- Changing `config` or a secret puts the connection back to `draft`, so it has to be tested again.
- A disconnected connection stays `disconnected` when probed.
- Other modules write status only through `connections::mark`.

Probe checks per provider: Odoo `purchase_request.list`, `purchase_request.read`, `source_controls.read`; NetSuite `auth.token`, `purchase_order.find_by_external_id`, `purchase_order.create`, `purchase_order.read`. Until the provider adapters land, every live check fails with `This provider is not connected yet.`

`INTEGRATIONS_PROVIDERS=fake` (debug builds only; a release build refuses to start with it) answers every probe from an in-process fake that tests can script.

## Connection JSON

```json
{ "id": "uuid", "provider": "odoo", "label": "Odoo main", "status": "active", "status_reason": null,
  "config": { "base_url": "https://acme.odoo.com", "database": null }, "secrets": { "api_key": "********" },
  "credential_expires_at": "2027-02-01T00:00:00Z",
  "last_probe": [{ "key": "purchase_request.read", "ok": true, "error_class": null, "detail": null }],
  "last_probe_at": "2026-11-17T10:00:00Z", "created_at": "…", "updated_at": "…" }
```

## Connection routes

| Route | Permission | Body | Success | Errors |
|---|---|---|---|---|
| `GET /connections` | view | — | list | 403 |
| `POST /connections` | manage | `{provider, label, config, secrets?, credential_expires_at?}` | 201 `{"data":{"connection_id"}}` | 422 fields; label taken (`The label has already been taken.`, any case, per provider); inward address; no credential key; mask on create |
| `GET /connections/{connection}` | view | — | `{"data":Connection}` | 404 |
| `POST /connections/{connection}` | manage | patch: `label`, `config` (merged into the stored config), `secrets`, `credential_expires_at` (null clears). `provider` cannot change | `{"message":"ok"}` | 404, 422 |
| `POST /connections/{connection}/test` | manage | — | `{"data":Connection}` with the new probe | 404 |
| `POST /connections/{connection}/disconnect` | manage | — | `{"message":"ok"}`; status `disconnected`, stored secret dropped | 404 |

A disconnected connection comes back as `draft` once a patch sends a new secret.

## Pairs

Pair JSON: `id, workflow_key, source_connection_id, target_connection_id, status (draft|active|paused), paused_reason, config, activated_at, last_polled_at, created_at, updated_at`.

`config` defaults to:

```json
{"poll_minutes": 5, "backfill_days": 0, "full_scan_minutes": 60, "succeeded_watch_days": 30, "tax_tolerance_minor": 0, "default_locations": {}}
```

| Key | Allowed values |
|---|---|
| `poll_minutes` | 1..1440 |
| `backfill_days` | 0..365 |
| `full_scan_minutes` | 15..1440 |
| `succeeded_watch_days` | 1..365 |
| `tax_tolerance_minor` | 0 or more |
| `default_locations` | object mapping a subsidiary id to a location id (each text, 1..100) |

Unknown keys are refused.

| Route | Permission | Body | Success | Errors |
|---|---|---|---|---|
| `GET /pairs` | view | — | list | 403 |
| `POST /pairs` | manage | `{source_connection_id, target_connection_id, workflow_key?, config?}` | 201 `{"data":{"pair_id"}}` | 422 when the source isn't an Odoo connection of the workspace (`The selected source connection id is invalid.`), the target isn't a NetSuite one, or the pair already exists |
| `GET /pairs/{pair}` | view | — | `{"data":Pair}` | 404 |
| `POST /pairs/{pair}` | manage | `{config}` (merged); connections cannot change | `{"message":"ok"}` | 404, 422 |
| `POST /pairs/{pair}/activate` | manage | — | `{"data":Pair}`; `activated_at` is set on the first activation | 422 `{"message","code":"connection_not_active"}` unless both connections are `active`; 422 `{"message","code":"source_controls_unmet"}` when no company is mapped or a mapped Odoo company is not two-step with amount 0 (`workflows.md` "WF-01 intake") |
| `POST /pairs/{pair}/pause` | manage | — | `{"data":Pair}`, `paused_reason` `manual` | 404 |

The WF-01 activation check (`source_controls_unmet`) is added by the workflow module.

## Mappings

Mapping JSON: `id, pair_id, entity, source_id, target_id, source_label, target_label, updated_at`. The entities are `company`, `vendor`, `product`, `uom`, `currency`, `user` and `location`.

| Route | Permission | Body | Success | Errors |
|---|---|---|---|---|
| `GET /pairs/{pair}/mappings?entity=&page=&page_size=` | view | — | list, ordered by entity then source id | 404; 422 `The selected entity is invalid.` |
| `POST /pairs/{pair}/mappings` | manage | `{entity, source_id, target_id, source_label?, target_label?}`, ids 1..100 chars, labels up to 200 | 201 `{"data":{"mapping_id"}}`. A second post for the same `(entity, source_id)` replaces the target and labels and keeps the id | 404; 422 fields; 422 `The selected target id is invalid.` when a `user` target isn't an active member's user id |
| `DELETE /pairs/{pair}/mappings/{mapping}` | manage | — | `{"message":"ok"}` | 404 |

## Service API for other modules

- `connections::load`, `connections::mark`
- `connections::secrets::open(db, key, ws, connection_id, name)`
- `mappings::resolve(db, ws, pair, entity, &[source_id])`
- `pairs::load_active`, `pairs::set_status`
- `cursors::get` and `cursors::advance` (inside the page's transaction)
- `links::upsert`
- `probe::run`, `probe::status`, `probe::expiry`

## Odoo adapter: verified against documented contract, not live

`src/integrations/providers/odoo/` (C3a). Built only from `docs/integrations/facts/odoo-19.md`. Tests run against an in-process fake whose replies cite that file. No Odoo 19 instance has been called; live gate L1 is still open.

A live `odoo` connection row gets `SourceClient::Odoo`. The client reads `base_url` and `database` from the config and opens the `api_key` secret on its first call. In fake mode (`INTEGRATIONS_PROVIDERS=fake`) the in-process fake still answers.

**Transport.** Every call is `POST {base_url}/json/2/<model>/<method>` with `Authorization: bearer <key>`, `Content-Type: application/json; charset=utf-8`, `User-Agent: verzo`, and `X-Odoo-Database` when `database` is set. The body is always a JSON object of named arguments, because Odoo drops an array body without an error (facts §0a). Each call has a 30 s limit. Amounts are parsed from the JSON number text into exact decimals and never pass through a float.

| Operation | Call |
|---|---|
| `list_changed(companies, cursor, limit)` | `purchase.order/search_read`. Domain: company in the list, `state` in `draft`/`sent`, then `write_date > ts` or (`write_date = ts` and `id > last_id`). Order `write_date asc, id asc`. `ts` is sent as `YYYY-MM-DD HH:MM:SS` UTC. A full page returns `next` = the last row's `(write_date, id)`; a short page returns `next = None`. The 120 s overlap belongs to the caller (PLAN §6a). |
| `list_eligible_ids(company, after_id, limit)` | `purchase.order/search` with company, eligible state and `id > after_id`, order `id asc` |
| `read(ids)` | `purchase.order/search_read` with `id in ids`. Ids that don't come back (deleted, or hidden by record rules) are returned as gone. Then one `purchase.order.line/search_read` and one `res.currency/search_read` (`active_test: false`), so a batch costs 3 calls. Lines with a `display_type` (sections, notes) are dropped. `currency_ref` = currency `name`, `currency_decimals` = `decimal_places` (O2). |
| `read_controls(companies)` | `res.company/search_read` for `po_double_validation` and `po_double_validation_amount` (O5): `two_step`, plus `threshold_zero` when the amount is 0. Companies that don't come back are left out. |
| probe | Runs each read above once with `limit 1` and the real field lists. Checks: `purchase_request.list`, `purchase_request.read` (order, line and currency fields), `source_controls.read`. |

**Error classes** (facts L28, L33-39, L84-87). Every C3a call is a read and safe to retry.

| Reply | Class |
|---|---|
| Not sent or not answered (connect, DNS, address guard, timeout) | `transient` |
| 401 (`Invalid apikey`) | `auth` |
| 403 (`AccessError`, `AccessDenied`) | `permission` |
| 404 (unknown model or method) | `not_found` |
| 409 (`LockError`) | `conflict` |
| 400 (bad JSON), 422 (`UserError`, bad argument, bad `order`) | `validation` |
| 500 with `name: builtins.ValueError` (unknown field or malformed domain, L86-87) | `validation` |
| 429, other 5xx | `transient` |
| 2xx that is not the documented shape | `unknown` |
| Other status | `unknown` |

`code` holds Odoo's error `name` and `http_status` the status. The detail holds a readable sentence plus up to 200 characters of Odoo's `message`. The `debug` traceback and the key are never copied.

**Not done here.**
- Money checks P1/P2 (O6) belong to C4a/C4b.
- `picking_type_id` (O4) is never requested, so `location_id` is always empty and WF-01 takes the location from the pair's default-location config. Later improvement: detect `purchase_stock` (for example with `fields_get`, whose JSON-2 signature is UNVERIFIED) and read the field when it exists.
- Field names `product_uom_id` and `tax_ids` follow PLAN §1b. The facts pack does not list them, so the probe is the first check against a real instance.
- UNVERIFIED items the parser guards against: many2one comes back as `[id, name]` or as a bare id, and both are accepted (item 4). Sub-second `write_date` only re-delivers rows, it loses none (item 3).
## NetSuite adapter (C3b)

Code: `src/integrations/providers/netsuite/` (`auth`, `errors`, `sink`, `probe`). `SinkClient::NetSuite` is built for a live `netsuite` connection; `SinkClient` implements `PurchaseOrderSink` and `ConnectionProbe` by delegation (the fake variant has no purchase orders and refuses them).

Everything here is **verified against the documented contract** (`docs/integrations/facts/netsuite-rest.md`, cited as `l.N`) with an in-process fake NetSuite (`tests/api/netsuite.rs`). No NetSuite account was used, so this is not an integration claim.

### Auth (OAuth 2.0 client credentials, M2M)

- Base URL `https://<rest_domain>`. Token: `POST /services/rest/auth/oauth2/v1/token`, form `grant_type=client_credentials`, `client_assertion_type=urn:ietf:params:oauth:client-assertion-type:jwt-bearer`, `client_assertion=<JWT>` (l.44-54).
- JWT header `{typ: "JWT", alg: "ES256", kid: certificate_id}`; claims `iss` = client id, `scope` = `"rest_webservices"`, `aud` = token URL, `iat`, `exp` = `iat` + 30 min (under the 60-minute cap, l.56-68). Signed with `ring` `ECDSA_P256_SHA256_FIXED_SIGNING` (raw R‖S, RFC 7518 §3.4). Tests: RFC 7515 A.3 known vector and a field check.
- Key: the connection's sealed `private_key`, opened only when a token is fetched. It must be an unencrypted PKCS#8 PEM (`BEGIN PRIVATE KEY`) on the named P-256 curve. A SEC1 key (`BEGIN EC PRIVATE KEY`) is refused with the fix: `openssl pkcs8 -topk8 -nocrypt`.
- Cache: in memory, per token URL + client id + certificate id, until `expires_in` − 60 s. Tokens and keys are never logged or stored.
- Token endpoint `invalid_grant`: one more try with a fresh assertion, then `Auth`. Other 4xx → `Auth`; 429/5xx/transport → `Transient`.
- REST 401 on a read (find, read, probe): drop the token, fetch a new one, retry once, then `Auth`. **Create never retries**: a 401 is `Rejected(Auth)`.

### Calls

| Call | Request | Result |
|---|---|---|
| `find_by_external_id(eid)` | `GET /services/rest/record/v1/purchaseOrder?q=externalId IS "<eid>"` (l.195) | 0 hits or 404 → `None`; 1 → `read(id)`; more → `Conflict`. eid must match `[A-Za-z0-9_-]` (l.194), else `Validation` and nothing is sent. PARTIAL: the miss shape is UNVERIFIED, so `None` is never proof of absence |
| `create(po)` | one `POST /services/rest/record/v1/purchaseOrder`, header `X-NetSuite-PropertyNameValidation: error` (l.282), body `externalId`, `entity`, `subsidiary`, `currency`, `location?`, `item.items[]{item, quantity, rate, units?}` (l.105, l.133-168). Never PUT, never PATCH, never async | see outcomes below |
| `read(id)` | `GET /services/rest/record/v1/purchaseOrder/<digits>?expandSubResources=true` (l.116) | `TargetPo`: `entity`→vendor, `subsidiary`, `currency`, `externalId`, lines `item.id`/`units`/`quantity`/`rate`, `subtotal`→untaxed, `taxTotal`→tax (l.146-147). Numbers are exact decimals. A missing reference or line value → `Unknown` |
| probe | token fetch, then `GET /services/rest/record/v1/purchaseOrder?limit=1` (l.119) | `auth.token` from the token; find, read and create from the one read. Create is never tested (a probe never writes); its check says so (LIVE L5) |

### Create outcomes (for C4b)

`Rejected` means NetSuite did not create the PO. `Ambiguous` means it may have: reconcile by externalId.

| What happened | Outcome | Class |
|---|---|---|
| 204 + `Location: …/record/v1/purchaseOrder/<digits>` (l.125) | `Created{id, location}` | — |
| 204 without a readable purchase order Location; 202; other 2xx/3xx | `Ambiguous` | Ambiguous |
| connect/DNS failure (nothing sent) | `Rejected` | Transient |
| host not public (address guard) | `Rejected` | Validation |
| timeout, reset or cut body after the request went out | `Ambiguous` | Ambiguous |
| 5xx (`UNEXPECTED_ERROR`, l.278) | `Ambiguous` | Ambiguous |
| 429 `CONCURRENCY_LIMIT_EXCEEDED` (l.256) | `Rejected` | Transient |
| 409 | `Conflict` | Conflict |
| other 4xx | `Rejected` | by status (below) |

### Error classes by status (l.271-278)

Keyed on HTTP status; `code` = first `o:errorDetails[].o:errorCode`, detail = its `detail` plus `o:errorPath` when given.

| Status | Example code | Class |
|---|---|---|
| 400 | `INVALID_CONTENT`, `INVALID_REQUEST` | Validation |
| 401 | `INVALID_LOGIN` | Auth |
| 403 | none documented for REST (N6) | Permission |
| 404 | `NONEXISTENT_ID` | NotFound |
| 409 | — | Conflict |
| 429 | `CONCURRENCY_LIMIT_EXCEEDED` | Transient |
| 5xx | `UNEXPECTED_ERROR` | Ambiguous on create, Transient on reads |

### UNVERIFIED (from the facts pack)

- Item sublist writability over REST (l.104-107, L3): the create body follows the documented sample.
- `units` on a line: id or name (RAB types it as a string, l.168).
- Required body/line fields, location and subsidiary needs (l.183, L3).
- `q=` miss shape and read-after-write consistency (l.202-206, L2).
- POST with an existing externalId (N3, L2): a 409 is treated as `Conflict`.
- `scope` as text vs array, `aud` host, `jti` (l.70-74, L5); raw R‖S vs DER signature (l.82).
- Token endpoint error bodies other than `invalid_grant` (l.93).
- REST permission error status and code (N6, L5): `Permission` only from 403.
- Approval routing of REST-created POs (N4, L4): Flow sets no approval fields.
- Sandbox host form for `<id>_SB1` (l.27).

### LIVE gates (need a NetSuite sandbox)

- **L2** create with a duplicate externalId; `q=` lookup misses and consistency.
- **L3** required purchaseOrder fields and item sublist writes under real account features.
- **L4** approval routing on REST create.
- **L5** M2M token (JWT form, signature encoding) and least-privilege role permissions.
- **L6** currency, vendor and subsidiary restrictions.

## WF-01 execute and reconcile (C4b)

Verified against the documented contract with an in-process fake Odoo and fake NetSuite. Not tested against a live account (LIVE gates L2–L6).

### `wf01_execute` (payload `{"execution_id"}`, dedupe on the execution id, 5 attempts)

1. **Own the run.** Claim a `queued` row. Otherwise take over a `running` row whose owner job has no live lease (P-2). Otherwise do nothing. The owner token is the job's `locked_by`, and every later state write is fenced on it.
2. **W1.** If the phase is past `prechecking`, or the newest attempt has no outcome, the run never POSTs. The attempt becomes `ambiguous`, the run goes to `awaiting_reconciliation`, and a `wf01_reconcile` job is queued in the same transaction.
3. **Pre-checks.** Any failure moves the run to `blocked`, with no write intent:
   - the pair must be active (`pair_paused`);
   - each connection must be usable, with its expiry applied (`connection_unauthorized`, `connection_expired`, `connection_inactive`);
   - the RFQ is read again. If it is gone, no longer draft/sent, or its fingerprint changed, the run goes to `cancelled` (`source_changed`), the approval to `invalidated`, and the next revision is created in the same transaction;
   - the C4a money check runs again (`rate_precision`, `untotalled_lines`);
   - the snapshot quantity and price must equal the exact source values (`rate_precision`);
   - snapshot mappings and the default location must be unchanged (`mapping_changed`);
   - the approver must still be an active member with the permission (`approver_revoked`).
   An Auth or Permission error from a provider also marks the connection (`unauthorized` / `degraded`). Any other provider error queues the run again with job backoff. On the last job attempt the run becomes `failed` (`attempts_exhausted`).
4. **Existence guard.** The run looks up the externalId (the execution's `correlation_id`). If a PO is found, it is verified (step 7) and nothing is POSTed. The guard is cheap; correctness does not depend on it.
5. **15-minute window.** An attempt after the first (granted by `allow_retry`) waits until the previous write intent is 15 minutes old, because NetSuite requests can run that long (facts l.244). Until then the run goes back to `queued` with no write intent. The tick backstop runs it again within a minute.
6. **Write intent, then one POST.** The attempt row and the `write_intent` event commit first. Then phase `write_sent` and `sent_at` are set, and exactly one POST is sent. The fenced outcome follows. If ownership was lost, the worker only appends `stale_worker_result` (attempt number, HTTP status, NetSuite id, Location).

| Create result | Attempt outcome | Run |
|---|---|---|
| 204 with the PO's Location | `created` | read back, then verify |
| timeout or 5xx after send, unreadable Location | `ambiguous` | `awaiting_reconciliation` |
| 409 | `conflict` | `awaiting_reconciliation` |
| 429 "Request blocked." (facts l.256) | `rejected`, evidence `{http_status, error_code}` | `awaiting_reconciliation`, error class `transient`; a reconciler grants the next attempt |
| connect failure, nothing sent | `not_sent` | same as 429 |
| 401 | `rejected` | `blocked` `connection_unauthorized`, connection `unauthorized` |
| 403 | `rejected` | `blocked` `permission_denied`, connection `degraded` |
| 400 | `rejected` | `blocked` `validation_rejected`, with NetSuite's detail |
| 404 | `rejected` | `blocked` `mapping_stale` |

7. **Verify (R3-§2b).**
   - **V1:** vendor, subsidiary, currency, and lines as (item, quantity, rate), compared as exact decimals. The unit is not compared, because whether `units` holds an id or a name is UNVERIFIED (facts l.168).
   - **V2 and V3** run in SQL `numeric`: `subtotal` × 10^decimals must equal the approved untaxed minor amount, and |`taxTotal` × 10^decimals − Odoo tax minor| must be ≤ the pair's `tax_tolerance_minor` (default 0).
   - **Outcomes:**
     - Match → `succeeded`, with a `verify_match` event and `external_links` set to `linked`.
     - Tax off or missing → `succeeded_with_variance` plus a `tax_variance` incident.
     - V1 or V2 off → `awaiting_reconciliation`, a `verify_mismatch` event, a `target_conflict` incident, and the link set to `conflict`.
   - Never PATCH or PUT.

### `wf01_reconcile` (dedupe `wf01_reconcile:<execution>`, 10 attempts)

Acts only on `awaiting_reconciliation` and never POSTs.
- **Evidence order:** NetSuite ids from `created` attempts, then from `stale_worker_result` events, then the externalId lookup. A PO that is found is read back and verified as in step 7.
- **Not found** never counts as proof that the PO is absent (facts l.202-206). The run records a `reconcile_check` event and checks again with job backoff. After the 10th check it stays open for a person: `link_existing`, `allow_retry` or `close_no_po`.

### `link_existing` (resolve route)

Reads the given NetSuite id inside the resolve transaction and verifies it as in step 7.
- **Match** → `succeeded` (or `succeeded_with_variance`).
- **Differences** → `resolved_manual` / `linked_with_differences` with `target_ref` set. It is never shown as verified.
- **Errors:** an id NetSuite can't read → 422 on `target_id`; NetSuite unreachable → 503.

### Jobs

- **Approve, `retry` and `allow_retry`** queue `wf01_execute` in their own transaction, through `dispatch::enqueue_execution`.
- **`wf01_tick`** also re-queues WF-01 runs that are `queued`, or `running` with a dead owner lease (500 per tick). It never re-queues `awaiting_reconciliation`.
