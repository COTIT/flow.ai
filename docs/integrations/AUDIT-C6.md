# AUDIT-C6: Odoo and NetSuite adapters vs. the docs

- Date: 2026-11-18
- Base: `45cc5b755`
- Mode: read-only. No code was changed.

## Scope

- **Odoo:** `apps/api/src/integrations/providers/odoo/{mod,client,errors,probe,source}.rs`, plus its in-crate fake `odoo/tests.rs`.
  - `apps/api/tests/api/odoo.rs` does not exist.
- **NetSuite:** `apps/api/src/integrations/providers/netsuite/{mod,auth,errors,probe,sink}.rs`, plus the fake in `apps/api/tests/api/netsuite.rs`.
- **Shared:** `apps/api/src/outbound.rs`, read only to confirm that every call goes through it.
- **Also checked:** `providers/fake.rs`.
- **Out of scope:** `workflows/` and `modules/approvals`. `wf01_purchase/fake_netsuite.rs` is not on this base; the parent will send it later.

## Sources opened for this audit

- **Odoo source code**, fetched raw at the pinned commit `61d8ae93ae5d2af016a794f67df3f1bb3df1d94f`:
  - `addons/purchase/models/purchase_order.py`
  - `addons/purchase/models/purchase_order_line.py`
  - `addons/purchase/models/res_company.py`
  - `addons/rpc/controllers/json2.py`
  - `odoo/http.py`
  - `odoo/exceptions.py`
  - `odoo/tools/json.py`
  - `odoo/addons/base/models/res_currency.py`
- **Odoo docs:** https://www.odoo.com/documentation/19.0/developer/reference/external_api.html
- **Oracle NetSuite help pages:** `section_162755359851` (token), `section_162790605110` (JWT), `section_1545141395` (create), `section_156334828635` (external IDs), `section_156570709583` (errors), `section_1545222128` (filtering), `article_161165602348` (purchase order), `subsect_1559222360` (concurrency and timeout).
  - Every page is under https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/
  - The REST API Browser (RAB) was not reopened. The facts pack's RAB statements were taken as given.

## Severity

| Severity | Meaning |
|---|---|
| **blocker** | The code is wrong against the docs. |
| **risk** | The docs are silent or ambiguous, and the code assumes something. |
| **note** | Worth knowing; no defect. |

Items marked UNVERIFIED in the facts pack (N1–N6, the Odoo UNVERIFIED list, L1–L6) are not findings unless the code treats them as settled.

## Summary

- **Blockers:** 0
- **Risks:** 5. Odoo: C6-O1. NetSuite: C6-N1, C6-N2, C6-N3. Fake: C6-F1.
- **Notes:** 4. C6-N4, C6-F2, C6-F3, C6-F4.

---

## Findings

### C6-O1: risk. Down-payment lines with no product make the whole RFQ read fail

- **Where:** `apps/api/src/integrations/providers/odoo/source.rs:292` (the line filter) and `:141-142` (the required fields).
- **What the code does:**
  - It skips a line only when `display_type` is a string.
  - Every other line must have `product_id` and `product_uom_id`, or `unreadable(...)` fails the entire `read()` batch with `ErrorClass::Unknown`.
- **What the source says:** the 19.0 constraint lets an accountable line have no product when `is_downpayment` is set.
  - Source: https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L105-L107
  - Quote: "CHECK(display_type IS NOT NULL OR is_downpayment OR (product_id IS NOT NULL …"
  - `product_id` and `product_uom_id` themselves have no `required=True` (lines 43-44 of the same file).
- **Why it is a risk, not a blocker:** neither the docs nor the facts pack say whether a down-payment line can exist on a `draft` or `sent` RFQ. It is created by `_create_downpayments` (`purchase_order.py#L744`), which the facts pack did not trace.
- **Smallest fix:** add `is_downpayment` to `LINE_FIELDS`. Then either skip lines where `is_downpayment` is true (as `display_type` lines are skipped), or fail with a clear `Validation` detail instead of `Unknown`. The parent picks which; it is a business choice.

### C6-N1: risk. Only HTTP 409 is treated as a duplicate-create conflict

- **Where:** `apps/api/src/integrations/providers/netsuite/sink.rs:203-204`.
- **What the code does:**
  - 409 becomes `CreateOutcome::Conflict`.
  - Every other 4xx becomes `CreateOutcome::Rejected`, which means "not created".
  - A POST whose `externalId` already exists is therefore classed `Conflict` only if NetSuite answers 409.
