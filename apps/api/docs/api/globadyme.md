# Globadyme hosted payments

Globadyme supports shop checkout and payment links for sent invoices. Payments use the workspace's selected sandbox or production account. A historical operation keeps its workspace, environment and merchant binding after settings change. Existing Stripe/fake operations retain their configured adapter.

Production collection is disabled: the published contract lists only `https://api.sbx.globadyme.com/api`. An official production origin must be confirmed and added to the server's approved origins before production can run. Workspace settings cannot supply an API host. The current currency gate requires two decimal places.

## Account setup

Incomplete setup keeps new collections disabled. In **Settings → Payments**, select the environment and save its merchant ID, username, password and callback secret. The warning lists fields missing from that saved environment; masked credentials count as stored, and the previous callback secret is optional. Field presence does not verify the provider login. Shop and public invoice pages show online payment as unavailable until local setup permits it; bank details remain visible on invoices. Existing live payments keep their original account binding after new collections are disabled or another provider is selected.

Configure the server's existing `CREDENTIALS_KEY` before saving credentials. Open **Settings → Payments**, select Globadyme and sandbox, then enter the merchant ID, login username, password and dashboard callback secret. Configure this HTTPS callback in the merchant dashboard:

```text
https://<api>/v1/payments/webhooks/globadyme/<workspace-uuid>/sandbox
```

The settings tree is:

```text
integrations.payment.active_provider = globadyme
integrations.payment.enabled = true
integrations.payment.environment = sandbox
integrations.payment.payment_providers.globadyme.sandbox.merchant_id
integrations.payment.payment_providers.globadyme.sandbox.username
integrations.payment.payment_providers.globadyme.sandbox.password
integrations.payment.payment_providers.globadyme.sandbox.callback_secret
integrations.payment.payment_providers.globadyme.sandbox.previous_callback_secret
```

Production has the same five fields under `.production`. Username, password and both callback secrets are sealed in `workspace_credentials`; merchant metadata lives in `payment_accounts`. GET masks stored credentials with `********`. Send that mask back to retain a credential when replacing the integrations section. Saving an unrelated integrations field must preserve the rest of that section.

Each workspace has two fixed account slots. A merchant may belong to only one workspace in each environment. First use freezes merchant, environment and login principal. Password rotation keeps the same principal and increments the credential revision. Used secrets cannot be cleared through section replacement. GET cannot verify merchant identity because the provider's request lookup omits merchant ID.

Replacing a callback secret retains its predecessor. A second rotation fails until an administrator retires the predecessor with a reason and provider evidence:

```text
POST /v1/workspace/{workspace}/payments/globadyme/{environment}/callback-secret/retire
{"reason":"...","evidence":"provider support or dashboard reference"}
```

Retirement removes the previous secret immediately. Confirm the provider has stopped signing callbacks with it before retirement. Credentials and callback bodies must stay out of tickets and logs.

## Collection routes

Shop clients keep the existing checkout, payment-intent and order-status routes. The order's customer snapshot supplies payer name, email and ISO country. Globadyme requires a payer name and a real ISO 3166-1 alpha-2 country. The shop returns `payment_status` and `payment_attention` after a gateway collection starts. Unknown submissions remain reserved and show a confirmation/review state.

Invoice emails and PDFs keep `/clients/pay-for-invoice/{token}`. These token-authorized routes share the existing public limiter and token revocation rules:

```text
POST /v1/invoices/fetch-from-hashed/{token}/payment
{"country":"NL","payer_name":"Example Customer","payer_email":"customer@example.test"}
GET  /v1/invoices/fetch-from-hashed/{token}/payment
GET  /v1/invoices/fetch-from-hashed/{token}/payment/qr
```

POST snapshots the exact outstanding invoice balance under its lock. One live collection is allowed. The browser cannot choose the amount. GET returns `data: null` before a collection exists. QR returns `image/png` with `Cache-Control: no-store`; the server authenticates to the provider's fixed QR endpoint. Only open collections without reconciliation warnings expose payment links or QR. A remaining invoice balance after capture starts a fresh collection. The invoice page preserves bank-transfer details.

Payment views include `payment_id`, `status`, integer `amount`, exact integer-text `amount_minor`, `currency`, stored `decimals`, `checkout_url` and attention fields. Use `amount_minor` for exact browser display. No credential is returned. Invoice tokens are rebuilt from their sealed copy when a return URL is needed; payment attempts never store the plaintext token.

