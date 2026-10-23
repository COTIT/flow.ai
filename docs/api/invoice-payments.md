# Invoice payments API

Payments on sent invoices, settles against credit notes and mark-as-credited. Ported from vimdesk-api `src/Domain/Finance/Invoice/V1` (StorePaymentRequest, CreateInvoicePayment, DeleteInvoicePayment, UpdateInvoiceBalanceAfterPayment, MarkInvoiceAsCredited, InvoicePaymentResource). Mollie payments and refunds come in F8.

Every route sits under `/v1/workspace/{workspace}/invoices/{invoice}`. An unknown, malformed or other workspace's invoice is a 404.

## Routes

| Route | Permission | Answer |
|---|---|---|
| `POST /payments` | `invoicepayment.create` | `201 {"data": {"invoice_payment_id": "<uuid>"}}` |
| `DELETE /payments/{payment}` | `invoicepayment.delete` | `{"message": "ok"}` (voids it) |
| `POST /mark-as-credited` | `invoice.credit` | `{"message": "ok"}` |

Not ported:
- `GET` and `POST /payments/{payment}` (show, update): the SPA doesn't call them, and payments are never rewritten.
- `fetch-provider-status`: Mollie, F8.

Payments are listed in the invoice detail (`relationships.payments`, `invoices.md`).

## The ledger

- A payment is stored once, in minor units of the invoice's currency at the decimals the invoice was priced with (`summary.dp`, frozen once sent).
- Refunds and the credit-note side of a settle are negative.
- A payment is never deleted or changed: `DELETE` sets `voided_at` / `voided_by`, and a voided payment drops out of every sum and list.
- The database refuses (SQLSTATE 23001), whoever asks:
  - a `DELETE`;
  - a change of id, amount, invoice, workspace, method, date, settle twin or `created_at`;
  - turning a paid payment unpaid (void it instead);
  - undoing a void;
  - `TRUNCATE` while any payment exists.
- Status, a pending payment turning paid, and provider fields stay writable for Mollie (F8). Keeping `paid_total` in step with them is up to the writers, which recompute it under the invoice lock.
- Laravel soft-deleted payments and let the update route change the amount.

## The invoice's balance

`invoices.paid_total` is the sum of the invoice's paid, unvoided payments. Every payment write recomputes it from the rows, never by adding to it, in the write's transaction and under the invoice's row lock. The status then follows Laravel's UpdateInvoiceBalanceAfterPayment.

**Invoice**:

| When | Status |
|---|---|
| total is 0 | `paid` |
| paid > 0 and paid ≥ total | `paid` (an overpayment too, as Laravel; `balance.open` goes negative) |
| paid = 0 | `pending` |
| paid < total | `partially_paid` |

**Credit note** (`is_credit`): `credited` when paid ≤ total (both negative: as much went back as the note is worth), else `pending`.

- **Cash rounding:** when the invoice has a paid cash payment, paid and total are compared at the nearest 0.05 (Laravel MoneyHelper::nearestAmount: half away from zero; no rounding below 2 decimals). Otherwise they are compared exactly. Laravel rounded for every payment, so a 99.98 bank transfer on a 100.00 invoice marked it paid.
- **Only these statuses move:** `pending`, `partially_paid` and `paid` (credit notes: `pending` and `credited`). Laravel also set a cancelled or draft invoice back to `pending` when a payment was deleted.

### Worked example (cents)

| Step | Rows (amount, method) | paid_total | Status | `balance.open` |
|---|---|---|---|---|
| Send an invoice of EUR 100.00 | none | 0 | `pending` | 100 |
| Pay 60.00 cash | 6000 cash | 6000 | `partially_paid` | 40 |
| Pay 39.98 cash | 6000 cash, 3998 cash | 9998 | `paid` (9998 → 10000 at the nearest 5 cents) | 0.02 |
| Same two by transfer | 6000 manual, 3998 provider | 9998 | `partially_paid` (exact) | 0.02 |
| Settle 30.00 with a credit note of −30.00 | invoice +3000 settled, note −3000 settled | invoice +3000, note −3000 | invoice `partially_paid`, note `credited` | the two rows sum to 0 |
| Void either side of the settle | both voided | 0 and 0 | `pending` and `pending` | |

## Create `POST /payments`

| Field | Rule |
|---|---|
| `date` | required: `Y-m-d`, `Y-m-d H:i:s` or RFC 3339 (`The date is not a valid date.`) |
| `amount` | required number > 0 (`The amount must be greater than 0.`; Laravel allowed 0), with at most the currency's decimals (`The amount must have at most 2 decimal places.`) |
| `payment_method` | required without `settle_invoice_id` (`The payment method field is required when settle invoice id is empty.`): `cash`, `pin`, `manual`, `mollie` or `bank`, any case (`The selected payment method is invalid.`; Laravel gave a 500 for unknown values) |
| `description` | string ≤ 180 |
| `provider_id` | string ≤ 255 |
| `settle_invoice_id` | uuid of a credit note to settle with (below) |
| `meta` | object or null (`The meta must be an array.`; Laravel stored anything), at most 8 KiB of JSON (`The meta is too large.`) |