- **What the docs say:** no REST page states the status for a POST with an existing `externalId` (facts N3, UNVERIFIED item 9).
  - The only REST error-status list documents 400, 401, 404, 429 and 500. It has no 409.
  - Source: https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156570709583.html
  - Quote: "4xx status codes are used for failures due to user error."
- **The conflict:** the facts pack's plan line (netsuite-rest.md L216) says "any conflict-shaped error is classed `Conflict`". The code narrows "conflict-shaped" to the HTTP 409 status. A 400 with a duplicate-ID code would return `Rejected`/`Validation`.
- **Smallest fix:** none until live gate L2. Add an L2 smoke step that POSTs a duplicate `externalId` and records the status and `o:errorCode`. Then map that pair to `Conflict` in `errors::from_status`/`create`. Until then, the caller's existence check (find before create) is the only guard.

### C6-N2: risk. A 401 on the create POST keeps the refused token in the cache

- **Where:** `apps/api/src/integrations/providers/netsuite/sink.rs:204`, compared with `netsuite/mod.rs:186-189`.
- **What the code does:**
  - `get_json` calls `forget_token()` on a 401 before retrying.
  - `create` maps a 401 to `Rejected(Auth)` but does not call `forget_token()`.
  - The token stays cached until about expiry − 60 s (`auth.rs:132`). Every later create in this process reuses the refused token and fails `Auth`, until a read happens to clear it or the cache entry expires.
- **What the docs say:** a 401 means the login token is invalid.
  - Source: https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156570709583.html
  - Quote: "The following error is returned if the request contains an invalid login token."
  - The docs are silent on whether a token can be refused before its documented 60 minutes are up.
- **Smallest fix:** in `create`, call `self.forget_token()` when `reply.status == 401`, before returning `Rejected`. Do not retry the POST.

### C6-N3: risk. A read reply with no `item` sublist is taken as a purchase order with zero lines

- **Where:** `apps/api/src/integrations/providers/netsuite/sink.rs:93-97`.
- **What the code does:** if `item.items` is missing or not an array, `.unwrap_or_default()` gives an empty `lines` list without an error. In the same function, a missing `entity`, `subsidiary` or `currency` is an `Unknown` error.
- **What the docs say:** the purchase-order page reads with `?expandSubResources=true`.
  - Source: https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_161165602348.html
  - Its sample: "GET …/purchaseOrder/{{id}}?expandSubResources=true".
  - The same page lists "items" as "not accessible through REST web services" (facts N1, UNVERIFIED item 1). Whether a GET returns `item` at all is unknown.
- **Smallest fix:** treat a missing or non-array `v["item"]["items"]` as `unknown("Purchase order {id} has no item list.")`, as the code already does for `entity`. A PO that really has zero lines then still shows up as an empty array, not a missing field.

### C6-F1: risk. The NetSuite fake answers a GET by sending back the body that was POSTed

- **Where:**
  - `apps/api/tests/api/netsuite.rs:181-202`: GET by internal ID returns `stored`.
  - `netsuite.rs:84-90` (`store`): `stored` is the POST body plus `"id"`.
- **What the fake does:** every create → find → read round trip reads back exactly the request JSON:
  - `entity`, `subsidiary` and `currency` come back as `{"id": …}`, without `refName` or `links`;
  - `quantity` and `rate` come back with the same digits that were sent;
  - there is no `subtotal`, `taxTotal` or `total`;
  - `item` has no `count`, `hasMore` or `links`.
- **What the docs say:**
  - A reference is `{id, refName}`, and the item collection carries read-only `count`, `hasMore`, `offset`, `totalResults` and `links` (facts netsuite-rest.md L131, L156, from the RAB).
  - NetSuite's rounding of `quantity`, `rate` and `amount` is UNVERIFIED (facts N1, item 5).
- **Effect:** the echo treats the UNVERIFIED rounding as settled, as "NetSuite stores and returns the exact decimals". Any verify or reconcile test that runs through this default path cannot see NetSuite changing a value. Only `read_normalises_mismatching_values_exactly` (`netsuite.rs:555`) uses a hand-written documented shape.
- **Smallest fix:** in the default GET branch, build the reply in the documented shape instead of echoing:
  - add `refName` to each reference;
  - add `count` and `hasMore` to `item`;
  - add `subtotal` and `taxTotal`.
  - Add one test where the fake returns a `rate` different from the one posted.

