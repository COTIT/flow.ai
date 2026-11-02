# Deferred third-party providers

Decision (2026-10-31): third-party providers (payments, email, shipping, tax, marketplaces) stay out of the build for now. Update (2026-11-02): payments = Stripe and email = Resend are built, waiting for keys; shipping and tax stay deferred. We build the provider-neutral parts first, so adding a provider later means writing one adapter file, its config and its tests.

Each entry: what is ready, what is left, where to start.

## Payments: Stripe (chosen 2026-11-02)

- **Built:** `src/payments/stripe.rs` (Checkout Sessions, refunds, signed webhooks on `POST /v1/payments/webhooks/stripe`), config `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, `STRIPE_API_URL` (loopback fake only), migration `payments_stripe`, tests against a local fake Stripe (`tests/payments_stripe.rs`). Contract: `docs/api/payments.md`.
- **Deploy:** `deploy/*/docker-compose.stripe.yml` overrides plus `deploy/set-provider-keys.sh`; each deploy README has "Turning on Stripe".
- **Left (user):** create the Stripe account, put the keys in with the script, add the dashboard webhook endpoint. One platform account for every workspace for now.
- **Later:** Stripe Connect when a second merchant sells (a `Stripe-Account` per workspace); Stripe's special currencies (HUF, TWD, ISK, UGX) if ever sold in.

## Payments: US buyers (gate G8)

- **Ready:** the same interface; Stripe covers US cards too.
- **Left:** choose the provider (business call), write the adapter variant, and decide the US sales tax approach (provider or table).

## Payments: PAY.nl terminals (till, F16)

- **Prepared (phase 2b):** a till sale takes `payment: {method: "card", reference?}`: the cashier runs any external terminal and records the slip reference by hand. `till_sales.method` and `card_reference` hold it; no rounding, no drawer row.
- **Left:** only needed if an EU till customer needs card terminals. Reference: Control POS terminal polling.

## Email delivery: Resend (chosen 2026-11-02)

- **Ready:** no code. Resend is SMTP through the existing `SMTP_URL`: `smtps://resend:<API key>@smtp.resend.com:465` (unit tests in `config.rs` and `mail/mod.rs` show it passes the TLS check and builds a mailer). Production only; staging stays on Mailpit. Per-workspace SMTP and Microsoft Graph accounts in `mail/` are unchanged.
- **Left (user):** verify the sending domain in the Resend dashboard and add its DNS records, then write the key with `deploy/set-provider-keys.sh production resend` (deploy/production/README.md "Turning on Resend").
- **Later:** deliverability events (bounces, complaints; F20), which need an inbound webhook like the payment inbox; "Forgot your cart" emails (Phase 5).

## Shipping: Shippo (Phase 5)

- **Left:** labels and tracking, an adapter behind a `shipping` interface, and a webhook through the same inbox pattern.

## Amazon FBA (Phase 5)

- **Left:** FBA as its own stock location, plus an inventory sync adapter.
