# NetSuite REST facts pack (R1: N1–N6, M2M JWT, PATCH sublists)

Retrieved 2026-11-17. This pack holds facts only. Each fact cites an official Oracle page. Anything a source does not state is marked **UNVERIFIED**. SOAP-only statements are not used for REST facts, as the parent instructed.

**Sources**
- **Help Center:** pages under `https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/<page>.html`, written below as `NS/<page>`.
- **REST API Browser (RAB):** record service v1, version **2026.1**, at <https://system.netsuite.com/help/helpcenter/en_US/APIs/REST_API_Browser/record/v1/2026.1/index.html>.
  - The page is a JS loader. Its static `chunks/data*.part` files were downloaded and parsed for `#definition-purchaseOrder`, `#definition-purchaseOrder-itemElement` and `#operation--purchaseOrder-*`.
  - The `.../2026.2/index.html` URL returned HTTP 405, so 2026.1 is the newest version read.

**What each verdict means**
- **VERIFIED:** read directly in the cited source.
- **PARTIAL:** some of it is verified and the rest is listed as UNVERIFIED.
- **UNVERIFIED:** no official statement was found.

No NetSuite account was used. Every behaviour below is the documented contract, not observed behaviour.

---

## 0. Base URL, auth, permissions (shared)

| Aspect | Fact | Source |
|---|---|---|
| Host | Account-specific domain `<account ID>.<service>.netsuite.com`, for example `123456.suitetalk.api.netsuite.com`. The service URL is shown at Setup > Company > Setup Tasks > Company Information > Company URLs, and the docs say to keep it a configurable parameter | [NS/section_1546938065](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1546938065.html) |
| Paths | `/services/rest/record/v1/<recordType>[/<id>]`; `/services/rest/query/v1/suiteql` | same |
| Sandbox account ID | The Postman guide gives "3604360 or 3604360_SB1 for Sandbox accounts - follow the exact spelling" | [NS/section_1545058149](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1545058149.html) |
| Sandbox **host** form | **UNVERIFIED**: no REST page found that shows how `_SB1` maps into the hostname | — |
| REST feature | The "REST Web Services" feature must be enabled at Setup > Company > Enable Features > SuiteCloud | [NS/article_5085602973](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_5085602973.html) |
| Role permissions (REST prerequisites page) | "REST Web Services" (Full), "Log in using Access Tokens" (Full), "SuiteAnalytics Workbook" (Edit) | [NS/article_5085602973](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_5085602973.html); [NS/article_0320025211](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_0320025211.html) |
| Role permission for OAuth 2.0 | "Log in using OAuth 2.0 Access Tokens" is needed to "Access REST web services, RESTlets, and SuiteAnalytics Connect using OAuth 2.0 access tokens". A role with only "OAuth 2.0 Authorized Applications Management" **can't** access REST | [NS/section_157771510070](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_157771510070.html) |
| Record-level permissions | REST page: "you must consider which permissions you need … to access forms, sublists, fields". The "Find Transaction" example is given for SuiteScript and SOAP, **not stated for REST** | [NS/article_0320025211](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_0320025211.html) |
| Administrator role | "isn't recommended for security reasons" | [NS/article_5085602973](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_5085602973.html) |
| Request size | "REST requests have a size limit of 104MB." | [NS/chapter_161251386312](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/chapter_161251386312.html) |
| Edit-mode load | "REST web services always load record instances in edit mode, even for GET requests." | [NS/subsect_1544786455](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_1544786455.html) |
| Custom sublists | "Custom sublists are not available in REST web services." | same |

- **UNVERIFIED:** which purchase-specific permissions (for example on Purchase Order) a least-privilege M2M role needs. This is live gate L5.
- The prerequisites page names "Log in using Access Tokens" (the TBA permission). The OAuth 2.0 page names "Log in using OAuth 2.0 Access Tokens". Whether an M2M-only role also needs the TBA permission is **UNVERIFIED**.

---

## JWT. OAuth 2.0 client credentials (M2M) — **VERIFIED**, with doc inconsistencies noted

### Token request ([NS/section_162755359851](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_162755359851.html))