### C6-N4: note. The client timeout is far shorter than NetSuite's server-side limit

- **Where:** `apps/api/src/integrations/providers/netsuite/mod.rs:24`, which sets 60 s.
- **What the docs say:** a request can run up to 15 minutes before NetSuite times it out.
  - Source: https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_1559222360.html (facts N5)
  - Quote: "if a request takes more than 15 minutes to complete, it automatically times out"
- **Why it is a note:** a slow create that times out on our side is already classed `Ambiguous` (`errors.rs:44-56`) and goes to reconciliation. The behaviour is correct; it just produces more Ambiguous outcomes on a slow account.
- **Fix:** none needed. Optionally, record create latency at the L3 smoke run.

### C6-F2: note. The fake error body differs from the documented sample in `type` and `title`

- **Where:** `apps/api/tests/api/netsuite.rs:205-212`.
- **What the fake sends:** `"type": "https://www.rfc-editor.org/rfc/rfc9110.html"` and `"title": "Error"` for every status.
- **What the docs show:** an rfc2616 `type` URL, and a status-specific title such as "Not Found" or "Bad Request".
  - Source: https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156570709583.html
- **Effect:** none. The code reads `title` only when `detail` is missing (`errors.rs:80-83`), and never reads `type`.
- **Fix:** optional. Copy the documented `type` and per-status `title`.

### C6-F3: note. Two fake error codes are invented

- **Where:**
  - `apps/api/tests/api/netsuite.rs:618` uses `(403, "FORBIDDEN")`.
  - `netsuite.rs:651` uses `(409, "CONFLICT")`.
- **What the docs say:** no REST page documents a 403 or a 409 `o:errorCode` (facts N6, UNVERIFIED items 9 and 13).
- **Effect:** none on the class. The code keys on the HTTP status (`errors.rs:69-79`). These tests only assert that the invented code is passed through.
- **Fix:** optional. Mark both codes as placeholders in the test comments. Live gates L2 and L5 will supply the real codes.

### C6-F4: note. `providers/fake.rs` sends no provider-shaped payloads

- **Where:** `apps/api/src/integrations/providers/fake.rs` (65 lines).
- **What it does:** it only returns scripted `CapabilityCheck` lists by connection ID. It has no JSON-2 bodies, no nsError bodies, no Location headers and no records, so there is nothing to compare with the docs.
- **Fix:** none.

---

## Checked and conformant

Each item below was checked line by line against the cited source.

### Odoo (JSON-2)

