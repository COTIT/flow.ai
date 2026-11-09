# Assistant controls

Assistant controls are internal Office bridge routes. They never enter model discovery. Ordinary business routes keep their existing bearer authentication, permissions, validation and response bodies.

All workspace controls require an active workspace member and the same user and login that created the session. Another workspace, user or login gets 404. A revoked login gets 401; inactive membership gets 403.

## Sessions

Under `/v1/workspace/{workspace}/assistant/sessions`:

- `POST` with `{language: "en" | "nl" | "sw"}` returns 201 `{data: session}`.
- `GET /{session}` returns 200 `{data: session}`.
- `GET /{session}?with=identity` returns 200 `{data: session + identity}`, where `identity` is `{workspaces: [uuid], permissions: [string], farm_roles: [string]}`. `workspaces` lists the same ids as `GET /v1/me/workspaces`, in the same order. `permissions` matches this workspace's entry in `GET /v1/me` (sorted grants, then `workspace_owner`). `farm_roles` holds every live farm role in this workspace that `GET /v1/farm/me` lists, ordered `admin`, `ops`, `field_officer`, `farmer`, or `[]` when there are none. Token, ownership and membership rules match the plain `GET` (401, 403 and 404 as above). Any other `with` value returns 422 `errors.with: ["The selected with is invalid."]`. Without `with`, the body is unchanged. The request costs three database statements however many workspaces, teams or roles the caller has: token, membership and one read for session plus identity. Nothing is cached, so a revoked token or removed member is refused on the next call.
- `DELETE /{session}` closes it and returns its current resource.
- `POST /{session}/heartbeat` refreshes `last_seen_at`. Send every 30 seconds. It cannot revive an ended session.
- `POST /{session}/attach` with `{conversation_id}` requires bridge signature. Same reference repeats safely; replacement returns 409.

Session resource: `id`, `task_id` (same server-generated UUID), `language`, `state`, `conversation_id`, `expires_at`, `last_seen_at`. Hard lifetime: one hour. Fifteen minutes without heartbeat ends usability. Provider identifiers allow only ASCII letters, numbers, `_` and `-`, at most 200 bytes. An attached conversation belongs to one session.

## Exact action approval

Under `/v1/workspace/{workspace}/assistant/actions`:

- `POST` creates a proposed action: `{session_id, operation_id, method, path, query?, body?}`. Returns 201 `{data: action}`. `path` may include `/v1`; query is separate, without `?`.
- `POST /{action}/approve` with `{session_id, approval_event_sha256?}` requires bridge signature. Office must verify trusted UI confirmation or an exact explicit instruction before signing. Model output cannot supply approval proof. Spoken confirmation supplies a lowercase SHA-256 of the session and final provider user-event id. One event approves only one review scope per session, enforced by session locking and partial unique indexes across replicas. Same-action/event retries report the existing state without reviving cancelled, expired or dispatched work. Click confirmation omits this hash.
- `POST /{action}/cancel` with `{session_id}` cancels only before dispatch.
- `GET /{action}` returns owned durable status and bounded evidence.
- `GET /sessions/{session}/actions?before={step_index}` under the sessions prefix restores newest-first history, at most 100 rows. Use oldest returned step index for the next page.

Action resource: `id`, `session_id`, `operation_id`, `step_index`, `state`, `expires_at`, `request_hash`, `proposal`, `http_status`, `evidence`. History omits `request_hash`. `proposal` is returned only while proposed/approved and unexpired. Review lasts 120 seconds; approval does not extend it.

Business draft JSON is limited to 32768 bytes. Oversize returns 422 `The business draft must not exceed 32768 bytes.` Credential fields, including exact `pin` keys at any depth, and binary/base64 payloads require secure forms or multipart upload. Assistant controls, app-key issuance, credentials, password changes and logout require secure UI flows. Secret-free app-key revocation supports exact action approval and retains native workspace-admin authorization.