| Item | Value |
|---|---|
| URL | `POST https://<accountID>.suitetalk.api.netsuite.com/services/rest/auth/oauth2/v1/token` |
| Content-Type | `application/x-www-form-urlencoded`. "Request parameters must be encoded based on the HTML specification for the application/x-www-form-urlencoded media type." |
| `grant_type` | "always `client_credentials`" |
| `client_assertion_type` | "always `urn:ietf:params:oauth:client-assertion-type:jwt-bearer`" |
| `client_assertion` | A JWT "signed with the private part of the certificate used for mapping of the application" |
| Response | `access_token` (JWT, "valid for 60 minutes"), `expires_in` ("always 3600"), `token_type` ("always bearer") |
| Expiry | "When the access token expires, the token endpoint returns an invalid_grant error. The application must restart the flow." |

### JWT header and payload ([NS/section_162790605110](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_162790605110.html))

| Claim | Doc text |
|---|---|
| `typ` | "always JWT" |
| `alg` | "PS256, PS384, PS512, ES256, ES384, or ES512" |
| `kid` | "the Certificate ID generated during the mapping of the application" |
| `iss` | "the client ID for the integration" |
| `scope` | "restlets, rest_webservices, suite_analytics, or all of them, separated by a comma" |
| `aud` | "the NetSuite token endpoint, `https://<accountID>.suitetalk.api.netsuite.com/services/rest/auth/oauth2/v1/token`" |
| `exp` | seconds since epoch; "must be less than 60 minutes greater than the value of the iat parameter" |
| `iat` | seconds since epoch |
| Encoding | "dot-separated values are Base64url encoded" |

- **Doc inconsistency (sample vs text).** Decoding the sample `client_assertion` in NS/section_162755359851 gives:
  - header `{"typ":"JWT","alg":"ES512","kid":"…"}`
  - payload with `"scope":["restlets","rest_webservices"]` (a JSON **array**), `"aud":"https://3829855.restlets.api.netsuite.com/…/token"` (a **restlets** host) and an extra `"jti"`.
  - The text says `scope` is comma-separated and `aud` is the suitetalk host. **Which forms the server accepts is UNVERIFIED** (live gate L5). The text form is the documented contract.
- `jti` is not listed in the claims table. Whether it is required is **UNVERIFIED**.
- The Login Audit Trail error `TokenValidityTooBroad` reads: "Ensure that the validity of the access token in the client credentials flow doesn't exceed two hours." That is a different limit from the 60-minute `exp − iat` rule ([NS/subsect_158324637123](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_158324637123.html)).

### Certificates ([NS/subsect_162755332391](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_162755332391.html), [NS/section_162686838198](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_162686838198.html))

- **Format:** the public key is x.509 (`.cer`/`.pem`/`.crt`). RSA keys are 3072 or 4096 bits. EC keys are 256, 384 or 521 bits.
- **Validity:** "The maximum certificate validity is two years. If the certificate is valid for a longer time period, the system automatically shortens the validity to two years."
- **ES256 example:** `openssl req -new -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 …`.
  - **Not stated:** the ECDSA signature encoding (raw R‖S vs DER) for the token endpoint. JWS ES256 normally uses raw R‖S, but **this is UNVERIFIED** against NetSuite text.
- **Limits:** "a maximum of five active certificates for each combination of integration record, entity, and role". "One certificate can only be used for one combination of integration record, role, and entity."
- **Revocation:** a revoked or expired certificate means you "must create a new mapping".
- **Setup location:** Setup > Integration > Manage Authentication > OAuth 2.0 Client Credentials (M2M) Setup. Doing it needs Administrator or "OAuth 2.0 Authorized Applications Management".
- **Sandbox refresh:** "Each time a sandbox account is refreshed, the setup gets cleared."
- **Not copied** to other production, Release Preview, sandbox or Next Preview accounts.

### Token and auth errors in the Login Audit Trail ([NS/subsect_158324637123](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_158324637123.html))

`InvalidSignature`, `ScopeMismatched`, `InvalidScope`, `EntityOrRoleDisabled`, `FeatureDisabled`, `IntegrationBlocked`, `UnsupportedAlgo`, `TokenValidityTooBroad`, `SignatureValidation`, `TokenNotAcceptable`, `TokenExpired`, `InvalidIssuer`, `MissingKeyId`, `MissingKey`, `AccessTokenExpired`.