| Check | Code | Source |
|---|---|---|
| `POST {base}/json/2/<model>/<method>`, POST only | `client.rs:127-130` | `json2.py` routes `/json/2/<__model__>/<__method__>` with `methods=['POST']` |
| `Authorization: bearer <key>` | `client.rs:131` | DOC example `Authorization: bearer …`; regex is case-insensitive (facts L28) |
| `Content-Type: application/json; charset=utf-8`; `User-Agent` set | `client.rs:132-133` | DOC header table and example |
| `X-Odoo-Database` only when configured | `client.rs:136-138` | DOC: needed only for multi-database servers without dbfilter |
| `Host` set | reqwest default | DOC: "Most HTTP client libraries automatically set the Host header" |
| Base URL has no trailing slash, so there is no `//json` | `connections/dto.rs:285-293` | — |
| Object body with named arguments; `context` key | `source.rs:168-178, 272-275` | `json2.py` signature `(ids=(), context=…, **kwargs)` |
| `search` called without `ids` (it is `@api.model`) | `source.rs:207-221` | `json2.py`: `@api.model` with ids returns 422 |
| `search` reply is a list of ids | `source.rs:222-226` | `json2.py`: a recordset result becomes `result.ids` |
| `order` strings `write_date asc, id asc` and `id asc` | `source.rs:24, 217, 259` | facts L83 regex |
| Domain prefix notation `[A, B, '\|', X, '&', Y, Z]` | `source.rs:169-174` | facts L79 domain grammar |
| `write_date` parsed as `%Y-%m-%d %H:%M:%S` UTC | `source.rs:22, 112-117` | `tools/json.py`: `json_default` uses `fields.Datetime.to_string` |
| Field names exist in 19.0 | `source.rs:27-50` | see the field list after this table |
| `state` values | `source.rs:119-127` | `purchase_order.py#L105-L111` |
| Money parsed exactly from JSON text, including exponent form | `source.rs:70-79`; `Cargo.toml:29` turns on `arbitrary_precision` | facts O6: `price_unit` is numeric with no fixed scale |
| Section and note lines skipped | `source.rs:292` | `display_type` covers `line_section`, `line_subsection` and `line_note` (purchase_order_line.py#L98-L101) |
| Error body fields `name` and `message`; `debug` never copied | `errors.rs:47-61` | `http.py serialize_exception` |
| 401 → Auth | `errors.rs:70` | DOC example: `werkzeug.exceptions.Unauthorized: 401 … Invalid apikey` |
| 403 → Permission; 404 → NotFound; 409 → Conflict; 422 → Validation | `errors.rs:74-83` | `exceptions.py`: `AccessDenied`/`AccessError` 403, `MissingError` 404, `LockError` 409, `UserError` 422 |
| 400 → Validation | `errors.rs:83` | `http.py`: an unparseable body raises `BadRequest` |
| 500 with `builtins.ValueError` → Validation | `errors.rs:85-87` | `http.py`: the name is `f'{module}.{name}'`; a non-`UserError` returns 500 |
| Every call goes through `Outbound::send`; 30 s timeout; no retry in the adapter; no PUT or PATCH | `client.rs:128-139` | grep of `providers/`: no `PATCH` or `PUT` |

Field names checked to exist in 19.0:
- On `purchase.order` (`purchase_order.py`): `state`, `company_id`, `partner_id`, `currency_id`, `user_id`, `date_order`, `amount_untaxed`, `amount_tax`, `amount_total`.
- On `purchase.order.line` (`purchase_order_line.py#L23-L98`): `order_id`, `display_type`, `product_id`, `product_qty`, `product_uom_id`, `price_unit`, `price_subtotal`, `tax_ids`, `date_planned`.
- `res.currency.name` and `decimal_places`.
- `res.company.po_double_validation` and `po_double_validation_amount` (`res_company.py#L16-L23`).

### NetSuite

| Check | Code | Source |
|---|---|---|
| Token request: `POST {base}/services/rest/auth/oauth2/v1/token`, form-urlencoded | `auth.rs:35-37, 181-186` | `section_162755359851`; the form prefix matches the doc sample byte for byte |
| `grant_type=client_credentials`; `client_assertion_type` jwt-bearer | `auth.rs:37` | `section_162755359851` |
| JWT header `typ=JWT`, `alg=ES256`, `kid`=certificate ID | `auth.rs:81` | `section_162790605110` |
| Claims `iss`=client ID, `scope`="rest_webservices" (text form), `aud`=token URL, `iat`, `exp`=iat+1800 (under 60 min) | `auth.rs:83-89` | `section_162790605110` |
| Base64url with no padding | `auth.rs:90-98` | `section_162790605110` |
| Token reply: `access_token` (string), `expires_in` (number; falls back to 3600) | `auth.rs:191-196` | doc sample `"expires_in": 3600` |
| `invalid_grant` → new assertion, once | `auth.rs:198-216` | `section_162755359851`: on `invalid_grant`, "The application must restart the flow." |
| Token cache keyed by (URL, client, certificate); valid until expiry − 60 s | `auth.rs:106-138` | access token "valid for 60 minutes" |
| `Authorization: Bearer <token>` | `mod.rs:163` | — |
| Create: `POST /services/rest/record/v1/purchaseOrder` with `Content-Type: application/json` | `sink.rs:171-180` | `article_161165602348` sample |
| Body fields `externalId`, `entity{id}`, `subsidiary{id}`, `currency{id}`, `location{id}`, `item.items[]{item{id}, quantity, rate, units}` | `sink.rs:42-66` | PO sample; `section_156334828635` (`externalId` in body); RAB field list (facts L133-181). `units` id-vs-name is noted as UNVERIFIED in the code |
| `X-NetSuite-PropertyNameValidation: error` | `sink.rs:179` | `section_156570709583`: values are `ignore`, `warning`, `error` |
| 204 + `Location`; ID taken from `…/record/v1/purchaseOrder/<digits>` | `sink.rs:186-201, 29-32` | `section_1545141395`: the example Location is an absolute URL |
| No `Prefer: respond-async`; a 202 is treated as Ambiguous | `sink.rs:205-210` | facts N3 |
| Lookup `GET …/purchaseOrder?q=externalId IS "<eid>"`; reads `items[].id` | `sink.rs:135-147` | `section_156334828635` and `section_1545222128` (response `{links, items:[{links, id}], totalResults}`) |
| External ID charset `[A-Za-z0-9_-]` | `sink.rs:21-26` | `section_156334828635` |
| Read: `GET …/purchaseOrder/{id}?expandSubResources=true` | `sink.rs:224-228` | `article_161165602348` sample |
| Error body: `o:errorDetails[0].o:errorCode`, `detail`, `o:errorPath` | `errors.rs:62-91` | `section_156570709583` |
| Status mapping: 400 → Validation, 401 → Auth, 404 → NotFound, 429 → Transient, 5xx → Ambiguous (Transient for reads) | `errors.rs:69-79, 94-99` | `section_156570709583`. The 429 sample is not valid JSON (a comma is missing after `"USER_ERROR"`), so the code keys on status. Correct |
| 403 → Permission by status only | `errors.rs:73` | facts N6 plan fallback |
| Every call goes through `Outbound::execute`: connect timeout 5 s, no redirects, guarded DNS; 60 s request timeout | `mod.rs:130-135`, `outbound.rs:51-66, 94-111` | — |
| One 401 retry on GET only; POST is never retried | `mod.rs:166-193`, `sink.rs:160-212` | — |
| No PUT or PATCH anywhere in `providers/` | grep | — |

### Fakes

| Fake | Verdict |
|---|---|
| `odoo/tests.rs`: replies for `search_read` and `search`, error bodies `{name, message, arguments, context, debug}`, many2one `[id, name]` and bare-id forms, `false` for empty fields | Match the documented shapes. Both many2one forms are tested because the shape is UNVERIFIED item 4. |
| `odoo/tests.rs:384`: `"amount_tax": 1.2e-2` | Python's `json` writes `0.012`, so this exact form never comes from Odoo. It harms nothing: the parser accepts both forms. |
| `tests/api/netsuite.rs`: token reply, Location header, `q=` collection `{items:[{id, links}], totalResults}`, 404 `NONEXISTENT_ID`, 401 `INVALID_LOGIN`, 400 `INVALID_CONTENT` with `o:errorPath`, 429 `CONCURRENCY_LIMIT_EXCEEDED`, 500 `UNEXPECTED_ERROR` | Match the docs. Exceptions: C6-F1, C6-F2 and C6-F3. |
| `tests/api/netsuite.rs:349-355`: token `invalid_grant` returned with HTTP 400 | The status is UNVERIFIED (facts JWT item 17). The code keys on the `error` field, so the status does not matter. |

---

## Addendum: `wf01_purchase/fake_netsuite.rs` (C4b)

- **Base:** worktree `verzo-odoo-netsuite-workflow-e3a751`, tip `b64fe1586` "Merge C4b WF-01 execute + reconcile".
- **File:** `apps/api/src/workflows/wf01_purchase/fake_netsuite.rs` (237 lines).
- **Same rules as above.** Read-only.

**Does the GET read-back echo the POST body?** Yes, the same way C6-F1 does, plus three fields:
- `id`, a computed `subtotal` and a scripted `taxTotal` (`fake_netsuite.rs:109-128, 194-203`).
- An optional `tamper` hook edits the stored PO before it is read back. `exec_tests.rs:550` and `:677` use it to change a line `rate`. Every other read-back returns the posted request JSON.

### C6-F5: risk. The read-back echoes the POST body

- **Where:** `fake_netsuite.rs:109-127` (`store`) and `:197-202` (GET by ID).
- **What the fake does:** the GET reply is the POSTed body plus `id`, `subtotal` and `taxTotal`:
  - `entity`, `subsidiary`, `currency` and the line `item` come back as `{"id": …}`, without `refName` or `links`;
  - `quantity` and `rate` come back with the same digits that were sent;
  - `item` has no `count`, `hasMore` or `links`.
- **What the docs say:**
  - A reference is `{id, refName}`, and the item collection has read-only `count`, `hasMore` and `links` (facts netsuite-rest.md L131, L156, from the RAB).
  - NetSuite's rounding of `quantity`, `rate` and `amount` is UNVERIFIED (facts N1, item 5).
  - Reading sample: https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_161165602348.html
  - Quote: "GET …/purchaseOrder/{{id}}?expandSubResources=true"
- **Effect:**
  - Every happy-path execute or reconcile test verifies against the exact values it sent.
  - Only the `tamper` cases (`exec_tests.rs:550`, `:677`) and `tax_total = "63.01"` (`exec_tests.rs:653`) prove that a mismatch is caught.
- **Smallest fix:** as for C6-F1. Have the default read-back add `refName` to each reference and `count`/`hasMore` to `item`. No default rounding change is needed until L3 measures it.
- **Fixed:** `store()` now reads back `refName` on `entity`, `subsidiary`, `currency` and line `item`, and `count`/`hasMore`/`links` on `item`. Rounding is unchanged.

### C6-F6: risk. The fake's `subtotal` is the exact, unrounded sum of quantity × rate

- **Where:** `fake_netsuite.rs:113-124`.
- **What the fake does:** `subtotal` = Σ `quantity` × `rate` in `Decimal`, with no rounding to the currency's precision.
  - Example: 3 × 0.1234567890123456789 is returned as 0.3703703670370370367.
  - When a test tampers with `rate`, `subtotal` is not recomputed, because it was set at POST time.
- **What the docs say:**
  - `subtotal` is "number (double)" (RAB, facts L146).
  - How NetSuite rounds line `amount` and `subtotal` is UNVERIFIED (facts N1, item 5, and UNVERIFIED list item 5).
- **Effect:** the fake treats "subtotal = exact Σ qty×rate" as settled. Any WF-01 untaxed-total comparison that passes against this fake has not been tested against a rounded, or float-shaped, NetSuite `subtotal`.
- **Smallest fix:** add one test that scripts a NetSuite-style rounded `subtotal` (for example, round to 2 places) and asserts the comparison outcome. Leave the default unchanged until L3 shows the real rule.
- **Fixed:** `exec_tests.rs` `rounded_float_subtotal_still_verifies` (300.0) and `rounded_inexact_float_subtotal_still_verifies` (12.33) read back a 2-place float `subtotal`; both verify as `succeeded`. The default subtotal is unchanged.

### C6-F7: note. A lookup miss is only ever an empty list

- **Where:** `fake_netsuite.rs:178-179` (`miss_lookup`).
- **What the fake does:** a miss returns `{"items": [], "totalResults": 0}`. It never returns 404 for a `q=` miss.
- **What the docs say:** what a `q=` miss returns is UNVERIFIED (facts N2, UNVERIFIED item 7).
  - The 404 `NONEXISTENT_ID` example is shown only for a GET by internal ID.
  - Source: https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156570709583.html
  - Quote: "returns the following error if the request is for a non-existent customer."
- **Effect:** none. `sink.rs` maps both an empty list and a 404 to `None` (C3b code). The WF-01 paths tested here only exercise the empty-list form.
- **Fix:** optional. Add a `miss_lookup_404` variant.

### C6-F8: note. The fake error body has the same `type` and `title` gap as C6-F2

- **Where:** `fake_netsuite.rs:100-103`.
- **What the fake sends:** `"type": "https://www.rfc-editor.org/rfc/rfc9110.html"` and `"title": "Error"` for every status.
- **What the docs show:** an rfc2616 `type` URL and a status-specific title.
  - Source: https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156570709583.html
- **Effect:** none. The code never reads `type`, and reads `title` only when `detail` is missing.
- **Fix:** optional. Copy the documented `type` and per-status `title`.

### Conformant in `fake_netsuite.rs`

- **Token reply:** `{access_token, expires_in: 3600, token_type}` (`:141`), matching `section_162755359851`.
- **POST:** returns 204 with an absolute `Location` of `…/record/v1/purchaseOrder/<id>` (`:150-167`), matching `section_1545141395`.
- **`q=` hit:** `{items:[{id (string), links}], totalResults}` (`:181-190`), matching `section_1545222128`.
- **GET by unknown ID:** 404 `NONEXISTENT_ID` "Record does not exist." (`:204-208`), matching `section_156570709583`.
- **Other methods:** any other method returns 405, and `writes_other_than_post` (`:77-84`) lets tests assert that no PATCH or PUT was sent.
