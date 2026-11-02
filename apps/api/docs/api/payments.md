# Payments

Provider payments of web orders. Code: `src/payments/` (provider-neutral interface, the fake and Stripe), `src/modules/orders/{checkout,payments,webhook}.rs` (the machinery).

## Configuration

`PAYMENTS_PROVIDER`: unset = none (payment intents answer 503 `no payment provider configured`); `fake` = the in-process test provider, debug builds only (a release build refuses to boot with it); `stripe` = Stripe Checkout. Unknown names fail at boot.

Stripe settings, read only when `PAYMENTS_PROVIDER=stripe` (each secret also as `<KEY>_FILE`, a Docker secret). Boot fails naming the var, never the value:

| Var                     | Rule                                                                                                                                                                           |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `STRIPE_SECRET_KEY`     | `sk_test_…`, `sk_live_…` or a restricted `rk_…` key. Live keys are refused in builds with debug assertions (dev, tests, the staging image), so staging can only run test mode. |
| `STRIPE_WEBHOOK_SECRET` | The dashboard endpoint's signing secret, `whsec_…`.                                                                                                                            |
| `STRIPE_API_URL`        | Optional. Default `https://api.stripe.com`; an override must be a loopback URL (`127.0.0.1`, `::1`, `localhost`): a local fake for tests.                                      |

## Stripe

One platform Stripe account for every workspace; the workspace travels in metadata. Stripe Connect (a `Stripe-Account` per workspace) can be added later without changing the interface.

- **Payment** = a Checkout Session (`POST /v1/checkout/sessions`, Stripe's hosted page; `checkout_url` is its `url`). One line item `Order <number>` of the order total in minor units, currency in lower case, `success_url` and `cancel_url` both the shop's `redirect_url`, `metadata` and `payment_intent_data[metadata]` = `{workspace_id, order_id, payment_id}`, `Idempotency-Key` = our row id, `Stripe-Version: 2024-06-20`.
- **Expiry:** the session's `expires_at` is the order's `expires_at` moved into Stripe's window (31 minutes to 24 hours ahead). Our hold is 30 minutes, so our `order_expire` job fires first and Stripe's session closes up to about a minute later (more when the shop starts the payment late in the hold). A payment in that gap goes the late-payment way: the order reopens if its stock is still there, else one automatic refund.
- **Status:** `open` → open; `complete` + `paid` (or `no_payment_required`) → paid; `complete` + `unpaid` → open (async methods such as SEPA debit); `expired` → expired. A session whose metadata names another workspace is "not found".
- **Refund** = `POST /v1/refunds` on the session's `payment_intent`, with `metadata[workspace_id]` and `metadata[checkout_session]`; `pending`/`requires_action` → open, `succeeded` → refunded, `failed`, `canceled`.
- **Errors:** 404 → not found; 409, 429, 5xx, timeout (20 s) or no connection → unavailable (retried); any other 4xx → refused, with Stripe's message.
- **Webhook:** one dashboard endpoint, `POST /v1/payments/webhooks/stripe`, events `checkout.session.completed`, `checkout.session.async_payment_succeeded`, `checkout.session.async_payment_failed`, `checkout.session.expired`, `refund.updated`, `refund.failed`. The `Stripe-Signature` header (HMAC-SHA256 of `"{t}.{raw body}"` with the signing secret, any `v1`, `t` within 5 minutes) is checked before the body is parsed. Then only `data.object.id` of a `checkout.session` or `refund` is read.
- **Currencies:** Stripe's minor units match ours except HUF, TWD, ISK and UGX (Stripe's special cases); check before a shop sells in one of them.

## Flow