These codes are shown **in the NetSuite UI audit trail**. What the HTTP response of the token endpoint contains for each is **UNVERIFIED**, beyond `invalid_grant` on expiry.

---

## N1. `purchaseOrder` over REST — **PARTIAL** (contradiction in Oracle docs)

### Support status

- **Supported:** "NetSuite exposes the purchase order record to REST web services." Record ID `purchaseOrder`. Prerequisite: the "Purchase Orders" feature ([NS/article_161165602348](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_161165602348.html)).
- **Not labelled beta.** The page does not say "beta". GA vs beta is therefore **not stated**. No beta label was found in the RAB entry either.
- **Same page, contradictory statements:**
  - "The following elements are not accessible through REST web services: accounting books, **items**, tax detail". If "Allow Expenses on Purchases" is set, the expenses sublist is also not accessible.
  - The **same page's** "Creating a Purchase Order" sample posts an item sublist: `{"entity":{"id":…},"item":{"items":[{"item":{"id":…},"rate":10}]}}`.
  - The RAB 2026.1 schema also defines `item: purchaseOrder-itemCollection`.
  - **Whether the `item` sublist can be written over REST is UNVERIFIED.** This blocks C3b's create payload until live gate **L3** confirms it.
- **Tax:** "REST web services do not support legacy tax features. To work with taxation through REST web services, you must have the SuiteTax feature enabled."
- **2026.1 tax behaviour:** setting `grossAmt` on a taxable line now recalculates the line `amount`.

### Operations (RAB 2026.1)

| Operation | Path | Success | Notes |
|---|---|---|---|
| Insert | `POST /purchaseOrder` | **204 No Content**, "Inserted record." (202 if async) | Query params `replace`, `fields`. Headers `Prefer: respond-async`, `X-NetSuite-Idempotency-Key`, `X-NetSuite-PropertyNameValidation` (default Warning), `X-NetSuite-PropertyValueValidation` (default Error) |
| Get | `GET /purchaseOrder/{id}` | 200 `purchaseOrder` | RAB types `id` as "integer (int32) … Internal identifier". Params `expandSubResources`, `simpleEnumFormat`, `fields` |
| Upsert | `PUT /purchaseOrder/{id}` | **204** "Upserted record." | RAB types `id` as "string … External identifier". Params `replace`, `replaceSelectedFields` |
| Update | `PATCH /purchaseOrder/{id}` | 204 | see the PATCH section |
| List / filter | `GET /purchaseOrder?q=…&limit=1000&offset=0` | 200 `purchaseOrderCollection` | `ids=` "comma separated list of internal or external IDs. Prefix external IDs with 'eid:'" (batch only, `expandRecords=true`) |
| Transform | `POST /purchaseOrder/{id}/!transform/itemReceipt` (also `vendorBill`, `vendorReturnAuthorization`) | — | anchors exist in RAB; contract not read |
| Error | any | `default: nsError` | see N6 |

Generic create rule ([NS/section_1545141395](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1545141395.html)):
- POST expects a body that "conforms to the record's metadata schema and contains values for at least each mandatory field".
- The response is "HTTP Code 204", and "The URL of the newly created record is given in the Location header", for example `Location: …/record/v1/customer/647`.
- "Any omitted fields are considered empty or to have default values." `id` is read-only.
- "REST web services do not support the update of subrecords, and do not support the bulk creation of records."

### Field names (RAB 2026.1, `definition-purchaseOrder`)

Body fields that matter for WF-01. Types are as the RAB shows them; a reference is an object `{id, refName}`.