Dispatch uses the original business endpoint and bearer with `Assistant-Action-Id` and `Assistant-Session-Id`. JSON object key order is normalized before hashing; method, concrete path, exact query and all JSON arguments are bound. Changed arguments, expiry, cancellation or consumed approval returns 409. Two concurrent copies admit at most one handler call.

GET effects `invoices_preview`, `pdf_templates_preview` and `messages_profile_test` create files or send mail, so require the same action claim. Ordinary reads without action headers keep their current behavior.

States: `proposed`, `approved`, `dispatching`, `completed`, `queued`, `partial`, `unknown`, `cancelled`, `expired`, `failed`. The claim commits before the existing handler runs. The handler runs inside its existing timeout. Non-2xx outcomes remain unknown unless a checked material-facts conflict proves refusal before mutation. Consumed claims are never replayed, including after restart. Existing optional `Idempotency-Key` behavior stays available.

Material order prices, existing order state and absolute stock before-values are private immutable review facts. Invoice mutations also bind current amounts, paid balance, currency decimals, every line and the selected contact. A private canonical SHA-256 binds the full material snapshot. Public review shows exact totals, line count, five preview lines and the explicit omitted count, including for all 1000 native invoice lines; the digest remains private. Existing domain locks protect the execution comparison. Changed facts return 409 before that transaction commits. Old critical proposals without facts require a new review. Public review facts require the native read permission (`order.view`, `stock.list`, `invoice.view`, or `relation.view` for recipient facts); write-only callers retain execution capability and receive `{factsHiddenByPermission:true}`. Order-to-invoice conversion also binds resolved invoice billing and line defaults; its public facts require both `order.view` and `invoice.view`, while native write authorization stays `order.update`. Private snapshots clear on dispatch completion, cancellation or expiry.

Product creation, update and duplication also bind resolved currency/VAT metadata and native normalized prices. Variant creation, update and generation bind inherited product money and the exact resulting variant prices. Product values remain decimal text, including up to eight decimal places; they are not converted to invoice/order minor units. Native currency/VAT locks protect the comparison, and public facts require `product.view`. Generated sets retain a private full-material digest with count, five preview variants and an explicit omitted count. Invoice creation reuses native currency, selected contact, billing address, line defaults and pricing; its bounded summary uses the same private-digest rule as existing invoices.