- **Stored:**
  - `cash`, `pin` and `manual` as such;
  - `mollie` as method `provider` with provider `mollie`;
  - `bank` as method `provider` with provider `afas` (Laravel's alias, kept because the SPA shows its delete button only for `afas`).
  - Every payment is `is_paid`, status `paid`, by the caller.
- **At most 500 live payments** per invoice: `422 {"message": "An invoice holds at most 500 payments."}`.
- **Only an open invoice:** `pending` or `partially_paid`. Anything else is `422 {"message": "Invoice must be pending or partially paid to create a payment"}`.
  - A credit note gets `422 {"message": "A credit note is refunded with mark-as-credited or a settle, not a payment."}`.
  - Laravel let `pin` skip every check; the till keeps that path in F15.

### Settle (`settle_invoice_id`)

Pays the invoice with a credit note. Two rows are written in one statement, each pointing at the other:
- a `settled` payment of `amount` on the invoice, with the body's description;
- a `settled` payment of −`amount` on the credit note, with "Payment for invoice {number}".

Both invoices are recomputed.

- The settle id must name a live invoice of this workspace: `The selected settle invoice id is invalid.` on `settle_invoice_id`.
- **New checks:** it must be a sent, `pending` credit note of the same relation, currency and decimals (`The settle invoice id must be a pending credit note of the same relation and currency.`). Laravel checked none of this. Settling an invoice with itself is `The selected settle invoice id is invalid.`.
- The amount may not be more than the credit note still has open, nor more than the invoice has open: `422 {"message": "The payment amount is invalid"}`. A plain payment may overpay (Laravel); a settle never spends credit beyond what the invoice owes.

## Void `DELETE /payments/{payment}`

- Voids the payment. A settle payment voids its twin with it, and both invoices are recomputed.
- Only a live payment of **this** invoice is found: any other id (another invoice's, a voided one, unknown) is a 404. Laravel looked payments up across the whole workspace.

## Mark as credited `POST /mark-as-credited`

| Field | Rule |
|---|---|
| `date` | required, as above |
| `payment_method` | required: `cash`, `pin`, `bank`, `mollie`, `manual` |
| `description` | string ≤ 180; default "Refunded payment" |
| `provider_id` | string ≤ 255; also stored as `meta.transaction_id` (Laravel) |
| `meta` | object or null |

- **Credit note `pending`:** writes a payment of its open balance (negative), then sets `credited`.
- **Already `credited`:** a quiet `{"message": "ok"}` that writes nothing (Laravel).
- **Anything else:** `400 {"message": "Only a pending credit note can be marked as credited."}`. Laravel answered 400 with a raw translation key.
- **Not ported yet:** Laravel's side effects on orders and tills in `pending_refund` (F12, F15).

## Idempotency-Key

The SPA sends an `Idempotency-Key` header on every POST and DELETE and reuses it on retry. All three routes honour it:

- A key is 1 to 255 visible characters (else `422 {"message": "The Idempotency-Key header must be 1 to 255 visible characters."}`). It is scoped to the workspace, bound to the member who used it first and to the method, path and raw body.
- **First use:** the answer (status and body) is stored by the same transaction as the write. A write that fails stores nothing, so a retry runs again.
- **Same key, same request:** the stored answer is sent again byte for byte, and nothing is written.
- **Same key, another request** (another body or endpoint, or another member): `422 {"message": "Idempotency-Key reused with different request payload"}` (Laravel's text). Another member never gets someone else's answer.
- **Copies in parallel:** they wait on the key until the first commits, then replay its answer.
- Keys are pruned after a day by the hourly cleanup. The body is read once; the JSON content-type check, the body limit and the 422s are those of every other route.
- No header: no dedupe.
- Laravel's middleware was wired to these routes but switched off, and keyed its cache globally.

## Payment resource (`relationships.payments`)

Unvoided, by date then entry:

```json
{ "id": "…", "description": "…", "date": "2026-10-23T00:00:00.000000Z", "amount": "60.00000000",
  "paid_amount": null, "is_paid": 1, "provider": null, "provider_id": null, "status": "paid",
  "payment_method": "cash", "created_at": "…", "last_updated_at": "…", "meta": null }
```

- `amount` prints Laravel's decimal(40,8) column: 8 places.
- `paid_amount` is null: Laravel set it only on till and settle rows.

## Concurrency

- **Lock order:** the Idempotency-Key row, then the invoice rows the write touches, in id order (`FOR NO KEY UPDATE`), then the ledger rows, then each invoice's paid total and status.
- F6's send and credit also take their invoice row first, so payments, sends and credits of one invoice queue. Two settles over the same pair never deadlock.
- **Tested:**
  - 20 payments of 5.00 at once on 100.00 all count, and the invoice ends `paid`;
  - payments and voids racing on one invoice leave the sum of what stays;
  - six settles against one credit note at once: only one fits;
  - five copies of one key give one payment and five identical answers.

## Not ported yet

- `InvoicePaid` side effects:
  - loyalty earning (F11);
  - order status (F12);
  - the cash register (F15).
- Activity log and workflows (batch 7).