| Field | RAB type |
|---|---|
| `entity` | reference, "one of: [ customer, partner, vendor, nsResource, employee, contact ]" |
| `subsidiary` | `subsidiary` reference |
| `currency` | `currency` reference |
| `exchangeRate` | number (double) |
| `location` | `location` reference |
| `externalId` | string, "External ID" |
| `tranDate` | string (date) |
| `tranId` | string, "PO #" |
| `memo` | string |
| `otherRefNum` | string, "PO/Check Number" |
| `dueDate` | string (date) |
| `subtotal` | number (double), "Subtotal" |
| `taxTotal` | number (double), "Tax Total" |
| `tax2Total` | number (double), "PST" |
| `total` | number (double), "Total" |
| `totalAfterTaxes` | number (double) |
| `approvalStatus` | object, `id` enum `11, 1, 2, 3` |
| `nextApprover` | `employee` reference |
| `supervisorApproval` | boolean |
| `orderStatus` / `status` | object, `id` enum `P, A, B, C, D, E, F, G, …` |
| `createdDate` / `lastModifiedDate` | string (date-time) |
| `item` | `purchaseOrder-itemCollection` (`items: purchaseOrder-itemElement[]`, plus read-only `count`, `hasMore`, `offset`, `totalResults`, `links`) |
| `expense` | `purchaseOrder-expenseCollection` |
| `taxDetails` | `purchaseOrder-taxDetailsCollection` |

Item line fields (`definition-purchaseOrder-itemElement`):

| Field | RAB type |
|---|---|
| `item` | reference ("one of: [ kitItem, … ]") |
| `quantity` | number (float) |
| `rate` | number (double) |
| `amount` | number (double) |
| `units` | **string** ("Units") |
| `description` | string |
| `location` | `location` reference |
| `line` | integer (int64), "Transaction Line" |
| `expectedReceiptDate` | string (date) |
| `taxCode` | nsResource |
| `taxAmount` | number (double) |
| `tax1Amt` | number (double) |
| `taxRate1` | number (double) |
| `grossAmt` | number (double) |
| `rateIncludingTax` | number (double) |
| `isClosed` | boolean |
| `uniqueKey` | integer (int64) |
| `vendorName` | string |

- **Required fields:** the RAB schema marks **no** body or line field as required in the parsed HTML (only `links`, `count`, `hasMore`, `offset` and `totalResults` carry a read-only marker). Required fields depend on account features and forms. **UNVERIFIED**, live gate **L3**.
- **Read-only:** whether `subtotal`, `total` and `taxTotal` are read-only is **not marked** in RAB. **UNVERIFIED**.
- **Enum meanings:** the meaning of the `approvalStatus` ids (`1`, `2`, `3`, `11`) is **not stated** in RAB. **UNVERIFIED**.
- **Number precision:** `quantity` is a float and `rate` and `amount` are doubles. NetSuite's decimal precision and rounding for `amount = quantity × rate` are **UNVERIFIED**.

---

## N2. Look up a record by external ID — **PARTIAL**

| Method | Fact | Source |
|---|---|---|
| `eid:` in the URL | "In REST web services, an external ID starts with the prefix "eid:" … You can use external IDs anywhere in the URL where an internal ID can be used." Example: `GET …/record/v1/customer/eid:CID001/subscriptions/eid:SUID042`. Allowed characters are letters, numbers, `_` and `-` | [NS/section_156334828635](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156334828635.html) |
| `q=` filter | `GET …/record/v1/purchaseOrder/?q=externalId IS "RED|PO|1337"`. This is the documented workaround when the ID contains `|`, which is otherwise treated as a multi-select delimiter | same |
| Filter grammar | `q` = field, operator, value; `AND`/`OR`/`()`. String operators: `CONTAIN, CONTAIN_NOT, IS, IS_NOT, START_WITH, START_WITH_NOT, ENDWITH, END_WITH_NOT`. Filterable fields carry `x-ns-filterable` in metadata. The response lists `{id, links}` per hit plus `totalResults`. The default page is `limit=1000&offset=0` | [NS/section_1545222128](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1545222128.html); RAB `operation--purchaseOrder-get` |
| Queries | "Queries only return record IDs and HATEOAS links … you can only use body fields in query conditions." | [NS/subsect_1544786455](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_1544786455.html) |
| SuiteQL | `POST …/services/rest/query/v1/suiteql?limit=&offset=` with the header `Prefer: transient` ("required"). Body `{"q": "...", "params": [...]}` (anonymous `?` bound parameters). At most 100,000 results. "Asynchronous request execution isn't supported in the REST query service." | [NS/section_157909186990](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_157909186990.html); [NS/subsect_1544786455](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_1544786455.html) |