## Refunds and accounting

An order without an invoice uses its existing order-refund endpoint. Once an order has an invoice, create and send a source-linked credit note, then refund through that note:

```text
POST /v1/workspace/{workspace}/invoices/{credit-note}/refunds
Idempotency-Key: <unique-request-key>
{"payment_id":"<local-source-collection-uuid>","amount":"2.00"}
```

The caller needs `invoice.credit`. The note must be sent, pending, linked to the source invoice, and match its workspace, customer, currency and stored decimal places. This includes invoices converted from shop orders. The source must have one verified captured transaction without ambiguity.

The amount must fit both the note's remaining credit and the captured payment less confirmed and pending refunds. Unknown refunds reserve both budgets. Submission does not mark the note credited. A confirmed refund appends one negative note ledger entry using the local refund UUID, then rebalances atomically. Partial refunds leave the note pending; a full refund makes it credited. The original invoice receipt remains.

Manual settle, mark-as-credited and void actions cannot consume money reserved by an outstanding refund. Confirmed Globadyme receipts cannot be voided, including receipts created when converting a paid order to an invoice. A valid late collection still books after a manual payment or credit; overpayment needs operator attention.

## Recovery and reconciliation

The submission claim commits before a financial POST. Globadyme documents no payment/refund deduplication guarantee, and its refund response has no refund ID or lookup route. A timeout, disconnect or malformed success therefore leaves an unknown operation. Recovery never repeats that POST. Payment recovery uses GET by the stable request UUID; an empty result does not prove nonacceptance.

`NEW`, `CREATED`, `PENDING` and `AUTHORIZED` do not book money. One exact `PROCESSED` transaction confirms capture. Multiple successes or conflicting request/customer/money data preserve evidence and block unsafe compensation. Accepted receipts stay fixed while settlement, external refund and chargeback observations continue to refresh. Ordinary local confirmed partial refunds remain refundable within the remaining local cap; the provider can reject a request above its own remaining balance.

Use **Payments** to inspect workspace operations. List access requires the owner's `order.list` or `invoice.list`; transaction details and refresh require the matching `.view` permission. The sweep continues after the collection switch is disabled, including refunds and chargebacks on older paid transactions. Account/page cursors let bounded jobs eventually scan all used accounts and chargeback pages.

An administrator may resolve an unknown submission only with affirmative provider evidence. An open collection can be closed only after the provider confirms it cannot produce a charge; link expiry alone is insufficient:

```text
POST /v1/workspace/{workspace}/payments/{payment}/resolve
{"outcome":"not_accepted","reason":"...","evidence":"provider case/reference"}
```

`refund_confirmed` applies only to an unknown refund whose exact source transaction and amount the provider confirmed. Audit, status and credit-note receipt commit together. A conflicting resolution returns 409. Collection capture is always confirmed through authoritative lookup. Record unexpected real money and escalate conflicting evidence instead of changing receipts to hide it.

Gateway financial and administrator actions require the explicit payment interface; autonomous voice execution is refused.

## Sandbox smoke check

Use sandbox credentials supplied by the provider. Create a small two-decimal shop checkout and a sent invoice. Confirm each hosted link and QR opens the intended amount. Complete one payment, replay its signed callback, and verify one capture/receipt. Exercise a partial credit-note refund followed by its remainder; verify the source receipt remains and the note changes to credited only after confirmation. Rotate the callback secret, verify both signatures during overlap, then retire the predecessor with audit evidence.

Disconnect a test submission after the provider accepts it. Confirm it remains unknown and recovery makes GET requests without another financial POST. Disable new collections and verify historical reconciliation and refunds still run. Repeat tenant/environment isolation checks before enabling another account.

Loopback tests prove local contract behavior only. Real sandbox and live collection remain unproven without provider credentials. No recurring billing, manual capture, direct card handling, split payouts or till-terminal redesign is included.

Provider references: [OpenAPI 1.44.1.6](https://api-documentation.globadyme.com/openapi/globadyme-gateway-1.44.1.6.yaml), [authentication](https://api-documentation.globadyme.com/getting-started/), [callbacks](https://api-documentation.globadyme.com/integration-documentation/), [payment links and QR](https://api-documentation.globadyme.com/payment-link/).
