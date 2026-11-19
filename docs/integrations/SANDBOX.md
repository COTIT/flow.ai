# Sandbox setup and live smoke kit (WF-01, PLAN REV 5)

This runbook takes you from no sandboxes to the six LIVE gates L1–L6 (PLAN §8). Once both sandboxes exist, running the tests in step 4 is the only step left.

Rules for this file:
- Every setup step cites an official URL that is already in the facts packs ([odoo-19.md](facts/odoo-19.md), [netsuite-rest.md](facts/netsuite-rest.md)).
- Where the facts packs have no source for a step, the step says **UNVERIFIED**. Do it by following the vendor UI and record what you did when you write results back (step 5).
- The tests live in `apps/api/src/integrations/live.rs`. They call the real adapters (`providers::odoo`, `providers::netsuite`) through `Outbound`.

Short forms (as in the facts packs):
- `[DOC]` = Odoo 19 "External JSON-2 API", <https://www.odoo.com/documentation/19.0/developer/reference/external_api.html>
- `GH/<path>#Lx` = `https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/<path>#Lx`
- `NS/<page>` = `https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/<page>.html`

---

## 1. Odoo 19 (gate L1; read only)

No test writes to Odoo. The API key belongs to a user that should only be able to read.

1. **Instance on a Custom plan.** JSON-2 is "only available on Custom Odoo pricing plans … not available on One App Free or Standard plans" ([DOC]). Version must be 19.0 (JSON-2 is new in 19.0, [DOC]).
   - Signing up for the instance and choosing the plan: **UNVERIFIED** (no source in the facts pack).