**Gaps**
- `GET /purchaseOrder/eid:<x>` is not shown for `purchaseOrder` specifically. The RAB types the GET `{id}` path parameter as `integer (int32)` "Internal identifier", while the guide says `eid:` works "anywhere … an internal ID can be used". **This conflict is UNVERIFIED** (live gate L2).
- What a miss returns:
  - **`eid:` GET miss:** **UNVERIFIED**. The generic non-existent id example is `404` with `o:errorCode: NONEXISTENT_ID`, shown for an internal id.
  - **`q=` miss:** **UNVERIFIED**. It presumably returns an empty collection; not stated.
- The SuiteQL column name for the external ID on `transaction` (`externalid`?) is **UNVERIFIED**. The Records Catalog is in-account only and was not readable.
- **Read-after-write consistency** of `q=` and SuiteQL lookups right after a create: **UNVERIFIED**. The plan never treats "not found" as proof, per R3-§6c.

---

## N3. POST that includes an existing `externalId` — **UNVERIFIED** (no REST source)

- **Verified:** `externalId` may be sent in a POST body ("You can also use an external ID in the request body with the field name externalId", [NS/section_156334828635](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156334828635.html)).
- **Verified:** upsert is `PUT …/eid:<id>`, which "will be added" if absent and "updated" if present. It is usable "only … when you use an external ID in the request URL and when you use the PUT HTTP method" ([NS/section_156335203191](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156335203191.html)).
- **Not found:** any REST page saying what `POST` does when that `externalId` already exists (error code, HTTP status, or silent duplicate).
  - CSV-import and SOAP pages mention duplicate external ID errors. Per the parent's rule they are **not** applied to REST.
  - The plan stays as written: the existence check runs first, and any conflict-shaped error is classed `Conflict` and sent to reconciliation (live gate **L2**).

### Async idempotency key (related; REST-documented)

- **Header name:** `X-NetSuite-Idempotency-Key`.
  - RAB: "A user-defined unique idempotency key that is applied to every asynchronous requests to ensure that the request is executed only once. Only one request can be executed with every unique idempotency key. Use UUID in string format as defined by RFC 4122. **If the request is executed synchronously, this value is ignored.**"
  - The help example writes the header as `X-NetSuite-idempotency-key` ([NS/subsect_164494900632](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_164494900632.html)).
- **Duplicate key:** the documented response has the status line `HTTP 400 Conflict`, the body `"status": 400`, `"title": "Conflict"` and `o:errorCode` `IDEMPOTENCY_ERROR`, plus `Location: /services/rest/async/v1/job/<id>` pointing to the earlier job (same page).
  - The text mixes 400 with the "Conflict" title and a `#sec10.4.10` (409) type URL. **The actual status code is UNVERIFIED.**
- **Async job flow:**
  - Sending `Prefer: respond-async` returns `202 Accepted`, `Preference-Applied: respond-async` and `Location: /services/rest/async/v1/job/<id>`.
  - `GET /async/v1/job/<id>` returns `{completed, id, progress, task{links}}` ([NS/subsect_161278579295](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_161278579295.html)).
  - Async processing is said to "help to avoid creating duplicate records" ([NS/chapter_161251386312](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/chapter_161251386312.html)).
- **UNVERIFIED:** how long a key is retained and its scope (per account, user or integration); how the task result (new record id) is read.

---

## N4. `approvalStatus` and approval routing on REST create — **UNVERIFIED** (no REST source)

- **Verified (RAB):** `purchaseOrder` exposes `approvalStatus` (object, `id` enum `11, 1, 2, 3`), `nextApprover` (employee) and `supervisorApproval` (boolean).
- **Not found:** any REST page on how PO approval routing, SuiteFlow workflows or the Purchase Order Approval Workflow SuiteApp act on a record created through REST. That includes whether the status is forced to pending, whether a client value is honoured, and whether workflows trigger.
  - The purchase-approval pages read ([NS/section_N2396564](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2396564.html), [NS/section_3960256538](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_3960256538.html)) do not mention REST or web services.
- **Plan impact:** Flow sets no approval fields (PLAN §8 row N4); live gate **L4**.

---

## N5. Concurrency per tier — **VERIFIED** (REST is covered by the account limit)

