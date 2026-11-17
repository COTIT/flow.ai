# Shipments

Flow ERP M4 (masterplan §12 PARTIAL SHIPMENTS / RECOGNITION, §15): ship an open sales order in parts. Base `/v1/workspace/{workspace}`, tag `shipments`, migration `sales_recognition`.

A draft plans quantities and holds no stock of its own; the order's confirm hold stays. Posting a draft takes exactly its quantities off that hold (stock lines), adds them to the order lines' delivered (`deducted`) quantity and, in an active book, recognises the sale. Picked is not delivered: only a post moves stock.

## Envelopes and replay

Writes take the accounting envelopes (`docs/api/accounting.md`) and a required `Idempotency-Key` header (missing → 422):

- C (create): `{client_ref: uuid, payload}` → 201.
- A (action): `{client_ref: uuid, expected_version: integer ≥ 1, payload}` → 200.

The same `client_ref` and body replays the committed answer for good (no second stock movement or journal); another body under it is 409. A version that moved, or a review fingerprint that no longer matches, is 409 `stale_document`.

## Permissions

| Operation | Route | Permission |
|---|---|---|
| `shipments_list` | `GET /orders/{order}/shipments?page=&page_size=` | `shipment.list` |
| `shipments_create` | `POST /orders/{order}/shipments` | `shipment.create` |
| `shipments_get` | `GET /orders/{order}/shipments/{shipment}` | `shipment.get` |
| `shipments_update` | `PATCH /orders/{order}/shipments/{shipment}` | `shipment.update` |
| `shipments_post_review` | `POST /orders/{order}/shipments/{shipment}/post/review` | `shipment.post` |
| `shipments_post` | `POST /orders/{order}/shipments/{shipment}/post` | `shipment.post` |
| `shipments_cancel` | `POST /orders/{order}/shipments/{shipment}/cancel` | `shipment.cancel` |
| `shipments_invoice` | `POST /orders/{order}/shipments/{shipment}/invoice` | `shipment.invoice` |
| `invoices_issue_review` | `POST /invoices/{invoice}/issue/review` | `invoice.send` |
| `invoices_issue` | `POST /invoices/{invoice}/issue` | `invoice.send` |

An order or shipment of another workspace, or a shipment under another order, is 404.

## Payloads