Farm and supply writes get display-only review facts (`assistant::farm_facts`). They name things for the approval card and bind nothing at execution, so the native handler still checks every rule. The caller must hold the operation's role, otherwise the proposal returns 403 before any name is read. An unknown village, crop, currency, harvest, link, warehouse or partner returns 422 on that field.
- `farm_supply_create` (ops/admin): `{kind:"farm_supply", locked:{label:"<village> · <crop>"}, priced:{lines:[{farmer, kg}], kg_total, price_per_kg_minor, currency:{code, decimals}}}`. It shows no money total, because buying credits nobody: farmers are credited at sale.
- `farm_records_harvest_reports_revise` (ops/admin, or an officer of the cycle's village): `{kind:"harvest", locked:{label:"<farmer> · <crop>", figure}, changes:[{field:"quantity_kg", before, after}], note:"The previous figure stays on record."}`. `before` is null when the cycle has no current figure of that kind.
- `supply_chain_dispatch` (`product.update`, or farm ops/admin): `{kind:"dispatch", locked:{label:"<lot code>", product, partner, warehouse}, changes:[{field:"quantity", after}]}`.
- `supply_links_grant` (`workspace_admin`): `{kind:"supply_link", locked:{label:"<partner display name>"}, fields, note:"Farmer names and photos stay off."}`. The caller's own workspace returns 422.

Public review shows these kinds to the proposing owner without a workspace read permission, because farm roles are not workspace permissions.

Audit evidence stores bounded typed receipts and HTTP meaning codes. Mail-server acceptance is `queued` with a resolved `finished_at`; it never proves inbox delivery. Blocked/no-recipient mail is `failed`, manual sending `partial`, ambiguous transport `unknown`. Invoice delivery waiting for its first attempt is `queued` with no `finished_at`. Its number already committed: manual/blocked/pre-transport failure is partial; accepted is resolved queued; ambiguous transport is unknown. Team permission changes retain only typed applied/partial status and counts. Partial changes are resolved partial outcomes and stop dependent batch steps. Variant generation reports applied/partial counts; skipped combinations are partial and stop the batch. Refund admission is queued until the native payment row verifies settlement. Its opaque refund id commits with the business row and action; the minute sweep maps verified refunded to completed and failed/canceled to failed, without provider calls or replay. A late HTTP finalizer cannot downgrade terminal proof, and settled children never restart paused batches. Refund audit keeps only id/status, without ambiguous raw amounts. Raw response bodies, provider errors, tokens, chat and file bytes are never copied into audit.

## Review facts for risky writes

`review_facts.facts` keeps `priced` / `locked` as the stages compared at execution. Extra keys are display facts.

- `products_update`, `product_variants_update`: `before` is the current quote (same shape as `priced`; variants nest under `variant`). `changes` lists only fields that differ: `{field, before, after}`, or for money `{field, currency: {code, decimals}, before_minor, after_minor}` when both values are exact at the currency's decimals, else exact decimal strings `before` / `after`. A missing amount is `null`. `change_bp` (signed basis points, truncated) appears when the old amount is above zero. `warning: "large_change"` appears for `price`, `gross_price` or `wholesale_price` when `change_bp >= 90000` (ten times) or `<= -9000` (90% cut). Warnings never block.
- `relations_delete`, `products_delete`: the proposal runs the native delete guard first. A refused delete returns the native `422` (for example `errors.relation_id[0]` "This relation cannot be deleted because it has invoices, orders, estimates, or work orders.") and stores no action. Allowed: `locked: {id, label}` (relation label: company name, else alias, else first contact's full name) and `impact: {effect: "archive", orders, contacts}` or `{effect: "archive", variants, stock_rows}`. Delete is a soft delete; `archive` means hidden, history kept. At execution the impact is recomputed under the row lock; any difference returns `409` and nothing is deleted.
- Permission (`relation.delete`, `product.delete`, `product.update`) is checked before any read. Another workspace's id returns `404` with no facts.

## Fixed ordered batches

Batch protocol is `1`; Office graph workflow is `2`. Batches reuse the action ledger and original business handlers. Each step is fully resolved; later steps cannot reference earlier results. Changed material facts, including changes made by earlier steps, require a fresh review.

Under `/v1/workspace/{workspace}/assistant`:

- Signed `POST /batches`: `{session_id,actions:[{operation_id,method,path,query?,body?,body_sha256?,content_type?},...]}`. Supply the session once. Two through ten steps; combined business drafts at most 65536 bytes, private drafts plus facts at most 131072 bytes. Individual limits still apply. Creation and child bindings commit together.
- `GET /batches/{batch}`: owned metadata and permission-projected children. `GET /sessions/{session}/batches`: at most fifty newest summaries.
- Signed `POST /batches/{batch}/approve`: `{session_id,approval_event_sha256?}` after one independent confirmation of the immutable sequence. One final user event cannot approve both a single action and a batch; the shared session lock serializes proof checks across replicas.
- Signed `POST /batches/{batch}/start`: `{session_id}`, returning `{data:{batch,execution_token}}`. One controller starts once. Its random proof returns once, persists only as SHA-256, and stays in the active BFF request.
- Signed `POST /batches/{batch}/advance`: `{session_id,execution_token,action_id}` after the BFF validates the current native response contract and completed outcome. Raw HTTP 2xx alone cannot admit the next child. Verified repeat acknowledgement is idempotent.
- Owner `POST /batches/{batch}/pause` or `/cancel`: `{session_id,reason}`. Pause prevents future claims and erases unclaimed drafts; cancel also marks those children cancelled. Admitted work may finish. Neither operation restarts execution or undoes effects.

Resource: `id`, `session_id`, `protocol_version`, `fingerprint`, `state`, `reason`, `next_index`, `action_count`, `expires_at`, `started_at`, `recoverable`, `steps`. The fingerprint binds actor, login, workspace, session, expiry and ordered request/material hashes. States: proposed, approved, running, paused, completed, cancelled, expired. `recoverable` allows restoring only an unexpired never-started JSON review. Multipart requires original browser bytes. Interrupted execution never resumes automatically.

Business dispatch adds private `Assistant-Batch-Id` and `Assistant-Batch-Token` alongside action/session headers. Under the batch row lock, the guard checks owner, live login/session, controller proof, 120-second review expiry, 45-second controller lease, request hash and current index. The child claim commits before its original handler. Each next step rechecks live authorization and material facts. Failure, unknown, partial or queued stops the sequence. Disconnect, end, logout or workspace change stops future dispatch; admitted work retains its existing timeout. Lease renewal never extends review expiry. The minute sweep marks up to 100 expired or lease-abandoned batches stopped and purges at most 1000 unclaimed child drafts/facts, without touching admitted work.

Unverified raw HTTP completion appears unknown until trusted acknowledgement. The minute sweep can resolve an unknown child to queued, partial or failed only from a terminal typed mail/invoice receipt; it never advances the batch or treats generic HTTP success as proof. A crash after business commit but before ledger finalization can leave an unknown consumed action whose business effect exists. Never replay it. Verify through native records or operation-specific durable receipts. Invoice intent and send-log correlation provide evidence where available; generic handlers do not promise atomic completion or exactly-once external transport.

Children use `batch_pending`, which older guards cannot approve or dispatch. Rollback compatibility must require batch support even when schema rollback is skipped. Resolved child audit and empty terminal batch metadata expire after ninety days; unresolved children retain batch/session/checkpoint context.

Models reach only `assistant_batch_prepare` and `assistant_batch_status`; approval/start/advance stay private. Ticket, code and phrase never enter model tools/context proactively. Native ASR necessarily hears the user's own spoken approval; replayed events and later model echoes cannot authorize another dispatch.

## Native multipart business uploads

Raw multipart proposals add `body_sha256` (lowercase 64-character SHA-256) and `content_type` (the exact multipart header including boundary). They require bridge signature. Optional `body` contains bounded business review metadata, never file bytes. At dispatch the guard hashes exact incoming bytes and content type. Rebuilding multipart requires the same boundary and bytes. Cap: 10 MiB plus 64 KiB framing, at most four raw guarded requests concurrently. Existing endpoint-specific MIME, file size and business rules still run.

## Conversation attachments

Under `/sessions/{session}/uploads`:

- `POST` reserves `{media_type, size_bytes}` before the bridge reads the full file. Returns 201 `{data: {id,state,media_type,size_bytes}}`.
- `GET` restores at most ten owned upload records.
- `GET /{upload}` reads one owned record.
- `POST /{upload}/complete` requires bridge signature and `{file_id}`. Optional `size_bytes`, `media_type`, `sha256` must appear together and record actual verified bytes.
- `DELETE /{upload}` cancels reservation or marks remote reference for cleanup.

Allowed types: JPEG, PNG, WebP and PDF. Size: 1 byte through 10 MiB. Ten reservations total per session, including cancelled ones; at most two unexpired reservations concurrently. Reservations expire after 60 seconds. Quota allocation locks the session row. Upload records hold provider references and actual metadata, with no names or content. Completion after session close remains cleanup-only. Completion after confirmed cleanup returns 409; bridge must attempt provider deletion when completion cannot be recorded.

## Bridge signature

Server-only `ASSISTANT_BRIDGE_SECRET`, at least 32 bytes, also accepts `_FILE`. Missing config returns 503. Invalid/missing/expired signature returns 403. Never expose the key in browser state or tool arguments.

Headers: `Assistant-Bridge-Timestamp` (decimal Unix seconds) and `Assistant-Bridge-Signature` (hex HMAC-SHA256). Signature input is UTF-8:

```text
{timestamp}\n{METHOD}\n{originalFullPathAndQuery}\n{lowercaseSHA256OfRawBodyBytes}
```

Path includes `/v1`. Timestamp tolerance: 60 seconds. Bodyless controls hash empty bytes. Signature comparison is constant-time.

## Cleanup and retention

Scheduled `assistant_cleanup` runs every minute through the existing Rust job rail. It deletes only whole due conversations at fixed `https://api.elevenlabs.io/v1/convai/conversations/{id}` using optional server-only `ELEVENLABS_API_KEY` (`_FILE` supported). Due means closed, expired, orphaned or login revoked. Fresh in-flight reservations delay cleanup for up to 60 seconds. A chunk processes at most 20 sessions with ten-second provider request timeouts. Only provider 200, 204 or 404 confirms deletion. Other statuses/network failures retain references for later sweeps. Provider acknowledgement does not prove immediate physical removal from every provider backup.

Signed, bodyless operator endpoints `GET /v1/assistant/cleanup` and `POST /v1/assistant/cleanup/{session}/ack` offer bounded inspection/ack independent of user login. Ack returns 409 while the session is not due or has a fresh upload reservation; confirmed-cleaned retries return 200. Acknowledge only after confirmed provider deletion.

Pending business drafts live temporarily in PostgreSQL JSONB and can exist in database backups. They disappear on dispatch/cancel, become unreadable at expiry, and the minute sweep removes expired payloads. No raw chat or files belong in these drafts. Resolved action audit is deleted after 90 days. Unresolved claims stay until reconciliation establishes an outcome. Cleanup references stay until provider deletion is confirmed.

After resolved action audit expires, hourly pruning deletes at most 100 cleaned sessions older than 90 days, only with no remaining actions or remote references. Checkpoint writes, blobs, checkpoints, upload metadata and session metadata leave in one transaction. Pending queued or unknown actions retain their context. Increase batch size or cadence before retirement exceeds 2400 sessions/day.

## Gap ledger (developers only)

Office records each voice request the assistant could not finish, so developers can see what to build next. Migration `assistant_gaps`. There is no read route: staff run `scripts/voice-gaps.sql` with `psql` on the server.

`POST /v1/workspace/{workspace}/assistant/gaps` needs a staff bearer for the workspace plus the bridge signature. Body (unknown fields refused):

| Field | Rule |
|---|---|
| `domain` | `^[a-z_]{1,40}$`, a catalog domain or `unknown` |
| `language` | `en`, `nl` or `sw` |
| `intent` | lowercase words, single spaces, at most 120 chars; no digits, capitals or punctuation. Office keeps only catalog words; `unknown` when fewer than two survive |
| `tools` | at most 10 operation ids the agent tried (optional) |
| `outcome` | `unsupported`, `refused`, `failed`, `cancelled` or `dropped` |
| `http_status` | 400-599 (optional) |

201 `{"data":{"id":"…"}}`. 422 on any rule. 403 unsigned, 503 bridge not configured, 404/403 for a token outside the workspace.

`POST /v1/assistant/gaps/{gap}/follow-up` needs the bridge signature only (Office's page proxy has no staff bearer). Body `{"workspace_id","signal","route"?}`: `rephrased` (no route) or `used_ui` with a page pattern such as `/orders/[id]` (lowercase segments or `[id]`, no query, at most 80 chars). 200 `{"data":{"id","follow_up"}}`. 404 when the gap is not in that workspace. 409 when a follow-up is already set or the gap is older than 10 minutes.

Rows are deleted 90 days after `created_at` by the hourly prune. Deleting a workspace deletes its gaps.