- **REST page:** "Concurrency for REST web services is governed by an account limit, where each request counts toward the account limit. The account governance limit applies to the combined total of web services and RESTlet requests." Also: "if a request takes more than 15 minutes to complete, it automatically times out" ([NS/subsect_1559222360](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_1559222360.html)).
- **Tier table:** the REST page links to it ([NS/bridgehead_1500275603](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/bridgehead_1500275603.html)). The base limit grows **+10 per SuiteCloud Plus licence**. Development and partner accounts are always 5 and do not scale.

  | Tier (contracts from June 2020) | Base | Older tier | Base |
  |---|---|---|---|
  | Standard | 5 | Shared, 3 | 5 |
  | Premium | 15 | 2 | 10 |
  | Enterprise | 20 | 1, 1+ | 15 |
  | Ultimate | 20 | 0 | 20 |

- **Sandbox:** "Sandbox accounts match your production governance limit. Developer accounts always have a governance limit of five." ([NS/section_1504769959](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1504769959.html)).
- **Runtime discovery:** `GET /services/rest/system/v1/governanceLimits` returns `accountConcurrencyLimit`, `accountUnallocatedConcurrencyLimit`, `integrationConcurrencyLimit` (when integration-specific) and `integrationLimitType` (`integrationSpecific` | `accountLimit` | `internal`). It "returns results only if you are logged in as an administrator", so it is **not usable by a least-privilege role** ([NS/article_0625010817](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_0625010817.html)).
- **When exceeded** ([NS/section_156570709583](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156570709583.html)): HTTP **429** with `o:errorCode` `CONCURRENCY_LIMIT_EXCEEDED` ("Concurrent request limit exceeded. Request blocked."). The doc says "retry sending the request".
  - The sample also carries a top-level `"o:errorCode": "USER_ERROR"` and `"title": "Bad Request"`, so the sample is internally inconsistent. Key on `status` and on the detail code.
- **UNVERIFIED:** a `Retry-After` header and any per-minute or per-day request quotas for REST.

---

## N6. Error format and permission errors — **PARTIAL**

**Error body** ([NS/section_156570709583](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156570709583.html); RAB `definition-nsError`):
- Top level: `{type, title, status, "o:errorDetails":[{detail, "o:errorCode", "o:errorPath"?, "o:urlPath"?, "o:errorHeader"?, "o:errorQueryParam"?}]}`.
  - The RAB names the URL field `o:errorUrl`. The help page names it `o:urlPath`. **Which one is real is UNVERIFIED.**
- Status classes: "4xx status codes are used for failures due to user error. 5xx status codes are used for failures due to system error."

**Documented REST examples**

| Status | `o:errorCode` | Case |
|---|---|---|
| 401 | `INVALID_LOGIN` | invalid login token; the detail points to the Login Audit Trail |
| 400 | `INVALID_CONTENT` | invalid field value, with `o:errorPath` such as `item.items[0].item` |
| 400 | `INVALID_REQUEST` | malformed request |
| 404 | `NONEXISTENT_ID` | "Record does not exist." |
| 429 | `CONCURRENCY_LIMIT_EXCEEDED` | concurrency (see N5) |
| 500 | `UNEXPECTED_ERROR` | system error, with an error ID |
| 400 (per example) | `IDEMPOTENCY_ERROR` | duplicate async idempotency key (see N3) |

**Validation headers.**
- `X-NetSuite-PropertyNameValidation`: `ignore | warning | error`, default `warning`. Unknown field names are not rejected unless this is set to `error`.
- `X-NetSuite-PropertyValueValidation`: default `error`.
- Warnings come back in response headers. Source: same page.

**Permission failures:**
- **Not documented for REST.** No REST page found gives the HTTP status or `o:errorCode` for missing role or record permissions, or for a disabled feature.
  - `INSUFFICIENT_PERMISSION` appears in the **SOAP** "Error Status Codes" list ([NS/section_N3539978](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N3539978.html)). Per the parent's rule it is **not** asserted for REST.
  - The one REST statement found is the edit-mode note: a non-admin GET of an admin employee fails with "For security reasons, only an administrator is allowed to edit an administrator record" ([NS/subsect_1544786455](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_1544786455.html)). Its status and code are not given.
- **Plan impact:** PLAN §8 row N6 fallback stands (`Permission` only by HTTP 403). Live gate **L5** must capture the real responses.