Create and update (update replaces the draft's lines and note):

```json
{ "lines": [{ "order_line_id": "uuid", "quantity": "4" }], "note": "Van 2" }
```

- 1-100 lines, each order line once, quantity > 0 with at most 2 decimals.
- Capacity, checked under the order lock: per order line, Σ draft + posted quantity ≤ ordered − returned. More → 422 on `payload.lines.N.quantity`. An unknown line of the order → 422 on `payload.lines.N.order_line_id`.
- The order must be `open` (else 409).

Post review body `{ "accounting_date": "YYYY-MM-DD" }`; post payload `{ "accounting_date", "review_fingerprint" }`. In an active book the date must be today in the book's timezone, the day the stock layer dates the COGS: any other date is 422 on `accounting_date` (review) or `payload.accounting_date` (post), nothing written. Setup books only store it. Cancel payload `{ "reason" }` (1-1000 characters), drafts only.

Only drafts change (posted or cancelled → 422). A cancelled draft frees its capacity.

## Resource

`ShipmentResource`: `id`, `order_id`, `state` (`draft`, `posted`, `cancelled`), `version`, `note`, `lines[{id, order_line_id, product_id, title, quantity}]`, `accounting_date`, `recognition_id` (active books), `created_by/at`, `updated_at`, `posted_by/at`, `cancelled_at`, `cancel_reason`, `allowed_actions` (`update`, `post`, `cancel` on a draft of an open order; `cancel` only on a draft of another order state).

`ShipmentPostReview`: `shipment_id`, `version`, `accounting_date`, `lines`, `recognition` (`{net, tax, gross}` in the order currency, null in setup books), `review_fingerprint`.

## Books

Setup book: stock only. No recognition, no journal.

Active book, per post:

- Amounts in SQL per order line: `money_round_div(line_excl × (shipped_before + q), quantity) − money_round_div(line_excl × shipped_before, quantity)`, same for `line_vat`. Shipments of one line add up to its exact `line_excl` / `line_vat`; the one reaching the full quantity takes the rest.
- One immutable `sales_recognitions` row (source `shipment`) with its lines, and one source journal: DR `unbilled_receivables` (relation = order relation) gross / CR `revenue` net / CR `tax_payable` tax (no zero line). Effect `shipment/<id>/fulfilled`, once.
- COGS stays with the stock layer (DR `cogs` / CR `inventory`); the order's cost allocation keeps `related = order`. Recognition and COGS are one fulfilment and share one accounting date: today in the book timezone (clock time, as the stock layer reads it). No backdating.
- An order in a currency other than the functional one: 422 `unsupported_active_erp_operation` before any effect. Missing mapping or closed period: 422 `accounting_not_configured` / `accounting_period_closed`.
- Free goods (gross 0) recognise nothing.

## Orders and Till

- `POST /orders/{id}/complete`: a draft shipment blocks it (409). Setup book: takes what posted shipments left on the hold (all of it without shipments). Active book: creates and posts one shipment for every line's remaining quantity in the same transaction, dated today in the book timezone.
- `POST /orders/{id}/cancel`: refused (409) once a shipment posted.
- Till sale in an active book: recognition source `retail_sale` (no relation, no invoice, no customer) with the same DR/CR, effect `retail_sale/<order>/fulfilled`; then the GL receipt DR `cash` (cash sale) or `provider_clearing` (card) for the amount due / CR `unbilled_receivables`, cash rounding to `rounding`, effect `retail_sale/<order>/received`. No second cash ledger row: the sale row already holds the drawer money. A replay of the same `client_ref` writes nothing.

## Later invoices

A recognised sale is billed by a draft invoice that reserves it with one billing link (`invoice_recognition_links`, one per recognition, `draft` → `issued`).

- `POST /orders/{order}/shipments/{shipment}/invoice`: A envelope `{client_ref, expected_version (the posted shipment's), payload: {relation_id, addressed_contact_id?}}` → 201 `{shipment_id, invoice_id}`. The shipment must be posted with a recognition (active books); `relation_id` and the order currency must match the recognition (else 422 on `payload.relation_id`). A recognition already on an invoice: 409.
- `POST /orders/{id}/invoice` in an active book: one draft billing every recognised, unbilled sale of the order; none → 422.
- Items copy the recognised quantity at the order line's unit price, VAT rate and discount. A linked draft refuses any line, party or currency change (422); notes, reference and contact stay editable. Cancelling (deleting) the draft releases its links.
- A linked invoice moves no stock on send or issue: its shipments did.

Issue: `POST /invoices/{invoice}/issue/review` `{accounting_date, addressed_contact_id?}` → `IssueReview {invoice_id, accounting_date, reclassify[{recognition_id, amount}], recognition {net, tax}?, review_fingerprint}`; then `POST /invoices/{invoice}/issue` A envelope `{accounting_date, addressed_contact_id?, review_fingerprint}` → `{id, invoice_number, status}`. It numbers the draft once without email, using the stored billing address and the contact (sent, or stored). Invoices carry no version: `expected_version` is required by the envelope but the review fingerprint is what guards changes. `POST /invoices/{id}/send` runs the same issue and then queues the email in one transaction; `resend` never issues again, and a failed email never undoes the issue.

In an active book (date = today in the book timezone, as shipments):

- Linked recognitions: for each with an unpaid unbilled residual, a billed receivable item opens for exactly that residual, then a `reclassification` allocation moves it from the unbilled item; one journal DR `receivables` / CR `unbilled_receivables` per recognition pair (customer on both lines), effect `invoice/<id>/issued`. A fully settled recognition bills as a document only (no journal).
- No link (standalone invoice): recognition source `invoice` (header only, no lines yet), journal DR `receivables` / CR `revenue` / CR `tax_payable` from the invoice's stored subtotal and total (tax = total − subtotal, in SQL), a billed receivable item. Stock and COGS as before.
- Credit notes: 422 `unsupported_active_erp_operation`.

Every recognition opens its receivable open item (`accounting_open_items`, role `unbilled_receivables` for shipments and till sales, `receivables` for standalone invoices) and links its source (`accounting_source_links`, `posted`). A till sale's receipt opens a customer credit for the gross and applies it at once, so the retail item closes to 0.

## Not yet

Customer deposits come in the next step; credits and returns of shipped goods later.
