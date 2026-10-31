# Payments

Provider payments of web orders. Code: `src/payments/` (provider-neutral interface and adapters), `src/modules/orders/{checkout,payments,webhook}.rs` (the machinery). No real provider is built yet: see `docs/deferred-providers.md`.

## Configuration

`PAYMENTS_PROVIDER`: unset = none (payment intents answer 503 `no payment provider configured`); `fake` = the in-process test provider, debug builds only (a release build refuses to boot with it). Unknown names fail at boot.

## Flow

1. **Checkout** (`POST /storefront/orders`): the order holds its stock, `awaiting_payment`, for 30 minutes.
2. **Intent** (`POST /storefront/orders/{order}/payment`): an `order_payments` row is committed as `creating` (with the shop's `redirect_url` and our `webhook_url`), then the provider's create is called outside any transaction with the row id as idempotency key. Success stores the provider id and checkout URL (`open`); an outage leaves the row `creating` (503, retry).
3. **Webhook** (`POST /payments/webhooks/{provider}/{workspace}`, no auth, IP rate limit, 1 KiB): the body only names an id. A known payment or refund id of that workspace at the configured provider is queued in `payment_webhook_inbox` (one pending row per id, so repeats coalesce); anything else is ignored. The answer is always an empty 200, so it never tells which workspaces or ids exist.
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
4. **Parse its webhook** (`webhook_id(body)`): the payment or refund id the body names, nothing else.
5. Classify errors: `Unavailable` (timeouts, 5xx, rate limits: retried), `Refused` (bad request or credentials: not retried), `NotFound`.
6. Use the workspace's sealed credentials (`settings/credentials.rs`) per call, never cached or logged, and the outbound client (`src/outbound.rs`: address guard, timeouts, no redirects, no proxy).
7. Widen the `provider` CHECKs of `order_payments` and `payment_webhook_inbox` in its own migration, and add its name to `payments::select`.

Tests it must pass (run the existing payment suites with the provider faked at its HTTP edge): the same webhook 5× gives one paid order; a "paid" claim the provider doesn't confirm changes nothing; a mismatched amount is rejected; a request killed after the provider call is recovered once by the sweep; a late payment re-holds stock or is refunded exactly once; refunds respect what is left; an invoice books paid − refunded.