---

## PATCH and sublist semantics — **VERIFIED** (generic records; not PO-specific)

Sources: [NS/chapter_1545142407](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/chapter_1545142407.html), [NS/section_1545295601](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1545295601.html), [NS/section_1545297048](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1545297048.html).

- **PATCH without `replace`:** "lines of a keyed sublist are updated if the line contains matching keys, and the remaining lines from the request are added. For non-keyed sublists, the lines are always **appended**." Omitted body fields stay as they were.
- **Nullify:** `"sublist": {"items": null}` or `"sublist": null` empties the sublist. "If the sublist is mandatory, it is expected that the operation fails."
- **PATCH `?replace=<sublist>[,<sublist2>]`:** keyed sublist lines with matching keys are updated, "other lines are removed", and unmatched request lines are added. For non-keyed sublists "all lines are always removed and replaced". "The operation fails if any replaced default sublist line is read-only."
- **Deleting one line:** "The delete operation is not available for removing single lines". Instead, use `PATCH /record/v1/purchaseOrder/:id?replace=item` with all lines except the ones to delete. This example is **explicitly for `purchaseOrder`**.
- **RAB `PUT`** also accepts `replaceSelectedFields` (boolean): "If set to 'true', all fields that should be deleted in the update request, including body fields, must be included in the 'replace' query parameter."
- **UNVERIFIED:** whether the PO `item` sublist is keyed, and if so on which field (`line`? `uniqueKey`?). This decides whether a PATCH without `replace` duplicates lines.
- **Doc inconsistency:** the "Replacing a Sublist" page shows `POST …?replace=` returning "201 Created" in its examples, while the create page and RAB say POST returns 204. The 204 contract (RAB) is taken as authoritative.

---

## UNVERIFIED (NetSuite)

1. **N1:** whether the PO `item` sublist is writable over REST. The Records Guide lists "items" as not accessible, while its own sample and the RAB use it. **Blocks the C3b create payload until L3.**
2. **N1:** GA vs beta label for `purchaseOrder`. The docs show none.
3. **N1:** required body and line fields (RAB marks none) and whether `subtotal`, `total` and `taxTotal` are read-only.
4. **N1:** meaning of the `approvalStatus` enum ids `1`, `2`, `3`, `11`, and of the `orderStatus` letters.
5. **N1:** NetSuite rounding of line `amount` from `quantity × rate`, and of `subtotal` and `total`.
6. **N2:** `GET /purchaseOrder/eid:<x>`. The guide says yes; the RAB types `{id}` as an int32 internal id.
7. **N2:** the response for a missed `eid:` GET or `q=` filter, and read-after-write consistency of `q=` and SuiteQL.
8. **N2:** the SuiteQL column name for the external ID on `transaction`.
9. **N3:** what POST does with an existing `externalId` (no REST statement).
10. **N3:** idempotency-key retention and scope, the real status code (400 vs 409), and how the async task result is read.
11. **N4:** how approval routing or SuiteFlow treats REST-created POs, and whether a client-set `approvalStatus` is honoured.
12. **N5:** `Retry-After` on 429; any REST quotas beyond concurrency.
13. **N6:** HTTP status and `o:errorCode` for missing role, record or field permission, or a disabled feature over REST.
14. **N6:** `o:urlPath` (help page) vs `o:errorUrl` (RAB).
15. **JWT:** whether `scope` must be a comma-separated string or may be a JSON array, and whether `aud` must be the suitetalk host (the sample uses the restlets host); whether `jti` is required.
16. **JWT:** the ECDSA signature encoding expected (raw R‖S per JWS vs DER).
17. **JWT:** the HTTP response bodies for the token endpoint's failure modes, other than `invalid_grant` on expiry.
18. **Permissions:** whether an OAuth 2.0 M2M role also needs the TBA "Log in using Access Tokens" permission, and which purchase permissions a least-privilege role needs.
19. **Host:** how a sandbox account ID (`<id>_SB1`) maps into the `suitetalk.api.netsuite.com` hostname.
20. **PATCH:** whether the PO `item` sublist is keyed, and on which field.
21. **Version:** RAB 2026.2 was not reachable (HTTP 405), so this pack reflects RAB 2026.1.