2. **Apps.** `purchase` must be installed (the adapter reads `purchase.order`, `purchase.order.line`). `rpc` is `auto_install` and depends only on `base` ([GH/addons/rpc/__manifest__.py#L10-L12](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/rpc/__manifest__.py#L10-L12)).
3. **Dedicated bot user.** [DOC] (Access Rights): every call is checked against "the access rights, record rules and field accesses of the user", and the doc recommends dedicated bot users.
   - The user must be an internal user (`base.group_user`) whose `company_ids` include the test company; that is enough to read `res.company` controls and `res.currency` ([odoo-19.md O5](facts/odoo-19.md), [GH/odoo/addons/base/security/ir.model.access.csv#L42-L45](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/security/ir.model.access.csv#L42-L45), [#L59-L65](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/security/ir.model.access.csv#L59-L65)).
   - The Purchase group that gives **read-only** access to `purchase.order` / `purchase.order.line`: **UNVERIFIED** (purchase ACLs were not read). Pick the lowest Purchase role and record it.
   - Do **not** give this user `purchase.group_purchase_manager`: with it `_approval_allowed()` is always true, which defeats the control in step 5 ([GH/addons/purchase/models/purchase_order.py#L1257-L1266](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order.py#L1257-L1266)).
4. **API key for that user.** [DOC] (API Keys): a key created in the UI needs a description and a duration; **keys cannot last more than three months**. Note the expiry date; L1 fails with 401 after it. The UI clicks to create the key: see [DOC] (API Keys).
5. **Company approval control (WF-01).** On the test company set `po_double_validation = two_step` and `po_double_validation_amount = 0`, so every RFQ needs a second approval ([GH/addons/purchase/models/res_company.py#L16-L23](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/res_company.py#L16-L23); settings mirror `res.config.settings.po_order_approval` / `po_double_validation_amount` at [GH/addons/purchase/models/res_config_settings.py#L11-L15](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/res_config_settings.py#L11-L15)).
   - The Settings screen label for this switch: **UNVERIFIED**.
6. **Test data.** One RFQ in state `draft` or `sent` in that company, with at least one product line. Note its id (optional; L1 otherwise takes the first eligible id).

## 2. NetSuite sandbox (gates L2–L6)

1. **Account.** A sandbox account. The Postman guide spells sandbox account ids like `3604360_SB1` ([NS/section_1545058149](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1545058149.html)).
   - How `_SB1` maps into the REST hostname: **UNVERIFIED** ([netsuite-rest.md l.27](facts/netsuite-rest.md)). Read the real service URL at Setup > Company > Setup Tasks > Company Information > Company URLs ([NS/section_1546938065](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1546938065.html)). Because the host form is unverified, every writing test also needs `LIVE_NETSUITE_CONFIRM_SANDBOX=yes` (step 4).
   - A sandbox refresh clears the M2M setup; redo steps 2.4–2.5 after each refresh ([NS/subsect_162755332391](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_162755332391.html)).
2. **Features.** Setup > Company > Enable Features > SuiteCloud: enable "REST Web Services" ([NS/article_5085602973](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_5085602973.html)).
   - "Purchase Orders" feature is a prerequisite of the `purchaseOrder` record ([NS/article_161165602348](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_161165602348.html)).
   - Taxes over REST need SuiteTax; legacy tax is not supported (same page).
   - The OAuth 2.0 feature switch: **UNVERIFIED** (no source in the facts pack beyond the M2M setup page below).
3. **Integration record.** Create one with the client credentials (M2M) grant and note its client id (the JWT `iss`, [NS/section_162790605110](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_162790605110.html)).
   - The integration-record screen and its checkbox names: **UNVERIFIED** (no source in the facts pack).
4. **EC P-256 certificate.** NetSuite takes an x.509 public certificate (`.cer`/`.pem`/`.crt`); EC keys may be 256, 384 or 521 bits; maximum validity two years ([NS/subsect_162755332391](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_162755332391.html), [NS/section_162686838198](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_162686838198.html)). NetSuite's ES256 example starts `openssl req -new -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 …` (same pages).

   ```bash
   openssl req -new -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 730 -subj "/CN=verzo-wf01-sandbox" -keyout verzo-ns-ec.key -out verzo-ns-ec.crt
   ```
   - The prefix up to `prime256v1` is from NetSuite. `-nodes -days 730 -subj -keyout -out` is **standard OpenSSL usage; not from NetSuite docs** ([openssl-req](https://docs.openssl.org/3.0/man1/openssl-req/)). 730 days = the two-year cap.

   ```bash
   openssl pkcs8 -topk8 -nocrypt -in verzo-ns-ec.key -out verzo-ns-ec.pk8.pem
   ```
   - **Standard OpenSSL usage; not from NetSuite docs** ([openssl-pkcs8](https://docs.openssl.org/3.0/man1/openssl-pkcs8/)).
   - Why: the adapter (`apps/api/src/integrations/providers/netsuite/auth.rs`, `pkcs8_der`) accepts only an **unencrypted PKCS#8** PEM (`-----BEGIN PRIVATE KEY-----`). It refuses SEC1 (`BEGIN EC PRIVATE KEY`) and encrypted keys, then loads it as an EC P-256 key for ES256. `openssl req -nodes` may already write PKCS#8; the `pkcs8` step makes it certain.
   - Keep `verzo-ns-ec.pk8.pem` outside the repo (`chmod 600`). Upload only `verzo-ns-ec.crt`.
5. **Map the certificate (M2M setup).** Setup > Integration > Manage Authentication > OAuth 2.0 Client Credentials (M2M) Setup; needs Administrator or "OAuth 2.0 Authorized Applications Management" ([NS/subsect_162755332391](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_162755332391.html)). Map integration record + entity + the role from section 3 + the `.crt`. The mapping shows the **certificate id**: that is the JWT `kid` ([NS/section_162790605110](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_162790605110.html)).
   - Limits: one certificate per integration/role/entity combination; at most five active certificates per combination (same page).
6. **Test data ids.** Note internal ids of: a vendor, its subsidiary, a currency the vendor uses, an inventory or non-inventory item, optionally a location, and (L6) a second currency the vendor is **not** set up for. Where to read internal ids in the UI: **UNVERIFIED**.
7. **Approval routing (L4).** Turn on the PO approval routing you want to prove (purchase-approval pages: [NS/section_N2396564](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2396564.html), [NS/section_3960256538](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_3960256538.html)). How routing treats a REST-created PO is **UNVERIFIED** (N4); that is exactly what L4 records. Exact setup clicks: **UNVERIFIED**.

## 3. Least-privilege NetSuite role (gate L5)

Do not use Administrator: "isn't recommended for security reasons" ([NS/article_5085602973](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_5085602973.html)).

| Permission | Level | Source |
|---|---|---|
| REST Web Services | Full | [NS/article_5085602973](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_5085602973.html), [NS/article_0320025211](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_0320025211.html) |
| Log in using OAuth 2.0 Access Tokens | (level not stated) | [NS/section_157771510070](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_157771510070.html) |
| Log in using Access Tokens (TBA) | Full | prerequisites page above; **UNVERIFIED** whether an M2M-only role needs it. Start without it; add it only if L5 fails, and record that |
| SuiteAnalytics Workbook | Edit | prerequisites page above |
| Purchase Order (and any record permissions) | **UNVERIFIED** | the REST page says to consider record, sublist and field permissions ([NS/article_0320025211](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_0320025211.html)) but lists none for PO. Start minimal, add what L2–L5 errors name, record each addition |

A role with only "OAuth 2.0 Authorized Applications Management" can't access REST ([NS/section_157771510070](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_157771510070.html)).

## 4. Running the live tests

Run from `apps/api`. Tests are `#[ignore]` and are selected by name with `--ignored`. A test whose env vars are missing prints `SKIP Lx: not set: …` and passes; nothing else is skipped. NetSuite tests that write (L2, L3, L4, L6) also print `SKIP` unless **both** `LIVE_NETSUITE_ENVIRONMENT=sandbox` and `LIVE_NETSUITE_CONFIRM_SANDBOX=yes` are set.

Never print or commit these values. Put them in a local file outside the repo and `source` it.

| Var | Used by | Meaning |
|---|---|---|
| `LIVE_ODOO_BASE_URL` | L1 | `https://<your-db>.odoo.com`, no trailing path |
| `LIVE_ODOO_API_KEY` | L1 | key from 1.4 |
| `LIVE_ODOO_COMPANY_ID` | L1 | `res.company` id with the 1.5 control |
| `LIVE_ODOO_DATABASE` | L1, optional | sent as `X-Odoo-Database` |
| `LIVE_ODOO_RFQ_ID` | L1, optional | RFQ from 1.6 |
| `LIVE_NETSUITE_REST_DOMAIN` | L2–L6 | host from 2.1, e.g. `<account>.suitetalk.api.netsuite.com` (no scheme) |
| `LIVE_NETSUITE_CLIENT_ID` | L2–L6 | 2.3 |
| `LIVE_NETSUITE_CERTIFICATE_ID` | L2–L6 | 2.5 |
| `LIVE_NETSUITE_PRIVATE_KEY_PATH` | L2–L6 | path to `verzo-ns-ec.pk8.pem` |
| `LIVE_NETSUITE_ENVIRONMENT` | L2, L3, L4, L6 | must be `sandbox` |
| `LIVE_NETSUITE_CONFIRM_SANDBOX` | L2, L3, L4, L6 | must be `yes` |
| `LIVE_NETSUITE_VENDOR_ID` / `_SUBSIDIARY_ID` / `_CURRENCY_ID` / `_ITEM_ID` | L2, L3, L4, L6 | 2.6 |
| `LIVE_NETSUITE_LOCATION_ID` | optional | sent as `location` when set |
| `LIVE_NETSUITE_OTHER_CURRENCY_ID` | L6 | currency the vendor is not set up for |

Commands (one per gate; `--nocapture` shows the observed blocks). If the local database is behind the latest migrations, the sqlx macros fail to compile; export `SQLX_OFFLINE=true` to build from the committed `.sqlx` cache:

```bash
cargo test -p verzo-360-api --lib live_l1 -- --ignored --nocapture
```
```bash
cargo test -p verzo-360-api --lib live_l2 -- --ignored --nocapture
```
```bash
cargo test -p verzo-360-api --lib live_l3 -- --ignored --nocapture
```
```bash
cargo test -p verzo-360-api --lib live_l4 -- --ignored --nocapture
```
```bash
cargo test -p verzo-360-api --lib live_l5 -- --ignored --nocapture
```
```bash
cargo test -p verzo-360-api --lib live_l6 -- --ignored --nocapture
```
All six:
```bash
cargo test -p verzo-360-api --lib live_ -- --ignored --nocapture --test-threads=1
```

What each test asserts (documented) and only observes (docs silent):

| Gate | Asserts | Observes (printed) |
|---|---|---|
| L1 | all Odoo capability checks pass; company has `two_step` + amount 0; `list_changed` / `list_eligible_ids` succeed; the RFQ is read with ≥ 1 line | capability checks, controls, `write_date` cursor values, eligible ids, the parsed RFQ (many2one shape, decimals, totals) |
| L2 | create → 204 + `Location` with an id; `q=externalId IS` finds that id within 5 tries | attempts needed (read-after-write), `GET purchaseOrder/eid:…`, **N3**: a second POST with the same `externalId` |
| L3 | full create succeeds; read-back has the one item line | raw GET body; create **without location** |
| L4 | create succeeds; GET succeeds; `approvalStatus` key present (else **FAIL**, never skip) | `approvalStatus`, `nextApprover`, `supervisorApproval`, `status`, `orderStatus` after REST create |
| L5 | token + all NetSuite capability reads pass on the role | capability checks; `GET governanceLimits` as non-admin (REST permission-error shape, N6) |
| L6 | nothing (docs silent) | create with a currency the vendor is not set up for |

**Cleanup.** The adapters have no delete, so tests never delete. Each created PO prints `CLEANUP Lx: purchaseOrder internal id …, external id VERZO-SMOKE-…; delete manually`. If L2's second POST creates a PO, it prints `DUPLICATE CREATED — delete manually` with both ids. Delete them in the NetSuite UI after recording results.

**Secrets in output.** All printing goes through one redacting helper: the Odoo key, the private key (whole and per line) and any JSON value under `access_token`, `client_assertion`, `authorization`, `api_key`, `private_key` are replaced by `[REDACTED]`. Tokens and JWT assertions are never handled by the test code. The in-crate test `printed_output_never_holds_secrets` guards this.

## 5. Recording results

1. Copy each `=== OBSERVED Lx: … ===` block into the matching facts pack:
   - L1 → [facts/odoo-19.md](facts/odoo-19.md), under the item it answers (O1 cursor precision, many2one shape, UNVERIFIED list items 2–4, 8, 10).
   - L2–L6 → [facts/netsuite-rest.md](facts/netsuite-rest.md), under N1–N6 / JWT, and in its UNVERIFIED list.
2. For each item now observed, change **UNVERIFIED** to **OBSERVED (live, `<date>`, `<account id or Odoo db>`)** and paste the block. Keep the doc citation; do not delete the documented contract.
3. Record any role permission, Odoo group or setup step you had to add or guess, next to the step in this file that said UNVERIFIED.
4. In [README.md](README.md), set the gate's Status to `passed`, fill Run date, and list the facts lines updated. A gate whose test failed stays `open` with a one-line reason.
5. If an observation contradicts what the adapter does (for example N3 creates a duplicate, or `approvalStatus` ignores routing), open a fix through the parent session; do not change adapters as part of recording.