1. **Checkout** (`POST /storefront/orders`): the order holds its stock, `awaiting_payment`, for 30 minutes.
2. **Intent** (`POST /storefront/orders/{order}/payment`): an `order_payments` row is committed as `creating` (with the shop's `redirect_url` and our `webhook_url`), then the provider's create is called outside any transaction with the row id as idempotency key. Success stores the provider id and checkout URL (`open`); an outage leaves the row `creating` (503, retry).
3. **Webhook** (no auth, IP rate limit, 64 KiB): the body only names an id; a signing provider (Stripe) checks the signature first.
   - `POST /payments/webhooks/{provider}/{workspace}`: a known payment or refund id of that workspace.
   - `POST /payments/webhooks/{provider}`: for providers with one endpoint for every workspace (Stripe). The workspace comes from our own `order_payments` row for that provider id, never from the body.

   A known id at the configured provider is queued in `payment_webhook_inbox` (one pending row per id, so repeats coalesce); anything else is ignored. The answer is always an empty 200, so it never tells which workspaces or ids exist and a bad event is never retried by the provider.

4. **Drain** (job `payment_inbox`, one per workspace): takes the oldest pending row, re-fetches the payment from the provider (outside any transaction) and rejects it when the provider doesn't know it or its amount, currency or metadata (workspace, order, payment) differ from our row. Otherwise it applies the status under the order lock, then the payment row's lock:
   - `paid`: the order gets `paid_at`; `awaiting_payment` → `open`. An order the expiry already cancelled re-holds its stock and opens, or, when the stock is gone, stays cancelled and gets one automatic full refund (`auto`, unique per payment).
   - `failed`, `expired`, `canceled`: stored; the shop may start a new intent.
   - A paid or refunded row never moves again; `open` changes nothing.
   - The inbox row closes only if no new webhook for it arrived during the fetch; a provider outage leaves it pending (given up after 10 tries).
5. **Sweep** (job `payment_sweep`, every minute): retries creates a dead request left behind (system refunds at once, others after a minute, each with its row id as idempotency key, so the provider returns the same payment; given up after an hour), queues a re-fetch of payments and refunds left `open` for 15 minutes (up to 2 days old), and re-drives workspaces with pending inbox rows.
6. **Expiry** (job `order_expire`): unpaid web orders past `expires_at` are cancelled and give their stock back.

## Refunds

`POST /workspace/{workspace}/orders/{order}/refunds` (`order.delete`, Idempotency-Key): `{amount}` in minor units, 1 to paid − refunds not failed. The refund row commits as `creating`, then the provider is called; a failure is left to the sweep. Its status follows by webhook like a payment. A paid order cancels only once fully refunded (else 409 `refund_required`).

## Adding a provider

A provider is one `Provider` variant plus one file in `src/payments/`. It must:

1. **Create** (`create(&NewIntent)`): start a payment of `amount` minor units in `currency` (use `decimals` for providers that want `"12.50"`), send `idempotency_key` so a retried create returns the same payment, pass `metadata` through, return the provider id, status and checkout URL.
2. **Fetch** (`fetch(workspace_id, id)`) and **fetch_refund** (`workspace_id, payment_id, refund_id`): the current state, with amount, currency and the metadata echoed back. Map the provider's words onto `Status` (`Open`, `Paid`, `Refunded`, `Failed`, `Expired`, `Canceled`).
3. **Refund** (`refund(&NewRefund)`): refund part or all of a paid payment, with the idempotency key.
4. **Parse its webhook** (`webhook_id(headers, body)`): check the signature when the provider signs, then return the payment or refund id the body names, nothing else.
5. Classify errors: `Unavailable` (timeouts, 5xx, rate limits: retried), `Refused` (bad request or credentials: not retried), `NotFound`.
6. Keep keys out of logs and `Debug`, and use the outbound client (`src/outbound.rs`: address guard, timeouts, no redirects, no proxy). Per-workspace keys would use the sealed credentials (`settings/credentials.rs`) per call.
7. Widen the `provider` CHECKs of `order_payments` and `payment_webhook_inbox` in its own migration (Stripe's: `payments_stripe`, which also allows ids up to 255 characters), and add its name to `payments::select`.

Tests it must pass (run the existing payment suites with the provider faked at its HTTP edge): the same webhook 5× gives one paid order; a "paid" claim the provider doesn't confirm changes nothing; a mismatched amount is rejected; a request killed after the provider call is recovered once by the sweep; a late payment re-holds stock or is refunded exactly once; refunds respect what is left; an invoice books paid − refunded.
