# Deferred third-party providers

Decision (2026-10-31): third-party providers (payments, email, shipping, tax, marketplaces) stay out of the build for now. We build the provider-neutral parts first, so adding a provider later means writing one adapter file, its config and its tests.

Each entry: what is ready, what is left, where to start.

## Payments: Mollie (EU)

- **Ready (Phase 2a):**
  - `payments` module with a provider-neutral interface (`create`, `fetch`, `refund`).
  - `order_payments` table, payment webhook inbox, drain job (re-fetches from the provider, never trusts the payload), recovery sweep, expiry job.
  - A dev/test-only fake provider. It is refused in release builds.
  - The route `POST /v1/payments/webhooks/{provider}/{workspace}`.
- **Left:**
  - A `mollie` variant plus its client: amounts from minor units, idempotency key, refunds.
  - Config `MOLLIE_API_URL` (loopback-only override for tests) and `MOLLIE_TEST_MODE`.
  - A per-workspace sealed API key at the settings path `integrations.payment.payment_providers.mollie.api_key`.
  - A fake Mollie HTTP server for tests.
- **Start from:**
  - In the old repo, branch `claude/genesy-f8a-mollie`, local commit `0f4172fe8` (not pushed, not reviewed). Look at `src/mollie.rs`, `tests/common/fake_mollie.rs` and `docs/api/mollie.md`.
  - Control: `vimdesk-api/src/Infrastructure/Integrations/PaymentProviders`.

## Payments: US buyers (gate G8)

- **Ready:** the same interface as Mollie.
- **Left:** choose the provider (business call), write the adapter variant, and decide the US sales tax approach (provider or table).

## Payments: PAY.nl terminals (till, F16)

- **Left:** only needed if an EU till customer needs card terminals. Reference: Control POS terminal polling.

## Email delivery (production)

- **Ready:** SMTP sending through `SMTP_URL` with TLS enforced, and Mailpit locally. Per-workspace SMTP and Microsoft Graph accounts exist in `mail/`.
- **Left:**
  - Choose the production provider (Postmark or Resend) and create a sandbox server for previews.
  - Deliverability events (bounces, complaints; F20 Resend events), which need an inbound webhook like the payment inbox.
  - DNS records (SPF, DKIM, DMARC).
  - "Forgot your cart" emails (Phase 5).

## Shipping: Shippo (Phase 5)

- **Left:** labels and tracking, an adapter behind a `shipping` interface, and a webhook through the same inbox pattern.

## Amazon FBA (Phase 5)

- **Left:** FBA as its own stock location, plus an inventory sync adapter.
